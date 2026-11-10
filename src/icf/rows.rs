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

use super::{Error, Icf, normalizer::ReferenceImage};


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

fn raw<const N: usize>(image: &[u8], at: usize) -> Result<[u8; N], Error> {
    image
        .get(
            at..at
                .checked_add(N)
                .ok_or(invalid("normal metadata address overflow"))?,
        )
        .ok_or(invalid("normal metadata window unavailable"))?
        .try_into()
        .map_err(|_| invalid("normal metadata width"))
}
fn put(image: &mut [u8], at: usize, bytes: &[u8]) -> Result<(), Error> {
    super::image::apply_patches(image, &[(at, bytes)])
}
fn descriptor(index: u32) -> usize {
    0x355c0_u32.wrapping_add(19_u32.wrapping_mul(index)) as usize
}

/// Borrowed B3800 allocation leaf. Only range, counts and accessed windows are
/// checked; no Icf reconstruction, whole-map validation or record reset occurs.
pub fn allocate_into(
    image: &mut [u8],
    group: usize,
    rows: Range<usize>,
) -> Result<Allocation, Error> {
    if group >= 100 || rows.start > rows.end || rows.end > 100 {
        return Err(invalid("normal allocation range outside map"));
    }
    let mut result = Allocation {
        requested: rows.len(),
        allocated: 0,
        exhausted: false,
    };
    let mut used = 0_u64;
    for g in 0..100 {
        used += u64::from(u32::from_le_bytes(raw(image, COUNTS + 4 * g)?));
    }
    let capacity = 500_u64
        .checked_sub(used)
        .ok_or(invalid("normal stored counts exceed physical capacity"))?
        as usize;
    for row in rows {
        let at = MAP + 400 * group + 4 * row;
        if u32::from_le_bytes(raw(image, at)?) != EMPTY {
            continue;
        }
        if result.allocated == capacity {
            result.exhausted = true;
            break;
        }
        let Some(p) = image
            .get(FLAGS..FLAGS + 500)
            .ok_or(invalid("normal allocation flags unavailable"))?
            .iter()
            .position(|b| b & 1 != 0)
        else {
            result.exhausted = true;
            break;
        };
        image[FLAGS + p] &= !1;
        put(image, at, &(p as u32).to_le_bytes())?;
        result.allocated += 1;
    }
    let count = u32::from_le_bytes(raw(image, COUNTS + 4 * group)?);
    put(
        image,
        COUNTS + 4 * group,
        &(count + result.allocated as u32).to_le_bytes(),
    )?;
    Ok(result)
}

/// Shared B4AC0 storage leaf. The caller owns the actual cache byte; no GUI
/// notification or successful callback is manufactured by this raw operation.
pub fn reset_home_into(
    image: &mut [u8],
    group: usize,
    defaults: ReferenceImage<'_>,
    home_invalidated: &mut u8,
) -> Result<(), Error> {
    if group >= 100 {
        return Err(invalid("normal Home group outside0..100"));
    }
    let at = 0x36821 + 2 * group;
    let word = raw::<2>(defaults.get(image), at)?;
    put(image, at, &word)?;
    if usize::from(raw::<1>(image, 0x3c84b)?[0]) == group {
        put(image, 0x3c84c, &word)?;
        *home_invalidated = 1;
    }
    Ok(())
}

fn release_map_into(image: &mut [u8], group: usize, rows: Range<usize>) -> Result<usize, Error> {
    if group >= 100 || rows.start > rows.end || rows.end > 100 {
        return Err(invalid("normal release range outside map"));
    }
    let mut released = 0;
    for row in rows {
        let at = MAP + 400 * group + 4 * row;
        let p = u32::from_le_bytes(raw(image, at)?);
        if p != EMPTY {
            let flag = (FLAGS as u32).wrapping_add(p) as usize;
            put(image, flag, &[raw::<1>(image, flag)?[0] | 1])?;
            put(image, at, &EMPTY.to_le_bytes())?;
            released += 1;
        }
    }
    let count = u32::from_le_bytes(raw(image, COUNTS + 4 * group)?).wrapping_sub(released as u32);
    put(image, COUNTS + 4 * group, &count.to_le_bytes())?;
    Ok(released)
}

/// Borrowed B38B8 for the document's current working image. The same map leaf
/// serves NormalMemory; physical addresses are checked, not clamped to p<500.
pub fn release_into(
    image: &mut [u8],
    group: usize,
    rows: Range<usize>,
    defaults: ReferenceImage<'_>,
    home_invalidated: &mut u8,
) -> Result<usize, Error> {
    let count = release_map_into(image, group, rows)?;
    if count != 0 {
        reset_home_into(image, group, defaults, home_invalidated)?;
    }
    Ok(count)
}

