//! Type-0C working-image adapter. Raw52/aux7 codecs belong to icf::record.
//! from_icf expands order/gateway from an immutable saved snapshot; from_working
//! accepts an already-expanded 0x4997a-byte image with its live count table.
//! Row shifts re-encode every physical slot, even beyond the active prefix.
//! Reset callbacks read the live global source's word0/1 at the actual commit
//! point. Unaffected words do not read it; failure on word1 keeps word0 stored.

use super::groups::{
    COUNTS, Groups, NAMES, ORDER, REFS, STARTS, group_of_counts, working_counts,
    write_storage_tables,
};
use crate::icf::{
    Error,
    context::Context,
    frequency::Ranges,
    record::{self, DecodedRecord, RepeaterWrite},
    special::Notification,
};

const BODY: usize = 0xaa0f;
const AUX: usize = 0x2a5df;
const USE: usize = 0x3bf6e;

fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 0, reason }
}

fn read<const N: usize>(image: &[u8], at: usize) -> Result<[u8; N], Error> {
    let end = at
        .checked_add(N)
        .ok_or_else(|| invalid("repeater read offset overflow"))?;
    image
        .get(at..end)
        .ok_or_else(|| invalid("repeater read outside working image"))?
        .try_into()
        .map_err(|_| invalid("repeater fixed read width"))
}
fn patch(image: &mut [u8], at: usize, bytes: &[u8]) -> Result<(), Error> {
    let end = at
        .checked_add(bytes.len())
        .ok_or_else(|| invalid("repeater write offset overflow"))?;
    image
        .get_mut(at..end)
        .ok_or_else(|| invalid("repeater write outside working image"))?
        .copy_from_slice(bytes);
    Ok(())
}
fn address(base: usize, width: usize, index: usize) -> Result<usize, Error> {
    index
        .checked_mul(width)
        .and_then(|n| base.checked_add(n))
        .ok_or_else(|| invalid("repeater row offset overflow"))
}

/// Borrowed 6B9A adapter; the owned Working entry calls this same implementation.
pub fn load_into(image: &[u8], index: usize, holder: &mut DecodedRecord) -> Result<(), Error> {
    if index >= 2500 {
        return Err(invalid("repeater physical load outside 0..2500"));
    }
    Groups::from_working(image)?;
    load_row_into(image, index, holder)
}
fn load_row_into(image: &[u8], index: usize, holder: &mut DecodedRecord) -> Result<(), Error> {
    let counts = working_counts(image)?;
    let raw = read::<52>(image, address(BODY, 52, index)?)?;
    let aux = read::<7>(image, address(AUX, 7, index)?)?;
    let used =
        u32::from(read::<1>(image, address(USE, 1, index >> 3)?)?[0] & (1 << (index & 7)) != 0);
    holder.set_header(0x0c, index as u32, holder.binding());
    holder.set_valid(index < counts.iter().map(|n| usize::from(*n)).sum());
    record::decode_repeater_into(
        holder,
        &raw,
        &aux,
        used,
        group_of_counts(&counts, index as i32) as u32,
    );
    Ok(())
}

/// Borrowed saver. In particular, USE bounds remain a late stage validation.
pub fn save_into(image: &mut [u8], index: usize, holder: &DecodedRecord) -> Result<(), Error> {
    save_row_with(image, index, holder, true, &mut |_, _| Ok(()))
}
fn store_with(
    image: &mut [u8],
    at: usize,
    bytes: &[u8],
    before: &mut impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    before(at, bytes)?;
    patch(image, at, bytes)
}
fn save_row_with(
    image: &mut [u8],
    index: usize,
    holder: &DecodedRecord,
    physical_limit: bool,
    before: &mut impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    let at = address(BODY, 52, index)?;
    let mut raw = read::<52>(image, at)?;
    record::encode_repeater_into(holder, &mut raw, |event| match event {
        RepeaterWrite::Body { offset, bytes, .. } => store_with(image, at + offset, bytes, before),
        RepeaterWrite::Gateway(bytes) => store_with(image, address(AUX, 7, index)?, &bytes, before),
        RepeaterWrite::Use(value) => {
            if physical_limit && index >= 2500 {
                return Err(invalid("repeater USE slot outside 0..2500"));
            }
            let at = address(USE, 1, index >> 3)?;
            let mask = 1 << (index & 7);
            let old = read::<1>(image, at)?[0];
            store_with(
                image,
                at,
                &[(old & !mask) | if value != 0 { mask } else { 0 }],
                before,
            )
        }
    })
}

/// C6A4C raw phase. Prefix length stops only at FFFF. All50 counts are first
/// zeroed in reverse order; valid starts then write low16(boundary-start).
/// No ICF signature, order uniqueness or monotonic-start policy belongs here.
pub fn expand_counts(image: &mut [u8]) -> Result<(), Error> {
    expand_counts_with(image, &mut |_, _| Ok(()))
}
fn expand_counts_with(
    image: &mut [u8],
    before: &mut impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut boundary = 0_u32;
    for row in 0..2500 {
        if u16::from_le_bytes(read::<2>(image, ORDER + 2 * row)?) == u16::MAX {
            break;
        }
        boundary += 1;
    }
    for group in (0..50).rev() {
        store_with(image, COUNTS + 2 * group, &[0, 0], before)?;
    }
    for group in (0..50).rev() {
        let start = u32::from(u16::from_le_bytes(read::<2>(image, STARTS + 2 * group)?));
        if start < 2500 {
            store_with(
                image,
                COUNTS + 2 * group,
                &(boundary.wrapping_sub(start) as u16).to_le_bytes(),
                before,
            )?;
            boundary = start;
        }
    }
    Ok(())
}

/// C6AEC raw phase. Its one snapshot is captured HERE, after preceding stage8
/// phases. Each physical source includes its USE bit; gateway links select the
/// snapshot's indirect aux before copying it into same-physical working aux.
pub fn expand_rows(image: &mut [u8], context: DefaultContext) -> Result<(), Error> {
    expand_rows_with(image, context, &mut |_, _| Ok(()))
}
fn expand_rows_with(
    image: &mut [u8],
    context: DefaultContext,
    before: &mut impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    let snapshot = image.to_vec();
    for index in 0..2500 {
        let physical = u16::from_le_bytes(read::<2>(&snapshot, ORDER + 2 * index)?);
        let mut holder = record::reset_holder(0x0c, index as u32, 0);
        if physical < 2500 {
            load_row_into(&snapshot, usize::from(physical), &mut holder)?;
            save_row_with(image, index, &holder, false, before)?;
            let link = holder.repeater_field(record::RepeaterField::GatewayIndex);
            if link < 2500 {
                store_with(
                    image,
                    address(AUX, 7, index)?,
                    &read::<7>(&snapshot, address(AUX, 7, link as usize)?)?,
                    before,
                )?;
            } else {
                holder.set_repeater_text(record::RepeaterText::Gateway, &[b' '; 8]);
                let mut raw = read::<52>(image, address(BODY, 52, index)?)?;
                record::encode_repeater_into(&holder, &mut raw, |event| match event {
                    RepeaterWrite::Gateway(bytes) => {
                        store_with(image, address(AUX, 7, index)?, &bytes, before)
                    }
                    _ => Ok(()),
                })?;
            }
        } else {
            let counts = working_counts(&snapshot)?;
            holder = context.holder(group_of_counts(&counts, index as i32) as u32, 0);
            save_row_with(image, index, &holder, false, before)?;
        }
    }
    Ok(())
}

