//! Finite Gregorian RTL date/time grammar with explicit locale/clock inputs.
//! F5968 scans ASCII spaces and at most a four-digit nonzero magnitude, counting
//! digits in a byte. Era names/offsets are independent supplied native outputs.
//! No OS calendar, clock or locale API is called by this module.

use super::{Error, is_lead, trim};

/// Gregorian calendar date, not raw BCD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}
/// RTL time; fractional digits are an integer millisecond, not decimal seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millisecond: u16,
}
/// Runtime era enumeration entry. A None entry terminates the native name scan.
#[derive(Debug, Clone, Copy)]
pub struct Era<'a> {
    pub name: Option<&'a [u8]>,
    pub offset: i32,
}
/// Live locale/clock inputs. These are not CsvDateFormat or copied CSV bytes.
#[derive(Debug, Clone)]
pub struct Context<'a> {
    pub short_date: &'a [u8],
    pub date_separator: u8,
    pub time_separator: u8,
    pub decimal_separator: u8,
    pub am: &'a [u8],
    pub pm: &'a [u8],
    pub current_year: u16,
    pub two_digit_window: u16,
    pub primary_language: u16,
    pub eras: &'a [Era<'a>],
}

/// Leap-aware final Gregorian check (F4A44).
pub fn valid_date(date: Date) -> bool {
    let leap = date.year.is_multiple_of(4)
        && (!date.year.is_multiple_of(100) || date.year.is_multiple_of(400));
    let days = match date.month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    (1..=9999).contains(&date.year) && date.day != 0 && date.day <= days
}
pub fn valid_time(time: Time) -> bool {
    time.hour < 24 && time.minute < 60 && time.second < 60 && time.millisecond < 1000
}

fn scan(source: &[u8], at: &mut usize) -> Option<(u16, u8)> {
    while source.get(*at) == Some(&b' ') {
        *at += 1;
    }
    let mut value = 0_u16;
    let mut count = 0_u8;
    let mut any = false;
    while value < 1000 && source.get(*at).is_some_and(u8::is_ascii_digit) {
        value = value * 10 + u16::from(source[*at] - b'0');
        count = count.wrapping_add(1);
        *at += 1;
        any = true;
    }
    any.then_some((value, count))
}
fn spaces(source: &[u8], at: &mut usize) {
    while source.get(*at) == Some(&b' ') {
        *at += 1;
    }
}

