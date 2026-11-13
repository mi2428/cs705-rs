//! Native Rx compact 0 paint and shared 47CF0, not the compact 1/extra 0 CSV profile.
//! Raw source widths and decoded holder offsets belong to the record caller.
//! Symbol alone uses the independent raw-byte display contract in history.

use super::{Error, datetime, payload};

/// Explicit live global reads. None is unavailable, never English/ISO/colon.
/// Timestamp and invalid HistoryDate read the two colon tokens independently.
#[derive(Debug, Clone, Copy, Default)]
pub struct Context {
    pub date_order: Option<u32>,
    pub gui_language: Option<u32>,
    pub system_langid: Option<u16>,
    pub time_separators: Option<[u8; 2]>,
}

impl Context {
    fn language(self) -> Result<bool, Error> {
        self.gui_language
            .map(|value| value == 1)
            .ok_or(Error::SourceUnavailable(
                "native Rx GUI language C00".into(),
            ))
    }
    fn extra(self) -> Result<bool, Error> {
        // 48B54 -> 48B28: Japanese GUI skips the SYSTEM LANGID read entirely.
        if self.language()? {
            return Ok(false);
        }
        let langid = self.system_langid.ok_or(Error::SourceUnavailable(
            "native Rx SYSTEM LANGID for48B54".into(),
        ))?;
        Ok(langid & 0x3ff != 0x11)
    }
    fn separators(self) -> Result<[u8; 2], Error> {
        self.time_separators.ok_or(Error::SourceUnavailable(
            "native Rx live TimeSeparator reads".into(),
        ))
    }
}

/// 47CF0 pure display over the ENTIRE logical byte string, including NUL and
/// trailing spaces. Caller-owned 47E14 C-prefix/width handling stays outside.
/// U0 uses its fixed lead range, not nls::sanitize/CSV's wider DBCS definition.
pub fn display_text(logical: &[u8], context: &Context) -> Result<Vec<u8>, Error> {
    let mut out = logical.to_vec();
    if !context.extra()? {
        return Ok(out);
    }
    let mut at = 0;
    while at < out.len() {
        if matches!(out[at], 0x81..=0x9f | 0xe0..=0xef) {
            out[at] = b'=';
            at += 1;
            if at < out.len() {
                out[at] = b'=';
            }
        }
        at += 1;
    }
    for byte in &mut out {
        if (0xa1..=0xdf).contains(byte) {
            *byte = b'_';
        }
    }
    Ok(out)
}

/// A10A0's fixed numeric masks; neither short-date nor era/AM-PM is consulted.
pub fn date_mask(order: u32) -> &'static [u8] {
    match order {
        0 => b"yyyy/mm/dd",
        1 => b"mm/dd/yyyy",
        _ => b"dd/mm/yyyy",
    }
}

fn date_time(
    date: datetime::Date,
    time: datetime::Time,
    order: u32,
    separators: [u8; 2],
) -> Result<Vec<u8>, Error> {
    if !datetime::valid_time(time) {
        return Err(Error::Conversion("native Rx clock time".into()));
    }
    let mask = date_mask(order);
    // Only the fixed numeric date tokens and forced slash are reachable. Other
    // Context fields are inert here, not invented native locale/clock sources.
    let frame = datetime::Context {
        short_date: mask,
        date_separator: b'/',
        time_separator: 0,
        decimal_separator: 0,
        am: b"",
        pm: b"",
        current_year: date.year,
        two_digit_window: 0,
        primary_language: 0,
        eras: &[],
    };
    let mut out = datetime::format_date(date, mask, &frame, |_| {
        Err(Error::SourceUnavailable(
            "unexpected native Rx date token".into(),
        ))
    })?;
    out.push(b' ');
    out.extend(datetime::fixed_time_separators(time, separators));
    Ok(out)
}