/// D1CB4's shared full-capacity typed UR shift, literal tail, count then live-F
/// reference reset. No Icf reconstruction or signature gate belongs in this phase.
pub fn delete_ur_into(
    image: &mut [u8],
    index: u32,
    defaults: ReferenceImage<'_>,
) -> Result<(), Error> {
    let count = u16::from_le_bytes(raw(image, 0x49978)?);
    for destination in index..299 {
        let mut holder = super::record::reset_holder(0x0d, destination + 1, 1);
        super::record_image::load_callsign_into(image, destination + 1, &mut holder)?;
        super::record_image::store_callsign_into(image, destination, &holder)?;
    }
    let tail = index.max(299); // Native j>=299 stores the supplied j, not a clamp.
    super::record_image::store_callsign_into(image, tail, &ur_default(tail, 2))?;
    put(image, 0x49978, &count.wrapping_sub(1).to_le_bytes())?;
    let value = raw::<2>(defaults.get(image), 0x3c901)?;
    put(image, 0x3c901, &value)
}

/// B5170's bounded descriptor walk and working-map expansion. This is the native
/// storage boundary, unlike NormalMemory::from_icf's strict topology view. It
/// does not reject repeated descriptors, contradictory flags or p>=500. Address
/// arithmetic stays32-bit; only unavailable actual image windows fail safely.
pub fn expand_map(image: &mut [u8]) -> Result<(), Error> {
    let mut order = [u32::MAX; 100];
    let mut next = raw::<1>(image, 0x36628)?[0];
    let mut groups = 0_u32;
    for (ordinal, slot) in order.iter_mut().enumerate() {
        if next == 0xff {
            break;
        }
        *slot = u32::from(next);
        let at = descriptor(*slot);
        if raw::<2>(image, at + 1)? != u16::MAX.to_le_bytes()
            || raw::<16>(image, at + 3)? != [b' '; 16]
        {
            groups = ordinal as u32 + 1;
        }
        next = raw::<1>(image, at)?[0];
    }
    put(image, GROUP_COUNT, &groups.to_le_bytes())?;
    for (group, &source) in order.iter().enumerate() {
        let mut physical = u16::from_le_bytes(raw(image, descriptor(source) + 1)?);
        let mut count = 0_u32;
        for row in 0..100 {
            let value = if source == 0xff
                || physical == 0xffff
                || raw::<1>(image, 0x36114 + 13 * group + row / 8)?[0] & (1 << (row % 8)) != 0
            {
                EMPTY
            } else {
                u32::from(physical)
            };
            put(image, MAP + 400 * group + 4 * row, &value.to_le_bytes())?;
            if value != EMPTY {
                physical = u16::from_le_bytes(raw(image, 0x35d2c + 2 * usize::from(physical))?);
                count += 1;
            }
        }
        put(image, COUNTS + 4 * group, &count.to_le_bytes())?;
    }
    Ok(())
}

/// B5334 takes its own later snapshot. Only reached descriptor names are copied;
/// unvisited names are not silently padded, and cycles stop after100 iterations.
pub fn expand_names(image: &mut [u8]) -> Result<(), Error> {
    let snapshot = image.to_vec();
    let mut next = raw::<1>(&snapshot, 0x36628)?[0];
    for ordinal in 0..100 {
        if next == 0xff {
            break;
        }
        let at = descriptor(u32::from(next));
        put(
            image,
            descriptor(ordinal) + 3,
            &raw::<16>(&snapshot, at + 3)?,
        )?;
        next = raw::<1>(&snapshot, at)?[0];
    }
    Ok(())
}

/// B540C storage reconstruction on the actual working image, not a canonical
/// strict Icf roundtrip. Root/descriptor links, occupied chains, free chain and
/// finally rowmask are committed in original order. Reserved rowmask bits stay.
pub fn rebuild_metadata(image: &mut [u8]) -> Result<(), Error> {
    put(image, 0x36628, &[0])?;
    for g in 0..100 {
        put(
            image,
            descriptor(g),
            &[if g == 99 { 0xff } else { g as u8 + 1 }],
        )?;
    }
    let mut total = 0_u32;
    for g in 0..100 {
        let count = u32::from_le_bytes(raw(image, COUNTS + 4 * g)?);
        total = total.wrapping_add(count);
        let mut occupied = Vec::new();
        for row in 0..100 {
            let p = u32::from_le_bytes(raw(image, MAP + 400 * g + 4 * row)?);
            if p != EMPTY {
                occupied.push(p);
            }
        }
        if occupied.len() != count as usize {
            return Err(invalid(
                "normal working count/map reads uninitialized native vector",
            ));
        }
        put(
            image,
            descriptor(g as u32) + 1,
            &(occupied.first().copied().unwrap_or(EMPTY) as u16).to_le_bytes(),
        )?;
        for (i, &p) in occupied.iter().enumerate() {
            let at = 0x35d2c_u32.wrapping_add(p.wrapping_mul(2)) as usize;
            put(
                image,
                at,
                &(occupied.get(i + 1).copied().unwrap_or(EMPTY) as u16).to_le_bytes(),
            )?;
        }
    }
    if total == 500 {
        put(image, 0x36629, &u16::MAX.to_le_bytes())?;
        put(image, 0x3662b, &u16::MAX.to_le_bytes())?;
    } else {
        let mut free = Vec::new();
        for p in 0..500 {
            if raw::<1>(image, FLAGS + p)?[0] & 1 != 0 {
                free.push(p as u16);
            }
        }
        if free.is_empty() {
            return Err(invalid("normal free chain reads empty native vector"));
        }
        put(image, 0x36629, &free[0].to_le_bytes())?;
        if free.len() != 500_usize.wrapping_sub(total as usize) {
            return Err(invalid(
                "normal free count/flag mismatch reads uninitialized native vector",
            ));
        }
        for (i, &p) in free.iter().enumerate() {
            put(
                image,
                0x35d2c + 2 * usize::from(p),
                &free.get(i + 1).copied().unwrap_or(u16::MAX).to_le_bytes(),
            )?;
        }
        put(image, 0x3662b, &free.last().copied().unwrap().to_le_bytes())?;
    }
    for g in 0..100 {
        for row in 0..100 {
            let empty = u32::from_le_bytes(raw(image, MAP + 400 * g + 4 * row)?) == EMPTY;
            let at = 0x36114 + 13 * g + row / 8;
            let bit = 1 << (row % 8);
            let old = raw::<1>(image, at)?[0];
            put(image, at, &[(old & !bit) | if empty { bit } else { 0 }])?;
        }
    }
    Ok(())
}