/// C6CE4 raw phase: starts, identity/FFFF order, all initial gateway self-links,
/// then live nested first-match dedup. Every row uses the one full typed saver.
/// Only actual read/write windows are checked, not a repaired file marker or a
/// synthetic valid topology; errors retain preceding stores.
pub fn rebuild_storage_into(image: &mut [u8]) -> Result<(), Error> {
    rebuild_storage_with(image, &mut |_, _| Ok(()))
}
fn rebuild_storage_with(
    image: &mut [u8],
    before: &mut impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    let counts = working_counts(image)?;
    let total = write_storage_tables(&counts, |at, bytes| store_with(image, at, bytes, before))?;
    let mut holder = record::reset_holder(0x0c, 0, 0);
    for index in 0..total {
        load_row_into(image, index, &mut holder)?;
        let link = if *holder.repeater_text(record::RepeaterText::Gateway) == [b' '; 8] {
            u16::MAX
        } else {
            index as u16
        };
        holder.set_repeater_field(record::RepeaterField::GatewayIndex, u32::from(link));
        save_row_with(image, index, &holder, false, before)?;
    }
    let mut other = record::reset_holder(0x0c, 0, 0);
    for first in 0..total {
        load_row_into(image, first, &mut holder)?;
        let call = *holder.repeater_text(record::RepeaterText::Gateway);
        if call == [b' '; 8] {
            continue;
        }
        for next in first + 1..total {
            load_row_into(image, next, &mut other)?;
            if call == *other.repeater_text(record::RepeaterText::Gateway) {
                other.set_repeater_text(record::RepeaterText::Gateway, &[b' '; 8]);
                other.set_repeater_field(record::RepeaterField::GatewayIndex, first as u32);
                save_row_with(image, next, &other, false, before)?;
            }
        }
    }
    Ok(())
}

fn delete_shift(
    image: &mut [u8],
    group: usize,
    relative: usize,
    seed: &DecodedRecord,
) -> Result<Groups, Error> {
    let groups = Groups::from_working(image)?;
    let start = groups.start(group)? as usize;
    if relative >= usize::from(groups.counts[group]) {
        return Err(invalid("repeater selection outside group"));
    }
    let mut holder = seed.clone();
    for target in start + relative..2499 {
        load_into(image, target + 1, &mut holder)?;
        save_into(image, target, &holder)?;
    }
    Ok(groups)
}
fn delete_metadata(
    image: &mut [u8],
    mut groups: Groups,
    group: usize,
    relative: usize,
) -> Result<(), Error> {
    groups.after_delete(
        group,
        relative,
        [groups.references[1], groups.references[2]],
    )?;
    patch(image, REFS[0], &groups.references[0].to_le_bytes())?;
    patch(
        image,
        COUNTS + 2 * group,
        &groups.counts[group].to_le_bytes(),
    )
}

/// Borrowed D1AD0. Generate the actual type22 at the default callback's read
/// point, after all tail shifts. Each reset callback reads live F separately;
/// a second-word failure keeps the shifted rows, count and first reset word.
pub fn delete_into(
    image: &mut [u8],
    group: usize,
    relative: usize,
    seed: &DecodedRecord,
    default: impl FnOnce(&mut [u8]) -> Result<DecodedRecord, Error>,
    mut reset: impl FnMut(&[u8], usize) -> Result<u16, Error>,
) -> Result<(), Error> {
    let groups = delete_shift(image, group, relative, seed)?;
    let holder = default(image)?;
    save_into(image, 2499, &holder)?;
    delete_metadata(image, groups, group, relative)?;
    for which in 0..2 {
        let value = reset(image, which)?;
        patch(image, REFS[which + 1], &value.to_le_bytes())?;
    }
    Ok(())
}

/// Owned expanded data. Exposing only checked writes keeps saved and working
/// layouts distinct without adding an unchecked mutable Icf image window.
pub struct Working {
    memory: Vec<u8>,
    original: Option<Vec<u8>>,
}

/// C7664's signed inclusive absolute range; count is not a loop gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UseRange {
    pub first: i32,
    pub last: i32,
}

/// Actual type22 inputs from569FC and10420. These are runtime Q/host results,
/// not a generated default image or an arbitrary616-byte row supplied by a CLI.
#[derive(Debug, Clone, Copy)]
pub struct DefaultContext {
    pub five_mhz: bool,
    pub hemispheres: [u8; 2],
}
impl DefaultContext {
    /// 569FC and10420 consume Q's selector/source-selector, not effective F/U.
    /// Core owns that decoding; only selector16 needs the explicit SYSTEM LANGID.
    /// Source0 has5MHz/NE, source1 has0/NW, and sources2..4 have0/NE.
    pub fn from_context(
        context: &crate::icf::context::Context,
        system_langid: Option<u16>,
    ) -> Result<Self, Error> {
        let source = context.source_selector(system_langid)?;
        Ok(Self {
            five_mhz: source == 0,
            hemispheres: if source == 1 { *b"NW" } else { *b"NE" },
        })
    }
    /// 6A9A..6B95's code/short-literal initialization. No factory payload is
    /// consulted. Group is the owning image's current C6704 result.
    pub fn holder(self, group: u32, binding: u32) -> DecodedRecord {
        let mut holder = record::reset_holder(0x22, 0, binding);
        holder.set_valid(false);
        holder.set_group(group);
        let bytes = holder.as_mut_bytes();
        bytes[0x15c..0x160].copy_from_slice(&2_u32.to_le_bytes());
        bytes[0x160..0x168].copy_from_slice(b"CALLSIGN");
        bytes[0x44..0x48].copy_from_slice(&u32::from(self.five_mhz).to_le_bytes());
        bytes[0x48..0x4c]
            .copy_from_slice(&(if self.five_mhz { 5_000_000_u32 } else { 0 }).to_le_bytes());
        bytes[0x178..0x17c].copy_from_slice(&8_u32.to_le_bytes());
        bytes[0x25d] = self.hemispheres[0];
        bytes[0x261] = self.hemispheres[1];
        bytes[0x184..0x188].copy_from_slice(&0x7fff_u32.to_le_bytes());
        bytes[0x188..0x18c].copy_from_slice(&0xffff_u32.to_le_bytes());
        holder
    }
}
impl Working {
    pub fn from_working(memory: Vec<u8>) -> Result<Self, Error> {
        Groups::from_working(&memory)?;
        Ok(Self {
            memory,
            original: None,
        })
    }
    /// C6AEC: immutable snapshot, order[j] physical source, typed save into j,
    /// then indirect gateway aux[k] into same-row working aux[j]. All2500 rows
    /// are processed; invalid order entries use the actual type22 initializer.
    pub fn from_icf(image: &crate::icf::Icf<'_>, context: DefaultContext) -> Result<Self, Error> {
        let mut memory = image.memory().to_vec();
        memory.resize(super::groups::WORKING_BYTES, 0);
        expand_counts(&mut memory)?;
        expand_rows(&mut memory, context)?;
        Ok(Self {
            memory,
            original: Some(image.to_bytes()),
        })
    }
    pub fn working(&self) -> &[u8] {
        &self.memory
    }
    pub fn into_working(self) -> Vec<u8> {
        self.memory
    }
    pub fn groups(&self) -> Result<Groups, Error> {
        Groups::from_working(&self.memory)
    }

