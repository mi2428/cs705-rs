use super::{Error, Icf, RECORD_BYTES};

impl Icf<'_> {
    /// Read canonical final Home Hz; zero is empty, not a GUI numeric input.
    ///
    /// Static CS-705 evidence: Home store RVA 0x7DC5F, receive ranges 0x1918D0.
    /// This does not reproduce GUI parsing, clamping, rounding or band selection.
    ///
    /// # Errors
    /// Returns [`Error::Unsupported`] for unknown stored frequencies.
    pub fn home_frequency_hz(&self) -> Result<Option<u32>, Error> {
        let value = u32::from_le_bytes(
            self.memory[0x392f6..0x392fa]
                .try_into()
                .expect("four-byte Home frequency"),
        );
        match value {
            0 => Ok(None),
            30_000..=199_999_999 | 400_000_000..=470_000_000 => Ok(Some(value)),
            _ => Err(Error::Unsupported {
                line: 5 + 0x392f6 / RECORD_BYTES,
                reason: "Home frequency outside canonical ranges",
            }),
        }
    }

    /// Replace only the four stored Home Hz bytes; `None` clears them to zero.
    ///
    /// No GUI correction, parent normalization or I/O occurs.
    ///
    /// # Errors
    /// Invalid new values, including `Some(0)`, are rejected before checking the
    /// current value. Unknown current frequencies are never repaired implicitly.
    pub fn set_home_frequency_hz(&mut self, value: Option<u32>) -> Result<(), Error> {
        let raw = match value {
            None => 0,
            Some(hz) if matches!(hz, 30_000..=199_999_999 | 400_000_000..=470_000_000) => hz,
            _ => {
                return Err(Error::Invalid {
                    line: 5 + 0x392f6 / RECORD_BYTES,
                    reason: "new Home frequency outside canonical ranges",
                });
            }
        };
        self.home_frequency_hz()?;
        self.memory[0x392f6..0x392fa].copy_from_slice(&raw.to_le_bytes());
        Ok(())
    }

    /// Return all 64 stored NTP server bytes without decoding or trimming.
    pub fn ntp_server_raw(&self) -> &[u8] {
        &self.memory[0x396be..0x396fe]
    }

    /// Store 0..=64 ASCII letters, digits, hyphens or dots, space-padded to 64 bytes.
    ///
    /// Static canonical-byte subset: RVA 0x72FBE -> 0x47FD0. This is not GUI
    /// normalization or DNS/IP validation. Unknown current bytes may be explicitly
    /// replaced. No other bytes change and no network or device I/O occurs.
    ///
    /// # Errors
    /// Returns [`Error::Invalid`] for invalid length/alphabet before any copy.
    pub fn set_ntp_server_ascii(&mut self, value: &str) -> Result<(), Error> {
        let bytes = value.as_bytes();
        if bytes.len() > 64
            || !bytes
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.'))
        {
            return Err(Error::Invalid {
                line: 5 + 0x396be / RECORD_BYTES,
                reason: "NTP server requires 0..=64 ASCII letters, digits, hyphens or dots",
            });
        }
        let mut field = [b' '; 64];
        field[..bytes.len()].copy_from_slice(bytes);
        self.memory[0x396be..0x396fe].copy_from_slice(&field);
        Ok(())
    }

    /// Return the exact 16-byte payload clone comment, not the raw header comment.
    pub fn clone_comment_raw(&self) -> &[u8] {
        &self.memory[0x35591..0x355a1]
    }

    /// Store canonical printable ASCII, space-padded to the 16-byte payload field.
    ///
    /// Static byte-entry subset: RVA 0x76673 -> 0x47FD0. Empty text is allowed;
    /// nonempty text has no leading/trailing spaces. Interior spaces and ASCII
    /// punctuation are preserved. Unlike GUI normalization, this rejects rather
    /// than trims/clips/corrects input. Unknown current bytes may be replaced.
    /// Header [`Self::comment_bytes`] is never synchronized; no I/O occurs.
    ///
    /// # Errors
    /// Returns [`Error::Invalid`] for invalid length/alphabet or edge spaces,
    /// before any image, metadata or original-source bytes change.
    pub fn set_clone_comment_canonical_ascii(&mut self, value: &str) -> Result<(), Error> {
        let bytes = value.as_bytes();
        if bytes.len() > 16
            || !bytes.iter().all(|b| (0x20..=0x7e).contains(b))
            || bytes.first() == Some(&b' ')
            || bytes.last() == Some(&b' ')
        {
            return Err(Error::Invalid {
                line: 5 + 0x35591 / RECORD_BYTES,
                reason: "clone comment requires 0..=16 printable ASCII bytes without edge spaces",
            });
        }
        let mut field = [b' '; 16];
        field[..bytes.len()].copy_from_slice(bytes);
        self.memory[0x35591..0x355a1].copy_from_slice(&field);
        Ok(())
    }
}

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

