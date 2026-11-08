//! Strict MapRev 6 raw repeater row/group mapping and stored RPT1USE bits.
//!
//! Static CS-705 evidence only, not original-GUI or device validation. This does
//! not expand working records, decode auxiliary fields, import CSV or create rows.

use super::{Error, Icf, RECORD_BYTES};

const COUNT: usize = 2500;
const GROUP_COUNT: usize = 50;
const ORDER: usize = 0x3ab82;
const STARTS: usize = 0x3bf0a;
const NAMES: usize = 0x2ea3b;
const RECORDS: usize = 0xaa0f;
const RECORD_SIZE: usize = 52;
const USE_BITS: usize = 0x3bf6e;

/// One raw repeater group; Vec position is its ordinal 0..50, not a descriptor.
///
/// All 50 groups are returned, including empty and trailing groups. Names borrow
/// the exact 16-byte payload field without trimming, decoding or removing NULs.
pub struct RepeaterGroup<'a> {
    name: &'a [u8],
    slots: Vec<usize>,
}

impl RepeaterGroup<'_> {
    /// Return the exact 16-byte raw group name.
    pub fn name_bytes(&self) -> &[u8] {
        self.name
    }

    /// Return physical slots in raw logical-row order, not fixed 50-row blocks.
    pub fn physical_slots(&self) -> &[usize] {
        &self.slots
    }
}

fn unsupported(offset: usize, reason: &'static str) -> Error {
    Error::Unsupported {
        line: 5 + offset / RECORD_BYTES,
        reason,
    }
}

