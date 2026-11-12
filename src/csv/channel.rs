//! Channel CSV parents. Runtime context numbers are not holder kinds or slots.
//! Shared byte grammar, text/NLS, field codecs and typed image stores stay sole
//! implementations. Reports expose mutations before a later failure, not undo.

use super::{
    Error, Locale, Reader,
    control::Counts,
    dispatch::{Binding, ImportKind},
    nls,
};
use crate::icf::{
    self, Icf, MemoryCallsign,
    record::{self, ChannelField, DecodedRecord},
    record_image,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Memory,
    FixedPair,
    AlternatingPair,
    Scan,
    Callsign,
    UserEdge,
}
impl Kind {
    pub fn from_context(context: u32) -> Result<Self, Error> {
        match context {
            2 => Ok(Self::Memory),
            3 => Ok(Self::FixedPair),
            6 => Ok(Self::AlternatingPair),
            8 => Ok(Self::Scan),
            13 => Ok(Self::Callsign),
            21 => Ok(Self::UserEdge),
            _ => Err(Error::Invalid(
                "channel CSV runtime context is not registered".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub first: u32,
    pub last: u32,
    pub number_base: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Text(Error),
    Image(icf::Error),
}
impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self::Text(error)
    }
}
impl From<icf::Error> for Failure {
    fn from(error: icf::Error) -> Self {
        Self::Image(error)
    }
}
#[derive(Debug)]
pub struct Import {
    pub counts: Counts,
    pub rows_read: usize,
    pub applied: usize,
    pub failure: Option<Failure>,
}
#[derive(Debug)]
pub struct Export {
    pub bytes: Vec<u8>,
    pub rows: usize,
    pub failure: Option<Failure>,
}

pub struct EdgeRow {
    pub holder: DecodedRecord,
    pub position: u32,
    pub discard: u32,
    pub corrections: u32,
}

const MEMORY_SIDE_LABELS: [&[u8]; 16] = [
    b"Frequency",
    b"Dup",
    b"Offset",
    b"Mode",
    b"DATA",
    b"Filter",
    b"TONE",
    b"Repeater Tone",
    b"TSQL Frequency",
    b"DTCS Code",
    b"DTCS Polarity",
    b"DV SQL",
    b"DV CSQL Code",
    b"Your Call Sign",
    b"RPT1 Call Sign",
    b"RPT2 Call Sign",
];

/// 8A3E8's cache and 80F80's whole/selected preparation. Plain No is cached but
/// does not replace CH No at 864954. This reader does not accept finalized rows.
pub struct MemoryRows<'a> {
    reader: Reader<'a>,
    binding: Binding,
    next: usize,
    kind: Kind,
    sequential: bool,
}
impl<'a> MemoryRows<'a> {
    pub fn new(source: &'a [u8], kind: Kind, locale: Locale, whole: bool) -> Result<Self, Error> {
        let import = match kind {
            Kind::Memory => ImportKind::Memory,
            Kind::FixedPair => ImportKind::FixedPair,
            _ => {
                return Err(Error::Invalid(
                    "memory reader requires normal or Call context".into(),
                ));
            }
        };
        let reader = Reader::new(source, locale)?;
        let mut labels = vec![
            b"CH No".to_vec(),
            b"No".to_vec(),
            b"Group No".to_vec(),
            b"Group Name".to_vec(),
            b"Name".to_vec(),
            b"SEL".to_vec(),
            b"Split".to_vec(),
        ];
        labels.extend(MEMORY_SIDE_LABELS.iter().map(|s| s.to_vec()));
        labels.extend(MEMORY_SIDE_LABELS.iter().map(|&s| [s, b"(2)"].concat()));
        let binding = Binding::builtin(
            reader.header(),
            locale.list,
            &labels.iter().map(|s| s.as_slice()).collect::<Vec<_>>(),
            import,
        )?;
        if !binding.accepted {
            return Err(Error::Invalid("memory CSV requires Frequency".into()));
        }
        let mut sequential = binding.positions[0].is_none();
        if !sequential && whole && kind == Kind::Memory && binding.positions[2].is_some() {
            // 80BC0 compares all Group No cells before clear. Different numeric
            // spellings of one group are equal; invalid text is a separate key.
            let mut first = None;
            for &line in reader.rows() {
                let text = binding.cell(line, 2)?.expect("present Group No header");
                let value = memory_group(text)?;
                let key = (
                    value,
                    if value == 0xffff {
                        text.iter().map(u8::to_ascii_uppercase).collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    },
                );
                if first.as_ref().is_some_and(|first| first != &key) {
                    sequential = true;
                    break;
                }
                first = Some(key);
            }
        }
        Ok(Self {
            reader,
            binding,
            next: 0,
            kind,
            sequential,
        })
    }
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn sequential(&self) -> bool {
        self.sequential
    }
    pub fn next_source(&mut self) -> Option<&'a [u8]> {
        let line = *self.reader.rows().get(self.next)?;
        self.next += 1;
        Some(line)
    }
}

fn memory_group(source: &[u8]) -> Result<u32, Error> {
    match super::number::int32(super::trim(source)) {
        Ok(value) if value >= 0 => Ok(value as u32),
        Ok(_) | Err(Error::Conversion(_)) => Ok(0xffff),
        Err(error) => Err(error),
    }
}

/// 805B0's actual default source, not an externally completed holder. Normal
/// uses the captured B class/type 1D; Call reads G at its parser-local No index.
/// A lower Call has no proven No assignment, so that source stays unavailable.
pub fn memory_default(
    kind: Kind,
    position: Option<u32>,
    frequency: u32,
    generation: &[u8],
    ranges: &icf::frequency::Ranges,
    system_langid: Option<u16>,
) -> Result<DecodedRecord, Failure> {
    if !ranges.is_complete() {
        return Err(
            Error::SourceUnavailable("805B0 actual completed B source unavailable".into()).into(),
        );
    }
    let mut holder = record::reset_holder(0x1d, 0, 0);
    match kind {
        Kind::Memory => {
            record::initial_normal_channel_into(
                generation,
                ranges.edf0(2, frequency) as u32,
                ranges.default_source_selector(system_langid)?,
                &mut holder,
            )?;
        }
        Kind::FixedPair => {
            let index = position
                .ok_or(Error::SourceUnavailable(
                    "Call lower parser-local No is unwritten".into(),
                ))?
                .wrapping_sub(1);
            record_image::load_into(generation, 3, index, &mut holder)?;
        }
        _ => {
            return Err(Error::Invalid(
                "memory initializer requires normal or Call context".into(),
            )
            .into());
        }
    }
    if matches!(holder.mode(), 0 | 1) {
        holder.set_mode(u32::from(frequency >= 10_000_000));
    }
    Ok(holder)
}

pub struct MemoryOptions<'a> {
    pub locale: Locale,
    pub crt_decimal: u8,
    pub generation: &'a [u8],
    pub ranges: Option<&'a icf::frequency::Ranges>,
    pub live_q: &'a icf::context::Context,
    pub system_langid: Option<u16>,
}
impl MemoryOptions<'_> {
    fn ranges(&self) -> Result<&icf::frequency::Ranges, Error> {
        self.ranges
            .filter(|ranges| ranges.is_complete())
            .ok_or(Error::SourceUnavailable(
                "CSV actual completed B range source unavailable".into(),
            ))
    }
}

impl MemoryRows<'_> {
    fn side(
        &self,
        line: &[u8],
        cursor: u32,
        other: bool,
        old_split: u8,
        options: &MemoryOptions<'_>,
    ) -> Result<EdgeRow, Failure> {
        let (code, base, limit) = if self.kind == Kind::Memory {
            (2, 0, 100)
        } else {
            (3, 1, 4)
        };
        let position = if other {
            None
        } else if self.sequential {
            Some(cursor)
        } else {
            Some(match self.binding.cell(line, 0)? {
                Some(text) => match super::number::int32(super::trim(text)) {
                    Ok(n) => n as u32,
                    Err(Error::Conversion(_)) => 0xffff,
                    Err(error) => return Err(error.into()),
                },
                None => 0xffff,
            })
        };
        let mut row = EdgeRow {
            holder: record::reset_holder(code, cursor, 0),
            position: position.unwrap_or(0xffff),
            discard: 0,
            corrections: 0,
        };
        row.holder.as_mut_bytes()[0x24] = u8::from(other);
        row.holder.set_split(old_split);
        row.holder.set_valid(false);
        if !other
            && !self.sequential
            && ((row.position as i32) < base || (row.position as i32) >= base + limit)
        {
            return Ok(row);
        }
        let side = 7 + 16 * usize::from(other);
        let cell = |field| self.binding.cell(line, side + field);
        let fail = |row: &mut EdgeRow| {
            row.holder.set_valid(false);
            if self.kind == Kind::FixedPair && !other {
                row.corrections = row.corrections.wrapping_add(1);
            } else {
                row.discard = row.discard.wrapping_add(1);
            }
        };
        let mut default = None;
        if let Some(text) = cell(0)? {
            if text.is_empty() {
                if other && old_split == 1 {
                    fail(&mut row);
                }
                return Ok(row);
            }
            let hz = super::number::frequency_hz(text, self.reader.locale().decimal)?;
            if !(30_000..=470_000_000).contains(&hz) {
                fail(&mut row);
                return Ok(row);
            }
            let seed = memory_default(
                self.kind,
                position,
                hz,
                options.generation,
                options.ranges()?,
                options.system_langid,
            )?;
            row.holder.copy_body_from(&seed);
            row.holder.set_channel_field(ChannelField::FrequencyHz, hz);
            row.holder.set_valid(true);
            default = Some(seed);
        }
        if let Some(text) = cell(3)?
            && !text.is_empty()
        {
            let mode = super::table::portable_import(
                super::payload::Adapter::Mode,
                text,
                self.reader.locale().decimal,
                options.crt_decimal,
                &mut row.corrections,
            )? as u8;
            if matches!(mode, 25 | 255) {
                fail(&mut row);
                return Ok(row);
            }
            row.holder.set_mode(u32::from(mode));
        }
        // 10194 calls the real C94B8 predicate. 10144 itself is constant false;
        // only valid 1/full mode 24 is unsuitable, with no UI current-value bypass.
        if row.holder.valid() && row.holder.mode() == 24 {
            row.holder.set_mode(
                default
                    .as_ref()
                    .ok_or(Error::SourceUnavailable(
                        "mode repair lacks actual frequency initializer".into(),
                    ))?
                    .mode(),
            );
            row.corrections = row.corrections.wrapping_add(1);
        }
        if let Some(text) = cell(4)?
            && !text.is_empty()
        {
            row.holder.set_data(super::table::portable_import(
                super::payload::Adapter::Data,
                text,
                self.reader.locale().decimal,
                options.crt_decimal,
                &mut row.corrections,
            )?);
        }
        if row.holder.data() == 0xffff
            || (!matches!(row.holder.mode(), 0 | 1 | 8 | 13) && row.holder.data() != 0)
        {
            row.holder.set_data(
                default
                    .as_ref()
                    .ok_or(Error::SourceUnavailable(
                        "DATA repair lacks actual frequency initializer".into(),
                    ))?
                    .data(),
            );
            row.corrections = row.corrections.wrapping_add(1);
        }
        icf::rows::native::default_filter_into(
            options.generation,
            options.ranges()?,
            &mut row.holder,
        )?;
        if !other {
            if let Some(text) = self.binding.cell(line, 4)? {
                let (name, clipped) = super::fixed_text::<16>(text);
                row.holder.set_name(&name);
                row.corrections = row.corrections.wrapping_add(u32::from(clipped));
            }
            if self.kind == Kind::Memory
                && let Some(text) = self.binding.cell(line, 5)?
                && !text.is_empty()
            {
                row.holder.set_channel_field(
                    ChannelField::Selection,
                    super::table::portable_import(
                        super::payload::Adapter::Selection,
                        text,
                        self.reader.locale().decimal,
                        options.crt_decimal,
                        &mut row.corrections,
                    )?,
                );
            }
            if let Some(text) = self.binding.cell(line, 6)?
                && !text.is_empty()
            {
                row.holder.set_split(super::table::portable_import(
                    super::payload::Adapter::Split,
                    text,
                    self.reader.locale().decimal,
                    options.crt_decimal,
                    &mut row.corrections,
                )? as u8);
            }
        }
        if let Some(text) = cell(1)?
            && !text.is_empty()
        {
            row.holder.set_channel_field(
                ChannelField::Duplex,
                super::table::portable_import(
                    super::payload::Adapter::Duplex,
                    text,
                    self.reader.locale().decimal,
                    options.crt_decimal,
                    &mut row.corrections,
                )?,
            );
        }
        if let Some(text) = cell(2)?
            && !text.is_empty()
        {
            row.holder.set_offset_hz(super::number::offset_hz(
                text,
                self.reader.locale().decimal,
            )?);
        }
        if let Some(text) = cell(5)?
            && !text.is_empty()
        {
            let filter = super::table::portable_import(
                super::payload::Adapter::Filter,
                text,
                self.reader.locale().decimal,
                options.crt_decimal,
                &mut row.corrections,
            )?;
            row.holder
                .set_filter(if filter == 255 { 0xffff } else { filter });
        }
        if row.holder.mode() == 13 {
            for column in 6..=10 {
                if let Some(text) = cell(column)?
                    && !text.is_empty()
                {
                    if matches!(column, 7 | 8) {
                        let value = super::table::tone_builtin_ascii(
                            text,
                            options.live_q.as_bytes()[0x12] == 0,
                            options.crt_decimal,
                            &mut row.corrections,
                        )?;
                        if column == 7 {
                            row.holder.set_tx_tone(value);
                        } else {
                            row.holder.set_rx_tone(value);
                        }
                    } else {
                        let (adapter, field) = match column {
                            6 => (super::payload::Adapter::Tone, ChannelField::Tone),
                            9 => (super::payload::Adapter::Dtcs, ChannelField::Dtcs),
                            _ => (
                                super::payload::Adapter::DtcsPolarity,
                                ChannelField::Polarity,
                            ),
                        };
                        let value = super::table::portable_import(
                            adapter,
                            text,
                            self.reader.locale().decimal,
                            options.crt_decimal,
                            &mut row.corrections,
                        )?;
                        row.holder.set_channel_field(field, value);
                    }
                }
            }
        }
        if row.holder.mode() == 16 {
            if let Some(text) = cell(11)?
                && !text.is_empty()
            {
                row.holder.set_channel_field(
                    ChannelField::DigitalSquelch,
                    super::table::portable_import(
                        super::payload::Adapter::DigitalSquelch,
                        text,
                        self.reader.locale().decimal,
                        options.crt_decimal,
                        &mut row.corrections,
                    )?,
                );
            }
            if let Some(text) = cell(12)?
                && !text.is_empty()
            {
                let value = match super::number::int32(super::trim(text)) {
                    Ok(n) => n as u32,
                    Err(Error::Conversion(_)) => 0xffff,
                    Err(error) => return Err(error.into()),
                };
                row.holder
                    .set_channel_field(ChannelField::DigitalCode, value);
            }
            for (column, which) in [
                (13, MemoryCallsign::Your),
                (14, MemoryCallsign::Repeater1),
                (15, MemoryCallsign::Repeater2),
            ] {
                if let Some(text) = cell(column)? {
                    let (call, clipped) =
                        nls::callsign_fixed(&mut text.to_vec(), nls::platform_map)?;
                    row.holder.set_callsign(which, &call);
                    row.corrections = row.corrections.wrapping_add(u32::from(clipped));
                }
            }
        }
        finalize_memory(&mut row, self.kind, position, options)?;
        Ok(row)
    }
}