// Stored selectors traced from CS-705 1.30. Full-byte access is the default;
// the four explicit masks preserve all other bits. No device I/O occurs.
macro_rules! stored_setting {
    ($get:ident, $set:ident, $ty:ty, $offset:literal, $label:literal $(, mask $mask:literal, shift $shift:literal)?,
     {$($raw:literal => $value:expr),+ $(,)?}) => {
        $(const _: () = {
            assert!($shift < 8);
            assert!($mask != 0 && (($mask as u16 >> $shift) << $shift) == $mask as u16);
            assert!(($mask as u16 >> $shift).trailing_ones() == ($mask as u16).count_ones());
        };)?
        impl Icf<'_> {
            #[doc = concat!("Read the stored ", $label, ".")]
            ///
            /// Static evidence only; not compared with the original GUI.
            /// Parent settings are neither checked nor normalized. No I/O occurs.
            ///
            /// # Errors
            /// Returns [`Error::Unsupported`] for an unknown stored code.
            pub fn $get(&self) -> Result<$ty, Error> {
                let raw = self.memory[$offset];
                $(let raw = (raw & $mask) >> $shift;)?
                match raw {
                    $($raw => Ok($value),)+
                    _ => Err(Error::Unsupported {
                        line: 5 + $offset / RECORD_BYTES,
                        reason: concat!(stringify!($get), " value outside supported codes"),
                    }),
                }
            }

            #[doc = concat!("Change only the stored ", $label, " byte, not device operation.")]
            ///
            /// All non-target bits, memory, metadata and original source bytes are preserved.
            /// Use [`Self::to_bytes`] to serialize. No I/O occurs.
            ///
            /// # Errors
            /// Refuses an unknown current code without changing any bytes.
            pub fn $set(&mut self, value: $ty) -> Result<(), Error> {
                self.$get()?;
                let raw = value as u8;
                $(let raw = (self.memory[$offset] & !$mask) | ((raw << $shift) & $mask);)?
                self.memory[$offset] = raw;
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

// Additional confirmed stored selectors (RC common IDs below). Static evidence
// only: no original-GUI comparison, CSV processing or device operation.
// Key 0x284: read RVA 0x762b0, store RVA 0x762ec (common ID A2-H54848-07).
stored_setting!(qso_log, set_qso_log, bool, 0x399dd, "QSO log", {0 => false, 1 => true});
// Key 0x285: read RVA 0x762ff, store RVA 0x7633b (common ID A2-H54848-08).
stored_setting!(receive_history_log, set_receive_history_log, bool, 0x399de, "receive history log", {0 => false, 1 => true});
// Key 0x2b1: read RVA 0x766a3, store RVA 0x766df (common ID A2-H54848-14).
stored_setting!(bluetooth, set_bluetooth, bool, 0x39701, "Bluetooth", {0 => false, 1 => true});
// Key 0x2b2: read RVA 0x766f2, store RVA 0x7672e (common ID A2-H54848-15).
stored_setting!(bluetooth_auto_connect, set_bluetooth_auto_connect, bool, 0x39702, "Bluetooth auto connect", {0 => false, 1 => true});
// Key 0x2b6: read RVA 0x7682e, store RVA 0x7686a (common ID A2-H54848-19).
stored_setting!(bluetooth_power_save, set_bluetooth_power_save, bool, 0x39705, "Bluetooth power save", {0 => false, 1 => true});
// Key 0x2b7: read RVA 0x7687d, store RVA 0x768b9 (common ID A2-H54848-20).
stored_setting!(bluetooth_one_touch_ptt, set_bluetooth_one_touch_ptt, bool, 0x39706, "Bluetooth one-touch PTT", {0 => false, 1 => true});
// Key 0x2b8: read RVA 0x768cc, store RVA 0x76908 (common ID A2-H54848-21).
stored_setting!(bluetooth_ptt_beep, set_bluetooth_ptt_beep, bool, 0x39707, "Bluetooth PTT beep", {0 => false, 1 => true});
// Key 0x2b9: read RVA 0x7691b, store RVA 0x76957 (common ID A2-H54848-22).
stored_setting!(bluetooth_custom_key_beep, set_bluetooth_custom_key_beep, bool, 0x39708, "Bluetooth custom-key beep", {0 => false, 1 => true});
// Key 0x260: read RVA 0x75b11, store RVA 0x75b4d (common ID A2-H54848-55).
stored_setting!(speech_compressor, set_speech_compressor, bool, 0x39999, "speech compressor", {0 => false, 1 => true});
// Key 0x264: read RVA 0x75c5e, store RVA 0x75c9a (common ID A2-H54848-59).
stored_setting!(monitor, set_monitor, bool, 0x3999b, "monitor", {0 => false, 1 => true});
// Key 0x279: read RVA 0x7608a, store RVA 0x760c6 (common ID A2-H54848-62).
stored_setting!(voice_auto_monitor, set_voice_auto_monitor, bool, 0x39962, "voice auto monitor", {0 => false, 1 => true});
// Key 0x25b: read RVA 0x75659, store RVA 0x756c3 (common ID A2-H54848-66).
stored_setting!(vox, set_vox, bool, 0x37781, "VOX", mask 0x80, shift 7, {0 => false, 1 => true});
// Key 0x291: read RVA 0x73ad1, store RVA 0x73b0a (common ID A2-H54848-76).
stored_setting!(ptt_lock, set_ptt_lock, bool, 0x3930a, "PTT lock", {0 => false, 1 => true});
// Key 0x243: read RVA 0x73c74, store RVA 0x73cad (common ID A2-H54848-81).
stored_setting!(tuner_memory, set_tuner_memory, bool, 0x39310, "tuner memory", {0 => false, 1 => true});
// Key 0x244: read RVA 0x73cbd, store RVA 0x73cf6 (common ID A2-H54848-82).
stored_setting!(ptt_tune, set_ptt_tune, bool, 0x39311, "PTT tune", {0 => false, 1 => true});
// Key 0x253: read RVA 0x7429c, store RVA 0x74304 (common ID A2-H54848-97).
stored_setting!(one_hz_mode, set_one_hz_mode, bool, 0x37780, "1 Hz mode", mask 0x02, shift 1, {0 => false, 1 => true});
// Key 0x255: read RVA 0x74319, store RVA 0x74355 (common ID A2-H54848-98).
stored_setting!(quarter_tuning, set_quarter_tuning, bool, 0x37784, "quarter tuning", {0 => false, 1 => true});
// Key 0x292: read RVA 0x7444f, store RVA 0x7448b (common ID A2-H54848-102).
stored_setting!(noise_blanker, set_noise_blanker, bool, 0x37782, "noise blanker", {0 => false, 1 => true});
// Key 0x296: read RVA 0x7480c, store RVA 0x74848 (common ID A2-H54848-106).
stored_setting!(noise_reduction, set_noise_reduction, bool, 0x37783, "noise reduction", {0 => false, 1 => true});
// Key 0x29d: read RVA 0x74b48, store RVA 0x74b81 (common ID A2-H54848-116).
stored_setting!(ssb_cw_frequency_shift, set_ssb_cw_frequency_shift, bool, 0x39326, "SSB/CW frequency shift", {0 => false, 1 => true});
// Key 0x2a0: read RVA 0x75155, store RVA 0x75191 (common ID A2-H54848-119).
stored_setting!(scan_resume, set_scan_resume, bool, 0x3998a, "scan resume", {0 => false, 1 => true});
// Key 0x226: read RVA 0x72f07, store RVA 0x72f43 (common ID A2-H54848-141).
stored_setting!(ntp, set_ntp, bool, 0x396bd, "NTP", {0 => false, 1 => true});
// Key 0x228: read RVA 0x72fee, store RVA 0x7302a (common ID A2-H54848-143).
stored_setting!(gps_time_auto_correction, set_gps_time_auto_correction, bool, 0x396fe, "GPS time auto correction", {0 => false, 1 => true});

// Key 0x286: read RVA 0x7634e, store RVA 0x7638a (common ID A2-H54848-09).
stored_enum!(CsvSeparatorDecimal, log_csv_separator_decimal, set_log_csv_separator_decimal, 0x399df, "log CSV separator/decimal", {CommaDot = 0, SemicolonDot = 1, SemicolonComma = 2});
// Key 0x287: read RVA 0x7639d, store RVA 0x763d9 (common ID A2-H54848-10).
stored_enum!(CsvDateFormat, log_csv_date_format, set_log_csv_date_format, 0x399e0, "log CSV date format", {YearMonthDay = 0, MonthDayYear = 1, DayMonthYear = 2});
// Key 0x2b3: read RVA 0x76741, store RVA 0x7677d (common ID A2-H54848-16).
stored_enum!(BluetoothAudioOutput, bluetooth_audio_output, set_bluetooth_audio_output, 0x39703, "Bluetooth audio output", {HeadsetOnly = 0, HeadsetAndSpeaker = 1});
// Key 0x2b4: read RVA 0x76790, store RVA 0x767cc (common ID A2-H54848-17).
stored_enum!(BluetoothAutoDisconnect, bluetooth_auto_disconnect, set_bluetooth_auto_disconnect, 0x39721, "Bluetooth auto disconnect", {Off = 0, Seconds0 = 1, Seconds1 = 2, Seconds2 = 3, Seconds3 = 4, Seconds4 = 5, Seconds5 = 6, Seconds6 = 7, Seconds7 = 8, Seconds8 = 9, Seconds9 = 10, Seconds10 = 11});
// Key 0x2b5: read RVA 0x767df, store RVA 0x7681b (common ID A2-H54848-18).
stored_enum!(BluetoothHeadsetFunction, bluetooth_headset_function, set_bluetooth_headset_function, 0x39704, "Bluetooth headset function", {Normal = 0, Microphone = 1, Ptt = 2});
// Key 0x2bb: read RVA 0x7696a, store RVA 0x769a6 (common ID A2-H54848-24).
stored_enum!(BluetoothSerialFunction, bluetooth_serial_function, set_bluetooth_serial_function, 0x39709, "Bluetooth serial function", {CivNoEcho = 0, CivEcho = 1, DvData = 2});
// Key 0x21e: read RVA 0x72c69, store RVA 0x72ca5 (common ID A2-H54848-27).
stored_enum!(ScreenSaverDelay, battery_screen_saver, set_battery_screen_saver, 0x396ae, "battery screen saver", {Off = 0, Minutes1 = 1, Minutes2 = 2, Minutes5 = 3, Minutes15 = 4, Minutes30 = 5, Minutes60 = 6});
// Key 0x21f: read RVA 0x72cb8, store RVA 0x72cf4 (common ID A2-H54848-28).
stored_setting!(external_power_screen_saver, set_external_power_screen_saver, ScreenSaverDelay, 0x396af, "external-power screen saver", {0 => ScreenSaverDelay::Off, 1 => ScreenSaverDelay::Minutes1, 2 => ScreenSaverDelay::Minutes2, 3 => ScreenSaverDelay::Minutes5, 4 => ScreenSaverDelay::Minutes15, 5 => ScreenSaverDelay::Minutes30, 6 => ScreenSaverDelay::Minutes60});
// Key 0x216: read RVA 0x72a40, store RVA 0x72a7c (common ID A2-H54848-32).
stored_enum!(TransmitMeter, transmit_meter, set_transmit_meter, 0x37786, "transmit meter", {Po = 0, Swr = 1, Alc = 2, Comp = 3, Vd = 4, Id = 5});
// Key 0x224: read RVA 0x72e43, store RVA 0x72e7b (common ID A2-H54848-38).
stored_enum!(Language, display_language, set_display_language, 0x396bb, "display language", {English = 0, Japanese = 1});
// Key 0x225: read RVA 0x72eb1, store RVA 0x72eed (common ID A2-H54848-39).
stored_setting!(system_language, set_system_language, Language, 0x396bc, "system language", {0 => Language::English, 1 => Language::Japanese});
// Key 0x22a: read RVA 0x73093;0x730e2, store RVA 0x730cf;0x7311e (common ID A2-H54848-40).
stored_enum!(CoordinateDisplay, coordinate_display, set_coordinate_display, 0x396b4, "coordinate display", {DegreesDecimalMinutes = 0, DegreesMinutesSeconds = 1, DecimalDegrees = 2});
// Key 0x22b: read RVA 0x73131, store RVA 0x7316d (common ID A2-H54848-41).
stored_enum!(AltitudeDistanceUnit, altitude_distance_unit, set_altitude_distance_unit, 0x396b5, "altitude/distance unit", {Meters = 0, FeetMiles = 1});
// Key 0x22c: read RVA 0x73180, store RVA 0x731bc (common ID A2-H54848-42).
stored_enum!(SpeedUnit, speed_unit, set_speed_unit, 0x396b6, "speed unit", {KilometersPerHour = 0, MilesPerHour = 1, Knots = 2});
// Key 0x22d: read RVA 0x731d8;0x73227, store RVA 0x73214;0x73263 (common ID A2-H54848-43).
stored_enum!(TemperatureUnit, temperature_unit, set_temperature_unit, 0x396b7, "temperature unit", {Celsius = 0, Fahrenheit = 1});
// Key 0x22e: read RVA 0x73276, store RVA 0x732b2 (common ID A2-H54848-44).
stored_enum!(PressureUnit, pressure_unit, set_pressure_unit, 0x396b8, "pressure unit", {Hectopascals = 0, Millibars = 1, MillimetersMercury = 2, InchesMercury = 3});
// Key 0x22f: read RVA 0x732c5, store RVA 0x73301 (common ID A2-H54848-45).
stored_enum!(RainfallUnit, rainfall_unit, set_rainfall_unit, 0x396b9, "rainfall unit", {Millimeters = 0, Inches = 1});
// Key 0x230: read RVA 0x73314, store RVA 0x73350 (common ID A2-H54848-46).
stored_enum!(WindSpeedUnit, wind_speed_unit, set_wind_speed_unit, 0x396ba, "wind speed unit", {MetersPerSecond = 0, KilometersPerHour = 1, MilesPerHour = 2, Knots = 3});
// Key 0x262: read RVA 0x75bc0, store RVA 0x75bfc (common ID A2-H54848-57).
stored_enum!(TransmitBandwidth, compressor_off_transmit_bandwidth, set_compressor_off_transmit_bandwidth, 0x399b0, "compressor-OFF transmit bandwidth", {Wide = 0, Mid = 1, Narrow = 2});
// Key 0x263: read RVA 0x75c0f, store RVA 0x75c4b (common ID A2-H54848-58).
stored_setting!(compressor_on_transmit_bandwidth, set_compressor_on_transmit_bandwidth, TransmitBandwidth, 0x399b1, "compressor-ON transmit bandwidth", {0 => TransmitBandwidth::Wide, 1 => TransmitBandwidth::Mid, 2 => TransmitBandwidth::Narrow});
// Key 0x27a: read RVA 0x760d9, store RVA 0x76115 (common ID A2-H54848-64).
stored_enum!(RecordingMicrophone, recording_microphone, set_recording_microphone, 0x37816, "recording microphone", {HandMicrophone = 0, BluetoothMicrophone = 1});
// Key 0x25f: read RVA 0x75ac2, store RVA 0x75afe (common ID A2-H54848-70).
stored_enum!(VoxVoiceDelay, vox_voice_delay, set_vox_voice_delay, 0x399ae, "VOX voice delay", {Off = 0, Short = 1, Mid = 2, Long = 3});
// Key 0x28e: read RVA 0x739f4, store RVA 0x73a2d (common ID A2-H54848-72).
stored_enum!(BatteryPowerLimit, battery_power_limit, set_battery_power_limit, 0x39302, "battery power limit", {Milliwatts500 = 0, Watts1 = 1, Milliwatts2500 = 2, Watts5 = 3});
// Key 0x28d: read RVA 0x739ab, store RVA 0x739e4 (common ID A2-H54848-73).
stored_enum!(ExternalPowerLimit, external_power_limit, set_external_power_limit, 0x39303, "external-power limit", {Milliwatts500 = 0, Watts1 = 1, Milliwatts2500 = 2, Watts5 = 3, Watts10 = 4});
// Key 0x290: read RVA 0x73a88, store RVA 0x73ac1 (common ID A2-H54848-75).
stored_enum!(TransmitTimeout, transmit_timeout, set_transmit_timeout, 0x39309, "transmit timeout", {Off = 0, Minutes3 = 1, Minutes5 = 2, Minutes10 = 3, Minutes20 = 4, Minutes30 = 5});
// Key 0x242: read RVA 0x73c2b, store RVA 0x73c64 (common ID A2-H54848-80).
stored_enum!(TunerSelection, tuner_selection, set_tuner_selection, 0x3930f, "tuner selection", {Ah705 = 0, Other = 1});
// Key 0x298: read RVA 0x748bb, store RVA 0x748f7 (common ID A2-H54848-108).
stored_enum!(NotchFilter, notch_filter, set_notch_filter, 0x399a5, "notch filter", {Off = 0, Auto = 1, Manual = 2});
// Key 0x299: read RVA 0x7490a, store RVA 0x74947 (common ID A2-H54848-109).
stored_enum!(ManualNotchWidth, ssb_manual_notch_width, set_ssb_manual_notch_width, 0x399a7, "SSB manual notch width", {Wide = 0, Mid = 1, Narrow = 2});
// Key 0x299: read RVA 0x7490a, store RVA 0x74947 (common ID A2-H54848-110).
stored_setting!(cw_manual_notch_width, set_cw_manual_notch_width, ManualNotchWidth, 0x399a8, "CW manual notch width", {0 => ManualNotchWidth::Wide, 1 => ManualNotchWidth::Mid, 2 => ManualNotchWidth::Narrow});
// Key 0x299: read RVA 0x7490a, store RVA 0x74947 (common ID A2-H54848-111).
stored_setting!(rtty_manual_notch_width, set_rtty_manual_notch_width, ManualNotchWidth, 0x399a9, "RTTY manual notch width", {0 => ManualNotchWidth::Wide, 1 => ManualNotchWidth::Mid, 2 => ManualNotchWidth::Narrow});
// Key 0x299: read RVA 0x7490a, store RVA 0x74947 (common ID A2-H54848-112).
stored_setting!(am_manual_notch_width, set_am_manual_notch_width, ManualNotchWidth, 0x399aa, "AM manual notch width", {0 => ManualNotchWidth::Wide, 1 => ManualNotchWidth::Mid, 2 => ManualNotchWidth::Narrow});
// Key 0x29b: read RVA 0x74ab6, store RVA 0x74aef (common ID A2-H54848-114).
stored_enum!(NotchKeyFunction, ssb_notch_key_function, set_ssb_notch_key_function, 0x39324, "SSB notch-key function", {Auto = 0, Manual = 1, AutoManual = 2});
// Key 0x29c: read RVA 0x74aff, store RVA 0x74b38 (common ID A2-H54848-115).
stored_setting!(am_notch_key_function, set_am_notch_key_function, NotchKeyFunction, 0x39325, "AM notch-key function", {0 => NotchKeyFunction::Auto, 1 => NotchKeyFunction::Manual, 2 => NotchKeyFunction::AutoManual});
// Key 0x29e: read RVA 0x74b91, store RVA 0x74bca (common ID A2-H54848-117).
stored_enum!(CwBfoSide, cw_bfo_side, set_cw_bfo_side, 0x39327, "CW BFO side", {LowerSideband = 0, UpperSideband = 1});
// Key 0x29f: read RVA 0x75106, store RVA 0x75142 (common ID A2-H54848-118).
stored_enum!(ScanSpeed, scan_speed, set_scan_speed, 0x39989, "scan speed", {Slow = 0, Fast = 1});
// Key 0x2a1: read RVA 0x751a4, store RVA 0x751e0 (common ID A2-H54848-120).
stored_enum!(ScanPauseTime, scan_pause_time, set_scan_pause_time, 0x3998b, "scan pause time", {Seconds2 = 0, Seconds4 = 1, Seconds6 = 2, Seconds8 = 3, Seconds10 = 4, Seconds12 = 5, Seconds14 = 6, Seconds16 = 7, Seconds18 = 8, Seconds20 = 9, Hold = 10});
// Key 0x2a2: read RVA 0x751f3, store RVA 0x7522f (common ID A2-H54848-121).
stored_enum!(ScanRestartTime, scan_restart_time, set_scan_restart_time, 0x3998c, "scan restart time", {Seconds0 = 0, Seconds1 = 1, Seconds2 = 2, Seconds3 = 3, Seconds4 = 4, Seconds5 = 5, Hold = 6});
// Key 0x2a3: read RVA 0x75242, store RVA 0x7527e (common ID A2-H54848-122).
stored_enum!(TemporarySkipTime, temporary_skip_time, set_temporary_skip_time, 0x3998d, "temporary skip time", {Minutes5 = 0, Minutes10 = 1, Minutes15 = 2, WhileScanning = 3, UntilPowerOff = 4});
// Key 0x2a5: read RVA 0x75291, store RVA 0x752cd (common ID A2-H54848-123).
stored_enum!(ScanMainDial, scan_main_dial, set_scan_main_dial, 0x3998e, "scan main dial", {Stop = 0, UpDown = 1});

/// Stored Delta-F span codes, not frequency or scan operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DeltaFSpan {
    /// Select 5 kHz.
    Khz5 = 0,
    /// Select 10 kHz.
    Khz10 = 1,
    /// Select 20 kHz.
    Khz20 = 2,
    /// Select 50 kHz.
    Khz50 = 3,
    /// Select 100 kHz.
    Khz100 = 4,
    /// Select 500 kHz.
    Khz500 = 5,
    /// Select 1 MHz.
    Mhz1 = 6,
}

/// Stored select-memory scan codes, not scan operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SelectMemoryScan {
    /// Select all stars.
    AllStars = 0,
    /// Select one star.
    OneStar = 1,
    /// Select two stars.
    TwoStars = 2,
    /// Select three stars.
    ThreeStars = 3,
}

// Key 0x2a6: read RVA 0x75371, store RVA 0x753e2 (common ID A2-H54848-124).
stored_setting!(delta_f_span, set_delta_f_span, DeltaFSpan, 0x37804, "Delta-F span", mask 0x0e, shift 1, {0 => DeltaFSpan::Khz5, 1 => DeltaFSpan::Khz10, 2 => DeltaFSpan::Khz20, 3 => DeltaFSpan::Khz50, 4 => DeltaFSpan::Khz100, 5 => DeltaFSpan::Khz500, 6 => DeltaFSpan::Mhz1});
// Key 0x2a8: read RVA 0x75515, store RVA 0x75573 (common ID A2-H54848-126).
stored_setting!(select_memory_scan, set_select_memory_scan, SelectMemoryScan, 0x37802, "select-memory scan", mask 0x03, shift 0, {0 => SelectMemoryScan::AllStars, 1 => SelectMemoryScan::OneStar, 2 => SelectMemoryScan::TwoStars, 3 => SelectMemoryScan::ThreeStars});
// Key 0x323: read RVA 0x791c9, store RVA 0x79202 (common ID A2-H54848-132).
stored_enum!(RemotePowerOffMode, remote_power_off_mode, set_remote_power_off_mode, 0x39328, "remote power-off mode", {ShutdownOnly = 0, StandbyShutdown = 1});
// Key 0x269: read RVA 0x74cc7, store RVA 0x74d00 (common ID A2-H54848-133).
stored_enum!(VoxBreakInKeyFunction, vox_break_in_key_function, set_vox_break_in_key_function, 0x39332, "VOX/break-in key function", {Transmit = 0, Tuner = 1, VoxBreakIn = 2, PreampAttenuator = 3, Notch = 4, NoiseBlanker = 5, NoiseReduction = 6, Split = 7, VfoAOrB = 8, VfoOrMemory = 9, Cd = 10, Preset = 11, HomeChannel = 12, TemporarySkip = 13, VoiceKeyerRttyMemory1 = 14, VoiceKeyerRttyMemory2 = 15, VoiceKeyerRttyMemory3 = 16, VoiceKeyerRttyMemory4 = 17});
// Key 0x26a: read RVA 0x74d10, store RVA 0x74d49 (common ID A2-H54848-134).
stored_enum!(AutoTuneRxCallsignKeyFunction, auto_tune_rx_callsign_key_function, set_auto_tune_rx_callsign_key_function, 0x39333, "auto-tune/RX-callsign key function", {AutoTuneRxCallsign = 0, CdRxCallsign = 1, PresetRxCallsign = 2, HomeChannelRxCallsign = 3, TemporarySkipRxCallsign = 4});
// Key 0x288: read RVA 0x763ec, store RVA 0x76428 (common ID A2-H54848-145).
stored_setting!(repeater_csv_separator_decimal, set_repeater_csv_separator_decimal, CsvSeparatorDecimal, 0x399e1, "repeater CSV separator/decimal", {0 => CsvSeparatorDecimal::CommaDot, 1 => CsvSeparatorDecimal::SemicolonDot, 2 => CsvSeparatorDecimal::SemicolonComma});
// Key 0x289: read RVA 0x7643b, store RVA 0x76477 (common ID A2-H54848-146).
stored_setting!(repeater_csv_date_format, set_repeater_csv_date_format, CsvDateFormat, 0x399e2, "repeater CSV date format", {0 => CsvDateFormat::YearMonthDay, 1 => CsvDateFormat::MonthDayYear, 2 => CsvDateFormat::DayMonthYear});

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

// Canonical stored CW/RTTY/DTMF scalars, not GUI correction or transmission.

// Exactly three COLORREF-order bytes, never the following USOS/log byte.
macro_rules! stored_color {
    ($get:ident, $set:ident, $at:literal) => {
        impl Icf<'_> {
            /// Read three raw COLORREF-order components (each 0..=255). No I/O.
            pub fn $get(&self) -> Result<[u8; 3], Error> {
                Ok(self.memory[$at..$at + 3]
                    .try_into()
                    .expect("three color bytes"))
            }
            /// Change only three COLORREF-order bytes; preserve all other state.
            pub fn $set(&mut self, value: [u8; 3]) -> Result<(), Error> {
                self.$get()?;
                self.memory[$at..$at + 3].copy_from_slice(&value);
                Ok(())
            }
        }
    };
}
// Raw byte units only: speed, not WPM.
// Key 0x2c1: handler RVA 76da4..76e97, store RVA 76de6 (A2-H56068-01).
raw_level!(cw_speed_raw, set_cw_speed_raw, 0x3999d);
// Key 0x2c2: handler RVA 76e9c..76f0c, store RVA 76f05 (A2-H56068-02).
bounded_word!(
    cw_pitch_hz,
    set_cw_pitch_hz,
    u16,
    0x3999e,
    300,
    900,
    "CW pitch in integer Hz (300..=900, step 5)",
    5
);
// Key 0x2c3: handler RVA 76f11..76f5b, store RVA 76f55 (A2-H56068-03).
stored_enum!(CwNumberStyle, cw_number_style, set_cw_number_style, 0x3996d, "cw number style", {Normal = 0, Digits190Ano = 1, Digits190Ant = 2, Digits90No = 3, Digits90Nt = 4});
// Key 0x2c4: handler RVA 76f60..76faa, store RVA 76fa4 (A2-H56068-04).
bounded_byte!(
    cw_count_up_trigger_channel,
    set_cw_count_up_trigger_channel,
    0x3996e,
    1,
    8,
    0,
    "CW one-based trigger channel M1..M8, not a zero-based memory index"
);
// Key 0x2c5: handler RVA 76faf..76ffe, store RVA 76ff7 (A2-H56068-05).
bounded_word!(
    cw_contest_number,
    set_cw_contest_number,
    u16,
    0x3996f,
    1,
    9999,
    "CW contest integer counter (1..=9999), not BCD"
);
// Raw byte units only: side tone, not percent.
// Key 0x2c6: handler RVA 77003..77186, store RVA 77161 (A2-H56068-06).
raw_level!(cw_side_tone_level_raw, set_cw_side_tone_level_raw, 0x39722);
// Key 0x2c7: handler RVA 7718b..771d5, store RVA 771cf (A2-H56068-07).
stored_setting!(cw_side_tone_limiter, set_cw_side_tone_limiter, bool, 0x39723, "cw side tone limiter", {0 => false, 1 => true});
// Key 0x2c8: handler RVA 771da..77270, store RVA 0x7725f->0xa4870 (A2-H56068-08).
bounded_byte!(
    cw_repeat_seconds,
    set_cw_repeat_seconds,
    0x39971,
    1,
    60,
    0,
    "cw repeat seconds (1..=60)"
);
// Key 0x2c9: handler RVA 77275..772db, store RVA 772d5 (A2-H56068-09).
bounded_byte!(
    cw_dot_dash_ratio_tenths,
    set_cw_dot_dash_ratio_tenths,
    0x39972,
    28,
    45,
    0,
    "cw dot dash ratio tenths (28..=45)"
);
// Key 0x2ca: handler RVA 772e0..77363, store/encode RVA 77347..7735d (A2-H56068-10).
stored_enum!(CwRiseTime, cw_rise_time, set_cw_rise_time, 0x39973, "cw rise time", {Ms2 = 0, Ms4 = 1, Ms6 = 2, Ms8 = 3});
// Key 0x2cb: handler RVA 77368..773b2, store RVA 773ac (A2-H56068-11).
stored_enum!(KeyPolarity, cw_paddle_polarity, set_cw_paddle_polarity, 0x39974, "cw paddle polarity", {Normal = 0, Reverse = 1});
// Key 0x2cc: handler RVA 773b7..77401, store RVA 773fb (A2-H56068-12).
stored_enum!(CwKeyType, cw_key_type, set_cw_key_type, 0x39975, "cw key type", {Straight = 0, Bug = 1, Paddle = 2});
// Key 0x2cd: handler RVA 77406..77450, store RVA 7744a (A2-H56068-13).
stored_enum!(CwMicKeyer, cw_mic_up_down_keyer, set_cw_mic_up_down_keyer, 0x39976, "cw mic up down keyer", {Off = 0, UpDown = 1, AB = 2});
// Key 0x2ce: handler RVA 77455..7749e, store RVA 77498 (A2-H56068-14).
stored_enum!(CwBreakIn, cw_break_in, set_cw_break_in, 0x37787, "cw break in", {Off = 0, Semi = 1, Full = 2});
// Key 0x2cf: handler RVA 774a3..7751c, store RVA 77516 (A2-H56068-15).
bounded_byte!(
    cw_break_in_delay_tenths,
    set_cw_break_in_delay_tenths,
    0x399af,
    20,
    130,
    0,
    "CW break-in delay in dot tenths (20..=130), not milliseconds"
);
// Key 0x2d2: handler RVA 77619..77663, store RVA 7765d (A2-H56207-01).
stored_setting!(rtty_decode_log, set_rtty_decode_log, bool, 0x39987, "rtty decode log", {0 => false, 1 => true});
// Key 0x2d3: handler RVA 77668..776b2, store RVA 776ac (A2-H56207-02).
stored_enum!(RttyLogFormat, rtty_log_format, set_rtty_log_format, 0x39988, "rtty log format", {Text = 0, Html = 1});
// Key 0x2d4: handler RVA 776b7..77701, store RVA 776fb (A2-H56207-03).
stored_setting!(rtty_log_timestamp, set_rtty_log_timestamp, bool, 0x3997e, "rtty log timestamp", {0 => false, 1 => true});
// Key 0x2d5: handler RVA 77706..77750, store RVA 7774a (A2-H56207-04).
stored_enum!(RttyLogTime, rtty_log_time, set_rtty_log_time, 0x3997f, "rtty log time", {Local = 0, Utc = 1});
// Key 0x2d6: handler RVA 77755..7779f, store RVA 77799 (A2-H56207-05).
stored_setting!(rtty_log_frequency, set_rtty_log_frequency, bool, 0x39980, "rtty log frequency", {0 => false, 1 => true});
// Key 0x2d7: handler RVA 777a4..777ee, store RVA 777e8 (A2-H56207-06).
stored_enum!(RttyFftAveraging, rtty_fft_averaging, set_rtty_fft_averaging, 0x39977, "rtty fft averaging", {Off = 0, Two = 1, Three = 2, Four = 3});
// Key 0x2d8: handler RVA 777f3..77810->9d308, store RVA 9d3d3/9d3d5/9d3df (A2-H56207-07).
stored_color!(
    rtty_waveform_color_raw,
    set_rtty_waveform_color_raw,
    0x39978
);
// Key 0x2d9: handler RVA 77815..7785f, store RVA 77859 (A2-H56207-08).
stored_setting!(rtty_decode_usos, set_rtty_decode_usos, bool, 0x3997b, "rtty decode usos", {0 => false, 1 => true});
// Key 0x2da: handler RVA 77864..778ae, store RVA 778a8 (A2-H56207-09).
stored_enum!(RttyLinefeed, rtty_linefeed, set_rtty_linefeed, 0x3997c, "rtty linefeed", {CrOrLfOrCrLf = 0, CrLf = 1});
// Key 0x2db: handler RVA 778b3..7790d, store RVA 77907 (A2-H56207-10).
bounded_byte!(
    rtty_threshold,
    set_rtty_threshold,
    0x37817,
    0,
    15,
    0,
    "rtty threshold (0..=15)"
);
// Key 0x2dc: handler RVA 77912..7795c, store RVA 77956 (A2-H56207-11).
stored_setting!(rtty_transmit_usos, set_rtty_transmit_usos, bool, 0x3997d, "rtty transmit usos", {0 => false, 1 => true});
// Key 0x2de: handler RVA 77961..7797e->9d308, store RVA 9d3d3/9d3d5/9d3df (A2-H56207-12).
stored_color!(
    rtty_receive_font_color_raw,
    set_rtty_receive_font_color_raw,
    0x39981
);
// Key 0x2df: handler RVA 77983..7799f->9d308, store RVA 9d3d3/9d3d5/9d3df (A2-H56207-13).
stored_color!(
    rtty_transmit_font_color_raw,
    set_rtty_transmit_font_color_raw,
    0x39984
);
// Key 0x2e0: handler RVA 779a4..77a5d, store RVA 77a04 (A2-H56207-14).
stored_setting!(rtty_twin_peak, set_rtty_twin_peak, bool, 0x399a0, "RTTY TwinPeak stored preference, independent of active effect or GUI enable conditions", {0 => false, 1 => true});
// Key 0x2e1: handler RVA 77a62..77aad, store RVA 77aa3 (A2-H56207-15).
stored_enum!(RttyMark, rtty_mark, set_rtty_mark, 0x39313, "rtty mark", {Hz1275 = 0, Hz1615 = 1, Hz2125 = 2});
// Key 0x2e2: handler RVA 77ab2..77afd, store RVA 77af3 (A2-H56207-16).
stored_enum!(RttyShift, rtty_shift, set_rtty_shift, 0x39314, "rtty shift", {Hz170 = 0, Hz200 = 1, Hz425 = 2});
// Key 0x2e3: handler RVA 77b02..77b46, store RVA 77b43 (A2-H56207-17).
stored_setting!(rtty_key_polarity, set_rtty_key_polarity, KeyPolarity, 0x39315, "rtty key polarity", {0 => KeyPolarity::Normal, 1 => KeyPolarity::Reverse});
// Key 0x120: handler RVA 6ac88..6acd2, store RVA 6accc (A2-H56361-02).
stored_enum!(DtmfSpeed, dtmf_speed, set_dtmf_speed, 0x399f4, "dtmf speed", {Ms100 = 0, Ms200 = 1, Ms300 = 2, Ms500 = 3});

