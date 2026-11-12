//! Semantic setting catalog and the original simple-selector edit operation.
//!
//! The catalog is schema, not a factory image. Each entry names its original
//! help ID and resource key. This first family covers full-byte selectors with
//! no additional case-body guard or dependent correction; numeric, indexed,
//! text and coupled handlers must not be inferred from these simple entries.
//! Source: A4 `facts.tsv`, accepted BA04/BA14 normal group-25 Basic routes.

use std::{borrow::Cow, sync::OnceLock};

use super::{Error, Icf};

pub mod digital;
pub mod extended;
pub mod gps;
pub mod network;

/// A contiguous numeric code domain; codes are not physical units or defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Domain {
    /// Smallest accepted checked-write value.
    pub min: i64,
    /// Largest accepted checked-write value.
    pub max: i64,
    /// Increment measured from `min`.
    pub step: i64,
}

/// One semantic stored setting, not a raw offset exposed as a CLI key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    /// Stable ASCII semantic property name, using source names where established.
    pub name: Cow<'static, str>,
    /// Original inventory help-feature ID; multiple controls can share one ID.
    pub feature_id: Cow<'static, str>,
    /// Original resource/dispatch key, distinct from the feature ID.
    pub key: u16,
    /// Popup selection-node key, not the outer setting dispatch key (normally 1).
    pub selection_key: u16,
    /// Legal code domain for checked writes.
    pub domain: Domain,
    /// Number of indexed instances; currently one for simple selectors.
    pub count: usize,
    /// Code versus a measured integer unit; this does not imply a native editor.
    pub kind: Kind,
    offset: usize,
    width: usize,
    signed: bool,
    bias: i64,
    stride: usize,
}

/// Semantic value interpretation, independently of the storage width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// Contiguous raw choice codes; use `select` for the original simple handler.
    Selector,
    /// Integer in the explicitly named units, not a display percentage conversion.
    Integer(Cow<'static, str>),
}

/// The two immediate keyboard selector edits; Enter only opens the native popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Space: advance and wrap.
    Next,
    /// Backspace: retreat and wrap.
    Previous,
}

/// Decoded values retain unknown stored numeric codes without sanitizing them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// Raw semantic code, not a display caption index lookup result.
    Number(i64),
}

/// Native selection result supplied by an offline caller, not a GUI invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Selection-node key must match `Setting::selection_key`, not its outer key.
    pub key: u16,
    /// Original accepted-selection phase is 2.
    pub phase: u32,
    /// Full candidate; original handlers store its low byte without a domain guard.
    pub candidate: i32,
    /// Native helper returned nonzero, independently of the candidate.
    pub accepted: bool,
}

/// Field effects; a store does not establish document dirty or pixel delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    /// Whether the case-body store executed, even if the byte was unchanged.
    pub stored: bool,
    /// Original case-body notification bits, before common callback gating.
    pub notify_flags: u32,
}

/// Case-body presentation/entry state, not a complete parent-GUI enable policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlState {
    /// Whether this case reaches its editor helper at all.
    pub helper_enabled: bool,
    /// Post-store gray presentation; gray is not an editing prohibition.
    pub gray: bool,
    /// WLAN authentication-length warning mark, not the secret itself.
    pub marked: bool,
}

/// All implemented semantic descriptors, in accepted source order.
pub fn catalog() -> &'static [Setting] {
    static SETTINGS: OnceLock<Vec<Setting>> = OnceLock::new();
    SETTINGS.get_or_init(|| {
        include_str!("settings.tsv")
            .lines()
            .map(|line| {
                // This is compiled project schema, never an untrusted runtime file.
                let mut cells = line.split('\t');
                let feature_id = cells.next().unwrap();
                let name = cells.next().unwrap();
                let key = u16::from_str_radix(cells.next().unwrap(), 16).unwrap();
                let offset = usize::from_str_radix(cells.next().unwrap(), 16).unwrap();
                let max = cells.next().unwrap().parse().unwrap();
                let selection_key = cells
                    .next()
                    .map_or(1, |s| u16::from_str_radix(s, 16).unwrap());
                assert!(cells.next().is_none());
                Setting {
                    feature_id: Cow::Borrowed(feature_id),
                    name: Cow::Borrowed(name),
                    key,
                    selection_key,
                    offset,
                    domain: Domain {
                        min: 0,
                        max,
                        step: 1,
                    },
                    count: 1,
                    kind: Kind::Selector,
                    width: 1,
                    signed: false,
                    bias: 0,
                    stride: 0,
                }
            })
            .chain(numeric_catalog())
            .collect()
    })
}