fn finalize_memory(
    row: &mut EdgeRow,
    kind: Kind,
    position: Option<u32>,
    options: &MemoryOptions<'_>,
) -> Result<(), Failure> {
    let mut default = memory_default(
        kind,
        position,
        row.holder.channel_field(ChannelField::FrequencyHz),
        options.generation,
        options.ranges()?,
        options.system_langid,
    )?;
    icf::rows::native::default_filter_into(options.generation, options.ranges()?, &mut default)?;
    let h = &mut row.holder;
    let mut changed = row.corrections;
    let mut name = *h.name();
    changed = changed.wrapping_add(u32::from(nls::sanitize(&mut name, b' ')));
    h.set_name(&name);
    if kind == Kind::Memory && h.channel_field(ChannelField::Selection) == 0xffff {
        h.set_channel_field(
            ChannelField::Selection,
            default.channel_field(ChannelField::Selection),
        );
        changed = changed.wrapping_add(1);
    }
    if h.split() == 255 {
        h.set_split(default.split());
        changed = changed.wrapping_add(1);
    }
    let duplex = h.channel_field(ChannelField::Duplex);
    if duplex == 0xffff {
        h.set_channel_field(
            ChannelField::Duplex,
            default.channel_field(ChannelField::Duplex),
        );
        changed = changed.wrapping_add(1);
    } else if (h.mode() == 17 && matches!(duplex, 1 | 2))
        || (h.mode() != 17 && matches!(duplex, 3 | 4))
    {
        h.set_channel_field(ChannelField::Duplex, 0);
        changed = changed.wrapping_add(1);
    }
    if h.offset_hz() > 9_999_900 {
        h.set_offset_hz(default.offset_hz());
        changed = changed.wrapping_add(1);
    } else {
        let offset = icf::normalizer::round_offset_hz(h.offset_hz());
        changed = changed.wrapping_add(u32::from(offset != h.offset_hz()));
        h.set_offset_hz(offset);
    }
    if h.filter() == 0xffff || (matches!(h.mode() as u8, 15 | 17) && h.filter() != 0) {
        h.set_filter(default.filter());
        changed = changed.wrapping_add(1);
    }
    for field in [
        ChannelField::Tone,
        ChannelField::Dtcs,
        ChannelField::Polarity,
        ChannelField::DigitalSquelch,
    ] {
        if h.channel_field(field) == 0xffff {
            h.set_channel_field(field, default.channel_field(field));
            changed = changed.wrapping_add(1);
        }
    }
    let mut tx = h.tx_tone();
    let mut rx = h.rx_tone();
    for (tone, fallback) in [(&mut tx, default.tx_tone()), (&mut rx, default.rx_tone())] {
        if *tone == 0xffff {
            *tone = fallback;
            changed = changed.wrapping_add(1);
        }
        if options.live_q.as_bytes()[0x12] != 0 && *tone == 50 {
            *tone = 24;
            changed = changed.wrapping_add(1);
        }
    }
    h.set_tx_tone(tx);
    h.set_rx_tone(rx);
    if h.channel_field(ChannelField::DigitalCode) > 99 {
        h.set_channel_field(
            ChannelField::DigitalCode,
            default.channel_field(ChannelField::DigitalCode),
        );
        changed = changed.wrapping_add(1);
    }
    for which in [
        MemoryCallsign::Your,
        MemoryCallsign::Repeater2,
        MemoryCallsign::Repeater1,
    ] {
        let mut call = *h.callsign(which);
        changed = changed.wrapping_add(u32::from(nls::fold_callsign(&mut call)));
        h.set_callsign(which, &call);
    }
    if h.callsign(MemoryCallsign::Repeater1)[7] == b'G'
        && h.callsign(MemoryCallsign::Repeater2) != b"        "
    {
        let mut call = *h.callsign(MemoryCallsign::Repeater1);
        call[7] = b' ';
        h.set_callsign(MemoryCallsign::Repeater1, &call);
        changed = changed.wrapping_add(1);
    }
    row.corrections = row.corrections.wrapping_add(u32::from(changed != 0));
    Ok(())
}

pub struct MemoryImport {
    pub report: Import,
    /// Invalidation requests, not invented initial cache bytes or GUI delivery.
    pub home_invalidated: bool,
    pub call_invalidated: bool,
}

fn memory_put(image: &mut [u8], at: usize, bytes: &[u8]) -> Result<(), icf::Error> {
    image
        .get_mut(
            at..at.checked_add(bytes.len()).ok_or(icf::Error::Invalid {
                line: 0,
                reason: "CSV parent metadata overflow".into(),
            })?,
        )
        .ok_or(icf::Error::Invalid {
            line: 0,
            reason: "CSV parent metadata unavailable".into(),
        })?
        .copy_from_slice(bytes);
    Ok(())
}

/// 82BD4/82818. Selection is a logical CSV row range for the BEFORE-READ clear,
/// not an extra rejection predicate for otherwise valid numbered input rows.
/// Applied counts real upper/provisional-lower/final-lower saves, excluding clear.
pub fn import_memory(
    image: &mut Icf<'_>,
    source: &[u8],
    kind: Kind,
    group: Option<usize>,
    selection: Option<Selection>,
    options: &MemoryOptions<'_>,
) -> MemoryImport {
    let mut out = MemoryImport {
        report: Import {
            counts: Counts::default(),
            rows_read: 0,
            applied: 0,
            failure: None,
        },
        home_invalidated: false,
        call_invalidated: false,
    };
    let mut working = None;
    let operation = (|| -> Result<(), Failure> {
        let normal = kind == Kind::Memory;
        let (limit, base) = match kind {
            Kind::Memory => (100, 0),
            Kind::FixedPair if group.is_none() => (4, 1),
            _ => {
                return Err(Error::Invalid(
                    "memory import requires normal or ungrouped Call context".into(),
                )
                .into());
            }
        };
        if normal && group.is_none() && selection.is_some() {
            return Err(Error::Invalid("selected normal CSV rows require a group".into()).into());
        }
        let selection = selection.unwrap_or(Selection {
            first: 0,
            last: limit - 1,
            number_base: base,
        });
        if selection.number_base != base {
            return Err(
                Error::Invalid("memory CSV number base differs from runtime kind".into()).into(),
            );
        }
        edge_selection(selection, limit)?;
        let whole = normal && group.is_none();
        // Header and complete preparation precede every map/name/default store.
        let mut reader = MemoryRows::new(source, kind, options.locale, whole)?;
        let (bytes, groups) = if normal {
            let mut expanded = image.memory().to_vec();
            expanded.resize(0x4997a, 0);
            icf::rows::expand_map(&mut expanded)?;
            icf::rows::expand_names(&mut expanded)?;
            let count = u32::from_le_bytes(
                expanded[0x3fb40..0x3fb44]
                    .try_into()
                    .expect("expanded group count"),
            ) as usize;
            let groups = match group {
                Some(g) if g < 100 => g..g + 1,
                None => 0..count,
                _ => return Err(Error::Invalid("normal group outside0..100".into()).into()),
            };
            (expanded, groups)
        } else {
            (image.memory().to_vec(), 100..101)
        };
        working = Some(bytes);
        let w = working.as_mut().expect("actual working source initialized");
        let mut home = 0;
        let clear = (|| -> Result<(), Failure> {
            if whole {
                memory_put(w, 0x3c84b, &[255])?;
                home = 1;
            }
            if normal {
                for g in groups.clone() {
                    icf::rows::release_into(
                        w,
                        g,
                        selection.first as usize..selection.last as usize + 1,
                        icf::normalizer::ReferenceImage::Separate(options.generation),
                        &mut home,
                    )?;
                    memory_put(w, 0x355c3 + 19 * g, &[b' '; 16])?;
                    icf::rows::reset_home_into(
                        w,
                        g,
                        icf::normalizer::ReferenceImage::Separate(options.generation),
                        &mut home,
                    )?;
                }
            } else {
                for index in 2 * selection.first..=2 * selection.last + 1 {
                    let mut holder = record::reset_holder(3, index, 0);
                    record_image::load_into(options.generation, 3, index, &mut holder)?;
                    record_image::store_channel_into(
                        w,
                        record_image::ChannelKind::Call,
                        index,
                        &mut holder,
                        |_| Ok(()),
                    )?;
                }
                home = u8::from(w[0x3c84b] == 100);
                out.call_invalidated = true;
            }
            Ok(())
        })();
        out.home_invalidated = home != 0;
        clear?;
        let auto_group = whole && reader.binding.positions[2].is_none();
        let mut current_group = group.unwrap_or(0);
        let mut cursor = [base; 100];
        let mut call_cursor = base;
        let mut names: [Option<[u8; 16]>; 100] = [None; 100];
        let mut max_group = None;
        while let Some(line) = reader.next_source() {
            out.report.rows_read += 1;
            let g = if !normal {
                100
            } else if auto_group {
                current_group
            } else if whole {
                let Some(text) = reader.binding.cell(line, 2)? else {
                    unreachable!("whole fixed-group header is present")
                };
                let value = memory_group(text)?;
                if value >= 100 {
                    out.report.counts.c2 = out.report.counts.c2.wrapping_add(1);
                    continue;
                }
                value as usize
            } else {
                group.expect("selected normal group")
            };
            let next = if normal { cursor[g] } else { call_cursor };
            let mut upper = reader.side(line, next, false, 0, options)?;
            let relative =
                if upper.position != 0xffff && (normal || (upper.position as i32) >= base as i32) {
                    upper.position.wrapping_sub(base)
                } else {
                    upper.position
                };
            if (relative as i32) >= limit as i32 {
                if reader.sequential {
                    let capacity = if whole { 500 } else { limit };
                    if out.report.counts.c4 >= capacity {
                        out.report.counts.s = 2;
                        break;
                    }
                    out.report.counts.s = 3;
                } else if relative == 0xffff {
                    out.report.counts.c2 = out.report.counts.c2.wrapping_add(1);
                }
                continue;
            }
            if normal && let Some(text) = reader.binding.cell(line, 3)? {
                let (raw, clipped) = super::fixed_text::<16>(text);
                let name = super::export_call(&raw, 16);
                if names[g].is_none() && !name.is_empty() {
                    let mut fixed = [b' '; 16];
                    fixed[..name.len()].copy_from_slice(&name);
                    names[g] = Some(fixed);
                    out.report.counts.c3 = out.report.counts.c3.wrapping_add(u32::from(clipped));
                }
            }
            if !upper.holder.valid() {
                out.report.counts.c2 = out
                    .report
                    .counts
                    .c2
                    .wrapping_add(u32::from(upper.discard != 0));
                if !normal {
                    out.report.counts.c3 = out.report.counts.c3.wrapping_add(upper.corrections);
                }
                continue;
            }
            let index = if normal {
                200 * g as u32 + 2 * relative
            } else {
                2 * relative
            };
            if normal
                && record_image::channel_address(w, record_image::ChannelKind::Normal, index)?
                    .is_none()
            {
                let allocated =
                    icf::rows::allocate_into(w, g, relative as usize..relative as usize + 1)?
                        .allocated;
                if allocated != 1 {
                    let capacity = if whole { 500 } else { 100 };
                    out.report
                        .counts
                        .capacity_marker(if out.report.counts.c4 >= capacity {
                            2
                        } else {
                            1
                        });
                    break;
                }
            }
            let dataset = if normal {
                record_image::ChannelKind::Normal
            } else {
                record_image::ChannelKind::Call
            };
            record_image::store_channel_into(w, dataset, index, &mut upper.holder, |_| Ok(()))?;
            out.report.applied += 1;
            if normal {
                max_group = Some(max_group.map_or(g, |old: usize| old.max(g)));
            }
            let split = upper.holder.split();
            upper.holder.set_split(0);
            record_image::store_channel_into(w, dataset, index + 1, &mut upper.holder, |_| Ok(()))?;
            out.report.applied += 1;
            // The provisional saver can correct this snapshot before lower parse.
            let snapshot = upper.holder.clone();
            let mut lower = reader.side(line, next, true, split, options)?;
            if lower.holder.valid() {
                lower.holder.set_name(snapshot.name());
                lower.holder.set_split(split);
                lower.holder.set_channel_field(
                    ChannelField::Selection,
                    snapshot.channel_field(ChannelField::Selection),
                );
                record_image::store_channel_into(w, dataset, index + 1, &mut lower.holder, |_| {
                    Ok(())
                })?;
                out.report.applied += 1;
            }
            upper.corrections = super::control::pair_correction(
                upper.corrections,
                lower.holder.valid(),
                lower.discard,
                lower.corrections,
                split != 0,
            );
            out.report.counts.c2 = out.report.counts.c2.wrapping_add(upper.discard);
            out.report.counts.c3 = out.report.counts.c3.wrapping_add(upper.corrections);
            out.report.counts.c4 = out.report.counts.c4.wrapping_add(1);
            if normal {
                cursor[g] = cursor[g].wrapping_add(1);
                if auto_group && reader.sequential && cursor[g] >= 100 {
                    current_group += 1;
                }
            } else {
                call_cursor = call_cursor.wrapping_add(1);
            }
        }
        if normal {
            if whole {
                memory_put(
                    w,
                    0x3fb40,
                    &(max_group.map_or(0, |g| g as u32 + 1)).to_le_bytes(),
                )?;
            }
            let end = if whole {
                max_group.map_or(0, |g| g + 1)
            } else {
                groups.end
            };
            for (g, name) in names.iter().enumerate().take(end).skip(groups.start) {
                memory_put(w, 0x355c3 + 19 * g, &name.unwrap_or([b' '; 16]))?;
            }
        }
        for g in groups {
            icf::rows::native::reference_into(w, 0xd7, g, 0, limit as usize - 1, 0)?;
        }
        Ok(())
    })();
    let publish = (|| -> Result<(), Failure> {
        if let Some(w) = &mut working {
            let rebuilt = if kind == Kind::Memory {
                icf::rows::rebuild_metadata(w)
            } else {
                Ok(())
            };
            image.patch_memory(&[(0, &w[..image.memory().len()])])?;
            rebuilt?;
        }
        Ok(())
    })();
    if let Err(error) = operation.and(publish) {
        out.report.failure = Some(error);
    }
    out
}

pub struct UserEdgeRows<'a> {
    reader: Reader<'a>,
    binding: Binding,
    next: usize,
    selection: Selection,
    sequential: bool,
    cursor: u32,
}
impl<'a> UserEdgeRows<'a> {
    /// 88AE4 receives whole byte from 89088. Only whole plus missing No uses
    /// sequential positions; selected import must retain its numeric No field.
    pub fn new(
        source: &'a [u8],
        locale: Locale,
        selection: Selection,
        whole: bool,
    ) -> Result<Self, Error> {
        let reader = Reader::new(source, locale)?;
        let binding = Binding::builtin(
            reader.header(),
            locale.list,
            &[b"No", b"Frequency Low", b"Frequency High"],
            ImportKind::UserEdge,
        )?;
        if !binding.accepted {
            return Err(Error::Invalid(
                "user-edge CSV requires Frequency Low and Frequency High".into(),
            ));
        }
        let sequential = whole && binding.positions[0].is_none();
        Ok(Self {
            reader,
            binding,
            next: 0,
            selection,
            sequential,
            cursor: selection.number_base,
        })
    }
    /// 945A4/88B08: out-of-range positions keep zero local counts. A present
    /// empty low exits before high; empty high clears low's already-set valid.
    /// No ordering/suitability predicate is called by this CSV finalizer.
    pub fn next_row(&mut self) -> Option<Result<EdgeRow, Error>> {
        let line = *self.reader.rows().get(self.next)?;
        self.next += 1;
        Some((|| {
            let position = if self.sequential {
                self.cursor
            } else {
                match self.binding.cell(line, 0)? {
                    Some(text) => match super::number::int32(super::trim(text)) {
                        Ok(n) => n as u32,
                        Err(Error::Conversion(_)) => 0xffff,
                        Err(error) => return Err(error),
                    },
                    None => 0xffff,
                }
            };
            let mut row = EdgeRow {
                holder: record::reset_holder(0x15, position, 0),
                position,
                discard: 0,
                corrections: 0,
            };
            let low = self
                .selection
                .number_base
                .wrapping_add(self.selection.first) as i32;
            let high = self.selection.number_base.wrapping_add(self.selection.last) as i32;
            if !self.sequential && ((position as i32) < low || (position as i32) > high) {
                return Ok(row);
            }
            row.holder.set_valid(false);
            for field in [1, 2] {
                let Some(text) = self.binding.cell(line, field)? else {
                    continue;
                };
                if text.is_empty() {
                    if field == 2 {
                        row.holder.set_valid(false);
                    }
                    return Ok(row);
                }
                let hz = super::number::frequency_hz(text, self.reader.locale().decimal)?;
                if !(30_000..=470_000_000).contains(&hz) {
                    row.holder.set_valid(false);
                    row.discard = 1;
                    return Ok(row);
                }
                if field == 1 {
                    row.holder.as_mut_bytes()[0x138..0x13c].copy_from_slice(&hz.to_le_bytes());
                } else {
                    row.holder.set_channel_field(ChannelField::FrequencyHz, hz);
                }
                row.holder.set_valid(true);
            }
            Ok(row)
        })())
    }
    fn applied(&mut self) {
        self.cursor = self.cursor.wrapping_add(1);
    }
}

