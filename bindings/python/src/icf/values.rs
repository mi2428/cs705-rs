//! Strict value conversion shared by the direct image methods.
use crate::conversion::*;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};

pub trait Input<'a>: Sized {
    fn input(value: &'a Bound<'_, PyAny>) -> PyResult<Self>;
}
pub trait Output {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>>;
}

/// End the native access before Python conversion can invoke Python code.
pub trait Capture {
    type Owned: Output;
    fn capture(self) -> Self::Owned;
}
macro_rules! owned {
    ($($ty:ty),* $(,)?) => {$ (
        impl Capture for $ty {
            type Owned = Self;
            fn capture(self) -> Self { self }
        }
    )*};
}
owned!(
    (),
    u8,
    u16,
    u32,
    u64,
    usize,
    i8,
    i16,
    i32,
    i64,
    isize,
    bool,
    Vec<u8>,
    (bool, bool)
);
impl Capture for &[u8] {
    type Owned = Vec<u8>;
    fn capture(self) -> Self::Owned {
        self.to_vec()
    }
}
impl<const N: usize> Capture for &[u8; N] {
    type Owned = Vec<u8>;
    fn capture(self) -> Self::Owned {
        self.to_vec()
    }
}
impl<const N: usize> Capture for [u8; N] {
    type Owned = Self;
    fn capture(self) -> Self {
        self
    }
}
impl<T: Capture> Capture for Option<T> {
    type Owned = Option<T::Owned>;
    fn capture(self) -> Self::Owned {
        self.map(Capture::capture)
    }
}
macro_rules! scalar {
    ($($ty:ty => $strict:ident),* $(,)?) => {$ (
        impl Input<'_> for $ty {
            fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> { $strict(value) }
        }
        impl Output for $ty {
            fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
                Ok(self.into_pyobject(py)?.to_owned().into_any())
            }
        }
    )*};
}
scalar!(u8 => strict_u8, u16 => strict_u16, u32 => strict_u32, u64 => strict_u64,
    usize => strict_usize, i8 => strict_i8, i16 => strict_i16, i32 => strict_i32,
    i64 => strict_i64, isize => strict_isize, bool => strict_bool);
impl<'a> Input<'a> for &'a [u8] {
    fn input(value: &'a Bound<'_, PyAny>) -> PyResult<Self> {
        strict_bytes(value)
    }
}
impl Input<'_> for Vec<u8> {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(strict_bytes(value)?.to_vec())
    }
}
impl Input<'_> for [[u8; 8]; 3] {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let (a, b, c): (Bound<'_, PyAny>, Bound<'_, PyAny>, Bound<'_, PyAny>) = value.extract()?;
        Ok([fixed_bytes(&a)?, fixed_bytes(&b)?, fixed_bytes(&c)?])
    }
}
impl Output for [[u8; 8]; 3] {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(
            pyo3::types::PyTuple::new(py, self.iter().map(|value| PyBytes::new(py, value)))?
                .into_any(),
        )
    }
}
impl<'a> Input<'a> for &'a str {
    fn input(value: &'a Bound<'_, PyAny>) -> PyResult<Self> {
        value.cast::<PyString>()?.to_str()
    }
}
impl<const N: usize> Input<'_> for [u8; N] {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        fixed_bytes(value)
    }
}
impl<'a, const N: usize> Input<'a> for &'a [u8; N] {
    fn input(value: &'a Bound<'_, PyAny>) -> PyResult<Self> {
        strict_bytes(value)?.try_into().map_err(|_| {
            pyo3::exceptions::PyValueError::new_err(format!("expected exactly {N} bytes"))
        })
    }
}
impl<'a, T: Input<'a>> Input<'a> for Option<T> {
    fn input(value: &'a Bound<'_, PyAny>) -> PyResult<Self> {
        if value.is_none() {
            Ok(None)
        } else {
            T::input(value).map(Some)
        }
    }
}
impl Output for () {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(py.None().into_bound(py))
    }
}
impl Output for &[u8] {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(PyBytes::new(py, self).into_any())
    }
}
impl Output for Vec<u8> {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.as_slice().output(py)
    }
}
impl<const N: usize> Output for [u8; N] {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.as_slice().output(py)
    }
}
impl<const N: usize> Output for &[u8; N] {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.as_slice().output(py)
    }
}
impl<T: Output> Output for Option<T> {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        match self {
            Some(value) => value.output(py),
            None => ().output(py),
        }
    }
}
impl Input<'_> for (bool, bool) {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let (a, b): (Bound<'_, PyAny>, Bound<'_, PyAny>) = value.extract()?;
        Ok((strict_bool(&a)?, strict_bool(&b)?))
    }
}
impl Input<'_> for std::ops::Range<usize> {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        strict_range(value)
    }
}
impl Input<'_> for (usize, usize) {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let (a, b): (Bound<'_, PyAny>, Bound<'_, PyAny>) = value.extract()?;
        Ok((strict_usize(&a)?, strict_usize(&b)?))
    }
}
impl Output for (bool, bool) {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(self.into_pyobject(py)?.into_any())
    }
}

