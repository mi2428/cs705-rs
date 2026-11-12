//! Native normal-memory operations over the parent's one map and typed codec.
//! Indices in Selection are local typed sides, not physical slots or grid rows.
//! Effects describe requests; this module does not claim native GUI delivery.

use super::{Error, NormalMemory, ReferenceImage, invalid, put, raw};
use crate::icf::{
    record::{self, ChannelField, ChannelWrite, DecodedRecord},
    record_image::{self, ChannelKind},
};

pub mod call;

#[derive(Debug, Clone, Copy)]
pub struct Selection {
    pub group: usize,
    pub active: usize,
    pub first: usize,
    pub last: usize,
    pub merged: bool,
    pub show_both: bool,
}
impl Selection {
    fn checked(self, model: &NormalMemory) -> Result<Self, Error> {
        if self.active >= 200 || self.first > self.last || self.last >= 200 {
            return Err(invalid("native normal side selection outside0..200"));
        }
        model.range(self.group, self.first / 2..self.last / 2 + 1)?;
        Ok(self)
    }
    fn pair(self) -> (usize, usize) {
        (self.first & !1, self.last | 1)
    }
    fn typed(self, side: usize) -> u32 {
        (200 * self.group + side) as u32
    }
}

/// A060 mode, N, M and the actual decoded snapshots remain distinct.
#[derive(Clone, Default)]
pub struct Clipboard {
    pub source_kind: u32,
    pub mode: u32,
    pub declared_count: usize,
    pub logical_count: usize,
    pub records: Vec<DecodedRecord>,
}
#[derive(Debug, Clone, Copy)]
pub struct CopySource {
    pub kind: u32,
    pub first: u32,
    pub count: usize,
    pub request: u32,
    pub expanded: bool,
    pub binding: u32,
    /// Actual 56DA4 result. Only a reached band-stack special branch reads it.
    pub band_enabled: Option<bool>,
}
pub fn compatible(source: u32, destination: u32) -> bool {
    (2..=7).contains(&source) && (2..=7).contains(&destination)
}
impl Clipboard {
    pub fn count(&self, paired: bool) -> usize {
        if paired {
            if self.mode == 0 {
                2 * self.declared_count
            } else {
                self.declared_count
            }
        } else {
            self.logical_count
        }
    }
    /// Copy mutates the live clipboard in native order. A later typed read
    /// failure retains K/N/M and the successfully loaded new prefix.
    pub fn copy_from(&mut self, image: &[u8], source: CopySource) -> Result<(), Error> {
        self.mode = match source.request {
            2 => 2,
            1 => {
                if source.expanded || source.kind == 6 {
                    1
                } else {
                    3
                }
            }
            _ => 0,
        };
        self.logical_count = 0;
        self.records.clear();
        self.source_kind = source.kind;
        self.declared_count = source.count;
        let restricted = if source.kind == 4
            && source.first < 27
            && u64::from(source.first) + source.count as u64 > 27
        {
            !source
                .band_enabled
                .ok_or(invalid("band copy requires the actual Only predicate"))?
        } else {
            false
        };
        if restricted {
            self.declared_count = self.declared_count.checked_sub(3).ok_or(invalid(
                "native band copy requests a negative allocation count",
            ))?;
        }
        for offset in 0..source.count {
            let index = source.first.wrapping_add(offset as u32);
            if restricted && index / 3 == 9 {
                continue;
            }
            let mut holder = record::reset_holder(source.kind, index, source.binding);
            record_image::load_into(image, source.kind, index, &mut holder)?;
            if source.kind == 6 && self.mode != 2 && holder.valid() {
                holder.set_split(1);
            }
            if self.mode == 2 {
                holder.as_mut_bytes()[0x24] = 0;
                holder.set_split(0);
            }
            if self.mode != 3 || holder.side() == 0 || holder.split() != 0 {
                self.logical_count += 1;
            }
            self.records.push(holder);
        }
        if self.records.len() != self.declared_count {
            return Err(invalid(
                "native clipboard allocation and read counts differ",
            ));
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), Error> {
        if !compatible(self.source_kind, 2)
            || self.mode > 3
            || self.declared_count != self.records.len()
        {
            return Err(invalid("native clipboard kind/mode/count is inconsistent"));
        }
        if self.records.iter().any(|h| h.kind() != self.source_kind) {
            return Err(invalid("native clipboard source header kind mismatch"));
        }
        let logical = self
            .records
            .iter()
            .filter(|h| self.mode != 3 || h.side() == 0 || h.split() != 0)
            .count();
        if logical != self.logical_count {
            return Err(invalid(
                "native clipboard logical count differs from snapshots",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    Overwrite,
    Truncate,
    Clear,
    Delete,
    DiscardTail,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Notification {
    pub id: u32,
    pub synchronous: bool,
    pub arg1: u32,
    pub arg2: u32,
}
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Effects {
    pub home_invalidated: u8,
    pub dr_invalidated: u8,
    pub context_flags: u8,
    pub selection: Option<(usize, usize)>,
    pub active: Option<usize>,
    pub display_mode: Option<u32>,
    pub notifications: Vec<Notification>,
    pub cache_invalidated: bool,
}
impl Effects {
    fn map_saved(&mut self, group: usize) {
        self.queue(0x3c, 1, 0);
        self.notify(0x39, 0, 0);
        self.notify(0x36, group as u32 + 2, 0);
    }
    fn queue(&mut self, id: u32, arg1: u32, arg2: u32) {
        self.notifications.push(Notification {
            id,
            synchronous: false,
            arg1,
            arg2,
        });
    }
    fn notify(&mut self, id: u32, arg1: u32, arg2: u32) {
        self.notifications.push(Notification {
            id,
            synchronous: true,
            arg1,
            arg2,
        });
    }
    fn selected(&mut self, first: usize, last: usize) {
        self.context_flags |= 4;
        self.selection = Some((first, last));
        self.display_mode = Some(1);
        self.cache_invalidated = true;
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub processed: bool,
    pub written: usize,
    pub native_count: usize,
    pub exhausted: bool,
}

/// Live scalar caller frame for 69977. These are not holder/body defaults.
/// Node flags may change while accepting a display request; pre_step is captured
/// before the editor and the final merge bit still uses that captured value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitFrame {
    pub layout: u32,
    pub pre_step: i32,
    pub selection_first: i32,
    pub node_index: i32,
    pub node_flags: u8,
    pub notify_flags: u32,
    pub active: i32,
    pub display_mode: i32,
}
pub struct SplitRequest<'a> {
    pub input: crate::icf::settings::extended::ChoiceInput,
    pub context: EditorContext<'a>,
    pub frame: &'a mut SplitFrame,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitOutcome {
    pub stored: bool,
    pub candidate: Option<i32>,
    pub popup_requested: bool,
}

/// The four 9A67C/9A6C4 channel fields with fixed key 1 catalogs. This engine is
/// shared with call consumers; call SEL is deliberately not a writable field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelChoice {
    Selection,
    Duplex,
    Polarity,
    DigitalSquelch,
}

// B5844's mode predicate is shared by input and normal paint. Gray styling and
// empty/out-of-band entry redirection are separate caller operations.
pub(crate) fn channel_mode_blocked(h: &DecodedRecord, key: u32) -> bool {
    h.valid()
        && match key {
            0x109 => !matches!(h.mode(), 0 | 1 | 8 | 13),
            0x10a => matches!(h.mode() as u8, 15 | 17),
            0x10d..=0x111 => !matches!(h.mode(), 13 | 14),
            0x112 | 0x113 => h.mode() != 16,
            _ => false,
        }
}
impl ChannelChoice {
    pub fn key(self) -> u32 {
        match self {
            Self::Selection => 0x117,
            Self::Duplex => 0x10b,
            Self::Polarity => 0x111,
            Self::DigitalSquelch => 0x112,
        }
    }
    fn field(self) -> ChannelField {
        match self {
            Self::Selection => ChannelField::Selection,
            Self::Duplex => ChannelField::Duplex,
            Self::Polarity => ChannelField::Polarity,
            Self::DigitalSquelch => ChannelField::DigitalSquelch,
        }
    }
    pub fn selector(self) -> &'static str {
        match self {
            Self::Selection => "GI_Sheet_Sel_Mch_Scan_Sel",
            Self::Duplex => "GI_Sheet_Sel_Mch_Dup",
            Self::Polarity => "GI_Sheet_Sel_Mch_DTCS_Polarity",
            Self::DigitalSquelch => "GI_Sheet_Sel_Mch_DSQL",
        }
    }
    pub fn choices(self, h: &DecodedRecord) -> Vec<i64> {
        let values: &[i64] = match self {
            Self::Selection => &[0, 1, 2, 3],
            Self::Duplex => &[0, 1, 2, 3, 4],
            Self::Polarity => &[2, 3, 4, 5],
            Self::DigitalSquelch => &[0, 2, 4],
        };
        let old = i64::from(h.channel_field(self.field()));
        values
            .iter()
            .copied()
            .filter(|&value| {
                self != Self::Duplex
                    || value == old
                    || if h.mode() == 0x11 {
                        !matches!(value, 1 | 2)
                            && !(matches!(old, 0 | 3) && value == 4 || old == 4 && value == 3)
                    } else {
                        !matches!(value, 3 | 4)
                    }
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceOutcome {
    pub stored: bool,
    pub candidate: Option<i32>,
    pub mode_blocked: bool,
    pub popup_requested: bool,
}

/// Channel field body only. The original full DWORD survives selection until
/// the shared writer/readback applies each field's own projection. No gray gate.
pub fn channel_choice_input(
    h: &mut DecodedRecord,
    field: ChannelChoice,
    input: crate::icf::settings::extended::ChoiceInput,
) -> Result<ChoiceOutcome, Error> {
    use crate::icf::settings::extended::{ChoiceInput, choice_candidate};
    if !matches!(h.kind(), 2 | 3) || field == ChannelChoice::Selection && h.kind() != 2 {
        return Err(invalid(
            "channel choice requires normal/call; call SEL is read-only",
        ));
    }
    let mode_blocked = channel_mode_blocked(h, field.key());
    let candidate = if mode_blocked {
        None
    } else {
        choice_candidate(
            i64::from(h.channel_field(field.field())),
            &field.choices(h),
            input,
        )
    };
    if let Some(candidate) = candidate {
        h.set_channel_field(field.field(), candidate as u32);
    }
    Ok(ChoiceOutcome {
        stored: candidate.is_some(),
        candidate,
        mode_blocked,
        popup_requested: !mode_blocked && input == ChoiceInput::Begin,
    })
}

/// Only live external grid inputs; the original pre-step is produced from the
/// holder and layout before editing, never supplied as a completed frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChoiceFrame {
    pub layout: u32,
    pub node_flags: u8,
}

/// Actual caller scalars used by 642F9, before any field editor runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelCaller {
    pub code: i32,
    pub command: u32,
    pub blocked: bool,
    pub node_index: i32,
    pub node_back: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelEntry {
    Edit,
    Blocked,
    Redirect { key: u32, index: i32, code: i32 },
}

/// Empty/out-of-band ordinary gestures move to frequency; they do not turn a
/// sibling field value into an already accepted frequency or invoke its saver.
pub fn channel_entry(
    h: &DecodedRecord,
    key: u32,
    caller: ChannelCaller,
    callsign_dialog: bool,
) -> Result<ChannelEntry, Error> {
    if !matches!(h.kind(), 2 | 3) {
        return Err(invalid("channel entry requires a normal/call holder"));
    }
    if caller.blocked || caller.command == 0xdc {
        return Ok(ChannelEntry::Blocked);
    }
    if matches!(caller.command, 0xd1 | 0xd2) && matches!(key, 0x114..=0x116) {
        return Ok(ChannelEntry::Edit);
    }
    if (!h.valid() || h.channel_field(ChannelField::FrequencyHz) > 470_000_000)
        && (caller.code <= 0x7f || matches!(caller.code, 0x7100 | 0x10007))
        && !matches!(key, 0x105 | 0x107)
        && !(callsign_dialog && matches!(key, 0x114..=0x116))
    {
        return Ok(ChannelEntry::Redirect {
            key: 0x107,
            index: caller.node_index.wrapping_sub(caller.node_back),
            code: caller.code,
        });
    }
    Ok(ChannelEntry::Edit)
}
pub struct ChoiceRequest<'a> {
    pub field: ChannelChoice,
    pub input: crate::icf::settings::extended::ChoiceInput,
    pub context: EditorContext<'a>,
    pub selection: Option<&'a mut ChoiceFrame>,
}

pub struct NameRequest<'a> {
    pub input: crate::icf::settings::extended::TextInput<'a>,
    pub context: EditorContext<'a>,
    pub frame: &'a mut ChoiceFrame,
    pub notification: Option<[u8; 24]>,
}

/// 68C48..68C98: only the live grid node index is external. Normal/call
/// producers supply their node kind, selected group, zero auxiliary and type.
pub fn name_notification(h: &DecodedRecord, node_index: u32) -> Result<[u8; 24], Error> {
    if !matches!(h.kind(), 2 | 3) {
        return Err(invalid("name notification requires a normal/call holder"));
    }
    let mut payload = [0; 24];
    for (slot, word) in [h.kind() + 1, node_index, 0x105, h.group(), 0, h.kind()]
        .into_iter()
        .enumerate()
    {
        payload[4 * slot..4 * slot + 4].copy_from_slice(&word.to_le_bytes());
    }
    Ok(payload)
}
pub struct CallsignTextRequest<'a> {
    pub field: crate::icf::MemoryCallsign,
    pub input: crate::icf::settings::extended::TextInput<'a>,
    pub right_word: bool,
    /// Actual 17FFCC full-EAX result for the signed input byte, not LCMapStringA.
    pub upper: &'a mut dyn FnMut(i32) -> Result<i32, Error>,
    pub context: EditorContext<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallsignInline {
    pub state_out: u32,
    pub display: Vec<u8>,
    pub cursor: usize,
    pub selection: usize,
    pub width: usize,
}

/// 9C44A..9C5A9's inline producer, after the caller has selected this branch.
/// Enter with the dialog option disabled is not the separate command D1 list.
pub fn callsign_inline(
    h: &DecodedRecord,
    field: crate::icf::MemoryCallsign,
    code: i32,
) -> Result<CallsignInline, Error> {
    if !matches!(code,13|0x7100|32|8|47|48..=57|65..=90|97..=122) {
        return Err(invalid(
            "callsign Begin code does not request an inline editor",
        ));
    }
    let mut display = h
        .callsign(field)
        .split(|b| *b == 0)
        .next()
        .expect("fixed callsign prefix")
        .trim_ascii_end()
        .to_vec();
    let (cursor, selection) = if matches!(code, 13 | 0x7100 | 32 | 8) {
        (0, display.len())
    } else {
        display.push(code as u8);
        (display.len(), 0)
    };
    Ok(CallsignInline {
        state_out: 2,
        display,
        cursor,
        selection,
        width: 8,
    })
}
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NameEffect {
    pub stored: bool,
    pub pending_requested: Option<[u8; 24]>,
    pub state_out: u32,
    pub display: Vec<u8>,
    pub shared_input: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumericOutcome {
    pub candidate: u32,
    pub accepted: bool,
    pub state_out: u32,
}

/// A5134/A522C callers here have scale 6, quantum 1 and no SPACE/BACKSPACE step.
/// The shared helper owns filtering, eleven-byte truncation and signed rounding.
fn scaled_channel_input(
    input: crate::icf::settings::extended::NumberInput<'_>,
    old: u32,
    bounds: (u32, u32),
    point: u8,
) -> Result<NumericOutcome, Error> {
    use crate::icf::settings::extended::{NumberInput, numeric_text};
    let (candidate, accepted) = if let NumberInput::Text(text) = input {
        let (value, accepted) =
            numeric_text(text, point, 6, i64::from(bounds.0), i64::from(bounds.1), 1)?;
        (value as u32, accepted)
    } else {
        (old, false)
    };
    Ok(NumericOutcome {
        candidate,
        accepted,
        state_out: if input == NumberInput::Begin { 2 } else { 0 },
    })
}

fn frequency_first(
    h: &DecodedRecord,
    request: &FrequencyRequest<'_>,
) -> Result<NumericOutcome, Error> {
    let bounds = request.context.ranges.f760(h.kind(), -1, 0);
    scaled_channel_input(
        request.input,
        h.channel_field(ChannelField::FrequencyHz),
        (bounds.low, bounds.high),
        request.point,
    )
}
fn frequency_second(
    h: &DecodedRecord,
    first: u32,
    request: &FrequencyRequest<'_>,
) -> Result<(NumericOutcome, i32), Error> {
    let ranges = request.context.ranges;
    let index = if h.kind() == 3 {
        h.range_index()
    } else {
        ranges.edf0(h.kind(), first)
    };
    let bounds = ranges.f760(h.kind(), index, first);
    Ok((
        scaled_channel_input(
            request.input,
            first,
            (bounds.low, bounds.high),
            request.point,
        )?,
        index,
    ))
}
fn frequency_tail(
    h: &DecodedRecord,
    state: &EditorState,
    request: &FrequencyRequest<'_>,
    out: &mut FrequencyOutcome,
) -> Result<(), Error> {
    if out.first.state_out == 2 && !state.pending.bytes.is_empty() {
        out.focus = 1;
    }
    if crate::icf::frequency::channel_frequency_bad(h, request.context.ranges, request.live_q)? {
        out.warning_bits |= 4;
    }
    if crate::icf::frequency::channel_bad(h, request.context.ranges, request.live_q, true)? {
        out.warning_bits |= 8;
    }
    Ok(())
}

fn apply_frequency_candidate(
    h: &mut DecodedRecord,
    candidate: u32,
    pending: &mut PendingName,
) -> Result<(), Error> {
    let frequency = if candidate == 0 {
        h.channel_field(ChannelField::FrequencyHz)
    } else {
        candidate
    };
    if (candidate as i32) >= 0 {
        h.set_valid(true);
        h.set_channel_field(ChannelField::FrequencyHz, frequency);
        if pending.active && !pending.bytes.is_empty() {
            if pending.bytes.len() > 16 {
                return Err(invalid("pending name exceeds its accepted producer width"));
            }
            let mut name = [b' '; 16];
            name[..pending.bytes.len()].copy_from_slice(&pending.bytes);
            h.set_name(&name);
            pending.reset();
        }
    }
    Ok(())
}

fn name_candidate_input(
    h: &DecodedRecord,
    pending: &PendingName,
    input: crate::icf::settings::extended::TextInput<'_>,
    layout: u32,
    effect: &mut NameEffect,
) -> Result<Option<Vec<u8>>, Error> {
    use crate::icf::settings::extended::TextInput;
    *effect = NameEffect::default();
    let index = h
        .index()
        .wrapping_sub(u32::from(layout == 1 && h.side() != 0));
    effect.display = if pending.index == index {
        pending.bytes.clone()
    } else if h.valid() {
        h.name()
            .split(|&b| b == 0)
            .next()
            .expect("fixed name C prefix")
            .to_vec()
    } else {
        Vec::new()
    };
    match input {
        TextInput::Begin(first) => {
            effect.state_out = 2;
            if let Some(first) = first {
                effect.display = vec![first];
            }
            Ok(None)
        }
        TextInput::Text(source) => {
            effect.shared_input = source.to_vec();
            let candidate = crate::csv::nls::name_candidate(&mut effect.shared_input, 16)
                .map_err(|_| invalid("native name byte grammar failed"))?;
            effect.display = candidate.clone();
            Ok(Some(candidate))
        }
        TextInput::Display | TextInput::Cancel => Ok(None),
    }
}

/// 69401 has no split/gray write gate. Unsigned 100 Hz rounding is a separate
/// native 99F04 step; the common saver still applies its own raw upper bound.
pub fn channel_offset_input(
    h: &mut DecodedRecord,
    input: crate::icf::settings::extended::NumberInput<'_>,
    point: u8,
) -> Result<NumericOutcome, Error> {
    if !matches!(h.kind(), 2 | 3) {
        return Err(invalid("offset input requires a normal/call holder"));
    }
    let out = scaled_channel_input(input, h.offset_hz(), (0, 9_999_900), point)?;
    if out.accepted {
        h.set_offset_hz(crate::icf::normalizer::round_offset_hz(out.candidate));
    }
    Ok(out)
}

pub struct FrequencyRequest<'a> {
    pub input: crate::icf::settings::extended::NumberInput<'a>,
    pub point: u8,
    pub context: EditorContext<'a>,
    pub live_q: &'a crate::icf::context::Context,
    pub warning_bits: u8,
    pub focus: u8,
}
pub struct OffsetRequest<'a> {
    pub input: crate::icf::settings::extended::NumberInput<'a>,
    pub point: u8,
    pub context: EditorContext<'a>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrequencyOutcome {
    pub stored: bool,
    pub first: NumericOutcome,
    pub second: Option<NumericOutcome>,
    pub notify_flags: u32,
    pub warning_bits: u8,
    pub focus: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelEnum {
    Mode,
    Data,
    Filter,
}
impl ChannelEnum {
    pub fn key(self) -> u32 {
        match self {
            Self::Mode => 0x108,
            Self::Data => 0x109,
            Self::Filter => 0x10a,
        }
    }
    pub fn choices(self, h: &DecodedRecord) -> Vec<i64> {
        match self {
            Self::Mode => (0..8)
                .chain([9, 8])
                .map(record::decode_mode)
                .filter(|&value| {
                    let mut candidate = h.clone();
                    candidate.set_mode(value);
                    value == h.mode() || !record::channel_mode_bad(&candidate)
                })
                .map(i64::from)
                .collect(),
            Self::Data => vec![0, 1],
            Self::Filter => vec![0, 1, 2],
        }
    }
}
pub struct EnumRequest<'a> {
    pub field: ChannelEnum,
    pub input: crate::icf::settings::extended::ChoiceInput,
    pub context: EditorContext<'a>,
    pub warning_bits: Option<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumOutcome {
    pub choice: ChoiceOutcome,
    pub context_state: Option<u32>,
    pub warning_bits: Option<u8>,
}

/// 69521/69697/69813 field bodies. Mode's selected key is 7, not the generic
/// key 1 used by DATA/filter; the shared choice calculator still owns actions.
pub fn channel_enum_input(
    h: &mut DecodedRecord,
    field: ChannelEnum,
    input: crate::icf::settings::extended::ChoiceInput,
) -> Result<ChoiceOutcome, Error> {
    use crate::icf::settings::extended::{ChoiceInput, choice_candidate};
    if !matches!(h.kind(), 2 | 3) {
        return Err(invalid(
            "mode/DATA/filter input requires a normal/call holder",
        ));
    }
    let mode_blocked = channel_mode_blocked(h, field.key());
    if mode_blocked && field == ChannelEnum::Data {
        return Ok(ChoiceOutcome {
            stored: false,
            candidate: None,
            mode_blocked,
            popup_requested: false,
        });
    }
    let expected = if field == ChannelEnum::Mode { 7 } else { 1 };
    let input = match input {
        ChoiceInput::Selected(mut e) => {
            if e.key == expected {
                e.key = 1;
                ChoiceInput::Selected(e)
            } else {
                ChoiceInput::Display
            }
        }
        other => other,
    };
    let old = if mode_blocked {
        0
    } else {
        match field {
            ChannelEnum::Mode => h.mode(),
            ChannelEnum::Data => h.data(),
            ChannelEnum::Filter => h.filter(),
        }
    };
    let candidate = choice_candidate(i64::from(old), &field.choices(h), input);
    if let Some(value) = candidate.filter(|_| !mode_blocked) {
        match field {
            ChannelEnum::Mode => h.set_mode(value as u32),
            ChannelEnum::Data => h.set_data(value as u32),
            ChannelEnum::Filter => h.set_filter(value as u32),
        }
    }
    Ok(ChoiceOutcome {
        stored: candidate.is_some() && !mode_blocked,
        candidate,
        mode_blocked,
        popup_requested: input == ChoiceInput::Begin,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneField {
    Tone,
    Tx,
    Rx,
    Dtcs,
}
impl ToneField {
    pub fn key(self) -> u32 {
        match self {
            Self::Tone => 0x10d,
            Self::Tx => 0x10e,
            Self::Rx => 0x10f,
            Self::Dtcs => 0x110,
        }
    }
    pub(crate) fn value(self, h: &DecodedRecord) -> u32 {
        match self {
            Self::Tone => h.channel_field(ChannelField::Tone),
            Self::Tx => h.tx_tone(),
            Self::Rx => h.rx_tone(),
            Self::Dtcs => h.channel_field(ChannelField::Dtcs),
        }
    }
    fn store(self, h: &mut DecodedRecord, value: u32) {
        match self {
            Self::Tone => h.set_channel_field(ChannelField::Tone, value),
            Self::Tx => h.set_tx_tone(value),
            Self::Rx => h.set_rx_tone(value),
            Self::Dtcs => h.set_channel_field(ChannelField::Dtcs, value),
        }
    }
}

/// Fixed source schemas, NOT second storage lookup tables. DTCS is 20 rows,
/// TFreq is 10 rows; resource-first tie order must not be sorted by saved index.
pub fn tone_choices(
    field: ToneField,
    live_q: Option<&crate::icf::context::Context>,
) -> Result<Vec<i64>, Error> {
    Ok(match field {
        ToneField::Tone => (0..8)
            .map(|raw| i64::from(record::decode_tone(raw)))
            .collect(),
        ToneField::Dtcs => (0..20)
            .flat_map(|r| (0..6).map(move |c| r + 20 * c))
            .filter(|&n| n < 104)
            .collect(),
        ToneField::Tx | ToneField::Rx => {
            let q = live_q.ok_or(invalid("TFreq source99E44 requires the actual live Q"))?;
            // 99E44->56E70: Q12==0 selects MTON 51, nonzero selects normal 50.
            let count = if q.as_bytes()[0x12] == 0 { 51 } else { 50 };
            (0..10)
                .flat_map(|r| (0..6).map(move |c| r + 10 * c))
                .filter(|&n| n < count)
                .collect()
        }
    })
}
pub(crate) fn tone_numeric_label(field: ToneField, value: u32) -> Option<Vec<u8>> {
    match field {
        ToneField::Tone => None,
        ToneField::Dtcs => crate::csv::table::DTCS_CODES
            .get(value as usize)
            .map(|v| format!("{v:03}").into_bytes()),
        ToneField::Tx | ToneField::Rx => {
            let value = if value == 50 {
                1500
            } else {
                *crate::repeater::REPEATER_TONES.get(value as usize)?
            };
            Some(format!("{}.{}", value / 10, value % 10).into_bytes())
        }
    }
}

pub struct ToneInput<'a> {
    pub field: ToneField,
    pub input: crate::icf::settings::extended::NumberInput<'a>,
    pub point: u8,
    pub live_q: Option<&'a crate::icf::context::Context>,
    /// Begin+inline is 7100/current label, or a digit-start when first is Some.
    pub inline: bool,
    pub first: Option<u8>,
    pub upper: &'a mut dyn FnMut(&mut [u8]) -> Result<(), crate::csv::Error>,
}
pub struct ToneRequest<'a> {
    pub input: ToneInput<'a>,
    pub context: EditorContext<'a>,
}
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ToneEffect {
    pub stored: bool,
    pub candidate: Option<i32>,
    pub mode_blocked: bool,
    pub popup_requested: bool,
    pub state_out: u32,
    pub shared_input: Vec<u8>,
    pub inline_text: Option<Vec<u8>>,
}

/// Shared A5EDC field engine for normal/call: mode 3 ignores typed text, while
/// mode 0 parses the entire trimmed/upper C-prefix without numeric 11 byte filter.
/// The supplied NLS boundary must be actual output; prior text survives errors.
pub fn channel_tone_input(
    h: &mut DecodedRecord,
    input: &mut ToneInput<'_>,
    effect: &mut ToneEffect,
) -> Result<(), Error> {
    use crate::icf::settings::extended::{ChoiceInput, NumberInput, choice_candidate};
    if !matches!(h.kind(), 2 | 3)
        || input.inline && (input.input != NumberInput::Begin || input.field == ToneField::Tone)
        || input
            .first
            .is_some_and(|b| !input.inline || !b.is_ascii_digit())
    {
        return Err(invalid(
            "tone input requires normal/call; inline is a mode0 Begin digit/current-label request",
        ));
    }
    *effect = ToneEffect::default();
    effect.mode_blocked = channel_mode_blocked(h, input.field.key());
    if effect.mode_blocked {
        return Ok(());
    }
    let choices = tone_choices(input.field, input.live_q)?;
    let old = input.field.value(h);
    if input.inline {
        effect.state_out = 2;
        effect.inline_text = Some(if let Some(first) = input.first {
            vec![first]
        } else {
            if !choices.contains(&i64::from(old)) {
                return Err(invalid(
                    "inline tone seed requires the actual current source label",
                ));
            }
            let mut text = tone_numeric_label(input.field, old)
                .ok_or(invalid("numeric tone label unavailable"))?;
            text.truncate(9);
            text
        });
        return Ok(());
    }
    effect.candidate = if let NumberInput::Text(text) = input.input {
        if input.field == ToneField::Tone {
            None
        } else {
            effect.shared_input = crate::csv::trim(text).to_vec();
            (input.upper)(&mut effect.shared_input)
                .map_err(|_| invalid("actual tone uppercase output unavailable"))?;
            let value = crate::repeater::editor::tone_crt(&effect.shared_input, input.point)
                .map_err(|_| invalid("native tone CRT conversion failed"))?;
            let mut best = choices[0];
            let mut difference = f64::INFINITY;
            for &candidate in &choices {
                let label = tone_numeric_label(input.field, candidate as u32)
                    .expect("generated numeric tone source label");
                let distance = (value
                    - crate::repeater::editor::tone_crt(&label, input.point)
                        .map_err(|_| invalid("numeric tone source label conversion failed"))?)
                .abs();
                if distance < difference {
                    best = candidate;
                    difference = distance;
                }
            }
            Some(best as i32)
        }
    } else {
        let choice = match input.input {
            NumberInput::Selected(e) => ChoiceInput::Selected(e),
            NumberInput::Next => ChoiceInput::Next,
            NumberInput::Previous => ChoiceInput::Previous,
            NumberInput::Begin => ChoiceInput::Begin,
            NumberInput::Cancel => ChoiceInput::Cancel,
            _ => ChoiceInput::Display,
        };
        effect.popup_requested = choice == ChoiceInput::Begin;
        choice_candidate(i64::from(old), &choices, choice)
    };
    if let Some(candidate) = effect.candidate {
        input.field.store(h, candidate as u32);
        effect.stored = true;
    }
    Ok(())
}

pub struct CodeRequest<'a> {
    pub input: crate::icf::settings::extended::NumberInput<'a>,
    /// Only a digit-start keyboard request; None is Enter/current selection.
    pub first: Option<u8>,
    pub node_flags: &'a mut u8,
    pub context: EditorContext<'a>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeOutcome {
    pub stored: bool,
    pub candidate: Option<u32>,
    pub mode_blocked: bool,
    pub state_out: u32,
    pub display: Vec<u8>,
    pub selection: (usize, usize),
}

/// 9A468's signed 32 minimum-width 2 display. No clipping, unsigned rewrite or
/// low-byte projection belongs to this shared readonly formatter.
pub fn digital_code_label(value: u32) -> Vec<u8> {
    format!("{:02}", value as i32).into_bytes()
}

/// 9A468 is NOT A522C or A5EDC. Validate its one/two ASCII digits before the
/// trivial decimal accumulation; selected events never supply its candidate.
pub fn channel_code_input(
    h: &mut DecodedRecord,
    input: crate::icf::settings::extended::NumberInput<'_>,
    first: Option<u8>,
    node_flags: &mut u8,
) -> Result<CodeOutcome, Error> {
    use crate::icf::settings::extended::NumberInput;
    if !matches!(h.kind(), 2 | 3)
        || first.is_some_and(|b| input != NumberInput::Begin || !b.is_ascii_digit())
    {
        return Err(invalid(
            "channel code requires normal/call; first is a Begin ASCII digit",
        ));
    }
    let old = h.channel_field(ChannelField::DigitalCode);
    let mut out = CodeOutcome {
        stored: false,
        candidate: None,
        mode_blocked: channel_mode_blocked(h, 0x113),
        state_out: 0,
        display: digital_code_label(old),
        selection: (0, 0),
    };
    out.selection.1 = out.display.len();
    if out.mode_blocked {
        return Ok(out);
    }
    *node_flags = (*node_flags & !3) | 2;
    out.candidate = match input {
        NumberInput::Text(text) => {
            let prefix = text.split(|&b| b == 0).next().expect("code C prefix");
            if matches!(prefix.len(), 1 | 2) && prefix.iter().all(u8::is_ascii_digit) {
                Some(
                    prefix
                        .iter()
                        .fold(0u32, |n, b| n * 10 + u32::from(b - b'0')),
                )
            } else {
                None
            }
        }
        NumberInput::Next | NumberInput::Previous => {
            let n = if input == NumberInput::Next {
                old.wrapping_add(1)
            } else {
                old.wrapping_sub(1)
            } as i32;
            Some(if n < 0 {
                99
            } else if n > 99 {
                0
            } else {
                n as u32
            })
        }
        NumberInput::Begin => {
            out.state_out = 2;
            if let Some(first) = first {
                out.display = vec![first];
                out.selection = (1, 0);
            }
            None
        }
        _ => None,
    };
    if let Some(candidate) = out.candidate {
        if matches!(input, NumberInput::Text(_)) {
            out.display = digital_code_label(candidate);
            out.selection = (0, out.display.len());
        }
        h.set_channel_field(ChannelField::DigitalCode, candidate);
        out.stored = true;
    }
    Ok(out)
}

/// Captured document pending state. The string is live, not the queued blob.
#[derive(Clone)]
pub struct PendingName {
    pub active: bool,
    pub index: u32,
    pub bytes: Vec<u8>,
}
impl PendingName {
    pub fn reset(&mut self) {
        self.active = false;
        self.index = u32::MAX;
        self.bytes.clear();
    }
}

/// Two independent native caches. Their binding tokens and unknown body bytes
/// come from the caller; an absent cache is not an invented cold/default state.
#[derive(Clone)]
pub struct EditorState {
    pub mode_cache: DecodedRecord,
    pub filter_cache: DecodedRecord,
    pub filter_enabled: u8,
    pub pending: PendingName,
}
#[derive(Clone, Copy)]
pub struct EditorContext<'a> {
    pub ranges: &'a crate::icf::frequency::Ranges,
    pub generation: ReferenceImage<'a>,
}
pub struct CacheWrite<'a> {
    pub key: u32,
    pub previous: Option<&'a DecodedRecord>,
}
fn cache_mode(mode: u32) -> u32 {
    match mode {
        1 => 0,
        3 => 2,
        5 => 4,
        7 => 6,
        9..=12 => 8,
        14 => 13,
        _ => mode,
    }
}
fn effective_data(h: &DecodedRecord) -> u8 {
    if matches!(h.mode(), 0 | 1 | 8 | 13) {
        h.data() as u8
    } else {
        0
    }
}

/// 8928 shared filter request. The caller supplies the actual live G slice at
/// this cutpoint; ineligible classes do not read it or invent a default byte.
pub fn default_filter_into(
    generation: &[u8],
    ranges: &crate::icf::frequency::Ranges,
    h: &mut DecodedRecord,
) -> Result<(), Error> {
    let class = record::mode_filter_class(h.mode(), u32::from(effective_data(h)));
    let band = band_class(ranges, h.channel_field(ChannelField::FrequencyHz));
    if class != 255 && band < 3 {
        h.set_filter(u32::from(
            raw::<1>(generation, 0x37823 + 9 * band + usize::from(class))?[0],
        ));
    }
    Ok(())
}

// EE90 is one projection of F014, shared by 8928 and B4BF8.
fn band_class(ranges: &crate::icf::frequency::Ranges, frequency: u32) -> usize {
    match ranges.f014(frequency) {
        0..=12 => 0,
        13..=15 => 1,
        16 => 2,
        _ => 3,
    }
}

fn same_filter_entry(cache: &DecodedRecord, h: &DecodedRecord) -> bool {
    cache.kind() == h.kind()
        && (cache.index() == h.index()
            || matches!(h.kind(), 2 | 3)
                && cache.index()
                    == h.index()
                        .wrapping_add(if h.side() == 0 { 1 } else { u32::MAX }))
}
impl EditorState {
    /// Document 4C5CB..4C648: two 5E1C/5F64 holders, valid cleared, enable 0;
    /// 53240 resets the separate pending object. No Q, G or GUI state is guessed.
    pub fn initialized() -> Self {
        let mut holder = record::reset_holder(0, 0, 0);
        holder.set_valid(false);
        Self {
            mode_cache: holder.clone(),
            filter_cache: holder,
            filter_enabled: 0,
            pending: PendingName {
                active: false,
                index: u32::MAX,
                bytes: Vec::new(),
            },
        }
    }
    fn mode_transition(
        &mut self,
        h: &mut DecodedRecord,
        request: &CacheWrite<'_>,
    ) -> Result<(), Error> {
        if request.key != 0x108 {
            self.mode_cache.set_valid(false);
        } else if self.mode_cache.valid()
            && self.mode_cache.kind() == h.kind()
            && self.mode_cache.index() == h.index()
        {
            if self.mode_cache.mode() == u32::from(h.mode() as u8) {
                for field in [
                    ChannelField::Tone,
                    ChannelField::Dtcs,
                    ChannelField::Polarity,
                    ChannelField::DigitalSquelch,
                    ChannelField::DigitalCode,
                ] {
                    h.set_channel_field(field, self.mode_cache.channel_field(field));
                }
                h.set_tx_tone(self.mode_cache.tx_tone());
                h.set_rx_tone(self.mode_cache.rx_tone());
                for which in [
                    crate::icf::MemoryCallsign::Your,
                    crate::icf::MemoryCallsign::Repeater1,
                    crate::icf::MemoryCallsign::Repeater2,
                ] {
                    h.set_callsign(which, self.mode_cache.callsign(which));
                }
            }
        } else {
            let previous = request.previous.ok_or(invalid(
                "mode transition requires its actual previous holder",
            ))?;
            self.mode_cache.copy_body_from(h);
            self.mode_cache.set_mode(u32::from(previous.mode() as u8));
            self.mode_cache.set_valid(true);
            self.mode_cache
                .set_header(h.kind(), h.index(), self.mode_cache.binding());
        }
        Ok(())
    }
}

fn save_reload_into(
    w: &mut [u8],
    kind: ChannelKind,
    h: &mut DecodedRecord,
    before_store: &mut impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    let index = h.index();
    record_image::store_channel_into(w, kind, index, h, |event| before_store(index, event))?;
    record_image::load_channel_into(w, kind, index, h)?;
    Ok(())
}

/// B4BF8 reads the actual current type 1A, not the edited call row or G. The
/// Home request precedes that read, so a missing DR source keeps its prefix.
pub fn call_after_into(
    w: &[u8],
    row: i32,
    ranges: &crate::icf::frequency::Ranges,
    effects: &mut Effects,
) -> Result<(), Error> {
    if raw::<1>(w, 0x3c84b)?[0] == 100 && i32::from(u16::from_le_bytes(raw(w, 0x3c84c)?)) == row {
        effects.home_invalidated = 1;
    }
    let mut dr = record::reset_holder(0x1a, 0, 0);
    record_image::load_into(w, 0x1a, 0, &mut dr)?;
    let class = band_class(ranges, dr.channel_field(ChannelField::FrequencyHz));
    if (class == 1 && row < 2 || class == 2 && row >= 2)
        && i32::from(raw::<1>(w, 0x3a780 + class - 1)?[0]) == row % 2
    {
        effects.dr_invalidated = 1;
    }
    Ok(())
}

/// One 53830/53A6C body for normal and call, including actual ordered saves and
/// re-reads. No second cache codec or synthesized normal image backs call edits.
pub fn channel_save_cached_into(
    w: &mut [u8],
    h: &mut DecodedRecord,
    state: &mut EditorState,
    request: CacheWrite<'_>,
    context: EditorContext<'_>,
    effects: &mut Effects,
    mut before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    channel_save_mode_into(w, h, state, &request, context, effects, &mut before_store)?;
    if !same_filter_entry(&state.filter_cache, h) {
        state.filter_enabled = 0;
    }
    if state.filter_enabled == 0 || !matches!(request.key, 0x108 | 0x109) {
        state.filter_cache.set_valid(false);
    } else if state.filter_cache.valid()
        && state.filter_cache.kind() == h.kind()
        && state.filter_cache.index() == h.index()
    {
        let class = cache_mode(h.mode()) as u8;
        if u32::from(class) == state.filter_cache.mode()
            && (class != 0 || u32::from(effective_data(h)) == state.filter_cache.data())
        {
            h.set_filter(state.filter_cache.filter());
        } else {
            default_filter_into(context.generation.get(w), context.ranges, h)?;
        }
    } else {
        let (side, mode, data) = request.previous.map_or((255, 255, 255), |old| {
            (old.side(), old.mode() as u8, old.data() as u8)
        });
        state.filter_cache.copy_body_from(h);
        state.filter_cache.as_mut_bytes()[0x24] = side;
        let old_class = cache_mode(u32::from(mode)) as u8;
        state.filter_cache.set_mode(u32::from(old_class));
        state.filter_cache.set_data(u32::from(data));
        state.filter_cache.set_valid(true);
        state
            .filter_cache
            .set_header(h.kind(), h.index(), state.filter_cache.binding());
        let class = cache_mode(h.mode()) as u8;
        if old_class != class || class == 0 && effective_data(h) != data {
            default_filter_into(context.generation.get(w), context.ranges, h)?;
        }
    }
    let kind = if h.kind() == 2 {
        ChannelKind::Normal
    } else {
        ChannelKind::Call
    };
    save_reload_into(w, kind, h, &mut before_store)
}

/// The first 53830 saver is also D1's complete save path; it does not execute
/// the second 53A6C filter-cache transition used by ordinary field handlers.
fn channel_save_mode_into(
    w: &mut [u8],
    h: &mut DecodedRecord,
    state: &mut EditorState,
    request: &CacheWrite<'_>,
    context: EditorContext<'_>,
    effects: &mut Effects,
    mut before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    let kind = match h.kind() {
        2 => ChannelKind::Normal,
        3 => ChannelKind::Call,
        _ => return Err(invalid("channel cache saver requires normal or call")),
    };
    state.mode_transition(h, request)?;
    save_reload_into(w, kind, h, &mut before_store)?;
    if kind == ChannelKind::Call {
        call_after_into(w, (h.index() as i32) / 2, context.ranges, effects)?;
    } else if u32::from(raw::<1>(w, 0x3c84b)?[0]) == h.group()
        && u16::from_le_bytes(raw(w, 0x3c84c)?) == (h.index() / 2 % 100) as u16
    {
        effects.home_invalidated = 1;
    }
    Ok(())
}

fn checked_call_holder(h: &DecodedRecord) -> Result<(), Error> {
    if h.kind() != 3 || h.index() >= 8 || !h.valid() {
        return Err(invalid(
            "call input requires an existing typed call side0..7",
        ));
    }
    Ok(())
}

pub fn call_choice_input_into(
    w: &mut [u8],
    h: &mut DecodedRecord,
    state: &mut EditorState,
    request: ChoiceRequest<'_>,
    effects: &mut Effects,
    before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<ChoiceOutcome, Error> {
    checked_call_holder(h)?;
    if request.selection.is_some() {
        return Err(invalid("call choice has no writable SEL/frame"));
    }
    let out = channel_choice_input(h, request.field, request.input)?;
    if out.stored {
        channel_save_cached_into(
            w,
            h,
            state,
            CacheWrite {
                key: request.field.key(),
                previous: None,
            },
            request.context,
            effects,
            before_store,
        )?;
    }
    Ok(out)
}

pub fn call_enum_input_into(
    w: &mut [u8],
    h: &mut DecodedRecord,
    state: &mut EditorState,
    request: EnumRequest<'_>,
    effects: &mut Effects,
    before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<EnumOutcome, Error> {
    checked_call_holder(h)?;
    if (request.field == ChannelEnum::Mode) != request.warning_bits.is_some() {
        return Err(invalid("only mode requires actual warning bits"));
    }
    let previous = h.clone();
    let choice = channel_enum_input(h, request.field, request.input)?;
    if choice.stored {
        channel_save_cached_into(
            w,
            h,
            state,
            CacheWrite {
                key: request.field.key(),
                previous: if request.field == ChannelEnum::Filter {
                    None
                } else {
                    Some(&previous)
                },
            },
            request.context,
            effects,
            before_store,
        )?;
        if request.field == ChannelEnum::Mode {
            effects.queue(0x3c, 1, 0);
        }
    }
    Ok(EnumOutcome {
        choice,
        context_state: if request.field == ChannelEnum::Filter {
            None
        } else {
            Some(2)
        },
        warning_bits: request
            .warning_bits
            .map(|flags| flags | if record::channel_mode_bad(h) { 4 } else { 0 }),
    })
}

pub fn call_offset_input_into(
    w: &mut [u8],
    h: &mut DecodedRecord,
    state: &mut EditorState,
    request: OffsetRequest<'_>,
    effects: &mut Effects,
    before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<NumericOutcome, Error> {
    checked_call_holder(h)?;
    let out = channel_offset_input(h, request.input, request.point)?;
    if out.accepted {
        channel_save_cached_into(
            w,
            h,
            state,
            CacheWrite {
                key: 0x10c,
                previous: None,
            },
            request.context,
            effects,
            before_store,
        )?;
    }
    Ok(out)
}

pub fn call_tone_input_into(
    w: &mut [u8],
    h: &mut DecodedRecord,
    state: &mut EditorState,
    mut request: ToneRequest<'_>,
    tone_effect: &mut ToneEffect,
    effects: &mut Effects,
    before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    checked_call_holder(h)?;
    channel_tone_input(h, &mut request.input, tone_effect)?;
    if tone_effect.stored {
        channel_save_cached_into(
            w,
            h,
            state,
            CacheWrite {
                key: request.input.field.key(),
                previous: None,
            },
            request.context,
            effects,
            before_store,
        )?;
    }
    Ok(())
}

pub fn call_code_input_into(
    w: &mut [u8],
    h: &mut DecodedRecord,
    state: &mut EditorState,
    request: CodeRequest<'_>,
    effects: &mut Effects,
    before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<CodeOutcome, Error> {
    checked_call_holder(h)?;
    let out = channel_code_input(h, request.input, request.first, request.node_flags)?;
    if out.stored {
        channel_save_cached_into(
            w,
            h,
            state,
            CacheWrite {
                key: 0x113,
                previous: None,
            },
            request.context,
            effects,
            before_store,
        )?;
    }
    Ok(out)
}

pub fn call_name_input_into(
    w: &mut [u8],
    h: &mut DecodedRecord,
    state: &mut EditorState,
    request: NameRequest<'_>,
    text_effect: &mut NameEffect,
    effects: &mut Effects,
    before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    checked_call_holder(h)?;
    let paired = h.split() == 1 || request.frame.layout != 0;
    if let Some(mut candidate) = name_candidate_input(
        h,
        &state.pending,
        request.input,
        request.frame.layout,
        text_effect,
    )? {
        candidate.resize(16, b' ');
        h.set_name(candidate.as_slice().try_into().expect("native name width"));
        channel_save_cached_into(
            w,
            h,
            state,
            CacheWrite {
                key: 0x105,
                previous: None,
            },
            request.context,
            effects,
            before_store,
        )?;
        text_effect.stored = true;
    }
    request.frame.node_flags = (request.frame.node_flags & !1) | u8::from(paired);
    Ok(())
}

/// The 9C2C8 direct-text branch of UR/RPT, not its DR modal/list branches. Each
/// native upper result commits one holder byte before the next call can fail.
/// The return value is an actual completed save, not 9C5C4's outer AL result.
pub fn channel_callsign_text_input_into(
    w: &mut [u8],
    h: &mut DecodedRecord,
    state: &mut EditorState,
    request: CallsignTextRequest<'_>,
    effects: &mut Effects,
    before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<bool, Error> {
    use crate::icf::settings::extended::TextInput;
    if !matches!(h.kind(), 2 | 3) || h.index() >= if h.kind() == 2 { 20_000 } else { 8 } {
        return Err(invalid(
            "UR/RPT direct text requires a normal/call typed holder",
        ));
    }
    if h.kind() == 3 {
        checked_call_holder(h)?;
    }
    let source = match request.input {
        TextInput::Text(source) => source,
        TextInput::Cancel | TextInput::Display => return Ok(false),
        TextInput::Begin(_) => {
            return Err(invalid(
                "UR/RPT Begin requires the actual DR modal consumer",
            ));
        }
    };
    let mut raw = *h.callsign(request.field);
    for slot in 0..8 {
        raw[slot] = if let Some(&byte) = source.get(slot) {
            let value = (request.upper)(i32::from(byte as i8))?;
            if matches!(value, 0x30..=0x39 | 0x41..=0x5a | 0x2f) {
                value as u8
            } else if matches!(value, 0x61..=0x7a) {
                (value as u8).to_ascii_uppercase()
            } else {
                b' '
            }
        } else {
            b' '
        };
        h.set_callsign(request.field, &raw);
    }
    crate::csv::nls::fold_callsign(&mut raw);
    if request.right_word {
        crate::icf::settings::extended::align_callsign(&mut raw);
    }
    h.set_callsign(request.field, &raw);
    if !h.valid() && registration_count(w)? >= 500 {
        return Ok(false);
    }
    save_callsign_into(
        w,
        h,
        state,
        request.field,
        request.context,
        effects,
        before_store,
    )?;
    effects.queue(0x3c, 1, 0);
    Ok(true)
}

fn save_callsign_into(
    w: &mut [u8],
    h: &mut DecodedRecord,
    state: &mut EditorState,
    field: crate::icf::MemoryCallsign,
    context: EditorContext<'_>,
    effects: &mut Effects,
    before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    channel_save_cached_into(
        w,
        h,
        state,
        CacheWrite {
            key: callsign_key(field),
            previous: None,
        },
        context,
        effects,
        before_store,
    )
}

fn callsign_key(field: crate::icf::MemoryCallsign) -> u32 {
    match field {
        crate::icf::MemoryCallsign::Your => 0x114,
        crate::icf::MemoryCallsign::Repeater1 => 0x115,
        crate::icf::MemoryCallsign::Repeater2 => 0x116,
    }
}

pub enum CallsignModal {
    List(crate::icf::callsign_dialog::ListSelection),
    Dr { modal_result: u32 },
}
pub struct CallsignModalRequest<'a> {
    pub field: crate::icf::MemoryCallsign,
    pub modal: CallsignModal,
    pub sources: &'a crate::icf::callsign_dialog::Sources<'a>,
    pub reads: &'a crate::icf::callsign_dialog::ReadInputs<'a>,
    pub context: EditorContext<'a>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallsignModalOutcome {
    pub stored: bool,
    pub accepted: bool,
    pub al: bool,
    pub checkbox: bool,
    pub initial_selection: Option<crate::icf::callsign_dialog::ListSelection>,
}

/// D1 keeps its false-return notifications. D2 saves even after modal Cancel;
/// only accepting a new normal clone allocates and mirrors a pair.
pub fn channel_callsign_modal_input_into(
    w: &mut [u8],
    h: &mut DecodedRecord,
    state: &mut EditorState,
    request: CallsignModalRequest<'_>,
    effects: &mut Effects,
    actions: impl FnOnce(&mut crate::icf::callsign_dialog::Dialog) -> Result<(), Error>,
    mut before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<CallsignModalOutcome, Error> {
    use crate::icf::callsign_dialog::{self as dialog, Dialog};
    if !matches!(h.kind(), 2 | 3) || h.index() >= if h.kind() == 2 { 20_000 } else { 8 } {
        return Err(invalid(
            "callsign modal requires a normal/call typed holder",
        ));
    }
    if h.kind() == 3 {
        checked_call_holder(h)?;
    }
    let was_empty = !h.valid();
    if matches!(&request.modal, CallsignModal::Dr { .. })
        && was_empty
        && registration_count(w)? >= 500
    {
        return Ok(CallsignModalOutcome {
            stored: false,
            accepted: false,
            al: false,
            checkbox: false,
            initial_selection: None,
        });
    }
    state.mode_cache.set_valid(false);
    if !same_filter_entry(&state.filter_cache, h) {
        state.filter_enabled = 0;
    }
    match request.modal {
        CallsignModal::List(selection) => {
            let mode = match request.field {
                crate::icf::MemoryCallsign::Your => 0,
                crate::icf::MemoryCallsign::Repeater1 => 1,
                crate::icf::MemoryCallsign::Repeater2 => 2,
            };
            let initial = dialog::initial_selection(
                request.sources,
                h,
                mode,
                h.callsign(request.field),
                h.mode(),
                request.reads,
            )?;
            let out =
                dialog::list_into(request.sources, h, request.field, selection, request.reads)?;
            if out.save {
                channel_save_mode_into(
                    w,
                    h,
                    state,
                    &CacheWrite {
                        key: callsign_key(request.field),
                        previous: None,
                    },
                    request.context,
                    effects,
                    &mut before_store,
                )?;
            }
            effects.queue(0x11, 0, 0);
            effects.notify(0x39, 0, 0);
            Ok(CallsignModalOutcome {
                stored: out.save,
                accepted: out.save,
                al: false,
                checkbox: out.checkbox,
                initial_selection: Some(initial),
            })
        }
        CallsignModal::Dr { modal_result } => {
            effects.queue(0x18, 0, 1);
            let mut dialog = Dialog::open(request.sources, h, request.reads)?;
            actions(&mut dialog)?;
            let out = dialog.finish(h, modal_result);
            effects.queue(0x18, 0, 0);
            state.filter_enabled = u8::from(out.accepted);
            if out.accepted {
                state.filter_cache.set_valid(false);
                state
                    .filter_cache
                    .set_header(h.kind(), h.index(), state.filter_cache.binding());
            }
            if was_empty && out.accepted && !allocate_registration_into(w, h, &mut state.pending)? {
                return Ok(CallsignModalOutcome {
                    stored: false,
                    accepted: out.accepted,
                    al: false,
                    checkbox: out.checkbox,
                    initial_selection: None,
                });
            }
            save_callsign_into(
                w,
                h,
                state,
                request.field,
                request.context,
                effects,
                &mut before_store,
            )?;
            let allocated = was_empty && out.accepted;
            if allocated {
                let index = h.index() ^ 1;
                let mut other = record::reset_holder(2, index, h.binding());
                other.copy_body_from(h);
                record_image::store_channel_into(
                    w,
                    ChannelKind::Normal,
                    index,
                    &mut other,
                    |write| before_store(index, write),
                )?;
            }
            effects.queue(0x3c, 1, 0);
            Ok(CallsignModalOutcome {
                stored: true,
                accepted: out.accepted,
                al: allocated,
                checkbox: out.checkbox,
                initial_selection: None,
            })
        }
    }
}

pub(crate) fn registration_count(w: &[u8]) -> Result<u64, Error> {
    (0..100).try_fold(0u64, |count, group| {
        Ok(count + u64::from(u32::from_le_bytes(raw(w, super::COUNTS + 4 * group)?)))
    })
}
fn allocate_registration_into(
    w: &mut [u8],
    h: &DecodedRecord,
    pending: &mut PendingName,
) -> Result<bool, Error> {
    if registration_count(w)? >= 500 {
        pending.reset();
        return Ok(false);
    }
    if h.index() / 200 >= u32::from_le_bytes(raw(w, super::GROUP_COUNT)?) {
        return Err(invalid("logical group or row range outside normal map"));
    }
    super::allocate_into(
        w,
        h.index() as usize / 200,
        h.index() as usize / 2 % 100..h.index() as usize / 2 % 100 + 1,
    )?;
    Ok(true)
}

/// Normal call rows already exist; there is no normal-slot allocation or first
/// opposite-side mirror. H38 from the actual call adapter selects range 12/13.
pub fn call_frequency_input_into(
    w: &mut [u8],
    h: &mut DecodedRecord,
    state: &mut EditorState,
    request: FrequencyRequest<'_>,
    effects: &mut Effects,
    before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<FrequencyOutcome, Error> {
    use crate::icf::settings::extended::NumberInput;
    checked_call_holder(h)?;
    if !request.context.ranges.is_complete() {
        return Err(invalid("call frequency requires completed B"));
    }
    let first = frequency_first(h, &request)?;
    let mut out = FrequencyOutcome {
        stored: false,
        first,
        second: None,
        notify_flags: 0,
        warning_bits: request.warning_bits,
        focus: request.focus,
    };
    if first.accepted || matches!(request.input, NumberInput::Text(_)) && state.pending.active {
        let (second, _) = frequency_second(h, first.candidate, &request)?;
        out.second = Some(second);
        apply_frequency_candidate(h, second.candidate, &mut state.pending)?;
        state.filter_enabled = 1;
        state.filter_cache.set_valid(false);
        state
            .filter_cache
            .set_header(3, h.index(), state.filter_cache.binding());
        channel_save_cached_into(
            w,
            h,
            state,
            CacheWrite {
                key: 0x107,
                previous: None,
            },
            request.context,
            effects,
            before_store,
        )?;
        effects.queue(0x3c, 1, 0);
        out.stored = true;
        out.notify_flags = 0x18;
    }
    frequency_tail(h, state, &request, &mut out)?;
    Ok(out)
}

/// Native 69977 uses the same split editor and cache saver for normal and call.
/// Callers supply live grid scalars; requests are not simulated OS dispatch.
pub fn channel_split_input_into(
    w: &mut [u8],
    h: &mut DecodedRecord,
    state: &mut EditorState,
    request: SplitRequest<'_>,
    effects: &mut Effects,
    before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
    mut before_notice: impl FnMut(&Notification, &mut SplitFrame) -> Result<(), Error>,
) -> Result<SplitOutcome, Error> {
    use crate::icf::settings::extended::{ChoiceInput, choice_candidate};
    if !h.valid()
        || !matches!(h.kind(), 2 | 3)
        || h.index() >= if h.kind() == 2 { 20_000 } else { 8 }
    {
        return Err(invalid(
            "split case requires an existing normal/call typed holder",
        ));
    }
    let frame = request.frame;
    let step = frame.pre_step;
    let previous = u32::from(h.split());
    let candidate = choice_candidate(i64::from(previous), &[0, 1], request.input);
    if let Some(candidate) = candidate {
        h.set_split(candidate as u8);
        channel_save_cached_into(
            w,
            h,
            state,
            CacheWrite {
                key: 0x106,
                previous: None,
            },
            request.context,
            effects,
            before_store,
        )?;
        if frame.layout == 0 && previous != candidate as u32 {
            let notice = Notification {
                id: if candidate == 0 { 0x46 } else { 0x45 },
                synchronous: true,
                arg1: 0,
                arg2: 0,
            };
            before_notice(&notice, frame)?;
            effects.notifications.push(notice);
            frame.active = if candidate == 0 {
                frame.selection_first
            } else {
                frame.node_index
            };
            frame.display_mode = 2;
        }
        frame.notify_flags |= 0x20;
        let notice = Notification {
            id: 0x3c,
            synchronous: false,
            arg1: 1,
            arg2: 0,
        };
        before_notice(&notice, frame)?;
        effects.notifications.push(notice);
    }
    frame.node_flags = (frame.node_flags & !1) | u8::from(step > 1);
    Ok(SplitOutcome {
        stored: candidate.is_some(),
        candidate,
        popup_requested: request.input == ChoiceInput::Begin,
    })
}

impl NormalMemory {
    /// Normal entry preserves its original domain and delegates to the single
    /// borrowed cache body shared with the call family.
    pub fn native_save_cached(
        &mut self,
        h: &mut DecodedRecord,
        state: &mut EditorState,
        request: CacheWrite<'_>,
        context: EditorContext<'_>,
        effects: &mut Effects,
        before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if h.kind() != 2 {
            return Err(invalid("normal cache saver requires a normal holder"));
        }
        channel_save_cached_into(
            &mut self.working,
            h,
            state,
            request,
            context,
            effects,
            before_store,
        )
    }

    /// Accepted name producer. Invalid nonempty names are pending only; the
    /// caller supplies the actual 24-byte notification context separately.
    pub fn native_pending_name(
        &self,
        h: &DecodedRecord,
        name: &[u8],
        pending: &mut PendingName,
    ) -> Result<bool, Error> {
        if h.kind() != 2 || h.valid() || name.len() > 16 {
            return Err(invalid(
                "pending name requires an empty normal holder and bounded accepted bytes",
            ));
        }
        if name.is_empty() {
            return Ok(false);
        }
        if self.used_slots() >= 500 {
            pending.reset();
            return Ok(false);
        }
        pending.active = true;
        pending.index = h.index();
        pending.bytes = name.to_vec();
        Ok(true)
    }

    /// D0978/B596C/B5A78 accepted registration cutpoint. source_selector is the
    /// completed B.selected projection, never a selector inferred from live Q.
    /// The caller performs the actual writer/cache sequence after this returns.
    pub fn native_prepare_registration(
        &mut self,
        h: &mut DecodedRecord,
        candidate: u32,
        initial: (u32, usize),
        context: EditorContext<'_>,
        pending: &mut PendingName,
    ) -> Result<bool, Error> {
        if h.kind() != 2 || h.index() >= 20_000 {
            return Err(invalid("registration requires a normal typed index"));
        }
        if !h.valid() && !self.allocate_registration(h, pending)? {
            return Ok(false);
        }
        self.finish_registration(h, candidate, initial, context, pending)?;
        Ok(true)
    }
    fn allocate_registration(
        &mut self,
        h: &DecodedRecord,
        pending: &mut PendingName,
    ) -> Result<bool, Error> {
        allocate_registration_into(&mut self.working, h, pending)
    }
    fn finish_registration(
        &mut self,
        h: &mut DecodedRecord,
        candidate: u32,
        initial: (u32, usize),
        context: EditorContext<'_>,
        pending: &mut PendingName,
    ) -> Result<(), Error> {
        if !h.valid() {
            if initial.1 != context.ranges.default_source_selector(None)? {
                return Err(invalid(
                    "registration source differs from completed B.selected",
                ));
            }
            let mut initial_holder = record::reset_holder(0x1d, initial.0, 0);
            record::initial_normal_channel_into(
                context.generation.get(self.working()),
                initial.0,
                initial.1,
                &mut initial_holder,
            )?;
            if matches!(initial_holder.mode(), 0 | 1) {
                initial_holder.set_mode(u32::from(candidate >= 10_000_000));
            }
            h.copy_body_from(&initial_holder);
        }
        apply_frequency_candidate(h, candidate, pending)
    }
}

impl NormalMemory {
    pub fn native_enum_input(
        &mut self,
        h: &mut DecodedRecord,
        state: &mut EditorState,
        request: EnumRequest<'_>,
        effects: &mut Effects,
        before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
    ) -> Result<EnumOutcome, Error> {
        if h.kind() != 2
            || h.index() >= 20_000
            || !h.valid()
            || (request.field == ChannelEnum::Mode) != request.warning_bits.is_some()
        {
            return Err(invalid(
                "normal enum requires an existing normal holder; only mode requires its actual warning bits",
            ));
        }
        let previous = h.clone();
        let choice = channel_enum_input(h, request.field, request.input)?;
        if choice.stored {
            self.native_save_cached(
                h,
                state,
                CacheWrite {
                    key: request.field.key(),
                    previous: if request.field == ChannelEnum::Filter {
                        None
                    } else {
                        Some(&previous)
                    },
                },
                request.context,
                effects,
                before_store,
            )?;
            if request.field == ChannelEnum::Mode {
                effects.queue(0x3c, 1, 0);
            }
        }
        let warning_bits = request
            .warning_bits
            .map(|flags| flags | if record::channel_mode_bad(h) { 4 } else { 0 });
        Ok(EnumOutcome {
            choice,
            context_state: if request.field == ChannelEnum::Filter {
                None
            } else {
                Some(2)
            },
            warning_bits,
        })
    }

    pub fn native_tone_input(
        &mut self,
        h: &mut DecodedRecord,
        state: &mut EditorState,
        mut request: ToneRequest<'_>,
        tone_effect: &mut ToneEffect,
        effects: &mut Effects,
        before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if h.kind() != 2 || h.index() >= 20_000 || !h.valid() {
            return Err(invalid(
                "normal tone requires an existing normal typed holder",
            ));
        }
        channel_tone_input(h, &mut request.input, tone_effect)?;
        if tone_effect.stored {
            self.native_save_cached(
                h,
                state,
                CacheWrite {
                    key: request.input.field.key(),
                    previous: None,
                },
                request.context,
                effects,
                before_store,
            )?;
        }
        Ok(())
    }

    pub fn native_offset_input(
        &mut self,
        h: &mut DecodedRecord,
        state: &mut EditorState,
        request: OffsetRequest<'_>,
        effects: &mut Effects,
        before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
    ) -> Result<NumericOutcome, Error> {
        if h.kind() != 2 || h.index() >= 20_000 || !h.valid() {
            return Err(invalid(
                "normal offset requires an existing normal typed holder",
            ));
        }
        let out = channel_offset_input(h, request.input, request.point)?;
        if out.accepted {
            self.native_save_cached(
                h,
                state,
                CacheWrite {
                    key: 0x10c,
                    previous: None,
                },
                request.context,
                effects,
                before_store,
            )?;
        }
        Ok(out)
    }

    /// 68D7C's SAME-text second parse follows the first allocation, not a
    /// re-clamp of the first candidate. Its bool is never the second save gate.
    pub fn native_frequency_input(
        &mut self,
        h: &mut DecodedRecord,
        state: &mut EditorState,
        request: FrequencyRequest<'_>,
        effects: &mut Effects,
        mut before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
    ) -> Result<FrequencyOutcome, Error> {
        use crate::icf::settings::extended::NumberInput;
        if h.kind() != 2 || h.index() >= 20_000 || !request.context.ranges.is_complete() {
            return Err(invalid(
                "normal frequency requires a normal typed holder and completed B",
            ));
        }
        let ranges = request.context.ranges;
        let first = frequency_first(h, &request)?;
        let mut out = FrequencyOutcome {
            stored: false,
            first,
            second: None,
            notify_flags: 0,
            warning_bits: request.warning_bits,
            focus: request.focus,
        };
        if first.accepted || matches!(request.input, NumberInput::Text(_)) && state.pending.active {
            let was_empty = !h.valid();
            if !was_empty || self.allocate_registration(h, &mut state.pending)? {
                let (second, class) = frequency_second(h, first.candidate, &request)?;
                out.second = Some(second);
                let source = ranges.default_source_selector(None)?;
                self.finish_registration(
                    h,
                    second.candidate,
                    (class as u32, source),
                    request.context,
                    &mut state.pending,
                )?;
                state.filter_enabled = 1;
                state.filter_cache.set_valid(false);
                state
                    .filter_cache
                    .set_header(2, h.index(), state.filter_cache.binding());
                self.native_save_cached(
                    h,
                    state,
                    CacheWrite {
                        key: 0x107,
                        previous: None,
                    },
                    request.context,
                    effects,
                    &mut before_store,
                )?;
                if was_empty {
                    let index = h.index() ^ 1;
                    let mut other = record::reset_holder(2, index, h.binding());
                    other.copy_body_from(h);
                    record_image::store_channel_into(
                        &mut self.working,
                        ChannelKind::Normal,
                        index,
                        &mut other,
                        |w| before_store(index, w),
                    )?;
                }
                effects.queue(0x3c, 1, 0);
                out.stored = true;
            }
            // Both flags are native normal-return writes, including capacity 0.
            out.notify_flags |= 0x18;
        }
        frequency_tail(h, state, &request, &mut out)?;
        Ok(out)
    }

    pub fn native_code_input(
        &mut self,
        h: &mut DecodedRecord,
        state: &mut EditorState,
        request: CodeRequest<'_>,
        effects: &mut Effects,
        before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
    ) -> Result<CodeOutcome, Error> {
        if h.kind() != 2 || h.index() >= 20_000 || !h.valid() {
            return Err(invalid(
                "normal code case requires an existing normal typed holder",
            ));
        }
        let out = channel_code_input(h, request.input, request.first, request.node_flags)?;
        if out.stored {
            self.native_save_cached(
                h,
                state,
                CacheWrite {
                    key: 0x113,
                    previous: None,
                },
                request.context,
                effects,
                before_store,
            )?;
        }
        Ok(out)
    }

    /// 689FD uses the shared key 105 byte grammar before either native saver or
    /// live pending-name producer. Effects retain sanitation before later error.
    pub fn native_name_input(
        &mut self,
        h: &mut DecodedRecord,
        state: &mut EditorState,
        request: NameRequest<'_>,
        text_effect: &mut NameEffect,
        effects: &mut Effects,
        before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if h.kind() != 2 || h.index() >= 20_000 {
            return Err(invalid("normal name requires a normal typed holder"));
        }
        let paired = h.split() == 1 || request.frame.layout != 0;
        if let Some(mut candidate) = name_candidate_input(
            h,
            &state.pending,
            request.input,
            request.frame.layout,
            text_effect,
        )? {
            if h.valid() {
                candidate.resize(16, b' ');
                h.set_name(candidate.as_slice().try_into().expect("native name width"));
                self.native_save_cached(
                    h,
                    state,
                    CacheWrite {
                        key: 0x105,
                        previous: None,
                    },
                    request.context,
                    effects,
                    before_store,
                )?;
                text_effect.stored = true;
            } else {
                if !candidate.is_empty()
                    && self.used_slots() < 500
                    && request.notification.is_none()
                {
                    return Err(invalid(
                        "pending name requires the actual24-byte notification context",
                    ));
                }
                if self.native_pending_name(h, &candidate, &mut state.pending)? {
                    text_effect.pending_requested = request.notification;
                }
            }
        }
        request.frame.node_flags = (request.frame.node_flags & !1) | u8::from(paired);
        Ok(())
    }

    /// Existing-row choice handlers. Common upstream redirect and downstream
    /// document/OS dispatch remain separate from these actual two saver calls.
    pub fn native_choice_input(
        &mut self,
        h: &mut DecodedRecord,
        state: &mut EditorState,
        request: ChoiceRequest<'_>,
        effects: &mut Effects,
        before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
    ) -> Result<ChoiceOutcome, Error> {
        if h.kind() != 2 || h.index() >= 20_000 || !h.valid() {
            return Err(invalid(
                "normal choice case requires an existing normal typed holder",
            ));
        }
        if (request.field == ChannelChoice::Selection) != request.selection.is_some() {
            return Err(invalid(
                "SEL requires its actual layout/node flags; other choices have no SEL frame",
            ));
        }
        let paired = request
            .selection
            .as_ref()
            .is_some_and(|frame| h.split() == 1 || frame.layout != 0);
        let result = channel_choice_input(h, request.field, request.input)?;
        if result.stored {
            self.native_save_cached(
                h,
                state,
                CacheWrite {
                    key: request.field.key(),
                    previous: None,
                },
                request.context,
                effects,
                before_store,
            )?;
        }
        if let Some(frame) = request.selection {
            frame.node_flags = (frame.node_flags & !1) | u8::from(paired);
        }
        Ok(result)
    }

    /// Native 69977 case with the shared OnOff2 editor and two actual cached
    /// save/reloads. The notice callback is a PRE-REQUEST cutpoint, not a fake
    /// GUI dispatcher. Effects own the accepted requests; OS delivery is absent.
    pub fn native_split_input(
        &mut self,
        h: &mut DecodedRecord,
        state: &mut EditorState,
        request: SplitRequest<'_>,
        effects: &mut Effects,
        before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
        before_notice: impl FnMut(&Notification, &mut SplitFrame) -> Result<(), Error>,
    ) -> Result<SplitOutcome, Error> {
        if h.kind() != 2 || h.index() >= 20_000 || !h.valid() {
            return Err(invalid(
                "split case requires an existing normal typed holder",
            ));
        }
        channel_split_input_into(
            &mut self.working,
            h,
            state,
            request,
            effects,
            before_store,
            before_notice,
        )
    }
}

fn holder(model: &NormalMemory, index: u32) -> Result<DecodedRecord, Error> {
    let mut h = record::reset_holder(2, index, 0);
    record_image::load_channel_into(model.working(), ChannelKind::Normal, index, &mut h)?;
    Ok(h)
}
fn present(model: &NormalMemory, s: Selection, first: usize, last: usize) -> Result<bool, Error> {
    for side in first..=last {
        if holder(model, s.typed(side))?.valid() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// D4/D5/D7/D8/D9 basic-reference object, h=0, one live header pair.
/// Home and this MR reference are separate; no zero/default reference is used.
fn reference(
    model: &mut NormalMemory,
    action: u32,
    group: usize,
    first: usize,
    last: usize,
    direction: i32,
) -> Result<(), Error> {
    reference_into(
        &mut model.working,
        action,
        group,
        first,
        last,
        direction as u32 as usize,
    )
}

/// Borrowed shared D4/D5/D7/D8/D9 primitive. For D5, destination carries the
/// original signed 32 direction bits; the other actions do not consume it.
pub fn reference_into(
    w: &mut [u8],
    action: u32,
    group: usize,
    first: usize,
    last: usize,
    destination: usize,
) -> Result<(), Error> {
    let direction = destination as i32;
    let g = usize::from(raw::<1>(w, 0x392fa)?[0]);
    let index = usize::from(u16::from_le_bytes(raw(w, 0x392fb)?));
    if g == 255 || index == 65535 {
        return Ok(());
    }
    let matches = g == group && (first..=last).contains(&index);
    let clear = match action {
        0xd4 => {
            if g < 100 {
                u32::from_le_bytes(raw(w, super::MAP + 400 * g + 4 * index)?) == super::EMPTY
            } else if g == 100 {
                false
            } else {
                return Err(invalid("native MR group has no typed dataset"));
            }
        }
        0xd7 | 0xd8 => matches,
        0xd9 => g == group && index >= first && index.wrapping_add(last - first + 1) > 99,
        _ => false,
    };
    if clear {
        put(w, 0x392fa, &[255])?;
        put(w, 0x392fb, &u16::MAX.to_le_bytes())?;
        return Ok(());
    }
    let width = (last - first + 1) as i32;
    let delta = match action {
        0xd5 if matches => direction,
        0xd5 if g == group && direction < 0 && first > 0 && index == first - 1 => width,
        0xd5 if g == group && direction > 0 && index == last + 1 => -width,
        0xd8 if g == group && index > last => -width,
        0xd9 if g == group && index >= first => width,
        _ => 0,
    };
    if delta != 0 {
        put(
            w,
            0x392fb,
            &(index as u16).wrapping_add(delta as u16).to_le_bytes(),
        )?;
    }
    Ok(())
}

impl NormalMemory {
    fn carry_step(&self, s: Selection, d: i32) -> Result<usize, Error> {
        let active = holder(self, s.typed(s.active))?;
        Ok(
            if s.first == s.last
                && ((d < 0 && active.side() != 0)
                    || (d > 0 && active.side() == 0 && (active.split() != 0 || s.show_both)))
            {
                1
            } else {
                2
            },
        )
    }
    pub fn native_carry_target(
        &self,
        selection: Selection,
        direction: i32,
    ) -> Result<Option<(usize, usize)>, Error> {
        let s = selection.checked(self)?;
        let d = if direction < 0 { -1 } else { 1 };
        let step = self.carry_step(s, d)?;
        let (first, last) = if step == 2 {
            s.pair()
        } else {
            (s.first, s.last)
        };
        if (d < 0 && first < step) || (d > 0 && last + step >= 200) {
            return Ok(None);
        }
        Ok(Some((
            (first as i32 + d * step as i32) as usize,
            (last as i32 + d * step as i32) as usize,
        )))
    }
    pub fn native_copy(
        &self,
        selection: Selection,
        clipboard: &mut Clipboard,
        effects: &mut Effects,
    ) -> Result<(), Error> {
        let s = selection.checked(self)?;
        let active = holder(self, s.typed(s.active))?;
        let (first, last, request) = if s.first != s.last {
            let (a, b) = s.pair();
            (a, b, 1)
        } else if s.show_both || active.split() != 0 {
            (s.first, s.last, 2)
        } else {
            (s.first, s.last + 1, 1)
        };
        clipboard.copy_from(
            self.working(),
            CopySource {
                kind: 2,
                first: s.typed(first),
                count: last - first + 1,
                request,
                expanded: s.show_both,
                binding: active.binding(),
                band_enabled: None,
            },
        )?;
        effects.selected(first, last);
        Ok(())
    }
    pub fn native_clear(
        &mut self,
        selection: Selection,
        g: ReferenceImage<'_>,
        effects: &mut Effects,
        mut confirm: impl FnMut(Confirmation) -> Result<i32, Error>,
    ) -> Result<Outcome, Error> {
        let s = selection.checked(self)?;
        if present(self, s, s.first, s.last)? && confirm(Confirmation::Clear)? != 1 {
            return Ok(Outcome::default());
        }
        let (first, last) = s.pair();
        self.release_with_home(
            s.group,
            first / 2..last / 2 + 1,
            g,
            &mut effects.home_invalidated,
        )?;
        effects.map_saved(s.group);
        reference(self, 0xd7, s.group, first / 2, last / 2, 0)?;
        effects.selected(first, last);
        effects.queue(0x11, 0, 0);
        Ok(Outcome {
            processed: true,
            ..Outcome::default()
        })
    }
    pub fn native_delete(
        &mut self,
        selection: Selection,
        g: ReferenceImage<'_>,
        effects: &mut Effects,
        mut confirm: impl FnMut(Confirmation) -> Result<i32, Error>,
    ) -> Result<Outcome, Error> {
        let s = selection.checked(self)?;
        let selected = holder(self, s.typed(s.active))?;
        if s.first == s.last && selected.split() == 1 {
            if selected.valid() && confirm(Confirmation::Delete)? != 1 {
                return Ok(Outcome::default());
            }
            let mut other = holder(self, s.typed(s.first ^ 1))?;
            other.set_split(0);
            self.store_channel(s.typed(s.first), &mut other)?;
            effects.context_flags &= !4;
            effects.active = Some(s.first);
            effects.display_mode = Some(1);
            effects.cache_invalidated = true;
            effects.queue(0x11, 0, 0);
            return Ok(Outcome {
                processed: true,
                written: 1,
                native_count: 1,
                exhausted: false,
            });
        }
        let (first, last) = s.pair();
        if present(self, s, first, last)? && confirm(Confirmation::Delete)? != 1 {
            return Ok(Outcome::default());
        }
        self.delete_with_home(
            s.group,
            first / 2..last / 2 + 1,
            g,
            &mut effects.home_invalidated,
        )?;
        effects.map_saved(s.group);
        reference(self, 0xd8, s.group, first / 2, last / 2, 0)?;
        effects.context_flags &= !4;
        effects.active = Some(first);
        effects.display_mode = Some(1);
        effects.cache_invalidated = true;
        effects.queue(0x11, 0, 0);
        Ok(Outcome {
            processed: true,
            ..Outcome::default()
        })
    }
    pub fn native_insert(
        &mut self,
        selection: Selection,
        g: ReferenceImage<'_>,
        effects: &mut Effects,
        mut confirm: impl FnMut(Confirmation) -> Result<i32, Error>,
    ) -> Result<Outcome, Error> {
        let s = selection.checked(self)?;
        let (first, last) = s.pair();
        let count = (last - first).div_ceil(2);
        if present(self, s, 200 - 2 * count, 199)? && confirm(Confirmation::DiscardTail)? != 1 {
            return Ok(Outcome::default());
        }
        self.insert_with_home(s.group, first / 2, count, g, &mut effects.home_invalidated)?;
        effects.map_saved(s.group);
        reference(self, 0xd9, s.group, first / 2, last / 2, 0)?;
        effects.selected(first, last);
        effects.active = Some(first);
        effects.queue(0x11, 0, 0);
        Ok(Outcome {
            processed: true,
            ..Outcome::default()
        })
    }
    /// The map-only native step 2 path. Step 1 uses decoded copy/paste separately.
    pub fn native_carry_step2(
        &mut self,
        selection: Selection,
        direction: i32,
        g: ReferenceImage<'_>,
        effects: &mut Effects,
    ) -> Result<Outcome, Error> {
        let s = selection.checked(self)?;
        let (first, last) = s.pair();
        let d = if direction < 0 { -1 } else { 1 };
        if !self.carry_with_home(
            s.group,
            first / 2..last / 2 + 1,
            d,
            g,
            &mut effects.home_invalidated,
        )? {
            return Ok(Outcome::default());
        }
        reference(self, 0xd5, s.group, first / 2, last / 2, d)?;
        let first = (first as i32 + 2 * d) as usize;
        let last = (last as i32 + 2 * d) as usize;
        effects.selected(first, last);
        effects.active = Some(if d < 0 { first } else { last });
        effects.queue(0x11, 0, 0);
        Ok(Outcome {
            processed: true,
            ..Outcome::default()
        })
    }
    /// CB63C chooses step from the active side, split, direction and show-both.
    /// Step 1 uses two private decoded clipboards and deliberately skips D1E88.
    pub fn native_carry(
        &mut self,
        selection: Selection,
        direction: i32,
        g: ReferenceImage<'_>,
        effects: &mut Effects,
        mut before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
    ) -> Result<Outcome, Error> {
        let s = selection.checked(self)?;
        let d = if direction < 0 { -1 } else { 1 };
        let step1 = self.carry_step(s, d)? == 1;
        if !step1 {
            return self.native_carry_step2(s, d, g, effects);
        }
        if (d < 0 && s.first == 0) || (d > 0 && s.last == 199) {
            return Ok(Outcome::default());
        }
        let neighbor = (s.first as i32 + d) as usize;
        let binding = holder(self, s.typed(s.active))?.binding();
        let mut selected = Clipboard::default();
        let mut other = Clipboard::default();
        for (clipboard, index) in [(&mut selected, s.first), (&mut other, neighbor)] {
            clipboard.copy_from(
                self.working(),
                CopySource {
                    kind: 2,
                    first: s.typed(index),
                    count: 1,
                    request: 0,
                    expanded: false,
                    binding,
                    band_enabled: None,
                },
            )?;
        }
        let old = self.working.clone();
        transfer_into(
            &mut self.working,
            ChannelKind::Normal,
            &old,
            s.typed(neighbor),
            0,
            1,
            &mut selected,
            &mut before_store,
        )?;
        let old = self.working.clone();
        transfer_into(
            &mut self.working,
            ChannelKind::Normal,
            &old,
            s.typed(s.first),
            0,
            1,
            &mut other,
            &mut before_store,
        )?;
        effects.selected(neighbor, neighbor);
        effects.active = Some(neighbor);
        effects.queue(0x11, 0, 0);
        Ok(Outcome {
            processed: true,
            written: 2,
            native_count: 2,
            exhausted: false,
        })
    }
    /// CAB08/B3A50 normal destination. The old image is a read source, never a
    /// rollback target. Allocation and each typed store leave their real prefix.
    pub fn native_paste(
        &mut self,
        selection: Selection,
        clipboard: &mut Clipboard,
        g: ReferenceImage<'_>,
        effects: &mut Effects,
        mut confirm: impl FnMut(Confirmation) -> Result<i32, Error>,
        mut before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
    ) -> Result<Outcome, Error> {
        let s = selection.checked(self)?;
        clipboard.validate()?;
        let active = holder(self, s.typed(s.active))?;
        let mut start = s.active;
        if active.side() != 0
            && (s.merged || (clipboard.count(true) >= 2 && clipboard.count(false) != 1))
        {
            start -= 1;
        }
        let mode =
            if !s.merged && (s.show_both || active.split() != 0) && clipboard.count(false) == 1 {
                2
            } else {
                1
            };
        let requested = clipboard.count(mode != 2);
        let remaining = 200 - start;
        if requested > remaining && confirm(Confirmation::Truncate)? != 1 {
            return Ok(Outcome::default());
        }
        let limit = requested.min(remaining);
        if limit != 0
            && present(self, s, start, start + limit - 1)?
            && confirm(Confirmation::Overwrite)? != 1
        {
            return Ok(Outcome::default());
        }
        let old = self.working.clone();
        let row = start / 2;
        let rows = if clipboard.mode == 2 || mode == 2 {
            1
        } else {
            limit / 2
        };
        self.release_with_home(s.group, row..row + rows, g, &mut effects.home_invalidated)?;
        self.reset_home(s.group, g, &mut effects.home_invalidated)?;
        let mut processed = 0;
        let mut exhausted = false;
        while processed < rows && row + processed < 100 {
            let source_index = if matches!(clipboard.mode, 0 | 2) {
                processed
            } else {
                2 * processed
            };
            let Some(source) = clipboard.records.get(source_index) else {
                break;
            };
            if source.valid()
                && self
                    .allocate(s.group, row + processed..row + processed + 1)?
                    .allocated
                    == 0
            {
                exhausted = true;
                break;
            }
            processed += 1;
        }
        let result = transfer_into(
            &mut self.working,
            ChannelKind::Normal,
            &old,
            s.typed(start),
            mode,
            2 * processed,
            clipboard,
            &mut before_store,
        )?;
        effects.map_saved(s.group);
        reference(
            self,
            0xd4,
            s.group,
            row,
            row + processed.saturating_sub(1),
            0,
        )?;
        let last = (start as u32)
            .wrapping_add(result.native_count as u32)
            .wrapping_sub(1) as usize;
        effects.selected(start, last);
        effects.active = Some(start);
        effects.queue(0x11, 0, 0);
        Ok(Outcome {
            processed: true,
            exhausted,
            ..result
        })
    }
}

/// A77C mode corrections reuse the shared engine but project only its DATA and
/// filter results. Offset rounding/tone normalization are NOT paste effects.
fn prepare_source(h: &mut DecodedRecord) {
    let mut v = crate::icf::normalizer::ChannelValues {
        mode: h.mode(),
        data: h.data(),
        filter: h.filter(),
        offset_hz: h.offset_hz(),
        tx_tone: h.tx_tone(),
        rx_tone: h.rx_tone(),
    };
    crate::icf::normalizer::normalize_channel_values(&mut v, false, false);
    h.set_data(v.data);
    h.set_filter(v.filter);
}

// Cross-kind paste keeps the source kind in the clipboard header. Only the
// explicit destination kind/index resolves storage: normal logical sides use
// the map, while call sides address four fixed raw83 pairs. Mode 2 borrows target
// name/split/SEL from the pre-write snapshot, never from a partly updated target.
fn write_source(
    image: &mut [u8],
    kind: ChannelKind,
    old: &[u8],
    index: u32,
    source: &mut DecodedRecord,
    preserve: bool,
    before_store: &mut impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut target = record::reset_holder(0, index, 0);
    record_image::load_channel_into(old, kind, index, &mut target)?;
    prepare_source(source);
    let saved = (
        source.split(),
        *source.name(),
        source.channel_field(ChannelField::Selection),
    );
    if preserve {
        source.set_split(target.split());
        source.set_name(target.name());
        source.set_channel_field(
            ChannelField::Selection,
            target.channel_field(ChannelField::Selection),
        );
    }
    source.set_header(source.kind(), index, target.binding());
    if record_image::channel_address(image, kind, index)?.is_some() {
        record_image::store_channel_into(image, kind, index, source, |write| {
            before_store(index, write)
        })?;
    } else if source.valid() {
        return Err(invalid("native valid paste destination was not allocated"));
    }
    if preserve {
        source.set_split(saved.0);
        source.set_name(&saved.1);
        source.set_channel_field(ChannelField::Selection, saved.2);
    }
    Ok(())
}

/// A124/A328: preserve the FE double-call and separate returned native counter.
#[allow(
    clippy::too_many_arguments,
    reason = "The pre-write target snapshot, live target and mutable clipboard have distinct ownership and failure effects."
)]
fn transfer_into(
    image: &mut [u8],
    kind: ChannelKind,
    old: &[u8],
    mut start: u32,
    mode: u32,
    mut limit: usize,
    clipboard: &mut Clipboard,
    before_store: &mut impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<Outcome, Error> {
    let mut duplicate = false;
    let mut preserve = false;
    let mut compressed = false;
    match clipboard.mode {
        0 | 2 => {
            if mode == 0 {
            } else if mode == 2 {
                let mut target = record::reset_holder(0, start, 0);
                record_image::load_channel_into(old, kind, start, &mut target)?;
                if target.valid() {
                    preserve = true;
                } else {
                    if target.side() != 0 {
                        start -= 1;
                    }
                    duplicate = true;
                }
            } else {
                duplicate = true;
            }
        }
        3 if mode == 2 && clipboard.logical_count == 1 => {
            if limit != 1 {
                limit = limit.wrapping_sub(1);
            }
            let mut target = record::reset_holder(0, start, 0);
            record_image::load_channel_into(old, kind, start, &mut target)?;
            if target.valid() {
                preserve = true;
            } else {
                if target.side() != 0 {
                    start -= 1;
                }
                duplicate = true;
            }
        }
        3 if mode == 0 => compressed = true,
        _ => {}
    }
    let mut writes = 0;
    for source in &mut clipboard.records {
        if writes >= limit {
            break;
        }
        if compressed && source.side() != 0 && source.split() == 0 {
            continue;
        }
        write_source(
            image,
            kind,
            old,
            start.wrapping_add(writes as u32),
            source,
            preserve,
            before_store,
        )?;
        writes += 1;
        if duplicate {
            write_source(
                image,
                kind,
                old,
                start.wrapping_add(writes as u32),
                source,
                false,
                before_store,
            )?;
            writes += 1;
        }
    }
    let native_count = if duplicate && mode == 2 {
        writes.wrapping_sub(1)
    } else {
        writes
    };
    Ok(Outcome {
        processed: true,
        written: writes,
        native_count,
        exhausted: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_entry_uses_signed_code_source_delta_and_keeps_callsign_option_separate() {
        let mut h = record::reset_holder(2, 10, 7);
        h.set_group(4);
        h.set_valid(false);
        let caller = ChannelCaller {
            code: -1,
            command: 0,
            blocked: false,
            node_index: i32::MIN,
            node_back: 1,
        };
        assert_eq!(
            channel_entry(&h, 0x108, caller, false).unwrap(),
            ChannelEntry::Redirect {
                key: 0x107,
                index: i32::MAX,
                code: -1
            }
        );
        assert_eq!(
            channel_entry(&h, 0x114, caller, true).unwrap(),
            ChannelEntry::Edit
        );
        assert!(matches!(
            channel_entry(&h, 0x114, caller, false).unwrap(),
            ChannelEntry::Redirect { .. }
        ));
        assert_eq!(
            channel_entry(&h, 0x105, caller, false).unwrap(),
            ChannelEntry::Edit
        );
        assert_eq!(
            channel_entry(
                &h,
                0x108,
                ChannelCaller {
                    code: 0x10008,
                    ..caller
                },
                false
            )
            .unwrap(),
            ChannelEntry::Edit
        );
        assert_eq!(
            channel_entry(
                &h,
                0x108,
                ChannelCaller {
                    command: 0xdc,
                    ..caller
                },
                true
            )
            .unwrap(),
            ChannelEntry::Blocked
        );
        h.set_valid(true);
        h.set_channel_field(ChannelField::FrequencyHz, 470_000_000);
        assert_eq!(
            channel_entry(&h, 0x108, caller, false).unwrap(),
            ChannelEntry::Edit
        );
        h.set_channel_field(ChannelField::FrequencyHz, 470_000_001);
        assert!(matches!(
            channel_entry(&h, 0x108, caller, false).unwrap(),
            ChannelEntry::Redirect { .. }
        ));
        let payload = name_notification(&h, 701).unwrap();
        let words: Vec<_> = payload
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_le_bytes(*b))
            .collect();
        assert_eq!(words, [3, 701, 0x105, 4, 0, 2]);
    }

    #[test]
    fn callsign_modal_wrapper_keeps_d1_false_notices_and_d2_cancel_two_savers() {
        use crate::icf::{MemoryCallsign, callsign_dialog as d};
        let ranges = crate::icf::frequency::Ranges::generate(
            &crate::icf::context::Context::from_bytes([0; 24]),
            None,
        )
        .unwrap();
        let mut w = vec![0; 0x4997a];
        let mut h = record::reset_holder(3, 0, 17);
        h.set_valid(true);
        h.set_mode(16);
        h.set_channel_field(ChannelField::FrequencyHz, 145_000_000);
        h.set_channel_field(ChannelField::Duplex, 0);
        h.set_callsign(MemoryCallsign::Your, b"CQCQCQ  ");
        record_image::store_channel_into(&mut w, ChannelKind::Call, 0, &mut h, |_| Ok(())).unwrap();
        let current = w.clone();
        let sources = d::Sources {
            current: &current,
            defaults: &[],
            ranges: &ranges,
            system_langid: None,
        };
        let reads = d::ReadInputs {
            ur_name_tail: Some([b' '; 8]),
            blank_source_index: Some(2500),
            string_tail: &[0; 16],
            initial_first_marker: None,
        };
        let context = EditorContext {
            ranges: &ranges,
            generation: ReferenceImage::Separate(&[]),
        };
        let mut state = EditorState::initialized();
        for (modal, count, stored, accepted, ids) in [
            (
                CallsignModal::List(d::ListSelection {
                    category: 0,
                    name: 0,
                    call: 0,
                    modal_result: 0,
                    checkbox: false,
                }),
                0,
                false,
                false,
                vec![0x11, 0x39],
            ),
            (
                CallsignModal::List(d::ListSelection {
                    category: 0,
                    name: 0,
                    call: 0,
                    modal_result: 0x100,
                    checkbox: false,
                }),
                1,
                true,
                true,
                vec![0x11, 0x39],
            ),
            (
                CallsignModal::Dr { modal_result: 2 },
                2,
                true,
                false,
                vec![0x18, 0x18, 0x3c],
            ),
        ] {
            let mut effects = Effects::default();
            let mut saves = 0;
            let out = channel_callsign_modal_input_into(
                &mut w,
                &mut h,
                &mut state,
                CallsignModalRequest {
                    field: MemoryCallsign::Your,
                    modal,
                    sources: &sources,
                    reads: &reads,
                    context,
                },
                &mut effects,
                |dialog| dialog.action(&sources, d::Action::UrText(b"NEWCALL"), &reads),
                |_, write| {
                    if write.stage() == record::ChannelStage::Frequency {
                        saves += 1;
                    }
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(
                (out.stored, out.accepted, out.al),
                (stored, accepted, false)
            );
            assert_eq!(saves, count);
            assert_eq!(
                effects
                    .notifications
                    .iter()
                    .map(|n| n.id)
                    .collect::<Vec<_>>(),
                ids
            );
            assert_eq!(h.callsign(MemoryCallsign::Your), b"CQCQCQ  ");
        }
        assert_ne!(
            u32::from_le_bytes(h.as_bytes()[0xe4..0xe8].try_into().unwrap()),
            0
        );
        let mut effects = Effects::default();
        assert!(
            channel_callsign_modal_input_into(
                &mut w,
                &mut h,
                &mut state,
                CallsignModalRequest {
                    field: MemoryCallsign::Your,
                    modal: CallsignModal::Dr { modal_result: 1 },
                    sources: &sources,
                    reads: &reads,
                    context
                },
                &mut effects,
                |_| Err(invalid("injected action failure")),
                |_, _| panic!("save after action error")
            )
            .is_err()
        );
        assert_eq!(
            effects
                .notifications
                .iter()
                .map(|n| n.id)
                .collect::<Vec<_>>(),
            [0x18]
        );
    }
    #[test]
    fn callsign_modal_initial_source_allocates_and_mirrors_only_after_accept() {
        use crate::icf::{MemoryCallsign, callsign_dialog as d};
        let ranges = crate::icf::frequency::Ranges::generate(
            &crate::icf::context::Context::from_bytes([0; 24]),
            None,
        )
        .unwrap();
        let mut w = model().working().to_vec();
        let before = NormalMemory::from_working(w.clone()).unwrap().used_slots();
        let mut current = w.clone();
        current[0x49914..0x49978].fill(0);
        current[0x49914..0x49916].copy_from_slice(&1u16.to_le_bytes());
        current[0x49978..0x4997a].fill(0);
        let mut source = record::reset_holder(0x0c, 0, 0);
        source.set_valid(true);
        source.set_name(b"SELF SOURCE     ");
        source.set_repeater_field(record::RepeaterField::Mode, 2);
        source.set_repeater_text(record::RepeaterText::Call, b"RPT777 A");
        source.set_repeater_text(record::RepeaterText::Gateway, b"RPT777 G");
        source.set_channel_field(ChannelField::FrequencyHz, 145_250_000);
        source.set_channel_field(ChannelField::Duplex, 1);
        source.set_offset_hz(600_000);
        crate::repeater::working::save_into(&mut current, 0, &source).unwrap();
        let defaults = vec![0; 0x4997a];
        let sources = d::Sources {
            current: &current,
            defaults: &defaults,
            ranges: &ranges,
            system_langid: None,
        };
        let reads = d::ReadInputs {
            blank_source_index: Some(2500),
            string_tail: &[0; 16],
            initial_first_marker: Some(7),
            ..Default::default()
        };
        let context = EditorContext {
            ranges: &ranges,
            generation: ReferenceImage::Separate(&[]),
        };
        let mut state = EditorState::initialized();
        for result in [2, 1] {
            let mut h = record::reset_holder(2, 10, 31);
            record_image::load_channel_into(&w, ChannelKind::Normal, 10, &mut h).unwrap();
            assert!(!h.valid());
            let mut saves = 0;
            let out = channel_callsign_modal_input_into(
                &mut w,
                &mut h,
                &mut state,
                CallsignModalRequest {
                    field: MemoryCallsign::Repeater1,
                    modal: CallsignModal::Dr {
                        modal_result: result,
                    },
                    sources: &sources,
                    reads: &reads,
                    context,
                },
                &mut Effects::default(),
                |dialog| {
                    dialog.action(
                        &sources,
                        d::Action::SourceList(d::ListSelection {
                            category: 1,
                            name: 0,
                            call: 0,
                            modal_result: 1,
                            checkbox: false,
                        }),
                        &reads,
                    )
                },
                |_, write| {
                    if write.stage() == record::ChannelStage::Frequency {
                        saves += 1;
                    }
                    Ok(())
                },
            )
            .unwrap();
            assert!(out.stored);
            assert_eq!(out.al, result == 1);
            assert_eq!(saves, if result == 1 { 3 } else { 0 });
            let model = NormalMemory::from_working(w.clone()).unwrap();
            assert_eq!(model.used_slots(), before + usize::from(result == 1));
            if result == 1 {
                for index in [10, 11] {
                    let h = holder(&model, index).unwrap();
                    assert!(h.valid());
                    assert_eq!(h.channel_field(ChannelField::FrequencyHz), 145_250_000);
                    assert_eq!(h.mode(), 16);
                    assert_eq!(h.callsign(MemoryCallsign::Your), b"CQCQCQ  ");
                    assert_eq!(h.callsign(MemoryCallsign::Repeater1), b"RPT777 A");
                }
            }
        }
    }
    use record::ChannelStage;
    fn model() -> NormalMemory {
        let mut memory = vec![0xa6; 0x4997a];
        memory[0x355a1] = 6;
        memory[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
        memory[super::super::GROUP_COUNT..super::super::GROUP_COUNT + 4]
            .copy_from_slice(&2u32.to_le_bytes());
        memory[super::super::COUNTS..super::super::MAP].fill(0);
        for cell in memory[super::super::MAP..0x49914].as_chunks_mut::<4>().0 {
            cell.copy_from_slice(&65535u32.to_le_bytes());
        }
        memory[super::super::FLAGS..super::super::FLAGS + 500].fill(1);
        memory[0x392fa] = 255;
        memory[0x392fb..0x392fd].fill(255);
        NormalMemory::from_working(memory).unwrap()
    }
    fn selection(first: usize, last: usize) -> Selection {
        Selection {
            group: 0,
            active: first,
            first,
            last,
            merged: false,
            show_both: false,
        }
    }
    fn editor_state(index: u32) -> EditorState {
        let mut mode = record::reset_holder(2, index, 77);
        mode.set_valid(false);
        let mut filter = record::reset_holder(2, index, 88);
        filter.set_valid(false);
        EditorState {
            mode_cache: mode,
            filter_cache: filter,
            filter_enabled: 1,
            pending: PendingName {
                active: false,
                index: u32::MAX,
                bytes: Vec::new(),
            },
        }
    }
    #[test]
    fn shared_default_filter_reads_only_the_reached_g_byte_and_preserves_holder() {
        let q = crate::icf::context::Context::from_bytes([0; 24]);
        let ranges = crate::icf::frequency::Ranges::generate(&q, None).unwrap();
        let mut generation = vec![0; 0x37823 + 27];
        generation[0x37823] = 3;
        generation[0x37824] = 11;
        for (mode, data, expected) in [(0, 1, 11), (0x100, 1, 3)] {
            let mut h = DecodedRecord::from_bytes([0xa4; 616]);
            h.set_header(2, 4, 0x8765_4321);
            h.set_mode(mode);
            h.set_data(data);
            h.set_filter(17);
            h.set_channel_field(ChannelField::FrequencyHz, 14_100_000);
            let mut expected_bytes = *h.as_bytes();
            expected_bytes[0x60..0x64].copy_from_slice(&u32::to_le_bytes(expected));
            default_filter_into(&generation, &ranges, &mut h).unwrap();
            assert_eq!(h.as_bytes(), &expected_bytes);
        }
        let mut h = record::reset_holder(2, 4, 19);
        h.set_filter(17);
        h.set_mode(0x11);
        h.set_channel_field(ChannelField::FrequencyHz, 14_100_000);
        let before = h.clone();
        default_filter_into(&[], &ranges, &mut h).unwrap();
        assert_eq!(h.as_bytes(), before.as_bytes());
        h.set_mode(0);
        let before = h.clone();
        assert!(default_filter_into(&[], &ranges, &mut h).is_err());
        assert_eq!(h.as_bytes(), before.as_bytes());
        default_filter_into(&[], &crate::icf::frequency::Ranges::unpopulated(), &mut h).unwrap();
        assert_eq!(h.as_bytes(), before.as_bytes());
    }
    #[test]
    fn borrowed_reference_actions_keep_literal_deltas_and_unrelated_bytes() {
        for (action, index, direction, occupied, group, expected) in [
            (0xd4, 3, 0, false, 255, 65535),
            (0xd4, 3, 0, true, 0, 3),
            (0xd7, 3, 0, false, 255, 65535),
            (0xd7, 7, 0, false, 0, 7),
            (0xd8, 3, 0, false, 255, 65535),
            (0xd8, 7, 0, false, 0, 4),
            (0xd9, 97, 0, false, 255, 65535),
            (0xd9, 7, 0, false, 0, 10),
            (0xd5, 3, -1i32, false, 0, 2),
            (0xd5, 1, -1, false, 0, 4),
            (0xd5, 5, 1, false, 0, 2),
        ] {
            let mut m = model();
            if occupied {
                m.allocate(0, 3..4).unwrap();
            }
            let mut w = m.working().to_vec();
            w[0x392fa] = 0;
            w[0x392fb..0x392fd].copy_from_slice(&u16::to_le_bytes(index));
            let mut after = w.clone();
            after[0x392fa] = group;
            after[0x392fb..0x392fd].copy_from_slice(&u16::to_le_bytes(expected));
            reference_into(&mut w, action, 0, 2, 4, direction as u32 as usize).unwrap();
            assert_eq!(w, after);
        }
    }
    fn split_frame() -> SplitFrame {
        SplitFrame {
            layout: 0,
            pre_step: 1,
            selection_first: 12,
            node_index: 47,
            node_flags: 0xa5,
            notify_flags: 1,
            active: 62,
            display_mode: 7,
        }
    }
    #[test]
    fn choice_catalog_predicate_and_call_read_only_sel_keep_native_domains() {
        use crate::icf::settings::extended::ChoiceInput;
        let mut h = record::reset_holder(2, 4, 0);
        for (mode, old, values) in [
            (17, 0, vec![0, 3]),
            (17, 3, vec![0, 3]),
            (17, 4, vec![0, 4]),
            (13, 3, vec![0, 1, 2, 3]),
            (13, 4, vec![0, 1, 2, 4]),
            (17, 255, vec![0, 3, 4]),
        ] {
            h.set_mode(mode);
            h.set_channel_field(ChannelField::Duplex, old);
            assert_eq!(ChannelChoice::Duplex.choices(&h), values);
        }
        h.set_header(3, 0, 0);
        assert!(channel_choice_input(&mut h, ChannelChoice::Selection, ChoiceInput::Next).is_err());
        h.set_mode(16);
        h.set_valid(true);
        h.set_channel_field(ChannelField::DigitalSquelch, 255);
        let out =
            channel_choice_input(&mut h, ChannelChoice::DigitalSquelch, ChoiceInput::Previous)
                .unwrap();
        assert_eq!(out.candidate, Some(0));
        assert!(!out.mode_blocked);
        h.set_mode(0x110);
        let before = h.clone();
        let out =
            channel_choice_input(&mut h, ChannelChoice::DigitalSquelch, ChoiceInput::Next).unwrap();
        assert!(out.mode_blocked);
        assert_eq!(h.as_bytes(), before.as_bytes());
    }
    #[test]
    fn offset_native_grammar_and_unsigned_rounding_use_shared_helpers() {
        use crate::icf::settings::extended::NumberInput;
        for (text, candidate, accepted, stored) in [
            (&b"0.000149"[..], 149, true, 100),
            (b"0.00015", 150, true, 200),
            (b"4.0000005", 4_000_000, true, 4_000_000),
            (b"1e-3", 1_000_000, true, 1_000_000),
            (b"9.99995", 9_999_900, true, 9_999_900),
            (b"-", 0, true, 0),
            (b".", 0, true, 0),
            (b"foo", 0, false, 149),
        ] {
            let mut h = record::reset_holder(3, 0, 0);
            h.set_offset_hz(149);
            h.set_split(1);
            let out = channel_offset_input(&mut h, NumberInput::Text(text), b'.').unwrap();
            assert_eq!((out.candidate, out.accepted), (candidate, accepted));
            assert_eq!(h.offset_hz(), stored);
        }
        let ranges = crate::icf::frequency::Ranges::unpopulated();
        let mut m = split_model(1);
        let mut h = holder(&m, 4).unwrap();
        let other = m.record(0, 2).unwrap().unwrap()[34..67].to_vec();
        let mut saves = 0;
        let out = m
            .native_offset_input(
                &mut h,
                &mut EditorState::initialized(),
                OffsetRequest {
                    input: NumberInput::Text(b"0.00015"),
                    point: b'.',
                    context: EditorContext {
                        ranges: &ranges,
                        generation: ReferenceImage::Separate(&[]),
                    },
                },
                &mut Effects::default(),
                |_, w| {
                    if w.stage() == ChannelStage::Frequency {
                        saves += 1;
                    }
                    Ok(())
                },
            )
            .unwrap();
        assert!(out.accepted);
        assert_eq!(saves, 2);
        assert_eq!(h.offset_hz(), 200);
        assert_eq!(&m.record(0, 2).unwrap().unwrap()[34..67], other);
    }
    #[test]
    fn mode_data_filter_use_real_keys_catalog_order_and_false_caller_state() {
        use crate::icf::settings::{Selection as InputSelection, extended::ChoiceInput};
        let ranges = crate::icf::frequency::Ranges::unpopulated();
        let mut h = record::reset_holder(2, 4, 0);
        h.set_valid(true);
        h.set_mode(13);
        assert_eq!(
            ChannelEnum::Mode.choices(&h),
            vec![0, 1, 2, 3, 6, 7, 8, 13, 16, 15]
        );
        let before = h.clone();
        let wrong = channel_enum_input(
            &mut h,
            ChannelEnum::Mode,
            ChoiceInput::Selected(InputSelection {
                key: 1,
                phase: 2,
                candidate: 6,
                accepted: true,
            }),
        )
        .unwrap();
        assert!(!wrong.stored);
        assert_eq!(h.as_bytes(), before.as_bytes());
        let next = channel_enum_input(&mut h, ChannelEnum::Mode, ChoiceInput::Next).unwrap();
        assert_eq!(next.candidate, Some(16));
        let full = channel_enum_input(
            &mut h,
            ChannelEnum::Mode,
            ChoiceInput::Selected(InputSelection {
                key: 7,
                phase: 2,
                candidate: 0x106,
                accepted: true,
            }),
        )
        .unwrap();
        assert!(full.stored);
        assert_eq!(h.mode(), 0x106);
        h.set_mode(0x10d);
        let blocked = channel_enum_input(&mut h, ChannelEnum::Data, ChoiceInput::Begin).unwrap();
        assert!(blocked.mode_blocked);
        assert!(!blocked.popup_requested);
        h.set_mode(0x10f);
        let before = h.clone();
        let blocked = channel_enum_input(&mut h, ChannelEnum::Filter, ChoiceInput::Next).unwrap();
        assert!(blocked.mode_blocked);
        assert_eq!(blocked.candidate, Some(1));
        assert!(!blocked.stored);
        assert_eq!(h.as_bytes(), before.as_bytes());
        for (mode, bits) in [(24, 0x85), (0x118, 0x81)] {
            let mut m = split_model(0);
            let mut h = holder(&m, 4).unwrap();
            h.set_mode(mode);
            let out = m
                .native_enum_input(
                    &mut h,
                    &mut EditorState::initialized(),
                    EnumRequest {
                        field: ChannelEnum::Mode,
                        input: ChoiceInput::Cancel,
                        context: EditorContext {
                            ranges: &ranges,
                            generation: ReferenceImage::Separate(&[]),
                        },
                        warning_bits: Some(0x81),
                    },
                    &mut Effects::default(),
                    |_, _| panic!("mode false cannot save"),
                )
                .unwrap();
            assert!(!out.choice.stored);
            assert_eq!(out.context_state, Some(2));
            assert_eq!(out.warning_bits, Some(bits));
        }
    }

    #[test]
    fn mode_data_filter_share_two_savers_previous_holder_cache_and_queue_rules() {
        use crate::icf::settings::{Selection as InputSelection, extended::ChoiceInput};
        let q = crate::icf::context::Context::from_bytes([0; 24]);
        let ranges = crate::icf::frequency::Ranges::generate(&q, None).unwrap();
        let mut g = vec![0; 0x37823 + 27];
        g[0x37823..].fill(9);
        for (field, key, candidate, expected) in [
            (ChannelEnum::Mode, 7, 0x106, 6),
            (ChannelEnum::Data, 1, 0x101, 1),
            (ChannelEnum::Filter, 1, 0x103, 3),
        ] {
            let mut m = split_model(0);
            let mut h = holder(&m, 4).unwrap();
            if field == ChannelEnum::Data {
                h.set_mode(0);
                h.set_data(0);
                m.store_channel(4, &mut h).unwrap();
            }
            let before = m.record(0, 2).unwrap().unwrap()[34..67].to_vec();
            let mut state = editor_state(4);
            let mut effects = Effects::default();
            let mut saves = 0;
            let out = m
                .native_enum_input(
                    &mut h,
                    &mut state,
                    EnumRequest {
                        field,
                        input: ChoiceInput::Selected(InputSelection {
                            key,
                            phase: 2,
                            candidate,
                            accepted: true,
                        }),
                        context: EditorContext {
                            ranges: &ranges,
                            generation: ReferenceImage::Separate(&g),
                        },
                        warning_bits: if field == ChannelEnum::Mode {
                            Some(0)
                        } else {
                            None
                        },
                    },
                    &mut effects,
                    |_, w| {
                        if w.stage() == ChannelStage::Frequency {
                            saves += 1;
                        }
                        Ok(())
                    },
                )
                .unwrap();
            assert!(out.choice.stored);
            assert_eq!(out.choice.candidate, Some(candidate));
            assert_eq!(saves, 2);
            assert_eq!(&m.record(0, 2).unwrap().unwrap()[34..67], before);
            assert_eq!(
                match field {
                    ChannelEnum::Mode => h.mode(),
                    ChannelEnum::Data => h.data(),
                    ChannelEnum::Filter => h.filter(),
                },
                expected
            );
            assert_eq!(
                effects
                    .notifications
                    .iter()
                    .filter(|n| n.id == 0x3c)
                    .count(),
                usize::from(field == ChannelEnum::Mode)
            );
            if field == ChannelEnum::Data {
                assert_eq!(h.filter(), 9);
                assert_eq!(state.filter_cache.data(), 0);
            }
        }
    }
    #[test]
    fn tone_sources_typed_nearest_resource_ties_and_mode3_are_distinct() {
        use crate::icf::settings::{Selection as InputSelection, extended::NumberInput};
        let q0 = crate::icf::context::Context::from_bytes([0; 24]);
        let mut bytes = [0; 24];
        bytes[0x12] = 1;
        let q1 = crate::icf::context::Context::from_bytes(bytes);
        assert_eq!(tone_choices(ToneField::Tx, Some(&q0)).unwrap().len(), 51);
        assert_eq!(tone_choices(ToneField::Rx, Some(&q1)).unwrap().len(), 50);
        let dtcs = tone_choices(ToneField::Dtcs, None).unwrap();
        assert_eq!(dtcs.len(), 104);
        assert!(dtcs.iter().position(|&i| i == 20) < dtcs.iter().position(|&i| i == 19));
        for (field, text, q, candidate) in [
            (ToneField::Tx, &b"1e2"[..], &q0, 12),
            (ToneField::Rx, b"0000000000088.5", &q0, 8),
            (ToneField::Tx, b" 8.85e1junk ", &q0, 8),
            (ToneField::Rx, b"150", &q0, 50),
            (ToneField::Rx, b"150", &q1, 24),
            (ToneField::Dtcs, b"24", &q0, 0),
            (ToneField::Dtcs, b"123.5", &q0, 20),
            (ToneField::Dtcs, b"1e100", &q0, 0),
            (ToneField::Dtcs, b"", &q0, 0),
        ] {
            let mut h = record::reset_holder(3, 0, 0);
            h.set_mode(13);
            h.set_valid(true);
            let mut effect = ToneEffect::default();
            let mut upper = |s: &mut [u8]| {
                s.make_ascii_uppercase();
                Ok(())
            };
            channel_tone_input(
                &mut h,
                &mut ToneInput {
                    field,
                    input: NumberInput::Text(text),
                    point: b'.',
                    live_q: Some(q),
                    inline: false,
                    first: None,
                    upper: &mut upper,
                },
                &mut effect,
            )
            .unwrap();
            assert!(effect.stored);
            assert_eq!(effect.candidate, Some(candidate));
            assert_eq!(field.value(&h), candidate as u32);
        }
        let mut h = record::reset_holder(2, 4, 0);
        h.set_mode(13);
        h.set_valid(true);
        h.set_channel_field(ChannelField::Tone, 2);
        let mut effect = ToneEffect::default();
        let mut upper = |_: &mut [u8]| panic!("mode3 cannot use text upper");
        channel_tone_input(
            &mut h,
            &mut ToneInput {
                field: ToneField::Tone,
                input: NumberInput::Text(b"4"),
                point: b'.',
                live_q: None,
                inline: false,
                first: None,
                upper: &mut upper,
            },
            &mut effect,
        )
        .unwrap();
        assert!(!effect.stored);
        assert_eq!(h.channel_field(ChannelField::Tone), 2);
        channel_tone_input(
            &mut h,
            &mut ToneInput {
                field: ToneField::Tone,
                input: NumberInput::Selected(InputSelection {
                    key: 1,
                    phase: 2,
                    candidate: 0x104,
                    accepted: true,
                }),
                point: b'.',
                live_q: None,
                inline: false,
                first: None,
                upper: &mut upper,
            },
            &mut effect,
        )
        .unwrap();
        assert!(effect.stored);
        assert_eq!(h.channel_field(ChannelField::Tone), 0x104);
        channel_tone_input(
            &mut h,
            &mut ToneInput {
                field: ToneField::Tone,
                input: NumberInput::Cancel,
                point: b'.',
                live_q: None,
                inline: false,
                first: None,
                upper: &mut upper,
            },
            &mut effect,
        )
        .unwrap();
        assert!(!effect.stored);
        assert!(effect.candidate.is_none());
    }

    #[test]
    fn tone_actual_two_savers_full_candidate_nls_prefix_and_inline_requests() {
        use crate::icf::settings::{Selection as InputSelection, extended::NumberInput};
        let ranges = crate::icf::frequency::Ranges::unpopulated();
        let q = crate::icf::context::Context::from_bytes([0; 24]);
        for (field, candidate, expected) in [
            (ToneField::Tone, 0x104, 4),
            (ToneField::Tx, 0x14d, 77),
            (ToneField::Rx, 0x14e, 78),
            (ToneField::Dtcs, 0x163, 99),
        ] {
            let mut m = split_model(0);
            let before = m.record(0, 2).unwrap().unwrap()[34..67].to_vec();
            let mut h = holder(&m, 4).unwrap();
            let mut state = EditorState::initialized();
            let mut effect = ToneEffect::default();
            let mut saves = 0;
            let mut upper = |_: &mut [u8]| panic!("selected cannot use text upper");
            m.native_tone_input(
                &mut h,
                &mut state,
                ToneRequest {
                    input: ToneInput {
                        field,
                        input: NumberInput::Selected(InputSelection {
                            key: 1,
                            phase: 2,
                            candidate,
                            accepted: true,
                        }),
                        point: b'.',
                        live_q: Some(&q),
                        inline: false,
                        first: None,
                        upper: &mut upper,
                    },
                    context: EditorContext {
                        ranges: &ranges,
                        generation: ReferenceImage::Separate(&[]),
                    },
                },
                &mut effect,
                &mut Effects::default(),
                |_, w| {
                    if w.stage() == ChannelStage::Frequency {
                        saves += 1;
                    }
                    Ok(())
                },
            )
            .unwrap();
            assert!(effect.stored);
            assert_eq!(effect.candidate, Some(candidate));
            assert_eq!(field.value(&h), expected);
            assert_eq!(saves, 2);
            assert_eq!(&m.record(0, 2).unwrap().unwrap()[34..67], before);
        }
        let mut h = record::reset_holder(2, 4, 0);
        h.set_mode(13);
        h.set_valid(true);
        h.set_tx_tone(8);
        let mut effect = ToneEffect::default();
        let mut upper = |_: &mut [u8]| panic!("keyboard cannot use text upper");
        for first in [None, Some(b'7')] {
            channel_tone_input(
                &mut h,
                &mut ToneInput {
                    field: ToneField::Tx,
                    input: NumberInput::Begin,
                    point: b'.',
                    live_q: Some(&q),
                    inline: true,
                    first,
                    upper: &mut upper,
                },
                &mut effect,
            )
            .unwrap();
            assert!(!effect.stored);
            assert_eq!(effect.state_out, 2);
            assert_eq!(
                effect.inline_text,
                Some(if first.is_some() {
                    b"7".to_vec()
                } else {
                    b"88.5".to_vec()
                })
            );
        }
        let before = h.clone();
        let mut upper = |s: &mut [u8]| {
            s[0] = b'X';
            Err(crate::csv::Error::Operation("actual upper failure".into()))
        };
        assert!(
            channel_tone_input(
                &mut h,
                &mut ToneInput {
                    field: ToneField::Tx,
                    input: NumberInput::Text(b" 88.5 "),
                    point: b'.',
                    live_q: Some(&q),
                    inline: false,
                    first: None,
                    upper: &mut upper
                },
                &mut effect
            )
            .is_err()
        );
        assert_eq!(effect.shared_input, b"X8.5");
        assert_eq!(h.as_bytes(), before.as_bytes());
        assert!(!effect.stored);
        h.set_mode(16);
        let mut upper = |_: &mut [u8]| panic!("mode guard precedes source/text");
        channel_tone_input(
            &mut h,
            &mut ToneInput {
                field: ToneField::Tx,
                input: NumberInput::Text(b"88.5"),
                point: b'.',
                live_q: None,
                inline: false,
                first: None,
                upper: &mut upper,
            },
            &mut effect,
        )
        .unwrap();
        assert!(effect.mode_blocked);
        assert!(!effect.stored);
    }

    #[test]
    fn frequency_input_two_pass_pending_false_and_failure_prefixes_are_native() {
        use crate::icf::settings::extended::NumberInput;
        let q = crate::icf::context::Context::from_bytes([0; 24]);
        let ranges = crate::icf::frequency::Ranges::generate(&q, None).unwrap();
        let g = vec![0; 0x4997a];
        for (text, pending, stored, accepted, frequency) in [
            (&b"146.52"[..], false, true, true, 146_520_000),
            (b"4.0000005", false, true, true, 4_000_000),
            (b"1e-3", false, true, true, 1_000_000),
            (b"", true, true, false, 30_000),
            (b"foo", false, false, false, 0),
        ] {
            let mut m = model();
            let mut h = holder(&m, 10).unwrap();
            let mut state = EditorState::initialized();
            if pending {
                m.native_pending_name(&h, b"KEEP", &mut state.pending)
                    .unwrap();
            }
            let mut effects = Effects::default();
            let mut saves = 0;
            let out = m
                .native_frequency_input(
                    &mut h,
                    &mut state,
                    FrequencyRequest {
                        input: NumberInput::Text(text),
                        point: b'.',
                        context: EditorContext {
                            ranges: &ranges,
                            generation: ReferenceImage::Separate(&g),
                        },
                        live_q: &q,
                        warning_bits: 0x81,
                        focus: 0,
                    },
                    &mut effects,
                    |_, w| {
                        if w.stage() == ChannelStage::Frequency {
                            saves += 1;
                        }
                        Ok(())
                    },
                )
                .unwrap();
            assert_eq!((out.stored, out.first.accepted), (stored, accepted));
            assert_eq!(h.channel_field(ChannelField::FrequencyHz), frequency);
            assert_eq!(out.warning_bits & 0x81, 0x81);
            assert_eq!(saves, if stored { 3 } else { 0 });
            assert_eq!(out.notify_flags, if stored { 0x18 } else { 0 });
            assert_eq!(
                out.second.map(|s| s.accepted),
                if stored { Some(accepted) } else { None }
            );
            if stored {
                assert_eq!(
                    holder(&m, 11)
                        .unwrap()
                        .channel_field(ChannelField::FrequencyHz),
                    frequency
                );
                assert_eq!(effects.notifications.last().unwrap().id, 0x3c);
            }
            if pending {
                assert_eq!(h.name(), b"KEEP            ");
                assert!(!state.pending.active);
            }
        }
        for fail in [0, 2, 3] {
            let mut m = model();
            let mut h = holder(&m, 10).unwrap();
            let mut state = EditorState::initialized();
            m.native_pending_name(&h, b"KEEP", &mut state.pending)
                .unwrap();
            let mut effects = Effects::default();
            let mut saves = 0;
            let out = m.native_frequency_input(
                &mut h,
                &mut state,
                FrequencyRequest {
                    input: NumberInput::Text(b"146.52"),
                    point: b'.',
                    context: EditorContext {
                        ranges: &ranges,
                        generation: ReferenceImage::Separate(if fail == 0 { &[] } else { &g }),
                    },
                    live_q: &q,
                    warning_bits: 0,
                    focus: 0,
                },
                &mut effects,
                |_, w| {
                    if w.stage() == ChannelStage::Frequency {
                        saves += 1;
                        if saves == fail {
                            return Err(invalid("stop real frequency saver"));
                        }
                    }
                    Ok(())
                },
            );
            assert!(out.is_err());
            assert_eq!(m.used_slots(), 1);
            assert!(effects.notifications.is_empty());
            if fail == 0 {
                assert!(!h.valid());
                assert!(state.pending.active);
            } else {
                assert!(holder(&m, 10).unwrap().valid());
                assert!(!state.pending.active);
                assert_eq!(holder(&m, 10).unwrap().name(), b"KEEP            ");
            }
        }
    }

    #[test]
    fn frequency_input_last_slot_capacity_and_false_focus_do_not_double_allocate() {
        use crate::icf::settings::extended::NumberInput;
        let q = crate::icf::context::Context::from_bytes([0; 24]);
        let ranges = crate::icf::frequency::Ranges::generate(&q, None).unwrap();
        let g = vec![0; 0x4997a];
        for full in [false, true] {
            let mut m = model();
            m.set_word(super::super::GROUP_COUNT, 6);
            for group in 0..4 {
                m.allocate(group, 0..100).unwrap();
            }
            m.allocate(4, 0..if full { 100 } else { 99 }).unwrap();
            let mut h = holder(&m, 1000).unwrap();
            let mut state = EditorState::initialized();
            state.pending = PendingName {
                active: true,
                index: 1000,
                bytes: b"KEEP".to_vec(),
            };
            let mut effects = Effects::default();
            let out = m
                .native_frequency_input(
                    &mut h,
                    &mut state,
                    FrequencyRequest {
                        input: NumberInput::Text(b"146.52"),
                        point: b'.',
                        context: EditorContext {
                            ranges: &ranges,
                            generation: ReferenceImage::Separate(&g),
                        },
                        live_q: &q,
                        warning_bits: 0,
                        focus: 0,
                    },
                    &mut effects,
                    |_, _| Ok(()),
                )
                .unwrap();
            assert_eq!(out.stored, !full);
            assert_eq!(m.used_slots(), 500);
            assert_eq!(out.notify_flags, 0x18);
            assert!(!state.pending.active);
        }
        let mut m = split_model(0);
        let mut h = holder(&m, 4).unwrap();
        let mut state = EditorState::initialized();
        state.pending = PendingName {
            active: true,
            index: 10,
            bytes: b"KEEP".to_vec(),
        };
        let out = m
            .native_frequency_input(
                &mut h,
                &mut state,
                FrequencyRequest {
                    input: NumberInput::Begin,
                    point: b'.',
                    context: EditorContext {
                        ranges: &ranges,
                        generation: ReferenceImage::Separate(&[]),
                    },
                    live_q: &q,
                    warning_bits: 0xa5,
                    focus: 0,
                },
                &mut Effects::default(),
                |_, _| panic!("inline request cannot save"),
            )
            .unwrap();
        assert!(!out.stored);
        assert_eq!(out.first.state_out, 2);
        assert_eq!(out.focus, 1);
        assert_eq!(out.warning_bits & 0xa5, 0xa5);
        assert_eq!(out.notify_flags, 0);
    }
    #[test]
    fn code_dedicated_c_prefix_wrap_display_and_false_node_effects() {
        use crate::icf::settings::{Selection as InputSelection, extended::NumberInput};
        for (text, expected) in [
            (&b"9"[..], Some(9)),
            (b"09", Some(9)),
            (b"99", Some(99)),
            (b"12\0TAIL", Some(12)),
            (b"", None),
            (b" 1", None),
            (b"1 ", None),
            (b"001", None),
            (b"-1", None),
            (b"1.0", None),
        ] {
            let mut h = record::reset_holder(2, 4, 0);
            h.set_mode(16);
            h.set_valid(true);
            h.set_channel_field(ChannelField::DigitalCode, 355);
            let mut flags = 0xad;
            let out =
                channel_code_input(&mut h, NumberInput::Text(text), None, &mut flags).unwrap();
            assert_eq!(out.candidate, expected);
            assert_eq!(out.stored, expected.is_some());
            assert_eq!(flags, 0xae);
            assert_eq!(
                h.channel_field(ChannelField::DigitalCode),
                expected.unwrap_or(355)
            );
            assert_eq!(
                out.display,
                format!("{:02}", expected.unwrap_or(355)).as_bytes()
            );
        }
        for (old, input, expected) in [
            (99, NumberInput::Next, 0),
            (0, NumberInput::Previous, 99),
            (255, NumberInput::Previous, 0),
            (u32::MAX, NumberInput::Next, 0),
            (0x7fff_ffff, NumberInput::Next, 99),
            (0x8000_0000, NumberInput::Previous, 0),
        ] {
            let mut h = record::reset_holder(3, 0, 0);
            h.set_mode(16);
            h.set_valid(true);
            h.set_channel_field(ChannelField::DigitalCode, old);
            let mut flags = 0;
            let out = channel_code_input(&mut h, input, None, &mut flags).unwrap();
            assert_eq!(out.candidate, Some(expected));
            assert_eq!(out.display, format!("{:02}", old as i32).as_bytes());
        }
        for (input, first, display, selection, state) in [
            (NumberInput::Begin, None, &b"07"[..], (0, 2), 2),
            (NumberInput::Begin, Some(b'3'), &b"3"[..], (1, 0), 2),
            (
                NumberInput::Selected(InputSelection {
                    key: 1,
                    phase: 2,
                    candidate: 99,
                    accepted: true,
                }),
                None,
                &b"07"[..],
                (0, 2),
                0,
            ),
            (NumberInput::Cancel, None, &b"07"[..], (0, 2), 0),
        ] {
            let mut h = record::reset_holder(2, 4, 0);
            h.set_mode(16);
            h.set_valid(true);
            h.set_channel_field(ChannelField::DigitalCode, 7);
            let mut flags = 0xa5;
            let out = channel_code_input(&mut h, input, first, &mut flags).unwrap();
            assert!(!out.stored);
            assert_eq!(out.display, display);
            assert_eq!(out.selection, selection);
            assert_eq!(out.state_out, state);
            assert_eq!(flags, 0xa6);
            assert_eq!(h.channel_field(ChannelField::DigitalCode), 7);
        }
        let mut m = split_model(0);
        let mut h = holder(&m, 4).unwrap();
        h.set_mode(16);
        m.store_channel(4, &mut h).unwrap();
        let other = m.record(0, 2).unwrap().unwrap()[34..67].to_vec();
        let mut state = EditorState::initialized();
        let mut effects = Effects::default();
        let ranges = crate::icf::frequency::Ranges::unpopulated();
        let mut flags = 0xa5;
        let mut saves = 0;
        let out = m
            .native_code_input(
                &mut h,
                &mut state,
                CodeRequest {
                    input: NumberInput::Text(b"09"),
                    first: None,
                    node_flags: &mut flags,
                    context: EditorContext {
                        ranges: &ranges,
                        generation: ReferenceImage::Separate(&[]),
                    },
                },
                &mut effects,
                |_, w| {
                    if w.stage() == ChannelStage::Frequency {
                        saves += 1;
                    }
                    Ok(())
                },
            )
            .unwrap();
        assert!(out.stored);
        assert_eq!(saves, 2);
        assert_eq!(
            holder(&m, 4)
                .unwrap()
                .channel_field(ChannelField::DigitalCode),
            9
        );
        assert_eq!(&m.record(0, 2).unwrap().unwrap()[34..67], other);
    }

    #[test]
    fn name_uses_shared_byte_sanitation_pending_and_real_two_savers() {
        use crate::icf::settings::extended::TextInput;
        let ranges = crate::icf::frequency::Ranges::unpopulated();
        for (source, expected) in [
            (&b"A\0TAIL  "[..], &b"A TAIL"[..]),
            (&b"A\x82\0TAIL"[..], &b"A  TAIL"[..]),
            (&b"123456789012345\x82\xa0X"[..], &b"123456789012345"[..]),
            (&b" \x82\xa0X\x81\x40 "[..], &b" \x82\xa0X"[..]),
            (b"", b""),
        ] {
            let mut m = split_model(0);
            let mut h = holder(&m, 4).unwrap();
            let mut state = EditorState::initialized();
            let mut text = NameEffect::default();
            let mut effects = Effects::default();
            let mut frame = ChoiceFrame {
                layout: 0,
                node_flags: 0xa5,
            };
            let mut saves = 0;
            let other = m.record(0, 2).unwrap().unwrap()[34..67].to_vec();
            m.native_name_input(
                &mut h,
                &mut state,
                NameRequest {
                    input: TextInput::Text(source),
                    context: EditorContext {
                        ranges: &ranges,
                        generation: ReferenceImage::Separate(&[]),
                    },
                    frame: &mut frame,
                    notification: None,
                },
                &mut text,
                &mut effects,
                |_, w| {
                    if w.stage() == ChannelStage::Frequency {
                        saves += 1;
                    }
                    Ok(())
                },
            )
            .unwrap();
            assert!(text.stored);
            assert_eq!(saves, 2);
            assert_eq!(text.display, expected);
            assert_eq!(h.name().trim_ascii_end(), expected);
            assert_eq!(&m.record(0, 2).unwrap().unwrap()[34..67], other);
            assert_eq!(frame.node_flags, 0xa4);
        }
        let mut m = split_model(0);
        let mut h = holder(&m, 10).unwrap();
        let mut state = EditorState::initialized();
        let mut text = NameEffect::default();
        let mut effects = Effects::default();
        let mut frame = ChoiceFrame {
            layout: 1,
            node_flags: 0xa4,
        };
        let notice = std::array::from_fn(|i| i as u8);
        let before = m.working().to_vec();
        m.native_name_input(
            &mut h,
            &mut state,
            NameRequest {
                input: TextInput::Text(b"P\0ENDING  "),
                context: EditorContext {
                    ranges: &ranges,
                    generation: ReferenceImage::Separate(&[]),
                },
                frame: &mut frame,
                notification: Some(notice),
            },
            &mut text,
            &mut effects,
            |_, _| panic!("pending cannot save"),
        )
        .unwrap();
        assert!(!text.stored);
        assert_eq!(text.pending_requested, Some(notice));
        assert_eq!(state.pending.bytes, b"P ENDING");
        assert_eq!(state.pending.index, 10);
        assert!(state.pending.active);
        assert_eq!(m.working(), before);
        let mut odd = holder(&m, 11).unwrap();
        let mut text = NameEffect::default();
        m.native_name_input(
            &mut odd,
            &mut state,
            NameRequest {
                input: TextInput::Cancel,
                context: EditorContext {
                    ranges: &ranges,
                    generation: ReferenceImage::Separate(&[]),
                },
                frame: &mut frame,
                notification: None,
            },
            &mut text,
            &mut effects,
            |_, _| panic!("cancel cannot save"),
        )
        .unwrap();
        assert_eq!(text.display, b"P ENDING");
        assert_eq!(frame.node_flags, 0xa5);
        assert_eq!(state.pending.bytes, b"P ENDING");
    }
    #[test]
    fn native_choices_preserve_full_candidate_until_two_saver_readback() {
        use crate::icf::settings::{Selection as InputSelection, extended::ChoiceInput};
        let ranges = crate::icf::frequency::Ranges::unpopulated();
        for (field, mode, candidate, expected) in [
            (ChannelChoice::Selection, 13, 0x102, 2),
            (ChannelChoice::Duplex, 17, 0x104, 0),
            (ChannelChoice::Duplex, 17, 4, 2),
            (ChannelChoice::Polarity, 13, 0x103, 2),
            (ChannelChoice::Polarity, 13, 5, 5),
            (ChannelChoice::DigitalSquelch, 16, 0x102, 2),
        ] {
            let mut m = split_model(0);
            let mut h = holder(&m, 4).unwrap();
            h.set_mode(mode);
            m.store_channel(4, &mut h).unwrap();
            // Mode 17 is a conditional holder input, not a raw MODE-table entry.
            h.set_mode(mode);
            let before = m.record(0, 2).unwrap().unwrap().to_vec();
            let used = m.used_slots();
            let mut state = editor_state(4);
            let mut effects = Effects::default();
            let mut frame = ChoiceFrame {
                layout: 0,
                node_flags: 0xa5,
            };
            let mut saves = 0;
            let out = m
                .native_choice_input(
                    &mut h,
                    &mut state,
                    ChoiceRequest {
                        field,
                        input: ChoiceInput::Selected(InputSelection {
                            key: 1,
                            phase: 2,
                            candidate,
                            accepted: true,
                        }),
                        context: EditorContext {
                            ranges: &ranges,
                            generation: ReferenceImage::Separate(&[]),
                        },
                        selection: if field == ChannelChoice::Selection {
                            Some(&mut frame)
                        } else {
                            None
                        },
                    },
                    &mut effects,
                    |_, w| {
                        if w.stage() == ChannelStage::Frequency {
                            saves += 1;
                        }
                        Ok(())
                    },
                )
                .unwrap();
            assert_eq!(out.candidate, Some(candidate));
            assert!(out.stored);
            assert_eq!(saves, 2);
            assert_eq!(h.channel_field(field.field()), expected);
            assert_eq!(m.used_slots(), used);
            assert_eq!(&m.record(0, 2).unwrap().unwrap()[34..67], &before[34..67]);
            assert!(!state.mode_cache.valid());
            assert!(!state.filter_cache.valid());
            assert_eq!(
                frame.node_flags,
                if field == ChannelChoice::Selection {
                    0xa4
                } else {
                    0xa5
                }
            );
        }
    }
    #[test]
    fn native_sel_false_tail_and_late_writer_failure_are_not_rollback() {
        use crate::icf::settings::{Selection as InputSelection, extended::ChoiceInput};
        let ranges = crate::icf::frequency::Ranges::unpopulated();
        for split in [0, 1] {
            for layout in [0, 3] {
                for input in [
                    ChoiceInput::Display,
                    ChoiceInput::Cancel,
                    ChoiceInput::Begin,
                    ChoiceInput::Selected(InputSelection {
                        key: 2,
                        phase: 2,
                        candidate: 3,
                        accepted: true,
                    }),
                    ChoiceInput::Selected(InputSelection {
                        key: 1,
                        phase: 1,
                        candidate: 3,
                        accepted: true,
                    }),
                    ChoiceInput::Selected(InputSelection {
                        key: 1,
                        phase: 2,
                        candidate: 3,
                        accepted: false,
                    }),
                ] {
                    let mut m = split_model(split);
                    let before = m.working().to_vec();
                    let mut h = holder(&m, 4).unwrap();
                    let mut state = editor_state(4);
                    let saved = state.mode_cache.clone();
                    let mut effects = Effects::default();
                    let mut frame = ChoiceFrame {
                        layout,
                        node_flags: 0xa4,
                    };
                    let out = m
                        .native_choice_input(
                            &mut h,
                            &mut state,
                            ChoiceRequest {
                                field: ChannelChoice::Selection,
                                input,
                                context: EditorContext {
                                    ranges: &ranges,
                                    generation: ReferenceImage::Separate(&[]),
                                },
                                selection: Some(&mut frame),
                            },
                            &mut effects,
                            |_, _| panic!("false choice cannot save"),
                        )
                        .unwrap();
                    assert!(!out.stored);
                    assert_eq!(m.working(), before);
                    assert_eq!(state.mode_cache.as_bytes(), saved.as_bytes());
                    assert_eq!(frame.node_flags, 0xa4 | u8::from(split == 1 || layout != 0));
                    assert_eq!(out.popup_requested, input == ChoiceInput::Begin);
                }
            }
        }
        let mut m = split_model(0);
        let mut h = holder(&m, 4).unwrap();
        let mut state = editor_state(4);
        let mut effects = Effects::default();
        let mut frame = ChoiceFrame {
            layout: 0,
            node_flags: 0xa5,
        };
        let mut saves = 0;
        let result = m.native_choice_input(
            &mut h,
            &mut state,
            ChoiceRequest {
                field: ChannelChoice::Selection,
                input: ChoiceInput::Selected(InputSelection {
                    key: 1,
                    phase: 2,
                    candidate: 3,
                    accepted: true,
                }),
                context: EditorContext {
                    ranges: &ranges,
                    generation: ReferenceImage::Separate(&[]),
                },
                selection: Some(&mut frame),
            },
            &mut effects,
            |_, w| {
                if w.stage() == ChannelStage::Frequency {
                    saves += 1;
                    if saves == 2 {
                        return Err(invalid("stop second saver"));
                    }
                }
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(
            holder(&m, 4)
                .unwrap()
                .channel_field(ChannelField::Selection),
            3
        );
        assert_eq!(frame.node_flags, 0xa5);
        assert!(!state.mode_cache.valid());
    }
    fn split_model(split: u8) -> NormalMemory {
        let mut m = model();
        m.allocate(0, 2..3).unwrap();
        let mut h = record::reset_holder(2, 4, 0);
        h.set_mode(13);
        h.set_filter(1);
        h.set_split(split);
        h.set_channel_field(ChannelField::FrequencyHz, 145_000_001);
        m.store_channel(4, &mut h).unwrap();
        h.set_channel_field(ChannelField::FrequencyHz, 146_000_002);
        m.store_channel(5, &mut h).unwrap();
        m
    }
    fn call_model() -> Vec<u8> {
        let mut w = model().working().to_vec();
        w.truncate(0x3c9c0);
        w[0x3c84b] = 100;
        w[0x3c84c..0x3c84e].copy_from_slice(&0u16.to_le_bytes());
        w[0x3c84a] = 0;
        w[0x3a8c9] = 0;
        w[0x3a8ca] = 0;
        for index in 0..8 {
            let mut h = record::reset_holder(3, index, 91);
            h.set_mode(13);
            h.set_filter(1);
            h.set_name(b"CALL NATIVE     ");
            h.set_channel_field(
                ChannelField::FrequencyHz,
                if index < 4 { 145_000_000 } else { 433_000_000 },
            );
            h.set_offset_hz(600_000);
            record_image::store_channel_into(&mut w, ChannelKind::Call, index, &mut h, |_| Ok(()))
                .unwrap();
        }
        w
    }
    fn call_holder(w: &[u8], index: u32) -> DecodedRecord {
        let mut h = record::reset_holder(3, index, 91);
        record_image::load_channel_into(w, ChannelKind::Call, index, &mut h).unwrap();
        h
    }
    #[test]
    fn call_post_save_uses_live_type1a_and_never_clamps_selection_bytes_or_signed_row() {
        let q = crate::icf::context::Context::from_bytes([0; 24]);
        let ranges = crate::icf::frequency::Ranges::generate(&q, None).unwrap();
        for (frequency, row, selected, expected) in [
            (145_000_000, 0, 0, true),
            (145_000_000, 1, 1, true),
            (145_000_000, 2, 0, false),
            (433_000_000, 2, 0, true),
            (433_000_000, 3, 1, true),
            (433_000_000, 1, 1, false),
            (145_000_000, 0, 2, false),
            (433_000_000, 3, 255, false),
            (145_000_000, -2, 0, true),
        ] {
            let mut w = call_model();
            w[0x3c84a] = 1;
            w[0x3a780] = selected;
            w[0x3a781] = selected;
            let mut dr = record::reset_holder(0x1a, 0, 0);
            dr.set_valid(true);
            dr.set_mode(13);
            dr.set_channel_field(ChannelField::FrequencyHz, frequency);
            record_image::store_paired_into(&mut w, 0x1a, 0, &mut dr, |_, _| Ok(())).unwrap();
            let before = w.clone();
            let mut effects = Effects {
                home_invalidated: 0xa5,
                dr_invalidated: 0x5a,
                ..Effects::default()
            };
            call_after_into(&w, row, &ranges, &mut effects).unwrap();
            assert_eq!(effects.dr_invalidated, if expected { 1 } else { 0x5a });
            assert_eq!(effects.home_invalidated, if row == 0 { 1 } else { 0xa5 });
            assert_eq!(w, before);
        }
    }
    #[test]
    fn call_consumers_share_field_engine_two_savers_and_do_not_require_normal_maps() {
        use crate::icf::settings::extended::{ChoiceInput, NumberInput, TextInput};
        let q = crate::icf::context::Context::from_bytes([0; 24]);
        let ranges = crate::icf::frequency::Ranges::generate(&q, None).unwrap();
        for family in 0..7 {
            let mut w = call_model();
            let mut h = call_holder(&w, 0);
            if family == 5 {
                h.set_mode(16);
                record_image::store_channel_into(&mut w, ChannelKind::Call, 0, &mut h, |_| Ok(()))
                    .unwrap();
            }
            assert_eq!((h.group(), h.range_index(), h.side()), (100, 12, 0));
            let other = w[0xa21c + 34..0xa21c + 67].to_vec();
            let mut state = EditorState::initialized();
            let mut effects = Effects::default();
            let context = EditorContext {
                ranges: &ranges,
                generation: ReferenceImage::Separate(&[]),
            };
            let mut saves = 0;
            let mut before = |_: u32, event: &ChannelWrite<'_>| {
                if event.stage() == ChannelStage::Frequency {
                    saves += 1;
                }
                Ok(())
            };
            match family {
                0 => {
                    let out = call_choice_input_into(
                        &mut w,
                        &mut h,
                        &mut state,
                        ChoiceRequest {
                            field: ChannelChoice::Duplex,
                            input: ChoiceInput::Next,
                            context,
                            selection: None,
                        },
                        &mut effects,
                        &mut before,
                    )
                    .unwrap();
                    assert!(out.stored);
                    assert_eq!(h.channel_field(ChannelField::Duplex), 1);
                }
                1 => {
                    let out = call_enum_input_into(
                        &mut w,
                        &mut h,
                        &mut state,
                        EnumRequest {
                            field: ChannelEnum::Mode,
                            input: ChoiceInput::Next,
                            context,
                            warning_bits: Some(0x81),
                        },
                        &mut effects,
                        &mut before,
                    )
                    .unwrap();
                    assert!(out.choice.stored);
                    assert_eq!(h.mode(), 16);
                    assert_eq!(out.warning_bits, Some(0x81));
                }
                2 => {
                    let out = call_offset_input_into(
                        &mut w,
                        &mut h,
                        &mut state,
                        OffsetRequest {
                            input: NumberInput::Text(b"0.00015"),
                            point: b'.',
                            context,
                        },
                        &mut effects,
                        &mut before,
                    )
                    .unwrap();
                    assert!(out.accepted);
                    assert_eq!(h.offset_hz(), 200);
                }
                3 => {
                    let mut effect = ToneEffect::default();
                    let mut upper = |s: &mut [u8]| {
                        s.make_ascii_uppercase();
                        Ok(())
                    };
                    call_tone_input_into(
                        &mut w,
                        &mut h,
                        &mut state,
                        ToneRequest {
                            input: ToneInput {
                                field: ToneField::Tx,
                                input: NumberInput::Text(b"88.5"),
                                point: b'.',
                                live_q: Some(&q),
                                inline: false,
                                first: None,
                                upper: &mut upper,
                            },
                            context,
                        },
                        &mut effect,
                        &mut effects,
                        &mut before,
                    )
                    .unwrap();
                    assert!(effect.stored);
                    assert_eq!(h.tx_tone(), 8);
                }
                4 => {
                    let mut effect = NameEffect::default();
                    let mut frame = ChoiceFrame {
                        layout: 0,
                        node_flags: 0xa5,
                    };
                    call_name_input_into(
                        &mut w,
                        &mut h,
                        &mut state,
                        NameRequest {
                            input: TextInput::Text(b"A\0CALL "),
                            context,
                            frame: &mut frame,
                            notification: None,
                        },
                        &mut effect,
                        &mut effects,
                        &mut before,
                    )
                    .unwrap();
                    assert!(effect.stored);
                    assert_eq!(h.name(), b"A CALL          ");
                    assert_eq!(frame.node_flags, 0xa4);
                }
                5 => {
                    let mut flags = 0xa5;
                    let out = call_code_input_into(
                        &mut w,
                        &mut h,
                        &mut state,
                        CodeRequest {
                            input: NumberInput::Text(b"09"),
                            first: None,
                            node_flags: &mut flags,
                            context,
                        },
                        &mut effects,
                        &mut before,
                    )
                    .unwrap();
                    assert!(out.stored);
                    assert_eq!(h.channel_field(ChannelField::DigitalCode), 9);
                    assert_eq!(flags, 0xa6);
                }
                _ => {
                    let out = call_frequency_input_into(
                        &mut w,
                        &mut h,
                        &mut state,
                        FrequencyRequest {
                            input: NumberInput::Text(b"150"),
                            point: b'.',
                            context,
                            live_q: &q,
                            warning_bits: 0x81,
                            focus: 0,
                        },
                        &mut effects,
                        &mut before,
                    )
                    .unwrap();
                    assert!(out.stored);
                    assert_eq!(out.first.candidate, 150_000_000);
                    assert_eq!(out.second.unwrap().candidate, 148_000_000);
                    assert_eq!(h.channel_field(ChannelField::FrequencyHz), 148_000_000);
                    assert_eq!(out.notify_flags, 0x18);
                }
            }
            assert_eq!(saves, 2);
            assert_eq!(&w[0xa21c + 34..0xa21c + 67], other);
            assert_eq!(effects.home_invalidated, 1);
        }
        let mut w = call_model();
        let mut h = call_holder(&w, 6);
        let mut state = EditorState::initialized();
        let out = call_frequency_input_into(
            &mut w,
            &mut h,
            &mut state,
            FrequencyRequest {
                input: NumberInput::Text(b"400"),
                point: b'.',
                context: EditorContext {
                    ranges: &ranges,
                    generation: ReferenceImage::Separate(&[]),
                },
                live_q: &q,
                warning_bits: 0,
                focus: 0,
            },
            &mut Effects::default(),
            |_, _| Ok(()),
        )
        .unwrap();
        assert_eq!(h.range_index(), 13);
        assert_eq!(out.first.candidate, 400_000_000);
        assert_eq!(out.second.unwrap().candidate, 420_000_000);
    }
    #[test]
    fn callsign_direct_text_uses_logical_slots_full_upper_and_two_real_savers() {
        use crate::icf::{MemoryCallsign, settings::extended::TextInput};
        let q = crate::icf::context::Context::from_bytes([0; 24]);
        let ranges = crate::icf::frequency::Ranges::generate(&q, None).unwrap();
        for field in [
            MemoryCallsign::Your,
            MemoryCallsign::Repeater1,
            MemoryCallsign::Repeater2,
        ] {
            let mut w = call_model();
            let mut h = call_holder(&w, 0);
            h.set_mode(16);
            record_image::store_channel_into(&mut w, ChannelKind::Call, 0, &mut h, |_| Ok(()))
                .unwrap();
            let other = w[0xa21c + 34..0xa21c + 67].to_vec();
            let mut state = EditorState::initialized();
            let mut effects = Effects::default();
            let mut saves = 0;
            let mut inputs = Vec::new();
            let mut upper = |v| {
                inputs.push(v);
                Ok(match v {
                    -128 => 65,
                    -127 => 0x141,
                    other => other,
                })
            };
            assert!(
                channel_callsign_text_input_into(
                    &mut w,
                    &mut h,
                    &mut state,
                    CallsignTextRequest {
                        field,
                        input: TextInput::Text(b"\x80\x81\xffa\0b"),
                        right_word: false,
                        upper: &mut upper,
                        context: EditorContext {
                            ranges: &ranges,
                            generation: ReferenceImage::Separate(&[])
                        }
                    },
                    &mut effects,
                    |_, event| {
                        if event.stage() == ChannelStage::Frequency {
                            saves += 1;
                        }
                        Ok(())
                    }
                )
                .unwrap()
            );
            assert_eq!(inputs, [-128, -127, -1, 97, 0, 98]);
            assert_eq!(h.callsign(field), b"A  A B  ");
            assert_eq!(saves, 2);
            assert_eq!(&w[0xa21c + 34..0xa21c + 67], other);
            assert_eq!(effects.notifications.last().unwrap().id, 0x3c);
        }
        let mut w = call_model();
        let mut h = call_holder(&w, 0);
        h.set_mode(16);
        h.set_callsign(MemoryCallsign::Your, b"OLDTAIL ");
        let before = w.clone();
        let mut state = EditorState::initialized();
        let mut effects = Effects::default();
        let mut upper = |v| {
            if v < 0 {
                Err(invalid("native upper failed"))
            } else {
                Ok(v)
            }
        };
        assert!(
            channel_callsign_text_input_into(
                &mut w,
                &mut h,
                &mut state,
                CallsignTextRequest {
                    field: MemoryCallsign::Your,
                    input: TextInput::Text(b"a\x80"),
                    right_word: false,
                    upper: &mut upper,
                    context: EditorContext {
                        ranges: &ranges,
                        generation: ReferenceImage::Separate(&[])
                    }
                },
                &mut effects,
                |_, _| panic!("upper failure precedes every saver")
            )
            .is_err()
        );
        assert_eq!(h.callsign(MemoryCallsign::Your), b"ALDTAIL ");
        assert_eq!(w, before);
        assert!(effects.notifications.is_empty());
    }

    #[test]
    fn call_split_shares_full_candidate_two_savers_and_pre_step_tail() {
        use crate::icf::settings::extended::ChoiceInput;
        let q = crate::icf::context::Context::from_bytes([0; 24]);
        let ranges = crate::icf::frequency::Ranges::generate(&q, None).unwrap();
        for (candidate, raw, notices) in [
            (Some(1), 1, vec![0x45, 0x3c]),
            (Some(256), 0, vec![0x45, 0x3c]),
            (Some(0), 0, vec![0x3c]),
            (None, 0, vec![]),
        ] {
            let input = candidate.map_or(ChoiceInput::Cancel, |candidate| {
                ChoiceInput::Selected(crate::icf::settings::Selection {
                    key: 1,
                    phase: 2,
                    candidate,
                    accepted: true,
                })
            });
            let mut w = call_model();
            let mut h = call_holder(&w, 0);
            let other = w[0xa21c + 34..0xa21c + 67].to_vec();
            let mut state = EditorState::initialized();
            let mut effects = Effects::default();
            let mut frame = split_frame();
            frame.pre_step = 1;
            let mut saves = 0;
            let out = channel_split_input_into(
                &mut w,
                &mut h,
                &mut state,
                SplitRequest {
                    input,
                    context: EditorContext {
                        ranges: &ranges,
                        generation: ReferenceImage::Separate(&[]),
                    },
                    frame: &mut frame,
                },
                &mut effects,
                |_, event| {
                    if event.stage() == ChannelStage::Frequency {
                        saves += 1;
                    }
                    Ok(())
                },
                |_, _| Ok(()),
            )
            .unwrap();
            assert_eq!(out.stored, !notices.is_empty());
            assert_eq!(saves, if out.stored { 2 } else { 0 });
            assert_eq!(h.split(), raw);
            assert_eq!(frame.node_flags, 0xa4);
            assert_eq!(
                effects
                    .notifications
                    .iter()
                    .map(|n| n.id)
                    .collect::<Vec<_>>(),
                notices
            );
            assert_eq!(&w[0xa21c + 34..0xa21c + 67], other);
        }
    }

    #[test]
    fn call_sel_rejection_and_late_g_failure_keep_real_cache_and_dr_prefixes() {
        use crate::icf::settings::{Selection as InputSelection, extended::ChoiceInput};
        let q = crate::icf::context::Context::from_bytes([0; 24]);
        let ranges = crate::icf::frequency::Ranges::generate(&q, None).unwrap();
        let mut w = call_model();
        let mut h = call_holder(&w, 0);
        let original = w.clone();
        let mut state = EditorState::initialized();
        let mut effects = Effects::default();
        let context = EditorContext {
            ranges: &ranges,
            generation: ReferenceImage::Separate(&[]),
        };
        assert!(
            call_choice_input_into(
                &mut w,
                &mut h,
                &mut state,
                ChoiceRequest {
                    field: ChannelChoice::Selection,
                    input: ChoiceInput::Next,
                    context,
                    selection: None
                },
                &mut effects,
                |_, _| panic!("call SEL cannot store")
            )
            .is_err()
        );
        assert_eq!(w, original);
        w[0x3c84a] = 1;
        w[0x3a780] = 0;
        let mut dr = record::reset_holder(0x1a, 0, 0);
        dr.set_valid(true);
        dr.set_mode(13);
        dr.set_channel_field(ChannelField::FrequencyHz, 145_000_000);
        record_image::store_paired_into(&mut w, 0x1a, 0, &mut dr, |_, _| Ok(())).unwrap();
        state.filter_enabled = 1;
        state.filter_cache.set_header(3, 0, 0);
        state.filter_cache.set_valid(false);
        let mut saves = 0;
        let out = call_enum_input_into(
            &mut w,
            &mut h,
            &mut state,
            EnumRequest {
                field: ChannelEnum::Mode,
                input: ChoiceInput::Selected(InputSelection {
                    key: 7,
                    phase: 2,
                    candidate: 0,
                    accepted: true,
                }),
                context,
                warning_bits: Some(0),
            },
            &mut effects,
            |_, event| {
                if event.stage() == ChannelStage::Frequency {
                    saves += 1;
                }
                Ok(())
            },
        );
        assert!(out.is_err());
        assert_eq!(saves, 1);
        assert_eq!(call_holder(&w, 0).mode(), 0);
        assert_eq!((effects.home_invalidated, effects.dr_invalidated), (1, 1));
        assert!(effects.notifications.is_empty());
        assert!(state.filter_cache.valid());
    }
    #[test]
    fn split_full_candidate_keeps_other_side_and_ordered_notice_requests() {
        use crate::icf::settings::{Selection as InputSelection, extended::ChoiceInput};
        let ranges = crate::icf::frequency::Ranges::unpopulated();
        for (old, candidate, layout, ids, active, mode) in [
            (0, 0, 0, vec![0x3c], 62, 7),
            (0, 1, 0, vec![0x45, 0x3c], 47, 2),
            (1, 0, 0, vec![0x46, 0x3c], 12, 2),
            (0, 0x100, 0, vec![0x45, 0x3c], 47, 2),
            (0, 1, 2, vec![0x3c], 62, 7),
        ] {
            let mut m = split_model(old);
            let before = m.record(0, 2).unwrap().unwrap().to_vec();
            let mut h = holder(&m, 4).unwrap();
            let mut state = EditorState::initialized();
            let mut frame = split_frame();
            frame.layout = layout;
            let mut effects = Effects::default();
            let mut saves = 0;
            let out = m
                .native_split_input(
                    &mut h,
                    &mut state,
                    SplitRequest {
                        input: ChoiceInput::Selected(InputSelection {
                            key: 1,
                            phase: 2,
                            candidate,
                            accepted: true,
                        }),
                        context: EditorContext {
                            ranges: &ranges,
                            generation: ReferenceImage::Separate(&[]),
                        },
                        frame: &mut frame,
                    },
                    &mut effects,
                    |_, w| {
                        if w.stage() == ChannelStage::Frequency {
                            saves += 1;
                        }
                        Ok(())
                    },
                    |_, _| Ok(()),
                )
                .unwrap();
            assert!(out.stored);
            assert_eq!(out.candidate, Some(candidate));
            assert_eq!(saves, 2);
            assert_eq!(h.split(), candidate as u8 & 1);
            assert_eq!(m.used_slots(), 1);
            assert_eq!(&m.record(0, 2).unwrap().unwrap()[34..67], &before[34..67]);
            assert_eq!(
                effects
                    .notifications
                    .iter()
                    .map(|n| n.id)
                    .collect::<Vec<_>>(),
                ids
            );
            assert!(
                effects
                    .notifications
                    .iter()
                    .filter(|n| n.id != 0x3c)
                    .all(|n| n.synchronous)
            );
            let queue = effects.notifications.last().unwrap();
            assert!(!queue.synchronous);
            assert_eq!((queue.arg1, queue.arg2), (1, 0));
            assert_eq!(
                (
                    frame.active,
                    frame.display_mode,
                    frame.notify_flags,
                    frame.node_flags
                ),
                (active, mode, 0x21, 0xa4)
            );
            assert!(!state.mode_cache.valid());
            assert!(!state.filter_cache.valid());
        }
    }
    #[test]
    fn split_shared_editor_false_keyboard_and_live_tail_keep_distinct_state() {
        use crate::icf::settings::{Selection as InputSelection, extended::ChoiceInput};
        let ranges = crate::icf::frequency::Ranges::unpopulated();
        for input in [
            ChoiceInput::Display,
            ChoiceInput::Cancel,
            ChoiceInput::Begin,
            ChoiceInput::Selected(InputSelection {
                key: 2,
                phase: 2,
                candidate: 1,
                accepted: true,
            }),
            ChoiceInput::Selected(InputSelection {
                key: 1,
                phase: 1,
                candidate: 1,
                accepted: true,
            }),
            ChoiceInput::Selected(InputSelection {
                key: 1,
                phase: 2,
                candidate: 1,
                accepted: false,
            }),
        ] {
            let mut m = split_model(0);
            let image = m.working().to_vec();
            let mut h = holder(&m, 4).unwrap();
            let old = h.clone();
            let mut state = EditorState::initialized();
            let mut frame = split_frame();
            frame.pre_step = 2;
            frame.node_flags = 0xa4;
            let mut effects = Effects::default();
            let out = m
                .native_split_input(
                    &mut h,
                    &mut state,
                    SplitRequest {
                        input,
                        context: EditorContext {
                            ranges: &ranges,
                            generation: ReferenceImage::Separate(&[]),
                        },
                        frame: &mut frame,
                    },
                    &mut effects,
                    |_, _| panic!("false split must not save"),
                    |_, _| panic!("false split must not request notice"),
                )
                .unwrap();
            assert!(!out.stored);
            assert_eq!(out.popup_requested, input == ChoiceInput::Begin);
            assert_eq!(h.as_bytes(), old.as_bytes());
            assert_eq!(m.working(), image);
            assert!(effects.notifications.is_empty());
            assert_eq!(
                (
                    frame.node_flags,
                    frame.notify_flags,
                    frame.active,
                    frame.display_mode
                ),
                (0xa5, 1, 62, 7)
            );
        }
        for (old, input, expected) in [
            (0, ChoiceInput::Next, 1),
            (1, ChoiceInput::Previous, 0),
            (7, ChoiceInput::Next, 0),
            (7, ChoiceInput::Previous, 0),
        ] {
            let mut m = split_model(old & 1);
            let mut h = holder(&m, 4).unwrap();
            h.set_split(old);
            let mut frame = split_frame();
            let mut effects = Effects::default();
            m.native_split_input(
                &mut h,
                &mut EditorState::initialized(),
                SplitRequest {
                    input,
                    context: EditorContext {
                        ranges: &ranges,
                        generation: ReferenceImage::Separate(&[]),
                    },
                    frame: &mut frame,
                },
                &mut effects,
                |_, _| Ok(()),
                |n, f| {
                    if n.synchronous {
                        f.node_flags = 0xad;
                        f.node_index = 54;
                        f.pre_step = 9;
                    }
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(h.split(), expected);
            assert_eq!(frame.node_flags, 0xac);
            assert_eq!(frame.active, if expected == 0 { 12 } else { 54 });
        }
    }
    #[test]
    fn split_failure_keeps_saver_and_notice_prefix_without_running_tail() {
        use crate::icf::settings::extended::ChoiceInput;
        let ranges = crate::icf::frequency::Ranges::unpopulated();
        for fail in [0, 1, 2] {
            let mut m = split_model(0);
            let mut h = holder(&m, 4).unwrap();
            let mut frame = split_frame();
            let mut effects = Effects::default();
            let mut saves = 0;
            let result = m.native_split_input(
                &mut h,
                &mut EditorState::initialized(),
                SplitRequest {
                    input: ChoiceInput::Next,
                    context: EditorContext {
                        ranges: &ranges,
                        generation: ReferenceImage::Separate(&[]),
                    },
                    frame: &mut frame,
                },
                &mut effects,
                |_, w| {
                    if w.stage() == ChannelStage::Frequency {
                        saves += 1;
                        if fail == 0 && saves == 2 {
                            return Err(invalid("injected second split saver failure"));
                        }
                    }
                    Ok(())
                },
                |n, _| {
                    if (fail == 1 && n.synchronous) || (fail == 2 && !n.synchronous) {
                        Err(invalid("injected split notice request failure"))
                    } else {
                        Ok(())
                    }
                },
            );
            assert!(result.is_err());
            assert_eq!(holder(&m, 4).unwrap().split(), 1);
            assert_eq!(frame.node_flags, 0xa5);
            if fail < 2 {
                assert!(effects.notifications.is_empty());
                assert_eq!(
                    (frame.active, frame.display_mode, frame.notify_flags),
                    (62, 7, 1)
                );
            } else {
                assert_eq!(
                    effects
                        .notifications
                        .iter()
                        .map(|n| n.id)
                        .collect::<Vec<_>>(),
                    vec![0x45]
                );
                assert_eq!(
                    (frame.active, frame.display_mode, frame.notify_flags),
                    (47, 2, 0x21)
                );
            }
        }
    }
    #[test]
    fn paired_selection_count_matches_every_checked_side_range() {
        let state = EditorState::initialized();
        for h in [&state.mode_cache, &state.filter_cache] {
            assert_eq!((h.kind(), h.index(), h.binding()), (0, 0, 0));
            assert!(!h.valid());
            assert_eq!(h.group(), u32::MAX);
            assert_eq!(h.tx_tone(), 8);
            assert_eq!(h.callsign(crate::icf::MemoryCallsign::Your), b"CQCQCQ  ");
        }
        assert_eq!(state.filter_enabled, 0);
        assert!(!state.pending.active);
        assert_eq!(state.pending.index, u32::MAX);
        assert!(state.pending.bytes.is_empty());
        let m = model();
        for first in 0..200 {
            for last in first..200 {
                let s = selection(first, last).checked(&m).unwrap();
                let (a, b) = s.pair();
                assert_eq!((b - a).div_ceil(2), (first / 2..=last / 2).count());
            }
        }
    }
    #[test]
    fn paste_with_one_free_slot_keeps_allocated_prefix_and_stops_at_row99() {
        let mut m = model();
        m.set_word(super::super::GROUP_COUNT, 6);
        for group in 0..4 {
            m.allocate(group, 0..100).unwrap();
        }
        m.allocate(4, 0..99).unwrap();
        assert_eq!(m.used_slots(), 499);
        let mut h = record::reset_holder(4, 1, 71);
        h.set_mode(13);
        h.set_filter(1);
        h.set_channel_field(ChannelField::FrequencyHz, 146_987_654);
        let mut clip = Clipboard {
            source_kind: 4,
            mode: 0,
            declared_count: 2,
            logical_count: 2,
            records: vec![h.clone(), h],
        };
        let s = Selection {
            group: 5,
            ..selection(0, 1)
        };
        let mut effects = Effects::default();
        let out = m
            .native_paste(
                s,
                &mut clip,
                ReferenceImage::Working,
                &mut effects,
                |_| Ok(1),
                |_, _| Ok(()),
            )
            .unwrap();
        assert!(out.exhausted);
        assert_eq!(out.written, 2);
        assert_eq!(m.slot(5, 0).unwrap(), Some(499));
        assert_eq!(m.slot(5, 1).unwrap(), None);
        assert_eq!(
            holder(&m, 1001)
                .unwrap()
                .channel_field(ChannelField::FrequencyHz),
            146_987_654
        );
        m.release(4, 98..99).unwrap();
        let s = Selection {
            group: 5,
            ..selection(198, 199)
        };
        let out = m
            .native_paste(
                s,
                &mut clip,
                ReferenceImage::Working,
                &mut effects,
                |_| Ok(1),
                |_, _| Ok(()),
            )
            .unwrap();
        assert!(!out.exhausted);
        assert_eq!(out.written, 2);
        assert_eq!(m.slot(5, 99).unwrap(), Some(498));
        assert_eq!(m.group_used_slots(5).unwrap(), 2);
    }
    #[test]
    fn cached_mode_filter_two_savers_and_late_g_failure_keep_real_prefixes() {
        let q = crate::icf::context::Context::from_bytes([0; 24]);
        let ranges = crate::icf::frequency::Ranges::generate(&q, None).unwrap();
        let mut m = model();
        m.allocate(0, 2..3).unwrap();
        let mut h = record::reset_holder(2, 4, 19);
        h.set_mode(0);
        h.set_filter(2);
        h.set_channel_field(ChannelField::FrequencyHz, 14_100_000);
        m.store_channel(4, &mut h).unwrap();
        let old = holder(&m, 4).unwrap();
        let mut state = editor_state(4);
        state.mode_cache.set_valid(true);
        state.mode_cache.set_mode(1);
        state.mode_cache.set_tx_tone(31);
        state.filter_cache.set_valid(true);
        state.filter_cache.set_mode(0);
        state.filter_cache.set_data(0);
        state.filter_cache.set_filter(7);
        h.set_mode(1);
        let mut effects = Effects::default();
        let mut calls = 0;
        m.native_save_cached(
            &mut h,
            &mut state,
            CacheWrite {
                key: 0x108,
                previous: Some(&old),
            },
            EditorContext {
                ranges: &ranges,
                generation: ReferenceImage::Separate(&[]),
            },
            &mut effects,
            |_, w| {
                if w.stage() == ChannelStage::Frequency {
                    calls += 1;
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(h.filter(), 7);
        assert_eq!(h.tx_tone(), 31);
        assert_eq!(state.mode_cache.binding(), 77);
        let old = h.clone();
        h.set_data(1);
        let before = m.working().to_vec();
        assert!(
            m.native_save_cached(
                &mut h,
                &mut state,
                CacheWrite {
                    key: 0x109,
                    previous: Some(&old)
                },
                EditorContext {
                    ranges: &ranges,
                    generation: ReferenceImage::Separate(&[])
                },
                &mut effects,
                |_, _| Ok(())
            )
            .is_err()
        );
        assert_ne!(m.working(), before);
        assert_eq!(holder(&m, 4).unwrap().data(), 1);
        assert!(!state.mode_cache.valid());
        let mut g = vec![0; 0x4997a];
        g[0x37824] = 9;
        m.native_save_cached(
            &mut h,
            &mut state,
            CacheWrite {
                key: 0x109,
                previous: Some(&old),
            },
            EditorContext {
                ranges: &ranges,
                generation: ReferenceImage::Separate(&g),
            },
            &mut effects,
            |_, _| Ok(()),
        )
        .unwrap();
        assert_eq!(h.filter(), 9);
        let old = h.clone();
        h.set_filter(3);
        m.native_save_cached(
            &mut h,
            &mut state,
            CacheWrite {
                key: 0x10a,
                previous: Some(&old),
            },
            EditorContext {
                ranges: &ranges,
                generation: ReferenceImage::Separate(&[]),
            },
            &mut effects,
            |_, _| Ok(()),
        )
        .unwrap();
        assert!(!state.filter_cache.valid());
        assert_eq!(h.filter(), 3);
    }
    #[test]
    fn pending_registration_uses_separate_compiled_source_and_resets_before_writer() {
        let q = crate::icf::context::Context::from_bytes([0; 24]);
        let mut ranges = crate::icf::frequency::Ranges::unpopulated();
        ranges.regenerate(3, &q, None).unwrap();
        let mut m = model();
        let mut h = holder(&m, 10).unwrap();
        let header = (h.kind(), h.index(), h.binding());
        let mut state = editor_state(10);
        assert!(
            m.native_pending_name(&h, b"PENDING", &mut state.pending)
                .unwrap()
        );
        assert_eq!(m.used_slots(), 0);
        let mut g = vec![0; 0x4997a];
        g[0x3c84f + 5] = 1;
        g[0x3c84f + 7] = 29;
        let context = EditorContext {
            ranges: &ranges,
            generation: ReferenceImage::Separate(&g),
        };
        assert!(
            m.native_prepare_registration(&mut h, 146_000_000, (2, 3), context, &mut state.pending)
                .unwrap()
        );
        assert_eq!((h.kind(), h.index(), h.binding()), header);
        assert_eq!(m.used_slots(), 1);
        assert_eq!(h.name(), b"PENDING         ");
        assert!(!state.pending.active);
        assert_eq!(state.pending.index, u32::MAX);
        assert_eq!(h.tx_tone(), 29);
        // F1DC index 2 selects class 1 directly, not EFCC(default frequency).
        assert_eq!(h.offset_hz(), 600_000);
        assert!(
            m.native_save_cached(
                &mut h,
                &mut state,
                CacheWrite {
                    key: 0x107,
                    previous: None
                },
                context,
                &mut Effects::default(),
                |_, _| Err(invalid("injected registration writer failure"))
            )
            .is_err()
        );
        assert!(!state.pending.active);
        assert_eq!(m.used_slots(), 1);
        let mut initial = record::reset_holder(0x1d, 2, 91);
        let mut other_ranges = crate::icf::frequency::Ranges::unpopulated();
        other_ranges.regenerate(1, &q, None).unwrap();
        record::initial_normal_channel_into(
            &g,
            2,
            other_ranges.default_source_selector(None).unwrap(),
            &mut initial,
        )
        .unwrap();
        assert_eq!(
            initial.channel_field(ChannelField::FrequencyHz),
            146_520_000
        );
        let mut negative = holder(&m, 12).unwrap();
        state.pending.active = true;
        state.pending.index = 12;
        state.pending.bytes = b"KEEP".to_vec();
        m.native_prepare_registration(&mut negative, u32::MAX, (0, 3), context, &mut state.pending)
            .unwrap();
        assert!(state.pending.active);
        assert_eq!(
            negative.channel_field(ChannelField::FrequencyHz),
            14_100_000
        );
        assert_eq!(negative.mode(), 1);
        let mut inconsistent = holder(&m, 14).unwrap();
        let used = m.used_slots();
        assert!(
            m.native_prepare_registration(
                &mut inconsistent,
                146_000_000,
                (2, 1),
                context,
                &mut state.pending
            )
            .is_err()
        );
        assert_eq!(m.used_slots(), used + 1);
        assert!(!inconsistent.valid());
        assert!(state.pending.active);
    }
    #[test]
    fn f1dc_registration_keeps_allocation_and_pending_prefix_on_g_or_saver_failure() {
        let q = crate::icf::context::Context::from_bytes([0; 24]);
        let mut ranges = crate::icf::frequency::Ranges::unpopulated();
        ranges.regenerate(1, &q, None).unwrap();
        let mut m = model();
        let mut h = holder(&m, 10).unwrap();
        let original = h.clone();
        let mut state = EditorState::initialized();
        m.native_pending_name(&h, b"KEEP", &mut state.pending)
            .unwrap();
        let missing = EditorContext {
            ranges: &ranges,
            generation: ReferenceImage::Separate(&[]),
        };
        assert!(
            m.native_prepare_registration(&mut h, 146_520_000, (2, 1), missing, &mut state.pending)
                .is_err()
        );
        assert_eq!(m.used_slots(), 1);
        assert_eq!(h.as_bytes(), original.as_bytes());
        assert_eq!(state.pending.bytes, b"KEEP");
        assert!(state.pending.active);
        let mut m = model();
        let mut h = holder(&m, 10).unwrap();
        let generation = vec![0; 0x3c873];
        let context = EditorContext {
            ranges: &ranges,
            generation: ReferenceImage::Separate(&generation),
        };
        assert!(
            m.native_prepare_registration(&mut h, 146_520_000, (2, 1), context, &mut state.pending)
                .unwrap()
        );
        assert_eq!(h.offset_hz(), 600_000);
        assert_eq!(h.name(), b"KEEP            ");
        assert!(!state.pending.active);
        let mut saves = 0;
        let out = m.native_save_cached(
            &mut h,
            &mut state,
            CacheWrite {
                key: 0x107,
                previous: None,
            },
            context,
            &mut Effects::default(),
            |_, w| {
                if w.stage() == ChannelStage::Frequency {
                    saves += 1;
                    if saves == 2 {
                        return Err(invalid("stop second F1DC registration saver"));
                    }
                }
                Ok(())
            },
        );
        assert!(out.is_err());
        assert_eq!(saves, 2);
        assert_eq!(m.used_slots(), 1);
        assert!(!state.pending.active);
        let saved = holder(&m, 10).unwrap();
        assert!(saved.valid());
        assert_eq!(saved.offset_hz(), 600_000);
        assert_eq!(saved.name(), b"KEEP            ");
    }
    #[test]
    fn copy_modes_split_delete_and_home_failure_keep_literal_state() {
        let mut m = model();
        m.allocate(0, 2..3).unwrap();
        let mut h = record::reset_holder(2, 4, 77);
        h.set_mode(13);
        h.set_filter(1);
        h.set_split(1);
        h.set_channel_field(ChannelField::FrequencyHz, 123456789);
        m.store_channel(4, &mut h).unwrap();
        h.set_channel_field(ChannelField::FrequencyHz, 98765432);
        m.store_channel(5, &mut h).unwrap();
        let mut c = Clipboard::default();
        let mut e = Effects::default();
        m.native_copy(selection(5, 5), &mut c, &mut e).unwrap();
        assert_eq!((c.mode, c.declared_count, c.logical_count), (2, 1, 1));
        assert_eq!(c.records[0].side(), 0);
        assert_eq!(c.records[0].index(), 5);
        m.native_copy(
            Selection {
                show_both: true,
                ..selection(4, 5)
            },
            &mut c,
            &mut e,
        )
        .unwrap();
        assert_eq!((c.mode, c.declared_count, c.logical_count), (1, 2, 2));
        let before = m.working().to_vec();
        let result = m
            .native_delete(
                selection(4, 4),
                ReferenceImage::Separate(&[]),
                &mut e,
                |_| Ok(1),
            )
            .unwrap();
        assert_eq!(result.written, 1);
        assert_eq!(m.slot(0, 2).unwrap(), Some(0));
        let a = holder(&m, 4).unwrap();
        assert_eq!(a.channel_field(ChannelField::FrequencyHz), 98765432);
        assert_eq!(a.split(), 0);
        assert_eq!(
            &m.working()[super::super::MAP..],
            &before[super::super::MAP..]
        );
        assert!(
            m.native_delete(
                selection(4, 5),
                ReferenceImage::Separate(&[]),
                &mut e,
                |_| Ok(1)
            )
            .is_err()
        );
        assert_eq!(m.slot(0, 2).unwrap(), None);
        assert_eq!(m.group_used_slots(0).unwrap(), 0);
    }
    #[test]
    fn map_operations_keep_raw_records_and_reference_action_deltas() {
        let mut m = model();
        m.allocate(0, 2..5).unwrap();
        let body = m.working()[..0xa21c].to_vec();
        let mut e = Effects::default();
        m.working[0x392fa] = 0;
        m.working[0x392fb..0x392fd].copy_from_slice(&4u16.to_le_bytes());
        m.native_delete(selection(4, 5), ReferenceImage::Working, &mut e, |_| Ok(1))
            .unwrap();
        assert_eq!(u16::from_le_bytes(raw(m.working(), 0x392fb).unwrap()), 3);
        assert_eq!(m.slot(0, 2).unwrap(), Some(1));
        m.native_insert(selection(2, 3), ReferenceImage::Working, &mut e, |_| Ok(1))
            .unwrap();
        assert_eq!(u16::from_le_bytes(raw(m.working(), 0x392fb).unwrap()), 4);
        m.native_carry_step2(selection(6, 7), -1, ReferenceImage::Working, &mut e)
            .unwrap();
        assert_eq!(u16::from_le_bytes(raw(m.working(), 0x392fb).unwrap()), 4);
        assert_eq!(&m.working()[..0xa21c], &body);
        assert!(compatible(4, 2) && compatible(5, 3) && compatible(6, 2) && compatible(7, 2));
        assert!(!compatible(0x0c, 2) && !compatible(8, 2));
    }
    #[test]
    fn paste_preserves_old_target_metadata_and_mutates_only_native_source_fields() {
        let mut m = model();
        m.allocate(0, 2..3).unwrap();
        let mut target = record::reset_holder(2, 4, 17);
        target.set_name(b"TARGET NAME     ");
        target.set_mode(13);
        target.set_split(1);
        target.set_channel_field(ChannelField::Selection, 2);
        m.store_channel(4, &mut target).unwrap();
        let mut source = record::reset_holder(4, 8, 99);
        source.set_name(b"SOURCE NAME     ");
        source.set_mode(0x10f);
        source.set_data(7);
        source.set_filter(8);
        source.set_offset_hz(149);
        source.set_channel_field(ChannelField::FrequencyHz, 145000000);
        let mut clip = Clipboard {
            source_kind: 4,
            mode: 2,
            declared_count: 1,
            logical_count: 1,
            records: vec![source],
        };
        let mut e = Effects::default();
        let s = Selection {
            show_both: true,
            ..selection(5, 5)
        };
        let out = m
            .native_paste(
                s,
                &mut clip,
                ReferenceImage::Working,
                &mut e,
                |_| Ok(1),
                |_, _| Ok(()),
            )
            .unwrap();
        assert_eq!(out.written, 1);
        assert_eq!(out.native_count, 1);
        let h = holder(&m, 5).unwrap();
        assert_eq!(h.name(), b"TARGET NAME     ");
        assert_eq!(h.split(), 1);
        assert_eq!(h.offset_hz(), 149);
        assert_eq!(h.filter(), 0);
        assert_eq!(h.data(), 0);
        assert_eq!(clip.records[0].name(), b"SOURCE NAME     ");
        assert_eq!(clip.records[0].mode(), 0x10f);
        assert_eq!(clip.records[0].offset_hz(), 149);
        assert_eq!(clip.records[0].data(), 0);
        assert_eq!(clip.records[0].index(), 5);
        clip.records[0].set_channel_field(ChannelField::FrequencyHz, 146000000);
        let before = m.working().to_vec();
        let error = m.native_paste(
            s,
            &mut clip,
            ReferenceImage::Working,
            &mut e,
            |_| Ok(1),
            |_, w| {
                if w.stage() == ChannelStage::Filter {
                    Err(invalid("injected native write failure"))
                } else {
                    Ok(())
                }
            },
        );
        assert!(error.is_err());
        assert_ne!(m.working(), before);
        assert_eq!(clip.records[0].name(), b"TARGET NAME     ");
    }
}
