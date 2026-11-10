//! Native print selection/layout consumers and a portable page renderer.
//! No printer, device, window, registry or drawing API is called.

use std::io::{self, Error, ErrorKind};
use std::ops::Range;

pub mod native;

fn invalid(reason: &'static str) -> Error {
    Error::new(ErrorKind::InvalidInput, reason)
}

/// 43328: fixed cells repeat, a boundary is BEFORE its indexed body cell.
/// Blocked boundaries come from fixed-row bit80/fixed-column node38, not pages.
/// The last remainder wins; a zero candidate becomes one. Output is BE16 spans.
pub fn axis_spans(
    sizes: &[i32],
    fixed: usize,
    blocked: &[bool],
    available: i32,
) -> io::Result<Vec<u8>> {
    if fixed > sizes.len()
        || blocked.len() != sizes.len()
        || sizes.len() > usize::from(u16::MAX)
        || sizes.iter().any(|&n| n < 0)
        || available <= 0
    {
        return Err(invalid("invalid print axis dimensions"));
    }
    let fixed_size = sizes[..fixed].iter().try_fold(0_i32, |sum, n| {
        sum.checked_add(*n)
            .ok_or_else(|| invalid("print fixed-axis sum overflow"))
    })?;
    let mut spans = Vec::new();
    let mut start = fixed;
    while start < sizes.len() {
        let mut extent = fixed_size;
        let mut candidate = 0;
        let remaining = sizes.len() - start;
        let mut index = 0;
        while index < remaining {
            extent = extent
                .checked_add(sizes[start + index])
                .ok_or_else(|| invalid("print axis sum overflow"))?;
            if !blocked[start + index] {
                candidate = index;
            }
            if candidate > 0 && extent > available {
                break;
            }
            index += 1;
        }
        if index == remaining {
            candidate = index;
        }
        let count = candidate.max(1);
        spans.extend_from_slice(&(count as u16).to_be_bytes());
        start += count;
    }
    Ok(spans)
}

/// Portable monospace metrics, not substituted GetTextExtentPoint32A results.
/// Integer pixel cells enclose the declared glyph/line advances; SVG textLength
/// uses the same advance. Only font-height conversion uses180BF8 truncation.
#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub font_height: i32,
    pub em: i32,
    pub advance_em: i32,
    pub line_em: i32,
    pub padding: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct Paper {
    pub width: i32,
    pub height: i32,
}

/// Text comes from actual typed views. Boundary flags are layout source data,
/// never precomputed spans/page rectangles. Every fixed cell repeats per page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub name: String,
    pub cells: Vec<Vec<String>>,
    pub fixed_rows: usize,
    pub fixed_columns: usize,
    pub column_boundaries: Vec<bool>,
    pub row_boundaries: Vec<bool>,
    pub nodes: Option<Vec<Vec<native::CellNode>>>,
    pub factory: Option<native::Factory>,
    pub show_split: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub columns: Range<usize>,
    pub rows: Range<usize>,
    pub font_height: i32,
    pub attempts: u8,
    pub content_width: i32,
    pub content_height: i32,
    pub fits: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub horizontal: Vec<u8>,
    pub vertical: Vec<u8>,
    pub pages: Vec<Page>,
}

fn pixels(units: i32, height: i32, em: i32) -> io::Result<i32> {
    let product = i64::from(units) * i64::from(height);
    i32::try_from((product + i64::from(em) - 1) / i64::from(em))
        .map_err(|_| invalid("portable font metric overflow"))
}

impl Metrics {
    pub fn validate(self) -> io::Result<()> {
        self.advances(self.font_height).map(|_| ())
    }
    fn advances(self, font: i32) -> io::Result<(i32, i32)> {
        if self.em <= 0 || self.advance_em <= 0 || self.line_em <= 0 || self.padding < 0 {
            return Err(invalid("invalid portable monospace measurement source"));
        }
        let height = font
            .checked_abs()
            .filter(|n| *n > 0)
            .ok_or_else(|| invalid("portable zero/overflow font has no measurement source"))?;
        Ok((
            pixels(self.advance_em, height, self.em)?,
            pixels(self.line_em, height, self.em)?,
        ))
    }
}

impl Grid {
    fn cell_span(&self, column: usize, row: usize) -> io::Result<(Range<usize>, Range<usize>)> {
        let Some(nodes) = &self.nodes else {
            return Ok((column..column + 1, row..row + 1));
        };
        let mut column = column;
        while nodes[row][column].flags1c & 0x80 != 0 {
            column = column
                .checked_sub(1)
                .ok_or_else(|| invalid("native merged cell has no left owner"))?;
        }
        let node = &nodes[row][column];
        let top = row
            .checked_sub(if node.merge { node.above } else { 0 })
            .ok_or_else(|| invalid("native vertical merge outside grid"))?;
        let bottom = row
            .checked_add(if node.merge { node.below } else { 0 })
            .and_then(|v| v.checked_add(1))
            .filter(|v| *v <= nodes.len())
            .ok_or_else(|| invalid("native vertical merge outside grid"))?;
        let mut end = column + 1;
        while end < nodes[row].len() && nodes[row][end].flags1c & 0x80 != 0 {
            end += 1;
        }
        Ok((column..end, top..bottom))
    }