/// 8C4D4's value/finalizer phase. The caller supplies the actual shared type 1F
/// code initializer, not a CSV row packet or an inferred G image.
pub struct ScanRows<'a> {
    reader: Reader<'a>,
    binding: Binding,
    next: usize,
    selection: Selection,
    sequential: bool,
    cursor: u32,
}
impl<'a> ScanRows<'a> {
    pub fn new(
        source: &'a [u8],
        locale: Locale,
        selection: Selection,
        whole: bool,
    ) -> Result<Self, Error> {
        let reader = Reader::new(source, locale)?;
        let binding = Binding::builtin(
            reader.header(),
            locale.list,
            &[
                b"No",
                b"Name",
                b"Frequency Low",
                b"Frequency High",
                b"Mode",
                b"DATA",
                b"Filter",
            ],
            ImportKind::Scan,
        )?;
        if !binding.accepted {
            return Err(Error::Invalid(
                "scan CSV requires Frequency Low and Frequency High".into(),
            ));
        }
        let sequential = whole && binding.positions[0].is_none();
        Ok(Self {
            reader,
            binding,
            next: 0,
            selection,
            sequential,
            cursor: selection.number_base,
        })
    }
    pub fn next_with(
        &mut self,
        default: &DecodedRecord,
        crt_decimal: u8,
    ) -> Option<Result<EdgeRow, Error>> {
        let line = *self.reader.rows().get(self.next)?;
        self.next += 1;
        Some((|| {
            if default.kind() != 0x1f {
                return Err(Error::Invalid(
                    "scan parser requires the actual type1F initializer".into(),
                ));
            }
            let position = if self.sequential {
                self.cursor
            } else {
                match self.binding.cell(line, 0)? {
                    Some(text) => match super::number::int32(super::trim(text)) {
                        Ok(n) => n as u32,
                        Err(Error::Conversion(_)) => 0xffff,
                        Err(error) => return Err(error),
                    },
                    None => 0xffff,
                }
            };
            let mut row = EdgeRow {
                holder: record::reset_holder(8, position, default.binding()),
                position,
                discard: 0,
                corrections: 0,
            };
            // 83D39 clears validity BEFORE calling 8C4D4. Unlike UserEdge, an
            // early signed-No return therefore cannot retain reset's valid 1.
            row.holder.set_valid(false);
            let low = self
                .selection
                .number_base
                .wrapping_add(self.selection.first) as i32;
            let high = self.selection.number_base.wrapping_add(self.selection.last) as i32;
            if !self.sequential && ((position as i32) < low || (position as i32) > high) {
                return Ok(row);
            }
            for field in [2, 3] {
                let Some(text) = self.binding.cell(line, field)? else {
                    continue;
                };
                if text.is_empty() {
                    if field == 3 {
                        row.holder.set_valid(false);
                    }
                    return Ok(row);
                }
                let hz = super::number::frequency_hz(text, self.reader.locale().decimal)?;
                if !(30_000..=470_000_000).contains(&hz) {
                    row.holder.set_valid(false);
                    row.discard = 1;
                    return Ok(row);
                }
                if field == 2 {
                    // 8C7DB copies the complete default body after accepting low.
                    row.holder.copy_body_from(default);
                    row.holder.as_mut_bytes()[0x138..0x13c].copy_from_slice(&hz.to_le_bytes());
                } else {
                    row.holder.set_channel_field(ChannelField::FrequencyHz, hz);
                }
                row.holder.set_valid(true);
            }
            for (field, adapter) in [
                (4, super::payload::Adapter::Mode),
                (5, super::payload::Adapter::Data),
                (6, super::payload::Adapter::Filter),
            ] {
                if let Some(text) = self.binding.cell(line, field)?
                    && !text.is_empty()
                {
                    let value = super::table::portable_import(
                        adapter,
                        text,
                        self.reader.locale().decimal,
                        crt_decimal,
                        &mut row.corrections,
                    )?;
                    match field {
                        4 => row.holder.set_mode(value),
                        5 => row.holder.set_data(value),
                        _ => row.holder.set_filter(value),
                    }
                }
            }
            if let Some(text) = self.binding.cell(line, 1)? {
                let (name, clipped) = super::fixed_text::<16>(text);
                row.holder.set_name(&name);
                row.corrections = row.corrections.wrapping_add(u32::from(clipped));
            }
            finalize_scan(&mut row, default);
            Ok(row)
        })())
    }
    /// Advance only after the parent's real typed save returns, not on parse.
    pub fn applied(&mut self) {
        self.cursor = self.cursor.wrapping_add(1);
    }
}

fn finalize_scan(row: &mut EdgeRow, default: &DecodedRecord) {
    let mut name = *row.holder.name();
    let mut local = row
        .corrections
        .wrapping_add(u32::from(nls::sanitize(&mut name, b' ')));
    row.holder.set_name(&name);
    if row.holder.mode() == 0xffff || (row.holder.valid() && row.holder.mode() == 24) {
        row.holder.set_mode(default.mode());
        local = local.wrapping_add(1);
    }
    if row.holder.data() == 0xffff
        || (!matches!(row.holder.mode(), 0 | 1 | 8 | 13) && row.holder.data() != 0)
    {
        row.holder.set_data(default.data());
        local = local.wrapping_add(1);
    }
    // C9640/10150: in blocked modes EVERY value except FF is unsuitable.
    // The UI current-value bypass is not called by this CSV finalizer.
    if row.holder.filter() == 0xffff
        || (row.holder.valid()
            && matches!(row.holder.mode() as u8, 15 | 17)
            && row.holder.filter() != 255)
    {
        row.holder.set_filter(default.filter());
        local = local.wrapping_add(1);
    }
    // 83C2D..83C41 adds bool(local), not the number of corrections just made.
    row.corrections = row.corrections.wrapping_add(u32::from(local != 0));
}

fn scan_default() -> DecodedRecord {
    // 610C type 1F -> 681F: code constants, not an absent G filled with zeros.
    // No existing library entry constructs this complete decoded initializer.
    let mut holder = record::reset_holder(0x1f, 0, 0);
    holder.set_channel_field(ChannelField::FrequencyHz, 30_000);
    holder.as_mut_bytes()[0x138..0x13c].copy_from_slice(&30_000_u32.to_le_bytes());
    holder.set_mode(25);
    holder.set_filter(255);
    holder
}

fn save_scan(
    image: &mut Icf<'_>,
    working: &mut [u8],
    index: u32,
    holder: &mut DecodedRecord,
) -> Result<(), icf::Error> {
    record_image::store_scan_into(working, index, holder, |at, bytes| {
        image.patch_memory(&[(at, bytes)])
    })
}

fn post_scan(
    image: &mut Icf<'_>,
    selection: Selection,
    defaults: Option<&Icf<'_>>,
) -> Result<(), icf::Error> {
    // D1E88(D7) -> D276F -> CF3F8: its sole source is the current selected scan.
    // The range-membership test is mandatory even when a suitable row exists.
    let selected = u32::from((image.memory()[0x37802] >> 2) & 31);
    let rows = icf::special::scan_suitable_rows(
        image.memory(),
        image.memory(),
        false,
        false,
        false,
        |_| unreachable!("scan CSV post never queries a specific frequency range"),
    )?;
    if (rows.is_empty() && selected == 0) || selected < selection.first || selected > selection.last
    {
        return Ok(());
    }
    // 99B9C retains the native range's first index 0 when no suitable row exists.
    let candidate = rows.first().copied().unwrap_or(0);
    let reset = icf::special::scan_reset_enabled(image.memory(), 0)?;
    // An absent source stays absent: select_scan writes its selection/flag
    // prefix, then a required G read fails through the shared checked window.
    let g = match defaults {
        Some(defaults) => defaults.memory(),
        None => &[],
    };
    icf::special::select_scan(image, g, candidate, reset)
}

/// 83C60: header -> valid-row clear -> 8C4D4 -> typed save -> counters -> D7.
/// Only a conditional post-selection reset reads the explicit G generation.
pub fn import_scan(
    image: &mut Icf<'_>,
    source: &[u8],
    selection: Selection,
    whole: bool,
    locale: Locale,
    defaults: Option<&Icf<'_>>,
) -> Import {
    let mut out = Import {
        counts: Counts::default(),
        rows_read: 0,
        applied: 0,
        failure: None,
    };
    let result = (|| -> Result<(), Failure> {
        edge_selection(selection, 25)?;
        let mut reader = ScanRows::new(source, locale, selection, whole)?;
        let default = scan_default();
        let mut working = image.memory().to_vec();
        let mut holder = record::reset_holder(8, 0, 0);
        for index in selection.first..=selection.last {
            record_image::load_scan_into(&working, index, &mut holder)?;
            if holder.valid() {
                holder.set_valid(false);
                save_scan(image, &mut working, index, &mut holder)?;
            }
        }
        while let Some(row) = reader.next_with(&default, locale.decimal) {
            out.rows_read += 1;
            let mut row = row?;
            let relative = if row.position != 0xffff
                && (row.position as i32) >= selection.number_base as i32
            {
                row.position.wrapping_sub(selection.number_base)
            } else {
                row.position
            };
            if (relative as i32) > selection.last as i32 {
                if reader.sequential {
                    out.counts.s = 2;
                    break;
                }
                continue;
            }
            if row.holder.valid() {
                save_scan(image, &mut working, relative, &mut row.holder)?;
                out.applied += 1;
                reader.applied();
            }
            // Actual 83D7D saves BEFORE 83D8E/96 add C2/C3. Do not reuse the
            // differently ordered control::scan_row helper at this boundary.
            out.counts.c2 = out.counts.c2.wrapping_add(row.discard);
            out.counts.c3 = out.counts.c3.wrapping_add(row.corrections);
            out.counts.c4 = out.counts.c4.wrapping_add(u32::from(row.holder.valid()));
        }
        post_scan(image, selection, defaults)?;
        Ok(())
    })();
    if let Err(error) = result {
        out.failure = Some(error);
    }
    out
}

fn edge_selection(selection: Selection, limit: u32) -> Result<(), Error> {
    if selection.first > selection.last || selection.last >= limit {
        return Err(Error::Invalid("edge selection outside dataset".into()));
    }
    Ok(())
}

fn save_user_edge(
    image: &mut Icf<'_>,
    working: &mut [u8],
    index: u32,
    holder: &mut DecodedRecord,
) -> Result<(), Failure> {
    // Range callbacks are raw 8-relative, unlike Scan's absolute callbacks.
    // Let the sole adapter write first, then expose its actual prefix even on
    // failure. Never reinterpret its callback offset as an image address.
    let saved = record_image::store_range_into(working, 0x15, index, holder, |_, _| Ok(()));
    image.patch_memory(&[(0, working)])?;
    saved.map_err(Failure::Image)
}

/// 88B34's real clear -> byte parser -> typed saver path. The normal 80984
/// config has +9=1: only currently valid target rows are cleared, through the
/// one shared kind 15 saver. No generated G/default predicate is needed here.
pub fn import_user_edge(
    image: &mut Icf<'_>,
    source: &[u8],
    selection: Selection,
    whole: bool,
    locale: Locale,
) -> Import {
    let mut out = Import {
        counts: Counts::default(),
        rows_read: 0,
        applied: 0,
        failure: None,
    };
    let result = (|| -> Result<(), Failure> {
        edge_selection(selection, 30)?;
        let mut reader = UserEdgeRows::new(source, locale, selection, whole)?;
        let mut working = image.memory().to_vec();
        let mut holder = record::reset_holder(0x15, 0, 0);
        for index in selection.first..=selection.last {
            record_image::load_range_into(&working, 0x15, index, &mut holder)?;
            if holder.valid() {
                holder.set_valid(false);
                save_user_edge(image, &mut working, index, &mut holder)?;
            }
        }
        while let Some(row) = reader.next_row() {
            out.rows_read += 1;
            let mut row = row?;
            let relative = if row.position != 0xffff
                && (row.position as i32) >= selection.number_base as i32
            {
                row.position.wrapping_sub(selection.number_base)
            } else {
                row.position
            };
            if (relative as i32) > selection.last as i32 {
                if reader.sequential {
                    out.counts.s = 2;
                    break;
                }
                continue;
            }
            if row.holder.valid() {
                save_user_edge(image, &mut working, relative, &mut row.holder)?;
                out.applied += 1;
                reader.applied();
            }
            // Unlike 83C60, UserEdge's typed save precedes the local-count adds.
            out.counts.c2 = out.counts.c2.wrapping_add(row.discard);
            out.counts.c3 = out.counts.c3.wrapping_add(row.corrections);
            out.counts.c4 = out.counts.c4.wrapping_add(u32::from(row.holder.valid()));
        }
        Ok(())
    })();
    if let Err(error) = result {
        out.failure = Some(error);
    }
    out
}

pub fn export_user_edge(image: &Icf<'_>, selection: Selection, locale: Locale) -> Export {
    export_edges(image, selection, locale, false)
}

/// 83E7C/8CD08 visits all selected rows, including invalid bitmap entries.
/// Scan mode/DATA/filter retain their raw-byte projection, not channel masks.
pub fn export_scan(image: &Icf<'_>, selection: Selection, locale: Locale) -> Export {
    export_edges(image, selection, locale, true)
}

/// 835D0 over actual normal working maps or the fixed Call source. Selection
/// indices are local typed SIDES, not physical slots or paired display rows.
pub fn export_memory(
    image: &Icf<'_>,
    kind: Kind,
    group: Option<usize>,
    selection: Option<Selection>,
    locale: Locale,
) -> Export {
    let mut out = Export {
        bytes: Vec::new(),
        rows: 0,
        failure: None,
    };
    let result = (|| -> Result<(), Failure> {
        let (code, limit, number_base) = match kind {
            Kind::Memory => (2, 200, 0),
            Kind::FixedPair if group.is_none() => (3, 8, 1),
            _ => {
                return Err(Error::Invalid(
                    "memory CSV requires normal or ungrouped Call source".into(),
                )
                .into());
            }
        };
        if kind == Kind::Memory && group.is_none() && selection.is_some() {
            return Err(
                Error::Invalid("selected normal sides require an explicit group".into()).into(),
            );
        }
        let selection = selection.unwrap_or(Selection {
            first: 0,
            last: limit - 1,
            number_base,
        });
        edge_selection(selection, limit)?;
        let normal = if kind == Kind::Memory {
            Some(icf::rows::NormalMemory::from_icf(image)?)
        } else {
            None
        };
        let source = normal.as_ref().map_or(image.memory(), |m| m.working());
        let mut ranges = Vec::new();
        if let Some(normal) = &normal {
            let groups = match group {
                Some(g) if g < 100 => g..g + 1,
                None => 0..normal.group_count(),
                _ => return Err(Error::Invalid("memory group outside0..100".into()).into()),
            };
            for group in groups {
                if normal.group_used_slots(group)? != 0 {
                    ranges.push(
                        (selection.first..=selection.last)
                            .map(|side| (group as u32, side))
                            .collect::<Vec<_>>(),
                    );
                }
            }
        } else {
            ranges.push(
                (selection.first..=selection.last)
                    .map(|side| (100, side))
                    .collect(),
            );
        }
        let header = memory_header(kind == Kind::Memory, locale.list)?;
        let mut load_failure = None;
        let mut header_pending = true;
        let result = super::control::export_memory(
            &header,
            &ranges,
            true,
            |&(group, side)| {
                // B4DCC/B4D38 translates only the logical group/side coordinate;
                // the shared outer loader alone resolves its physical map and name.
                let index = if code == 2 { 200 * group + side } else { side };
                let mut holder = record::reset_holder(code, index, 0);
                if let Err(error) = record_image::load_into(source, code, index, &mut holder) {
                    load_failure = Some(error);
                    return Err(Error::Operation("memory typed source load failed".into()));
                }
                if !holder.valid() {
                    return Ok(None);
                }
                let context = super::payload::Context {
                    separator: locale.list,
                    decimal: locale.decimal,
                    row: side,
                    number_base,
                    group,
                    group_name: b"",
                    alarm: false,
                    utc_offset_minutes: 0,
                    memory_group_columns: code == 2,
                    memory_selection_column: code == 2,
                    memory_pair: true,
                };
                let bytes = super::payload::build_portable(
                    super::payload::Kind::Memory,
                    super::payload::Holder::new(holder.as_bytes())?,
                    context,
                    |_| {
                        Err(Error::SourceUnavailable(
                            "unexpected memory runtime formatter".into(),
                        ))
                    },
                )?;
                Ok(Some((holder.as_bytes()[0x24], bytes)))
            },
            |bytes| {
                out.bytes.extend_from_slice(bytes);
                out.rows += usize::from(!header_pending);
                header_pending = false;
                Ok(super::control::WriteResult {
                    bool_value: 1,
                    bytes_written: bytes.len(),
                })
            },
        );
        if let Some(error) = load_failure {
            return Err(error.into());
        }
        result?;
        Ok(())
    })();
    if let Err(error) = result {
        out.failure = Some(error);
    }
    out
}

fn memory_header(normal: bool, separator: u8) -> Result<Vec<u8>, Error> {
    // 89840's normal built-in Mch captions; 8A27C adds the group prefix.
    let mut cells = Vec::new();
    if normal {
        cells.extend([b"Group No".to_vec(), b"Group Name".to_vec()]);
    }
    cells.extend([b"CH No".to_vec(), b"Name".to_vec()]);
    if normal {
        cells.push(b"SEL".to_vec());
    }
    cells.extend(MEMORY_SIDE_LABELS.iter().map(|s| s.to_vec()));
    cells.push(b"Split".to_vec());
    cells.extend(MEMORY_SIDE_LABELS.iter().map(|&s| [s, b"(2)"].concat()));
    super::join(&cells, separator, true)
}

fn export_edges(image: &Icf<'_>, selection: Selection, locale: Locale, scan: bool) -> Export {
    let mut out = Export {
        bytes: Vec::new(),
        rows: 0,
        failure: None,
    };
    let result = (|| -> Result<(), Failure> {
        edge_selection(selection, if scan { 25 } else { 30 })?;
        let labels: &[&[u8]] = if scan {
            &[
                b"No",
                b"Name",
                b"Frequency Low",
                b"Frequency High",
                b"Mode",
                b"DATA",
                b"Filter",
            ]
        } else {
            &[b"No", b"Frequency Low", b"Frequency High"]
        };
        out.bytes = super::join(
            &labels.iter().map(|s| s.to_vec()).collect::<Vec<_>>(),
            locale.list,
            true,
        )?;
        let mut holder = record::reset_holder(if scan { 8 } else { 0x15 }, 0, 0);
        for index in selection.first..=selection.last {
            if scan {
                record_image::load_scan_into(image.memory(), index, &mut holder)?;
            } else {
                record_image::load_range_into(image.memory(), 0x15, index, &mut holder)?;
            }
            let context = super::payload::Context {
                separator: locale.list,
                decimal: locale.decimal,
                row: index,
                number_base: selection.number_base,
                group: 0,
                group_name: b"",
                alarm: false,
                utc_offset_minutes: 0,
                memory_group_columns: false,
                memory_selection_column: false,
                memory_pair: false,
            };
            let bytes = super::payload::build_portable(
                if scan {
                    super::payload::Kind::Scan
                } else {
                    super::payload::Kind::UserEdge
                },
                super::payload::Holder::new(holder.as_bytes())?,
                context,
                |_| {
                    Err(Error::SourceUnavailable(
                        "unexpected edge runtime formatter".into(),
                    ))
                },
            )?;
            out.bytes.extend_from_slice(&bytes);
            out.rows += 1;
        }
        Ok(())
    })();
    if let Err(error) = result {
        out.failure = Some(error);
    }
    out
}

