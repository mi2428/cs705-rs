//! Native GPS setting operations over the existing Setting storage codec.
//! GPS physical records and CSV parsing are separate callers and are not used.

use super::{
    Domain, Kind, Setting, Step, Value,
    extended::{ChoiceEffect, ChoiceInput},
};
use crate::icf::{Error, Icf};

fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 0, reason }
}

#[derive(Clone, Copy)]
pub struct Resources {
    pub system_langid: Option<u16>,
    pub language_descriptor: Option<u32>,
}

pub struct ChoiceField {
    pub setting: Setting,
    selector: &'static str,
    notify_flags: u32,
    gray_parent: usize,
}
const fn choice(
    id: &'static str,
    name: &'static str,
    key: u16,
    at: usize,
    range: (i64, i64),
    selector: &'static str,
    post: (u32, usize),
) -> ChoiceField {
    ChoiceField {
        setting: Setting {
            feature_id: id,
            name,
            key,
            selection_key: 1,
            domain: Domain {
                min: range.0,
                max: range.1,
                step: 1,
            },
            count: 1,
            kind: Kind::Selector,
            offset: at,
            width: 1,
            signed: false,
            bias: 0,
            stride: 0,
        },
        selector,
        notify_flags: post.0,
        gray_parent: post.1,
    }
}

static CHOICES: &[ChoiceField] = &[
    choice(
        "A2-H55075-01",
        "gps-select",
        0x1bc,
        0x399e3,
        (0, 2),
        "GI_Sheet_Sel_Common_GPS_Select",
        (0x20, 0),
    ),
    choice(
        "A2-H55075-05",
        "compass",
        0x1c5,
        0x399f6,
        (0, 2),
        "GI_Sheet_Sel_Common_Compass",
        (0, 0),
    ),
    choice(
        "A2-H55075-06",
        "sbas",
        0x1bd,
        0x399e4,
        (0, 1),
        "GI_Sheet_Sel_Misc_OnOff",
        (0, 0),
    ),
    choice(
        "A2-H55075-07",
        "glonass",
        0x1be,
        0x399e5,
        (0, 1),
        "GI_Sheet_Sel_Misc_OnOff",
        (0, 0),
    ),
    choice(
        "A2-H55075-08",
        "power-save",
        0x1bf,
        0x399e6,
        (0, 5),
        "GI_Sheet_Sel_Common_GPS_IntPSave",
        (0, 0),
    ),
    choice(
        "A2-H55075-09",
        "satellite-output",
        0x1c0,
        0x399e7,
        (0, 1),
        "GI_Sheet_Sel_Common_GPS_SateInfoOut",
        (0, 0),
    ),
    choice(
        "A2-H55075-11",
        "alarm-area-memory",
        0x1c8,
        0x399e8,
        (0, 2),
        "GI_Sheet_Sel_Common_POS_Area2",
        (0, 0),
    ),
    choice(
        "A2-H55075-15",
        "tx-mode",
        0x1cc,
        0x399e9,
        (0, 2),
        "GI_Sheet_Sel_Common_GpsTX",
        (0, 0),
    ),
    choice(
        "A2-H55075-16",
        "auto-tx",
        0x1cd,
        0x399f0,
        (0, 6),
        "GI_Sheet_Sel_Common_GpsAutoTX",
        (0, 0),
    ),
    choice(
        "A2-H55075-18",
        "tx-format",
        0x1cf,
        0x3c63c,
        (0, 3),
        "GI_Sheet_Sel_Common_DprsTxFmt",
        (0, 0),
    ),
    choice(
        "A2-H55075-19",
        "symbol-channel",
        0x1d0,
        0x3c676,
        (0, 3),
        "GI_Sheet_Sel_Common_GpsSymCh",
        (0, 0),
    ),
    choice(
        "A2-H55075-21",
        "position-ssid",
        0x1de,
        0x3c675,
        (0, 42),
        "GI_Sheet_Sel_SSID",
        (0, 0),
    ),
    choice(
        "A2-H55075-22",
        "comment-channel",
        0x1d9,
        0x3c67f,
        (0, 3),
        "GI_Sheet_Sel_Common_GpsCommentCh",
        (0, 0),
    ),
    choice(
        "A2-H55075-24",
        "position-timestamp",
        0x1df,
        0x3c731,
        (0, 2),
        "GI_Sheet_Sel_Common_Time_Stamp",
        (0, 0),
    ),
    choice(
        "A2-H55075-25",
        "position-altitude",
        0x1e0,
        0x3c732,
        (0, 1),
        "GI_Sheet_Sel_Misc_OnOff",
        (0x20, 0),
    ),
    choice(
        "A2-H55075-26",
        "position-extension",
        0x1e1,
        0x3c72c,
        (0, 2),
        "GI_Sheet_Sel_Common_Data_Extension",
        (0x20, 0),
    ),
    choice(
        "A2-H55075-27",
        "position-power",
        0x1e2,
        0x3c72d,
        (0, 9),
        "GI_Sheet_Sel_Common_PhgdPower",
        (0, 0x3c72c),
    ),
    choice(
        "A2-H55075-28",
        "position-height",
        0x1e3,
        0x3c72e,
        (0, 9),
        "height",
        (0, 0x3c72c),
    ),
    choice(
        "A2-H55075-29",
        "position-gain",
        0x1e4,
        0x3c72f,
        (0, 9),
        "GI_Sheet_Sel_Common_PhgdGain",
        (0, 0x3c72c),
    ),
    choice(
        "A2-H55075-30",
        "position-directivity",
        0x1e5,
        0x3c730,
        (0, 8),
        "directivity",
        (0, 0x3c72c),
    ),
    choice(
        "A2-H55075-32",
        "object-type",
        0x1e7,
        0x3c73c,
        (0, 1),
        "GI_Sheet_Sel_Common_ObjType",
        (0, 0),
    ),
    choice(
        "A2-H55075-38",
        "object-extension",
        0x1ee,
        0x3c77a,
        (0, 2),
        "GI_Sheet_Sel_Common_Data_Extension",
        (0x20, 0),
    ),
    choice(
        "A2-H55075-41",
        "object-power",
        0x1f1,
        0x3c77b,
        (0, 9),
        "GI_Sheet_Sel_Common_PhgdPower",
        (0, 0x3c77a),
    ),
    choice(
        "A2-H55075-42",
        "object-height",
        0x1f2,
        0x3c77c,
        (0, 9),
        "height",
        (0, 0x3c77a),
    ),
    choice(
        "A2-H55075-43",
        "object-gain",
        0x1f3,
        0x3c77d,
        (0, 9),
        "GI_Sheet_Sel_Common_PhgdGain",
        (0, 0x3c77a),
    ),
    choice(
        "A2-H55075-44",
        "object-directivity",
        0x1f4,
        0x3c77e,
        (0, 8),
        "directivity",
        (0, 0x3c77a),
    ),
    choice(
        "A2-H55075-45",
        "object-ssid",
        0x1f5,
        0x3c77f,
        (0, 42),
        "GI_Sheet_Sel_SSID",
        (0, 0),
    ),
    choice(
        "A2-H55075-46",
        "object-timestamp",
        0x1f6,
        0x3c780,
        (1, 2),
        "GI_Sheet_Sel_Common_ObjTimeStamp",
        (0, 0),
    ),
    choice(
        "A2-H55075-48",
        "item-type",
        0x1f8,
        0x3c78a,
        (0, 1),
        "GI_Sheet_Sel_Common_ItmType",
        (0, 0),
    ),
    choice(
        "A2-H55075-54",
        "item-extension",
        0x1ff,
        0x3c7c8,
        (0, 2),
        "GI_Sheet_Sel_Common_Data_Extension",
        (0x20, 0),
    ),
    choice(
        "A2-H55075-57",
        "item-power",
        0x202,
        0x3c7c9,
        (0, 9),
        "GI_Sheet_Sel_Common_PhgdPower",
        (0, 0x3c7c8),
    ),
    choice(
        "A2-H55075-58",
        "item-height",
        0x203,
        0x3c7ca,
        (0, 9),
        "height",
        (0, 0x3c7c8),
    ),
    choice(
        "A2-H55075-59",
        "item-gain",
        0x204,
        0x3c7cb,
        (0, 9),
        "GI_Sheet_Sel_Common_PhgdGain",
        (0, 0x3c7c8),
    ),
    choice(
        "A2-H55075-60",
        "item-directivity",
        0x205,
        0x3c7cc,
        (0, 8),
        "directivity",
        (0, 0x3c7c8),
    ),
    choice(
        "A2-H55075-61",
        "item-ssid",
        0x206,
        0x3c7cd,
        (0, 42),
        "GI_Sheet_Sel_SSID",
        (0, 0),
    ),
    choice(
        "A2-H55075-63",
        "weather-ssid",
        0x209,
        0x3c7cf,
        (0, 42),
        "GI_Sheet_Sel_SSID",
        (0, 0),
    ),
    choice(
        "A2-H55075-65",
        "weather-timestamp",
        0x20b,
        0x3c7fd,
        (0, 2),
        "GI_Sheet_Sel_Common_Time_Stamp",
        (0, 0),
    ),
    choice(
        "A2-H55075-68",
        "logger",
        0x20e,
        0x399f1,
        (0, 1),
        "GI_Sheet_Sel_Misc_OnOff",
        (0, 0),
    ),
    choice(
        "A2-H55075-69",
        "logger-interval",
        0x20f,
        0x399f2,
        (0, 6),
        "GI_Sheet_Sel_Common_Gps_Log_Interval",
        (0, 0),
    ),
];