impl Icf<'_> {
    // Import RVA 0xC6A4C/C6AEC reads this prefix; native save 0xC6CE4
    // rebuilds identity order from working records. This raw API does not.
    fn repeater_raw_order(&self) -> Result<Vec<usize>, Error> {
        let mut order = Vec::new();
        let mut seen = [false; COUNT];
        let mut ended = false;
        for (row, bytes) in self.memory[ORDER..STARTS]
            .as_chunks::<2>()
            .0
            .iter()
            .enumerate()
        {
            let value = u16::from_le_bytes([bytes[0], bytes[1]]);
            if value == u16::MAX {
                ended = true;
                continue;
            }
            let at = ORDER + 2 * row;
            if ended {
                return Err(unsupported(at, "repeater row value after FFFF terminator"));
            }
            let slot = usize::from(value);
            if slot >= COUNT || seen[slot] {
                return Err(unsupported(
                    at,
                    "repeater order slot outside 0..2500 or repeated",
                ));
            }
            seen[slot] = true;
            order.push(slot);
        }
        Ok(order)
    }

    fn repeater_slot_used(&self, slot: usize) -> Result<bool, Error> {
        if slot >= COUNT {
            return Err(Error::Unsupported {
                line: 5,
                reason: "physical repeater slot outside 0..2500",
            });
        }
        Ok(self.repeater_raw_order()?.contains(&slot))
    }

    /// Read all 50 raw group names and their physical-slot order.
    ///
    /// Group starts are logical-row positions, not physical slots. Empty groups
    /// keep their ordinal/name. No working count, GUI list or save reconstruction
    /// is inferred. See CS-705 import RVA 0xC6A4C and group resolver 0xC6704.
    ///
    /// # Errors
    /// Rejects malformed order prefixes, invalid/nonincreasing starts, missing
    /// start zero or nonempty starts for an empty order. No data is repaired.
    pub fn repeater_groups(&self) -> Result<Vec<RepeaterGroup<'_>>, Error> {
        let order = self.repeater_raw_order()?;
        let mut groups: Vec<RepeaterGroup<'_>> = Vec::with_capacity(GROUP_COUNT);
        let mut previous: Option<(usize, usize)> = None;
        for group in 0..GROUP_COUNT {
            let at = STARTS + 2 * group;
            let raw = u16::from_le_bytes([self.memory[at], self.memory[at + 1]]);
            if raw != u16::MAX {
                let start = usize::from(raw);
                if start >= order.len()
                    || previous.is_none() && start != 0
                    || previous.is_some_and(|(_, old)| start <= old)
                {
                    return Err(unsupported(
                        at,
                        "repeater group starts must increase from zero within row count",
                    ));
                }
                if let Some((old_group, old_start)) = previous {
                    groups[old_group].slots = order[old_start..start].to_vec();
                }
                previous = Some((group, start));
            }
            groups.push(RepeaterGroup {
                name: &self.memory[NAMES + 16 * group..NAMES + 16 * (group + 1)],
                slots: Vec::new(),
            });
        }
        if let Some((group, start)) = previous {
            groups[group].slots = order[start..].to_vec();
        } else if !order.is_empty() {
            return Err(unsupported(
                STARTS,
                "repeater nonempty order requires a group start at zero",
            ));
        }
        Ok(groups)
    }

    /// Replace one fixed 16-byte raw group name with space-padded printable ASCII.
    ///
    /// Empty groups also have a name field. This edits no order/start/row and does
    /// not convert CP932, trim, clip or change case. Writer: RVA 0x6C795 -> 0x47FD0.
    ///
    /// # Errors
    /// Rejects group indices outside 0..50 before validating the new 0..=16-byte
    /// ASCII 20..7E value. All validation precedes one field copy.
    pub fn set_repeater_group_name_ascii(
        &mut self,
        group: usize,
        value: &str,
    ) -> Result<(), Error> {
        if group >= GROUP_COUNT {
            return Err(Error::Unsupported {
                line: 5,
                reason: "repeater group outside 0..50",
            });
        }
        let at = NAMES + 16 * group;
        let bytes = value.as_bytes();
        if bytes.len() > 16 || !bytes.iter().all(|b| (0x20..=0x7e).contains(b)) {
            return Err(Error::Invalid {
                line: 5 + at / RECORD_BYTES,
                reason: "repeater group name requires 0..=16 printable ASCII bytes",
            });
        }
        let mut field = [b' '; 16];
        field[..bytes.len()].copy_from_slice(bytes);
        self.memory[at..at + 16].copy_from_slice(&field);
        Ok(())
    }

    /// Borrow an occupied physical repeater record's exact 52 bytes.
    ///
    /// Occupancy is membership in the verified order prefix, never record bytes
    /// or USE. Group starts are deliberately not validated by this physical view.
    /// It neither decodes nor changes records or auxiliary references.
    ///
    /// # Errors
    /// Rejects out-of-range slots before multiplication and invalid raw order.
    pub fn physical_repeater_record_raw(&self, slot: usize) -> Result<Option<&[u8]>, Error> {
        if !self.repeater_slot_used(slot)? {
            return Ok(None);
        }
        let at = RECORDS + RECORD_SIZE * slot;
        Ok(Some(&self.memory[at..at + RECORD_SIZE]))
    }

    /// Read an occupied physical slot's RPT1USE bit, not deletion/working validity.
    ///
    /// CS-705 getter RVA 0x76F6 -> 0xD65B8; bit masks at 0x41E854 are LSB-first.
    /// Like the physical record view, this does not validate group starts.
    ///
    /// # Errors
    /// Rejects out-of-range slots or invalid raw order; unused slots return None.
    pub fn physical_repeater_use(&self, slot: usize) -> Result<Option<bool>, Error> {
        if !self.repeater_slot_used(slot)? {
            return Ok(None);
        }
        Ok(Some(
            self.memory[USE_BITS + (slot >> 3)] & (1 << (slot & 7)) != 0,
        ))
    }

    /// Change one existing RPT1USE bit, preserving all other bits and image bytes.
    ///
    /// Writer RVA 0x7FBF -> 0xD65E0. This does not create/delete rows or modify
    /// records, auxiliary fields, order, group starts or metadata. No I/O occurs.
    ///
    /// # Errors
    /// Rejects invalid order, unavailable slots or unused physical records before
    /// changing any byte. There is no fallback occupancy or record allocation.
    pub fn set_physical_repeater_use(&mut self, slot: usize, value: bool) -> Result<(), Error> {
        if self.physical_repeater_use(slot)?.is_none() {
            return Err(unsupported(
                USE_BITS + (slot >> 3),
                "cannot edit USE of an unused physical repeater slot",
            ));
        }
        let at = USE_BITS + (slot >> 3);
        let mask = 1 << (slot & 7);
        self.memory[at] = (self.memory[at] & !mask) | if value { mask } else { 0 };
        Ok(())
    }
}
