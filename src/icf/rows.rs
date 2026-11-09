//! Normal-memory working maps and storage reconstruction.
//!
//! Physical records remain at `83*p`; logical `(group,row)` maps are DWORDs in
//! the expanded tail, not physical records or split-view row coordinates.
//!
//! ```text
//! storage: [records][descriptors][next][row masks][flags] ... [opaque]
//! working: storage + ... [group count][100 counts][100 * 100 slot DWORDs]
//!                         3FB40       3FB44        3FCD4
//! ```
//!
//! Implements the accepted normal flat-map domain of B3800/B38B8/B3E1C/B3F18/
//! B3CD0 and B540C. This is not the GUI split-side clipboard/confirmation layer.
//! Allocation commits a prefix on exhaustion; no record initialization or hidden
//! fallback template is supplied. Save reconstructs links from maps and flags,
//! so stale saved free links are not an allocation input.

use std::ops::Range;

use super::{Error, Icf};

const WORKING_BYTES: usize = 0x4997a;
const GROUP_COUNT: usize = 0x3fb40;
const COUNTS: usize = 0x3fb44;
const MAP: usize = 0x3fcd4;
const FLAGS: usize = 0x3662d;
const EMPTY: u32 = 0xffff;

/// Owned expanded memory model. Opaque storage bytes survive all map operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalMemory {
    working: Vec<u8>,
    original: Vec<u8>,
    comment: Vec<u8>,
    extra_data: u32,
}

/// Allocation's actual result, not an all-or-nothing success flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allocation {
    /// Requested logical row count, including already occupied rows.
    pub requested: usize,
    /// Newly allocated physical slots; preceding allocations are already committed.
    pub allocated: usize,
    /// No free slot remained while an empty requested row still needed one.
    pub exhausted: bool,
}

impl NormalMemory {
    /// Expand storage links/masks into the normal working map, without field edits.
    ///
    /// Uses the existing checked topology decoder. This is an explicit normal-
    /// topology API, not the original reader's corrupted-link repair policy.
    pub fn from_icf(image: &Icf<'_>) -> Result<Self, Error> {
        let groups = image.memory_groups()?;
        let mut working = vec![0; WORKING_BYTES];
        working[..image.memory.len()].copy_from_slice(&image.memory);
        for g in 0..100 {
            working[MAP + 400 * g..MAP + 400 * (g + 1)]
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .for_each(|cell| cell.copy_from_slice(&EMPTY.to_le_bytes()));
            let descriptor = 0x355c0 + 19 * g;
            working[descriptor + 3..descriptor + 19].fill(b' ');
        }
        let mut model = Self {
            working,
            original: image.original_bytes().to_vec(),
            comment: image.comment_bytes().to_vec(),
            extra_data: image.extra_data(),
        };
        model.set_word(GROUP_COUNT, groups.len() as u32);
        for (g, group) in groups.iter().enumerate() {
            let name = 0x355c3 + 19 * g;
            model.working[name..name + 16].copy_from_slice(group.name_bytes());
            for (row, slot) in group.slots().iter().enumerate() {
                model.set_word(MAP + 400 * g + 4 * row, slot.map_or(EMPTY, |p| p as u32));
            }
            model.set_word(
                COUNTS + 4 * g,
                group.slots().iter().flatten().count() as u32,
            );
        }
        model.validate()?;
        Ok(model)
    }

    /// Adopt an explicit expanded image and check maps/counts/flags coherently.
    /// Unknown physical field values and saved free links do not prevent editing.
    pub fn from_working(working: Vec<u8>) -> Result<Self, Error> {
        let model = Self {
            working,
            original: Vec::new(),
            comment: Vec::new(),
            extra_data: 0,
        };
        model.validate()?;
        Ok(model)
    }

    /// Borrow the expanded image; no mutable unchecked raw slice is exposed.
    pub fn working(&self) -> &[u8] {
        &self.working
    }

    /// Current working group count, including explicitly appended blank groups.
    pub fn group_count(&self) -> usize {
        self.word(GROUP_COUNT) as usize
    }

    /// Physical usage from all 100 group counts, as B37AC computes it.
    pub fn used_slots(&self) -> usize {
        (0..100).map(|g| self.word(COUNTS + 4 * g) as usize).sum()
    }

