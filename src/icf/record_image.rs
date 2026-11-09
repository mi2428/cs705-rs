//! Working-image address adapters for the shared record codecs.
//!
//! These are the 8230/8360/8401 address and ordered-store boundary, not the
//! outer 610C holder reset, allocation, stage8 map generation or GUI operation.
//! A normal map entry alone establishes typed validity: the physical empty bit
//! and an inactive split side do NOT suppress this original low-level loader.

use super::{
    Error,
    image::apply_patches,
    record::{self, ChannelWrite, DecodedRecord},
};

/// The two raw83 datasets with different map, group and flag semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Normal,
    Call,
}

impl ChannelKind {
    fn code(self) -> u32 {
        match self {
            Self::Normal => 2,
            Self::Call => 3,
        }
    }
    fn coordinates(self, index: u32) -> Result<(u32, u32, usize), Error> {
        let limit = match self {
            Self::Normal => 20000,
            Self::Call => 8,
        };
        if index >= limit {
            return Err(invalid("channel typed index outside dataset"));
        }
        Ok(match self {
            Self::Normal => (index / 200, index % 200 / 2, (index % 2) as usize),
            Self::Call => (100, index / 2, (index % 2) as usize),
        })
    }
}

/// Resolved image offsets, not raw host pointers or allocation ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelAddress {
    pub group: u32,
    pub row: u32,
    pub side: usize,
    pub physical: u32,
    pub record_offset: usize,
    pub flag_offset: Option<usize>,
}

/// Resolve a typed side index. Normal FFFF means no address; call group is100.
/// No order table, physical empty bit or split flag is substituted for the map.
/// Like the original address helper, this does not clamp a non-FFFF map value
/// to0..499. The actual bounded windows are checked before load/store instead.
pub fn channel_address(
    image: &[u8],
    kind: ChannelKind,
    index: u32,
) -> Result<Option<ChannelAddress>, Error> {
    let (group, row, side) = kind.coordinates(index)?;
    let physical = match kind {
        ChannelKind::Normal => {
            let at = 0x3fcd4 + group as usize * 400 + row as usize * 4;
            u32::from_le_bytes(window(image, at, 4)?.try_into().unwrap())
        }
        ChannelKind::Call => row,
    };
    if physical == 0xffff {
        return Ok(None);
    }
    let bytes = (physical as usize)
        .checked_mul(83)
        .ok_or(invalid("channel record offset overflow"))?;
    let (record_offset, flag_offset) = match kind {
        ChannelKind::Normal => (
            bytes,
            Some(
                0x3662d_usize
                    .checked_add(physical as usize)
                    .ok_or(invalid("channel flag offset overflow"))?,
            ),
        ),
        ChannelKind::Call => (0xa21c + bytes, None),
    };
    Ok(Some(ChannelAddress {
        group,
        row,
        side,
        physical,
        record_offset,
        flag_offset,
    }))
}

/// Load source fields and typed address metadata into caller-owned scratch.
/// Binding and all non-loader fields survive. Call the shared outer holder reset
/// separately when implementing610C; do not seed unknown fields with zeros here.
/// None/FFFF clears valid but leaves non-address body fields untouched.
pub fn load_channel_into(
    image: &[u8],
    kind: ChannelKind,
    index: u32,
    holder: &mut DecodedRecord,
) -> Result<bool, Error> {
    let (group, _, side) = kind.coordinates(index)?;
    let address = channel_address(image, kind, index)?;
    let raw = address
        .map(|address| {
            window(image, address.record_offset, 83)
                .map(|bytes| <&[u8; 83]>::try_from(bytes).unwrap())
        })
        .transpose()?;
    let flag = address
        .and_then(|address| address.flag_offset)
        .map(|at| window(image, at, 1).map(|bytes| bytes[0]))
        .transpose()?;
    holder.set_header(kind.code(), index, holder.binding());
    holder.set_group(group);
    holder.as_mut_bytes()[0x24] = side as u8;
    // 8230 descriptor+28, NOT its +18 physical slot or +24 logical row.
    // Normal keeps the initialized FFFFFFFF; call assigns range12/13.
    let range_kind: u32 = match kind {
        ChannelKind::Normal => u32::MAX,
        ChannelKind::Call => 12 + index / 4,
    };
    holder.as_mut_bytes()[0x38..0x3c].copy_from_slice(&range_kind.to_le_bytes());
    holder.set_valid(address.is_some());
    if let Some(raw) = raw {
        record::decode_channel_into(holder, raw, side, flag)?;
    }
    Ok(address.is_some())
}

