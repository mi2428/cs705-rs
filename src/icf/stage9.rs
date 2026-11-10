//! Actual 4D3D8 driver. W/F/C/T bindings are explicit and remain live aliases.
//! The terminal marker belongs ONLY to the end of this driver, after all work.

use super::{
    Error,
    context::Context,
    frequency::{self, Ranges},
    image::apply_patches,
    normalizer::{self, ReferenceImage, copy_field, copy_mask},
    record::{self, DecodedRecord},
    record_image,
};
use crate::repeater::{groups::Groups, predicate, staging, working};

/// Raw document controls and actual Q/B/OS sources, not supplied predicate results.
#[derive(Clone, Copy)]
pub struct Runtime<'a> {
    pub context: &'a Context,
    pub ranges: &'a Ranges,
    pub system_langid: Option<u16>,
    pub previous_reload_guard: u32,
    /// D+1AC/1AD, distinct from image flags and captured Q options.
    pub document_flags: [u8; 2],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    pub typed: normalizer::TypedCounts,
    pub deleted_repeaters: u32,
}

fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 0, reason }
}
fn raw<const N: usize>(image: &[u8], at: usize) -> Result<[u8; N], Error> {
    image
        .get(
            at..at
                .checked_add(N)
                .ok_or(invalid("stage9 address overflow"))?,
        )
        .ok_or(invalid("stage9 read window unavailable"))?
        .try_into()
        .map_err(|_| invalid("stage9 fixed width"))
}
fn byte(image: &[u8], at: usize) -> Result<u8, Error> {
    Ok(raw::<1>(image, at)?[0])
}
fn word(image: &[u8], at: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(raw(image, at)?))
}
fn dword(image: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(raw(image, at)?))
}
fn put(image: &mut [u8], at: usize, value: &[u8]) -> Result<(), Error> {
    apply_patches(image, &[(at, value)])
}
fn move_field(image: &mut [u8], from: usize, to: usize, len: usize) -> Result<(), Error> {
    let value = image
        .get(from..from + len)
        .ok_or(invalid("stage9 internal copy source unavailable"))?
        .to_vec();
    put(image, to, &value)
}
fn load(image: &[u8], kind: u32, index: u32) -> Result<DecodedRecord, Error> {
    let mut h = record::reset_holder(kind, index, 1);
    record_image::load_into(image, kind, index, &mut h)?;
    Ok(h)
}
fn frequency_bad(image: &[u8], kind: u32, index: u32, r: Runtime<'_>) -> Result<bool, Error> {
    frequency::channel_frequency_bad(&load(image, kind, index)?, r.ranges, r.context)
}
fn channel_bad(image: &[u8], kind: u32, index: u32, r: Runtime<'_>) -> Result<bool, Error> {
    let h = load(image, kind, index)?;
    Ok(frequency::channel_bad(&h, r.ranges, r.context, false)?
        || frequency::channel_bad(&h, r.ranges, r.context, true)?)
}

fn repeater_rows(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    r: Runtime<'_>,
) -> Result<u32, Error> {
    let mut index = 0;
    let mut deleted = 0;
    while index < 2500 {
        let mut h = record::reset_holder(0x0c, index, 1);
        working::load_into(image, index as usize, &mut h)?;
        if !h.valid() {
            index += 1;
            continue;
        }
        h.set_offset_hz(normalizer::round_offset_hz(h.offset_hz()));
        let remove = u32::from_le_bytes(h.as_bytes()[0x15c..0x160].try_into().unwrap()) >= 3;
        if remove {
            let groups = Groups::from_working(image)?;
            let group = groups.group_of(index as i32);
            let relative = index as usize - groups.start(group)? as usize;
            working::delete_into(
                image,
                group,
                relative,
                &h,
                |image| {
                    let source = defaults.get(image);
                    let group = Groups::from_working(source)?.group_of(0) as u32;
                    Ok(
                        working::DefaultContext::from_context(r.context, r.system_langid)?
                            .holder(group, 2),
                    )
                },
                |image, which| word(defaults.get(image), [0x3c8b9, 0x3c903][which]),
            )?;
            deleted += 1;
        }
        let kind = staging::classify_bytes(h.as_bytes());
        let b = h.as_mut_bytes();
        if matches!(kind, 1 | 3 | 5) {
            b[0x160..0x170].fill(b' ');
        }
        if kind == 2 {
            b[0x168..0x170].fill(b' ');
        }
        if matches!(kind, 0 | 1 | 2 | 3 | 5) {
            b[0x170..0x174].fill(0);
        }
        if matches!(kind, 1 | 3 | 5) {
            b[0x44..0x4c].fill(0);
        }
        if kind == 4 {
            b[0x48..0x4c].fill(0);
        }
        if matches!(kind, 0 | 1 | 4 | 5) {
            b[0x174..0x178].fill(0);
            b[0x178..0x17c].copy_from_slice(&8_u32.to_le_bytes());
        }
        if !remove {
            working::save_into(image, index as usize, &h)?;
            working::load_into(image, index as usize, &mut h)?;
            index += 1;
        }
    }
    Ok(deleted)
}

