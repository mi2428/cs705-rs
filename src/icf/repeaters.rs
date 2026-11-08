//! Strict MapRev 6 raw repeater row/group mapping and stored RPT1USE bits.
//!
//! Static CS-705 evidence only, not original-GUI or device validation. This does
//! not expand working records, import CSV or create rows.

use super::memory::{pack_callsign, unpack_callsign};
use super::{Error, Icf, RECORD_BYTES};

const COUNT: usize = 2500;
const GROUP_COUNT: usize = 50;
const ORDER: usize = 0x3ab82;
const STARTS: usize = 0x3bf0a;
const NAMES: usize = 0x2ea3b;
const RECORDS: usize = 0xaa0f;
const RECORD_SIZE: usize = 52;
const USE_BITS: usize = 0x3bf6e;
const AUX: usize = 0x2a5df;

macro_rules! repeater_enum {
    ($ty:ident, $doc:literal, {$($name:ident = $raw:literal),+ $(,)?}) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum $ty {
            $(#[doc = concat!("Stored ", stringify!($name), " selection.")]
              $name = $raw,)+
        }
        impl $ty {
            fn decode(raw: u8, at: usize) -> Result<Self, Error> {
                match raw {
                    $($raw => Ok(Self::$name),)+
                    _ => Err(unsupported(at, concat!(stringify!($ty), " unknown stored code"))),
                }
            }
        }
    };
}

repeater_enum!(RepeaterMode, "Stored repeater mode, not GUI view order.", {
    Dv=0, Fm=1, FmNarrow=2, Dd=3
});
repeater_enum!(RepeaterDuplex, "Stored repeater duplex selection.", {
    Off=0, Minus=1, Plus=2, Rps=3
});
repeater_enum!(RepeaterTone, "Stored repeater tone selection.", {
    Off=0, Tone=1, ToneSquelch=2
});
repeater_enum!(RepeaterPosition, "Stored repeater position selection.", {
    None=0, Approximate=1, Exact=2
});

/// Exact integer coordinate storage, including negative zero; no rounding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepeaterCoordinate {
    /// Stored whole degrees.
    pub degrees: u8,
    /// Stored thousandths of one minute.
    pub thousandths_of_minute: u16,
    /// Stored south (latitude) or west (longitude) sign.
    pub negative: bool,
}