/// Save to the explicit DESTINATION side, then leave any reload to the caller.
/// Each accepted raw-codec event commits immediately. `before_store` can fail
/// at that event; earlier commits and holder corrections are never rolled back.
/// An empty normal map is not implicitly allocated. Header source index/side
/// does not select the destination, and unknown opaque target bytes survive.
pub fn store_channel_into(
    image: &mut [u8],
    kind: ChannelKind,
    index: u32,
    holder: &mut DecodedRecord,
    mut before_store: impl FnMut(&ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    let address = channel_address(image, kind, index)?.ok_or(invalid(
        "channel destination has no allocated physical slot",
    ))?;
    let mut raw: [u8; 83] = window(image, address.record_offset, 83)?
        .try_into()
        .unwrap();
    let mut flag = address
        .flag_offset
        .map(|at| window(image, at, 1).map(|bytes| bytes[0]))
        .transpose()?;
    record::encode_channel_into(holder, &mut raw, address.side, flag.as_mut(), |write| {
        before_store(&write)?;
        match write {
            ChannelWrite::Body { offset, bytes, .. } => {
                apply_patches(image, &[(address.record_offset + offset, bytes)])
            }
            ChannelWrite::Flag { byte, .. } => apply_patches(
                image,
                &[(
                    address.flag_offset.expect("normal codec flag event"),
                    &[byte],
                )],
            ),
        }
    })
}

fn window(image: &[u8], at: usize, length: usize) -> Result<&[u8], Error> {
    image
        .get(
            at..at
                .checked_add(length)
                .ok_or(invalid("channel window overflow"))?,
        )
        .ok_or(invalid("channel image window unavailable"))
}

fn range_address(kind: u32, index: u32) -> Result<(usize, usize), Error> {
    let (base, length, count) = match kind {
        0x15 => (0x378c0, 8, 30),
        0x16 => (0x3973a, 8, 68),
        0x18 => (0x370e0, 10, 12),
        _ => return Err(invalid("not a typed pair or mode-pair kind")),
    };
    if index >= count {
        return Err(invalid("typed range index outside dataset"));
    }
    Ok((base + index as usize * length, length))
}

/// Load kind15/16 raw8 or kind18 raw10 through the single shared codec.
/// Header/address ownership stays here; untouched decoded fields stay unchanged.
pub fn load_range_into(
    image: &[u8],
    kind: u32,
    index: u32,
    holder: &mut DecodedRecord,
) -> Result<bool, Error> {
    let (at, length) = range_address(kind, index)?;
    let raw = window(image, at, length)?;
    holder.set_header(kind, index, holder.binding());
    if kind == 0x18 {
        record::decode_mode_pair_into(holder, raw.try_into().unwrap());
    } else {
        record::decode_pair_into(holder, raw.try_into().unwrap(), kind)?;
    }
    Ok(holder.valid())
}

/// Ordered pair saver. The callback is a pre-store cutpoint, not a transaction;
/// kind18 byte1 and every other image window survive untouched.
pub fn store_range_into(
    image: &mut [u8],
    kind: u32,
    index: u32,
    holder: &mut DecodedRecord,
    mut before_store: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    let (at, length) = range_address(kind, index)?;
    let mut raw = window(image, at, length)?.to_vec();
    let mut commit = |offset, bytes: &[u8]| {
        before_store(offset, bytes)?;
        apply_patches(image, &[(at + offset, bytes)])
    };
    // Each scratch contains the old target so reserved bytes are never fabricated.
    if kind == 0x18 {
        record::encode_mode_pair_into(holder, raw.as_mut_slice().try_into().unwrap(), &mut commit)
    } else {
        record::encode_pair_into(
            holder,
            raw.as_mut_slice().try_into().unwrap(),
            kind,
            &mut commit,
        )
    }
}

fn compact_kind(kind: u32) -> Result<super::special::CompactKind, Error> {
    match kind {
        4 => Ok(super::special::CompactKind::BandStack),
        5 => Ok(super::special::CompactKind::Memopad),
        _ => Err(invalid("not a band-stack or memopad kind")),
    }
}

/// Load kind04/05 through special's single raw36 decoder and record projection.
/// Memopad slot0 is addressable, though stage9 visits only1..10. No outer reset
/// is implied; invalid rows write the sentinel, not the remaining body fields.
pub fn load_compact_into(
    image: &[u8],
    kind: u32,
    index: u32,
    holder: &mut DecodedRecord,
) -> Result<bool, Error> {
    let dataset = compact_kind(kind)?;
    let source = super::special::CompactChannel::read(image, dataset, index as usize)?;
    let (side, range_kind) = if kind == 4 {
        let band = ((dataset.offset(index as usize)? - 0x368eb) / 108) as u32;
        (index % 3, band)
    } else {
        (index, u32::MAX)
    };
    holder.set_header(kind, index, holder.binding());
    holder.set_group(u32::MAX);
    holder.as_mut_bytes()[0x24] = side as u8;
    holder.as_mut_bytes()[0x38..0x3c].copy_from_slice(&range_kind.to_le_bytes());
    record::decode_compact_into(holder, &source, 0);
    Ok(holder.valid())
}

/// Save kind04/05 at an explicit destination via special's ordered raw36 saver.
/// Frequency commits even when invalid; its remaining32bytes then stay intact.
/// Saver-side callsign correction survives a later callback error in the holder.
pub fn store_compact_into(
    image: &mut [u8],
    kind: u32,
    index: u32,
    holder: &mut DecodedRecord,
    mut before_store: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    let at = compact_kind(kind)?.offset(index as usize)?;
    let old: [u8; 36] = window(image, at, 36)?.try_into().unwrap();
    let mut values = record::compact_values(holder);
    let result = values.write_to(&old, |offset, bytes| {
        before_store(offset, bytes)?;
        apply_patches(image, &[(at + offset, bytes)])
    });
    holder.as_mut_bytes()[0xa4..0xac].copy_from_slice(&values.callsigns[1]);
    result
}

/// Load kind08's raw27 and existence bitmap without resetting unrelated fields.
pub fn load_scan_into(image: &[u8], index: u32, holder: &mut DecodedRecord) -> Result<bool, Error> {
    let source = super::special::ScanEdge::read(image, index as usize)?;
    holder.set_header(8, index, holder.binding());
    record::decode_scan_into(holder, &source);
    Ok(holder.valid())
}

/// Save kind08 through the shared bitmap-before-body saver. The callback uses
/// absolute image offsets. Invalid clearing occurs only after the bitmap commits
/// and remains visible in the holder even if the following raw27 store fails.
pub fn store_scan_into(
    image: &mut [u8],
    index: u32,
    holder: &mut DecodedRecord,
    mut before_store: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    if index >= 25 {
        return Err(invalid("scan typed index outside dataset"));
    }
    let flag = window(image, 0xa60b + index as usize / 8, 1)?[0];
    let mut values = record::scan_values(holder);
    let result = values.save_with(index as usize, flag, |at, bytes| {
        before_store(at, bytes)?;
        apply_patches(image, &[(at, bytes)])
    });
    holder.as_mut_bytes()[0x3c..0x40].copy_from_slice(&values.high_hz.to_le_bytes());
    holder.as_mut_bytes()[0x138..0x13c].copy_from_slice(&values.low_hz.to_le_bytes());
    holder.as_mut_bytes()[0x26..0x36].copy_from_slice(&values.name);
    result
}
fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 0, reason }
}