/// Actual FromDV/FromFM/ToDV parents, not a DD decoder or a GUI caption dump.
/// Both current lists are expanded once; the shared loader owns every lookup.
pub fn export_history(
    image: &Icf<'_>,
    kind: u32,
    selection: Option<Selection>,
    locale: Locale,
    defaults: crate::repeater::working::DefaultContext,
) -> Export {
    let mut out = Export {
        bytes: Vec::new(),
        rows: 0,
        failure: None,
    };
    let result = (|| -> Result<(), Failure> {
        let (limit, labels): (u32, &[&[u8]]) = match kind {
            0x0f | 0x10 => (
                10,
                &[b"Call Sign", b"Name", b"Frequency", b"Dup", b"Offset"],
            ),
            0x12 => (20, &[b"Call Sign", b"Name"]),
            _ => {
                return Err(
                    Error::Invalid("history CSV root requires FromDV/FromFM/ToDV".into()).into(),
                );
            }
        };
        let range = match selection {
            None => 0..limit,
            Some(s) if s.first <= s.last && s.last < limit => s.first..s.last + 1,
            _ => {
                return Err(
                    Error::Invalid("history CSV selected range outside dataset".into()).into(),
                );
            }
        };
        let repeaters = crate::repeater::working::Working::from_icf(image, defaults)?;
        let ur = icf::rows::WorkingUr::from_icf(image)?;
        out.bytes = super::join(
            &labels
                .iter()
                .map(|label| label.to_vec())
                .collect::<Vec<_>>(),
            locale.list,
            true,
        )?;
        for index in range {
            let mut holder = record::reset_holder(kind, index, 0);
            record::load_history_storage_into(
                image,
                kind,
                index as usize,
                &repeaters,
                &ur,
                &mut holder,
            )?;
            if !holder.valid() {
                continue;
            }
            let context = super::payload::Context {
                separator: locale.list,
                decimal: locale.decimal,
                row: index,
                number_base: 0,
                group: 0,
                group_name: b"",
                alarm: false,
                utc_offset_minutes: 0,
                memory_group_columns: false,
                memory_selection_column: false,
                memory_pair: false,
            };
            out.bytes.extend_from_slice(&super::payload::build_portable(
                super::dispatch::for_context(kind).export,
                super::payload::Holder::new(holder.as_bytes())?,
                context,
                |_| {
                    Err(Error::SourceUnavailable(
                        "unexpected From/To runtime formatter".into(),
                    ))
                },
            )?);
            out.rows += 1;
        }
        Ok(())
    })();
    if let Err(error) = result {
        out.failure = Some(error);
    }
    out
}

/// 88208/91EE4/92AFC. Reverse rank maps stop at the first missing logical rank;
/// they are not the forward From/To maps and do not skip holes.
/// Symbol cells use independent APRS byte IDs, not original source captions.
pub fn export_receive_history(
    image: &Icf<'_>,
    selection: Option<Selection>,
    locale: Locale,
    datetime: &super::datetime::Context<'_>,
) -> Export {
    export_receive_history_with_date(image, selection, locale, datetime, |_| {
        Err(Error::SourceUnavailable(
            "Rx native calendar token output".into(),
        ))
    })
}

/// Native outputs are individual DateToken responses, never formatted rows.
pub fn export_receive_history_with_date(
    image: &Icf<'_>,
    selection: Option<Selection>,
    locale: Locale,
    datetime: &super::datetime::Context<'_>,
    mut native: impl FnMut(super::datetime::DateToken) -> Result<Vec<u8>, Error>,
) -> Export {
    let mut out = Export {
        bytes: Vec::new(),
        rows: 0,
        failure: None,
    };
    let result = (|| -> Result<(), Failure> {
        let range = match selection {
            None => 0..50,
            Some(s) if s.first <= s.last && s.last < 50 => s.first..s.last + 1,
            _ => return Err(Error::Invalid("Rx CSV rank range outside0..50".into()).into()),
        };
        let labels: [&[u8]; 37] = [
            b"Type",
            b"Frequency",
            b"Mode",
            b"Caller",
            b"/",
            b"Called",
            b"Rx RPT1",
            b"Rx RPT2",
            b"Message",
            b"Status",
            b"Received date",
            b"BK",
            b"EMR",
            b"Latitude",
            b"Longitude",
            b"Altitude",
            b"SSID",
            b"D-PRS Symbol",
            b"Course",
            b"Speed",
            b"Power",
            b"Height",
            b"Gain",
            b"Directivity",
            b"Object/Item Name",
            b"Data Type",
            b"Temperature",
            b"Rainfall",
            b"Rainfall (24 Hours)",
            b"Rainfall (Midnight)",
            b"Wind Direction",
            b"Wind Speed",
            b"Gust Speed",
            b"Barometric",
            b"Humidity",
            b"GPS Time Stamp",
            b"GPS Message",
        ];
        out.bytes = super::join(
            &labels.iter().map(|s| s.to_vec()).collect::<Vec<_>>(),
            locale.list,
            true,
        )?;
        for index in range {
            let mut holder = record::reset_holder(0x0e, index, 0);
            record::load_receive_history_storage_into(image, index, &mut holder)?;
            let Some(bytes) = receive_row(image.memory(), &holder, locale, datetime, &mut native)?
            else {
                break;
            };
            out.bytes.extend_from_slice(&bytes);
            out.rows += 1;
        }
        Ok(())
    })();
    if let Err(error) = result {
        out.failure = Some(error);
    }
    out
}

fn receive_row(
    image: &[u8],
    holder: &DecodedRecord,
    locale: Locale,
    datetime: &super::datetime::Context<'_>,
    native: &mut impl FnMut(super::datetime::DateToken) -> Result<Vec<u8>, Error>,
) -> Result<Option<Vec<u8>>, Error> {
    // 92B51..92B5E precedes even the invalid-H path. Capture once and pass the
    // same raw signed word to HistoryDate and Timestamp, without host state.
    let offset = i16::from_le_bytes(
        image
            .get(0x396ff..0x39701)
            .ok_or(Error::SourceUnavailable(
                "Rx live UTC offset word unavailable".into(),
            ))?
            .try_into()
            .expect("checked UTC word"),
    );
    if !holder.valid() {
        return Ok(None);
    }
    let context = super::payload::Context {
        separator: locale.list,
        decimal: locale.decimal,
        row: holder.index(),
        number_base: 0,
        group: 0,
        group_name: b"",
        alarm: false,
        utc_offset_minutes: offset,
        memory_group_columns: false,
        memory_selection_column: false,
        memory_pair: false,
    };
    super::payload::build_portable(
        super::payload::Kind::ReceiveHistory,
        super::payload::Holder::new(holder.as_bytes())?,
        context,
        |request| super::history::format_with_date(request, datetime, &mut *native),
    )
    .map(Some)
}

/// 847B8 selected/whole root. Selection controls how many initial rows are
/// deleted, but its original D1CB4 argument is always 0, not selection.first.
/// Accepted rows insert at ordinal 0,1,...; invalid rows do not advance it.
pub fn import_callsign(
    image: &mut Icf<'_>,
    source: &[u8],
    selection: Option<Selection>,
    locale: Locale,
    defaults: &Icf<'_>,
) -> Import {
    let mut out = Import {
        counts: Counts::default(),
        rows_read: 0,
        applied: 0,
        failure: None,
    };
    let result = (|| -> Result<(), Failure> {
        let mut rows = CallsignRows::new(source, locale)?;
        let mut working = icf::rows::WorkingUr::from_icf(image)?;
        let count = usize::from(working.count());
        let remove = match selection {
            None => count,
            Some(s) if s.first <= s.last && (s.last as usize) < count => {
                (s.last - s.first + 1) as usize
            }
            _ => {
                return Err(Error::Invalid("UR selected range outside current rows".into()).into());
            }
        };
        // Keep the reconstructed prefix even if a later per-field NLS fails.
        let operation = (|| -> Result<(), Failure> {
            let defaults = icf::normalizer::ReferenceImage::Separate(defaults.memory());
            for _ in 0..remove {
                working.delete_one(0, defaults)?;
            }
            let mut cursor = 0;
            while let Some(row) = rows.next_with(&icf::rows::ur_default(0, 0), nls::platform_map) {
                out.rows_read += 1;
                let row = row?;
                if !row.accepted {
                    out.counts.c2 = out.counts.c2.wrapping_add(1);
                    continue;
                }
                if working.count() > 299 {
                    out.counts.capacity_marker(1);
                    break;
                }
                if !working.insert_one(cursor, defaults)? {
                    return Err(
                        Error::Operation("UR capacity changed during typed insert".into()).into(),
                    );
                }
                working.save(cursor, &row.holder)?;
                cursor += 1;
                out.applied += 1;
                out.counts.c3 = out.counts.c3.wrapping_add(
                    row.corrections
                        .expect("accepted UR stores corrected output"),
                );
                out.counts.c4 = out.counts.c4.wrapping_add(1);
            }
            Ok(())
        })();
        image.patch_memory(&[(0, working.to_icf()?.memory())])?;
        operation
    })();
    if let Err(error) = result {
        out.failure = Some(error);
    }
    out
}

pub fn export_callsign(image: &Icf<'_>, selection: Option<Selection>, locale: Locale) -> Export {
    let mut out = Export {
        bytes: Vec::new(),
        rows: 0,
        failure: None,
    };
    let result = (|| -> Result<(), Failure> {
        let working = icf::rows::WorkingUr::from_icf(image)?;
        let count = usize::from(working.count());
        let range = match selection {
            None => 0..count as u32,
            Some(s) if s.first <= s.last && s.last < 300 => s.first..s.last + 1,
            _ => return Err(Error::Invalid("UR export range outside 0..300".into()).into()),
        };
        out.bytes = super::join(
            &[b"No".to_vec(), b"Name".to_vec(), b"Call Sign".to_vec()],
            locale.list,
            true,
        )?;
        let view = working.to_icf()?;
        for index in range {
            let context = super::payload::Context {
                separator: locale.list,
                decimal: locale.decimal,
                row: index,
                number_base: 1,
                group: 0,
                group_name: b"",
                alarm: false,
                utc_offset_minutes: 0,
                memory_group_columns: false,
                memory_selection_column: false,
                memory_pair: false,
            };
            out.bytes.extend_from_slice(&super::payload::callsign_row(
                &view,
                index as usize,
                context,
            )?);
            out.rows += 1;
        }
        Ok(())
    })();
    if let Err(error) = result {
        out.failure = Some(error);
    }
    out
}

