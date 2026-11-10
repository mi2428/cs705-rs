//! Rx 92AFC's individual compact 1/extra 0 formatter requests, not whole CSV rows.
//! Offset is captured by the caller before holder validity checks. Resource
//! labels share table::portable_label; Gregorian/BCD/time logic stays in datetime.
//! Symbol display is intentionally independent of original captions: raw APRS
//! bytes become a lossless ASCII identifier, not a native-caption fallback.

use super::{Error, datetime, payload, table};

#[path = "history/paint.rs"]
pub mod paint;

/// Normal portable profile. Textual calendar/era tokens need actual native
/// outputs; they are not reconstructed from import-era names or host locale.
pub fn format(
    request: payload::Request<'_>,
    context: &datetime::Context<'_>,
) -> Result<Vec<u8>, Error> {
    format_with_date(request, context, |_| {
        Err(Error::SourceUnavailable("Rx native calendar token output"))
    })
}

/// Native date tokens use datetime's existing seam, never completed cells/rows.
pub fn format_with_date(
    request: payload::Request<'_>,
    context: &datetime::Context<'_>,
    native: impl FnMut(datetime::DateToken) -> Result<Vec<u8>, Error>,
) -> Result<Vec<u8>, Error> {
    use payload::Adapter as A;
    match request.adapter {
        A::RxType | A::RxStatus | A::Directivity | A::DataType => {
            table::portable_label(payload::Kind::ReceiveHistory, &request, b'.')?
                .ok_or(Error::SourceUnavailable("Rx static label adapter"))
        }
        A::Symbol => symbol(request.source, true),
        A::HistoryDate => {
            let raw = request
                .source
                .try_into()
                .map_err(|_| Error::Invalid("Rx date requires raw6"))?;
            let (date, time) = datetime::decode_raw(raw);
            // A11A7 calls the throwing offset encoder BEFORE both valid gates.
            let offset = datetime::effective_offset(request.utc_offset_minutes)?;
            if !datetime::valid_date(date) || !datetime::valid_time(time) {
                return Ok(Vec::new());
            }
            let (date, time) = datetime::shift_minutes(date, time, offset)?;
            // Compact 1 uses ddddd hh:nn:ss, not the GUI's CsvDateFormat order.
            let mut out = datetime::format_date(date, b"ddddd", context, native)?;
            out.push(b' ');
            out.extend(datetime::fixed_time(time, context.time_separator));
            Ok(out)
        }
        A::Timestamp => {
            let raw = request
                .source
                .try_into()
                .map_err(|_| Error::Invalid("Rx timestamp requires raw3"))?;
            Ok(datetime::timestamp(
                raw,
                request.utc_offset_minutes,
                [context.time_separator; 2],
            ))
        }
        A::Message | A::GpsMessage => {
            let width = if request.adapter == A::Message {
                20
            } else {
                43
            };
            // 47E14 C-prefix -> TrimRight -> 479EC('_'). Extra 0 skips 478C8;
            // half-width kana survive. The compact 1 B62E8 pass happens last.
            let text = message(request.source, width, false)?;
            Ok(super::export_text(&text, width))
        }
        _ => Err(Error::Invalid("request is not an Rx history formatter")),
    }
}

// 479EC is byte-wise, unlike nls::sanitize's valid-DBCS-pair grammar. Keep this
// existing Rx primitive shared; 478C8 is a separate, later half-width-kana pass.
fn message(source: &[u8], width: usize, extra: bool) -> Result<Vec<u8>, Error> {
    if source.len() != width {
        return Err(Error::Invalid("Rx message requires exact raw20/raw43"));
    }
    let mut text = super::export_call(source, width);
    for byte in &mut text {
        if !matches!(*byte, 0x20..=0x7e | 0xa1..=0xdf) {
            *byte = b'_';
        }
    }
    if extra {
        for byte in &mut text {
            if (0xa1..=0xdf).contains(byte) {
                *byte = b'_';
            }
        }
    }
    Ok(text)
}

