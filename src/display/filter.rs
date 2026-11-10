//! Native Filter FIX text, distinct from editable12A..12E and Preset389.

use super::{NoticeParent, invalid, notice_cell};
use crate::icf::Icf;
use std::io;

/// IC-705's St_Common registers only12 predicates at5864C..58834; none is
/// Hide_Filter_DD. Its VMT+10 routes through58DBC/58D00 to2DD58, where this
/// directive is a no-op. Thus the six DD resource rows remain, without live Q.
/// This is specific to the registered IC-705 Filter factory, not an unknown-name
/// success policy for other directives or products.
pub fn dd_rows_visible() -> bool {
    true
}

/// Readonly6B698/6B6E8/6B730. FIX cells never read or clip the live filter image.
/// BW/PBW use the existing captured PE provider; PBT/Shift need no source asset.
/// Keep the actual resource index: editable0..14 is not a fixed-row index/side.
pub fn paint(
    _image: &Icf<'_>,
    key: u16,
    index: usize,
    source_pe: Option<&[u8]>,
) -> io::Result<Vec<u8>> {
    if !matches!(key, 0x12f..=0x132) {
        return Err(invalid("native Filter FIX key outside12F..132"));
    }
    let index =
        i32::try_from(index).map_err(|_| invalid("native Filter index exceeds signed32"))?;
    notice_cell(
        source_pe,
        NoticeParent::Filter,
        u32::from(key),
        index,
        false,
    )?
    .map(|cell| cell.text)
    .ok_or_else(|| invalid("native Filter FIX index is not in its resource layout"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_fields_keep_signed_units_indices_and_both_live_sides_untouched() {
        let mut memory = vec![0; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        memory[0x37850] = 255;
        memory[0x37867..0x37869].copy_from_slice(&[127, 128]);
        let image = Icf::from_memory(memory.clone()).unwrap();
        // Synthetic file-backed PE section, not copied native table payload.
        let mut source = vec![0; 0x6300];
        let put = |bytes: &mut [u8], at: usize, value: u32| {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        source[..2].copy_from_slice(b"MZ");
        put(&mut source, 0x3c, 0x40);
        source[0x40..0x44].copy_from_slice(b"PE\0\0");
        source[0x44..0x46].copy_from_slice(&0x14c_u16.to_le_bytes());
        source[0x46..0x48].copy_from_slice(&1_u16.to_le_bytes());
        source[0x54..0x56].copy_from_slice(&96_u16.to_le_bytes());
        source[0x58..0x5a].copy_from_slice(&0x10b_u16.to_le_bytes());
        put(&mut source, 0x74, 0x400000);
        put(&mut source, 0xc4, 0x3f3000);
        put(&mut source, 0xc8, 0x6200);
        put(&mut source, 0xcc, 0x100);
        source[0x61d0..0x61d6].copy_from_slice(b"%dHz \0");
        for (index, value) in [
            (36, -125_i32),
            (37, 0),
            (38, 125),
            (42, 400),
            (45, 500),
            (46, 600),
            (47, 700),
        ] {
            put(
                &mut source,
                0x100 + 0x3f3a2c - 0x3f3000 + 4 * index,
                value as u32,
            );
        }
        let before = source.clone();
        for key in [0x12f, 0x130] {
            assert_eq!(paint(&image, key, 36, Some(&source)).unwrap(), b"-125Hz ");
            assert_eq!(paint(&image, key, 37, Some(&source)).unwrap(), b"0Hz ");
            assert_eq!(paint(&image, key, 38, Some(&source)).unwrap(), b"125Hz ");
            for index in [42, 45, 46, 47] {
                assert!(
                    paint(&image, key, index, Some(&source))
                        .unwrap()
                        .ends_with(b"Hz ")
                );
            }
            assert!(paint(&image, key, 0, Some(&source)).is_err());
            assert!(paint(&image, key, 39, Some(&source)).is_err());
            assert!(paint(&image, key, 36, None).is_err());
            assert!(paint(&image, key, 36, Some(&[])).is_err());
        }
        for index in [0, 1, 36, 47] {
            assert_eq!(paint(&image, 0x131, index, None).unwrap(), b"---");
            assert_eq!(paint(&image, 0x132, index, None).unwrap(), b"0");
        }
        for key in [0x12a, 0x12b, 0x12c, 0x12d, 0x12e, 0x133, 0x389] {
            assert!(paint(&image, key, 0, Some(&source)).is_err());
        }
        assert!(paint(&image, 0x131, usize::MAX, None).is_err());
        assert_eq!(image.memory(), memory);
        assert_eq!(source, before);
    }
}