/// Resolve a semantic source property or exact help-feature ID.
///
/// A feature ID shared by multiple controls is ambiguous and must be selected
/// by property name instead. Unknown names are errors, never raw offsets.
pub fn find(name: &str) -> Result<&'static Setting, Error> {
    let mut entries = catalog()
        .iter()
        .filter(|s| s.name == name || s.feature_id == name);
    let entry = entries
        .next()
        .ok_or_else(|| invalid("unknown semantic setting"))?;
    if entries.next().is_some() {
        return Err(invalid("ambiguous feature ID; use property name"));
    }
    Ok(entry)
}

impl Setting {
    fn check_index(&self, index: usize) -> Result<(), Error> {
        if index < self.count {
            Ok(())
        } else {
            Err(invalid("setting index outside descriptor"))
        }
    }

    fn read_storage(&self, image: &Icf<'_>, index: usize) -> Result<i64, Error> {
        self.check_index(index)?;
        let at = self.offset + self.stride * index;
        let mut bytes = [0; 8];
        bytes[..self.width].copy_from_slice(&image.memory[at..at + self.width]);
        if self.signed && bytes[self.width - 1] & 0x80 != 0 {
            bytes[self.width..].fill(0xff);
        }
        Ok(i64::from_le_bytes(bytes))
    }

    /// Read the actual stored code, including unknown values, without correction.
    pub fn read(&self, image: &Icf<'_>, index: usize) -> Result<Value, Error> {
        Ok(Value::Number(self.read_storage(image, index)? + self.bias))
    }

    /// Read an Integer editor's local raw DWORD before semantic bias. Width and
    /// sign use the same storage decoder as `read`; selector entries reject.
    pub fn read_numeric_raw(&self, image: &Icf<'_>, index: usize) -> Result<i32, Error> {
        if !matches!(self.kind, Kind::Integer(_)) {
            return Err(invalid("raw numeric read requires Integer descriptor"));
        }
        Ok(self.read_storage(image, index)? as i32)
    }

    /// Store an already accepted numeric helper's raw local, truncating to the
    /// descriptor width. This named selected-event bypass checks Integer/index,
    /// not domain/current value/bias or GUI corrections. Checked `write` is
    /// unchanged and must still be used for ordinary semantic API replacements.
    pub fn store_numeric_raw(
        &self,
        image: &mut Icf<'_>,
        index: usize,
        raw: i32,
    ) -> Result<(), Error> {
        if !matches!(self.kind, Kind::Integer(_)) {
            return Err(invalid("raw numeric store requires Integer descriptor"));
        }
        self.check_index(index)?;
        let at = self.offset + self.stride * index;
        image.memory[at..at + self.width].copy_from_slice(&raw.to_le_bytes()[..self.width]);
        Ok(())
    }

    /// Explicit checked replacement; unknown current codes do not block a legal edit.
    ///
    /// All validation precedes the bounded field store. This is an atomic API, not
    /// the original raw selected-event bypass or a document-save pipeline.
    pub fn write(&self, image: &mut Icf<'_>, index: usize, value: &Value) -> Result<(), Error> {
        self.check_index(index)?;
        let Value::Number(value) = *value;
        let Domain { min, max, step } = self.domain;
        if !(min..=max).contains(&value) || (value - min) % step != 0 {
            return Err(invalid("setting value outside semantic domain"));
        }
        let at = self.offset + self.stride * index;
        let raw = (value - self.bias).to_le_bytes();
        if !self.control_state(image, index, false)?.helper_enabled {
            return Err(invalid(
                "setting editor is disabled by its actual case guard",
            ));
        }
        if self.kind == Kind::Selector && !self.choices(image, index)?.contains(&value) {
            return Err(invalid(
                "new setting choice is filtered in the current context",
            ));
        }
        image.memory[at..at + self.width].copy_from_slice(&raw[..self.width]);
        self.after_store(image, raw[0]);
        Ok(())
    }