// Audio Scope stored preferences only, not drawing, measurement, audio/RF
// operation, GUI enable conditions or whole-document save normalization.
// Keys 0x357/0x358/0x35a: read/store 0x7a8fc/0x7a938,
// 0x7a9e9/0x7aa25 and 0x7aa59/0x7aa95. All are full-byte selectors.
stored_enum!(AudioFftAttenuator, audio_fft_attenuator, set_audio_fft_attenuator, 0x37801, "Audio FFT attenuator", {Off = 0, Db10 = 1, Db20 = 2, Db30 = 3});
stored_enum!(AudioFftWaveform, audio_fft_waveform, set_audio_fft_waveform, 0x3995a, "Audio FFT waveform", {Line = 0, Fill = 1});
stored_setting!(audio_fft_waterfall_display, set_audio_fft_waterfall_display, bool, 0x3995e, "Audio FFT waterfall display", {0 => false, 1 => true});
// Keys 0x35b/0x35c: read/store 0x7a94b/0x7a987 and 0x7a99a/0x7a9d6.
// Negative dB and milliseconds per division are labels, not stored quantities.
stored_enum!(AudioOscLevel, audio_osc_level, set_audio_osc_level, 0x37800, "Audio Osc level", {Db0 = 0, Minus10Db = 1, Minus20Db = 2, Minus30Db = 3});
stored_enum!(AudioOscSweepTime, audio_osc_sweep_time, set_audio_osc_sweep_time, 0x377ff, "Audio Osc sweep time", {Ms1 = 0, Ms3 = 1, Ms10 = 2, Ms30 = 3, Ms100 = 4, Ms300 = 5});
// Keys 0x359/0x35d: 0x7aa44/0x7aab5 -> 0x9d308. COLORREF 0x00BBGGRR
// is stored as exactly R,G,B bytes at 0x9d3d3/0x9d3d5/0x9d3df, never alpha.
stored_color!(
    audio_fft_waveform_color,
    set_audio_fft_waveform_color,
    0x3995b
);
stored_color!(
    audio_osc_waveform_color,
    set_audio_osc_waveform_color,
    0x3995f
);

