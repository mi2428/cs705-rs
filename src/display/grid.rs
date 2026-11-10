//! Typed display cells and measured geometry. No radio codec lives here.

use super::*;
use crate::icf::{
    MemoryCallsign,
    record::{ChannelField, HistoryValue},
};

/// Explicit fixed-pitch measurement source, not fabricated GDI return values.
/// A native backend can supply its measured em/advance/line/DPI. This portable
/// consumer supports only that declared fixed-pitch model, not arbitrary fonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontMetrics {
    pub em: i32,
    pub advance: i32,
    pub line: i32,
    pub dpi: i32,
    pub padding: i32,
    pub explicit_height: i32,
}
impl FontMetrics {
    pub fn validate(self) -> io::Result<()> {
        if self.em <= 0 || self.advance <= 0 || self.line <= 0 || self.dpi <= 0 || self.padding < 0
        {
            return Err(invalid("invalid explicit fixed-pitch display metrics"));
        }
        Ok(())
    }
    fn height(self, percent: u32) -> io::Result<i32> {
        let points = if self.explicit_height != 0 {
            self.explicit_height
                .checked_abs()
                .ok_or_else(|| invalid("display font height overflow"))?
        } else {
            // 2F575 uses signed 32 multiply and truncating division, not clamp.
            (16_i32.wrapping_mul(percent as i32)) / 100
        };
        let height = i64::from(points) * i64::from(self.dpi) / 96;
        i32::try_from(height)
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| invalid("display font has no positive measurement source"))
    }
    fn measure(self, text: &[u8], height: i32) -> io::Result<(i32, i32)> {
        // A native paint clone is C-prefix; logical payload remains in the cell.
        let text = &text[..text.iter().position(|b| *b == 0).unwrap_or(text.len())];
        let mut width = 0;
        let mut lines = 0;
        for line in text.split(|b| *b == b'\n') {
            width = width.max(line.len());
            lines += 1;
        }
        let pixel = |units: i64| -> io::Result<i32> {
            let value = units
                .checked_mul(i64::from(height))
                .and_then(|n| n.checked_add(i64::from(self.em) - 1))
                .ok_or_else(|| invalid("display cell metric overflow"))?
                / i64::from(self.em);
            i32::try_from(value + 2 * i64::from(self.padding))
                .map_err(|_| invalid("display cell metric overflow"))
        };
        Ok((
            pixel(width as i64 * i64::from(self.advance))?,
            pixel(lines * i64::from(self.line))?,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub key: u32,
    pub text: Vec<u8>,
    pub width: i32,
    pub height: i32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub key: u32,
    pub width: i32,
    pub measured_width: i32,
    pub hidden: bool,
    pub boundary: bool,
    pub x: i32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub display_index: i32,
    pub typed_index: u32,
    pub side: u8,
    pub valid: bool,
    pub cells: Vec<Cell>,
    pub height: i32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub view_id: usize,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    pub sql_mask: u32,
    pub frozen_columns: usize,
    pub fixed_origin: usize,
    pub selected_column: usize,
    pub font_height: i32,
    pub language: u32,
    pub rebuilds: u64,
    rx: bool,
}

/// Portable typed projection used by Grid's resource-free geometry model.
/// Numeric enum/index bytes here are NOT native paint captions; native clients
/// must use channel_paint and the actual selector provider instead.
pub fn channel_cell(holder: &record::DecodedRecord, key: u32, row: u32) -> io::Result<Vec<u8>> {
    use ChannelField::*;
    if !matches!(key, 0xed | 0x3ae | 0x105..=0x117) {
        return Err(invalid("unknown native channel display key"));
    }
    if !holder.valid() {
        return Ok(Vec::new());
    }
    let number = |value: u32| value.to_string().into_bytes();
    Ok(match key {
        0xed => Vec::new(),
        0x3ae => number(row),
        0x105 => holder.name().to_vec(),
        0x106 => number(u32::from(holder.as_bytes()[0x25])),
        0x107 => number(holder.channel_field(FrequencyHz)),
        0x108 => number(holder.mode()),
        0x109 => number(holder.data()),
        0x10a => number(holder.filter()),
        0x10b => number(holder.channel_field(Duplex)),
        0x10c => number(holder.offset_hz()),
        0x10d => number(holder.channel_field(Tone)),
        0x10e => number(holder.tx_tone()),
        0x10f => number(holder.rx_tone()),
        0x110 => number(holder.channel_field(Dtcs)),
        0x111 => number(holder.channel_field(Polarity)),
        0x112 => number(holder.channel_field(DigitalSquelch)),
        0x113 => number(holder.channel_field(DigitalCode)),
        0x114 => holder.callsign(MemoryCallsign::Your).to_vec(),
        0x115 => holder.callsign(MemoryCallsign::Repeater1).to_vec(),
        0x116 => holder.callsign(MemoryCallsign::Repeater2).to_vec(),
        0x117 => number(holder.channel_field(Selection)),
        _ => unreachable!(),
    })
}

/// Caption/numeric body after the owning caller's valid/mode guards. Compact
/// callers can reuse their existing CompactChannel::display guard and pass its
/// Number here, without constructing a second holder/codec. DR126 passes its
/// actual typed code byte; its SQL gray state is not a text suppression gate.
/// `label` is the real resource selector provider (source,key,full DWORD index),
/// preserving explicit aliases including empty. No completed-caption packet.
pub fn channel_value_paint(
    key: u32,
    value: u32,
    live_q: Option<&Context>,
    mut label: impl FnMut(&[u8], u32, u32) -> io::Result<Vec<u8>>,
) -> io::Result<Vec<u8>> {
    use crate::icf::rows::native::{
        ToneField, digital_code_label, tone_choices, tone_numeric_label,
    };
    let source = match key {
        0x106 | 0x109 => b"GI_Sheet_Sel_Misc_OnOff2".as_slice(),
        0x108 => b"GI_Sheet_Sel_Mch_Mode",
        0x10a => b"GI_Sheet_Sel_Mch_Filter",
        0x10b => b"GI_Sheet_Sel_Mch_Dup",
        0x10d => b"GI_Sheet_Sel_Mch_TSQL",
        0x111 => b"GI_Sheet_Sel_Mch_DTCS_Polarity",
        0x112 => b"GI_Sheet_Sel_Mch_DSQL",
        0x117 => b"GI_Sheet_Sel_Mch_Scan_Sel",
        0x10e..=0x110 => {
            let field = match key {
                0x10e => ToneField::Tx,
                0x10f => ToneField::Rx,
                _ => ToneField::Dtcs,
            };
            if !tone_choices(field, live_q)
                .map_err(data_error)?
                .contains(&i64::from(value))
            {
                return Err(invalid(
                    "native tone index has no label in the actual source",
                ));
            }
            return tone_numeric_label(field, value)
                .ok_or_else(|| invalid("native numeric tone label unavailable"));
        }
        0x113 | 0x126 => return Ok(digital_code_label(value)),
        _ => return Err(invalid("unknown native channel choice/tone/code key")),
    };
    label(source, if key == 0x108 { 7 } else { 1 }, value)
}

/// Phase 0 normal/call/compact choice/tone/code text only, not an input gate or
/// a whole channel renderer. No gray flag prevents a native text lookup.
pub fn channel_paint(
    holder: &record::DecodedRecord,
    key: u32,
    live_q: Option<&Context>,
    label: impl FnMut(&[u8], u32, u32) -> io::Result<Vec<u8>>,
) -> io::Result<Vec<u8>> {
    use crate::icf::rows::native::{ToneField, channel_mode_blocked};
    use ChannelField::*;
    if !matches!(holder.kind(), 2..=5)
        || !matches!(key, 0x106 | 0x108..=0x10b | 0x10d..=0x113 | 0x117)
    {
        return Err(invalid(
            "native channel paint requires a supported holder and field",
        ));
    }
    if !holder.valid()
        || matches!(holder.kind(), 2 | 3) && holder.channel_field(FrequencyHz) > 470_000_000
    {
        return Ok(Vec::new());
    }
    let blocked = channel_mode_blocked(holder, key);
    if blocked && key != 0x10a {
        return Ok(b"---".to_vec());
    }
    let value = match key {
        0x106 => u32::from(holder.split()),
        0x108 => holder.mode(),
        0x109 => holder.data(),
        0x10a if blocked => 0,
        0x10a => holder.filter(),
        0x10b => holder.channel_field(Duplex),
        0x10d => ToneField::Tone.value(holder),
        0x10e => ToneField::Tx.value(holder),
        0x10f => ToneField::Rx.value(holder),
        0x110 => ToneField::Dtcs.value(holder),
        0x111 => holder.channel_field(Polarity),
        0x112 => holder.channel_field(DigitalSquelch),
        0x113 => holder.channel_field(DigitalCode),
        _ => holder.channel_field(Selection),
    };
    channel_value_paint(key, value, live_q, label)
}

fn rx_text(holder: &record::DecodedRecord, key: u32) -> Vec<u8> {
    if !holder.valid() {
        return Vec::new();
    }
    let name = match key {
        0x163 => "latitude",
        0x164 => "longitude",
        0x165 => "altitude-tenths-ft",
        0x166 => "ssid",
        0x167 => "symbol",
        0x168 => "course",
        0x169 => "speed",
        0x16a => "gps-timestamp",
        0x16b => "power",
        0x16c => "height",
        0x16d => "gain",
        0x16e => "directivity",
        0x16f => "object-name",
        0x170 => "data-type",
        0x171 => "temperature-f",
        0x172 => "rain",
        0x173 => "rain24",
        0x174 => "rain-midnight",
        0x175 => "wind-direction",
        0x176 => "wind-speed",
        0x177 => "gust-speed",
        0x178 => "pressure",
        0x179 => "humidity",
        0x17a => "gps-message",
        _ => return Vec::new(),
    };
    match record::receive_history_fields(holder)
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, value)| value)
    {
        Some(HistoryValue::Unsigned(value)) => value.to_string().into_bytes(),
        Some(HistoryValue::Signed(value)) => value.to_string().into_bytes(),
        Some(HistoryValue::Bytes(value)) => value.to_vec(),
        None => Vec::new(),
    }
}

