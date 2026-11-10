//! Ordered CSV alias tables (B5F30/B60B8), kept as caller-supplied resource data.
//! A label->value map would destroy first-match collisions such as RPS3/RPS4.
//! Table bytes are not Unicode text and are never copied from factory payloads.

use super::{Error, trim};

/// Public IC-705 DTCS labels, represented as decimal digits, not octal numbers.
/// The saved index selects this104-member set; arbitrary three-digit codes are
/// not accepted. This is a public radio symbol set, not a factory image.
pub const DTCS_CODES: [u16; 104] = [
    23, 25, 26, 31, 32, 36, 43, 47, 51, 53, 54, 65, 71, 72, 73, 74, 114, 115, 116, 122, 125, 131,
    132, 134, 143, 145, 152, 155, 156, 162, 165, 172, 174, 205, 212, 223, 225, 226, 243, 244, 245,
    246, 251, 252, 255, 261, 263, 265, 266, 271, 274, 306, 311, 315, 325, 331, 332, 343, 346, 351,
    356, 364, 365, 371, 411, 412, 413, 423, 431, 432, 445, 446, 452, 454, 455, 462, 464, 465, 466,
    503, 506, 516, 523, 526, 532, 546, 565, 606, 612, 624, 627, 631, 632, 654, 662, 664, 703, 712,
    723, 731, 732, 734, 743, 754,
];

/// One source table entry. Duplicate labels and duplicate values are meaningful.
#[derive(Debug, Clone, Copy)]
pub struct Entry<'a> {
    pub label: &'a [u8],
    pub value: u32,
}