    /// Apply the original simple-selector accepted-event branch.
    ///
    /// Key mismatch, phase other than 2 or helper cancellation executes no store.
    /// Otherwise the low byte is written, even outside the displayed choices,
    /// with only that case's additional notification bits. No current-value
    /// guard or rollback is added; the common initial flag 1 is not returned.
    pub fn select(
        &self,
        image: &mut Icf<'_>,
        index: usize,
        event: Selection,
    ) -> Result<Effect, Error> {
        self.check_index(index)?;
        if self.kind != Kind::Selector {
            return Err(invalid(
                "numeric editor requires its own operation contract",
            ));
        }
        let stored = event.accepted
            && event.key == self.selection_key
            && event.phase == 2
            && self.control_state(image, index, false)?.helper_enabled;
        if stored {
            let at = self.offset + self.stride * index;
            image.memory[at] = event.candidate as u8;
            self.after_store(image, event.candidate as u8);
        }
        Ok(Effect {
            stored,
            // Four original scalar cases and six later, separately traced cases.
            notify_flags: if stored
                && matches!(
                    self.key,
                    0x212 | 0x225 | 0x2aa | 0x2e1 | 0x2e2 | 0x2fb | 0x30f | 0x316 | 0x317 | 0x318
                ) {
                0x20
            } else {
                0
            },
        })
    }

    /// Apply Space/Backspace to the actual contiguous choice domain.
    /// An unknown current code goes to the first choice in either direction,
    /// rather than being rejected or repaired merely by reading/displaying it.
    pub fn cycle(&self, image: &mut Icf<'_>, index: usize, step: Step) -> Result<Effect, Error> {
        if self.kind != Kind::Selector {
            return Err(invalid("numeric editor is not a simple selector"));
        }
        let Value::Number(current) = self.read(image, index)?;
        let choices = self.choices(image, index)?;
        let candidate = match choices.iter().position(|&value| value == current) {
            None => choices[0],
            Some(at) => {
                choices[match step {
                    Step::Next => (at + 1) % choices.len(),
                    Step::Previous => (at + choices.len() - 1) % choices.len(),
                }]
            }
        };
        self.select(
            image,
            index,
            Selection {
                key: self.selection_key,
                phase: 2,
                candidate: candidate as i32,
                accepted: true,
            },
        )
    }

    /// Actual contiguous selector choices after the current USB assignment filter.
    /// Existing conflicting current values remain in the list, as 9A6C4 does.
    pub fn choices(&self, image: &Icf<'_>, index: usize) -> Result<Vec<i64>, Error> {
        if self.kind != Kind::Selector {
            return Err(invalid("numeric editor has no simple selector list"));
        }
        let Value::Number(current) = self.read(image, index)?;
        let send = i64::from(image.memory[0x39346]);
        Ok((self.domain.min..=self.domain.max)
            .filter(|&value| {
                !matches!(self.key, 0x2fc | 0x2fd)
                    || value == current
                    || value == 0
                    || value != send
            })
            .collect())
    }

    /// Explicit case-body display effects. `marker_flag` is the native caller's
    /// suppress-mark flag; no password contents or lossy text are returned.
    pub fn control_state(
        &self,
        image: &Icf<'_>,
        index: usize,
        marker_flag: bool,
    ) -> Result<ControlState, Error> {
        self.check_index(index)?;
        let helper_enabled =
            self.key != 0x2e0 || (image.memory[0x39313] == 2 && image.memory[0x39314] == 0);
        let gray = self.key == 0x311 && image.memory[0x39353] == 1;
        let marked = if self.key == 0x316
            && image.memory[0x39359] != 0
            && image.memory[0x39a05] == 1
            && !marker_flag
        {
            let bytes = &image.memory[0x39a27..0x39a66];
            let prefix = bytes.split(|&b| b == 0).next().unwrap();
            prefix.iter().rposition(|&b| b > 0x20).map_or(0, |p| p + 1) < 8
        } else {
            false
        };
        Ok(ControlState {
            helper_enabled,
            gray,
            marked,
        })
    }

    fn after_store(&self, image: &mut Icf<'_>, byte: u8) {
        if self.key == 0x2fb {
            for at in [0x39347, 0x39348] {
                if image.memory[at] == byte {
                    image.memory[at] = 0;
                }
            }
        }
    }
}

fn invalid(reason: &'static str) -> Error {
    Error::Invalid {
        line: 0,
        reason: reason.into(),
    }
}

// Stored integer codecs reuse one width/sign/bias path. This is project schema,
// not a serialized default image. Native parsing, clamp and postlude remain
// separate operations; do not use `select` as a numeric-editor substitute.
macro_rules! integer {
    ($id:literal, $name:literal, $key:literal, $at:literal, $width:literal, $signed:literal,
     $bias:literal, $min:literal, $max:literal, $step:literal, $unit:literal) => {
        Setting {
            feature_id: Cow::Borrowed($id),
            name: Cow::Borrowed($name),
            key: $key,
            selection_key: 1,
            offset: $at,
            width: $width,
            signed: $signed,
            bias: $bias,
            count: 1,
            stride: 0,
            domain: Domain {
                min: $min,
                max: $max,
                step: $step,
            },
            kind: Kind::Integer(Cow::Borrowed($unit)),
        }
    };
}

