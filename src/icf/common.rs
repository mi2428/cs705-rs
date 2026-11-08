use super::{Error, Icf, RECORD_BYTES};

// Numeric fields are stored settings only, based on static CS-705 1.30 evidence.
// They have not been compared with the original GUI. No I/O or parent/mode
// normalization occurs. Read/write RVAs accompany the literal offsets below.
macro_rules! raw_level {
    ($get:ident, $set:ident, $offset:literal) => {
        impl Icf<'_> {
            /// Read the stored raw level (0..=255), not percent or a device action.
            ///
            /// Static CS-705 evidence only; no original-GUI/hardware comparison.
            /// Parent/mode settings and every other byte remain unchanged.
            pub fn $get(&self) -> Result<u8, Error> {
                Ok(self.memory[$offset])
            }

            /// Change only this raw-level byte; preserve all other bytes. No I/O.
            pub fn $set(&mut self, value: u8) -> Result<(), Error> {
                self.memory[$offset] = value;
                Ok(())
            }
        }
    };
}

macro_rules! bounded_byte {
    ($get:ident, $set:ident, $offset:literal, $min:literal, $max:literal, $bias:literal, $unit:literal) => {
        impl Icf<'_> {
            #[doc = concat!("Read the stored ", $unit, ". No I/O or parent normalization.")]
            ///
            /// Static CS-705 evidence only; no original-GUI/hardware comparison.
            ///
            /// # Errors
            /// Returns [`Error::Unsupported`] for an unknown current value.
            pub fn $get(&self) -> Result<u8, Error> {
                let value = i16::from(self.memory[$offset]) + $bias;
                if !($min..=$max).contains(&value) {
                    return Err(Error::Unsupported {
                        line: 5 + $offset / RECORD_BYTES,
                        reason: concat!(stringify!($get), " current value outside supported range"),
                    });
                }
                Ok(value as u8)
            }

            #[doc = concat!("Change only the stored ", $unit, " byte. No I/O.")]
            ///
            /// # Errors
            /// Rejects an invalid new value with [`Error::Invalid`] before checking
            /// the current value. An unsupported current value is not repaired.
            /// All errors preserve every image, metadata and original-source byte.
            pub fn $set(&mut self, value: u8) -> Result<(), Error> {
                if !($min..=$max).contains(&value) {
                    return Err(Error::Invalid {
                        line: 5 + $offset / RECORD_BYTES,
                        reason: concat!(stringify!($get), " new value outside supported range"),
                    });
                }
                self.$get()?;
                self.memory[$offset] = (i16::from(value) - $bias) as u8;
                Ok(())
            }
        }
    };
}

macro_rules! bounded_word {
    ($get:ident, $set:ident, $ty:ty, $offset:literal, $min:expr, $max:expr, $unit:literal $(, $step:literal)?) => {
        impl Icf<'_> {
            #[doc = concat!("Read the stored ", $unit, ". No I/O or parent normalization.")]
            ///
            /// Static CS-705 evidence only; no original-GUI/hardware comparison.
            ///
            /// # Errors
            /// Returns [`Error::Unsupported`] for an out-of-range or off-step word.
            pub fn $get(&self) -> Result<$ty, Error> {
                let value = <$ty>::from_le_bytes([self.memory[$offset], self.memory[$offset + 1]]);
                if !($min..=$max).contains(&value) $(|| value % $step != 0)? {
                    return Err(Error::Unsupported {
                        line: 5 + $offset / RECORD_BYTES,
                        reason: concat!(stringify!($get), " current value outside supported range or step"),
                    });
                }
                Ok(value)
            }

            #[doc = concat!("Change only the two stored little-endian bytes for ", $unit, ". No I/O.")]
            ///
            /// # Errors
            /// Rejects an invalid new value with [`Error::Invalid`] before checking
            /// the current word. An unsupported current word is never repaired.
            /// Errors preserve all memory, metadata and original source bytes.
            pub fn $set(&mut self, value: $ty) -> Result<(), Error> {
                if !($min..=$max).contains(&value) $(|| value % $step != 0)? {
                    return Err(Error::Invalid {
                        line: 5 + $offset / RECORD_BYTES,
                        reason: concat!(stringify!($get), " new value outside supported range or step"),
                    });
                }
                self.$get()?;
                self.memory[$offset..$offset + 2].copy_from_slice(&value.to_le_bytes());
                Ok(())
            }
        }
    };
}