    /// Read one physical slot mapping; empty rows remain distinct from zero.
    pub fn slot(&self, group: usize, row: usize) -> Result<Option<usize>, Error> {
        self.range(
            group,
            row..row.checked_add(1).ok_or_else(|| invalid("row overflow"))?,
        )?;
        let value = self.word(MAP + 400 * group + 4 * row);
        Ok((value != EMPTY).then_some(value as usize))
    }

    /// Borrow a physical record, including both stored sides and the raw name.
    pub fn record(&self, group: usize, row: usize) -> Result<Option<&[u8]>, Error> {
        Ok(self
            .slot(group, row)?
            .map(|p| &self.working[83 * p..83 * (p + 1)]))
    }

    /// Allocate only empty rows, choosing the lowest free flag each time.
    ///
    /// Capacity exhaustion returns the committed prefix. Existing record bytes
    /// are not initialized; use an explicit decoded/template writer afterwards.
    pub fn allocate(&mut self, group: usize, rows: Range<usize>) -> Result<Allocation, Error> {
        self.range(group, rows.clone())?;
        let mut result = Allocation {
            requested: rows.len(),
            allocated: 0,
            exhausted: false,
        };
        let capacity = 500 - self.used_slots();
        for row in rows {
            let at = MAP + 400 * group + 4 * row;
            if self.word(at) != EMPTY {
                continue;
            }
            if result.allocated == capacity {
                result.exhausted = true;
                break;
            }
            let Some(p) = self.working[FLAGS..FLAGS + 500]
                .iter()
                .position(|b| b & 1 != 0)
            else {
                result.exhausted = true;
                break;
            };
            self.working[FLAGS + p] &= !1;
            self.set_word(at, p as u32);
            result.allocated += 1;
        }
        self.set_word(
            COUNTS + 4 * group,
            self.word(COUNTS + 4 * group) + result.allocated as u32,
        );
        Ok(result)
    }

    /// B38B8: release rows without shifting, erasing records or changing other flag bits.
    /// Returns the actual released slot count; zero is still a completed operation.
    pub fn release(&mut self, group: usize, rows: Range<usize>) -> Result<usize, Error> {
        self.range(group, rows.clone())?;
        let mut released = 0;
        for row in rows {
            let at = MAP + 400 * group + 4 * row;
            let p = self.word(at);
            if p != EMPTY {
                self.working[FLAGS + p as usize] |= 1;
                self.set_word(at, EMPTY);
                released += 1;
            }
        }
        self.set_word(
            COUNTS + 4 * group,
            self.word(COUNTS + 4 * group) - released as u32,
        );
        Ok(released)
    }

    /// B3E1C: release selected rows, shift following maps up, fill the tail with holes.
    /// Physical bytes are never copied or erased, even when deleting empty rows.
    pub fn delete(&mut self, group: usize, rows: Range<usize>) -> Result<usize, Error> {
        self.range(group, rows.clone())?;
        let count = rows.len();
        let released = self.release(group, rows.clone())?;
        let base = MAP + 400 * group;
        self.working
            .copy_within(base + 4 * rows.end..base + 400, base + 4 * rows.start);
        self.fill_holes(group, 100 - count..100);
        Ok(released)
    }

    /// B3F18: release the pushed-out tail before shifting; do not allocate new slots.
    /// The caller supplies any required confirmation before invoking this method.
    pub fn insert(&mut self, group: usize, at: usize, count: usize) -> Result<usize, Error> {
        let end = at
            .checked_add(count)
            .ok_or_else(|| invalid("insert range overflow"))?;
        self.range(group, at..end)?;
        let released = self.release(group, 100 - count..100)?;
        let base = MAP + 400 * group;
        self.working
            .copy_within(base + 4 * at..base + 4 * (100 - count), base + 4 * end);
        self.fill_holes(group, at..end);
        Ok(released)
    }

