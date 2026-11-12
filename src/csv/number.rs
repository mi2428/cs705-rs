//! Separate RTL decimal, signed 32/hex, coordinate and altitude boundaries.
//! Finite binary64 operations are explicit; arbitrary x87/RTL digit ties are not
//! claimed identical to Rust's formatter. No host locale or C atof is used.

use super::{Error, trim};

#[path = "number/field.rs"]
pub mod field;
#[path = "number/paint.rs"]
pub mod paint;

/// D5560 lexical gate. No trimming, plus, exponent or suffix; dot-only is zero.
/// Safe API rejects nonfinite/overlong input instead of invoking x87 overflow.
pub fn decimal(source: &[u8], point: u8) -> Result<f64, Error> {
    if source.len() > 4096 {
        return Err(Error::Limit("decimal exceeds 4096 bytes".into()));
    }
    let (negative, digits) = if let Some(tail) = source.strip_prefix(b"-") {
        (true, tail)
    } else {
        (false, source)
    };
    if digits.is_empty() {
        return Err(Error::Conversion("empty decimal".into()));
    }
    let mut dot = false;
    for &byte in digits {
        if byte == point && !dot {
            dot = true;
        } else if !byte.is_ascii_digit() {
            return Err(Error::Conversion("decimal grammar".into()));
        }
    }
    if digits == [point] {
        return Ok(if negative { -0.0 } else { 0.0 });
    }
    let bytes: Vec<_> = source
        .iter()
        .map(|b| if *b == point { b'.' } else { *b })
        .collect();
    let value: f64 = std::str::from_utf8(&bytes)
        .map_err(|_| Error::Conversion("decimal encoding".into()))?
        .parse()
        .map_err(|_| Error::Conversion("decimal value".into()))?;
    if !value.is_finite() {
        return Err(Error::Conversion("nonfinite decimal".into()));
    }
    Ok(value)
}

/// F331C/1035A4 C-prefix integer. Leading ASCII spaces and one sign; hex accepts
/// all 32 bits and a negative sign wraps. Trailing spaces and tabs are invalid.
pub fn int32(source: &[u8]) -> Result<i32, Error> {
    let source = source.split(|b| *b == 0).next().unwrap_or(b"");
    if source.len() > 4096 {
        return Err(Error::Limit("integer exceeds 4096 bytes".into()));
    }
    let source = &source[source
        .iter()
        .position(|b| *b != b' ')
        .unwrap_or(source.len())..];
    let (negative, digits) = match source.first() {
        Some(b'-') => (true, &source[1..]),
        Some(b'+') => (false, &source[1..]),
        _ => (false, source),
    };
    let (radix, digits) = if let Some(s) = digits.strip_prefix(b"$") {
        (16_u32, s)
    } else if digits.starts_with(b"0x") || digits.starts_with(b"0X") {
        (16, &digits[2..])
    } else if digits.starts_with(b"x") || digits.starts_with(b"X") {
        (16, &digits[1..])
    } else {
        (10, digits)
    };
    if digits.is_empty() {
        return Err(Error::Conversion("empty integer".into()));
    }
    let maximum = if radix == 16 {
        u32::MAX
    } else if negative {
        0x8000_0000
    } else {
        0x7fff_ffff
    };
    let mut value = 0_u32;
    for &b in digits {
        let d = match b {
            b'0'..=b'9' => u32::from(b - b'0'),
            b'a'..=b'f' => u32::from(b - b'a' + 10),
            b'A'..=b'F' => u32::from(b - b'A' + 10),
            _ => return Err(Error::Conversion("integer grammar".into())),
        };
        if d >= radix {
            return Err(Error::Conversion("integer digit".into()));
        }
        value = value
            .checked_mul(radix)
            .and_then(|v| v.checked_add(d))
            .filter(|v| *v <= maximum)
            .ok_or(Error::Conversion("integer overflow".into()))?;
    }
    Ok(if negative {
        0_u32.wrapping_sub(value)
    } else {
        value
    } as i32)
}

/// A4E2C's byte editor: C-prefix, first two retained hex digits, then 2..DF.
/// Empty and wholly filtered input are accepted as 2, not an integer parse error.
pub fn civ_address(source: &[u8]) -> u8 {
    let digits: Vec<_> = source
        .iter()
        .copied()
        .take_while(|&b| b != 0)
        .filter(u8::is_ascii_hexdigit)
        .take(2)
        .collect();
    crate::icf::text::hexadecimal_prefix(&digits).clamp(2, 223) as u8
}

/// B8A18/B8D48: FF sentinel on invalid input; sign includes textual minus zero.
pub fn coordinate(source: &[u8], latitude: bool, point: u8) -> [u8; 4] {
    let text = trim(source);
    let Ok(value) = decimal(text, point) else {
        return [0xff; 4];
    };
    if value.abs() > if latitude { 90.0 } else { 180.0 } {
        return [0xff; 4];
    }
    let magnitude = value.abs() + 0.0000005_f64;
    let degree = magnitude.trunc() as u8;
    let ticks = ((magnitude - f64::from(degree)) * 60000.0).trunc() as u16;
    let negative = value < 0.0 || text.first() == Some(&b'-');
    [
        degree,
        (ticks >> 8) as u8,
        ticks as u8,
        match (latitude, negative) {
            (true, false) => b'N',
            (true, true) => b'S',
            (false, false) => b'E',
            (false, true) => b'W',
        },
    ]
}