impl Grid {
    pub(super) fn build(
        current: &[u8],
        id: usize,
        settings: &Settings,
        views: &mut Views,
        metrics: FontMetrics,
        q: Option<&Context>,
        cached_mask: Option<u32>,
    ) -> io::Result<Self> {
        let view = views.states[id].clone();
        let kind = view
            .record_type()
            .ok_or_else(|| invalid("native display factory has no record type"))?;
        if !matches!(kind, 2..=5 | 0x0e) {
            return Err(invalid(
                "native display supports memory/call/bandstack/memopad/Rx factories",
            ));
        }
        let rx = kind == 0x0e;
        let mask = if rx {
            0
        } else if let Some(mask) = cached_mask {
            mask
        } else {
            sql_mask(current, view.node.unwrap_or(0), settings, views, q)?
        };
        let height = metrics.height(settings.font_percent)?;
        let keys: Vec<u32> = if rx {
            (0x163..=0x17a).collect()
        } else {
            vec![
                0xed, 0x3ae, 0x105, 0x106, 0x107, 0x108, 0x109, 0x10a, 0x10b, 0x10c, 0x10d, 0x10e,
                0x10f, 0x110, 0x111, 0x112, 0x113, 0x114, 0x115, 0x116, 0x117,
            ]
        };
        let mut grid = Self {
            view_id: id,
            columns: keys
                .iter()
                .map(|&key| Column {
                    key,
                    width: 0,
                    measured_width: 0,
                    hidden: false,
                    boundary: key == 0x106,
                    x: 0,
                })
                .collect(),
            rows: Vec::new(),
            sql_mask: mask,
            frozen_columns: 0,
            fixed_origin: 0,
            selected_column: 0,
            font_height: height,
            language: settings.language,
            rebuilds: 1,
            rx,
        };
        let storage = if rx {
            Some(
                crate::icf::Icf::from_memory(
                    current
                        .get(..0x3c9c0)
                        .ok_or_else(|| invalid("Rx storage image unavailable"))?
                        .to_vec(),
                )
                .map_err(data_error)?,
            )
        } else {
            None
        };
        let range = match kind {
            2 => 0..100,
            3 => 0..4,
            4 => 0..45,
            5 => 1..11,
            _ => 0..50,
        };
        let mut holder = record::reset_holder(kind, 0, 0);
        for row in range {
            if kind == 4 {
                // Only_50M_Enable3 excludes explicit 27..29, never renumbers.
                if (27..30).contains(&row)
                    && q.ok_or_else(|| invalid("bandstack Only needs live Q"))?
                        .effective_f()
                        .map_err(data_error)?
                        == 0
                {
                    continue;
                }
            }
            let doubled = matches!(kind, 2 | 3);
            let index = if kind == 2 {
                view.group as u32 * 200 + row * 2
            } else if kind == 3 {
                row * 2
            } else {
                row
            };
            if rx {
                record::load_receive_history_storage_into(
                    storage.as_ref().unwrap(),
                    row,
                    &mut holder,
                )
                .map_err(data_error)?;
            } else if doubled {
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
            grid.push_row(&holder, index, row, 0, &keys, metrics)?;
            if doubled && (settings.split != 0 || holder.valid() && holder.as_bytes()[0x25] == 1) {
                record_image::load_channel_into(
                    current,
                    if kind == 2 {
                        record_image::ChannelKind::Normal
                    } else {
                        record_image::ChannelKind::Call
                    },
                    index + 1,
                    &mut holder,
                )
                .map_err(data_error)?;
                // 60BBE uses ==1, not the nonzero row-count predicate.
                if !(settings.split == 1 || holder.as_bytes()[0x25] == 1 && holder.valid()) {
                    holder.set_valid(false);
                }
                grid.push_row(&holder, index + 1, row, 1, &keys, metrics)?;
            }
        }
        grid.apply_columns(settings)?;
        Ok(grid)
    }
    fn push_row(
        &mut self,
        holder: &record::DecodedRecord,
        index: u32,
        row: u32,
        side: u8,
        keys: &[u32],
        metrics: FontMetrics,
    ) -> io::Result<()> {
        let mut cells = Vec::new();
        let mut height = 0;
        for (column, &key) in self.columns.iter_mut().zip(keys) {
            let text = if self.rx {
                rx_text(holder, key)
            } else {
                channel_cell(holder, key, row)?
            };
            let (width, h) = metrics.measure(&text, self.font_height)?;
            column.measured_width = column.measured_width.max(width);
            height = height.max(h);
            cells.push(Cell {
                key,
                text,
                width,
                height: h,
            });
        }
        self.rows.push(Row {
            display_index: self.rows.len() as i32,
            typed_index: index,
            side,
            valid: holder.valid(),
            cells,
            height,
        });
        Ok(())
    }
    pub(super) fn apply_columns(&mut self, settings: &Settings) -> io::Result<()> {
        let mut x = 0_i32;
        for column in &mut self.columns {
            column.hidden = column_hidden(column.key, self.rx, settings, self.sql_mask);
            column.width = if column.hidden {
                0
            } else {
                column.measured_width
            };
            column.x = x;
            x = x
                .checked_add(column.width)
                .ok_or_else(|| invalid("display grid width overflow"))?;
        }
        // The portable schema has two structural cells before Name. 31AF4
        // expands the fixed area until the first bit 10 boundary, not all cells.
        self.fixed_origin = if settings.fixed_name != 0 && !self.rx {
            2
        } else {
            0
        };
        self.frozen_columns = self.fixed_origin;
        if self.fixed_origin != 0 {
            while self.frozen_columns < self.columns.len()
                && !self.columns[self.frozen_columns].boundary
            {
                self.frozen_columns += 1;
            }
        }
        self.rebuilds += 1;
        Ok(())
    }
    pub(super) fn align_selection(&mut self) {
        // 36986 aligns to fixed_columns-extra while preserving the live row.
        self.selected_column = self.frozen_columns.saturating_sub(self.fixed_origin);
    }
    /// Scrolling applies only after the frozen area; hidden columns occupy 0.
    /// Widen before subtracting so every signed 32 scroll remains representable.
    pub fn column_positions(&self, horizontal_scroll: i32) -> Vec<i64> {
        self.columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if i < self.frozen_columns {
                    i64::from(c.x)
                } else {
                    i64::from(c.x) - i64::from(horizontal_scroll)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icf::{rows::native, settings::extended::NumberInput};

    fn state(padding: i32) -> State {
        let settings = Settings {
            language: 0,
            system_language: 0,
            font_percent: 100,
            toolbar: 0,
            split: 0,
            callsign_dialog: 1,
            auto_correct: 1,
            fixed_name: 0,
            column_bits: 15,
            column_bits_high: 0,
        };
        State::new(
            settings,
            FontMetrics {
                em: 16,
                advance: 8,
                line: 16,
                dpi: 96,
                padding,
                explicit_height: 0,
            },
            [0; 4],
        )
        .unwrap()
    }

    #[test]
    fn display_grid_rejects_total_width_overflow() {
        let mut state = state(300_000_000);
        assert!(
            state
                .open(&vec![0; 0x4997a], ViewKind::Call, 0, None)
                .is_err()
        );
    }

    #[test]
    fn display_scroll_preserves_the_full_signed_input_domain() {
        let mut state = state(1);
        state
            .open(&vec![0; 0x4997a], ViewKind::Call, 0, None)
            .unwrap();
        let grid = state.current.as_ref().unwrap();
        for scroll in [i32::MIN, 0, i32::MAX] {
            let positions = grid.column_positions(scroll);
            for (column, x) in grid.columns.iter().zip(positions) {
                assert_eq!(x, i64::from(column.x) - i64::from(scroll));
            }
        }
    }

    #[test]
    fn failed_display_build_retires_its_query_receiver() {
        let mut state = state(1);
        state.metrics.explicit_height = i32::MIN;
        assert!(
            state
                .open(&vec![0; 0x4997a], ViewKind::Call, 0, None)
                .is_err()
        );
        assert!(state.current.is_none());
        assert!(state.views.states.iter().all(|view| !view.active));
        assert!(state.views.query(&[], 0x3f, 0).unwrap().is_empty());
    }

    fn no_label(_: &[u8], _: u32, _: u32) -> io::Result<Vec<u8>> {
        panic!("this paint branch must not read a selector source")
    }

    #[test]
    fn native_channel_choices_tones_and_codes_keep_guards_and_sources_separate() {
        use ChannelField::*;
        let mut h = record::reset_holder(2, 0, 0);
        h.set_valid(false);
        let keys = [
            0x106, 0x108, 0x109, 0x10a, 0x10b, 0x10d, 0x10e, 0x10f, 0x110, 0x111, 0x112, 0x113,
            0x117,
        ];
        for key in keys {
            assert_eq!(channel_paint(&h, key, None, no_label).unwrap(), b"");
        }
        h.set_valid(true);
        h.set_channel_field(FrequencyHz, 470_000_001);
        for key in keys {
            assert_eq!(channel_paint(&h, key, None, no_label).unwrap(), b"");
        }
        h.set_channel_field(FrequencyHz, 470_000_000);
        for (key, source) in [
            (0x106, b"GI_Sheet_Sel_Misc_OnOff2".as_slice()),
            (0x108, b"GI_Sheet_Sel_Mch_Mode"),
            (0x109, b"GI_Sheet_Sel_Misc_OnOff2"),
            (0x10a, b"GI_Sheet_Sel_Mch_Filter"),
            (0x10b, b"GI_Sheet_Sel_Mch_Dup"),
            (0x10d, b"GI_Sheet_Sel_Mch_TSQL"),
            (0x111, b"GI_Sheet_Sel_Mch_DTCS_Polarity"),
            (0x112, b"GI_Sheet_Sel_Mch_DSQL"),
            (0x117, b"GI_Sheet_Sel_Mch_Scan_Sel"),
        ] {
            for value in match key {
                0x106 | 0x109 => vec![0, 1],
                0x108 => (0..10).map(record::decode_mode).collect(),
                0x10a => vec![0, 1, 2],
                0x10b => vec![0, 1, 2, 3, 4],
                0x10d => (0..8).map(record::decode_tone).collect(),
                0x111 => vec![2, 3, 4, 5],
                0x112 => vec![0, 2, 4],
                _ => vec![0, 1, 2, 3],
            } {
                h.set_mode(if key == 0x112 { 16 } else { 13 });
                match key {
                    0x106 => h.set_split(value as u8),
                    0x108 => h.set_mode(value),
                    0x109 => h.set_data(value),
                    0x10a => h.set_filter(value),
                    0x10b => h.set_channel_field(Duplex, value),
                    0x10d => h.set_channel_field(Tone, value),
                    0x111 => h.set_channel_field(Polarity, value),
                    0x112 => h.set_channel_field(DigitalSquelch, value),
                    _ => h.set_channel_field(Selection, value),
                }
                let mut calls = 0;
                let out = channel_paint(&h, key, None, |actual, selector, index| {
                    calls += 1;
                    assert_eq!(actual, source);
                    assert_eq!(selector, if key == 0x108 { 7 } else { 1 });
                    assert_eq!(index, value);
                    // A real registered empty alias must remain empty.
                    Ok(if value == 0 {
                        Vec::new()
                    } else {
                        b"source-alias".to_vec()
                    })
                })
                .unwrap();
                assert_eq!(calls, 1);
                assert_eq!(
                    out,
                    if value == 0 {
                        b"".as_slice()
                    } else {
                        b"source-alias"
                    }
                );
            }
        }
        h.set_mode(17);
        for key in [0x109, 0x10d, 0x10e, 0x10f, 0x110, 0x111, 0x112, 0x113] {
            assert_eq!(channel_paint(&h, key, None, no_label).unwrap(), b"---");
        }
        for mode in [15, 17, 0x10f, 0x111] {
            h.set_mode(mode);
            h.set_filter(2);
            assert_eq!(
                channel_paint(&h, 0x10a, None, |_, key, value| {
                    assert_eq!((key, value), (1, 0));
                    Ok(b"filter-zero".to_vec())
                })
                .unwrap(),
                b"filter-zero"
            );
        }
        h.set_mode(13);
        h.set_channel_field(Duplex, u32::MAX);
        assert!(
            channel_paint(&h, 0x10b, None, |_, _, index| {
                assert_eq!(index, u32::MAX);
                Err(invalid("missing original selector label"))
            })
            .is_err()
        );
        let q0 = Context::from_bytes([0; 24]);
        let mut raw_q = [0; 24];
        raw_q[0x12] = 255;
        let q1 = Context::from_bytes(raw_q);
        for key in [0x10e, 0x10f] {
            for (value, expected) in [(0, b"67.0".as_slice()), (49, b"254.1"), (50, b"150.0")] {
                if key == 0x10e {
                    h.set_tx_tone(value);
                } else {
                    h.set_rx_tone(value);
                }
                let before = h.clone();
                assert_eq!(
                    channel_paint(&h, key, Some(&q0), no_label).unwrap(),
                    expected
                );
                assert_eq!(h.as_bytes(), before.as_bytes());
                assert_eq!(
                    channel_paint(&h, key, Some(&q1), no_label).is_err(),
                    value == 50
                );
                assert!(channel_paint(&h, key, None, no_label).is_err());
            }
            assert!(channel_value_paint(key, 51, Some(&q0), no_label).is_err());
            assert!(channel_value_paint(key, 255, Some(&q1), no_label).is_err());
        }
        for (index, code) in crate::csv::table::DTCS_CODES.iter().enumerate() {
            h.set_channel_field(Dtcs, index as u32);
            assert_eq!(
                channel_paint(&h, 0x110, None, no_label).unwrap(),
                format!("{code:03}").into_bytes()
            );
        }
        assert!(channel_value_paint(0x110, 104, None, no_label).is_err());
        h.set_mode(16);
        h.set_channel_field(DigitalSquelch, 0);
        for (value, expected) in [
            (0, b"00".as_slice()),
            (9, b"09"),
            (99, b"99"),
            (100, b"100"),
            (255, b"255"),
            (355, b"355"),
            (u32::MAX, b"-1"),
            (0x80000000, b"-2147483648"),
        ] {
            h.set_channel_field(DigitalCode, value);
            assert_eq!(channel_paint(&h, 0x113, None, no_label).unwrap(), expected);
            assert_eq!(
                channel_value_paint(0x126, value, None, no_label).unwrap(),
                expected
            );
            let mut flags = 0;
            let out =
                native::channel_code_input(&mut h, NumberInput::Display, None, &mut flags).unwrap();
            assert_eq!(out.display, expected);
            assert!(!out.stored);
        }
        let mut raw = [0; 36];
        raw[..4].copy_from_slice(&145_000_000_u32.to_le_bytes());
        raw[4] = record::encode_mode(13) << 4;
        raw[7] = 50;
        raw[8] = 49;
        let compact = crate::icf::special::CompactChannel::decode(&raw).unwrap();
        use crate::icf::special::{CompactField as F, DisplayValue as V};
        for field in [F::TxTone, F::RxTone] {
            let V::Number(value) = compact.display(field) else {
                panic!("actual compact guard")
            };
            assert_eq!(
                channel_value_paint(u32::from(field.key()), value, Some(&q0), no_label).unwrap(),
                if field == F::TxTone {
                    b"150.0".as_slice()
                } else {
                    b"254.1"
                }
            );
        }
        let mut call = record::reset_holder(3, 0, 0);
        call.set_valid(true);
        call.set_channel_field(FrequencyHz, 145_000_000);
        call.set_mode(16);
        call.set_channel_field(DigitalCode, 9);
        assert_eq!(channel_paint(&call, 0x113, None, no_label).unwrap(), b"09");
        assert!(channel_paint(&h, 0x107, None, no_label).is_err());
        assert!(channel_value_paint(0x107, 1, None, no_label).is_err());
    }
}
