//! Occupied physical-slot scalar storage codecs, statically traced from CS-705.
//!
//! No original-GUI or hardware comparison is implied. These APIs do not create
//! slots, change GUI mode/DATA caches, infer MTON/region rules or perform I/O.
//! Parameter errors use line 5; stored-field errors identify their record.

use super::{
    Error, Icf, MEMORY_CHANNEL_BYTES, MEMORY_CHANNEL_COUNT, MEMORY_EMPTY_FLAGS, RECORD_BYTES,
};

/// Stored side of an occupied physical slot; Split requires the SPLIT flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemorySide {
    /// Main stored side.
    Main,
    /// Split stored side, without creating or initializing it.
    Split,
}

/// Select one of the three packed DV callsign windows, not a callsign table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryCallsign {
    /// Destination callsign.
    Your,
    /// First repeater callsign.
    Repeater1,
    /// Second repeater callsign.
    Repeater2,
}

impl MemoryCallsign {
    fn index(self) -> usize {
        match self {
            Self::Your => 0,
            Self::Repeater1 => 1,
            Self::Repeater2 => 2,
        }
    }
}

macro_rules! memory_enum {
    ($ty:ident, $doc:literal, {$($name:ident = $raw:literal),+ $(,)?}) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum $ty {
            $(#[doc = concat!("Stored ", stringify!($name), " selection.")]
              $name = $raw,)+
        }
        impl $ty {
            fn decode(raw: u8, offset: usize) -> Result<Self, Error> {
                match raw {
                    $($raw => Ok(Self::$name),)+
                    _ => Err(unsupported(offset, concat!(stringify!($ty), " unknown stored code"))),
                }
            }
        }
    };
}

// Mode/filter: read RVA 0x63A4..0x63CB; filter write 0x7949..0x795F.
memory_enum!(MemoryMode, "Stored normal-memory mode; no mode transition setter.", {
    Lsb=0, Usb=1, Cw=2, CwReverse=3, Rtty=4, RttyReverse=5, Am=6, Fm=7, Wfm=8, Dv=9
});
memory_enum!(MemoryFilter, "Stored filter nibble, including the unnormalized WFM value.", {
    Filter1=0, Filter2=1, Filter3=2
});
// SEL: read 0x6316..0x6325, write 0x78D0..0x78E5.
memory_enum!(MemorySelection, "Stored shared selection bits.", {Off=0, One=1, Two=2, Three=3});
// DUP: read 0x63F0..0x6400, write 0x797E..0x7995.
memory_enum!(MemoryDuplex, "Stored normal-memory duplex bits, not radio operation.", {Off=0, Minus=1, Plus=2});
// FM tone: read 0x6470..0x6482, write 0x79DF..0x79EC, table 0x19046E.
memory_enum!(MemoryTone, "Stored FM tone selection codes, not emitted tones.", {
    Off=0, Tone=1, ToneSquelch=2, Dtcs=3, DtcsTransmit=4,
    ToneTransmitDtcsReceive=5, DtcsTransmitToneSquelchReceive=6, ToneTransmitToneSquelchReceive=7
});
// DV: read 0x64DF..0x64F0, write 0x7A5A..0x7A6A, table 0x190486.
memory_enum!(MemoryDigitalSquelch, "Stored DV digital-squelch selection.", {Off=0, Callsign=1, Code=2});

fn unsupported(offset: usize, reason: &'static str) -> Error {
    Error::Unsupported {
        line: 5 + offset / RECORD_BYTES,
        reason,
    }
}

fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 5, reason }
}

pub(super) fn unpack_callsign(bytes: &[u8]) -> [u8; 8] {
    let bits = bytes
        .iter()
        .fold(0_u64, |value, byte| value << 8 | u64::from(*byte));
    std::array::from_fn(|i| ((bits >> (7 * (7 - i))) & 0x7f) as u8)
}

pub(super) fn pack_callsign(value: &[u8; 8]) -> [u8; 7] {
    let bits = value
        .iter()
        .fold(0_u64, |bits, byte| bits << 7 | u64::from(*byte));
    bits.to_be_bytes()[1..].try_into().unwrap()
}