/// One physical repeater's stored fields; group membership is a separate API.
///
/// A getter exposes raw values, not a promise of canonical setter input. Unknown
/// raw data can be retained through the existing lossless/raw image path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeaterRecord {
    /// Exact fixed-width name bytes.
    pub name: [u8; 16],
    /// Exact fixed-width sub-name bytes.
    pub sub_name: [u8; 8],
    /// Unpacked seven-bit repeater callsign.
    pub repeater_call: [u8; 8],
    /// Unpacked indirect gateway callsign; FFFF reads as eight spaces.
    pub gateway_call: [u8; 8],
    /// Stored frequency in Hz.
    pub frequency_hz: u32,
    /// Stored offset in Hz.
    pub offset_hz: u32,
    /// Stored mode.
    pub mode: RepeaterMode,
    /// Stored duplex selection.
    pub duplex: RepeaterDuplex,
    /// Stored tone selection.
    pub tone: RepeaterTone,
    /// Raw tone byte for FM/FM-N; None for DV/DD, whose byte remains opaque.
    pub tone_index: Option<u8>,
    /// Physical RPT1USE bit, not occupancy.
    pub use_repeater: bool,
    /// Stored position selection.
    pub position: RepeaterPosition,
    /// Stored latitude, including south zero.
    pub latitude: RepeaterCoordinate,
    /// Stored longitude, including west zero.
    pub longitude: RepeaterCoordinate,
    /// Stored signed offset from UTC in minutes.
    pub utc_offset_minutes: i16,
}

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
    /// Read an occupied physical row as a typed wire view, not canonical input.
    ///
    /// Numeric values, text bytes and signed zero are returned without repair.
    /// FM-N/DD/RPS can be read but are not accepted by the replacement setter.
    /// Group starts are independent of this physical view.
    ///
    /// # Errors
    /// Rejects invalid slot/order, unknown tone/position codes or gateway links.
    /// Unused slots return None. No I/O or normalization occurs.
    pub fn physical_repeater(&self, slot: usize) -> Result<Option<RepeaterRecord>, Error> {
        let Some(r) = self.physical_repeater_record_raw(slot)? else {
            return Ok(None);
        };
        let at = RECORDS + RECORD_SIZE * slot;
        let mode = RepeaterMode::decode(r[10] >> 6, at + 10)?;
        let gateway = self.repeater_gateway_offset(at)?;
        let coordinate = |offset, mask| RepeaterCoordinate {
            degrees: r[offset],
            thousandths_of_minute: u16::from_be_bytes([r[offset + 1], r[offset + 2]]),
            negative: r[51] & mask != 0,
        };
        Ok(Some(RepeaterRecord {
            name: r[18..34].try_into().unwrap(),
            sub_name: r[34..42].try_into().unwrap(),
            repeater_call: unpack_callsign(&r[11..18]),
            gateway_call: gateway.map_or([b' '; 8], |a| unpack_callsign(&self.memory[a..a + 7])),
            frequency_hz: u32::from_le_bytes(r[..4].try_into().unwrap()),
            offset_hz: u32::from_le_bytes(r[4..8].try_into().unwrap()),
            mode,
            duplex: RepeaterDuplex::decode((r[10] >> 4) & 3, at + 10)?,
            tone: RepeaterTone::decode((r[10] >> 2) & 3, at + 10)?,
            tone_index: matches!(mode, RepeaterMode::Fm | RepeaterMode::FmNarrow).then_some(r[48]),
            use_repeater: self.memory[USE_BITS + (slot >> 3)] & (1 << (slot & 7)) != 0,
            position: RepeaterPosition::decode(r[10] & 3, at + 10)?,
            latitude: coordinate(42, 0x80),
            longitude: coordinate(45, 0x40),
            utc_offset_minutes: i16::from_le_bytes([r[49], r[50]]),
        }))
    }

    fn repeater_gateway_offset(&self, at: usize) -> Result<Option<usize>, Error> {
        let k = u16::from_le_bytes([self.memory[at + 8], self.memory[at + 9]]);
        if k == u16::MAX {
            Ok(None)
        } else if usize::from(k) < COUNT {
            Ok(Some(AUX + 7 * usize::from(k)))
        } else {
            Err(unsupported(
                at + 8,
                "repeater gateway index outside 0..2500 or FFFF",
            ))
        }
    }

    /// Atomically replace an existing row with canonical FM/DV stored values.
    ///
    /// No row allocation, gateway allocation/deduplication, CSV/GUI correction or
    /// group movement occurs. Changed nonblank gateways require a unique existing
    /// reference across all 2500 physical rows, including unused rows. Unchanged
    /// gateway bytes preserve links, and blank changes unlink without clearing aux.
    /// DV preserves its opaque tone byte; coordinate sign low bits are preserved.
    /// Other old fields need not decode successfully for an explicit replacement.
    ///
    /// # Errors
    /// Unsupported structure/modes/references or invalid canonical values leave
    /// the entire image and metadata unchanged. All checks precede every write.
    pub fn set_physical_repeater(
        &mut self,
        slot: usize,
        value: &RepeaterRecord,
    ) -> Result<(), Error> {
        if !self.repeater_slot_used(slot)? {
            return Err(unsupported(
                RECORDS + RECORD_SIZE * slot,
                "cannot replace an unused repeater row",
            ));
        }
        let at = RECORDS + RECORD_SIZE * slot;
        let gateway = self.repeater_gateway_offset(at)?;
        let invalid = |offset, reason| Error::Invalid {
            line: 5 + (at + offset) / RECORD_BYTES,
            reason,
        };
        if !matches!(value.mode, RepeaterMode::Fm | RepeaterMode::Dv)
            || value.duplex == RepeaterDuplex::Rps
        {
            return Err(unsupported(
                at + 10,
                "repeater replacement requires FM/DV and non-RPS duplex",
            ));
        }
        if !(30_000..=1_399_999_999).contains(&value.frequency_hz) {
            return Err(invalid(
                0,
                "repeater frequency outside 30000..=1399999999 Hz",
            ));
        }
        if value.offset_hz > 9_999_900 || !value.offset_hz.is_multiple_of(100) {
            return Err(invalid(
                4,
                "repeater offset requires 0..=9999900 Hz in steps of 100",
            ));
        }
        for (name, offset) in [(value.name.as_slice(), 18), (value.sub_name.as_slice(), 34)] {
            if !name.iter().all(|b| (0x20..=0x7e).contains(b)) {
                return Err(invalid(
                    offset,
                    "repeater names require fixed-width printable ASCII",
                ));
            }
        }
        for (call, offset) in [(&value.repeater_call, 11), (&value.gateway_call, 8)] {
            if !call
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || matches!(b, b'/' | b' '))
            {
                return Err(invalid(
                    offset,
                    "repeater callsigns require eight A-Z, 0-9, / or space bytes",
                ));
            }
        }
        if !matches!(value.gateway_call[7], b'G' | b' ') {
            return Err(invalid(8, "repeater gateway suffix requires G or space"));
        }
        if (value.mode == RepeaterMode::Fm
            && value.duplex == RepeaterDuplex::Off
            && (value.repeater_call != [b' '; 8] || value.gateway_call != [b' '; 8]))
            || (value.mode == RepeaterMode::Dv
                && value.repeater_call == [b' '; 8]
                && (value.gateway_call != [b' '; 8] || value.duplex != RepeaterDuplex::Off))
        {
            return Err(invalid(11, "repeater callsigns conflict with mode/duplex"));
        }
        match (value.mode, value.tone_index) {
            (RepeaterMode::Fm, Some(0..=49)) | (RepeaterMode::Dv, None) => {}
            _ => {
                return Err(invalid(
                    48,
                    "repeater FM tone index requires 0..=49; DV requires None",
                ));
            }
        }
        for (coordinate, limit, offset) in [(value.latitude, 90, 42), (value.longitude, 180, 45)] {
            if coordinate.degrees > limit
                || coordinate.thousandths_of_minute >= 60_000
                || (coordinate.degrees == limit && coordinate.thousandths_of_minute != 0)
            {
                return Err(invalid(
                    offset,
                    "repeater coordinate outside degree/minute domain",
                ));
            }
        }
        if !(-840..=840).contains(&value.utc_offset_minutes) || value.utc_offset_minutes % 5 != 0 {
            return Err(invalid(
                49,
                "repeater UTC offset requires -840..=840 minutes in steps of 5",
            ));
        }
        let packed_gateway = pack_callsign(&value.gateway_call);
        let current_gateway = gateway.map_or(pack_callsign(&[b' '; 8]), |a| {
            self.memory[a..a + 7].try_into().unwrap()
        });
        let changed_gateway = packed_gateway != current_gateway;
        let blank_gateway = value.gateway_call == [b' '; 8];
        if changed_gateway && !blank_gateway {
            let Some(_) = gateway else {
                return Err(unsupported(
                    at + 8,
                    "repeater gateway allocation is not supported",
                ));
            };
            let reference = &self.memory[at + 8..at + 10];
            let references = (0..COUNT)
                .filter(|p| {
                    let r = RECORDS + RECORD_SIZE * p;
                    &self.memory[r + 8..r + 10] == reference
                })
                .count();
            if references != 1 {
                return Err(unsupported(
                    at + 8,
                    "cannot change a shared repeater gateway",
                ));
            }
        }
        let mut row: [u8; RECORD_SIZE] = self.memory[at..at + RECORD_SIZE].try_into().unwrap();
        row[..4].copy_from_slice(&value.frequency_hz.to_le_bytes());
        row[4..8].copy_from_slice(&value.offset_hz.to_le_bytes());
        if changed_gateway && blank_gateway {
            row[8..10].fill(0xff);
        }
        row[10] = (value.mode as u8) << 6
            | (value.duplex as u8) << 4
            | (value.tone as u8) << 2
            | value.position as u8;
        row[11..18].copy_from_slice(&pack_callsign(&value.repeater_call));
        row[18..34].copy_from_slice(&value.name);
        row[34..42].copy_from_slice(&value.sub_name);
        for (coordinate, offset) in [(value.latitude, 42), (value.longitude, 45)] {
            row[offset] = coordinate.degrees;
            row[offset + 1..offset + 3]
                .copy_from_slice(&coordinate.thousandths_of_minute.to_be_bytes());
        }
        if let Some(tone) = value.tone_index {
            row[48] = tone;
        }
        row[49..51].copy_from_slice(&value.utc_offset_minutes.to_le_bytes());
        row[51] = row[51] & 0x3f
            | (u8::from(value.latitude.negative) << 7)
            | (u8::from(value.longitude.negative) << 6);
        // Commit only after every value, reference and auxiliary check succeeds.
        self.memory[at..at + RECORD_SIZE].copy_from_slice(&row);
        if changed_gateway && !blank_gateway {
            let a = gateway.unwrap();
            self.memory[a..a + 7].copy_from_slice(&packed_gateway);
        }
        let use_at = USE_BITS + (slot >> 3);
        let mask = 1 << (slot & 7);
        self.memory[use_at] =
            self.memory[use_at] & !mask | if value.use_repeater { mask } else { 0 };
        Ok(())
    }

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