fn swr_graph_offset(slot: usize) -> Result<usize, Error> {
    if slot >= 17 {
        return Err(Error::Unsupported {
            line: 5,
            reason: "SWR graph physical slot outside 0..=16",
        });
    }
    Ok(0x379b9 + 2 * slot)
}

impl Icf<'_> {
    /// Read a raw SWR measuring-step index from physical slot 0..=16 (all byte values).
    /// Codes 0..3 label 10k/50k/100k/500k; this does not convert to Hz or measure RF.
    /// Only_50M/Only_TX70M/Only_FMradio_Air visibility and band/model applicability
    /// remain unverified and are separate from these 17 physical storage slots.
    /// # Errors
    /// Rejects an invalid slot, without inferring a display-row or band mapping.
    pub fn swr_graph_step_index(&self, slot: usize) -> Result<u8, Error> {
        Ok(self.memory[swr_graph_offset(slot)?])
    }

    /// Replace only one stored SWR step index with 0..=3, even if its old byte is unknown.
    /// No paired bar, other slot, GUI visibility or measurement state is changed.
    /// # Errors
    /// Slot bounds take priority; an invalid new index leaves all state intact.
    pub fn set_swr_graph_step_index(&mut self, slot: usize, value: u8) -> Result<(), Error> {
        let at = swr_graph_offset(slot)?;
        if value > 3 {
            return Err(Error::Invalid {
                line: 5 + at / RECORD_BYTES,
                reason: "new SWR graph step index outside 0..=3",
            });
        }
        self.memory[at] = value;
        Ok(())
    }

    /// Read a raw SWR graph-bar index from physical slot 0..=16 (all byte values).
    /// Codes 0..5 label 3/5/7/9/11/13 bars, not literal bar counts to store.
    /// The unresolved Only_50M/Only_TX70M/Only_FMradio_Air visibility conditions
    /// and band/model applicability are separate from physical slot storage.
    /// # Errors
    /// Rejects an invalid slot; unknown stored bytes are returned without defaulting.
    pub fn swr_graph_bar_index(&self, slot: usize) -> Result<u8, Error> {
        Ok(self.memory[swr_graph_offset(slot)? + 1])
    }

    /// Replace only one stored SWR bar index with 0..=5, even if its old byte is unknown.
    /// Preserves the paired step, all other slots and every non-target byte.
    /// # Errors
    /// Slot bounds take priority; an invalid new index leaves all state intact.
    pub fn set_swr_graph_bar_index(&mut self, slot: usize, value: u8) -> Result<(), Error> {
        let at = swr_graph_offset(slot)? + 1;
        if value > 5 {
            return Err(Error::Invalid {
                line: 5 + at / RECORD_BYTES,
                reason: "new SWR graph bar index outside 0..=5",
            });
        }
        self.memory[at] = value;
        Ok(())
    }
}