fn central(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    current: ReferenceImage<'_>,
    late: ReferenceImage<'_>,
    r: Runtime<'_>,
    memopad_valid: u32,
) -> Result<bool, Error> {
    normalizer::stage9_profile_prefix(image, defaults, r.context)?;
    let mut restore = memopad_valid == 0 || frequency_bad(image, 5, 0, r)?;
    if !restore {
        for broad in [false, true] {
            for index in 1..=10 {
                if frequency::channel_bad(&load(image, 5, index)?, r.ranges, r.context, broad)? {
                    restore = true;
                    break;
                }
            }
            if restore {
                break;
            }
        }
    }
    if restore {
        copy_field(image, defaults, 0x36f4e, 6)?;
        copy_field(image, defaults, 0x36f54, 36)?;
    }
    if r.context.as_bytes()[0x13] != 1 || r.context.source_selector(r.system_langid)? == 0 {
        copy_field(image, defaults, 0x370e0, 120)?;
    } else if r.context.condition_58bdc()? {
        copy_field(image, defaults, 0x370fe, 30)?;
    }
    normalizer::stage9_keyer(image, current)?;
    normalizer::stage9_control_bits(image, defaults)?;
    let mut no_valid = true;
    for index in 0..25 {
        if super::special::ScanEdge::read(image, index)?.valid {
            no_valid = false;
        }
    }
    if no_valid {
        put(image, 0x37802, &[byte(image, 0x37802)? & !0x7c])?;
    }
    let selected = usize::from((byte(image, 0x37802)? >> 2) & 31);
    let source = super::special::ScanEdge::read(image, selected)?;
    let suitable = source.valid
        && source.low_hz != source.high_hz
        && super::special::ScanEdge::read(late.get(image), selected)?.suitable(
            true,
            true,
            true,
            |f| r.ranges.eb64(r.context, f),
        );
    let reset_scan = no_valid || !suitable;
    copy_field(image, defaults, 0x37818, 1)?;
    normalizer::stage9_filter_bits(image, defaults, r.context)?;
    normalizer::stage9_settings_tail(
        image,
        defaults,
        r.context,
        r.context.as_bytes()[0x13],
        r.system_langid,
    )?;
    Ok(reset_scan)
}

