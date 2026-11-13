//! Shared source-generation and stage-9 normalization primitives.
//!
//! These functions preserve surprising original arithmetic and mutation order.
//! The complete driver is [`super::stage9::normalize`]; these components do not
//! publish its terminal signature. F supplies defaults, C supplies current, and
//! T supplies late list state. Each role can alias W, so reads must observe prior
//! writes rather than an entry snapshot. Errors retain the completed write prefix.

use super::Error;
pub use super::frequency::{FrequencyContext, FrequencyRange, previous_channel_bad};
use super::record::{self, ChannelWrite, DecodedRecord};
use super::record_image::{self, ChannelKind};

/// F360 stage 7 for an immutable, fully generated range-list generation.
/// The caller selects F8 when its captured selector is zero, otherwise D0.
pub fn stage7(image: &mut [u8], ranges: &[FrequencyRange]) -> Result<(), Error> {
    let count = i32::try_from(ranges.len())
        .map_err(|_| invalid("range-list count exceeds signed DWORD"))?;
    stage7_live(
        image,
        || Ok(count),
        |i| {
            Ok(ranges.get(i as usize).copied().unwrap_or(FrequencyRange {
                low: u32::MAX,
                high: u32::MAX,
                flag: 1,
            }))
        },
    )
}

/// F360's exact live read order: count once per iteration, then getter twice.
///
/// The first getter supplies low and the second supplies high. A merged range
/// updates only the preceding output's high: previous_high is deliberately not
/// updated. An invalid output writes zero/zero but also keeps previous_high.
/// Every successful call fills exactly 30 pairs at 378C0..379B0. Callback errors
/// expose an already-written prefix rather than rolling it back.
pub fn stage7_live(
    image: &mut [u8],
    mut count: impl FnMut() -> Result<i32, Error>,
    mut get: impl FnMut(i32) -> Result<FrequencyRange, Error>,
) -> Result<(), Error> {
    if image.len() < 0x379b0 {
        return Err(invalid("stage 7 destination window unavailable"));
    }
    let (mut index, mut output, mut previous_high) = (0_i32, 0_usize, 0_u32);
    while output < 30 {
        let (low, high) = if count()? > index {
            (get(index)?.low, get(index)?.high)
        } else {
            (u32::MAX, u32::MAX)
        };
        if low == u32::MAX || high == u32::MAX {
            image[0x378c0 + 8 * output..0x378c8 + 8 * output].fill(0);
            output += 1;
        } else if index > 0 && low.wrapping_sub(1) == previous_high {
            let at = 0x378c0 + 8 * (output - 1) + 4;
            image[at..at + 4].copy_from_slice(&high.to_le_bytes());
        } else {
            let at = 0x378c0 + 8 * output;
            image[at..at + 4].copy_from_slice(&low.to_le_bytes());
            image[at + 4..at + 8].copy_from_slice(&high.to_le_bytes());
            previous_high = high;
            output += 1;
        }
        index = index
            .checked_add(1)
            .ok_or_else(|| invalid("stage 7 source index exhausted signed DWORD"))?;
    }
    Ok(())
}

/// 99F04: unsigned addition wraps before division; the frequency argument is unused.
pub fn round_offset_hz(value: u32) -> u32 {
    value.wrapping_add(50) / 100 * 100
}

/// The fields changed by the stage-9 typed-prefix code before conditional reload.
/// Full mode DWORD and low mode byte remain distinct inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelValues {
    pub mode: u32,
    pub data: u32,
    pub filter: u32,
    pub offset_hz: u32,
    pub tx_tone: u32,
    pub rx_tone: u32,
}

/// Apply only 4D5xx's pre-reload arithmetic and mode corrections.
///
/// A later previous-row reload can replace these values; callers must not repeat
/// offset rounding after that reload. `remove_mton` is the captured Q12==0 result,
/// not a guess from a radio/region name. Scan rows never run the tone replacement.
pub fn normalize_channel_values(values: &mut ChannelValues, scan: bool, remove_mton: bool) {
    values.offset_hz = round_offset_hz(values.offset_hz);
    normalize_channel_mode(values, scan, remove_mton);
}

fn normalize_channel_mode(values: &mut ChannelValues, scan: bool, remove_mton: bool) {
    if matches!(values.mode as u8, 0x0f | 0x11) {
        values.filter = if scan { 255 } else { 0 };
    }
    if !matches!(values.mode, 0 | 1 | 8 | 13) {
        values.data = 0;
    }
    if !scan && remove_mton {
        if values.tx_tone == 50 {
            values.tx_tone = 24;
        }
        if values.rx_tone == 50 {
            values.rx_tone = 24;
        }
    }
}

/// Explicit source binding for a read from the entry-time default object.
/// Working means proven W=F alias: every read sees the current working bytes,
/// not an independent snapshot made before earlier rows were stored.
#[derive(Debug, Clone, Copy)]
pub enum ReferenceImage<'a> {
    Working,
    Separate(&'a [u8]),
}
impl ReferenceImage<'_> {
    pub(super) fn get<'a>(&'a self, working: &'a [u8]) -> &'a [u8] {
        match self {
            Self::Working => working,
            Self::Separate(image) => image,
        }
    }
}

/// Captured stage9 inputs for one normal/call typed index, not inferred defaults.
#[derive(Debug, Clone, Copy)]
pub struct ChannelNormalization {
    pub kind: ChannelKind,
    pub index: u32,
    pub working_binding: u32,
    pub default_binding: u32,
    /// Captured `Q[12]`; only zero enables replacement of tone index 50.
    pub q12: u8,
    /// Captured VA 864614; only zero permits the split-OFF previous-side reload.
    pub previous_reload_guard: u32,
}