pub fn choice_fields() -> &'static [ChoiceField] {
    CHOICES
}
pub fn find_choice(name_or_id: &str) -> Result<&'static ChoiceField, Error> {
    CHOICES
        .iter()
        .find(|f| f.setting.name == name_or_id || f.setting.feature_id == name_or_id)
        .ok_or_else(|| invalid("unknown GPS choice field"))
}
impl ChoiceField {
    pub fn selector(&self, image: &Icf<'_>, r: Resources) -> Result<&'static str, Error> {
        Ok(match self.selector {
            "height" => {
                if image.memory()[0x396b5] == 0 {
                    "GI_Sheet_Sel_Common_PhgdHeight_m"
                } else {
                    "GI_Sheet_Sel_Common_PhgdHeight_ft"
                }
            }
            "directivity" => {
                let descriptor = r
                    .language_descriptor
                    .ok_or_else(|| invalid("directivity requires actual VA8645FC"))?;
                let jp = descriptor == 1
                    || r.system_langid
                        .ok_or_else(|| invalid("directivity requires actual SYSTEM LANGID"))?
                        & 0x3ff
                        == 0x11;
                if jp {
                    "GI_Sheet_Sel_Common_PhgdDir_JPN"
                } else {
                    "GI_Sheet_Sel_Common_PhgdDir"
                }
            }
            selector => selector,
        })
    }
    /// `image` is this caller's actual setting target, including direct-current
    /// symbol-channel calls. This does not adopt another context image as current.
    pub fn edit(
        &self,
        image: &mut Icf<'_>,
        input: ChoiceInput,
        r: Resources,
    ) -> Result<ChoiceEffect, Error> {
        self.selector(image, r)?;
        let effect = match input {
            ChoiceInput::Selected(event) => self.setting.select(image, 0, event)?,
            ChoiceInput::Next => self.setting.cycle(image, 0, Step::Next)?,
            ChoiceInput::Previous => self.setting.cycle(image, 0, Step::Previous)?,
            _ => super::Effect {
                stored: false,
                notify_flags: 0,
            },
        };
        let Value::Number(value) = self.setting.read(image, 0)?;
        Ok(ChoiceEffect {
            stored: effect.stored,
            value,
            notify_flags: if effect.stored { self.notify_flags } else { 0 },
            gray: self.gray_parent != 0 && image.memory()[self.gray_parent] != 2,
            popup_requested: input == ChoiceInput::Begin,
            fixed_label: None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlarmField {
    Type,
    Group,
}
impl AlarmField {
    pub const ALL: [Self; 2] = [Self::Type, Self::Group];
    pub fn name(self) -> &'static str {
        if self == Self::Type {
            "alarm-type"
        } else {
            "alarm-group"
        }
    }
    pub fn feature_id(self) -> &'static str {
        if self == Self::Type {
            "A2-H55075-12"
        } else {
            "A2-H55075-13"
        }
    }
    pub fn key(self) -> u16 {
        if self == Self::Type { 0x1c9 } else { 0x1ca }
    }
    pub fn read(self, image: &Icf<'_>) -> u8 {
        image.memory()[if self == Self::Type { 0x3c622 } else { 0x3c623 }]
    }
    pub fn choices(self, image: &Icf<'_>) -> Vec<i64> {
        match self {
            Self::Type => (0..=3).collect(),
            Self::Group => match image.memory()[0x3c622] {
                2 => (0..=27).collect(),
                3 => (0..=26).collect(),
                _ => Vec::new(),
            },
        }
    }
    pub fn edit(self, image: &mut Icf<'_>, input: ChoiceInput) -> Result<ChoiceEffect, Error> {
        let at = if self == Self::Type { 0x3c622 } else { 0x3c623 };
        let old = image.memory()[at];
        let choices = self.choices(image);
        let candidate = if choices.is_empty() {
            None
        } else {
            super::extended::choice_candidate(i64::from(old), &choices, input)
        };
        let mut stored = false;
        let mut notify_flags = 0;
        if let Some(candidate) = candidate {
            match self {
                Self::Type if candidate != i32::from(old) => {
                    if candidate == 3 {
                        if let Some(group) =
                            (0..27).find(|&g| gps_word(image, 0x3c5dd + 2 * g) < 300)
                        {
                            image.patch_memory(&[(0x3c622, &[3, group as u8, 0, 0])])?;
                            stored = true;
                        }
                    } else {
                        image.patch_memory(&[(0x3c622, &[candidate as u8, 0, 0, 0])])?;
                        stored = true;
                    }
                    notify_flags = 0x20;
                }
                Self::Group if image.memory()[0x3c622] == 2 => {
                    image.patch_memory(&[(0x3c623, &[candidate as u8, 0, 0])])?;
                    stored = true;
                }
                Self::Group
                    if candidate != i32::from(old)
                        && gps_group_count(image, candidate as u8) != 0 =>
                {
                    image.patch_memory(&[(0x3c623, &[candidate as u8, 0, 0])])?;
                    stored = true;
                    notify_flags = 0x20;
                }
                _ => {}
            }
        }
        Ok(ChoiceEffect {
            stored,
            value: i64::from(image.memory()[at]),
            notify_flags,
            gray: false,
            popup_requested: !choices.is_empty() && input == ChoiceInput::Begin,
            fixed_label: None,
        })
    }
}
fn gps_word(image: &Icf<'_>, at: usize) -> u16 {
    u16::from_le_bytes(image.memory()[at..at + 2].try_into().unwrap())
}
/// Native C8100, unlike strict gps_groups: starts are not normalized or repaired.
fn gps_group_count(image: &Icf<'_>, group: u8) -> u16 {
    if group > 26 {
        return 0;
    }
    let start = gps_word(image, 0x3c5dd + 2 * usize::from(group));
    if start >= 300 {
        return 0;
    }
    if let Some(next) = (usize::from(group) + 1..27)
        .map(|g| gps_word(image, 0x3c5dd + 2 * g))
        .find(|&s| s < 300)
    {
        return next.wrapping_sub(start);
    }
    (usize::from(start)..300)
        .take_while(|&i| gps_word(image, 0x3c385 + 2 * i) < 300)
        .count() as u16
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NumberKind {
    Altitude,
    Course,
    Speed,
    AlarmRow,
}
pub struct NumberField {
    pub setting: Setting,
    kind: NumberKind,
}
const fn gps_number(
    id: &'static str,
    name: &'static str,
    key: u16,
    at: usize,
    kind: NumberKind,
) -> NumberField {
    NumberField {
        setting: Setting {
            feature_id: id,
            name,
            key,
            selection_key: 1,
            domain: Domain {
                min: i32::MIN as i64,
                max: i32::MAX as i64,
                step: 1,
            },
            count: 1,
            kind: Kind::Integer("native GPS wrapper"),
            offset: at,
            width: if matches!(kind, NumberKind::Altitude) {
                4
            } else {
                2
            },
            signed: matches!(kind, NumberKind::Altitude),
            bias: 0,
            stride: 0,
        },
        kind,
    }
}
static NUMBERS: &[NumberField] = &[
    gps_number(
        "A2-H55075-04",
        "manual-altitude",
        0x1c4,
        0x3c61b,
        NumberKind::Altitude,
    ),
    gps_number(
        "A2-H55075-14",
        "alarm-memory",
        0x1cb,
        0x3c624,
        NumberKind::AlarmRow,
    ),
    gps_number(
        "A2-H55075-37",
        "object-altitude",
        0x1ed,
        0x3c772,
        NumberKind::Altitude,
    ),
    gps_number(
        "A2-H55075-39",
        "object-course",
        0x1ef,
        0x3c776,
        NumberKind::Course,
    ),
    gps_number(
        "A2-H55075-40",
        "object-speed",
        0x1f0,
        0x3c778,
        NumberKind::Speed,
    ),
    gps_number(
        "A2-H55075-53",
        "item-altitude",
        0x1fe,
        0x3c7c0,
        NumberKind::Altitude,
    ),
    gps_number(
        "A2-H55075-55",
        "item-course",
        0x200,
        0x3c7c4,
        NumberKind::Course,
    ),
    gps_number(
        "A2-H55075-56",
        "item-speed",
        0x201,
        0x3c7c6,
        NumberKind::Speed,
    ),
];
pub fn number_fields() -> &'static [NumberField] {
    NUMBERS
}
/// D5E9C/D5EC8's signed low32 wrappers, not the CSV altitude parser.
fn altitude_unit(value: i32, to_meters: bool) -> i32 {
    let n = value.unsigned_abs();
    let (q, r) = (n / 10000, n % 10000);
    let converted = if to_meters {
        q.wrapping_mul(3048).wrapping_add((r * 3048 + 5000) / 10000)
    } else {
        let a = q.wrapping_mul(8399);
        q.wrapping_mul(32800)
            .wrapping_add(a / 1000)
            .wrapping_add((r * 32800 + (r * 8399 + 500) / 1000 + (a % 1000) * 10 + 5000) / 10000)
    };
    if value < 0 {
        converted.wrapping_neg() as i32
    } else {
        converted as i32
    }
}
/// D501C's native sentinel and wrapped integer ratio, no new floating rounding.
fn speed_unit(value: u32, from: u8, to: u8) -> u32 {
    if value == 0 || value == 65535 || from == to {
        value
    } else {
        value.wrapping_mul([1000, 1852, 1609][usize::from(from)])
            / [1000, 1852, 1609][usize::from(to)]
    }
}
impl NumberField {
    pub fn bounds(&self, image: &Icf<'_>) -> (i64, i64) {
        match self.kind {
            NumberKind::Altitude => {
                if image.memory()[0x396b5] == 0 {
                    (-10000, 10000)
                } else {
                    (-32808, 32808)
                }
            }
            NumberKind::Course => (0, 360),
            NumberKind::AlarmRow => (0, 299),
            NumberKind::Speed => (
                0,
                match image.memory()[0x396b6] {
                    1 => 1150,
                    2 => 999,
                    _ => 1850,
                },
            ),
        }
    }
    /// unit_output is actual native degree/unit text. Missing output fails before
    /// the wrapper's final output copy, including its false-helper display path.
    pub fn edit(
        &self,
        image: &mut Icf<'_>,
        input: super::extended::NumberInput<'_>,
        point: u8,
        unit_output: Option<&[u8]>,
    ) -> Result<super::extended::NumberEffect, Error> {
        use super::extended::{NumberEffect, NumberInput};
        let Value::Number(old) = self.setting.read(image, 0)?;
        let (min, max) = self.bounds(image);
        let meter = image.memory()[0x396b5] == 0;
        let unit = image.memory()[0x396b6];
        let speed = match unit {
            1 => 2,
            2 => 1,
            _ => 0,
        };
        let missing = self.kind == NumberKind::Altitude && old == i64::from(i32::MAX);
        let local = match self.kind {
            NumberKind::Altitude if !missing => {
                let n = if meter {
                    altitude_unit(old as i32, true)
                } else {
                    old as i32
                };
                n.wrapping_add(if n > 0 { 5 } else { -5 }) / 10
            }
            NumberKind::Speed => (speed_unit(old as u32, 1, speed) as i32).wrapping_add(5) / 10,
            _ => old as i32,
        };
        let active = self.kind != NumberKind::AlarmRow || image.memory()[0x3c622] == 3;
        let mut state_out = if active && input == NumberInput::Begin {
            2
        } else {
            0
        };
        let (mut candidate, accepted) = if !active {
            (local, false)
        } else {
            match input {
                NumberInput::Text(text) => {
                    if !text.is_ascii() {
                        return Err(invalid(
                            "GPS numeric ctype for non-ASCII requires actual native output",
                        ));
                    }
                    if self.kind == NumberKind::Altitude
                        && !text.iter().take_while(|&&b| b != 0).any(u8::is_ascii_digit)
                    {
                        (i32::MAX, true)
                    } else {
                        let (n, accepted) = super::extended::numeric_value(text, point, 0, 1)?;
                        (n.clamp(min, max) as i32, accepted)
                    }
                }
                NumberInput::Next | NumberInput::Previous if missing => {
                    state_out = 2;
                    (local, false)
                }
                NumberInput::Next | NumberInput::Previous => {
                    let n = local.wrapping_add(if input == NumberInput::Next { 1 } else { -1 });
                    (
                        if i64::from(n) < min {
                            max as i32
                        } else if i64::from(n) > max {
                            min as i32
                        } else {
                            n
                        },
                        true,
                    )
                }
                _ => (local, false),
            }
        };
        if self.kind == NumberKind::Speed {
            candidate = speed_unit(candidate.wrapping_mul(10) as u32, speed, 1) as i32;
        }
        if self.kind == NumberKind::Altitude && accepted && candidate != i32::MAX {
            candidate = candidate.wrapping_mul(10);
            if meter {
                candidate = altitude_unit(candidate, false);
            }
        }
        let suffix = if self.kind == NumberKind::AlarmRow || !active {
            Vec::new()
        } else {
            let output = unit_output
                .ok_or_else(|| invalid("actual native GPS unit/degree output unavailable"))?;
            if state_out == 2 {
                Vec::new()
            } else {
                output.to_vec()
            }
        };
        let stored = accepted
            && (self.kind != NumberKind::AlarmRow
                || i32::from(gps_group_count(image, image.memory()[0x3c623])) > candidate);
        if stored {
            self.setting.store_numeric_raw(image, 0, candidate)?;
        }
        let gray = match self.setting.key {
            0x1c4 => image.memory()[0x399e3] != 2,
            0x1ef | 0x1f0 => image.memory()[0x3c77a] != 1,
            0x200 | 0x201 => image.memory()[0x3c7c8] != 1,
            _ => false,
        };
        Ok(NumberEffect {
            stored,
            accepted,
            candidate: i64::from(candidate),
            raw_candidate: candidate,
            state_out,
            notify_flags: if stored && matches!(self.setting.key, 0x1ed | 0x1fe) {
                0x20
            } else {
                0
            },
            suffix,
            gray,
        })
    }
}

/// The actual148-byte native lookup resource, not an approved storage packet.
pub struct NativeSymbolPairs<'a> {
    pairs: &'a [[u8; 2]; 74],
}
impl<'a> NativeSymbolPairs<'a> {
    pub fn read(bytes: &'a [u8]) -> Result<Self, Error> {
        let (pairs, tail) = bytes.as_chunks::<2>();
        if !tail.is_empty() {
            return Err(invalid("native symbol resource requires74pairs"));
        }
        Ok(Self {
            pairs: pairs
                .try_into()
                .map_err(|_| invalid("native symbol resource requires74pairs"))?,
        })
    }
    fn index(&self, mut raw: [u8; 2]) -> i64 {
        if raw != *b"IY" && (raw[0].is_ascii_digit() || raw[0].is_ascii_uppercase()) {
            raw[0] = b'\\';
        }
        self.pairs
            .iter()
            .position(|&p| p == raw)
            .map_or(74, |i| i as i64)
    }
}
pub struct SymbolField {
    pub feature_id: &'static str,
    pub name: &'static str,
    pub count: usize,
    pub key: u16,
    at: usize,
}
static SYMBOLS: &[SymbolField] = &[
    SymbolField {
        feature_id: "A2-H55075-20",
        name: "position-symbol",
        count: 4,
        key: 0x1d1,
        at: 0x3c677,
    },
    SymbolField {
        feature_id: "A2-H55075-33",
        name: "object-symbol",
        count: 1,
        key: 0x1e8,
        at: 0x3c73d,
    },
    SymbolField {
        feature_id: "A2-H55075-49",
        name: "item-symbol",
        count: 1,
        key: 0x1f9,
        at: 0x3c78b,
    },
    SymbolField {
        feature_id: "A2-H55075-62",
        name: "weather-symbol",
        count: 1,
        key: 0x207,
        at: 0x3c7d0,
    },
];
pub fn symbol_fields() -> &'static [SymbolField] {
    SYMBOLS
}
pub fn find_symbol(name_or_id: &str) -> Result<&'static SymbolField, Error> {
    SYMBOLS
        .iter()
        .find(|f| f.name == name_or_id || f.feature_id == name_or_id)
        .ok_or_else(|| invalid("unknown GPS symbol field"))
}
impl SymbolField {
    fn address(&self, index: usize) -> Result<usize, Error> {
        if index >= self.count {
            return Err(invalid("GPS symbol index outside native axis"));
        }
        Ok(self.at + 2 * index)
    }
    pub fn read(&self, image: &Icf<'_>, index: usize) -> Result<[u8; 2], Error> {
        let at = self.address(index)?;
        Ok(image.memory()[at..at + 2].try_into().unwrap())
    }
    pub fn select(
        &self,
        image: &mut Icf<'_>,
        index: usize,
        input: ChoiceInput,
        resource: &NativeSymbolPairs<'_>,
    ) -> Result<ChoiceEffect, Error> {
        let at = self.address(index)?;
        let old = resource.index(self.read(image, index)?);
        let choices: Vec<_> = (0..=74).collect();
        let candidate = super::extended::choice_candidate(old, &choices, input);
        let pair = candidate.and_then(|n| {
            if n < 74 {
                resource.pairs.get(usize::from(n as u8)).copied()
            } else if n == 74 {
                Some(*b"/\"")
            } else {
                None
            }
        });
        if let Some(pair) = pair {
            image.patch_memory(&[(at, &pair)])?;
        }
        Ok(ChoiceEffect {
            stored: pair.is_some(),
            value: candidate.map_or(old, i64::from),
            notify_flags: if candidate.is_some() { 0x20 } else { 0 },
            gray: false,
            popup_requested: input == ChoiceInput::Begin,
            fixed_label: None,
        })
    }
    pub fn edit(
        &self,
        image: &mut Icf<'_>,
        index: usize,
        input: super::extended::FixedTextInput<'_>,
    ) -> Result<super::extended::TextEffect, crate::csv::Error> {
        self.edit_with_nls(image, index, input, crate::csv::nls::ascii_map)
    }
    pub fn edit_with_nls(
        &self,
        image: &mut Icf<'_>,
        index: usize,
        input: super::extended::FixedTextInput<'_>,
        map: impl FnMut(&[u8], &mut [u8]) -> Result<i32, crate::csv::Error>,
    ) -> Result<super::extended::TextEffect, crate::csv::Error> {
        use super::extended::{FixedTextInput, TextEffect};
        use crate::csv::{Error, nls};
        let at = self
            .address(index)
            .map_err(|_| Error::Invalid("GPS symbol index outside native axis"))?;
        let mut stored = false;
        let state_out = if matches!(input, FixedTextInput::Begin) {
            2
        } else {
            0
        };
        if let FixedTextInput::Text { source, phase } = input {
            if !matches!(phase, 1 | 2) {
                return Err(Error::Invalid("GPS symbol text phase must be1or2"));
            }
            let candidate = nls::whole_text(
                source,
                nls::Profile {
                    uppercase: false,
                    keep_kana: false,
                    keep_spaces: false,
                    punctuation_exceptions: b"!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~",
                    maximum: 2,
                },
                map,
                |_| unreachable!("symbol profile has no additional uppercase"),
            )?;
            if candidate.len() == 2 {
                let first = candidate[0].to_ascii_uppercase();
                if (first.is_ascii_alphanumeric() || matches!(first, b'/' | b'\\'))
                    && (0x21..=0x7e).contains(&candidate[1])
                {
                    image
                        .patch_memory(&[(at, &[first, candidate[1]])])
                        .map_err(|_| Error::Operation("GPS symbol direct store"))?;
                    stored = true;
                }
            }
        }
        Ok(TextEffect {
            stored,
            state_out,
            notify_flags: if stored { 0x20 } else { 0 },
            display: image.memory()[at..at + 2].to_vec(),
        })
    }
}

