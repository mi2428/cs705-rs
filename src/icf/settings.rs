//! Semantic setting catalog and the original simple-selector edit operation.
//!
//! The catalog is schema, not a factory image. Each entry names its original
//! help ID and resource key. This first family covers full-byte selectors with
//! no additional case-body guard or dependent correction; numeric, indexed,
//! text and coupled handlers must not be inferred from these simple entries.
//! Source: A4 `facts.tsv`, accepted BA04/BA14 normal group-25 Basic routes.

use std::sync::OnceLock;

use super::{Error, Icf};

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
#[derive(Debug, PartialEq, Eq)]
pub struct Setting {
    /// Stable ASCII source property name.
    pub name: &'static str,
    /// Original inventory help-feature ID; multiple controls can share one ID.
    pub feature_id: &'static str,
    /// Original resource/dispatch key, distinct from the feature ID.
    pub key: u16,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Contiguous raw choice codes; use `select` for the original simple handler.
    Selector,
    /// Integer in the explicitly named units, not a display percentage conversion.
    Integer(&'static str),
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
    /// Actual event key must match the target entry.
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
                assert!(cells.next().is_none());
                Setting {
                    feature_id,
                    name,
                    key,
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

    /// Read the actual stored code, including unknown values, without correction.
    pub fn read(&self, image: &Icf<'_>, index: usize) -> Result<Value, Error> {
        self.check_index(index)?;
        let at = self.offset + self.stride * index;
        let mut bytes = [0; 8];
        bytes[..self.width].copy_from_slice(&image.memory[at..at + self.width]);
        if self.signed && bytes[self.width - 1] & 0x80 != 0 {
            bytes[self.width..].fill(0xff);
        }
        Ok(Value::Number(i64::from_le_bytes(bytes) + self.bias))
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
        image.memory[at..at + self.width].copy_from_slice(&raw[..self.width]);
        Ok(())
    }

    /// Apply the original simple-selector accepted-event branch.
    ///
    /// Key mismatch, phase other than 2 or helper cancellation executes no store.
    /// Otherwise the low byte is written, even outside the displayed choices,
    /// and flag 0x20 is requested. No current-value guard or rollback is added.
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
        let stored = event.accepted && event.key == self.key && event.phase == 2;
        if stored {
            image.memory[self.offset] = event.candidate as u8;
        }
        Ok(Effect {
            stored,
            notify_flags: if stored { 0x20 } else { 0 },
        })
    }
}

fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 0, reason }
}

// Stored integer codecs reuse one width/sign/bias path. This is project schema,
// not a serialized default image. Native parsing, clamp and postlude remain
// separate operations; do not use `select` as a numeric-editor substitute.
macro_rules! integer {
    ($id:literal, $name:literal, $key:literal, $at:literal, $width:literal, $signed:literal,
     $bias:literal, $min:literal, $max:literal, $step:literal, $unit:literal) => {
        Setting {
            feature_id: $id,
            name: $name,
            key: $key,
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
            kind: Kind::Integer($unit),
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
            assert!(keys.insert(setting.key));
            assert_eq!(find(setting.name).unwrap(), setting);
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
            setting
                .write(&mut image, 0, &Value::Number(setting.domain.max))
                .unwrap();
            let raw = (setting.domain.max - setting.bias).to_le_bytes();
            expected[setting.offset..setting.offset + setting.width]
                .copy_from_slice(&raw[..setting.width]);
            assert_eq!(image.memory(), expected);
            assert_eq!(
                setting.read(&image, 0).unwrap(),
                Value::Number(setting.domain.max)
            );
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
            assert_eq!(setting.domain.max, max);
            for (phase, accepted) in [(1, true), (2, false)] {
                assert_eq!(
                    setting
                        .select(
                            &mut image,
                            0,
                            Selection {
                                key,
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
                            key,
                            phase: 2,
                            accepted: true,
                            candidate: 0x1fe
                        }
                    )
                    .unwrap(),
                Effect {
                    stored: true,
                    notify_flags: 0x20
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
}
