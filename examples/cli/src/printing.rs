//! Portable print command. Typed sheet producers and publication stay shared.

use crate::{
    AppError, Result,
    args::{Sheet, Write},
    io, records, sheets,
};
use clap::{Args as ClapArgs, ValueEnum};
use cs705::{
    icf::{Icf, record, record_image, rows},
    printing::{self as lib, Editor, Grid, Job, Metrics, Paper, ViewKind, ViewState, Views},
};
use serde_json::{Value, json};

#[derive(ClapArgs)]
pub struct Args {
    input: String,
    /// Current registered sheet; --all prints all registered portable views.
    #[arg(
        long,
        value_enum,
        required_unless_present_any = ["all", "native_sheet", "field"],
        conflicts_with_all = ["all", "native_sheet", "field"]
    )]
    sheet: Option<Sheet>,
    #[arg(long)]
    all: bool,
    /// Print semantic fields using independent English labels.
    #[arg(long, conflicts_with_all = ["all", "native_resource", "native_sheet"])]
    field: Vec<String>,
    /// Optional PE analysis oracle; never used by normal typed printing.
    #[arg(long)]
    native_resource: Option<String>,
    /// Registered native resource name, optionally followed by /tree-node.
    #[arg(long, requires = "native_resource", conflicts_with_all = ["sheet", "all"])]
    native_sheet: Option<String>,
    /// Visible native editor gesture COLUMN/ROW=TEXT, not a context/frame packet.
    #[arg(long, requires = "native_resource", conflicts_with_all = ["pending_edit", "all"])]
    pending_cell: Option<String>,
    /// Native group for the current sheet (zero based); --all visits every group.
    #[arg(long, default_value_t = 0, conflicts_with = "all")]
    group: i32,
    /// Printable rectangle width/height in the explicit measurement's pixels.
    #[arg(long, default_value_t = 1200)]
    paper_width: i32,
    #[arg(long, default_value_t = 800)]
    paper_height: i32,
    /// Optional independent font metrics; the PE oracle requires this source.
    #[arg(long)]
    measurement: Option<String>,
    /// Approve publication of the completed print document.
    #[arg(long)]
    yes: bool,
    #[arg(long, value_enum, default_value = "none")]
    preview: Preview,
    /// Virtual 50ms callbacks; cancellation drains remaining sheet names.
    #[arg(long)]
    cancel_after_sheets: Option<usize>,
    /// Portable semantic KEY=VALUE input dispatched before printing, not a
    /// captured native cell/choice packet or proof of GUI inline reachability.
    /// Only the private in-memory document is edited; no ICF is published.
    #[arg(long)]
    pending_edit: Option<String>,
    #[command(flatten)]
    record_options: records::Options,
    #[command(flatten)]
    write: Write,
}

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
enum Preview {
    None,
    Print,
    Cancel,
}

/// A declared portable font, not an OS measurement capture or ready-made pages.
fn measurement(path: Option<&str>) -> Result<Metrics> {
    let Some(path) = path else {
        return Ok(Metrics {
            font_height: -12,
            em: 12,
            advance_em: 6,
            line_em: 12,
            padding: 1,
        });
    };
    let m: Value = serde_json::from_slice(&io::read(path)?)
        .map_err(|_| AppError::invalid("invalid portable print measurement JSON"))?;
    if !m.as_object().is_some_and(|o| {
        o.len() == 7
            && [
                "version",
                "schema",
                "font_height",
                "em",
                "advance_em",
                "line_em",
                "padding",
            ]
            .iter()
            .all(|k| o.contains_key(*k))
    }) || m["version"] != 1
        || m["schema"] != "cs705.portable-monospace"
    {
        return Err(AppError::invalid(
            "print measurement requires exact version1 cs705.portable-monospace fields",
        ));
    }
    let number = |key: &str| {
        m[key]
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| AppError::invalid("print measurement requires signed32-bit integers"))
    };
    Ok(Metrics {
        font_height: number("font_height")?,
        em: number("em")?,
        advance_em: number("advance_em")?,
        line_em: number("line_em")?,
        padding: number("padding")?,
    })
}

#[derive(Clone)]
struct SheetView {
    sheet: Sheet,
    group: i32,
    state: ViewState,
    name: String,
}
impl SheetView {
    fn new(sheet: Sheet, group: i32) -> Result<Self> {
        let kind = match sheet {
            Sheet::Memory => ViewKind::Memory,
            Sheet::Call => ViewKind::Call,
            Sheet::Bandstack => ViewKind::Bandstack,
            Sheet::Memopad => ViewKind::Memopad,
            Sheet::Scan => ViewKind::Scan,
            Sheet::Gps => ViewKind::Gps,
            Sheet::Repeater => ViewKind::Repeater,
            Sheet::Ur => ViewKind::Ur,
            Sheet::Rx => ViewKind::Rx,
            Sheet::FromDv => ViewKind::FromDv,
            Sheet::FromFm => ViewKind::FromFm,
            Sheet::To => ViewKind::To,
            Sheet::UserEdge => ViewKind::UserEdge,
            Sheet::Preset => ViewKind::Preset,
            Sheet::All => return Err(AppError::invalid("use --all, not --sheet all")),
        };
        if group != 0
            && !matches!(
                sheet,
                Sheet::Memory | Sheet::Gps | Sheet::Repeater | Sheet::Preset
            )
        {
            return Err(AppError::invalid("this print factory has no group axis"));
        }
        let state = ViewState::new(kind, group)?;
        let name = format!("{}/{group}", sheet.to_possible_value().unwrap().get_name());
        Ok(Self {
            sheet,
            group,
            state,
            name,
        })
    }
}