    /// B3CD0: rotate an entire logical row block with its neighboring map entry.
    /// Negative means up; zero and positive mean down. A boundary move returns
    /// false without mutation; physical records, flags and counts never move.
    pub fn carry(
        &mut self,
        group: usize,
        rows: Range<usize>,
        direction: i32,
    ) -> Result<bool, Error> {
        self.range(group, rows.clone())?;
        if rows.is_empty()
            || (direction < 0 && rows.start == 0)
            || (direction >= 0 && rows.end == 100)
        {
            return Ok(false);
        }
        let base = MAP + 400 * group;
        if direction < 0 {
            let neighbor = self.word(base + 4 * (rows.start - 1));
            self.working.copy_within(
                base + 4 * rows.start..base + 4 * rows.end,
                base + 4 * (rows.start - 1),
            );
            self.set_word(base + 4 * (rows.end - 1), neighbor);
        } else {
            let neighbor = self.word(base + 4 * rows.end);
            self.working.copy_within(
                base + 4 * rows.start..base + 4 * rows.end,
                base + 4 * (rows.start + 1),
            );
            self.set_word(base + 4 * rows.start, neighbor);
        }
        Ok(true)
    }

    /// Add a working group before its name editor, without clearing its fields.
    /// False means the 100-group limit; a later name cancellation does not undo it.
    pub fn add_group(&mut self) -> bool {
        let count = self.group_count();
        if count == 100 {
            return false;
        }
        self.set_word(GROUP_COUNT, count as u32 + 1);
        true
    }

    /// Replace a group name with already-decoded editor bytes, space padded.
    /// `None` is cancellation. Placeholder addition occurs before cancellation,
    /// as in 0x686B7; the supplied candidate is not an implicit text-codec fallback.
    pub fn rename_group(&mut self, group: usize, candidate: Option<&[u8]>) -> Result<bool, Error> {
        if group > self.group_count() || group >= 100 {
            return Err(invalid("group outside current or placeholder"));
        }
        if candidate.is_some_and(|name| name.len() > 16) {
            return Err(invalid("decoded group name exceeds 16 bytes"));
        }
        if group == self.group_count() && !self.add_group() {
            return Ok(false);
        }
        let Some(name) = candidate else {
            return Ok(false);
        };
        let at = 0x355c3 + 19 * group;
        self.working[at..at + 16].fill(b' ');
        self.working[at..at + name.len()].copy_from_slice(name);
        Ok(true)
    }

