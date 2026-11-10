//! C96A8/C9744/C9898 and 625F8's type 0C all-group path. True means bad,
//! not a suitable/used row. B generation and live Q are separate explicit inputs.

use super::{groups::Groups, staging::classify_bytes, working};
use crate::icf::{
    Error,
    context::Context,
    frequency::Ranges,
    record::{self, ChannelField as C, DecodedRecord, RepeaterField as R, RepeaterText as T},
};

/// C96A8: current row participates in the first-match search; only an exact
/// first-index match is good. DV/DD allow final space/A aliases, FM does not.
/// The native end argument truncates index+1 to 16 bits, not a count clamp.
pub fn callsign_bad(image: &[u8], holder: &DecodedRecord, ranges: &Ranges) -> Result<bool, Error> {
    let call = holder.repeater_text(T::Call);
    if *call == [b' '; 8] {
        return Ok(false);
    }
    let mode = holder.repeater_field(R::Mode);
    let frequency_class = |hz| if hz == 0 { 4 } else { ranges.edf0(0x0c, hz) };
    let class = frequency_class(holder.channel_field(C::FrequencyHz));
    let mut candidate = record::reset_holder(0x0c, 0, holder.binding());
    let mut found = u16::MAX;
    for index in 0..holder.index().wrapping_add(1) as u16 {
        let at = 0xaa0f + 52 * usize::from(index) + 10;
        let flags = *image.get(at).ok_or(Error::Invalid {
            line: 0,
            reason: "repeater callsign search outside image",
        })?;
        let eligible = match mode {
            2 => flags >> 6 == 0,
            3 => flags >> 4 == 15,
            _ => matches!(flags >> 6, 1 | 2),
        };
        if !eligible {
            continue;
        }
        working::load_into(image, usize::from(index), &mut candidate)?;
        let other = candidate.repeater_text(T::Call);
        if mode == 2 && *other == [b' '; 8] {
            continue;
        }
        let same = if matches!(mode, 2 | 3) {
            call[..7] == other[..7]
                && (call[7] == other[7]
                    || (matches!(call[7], b' ' | b'A') && matches!(other[7], b' ' | b'A')))
        } else {
            call == other && class == frequency_class(candidate.channel_field(C::FrequencyHz))
        };
        if same {
            found = index;
            break;
        }
    }
    Ok(u32::from(found) != holder.index())
}