fn candidates(args: &Args) -> Result<Vec<SheetView>> {
    if !args.all {
        return Ok(vec![SheetView::new(args.sheet.unwrap(), args.group)?]);
    }
    let mut result = Vec::new();
    // This is the registered portable tree, not all native application classes.
    // Dataset predicates are deliberately evaluated AFTER this enumeration.
    for (sheet, groups) in [
        (Sheet::Memory, 100),
        (Sheet::Call, 1),
        (Sheet::Bandstack, 1),
        (Sheet::Memopad, 1),
        (Sheet::Scan, 1),
        (Sheet::Gps, 27),
        (Sheet::Repeater, 50),
        (Sheet::Ur, 1),
        (Sheet::FromDv, 1),
        (Sheet::FromFm, 1),
        (Sheet::To, 1),
        (Sheet::Rx, 1),
        (Sheet::UserEdge, 1),
        (Sheet::Preset, 5),
    ] {
        for group in 0..groups {
            result.push(SheetView::new(sheet, group)?);
        }
    }
    Ok(result)
}

fn working(
    image: &Icf<'_>,
    candidates: &[SheetView],
    runtime: &records::Runtime,
) -> Result<Vec<u8>> {
    working_kinds(
        image,
        &candidates.iter().map(|v| v.state.kind).collect::<Vec<_>>(),
        runtime,
    )
}

pub fn view_rows(
    image: &Icf<'_>,
    sheet: Sheet,
    group: i32,
    runtime: &records::Runtime,
) -> Result<Value> {
    let view = SheetView::new(sheet, group)?;
    let current = working(image, std::slice::from_ref(&view), runtime)?;
    typed_rows(image, &current, &view, runtime)
}

fn working_kinds(
    image: &Icf<'_>,
    kinds: &[ViewKind],
    runtime: &records::Runtime,
) -> Result<Vec<u8>> {
    let mut current = image.memory().to_vec();
    current.resize(cs705::clone::IMAGE_CAPACITY, 0);
    // Allocation is not a count source. Each reached working leaf is generated
    // by its existing stage8 body before any print predicate consumes it.
    if kinds.contains(&ViewKind::Memory) {
        rows::expand_map(&mut current)?;
    }
    let history = kinds
        .iter()
        .any(|kind| matches!(kind, ViewKind::FromDv | ViewKind::FromFm | ViewKind::To));
    let repeater = kinds.contains(&ViewKind::Repeater) || history;
    let ur = kinds.contains(&ViewKind::Ur) || history;
    if repeater {
        cs705::repeater::working::expand_counts(&mut current)?;
    }
    if ur {
        rows::expand_ur_count(&mut current)?;
    }
    // 4FB00 count/map phase precedes each 4FB20 snapshot/data phase.
    if kinds.contains(&ViewKind::Memory) {
        rows::expand_names(&mut current)?;
    }
    if repeater {
        let context = cs705::repeater::working::DefaultContext::from_context(
            runtime.display_context()?,
            runtime.display_system_langid(),
        )?;
        cs705::repeater::working::expand_rows(&mut current, context)?;
    }
    if ur {
        rows::expand_ur_data(&mut current)?;
    }
    Ok(current)
}