pub(crate) fn native_substring(source: &[u8], start: usize, len: usize) -> &[u8] {
    source
        .get(start..source.len().min(start + len))
        .unwrap_or_default()
}
pub(crate) fn native_integer(text: &[u8]) -> Result<i32, Error> {
    match int32(text) {
        Ok(n) => Ok(n),
        Err(Error::Conversion(_)) => Ok(0),
        Err(e) => Err(e),
    }
}
fn native_seconds(seconds: &[u8]) -> Result<u32, Error> {
    let first = *seconds
        .first()
        .ok_or(Error::Invalid("coordinate seconds logical index1".into()))?;
    let tail = *seconds.get(1).unwrap_or(&0);
    let n = first
        .wrapping_sub(b'0')
        .wrapping_mul(10)
        .wrapping_add(tail.wrapping_sub(b'0'));
    Ok(if first == 0 {
        0
    } else {
        (1000 * u32::from(n) / 60) % 1000
    })
}

/// Native 9E714 phase2, NOT B8A18/B8D48 CSV decimal-coordinate grammar.
/// Fixed byte Substrings use 188948/StrToIntDef 0; separators are not validated.
/// Degree is stored BEFORE reading live current 396B4, so source/read failures
/// retain that prefix. U1 seconds use byte wrapping, not a floating parser.
pub fn coordinate_native_phase2(
    destination: &mut [u8; 4],
    source: &[u8],
    current: &[u8],
) -> Result<(), Error> {
    let substring = |start, len| native_substring(source, start, len);
    destination[0] = native_integer(substring(0, 3))? as u8;
    let unit = *current.get(0x396b4).ok_or(Error::SourceUnavailable(
        "native coordinate current DMSType byte unavailable".into(),
    ))?;
    let (fraction, direction) = match unit {
        1 => {
            let minutes = native_integer(substring(4, 2))? as u16;
            let f = native_seconds(substring(7, 2))?;
            ((1000 * u32::from(minutes) + f) as u16, 10)
        }
        0 => {
            let mut digits = substring(4, 2).to_vec();
            digits.extend_from_slice(substring(7, 2));
            digits.push(b'0');
            (native_integer(&digits)? as u16, 10)
        }
        _ => ((native_integer(substring(4, 4))? as u16).wrapping_mul(6), 9),
    };
    destination[1..3].copy_from_slice(&fraction.to_be_bytes());
    destination[3] = *source.get(direction).unwrap_or(&0);
    Ok(())
}

/// A0D5C reads live U; the dialog separately captures its mode at construction.
pub fn area_native_mode(current: &[u8]) -> Result<u8, Error> {
    Ok(match current.get(0x396b4) {
        Some(1) => 11,
        Some(2) => 13,
        Some(_) => 12,
        None => {
            return Err(Error::SourceUnavailable(
                "native area current DMSType byte unavailable".into(),
            ));
        }
    })
}

/// Native 9E760 for the actual producer's modes 11..13. Unlike 9E714, all
/// calculations (including the seconds index check) precede EVERY raw store.
pub fn area_native_phase2(destination: &mut [u8; 4], source: &[u8], mode: u8) -> Result<(), Error> {
    let substring = |start, len| native_substring(source, start, len);
    let fraction = match mode {
        11 => {
            let minutes = native_integer(substring(0, 2))? as u16;
            (1000 * u32::from(minutes) + native_seconds(substring(3, 2))?) as u16
        }
        12 => {
            let mut digits = substring(0, 2).to_vec();
            digits.extend_from_slice(substring(3, 2));
            digits.push(b'0');
            native_integer(&digits)? as u16
        }
        13 => (native_integer(substring(2, 4))? as u16).wrapping_mul(6),
        _ => return Err(Error::Invalid("native area mode outside11..13".into())),
    };
    destination[0] = 0;
    destination[1..3].copy_from_slice(&fraction.to_be_bytes());
    destination[3] = 0;
    Ok(())
}

fn native_dms(q: u32) -> (u32, u32) {
    let mut minute = q / 1000;
    let mut second = (q % 1000 * 60 + 500) / 1000;
    if second == 60 {
        minute += 1;
        second = 0;
        if minute > 59 {
            minute = 59;
            second = 59;
        }
    }
    (minute, second)
}
fn native_decimal_fraction(q: u32) -> u32 {
    ((q * 10 / 6 + 5) / 10).min(9999)
}

/// The nine short coordinate/area mask literals decoded by 3AA9C.
/// These are code literals, not a supplied completed cell or resource packet.
const COORDINATE_MASKS: [&[u8]; 9] = [
    b"! 99\xb099'99\">l;1;_",
    b"!999\xb099'99\">l;1;_",
    b"! 99\xb099.99'>l;1;_",
    b"!999\xb099.99'>l;1;_",
    b"! 99.9999\xb0>l;1;_",
    b"!999.9999\xb0>l;1;_",
    b"!99'99\";1;_",
    b"!99.99';1;_",
    b"!\\0.9999\xb0;1;_",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateEvent {
    Accept,
    Cancel,
    EscapeKeyPress,
    Deactivate,
    Close,
}

/// cuciFTextGps's bounded ASCII keyboard/control producer for modes 5..13.
/// Captured mode/seed remain distinct from phase2's later live current/U read.
/// Paste/IME is not silently treated as the normal OnKeyPress byte path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateDialog {
    pub mode: u8,
    pub mask: &'static [u8],
    pub seed: Vec<u8>,
    pub control: Vec<u8>,
    pub phase2: Option<Vec<u8>>,
    pub cancel: bool,
    pub done: bool,
    pub release_requested: bool,
    slots: Vec<u8>,
    caret: usize,
}