/// F5C54 ASCII/native-comparison seam. `equal` represents CompareStringA(400,0)
/// over candidate era bytes, never implicit Unicode case folding.
pub fn parse_date_with(
    source: &[u8],
    context: &Context<'_>,
    mut equal: impl FnMut(&[u8], &[u8]) -> bool,
) -> Result<Date, Error> {
    let source = trim(source);
    if source.len() > 4096 || context.eras.len() > 7 {
        return Err(Error::Limit("date input or era table"));
    }
    let mut offset = 0_i32;
    let mut at = 0;
    if context.short_date.first() == Some(&b'g') {
        while at < source.len() && !source[at].is_ascii_digit() {
            at += if is_lead(source[at]) && at + 1 < source.len() {
                2
            } else {
                1
            };
        }
        let prefix = trim(&source[..at]);
        if !prefix.is_empty() {
            'era: for era in context.eras {
                let Some(name) = era.name else {
                    break;
                };
                let mut p = 0;
                while p + prefix.len() <= name.len() {
                    if equal(&name[p..p + prefix.len()], prefix) {
                        offset = era.offset;
                        break 'era;
                    }
                    p += if is_lead(name[p]) { 2 } else { 1 };
                }
            }
        }
    } else if context.short_date.contains(&b'e') {
        offset = context.eras.first().map_or(0, |e| e.offset);
    }
    let mut values = [0_u16; 3];
    let mut counts = [0_u8; 3];
    let mut n = 0;
    for i in 0..3 {
        let (value, count) = scan(source, &mut at).ok_or(Error::Conversion("date number"))?;
        values[i] = value;
        counts[i] = count;
        n += 1;
        spaces(source, &mut at);
        if source.get(at) != Some(&context.date_separator) {
            break;
        }
        at += 1;
        spaces(source, &mut at);
        if at == source.len() {
            break;
        }
    }
    spaces(source, &mut at);
    if at != source.len() || n < 2 {
        return Err(Error::Conversion("date tail"));
    }
    let order = context
        .short_date
        .iter()
        .find_map(|b| match b.to_ascii_uppercase() {
            b'D' => Some(0),
            b'M' => Some(1),
            b'Y' | b'E' => Some(2),
            _ => None,
        })
        .unwrap_or(1);
    let (mut year, month, day, year_digits) = if n == 2 {
        (
            i32::from(context.current_year),
            values[usize::from(order == 0)],
            values[usize::from(order != 0)],
            4,
        )
    } else {
        match order {
            0 => (i32::from(values[2]), values[1], values[0], counts[2]),
            1 => (i32::from(values[2]), values[0], values[1], counts[2]),
            _ => (i32::from(values[0]), values[1], values[2], counts[0]),
        }
    };
    if n == 3 {
        if offset > 0 {
            if context.primary_language == 0x12 {
                if year <= 99 {
                    year = year.wrapping_add(
                        (i32::from(context.current_year).wrapping_add(offset) / 100) * 100,
                    );
                }
                year = year.wrapping_sub(offset);
            } else {
                year = year.wrapping_add(offset).wrapping_sub(1);
            }
        } else if year_digits <= 2 {
            let pivot = i32::from(context.current_year) - i32::from(context.two_digit_window);
            year += pivot.div_euclid(100) * 100;
            if year < pivot {
                year += 100;
            }
        }
    }
    let date = Date {
        year: year as u16,
        month: u8::try_from(month).map_err(|_| Error::Conversion("date month"))?,
        day: u8::try_from(day).map_err(|_| Error::Conversion("date day"))?,
    };
    if !valid_date(date) {
        return Err(Error::Conversion("Gregorian date"));
    }
    Ok(date)
}
/// Exact-byte era comparison is an explicitly selected portable context.
pub fn parse_date(source: &[u8], context: &Context<'_>) -> Result<Date, Error> {
    parse_date_with(source, context, |a, b| a == b)
}

/// F6230 finite time grammar with explicit CompareStringA(400,1)-like matching.
pub fn parse_time_with(
    source: &[u8],
    context: &Context<'_>,
    mut equal: impl FnMut(&[u8], &[u8]) -> bool,
) -> Result<Time, Error> {
    let mut source = trim(source);
    if source.len() > 4096 {
        return Err(Error::Limit("time input"));
    }
    let mut marker = None;
    for (word, pm) in [
        (context.am, false),
        (context.pm, true),
        (b"AM".as_slice(), false),
        (b"PM".as_slice(), true),
    ] {
        if word.is_empty() {
            continue;
        }
        if source.len() >= word.len() && equal(&source[..word.len()], word) {
            marker = Some(pm);
            source = trim(&source[word.len()..]);
            break;
        }
        if source.len() >= word.len() && equal(&source[source.len() - word.len()..], word) {
            marker = Some(pm);
            source = trim(&source[..source.len() - word.len()]);
            break;
        }
    }
    let mut parts = [0_u16; 4];
    let mut at = 0;
    let mut fields = 0;
    for (i, part) in parts[..3].iter_mut().enumerate() {
        *part = scan(source, &mut at)
            .ok_or(Error::Conversion("time number"))?
            .0;
        fields += 1;
        spaces(source, &mut at);
        if i < 2 && source.get(at) == Some(&context.time_separator) {
            at += 1;
        } else {
            break;
        }
    }
    if source.get(at) == Some(&context.decimal_separator) {
        // Count parsed fields, not matching bytes: a live separator may itself
        // be a digit consumed by scan. Only a third field admits milliseconds.
        if fields != 3 {
            return Err(Error::Conversion("time fraction position"));
        }
        at += 1;
        parts[3] = scan(source, &mut at)
            .ok_or(Error::Conversion("time milliseconds"))?
            .0;
    }
    spaces(source, &mut at);
    if at != source.len() {
        return Err(Error::Conversion("time tail"));
    }
    if let Some(pm) = marker {
        if !(1..=12).contains(&parts[0]) {
            return Err(Error::Conversion("AM/PM hour"));
        }
        parts[0] = parts[0] % 12 + if pm { 12 } else { 0 };
    }
    if parts[0] >= 24 || parts[1] >= 60 || parts[2] >= 60 || parts[3] >= 1000 {
        return Err(Error::Conversion("time range"));
    }
    Ok(Time {
        hour: parts[0] as u8,
        minute: parts[1] as u8,
        second: parts[2] as u8,
        millisecond: parts[3],
    })
}
pub fn parse_time(source: &[u8], context: &Context<'_>) -> Result<Time, Error> {
    parse_time_with(source, context, |a, b| a.eq_ignore_ascii_case(b))
}