// Read/write RVAs 0x72755/0x728AB, 0x73363/0x734B3, 0x73698/0x737EE.
raw_level!(lcd_backlight_raw, set_lcd_backlight_raw, 0x3969d);
raw_level!(beep_level_raw, set_beep_level_raw, 0x392f2);
raw_level!(microphone_gain_raw, set_microphone_gain_raw, 0x39998);
// Read/write RVAs 0x75CAD/0x75E03, 0x75E35/0x75F8B, 0x7612E/0x7627E.
raw_level!(monitor_level_raw, set_monitor_level_raw, 0x3999c);
raw_level!(
    voice_transmit_level_raw,
    set_voice_transmit_level_raw,
    0x39964
);
raw_level!(
    recording_microphone_gain_raw,
    set_recording_microphone_gain_raw,
    0x37815
);
// Read/write RVAs 0x756D8/0x7582E, 0x75860/0x759B6, 0x73FFB/0x7414B.
raw_level!(vox_gain_raw, set_vox_gain_raw, 0x399ab);
raw_level!(anti_vox_gain_raw, set_anti_vox_gain_raw, 0x399ac);
raw_level!(speech_level_raw, set_speech_level_raw, 0x3931e);
// Read/write RVAs 0x7449E/0x745F4, 0x74686/0x747DA (width has its own GUI format).
raw_level!(
    noise_blanker_level_raw,
    set_noise_blanker_level_raw,
    0x399a1
);
raw_level!(
    noise_blanker_width_raw,
    set_noise_blanker_width_raw,
    0x399a3
);
// Read/write RVAs 0x75B66/0x75BAD, 0x76016/0x76077, 0x759E8/0x75AAF.
bounded_byte!(
    speech_compression_level,
    set_speech_compression_level,
    0x3999a,
    0,
    10,
    0,
    "speech compression level (0..=10)"
);
bounded_byte!(
    voice_repeat_seconds,
    set_voice_repeat_seconds,
    0x39963,
    1,
    15,
    0,
    "voice repeat time in seconds (1..=15)"
);
bounded_byte!(
    vox_delay_deciseconds,
    set_vox_delay_deciseconds,
    0x399ad,
    0,
    20,
    0,
    "VOX delay in 0.1-second units (0..=20)"
);
// Read/write RVAs 0x7462C/0x74673, 0x74861/0x748A8. Depth is a count, not dB.
bounded_byte!(
    noise_blanker_depth,
    set_noise_blanker_depth,
    0x399a2,
    1,
    10,
    1,
    "noise blanker depth displayed count (1..=10, raw+1)"
);
bounded_byte!(
    noise_reduction_level,
    set_noise_reduction_level,
    0x399a4,
    0,
    15,
    0,
    "noise reduction level (0..=15)"
);
// Read/write RVAs 0x73B67/0x73BD1, 0x73040/0x73076, 0x76488/0x765DB.
bounded_word!(
    split_offset_khz,
    set_split_offset_khz,
    i16,
    0x3930c,
    -9999,
    9999,
    "SPLIT offset in integer kHz (-9999..=9999)"
);
bounded_word!(
    utc_offset_minutes,
    set_utc_offset_minutes,
    i16,
    0x396ff,
    -840,
    840,
    "UTC offset in integer minutes (-840..=840, step 5; no empty sentinel)",
    5
);
bounded_word!(
    reference_frequency_adjustment,
    set_reference_frequency_adjustment,
    u16,
    0x39330,
    0,
    511,
    "reference frequency raw adjustment count (0..=511), not Hz or ppm"
);