fn typed_rows(
    image: &Icf<'_>,
    current: &[u8],
    view: &SheetView,
    runtime: &records::Runtime,
) -> Result<Value> {
    if view.sheet == Sheet::Memory {
        let mut result = Vec::new();
        for side in 0..200 {
            let index = view.group as u32 * 200 + side;
            let mut holder = record::reset_holder(2, index, 0);
            if record_image::load_channel_into(
                current,
                record_image::ChannelKind::Normal,
                index,
                &mut holder,
            )? {
                result.push(records::channel_json(&holder)?);
            }
        }
        return Ok(Value::Array(result));
    }
    let (start, count) = match view.sheet {
        Sheet::Call => (0, 8),
        Sheet::Bandstack => (0, 45),
        Sheet::Memopad => (1, 10),
        Sheet::Scan => (0, 25),
        Sheet::Gps => (0, 300),
        Sheet::Repeater => (0, 2500),
        Sheet::Ur => (
            0,
            record_image::print_dataset_count(current, 0x0d, 0)? as usize,
        ),
        Sheet::FromDv | Sheet::FromFm => (0, 10),
        Sheet::To => (0, 20),
        Sheet::Rx => (0, 50),
        Sheet::UserEdge => (0, 30),
        Sheet::Preset => (view.group as usize, 1),
        Sheet::Memory | Sheet::All => unreachable!(),
    };
    if view.sheet == Sheet::Ur && count > 300 {
        return Err(AppError::invalid(
            "cached UR count exceeds the portable typed row domain",
        ));
    }
    if view.sheet == Sheet::Ur {
        let working = rows::WorkingUr::from_icf(image)?;
        // The shared UR presentation reads raw slots, so supply stage8's actual
        // ordered rows in a presentation-only storage shape. This temporary Icf
        // never feeds predicates and is never published or substituted for I.
        let ordered = Icf::from_memory(working.working()[..image.memory().len()].to_vec())?;
        return sheets::view(&ordered, view.sheet, start, count, Some(runtime));
    }
    let mut value = sheets::view(image, view.sheet, start, count, Some(runtime))?;
    if view.sheet == Sheet::Gps {
        let groups = image.gps_groups()?;
        let slots = groups
            .get(view.group as usize)
            .ok_or_else(|| AppError::invalid("GPS group unavailable"))?
            .physical_slots();
        let rows = value.as_array().unwrap();
        let ordered = slots
            .iter()
            .map(|slot| {
                rows.iter()
                    .find(|r| r["physical_slot"].as_u64() == Some(*slot as u64))
                    .cloned()
                    .ok_or_else(|| AppError::invalid("GPS ordered slot missing from typed view"))
            })
            .collect::<Result<Vec<_>>>()?;
        value = Value::Array(ordered);
    } else if view.sheet == Sheet::Repeater {
        value
            .as_array_mut()
            .unwrap()
            .retain(|r| r["group"].as_i64() == Some(i64::from(view.group)));
    }
    Ok(value)
}

fn cell(value: &Value) -> String {
    // Typed JSON values are displayed as data, not re-decoded. Non-ASCII glyphs
    // use explicit scalar notation; raw byte values retain their existing hex.
    value
        .to_string()
        .chars()
        .map(|c| {
            if c.is_ascii() {
                c.to_string()
            } else {
                c.escape_unicode().to_string()
            }
        })
        .collect()
}

fn fields(prefix: &str, value: &Value, result: &mut std::collections::BTreeMap<String, String>) {
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            fields(
                &if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                },
                value,
                result,
            );
        }
    } else {
        result.insert(prefix.to_owned(), cell(value));
    }
}

fn grid(name: &str, value: &Value) -> Result<Grid> {
    let rows = value
        .as_array()
        .ok_or_else(|| AppError::invalid("typed print view requires rows"))?;
    let mut columns = std::collections::BTreeSet::new();
    let mut values = Vec::new();
    for row in rows {
        let mut flat = std::collections::BTreeMap::new();
        if row.is_object() {
            fields("", row, &mut flat);
        } else if let Some(selected) = row.as_array() {
            for (index, selected) in selected.iter().enumerate() {
                let column = selected["column"]
                    .as_str()
                    .ok_or_else(|| AppError::invalid("projected print cell needs a column name"))?;
                let value = selected
                    .get("value")
                    .ok_or_else(|| AppError::invalid("projected print cell needs a value"))?;
                flat.insert(format!("{index:05}. {column}"), cell(value));
            }
        } else {
            return Err(AppError::invalid("typed print row requires fields"));
        }
        columns.extend(flat.keys().cloned());
        values.push(flat);
    }
    if columns.is_empty() {
        return Err(AppError::invalid(
            "nonempty native view has no portable typed fields",
        ));
    }
    let columns: Vec<_> = columns.into_iter().collect();
    let mut cells = vec![
        std::iter::once("row".to_string())
            .chain(columns.iter().cloned())
            .collect(),
    ];
    for (i, row) in values.iter().enumerate() {
        cells.push(
            std::iter::once(i.to_string())
                .chain(
                    columns
                        .iter()
                        .map(|key| row.get(key).cloned().unwrap_or_default()),
                )
                .collect(),
        );
    }
    Ok(Grid {
        name: name.to_owned(),
        fixed_rows: 1,
        fixed_columns: 1,
        column_boundaries: vec![false; columns.len() + 1],
        row_boundaries: vec![false; cells.len()],
        cells,
        nodes: None,
        factory: None,
        show_split: 0,
    })
}

/// SVG pages share the same typed cells and layout engine as print output.
pub fn svg_document(name: &str, value: &Value, width: i32, height: i32) -> Result<Vec<u8>> {
    if width <= 0 || height <= 0 {
        return Err(AppError::invalid("SVG page dimensions must be positive"));
    }
    let rows = if value.is_array() {
        value.clone()
    } else {
        json!([{"value":value}])
    };
    if rows.as_array().is_some_and(Vec::is_empty) {
        return Ok(format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\"><title>Empty configuration view</title></svg>\n").into_bytes());
    }
    let grid = grid(name, &rows)?;
    let metrics = measurement(None)?;
    let paper = Paper { width, height };
    let layout = lib::layout(&grid, paper, metrics)?;
    let total = i64::from(height)
        .checked_mul(
            i64::try_from(layout.pages.len())
                .map_err(|_| AppError::invalid("SVG page count overflow"))?,
        )
        .filter(|n| *n <= i64::from(i32::MAX))
        .ok_or_else(|| AppError::invalid("SVG document height overflow"))?;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{total}\" viewBox=\"0 0 {width} {total}\"><title>Configuration view</title>"
    );
    for (index, page) in layout.pages.iter().enumerate() {
        let html = lib::html_page(&grid, page, paper, metrics, index + 1)?;
        let (_, body) = html
            .split_once("<svg ")
            .ok_or_else(|| AppError::invalid("page renderer omitted SVG"))?;
        let (body, _) = body
            .rsplit_once("</svg>")
            .ok_or_else(|| AppError::invalid("page renderer returned incomplete SVG"))?;
        svg.push_str(&format!(
            "<svg y=\"{}\" {body}</svg>",
            index as i64 * i64::from(height)
        ));
    }
    svg.push_str("</svg>\n");
    Ok(svg.into_bytes())
}

