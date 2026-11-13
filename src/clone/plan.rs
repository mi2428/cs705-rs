use super::Error;
use std::ops::Range;

// CS-705 1.30 CS705.exe SHA-256:
// e9439e1413e1258be77e6ef2188b72276dd714642e3858f4690e5d45653aeca3.
// Raw selector dispatch: RVA 0x45B7A0 + 28*s; read/write 0xDF508/0xDF52C.
// Tagged exclusions: 0xDF574; write splitting: 0x17317..0x17340.
// All endpoints below are exclusive. Adjacent always-excluded rows are merged.
const ENDS: [u32; 4] = [0x3ad00, 0x3b100, 0x3c9c0, 0x3c9c0];
const EXCLUSIONS: [[(u8, u32, u32); 9]; 4] = [
    [
        (3, 0x2e95b, 0x2e9a3),
        (3, 0x32783, 0x327e7),
        (5, 0x37573, 0x37578),
        (5, 0x3796e, 0x37c4a),
        (0, 0x37cee, 0x37cf0),
        (2, 0x37d09, 0x37d0a),
        (1, 0x37d15, 0x37f1d),
        (4, 0x37f1d, 0x38059),
        (3, 0x38ec0, 0x38ec2),
    ],
    [
        (3, 0x2ed5b, 0x2eda3),
        (3, 0x32b83, 0x32be7),
        (5, 0x37973, 0x37978),
        (5, 0x37d6e, 0x3804a),
        (0, 0x380f0, 0x380f2),
        (2, 0x3810d, 0x3810e),
        (1, 0x38119, 0x38321),
        (4, 0x38321, 0x3845d),
        (3, 0x392c0, 0x392c2),
    ],
    [
        (3, 0x2ed5b, 0x2eda3),
        (3, 0x32b83, 0x32be7),
        (5, 0x37973, 0x37978),
        (5, 0x37d6e, 0x3804a),
        (0, 0x39330, 0x39332),
        (2, 0x3934d, 0x3934e),
        (1, 0x39359, 0x39561),
        (4, 0x39561, 0x3969d),
        (3, 0x3ab80, 0x3ab82),
    ],
    [
        (3, 0x2ed5b, 0x2eda3),
        (3, 0x32bb5, 0x32c19),
        (5, 0x379b3, 0x379b8),
        (5, 0x37dae, 0x3808a),
        (0, 0x39330, 0x39332),
        (2, 0x3934d, 0x3934e),
        (1, 0x39359, 0x39561),
        (4, 0x39561, 0x3969d),
        (3, 0x3ab80, 0x3ab82),
    ],
];

fn selector_index(selector: u8) -> Result<usize, Error> {
    match selector {
        1..=3 => Ok(usize::from(selector - 1)),
        4..=6 => Ok(3), // These three raw selectors share the same table.
        _ => Err(Error::Invalid("selector outside 1..=6".into())),
    }
}

/// Return the half-open expected read coverage for a raw selector.
///
/// This is not a prediction of received block sizes or order, an allocation
/// capacity, or a receive-coverage implementation. Selectors do not identify
/// established firmware versions, regions or transports here. No I/O occurs.
///
/// # Errors
///
/// Returns [`Error::Invalid`] for selectors outside 1..=6, without a fallback.
pub fn read_range(selector: u8) -> Result<Range<u32>, Error> {
    Ok(0..ENDS[selector_index(selector)?])
}

/// Plan ascending, half-open write address blocks for a raw selector and mask.
///
/// Each block contains 1..=64 byte addresses and stays within an absolute
/// 64-byte boundary. Mask bits 0..4 retain their tagged optional regions when
/// set; always-excluded addresses are never emitted. Gaps are not compacted.
/// The result contains no data and does not convert image layouts or infer an
/// ICF length, capacity, profile, receive behavior or transfer session.
///
/// E5 counts blocks, not bytes. Use `blocks.len() as u8` for its modulo-256 count;
/// this does not authorize sending frames or establish radio compatibility.
///
/// ```
/// use cs705::clone::{read_range, write_blocks, Frame};
/// # fn main() -> Result<(), cs705::clone::Error> {
/// assert_eq!(read_range(6)?, 0..0x3c9c0);
/// let blocks = write_blocks(6, 0x1f)?;
/// assert_eq!(blocks.len(), 3869);
/// let end = Frame::End { block_count: blocks.len() as u8 };
/// assert_eq!(end, Frame::End { block_count: 0x1d });
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns [`Error::Invalid`] for selectors outside 1..=6 or any mask bit above
/// bit 4. No selector normalization, default mask or missing-data fill is used.
pub fn write_blocks(selector: u8, mask: u8) -> Result<Vec<Range<u32>>, Error> {
    write_blocks_with_cap(selector, mask, 64)
}

