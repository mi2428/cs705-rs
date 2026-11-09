//! Separate RTL decimal, signed32/hex, coordinate and altitude boundaries.
//! Finite binary64 operations are explicit; arbitrary x87/RTL digit ties are not
//! claimed identical to Rust's formatter. No host locale or C atof is used.

use super::{Error, trim};

/// D5560 lexical gate. No trimming, plus, exponent or suffix; dot-only is zero.
/// Safe API rejects nonfinite/overlong input instead of invoking x87 overflow.
pub fn decimal(source: &[u8], point: u8) -> Result<f64, Error> {
    if source.len() > 4096 {
        return Err(Error::Limit("decimal exceeds 4096 bytes"));
    }
    let (negative, digits) = if let Some(tail) = source.strip_prefix(b"-") {
        (true, tail)
    } else {
        (false, source)
    };
    if digits.is_empty() {
        return Err(Error::Conversion("empty decimal"));
    }
    let mut dot = false;
    for &byte in digits {
        if byte == point && !dot {
            dot = true;
        } else if !byte.is_ascii_digit() {
            return Err(Error::Conversion("decimal grammar"));
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
        .map_err(|_| Error::Conversion("decimal encoding"))?
        .parse()
        .map_err(|_| Error::Conversion("decimal value"))?;
    if !value.is_finite() {
        return Err(Error::Conversion("nonfinite decimal"));
    }
    Ok(value)
}

/// F331C/1035A4 C-prefix integer. Leading ASCII spaces and one sign; hex accepts
/// all 32 bits and a negative sign wraps. Trailing spaces and tabs are invalid.
pub fn int32(source: &[u8]) -> Result<i32, Error> {
    let source = source.split(|b| *b == 0).next().unwrap_or(b"");
    if source.len() > 4096 {
        return Err(Error::Limit("integer exceeds 4096 bytes"));
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
        return Err(Error::Conversion("empty integer"));
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
            _ => return Err(Error::Conversion("integer grammar")),
        };
        if d >= radix {
            return Err(Error::Conversion("integer digit"));
        }
        value = value
            .checked_mul(radix)
            .and_then(|v| v.checked_add(d))
            .filter(|v| *v <= maximum)
            .ok_or(Error::Conversion("integer overflow"))?;
    }
    Ok(if negative {
        0_u32.wrapping_sub(value)
    } else {
        value
    } as i32)
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
        return Err(Error::Conversion("altitude intermediate overflow"));
    }
    let n = tenths_metres as i32;
    let magnitude = (u64::from(n.unsigned_abs()) * 10000 + 1524) / 3048;
    if magnitude > i32::MAX as u64 {
        return Err(Error::Conversion("altitude result overflow"));
    }
    Ok(if n < 0 {
        -(magnitude as i32)
    } else {
        magnitude as i32
    })
}

/// B917C: all signed32 magnitudes including INT_MIN; only 7FFFFFFF is empty.
pub fn altitude_text(value: i32, point: u8) -> Vec<u8> {
    if value == i32::MAX {
        return Vec::new();
    }
    let n = (u64::from(value.unsigned_abs()) * 3048 + 5000) / 10000;
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

/// B65AC/B68B8's scale/bias then signed64 truncation and low32 projection.
/// Final band/range validation is a separate record operation. Nonfinite or
/// signed64 overflow is an explicit safe-API error, not native x87 indefinite.
pub fn frequency_hz(source: &[u8], point: u8) -> Result<u32, Error> {
    let value = (decimal(trim(source), point)? * 1_000_000.0 + 0.5).trunc();
    if !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value) {
        return Err(Error::Conversion("frequency signed64 overflow"));
    }
    Ok((value as i64) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(altitude_text(i32::MIN, b'.'), b"-65455301.6");
        assert_eq!(frequency_text(u32::MAX, b','), b"4294,967295");
        assert_eq!(frequency_hz(b"145.125000", b'.').unwrap(), 145125000);
        assert_eq!(frequency_hz(b"-1", b'.').unwrap(), 4293967297);
    }
}