/// Normalize with actual CharLowerBuffA-equivalent output, in original order.
pub fn import_with(
    source: &[u8],
    entries: &[Entry<'_>],
    mut lower: impl FnMut(&[u8]) -> Result<Vec<u8>, Error>,
) -> Result<Option<u32>, Error> {
    let source = lower(trim(source))?;
    for entry in entries {
        if lower(entry.label)? == source {
            return Ok(Some(entry.value));
        }
    }
    Ok(None)
}
/// Explicit ASCII mapping for known ASCII resource contexts, not all NLS.
pub fn import_ascii(source: &[u8], entries: &[Entry<'_>]) -> Option<u32> {
    entries
        .iter()
        .find(|entry| trim(source).eq_ignore_ascii_case(entry.label))
        .map(|entry| entry.value)
}
/// B60B8 returns the first matching value's label; missing output is an explicit
/// resource input, not an invented empty/default success.
pub fn export<'a>(value: u32, entries: &[Entry<'a>], not_found: &'a [u8]) -> &'a [u8] {
    entries
        .iter()
        .find(|entry| entry.value == value)
        .map_or(not_found, |entry| entry.label)
}
/// B6AB4 searches the secondary mode table only on primary miss. Only a secondary
/// success increments the correction counter; canonical alias changes do not.
pub fn mode_ascii(
    source: &[u8],
    primary: &[Entry<'_>],
    secondary: &[Entry<'_>],
    corrections: &mut u32,
) -> Option<u32> {
    if let Some(value) = import_ascii(source, primary) {
        return Some(value);
    }
    let value = import_ascii(source, secondary)?;
    *corrections = corrections.wrapping_add(1);
    Some(value)
}
/// B7BD8 adds exactly one leading zero to trimmed strings of length <=2. It does
/// not pad to width3 or accept arbitrary octal codes absent from the source table.
pub fn dtcs_ascii(source: &[u8], entries: &[Entry<'_>]) -> Option<u32> {
    let source = trim(source);
    if source.len() <= 2 {
        let mut text = vec![b'0'];
        text.extend_from_slice(source);
        import_ascii(&text, entries)
    } else {
        import_ascii(source, entries)
    }
}
/// B6D31 fallback is a four-byte DATA prefix; B7355 fallback requires full ON.
pub fn data_ascii(source: &[u8], entries: &[Entry<'_>], corrections: &mut u32) -> Option<u32> {
    if let Some(value) = import_ascii(source, entries) {
        return Some(value);
    }
    if trim(source)
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"data"))
    {
        *corrections = corrections.wrapping_add(1);
        Some(1)
    } else {
        None
    }
}
pub fn on_ascii(source: &[u8], entries: &[Entry<'_>], corrections: &mut u32) -> Option<u32> {
    if let Some(value) = import_ascii(source, entries) {
        return Some(value);
    }
    if trim(source).eq_ignore_ascii_case(b"on") {
        *corrections = corrections.wrapping_add(1);
        Some(1)
    } else {
        None
    }
}

/// ASCII Group/Bank normal table computed from its public ordinal schema.
/// Missing header is represented by None by the caller, not by this function.
pub fn gps_group_ascii(source: &[u8]) -> u32 {
    let source = trim(source);
    if source.eq_ignore_ascii_case(b"off") {
        0
    } else if source.len() == 1 && source[0].is_ascii_alphabetic() {
        u32::from(source[0].to_ascii_uppercase() - b'A' + 1)
    } else {
        0xffff
    }
}
pub fn gps_group_text(group: u32) -> Result<Vec<u8>, Error> {
    match group {
        0 => Ok(b"OFF".to_vec()),
        1..=26 => Ok(vec![b'A' + group as u8 - 1]),
        _ => Ok(b"NOT_FOUND".to_vec()),
    }
}

/// Static public CSV labels and normal tone frequencies, not OS/runtime output.
/// None identifies a still-external resource/calendar profile; it is not an
/// empty successful label. Full DWORD values are never storage-codec narrowed.
pub fn portable_label(
    kind: super::payload::Kind,
    request: &super::payload::Request<'_>,
    decimal: u8,
) -> Result<Option<Vec<u8>>, Error> {
    use super::payload::Adapter as A;
    if request.adapter == A::Symbol {
        if request.source.len() != 2 {
            return Err(Error::Invalid("Rx symbol requires raw2"));
        }
        // Raw resource bytes belong to history's bounded PE source lookup.
        return Ok(None);
    }
    if matches!(
        request.adapter,
        A::RxType | A::RxStatus | A::Directivity | A::DataType
    ) {
        return rx_label(request).map(Some);
    }
    if !matches!(
        request.adapter,
        A::Group
            | A::Alarm
            | A::Duplex
            | A::RepeaterDuplex
            | A::RepeaterMode
            | A::RepeaterUse
            | A::Position
            | A::Tone
            | A::ToneFrequency
            | A::Mode
            | A::Selection
            | A::Data
            | A::Filter
            | A::Split
            | A::DtcsPolarity
            | A::DigitalSquelch
            | A::Dtcs
            | A::UtcOffsetSentinel
    ) {
        return Ok(None);
    }
    let value = match request.source {
        [n] => u32::from(*n),
        [a, b] => u32::from(u16::from_le_bytes([*a, *b])),
        [a, b, c, d] => u32::from_le_bytes([*a, *b, *c, *d]),
        _ => return Err(Error::Invalid("CSV scalar label width")),
    };
    let missing = b"NOT_FOUND".as_slice();
    let label: &[u8] = match request.adapter {
        A::Group => return gps_group_text(value).map(Some),
        A::ToneFrequency => {
            let tone = if value == 50 {
                1500
            } else {
                match crate::repeater::REPEATER_TONES.get(value as usize) {
                    Some(tone) => *tone,
                    None => return Ok(Some(missing.to_vec())),
                }
            };
            let mut label = format!("{}.{}Hz", tone / 10, tone % 10).into_bytes();
            if decimal != b'.'
                && let Some(at) = label.iter().position(|b| *b == b'.')
            {
                label[at] = decimal;
            }
            return Ok(Some(label));
        }
        A::Dtcs => {
            return Ok(Some(DTCS_CODES.get(value as usize).map_or_else(
                || missing.to_vec(),
                |code| format!("{code:03}").into_bytes(),
            )));
        }
        A::UtcOffsetSentinel => b"--:--",
        _ => static_label(kind, request.adapter, value).unwrap_or(missing),
    };
    Ok(Some(label.to_vec()))
}

// The compact Rx profile has different resource keys from GUI Type/Status.
// Unknown physical resource slots and table-outside-memory are not NOT_FOUND.
fn rx_label(request: &super::payload::Request<'_>) -> Result<Vec<u8>, Error> {
    use super::payload::Adapter as A;
    let label: &[u8] = match (request.adapter, request.source) {
        (A::RxStatus, [0]) | (A::Directivity | A::DataType, [255]) => b"",
        (A::RxType, [0]) => b"RF",
        (A::RxType, [1]) => b"TM",
        (A::RxType, [2]) => b"MONI",
        (A::RxStatus, [1]) => b"RPT?",
        (A::RxStatus, [2]) => b"UR?",
        (A::RxStatus, [3]) => b"RPT UP",
        (A::Directivity, [0]) => b"Omni",
        (A::Directivity, [n @ 1..=8]) => return Ok((u32::from(*n) * 45).to_string().into_bytes()),
        (A::DataType, [0]) => b"Live Object",
        (A::DataType, [1]) => b"Live Item",
        (A::DataType, [2]) => b"Killed Object",
        (A::DataType, [3]) => b"Killed Item",
        (_, [_]) => {
            return Err(Error::SourceUnavailable(
                "Rx label outside normal source profile",
            ));
        }
        _ => return Err(Error::Invalid("Rx scalar label requires raw1")),
    };
    Ok(label.to_vec())
}

/// Built-in ASCII enum import over the same semantics used by export. The
/// numeric scan preserves the only duplicate label (DUP3/4 -> first3); explicit
/// D-STAR aliases are primary successes, never an invented correction event.
pub fn builtin_import_ascii(
    kind: super::payload::Kind,
    adapter: super::payload::Adapter,
    source: &[u8],
) -> Option<u32> {
    use super::payload::Adapter as A;
    if adapter == A::Dtcs {
        let text = trim(source);
        let prefixed;
        let text = if text.len() <= 2 {
            prefixed = [b"0".as_slice(), text].concat();
            &prefixed
        } else {
            text
        };
        return DTCS_CODES
            .iter()
            .position(|code| format!("{code:03}").as_bytes() == text)
            .map(|index| index as u32);
    }
    if trim(source).eq_ignore_ascii_case(b"D-STAR") {
        match adapter {
            A::Mode => return Some(16),
            A::RepeaterMode => return Some(2),
            _ => {}
        }
    }
    (0..=255).find(|value| {
        static_label(kind, adapter, *value)
            .is_some_and(|label| trim(source).eq_ignore_ascii_case(label))
    })
}

/// Portable ASCII field reader, including native correction-only alias paths.
/// Unknown labels return FFFF; unsupported adapters are not successful misses.
/// Tone syntax normalizes comma itself, independently of the copied CSV point.
pub fn portable_import(
    adapter: super::payload::Adapter,
    source: &[u8],
    _decimal: u8,
    crt_decimal: u8,
    corrections: &mut u32,
) -> Result<u32, Error> {
    use super::payload::{Adapter as A, Kind};
    if adapter == A::ToneFrequency {
        return tone_builtin_ascii(source, false, crt_decimal, corrections);
    }
    if adapter == A::Group {
        return Ok(gps_group_ascii(source));
    }
    if !matches!(
        adapter,
        A::Mode
            | A::Selection
            | A::Data
            | A::Filter
            | A::Duplex
            | A::Split
            | A::Tone
            | A::Dtcs
            | A::DtcsPolarity
            | A::DigitalSquelch
            | A::Alarm
            | A::RepeaterMode
            | A::RepeaterDuplex
            | A::RepeaterUse
            | A::Position
    ) {
        return Err(Error::SourceUnavailable(
            "portable import adapter has no enum reader",
        ));
    }
    if let Some(value) = builtin_import_ascii(Kind::Memory, adapter, source) {
        return Ok(value);
    }
    let text = trim(source);
    let alternate = match adapter {
        A::Mode
            if [
                b"S-AM (D)".as_slice(),
                b"S-AM (U)",
                b"S-AM (L)",
                b"S-AM(D)",
                b"S-AM(U)",
                b"S-AM(L)",
                b"AM-N",
            ]
            .iter()
            .any(|label| text.eq_ignore_ascii_case(label)) =>
        {
            Some(8)
        }
        A::Mode if text.eq_ignore_ascii_case(b"FM-N") => Some(13),
        A::Data
            if text
                .get(..4)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"data")) =>
        {
            Some(1)
        }
        A::Selection if text.eq_ignore_ascii_case(b"on") => Some(1),
        _ => None,
    };
    if let Some(value) = alternate {
        *corrections = corrections.wrapping_add(1);
        Ok(value)
    } else {
        Ok(0xffff)
    }
}

/// Public normal50/MTON51 profiles. Alias51 is checked before candidate profile
/// selection, and candidate order is r+10*c, including MTON's50 in its first row.
pub fn tone_builtin_ascii(
    source: &[u8],
    mtone: bool,
    crt_point: u8,
    corrections: &mut u32,
) -> Result<u32, Error> {
    let mut values = crate::repeater::REPEATER_TONES.to_vec();
    values.push(1500);
    let labels: Vec<_> = values
        .iter()
        .map(|n| format!("{}.{}Hz", n / 10, n % 10).into_bytes())
        .collect();
    let aliases: Vec<_> = labels
        .iter()
        .enumerate()
        .map(|(value, label)| Entry {
            label,
            value: value as u32,
        })
        .collect();
    let limit = if mtone { 51 } else { 50 };
    let candidates: Vec<_> = (0..10)
        .flat_map(|r| (0..6).map(move |c| r + 10 * c))
        .filter(|value| *value < limit)
        .map(|value| ToneCandidate {
            value: value as u32,
            label: &labels[value][..labels[value].len() - 2],
        })
        .collect();
    tone_ascii(source, &aliases, &candidates, corrections, |text| {
        super::number::crt_decimal_prefix(text, crt_point)
    })
}
fn static_label(
    kind: super::payload::Kind,
    adapter: super::payload::Adapter,
    value: u32,
) -> Option<&'static [u8]> {
    use super::payload::{Adapter as A, Kind};
    let label: &[u8] = match adapter {
        A::Mode => match value {
            0 => b"LSB",
            1 => b"USB",
            2 => b"CW",
            3 => b"CW-R",
            6 => b"RTTY",
            7 => b"RTTY-R",
            8 => b"AM",
            13 => b"FM",
            15 => b"WFM",
            16 => b"DV",
            24 => b"Auto",
            25 => b"-",
            _ => return None,
        },
        A::Data | A::Split | A::Alarm => match value {
            0 => b"OFF",
            1 => b"ON",
            _ => return None,
        },
        A::Filter => match value {
            0 => b"1",
            1 => b"2",
            2 => b"3",
            255 => b"-",
            _ => return None,
        },
        A::Selection => match value {
            0 => b"OFF",
            1 => b"SEL1",
            2 => b"SEL2",
            3 => b"SEL3",
            _ => return None,
        },
        A::Tone => match value {
            0 => b"OFF",
            1 => b"TONE",
            2 => b"TSQL",
            _ if kind == Kind::Repeater => return None,
            4 => b"DTCS",
            6 => b"DTCS(T)",
            7 => b"TONE(T)/DTCS(R)",
            8 => b"DTCS(T)/TSQL(R)",
            9 => b"TONE(T)/TSQL(R)",
            _ => return None,
        },
        A::DtcsPolarity => match value {
            2 => b"BOTH N",
            3 => b"TN-RR",
            4 => b"TR-RN",
            5 => b"BOTH R",
            _ => return None,
        },
        A::DigitalSquelch => match value {
            0 => b"OFF",
            2 => b"DSQL",
            4 => b"CSQL",
            _ => return None,
        },
        A::Duplex => match value {
            0 => b"OFF",
            1 => b"DUP-",
            2 => b"DUP+",
            3 | 4 => b"RPS",
            _ => return None,
        },
        A::RepeaterDuplex => match value {
            0 => b"OFF",
            1 => b"DUP-",
            2 => b"DUP+",
            3 => b"RPS",
            _ => return None,
        },
        A::RepeaterMode => match value {
            0 => b"FM",
            1 => b"FM-N",
            2 => b"DV",
            3 => b"DD",
            _ => return None,
        },
        A::RepeaterUse => match value {
            0 => b"NO",
            1 => b"YES",
            _ => return None,
        },
        A::Position => match value {
            0 => b"None",
            1 => b"Approximate",
            2 => b"Exact",
            _ => return None,
        },
        _ => return None,
    };
    Some(label)
}

/// Resource candidate order is row-major r+10*c, not ascending numeric tone.
#[derive(Debug, Clone, Copy)]
pub struct ToneCandidate<'a> {
    pub value: u32,
    pub label: &'a [u8],
}

