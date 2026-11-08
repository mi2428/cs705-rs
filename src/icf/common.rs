use super::{Error, Icf, RECORD_BYTES};

// Key 0x233 dispatches to RVA 0x73523: block 0x392C0 + byte 0x34.
// GI_Sheet_Sel_Misc_OnOff at RVA 0x3177F2 maps OFF/ON to 0/1.
// The official reference's native GUI OFF edit, Rust output and native resave
// matched byte-for-byte, SHA-256:
// 084591cc157fcff73058d776beb2c61fd9aeab9031c566ba6a7fa9687ebe3065.
const KEY_TOUCH_BEEP: usize = 0x392f4;
// The following keys use the same block, with full, unmasked u8 reads/writes.
// Key 0x232: read RVA 0x734E2, write 0x7351B; OnOff cells at 0x3177F2.
const BEEP_LEVEL_LIMIT: usize = 0x392f3;
// Key 0x234: read RVA 0x73574, write 0x735AD; the same OnOff cells.
const HOME_CHANNEL_BEEP: usize = 0x392f5;
// Key 0x235: read RVA 0x735BD, write 0x735F6; choice cells at 0x33E5F6.
const BAND_EDGE_BEEP: usize = 0x392ff;
// Key 0x23B: read RVA 0x73606, write 0x7363F; choice cells at 0x33E704.
const AUTO_POWER_OFF: usize = 0x39300;
// Key 0x23C: read RVA 0x7364F, write 0x73688; choice cells at 0x33E7F0.
const POWER_SAVE: usize = 0x39301;
// Fixed-reference GUI edits, Rust outputs and original-application resaves
// matched byte-for-byte for these five transitions, in the field order above:
// 01->00, 01->00, 03->02, 00->04, 01->03. Other values/images were not compared.

/// Stored band-edge beep selection (CS-705 help 55028).
///
/// These are selection codes, not commands to transmit or enforce limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BandEdgeBeep {
    /// Disable band-edge beeps.
    Off = 0,
    /// Use the default band edges.
    DefaultEdges = 1,
    /// Use the separately stored user band edges.
    UserEdges = 2,
    /// Select user band edges with the radio's transmit-limit option.
    UserEdgesWithTransmitLimit = 3,
}

/// Stored inactivity power-off selection (CS-705 help 55030).
///
/// The encoded values 0 through 4 are selection codes, not minute counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AutoPowerOff {
    /// Disable automatic power-off.
    Off = 0,
    /// Select 30 minutes of inactivity.
    Minutes30 = 1,
    /// Select 60 minutes of inactivity.
    Minutes60 = 2,
    /// Select 90 minutes of inactivity.
    Minutes90 = 3,
    /// Select 120 minutes of inactivity.
    Minutes120 = 4,
}

/// Stored power-save selection (CS-705 help 55032).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PowerSave {
    /// Disable power saving.
    Off = 0,
    /// Select the short automatic receive interval.
    AutoShort = 1,
    /// Select the medium automatic receive interval.
    AutoMedium = 2,
    /// Select the long automatic receive interval.
    AutoLong = 3,
}