    /// 630EC -> 7FFA4 range from the first query3F reply's group. A missing or
    /// out-of-axis reply produces the native zero descriptor, not an empty range.
    /// In particular, an empty leading group can still visit valid physical0.
    pub fn bulk_use_range(&self, group: Option<usize>) -> Result<UseRange, Error> {
        let groups = self.groups()?;
        let Some(group) = group.filter(|g| *g < 50) else {
            return Ok(UseRange { first: 0, last: 0 });
        };
        Ok(UseRange {
            first: groups.start(group)? as i32,
            last: groups.end(group)?.max(0),
        })
    }

    /// C7664 shared No/Yes body. Each reached Yes predicate obtains its own live
    /// B/Q pair; no refresh or synthetic successful predicate is substituted.
    /// Accepted rows always use the full typed saver, even for an equal USE.
    /// There is no save/reload, confirmation or rollback. Normal completion queues
    /// 11(0,0) then13(0,14F), including reversed and all-skipped ranges.
    pub fn bulk_use<'a>(
        &mut self,
        range: UseRange,
        value: bool,
        binding: u32,
        live: impl FnMut() -> Result<(&'a Ranges, &'a Context), Error>,
        enqueue: impl FnMut(Notification) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.bulk_use_with(range, value, binding, live, enqueue, &mut |_, _| Ok(()))
    }

    fn bulk_use_with<'a>(
        &mut self,
        range: UseRange,
        value: bool,
        binding: u32,
        mut live: impl FnMut() -> Result<(&'a Ranges, &'a Context), Error>,
        mut enqueue: impl FnMut(Notification) -> Result<(), Error>,
        before: &mut impl FnMut(usize, &[u8]) -> Result<(), Error>,
    ) -> Result<(), Error> {
        use crate::icf::record::{ChannelField as C, RepeaterField as R};
        let mut holder = record::reset_holder(0x0c, 0, binding);
        for index in range.first..=range.last {
            self.load_into(index as usize, &mut holder)?;
            if !holder.valid() {
                continue;
            }
            if value {
                let hz = holder.channel_field(C::FrequencyHz);
                if hz == 0 {
                    continue;
                }
                let (ranges, q) = live()?;
                if ranges.db98(hz).ecdc(q, hz) {
                    continue;
                }
                let (ranges, q) = live()?;
                if ranges.db98(hz).eb64(q, hz) {
                    continue;
                }
                let kind = super::staging::classify_bytes(holder.as_bytes());
                let duplex = holder.channel_field(C::Duplex);
                if (kind == 0 && duplex == 0) || (kind == 1 && duplex != 0) {
                    continue;
                }
            }
            holder.set_repeater_field(R::Use, u32::from(value));
            save_row_with(&mut self.memory, index as usize, &holder, true, before)?;
        }
        drop(holder);
        for (event, second) in [(0x11, 0), (0x13, 0x14f)] {
            enqueue(Notification {
                event,
                synchronous: false,
                first: 0,
                second,
                payload: Vec::new(),
            })?;
        }
        Ok(())
    }

    fn read<const N: usize>(&self, at: usize) -> Result<[u8; N], Error> {
        read(&self.memory, at)
    }
    pub(crate) fn patch(&mut self, at: usize, bytes: &[u8]) -> Result<(), Error> {
        patch(&mut self.memory, at, bytes)
    }
    fn range(
        &self,
        group: usize,
        first: usize,
        last: usize,
    ) -> Result<(Groups, usize, usize), Error> {
        let groups = self.groups()?;
        let start = groups.start(group)? as usize;
        if first > last || last >= usize::from(groups.counts[group]) {
            return Err(invalid("repeater selection outside group"));
        }
        Ok((groups, start + first, start + last))
    }
    fn put_reference(&mut self, which: usize, value: u16) -> Result<(), Error> {
        self.patch(REFS[which], &value.to_le_bytes())
    }
    fn reset_references(
        &mut self,
        first: usize,
        last: usize,
        mut source: impl FnMut(&Self, usize) -> Result<u16, Error>,
    ) -> Result<(), Error> {
        for which in 0..2 {
            let old = u16::from_le_bytes(self.read::<2>(REFS[which + 1])?);
            if (first..=last).contains(&usize::from(old)) {
                let value = source(self, which)?;
                self.put_reference(which + 1, value)?;
            }
        }
        Ok(())
    }

    /// 6B9A always decodes after assigning validity from index<working total.
    /// The inactive physical tail is readable and never replaced with a seed.
    /// The supplied holder supplies residual/unwritten fields and binding token.
    pub fn load_into(&self, index: usize, holder: &mut DecodedRecord) -> Result<(), Error> {
        load_into(&self.memory, index, holder)
    }

    /// Immediate per-stage image stores, not an atomic raw-row replacement.
    /// The checked physical API validates USE's slot at its original store stage,
    /// after earlier field stores. Raw storage phases check actual windows only.
    /// In particular, a source.valid=0 holder still executes the entire saver.
    pub fn save(&mut self, index: usize, holder: &DecodedRecord) -> Result<(), Error> {
        save_into(&mut self.memory, index, holder)
    }
    /// 81E8 save then reload, retaining an already-written prefix on error.
    pub fn save_reload(&mut self, holder: &mut DecodedRecord) -> Result<(), Error> {
        let index = holder.index() as usize;
        self.save(index, holder)?;
        self.load_into(index, holder)
    }

    /// Content snapshots with independent explicit residual fields, not raw52
    /// byte copies. A snapshot's header does not choose a later save target.
    pub fn copy(
        &self,
        group: usize,
        first: usize,
        last: usize,
        seed: &DecodedRecord,
    ) -> Result<Vec<DecodedRecord>, Error> {
        let (_, first, last) = self.range(group, first, last)?;
        (first..=last)
            .map(|index| {
                let mut holder = seed.clone();
                self.load_into(index, &mut holder)?;
                Ok(holder)
            })
            .collect()
    }
    /// Approved inclusive target range. D4 preserves the first reference and
    /// resets the later two only after all typed row saves have completed.
    pub fn paste(
        &mut self,
        group: usize,
        first: usize,
        source: &[DecodedRecord],
        reset: impl FnMut(&Self, usize) -> Result<u16, Error>,
    ) -> Result<usize, Error> {
        if source.is_empty() {
            return Ok(0);
        }
        let last = first
            .checked_add(source.len() - 1)
            .ok_or_else(|| invalid("repeater paste range overflow"))?;
        let (_, absolute, absolute_last) = self.range(group, first, last)?;
        for (n, holder) in source.iter().enumerate() {
            self.save(absolute + n, holder)?;
        }
        self.reset_references(absolute, absolute_last, reset)?;
        Ok(source.len())
    }
    /// Conditional low-level D7, not a claim that a Clear menu is registered.
    /// The one type22 load occurs before the first typed save, including valid0.
    pub fn clear(
        &mut self,
        group: usize,
        first: usize,
        last: usize,
        default: impl FnOnce(&mut Self) -> Result<DecodedRecord, Error>,
    ) -> Result<(), Error> {
        let (mut groups, absolute_first, absolute_last) = self.range(group, first, last)?;
        let holder = default(self)?;
        for index in absolute_first..=absolute_last {
            self.save(index, &holder)?;
        }
        groups.after_clear(group, first, last)?;
        self.put_reference(0, groups.references[0])
    }
    /// Both directions rotate content without altering count/order. The first
    /// reference follows content; the other words reset rather than add delta.
    pub fn carry(
        &mut self,
        group: usize,
        first: usize,
        last: usize,
        direction: i32,
        seed: &DecodedRecord,
        reset: impl FnMut(&Self, usize) -> Result<u16, Error>,
    ) -> Result<bool, Error> {
        let (mut groups, absolute_first, absolute_last) = self.range(group, first, last)?;
        let up = direction < 0;
        if (up && first == 0) || (!up && last + 1 == usize::from(groups.counts[group])) {
            return Ok(false);
        }
        let selected = self.copy(group, first, last, seed)?;
        let neighbor_index = if up {
            absolute_first - 1
        } else {
            absolute_last + 1
        };
        let mut neighbor = seed.clone();
        self.load_into(neighbor_index, &mut neighbor)?;
        let destination = if up {
            absolute_first - 1
        } else {
            absolute_first + 1
        };
        for (n, holder) in selected.iter().enumerate() {
            self.save(destination + n, holder)?;
        }
        self.save(if up { absolute_last } else { absolute_first }, &neighbor)?;
        // The first walker precedes the separate global-reset walker.
        groups.after_carry(
            group,
            first,
            last,
            direction,
            [groups.references[1], groups.references[2]],
        )?;
        self.put_reference(0, groups.references[0])?;
        self.reset_references(
            absolute_first.min(neighbor_index),
            absolute_last.max(neighbor_index),
            reset,
        )?;
        Ok(true)
    }
    /// D1AD0's single delete: shift through physical2498, then load type22 for
    /// physical2499. The default callback may fail AFTER all prior shifts.
    pub fn delete_one(
        &mut self,
        group: usize,
        relative: usize,
        seed: &DecodedRecord,
        default: impl FnOnce(&mut Self) -> Result<DecodedRecord, Error>,
        reset: impl FnMut(&Self, usize) -> Result<u16, Error>,
    ) -> Result<(), Error> {
        let groups = delete_shift(&mut self.memory, group, relative, seed)?;
        let holder = default(self)?;
        self.save(2499, &holder)?;
        delete_metadata(&mut self.memory, groups, group, relative)?;
        self.reset_references(0, usize::from(u16::MAX), reset)
    }
    /// D1678's single insert. Returns false at capacity without any source load.
    /// Count increments after row stores but before the D9 reference walker.
    pub fn insert_one(
        &mut self,
        group: usize,
        relative: usize,
        seed: &DecodedRecord,
        default: impl FnOnce(&mut Self) -> Result<DecodedRecord, Error>,
        reset: impl FnMut(&Self, usize) -> Result<u16, Error>,
    ) -> Result<bool, Error> {
        let mut groups = self.groups()?;
        let start = groups.start(group)? as usize;
        if relative > usize::from(groups.counts[group]) {
            return Err(invalid("repeater insertion outside group"));
        }
        if groups.total() >= 2500 {
            return Ok(false);
        }
        let absolute = start + relative;
        let mut holder = seed.clone();
        for source in (absolute..2499).rev() {
            self.load_into(source, &mut holder)?;
            self.save(source + 1, &holder)?;
        }
        let holder = default(self)?;
        self.save(absolute, &holder)?;
        groups.after_insert(
            group,
            relative,
            [groups.references[1], groups.references[2]],
        )?;
        self.patch(COUNTS + 2 * group, &groups.counts[group].to_le_bytes())?;
        self.put_reference(0, groups.references[0])?;
        self.reset_references(0, usize::from(u16::MAX), reset)?;
        Ok(true)
    }
    /// Mode1-normalized accepted bytes, independent of row decode. Labels and
    /// document notifications remain with the caller's operation tail.
    pub fn rename_group_raw(&mut self, group: usize, name: [u8; 16]) -> Result<(), Error> {
        if group >= 50 {
            return Err(invalid("repeater named group outside 0..50"));
        }
        self.patch(NAMES + 16 * group, &name)
    }

    /// C6CE4: directory/order, then all initial gateway self-links, then nested
    /// first-match dedup. Both passes commit in order; no save-time rollback.
    pub fn rebuild_storage(&mut self) -> Result<(), Error> {
        rebuild_storage_into(&mut self.memory)
    }
    /// Convert a copy to storage, keeping this working aux layout unchanged.
    /// from_icf retains comment/EtcData; from_working has no file metadata and
    /// uses the explicit from_memory raw-constructor contract instead.
    pub fn to_icf(&self) -> Result<crate::icf::Icf<'_>, Error> {
        let mut storage = Self {
            memory: self.memory.clone(),
            original: None,
        };
        storage.rebuild_storage()?;
        let mut image = match &self.original {
            Some(source) => crate::icf::Icf::parse(source)?,
            None => crate::icf::Icf::from_memory(storage.memory[..0x3c9c0].to_vec())?,
        };
        image.patch_memory(&[(0, &storage.memory[..0x3c9c0])])?;
        Ok(image)
    }
}