impl CoordinateDialog {
    /// A02AC edit-flag 1 uses the fixed B0 literal even when 48B28's degree
    /// glyph selector differs. No host ACP or GUI language is guessed here.
    pub fn new(
        raw: [u8; 4],
        longitude: bool,
        current: &[u8],
        first: Option<u8>,
    ) -> Result<Self, Error> {
        let unit = *current.get(0x396b4).ok_or(Error::SourceUnavailable(
            "native coordinate current DMSType byte unavailable".into(),
        ))?;
        let mode = match unit {
            1 => 5,
            2 => 9,
            _ => 7,
        } + u8::from(longitude);
        let degree = if longitude {
            format!("{:03}", raw[0])
        } else {
            format!(" {:02}", raw[0])
        };
        let q = u32::from(u16::from_be_bytes([raw[1], raw[2]]));
        let mut seed = degree.into_bytes();
        match unit {
            0 => {
                seed.push(0xb0);
                seed.extend_from_slice(format!("{:02}.{:02}'", q / 1000, q % 1000 / 10).as_bytes());
            }
            1 => {
                let (minute, second) = native_dms(q);
                seed.push(0xb0);
                seed.extend_from_slice(format!("{minute:02}'{second:02}\"").as_bytes());
            }
            2 => {
                seed.push(b'.');
                seed.extend_from_slice(format!("{:04}", native_decimal_fraction(q)).as_bytes());
                seed.push(0xb0);
            }
            _ => {
                seed.clear();
            }
        }
        if unit <= 2 {
            seed.push(raw[3]);
        }
        Ok(Self::from_seed(mode, seed, first))
    }
    /// A0D5C/9F70C edit seed, including mode 13's display-only scratch minimum.
    pub fn area(raw: [u8; 4], current: &[u8], first: Option<u8>) -> Result<Self, Error> {
        let mode = area_native_mode(current)?;
        let q = u32::from(u16::from_be_bytes([raw[1], raw[2]]));
        let seed = match mode {
            11 => {
                let (minute, second) = native_dms(q);
                format!("{minute:02}'{second:02}\"").into_bytes()
            }
            12 => format!("{:02}.{:02}'", q / 1000, q % 1000 / 10).into_bytes(),
            13 => {
                let mut text = format!("0.{:04}", native_decimal_fraction(q).max(14)).into_bytes();
                text.push(0xb0);
                text
            }
            _ => unreachable!(),
        };
        Ok(Self::from_seed(mode, seed, first))
    }
    fn first_slot(mode: u8) -> usize {
        match mode {
            5 | 7 | 9 => 1,
            0 | 13 => 2,
            _ => 0,
        }
    }
    fn from_seed(mode: u8, seed: Vec<u8>, first: Option<u8>) -> Self {
        let mask = COORDINATE_MASKS[usize::from(mode - 5)];
        Self::from_mask(mode, mask, seed, first)
    }
    /// Shared 3AA9C mask/control constructor. Each field owns its validator;
    /// this routine only supplies literal slots, initial caret and key filtering.
    pub(crate) fn from_mask(
        mode: u8,
        mask: &'static [u8],
        seed: Vec<u8>,
        first: Option<u8>,
    ) -> Self {
        let slots: Vec<_> = mask[1..]
            .iter()
            .copied()
            .take_while(|&b| b != b';')
            .filter(|&b| !matches!(b, b'>' | b'\\'))
            .collect();
        let control = slots
            .iter()
            .enumerate()
            .map(|(i, &b)| {
                if matches!(b, b'9' | b'l') {
                    *seed.get(i).unwrap_or(&b'_')
                } else {
                    b
                }
            })
            .collect();
        let mut dialog = Self {
            mode,
            mask,
            seed,
            control,
            phase2: None,
            cancel: false,
            done: false,
            release_requested: false,
            slots,
            caret: Self::first_slot(mode),
        };
        if let Some(b) = first.filter(u8::is_ascii_digit) {
            dialog.key(b);
        }
        dialog
    }
    /// Select-all replacement followed by the actual 3D558 ASCII key filter and
    /// the decoded 9/uppercase-l mask slots. Literal keys never become raw digits.
    pub fn replace(&mut self, text: &[u8]) {
        for (byte, &slot) in self.control.iter_mut().zip(&self.slots) {
            if matches!(slot, b'9' | b'l') {
                *byte = b'_';
            }
        }
        self.caret = Self::first_slot(self.mode);
        for &byte in text {
            self.key(byte);
        }
    }
    /// Full saved-literal control text, distinct from replace's key sequence.
    /// Field validators still own date ranges, coordinates and direction rules.
    pub(crate) fn replace_masked(&mut self, source: &[u8]) -> Result<(), Error> {
        if source.len() != self.control.len() {
            return Err(Error::Conversion("native mask width".into()));
        }
        for (at, &slot) in self.slots.iter().enumerate() {
            if !matches!(slot, b'9' | b'l') {
                continue;
            }
            let value = if slot == b'l' {
                source[at].to_ascii_uppercase()
            } else {
                source[at]
            };
            if !matches!(value, b' ' | b'_')
                && !(slot == b'9' && value.is_ascii_digit())
                && !(slot == b'l' && value.is_ascii_alphabetic())
            {
                return Err(Error::Conversion("native mask field character".into()));
            }
            self.control[at] = value;
        }
        Ok(())
    }
    fn key(&mut self, byte: u8) {
        let byte = byte.to_ascii_uppercase();
        let allowed_direction = match self.mode {
            5 | 7 | 9 => b"NS".contains(&byte),
            6 | 8 | 10 => b"EW".contains(&byte),
            _ => false,
        };
        if !byte.is_ascii_digit() && !allowed_direction {
            return;
        }
        while self.caret < self.slots.len() && !matches!(self.slots[self.caret], b'9' | b'l') {
            self.caret += 1;
        }
        let Some(&slot) = self.slots.get(self.caret) else {
            return;
        };
        if (slot == b'9' && byte.is_ascii_digit()) || (slot == b'l' && allowed_direction) {
            self.control[self.caret] = byte;
            self.caret += 1;
        }
    }
    /// TMaskEdit's saved literals and underscore-to-space getter, then 3C3E4.
    pub fn candidate(&self) -> Vec<u8> {
        self.control
            .iter()
            .map(|&b| if matches!(b, b'_' | b' ') { b'0' } else { b })
            .collect()
    }
    /// 3C544/3CA60 uses concatenated fixed-position integers, no lower bound.
    pub fn valid(&self) -> bool {
        let text = self.candidate();
        let sub = |at, len| native_substring(&text, at, len);
        if self.mode >= 11 {
            let (head, tail) = if self.mode == 13 {
                (sub(0, 1), sub(2, 4))
            } else {
                (sub(0, 2), sub(3, 2))
            };
            let mut numeric = head.to_vec();
            numeric.extend_from_slice(tail);
            let (low, high) = match self.mode {
                11 => (5, 5959),
                12 => (8, 5999),
                13 => (14, 9999),
                _ => return false,
            };
            return (low..=high).contains(&int32(&numeric).unwrap_or(high + 1))
                && (self.mode != 11 || int32(sub(3, 2)).unwrap_or(60) < 60);
        }
        let max = if matches!(self.mode, 5 | 7 | 9) {
            900000
        } else {
            1800000
        };
        let direction = if self.mode <= 8 { 10 } else { 9 };
        let allowed = if matches!(self.mode, 5 | 7 | 9) {
            b"NS"
        } else {
            b"EW"
        };
        if !text.get(direction).is_some_and(|b| allowed.contains(b)) {
            return false;
        }
        let mut numeric = sub(0, 3).to_vec();
        numeric.extend_from_slice(sub(4, if self.mode <= 8 { 2 } else { 4 }));
        if self.mode <= 8 {
            numeric.extend_from_slice(sub(7, 2));
        }
        if int32(&numeric).unwrap_or(max + 1) > max {
            return false;
        }
        if self.mode <= 8 {
            int32(sub(4, 2)).unwrap_or(60) < 60
                && int32(sub(7, 2)).unwrap_or(100) < if self.mode <= 6 { 60 } else { 100 }
        } else {
            int32(sub(4, 4)).unwrap_or(10000) < 10000
        }
    }
    /// A disabled normal button does not Click. OnDeactivate still calls 3B8C8,
    /// whose false-validator branch dispatches the ORIGINAL seed, not a default.
    /// Caller must run phase2/save, then call dispatched() only on normal return.
    pub fn event(&mut self, event: CoordinateEvent) -> Option<Vec<u8>> {
        let candidate = if matches!(event, CoordinateEvent::Accept | CoordinateEvent::Deactivate)
            && !self.done
            && !(event == CoordinateEvent::Deactivate && self.cancel)
            && self.valid()
        {
            Some(self.candidate())
        } else {
            None
        };
        self.event_with_candidate(event, candidate)
    }
    /// A field supplies its own accepted candidate. None means validation false,
    /// NOT cancellation: Deactivate dispatches the saved seed on that branch.
    pub(crate) fn event_with_candidate(
        &mut self,
        event: CoordinateEvent,
        candidate: Option<Vec<u8>>,
    ) -> Option<Vec<u8>> {
        match event {
            CoordinateEvent::Cancel => {
                self.cancel = true;
                self.release_requested = true;
                return None;
            }
            CoordinateEvent::EscapeKeyPress | CoordinateEvent::Close => {
                self.release_requested = true;
                return None;
            }
            CoordinateEvent::Accept if candidate.is_none() => return None,
            CoordinateEvent::Deactivate if self.cancel || self.done => {
                self.release_requested = true;
                return None;
            }
            _ => {}
        }
        if self.done {
            return None;
        }
        let text = candidate.unwrap_or_else(|| self.seed.clone());
        self.phase2 = Some(text.clone());
        Some(text)
    }
    pub fn dispatched(&mut self) {
        self.done = true;
        self.release_requested = true;
    }
}