/// R_pre at 4E5D3..4EACC. Early true returns mirror the native jumps; controls
/// D1AC/1AD and the C/T sources are never replaced with W or arbitrary booleans.
fn r_pre(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    late: ReferenceImage<'_>,
    r: Runtime<'_>,
    reset_scan: bool,
) -> Result<(bool, bool), Error> {
    let mut home_bad = r.document_flags[0] != 0 || byte(image, 0x3c84b)? == 0xff;
    if !home_bad {
        let group = u32::from(byte(image, 0x3c84b)?);
        let row = u32::from(word(image, 0x3c84c)?);
        let (kind, index) = if group == 100 {
            (3, 2 * row)
        } else {
            (2, 2 * (100 * group + row))
        };
        home_bad = channel_bad(image, kind, index, r)? || channel_bad(image, kind, index + 1, r)?;
    }
    let old = byte(image, 0x37b84)?;
    if old != 0 {
        copy_field(image, defaults, 0x37b84, 1)?;
        if old == 2 {
            return Ok((true, false));
        }
    }
    if frequency_bad(image, 0x1b, 0, r)? || frequency_bad(image, 0x1b, 1, r)? {
        return Ok((true, false));
    }
    let mut use_pair = false;
    let mut pair_split = false;
    if byte(image, 0x3a8ca)? != 0 {
        if r.context.selector()? != 1 {
            return Ok((true, false));
        }
        let flags = byte(image, 0x3a863)?;
        if flags & 1 == 0 && flags & 2 != 0 && home_bad {
            return Ok((true, false));
        }
        if frequency_bad(image, 0x1a, 2, r)? || frequency_bad(image, 0x1a, 3, r)? {
            return Ok((true, false));
        }
        if flags & 1 != 0 {
            use_pair = true;
            pair_split = flags & 2 != 0;
        }
    }
    if byte(image, 0x3a8c9)? != 0 {
        if word(image, 0x3c8b9)? >= 2500 {
            return Ok((true, false));
        }
        if predicate::all_groups_bad(late.get(image), r.ranges, r.context, 3)? {
            return Ok((true, false));
        }
        let flags = byte(image, 0x3a815)?;
        if flags & 1 == 0 && flags & 2 != 0 && home_bad {
            return Ok((true, false));
        }
        if frequency_bad(image, 0x1a, 1, r)? || frequency_bad(image, 0x1a, 2, r)? {
            return Ok((true, false));
        }
        if flags & 1 != 0 {
            use_pair = true;
            pair_split = flags & 2 != 0;
        }
    }
    let flags = byte(image, 0x3c84a)?;
    if use_pair || flags & 1 != 0 {
        if r.document_flags[1] != 0
            || (flags & 2 != 0 && home_bad)
            || (home_bad && use_pair && pair_split)
        {
            return Ok((true, false));
        }
        if frequency_bad(image, 0x1a, 0, r)? {
            return Ok((true, false));
        }
        let hz = dword(image, 0x3a783)?;
        // Native EE90==1 iff the shared EFCC class is 2 (F014 indices 13..15).
        let class = r.ranges.efcc(hz);
        if frequency_bad(image, 0x1a, 1, r)? {
            return Ok((true, false));
        }
        let call = u32::from(byte(image, 0x3a780 + usize::from(class != 2))?) * 2;
        if channel_bad(image, 3, call, r)? || channel_bad(image, 3, call + 1, r)? {
            return Ok((true, false));
        }
    }
    if flags & 1 == 0 && flags & 2 != 0 && home_bad {
        return Ok((true, false));
    }
    let mode = (byte(image, 0x37806)? >> 3) & 31;
    Ok((
        false,
        reset_scan && byte(image, 0x37807)? & 1 != 0 && matches!(mode, 1 | 5),
    ))
}

