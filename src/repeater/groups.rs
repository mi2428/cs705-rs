//! Repeater working 50 counts and three distinct reference words. Counts live at
//! expanded 49914, NOT in storage Icf; saved starts/order are rebuilt at 3BF0A /
//! 3AB82. This module never decodes 52 byte row fields or copies them as a shift.
//! The row backend calls these transitions at their original commit phase.

use crate::icf::{Error, Icf};

pub(crate) const NAMES: usize = 0x2ea3b;
pub(crate) const REFS: [usize; 3] = [0x392fd, 0x3c8b9, 0x3c903];
pub(crate) const COUNTS: usize = 0x49914;
pub(crate) const WORKING_BYTES: usize = 0x4997a;
pub(crate) const ORDER: usize = 0x3ab82;
pub(crate) const STARTS: usize = 0x3bf0a;
fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 0, reason }
}

pub(crate) fn working_counts(memory: &[u8]) -> Result<[u16; 50], Error> {
    let bytes = memory
        .get(COUNTS..COUNTS + 100)
        .ok_or_else(|| invalid("repeater working counts are truncated"))?;
    Ok(std::array::from_fn(|g| {
        u16::from_le_bytes([bytes[2 * g], bytes[2 * g + 1]])
    }))
}
pub(crate) fn group_of_counts(counts: &[u16; 50], absolute: i32) -> usize {
    let mut end = 0;
    for (group, count) in counts.iter().enumerate() {
        end += i32::from(*count);
        if absolute < end {
            return group;
        }
    }
    50
}

