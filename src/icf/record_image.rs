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
    let Some(address) = channel_address(image, kind, index)? else {
        // 7857 skips the absent physical flag;78A3 returns for valid0 before
        // any name/body pointer use. Slot10 has already released this map.
        if !holder.valid() {
            return Ok(());
        }
        return Err(invalid(
            "channel destination has no allocated physical slot",
        ));
    };
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

/// Load kind04/05/19/1A/1B through special's one raw36 decoder/projection.
/// Memopad slot0 is addressable, though stage9 visits only1..10. No outer reset
/// is implied; invalid rows write the sentinel, not the remaining body fields.
pub fn load_compact_into(
    image: &[u8],
    kind: u32,
    index: u32,
    holder: &mut DecodedRecord,
) -> Result<bool, Error> {
    if matches!(kind, 0x19..=0x1b) {
        return load_paired_into(image, kind, index, holder);
    }
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

/// Save compact kinds at an explicit destination via special's raw36 saver.
/// Frequency commits even when invalid; its remaining32bytes then stay intact.
/// Saver-side callsign correction survives a later callback error in the holder.
/// Shared19..1B instead skip valid0 and store the external flag first. Callback
/// offsets for these shared kinds are absolute; kind04/05 retain raw-relative.
pub fn store_compact_into(
    image: &mut [u8],
    kind: u32,
    index: u32,
    holder: &mut DecodedRecord,
    mut before_store: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    if matches!(kind, 0x19..=0x1b) {
        return store_paired_into(image, kind, index, holder, before_store);
    }
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
/// Type0D's direct logical name/call arrays and live working count. The saved
/// order table is NOT a physical mapping here. Even inactive tail slots decode
/// both fields; the outer reset remains caller-owned. No extra300-row clamp is
/// inserted before the original address computation; actual slices are bounded.
pub fn load_callsign_into(
    image: &[u8],
    index: u32,
    holder: &mut DecodedRecord,
) -> Result<bool, Error> {
    let count = u16::from_le_bytes(window(image, 0x49978, 2)?.try_into().unwrap());
    let name_at = (index as usize)
        .checked_mul(16)
        .and_then(|at| at.checked_add(0x2f703))
        .ok_or(invalid("UR name offset overflow"))?;
    let call_at = (index as usize)
        .checked_mul(8)
        .and_then(|at| at.checked_add(0x2eda3))
        .ok_or(invalid("UR call offset overflow"))?;
    let name = window(image, name_at, 16)?.try_into().unwrap();
    let call = window(image, call_at, 8)?.try_into().unwrap();
    holder.set_header(0x0d, index, holder.binding());
    holder.set_valid(index < u32::from(count));
    record::decode_callsign_into(holder, name, call);
    Ok(holder.valid())
}

/// The type0D saver has no validity/count guard: name16 then call8, each through
/// the shared record codec. Address checks occur at the individual store.
pub fn store_callsign_into(
    image: &mut [u8],
    index: u32,
    holder: &DecodedRecord,
) -> Result<(), Error> {
    record::encode_callsign_into(holder, |event| {
        let (base, stride, bytes): (u32, u32, &[u8]) = match event {
            record::CallsignWrite::Name(bytes) => (0x2f703, 16, bytes),
            record::CallsignWrite::Call(bytes) => (0x2eda3, 8, bytes),
        };
        apply_patches(
            image,
            &[(
                base.wrapping_add(index.wrapping_mul(stride)) as usize,
                bytes,
            )],
        )
    })
}
fn paired_address(kind: u32, index: u32) -> Result<(usize, usize), Error> {
    let flag = match kind {
        0x19 if index < 2 => 0x3c84e,
        0x1a if index < 6 => [0x3a782, 0x3a7cb, 0x3a819][index as usize / 2],
        0x1b if index < 2 => 0x3c800,
        _ => return Err(invalid("paired compact kind/index outside dataset")),
    };
    Ok((flag, flag + 1 + 36 * (index as usize % 2)))
}
fn paired_valid(image: &[u8], kind: u32, index: u32) -> Result<bool, Error> {
    if kind != 0x1a {
        return Ok(true);
    }
    let b = |at| window(image, at, 1).map(|v| v[0]);
    Ok(match index / 2 {
        0 => {
            b(0x3c84a)? & 1 != 0
                || (b(0x3a8c9)? != 0 && b(0x3a815)? & 1 != 0)
                || (b(0x3a8ca)? != 0 && b(0x3a863)? & 1 != 0)
        }
        1 => b(0x3a8c9)? & 1 != 0,
        2 => b(0x3a8ca)? & 1 != 0,
        _ => return Err(invalid("paired compact validity index outside dataset")),
    })
}
/// 6550/8230's paired shared compact datasets. External validity is independent
/// of frequencyMAX; an invalid row never reads its body or split flag source.
pub fn load_paired_into(
    image: &[u8],
    kind: u32,
    index: u32,
    holder: &mut DecodedRecord,
) -> Result<bool, Error> {
    let (flag, at) = paired_address(kind, index)?;
    let valid = paired_valid(image, kind, index)?;
    holder.set_header(kind, index, holder.binding());
    holder.set_group(u32::MAX);
    holder.as_mut_bytes()[0x24] = (index % 2) as u8;
    holder.as_mut_bytes()[0x38..0x3c].fill(0xff);
    let source = if valid {
        Some(window(image, at, 36)?.try_into().unwrap())
    } else {
        None
    };
    let split = if valid {
        window(image, flag, 1)?[0] >> 1
    } else {
        0
    };
    let values = super::special::CompactChannel::decode_bound(source, valid)?;
    record::decode_compact_into(holder, &values, split);
    Ok(valid)
}
/// Shared-kind save skips everything for valid0. Otherwise the external split
/// bit commits before the raw36 frequency/body and holder callsign correction.
pub fn store_paired_into(
    image: &mut [u8],
    kind: u32,
    index: u32,
    holder: &mut DecodedRecord,
    mut before_store: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    let (flag, at) = paired_address(kind, index)?;
    if !holder.valid() {
        return Ok(());
    }
    let oldflag = window(image, flag, 1)?[0];
    let newflag = (oldflag & !2) | ((holder.as_bytes()[0x25] & 1) << 1);
    before_store(flag, &[newflag])?;
    apply_patches(image, &[(flag, &[newflag])])?;
    let old: [u8; 36] = window(image, at, 36)?.try_into().unwrap();
    let mut values = record::compact_values(holder);
    let result = values.write_to(&old, |offset, bytes| {
        before_store(at + offset, bytes)?;
        apply_patches(image, &[(at + offset, bytes)])
    });
    holder.as_mut_bytes()[0xa4..0xac].copy_from_slice(&values.callsigns[1]);
    result
}

/// Outer610C dispatch for all11 stage9 runtime kinds plus the UR list/default.
/// Reset occurs before dispatch, not in lower field-only adapters.
pub fn load_into(
    image: &[u8],
    kind: u32,
    index: u32,
    holder: &mut DecodedRecord,
) -> Result<bool, Error> {
    *holder = record::reset_holder(kind, index, holder.binding());
    match kind {
        2 => {
            let valid = load_channel_into(image, ChannelKind::Normal, index, holder)?;
            let name = window(image, 0x355c3 + 19 * holder.group() as usize, 16)?;
            holder.as_mut_bytes()[0x0d..0x1d].copy_from_slice(name);
            Ok(valid)
        }
        3 => load_channel_into(image, ChannelKind::Call, index, holder),
        4 | 5 => load_compact_into(image, kind, index, holder),
        8 => load_scan_into(image, index, holder),
        0x0c => {
            crate::repeater::working::load_into(image, index as usize, holder)?;
            Ok(holder.valid())
        }
        0x0d => load_callsign_into(image, index, holder),
        0x17 => {
            let at = 0x395ad_u32.wrapping_add(index.wrapping_mul(8)) as usize;
            holder.set_callsign(
                super::MemoryCallsign::Your,
                window(image, at, 8)?.try_into().unwrap(),
            );
            Ok(holder.valid())
        }
        0x15 | 0x16 | 0x18 => load_range_into(image, kind, index, holder),
        0x19..=0x1b => load_paired_into(image, kind, index, holder),
        0x23 => {
            *holder = super::rows::ur_default(index, holder.binding());
            Ok(true)
        }
        0x24 => {
            holder.set_callsign(super::MemoryCallsign::Your, &[b' '; 8]);
            Ok(true)
        }
        _ => Err(invalid("typed image load kind not implemented")),
    }
}
/// Outer77B0 save boundary. The explicit destination selects kind/index; it is
/// not inferred from a copied source header. Each codec commits its own stages.
pub fn store_into(
    image: &mut [u8],
    kind: u32,
    index: u32,
    holder: &mut DecodedRecord,
) -> Result<(), Error> {
    match kind {
        2 => store_channel_into(image, ChannelKind::Normal, index, holder, |_| Ok(())),
        3 => store_channel_into(image, ChannelKind::Call, index, holder, |_| Ok(())),
        4 | 5 => store_compact_into(image, kind, index, holder, |_, _| Ok(())),
        8 => store_scan_into(image, index, holder, |_, _| Ok(())),
        0x0c => crate::repeater::working::save_into(image, index as usize, holder),
        0x0d => store_callsign_into(image, index, holder),
        0x17 => apply_patches(
            image,
            &[(
                0x395ad_u32.wrapping_add(index.wrapping_mul(8)) as usize,
                holder.callsign(super::MemoryCallsign::Your),
            )],
        ),
        0x15 | 0x16 | 0x18 => store_range_into(image, kind, index, holder, |_, _| Ok(())),
        0x19..=0x1b => store_paired_into(image, kind, index, holder, |_, _| Ok(())),
        _ => Err(invalid("typed image save kind not implemented")),
    }
}

/// C6190 cached UR count, or C63B8's first EXACT8space allowed-call prefix.
pub fn callsign_count(image: &[u8], kind: u32) -> Result<u32, Error> {
    match kind {
        0x0d => Ok(u32::from(u16::from_le_bytes(
            window(image, 0x49978, 2)?.try_into().unwrap(),
        ))),
        0x17 => {
            for index in 0..30 {
                if window(image, 0x395ad + 8 * index, 8)? == [b' '; 8] {
                    return Ok(index as u32);
                }
            }
            Ok(30)
        }
        _ => Err(invalid("callsign count requires kind0D or17")),
    }
}

/// Native print dataset leaves:9969C/common, C6530, C6190, C63B8 and C8100.
/// Working49914/49978 must actually exist when reached. No source is padded,
/// no count is clamped/recounted, and no whole-image topology pass runs first.
/// The final false arm is ONLY9969C's explicit other-type branch, not a generic
/// factory/base predicate. Settings/history classes select their own base path.
pub fn print_dataset_count(image: &[u8], kind: u32, group: i32) -> Result<u32, Error> {
    match kind {
        2 if (0..100).contains(&group) => {
            let mut count = 0;
            for row in 0..100 {
                count += u32::from(
                    channel_address(image, ChannelKind::Normal, group as u32 * 200 + row * 2)?
                        .is_some(),
                );
            }
            Ok(count)
        }
        2 => Ok(0),
        3 => Ok(4),
        4 | 7 => Ok(1),
        5 | 8 => {
            let rows = if kind == 5 { 1..11 } else { 0..25 };
            let mut count = 0;
            let mut holder = record::reset_holder(kind, 0, 0);
            for row in rows {
                let valid = if kind == 5 {
                    // 610C/6550 checks MAX before the full compact decoder. An
                    // invalid final slot needs its DWORD, not an invented raw36.
                    let at = super::special::CompactKind::Memopad.offset(row as usize)?;
                    if u32::from_le_bytes(window(image, at, 4)?.try_into().unwrap()) == u32::MAX {
                        false
                    } else {
                        load_compact_into(image, kind, row, &mut holder)?
                    }
                } else {
                    load_scan_into(image, row, &mut holder)?
                };
                count += u32::from(valid);
            }
            Ok(count)
        }
        0x0c if (0..50).contains(&group) => Ok(u32::from(u16::from_le_bytes(
            window(image, 0x49914 + 2 * group as usize, 2)?
                .try_into()
                .unwrap(),
        ))),
        0x0c => Ok(0),
        0x0d | 0x17 => callsign_count(image, kind),
        0x0a => Ok(u32::from(super::settings::gps::gps_group_count_source(
            image,
            group as u8,
        )?)),
        _ => Ok(0),
    }
}

/// B40B8/B425C metadata needed by the print view's query42 row conversion.
/// None is an absent map; Some carries ONLY the actual raw83 header split bit.
pub fn print_channel_row(
    image: &[u8],
    kind: u32,
    group: i32,
    row: u32,
) -> Result<Option<bool>, Error> {
    let (dataset, index) = match kind {
        2 if (0..100).contains(&group) && row < 100 => {
            (ChannelKind::Normal, group as u32 * 200 + row * 2)
        }
        3 if row < 4 => (ChannelKind::Call, row * 2),
        _ => return Ok(None),
    };
    channel_address(image, dataset, index)?
        .map(|address| window(image, address.record_offset, 1).map(|b| b[0] & 2 != 0))
        .transpose()
}

/// C9678/C98E0: blank maps to the capacity sentinel; otherwise compare the
/// first exact8byte match in the CURRENT list/prefix with the holder's index.
pub fn callsign_bad(image: &[u8], holder: &DecodedRecord) -> Result<bool, Error> {
    let sentinel = match holder.kind() {
        0x0d => 300,
        0x17 => 30,
        _ => return Err(invalid("callsign predicate requires kind0D or17")),
    };
    let query = holder.callsign(super::MemoryCallsign::Your);
    let mut first = sentinel;
    if query != &[b' '; 8] {
        for index in 0..callsign_count(image, holder.kind())? {
            let mut candidate = record::reset_holder(holder.kind(), index, holder.binding());
            load_into(image, holder.kind(), index, &mut candidate)?;
            if candidate.callsign(super::MemoryCallsign::Your) == query {
                first = index;
                break;
            }
        }
    }
    Ok(first != holder.index())
}
fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 0, reason }
}