impl Icf<'_> {
    /// Read the stored manual notch position (-127..=127), raw offset-binary minus 128.
    ///
    /// Static evidence: read RVA 0x74967, write 0x74A98 -> 0xA490F.
    /// No GUI comparison, I/O or operating-mode normalization is implied.
    ///
    /// # Errors
    /// Returns [`Error::Unsupported`] for raw zero, not a signed i8 interpretation.
    pub fn manual_notch_position(&self) -> Result<i8, Error> {
        let raw = self.memory[0x399a6];
        if raw == 0 {
            return Err(Error::Unsupported {
                line: 5 + 0x399a6 / RECORD_BYTES,
                reason: "manual_notch_position current raw zero",
            });
        }
        Ok((i16::from(raw) - 128) as i8)
    }

    /// Change only the stored notch position byte, using position+128. No I/O.
    ///
    /// # Errors
    /// Returns [`Error::Invalid`] for -128 before checking the current raw byte.
    /// An unsupported current value is never repaired; errors change no bytes.
    pub fn set_manual_notch_position(&mut self, value: i8) -> Result<(), Error> {
        if value == i8::MIN {
            return Err(Error::Invalid {
                line: 5 + 0x399a6 / RECORD_BYTES,
                reason: "manual_notch_position new value outside -127..=127",
            });
        }
        self.manual_notch_position()?;
        self.memory[0x399a6] = (i16::from(value) + 128) as u8;
        Ok(())
    }
}

// Full-byte stored selectors traced from CS-705 1.30. Each invocation records
// its read/write RVAs below. No GUI, audio, charging, power or device I/O occurs.
macro_rules! stored_setting {
    ($get:ident, $set:ident, $ty:ty, $offset:literal, $label:literal,
     {$($raw:literal => $value:expr),+ $(,)?}) => {
        impl Icf<'_> {
            #[doc = concat!("Read the stored ", $label, ".")]
            ///
            /// Static evidence only; not compared with the original GUI.
            /// Parent settings are neither checked nor normalized. No I/O occurs.
            ///
            /// # Errors
            /// Returns [`Error::Unsupported`] for an unknown stored code.
            pub fn $get(&self) -> Result<$ty, Error> {
                match self.memory[$offset] {
                    $($raw => Ok($value),)+
                    _ => Err(Error::Unsupported {
                        line: 5 + $offset / RECORD_BYTES,
                        reason: concat!(stringify!($get), " value outside supported codes"),
                    }),
                }
            }

            #[doc = concat!("Change only the stored ", $label, " byte, not device operation.")]
            ///
            /// All other memory, metadata and original source bytes are preserved.
            /// Use [`Self::to_bytes`] to serialize. No I/O occurs.
            ///
            /// # Errors
            /// Refuses an unknown current code without changing any bytes.
            pub fn $set(&mut self, value: $ty) -> Result<(), Error> {
                self.$get()?;
                self.memory[$offset] = value as u8;
                Ok(())
            }
        }
    };
}

macro_rules! stored_enum {
    ($ty:ident, $get:ident, $set:ident, $offset:literal, $label:literal,
     {$($variant:ident = $raw:literal),+ $(,)?}) => {
        #[doc = concat!("Stored ", $label, " selection codes, not commands.")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum $ty {
            $(#[doc = concat!("Select ", stringify!($variant), ".")]
              $variant = $raw,)+
        }
        stored_setting!($get, $set, $ty, $offset, $label, {$($raw => $ty::$variant),+});
    };
}