/// Gregorian date plus signed minutes, preserving seconds; offset is not a mask.
pub fn shift_minutes(mut date: Date, time: Time, minutes: i16) -> Result<(Date, Time), Error> {
    if !valid_date(date) || !valid_time(time) {
        return Err(Error::Conversion("offset source datetime"));
    }
    let total = i32::from(time.hour) * 60 + i32::from(time.minute) + i32::from(minutes);
    let mut days = total.div_euclid(1440);
    while days > 0 {
        if valid_date(Date {
            day: date.day + 1,
            ..date
        }) {
            date.day += 1;
        } else {
            date.day = 1;
            if date.month == 12 {
                date.month = 1;
                date.year += 1;
            } else {
                date.month += 1;
            }
        }
        days -= 1;
    }
    while days < 0 {
        if date.day > 1 {
            date.day -= 1;
        } else {
            if date.month == 1 {
                date.month = 12;
                date.year = date
                    .year
                    .checked_sub(1)
                    .ok_or(Error::Conversion("offset year"))?;
            } else {
                date.month -= 1;
            }
            date.day = 31;
            while date.day > 0 && !valid_date(date) {
                date.day -= 1;
            }
        }
        days += 1;
    }
    if !valid_date(date) {
        return Err(Error::Conversion("offset date range"));
    }
    let total = total.rem_euclid(1440);
    Ok((
        date,
        Time {
            hour: (total / 60) as u8,
            minute: (total % 60) as u8,
            ..time
        },
    ))
}

/// Original percent-X then decimal TryStrToInt(default99), including malformed
/// BCD nibbles; it is not the usual nibble validation operation.
pub fn decode_bcd(byte: u8) -> u8 {
    if byte >> 4 < 10 && byte & 15 < 10 {
        (byte >> 4) * 10 + (byte & 15)
    } else {
        99
    }
}
/// Pack decimal tens/units, retaining the low byte even for values above 99.
/// Calendar validation remains the caller's gate. A valid-looking output does
/// not establish valid input; wide intermediates avoid profile-specific panic.
pub fn encode_bcd(byte: u8) -> u8 {
    (u16::from(byte / 10) * 16 + u16::from(byte % 10)) as u8
}

/// Raw date order is day/month/year, followed by hour/minute/second.
pub fn decode_raw(raw: [u8; 6]) -> (Date, Time) {
    (
        Date {
            year: 2000 + u16::from(decode_bcd(raw[2])),
            month: decode_bcd(raw[1]),
            day: decode_bcd(raw[0]),
        },
        Time {
            hour: decode_bcd(raw[3]),
            minute: decode_bcd(raw[4]),
            second: decode_bcd(raw[5]),
            millisecond: 0,
        },
    )
}
/// B82B4/B8684 mutate separate scratch halves. The empty Date bug intentionally
/// writes Time=99, while missing Date (None) writes nothing.
pub fn import_raw(
    raw: &mut [u8; 6],
    date: Option<&[u8]>,
    time: Option<&[u8]>,
    context: &Context<'_>,
) {
    if let Some(date) = date {
        if trim(date).is_empty() {
            raw[3..].fill(0x99);
        } else if let Ok(d) = parse_date(date, context)
            && (2000..=2099).contains(&d.year)
        {
            raw[..3].copy_from_slice(&[
                encode_bcd(d.day),
                encode_bcd(d.month),
                encode_bcd((d.year - 2000) as u8),
            ]);
        } else {
            raw[..3].fill(0xff);
        }
    }
    if let Some(time) = time {
        if trim(time).is_empty() {
            raw[3..].fill(0x99);
        } else if let Ok(t) = parse_time(time, context) {
            raw[3..].copy_from_slice(&[
                encode_bcd(t.hour),
                encode_bcd(t.minute),
                encode_bcd(t.second),
            ]);
        } else {
            raw[3..].fill(0xff);
        }
    }
}