/// C6CE4's ordered table stores, shared by raw phases and checked table output.
pub(crate) fn write_storage_tables(
    counts: &[u16; 50],
    mut write: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<usize, Error> {
    let total: usize = counts.iter().map(|n| usize::from(*n)).sum();
    let mut start = 0_u32;
    for (group, count) in counts.iter().enumerate() {
        let word = if *count == 0 { u16::MAX } else { start as u16 };
        write(STARTS + 2 * group, &word.to_le_bytes())?;
        start += u32::from(*count);
    }
    for index in 0..2500 {
        let word = if index < total {
            index as u16
        } else {
            u16::MAX
        };
        write(ORDER + 2 * index, &word.to_le_bytes())?;
    }
    Ok(total)
}

/// Working counts/name/reference state, retaining opaque saved reference words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Groups {
    pub counts: [u16; 50],
    pub names: [[u8; 16]; 50],
    pub references: [u16; 3],
}
impl Groups {
    /// Expand only normal saved group topology through core's checked decoder.
    /// Unknown row field values do not need to decode merely to rename a group.
    pub fn from_icf(image: &Icf<'_>) -> Result<Self, Error> {
        let groups = image.repeater_groups()?;
        if groups.len() != 50 {
            return Err(invalid("repeater group decoder did not return 50 groups"));
        }
        let mut counts = [0; 50];
        for (g, group) in groups.iter().enumerate() {
            counts[g] = u16::try_from(group.physical_slots().len())
                .map_err(|_| invalid("repeater group count exceeds u16"))?;
        }
        Self::metadata(image.memory(), counts)
    }
    /// Already-expanded working image, not a saved ICF prefix with inferred tail.
    pub fn from_working(memory: &[u8]) -> Result<Self, Error> {
        if memory.len() != WORKING_BYTES {
            return Err(invalid("repeater working image must contain 0x4997a bytes"));
        }
        let counts = working_counts(memory)?;
        let groups = Self::metadata(memory, counts)?;
        if groups.total() > 2500 {
            return Err(invalid("repeater working total exceeds 2500"));
        }
        Ok(groups)
    }
    fn metadata(memory: &[u8], counts: [u16; 50]) -> Result<Self, Error> {
        let mut names = [[0; 16]; 50];
        let mut references = [0; 3];
        for (g, name) in names.iter_mut().enumerate() {
            name.copy_from_slice(
                memory
                    .get(NAMES + 16 * g..NAMES + 16 * (g + 1))
                    .ok_or_else(|| invalid("repeater name storage is truncated"))?,
            );
        }
        for (index, at) in REFS.into_iter().enumerate() {
            let raw = memory
                .get(at..at + 2)
                .ok_or_else(|| invalid("repeater reference storage is truncated"))?;
            references[index] = u16::from_le_bytes([raw[0], raw[1]]);
        }
        Ok(Self {
            counts,
            names,
            references,
        })
    }
    /// C6624's DWORD sum; no invented per-group 50-row restriction.
    pub fn total(&self) -> u32 {
        self.counts.iter().map(|n| u32::from(*n)).sum()
    }
    pub fn start(&self, group: usize) -> Result<u32, Error> {
        if group >= 50 {
            return Err(invalid("repeater group outside 0..50"));
        }
        Ok(self.counts[..group].iter().map(|n| u32::from(*n)).sum())
    }
    pub fn end(&self, group: usize) -> Result<i32, Error> {
        Ok(self
            .start(group)?
            .wrapping_add(u32::from(self.counts[group]))
            .wrapping_sub(1) as i32)
    }
    /// C6704 intentionally returns 50 for an out-of-total reference. It is not
    /// clamped or treated as FFFF, including during delete reference repair.
    pub fn group_of(&self, absolute: i32) -> usize {
        group_of_counts(&self.counts, absolute)
    }
    fn target(&self, group: usize, relative: usize, insert: bool) -> Result<u32, Error> {
        let start = self.start(group)?;
        let count = usize::from(self.counts[group]);
        if relative > count || (!insert && relative == count) || self.total() > 2500 {
            return Err(invalid("repeater row outside normal working range"));
        }
        Ok(start + relative as u32)
    }
    /// D1AD0: call AFTER typed left shifts/tail source, using OLD counts for the
    /// reference predicate; decrement count only after reference adjustment.
    pub fn after_delete(
        &mut self,
        group: usize,
        relative: usize,
        reset: [u16; 2],
    ) -> Result<(), Error> {
        let absolute = self.target(group, relative, false)?;
        let reference = self.references[0];
        if reference != u16::MAX {
            self.references[0] = if u32::from(reference) == absolute {
                u16::MAX
            } else if u32::from(reference) > absolute {
                reference.wrapping_sub(1)
            } else {
                reference
            };
        }
        self.counts[group] = self.counts[group].wrapping_sub(1);
        self.references[1..].copy_from_slice(&reset);
        Ok(())
    }
    /// D1678: call AFTER typed right shifts/default row. Count is incremented
    /// BEFORE D9 reference repair, and both later references reset unconditionally.
    pub fn after_insert(
        &mut self,
        group: usize,
        relative: usize,
        reset: [u16; 2],
    ) -> Result<(), Error> {
        let absolute = self.target(group, relative, true)?;
        if self.total() >= 2500 {
            return Err(invalid("repeater insertion requires total below 2500"));
        }
        self.counts[group] = self.counts[group].wrapping_add(1);
        if self.references[0] != u16::MAX && u32::from(self.references[0]) >= absolute {
            self.references[0] = self.references[0].wrapping_add(1);
        }
        self.references[1..].copy_from_slice(&reset);
        Ok(())
    }
    /// D4 leaves the first reference untouched; only the two later words whose
    /// absolute value lies within the pasted range reset to supplied live G.
    pub fn after_paste(
        &mut self,
        group: usize,
        first: usize,
        last: usize,
        reset: [u16; 2],
    ) -> Result<(), Error> {
        let first = self.target(group, first, false)?;
        let last = self.target(group, last, false)?;
        if first > last {
            return Err(invalid("repeater paste range is reversed"));
        }
        for (index, value) in self.references[1..].iter_mut().enumerate() {
            if (first..=last).contains(&u32::from(*value)) {
                *value = reset[index];
            }
        }
        Ok(())
    }
    /// D7 is a conditional low-level producer, not a newly registered menu.
    pub fn after_clear(&mut self, group: usize, first: usize, last: usize) -> Result<(), Error> {
        let first = self.target(group, first, false)?;
        let last = self.target(group, last, false)?;
        if first > last {
            return Err(invalid("repeater clear range is reversed"));
        }
        if self.references[0] != u16::MAX && (first..=last).contains(&u32::from(self.references[0]))
        {
            self.references[0] = u16::MAX;
        }
        Ok(())
    }
    /// Both carry directions use action D5. The first word follows content, but
    /// the other two words RESET rather than receive the same +/-delta.
    pub fn after_carry(
        &mut self,
        group: usize,
        first: usize,
        last: usize,
        direction: i32,
        reset: [u16; 2],
    ) -> Result<bool, Error> {
        let absolute_first = self.target(group, first, false)?;
        let absolute_last = self.target(group, last, false)?;
        if first > last {
            return Err(invalid("repeater carry range is reversed"));
        }
        let up = direction < 0;
        if (up && first == 0) || (!up && last + 1 == usize::from(self.counts[group])) {
            return Ok(false);
        }
        let neighbor = if up {
            absolute_first - 1
        } else {
            absolute_last + 1
        };
        let affected = if up {
            neighbor..=absolute_last
        } else {
            absolute_first..=neighbor
        };
        let old = u32::from(self.references[0]);
        if self.references[0] != u16::MAX {
            self.references[0] = if (absolute_first..=absolute_last).contains(&old) {
                if up {
                    old.wrapping_sub(1) as u16
                } else {
                    old.wrapping_add(1) as u16
                }
            } else if old == neighbor {
                if up {
                    absolute_last as u16
                } else {
                    absolute_first as u16
                }
            } else {
                self.references[0]
            };
        }
        for (index, value) in self.references[1..].iter_mut().enumerate() {
            if *value < 2500 && affected.contains(&u32::from(*value)) {
                *value = reset[index];
            }
        }
        Ok(true)
    }
    /// Saved identity order/group starts only. Gateway deduplication/typed row
    /// encoding are separate shared-codec work, not hidden in this table helper.
    pub fn storage_tables(&self) -> Result<([u8; 5000], [u8; 100]), Error> {
        if self.total() > 2500 {
            return Err(invalid("repeater storage table total exceeds 2500"));
        }
        let mut order = [0xff; 5000];
        let mut starts = [0xff; 100];
        write_storage_tables(&self.counts, |at, bytes| {
            if at >= STARTS {
                starts[at - STARTS..at - STARTS + 2].copy_from_slice(bytes);
            } else {
                order[at - ORDER..at - ORDER + 2].copy_from_slice(bytes);
            }
            Ok(())
        })?;
        Ok((order, starts))
    }
    /// Explicit checked storage commit; not a full original save/normalizer.
    pub fn write_storage_tables_atomic(&self, image: &mut Icf<'_>) -> Result<(), Error> {
        let (order, starts) = self.storage_tables()?;
        image.patch_memory(&[(0x3bf0a, &starts), (0x3ab82, &order)])
    }
    pub fn write_references(&self, image: &mut Icf<'_>) -> Result<(), Error> {
        for (at, value) in REFS.into_iter().zip(self.references) {
            image.patch_memory(&[(at, &value.to_le_bytes())])?;
        }
        Ok(())
    }
}