/// C9744: valid gate, ECDC, then K-specific zero/class checks. EDF0 reads the
/// holder's actual kind, rather than substituting a fixed profile or band map.
pub fn frequency_bad(holder: &DecodedRecord, ranges: &Ranges, q: &Context) -> bool {
    if !holder.valid() {
        return false;
    }
    let hz = holder.channel_field(C::FrequencyHz);
    if ranges.db98(hz).ecdc(q, hz) {
        return true;
    }
    let k = classify_bytes(holder.as_bytes());
    let class = ranges.db98(hz).edf0(holder.kind(), hz);
    match k {
        1..=3 => hz == 0,
        4 => hz != 0 && class != 3,
        5 => hz == 0 || class != 3,
        _ => false,
    }
}
/// C9898 has just one true branch; K0/DUP3 is not an extra error.
pub fn duplex_bad(holder: &DecodedRecord) -> bool {
    classify_bytes(holder.as_bytes()) == 1 && holder.channel_field(C::Duplex) != 0
}
/// C9800, independent of Q/B: zero is allowed and endpoints are inclusive.
pub fn numeric_frequency_bad(holder: &DecodedRecord) -> bool {
    let hz = holder.channel_field(C::FrequencyHz);
    holder.valid() && hz != 0 && !(30_000..=1_399_999_999).contains(&hz)
}
/// 61DC0: arg 0 short-circuits callsign -> frequency -> DUP; arg 1 only C9800.
pub fn row_bad(
    image: &[u8],
    holder: &DecodedRecord,
    numeric_only: bool,
    ranges: &Ranges,
    q: &Context,
) -> Result<bool, Error> {
    if numeric_only {
        return Ok(numeric_frequency_bad(holder));
    }
    Ok(callsign_bad(image, holder, ranges)?
        || frequency_bad(holder, ranges, q)
        || duplex_bad(holder))
}
/// 630EC/625F8 countMode 0: a full absolute group view, not the selected row.
pub fn group_bad(
    image: &[u8],
    group: usize,
    numeric_only: bool,
    ranges: &Ranges,
    q: &Context,
    binding: u32,
) -> Result<bool, Error> {
    let groups = Groups::from_working(image)?;
    let start = groups.start(group)? as usize;
    let mut holder = record::reset_holder(0x0c, 0, binding);
    for index in start..start + usize::from(groups.counts[group]) {
        working::load_into(image, index, &mut holder)?;
        if row_bad(image, &holder, numeric_only, ranges, q)? {
            return Ok(true);
        }
    }
    Ok(false)
}
/// R_pre 4E858..4E8AC: each of all 50 group views runs arg 0, then arg 1 only
/// on false. This is a read-only predicate, not the later normalization pass.
pub fn all_groups_bad(
    image: &[u8],
    ranges: &Ranges,
    q: &Context,
    binding: u32,
) -> Result<bool, Error> {
    for group in 0..50 {
        if group_bad(image, group, false, ranges, q, binding)?
            || group_bad(image, group, true, ranges, q, binding)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// C6F24: true is Home-usable, unlike row_bad's exclusion polarity. Keep the
/// EB98 L30 test even when live Q makes EB64 bypass its L30/L58 intersection.
pub fn home_usable(
    image: &[u8],
    holder: &DecodedRecord,
    ranges: &Ranges,
    q: &Context,
) -> Result<bool, Error> {
    let k = classify_bytes(holder.as_bytes());
    let hz = holder.channel_field(C::FrequencyHz);
    if hz == 0
        || row_bad(image, holder, false, ranges, q)?
        || ranges.db98(hz).eb64(q, hz)
        || !ranges.db98(hz).eb98(hz)
    {
        return Ok(false);
    }
    Ok(!matches!(k, 0 | 2 | 4) || holder.channel_field(C::Duplex) != 0)
}

/// C6FA4 counts suitable rows, not a group's stored/used count. The explicit
/// image is the caller's I generation; it is not replaced with global current.
pub fn home_usable_count(
    image: &[u8],
    group: usize,
    ranges: &Ranges,
    q: &Context,
    binding: u32,
) -> Result<u32, Error> {
    let groups = Groups::from_working(image)?;
    groups.start(group)?;
    let count = groups.counts[group];
    let mut holder = record::reset_holder(0x0c, 0, binding);
    let mut usable = 0;
    for relative in 0..count {
        let start = Groups::from_working(image)?.start(group)?;
        working::load_into(
            image,
            start.wrapping_add(u32::from(relative)) as usize,
            &mut holder,
        )?;
        if home_usable(image, &holder, ranges, q)? {
            usable += 1;
        }
    }
    Ok(usable)
}

/// C705C: only count 0 skips the load. C6760 adds the signed relative DWORD
/// with wrap; do not introduce relative<count, or correct the caller's separate
/// typed/list numeric-filter coordinate choice here.
pub fn home_row_usable(
    image: &[u8],
    group: usize,
    relative: i32,
    ranges: &Ranges,
    q: &Context,
    binding: u32,
) -> Result<bool, Error> {
    let groups = Groups::from_working(image)?;
    let start = groups.start(group)?;
    if groups.counts[group] == 0 {
        return Ok(false);
    }
    let mut holder = record::reset_holder(0x0c, 0, binding);
    working::load_into(
        image,
        start.wrapping_add(relative as u32) as usize,
        &mut holder,
    )?;
    home_usable(image, &holder, ranges, q)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn home_polarity_membership_live_q_and_dup_use_the_shared_engines() {
        let mut q = Context::before_registry();
        let mut fields = q.fields();
        fields.0[0] = 0;
        q.adopt(fields);
        let ranges = Ranges::generate(&q, None).unwrap();
        let mut wide = q;
        let mut fields = wide.fields();
        fields.0[0] = 16;
        wide.adopt(fields);
        let mut holder = record::reset_holder(0x0c, 0, 0);
        holder.set_valid(false);
        holder.set_repeater_text(T::Call, b"        ");
        // Zero skips the image/source reads. Native C6F24 has no added valid
        // guard; its shared bad predicates and K/DUP condition decide instead.
        assert!(!home_usable(&[], &holder, &ranges, &q).unwrap());
        holder.set_channel_field(C::FrequencyHz, 100_000_000);
        assert!(!home_usable(&[], &holder, &ranges, &q).unwrap());
        holder.set_channel_field(C::Duplex, 1);
        assert!(home_usable(&[], &holder, &ranges, &q).unwrap());
        holder.set_channel_field(C::FrequencyHz, 150_000_000);
        assert!(ranges.eb98(150_000_000));
        assert!(ranges.eb64(&q, 150_000_000));
        assert!(!home_usable(&[], &holder, &ranges, &q).unwrap());
        assert!(home_usable(&[], &holder, &ranges, &wide).unwrap());
        holder.set_channel_field(C::FrequencyHz, 300_000_000);
        assert!(!ranges.eb64(&wide, 300_000_000));
        assert!(!ranges.eb98(300_000_000));
        assert!(!home_usable(&[], &holder, &ranges, &wide).unwrap());
        let mut image = vec![0; 0x4997a];
        image[0x49914..0x49916].copy_from_slice(&1_u16.to_le_bytes());
        holder.set_valid(true);
        holder.set_repeater_field(R::Mode, 2);
        holder.set_repeater_text(T::Call, b"TEST   B");
        holder.set_channel_field(C::FrequencyHz, 100_000_000);
        for dup in [0, 1] {
            holder.set_channel_field(C::Duplex, dup);
            working::save_into(&mut image, 0, &holder).unwrap();
            assert_eq!(home_usable(&image, &holder, &ranges, &q).unwrap(), dup != 0);
        }
    }
    #[test]
    fn home_count_and_relative_row_keep_empty_and_cross_group_native_boundaries() {
        let mut image = vec![0; 0x4997a];
        for (group, count) in [(0, 1_u16), (3, 2), (4, 1)] {
            image[0x49914 + 2 * group..0x49916 + 2 * group].copy_from_slice(&count.to_le_bytes());
        }
        let q = Context::before_registry();
        let ranges = Ranges::generate(&q, Some(0x411)).unwrap();
        let mut holder = record::reset_holder(0x0c, 0, 0);
        holder.set_valid(true);
        holder.set_repeater_field(R::Mode, 0);
        holder.set_repeater_text(T::Call, b"        ");
        for (index, hz) in [(0, 0), (1, 100_000_000), (2, 0), (3, 145_000_000)] {
            holder.set_channel_field(C::FrequencyHz, hz);
            working::save_into(&mut image, index, &holder).unwrap();
        }
        let before = image.clone();
        assert_eq!(
            home_usable_count(&image, 3, &ranges, &q, 0x1234).unwrap(),
            1
        );
        assert!(!home_row_usable(&image, 1, i32::MAX, &ranges, &q, 0).unwrap());
        assert!(home_row_usable(&image, 3, 0, &ranges, &q, 0).unwrap());
        assert!(!home_row_usable(&image, 3, 1, &ranges, &q, 0).unwrap());
        assert!(home_row_usable(&image, 3, 2, &ranges, &q, 0).unwrap());
        assert!(!home_row_usable(&image, 3, -1, &ranges, &q, 0).unwrap());
        assert!(home_row_usable(&image, 3, i32::MAX, &ranges, &q, 0).is_err());
        assert_eq!(image, before);
    }
    #[test]
    fn full_group_search_alias_class_zero_and_numeric_pass_are_distinct() {
        let mut image = vec![0; 0x4997a];
        image[0x49914 + 98..0x49914 + 100].copy_from_slice(&2_u16.to_le_bytes());
        let q = Context::before_registry();
        let ranges = Ranges::generate(&q, Some(0x411)).unwrap();
        let mut holder = record::reset_holder(0x0c, 0, 0x1234);
        holder.set_valid(true);
        holder.set_repeater_field(R::Mode, 2);
        holder.set_repeater_text(T::Call, b"TEST   A");
        working::save_into(&mut image, 0, &holder).unwrap();
        holder.set_repeater_text(T::Call, b"TEST    ");
        working::save_into(&mut image, 1, &holder).unwrap();
        working::load_into(&image, 1, &mut holder).unwrap();
        assert!(callsign_bad(&image, &holder, &ranges).unwrap());
        assert!(!group_bad(&image, 0, false, &ranges, &q, 0).unwrap());
        assert!(all_groups_bad(&image, &ranges, &q, 0).unwrap());
        holder.set_repeater_text(T::Call, b"        ");
        holder.set_repeater_field(R::Mode, 0);
        holder.set_channel_field(C::Duplex, 1);
        assert!(frequency_bad(&holder, &ranges, &q));
        holder.set_channel_field(C::FrequencyHz, 145_000_000);
        assert!(!frequency_bad(&holder, &ranges, &q));
        assert!(!duplex_bad(&holder));
        for hz in [0, 30_000, 1_399_999_999] {
            holder.set_channel_field(C::FrequencyHz, hz);
            assert!(!numeric_frequency_bad(&holder));
        }
        holder.set_channel_field(C::FrequencyHz, 1_400_000_000);
        assert!(numeric_frequency_bad(&holder));
        holder.set_valid(false);
        assert!(!numeric_frequency_bad(&holder));
        assert!(!frequency_bad(&holder, &ranges, &q));
        image[0x49914 + 98..0x49914 + 100].copy_from_slice(&1_u16.to_le_bytes());
        holder.set_valid(true);
        holder.set_repeater_field(R::Mode, 2);
        holder.set_repeater_text(T::Call, b"NUMERIC ");
        working::save_into(&mut image, 0, &holder).unwrap();
        let mut wide = q;
        let mut fields = wide.fields();
        fields.0[0] = 16;
        wide.adopt(fields);
        assert!(!group_bad(&image, 49, false, &ranges, &wide, 0).unwrap());
        assert!(group_bad(&image, 49, true, &ranges, &wide, 0).unwrap());
        assert!(all_groups_bad(&image, &ranges, &wide, 0).unwrap());
    }
}