/// Fixed hh:nn:ss uses live separator tokens; separator zero omits the token.
pub fn fixed_time(time: Time, separator: u8) -> Vec<u8> {
    fixed_time_separators(time, [separator; 2])
}

/// 9F8A0/189018 offset ABI: abs hours are narrowed to low8 BEFORE EncodeTime.
/// This preserves accepted large signed-minute bands, instead of clamping all
/// offsets to +/-1439 or treating the stored word as a display mask.
pub fn effective_offset(minutes: i16) -> Result<i16, Error> {
    let n = i32::from(minutes).unsigned_abs();
    let hour = (n / 60) & 255;
    if hour >= 24 {
        return Err(Error::Conversion("offset EncodeTime hour"));
    }
    let effective = (hour * 60 + n % 60) as i16;
    Ok(if minutes < 0 { -effective } else { effective })
}

/// BAA10..BAE83, distinct from display's percent-X/default99 BCD conversion.
/// Missing time keeps a nonmissing BCD date and reports a correction even when
/// bytes do not change. The date bound is tested BEFORE local-to-UTC shifting.
/// An offset EncodeTime failure retains any preceding date repair.
pub fn finalize_gps_raw(raw: &mut [u8; 6], minutes: i16) -> Result<bool, Error> {
    fn bcd(bytes: &[u8]) -> bool {
        bytes.iter().all(|b| *b < 0xa0 && *b & 15 < 10)
    }
    if !bcd(&raw[3..]) {
        raw.fill(0x99);
        return Ok(true);
    }
    if raw[3..] == [0x99; 3] {
        if !bcd(&raw[..3]) {
            raw[..3].fill(0x99);
            return Ok(true);
        }
        return Ok(raw[..3] != [0x99; 3]);
    }
    let mut corrected = false;
    let has_date = if !bcd(&raw[..3]) {
        raw[..3].fill(0x99);
        corrected = true;
        false
    } else {
        raw[..3] != [0x99; 3]
    };
    let (date, time) = decode_raw(*raw);
    if !valid_time(time) {
        raw.fill(0x99);
        return Ok(true);
    }
    let offset = effective_offset(minutes)?;
    if has_date {
        if !valid_date(date) {
            raw[..3].fill(0x99);
            return Ok(true);
        }
        let key = (date.year, date.month, date.day);
        if !((2000, 1, 2)..=(2099, 12, 30)).contains(&key) {
            raw.fill(0x99);
            return Ok(true);
        }
        let (date, time) = shift_minutes(date, time, -offset)?;
        raw.copy_from_slice(&[
            encode_bcd(date.day),
            encode_bcd(date.month),
            encode_bcd((date.year % 100) as u8),
            encode_bcd(time.hour),
            encode_bcd(time.minute),
            encode_bcd(time.second),
        ]);
    } else {
        let (_, time) = shift_minutes(
            Date {
                year: 2000,
                month: 10,
                day: 10,
            },
            time,
            -offset,
        )?;
        raw[3..].copy_from_slice(&[
            encode_bcd(time.hour),
            encode_bcd(time.minute),
            encode_bcd(time.second),
        ]);
    }
    Ok(corrected)
}

/// Native date token request. Era/day/month captions depend on actual runtime
/// locale/API outputs, not on a guessed English/Japanese calendar table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateToken {
    pub date: Date,
    pub letter: u8,
    pub count: usize,
}

