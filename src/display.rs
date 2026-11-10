//! Native display settings and owned software controls, without an OS UI.

use crate::icf::{context::Context, record, record_image};
use crate::printing::{ViewKind, ViewState, Views};
use std::collections::VecDeque;
use std::io::{self, Error, ErrorKind};

mod grid;
pub use grid::{
    Cell, Column, FontMetrics, Grid, Row, channel_cell, channel_paint, channel_value_paint,
};
pub mod filter;
mod notice;
pub use notice::{NoticeCell, NoticeParent, notice_cell};

fn invalid(reason: &'static str) -> Error {
    Error::new(ErrorKind::InvalidInput, reason)
}

/// Live C34/C38 getters use the FULL DWORD; menu Checked uses its low bit.
/// GUI language is C00, never Q, SYSTEM LANGID or a guessed process locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputOptions {
    pub callsign_dialog: u32,
    pub auto_correct: u32,
    pub gui_language: Option<u32>,
}

impl Default for InputOptions {
    fn default() -> Self {
        // 54789/54790 initialize both options. Language needs its own source.
        Self {
            callsign_dialog: 1,
            auto_correct: 1,
            gui_language: None,
        }
    }
}

impl InputOptions {
    pub fn callsign_dialog_enabled(self) -> bool {
        self.callsign_dialog != 0
    }
    pub fn auto_correct_enabled(self) -> bool {
        self.auto_correct != 0
    }
    pub fn japanese(self) -> io::Result<bool> {
        self.gui_language
            .map(|language| language == 1)
            .ok_or_else(|| invalid("GUI language C00 source unavailable"))
    }
}

/// GUI configuration words, not radio image fields. C98 is the high bitset word.
/// Adoption is raw; only the registry loader applies its documented correction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub language: u32,
    pub system_language: u32,
    pub font_percent: u32,
    pub toolbar: u32,
    pub split: u32,
    pub callsign_dialog: u32,
    pub auto_correct: u32,
    pub fixed_name: u32,
    pub column_bits: u32,
    pub column_bits_high: u32,
}

fn column_bit(key: u32) -> Option<u32> {
    match key {
        0x2d => Some(0),
        0x2f..=0x31 => Some(key - 0x2e),
        0xe6..=0xe9 => Some(key - 0xe2),
        _ => None,
    }
}

impl Settings {
    /// SYSTEM input is48B0C's0/1 predicate, not a raw LANGID or GUI language.
    pub fn system_japanese(&self) -> bool {
        self.system_language == 1
    }
    /// Shared event0 menu query. Only Checked bit7/submenu disabled bit0 change.
    /// No view, font metric, queue, source image or mutation is required.
    pub fn query_menu(
        &self,
        key: u32,
        index: u32,
        flags1d: &mut u8,
        flags1e: &mut u8,
    ) -> io::Result<()> {
        let checked = if let Some(bit) = column_bit(key) {
            self.bit(bit)
        } else {
            match key {
                0x2b => self.fixed_name != 0,
                0x2c => self.split & 1 != 0,
                0x2e => {
                    *flags1e = *flags1e & !1 | u8::from(!self.bit(0));
                    return Ok(());
                }
                0x36 => self.font_percent == index,
                0x37 => self.language == index,
                0x38 => self.toolbar & 1 != 0,
                0x40 => self.callsign_dialog & 1 != 0,
                0x41 => self.auto_correct & 1 != 0,
                _ => return Err(invalid("unknown native display command")),
            }
        };
        *flags1d = *flags1d & 0x7f | (u8::from(checked) << 7);
        Ok(())
    }
    pub fn input_options(&self) -> InputOptions {
        InputOptions {
            callsign_dialog: self.callsign_dialog,
            auto_correct: self.auto_correct,
            gui_language: Some(self.language),
        }
    }
    pub fn registry_loaded(mut self) -> Self {
        fn boolean(value: u32) -> u32 {
            (value as i32).clamp(0, 1) as u32
        }
        self.language = boolean(self.language);
        self.font_percent = (self.font_percent as i32).clamp(25, 1000) as u32;
        self.toolbar = u32::from(self.toolbar != 0);
        self.split = u32::from(self.split != 0);
        self.fixed_name = boolean(self.fixed_name);
        self.callsign_dialog = boolean(self.callsign_dialog);
        self.auto_correct = boolean(self.auto_correct);
        self
    }
    pub fn bit(&self, index: u32) -> bool {
        match index {
            0..32 => self.column_bits & (1 << index) != 0,
            32..64 => self.column_bits_high & (1 << (index - 32)) != 0,
            _ => false,
        }
    }
    pub fn toggle_bit(&mut self, index: u32) -> io::Result<()> {
        match index {
            0..32 => self.column_bits ^= 1 << index,
            32..64 => self.column_bits_high ^= 1 << (index - 32),
            _ => return Err(invalid("column bit outside native two-DWORD set")),
        }
        Ok(())
    }
}

