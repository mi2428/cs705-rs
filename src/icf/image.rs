//! Checked raw-image boundary shared by offline codecs.

use super::{Error, IMAGE_BYTES, IMAGE_SIGNATURE, INTERNAL_MAP_REV, Icf};

impl Icf<'_> {
    /// Adopt an owned MapRev 6 storage image without inventing setting defaults.
    ///
    /// Checks the exact storage size, internal revision and format signature, not
    /// individual settings. Unknown stored values remain intact. The object has
    /// no original file, an empty header comment and EtcData zero; it is not a
    /// factory image or evidence of device compatibility.
    pub fn from_memory(memory: Vec<u8>) -> Result<Icf<'static>, Error> {
        if memory.len() != IMAGE_BYTES {
            return Err(Error::Invalid {
                line: 0,
                reason: "MapRev 6 storage image requires exactly 0x3c9c0 bytes".into(),
            });
        }
        if memory[INTERNAL_MAP_REV] != 6 || !memory.ends_with(IMAGE_SIGNATURE) {
            return Err(Error::Invalid {
                line: 0,
                reason: "MapRev 6 storage revision or signature".into(),
            });
        }
        Ok(Icf {
            source: (&[][..]).into(),
            memory,
            comment: (&[][..]).into(),
            extra_data: 0,
        })
    }

    /// Apply bounded raw patches atomically, in order; later overlaps win.
    ///
    /// This is an integration boundary, not a field editor or normalization
    /// operation. It permits explicit replacement of unknown values. All windows
    /// are checked before any write, including overflow and empty-patch offsets.
    /// Original file bytes and header metadata never change.
    pub fn patch_memory(&mut self, patches: &[(usize, &[u8])]) -> Result<(), Error> {
        apply_patches(&mut self.memory, patches)
    }
}

pub(super) fn apply_patches(image: &mut [u8], patches: &[(usize, &[u8])]) -> Result<(), Error> {
    for &(offset, bytes) in patches {
        if offset > image.len() || bytes.len() > image.len() - offset {
            return Err(Error::Invalid {
                line: 0,
                reason: "raw patch exceeds storage image".into(),
            });
        }
    }
    for &(offset, bytes) in patches {
        image[offset..offset + bytes.len()].copy_from_slice(bytes);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_keeps_original_metadata_and_current_edits_separate() {
        let mut memory = vec![0; IMAGE_BYTES];
        memory[INTERNAL_MAP_REV] = 6;
        memory[IMAGE_BYTES - IMAGE_SIGNATURE.len()..].copy_from_slice(IMAGE_SIGNATURE);
        let source = super::super::format::write_storage(
            &memory,
            super::super::format::Layout::from_map_revision(6),
            b"raw\x82\xa0",
            0,
        )
        .unwrap();
        let mut borrowed = Icf::parse(&source).unwrap();
        assert_eq!(borrowed.original_bytes().as_ptr(), source.as_ptr());
        borrowed.patch_memory(&[(0, &[0x73])]).unwrap();
        let owned = borrowed.into_owned();
        assert_eq!(owned.original_bytes(), source);
        drop(source);
        assert_eq!(owned.comment_bytes(), b"raw\x82\xa0");
        assert_eq!(owned.extra_data(), 0);
        assert_eq!(owned.memory()[0], 0x73);
        let serialized = owned.to_bytes();
        assert_ne!(serialized, owned.original_bytes());
        assert_eq!(Icf::parse(&serialized).unwrap().memory(), owned.memory());
    }

    #[test]
    fn checked_raw_boundary_preserves_unknowns_and_has_one_commit_point() {
        let mut memory = vec![0xa7; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        let mut image = Icf::from_memory(memory.clone()).unwrap();
        for (offset, bytes) in [(usize::MAX, &[][..]), (0x3c9bf, &[1, 2][..])] {
            assert!(image.patch_memory(&[(1, &[0]), (offset, bytes)]).is_err());
            assert_eq!(image.memory(), memory);
        }
        image
            .patch_memory(&[(0x3c9c0, &[]), (1, &[9, 8]), (2, &[7])])
            .unwrap();
        memory[1..3].copy_from_slice(&[9, 7]);
        assert_eq!(image.memory(), memory);
        let file = image.to_bytes();
        assert_eq!(Icf::parse(&file).unwrap().memory(), memory);
        for size in [0, 0x3c9bf, 0x3c9c1] {
            assert!(Icf::from_memory(vec![0; size]).is_err());
        }
        memory[0x355a1] = 5;
        assert!(Icf::from_memory(memory).is_err());
    }
}
