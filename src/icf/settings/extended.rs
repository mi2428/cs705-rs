//! Original setting editor operations above the shared storage codecs.
//!
//! Numeric paths use A4B5C's nine-byte filter or A522C's eleven-byte filter,
//! not CSV's strict decimal grammar. The CRT decimal byte is explicit; no host
//! locale, factory image, Q generation or successful native callback is guessed.

use super::{Kind, Value};
use crate::icf::{Error, Icf};

/// Input from a normal editor invocation; display and cancellation are distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberInput<'a> {
    Display,
    Cancel,
    Text(&'a [u8]),
    Selected(super::Selection),
    Next,
    Previous,
    Begin,
}

/// Numeric result before the common document callback/dirty tail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberEffect {
    /// A case-body store executed, including an unconditional same-value store.
    pub stored: bool,
    /// Native helper success, not a strict parse-valid flag or document dirty.
    pub accepted: bool,
    /// Inner helper candidate, before a byte wrapper's success-only copy-back.
    pub candidate: i64,
    /// Raw DWORD before width projection, using the shared descriptor decoder.
    pub raw_candidate: i32,
    /// State2 requests inline input and does not itself commit a value.
    pub state_out: u32,
    pub notify_flags: u32,
    /// Old-raw suffix value; percentage/WPM are not the accepted input units.
    pub suffix: Vec<u8>,
    pub gray: bool,
}

/// Implemented numeric editor metadata, reusing the parent's storage descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumberField {
    pub feature_id: &'static str,
    pub name: &'static str,
    pub key: u16,
    pub minimum: i64,
    pub maximum: i64,
    pub step: i64,
    pub unit: &'static str,
    pub count: usize,
    /// Typed text multiplied by 10^text_scale before storage-unit rounding.
    pub text_scale: u32,
    pub selected_raw_bypass: bool,
}

impl NumberField {
    pub fn accepts_index(&self, index: usize) -> bool {
        number_index_valid(self.key, index, self.count)
    }
    pub fn text_grammar(&self) -> &'static str {
        if self.key == 0x307 {
            "hex byte, first two filtered nibbles"
        } else {
            "native decimal prefix"
        }
    }
}

fn number_index_valid(key: u16, index: usize, count: usize) -> bool {
    index < count && !(matches!(key, 0x348 | 0x349) && index == 3)
}

const fn number(
    id: &'static str,
    name: &'static str,
    key: u16,
    at: usize,
    encoding: (usize, i64),
    domain: (i64, i64, i64, &'static str),
    axis: (usize, usize),
) -> super::Setting {
    super::Setting {
        feature_id: id,
        name,
        key,
        selection_key: 1,
        domain: super::Domain {
            min: domain.0,
            max: domain.1,
            step: domain.2,
        },
        count: axis.0,
        kind: Kind::Integer(domain.3),
        offset: at,
        width: encoding.0,
        signed: false,
        bias: encoding.1,
        stride: axis.1,
    }
}

static EXTRA_NUMBERS: [super::Setting; 19] = [
    number(
        "A2-H56068-10",
        "cw_rise_milliseconds",
        0x2ca,
        0x39973,
        (1, 0),
        (2, 8, 2, "milliseconds"),
        (1, 0),
    ),
    number(
        "A2-H56399-21",
        "civ_address",
        0x307,
        0x3934d,
        (1, 0),
        (2, 223, 1, "hex address"),
        (1, 0),
    ),
    number(
        "A2-H56153-03",
        "receive_bass",
        0x346,
        0x392ca,
        (1, -5),
        (-5, 5, 1, "tone level"),
        (5, 1),
    ),
    number(
        "A2-H56153-04",
        "receive_treble",
        0x347,
        0x392d2,
        (1, -5),
        (-5, 5, 1, "tone level"),
        (5, 1),
    ),
    number(
        "A2-H56153-05",
        "transmit_bass",
        0x348,
        0x392c0,
        (1, -5),
        (-5, 5, 1, "tone level"),
        (5, 1),
    ),
    number(
        "A2-H56153-06",
        "transmit_treble",
        0x349,
        0x392c5,
        (1, -5),
        (-5, 5, 1, "tone level"),
        (5, 1),
    ),
    number(
        "A2-H54848-71",
        "rf_power_raw",
        0x28a,
        0x39993,
        (1, 0),
        (0, 255, 1, "raw level"),
        (5, 1),
    ),
    number(
        "A2-H54848-127",
        "weather_channel",
        0x2a9,
        0x3c8b3,
        (1, 1),
        (1, 10, 1, "one-based channel"),
        (1, 0),
    ),
    number(
        "A2-H55922-20",
        "emr_af_level_raw",
        0x1a3,
        0x399b9,
        (1, 0),
        (0, 255, 1, "raw level"),
        (1, 0),
    ),
    number(
        "A2-H56371-04",
        "dr_noise_reduction_level",
        0x124,
        0x3c37d,
        (1, 0),
        (0, 15, 1, "raw level"),
        (1, 0),
    ),
    number(
        "A2-H56399-02",
        "phones_level_ratio",
        0x2e6,
        0x39335,
        (1, -15),
        (-15, 15, 1, "ratio"),
        (1, 0),
    ),
    number(
        "A2-H56399-04",
        "usb_af_output_level_raw",
        0x2ef,
        0x39337,
        (1, 0),
        (0, 255, 1, "raw level"),
        (1, 0),
    ),
    number(
        "A2-H56399-07",
        "usb_if_output_level_raw",
        0x2f2,
        0x3933a,
        (1, 0),
        (0, 255, 1, "raw level"),
        (1, 0),
    ),
    number(
        "A2-H56399-10",
        "usb_modulation_level_raw",
        0x2f6,
        0x3933d,
        (1, 0),
        (0, 255, 1, "raw level"),
        (1, 0),
    ),
    number(
        "A2-H56399-11",
        "wlan_modulation_level_raw",
        0x2f7,
        0x3933e,
        (1, 0),
        (0, 255, 1, "raw level"),
        (1, 0),
    ),
    number(
        "A2-H56202-14",
        "network_control_port",
        0x324,
        0x3937c,
        (2, 0),
        (1, 65535, 1, "port"),
        (1, 0),
    ),
    number(
        "A2-H56202-15",
        "network_serial_port",
        0x325,
        0x3937e,
        (2, 0),
        (1, 65535, 1, "port"),
        (1, 0),
    ),
    number(
        "A2-H56202-16",
        "network_audio_port",
        0x326,
        0x39380,
        (2, 0),
        (1, 65535, 1, "port"),
        (1, 0),
    ),
    number(
        "A2-H56093-01",
        "scope_reference_tenths_db",
        0x362,
        0x377db,
        (2, -200),
        (-200, 200, 5, "tenths dB"),
        (17, 2),
    ),
];

fn number_settings() -> Vec<&'static super::Setting> {
    super::catalog()
        .iter()
        .filter(|field| {
            matches!(
                field.key,
                0x211
                    | 0x231
                    | 0x2ab
                    | 0x265
                    | 0x277
                    | 0x27b
                    | 0x25c
                    | 0x25d
                    | 0x24e
                    | 0x293
                    | 0x295
                    | 0x261
                    | 0x278
                    | 0x25e
                    | 0x294
                    | 0x297
                    | 0x240
                    | 0x2ad
                    | 0x29a
                    | 0x2c1
                    | 0x2c2
                    | 0x2c5
                    | 0x2c6
                    | 0x2c8
                    | 0x2c9
                    | 0x2cf
                    | 0x2db
            )
        })
        .chain(EXTRA_NUMBERS.iter())
        .collect()
}

/// First independent numeric family. UTC and the M1..M8 choice are different
/// grammars and deliberately do not enter the generic finite numeric editor.
pub fn number_fields() -> Vec<NumberField> {
    number_settings()
        .into_iter()
        .filter_map(|field| {
            let Kind::Integer(unit) = field.kind else {
                return None;
            };
            Some(NumberField {
                feature_id: field.feature_id,
                name: field.name,
                key: field.key,
                minimum: field.domain.min,
                maximum: field.domain.max,
                step: field.domain.step,
                unit,
                count: field.count,
                text_scale: number_scale(field.key),
                selected_raw_bypass: number_selected(field.key),
            })
        })
        .collect()
}

/// Edit a stored integer using its original typed numeric path.
/// `crt_point` is the actual CRT decimal byte; filtered source still only keeps
/// ASCII dot/digits/minus, as B0E00 does. This finite binary64 normal-CW profile
/// excludes native x87 indefinite/exception behavior rather than defaulting it.
pub fn edit_number(
    image: &mut Icf<'_>,
    name: &str,
    input: NumberInput<'_>,
    crt_point: u8,
) -> Result<NumberEffect, Error> {
    edit_number_at(image, name, 0, input, crt_point)
}

pub fn edit_number_at(
    image: &mut Icf<'_>,
    name: &str,
    index: usize,
    input: NumberInput<'_>,
    crt_point: u8,
) -> Result<NumberEffect, Error> {
    let candidates = number_settings();
    let mut matches = candidates
        .into_iter()
        .filter(|field| field.name == name || field.feature_id == name);
    let field = matches
        .next()
        .ok_or_else(|| invalid("setting has a different editor grammar"))?;
    if matches.next().is_some() {
        return Err(invalid("ambiguous numeric feature; use semantic name"));
    }
    if !number_index_valid(field.key, index, field.count) {
        return Err(invalid("numeric index outside native layout"));
    }
    let Value::Number(mut old) = field.read(image, index)?;
    let old_raw = field.read_numeric_raw(image, index)?;
    if field.key == 0x2ca {
        old = 2 * i64::from(old_raw) + 2;
    }
    let bias = old - i64::from(old_raw);
    let scale = number_scale(field.key);
    let (candidate, accepted, state_out) = match input {
        NumberInput::Display | NumberInput::Cancel => (old, false, 0),
        NumberInput::Begin => (old, false, 2),
        NumberInput::Next | NumberInput::Previous => {
            let value = old
                + if input == NumberInput::Next {
                    field.domain.step
                } else {
                    -field.domain.step
                };
            (
                if value < field.domain.min {
                    field.domain.max
                } else if value > field.domain.max {
                    field.domain.min
                } else {
                    value
                },
                true,
                0,
            )
        }
        NumberInput::Text(text) => {
            if field.key == 0x307 {
                let digits: Vec<_> = text
                    .iter()
                    .copied()
                    .take_while(|&b| b != 0)
                    .filter(u8::is_ascii_hexdigit)
                    .take(2)
                    .collect();
                (
                    i64::from(crate::icf::text::hexadecimal_prefix(&digits)).clamp(2, 223),
                    true,
                    0,
                )
            } else {
                let parsed = numeric_text(
                    text,
                    crt_point,
                    scale,
                    field.domain.min,
                    field.domain.max,
                    field.domain.step,
                )?;
                (parsed.0, parsed.1, 0)
            }
        }
        NumberInput::Selected(event) => {
            let accepted =
                number_selected(field.key) && event.key == 1 && event.phase == 2 && event.accepted;
            (
                if accepted {
                    i64::from(event.candidate) + if field.key == 0x2ca { 0 } else { bias }
                } else {
                    old
                },
                accepted,
                0,
            )
        }
    };
    // NB width's caller ignores the helper bool; its byte wrapper still retains
    // old raw on false. A same-value write is not an implicit old-value repair.
    let stored = accepted || field.key == 0x295;
    let raw_candidate = if field.key == 0x2ca {
        (candidate / 2 - 1) as i32
    } else {
        (candidate - bias) as i32
    };
    let mut notify_flags = 0;
    if stored {
        if (matches!(input, NumberInput::Selected(_)) || field.key == 0x2ca) && accepted {
            field.store_numeric_raw(image, index, raw_candidate)?;
        } else {
            field.write(
                image,
                index,
                &Value::Number(if accepted { candidate } else { old }),
            )?;
        }
        // Reverse EQ coupling tests the stored low byte, not the helper DWORD.
        if matches!(field.key, 0x346 | 0x347) && raw_candidate as u8 != 5 {
            image.patch_memory(&[(0x392da + index, &[0]), (0x392e2 + index, &[25])])?;
            notify_flags = 0x20;
        }
    }
    let suffix = if state_out == 2 {
        Vec::new()
    } else {
        number_suffix(field.key, old)
    };
    Ok(NumberEffect {
        stored,
        accepted,
        candidate,
        raw_candidate,
        state_out,
        notify_flags,
        suffix,
        gray: (field.key == 0x211 && image.memory()[0x3969e] != 0)
            || (field.key == 0x2a9 && image.memory()[0x3a8cb] == 0),
    })
}

fn number_scale(key: u16) -> u32 {
    match key {
        0x240 => 3,
        0x25e | 0x2c9 | 0x2cf | 0x362 => 1,
        _ => 0,
    }
}

fn number_selected(key: u16) -> bool {
    matches!(
        key,
        0x261 | 0x294 | 0x297 | 0x2db | 0x2e6 | 0x124 | 0x346..=0x349 | 0x2ca
    )
}

pub(crate) fn numeric_text(
    text: &[u8],
    point: u8,
    scale: u32,
    min: i64,
    max: i64,
    step: i64,
) -> Result<(i64, bool), Error> {
    let (value, accepted) = numeric_value(text, point, scale, step)?;
    Ok((value.clamp(min, max), accepted))
}