fn data_error(error: crate::icf::Error) -> Error {
    Error::new(ErrorKind::InvalidData, error)
}

/// B0164: the live query3F head supplies the descriptor, not node-2.
/// Counts are NOT loop guards: a zero descriptor still loads row0 once.
pub fn sql_mask(
    current: &[u8],
    node: u32,
    settings: &Settings,
    views: &mut Views,
    q: Option<&Context>,
) -> io::Result<u32> {
    let kind = match node {
        2..=101 => 2,
        103 => 3,
        401 => 4,
        402 => 5,
        _ => 0,
    };
    let answer = if kind == 3 {
        None
    } else {
        views.query(current, 0x3f, 0)?.first().copied()
    };
    let (group, start, end) = match kind {
        2 => match answer.map(|a| a[0] as i32).filter(|g| (0..100).contains(g)) {
            Some(group) => (group, 0, 199),
            None => (0, 0, 0),
        },
        3 => (100, 0, 7),
        4 | 5 => match answer.map(|a| a[0] as i32).filter(|g| (0..50).contains(g)) {
            Some(group) => {
                let mut start = 0_i32;
                for g in 0..group {
                    start = start.wrapping_add(
                        record_image::print_dataset_count(current, 0x0c, g).map_err(data_error)?
                            as i32,
                    );
                }
                let count = record_image::print_dataset_count(current, 0x0c, group)
                    .map_err(data_error)? as i32;
                (
                    group,
                    start,
                    start.wrapping_add(count).wrapping_sub(1).max(0),
                )
            }
            None => (0, 0, 0),
        },
        _ => return Ok(0),
    };
    let mut holder = record::reset_holder(kind, 0, 0);
    let mut mask = 0;
    for row in start..=end {
        let index = if kind == 2 {
            (group as u32).wrapping_mul(200).wrapping_add(row as u32)
        } else {
            row as u32
        };
        if matches!(kind, 2 | 3) {
            record_image::load_channel_into(
                current,
                if kind == 2 {
                    record_image::ChannelKind::Normal
                } else {
                    record_image::ChannelKind::Call
                },
                index,
                &mut holder,
            )
            .map_err(data_error)?;
        } else {
            record_image::load_compact_into(current, kind, index, &mut holder)
                .map_err(data_error)?;
        }
        if !holder.valid() {
            continue;
        }
        if matches!(kind, 2 | 3)
            && settings.split == 0
            && holder.as_bytes()[0x24] == 1
            && holder.as_bytes()[0x25] == 0
        {
            continue;
        }
        if kind == 4 {
            let band = holder.range_index();
            if band == 9
                && q.ok_or_else(|| invalid("SQL band9 needs live Q"))?
                    .effective_f()
                    .map_err(data_error)?
                    == 0
            {
                continue;
            }
            if band == 14
                && q.ok_or_else(|| invalid("SQL band14 needs live Q"))?
                    .as_bytes()[0x10]
                    == 0
            {
                continue;
            }
        }
        mask |= match holder.mode() {
            13 | 14 => 2,
            16 => 12,
            17 => 8,
            _ => 0,
        };
    }
    Ok(mask)
}