// Spectrum Scope stored preferences only. No GUI enable/correction, REF/FIX/G
// generation, drawing, FFT/DSP/RF operation or whole-document save is reproduced.
// Key 0x35e: read 0x7aac9/0x7aad7, store 0x7ab30/0x7ab1c in two separate bytes.
/// Stored Spectrum mode, split across fixed/scroll bytes rather than one index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeMode {
    /// Select CENTER.
    Center,
    /// Select FIX.
    Fixed,
    /// Select SCROLL (CENTER).
    ScrollCenter,
    /// Select SCROLL (FIX).
    ScrollFixed,
}

impl Icf<'_> {
    /// Read the two canonical stored Spectrum mode bytes without masking them.
    /// This is storage only, not GUI mode coupling, FIX edge or REF calculation.
    /// # Errors
    /// Rejects an unknown byte, checking the fixed byte before the scroll byte.
    pub fn scope_mode(&self) -> Result<ScopeMode, Error> {
        let fixed = self.memory[0x377c6];
        if fixed > 1 {
            return Err(Error::Unsupported {
                line: 5 + 0x377c6 / RECORD_BYTES,
                reason: "scope_mode fixed byte outside 0..=1",
            });
        }
        let scroll = self.memory[0x3808a];
        if scroll > 1 {
            return Err(Error::Unsupported {
                line: 5 + 0x3808a / RECORD_BYTES,
                reason: "scope_mode scroll byte outside 0..=1",
            });
        }
        Ok(match (fixed, scroll) {
            (0, 0) => ScopeMode::Center,
            (1, 0) => ScopeMode::Fixed,
            (0, 1) => ScopeMode::ScrollCenter,
            (1, 1) => ScopeMode::ScrollFixed,
            _ => unreachable!("both mode bytes were checked"),
        })
    }

    /// Change only the two stored mode bytes, preserving Span and every other byte.
    /// No UI refresh flag, defaulting, parent normalization or I/O occurs.
    /// # Errors
    /// Validates both current bytes before either write; unknown current codes
    /// leave the entire state, including prior successful edits, unchanged.
    pub fn set_scope_mode(&mut self, value: ScopeMode) -> Result<(), Error> {
        self.scope_mode()?;
        let (fixed, scroll) = match value {
            ScopeMode::Center => (0, 0),
            ScopeMode::Fixed => (1, 0),
            ScopeMode::ScrollCenter => (0, 1),
            ScopeMode::ScrollFixed => (1, 1),
        };
        self.memory[0x377c6] = fixed;
        self.memory[0x3808a] = scroll;
        Ok(())
    }
}