/// Format one native paint request. `clock` supplies actual GetLocalTime/RTL
/// decoded fields ONLY on invalid HistoryDate, after the throwing offset stage.
/// No host clock, completed caption, whole row or pre-rendered mask is accepted.
/// Symbol uses raw byte IDs and does not read native language or caption tables.
pub fn format(
    request: payload::Request<'_>,
    context: &Context,
    mut clock: impl FnMut() -> Result<(datetime::Date, datetime::Time), Error>,
) -> Result<Vec<u8>, Error> {
    use payload::Adapter as A;
    match request.adapter {
        A::HistoryDate => {
            let raw = request
                .source
                .try_into()
                .map_err(|_| Error::Invalid("native Rx date requires raw6".into()))?;
            let (date, time) = datetime::decode_raw(raw);
            // A11A7 reaches EncodeTime before BOTH validity gates and all globals.
            let offset = datetime::effective_offset(request.utc_offset_minutes)?;
            let order = context.date_order.ok_or(Error::SourceUnavailable(
                "native Rx date order8648E0".into(),
            ))?;
            if datetime::valid_date(date) && datetime::valid_time(time) {
                let (date, time) = datetime::shift_minutes(date, time, offset)?;
                date_time(date, time, order, [b':'; 2])
            } else {
                let (date, time) = clock()?;
                let mut out = date_time(date, time, order, context.separators()?)?;
                for byte in &mut out {
                    if !matches!(*byte, b' ' | b'.' | b',' | b'/' | b'(' | b')' | b':' | b';') {
                        *byte = b'-';
                    }
                }
                Ok(out)
            }
        }
        A::Timestamp => {
            let raw = request
                .source
                .try_into()
                .map_err(|_| Error::Invalid("native Rx timestamp requires raw3".into()))?;
            // The shared pure primitive's empty result is precisely the invalid
            // hour/minute branch. Do not demand separator reads on that branch.
            if datetime::timestamp(raw, request.utc_offset_minutes, [0; 2]).is_empty() {
                return Ok(b"--:--:--".to_vec());
            }
            Ok(datetime::timestamp(
                raw,
                request.utc_offset_minutes,
                context.separators()?,
            ))
        }
        A::Symbol => super::symbol(request.source, false),
        A::Message | A::GpsMessage => {
            let extra = context.extra()?;
            let width = if request.adapter == A::Message {
                20
            } else {
                43
            };
            // Compact 0 preserves comma/semicolon; no B62E8 export pass follows.
            super::message(request.source, width, extra)
        }
        _ => Err(Error::Invalid(
            "request is not a native Rx paint formatter".into(),
        )),
    }
}

fn unit(current: &[u8], at: usize) -> Result<u8, Error> {
    current
        .get(at)
        .copied()
        .ok_or(Error::SourceUnavailable("native Rx live unit byte".into()))
}
fn raw_word(source: &[u8]) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(source.try_into().map_err(|_| {
        Error::Invalid("native Rx field requires raw2".into())
    })?))
}
fn raw_dword(source: &[u8]) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(source.try_into().map_err(|_| {
        Error::Invalid("native Rx field requires raw4".into())
    })?))
}
fn raw_byte(source: &[u8]) -> Result<u8, Error> {
    let [value] = source else {
        return Err(Error::Invalid("native Rx field requires raw1".into()));
    };
    Ok(*value)
}
fn suffix(mut text: Vec<u8>, unit: &[u8]) -> Vec<u8> {
    text.extend_from_slice(unit);
    text
}
fn degree(context: &Context) -> Result<u8, Error> {
    crate::csv::number::paint::degree(context.gui_language, context.system_langid)
}
fn table_text(pe: Option<&[u8]>, base: u32, index: u8, count: u8) -> Result<Vec<u8>, Error> {
    if index >= count {
        return Err(Error::SourceUnavailable(
            "native Rx table-outside pointer requires live memory source".into(),
        ));
    }
    let pe = pe.ok_or(Error::SourceUnavailable(
        "native Rx original caption asset".into(),
    ))?;
    let pointer = super::dword(super::pe_tail(pe, base + 4 * u32::from(index))?, 0)?;
    let rva = pointer
        .checked_sub(0x400000)
        .ok_or(Error::SourceUnavailable(
            "native Rx caption pointer outside asset".into(),
        ))?;
    let text = super::pe_tail(pe, rva)?;
    let end = text
        .iter()
        .take(4097)
        .position(|b| *b == 0)
        .ok_or(Error::SourceUnavailable(
            "native Rx caption bounded NUL".into(),
        ))?;
    Ok(text[..end].to_vec())
}