pub(super) fn numeric_value(
    text: &[u8],
    point: u8,
    scale: u32,
    step: i64,
) -> Result<(i64, bool), Error> {
    let mut filtered: Vec<_> = text
        .iter()
        .copied()
        .take_while(|&b| b != 0)
        .filter(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-'))
        .collect();
    let accepted = if scale == 0 {
        filtered.iter().any(|b| b.is_ascii_digit() || *b == b'.')
    } else {
        !filtered.is_empty()
    };
    filtered.truncate(if scale == 0 { 9 } else { 11 });
    let mut number = crate::csv::number::crt_decimal_prefix(&filtered, point)
        .map_err(|_| invalid("numeric CRT finite-prefix conversion failed"))?;
    number *= 10_f64.powi(scale as i32);
    if scale != 0 {
        number = number.clamp(-2147483648.0, 2147483644.0);
    }
    let rounded = (number + if number < 0.0 { -0.5 } else { 0.5 }).trunc() as i64;
    let quantized = if scale == 0 && step != 1 {
        let n = rounded as f64 / step as f64;
        (n + if n < 0.0 { -0.5 } else { 0.5 }).trunc() as i64 * step
    } else {
        rounded - rounded % step
    };
    Ok((quantized, accepted))
}

fn number_suffix(key: u16, old: i64) -> Vec<u8> {
    match key {
        0x211 | 0x231 | 0x2ab | 0x265 | 0x277 | 0x27b | 0x25c | 0x25d | 0x24e | 0x293 | 0x2c6
        | 0x28a | 0x1a3 | 0x2ef | 0x2f2 | 0x2f6 | 0x2f7 => {
            format!(" ({}%)", old * 100 / 255).into_bytes()
        }
        0x295 => format!(" ({})", 1 + old * 99 / 255).into_bytes(),
        0x2c1 => format!(" ({}WPM)", 1200 / (51000 / (7 * old + 255))).into_bytes(),
        0x2ad => {
            let q = ((old * 10000 / 511 + 5) / 10) % 10000;
            format!(" ({}.{})", q / 10, q % 10).into_bytes()
        }
        _ => Vec::new(),
    }
}

/// 73035 -> 9D968 with optional-sentinel flag0. This is not decimal minutes
/// entry: sign filtering, two colon fields, u8 projection and five-minute
/// rounding happen before the signed LE16 storage codec is called.
pub fn edit_utc_offset(image: &mut Icf<'_>, input: NumberInput<'_>) -> Result<NumberEffect, Error> {
    let field = super::find("utc_offset_minutes")?;
    let Value::Number(old) = field.read(image, 0)?;
    let (candidate, accepted, state_out) = match input {
        NumberInput::Display | NumberInput::Cancel => (old, false, 0),
        NumberInput::Selected(_) => (old, false, 0),
        NumberInput::Begin => (old, false, 2),
        NumberInput::Next | NumberInput::Previous => {
            let n = old + if input == NumberInput::Next { 5 } else { -5 };
            (
                if n < -840 {
                    840
                } else if n > 840 {
                    -840
                } else {
                    n
                },
                true,
                0,
            )
        }
        NumberInput::Text(text) => {
            let first: Vec<_> = text
                .iter()
                .copied()
                .take_while(|&b| b != 0)
                .filter(|b| b.is_ascii_digit() || b"+-:".contains(b))
                .collect();
            let negative = first.first() == Some(&b'-');
            let digits: Vec<_> = first
                .into_iter()
                .filter(|b| b.is_ascii_digit() || *b == b':')
                .collect();
            if digits.len() > 5 {
                (old, false, 0)
            } else {
                let mut parts = digits.split(|&b| b == b':');
                let hour = crate::icf::text::decimal_prefix(parts.next().unwrap_or_default()) as u8;
                let minute =
                    crate::icf::text::decimal_prefix(parts.next().unwrap_or_default()) as u8;
                let minute = (5 * ((u16::from(minute) + 2) / 5)) as u8;
                let total = i64::from(hour) * 60 + i64::from(minute);
                if minute > 59 || total > 840 {
                    (old, false, 0)
                } else {
                    (if negative { -total } else { total }, true, 0)
                }
            }
        }
    };
    if accepted {
        field.write(image, 0, &Value::Number(candidate))?;
    }
    // Keyboard changes follow display construction; text changes precede it.
    let display = if matches!(input, NumberInput::Text(_)) && accepted {
        candidate
    } else {
        old
    };
    let n = display.unsigned_abs();
    let hour = (n / 60) as u8;
    let minute = n % 60;
    let sign = if hour == 0 && minute == 0 {
        ""
    } else if display < 0 {
        "-"
    } else {
        "+"
    };
    let suffix = format!("{sign}{hour}:{minute:02}").into_bytes();
    Ok(NumberEffect {
        stored: accepted,
        accepted,
        candidate,
        raw_candidate: candidate as i32,
        state_out,
        notify_flags: 0,
        suffix,
        gray: false,
    })
}

/// Text commit versus inline-input request. No native result is fabricated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInput<'a> {
    Display,
    Cancel,
    Text(&'a [u8]),
    /// None is Enter; Some is a starting character supplied by the keyboard.
    Begin(Option<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEffect {
    pub stored: bool,
    pub state_out: u32,
    pub notify_flags: u32,
    pub display: Vec<u8>,
}

/// The eight color fields dispatched to 9D308. Native dialog invocation and its
/// palette belong to the caller; an actual chosen RGB value or cancellation is
/// applied here through the already-shared three-byte storage codecs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorField {
    ScopeCurrent,
    ScopeOutline,
    ScopeMaxHold,
    AudioFft,
    AudioOsc,
    RttyFft,
    RttyReceive,
    RttyTransmit,
}
impl ColorField {
    pub const ALL: [Self; 8] = [
        Self::ScopeCurrent,
        Self::ScopeOutline,
        Self::ScopeMaxHold,
        Self::AudioFft,
        Self::AudioOsc,
        Self::RttyFft,
        Self::RttyReceive,
        Self::RttyTransmit,
    ];
    pub fn feature_id(self) -> &'static str {
        match self {
            Self::ScopeCurrent => "A2-H56095-11",
            Self::ScopeOutline => "A2-H56095-12",
            Self::ScopeMaxHold => "A2-H56095-13",
            Self::AudioFft => "A2-H56115-03",
            Self::AudioOsc => "A2-H56115-07",
            Self::RttyFft => "A2-H56207-07",
            Self::RttyReceive => "A2-H56207-12",
            Self::RttyTransmit => "A2-H56207-13",
        }
    }
    pub fn key(self) -> u16 {
        match self {
            Self::ScopeCurrent => 0x36a,
            Self::ScopeOutline => 0x36b,
            Self::ScopeMaxHold => 0x36c,
            Self::AudioFft => 0x359,
            Self::AudioOsc => 0x35d,
            Self::RttyFft => 0x2d8,
            Self::RttyReceive => 0x2de,
            Self::RttyTransmit => 0x2df,
        }
    }
    fn read(self, image: &Icf<'_>) -> Result<[u8; 3], Error> {
        match self {
            Self::ScopeCurrent => image.scope_waveform_color_current(),
            Self::ScopeOutline => image.scope_waveform_color_line(),
            Self::ScopeMaxHold => image.scope_waveform_color_max_hold(),
            Self::AudioFft => image.audio_fft_waveform_color(),
            Self::AudioOsc => image.audio_osc_waveform_color(),
            Self::RttyFft => image.rtty_waveform_color_raw(),
            Self::RttyReceive => image.rtty_receive_font_color_raw(),
            Self::RttyTransmit => image.rtty_transmit_font_color_raw(),
        }
    }
    fn store(self, image: &mut Icf<'_>, value: [u8; 3]) -> Result<(), Error> {
        match self {
            Self::ScopeCurrent => image.set_scope_waveform_color_current(value),
            Self::ScopeOutline => image.set_scope_waveform_color_line(value),
            Self::ScopeMaxHold => image.set_scope_waveform_color_max_hold(value),
            Self::AudioFft => image.set_audio_fft_waveform_color(value),
            Self::AudioOsc => image.set_audio_osc_waveform_color(value),
            Self::RttyFft => image.set_rtty_waveform_color_raw(value),
            Self::RttyReceive => image.set_rtty_receive_font_color_raw(value),
            Self::RttyTransmit => image.set_rtty_transmit_font_color_raw(value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorEffect {
    pub stored: bool,
    pub value: [u8; 3],
    pub notify_flags: u32,
}

/// Apply the actual optional dialog choice. The alpha/high byte is not a field;
/// cancellation keeps every raw byte and requests no case-body notification.
pub fn edit_color(
    image: &mut Icf<'_>,
    field: ColorField,
    choice: Option<[u8; 3]>,
) -> Result<ColorEffect, Error> {
    let value = if let Some(value) = choice {
        field.store(image, value)?;
        value
    } else {
        field.read(image)?
    };
    Ok(ColorEffect {
        stored: choice.is_some(),
        value,
        notify_flags: 0,
    })
}

/// Native filter view for i=3*class+slot. The canonical BW getter rejects
/// unknown raw codes; this original display uses the shared unchecked BW
/// formula instead, preserving negative derived passbands and signed PBT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterView {
    pub bandwidth_hz: i32,
    pub pbt_hz: [i32; 2],
    pub passband_hz: i32,
    pub shift_hz: i32,
}

/// H56057-03/04 are read-only expressions, not guessed storage fields.
/// Index validation precedes both the raw BW read and typed PBT access.
pub fn filter_view(image: &Icf<'_>, index: usize) -> Result<FilterView, Error> {
    use crate::icf::{FilterMode, FilterSlot, PbtSide, special};
    if index >= 15 {
        return Err(invalid("filter row outside 0..15"));
    }
    let class = index / 3;
    let mode = [
        FilterMode::Ssb,
        FilterMode::SsbData,
        FilterMode::Cw,
        FilterMode::Rtty,
        FilterMode::Am,
    ][class];
    let slot = [FilterSlot::One, FilterSlot::Two, FilterSlot::Three][index % 3];
    let bandwidth_hz = special::decode_bandwidth(class as u32, image.memory()[0x37850 + index])?;
    let pbt_hz = [
        image.filter_pbt_hz(mode, slot, PbtSide::One)?,
        image.filter_pbt_hz(mode, slot, PbtSide::Two)?,
    ];
    Ok(FilterView {
        bandwidth_hz,
        pbt_hz,
        passband_hz: bandwidth_hz - (pbt_hz[0] - pbt_hz[1]).abs(),
        shift_hz: (pbt_hz[0] + pbt_hz[1]) / 2,
    })
}

/// AFF40 uses the real signed PBT codec and bounds derived from the live BW.
/// Display invocations can clip corrupt old raw; Cancel denotes a dispatched
/// false helper invocation, not a guarantee that a later refresh is read-only.
pub fn edit_pbt(
    image: &mut Icf<'_>,
    index: usize,
    side: crate::icf::PbtSide,
    input: NumberInput<'_>,
    crt_point: u8,
) -> Result<NumberEffect, Error> {
    let view = filter_view(image, index)?;
    let class = index / 3;
    let step = if class == 4 { 200 } else { 50 };
    let old = i64::from(view.pbt_hz[side as usize]);
    let bw = i64::from(view.bandwidth_hz);
    let half = bw / 2;
    let delta = if class == 2 && bw >= 600 {
        (bw - 600) / 2
    } else {
        0
    };
    let (low, high) = (-half - delta, half - delta);
    let clipped = old.clamp(low, high);
    let (mut candidate, mut accepted, state_out) = match input {
        NumberInput::Text(text) => {
            let (value, ok) = numeric_text(text, crt_point, 0, low, high, step)?;
            (value, ok || clipped != old, 0)
        }
        NumberInput::Next | NumberInput::Previous => {
            let value = clipped
                + if input == NumberInput::Next {
                    step
                } else {
                    -step
                };
            (
                if value < low {
                    high
                } else if value > high {
                    low
                } else {
                    value
                },
                true,
                0,
            )
        }
        NumberInput::Begin => (clipped, clipped != old, 2),
        _ => (clipped, clipped != old, 0),
    };
    if (input == NumberInput::Next && candidate < old)
        || (input == NumberInput::Previous && candidate > old)
    {
        candidate = old;
        accepted = false;
    }
    let raw_candidate = (candidate / step) as i32;
    if accepted {
        const RAW: super::Setting = super::Setting {
            signed: true,
            ..number(
                "A2-H56057-02",
                "pbt_raw",
                0x12b,
                0x37867,
                (1, 0),
                (-128, 127, 1, "raw PBT code"),
                (30, 1),
            )
        };
        RAW.store_numeric_raw(image, 2 * index + side as usize, raw_candidate)?;
    }
    Ok(NumberEffect {
        stored: accepted,
        accepted,
        candidate,
        raw_candidate,
        state_out,
        notify_flags: if accepted { 0x20 } else { 0 },
        suffix: Vec::new(),
        gray: false,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BandwidthEffect {
    pub number: NumberEffect,
    pub popup_requested: bool,
    /// A4A8C replaces phase1 context48 with the FIRST integer before the grid
    /// evaluation. It does not re-parse the original text as Home VFO does.
    pub rewritten_text: Option<Vec<u8>>,
}

fn bandwidth_step(class: u32, width: i64, previous: bool) -> Result<i64, Error> {
    use crate::icf::special::{decode_bandwidth, encode_bandwidth};
    let small = i64::from(decode_bandwidth(class, 1)? - decode_bandwidth(class, 0)?);
    let raw = encode_bandwidth(class, (width - if previous { small } else { 0 }) as i32)?.min(254);
    Ok(i64::from(
        decode_bandwidth(class, raw + 1)? - decode_bandwidth(class, raw)?,
    ))
}

/// AFD74/A4A8C typed/keyboard path over the SINGLE shared BW codec. Limits are
/// editor metadata from407CA0, not a second raw-code lookup. Native selected
/// key1/phase2 returns Hz directly, without imposing the typed bounds/grid.
pub fn edit_bandwidth(
    image: &mut Icf<'_>,
    index: usize,
    input: NumberInput<'_>,
    crt_point: u8,
) -> Result<BandwidthEffect, Error> {
    use crate::icf::special::{decode_bandwidth, encode_bandwidth};
    let view = filter_view(image, index)?;
    let class = (index / 3) as u32;
    let old = i64::from(view.bandwidth_hz);
    let min = i64::from(decode_bandwidth(class, 0)?);
    let max = [3600, 3600, 3600, 2700, 10000][class as usize];
    let mut rewritten_text = None;
    let (candidate, accepted) = match input {
        NumberInput::Selected(event) if event.accepted && event.key == 1 && event.phase == 2 => {
            (i64::from(event.candidate), true)
        }
        NumberInput::Text(text) => {
            let (first, ok) = numeric_text(text, crt_point, 0, min, max, 1)?;
            if ok {
                let integer = first.to_string().into_bytes();
                let value = numeric_text(
                    &integer,
                    crt_point,
                    0,
                    min,
                    max,
                    bandwidth_step(class, first, false)?,
                )?
                .0;
                rewritten_text = Some(integer);
                (value, true)
            } else {
                (first, false)
            }
        }
        NumberInput::Next | NumberInput::Previous => {
            let step = bandwidth_step(class, old, input == NumberInput::Previous)?;
            let value = old.clamp(min, max)
                + if input == NumberInput::Next {
                    step
                } else {
                    -step
                };
            (
                if value < min {
                    max
                } else if value > max {
                    min
                } else {
                    value
                },
                true,
            )
        }
        _ => (old, false),
    };
    let mut notify_flags = 0;
    let raw = encode_bandwidth(class, candidate as i32)?;
    if accepted {
        let changed = raw != image.memory()[0x37850 + index];
        image.patch_memory(&[(0x37850 + index, &[raw])])?;
        if changed {
            image.patch_memory(&[(0x37867 + 2 * index, &[0, 0])])?;
            notify_flags = 0x20;
        }
    }
    Ok(BandwidthEffect {
        number: NumberEffect {
            stored: accepted,
            accepted,
            candidate,
            raw_candidate: i32::from(raw),
            state_out: 0,
            notify_flags,
            suffix: Vec::new(),
            gray: false,
        },
        popup_requested: input == NumberInput::Begin,
        rewritten_text,
    })
}

/// Full-byte selector cases. Private parent storage metadata stays private;
/// this child module supplies compiled schema and reuses its read/select codec.
#[derive(Debug)]
pub struct ChoiceField {
    storage: super::Setting,
    pub first_index: usize,
    pub count: usize,
}

const fn choice(
    id: &'static str,
    name: &'static str,
    key: u16,
    at: usize,
    domain: (i64, i64),
    axis: (usize, usize, usize),
) -> ChoiceField {
    ChoiceField {
        storage: super::Setting {
            feature_id: id,
            name,
            key,
            selection_key: 1,
            domain: super::Domain {
                min: domain.0,
                max: domain.1,
                step: 1,
            },
            count: axis.0 + axis.1,
            kind: Kind::Selector,
            offset: at,
            width: 1,
            signed: false,
            bias: 0,
            stride: axis.2,
        },
        first_index: axis.0,
        count: axis.1,
    }
}

static CHOICE_FIELDS: [ChoiceField; 24] = [
    choice(
        "A2-H54848-38",
        "display_language",
        0x224,
        0x396bb,
        (0, 1),
        (0, 1, 0),
    ),
    choice(
        "A2-H54848-95",
        "memory_pad_channels",
        0x251,
        0x39321,
        (0, 1),
        (0, 1, 0),
    ),
    choice(
        "A2-H56068-04",
        "cw_count_up_trigger_channel",
        0x2c4,
        0x3996e,
        (1, 8),
        (0, 1, 0),
    ),
    choice(
        "A2-H54848-109",
        "notch_width_ssb",
        0x299,
        0x399a7,
        (0, 2),
        (0, 1, 1),
    ),
    choice(
        "A2-H54848-110",
        "notch_width_cw",
        0x299,
        0x399a7,
        (0, 2),
        (1, 1, 1),
    ),
    choice(
        "A2-H54848-111",
        "notch_width_rtty",
        0x299,
        0x399a7,
        (0, 2),
        (2, 1, 1),
    ),
    choice(
        "A2-H54848-112",
        "notch_width_am",
        0x299,
        0x399a7,
        (0, 2),
        (3, 1, 1),
    ),
    choice(
        "A2-H56095-04",
        "scope_during_tx_center",
        0x363,
        0x39725,
        (0, 1),
        (0, 1, 0),
    ),
    choice(
        "A2-H56095-06",
        "scope_center_display",
        0x365,
        0x3972b,
        (0, 2),
        (0, 1, 0),
    ),
    choice(
        "A2-H56095-07",
        "scope_marker_position",
        0x366,
        0x3972c,
        (0, 1),
        (0, 1, 0),
    ),
    choice(
        "A2-H56117-01",
        "swr_step",
        0x355,
        0x379b9,
        (0, 3),
        (0, 17, 2),
    ),
    choice(
        "A2-H56117-02",
        "swr_bars",
        0x356,
        0x379ba,
        (0, 5),
        (0, 17, 2),
    ),
    choice(
        "A2-H54848-74",
        "transmit_delay",
        0x28f,
        0x39304,
        (0, 5),
        (0, 5, 1),
    ),
    choice(
        "A2-H54848-136",
        "mode_select_ssb",
        0x26c,
        0x3971a,
        (0, 1),
        (0, 1, 1),
    ),
    choice(
        "A2-H54848-136",
        "mode_select_cw",
        0x26d,
        0x3971a,
        (0, 1),
        (1, 1, 1),
    ),
    choice(
        "A2-H54848-136",
        "mode_select_rtty",
        0x26e,
        0x3971a,
        (0, 1),
        (2, 1, 1),
    ),
    choice(
        "A2-H54848-136",
        "mode_select_am",
        0x26f,
        0x3971a,
        (0, 1),
        (3, 1, 1),
    ),
    choice(
        "A2-H54848-136",
        "mode_select_fm",
        0x270,
        0x3971a,
        (0, 1),
        (4, 1, 1),
    ),
    choice(
        "A2-H54848-136",
        "mode_select_dv",
        0x271,
        0x3971a,
        (0, 1),
        (5, 1, 1),
    ),
    choice(
        "A2-H54848-136",
        "mode_select_wfm",
        0x272,
        0x3971a,
        (0, 1),
        (6, 1, 1),
    ),
    choice(
        "A2-H56399-14",
        "acc_send",
        0x2fa,
        0x39341,
        (0, 1),
        (0, 5, 1),
    ),
    choice(
        "A2-H55998-02",
        "picture_size",
        0x1b0,
        0x399f7,
        (0, 2),
        (0, 1, 0),
    ),
    choice(
        "A2-H55998-03",
        "picture_quality",
        0x1b1,
        0x399f8,
        (0, 2),
        (0, 1, 0),
    ),
    choice(
        "A2-H56371-03",
        "dr_noise_reduction",
        0x123,
        0x3c37c,
        (0, 1),
        (0, 1, 0),
    ),
];

pub fn choice_fields() -> &'static [ChoiceField] {
    &CHOICE_FIELDS
}

// Same codec/selector operation, with RF-specific index gates and linked stores
// in ChoiceField::edit. HF/50M filter-shape holes retain their literal indexes.
static RF_CHOICES: [ChoiceField; 20] = [
    choice(
        "A2-H56153-01",
        "receive_high_pass",
        0x344,
        0x392da,
        (0, 20),
        (0, 7, 1),
    ),
    choice(
        "A2-H56153-02",
        "receive_low_pass",
        0x345,
        0x392e2,
        (5, 25),
        (0, 7, 1),
    ),
    choice(
        "A2-H56153-07",
        "transmit_wide_low",
        0x34a,
        0x392ea,
        (0, 3),
        (0, 1, 1),
    ),
    choice(
        "A2-H56153-07",
        "transmit_wide_high",
        0x34a,
        0x392ea,
        (0, 3),
        (1, 1, 1),
    ),
    choice(
        "A2-H56153-08",
        "transmit_mid_low",
        0x34b,
        0x392ea,
        (0, 3),
        (2, 1, 1),
    ),
    choice(
        "A2-H56153-08",
        "transmit_mid_high",
        0x34b,
        0x392ea,
        (0, 3),
        (3, 1, 1),
    ),
    choice(
        "A2-H56153-09",
        "transmit_narrow_low",
        0x34c,
        0x392ea,
        (0, 3),
        (4, 1, 1),
    ),
    choice(
        "A2-H56153-09",
        "transmit_narrow_high",
        0x34c,
        0x392ea,
        (0, 3),
        (5, 1, 1),
    ),
    choice(
        "A2-H56153-10",
        "transmit_data_low",
        0x34d,
        0x392f0,
        (0, 3),
        (0, 1, 1),
    ),
    choice(
        "A2-H56153-10",
        "transmit_data_high",
        0x34d,
        0x392f0,
        (0, 3),
        (1, 1, 1),
    ),
    choice(
        "A2-H56180-01",
        "tuning_step",
        0x33f,
        0x37885,
        (0, 12),
        (0, 8, 1),
    ),
    choice(
        "A2-H56180-02",
        "air_tuning_step",
        0x341,
        0x3788d,
        (0, 12),
        (0, 1, 0),
    ),
    choice("A2-H56186-01", "agc", 0x336, 0x378a7, (0, 2), (0, 8, 1)),
    choice(
        "A2-H56186-02",
        "agc_fast_delay",
        0x339,
        0x3788f,
        (0, 13),
        (0, 8, 3),
    ),
    choice(
        "A2-H56186-02",
        "agc_mid_delay",
        0x33a,
        0x37890,
        (0, 13),
        (0, 8, 3),
    ),
    choice(
        "A2-H56186-02",
        "agc_slow_delay",
        0x33b,
        0x37891,
        (0, 13),
        (0, 8, 3),
    ),
    choice(
        "A2-H56192-01",
        "filter_shape_hf_narrow",
        0x330,
        0x3785f,
        (0, 1),
        (2, 1, 1),
    ),
    choice(
        "A2-H56192-02",
        "filter_shape_hf_wide",
        0x331,
        0x3785f,
        (0, 1),
        (0, 4, 1),
    ),
    choice(
        "A2-H56192-03",
        "filter_shape_vhf_narrow",
        0x332,
        0x3785f,
        (0, 1),
        (6, 1, 1),
    ),
    choice(
        "A2-H56192-04",
        "filter_shape_vhf_wide",
        0x333,
        0x3785f,
        (0, 1),
        (4, 4, 1),
    ),
];

pub fn rf_choice_fields() -> &'static [ChoiceField] {
    &RF_CHOICES
}

static KEY_CHOICES: [ChoiceField; 2] = [
    choice(
        "A2-H54848-135",
        "microphone_keys",
        0x26b,
        0x3970a,
        (0, 30),
        (0, 4, 1),
    ),
    choice(
        "A2-H54848-23",
        "bluetooth_keys",
        0x2ba,
        0x3970a,
        (0, 30),
        (5, 3, 1),
    ),
];
pub fn key_assignment_fields() -> &'static [ChoiceField] {
    &KEY_CHOICES
}

pub fn key_assignment_choices(
    image: &Icf<'_>,
    name: &str,
    index: usize,
    live_q: &crate::icf::context::Context,
) -> Result<Vec<i64>, Error> {
    let field = KEY_CHOICES
        .iter()
        .find(|f| f.name() == name || f.feature_id() == name)
        .ok_or_else(|| invalid("unknown key assignment field"))?;
    let old = field.read(image, index)?;
    Ok((0..=30)
        .filter(|&value| value == old || value != 24 || live_q.as_bytes()[0x11] != 0)
        .collect())
}
pub fn edit_key_assignment(
    image: &mut Icf<'_>,
    name: &str,
    index: usize,
    input: ChoiceInput,
    live_q: &crate::icf::context::Context,
) -> Result<ChoiceEffect, Error> {
    let values = key_assignment_choices(image, name, index, live_q)?;
    let field = KEY_CHOICES
        .iter()
        .find(|f| f.name() == name || f.feature_id() == name)
        .unwrap();
    let old = field.read(image, index)?;
    let event = choice_candidate(old, &values, input);
    field.edit(
        image,
        index,
        if let Some(candidate) = event {
            ChoiceInput::Selected(super::Selection {
                key: 1,
                phase: 2,
                candidate,
                accepted: true,
            })
        } else {
            input
        },
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionalField {
    Coordinates,
    Temperature,
    AutoRepeater,
    ScopeSpan,
}
impl ConditionalField {
    pub const ALL: [Self; 4] = [
        Self::Coordinates,
        Self::Temperature,
        Self::AutoRepeater,
        Self::ScopeSpan,
    ];
    fn descriptor(self) -> ChoiceField {
        match self {
            Self::Coordinates => choice(
                "A2-H54848-40",
                "coordinate_format",
                0x22a,
                0x396b4,
                (0, 2),
                (0, 1, 0),
            ),
            Self::Temperature => choice(
                "A2-H54848-43",
                "temperature_units",
                0x22d,
                0x396b7,
                (0, 1),
                (0, 1, 0),
            ),
            Self::AutoRepeater => choice(
                "A2-H54848-83",
                "auto_repeater",
                0x245,
                0x39312,
                (0, 2),
                (0, 1, 0),
            ),
            Self::ScopeSpan => choice(
                "A2-H56095-02",
                "scope_span",
                0x35f,
                0x377c7,
                (0, 7),
                (0, 1, 0),
            ),
        }
    }
    pub fn feature_id(self) -> &'static str {
        self.descriptor().feature_id()
    }
    pub fn key(self) -> u16 {
        self.descriptor().key()
    }
    /// Capture the actual resource branch. AutoRepeater JPN has codes0/2, not
    /// contiguous0/1/2; all three PE language resources share this exact schema.
    pub fn catalog(self, runtime: SelectorRuntime<'_>) -> Result<ConditionalCatalog, Error> {
        let jp = runtime.system_langid & 0x3ff == 0x11;
        let units = runtime.descriptor_8645fc == 1 || jp;
        let jpn = match self {
            Self::Coordinates | Self::Temperature => units,
            Self::ScopeSpan => jp,
            Self::AutoRepeater => {
                let k = runtime.q.selector()?;
                k == 0 || (k == 16 && jp)
            }
        };
        let (selector, values) = match (self, jpn) {
            (Self::Coordinates, true) => ("GI_Sheet_Sel_MenuMode_GpsFormat_JPN", &[0, 1, 2][..]),
            (Self::Coordinates, false) => ("GI_Sheet_Sel_MenuMode_GpsFormat", &[0, 1, 2][..]),
            (Self::Temperature, true) => ("GI_Sheet_Sel_Common_Temp_Units_JPN", &[0, 1][..]),
            (Self::Temperature, false) => ("GI_Sheet_Sel_Common_Temp_Units", &[0, 1][..]),
            (Self::AutoRepeater, true) => ("GI_Sheet_Sel_Common_AutoRepeater_JPN", &[0, 2][..]),
            (Self::AutoRepeater, false) => ("GI_Sheet_Sel_Common_AutoRepeater", &[0, 1, 2][..]),
            (Self::ScopeSpan, true) => (
                "GI_Sheet_Sel_Common_Scope_SpanJpn",
                &[0, 1, 2, 3, 4, 5, 6, 7][..],
            ),
            (Self::ScopeSpan, false) => (
                "GI_Sheet_Sel_Common_Scope_Span",
                &[0, 1, 2, 3, 4, 5, 6, 7][..],
            ),
        };
        Ok(ConditionalCatalog {
            field: self,
            selector,
            values,
        })
    }
}

#[derive(Clone, Copy)]
pub struct SelectorRuntime<'a> {
    pub system_langid: u16,
    pub descriptor_8645fc: u32,
    pub q: &'a crate::icf::context::Context,
}

/// Generated finite catalog, not a caller-supplied predicate or country code.
#[derive(Debug, Clone)]
pub struct ConditionalCatalog {
    field: ConditionalField,
    pub selector: &'static str,
    values: &'static [i64],
}
impl ConditionalCatalog {
    pub fn values(&self) -> &'static [i64] {
        self.values
    }
    pub fn field(&self) -> ConditionalField {
        self.field
    }
    pub fn edit(&self, image: &mut Icf<'_>, input: ChoiceInput) -> Result<ChoiceEffect, Error> {
        let field = self.field.descriptor();
        let old = field.read(image, 0)?;
        let event = choice_candidate(old, self.values, input);
        let mut out = field.edit(
            image,
            0,
            if let Some(candidate) = event {
                ChoiceInput::Selected(super::Selection {
                    key: 1,
                    phase: 2,
                    candidate,
                    accepted: true,
                })
            } else {
                input
            },
        )?;
        if self.field == ConditionalField::ScopeSpan {
            out.gray = image.memory()[0x377c6] != 0;
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChoiceInput {
    Display,
    Cancel,
    Selected(super::Selection),
    Next,
    Previous,
    Begin,
}

/// 9A67C choice calculation shared by packed cases without a Setting descriptor.
/// It returns the original full candidate; each case applies its storage codec.
pub fn choice_candidate(old: i64, choices: &[i64], input: ChoiceInput) -> Option<i32> {
    match input {
        ChoiceInput::Selected(event) if event.accepted && event.key == 1 && event.phase == 2 => {
            Some(event.candidate)
        }
        ChoiceInput::Next | ChoiceInput::Previous => {
            Some(match choices.iter().position(|&v| v == old) {
                None => choices[0],
                Some(at) => {
                    choices[if input == ChoiceInput::Next {
                        (at + 1) % choices.len()
                    } else {
                        (at + choices.len() - 1) % choices.len()
                    }]
                }
            } as i32)
        }
        _ => None,
    }
}

/// Packed/composite code domains: reuse shared storage codecs instead
/// of reproducing their masks. Extra selected bits are projected by each domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackedField {
    Vox,
    OneHz,
    SelectMemoryScan,
    DeltaFSpan,
    ScopeMode,
}
impl PackedField {
    pub const ALL: [Self; 5] = [
        Self::Vox,
        Self::OneHz,
        Self::SelectMemoryScan,
        Self::DeltaFSpan,
        Self::ScopeMode,
    ];
    pub fn feature_id(self) -> &'static str {
        match self {
            Self::Vox => "A2-H54848-66",
            Self::OneHz => "A2-H54848-97",
            Self::SelectMemoryScan => "A2-H54848-126",
            Self::DeltaFSpan => "A2-H54848-124",
            Self::ScopeMode => "A2-H56095-01",
        }
    }
    pub fn key(self) -> u16 {
        match self {
            Self::Vox => 0x25b,
            Self::OneHz => 0x253,
            Self::SelectMemoryScan => 0x2a8,
            Self::DeltaFSpan => 0x2a6,
            Self::ScopeMode => 0x35e,
        }
    }
    pub fn read(self, image: &Icf<'_>) -> Result<i64, Error> {
        Ok(match self {
            Self::Vox => i64::from(image.vox()?),
            Self::OneHz => i64::from(image.one_hz_mode()?),
            Self::SelectMemoryScan => image.select_memory_scan()? as i64,
            Self::DeltaFSpan => i64::from(image.delta_f_span_raw()),
            Self::ScopeMode => i64::from(image.scope_mode_raw()),
        })
    }
    /// Only the scan-span label resource depends on system LANGID; its code
    /// domain0..6 is equal in all three checked language resource generations.
    pub fn selector(self, system_langid: u16) -> Option<&'static str> {
        (self == Self::DeltaFSpan).then_some(if system_langid & 0x3ff == 0x11 {
            "GI_Sheet_Sel_Common_dFSpanJpn"
        } else {
            "GI_Sheet_Sel_Common_dFSpan"
        })
    }
    pub fn edit(self, image: &mut Icf<'_>, input: ChoiceInput) -> Result<ChoiceEffect, Error> {
        use crate::icf::SelectMemoryScan;
        let old = self.read(image)?;
        let candidate = choice_candidate(
            old,
            match self {
                Self::SelectMemoryScan | Self::ScopeMode => &[0, 1, 2, 3],
                Self::DeltaFSpan => &[0, 1, 2, 3, 4, 5, 6],
                _ => &[0, 1],
            },
            input,
        );
        if let Some(value) = candidate {
            match self {
                Self::Vox => image.set_vox(value & 1 != 0)?,
                Self::OneHz => image.set_one_hz_mode(value & 1 != 0)?,
                Self::DeltaFSpan => image.set_delta_f_span_raw(value),
                Self::ScopeMode => image.set_scope_mode_raw(value),
                Self::SelectMemoryScan => image.set_select_memory_scan(
                    [
                        SelectMemoryScan::AllStars,
                        SelectMemoryScan::OneStar,
                        SelectMemoryScan::TwoStars,
                        SelectMemoryScan::ThreeStars,
                    ][(value & 3) as usize],
                )?,
            }
        }
        Ok(ChoiceEffect {
            stored: candidate.is_some(),
            value: self.read(image)?,
            notify_flags: if candidate.is_some() && self == Self::ScopeMode {
                0x20
            } else {
                0
            },
            gray: false,
            popup_requested: input == ChoiceInput::Begin,
            fixed_label: None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChoiceEffect {
    pub stored: bool,
    pub value: i64,
    pub notify_flags: u32,
    pub gray: bool,
    pub popup_requested: bool,
    /// Fixed AGC/delay rows do not read a raw byte or call the selector.
    pub fixed_label: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmplifierField {
    Preamp,
    Attenuator,
}
impl AmplifierField {
    pub const ALL: [Self; 2] = [Self::Preamp, Self::Attenuator];
    pub fn feature_id(self) -> &'static str {
        match self {
            Self::Preamp => "A2-H56137-01",
            Self::Attenuator => "A2-H56137-02",
        }
    }
    pub fn key(self) -> u16 {
        match self {
            Self::Preamp => 0x34f,
            Self::Attenuator => 0x350,
        }
    }
    /// Resource visibility is rf_row_visible(PreampAttenuator); gray PAMP still
    /// reaches its helper. ATT13..16 is fixed and reads no raw setting value.
    pub fn edit(
        self,
        image: &mut Icf<'_>,
        index: usize,
        input: ChoiceInput,
    ) -> Result<ChoiceEffect, Error> {
        if index >= 17 {
            return Err(invalid("PAMP/ATT index outside native17rows"));
        }
        if self == Self::Attenuator && index >= 13 {
            return Ok(ChoiceEffect {
                stored: false,
                value: 0,
                notify_flags: 0,
                gray: true,
                popup_requested: false,
                fixed_label: Some("---"),
            });
        }
        let old = if self == Self::Preamp {
            image.preamp_raw(index)?
        } else {
            image.attenuator_raw(index)?
        };
        let choices = if self == Self::Preamp && index < 13 {
            &[0, 1, 2][..]
        } else {
            &[0, 1][..]
        };
        let candidate = choice_candidate(i64::from(old), choices, input);
        if let Some(value) = candidate {
            match self {
                Self::Preamp => image.set_preamp_raw(index, value)?,
                Self::Attenuator => image.set_attenuator_raw(index, value)?,
            }
        }
        let value = if self == Self::Preamp {
            image.preamp_raw(index)?
        } else {
            image.attenuator_raw(index)?
        };
        Ok(ChoiceEffect {
            stored: candidate.is_some(),
            value: i64::from(value),
            notify_flags: if candidate.is_some() && self == Self::Attenuator {
                0x20
            } else {
                0
            },
            gray: self == Self::Preamp && image.attenuator_raw(index)? != 0,
            popup_requested: input == ChoiceInput::Begin,
            fixed_label: None,
        })
    }
}

impl ChoiceField {
    pub fn feature_id(&self) -> &'static str {
        self.storage.feature_id
    }
    pub fn name(&self) -> &'static str {
        self.storage.name
    }
    pub fn key(&self) -> u16 {
        self.storage.key
    }
    pub fn domain(&self) -> super::Domain {
        self.storage.domain
    }
    pub fn accepts_index(&self, index: usize) -> bool {
        self.check(index).is_ok()
    }
    fn check(&self, index: usize) -> Result<(), Error> {
        if index < self.first_index || index - self.first_index >= self.count {
            return Err(invalid("choice index outside original native axis"));
        }
        if (matches!(self.key(), 0x344 | 0x345) && index == 3)
            || (self.key() == 0x331 && index == 2)
            || (self.key() == 0x333 && index == 6)
        {
            return Err(invalid("choice index is a native layout hole"));
        }
        Ok(())
    }
    pub fn fixed_label(&self, index: usize) -> Result<Option<&'static str>, Error> {
        self.check(index)?;
        Ok(if index >= 4 {
            match self.key() {
                0x336 => Some("FAST"),
                0x339 => Some("0.1sec"),
                0x33a | 0x33b => Some("---"),
                _ => None,
            }
        } else {
            None
        })
    }
    pub fn read(&self, image: &Icf<'_>, index: usize) -> Result<i64, Error> {
        self.check(index)?;
        if self.fixed_label(index)?.is_some() {
            return Ok(0);
        }
        let Value::Number(value) = self.storage.read(image, index)?;
        Ok(value)
    }
    /// Dispatch only real case effects. Palette, Only_* resource generation and
    /// the common document tail are not manufactured by this field operation.
    pub fn edit(
        &self,
        image: &mut Icf<'_>,
        index: usize,
        input: ChoiceInput,
    ) -> Result<ChoiceEffect, Error> {
        if matches!(self.key(), 0x26b | 0x2ba)
            && matches!(input, ChoiceInput::Next | ChoiceInput::Previous)
        {
            return Err(invalid("key assignment cycling requires actual live Q"));
        }
        let old = self.read(image, index)?;
        let fixed_label = self.fixed_label(index)?;
        let effect = match if fixed_label.is_some() {
            ChoiceInput::Display
        } else {
            input
        } {
            ChoiceInput::Selected(event) => self.storage.select(image, index, event)?,
            ChoiceInput::Next => self.storage.cycle(image, index, super::Step::Next)?,
            ChoiceInput::Previous => self.storage.cycle(image, index, super::Step::Previous)?,
            ChoiceInput::Display | ChoiceInput::Cancel | ChoiceInput::Begin => super::Effect {
                stored: false,
                notify_flags: 0,
            },
        };
        // B085C follows picture size/quality's first successful byte store.
        if effect.stored && matches!(self.key(), 0x1b0 | 0x1b1) {
            image.patch_memory(&[(0x37d87, &[1])])?;
        }
        let mut notify_flags = effect.notify_flags;
        if effect.stored && matches!(self.key(), 0x344 | 0x345) {
            let hpf = image.memory()[0x392da + index];
            let lpf = image.memory()[0x392e2 + index];
            if self.key() == 0x344 && hpf >= lpf {
                image.patch_memory(&[(0x392e2 + index, &[hpf.wrapping_add(1)])])?;
                notify_flags |= 0x20;
            } else if self.key() == 0x345 && lpf <= hpf {
                image.patch_memory(&[(0x392da + index, &[lpf.wrapping_sub(1)])])?;
                notify_flags |= 0x20;
            }
            if matches!(index, 0 | 1 | 2 | 4)
                && (if self.key() == 0x344 {
                    image.memory()[0x392da + index] != 0
                } else {
                    image.memory()[0x392e2 + index] != 25
                })
            {
                image.patch_memory(&[(0x392ca + index, &[5]), (0x392d2 + index, &[5])])?;
                notify_flags |= 0x20;
            }
        }
        let gray = self.key() == 0x224 && image.memory()[0x396bc] != 1;
        Ok(ChoiceEffect {
            stored: effect.stored,
            value: if effect.stored {
                self.read(image, index)?
            } else {
                old
            },
            notify_flags,
            gray,
            popup_requested: fixed_label.is_none() && input == ChoiceInput::Begin,
            fixed_label,
        })
    }
}

/// 11F/6ABB3 -> 9A1CC, sixteen channels. Typed E/F aliases survive filtering
/// and become */#. One code is one byte; the existing canonical storage codec
/// performs the FF padding rather than a second packed-nibble implementation.
pub fn edit_dtmf(
    image: &mut Icf<'_>,
    channel: usize,
    input: TextInput<'_>,
) -> Result<TextEffect, Error> {
    let raw = image.dtmf_memory_raw(channel)?;
    let labels = b"0123456789ABCD*#";
    let old: Vec<_> = raw
        .iter()
        .take_while(|&&b| b != 0xff)
        .map(|b| labels[usize::from(b & 15)])
        .collect();
    let (stored, state_out, display) = match input {
        TextInput::Display | TextInput::Cancel => (false, 0, old),
        TextInput::Begin(first) => (false, 2, first.map_or(old, |b| vec![b])),
        TextInput::Text(text) => {
            let value: Vec<_> = text
                .iter()
                .copied()
                .take_while(|&b| b != 0)
                .filter(|b| b.is_ascii_hexdigit() || matches!(b, b'*' | b'#'))
                .take(24)
                .map(|b| match b.to_ascii_uppercase() {
                    b'E' => b'*',
                    b'F' => b'#',
                    b => b,
                })
                .collect();
            image.set_dtmf_memory(channel, &value)?;
            (true, 0, value)
        }
    };
    Ok(TextEffect {
        stored,
        state_out,
        notify_flags: 0,
        display,
    })
}

/// 2D1/7753B -> D497C -> 9A124. The eight 70-byte rows uppercase ASCII,
/// replace rather than delete foreign bytes, and turn backslash into raw CR.
pub fn edit_rtty(
    image: &mut Icf<'_>,
    channel: usize,
    input: TextInput<'_>,
) -> Result<TextEffect, Error> {
    let raw = image.rtty_memory_raw(channel)?;
    let display = |raw: &[u8]| {
        let raw = raw.split(|&b| b == 0).next().unwrap_or_default();
        let end = raw.iter().rposition(|&b| b > 0x20).map_or(0, |p| p + 1);
        raw[..end]
            .iter()
            .map(|&b| if b == b'\r' { b'\\' } else { b })
            .collect::<Vec<_>>()
    };
    let old = display(raw);
    let (stored, state_out, display) = match input {
        TextInput::Display | TextInput::Cancel => (false, 0, old),
        TextInput::Begin(first) => (false, 2, first.map_or(old, |b| vec![b])),
        TextInput::Text(text) => {
            let mut value = [b' '; 70];
            for (out, &b) in value.iter_mut().zip(text) {
                let b = if b == b'\\' {
                    b'\r'
                } else {
                    b.to_ascii_uppercase()
                };
                *out = if b.is_ascii_uppercase()
                    || b.is_ascii_digit()
                    || b" !$&?\"'-/.,:;()\r".contains(&b)
                {
                    b
                } else {
                    b' '
                };
            }
            image.set_rtty_memory(channel, &value)?;
            (true, 0, display(&value))
        }
    };
    Ok(TextEffect {
        stored,
        state_out,
        notify_flags: 0,
        display,
    })
}

fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 0, reason }
}

/// Mode0 text invocations retain the shared source buffer's observable prefix
/// on native failure. Cancellation is not a phase1/2 text acceptance.
pub enum FixedTextInput<'a> {
    Display,
    Cancel,
    Begin,
    Text { source: &'a mut [u8], phase: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedTextField {
    GatewayAddress,
    GatewayCallsign,
    NtpAddress,
    Comment,
    PictureRecipient,
    NetworkName,
    Ssid,
    ApPassword,
    NetworkRadioName,
    Keyer,
}
impl FixedTextField {
    pub const ALL: [Self; 10] = [
        Self::GatewayAddress,
        Self::GatewayCallsign,
        Self::NtpAddress,
        Self::Comment,
        Self::PictureRecipient,
        Self::NetworkName,
        Self::Ssid,
        Self::ApPassword,
        Self::NetworkRadioName,
        Self::Keyer,
    ];
    pub fn feature_id(self) -> &'static str {
        match self {
            Self::GatewayAddress => "A2-H55976-02",
            Self::GatewayCallsign => "A2-H55976-03",
            Self::NtpAddress => "A2-H54848-142",
            Self::Comment => "A2-H54848-148",
            Self::PictureRecipient => "A2-H55998-04",
            Self::NetworkName => "A2-H56202-03",
            Self::Ssid => "A2-H56202-10",
            Self::ApPassword => "A2-H56202-11",
            Self::NetworkRadioName => "A2-H56202-19",
            Self::Keyer => "A2-H56064-01",
        }
    }
    pub fn key(self) -> u16 {
        match self {
            Self::GatewayAddress => 0x1a7,
            Self::GatewayCallsign => 0x1a8,
            Self::NtpAddress => 0x227,
            Self::Comment => 0x2b0,
            Self::PictureRecipient => 0x1b2,
            Self::NetworkName => 0x321,
            Self::Ssid => 0x31e,
            Self::ApPassword => 0x31f,
            Self::NetworkRadioName => 0x32e,
            Self::Keyer => 0x2c0,
        }
    }
    pub fn width(self) -> usize {
        match self {
            Self::GatewayAddress | Self::NtpAddress => 64,
            Self::GatewayCallsign | Self::PictureRecipient => 8,
            Self::Comment | Self::NetworkRadioName => 16,
            Self::NetworkName => 15,
            Self::Ssid => 32,
            Self::ApPassword => 63,
            Self::Keyer => 70,
        }
    }
    pub fn count(self) -> usize {
        if self == Self::Keyer { 8 } else { 1 }
    }
    fn at(self, index: usize) -> Result<usize, crate::csv::Error> {
        if index >= self.count() {
            return Err(crate::csv::Error::Invalid(
                "fixed text index outside native axis",
            ));
        }
        Ok(match self {
            Self::GatewayAddress => 0x39562,
            Self::GatewayCallsign => 0x395a2,
            Self::NtpAddress => 0x396be,
            Self::Comment => 0x35591,
            Self::PictureRecipient => 0x399f9,
            Self::NetworkName => 0x3936c,
            Self::Ssid => 0x39a07,
            Self::ApPassword => 0x39a27,
            Self::NetworkRadioName => 0x39551,
            Self::Keyer => 0x37158 + 70 * index,
        })
    }
    fn profile(self) -> crate::csv::nls::Profile<'static> {
        const ALL: &[u8] = b"!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";
        // Network Name exception RVA408FF3, length23, SHA2567d3c5cd5...83b2b4.
        let punctuation = match self {
            Self::GatewayAddress | Self::NtpAddress => b"-.".as_slice(),
            Self::GatewayCallsign | Self::PictureRecipient => b"",
            Self::NetworkName => b"!#$%&\"`^+-.,;=()[]{}_~@",
            Self::NetworkRadioName => b"!\"#$%&'()*+,-./:;<=>?@[]^_`{|}~",
            Self::Keyer => b"*,./?@^",
            _ => ALL,
        };
        crate::csv::nls::Profile {
            uppercase: matches!(
                self,
                Self::GatewayCallsign | Self::PictureRecipient | Self::NetworkName | Self::Keyer
            ),
            keep_kana: matches!(self, Self::Ssid | Self::NetworkRadioName),
            keep_spaces: !matches!(self, Self::GatewayAddress | Self::NtpAddress),
            punctuation_exceptions: punctuation,
            maximum: self.width(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedTextEffect {
    pub text: TextEffect,
    pub context60: Option<u32>,
    pub warning_bits: u32,
    pub marked: bool,
}

/// Execute the portable ASCII NLS domain, not host locale emulation. Non-ASCII
/// requires actual native outputs through the explicit entry below; it never
/// silently falls back to ASCII, UTF-8, an identity map or empty text.
pub fn edit_fixed_text(
    image: &mut Icf<'_>,
    field: FixedTextField,
    index: usize,
    input: FixedTextInput<'_>,
    alignment_option: u32,
) -> Result<FixedTextEffect, crate::csv::Error> {
    edit_fixed_text_with_nls(
        image,
        field,
        index,
        input,
        alignment_option,
        crate::csv::nls::ascii_map,
        |bytes| {
            if !bytes.is_ascii() {
                return Err(crate::csv::Error::SourceUnavailable(
                    "non-ASCII CharUpperBuffA output required",
                ));
            }
            bytes.make_ascii_uppercase();
            Ok(bytes.len() as u32)
        },
    )
}

/// Raw VA864634 controls phase1 callsign alignment. NLS callbacks are the exact
/// LCMapStringA2n/CharUpperBuffAn boundaries consumed by csv::nls::whole_text.
/// A native Err leaves prior shared sanitation/map writes visible, but no image
/// store occurs. Native zero is not an acceptance or output-length gate.
pub fn edit_fixed_text_with_nls(
    image: &mut Icf<'_>,
    field: FixedTextField,
    index: usize,
    input: FixedTextInput<'_>,
    alignment_option: u32,
    map: impl FnMut(&[u8], &mut [u8]) -> Result<i32, crate::csv::Error>,
    upper: impl FnMut(&mut [u8]) -> Result<u32, crate::csv::Error>,
) -> Result<FixedTextEffect, crate::csv::Error> {
    use crate::csv::nls;
    let at = field.at(index)?;
    let old = image.memory()[at..at + field.width()]
        .split(|&b| b == 0)
        .next()
        .unwrap()
        .to_vec();
    let state_out = u32::from(matches!(input, FixedTextInput::Begin)) * 2;
    let cancelled = matches!(input, FixedTextInput::Cancel);
    let mut stored = false;
    let mut candidate = old;
    if let FixedTextInput::Text { source, phase } = input {
        if !matches!(phase, 1 | 2) {
            return Err(crate::csv::Error::Invalid("fixed text requires phase1 or2"));
        }
        candidate = nls::whole_text(source, field.profile(), map, upper)?;
        stored = field != FixedTextField::Ssid || !candidate.is_empty();
        if stored
            && matches!(
                field,
                FixedTextField::GatewayCallsign | FixedTextField::PictureRecipient
            )
        {
            candidate.resize(8, b' ');
            if alignment_option != 0 && phase == 1 {
                align_callsign(candidate.as_mut_slice().try_into().unwrap());
            }
            if field == FixedTextField::GatewayCallsign && candidate != [b' '; 8] {
                stored = gateway_callsign_valid(&candidate, image.memory()[0x395aa]);
            }
        }
        if stored && field == FixedTextField::NetworkName {
            while candidate.first().is_some_and(|b| *b == b'.' || *b <= 0x20) {
                candidate.remove(0);
            }
        }
        if stored && field == FixedTextField::ApPassword {
            let trimmed = crate::csv::trim(&candidate);
            stored = trimmed.is_empty() || trimmed.len() >= 8;
        }
    }
    //9B80C runs after a false base helper too, except an ESC that creates no
    //invocation. Only the current row is repaired; unknown sibling tails survive.
    if field == FixedTextField::Keyer && !cancelled {
        let other = (0..8).filter(|&i| i != index).any(|i| {
            image
                .keyer_memory_raw(i)
                .unwrap()
                .split(|&b| b == 0)
                .next()
                .unwrap()
                .contains(&b'*')
        });
        let mut star = other;
        for byte in &mut candidate {
            if *byte == b'*' {
                if star {
                    *byte = b' ';
                    stored = true;
                }
                star = true;
            }
        }
    }
    if stored {
        let mut raw = vec![b' '; field.width()];
        raw[..candidate.len()].copy_from_slice(&candidate);
        image
            .patch_memory(&[(at, &raw)])
            .map_err(|_| crate::csv::Error::Invalid("fixed text image store"))?;
    }
    let marked =
        field == FixedTextField::PictureRecipient && candidate == [b' '; 8] && state_out != 2;
    let display = if field == FixedTextField::ApPassword {
        vec![b'*'; candidate.len()]
    } else {
        candidate
    };
    Ok(FixedTextEffect {
        text: TextEffect {
            stored,
            state_out,
            notify_flags: if stored && field == FixedTextField::ApPassword {
                0x20
            } else {
                0
            },
            display,
        },
        context60: if field == FixedTextField::ApPassword {
            Some(3)
        } else {
            None
        },
        warning_bits: if field == FixedTextField::ApPassword
            && super::find("CL_Common_Network_WLAN")
                .map_err(|_| crate::csv::Error::Invalid("compiled WLAN descriptor unavailable"))?
                .control_state(image, 0, false)
                .map_err(|_| crate::csv::Error::Invalid("WLAN control-state read"))?
                .marked
        {
            2
        } else {
            0
        },
        marked,
    })
}

pub fn align_callsign(bytes: &mut [u8; 8]) {
    if let Some(end) = (1..8).rev().find(|&i| bytes[i] != b' ')
        && let Some(gap) = (1..end).rev().find(|&i| bytes[i] == b' ')
    {
        let start = gap + 1;
        let width = end - start + 1;
        bytes.copy_within(start..=end, 8 - width);
        bytes[start..8 - width].fill(b' ');
    }
}

fn gateway_callsign_valid(c: &[u8], prefix_gate: u8) -> bool {
    if !c[7].is_ascii_uppercase() || b"GS".contains(&c[7]) {
        return false;
    }
    let digit = c[2].is_ascii_digit();
    let upper = |b: u8| b.is_ascii_uppercase();
    if (&c[..2] == b"JP" && digit && c[3] == b'Y' && upper(c[4]) && upper(c[5]))
        || (&c[..2] == b"JR" && digit && b"VW".contains(&c[3]) && upper(c[4]) && c[5] == b' ')
        || (&c[..4] == b"JR6Y" && upper(c[4]) && c[5] == b' ')
        || (&c[..4] == b"JQ6Y" && upper(c[4]) && upper(c[5]))
    {
        return false;
    }
    prefix_gate != 0
        || (c[0] == b'J' && (b'A'..=b'S').contains(&c[1]))
        || (b"78".contains(&c[0]) && (b'J'..=b'N').contains(&c[1]))
}

/// Two read-only metadata cells; clear is the separate EB/EC command, not a
/// fabricated name editor or the IP-list/image-file import producer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataField {
    TransmitPicture,
    ConnectionIpList,
}
impl MetadataField {
    pub const ALL: [Self; 2] = [Self::TransmitPicture, Self::ConnectionIpList];
    pub fn feature_id(self) -> &'static str {
        match self {
            Self::TransmitPicture => "A2-H55998-01",
            Self::ConnectionIpList => "A2-H65704-02",
        }
    }
    pub fn key(self) -> u16 {
        match self {
            Self::TransmitPicture => 0x1af,
            Self::ConnectionIpList => 0x1a5,
        }
    }
    pub fn clear_command(self) -> u32 {
        match self {
            Self::TransmitPicture => 0xeb,
            Self::ConnectionIpList => 0xec,
        }
    }
    fn at(self) -> usize {
        match self {
            Self::TransmitPicture => 0x37d89,
            Self::ConnectionIpList => 0x399c1,
        }
    }
    /// B03C8/B05D4 bounded28/NUL metadata, before common47CF0 language rendering.
    /// ExtractFileExt searches the last '.', backslash or colon; Pos then removes
    /// the FIRST occurrence of that extension, rather than simply truncating at
    /// its last dot. The empty label itself is a caller/resource presentation.
    pub fn read(self, image: &Icf<'_>) -> MetadataView {
        let name = image.memory()[self.at()..self.at() + 28]
            .split(|&b| b == 0)
            .next()
            .unwrap()
            .to_vec();
        let mut stem = name.clone();
        if let Some(at) = name.iter().rposition(|b| b".\\:".contains(b))
            && name[at] == b'.'
        {
            let ext = &name[at..];
            if let Some(first) = name.windows(ext.len()).position(|w| w == ext) {
                stem.truncate(first);
            }
        }
        MetadataView {
            attached: !name.is_empty(),
            name,
            stem,
            read_only: true,
        }
    }
    /// The command gate is explicit; false does not touch even the first byte.
    pub fn clear(self, image: &mut Icf<'_>, command: u32) -> Result<super::Effect, Error> {
        let stored = command == self.clear_command();
        if stored {
            match self {
                Self::TransmitPicture => image.patch_memory(&[
                    (0x37d89, &[0]),
                    (0x37da5, &[0, 255]),
                    (0x37da7, &[0; 4]),
                ])?,
                Self::ConnectionIpList => image.patch_memory(&[(0x399c1, &[0])])?,
            }
        }
        Ok(super::Effect {
            stored,
            notify_flags: 0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataView {
    pub attached: bool,
    pub name: Vec<u8>,
    pub stem: Vec<u8>,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeVfoEffect {
    pub number: NumberEffect,
    pub warning_bits: u32,
    pub empty_marker: bool,
    pub outside_marker: bool,
}

/// Home37D performs the actual two range queries and re-parses the SAME source.
/// B is an already-generated owner, Q is the separate live warning context.
/// Empty phase1 text is A5184's explicit zero, not a missing source fallback.
pub fn edit_home_vfo(
    image: &mut Icf<'_>,
    ranges: &crate::icf::frequency::Ranges,
    live_q: &crate::icf::context::Context,
    input: NumberInput<'_>,
    crt_point: u8,
    old_warning_bits: u32,
) -> Result<HomeVfoEffect, Error> {
    if !ranges.is_complete() {
        return Err(invalid("Home VFO requires completed B generation"));
    }
    const FIELD: super::Setting = number(
        "A2-H54848-11",
        "home_vfo",
        0x37d,
        0x392f6,
        (4, 0),
        (0, u32::MAX as i64, 1, "Hz"),
        (1, 0),
    );
    let Value::Number(old) = FIELD.read(image, 0)?;
    let state_out = if input == NumberInput::Begin { 2 } else { 0 };
    let (candidate, accepted) = match input {
        NumberInput::Text(b"") => (0, true),
        NumberInput::Text(text) => {
            let first = ranges.f760(2, -1, 0);
            let (mut value, accepted) = numeric_text(
                text,
                crt_point,
                6,
                i64::from(first.low),
                i64::from(first.high),
                1,
            )?;
            if accepted && value != 0 {
                let second = ranges.f760(2, ranges.edf0(2, value as u32), 0);
                //The second bool is deliberately not a save gate.
                value = numeric_text(
                    text,
                    crt_point,
                    6,
                    i64::from(second.low),
                    i64::from(second.high),
                    1,
                )?
                .0;
            }
            (value, accepted)
        }
        _ => (old, false),
    };
    if accepted {
        FIELD.store_numeric_raw(image, 0, candidate as i32)?;
    }
    let mut warning_bits = old_warning_bits;
    if candidate != 0 {
        let Value::Number(current) = FIELD.read(image, 0)?;
        let current = current as u32;
        if current != 0 {
            if ranges.eb64(live_q, current) {
                warning_bits |= 4;
            }
            let absolute = ranges.f760(2, -1, 0);
            if !(absolute.low..=absolute.high).contains(&current) {
                warning_bits |= 8;
            }
        }
    }
    Ok(HomeVfoEffect {
        number: NumberEffect {
            stored: accepted,
            accepted,
            candidate,
            raw_candidate: candidate as i32,
            state_out,
            notify_flags: 0,
            suffix: Vec::new(),
            gray: false,
        },
        warning_bits,
        empty_marker: candidate == 0 && state_out != 2,
        outside_marker: matches!(input, NumberInput::Display)
            && candidate != 0
            && warning_bits & 8 != 0,
    })
}

/// Captured source binding and the two distinct live B/Q roles for range edits.
#[derive(Clone, Copy)]
pub struct FrequencyRuntime<'a> {
    pub ranges: &'a crate::icf::frequency::Ranges,
    pub live_q: &'a crate::icf::context::Context,
    pub binding: u32,
    pub crt_point: u8,
}

/// Only_* filters consume the resource-generation Q, not a gray/editor guard.
/// Calling this never renumbers literal native slots or regenerates B.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RfLayout {
    Reference,
    FixedEdges,
    PreampAttenuator,
    AirTuningStep,
}
pub fn rf_row_visible(
    q: &crate::icf::context::Context,
    layout: RfLayout,
    index: usize,
) -> Result<bool, Error> {
    let count = match layout {
        RfLayout::Reference | RfLayout::PreampAttenuator => 17,
        RfLayout::FixedEdges => 68,
        RfLayout::AirTuningStep => 1,
    };
    if index >= count {
        return Err(invalid("RF layout row outside native axis"));
    }
    let skip = match layout {
        RfLayout::Reference | RfLayout::PreampAttenuator => {
            if (10..=12).contains(&index) {
                q.condition_58bdc()?
            } else {
                (13..=14).contains(&index) && q.condition_58c5c()?
            }
        }
        RfLayout::FixedEdges => {
            if (40..=51).contains(&index) {
                q.condition_58bdc()?
            } else {
                (52..=59).contains(&index) && q.condition_58c5c()?
            }
        }
        RfLayout::AirTuningStep => q.condition_58c5c()?,
    };
    Ok(!skip)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeSide {
    Low,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedEdgeEffect {
    pub stored: bool,
    pub helper_accepted: bool,
    pub candidate_hz: u32,
    /// A522C X before bounds clamp is separate from Y, and both are checked.
    pub raw_candidate_hz: u32,
    pub pair_hz: [u32; 2],
    pub range_bad: [bool; 2],
    pub absolute_bad: [bool; 2],
    pub state_out: u32,
    pub notify_flags: u32,
}

fn fixed_edge_warnings(
    holder: &crate::icf::record::DecodedRecord,
    ranges: &crate::icf::frequency::Ranges,
    q: &crate::icf::context::Context,
    numeric_only: bool,
) -> Result<[bool; 2], Error> {
    if holder.kind() != 0x16 {
        return Err(invalid("FIX predicate requires kind16"));
    }
    if !holder.valid() {
        return Ok([false; 2]);
    }
    let low = u32::from_le_bytes(holder.as_bytes()[0x138..0x13c].try_into().unwrap());
    let high = holder.channel_field(crate::icf::record::ChannelField::FrequencyHz);
    if numeric_only {
        let absolute = ranges.f760(2, -1, 0);
        return Ok([low, high].map(|value| !(absolute.low..=absolute.high).contains(&value)));
    }
    if !ranges.is_complete() {
        return Err(invalid("FIX predicate requires completed B generation"));
    }
    let auxiliary = i32::from_le_bytes(holder.as_bytes()[0x38..0x3c].try_into().unwrap());
    let range = ranges.f760(0x16, auxiliary, 0);
    let low_bad = low < range.low || low > range.high || ranges.ed3c(q, low);
    let high_bad =
        (!low_bad && low > high) || high < range.low || high > range.high || ranges.ed3c(q, high);
    Ok([low_bad, high_bad])
}

/// Kind16 C9BC8, or the broad closed-bound check for both endpoints.
/// The decoded holder is borrowed directly; no storage signature gate is added.
pub fn fixed_edge_bad(
    holder: &crate::icf::record::DecodedRecord,
    ranges: &crate::icf::frequency::Ranges,
    q: &crate::icf::context::Context,
    numeric_only: bool,
) -> Result<bool, Error> {
    Ok(fixed_edge_warnings(holder, ranges, q, numeric_only)?
        .into_iter()
        .any(|bad| bad))
}

pub fn edit_fixed_edge(
    image: &mut Icf<'_>,
    runtime: FrequencyRuntime<'_>,
    index: usize,
    side: EdgeSide,
    input: NumberInput<'_>,
) -> Result<FixedEdgeEffect, Error> {
    edit_fixed_edge_with_store(image, runtime, index, side, input, |_, _| Ok(()))
}

/// Kind16h's real load/save/reload adapter; no raw8 codec is reimplemented.
/// before_store is an ordered failure cutpoint, never an approval predicate.
pub fn edit_fixed_edge_with_store(
    image: &mut Icf<'_>,
    runtime: FrequencyRuntime<'_>,
    index: usize,
    side: EdgeSide,
    input: NumberInput<'_>,
    before_store: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<FixedEdgeEffect, Error> {
    use crate::icf::{
        record::{self, ChannelField},
        record_image,
    };
    if index >= 68 {
        return Err(invalid("FIX edge index outside native68rows"));
    }
    if !runtime.ranges.is_complete() {
        return Err(invalid("FIX edge requires completed B generation"));
    }
    let mut holder = record::reset_holder(0x16, index as u32, runtime.binding);
    record_image::load_range_into(image.memory(), 0x16, index as u32, &mut holder)?;
    let old_low = u32::from_le_bytes(holder.as_bytes()[0x138..0x13c].try_into().unwrap());
    let old_high = holder.channel_field(ChannelField::FrequencyHz);
    let range = runtime.ranges.f760(0x16, (index / 4) as i32, 0);
    let (min, max) = match side {
        EdgeSide::Low => (range.low, range.high.wrapping_sub(5000)),
        EdgeSide::High => (range.low.wrapping_add(5000), range.high),
    };
    let old = if side == EdgeSide::Low {
        old_low
    } else {
        old_high
    };
    let (raw, candidate, helper_accepted) = if let NumberInput::Text(text) = input {
        let (raw, accepted) = numeric_value(text, runtime.crt_point, 6, 1)?;
        //Low comparison precedes high comparison; do not panic on reversed B.
        let candidate = if raw < i64::from(min) {
            min
        } else if raw > i64::from(max) {
            max
        } else {
            raw as u32
        };
        (raw as u32, candidate, accepted)
    } else {
        (old, old, false)
    };
    let fits = |value| value >= min && value <= max;
    let mut stored = helper_accepted && fits(raw) && fits(candidate);
    let mut pair = [old_low, old_high];
    if stored {
        match side {
            EdgeSide::Low => {
                pair[0] = candidate;
                pair[1] = old_high
                    .max(candidate.wrapping_add(5000))
                    .min(candidate.wrapping_add(1_000_000));
            }
            EdgeSide::High => {
                stored = candidate >= old_low.wrapping_add(5000)
                    && candidate <= old_low.wrapping_add(1_000_000);
                if stored {
                    pair[1] = candidate;
                }
            }
        }
    }
    if stored {
        holder.as_mut_bytes()[0x138..0x13c].copy_from_slice(&pair[0].to_le_bytes());
        holder.set_channel_field(ChannelField::FrequencyHz, pair[1]);
        record_image::store_range_into(
            &mut image.memory,
            0x16,
            index as u32,
            &mut holder,
            before_store,
        )?;
        record_image::load_range_into(image.memory(), 0x16, index as u32, &mut holder)?;
    }
    Ok(FixedEdgeEffect {
        stored,
        helper_accepted,
        candidate_hz: candidate,
        raw_candidate_hz: raw,
        pair_hz: pair,
        range_bad: fixed_edge_warnings(&holder, runtime.ranges, runtime.live_q, false)?,
        absolute_bad: fixed_edge_warnings(&holder, runtime.ranges, runtime.live_q, true)?,
        state_out: if input == NumberInput::Begin { 2 } else { 0 },
        notify_flags: 0,
    })
}

/// Native group23 input context. Fresh invocations use code/argument/warnings0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AutoRepeaterInvocation {
    pub code: u32,
    pub argument: u32,
    pub warning_bits: u8,
}

#[derive(Clone, Copy)]
pub struct AutoRepeaterRuntime<'a> {
    pub frequency: FrequencyRuntime<'a>,
    pub system_langid: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoRepeaterEffect {
    pub stored: bool,
    pub helper_accepted: bool,
    /// Full helper DWORD before the shared saver's low-byte DUP projection.
    pub candidate: i32,
    pub pair_hz: [u32; 2],
    pub dup: u32,
    pub valid: bool,
    pub state_out: u32,
    pub notify_flags: u32,
    pub warning_bits: u8,
    pub gray: bool,
    pub popup_requested: bool,
    pub blocked: bool,
    /// Parent focus/redispatch requests, not successful UI actions.
    pub move_to_low: bool,
    pub redispatch_code: Option<u32>,
}

fn auto_repeater_load(
    image: &Icf<'_>,
    index: usize,
    binding: u32,
) -> Result<crate::icf::record::DecodedRecord, Error> {
    if index >= 12 {
        return Err(invalid("Auto Repeater row outside native0..11"));
    }
    let mut holder = crate::icf::record::reset_holder(0x18, index as u32, binding);
    crate::icf::record_image::load_range_into(image.memory(), 0x18, index as u32, &mut holder)?;
    Ok(holder)
}

impl AutoRepeaterEffect {
    fn read(
        holder: &crate::icf::record::DecodedRecord,
        invocation: AutoRepeaterInvocation,
    ) -> Self {
        let word = |at| u32::from_le_bytes(holder.as_bytes()[at..at + 4].try_into().unwrap());
        Self {
            stored: false,
            helper_accepted: false,
            candidate: 0,
            pair_hz: [word(0x138), word(0x3c)],
            dup: word(0x44),
            valid: holder.valid(),
            state_out: 0,
            notify_flags: 0,
            warning_bits: invocation.warning_bits,
            gray: false,
            popup_requested: false,
            blocked: invocation.argument == 0xdc,
            move_to_low: false,
            redispatch_code: None,
        }
    }
}

/// English Auto Repeater resource Only_50M skip, never a field-store gate.
pub fn auto_repeater_row_visible(
    q: &crate::icf::context::Context,
    index: usize,
) -> Result<bool, Error> {
    if index >= 12 {
        return Err(invalid("Auto Repeater row outside native0..11"));
    }
    Ok(!(3..=5).contains(&index) || !q.condition_58bdc()?)
}

pub fn edit_auto_repeater_edge(
    image: &mut Icf<'_>,
    runtime: AutoRepeaterRuntime<'_>,
    index: usize,
    side: EdgeSide,
    input: NumberInput<'_>,
    invocation: AutoRepeaterInvocation,
) -> Result<AutoRepeaterEffect, Error> {
    edit_auto_repeater_edge_with_store(image, runtime, (index, side), input, invocation, |_, _| {
        Ok(())
    })
}

/// 7B65D: actual A5134 grammar/bounds, then kind18's ordered save/reload.
/// A cutpoint can fail a store; it never supplies an approved candidate/packet.
pub fn edit_auto_repeater_edge_with_store(
    image: &mut Icf<'_>,
    runtime: AutoRepeaterRuntime<'_>,
    target: (usize, EdgeSide),
    input: NumberInput<'_>,
    invocation: AutoRepeaterInvocation,
    before_store: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<AutoRepeaterEffect, Error> {
    use crate::icf::{frequency, record_image};
    let (index, side) = target;
    let f = runtime.frequency;
    let mut holder = auto_repeater_load(image, index, f.binding)?;
    let mut effect = AutoRepeaterEffect::read(&holder, invocation);
    if effect.blocked {
        return Ok(effect);
    }
    if !f.ranges.is_complete() {
        return Err(invalid(
            "Auto Repeater edge requires completed B generation",
        ));
    }
    let auxiliary = i32::from_le_bytes(holder.as_bytes()[0x38..0x3c].try_into().unwrap());
    let bounds = f.ranges.f760(0x18, auxiliary, 0);
    let low = side == EdgeSide::Low;
    let (at, opposite) = if low { (0x138, 0x3c) } else { (0x3c, 0x138) };
    effect.candidate = effect.pair_hz[usize::from(!low)] as i32;
    effect.state_out = if input == NumberInput::Begin { 2 } else { 0 };
    if let NumberInput::Text(text) = input {
        let (raw, accepted) = numeric_value(text, f.crt_point, 6, 1)?;
        // Native low comparison precedes high, even for inconsistent bounds.
        effect.candidate = if raw < i64::from(bounds.low) {
            bounds.low
        } else if raw > i64::from(bounds.high) {
            bounds.high
        } else {
            raw as u32
        } as i32;
        effect.helper_accepted = accepted;
        if accepted {
            if effect.candidate >= 0 {
                if !holder.valid() {
                    holder.set_valid(true);
                    holder.as_mut_bytes()[opposite..opposite + 4]
                        .copy_from_slice(&effect.candidate.to_le_bytes());
                    holder.as_mut_bytes()[0x44..0x48].copy_from_slice(&1_u32.to_le_bytes());
                }
                holder.as_mut_bytes()[at..at + 4].copy_from_slice(&effect.candidate.to_le_bytes());
                record_image::store_range_into(
                    &mut image.memory,
                    0x18,
                    index as u32,
                    &mut holder,
                    before_store,
                )?;
                record_image::load_range_into(image.memory(), 0x18, index as u32, &mut holder)?;
                let current = AutoRepeaterEffect::read(&holder, invocation);
                effect.pair_hz = current.pair_hz;
                effect.dup = current.dup;
                effect.valid = current.valid;
                effect.stored = true;
            }
            effect.notify_flags = 0x20;
        }
    }
    if frequency::auto_repeater_edge_bad(
        &holder,
        f.ranges,
        f.live_q,
        low,
        false,
        runtime.system_langid,
    )? {
        effect.warning_bits |= 4;
    }
    if frequency::auto_repeater_edge_bad(
        &holder,
        f.ranges,
        f.live_q,
        low,
        true,
        runtime.system_langid,
    )? {
        effect.warning_bits |= 8;
    }
    effect.gray = !low && effect.pair_hz[0] == effect.pair_hz[1];
    Ok(effect)
}

pub fn edit_auto_repeater_dup(
    image: &mut Icf<'_>,
    index: usize,
    input: ChoiceInput,
    binding: u32,
    invocation: AutoRepeaterInvocation,
) -> Result<AutoRepeaterEffect, Error> {
    edit_auto_repeater_dup_with_store(image, index, input, binding, invocation, |_, _| Ok(()))
}

/// 7B8C4: invalid rows request the parent's low field, never enable via DUP.
/// No B, G, live Q, OS locale or additional flag20 is read by this case.
pub fn edit_auto_repeater_dup_with_store(
    image: &mut Icf<'_>,
    index: usize,
    input: ChoiceInput,
    binding: u32,
    invocation: AutoRepeaterInvocation,
    before_store: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<AutoRepeaterEffect, Error> {
    use crate::icf::record_image;
    let mut holder = auto_repeater_load(image, index, binding)?;
    let mut effect = AutoRepeaterEffect::read(&holder, invocation);
    effect.candidate = effect.dup as i32;
    if effect.blocked {
        return Ok(effect);
    }
    if !holder.valid() {
        effect.move_to_low = invocation.code <= 0x7f || invocation.code == 0x7100;
        effect.redispatch_code = if effect.move_to_low && invocation.code != 0 {
            Some(invocation.code)
        } else {
            None
        };
        return Ok(effect);
    }
    effect.popup_requested = input == ChoiceInput::Begin;
    if let Some(candidate) = choice_candidate(i64::from(effect.dup), &[1, 2], input) {
        effect.candidate = candidate;
        effect.helper_accepted = true;
        holder.as_mut_bytes()[0x44..0x48].copy_from_slice(&candidate.to_le_bytes());
        record_image::store_range_into(
            &mut image.memory,
            0x18,
            index as u32,
            &mut holder,
            before_store,
        )?;
        record_image::load_range_into(image.memory(), 0x18, index as u32, &mut holder)?;
        let current = AutoRepeaterEffect::read(&holder, invocation);
        effect.pair_hz = current.pair_hz;
        effect.dup = current.dup;
        effect.valid = current.valid;
        effect.stored = true;
    }
    Ok(effect)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrField {
    DvFilter,
    FmFilter,
    DigitalSquelch,
    DigitalCode,
}
impl DrField {
    pub const ALL: [Self; 4] = [
        Self::DvFilter,
        Self::FmFilter,
        Self::DigitalSquelch,
        Self::DigitalCode,
    ];
    pub fn feature_id(self) -> &'static str {
        match self {
            Self::DvFilter => "A2-H56371-01",
            Self::FmFilter => "A2-H56371-02",
            Self::DigitalSquelch => "A2-H56371-05",
            Self::DigitalCode => "A2-H56371-06",
        }
    }
    pub fn key(self) -> u16 {
        match self {
            Self::DvFilter => 0x121,
            Self::FmFilter => 0x122,
            Self::DigitalSquelch => 0x125,
            Self::DigitalCode => 0x126,
        }
    }
    fn at(self) -> usize {
        match self {
            Self::DvFilter => 0x3c383,
            Self::FmFilter => 0x3c384,
            Self::DigitalSquelch => 0x3c8b5,
            Self::DigitalCode => 0x3c8b6,
        }
    }
    pub fn read(self, image: &Icf<'_>) -> u8 {
        let value = image.memory()[self.at()];
        if self == Self::DigitalSquelch {
            value & 3
        } else {
            value
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrEffect {
    pub stored: bool,
    pub synchronized: bool,
    pub candidate: i32,
    pub state_out: u32,
    pub gray: bool,
    pub notify_flags: u32,
    pub display: Vec<u8>,
}

pub fn edit_dr(
    image: &mut Icf<'_>,
    field: DrField,
    input: NumberInput<'_>,
    binding: u32,
    crt_point: u8,
    node_flags: &mut u32,
) -> Result<DrEffect, Error> {
    edit_dr_with_store(
        image,
        field,
        input,
        (binding, crt_point),
        node_flags,
        |_, _| Ok(()),
    )
}

/// First raw setting commits BEFORE the conditional kind1B/slot0 save/reload.
/// The shared compact adapter supplies the real kind, never a band-stack
/// disguise. A later adapter/store failure retains that first setting store.
pub fn edit_dr_with_store(
    image: &mut Icf<'_>,
    field: DrField,
    input: NumberInput<'_>,
    runtime: (u32, u8),
    node_flags: &mut u32,
    before_store: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<DrEffect, Error> {
    use crate::icf::{
        record::{self, ChannelField},
        record_image,
    };
    let old = field.read(image);
    if field == DrField::DigitalCode {
        *node_flags = (*node_flags & !3) | 2;
    }
    let gray = field == DrField::DigitalCode && DrField::DigitalSquelch.read(image) != 2;
    let (candidate, accepted) = if field == DrField::DigitalCode {
        match input {
            NumberInput::Text(text) => {
                let text = text.split(|&b| b == 0).next().unwrap();
                if matches!(text.len(), 1 | 2) && text.iter().all(u8::is_ascii_digit) {
                    let value = crate::csv::number::crt_decimal_prefix(text, runtime.1)
                        .map_err(|_| invalid("DR digital-code CRT conversion"))?;
                    ((value as u8) as i32, true)
                } else {
                    (i32::from(old), false)
                }
            }
            NumberInput::Next | NumberInput::Previous => {
                let value = i32::from(old) + if input == NumberInput::Next { 1 } else { -1 };
                (
                    if value < 0 {
                        99
                    } else if value > 99 {
                        0
                    } else {
                        value
                    },
                    true,
                )
            }
            _ => (i32::from(old), false),
        }
    } else {
        let choices = &[0, 1, 2];
        let mapped = match input {
            NumberInput::Selected(event) => ChoiceInput::Selected(event),
            NumberInput::Next => ChoiceInput::Next,
            NumberInput::Previous => ChoiceInput::Previous,
            _ => ChoiceInput::Display,
        };
        let value = choice_candidate(i64::from(old), choices, mapped);
        (value.unwrap_or(i32::from(old)), value.is_some())
    };
    let mut synchronized = false;
    if accepted {
        let raw = if field == DrField::DigitalSquelch {
            (image.memory()[field.at()] & !3) | (candidate as u8 & 3)
        } else {
            candidate as u8
        };
        image.patch_memory(&[(field.at(), &[raw])])?;
        if image.memory()[0x3a8ca] == 0 && image.memory()[0x3a8c9] != 0 {
            let mut holder = record::reset_holder(0x1b, 0, runtime.0);
            record_image::load_compact_into(image.memory(), 0x1b, 0, &mut holder)?;
            let sync = match field {
                DrField::DvFilter | DrField::FmFilter => {
                    let mode = if field == DrField::DvFilter {
                        0x10
                    } else {
                        0x0d
                    };
                    if holder.mode() == mode {
                        holder.set_filter(candidate as u32);
                        true
                    } else {
                        false
                    }
                }
                DrField::DigitalSquelch => {
                    holder.set_channel_field(
                        ChannelField::DigitalSquelch,
                        match candidate {
                            1 => 2,
                            2 => 4,
                            _ => 0,
                        },
                    );
                    true
                }
                DrField::DigitalCode => {
                    holder.set_channel_field(ChannelField::DigitalCode, candidate as u32);
                    true
                }
            };
            if sync {
                record_image::store_compact_into(
                    &mut image.memory,
                    0x1b,
                    0,
                    &mut holder,
                    before_store,
                )?;
                record_image::load_compact_into(image.memory(), 0x1b, 0, &mut holder)?;
                synchronized = true;
                if matches!(field, DrField::DvFilter | DrField::FmFilter) {
                    image.patch_memory(&[(0x3c8b4, &[candidate as u8])])?;
                }
            }
        }
    }
    if field == DrField::DigitalCode {
        *node_flags &= !3;
    }
    Ok(DrEffect {
        stored: accepted,
        synchronized,
        candidate,
        state_out: if input == NumberInput::Begin { 2 } else { 0 },
        gray,
        notify_flags: if accepted && field == DrField::DigitalSquelch {
            0x20
        } else {
            0
        },
        display: if field == DrField::DigitalCode {
            format!("{candidate:02}").into_bytes()
        } else {
            Vec::new()
        },
    })
}

/// The two actual Home catalog scalars, not a copied popup table. initial() is
/// the PE's normal pre-mutation catalog generation, never an implicit Default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HomeGroupCatalog {
    pub empty: i32,
    pub call: i32,
}
impl HomeGroupCatalog {
    pub const fn initial() -> Self {
        Self {
            empty: 255,
            call: 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeGroupEffect {
    pub stored: bool,
    pub candidate: i32,
    pub group_raw: u8,
    pub channel_raw: u16,
    pub state_out: u32,
    pub notify_flags: u32,
}

/// A72C8 flags0 and the real stored-count/map getters. The source NormalMemory
/// is explicit, not reconstructed from ICF for each candidate or silently
/// substituted for a different live generation. Native group-byte store
/// precedes row search; no matching row retains the old CH and still notifies.
pub fn edit_home_group(
    image: &mut Icf<'_>,
    normal: &crate::icf::rows::NormalMemory,
    catalog: HomeGroupCatalog,
    input: NumberInput<'_>,
) -> Result<HomeGroupEffect, Error> {
    const GROUP: super::Setting = number(
        "A2-H54848-12",
        "home_group",
        0x37e,
        0x392fa,
        (1, 0),
        (0, 255, 1, "group code"),
        (1, 0),
    );
    const CHANNEL: super::Setting = number(
        "A2-H54848-13",
        "home_channel",
        0x37f,
        0x392fb,
        (2, 0),
        (0, 65535, 1, "row code"),
        (1, 0),
    );
    let old = GROUP.read_numeric_raw(image, 0)?;
    let local = if old > 100 { 255 } else { old };
    let (candidate, accepted) = match input {
        NumberInput::Text(text) => {
            if !text.is_ascii() {
                return Err(invalid(
                    "Home group non-ASCII normalization source required",
                ));
            }
            if text.len() > 4096 {
                return Err(invalid("Home group shared integer source exceeds4096bytes"));
            }
            let mut text = crate::csv::trim(text).to_vec();
            text.make_ascii_uppercase();
            if text.is_empty() {
                (255, true)
            } else if text == b"C" {
                (100, true)
            } else if let Ok(value) = crate::csv::number::int32(&text) {
                if (0..100).contains(&value) {
                    (value, normal.group_used_slots(value as usize)? != 0)
                } else {
                    (value, value == 100)
                }
            } else {
                (local, false)
            }
        }
        NumberInput::Selected(event) => (event.candidate, event.accepted && event.phase == 2),
        NumberInput::Next | NumberInput::Previous => {
            let mut values = Vec::new();
            if catalog.empty != -1 {
                values.push(i64::from(catalog.empty));
            }
            for group in 0..100 {
                if local == group || normal.group_used_slots(group as usize)? != 0 {
                    values.push(i64::from(group));
                }
            }
            if catalog.call != -1 {
                values.push(i64::from(catalog.call));
            }
            if values.is_empty() {
                (local, false)
            } else {
                let next = choice_candidate(
                    i64::from(local),
                    &values,
                    if input == NumberInput::Next {
                        ChoiceInput::Next
                    } else {
                        ChoiceInput::Previous
                    },
                )
                .unwrap();
                (next, true)
            }
        }
        _ => (local, false),
    };
    if accepted {
        if candidate == 255 {
            GROUP.store_numeric_raw(image, 0, 255)?;
            CHANNEL.store_numeric_raw(image, 0, 65535)?;
        } else {
            GROUP.store_numeric_raw(image, 0, candidate)?;
            let first = match candidate {
                0..=99 => {
                    let mut found = None;
                    for row in 0..100 {
                        if crate::icf::record_image::channel_address(
                            normal.working(),
                            crate::icf::record_image::ChannelKind::Normal,
                            candidate as u32 * 200 + row as u32 * 2,
                        )?
                        .is_some()
                        {
                            found = Some(row);
                            break;
                        }
                    }
                    found
                }
                100 => Some(0),
                _ => {
                    return Err(invalid(
                        "Home group stored lowbyte; full candidate has no normal kind source",
                    ));
                }
            };
            if let Some(row) = first {
                CHANNEL.store_numeric_raw(image, 0, row)?;
            }
        }
    }
    Ok(HomeGroupEffect {
        stored: accepted,
        candidate,
        group_raw: GROUP.read_numeric_raw(image, 0)? as u8,
        channel_raw: CHANNEL.read_numeric_raw(image, 0)? as u16,
        state_out: if input == NumberInput::Begin { 2 } else { 0 },
        notify_flags: if accepted { 0x20 } else { 0 },
    })
}

#[derive(Clone, Copy)]
pub struct HomeChannelRuntime<'a> {
    pub frequency: FrequencyRuntime<'a>,
    /// Actual global current working image, distinct from the edited I argument.
    pub current: &'a [u8],
    /// Raw VA864614 read by61F20 before loading each odd side.
    pub show_both_864614: u32,
}

fn home_pair_bad(
    runtime: HomeChannelRuntime<'_>,
    group: u8,
    row: u32,
    broad: bool,
) -> Result<bool, Error> {
    use crate::icf::record_image::{ChannelFilter, ChannelView, channel_bad_count};
    let first = row.wrapping_mul(2) as i32;
    Ok(channel_bad_count(
        runtime.current,
        ChannelView {
            group: u32::from(group),
            first,
            last: first.wrapping_add(1),
            count: 1,
        },
        ChannelFilter {
            broad,
            include_side1: true,
            count_all: false,
            show_both: runtime.show_both_864614,
        },
        runtime.frequency.ranges,
        runtime.frequency.live_q,
        runtime.frequency.binding,
    )? != 0)
}

fn home_row_empty(runtime: HomeChannelRuntime<'_>, group: u8, row: u32) -> Result<bool, Error> {
    use crate::icf::record_image::{self, ChannelKind};
    if group == 100 {
        return Ok(false);
    }
    Ok(record_image::channel_address(
        runtime.current,
        ChannelKind::Normal,
        u32::from(group) * 200 + row * 2,
    )?
    .is_none())
}

pub fn home_channel_choices(
    image: &Icf<'_>,
    runtime: HomeChannelRuntime<'_>,
    popup: bool,
) -> Result<Vec<i64>, Error> {
    if !runtime.frequency.ranges.is_complete() {
        return Err(invalid("Home CH choices require completed B generation"));
    }
    let group = image.memory()[0x392fa];
    if group > 100 {
        return Err(invalid("Home CH parent group is outside0..100"));
    }
    let old = u16::from_le_bytes(image.memory()[0x392fb..0x392fd].try_into().unwrap());
    let count = if group == 100 { 4 } else { 100 };
    let mut values = Vec::new();
    for row in 0..count {
        if !home_pair_bad(runtime, group, row, true)?
            && ((!popup && u32::from(old) == row) || !home_row_empty(runtime, group, row)?)
        {
            values.push(i64::from(row));
        }
    }
    Ok(values)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeChannelEffect {
    pub stored: bool,
    pub candidate: u32,
    pub state_out: u32,
    pub popup_requested: bool,
    pub notify_flags: u32,
    pub warning_bits: u32,
}

/// Normal stable GI_CchNo has the SAME four generated labels in all3 PE language
/// resources. Both exact and dash-deleted probes are resolved anew before the
/// digits/minus fallback. That fallback has no9byte cap and uses shared int32.
pub fn edit_home_channel(
    image: &mut Icf<'_>,
    runtime: HomeChannelRuntime<'_>,
    input: NumberInput<'_>,
    old_warning_bits: u32,
) -> Result<HomeChannelEffect, Error> {
    if !runtime.frequency.ranges.is_complete() {
        return Err(invalid("Home CH requires completed B generation"));
    }
    let group = image.memory()[0x392fa];
    if group > 100 {
        return Err(invalid("Home CH parent group outside0..100"));
    }
    let old = u16::from_le_bytes(image.memory()[0x392fb..0x392fd].try_into().unwrap());
    let (candidate, accepted) = match input {
        NumberInput::Text(text) => {
            if !text.is_ascii() {
                return Err(invalid("Home CH non-ASCII normalization output required"));
            }
            let mut normalized = crate::csv::trim(text).to_vec();
            normalized.make_ascii_uppercase();
            let labels = [b"144-C1".as_slice(), b"144-C2", b"430-C1", b"430-C2"];
            let exact = labels.iter().position(|label| *label == normalized);
            let no_dash: Vec<_> = normalized.iter().copied().filter(|&b| b != b'-').collect();
            let alias = || {
                labels.iter().position(|label| {
                    label
                        .iter()
                        .copied()
                        .filter(|&b| b != b'-')
                        .eq(no_dash.iter().copied())
                })
            };
            let resolved = exact.or_else(alias).map(|row| row as i32).or_else(|| {
                let digits: Vec<_> = normalized
                    .into_iter()
                    .filter(|b| b.is_ascii_digit() || *b == b'-')
                    .collect();
                crate::csv::number::int32(&digits).ok()
            });
            if let Some(row) = resolved
                && row >= 0
                && row < if group == 100 { 4 } else { 100 }
                && !home_row_empty(runtime, group, row as u32)?
                && !home_pair_bad(runtime, group, row as u32, true)?
            {
                (row as u32, true)
            } else {
                (u32::from(old), false)
            }
        }
        NumberInput::Selected(event) => (
            event.candidate as u32,
            event.accepted && event.phase == 2 && event.key == 0x0e,
        ),
        NumberInput::Next | NumberInput::Previous => {
            let values = home_channel_choices(image, runtime, false)?;
            let selected = if values.is_empty() {
                None
            } else {
                choice_candidate(
                    i64::from(old),
                    &values,
                    if input == NumberInput::Next {
                        ChoiceInput::Next
                    } else {
                        ChoiceInput::Previous
                    },
                )
            };
            (
                selected.unwrap_or(i32::from(old)) as u32,
                selected.is_some(),
            )
        }
        _ => (u32::from(old), false),
    };
    if accepted {
        const CHANNEL: super::Setting = number(
            "A2-H54848-13",
            "home_channel",
            0x37f,
            0x392fb,
            (2, 0),
            (0, 65535, 1, "row code"),
            (1, 0),
        );
        CHANNEL.store_numeric_raw(image, 0, candidate as i32)?;
    }
    let mut warning_bits = old_warning_bits;
    if candidate != 65535 {
        let current = u16::from_le_bytes(image.memory()[0x392fb..0x392fd].try_into().unwrap());
        if current != 65535 {
            if home_pair_bad(runtime, group, u32::from(current), false)? {
                warning_bits |= 4;
            }
            if home_pair_bad(runtime, group, u32::from(current), true)? {
                warning_bits |= 8;
            }
        }
    }
    Ok(HomeChannelEffect {
        stored: accepted,
        candidate,
        state_out: 0,
        popup_requested: input == NumberInput::Begin,
        notify_flags: if accepted { 0x20 } else { 0 },
        warning_bits,
    })
}

#[derive(Clone, Copy)]
pub struct HomeDrRuntime<'a> {
    pub frequency: FrequencyRuntime<'a>,
    /// Actual caller I working generation for counts/C6FA4/C705C.
    pub source: &'a [u8],
    /// Separate global current for span/625F8 loads and post-save warnings.
    pub current: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeDrEffect {
    pub accepted: bool,
    pub stored: bool,
    pub candidate: i32,
    pub reference: u16,
    pub notify_flags: u32,
    pub warning_bits: u32,
    pub state_out: u32,
    pub popup_requested: bool,
}

fn dr_reference(image: &Icf<'_>) -> u16 {
    u16::from_le_bytes(image.memory()[0x392fd..0x392ff].try_into().unwrap())
}
fn dr_prefix(groups: &crate::repeater::groups::Groups, group: usize) -> Result<u32, Error> {
    //C664C's real high49 constructor makes group50 prefix0, not total.
    if group == 50 {
        Ok(0)
    } else {
        groups.start(group)
    }
}
fn dr_reference_store(image: &mut Icf<'_>, value: u16) -> Result<(), Error> {
    const FIELD: super::Setting = number(
        "A2-H56371-08",
        "home_dr_reference",
        0x383,
        0x392fd,
        (2, 0),
        (0, 65535, 1, "absolute repeater reference"),
        (1, 0),
    );
    FIELD.store_numeric_raw(image, 0, i32::from(value))
}

pub fn home_dr_group_choices(
    image: &Icf<'_>,
    runtime: HomeDrRuntime<'_>,
) -> Result<Vec<i64>, Error> {
    use crate::repeater::{groups::Groups, predicate};
    if !runtime.frequency.ranges.is_complete() {
        return Err(invalid("Home DR requires completed B generation"));
    }
    let groups = Groups::from_working(runtime.source)?;
    let old = dr_reference(image);
    let current = if old == 65535 {
        50
    } else {
        groups.group_of(i32::from(old))
    };
    let mut values = vec![50];
    for group in 0..50 {
        if group == current
            || predicate::home_usable_count(
                runtime.source,
                group,
                runtime.frequency.ranges,
                runtime.frequency.live_q,
                runtime.frequency.binding,
            )? != 0
        {
            values.push(group as i64);
        }
    }
    Ok(values)
}

pub fn edit_home_dr_group(
    image: &mut Icf<'_>,
    runtime: HomeDrRuntime<'_>,
    input: NumberInput<'_>,
) -> Result<HomeDrEffect, Error> {
    use crate::repeater::{groups::Groups, predicate};
    let groups = Groups::from_working(runtime.source)?;
    let old = dr_reference(image);
    let current = if old == 65535 {
        50
    } else {
        groups.group_of(i32::from(old)) as i32
    };
    let (candidate, accepted) = match input {
        NumberInput::Text(text) => {
            if !text.is_ascii() {
                return Err(invalid(
                    "Home DR group non-ASCII normalization output required",
                ));
            }
            let text = crate::csv::trim(text);
            if text.is_empty() {
                (50, true)
            } else if let Ok(number) = crate::csv::number::int32(text) {
                if (1..=50).contains(&number) {
                    (number - 1, groups.counts[(number - 1) as usize] != 0)
                } else {
                    (current, false)
                }
            } else {
                (current, false)
            }
        }
        NumberInput::Selected(event) => (event.candidate, event.accepted && event.phase == 2),
        NumberInput::Next | NumberInput::Previous => {
            let values = home_dr_group_choices(image, runtime)?;
            let value = choice_candidate(
                i64::from(current),
                &values,
                if input == NumberInput::Next {
                    ChoiceInput::Next
                } else {
                    ChoiceInput::Previous
                },
            )
            .unwrap();
            (value, true)
        }
        _ => (current, false),
    };
    let mut stored = false;
    if accepted {
        if candidate == 50 {
            dr_reference_store(image, 65535)?;
            stored = true;
        } else if (0..50).contains(&candidate) {
            if groups.counts[candidate as usize] != 0 && !runtime.frequency.ranges.is_complete() {
                return Err(invalid(
                    "Home DR suitability requires completed B generation",
                ));
            }
            for row in 0..groups.counts[candidate as usize] {
                if predicate::home_row_usable(
                    runtime.source,
                    candidate as usize,
                    i32::from(row),
                    runtime.frequency.ranges,
                    runtime.frequency.live_q,
                    runtime.frequency.binding,
                )? {
                    let start = Groups::from_working(runtime.source)?.start(candidate as usize)?;
                    dr_reference_store(image, start.wrapping_add(u32::from(row)) as u16)?;
                    stored = true;
                    break;
                }
            }
        } else {
            return Err(invalid(
                "Home DR selected group requires its real count source",
            ));
        }
    }
    Ok(HomeDrEffect {
        accepted,
        stored,
        candidate,
        reference: dr_reference(image),
        notify_flags: if accepted { 0x20 } else { 0 },
        warning_bits: 0,
        state_out: 0,
        popup_requested: input == NumberInput::Begin,
    })
}

pub enum HomeDrMemoryInput<'a> {
    Display,
    Cancel,
    Popup,
    Next,
    Previous,
    /// ACCC4 replaces context48 with Trim/digits-minus BEFORE later rejection.
    Text(&'a mut Vec<u8>),
    Selected(super::Selection),
}

fn dr_numeric_filter(runtime: HomeDrRuntime<'_>, absolute: u32) -> Result<bool, Error> {
    let mut h = crate::icf::record::reset_holder(0x0c, absolute, runtime.frequency.binding);
    crate::repeater::working::load_into(runtime.current, absolute as usize, &mut h)?;
    Ok(crate::repeater::predicate::numeric_frequency_bad(&h))
}

pub fn home_dr_memory_choices(
    image: &Icf<'_>,
    runtime: HomeDrRuntime<'_>,
) -> Result<Vec<i64>, Error> {
    use crate::repeater::{groups::Groups, predicate};
    if !runtime.frequency.ranges.is_complete() {
        return Err(invalid("Home DR memory requires completed B generation"));
    }
    let source = Groups::from_working(runtime.source)?;
    let word = dr_reference(image);
    let group = source.group_of(i32::from(word));
    if group == 50 {
        return Ok(Vec::new());
    }
    let old = i32::from(word).wrapping_sub(dr_prefix(&source, group)? as i32);
    let live = Groups::from_working(runtime.current)?;
    let start = live.start(group)?;
    let high = live.end(group)?.max(0).wrapping_sub(start as i32);
    let mut values = Vec::new();
    for row in 0..=high {
        if !dr_numeric_filter(runtime, start.wrapping_add(row as u32))?
            && (row == old
                || predicate::home_row_usable(
                    runtime.source,
                    group,
                    row,
                    runtime.frequency.ranges,
                    runtime.frequency.live_q,
                    runtime.frequency.binding,
                )?)
        {
            values.push(i64::from(row));
        }
    }
    Ok(values)
}

pub fn edit_home_dr_memory(
    image: &mut Icf<'_>,
    runtime: HomeDrRuntime<'_>,
    input: HomeDrMemoryInput<'_>,
    old_warning_bits: u32,
) -> Result<HomeDrEffect, Error> {
    use crate::repeater::{groups::Groups, predicate, working};
    if !runtime.frequency.ranges.is_complete() {
        return Err(invalid("Home DR memory requires completed B generation"));
    }
    let groups = Groups::from_working(runtime.source)?;
    let old = dr_reference(image);
    let group = groups.group_of(i32::from(old));
    let relative = i32::from(old).wrapping_sub(dr_prefix(&groups, group)? as i32);
    let popup_requested = matches!(input, HomeDrMemoryInput::Popup);
    let (candidate, accepted) = match input {
        HomeDrMemoryInput::Text(source) if group < 50 => {
            if !source.is_ascii() {
                return Err(invalid("Home DR memory non-ASCII ctype output required"));
            }
            *source = crate::csv::trim(source)
                .iter()
                .copied()
                .filter(|b| b.is_ascii_digit() || *b == b'-')
                .collect();
            let live = Groups::from_working(runtime.current)?;
            let high = live
                .end(group)?
                .max(0)
                .wrapping_sub(live.start(group)? as i32);
            if source.is_empty() {
                (relative, false)
            } else {
                let value = numeric_value(source, runtime.frequency.crt_point, 0, 1)?.0 as i32;
                //Typed625F8 uses relative q AS absolute, unlike list's start+q.
                if value >= 0
                    && value <= high
                    && predicate::home_row_usable(
                        runtime.source,
                        group,
                        value,
                        runtime.frequency.ranges,
                        runtime.frequency.live_q,
                        runtime.frequency.binding,
                    )?
                    && !dr_numeric_filter(runtime, value as u32)?
                {
                    (value, true)
                } else {
                    (relative, false)
                }
            }
        }
        HomeDrMemoryInput::Selected(event) if group < 50 => (
            event.candidate,
            event.accepted && event.key == 0x13 && event.phase == 2,
        ),
        HomeDrMemoryInput::Next | HomeDrMemoryInput::Previous if group < 50 => {
            let values = home_dr_memory_choices(image, runtime)?;
            if values.is_empty() {
                (relative, false)
            } else {
                (
                    choice_candidate(
                        i64::from(relative),
                        &values,
                        if matches!(input, HomeDrMemoryInput::Next) {
                            ChoiceInput::Next
                        } else {
                            ChoiceInput::Previous
                        },
                    )
                    .unwrap(),
                    true,
                )
            }
        }
        _ => (relative, false),
    };
    let mut stored = false;
    if accepted {
        let latest = dr_reference(image);
        let latest_groups = Groups::from_working(runtime.source)?;
        let latest_group = latest_groups.group_of(i32::from(latest));
        let latest_relative =
            i32::from(latest).wrapping_sub(dr_prefix(&latest_groups, latest_group)? as i32);
        let delta = candidate.wrapping_sub(latest_relative);
        if delta != 0 {
            dr_reference_store(image, latest.wrapping_add(delta as u16))?;
            stored = true;
        }
    }
    let mut warning_bits = old_warning_bits;
    let now = dr_reference(image);
    if now != 65535 {
        let source_groups = Groups::from_working(runtime.source)?;
        let g = source_groups.group_of(i32::from(now));
        let q = i32::from(now).wrapping_sub(dr_prefix(&source_groups, g)? as i32);
        let absolute =
            dr_prefix(&Groups::from_working(runtime.current)?, g)?.wrapping_add(q as u32);
        let mut h = crate::icf::record::reset_holder(0x0c, absolute, runtime.frequency.binding);
        working::load_into(runtime.current, absolute as usize, &mut h)?;
        if !predicate::home_usable(
            runtime.current,
            &h,
            runtime.frequency.ranges,
            runtime.frequency.live_q,
        )? {
            warning_bits |= 4;
        }
        if predicate::numeric_frequency_bad(&h) {
            warning_bits |= 8;
        }
    }
    Ok(HomeDrEffect {
        accepted,
        stored,
        candidate,
        reference: dr_reference(image),
        notify_flags: if accepted { 0x20 } else { 0 },
        warning_bits,
        state_out: 0,
        popup_requested,
    })
}

pub enum ProgramScanInput<'a> {
    Display,
    Cancel,
    Popup,
    Next,
    Previous,
    Text(&'a mut Vec<u8>),
    Selected(super::Selection),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramScanEffect {
    pub accepted: bool,
    pub stored: bool,
    pub candidate: i32,
    pub selected: u8,
    pub notify_flags: u32,
    pub popup_requested: bool,
}

/// AA658's actual0/0/0/0/1 caller always checks broad bounds, not B/Q warnings.
fn program_scan_bounds_ok(current: &[u8], row: usize) -> Result<bool, Error> {
    Ok(
        crate::icf::special::ScanEdge::read(current, row)?.suitable(true, true, false, |_| {
            unreachable!("AA658 does not request the machine-specific predicate")
        }),
    )
}

pub fn program_scan_choices(image: &Icf<'_>, current: &[u8]) -> Result<Vec<i64>, Error> {
    use crate::icf::special;
    let selected = special::selected_scan_raw(image.memory())?;
    let mut values = Vec::new();
    for row in 0..25 {
        let h = special::ScanEdge::read(current, row)?;
        if (usize::from(selected) == row || (h.valid && h.low_hz != h.high_hz))
            && program_scan_bounds_ok(current, row)?
        {
            values.push(row as i64);
        }
    }
    Ok(values)
}

/// AA658 binds existence, equality and broad bounds to global current.
/// G is read only by the real conditional reset, after the selected-byte store.
pub fn edit_program_scan(
    image: &mut Icf<'_>,
    current: &[u8],
    g: &[u8],
    input: ProgramScanInput<'_>,
) -> Result<ProgramScanEffect, Error> {
    use crate::icf::special;
    let old = special::selected_scan_raw(image.memory())?;
    let popup_requested = matches!(input, ProgramScanInput::Popup);
    let (candidate, accepted) = match input {
        ProgramScanInput::Text(source) => {
            if !source.is_ascii() {
                return Err(invalid("program scan non-ASCII ctype output required"));
            }
            source.make_ascii_uppercase();
            *source = crate::csv::trim(source).to_vec();
            //AA79F precedes the optional25 aliases, disabled in this caller.
            let _first = crate::csv::number::int32(source).unwrap_or(i32::MAX);
            *source = source
                .split(|&b| b == 0)
                .next()
                .unwrap()
                .iter()
                .copied()
                .filter(|b| b.is_ascii_digit() || *b == b'-')
                .collect();
            //AA955 skips the second parse when the C-prefix filter is empty.
            if !source.is_empty()
                && let Ok(candidate) = crate::csv::number::int32(source)
            {
                if (0..25).contains(&candidate) {
                    let h = special::ScanEdge::read(current, candidate as usize)?;
                    (
                        candidate,
                        h.valid
                            && h.low_hz != h.high_hz
                            && program_scan_bounds_ok(current, candidate as usize)?,
                    )
                } else {
                    (i32::from(old), false)
                }
            } else {
                (i32::from(old), false)
            }
        }
        ProgramScanInput::Selected(event) => (
            event.candidate,
            event.accepted && event.key == 0x10 && event.phase == 2,
        ),
        ProgramScanInput::Next | ProgramScanInput::Previous => {
            let previous = matches!(input, ProgramScanInput::Previous);
            let values = program_scan_choices(image, current)?;
            if values.is_empty() {
                (i32::from(old), false)
            } else {
                (
                    choice_candidate(
                        i64::from(old),
                        &values,
                        if previous {
                            ChoiceInput::Previous
                        } else {
                            ChoiceInput::Next
                        },
                    )
                    .unwrap(),
                    true,
                )
            }
        }
        _ => (i32::from(old), false),
    };
    if accepted {
        //The reset guard reads no selected-index bits; G remains a later read.
        let reset = special::scan_reset_enabled(image.memory(), 0)?;
        special::select_scan_raw(image, g, candidate, reset)?;
    }
    Ok(ProgramScanEffect {
        accepted,
        stored: accepted,
        candidate,
        selected: special::selected_scan_raw(image.memory())?,
        notify_flags: if accepted { 0x20 } else { 0 },
        popup_requested,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chosen(value: i32) -> ChoiceInput {
        ChoiceInput::Selected(super::super::Selection {
            key: 1,
            phase: 2,
            candidate: value,
            accepted: true,
        })
    }
    fn image() -> Icf<'static> {
        let mut raw = vec![0xa5; 0x3c9c0];
        raw[0x355a1] = 6;
        raw[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        Icf::from_memory(raw).unwrap()
    }

    #[test]
    fn auto_repeater_edges_use_native_bounds_initialize_invalid_and_warn_after_save() {
        use crate::icf::{context::Context, frequency::Ranges};
        let mut bytes = [0; 24];
        bytes[0] = 1;
        bytes[0x13] = 1;
        let q = Context::from_bytes(bytes);
        let ranges = Ranges::generate(&q, None).unwrap();
        let runtime = AutoRepeaterRuntime {
            frequency: FrequencyRuntime {
                ranges: &ranges,
                live_q: &q,
                binding: 77,
                crt_point: b'.',
            },
            system_langid: None,
        };
        let invocation = AutoRepeaterInvocation::default();
        for (row, text, hz) in [
            (0, b"1.25".as_slice(), 1_250_000_u32),
            (2, b"29.999999", 29_999_999),
            (3, b"31", 31_000_000),
            (8, b"145", 145_000_000),
            (9, b"433", 433_000_000),
            (11, b"470", 470_000_000),
        ] {
            for missing in [0, 2, 6] {
                let mut image = image();
                let at = 0x370e0 + 10 * row;
                image
                    .patch_memory(&[
                        (at, &[2, 0x7e]),
                        (at + 2, &42_u32.to_le_bytes()),
                        (at + 6, &99_u32.to_le_bytes()),
                        (
                            at + missing,
                            if missing == 0 { &[0] } else { &[0, 0, 0, 0] },
                        ),
                    ])
                    .unwrap();
                let before = image.memory().to_vec();
                let out = edit_auto_repeater_edge(
                    &mut image,
                    runtime,
                    row,
                    EdgeSide::High,
                    NumberInput::Text(text),
                    invocation,
                )
                .unwrap();
                assert!(out.stored && out.helper_accepted && out.valid && out.gray);
                assert_eq!(out.notify_flags, 0x20);
                assert_eq!(out.pair_hz, [hz, hz]);
                assert_eq!(out.dup, 1);
                let mut expected = before;
                expected[at] = 1;
                expected[at + 2..at + 6].copy_from_slice(&hz.to_le_bytes());
                expected[at + 6..at + 10].copy_from_slice(&hz.to_le_bytes());
                assert_eq!(image.memory(), expected);
            }
        }
        let mut image = image();
        let at = 0x370e0;
        image
            .patch_memory(&[
                (at, &[2]),
                (at + 2, &1_000_000_u32.to_le_bytes()),
                (at + 6, &2_000_000_u32.to_le_bytes()),
            ])
            .unwrap();
        for (text, hz) in [
            (b"-4".as_slice(), 30_000_u32),
            (b"999999999999999", 29_999_999),
            (b"1e-3", 1_000_000),
            (b"2.5\0.7", 2_500_000),
            (b"1.23456789099", 1_234_568),
        ] {
            let out = edit_auto_repeater_edge(
                &mut image,
                runtime,
                0,
                EdgeSide::Low,
                NumberInput::Text(text),
                invocation,
            )
            .unwrap();
            assert!(out.stored);
            assert_eq!(out.pair_hz, [hz, 2_000_000]);
        }
        let out = edit_auto_repeater_edge(
            &mut image,
            runtime,
            0,
            EdgeSide::High,
            NumberInput::Text(b".5"),
            invocation,
        )
        .unwrap();
        assert!(out.stored);
        assert_eq!(out.pair_hz, [1_234_568, 500_000]);
        assert_eq!(out.warning_bits, 4);
        let before = image.memory().to_vec();
        for input in [
            NumberInput::Cancel,
            NumberInput::Begin,
            NumberInput::Display,
            NumberInput::Text(b"abc"),
        ] {
            let out =
                edit_auto_repeater_edge(&mut image, runtime, 0, EdgeSide::Low, input, invocation)
                    .unwrap();
            assert!(!out.stored && !out.helper_accepted);
            assert_eq!(out.notify_flags, 0);
            assert_eq!(
                out.state_out,
                if input == NumberInput::Begin { 2 } else { 0 }
            );
            assert_eq!(image.memory(), before);
        }
        image
            .patch_memory(&[
                (0x370e0 + 30, &[1]),
                (0x370e0 + 32, &1_000_000_u32.to_le_bytes()),
                (0x370e0 + 36, &31_000_000_u32.to_le_bytes()),
            ])
            .unwrap();
        let out = edit_auto_repeater_edge(
            &mut image,
            runtime,
            3,
            EdgeSide::Low,
            NumberInput::Display,
            invocation,
        )
        .unwrap();
        assert_eq!(
            out.warning_bits, 4,
            "kind18 warning must query H38, not H20"
        );
        assert!(!auto_repeater_row_visible(&q, 3).unwrap());
        assert!(auto_repeater_row_visible(&q, 2).unwrap());
        assert!(
            edit_auto_repeater_edge(
                &mut image,
                runtime,
                12,
                EdgeSide::Low,
                NumberInput::Display,
                invocation
            )
            .is_err()
        );
    }

    #[test]
    fn auto_repeater_dup_and_ordered_failures_keep_reserved_and_real_prefixes() {
        use crate::icf::{context::Context, frequency::Ranges};
        let mut raw_q = [0; 24];
        raw_q[0] = 1;
        let q = Context::from_bytes(raw_q);
        let ranges = Ranges::generate(&q, None).unwrap();
        let runtime = AutoRepeaterRuntime {
            frequency: FrequencyRuntime {
                ranges: &ranges,
                live_q: &q,
                binding: 1,
                crt_point: b'.',
            },
            system_langid: None,
        };
        let invocation = AutoRepeaterInvocation::default();
        let mut image = image();
        let at = 0x370e0;
        image
            .patch_memory(&[
                (at, &[2, 0x7e]),
                (at + 2, &1_000_000_u32.to_le_bytes()),
                (at + 6, &2_000_000_u32.to_le_bytes()),
            ])
            .unwrap();
        let before = image.memory().to_vec();
        for input in [
            ChoiceInput::Cancel,
            ChoiceInput::Begin,
            ChoiceInput::Display,
        ] {
            let out = edit_auto_repeater_dup(&mut image, 0, input, 1, invocation).unwrap();
            assert!(!out.stored);
            assert_eq!(out.popup_requested, input == ChoiceInput::Begin);
            assert_eq!(image.memory(), before);
        }
        for candidate in [2, 0x101, -1, 0x100] {
            let out =
                edit_auto_repeater_dup(&mut image, 0, chosen(candidate), 1, invocation).unwrap();
            assert!(out.stored);
            assert_eq!(out.candidate, candidate);
            assert_eq!(out.dup, candidate as u8 as u32);
            assert_eq!(out.notify_flags, 0);
            assert_eq!(image.memory()[at + 1], 0x7e);
        }
        let before = image.memory().to_vec();
        let out = edit_auto_repeater_dup(
            &mut image,
            0,
            chosen(2),
            1,
            AutoRepeaterInvocation {
                code: 0x7100,
                ..invocation
            },
        )
        .unwrap();
        assert!(!out.stored && out.move_to_low && !out.popup_requested);
        assert_eq!(out.redispatch_code, Some(0x7100));
        assert_eq!(image.memory(), before);
        let out = edit_auto_repeater_dup(
            &mut image,
            0,
            chosen(2),
            1,
            AutoRepeaterInvocation {
                argument: 0xdc,
                ..invocation
            },
        )
        .unwrap();
        assert!(out.blocked && !out.move_to_low && !out.stored);
        for failure in [6, 0] {
            let before = image.memory().to_vec();
            let mut offsets = Vec::new();
            assert!(
                edit_auto_repeater_edge_with_store(
                    &mut image,
                    runtime,
                    (0, EdgeSide::Low),
                    NumberInput::Text(b"1.5"),
                    invocation,
                    |offset, _| {
                        offsets.push(offset);
                        if offset == failure {
                            Err(invalid("injected ordered store failure"))
                        } else {
                            Ok(())
                        }
                    }
                )
                .is_err()
            );
            let mut expected = before;
            expected[at + 2..at + 6].copy_from_slice(&1_500_000_u32.to_le_bytes());
            if failure == 0 {
                expected[at + 6..at + 10].copy_from_slice(&1_500_000_u32.to_le_bytes());
                assert_eq!(offsets, [2, 6, 0]);
            } else {
                assert_eq!(offsets, [2, 6]);
            }
            assert_eq!(image.memory(), expected);
        }
        let mut raw = [0; 24];
        raw[0] = 16;
        let late_q = Context::from_bytes(raw);
        let late = AutoRepeaterRuntime {
            frequency: FrequencyRuntime {
                live_q: &late_q,
                ..runtime.frequency
            },
            ..runtime
        };
        assert!(
            edit_auto_repeater_edge(
                &mut image,
                late,
                0,
                EdgeSide::Low,
                NumberInput::Text(b"1.75"),
                invocation
            )
            .is_err()
        );
        assert_eq!(
            &image.memory()[at + 2..at + 6],
            &1_750_000_u32.to_le_bytes()
        );
        let empty = Ranges::unpopulated();
        let missing = AutoRepeaterRuntime {
            frequency: FrequencyRuntime {
                ranges: &empty,
                ..runtime.frequency
            },
            ..runtime
        };
        let before = image.memory().to_vec();
        assert!(
            edit_auto_repeater_edge(
                &mut image,
                missing,
                0,
                EdgeSide::Low,
                NumberInput::Text(b"1"),
                invocation
            )
            .is_err()
        );
        assert_eq!(image.memory(), before);
    }
    #[test]
    fn typed_numeric_uses_native_filter_and_shared_unknown_replacement() {
        let mut image = image();
        image.patch_memory(&[(0x3999a, &[0xff])]).unwrap();
        let before = image.memory().to_vec();
        let result = edit_number(
            &mut image,
            "speech_compression_level",
            NumberInput::Text(b"1e2"),
            b'.',
        )
        .unwrap();
        assert_eq!((result.candidate, result.stored), (10, true));
        let mut expected = before;
        expected[0x3999a] = 10;
        assert_eq!(image.memory(), expected);
        for text in [b"foo".as_slice(), b"-"] {
            let result = edit_number(
                &mut image,
                "speech_compression_level",
                NumberInput::Text(text),
                b'.',
            )
            .unwrap();
            assert!(!result.stored);
            assert_eq!(image.memory(), expected);
        }
        assert!(
            edit_number(
                &mut image,
                "speech_compression_level",
                NumberInput::Text(b"."),
                b'.'
            )
            .unwrap()
            .stored
        );
        assert_eq!(image.memory()[0x3999a], 0);
    }
    #[test]
    fn scale_step_cancel_unconditional_store_and_old_suffix_are_distinct() {
        let mut image = image();
        image.patch_memory(&[(0x3999d, &[0])]).unwrap();
        let result =
            edit_number(&mut image, "cw_speed_raw", NumberInput::Text(b"255"), b'.').unwrap();
        assert_eq!(result.suffix, b" (6WPM)");
        assert_eq!(image.memory()[0x3999d], 255);
        let before = image.memory().to_vec();
        assert!(
            !edit_number(&mut image, "cw_pitch_hz", NumberInput::Cancel, b'.')
                .unwrap()
                .stored
        );
        assert_eq!(image.memory(), before);
        assert!(
            edit_number(
                &mut image,
                "noise_blanker_width_raw",
                NumberInput::Display,
                b'.'
            )
            .unwrap()
            .stored
        );
        assert_eq!(image.memory(), before);
        edit_number(&mut image, "cw_pitch_hz", NumberInput::Text(b"303"), b'.').unwrap();
        assert_eq!(image.cw_pitch_hz().unwrap(), 305);
        edit_number(
            &mut image,
            "cw_dot_dash_ratio_tenths",
            NumberInput::Text(b"3.15"),
            b'.',
        )
        .unwrap();
        assert_eq!(image.cw_dot_dash_ratio_tenths().unwrap(), 32);
    }

    #[test]
    fn every_published_number_executes_typed_cancel_begin_and_wrap() {
        let fields = number_fields();
        assert_eq!(fields.len(), 46);
        for field in fields {
            for index in 0..field.count {
                let mut image = image();
                if !field.accepts_index(index) {
                    let before = image.memory().to_vec();
                    assert!(
                        edit_number_at(&mut image, field.name, index, NumberInput::Next, b'.')
                            .is_err()
                    );
                    assert_eq!(image.memory(), before);
                    continue;
                }
                let storage = number_settings()
                    .into_iter()
                    .find(|s| s.name == field.name)
                    .unwrap();
                let effect =
                    edit_number_at(&mut image, field.name, index, NumberInput::Text(b"0"), b'.')
                        .unwrap();
                assert!(effect.stored);
                assert_eq!(
                    edit_number_at(&mut image, field.name, index, NumberInput::Display, b'.')
                        .unwrap()
                        .candidate,
                    field.minimum.max(0)
                );
                let before = image.memory().to_vec();
                let cancelled =
                    edit_number_at(&mut image, field.name, index, NumberInput::Cancel, b'.')
                        .unwrap();
                assert!(!cancelled.accepted);
                let begin = edit_number_at(&mut image, field.name, index, NumberInput::Begin, b'.')
                    .unwrap();
                assert_eq!(begin.state_out, 2);
                assert!(begin.suffix.is_empty());
                assert_eq!(image.memory(), before);
                if field.key == 0x2ca {
                    storage
                        .store_numeric_raw(&mut image, index, (field.maximum / 2 - 1) as i32)
                        .unwrap();
                } else {
                    storage
                        .write(&mut image, index, &Value::Number(field.maximum))
                        .unwrap();
                }
                edit_number_at(&mut image, field.name, index, NumberInput::Next, b'.').unwrap();
                assert_eq!(
                    edit_number_at(&mut image, field.name, index, NumberInput::Display, b'.')
                        .unwrap()
                        .candidate,
                    field.minimum
                );
                edit_number_at(&mut image, field.name, index, NumberInput::Previous, b'.').unwrap();
                assert_eq!(
                    edit_number_at(&mut image, field.name, index, NumberInput::Display, b'.')
                        .unwrap()
                        .candidate,
                    field.maximum
                );
                let before = image.memory().to_vec();
                assert!(
                    edit_number_at(&mut image, field.name, field.count, NumberInput::Next, b'.')
                        .is_err()
                );
                assert!(
                    edit_number_at(
                        &mut image,
                        field.name,
                        usize::MAX,
                        NumberInput::Text(b"1"),
                        b'.'
                    )
                    .is_err()
                );
                assert_eq!(image.memory(), before);
            }
        }
    }

    #[test]
    fn numeric_selected_branch_preserves_raw_dword_before_low_byte_and_bias() {
        for name in [
            "speech_compression_level",
            "noise_blanker_depth",
            "noise_reduction_level",
            "rtty_threshold",
            "phones_level_ratio",
            "dr_noise_reduction_level",
        ] {
            let mut image = image();
            let field = number_settings()
                .into_iter()
                .find(|s| s.name == name)
                .unwrap();
            let before = image.memory().to_vec();
            for (key, phase, accepted) in [(field.key, 2, true), (1, 1, true), (1, 2, false)] {
                let out = edit_number(
                    &mut image,
                    name,
                    NumberInput::Selected(super::super::Selection {
                        key,
                        phase,
                        candidate: 0x1fe,
                        accepted,
                    }),
                    b'.',
                )
                .unwrap();
                assert!(!out.accepted);
                assert!(!out.stored);
                assert_eq!(image.memory(), before);
            }
            let out = edit_number(
                &mut image,
                name,
                NumberInput::Selected(super::super::Selection {
                    key: 1,
                    phase: 2,
                    candidate: 0x1fe,
                    accepted: true,
                }),
                b'.',
            )
            .unwrap();
            assert!(out.stored);
            assert_eq!(out.raw_candidate, 510);
            assert_eq!(field.read_numeric_raw(&image, 0).unwrap(), 254);
            let Value::Number(value) = field.read(&image, 0).unwrap();
            assert_eq!(value, 254 + field.bias);
        }
    }

    #[test]
    fn utc_is_not_a_decimal_minute_editor_and_preserves_false_and_old_keyboard_display() {
        let mut image = image();
        for (text, minutes) in [
            (b"-+00:05".as_slice(), -5),
            (b"+-00:05", 5),
            (b"256", 0),
            (b"0:258", 0),
            (b"1:2:3", 60),
            (b"14:01", 840),
            (b":::::", 0),
        ] {
            let out = edit_utc_offset(&mut image, NumberInput::Text(text)).unwrap();
            assert!(out.stored);
            assert_eq!(out.candidate, minutes);
        }
        let before = image.memory().to_vec();
        for text in [b"14:03".as_slice(), b"00:58", b"123456"] {
            assert!(
                !edit_utc_offset(&mut image, NumberInput::Text(text))
                    .unwrap()
                    .stored
            );
            assert_eq!(image.memory(), before);
        }
        image
            .patch_memory(&[(0x396ff, &32767_i16.to_le_bytes())])
            .unwrap();
        let out = edit_utc_offset(&mut image, NumberInput::Next).unwrap();
        assert_eq!(out.candidate, -840);
        assert_eq!(out.suffix, b"+34:07");
    }

    #[test]
    fn text_messages_reuse_storage_but_keep_native_filter_alias_and_fixed_replacement() {
        let mut image = image();
        let out = edit_dtmf(&mut image, 15, TextInput::Text(b"1-2.3EFef")).unwrap();
        assert_eq!(out.display, b"123*#*#");
        assert_eq!(
            &image.dtmf_memory_raw(15).unwrap()[..7],
            &[1, 2, 3, 14, 15, 14, 15]
        );
        edit_dtmf(&mut image, 15, TextInput::Text(b"")).unwrap();
        assert_eq!(image.dtmf_memory_raw(15).unwrap(), &[0xff; 24]);
        let before = image.memory().to_vec();
        assert!(edit_dtmf(&mut image, 16, TextInput::Text(b"0")).is_err());
        assert_eq!(image.memory(), before);
        let out = edit_rtty(&mut image, 7, TextInput::Text(b"ab\\c\0d!@\xff")).unwrap();
        assert_eq!(out.display, b"AB\\C D!");
        assert_eq!(&image.rtty_memory_raw(7).unwrap()[..9], b"AB\rC D!  ");
        let before = image.memory().to_vec();
        assert!(!edit_rtty(&mut image, 7, TextInput::Cancel).unwrap().stored);
        assert!(edit_rtty(&mut image, 8, TextInput::Text(b"x")).is_err());
        assert_eq!(image.memory(), before);
    }

    #[test]
    fn all_eight_color_operations_preserve_cancel_and_the_fourth_byte() {
        for (field, at) in ColorField::ALL.into_iter().zip([
            0x3972d, 0x39730, 0x39733, 0x3995b, 0x3995f, 0x39978, 0x39981, 0x39984,
        ]) {
            let mut image = image();
            let before = image.memory().to_vec();
            let cancelled = edit_color(&mut image, field, None).unwrap();
            assert_eq!(cancelled.value, [0xa5; 3]);
            assert!(!cancelled.stored);
            assert_eq!(image.memory(), before);
            let chosen = edit_color(&mut image, field, Some([0, 255, 4])).unwrap();
            assert!(chosen.stored);
            assert_eq!(chosen.notify_flags, 0);
            assert_eq!(field.read(&image).unwrap(), [0, 255, 4]);
            let mut expected = before.clone();
            expected[at..at + 3].copy_from_slice(&[0, 255, 4]);
            assert_eq!(image.memory(), expected);
            assert_eq!(
                before
                    .iter()
                    .zip(image.memory())
                    .filter(|(a, b)| a != b)
                    .count(),
                3
            );
            let after = image.memory().to_vec();
            assert!(!edit_color(&mut image, field, None).unwrap().stored);
            assert_eq!(image.memory(), after);
        }
    }

    #[test]
    fn filter_derived_values_keep_unknown_bw_signed_pbt_and_negative_passbands() {
        let mut image = image();
        image
            .patch_memory(&[(0x37850, &[0]), (0x37867, &[127, 128])])
            .unwrap();
        let before = image.memory().to_vec();
        let view = filter_view(&image, 0).unwrap();
        assert_eq!(view.bandwidth_hz, 50);
        assert_eq!(view.pbt_hz, [6350, -6400]);
        assert_eq!(view.passband_hz, -12700);
        assert_eq!(view.shift_hz, -25);
        image.patch_memory(&[(0x37850, &[255])]).unwrap();
        assert_eq!(filter_view(&image, 0).unwrap().bandwidth_hz, 25100);
        assert!(filter_view(&image, 15).is_err());
        assert!(filter_view(&image, usize::MAX).is_err());
        let mut expected = before;
        expected[0x37850] = 255;
        assert_eq!(image.memory(), expected);
    }

    #[test]
    fn choice_cases_cover_native_axes_raw_accept_cancel_wrap_and_picture_dependency() {
        assert_eq!(choice_fields().len(), 24);
        for field in choice_fields() {
            for index in field.first_index..field.first_index + field.count {
                let mut image = image();
                let before = image.memory().to_vec();
                assert_eq!(field.read(&image, index).unwrap(), 165);
                assert!(
                    !field
                        .edit(&mut image, index, ChoiceInput::Cancel)
                        .unwrap()
                        .stored
                );
                assert!(
                    field
                        .edit(&mut image, index, ChoiceInput::Begin)
                        .unwrap()
                        .popup_requested
                );
                assert_eq!(image.memory(), before);
                let out = field
                    .edit(
                        &mut image,
                        index,
                        ChoiceInput::Selected(super::super::Selection {
                            key: 1,
                            phase: 2,
                            candidate: 0x1fe,
                            accepted: true,
                        }),
                    )
                    .unwrap();
                assert!(out.stored);
                assert_eq!(out.value, 254);
                assert_eq!(out.notify_flags, 0);
                let mut expected = before;
                expected[field.storage.offset + field.storage.stride * index] = 254;
                if matches!(field.key(), 0x1b0 | 0x1b1) {
                    expected[0x37d87] = 1;
                }
                assert_eq!(image.memory(), expected);
                let out = field.edit(&mut image, index, ChoiceInput::Next).unwrap();
                assert_eq!(out.value, field.domain().min);
                assert_eq!(
                    field
                        .edit(&mut image, index, ChoiceInput::Previous)
                        .unwrap()
                        .value,
                    field.domain().max
                );
                let before = image.memory().to_vec();
                assert!(
                    field
                        .edit(&mut image, usize::MAX, ChoiceInput::Next)
                        .is_err()
                );
                if field.first_index != 0 {
                    assert!(field.read(&image, field.first_index - 1).is_err());
                }
                assert_eq!(image.memory(), before);
            }
        }
        let language = &choice_fields()[0];
        let mut image = image();
        assert!(
            language
                .edit(&mut image, 0, ChoiceInput::Display)
                .unwrap()
                .gray
        );
        image.patch_memory(&[(0x396bc, &[1])]).unwrap();
        assert!(
            !language
                .edit(&mut image, 0, ChoiceInput::Display)
                .unwrap()
                .gray
        );
    }

    #[test]
    fn rf_choices_preserve_fixed_rows_holes_and_opaque_bytes() {
        assert_eq!(rf_choice_fields().len(), 20);
        for field in rf_choice_fields() {
            for index in field.first_index..field.first_index + field.count {
                let mut image = image();
                let before = image.memory().to_vec();
                if !field.accepts_index(index) {
                    assert!(field.edit(&mut image, index, chosen(1)).is_err());
                    assert_eq!(image.memory(), before);
                    continue;
                }
                let out = field.edit(&mut image, index, ChoiceInput::Cancel).unwrap();
                assert!(!out.stored);
                assert_eq!(image.memory(), before);
                let out = field.edit(&mut image, index, chosen(254)).unwrap();
                if field.fixed_label(index).unwrap().is_some() {
                    assert!(!out.stored);
                    assert_eq!(image.memory(), before);
                    continue;
                }
                assert!(out.stored);
                assert_eq!(out.value, 254);
                let at = field.storage.offset + field.storage.stride * index;
                assert_eq!(image.memory()[at], 254);
                let allowed = if matches!(field.key(), 0x344 | 0x345) {
                    vec![
                        at,
                        0x392da + index,
                        0x392e2 + index,
                        0x392ca + index,
                        0x392d2 + index,
                    ]
                } else {
                    vec![at]
                };
                assert!(
                    before
                        .iter()
                        .zip(image.memory())
                        .enumerate()
                        .all(|(at, (a, b))| a == b || allowed.contains(&at))
                );
                let before = image.memory().to_vec();
                assert!(field.edit(&mut image, usize::MAX, chosen(0)).is_err());
                assert_eq!(image.memory(), before);
            }
        }
    }

    #[test]
    fn tone_coupling_uses_low_byte_and_not_value_change_as_notification_gate() {
        let hpf = &rf_choice_fields()[0];
        let lpf = &rf_choice_fields()[1];
        let mut image = image();
        image
            .patch_memory(&[
                (0x392da, &[0]),
                (0x392e2, &[25]),
                (0x392ca, &[5]),
                (0x392d2, &[5]),
            ])
            .unwrap();
        assert_eq!(hpf.edit(&mut image, 0, chosen(0)).unwrap().notify_flags, 0);
        assert_eq!(
            hpf.edit(&mut image, 0, chosen(255)).unwrap().notify_flags,
            0x20
        );
        assert_eq!(image.memory()[0x392e2], 0);
        assert_eq!(
            lpf.edit(&mut image, 0, chosen(0)).unwrap().notify_flags,
            0x20
        );
        assert_eq!(image.memory()[0x392da], 255);
        image
            .patch_memory(&[
                (0x392da, &[1]),
                (0x392e2, &[25]),
                (0x392ca, &[5]),
                (0x392d2, &[5]),
            ])
            .unwrap();
        let before = image.memory().to_vec();
        assert_eq!(
            hpf.edit(&mut image, 0, chosen(1)).unwrap().notify_flags,
            0x20
        );
        assert_eq!(image.memory(), before);
        //79FE0 and7A0DD test only the edited endpoint, not the combined pair.
        image
            .patch_memory(&[
                (0x392da, &[0]),
                (0x392e2, &[20]),
                (0x392ca, &[7]),
                (0x392d2, &[8]),
            ])
            .unwrap();
        assert_eq!(hpf.edit(&mut image, 0, chosen(0)).unwrap().notify_flags, 0);
        assert_eq!(&image.memory()[0x392ca..0x392cb], &[7]);
        image
            .patch_memory(&[(0x392da, &[1]), (0x392e2, &[25])])
            .unwrap();
        assert_eq!(lpf.edit(&mut image, 0, chosen(25)).unwrap().notify_flags, 0);
        assert_eq!(image.memory()[0x392d2], 8);
        // WFM's Rx EQ branch resets HPF/LPF although those UI rows do not exist.
        let input = NumberInput::Selected(super::super::Selection {
            key: 1,
            phase: 2,
            candidate: 261,
            accepted: true,
        });
        assert_eq!(
            edit_number_at(&mut image, "receive_bass", 3, input, b'.')
                .unwrap()
                .notify_flags,
            0
        );
        let input = NumberInput::Selected(super::super::Selection {
            key: 1,
            phase: 2,
            candidate: 262,
            accepted: true,
        });
        assert_eq!(
            edit_number_at(&mut image, "receive_bass", 3, input, b'.')
                .unwrap()
                .notify_flags,
            0x20
        );
        assert_eq!(image.memory()[0x392dd], 0);
        assert_eq!(image.memory()[0x392e5], 25);
    }

    #[test]
    fn packed_operations_reuse_typed_masks_and_do_not_run_neighbor_scan_reset() {
        for (field, at, mask) in [
            (PackedField::Vox, 0x37781, 0x80),
            (PackedField::OneHz, 0x37780, 2),
            (PackedField::SelectMemoryScan, 0x37802, 3),
        ] {
            for old in [0, 255, 165] {
                let mut image = image();
                image.patch_memory(&[(at, &[old])]).unwrap();
                let before = image.memory().to_vec();
                assert!(!field.edit(&mut image, ChoiceInput::Cancel).unwrap().stored);
                assert_eq!(image.memory(), before);
                let out = field.edit(&mut image, chosen(-1)).unwrap();
                assert!(out.stored);
                assert_eq!(out.notify_flags, 0);
                let mut expected = before;
                expected[at] = old | mask;
                assert_eq!(image.memory(), expected);
                assert!(field.edit(&mut image, ChoiceInput::Next).unwrap().stored);
            }
        }
    }

    #[test]
    fn fixed_text_profiles_execute_real_ascii_edits_and_preserve_other_windows() {
        for (field, at, text, expected) in [
            (
                FixedTextField::GatewayAddress,
                0x39562,
                b"a-b.c/d:8".as_slice(),
                b"a-b.cd8".as_slice(),
            ),
            (
                FixedTextField::NtpAddress,
                0x396be,
                b" a-b.c/d:8 ",
                b"a-b.cd8",
            ),
            (FixedTextField::Comment, 0x35591, b"a /b ", b"a /b"),
            (
                FixedTextField::GatewayCallsign,
                0x395a2,
                b"ja1abc a",
                b"JA1ABC A",
            ),
            (
                FixedTextField::PictureRecipient,
                0x399f9,
                b"ab/c d",
                b"AB C D  ",
            ),
            (FixedTextField::NetworkName, 0x3936c, b". .a/b:c", b"A B C"),
            (FixedTextField::Ssid, 0x39a07, b"a/b c\\d", b"a/b c\\d"),
            (
                FixedTextField::ApPassword,
                0x39a27,
                b"aaaaaaaa",
                b"aaaaaaaa",
            ),
            (FixedTextField::NetworkRadioName, 0x39551, b"a\\b", b"a b"),
            (FixedTextField::Keyer, 0x37158, b"ab!c**", b"AB C* "),
        ] {
            let mut image = image();
            let before = image.memory().to_vec();
            let mut source = text.to_vec();
            let out = edit_fixed_text(
                &mut image,
                field,
                0,
                FixedTextInput::Text {
                    source: &mut source,
                    phase: 1,
                },
                0,
            )
            .unwrap();
            assert!(out.text.stored, "{field:?}");
            let mut raw = vec![b' '; field.width()];
            raw[..expected.len()].copy_from_slice(expected);
            let mut expected_image = before;
            expected_image[at..at + field.width()].copy_from_slice(&raw);
            assert_eq!(image.memory(), expected_image, "{field:?}");
            let before = image.memory().to_vec();
            assert!(
                !edit_fixed_text(&mut image, field, 0, FixedTextInput::Cancel, 0)
                    .unwrap()
                    .text
                    .stored
            );
            assert!(
                edit_fixed_text(&mut image, field, field.count(), FixedTextInput::Display, 0)
                    .is_err()
            );
            assert!(
                edit_fixed_text(&mut image, field, usize::MAX, FixedTextInput::Display, 0).is_err()
            );
            assert_eq!(image.memory(), before);
        }
    }

    #[test]
    fn fixed_text_acceptance_alignment_hooks_and_native_failure_prefix_are_distinct() {
        let mut image = image();
        let mut source = b"AB CD".to_vec();
        edit_fixed_text(
            &mut image,
            FixedTextField::PictureRecipient,
            0,
            FixedTextInput::Text {
                source: &mut source,
                phase: 1,
            },
            1,
        )
        .unwrap();
        assert_eq!(&image.memory()[0x399f9..0x39a01], b"AB    CD");
        image
            .patch_memory(&[(0x39359, &[2]), (0x39a05, &[1]), (0x39a27, &[b' '; 63])])
            .unwrap();
        let before = image.memory().to_vec();
        for field in [
            FixedTextField::Ssid,
            FixedTextField::ApPassword,
            FixedTextField::GatewayCallsign,
        ] {
            let mut source = if field == FixedTextField::Ssid {
                b"".to_vec()
            } else if field == FixedTextField::ApPassword {
                b"short".to_vec()
            } else {
                b"JP1YAA A".to_vec()
            };
            let out = edit_fixed_text(
                &mut image,
                field,
                0,
                FixedTextInput::Text {
                    source: &mut source,
                    phase: 1,
                },
                0,
            )
            .unwrap();
            assert!(!out.text.stored);
            assert_eq!(image.memory(), before);
            if field == FixedTextField::ApPassword {
                assert_eq!(out.context60, Some(3));
                assert_eq!(out.warning_bits, 2);
                assert_eq!(out.text.display, b"*****");
            }
        }
        let mut source = b"A\0B".to_vec();
        let out = edit_fixed_text_with_nls(
            &mut image,
            FixedTextField::Comment,
            0,
            FixedTextInput::Text {
                source: &mut source,
                phase: 1,
            },
            0,
            |_, _| {
                Err(crate::csv::Error::SourceUnavailable(
                    "synthetic native failure",
                ))
            },
            |_| unreachable!(),
        );
        assert!(out.is_err());
        assert_eq!(source, b"A B");
        assert_eq!(image.memory(), before);
        let mut source = b"abc".to_vec();
        let out = edit_fixed_text_with_nls(
            &mut image,
            FixedTextField::PictureRecipient,
            0,
            FixedTextInput::Text {
                source: &mut source,
                phase: 1,
            },
            0,
            |_, dest| {
                dest[..3].copy_from_slice(b"xyz");
                Ok(0)
            },
            |_| {
                Err(crate::csv::Error::SourceUnavailable(
                    "synthetic upper failure",
                ))
            },
        );
        assert!(out.is_err());
        assert_eq!(source, b"xyz");
        assert_eq!(image.memory(), before);
        let mut source = b"abc".to_vec();
        let out = edit_fixed_text_with_nls(
            &mut image,
            FixedTextField::Comment,
            0,
            FixedTextInput::Text {
                source: &mut source,
                phase: 1,
            },
            0,
            |_, dest| {
                dest[0] = b'Z';
                Ok(0)
            },
            |_| unreachable!(),
        )
        .unwrap();
        assert!(out.text.stored);
        assert_eq!(out.text.display, b"Z");
    }

    #[test]
    fn keyer_false_display_repairs_only_current_stars_and_clears_hidden_nul_tail() {
        let mut image = image();
        image
            .patch_memory(&[(0x37158, b"A**\0TAIL"), (0x37158 + 70, b"B*\0TAIL")])
            .unwrap();
        let before = image.memory().to_vec();
        assert!(
            !edit_fixed_text(
                &mut image,
                FixedTextField::Keyer,
                0,
                FixedTextInput::Cancel,
                0
            )
            .unwrap()
            .text
            .stored
        );
        assert_eq!(image.memory(), before);
        let out = edit_fixed_text(
            &mut image,
            FixedTextField::Keyer,
            0,
            FixedTextInput::Display,
            0,
        )
        .unwrap();
        assert!(out.text.stored);
        assert_eq!(out.text.display, b"A  ");
        let mut expected = before;
        expected[0x37158..0x37158 + 70].fill(b' ');
        expected[0x37158] = b'A';
        assert_eq!(image.memory(), expected);
    }

    #[test]
    fn civ_hex_and_cw_rise_use_their_own_grammars_and_raw_projection() {
        let mut image = image();
        for (text, value) in [
            (b"".as_slice(), 2),
            (b"xyz", 2),
            (b"foo", 15),
            (b"FF", 223),
            (b"$DF", 223),
            (b"0xDF", 13),
            (b"1234", 18),
        ] {
            let out =
                edit_number(&mut image, "civ_address", NumberInput::Text(text), b'.').unwrap();
            assert!(out.accepted);
            assert_eq!(out.candidate, value);
            assert_eq!(image.memory()[0x3934d], value as u8);
        }
        edit_number(
            &mut image,
            "cw_rise_milliseconds",
            NumberInput::Text(b"3"),
            b'.',
        )
        .unwrap();
        assert_eq!(image.memory()[0x39973], 1);
        let input = NumberInput::Selected(super::super::Selection {
            key: 1,
            phase: 2,
            candidate: 3,
            accepted: true,
        });
        edit_number(&mut image, "cw_rise_milliseconds", input, b'.').unwrap();
        assert_eq!(image.memory()[0x39973], 0);
        image.patch_memory(&[(0x39973, &[255])]).unwrap();
        let before = image.memory().to_vec();
        let out = edit_number(
            &mut image,
            "cw_rise_milliseconds",
            NumberInput::Display,
            b'.',
        )
        .unwrap();
        assert_eq!(out.candidate, 512);
        assert!(!out.stored);
        assert_eq!(image.memory(), before);
    }

    #[test]
    fn pbt_display_clips_but_keyboard_does_not_wrap_and_uses_live_cw_bounds() {
        use crate::icf::PbtSide;
        let mut image = image();
        image
            .patch_memory(&[(0x37850, &[0]), (0x37867, &[127, 128])])
            .unwrap();
        let out = edit_pbt(&mut image, 0, PbtSide::One, NumberInput::Display, b'.').unwrap();
        assert_eq!(
            (out.candidate, out.raw_candidate, out.notify_flags),
            (25, 0, 0x20)
        );
        assert_eq!(image.memory()[0x37868], 128);
        image
            .patch_memory(&[(0x37850, &[9]), (0x37867, &[5, 0])])
            .unwrap();
        let before = image.memory().to_vec();
        assert!(
            !edit_pbt(&mut image, 0, PbtSide::One, NumberInput::Next, b'.')
                .unwrap()
                .stored
        );
        assert_eq!(image.memory(), before);
        edit_pbt(
            &mut image,
            0,
            PbtSide::One,
            NumberInput::Text(b"-125"),
            b'.',
        )
        .unwrap();
        assert_eq!(image.memory()[0x37867], (-3_i8) as u8);
        image.patch_memory(&[(0x37856, &[20])]).unwrap();
        let out = edit_pbt(
            &mut image,
            6,
            PbtSide::Two,
            NumberInput::Text(b"99999"),
            b'.',
        )
        .unwrap();
        assert_eq!(out.candidate, 300);
        let before = image.memory().to_vec();
        assert!(edit_pbt(&mut image, 15, PbtSide::One, NumberInput::Text(b"0"), b'.').is_err());
        assert_eq!(image.memory(), before);
    }

    #[test]
    fn bandwidth_uses_shared_codec_two_step_input_and_only_changed_raw_resets_pbt() {
        for index in 0..15 {
            let mut image = image();
            image
                .patch_memory(&[(0x37850 + index, &[9]), (0x37867 + 2 * index, &[1, 255])])
                .unwrap();
            let before = image.memory().to_vec();
            assert!(
                !edit_bandwidth(&mut image, index, NumberInput::Cancel, b'.')
                    .unwrap()
                    .number
                    .stored
            );
            assert_eq!(image.memory(), before);
            let old = filter_view(&image, index).unwrap().bandwidth_hz;
            let input = NumberInput::Selected(super::super::Selection {
                key: 1,
                phase: 2,
                candidate: old + 1,
                accepted: true,
            });
            let out = edit_bandwidth(&mut image, index, input, b'.').unwrap();
            assert!(out.number.stored);
            assert_eq!(out.number.notify_flags, 0);
            assert_eq!(image.memory(), before);
            let out = edit_bandwidth(&mut image, index, NumberInput::Text(b"475.2"), b'.').unwrap();
            assert_eq!(out.rewritten_text.as_deref(), Some(b"475".as_slice()));
            assert_eq!(out.number.candidate, if index / 3 == 4 { 400 } else { 500 });
        }
        let mut image = image();
        image
            .patch_memory(&[(0x37850, &[9]), (0x37867, &[1, 255])])
            .unwrap();
        assert_eq!(
            edit_bandwidth(&mut image, 0, NumberInput::Previous, b'.')
                .unwrap()
                .number
                .candidate,
            450
        );
        assert_eq!(&image.memory()[0x37867..0x37869], &[0, 0]);
        image.patch_memory(&[(0x37850, &[9])]).unwrap();
        assert_eq!(
            edit_bandwidth(&mut image, 0, NumberInput::Next, b'.')
                .unwrap()
                .number
                .candidate,
            600
        );
        let before = image.memory().to_vec();
        assert!(edit_bandwidth(&mut image, 15, NumberInput::Next, b'.').is_err());
        assert_eq!(image.memory(), before);
    }

    #[test]
    fn metadata_is_bounded_read_only_and_only_explicit_clear_touches_sentinels() {
        for field in MetadataField::ALL {
            let mut image = image();
            let at = field.at();
            image.patch_memory(&[(at, b"a.ext.ext\0OPAQUE")]).unwrap();
            let before = image.memory().to_vec();
            let view = field.read(&image);
            assert_eq!(view.name, b"a.ext.ext");
            assert_eq!(view.stem, b"a");
            assert!(view.read_only);
            assert_eq!(image.memory(), before);
            assert!(!field.clear(&mut image, 0).unwrap().stored);
            assert_eq!(image.memory(), before);
            assert!(
                field
                    .clear(&mut image, field.clear_command())
                    .unwrap()
                    .stored
            );
            let mut expected = before;
            expected[at] = 0;
            if field == MetadataField::TransmitPicture {
                expected[0x37da5..0x37dab].copy_from_slice(&[0, 255, 0, 0, 0, 0]);
            }
            assert_eq!(image.memory(), expected);
            assert!(!field.read(&image).attached);
        }
    }

    #[test]
    fn home_vfo_reuses_generated_ranges_reparses_text_and_preserves_sticky_warnings() {
        use crate::icf::{context::Context, frequency::Ranges};
        let q = Context::before_registry();
        let ranges = Ranges::generate(&q, Some(0x411)).unwrap();
        let mut image = image();
        let before = image.memory().to_vec();
        assert!(
            edit_home_vfo(
                &mut image,
                &Ranges::unpopulated(),
                &q,
                NumberInput::Text(b"14.2"),
                b'.',
                0
            )
            .is_err()
        );
        assert_eq!(image.memory(), before);
        let out =
            edit_home_vfo(&mut image, &ranges, &q, NumberInput::Text(b"14.2"), b'.', 8).unwrap();
        assert!(out.number.stored);
        assert_eq!(out.number.candidate, 14_200_000);
        assert_eq!(out.warning_bits, 8);
        let mut expected = before;
        expected[0x392f6..0x392fa].copy_from_slice(&14_200_000_u32.to_le_bytes());
        assert_eq!(image.memory(), expected);
        assert!(
            edit_home_vfo(&mut image, &ranges, &q, NumberInput::Display, b'.', 8)
                .unwrap()
                .outside_marker
        );
        let before = image.memory().to_vec();
        assert!(
            !edit_home_vfo(&mut image, &ranges, &q, NumberInput::Cancel, b'.', 8)
                .unwrap()
                .number
                .stored
        );
        assert_eq!(image.memory(), before);
        let out = edit_home_vfo(&mut image, &ranges, &q, NumberInput::Text(b""), b'.', 8).unwrap();
        assert!(out.empty_marker);
        assert_eq!(out.number.candidate, 0);
        assert_eq!(out.warning_bits, 8);
    }

    #[test]
    fn program_scan_ps01_ps02_bind_current_and_filter_only_trimmed_c_prefix() {
        fn empty() -> Icf<'static> {
            let mut image = image();
            image
                .patch_memory(&[(0xa60b, &[255; 4]), (0x37802, &[255]), (0x3a8ca, &[1])])
                .unwrap();
            image
        }
        fn row(image: &mut Icf<'_>, index: usize, low: u32, high: u32) {
            let flags = image.memory()[0xa60b + index / 8] & !(1 << (index % 8));
            image
                .patch_memory(&[
                    (0xa60b + index / 8, &[flags]),
                    (0xa368 + 27 * index, &high.to_le_bytes()),
                    (0xa36c + 27 * index, &low.to_le_bytes()),
                ])
                .unwrap();
        }
        let mut image = empty();
        let mut current = empty();
        row(&mut current, 5, 145_000_000, 146_000_000);
        let mut text = b"5".to_vec();
        let before = image.memory().to_vec();
        let effect = edit_program_scan(
            &mut image,
            current.memory(),
            &[],
            ProgramScanInput::Text(&mut text),
        )
        .unwrap();
        assert!(effect.accepted);
        assert_eq!(effect.selected, 5);
        let mut expected = before;
        expected[0x37802] = 0x97;
        assert_eq!(image.memory(), expected);
        let mut image = empty();
        row(&mut image, 0, 145_000_000, 146_000_000);
        row(&mut image, 5, 145_000_000, 146_000_000);
        let current = empty();
        assert_eq!(
            program_scan_choices(&image, current.memory()).unwrap(),
            Vec::<i64>::new()
        );
        let before = image.memory().to_vec();
        let mut text = b"5".to_vec();
        assert!(
            !edit_program_scan(
                &mut image,
                current.memory(),
                &[],
                ProgramScanInput::Text(&mut text)
            )
            .unwrap()
            .accepted
        );
        assert_eq!(image.memory(), before);
        let image = empty();
        let mut current = empty();
        row(&mut current, 0, 30_000, 470_000_000);
        row(&mut current, 24, 433_000_000, 430_000_000);
        row(&mut current, 1, 145_000_000, 145_000_000);
        row(&mut current, 2, 29_999, 470_000_000);
        assert_eq!(
            program_scan_choices(&image, current.memory()).unwrap(),
            vec![0, 24]
        );
        assert!(program_scan_choices(&image, &[]).is_err());
        assert!(program_scan_choices(&image, &vec![255; 0xa60e]).is_err());
        for index in [5, 10, 12] {
            row(&mut current, index, 30_000, 470_000_000);
        }
        for (input, normalized, candidate) in [
            (b"P0\0 5".as_slice(), b"0".as_slice(), 0),
            (b"\0 P05\0", b"05", 5),
            (b"12\0 -3", b"12", 12),
            (b"0x10", b"010", 10),
            (b"000000000000000000000024", b"000000000000000000000024", 24),
        ] {
            let mut image = empty();
            let mut text = input.to_vec();
            let before = image.memory().to_vec();
            let effect = edit_program_scan(
                &mut image,
                current.memory(),
                &[],
                ProgramScanInput::Text(&mut text),
            )
            .unwrap();
            assert_eq!(
                (effect.accepted, effect.candidate, effect.selected),
                (true, candidate, candidate as u8)
            );
            assert_eq!(text, normalized);
            let mut expected = before;
            expected[0x37802] = 0x83 | ((candidate as u8) << 2);
            assert_eq!(image.memory(), expected);
        }
        for input in [b"OFF".as_slice(), b"--1", b"\0 \0"] {
            let mut image = empty();
            let before = image.memory().to_vec();
            let mut text = input.to_vec();
            assert!(
                !edit_program_scan(&mut image, &[], &[], ProgramScanInput::Text(&mut text))
                    .unwrap()
                    .accepted
            );
            assert_eq!(image.memory(), before);
        }
    }

    #[test]
    fn program_scan_keeps_text_list_current_raw_codes_and_ordered_reset_prefix_distinct() {
        let mut image = image();
        let mut patches = vec![(0xa60b, vec![0; 4]), (0x3a8ca, vec![1])];
        for row in 0..25 {
            patches.push((
                0xa368 + 27 * row,
                [14_300_000_u32.to_le_bytes(), 14_200_000_u32.to_le_bytes()].concat(),
            ));
        }
        image
            .patch_memory(
                &patches
                    .iter()
                    .map(|(at, bytes)| (*at, bytes.as_slice()))
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let current = image.memory().to_vec();
        for (text, row) in [(b"P05".as_slice(), 5), (b"1.0", 10), (b"1e1", 11)] {
            let mut source = text.to_vec();
            let out = edit_program_scan(
                &mut image,
                &current,
                &[],
                ProgramScanInput::Text(&mut source),
            )
            .unwrap();
            assert!(out.accepted);
            assert_eq!(out.selected, row);
            assert_eq!(out.notify_flags, 0x20);
        }
        let mut text = b"25".to_vec();
        assert!(
            !edit_program_scan(&mut image, &current, &[], ProgramScanInput::Text(&mut text))
                .unwrap()
                .accepted
        );
        for code in 25..32 {
            let before = image.memory().to_vec();
            let out = edit_program_scan(
                &mut image,
                &[],
                &[],
                ProgramScanInput::Selected(super::super::Selection {
                    key: 0x10,
                    phase: 2,
                    candidate: code,
                    accepted: true,
                }),
            )
            .unwrap();
            let mut expected = before;
            expected[0x37802] = (expected[0x37802] & 0x83) | ((code as u8) << 2);
            assert_eq!(image.memory(), expected);
            assert_eq!(out.selected, code as u8);
        }
        let before = image.memory().to_vec();
        assert!(
            !edit_program_scan(&mut image, &[], &[], ProgramScanInput::Cancel)
                .unwrap()
                .accepted
        );
        assert_eq!(image.memory(), before);
        image
            .patch_memory(&[(0x37802, &[0x83 | (5 << 2)]), (0xa60b, &[1 << 5])])
            .unwrap();
        let live = image.memory().to_vec();
        assert!(program_scan_choices(&image, &live).unwrap().contains(&5));
        let mut text = b"P05".to_vec();
        assert!(
            !edit_program_scan(&mut image, &live, &[], ProgramScanInput::Text(&mut text))
                .unwrap()
                .accepted
        );
        assert_eq!(text, b"05");
        image
            .patch_memory(&[
                (0xa60b, &[0]),
                (0xa368 + 27 * 5, &14_200_000_u32.to_le_bytes()),
            ])
            .unwrap();
        let mut live = image.memory().to_vec();
        assert!(program_scan_choices(&image, &live).unwrap().contains(&5));
        live[0xa368 + 27 * 5..0xa36c + 27 * 5].fill(0);
        assert!(!program_scan_choices(&image, &live).unwrap().contains(&5));
        image
            .patch_memory(&[
                (0x3a8ca, &[0]),
                (0x3a8c9, &[0]),
                (0x3c84a, &[0]),
                (0x37807, &[1]),
                (0x37806, &[8]),
            ])
            .unwrap();
        let before = image.memory().to_vec();
        let selected = super::super::Selection {
            key: 0x10,
            phase: 2,
            candidate: 31,
            accepted: true,
        };
        assert!(
            edit_program_scan(&mut image, &[], &[], ProgramScanInput::Selected(selected)).is_err()
        );
        let mut expected = before;
        expected[0x37802] = (expected[0x37802] & 0x83) | 0x7c;
        expected[0x37807] = 0;
        assert_eq!(image.memory(), expected);
        image.patch_memory(&[(0x37807, &[1])]).unwrap();
        let mut g = vec![0; 0x37814];
        g[0x37810..0x37814].copy_from_slice(&[1, 2, 3, 4]);
        let out =
            edit_program_scan(&mut image, &[], &g, ProgramScanInput::Selected(selected)).unwrap();
        assert_eq!(out.notify_flags, 0x20);
        assert_eq!(&image.memory()[0x37810..0x37814], &[1, 2, 3, 4]);
    }

    #[test]
    fn fixed_predicate_borrows_raw_holder_and_shares_editor_warnings() {
        use crate::icf::{context::Context, frequency::Ranges, record, record_image};
        let q = Context::before_registry();
        let ranges = Ranges::generate(&q, Some(0x411)).unwrap();
        let mut raw = vec![0; 0x4997a];
        for index in [0, 40, 67] {
            let band = ranges.f760(0x16, index / 4, 0);
            let at = 0x3973a + index as usize * 8;
            for pair in [
                [band.low + 10_000, band.low + 5_000],
                [0, u32::MAX],
                [band.low + 5_000, band.low + 10_000],
            ] {
                raw[at..at + 4].copy_from_slice(&pair[0].to_le_bytes());
                raw[at + 4..at + 8].copy_from_slice(&pair[1].to_le_bytes());
                let mut holder = record::reset_holder(0x16, index as u32, 1);
                record_image::load_range_into(&raw, 0x16, index as u32, &mut holder).unwrap();
                assert_eq!(
                    fixed_edge_bad(&holder, &ranges, &q, false).unwrap(),
                    fixed_edge_warnings(&holder, &ranges, &q, false)
                        .unwrap()
                        .into_iter()
                        .any(|bad| bad)
                );
                assert_eq!(
                    fixed_edge_bad(&holder, &ranges, &q, true).unwrap(),
                    pair.into_iter()
                        .any(|value| !(30_000..=470_000_000).contains(&value))
                );
                if pair[0] > pair[1] {
                    assert!(fixed_edge_bad(&holder, &ranges, &q, false).unwrap());
                }
            }
        }
        let mut holder = record::reset_holder(0x16, 0, 1);
        holder.set_valid(false);
        assert!(!fixed_edge_bad(&holder, &ranges, &q, false).unwrap());
        assert!(!fixed_edge_bad(&holder, &ranges, &q, true).unwrap());
        assert!(fixed_edge_bad(&record::reset_holder(2, 0, 1), &ranges, &q, false).is_err());
    }

    #[test]
    fn fixed_edges_use_raw_x_guard_asymmetric_width_and_ordered_shared_saver() {
        use crate::icf::{context::Context, frequency::Ranges};
        let q = Context::before_registry();
        let ranges = Ranges::generate(&q, Some(0x411)).unwrap();
        let rt = FrequencyRuntime {
            ranges: &ranges,
            live_q: &q,
            binding: 0x1234,
            crt_point: b'.',
        };
        for index in 0..68 {
            let mut image = image();
            let at = 0x3973a + 8 * index;
            let band = ranges.f760(0x16, (index / 4) as i32, 0);
            let hz = band.low + 10_000;
            let text = format!("{}.{:06}", hz / 1_000_000, hz % 1_000_000);
            image
                .patch_memory(&[
                    (at, &(hz - 5000).to_le_bytes()),
                    (at + 4, &u32::MAX.to_le_bytes()),
                ])
                .unwrap();
            let before = image.memory().to_vec();
            assert!(
                !edit_fixed_edge(&mut image, rt, index, EdgeSide::Low, NumberInput::Cancel)
                    .unwrap()
                    .stored
            );
            assert_eq!(image.memory(), before);
            let out = edit_fixed_edge(
                &mut image,
                rt,
                index,
                EdgeSide::Low,
                NumberInput::Text(text.as_bytes()),
            )
            .unwrap();
            assert!(out.stored, "{index}");
            assert_eq!(out.pair_hz, [hz, hz + 1_000_000]);
            assert_eq!(out.raw_candidate_hz, hz);
            let mut expected = before;
            expected[at..at + 4].copy_from_slice(&hz.to_le_bytes());
            expected[at + 4..at + 8].copy_from_slice(&(hz + 1_000_000).to_le_bytes());
            assert_eq!(image.memory(), expected);
            let before = image.memory().to_vec();
            assert!(
                !edit_fixed_edge(
                    &mut image,
                    rt,
                    index,
                    EdgeSide::High,
                    NumberInput::Text(b"0")
                )
                .unwrap()
                .stored
            );
            assert_eq!(image.memory(), before);
            assert!(
                edit_fixed_edge(
                    &mut image,
                    rt,
                    usize::MAX,
                    EdgeSide::Low,
                    NumberInput::Text(b"1")
                )
                .is_err()
            );
            assert_eq!(image.memory(), before);
        }
        let mut image = image();
        let hz = ranges.f760(0x16, 0, 0).low + 10_000;
        let text = format!("{}.{:06}", hz / 1_000_000, hz % 1_000_000);
        let before = image.memory().to_vec();
        assert!(
            edit_fixed_edge_with_store(
                &mut image,
                rt,
                0,
                EdgeSide::Low,
                NumberInput::Text(text.as_bytes()),
                |offset, _| if offset == 4 {
                    Err(invalid("synthetic late pair store"))
                } else {
                    Ok(())
                }
            )
            .is_err()
        );
        let mut expected = before;
        expected[0x3973a..0x3973e].copy_from_slice(&hz.to_le_bytes());
        assert_eq!(image.memory(), expected);
        let mut fields = q.fields();
        fields.0[0] = 5;
        fields.0[1] = 2;
        fields.0[7] = 0;
        let mut hidden = q;
        hidden.adopt(fields);
        assert!(!rf_row_visible(&hidden, RfLayout::Reference, 10).unwrap());
        assert!(!rf_row_visible(&hidden, RfLayout::FixedEdges, 52).unwrap());
        assert!(rf_row_visible(&hidden, RfLayout::FixedEdges, 60).unwrap());
    }

    #[test]
    fn conditional_catalogs_use_independent_j_u_r_and_noncontiguous_native_codes() {
        use crate::icf::context::Context;
        let mut q = Context::before_registry();
        let mut fields = q.fields();
        fields.0[0] = 1;
        q.adopt(fields);
        for field in ConditionalField::ALL {
            for lang in [0x409, 0x411] {
                let runtime = SelectorRuntime {
                    system_langid: lang,
                    descriptor_8645fc: 2,
                    q: &q,
                };
                let catalog = field.catalog(runtime).unwrap();
                let mut image = image();
                let before = image.memory().to_vec();
                assert!(
                    !catalog
                        .edit(&mut image, ChoiceInput::Cancel)
                        .unwrap()
                        .stored
                );
                assert_eq!(image.memory(), before);
                let out = catalog.edit(&mut image, ChoiceInput::Next).unwrap();
                assert_eq!(out.value, catalog.values()[0]);
                let out = catalog.edit(&mut image, chosen(254)).unwrap();
                assert_eq!(out.value, 254);
                let mut expected = before;
                expected[field.descriptor().storage.offset] = 254;
                assert_eq!(image.memory(), expected);
            }
        }
        let runtime = SelectorRuntime {
            system_langid: 0x409,
            descriptor_8645fc: 1,
            q: &q,
        };
        assert!(
            ConditionalField::Coordinates
                .catalog(runtime)
                .unwrap()
                .selector
                .ends_with("_JPN")
        );
        assert!(
            !ConditionalField::AutoRepeater
                .catalog(runtime)
                .unwrap()
                .selector
                .ends_with("_JPN")
        );
        let mut fields = q.fields();
        fields.0[0] = 0;
        q.adopt(fields);
        let catalog = ConditionalField::AutoRepeater
            .catalog(SelectorRuntime {
                system_langid: 0x409,
                descriptor_8645fc: 0,
                q: &q,
            })
            .unwrap();
        assert_eq!(catalog.values(), &[0, 2]);
        let mut image = image();
        catalog.edit(&mut image, chosen(0)).unwrap();
        assert_eq!(
            catalog.edit(&mut image, ChoiceInput::Next).unwrap().value,
            2
        );
        assert_eq!(
            catalog.edit(&mut image, ChoiceInput::Next).unwrap().value,
            0
        );
        //A selected1 remains a raw native bypass, not a checked catalog write.
        assert_eq!(catalog.edit(&mut image, chosen(1)).unwrap().value, 1);
    }

    #[test]
    fn key_assignments_keep_literal_gap_live_q_filter_current_retention_and_raw_bypass() {
        use crate::icf::context::Context;
        let q = Context::before_registry();
        for field in key_assignment_fields() {
            for index in field.first_index..field.first_index + field.count {
                let mut image = image();
                let before = image.memory().to_vec();
                assert!(
                    !edit_key_assignment(&mut image, field.name(), index, ChoiceInput::Cancel, &q)
                        .unwrap()
                        .stored
                );
                assert_eq!(image.memory(), before);
                edit_key_assignment(&mut image, field.name(), index, chosen(23), &q).unwrap();
                assert!(
                    !key_assignment_choices(&image, field.name(), index, &q)
                        .unwrap()
                        .contains(&24)
                );
                assert_eq!(
                    edit_key_assignment(&mut image, field.name(), index, ChoiceInput::Next, &q)
                        .unwrap()
                        .value,
                    25
                );
                assert_eq!(
                    edit_key_assignment(&mut image, field.name(), index, chosen(24), &q)
                        .unwrap()
                        .value,
                    24
                );
                assert!(
                    key_assignment_choices(&image, field.name(), index, &q)
                        .unwrap()
                        .contains(&24)
                );
                let before = image.memory().to_vec();
                assert!(edit_key_assignment(&mut image, field.name(), 4, chosen(1), &q).is_err());
                assert_eq!(image.memory(), before);
            }
        }
    }

    #[test]
    fn dr_fields_store_before_sync_gate_and_keep_code_c_prefix_cleanup_and_unknown_wrap() {
        let mut image = image();
        image
            .patch_memory(&[(0x3a8ca, &[1]), (0x3c8b5, &[0xa5]), (0x3c8b6, &[255])])
            .unwrap();
        let mut flags = 0xa7;
        let before = image.memory().to_vec();
        let out = edit_dr(
            &mut image,
            DrField::DigitalCode,
            NumberInput::Cancel,
            0x1234,
            b'.',
            &mut flags,
        )
        .unwrap();
        assert!(!out.stored);
        assert!(out.gray);
        assert_eq!(flags, 0xa4);
        assert_eq!(image.memory(), before);
        let selected = NumberInput::Selected(super::super::Selection {
            key: 1,
            phase: 2,
            candidate: 0x101,
            accepted: true,
        });
        let out = edit_dr(
            &mut image,
            DrField::DigitalSquelch,
            selected,
            0x1234,
            b'.',
            &mut flags,
        )
        .unwrap();
        assert!(out.stored);
        assert!(!out.synchronized);
        assert_eq!(out.candidate, 257);
        assert_eq!(out.notify_flags, 0x20);
        assert_eq!(image.memory(), before);
        let out = edit_dr(
            &mut image,
            DrField::DigitalCode,
            NumberInput::Previous,
            0x1234,
            b'.',
            &mut flags,
        )
        .unwrap();
        assert_eq!(out.candidate, 0);
        assert!(out.stored);
        assert!(out.gray);
        assert_eq!(
            edit_dr(
                &mut image,
                DrField::DigitalCode,
                NumberInput::Previous,
                0x1234,
                b'.',
                &mut flags
            )
            .unwrap()
            .candidate,
            99
        );
        assert_eq!(
            edit_dr(
                &mut image,
                DrField::DigitalCode,
                NumberInput::Text(b"12\0tail"),
                0x1234,
                b'.',
                &mut flags
            )
            .unwrap()
            .candidate,
            12
        );
        let before = image.memory().to_vec();
        assert!(
            !edit_dr(
                &mut image,
                DrField::DigitalCode,
                NumberInput::Text(b"1e2"),
                0x1234,
                b'.',
                &mut flags
            )
            .unwrap()
            .stored
        );
        assert_eq!(image.memory(), before);
    }

    #[test]
    fn extra_numeric_literal_windows_bias_width_and_last_indexes_are_independent() {
        for (name, index, text, at, bytes) in [
            (
                "rf_power_raw",
                4,
                b"255".as_slice(),
                0x39997,
                b"\xff".as_slice(),
            ),
            ("weather_channel", 0, b"10", 0x3c8b3, b"\x09"),
            ("emr_af_level_raw", 0, b"255", 0x399b9, b"\xff"),
            ("dr_noise_reduction_level", 0, b"15", 0x3c37d, b"\x0f"),
            ("phones_level_ratio", 0, b"-15", 0x39335, b"\x00"),
            ("usb_af_output_level_raw", 0, b"255", 0x39337, b"\xff"),
            ("usb_if_output_level_raw", 0, b"255", 0x3933a, b"\xff"),
            ("usb_modulation_level_raw", 0, b"255", 0x3933d, b"\xff"),
            ("wlan_modulation_level_raw", 0, b"255", 0x3933e, b"\xff"),
            ("network_control_port", 0, b"65535", 0x3937c, b"\xff\xff"),
            ("network_serial_port", 0, b"65535", 0x3937e, b"\xff\xff"),
            ("network_audio_port", 0, b"65535", 0x39380, b"\xff\xff"),
            (
                "scope_reference_tenths_db",
                16,
                b"1.5",
                0x377fb,
                b"\xd7\x00",
            ),
        ] {
            let mut image = image();
            let mut expected = image.memory().to_vec();
            expected[at..at + bytes.len()].copy_from_slice(bytes);
            assert!(
                edit_number_at(&mut image, name, index, NumberInput::Text(text), b'.')
                    .unwrap()
                    .stored
            );
            assert_eq!(image.memory(), expected, "{name}");
        }
    }

    #[test]
    fn raw_scope_and_scan_span_reuse_shared_codecs_without_enum_clamps() {
        let mut image = image();
        let before = image.memory().to_vec();
        assert!(
            !PackedField::ScopeMode
                .edit(&mut image, ChoiceInput::Cancel)
                .unwrap()
                .stored
        );
        assert_eq!(image.memory(), before);
        let out = PackedField::ScopeMode
            .edit(&mut image, chosen(510))
            .unwrap();
        assert_eq!(out.notify_flags, 0x20);
        let mut expected = before;
        expected[0x377c6] = 0;
        expected[0x3808a] = 255;
        assert_eq!(image.memory(), expected);
        let out = PackedField::DeltaFSpan
            .edit(&mut image, chosen(-1))
            .unwrap();
        assert_eq!(out.value, 7);
        assert_eq!(out.notify_flags, 0);
        expected[0x37804] = (expected[0x37804] & 0xf1) | 14;
        assert_eq!(image.memory(), expected);
        assert_eq!(
            PackedField::DeltaFSpan.selector(0x411),
            Some("GI_Sheet_Sel_Common_dFSpanJpn")
        );
        assert_eq!(
            PackedField::DeltaFSpan
                .edit(&mut image, ChoiceInput::Next)
                .unwrap()
                .value,
            0
        );
    }

    #[test]
    fn dr_sync_reencodes_real_kind1b_and_failure_keeps_first_store_and_node_flags() {
        use crate::icf::special::CompactChannel;
        let mut image = image();
        let mut raw = [0_u8; 36];
        let mut h = CompactChannel::decode_bound(Some(&raw), true).unwrap();
        h.frequency_hz = u32::MAX;
        h.mode = 0x10;
        h.write_to(&[0; 36], |at, b| {
            raw[at..at + b.len()].copy_from_slice(b);
            Ok(())
        })
        .unwrap();
        image
            .patch_memory(&[(0x3c801, &raw), (0x3a8ca, &[0]), (0x3a8c9, &[1])])
            .unwrap();
        let mut flags = 0xa5;
        let mut order = Vec::new();
        let out = edit_dr_with_store(
            &mut image,
            DrField::DvFilter,
            NumberInput::Selected(super::super::Selection {
                key: 1,
                phase: 2,
                candidate: 258,
                accepted: true,
            }),
            (0x1234, b'.'),
            &mut flags,
            |at, _| {
                order.push(at);
                Ok(())
            },
        )
        .unwrap();
        assert!(out.synchronized);
        assert_eq!(order, vec![0x3c800, 0x3c801, 0x3c805]);
        assert_eq!(image.memory()[0x3c383], 2);
        assert_eq!(image.memory()[0x3c8b4], 2);
        assert_eq!(image.memory()[0x3c805] & 15, 2);
        let selected = NumberInput::Selected(super::super::Selection {
            key: 1,
            phase: 2,
            candidate: 257,
            accepted: true,
        });
        assert!(
            edit_dr(
                &mut image,
                DrField::DigitalSquelch,
                selected,
                0x1234,
                b'.',
                &mut flags
            )
            .unwrap()
            .synchronized
        );
        assert_eq!(image.memory()[0x3c8b5] & 3, 1);
        let body: &[u8; 36] = image.memory()[0x3c801..0x3c825].try_into().unwrap();
        assert_eq!(
            CompactChannel::decode_bound(Some(body), true)
                .unwrap()
                .digital_squelch,
            0
        );
        let before = image.memory().to_vec();
        assert!(
            edit_dr_with_store(
                &mut image,
                DrField::DigitalCode,
                NumberInput::Text(b"99"),
                (0x1234, b'.'),
                &mut flags,
                |_, _| Err(invalid("synthetic DR sync cut"))
            )
            .is_err()
        );
        assert_eq!(flags & 3, 2);
        let mut expected = before;
        expected[0x3c8b6] = 99;
        assert_eq!(image.memory(), expected);
        assert!(
            edit_dr(
                &mut image,
                DrField::DigitalCode,
                NumberInput::Text(b"99"),
                0x1234,
                b'.',
                &mut flags
            )
            .unwrap()
            .synchronized
        );
        assert_eq!(flags & 3, 0);
        assert_eq!(image.memory()[0x3c80f], 99);
    }

    #[test]
    fn home_group_uses_real_stored_count_and_map_and_preserves_no_row_partial_store() {
        use crate::icf::rows::NormalMemory;
        let mut working = vec![0xa5; 0x4997a];
        working[0x3fb40..0x3fb44].copy_from_slice(&1_u32.to_le_bytes());
        working[0x3fb44..0x3fcd4].fill(0);
        for cell in working[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
            cell.copy_from_slice(&65535_u32.to_le_bytes());
        }
        working[0x3662d..0x3662d + 500].fill(1);
        working[0x36630] = 0;
        working[0x3fb44..0x3fb48].copy_from_slice(&1_u32.to_le_bytes());
        working[0x3fcd4 + 28..0x3fcd4 + 32].copy_from_slice(&3_u32.to_le_bytes());
        let normal = NormalMemory::from_working(working).unwrap();
        let catalog = HomeGroupCatalog::initial();
        let mut image = image();
        image
            .patch_memory(&[(0x392fa, &[254]), (0x392fb, &55_u16.to_le_bytes())])
            .unwrap();
        let before = image.memory().to_vec();
        assert!(
            !edit_home_group(&mut image, &normal, catalog, NumberInput::Cancel)
                .unwrap()
                .stored
        );
        assert_eq!(image.memory(), before);
        let out = edit_home_group(&mut image, &normal, catalog, NumberInput::Text(b"0")).unwrap();
        assert_eq!(
            (out.group_raw, out.channel_raw, out.notify_flags),
            (0, 7, 0x20)
        );
        let out = edit_home_group(
            &mut image,
            &normal,
            catalog,
            NumberInput::Selected(super::super::Selection {
                key: 99,
                phase: 2,
                candidate: 1,
                accepted: true,
            }),
        )
        .unwrap();
        assert_eq!(
            (out.group_raw, out.channel_raw, out.notify_flags),
            (1, 7, 0x20)
        );
        let before = image.memory().to_vec();
        assert!(
            !edit_home_group(&mut image, &normal, catalog, NumberInput::Text(b"1"))
                .unwrap()
                .stored
        );
        assert_eq!(image.memory(), before);
        let out = edit_home_group(&mut image, &normal, catalog, NumberInput::Text(b" c ")).unwrap();
        assert_eq!((out.group_raw, out.channel_raw), (100, 0));
        let out = edit_home_group(&mut image, &normal, catalog, NumberInput::Text(b"")).unwrap();
        assert_eq!((out.group_raw, out.channel_raw), (255, 65535));
        let mut expected = image.memory().to_vec();
        expected[0x392fa] = 0;
        assert!(
            edit_home_group(
                &mut image,
                &normal,
                catalog,
                NumberInput::Selected(super::super::Selection {
                    key: 1,
                    phase: 2,
                    candidate: 256,
                    accepted: true
                })
            )
            .is_err()
        );
        assert_eq!(image.memory(), expected);
    }

    #[test]
    fn home_channel_uses_real_pair_predicate_previous_split_and_call_label_resource() {
        use crate::icf::{
            context::Context,
            frequency::Ranges,
            record::{self, ChannelField},
            record_image::{self, ChannelKind},
        };
        let q = Context::before_registry();
        let ranges = Ranges::generate(&q, Some(0x411)).unwrap();
        let mut current = vec![0; 0x4997a];
        for cell in current[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
            cell.copy_from_slice(&65535_u32.to_le_bytes());
        }
        current[0x3fcd4..0x3fcd8].copy_from_slice(&0_u32.to_le_bytes());
        let mut holder = record::reset_holder(2, 0, 0x1234);
        holder.set_channel_field(ChannelField::FrequencyHz, 14_200_000);
        holder.set_split(0);
        record_image::store_channel_into(&mut current, ChannelKind::Normal, 0, &mut holder, |_| {
            Ok(())
        })
        .unwrap();
        let mut call = record::reset_holder(3, 0, 0x1234);
        call.set_channel_field(ChannelField::FrequencyHz, 144_000_000);
        call.set_split(0);
        record_image::store_channel_into(&mut current, ChannelKind::Call, 0, &mut call, |_| Ok(()))
            .unwrap();
        let runtime = HomeChannelRuntime {
            frequency: FrequencyRuntime {
                ranges: &ranges,
                live_q: &q,
                binding: 0x1234,
                crt_point: b'.',
            },
            current: &current,
            show_both_864614: 0,
        };
        let mut image = image();
        image
            .patch_memory(&[(0x392fa, &[0]), (0x392fb, &1_u16.to_le_bytes())])
            .unwrap();
        assert_eq!(
            home_channel_choices(&image, runtime, false).unwrap(),
            vec![0, 1]
        );
        assert_eq!(
            home_channel_choices(&image, runtime, true).unwrap(),
            vec![0]
        );
        assert_eq!(
            home_channel_choices(
                &image,
                HomeChannelRuntime {
                    show_both_864614: 1,
                    ..runtime
                },
                true
            )
            .unwrap(),
            Vec::<i64>::new()
        );
        let out = edit_home_channel(&mut image, runtime, NumberInput::Text(b"0x0"), 0).unwrap();
        assert!(out.stored);
        assert_eq!(out.candidate, 0);
        let before = image.memory().to_vec();
        assert!(
            !edit_home_channel(&mut image, runtime, NumberInput::Text(b"1.0"), 0)
                .unwrap()
                .stored
        );
        assert_eq!(image.memory(), before);
        assert!(
            !edit_home_channel(&mut image, runtime, NumberInput::Cancel, 8)
                .unwrap()
                .stored
        );
        assert_eq!(image.memory(), before);
        image
            .patch_memory(&[(0x392fa, &[100]), (0x392fb, &65535_u16.to_le_bytes())])
            .unwrap();
        let out = edit_home_channel(&mut image, runtime, NumberInput::Text(b"144c1"), 0).unwrap();
        assert!(out.stored);
        assert_eq!(out.candidate, 0);
        assert_eq!(out.notify_flags, 0x20);
        let before = image.memory().to_vec();
        assert!(
            !edit_home_channel(
                &mut image,
                runtime,
                NumberInput::Selected(super::super::Selection {
                    key: 1,
                    phase: 2,
                    candidate: 1,
                    accepted: true
                }),
                0
            )
            .unwrap()
            .stored
        );
        assert_eq!(image.memory(), before);
        assert!(
            edit_home_channel(&mut image, runtime, NumberInput::Begin, 0)
                .unwrap()
                .popup_requested
        );
        assert_eq!(image.memory(), before);
    }

    #[test]
    fn amplifier_native_axes_unknown_codes_gray_and_fixed_att_use_shared_raw_codecs() {
        for field in AmplifierField::ALL {
            for index in 0..17 {
                let mut image = image();
                let before = image.memory().to_vec();
                assert!(
                    !field
                        .edit(&mut image, index, ChoiceInput::Cancel)
                        .unwrap()
                        .stored
                );
                assert_eq!(image.memory(), before);
                let out = field.edit(&mut image, index, chosen(-1)).unwrap();
                if field == AmplifierField::Attenuator && index >= 13 {
                    assert_eq!(out.fixed_label, Some("---"));
                    assert!(!out.stored);
                    assert_eq!(image.memory(), before);
                    continue;
                }
                assert!(out.stored);
                let mut expected = before;
                if field == AmplifierField::Preamp {
                    expected[0x378af + index] |= 3;
                    assert_eq!(out.notify_flags, 0);
                    assert!(out.gray);
                } else {
                    expected[0x378af + index] |= 4;
                    assert_eq!(out.notify_flags, 0x20);
                }
                assert_eq!(image.memory(), expected);
                assert_eq!(
                    field
                        .edit(&mut image, index, ChoiceInput::Next)
                        .unwrap()
                        .value,
                    0
                );
                let before = image.memory().to_vec();
                assert!(field.edit(&mut image, 17, chosen(0)).is_err());
                assert!(field.edit(&mut image, usize::MAX, chosen(0)).is_err());
                assert_eq!(image.memory(), before);
            }
        }
    }

    #[test]
    fn home_dr_group_and_memory_use_shared_predicates_and_preserve_typed_list_coordinates() {
        use crate::{
            icf::{
                context::Context,
                frequency::Ranges,
                record::{self, ChannelField as C, RepeaterField as R},
            },
            repeater::working,
        };
        let q = Context::before_registry();
        let ranges = Ranges::generate(&q, Some(0x411)).unwrap();
        let mut source = vec![0; 0x4997a];
        source[0x49914..0x49916].copy_from_slice(&2_u16.to_le_bytes());
        source[0x4991a..0x4991c].copy_from_slice(&1_u16.to_le_bytes());
        for (index, hz) in [(0, 10_000), (1, 14_200_000), (2, 14_200_000)] {
            let mut holder = record::reset_holder(0x0c, index, 0x1234);
            holder.set_channel_field(C::FrequencyHz, hz);
            holder.set_channel_field(C::Duplex, 1);
            holder.set_repeater_field(R::Mode, 0);
            holder.set_repeater_field(R::Use, 1);
            working::save_into(&mut source, index as usize, &holder).unwrap();
        }
        let runtime = HomeDrRuntime {
            frequency: FrequencyRuntime {
                ranges: &ranges,
                live_q: &q,
                binding: 0x1234,
                crt_point: b'.',
            },
            source: &source,
            current: &source,
        };
        let mut image = image();
        image
            .patch_memory(&[(0x392fd, &2_u16.to_le_bytes())])
            .unwrap();
        assert_eq!(
            home_dr_group_choices(&image, runtime).unwrap(),
            vec![50, 0, 3]
        );
        assert_eq!(home_dr_memory_choices(&image, runtime).unwrap(), vec![0]);
        let before = image.memory().to_vec();
        let mut text = b" x0.0 ".to_vec();
        let out = edit_home_dr_memory(&mut image, runtime, HomeDrMemoryInput::Text(&mut text), 0)
            .unwrap();
        assert!(!out.accepted);
        assert_eq!(text, b"00");
        assert_eq!(image.memory(), before);
        let event = super::super::Selection {
            key: 0x13,
            phase: 2,
            candidate: 0,
            accepted: true,
        };
        let out = edit_home_dr_memory(&mut image, runtime, HomeDrMemoryInput::Selected(event), 8)
            .unwrap();
        assert!(out.accepted);
        assert!(!out.stored);
        assert_eq!(out.notify_flags, 0x20);
        assert_eq!(out.warning_bits, 8);
        assert_eq!(image.memory(), before);
        assert!(
            !edit_home_dr_memory(&mut image, runtime, HomeDrMemoryInput::Cancel, 0)
                .unwrap()
                .accepted
        );
        assert_eq!(image.memory(), before);
        let out = edit_home_dr_group(&mut image, runtime, NumberInput::Text(b"1")).unwrap();
        assert!(out.accepted);
        assert_eq!(out.reference, 1);
        let before = image.memory().to_vec();
        let event = super::super::Selection {
            key: 999,
            phase: 2,
            candidate: 1,
            accepted: true,
        };
        let out = edit_home_dr_group(&mut image, runtime, NumberInput::Selected(event)).unwrap();
        assert!(out.accepted);
        assert!(!out.stored);
        assert_eq!(out.notify_flags, 0x20);
        assert_eq!(image.memory(), before);
        let out = edit_home_dr_group(&mut image, runtime, NumberInput::Text(b"")).unwrap();
        assert_eq!(out.reference, 65535);
        let mut expected = before;
        expected[0x392fd..0x392ff].copy_from_slice(&65535_u16.to_le_bytes());
        assert_eq!(image.memory(), expected);
    }
}