macro_rules! overlay_enum {
    ($get:ident, $set:ident, $ty:ident, $mode:ident) => {
        impl Icf<'_> {
            #[doc = concat!("Read the stored ", stringify!($ty), " selection in ", stringify!($mode), ".")]
            /// # Errors
            /// Rejects unavailable slots/sides, incompatible modes and unknown codes.
            pub fn $get(&self, slot: usize, side: MemorySide) -> Result<$ty, Error> {
                let t = self.memory_overlay_offset(slot, side, MemoryMode::$mode)?;
                $ty::decode(self.memory[t + 6], t + 6)
            }
            /// Change only the stored selection byte, not operating state.
            /// # Errors
            /// Rejects incompatible modes or unknown current codes without changes.
            pub fn $set(&mut self, slot: usize, side: MemorySide, value: $ty) -> Result<(), Error> {
                self.$get(slot, side)?;
                let t = self.memory_side_offset(slot, side)?;
                self.memory[t + 6] = value as u8;
                Ok(())
            }
        }
    };
}

macro_rules! overlay_index {
    ($get:ident, $set:ident, $mode:ident, $at:literal, $max:literal, $doc:literal) => {
        impl Icf<'_> {
            #[doc = $doc]
            /// # Errors
            /// Rejects unavailable slots/sides, incompatible modes or unknown indices.
            pub fn $get(&self, slot: usize, side: MemorySide) -> Result<u8, Error> {
                let t = self.memory_overlay_offset(slot, side, MemoryMode::$mode)?;
                let value = self.memory[t + $at];
                if value > $max {
                    return Err(unsupported(
                        t + $at,
                        concat!(stringify!($get), " current index outside supported range"),
                    ));
                }
                Ok(value)
            }
            /// Change only this stored index byte; do not apply it to a radio.
            /// # Errors
            /// Invalid new indices are rejected before current-state checks.
            /// Unknown current indices or incompatible modes are never repaired.
            pub fn $set(&mut self, slot: usize, side: MemorySide, value: u8) -> Result<(), Error> {
                if value > $max {
                    return Err(invalid(concat!(
                        stringify!($get),
                        " new index outside supported range"
                    )));
                }
                self.$get(slot, side)?;
                let t = self.memory_side_offset(slot, side)?;
                self.memory[t + $at] = value;
                Ok(())
            }
        }
    };
}

overlay_enum!(
    physical_memory_tone,
    set_physical_memory_tone,
    MemoryTone,
    Fm
);
overlay_enum!(
    physical_memory_digital_squelch,
    set_physical_memory_digital_squelch,
    MemoryDigitalSquelch,
    Dv
);
// TX/RX tone read 0x6485..0x649A, write 0x79EF..0x79FE. MTON 50 excluded.
overlay_index!(
    physical_memory_tx_tone_index,
    set_physical_memory_tx_tone_index,
    Fm,
    9,
    49,
    "Read FM TX tone index 0..=49, not Hz; MTON index 50 is unsupported."
);
overlay_index!(
    physical_memory_rx_tone_index,
    set_physical_memory_rx_tone_index,
    Fm,
    10,
    49,
    "Read FM RX tone index 0..=49, not Hz; MTON index 50 is unsupported."
);
// DTCS read 0x649D..0x64A3, write 0x7A01..0x7A07; code read/write 0x64F6/0x7A6D.
overlay_index!(
    physical_memory_dtcs_index,
    set_physical_memory_dtcs_index,
    Fm,
    11,
    103,
    "Read FM DTCS table index 0..=103, not a printed code or BCD number."
);
overlay_index!(
    physical_memory_digital_code,
    set_physical_memory_digital_code,
    Dv,
    7,
    99,
    "Read stored DV digital code 0..=99, independent of the squelch selection."
);