impl Icf<'_> {
    /// Read the stored key/function confirmation-beep flag (CS-705 help 55024).
    ///
    /// This is a configuration value, not an audio operation. A zero beep level
    /// can still silence confirmation beeps even when this flag is enabled.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for values other than OFF (0) or ON (1).
    pub fn key_touch_beep(&self) -> Result<bool, Error> {
        match self.memory[KEY_TOUCH_BEEP] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Unsupported {
                line: 5 + KEY_TOUCH_BEEP / RECORD_BYTES,
                reason: "key-touch beep value other than OFF or ON",
            }),
        }
    }

    /// Change only the stored confirmation-beep byte, preserving all other data.
    ///
    /// Use [`Self::to_bytes`] for the edited image; [`Self::original_bytes`]
    /// continues to return the untouched input. No radio or audio I/O occurs.
    ///
    /// # Errors
    ///
    /// Refuses to overwrite an unsupported existing flag value.
    pub fn set_key_touch_beep(&mut self, enabled: bool) -> Result<(), Error> {
        self.key_touch_beep()?;
        self.memory[KEY_TOUCH_BEEP] = u8::from(enabled);
        Ok(())
    }

    /// Read the stored beep-level limit flag (CS-705 help 55022).
    ///
    /// This selects a cap on beep volume as AF volume increases; it does not
    /// change AF volume or the separate beep-level setting.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for values other than OFF (0) or ON (1).
    pub fn beep_level_limit(&self) -> Result<bool, Error> {
        match self.memory[BEEP_LEVEL_LIMIT] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Unsupported {
                line: 5 + BEEP_LEVEL_LIMIT / RECORD_BYTES,
                reason: "beep-level limit value other than OFF or ON",
            }),
        }
    }

    /// Change only the stored beep-level limit byte.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported existing value without changing any state.
    pub fn set_beep_level_limit(&mut self, enabled: bool) -> Result<(), Error> {
        self.beep_level_limit()?;
        self.memory[BEEP_LEVEL_LIMIT] = u8::from(enabled);
        Ok(())
    }

    /// Read the stored home-channel beep flag (CS-705 help 55026).
    ///
    /// This selects notification on choosing a registered VFO frequency, memory
    /// channel or DR FROM repeater; it does not edit those registrations.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for values other than OFF (0) or ON (1).
    pub fn home_channel_beep(&self) -> Result<bool, Error> {
        match self.memory[HOME_CHANNEL_BEEP] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Unsupported {
                line: 5 + HOME_CHANNEL_BEEP / RECORD_BYTES,
                reason: "home-channel beep value other than OFF or ON",
            }),
        }
    }

    /// Change only the stored home-channel beep byte.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported existing value without changing any state.
    pub fn set_home_channel_beep(&mut self, enabled: bool) -> Result<(), Error> {
        self.home_channel_beep()?;
        self.memory[HOME_CHANNEL_BEEP] = u8::from(enabled);
        Ok(())
    }

    /// Read the stored band-edge beep selection (CS-705 help 55028).
    ///
    /// User modes refer to separately stored user band edges. A zero beep level
    /// silences beeps; reading this value does not check or change that level.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for selection codes outside 0 through 3.
    pub fn band_edge_beep(&self) -> Result<BandEdgeBeep, Error> {
        match self.memory[BAND_EDGE_BEEP] {
            0 => Ok(BandEdgeBeep::Off),
            1 => Ok(BandEdgeBeep::DefaultEdges),
            2 => Ok(BandEdgeBeep::UserEdges),
            3 => Ok(BandEdgeBeep::UserEdgesWithTransmitLimit),
            _ => Err(Error::Unsupported {
                line: 5 + BAND_EDGE_BEEP / RECORD_BYTES,
                reason: "band-edge beep selection outside 0 through 3",
            }),
        }
    }

    /// Change only the stored band-edge beep byte, not user edges or radio state.
    ///
    /// This does not transmit or enforce a transmit limit.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported existing value without changing any state.
    pub fn set_band_edge_beep(&mut self, selection: BandEdgeBeep) -> Result<(), Error> {
        self.band_edge_beep()?;
        self.memory[BAND_EDGE_BEEP] = selection as u8;
        Ok(())
    }

    /// Read the stored inactivity power-off selection (CS-705 help 55030).
    ///
    /// The radio starts the timer on each power-on. This only reads its selection;
    /// it neither starts a timer nor performs a power operation.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for selection codes outside 0 through 4.
    pub fn auto_power_off(&self) -> Result<AutoPowerOff, Error> {
        match self.memory[AUTO_POWER_OFF] {
            0 => Ok(AutoPowerOff::Off),
            1 => Ok(AutoPowerOff::Minutes30),
            2 => Ok(AutoPowerOff::Minutes60),
            3 => Ok(AutoPowerOff::Minutes90),
            4 => Ok(AutoPowerOff::Minutes120),
            _ => Err(Error::Unsupported {
                line: 5 + AUTO_POWER_OFF / RECORD_BYTES,
                reason: "auto power-off selection outside 0 through 4",
            }),
        }
    }

    /// Change only the stored automatic power-off byte, without power I/O.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported existing value without changing any state.
    pub fn set_auto_power_off(&mut self, selection: AutoPowerOff) -> Result<(), Error> {
        self.auto_power_off()?;
        self.memory[AUTO_POWER_OFF] = selection as u8;
        Ok(())
    }

    /// Read the stored power-save selection (CS-705 help 55032).
    ///
    /// External/USB power, scanning, SPECTRUM SCOPE, RTTY DECODE, DV gateway
    /// operation or VOX ON can inhibit radio power saving. These conditions do
    /// not cause this stored value to be normalized to OFF.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for selection codes outside 0 through 3.
    pub fn power_save(&self) -> Result<PowerSave, Error> {
        match self.memory[POWER_SAVE] {
            0 => Ok(PowerSave::Off),
            1 => Ok(PowerSave::AutoShort),
            2 => Ok(PowerSave::AutoMedium),
            3 => Ok(PowerSave::AutoLong),
            _ => Err(Error::Unsupported {
                line: 5 + POWER_SAVE / RECORD_BYTES,
                reason: "power-save selection outside 0 through 3",
            }),
        }
    }

    /// Change only the stored power-save byte, without receive control or I/O.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported existing value without changing any state.
    pub fn set_power_save(&mut self, selection: PowerSave) -> Result<(), Error> {
        self.power_save()?;
        self.memory[POWER_SAVE] = selection as u8;
        Ok(())
    }
}