pub struct CoordinateField {
    pub feature_id: &'static str,
    pub name: &'static str,
    pub key: u16,
    pub longitude: bool,
    pub area: bool,
    at: usize,
    manual: bool,
}
pub static ALARM_AREA: CoordinateField = CoordinateField {
    feature_id: "A2-H55075-10",
    name: "alarm-area-group",
    key: 0x1c7,
    longitude: false,
    area: true,
    at: 0x3c626,
    manual: false,
};
static COORDINATES: &[CoordinateField] = &[
    CoordinateField {
        feature_id: "A2-H55075-02",
        name: "manual-latitude",
        key: 0x1c2,
        longitude: false,
        area: false,
        at: 0x3c613,
        manual: true,
    },
    CoordinateField {
        feature_id: "A2-H55075-03",
        name: "manual-longitude",
        key: 0x1c3,
        longitude: true,
        area: false,
        at: 0x3c617,
        manual: true,
    },
    CoordinateField {
        feature_id: "A2-H55075-35",
        name: "object-latitude",
        key: 0x1eb,
        longitude: false,
        area: false,
        at: 0x3c76a,
        manual: false,
    },
    CoordinateField {
        feature_id: "A2-H55075-36",
        name: "object-longitude",
        key: 0x1ec,
        longitude: true,
        area: false,
        at: 0x3c76e,
        manual: false,
    },
    CoordinateField {
        feature_id: "A2-H55075-51",
        name: "item-latitude",
        key: 0x1fc,
        longitude: false,
        area: false,
        at: 0x3c7b8,
        manual: false,
    },
    CoordinateField {
        feature_id: "A2-H55075-52",
        name: "item-longitude",
        key: 0x1fd,
        longitude: true,
        area: false,
        at: 0x3c7bc,
        manual: false,
    },
];
pub fn coordinate_fields() -> &'static [CoordinateField] {
    COORDINATES
}
pub fn find_coordinate(name_or_id: &str) -> Result<&'static CoordinateField, Error> {
    COORDINATES
        .iter()
        .chain(std::iter::once(&ALARM_AREA))
        .find(|f| f.name == name_or_id || f.feature_id == name_or_id)
        .ok_or_else(|| invalid("unknown GPS coordinate field"))
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateEffect {
    pub stored: bool,
    pub raw: [u8; 4],
    pub gray: bool,
}
impl CoordinateField {
    pub fn read(&self, image: &Icf<'_>) -> [u8; 4] {
        if self.area {
            let [high, low] = gps_word(image, self.at).to_be_bytes();
            [0, high, low, 0]
        } else {
            image.memory()[self.at..self.at + 4].try_into().unwrap()
        }
    }
    pub fn dialog(
        &self,
        image: &Icf<'_>,
        current: &[u8],
        first: Option<u8>,
    ) -> Result<crate::csv::number::CoordinateDialog, crate::csv::Error> {
        if self.area {
            crate::csv::number::CoordinateDialog::area(self.read(image), current, first)
        } else {
            crate::csv::number::CoordinateDialog::new(
                self.read(image),
                self.longitude,
                current,
                first,
            )
        }
    }
    /// Coordinates mutate I directly, retaining degree on a later error. Area
    /// parses a temporary first and packs the word only after normal return.
    pub fn phase2(
        &self,
        image: &mut Icf<'_>,
        source: &[u8],
        current: &[u8],
    ) -> Result<CoordinateEffect, crate::csv::Error> {
        if self.area {
            let mut temporary = self.read(image);
            let mode = crate::csv::number::area_native_mode(current)?;
            crate::csv::number::area_native_phase2(&mut temporary, source, mode)?;
            let word = u16::from_be_bytes([temporary[1], temporary[2]]);
            image
                .patch_memory(&[(self.at, &word.to_le_bytes())])
                .map_err(|_| crate::csv::Error::Operation("GPS area word store"))?;
        } else {
            crate::csv::number::coordinate_native_phase2(
                (&mut image.memory[self.at..self.at + 4])
                    .try_into()
                    .unwrap(),
                source,
                current,
            )?;
        }
        Ok(CoordinateEffect {
            stored: true,
            raw: self.read(image),
            gray: self.manual && image.memory()[0x399e3] != 2,
        })
    }
    /// Dispatch only the shared mask/validator producer's real phase2 text.
    pub fn apply_dialog(
        &self,
        image: &mut Icf<'_>,
        dialog: &mut crate::csv::number::CoordinateDialog,
        event: crate::csv::number::CoordinateEvent,
        current: &[u8],
    ) -> Result<CoordinateEffect, crate::csv::Error> {
        if let Some(source) = dialog.event(event) {
            let effect = self.phase2(image, &source, current)?;
            dialog.dispatched();
            Ok(effect)
        } else {
            Ok(CoordinateEffect {
                stored: false,
                raw: self.read(image),
                gray: self.manual && image.memory()[0x399e3] != 2,
            })
        }
    }
}

