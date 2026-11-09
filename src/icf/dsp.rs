//! Canonical RF/DSP preferences, not GUI operations or device DSP behavior.
//!
//! Mode groups and band contexts are explicit, with no frequency/profile inference.
//! Each setter validates its new value and required context, then changes only
//! its field. Unrelated unknown bytes survive; GUI PBT/EQ resets are not applied.

use super::{Error, Icf, RECORD_BYTES};

macro_rules! dsp_enum {
    ($ty:ident, $doc:literal, {$($variant:ident = $code:literal),+ $(,)?}) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum $ty {
            $(#[doc = concat!("Stored ", stringify!($variant), " selection.")]
            $variant = $code,)+
        }
    };
}

dsp_enum!(FilterMode, "Filter preference group, not a device mode code.", {Ssb=0, SsbData=1, Cw=2, Rtty=3, Am=4});
dsp_enum!(FilterSlot, "One of three stored filter preferences.", {One=0, Two=1, Three=2});
dsp_enum!(PbtSide, "One of two stored PBT sides.", {One=0, Two=1});
dsp_enum!(FilterBandGroup, "Explicit filter-shape band group; no frequency inference.", {Hf=0, Mhz50To430=1});
dsp_enum!(FilterShapeSlot, "SSB/SSB-D wide and distinct CW narrow/wide shape slots; no dummy slots.", {Ssb=0, SsbData=1, CwNarrow=2, CwWide=3});
dsp_enum!(FilterShape, "Full-byte filter shape code.", {Sharp=0, Soft=1});
dsp_enum!(Preamp, "Stored preamp selection; Amp2 is unavailable in bands 13..=16.", {Off=0, Amp1=1, Amp2=2});
dsp_enum!(StepAgcMode, "Tuning-step/AGC preference group, distinct from FilterMode and ToneMode.", {Ssb=0, Cw=1, Rtty=2, Am=3, Fm=4, Wfm=5, Dv=6});
dsp_enum!(Agc, "Stored AGC selector; Fast code zero is not delay OFF.", {Fast=0, Mid=1, Slow=2});
dsp_enum!(ToneMode, "Tone preference group, distinct from other mode indices.", {Ssb=0, Am=1, Fm=2, Wfm=3, Dv=4, Cw=5, Rtty=6});
dsp_enum!(TransmitBandwidthPreset, "Independent SSB W/M/N and SSB-D endpoint presets.", {Wide=0, Mid=1, Narrow=2, Data=3});
dsp_enum!(TransmitLow, "Stored low endpoint selector, not raw Hz.", {Hz100=0, Hz200=1, Hz300=2, Hz500=3});
dsp_enum!(TransmitHigh, "Stored high endpoint selector, not raw Hz.", {Hz2500=0, Hz2700=1, Hz2800=2, Hz2900=3});

fn unsupported(at: usize, reason: &'static str) -> Error {
    Error::Unsupported {
        line: 5 + at / RECORD_BYTES,
        reason,
    }
}

fn invalid(at: usize, reason: &'static str) -> Error {
    Error::Invalid {
        line: 5 + at / RECORD_BYTES,
        reason,
    }
}

fn context(allowed: bool) -> Result<(), Error> {
    if allowed {
        Ok(())
    } else {
        Err(unsupported(0, "RF/DSP context is not a stored field"))
    }
}

fn filter_step(mode: FilterMode) -> i32 {
    if mode == FilterMode::Am { 200 } else { 50 }
}

// Repeated single-byte code fields share validation, not a public registry.
macro_rules! code_scalar {
    ($get:ident, $set:ident, ($($arg:ident: $ty:ty),*), $at:expr, $allowed:expr, $min:literal, $max:literal, $doc:literal) => {
        impl Icf<'_> {
            #[doc = $doc]
            /// # Errors
            /// Rejects unavailable contexts and unknown stored codes.
            pub fn $get(&self, $($arg: $ty),*) -> Result<u8, Error> {
                context($allowed)?;
                self.dsp_code($at, $min, $max)
            }
            #[doc = concat!("Replace only ", $doc)]
            /// # Errors
            /// Rejects unavailable contexts or invalid new codes before writing.
            /// Unknown current values do not prevent a legal replacement.
            pub fn $set(&mut self, $($arg: $ty,)* value: u8) -> Result<(), Error> {
                context($allowed)?;
                self.set_dsp_code($at, $min, $max, value)
            }
        }
    };
}