/// Explicit 16-byte group rename, independent of row decoding/topology. Caller
/// provides the mode 1-normalized accepted candidate and owns label/notify tail.
pub fn rename_group_raw(image: &mut Icf<'_>, group: usize, name: [u8; 16]) -> Result<(), Error> {
    if group >= 50 {
        return Err(invalid("repeater named group outside 0..50"));
    }
    image.patch_memory(&[(NAMES + 16 * group, &name)])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn groups() -> Groups {
        let mut counts = [0; 50];
        counts[0] = 2;
        counts[2] = 3;
        counts[49] = 1;
        Groups {
            counts,
            names: [[0xa7; 16]; 50],
            references: [3, 4, 65535],
        }
    }
    #[test]
    fn metadata_reference_kinds_do_not_share_delete_insert_paste_rules() {
        let mut g = groups();
        assert_eq!(
            (
                g.total(),
                g.start(2).unwrap(),
                g.end(1).unwrap(),
                g.group_of(600)
            ),
            (6, 2, 1, 50)
        );
        g.after_delete(2, 0, [400, 65535]).unwrap();
        assert_eq!(g.references, [2, 400, 65535]);
        assert_eq!(g.counts[2], 2);
        g.references = [65000, 3, 4];
        g.after_insert(2, 1, [12, 13]).unwrap();
        assert_eq!(g.references, [65001, 12, 13]);
        assert_eq!(g.counts[2], 3);
        assert_eq!(g.group_of(-1), 0);
        g.references[0] = 65534;
        g.after_insert(2, 0, [12, 13]).unwrap();
        assert_eq!(g.references[0], 65535);
        g.references = [3, 2, 4];
        g.after_paste(2, 0, 1, [65535, 900]).unwrap();
        assert_eq!(g.references, [3, 65535, 4]);
        g.after_clear(2, 1, 1).unwrap();
        assert_eq!(g.references, [65535, 65535, 4]);
    }
    #[test]
    fn carry_tracks_only_first_word_and_storage_tables_keep_empty_groups() {
        let mut g = groups();
        g.references = [3, 4, 2];
        assert!(g.after_carry(2, 0, 1, 0, [99, 65535]).unwrap());
        assert_eq!(g.references, [4, 99, 65535]);
        let (order, starts) = g.storage_tables().unwrap();
        assert_eq!(
            &order[..14],
            &[0, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 255, 255]
        );
        assert_eq!(&starts[..8], &[0, 0, 255, 255, 2, 0, 255, 255]);
        assert_eq!(&starts[98..], &[5, 0]);
        let before = g.clone();
        assert!(!g.after_carry(2, 0, 0, -1, [1, 2]).unwrap());
        assert_eq!(g, before);
        g.counts[0] = 2501;
        assert!(g.storage_tables().is_err());
    }
}