/// The source setter keeps the previous cap for nonpositive inputs. This
/// checked serialization boundary accepts only a positive, byte-sized cap.
pub fn write_blocks_with_cap(selector: u8, mask: u8, cap: u32) -> Result<Vec<Range<u32>>, Error> {
    let index = selector_index(selector)?;
    if !(1..=255).contains(&cap) {
        return Err(Error::Invalid("chunk cap outside 1..=255".into()));
    }
    if mask > 0x1f {
        return Err(Error::Invalid("write mask outside 0..=31".into()));
    }
    let mut blocks = Vec::new();
    let mut address = 0;
    // A final empty exclusion emits the tail without a second splitting loop.
    for (tag, start, end) in EXCLUSIONS[index]
        .into_iter()
        .chain([(5, ENDS[index], ENDS[index])])
    {
        if tag == 5 || mask & (1 << tag) == 0 {
            while address < start {
                let next = (address + cap - address % cap).min(start);
                blocks.push(address..next);
                address = next;
            }
            address = end;
        }
    }
    Ok(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_plans_cover_exact_masked_ranges_without_overlap() {
        for selector in 1..=6 {
            let expected_end = match selector {
                1 => 0x3ad00,
                2 => 0x3b100,
                _ => 0x3c9c0,
            };
            assert_eq!(read_range(selector), Ok(0..expected_end));
            for mask in 0..32 {
                // The reference marks byte addresses directly. It does not
                // split ranges or reuse the planner's cursor/boundary arithmetic.
                let mut required = vec![true; expected_end as usize];
                for (tag, start, end) in EXCLUSIONS[selector_index(selector).unwrap()] {
                    if tag == 5 || mask & (1 << tag) == 0 {
                        required[start as usize..end as usize].fill(false);
                    }
                }
                let blocks = write_blocks(selector, mask).unwrap();
                assert!(blocks.windows(2).all(|pair| pair[0].end <= pair[1].start));
                let mut covered = vec![false; required.len()];
                for block in &blocks {
                    assert!(block.start < block.end && block.end <= expected_end);
                    assert!(block.end - block.start <= 64);
                    assert_eq!(block.start / 64, (block.end - 1) / 64);
                    covered[block.start as usize..block.end as usize].fill(true);
                }
                assert!(covered == required, "selector={selector} mask={mask}");
                assert_eq!(
                    blocks.iter().map(|b| b.end - b.start).sum::<u32>(),
                    required.iter().filter(|byte| **byte).count() as u32,
                );
            }
        }
    }

    #[test]
    fn literal_boundaries_totals_and_end_counts() {
        for (mask, count, bytes, end_count) in
            [(0x1f, 3869, 0x3c6df, 0x1d), (0, 3858, 0x3c2ea, 0x12)]
        {
            let blocks = write_blocks(6, mask).unwrap();
            assert_eq!(blocks.len(), count);
            assert_eq!(blocks.first(), Some(&(0..64)));
            assert_eq!(blocks.last(), Some(&(0x3c980..0x3c9c0)));
            assert_eq!(blocks.iter().map(|b| b.end - b.start).sum::<u32>(), bytes);
            assert_eq!(blocks.len() as u8, end_count);
            assert!(blocks.windows(2).all(|pair| pair[0].end <= pair[1].start));
            assert!(blocks.iter().all(|b| {
                (1..=64).contains(&(b.end - b.start)) && b.start / 64 == (b.end - 1) / 64
            }));
        }
    }

    #[test]
    fn rejects_every_unknown_selector_and_extra_mask_bit() {
        for selector in 0..=255 {
            if !(1..=6).contains(&selector) {
                let expected = Err(Error::Invalid("selector outside 1..=6".into()));
                assert_eq!(read_range(selector), expected);
                assert_eq!(
                    write_blocks(selector, 0),
                    Err(Error::Invalid("selector outside 1..=6".into()))
                );
            }
        }
        for selector in 1..=6 {
            for mask in 32..=255 {
                assert_eq!(
                    write_blocks(selector, mask),
                    Err(Error::Invalid("write mask outside 0..=31".into()))
                );
            }
        }
    }

    #[test]
    fn every_positive_byte_cap_preserves_holes_and_absolute_grid() {
        for cap in 1..=255 {
            let blocks = write_blocks_with_cap(6, 31, cap).unwrap();
            assert_eq!(blocks.iter().map(|b| b.end - b.start).sum::<u32>(), 0x3c6df);
            assert!(blocks.windows(2).all(|p| p[0].end <= p[1].start));
            assert!(
                blocks
                    .iter()
                    .all(|b| b.start < b.end && b.start / cap == (b.end - 1) / cap)
            );
            assert!(
                blocks
                    .iter()
                    .all(|b| b.end <= 0x379b3 || b.start >= 0x379b8)
            );
            assert!(
                blocks
                    .iter()
                    .all(|b| b.end <= 0x37dae || b.start >= 0x3808a)
            );
        }
        assert!(write_blocks_with_cap(6, 31, 0).is_err());
        assert!(write_blocks_with_cap(6, 31, 256).is_err());
    }
}