// Read/write RVAs 0x74FCA/0x75006, 0x75019/0x75055.
stored_setting!(recording_file_split, set_recording_file_split, bool, 0x39968, "recording file split", {0 => false, 1 => true});
stored_setting!(ptt_auto_record, set_ptt_auto_record, bool, 0x39969, "PTT auto record", {0 => false, 1 => true});
// Read/write RVAs 0x728FE/0x7293A, 0x72D07/0x72D43, 0x72D56/0x72D92.
stored_setting!(lcd_backlight_auto_adjust, set_lcd_backlight_auto_adjust, bool, 0x3969e, "LCD backlight auto adjustment", {0 => false, 1 => true});
stored_setting!(power_key_screen_off, set_power_key_screen_off, bool, 0x396b0, "power-key screen-off flag", {0 => false, 1 => true});
stored_setting!(receive_led, set_receive_led, bool, 0x396b1, "receive LED flag", {0 => false, 1 => true});
// Read/write RVAs 0x729F2/0x72A2E, 0x72A8F/0x72ACB, 0x72ADE/0x72B1A.
stored_setting!(meter_peak_hold, set_meter_peak_hold, bool, 0x396a1, "meter peak hold", {0 => false, 1 => true});
stored_setting!(memory_name_display, set_memory_name_display, bool, 0x396a2, "memory name display", {0 => false, 1 => true});
stored_setting!(memory_group_name_popup, set_memory_group_name_popup, bool, 0x396a3, "memory group name popup", {0 => false, 1 => true});
// Read/write RVAs 0x72DA5/0x72DE1, 0x72DF4/0x72E30.
stored_setting!(opening_message, set_opening_message, bool, 0x396b2, "opening message", {0 => false, 1 => true});
stored_setting!(power_on_check, set_power_on_check, bool, 0x396b3, "power-on check", {0 => false, 1 => true});
// Read/write RVAs 0x74BDA/0x74C16, 0x74C29/0x74C65, 0x74C78/0x74CB4.
stored_setting!(battery_pack_selection_display, set_battery_pack_selection_display, bool, 0x39a02, "battery pack selection display", {0 => false, 1 => true});
stored_setting!(charging_when_power_on, set_charging_when_power_on, bool, 0x39a03, "charging-when-powered-on flag", {0 => false, 1 => true});
stored_setting!(usb_power_input, set_usb_power_input, bool, 0x39a04, "USB power input flag", {0 => false, 1 => true});
// Read/write RVAs 0x74E08/0x74E41.
stored_setting!(screen_capture_power_key, set_screen_capture_power_key, bool, 0x3932c, "power-key screen capture flag", {0 => false, 1 => true});

// Read/write RVAs 0x74F2C/0x74F68, 0x74F7B/0x74FB7.
stored_enum!(TxRecordingAudio, transmit_recording_audio, set_transmit_recording_audio, 0x39966, "transmit recording audio", {Direct = 0, Monitor = 1});
stored_enum!(RecordingCondition, receive_recording_condition, set_receive_recording_condition, 0x39967, "receive recording condition", {Always = 0, SquelchLinked = 1});
// Read/write RVAs 0x75068/0x750A4, 0x750B7/0x750F3. 0x3996B is untouched.
stored_enum!(PreRecordingTime, pre_recording_time, set_pre_recording_time, 0x3996a, "pre-recording time", {Off = 0, Seconds5 = 1, Seconds10 = 2, Seconds15 = 3});
stored_enum!(PlaybackSkipTime, playback_skip_time, set_playback_skip_time, 0x3996c, "playback skip time", {Seconds3 = 0, Seconds5 = 1, Seconds10 = 2, Seconds30 = 3});
// Read/write RVAs 0x72C1A/0x72C56.
stored_enum!(ScrollSpeed, scroll_speed, set_scroll_speed, 0x396ad, "scroll speed", {Slow = 0, Fast = 1});
// Read/write RVAs 0x7417A/0x741B3, 0x741C3/0x741FC, 0x7420C/0x74245.
stored_enum!(SpeechLockKey, speech_lock_key, set_speech_lock_key, 0x3931f, "speech/lock key order", {SpeechLock = 0, LockSpeech = 1});
stored_enum!(LockFunction, lock_function, set_lock_function, 0x39320, "lock function", {MainDial = 0, Panel = 1});
stored_enum!(MemoryPadChannels, memory_pad_channels, set_memory_pad_channels, 0x39321, "memory pad channel count", {Five = 0, Ten = 1});
// Read/write RVAs 0x74255/0x7428E, 0x74368/0x743A1.
stored_enum!(DialAutoTs, dial_auto_ts, set_dial_auto_ts, 0x39322, "dial auto TS", {Off = 0, Low = 1, High = 2});
stored_enum!(MicUpDownSpeed, mic_up_down_speed, set_mic_up_down_speed, 0x39323, "microphone up/down speed", {Slow = 0, Fast = 1});
// Read/write RVAs 0x743B1/0x743ED, 0x74400/0x7443C: direct image offsets.
stored_enum!(MemoryChannelSelectionScope, memory_channel_selection_scope, set_memory_channel_selection_scope, 0x37788, "memory channel selection scope", {CurrentGroup = 0, AllGroups = 1});
stored_enum!(MemoryGroupDisplay, memory_group_display, set_memory_group_display, 0x37789, "memory group display", {RegisteredOnly = 0, AllGroups = 1});
// Read/write RVAs 0x74E9A/0x74ED3, 0x74EE3/0x74F1C, 0x74E51/0x74E8A.
stored_enum!(KeyboardInput, keyboard_input, set_keyboard_input, 0x3932e, "keyboard input", {TenKey = 0, Full = 1});
stored_enum!(KeyboardLayout, keyboard_layout, set_keyboard_layout, 0x3932f, "keyboard layout", {JapanUsUk = 0, Germany = 1, France = 2});
stored_enum!(ScreenCaptureFileType, screen_capture_file_type, set_screen_capture_file_type, 0x3932d, "screen capture file type", {Png = 0, Bmp = 1});

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