/// F58D4's finite date token domain. Numeric tokens and literal quoting are
/// portable; textual/era tokens call the supplied native boundary. Slash token
/// uses live DateSeparator, while a zero separator omits that token.
pub fn format_date(
    date: Date,
    format: &[u8],
    context: &Context<'_>,
    mut native: impl FnMut(DateToken) -> Result<Vec<u8>, Error>,
) -> Result<Vec<u8>, Error> {
    fn render(
        date: Date,
        format: &[u8],
        context: &Context<'_>,
        native: &mut impl FnMut(DateToken) -> Result<Vec<u8>, Error>,
        depth: usize,
    ) -> Result<Vec<u8>, Error> {
        if format.len() > 4096 || depth > 8 {
            return Err(Error::Limit("date format length or recursion"));
        }
        let mut out = Vec::new();
        let mut at = 0;
        while at < format.len() {
            let byte = format[at];
            if matches!(byte, b'\'' | b'"') {
                at += 1;
                while at < format.len() && format[at] != byte {
                    out.push(format[at]);
                    at += 1;
                }
                if at < format.len() {
                    at += 1;
                }
                continue;
            }
            if is_lead(byte) && at + 1 < format.len() {
                out.extend_from_slice(&format[at..at + 2]);
                at += 2;
                continue;
            }
            let letter = byte.to_ascii_lowercase();
            if matches!(letter, b'y' | b'm' | b'd' | b'g' | b'e') {
                let start = at;
                while at < format.len() && format[at].to_ascii_lowercase() == letter {
                    at += 1;
                }
                let count = at - start;
                let value = match (letter, count) {
                    (b'y', 1 | 2) => format!("{:02}", date.year % 100).into_bytes(),
                    (b'y', _) => format!("{:04}", date.year).into_bytes(),
                    (b'm', 1) => date.month.to_string().into_bytes(),
                    (b'm', 2) => format!("{:02}", date.month).into_bytes(),
                    (b'd', 1) => date.day.to_string().into_bytes(),
                    (b'd', 2) => format!("{:02}", date.day).into_bytes(),
                    (b'd', 5) => render(date, context.short_date, context, native, depth + 1)?,
                    _ => native(DateToken {
                        date,
                        letter,
                        count,
                    })?,
                };
                out.extend_from_slice(&value);
            } else {
                if byte != b'/' {
                    out.push(byte);
                } else if context.date_separator != 0 {
                    out.push(context.date_separator);
                }
                at += 1;
            }
            if out.len() > 65536 {
                return Err(Error::Limit("date formatted output exceeds 64 KiB"));
            }
        }
        Ok(out)
    }
    if !valid_date(date) {
        return Err(Error::Conversion("date formatter input"));
    }
    render(date, format, context, &mut native, 0)
}

/// B8428 formats a dummy date before clearing invalid-date output. On valid
/// Date, an invalid TryEncodeTime result is supplied explicitly because the
/// original caller does not gate on its bool; zero is not silently substituted.
pub fn gps_date(
    raw: [u8; 6],
    offset: i16,
    context: &Context<'_>,
    failed_time: Time,
    native: impl FnMut(DateToken) -> Result<Vec<u8>, Error>,
) -> Result<Vec<u8>, Error> {
    let (date, time) = decode_raw(raw);
    let valid = valid_date(date);
    let display = if valid {
        shift_minutes(
            date,
            if valid_time(time) { time } else { failed_time },
            effective_offset(offset)?,
        )?
        .0
    } else {
        Date {
            year: 2000,
            month: 10,
            day: 10,
        }
    };
    let formatted = format_date(display, b"ddddd", context, native)?;
    Ok(if valid {
        trim(&formatted).to_vec()
    } else {
        Vec::new()
    })
}
/// B87DC uses raw Time and a dummy date, independent of Date validity. The
/// invalid-time formatting source is an explicit input, not a host-clock fallback.
pub fn gps_time(
    raw: [u8; 6],
    offset: i16,
    context: &Context<'_>,
    invalid_format_time: Time,
) -> Result<Vec<u8>, Error> {
    let (_, time) = decode_raw(raw);
    let valid = valid_time(time);
    let time = if valid {
        shift_minutes(
            Date {
                year: 2000,
                month: 10,
                day: 10,
            },
            time,
            effective_offset(offset)?,
        )?
        .1
    } else {
        invalid_format_time
    };
    let formatted = fixed_time(time, context.time_separator);
    Ok(if valid {
        trim(&formatted).to_vec()
    } else {
        Vec::new()
    })
}