/// 610C type23: common reset plus the native eight-byte code literal59064A.
/// This is an instruction/string recipe, not an externally supplied holder/G.
pub fn ur_default(index: u32, binding: u32) -> super::record::DecodedRecord {
    let mut holder = super::record::reset_holder(0x23, index, binding);
    holder.as_mut_bytes()[0x88..0x90].copy_from_slice(b"CALLSIGN");
    holder
}

/// C61B4: prefix count stops only atFFFF, not at an out-of-table physical index.
pub fn expand_ur_count(image: &mut [u8]) -> Result<(), Error> {
    let mut count = 0_u16;
    for row in 0..300 {
        if raw::<2>(image, 0x3c0a7 + 2 * row)? == u16::MAX.to_le_bytes() {
            break;
        }
        count += 1;
    }
    put(image, 0x49978, &count.to_le_bytes())
}

/// C61F0's separate post-count snapshot and all300 typed load/body/save/reloads.
/// An invalid order entry generates type23, never an empty/zero record fallback.
pub fn expand_ur_data(image: &mut [u8]) -> Result<(), Error> {
    let snapshot = image.to_vec();
    for index in 0..300_u32 {
        let p = u16::from_le_bytes(raw(&snapshot, 0x3c0a7 + 2 * index as usize)?);
        let mut source = ur_default(index, 2);
        if p < 300 {
            super::record_image::load_callsign_into(&snapshot, u32::from(p), &mut source)?;
        }
        let mut target = super::record::reset_holder(0x0d, index, 1);
        super::record_image::load_callsign_into(image, index, &mut target)?;
        target.copy_body_from(&source);
        super::record_image::store_callsign_into(image, index, &target)?;
        super::record_image::load_callsign_into(image, index, &mut target)?;
    }
    Ok(())
}

/// C637C: identity order below the live cached count,FFFF thereafter.
pub fn rebuild_ur_metadata(image: &mut [u8]) -> Result<(), Error> {
    let count = u16::from_le_bytes(raw(image, 0x49978)?);
    for index in 0..300_u16 {
        put(
            image,
            0x3c0a7 + 2 * usize::from(index),
            &if index < count { index } else { u16::MAX }.to_le_bytes(),
        )?;
    }
    Ok(())
}