/// Fixed six-place finite coordinate output; only the degree FF byte is empty.
pub fn coordinate_text(value: [u8; 4], latitude: bool, point: u8) -> Vec<u8> {
    if value[0] == 0xff {
        return Vec::new();
    }
    let ticks = u32::from(u16::from_be_bytes([value[1], value[2]]));
    // All stored fractions fit integer nearest-microdegree conversion; the
    // accepted non-tie tick grid has denominator 3, including noncanonical ticks.
    let millionths = u32::from(value[0]) * 1_000_000 + (ticks * 50 + 1) / 3;
    let sign = if value[3] == if latitude { b'S' } else { b'W' } {
        "-"
    } else {
        ""
    };
    let mut out = format!(
        "{sign}{}.{:06}",
        millionths / 1_000_000,
        millionths % 1_000_000
    )
    .into_bytes();
    for b in &mut out {
        if *b == b'.' {
            *b = point;
        }
    }
    out
}

/// B9078 input metres -> signed tenths-foot. Original x*10+0.5 truncates
/// asymmetrically before the second unit conversion; invalid returns sentinel.
pub fn altitude(source: &[u8], point: u8) -> Result<i32, Error> {
    let source = trim(source);
    if source.is_empty() {
        return Ok(i32::MAX);
    }
    let value = decimal(source, point)?;
    let tenths_metres = (value * 10.0 + 0.5).trunc();
    if tenths_metres < f64::from(i32::MIN) || tenths_metres > f64::from(i32::MAX) {
        return Err(Error::Conversion("altitude intermediate overflow".into()));
    }
    let n = tenths_metres as i32;
    let magnitude = (u64::from(n.unsigned_abs()) * 10000 + 1524) / 3048;
    if magnitude > i32::MAX as u64 {
        return Err(Error::Conversion("altitude result overflow".into()));
    }
    Ok(if n < 0 {
        -(magnitude as i32)
    } else {
        magnitude as i32
    })
}

/// B917C signed arithmetic only; callers own the 7FFFFFFF sentinel check.
pub(crate) fn altitude_metres_tenths(value: i32) -> i32 {
    let n = ((u64::from(value.unsigned_abs()) * 3048 + 5000) / 10000) as i32;
    if value < 0 { -n } else { n }
}

