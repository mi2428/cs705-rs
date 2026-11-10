//! Typed JSON boundary for the public library APIs. No stored-value codecs live here.
//! Enum spellings follow public Rust variants. Coordinates retain negative zero.

use crate::{AppError, Result};
use cs705::gps::operations::Record as GpsRecord;
use cs705::icf::*;
use serde_json::{Value, json};

pub trait ValueCodec: Sized {
    fn parse(value: &Value) -> Result<Self>;
    fn json(&self) -> Value;
    fn domain() -> Value {
        json!(std::any::type_name::<Self>())
    }
}

/// Inspect a JSON type without changing the original native editor text.
/// JSON permits these four whitespace bytes, not arbitrary Unicode whitespace.
pub fn json_view(text: &str) -> &str {
    text.trim_start_matches([' ', '\t', '\r', '\n'])
}

pub fn parse<T: ValueCodec>(text: &str) -> Result<T> {
    // Bare enum/text is allowed; malformed JSON containers are not silently text.
    let value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) if !json_view(text).starts_with(['{', '[', '"']) => Value::String(text.into()),
        Err(_) => return Err(AppError::invalid("malformed JSON value")),
    };
    T::parse(&value)
}

macro_rules! integer {
    ($access:ident; $($ty:ty),+) => {$ (
        impl ValueCodec for $ty {
            fn parse(value: &Value) -> Result<Self> {
                value.$access().and_then(|n| Self::try_from(n).ok())
                    .ok_or_else(|| AppError::invalid(concat!("expected integer ", stringify!($ty))))
            }
            fn json(&self) -> Value { json!(self) }
        }
    )+};
}
integer!(as_u64; u8, u16, u32, usize);
integer!(as_i64; i8, i16, i32, i64);
impl ValueCodec for bool {
    fn parse(value: &Value) -> Result<Self> {
        value
            .as_bool()
            .ok_or_else(|| AppError::invalid("expected true or false"))
    }
    fn json(&self) -> Value {
        json!(self)
    }
    fn domain() -> Value {
        json!([false, true])
    }
}
impl ValueCodec for String {
    fn parse(value: &Value) -> Result<Self> {
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| AppError::invalid("expected text"))
    }
    fn json(&self) -> Value {
        json!(self)
    }
}
impl<T: ValueCodec> ValueCodec for Option<T> {
    fn parse(value: &Value) -> Result<Self> {
        if value.is_null() {
            Ok(None)
        } else {
            T::parse(value).map(Some)
        }
    }
    fn json(&self) -> Value {
        self.as_ref().map_or(Value::Null, ValueCodec::json)
    }
    fn domain() -> Value {
        json!({"nullable": T::domain()})
    }
}
impl<T: ValueCodec, const N: usize> ValueCodec for [T; N] {
    fn parse(value: &Value) -> Result<Self> {
        let values = value
            .as_array()
            .filter(|a| a.len() == N)
            .ok_or_else(|| AppError::invalid("expected fixed-length JSON array"))?;
        values
            .iter()
            .map(T::parse)
            .collect::<Result<Vec<_>>>()?
            .try_into()
            .map_err(|_| AppError::invalid("wrong array length"))
    }
    fn json(&self) -> Value {
        Value::Array(self.iter().map(ValueCodec::json).collect())
    }
    fn domain() -> Value {
        json!({"length": N, "element": T::domain()})
    }
}
impl ValueCodec for (bool, bool) {
    fn parse(value: &Value) -> Result<Self> {
        let [a, b] = <[bool; 2]>::parse(value)?;
        Ok((a, b))
    }
    fn json(&self) -> Value {
        json!([self.0, self.1])
    }
}

macro_rules! enums {
    ($($ty:ident {$($v:ident),+ $(,)?};)+) => {$ (
        impl ValueCodec for $ty {
            fn parse(value: &Value) -> Result<Self> {
                match value.as_str() {
                    $(Some(stringify!($v)) => Ok(Self::$v),)+
                    _ => Err(AppError::invalid(concat!("expected ",stringify!($ty), " variant; see show --list-fields"))),
                }
            }
            fn json(&self) -> Value { json!(match self { $(Self::$v => stringify!($v),)+ }) }
            fn domain() -> Value { json!([$(stringify!($v)),+]) }
        }
    )+};
}