fn span(source: &[u8], at: usize, len: usize) -> Result<&[u8], Error> {
    let end = at
        .checked_add(len)
        .ok_or(Error::Invalid("PE source offset overflow"))?;
    source
        .get(at..end)
        .ok_or(Error::SourceUnavailable("truncated PE source asset"))
}
fn word(source: &[u8], at: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(span(source, at, 2)?.try_into().unwrap()))
}
fn dword(source: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(span(source, at, 4)?.try_into().unwrap()))
}

// CS-705's PE32/x86/ImageBase schema only. Each lookup is bounded to at most 96
// section headers and the actual file-backed tail; virtual/BSS bytes are not 0.
pub(crate) fn pe_tail(source: &[u8], rva: u32) -> Result<&[u8], Error> {
    if span(source, 0, 2)? != b"MZ" {
        return Err(Error::Invalid("symbol source is not PE"));
    }
    let pe = dword(source, 0x3c)? as usize;
    let header = span(source, pe, 24)?;
    if &header[..4] != b"PE\0\0" || word(header, 4)? != 0x14c {
        return Err(Error::Invalid("symbol source requires x86 PE"));
    }
    let count = usize::from(word(header, 6)?);
    let optional_size = usize::from(word(header, 20)?);
    if !(1..=96).contains(&count) || optional_size < 96 {
        return Err(Error::Invalid("symbol PE header schema"));
    }
    let optional = span(source, pe + 24, optional_size)?;
    if word(optional, 0)? != 0x10b || dword(optional, 28)? != 0x400000 {
        return Err(Error::Invalid(
            "symbol source requires PE32 ImageBase400000",
        ));
    }
    let sections = span(source, pe + 24 + optional_size, count * 40)?;
    let mut found = None;
    for section in sections.as_chunks::<40>().0 {
        let virtual_at = dword(section, 12)?;
        let raw_size = dword(section, 16)?;
        if let Some(delta) = rva.checked_sub(virtual_at)
            && delta < raw_size
        {
            if found.is_some() {
                return Err(Error::Invalid("overlapping PE symbol sections"));
            }
            let at = (dword(section, 20)? as usize)
                .checked_add(delta as usize)
                .ok_or(Error::Invalid("PE symbol file offset overflow"))?;
            let tail = source.get(at..).ok_or(Error::SourceUnavailable(
                "PE symbol RVA has no source bytes",
            ))?;
            found = Some(&tail[..tail.len().min((raw_size - delta) as usize)]);
        }
    }
    found.ok_or(Error::SourceUnavailable("PE symbol RVA is not file-backed"))
}

/// Independent display contract, not the original caption table or a fallback.
/// Both bytes survive even for unknown pairs. Only first=FF denotes missing;
/// fixed uppercase hex cannot introduce CSV/JSON delimiters or embedded NUL.
pub fn symbol(raw: &[u8], compact: bool) -> Result<Vec<u8>, Error> {
    let [first, second]: [u8; 2] = raw
        .try_into()
        .map_err(|_| Error::Invalid("Rx symbol requires raw2"))?;
    Ok(if first == 255 {
        if compact {
            Vec::new()
        } else {
            b"No symbol".to_vec()
        }
    } else {
        format!("APRS {first:02X}{second:02X}").into_bytes()
    })
}

