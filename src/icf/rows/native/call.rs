//! Native type 3 row operations over the fixed four raw83 call pairs.
//! Selection coordinates are post-B5D28 typed sides, never visible node indices.
//! K/N/M, A124/A77C and the raw codec are shared with normal memory. No map
//! allocation, permission-bit override or alternate clipboard format is used.

use super::{
    ChannelKind, ChannelWrite, Clipboard, Confirmation, CopySource, DecodedRecord, Effects, Error,
    Outcome, ReferenceImage, call_after_into, invalid, record, record_image, reference_into,
    transfer_into, write_source,
};
use crate::icf::frequency::Ranges;

#[derive(Debug, Clone, Copy)]
pub struct Selection {
    pub active: usize,
    pub first: usize,
    pub last: usize,
    pub merged: bool,
    pub show_both: bool,
}
impl Selection {
    fn checked(self) -> Result<Self, Error> {
        if self.active >= 8 || self.first > self.last || self.last >= 8 {
            return Err(invalid("native call side selection outside0..8"));
        }
        Ok(self)
    }
    fn pair(self) -> (usize, usize) {
        (self.first & !1, self.last | 1)
    }
}

fn holder(image: &[u8], index: usize) -> Result<DecodedRecord, Error> {
    let mut h = record::reset_holder(3, index as u32, 0);
    record_image::load_channel_into(image, ChannelKind::Call, index as u32, &mut h)?;
    Ok(h)
}

fn snapshot(image: &[u8], first: usize, count: usize) -> Result<Clipboard, Error> {
    let mut clipboard = Clipboard::default();
    clipboard.copy_from(
        image,
        CopySource {
            kind: 3,
            first: first as u32,
            count,
            request: 0,
            expanded: false,
            binding: 0,
            band_enabled: None,
        },
    )?;
    Ok(clipboard)
}

/// CA624 -> A060. Copy updates only the selection bit/range, not image bytes,
/// display mode or a native notification delivery. A failed copy keeps its prefix.
pub fn copy(
    image: &[u8],
    selection: Selection,
    clipboard: &mut Clipboard,
    effects: &mut Effects,
) -> Result<(), Error> {
    let s = selection.checked()?;
    let active = holder(image, s.active)?;
    let (first, last, request) = if s.first != s.last {
        let (first, last) = s.pair();
        (first, last, 1)
    } else if s.show_both || active.split() != 0 {
        (s.first, s.last, 2)
    } else {
        (s.first, s.last + 1, 1)
    };
    clipboard.copy_from(
        image,
        CopySource {
            kind: 3,
            first: first as u32,
            count: last - first + 1,
            request,
            expanded: s.show_both,
            binding: active.binding(),
            band_enabled: None,
        },
    )?;
    effects.context_flags |= 4;
    effects.selection = Some((first, last));
    Ok(())
}