/// B917C: all signed 32 magnitudes including INT_MIN; only 7FFFFFFF is empty.
pub fn altitude_text(value: i32, point: u8) -> Vec<u8> {
    if value == i32::MAX {
        return Vec::new();
    }
    let n = altitude_metres_tenths(value).unsigned_abs();
    let mut out = format!(
        "{}{}.{}",
        if value < 0 && n != 0 { "-" } else { "" },
        n / 10,
        n % 10
    )
    .into_bytes();
    if let Some(b) = out.iter_mut().find(|b| **b == b'.') {
        *b = point;
    }
    out
}

/// Unsigned Hz -> MHz, with exact six-place output over the stored u32 domain.
pub fn frequency_text(hz: u32, point: u8) -> Vec<u8> {
    let mut out = format!("{}.{:06}", hz / 1_000_000, hz % 1_000_000).into_bytes();
    if let Some(b) = out.iter_mut().find(|b| **b == b'.') {
        *b = point;
    }
    out
}

/// B65AC: parse failure is 0; scale/spill outside the unsigned DWORD range is 0
/// BEFORE adding 0.5. The following record-level band check is separate.
pub fn frequency_hz(source: &[u8], point: u8) -> Result<u32, Error> {
    mhz_hz(source, point, 0)
}
/// B68B8 differs only in its empty/invalid parse result FFFFFFFF.
pub fn offset_hz(source: &[u8], point: u8) -> Result<u32, Error> {
    mhz_hz(source, point, u32::MAX)
}
fn mhz_hz(source: &[u8], point: u8, invalid: u32) -> Result<u32, Error> {
    let value = match decimal(trim(source), point) {
        Ok(value) => value * 1_000_000.0,
        Err(Error::Conversion(_)) => return Ok(invalid),
        Err(error) => return Err(error),
    };
    Ok(if (0.0..=f64::from(u32::MAX)).contains(&value) {
        (value + 0.5).trunc() as u32
    } else {
        0
    })
}