/// Execute stage9's actual normal/call typed load/correct/store/reload sequence.
///
/// This is a prefix component, not a complete stage9 or a signature repair. The
/// caller supplies the real 61CE0(arg0) predicate, including its ordered range and
/// record reads. It is called only after offset rounding and only for side 1 with
/// split 0/guard 0. The unused-looking same-index default load is mandatory and can
/// fail before any working-image write. Previous-side reload is NOT re-rounded.
/// Every raw store uses the shared codec; failure retains earlier image commits.
pub fn normalize_channel_at(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    input: ChannelNormalization,
    mut previous_bad: impl FnMut(&[u8], &DecodedRecord) -> Result<bool, Error>,
    before_store: impl FnMut(&ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<Option<DecodedRecord>, Error> {
    let load = |source: &[u8], index: u32, binding: u32| -> Result<DecodedRecord, Error> {
        let mut holder = record::reset_holder(
            match input.kind {
                ChannelKind::Normal => 2,
                ChannelKind::Call => 3,
            },
            index,
            binding,
        );
        record_image::load_channel_into(source, input.kind, index, &mut holder)?;
        Ok(holder)
    };
    let mut holder = load(image, input.index, input.working_binding)?;
    if !holder.valid() {
        return Ok(None);
    }
    let _default_holder = load(defaults.get(image), input.index, input.default_binding)?;
    holder.set_offset_hz(round_offset_hz(holder.offset_hz()));
    if input.previous_reload_guard == 0
        && holder.as_bytes()[0x25] == 0
        && holder.as_bytes()[0x24] == 1
        && previous_bad(image, &holder)?
    {
        let previous = input
            .index
            .checked_sub(1)
            .ok_or(invalid("previous typed channel index unavailable"))?;
        holder = load(image, previous, input.working_binding)?;
        holder.set_header(holder.kind(), input.index, holder.binding());
    }
    normalize_holder_mode(&mut holder, false, input.q12);
    record_image::store_channel_into(image, input.kind, input.index, &mut holder, before_store)?;
    Ok(Some(load(image, input.index, input.working_binding)?))
}

fn normalize_holder_mode(holder: &mut DecodedRecord, scan: bool, q12: u8) {
    let mut values = ChannelValues {
        mode: holder.mode(),
        data: holder.data(),
        filter: holder.filter(),
        offset_hz: holder.offset_hz(),
        tx_tone: holder.tx_tone(),
        rx_tone: holder.rx_tone(),
    };
    normalize_channel_mode(&mut values, scan, q12 == 0);
    holder.set_filter(values.filter);
    holder.set_data(values.data);
    holder.set_tx_tone(values.tx_tone);
    holder.set_rx_tone(values.rx_tone);
}

/// Stage9's pair/mode-pair branch: load W, test validity, read the same live F
/// row, identity both endpoints, ordered save and reload. These kinds must NOT
/// run the channel offset rounding or mode/DATA/MTON corrections.
pub fn normalize_range_at(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    kind: u32,
    index: u32,
    bindings: (u32, u32),
    before_store: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<Option<DecodedRecord>, Error> {
    let load = |source: &[u8], binding| -> Result<DecodedRecord, Error> {
        let mut holder = record::reset_holder(kind, index, binding);
        record_image::load_range_into(source, kind, index, &mut holder)?;
        Ok(holder)
    };
    let mut holder = load(image, bindings.0)?;
    if !holder.valid() {
        return Ok(None);
    }
    let _default_holder = load(defaults.get(image), bindings.1)?;
    record_image::store_range_into(image, kind, index, &mut holder, before_store)?;
    Ok(Some(load(image, bindings.0)?))
}

/// Stage9 kind 04/05/08/19/1A/1B: real W load, validity gate, same-row live F
/// load, offset rounding and mode/DATA/tone correction, ordered save and reload.
/// Scan uses filter 255 and never replaces tone 50. The raw saver remains special's
/// single implementation. Callback offsets are relative for compact, absolute for
/// scan, exactly as their image-adapter APIs specify.
pub fn normalize_special_at(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    kind: u32,
    index: u32,
    bindings: (u32, u32),
    q12: u8,
    before_store: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<Option<DecodedRecord>, Error> {
    let load = |source: &[u8], binding| -> Result<DecodedRecord, Error> {
        let mut holder = record::reset_holder(kind, index, binding);
        match kind {
            4 | 5 => record_image::load_compact_into(source, kind, index, &mut holder)?,
            8 => record_image::load_scan_into(source, index, &mut holder)?,
            0x19..=0x1b => record_image::load_paired_into(source, kind, index, &mut holder)?,
            _ => {
                return Err(invalid(
                    "stage9 special prefix requires a compact or scan kind",
                ));
            }
        };
        Ok(holder)
    };
    let mut holder = load(image, bindings.0)?;
    if !holder.valid() {
        return Ok(None);
    }
    let _default_holder = load(defaults.get(image), bindings.1)?;
    holder.set_offset_hz(round_offset_hz(holder.offset_hz()));
    normalize_holder_mode(&mut holder, kind == 8, q12);
    if kind == 8 {
        record_image::store_scan_into(image, index, &mut holder, before_store)?;
    } else if matches!(kind, 0x19..=0x1b) {
        record_image::store_paired_into(image, kind, index, &mut holder, before_store)?;
    } else {
        record_image::store_compact_into(image, kind, index, &mut holder, before_store)?;
    }
    Ok(Some(load(image, bindings.0)?))
}

/// Runtime table built by 5B30/4C404, in actual slot order (not numeric kind order).
pub const TYPED_SLOTS: [(u32, u32, u32); 11] = [
    (2, 0, 19999),
    (3, 0, 7),
    (4, 0, 44),
    (5, 1, 10),
    (8, 0, 24),
    (0x15, 0, 29),
    (0x16, 0, 67),
    (0x18, 0, 11),
    (0x19, 0, 1),
    (0x1a, 0, 5),
    (0x1b, 0, 1),
];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypedCounts {
    pub saved: u32,
    pub memopad_valid: u32,
}

/// Execute ALL 11 runtime slots, using the shared generated B and actual codecs.
/// The type 18 entry condition and tone byte are captured before the table walk.
/// Every valid row loads live F, stores, then reloads. Memopad compaction runs
/// after its slot even if no valid row was encountered. No marker is written
/// here: the full driver must still finish type 0C, central and R/V/reference work.
pub fn typed_loop(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    q: &super::context::Context,
    ranges: &super::frequency::Ranges,
    system_langid: Option<u16>,
    previous_reload_guard: u32,
) -> Result<TypedCounts, Error> {
    // 4D4A7/4D4B9 captures NOT(56E9C(Q)); 56E9C is Q13==1.
    // 569FC is also captured unconditionally, even when the first flag skips 18.
    let q13 = q.as_bytes()[0x13];
    let japanese = q.source_selector(system_langid)? == 0;
    let skip18 = q13 != 1 || japanese;
    let q12 = q.as_bytes()[0x12];
    let bindings = (
        1,
        if matches!(defaults, ReferenceImage::Working) {
            1
        } else {
            2
        },
    );
    let frequency = ranges.frequency_context(q.fields().0[0] as i32);
    let mut counts = TypedCounts {
        saved: 0,
        memopad_valid: 0,
    };
    for (kind, first, last) in TYPED_SLOTS {
        if kind == 0x18 && skip18 {
            continue;
        }
        for index in first..=last {
            let result = match kind {
                2 | 3 => normalize_channel_at(
                    image,
                    defaults,
                    ChannelNormalization {
                        kind: if kind == 2 {
                            ChannelKind::Normal
                        } else {
                            ChannelKind::Call
                        },
                        index,
                        working_binding: bindings.0,
                        default_binding: bindings.1,
                        q12,
                        previous_reload_guard,
                    },
                    |_, holder| previous_channel_bad(holder, Some(&frequency)),
                    |_| Ok(()),
                )?,
                4 | 5 | 8 | 0x19..=0x1b => normalize_special_at(
                    image,
                    defaults,
                    kind,
                    index,
                    bindings,
                    q12,
                    |_, _| Ok(()),
                )?,
                _ => normalize_range_at(image, defaults, kind, index, bindings, |_, _| Ok(()))?,
            };
            if result.is_some() {
                counts.saved += 1;
                if kind == 5 {
                    counts.memopad_valid += 1;
                }
            }
        }
        if kind == 5 {
            stage9_memopad(image, defaults, bindings, |_, _, _| Ok(()))?;
        }
    }
    Ok(counts)
}

/// 4F318's memopad 1..10 post-loop compaction. Exactly 10 passes, not a raw slice
/// rotation: each shifted row goes through the shared loader/saver. Only the
/// frequency of the retained shift holder becomes MAX at the tail; its validity
/// and body are NOT cleared. With no observed non-hole, restore 151 DWORDs from
/// each live F holder (not a raw 36 copy). Slot 0 is never visited.
pub fn stage9_memopad(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    bindings: (u32, u32),
    mut before_store: impl FnMut(u32, usize, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    let load = |source: &[u8], index, binding| -> Result<DecodedRecord, Error> {
        let mut holder = record::reset_holder(5, index, binding);
        record_image::load_compact_into(source, 5, index, &mut holder)?;
        Ok(holder)
    };
    let mut shifted = record::reset_holder(0, 0, bindings.0);
    let mut cursor = 1;
    let mut observed_non_hole = false;
    for _ in 0..10 {
        let holder = load(image, cursor, bindings.0)?;
        if holder.as_bytes()[0x3c..0x40] == u32::MAX.to_le_bytes() {
            for source in cursor + 1..=10 {
                shifted = load(image, source, bindings.0)?;
                record_image::store_compact_into(
                    image,
                    5,
                    source - 1,
                    &mut shifted,
                    |at, bytes| before_store(source - 1, at, bytes),
                )?;
            }
            shifted.as_mut_bytes()[0x3c..0x40].fill(0xff);
            record_image::store_compact_into(image, 5, 10, &mut shifted, |at, bytes| {
                before_store(10, at, bytes)
            })?;
        } else {
            observed_non_hole = true;
            cursor += 1;
        }
    }
    if !observed_non_hole {
        let mut target = record::reset_holder(5, cursor, bindings.0);
        for index in 1..=10 {
            let source = load(defaults.get(image), index, bindings.1)?;
            target.as_mut_bytes()[12..].copy_from_slice(&source.as_bytes()[12..]);
            record_image::store_compact_into(image, 5, index, &mut target, |at, bytes| {
                before_store(index, at, bytes)
            })?;
        }
    }
    Ok(())
}

/// 476E4 bounded byte sanitation. Lead/trail classes are byte ranges, not an
/// assertion that the pair is assigned in CP932. NUL does not stop this pass.
/// Replacement zero additionally compacts zeros and pads the remainder with spaces.
pub fn sanitize_fixed(bytes: &mut [u8], replacement: u8) {
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        if crate::csv::is_lead(byte) {
            if at + 1 == bytes.len() {
                bytes[at] = replacement;
                at += 1;
            } else {
                if !matches!(bytes[at + 1], 0x40..=0x7e | 0x80..=0xfc) {
                    bytes[at] = replacement;
                    bytes[at + 1] = replacement;
                }
                at += 2;
            }
        } else {
            if !matches!(byte, 0x20..=0x7e | 0xa1..=0xdf) {
                bytes[at] = replacement;
            }
            at += 1;
        }
    }
    if replacement == 0 {
        compact_zero(bytes);
    }
}

/// 47860 bounded DBCS removal. A lead consumes/replaces its following byte
/// without checking that byte's trail class. The last lead touches only itself.
pub fn remove_dbcs_fixed(bytes: &mut [u8], replacement: u8) {
    let mut at = 0;
    while at < bytes.len() {
        if crate::csv::is_lead(bytes[at]) {
            bytes[at] = replacement;
            at += 1;
            if at < bytes.len() {
                bytes[at] = replacement;
            }
        }
        at += 1;
    }
    if replacement == 0 {
        compact_zero(bytes);
    }
}

fn compact_zero(bytes: &mut [u8]) {
    let mut target = 0;
    for source in 0..bytes.len() {
        if bytes[source] != 0 {
            bytes[target] = bytes[source];
            target += 1;
        }
    }
    bytes[target..].fill(b' ');
}

/// Stage-9's six fixed 20-byte windows, in actual call order. Both helpers use
/// replacement space, so they never append a terminator or compact the buffer.
pub fn stage9_fixed_text(image: &mut [u8]) -> Result<(), Error> {
    if image.len() < 0x3c63c {
        return Err(invalid("stage 9 fixed-text windows unavailable"));
    }
    for at in (0..5).map(|i| 0x32bb5 + 20 * i).chain([0x3c628]) {
        let field = &mut image[at..at + 20];
        sanitize_fixed(field, b' ');
        remove_dbcs_fixed(field, b' ');
    }
    Ok(())
}

/// 99FD0's fixed byte scan: ASCII uppercase, legal keyer alphabet, and at most
/// the first star unless the incoming byte flag already suppresses every star.
/// NUL is replaced with space and does not terminate this fixed-length pass.
pub fn normalize_keyer_fixed(bytes: &mut [u8], disallow_star: bool) {
    let mut star = disallow_star;
    for byte in bytes {
        *byte = byte.to_ascii_uppercase();
        if *byte == b'*' {
            if star {
                *byte = b' ';
            }
            star = true;
        } else if !super::messages::keyer_byte(*byte) {
            *byte = b' ';
        }
    }
}

/// 4DBE3..4DCA4 keyer loop, preserving the distinct CURRENT role's read order.
/// After each W row is sanitized, the still-zero cross-row flag is set by that
/// row of live C, not by an entry snapshot of W. Once set, no more C rows are
/// read. Separate C therefore need not produce the same star outcome as W=C.
pub fn stage9_keyer(image: &mut [u8], current: ReferenceImage<'_>) -> Result<(), Error> {
    if image.len() < 0x37388 {
        return Err(invalid("stage9 keyer working windows unavailable"));
    }
    let mut star = false;
    for row in 0..8 {
        let at = 0x37158 + 70 * row;
        normalize_keyer_fixed(&mut image[at..at + 70], star);
        if !star {
            let bytes = current
                .get(image)
                .get(at..at + 70)
                .ok_or(invalid("stage9 keyer current window unavailable"))?;
            star = bytes
                .split(|&byte| byte == 0)
                .next()
                .unwrap()
                .contains(&b'*');
        }
    }
    Ok(())
}

pub(super) fn copy_mask(
    image: &mut [u8],
    source: ReferenceImage<'_>,
    at: usize,
    mask: u8,
) -> Result<(), Error> {
    let old = *image
        .get(at)
        .ok_or(invalid("stage9 masked destination unavailable"))?;
    let value = *source
        .get(image)
        .get(at)
        .ok_or(invalid("stage9 masked source unavailable"))?;
    image[at] = (old & !mask) | (value & mask);
    Ok(())
}

pub(super) fn copy_field(
    image: &mut [u8],
    source: ReferenceImage<'_>,
    at: usize,
    length: usize,
) -> Result<(), Error> {
    let end = at
        .checked_add(length)
        .ok_or(invalid("stage9 copy window overflow"))?;
    let bytes = source
        .get(image)
        .get(at..end)
        .ok_or(invalid("stage9 copy source unavailable"))?
        .to_vec();
    super::image::apply_patches(image, &[(at, &bytes)])
}

/// 4DCA4..4DEFB's independent control-bit restoration block. Only original
/// bits 7/6/4/3/0 of 37780, bits 3..0 of 37781 and full 377FE come from live F.
/// No mode/radio inference, opaque-bit reset or whole-byte substitute is added.
pub fn stage9_control_bits(image: &mut [u8], defaults: ReferenceImage<'_>) -> Result<(), Error> {
    copy_mask(image, defaults, 0x37780, 0xd9)?;
    copy_mask(image, defaults, 0x37781, 0x0f)?;
    copy_field(image, defaults, 0x377fe, 1)
}

/// 4D9F9..4DAF8: restore the default revision, validate the selected preset via
/// its existing dependency codec, then apply the three real Q conditions to
/// band-stack windows in original order. This stops BEFORE the reserved-memopad
/// predicate block; it does not pretend to complete central stage9 or write its
/// final marker. Defaults are read live at each copy, including proven W=F.
pub fn stage9_profile_prefix(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    context: &super::context::Context,
) -> Result<(), Error> {
    copy_field(image, defaults, 0x355a1, 1)?;
    let selected = usize::from(image[0xa78c]);
    let usable =
        selected < 5 && super::special::Preset::read(image, selected)?.has_enabled_fields()?;
    if !usable {
        copy_field(image, defaults, 0xa78c, 1)?;
    }
    if context.condition_58bdc()? {
        copy_field(image, defaults, 0x36cb7, 108)?;
        copy_field(image, defaults, 0x36f48, 1)?;
    }
    if context.condition_58c5c()? {
        copy_field(image, defaults, 0x36d23, 108)?;
        copy_field(image, defaults, 0x36d8f, 108)?;
        copy_field(image, defaults, 0x36f49, 1)?;
        copy_field(image, defaults, 0x36f4a, 1)?;
    }
    if context.condition_58c78() {
        copy_field(image, defaults, 0x36ed3, 108)?;
        copy_field(image, defaults, 0x36f4d, 1)?;
    }
    Ok(())
}

/// 4DF55..4E222's Q-conditional bits 0..2 restoration followed by the independent
/// 17-byte bit loop. The low 2-bit passes precede the bit 2 passes. W=F is still
/// nonidentity: indices 13..16 lose bit 2, then all 17 bytes gain bit 3.
pub fn stage9_filter_bits(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    context: &super::context::Context,
) -> Result<(), Error> {
    if context.condition_58bdc()? {
        for mask in [3, 4] {
            for at in 0x378b9..0x378bc {
                copy_mask(image, defaults, at, mask)?;
            }
        }
    }
    if context.condition_58c5c()? {
        for mask in [3, 4] {
            for at in 0x378bc..0x378be {
                copy_mask(image, defaults, at, mask)?;
            }
        }
    }
    for at in 0x378bc..0x378c0 {
        *image
            .get_mut(at)
            .ok_or(invalid("stage9 filter bit window unavailable"))? &= !4;
    }
    for at in 0x378af..0x378c0 {
        *image
            .get_mut(at)
            .ok_or(invalid("stage9 filter bit window unavailable"))? |= 8;
    }
    Ok(())
}

/// 4E222..4E5D1's remaining central settings copies/corrections. q13 is the
/// ENTRY-captured byte; the other capability predicates use the supplied live Q.
/// SYSTEM LANGID is consulted only on the original selected branch. This does
/// not execute the preceding selector validation or the following R/V pipeline.
pub fn stage9_settings_tail(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    context: &super::context::Context,
    q13: u8,
    system_langid: Option<u16>,
) -> Result<(), Error> {
    if context.condition_58bdc()? {
        for at in [0x377ef, 0x377f1, 0x377f3] {
            copy_field(image, defaults, at, 2)?;
        }
    }
    if context.condition_58c5c()? {
        for at in [0x377f5, 0x377f7] {
            copy_field(image, defaults, at, 2)?;
        }
    }
    for at in 0x379b0..0x379b3 {
        copy_field(image, defaults, at, 1)?;
    }
    if context.condition_58bdc()? {
        for (word, body) in [(0x379cd, 0x37ad5), (0x379cf, 0x37aee)] {
            copy_field(image, defaults, word, 2)?;
            copy_field(image, defaults, body, 25)?;
        }
    }
    if context.condition_58c14()? {
        copy_field(image, defaults, 0x379d1, 2)?;
        copy_field(image, defaults, 0x37b07, 25)?;
    }
    if context.condition_58c5c()? {
        for (word, body) in [(0x379d3, 0x37b20), (0x379d5, 0x37b39)] {
            copy_field(image, defaults, word, 2)?;
            copy_field(image, defaults, body, 25)?;
        }
    }
    copy_field(image, defaults, 0x37b85, 512)?;
    if byte(image, 0x37d89)? == 0 {
        copy_field(image, defaults, 0x37d85, 1)?;
    }
    if byte(image, 0x37da5)? == 0 {
        copy_field(image, defaults, 0x37da6, 1)?;
        copy_field(image, defaults, 0x37da7, 4)?;
    }
    copy_field(image, defaults, 0x37dab, 2)?;
    copy_field(image, defaults, 0x37dad, 1)?;
    if q13 != 1 {
        copy_field(image, defaults, 0x39312, 1)?;
    } else if context.source_selector(system_langid)? == 0 && byte(image, 0x39312)? == 1 {
        super::image::apply_patches(image, &[(0x39312, &[2])])?;
    }
    if context.as_bytes()[0x11] == 0 {
        for at in 0x3970a..0x3971a {
            if byte(image, at)? == 0x18 {
                copy_field(image, defaults, at, 1)?;
            }
        }
    }
    if context.condition_58bdc()? {
        for at in [0x3987a, 0x3989a, 0x398ba] {
            copy_field(image, defaults, at, 32)?;
        }
    }
    if context.condition_58c5c()? {
        for at in [0x398da, 0x398fa] {
            copy_field(image, defaults, at, 32)?;
        }
    }
    copy_field(image, defaults, 0x3996b, 1)?;
    if byte(image, 0x39313)? != 2 || byte(image, 0x39314)? != 0 {
        *image
            .get_mut(0x37781)
            .ok_or(invalid("stage9 RTTY bit window unavailable"))? &= !0x20;
        super::image::apply_patches(image, &[(0x399a0, &[0])])?;
    }
    for at in [0x399b7, 0x399b8, 0x399bf] {
        copy_field(image, defaults, at, 1)?;
    }
    Ok(())
}

fn byte(image: &[u8], at: usize) -> Result<u8, Error> {
    image
        .get(at)
        .copied()
        .ok_or(invalid("stage9 source byte unavailable"))
}

/// 631CC/C6190 ->625F8(type0D,arg0 then arg1): the WHOLE live T list, not only
/// the selected row. C9678 compares each row with BE4A4's first exact match;
/// blank returns 300 without a search. No trim/casefold/valid gate or order map.
/// Count 0 returns before any holder load. The arg 1 implementation is originally
/// constant false, so it adds no reads after a successful complete arg 0 pass.
pub fn late_callsign_list_bad(current: &[u8], binding: u32) -> Result<bool, Error> {
    let count = u16::from_le_bytes(
        current
            .get(0x49978..0x4997a)
            .ok_or(invalid("late T callsign count unavailable"))?
            .try_into()
            .unwrap(),
    );
    for index in 0..u32::from(count) {
        let mut holder = record::reset_holder(0x0d, index, binding);
        record_image::load_callsign_into(current, index, &mut holder)?;
        if record_image::callsign_bad(current, &holder)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Observable list phase of 4F538, not the original AL return (AL only reports
/// typed deletion; a saved frequency/callsign correction can leave AL zero).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResetLists {
    pub saved_repeaters: u32,
    pub deleted_repeaters: u32,
    pub deleted_ur: u32,
    pub deleted_allowed: u32,
}

/// Shared 61CE0/61D34/61E3C/61ED4/61E88/61DC0/61D88/61EAC dispatch.
/// The bool is the native arg: false=specific, true=numeric-only. Each field
/// predicate remains owned by its existing codec/engine; no arbitrary result
/// provider or second band classifier is accepted here.
pub fn typed_bad(
    image: &[u8],
    holder: &DecodedRecord,
    numeric_only: bool,
    q: &super::context::Context,
    ranges: &super::frequency::Ranges,
    system_langid: Option<u16>,
) -> Result<bool, Error> {
    use super::special::{EdgeSide, ScanEdge, UserEdge};
    match holder.kind() {
        2..=5 | 0x19..=0x1b => super::frequency::channel_bad(holder, ranges, q, numeric_only),
        8 => {
            let h = ScanEdge::read(image, holder.index() as usize)?;
            if numeric_only {
                return Ok(!h.suitable(true, true, false, |_| {
                    unreachable!("numeric scan does not query B/Q")
                })?);
            }
            let bit = if numeric_only { 8 } else { 4 };
            Ok(
                h.warnings(EdgeSide::High, |hz| Ok(ranges.eb64(q, hz)))? & bit != 0
                    || h.warnings(EdgeSide::Low, |hz| Ok(ranges.eb64(q, hz)))? & bit != 0
                    || (!numeric_only && h.valid && holder.mode() == 0x18),
            )
        }
        0x15 => {
            if numeric_only {
                let mut low = holder.clone();
                low.set_channel_field(
                    record::ChannelField::FrequencyHz,
                    u32::from_le_bytes(holder.as_bytes()[0x138..0x13c].try_into().unwrap()),
                );
                return Ok(super::frequency::channel_bad(holder, ranges, q, true)?
                    || super::frequency::channel_bad(&low, ranges, q, true)?);
            }
            let bit = if numeric_only { 8 } else { 4 };
            Ok(UserEdge::warnings(
                image,
                holder.index() as usize,
                EdgeSide::High,
                |hz| Ok(ranges.f32c(q, hz)),
                |hz| Ok(ranges.ed70(hz)),
            )?
            .0 & bit
                != 0
                || UserEdge::warnings(
                    image,
                    holder.index() as usize,
                    EdgeSide::Low,
                    |hz| Ok(ranges.f32c(q, hz)),
                    |hz| Ok(ranges.ed70(hz)),
                )?
                .0 & bit
                    != 0)
        }
        0x16 => super::settings::extended::fixed_edge_bad(holder, ranges, q, numeric_only),
        0x18 => super::frequency::auto_repeater_bad(holder, ranges, q, numeric_only, system_langid),
        0x0c => crate::repeater::predicate::row_bad(image, holder, numeric_only, ranges, q),
        0x0d | 0x17 => {
            if numeric_only {
                Ok(false)
            } else {
                record_image::callsign_bad(image, holder)
            }
        } // Native arg 1 is constant false.
        _ => Err(invalid("typed warning kind not implemented")),
    }
}

/// Actual caller-owned show-both DWORD and VA 864930 cache byte. Neither is a
/// predicate result, image field or default inferred from the current profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResetState {
    pub show_both: u32,
    pub home_invalidated: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResetReport {
    /// Valid-row 81E8 invocations, not a byte-difference count.
    pub typed_saved: u32,
    pub lists: ResetLists,
}

/// Actual 4F538 for W=current (C/T alias W). Stage9/terminal marker are separate.
/// Errors leave earlier Home, map, typed save and deletion prefixes intact.
pub fn reset_operating_state(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    q: &super::context::Context,
    ranges: &super::frequency::Ranges,
    system_langid: Option<u16>,
    state: &mut ResetState,
) -> Result<ResetReport, Error> {
    use super::record_image::{ChannelFilter, ChannelView};
    let word = |image: &[u8], at: usize| -> Result<u16, Error> {
        Ok(u16::from_le_bytes(
            image
                .get(at..at + 2)
                .ok_or(invalid("reset word unavailable"))?
                .try_into()
                .unwrap(),
        ))
    };
    let row = word(image, 0x392fb)?;
    if row != u16::MAX {
        let group = u32::from(byte(image, 0x392fa)?);
        if record_image::channel_bad_count(
            image,
            ChannelView {
                group,
                first: 2 * i32::from(row),
                last: 2 * i32::from(row) + 1,
                count: 2,
            },
            ChannelFilter {
                broad: false,
                include_side1: true,
                count_all: false,
                show_both: state.show_both,
            },
            ranges,
            q,
            1,
        )? != 0
        {
            super::image::apply_patches(image, &[(0x392fa, &[255])])?;
            super::image::apply_patches(image, &[(0x392fb, &u16::MAX.to_le_bytes())])?;
        }
    }
    let reference = word(image, 0x392fd)?;
    if reference != u16::MAX {
        let mut h = record::reset_holder(0x0c, u32::from(reference), 1);
        record_image::load_into(image, 0x0c, u32::from(reference), &mut h)?;
        if !crate::repeater::predicate::home_usable(image, &h, ranges, q)? {
            super::image::apply_patches(image, &[(0x392fd, &u16::MAX.to_le_bytes())])?;
        }
    }
    let selected = usize::from(super::special::selected_scan_raw(image)?);
    if selected != 25 {
        let mut h = record::reset_holder(8, selected as u32, 1);
        record_image::load_into(image, 8, selected as u32, &mut h)?;
        if typed_bad(image, &h, false, q, ranges, system_langid)? {
            let suitable =
                super::special::scan_suitable_rows(image, image, false, false, false, |hz| {
                    Ok(ranges.eb64(q, hz))
                })?;
            if suitable.len() == 1 {
                let reset = super::special::scan_reset_enabled(image, 0)?;
                let mut editor = super::Icf::from_memory(
                    image
                        .get(..0x3c9c0)
                        .ok_or(invalid("reset current prefix unavailable"))?
                        .to_vec(),
                )?;
                let result = super::special::select_scan_raw(
                    &mut editor,
                    defaults.get(image),
                    suitable[0] as i32,
                    reset,
                );
                image[..0x3c9c0].copy_from_slice(editor.memory());
                result?;
            }
        }
    }
    let skip18 = q.as_bytes()[0x13] != 1;
    let mut saved = 0;
    for (kind, first, last) in TYPED_SLOTS {
        if matches!(kind, 0x19..=0x1b) || (kind == 0x18 && skip18) {
            continue;
        }
        let mut memopad_changed = false;
        for index in first..=last {
            let mut h = record::reset_holder(kind, index, 1);
            record_image::load_into(image, kind, index, &mut h)?;
            let mut f = record::reset_holder(kind, index, 2);
            record_image::load_into(defaults.get(image), kind, index, &mut f)?; // Even W.valid 0 reads F first.
            if !h.valid() {
                continue;
            }
            if typed_bad(image, &h, false, q, ranges, system_langid)? {
                match kind {
                    2..=4 => {
                        if matches!(kind, 2 | 3)
                            && h.as_bytes()[0x25] == 0
                            && h.as_bytes()[0x24] == 1
                        {
                            record_image::load_into(image, kind, index - 1, &mut h)?;
                            h.set_header(kind, index, 1);
                        } else {
                            h.copy_body_from(&f);
                        }
                        if kind == 2 {
                            let row = index / 2;
                            super::rows::release_into(
                                image,
                                (row / 100) as usize,
                                (row % 100) as usize..(row % 100 + 1) as usize,
                                defaults,
                                &mut state.home_invalidated,
                            )?;
                            h.set_valid(false);
                        }
                    }
                    5 => {
                        h.set_channel_field(record::ChannelField::FrequencyHz, u32::MAX);
                        memopad_changed = true;
                    }
                    8 => {
                        h.copy_body_from(&f);
                        h.set_valid(false);
                    }
                    0x15 | 0x18 => h.set_valid(false),
                    0x16 => h.copy_body_from(&f),
                    _ => return Err(invalid("operating reset typed kind unavailable")),
                }
            }
            record_image::store_into(image, kind, index, &mut h)?;
            record_image::load_into(image, kind, index, &mut h)?;
            saved += 1;
        }
        if memopad_changed {
            stage9_memopad(image, defaults, (1, 2), |_, _, _| Ok(()))?;
        }
    }
    Ok(ResetReport {
        typed_saved: saved,
        lists: reset_lists(image, defaults, q, ranges, system_langid)?,
    })
}

/// D1DBC: shift through the full 30-slot tail using the shared typed adapter;
/// type 24 is 8 spaces. There is no stored count or default-reference reset here.
pub fn delete_allowed_into(image: &mut [u8], index: u32) -> Result<(), Error> {
    for destination in (index as i32)..29 {
        let next = destination.wrapping_add(1) as u32;
        let mut holder = record::reset_holder(0x17, next, 1);
        record_image::load_into(image, 0x17, next, &mut holder)?;
        record_image::store_into(image, 0x17, destination as u32, &mut holder)?;
    }
    let tail = (index as i32).max(29) as u32;
    let mut holder = record::reset_holder(0x24, tail, 2);
    record_image::load_into(image, 0x24, tail, &mut holder)?;
    record_image::store_into(image, 0x17, tail, &mut holder)
}

/// 4F8EB..4FA84's actual 0C->0D->17 descending phases. Each predicate/load/save
/// reads current again. This is shared with the full operating reset root; it
/// does not replace that root's earlier Home/scan/typed correction work.
pub fn reset_lists(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    q: &super::context::Context,
    ranges: &super::frequency::Ranges,
    system_langid: Option<u16>,
) -> Result<ResetLists, Error> {
    use crate::repeater::{groups::Groups, predicate, working};
    let mut report = ResetLists::default();
    let delete = |image: &mut [u8], index: u32, holder: &DecodedRecord| -> Result<(), Error> {
        let groups = Groups::from_working(image)?;
        let group = groups.group_of(index as i32);
        let relative = index as usize - groups.start(group)? as usize;
        working::delete_into(
            image,
            group,
            relative,
            holder,
            |image| {
                let group = Groups::from_working(defaults.get(image))?.group_of(0) as u32;
                Ok(working::DefaultContext::from_context(q, system_langid)?.holder(group, 2))
            },
            |image, which| {
                let at = [0x3c8b9, 0x3c903][which];
                Ok(u16::from_le_bytes(
                    defaults
                        .get(image)
                        .get(at..at + 2)
                        .ok_or(invalid("reset F repeater word unavailable"))?
                        .try_into()
                        .unwrap(),
                ))
            },
        )
    };
    for index in (0..Groups::from_working(image)?.total()).rev() {
        let mut holder = record::reset_holder(0x0c, index, 1);
        record_image::load_into(image, 0x0c, index, &mut holder)?;
        let mode = holder.repeater_field(record::RepeaterField::Mode);
        if predicate::callsign_bad(image, &holder, ranges)? {
            if matches!(mode, 2 | 3) {
                delete(image, index, &holder)?;
                report.deleted_repeaters += 1;
                continue;
            }
            holder.as_mut_bytes()[0x160..0x168].fill(b' ');
        }
        let frequency = holder.channel_field(record::ChannelField::FrequencyHz);
        if frequency != 0
            && matches!(mode, 2 | 3)
            && (ranges.db98(frequency).ecdc(q, frequency)
                || (mode == 3 && ranges.db98(frequency).edf0(0x0c, frequency) != 3))
        {
            holder.set_channel_field(record::ChannelField::FrequencyHz, 0);
        }
        if predicate::frequency_bad(&holder, ranges, q) || predicate::duplex_bad(&holder) {
            delete(image, index, &holder)?;
            report.deleted_repeaters += 1;
        } else {
            record_image::store_into(image, 0x0c, index, &mut holder)?;
            record_image::load_into(image, 0x0c, index, &mut holder)?;
            report.saved_repeaters += 1;
        }
    }
    for index in (0..record_image::callsign_count(image, 0x0d)?).rev() {
        let mut holder = record::reset_holder(0x0d, index, 1);
        record_image::load_into(image, 0x0d, index, &mut holder)?;
        if record_image::callsign_bad(image, &holder)? {
            super::rows::delete_ur_into(image, index, defaults)?;
            report.deleted_ur += 1;
        }
    }
    for index in (0..record_image::callsign_count(image, 0x17)?).rev() {
        let mut holder = record::reset_holder(0x17, index, 1);
        record_image::load_into(image, 0x17, index, &mut holder)?;
        if record_image::callsign_bad(image, &holder)? {
            delete_allowed_into(image, index)?;
            report.deleted_allowed += 1;
        }
    }
    Ok(report)
}

/// Full default name-editor byte path for normal/group/GPS/repeater names.
/// Sanitation mutates the shared logical input before producing a new candidate.
/// NUL becomes space, not a suffix terminator. Trailing ASCII space and literal
/// 8140 pairs are removed; leading spaces remain. Width counts bytes, shortened
/// by one when its last byte would begin a DBCS pair. No Unicode mapping occurs.
pub fn normalize_name(shared: &mut [u8], width: usize) -> Vec<u8> {
    sanitize_fixed(shared, b' ');
    let mut end = shared.len();
    loop {
        if end != 0 && shared[end - 1] == b' ' {
            end -= 1;
        } else if end >= 2 && shared[end - 2..end] == [0x81, 0x40] {
            end -= 2;
        } else {
            break;
        }
    }
    let mut take = end.min(width);
    if end > width && width != 0 {
        let mut at = 0;
        while at < width {
            let size = if crate::csv::is_lead(shared[at]) {
                2
            } else {
                1
            };
            if at + size > width {
                take = at;
                break;
            }
            at += size;
        }
    }
    shared[..take].to_vec()
}

fn invalid(reason: &'static str) -> Error {
    Error::Invalid {
        line: 0,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel_image() -> Vec<u8> {
        let mut image = vec![0; 0x4997a];
        for cell in image[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
            cell.copy_from_slice(&0xffff_u32.to_le_bytes());
        }
        image[0x3fcd4..0x3fcd8].copy_from_slice(&3_u32.to_le_bytes());
        for (side, frequency, offset, mode) in [(0, 321_u32, 149_u32, 0x82), (1, 456, 150, 0x70)] {
            let at = 249 + 1 + 33 * side;
            image[at..at + 4].copy_from_slice(&frequency.to_le_bytes());
            image[at + 4] = mode;
            image[at + 5] = 1;
            image[at + 9..at + 11].fill(50);
            image[at + 29..at + 33].copy_from_slice(&offset.to_le_bytes());
        }
        image[0x3c9b0..0x3c9c0].fill(0xcc);
        image
    }

    fn channel_input(index: u32) -> ChannelNormalization {
        ChannelNormalization {
            kind: ChannelKind::Normal,
            index,
            working_binding: 17,
            default_binding: 29,
            q12: 0,
            previous_reload_guard: 0,
        }
    }

    #[test]
    fn actual_channel_prefix_reads_default_then_rounds_before_previous_reload_without_second_round()
    {
        let mut image = channel_image();
        let before = image.clone();
        let mut calls = 0;
        let ranges = FrequencyContext {
            common_kind: 0,
            l30: Some(&[]),
            l58: Some(&[]),
            l120: None,
        };
        let holder = normalize_channel_at(
            &mut image,
            ReferenceImage::Working,
            channel_input(1),
            |_, holder| {
                calls += 1;
                assert_eq!(holder.offset_hz(), 200);
                assert_eq!(holder.index(), 1);
                previous_channel_bad(holder, Some(&ranges))
            },
            |_| Ok(()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(
            (
                holder.index(),
                holder.binding(),
                holder.offset_hz(),
                holder.mode(),
                holder.data(),
                holder.filter()
            ),
            (1, 17, 149, 15, 0, 0)
        );
        assert_eq!(&image[249 + 34..249 + 38], &321_u32.to_le_bytes());
        assert_eq!(&image[249 + 34 + 29..249 + 67], &149_u32.to_le_bytes());
        assert_eq!(&image[249 + 1..249 + 34], &before[249 + 1..249 + 34]);
        assert_eq!(&image[0x3c9b0..0x3c9c0], &[0xcc; 16]);
        let mut bad_default = before.clone();
        bad_default[0x3fcd4..0x3fcd8].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut image = before.clone();
        assert!(
            normalize_channel_at(
                &mut image,
                ReferenceImage::Separate(&bad_default),
                channel_input(1),
                |_, _| panic!("default load must fail first"),
                |_| panic!("no store")
            )
            .is_err()
        );
        assert_eq!(image, before);
    }

    #[test]
    fn actual_channel_prefix_skips_invalid_and_preserves_sequential_raw_failure_prefix() {
        let mut image = channel_image();
        let before = image.clone();
        let ranges = FrequencyContext {
            common_kind: 0,
            l30: Some(&[]),
            l58: Some(&[]),
            l120: None,
        };
        assert!(
            normalize_channel_at(
                &mut image,
                ReferenceImage::Separate(&[]),
                channel_input(2),
                |_, _| panic!("invalid row"),
                |_| panic!("invalid row")
            )
            .unwrap()
            .is_none()
        );
        assert_eq!(image, before);
        assert!(
            normalize_channel_at(
                &mut image,
                ReferenceImage::Working,
                channel_input(1),
                |_, holder| previous_channel_bad(holder, Some(&ranges)),
                |write| {
                    if write.stage() == record::ChannelStage::Mode {
                        Err(invalid("synthetic late mode store failure"))
                    } else {
                        Ok(())
                    }
                }
            )
            .is_err()
        );
        let mut expected = before.clone();
        expected[249 + 34..249 + 38].copy_from_slice(&321_u32.to_le_bytes());
        assert_eq!(image, expected);
        // No previous-side query when the captured global guard is nonzero.
        let mut image = before;
        image[249 + 34 + 29..249 + 67].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut input = channel_input(1);
        input.previous_reload_guard = 1;
        let holder = normalize_channel_at(
            &mut image,
            ReferenceImage::Working,
            input,
            |_, _| panic!("guard skips query"),
            |_| Ok(()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            (holder.offset_hz(), holder.tx_tone(), holder.rx_tone()),
            (0, 24, 24)
        );
        assert_eq!(&image[0x3c9b0..0x3c9c0], &[0xcc; 16]);
    }

    #[test]
    fn actual_range_prefix_keeps_identity_endpoints_and_invalid_rows_skip_default_read() {
        let mut image = channel_image();
        for (kind, at, length) in [(0x15, 0x378c0, 8), (0x16, 0x3973a, 8), (0x18, 0x370e0, 10)] {
            image[at..at + length].fill(0);
            let base = if kind == 0x18 {
                image[at] = 255;
                image[at + 1] = 0xa7;
                at + 2
            } else {
                at
            };
            image[base..base + 4].copy_from_slice(&149_u32.to_le_bytes());
            image[base + 4..base + 8].copy_from_slice(&150_u32.to_le_bytes());
            let before = image.clone();
            let holder = normalize_range_at(
                &mut image,
                ReferenceImage::Working,
                kind,
                0,
                (17, 29),
                |_, _| Ok(()),
            )
            .unwrap()
            .unwrap();
            assert_eq!(&holder.as_bytes()[0x3c..0x40], &150_u32.to_le_bytes());
            assert_eq!(&holder.as_bytes()[0x138..0x13c], &149_u32.to_le_bytes());
            assert_eq!(image, before);
        }
        image[0x378c0..0x378c4].fill(0);
        let before = image.clone();
        assert!(
            normalize_range_at(
                &mut image,
                ReferenceImage::Separate(&[]),
                0x15,
                0,
                (17, 29),
                |_, _| panic!("invalid pair is not stored")
            )
            .unwrap()
            .is_none()
        );
        assert_eq!(image, before);
        assert!(
            normalize_range_at(
                &mut image,
                ReferenceImage::Separate(&[]),
                0x16,
                0,
                (17, 29),
                |_, _| panic!("default load must fail")
            )
            .is_err()
        );
        assert_eq!(image, before);
    }

    #[test]
    fn actual_eleven_slot_driver_counts_all_kinds_and_captures_type18_condition() {
        let mut source = channel_image();
        for row in 0..=10 {
            put_memopad(&mut source, row, u32::MAX, 0x82);
        }
        put_memopad(&mut source, 1, 123, 0x82);
        source[0x3c84a] = 1;
        source[0x3a8c9] = 1;
        source[0x3a8ca] = 1;
        source[0x3c84f..0x3c853].fill(0xff); // Kind 19 remains valid, unlike kind 04/05.
        for row in 0..12 {
            let at = 0x370e0 + 10 * row;
            source[at] = 1; // Seed valid type 18 rows, not the old always-valid 0 pair.
            source[at + 2..at + 6].copy_from_slice(&1_u32.to_le_bytes());
            source[at + 6..at + 10].copy_from_slice(&2_u32.to_le_bytes());
        }
        let mut q = super::super::context::Context::before_registry();
        for (q13, japanese, expected) in [
            (0, false, 159),
            (1, false, 171),
            (2, false, 159),
            (1, true, 159),
        ] {
            let mut fields = q.fields();
            fields.0[0] = if japanese { 0 } else { 1 };
            fields.0[11] = q13;
            q.adopt(fields);
            let ranges = super::super::frequency::Ranges::generate(&q, None).unwrap();
            let mut image = source.clone();
            let result =
                typed_loop(&mut image, ReferenceImage::Working, &q, &ranges, None, 1).unwrap();
            assert_eq!(
                result,
                TypedCounts {
                    saved: expected,
                    memopad_valid: 1
                }
            );
            assert_eq!(&image[0x3c84f..0x3c853], &u32::MAX.to_le_bytes());
            assert_eq!(&image[0x3c9b0..0x3c9c0], &[0xcc; 16]);
        }
    }

    fn put_memopad(image: &mut [u8], row: usize, frequency: u32, mode: u8) {
        let at = 0x36f54 + 36 * row;
        image[at..at + 36].fill(0);
        image[at..at + 4].copy_from_slice(&frequency.to_le_bytes());
        image[at + 4] = mode;
        image[at + 5] = 1;
        image[at + 7..at + 9].fill(50);
        image[at + 10..at + 14].copy_from_slice(&149_u32.to_le_bytes());
    }

    #[test]
    fn special_prefix_reads_live_default_before_rounding_and_keeps_scan_tones_separate() {
        let mut image = channel_image();
        put_memopad(&mut image, 1, 123, 0x82); // Full mode 0F: DATA clears, filter 0.
        let before = image.clone();
        assert!(
            normalize_special_at(
                &mut image,
                ReferenceImage::Separate(&[]),
                5,
                1,
                (17, 29),
                0,
                |_, _| panic!("F load first")
            )
            .is_err()
        );
        assert_eq!(image, before);
        let holder = normalize_special_at(
            &mut image,
            ReferenceImage::Working,
            5,
            1,
            (17, 29),
            0,
            |_, _| Ok(()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            (
                holder.offset_hz(),
                holder.mode(),
                holder.data(),
                holder.filter(),
                holder.tx_tone()
            ),
            (100, 15, 0, 0, 24)
        );
        put_memopad(&mut image, 2, u32::MAX, 0x82);
        assert!(
            normalize_special_at(
                &mut image,
                ReferenceImage::Separate(&[]),
                5,
                2,
                (17, 29),
                0,
                |_, _| panic!("invalid skip")
            )
            .unwrap()
            .is_none()
        );
        image[0xa368..0xa36c].copy_from_slice(&123_u32.to_le_bytes());
        image[0xa36c..0xa370].copy_from_slice(&149_u32.to_le_bytes());
        image[0xa370..0xa373].copy_from_slice(&[8, 1, 2]);
        let holder = normalize_special_at(
            &mut image,
            ReferenceImage::Working,
            8,
            0,
            (17, 29),
            0,
            |_, _| Ok(()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            (
                holder.mode(),
                holder.data(),
                holder.filter(),
                holder.tx_tone()
            ),
            (15, 0, 255, 8)
        );
        assert_eq!(&holder.as_bytes()[0x138..0x13c], &149_u32.to_le_bytes());
        assert_eq!(&image[0x3c9b0..0x3c9c0], &[0xcc; 16]);
    }

    #[test]
    fn memopad_compaction_uses_codec_and_retains_valid_shift_holder_at_hole_tail() {
        let mut image = channel_image();
        put_memopad(&mut image, 0, 777, 0x82);
        for row in 1..=10 {
            put_memopad(&mut image, row, (row * 10) as u32, 0x02);
        }
        let first = 0x36f54 + 36;
        image[first..first + 4].fill(0xff);
        let reserved = image[0x36f54..first].to_vec();
        let mut stores = Vec::new();
        stage9_memopad(
            &mut image,
            ReferenceImage::Separate(&[]),
            (17, 29),
            |row, at, _| {
                stores.push((row, at));
                Ok(())
            },
        )
        .unwrap();
        for row in 1..=10 {
            let at = 0x36f54 + 36 * row;
            let expected = if row < 10 {
                ((row + 1) * 10) as u32
            } else {
                u32::MAX
            };
            assert_eq!(&image[at..at + 4], &expected.to_le_bytes());
        }
        assert_eq!(&image[0x36f54..first], reserved);
        assert_eq!(stores.len(), 20); // Nine shifts and a valid tail holder: two stores each.
        assert_eq!(&stores[18..], &[(10, 0), (10, 4)]);
        // No stage9 mode or 100 Hz rounding is smuggled into the compactor.
        assert_eq!(&image[first + 10..first + 14], &149_u32.to_le_bytes());
        assert_eq!(&image[0x3c9b0..0x3c9c0], &[0xcc; 16]);
    }

    #[test]
    fn all_hole_memopad_restores_live_f_per_row_and_late_source_failure_keeps_prefix() {
        let mut image = channel_image();
        for row in 0..=10 {
            put_memopad(&mut image, row, u32::MAX, 0x82);
        }
        let before = image.clone();
        stage9_memopad(&mut image, ReferenceImage::Working, (17, 29), |_, _, _| {
            Ok(())
        })
        .unwrap();
        assert_eq!(image, before); // F=W remains the already compacted live source.
        let mut defaults = before.clone();
        for row in 1..=10 {
            put_memopad(&mut defaults, row, (100 + row) as u32, 0x82);
        }
        let mut image = before.clone();
        assert!(
            stage9_memopad(
                &mut image,
                ReferenceImage::Separate(&defaults[..0x36f54 + 36 * 5]),
                (17, 29),
                |_, _, _| Ok(())
            )
            .is_err()
        );
        for row in 1..=4 {
            let at = 0x36f54 + 36 * row;
            assert_eq!(&image[at..at + 4], &(100 + row as u32).to_le_bytes());
            assert_eq!(&image[at + 10..at + 14], &149_u32.to_le_bytes());
        }
        assert_eq!(
            &image[0x36f54..0x36f54 + 36],
            &before[0x36f54..0x36f54 + 36]
        );
        assert_eq!(&image[0x36f54 + 36 * 5..], &before[0x36f54 + 36 * 5..]);
    }

    #[test]
    fn channel_predicate_corrected_four_cases_full_mode_and_call_descriptor_range_are_independent()
    {
        let hit = [FrequencyRange {
            low: 30,
            high: 50,
            flag: 0,
        }];
        let miss = [FrequencyRange {
            low: 60,
            high: 70,
            flag: 1,
        }];
        let mut holder = record::reset_holder(2, 1, 17);
        holder.as_mut_bytes()[0x3c..0x40].copy_from_slice(&42_u32.to_le_bytes());
        for (h30, h58, expected) in [
            (false, false, true),
            (false, true, true),
            (true, false, true),
            (true, true, false),
        ] {
            let context = FrequencyContext {
                common_kind: 0,
                l30: Some(if h30 { &hit } else { &miss }),
                l58: Some(if h58 { &hit } else { &miss }),
                l120: None,
            };
            assert_eq!(
                previous_channel_bad(&holder, Some(&context)).unwrap(),
                expected
            );
        }
        let missing = FrequencyContext {
            common_kind: 0,
            l30: Some(&miss),
            l58: None,
            l120: None,
        };
        assert!(previous_channel_bad(&holder, Some(&missing)).is_err());
        let unrestricted = FrequencyContext {
            common_kind: 16,
            l30: None,
            l58: None,
            l120: None,
        };
        assert!(!previous_channel_bad(&holder, Some(&unrestricted)).unwrap());
        assert!(previous_channel_bad(&holder, None).is_err());
        holder.set_mode(0x118); // Low byte 24 is not full mode 24.
        assert!(!previous_channel_bad(&holder, Some(&unrestricted)).unwrap());
        holder.set_mode(24);
        assert!(previous_channel_bad(&holder, Some(&unrestricted)).unwrap());
        holder.set_valid(false);
        assert!(!previous_channel_bad(&holder, None).unwrap());
        let mut image = channel_image();
        let at = 0xa21c + 83 * 3 + 34;
        image[at..at + 4].copy_from_slice(&42_u32.to_le_bytes());
        record_image::load_channel_into(&image, ChannelKind::Call, 7, &mut holder).unwrap();
        let mut additional = [miss[0]; 14];
        additional[13] = hit[0];
        let context = FrequencyContext {
            common_kind: 0,
            l30: Some(&hit),
            l58: Some(&hit),
            l120: Some(&additional),
        };
        assert!(!previous_channel_bad(&holder, Some(&context)).unwrap());
        let missing = FrequencyContext {
            common_kind: 0,
            l30: Some(&miss),
            l58: Some(&miss),
            l120: None,
        };
        assert!(previous_channel_bad(&holder, Some(&missing)).is_err()); // Extra call read is mandatory even after shared bad.
    }

    #[test]
    fn late_t_callsign_predicate_checks_whole_live_list_exact_duplicates_and_blank_without_order() {
        let mut current = channel_image();
        current[0x49978..0x4997a].copy_from_slice(&3_u16.to_le_bytes());
        for (index, call) in [*b"AA      ", *b"BB      ", *b"aa      "]
            .into_iter()
            .enumerate()
        {
            current[0x2eda3 + 8 * index..0x2edab + 8 * index].copy_from_slice(&call);
        }
        current[0x3c0a7..0x3c2ff].fill(0xff); // Stored order is not consulted.
        assert!(!late_callsign_list_bad(&current, 17).unwrap());
        current[0x2edb3..0x2edbb].copy_from_slice(b"AA      ");
        assert!(late_callsign_list_bad(&current, 17).unwrap()); // A later row, not only selected 0.
        current[0x49978..0x4997a].copy_from_slice(&2_u16.to_le_bytes());
        assert!(!late_callsign_list_bad(&current, 17).unwrap()); // Tail duplicate is outside live T count.
        current[0x2edab..0x2edb3].fill(b' ');
        assert!(late_callsign_list_bad(&current, 17).unwrap());
        current[0x49978..0x4997a].fill(0);
        assert!(!late_callsign_list_bad(&current, 17).unwrap());
        assert!(late_callsign_list_bad(&current[..0x49979], 17).is_err());
    }

    #[test]
    fn stage7_keeps_stale_previous_high_and_can_merge_into_an_invalid_output() {
        let mut image = vec![0xa6; 0x3c9c0];
        let range = |low, high| FrequencyRange { low, high, flag: 1 };
        stage7(&mut image, &[range(10, 20), range(21, 30), range(31, 40)]).unwrap();
        assert_eq!(
            &image[0x378c0..0x378d0],
            &[10, 0, 0, 0, 30, 0, 0, 0, 31, 0, 0, 0, 40, 0, 0, 0]
        );
        assert!(image[0x378d0..0x379b0].iter().all(|&b| b == 0));
        assert!(
            image[..0x378c0]
                .iter()
                .chain(&image[0x379b0..])
                .all(|&b| b == 0xa6)
        );
        stage7(
            &mut image,
            &[range(10, 20), range(u32::MAX, 30), range(21, 25)],
        )
        .unwrap();
        assert_eq!(
            &image[0x378c0..0x378d0],
            &[10, 0, 0, 0, 20, 0, 0, 0, 0, 0, 0, 0, 25, 0, 0, 0]
        );
        let mut calls = Vec::new();
        stage7_live(
            &mut image,
            || Ok(1),
            |i| {
                calls.push(i);
                Ok(if calls.len() == 1 {
                    range(80, 99)
                } else {
                    range(7, 3)
                })
            },
        )
        .unwrap();
        assert_eq!(calls, [0, 0]);
        assert_eq!(&image[0x378c0..0x378c8], &[80, 0, 0, 0, 3, 0, 0, 0]);
    }

    #[test]
    fn stage7_failure_prefix_and_mode_widths_do_not_silently_rollback_or_clamp() {
        let mut image = vec![0xa6; 0x3c9c0];
        let mut calls = 0;
        assert!(
            stage7_live(
                &mut image,
                || Ok(2),
                |_| {
                    calls += 1;
                    if calls == 3 {
                        Err(invalid("synthetic range read failure"))
                    } else {
                        Ok(FrequencyRange {
                            low: 10,
                            high: 20,
                            flag: 0,
                        })
                    }
                }
            )
            .is_err()
        );
        assert_eq!(&image[0x378c0..0x378c8], &[10, 0, 0, 0, 20, 0, 0, 0]);
        assert_eq!(&image[0x378c8..0x378d0], &[0xa6; 8]);
        for (value, expected) in [
            (149, 100),
            (150, 200),
            (u32::MAX, 0),
            (9_999_950, 10_000_000),
        ] {
            assert_eq!(round_offset_hz(value), expected);
        }
        let mut values = ChannelValues {
            mode: 0x10f,
            data: 7,
            filter: 2,
            offset_hz: 150,
            tx_tone: 50,
            rx_tone: 49,
        };
        normalize_channel_values(&mut values, false, true);
        assert_eq!(
            values,
            ChannelValues {
                mode: 0x10f,
                data: 0,
                filter: 0,
                offset_hz: 200,
                tx_tone: 24,
                rx_tone: 49
            }
        );
        values.mode = 0x11;
        values.filter = 2;
        values.tx_tone = 50;
        normalize_channel_values(&mut values, true, true);
        assert_eq!((values.filter, values.tx_tone), (255, 50));
    }

    #[test]
    fn fixed_byte_helpers_bound_every_read_and_preserve_nul_suffix_length() {
        let mut field = *b"A\0B\x82\xa0C\x81!D\xe0";
        sanitize_fixed(&mut field, b' ');
        assert_eq!(field, *b"A B\x82\xa0C  D ");
        remove_dbcs_fixed(&mut field, b' ');
        assert_eq!(field, *b"A B  C  D ");
        let mut compact = *b"A\0B\x82\xa0C";
        remove_dbcs_fixed(&mut compact, 0);
        assert_eq!(compact, *b"ABC   ");
        let mut image = vec![0xa6; 0x3c9c0];
        for at in [0x32bb5, 0x32bc9, 0x32bdd, 0x32bf1, 0x32c05, 0x3c628] {
            image[at..at + 20].fill(0);
            image[at..at + 7].copy_from_slice(b"A\0B\x82\xa0CZ");
        }
        let before = image.clone();
        stage9_fixed_text(&mut image).unwrap();
        for (at, (&old, &new)) in before.iter().zip(&image).enumerate() {
            let inside = (0x32bb5..0x32c19).contains(&at) || (0x3c628..0x3c63c).contains(&at);
            if !inside {
                assert_eq!(old, new);
            }
        }
        assert_eq!(&image[0x32bb5..0x32bc9], b"A B  CZ             ");
    }

    #[test]
    fn name_sanitation_precedes_trim_and_preserves_complete_multibyte_boundaries() {
        let mut text = b"  A\0B\x81\x40  ".to_vec();
        assert_eq!(normalize_name(&mut text, 16), b"  A B");
        assert_eq!(text, b"  A B\x81\x40  ");
        let mut crosses = [b'A'; 17];
        crosses[15..17].copy_from_slice(&[0x82, 0xa0]);
        assert_eq!(normalize_name(&mut crosses, 16), [b'A'; 15]);
        let mut ends_with_pair = [b'A'; 17];
        ends_with_pair[14..16].copy_from_slice(&[0x81, 0x82]);
        let candidate = normalize_name(&mut ends_with_pair, 16);
        assert_eq!(candidate.len(), 16);
        assert_eq!(&candidate[14..], &[0x81, 0x82]);
        let mut orphan = [b'A'; 16];
        orphan[15] = 0x82;
        assert_eq!(normalize_name(&mut orphan, 16), [b'A'; 15]);
        assert_eq!(orphan[15], b' ');
        let mut empty = [0, 1, b' '];
        assert!(normalize_name(&mut empty, 16).is_empty());
    }

    #[test]
    fn keyer_fixed_all_bytes_and_live_current_star_order_are_distinct() {
        let mut bytes: Vec<u8> = (0..=255).collect();
        normalize_keyer_fixed(&mut bytes, false);
        for (source, &actual) in (0_u8..=255).zip(&bytes) {
            let upper = match source {
                b'a'..=b'z' => source - 32,
                other => other,
            };
            let expected = if matches!(upper,b'A'..=b'Z'|b'0'..=b'9'|b' '|b'*'|b','|b'.'|b'/'|b'?'|b'@'|b'^')
            {
                upper
            } else {
                b' '
            };
            assert_eq!(actual, expected);
        }
        let mut bytes = *b"a*\0*b";
        normalize_keyer_fixed(&mut bytes, false);
        assert_eq!(bytes, *b"A*  B");
        normalize_keyer_fixed(&mut bytes, true);
        assert_eq!(bytes, *b"A   B");
        let mut working = vec![b' '; 0x3c9c0];
        working[0x37158..0x3715d].copy_from_slice(b"a\0*?*");
        working[0x37158 + 70] = b'*';
        let mut alias = working.clone();
        stage9_keyer(&mut alias, ReferenceImage::Working).unwrap();
        assert_eq!(&alias[0x37158..0x3715d], b"A *? ");
        assert_eq!(alias[0x37158 + 70], b' ');
        let current = vec![b' '; 0x37388];
        stage9_keyer(&mut working, ReferenceImage::Separate(&current)).unwrap();
        assert_eq!(working[0x37158 + 70], b'*');
        let mut partial = vec![b' '; 0x3c9c0];
        partial[0x37158] = b'a';
        assert!(stage9_keyer(&mut partial, ReferenceImage::Separate(&[])).is_err());
        assert_eq!(partial[0x37158], b'A');
    }

    #[test]
    fn central_control_bit_block_preserves_all_other_bits_and_late_failure_prefix() {
        let mut image = vec![0xa6; 0x3c9c0];
        let mut defaults = image.clone();
        defaults[0x37780] = 0x59;
        defaults[0x37781] = 0xf3;
        defaults[0x377fe] = 0x17;
        for byte in 0_u8..=255 {
            image[0x37780] = byte;
            image[0x37781] = byte;
            stage9_control_bits(&mut image, ReferenceImage::Separate(&defaults)).unwrap();
            assert_eq!(image[0x37780], (byte & 0x26) | 0x59);
            assert_eq!(image[0x37781], (byte & 0xf0) | 3);
            assert_eq!(image[0x377fe], 0x17);
        }
        let before = image.clone();
        stage9_control_bits(&mut image, ReferenceImage::Working).unwrap();
        assert_eq!(image, before);
        image.fill(0xa6);
        assert!(
            stage9_control_bits(&mut image, ReferenceImage::Separate(&defaults[..0x37782]))
                .is_err()
        );
        let mut expected = vec![0xa6; 0x3c9c0];
        expected[0x37780] = 0x7f;
        expected[0x37781] = 0xa3;
        assert_eq!(image, expected);
    }

    #[test]
    fn central_filter_bits_preserve_masks_pass_order_and_nonidentity_working_alias() {
        let mut context = super::super::context::Context::before_registry();
        context.adopt(crate::clone::ContextFields([
            5, 1, 0, 0, 0, 0, 0, 0x80, 0, 0, 1, 0,
        ]));
        let defaults = vec![0x51; 0x4997a];
        for value in 0..=255_u8 {
            let mut image = vec![value; 0x4997a];
            stage9_filter_bits(&mut image, ReferenceImage::Working, &context).unwrap();
            for i in 0..17 {
                assert_eq!(
                    image[0x378af + i],
                    if i < 13 { value | 8 } else { (value & !4) | 8 }
                );
            }
            let mut image = vec![value; 0x4997a];
            let mut expected = image.clone();
            expected[0x378b9..0x378be].fill((value & !7) | 1);
            for byte in &mut expected[0x378bc..0x378c0] {
                *byte &= !4;
            }
            for byte in &mut expected[0x378af..0x378c0] {
                *byte |= 8;
            }
            stage9_filter_bits(&mut image, ReferenceImage::Separate(&defaults), &context).unwrap();
            assert_eq!(image, expected);
        }
        let mut image = vec![0xa6; 0x4997a];
        assert!(
            stage9_filter_bits(
                &mut image,
                ReferenceImage::Separate(&defaults[..0x378bb]),
                &context
            )
            .is_err()
        );
        let mut expected = vec![0xa6; 0x4997a];
        expected[0x378b9] = 0xa5;
        expected[0x378ba] = 0xa5;
        assert_eq!(image, expected); // Low 2 pass failed BEFORE any bit 2 or unconditional loop.
    }

    #[test]
    fn central_settings_tail_uses_real_q_captured_q13_and_exact_independent_windows() {
        let mut context = super::super::context::Context::before_registry();
        context.adopt(crate::clone::ContextFields([
            5, 1, 0, 0, 0, 0, 0, 0x80, 0, 0, 1, 0,
        ]));
        let mut image = vec![0xa6; 0x4997a];
        image[0x37d89] = 0;
        image[0x37da5] = 0;
        image[0x3970a..0x3971a].fill(0x18);
        let defaults = vec![0x59; 0x4997a];
        let mut expected = image.clone();
        for (at, len) in [
            (0x377ef, 10),
            (0x379b0, 3),
            (0x379cd, 10),
            (0x37ad5, 125),
            (0x37b85, 512),
            (0x37d85, 1),
            (0x37da6, 5),
            (0x37dab, 3),
            (0x39312, 1),
            (0x3970a, 16),
            (0x3987a, 160),
            (0x3996b, 1),
            (0x399b7, 2),
            (0x399bf, 1),
        ] {
            expected[at..at + len].copy_from_slice(&defaults[at..at + len]);
        }
        expected[0x37781] &= !0x20;
        expected[0x399a0] = 0;
        stage9_settings_tail(
            &mut image,
            ReferenceImage::Separate(&defaults),
            &context,
            0,
            None,
        )
        .unwrap();
        assert_eq!(image, expected);
        // q13 is captured, not silently re-read from the current Q byte.
        context.adopt(crate::clone::ContextFields([
            0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1, 0,
        ]));
        image[0x39312] = 1;
        stage9_settings_tail(&mut image, ReferenceImage::Working, &context, 1, None).unwrap();
        assert_eq!(image[0x39312], 2);
        image[0x39313] = 2;
        image[0x39314] = 0;
        let before = image.clone();
        stage9_settings_tail(&mut image, ReferenceImage::Working, &context, 0, None).unwrap();
        assert_eq!(image, before);
        // kind 16 needs the actual SYSTEM LANGID only on captured-q13==1.
        let context = super::super::context::Context::before_registry();
        assert!(
            stage9_settings_tail(
                &mut image,
                ReferenceImage::Separate(&defaults),
                &context,
                1,
                None
            )
            .is_err()
        );
        assert_eq!(image[0x379b0], 0x59); // Earlier writes were not rolled back.
    }

    #[test]
    fn profile_prefix_uses_real_context_preset_predicate_and_live_copy_failure_order() {
        let mut image = vec![0xa6; 0x4997a];
        image[0xa78c] = 7;
        let mut defaults = vec![0x59; 0x4997a];
        defaults[0x355a1] = 6;
        defaults[0xa78c] = 3;
        let mut context = super::super::context::Context::before_registry();
        context.adopt(crate::clone::ContextFields([
            5, 1, 0, 0, 0, 0, 0, 0x80, 0, 0, 1, 0,
        ]));
        let before = image.clone();
        stage9_profile_prefix(&mut image, ReferenceImage::Separate(&defaults), &context).unwrap();
        let writes = [
            (0x355a1, 1),
            (0xa78c, 1),
            (0x36cb7, 108),
            (0x36f48, 1),
            (0x36d23, 108),
            (0x36d8f, 108),
            (0x36f49, 1),
            (0x36f4a, 1),
            (0x36ed3, 108),
            (0x36f4d, 1),
        ];
        let mut expected = before.clone();
        for (at, size) in writes {
            expected[at..at + size].copy_from_slice(&defaults[at..at + size]);
        }
        assert_eq!(image, expected);
        let before = image.clone();
        stage9_profile_prefix(&mut image, ReferenceImage::Working, &context).unwrap();
        assert_eq!(image, before);
        let mut image = vec![0xa6; 0x4997a];
        image[0xa78c] = 7;
        assert!(
            stage9_profile_prefix(
                &mut image,
                ReferenceImage::Separate(&defaults[..0x36f49]),
                &context
            )
            .is_err()
        );
        let mut expected = vec![0xa6; 0x4997a];
        expected[0xa78c] = 7;
        for (at, size) in &writes[..6] {
            expected[*at..at + size].copy_from_slice(&defaults[*at..at + size]);
        }
        assert_eq!(image, expected);
        let mut image = vec![0; 0x4997a];
        image[0xa78c] = 0;
        stage9_profile_prefix(
            &mut image,
            ReferenceImage::Separate(&defaults),
            &super::super::context::Context::before_registry(),
        )
        .unwrap();
        assert_eq!(image[0xa78c], 0); // Real selected/compatible mask 00 field keeps the current preset.
        image[0xa6dc..0xa6dc + 22].fill(0xff);
        stage9_profile_prefix(
            &mut image,
            ReferenceImage::Separate(&defaults),
            &super::super::context::Context::before_registry(),
        )
        .unwrap();
        assert_eq!(image[0xa78c], 3);
    }
}