macro_rules! enum_scalar {
    ($get:ident, $set:ident, ($($arg:ident: $ty:ty),*), $at:expr, $allowed:expr, $value:ident, [$($variant:ident),+], $doc:literal) => {
        impl Icf<'_> {
            #[doc = $doc]
            /// # Errors
            /// Rejects unavailable contexts and unknown full-byte codes.
            pub fn $get(&self, $($arg: $ty),*) -> Result<$value, Error> {
                context($allowed)?;
                let values = [$($value::$variant),+];
                Ok(values[usize::from(self.dsp_code($at, 0, (values.len()-1) as u8)?)])
            }
            #[doc = concat!("Replace only ", $doc)]
            /// # Errors
            /// Rejects unavailable contexts before writing. Unknown current codes
            /// do not prevent a legal full-byte replacement.
            pub fn $set(&mut self, $($arg: $ty,)* value: $value) -> Result<(), Error> {
                context($allowed)?;
                self.memory[$at] = value as u8;
                Ok(())
            }
        }
    };
}

macro_rules! level_scalar {
    ($get:ident, $set:ident, $at:expr, $allowed:expr, $doc:literal) => {
        impl Icf<'_> {
            #[doc = $doc]
            /// # Errors
            /// Rejects unavailable tone contexts and raw codes outside 0..=10.
            pub fn $get(&self, mode: ToneMode) -> Result<i8, Error> {
                let t = mode as usize;
                context(($allowed)(t))?;
                Ok(self.dsp_code($at + t, 0, 10)? as i8 - 5)
            }
            #[doc = concat!("Replace only ", $doc)]
            /// No HPF/LPF reset or partner-field correction is performed.
            /// # Errors
            /// Rejects unavailable contexts or levels outside -5..=5 before writing.
            pub fn $set(&mut self, mode: ToneMode, value: i8) -> Result<(), Error> {
                let t = mode as usize;
                context(($allowed)(t))?;
                if !(-5..=5).contains(&value) {
                    return Err(invalid(
                        $at + t,
                        "EQ level requires -5..=5, not physical dB",
                    ));
                }
                self.memory[$at + t] = (value + 5) as u8;
                Ok(())
            }
        }
    };
}