/// Owned UR working view used by CLI/CSV. Full image metadata belongs to the
/// caller; these methods preserve source/comment/EtcData and unrelated payloads.
#[derive(Debug, Clone)]
pub struct WorkingUr {
    memory: Vec<u8>,
    original: Vec<u8>,
    comment: Vec<u8>,
    extra_data: u32,
}
impl WorkingUr {
    pub fn from_icf(image: &Icf<'_>) -> Result<Self, Error> {
        let mut memory = image.memory().to_vec();
        memory.resize(WORKING_BYTES, 0);
        expand_ur_count(&mut memory)?;
        expand_ur_data(&mut memory)?;
        Ok(Self {
            memory,
            original: image.original_bytes().to_vec(),
            comment: image.comment_bytes().to_vec(),
            extra_data: image.extra_data(),
        })
    }
    pub fn working(&self) -> &[u8] {
        &self.memory
    }
    pub fn count(&self) -> u16 {
        u16::from_le_bytes(self.memory[0x49978..0x4997a].try_into().unwrap())
    }
    pub fn load_into(
        &self,
        index: u32,
        holder: &mut super::record::DecodedRecord,
    ) -> Result<bool, Error> {
        super::record_image::load_callsign_into(&self.memory, index, holder)
    }
    pub fn save(&mut self, index: u32, holder: &super::record::DecodedRecord) -> Result<(), Error> {
        super::record_image::store_callsign_into(&mut self.memory, index, holder)
    }
    fn reset_reference(
        &mut self,
        defaults: super::normalizer::ReferenceImage<'_>,
    ) -> Result<(), Error> {
        let value = raw::<2>(defaults.get(&self.memory), 0x3c901)?;
        put(&mut self.memory, 0x3c901, &value)
    }
    /// D1864. The caller's editor/confirmation is later, so cancellation must not
    /// restore this typed shift, literal tail, count or reference reset.
    pub fn insert_one(
        &mut self,
        index: u32,
        defaults: super::normalizer::ReferenceImage<'_>,
    ) -> Result<bool, Error> {
        let count = self.count();
        if count >= 300 {
            return Ok(false);
        }
        if index >= 300 {
            return Err(invalid("UR insert index outside capacity"));
        }
        for destination in (index + 1..300).rev() {
            let mut h = super::record::reset_holder(0x0d, destination - 1, 1);
            self.load_into(destination - 1, &mut h)?;
            self.save(destination, &h)?;
        }
        self.save(index, &ur_default(index, 2))?;
        put(
            &mut self.memory,
            0x49978,
            &count.wrapping_add(1).to_le_bytes(),
        )?;
        self.reset_reference(defaults)?;
        Ok(true)
    }
    pub fn append(
        &mut self,
        defaults: super::normalizer::ReferenceImage<'_>,
    ) -> Result<bool, Error> {
        self.insert_one(u32::from(self.count()), defaults)
    }
    /// D1CB4 shifts the entire capacity tail, not only active rows. Count0 still
    /// wraps toFFFF if explicitly called, as in the native low-level helper.
    pub fn delete_one(
        &mut self,
        index: u32,
        defaults: super::normalizer::ReferenceImage<'_>,
    ) -> Result<(), Error> {
        if index >= 300 {
            return Err(invalid("UR delete index outside capacity"));
        }
        delete_ur_into(&mut self.memory, index, defaults)
    }
    pub fn to_icf(&self) -> Result<Icf<'_>, Error> {
        let mut memory = self.memory.clone();
        rebuild_ur_metadata(&mut memory)?;
        memory.truncate(0x3c9c0);
        let mut image = Icf::from_memory(memory)?;
        image.source = &self.original;
        image.comment = &self.comment;
        image.extra_data = self.extra_data;
        Ok(image)
    }
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

    /// Save an already decoded holder to a logical typed side, preserving this
    /// model's original file/comment/EtcData. Uses the one shared ordered codec.
    /// No allocation, side initialization, map release or reload is implied.
    /// A valid0 raw save can mark the physical slot empty; its caller must perform
    /// the original map/release operation separately before storage reconstruction.
    pub fn store_channel(
        &mut self,
        index: u32,
        holder: &mut super::record::DecodedRecord,
    ) -> Result<(), Error> {
        super::record_image::store_channel_into(
            &mut self.working,
            super::record_image::ChannelKind::Normal,
            index,
            holder,
            |_| Ok(()),
        )
    }

    /// Current working group count, including explicitly appended blank groups.
    pub fn group_count(&self) -> usize {
        self.word(GROUP_COUNT) as usize
    }

    /// B37EC reads the stored DWORD even beyond displayed group_count. Do not
    /// recount slots or silently repair an inconsistent working generation.
    pub fn group_used_slots(&self, group: usize) -> Result<usize, Error> {
        if group >= 100 {
            return Err(invalid("normal stored group index outside0..100"));
        }
        Ok(self.word(COUNTS + 4 * group) as usize)
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
        allocate_into(&mut self.working, group, rows)
    }

    /// B38B8: release rows without shifting, erasing records or changing other flag bits.
    /// Returns the actual released slot count; zero is still a completed operation.
    pub fn release(&mut self, group: usize, rows: Range<usize>) -> Result<usize, Error> {
        self.range(group, rows.clone())?;
        release_map_into(&mut self.working, group, rows)
    }

    /// B4AC0: read G's group word at the actual reset point. A matching current
    /// Home group receives that SAME word, not G's global Home row. Only the
    /// matching branch writes the external VA864930 byte to1.
    pub fn reset_home(
        &mut self,
        group: usize,
        defaults: ReferenceImage<'_>,
        home_invalidated: &mut u8,
    ) -> Result<(), Error> {
        reset_home_into(&mut self.working, group, defaults, home_invalidated)
    }

    /// Native B38B8 tail, unlike the map-only release primitive. Missing G fails
    /// after release/count, and only a nonzero removal reaches the reset read.
    pub fn release_with_home(
        &mut self,
        group: usize,
        rows: Range<usize>,
        defaults: ReferenceImage<'_>,
        home_invalidated: &mut u8,
    ) -> Result<usize, Error> {
        let released = self.release(group, rows)?;
        if released != 0 {
            self.reset_home(group, defaults, home_invalidated)?;
        }
        Ok(released)
    }

    /// B3E1C: release selected rows, shift following maps up, fill the tail with holes.
    /// Physical bytes are never copied or erased, even when deleting empty rows.
    pub fn delete(&mut self, group: usize, rows: Range<usize>) -> Result<usize, Error> {
        self.range(group, rows.clone())?;
        let count = rows.len();
        let released = self.release(group, rows.clone())?;
        self.delete_maps(group, rows, count);
        Ok(released)
    }

    fn delete_maps(&mut self, group: usize, rows: Range<usize>, count: usize) {
        let base = MAP + 400 * group;
        self.working
            .copy_within(base + 4 * rows.end..base + 400, base + 4 * rows.start);
        self.fill_holes(group, 100 - count..100);
    }

    /// B3E1C: conditional Home reset is INSIDE release, before the map shift.
    /// Deleting holes still shifts maps but does not read G or invalidate Home.
    pub fn delete_with_home(
        &mut self,
        group: usize,
        rows: Range<usize>,
        defaults: ReferenceImage<'_>,
        home_invalidated: &mut u8,
    ) -> Result<usize, Error> {
        self.range(group, rows.clone())?;
        let count = rows.len();
        let released = self.release_with_home(group, rows.clone(), defaults, home_invalidated)?;
        self.delete_maps(group, rows, count);
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
        self.insert_maps(group, at, end, count);
        Ok(released)
    }

    fn insert_maps(&mut self, group: usize, at: usize, end: usize, count: usize) {
        let base = MAP + 400 * group;
        self.working
            .copy_within(base + 4 * at..base + 4 * (100 - count), base + 4 * end);
        self.fill_holes(group, at..end);
    }

    /// B3F18: tail release may reset Home before shifting; the unconditional
    /// post-shift reset is a separate read, even when the tail held only holes.
    pub fn insert_with_home(
        &mut self,
        group: usize,
        at: usize,
        count: usize,
        defaults: ReferenceImage<'_>,
        home_invalidated: &mut u8,
    ) -> Result<usize, Error> {
        let end = at
            .checked_add(count)
            .ok_or(invalid("insert range overflow"))?;
        self.range(group, at..end)?;
        let released =
            self.release_with_home(group, 100 - count..100, defaults, home_invalidated)?;
        self.insert_maps(group, at, end, count);
        self.reset_home(group, defaults, home_invalidated)?;
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

    /// Native step2 move: rotate maps first, then reset Home even for all holes.
    /// A boundary/no-selection result does not read G or change the cache byte.
    /// This is not the step1 decoded-clipboard or actionD5 reference runner.
    pub fn carry_with_home(
        &mut self,
        group: usize,
        rows: Range<usize>,
        direction: i32,
        defaults: ReferenceImage<'_>,
        home_invalidated: &mut u8,
    ) -> Result<bool, Error> {
        let moved = self.carry(group, rows, direction)?;
        if moved {
            self.reset_home(group, defaults, home_invalidated)?;
        }
        Ok(moved)
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

    /// Apply the name editor's byte normalization, with placeholder addition
    /// before the editor/cancellation boundary. Shared input sanitation is not
    /// rolled back. The bool reports name acceptance, not group append or native
    /// notification delivery.
    pub fn edit_group_name(
        &mut self,
        group: usize,
        input: Option<&mut [u8]>,
    ) -> Result<bool, Error> {
        if group > self.group_count() || group >= 100 {
            return Err(invalid("group outside current or placeholder"));
        }
        if group == self.group_count() {
            self.add_group();
        }
        let Some(input) = input else {
            return Ok(false);
        };
        let candidate = super::normalizer::normalize_name(input, 16);
        self.rename_group(group, Some(&candidate))
    }

    /// Rebuild saved links/masks in B540C order, preserving all other storage.
    /// Low 100 row bits change; the final four mask bits per group remain opaque.
    pub fn to_icf<'a>(&'a self) -> Result<Icf<'a>, Error> {
        self.validate()?;
        let mut memory = self.working.clone();
        rebuild_metadata(&mut memory)?;
        memory.truncate(0x3c9c0);
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
    fn native_expansion_keeps_nonidentity_descriptors_sparse_rows_and_unclamped_map_values() {
        let mut image = model().to_icf().unwrap().memory().to_vec();
        image.resize(WORKING_BYTES, 0xa6);
        for g in 0..100 {
            image[0x355c1 + 19 * g..0x355c3 + 19 * g].fill(0xff);
            image[0x355c3 + 19 * g..0x355d3 + 19 * g].fill(b' ');
        }
        image[0x36114..0x36628].fill(0xff);
        image[0x36628] = 7;
        image[0x355c0 + 19 * 7] = 0;
        image[0x355c0 + 19 * 6] = 8;
        image[0x355c3 + 19 * 7..0x355d3 + 19 * 7].copy_from_slice(b"NONIDENTITY     ");
        image[0x355c1 + 19 * 7..0x355c3 + 19 * 7].copy_from_slice(&0xb1b_u16.to_le_bytes());
        image[0x35d2c + 2 * 0xb1b..0x35d2e + 2 * 0xb1b].fill(0xff);
        image[0x36114 + 23 / 8] &= !(1 << (23 % 8));
        expand_map(&mut image).unwrap();
        assert_eq!(u32::from_le_bytes(raw(&image, GROUP_COUNT).unwrap()), 1);
        assert_eq!(u32::from_le_bytes(raw(&image, COUNTS).unwrap()), 1);
        assert_eq!(
            u32::from_le_bytes(raw(&image, MAP + 23 * 4).unwrap()),
            0xb1b
        );
        assert_eq!(
            u32::from_le_bytes(raw(&image, MAP + 22 * 4).unwrap()),
            EMPTY
        );
        expand_names(&mut image).unwrap();
        assert_eq!(raw::<16>(&image, 0x355c3).unwrap(), *b"NONIDENTITY     ");
        assert_eq!(raw::<16>(&image, 0x355c3 + 19).unwrap(), [b' '; 16]);
    }

    #[test]
    fn native_rebuild_retains_reserved_mask_bits_and_exposes_late_inconsistent_state_prefix() {
        let mut model = model();
        model.allocate(0, 23..24).unwrap();
        let mut image = model.working().to_vec();
        rebuild_metadata(&mut image).unwrap();
        assert_eq!(u16::from_le_bytes(raw(&image, 0x355c1).unwrap()), 0);
        assert_eq!(image[0x36114 + 23 / 8] & (1 << (23 % 8)), 0);
        assert_eq!(image[0x36114 + 12] & 0xf0, 0xa0);
        let mut bad = model.working().to_vec();
        bad[COUNTS..COUNTS + 4].copy_from_slice(&2_u32.to_le_bytes());
        assert!(rebuild_metadata(&mut bad).is_err());
        assert_eq!(bad[0x36628], 0);
        assert_eq!(bad[0x35d19], 0xff); // Earlier descriptor reconstruction is committed.
        assert_eq!(&bad[..0x355c0], &model.working()[..0x355c0]);
    }

    #[test]
    fn stored_group_usage_is_not_display_length_or_recounted_map_population() {
        let mut model = model();
        assert_eq!(model.group_count(), 2);
        model.set_word(COUNTS + 4 * 99, 0xffff_fffe);
        assert_eq!(model.group_used_slots(99).unwrap(), 0xffff_fffe);
        assert!(model.group_used_slots(100).is_err());
    }

    #[test]
    fn ur_storage_working_operations_use_literal_seed_nonidentity_order_and_metadata() {
        let mut memory = model().to_icf().unwrap().memory().to_vec();
        memory[0x3c0a7..0x3c2ff].fill(0xff);
        memory[0x3c0a7..0x3c0ab].copy_from_slice(&[2, 0, 0, 0]);
        memory[0x2eda3..0x2edab].copy_from_slice(b"ZERO    ");
        memory[0x2edb3..0x2edbb].copy_from_slice(b"TWO     ");
        let mut storage = Icf::from_memory(memory).unwrap();
        storage.comment = b"UR METADATA";
        storage.extra_data = 0x12345639;
        let file = storage.to_bytes();
        let parsed = Icf::parse(&file).unwrap();
        let mut working = WorkingUr::from_icf(&parsed).unwrap();
        assert_eq!(working.count(), 2);
        assert_eq!(&working.working()[0x2eda3..0x2edab], b"TWO     ");
        assert_eq!(&working.working()[0x2edab..0x2edb3], b"ZERO    ");
        assert_eq!(&working.working()[0x2edb3..0x2edbb], b"CALLSIGN");
        assert!(
            working
                .append(super::super::normalizer::ReferenceImage::Separate(&[]))
                .is_err()
        );
        assert_eq!(working.count(), 3); // Count commits before the late default reference read.
        assert_eq!(&working.working()[0x2edb3..0x2edbb], b"CALLSIGN");
        working
            .delete_one(0, super::super::normalizer::ReferenceImage::Working)
            .unwrap();
        assert_eq!(working.count(), 2);
        assert_eq!(&working.working()[0x2eda3..0x2edab], b"ZERO    ");
        assert_eq!(
            &working.working()[0x2f703 + 16 * 299..0x2f713 + 16 * 299],
            &[b' '; 16]
        );
        assert_eq!(
            &working.working()[0x2eda3 + 8 * 299..0x2edab + 8 * 299],
            b"CALLSIGN"
        );
        let saved = working.to_icf().unwrap();
        assert_eq!(saved.comment_bytes(), b"UR METADATA");
        assert_eq!(saved.extra_data(), 0x12345639);
        assert_eq!(saved.original_bytes(), file);
        assert_eq!(&saved.memory()[0x3c0a7..0x3c0ad], &[0, 0, 1, 0, 0xff, 0xff]);
    }

    #[test]
    fn borrowed_allocation_shares_method_and_keeps_raw_range_capacity_and_failure_prefixes() {
        let mut owned = model();
        let mut borrowed = owned.working.clone();
        let physical = borrowed[..0xa21c].to_vec();
        for rows in [1..4, 1..4, 3..5, 42..42] {
            assert_eq!(
                allocate_into(&mut borrowed, 0, rows.clone()).unwrap(),
                owned.allocate(0, rows).unwrap()
            );
            assert_eq!(borrowed, owned.working);
        }
        assert_eq!(raw::<1>(&borrowed, FLAGS).unwrap(), [0xa6]);
        assert_eq!(u32::from_le_bytes(raw(&borrowed, MAP + 4).unwrap()), 0);
        assert_eq!(u32::from_le_bytes(raw(&borrowed, COUNTS).unwrap()), 4);
        assert_eq!(&borrowed[..0xa21c], physical);
        let before = owned.clone();
        assert!(owned.allocate(99, 1..2).is_err());
        assert_eq!(owned, before); // The method keeps its displayed-group guard.

        put(&mut borrowed, GROUP_COUNT, &0_u32.to_le_bytes()).unwrap();
        put(&mut borrowed, MAP + 400 * 99, &600_u32.to_le_bytes()).unwrap();
        put(&mut borrowed, COUNTS + 4 * 99, &1_u32.to_le_bytes()).unwrap();
        borrowed[0x355a1] = 0; // Raw working phases need no Icf/signature gate.
        assert_eq!(allocate_into(&mut borrowed, 99, 1..2).unwrap().allocated, 1);
        assert_eq!(
            u32::from_le_bytes(raw(&borrowed, MAP + 400 * 99).unwrap()),
            600
        );
        assert_eq!(
            u32::from_le_bytes(raw(&borrowed, MAP + 400 * 99 + 4).unwrap()),
            4
        );
        assert_eq!(
            u32::from_le_bytes(raw(&borrowed, COUNTS + 4 * 99).unwrap()),
            2
        );
        let before = borrowed.clone();
        for (group, start, end) in [
            (100, 0, 1),
            (usize::MAX, 0, 0),
            (0, 4, 3),
            (0, 0, 101),
            (0, 0, usize::MAX),
        ] {
            assert!(allocate_into(&mut borrowed, group, start..end).is_err());
            assert_eq!(borrowed, before);
        }

        let mut capacity = model().working;
        put(&mut capacity, COUNTS, &499_u32.to_le_bytes()).unwrap();
        assert_eq!(
            allocate_into(&mut capacity, 1, 7..10).unwrap(),
            Allocation {
                requested: 3,
                allocated: 1,
                exhausted: true,
            }
        );
        assert_eq!(
            u32::from_le_bytes(raw(&capacity, MAP + 400 + 7 * 4).unwrap()),
            0
        );
        assert_eq!(u32::from_le_bytes(raw(&capacity, COUNTS + 4).unwrap()), 1);
        assert_eq!(
            u32::from_le_bytes(raw(&capacity, MAP + 400 + 8 * 4).unwrap()),
            EMPTY
        );
        let mut no_flags = model().working;
        no_flags[FLAGS..FLAGS + 500].fill(0xa6);
        assert_eq!(
            allocate_into(&mut no_flags, 0, 0..1).unwrap(),
            Allocation {
                requested: 1,
                allocated: 0,
                exhausted: true,
            }
        );
        let mut short = model().working[..MAP + 4].to_vec();
        assert!(allocate_into(&mut short, 0, 0..2).is_err());
        assert_eq!(raw::<1>(&short, FLAGS).unwrap(), [0xa6]);
        assert_eq!(raw::<1>(&short, FLAGS + 1).unwrap(), [0xa7]);
        assert_eq!(u32::from_le_bytes(raw(&short, MAP).unwrap()), 0);
        assert_eq!(u32::from_le_bytes(raw(&short, COUNTS).unwrap()), 0); // Tail count not reached.
        let mut bad_count = model().working;
        put(&mut bad_count, COUNTS, &501_u32.to_le_bytes()).unwrap();
        let before = bad_count.clone();
        assert!(allocate_into(&mut bad_count, 0, 0..1).is_err());
        assert_eq!(bad_count, before);
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
    fn native_home_reads_group_word_not_default_global_row_and_marks_only_matching_group() {
        let mut m = model();
        m.working[0x3c84b] = 1;
        let mut generation = vec![0; 0x36825]; // No global Home record is supplied.
        generation[0x36821..0x36825].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
        let mut invalidated = 0xa7;
        let old_home = raw::<5>(m.working(), 0x3c84b).unwrap();
        m.reset_home(0, ReferenceImage::Separate(&generation), &mut invalidated)
            .unwrap();
        assert_eq!(raw::<2>(m.working(), 0x36821).unwrap(), [0x12, 0x34]);
        assert_eq!(raw::<5>(m.working(), 0x3c84b).unwrap(), old_home);
        assert_eq!(invalidated, 0xa7);
        m.reset_home(1, ReferenceImage::Separate(&generation), &mut invalidated)
            .unwrap();
        assert_eq!(raw::<2>(m.working(), 0x36823).unwrap(), [0x56, 0x78]);
        assert_eq!(
            raw::<5>(m.working(), 0x3c84b).unwrap(),
            [1, 0x56, 0x78, old_home[3], old_home[4]]
        );
        assert_eq!(invalidated, 1);
        m.working[0x36823..0x36825].copy_from_slice(&[9, 7]);
        m.reset_home(1, ReferenceImage::Working, &mut invalidated)
            .unwrap();
        assert_eq!(raw::<2>(m.working(), 0x3c84c).unwrap(), [9, 7]);
    }

    #[test]
    fn native_home_operations_preserve_release_before_shift_and_late_reset_failure_prefixes() {
        let missing = ReferenceImage::Separate(&[]);
        let mut invalidated = 0xa7;
        let mut m = model();
        m.working[0x3c84b] = 0;
        m.allocate(0, 2..4).unwrap();
        let physical = m.working()[..0xa21c].to_vec();
        assert!(
            m.delete_with_home(0, 2..3, missing, &mut invalidated)
                .is_err()
        );
        assert_eq!(m.slot(0, 2).unwrap(), None);
        assert_eq!(m.slot(0, 3).unwrap(), Some(1)); // Reset error precedes shift.
        assert_eq!(m.group_used_slots(0).unwrap(), 1);
        assert_eq!(invalidated, 0xa7);
        assert_eq!(
            m.delete_with_home(0, 2..3, missing, &mut invalidated)
                .unwrap(),
            0
        );
        assert_eq!(m.slot(0, 2).unwrap(), Some(1)); // Empty deletion shifts without G.
        assert!(
            m.insert_with_home(0, 1, 1, missing, &mut invalidated)
                .is_err()
        );
        assert_eq!(m.slot(0, 3).unwrap(), Some(1)); // Empty tail: shift precedes G.
        assert_eq!(m.slot(0, 2).unwrap(), None);
        m.allocate(0, 99..100).unwrap();
        assert!(
            m.insert_with_home(0, 0, 1, missing, &mut invalidated)
                .is_err()
        );
        assert_eq!(m.slot(0, 99).unwrap(), None);
        assert_eq!(m.slot(0, 3).unwrap(), Some(1)); // Occupied tail: G before shift.
        assert!(
            m.carry_with_home(0, 3..4, -1, missing, &mut invalidated)
                .is_err()
        );
        assert_eq!(m.slot(0, 2).unwrap(), Some(1)); // Rotation commits before G.
        assert!(
            !m.carry_with_home(0, 0..1, -1, missing, &mut invalidated)
                .unwrap()
        );
        assert_eq!(
            m.release_with_home(0, 90..91, missing, &mut invalidated)
                .unwrap(),
            0
        );
        assert_eq!(invalidated, 0xa7);
        assert_eq!(&m.working()[..0xa21c], physical);
        let mut holes = model();
        holes.working[0x3c84b] = 0;
        assert!(
            holes
                .carry_with_home(0, 2..3, 0, ReferenceImage::Working, &mut invalidated)
                .unwrap()
        );
        assert_eq!(invalidated, 1); // Allowed all-hole movement is still an action.
        assert_eq!(holes.used_slots(), 0);
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
    fn typed_channel_store_keeps_original_comment_etc_and_non_target_records() {
        let mut model = model();
        model.allocate(0, 0..1).unwrap();
        model.comment = b"SYNTHETIC".to_vec();
        model.extra_data = 0x12345639;
        let source = model.to_icf().unwrap().to_bytes();
        let parsed = Icf::parse(&source).unwrap();
        let mut model = NormalMemory::from_icf(&parsed).unwrap();
        let before = model.working().to_vec();
        let mut holder = super::super::record::reset_holder(2, 0, 17);
        super::super::record_image::load_channel_into(
            model.working(),
            super::super::record_image::ChannelKind::Normal,
            0,
            &mut holder,
        )
        .unwrap();
        holder.as_mut_bytes()[0x3c..0x40].copy_from_slice(&98765432_u32.to_le_bytes());
        model.store_channel(1, &mut holder).unwrap();
        assert_eq!(&model.working()[34..38], &98765432_u32.to_le_bytes());
        assert_eq!(&model.working()[83..0xa21c], &before[83..0xa21c]);
        let saved = model.to_icf().unwrap();
        assert_eq!(saved.original_bytes(), source);
        assert_eq!(saved.comment_bytes(), b"SYNTHETIC");
        assert_eq!(saved.extra_data(), 0x12345639);
        let before = model.clone();
        assert!(model.store_channel(20000, &mut holder).is_err());
        assert_eq!(model, before);
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

    #[test]
    fn group_name_editor_shares_the_normal_name_codec_and_cancel_keeps_the_append() {
        let mut m = model();
        let count = m.group_count();
        assert!(!m.edit_group_name(count, None).unwrap());
        assert_eq!(m.group_count(), count + 1);
        let mut input = b" A\0B\x81\x40 ".to_vec();
        assert!(m.edit_group_name(count, Some(&mut input)).unwrap());
        let at = 0x355c3 + 19 * count;
        assert_eq!(&m.working()[at..at + 16], b" A B            ");
        assert_eq!(input, b" A B\x81\x40 ");
        let mut image = m.to_icf().unwrap();
        image.patch_memory(&[(0x3662d, &[0xa6])]).unwrap();
        let mut name = b"LOWER\0suffix".to_vec();
        assert!(image.edit_physical_memory_name(0, Some(&mut name)).unwrap());
        assert_eq!(&image.memory()[0x43..0x53], b"LOWER suffix    ");
        assert!(!image.edit_physical_memory_name(0, None).unwrap());
    }
}