#[cfg(test)]
mod tests {
    use super::super::groups::WORKING_BYTES;
    use super::*;
    use crate::icf::{
        context::Context,
        frequency::Ranges,
        record::{ChannelField as C, RepeaterField as R, RepeaterText as T},
    };

    fn bulk_fixture() -> Working {
        let mut memory = vec![0; WORKING_BYTES];
        memory[COUNTS + 98..COUNTS + 100].copy_from_slice(&12_u16.to_le_bytes());
        let mut w = Working::from_working(memory).unwrap();
        for (index, (mode, blank, dup, hz)) in [
            (2, false, 0, 145_000_000),
            (2, false, 1, 145_000_000),
            (2, true, 0, 145_000_000),
            (2, true, 1, 145_000_000),
            (0, false, 0, 145_000_000),
            (0, false, 1, 145_000_000),
            (1, false, 1, 145_000_000),
            (3, false, 0, 145_000_000),
            (3, false, 3, 145_000_000),
            (2, false, 1, 0),
            (2, false, 1, 150_000_000),
            (2, false, 1, 1_400_000_000),
        ]
        .into_iter()
        .enumerate()
        {
            let mut h = record::reset_holder(0x0c, index as u32, 0);
            h.set_valid(true);
            h.set_repeater_field(R::Mode, mode);
            h.set_repeater_text(T::Call, if blank { b"        " } else { b"TEST   B" });
            h.set_repeater_text(T::Gateway, b"TEST   G");
            h.set_channel_field(C::Duplex, dup);
            h.set_channel_field(C::FrequencyHz, hz);
            w.save(index, &h).unwrap();
        }
        w
    }

    fn bulk_context() -> Context {
        let mut q = Context::before_registry();
        let mut fields = q.fields();
        fields.0[0] = 0;
        q.adopt(fields);
        q
    }