/// F5930 reads each separator token separately. Zero omits one token but does
/// not remove the surrounding numeric fields (possible lengths6/7/8).
pub fn fixed_time_separators(time: Time, separators: [u8; 2]) -> Vec<u8> {
    let fields = [time.hour, time.minute, time.second].map(|n| format!("{n:02}").into_bytes());
    let mut out = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        if i > 0 && separators[i - 1] != 0 {
            out.push(separators[i - 1]);
        }
        out.extend_from_slice(field);
    }
    out
}
/// A2274 compact timestamp: invalid seconds alone become zero; invalid offsets
/// keep the initialized offset0 after TryEncodeTime false, instead of throwing.
pub fn timestamp(raw: [u8; 3], offset: i16, separators: [u8; 2]) -> Vec<u8> {
    let hour = decode_bcd(raw[0]);
    let minute = decode_bcd(raw[1]);
    let second = decode_bcd(raw[2]);
    if hour >= 24 || minute >= 60 {
        return Vec::new();
    }
    let offset = if (-1439..=1439).contains(&offset) {
        i32::from(offset)
    } else {
        0
    };
    let minutes = (i32::from(hour) * 60 + i32::from(minute) + offset).rem_euclid(1440);
    fixed_time_separators(
        Time {
            hour: (minutes / 60) as u8,
            minute: (minutes % 60) as u8,
            second: if second < 60 { second } else { 0 },
            millisecond: 0,
        },
        separators,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn time_fraction_requires_three_scanned_fields_not_two_matching_bytes() {
        let mut c = context();
        c.time_separator = b'1';
        assert!(matches!(parse_time(b"11.1", &c), Err(Error::Conversion(_))));
        c.time_separator = b':';
        assert!(parse_time(b"1:2.1", &c).is_err());
        assert_eq!(parse_time(b"1:2:3.01", &c).unwrap().millisecond, 1);
        c.time_separator = b'.';
        assert_eq!(parse_time(b"1.2.3.01", &c).unwrap().millisecond, 1);
    }

    #[test]
    fn bcd_encoding_narrows_once_over_the_full_byte_domain() {
        for n in 0..=u8::MAX {
            let expected = (u16::from(n / 10) * 16 + u16::from(n % 10)) as u8;
            assert_eq!(encode_bcd(n), expected);
            if n < 100 {
                assert_eq!(decode_bcd(encode_bcd(n)), n);
            }
        }
    }

    #[test]
    fn baa10_missing_bcd_bounds_offset_order_and_partial_failure_match_literals() {
        for (input, offset, expected, changed) in [
            ([2, 1, 0, 0, 0x30, 0], 60, [1, 1, 0, 0x23, 0x30, 0], false),
            ([1, 1, 0, 0, 0, 0], 0, [0x99; 6], true),
            ([0x31, 0x12, 0x99, 0, 0, 0], 0, [0x99; 6], true),
            ([0xff, 0xff, 0xff, 0x99, 0x99, 0x99], 1440, [0x99; 6], true),
            (
                [0, 0, 0, 0x99, 0x99, 0x99],
                1440,
                [0, 0, 0, 0x99, 0x99, 0x99],
                true,
            ),
            (
                [0x99, 0x99, 0x99, 0, 0x30, 0],
                60,
                [0x99, 0x99, 0x99, 0x23, 0x30, 0],
                false,
            ),
            (
                [0xff, 0xff, 0xff, 0, 0x30, 0],
                60,
                [0x99, 0x99, 0x99, 0x23, 0x30, 0],
                true,
            ),
            (
                [0x31, 2, 0x26, 0, 0x30, 0],
                60,
                [0x99, 0x99, 0x99, 0, 0x30, 0],
                true,
            ),
            ([2, 1, 0, 0x24, 0, 0], 1440, [0x99; 6], true),
        ] {
            let mut raw = input;
            assert_eq!(finalize_gps_raw(&mut raw, offset).unwrap(), changed);
            assert_eq!(raw, expected);
        }
        let mut raw = [0xff, 0xff, 0xff, 0, 0x30, 0];
        assert!(finalize_gps_raw(&mut raw, 1440).is_err());
        assert_eq!(raw, [0x99, 0x99, 0x99, 0, 0x30, 0]);
    }
    fn context() -> Context<'static> {
        Context {
            short_date: b"yyyy/mm/dd",
            date_separator: b'/',
            time_separator: b':',
            decimal_separator: b'.',
            am: b"AM",
            pm: b"PM",
            current_year: 2026,
            two_digit_window: 50,
            primary_language: 0x11,
            eras: &[],
        }
    }
    #[test]
    fn literal_locale_grammar_and_empty_date_write() {
        let c = context();
        assert_eq!(
            parse_date(b"11/8", &c).unwrap(),
            Date {
                year: 2026,
                month: 11,
                day: 8
            }
        );
        assert_eq!(parse_date(b"2026/11/8/", &c).unwrap().day, 8);
        assert!(parse_date(b"2026/2/29", &c).is_err());
        assert!(parse_date(b"2026/\t11/8", &c).is_err());
        assert_eq!(
            parse_time(b"PM 1:02:03.01", &c).unwrap(),
            Time {
                hour: 13,
                minute: 2,
                second: 3,
                millisecond: 1
            }
        );
        assert_eq!(parse_time(b"1", &c).unwrap().minute, 0);
        assert!(parse_time(b"13 PM", &c).is_err());
        let mut raw = [0x08, 0x11, 0x26, 0x23, 0x30, 0x45];
        import_raw(&mut raw, Some(b""), None, &c);
        assert_eq!(raw, [0x08, 0x11, 0x26, 0x99, 0x99, 0x99]);
        import_raw(&mut raw, Some(b"invalid"), Some(b"0:1:2.1"), &c);
        assert_eq!(raw, [0xff, 0xff, 0xff, 0, 1, 2]);
    }
    #[test]
    fn era_substring_unknown_prefix_and_offset_rollover() {
        let eras = [Era {
            name: Some(b"NAME"),
            offset: 2001,
        }];
        let mut c = context();
        c.short_date = b"gg e/mm/dd";
        c.eras = &eras;
        assert_eq!(parse_date(b"AM0/1/1", &c).unwrap().year, 2000);
        assert_eq!(parse_date(b"UNKNOWN2026/11/8", &c).unwrap().year, 2026);
        let (d, t) = decode_raw([0x08, 0x11, 0x26, 0x23, 0x30, 0x45]);
        assert_eq!(
            shift_minutes(d, t, 60).unwrap(),
            (
                Date {
                    year: 2026,
                    month: 11,
                    day: 9
                },
                Time {
                    hour: 0,
                    minute: 30,
                    second: 45,
                    millisecond: 0
                }
            )
        );
        assert_eq!(fixed_time(t, 0), b"233045");
        let extreme = [Era {
            name: Some(b"NAME"),
            offset: i32::MAX,
        }];
        c.eras = &extreme;
        c.primary_language = 0x12;
        assert_eq!(parse_date(b"AM0/1/1", &c).unwrap().year, 2049);
    }

    #[test]
    fn date_export_native_failure_before_invalid_empty_and_large_offset_bands() {
        let c = context();
        let raw = [0x08, 0x11, 0x26, 0x23, 0x30, 0x45];
        let zero = Time {
            hour: 0,
            minute: 0,
            second: 0,
            millisecond: 0,
        };
        assert_eq!(
            gps_date(raw, 60, &c, zero, |_| panic!()).unwrap(),
            b"2026/11/09"
        );
        assert_eq!(gps_time(raw, 60, &c, zero).unwrap(), b"00:30:45");
        assert_eq!(effective_offset(15360).unwrap(), 0);
        assert_eq!(effective_offset(-15361).unwrap(), -1);
        assert!(effective_offset(1440).is_err());
        let mut native_context = context();
        native_context.short_date = b"gg e/mm/dd";
        let mut invalid = raw;
        invalid[0] = 0xff;
        assert!(
            gps_date(invalid, 0, &native_context, zero, |_| Err(
                Error::Operation("injected native date failure")
            ))
            .is_err()
        );
        assert_eq!(timestamp([0x23, 0x59, 0xff], 1, [b':'; 2]), b"00:00:00");
        assert_eq!(timestamp([0, 0, 0x30], -32768, [0, b':']), b"0000:30");
        assert_eq!(
            format_date(
                Date {
                    year: 2026,
                    month: 11,
                    day: 8
                },
                b"'literal' yyyy/mm/dd",
                &c,
                |_| panic!()
            )
            .unwrap(),
            b"literal 2026/11/08"
        );
    }
}