/// Explicit source-caption oracle, isolated from normal export and paint.
/// The caller supplies source bytes; this function never finds or executes an
/// application. Its native table semantics do not define normal Symbol display.
pub fn symbol_from_source(
    raw: &[u8],
    source: &[u8],
    locale: bool,
    compact: bool,
) -> Result<Vec<u8>, Error> {
    let [first, second]: [u8; 2] = raw
        .try_into()
        .map_err(|_| Error::Invalid("Rx symbol requires raw2"))?;
    if first == 255 {
        return Ok(if compact { Vec::new() } else { b"---".to_vec() });
    }
    let finish = |text: &[u8], width| {
        if compact {
            super::export_text(text, width)
        } else {
            super::export_call(text, width)
        }
    };
    let direct = || {
        finish(
            &[first, second].map(|b| if (32..=126).contains(&b) { b } else { b' ' }),
            2,
        )
    };
    if !(33..=128).contains(&second) || !matches!(first,b'/'|b'\\'|b'0'..=b'9'|b'A'..=b'Z') {
        return Ok(direct());
    }
    let special = [first, second] == *b"IY";
    // C88B8 forces locale 0 for compact 1. Native paint uses 48B44's canonical bit.
    let slot = if special {
        0x419518
    } else {
        (if first == b'/' { 0x418f18 } else { 0x419218 }) + 8 * u32::from(second - 33)
    } + u32::from(locale && !compact) * 4;
    let pointer = dword(pe_tail(source, slot)?, 0)?;
    let rva = pointer
        .checked_sub(0x400000)
        .ok_or(Error::SourceUnavailable(
            "Rx symbol caption pointer outside source image",
        ))?;
    let tail = pe_tail(source, rva)?;
    let end = tail
        .iter()
        .take(39)
        .position(|b| *b == 0)
        .ok_or(Error::SourceUnavailable(
            "Rx symbol caption has no bounded NUL",
        ))?;
    if end == 0 && !special {
        return Ok(direct());
    }
    let mut out = tail[..end].to_vec();
    if !special && first != b'/' && first != b'\\' {
        out.extend_from_slice(&[b' ', b'(', first, b')']);
    }
    if out.len() > 38 {
        return Err(Error::SourceUnavailable(
            "Rx symbol exceeds native38-byte buffer",
        ));
    }
    Ok(finish(&out, 38))
}

#[cfg(test)]
mod tests {
    use super::*;
    use payload::Adapter as A;