impl Icf<'_> {
    fn dsp_code(&self, at: usize, min: u8, max: u8) -> Result<u8, Error> {
        let raw = self.memory[at];
        if (min..=max).contains(&raw) {
            Ok(raw)
        } else {
            Err(unsupported(at, "unknown stored RF/DSP code"))
        }
    }

    fn set_dsp_code(&mut self, at: usize, min: u8, max: u8, value: u8) -> Result<(), Error> {
        if !(min..=max).contains(&value) {
            return Err(invalid(at, "RF/DSP code outside the field domain"));
        }
        self.memory[at] = value;
        Ok(())
    }

    // BW read/store RVA 0xAFC9C/0xAFD10; table 0x407CA0.
    /// Read a known bandwidth code as exact Hz on the mode-specific grid.
    /// # Errors
    /// Rejects unknown bandwidth codes rather than extending the table.
    pub fn filter_bandwidth_hz(&self, mode: FilterMode, slot: FilterSlot) -> Result<u32, Error> {
        let max = match mode {
            FilterMode::Am => 49,
            FilterMode::Rtty => 31,
            _ => 40,
        };
        let code = u32::from(self.dsp_code(0x37850 + 3 * mode as usize + slot as usize, 0, max)?);
        Ok(if mode == FilterMode::Am {
            200 * (code + 1)
        } else if code < 10 {
            50 * (code + 1)
        } else {
            500 + 100 * (code - 9)
        })
    }

    /// Replace only the bandwidth byte; do not clear either stored PBT side.
    /// # Errors
    /// Rejects off-grid/out-of-range Hz before writing, without reading old BW.
    pub fn set_filter_bandwidth_hz(
        &mut self,
        mode: FilterMode,
        slot: FilterSlot,
        value: u32,
    ) -> Result<(), Error> {
        let at = 0x37850 + 3 * mode as usize + slot as usize;
        let max = match mode {
            FilterMode::Am => 10_000,
            FilterMode::Rtty => 2700,
            _ => 3600,
        };
        let (min, step) = if mode == FilterMode::Am {
            (200, 200)
        } else if value <= 500 {
            (50, 50)
        } else {
            (600, 100)
        };
        if !(min..=max).contains(&value) || !value.is_multiple_of(step) {
            return Err(invalid(
                at,
                "filter bandwidth requires exact mode-specific Hz grid",
            ));
        }
        self.memory[at] = if mode == FilterMode::Am {
            (value / 200 - 1) as u8
        } else if value <= 500 {
            (value / 50 - 1) as u8
        } else {
            ((value - 500) / 100 + 9) as u8
        };
        Ok(())
    }

    // PBT read/store RVA 0xAFEDC/0xAFF08; bounds 0xAFF4D..0xAFF9F.
    /// Read any stored signed PBT byte as Hz, including GUI-out-of-range values.
    /// # Errors
    /// This typed context has no unknown PBT code; all 256 byte values are readable.
    pub fn filter_pbt_hz(
        &self,
        mode: FilterMode,
        slot: FilterSlot,
        side: PbtSide,
    ) -> Result<i32, Error> {
        let at = 0x37867 + 6 * mode as usize + 2 * slot as usize + side as usize;
        Ok(i32::from(self.memory[at] as i8) * filter_step(mode))
    }

    /// Replace one PBT byte using this slot's known BW as required context.
    /// # Errors
    /// Rejects unknown BW, off-grid Hz, BW-bound violations or i8 overflow.
    pub fn set_filter_pbt_hz(
        &mut self,
        mode: FilterMode,
        slot: FilterSlot,
        side: PbtSide,
        value: i32,
    ) -> Result<(), Error> {
        let bw = self.filter_bandwidth_hz(mode, slot)? as i32;
        let d = if mode == FilterMode::Cw && bw >= 600 {
            (bw - 600) / 2
        } else {
            0
        };
        let step = filter_step(mode);
        let at = 0x37867 + 6 * mode as usize + 2 * slot as usize + side as usize;
        if value % step != 0
            || !(-bw / 2 - d..=bw / 2 - d).contains(&value)
            || i8::try_from(value / step).is_err()
        {
            return Err(invalid(
                at,
                "PBT requires mode step, known BW bounds and signed-byte range",
            ));
        }
        self.memory[at] = (value / step) as i8 as u8;
        Ok(())
    }

    // Derived read-only formulas RVA 0xAFFDC..0xB0038 / 0xB0044..0xB0083.
    /// Read BW minus the absolute PBT difference; negative values are not clamped.
    /// # Errors
    /// Rejects unknown bandwidth codes.
    pub fn filter_passband_hz(&self, mode: FilterMode, slot: FilterSlot) -> Result<i32, Error> {
        Ok(self.filter_bandwidth_hz(mode, slot)? as i32
            - (self.filter_pbt_hz(mode, slot, PbtSide::One)?
                - self.filter_pbt_hz(mode, slot, PbtSide::Two)?)
            .abs())
    }

    /// Read the mean stored PBT in Hz, dividing toward zero, without BW validation.
    /// # Errors
    /// All signed PBT bytes are readable for the typed mode/slot context.
    pub fn filter_shift_hz(&self, mode: FilterMode, slot: FilterSlot) -> Result<i32, Error> {
        Ok((self.filter_pbt_hz(mode, slot, PbtSide::One)?
            + self.filter_pbt_hz(mode, slot, PbtSide::Two)?)
            / 2)
    }

    // PAMP read/store RVA 0x7A635..0x7A721; ATT 0x7A726..0x7A811.
    /// Read masked PAMP for an explicit antenna-band index, not an EE90 class.
    /// # Errors
    /// Rejects indices outside 0..=16 and unknown codes for the selected band.
    pub fn preamp(&self, band: usize) -> Result<Preamp, Error> {
        let raw = self.preamp_raw(band)?;
        let at = 0x378af + band;
        match raw {
            0 => Ok(Preamp::Off),
            1 => Ok(Preamp::Amp1),
            2 if band <= 12 => Ok(Preamp::Amp2),
            _ => Err(unsupported(at, "unknown preamp code for antenna band")),
        }
    }

    /// Replace PAMP bits only, preserving ATT and all other bits.
    /// # Errors
    /// Rejects indices outside 0..=16 or Amp2 for bands 13..=16 before writing.
    pub fn set_preamp(&mut self, band: usize, value: Preamp) -> Result<(), Error> {
        context(band < 17)?;
        let at = 0x378af + band;
        if band > 12 && value == Preamp::Amp2 {
            return Err(invalid(
                at,
                "upper antenna bands only support preamp Off/Amp1",
            ));
        }
        self.set_preamp_raw(band, value as i32)
    }

    /// Native selected-event storage, including unknown code3; no UI band gate.
    pub fn preamp_raw(&self, band: usize) -> Result<u8, Error> {
        context(band < 17)?;
        Ok(self.memory[0x378af + band] & 3)
    }
    pub fn set_preamp_raw(&mut self, band: usize, candidate: i32) -> Result<(), Error> {
        context(band < 17)?;
        let at = 0x378af + band;
        self.memory[at] = (self.memory[at] & !3) | (candidate as u8 & 3);
        Ok(())
    }

    /// Read ATT for bands 0..=12 without decoding the PAMP bits.
    /// # Errors
    /// Rejects unavailable band indices before address arithmetic.
    pub fn attenuator(&self, band: usize) -> Result<bool, Error> {
        context(band < 13)?;
        Ok(self.attenuator_raw(band)? != 0)
    }

    /// Native PAMP post-store gray reads ATT bit2 for all17 antenna bands.
    pub fn attenuator_raw(&self, band: usize) -> Result<u8, Error> {
        context(band < 17)?;
        Ok((self.memory[0x378af + band] >> 2) & 1)
    }

    /// Replace ATT only, even if the stored PAMP code is unknown.
    /// # Errors
    /// Rejects unavailable band indices before any write.
    pub fn set_attenuator(&mut self, band: usize, value: bool) -> Result<(), Error> {
        self.set_attenuator_raw(band, i32::from(value))
    }
    /// Selected ATT uses just candidate bit0; preserves PAMP and opaque bits.
    pub fn set_attenuator_raw(&mut self, band: usize, candidate: i32) -> Result<(), Error> {
        context(band < 13)?;
        let at = 0x378af + band;
        self.memory[at] = self.memory[at] & !4 | ((candidate as u8 & 1) << 2);
        Ok(())
    }
}