enums! {
    MemoryPadChannels { Five, Ten };
    Language { English, Japanese };
    CoordinateDisplay { DegreesDecimalMinutes, DegreesMinutesSeconds, DecimalDegrees };
    TemperatureUnit { Celsius, Fahrenheit };
    ManualNotchWidth { Wide, Mid, Narrow };
    CwRiseTime { Ms2, Ms4, Ms6, Ms8 };
    ScopeSpan { K2p5, K5, K10, K25, K50, K100, K250, K500 };
    ScopeCenterDisplay { FilterCenter, CarrierPoint, CarrierPointAbsolute };
    ScopeMarkerPosition { FilterCenter, CarrierPoint };
    DeltaFSpan { Khz5, Khz10, Khz20, Khz50, Khz100, Khz500, Mhz1 };
    SelectMemoryScan { AllStars, OneStar, TwoStars, ThreeStars };
    BandEdgeBeep { Off, DefaultEdges, UserEdges, UserEdgesWithTransmitLimit };
    AutoPowerOff { Off, Minutes30, Minutes60, Minutes90, Minutes120 };
    PowerSave { Off, AutoShort, AutoMedium, AutoLong };
    ReceivedCallsignSpeech { Off, Kerchunk, All };
    SpeechLanguage { English, Japanese };
    SpeechAlphabet { Normal, Phonetic };
    SpeechSpeed { Slow, Fast };
    ScopeMode { Center, Fixed, ScrollCenter, ScrollFixed };
    FilterMode { Ssb, SsbData, Cw, Rtty, Am };
    FilterSlot { One, Two, Three };
    PbtSide { One, Two };
    FilterBandGroup { Hf, Mhz50To430 };
    FilterShapeSlot { Ssb, SsbData, CwNarrow, CwWide };
    FilterShape { Sharp, Soft };
    Preamp { Off, Amp1, Amp2 };
    StepAgcMode { Ssb, Cw, Rtty, Am, Fm, Wfm, Dv };
    Agc { Fast, Mid, Slow };
    ToneMode { Ssb, Am, Fm, Wfm, Dv, Cw, Rtty };
    TransmitBandwidthPreset { Wide, Mid, Narrow, Data };
    TransmitLow { Hz100, Hz200, Hz300, Hz500 };
    TransmitHigh { Hz2500, Hz2700, Hz2800, Hz2900 };
    MemorySide { Main, Split };
    MemoryCallsign { Your, Repeater1, Repeater2 };
    MemoryMode { Lsb, Usb, Cw, CwReverse, Rtty, RttyReverse, Am, Fm, Wfm, Dv };
    MemoryFilter { Filter1, Filter2, Filter3 };
    MemorySelection { Off, One, Two, Three };
    MemoryDuplex { Off, Minus, Plus };
    MemoryTone { Off, Tone, ToneSquelch, Dtcs, DtcsTransmit, ToneTransmitDtcsReceive, DtcsTransmitToneSquelchReceive, ToneTransmitToneSquelchReceive };
    MemoryDigitalSquelch { Off, Callsign, Code };
    RepeaterMode { Dv, Fm, FmNarrow, Dd };
    RepeaterDuplex { Off, Minus, Plus, Rps };
    RepeaterTone { Off, Tone, ToneSquelch };
    RepeaterPosition { None, Approximate, Exact };
}

fn member<T: ValueCodec>(value: &Value, key: &str) -> Result<T> {
    T::parse(
        value
            .get(key)
            .ok_or_else(|| AppError::invalid("missing structured value member"))?,
    )
}
macro_rules! structure {
    ($ty:ident {$($field:ident: $kind:ty),+}) => {
        impl ValueCodec for $ty {
            fn parse(value: &Value) -> Result<Self> {
                let object = value.as_object().filter(|o| o.len() == [$(stringify!($field)),+].len())
                    .ok_or_else(|| AppError::invalid("structured value has missing or unknown members"))?;
                let _ = object;
                Ok(Self {$($field: member::<$kind>(value, stringify!($field))?),+})
            }
            fn json(&self) -> Value { json!({$(stringify!($field): self.$field.json()),+}) }
            fn domain() -> Value { json!({$(stringify!($field): <$kind>::domain()),+}) }
        }
    };
}
structure!(GpsCoordinate {
    degrees: u8,
    thousandths_of_minute: u16,
    negative: bool
});
structure!(RepeaterCoordinate {
    degrees: u8,
    thousandths_of_minute: u16,
    negative: bool
});
structure!(GpsDate {
    year: u16,
    month: u8,
    day: u8
});
structure!(GpsTime {
    hour: u8,
    minute: u8,
    second: u8
});
structure!(RepeaterRecord {name: [u8;16], sub_name: [u8;8], repeater_call: [u8;8], gateway_call: [u8;8], frequency_hz: u32, offset_hz: u32, mode: RepeaterMode, duplex: RepeaterDuplex, tone: RepeaterTone, tone_index: Option<u8>, use_repeater: bool, position: RepeaterPosition, latitude: RepeaterCoordinate, longitude: RepeaterCoordinate, utc_offset_minutes: i16});
impl ValueCodec for GpsAlarm {
    fn parse(value: &Value) -> Result<Self> {
        if value.as_str() == Some("Off") {
            return Ok(Self::Off);
        }
        if value.as_str() == Some("Receive") {
            return Ok(Self::Receive);
        }
        let object = value
            .as_object()
            .filter(|o| o.len() == 1)
            .ok_or_else(|| AppError::invalid("expected GPS alarm selection"))?;
        if let Some(group) = object.get("Group") {
            return Ok(Self::Group(Option::<usize>::parse(group)?));
        }
        let memory = object
            .get("Memory")
            .filter(|m| m.as_object().is_some_and(|o| o.len() == 2))
            .ok_or_else(|| AppError::invalid("expected GPS alarm Group or Memory"))?;
        Ok(Self::Memory {
            group: member(memory, "group")?,
            index: member(memory, "index")?,
        })
    }
    fn json(&self) -> Value {
        match self {
            Self::Off => json!("Off"),
            Self::Receive => json!("Receive"),
            Self::Group(g) => json!({"Group":g}),
            Self::Memory { group, index } => json!({"Memory":{"group":group,"index":index}}),
        }
    }
    fn domain() -> Value {
        json!(["Off","Receive",{"Group":null},{"Group":1},{"Memory":{"group":1,"index":0}}])
    }
}
structure!(GpsRecord {
    valid: bool,
    bytes: [u8; 34]
});
impl ValueCodec for GpsAltitude {
    fn parse(value: &Value) -> Result<Self> {
        if value.is_null() {
            Ok(Self::Unknown)
        } else {
            i32::parse(value).map(Self::TenthsOfFoot)
        }
    }
    fn json(&self) -> Value {
        match self {
            Self::Unknown => Value::Null,
            Self::TenthsOfFoot(n) => json!(n),
        }
    }
    fn domain() -> Value {
        json!("integer tenths of a foot, or null for unknown")
    }
}