    #[test]
    fn symbol_raw_bytes_are_lossless_and_delimiter_safe_without_source() {
        assert_eq!(
            format(request(A::Symbol, b"/>", 0), &context()).unwrap(),
            b"APRS 2F3E"
        );
        for first in 0..=u8::MAX {
            for second in 0..=u8::MAX {
                let raw = [first, second];
                let csv = format(request(A::Symbol, &raw, 0), &context()).unwrap();
                let painted = paint::format(
                    request(A::Symbol, &raw, 0),
                    &paint::Context::default(),
                    || panic!("symbol must not read the clock"),
                )
                .unwrap();
                if first == 255 {
                    assert!(csv.is_empty());
                    assert_eq!(painted, b"No symbol");
                } else {
                    assert_eq!(csv, painted);
                    assert_eq!(&csv[..5], b"APRS ");
                    assert_eq!(csv.len(), 9);
                    for (hex, byte) in csv[5..].as_chunks::<2>().0.iter().zip(raw) {
                        assert!(
                            hex.iter()
                                .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(b))
                        );
                        assert_eq!(
                            u8::from_str_radix(std::str::from_utf8(hex).unwrap(), 16).unwrap(),
                            byte
                        );
                    }
                }
                assert!(csv.iter().all(|b| !b"\0\r\n,;\"\\".contains(b)));
            }
        }
        for raw in [b"".as_slice(), b"/", b"/>>"] {
            assert!(matches!(
                format(request(A::Symbol, raw, 0), &context()),
                Err(Error::Invalid(_))
            ));
            assert!(matches!(
                paint::format(
                    request(A::Symbol, raw, 0),
                    &paint::Context::default(),
                    || panic!()
                ),
                Err(Error::Invalid(_))
            ));
        }
    }

    pub(super) fn context() -> datetime::Context<'static> {
        datetime::Context {
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
    fn request(adapter: A, source: &[u8], offset: i16) -> payload::Request<'_> {
        payload::Request {
            adapter,
            source,
            utc_offset_minutes: offset,
        }
    }
    fn put32(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    pub(super) fn symbol_source() -> Vec<u8> {
        // Synthetic PE geometry, not copied resource tables or a factory image.
        let mut pe = vec![0; 0xa00];
        pe[..2].copy_from_slice(b"MZ");
        put32(&mut pe, 0x3c, 0x80);
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        pe[0x84..0x86].copy_from_slice(&0x14c_u16.to_le_bytes());
        pe[0x86] = 1;
        pe[0x94..0x96].copy_from_slice(&0xe0_u16.to_le_bytes());
        pe[0x98..0x9a].copy_from_slice(&0x10b_u16.to_le_bytes());
        put32(&mut pe, 0xb4, 0x400000);
        put32(&mut pe, 0x180, 0x1000);
        put32(&mut pe, 0x184, 0x418f18);
        put32(&mut pe, 0x188, 0x800);
        put32(&mut pe, 0x18c, 0x200);
        for i in 0..=192 {
            put32(&mut pe, 0x200 + 8 * i, 0x819618);
            put32(&mut pe, 0x204 + 8 * i, u32::MAX); // Locale 1 is not a compact source.
        }
        let mut at = 0x701;
        for (slot, text) in [
            (0xe8, b"P0\0".as_slice()),
            (0x1d0, b"P1\0"),
            (0x290, b"P2\0"),
            (0x3e8, b"A0\0"),
            (0x600, b"S0\0"),
        ] {
            put32(&mut pe, 0x200 + slot, 0x818f18 + at as u32);
            pe[0x200 + at..0x200 + at + text.len()].copy_from_slice(text);
            at += text.len();
        }
        pe
    }
    fn cell(adapter: A, source: &[u8], offset: i16) -> Result<Vec<u8>, Error> {
        format(request(adapter, source, offset), &context())
    }

    #[test]
    fn nine_request_widths_and_non_rx_adapters_fail_closed() {
        for (adapter, width) in [
            (A::RxType, 1),
            (A::RxStatus, 1),
            (A::Symbol, 2),
            (A::Directivity, 1),
            (A::DataType, 1),
            (A::HistoryDate, 6),
            (A::Timestamp, 3),
            (A::Message, 20),
            (A::GpsMessage, 43),
        ] {
            for n in [0, width - 1, width + 1] {
                assert!(matches!(
                    cell(adapter, &vec![0; n], 0),
                    Err(Error::Invalid(_))
                ));
            }
        }
        assert!(matches!(cell(A::Mode, &[0], 0), Err(Error::Invalid(_))));
    }

    #[test]
    fn static_labels_independent_symbols_and_source_oracle_are_distinct() {
        for (adapter, raw, expected) in [
            (A::RxType, b"\0".as_slice(), b"RF".as_slice()),
            (A::RxType, b"\x01", b"TM"),
            (A::RxType, b"\x02", b"MONI"),
            (A::RxStatus, b"\0", b""),
            (A::RxStatus, b"\x01", b"RPT?"),
            (A::RxStatus, b"\x02", b"UR?"),
            (A::RxStatus, b"\x03", b"RPT UP"),
            (A::Directivity, b"\0", b"Omni"),
            (A::Directivity, b"\x01", b"45"),
            (A::Directivity, b"\x08", b"360"),
            (A::Directivity, b"\xff", b""),
            (A::DataType, b"\0", b"Live Object"),
            (A::DataType, b"\x01", b"Live Item"),
            (A::DataType, b"\x02", b"Killed Object"),
            (A::DataType, b"\x03", b"Killed Item"),
            (A::DataType, b"\xff", b""),
            (A::Symbol, b"\xff\0", b""),
            (A::Symbol, b"/>", b"APRS 2F3E"),
            (A::Symbol, b"/[", b"APRS 2F5B"),
            (A::Symbol, b"/s", b"APRS 2F73"),
            (A::Symbol, b"IY", b"APRS 4959"),
            (A::Symbol, b"\\>", b"APRS 5C3E"),
            (A::Symbol, b"A>", b"APRS 413E"),
            (A::Symbol, b"0>", b"APRS 303E"),
            (A::Symbol, b"/d", b"APRS 2F64"),
            (A::Symbol, b"\\[", b"APRS 5C5B"),
            (A::Symbol, b"A[", b"APRS 415B"),
            (A::Symbol, b"\x01A", b"APRS 0141"),
            (A::Symbol, b"/\xff", b"APRS 2FFF"),
            (A::Symbol, b",\xff", b"APRS 2CFF"),
        ] {
            assert_eq!(cell(adapter, raw, 0).unwrap(), expected);
        }
        for (adapter, raw) in [
            (A::RxType, b"\xff".as_slice()),
            (A::RxStatus, b"\x04"),
            (A::Directivity, b"\x09"),
            (A::DataType, b"\x04"),
        ] {
            assert!(matches!(
                cell(adapter, raw, 0),
                Err(Error::SourceUnavailable(_))
            ));
        }
        assert_eq!(cell(A::Symbol, b"/?", 0).unwrap(), b"APRS 2F3F");
        assert!(matches!(
            symbol_from_source(b"/>", b"", false, true),
            Err(Error::SourceUnavailable(_))
        ));
        assert_eq!(
            symbol_from_source(b"\xffX", b"not PE", false, true).unwrap(),
            b""
        );
        assert_eq!(
            symbol_from_source(b"/\xff", b"not PE", false, true).unwrap(),
            b"/"
        );
    }

    #[test]
    fn symbol_source_selects_all_ninety_six_slots_locale_zero_and_real_empty() {
        let mut pe = symbol_source();
        pe[0x980..0x982].copy_from_slice(b"P\0");
        pe[0x990..0x994].copy_from_slice(b"ALT\0");
        for i in 0..96 {
            put32(&mut pe, 0x200 + 8 * i, 0x819698);
            put32(&mut pe, 0x500 + 8 * i, 0x8196a8);
        }
        for second in 33..=128 {
            assert_eq!(
                symbol_from_source(&[b'/', second], &pe, false, true).unwrap(),
                b"P"
            );
            assert_eq!(
                symbol_from_source(&[b'\\', second], &pe, false, true).unwrap(),
                b"ALT"
            );
        }
        assert_eq!(
            symbol_from_source(b"0!", &pe, false, true).unwrap(),
            b"ALT (0)"
        );
        assert_eq!(symbol_from_source(b"IY", &pe, false, true).unwrap(), b"S0");
        // A real empty special caption is success, NOT the primary/alternate
        // empty-caption direct branch. C895F marks IY found unconditionally.
        put32(&mut pe, 0x800, 0x819618);
        assert_eq!(symbol_from_source(b"IY", &pe, false, true).unwrap(), b"");
        pe[0x990..0x997].copy_from_slice(b"Q,R;  \0");
        put32(&mut pe, 0x2ec, 0x8196a8);
        assert_eq!(
            symbol_from_source(b"/>", &pe, true, false).unwrap(),
            b"Q,R;"
        );
        assert_eq!(symbol_from_source(b"/>", &pe, true, true).unwrap(), b"P");
        assert_eq!(
            symbol_from_source(b"\xffX", &pe, true, false).unwrap(),
            b"---"
        );
    }

    #[test]
    fn symbol_pe_geometry_pointer_nul_buffer_and_source_failures_are_checked() {
        let pe = symbol_source();
        let render = |source: &[u8], raw: &[u8]| symbol_from_source(raw, source, false, true);
        for len in [0, 1, 63, 0x83, 0x97, 0x177, 0x200, 0x2e8, 0x900] {
            assert!(render(&pe[..len], b"/>").is_err());
        }
        for (at, value) in [
            (0x3c, u32::MAX),
            (0xb4, 0x500000),
            (0x18c, u32::MAX),
            (0x2e8, 0),
            (0x2e8, u32::MAX),
            (0x2e8, 0x819718),
        ] {
            let mut bad = pe.clone();
            put32(&mut bad, at, value);
            assert!(render(&bad, b"/>").is_err());
        }
        let mut bad = pe.clone();
        bad[0x84..0x86].copy_from_slice(&0xaa64_u16.to_le_bytes());
        assert!(matches!(render(&bad, b"/>"), Err(Error::Invalid(_))));
        let mut bad = pe.clone();
        bad[0x98..0x9a].copy_from_slice(&0x20b_u16.to_le_bytes());
        assert!(matches!(render(&bad, b"/>"), Err(Error::Invalid(_))));
        let mut bad = pe.clone();
        bad[0x86] = 97;
        assert!(matches!(render(&bad, b"/>"), Err(Error::Invalid(_))));
        let mut bad = pe.clone();
        bad[0x86] = 2;
        bad.copy_within(0x178..0x1a0, 0x1a0);
        assert!(matches!(render(&bad, b"/>"), Err(Error::Invalid(_))));
        let mut bounds = pe.clone();
        put32(&mut bounds, 0x2e8, 0x819698);
        bounds[0x980..0x9a8].fill(b'Q');
        assert!(matches!(
            render(&bounds, b"/>"),
            Err(Error::SourceUnavailable(_))
        ));
        bounds[0x9a6] = 0;
        assert_eq!(render(&bounds, b"/>").unwrap(), vec![b'Q'; 38]);
        put32(&mut bounds, 0x5e8, 0x819698);
        assert!(matches!(
            render(&bounds, b"A>"),
            Err(Error::SourceUnavailable(_))
        ));
        bounds[0x9a2] = 0;
        assert_eq!(
            render(&bounds, b"A>").unwrap(),
            [vec![b'Q'; 34], b" (A)".to_vec()].concat()
        );
        bounds[0x980..0x986].copy_from_slice(b"A,\x81;B\0");
        assert_eq!(render(&bounds, b"/>").unwrap(), b"A \x81 B");
        put32(&mut bounds, 0x2e8, 0x819717);
        bounds[0x9ff] = b'Q';
        assert!(matches!(
            render(&bounds, b"/>"),
            Err(Error::SourceUnavailable(_))
        ));
    }

    #[test]
    fn message_fixed_nul_trim_control_delimiter_and_kana_order_matches_literals() {
        for (adapter, width) in [(A::Message, 20), (A::GpsMessage, 43)] {
            let mut source = vec![b' '; width];
            let prefix = b" a\x81\xa1\xff,;\r\nB\tC \0TAIL";
            source[..prefix.len()].copy_from_slice(prefix);
            assert_eq!(cell(adapter, &source, 0).unwrap(), b" a_\xa1_  __B_C");
            source.fill(b' ');
            assert_eq!(cell(adapter, &source, 0).unwrap(), b"");
            source.fill(255);
            assert_eq!(cell(adapter, &source, 0).unwrap(), vec![b'_'; width]);
            source.fill(b'X');
            source[0] = 0;
            assert_eq!(cell(adapter, &source, 0).unwrap(), b"");
            source.fill(b'X');
            source[width - 1] = b';';
            assert_eq!(cell(adapter, &source, 0).unwrap(), vec![b'X'; width - 1]);
            source[width - 1] = 0x81;
            let mut expected = vec![b'X'; width];
            expected[width - 1] = b'_';
            assert_eq!(cell(adapter, &source, 0).unwrap(), expected);
        }
        let mut source = [b' '; 20];
        source[..4].copy_from_slice(b"A\r\nB");
        assert_eq!(cell(A::Message, &source, 0).unwrap(), b"A__B");
        for (byte, expected) in [
            (0x1f, b'A'),
            (0x20, b'A'),
            (0x7e, b'~'),
            (0x7f, b'_'),
            (0x80, b'_'),
            (0xa0, b'_'),
            (0xa1, 0xa1),
            (0xdf, 0xdf),
            (0xe0, b'_'),
            (0xff, b'_'),
        ] {
            source.fill(b' ');
            source[0] = b'A';
            source[1] = byte;
            let expected = if byte <= 0x20 {
                vec![b'A']
            } else {
                vec![b'A', expected]
            };
            assert_eq!(cell(A::Message, &source, 0).unwrap(), expected);
        }
    }

    #[test]
    fn history_date_offset_precedes_invalid_empty_and_uses_live_short_date() {
        for (raw, offset, expected) in [
            (
                [0x31, 0x12, 0x24, 0x23, 0x30, 0x45],
                60,
                b"2025/01/01 00:30:45".as_slice(),
            ),
            ([1, 1, 0, 0, 0x30, 0], -60, b"1999/12/31 23:30:00"),
            ([1, 1, 0xff, 0, 0, 0], 0, b"2099/01/01 00:00:00"),
            ([1, 1, 0, 0, 0, 0], 15360, b"2000/01/01 00:00:00"),
            ([1, 1, 0, 0, 0, 0], -15361, b"1999/12/31 23:59:00"),
            ([0x31, 2, 0x24, 0, 0, 0], 0, b""),
            ([1, 1, 0x24, 0xff, 0, 0], 0, b""),
        ] {
            assert_eq!(cell(A::HistoryDate, &raw, offset).unwrap(), expected);
        }
        for offset in [1440, -1440, 32767, i16::MIN] {
            assert!(matches!(
                cell(A::HistoryDate, &[0xff; 6], offset),
                Err(Error::Conversion(_))
            ));
        }
        let mut c = context();
        c.short_date = b"dd/mm/yyyy";
        c.date_separator = b'-';
        c.time_separator = 0;
        let raw = [0x08, 0x11, 0x26, 1, 2, 3];
        assert_eq!(
            format(request(A::HistoryDate, &raw, 0), &c).unwrap(),
            b"08-11-2026 010203"
        );
        c.short_date = b"gg e/mm/dd";
        assert!(matches!(
            format(request(A::HistoryDate, &raw, 0), &c),
            Err(Error::SourceUnavailable(_))
        ));
        assert_eq!(
            format_with_date(request(A::HistoryDate, &raw, 0), &c, |_| {
                Err(Error::Operation("native date failure"))
            }),
            Err(Error::Operation("native date failure"))
        );
        assert_eq!(
            format_with_date(request(A::HistoryDate, &raw, 0), &c, |token| {
                assert_eq!(token.date.year, 2026);
                Ok(match token.letter {
                    b'g' => b"ERA".to_vec(),
                    b'e' => b"7".to_vec(),
                    _ => panic!(),
                })
            })
            .unwrap(),
            b"ERA 7-11-08 010203"
        );
        assert_eq!(
            format_with_date(request(A::HistoryDate, &raw, 0), &c, |_| Ok(Vec::new())).unwrap(),
            b" -11-08 010203"
        );
        assert_eq!(
            format_with_date(request(A::HistoryDate, &[0xff; 6], 0), &c, |_| panic!(
                "invalid compact date never reads native token"
            ))
            .unwrap(),
            b""
        );
    }

    #[test]
    fn timestamp_uses_shared_nonthrowing_offset_and_invalid_second_rules() {
        for (raw, offset, expected) in [
            ([0x23, 0x59, 0xff], 1, b"00:00:00".as_slice()),
            ([0, 0, 0x30], -1, b"23:59:30"),
            ([0, 0, 0x30], 1440, b"00:00:30"),
            ([0, 0, 0x30], i16::MIN, b"00:00:30"),
            ([0x24, 0, 0], 1440, b""),
            ([0, 0x60, 0], 0, b""),
        ] {
            assert_eq!(cell(A::Timestamp, &raw, offset).unwrap(), expected);
        }
        let mut c = context();
        c.time_separator = 0;
        assert_eq!(
            format(request(A::Timestamp, &[0x23, 0x59, 0xff], 1), &c).unwrap(),
            b"000000"
        );
    }

    #[test]
    fn actual_reverse_rank_loader_reaches_shared_thirty_seven_cell_payload() {
        use crate::icf::{Icf, record};
        let mut memory = vec![0; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        memory[0x3c2ff..0x3c331].fill(255);
        memory[0x3c2ff + 7] = 3;
        memory[0x3c2ff + 9] = 3;
        let base = 0x309c3;
        memory[base + 7] = 0x21;
        let at = base + 0x32 + 48 * 7;
        memory[at..at + 36].fill(b' ');
        memory[at..at + 8].copy_from_slice(b"FIRST   ");
        memory[at + 8..at + 12].copy_from_slice(b"SUFX");
        memory[at + 36..at + 40].copy_from_slice(&145_000_000_u32.to_le_bytes());
        memory[at + 41..at + 47].copy_from_slice(&[0x31, 0x12, 0x24, 0x23, 0x59, 0]);
        memory[at + 47] = 2;
        let msg = base + 0x992 + 20 * 7;
        memory[msg..msg + 20].fill(b' ');
        memory[msg..msg + 4].copy_from_slice(b"A,B;");
        let gps = base + 0xd7a + 92 * 7;
        memory[gps + 3] = b'S';
        memory[gps + 4] = 255;
        memory[gps + 8..gps + 12].copy_from_slice(&i32::MAX.to_le_bytes());
        memory[gps + 12..gps + 14].copy_from_slice(b"/>");
        memory[gps + 14..gps + 16].fill(255);
        memory[gps + 26..gps + 29].copy_from_slice(&[0x23, 0x59, 0xff]);
        memory[gps + 25] = 255; // Timestamp type is not an added CSV gate.
        memory[gps + 29] = 255; // Unknown GPS union tag keeps shared defaults.
        memory[gps + 49..gps + 92].fill(b' ');
        memory[gps + 49..gps + 53].copy_from_slice(b"GPS;");
        let image = Icf::from_memory(memory).unwrap();
        let mut h = record::reset_holder(0x0e, 3, 1);
        record::load_receive_history_storage_into(&image, 3, &mut h).unwrap();
        let c = payload::Context {
            separator: b',',
            decimal: b'.',
            row: 3,
            number_base: 0,
            group: 0,
            group_name: b"",
            alarm: false,
            utc_offset_minutes: 1,
            memory_group_columns: false,
            memory_selection_column: false,
            memory_pair: false,
        };
        let out = payload::build_portable(
            payload::Kind::ReceiveHistory,
            payload::Holder::new(h.as_bytes()).unwrap(),
            c,
            |r| {
                assert_eq!(r.utc_offset_minutes, 1);
                format(r, &context())
            },
        )
        .unwrap();
        assert!(out.ends_with(b"\r\n"));
        let cells = super::super::split_cells(&out[..out.len() - 2], b',');
        assert_eq!(cells.len(), 37);
        assert_eq!(
            &cells[..5],
            &[b"MONI".as_slice(), b"145.000000", b"LSB", b"FIRST", b"SUFX"]
        );
        assert_eq!(
            &cells[8..14],
            &[
                b"A B".as_slice(),
                b"RPT?",
                b"2025/01/01 00:00:00",
                b"*",
                b"",
                b"-0.000000"
            ]
        );
        assert_eq!(cells[17], b"APRS 2F3E");
        assert_eq!(&cells[35..], &[b"00:00:00".as_slice(), b"GPS"]);
        record::load_receive_history_storage_into(&image, 2, &mut h).unwrap();
        assert!(!h.valid());
        assert_eq!(
            payload::build_portable(
                payload::Kind::ReceiveHistory,
                payload::Holder::new(h.as_bytes()).unwrap(),
                c,
                |_| panic!("invalid H has no cell requests")
            )
            .unwrap(),
            [vec![b','; 36], b"\r\n".to_vec()].concat()
        );
    }
}