/// Remaining native Rx normal finite fields. Source bytes come from the record
/// caller, never a second holder codec. `resource` borrows the actual named
/// resource provider (not a client caption map); only Status reaches it.
/// Units are read from current at this call. Native decimal points are fixed '.'.
pub fn format_field(
    key: u32,
    source: &[u8],
    current: &[u8],
    context: &Context,
    source_pe: Option<&[u8]>,
    mut resource: impl FnMut(&[u8], usize) -> Result<Vec<u8>, Error>,
) -> Result<Vec<u8>, Error> {
    use crate::csv::{self, number};
    match key {
        0x15f => {
            let status = raw_byte(source)?;
            if status > 3 {
                return Err(Error::SourceUnavailable(
                    "native Rx Status outside reader0..3".into(),
                ));
            }
            resource(b"GI_CsvExp_RxHis_HeaderRptInfo", usize::from(status))
        }
        0x161 | 0x162 => Ok(if raw_byte(source)? == 0 {
            Vec::new()
        } else {
            b"*".to_vec()
        }),
        0x165 => {
            let value = raw_dword(source)? as i32;
            let feet = unit(current, 0x396b5)? == 1;
            let label = if feet { b"ft".as_slice() } else { b"m" };
            if value == i32::MAX {
                return Ok(suffix(b"------".to_vec(), label));
            }
            let value = if feet {
                value
            } else {
                number::altitude_metres_tenths(value)
            };
            let negative = value < 0;
            let magnitude = if negative {
                value.wrapping_neg()
            } else {
                value
            };
            // Native signed 32 ADD 5/IDIV 10, including -0 and the double-minus.
            Ok(suffix(
                format!(
                    "{}{}",
                    if negative { "-" } else { "" },
                    magnitude.wrapping_add(5) / 10
                )
                .into_bytes(),
                label,
            ))
        }
        0x166 => {
            let pair: [u8; 2] = source
                .try_into()
                .map_err(|_| Error::Invalid("native Rx SSID requires raw2".into()))?;
            Ok(if pair == [255; 2] {
                b"---".to_vec()
            } else {
                payload::ssid(pair)
            })
        }
        0x168 | 0x175 => {
            let value = raw_word(source)?;
            let text = if value == u16::MAX {
                b"---".to_vec()
            } else {
                value.to_string().into_bytes()
            };
            Ok(suffix(text, &[degree(context)?]))
        }
        0x169 => {
            let value = raw_word(source)?;
            let u = unit(current, 0x396b6)?;
            let label = match u {
                1 => b"mph".as_slice(),
                2 => b"kt",
                _ => b"km/h",
            };
            let text = if value == u16::MAX {
                b"---".to_vec()
            } else {
                match u {
                    1 => payload::one_decimal(
                        u32::from((u32::from(value) * 1852 / 1609) as u16),
                        b'.',
                        false,
                    ),
                    2 => payload::one_decimal(u32::from(value), b'.', false),
                    _ => payload::speed(value, b'.'),
                }
            };
            Ok(suffix(text, label))
        }
        0x16b..=0x16d => {
            let value = raw_byte(source)?;
            let (base, label, missing) = match key {
                0x16b => (0x4077b8, b"W".as_slice(), b"--".as_slice()),
                0x16d => (0x407830, b"dB".as_slice(), b"-".as_slice()),
                _ if unit(current, 0x396b5)? == 1 => (0x407808, b"ft".as_slice(), b"--".as_slice()),
                _ => (0x4077e0, b"m".as_slice(), b"--".as_slice()),
            };
            Ok(suffix(
                if value == 255 {
                    missing.to_vec()
                } else {
                    table_text(source_pe, base, value, 10)?
                },
                label,
            ))
        }
        0x16e => {
            let value = raw_byte(source)?;
            if value == 255 {
                return Ok(b"---".to_vec());
            }
            let locale = context.language()?;
            let glyph = degree(context)?; // Original reads G even when value 0.
            let mut text = table_text(source_pe, 0x407858 + u32::from(locale) * 36, value, 9)?;
            if value != 0 {
                text.extend_from_slice(&[glyph, b' ']);
                text.extend(table_text(source_pe, 0x4078a0, value, 9)?);
            }
            Ok(text)
        }
        0x16f => {
            if source.len() != 9 {
                return Err(Error::Invalid("native Rx object name requires raw9".into()));
            }
            Ok(csv::export_call(source, 9))
        }
        0x170 => {
            let value = raw_byte(source)?;
            if value == 255 {
                return Ok(Vec::new());
            }
            table_text(source_pe, 0x4078c4, value, 4)
        }
        0x171 => {
            let value = raw_word(source)? as i16;
            let celsius = unit(current, 0x396b7)? == 0;
            let text = if value == i16::MAX {
                b"---".to_vec()
            } else if celsius {
                payload::temperature(value, b'.')
            } else {
                value.to_string().into_bytes()
            };
            Ok(suffix(
                text,
                &[degree(context)?, if celsius { b'C' } else { b'F' }],
            ))
        }
        0x172..=0x174 => {
            let value = raw_word(source)?;
            let mm = unit(current, 0x396b9)? == 0;
            let text = if value == u16::MAX {
                b"---".to_vec()
            } else if mm {
                payload::rain(value, b'.')
            } else {
                format!("{}.{:02}", value / 100, value % 100).into_bytes()
            };
            Ok(suffix(text, if mm { b"mm" } else { b"in" }))
        }
        0x176 | 0x177 => {
            let value = raw_word(source)?;
            let u = unit(current, 0x396ba)?;
            let label = match u {
                0 => b"m/s".as_slice(),
                1 => b"km/h",
                3 => b"kt",
                _ => b"mph",
            };
            let text = if value == u16::MAX {
                b"---".to_vec()
            } else {
                match u {
                    0 => payload::wind(value, b'.'),
                    1 => {
                        // BA04 normal CW 1332: FILD/FMUL(binary64 1609.344)/FISTP
                        // equals this integer quotient for every nonmissing u16.
                        let t = (201168_u64 * u64::from(value) / 125) as u32;
                        payload::one_decimal(u32::from(((t + 50) / 100) as u16), b'.', false)
                    }
                    3 => payload::one_decimal(
                        u32::from(((869 * u32::from(value) + 50) / 100) as u16),
                        b'.',
                        false,
                    ),
                    _ => value.to_string().into_bytes(),
                }
            };
            Ok(suffix(text, label))
        }
        0x178 => {
            let value = raw_dword(source)?;
            let u = unit(current, 0x396b8)?;
            let label = match u {
                1 => b"mbar".as_slice(),
                2 => b"mmHg",
                3 => b"inHg",
                _ => b"hPa",
            };
            let text = if value == u32::MAX {
                b"----".to_vec()
            } else {
                match u {
                    2 => (value.wrapping_mul(7501).wrapping_add(50000) / 100000)
                        .to_string()
                        .into_bytes(),
                    3 => payload::one_decimal(
                        value.wrapping_mul(2953).wrapping_add(50000) / 100000,
                        b'.',
                        false,
                    ),
                    _ => (value.wrapping_add(5) / 10).to_string().into_bytes(),
                }
            };
            Ok(suffix(text, label))
        }
        0x179 => {
            let value = raw_byte(source)?;
            Ok(suffix(
                if matches!(value, 0 | 255) {
                    b"--".to_vec()
                } else {
                    value.to_string().into_bytes()
                },
                b"%",
            ))
        }
        _ => Err(Error::Invalid("unknown native Rx finite field key".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use payload::Adapter as A;

    fn request(adapter: A, source: &[u8], offset: i16) -> payload::Request<'_> {
        payload::Request {
            adapter,
            source,
            utc_offset_minutes: offset,
        }
    }
    fn render(adapter: A, source: &[u8], offset: i16, context: &Context) -> Result<Vec<u8>, Error> {
        format(request(adapter, source, offset), context, || {
            panic!("clock must not be reached")
        })
    }
    fn clock() -> (datetime::Date, datetime::Time) {
        (
            datetime::Date {
                year: 2026,
                month: 11,
                day: 8,
            },
            datetime::Time {
                hour: 1,
                minute: 2,
                second: 3,
                millisecond: 456,
            },
        )
    }
    fn finite_source() -> Vec<u8> {
        // Reuse only synthetic PE geometry. Labels are deliberately invented so
        // a copied native table or a computed-caption substitute cannot pass.
        let mut pe = super::super::tests::symbol_source();
        pe[0x184..0x188].copy_from_slice(&0x4077b8_u32.to_le_bytes());
        pe[0x200..].fill(0);
        let mut at = 0x400;
        for (base, count, prefix) in [
            (0x4077b8, 10, "P"),
            (0x4077e0, 10, "M"),
            (0x407808, 10, "F"),
            (0x407830, 10, "G"),
            (0x407858, 9, "D0"),
            (0x40787c, 9, "D1"),
            (0x4078a0, 9, "A"),
            (0x4078c4, 4, "T"),
        ] {
            for index in 0..count {
                let slot = 0x200 + (base - 0x4077b8) as usize + 4 * index;
                let pointer = 0x400000 + 0x4077b8 + (at - 0x200) as u32;
                pe[slot..slot + 4].copy_from_slice(&pointer.to_le_bytes());
                let text = format!("{prefix}{index}\0");
                pe[at..at + text.len()].copy_from_slice(text.as_bytes());
                at += text.len();
            }
        }
        pe
    }
    fn field(
        key: u32,
        raw: &[u8],
        current: &[u8],
        context: &Context,
        pe: Option<&[u8]>,
    ) -> Result<Vec<u8>, Error> {
        format_field(key, raw, current, context, pe, |_, _| {
            panic!("non-status field must not read a resource row")
        })
    }

    #[test]
    fn native_paint_keeps_date_clock_symbol_and_text_branches_separate() {
        let mut context = Context {
            date_order: Some(0),
            ..Context::default()
        };
        let raw = [0x31, 0x12, 0x24, 0x23, 0x30, 0x45];
        for (order, expected, ordinary) in [
            (
                0,
                b"2025/01/01 00:30:45".as_slice(),
                b"2026/11/08 01:02:03".as_slice(),
            ),
            (1, b"01/01/2025 00:30:45", b"11/08/2026 01:02:03"),
            (2, b"01/01/2025 00:30:45", b"08/11/2026 01:02:03"),
            (u32::MAX, b"01/01/2025 00:30:45", b"08/11/2026 01:02:03"),
        ] {
            context.date_order = Some(order);
            assert_eq!(
                render(A::HistoryDate, &raw, 60, &context).unwrap(),
                expected
            );
            assert_eq!(
                render(A::HistoryDate, &[8, 0x11, 0x26, 1, 2, 3], 0, &context).unwrap(),
                ordinary
            );
        }
        context.date_order = Some(0);
        assert_eq!(
            render(A::HistoryDate, &[8, 0x11, 255, 1, 2, 3], 0, &context).unwrap(),
            b"2099/11/08 01:02:03"
        );
        assert_eq!(
            render(A::HistoryDate, &[1, 1, 0, 0, 0, 0], -15361, &context).unwrap(),
            b"1999/12/31 23:59:00"
        );
        assert!(matches!(
            render(A::HistoryDate, &[255; 6], 1440, &Context::default()),
            Err(Error::Conversion(_))
        ));
        assert!(matches!(
            render(A::HistoryDate, &raw, 0, &Context::default()),
            Err(Error::SourceUnavailable(_))
        ));
        let mut calls = 0;
        for (order, separators, expected) in [
            (0, [b':'; 2], b"----/--/-- --:--:--".as_slice()),
            (1, [0, b':'], b"--/--/---- ----:--"),
            (2, *b",;", b"--/--/---- --,--;--"),
            (0, [0; 2], b"----/--/-- ------"),
        ] {
            context.date_order = Some(order);
            context.time_separators = Some(separators);
            assert_eq!(
                format(request(A::HistoryDate, &[255; 6], 0), &context, || {
                    calls += 1;
                    Ok(clock())
                })
                .unwrap(),
                expected
            );
        }
        assert_eq!(calls, 4);
        context.time_separators = None;
        assert!(
            format(request(A::HistoryDate, &[255; 6], 0), &context, || {
                calls += 1;
                Ok(clock())
            })
            .is_err()
        );
        assert_eq!(calls, 5);
        assert_eq!(
            format(request(A::HistoryDate, &[255; 6], 0), &context, || Err(
                Error::Operation("clock failure".into())
            )),
            Err(Error::Operation("clock failure".into()))
        );
        assert_eq!(
            render(A::Timestamp, &[0x24, 0, 0], i16::MIN, &Context::default()).unwrap(),
            b"--:--:--"
        );
        context.time_separators = Some([b':'; 2]);
        assert_eq!(
            render(A::Timestamp, &[0x23, 0x59, 255], 1, &context).unwrap(),
            b"00:00:00"
        );
        assert_eq!(
            render(A::Timestamp, &[0, 0, 0x30], -1, &context).unwrap(),
            b"23:59:30"
        );
        assert_eq!(
            render(A::Timestamp, &[0, 0, 0x30], 1440, &context).unwrap(),
            b"00:00:30"
        );
        context.time_separators = Some([0, b':']);
        assert_eq!(
            render(A::Timestamp, &[0x23, 0x59, 255], 1, &context).unwrap(),
            b"0000:00"
        );
        assert!(render(A::Timestamp, &[0, 0, 0], 0, &Context::default()).is_err());

        context.gui_language = Some(1);
        assert_eq!(render(A::Symbol, b"/>", 0, &context).unwrap(), b"APRS 2F3E");
        context.gui_language = Some(0);
        context.system_langid = Some(0x411);
        assert_eq!(render(A::Symbol, b"/>", 0, &context).unwrap(), b"APRS 2F3E");
        assert_eq!(render(A::Symbol, b"A>", 0, &context).unwrap(), b"APRS 413E");
        assert_eq!(
            render(A::Symbol, b",\xff", 0, &context).unwrap(),
            b"APRS 2CFF"
        );
        assert_eq!(
            render(A::Symbol, b"\xffX", 0, &Context::default()).unwrap(),
            b"No symbol"
        );
        assert!(render(A::Symbol, b"/>", 0, &Context::default()).is_ok());
        assert_eq!(
            super::super::format(
                request(A::Symbol, b"/>", 0),
                &super::super::tests::context()
            )
            .unwrap(),
            b"APRS 2F3E"
        );
        for (adapter, width) in [(A::Message, 20), (A::GpsMessage, 43)] {
            let mut source = vec![b' '; width];
            let prefix = b" a\x81\xa1\xff,;\t \0TAIL";
            source[..prefix.len()].copy_from_slice(prefix);
            let original = source.clone();
            context.gui_language = Some(1);
            context.system_langid = None;
            assert_eq!(
                render(adapter, &source, 0, &context).unwrap(),
                b" a_\xa1_,;"
            );
            context.gui_language = Some(0);
            context.system_langid = Some(0x411);
            assert_eq!(
                render(adapter, &source, 0, &context).unwrap(),
                b" a_\xa1_,;"
            );
            context.system_langid = Some(0x409);
            assert_eq!(render(adapter, &source, 0, &context).unwrap(), b" a___,;");
            context.gui_language = Some(0x101);
            assert_eq!(render(adapter, &source, 0, &context).unwrap(), b" a___,;");
            context.system_langid = None;
            assert!(render(adapter, &source, 0, &context).is_err());
            assert_eq!(source, original);
            assert_eq!(
                super::super::format(
                    request(adapter, &source, 0),
                    &super::super::tests::context()
                )
                .unwrap(),
                b" a_\xa1_"
            );
        }
        let logical = b"A\x81\0B\xa1 ";
        context.gui_language = Some(1);
        context.system_langid = None;
        assert_eq!(display_text(logical, &context).unwrap(), logical);
        context.gui_language = Some(0);
        context.system_langid = Some(0x409);
        assert_eq!(display_text(logical, &context).unwrap(), b"A==B_ ");
        assert_eq!(display_text(&logical[..2], &context).unwrap(), b"A=");
        for (input, expected) in [
            (b"\x81A".as_slice(), b"==".as_slice()),
            (b"\x9f\xa1", b"=="),
            (b"\xe0\0", b"=="),
            (b"\xef ", b"=="),
            (b"\x81", b"="),
            (b"\xf0\xa1", b"\xf0_"),
            (b"\xa1\xdf ", b"__ "),
            (b"\0\t ", b"\0\t "),
        ] {
            let out = display_text(input, &context).unwrap();
            assert_eq!(out, expected);
            assert_eq!(out.len(), input.len());
        }
        context.system_langid = Some(0x411);
        assert_eq!(display_text(logical, &context).unwrap(), logical);
        context.system_langid = None;
        assert!(display_text(logical, &context).is_err());
        let mut current = vec![0; 0x396bb];
        context.gui_language = Some(1);
        context.system_langid = None;
        let finite = finite_source();
        for value in 0..=3 {
            let out = format_field(
                0x15f,
                &[value],
                &[],
                &Context::default(),
                None,
                |name, index| {
                    assert_eq!(name, b"GI_CsvExp_RxHis_HeaderRptInfo");
                    assert_eq!(index, usize::from(value));
                    Ok(format!("resource{index}").into_bytes())
                },
            )
            .unwrap();
            assert_eq!(out, format!("resource{value}").into_bytes());
        }
        assert_eq!(
            format_field(0x15f, &[0], &[], &context, None, |_, _| Err(
                Error::Operation("resource failure".into())
            )),
            Err(Error::Operation("resource failure".into()))
        );
        for key in [0x161, 0x162] {
            assert_eq!(
                field(key, &[0], &[], &Context::default(), None).unwrap(),
                b""
            );
            assert_eq!(
                field(key, &[255], &[], &Context::default(), None).unwrap(),
                b"*"
            );
        }
        assert_eq!(
            field(0x165, &15_i32.to_le_bytes(), &current, &context, None).unwrap(),
            b"1m"
        );
        current[0x396b5] = 1;
        for (value, expected) in [
            (-4, b"-0ft".as_slice()),
            (-5, b"-1ft"),
            (i32::MIN, b"--214748364ft"),
            (i32::MAX - 1, b"-214748364ft"),
            (i32::MAX, b"------ft"),
        ] {
            assert_eq!(
                field(0x165, &value.to_le_bytes(), &current, &context, None).unwrap(),
                expected
            );
        }
        current[0x396b5] = 255;
        assert_eq!(
            field(0x165, &15_i32.to_le_bytes(), &current, &context, None).unwrap(),
            b"1m"
        );
        for (pair, expected) in [
            ([255, 255], b"---".as_slice()),
            ([b'0', 255], b"(-0)"),
            ([0, b'A'], b"\0A"),
            ([b'X', 255], b"X"),
        ] {
            assert_eq!(field(0x166, &pair, &[], &context, None).unwrap(), expected);
        }
        assert_eq!(
            field(0x168, &65534_u16.to_le_bytes(), &[], &context, None).unwrap(),
            b"65534\xdf"
        );
        assert_eq!(
            field(0x175, &u16::MAX.to_le_bytes(), &[], &context, None).unwrap(),
            b"---\xdf"
        );
        assert!(field(0x168, &[0; 2], &[], &Context::default(), None).is_err());
        let mut glyph_context = Context {
            gui_language: Some(0),
            ..Context::default()
        };
        assert!(field(0x168, &[0; 2], &[], &glyph_context, None).is_err());
        glyph_context.system_langid = Some(0x409);
        assert_eq!(
            field(0x168, &[0; 2], &[], &glyph_context, None).unwrap(),
            b"0\xb0"
        );
        for (u, expected) in [
            (0, b"1.8km/h".as_slice()),
            (1, b"1.1mph"),
            (2, b"1.0kt"),
            (255, b"1.8km/h"),
        ] {
            current[0x396b6] = u;
            assert_eq!(
                field(0x169, &10_u16.to_le_bytes(), &current, &context, None).unwrap(),
                expected
            );
        }
        current[0x396b6] = 0;
        assert_eq!(
            field(0x169, &65534_u16.to_le_bytes(), &current, &context, None).unwrap(),
            b"5583.2km/h"
        );
        assert_eq!(
            field(0x169, &u16::MAX.to_le_bytes(), &current, &context, None).unwrap(),
            b"---km/h"
        );
        for value in 0..10 {
            assert_eq!(
                field(0x16b, &[value], &[], &context, Some(&finite)).unwrap(),
                format!("P{value}W").into_bytes()
            );
            assert_eq!(
                field(0x16d, &[value], &[], &context, Some(&finite)).unwrap(),
                format!("G{value}dB").into_bytes()
            );
            current[0x396b5] = 1;
            assert_eq!(
                field(0x16c, &[value], &current, &context, Some(&finite)).unwrap(),
                format!("F{value}ft").into_bytes()
            );
            current[0x396b5] = 255;
            assert_eq!(
                field(0x16c, &[value], &current, &context, Some(&finite)).unwrap(),
                format!("M{value}m").into_bytes()
            );
        }
        for (key, expected) in [
            (0x16b, b"--W".as_slice()),
            (0x16c, b"--m"),
            (0x16d, b"-dB"),
            (0x16e, b"---"),
            (0x170, b""),
        ] {
            assert_eq!(
                field(key, &[255], &current, &Context::default(), None).unwrap(),
                expected
            );
        }
        for value in 0..9 {
            let mut expected = format!("D1{value}").into_bytes();
            if value != 0 {
                expected.extend_from_slice(&[0xdf, b' ']);
                expected.extend_from_slice(format!("A{value}").as_bytes());
            }
            assert_eq!(
                field(0x16e, &[value], &[], &context, Some(&finite)).unwrap(),
                expected
            );
        }
        context.gui_language = Some(0);
        context.system_langid = Some(0x411);
        assert_eq!(
            field(0x16e, &[1], &[], &context, Some(&finite)).unwrap(),
            b"D01\xdf A1"
        );
        context.gui_language = Some(1);
        context.system_langid = None;
        assert_eq!(
            field(0x16f, b" \x81\xa1,; \0 X", &[], &context, None).unwrap(),
            b" \x81\xa1,;"
        );
        for value in 0..4 {
            assert_eq!(
                field(0x170, &[value], &[], &context, Some(&finite)).unwrap(),
                format!("T{value}").into_bytes()
            );
        }
        assert!(matches!(
            field(0x170, &[4], &[], &context, Some(&finite)),
            Err(Error::SourceUnavailable(_))
        ));
        assert!(field(0x16b, &[10], &[], &context, Some(&finite)).is_err());
        assert_eq!(
            field(0x171, &32766_i16.to_le_bytes(), &current, &context, None).unwrap(),
            b"-1475.2\xdfC"
        );
        assert_eq!(
            field(0x171, &i16::MIN.to_le_bytes(), &current, &context, None).unwrap(),
            b" 1438.6\xdfC"
        );
        current[0x396b7] = 255;
        assert_eq!(
            field(0x171, &i16::MAX.to_le_bytes(), &current, &context, None).unwrap(),
            b"---\xdfF"
        );
        assert_eq!(
            field(0x171, &(-40_i16).to_le_bytes(), &current, &context, None).unwrap(),
            b"-40\xdfF"
        );
        for key in 0x172..=0x174 {
            current[0x396b9] = 0;
            assert_eq!(
                field(key, &65534_u16.to_le_bytes(), &current, &context, None).unwrap(),
                b"3538.4mm"
            );
            current[0x396b9] = 255;
            assert_eq!(
                field(key, &65534_u16.to_le_bytes(), &current, &context, None).unwrap(),
                b"655.34in"
            );
            assert_eq!(
                field(key, &u16::MAX.to_le_bytes(), &current, &context, None).unwrap(),
                b"---in"
            );
        }
        for key in [0x176, 0x177] {
            for (u, expected) in [
                (0, b"4.5m/s".as_slice()),
                (1, b"16.1km/h"),
                (2, b"10mph"),
                (3, b"8.7kt"),
                (255, b"10mph"),
            ] {
                current[0x396ba] = u;
                assert_eq!(
                    field(key, &10_u16.to_le_bytes(), &current, &context, None).unwrap(),
                    expected
                );
            }
            current[0x396ba] = 1;
            assert_eq!(
                field(key, &65534_u16.to_le_bytes(), &current, &context, None).unwrap(),
                b"609.1km/h"
            );
            assert_eq!(
                field(key, &u16::MAX.to_le_bytes(), &current, &context, None).unwrap(),
                b"---km/h"
            );
        }
        for (u, expected) in [
            (0, b"0hPa".as_slice()),
            (1, b"0mbar"),
            (2, b"0mmHg"),
            (3, b"0.0inHg"),
            (255, b"0hPa"),
        ] {
            current[0x396b8] = u;
            assert_eq!(
                field(
                    0x178,
                    &(u32::MAX - 1).to_le_bytes(),
                    &current,
                    &context,
                    None
                )
                .unwrap(),
                expected
            );
        }
        current[0x396b8] = 0;
        assert_eq!(
            field(0x178, &u32::MAX.to_le_bytes(), &current, &context, None).unwrap(),
            b"----hPa"
        );
        for (value, expected) in [(0, b"--%".as_slice()), (255, b"--%"), (254, b"254%")] {
            assert_eq!(
                field(0x179, &[value], &[], &Context::default(), None).unwrap(),
                expected
            );
        }
        assert!(field(0x165, &15_i32.to_le_bytes(), &[], &context, None).is_err());
        assert!(field(0x170, &[0], &[], &context, None).is_err());
        assert!(field(0x155, &[0], &[], &context, None).is_err());
        assert!(render(A::Mode, &[0], 0, &Context::default()).is_err());
        for (adapter, width) in [(A::HistoryDate, 6), (A::Timestamp, 3), (A::Symbol, 2)] {
            assert!(render(adapter, &vec![0; width - 1], 0, &context).is_err());
        }
    }
}