    fn measure(&self, font: i32, metrics: Metrics) -> io::Result<(Vec<i32>, Vec<i32>)> {
        let columns = self.column_boundaries.len();
        if self.cells.len() != self.row_boundaries.len()
            || self.fixed_rows > self.cells.len()
            || self.fixed_columns > columns
            || self.cells.iter().any(|row| row.len() != columns)
        {
            return Err(invalid("inconsistent print grid source"));
        }
        if self.nodes.as_ref().is_some_and(|nodes| {
            nodes.len() != self.cells.len() || nodes.iter().any(|r| r.len() != columns)
        }) {
            return Err(invalid("native node dimensions do not match print grid"));
        }
        let (advance, line) = metrics.advances(font)?;
        let padding = metrics
            .padding
            .checked_mul(2)
            .ok_or_else(|| invalid("print padding overflow"))?;
        let mut widths = vec![padding; columns];
        let mut heights = vec![padding; self.cells.len()];
        for (r, row) in self.cells.iter().enumerate() {
            for (c, text) in row.iter().enumerate() {
                let (cs, rs) = self.cell_span(c, r)?;
                if cs.start != c || rs.start != r {
                    continue;
                }
                if !text.is_ascii() || text.bytes().any(|b| b < 32 && b != b'\n') {
                    return Err(invalid(
                        "portable print needs measured ASCII text or explicit byte notation",
                    ));
                }
                let hint = self.nodes.as_ref().map(|nodes| nodes[r][c].literal.as_slice());
                let (auto_width, auto_height) = self.nodes.as_ref().map_or((true, true), |nodes| {
                    (nodes[r][c].flags1c & 0x10 != 0, nodes[r][c].flags1c & 0x20 != 0)
                });
                let mut needed_width = padding;
                let mut needed_height = padding;
                // Native W/H size flags use ANSI template byte advances, not
                // live paint replies or the diagnostic byte-escape projection.
                for value in std::iter::once(hint.unwrap_or(text.as_bytes())) {
                    let mut lines = 0_i32;
                    for part in value.split(|b| *b == b'\n') {
                        lines += 1;
                        if auto_width {
                          let width = i32::try_from(part.len())
                            .ok()
                            .and_then(|n| n.checked_mul(advance))
                            .and_then(|n| n.checked_add(padding))
                            .ok_or_else(|| invalid("print text width overflow"))?;
                        needed_width = needed_width.max(width);
                        }
                    }
                    if auto_height {
                      let height = lines
                        .checked_mul(line)
                        .and_then(|n| n.checked_add(padding))
                        .ok_or_else(|| invalid("print text height overflow"))?;
                    needed_height = needed_height.max(height);
                    }
                }
                let width = selected_extent(&widths, 0, cs.clone())?;
                let height = selected_extent(&heights, 0, rs.clone())?;
                if needed_width > width {
                    widths[cs.end - 1] += needed_width - width;
                }
                if needed_height > height {
                    heights[rs.end - 1] += needed_height - height;
                }
            }
        }
        Ok((widths, heights))
    }
}

fn selected_extent(sizes: &[i32], fixed: usize, selected: Range<usize>) -> io::Result<i32> {
    (0..fixed).chain(selected).try_fold(0_i32, |sum, i| {
        sum.checked_add(sizes[i])
            .ok_or_else(|| invalid("print page extent overflow"))
    })
}

