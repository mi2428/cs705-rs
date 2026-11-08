use super::{
    Error, Icf, MEMORY_CHANNEL_COUNT, MEMORY_EMPTY_FLAGS, RAW_MEMORY_GROUP_BYTES,
    RAW_MEMORY_GROUPS, RECORD_BYTES,
};

const SLOT_NEXT: usize = 0x35d2c;
const ROW_MASKS: usize = 0x36114;

/// A read-only logical group in stored ordinal order, not split UI rows.
///
/// Names borrow the exact raw descriptor bytes, without decoding or trimming.
pub struct MemoryGroup<'a> {
    descriptor: usize,
    name: &'a [u8],
    slots: [Option<usize>; 100],
}

impl MemoryGroup<'_> {
    /// Return the stored descriptor index, which need not equal the ordinal.
    pub fn raw_descriptor(&self) -> usize {
        self.descriptor
    }

    /// Return the exact 16-byte raw name, including NUL/non-ASCII bytes.
    pub fn name_bytes(&self) -> &[u8] {
        self.name
    }

    /// Return physical slots for saved rows 0..100; empty rows are `None`.
    pub fn slots(&self) -> &[Option<usize>; 100] {
        &self.slots
    }
}

impl Icf<'_> {
    /// Read logical groups and their 100 saved rows without changing the image.
    ///
    /// This static-only CS-705 1.30 mapping (RVA 0xB5170/0xB5124/0xB5293)
    /// requires all 100 descriptors exactly once, ending with FF. Masks follow
    /// logical ordinal, not raw descriptor. Intermediate empty groups remain;
    /// only trailing groups with head FFFF and an all-space name are omitted.
    /// This is not split UI projection, save reconstruction or radio validation.
    /// Original-GUI behavior has not been compared. Free lists, unreferenced
    /// fields and channel meanings are not validated or normalized.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for short/invalid descriptor chains,
    /// out-of-range heads/links, repeated slots, empty-flag contradictions or
    /// links remaining after 100 rows. These are strict subset guards, not the
    /// original application's full invalid-input acceptance rules.
    pub fn memory_groups(&self) -> Result<Vec<MemoryGroup<'_>>, Error> {
        let order = self.raw_memory_group_order()?;
        let mut seen = [false; MEMORY_CHANNEL_COUNT];
        let mut groups = Vec::new();
        let mut length = 0;
        for (ordinal, descriptor) in order.into_iter().enumerate() {
            let base = RAW_MEMORY_GROUPS + RAW_MEMORY_GROUP_BYTES * descriptor;
            let name = &self.memory[base + 3..base + 19];
            let mut link = base + 1;
            let mut slot = u16::from_le_bytes([self.memory[link], self.memory[link + 1]]);
            if slot != u16::MAX || name.iter().any(|byte| *byte != b' ') {
                length = ordinal + 1;
            }
            if slot != u16::MAX && usize::from(slot) >= MEMORY_CHANNEL_COUNT {
                return Err(Error::Unsupported {
                    line: 5 + link / RECORD_BYTES,
                    reason: "raw memory group head outside physical slots or FFFF",
                });
            }
            let mut slots = [None; 100];
            for (row, entry) in slots.iter_mut().enumerate() {
                if slot == u16::MAX
                    || self.memory[ROW_MASKS + 13 * ordinal + row / 8] & (1 << (row % 8)) != 0
                {
                    continue;
                }
                let physical = usize::from(slot);
                if seen[physical] {
                    return Err(Error::Unsupported {
                        line: 5 + link / RECORD_BYTES,
                        reason: "logical memory groups repeat a physical slot",
                    });
                }
                if self.memory[MEMORY_EMPTY_FLAGS + physical] & 1 != 0 {
                    return Err(Error::Unsupported {
                        line: 5 + (MEMORY_EMPTY_FLAGS + physical) / RECORD_BYTES,
                        reason: "logical memory slot has empty physical flag",
                    });
                }
                seen[physical] = true;
                *entry = Some(physical);
                link = SLOT_NEXT + 2 * physical;
                slot = u16::from_le_bytes([self.memory[link], self.memory[link + 1]]);
                if slot != u16::MAX && usize::from(slot) >= MEMORY_CHANNEL_COUNT {
                    return Err(Error::Unsupported {
                        line: 5 + link / RECORD_BYTES,
                        reason: "logical memory next outside physical slots or FFFF",
                    });
                }
            }
            if slot != u16::MAX {
                return Err(Error::Unsupported {
                    line: 5 + link / RECORD_BYTES,
                    reason: "logical memory link remains after 100 rows",
                });
            }
            groups.push(MemoryGroup {
                descriptor,
                name,
                slots,
            });
        }
        groups.truncate(length);
        Ok(groups)
    }
}