macro_rules! enums {
    ($($name:ident { $($variant:ident),* };)*) => {
        $(impl Capture for cs705_core::icf::$name {
            type Owned = Self;
            fn capture(self) -> Self { self }
        }
        impl Input<'_> for cs705_core::icf::$name {
            fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
                let code = super::enum_value(value, "cs705.icf", stringify!($name))?;
                $(if code == Self::$variant as i64 { return Ok(Self::$variant); })*
                Err(pyo3::exceptions::PyValueError::new_err("invalid enum value"))
            }
        }
        impl Output for cs705_core::icf::$name {
            fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
                super::enum_result(py, "cs705.icf", stringify!($name), self as i64)
            }
        })*
        pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
            $(super::add_enum(m, stringify!($name), &[$((stringify!($variant), cs705_core::icf::$name::$variant as i64)),*])?;)*
            Ok(())
        }
    };
}
enums! {
    Agc { Fast, Mid, Slow };
    AltitudeDistanceUnit { Meters, FeetMiles };
    AudioFftAttenuator { Off, Db10, Db20, Db30 };
    AudioFftWaveform { Line, Fill };
    AudioOscLevel { Db0, Minus10Db, Minus20Db, Minus30Db };
    AudioOscSweepTime { Ms1, Ms3, Ms10, Ms30, Ms100, Ms300 };
    AutoPowerOff { Off, Minutes30, Minutes60, Minutes90, Minutes120 };
    AutoTuneRxCallsignKeyFunction { AutoTuneRxCallsign, CdRxCallsign, PresetRxCallsign, HomeChannelRxCallsign, TemporarySkipRxCallsign };
    BandEdgeBeep { Off, DefaultEdges, UserEdges, UserEdgesWithTransmitLimit };
    BatteryPowerLimit { Milliwatts500, Watts1, Milliwatts2500, Watts5 };
    BluetoothAudioOutput { HeadsetOnly, HeadsetAndSpeaker };
    BluetoothAutoDisconnect { Off, Seconds0, Seconds1, Seconds2, Seconds3, Seconds4, Seconds5, Seconds6, Seconds7, Seconds8, Seconds9, Seconds10 };
    BluetoothHeadsetFunction { Normal, Microphone, Ptt };
    BluetoothSerialFunction { CivNoEcho, CivEcho, DvData };
    CoordinateDisplay { DegreesDecimalMinutes, DegreesMinutesSeconds, DecimalDegrees };
    CsvDateFormat { YearMonthDay, MonthDayYear, DayMonthYear };
    CsvSeparatorDecimal { CommaDot, SemicolonDot, SemicolonComma };
    CwBfoSide { LowerSideband, UpperSideband };
    CwBreakIn { Off, Semi, Full };
    CwKeyType { Straight, Bug, Paddle };
    CwMicKeyer { Off, UpDown, AB };
    CwNumberStyle { Normal, Digits190Ano, Digits190Ant, Digits90No, Digits90Nt };
    CwRiseTime { Ms2, Ms4, Ms6, Ms8 };
    DeltaFSpan { Khz5, Khz10, Khz20, Khz50, Khz100, Khz500, Mhz1 };
    DialAutoTs { Off, Low, High };
    DtmfSpeed { Ms100, Ms200, Ms300, Ms500 };
    ExternalPowerLimit { Milliwatts500, Watts1, Milliwatts2500, Watts5, Watts10 };
    FilterBandGroup { Hf, Mhz50To430 };
    FilterMode { Ssb, SsbData, Cw, Rtty, Am };
    FilterShape { Sharp, Soft };
    FilterShapeSlot { Ssb, SsbData, CwNarrow, CwWide };
    FilterSlot { One, Two, Three };
    KeyPolarity { Normal, Reverse };
    KeyboardInput { TenKey, Full };
    KeyboardLayout { JapanUsUk, Germany, France };
    Language { English, Japanese };
    LockFunction { MainDial, Panel };
    ManualNotchWidth { Wide, Mid, Narrow };
    MemoryCallsign { Your, Repeater1, Repeater2 };
    MemoryChannelSelectionScope { CurrentGroup, AllGroups };
    MemoryDigitalSquelch { Off, Callsign, Code };
    MemoryDuplex { Off, Minus, Plus };
    MemoryFilter { Filter1, Filter2, Filter3 };
    MemoryGroupDisplay { RegisteredOnly, AllGroups };
    MemoryMode { Lsb, Usb, Cw, CwReverse, Rtty, RttyReverse, Am, Fm, Wfm, Dv };
    MemoryPadChannels { Five, Ten };
    MemorySelection { Off, One, Two, Three };
    MemorySide { Main, Split };
    MemoryTone { Off, Tone, ToneSquelch, Dtcs, DtcsTransmit, ToneTransmitDtcsReceive, DtcsTransmitToneSquelchReceive, ToneTransmitToneSquelchReceive };
    MicUpDownSpeed { Slow, Fast };
    NotchFilter { Off, Auto, Manual };
    NotchKeyFunction { Auto, Manual, AutoManual };
    PbtSide { One, Two };
    PlaybackSkipTime { Seconds3, Seconds5, Seconds10, Seconds30 };
    PowerSave { Off, AutoShort, AutoMedium, AutoLong };
    PreRecordingTime { Off, Seconds5, Seconds10, Seconds15 };
    Preamp { Off, Amp1, Amp2 };
    PressureUnit { Hectopascals, Millibars, MillimetersMercury, InchesMercury };
    RainfallUnit { Millimeters, Inches };
    ReceivedCallsignSpeech { Off, Kerchunk, All };
    RecordingCondition { Always, SquelchLinked };
    RecordingMicrophone { HandMicrophone, BluetoothMicrophone };
    RemotePowerOffMode { ShutdownOnly, StandbyShutdown };
    RepeaterDuplex { Off, Minus, Plus, Rps };
    RepeaterMode { Dv, Fm, FmNarrow, Dd };
    RepeaterPosition { None, Approximate, Exact };
    RepeaterTone { Off, Tone, ToneSquelch };
    RttyFftAveraging { Off, Two, Three, Four };
    RttyLinefeed { CrOrLfOrCrLf, CrLf };
    RttyLogFormat { Text, Html };
    RttyLogTime { Local, Utc };
    RttyMark { Hz1275, Hz1615, Hz2125 };
    RttyShift { Hz170, Hz200, Hz425 };
    ScanMainDial { Stop, UpDown };
    ScanPauseTime { Seconds2, Seconds4, Seconds6, Seconds8, Seconds10, Seconds12, Seconds14, Seconds16, Seconds18, Seconds20, Hold };
    ScanRestartTime { Seconds0, Seconds1, Seconds2, Seconds3, Seconds4, Seconds5, Hold };
    ScanSpeed { Slow, Fast };
    ScopeAveraging { Off, Two, Three, Four };
    ScopeCenterDisplay { FilterCenter, CarrierPoint, CarrierPointAbsolute };
    ScopeMarkerPosition { FilterCenter, CarrierPoint };
    ScopeMaxHold { Off, Hold10Seconds, On };
    ScopeMode { Center, Fixed, ScrollCenter, ScrollFixed };
    ScopeSpan { K2p5, K5, K10, K25, K50, K100, K250, K500 };
    ScopeSweepSpeed { Fast, Mid, Slow };
    ScopeVbw { Narrow, Wide };
    ScopeWaterfallPeak { Grid1, Grid2, Grid3, Grid4, Grid5, Grid6, Grid7, Grid8 };
    ScopeWaterfallSize { Small, Mid, Large };
    ScopeWaterfallSpeed { Slow, Mid, Fast };
    ScopeWaveform { Fill, FillAndLine };
    ScreenCaptureFileType { Png, Bmp };
    ScreenSaverDelay { Off, Minutes1, Minutes2, Minutes5, Minutes15, Minutes30, Minutes60 };
    ScrollSpeed { Slow, Fast };
    SelectMemoryScan { AllStars, OneStar, TwoStars, ThreeStars };
    SpeechAlphabet { Normal, Phonetic };
    SpeechLanguage { English, Japanese };
    SpeechLockKey { SpeechLock, LockSpeech };
    SpeechSpeed { Slow, Fast };
    SpeedUnit { KilometersPerHour, MilesPerHour, Knots };
    StepAgcMode { Ssb, Cw, Rtty, Am, Fm, Wfm, Dv };
    TemperatureUnit { Celsius, Fahrenheit };
    TemporarySkipTime { Minutes5, Minutes10, Minutes15, WhileScanning, UntilPowerOff };
    ToneMode { Ssb, Am, Fm, Wfm, Dv, Cw, Rtty };
    TransmitBandwidth { Wide, Mid, Narrow };
    TransmitBandwidthPreset { Wide, Mid, Narrow, Data };
    TransmitHigh { Hz2500, Hz2700, Hz2800, Hz2900 };
    TransmitLow { Hz100, Hz200, Hz300, Hz500 };
    TransmitMeter { Po, Swr, Alc, Comp, Vd, Id };
    TransmitTimeout { Off, Minutes3, Minutes5, Minutes10, Minutes20, Minutes30 };
    TunerSelection { Ah705, Other };
    TxRecordingAudio { Direct, Monitor };
    VoxBreakInKeyFunction { Transmit, Tuner, VoxBreakIn, PreampAttenuator, Notch, NoiseBlanker, NoiseReduction, Split, VfoAOrB, VfoOrMemory, Cd, Preset, HomeChannel, TemporarySkip, VoiceKeyerRttyMemory1, VoiceKeyerRttyMemory2, VoiceKeyerRttyMemory3, VoiceKeyerRttyMemory4 };
    VoxVoiceDelay { Off, Short, Mid, Long };
    WindSpeedUnit { MetersPerSecond, KilometersPerHour, MilesPerHour, Knots };
}