/// 8E168's accepted result and original optional correction output. A blank
/// call returns before Name/finalizer and never stores corrected_out.
pub struct CallsignRow {
    pub holder: DecodedRecord,
    pub accepted: bool,
    pub corrections: Option<u32>,
}
pub struct CallsignRows<'a> {
    reader: Reader<'a>,
    binding: Binding,
    next: usize,
}
impl<'a> CallsignRows<'a> {
    pub fn new(source: &'a [u8], locale: Locale) -> Result<Self, Error> {
        let reader = Reader::new(source, locale)?;
        let binding = Binding::builtin(
            reader.header(),
            locale.list,
            &[b"Name", b"Call Sign"],
            ImportKind::Callsign,
        )?;
        if !binding.accepted {
            return Err(Error::Invalid("UR CSV requires Call Sign".into()));
        }
        Ok(Self {
            reader,
            binding,
            next: 0,
        })
    }
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
    /// The default is the actual type 23 body; a raw CSV packet is not a seed.
    /// Each row reinitializes the body before advancing through its fields.
    pub fn next_with(
        &mut self,
        default: &DecodedRecord,
        native: impl FnMut(&[u8], &mut [u8]) -> Result<i32, Error>,
    ) -> Option<Result<CallsignRow, Error>> {
        let line = *self.reader.rows().get(self.next)?;
        self.next += 1;
        Some((|| {
            let mut holder = record::reset_holder(0x0d, 0, default.binding());
            holder.copy_body_from(default);
            holder.set_valid(false);
            let mut corrections = 0_u32;
            if let Some(call) = self.binding.cell(line, 1)? {
                let mut shared = call.to_vec();
                let (mut call, clipped) = nls::callsign_fixed(&mut shared, native)?;
                corrections = corrections.wrapping_add(u32::from(clipped));
                corrections = corrections.wrapping_add(u32::from(nls::fold_callsign(&mut call)));
                holder.set_callsign(MemoryCallsign::Your, &call);
                holder.set_valid(call != [b' '; 8]);
                if !holder.valid() {
                    return Ok(CallsignRow {
                        holder,
                        accepted: false,
                        corrections: None,
                    });
                }
            }
            if let Some(name) = self.binding.cell(line, 0)? {
                let (name, clipped) = super::fixed_text::<16>(name);
                holder.set_name(&name);
                corrections = corrections.wrapping_add(u32::from(clipped));
            }
            let mut name = *holder.name();
            let name_changed = nls::sanitize(&mut name, b' ');
            holder.set_name(&name);
            let corrections = nls::callsign_correction_count(corrections, name_changed);
            Ok(CallsignRow {
                holder,
                accepted: true,
                corrections: Some(corrections),
            })
        })())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icf::record;
    fn same_bytes(actual: &[u8], expected: &[u8]) {
        assert_eq!(actual.len(), expected.len());
        assert_eq!(actual.iter().zip(expected).position(|(a, b)| a != b), None);
    }
    fn image() -> Icf<'static> {
        let mut bytes = vec![0xa7; 0x3c9c0];
        bytes[0x355a1] = 6;
        bytes[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        bytes[0x378c0..0x379b0].fill(0);
        Icf::from_memory(bytes).unwrap()
    }
    fn ur_image() -> Icf<'static> {
        let mut image = image();
        image.patch_memory(&[(0x3c0a7, &[0xff; 600])]).unwrap();
        image
    }
    fn ur_row(image: &mut Icf<'_>, index: usize, name: &[u8], call: &[u8; 8]) {
        let mut holder = icf::rows::ur_default(0, 0);
        let mut fixed = [b' '; 16];
        fixed[..name.len()].copy_from_slice(name);
        holder.set_name(&fixed);
        holder.set_callsign(MemoryCallsign::Your, call);
        record::store_callsign_storage_into(image, index, &holder).unwrap();
    }
    fn scan_seed() -> DecodedRecord {
        let mut holder = record::reset_holder(0x1f, 0, 0x12345678);
        holder.set_channel_field(ChannelField::FrequencyHz, 30_000);
        holder.as_mut_bytes()[0x138..0x13c].copy_from_slice(&30_000_u32.to_le_bytes());
        holder.set_mode(25);
        holder.set_data(0);
        holder.set_filter(255);
        holder
    }
    fn scan_image() -> Icf<'static> {
        let mut image = image();
        image
            .patch_memory(&[(0xa60b, &[0xff; 4]), (0x37802, &[0x83]), (0x37807, &[0])])
            .unwrap();
        image
    }
    fn memory_image() -> Icf<'static> {
        let mut raw = vec![0; 0x4997a];
        raw[0x355a1] = 6;
        raw[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
        raw[0x3fb40..0x3fb44].copy_from_slice(&3_u32.to_le_bytes());
        for cell in raw[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
            cell.copy_from_slice(&0xffff_u32.to_le_bytes());
        }
        raw[0x3662d..0x36821].fill(1);
        for g in 0..100 {
            raw[0x355c3 + 19 * g..0x355d3 + 19 * g].fill(b' ');
        }
        raw[0x355c3..0x355d3].copy_from_slice(b"HOME            ");
        raw[0x355c3 + 38..0x355d3 + 38].copy_from_slice(b"SECOND          ");
        let mut normal = icf::rows::NormalMemory::from_working(raw).unwrap();
        for (group, row) in [(0, 1), (2, 3)] {
            normal.allocate(group, row..row + 1).unwrap();
            for side in 0..2 {
                let index = (200 * group + 2 * row + side) as u32;
                let mut holder = record::reset_holder(2, index, 0);
                holder.set_name(b"ROW,NAME;       ");
                holder.set_mode(side as u32);
                holder.set_channel_field(
                    ChannelField::FrequencyHz,
                    if side == 0 { 145_125_000 } else { 433_500_000 },
                );
                holder.set_channel_field(ChannelField::Selection, 1);
                holder.set_filter(0);
                normal.store_channel(index, &mut holder).unwrap();
            }
        }
        Icf::from_memory(normal.to_icf().unwrap().memory().to_vec()).unwrap()
    }
    #[test]
    fn memory_header_keeps_ch_no_distinct_from_no_and_whole_group_preparation() {
        let locale = Locale::copied(b'.', b',');
        let mut rows = MemoryRows::new(
            b"No,Frequency,Frequency(2)\n9,145,433\n",
            Kind::Memory,
            locale,
            false,
        )
        .unwrap();
        assert!(rows.sequential());
        assert_eq!(rows.binding().positions[0], None);
        assert_eq!(rows.binding().positions[1], Some(0));
        assert_eq!(
            (rows.binding().positions[7], rows.binding().positions[23]),
            (Some(1), Some(2))
        );
        assert_eq!(rows.next_source(), Some(b"9,145,433".as_slice()));
        assert!(rows.next_source().is_none());
        let csv = b"Group No,CH No,Frequency\n0,27,145\n00,28,145\n";
        assert!(
            !MemoryRows::new(csv, Kind::Memory, locale, true)
                .unwrap()
                .sequential()
        );
        let csv = b"Group No,CH No,Frequency\n0,27,145\n1,28,145\n";
        assert!(
            MemoryRows::new(csv, Kind::Memory, locale, true)
                .unwrap()
                .sequential()
        );
        assert!(
            !MemoryRows::new(csv, Kind::Memory, locale, false)
                .unwrap()
                .sequential()
        );
        assert!(
            !MemoryRows::new(csv, Kind::FixedPair, locale, true)
                .unwrap()
                .sequential()
        );
        for csv in [
            b"CH No,Frequency(2)\n".as_slice(),
            b"\xef\xbb\xbfFrequency\n",
            b"\"Frequency\"\n",
        ] {
            assert!(MemoryRows::new(csv, Kind::Memory, locale, true).is_err());
        }
    }
    #[test]
    fn memory_default_uses_real_g_and_captured_b_while_call_lower_does_not_invent_no() {
        let q = icf::context::Context::from_bytes([0; 24]);
        let ranges = icf::frequency::Ranges::generate(&q, None).unwrap();
        let generation = image();
        let h = memory_default(
            Kind::Memory,
            None,
            7_100_000,
            generation.memory(),
            &ranges,
            None,
        )
        .unwrap();
        assert_eq!(
            (
                h.kind(),
                h.mode(),
                h.channel_field(ChannelField::FrequencyHz),
                h.offset_hz()
            ),
            (0x1d, 0, 14_100_000, 100_000)
        );
        assert!(memory_default(Kind::Memory, None, 7_100_000, &[], &ranges, None).is_err());
        let mut captured = [0; 24];
        captured[0] = 1;
        let ranges1 =
            icf::frequency::Ranges::generate(&icf::context::Context::from_bytes(captured), None)
                .unwrap();
        // F1DC maps index 2 -> class 1 and index 3 -> class 2, not EFCC(default Hz).
        for (candidate, frequency, offset) in [
            (145_000_000, 146_520_000, 600_000),
            (433_000_000, 446_000_000, 5_000_000),
        ] {
            let h = memory_default(
                Kind::Memory,
                None,
                candidate,
                generation.memory(),
                &ranges1,
                None,
            )
            .unwrap();
            assert_eq!(
                (h.channel_field(ChannelField::FrequencyHz), h.offset_hz()),
                (frequency, offset)
            );
        }
        let mut generation = generation.memory().to_vec();
        let mut call = record::reset_holder(3, 1, 0);
        call.set_name(b"ACTUAL CALL G   ");
        call.set_mode(13);
        call.set_filter(2);
        call.set_channel_field(ChannelField::FrequencyHz, 145_800_000);
        record_image::store_channel_into(
            &mut generation,
            record_image::ChannelKind::Call,
            1,
            &mut call,
            |_| Ok(()),
        )
        .unwrap();
        let h = memory_default(
            Kind::FixedPair,
            Some(2),
            433_000_000,
            &generation,
            &ranges,
            None,
        )
        .unwrap();
        assert_eq!(
            (
                h.index(),
                h.side(),
                h.mode(),
                h.filter(),
                h.channel_field(ChannelField::FrequencyHz)
            ),
            (1, 1, 13, 2, 145_800_000)
        );
        assert_eq!(h.name(), b"ACTUAL CALL G   ");
        assert!(matches!(
            memory_default(
                Kind::FixedPair,
                None,
                145_000_000,
                &generation,
                &ranges,
                None
            ),
            Err(Failure::Text(Error::SourceUnavailable(_)))
        ));
    }
    fn memory_options<'a>(
        g: &'a [u8],
        ranges: &'a icf::frequency::Ranges,
        q: &'a icf::context::Context,
    ) -> MemoryOptions<'a> {
        MemoryOptions {
            locale: Locale::copied(b'.', b','),
            crt_decimal: b'.',
            generation: g,
            ranges: Some(ranges),
            live_q: q,
            system_langid: None,
        }
    }

    #[test]
    fn memory_default_platform_nls_consumer() {
        let q = icf::context::Context::from_bytes([0; 24]);
        let ranges = icf::frequency::Ranges::generate(&q, None).unwrap();
        let g = memory_image();
        let options = memory_options(g.memory(), &ranges, &q);
        // CP932 fullwidth A/B/1 maps to the literal ASCII call on Windows.
        let source = b"CH No,Frequency,Mode,Your Call Sign,RPT1 Call Sign,RPT2 Call Sign\n0,145,DV,\x82\x60\x82\x61\x82\x50,\x82\x60\x82\x61\x82\x50,\x82\x60\x82\x61\x82\x50\n";
        let mut rows = MemoryRows::new(source, Kind::Memory, options.locale, false).unwrap();
        let line = rows.next_source().unwrap();
        let row = rows.side(line, 0, false, 0, &options);
        #[cfg(windows)]
        {
            let row = row.unwrap();
            assert!(row.holder.valid());
            for field in [
                MemoryCallsign::Your,
                MemoryCallsign::Repeater1,
                MemoryCallsign::Repeater2,
            ] {
                assert_eq!(row.holder.callsign(field), b"AB1     ");
            }
        }
        #[cfg(not(windows))]
        assert!(matches!(
            row,
            Err(Failure::Text(Error::SourceUnavailable(_)))
        ));
    }
    #[test]
    fn memory_csv_finalizer_keeps_alias_counts_dbcs_and_native_mode_data_filter_rules() {
        let q = icf::context::Context::from_bytes([0; 24]);
        let ranges = icf::frequency::Ranges::generate(&q, None).unwrap();
        let g = memory_image();
        let options = memory_options(g.memory(), &ranges, &q);
        for (mode, data, filter, expected, corrections, discard) in [
            ("WFM", "ON", "2", (15, 0, 0), 2, 0),
            ("Auto", "OFF", "1", (13, 0, 0), 2, 0),
            ("FM-N", "ON", "1", (13, 1, 0), 2, 0),
            ("bad", "OFF", "1", (13, 0, 0), 0, 1),
        ] {
            let csv = format!(
                "CH No,Frequency,Offset,Mode,DATA,Filter,Name\n0,145,0,{mode},{data},{filter},X\n"
            );
            let mut reader =
                MemoryRows::new(csv.as_bytes(), Kind::Memory, options.locale, false).unwrap();
            let line = reader.next_source().unwrap();
            let row = reader.side(line, 0, false, 0, &options).unwrap();
            assert_eq!(
                (row.corrections, row.discard),
                (corrections, discard),
                "{mode}"
            );
            assert_eq!(row.holder.valid(), discard == 0);
            if discard == 0 {
                assert_eq!(
                    (row.holder.mode(), row.holder.data(), row.holder.filter()),
                    expected,
                    "{mode}"
                );
            }
        }
        let mut csv = b"CH No,Frequency,Offset,Mode,DATA,Filter,Name\n0,145,0,LSB,OFF,1,".to_vec();
        csv.extend_from_slice(&[b'A'; 15]);
        csv.extend_from_slice(&[0x81, 0x41, b'\n']);
        let mut reader = MemoryRows::new(&csv, Kind::Memory, options.locale, false).unwrap();
        let line = reader.next_source().unwrap();
        let row = reader.side(line, 0, false, 0, &options).unwrap();
        assert_eq!(row.holder.name(), b"AAAAAAAAAAAAAAA ");
        assert_eq!(row.corrections, 2);
        // Captured B selects F1DC offset; live Q+12 alone gates MTON.
        for (captured, live, gate, offset, tone) in [
            (0, 1, 0, 0, 50),
            (1, 0, 0, 600_000, 50),
            (0, 1, 1, 0, 24),
            (1, 0, 1, 600_000, 24),
        ] {
            let mut captured_q = [0; 24];
            captured_q[0] = captured;
            let ranges = icf::frequency::Ranges::generate(
                &icf::context::Context::from_bytes(captured_q),
                None,
            )
            .unwrap();
            let mut live_q = [0; 24];
            live_q[0] = live;
            live_q[0x12] = gate;
            let live_q = icf::context::Context::from_bytes(live_q);
            let options = memory_options(g.memory(), &ranges, &live_q);
            let source = b"CH No,Frequency,Mode,Offset,Repeater Tone,TSQL Frequency\n0,145,FM,100,150.0Hz,150.0Hz\n";
            let mut reader = MemoryRows::new(source, Kind::Memory, options.locale, false).unwrap();
            let line = reader.next_source().unwrap();
            let row = reader.side(line, 0, false, 0, &options).unwrap();
            assert!(row.holder.valid());
            assert_eq!(
                (
                    row.holder.offset_hz(),
                    row.holder.tx_tone(),
                    row.holder.rx_tone()
                ),
                (offset, tone, tone)
            );
        }
    }
    #[test]
    fn memory_no_negative_overflow_empty_and_missing_b_remain_distinct_after_clear() {
        let q = icf::context::Context::from_bytes([0; 24]);
        let ranges = icf::frequency::Ranges::generate(&q, None).unwrap();
        let g = memory_image();
        let mut image = memory_image();
        let before = image.memory().to_vec();
        let out = import_memory(
            &mut image,
            b"Frequency\n145\n",
            Kind::Memory,
            Some(0),
            Some(Selection {
                first: 0,
                last: 0,
                number_base: 1,
            }),
            &memory_options(g.memory(), &ranges, &q),
        );
        assert!(matches!(
            out.report.failure,
            Some(Failure::Text(Error::Invalid(_)))
        ));
        assert_eq!((out.report.rows_read, out.report.applied), (0, 0));
        same_bytes(image.memory(), &before);
        for (no, discard) in [
            ("-1", 0),
            ("$FFFFFFFF", 0),
            ("$80000000", 0),
            ("100", 0),
            ("65535", 1),
            ("2147483648", 1),
            ("", 1),
        ] {
            let mut image = memory_image();
            let source = format!("CH No,Frequency\n{no},145\n");
            let out = import_memory(
                &mut image,
                source.as_bytes(),
                Kind::Memory,
                Some(0),
                None,
                &memory_options(g.memory(), &ranges, &q),
            );
            assert!(
                out.report.failure.is_none(),
                "{no}: {:?}",
                out.report.failure
            );
            assert_eq!((out.report.rows_read, out.report.applied), (1, 0));
            assert_eq!(
                out.report.counts,
                Counts {
                    s: 0,
                    c2: discard,
                    c3: 0,
                    c4: 0
                },
                "{no}"
            );
        }
        let mut options = memory_options(g.memory(), &ranges, &q);
        options.ranges = None;
        let mut image = memory_image();
        let out = import_memory(
            &mut image,
            b"Frequency\n",
            Kind::Memory,
            Some(0),
            None,
            &options,
        );
        assert!(out.report.failure.is_none());
        let mut image = memory_image();
        let out = import_memory(
            &mut image,
            b"Frequency\n145\n",
            Kind::Memory,
            Some(0),
            None,
            &options,
        );
        assert!(matches!(
            out.report.failure,
            Some(Failure::Text(Error::SourceUnavailable(_)))
        ));
        assert_eq!(out.report.applied, 0);
    }
    #[test]
    fn memory_import_uses_native_clear_default_map_pair_and_real_save_counts() {
        let mut image = memory_image();
        let mut g = memory_image();
        g.patch_memory(&[
            (0x36821, &0x1234_u16.to_le_bytes()),
            (0x3c84c, &0x5678_u16.to_le_bytes()),
        ])
        .unwrap();
        image
            .patch_memory(&[
                (9, &[0xa7]),
                (0x392fa, &[0]),
                (0x392fb, &2_u16.to_le_bytes()),
            ])
            .unwrap();
        let q = icf::context::Context::from_bytes([0; 24]);
        let ranges = icf::frequency::Ranges::generate(&q, None).unwrap();
        let source=b"Group No,Group Name,CH No,Name,SEL,Frequency,Offset,Mode,Split,Frequency(2),Offset(2),Mode(2)\n99,NEW,2,PAIR;NAME,SEL2,145.125,0,LSB,ON,145.5,0,USB\n";
        let out = import_memory(
            &mut image,
            source,
            Kind::Memory,
            Some(0),
            Some(Selection {
                first: 1,
                last: 1,
                number_base: 0,
            }),
            &memory_options(g.memory(), &ranges, &q),
        );
        assert!(out.report.failure.is_none(), "{:?}", out.report.failure);
        assert_eq!((out.report.rows_read, out.report.applied), (1, 3));
        assert_eq!(
            out.report.counts,
            Counts {
                s: 0,
                c2: 0,
                c3: 1,
                c4: 1
            }
        );
        assert!(out.home_invalidated);
        let model = icf::rows::NormalMemory::from_icf(&image).unwrap();
        assert_eq!(model.slot(0, 1).unwrap(), None);
        assert!(model.slot(0, 2).unwrap().is_some());
        assert!(model.slot(2, 3).unwrap().is_some());
        assert_eq!(image.memory()[9], 0xa7);
        assert_eq!(&image.memory()[0x36821..0x36823], &0x1234_u16.to_le_bytes());
        assert_eq!(&image.memory()[0x3c84c..0x3c84e], &0x1234_u16.to_le_bytes());
        assert_eq!(image.memory()[0x392fa], 255);
        let exported = export_memory(
            &image,
            Kind::Memory,
            Some(0),
            None,
            Locale::copied(b'.', b','),
        );
        assert!(exported.bytes.ends_with(b"00,NEW,02,PAIR NAME,SEL2,145.125000,OFF,0.000000,LSB,OFF,1,,,,,,,,,,,ON,145.500000,OFF,0.000000,USB,OFF,1,,,,,,,,,,\r\n"));
    }
    #[test]
    fn memory_import_header_and_g_failure_preserve_actual_order_and_late_lower_prefix() {
        let q = icf::context::Context::from_bytes([0; 24]);
        let ranges = icf::frequency::Ranges::generate(&q, None).unwrap();
        let mut image = memory_image();
        let before = image.memory().to_vec();
        let bad = import_memory(
            &mut image,
            b"Name\nX\n",
            Kind::Memory,
            Some(0),
            None,
            &memory_options(&[], &ranges, &q),
        );
        assert!(bad.report.failure.is_some());
        same_bytes(image.memory(), &before);
        let out = import_memory(
            &mut image,
            b"Frequency\n",
            Kind::Memory,
            Some(0),
            None,
            &memory_options(&[], &ranges, &q),
        );
        assert_eq!((out.report.rows_read, out.report.applied), (0, 0));
        assert!(out.report.failure.is_some());
        assert_eq!(&image.memory()[0x355c3..0x355d3], b"HOME            ");
        assert!(
            icf::rows::NormalMemory::from_icf(&image)
                .unwrap()
                .slot(0, 1)
                .unwrap()
                .is_none()
        );
        let mut image = memory_image();
        let g = memory_image();
        image
            .patch_memory(&[(0x392fa, &[0]), (0x392fb, &1_u16.to_le_bytes())])
            .unwrap();
        let mut source =
            b"CH No,Name,Frequency,Mode,Split,Frequency(2),Mode(2),Offset(2)\n0,PREFIX,145,LSB,ON,433,LSB,"
                .to_vec();
        source.extend_from_slice(&vec![b'9'; 4097]);
        source.push(b'\n');
        let out = import_memory(
            &mut image,
            &source,
            Kind::Memory,
            Some(0),
            None,
            &memory_options(g.memory(), &ranges, &q),
        );
        assert!(
            matches!(out.report.failure, Some(Failure::Text(Error::Limit(_)))),
            "{:?}",
            out.report.failure
        );
        assert_eq!((out.report.rows_read, out.report.applied), (1, 2));
        assert_eq!(out.report.counts, Counts::default());
        let mut h = record::reset_holder(2, 0, 0);
        record::load_channel_image_into(&image, record_image::ChannelKind::Normal, 0, &mut h)
            .unwrap();
        assert_eq!(
            (h.channel_field(ChannelField::FrequencyHz), h.split()),
            (145_000_000, 0)
        );
        assert_eq!(
            (image.memory()[0x392fa], &image.memory()[0x392fb..0x392fd]),
            (0, 1_u16.to_le_bytes().as_slice())
        );
    }
    #[test]
    fn call_import_keeps_provisional_lower_and_unwritten_default_source_distinct() {
        let q = icf::context::Context::from_bytes([0; 24]);
        let ranges = icf::frequency::Ranges::generate(&q, None).unwrap();
        let g = memory_image();
        for (lower, fails) in [(b"".as_slice(), false), (b"433", true)] {
            let mut image = memory_image();
            image
                .patch_memory(&[
                    (0x3c84b, &[100]),
                    (0x392fa, &[100]),
                    (0x392fb, &3_u16.to_le_bytes()),
                ])
                .unwrap();
            let source = [
                b"CH No,Name,Frequency,Mode,Split,Frequency(2)\n1,CALL ROW,145.25,LSB,ON,"
                    .as_slice(),
                lower,
                b"\n",
            ]
            .concat();
            let out = import_memory(
                &mut image,
                &source,
                Kind::FixedPair,
                None,
                Some(Selection {
                    first: 0,
                    last: 0,
                    number_base: 1,
                }),
                &memory_options(g.memory(), &ranges, &q),
            );
            assert_eq!((out.report.rows_read, out.report.applied), (1, 2));
            assert!(out.home_invalidated && out.call_invalidated);
            if fails {
                assert!(matches!(
                    out.report.failure,
                    Some(Failure::Text(Error::SourceUnavailable(_)))
                ));
                assert_eq!(out.report.counts, Counts::default());
                assert_eq!(image.memory()[0x392fa], 100);
            } else {
                assert!(out.report.failure.is_none(), "{:?}", out.report.failure);
                assert_eq!(
                    out.report.counts,
                    Counts {
                        s: 0,
                        c2: 0,
                        c3: 1,
                        c4: 1
                    }
                );
                assert_eq!(image.memory()[0x392fa], 255);
            }
            for side in 0..2 {
                let mut h = record::reset_holder(3, side, 0);
                record_image::load_into(image.memory(), 3, side, &mut h).unwrap();
                assert_eq!(
                    (h.channel_field(ChannelField::FrequencyHz), h.split()),
                    (145_250_000, 0)
                );
                assert_eq!(h.name(), b"CALL ROW        ");
            }
        }
    }
    #[test]
    fn memory_sequential_capacity_keeps_real_three_or_six_saves_not_c4_marker() {
        for free in [1, 2] {
            let mut raw = vec![0; 0x4997a];
            raw[0x355a1] = 6;
            raw[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
            raw[0x3fb40..0x3fb44].copy_from_slice(&6_u32.to_le_bytes());
            raw[0x3662d..0x36821].fill(1);
            for g in 0..100 {
                raw[0x355c3 + 19 * g..0x355d3 + 19 * g].fill(b' ');
            }
            for cell in raw[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
                cell.copy_from_slice(&0xffff_u32.to_le_bytes());
            }
            let mut model = icf::rows::NormalMemory::from_working(raw).unwrap();
            for g in 1..=4 {
                model.allocate(g, 0..100).unwrap();
            }
            model.allocate(5, 0..100 - free).unwrap();
            let mut image = Icf::from_memory(model.to_icf().unwrap().memory().to_vec()).unwrap();
            let g = memory_image();
            let q = icf::context::Context::from_bytes([0; 24]);
            let ranges = icf::frequency::Ranges::generate(&q, None).unwrap();
            let source=b"Name,Frequency,Offset,Mode,Frequency(2),Offset(2),Mode(2)\nONE,145,0,LSB,145.5,0,USB\nTWO,145,0,LSB,145.5,0,USB\nTHREE,145,0,LSB,145.5,0,USB\n";
            let out = import_memory(
                &mut image,
                source,
                Kind::Memory,
                Some(0),
                None,
                &memory_options(g.memory(), &ranges, &q),
            );
            assert!(out.report.failure.is_none(), "{:?}", out.report.failure);
            assert_eq!(
                out.report.counts,
                Counts {
                    s: 1,
                    c2: 0,
                    c3: free as u32,
                    c4: 1
                }
            );
            assert_eq!(
                (out.report.rows_read, out.report.applied),
                (free + 1, 3 * free)
            );
            let model = icf::rows::NormalMemory::from_icf(&image).unwrap();
            assert_eq!(model.used_slots(), 500);
            assert!(model.slot(0, 0).unwrap().is_some());
            assert_eq!(model.slot(0, free).unwrap(), None);
        }
    }
    const MEMORY_HEADER:&[u8]=b"Group No,Group Name,CH No,Name,SEL,Frequency,Dup,Offset,Mode,DATA,Filter,TONE,Repeater Tone,TSQL Frequency,DTCS Code,DTCS Polarity,DV SQL,DV CSQL Code,Your Call Sign,RPT1 Call Sign,RPT2 Call Sign,Split,Frequency(2),Dup(2),Offset(2),Mode(2),DATA(2),Filter(2),TONE(2),Repeater Tone(2),TSQL Frequency(2),DTCS Code(2),DTCS Polarity(2),DV SQL(2),DV CSQL Code(2),Your Call Sign(2),RPT1 Call Sign(2),RPT2 Call Sign(2)\r\n";
    #[test]
    fn memory_export_uses_shared_outer_group_name_map_and_pair_buffer() {
        let image = memory_image();
        let before = image.memory().to_vec();
        let out = export_memory(&image, Kind::Memory, None, None, Locale::copied(b'.', b','));
        assert!(out.failure.is_none(), "{:?}", out.failure);
        assert_eq!(out.rows, 2);
        let expected=[MEMORY_HEADER,b"00,HOME,01,ROW NAME,SEL1,145.125000,OFF,0.000000,LSB,OFF,1,,,,,,,,,,,OFF,433.500000,OFF,0.000000,USB,OFF,1,,,,,,,,,,\r\n02,SECOND,03,ROW NAME,SEL1,145.125000,OFF,0.000000,LSB,OFF,1,,,,,,,,,,,OFF,433.500000,OFF,0.000000,USB,OFF,1,,,,,,,,,,\r\n"].concat();
        same_bytes(&out.bytes, &expected);
        let out = export_memory(
            &image,
            Kind::Memory,
            Some(2),
            Some(Selection {
                first: 7,
                last: 7,
                number_base: 0,
            }),
            Locale::copied(b'.', b','),
        );
        assert!(out.failure.is_none());
        assert_eq!(out.rows, 1);
        assert_eq!(
            &out.bytes[MEMORY_HEADER.len()..],
            b",433.500000,OFF,0.000000,USB,OFF,1,,,,,,,,,,\r\n"
        );
        let bad = export_memory(
            &image,
            Kind::Memory,
            None,
            Some(Selection {
                first: 0,
                last: 1,
                number_base: 0,
            }),
            Locale::copied(b'.', b','),
        );
        assert!(bad.failure.is_some());
        assert!(bad.bytes.is_empty());
        same_bytes(image.memory(), &before);
    }
    #[test]
    fn call_export_uses_eight_fixed_typed_sides_and_actual_one_based_pair_numbers() {
        let image = memory_image();
        let before = image.memory().to_vec();
        let out = export_memory(
            &image,
            Kind::FixedPair,
            None,
            None,
            Locale::copied(b'.', b','),
        );
        assert!(out.failure.is_none(), "{:?}", out.failure);
        assert_eq!(out.rows, 4);
        assert!(
            out.bytes
                .starts_with(b"CH No,Name,Frequency,Dup,Offset,Mode,DATA,Filter,")
        );
        let lines = out
            .bytes
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        for (line, no) in lines[1..]
            .iter()
            .zip([b"01,".as_slice(), b"02,", b"03,", b"04,"])
        {
            assert!(line.starts_with(no));
            assert_eq!(line.split(|b| *b == b',').count(), 35);
        }
        let out = export_memory(
            &image,
            Kind::FixedPair,
            None,
            Some(Selection {
                first: 6,
                last: 7,
                number_base: 1,
            }),
            Locale::copied(b',', b';'),
        );
        assert!(out.failure.is_none());
        assert_eq!(out.rows, 1);
        assert!(out.bytes.windows(5).any(|w| w == b"\n04;;"));
        let bad = export_memory(
            &image,
            Kind::AlternatingPair,
            None,
            None,
            Locale::copied(b'.', b','),
        );
        assert!(bad.failure.is_some());
        assert!(bad.bytes.is_empty());
        same_bytes(image.memory(), &before);
    }
    fn history_image() -> (Icf<'static>, crate::repeater::working::DefaultContext) {
        let mut image = ur_image();
        image
            .patch_memory(&[
                (0x3ab82, &[0xff; 5000]),
                (0x3bf0a, &[0; 100]),
                (0x392fd, &[0xff; 2]),
                (0x3c8b9, &[0xff; 2]),
                (0x3c903, &[0xff; 2]),
            ])
            .unwrap();
        let defaults =
            record::repeater_default_context(&icf::context::Context::from_bytes([0; 24]), None)
                .unwrap();
        let mut working = crate::repeater::working::Working::from_icf(&image, defaults).unwrap();
        let seed = record::reset_holder(12, 0, 0);
        for (index, call, name, hz, mode, duplex, offset) in [
            (
                0,
                *b"DVTEST B",
                b"DV NAME".as_slice(),
                145_125_000,
                2,
                1,
                600_000,
            ),
            (
                1,
                *b"FMTEST B",
                b"FM NAME".as_slice(),
                433_500_000,
                0,
                2,
                5_000_000,
            ),
        ] {
            assert!(
                working
                    .insert_one(
                        0,
                        index,
                        &seed,
                        |_| Ok(defaults.holder(0, 0)),
                        |_, which| record::repeater_reset_word(image.memory(), which)
                    )
                    .unwrap()
            );
            let mut holder = defaults.holder(0, 0);
            holder.set_valid(true);
            let mut fixed = [b' '; 16];
            fixed[..name.len()].copy_from_slice(name);
            holder.set_name(&fixed);
            holder.set_repeater_text(record::RepeaterText::Call, &call);
            holder.set_repeater_field(record::RepeaterField::Mode, mode);
            holder.set_channel_field(ChannelField::FrequencyHz, hz);
            holder.set_channel_field(ChannelField::Duplex, duplex);
            holder.set_offset_hz(offset);
            working.save(index, &holder).unwrap();
        }
        let mut image = Icf::from_memory(working.to_icf().unwrap().memory().to_vec()).unwrap();
        image
            .patch_memory(&[
                (0x3c331, &[0xff; 10]),
                (0x3c33b, &[0xff; 10]),
                (0x3c34f, &[0xff; 20]),
                (0x3c331, &[7, 2, 9]),
                (0x32935 + 7 * 8, b"DVTEST B"),
                (0x32935 + 2 * 8, b"NOMATCH "),
                (0x32935 + 9 * 8, b"        "),
                (0x3c33b, &[4, 5]),
                (
                    0x32985 + 4 * 12,
                    &[b"FMTEST B".as_slice(), &433_500_000_u32.to_le_bytes()].concat(),
                ),
                (
                    0x32985 + 5 * 12,
                    &[b"FMTEST B".as_slice(), &145_000_000_u32.to_le_bytes()].concat(),
                ),
                (0x3c34f, &[3, 4, 5]),
                (0x32a4d + 3 * 8, b"/DVTESTB"),
                (0x32a4d + 4 * 8, b"URCALL A"),
                (0x32a4d + 5 * 8, b"NOSUPPL "),
            ])
            .unwrap();
        ur_row(&mut image, 0, b"UR NAME", b"URCALL  ");
        image.patch_memory(&[(0x3c0a7, &[0, 0])]).unwrap();
        (image, defaults)
    }
    fn rx_datetime() -> super::super::datetime::Context<'static> {
        super::super::datetime::Context {
            short_date: b"yyyy/mm/dd",
            date_separator: b'/',
            time_separator: b':',
            decimal_separator: b'.',
            am: b"AM",
            pm: b"PM",
            current_year: 2026,
            two_digit_window: 50,
            primary_language: 9,
            eras: &[],
        }
    }
    fn rx_image() -> Icf<'static> {
        let mut image = image();
        image
            .patch_memory(&[(0x3c2ff, &[255; 50]), (0x396ff, &60_i16.to_le_bytes())])
            .unwrap();
        for (physical, rank, call) in [
            (1, 0, b"CALLONE "),
            (3, 0, b"DUPLATE "),
            (7, 2, b"LATER   "),
        ] {
            let mut basic = [b' '; 48];
            basic[..8].copy_from_slice(call);
            basic[8..12].copy_from_slice(b"SUFX");
            basic[12..20].copy_from_slice(b"RPTONE B");
            basic[20..28].copy_from_slice(b"RPTTWO G");
            basic[28..36].copy_from_slice(b"CQCQCQ  ");
            basic[36..40].copy_from_slice(&145_125_000_u32.to_le_bytes());
            basic[40] = 9;
            basic[41..47].copy_from_slice(&[0x31, 0x12, 0x24, 0x23, 0x30, 0x45]);
            basic[47] = 0;
            let mut message = [b' '; 20];
            message[..11].copy_from_slice(b"MSG,; \r\nEND");
            let mut gps = [255; 92];
            gps[8..12].copy_from_slice(&i32::MAX.to_le_bytes());
            gps[12..14].copy_from_slice(b"/>");
            gps[26..29].copy_from_slice(&[0x23, 0x30, 0x45]);
            gps[29] = 0;
            gps[49..].fill(b' ');
            gps[49..58].copy_from_slice(b"GPS;,\tEND");
            image
                .patch_memory(&[
                    (0x3c2ff + physical, &[rank]),
                    (0x309c3 + physical, &[0x29]),
                    (0x309c3 + 0x32 + 48 * physical, &basic),
                    (0x309c3 + 0x992 + 20 * physical, &message),
                    (0x309c3 + 0xd7a + 92 * physical, &gps),
                ])
                .unwrap();
        }
        image
    }
    #[test]
    fn rx_root_uses_first_reverse_rank_stops_at_hole_and_shares_real_formatters() {
        let image = rx_image();
        let before = image.memory().to_vec();
        let out = export_receive_history(&image, None, Locale::copied(b'.', b','), &rx_datetime());
        assert!(out.failure.is_none(), "{:?}", out.failure);
        assert_eq!(out.rows, 1);
        let lines = out.bytes.split(|b| *b == b'\n').collect::<Vec<_>>();
        assert!(lines[0].starts_with(b"Type,Frequency,Mode,Caller,/,Called,Rx RPT1,Rx RPT2,"));
        assert_eq!(lines[0].split(|b| *b == b',').count(), 37);
        let expected = [
            "RF",
            "145.125000",
            "DV",
            "CALLONE",
            "SUFX",
            "CQCQCQ",
            "RPTONE B",
            "RPTTWO G",
            "MSG   __END",
            "RPT?",
            "2025/01/01 00:30:45",
            "*",
            "*",
            "",
            "",
            "",
            "",
            "APRS 2F3E",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "00:30:45",
            "GPS  _END\r",
        ]
        .join(",");
        assert_eq!(lines[1], expected.as_bytes());
        let out = export_receive_history(
            &image,
            Some(Selection {
                first: 2,
                last: 2,
                number_base: 0,
            }),
            Locale::copied(b',', b';'),
            &rx_datetime(),
        );
        assert!(out.failure.is_none());
        assert_eq!(out.rows, 1);
        assert!(
            out.bytes
                .windows(b"RF;145,125000;DV;LATER;SUFX".len())
                .any(|w| w == b"RF;145,125000;DV;LATER;SUFX")
        );
        same_bytes(image.memory(), &before);
    }
    #[test]
    fn rx_native_date_tokens_follow_shifted_repeated_requests_and_late_failure_prefix() {
        use crate::csv::datetime::Date;
        let mut image = rx_image();
        image.patch_memory(&[(0x3c2ff + 3, &[1])]).unwrap();
        let before = image.memory().to_vec();
        let mut date = rx_datetime();
        date.short_date = b"ddd mmmm d ggg e mmm mmm";
        let tokens = [
            (b'd', 3, b"W\"ed".as_slice()),
            (b'm', 4, b"January"),
            (b'g', 3, b"ERA"),
            (b'e', 1, b"7"),
            (b'm', 3, b"J1"),
            (b'm', 3, b"J2"),
        ];
        let selection = Some(Selection {
            first: 0,
            last: 1,
            number_base: 0,
        });
        let mut seen = 0;
        let out = export_receive_history_with_date(
            &image,
            selection,
            Locale::copied(b'.', b','),
            &date,
            |request| {
                assert_eq!(
                    request.date,
                    Date {
                        year: 2025,
                        month: 1,
                        day: 1
                    }
                );
                let (letter, count, output) = tokens[seen % tokens.len()];
                assert_eq!((request.letter, request.count), (letter, count));
                seen += 1;
                Ok(output.to_vec())
            },
        );
        assert!(out.failure.is_none(), "{:?}", out.failure);
        assert_eq!((out.rows, seen), (2, 12));
        for line in out.bytes.split(|b| *b == b'\n').skip(1).take(2) {
            let cells = line.split(|b| *b == b',').collect::<Vec<_>>();
            assert_eq!(cells.len(), 37);
            assert_eq!(cells[10], b"W\"ed January 1 ERA 7 J1 J2 00:30:45");
            assert_eq!(cells[35], b"00:30:45");
        }
        let mut seen = 0;
        let out = export_receive_history_with_date(
            &image,
            selection,
            Locale::copied(b'.', b','),
            &date,
            |_| {
                seen += 1;
                if seen > 6 {
                    Err(Error::Operation("late native DateToken failure".into()))
                } else {
                    Ok(b"TOKEN".to_vec())
                }
            },
        );
        assert_eq!((out.rows, seen), (1, 7));
        assert!(matches!(
            out.failure,
            Some(Failure::Text(Error::Operation(_)))
        ));
        assert_eq!(out.bytes.iter().filter(|&&b| b == b'\n').count(), 2);
        same_bytes(image.memory(), &before);
    }
    #[test]
    fn rx_native_date_source_is_not_read_for_numeric_empty_or_invalid_date() {
        let mut image = rx_image();
        let out = export_receive_history_with_date(
            &image,
            None,
            Locale::copied(b'.', b','),
            &rx_datetime(),
            |_| panic!("numeric date needs no native source"),
        );
        assert!(out.failure.is_none());
        let mut date = rx_datetime();
        date.short_date = b"mmmm ddd ggg e";
        image
            .patch_memory(&[(0x309c3 + 0x32 + 48 + 41, &[255])])
            .unwrap();
        let out = export_receive_history_with_date(
            &image,
            None,
            Locale::copied(b'.', b','),
            &date,
            |_| panic!("compact invalid date needs no native source"),
        );
        assert!(out.failure.is_none());
        assert_eq!(out.rows, 1);
        let line = out.bytes.split(|b| *b == b'\n').nth(1).unwrap();
        assert!(line.split(|b| *b == b',').nth(10).unwrap().is_empty());
        image.patch_memory(&[(0x3c2ff, &[255; 50])]).unwrap();
        let out = export_receive_history_with_date(
            &image,
            None,
            Locale::copied(b'.', b','),
            &date,
            |_| panic!("missing rank needs no native source"),
        );
        assert!(out.failure.is_none());
        assert_eq!(out.rows, 0);
    }
    #[test]
    fn rx_capture_precedes_invalid_h_and_formatter_error_keeps_only_real_prefix() {
        let mut invalid = record::reset_holder(14, 0, 0);
        invalid.set_valid(false);
        assert!(matches!(
            receive_row(
                &[],
                &invalid,
                Locale::copied(b'.', b','),
                &rx_datetime(),
                &mut |_| panic!("invalid holder must not request native date output")
            ),
            Err(Error::SourceUnavailable(_))
        ));
        let mut image = rx_image();
        assert!(
            receive_row(
                image.memory(),
                &invalid,
                Locale::copied(b'.', b','),
                &rx_datetime(),
                &mut |_| panic!("invalid holder must not request native date output")
            )
            .unwrap()
            .is_none()
        );
        image
            .patch_memory(&[(0x3c2ff + 3, &[1]), (0x309c3 + 0xd7a + 92 * 3 + 12, b"/?")])
            .unwrap();
        let out = export_receive_history(&image, None, Locale::copied(b'.', b','), &rx_datetime());
        assert!(out.failure.is_none());
        assert_eq!(out.rows, 3);
        assert!(out.bytes.windows(9).any(|b| b == b"APRS 2F3F"));
        // Unknown symbols are data, not formatter failures. An invalid Type on
        // the second row still exercises the existing completed-prefix boundary.
        image
            .patch_memory(&[(0x309c3 + 0x32 + 48 * 3 + 47, &[3])])
            .unwrap();
        let out = export_receive_history(&image, None, Locale::copied(b'.', b','), &rx_datetime());
        assert_eq!(out.rows, 1);
        assert!(matches!(
            out.failure,
            Some(Failure::Text(Error::SourceUnavailable(_)))
        ));
        image
            .patch_memory(&[(0x396ff, &1440_i16.to_le_bytes())])
            .unwrap();
        let out = export_receive_history(&image, None, Locale::copied(b'.', b','), &rx_datetime());
        assert_eq!(out.rows, 0);
        assert!(matches!(
            out.failure,
            Some(Failure::Text(Error::Conversion(_)))
        ));
        assert_eq!(out.bytes.iter().filter(|&&b| b == b'\n').count(), 1);
    }
    #[test]
    fn history_export_uses_nonidentity_maps_and_actual_current_lists_not_gui_fields() {
        let (image, defaults) = history_image();
        let before = image.memory().to_vec();
        for (kind,rows,expected) in [
            (0x0f,3,b"Call Sign;Name;Frequency;Dup;Offset\r\nDVTEST B;DV NAME;145,125000;DUP-;0,600000\r\nNOMATCH;;;;\r\n;;;;\r\n".as_slice()),
            (0x10,2,b"Call Sign;Name;Frequency;Dup;Offset\r\nFMTEST B;FM NAME;433,500000;DUP+;5,000000\r\nFMTEST B;;;;\r\n"),
            (0x12,3,b"Call Sign;Name\r\n/DVTESTB;DV NAME\r\nURCALL A;UR NAME\r\nNOSUPPL;\r\n"),
        ] {
            let out=export_history(&image,kind,None,Locale::copied(b',',b';'),defaults);
            assert!(out.failure.is_none(),"{:?}",out.failure);assert_eq!(out.rows,rows);assert_eq!(out.bytes,expected);
        }
        let selected = export_history(
            &image,
            0x10,
            Some(Selection {
                first: 1,
                last: 1,
                number_base: 0,
            }),
            Locale::copied(b'.', b','),
            defaults,
        );
        assert_eq!(
            selected.bytes,
            b"Call Sign,Name,Frequency,Dup,Offset\r\nFMTEST B,,,,\r\n"
        );
        same_bytes(image.memory(), &before);
    }
    #[test]
    fn history_export_rejects_dd_alias_and_invalid_range_before_output() {
        let (image, defaults) = history_image();
        for kind in [0x0e, 0x11, 0x13, 0x14, 0x15] {
            let out = export_history(&image, kind, None, Locale::copied(b'.', b','), defaults);
            assert!(out.failure.is_some());
            assert!(out.bytes.is_empty());
        }
        let out = export_history(
            &image,
            0x0f,
            Some(Selection {
                first: 0,
                last: 10,
                number_base: 0,
            }),
            Locale::copied(b'.', b','),
            defaults,
        );
        assert!(out.failure.is_some());
        assert!(out.bytes.is_empty());
    }
    #[test]
    fn scan_root_reaches_typed_bitmap_raw27_and_literal_export_with_native_counts() {
        let mut image = scan_image();
        let before = image.memory().to_vec();
        let selection = Selection {
            first: 0,
            last: 24,
            number_base: 0,
        };
        let out = import_scan(&mut image,
            b"Name,Frequency Low,Frequency High,Mode,DATA,Filter\n12345678901234567,2,1,Auto,ON,1\nBAD,0.029999,1,FM,ON,2\nREVERSE,4,3,FM,ON,2\n",
            selection, true, Locale::copied(b'.', b','), None);
        assert!(out.failure.is_none(), "{:?}", out.failure);
        assert_eq!(
            (out.rows_read, out.applied, out.counts),
            (
                3,
                2,
                Counts {
                    s: 0,
                    c2: 1,
                    c3: 2,
                    c4: 2
                }
            )
        );
        assert_eq!(
            &image.memory()[0xa368..0xa373],
            &[0x40, 0x42, 0x0f, 0, 0x80, 0x84, 0x1e, 0, 0xff, 0, 0]
        );
        assert_eq!(&image.memory()[0xa373..0xa383], b"1234567890123456");
        assert_eq!(image.memory()[0xa60b], 0xfc);
        same_bytes(&image.memory()[..0xa368], &before[..0xa368]);
        same_bytes(&image.memory()[0xa39e..0xa60b], &before[0xa39e..0xa60b]);
        same_bytes(&image.memory()[0xa60c..], &before[0xa60c..]);
        assert_eq!(export_scan(&image,Selection{last:1,..selection},Locale::copied(b'.',b',')).bytes,
            b"No,Name,Frequency Low,Frequency High,Mode,DATA,Filter\r\n00,1234567890123456,2.000000,1.000000,-,OFF,1\r\n01,REVERSE,4.000000,3.000000,FM,ON,2\r\n");
        let mut full = b"Frequency Low,Frequency High\n".to_vec();
        for _ in 0..26 {
            full.extend_from_slice(b"1,2\n");
        }
        let out = import_scan(
            &mut image,
            &full,
            selection,
            true,
            Locale::copied(b'.', b','),
            None,
        );
        assert!(out.failure.is_none());
        assert_eq!(
            (out.rows_read, out.applied, out.counts),
            (
                26,
                25,
                Counts {
                    s: 2,
                    c2: 0,
                    c3: 0,
                    c4: 25
                }
            )
        );
    }
    #[test]
    fn scan_root_post_uses_old_selected_range_and_does_not_run_after_late_error() {
        for (selection, selected_after) in [
            (
                Selection {
                    first: 3,
                    last: 5,
                    number_base: 0,
                },
                0x83,
            ),
            (
                Selection {
                    first: 4,
                    last: 4,
                    number_base: 0,
                },
                0x97,
            ),
        ] {
            let mut image = scan_image();
            image
                .patch_memory(&[
                    (0x37802, &[0x97]),
                    (0xa60b, &[0xfe]),
                    (0xa368, &[0x80, 0x38, 1, 0, 0x70, 0x11, 1, 0, 0xff, 0, 0xff]),
                    (0xa373, b"OUTSIDE         "),
                ])
                .unwrap();
            let outside = image.memory()[0xa368..0xa383].to_vec();
            let out = import_scan(
                &mut image,
                b"No,Name,Frequency Low,Frequency High\n4,NEW,1,2\n",
                selection,
                false,
                Locale::copied(b'.', b','),
                None,
            );
            assert!(out.failure.is_none());
            assert_eq!((out.applied, out.counts.c4), (1, 1));
            assert_eq!(image.memory()[0x37802], selected_after);
            same_bytes(&image.memory()[0xa368..0xa383], &outside);
        }
        let mut image = scan_image();
        image.patch_memory(&[(0x37802, &[0x97])]).unwrap();
        let mut source = b"No,Frequency Low,Frequency High\n5,1,2\n5,".to_vec();
        source.extend(std::iter::repeat_n(b'1', 4097));
        source.extend_from_slice(b",2\n");
        let out = import_scan(
            &mut image,
            &source,
            Selection {
                first: 5,
                last: 5,
                number_base: 0,
            },
            false,
            Locale::copied(b'.', b','),
            None,
        );
        assert!(matches!(out.failure, Some(Failure::Text(Error::Limit(_)))));
        assert_eq!((out.rows_read, out.applied, out.counts.c4), (2, 1, 1));
        assert_eq!(image.memory()[0x37802], 0x97);
        assert_eq!(export_scan(&image,Selection{first:5,last:5,number_base:0},Locale::copied(b'.',b',')).bytes,
            b"No,Name,Frequency Low,Frequency High,Mode,DATA,Filter\r\n05,,1.000000,2.000000,-,OFF,-\r\n");
    }
    #[test]
    fn scan_root_conditional_g_failure_keeps_selection_flag_prefix_and_header_still_gates_clear() {
        let setup = || {
            let mut image = scan_image();
            image
                .patch_memory(&[
                    (0x37802, &[0x97]),
                    (0x37807, &[0x81]),
                    (0x37806, &[8]),
                    (0x3a8ca, &[0]),
                    (0x3a8c9, &[0]),
                    (0x3c84a, &[0]),
                ])
                .unwrap();
            image
        };
        let selection = Selection {
            first: 5,
            last: 5,
            number_base: 0,
        };
        let mut image = setup();
        let before = image.memory().to_vec();
        let out = import_scan(
            &mut image,
            b"Frequency Low\n1\n",
            selection,
            false,
            Locale::copied(b'.', b','),
            None,
        );
        assert!(out.failure.is_some());
        same_bytes(image.memory(), &before);
        let source = b"Frequency Low,Frequency High\n";
        let out = import_scan(
            &mut image,
            source,
            selection,
            false,
            Locale::copied(b'.', b','),
            None,
        );
        assert!(matches!(
            out.failure,
            Some(Failure::Image(icf::Error::Invalid { .. }))
        ));
        assert_eq!(
            (out.rows_read, out.applied, out.counts),
            (0, 0, Counts::default())
        );
        assert_eq!(
            (image.memory()[0x37802], image.memory()[0x37807]),
            (0x83, 0x80)
        );
        assert_eq!(&image.memory()[0x37810..0x37814], &before[0x37810..0x37814]);
        let mut defaults = scan_image();
        defaults
            .patch_memory(&[(0x37810, &[0x78, 0x56, 0x34, 0x12])])
            .unwrap();
        let mut image = setup();
        let out = import_scan(
            &mut image,
            source,
            selection,
            false,
            Locale::copied(b'.', b','),
            Some(&defaults),
        );
        assert!(out.failure.is_none());
        assert_eq!(&image.memory()[0x37810..0x37814], &[0x78, 0x56, 0x34, 0x12]);
        let mut image = setup();
        image.patch_memory(&[(0x3a8ca, &[1])]).unwrap();
        let out = import_scan(
            &mut image,
            source,
            selection,
            false,
            Locale::copied(b'.', b','),
            None,
        );
        assert!(out.failure.is_none());
        assert_eq!(
            (image.memory()[0x37802], image.memory()[0x37807]),
            (0x83, 0x81)
        );
    }
    #[test]
    fn scan_reader_uses_native_type1f_copy_finalizer_and_compressed_correction_count() {
        let selection = Selection {
            first: 0,
            last: 24,
            number_base: 0,
        };
        let seed = scan_seed();
        let mut rows = ScanRows::new(b"No,Name,Frequency Low,Frequency High,Mode,DATA,Filter\n0,12345678901234567,2,1,Auto,ON,1\n1,BLOCKED,1,2,WFM,ON,1\n2,HOLD,1,2,,,\n", Locale::copied(b'.', b','), selection, true).unwrap();
        let row = rows.next_with(&seed, b'.').unwrap().unwrap();
        assert!(row.holder.valid());
        assert_eq!(
            (row.holder.kind(), row.holder.index(), row.holder.binding()),
            (8, 0, 0x12345678)
        );
        assert_eq!(row.holder.name(), b"1234567890123456");
        assert_eq!(
            (
                row.holder.mode(),
                row.holder.data(),
                row.holder.filter(),
                row.discard,
                row.corrections
            ),
            (25, 0, 0, 0, 2)
        );
        assert_eq!(
            row.holder.channel_field(ChannelField::FrequencyHz),
            1_000_000
        );
        assert_eq!(
            &row.holder.as_bytes()[0x138..0x13c],
            &2_000_000_u32.to_le_bytes()
        );
        let blocked = rows.next_with(&seed, b'.').unwrap().unwrap();
        assert_eq!(
            (
                blocked.holder.mode(),
                blocked.holder.data(),
                blocked.holder.filter(),
                blocked.corrections
            ),
            (15, 0, 255, 1)
        );
        let hold = rows.next_with(&seed, b'.').unwrap().unwrap();
        assert_eq!(
            (
                hold.holder.mode(),
                hold.holder.data(),
                hold.holder.filter(),
                hold.corrections
            ),
            (25, 0, 255, 0)
        );
        assert!(rows.next_with(&seed, b'.').is_none());
        assert_eq!((seed.mode(), seed.data(), seed.filter()), (25, 0, 255));
    }
    #[test]
    fn scan_reader_keeps_blank_and_out_of_range_no_distinct_from_discard_and_applied_cursor() {
        let selection = Selection {
            first: 0,
            last: 24,
            number_base: 0,
        };
        let seed = scan_seed();
        for header in [
            b"\xef\xbb\xbfFrequency Low,Frequency High\n1,2\n".as_slice(),
            b"Frequency Low\n1\n",
        ] {
            assert!(ScanRows::new(header, Locale::copied(b'.', b','), selection, true).is_err());
        }
        let mut rows = ScanRows::new(
            b"Frequency Low,Frequency High\n,2\n1,\n0.029999,1\n470,0.030000\n1,2\n",
            Locale::copied(b'.', b','),
            selection,
            true,
        )
        .unwrap();
        for discard in [0, 0, 1] {
            let row = rows.next_with(&seed, b'.').unwrap().unwrap();
            assert!(!row.holder.valid());
            assert_eq!(
                (row.position, row.discard, row.corrections),
                (0, discard, 0)
            );
        }
        let row = rows.next_with(&seed, b'.').unwrap().unwrap();
        assert!(row.holder.valid());
        assert_eq!(row.position, 0);
        rows.applied();
        assert_eq!(rows.next_with(&seed, b'.').unwrap().unwrap().position, 1);
        let mut selected = ScanRows::new(
            b"No,Frequency Low,Frequency High\n$FFFFFFFF,1,2\n2147483648,1,2\n0,1,2\n",
            Locale::copied(b'.', b','),
            selection,
            false,
        )
        .unwrap();
        for position in [u32::MAX, 0xffff] {
            let row = selected.next_with(&seed, b'.').unwrap().unwrap();
            assert_eq!(
                (row.position, row.discard, row.corrections),
                (position, 0, 0)
            );
            assert!(!row.holder.valid());
        }
        assert!(
            selected
                .next_with(&seed, b'.')
                .unwrap()
                .unwrap()
                .holder
                .valid()
        );
    }
    #[test]
    fn scan_reader_preserves_dbcs_clipping_and_late_limit_error_without_seed_fallback() {
        let selection = Selection {
            first: 0,
            last: 24,
            number_base: 0,
        };
        let seed = scan_seed();
        let mut text =
            b"Name,Frequency Low,Frequency High\naaaaaaaaaaaaaaa\x81\x41,1,2\nNEXT,".to_vec();
        text.extend(std::iter::repeat_n(b'1', 4097));
        text.extend_from_slice(b",2\n");
        let mut rows = ScanRows::new(&text, Locale::copied(b'.', b','), selection, true).unwrap();
        let row = rows.next_with(&seed, b'.').unwrap().unwrap();
        assert_eq!(row.holder.name(), b"aaaaaaaaaaaaaaa ");
        assert_eq!(row.corrections, 2);
        rows.applied();
        assert!(matches!(
            rows.next_with(&seed, b'.'),
            Some(Err(Error::Limit(_)))
        ));
        let mut rows = ScanRows::new(
            b"Frequency Low,Frequency High\n1,2\n",
            Locale::copied(b'.', b','),
            selection,
            true,
        )
        .unwrap();
        assert!(matches!(
            rows.next_with(&record::reset_holder(8, 0, 0), b'.'),
            Some(Err(Error::Invalid(_)))
        ));
    }
    #[test]
    fn ur_root_expands_nonidentity_order_and_selected_parent_really_deletes_index_zero() {
        let mut image = ur_image();
        for (index, name, call) in [
            (0, b"ONE".as_slice(), b"A1      "),
            (1, b"TWO".as_slice(), b"B2      "),
            (2, b"THREE".as_slice(), b"C3      "),
        ] {
            ur_row(&mut image, index, name, call);
        }
        image
            .patch_memory(&[(0x3c0a7, &[2, 0, 0, 0, 1, 0])])
            .unwrap();
        let out = export_callsign(&image, None, Locale::copied(b'.', b','));
        assert_eq!(
            out.bytes,
            b"No,Name,Call Sign\r\n01,THREE,C3\r\n02,ONE,A1\r\n03,TWO,B2\r\n"
        );
        let mut defaults = ur_image();
        defaults
            .patch_memory(&[(0x3c901, &0x1234_u16.to_le_bytes())])
            .unwrap();
        let out = import_callsign(
            &mut image,
            b"Call Sign,Name\nnew1,NEW\n",
            Some(Selection {
                first: 1,
                last: 1,
                number_base: 1,
            }),
            Locale::copied(b'.', b','),
            &defaults,
        );
        assert!(out.failure.is_none());
        assert_eq!((out.applied, out.counts.c3, out.counts.c4), (1, 2, 1));
        assert_eq!(&image.memory()[0x3c901..0x3c903], &0x1234_u16.to_le_bytes());
        let out = export_callsign(&image, None, Locale::copied(b'.', b','));
        assert_eq!(
            out.bytes,
            b"No,Name,Call Sign\r\n01,NEW,NEW1\r\n02,ONE,A1\r\n03,TWO,B2\r\n"
        );
    }
    #[cfg(not(windows))]
    #[test]
    fn ur_nls_failure_retains_cleared_prefix_and_original_counts_without_header_rollback() {
        let mut image = ur_image();
        let defaults = ur_image();
        ur_row(&mut image, 0, b"OLD", b"OLD     ");
        image.patch_memory(&[(0x3c0a7, &[0, 0])]).unwrap();
        let before = image.memory().to_vec();
        let no = import_callsign(
            &mut image,
            b"\xef\xbb\xbfCall Sign\nA\n",
            None,
            Locale::copied(b'.', b','),
            &defaults,
        );
        assert!(no.failure.is_some());
        same_bytes(image.memory(), &before);
        let out = import_callsign(
            &mut image,
            b"Call Sign,Name\nA1,FIRST\n   ,SHOULD_SKIP\n\xa7,FAIL\n",
            None,
            Locale::copied(b'.', b','),
            &defaults,
        );
        assert!(matches!(
            out.failure,
            Some(Failure::Text(Error::SourceUnavailable(_)))
        ));
        assert_eq!(
            (out.rows_read, out.applied, out.counts),
            (
                3,
                1,
                Counts {
                    s: 0,
                    c2: 1,
                    c3: 0,
                    c4: 1
                }
            )
        );
        assert_eq!(
            export_callsign(&image, None, Locale::copied(b'.', b',')).bytes,
            b"No,Name,Call Sign\r\n01,FIRST,A1\r\n"
        );
        assert_eq!(&image.memory()[0x10000..0x10020], &before[0x10000..0x10020]);
        let out = import_callsign(
            &mut image,
            b"Call Sign\n",
            None,
            Locale::copied(b'.', b','),
            &defaults,
        );
        assert!(out.failure.is_none());
        assert_eq!(out.counts, Counts::default());
        assert_eq!(
            export_callsign(&image, None, Locale::copied(b'.', b',')).bytes,
            b"No,Name,Call Sign\r\n"
        );
        let mut tail = record::reset_holder(0x0d, 299, 0);
        record::load_callsign_storage_into(&image, 299, &mut tail).unwrap();
        assert_eq!(tail.callsign(MemoryCallsign::Your), b"CALLSIGN");
    }

    #[test]
    fn ur_default_platform_nls_consumer() {
        let mut image = ur_image();
        let defaults = ur_image();
        let before = image.memory().to_vec();
        let no = import_callsign(
            &mut image,
            b"Name\nIGNORED\n",
            None,
            Locale::copied(b'.', b','),
            &defaults,
        );
        assert!(no.failure.is_some());
        same_bytes(image.memory(), &before);
        let out = import_callsign(
            &mut image,
            b"Call Sign,Name\n\x82\x60\x82\x61\x82\x50,MAPPED\n",
            None,
            Locale::copied(b'.', b','),
            &defaults,
        );
        #[cfg(windows)]
        {
            assert!(out.failure.is_none(), "{:?}", out.failure);
            assert_eq!(out.applied, 1);
            assert_eq!(
                export_callsign(&image, None, Locale::copied(b'.', b',')).bytes,
                b"No,Name,Call Sign\r\n01,MAPPED,AB1\r\n"
            );
        }
        #[cfg(not(windows))]
        {
            assert!(matches!(
                out.failure,
                Some(Failure::Text(Error::SourceUnavailable(_)))
            ));
            assert_eq!(out.applied, 0);
        }
    }
    #[test]
    fn ur_capacity_overwrites_c4_marker_not_applied_prefix() {
        let mut image = ur_image();
        let defaults = ur_image();
        let mut source = b"Call Sign\n".to_vec();
        for _ in 0..301 {
            source.extend_from_slice(b"A1\n");
        }
        let out = import_callsign(
            &mut image,
            &source,
            None,
            Locale::copied(b'.', b','),
            &defaults,
        );
        assert!(out.failure.is_none());
        assert_eq!(
            (out.rows_read, out.applied, out.counts),
            (
                301,
                300,
                Counts {
                    s: 1,
                    c2: 0,
                    c3: 0,
                    c4: 1
                }
            )
        );
        assert_eq!(
            export_callsign(&image, None, Locale::copied(b'.', b',')).rows,
            300
        );
    }
    #[test]
    fn user_edge_real_root_has_literal_output_reverse_order_and_no_blank_discard() {
        let mut image = image();
        let selection = Selection {
            first: 0,
            last: 1,
            number_base: 1,
        };
        let out = import_user_edge(
            &mut image,
            b"Frequency Low;Frequency High\r2.000000;1.000000\r;3\r4;\r",
            selection,
            true,
            Locale::copied(b'.', b';'),
        );
        assert!(out.failure.is_none());
        assert_eq!(
            (out.rows_read, out.applied, out.counts),
            (
                3,
                1,
                Counts {
                    s: 0,
                    c2: 0,
                    c3: 0,
                    c4: 1
                }
            )
        );
        assert_eq!(
            &image.memory()[0x378c0..0x378c8],
            &[0x80, 0x84, 0x1e, 0, 0x40, 0x42, 0x0f, 0]
        );
        let out = export_user_edge(&image, selection, Locale::copied(b'.', b';'));
        assert!(out.failure.is_none());
        assert_eq!(
            out.bytes,
            b"No;Frequency Low;Frequency High\r\n01;2.000000;1.000000\r\n02;;\r\n"
        );
        assert_eq!(out.rows, 2);
        let missing = import_user_edge(
            &mut image,
            b"Frequency Low\n9\n",
            selection,
            true,
            Locale::copied(b'.', b','),
        );
        assert!(missing.failure.is_some());
        assert_eq!(
            &image.memory()[0x378c0..0x378c8],
            &[0x80, 0x84, 0x1e, 0, 0x40, 0x42, 0x0f, 0]
        );
    }
    #[test]
    fn user_edge_selected_numbers_clear_only_selection_and_keep_prefix_on_late_error() {
        let mut image = image();
        let whole = Selection {
            first: 0,
            last: 29,
            number_base: 1,
        };
        assert!(
            import_user_edge(
                &mut image,
                b"No,Frequency Low,Frequency High\n1,1,2\n2,3,4\n3,5,6\n",
                whole,
                true,
                Locale::copied(b'.', b',')
            )
            .failure
            .is_none()
        );
        let before = image.memory().to_vec();
        let selection = Selection {
            first: 1,
            last: 1,
            number_base: 1,
        };
        let mut text = b"No,Frequency Low,Frequency High\n2,7,8\n2,".to_vec();
        text.extend(std::iter::repeat_n(b'1', 4097));
        text.extend_from_slice(b",9\n");
        let out = import_user_edge(
            &mut image,
            &text,
            selection,
            false,
            Locale::copied(b'.', b','),
        );
        assert_eq!((out.rows_read, out.applied, out.counts.c4), (2, 1, 1));
        assert!(matches!(out.failure, Some(Failure::Text(Error::Limit(_)))));
        same_bytes(&image.memory()[..0x378c8], &before[..0x378c8]);
        same_bytes(&image.memory()[0x378d0..], &before[0x378d0..]);
        let out = export_user_edge(&image, selection, Locale::copied(b'.', b','));
        assert_eq!(
            out.bytes,
            b"No,Frequency Low,Frequency High\r\n02,7.000000,8.000000\r\n"
        );
        let out = import_user_edge(
            &mut image,
            b"Frequency Low,Frequency High\n1,2\n",
            selection,
            false,
            Locale::copied(b'.', b','),
        );
        assert!(out.failure.is_none());
        assert_eq!((out.applied, out.counts), (0, Counts::default()));
        assert_eq!(&image.memory()[0x378c8..0x378d0], &[0; 8]);
    }
    #[test]
    fn user_edge_capacity_invalid_number_and_numeric_boundaries_have_native_counts() {
        let mut image = image();
        let selection = Selection {
            first: 0,
            last: 29,
            number_base: 1,
        };
        let source =
            b"Frequency Low,Frequency High\n0.029999,1\n0.030000,470\n1,470.000001\n-1,1\n";
        let out = import_user_edge(
            &mut image,
            source,
            selection,
            true,
            Locale::copied(b'.', b','),
        );
        assert_eq!(
            (out.applied, out.counts),
            (
                1,
                Counts {
                    s: 0,
                    c2: 3,
                    c3: 0,
                    c4: 1
                }
            )
        );
        let mut full = b"Frequency Low,Frequency High\n".to_vec();
        for _ in 0..31 {
            full.extend_from_slice(b"1,2\n");
        }
        let out = import_user_edge(
            &mut image,
            &full,
            selection,
            true,
            Locale::copied(b'.', b','),
        );
        assert_eq!(
            (out.rows_read, out.applied, out.counts),
            (
                31,
                30,
                Counts {
                    s: 2,
                    c2: 0,
                    c3: 0,
                    c4: 30
                }
            )
        );
        let out = import_user_edge(
            &mut image,
            b"No,Frequency Low,Frequency High\n65535,1,2\n2147483648,1,2\n1,3,4\0bad",
            selection,
            true,
            Locale::copied(b'.', b','),
        );
        assert_eq!(
            (out.rows_read, out.applied, out.counts),
            (
                3,
                1,
                Counts {
                    s: 0,
                    c2: 0,
                    c3: 0,
                    c4: 1
                }
            )
        );
        let mut rows = UserEdgeRows::new(
            b"No,Frequency Low,Frequency High\n$FFFFFFFF,1,2\n",
            Locale::copied(b'.', b','),
            selection,
            true,
        )
        .unwrap();
        let negative = rows.next_row().unwrap().unwrap();
        assert_eq!(
            (negative.position, negative.discard, negative.corrections),
            (u32::MAX, 0, 0)
        );
        assert!(negative.holder.valid());
        let before = image.memory()[..0x378c0].to_vec();
        let out = import_user_edge(
            &mut image,
            b"No,Frequency Low,Frequency High\n$FFFFFFFF,1,2\n1,3,4\n",
            selection,
            true,
            Locale::copied(b'.', b','),
        );
        // Native 945A4 returns below-range before clearing 5F64's valid 1. The
        // downstream unsigned index would address before the pair table. The
        // shared safe image adapter rejects that invalid index visibly instead
        // of silently skipping it or touching an unrelated image window.
        assert_eq!((out.rows_read, out.applied), (1, 0));
        assert!(matches!(
            out.failure,
            Some(Failure::Image(icf::Error::Invalid { .. }))
        ));
        same_bytes(&image.memory()[..0x378c0], &before);
    }

    #[test]
    fn scan_export_uses_bitmap_raw27_and_static_labels_without_channel_masking() {
        let mut image = image();
        image
            .patch_memory(&[
                (0xa60b, &[0xfe, 0xff, 0xff, 0xff]),
                (
                    0xa368,
                    &[0x80, 0x84, 0x1e, 0, 0x40, 0x42, 0x0f, 0, 7, 0x81, 0xff],
                ),
                (0xa373, b"RAW,NAME;       "),
            ])
            .unwrap();
        let before = image.memory().to_vec();
        let out = export_scan(
            &image,
            Selection {
                first: 0,
                last: 1,
                number_base: 0,
            },
            Locale::copied(b',', b';'),
        );
        assert!(out.failure.is_none());
        assert_eq!(out.rows, 2);
        assert_eq!(out.bytes, b"No;Name;Frequency Low;Frequency High;Mode;DATA;Filter\r\n00;RAW NAME;1,000000;2,000000;FM;NOT_FOUND;-\r\n01;;;;;;\r\n");
        same_bytes(image.memory(), &before);
        let out = export_scan(
            &image,
            Selection {
                first: 24,
                last: 24,
                number_base: 0,
            },
            Locale::copied(b'.', b','),
        );
        assert_eq!(
            out.bytes,
            b"No,Name,Frequency Low,Frequency High,Mode,DATA,Filter\r\n24,,,,,,\r\n"
        );
        let out = export_scan(
            &image,
            Selection {
                first: 0,
                last: 25,
                number_base: 0,
            },
            Locale::copied(b'.', b','),
        );
        assert!(out.failure.is_some());
        assert!(out.bytes.is_empty());
    }
    #[test]
    fn ur_reader_uses_shared_header_byte_grammar_and_original_correction_count() {
        let locale = Locale::copied(b'.', b';');
        assert!(CallsignRows::new(b"\xef\xbb\xbfCall Sign\nA\n", locale).is_err());
        assert!(CallsignRows::new(b"Name\nA\n", locale).is_err());
        let mut rows = CallsignRows::new(
            b"Name;Call Sign\r12345678901234567;ab1/c\rIGNORED;   \rTAIL;d2\0;ignored",
            locale,
        )
        .unwrap();
        let seed = record::reset_holder(0x23, 0, 0x12345678);
        let row = rows.next_with(&seed, nls::ascii_map).unwrap().unwrap();
        assert!(row.accepted && row.holder.valid());
        assert_eq!(row.holder.name(), b"1234567890123456");
        assert_eq!(row.holder.callsign(MemoryCallsign::Your), b"AB1/C   ");
        assert_eq!(row.corrections, Some(3));
        let blank = rows.next_with(&seed, nls::ascii_map).unwrap().unwrap();
        assert!(!blank.accepted && !blank.holder.valid());
        assert_eq!(blank.corrections, None);
        assert_eq!(blank.holder.name(), seed.name());
        let last = rows.next_with(&seed, nls::ascii_map).unwrap().unwrap();
        assert_eq!(last.holder.callsign(MemoryCallsign::Your), b"D2      ");
        assert!(rows.next_with(&seed, nls::ascii_map).is_none());
        let mut short = CallsignRows::new(b"Name;Call Sign\nSHORT\n", locale).unwrap();
        assert_eq!(
            short
                .next_with(&seed, nls::ascii_map)
                .unwrap()
                .unwrap()
                .corrections,
            None
        );
    }
    #[test]
    fn ur_nls_is_not_a_silent_unicode_or_unknown_byte_fallback() {
        let mut rows = CallsignRows::new(b"Call Sign\n\xa7\n", Locale::copied(b'.', b',')).unwrap();
        assert!(matches!(
            rows.next_with(&record::reset_holder(0x23, 0, 0), nls::ascii_map),
            Some(Err(Error::SourceUnavailable(_)))
        ));
        for bad in [0, 1, 4, 5, 7, 14, 0x15_0000] {
            assert!(Kind::from_context(bad).is_err());
        }
        assert_eq!(Kind::from_context(21), Ok(Kind::UserEdge));
    }
}