    /// Rebuild saved links/masks in B540C order, preserving all other storage.
    /// Low 100 row bits change; the final four mask bits per group remain opaque.
    pub fn to_icf<'a>(&'a self) -> Result<Icf<'a>, Error> {
        self.validate()?;
        let mut memory = self.working[..0x3c9c0].to_vec();
        memory[0x36628] = 0;
        for g in 0..100 {
            let descriptor = 0x355c0 + 19 * g;
            memory[descriptor] = if g == 99 { 0xff } else { g as u8 + 1 };
            let occupied: Vec<_> = (0..100)
                .filter_map(|row| {
                    let p = self.word(MAP + 400 * g + 4 * row);
                    let mask = 0x36114 + 13 * g + row / 8;
                    let bit = 1 << (row % 8);
                    if p == EMPTY {
                        memory[mask] |= bit;
                        None
                    } else {
                        memory[mask] &= !bit;
                        Some(p as u16)
                    }
                })
                .collect();
            memory[descriptor + 1..descriptor + 3]
                .copy_from_slice(&occupied.first().copied().unwrap_or(u16::MAX).to_le_bytes());
            for (index, &p) in occupied.iter().enumerate() {
                let at = 0x35d2c + 2 * usize::from(p);
                memory[at..at + 2].copy_from_slice(
                    &occupied
                        .get(index + 1)
                        .copied()
                        .unwrap_or(u16::MAX)
                        .to_le_bytes(),
                );
            }
        }
        let free: Vec<_> = (0..500)
            .filter(|&p| memory[FLAGS + p] & 1 != 0)
            .map(|p| p as u16)
            .collect();
        memory[0x36629..0x3662b]
            .copy_from_slice(&free.first().copied().unwrap_or(u16::MAX).to_le_bytes());
        memory[0x3662b..0x3662d]
            .copy_from_slice(&free.last().copied().unwrap_or(u16::MAX).to_le_bytes());
        for (index, &p) in free.iter().enumerate() {
            let at = 0x35d2c + 2 * usize::from(p);
            memory[at..at + 2].copy_from_slice(
                &free
                    .get(index + 1)
                    .copied()
                    .unwrap_or(u16::MAX)
                    .to_le_bytes(),
            );
        }
        let mut image: Icf<'a> = Icf::from_memory(memory)?;
        image.source = &self.original;
        image.comment = &self.comment;
        image.extra_data = self.extra_data;
        Ok(image)
    }

    fn validate(&self) -> Result<(), Error> {
        if self.working.len() != WORKING_BYTES {
            return Err(invalid("normal working image requires 0x4997a bytes"));
        }
        if self.group_count() > 100 {
            return Err(invalid("working group count exceeds 100"));
        }
        let mut seen = [false; 500];
        for g in 0..100 {
            let mut count = 0;
            for r in 0..100 {
                let p = self.word(MAP + 400 * g + 4 * r);
                if p == EMPTY {
                    continue;
                }
                if p >= 500 || seen[p as usize] || self.working[FLAGS + p as usize] & 1 != 0 {
                    return Err(invalid("working map is invalid, repeated or marked empty"));
                }
                seen[p as usize] = true;
                count += 1;
            }
            if self.word(COUNTS + 4 * g) != count {
                return Err(invalid("working group count differs from map"));
            }
        }
        if seen
            .iter()
            .enumerate()
            .any(|(p, &used)| used != (self.working[FLAGS + p] & 1 == 0))
        {
            return Err(invalid("occupied physical slot has no working row"));
        }
        Ok(())
    }

    fn range(&self, group: usize, rows: Range<usize>) -> Result<(), Error> {
        if group >= self.group_count() || rows.start > rows.end || rows.end > 100 {
            Err(invalid("logical group or row range outside normal map"))
        } else {
            Ok(())
        }
    }
    fn word(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.working[at..at + 4].try_into().unwrap())
    }
    fn set_word(&mut self, at: usize, value: u32) {
        self.working[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn fill_holes(&mut self, group: usize, rows: Range<usize>) {
        for row in rows {
            self.set_word(MAP + 400 * group + 4 * row, EMPTY);
        }
    }
}

fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 0, reason }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> NormalMemory {
        let mut raw = vec![0xa6; 0x4997a];
        raw[0x355a1] = 6;
        raw[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
        raw[0x3fb40..0x3fb44].copy_from_slice(&2_u32.to_le_bytes());
        raw[0x3fb44..0x3fcd4].fill(0);
        for cell in raw[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
            cell.copy_from_slice(&0xffff_u32.to_le_bytes());
        }
        raw[0x3662d..0x36821].fill(0xa7);
        NormalMemory::from_working(raw).unwrap()
    }

    #[test]
    fn logical_release_delete_insert_and_carry_do_not_move_or_zero_records() {
        let mut m = model();
        assert_eq!(
            m.allocate(0, 1..4).unwrap(),
            Allocation {
                requested: 3,
                allocated: 3,
                exhausted: false
            }
        );
        let physical = m.working[..0xa21c].to_vec();
        assert_eq!(m.slot(0, 1).unwrap(), Some(0));
        assert_eq!(m.slot(0, 2).unwrap(), Some(1));
        assert_eq!(m.release(0, 2..3).unwrap(), 1);
        assert_eq!(m.slot(0, 3).unwrap(), Some(2));
        assert_eq!(m.working[0x3662e], 0xa7);
        assert_eq!(m.delete(0, 2..3).unwrap(), 0);
        assert_eq!(m.slot(0, 2).unwrap(), Some(2));
        assert_eq!(m.insert(0, 1, 2).unwrap(), 0);
        assert_eq!(m.slot(0, 3).unwrap(), Some(0));
        assert_eq!(m.slot(0, 4).unwrap(), Some(2));
        assert!(m.carry(0, 3..5, -1).unwrap());
        assert_eq!(m.slot(0, 2).unwrap(), Some(0));
        assert_eq!(m.slot(0, 3).unwrap(), Some(2));
        assert_eq!(&m.working[..0xa21c], physical);
        let before = m.clone();
        assert!(m.insert(0, 99, 2).is_err());
        assert!(m.slot(0, usize::MAX).is_err());
        assert!(!m.carry(0, 0..1, -1).unwrap());
        assert_eq!(m, before);
        let image = m.to_icf().unwrap();
        assert_eq!(&image.memory()[0x355c1..0x355c3], &[0, 0]);
        assert_eq!(&image.memory()[0x35d2c..0x35d2e], &[2, 0]);
        assert_eq!(image.memory()[0x36120] & 0xf0, 0xa0);
        let restored = NormalMemory::from_icf(&image).unwrap();
        assert_eq!(restored.slot(0, 2).unwrap(), Some(0));
        assert_eq!(restored.slot(0, 3).unwrap(), Some(2));
        let source = image.to_bytes();
        let parsed = Icf::parse(&source).unwrap();
        let expanded = NormalMemory::from_icf(&parsed).unwrap();
        let saved = expanded.to_icf().unwrap();
        assert_eq!(saved.original_bytes(), source);
        assert_eq!(saved.comment_bytes(), parsed.comment_bytes());
        assert_eq!(saved.extra_data(), parsed.extra_data());
    }

    #[test]
    fn lowest_free_partial_capacity_and_placeholder_cancel_preserve_committed_prefix() {
        let mut m = model();
        for _ in 2..6 {
            assert!(m.add_group());
        }
        for g in 0..4 {
            assert_eq!(m.allocate(g, 0..100).unwrap().allocated, 100);
        }
        assert_eq!(m.allocate(4, 0..99).unwrap().allocated, 99);
        assert_eq!(
            m.allocate(5, 7..10).unwrap(),
            Allocation {
                requested: 3,
                allocated: 1,
                exhausted: true
            }
        );
        assert_eq!(m.slot(5, 7).unwrap(), Some(499));
        assert_eq!(m.slot(5, 8).unwrap(), None);
        assert_eq!(m.used_slots(), 500);
        m.release(0, 3..4).unwrap();
        assert_eq!(m.allocate(5, 9..10).unwrap().allocated, 1);
        assert_eq!(m.slot(5, 9).unwrap(), Some(3));
        assert_eq!(m.insert(5, 0, 93).unwrap(), 2);
        assert_eq!(m.used_slots(), 498);
        let count = m.group_count();
        let name_before = m.working[0x355c3 + 19 * count..0x355d3 + 19 * count].to_vec();
        assert!(!m.rename_group(count, None).unwrap());
        assert_eq!(m.group_count(), count + 1);
        assert_eq!(
            &m.working[0x355c3 + 19 * count..0x355d3 + 19 * count],
            name_before
        );
    }

    #[test]
    fn group_count_is_last_nonempty_ordinal_not_descriptor_count_or_nonempty_count() {
        let mut raw = vec![0xa6; 0x3c9c0];
        raw[0x355a1] = 6;
        raw[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        raw[0x3662d..0x36821].fill(0xa7);
        raw[0x36114..0x36628].fill(0xff);
        let order: Vec<_> = (0..100).map(|g| (37 * g + 19) % 100).collect();
        raw[0x36628] = order[0] as u8;
        for (g, &d) in order.iter().enumerate() {
            let at = 0x355c0 + 19 * d;
            raw[at] = order.get(g + 1).copied().map_or(0xff, |next| next as u8);
            raw[at + 1..at + 3].copy_from_slice(&[0xff, 0xff]);
            raw[at + 3..at + 19].fill(b' ');
        }
        let blank = Icf::from_memory(raw.clone()).unwrap();
        let mut all_blank = NormalMemory::from_icf(&blank).unwrap();
        assert_eq!(all_blank.group_count(), 0);
        assert!(all_blank.add_group());
        assert_eq!(all_blank.group_count(), 1);
        for ordinal in [3, 8] {
            raw[0x355c3 + 19 * order[ordinal]] = b'A' + ordinal as u8;
        }
        let image = Icf::from_memory(raw.clone()).unwrap();
        let mut middle_empty = NormalMemory::from_icf(&image).unwrap();
        assert_eq!(middle_empty.group_count(), 9);
        assert_eq!(middle_empty.working()[0x355c3 + 19 * 3], b'D');
        assert_eq!(middle_empty.working()[0x355c3 + 19 * 8], b'I');
        assert_eq!(
            &middle_empty.working()[0x355c3 + 19 * 4..0x355d3 + 19 * 4],
            &[b' '; 16]
        );
        assert!(!middle_empty.rename_group(9, None).unwrap());
        assert_eq!(middle_empty.group_count(), 10);
        raw[0x355c3 + 19 * order[99]] = b'Z';
        let last = Icf::from_memory(raw).unwrap();
        let mut full = NormalMemory::from_icf(&last).unwrap();
        assert_eq!(full.group_count(), 100);
        assert!(!full.add_group());
    }
}