// Additional split/speech mappings from the same fixed CS-705 1.30 binary.
// All are full, unmasked u8 values in block 0x392C0. Native edit comparisons
// remain pending. Boolean choices use OnOff cells at RVA 0x3177F2.
// Key 0x23F: read RVA 0x73B1A, write 0x73B53.
const QUICK_SPLIT: usize = 0x3930b;
// Key 0x241: read RVA 0x73BE2, write 0x73C1B.
const SPLIT_LOCK: usize = 0x3930e;
// Key 0x246: read RVA 0x73DB3, write 0x73DEC; choice cells at 0x33EF9E.
const RECEIVED_CALLSIGN_SPEECH: usize = 0x39319;
// Key 0x247: read RVA 0x73DFC, write 0x73E35.
const RX_CS_SPEECH: usize = 0x3931a;
// Key 0x248: read RVA 0x73E45, write 0x73E7E; MIC Up/Down, not dial rotation.
const MIC_UP_DOWN_SPEECH: usize = 0x3931c;
// Key 0x249: read RVA 0x73E8E, write 0x73EC7.
const S_METER_LEVEL_SPEECH: usize = 0x3931b;
// Key 0x24A: read RVA 0x73ED7, write 0x73F10.
const MODE_SPEECH: usize = 0x3931d;
// Key 0x24B: read RVA 0x73F20, write 0x73F59; choice cells at 0x33F07E.
const SPEECH_LANGUAGE: usize = 0x39316;
// Key 0x24C: read RVA 0x73F69, write 0x73FA2; choice cells at 0x33F13E.
const SPEECH_ALPHABET: usize = 0x39317;
// Key 0x24D: read RVA 0x73FB2, write 0x73FEB; choice cells at 0x33F206.
const SPEECH_SPEED: usize = 0x39318;

/// Stored received-callsign announcement selection (CS-705 help 55400).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ReceivedCallsignSpeech {
    /// Do not announce received callsigns.
    Off = 0,
    /// Announce callsigns only for short DV signals while standing by.
    Kerchunk = 1,
    /// Announce callsigns for all applicable DV signals while standing by.
    All = 2,
}

/// Stored announcement language, not the UI language (CS-705 help 55394).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SpeechLanguage {
    /// Announce in English.
    English = 0,
    /// Announce in Japanese.
    Japanese = 1,
}

/// Stored pronunciation of callsign letters A through Z (CS-705 help 55396).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SpeechAlphabet {
    /// Use ordinary letter names.
    Normal = 0,
    /// Use phonetic code words.
    Phonetic = 1,
}