fn weather_template() -> Result<[u8; 36], Error> {
    let mut h = record::reset_holder(0x19, 0, 1);
    h.set_mode(13);
    let mut values = record::compact_values(&h);
    let mut raw = [0; 36];
    values.write_to(&[0; 36], |at, bytes| {
        raw[at..at + bytes.len()].copy_from_slice(bytes);
        Ok(())
    })?;
    Ok(raw)
}
fn weather_frequency(index: u8) -> Result<u32, Error> {
    Ok(match index {
        0 => 162_550_000,
        1 => 162_400_000,
        2 => 162_475_000,
        3 => 162_425_000,
        4 => 162_450_000,
        5 => 162_500_000,
        6 => 162_525_000,
        7 => 161_650_000,
        8 => 161_775_000,
        9 => 163_275_000,
        _ => {
            return Err(invalid(
                "weather index reads outside native ten-DWORD stack table",
            ));
        }
    })
}
fn suffix(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    current: ReferenceImage<'_>,
    late: ReferenceImage<'_>,
    r: Runtime<'_>,
    reset_scan: bool,
) -> Result<(), Error> {
    let (mut restore, mut scan) = r_pre(image, defaults, late, r, reset_scan)?;
    let v = frequency_bad(image, 0x19, 0, r)? || frequency_bad(image, 0x19, 1, r)?;
    if v {
        copy_field(image, defaults, 0x3c84e, 0x49)?;
        copy_field(image, defaults, 0x3a868, 0x18)?;
        // 4EB5F is inside the V branch, not an unconditional R fallback.
        if !restore
            && byte(image, 0x3a8ca)? == 0
            && byte(image, 0x3a8c9)? == 0
            && byte(image, 0x3c84a)? & 3 == 0
        {
            restore = true;
        }
    }
    if restore {
        move_field(image, 0x3c84e, 0x3c800, 0x49)?;
        copy_field(image, defaults, 0x3c84a, 1)?;
        move_field(image, 0x3a868, 0x3c89b, 0x18)?;
        scan = true;
        for at in [0x3778a, 0x3778c, 0x3778e] {
            copy_field(image, defaults, at, 2)?;
        }
        copy_field(image, defaults, 0x377bf, 1)?;
        if byte(image, 0x3808a)? != 0 && byte(current.get(image), 0x377c6)? == 0 {
            move_field(image, 0x3c801, 0x3c897, 4)?;
        }
        for (at, len) in [
            (0x3a782, 0x49),
            (0x3a7cb, 0x49),
            (0x3a814, 5),
            (0x3a819, 0x49),
            (0x3a862, 5),
            (0x3a867, 1),
            (0x3a8c9, 1),
            (0x3a8ca, 1),
        ] {
            copy_field(image, defaults, at, len)?;
        }
    }
    if byte(image, 0x3c84b)? == 0xff {
        copy_field(image, defaults, 0x3c84b, 1)?;
        copy_field(image, defaults, 0x3c84c, 2)?;
    }
    if scan {
        copy_mask(image, defaults, 0x37807, 1)?;
        copy_mask(image, defaults, 0x37806, 4)?;
        copy_field(image, defaults, 0x37810, 4)?;
    }
    if r.context.selector()? != 1 {
        for (at, len) in [
            (0x3c8b3, 1),
            (0x3a8cb, 1),
            (0x3a819, 0x49),
            (0x3a862, 5),
            (0x3a867, 1),
        ] {
            copy_field(image, defaults, at, len)?;
        }
    }
    if byte(image, 0x3a8ca)? != 0 {
        put(image, 0x3c800, &[byte(image, 0x3c800)? & !3])?;
        put(image, 0x3c84a, &[byte(image, 0x3c84a)? & !7])?;
        put(image, 0x3c801, &weather_template()?)?;
        let frequency = weather_frequency(byte(image, 0x3c8b3)?)?;
        put(image, 0x3c801, &frequency.to_le_bytes())?;
        move_field(image, 0x3c801, 0x3c825, 36)?;
    }
    if word(image, 0x3c8b9)? >= 2500 {
        for (at, len) in [
            (0x3c8b8, 1),
            (0x3c8bc, 0x43),
            (0x3c8ff, 1),
            (0x3c903, 2),
            (0x3c926, 0x43),
        ] {
            copy_field(image, defaults, at, len)?;
        }
    } else {
        let mode = byte(image, 0x3c8ff)?;
        let bad = if mode == 1 {
            word(image, 0x3c903)? >= 2500
        } else if mode == 2 {
            word(image, 0x3c901)? >= 300 || normalizer::late_callsign_list_bad(late.get(image), 3)?
        } else {
            false
        };
        if bad {
            put(image, 0x3c8ff, &[0])?;
            copy_field(image, defaults, 0x3c926, 0x43)?;
            put(image, 0x3c94c, b"CQCQCQ  ")?;
            put(image, 0x3c934, &[b' '; 16])?;
            if byte(image, 0x3a8c9)? != 0 {
                let mut h = load(image, 0x1b, 0)?;
                if byte(image, 0x3c8c4)? == 0 {
                    h.as_mut_bytes()[0x88..0x90].copy_from_slice(b"CQCQCQ  ");
                    h.as_mut_bytes()[0xc4..0xcc].fill(b' ');
                    record_image::store_into(image, 0x1b, 0, &mut h)?;
                    record_image::load_into(image, 0x1b, 0, &mut h)?;
                }
            }
        }
    }
    Ok(())
}