/// B77E8: comma->dot, first alias (including index50), then suffix/range check.
/// Nearest selection re-parses ORIGINAL numeric text with CRT, not D5560's
/// already parsed double; strict smaller difference keeps the first tie.
/// Wrapper ignores selector bool, retains initial index8, and corrects once.
pub fn tone_ascii(
    source: &[u8],
    aliases: &[Entry<'_>],
    candidates: &[ToneCandidate<'_>],
    corrections: &mut u32,
    mut crt: impl FnMut(&[u8]) -> Result<f64, Error>,
) -> Result<u32, Error> {
    if source.len() > 4096 {
        return Err(Error::Limit("CSV tone source exceeds 4096 bytes"));
    }
    let normalized: Vec<_> = source
        .iter()
        .map(|b| if *b == b',' { b'.' } else { *b })
        .collect();
    if let Some(value) = import_ascii(&normalized, aliases) {
        return Ok(value);
    }
    let text = trim(&normalized);
    if text.len() < 2 || !text[text.len() - 2..].eq_ignore_ascii_case(b"hz") {
        return Ok(0xffff);
    }
    let number = &text[..text.len() - 2];
    let value = match super::number::decimal(number, b'.') {
        Ok(value) => value,
        Err(Error::Conversion(_)) => return Ok(0xffff),
        Err(error) => return Err(error),
    };
    if !(60.0..=300.0).contains(&value) {
        return Ok(0xffff);
    }
    let input = crt(number)?;
    let mut best = 8;
    let mut difference = f64::MAX;
    for candidate in candidates {
        let distance = (input - crt(candidate.label)?).abs();
        if distance < difference {
            difference = distance;
            best = candidate.value;
        }
    }
    *corrections = corrections.wrapping_add(1);
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_reader_counts_secondary_only_and_keeps_native_misses() {
        use super::super::payload::Adapter as A;
        let mut corrections = u32::MAX;
        for (adapter, text, expected, count) in [
            (A::Mode, b"D-STAR".as_slice(), 16, u32::MAX),
            (A::Mode, b"fm-n", 13, 0),
            (A::Mode, b"S-AM (U)", 8, 1),
            (A::Data, b"database", 1, 2),
            (A::Selection, b"on", 1, 3),
            (A::Selection, b"onward", 0xffff, 3),
            (A::Dtcs, b"23", 0, 3),
            (A::Dtcs, b"3", 0xffff, 3),
            (A::Duplex, b"RPS", 3, 3),
            (A::Mode, b"14", 0xffff, 3),
            (A::ToneFrequency, b"150,0Hz", 50, 3),
        ] {
            assert_eq!(
                portable_import(adapter, text, b',', b'.', &mut corrections).unwrap(),
                expected
            );
            assert_eq!(corrections, count);
        }
        assert!(portable_import(A::GpsDate, b"0", b'.', b'.', &mut corrections).is_err());
    }
    #[test]
    fn builtin_symbols_use_full_dwords_and_dtcs_membership_not_generic_numbers() {
        use super::super::payload::{Adapter as A, Kind, Request};
        for (adapter, value, expected) in [
            (A::Mode, 2_u32, b"CW".as_slice()),
            (A::Mode, 3, b"CW-R"),
            (A::Mode, 8, b"AM"),
            (A::Mode, 14, b"NOT_FOUND"),
            (A::Mode, 17, b"NOT_FOUND"),
            (A::Mode, 24, b"Auto"),
            (A::Mode, 25, b"-"),
            (A::Mode, 0x1000d, b"NOT_FOUND"),
            (A::Data, 1, b"ON"),
            (A::Filter, 0, b"1"),
            (A::Filter, 255, b"-"),
            (A::Selection, 3, b"SEL3"),
            (A::Split, 1, b"ON"),
            (A::Tone, 9, b"TONE(T)/TSQL(R)"),
            (A::DtcsPolarity, 3, b"TN-RR"),
            (A::DigitalSquelch, 4, b"CSQL"),
            (A::Dtcs, 0, b"023"),
            (A::Dtcs, 103, b"754"),
        ] {
            let source = value.to_le_bytes();
            assert_eq!(
                portable_label(
                    Kind::Memory,
                    &Request {
                        adapter,
                        source: &source,
                        utc_offset_minutes: 0
                    },
                    b'.'
                )
                .unwrap()
                .unwrap(),
                expected
            );
        }
        assert_eq!(
            builtin_import_ascii(Kind::Memory, A::Mode, b" d-star "),
            Some(16)
        );
        assert_eq!(
            builtin_import_ascii(Kind::Memory, A::Duplex, b"RPS"),
            Some(3)
        );
        for (source, expected) in [
            (b"23".as_slice(), Some(0)),
            (b"023", Some(0)),
            (b"000", None),
            (b"3", None),
            (b"754", Some(103)),
        ] {
            assert_eq!(
                builtin_import_ascii(Kind::Memory, A::Dtcs, source),
                expected
            );
        }
        let mut corrections = 0;
        assert_eq!(
            tone_builtin_ascii(b"150.0Hz", false, b'.', &mut corrections).unwrap(),
            50
        );
        assert_eq!(corrections, 0);
        tone_builtin_ascii(b"105.1Hz", false, b'.', &mut corrections).unwrap();
        assert_eq!(corrections, 1);
    }
    #[test]
    fn first_collision_secondary_count_and_non_numeric_short_codes() {
        let dup = [
            Entry {
                label: b"RPS",
                value: 3,
            },
            Entry {
                label: b"RPS",
                value: 4,
            },
        ];
        assert_eq!(export(4, &dup, b"missing"), b"RPS");
        assert_eq!(import_ascii(b" rps ", &dup), Some(3));
        let primary = [
            Entry {
                label: b"CANON",
                value: 1,
            },
            Entry {
                label: b"alias",
                value: 1,
            },
        ];
        let secondary = [Entry {
            label: b"old",
            value: 1,
        }];
        let mut c = u32::MAX;
        assert_eq!(mode_ascii(b"alias", &primary, &secondary, &mut c), Some(1));
        assert_eq!(c, u32::MAX);
        assert_eq!(mode_ascii(b"OLD", &primary, &secondary, &mut c), Some(1));
        assert_eq!(c, 0);
        let dtcs = [Entry {
            label: b"023",
            value: 0,
        }];
        assert_eq!(dtcs_ascii(b"23", &dtcs), Some(0));
        assert_eq!(dtcs_ascii(b"3", &dtcs), None);
        assert_eq!(dtcs_ascii(b"000", &dtcs), None);
        assert_eq!(data_ascii(b"database", &[], &mut c), Some(1));
        assert_eq!(on_ascii(b"Onward", &[], &mut c), None);
        assert_eq!(gps_group_ascii(b"01"), 0xffff);
        assert_eq!(gps_group_ascii(b"z"), 26);
    }

    #[test]
    fn csv_tone_alias_fifty_and_crt_prefix_tie_are_separate_branches() {
        let aliases = [Entry {
            label: b"150.0Hz",
            value: 50,
        }];
        let candidates = [
            ToneCandidate {
                value: 7,
                label: b"100.0",
            },
            ToneCandidate {
                value: 2,
                label: b"110.0",
            },
            ToneCandidate {
                value: 9,
                label: b"150.0",
            },
        ];
        let mut corrections = 0;
        assert_eq!(
            tone_ascii(
                b"150,0Hz",
                &aliases,
                &candidates,
                &mut corrections,
                |_| panic!()
            )
            .unwrap(),
            50
        );
        assert_eq!(corrections, 0);
        assert_eq!(
            tone_ascii(
                b"105.1Hz",
                &aliases,
                &candidates,
                &mut corrections,
                |text| super::super::number::crt_decimal_prefix(text, b'.')
            )
            .unwrap(),
            2
        );
        assert_eq!(
            tone_ascii(
                b"105.1Hz",
                &aliases,
                &candidates,
                &mut corrections,
                |text| super::super::number::crt_decimal_prefix(text, b',')
            )
            .unwrap(),
            7
        );
        assert_eq!(corrections, 2);
        assert_eq!(
            tone_ascii(
                b"59.9Hz",
                &aliases,
                &candidates,
                &mut corrections,
                |_| panic!()
            )
            .unwrap(),
            0xffff
        );
        assert_eq!(
            tone_ascii(b"100Hz", &[], &[], &mut corrections, |text| {
                super::super::number::crt_decimal_prefix(text, b'.')
            })
            .unwrap(),
            8
        );
        assert_eq!(corrections, 3);
    }
}
