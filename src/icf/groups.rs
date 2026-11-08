use super::{
    Error, Icf, MEMORY_CHANNEL_COUNT, MEMORY_EMPTY_FLAGS, RAW_MEMORY_GROUP_BYTES,
    RAW_MEMORY_GROUPS, RECORD_BYTES,
};

const SLOT_NEXT: usize = 0x35d2c;
const ROW_MASKS: usize = 0x36114;

impl Icf<'_> {
    /// Explicitly canonicalize ordinary-memory descriptors and occupied/free links.
    ///
    /// Static CS-705 evidence: name projection RVA 0xB5334, rebuild 0xB540C,
    /// occupied links 0xB4FEC and ascending free links 0xB5080. Original-GUI
    /// comparison remains pending; this is not the full document-save pipeline.
    /// All saved row positions, raw names, physical IDs, records, row masks and
    /// opaque data remain unchanged. Serialization never applies this implicitly.
    ///
    /// # Errors
    /// Strictly rejects descriptor/order/link/mask/flag contradictions, orphan
    /// used slots or an incomplete/cyclic/stale free chain. All validation and
    /// candidate computation precede storage; errors preserve every image,
    /// metadata and original-source byte. Repeating a successful call is a no-op.
    pub fn canonicalize_memory_layout(&mut self) -> Result<(), Error> {
        const FREE_HEAD: usize = 0x36629;
        const FREE_TAIL: usize = 0x3662b;
        let unsupported = |at: usize, reason| Error::Unsupported {
            line: 5 + at / RECORD_BYTES,
            reason,
        };
        let word = |at: usize| u16::from_le_bytes([self.memory[at], self.memory[at + 1]]);
        let order = self.raw_memory_group_order()?;
        let groups = self.memory_groups()?;
        let mut used = [false; MEMORY_CHANNEL_COUNT];
        for g in 0..100 {
            for row in 0..100 {
                let slot = groups.get(g).and_then(|group| group.slots[row]);
                let mask = ROW_MASKS + 13 * g + row / 8;
                let empty = self.memory[mask] & (1 << (row % 8)) != 0;
                if empty != slot.is_none() {
                    return Err(unsupported(
                        mask,
                        "memory row mask contradicts occupied row map",
                    ));
                }
                if let Some(p) = slot {
                    used[p] = true;
                }
            }
        }
        for (p, &mapped) in used.iter().enumerate() {
            if mapped != (self.memory[MEMORY_EMPTY_FLAGS + p] & 1 == 0) {
                return Err(unsupported(
                    MEMORY_EMPTY_FLAGS + p,
                    "memory used flag differs from row map",
                ));
            }
        }
        let tail = word(FREE_TAIL);
        if tail != u16::MAX && usize::from(tail) >= MEMORY_CHANNEL_COUNT {
            return Err(unsupported(
                FREE_TAIL,
                "memory free tail outside physical slots",
            ));
        }
        let mut free_seen = [false; MEMORY_CHANNEL_COUNT];
        let mut link = FREE_HEAD;
        let mut next = word(link);
        let mut last = u16::MAX;
        let mut count = 0;
        while next != u16::MAX {
            let p = usize::from(next);
            if p >= MEMORY_CHANNEL_COUNT || used[p] || free_seen[p] {
                return Err(unsupported(
                    link,
                    "memory free chain contains invalid, used or repeated slot",
                ));
            }
            free_seen[p] = true;
            count += 1;
            last = next;
            link = SLOT_NEXT + 2 * p;
            next = word(link);
        }
        if count != used.iter().filter(|&&p| !p).count() || last != tail {
            return Err(unsupported(
                FREE_TAIL,
                "memory free chain missing slots or tail mismatch",
            ));
        }

        // One bounded metadata candidate; row masks/flags/Home/records stay opaque.
        let mut candidate = self.memory[RAW_MEMORY_GROUPS..ROW_MASKS].to_vec();
        let links = SLOT_NEXT - RAW_MEMORY_GROUPS;
        candidate[links..].fill(0xff);
        for (g, &descriptor) in order.iter().enumerate() {
            let base = RAW_MEMORY_GROUP_BYTES * g;
            candidate[base] = if g == 99 { 0xff } else { (g + 1) as u8 };
            let head = groups
                .get(g)
                .and_then(|group| group.slots.iter().flatten().next())
                .copied();
            candidate[base + 1..base + 3]
                .copy_from_slice(&head.map_or(u16::MAX, |p| p as u16).to_le_bytes());
            let old = RAW_MEMORY_GROUPS + RAW_MEMORY_GROUP_BYTES * descriptor;
            candidate[base + 3..base + 19].copy_from_slice(&self.memory[old + 3..old + 19]);
            if let Some(group) = groups.get(g) {
                let slots: Vec<_> = group.slots.iter().flatten().copied().collect();
                for pair in slots.windows(2) {
                    let at = links + 2 * pair[0];
                    candidate[at..at + 2].copy_from_slice(&(pair[1] as u16).to_le_bytes());
                }
            }
        }
        let free: Vec<_> = used
            .iter()
            .enumerate()
            .filter_map(|(p, &mapped)| (!mapped).then_some(p))
            .collect();
        for pair in free.windows(2) {
            let at = links + 2 * pair[0];
            candidate[at..at + 2].copy_from_slice(&(pair[1] as u16).to_le_bytes());
        }
        let head = free.first().map_or(u16::MAX, |&p| p as u16).to_le_bytes();
        let tail = free.last().map_or(u16::MAX, |&p| p as u16).to_le_bytes();
        self.memory[RAW_MEMORY_GROUPS..ROW_MASKS].copy_from_slice(&candidate);
        self.memory[0x36628..0x3662d].copy_from_slice(&[0, head[0], head[1], tail[0], tail[1]]);
        Ok(())
    }
}

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