/// Complete stage9. Failure leaves every already-committed W prefix observable.
/// Pointer-equivalent F/C/T roles use Working, never an entry image snapshot.
pub fn normalize(
    image: &mut [u8],
    defaults: ReferenceImage<'_>,
    current: ReferenceImage<'_>,
    late: ReferenceImage<'_>,
    r: Runtime<'_>,
) -> Result<Report, Error> {
    if image.len() != crate::clone::IMAGE_CAPACITY {
        return Err(invalid("stage9 requires the full working image"));
    }
    let typed = normalizer::typed_loop(
        image,
        defaults,
        r.context,
        r.ranges,
        r.system_langid,
        r.previous_reload_guard,
    )?;
    normalizer::stage9_fixed_text(image)?;
    let deleted_repeaters = repeater_rows(image, defaults, r)?;
    let reset_scan = central(image, defaults, current, late, r, typed.memopad_valid)?;
    suffix(image, defaults, current, late, r, reset_scan)?;
    copy_field(image, defaults, 0x3a8c8, 1)?;
    put(image, 0x3c9b0, super::IMAGE_SIGNATURE)?;
    Ok(Report {
        typed,
        deleted_repeaters,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn empty_working() -> Vec<u8> {
        let mut image = vec![0; crate::clone::IMAGE_CAPACITY];
        image[0x355a1] = 6;
        image[0x3c9b0..0x3c9c0].fill(0xcc);
        for cell in image[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
            cell.copy_from_slice(&0xffff_u32.to_le_bytes());
        }
        for row in 0..45 {
            let at = super::super::special::CompactKind::BandStack
                .offset(row)
                .unwrap();
            image[at..at + 4].fill(0xff);
        }
        for row in 0..=10 {
            let at = 0x36f54 + 36 * row;
            image[at..at + 4].fill(0xff);
        }
        image[0xa60b..0xa60f].fill(0xff);
        image[0x3c84b] = 0xff;
        image
    }
    fn runtime<'a>(q: &'a Context, ranges: &'a Ranges) -> Runtime<'a> {
        Runtime {
            context: q,
            ranges,
            system_langid: Some(0x409),
            previous_reload_guard: 1,
            document_flags: [1, 0],
        }
    }

    #[test]
    fn full_driver_empty_working_reaches_terminal_marker_only_after_real_all_kind_and_suffix_work()
    {
        let q = Context::before_registry();
        let ranges = Ranges::generate(&q, Some(0x409)).unwrap();
        let mut image = empty_working();
        image[0x32bb5..0x32bb9].copy_from_slice(b"a\0b\x01");
        image[0x378bc..0x378c0].fill(4);
        let result = normalize(
            &mut image,
            ReferenceImage::Working,
            ReferenceImage::Working,
            ReferenceImage::Working,
            runtime(&q, &ranges),
        )
        .unwrap();
        assert_eq!(
            result.typed,
            normalizer::TypedCounts {
                saved: 80,
                memopad_valid: 0
            }
        );
        assert_eq!(result.deleted_repeaters, 0);
        assert_eq!(&image[0x32bb5..0x32bb9], b"a b ");
        assert_eq!(&image[0x378bc..0x378c0], &[8; 4]);
        assert_eq!(&image[0x3c9b0..0x3c9c0], b"IcomCloneFormat3");
        let again = image.clone();
        normalize(
            &mut image,
            ReferenceImage::Working,
            ReferenceImage::Working,
            ReferenceImage::Working,
            runtime(&q, &ranges),
        )
        .unwrap();
        assert_eq!(image, again);
    }

    #[test]
    fn late_current_gate_failure_preserves_r_copy_prefix_without_writing_signature() {
        let q = Context::before_registry();
        let ranges = Ranges::generate(&q, Some(0x409)).unwrap();
        let mut image = empty_working();
        let mut defaults = image.clone();
        image[0x37b84] = 2;
        image[0x3808a] = 1;
        image[0x3c84e] = 0x59;
        defaults[0x3c84e] = 0x17;
        defaults[0x377bf] = 0x17;
        assert!(
            normalize(
                &mut image,
                ReferenceImage::Separate(&defaults),
                ReferenceImage::Separate(&defaults[..0x377c6]),
                ReferenceImage::Working,
                runtime(&q, &ranges)
            )
            .is_err()
        );
        assert_eq!(image[0x3c800], 0x59);
        assert_eq!(image[0x377bf], 0x17);
        assert_eq!(image[0x37b84], 0);
        assert_eq!(&image[0x3c9b0..0x3c9c0], &[0xcc; 16]);
    }

    #[test]
    fn invalid_weather_index_fails_after_flags_and_native_template_but_before_final_marker() {
        let mut q = Context::before_registry();
        let mut fields = q.fields();
        fields.0[0] = 1;
        q.adopt(fields);
        let ranges = Ranges::generate(&q, None).unwrap();
        let mut image = empty_working();
        image[0x3a8ca] = 1;
        image[0x3c8b3] = 255;
        image[0x3c800] = 0xff;
        image[0x3c84a] = 0xff;
        assert!(
            normalize(
                &mut image,
                ReferenceImage::Working,
                ReferenceImage::Working,
                ReferenceImage::Working,
                runtime(&q, &ranges)
            )
            .is_err()
        );
        assert_eq!(image[0x3c800] & 3, 0);
        assert_eq!(image[0x3c84a] & 7, 0);
        assert_eq!(&image[0x3c801..0x3c805], &[0; 4]);
        assert_eq!(image[0x3c805], 0x70);
        assert_eq!(&image[0x3c808..0x3c80b], &[8, 8, 0]);
        assert_eq!(&image[0x3c9b0..0x3c9c0], &[0xcc; 16]);
    }

    #[test]
    fn full_driver_deletes_type0c_and_keeps_nonzero_group_typed_tail_and_count_order() {
        let mut q = Context::before_registry();
        let mut fields = q.fields();
        fields.0[0] = 1;
        q.adopt(fields);
        let ranges = Ranges::generate(&q, None).unwrap();
        let mut image = empty_working();
        image[0x49914..0x49916].copy_from_slice(&1_u16.to_le_bytes());
        image[0x49914 + 14..0x49916 + 14].copy_from_slice(&1_u16.to_le_bytes());
        let context = working::DefaultContext::from_context(&q, None).unwrap();
        let mut first = context.holder(0, 1);
        first.set_valid(true);
        first.as_mut_bytes()[0x15c..0x160].copy_from_slice(&3_u32.to_le_bytes());
        working::save_into(&mut image, 0, &first).unwrap();
        let mut second = context.holder(7, 1);
        second.set_valid(true);
        second.as_mut_bytes()[0x160..0x168].copy_from_slice(b"SURVIVE ");
        second.as_mut_bytes()[0x3c..0x40].copy_from_slice(&145_000_000_u32.to_le_bytes());
        working::save_into(&mut image, 1, &second).unwrap();
        let report = normalize(
            &mut image,
            ReferenceImage::Working,
            ReferenceImage::Working,
            ReferenceImage::Working,
            runtime(&q, &ranges),
        )
        .unwrap();
        assert_eq!(report.deleted_repeaters, 1);
        assert_eq!(&image[0x49914..0x49916], &[0, 0]);
        assert_eq!(&image[0x49922..0x49924], &[1, 0]);
        let mut survivor = record::reset_holder(0x0c, 0, 1);
        working::load_into(&image, 0, &mut survivor).unwrap();
        assert!(survivor.valid());
        assert_eq!(survivor.group(), 7);
        assert_eq!(&survivor.as_bytes()[0x160..0x168], b"SURVIVE ");
        assert_eq!(
            &survivor.as_bytes()[0x3c..0x40],
            &145_000_000_u32.to_le_bytes()
        );
        assert_eq!(&image[0x3c9b0..0x3c9c0], b"IcomCloneFormat3");
    }

    #[test]
    fn v_before_r_copies_full_73_and24byte_windows_not_decimal49_and18() {
        let mut q = Context::before_registry();
        let mut fields = q.fields();
        fields.0[0] = 1;
        q.adopt(fields);
        let ranges = Ranges::generate(&q, None).unwrap();
        let mut image = empty_working();
        let mut defaults = image.clone();
        defaults[0x3c84e] = 0x59;
        defaults[0x3c896] = 0x77;
        defaults[0x3a87f] = 0x66;
        normalize(
            &mut image,
            ReferenceImage::Separate(&defaults),
            ReferenceImage::Working,
            ReferenceImage::Working,
            runtime(&q, &ranges),
        )
        .unwrap();
        assert_eq!(image[0x3c800], 0x59);
        assert_eq!(image[0x3c848], 0x77);
        assert_eq!(image[0x3c8b2], 0x66);
        assert_eq!(&image[0x3c9b0..0x3c9c0], b"IcomCloneFormat3");
    }

    #[test]
    fn full_suffix_uses_distinct_late_t_count_and_all_list_duplicate_not_working_count() {
        let q = Context::before_registry();
        let ranges = Ranges::generate(&q, Some(0x409)).unwrap();
        let mut image = empty_working();
        image[0x3c8ff] = 2;
        image[0x3c8b9..0x3c8bb].fill(0);
        let mut late = image.clone();
        late[0x49978..0x4997a].copy_from_slice(&2_u16.to_le_bytes());
        late[0x2eda3..0x2edb3].copy_from_slice(b"DUP     DUP     ");
        normalize(
            &mut image,
            ReferenceImage::Working,
            ReferenceImage::Working,
            ReferenceImage::Separate(&late),
            runtime(&q, &ranges),
        )
        .unwrap();
        assert_eq!(&image[0x49978..0x4997a], &[0, 0]);
        assert_eq!(image[0x3c8ff], 0);
        assert_eq!(&image[0x3c94c..0x3c954], b"CQCQCQ  ");
        assert_eq!(&image[0x3c934..0x3c944], &[b' '; 16]);
        assert_eq!(&late[0x49978..0x4997a], &[2, 0]);
    }
}