impl Icf<'_> {
    fn memory_slot_base(&self, slot: usize) -> Result<usize, Error> {
        if slot >= MEMORY_CHANNEL_COUNT {
            return Err(Error::Unsupported {
                line: 5,
                reason: "normal physical memory slot outside 0..500",
            });
        }
        if self.memory[MEMORY_EMPTY_FLAGS + slot] & 1 != 0 {
            return Err(unsupported(
                MEMORY_EMPTY_FLAGS + slot,
                "physical memory slot is empty",
            ));
        }
        Ok(MEMORY_CHANNEL_BYTES * slot)
    }

    fn memory_side_offset(&self, slot: usize, side: MemorySide) -> Result<usize, Error> {
        let b = self.memory_slot_base(slot)?;
        match side {
            MemorySide::Main => Ok(b + 1),
            MemorySide::Split if self.memory[b] & 2 != 0 => Ok(b + 0x22),
            MemorySide::Split => Err(unsupported(b, "physical memory split side is inactive")),
        }
    }

    fn memory_overlay_offset(
        &self,
        slot: usize,
        side: MemorySide,
        mode: MemoryMode,
    ) -> Result<usize, Error> {
        let t = self.memory_side_offset(slot, side)?;
        if MemoryMode::decode(self.memory[t + 4] >> 4, t + 4)? != mode {
            return Err(unsupported(
                t + 4,
                "physical memory field incompatible with stored mode",
            ));
        }
        Ok(t)
    }

    /// Read all stored frequency bits as integer Hz, even outside the input envelope.
    /// # Errors
    /// Rejects unavailable slots/sides, not zero or out-of-envelope stored frequencies.
    pub fn physical_memory_frequency_hz(
        &self,
        slot: usize,
        side: MemorySide,
    ) -> Result<u32, Error> {
        let t = self.memory_side_offset(slot, side)?;
        Ok(u32::from_le_bytes(
            self.memory[t..t + 4]
                .try_into()
                .expect("four-byte frequency window"),
        ))
    }

    /// Store integer Hz in 30,000..=470,000,000 without region/gap inference.
    /// # Errors
    /// Rejects invalid input first, then unavailable slots/sides. No bytes change on error.
    pub fn set_physical_memory_frequency_hz(
        &mut self,
        slot: usize,
        side: MemorySide,
        value: u32,
    ) -> Result<(), Error> {
        if !(30_000..=470_000_000).contains(&value) {
            return Err(invalid(
                "physical memory frequency outside 30000..=470000000 Hz",
            ));
        }
        self.physical_memory_frequency_hz(slot, side)?;
        let t = self.memory_side_offset(slot, side)?;
        self.memory[t..t + 4].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    /// Read the stored mode nibble. Mode/cache/filter transitions are not implemented.
    /// # Errors
    /// Rejects unavailable slots/sides or mode codes 10..15.
    pub fn physical_memory_mode(&self, slot: usize, side: MemorySide) -> Result<MemoryMode, Error> {
        let t = self.memory_side_offset(slot, side)?;
        MemoryMode::decode(self.memory[t + 4] >> 4, t + 4)
    }

    /// Read the stored DATA bit without normalizing inactive-mode residual values.
    /// # Errors
    /// Rejects unavailable slots/sides.
    pub fn physical_memory_data(&self, slot: usize, side: MemorySide) -> Result<bool, Error> {
        let t = self.memory_side_offset(slot, side)?;
        Ok(self.memory[t + 5] & 1 != 0)
    }

    /// Read the actual filter nibble, including WFM's unnormalized stored value.
    /// # Errors
    /// Rejects unavailable slots/sides or unknown filter codes.
    pub fn physical_memory_filter(
        &self,
        slot: usize,
        side: MemorySide,
    ) -> Result<MemoryFilter, Error> {
        let t = self.memory_side_offset(slot, side)?;
        MemoryFilter::decode(self.memory[t + 4] & 15, t + 4)
    }

    /// Change only the filter nibble, not a mode/cache preference or WFM fixed display.
    /// # Errors
    /// Rejects WFM, unknown modes/filters and unavailable slots/sides without changes.
    pub fn set_physical_memory_filter(
        &mut self,
        slot: usize,
        side: MemorySide,
        value: MemoryFilter,
    ) -> Result<(), Error> {
        if self.physical_memory_mode(slot, side)? == MemoryMode::Wfm {
            let t = self.memory_side_offset(slot, side)?;
            return Err(unsupported(
                t + 4,
                "physical memory WFM filter editing unsupported",
            ));
        }
        self.physical_memory_filter(slot, side)?;
        let t = self.memory_side_offset(slot, side)?;
        self.memory[t + 4] = (self.memory[t + 4] & 0xf0) | value as u8;
        Ok(())
    }

    /// Read shared SPLIT bit 02, not split UI rows.
    /// # Errors
    /// Rejects unavailable slots.
    pub fn physical_memory_split(&self, slot: usize) -> Result<bool, Error> {
        Ok(self.memory[self.memory_slot_base(slot)?] & 2 != 0)
    }

    /// Change only SPLIT bit 02; never initialize, clear or create the second side.
    /// # Errors
    /// Rejects unavailable slots without changes.
    pub fn set_physical_memory_split(&mut self, slot: usize, value: bool) -> Result<(), Error> {
        self.physical_memory_split(slot)?;
        let b = self.memory_slot_base(slot)?;
        self.memory[b] = (self.memory[b] & !2) | (u8::from(value) << 1);
        Ok(())
    }

    /// Read selection bits C0 from the physical flag, not the record SPLIT byte.
    /// # Errors
    /// Rejects unavailable slots.
    pub fn physical_memory_selection(&self, slot: usize) -> Result<MemorySelection, Error> {
        self.memory_slot_base(slot)?;
        let b = MEMORY_EMPTY_FLAGS + slot;
        MemorySelection::decode(self.memory[b] >> 6, b)
    }

    /// Change only flag selection bits C0, preserving occupancy and all record bytes.
    /// # Errors
    /// Rejects unavailable slots without changes.
    pub fn set_physical_memory_selection(
        &mut self,
        slot: usize,
        value: MemorySelection,
    ) -> Result<(), Error> {
        self.physical_memory_selection(slot)?;
        let b = MEMORY_EMPTY_FLAGS + slot;
        self.memory[b] = (self.memory[b] & 0x3f) | ((value as u8) << 6);
        Ok(())
    }

    /// Read DUP bits 06 without treating grey/SPLIT UI conditions as a rejection.
    /// # Errors
    /// Rejects unavailable slots/sides or unknown DUP code 3.
    pub fn physical_memory_duplex(
        &self,
        slot: usize,
        side: MemorySide,
    ) -> Result<MemoryDuplex, Error> {
        let t = self.memory_side_offset(slot, side)?;
        MemoryDuplex::decode((self.memory[t + 5] >> 1) & 3, t + 5)
    }

    /// Change only DUP bits 06, preserving DATA and all opaque control bits.
    /// # Errors
    /// Rejects unknown current DUP codes or unavailable slots/sides without changes.
    pub fn set_physical_memory_duplex(
        &mut self,
        slot: usize,
        side: MemorySide,
        value: MemoryDuplex,
    ) -> Result<(), Error> {
        self.physical_memory_duplex(slot, side)?;
        let t = self.memory_side_offset(slot, side)?;
        self.memory[t + 5] = (self.memory[t + 5] & !6) | ((value as u8) << 1);
        Ok(())
    }

    /// Read canonical stored offset Hz (0..=9,999,900 in steps of 100).
    /// # Errors
    /// Rejects noncanonical current values; never rounds or clamps them.
    pub fn physical_memory_offset_hz(&self, slot: usize, side: MemorySide) -> Result<u32, Error> {
        let t = self.memory_side_offset(slot, side)?;
        let value = u32::from_le_bytes(
            self.memory[t + 29..t + 33]
                .try_into()
                .expect("four-byte offset window"),
        );
        if value > 9_999_900 || !value.is_multiple_of(100) {
            return Err(unsupported(
                t + 29,
                "physical memory offset outside canonical range or step",
            ));
        }
        Ok(value)
    }

    /// Store canonical offset Hz without DUP/SPLIT grey-condition rejection.
    /// # Errors
    /// Rejects invalid input first, then unknown current values or unavailable sides.
    pub fn set_physical_memory_offset_hz(
        &mut self,
        slot: usize,
        side: MemorySide,
        value: u32,
    ) -> Result<(), Error> {
        if value > 9_999_900 || !value.is_multiple_of(100) {
            return Err(invalid(
                "physical memory offset outside canonical range or step",
            ));
        }
        self.physical_memory_offset_hz(slot, side)?;
        let t = self.memory_side_offset(slot, side)?;
        self.memory[t + 29..t + 33].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    /// Read FM polarity as (RX reverse, TX reverse), using two whole binary bytes.
    /// # Errors
    /// Rejects nonbinary current bytes, incompatible modes or unavailable slots/sides.
    pub fn physical_memory_dtcs_polarity(
        &self,
        slot: usize,
        side: MemorySide,
    ) -> Result<(bool, bool), Error> {
        let t = self.memory_overlay_offset(slot, side, MemoryMode::Fm)?;
        for at in [t + 7, t + 8] {
            if self.memory[at] > 1 {
                return Err(unsupported(
                    at,
                    "physical memory DTCS polarity byte other than 0 or 1",
                ));
            }
        }
        Ok((self.memory[t + 7] != 0, self.memory[t + 8] != 0))
    }

    /// Store both FM polarity bytes in RX,TX order, without canonicalizing unknown bytes.
    /// # Errors
    /// Rejects unknown current bytes or incompatible modes before either byte changes.
    pub fn set_physical_memory_dtcs_polarity(
        &mut self,
        slot: usize,
        side: MemorySide,
        value: (bool, bool),
    ) -> Result<(), Error> {
        self.physical_memory_dtcs_polarity(slot, side)?;
        let t = self.memory_side_offset(slot, side)?;
        self.memory[t + 7..t + 9].copy_from_slice(&[u8::from(value.0), u8::from(value.1)]);
        Ok(())
    }

    /// Decode exactly eight raw 7-bit DV callsign bytes, without trimming or correction.
    /// # Errors
    /// Rejects incompatible modes or unavailable slots/sides, not stored alphabets.
    pub fn physical_memory_callsign_raw(
        &self,
        slot: usize,
        side: MemorySide,
        which: MemoryCallsign,
    ) -> Result<[u8; 8], Error> {
        let t = self.memory_overlay_offset(slot, side, MemoryMode::Dv)?;
        let at = t + 8 + 7 * which.index();
        Ok(unpack_callsign(&self.memory[at..at + 7]))
    }

    /// Store exactly eight `[A-Z0-9/ ]` bytes into one packed DV callsign window.
    ///
    /// Strict final-storage API, not the original writer's automatic G-to-space
    /// correction. All three post-edit callsigns are checked: nonblank RPT2 with
    /// RPT1 ending in G is invalid, even on a Your edit. Explicit repeater edits
    /// may resolve that inconsistency. Unrelated raw alphabets remain untouched.
    /// # Errors
    /// Rejects invalid input first, then incompatible modes/unavailable sides or
    /// an invalid post-edit combination, before writing any of the seven bytes.
    pub fn set_physical_memory_callsign_raw(
        &mut self,
        slot: usize,
        side: MemorySide,
        which: MemoryCallsign,
        value: [u8; 8],
    ) -> Result<(), Error> {
        if !value
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || matches!(b, b'/' | b' '))
        {
            return Err(invalid(
                "physical memory callsign requires eight A-Z, 0-9, / or space bytes",
            ));
        }
        let t = self.memory_overlay_offset(slot, side, MemoryMode::Dv)?;
        let mut callsigns = [
            self.physical_memory_callsign_raw(slot, side, MemoryCallsign::Your)?,
            self.physical_memory_callsign_raw(slot, side, MemoryCallsign::Repeater1)?,
            self.physical_memory_callsign_raw(slot, side, MemoryCallsign::Repeater2)?,
        ];
        callsigns[which.index()] = value;
        let at = t + 8 + 7 * which.index();
        if callsigns[2] != [b' '; 8] && callsigns[1][7] == b'G' {
            return Err(Error::Invalid {
                line: 5 + at / RECORD_BYTES,
                reason: "physical memory nonblank RPT2 conflicts with RPT1 ending in G",
            });
        }
        self.memory[at..at + 7].copy_from_slice(&pack_callsign(&value));
        Ok(())
    }
}