    #[test]
    fn bulk_use_yes_and_no_keep_distinct_predicates_and_same_value_full_saves() {
        let q = bulk_context();
        let ranges = Ranges::generate(&q, None).unwrap();
        let mut w = bulk_fixture();
        let before = w.working().to_vec();
        let mut getters = 0;
        let mut starts = Vec::new();
        let mut queued = Vec::new();
        w.bulk_use_with(
            UseRange { first: 0, last: 12 },
            true,
            0x1234,
            || {
                getters += 1;
                Ok((&ranges, &q))
            },
            |event| {
                queued.push(event);
                Ok(())
            },
            &mut |at, _| {
                if (BODY..BODY + 52 * 2500).contains(&at) && (at - BODY) % 52 == 18 {
                    starts.push((at - BODY) / 52);
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(starts, [1, 2, 4, 5, 6, 7, 8]);
        assert_eq!(getters, 21);
        assert_eq!((w.memory[USE], w.memory[USE + 1]), (0xf6, 1));
        assert_eq!(&w.memory[..USE], &before[..USE]);
        assert_eq!(&w.memory[USE + 2..], &before[USE + 2..]);
        assert_eq!(
            queued
                .iter()
                .map(|e| (e.event, e.synchronous, e.first, e.second))
                .collect::<Vec<_>>(),
            [(0x11, false, 0, 0), (0x13, false, 0, 0x14f)]
        );
        let mut writes = 0;
        w.bulk_use_with(
            UseRange { first: 1, last: 1 },
            true,
            0,
            || Ok((&ranges, &q)),
            |_| Ok(()),
            &mut |_, _| {
                writes += 1;
                Ok(())
            },
        )
        .unwrap();
        assert!(writes >= 17);
        starts.clear();
        w.bulk_use_with(
            UseRange { first: 0, last: 12 },
            false,
            0,
            || panic!("No does not read Q/B"),
            |_| Ok(()),
            &mut |at, _| {
                if (BODY..BODY + 52 * 2500).contains(&at) && (at - BODY) % 52 == 18 {
                    starts.push((at - BODY) / 52);
                }
                Ok(())
            },
        )
        .unwrap();
        // Includes five rows already0, zero frequency and both rejected bands.
        assert_eq!(starts, (0..12).collect::<Vec<_>>());
        assert_eq!((w.memory[USE], w.memory[USE + 1]), (0, 0));
        let mut writes = 0;
        w.bulk_use_with(
            UseRange { first: 0, last: 0 },
            false,
            0,
            || panic!(),
            |_| Ok(()),
            &mut |_, _| {
                writes += 1;
                Ok(())
            },
        )
        .unwrap();
        assert!(writes >= 17);
    }

    #[test]
    fn bulk_use_ranges_keep_u16_counts_empty_leading_group_and_physical_last() {
        let mut memory = vec![0; WORKING_BYTES];
        memory[COUNTS + 2..COUNTS + 4].copy_from_slice(&256_u16.to_le_bytes());
        memory[COUNTS + 98..COUNTS + 100].copy_from_slice(&2244_u16.to_le_bytes());
        let mut w = Working::from_working(memory).unwrap();
        assert_eq!(
            w.bulk_use_range(Some(0)).unwrap(),
            UseRange { first: 0, last: 0 }
        );
        assert_eq!(
            w.bulk_use_range(Some(1)).unwrap(),
            UseRange {
                first: 0,
                last: 255
            }
        );
        assert_eq!(
            w.bulk_use_range(Some(2)).unwrap(),
            UseRange {
                first: 256,
                last: 255
            }
        );
        assert_eq!(
            w.bulk_use_range(Some(49)).unwrap(),
            UseRange {
                first: 256,
                last: 2499
            }
        );
        for group in [None, Some(50), Some(usize::MAX)] {
            assert_eq!(
                w.bulk_use_range(group).unwrap(),
                UseRange { first: 0, last: 0 }
            );
        }
        w.memory[USE] = 1;
        w.memory[USE + 312] = 0xff;
        for group in [0, 49] {
            w.bulk_use(
                w.bulk_use_range(Some(group)).unwrap(),
                false,
                0,
                || panic!(),
                |_| Ok(()),
            )
            .unwrap();
        }
        assert_eq!(w.memory[USE], 0);
        assert_eq!(w.memory[USE + 312], 0xf0);
        let mut events = Vec::new();
        w.bulk_use(
            UseRange { first: 3, last: 2 },
            true,
            0,
            || panic!(),
            |e| {
                events.push(e.event);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(events, [0x11, 0x13]);
        w.memory[COUNTS + 98..COUNTS + 100].copy_from_slice(&2245_u16.to_le_bytes());
        assert!(w.bulk_use_range(Some(49)).is_err());
    }

    #[test]
    fn bulk_use_live_getters_failures_and_all_skip_queue_keep_prefixes() {
        let q = bulk_context();
        let ranges = Ranges::generate(&q, None).unwrap();
        let mut missing = Context::before_registry();
        let mut fields = missing.fields();
        fields.0[0] = 16;
        missing.adopt(fields);
        let mut w = bulk_fixture();
        let mut reads = 0;
        // 150MHz is excluded by Japan EB64, but the SECOND live Q permits it.
        w.bulk_use(
            UseRange {
                first: 10,
                last: 10,
            },
            true,
            0,
            || {
                reads += 1;
                Ok((&ranges, if reads == 1 { &q } else { &missing }))
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(reads, 2);
        assert_eq!(w.memory[USE + 1] & 4, 4);
        let mut events = Vec::new();
        w.bulk_use(
            UseRange {
                first: 12,
                last: 12,
            },
            true,
            0,
            || panic!("invalid row skips getters"),
            |e| {
                events.push(e.event);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(events, [0x11, 0x13]);
        w.memory[USE] = 0xff;
        let mut queued = 0;
        assert!(
            w.bulk_use_with(
                UseRange { first: 0, last: 1 },
                false,
                0,
                || panic!(),
                |_| {
                    queued += 1;
                    Ok(())
                },
                &mut |at, _| if at == BODY + 52 + 18 {
                    Err(invalid("injected second row save failure"))
                } else {
                    Ok(())
                }
            )
            .is_err()
        );
        assert_eq!(w.memory[USE], 0xfe);
        assert_eq!(queued, 0);
        assert!(
            w.bulk_use_with(
                UseRange { first: 1, last: 1 },
                false,
                0,
                || panic!(),
                |_| panic!("failed saver skips queue"),
                &mut |at, _| if at == BODY + 52 + 42 {
                    Err(invalid("injected post-USE latitude failure"))
                } else {
                    Ok(())
                }
            )
            .is_err()
        );
        assert_eq!(w.memory[USE], 0xfc);
        assert!(
            w.bulk_use(
                UseRange {
                    first: 2499,
                    last: 2500
                },
                false,
                0,
                || panic!(),
                |_| {
                    queued += 1;
                    Ok(())
                }
            )
            .is_err()
        );
        assert_eq!(queued, 0);
        events.clear();
        assert!(
            w.bulk_use(
                UseRange { first: 0, last: -1 },
                false,
                0,
                || panic!(),
                |e| {
                    if e.event == 0x13 {
                        Err(invalid("injected second queue failure"))
                    } else {
                        events.push(e.event);
                        Ok(())
                    }
                }
            )
            .is_err()
        );
        assert_eq!(events, [0x11]);
    }
    fn fixture() -> Working {
        let mut memory = vec![0xa5; WORKING_BYTES];
        memory[COUNTS..COUNTS + 100].fill(0);
        memory[COUNTS + 4..COUNTS + 6].copy_from_slice(&3_u16.to_le_bytes());
        for (at, value) in REFS.into_iter().zip([1_u16, 2, 65535]) {
            memory[at..at + 2].copy_from_slice(&value.to_le_bytes());
        }
        for index in 0..2500 {
            let at = BODY + 52 * index;
            memory[at..at + 4].copy_from_slice(&(index as u32 + 1).to_le_bytes());
            memory[at + 10] = 0x40;
            memory[at + 48] = (index % 50) as u8;
            // Metadata deliberately points elsewhere: loader must use aux[index].
            memory[at + 8..at + 10].copy_from_slice(&0_u16.to_le_bytes());
        }
        memory[AUX..AUX + 7].copy_from_slice(&[0; 7]);
        memory[AUX + 7..AUX + 14].copy_from_slice(&[0x83, 0x0a, 0x1c, 0x48, 0xb1, 0xa3, 0xc8]);
        Working::from_working(memory).unwrap()
    }
    fn seed() -> DecodedRecord {
        DecodedRecord::from_bytes([0x63; 616])
    }
    #[test]
    fn borrowed_rows_share_owned_codec_delete_and_late_reset_prefix() {
        let mut owned = fixture();
        let mut borrowed = owned.working().to_vec();
        let mut left = seed();
        let mut right = seed();
        owned.load_into(2499, &mut left).unwrap();
        load_into(&borrowed, 2499, &mut right).unwrap();
        assert_eq!(left.as_bytes(), right.as_bytes());
        let fail = |_: &[u8], which| {
            if which == 0 {
                Ok(12345)
            } else {
                Err(invalid("injected F word1 failure"))
            }
        };
        assert!(
            owned
                .delete_one(
                    2,
                    0,
                    &seed(),
                    |_| Ok(default()),
                    |w, which| fail(w.working(), which)
                )
                .is_err()
        );
        assert!(
            delete_into(
                &mut borrowed,
                2,
                0,
                &seed(),
                |image| {
                    assert_eq!(read::<4>(image, BODY).unwrap(), 2_u32.to_le_bytes());
                    Ok(default())
                },
                fail
            )
            .is_err()
        );
        assert_eq!(owned.working(), borrowed);
        assert_eq!(owned.groups().unwrap().references, [0, 12345, 65535]);
        assert!(save_into(&mut borrowed, 2500, &default()).is_err());
        assert!(owned.save(2500, &default()).is_err());
        assert_eq!(owned.working(), borrowed);
        borrowed.truncate(USE);
        assert!(save_into(&mut borrowed, 0, &default()).is_err());
        assert_eq!(read::<4>(&borrowed, BODY).unwrap(), 9000_u32.to_le_bytes());
    }
    fn frequency(w: &Working, index: usize) -> u32 {
        u32::from_le_bytes(w.read::<4>(BODY + 52 * index).unwrap())
    }
    fn put(h: &mut DecodedRecord, at: usize, value: u32) {
        h.as_mut_bytes()[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn default() -> DecodedRecord {
        let mut h = seed();
        h.set_valid(false);
        put(&mut h, 0x3c, 9000);
        put(&mut h, 0x15c, 2);
        h
    }
    fn raw_phase_fixture() -> Vec<u8> {
        // Deliberately no repaired ICF marker, revision or text header.
        let mut image = vec![0xa5; WORKING_BYTES + 9];
        image[ORDER..STARTS + 100].fill(255);
        image[ORDER..ORDER + 8].copy_from_slice(&[2, 0, 0, 0, 1, 0, 255, 255]);
        image[STARTS..STARTS + 2].copy_from_slice(&0_u16.to_le_bytes());
        image[STARTS + 98..STARTS + 100].copy_from_slice(&2_u16.to_le_bytes());
        for (index, link, sign) in [(0, 5_u16, 0xa5), (1, 5, 0xc6), (2, 65535, 0x17)] {
            let at = BODY + 52 * index;
            image[at..at + 4].copy_from_slice(&(145_000_000 + index as u32).to_le_bytes());
            image[at + 8..at + 10].copy_from_slice(&link.to_le_bytes());
            image[at + 10] = 0;
            image[at + 48] = 201 + index as u8;
            image[at + 51] = sign;
        }
        image[AUX + 35..AUX + 42].copy_from_slice(&[0x83, 0x0a, 0x1c, 0x48, 0xb1, 0xa3, 0xc8]);
        image[USE] = 5;
        image[USE + 312] = 0xfb;
        image
    }
    fn phase_context() -> DefaultContext {
        DefaultContext::from_context(
            &crate::icf::context::Context::before_registry(),
            Some(0x411),
        )
        .unwrap()
    }
    #[test]
    fn raw_phases_accept_unrepaired_marker_and_take_the_rows_time_snapshot() {
        let mut image = raw_phase_fixture();
        let marker = image[0x3c9b0..0x3c9c0].to_vec();
        expand_counts(&mut image).unwrap();
        let counts = working_counts(&image).unwrap();
        assert_eq!(counts[0], 2);
        assert_eq!(counts[49], 1);
        assert!(counts[1..49].iter().all(|n| *n == 0));
        // A preceding phase changes this source AFTER count expansion. Later
        // target0 overwrites physical0: target1 must still read this snapshot.
        image[BODY..BODY + 4].copy_from_slice(&433_000_007_u32.to_le_bytes());
        image[AUX + 35..AUX + 42].copy_from_slice(&[0xa9, 0x16, 0x9d, 0x46, 0x0c, 0x50, 0x42]);
        expand_rows(&mut image, phase_context()).unwrap();
        assert_eq!(
            read::<4>(&image, BODY).unwrap(),
            145_000_002_u32.to_le_bytes()
        );
        assert_eq!(
            read::<4>(&image, BODY + 52).unwrap(),
            433_000_007_u32.to_le_bytes()
        );
        assert_eq!(
            read::<4>(&image, BODY + 104).unwrap(),
            145_000_001_u32.to_le_bytes()
        );
        assert_eq!(
            &image[AUX..AUX + 7],
            &[0x40, 0x81, 0x02, 0x04, 0x08, 0x10, 0x20]
        );
        assert_eq!(
            &image[AUX + 7..AUX + 14],
            &[0xa9, 0x16, 0x9d, 0x46, 0x0c, 0x50, 0x42]
        );
        assert_eq!(&image[AUX + 14..AUX + 21], &image[AUX + 7..AUX + 14]);
        assert_eq!(image[USE], 3);
        assert_eq!(image[USE + 312], 0xf0);
        for (index, sign) in [(0, 0x25), (1, 0x86), (2, 0xd7)] {
            assert_eq!(image[BODY + 52 * index + 51], sign);
            assert_eq!(image[BODY + 52 * index + 48], 201 + index as u8);
        }
        rebuild_storage_into(&mut image).unwrap();
        assert_eq!(&image[ORDER..ORDER + 8], &[0, 0, 1, 0, 2, 0, 255, 255]);
        assert_eq!(&image[STARTS..STARTS + 4], &[0, 0, 255, 255]);
        assert_eq!(&image[STARTS + 98..STARTS + 100], &[2, 0]);
        assert_eq!(read::<2>(&image, BODY + 8).unwrap(), [255, 255]);
        assert_eq!(read::<2>(&image, BODY + 52 + 8).unwrap(), [1, 0]);
        assert_eq!(read::<2>(&image, BODY + 104 + 8).unwrap(), [1, 0]);
        assert_eq!(
            &image[AUX + 14..AUX + 21],
            &[0x40, 0x81, 0x02, 0x04, 0x08, 0x10, 0x20]
        );
        assert_eq!(&image[0x3c9b0..0x3c9c0], marker);
        assert_eq!(&image[WORKING_BYTES..], &[0xa5; 9]);
    }
    #[test]
    fn raw_count_prefix_and_low16_subtraction_are_not_strict_editor_topology() {
        let mut image = raw_phase_fixture();
        // Duplicate, out-of-range and post-FFFF values are not prefix errors.
        image[ORDER..ORDER + 10].copy_from_slice(&[7, 0, 7, 0, 0xfe, 0xff, 255, 255, 0, 0]);
        image[STARTS + 98..STARTS + 100].copy_from_slice(&4_u16.to_le_bytes());
        expand_counts(&mut image).unwrap();
        assert_eq!(working_counts(&image).unwrap()[49], 65535);
        assert_eq!(working_counts(&image).unwrap()[0], 4);
        // rows visits all2500 order slots, including after the first FFFF.
        image[ORDER..STARTS].fill(255);
        image[ORDER + 8..ORDER + 10].copy_from_slice(&0_u16.to_le_bytes());
        expand_rows(&mut image, phase_context()).unwrap();
        assert_eq!(
            read::<4>(&image, BODY + 52 * 4).unwrap(),
            145_000_000_u32.to_le_bytes()
        );
    }
    #[test]
    fn raw_rebuild_checks_actual_windows_not_the_physical_editor_slot_limit() {
        let mut image = vec![0; WORKING_BYTES];
        image[COUNTS..COUNTS + 2].copy_from_slice(&2501_u16.to_le_bytes());
        for index in 0..2501 {
            image[AUX + 7 * index..AUX + 7 * (index + 1)]
                .copy_from_slice(&[0x40, 0x81, 0x02, 0x04, 0x08, 0x10, 0x20]);
        }
        rebuild_storage_into(&mut image).unwrap();
        assert_eq!(
            read::<2>(&image, ORDER + 4998).unwrap(),
            2499_u16.to_le_bytes()
        );
        assert_eq!(read::<2>(&image, BODY + 52 * 2500 + 8).unwrap(), [255, 255]);
        assert!(load_into(&image, 2500, &mut seed()).is_err());
    }
    #[test]
    fn raw_phase_short_inputs_and_late_faults_preserve_actual_store_prefixes() {
        for mut short in [vec![], vec![0; ORDER], vec![0; COUNTS + 99]] {
            assert!(expand_counts(&mut short).is_err());
            assert!(expand_rows(&mut short, phase_context()).is_err());
            assert!(rebuild_storage_into(&mut short).is_err());
        }
        let mut image = raw_phase_fixture();
        assert!(
            expand_counts_with(&mut image, &mut |at, _| {
                if at == COUNTS + 96 {
                    Err(invalid("injected reverse clear failure"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        assert_eq!(read::<2>(&image, COUNTS + 98).unwrap(), [0, 0]);
        assert_eq!(read::<2>(&image, COUNTS + 96).unwrap(), [0xa5; 2]);
        expand_counts(&mut image).unwrap();
        let before = image.clone();
        assert!(
            expand_rows_with(&mut image, phase_context(), &mut |at, _| {
                if at == BODY + 52 + 18 {
                    Err(invalid("injected next row name failure"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        assert_eq!(
            read::<4>(&image, BODY).unwrap(),
            145_000_002_u32.to_le_bytes()
        );
        assert_eq!(
            &image[BODY + 52..BODY + 104],
            &before[BODY + 52..BODY + 104]
        );
        assert_eq!(&image[COUNTS..COUNTS + 100], &before[COUNTS..COUNTS + 100]);
        let mut image = before;
        expand_rows(&mut image, phase_context()).unwrap();
        assert!(
            rebuild_storage_with(&mut image, &mut |at, _| {
                if at == ORDER + 4 {
                    Err(invalid("injected third order write failure"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        assert_eq!(&image[ORDER..ORDER + 6], &[0, 0, 1, 0, 1, 0]);
        assert_eq!(&image[STARTS + 98..STARTS + 100], &[2, 0]);
        assert_eq!(read::<2>(&image, BODY + 52 + 8).unwrap(), [5, 0]);
        let mut gateway_writes = 0;
        assert!(
            rebuild_storage_with(&mut image, &mut |at, _| {
                if at == AUX + 14 {
                    gateway_writes += 1;
                    if gateway_writes == 2 {
                        return Err(invalid("injected dedup gateway failure"));
                    }
                }
                Ok(())
            })
            .is_err()
        );
        // Every initial self-link already committed; the failed duplicate store
        // must not leave row2 linked to row1 before that row's final index stage.
        assert_eq!(read::<2>(&image, BODY + 104 + 8).unwrap(), [2, 0]);
        assert_eq!(&image[AUX + 7..AUX + 14], &image[AUX + 14..AUX + 21]);
    }
    #[test]
    fn shared_codec_uses_direct_aux_inactive_tail_and_late_use_failure_prefix() {
        assert!(Working::from_working(vec![0; 0x3c9c0]).is_err());
        let mut w = fixture();
        let mut h = seed();
        w.load_into(1, &mut h).unwrap();
        assert_eq!(&h.as_bytes()[0x168..0x170], b"ABCDEFGH");
        assert!(h.valid());
        assert_eq!(h.binding(), 0x63636363);
        w.load_into(2499, &mut h).unwrap();
        assert!(!h.valid());
        assert_eq!(h.group(), 50);
        assert_eq!(
            u32::from_le_bytes(h.as_bytes()[0x3c..0x40].try_into().unwrap()),
            2500
        );
        h = default();
        put(&mut h, 0x17c, 0x100);
        let before = w.memory[USE];
        w.save(1, &h).unwrap();
        assert_eq!(w.memory[USE], before | 2);
        assert_eq!(frequency(&w, 1), 9000);
        assert_eq!(w.memory[BODY + 52 + 48], 1);
        let before = w.memory.clone();
        assert!(w.save(2500, &h).is_err());
        assert_eq!(
            &w.memory[BODY + 52 * 2500 + 18..BODY + 52 * 2500 + 34],
            &h.as_bytes()[0x26..0x36]
        );
        assert_eq!(&w.memory[USE..USE + 313], &before[USE..USE + 313]);
        assert_eq!(
            &w.memory[BODY + 52 * 2500 + 42..BODY + 52 * 2500 + 52],
            &before[BODY + 52 * 2500 + 42..BODY + 52 * 2500 + 52]
        );
    }
    #[test]
    fn typed_carry_paste_and_clear_keep_destination_opaque_bytes_and_reference_kinds() {
        let mut w = fixture();
        let order = w.memory[0x3ab82..0x3bf6e].to_vec();
        assert!(
            w.carry(2, 0, 1, 0, &seed(), |_, which| Ok([60000, 65535][which]))
                .unwrap()
        );
        assert_eq!(
            (frequency(&w, 0), frequency(&w, 1), frequency(&w, 2)),
            (3, 1, 2)
        );
        assert_eq!(w.groups().unwrap().references, [2, 60000, 65535]);
        assert_eq!(&w.memory[0x3ab82..0x3bf6e], order);
        let snapshot = w.copy(2, 0, 0, &seed()).unwrap();
        w.paste(2, 2, &snapshot, |_, _| {
            panic!("unaffected references do not read global source")
        })
        .unwrap();
        assert_eq!(w.groups().unwrap().references, [2, 60000, 65535]);
        let low = w.memory[BODY + 2 * 52 + 51] & 0x3f;
        w.clear(2, 2, 2, |_| Ok(default())).unwrap();
        assert_eq!(frequency(&w, 2), 9000);
        assert_eq!(w.groups().unwrap().references[0], 65535);
        assert_eq!(w.memory[BODY + 2 * 52 + 51] & 0x3f, low);
        let mut expected = w.memory.clone();
        expected[NAMES + 16 * 49..NAMES + 16 * 50].fill(b'R');
        w.rename_group_raw(49, [b'R'; 16]).unwrap();
        assert_eq!(w.memory, expected);
        assert!(w.rename_group_raw(50, [b'R'; 16]).is_err());
        assert_eq!(w.memory, expected);
    }
    #[test]
    fn physical_tail_shifts_precede_default_failure_and_count_reference_commit() {
        let mut w = fixture();
        let old_counts = w.memory[COUNTS..COUNTS + 100].to_vec();
        assert!(
            w.delete_one(
                2,
                0,
                &seed(),
                |w| {
                    assert_eq!(frequency(w, 0), 2);
                    assert_eq!(frequency(w, 2498), 2500);
                    Err(invalid("injected late default failure"))
                },
                |_, _| panic!()
            )
            .is_err()
        );
        assert_eq!(&w.memory[COUNTS..COUNTS + 100], old_counts);
        assert_eq!(w.groups().unwrap().references, [1, 2, 65535]);
        let mut w = fixture();
        w.delete_one(
            2,
            0,
            &seed(),
            |_| Ok(default()),
            |w, which| {
                assert_eq!(w.groups()?.counts[2], 2);
                assert_eq!(w.groups()?.references[0], 0);
                Ok([65535, 60000][which])
            },
        )
        .unwrap();
        assert_eq!(frequency(&w, 2499), 9000);
        assert_eq!(w.groups().unwrap().references, [0, 65535, 60000]);
        let mut w = fixture();
        assert!(
            w.insert_one(
                2,
                1,
                &seed(),
                |w| {
                    assert_eq!(frequency(w, 2499), 2499);
                    assert_eq!(frequency(w, 2), 2);
                    Ok(default())
                },
                |w, which| {
                    assert_eq!(w.groups()?.counts[2], 4);
                    assert_eq!(w.groups()?.references[0], 2);
                    Ok([800, 900][which])
                }
            )
            .unwrap()
        );
        assert_eq!(
            (frequency(&w, 0), frequency(&w, 1), frequency(&w, 2)),
            (1, 9000, 2)
        );
        assert_eq!(w.groups().unwrap().references, [2, 800, 900]);
        w.memory[COUNTS + 4..COUNTS + 6].copy_from_slice(&2500_u16.to_le_bytes());
        let before = w.memory.clone();
        assert!(
            !w.insert_one(2, 0, &seed(), |_| panic!(), |_, _| panic!())
                .unwrap()
        );
        assert_eq!(w.memory, before);
    }
    #[test]
    fn reset_source_is_per_word_and_late_failure_keeps_prior_row_and_reference_stores() {
        let mut w = fixture();
        w.put_reference(1, 1).unwrap();
        w.put_reference(2, 2).unwrap();
        assert!(
            w.carry(2, 0, 1, 0, &seed(), |w, which| {
                assert_eq!(w.groups()?.references[0], 2);
                if which == 0 {
                    Ok(50000)
                } else {
                    assert_eq!(w.groups()?.references[1], 50000);
                    Err(invalid("injected second global word read failure"))
                }
            })
            .is_err()
        );
        assert_eq!(
            (frequency(&w, 0), frequency(&w, 1), frequency(&w, 2)),
            (3, 1, 2)
        );
        assert_eq!(w.groups().unwrap().references, [2, 50000, 2]);
        let before = w.memory.clone();
        assert!(!w.carry(2, 0, 0, -1, &seed(), |_, _| panic!()).unwrap());
        assert_eq!(w.memory, before);
    }
    #[test]
    fn storage_expansion_uses_snapshot_order_indirect_gateway_and_preserves_metadata_on_save() {
        let mut memory = vec![0xa5; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        memory[0x3ab82..0x3bf6e].fill(255);
        memory[NAMES..NAMES + 800].fill(b' ');
        memory[0x3ab82..0x3ab88].copy_from_slice(&[7, 0, 2, 0, 0, 0]);
        memory[0x3bf0a..0x3bf0c].copy_from_slice(&0_u16.to_le_bytes());
        memory[0x3bf0e..0x3bf10].copy_from_slice(&2_u16.to_le_bytes());
        for (physical, link) in [(7, 5_u16), (2, 5), (0, 65535)] {
            let at = BODY + 52 * physical;
            memory[at..at + 4].copy_from_slice(&(145000000 + physical as u32).to_le_bytes());
            memory[at + 8..at + 10].copy_from_slice(&link.to_le_bytes());
            memory[at + 10] = 0x40;
        }
        memory[AUX + 7 * 5..AUX + 7 * 6]
            .copy_from_slice(&[0x83, 0x0a, 0x1c, 0x48, 0xb1, 0xa3, 0xc8]);
        memory[BODY + 52 * 2499 + 48] = 199;
        let source = crate::icf::format::write_storage(
            &memory,
            crate::icf::format::Layout::from_map_revision(6),
            b"META",
            0x00040001,
        )
        .unwrap();
        let image = crate::icf::Icf::parse(&source).unwrap();
        let context = DefaultContext {
            five_mhz: true,
            hemispheres: *b"SW",
        };
        let working = Working::from_icf(&image, context).unwrap();
        let mut borrowed = image.memory().to_vec();
        borrowed.resize(WORKING_BYTES, 0);
        expand_counts(&mut borrowed).unwrap();
        expand_rows(&mut borrowed, context).unwrap();
        assert_eq!(working.working(), borrowed);
        assert_eq!(working.groups().unwrap().counts[..4], [2, 0, 1, 0]);
        let mut holder = record::reset_holder(0x0c, 0, 0);
        working.load_into(0, &mut holder).unwrap();
        assert_eq!(&holder.as_bytes()[0x168..0x170], b"ABCDEFGH");
        assert_eq!(frequency(&working, 0), 145000007);
        assert_eq!(frequency(&working, 2), 145000000);
        assert_eq!(working.memory[USE] & 7, 7);
        assert_eq!(working.memory[BODY + 51] & 0x3f, memory[BODY + 51] & 0x3f);
        working.load_into(2499, &mut holder).unwrap();
        assert!(!holder.valid());
        assert_eq!(&holder.as_bytes()[0x160..0x168], b"CALLSIGN");
        assert_eq!(holder.offset_hz(), 5_000_000);
        assert_eq!(working.memory[BODY + 52 * 2499 + 48], 199);
        let before = working.working().to_vec();
        let saved = working.to_icf().unwrap();
        rebuild_storage_into(&mut borrowed).unwrap();
        assert_eq!(saved.memory(), &borrowed[..0x3c9c0]);
        assert_eq!(saved.comment_bytes(), b"META");
        assert_eq!(saved.extra_data(), 0x00040001);
        assert_eq!(saved.memory()[0x1234], memory[0x1234]);
        assert_eq!(
            saved.physical_repeater(1).unwrap().unwrap().gateway_call,
            *b"ABCDEFGH"
        );
        assert_eq!(working.working(), before);
    }
    #[test]
    fn type22_context_reuses_core_source_selector_and_only_requests_needed_host_input() {
        let q = crate::icf::context::Context::before_registry();
        assert!(DefaultContext::from_context(&q, None).is_err());
        let japan = DefaultContext::from_context(&q, Some(0x411)).unwrap();
        assert!(japan.five_mhz);
        assert_eq!(japan.hemispheres, *b"NE");
        let other = DefaultContext::from_context(&q, Some(0x409)).unwrap();
        assert!(!other.five_mhz);
        assert_eq!(other.hemispheres, *b"NW");
        let mut fields = q.fields();
        fields.0[0] = 2;
        let mut q = q;
        q.adopt(fields);
        let selected = DefaultContext::from_context(&q, None).unwrap();
        assert!(!selected.five_mhz);
        assert_eq!(selected.hemispheres, *b"NE");
    }
}