fn numeric_catalog() -> Vec<Setting> {
    vec![
        integer!(
            "A2-H54848-25",
            "lcd_backlight_raw",
            0x211,
            0x3969d,
            1,
            false,
            0,
            0,
            255,
            1,
            "raw level"
        ),
        integer!(
            "A2-H54848-47",
            "beep_level_raw",
            0x231,
            0x392f2,
            1,
            false,
            0,
            0,
            255,
            1,
            "raw level"
        ),
        integer!(
            "A2-H54848-54",
            "microphone_gain_raw",
            0x2ab,
            0x39998,
            1,
            false,
            0,
            0,
            255,
            1,
            "raw level"
        ),
        integer!(
            "A2-H54848-60",
            "monitor_level_raw",
            0x265,
            0x3999c,
            1,
            false,
            0,
            0,
            255,
            1,
            "raw level"
        ),
        integer!(
            "A2-H54848-61",
            "voice_transmit_level_raw",
            0x277,
            0x39964,
            1,
            false,
            0,
            0,
            255,
            1,
            "raw level"
        ),
        integer!(
            "A2-H54848-65",
            "recording_microphone_gain_raw",
            0x27b,
            0x37815,
            1,
            false,
            0,
            0,
            255,
            1,
            "raw level"
        ),
        integer!(
            "A2-H54848-67",
            "vox_gain_raw",
            0x25c,
            0x399ab,
            1,
            false,
            0,
            0,
            255,
            1,
            "raw level"
        ),
        integer!(
            "A2-H54848-68",
            "anti_vox_gain_raw",
            0x25d,
            0x399ac,
            1,
            false,
            0,
            0,
            255,
            1,
            "raw level"
        ),
        integer!(
            "A2-H54848-92",
            "speech_level_raw",
            0x24e,
            0x3931e,
            1,
            false,
            0,
            0,
            255,
            1,
            "raw level"
        ),
        integer!(
            "A2-H54848-103",
            "noise_blanker_level_raw",
            0x293,
            0x399a1,
            1,
            false,
            0,
            0,
            255,
            1,
            "raw level"
        ),
        integer!(
            "A2-H54848-105",
            "noise_blanker_width_raw",
            0x295,
            0x399a3,
            1,
            false,
            0,
            0,
            255,
            1,
            "raw level"
        ),
        integer!(
            "A2-H54848-56",
            "speech_compression_level",
            0x261,
            0x3999a,
            1,
            false,
            0,
            0,
            10,
            1,
            "level"
        ),
        integer!(
            "A2-H54848-63",
            "voice_repeat_seconds",
            0x278,
            0x39963,
            1,
            false,
            0,
            1,
            15,
            1,
            "seconds"
        ),
        integer!(
            "A2-H54848-69",
            "vox_delay_deciseconds",
            0x25e,
            0x399ad,
            1,
            false,
            0,
            0,
            20,
            1,
            "deciseconds"
        ),
        integer!(
            "A2-H54848-104",
            "noise_blanker_depth",
            0x294,
            0x399a2,
            1,
            false,
            1,
            1,
            10,
            1,
            "count"
        ),
        integer!(
            "A2-H54848-107",
            "noise_reduction_level",
            0x297,
            0x399a4,
            1,
            false,
            0,
            0,
            15,
            1,
            "level"
        ),
        integer!(
            "A2-H54848-78",
            "split_offset_khz",
            0x240,
            0x3930c,
            2,
            true,
            0,
            -9999,
            9999,
            1,
            "kHz"
        ),
        integer!(
            "A2-H54848-144",
            "utc_offset_minutes",
            0x229,
            0x396ff,
            2,
            true,
            0,
            -840,
            840,
            5,
            "minutes"
        ),
        integer!(
            "A2-H54848-147",
            "reference_frequency_adjustment",
            0x2ad,
            0x39330,
            2,
            false,
            0,
            0,
            511,
            1,
            "count"
        ),
        integer!(
            "A2-H54848-113",
            "manual_notch_position",
            0x29a,
            0x399a6,
            1,
            false,
            -128,
            -127,
            127,
            1,
            "position"
        ),
        integer!(
            "A2-H56068-01",
            "cw_speed_raw",
            0x2c1,
            0x3999d,
            1,
            false,
            0,
            0,
            255,
            1,
            "raw level"
        ),
        integer!(
            "A2-H56068-02",
            "cw_pitch_hz",
            0x2c2,
            0x3999e,
            2,
            false,
            0,
            300,
            900,
            5,
            "Hz"
        ),
        integer!(
            "A2-H56068-04",
            "cw_count_up_trigger_channel",
            0x2c4,
            0x3996e,
            1,
            false,
            0,
            1,
            8,
            1,
            "one-based channel"
        ),
        integer!(
            "A2-H56068-05",
            "cw_contest_number",
            0x2c5,
            0x3996f,
            2,
            false,
            0,
            1,
            9999,
            1,
            "integer counter"
        ),
        integer!(
            "A2-H56068-06",
            "cw_side_tone_level_raw",
            0x2c6,
            0x39722,
            1,
            false,
            0,
            0,
            255,
            1,
            "raw level"
        ),
        integer!(
            "A2-H56068-08",
            "cw_repeat_seconds",
            0x2c8,
            0x39971,
            1,
            false,
            0,
            1,
            60,
            1,
            "seconds"
        ),
        integer!(
            "A2-H56068-09",
            "cw_dot_dash_ratio_tenths",
            0x2c9,
            0x39972,
            1,
            false,
            0,
            28,
            45,
            1,
            "ratio tenths"
        ),
        integer!(
            "A2-H56068-15",
            "cw_break_in_delay_tenths",
            0x2cf,
            0x399af,
            1,
            false,
            0,
            20,
            130,
            1,
            "dot tenths"
        ),
        integer!(
            "A2-H56207-10",
            "rtty_threshold",
            0x2db,
            0x37817,
            1,
            false,
            0,
            0,
            15,
            1,
            "level"
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_metadata_clone_preserves_private_storage() {
        let source = find("lcd_backlight_raw").unwrap();
        assert!(matches!(source.name, Cow::Borrowed(_)));
        let mut descriptor = source.clone();
        descriptor.name = Cow::Owned("custom numeric name".to_owned());
        descriptor.feature_id = Cow::Owned("custom feature".to_owned());
        descriptor.kind = Kind::Integer(Cow::Owned("custom unit".to_owned()));
        let mut edited = image();
        descriptor.store_numeric_raw(&mut edited, 0, 73).unwrap();
        assert_eq!(source.read_numeric_raw(&edited, 0).unwrap(), 73);
        assert_eq!(source.name, "lcd_backlight_raw");
        assert_eq!(descriptor.kind, Kind::Integer(Cow::Borrowed("custom unit")));
    }

    fn image() -> Icf<'static> {
        let mut memory = vec![0xe7; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        Icf::from_memory(memory).unwrap()
    }

    #[test]
    fn every_descriptor_checks_input_and_replaces_unknown_current_without_other_writes() {
        let mut image = image();
        let mut expected = image.memory().to_vec();
        let mut keys = std::collections::HashSet::new();
        for setting in catalog() {
            if setting.key == 0x2e0 {
                image.memory[0x39313] = 2;
                image.memory[0x39314] = 0;
                expected[0x39313..0x39315].copy_from_slice(&[2, 0]);
            }
            assert!(keys.insert(setting.key));
            assert_eq!(find(&setting.name).unwrap(), setting);
            if setting.kind == Kind::Selector {
                assert_eq!(
                    setting.read(&image, 0).unwrap(),
                    Value::Number(i64::from(expected[setting.offset]))
                );
            }
            for value in [setting.domain.min - 1, setting.domain.max + 1] {
                assert!(setting.write(&mut image, 0, &Value::Number(value)).is_err());
                assert_eq!(image.memory(), expected);
            }
            assert!(setting.write(&mut image, 1, &Value::Number(0)).is_err());
            let value = if setting.kind == Kind::Selector {
                *setting.choices(&image, 0).unwrap().last().unwrap()
            } else {
                setting.domain.max
            };
            setting.write(&mut image, 0, &Value::Number(value)).unwrap();
            let raw = (value - setting.bias).to_le_bytes();
            expected[setting.offset..setting.offset + setting.width]
                .copy_from_slice(&raw[..setting.width]);
            assert_eq!(image.memory(), expected);
            assert_eq!(setting.read(&image, 0).unwrap(), Value::Number(value));
        }
        assert!(find("A2-H56202-18").is_err());
        assert!(find("0x39323").is_err());
    }

    #[test]
    fn literal_source_offsets_and_raw_event_domain_bypass_are_independent() {
        let mut image = image();
        let mut expected = image.memory().to_vec();
        for (id, key, offset, max) in [
            ("A2-H54848-99", 0x258, 0x39323, 1),
            ("A2-H56399-12", 0x2f8, 0x3933f, 3),
            ("A2-H55922-05", 0x191, 0x399be, 10),
            ("A2-H56095-17", 0x370, 0x39729, 7),
        ] {
            let setting = find(id).unwrap();
            assert_eq!(setting.key, key);
            assert_eq!(setting.selection_key, 1);
            assert_eq!(setting.domain.max, max);
            assert!(
                !setting
                    .select(
                        &mut image,
                        0,
                        Selection {
                            key,
                            phase: 2,
                            accepted: true,
                            candidate: 0
                        }
                    )
                    .unwrap()
                    .stored
            );
            assert_eq!(image.memory(), expected);
            for (phase, accepted) in [(1, true), (2, false)] {
                assert_eq!(
                    setting
                        .select(
                            &mut image,
                            0,
                            Selection {
                                key: 1,
                                phase,
                                accepted,
                                candidate: -2
                            }
                        )
                        .unwrap(),
                    Effect {
                        stored: false,
                        notify_flags: 0
                    }
                );
                assert_eq!(image.memory(), expected);
            }
            assert_eq!(
                setting
                    .select(
                        &mut image,
                        0,
                        Selection {
                            key: 1,
                            phase: 2,
                            accepted: true,
                            candidate: 0x1fe
                        }
                    )
                    .unwrap(),
                Effect {
                    stored: true,
                    notify_flags: 0
                }
            );
            expected[offset] = 0xfe;
            assert_eq!(image.memory(), expected);
            assert_eq!(setting.read(&image, 0).unwrap(), Value::Number(254));
            setting.write(&mut image, 0, &Value::Number(0)).unwrap();
            expected[offset] = 0;
            assert_eq!(image.memory(), expected);
        }
    }

    #[test]
    fn literal_signed_biased_word_and_step_encodings_keep_unknown_replacement_explicit() {
        let mut image = image();
        let mut expected = image.memory().to_vec();
        for (name, value, offset, raw) in [
            ("noise_blanker_depth", 10, 0x399a2, &[9][..]),
            ("manual_notch_position", -127, 0x399a6, &[1][..]),
            ("split_offset_khz", -9999, 0x3930c, &[0xf1, 0xd8][..]),
            ("utc_offset_minutes", -840, 0x396ff, &[0xb8, 0xfc][..]),
            ("cw_pitch_hz", 900, 0x3999e, &[0x84, 3][..]),
            ("cw_contest_number", 9999, 0x3996f, &[0x0f, 0x27][..]),
        ] {
            let setting = find(name).unwrap();
            setting.write(&mut image, 0, &Value::Number(value)).unwrap();
            expected[offset..offset + raw.len()].copy_from_slice(raw);
            assert_eq!(image.memory(), expected);
            assert_eq!(setting.read(&image, 0).unwrap(), Value::Number(value));
            assert!(
                setting
                    .select(
                        &mut image,
                        0,
                        Selection {
                            key: setting.key,
                            phase: 2,
                            candidate: 0,
                            accepted: true
                        }
                    )
                    .is_err()
            );
            assert_eq!(image.memory(), expected);
        }
        for (name, value) in [
            ("utc_offset_minutes", 1),
            ("cw_pitch_hz", 901),
            ("cw_pitch_hz", 301),
            ("manual_notch_position", -128),
        ] {
            assert!(
                find(name)
                    .unwrap()
                    .write(&mut image, 0, &Value::Number(value))
                    .is_err()
            );
            assert_eq!(image.memory(), expected);
        }
    }

    #[test]
    fn keyboard_unknown_rescue_and_both_wrap_edges_are_distinct_from_display() {
        let mut image = image();
        let setting = find("A2-H54848-17").unwrap();
        assert_eq!(setting.read(&image, 0).unwrap(), Value::Number(231));
        setting.cycle(&mut image, 0, Step::Previous).unwrap();
        assert_eq!(image.memory()[0x39721], 0);
        setting.cycle(&mut image, 0, Step::Previous).unwrap();
        assert_eq!(image.memory()[0x39721], 11);
        setting.cycle(&mut image, 0, Step::Next).unwrap();
        assert_eq!(image.memory()[0x39721], 0);
        image.memory[0x39721] = 0xff;
        setting.cycle(&mut image, 0, Step::Next).unwrap();
        assert_eq!(image.memory()[0x39721], 0);
    }

    #[test]
    fn case_specific_notifications_cover_plain_extra_same_value_cancel_and_cycle() {
        let mut image = image();
        for setting in catalog()
            .iter()
            .filter(|setting| setting.kind == Kind::Selector)
        {
            let flags = if [
                "CL_Common_SysLang",
                "CL_Common_Wx_Alert",
                "CL_Common_RTTY_Tone",
                "CL_Common_RTTY_Shift",
                "CL_Common_Dimmer_AutoAdj",
                "CL_Common_Con_UsbSend",
                "CL_Common_Con_USB2Func",
                "CL_Common_Network_WLAN",
                "CL_Common_Network_ConnectionType",
                "CL_Common_Network_DHCP",
            ]
            .contains(&setting.name.as_ref())
            {
                0x20
            } else {
                0
            };
            if setting.key == 0x2e0 {
                image.memory[0x39313..0x39315].copy_from_slice(&[2, 0]);
            }
            image.memory[setting.offset] = 0;
            let event = Selection {
                key: setting.selection_key,
                phase: 2,
                candidate: 0,
                accepted: true,
            };
            let before = image.memory().to_vec();
            assert_eq!(
                setting.select(&mut image, 0, event).unwrap(),
                Effect {
                    stored: true,
                    notify_flags: flags
                }
            );
            assert_eq!(image.memory(), before);
            assert_eq!(
                setting
                    .select(
                        &mut image,
                        0,
                        Selection {
                            accepted: false,
                            ..event
                        }
                    )
                    .unwrap(),
                Effect {
                    stored: false,
                    notify_flags: 0
                }
            );
            assert_eq!(image.memory(), before);
            assert_eq!(
                setting.cycle(&mut image, 0, Step::Next).unwrap(),
                Effect {
                    stored: true,
                    notify_flags: flags
                }
            );
        }
    }

    #[test]
    fn typed_and_catalog_explicit_replacement_agree_without_touching_other_unknowns() {
        use super::super::{Language, ScopeMaxHold};
        fn check(name: &str, value: i64, edit: impl FnOnce(&mut Icf<'_>) -> Result<(), Error>) {
            let setting = find(name).unwrap();
            let mut typed = image();
            typed.memory[setting.offset] = if name == "manual_notch_position" {
                0
            } else {
                0xff
            };
            let mut catalog = typed.clone();
            edit(&mut typed).unwrap();
            setting
                .write(&mut catalog, 0, &Value::Number(value))
                .unwrap();
            assert_eq!(typed.memory(), catalog.memory(), "{name}");
            assert_eq!(typed.original_bytes(), catalog.original_bytes());
        }
        check("CL_Common_KeyTouchBeep", 0, |image| {
            image.set_key_touch_beep(false)
        });
        check("CL_Common_Bluetooth", 1, |image| image.set_bluetooth(true));
        check("CL_Common_SysLang", 1, |image| {
            image.set_system_language(Language::Japanese)
        });
        check("CL_Common_Scope_MaxHold", 0, |image| {
            image.set_scope_max_hold(ScopeMaxHold::Off)
        });
        check("speech_compression_level", 10, |image| {
            image.set_speech_compression_level(10)
        });
        check("manual_notch_position", -127, |image| {
            image.set_manual_notch_position(-127)
        });
    }

    #[test]
    fn integer_raw_local_preserves_sign_width_bias_separation_and_bypass_boundaries() {
        let mut image = image();
        for (name, at, old, read, new, bytes, after) in [
            (
                "noise_blanker_depth",
                0x399a2,
                &[0xfe][..],
                254,
                300,
                &[0x2c][..],
                44,
            ),
            (
                "manual_notch_position",
                0x399a6,
                &[0][..],
                0,
                -1,
                &[0xff][..],
                255,
            ),
            (
                "split_offset_khz",
                0x3930c,
                &[0xff, 0xff][..],
                -1,
                0x12345678,
                &[0x78, 0x56][..],
                22136,
            ),
            (
                "reference_frequency_adjustment",
                0x39330,
                &[0xff, 0xff][..],
                65535,
                i32::MIN,
                &[0, 0][..],
                0,
            ),
        ] {
            let setting = find(name).unwrap();
            image.memory[at..at + old.len()].copy_from_slice(old);
            assert_eq!(setting.read_numeric_raw(&image, 0).unwrap(), read);
            let mut expected = image.memory().to_vec();
            expected[at..at + bytes.len()].copy_from_slice(bytes);
            setting.store_numeric_raw(&mut image, 0, new).unwrap();
            assert_eq!(image.memory(), expected);
            assert_eq!(setting.read_numeric_raw(&image, 0).unwrap(), after);
            assert_eq!(
                setting.read(&image, 0).unwrap(),
                Value::Number(i64::from(after) + setting.bias)
            );
            assert!(
                setting
                    .store_numeric_raw(&mut image, usize::MAX, 0)
                    .is_err()
            );
            assert_eq!(image.memory(), expected);
        }
        let before = image.memory().to_vec();
        let selector = find("CL_Common_Bluetooth").unwrap();
        assert!(selector.read_numeric_raw(&image, 0).is_err());
        assert!(selector.store_numeric_raw(&mut image, 0, 1).is_err());
        assert_eq!(image.memory(), before);
        assert!(
            find("noise_blanker_depth")
                .unwrap()
                .write(&mut image, 0, &Value::Number(45))
                .is_err()
        );
        assert_eq!(image.memory(), before);
    }

    #[test]
    fn usb_send_clears_conflicts_keying_filter_keeps_current_and_gray_does_not_block_store() {
        let mut image = image();
        let send = find("A2-H56399-15").unwrap();
        let cw = find("A2-H56399-16").unwrap();
        let rtty = find("A2-H56399-17").unwrap();
        image.memory[0x39346..0x39349].copy_from_slice(&[0, 3, 4]);
        assert_eq!(
            send.select(
                &mut image,
                0,
                Selection {
                    key: 1,
                    phase: 2,
                    candidate: 3,
                    accepted: true
                }
            )
            .unwrap(),
            Effect {
                stored: true,
                notify_flags: 0x20
            }
        );
        assert_eq!(&image.memory()[0x39346..0x39349], &[3, 0, 4]);
        assert_eq!(cw.choices(&image, 0).unwrap(), [0, 1, 2, 4]);
        assert!(cw.write(&mut image, 0, &Value::Number(3)).is_err());
        image.memory[0x39347] = 3;
        assert_eq!(cw.choices(&image, 0).unwrap(), [0, 1, 2, 3, 4]);
        rtty.write(&mut image, 0, &Value::Number(4)).unwrap();
        cw.write(&mut image, 0, &Value::Number(4)).unwrap();
        assert_eq!(&image.memory()[0x39346..0x39349], &[3, 4, 4]);
        send.write(&mut image, 0, &Value::Number(0)).unwrap();
        assert_eq!(&image.memory()[0x39346..0x39349], &[0, 4, 4]);
        image.memory[0x39353] = 1;
        let gps = find("A2-H56399-25").unwrap();
        assert!(gps.control_state(&image, 0, false).unwrap().gray);
        gps.write(&mut image, 0, &Value::Number(1)).unwrap();
        assert_eq!(image.memory()[0x39355], 1);
        assert_eq!(image.memory()[0x39353], 1);
    }

    #[test]
    fn wlan_selection_key_and_password_length_mark_are_separate_from_twin_peak_guard() {
        let mut image = image();
        let wlan = find("A2-H56202-01").unwrap();
        assert_eq!(wlan.selection_key, 0x14);
        image.memory[0x39359] = 0;
        image.memory[0x39a05] = 1;
        image.memory[0x39a27..0x39a66].fill(b' ');
        image.memory[0x39a27..0x39a2e].copy_from_slice(b"1234567");
        assert!(
            !wlan
                .select(
                    &mut image,
                    0,
                    Selection {
                        key: 1,
                        phase: 2,
                        candidate: 1,
                        accepted: true
                    }
                )
                .unwrap()
                .stored
        );
        assert!(
            wlan.select(
                &mut image,
                0,
                Selection {
                    key: 0x14,
                    phase: 2,
                    candidate: 1,
                    accepted: true
                }
            )
            .unwrap()
            .stored
        );
        assert!(wlan.control_state(&image, 0, false).unwrap().marked);
        assert!(!wlan.control_state(&image, 0, true).unwrap().marked);
        image.memory[0x39a2e] = b'8';
        assert!(!wlan.control_state(&image, 0, false).unwrap().marked);
        let twin = find("A2-H56207-14").unwrap();
        for (mark, shift, enabled) in [(0, 0, false), (2, 1, false), (2, 0, true)] {
            image.memory[0x39313..0x39315].copy_from_slice(&[mark, shift]);
            let before = image.memory().to_vec();
            let effect = twin
                .select(
                    &mut image,
                    0,
                    Selection {
                        key: 1,
                        phase: 2,
                        candidate: 1,
                        accepted: true,
                    },
                )
                .unwrap();
            assert_eq!(
                effect,
                Effect {
                    stored: enabled,
                    notify_flags: 0
                }
            );
            if enabled {
                assert_eq!(image.memory()[0x399a0], 1);
            } else {
                assert_eq!(image.memory(), before);
            }
        }
    }
}