/// CAB08 -> A300 -> D4 -> B4BF8. Call rows are always allocated/valid; a mode 2
/// single-side paste preserves target name/split/SEL. K2/mode 1/limit 1 still calls
/// the writer twice, while the DR loop uses the returned native counter/2.
pub fn paste(
    image: &mut [u8],
    selection: Selection,
    clipboard: &mut Clipboard,
    ranges: &Ranges,
    effects: &mut Effects,
    mut confirm: impl FnMut(Confirmation) -> Result<i32, Error>,
    mut before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<Outcome, Error> {
    let s = selection.checked()?;
    clipboard.validate()?;
    let active = holder(image, s.active)?;
    let mut start = s.active;
    if active.side() != 0 && (s.merged || clipboard.count(true) >= 2 && clipboard.count(false) != 1)
    {
        start -= 1;
    }
    let mode = if !s.merged && (s.show_both || active.split() != 0) && clipboard.count(false) == 1 {
        2
    } else {
        1
    };
    let requested = clipboard.count(mode != 2);
    if requested > 8 - start && confirm(Confirmation::Truncate)? != 1 {
        return Ok(Outcome::default());
    }
    let limit = requested.min(8 - start);
    if limit != 0 && confirm(Confirmation::Overwrite)? != 1 {
        return Ok(Outcome::default());
    }
    let old = image.to_vec();
    let out = transfer_into(
        image,
        ChannelKind::Call,
        &old,
        start as u32,
        mode,
        limit,
        clipboard,
        &mut before_store,
    )?;
    effects.queue(0x3c, 1, 0);
    reference_into(
        image,
        0xd4,
        100,
        start / 2,
        (start + limit.saturating_sub(1)) / 2,
        0,
    )?;
    for row in 0..out.native_count / 2 {
        call_after_into(image, (start / 2 + row) as i32, ranges, effects)?;
    }
    holder(image, start)?; // CAF3A reload precedes selection/context writes.
    effects.selected(start, start.wrapping_add(out.native_count).wrapping_sub(1));
    effects.active = Some(start);
    effects.queue(0x11, 0, 0);
    Ok(out)
}

fn move_range(image: &[u8], s: Selection, direction: i32) -> Result<(usize, usize, usize), Error> {
    let active = holder(image, s.active)?;
    let step = if s.first == s.last
        && (direction < 0 && active.side() != 0
            || direction > 0 && active.side() == 0 && (active.split() != 0 || s.show_both))
    {
        1
    } else {
        2
    };
    let (first, last) = if step == 2 {
        s.pair()
    } else {
        (s.first, s.last)
    };
    Ok((first, last, step))
}

/// CB63C uses two decoded snapshots even for step 2; call has no normal-map
/// rotation shortcut. D5 runs after both transfers, then direction-side DR pairs.
pub fn carry(
    image: &mut [u8],
    selection: Selection,
    direction: i32,
    ranges: &Ranges,
    effects: &mut Effects,
    mut before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<Outcome, Error> {
    let s = selection.checked()?;
    let d = if direction < 0 { -1 } else { 1 };
    let (first, last, step) = move_range(image, s, d)?;
    if d < 0 && first < step || d > 0 && last + step >= 8 {
        return Ok(Outcome::default());
    }
    let neighbor = if d < 0 { first - step } else { last + 1 };
    let mut other = snapshot(image, neighbor, step)?;
    let mut selected = snapshot(image, first, last - first + 1)?;
    let destination = (first as i32 + d * step as i32) as usize;
    let old = image.to_vec();
    let a = transfer_into(
        image,
        ChannelKind::Call,
        &old,
        destination as u32,
        0,
        last - first + step,
        &mut selected,
        &mut before_store,
    )?;
    let old = image.to_vec();
    let b = transfer_into(
        image,
        ChannelKind::Call,
        &old,
        if d < 0 { last - step + 1 } else { first } as u32,
        0,
        step,
        &mut other,
        &mut before_store,
    )?;
    reference_into(image, 0xd5, 100, first / 2, last / 2, d as u32 as usize)?;
    let row = if d < 0 {
        (first as i32 - 1) / 2
    } else {
        first as i32 / 2
    };
    for offset in 0..(last - first).div_ceil(2) + 1 {
        call_after_into(image, row + offset as i32, ranges, effects)?;
    }
    let end = (last as i32 + d * step as i32) as usize;
    effects.selected(destination, end);
    effects.active = Some(if d < 0 { destination } else { end });
    effects.queue(0x11, 0, 0);
    Ok(Outcome {
        processed: true,
        written: a.written + b.written,
        native_count: a.native_count + b.native_count,
        exhausted: false,
    })
}

/// O044: DA -> CEEC8 -> CBE44(flag 0), not clear/delete/insert. Confirmation 1
/// precedes pair expansion and every G read. All G holders are captured before
/// any write, including W=G alias. No D4/D5 or B4BF8 tail belongs to this reset.
pub fn reset_pair(
    image: &mut [u8],
    selection: Selection,
    generation: ReferenceImage<'_>,
    confirmation: i32,
    effects: &mut Effects,
    mut before_store: impl FnMut(u32, &ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<Outcome, Error> {
    if confirmation != 1 {
        return Ok(Outcome::default());
    }
    let s = selection.checked()?;
    let (first, last) = s.pair();
    let mut clipboard = snapshot(generation.get(image), first, last - first + 1)?;
    let old = image.to_vec();
    for (offset, source) in clipboard.records.iter_mut().enumerate() {
        write_source(
            image,
            ChannelKind::Call,
            &old,
            (first + offset) as u32,
            source,
            false,
            &mut before_store,
        )?;
    }
    for index in first..=last {
        effects.queue(0x14, 0, index as u32);
    }
    if !s.show_both {
        effects.display_mode = Some(2);
    }
    effects.queue(0x11, 0, 0);
    effects.queue(0x3c, 1, 0);
    Ok(Outcome {
        processed: true,
        written: last - first + 1,
        native_count: last - first + 1,
        exhausted: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icf::{context::Context, record::ChannelStage};

    fn selection(first: usize, last: usize) -> Selection {
        Selection {
            active: first,
            first,
            last,
            merged: false,
            show_both: false,
        }
    }

    fn ranges() -> Ranges {
        Ranges::generate(&Context::from_bytes([0; 24]), None).unwrap()
    }

    fn image() -> Vec<u8> {
        let mut image = vec![0xa6; 0x4997a];
        image[0x392fa] = 255;
        image[0x392fb..0x392fd].fill(255);
        image[0x3c84b] = 0;
        for index in 0..8 {
            let mut h = record::reset_holder(3, index, 1);
            h.set_valid(true);
            h.set_split(0);
            h.set_name(b"TARGET NAME     ");
            h.set_mode(13);
            h.set_filter(2);
            h.set_channel_field(ChannelField::FrequencyHz, 145_000_000 + index * 1000);
            record_image::store_channel_into(&mut image, ChannelKind::Call, index, &mut h, |_| {
                Ok(())
            })
            .unwrap();
        }
        image
    }

    use crate::icf::record::ChannelField;

    fn compact(image: &mut [u8], kind: u32, count: usize) -> Clipboard {
        let first = u32::from(kind == 5);
        for index in first..first + count as u32 {
            let mut h = record::reset_holder(kind, index, 2);
            h.set_valid(true);
            h.set_name(b"SOURCE NAME     ");
            h.set_mode(15);
            h.set_data(7);
            h.set_filter(8);
            h.set_offset_hz(149);
            h.set_channel_field(ChannelField::FrequencyHz, 146_000_000 + index * 1000);
            record_image::store_compact_into(image, kind, index, &mut h, |_, _| Ok(())).unwrap();
        }
        let mut clipboard = Clipboard::default();
        clipboard
            .copy_from(
                image,
                CopySource {
                    kind,
                    first,
                    count,
                    request: 0,
                    expanded: false,
                    binding: 2,
                    band_enabled: None,
                },
            )
            .unwrap();
        clipboard
    }

    fn ids(effects: &Effects) -> Vec<u32> {
        effects.notifications.iter().map(|n| n.id).collect()
    }

    fn dr(image: &mut [u8], frequency: u32, selected: u8) {
        image[0x3c84a] = 1;
        image[0x3a780] = selected;
        image[0x3a781] = selected;
        let mut h = record::reset_holder(0x1a, 0, 0);
        h.set_valid(true);
        h.set_mode(13);
        h.set_channel_field(ChannelField::FrequencyHz, frequency);
        record_image::store_paired_into(image, 0x1a, 0, &mut h, |_, _| Ok(())).unwrap();
    }

    #[test]
    fn native_call_copy_produces_knm_and_retains_failed_clipboard_prefix() {
        let image = image();
        for (s, expected) in [
            (selection(2, 2), (3, 2, 1, (2, 3))),
            (
                Selection {
                    show_both: true,
                    ..selection(3, 3)
                },
                (2, 1, 1, (3, 3)),
            ),
            (
                Selection {
                    show_both: true,
                    ..selection(2, 5)
                },
                (1, 4, 4, (2, 5)),
            ),
        ] {
            let mut clipboard = Clipboard::default();
            let mut effects = Effects::default();
            copy(&image, s, &mut clipboard, &mut effects).unwrap();
            assert_eq!(
                (
                    clipboard.mode,
                    clipboard.declared_count,
                    clipboard.logical_count,
                    effects.selection.unwrap()
                ),
                expected
            );
            assert_eq!(clipboard.source_kind, 3);
            assert_eq!(effects.context_flags, 4);
            assert_eq!(effects.display_mode, None);
            assert!(!effects.cache_invalidated);
            assert!(effects.notifications.is_empty());
            clipboard.validate().unwrap();
        }
        let mut clipboard = Clipboard::default();
        let mut effects = Effects::default();
        let short = &image[..0xa21c + 83];
        assert!(copy(short, selection(0, 2), &mut clipboard, &mut effects).is_err());
        assert_eq!(
            (
                clipboard.mode,
                clipboard.declared_count,
                clipboard.logical_count,
                clipboard.records.len()
            ),
            (3, 4, 1, 2)
        );
        assert_eq!(effects, Effects::default());
    }

    #[test]
    fn native_call_compact_paste_uses_real_copy_a77c_tail_and_declines_without_mutation() {
        for kind in [4, 5] {
            let mut image = image();
            let mut clipboard = compact(&mut image, kind, 1);
            let before = image.clone();
            let mut effects = Effects::default();
            let mut events = Vec::new();
            let out = paste(
                &mut image,
                Selection {
                    merged: true,
                    ..selection(7, 7)
                },
                &mut clipboard,
                &ranges(),
                &mut effects,
                |reason| {
                    assert_eq!(reason, Confirmation::Overwrite);
                    Ok(1)
                },
                |index, write| {
                    if write.stage() == ChannelStage::Frequency {
                        events.push(index);
                    }
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!((out.written, out.native_count), (2, 2));
            assert_eq!(events, [6, 7]);
            for index in [6, 7] {
                let h = holder(&image, index).unwrap();
                assert_eq!(
                    h.channel_field(ChannelField::FrequencyHz),
                    146_000_000 + u32::from(kind == 5) * 1000
                );
                assert_eq!(
                    (h.mode(), h.data(), h.filter(), h.offset_hz()),
                    (15, 0, 0, 149)
                );
                assert_eq!(h.name(), &[b' '; 16]); // Compact storage has no name.
            }
            assert_eq!(&image[..0xa21c + 3 * 83], &before[..0xa21c + 3 * 83]);
            assert_eq!(&image[0xa368..], &before[0xa368..]);
            assert_eq!(ids(&effects), [0x3c, 0x11]);
            assert_eq!(effects.selection, Some((6, 7)));
            assert_eq!(
                (clipboard.records[0].data(), clipboard.records[0].filter()),
                (0, 0)
            );

            let mut clipboard = compact(&mut image, kind, 3);
            let before = image.clone();
            let source = *clipboard.records[0].as_bytes();
            let mut effects = Effects::default();
            let mut confirmations = Vec::new();
            let out = paste(
                &mut image,
                selection(6, 6),
                &mut clipboard,
                &ranges(),
                &mut effects,
                |reason| {
                    confirmations.push(reason);
                    Ok(0)
                },
                |_, _| panic!("declined store"),
            )
            .unwrap();
            assert!(!out.processed);
            assert_eq!(confirmations, [Confirmation::Truncate]);
            assert_eq!(image, before);
            assert_eq!(clipboard.records[0].as_bytes(), &source);
            let out = paste(
                &mut image,
                selection(0, 0),
                &mut clipboard,
                &ranges(),
                &mut effects,
                |reason| {
                    assert_eq!(reason, Confirmation::Overwrite);
                    Ok(2)
                },
                |_, _| panic!("declined overwrite"),
            )
            .unwrap();
            assert!(!out.processed);
            assert_eq!(image, before);
            assert_eq!(effects, Effects::default());
        }
    }

    #[test]
    fn native_call_k2_limit_one_double_write_and_mode2_preservation_have_distinct_dr_counts() {
        let ranges = ranges();
        let mut source = image();
        let mut h = holder(&source, 4).unwrap();
        h.set_name(b"CLIPBOARD NAME  ");
        record_image::store_channel_into(&mut source, ChannelKind::Call, 4, &mut h, |_| Ok(()))
            .unwrap();
        let mut clipboard = Clipboard::default();
        copy(
            &source,
            Selection {
                show_both: true,
                ..selection(4, 4)
            },
            &mut clipboard,
            &mut Effects::default(),
        )
        .unwrap();
        assert_eq!(
            (
                clipboard.mode,
                clipboard.count(true),
                clipboard.count(false)
            ),
            (2, 1, 1)
        );
        for merged in [true, false] {
            let mut image = image();
            let mut h = holder(&image, 0).unwrap();
            h.set_split(1);
            record_image::store_channel_into(&mut image, ChannelKind::Call, 0, &mut h, |_| Ok(()))
                .unwrap();
            image[0x3c84b] = 100;
            image[0x3c84c..0x3c84e].copy_from_slice(&0u16.to_le_bytes());
            dr(&mut image, 145_000_000, 0);
            let mut clipboard = clipboard.clone();
            let mut effects = Effects::default();
            let mut events = Vec::new();
            let out = paste(
                &mut image,
                Selection {
                    merged,
                    show_both: true,
                    ..selection(1, 1)
                },
                &mut clipboard,
                &ranges,
                &mut effects,
                |_| Ok(1),
                |index, write| {
                    if write.stage() == ChannelStage::Frequency {
                        events.push(index);
                    }
                    Ok(())
                },
            )
            .unwrap();
            if merged {
                assert_eq!((out.written, out.native_count), (2, 2));
                assert_eq!(events, [0, 1]);
                assert_eq!(effects.selection, Some((0, 1)));
                assert_eq!(effects.home_invalidated, 1);
                assert_eq!(effects.dr_invalidated, 1);
                assert_eq!(holder(&image, 1).unwrap().name(), b"CLIPBOARD NAME  ");
            } else {
                assert_eq!((out.written, out.native_count), (1, 1));
                assert_eq!(events, [1]);
                assert_eq!(effects.selection, Some((1, 1)));
                assert_eq!(effects.home_invalidated, 0); // Counter 1 never reads DR.
                assert_eq!(effects.dr_invalidated, 0);
                assert_eq!(holder(&image, 1).unwrap().name(), b"TARGET NAME     ");
                assert_eq!(holder(&image, 1).unwrap().split(), 1);
                assert_eq!(clipboard.records[0].name(), b"CLIPBOARD NAME  ");
                assert_eq!(clipboard.records[0].split(), 0);
            }
        }
    }

    #[test]
    fn native_call_move_uses_snapshots_reference_neighbor_delta_and_directional_dr_pairs() {
        for (s, direction, expected, reference, active, home) in [
            (selection(2, 3), -1, [2, 3, 0, 1, 4, 5, 6, 7], 1, 0, 1),
            (selection(2, 3), 0, [0, 1, 4, 5, 2, 3, 6, 7], 1, 5, 0),
            (
                Selection {
                    show_both: true,
                    ..selection(3, 3)
                },
                -7,
                [0, 1, 3, 2, 4, 5, 6, 7],
                1,
                2,
                0,
            ),
        ] {
            let mut image = image();
            image[0x392fa] = 100;
            image[0x392fb..0x392fd]
                .copy_from_slice(&if direction < 0 { 0u16 } else { 2 }.to_le_bytes());
            image[0x3c84b] = 100;
            image[0x3c84c..0x3c84e].copy_from_slice(&0u16.to_le_bytes());
            dr(&mut image, 145_000_000, 1);
            let before = image.clone();
            let mut effects = Effects::default();
            let out = carry(&mut image, s, direction, &ranges(), &mut effects, |_, _| {
                Ok(())
            })
            .unwrap();
            assert!(out.processed);
            assert_eq!(out.written, if s.first == s.last { 2 } else { 4 });
            for (index, source) in expected.into_iter().enumerate() {
                assert_eq!(
                    holder(&image, index)
                        .unwrap()
                        .channel_field(ChannelField::FrequencyHz),
                    145_000_000 + source * 1000
                );
            }
            assert_eq!(
                u16::from_le_bytes(image[0x392fb..0x392fd].try_into().unwrap()),
                reference
            );
            assert_eq!(effects.active, Some(active));
            assert_eq!(effects.home_invalidated, home);
            assert_eq!(effects.dr_invalidated, 1);
            assert_eq!(ids(&effects), [0x11]);
            assert_eq!(&image[..0xa21c], &before[..0xa21c]);
        }
        let mut image = image();
        let before = image.clone();
        let mut effects = Effects::default();
        assert!(
            !carry(
                &mut image,
                selection(0, 0),
                -1,
                &Ranges::unpopulated(),
                &mut effects,
                |_, _| panic!("boundary store")
            )
            .unwrap()
            .processed
        );
        assert!(
            !carry(
                &mut image,
                selection(6, 7),
                1,
                &ranges(),
                &mut effects,
                |_, _| panic!("boundary store")
            )
            .unwrap()
            .processed
        );
        assert_eq!(image, before);
        assert_eq!(effects, Effects::default());
    }

    #[test]
    fn native_call_reset_pairs_projected_sides_reads_all_g_first_and_has_no_dr_tail() {
        let mut generation = image();
        let mut h = holder(&generation, 2).unwrap();
        h.set_name(b"DEFAULT PAIR    ");
        h.set_split(1);
        h.set_mode(15);
        h.set_data(7);
        h.set_filter(8);
        record_image::store_channel_into(&mut generation, ChannelKind::Call, 2, &mut h, |_| Ok(()))
            .unwrap();
        for confirmation in [0, 2, 1] {
            let mut image = image();
            let before = image.clone();
            let mut effects = Effects::default();
            let out = reset_pair(
                &mut image,
                selection(2, 2),
                ReferenceImage::Separate(&generation),
                confirmation,
                &mut effects,
                |_, _| Ok(()),
            )
            .unwrap();
            assert_eq!(out.processed, confirmation == 1);
            if confirmation == 1 {
                assert_eq!(out.written, 2);
                assert_eq!(holder(&image, 2).unwrap().name(), b"DEFAULT PAIR    ");
                assert_eq!(holder(&image, 3).unwrap().name(), b"DEFAULT PAIR    ");
                assert_eq!(
                    (
                        holder(&image, 2).unwrap().data(),
                        holder(&image, 2).unwrap().filter()
                    ),
                    (0, 0)
                );
                assert_eq!(&image[..0xa21c + 83], &before[..0xa21c + 83]);
                assert_eq!(&image[0xa21c + 2 * 83..], &before[0xa21c + 2 * 83..]);
                assert_eq!(ids(&effects), [0x14, 0x14, 0x11, 0x3c]);
                assert_eq!(effects.notifications[0].arg2, 2);
                assert_eq!(effects.notifications[1].arg2, 3);
                assert_eq!(effects.display_mode, Some(2));
                assert_eq!(effects.selection, None);
                assert_eq!((effects.home_invalidated, effects.dr_invalidated), (0, 0));
            } else {
                assert_eq!(image, before);
                assert_eq!(effects, Effects::default());
            }
        }
        let mut image = image();
        let before = image.clone();
        assert!(
            reset_pair(
                &mut image,
                selection(0, 3),
                ReferenceImage::Separate(&generation[..0xa21c + 83]),
                1,
                &mut Effects::default(),
                |_, _| panic!("all G reads must precede writes")
            )
            .is_err()
        );
        assert_eq!(image, before);
        assert!(
            !reset_pair(
                &mut image,
                selection(8, 8),
                ReferenceImage::Separate(&[]),
                0,
                &mut Effects::default(),
                |_, _| panic!("cancelled store")
            )
            .unwrap()
            .processed
        );
        let out = reset_pair(
            &mut image,
            selection(0, 3),
            ReferenceImage::Working,
            1,
            &mut Effects::default(),
            |_, _| Ok(()),
        )
        .unwrap();
        assert_eq!(out.written, 4);
        assert_eq!(image, before);
    }

    #[test]
    fn native_call_reset_covers_all_four_pairs_without_normal_maps_or_dr_sources() {
        let generation = image();
        for first in [0, 2, 4, 6] {
            let mut image = generation[..0xa368].to_vec();
            let mut h = holder(&image, first).unwrap();
            h.set_name(b"CHANGED NAME    ");
            h.set_channel_field(ChannelField::FrequencyHz, 147_123_456);
            record_image::store_channel_into(
                &mut image,
                ChannelKind::Call,
                first as u32,
                &mut h,
                |_| Ok(()),
            )
            .unwrap();
            let mut effects = Effects::default();
            let mut writes = Vec::new();
            let out = reset_pair(
                &mut image,
                Selection {
                    show_both: true,
                    ..selection(first + 1, first + 1)
                },
                ReferenceImage::Separate(&generation[..0xa368]),
                1,
                &mut effects,
                |index, write| {
                    if write.stage() == ChannelStage::Frequency {
                        writes.push(index);
                    }
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(out.written, 2);
            assert_eq!(writes, [first as u32, first as u32 + 1]);
            assert_eq!(image, generation[..0xa368]);
            assert_eq!(effects.display_mode, None);
            assert_eq!((effects.home_invalidated, effects.dr_invalidated), (0, 0));
        }
    }

    #[test]
    fn native_call_store_and_late_reference_failures_keep_their_actual_prefixes() {
        let mut image = image();
        let mut clipboard = compact(&mut image, 4, 1);
        let before = image.clone();
        let mut effects = Effects::default();
        assert!(
            paste(
                &mut image,
                selection(0, 0),
                &mut clipboard,
                &ranges(),
                &mut effects,
                |_| Ok(1),
                |index, write| {
                    if index == 1 && write.stage() == ChannelStage::Frequency {
                        Err(invalid("injected second call store failure"))
                    } else {
                        Ok(())
                    }
                }
            )
            .is_err()
        );
        assert_eq!(
            holder(&image, 0)
                .unwrap()
                .channel_field(ChannelField::FrequencyHz),
            146_000_000
        );
        assert_eq!(
            holder(&image, 1)
                .unwrap()
                .channel_field(ChannelField::FrequencyHz),
            145_001_000
        );
        assert_ne!(image, before);
        assert!(effects.notifications.is_empty());

        let mut short = before[..0xa368].to_vec(); // Raw call writes succeed; MR header is absent.
        let mut effects = Effects::default();
        assert!(
            paste(
                &mut short,
                selection(0, 0),
                &mut clipboard,
                &ranges(),
                &mut effects,
                |_| Ok(1),
                |_, _| Ok(())
            )
            .is_err()
        );
        assert_eq!(
            holder(&short, 1)
                .unwrap()
                .channel_field(ChannelField::FrequencyHz),
            146_000_000
        );
        assert_eq!(ids(&effects), [0x3c]);
        assert_eq!(effects.selection, None);

        let mut effects = Effects::default();
        let mut image = before.clone();
        let mut h = holder(&image, 0).unwrap();
        h.set_channel_field(ChannelField::FrequencyHz, 147_000_000);
        record_image::store_channel_into(&mut image, ChannelKind::Call, 0, &mut h, |_| Ok(()))
            .unwrap();
        assert!(
            reset_pair(
                &mut image,
                selection(0, 0),
                ReferenceImage::Separate(&before),
                1,
                &mut effects,
                |index, write| {
                    if index == 1 && write.stage() == ChannelStage::Frequency {
                        Err(invalid("injected reset failure"))
                    } else {
                        Ok(())
                    }
                }
            )
            .is_err()
        );
        assert!(effects.notifications.is_empty());
        assert_eq!(effects.display_mode, None);
        assert_eq!(
            holder(&image, 0)
                .unwrap()
                .channel_field(ChannelField::FrequencyHz),
            145_000_000
        );
    }
}