struct Plan {
    view: Option<SheetView>,
    grid: Grid,
    layout: lib::Layout,
}

fn render_plan(
    plan: &Plan,
    current: &[u8],
    views: &mut Views,
    job: &mut Job,
    html: &mut String,
    paper: Paper,
    metrics: Metrics,
) -> std::io::Result<()> {
    let Some(view) = &plan.view else {
        if !job.cancelled {
            for page in &plan.layout.pages {
                job.begin_page();
                html.push_str(&lib::html_page(
                    &plan.grid,
                    page,
                    paper,
                    metrics,
                    job.pages + 1,
                )?);
                job.page_drawn();
            }
        }
        return Ok(());
    };
    let id = views.register(ViewState::from_image(current, view.state.kind, view.group)?);
    // A cancelled callback still creates the factory and reads its predicate.
    if views.nonempty(current, id)? && !job.cancelled {
        for page in &plan.layout.pages {
            if !views.nonempty(current, id)? || job.cancelled {
                break;
            }
            job.begin_page();
            html.push_str(&lib::html_page(
                &plan.grid,
                page,
                paper,
                metrics,
                job.pages + 1,
            )?);
            job.page_drawn();
        }
    }
    views.deactivate(id);
    Ok(())
}

pub fn run(args: Args, preferences: &Value) -> Result<u8> {
    if args.native_resource.is_some() {
        return run_native(args, preferences);
    }
    let runtime = args
        .record_options
        .load(std::iter::once(args.input.as_str()).chain(args.measurement.as_deref()))?
        .with_input_options(crate::display::input_options(preferences)?);
    let metrics = measurement(args.measurement.as_deref())?;
    metrics.validate()?;
    let paper = Paper {
        width: args.paper_width,
        height: args.paper_height,
    };
    if paper.width <= 0 || paper.height <= 0 {
        return Err(AppError::invalid(
            "printable paper dimensions must be positive",
        ));
    }
    let approved = args.yes || args.preview == Preview::Print;
    if approved && args.preview != Preview::Cancel {
        io::preflight(&args.write)?;
    }
    let input = io::read(&args.input)?;
    let mut image = Icf::parse(&input)?;
    let original = image.memory().to_vec();
    let editor_adapter = if args.pending_edit.is_some() {
        "portable-semantic-assignment; native inline reachability not asserted"
    } else {
        "headless-no-visible-editor"
    };
    let mut editor = Editor {
        validation_guard: 0,
        visible: args.pending_edit.is_some(),
        text: args.pending_edit.clone().unwrap_or_default().into_bytes(),
    };
    let mut syncs = Vec::new();
    for origin in ["wrapper", "41E00"] {
        syncs.push(
            json!({"id":0x1b,"synchronous":true,"origin":origin,"visible_before":editor.visible}),
        );
        let result = lib::sync_editor(&mut editor, |text| {
            let text = std::str::from_utf8(text)
                .map_err(|_| AppError::invalid("pending editor requires UTF-8 command input"))?;
            crate::assignments(&mut image, &[text.to_owned()], &runtime).map(|_| ())
        });
        if let Err(error) = result {
            io::report(
                &json!({"operation":"portable-print","editor_adapter":editor_adapter,"syncs":syncs,"editor_visible":editor.visible,"validation_guard":editor.validation_guard,"changed_bytes":original.iter().zip(image.memory()).filter(|(a,b)| a!=b).count(),"begin_doc":false,"published":false}),
            )?;
            return Err(error);
        }
    }
    let candidates = if args.field.is_empty() {
        candidates(&args)?
    } else {
        Vec::new()
    };
    let current = working(&image, &candidates, &runtime)?;
    let mut views = Views::default();
    let mut plans = Vec::new();
    if !args.field.is_empty() {
        let g = grid(
            "Configuration fields",
            &crate::display::english::read(&image, &args.field, &runtime)?,
        )?;
        let layout = lib::layout(&g, paper, metrics)?;
        plans.push(Plan {
            view: None,
            grid: g,
            layout,
        });
    }
    for view in &candidates {
        let id = views.register(ViewState::from_image(
            &current,
            view.state.kind,
            view.group,
        )?);
        if views.nonempty(&current, id)? {
            let g = grid(&view.name, &typed_rows(&image, &current, view, &runtime)?)?;
            let layout = lib::layout(&g, paper, metrics)?;
            if !layout.pages.is_empty() {
                plans.push(Plan {
                    view: Some(view.clone()),
                    grid: g,
                    layout,
                });
            }
        }
        views.deactivate(id);
    }
    let count: usize = plans.iter().map(|p| p.layout.pages.len()).sum();
    let layouts: Vec<_> = plans.iter().map(|p| json!({"name":p.grid.name,"node":p.view.as_ref().and_then(|v|v.state.node),"type":p.view.as_ref().map(|v|v.state.record_type()),"bc":p.view.as_ref().map(|v|v.state.bc),"native_fixed_columns":p.view.as_ref().map(|v|v.state.fixed_columns),"native_fixed_rows":p.view.as_ref().map(|v|v.state.fixed_rows),"portable_fixed_columns":p.grid.fixed_columns,"portable_fixed_rows":p.grid.fixed_rows,"horizontal_be16":p.layout.horizontal,"vertical_be16":p.layout.vertical,"pages":p.layout.pages.iter().map(|p|json!({"columns":[p.columns.start,p.columns.end],"rows":[p.rows.start,p.rows.end],"font_height":p.font_height,"attempts":p.attempts,"fits":p.fits})).collect::<Vec<_>>()})).collect();
    if args.preview == Preview::Cancel || !approved || plans.is_empty() {
        io::report(
            &json!({"operation":"portable-print","backend":"svg-html","os_printed":false,"editor_adapter":editor_adapter,"projection":"independent typed fields, not native merged cells","syncs":syncs,"editor_visible":editor.visible,"changed_bytes":original.iter().zip(image.memory()).filter(|(a,b)| a!=b).count(),"candidates":candidates.len(),"count_pages":count,"layouts":layouts,"preview_approved":false,"begin_doc":false,"published":false}),
        )?;
        return Ok(0);
    }
    let mut job = Job::begin(plans.iter().map(|p| p.grid.name.clone()).collect())?;
    let mut html = String::from(
        "<!doctype html>\n<html lang=\"en\"><meta charset=\"utf-8\"><title>Portable typed print</title><style>.page{break-after:page}svg{display:block}@media print{h2{display:none}}</style><body>\n",
    );
    let mut done = 0;
    job.cancelled = args.cancel_after_sheets == Some(0);
    let rendered = if args.all {
        let mut result = Ok(());
        loop {
            match job.tick(|name, job| {
                let plan = plans.iter().find(|p| p.grid.name == name).unwrap();
                render_plan(plan, &current, &mut views, job, &mut html, paper, metrics)?;
                done += 1;
                if args.cancel_after_sheets == Some(done) {
                    job.cancelled = true;
                }
                Ok(())
            }) {
                Ok(true) => {}
                Ok(false) => break,
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        result
    } else {
        let result = render_plan(
            &plans[0], &current, &mut views, &mut job, &mut html, paper, metrics,
        );
        if result.is_ok() {
            if args.cancel_after_sheets == Some(1) {
                job.cancelled = true;
            }
            job.finish();
        }
        result
    };
    let events: Vec<_> = job
        .events
        .iter()
        .map(|event| match event {
            lib::Event::BeginDoc => json!({"event":"BeginDoc"}),
            lib::Event::Tick { due_ms, name } => {
                json!({"event":"Tick","virtual_due_ms":due_ms,"name":name})
            }
            lib::Event::NewPage => json!({"event":"NewPage"}),
            lib::Event::Page => json!({"event":"Page"}),
            lib::Event::EndDoc => json!({"event":"EndDoc"}),
            lib::Event::ClearNames => json!({"event":"ClearNames"}),
        })
        .collect();
    io::report(
        &json!({"operation":"portable-print","backend":"svg-html","os_printed":false,"editor_adapter":editor_adapter,"projection":"independent typed fields, not native merged cells","syncs":syncs,"editor_visible":editor.visible,"changed_bytes":original.iter().zip(image.memory()).filter(|(a,b)| a!=b).count(),"candidates":candidates.len(),"count_pages":count,"layouts":layouts,"preview_approved":args.preview==Preview::Print,"drawn_pages":job.pages,"events":events,"cancelled":job.cancelled,"ended":job.ended,"remaining_names":job.remaining_names(),"published":false}),
    )?;
    rendered?;
    if job.cancelled {
        return Ok(0);
    }
    html.push_str("</body></html>\n");
    io::publish(html.as_bytes(), &args.write)?;
    io::report(
        &json!({"operation":"portable-print-publication","published":!args.write.dry_run,"approved_by_yes":args.yes,"approved_by_preview":args.preview==Preview::Print,"os_printed":false}),
    )?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_svg_preserves_projected_order_duplicates_and_escaping() {
        let rows = json!([[{"column":"name","value":"<UI&>"},{"column":"name","value":"second"}]]);
        let grid = grid("Independent view", &rows).unwrap();
        assert_eq!(grid.cells[0], ["row", "00000. name", "00001. name"]);
        let svg = svg_document("Independent view", &rows, 300, 200).unwrap();
        let svg = String::from_utf8(svg).unwrap();
        assert!(svg.starts_with("<svg ") && svg.ends_with("</svg>\n"));
        assert!(svg.contains("&lt;UI&amp;&gt;") && svg.contains("second"));
        assert!(svg_document("Invalid view", &json!([1]), 300, 200).is_err());
    }

    #[test]
    fn working_data_uses_ordered_snapshots_gateway_aux_and_live_q_for_history() {
        let mut memory = vec![0; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        memory[0x3ab82..0x3bf6e].fill(255);
        memory[0x3ab82..0x3ab86].copy_from_slice(&[7, 0, 4, 0]);
        memory[0x3bf10..0x3bf12].copy_from_slice(&0_u16.to_le_bytes());
        for (physical, link) in [(7, 5_u16), (4, 6)] {
            let at = 0xaa0f + 52 * physical;
            memory[at..at + 4].copy_from_slice(&(145_000_000 + physical as u32).to_le_bytes());
            memory[at + 8..at + 10].copy_from_slice(&link.to_le_bytes());
        }
        memory[0x2a5df + 7 * 5..0x2a5df + 7 * 6].fill(1);
        memory[0x2a5df + 7 * 6..0x2a5df + 7 * 7].fill(2);
        memory[0x3c0a7..0x3c2ff].fill(255);
        memory[0x3c0a7..0x3c0ab].copy_from_slice(&[7, 0, 0, 0]);
        for (physical, text) in [(0, b"UR-ZERO"), (7, b"UR-SEVN")] {
            let at = 0x2f703 + 16 * physical;
            memory[at..at + 16].fill(b' ');
            memory[at..at + text.len()].copy_from_slice(text);
        }
        let image = Icf::from_memory(memory.clone()).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/private/cli-r3/fixtures");
        std::fs::create_dir_all(&root).unwrap();
        let q_path = root.join(format!("working-q-{}.bin", std::process::id()));
        for source in [0_u8, 1] {
            let mut q = [0; 24];
            q[0] = source;
            std::fs::write(&q_path, q).unwrap();
            let runtime = records::Options {
                record_context: Some(q_path.to_str().unwrap().to_owned()),
                ..Default::default()
            }
            .load(std::iter::empty::<&str>())
            .unwrap();
            for kinds in [
                vec![ViewKind::Repeater, ViewKind::Ur],
                vec![ViewKind::FromDv],
                vec![ViewKind::FromFm],
                vec![ViewKind::To],
            ] {
                let current = working_kinds(&image, &kinds, &runtime).unwrap();
                assert_eq!(
                    record_image::print_dataset_count(&current, 0x0c, 3).unwrap(),
                    2
                );
                assert_eq!(
                    record_image::print_dataset_count(&current, 0x0d, 0).unwrap(),
                    2
                );
                assert_eq!(&current[0x2f703..0x2f70a], b"UR-SEVN");
                assert_eq!(&current[0x2f713..0x2f71a], b"UR-ZERO");
                assert_eq!(&current[0xaa0f..0xaa13], &145_000_007_u32.to_le_bytes());
                assert_eq!(
                    &current[0xaa0f + 52..0xaa13 + 52],
                    &145_000_004_u32.to_le_bytes()
                );
                assert_eq!(&current[0x2a5df..0x2a5e6], &[1; 7]);
                assert_eq!(&current[0x2a5e6..0x2a5ed], &[2; 7]);
                let mut holder = record::reset_holder(0x0c, 2, 0);
                cs705::repeater::working::load_into(&current, 2, &mut holder).unwrap();
                assert_eq!(holder.offset_hz(), if source == 0 { 5_000_000 } else { 0 });
                assert_eq!(
                    holder.coordinate_bytes(true)[3],
                    if source == 0 { b'E' } else { b'W' }
                );
            }
        }
        std::fs::remove_file(q_path).unwrap();
        let absent = records::Options::default()
            .load(std::iter::empty::<&str>())
            .unwrap();
        assert!(working_kinds(&image, &[ViewKind::Repeater], &absent).is_err());
        assert_eq!(image.memory(), memory);
    }
}

fn source_error(error: AppError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, error.message)
}

fn run_native(args: Args, preferences: &Value) -> Result<u8> {
    use lib::native::{Class, Factory, InlineFrame, Resources, read_source};
    let asset = args.native_resource.as_deref().unwrap();
    let measurement_path = args
        .measurement
        .as_deref()
        .ok_or_else(|| AppError::invalid("PE print oracle needs --measurement"))?;
    let runtime = args
        .record_options
        .load([args.input.as_str(), measurement_path, asset])?
        .with_input_options(crate::display::input_options(preferences)?);
    if asset == "-" {
        return Err(AppError::invalid(
            "native resource asset requires a seekable file",
        ));
    }
    if args.pending_edit.is_some() {
        return Err(AppError::invalid(
            "native resource printing uses --pending-cell, not semantic --pending-edit",
        ));
    }
    let metrics = measurement(Some(measurement_path))?;
    metrics.validate()?;
    let paper = Paper {
        width: args.paper_width,
        height: args.paper_height,
    };
    if paper.width <= 0 || paper.height <= 0 {
        return Err(AppError::invalid(
            "printable paper dimensions must be positive",
        ));
    }
    let approved = args.yes || args.preview == Preview::Print;
    if approved && args.preview != Preview::Cancel {
        io::preflight(&args.write)?;
    }
    let bytes = read_source(std::fs::File::open(asset)?)?;
    let source = Resources::from_pe(&bytes)?;
    let input = io::read(&args.input)?;
    let mut image = Icf::parse(&input)?;
    let original = image.memory().to_vec();
    let mut document =
        cs705::icf::document::Document::from_current(original.clone(), Vec::new(), 6);
    let mut invalidations = Vec::new();
    let mut factories = source.registered(&image)?;
    if let Some(selector) = &args.native_sheet {
        let (name, node) = selector
            .rsplit_once('/')
            .map_or((selector.as_str(), None), |(name, node)| (name, Some(node)));
        let name = if name.starts_with("GI_Sheet_") {
            name.to_owned()
        } else {
            format!("GI_Sheet_{name}")
        };
        let node = node.map(crate::values::parse::<i32>).transpose()?;
        factories
            .retain(|f| f.resource == name && node.map_or(f.group == args.group, |n| f.node == n));
    } else if !args.all {
        let selected = SheetView::new(args.sheet.unwrap(), args.group)?;
        factories.retain(|f| Some(f.node as u32) == selected.state.node);
    }
    if factories.is_empty() {
        return Err(AppError::invalid(
            "native sheet is not in the generated registration tree",
        ));
    }
    let split = preferences
        .get("native-display")
        .map(crate::display::settings)
        .transpose()?
        .map(|s| s.split);
    let show_split = if factories.iter().any(|f| f.class == Class::Split) {
        split.ok_or_else(|| {
            AppError::safety(
                "native split factory needs C18 from existing native-display configuration",
            )
        })?
    } else {
        split.unwrap_or(0)
    };
    let kinds: Vec<_> = factories
        .iter()
        .filter_map(|f| match f.class {
            Class::Split if f.node != 103 => Some(ViewKind::Memory),
            Class::Repeater => Some(ViewKind::Repeater),
            Class::Ur => Some(ViewKind::Ur),
            _ if f.node == 210 => Some(ViewKind::FromDv),
            _ if f.node == 211 => Some(ViewKind::FromFm),
            _ if f.node == 213 => Some(ViewKind::To),
            _ => None,
        })
        .collect();
    let mut current = working_kinds(&image, &kinds, &runtime)?;
    let pending = args
        .pending_cell
        .as_deref()
        .map(|input| -> Result<_> {
            let (coordinate, text) = input
                .split_once('=')
                .ok_or_else(|| AppError::invalid("pending cell requires COLUMN/ROW=TEXT"))?;
            let (column, row) = coordinate
                .split_once('/')
                .ok_or_else(|| AppError::invalid("pending cell requires COLUMN/ROW"))?;
            if !text.is_ascii() {
                return Err(AppError::invalid(
                    "pending native text requires explicit native ASCII input",
                ));
            }
            Ok((
                crate::values::parse::<usize>(column)?,
                crate::values::parse::<usize>(row)?,
                text.as_bytes().to_vec(),
            ))
        })
        .transpose()?;
    let editor_grid = if pending.is_some() {
        Some(
            source.instantiate(&image, &current, &factories[0], show_split, |name| {
                crate::display::print_condition(name, &runtime).map_err(source_error)
            })?,
        )
    } else {
        None
    };
    let mut editor = Editor {
        validation_guard: 0,
        visible: pending.is_some(),
        text: pending.as_ref().map(|p| p.2.clone()).unwrap_or_default(),
    };
    let mut syncs = Vec::new();
    for origin in ["wrapper", "41E00"] {
        syncs.push(
            json!({"id":0x1b,"synchronous":true,"origin":origin,"visible_before":editor.visible}),
        );
        let result = lib::sync_editor(&mut editor, |text| -> Result<()> {
            let (column, row, _) = pending
                .as_ref()
                .ok_or_else(|| AppError::invalid("visible native editor has no selected cell"))?;
            let grid = editor_grid
                .as_ref()
                .ok_or_else(|| AppError::invalid("visible native editor has no resource grid"))?;
            let frame = InlineFrame::commit(grid, *column, *row, text)?;
            crate::records::native_inline_edit(&mut image, &frame, &runtime)?;
            let mut action = runtime.take_native_action().ok_or_else(|| {
                AppError::invalid("native inline dispatcher did not return its action collector")
            })?;
            document.replace_current(image.memory().to_vec());
            document.finish_action(&mut action.context, frame.notify != 0, action.flags, |_, _, flags| {
                if frame.callback != 0x3173c { return Err(cs705::icf::Error::Invalid { line:0, reason:"native inline callback has no registered receiver" }); }
                invalidations.push(json!({"receiver":"native-grid","column":frame.column,"row":frame.row,"flags":flags}));
                Ok(())
            })?;
            Ok(())
        });
        if let Err(error) = result {
            io::report(
                &json!({"operation":"native-resource-print","syncs":syncs,"editor_visible":editor.visible,"validation_guard":editor.validation_guard,"changed_bytes":original.iter().zip(image.memory()).filter(|(a,b)|a!=b).count(),"invalidations":invalidations,"document_dirty":document.dirty(),"timer_ms":document.timer_ms(),"begin_doc":false,"published":false}),
            )?;
            return Err(error);
        }
    }
    current = working_kinds(&image, &kinds, &runtime)?;
    let mut views = Views::default();
    let mut plans: Vec<(Factory, Grid, lib::Layout)> = Vec::new();
    for factory in &factories {
        if !source.eligible(&current, factory, show_split, &mut views)? {
            continue;
        }
        let grid = source.instantiate(&image, &current, factory, show_split, |name| {
            crate::display::print_condition(name, &runtime).map_err(source_error)
        })?;
        let layout = lib::layout(&grid, paper, metrics)?;
        if !layout.pages.is_empty() {
            plans.push((factory.clone(), grid, layout));
        }
    }
    let count: usize = plans.iter().map(|(_, _, l)| l.pages.len()).sum();
    let layouts: Vec<_> = plans.iter().map(|(f,g,l)| json!({"name":g.name,"class":format!("{:?}",f.class),"node":f.node,"group":f.group,"native_fixed_columns":g.fixed_columns,"native_fixed_rows":g.fixed_rows,"horizontal_be16":l.horizontal,"vertical_be16":l.vertical,"column_boundaries":g.column_boundaries,"row_boundaries":g.row_boundaries,"pages":l.pages.iter().map(|p|json!({"columns":[p.columns.start,p.columns.end],"rows":[p.rows.start,p.rows.end],"font_height":p.font_height,"attempts":p.attempts,"fits":p.fits})).collect::<Vec<_>>()})).collect();
    if args.preview == Preview::Cancel || !approved || plans.is_empty() {
        io::report(
            &json!({"operation":"native-resource-print","backend":"svg-html","os_printed":false,"projection":"native resource merges; explicit byte glyph notation and declared monospace metrics","syncs":syncs,"editor_visible":editor.visible,"changed_bytes":original.iter().zip(image.memory()).filter(|(a,b)|a!=b).count(),"invalidations":invalidations,"document_dirty":document.dirty(),"timer_ms":document.timer_ms(),"candidates":factories.len(),"count_pages":count,"layouts":layouts,"begin_doc":false,"published":false}),
        )?;
        return Ok(0);
    }
    let mut job = Job::begin(plans.iter().map(|(_, g, _)| g.name.clone()).collect())?;
    let mut html = String::from(
        "<!doctype html>\n<html lang=\"en\"><meta charset=\"utf-8\"><title>Native resource print</title><style>.page{break-after:page}svg{display:block}@media print{h2{display:none}}</style><body>\n",
    );
    job.cancelled = args.cancel_after_sheets == Some(0);
    let mut done = 0;
    let mut render = |name: &str, job: &mut Job| {
        let (factory, grid, layout) =
            plans
                .iter()
                .find(|(_, g, _)| g.name == name)
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "native print task is not registered",
                    )
                })?;
        source.render(
            &current,
            grid,
            layout,
            &mut views,
            job,
            paper,
            metrics,
            &mut html,
            |node| {
                crate::display::print_cell(&image, &current, factory, node, &runtime, &source)
                    .map_err(source_error)
            },
        )?;
        done += 1;
        if args.cancel_after_sheets == Some(done) {
            job.cancelled = true;
        }
        Ok(())
    };
    let rendered = if args.all {
        loop {
            match job.tick(&mut render) {
                Ok(true) => (),
                Ok(false) => break Ok(()),
                Err(error) => break Err(error),
            }
        }
    } else {
        let result = render(&plans[0].1.name, &mut job);
        if result.is_ok() {
            job.finish();
        }
        result
    };
    let events: Vec<_> = job.events.iter().map(|e| format!("{e:?}")).collect();
    io::report(
        &json!({"operation":"native-resource-print","backend":"svg-html","os_printed":false,"projection":"native resource merges; explicit byte glyph notation and declared monospace metrics","syncs":syncs,"editor_visible":editor.visible,"changed_bytes":original.iter().zip(image.memory()).filter(|(a,b)|a!=b).count(),"invalidations":invalidations,"document_dirty":document.dirty(),"timer_ms":document.timer_ms(),"candidates":factories.len(),"count_pages":count,"layouts":layouts,"preview_approved":args.preview==Preview::Print,"drawn_pages":job.pages,"events":events,"cancelled":job.cancelled,"ended":job.ended,"remaining_names":job.remaining_names(),"published":false}),
    )?;
    rendered?;
    if job.cancelled {
        return Ok(0);
    }
    html.push_str("</body></html>\n");
    io::publish(html.as_bytes(), &args.write)?;
    io::report(
        &json!({"operation":"native-resource-print-publication","published":!args.write.dry_run,"approved_by_yes":args.yes,"approved_by_preview":args.preview==Preview::Print,"os_printed":false}),
    )?;
    Ok(0)
}