/// Finite CRT decimal-prefix profile used by CSV tone's already-filtered input
/// and numeric resource labels. Unlike D5560, this stops at the first foreign
/// byte and uses an independently supplied CRT point. Exponent/hex/NaN profiles
/// are explicitly outside this helper, not silently modeled as the full CRT.
pub fn crt_decimal_prefix(source: &[u8], point: u8) -> Result<f64, Error> {
    if source.len() > 4096 {
        return Err(Error::Limit("CRT finite prefix exceeds 4096 bytes".into()));
    }
    let source = source.split(|b| *b == 0).next().unwrap_or(b"");
    let mut at = 0;
    while source.get(at).is_some_and(|b| matches!(*b, 9..=13 | 32)) {
        at += 1;
    }
    let start = at;
    if matches!(source.get(at), Some(b'+' | b'-')) {
        at += 1;
    }
    let mut digits = 0;
    while source.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
        digits += 1;
    }
    if source.get(at) == Some(&point) {
        at += 1;
        while source.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
            digits += 1;
        }
    }
    if matches!(source.get(at), Some(b'e' | b'E' | b'x' | b'X')) {
        return Err(Error::SourceUnavailable(
            "CRT exponent or hex profile requires explicit conversion output".into(),
        ));
    }
    if digits == 0 {
        return Ok(0.0);
    }
    let bytes: Vec<_> = source[start..at]
        .iter()
        .map(|b| if *b == point { b'.' } else { *b })
        .collect();
    let value: f64 = std::str::from_utf8(&bytes)
        .map_err(|_| Error::Conversion("CRT prefix bytes".into()))?
        .parse()
        .map_err(|_| Error::Conversion("CRT prefix number".into()))?;
    if !value.is_finite() {
        return Err(Error::Conversion("CRT prefix nonfinite".into()));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_limits_keep_native_defaults_separate_from_safe_errors() {
        let maximum = vec![b'0'; 4096];
        assert_eq!(decimal(&maximum, b'.').unwrap(), 0.0);
        assert_eq!(int32(&maximum).unwrap(), 0);
        let overlong = vec![b'0'; 4097];
        assert!(matches!(decimal(&overlong, b'.'), Err(Error::Limit(_))));
        assert!(matches!(int32(&overlong), Err(Error::Limit(_))));
        assert!(matches!(
            frequency_hz(&overlong, b'.'),
            Err(Error::Limit(_))
        ));
        assert!(matches!(offset_hz(&overlong, b'.'), Err(Error::Limit(_))));
        let nonfinite = vec![b'9'; 309];
        assert!(matches!(
            decimal(&nonfinite, b'.'),
            Err(Error::Conversion(_))
        ));
        assert!(altitude(&nonfinite, b'.').is_err());
        for text in [b"+1".as_slice(), b"1e9999", b"1..2", b"\xff", b"1\0x"] {
            assert!(matches!(decimal(text, b'.'), Err(Error::Conversion(_))));
            assert_eq!(frequency_hz(text, b'.').unwrap(), 0);
            assert_eq!(offset_hz(text, b'.').unwrap(), u32::MAX);
        }
        assert_eq!(int32(b"2147483647\0ignored").unwrap(), i32::MAX);
        assert!(int32(b"$100000000").is_err());
        assert_eq!(
            decimal(b"-0,0", b',').unwrap().to_bits(),
            (-0.0_f64).to_bits()
        );
        assert_eq!(coordinate(b"90.000000", true, b'.'), [90, 0, 0, b'N']);
        assert_eq!(coordinate(b"90.000001", true, b'.'), [0xff; 4]);
    }

    #[test]
    fn civ_address_keeps_native_filter_empty_prefix_and_two_digit_limit() {
        for (text, value) in [
            (b"".as_slice(), 2),
            (b"xyz", 2),
            (b"foo", 15),
            (b"FF", 223),
            (b"$DF", 223),
            (b"0xDF", 13),
            (b"1234", 18),
            (b"01", 2),
            (b"7c\0FF", 124),
        ] {
            assert_eq!(civ_address(text), value);
        }
    }

    #[test]
    fn native_area_literals_seconds_wrap_and_pre_store_errors() {
        for (mode, text, expected) in [
            (11, b"12x34".as_slice(), [0, 0x31, 0x16, 0]),
            (12, b"12x34", [0, 0x30, 0x34, 0]),
            (13, b"0x1234", [0, 0x1c, 0xec, 0]),
            (11, b"12x59", [0, 0x32, 0xb7, 0]),
            (11, b"12x60", [0, 0x2e, 0xe0, 0]),
            (11, b"12x3", [0, 0x32, 0xa6, 0]),
            (11, b"12x/9", [0, 0x2f, 0xda, 0]),
            (11, b"12x\0a", [0, 0x2e, 0xe0, 0]),
            (11, b"-1x00", [0, 0xfc, 0x18, 0]),
            (12, b"xxxyy", [0; 4]),
            (12, b"", [0; 4]),
            (13, b"", [0; 4]),
            (13, b"0x0001", [0, 0, 6, 0]),
            (13, b"0x$FFF", [0, 0x5f, 0xfa, 0]),
            (13, b"\0x1234", [0, 0x1c, 0xec, 0]),
        ] {
            let mut raw = [9, 8, 7, 6];
            area_native_phase2(&mut raw, text, mode).unwrap();
            assert_eq!(raw, expected, "mode{mode}: {text:?}");
        }
        for text in [b"".as_slice(), b"12", b"12x"] {
            let mut temporary = [9, 8, 7, 6];
            let mut word = 0xabcd;
            let result = (|| {
                area_native_phase2(&mut temporary, text, 11)?;
                word = u16::from_be_bytes([temporary[1], temporary[2]]);
                Ok::<_, Error>(())
            })();
            assert!(matches!(result, Err(Error::Invalid(_))));
            assert_eq!(temporary, [9, 8, 7, 6]);
            assert_eq!(word, 0xabcd);
        }
        let mut raw = [9, 8, 7, 6];
        assert!(area_native_phase2(&mut raw, b"0x1234", 14).is_err());
        assert_eq!(raw, [9, 8, 7, 6]);
    }

    #[test]
    fn native_area_modes_masks_validator_and_display_not_input_minimum() {
        let mut current = vec![0; 0x396b5];
        for (unit, mode, keys, candidate, expected) in [
            (
                1,
                11,
                b"12 34nE".as_slice(),
                b"12'34\"".as_slice(),
                [0, 0x31, 0x16, 0],
            ),
            (0, 12, b"12.34wS", b"12.34'", [0, 0x30, 0x34, 0]),
            (2, 13, b"1234nW", b"0.1234\xb0", [0, 0x1c, 0xec, 0]),
            (255, 12, b"12.34", b"12.34'", [0, 0x30, 0x34, 0]),
        ] {
            current[0x396b4] = unit;
            assert_eq!(area_native_mode(&current).unwrap(), mode);
            let mut d = CoordinateDialog::area([9, 0x30, 0x34, 8], &current, None).unwrap();
            assert_eq!(d.mode, mode);
            let (mask, seed) = match mode {
                11 => (b"!99'99\";1;_".as_slice(), b"12'20\"".as_slice()),
                12 => (b"!99.99';1;_".as_slice(), b"12.34'".as_slice()),
                13 => (b"!\\0.9999\xb0;1;_".as_slice(), b"0.2057\xb0".as_slice()),
                _ => unreachable!(),
            };
            assert_eq!(d.mask, mask);
            assert_eq!(d.seed, seed);
            d.replace(keys);
            assert_eq!(d.candidate(), candidate);
            assert!(d.valid());
            let text = d.event(CoordinateEvent::Accept).unwrap();
            let mut raw = [9; 4];
            area_native_phase2(&mut raw, &text, mode).unwrap();
            assert_eq!(raw, expected);
            d.dispatched();
            assert!(d.done && d.release_requested);
        }
        for (unit, cases) in [
            (
                1,
                [
                    (b"0004".as_slice(), false),
                    (b"0005", true),
                    (b"5959", true),
                    (b"5960", false),
                ],
            ),
            (
                0,
                [
                    (b"0007".as_slice(), false),
                    (b"0008", true),
                    (b"5999", true),
                    (b"6000", false),
                ],
            ),
            (
                2,
                [
                    (b"0013".as_slice(), false),
                    (b"0014", true),
                    (b"9999", true),
                    (b"", false),
                ],
            ),
        ] {
            current[0x396b4] = unit;
            let mut d = CoordinateDialog::area([0; 4], &current, None).unwrap();
            for (keys, valid) in cases {
                d.replace(keys);
                assert_eq!(d.valid(), valid, "unit{unit}: {keys:?}");
            }
        }
        current[0x396b4] = 2;
        let raw = [0; 4];
        let d = CoordinateDialog::area(raw, &current, Some(b'2')).unwrap();
        assert_eq!(d.seed, b"0.0014\xb0");
        assert_eq!(d.control, b"0.2014\xb0");
        assert_eq!(raw, [0; 4]);
        let mut parsed = [9; 4];
        area_native_phase2(&mut parsed, b"0.0000\xb0", 13).unwrap();
        assert_eq!(parsed, [0; 4]);
        current[0x396b4] = 255;
        assert_eq!(area_native_mode(&current).unwrap(), 12);
        assert_eq!(d.mode, 13);
        assert!(area_native_mode(&current[..0x396b4]).is_err());
        assert!(CoordinateDialog::area(raw, &current[..0x396b4], None).is_err());
    }

    #[test]
    fn native_area_cancel_same_value_deactivate_and_late_error() {
        let mut current = vec![0; 0x396b5];
        current[0x396b4] = 1;
        let raw = [0, 0x2e, 0xe0, 0];
        let mut cancelled = CoordinateDialog::area(raw, &current, None).unwrap();
        cancelled.replace(b"1234");
        assert!(cancelled.event(CoordinateEvent::Cancel).is_none());
        assert!(cancelled.event(CoordinateEvent::Deactivate).is_none());
        assert!(cancelled.cancel && cancelled.phase2.is_none() && !cancelled.done);
        let mut same = CoordinateDialog::area(raw, &current, None).unwrap();
        assert_eq!(same.seed, b"12'00\"");
        let text = same.event(CoordinateEvent::Accept).unwrap();
        let mut saved = raw;
        area_native_phase2(&mut saved, &text, 11).unwrap();
        assert_eq!(saved, raw);
        assert_eq!(u16::from_be_bytes([saved[1], saved[2]]), 12_000);
        same.dispatched();
        assert!(same.done);
        assert!(same.event(CoordinateEvent::Deactivate).is_none());
        let mut d = CoordinateDialog::area(raw, &current, None).unwrap();
        d.replace(b"0060");
        assert!(!d.valid() && d.event(CoordinateEvent::Accept).is_none());
        assert_eq!(d.event(CoordinateEvent::Deactivate), Some(d.seed.clone()));
        d.replace(b"1234");
        assert!(d.event(CoordinateEvent::EscapeKeyPress).is_none());
        assert!(!d.cancel);
        assert!(d.event(CoordinateEvent::Deactivate).is_some());
        let mut late = CoordinateDialog::area(raw, &current, None).unwrap();
        late.replace(b"1234");
        let text = late.event(CoordinateEvent::Accept).unwrap();
        let mut temporary = [9; 4];
        let mut word = 0xabcd;
        let result = (|| {
            let wrapper_return = (|| {
                area_native_phase2(&mut temporary, &text, area_native_mode(&current)?)?;
                Err::<(), _>(Error::Operation("later display failed".into()))
            })();
            wrapper_return?;
            word = u16::from_be_bytes([temporary[1], temporary[2]]);
            Ok::<_, Error>(())
        })();
        assert!(result.is_err());
        assert_eq!(temporary, [0, 0x31, 0x16, 0]);
        assert_eq!(word, 0xabcd);
        assert_eq!(late.phase2, Some(text));
        assert!(!late.done && !late.release_requested);
        area_native_phase2(&mut temporary, &late.seed, 11).unwrap();
        word = u16::from_be_bytes([temporary[1], temporary[2]]);
        assert_eq!(word, 12_000);
    }

    #[test]
    fn native_coordinate_mask_validator_and_finite_modal_producer() {
        let mut current = vec![0; 0x396b5];
        for (unit, source, expected) in [
            (0, b"35 12.34 s".as_slice(), [35, 0x30, 0x34, b'S']),
            (0, b"-35+12.34s", [35, 0x30, 0x34, b'S']),
            (1, b"35 12 34 s", [35, 0x31, 0x16, b'S']),
            (2, b"35.1234s", [35, 0x1c, 0xec, b'S']),
        ] {
            current[0x396b4] = unit;
            let mut d =
                CoordinateDialog::new([35, 0x30, 0x34, b'N'], false, &current, None).unwrap();
            assert_eq!(d.mask, COORDINATE_MASKS[usize::from(d.mode - 5)]);
            d.replace(source);
            assert!(d.valid());
            let text = d.event(CoordinateEvent::Accept).unwrap();
            assert!(!d.done && !d.release_requested);
            let mut raw = [9; 4];
            coordinate_native_phase2(&mut raw, &text, &current).unwrap();
            assert_eq!(raw, expected);
            d.dispatched();
            assert!(d.done && d.release_requested);
            assert!(d.event(CoordinateEvent::Accept).is_none());
        }
        current[0x396b4] = 1;
        let d = CoordinateDialog::new([0, 0, 0, b'N'], false, &current, Some(b'1')).unwrap();
        assert_eq!(&d.control[..3], b" 10");
        assert!(d.phase2.is_none());
        let mut d = CoordinateDialog::new([35, 0x30, 0x34, b'N'], false, &current, None).unwrap();
        assert_eq!(d.seed, b" 35\xb012'20\"N");
        for source in [b"900001N".as_slice(), b"351260N", b"351234W", b"", b"-++Q"] {
            d.replace(source);
            assert!(!d.valid());
            assert!(d.event(CoordinateEvent::Accept).is_none());
        }
        assert_eq!(
            d.event(CoordinateEvent::Deactivate),
            Some(b" 35\xb012'20\"N".to_vec())
        );
        assert!(!d.done);
        d.dispatched();
        let mut d = CoordinateDialog::new([255, 255, 255, 0], true, &current, None).unwrap();
        d.replace(b"1800000w");
        assert!(d.valid());
        d.event(CoordinateEvent::Cancel);
        assert!(d.cancel && d.release_requested);
        assert!(d.event(CoordinateEvent::Deactivate).is_none());
        let mut d = CoordinateDialog::new([0, 0, 0, b'E'], true, &current, None).unwrap();
        d.replace(b"1391234e");
        d.event(CoordinateEvent::EscapeKeyPress);
        assert!(!d.cancel);
        assert!(d.event(CoordinateEvent::Deactivate).is_some());
        let mut closed = CoordinateDialog::new([0, 0, 0, b'N'], false, &current, None).unwrap();
        assert!(closed.event(CoordinateEvent::Close).is_none());
        assert!(!closed.cancel && closed.release_requested);
        // Mode and seed are captured at dialog open, but phase2 re-reads live U.
        current[0x396b4] = 0;
        let mut raw = [9; 4];
        coordinate_native_phase2(&mut raw, d.phase2.as_ref().unwrap(), &current).unwrap();
        assert_eq!(raw, [139, 0x30, 0x34, b'E']);
        current[0x396b4] = 255;
        let mut d = CoordinateDialog::new([35, 0, 0, b'N'], false, &current, None).unwrap();
        assert_eq!(d.mode, 7);
        assert!(d.seed.is_empty());
        d.replace(b"351234N");
        let text = d.event(CoordinateEvent::Accept).unwrap();
        coordinate_native_phase2(&mut raw, &text, &current).unwrap();
        assert_eq!(raw, [35, 0, 0, b'\'']);
        assert!(CoordinateDialog::new([0; 4], false, &current[..0x396b4], None).is_err());
    }

    #[test]
    fn native_phase2_literals_units_short_tail_nul_and_ordered_errors() {
        let mut current = vec![0; 0x396b5];
        for (unit, source, expected) in [
            (0, b"035x12y34zS".as_slice(), [35, 0x30, 0x34, b'S']),
            (1, b"035x12y34zS", [35, 0x31, 0x16, b'S']),
            (2, b"139x1234zW", [139, 0x1c, 0xec, b'W']),
            (255, b"139x1234zw", [139, 0x1c, 0xec, b'w']),
            (1, b"035x12y3", [35, 0x32, 0xa6, 0]),
            (1, b"035x12y\0azS", [35, 0x2e, 0xe0, b'S']),
            (1, b"035x12y/9zS", [35, 0x2f, 0xda, b'S']),
            (0, b"-01x12y34zs", [255, 0x30, 0x34, b's']),
            (2, b"$FFx$FFFz\0TAIL", [255, 0x5f, 0xfa, 0]),
            (2, b"1\0Xx1234zE", [1, 0x1c, 0xec, b'E']),
            (0, b"badx12y34zS", [0, 0x30, 0x34, b'S']),
            (0, b"", [0, 0, 0, 0]),
        ] {
            current[0x396b4] = unit;
            let mut raw = [9, 8, 7, 6];
            coordinate_native_phase2(&mut raw, source, &current).unwrap();
            assert_eq!(raw, expected);
        }
        let mut raw = [9, 8, 7, 6];
        assert!(matches!(
            coordinate_native_phase2(&mut raw, b"123x45y67zS", &current[..0x396b4]),
            Err(Error::SourceUnavailable(_))
        ));
        assert_eq!(raw, [123, 8, 7, 6]);
        current[0x396b4] = 1;
        for source in [b"123x45".as_slice(), b""] {
            let mut raw = [9, 8, 7, 6];
            assert!(matches!(
                coordinate_native_phase2(&mut raw, source, &current),
                Err(Error::Invalid(_))
            ));
            assert_eq!(raw, [if source.is_empty() { 0 } else { 123 }, 8, 7, 6]);
        }
    }

    #[test]
    fn decimal_integer_and_low32_are_distinct() {
        for (text, expected) in [
            (b"-2147483648".as_slice(), i32::MIN),
            (b"2147483647", i32::MAX),
            (b"$FFFFFFFF", -1),
            (b"-$FFFFFFFF", 1),
            (b" +0X80000000", i32::MIN),
            (b"12\0tail", 12),
        ] {
            assert_eq!(int32(text).unwrap(), expected);
        }
        for text in [
            b"2147483648".as_slice(),
            b"-2147483649",
            b"1 ",
            b"\t1",
            b"",
            b"$100000000",
        ] {
            assert!(int32(text).is_err());
        }
        assert_eq!(
            decimal(b"-.", b'.').unwrap().to_bits(),
            (-0.0_f64).to_bits()
        );
        assert_eq!(decimal(b"1,25", b',').unwrap(), 1.25);
        for text in [b" 1".as_slice(), b"+1", b"1e2", b"1..2", b"-", b"NaN"] {
            assert!(decimal(text, b'.').is_err());
        }
    }
    #[test]
    fn coordinate_bias_negative_zero_and_altitude_literals() {
        assert_eq!(coordinate(b"1.00001", true, b'.'), [1, 0, 0, b'N']);
        assert_eq!(coordinate(b" -0 ", true, b'.'), [0, 0, 0, b'S']);
        assert_eq!(coordinate(b"180.000001", false, b'.'), [0xff; 4]);
        assert_eq!(coordinate_text([12, 0, 1, b'S'], true, b','), b"-12,000017");
        assert_eq!(coordinate_text([0xff, 0, 0, b'N'], true, b'.'), b"");
        assert_eq!(altitude(b"1.0", b'.').unwrap(), 33);
        assert_eq!(altitude(b"-1.0", b'.').unwrap(), -30);
        for (value, expected) in [
            (i32::MIN, -654_553_016),
            (i32::MAX, 654_553_016),
            (-2, -1),
            (-1, 0),
            (0, 0),
            (1, 0),
            (2, 1),
        ] {
            assert_eq!(altitude_metres_tenths(value), expected);
        }
        assert_eq!(altitude_text(i32::MIN, b'.'), b"-65455301.6");
        assert_eq!(altitude_text(-1, b','), b"0,0");
        assert_eq!(altitude_text(-2, b','), b"-0,1");
        assert!(altitude_text(i32::MAX, b'.').is_empty());
        assert_eq!(frequency_text(u32::MAX, b','), b"4294,967295");
        assert_eq!(frequency_hz(b"145.125000", b'.').unwrap(), 145125000);
        assert_eq!(frequency_hz(b"-1", b'.').unwrap(), 0);
        assert_eq!(offset_hz(b"bad", b'.').unwrap(), u32::MAX);
        assert_eq!(offset_hz(b"4295", b'.').unwrap(), 0);
    }
}