#[cfg(test)]
mod tests {
    use super::*;
    use record::ChannelStage;

    fn image() -> Vec<u8> {
        let mut image = vec![0xa6; 0x4997a];
        for cell in image[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
            cell.copy_from_slice(&0xffff_u32.to_le_bytes());
        }
        image
    }
    fn scratch() -> DecodedRecord {
        DecodedRecord::from_bytes([0x5a; 616])
    }
    fn put_map(image: &mut [u8], group: usize, row: usize, physical: u32) {
        let at = 0x3fcd4 + 400 * group + 4 * row;
        image[at..at + 4].copy_from_slice(&physical.to_le_bytes());
    }

    #[test]
    fn nonidentity_map_both_sides_call_group_and_empty_flags_do_not_change_address_semantics() {
        let mut image = image();
        put_map(&mut image, 7, 23, 419);
        image[0x3662d + 419] = 0x81; // Even a contradictory empty bit does not override the map.
        image[83 * 419] = 0; // An inactive split side is still a typed-loader source.
        image[83 * 419 + 34..83 * 419 + 38].copy_from_slice(&123456_u32.to_le_bytes());
        let mut holder = scratch();
        assert!(
            load_channel_into(
                &image,
                ChannelKind::Normal,
                7 * 200 + 23 * 2 + 1,
                &mut holder
            )
            .unwrap()
        );
        assert_eq!(
            (holder.group(), holder.index(), holder.binding()),
            (7, 1447, 0x5a5a5a5a)
        );
        assert_eq!(holder.as_bytes()[0x24], 1);
        assert_eq!(
            &holder.as_bytes()[0x38..0x40],
            &[0xff, 0xff, 0xff, 0xff, 0x40, 0xe2, 1, 0]
        );
        assert_eq!(holder.as_bytes()[0x25], 0);
        assert_eq!(&holder.as_bytes()[0x54..0x58], &2_u32.to_le_bytes());
        assert!(!load_channel_into(&image, ChannelKind::Normal, 0, &mut holder).unwrap());
        assert!(!holder.valid());
        assert_eq!(&holder.as_bytes()[0x3c..0x40], &123456_u32.to_le_bytes());
        let call = channel_address(&image, ChannelKind::Call, 7)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                call.group,
                call.row,
                call.side,
                call.record_offset,
                call.flag_offset
            ),
            (100, 3, 1, 0xa21c + 249, None)
        );
        assert!(load_channel_into(&image, ChannelKind::Call, 7, &mut holder).unwrap());
        assert_eq!(holder.group(), 100);
        assert_eq!(&holder.as_bytes()[0x38..0x3c], &13_u32.to_le_bytes());
        assert!(load_channel_into(&image, ChannelKind::Call, 0, &mut holder).unwrap());
        assert_eq!(&holder.as_bytes()[0x38..0x3c], &12_u32.to_le_bytes());
        assert_eq!(&holder.as_bytes()[0x54..0x58], &[0; 4]);
    }

    #[test]
    fn ordered_failure_keeps_empty_name_select_prefix_and_uses_destination_side() {
        let mut image = image();
        put_map(&mut image, 0, 0, 3);
        let before = image.clone();
        let mut holder = scratch();
        holder.set_valid(true);
        holder.as_mut_bytes()[0x24] = 0;
        holder.as_mut_bytes()[0x26..0x36].copy_from_slice(b"DESTINATION     ");
        holder.as_mut_bytes()[0x54..0x58].copy_from_slice(&3_u32.to_le_bytes());
        let mut seen = Vec::new();
        assert!(
            store_channel_into(&mut image, ChannelKind::Normal, 1, &mut holder, |write| {
                seen.push(write.stage());
                if write.stage() == ChannelStage::Split {
                    Err(invalid("synthetic store failure"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        assert_eq!(
            seen,
            [
                ChannelStage::Empty,
                ChannelStage::Name,
                ChannelStage::Select,
                ChannelStage::Split
            ]
        );
        let mut expected = before;
        expected[0x3662d + 3] = 0xe6;
        expected[249 + 67..249 + 83].copy_from_slice(b"DESTINATION     ");
        assert_eq!(image, expected);
        store_channel_into(&mut image, ChannelKind::Normal, 1, &mut holder, |_| Ok(())).unwrap();
        assert_eq!(&image[249 + 1..249 + 34], &expected[249 + 1..249 + 34]);
        assert_ne!(&image[249 + 34..249 + 67], &expected[249 + 34..249 + 67]);
    }

    #[test]
    fn out_of_physical_domain_is_not_clamped_but_out_of_image_and_indices_fail_closed() {
        let mut image = image();
        put_map(&mut image, 0, 0, 0xb1b);
        assert_eq!(
            channel_address(&image, ChannelKind::Normal, 0)
                .unwrap()
                .unwrap()
                .record_offset,
            0x399c1
        );
        let mut holder = scratch();
        assert!(load_channel_into(&image, ChannelKind::Normal, 0, &mut holder).unwrap());
        put_map(&mut image, 0, 0, u32::MAX);
        let before = holder.clone();
        assert!(load_channel_into(&image, ChannelKind::Normal, 0, &mut holder).is_err());
        assert!(holder == before);
        assert!(channel_address(&image, ChannelKind::Normal, 20000).is_err());
        assert!(channel_address(&image, ChannelKind::Call, 8).is_err());
        put_map(&mut image, 0, 0, 0xffff);
        let before = image.clone();
        assert!(
            store_channel_into(&mut image, ChannelKind::Normal, 0, &mut holder, |_| Ok(()))
                .is_err()
        );
        assert_eq!(image, before);
    }

    #[test]
    fn shared_pair_adapters_preserve_reserved_byte_and_failure_prefix() {
        let mut image = image();
        let mut holder = record::reset_holder(0x18, 11, 17);
        let at = 0x370e0 + 110;
        image[at] = 3;
        image[at + 1] = 0xa7;
        image[at + 2..at + 6].copy_from_slice(&149_u32.to_le_bytes());
        image[at + 6..at + 10].copy_from_slice(&150_u32.to_le_bytes());
        assert!(load_range_into(&image, 0x18, 11, &mut holder).unwrap());
        assert_eq!(&holder.as_bytes()[0x138..0x13c], &149_u32.to_le_bytes());
        holder.as_mut_bytes()[0x138..0x13c].copy_from_slice(&777_u32.to_le_bytes());
        let before = image.clone();
        let mut seen = Vec::new();
        assert!(
            store_range_into(&mut image, 0x18, 11, &mut holder, |offset, _| {
                seen.push(offset);
                if offset == 6 {
                    Err(invalid("synthetic second range store failure"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        assert_eq!(seen, [2, 6]);
        let mut expected = before;
        expected[at + 2..at + 6].copy_from_slice(&777_u32.to_le_bytes());
        assert_eq!(image, expected);
        assert_eq!(image[at + 1], 0xa7);
        for (kind, limit) in [(0x15, 30), (0x16, 68), (0x18, 12)] {
            assert!(load_range_into(&image, kind, limit, &mut holder).is_err());
        }
    }

    #[test]
    fn compact_adapter_keeps_permuted_range_reserved_slot_and_ordered_holder_correction() {
        let mut image = image();
        let at = super::super::special::CompactKind::BandStack
            .offset(30)
            .unwrap();
        image[at..at + 4].copy_from_slice(&42_u32.to_le_bytes());
        let mut holder = scratch();
        assert!(load_compact_into(&image, 4, 30, &mut holder).unwrap());
        assert_eq!(holder.group(), u32::MAX);
        assert_eq!(holder.as_bytes()[0x24], 0);
        assert_eq!(&holder.as_bytes()[0x38..0x3c], &12_u32.to_le_bytes());
        holder.as_mut_bytes()[0x3c..0x40].copy_from_slice(&123_u32.to_le_bytes());
        holder.as_mut_bytes()[0xa4..0xac].copy_from_slice(b"ABCDEFGG");
        holder.as_mut_bytes()[0xc4..0xcc].copy_from_slice(b"RPT2    ");
        let before = image.clone();
        assert!(
            store_compact_into(&mut image, 4, 30, &mut holder, |offset, _| {
                if offset == 4 {
                    Err(invalid("synthetic compact body failure"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        let mut expected = before;
        expected[at..at + 4].copy_from_slice(&123_u32.to_le_bytes());
        assert_eq!(image, expected);
        assert_eq!(&holder.as_bytes()[0xa4..0xac], b"ABCDEFG ");
        store_compact_into(&mut image, 5, 0, &mut holder, |_, _| Ok(())).unwrap();
        assert_eq!(&image[0x36f54..0x36f58], &123_u32.to_le_bytes());
        image[0x36f54..0x36f58].fill(0xff);
        let body = holder.as_bytes()[0x40..].to_vec();
        assert!(!load_compact_into(&image, 5, 0, &mut holder).unwrap());
        assert_eq!(&holder.as_bytes()[0x40..], body);
        assert_eq!(&holder.as_bytes()[0x3c..0x40], &u32::MAX.to_le_bytes());
        let before = image.clone();
        store_compact_into(&mut image, 5, 1, &mut holder, |offset, _| {
            assert_eq!(offset, 0);
            Ok(())
        })
        .unwrap();
        let mut expected = before;
        expected[0x36f78..0x36f7c].fill(0xff);
        assert_eq!(image, expected);
        assert!(load_compact_into(&image, 4, 45, &mut holder).is_err());
        assert!(load_compact_into(&image, 5, 11, &mut holder).is_err());
        assert!(load_compact_into(&image, 0x19, 0, &mut holder).is_err());
    }

    #[test]
    fn scan_adapter_preserves_invalid_body_until_bitmap_commit_and_keeps_failure_prefix() {
        let mut image = image();
        image[0xa60c] = 0xff;
        let mut holder = scratch();
        let before = holder.clone();
        assert!(!load_scan_into(&image, 9, &mut holder).unwrap());
        assert_eq!(&holder.as_bytes()[0x0d..], &before.as_bytes()[0x0d..]);
        let before = image.clone();
        let body = holder.clone();
        assert!(
            store_scan_into(&mut image, 9, &mut holder, |_, _| Err(invalid(
                "synthetic bitmap failure"
            )))
            .is_err()
        );
        assert_eq!(image, before);
        assert!(holder == body);
        image[0xa60c] = 0xfd;
        assert!(
            store_scan_into(&mut image, 9, &mut holder, |at, _| {
                if at == 0xa368 + 27 * 9 {
                    Err(invalid("synthetic scan body failure"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        assert_eq!(image, before);
        assert_eq!(&holder.as_bytes()[0x3c..0x40], &[0; 4]);
        assert_eq!(&holder.as_bytes()[0x138..0x13c], &[0; 4]);
        assert_eq!(&holder.as_bytes()[0x26..0x36], &[b' '; 16]);
        store_scan_into(&mut image, 9, &mut holder, |_, _| Ok(())).unwrap();
        assert_eq!(&image[0xa368 + 27 * 9..0xa368 + 27 * 9 + 8], &[0; 8]);
        assert!(load_scan_into(&image, 25, &mut holder).is_err());
        assert!(
            store_scan_into(&mut image, 25, &mut holder, |_, _| panic!("invalid index")).is_err()
        );
    }
}