/// Stored announcement speed (CS-705 help 55398).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SpeechSpeed {
    /// Use the slow announcement speed.
    Slow = 0,
    /// Use the fast announcement speed.
    Fast = 1,
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

    /// Read the stored quick-split flag (CS-705 help 55312).
    ///
    /// Radio operation also depends on the separate split offset. This value
    /// does not activate split or set either VFO's frequency or mode.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for values other than OFF (0) or ON (1).
    pub fn quick_split(&self) -> Result<bool, Error> {
        match self.memory[QUICK_SPLIT] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Unsupported {
                line: 5 + QUICK_SPLIT / RECORD_BYTES,
                reason: "quick-split value other than OFF or ON",
            }),
        }
    }

    /// Change only the stored quick-split byte, not VFOs or the split offset.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported current value without changing any state.
    pub fn set_quick_split(&mut self, enabled: bool) -> Result<(), Error> {
        self.quick_split()?;
        self.memory[QUICK_SPLIT] = u8::from(enabled);
        Ok(())
    }

    /// Read the stored split-lock flag (CS-705 help 55316).
    ///
    /// This selects the radio's XFC behavior during dial lock: adjusting the
    /// transmit frequency while keeping the receive frequency fixed.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for values other than OFF (0) or ON (1).
    pub fn split_lock(&self) -> Result<bool, Error> {
        match self.memory[SPLIT_LOCK] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Unsupported {
                line: 5 + SPLIT_LOCK / RECORD_BYTES,
                reason: "split-lock value other than OFF or ON",
            }),
        }
    }

    /// Change only the stored split-lock byte, without button or frequency I/O.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported current value without changing any state.
    pub fn set_split_lock(&mut self, enabled: bool) -> Result<(), Error> {
        self.split_lock()?;
        self.memory[SPLIT_LOCK] = u8::from(enabled);
        Ok(())
    }

    /// Read the stored received-callsign speech selection (CS-705 help 55400).
    ///
    /// DV standby, squelch matching and received-signal conditions affect radio
    /// announcements, not this stored value. They do not normalize it to OFF.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for selection codes outside 0 through 2.
    pub fn received_callsign_speech(&self) -> Result<ReceivedCallsignSpeech, Error> {
        match self.memory[RECEIVED_CALLSIGN_SPEECH] {
            0 => Ok(ReceivedCallsignSpeech::Off),
            1 => Ok(ReceivedCallsignSpeech::Kerchunk),
            2 => Ok(ReceivedCallsignSpeech::All),
            _ => Err(Error::Unsupported {
                line: 5 + RECEIVED_CALLSIGN_SPEECH / RECORD_BYTES,
                reason: "received callsign speech selection outside 0 through 2",
            }),
        }
    }

    /// Change only the stored received-callsign speech byte, not callsign data.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported current value without changing any state.
    pub fn set_received_callsign_speech(
        &mut self,
        selection: ReceivedCallsignSpeech,
    ) -> Result<(), Error> {
        self.received_callsign_speech()?;
        self.memory[RECEIVED_CALLSIGN_SPEECH] = selection as u8;
        Ok(())
    }

    /// Read the stored RX-to-CS speech flag (CS-705 help 55402).
    ///
    /// This selects announcement of the callsign chosen by a long press of
    /// AUTOTUNE/RX-to-CS. It does not perform that press or set a reply target.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for values other than OFF (0) or ON (1).
    pub fn rx_cs_speech(&self) -> Result<bool, Error> {
        match self.memory[RX_CS_SPEECH] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Unsupported {
                line: 5 + RX_CS_SPEECH / RECORD_BYTES,
                reason: "RX-to-CS speech value other than OFF or ON",
            }),
        }
    }

    /// Change only the stored RX-to-CS speech byte, without audio or recording I/O.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported current value without changing any state.
    pub fn set_rx_cs_speech(&mut self, enabled: bool) -> Result<(), Error> {
        self.rx_cs_speech()?;
        self.memory[RX_CS_SPEECH] = u8::from(enabled);
        Ok(())
    }

    /// Read the stored MIC Up/Down speech flag (CS-705 help 55412).
    ///
    /// This is not a general dial-rotation setting. The radio announces frequency
    /// in VFO/memory mode or the repeater callsign in DR mode after MIC Up/Down.
    /// Reception can cancel an announcement without changing this stored flag.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for values other than OFF (0) or ON (1).
    pub fn mic_up_down_speech(&self) -> Result<bool, Error> {
        match self.memory[MIC_UP_DOWN_SPEECH] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Unsupported {
                line: 5 + MIC_UP_DOWN_SPEECH / RECORD_BYTES,
                reason: "MIC Up/Down speech value other than OFF or ON",
            }),
        }
    }

    /// Change only the stored MIC Up/Down speech byte, not channel or VFO data.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported current value without changing any state.
    pub fn set_mic_up_down_speech(&mut self, enabled: bool) -> Result<(), Error> {
        self.mic_up_down_speech()?;
        self.memory[MIC_UP_DOWN_SPEECH] = u8::from(enabled);
        Ok(())
    }

    /// Read whether speech includes the S-meter level (CS-705 help 55405).
    ///
    /// OFF still leaves mode/frequency announcements on SPEECH/LOCK; this is not
    /// a master speech mute. Reading it does not measure a signal or play audio.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for values other than OFF (0) or ON (1).
    pub fn s_meter_level_speech(&self) -> Result<bool, Error> {
        match self.memory[S_METER_LEVEL_SPEECH] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Unsupported {
                line: 5 + S_METER_LEVEL_SPEECH / RECORD_BYTES,
                reason: "S-meter level speech value other than OFF or ON",
            }),
        }
    }

    /// Change only the stored S-meter level announcement byte, not a master mute.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported current value without changing any state.
    pub fn set_s_meter_level_speech(&mut self, enabled: bool) -> Result<(), Error> {
        self.s_meter_level_speech()?;
        self.memory[S_METER_LEVEL_SPEECH] = u8::from(enabled);
        Ok(())
    }

    /// Read the stored mode-change announcement flag (CS-705 help 55407).
    ///
    /// This controls announcement after a radio mode change; it does not change
    /// the operating mode or produce an announcement itself.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for values other than OFF (0) or ON (1).
    pub fn mode_speech(&self) -> Result<bool, Error> {
        match self.memory[MODE_SPEECH] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Unsupported {
                line: 5 + MODE_SPEECH / RECORD_BYTES,
                reason: "mode speech value other than OFF or ON",
            }),
        }
    }

    /// Change only the stored mode-change speech byte, not the operating mode.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported current value without changing any state.
    pub fn set_mode_speech(&mut self, enabled: bool) -> Result<(), Error> {
        self.mode_speech()?;
        self.memory[MODE_SPEECH] = u8::from(enabled);
        Ok(())
    }

    /// Read the stored speech language, not UI language (CS-705 help 55394).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for selection codes outside 0 through 1.
    pub fn speech_language(&self) -> Result<SpeechLanguage, Error> {
        match self.memory[SPEECH_LANGUAGE] {
            0 => Ok(SpeechLanguage::English),
            1 => Ok(SpeechLanguage::Japanese),
            _ => Err(Error::Unsupported {
                line: 5 + SPEECH_LANGUAGE / RECORD_BYTES,
                reason: "speech language selection outside 0 through 1",
            }),
        }
    }

    /// Change only the stored speech-language byte, without audio I/O.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported current value without changing any state.
    pub fn set_speech_language(&mut self, selection: SpeechLanguage) -> Result<(), Error> {
        self.speech_language()?;
        self.memory[SPEECH_LANGUAGE] = selection as u8;
        Ok(())
    }

    /// Read the stored callsign-letter pronunciation selection (CS-705 help 55396).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for selection codes outside 0 through 1.
    pub fn speech_alphabet(&self) -> Result<SpeechAlphabet, Error> {
        match self.memory[SPEECH_ALPHABET] {
            0 => Ok(SpeechAlphabet::Normal),
            1 => Ok(SpeechAlphabet::Phonetic),
            _ => Err(Error::Unsupported {
                line: 5 + SPEECH_ALPHABET / RECORD_BYTES,
                reason: "speech alphabet selection outside 0 through 1",
            }),
        }
    }

    /// Change only the stored pronunciation byte, not callsign contents.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported current value without changing any state.
    pub fn set_speech_alphabet(&mut self, selection: SpeechAlphabet) -> Result<(), Error> {
        self.speech_alphabet()?;
        self.memory[SPEECH_ALPHABET] = selection as u8;
        Ok(())
    }

    /// Read the stored speech-speed selection (CS-705 help 55398).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for selection codes outside 0 through 1.
    pub fn speech_speed(&self) -> Result<SpeechSpeed, Error> {
        match self.memory[SPEECH_SPEED] {
            0 => Ok(SpeechSpeed::Slow),
            1 => Ok(SpeechSpeed::Fast),
            _ => Err(Error::Unsupported {
                line: 5 + SPEECH_SPEED / RECORD_BYTES,
                reason: "speech speed selection outside 0 through 1",
            }),
        }
    }

    /// Change only the stored speech-speed byte, without timers or audio I/O.
    ///
    /// # Errors
    ///
    /// Refuses an unsupported current value without changing any state.
    pub fn set_speech_speed(&mut self, selection: SpeechSpeed) -> Result<(), Error> {
        self.speech_speed()?;
        self.memory[SPEECH_SPEED] = selection as u8;
        Ok(())
    }
}