// Shape read/store RVA 0x79767/0x797C1, all eight full-byte slots.
enum_scalar!(filter_shape, set_filter_shape, (band: FilterBandGroup, slot: FilterShapeSlot),
    0x3785f + 4 * band as usize + slot as usize, true, FilterShape, [Sharp, Soft], "the full-byte filter shape.");
// TS read/store RVA 0x79D5E..0x79DCE; AIR 0x79E13..0x79E5D.
code_scalar!(tuning_step_code, set_tuning_step_code, (mode: StepAgcMode),
    0x37885 + mode as usize, true, 0, 12, "the tuning-step code (code 5 is not inferred raw Hz).");
code_scalar!(
    air_tuning_step_code,
    set_air_tuning_step_code,
    (),
    0x3788d,
    true,
    0,
    12,
    "the independent AIR tuning-step code."
);
// AGC read/store RVA 0x798D9/0x79987; delays 0x79AAA/0x79BD0/0x79CF6.
enum_scalar!(agc, set_agc, (mode: StepAgcMode), 0x378a7 + mode as usize,
    (mode as u8) < 4, Agc, [Fast, Mid, Slow], "the AGC selector, not fixed FM/WFM/DV displays.");
code_scalar!(agc_delay_code, set_agc_delay_code, (mode: StepAgcMode, state: Agc),
    0x3788f + 3 * mode as usize + state as usize, (mode as u8) < 4, 0, 13,
    "the AGC delay code (zero is OFF; no inferred milliseconds).");
// HPF/LPF read/store RVA 0x79F53..0x7A004 / 0x7A049..0x7A103.
code_scalar!(receive_high_pass_code, set_receive_high_pass_code, (mode: ToneMode),
    0x392da + mode as usize, mode != ToneMode::Wfm, 0, 20, "the receive HPF code (zero is OFF), without partner/EQ correction.");
code_scalar!(receive_low_pass_code, set_receive_low_pass_code, (mode: ToneMode),
    0x392e2 + mode as usize, mode != ToneMode::Wfm, 5, 25, "the receive LPF code (25 is OFF), without partner/EQ correction.");
// EQ read/store handlers RVA 0x7A148/0x7A20E/0x7A2D4/0x7A36A; bias 0xA6D48.
level_scalar!(
    receive_bass,
    set_receive_bass,
    0x392ca,
    |t| t <= 4,
    "the receive bass level, not physical dB."
);
level_scalar!(
    receive_treble,
    set_receive_treble,
    0x392d2,
    |t| t <= 4,
    "the receive treble level, not physical dB."
);
level_scalar!(
    transmit_bass,
    set_transmit_bass,
    0x392c0,
    |t| t <= 4 && t != 3,
    "the transmit bass level, not physical dB."
);
level_scalar!(
    transmit_treble,
    set_transmit_treble,
    0x392c5,
    |t| t <= 4 && t != 3,
    "the transmit treble level, not physical dB."
);
// Endpoint read/store RVA 0x7A402..0x7A4E3 / 0x7A528..0x7A5F0.
enum_scalar!(transmit_bandwidth_low, set_transmit_bandwidth_low, (preset: TransmitBandwidthPreset),
    0x392ea + 2 * preset as usize, true, TransmitLow, [Hz100, Hz200, Hz300, Hz500], "the independent transmit low endpoint, not the common TBW selector.");
enum_scalar!(transmit_bandwidth_high, set_transmit_bandwidth_high, (preset: TransmitBandwidthPreset),
    0x392eb + 2 * preset as usize, true, TransmitHigh, [Hz2500, Hz2700, Hz2800, Hz2900], "the independent transmit high endpoint, not the common TBW selector.");