// Keys 0x35f/0x361: read/store 0x7ab4a/0x7ab93 and 0x7abc6/0x7ac01.
stored_enum!(ScopeSpan, scope_span, set_scope_span, 0x377c7, "Spectrum span caption (not Hz)", {K2p5 = 0, K5 = 1, K10 = 2, K25 = 3, K50 = 4, K100 = 5, K250 = 6, K500 = 7});
stored_enum!(ScopeSweepSpeed, scope_sweep_speed, set_scope_sweep_speed, 0x377d9, "Spectrum sweep speed", {Fast = 0, Mid = 1, Slow = 2});
// Keys 0x363/0x364: read/store 0x7ac14/0x7ac50 and 0x7ac63/0x7ac9f.
stored_setting!(scope_during_tx_center, set_scope_during_tx_center, bool, 0x39725, "Spectrum during TX (CENTER Type) preference", {0 => false, 1 => true});
stored_enum!(ScopeMaxHold, scope_max_hold, set_scope_max_hold, 0x39726, "Spectrum max hold", {Off = 0, Hold10Seconds = 1, On = 2});
// Keys 0x365/0x366: read/store 0x7acb2/0x7acee and 0x7ad01/0x7ad3d.
stored_enum!(ScopeCenterDisplay, scope_center_display, set_scope_center_display, 0x3972b, "Spectrum CENTER type display", {FilterCenter = 0, CarrierPoint = 1, CarrierPointAbsolute = 2});
stored_enum!(ScopeMarkerPosition, scope_marker_position, set_scope_marker_position, 0x3972c, "Spectrum marker position", {FilterCenter = 0, CarrierPoint = 1});
// Keys 0x367/0x368/0x369: read/store 0x7ad50/0x7ad8c,
// 0x7ad9f/0x7addb and 0x7adee/0x7ae2a. All selectors use full-byte codes.
stored_enum!(ScopeVbw, scope_vbw, set_scope_vbw, 0x39736, "Spectrum VBW", {Narrow = 0, Wide = 1});
stored_enum!(ScopeAveraging, scope_averaging, set_scope_averaging, 0x39737, "Spectrum averaging", {Off = 0, Two = 1, Three = 2, Four = 3});
stored_enum!(ScopeWaveform, scope_waveform, set_scope_waveform, 0x39727, "Spectrum waveform", {Fill = 0, FillAndLine = 1});
// Keys 0x36a/0x36b/0x36c: callers 0x7ae4a/0x7ae6c/0x7ae8d -> 0x9d308.
// Exactly R,G,B bytes (COLORREF 0x00BBGGRR), no alpha or following VBW byte.
stored_color!(
    scope_waveform_color_current,
    set_scope_waveform_color_current,
    0x3972d
);
stored_color!(
    scope_waveform_color_line,
    set_scope_waveform_color_line,
    0x39730
);
stored_color!(
    scope_waveform_color_max_hold,
    set_scope_waveform_color_max_hold,
    0x39733
);
// Keys 0x36d/0x36e: read/store 0x7aea2/0x7aede and 0x7aef1/0x7af2d.
// Waterfall speed order is the reverse of sweep speed; do not share their enum.
stored_setting!(scope_waterfall_display, set_scope_waterfall_display, bool, 0x39728, "Spectrum waterfall display", {0 => false, 1 => true});
stored_enum!(ScopeWaterfallSpeed, scope_waterfall_speed, set_scope_waterfall_speed, 0x3972a, "Spectrum waterfall speed", {Slow = 0, Mid = 1, Fast = 2});
// Keys 0x36f/0x370/0x371: read/store 0x7af40/0x7af7c,
// 0x7af8f/0x7afcb and 0x7afde/0x7b01a. Grid captions are not raw grid numbers.
stored_enum!(ScopeWaterfallSize, scope_waterfall_size_expand, set_scope_waterfall_size_expand, 0x39738, "Spectrum waterfall size (Expand Screen)", {Small = 0, Mid = 1, Large = 2});
stored_enum!(ScopeWaterfallPeak, scope_waterfall_peak_color_level, set_scope_waterfall_peak_color_level, 0x39729, "Spectrum waterfall peak color level", {Grid1 = 0, Grid2 = 1, Grid3 = 2, Grid4 = 3, Grid5 = 4, Grid6 = 5, Grid7 = 6, Grid8 = 7});
stored_setting!(scope_waterfall_marker_autohide, set_scope_waterfall_marker_autohide, bool, 0x39739, "Spectrum waterfall marker auto-hide", {0 => false, 1 => true});