/// Initial spans are generated once. Each page remeasures at most four times;
/// a shrink destroys/recreates the printer view in the native caller. The font
/// value survives across pages, and180BF8 truncates abs(font)*min(axis ratios).
pub fn layout(grid: &Grid, paper: Paper, metrics: Metrics) -> io::Result<Layout> {
    let (widths, heights) = grid.measure(metrics.font_height, metrics)?;
    let horizontal = axis_spans(
        &widths,
        grid.fixed_columns,
        &grid.column_boundaries,
        paper.width,
    )?;
    let vertical = axis_spans(
        &heights,
        grid.fixed_rows,
        &grid.row_boundaries,
        paper.height,
    )?;
    let mut pages = Vec::new();
    let mut font = metrics.font_height;
    let mut row = grid.fixed_rows;
    for pair in vertical.as_chunks::<2>().0 {
        let rows = row..row + usize::from(u16::from_be_bytes(*pair));
        let mut column = grid.fixed_columns;
        for pair in horizontal.as_chunks::<2>().0 {
            let columns = column..column + usize::from(u16::from_be_bytes(*pair));
            for attempt in 1..=4 {
                let (widths, heights) = grid.measure(font, metrics)?;
                let width = selected_extent(&widths, grid.fixed_columns, columns.clone())?.max(1);
                let height = selected_extent(&heights, grid.fixed_rows, rows.clone())?.max(1);
                let fits = width <= paper.width && height <= paper.height;
                if fits || attempt == 4 {
                    pages.push(Page {
                        columns: columns.clone(),
                        rows: rows.clone(),
                        font_height: font,
                        attempts: attempt,
                        content_width: width,
                        content_height: height,
                        fits,
                    });
                    break;
                }
                let ratio = (f64::from(paper.width) / f64::from(width))
                    .min(f64::from(paper.height) / f64::from(height));
                font = (f64::from(
                    font.checked_abs()
                        .ok_or_else(|| invalid("print font overflow"))?,
                ) * ratio)
                    .trunc() as i32;
            }
            column = columns.end;
        }
        row = rows.end;
    }
    Ok(Layout {
        horizontal,
        vertical,
        pages,
    })
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// A real portable SVG page: fixed/selected cells are measured and drawn, not
/// requests or a dump of a caller-supplied page packet. No native printer claim.
pub fn html_page(
    grid: &Grid,
    page: &Page,
    paper: Paper,
    metrics: Metrics,
    number: usize,
) -> io::Result<String> {
    use std::fmt::Write;
    let (widths, heights) = grid.measure(page.font_height, metrics)?;
    if page.columns.start < grid.fixed_columns
        || page.columns.end > widths.len()
        || page.rows.start < grid.fixed_rows
        || page.rows.end > heights.len()
        || page.columns.is_empty()
        || page.rows.is_empty()
        || paper.width <= 0
        || paper.height <= 0
    {
        return Err(invalid("page does not belong to the print grid"));
    }
    selected_extent(&widths, grid.fixed_columns, page.columns.clone())?;
    selected_extent(&heights, grid.fixed_rows, page.rows.clone())?;
    let (advance, line_height) = metrics.advances(page.font_height)?;
    let mut html = format!(
        "<section class=\"page\" data-page=\"{number}\"><h2>{}</h2><svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><defs><clipPath id=\"page-{number}\"><rect width=\"{}\" height=\"{}\"/></clipPath></defs><g clip-path=\"url(#page-{number})\">",
        escape(&grid.name),
        paper.width,
        paper.height,
        paper.width,
        paper.height,
        paper.width,
        paper.height
    );
    let mut y = 0;
    let mut drawn = std::collections::BTreeSet::new();
    for r in (0..grid.fixed_rows).chain(page.rows.clone()) {
        let mut x = 0;
        for c in (0..grid.fixed_columns).chain(page.columns.clone()) {
            let (cs, rs) = grid.cell_span(c, r)?;
            let width = selected_extent(&widths, 0, cs.clone())?;
            let height = selected_extent(&heights, 0, rs.clone())?;
            if width > 0 && height > 0 && drawn.insert((cs.start, rs.start)) {
                // 3490C/5EB38 retain the full owner rectangle even when the
                // forced span begins on a continuation. The page clips it.
                let x = x - selected_extent(&widths, 0, cs.start..c)?;
                let y = y - selected_extent(&heights, 0, rs.start..r)?;
                let node = grid.nodes.as_ref().map(|n| &n[rs.start][cs.start]);
                if let Some(node) = node {
                    write!(html, "<defs><clipPath id=\"cell-{number}-{}-{}\"><rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\"/></clipPath></defs><g clip-path=\"url(#cell-{number}-{}-{})\">", rs.start, cs.start, rs.start, cs.start).unwrap();
                    write!(html, "<rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" fill=\"white\" data-node-1c=\"{}\" data-node-1d=\"{}\"/>", node.flags1c, node.flags1d).unwrap();
                    if node.flags1d & 1 != 0 {
                        write!(
                            html,
                            "<path d=\"M{x} {}h{width}\" stroke=\"black\"/>",
                            y + height - 1
                        )
                        .unwrap();
                    }
                    if node.flags1d & 6 != 0 && cs.end < widths.len() {
                        let top = if node.flags1d & 6 == 2 { y } else { y + (i64::from(height) * 7 / 10) as i32 };
                        write!(
                            html,
                            "<path d=\"M{} {top}v{}\" stroke=\"black\"/>",
                            x + width - 1, y + height - top
                        )
                        .unwrap();
                    }
                } else {
                    write!(html, "<rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" fill=\"white\" stroke=\"black\"/>").unwrap();
                }
                if let Some(checked) = node.and_then(|node| node.checked) {
                    let side = width.min(height).min(line_height);
                    let cx = i64::from(x) + (i64::from(width) - i64::from(side)) / 2;
                    let cy = i64::from(y) + (i64::from(height) - i64::from(side)) / 2;
                    write!(html, "<svg x=\"{cx}\" y=\"{cy}\" width=\"{side}\" height=\"{side}\" viewBox=\"0 0 16 16\" role=\"checkbox\" aria-checked=\"{checked}\" aria-label=\"Cell checkbox\" fill=\"none\" stroke=\"black\"><path d=\"M1 1H15V15H1Z\"/>").unwrap();
                    if checked {
                        html.push_str("<path d=\"M4 8l3 3 5-6\"/>");
                    }
                    html.push_str("</svg>");
                }
                let text = &grid.cells[rs.start][cs.start];
                let block_height = i32::try_from(text.split('\n').count())
                    .ok()
                    .and_then(|n| n.checked_mul(line_height))
                    .ok_or_else(|| invalid("merged text height overflow"))?;
                let ty = match node.map(|n| n.flags1c & 12) {
                    Some(4) => i64::from(y) + (i64::from(height) - i64::from(block_height)) / 2,
                    Some(8) => i64::from(y) + i64::from(height) - i64::from(metrics.padding) - i64::from(block_height),
                    _ => i64::from(y) + i64::from(metrics.padding),
                };
                for (i, text) in text.split('\n').enumerate() {
                    if !text.is_empty() {
                        let length = i32::try_from(text.len()).ok().and_then(|n| n.checked_mul(advance)).ok_or_else(|| invalid("print text advance overflow"))?;
                        let tx = match node.map(|n| n.flags1c & 3) {
                            Some(1) => x + (width - length) / 2,
                            Some(2) => x + width - metrics.padding - length,
                            _ => x + metrics.padding,
                        };
                        write!(html, "<text x=\"{tx}\" y=\"{}\" font-family=\"monospace\" font-size=\"{}\" textLength=\"{length}\" lengthAdjust=\"spacingAndGlyphs\">{}</text>", ty + (i as i64 + 1) * i64::from(line_height), page.font_height.abs(), escape(text)).unwrap();
                    }
                }
                if node.is_some() { html.push_str("</g>"); }
            }
            x += widths[c];
        }
        y += heights[r];
    }
    html.push_str("</g></svg></section>\n");
    Ok(html)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredicateOrder {
    BaseFirst,
    DatasetFirst,
}

/// 59F84 differs from5BBE8/5BF0C/5C1F8/5C5D0. Do not short circuit an
/// A5 dataset source/failure merely because BC.bit0 already disables the view.
pub fn eligible(
    bc: u8,
    order: PredicateOrder,
    dataset: impl FnOnce() -> io::Result<bool>,
) -> io::Result<bool> {
    let base = bc & 1 == 0;
    if order == PredicateOrder::BaseFirst && !base {
        return Ok(false);
    }
    let data = dataset()?;
    Ok(base && data)
}

/// Concrete printer factories used by the existing typed sheet producers.
/// The scalar layout attributes below are fixed resource schema, not page data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    Memory,
    Call,
    Bandstack,
    Memopad,
    Scan,
    Gps,
    Repeater,
    Ur,
    Allowed,
    Rx,
    FromDv,
    FromFm,
    To,
    UserEdge,
    Preset,
    RepeaterTop,
}

#[derive(Debug, Clone)]
pub struct ViewState {
    pub kind: ViewKind,
    pub node: Option<u32>,
    pub group: i32,
    pub bc: u8,
    pub fixed_columns: usize,
    pub fixed_rows: usize,
    pub current_row: i32,
    pub show_split: u32,
    pub active: bool,
    reply_rows: Option<u32>,
}

impl ViewState {
    /// Resource Style/CallSheet bindings at31D281/31DAD1/31E324/327DA0,
    /// 32AB56/32DE81/32F646/32FA1C/330702/33185F/331B7E/332128/
    /// 341AA5/351251. Default BC.bit0 follows NoPrint; other live bits survive
    /// later state changes rather than being canonicalized by the predicate.
    pub fn new(kind: ViewKind, group: i32) -> io::Result<Self> {
        use ViewKind::*;
        let (node, columns, rows, no_print) = match kind {
            Memory if (0..100).contains(&group) => (2 + group as u32, 3, 5, false),
            Call => (103, 3, 5, false),
            Bandstack => (401, 6, 5, false),
            Memopad => (402, 6, 5, false),
            Scan => (104, 3, 3, false),
            Gps if (0..27).contains(&group) => (252 + group as u32, 3, 3, false),
            Repeater if (0..50).contains(&group) => (123 + group as u32, 3, 5, false),
            Ur => (120, 3, 3, false),
            Allowed => (222, 3, 3, false),
            Rx => (216, 3, 7, false),
            FromDv => (210, 3, 3, false),
            FromFm => (211, 3, 3, false),
            To => (213, 3, 3, false),
            UserEdge => (404, 3, 3, false),
            Preset if (0..5).contains(&group) => (422 + group as u32, 3, 1, false),
            RepeaterTop => (0, 1, 1, true),
            _ => return Err(invalid("print factory group outside its native axis")),
        };
        Ok(Self {
            kind,
            node: (!no_print).then_some(node),
            group,
            bc: u8::from(no_print),
            fixed_columns: columns,
            fixed_rows: rows,
            current_row: rows as i32,
            show_split: 0,
            active: true,
            reply_rows: None,
        })
    }
    /// The split factory's609BB/60A46 cache is generated from current source.
    /// A bare resource schema has no invented group-query second scalar.
    pub fn from_image(current: &[u8], kind: ViewKind, group: i32) -> io::Result<Self> {
        Self::from_image_with_split(current, kind, group, 0)
    }
    pub fn from_image_with_split(
        current: &[u8],
        kind: ViewKind,
        group: i32,
        show_split: u32,
    ) -> io::Result<Self> {
        let mut view = Self::new(kind, group)?;
        view.show_split = show_split;
        if view.group_reply() {
            let rows = if kind == ViewKind::Memory { 200 } else { 8 };
            view.reply_rows = Some(split_index(current, &view, rows, false)? as u32);
        }
        Ok(view)
    }
    pub fn record_type(&self) -> Option<u32> {
        use ViewKind::*;
        Some(match self.kind {
            Memory => 2,
            Call => 3,
            Bandstack => 4,
            Memopad => 5,
            Scan => 8,
            Gps => 0x0a,
            Repeater => 0x0c,
            Ur => 0x0d,
            Allowed => 0x17,
            Rx => 0x0e,
            FromDv => 0x0f,
            FromFm => 0x10,
            To => 0x12,
            UserEdge => 0x15,
            Preset | RepeaterTop => return None,
        })
    }
    fn group_reply(&self) -> bool {
        matches!(self.kind, ViewKind::Memory | ViewKind::Call)
    }
    fn dataset_first(&self) -> bool {
        matches!(
            self.kind,
            ViewKind::Repeater | ViewKind::Ur | ViewKind::Allowed | ViewKind::Gps
        )
    }
    fn base_only(&self) -> bool {
        matches!(
            self.kind,
            ViewKind::Rx
                | ViewKind::FromDv
                | ViewKind::FromFm
                | ViewKind::To
                | ViewKind::UserEdge
                | ViewKind::Preset
                | ViewKind::RepeaterTop
        )
    }
}

fn data_error(error: crate::icf::Error) -> Error {
    Error::new(ErrorKind::InvalidData, error)
}

/// B5D28/B5DE0's actual row conversion, not a caller-supplied reply scalar.
pub fn split_index(current: &[u8], view: &ViewState, index: i32, expand: bool) -> io::Result<i32> {
    let Some(kind @ (2 | 3)) = view.record_type() else {
        return Ok(index);
    };
    if view.show_split != 0 {
        return Ok(index);
    }
    let limit = if kind == 2 { 100 } else { 4 };
    let mut result = index;
    let mut cursor = 0;
    for row in 0..limit {
        if cursor >= index {
            break;
        }
        let split = crate::icf::record_image::print_channel_row(current, kind, view.group, row)
            .map_err(data_error)?
            == Some(true);
        if split {
            cursor += 1;
        } else {
            result = if expand {
                result.wrapping_add(1)
            } else {
                result.wrapping_sub(1)
            };
        }
        cursor += 1;
        if !expand && !split {
            cursor += 1;
        }
    }
    Ok(result)
}

/// Only the live3F/42 producers needed by print. No generic GUI bus/interface.
/// Registration appends a new active receiver; traversal is reverse live order.
#[derive(Debug, Default)]
pub struct Views {
    pub states: Vec<ViewState>,
    answers: Vec<[u32; 2]>,
}
impl Views {
    pub fn register(&mut self, view: ViewState) -> usize {
        self.states.push(view);
        self.states.len() - 1
    }
    pub fn deactivate(&mut self, id: usize) {
        self.states[id].active = false;
    }
    pub fn query(&mut self, current: &[u8], event: u32, argument: u32) -> io::Result<&[[u32; 2]]> {
        // Previous answers are invalidated BEFORE any new receiver/source read.
        self.answers.clear();
        for view in self.states.iter().rev().filter(|v| v.active) {
            if event == 0x3f && view.group_reply() {
                self.answers.push([
                    view.group as u32,
                    view.reply_rows.ok_or_else(|| {
                        invalid("group query requires an initialized native view cache")
                    })?,
                ]);
            } else if event == 0x42 && matches!(view.kind, ViewKind::Memory | ViewKind::Call) {
                let row = view.current_row.wrapping_sub(view.fixed_rows as i32);
                self.answers
                    .push([split_index(current, view, row, argument == 1)? as u32, 0]);
            }
        }
        Ok(&self.answers)
    }
    pub fn nonempty(&mut self, current: &[u8], id: usize) -> io::Result<bool> {
        let view = self
            .states
            .get(id)
            .ok_or_else(|| invalid("unknown print view"))?
            .clone();
        // St_Preset VMT45B3D4: D68CC returns1 without reading BC or data.
        if view.kind == ViewKind::Preset {
            return Ok(true);
        }
        if view.base_only() {
            return Ok(view.bc & 1 == 0);
        }
        let order = if view.dataset_first() {
            PredicateOrder::DatasetFirst
        } else {
            PredicateOrder::BaseFirst
        };
        eligible(view.bc, order, || {
            let group = if view.group_reply() {
                // 612DF/63488 copies head00 before the next query. NULL keeps0.
                self.query(current, 0x3f, 0)?
                    .first()
                    .map_or(0, |answer| answer[0]) as i32
            } else {
                view.group
            };
            Ok(crate::icf::record_image::print_dataset_count(
                current,
                view.record_type().unwrap(),
                group,
            )
            .map_err(data_error)?
                != 0)
        })
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Editor {
    pub validation_guard: u32,
    pub visible: bool,
    pub text: Vec<u8>,
}

/// 36710/event1B clears104 BEFORE dispatch. The child calls the existing field
/// root, not a supplied success result. Hide follows normal dispatch only. This
/// envelope does not establish native cell/choice inline reachability by itself.
/// A headless-created view has no visible editor to dispatch.
pub fn sync_editor<E>(
    editor: &mut Editor,
    dispatch_phase1: impl FnOnce(&[u8]) -> Result<(), E>,
) -> Result<(), E> {
    editor.validation_guard = 0;
    if editor.visible {
        dispatch_phase1(&editor.text)?;
        editor.visible = false;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    BeginDoc,
    Tick { due_ms: u64, name: String },
    NewPage,
    Page,
    EndDoc,
    ClearNames,
}

/// All-print's finite task consumer. The renderer is invoked even after Cancel
/// so its normal factory/predicate prefix is not silently removed. No finally
/// adds EndDoc after an exception. Backend output must be produced by the child.
#[derive(Debug, Clone)]
pub struct Job {
    names: Vec<String>,
    cursor: usize,
    pub cancelled: bool,
    pub pages: usize,
    pub ended: bool,
    pub events: Vec<Event>,
}

impl Job {
    /// Call only after source preparation/preview approval, never preview0.
    pub fn begin(names: Vec<String>) -> io::Result<Self> {
        if names.is_empty() {
            return Err(invalid("no eligible print sheet"));
        }
        Ok(Self {
            names,
            cursor: 0,
            cancelled: false,
            pages: 0,
            ended: false,
            events: vec![Event::BeginDoc],
        })
    }
    /// One50ms task callback per sheet, including the Cancel drain. Current
    /// print calls the renderer directly; it does not fabricate a timer delivery.
    pub fn tick(
        &mut self,
        render: impl FnOnce(&str, &mut Self) -> io::Result<()>,
    ) -> io::Result<bool> {
        if self.ended {
            return Ok(false);
        }
        let name = self.names[self.cursor].clone();
        let due_ms = (self.cursor as u64 + 1) * 50;
        self.events.push(Event::Tick {
            due_ms,
            name: name.clone(),
        });
        render(&name, self)?;
        self.cursor += 1;
        if self.cursor == self.names.len() {
            self.finish();
        }
        Ok(!self.ended)
    }
    /// NewPage precedes page2+ drawing, including a later drawing failure.
    /// Count/layout-only measurement passes never call this method.
    pub fn begin_page(&mut self) {
        if self.pages != 0 {
            self.events.push(Event::NewPage);
        }
    }
    /// Invoke only after the real backend emitted a complete page.
    pub fn page_drawn(&mut self) {
        self.events.push(Event::Page);
        self.pages += 1;
    }
    /// Normal current renderer return (including Cancel) reaches this. Error
    /// propagation deliberately leaves an un-ended job and already drawn prefix.
    pub fn finish(&mut self) {
        self.events.push(Event::EndDoc);
        self.ended = true;
        self.names.clear();
        self.events.push(Event::ClearNames);
    }
    pub fn remaining_names(&self) -> &[String] {
        &self.names[self.cursor.min(self.names.len())..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics() -> Metrics {
        Metrics {
            font_height: -12,
            em: 12,
            advance_em: 6,
            line_em: 12,
            padding: 1,
        }
    }
    fn grid(rows: &[&[&str]]) -> Grid {
        Grid {
            name: "typed view".into(),
            cells: rows
                .iter()
                .map(|r| r.iter().map(|s| s.to_string()).collect())
                .collect(),
            fixed_rows: 1,
            fixed_columns: 1,
            column_boundaries: vec![false; rows[0].len()],
            row_boundaries: vec![false; rows.len()],
            nodes: None,
            factory: None,
            show_split: 0,
        }
    }

    #[test]
    fn spans_keep_fixed_axes_boundaries_tail_and_zero_to_one() {
        assert_eq!(
            axis_spans(&[10, 10, 10, 10], 1, &[false; 4], 35).unwrap(),
            [0, 2, 0, 1]
        );
        assert_eq!(
            axis_spans(&[10, 10, 10, 10], 1, &[false, false, false, true], 35).unwrap(),
            [0, 1, 0, 2]
        );
        assert_eq!(
            axis_spans(&[10, 100, 100], 1, &[false; 3], 5).unwrap(),
            [0, 1, 0, 1]
        );
        assert_eq!(
            axis_spans(&[10, 100, 100], 1, &[true; 3], 5).unwrap(),
            [0, 2]
        );
        assert_eq!(axis_spans(&[10], 1, &[false], 20).unwrap(), []);
        assert_eq!(axis_spans(&[], 0, &[], 20).unwrap(), []);
        assert!(axis_spans(&[10], 2, &[false], 20).is_err());
        assert!(axis_spans(&[10], 0, &[], 20).is_err());
        assert_eq!(
            axis_spans(&[1; 300], 0, &[false; 300], 500).unwrap(),
            [1, 44]
        );
    }

    #[test]
    fn page_spans_are_initial_but_four_pass_font_state_survives_pages() {
        let g = grid(&[
            &["#", "A", "B"],
            &["0", "abcdefghijklmnop", "x"],
            &["1", "q", "q"],
            &["2", "q", "q"],
        ]);
        let l = layout(
            &g,
            Paper {
                width: 80,
                height: 42,
            },
            metrics(),
        )
        .unwrap();
        assert_eq!(l.horizontal, [0, 1, 0, 1]);
        assert_eq!(l.vertical, [0, 2, 0, 1]);
        assert_eq!(l.pages.len(), 4);
        assert_eq!((&l.pages[0].columns, &l.pages[0].rows), (&(1..2), &(1..3)));
        assert_eq!((l.pages[0].font_height, l.pages[0].attempts), (8, 3));
        assert_eq!(
            (l.pages[0].content_width, l.pages[0].content_height),
            (72, 30)
        );
        assert_eq!((&l.pages[1].columns, &l.pages[1].rows), (&(2..3), &(1..3)));
        assert_eq!((&l.pages[2].columns, &l.pages[2].rows), (&(1..2), &(3..4)));
        assert!(
            l.pages
                .iter()
                .skip(1)
                .all(|p| p.font_height == 8 && p.attempts == 1)
        );
        let mut m = metrics();
        m.padding = 100;
        let g = grid(&[&["", ""], &["", ""]]);
        let l = layout(
            &g,
            Paper {
                width: 220,
                height: 220,
            },
            m,
        )
        .unwrap();
        assert_eq!((l.pages[0].attempts, l.pages[0].font_height), (4, 1));
        assert_eq!(
            (l.pages[0].content_width, l.pages[0].content_height),
            (400, 402)
        );
        assert!(!l.pages[0].fits);
    }

    #[test]
    fn portable_backend_draws_only_fixed_and_selected_cells_and_escapes_text() {
        let g = grid(&[&["#", "NAME"], &["0", "<A&"], &["1", "not-this-page"]]);
        let paper = Paper {
            width: 200,
            height: 28,
        };
        let l = layout(&g, paper, metrics()).unwrap();
        let html = html_page(&g, &l.pages[0], paper, metrics(), 1).unwrap();
        assert!(html.contains("&lt;A&amp;") && html.contains("NAME"));
        assert!(!html.contains("not-this-page"));
        assert_eq!(html.matches("<text ").count(), 4);
        assert!(html.contains("textLength=\"18\""));
        let mut invalid_page = l.pages[0].clone();
        invalid_page.rows = 1..99;
        assert!(html_page(&g, &invalid_page, paper, metrics(), 1).is_err());
        let mut bad = g;
        bad.cells[1][1] = "\u{00e9}".into();
        assert!(layout(&bad, paper, metrics()).is_err());
    }

    #[test]
    fn portable_checkbox_keeps_state_text_and_merged_cell_clipping() {
        let mut g = grid(&[&["", "not the owner"]]);
        g.fixed_rows = 0;
        g.fixed_columns = 0;
        g.nodes = Some(vec![vec![
            native::CellNode {
                flags1c: 0x30,
                literal: b"AA".to_vec(),
                ..Default::default()
            },
            native::CellNode {
                flags1c: 0x80,
                checked: Some(false),
                ..Default::default()
            },
        ]]);
        let plan = layout(
            &g,
            Paper {
                width: 100,
                height: 100,
            },
            metrics(),
        )
        .unwrap();
        let mut page = plan.pages[0].clone();
        page.columns = 1..2;
        let paper = Paper {
            width: 12,
            height: 14,
        };
        for (checked, text) in [(Some(true), ""), (Some(false), "<&\"'>"), (None, "--")] {
            g.nodes.as_mut().unwrap()[0][0].checked = checked;
            g.cells[0][0] = text.into();
            assert_eq!(g.measure(-12, metrics()).unwrap(), (vec![2, 12], vec![14]));
            let html = html_page(&g, &page, paper, metrics(), 7).unwrap();
            assert!(html.contains("clip-path=\"url(#page-7)\""));
            let cell = html
                .split_once("<g clip-path=\"url(#cell-7-0-0)\">")
                .unwrap()
                .1
                .split("</g>")
                .next()
                .unwrap();
            assert!(html.contains("<rect x=\"-2\" y=\"0\" width=\"14\" height=\"14\""));
            assert_eq!(
                html.matches("role=\"checkbox\"").count(),
                usize::from(checked.is_some())
            );
            assert_eq!(
                cell.matches("d=\"M1 1H15V15H1Z\"").count(),
                usize::from(checked.is_some())
            );
            assert_eq!(
                cell.matches("d=\"M4 8l3 3 5-6\"").count(),
                usize::from(checked == Some(true))
            );
            if let Some(checked) = checked {
                assert!(html.contains(&format!("aria-checked=\"{checked}\"")));
                assert!(html.contains("<svg x=\"-1\" y=\"1\" width=\"12\" height=\"12\""));
            }
            if checked == Some(true) {
                assert!(!html.contains("<text "));
            } else if checked == Some(false) {
                assert!(html.contains("&lt;&amp;&quot;&#39;&gt;"));
                assert!(!html.contains("<&\"'>"));
            } else {
                assert!(html.contains(">--</text>"));
            }
            assert!(!html.contains("not the owner"));
        }
        let owner = &mut g.nodes.as_mut().unwrap()[0][0];
        owner.flags1c = 0;
        owner.checked = Some(true);
        let mut zero = metrics();
        zero.padding = 0;
        assert!(
            !html_page(&g, &page, paper, zero, 7)
                .unwrap()
                .contains("role=\"checkbox\"")
        );
        zero.padding = -1;
        assert!(html_page(&g, &page, paper, zero, 7).is_err());
    }

    #[test]
    fn predicates_query_copy_and_sync_failure_keep_real_prefix_order() {
        assert!(
            !eligible(3, PredicateOrder::BaseFirst, || Err(invalid(
                "must not be called"
            )))
            .unwrap()
        );
        assert!(
            eligible(3, PredicateOrder::DatasetFirst, || Err(invalid(
                "dataset called first"
            )))
            .is_err()
        );
        assert!(eligible(2, PredicateOrder::DatasetFirst, || Ok(true)).unwrap());
        let mut source = vec![0; 0x4997a];
        for entry in source[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
            entry.copy_from_slice(&0xffff_u32.to_le_bytes());
        }
        let mut views = Views::default();
        views.register(ViewState::from_image(&source, ViewKind::Memory, 0).unwrap());
        let second = views.register(ViewState::from_image(&source, ViewKind::Memory, 9).unwrap());
        let saved = views.query(&[], 0x3f, 0).unwrap()[0];
        assert_eq!(views.query(&[], 0x3f, 0).unwrap(), [[9, 100], [0, 100]]);
        views.deactivate(second);
        assert_eq!(views.query(&[], 0x3f, 0).unwrap(), [[0, 100]]);
        assert_eq!(saved, [9, 100]);
        assert!(views.query(&[], 0, 0).unwrap().is_empty());
        let mut failed = Views::default();
        failed.register(ViewState::new(ViewKind::Memory, 0).unwrap());
        failed.register(ViewState::from_image(&source, ViewKind::Memory, 9).unwrap());
        assert!(failed.query(&[], 0x3f, 0).is_err());
        assert_eq!(failed.answers, [[9, 100]]);
        assert!(failed.query(&[], 0, 0).unwrap().is_empty());
        let mut editor = Editor {
            validation_guard: 7,
            visible: true,
            text: b"AB".to_vec(),
        };
        let mut destination = [0_u8; 1];
        let result = sync_editor(&mut editor, |text| {
            use std::io::Write;
            io::Cursor::new(&mut destination[..]).write_all(text)
        });
        assert!(result.is_err());
        assert_eq!(destination, [b'A']);
        assert_eq!(editor.validation_guard, 0);
        assert!(editor.visible);
        sync_editor(&mut editor, |text| {
            assert_eq!(text, b"AB");
            Ok::<_, Error>(())
        })
        .unwrap();
        assert!(!editor.visible);
        sync_editor(&mut editor, |_| {
            Err(invalid("hidden editor must not dispatch twice"))
        })
        .unwrap();
    }

    #[test]
    fn cancel_drains_sheet_names_and_normal_end_differs_from_exception() {
        let mut job = Job::begin(vec!["A".into(), "B".into(), "C".into()]).unwrap();
        let mut visits = Vec::new();
        while job
            .tick(|name, job| {
                visits.push((name.to_string(), job.cancelled));
                if !job.cancelled {
                    job.page_drawn();
                    job.cancelled = true;
                }
                Ok(())
            })
            .unwrap()
        {}
        assert_eq!(
            visits,
            [("A".into(), false), ("B".into(), true), ("C".into(), true)]
        );
        assert_eq!(job.pages, 1);
        assert!(job.ended && job.remaining_names().is_empty());
        assert_eq!(
            job.events,
            [
                Event::BeginDoc,
                Event::Tick {
                    due_ms: 50,
                    name: "A".into()
                },
                Event::Page,
                Event::Tick {
                    due_ms: 100,
                    name: "B".into()
                },
                Event::Tick {
                    due_ms: 150,
                    name: "C".into()
                },
                Event::EndDoc,
                Event::ClearNames
            ]
        );
        let mut failed = Job::begin(vec!["A".into(), "B".into()]).unwrap();
        assert!(
            failed
                .tick(|_, job| {
                    job.page_drawn();
                    Err(invalid("late renderer failure"))
                })
                .is_err()
        );
        assert_eq!(failed.pages, 1);
        assert!(!failed.ended && !failed.events.contains(&Event::EndDoc));
        assert_eq!(failed.remaining_names(), ["A", "B"]);
        let mut current = Job::begin(vec!["A".into()]).unwrap();
        current.begin_page();
        current.page_drawn();
        current.begin_page();
        current.page_drawn();
        current.finish();
        assert_eq!(
            current.events,
            [
                Event::BeginDoc,
                Event::Page,
                Event::NewPage,
                Event::Page,
                Event::EndDoc,
                Event::ClearNames
            ]
        );
    }

    #[test]
    fn native_dataset_leaves_keep_cached_words_exact_prefix_and_lazy_sources() {
        use crate::icf::{
            record_image::print_dataset_count as count, settings::gps::gps_group_count_source,
        };
        assert_eq!(count(&[], 3, 100).unwrap(), 4);
        assert_eq!(count(&[], 4, 0).unwrap(), 1);
        assert_eq!(count(&[], 99, 0).unwrap(), 0);
        assert_eq!(count(&[], 2, -1).unwrap(), 0);
        assert_eq!(count(&[], 0x0c, 50).unwrap(), 0);
        assert!(count(&[], 0x0c, 0).is_err());
        assert!(count(&[], 0x0d, 0).is_err());
        let mut source = vec![0; 0x4997a];
        source[0x49914..0x49916].copy_from_slice(&65535_u16.to_le_bytes());
        assert_eq!(count(&source, 0x0c, 0).unwrap(), 65535);
        assert_eq!(count(&source, 0x0d, 0).unwrap(), 0);
        source[0x49978..0x4997a].copy_from_slice(&65535_u16.to_le_bytes());
        assert_eq!(count(&source, 0x0d, 0).unwrap(), 65535);
        for slot in source[0x3fcd4..0x3fe64].as_chunks_mut::<4>().0 {
            slot.copy_from_slice(&0xffff_u32.to_le_bytes());
        }
        source[0x3fcd4..0x3fcd8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(count(&source, 2, 0).unwrap(), 1);
        let mut prefix = vec![0; 0x395ad + 16];
        prefix[0x395ad + 8..].fill(b' ');
        assert_eq!(count(&prefix, 0x17, 0).unwrap(), 1);
        prefix[0x395ad..0x395ad + 8].fill(b' ');
        assert_eq!(count(&prefix, 0x17, 0).unwrap(), 0);
        let mut gps = vec![0xff; 0x3c5e1];
        gps[0x3c5dd..0x3c5df].copy_from_slice(&5_u16.to_le_bytes());
        gps[0x3c5df..0x3c5e1].copy_from_slice(&3_u16.to_le_bytes());
        assert_eq!(gps_group_count_source(&gps, 0).unwrap(), 65534);
        assert_eq!(count(&gps, 0x0a, 256).unwrap(), 65534);
        assert_eq!(gps_group_count_source(&[], 27).unwrap(), 0);
        assert!(gps_group_count_source(&[], 0).is_err());
        gps[0x3c5dd..0x3c5df].copy_from_slice(&300_u16.to_le_bytes());
        assert_eq!(gps_group_count_source(&gps[..0x3c5df], 0).unwrap(), 0);
        let last = crate::icf::special::CompactKind::Memopad
            .offset(10)
            .unwrap();
        let mut compact = vec![0xff; last + 4];
        assert_eq!(count(&compact, 5, 0).unwrap(), 0);
        compact[last..last + 4].copy_from_slice(&123_u32.to_le_bytes());
        assert!(count(&compact, 5, 0).is_err());
        compact.resize(last + 36, 0xff);
        assert_eq!(count(&compact, 5, 0).unwrap(), 1);
        let mut scan = vec![0xff; 0xa60f];
        assert_eq!(count(&scan, 8, 0).unwrap(), 0);
        scan[0xa60b] &= !1;
        assert_eq!(count(&scan, 8, 0).unwrap(), 1);
        assert!(count(&scan[..0xa60e], 8, 0).is_err());
    }

    #[test]
    fn concrete_factories_queries_live_order_and_raw_predicates_are_not_packets() {
        let mut current = vec![0; 0x4997a];
        for map in current[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
            map.copy_from_slice(&0xffff_u32.to_le_bytes());
        }
        current[0x3fcd4..0x3fcd8].copy_from_slice(&0_u32.to_le_bytes());
        current[0x3fcd8..0x3fcdc].copy_from_slice(&1_u32.to_le_bytes());
        current[0] = 2;
        let mut views = Views::default();
        let first = views.register(ViewState::from_image(&current, ViewKind::Memory, 0).unwrap());
        assert_eq!(
            (
                views.states[first].node,
                views.states[first].fixed_columns,
                views.states[first].fixed_rows
            ),
            (Some(2), 3, 5)
        );
        assert!(views.nonempty(&current, first).unwrap());
        let later = views.register(ViewState::from_image(&current, ViewKind::Memory, 1).unwrap());
        assert!(!views.nonempty(&current, first).unwrap());
        views.deactivate(later);
        assert!(views.nonempty(&current, first).unwrap());
        views.states[first].current_row = 8;
        assert_eq!(views.query(&current, 0x3f, 0).unwrap(), [[0, 101]]);
        assert_eq!(views.query(&current, 0x42, 1).unwrap(), [[4, 0]]);
        assert_eq!(views.query(&current, 0x42, 0).unwrap(), [[2, 0]]);
        views.states[first].show_split = 2;
        assert_eq!(views.query(&[], 0x42, 1).unwrap(), [[3, 0]]);
        views.states[first].bc = 3;
        assert!(!views.nonempty(&[], first).unwrap());
        let mut rpt = ViewState::new(ViewKind::Repeater, 0).unwrap();
        rpt.bc = 3;
        let rpt = views.register(rpt);
        assert!(views.nonempty(&[], rpt).is_err());
        let top = views.register(ViewState::new(ViewKind::RepeaterTop, 0).unwrap());
        assert!(!views.nonempty(&[], top).unwrap());
        for kind in [
            ViewKind::Rx,
            ViewKind::FromDv,
            ViewKind::FromFm,
            ViewKind::To,
            ViewKind::UserEdge,
        ] {
            let id = views.register(ViewState::new(kind, 0).unwrap());
            assert!(views.nonempty(&[], id).unwrap());
            views.states[id].bc = 1;
            assert!(!views.nonempty(&[], id).unwrap());
        }
        let mut preset = ViewState::new(ViewKind::Preset, 0).unwrap();
        preset.bc = 255;
        let preset = views.register(preset);
        assert!(views.nonempty(&[], preset).unwrap());
        let raw = views.register(ViewState::new(ViewKind::Memory, 0).unwrap());
        assert!(views.query(&[], 0x3f, 0).is_err());
        views.deactivate(raw);
        assert_eq!(views.query(&[], 0x3f, 0).unwrap(), [[0, 101]]);
        assert!(ViewState::new(ViewKind::Gps, 27).is_err());
        assert_eq!(
            (
                ViewState::new(ViewKind::Gps, 0).unwrap().fixed_columns,
                ViewState::new(ViewKind::Gps, 0).unwrap().fixed_rows
            ),
            (3, 3)
        );
    }
}