/// Native16-byte view used by61F20; count is the entry gate, not a new loop cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelView {
    pub group: u32,
    pub first: i32,
    pub last: i32,
    pub count: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelFilter {
    pub broad: bool,
    pub include_side1: bool,
    pub count_all: bool,
    pub show_both: u32,
}
/// 61F20 for Home MR/call. Side1 is skipped BEFORE load using the PREVIOUS
/// loaded holder's split byte, not the current row's raw header. Returns bad
/// count, or0/1 at the first bad row when count_all isfalse. View count<=0 reads
/// no image/holder. B/Q remain the shared frequency engine's actual sources.
pub fn channel_bad_count(
    image: &[u8],
    view: ChannelView,
    filter: ChannelFilter,
    ranges: &super::frequency::Ranges,
    q: &super::context::Context,
    binding: u32,
) -> Result<u32, Error> {
    if view.count <= 0 {
        return Ok(0);
    }
    let kind = match view.group {
        0..=99 => 2,
        100 => 3,
        _ => return Err(invalid("Home channel view group outside0..100")),
    };
    let mut holder = record::reset_holder(kind, 0, binding);
    let mut count = 0_u32;
    let mut side = view.first;
    while side <= view.last {
        let skip = side & 1 != 0
            && (!filter.include_side1 || (filter.show_both == 0 && holder.as_bytes()[0x25] == 0));
        if !skip {
            let index = if kind == 3 {
                side as u32
            } else {
                view.group.wrapping_mul(200).wrapping_add(side as u32)
            };
            load_into(image, kind, index, &mut holder)?;
            if super::frequency::channel_bad(&holder, ranges, q, filter.broad)? {
                count = count.wrapping_add(1);
                if !filter.count_all {
                    return Ok(count);
                }
            }
        }
        if side == view.last {
            break;
        }
        side = side
            .checked_add(1)
            .ok_or(invalid("Home side iteration exceeds signed DWORD"))?;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outer_normal_load_supplies_logical_group_name_but_field_only_load_preserves_it() {
        let mut image = image();
        put_map(&mut image, 7, 23, 3);
        image[0x355c3 + 19 * 7..0x355d3 + 19 * 7].copy_from_slice(b"GROUP NAME      ");
        let mut holder = record::reset_holder(2, 1446, 9);
        holder.as_mut_bytes()[0x0d..0x1d].fill(0xa7);
        load_channel_into(&image, ChannelKind::Normal, 1446, &mut holder).unwrap();
        assert_eq!(&holder.as_bytes()[0x0d..0x1d], &[0xa7; 16]);
        load_into(&image, 2, 1446, &mut holder).unwrap();
        assert_eq!(&holder.as_bytes()[0x0d..0x1d], b"GROUP NAME      ");
        load_into(&image, 2, 1447, &mut holder).unwrap();
        assert_eq!(&holder.as_bytes()[0x0d..0x1d], b"GROUP NAME      ");
    }

    #[test]
    fn allowed_callsign_prefix_first_match_and_typed_save_do_not_repair_later_holes() {
        let mut image = vec![0x9a; 0x4997a];
        image[0x395ad..0x3969d].fill(b' ');
        image[0x395ad..0x395bd].copy_from_slice(b"AB\0CD   AB\0CD   ");
        image[0x395c5..0x395cd].copy_from_slice(b"LATER   ");
        assert_eq!(callsign_count(&image, 0x17).unwrap(), 2);
        let mut h = record::reset_holder(0x17, 0, 7);
        load_into(&image, 0x17, 0, &mut h).unwrap();
        assert!(!callsign_bad(&image, &h).unwrap());
        load_into(&image, 0x17, 1, &mut h).unwrap();
        assert!(callsign_bad(&image, &h).unwrap());
        h.set_callsign(super::super::MemoryCallsign::Your, b"AB\0CE   ");
        let before = image.clone();
        store_into(&mut image, 0x17, 1, &mut h).unwrap();
        assert_eq!(&image[..0x395b5], &before[..0x395b5]);
        assert_eq!(&image[0x395bd..], &before[0x395bd..]);
        assert!(!callsign_bad(&image, &h).unwrap());
        load_into(&image, 0x24, 29, &mut h).unwrap();
        assert_eq!(h.callsign(super::super::MemoryCallsign::Your), &[b' '; 8]);
        super::super::normalizer::delete_allowed_into(&mut image, u32::MAX).unwrap();
        assert_eq!(&image[0x395a5..0x395ad], b"AB\0CD   "); // Signed j=-1 still enters native D1DBC.
        assert_eq!(&image[0x395ad..0x395b5], b"AB\0CE   ");
    }
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
        holder.set_valid(false);
        store_channel_into(&mut image, ChannelKind::Normal, 0, &mut holder, |_| {
            panic!("native invalid+empty skips every store")
        })
        .unwrap();
        assert_eq!(image, before);
    }

    #[test]
    fn callsign_adapter_uses_direct_arrays_and_decodes_inactive_tail_without_order_mapping() {
        let mut image = image();
        image[0x49978..0x4997a].copy_from_slice(&1_u16.to_le_bytes());
        image[0x2f713..0x2f723].copy_from_slice(b"INACTIVE TAIL   ");
        image[0x2edab..0x2edb3].copy_from_slice(b"TAIL    ");
        let mut holder = scratch();
        assert!(!load_callsign_into(&image, 1, &mut holder).unwrap());
        assert_eq!(
            (holder.kind(), holder.index(), holder.binding()),
            (0x0d, 1, 0x5a5a5a5a)
        );
        assert_eq!(&holder.as_bytes()[0x26..0x36], b"INACTIVE TAIL   ");
        assert_eq!(&holder.as_bytes()[0x88..0x90], b"TAIL    ");
        assert_eq!(&holder.as_bytes()[0x18c..0x194], &[0x5a; 8]);
        let before = holder.clone();
        assert!(load_callsign_into(&image, u32::MAX, &mut holder).is_err());
        assert!(holder == before);
    }

    #[test]
    fn paired_external_validity_decodes_max_frequency_and_keeps_flag_first_failure_prefix() {
        let mut image = image();
        let mut holder = scratch();
        image[0x3c84e] = 0xa7;
        image[0x3c84f..0x3c853].fill(0xff);
        image[0x3c853] = 0x70;
        assert!(load_paired_into(&image, 0x19, 0, &mut holder).unwrap());
        assert_eq!(holder.mode(), 13); // Native lookup raw7 -> decoded13.
        assert_eq!(&holder.as_bytes()[0x3c..0x40], &u32::MAX.to_le_bytes());
        assert_eq!(holder.as_bytes()[0x25], 1);
        holder.as_mut_bytes()[0x25] = 0;
        holder.as_mut_bytes()[0x3c..0x40].copy_from_slice(&42_u32.to_le_bytes());
        let before = image.clone();
        let mut stores = Vec::new();
        assert!(
            store_paired_into(&mut image, 0x19, 0, &mut holder, |at, _| {
                stores.push(at);
                if at == 0x3c853 {
                    Err(invalid("synthetic paired body failure"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        assert_eq!(stores, [0x3c84e, 0x3c84f, 0x3c853]);
        let mut expected = before;
        expected[0x3c84e] = 0xa5;
        expected[0x3c84f..0x3c853].copy_from_slice(&42_u32.to_le_bytes());
        assert_eq!(image, expected);
        holder.set_valid(false);
        store_paired_into(&mut [], 0x1b, 1, &mut holder, |_, _| {
            panic!("invalid skips all source")
        })
        .unwrap();
        image[0x3c84a] = 0;
        image[0x3a8c9] = 2;
        image[0x3a8ca] = 0;
        image[0x3a815] = 1;
        assert!(load_paired_into(&image, 0x1a, 0, &mut holder).unwrap());
        assert!(!load_paired_into(&image, 0x1a, 2, &mut holder).unwrap()); // Nonzero is not lowbit1.
        assert!(load_paired_into(&image, 0x1b, 1, &mut holder).unwrap());
        assert_eq!(holder.as_bytes()[0x24], 1);
        assert!(load_paired_into(&image, 0x1a, 6, &mut holder).is_err());
    }

    #[test]
    fn home_aggregate_gates_side_before_load_and_distinguishes_first_bad_from_bad_count() {
        let mut image = image();
        put_map(&mut image, 0, 0, 3);
        image[249] = 0;
        image[250..254].copy_from_slice(&321_u32.to_le_bytes());
        image[283..287].copy_from_slice(&456_u32.to_le_bytes());
        let q = super::super::context::Context::before_registry();
        let ranges = super::super::frequency::Ranges::generate(&q, Some(0x409)).unwrap();
        let view = ChannelView {
            group: 0,
            first: 0,
            last: 1,
            count: 2,
        };
        let filter = ChannelFilter {
            broad: true,
            include_side1: true,
            count_all: true,
            show_both: 0,
        };
        assert_eq!(
            channel_bad_count(&image, view, filter, &ranges, &q, 17).unwrap(),
            1
        );
        assert_eq!(
            channel_bad_count(
                &image,
                view,
                ChannelFilter {
                    show_both: 1,
                    ..filter
                },
                &ranges,
                &q,
                17
            )
            .unwrap(),
            2
        );
        assert_eq!(
            channel_bad_count(
                &image,
                view,
                ChannelFilter {
                    show_both: 1,
                    count_all: false,
                    ..filter
                },
                &ranges,
                &q,
                17
            )
            .unwrap(),
            1
        );
        image[249] = 2; // Odd singleton still sees reset split0 BEFORE its own header load.
        assert_eq!(
            channel_bad_count(
                &image,
                ChannelView { first: 1, ..view },
                filter,
                &ranges,
                &q,
                17
            )
            .unwrap(),
            0
        );
        assert_eq!(
            channel_bad_count(
                &[],
                ChannelView { count: 0, ..view },
                filter,
                &ranges,
                &q,
                17
            )
            .unwrap(),
            0
        );
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
        assert!(load_compact_into(&image, 0x19, 0, &mut holder).unwrap());
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