/// 5A07C/5A604: true is the width-zero predicate, not a visible bit.
pub fn column_hidden(key: u32, rx: bool, settings: &Settings, mask: u32) -> bool {
    if key == 0xed {
        return true;
    }
    if rx {
        let bit = match key {
            0x168..=0x169 => Some(4),
            0x16b..=0x16e => Some(5),
            0x16f..=0x170 => Some(6),
            0x171..=0x179 => Some(7),
            _ => None,
        };
        if let Some(bit) = bit {
            return !settings.bit(bit);
        }
        if matches!(key, 0x163..=0x167 | 0x16a | 0x17a) {
            return settings.column_bits & 0xf0 == 0;
        }
        return false;
    }
    let category = match key {
        0x10d..=0x111 => 1,
        0x112..=0x113 => 2,
        0x114..=0x116 => 3,
        _ => return false,
    };
    !(settings.bit(0) && settings.bit(category) && mask & (1 << category) != 0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Queued {
    pub event: u32,
    pub argument: u32,
    pub value: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toolbar {
    pub visible: bool,
    /// Explicit control placement (left, top, width, height), not window pixels.
    pub placement: [i32; 4],
    pub visible_changes: u64,
}
impl Toolbar {
    pub fn set_visible(
        &mut self,
        visible: bool,
        notify: impl FnOnce(&Self) -> io::Result<()>,
    ) -> io::Result<()> {
        if self.visible != visible {
            // 14FF98 writes57 before B00B/virtual5C, including their failure.
            self.visible = visible;
            self.visible_changes += 1;
            notify(self)?;
        }
        Ok(())
    }
    pub fn occupied_height(&self) -> i32 {
        if self.visible { self.placement[3] } else { 0 }
    }
}

/// Owned software display. The original shared query producer is reused intact.
pub struct State {
    pub settings: Settings,
    pub views: Views,
    pub current: Option<Grid>,
    pub toolbar: Toolbar,
    pub queue: VecDeque<Queued>,
    pub font_changed: bool,
    pub resource_language: u32,
    pub resource_generation: u64,
    pub metrics: FontMetrics,
    saved: Option<(ViewKind, i32, i32, usize)>,
}
impl State {
    /// Borrow the resource generation currently adopted by event26. Before its
    /// drain, an accepted C00 change still leaves the previous generation live.
    pub fn resources<'a>(
        &self,
        asset: &'a [u8],
    ) -> io::Result<crate::printing::native::Resources<'a>> {
        Ok(crate::printing::native::Resources::from_pe(asset)?
            .with_language(self.resource_language, self.settings.system_japanese()))
    }

    pub fn new(settings: Settings, metrics: FontMetrics, placement: [i32; 4]) -> io::Result<Self> {
        metrics.validate()?;
        if placement[2] < 0 || placement[3] < 0 {
            return Err(invalid("negative toolbar dimensions"));
        }
        Ok(Self {
            resource_language: settings.language,
            toolbar: Toolbar {
                visible: settings.toolbar != 0,
                placement,
                visible_changes: 0,
            },
            settings,
            metrics,
            views: Views::default(),
            current: None,
            queue: VecDeque::new(),
            font_changed: false,
            resource_generation: 0,
            saved: None,
        })
    }
    pub fn open(
        &mut self,
        current: &[u8],
        kind: ViewKind,
        group: i32,
        q: Option<&Context>,
    ) -> io::Result<()> {
        self.invalidate();
        let view = ViewState::from_image_with_split(current, kind, group, self.settings.split)?;
        let id = self.views.register(view);
        let grid = Grid::build(
            current,
            id,
            &self.settings,
            &mut self.views,
            self.metrics,
            q,
            None,
        );
        // A failed build owns no displayed grid. Retire only its new query
        // receiver; the earlier invalidation and saved selection remain.
        if grid.is_err() {
            self.views.deactivate(id);
        }
        self.current = Some(grid?);
        self.saved = Some((kind, group, self.views.states[id].current_row, 0));
        Ok(())
    }
    fn invalidate(&mut self) {
        if let Some(grid) = self.current.take() {
            let view = &self.views.states[grid.view_id];
            self.saved = Some((
                view.kind,
                view.group,
                view.current_row,
                grid.selected_column,
            ));
            self.views.deactivate(grid.view_id);
        }
    }
    pub fn show(&mut self, current: &[u8], q: Option<&Context>) -> io::Result<()> {
        if self.current.is_none()
            && let Some((kind, group, row, column)) = self.saved
        {
            self.open(current, kind, group, q)?;
            let id = self.current.as_ref().unwrap().view_id;
            self.views.states[id].current_row = row;
            self.current.as_mut().unwrap().selected_column = column;
        }
        Ok(())
    }
    /// Menu evaluation and Click share Checked, but evaluation never mutates C.
    /// Confirmation is a real external modal return, required only if reached.
    #[allow(
        clippy::too_many_arguments,
        reason = "The menu boundary preserves command inputs and independent native flag outputs."
    )]
    pub fn menu(
        &mut self,
        current: &[u8],
        key: u32,
        event: u32,
        index: u32,
        flags: &mut u8,
        disabled: &mut u8,
        confirmation: Option<i32>,
    ) -> io::Result<()> {
        if event == 0x0d {
            if let Some(bit) = column_bit(key) {
                self.settings.toggle_bit(bit)?;
                self.queue.push_back(Queued {
                    event: 0x27,
                    argument: 0,
                    value: 0,
                });
            } else {
                match key {
                    0x2b => {
                        self.settings.fixed_name ^= 1;
                        self.queue.extend([
                            Queued {
                                event: 0x27,
                                argument: 0,
                                value: 0,
                            },
                            Queued {
                                event: 0x19,
                                argument: 0,
                                value: 0,
                            },
                        ]);
                    }
                    0x2c => {
                        let enable = self.settings.split == 0;
                        if !enable {
                            self.settings.split = 0;
                        }
                        // Both branches query with C18==0. Copy head before reuse.
                        for view in self.views.states.iter_mut().filter(|v| v.active) {
                            view.show_split = self.settings.split;
                        }
                        let answer = self
                            .views
                            .query(current, 0x42, u32::from(enable))?
                            .first()
                            .copied();
                        if enable {
                            self.settings.split = 1;
                        }
                        for view in self.views.states.iter_mut().filter(|v| v.active) {
                            view.show_split = self.settings.split;
                        }
                        if let Some(answer) = answer {
                            self.queue.push_back(Queued {
                                event: 0x27,
                                argument: 1,
                                value: answer[0],
                            });
                        }
                    }
                    0x2e => (),
                    0x36 => {
                        self.settings.font_percent = index;
                        self.font_changed = true;
                        self.invalidate();
                    }
                    0x37 => {
                        if index != self.settings.language {
                            let accepted = if index == 1 && self.settings.system_language != 1 {
                                confirmation.ok_or_else(|| {
                                    invalid(
                                        "Japanese language change needs native confirmation return",
                                    )
                                })? == 1
                            } else {
                                true
                            };
                            if accepted {
                                self.settings.language = index;
                                self.queue.push_back(Queued {
                                    event: 0x26,
                                    argument: 0,
                                    value: 0,
                                });
                            }
                        }
                    }
                    0x38 => {
                        self.settings.toolbar = u32::from(self.settings.toolbar == 0);
                        self.toolbar
                            .set_visible(self.settings.toolbar != 0, |_| Ok(()))?;
                    }
                    0x40 => {
                        self.settings.callsign_dialog =
                            u32::from(self.settings.callsign_dialog == 0);
                    }
                    0x41 => {
                        self.settings.auto_correct = u32::from(self.settings.auto_correct == 0);
                    }
                    _ => return Err(invalid("unknown native display command")),
                }
            }
        }
        self.settings.query_menu(key, index, flags, disabled)
    }
    /// Drain reads current NOW, not an enqueue-time image/mask snapshot.
    pub fn drain(&mut self, current: &[u8], q: Option<&Context>) -> io::Result<()> {
        while let Some(message) = self.queue.pop_front() {
            match message.event {
                0x26 => {
                    self.resource_language = self.settings.language;
                    self.resource_generation += 1;
                    self.invalidate();
                    self.show(current, q)?;
                }
                0x27 => {
                    self.invalidate();
                    self.show(current, q)?;
                    if message.argument != 0
                        && let Some(grid) = &self.current
                    {
                        self.views.states[grid.view_id].current_row = (message.value as i32)
                            .wrapping_add(self.views.states[grid.view_id].fixed_rows as i32);
                    }
                }
                0x19 => {
                    if let Some(grid) = &mut self.current {
                        grid.align_selection();
                    }
                }
                0x3c => {
                    if let Some(grid) = &mut self.current {
                        let mask = sql_mask(
                            current,
                            self.views.states[grid.view_id].node.unwrap_or(0),
                            &self.settings,
                            &mut self.views,
                            q,
                        )?;
                        if grid.sql_mask != mask {
                            //59B24 stores cache before59354 may fail/rebuild.
                            grid.sql_mask = mask;
                            let mut rebuilt = Grid::build(
                                current,
                                grid.view_id,
                                &self.settings,
                                &mut self.views,
                                self.metrics,
                                q,
                                Some(mask),
                            )?;
                            rebuilt.rebuilds = grid.rebuilds + 1;
                            *grid = rebuilt;
                        }
                    }
                }
                _ => return Err(invalid("unsupported display queue event")),
            }
        }
        Ok(())
    }
}