pub struct TextField {
    pub feature_id: &'static str,
    pub name: &'static str,
    pub key: u16,
    pub count: usize,
    pub width: usize,
    at: usize,
    keep_kana: bool,
    extension: usize,
    altitude: usize,
}
static TEXT: &[TextField] = &[
    TextField {
        feature_id: "A2-H55075-17",
        name: "unproto-address",
        key: 0x1ce,
        count: 1,
        width: 56,
        at: 0x3c63d,
        keep_kana: true,
        extension: 0,
        altitude: 0,
    },
    TextField {
        feature_id: "A2-H55075-23",
        name: "position-comment",
        key: 0x1da,
        count: 4,
        width: 43,
        at: 0x3c680,
        keep_kana: true,
        extension: 0x3c72c,
        altitude: 0x3c732,
    },
    TextField {
        feature_id: "A2-H55075-31",
        name: "object-name",
        key: 0x1e6,
        count: 1,
        width: 9,
        at: 0x3c733,
        keep_kana: false,
        extension: 0,
        altitude: 0,
    },
    TextField {
        feature_id: "A2-H55075-34",
        name: "object-comment",
        key: 0x1ea,
        count: 1,
        width: 43,
        at: 0x3c73f,
        keep_kana: true,
        extension: 0x3c77a,
        altitude: 0x3c772,
    },
    TextField {
        feature_id: "A2-H55075-47",
        name: "item-name",
        key: 0x1f7,
        count: 1,
        width: 9,
        at: 0x3c781,
        keep_kana: false,
        extension: 0,
        altitude: 0,
    },
    TextField {
        feature_id: "A2-H55075-50",
        name: "item-comment",
        key: 0x1fb,
        count: 1,
        width: 43,
        at: 0x3c78d,
        keep_kana: true,
        extension: 0x3c7c8,
        altitude: 0x3c7c0,
    },
    TextField {
        feature_id: "A2-H55075-64",
        name: "weather-comment",
        key: 0x20a,
        count: 1,
        width: 43,
        at: 0x3c7d2,
        keep_kana: true,
        extension: 0,
        altitude: 0,
    },
    TextField {
        feature_id: "A2-H55075-67",
        name: "gps-message",
        key: 0x20d,
        count: 1,
        width: 20,
        at: 0x3c628,
        keep_kana: true,
        extension: 0,
        altitude: 0,
    },
];
pub fn text_fields() -> &'static [TextField] {
    TEXT
}
pub fn find_text(name_or_id: &str) -> Result<&'static TextField, Error> {
    TEXT.iter()
        .find(|f| f.name == name_or_id || f.feature_id == name_or_id)
        .ok_or_else(|| invalid("unknown GPS text field"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentenceField {
    Nmea,
    Logger,
}
impl SentenceField {
    pub const ALL: [Self; 2] = [Self::Nmea, Self::Logger];
    pub fn key(self) -> u16 {
        if self == Self::Nmea { 0x20c } else { 0x210 }
    }
    pub fn feature_id(self) -> &'static str {
        match self {
            Self::Nmea => "A2-H55075-66",
            Self::Logger => "A2-H55075-70",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Nmea => "nmea-sentences",
            Self::Logger => "logger-sentences",
        }
    }
    pub fn count(self) -> usize {
        if self == Self::Nmea { 6 } else { 4 }
    }
    fn read(self, image: &Icf<'_>) -> u8 {
        let count = self.count();
        let mut selected = 0;
        for index in 0..count {
            let value = if self == Self::Nmea {
                image.memory()[0x399ea + index] & 1 != 0
            } else {
                image.memory()[0x399f3] & (0x80 >> index) != 0
            };
            if value {
                selected |= 1 << index;
            }
        }
        selected
    }
}
pub fn find_sentence(name_or_id: &str) -> Result<SentenceField, Error> {
    SentenceField::ALL
        .into_iter()
        .find(|f| f.name() == name_or_id || f.feature_id() == name_or_id)
        .ok_or_else(|| invalid("unknown GPS sentence field"))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentenceEvent {
    Choose { index: usize, selected: bool },
    Ok,
    Cancel,
    Close,
    Deactivate,
    OwnerDestroyed,
}
pub struct SentenceDialog {
    field: SentenceField,
    selected: u8,
    pub cancelled: bool,
    pub done: bool,
    pub release_requested: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SentenceEffect {
    pub stored: bool,
    pub helper_changed: bool,
    pub valid: bool,
    pub selected: u8,
}
impl SentenceDialog {
    pub fn new(image: &Icf<'_>, field: SentenceField) -> Self {
        Self {
            field,
            selected: field.read(image),
            cancelled: false,
            done: false,
            release_requested: false,
        }
    }
    pub fn selected(&self) -> u8 {
        self.selected
    }
    pub fn valid(&self) -> bool {
        let n = self.selected.count_ones();
        n != 0 && (self.field == SentenceField::Logger || n <= 4)
    }
    pub fn apply(
        &mut self,
        image: &mut Icf<'_>,
        event: SentenceEvent,
    ) -> Result<SentenceEffect, Error> {
        let mut stored = false;
        let mut helper_changed = false;
        match event {
            SentenceEvent::Choose { index, selected } => {
                if !(1..=self.field.count()).contains(&index) {
                    return Err(invalid("GPS sentence native index outside dialog"));
                }
                if selected {
                    self.selected |= 1 << (index - 1);
                } else {
                    self.selected &= !(1 << (index - 1));
                }
            }
            SentenceEvent::Cancel | SentenceEvent::OwnerDestroyed => {
                self.cancelled = true;
                self.release_requested = true;
            }
            SentenceEvent::Close => self.release_requested = true,
            SentenceEvent::Ok | SentenceEvent::Deactivate => {
                if !self.done && (event == SentenceEvent::Ok || !self.cancelled) {
                    helper_changed = true;
                    if self.field == SentenceField::Nmea {
                        if self.valid() && self.selected != self.field.read(image) {
                            let bytes: Vec<_> =
                                (0..6).map(|index| (self.selected >> index) & 1).collect();
                            image.patch_memory(&[(0x399ea, &bytes)])?;
                            stored = true;
                        }
                    } else if self.selected != 0 {
                        let value = (0..4).fold(0, |value, index| {
                            value | (((self.selected >> index) & 1) << (7 - index))
                        });
                        image.patch_memory(&[(0x399f3, &[value])])?;
                        stored = true;
                    }
                    self.done = true;
                    self.release_requested = true;
                }
                if event == SentenceEvent::Deactivate {
                    self.release_requested = true;
                }
            }
        }
        Ok(SentenceEffect {
            stored,
            helper_changed,
            valid: self.valid(),
            selected: self.selected,
        })
    }
}
impl TextField {
    fn address(&self, index: usize) -> Result<usize, Error> {
        if index >= self.count {
            return Err(invalid("GPS text index outside native axis"));
        }
        Ok(self.at + self.width * index)
    }
    pub fn read(&self, image: &Icf<'_>, index: usize) -> Result<Vec<u8>, Error> {
        let at = self.address(index)?;
        Ok(image.memory()[at..at + self.width]
            .split(|&b| b == 0)
            .next()
            .unwrap()
            .to_vec())
    }
    /// Rendering budget only, never a new input maximum or storage truncation.
    pub fn budget(&self, image: &Icf<'_>) -> usize {
        if self.extension == 0 {
            return self.width;
        }
        let extension = image.memory()[self.extension] != 0;
        let altitude = if self.altitude == 0x3c732 {
            image.memory()[self.altitude] != 0
        } else {
            image.memory()[self.altitude..self.altitude + 4] != i32::MAX.to_le_bytes()
        };
        43 - 7 * usize::from(extension) - 8 * usize::from(altitude)
    }
    pub fn edit(
        &self,
        image: &mut Icf<'_>,
        index: usize,
        input: super::extended::FixedTextInput<'_>,
    ) -> Result<super::extended::TextEffect, crate::csv::Error> {
        self.edit_with_nls(image, index, input, crate::csv::nls::ascii_map)
    }
    pub fn edit_with_nls(
        &self,
        image: &mut Icf<'_>,
        index: usize,
        input: super::extended::FixedTextInput<'_>,
        map: impl FnMut(&[u8], &mut [u8]) -> Result<i32, crate::csv::Error>,
    ) -> Result<super::extended::TextEffect, crate::csv::Error> {
        use crate::csv::{self, nls};
        let at = self
            .address(index)
            .map_err(|_| csv::Error::Invalid("GPS text index outside native axis"))?;
        let mut stored = false;
        let state_out = if matches!(input, super::extended::FixedTextInput::Begin) {
            2
        } else {
            0
        };
        if let super::extended::FixedTextInput::Text { source, phase } = input {
            if !matches!(phase, 1 | 2) {
                return Err(csv::Error::Invalid("GPS fixed text phase must be1or2"));
            }
            let profile = nls::Profile {
                uppercase: false,
                keep_kana: self.keep_kana,
                keep_spaces: true,
                punctuation_exceptions: b"!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~",
                maximum: self.width,
            };
            let candidate = nls::whole_text(source, profile, map, |_| {
                unreachable!("GPS1111 has no additional uppercase stage")
            })?;
            let mut bytes = vec![b' '; self.width];
            bytes[..candidate.len()].copy_from_slice(&candidate);
            image
                .patch_memory(&[(at, &bytes)])
                .map_err(|_| csv::Error::Operation("GPS fixed text store"))?;
            stored = true;
        }
        Ok(super::extended::TextEffect {
            stored,
            state_out,
            notify_flags: 0,
            display: self
                .read(image, index)
                .map_err(|_| csv::Error::Operation("GPS fixed text read"))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icf::settings::Selection;
    fn image() -> Icf<'static> {
        let mut bytes = vec![0xa5; 0x3c9c0];
        bytes[0x355a1] = 6;
        bytes[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        Icf::from_memory(bytes).unwrap()
    }
    fn resources() -> Resources {
        Resources {
            system_langid: Some(0x409),
            language_descriptor: Some(0),
        }
    }
    #[test]
    fn every_choice_runs_cancel_begin_raw_selection_unknown_replacement_and_native_domains() {
        assert_eq!(choice_fields().len(), 39);
        for field in choice_fields() {
            let mut image = image();
            let before = image.memory().to_vec();
            assert!(
                !field
                    .edit(&mut image, ChoiceInput::Cancel, resources())
                    .unwrap()
                    .stored
            );
            assert_eq!(image.memory(), before);
            assert!(
                field
                    .edit(&mut image, ChoiceInput::Begin, resources())
                    .unwrap()
                    .popup_requested
            );
            assert_eq!(image.memory(), before);
            let effect = field
                .edit(
                    &mut image,
                    ChoiceInput::Selected(Selection {
                        key: 1,
                        phase: 2,
                        candidate: -1,
                        accepted: true,
                    }),
                    resources(),
                )
                .unwrap();
            let mut expected = before;
            expected[field.setting.offset] = 255;
            assert_eq!(image.memory(), expected);
            assert_eq!(effect.value, 255);
            assert_eq!(effect.notify_flags, field.notify_flags);
            let effect = field
                .edit(&mut image, ChoiceInput::Next, resources())
                .unwrap();
            assert_eq!(effect.value, field.setting.domain.min);
            let effect = field
                .edit(&mut image, ChoiceInput::Previous, resources())
                .unwrap();
            assert_eq!(effect.value, field.setting.domain.max);
        }
    }
    #[test]
    fn ssid_timestamp_height_locale_and_gray_have_literal_boundaries() {
        let mut image = image();
        let before = image.memory().to_vec();
        assert_eq!(
            find_choice("object-timestamp")
                .unwrap()
                .setting
                .choices(&image, 0)
                .unwrap(),
            vec![1, 2]
        );
        assert_eq!(
            find_choice("weather-ssid")
                .unwrap()
                .setting
                .choices(&image, 0)
                .unwrap(),
            (0..43).collect::<Vec<_>>()
        );
        let height = find_choice("position-height").unwrap();
        assert!(
            height
                .selector(&image, resources())
                .unwrap()
                .ends_with("_ft")
        );
        let effect = height
            .edit(
                &mut image,
                ChoiceInput::Selected(Selection {
                    key: 1,
                    phase: 2,
                    candidate: 8,
                    accepted: true,
                }),
                resources(),
            )
            .unwrap();
        assert!(effect.gray && effect.stored);
        let mut expected = before;
        expected[0x3c72e] = 8;
        assert_eq!(image.memory(), expected);
        image.patch_memory(&[(0x396b5, &[0])]).unwrap();
        assert!(
            height
                .selector(&image, resources())
                .unwrap()
                .ends_with("_m")
        );
        assert_eq!(height.setting.read(&image, 0).unwrap(), Value::Number(8));
        let directivity = find_choice("item-directivity").unwrap();
        let before = image.memory().to_vec();
        assert!(
            directivity
                .edit(
                    &mut image,
                    ChoiceInput::Cancel,
                    Resources {
                        system_langid: None,
                        language_descriptor: None
                    }
                )
                .is_err()
        );
        assert_eq!(image.memory(), before);
        assert!(
            directivity
                .selector(
                    &image,
                    Resources {
                        system_langid: None,
                        language_descriptor: Some(1)
                    }
                )
                .unwrap()
                .ends_with("_JPN")
        );
    }

    #[test]
    fn gps_text_profiles_keep_all_native_rows_budget_and_nls_source_prefix() {
        use super::super::extended::FixedTextInput;
        for field in text_fields() {
            for index in 0..field.count {
                let mut image = image();
                let before = image.memory().to_vec();
                assert!(
                    !field
                        .edit(&mut image, index, FixedTextInput::Cancel)
                        .unwrap()
                        .stored
                );
                assert_eq!(image.memory(), before);
                let mut source = b"a ! /b ".to_vec();
                assert!(
                    field
                        .edit(
                            &mut image,
                            index,
                            FixedTextInput::Text {
                                source: &mut source,
                                phase: 1
                            }
                        )
                        .unwrap()
                        .stored
                );
                let mut expected = before;
                let at = field.at + field.width * index;
                expected[at..at + field.width].fill(b' ');
                expected[at..at + 6].copy_from_slice(b"a ! /b");
                assert_eq!(image.memory(), expected);
                let before = image.memory().to_vec();
                assert!(
                    field
                        .edit(&mut image, field.count, FixedTextInput::Cancel)
                        .is_err()
                );
                assert_eq!(image.memory(), before);
            }
        }
        let field = find_text("position-comment").unwrap();
        let mut image = image();
        assert_eq!(field.budget(&image), 28);
        let mut source = vec![b'A'; 43];
        field
            .edit(
                &mut image,
                3,
                FixedTextInput::Text {
                    source: &mut source,
                    phase: 2,
                },
            )
            .unwrap();
        assert_eq!(&image.memory()[0x3c701..0x3c72c], &[b'A'; 43]);
        let mut source = b"ABC".to_vec();
        let before = image.memory().to_vec();
        assert!(
            field
                .edit_with_nls(
                    &mut image,
                    0,
                    FixedTextInput::Text {
                        source: &mut source,
                        phase: 1
                    },
                    |_, _| Err(crate::csv::Error::SourceUnavailable(
                        "actual LCMap output missing"
                    ))
                )
                .is_err()
        );
        assert_eq!(image.memory(), before);
        field
            .edit_with_nls(
                &mut image,
                0,
                FixedTextInput::Text {
                    source: &mut source,
                    phase: 1,
                },
                |_, out| {
                    out.copy_from_slice(b"a\xffb   ");
                    Ok(0)
                },
            )
            .unwrap();
        assert_eq!(&image.memory()[0x3c680..0x3c683], b"a\xffb");
    }

    #[test]
    fn sentence_dialogs_choose_ok_cancel_projection_and_deactivate_use_real_stores() {
        for old in 0_u8..64 {
            let mut image = image();
            let bytes: Vec<_> = (0..6).map(|index| 0xa4 | ((old >> index) & 1)).collect();
            image.patch_memory(&[(0x399ea, &bytes)]).unwrap();
            let before = image.memory().to_vec();
            let mut dialog = SentenceDialog::new(&image, SentenceField::Nmea);
            dialog.apply(&mut image, SentenceEvent::Ok).unwrap();
            assert_eq!(image.memory(), before);
            let mut dialog = SentenceDialog::new(&image, SentenceField::Nmea);
            for index in 1..=6 {
                dialog
                    .apply(
                        &mut image,
                        SentenceEvent::Choose {
                            index,
                            selected: index == 1 || index == 6,
                        },
                    )
                    .unwrap();
            }
            assert!(dialog.valid());
            let effect = dialog.apply(&mut image, SentenceEvent::Ok).unwrap();
            assert!(effect.helper_changed);
            let mut expected = before;
            if old != 33 {
                expected[0x399ea..0x399f0].copy_from_slice(&[1, 0, 0, 0, 0, 1]);
            }
            assert_eq!(image.memory(), expected);
        }
        for old in 0_u8..=255 {
            let mut image = image();
            image.patch_memory(&[(0x399f3, &[old])]).unwrap();
            let before = image.memory().to_vec();
            let mut dialog = SentenceDialog::new(&image, SentenceField::Logger);
            let effect = dialog.apply(&mut image, SentenceEvent::Ok).unwrap();
            let mut expected = before;
            if old & 0xf0 != 0 {
                expected[0x399f3] = old & 0xf0;
                assert!(effect.stored);
            } else {
                assert!(!effect.stored);
            }
            assert_eq!(image.memory(), expected);
        }
        let mut image = image();
        let before = image.memory().to_vec();
        let mut dialog = SentenceDialog::new(&image, SentenceField::Logger);
        dialog.apply(&mut image, SentenceEvent::Cancel).unwrap();
        assert!(
            !dialog
                .apply(&mut image, SentenceEvent::Deactivate)
                .unwrap()
                .stored
        );
        assert_eq!(image.memory(), before);
        assert!(dialog.apply(&mut image, SentenceEvent::Ok).unwrap().stored);
        let committed = image.memory().to_vec();
        dialog
            .apply(&mut image, SentenceEvent::OwnerDestroyed)
            .unwrap();
        assert_eq!(image.memory(), committed);
        let mut dialog = SentenceDialog::new(&image, SentenceField::Logger);
        dialog.apply(&mut image, SentenceEvent::Close).unwrap();
        assert!(!dialog.done);
        assert!(
            dialog
                .apply(&mut image, SentenceEvent::Deactivate)
                .unwrap()
                .helper_changed
        );
        assert!(
            dialog
                .apply(
                    &mut image,
                    SentenceEvent::Choose {
                        index: 0,
                        selected: true
                    }
                )
                .is_err()
        );
        assert!(
            dialog
                .apply(
                    &mut image,
                    SentenceEvent::Choose {
                        index: 5,
                        selected: true
                    }
                )
                .is_err()
        );
    }

    #[test]
    fn gs01_sentence_dispatch_is_not_gated_by_checkbox_validator() {
        for field in SentenceField::ALL {
            for selected in [0, 5] {
                if field == SentenceField::Logger && selected == 5 {
                    continue;
                }
                let mut image = image();
                image
                    .patch_memory(&[
                        (0x399ea, &[0x81, 0x80, 0x80, 0x80, 0x80, 0x80]),
                        (0x399f3, &[0x81]),
                    ])
                    .unwrap();
                let before = image.memory().to_vec();
                let mut dialog = SentenceDialog::new(&image, field);
                for index in 1..=field.count() {
                    dialog
                        .apply(
                            &mut image,
                            SentenceEvent::Choose {
                                index,
                                selected: index <= selected,
                            },
                        )
                        .unwrap();
                }
                assert!(!dialog.valid());
                let effect = dialog.apply(&mut image, SentenceEvent::Deactivate).unwrap();
                assert_eq!(
                    (
                        effect.helper_changed,
                        effect.stored,
                        dialog.done,
                        dialog.release_requested
                    ),
                    (true, false, true, true)
                );
                assert_eq!(image.memory(), before);
                assert!(
                    !dialog
                        .apply(&mut image, SentenceEvent::Ok)
                        .unwrap()
                        .helper_changed
                );
            }
        }
        let mut image = image();
        image.patch_memory(&[(0x399f3, &[0])]).unwrap();
        let before = image.memory().to_vec();
        let mut dialog = SentenceDialog::new(&image, SentenceField::Logger);
        dialog.apply(&mut image, SentenceEvent::Cancel).unwrap();
        assert!(
            !dialog
                .apply(&mut image, SentenceEvent::Deactivate)
                .unwrap()
                .helper_changed
        );
        assert!(
            dialog
                .apply(&mut image, SentenceEvent::Ok)
                .unwrap()
                .helper_changed
        );
        assert!(dialog.done && dialog.release_requested);
        assert_eq!(image.memory(), before);
    }

    #[test]
    fn alarm_type_group_and_row_keep_first_start_counts_full_candidates_and_linked_flags() {
        use super::super::extended::NumberInput;
        let chosen = |candidate| {
            ChoiceInput::Selected(super::super::Selection {
                key: 1,
                phase: 2,
                candidate,
                accepted: true,
            })
        };
        let mut image = image();
        image
            .patch_memory(&[
                (0x3c5dd, &[0xff; 54]),
                (0x3c385, &[0xff; 600]),
                (0x3c622, &[0, 9, 8, 7]),
            ])
            .unwrap();
        let before = image.memory().to_vec();
        let out = AlarmField::Type.edit(&mut image, chosen(3)).unwrap();
        assert!(!out.stored);
        assert_eq!(out.notify_flags, 32);
        assert_eq!(image.memory(), before);
        image
            .patch_memory(&[(0x3c5dd + 10, &0_u16.to_le_bytes())])
            .unwrap();
        let out = AlarmField::Type.edit(&mut image, chosen(3)).unwrap();
        assert!(out.stored);
        assert_eq!(&image.memory()[0x3c622..0x3c626], &[3, 5, 0, 0]);
        let out = AlarmField::Type.edit(&mut image, chosen(259)).unwrap();
        assert!(out.stored);
        assert_eq!(&image.memory()[0x3c622..0x3c626], &[3, 0, 0, 0]);
        image.patch_memory(&[(0x3c622, &[2, 27, 8, 7])]).unwrap();
        let out = AlarmField::Group.edit(&mut image, chosen(27)).unwrap();
        assert!(out.stored);
        assert_eq!(out.notify_flags, 0);
        assert_eq!(&image.memory()[0x3c623..0x3c626], &[27, 0, 0]);
        image
            .patch_memory(&[
                (0x3c622, &[3, 0, 8, 7]),
                (0x3c385, &[0, 0, 1, 0, 0xff, 0xff]),
            ])
            .unwrap();
        let before = image.memory().to_vec();
        assert!(
            !AlarmField::Group
                .edit(&mut image, chosen(0))
                .unwrap()
                .stored
        );
        assert_eq!(image.memory(), before);
        let out = AlarmField::Group.edit(&mut image, chosen(5)).unwrap();
        assert!(out.stored);
        assert_eq!(out.notify_flags, 32);
        assert_eq!(&image.memory()[0x3c623..0x3c626], &[5, 0, 0]);
        let row = number_fields()
            .iter()
            .find(|f| f.setting.key == 0x1cb)
            .unwrap();
        let out = row
            .edit(&mut image, NumberInput::Text(b"1"), b'.', None)
            .unwrap();
        assert!(out.stored && out.accepted);
        assert_eq!(gps_word(&image, 0x3c624), 1);
        let before = image.memory().to_vec();
        let out = row
            .edit(&mut image, NumberInput::Text(b"2"), b'.', None)
            .unwrap();
        assert!(!out.stored && out.accepted);
        assert_eq!(image.memory(), before);
        let out = AlarmField::Group.edit(&mut image, chosen(6)).unwrap();
        assert!(!out.stored);
        assert_eq!(image.memory(), before);
        image.patch_memory(&[(0x3c622, &[1])]).unwrap();
        let before = image.memory().to_vec();
        assert!(
            !row.edit(&mut image, NumberInput::Text(b"0"), b'.', None)
                .unwrap()
                .accepted
        );
        assert!(
            !AlarmField::Group
                .edit(&mut image, ChoiceInput::Begin)
                .unwrap()
                .popup_requested
        );
        assert_eq!(image.memory(), before);
    }

    #[test]
    fn gps_numeric_wrappers_use_shared_grammar_native_units_missing_gate_and_gray_after_store() {
        use super::super::extended::NumberInput;
        for field in number_fields().iter().filter(|f| f.setting.key != 0x1cb) {
            let mut image = image();
            image
                .patch_memory(&[(0x396b5, &[0]), (0x396b6, &[2])])
                .unwrap();
            let before = image.memory().to_vec();
            let out = field
                .edit(
                    &mut image,
                    NumberInput::Text(b"1"),
                    b'.',
                    Some(b"native unit"),
                )
                .unwrap();
            assert!(out.stored && out.accepted);
            let expected_raw = match field.kind {
                NumberKind::Altitude => 33_i32,
                NumberKind::Course => 1,
                NumberKind::Speed => 10,
                _ => unreachable!(),
            };
            let mut expected = before;
            expected[field.setting.offset..field.setting.offset + field.setting.width]
                .copy_from_slice(&expected_raw.to_le_bytes()[..field.setting.width]);
            assert_eq!(image.memory(), expected);
            let before = image.memory().to_vec();
            assert!(
                !field
                    .edit(&mut image, NumberInput::Cancel, b'.', Some(b"unit"))
                    .unwrap()
                    .stored
            );
            assert_eq!(image.memory(), before);
            assert_eq!(
                field
                    .edit(&mut image, NumberInput::Begin, b'.', Some(b"unit"))
                    .unwrap()
                    .state_out,
                2
            );
            assert_eq!(image.memory(), before);
            assert!(
                field
                    .edit(&mut image, NumberInput::Text(b"9"), b'.', None)
                    .is_err()
            );
            assert_eq!(image.memory(), before);
        }
        let mut image = image();
        image.patch_memory(&[(0x396b5, &[0])]).unwrap();
        let altitude = number_fields()
            .iter()
            .find(|f| f.setting.key == 0x1ed)
            .unwrap();
        for (text, raw) in [
            (b"-1".as_slice(), -33_i32),
            (b"10001", 328084),
            (b".", i32::MAX),
            (b"\0-2", i32::MAX),
        ] {
            let out = altitude
                .edit(&mut image, NumberInput::Text(text), b'.', Some(b"m"))
                .unwrap();
            assert!(out.stored);
            assert_eq!(out.candidate, i64::from(raw));
            assert_eq!(out.notify_flags, 32);
        }
        let before = image.memory().to_vec();
        let out = altitude
            .edit(&mut image, NumberInput::Next, b'.', Some(b"m"))
            .unwrap();
        assert!(!out.stored);
        assert_eq!(out.state_out, 2);
        assert_eq!(image.memory(), before);
        let course = number_fields()
            .iter()
            .find(|f| f.setting.key == 0x1ef)
            .unwrap();
        let out = course
            .edit(&mut image, NumberInput::Text(b"360"), b'.', Some(b"degree"))
            .unwrap();
        assert_eq!(out.candidate, 360);
        assert_eq!(gps_word(&image, 0x3c776), 360);
        let speed = number_fields()
            .iter()
            .find(|f| f.setting.key == 0x1f0)
            .unwrap();
        for (unit, raw, max) in [(0, 53, 1850), (1, 86, 1150), (2, 100, 999), (255, 53, 1850)] {
            image.patch_memory(&[(0x396b6, &[unit])]).unwrap();
            let out = speed
                .edit(&mut image, NumberInput::Text(b"10"), b'.', Some(b"unit"))
                .unwrap();
            assert_eq!(out.candidate, raw);
            assert_eq!(speed.bounds(&image), (0, max));
            assert!(out.gray);
        }
        image
            .patch_memory(&[(0x3c778, &65535_u16.to_le_bytes()), (0x396b6, &[0])])
            .unwrap();
        let before = image.memory().to_vec();
        let out = speed
            .edit(&mut image, NumberInput::Display, b'.', Some(b"unit"))
            .unwrap();
        assert!(!out.stored);
        assert_ne!(out.candidate, 65535);
        assert_eq!(image.memory(), before);
    }

    #[test]
    fn symbol_callers_use_native_resource_lookup_overlay_and_distinct_direct_profile() {
        use super::super::extended::FixedTextInput;
        let mut bytes = [b'/'; 148];
        for pair in bytes.as_chunks_mut::<2>().0 {
            pair[1] = b'!';
        }
        bytes[6..8].copy_from_slice(b"\\>");
        bytes[8..10].copy_from_slice(b"IY");
        let resource = NativeSymbolPairs::read(&bytes).unwrap();
        let chosen = |candidate| {
            ChoiceInput::Selected(super::super::Selection {
                key: 1,
                phase: 2,
                candidate,
                accepted: true,
            })
        };
        for field in symbol_fields() {
            for index in 0..field.count {
                let mut image = image();
                let at = field.at + 2 * index;
                let before = image.memory().to_vec();
                assert!(
                    !field
                        .edit(&mut image, index, FixedTextInput::Cancel)
                        .unwrap()
                        .stored
                );
                assert_eq!(image.memory(), before);
                let mut source = b"a /ignored".to_vec();
                let out = field
                    .edit(
                        &mut image,
                        index,
                        FixedTextInput::Text {
                            source: &mut source,
                            phase: 1,
                        },
                    )
                    .unwrap();
                assert!(out.stored);
                assert_eq!(out.notify_flags, 32);
                let mut expected = before;
                expected[at..at + 2].copy_from_slice(b"A/");
                assert_eq!(image.memory(), expected);
                for source in [b"".as_slice(), b"A", b"~!", b"\x7fA"] {
                    let mut source = source.to_vec();
                    let before = image.memory().to_vec();
                    assert!(
                        !field
                            .edit(
                                &mut image,
                                index,
                                FixedTextInput::Text {
                                    source: &mut source,
                                    phase: 1
                                }
                            )
                            .unwrap()
                            .stored
                    );
                    assert_eq!(image.memory(), before);
                }
                image.patch_memory(&[(at, b"A>")]).unwrap();
                let out = field
                    .select(&mut image, index, chosen(3), &resource)
                    .unwrap();
                assert!(out.stored);
                assert_eq!(field.read(&image, index).unwrap(), *b"\\>");
                let out = field
                    .select(&mut image, index, chosen(74), &resource)
                    .unwrap();
                assert!(out.stored);
                assert_eq!(field.read(&image, index).unwrap(), *b"/\"");
                let before = image.memory().to_vec();
                let out = field
                    .select(&mut image, index, chosen(-1), &resource)
                    .unwrap();
                assert!(!out.stored);
                assert_eq!(out.notify_flags, 32);
                assert_eq!(image.memory(), before);
                assert!(
                    field
                        .select(&mut image, index, chosen(-256), &resource)
                        .unwrap()
                        .stored
                );
                assert_eq!(field.read(&image, index).unwrap(), *b"/!");
                image.patch_memory(&[(at, b"IY")]).unwrap();
                assert_eq!(
                    field
                        .select(&mut image, index, ChoiceInput::Display, &resource)
                        .unwrap()
                        .value,
                    4
                );
                assert!(
                    field
                        .edit(&mut image, field.count, FixedTextInput::Cancel)
                        .is_err()
                );
            }
        }
        assert!(NativeSymbolPairs::read(&bytes[..147]).is_err());
    }

    #[test]
    fn coordinate_callers_reuse_shared_mask_phase2_and_keep_degree_failure_prefix() {
        use crate::csv::number::CoordinateEvent;
        for field in coordinate_fields() {
            let mut image = image();
            let mut current = image.memory().to_vec();
            current[0x396b4] = 1;
            let before = image.memory().to_vec();
            let out = field.phase2(&mut image, b"035x12y34zS", &current).unwrap();
            assert!(out.stored);
            let mut expected = before;
            expected[field.at..field.at + 4].copy_from_slice(&[35, 0x31, 0x16, b'S']);
            assert_eq!(image.memory(), expected);
            let before = image.memory().to_vec();
            assert!(field.phase2(&mut image, b"123", &current).is_err());
            let mut expected = before;
            expected[field.at] = 123;
            assert_eq!(image.memory(), expected);
            let before = image.memory().to_vec();
            assert!(field.phase2(&mut image, b"045", &[]).is_err());
            let mut expected = before;
            expected[field.at] = 45;
            assert_eq!(image.memory(), expected);
            let mut dialog = field.dialog(&image, &current, None).unwrap();
            dialog.replace(if field.longitude {
                b"1234567W"
            } else {
                b"351234S"
            });
            let before = image.memory().to_vec();
            assert!(
                !field
                    .apply_dialog(&mut image, &mut dialog, CoordinateEvent::Cancel, &current)
                    .unwrap()
                    .stored
            );
            assert_eq!(image.memory(), before);
            let mut dialog = field.dialog(&image, &current, None).unwrap();
            dialog.replace(if field.longitude {
                b"1234567W"
            } else {
                b"351234S"
            });
            if field.longitude {
                assert!(!dialog.valid());
                dialog.replace(b"1234534W");
            }
            assert!(dialog.valid());
            assert!(
                field
                    .apply_dialog(&mut image, &mut dialog, CoordinateEvent::Accept, &current)
                    .unwrap()
                    .stored
            );
            assert!(dialog.done && dialog.release_requested);
            assert_eq!(
                field.read(&image),
                if field.longitude {
                    [123, 0xb1, 0xfe, b'W']
                } else {
                    [35, 0x31, 0x16, b'S']
                }
            );
        }
    }

    #[test]
    fn area_caller_packs_only_after_shared_parser_return_and_uses_live_mode() {
        use crate::csv::number::CoordinateEvent;
        for (unit, mode, text, word) in [
            (1, 11, b"12x34".as_slice(), 12566_u16),
            (0, 12, b"12x34", 12340),
            (2, 13, b"0.1234", 7404),
            (255, 12, b"12x34", 12340),
        ] {
            let mut image = image();
            let mut current = image.memory().to_vec();
            current[0x396b4] = unit;
            let before = image.memory().to_vec();
            let out = ALARM_AREA.phase2(&mut image, text, &current).unwrap();
            assert!(out.stored && !out.gray);
            let mut expected = before;
            expected[0x3c626..0x3c628].copy_from_slice(&word.to_le_bytes());
            assert_eq!(image.memory(), expected);
            let mut dialog = ALARM_AREA.dialog(&image, &current, None).unwrap();
            assert_eq!(dialog.mode, mode);
            dialog.replace(b"1234");
            assert!(dialog.valid());
            let out = ALARM_AREA
                .apply_dialog(&mut image, &mut dialog, CoordinateEvent::Accept, &current)
                .unwrap();
            assert!(out.stored && dialog.done && dialog.release_requested);
            assert_eq!(gps_word(&image, 0x3c626), word);
        }
        let mut image = image();
        let mut current = image.memory().to_vec();
        current[0x396b4] = 1;
        for text in [b"".as_slice(), b"12"] {
            let before = image.memory().to_vec();
            assert!(ALARM_AREA.phase2(&mut image, text, &current).is_err());
            assert_eq!(image.memory(), before);
        }
        let before = image.memory().to_vec();
        assert!(ALARM_AREA.phase2(&mut image, b"12x34", &[]).is_err());
        assert_eq!(image.memory(), before);
        let mut dialog = ALARM_AREA.dialog(&image, &current, None).unwrap();
        dialog.replace(b"1234");
        assert!(
            ALARM_AREA
                .apply_dialog(&mut image, &mut dialog, CoordinateEvent::Accept, &[])
                .is_err()
        );
        assert!(!dialog.done && !dialog.release_requested);
        assert!(dialog.phase2.is_some());
        assert_eq!(image.memory(), before);
        current[0x396b4] = 2;
        assert!(
            ALARM_AREA
                .apply_dialog(&mut image, &mut dialog, CoordinateEvent::Accept, &current)
                .unwrap()
                .stored
        );
        assert_eq!(dialog.mode, 11);
        assert_eq!(gps_word(&image, 0x3c626), 0);
        assert!(dialog.done);
    }

    #[test]
    fn area_dialog_seed_minimum_cancel_and_deactivate_do_not_become_raw_input_clamps() {
        use crate::csv::number::CoordinateEvent;
        let mut image = image();
        let mut current = image.memory().to_vec();
        current[0x396b4] = 2;
        image
            .patch_memory(&[(0x3c626, &0_u16.to_le_bytes())])
            .unwrap();
        let before = image.memory().to_vec();
        let mut dialog = ALARM_AREA.dialog(&image, &current, None).unwrap();
        assert_eq!(dialog.seed, b"0.0014\xb0");
        assert_eq!(image.memory(), before);
        dialog.replace(b"0000");
        assert!(!dialog.valid());
        assert!(
            !ALARM_AREA
                .apply_dialog(&mut image, &mut dialog, CoordinateEvent::Accept, &current)
                .unwrap()
                .stored
        );
        assert_eq!(image.memory(), before);
        assert!(
            ALARM_AREA
                .apply_dialog(
                    &mut image,
                    &mut dialog,
                    CoordinateEvent::Deactivate,
                    &current
                )
                .unwrap()
                .stored
        );
        assert_eq!(gps_word(&image, 0x3c626), 84);
        ALARM_AREA
            .phase2(&mut image, b"0.0000\xb0", &current)
            .unwrap();
        assert_eq!(gps_word(&image, 0x3c626), 0);
        let mut dialog = ALARM_AREA.dialog(&image, &current, None).unwrap();
        dialog.replace(b"1234");
        let before = image.memory().to_vec();
        assert!(
            !ALARM_AREA
                .apply_dialog(&mut image, &mut dialog, CoordinateEvent::Cancel, &current)
                .unwrap()
                .stored
        );
        assert!(
            !ALARM_AREA
                .apply_dialog(
                    &mut image,
                    &mut dialog,
                    CoordinateEvent::Deactivate,
                    &current
                )
                .unwrap()
                .stored
        );
        assert_eq!(image.memory(), before);
        let mut dialog = ALARM_AREA.dialog(&image, &current, None).unwrap();
        dialog.replace(b"1234");
        assert!(
            !ALARM_AREA
                .apply_dialog(
                    &mut image,
                    &mut dialog,
                    CoordinateEvent::EscapeKeyPress,
                    &current
                )
                .unwrap()
                .stored
        );
        assert!(
            ALARM_AREA
                .apply_dialog(
                    &mut image,
                    &mut dialog,
                    CoordinateEvent::Deactivate,
                    &current
                )
                .unwrap()
                .stored
        );
        assert_eq!(gps_word(&image, 0x3c626), 7404);
        let before = image.memory().to_vec();
        assert!(
            !ALARM_AREA
                .apply_dialog(
                    &mut image,
                    &mut dialog,
                    CoordinateEvent::Deactivate,
                    &current
                )
                .unwrap()
                .stored
        );
        assert_eq!(image.memory(), before);
    }
}
