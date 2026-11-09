//! Nine payload builders plus repeater/callsign. Inputs are decoded 616-byte
//! holders, not ICF image slices. Field codecs remain owned by icf.
//!
//! Source schema: BA28 history-csv-schema-calls, BA03 scan-gps-schema, BA16
//! memory-csv-schema, BA05 repeater-export-source and the accepted correction
//! repeater-csv-17-correction.md. Three alternative branch sites are NOT cells.
//! Native label/calendar/symbol
//! outputs are requested individually, never as an opaque prebuilt CSV packet.
//! The callback is also the explicit boundary for original table-outside-memory
//! results. No missing caption is silently made empty or replaced by a default.

use super::{Error, export_call, export_text, join, number};

/// A checked decoded record (5F64); preserves unknown/residual field bytes.
#[derive(Clone, Copy)]
pub struct Holder<'a>(&'a [u8]);
impl<'a> Holder<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, Error> {
        if bytes.len() != 0x268 {
            return Err(Error::Invalid("decoded holder must have 616 bytes"));
        }
        Ok(Self(bytes))
    }
    fn valid(self) -> bool {
        self.0[12] & 1 != 0
    }
    fn b(self, at: usize) -> u8 {
        self.0[at]
    }
    fn w(self, at: usize) -> u16 {
        u16::from_le_bytes([self.0[at], self.0[at + 1]])
    }
    fn d(self, at: usize) -> u32 {
        u32::from_le_bytes([self.0[at], self.0[at + 1], self.0[at + 2], self.0[at + 3]])
    }
    fn raw(self, at: usize, n: usize) -> &'a [u8] {
        &self.0[at..at + n]
    }
    fn coord(self, at: usize) -> [u8; 4] {
        [self.b(at), self.b(at + 1), self.b(at + 2), self.b(at + 3)]
    }
}

/// Exact builder identities, not GUI context type aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Memory,
    Scan,
    Gps,
    ReceiveHistory,
    FromHistory,
    FromDataHistory,
    ToHistory,
    Reflector,
    UserEdge,
    Repeater,
    Callsign,
}
/// Individually observable runtime/resource/RTL boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adapter {
    Selection,
    Duplex,
    Mode,
    Data,
    Filter,
    Tone,
    ToneFrequency,
    Dtcs,
    DtcsPolarity,
    DigitalSquelch,
    Split,
    Group,
    Alarm,
    RepeaterDuplex,
    RepeaterMode,
    RepeaterUse,
    Position,
    UtcOffsetSentinel,
    RxType,
    RxStatus,
    Symbol,
    Directivity,
    DataType,
    GpsDate,
    GpsTime,
    HistoryDate,
    Timestamp,
    Message,
    GpsMessage,
}

/// Captured inputs at the actual builder boundary. Distinct live reads must be
/// supplied separately by the caller instead of implicitly reading host state.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    pub separator: u8,
    pub decimal: u8,
    pub row: u32,
    pub number_base: u32,
    pub group: u32,
    pub group_name: &'a [u8],
    pub alarm: bool,
    pub utc_offset_minutes: i16,
    pub memory_group_columns: bool,
    pub memory_selection_column: bool,
    pub memory_pair: bool,
}

/// A request identifies exact source bytes and source kind. Scalar data is
/// little-endian at its original width; raw datetime/coordinate/text is borrowed.
pub struct Request<'a> {
    pub adapter: Adapter,
    pub source: &'a [u8],
    pub utc_offset_minutes: i16,
}

fn number(n: u32) -> Vec<u8> {
    format!("{:02}", n as i32).into_bytes()
}
fn one_decimal(n: u32, point: u8, sign: bool) -> Vec<u8> {
    let mut out = format!("{}{}.{}", if sign { "-" } else { "" }, n / 10, n % 10).into_bytes();
    if let Some(p) = out.iter_mut().find(|b| **b == b'.') {
        *p = point;
    }
    out
}
fn scalar(n: u32, sentinel: u32) -> Vec<u8> {
    if n == sentinel {
        Vec::new()
    } else {
        n.to_string().into_bytes()
    }
}

/// Build one logical row. A provider failure leaves caller output unassigned;
/// provider side effects and previously written rows are not rolled back.
pub fn build(
    kind: Kind,
    h: Holder<'_>,
    c: Context<'_>,
    mut native: impl FnMut(Request<'_>) -> Result<Vec<u8>, Error>,
) -> Result<Vec<u8>, Error> {
    let mut cells = Vec::new();
    let valid = h.valid();
    let mut adapter = |a, source: &[u8]| {
        native(Request {
            adapter: a,
            source,
            utc_offset_minutes: c.utc_offset_minutes,
        })
    };
    macro_rules! a {
        ($a:ident,$at:expr,$n:expr) => {
            adapter(Adapter::$a, h.raw($at, $n))?
        };
    }
    macro_rules! text {
        ($at:expr,$n:expr) => {
            export_text(h.raw($at, $n), $n)
        };
    }
    macro_rules! call {
        ($at:expr,$n:expr) => {
            export_call(h.raw($at, $n), $n)
        };
    }
    macro_rules! hz {
        ($at:expr) => {
            number::frequency_text(h.d($at), c.decimal)
        };
    }
    match kind {
        Kind::Memory => {
            // 8B5F8 normalizes only this local low-byte missing-value gate.
            // Mode's provider below must still receive the original fullDWORD.
            let gate_mode = match h.b(0x58) {
                9 => 8,
                14 => 13,
                mode => mode,
            };
            let prefix = !c.memory_pair || h.b(0x24) == 0;
            if prefix {
                if c.memory_group_columns {
                    cells.push(number(c.group));
                    cells.push(export_text(h.raw(0x0d, 16), 16));
                }
                let row = if c.memory_pair {
                    (c.row as i32 / 2) as u32
                } else {
                    c.row
                };
                cells.push(number(row.wrapping_add(c.number_base)));
                cells.push(if valid { text!(0x26, 16) } else { Vec::new() });
                if c.memory_selection_column {
                    cells.push(if valid {
                        a!(Selection, 0x54, 4)
                    } else {
                        Vec::new()
                    });
                }
            }
            cells.extend(if valid {
                vec![
                    hz!(0x3c),
                    a!(Duplex, 0x44, 4),
                    hz!(0x48),
                    a!(Mode, 0x58, 4),
                    a!(Data, 0x5c, 4),
                    a!(Filter, 0x60, 4),
                ]
            } else {
                vec![Vec::new(); 6]
            });
            if valid && gate_mode == 0x0d {
                cells.extend([
                    a!(Tone, 0x68, 4),
                    a!(ToneFrequency, 0x6c, 4),
                    a!(ToneFrequency, 0x70, 4),
                    a!(Dtcs, 0x74, 4),
                    a!(DtcsPolarity, 0x78, 4),
                ]);
            } else {
                cells.extend(vec![Vec::new(); 5]);
            }
            if valid && gate_mode == 0x10 {
                cells.extend([
                    a!(DigitalSquelch, 0x80, 4),
                    h.d(0x84).to_string().into_bytes(),
                    call!(0x88, 8),
                    call!(0xa4, 8),
                    call!(0xc4, 8),
                ]);
            } else {
                cells.extend(vec![Vec::new(); 5]);
            }
            if c.memory_pair && h.b(0x24) == 0 {
                cells.push(if valid {
                    a!(Split, 0x25, 1)
                } else {
                    Vec::new()
                });
            }
            let mut out = join(&cells, c.separator, !c.memory_pair || h.b(0x24) != 0)?;
            if c.memory_pair && h.b(0x24) != 0 {
                out.insert(0, c.separator);
            }
            return Ok(out);
        }
        Kind::Scan => {
            cells.push(number(c.row.wrapping_add(c.number_base)));
            cells.extend(if valid {
                vec![
                    text!(0x26, 16),
                    hz!(0x138),
                    hz!(0x3c),
                    a!(Mode, 0x58, 4),
                    a!(Data, 0x5c, 4),
                    a!(Filter, 0x60, 4),
                ]
            } else {
                vec![Vec::new(); 6]
            });
        }
        Kind::UserEdge => {
            cells.push(number(c.row.wrapping_add(c.number_base)));
            cells.extend(if valid {
                vec![hz!(0x138), hz!(0x3c)]
            } else {
                vec![Vec::new(); 2]
            });
        }
        Kind::Gps => {
            cells.push(adapter(Adapter::Group, &c.group.to_le_bytes())?);
            cells.push(if c.group == 0 {
                Vec::new()
            } else {
                export_text(c.group_name, 16)
            });
            cells.extend([
                text!(0x244, 16),
                a!(GpsDate, 0x254, 6),
                a!(GpsTime, 0x254, 6),
                number::coordinate_text(h.coord(0x25a), true, c.decimal),
                number::coordinate_text(h.coord(0x25e), false, c.decimal),
                number::altitude_text(h.d(0x264) as i32, c.decimal),
                adapter(Adapter::Alarm, &[u8::from(c.alarm)])?,
            ]);
        }
        Kind::FromHistory | Kind::FromDataHistory | Kind::ToHistory | Kind::Reflector => {
            cells.push(if valid { call!(0x18c, 8) } else { Vec::new() });
            if kind != Kind::Reflector {
                cells.push(if valid && h.b(0x194) != 0 {
                    text!(0x26, 16)
                } else {
                    Vec::new()
                });
            }
            if matches!(kind, Kind::FromHistory | Kind::FromDataHistory) {
                cells.push(if valid && h.b(0x194) != 0 && h.d(0x3c) != 0 {
                    hz!(0x3c)
                } else {
                    Vec::new()
                });
            }
            if kind == Kind::FromHistory {
                cells.extend(if valid && h.b(0x194) != 0 {
                    vec![a!(Duplex, 0x44, 4), hz!(0x48)]
                } else {
                    vec![Vec::new(); 2]
                });
            }
        }
        Kind::Callsign => {
            cells.extend([
                number(c.row.wrapping_add(c.number_base)),
                text!(0x26, 16),
                call!(0x88, 8),
            ]);
        }
        Kind::Repeater => {
            if !valid {
                cells.resize(17, Vec::new());
            } else {
                let g = c.group.wrapping_add(1);
                // S erratum: static sites8/14/15 are empty alternatives to
                // Frequency/Tone/RepeaterTone. Every path has exactly17 cells.
                cells.extend([
                    if g <= 50 {
                        g.to_string().into_bytes()
                    } else {
                        Vec::new()
                    },
                    export_text(c.group_name, 16),
                    text!(0x26, 16),
                    text!(0x154, 8),
                    call!(0x160, 8),
                    call!(0x168, 8),
                    if h.d(0x3c) == 0 {
                        Vec::new()
                    } else {
                        hz!(0x3c)
                    },
                    a!(RepeaterDuplex, 0x44, 4),
                    hz!(0x48),
                    a!(RepeaterMode, 0x15c, 4),
                    if h.d(0x15c) <= 1 {
                        a!(Tone, 0x174, 4)
                    } else {
                        Vec::new()
                    },
                    if h.d(0x15c) <= 1 {
                        a!(ToneFrequency, 0x178, 4)
                    } else {
                        Vec::new()
                    },
                    a!(RepeaterUse, 0x17c, 4),
                    a!(Position, 0x180, 4),
                    number::coordinate_text(h.coord(0x25a), true, c.decimal),
                    number::coordinate_text(h.coord(0x25e), false, c.decimal),
                    if h.w(0x184) == 0x7fff {
                        a!(UtcOffsetSentinel, 0x184, 2)
                    } else {
                        utc_text(h.d(0x184) as i16 as i32)?
                    },
                ]);
            }
        }
        Kind::ReceiveHistory => {
            if !valid {
                cells.resize(37, Vec::new());
            } else {
                cells.extend([
                    a!(RxType, 0x195, 1),
                    hz!(0x198),
                    a!(Mode, 0x19c, 1),
                    call!(0x19d, 8),
                    call!(0x1a5, 4),
                    call!(0x1a9, 8),
                    call!(0x1b1, 8),
                    call!(0x1b9, 8),
                    a!(Message, 0x1c1, 20),
                    a!(RxStatus, 0x1d5, 1),
                    a!(HistoryDate, 0x1d8, 6),
                    if h.b(0x1d6) == 0 {
                        Vec::new()
                    } else {
                        b"*".to_vec()
                    },
                    if h.b(0x1d7) == 0 {
                        Vec::new()
                    } else {
                        b"*".to_vec()
                    },
                    number::coordinate_text(h.coord(0x1de), true, c.decimal),
                    number::coordinate_text(h.coord(0x1e2), false, c.decimal),
                    number::altitude_text(h.d(0x1e8) as i32, c.decimal),
                    ssid([h.b(0x1ec), h.b(0x1ed)]),
                    a!(Symbol, 0x1ee, 2),
                    scalar(u32::from(h.w(0x1f0)), 65535),
                    speed(h.w(0x1f2), c.decimal),
                ]);
                let power = h.b(0x1f8);
                cells.push(if power == 0xff {
                    Vec::new()
                } else if power < 10 {
                    (u32::from(power).pow(2)).to_string().into_bytes()
                } else {
                    return Err(Error::SourceUnavailable(
                        "PHG power table outside normal range",
                    ));
                });
                let height = h.b(0x1f9);
                cells.push(if height == 0xff {
                    Vec::new()
                } else if height < 10 {
                    ((3048_u32 * (1_u32 << height) + 500) / 1000)
                        .to_string()
                        .into_bytes()
                } else {
                    return Err(Error::SourceUnavailable(
                        "PHG height table outside normal range",
                    ));
                });
                cells.extend([
                    match h.b(0x1fa) {
                        0xff => Vec::new(),
                        n @ 0..=9 => n.to_string().into_bytes(),
                        _ => {
                            return Err(Error::SourceUnavailable(
                                "PHG gain table outside normal range",
                            ));
                        }
                    },
                    a!(Directivity, 0x1fb, 1),
                    export_text(h.raw(0x1fc, 9), 9),
                    a!(DataType, 0x205, 1),
                    temperature(h.w(0x206) as i16, c.decimal),
                    rain(h.w(0x208), c.decimal),
                    rain(h.w(0x20a), c.decimal),
                    rain(h.w(0x20c), c.decimal),
                    scalar(u32::from(h.w(0x20e)), 65535),
                    wind(h.w(0x210), c.decimal),
                    wind(h.w(0x212), c.decimal),
                    pressure(h.d(0x214), c.decimal),
                    if matches!(h.b(0x218), 0 | 255) {
                        Vec::new()
                    } else {
                        h.b(0x218).to_string().into_bytes()
                    },
                    a!(Timestamp, 0x1f5, 3),
                    a!(GpsMessage, 0x219, 43),
                ]);
            }
        }
    }
    join(&cells, c.separator, true)
}

/// Portable static-label profile. Only calendar, overlay and not-yet-modeled
/// profiles reach runtime; the raw builder remains available for explicit live
/// resources without silently overriding them with a built-in profile.
pub fn build_portable(
    kind: Kind,
    h: Holder<'_>,
    c: Context<'_>,
    mut runtime: impl FnMut(Request<'_>) -> Result<Vec<u8>, Error>,
) -> Result<Vec<u8>, Error> {
    build(kind, h, c, |request| {
        match super::table::portable_label(kind, &request, c.decimal)? {
            Some(label) => Ok(label),
            None => runtime(request),
        }
    })
}

/// 8E37C reads live name/call arrays directly for the same row. Shared type0D
/// projection is only a bridge to the builder; header/validity is not a gate.
pub fn callsign_row(
    image: &crate::icf::Icf<'_>,
    index: usize,
    c: Context<'_>,
) -> Result<Vec<u8>, Error> {
    if index >= 300 {
        return Err(Error::Invalid("CSV callsign row outside 0..300"));
    }
    let memory = image.memory();
    let name: &[u8; 16] = memory[0x2f703 + 16 * index..0x2f703 + 16 * (index + 1)]
        .try_into()
        .expect("UR name width");
    let call: &[u8; 8] = memory[0x2eda3 + 8 * index..0x2eda3 + 8 * (index + 1)]
        .try_into()
        .expect("UR call width");
    let mut holder = crate::icf::record::reset_holder(0x0d, index as u32, 0);
    crate::icf::record::decode_callsign_into(&mut holder, name, call);
    build(
        Kind::Callsign,
        Holder::new(holder.as_bytes())?,
        Context {
            row: index as u32,
            ..c
        },
        |_| Err(Error::Invalid("unexpected callsign formatter request")),
    )
}

/// B9990's signed-minute formatter, including the 7FFF literal and hour low8.
/// Other raw values are not clamped to canonical setter bounds.
pub fn utc_text(minutes: i32) -> Result<Vec<u8>, Error> {
    if minutes == 0x7fff {
        return Ok(b"--:--".to_vec());
    }
    let n = minutes.unsigned_abs();
    Ok(format!(
        "{}{}:{:02}",
        if minutes > 0 {
            "+"
        } else if minutes < 0 {
            "-"
        } else {
            ""
        },
        (n / 60) as u8,
        n % 60
    )
    .into_bytes())
}
fn ssid(raw: [u8; 2]) -> Vec<u8> {
    match raw {
        [255, 255] => Vec::new(),
        [b'0', 255] => b"(-0)".to_vec(),
        [a, 255] => vec![a],
        [a, b] => vec![a, b],
    }
}
fn speed(n: u16, point: u8) -> Vec<u8> {
    if n == 65535 {
        Vec::new()
    } else {
        one_decimal((u32::from(n) * 1852 / 1000) as u16 as u32, point, false)
    }
}
fn temperature(n: i16, point: u8) -> Vec<u8> {
    if n == 32767 {
        return Vec::new();
    }
    let n32 = i32::from(n);
    let c = (((n32 - 32) * 500 / 9 + if n >= 32 { 5 } else { -5 }) / 10) as i16;
    let mut out = one_decimal(u32::from(c.unsigned_abs()), point, c < 0);
    if c >= 0 {
        out.insert(0, b' ');
    }
    out
}
fn rain(n: u16, point: u8) -> Vec<u8> {
    if n == 65535 {
        Vec::new()
    } else {
        one_decimal(
            ((u32::from(n) * 254 + 50) / 100) as u16 as u32,
            point,
            false,
        )
    }
}
fn wind(n: u16, point: u8) -> Vec<u8> {
    if n == 65535 {
        Vec::new()
    } else {
        one_decimal(
            ((447 * u32::from(n) + 50) / 100) as u16 as u32,
            point,
            false,
        )
    }
}
fn pressure(n: u32, point: u8) -> Vec<u8> {
    if n == u32::MAX {
        Vec::new()
    } else {
        one_decimal(n, point, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callsign_live_arrays_and_portable_labels_do_not_use_history_or_type_guards() {
        let mut raw = [0; 616];
        raw[..12].fill(0xff);
        raw[0x26..0x36].fill(b' ');
        raw[0x26..0x2a].copy_from_slice(b"NAME");
        raw[0x88..0x90].copy_from_slice(b"RIGHT   ");
        raw[0x18c..0x194].copy_from_slice(b"WRONG   ");
        assert_eq!(
            build(
                Kind::Callsign,
                Holder::new(&raw).unwrap(),
                Context {
                    row: 0,
                    ..context()
                },
                |_| panic!()
            )
            .unwrap(),
            b"00;NAME;RIGHT\r\n"
        );
        let mut memory = vec![0; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        let name = 0x2f703 + 16 * 299;
        let call = 0x2eda3 + 8 * 299;
        memory[name..name + 16].fill(b' ');
        memory[name..name + 4].copy_from_slice(b"LIVE");
        memory[call..call + 8].copy_from_slice(b"SAMEGEN ");
        let image = crate::icf::Icf::from_memory(memory).unwrap();
        assert_eq!(
            callsign_row(&image, 299, context()).unwrap(),
            b"299;LIVE;SAMEGEN\r\n"
        );
        assert_eq!(utc_text(32767).unwrap(), b"--:--");
        assert_eq!(utc_text(32766).unwrap(), b"+34:06");
        let request = Request {
            adapter: Adapter::ToneFrequency,
            source: &50_u32.to_le_bytes(),
            utc_offset_minutes: 0,
        };
        assert_eq!(
            super::super::table::portable_label(Kind::Repeater, &request, b',')
                .unwrap()
                .unwrap(),
            b"150,0Hz"
        );
    }
    fn context() -> Context<'static> {
        Context {
            separator: b';',
            decimal: b',',
            row: 101,
            number_base: 0,
            group: 4,
            group_name: b"G",
            alarm: false,
            utc_offset_minutes: 0,
            memory_group_columns: false,
            memory_selection_column: false,
            memory_pair: false,
        }
    }
    #[test]
    fn nine_schemas_keep_invalid_frames_and_number_wrap() {
        let bytes = [0; 616];
        let h = Holder::new(&bytes).unwrap();
        for (kind, expected) in [
            (Kind::Scan, b"101;;;;;;\r\n".as_slice()),
            (Kind::UserEdge, b"101;;\r\n"),
            (Kind::FromHistory, b";;;;\r\n"),
            (Kind::FromDataHistory, b";;\r\n"),
            (Kind::ToHistory, b";\r\n"),
            (Kind::Reflector, b"\r\n"),
        ] {
            assert_eq!(
                build(kind, h, context(), |_| panic!(
                    "invalid must not request native output"
                ))
                .unwrap(),
                expected
            );
        }
        for (kind, n) in [(Kind::ReceiveHistory, 36), (Kind::Repeater, 16)] {
            let out = build(kind, h, context(), |_| panic!()).unwrap();
            assert_eq!(out.iter().filter(|b| **b == b';').count(), n);
            assert!(out.ends_with(b"\r\n"));
        }
        let mut c = context();
        c.row = u32::MAX;
        assert_eq!(
            build(Kind::UserEdge, h, c, |_| panic!()).unwrap(),
            b"-1;;\r\n"
        );
    }
    #[test]
    fn from_history_guard_and_distinct_group_source() {
        let mut bytes = [b' '; 616];
        bytes[12] = 1;
        bytes[0x194] = 0;
        bytes[0x18c..0x194].copy_from_slice(b"A,B;C   ");
        let out = build(
            Kind::FromHistory,
            Holder::new(&bytes).unwrap(),
            context(),
            |_| panic!(),
        )
        .unwrap();
        assert_eq!(out, b"A,B;C;;;;\r\n");
        bytes[0x194] = 1;
        bytes[0x3c..0x40].copy_from_slice(&145000000_u32.to_le_bytes());
        bytes[0x48..0x4c].fill(0);
        let out = build(
            Kind::FromHistory,
            Holder::new(&bytes).unwrap(),
            context(),
            |r| {
                assert_eq!(r.adapter, Adapter::Duplex);
                Ok(b"RPS".to_vec())
            },
        )
        .unwrap();
        assert_eq!(out, b"A,B;C;;145,000000;RPS;0,000000\r\n");
        assert!(Holder::new(&bytes[..615]).is_err());
        assert_eq!(speed(65534, b'.'), b"5583.2");
        assert_eq!(temperature(32766, b'.'), b"-1475.2");
        assert_eq!(rain(65534, b'.'), b"3538.4");
    }

    #[test]
    fn repeater_erratum_seventeen_cells_on_all_frequency_mode_valid_paths() {
        let mut bytes = [0; 616];
        bytes[12] = 1;
        bytes[0x26..0x36].copy_from_slice(b"NAME            ");
        bytes[0x154..0x15c].copy_from_slice(b"SUB     ");
        bytes[0x160..0x168].copy_from_slice(b"CALL    ");
        bytes[0x168..0x170].copy_from_slice(b"GATE    ");
        bytes[0x48..0x4c].copy_from_slice(&600000_u32.to_le_bytes());
        bytes[0x25d] = b'N';
        bytes[0x261] = b'E';
        bytes[0x184..0x188].copy_from_slice(&60_u32.to_le_bytes());
        let mut c = context();
        c.separator = b',';
        c.decimal = b'.';
        for valid in [false, true] {
            for hz in [0, 145000000_u32] {
                for mode in [0, 1, 2, 3, 0x100_u32] {
                    bytes[12] = u8::from(valid);
                    bytes[0x3c..0x40].copy_from_slice(&hz.to_le_bytes());
                    bytes[0x15c..0x160].copy_from_slice(&mode.to_le_bytes());
                    let mut tone_calls = 0;
                    let out = build(Kind::Repeater, Holder::new(&bytes).unwrap(), c, |request| {
                        Ok(match request.adapter {
                            Adapter::RepeaterDuplex => b"DUP-".to_vec(),
                            Adapter::RepeaterMode => match mode {
                                0 => b"FM".to_vec(),
                                1 => b"FM-N".to_vec(),
                                2 => b"DV".to_vec(),
                                3 => b"DD".to_vec(),
                                _ => b"UNKNOWN".to_vec(),
                            },
                            Adapter::Tone => {
                                tone_calls += 1;
                                b"TONE".to_vec()
                            }
                            Adapter::ToneFrequency => {
                                tone_calls += 1;
                                b"88.5Hz".to_vec()
                            }
                            Adapter::RepeaterUse => b"YES".to_vec(),
                            Adapter::Position => b"Approximate".to_vec(),
                            _ => panic!("unexpected producer"),
                        })
                    })
                    .unwrap();
                    assert_eq!(out.iter().filter(|b| **b == b',').count(), 16);
                    let cells = crate::csv::split_cells(&out[..out.len() - 2], b',');
                    assert_eq!(cells.len(), 17);
                    if !valid {
                        assert_eq!(out, b",,,,,,,,,,,,,,,,\r\n");
                        assert_eq!(tone_calls, 0);
                        continue;
                    }
                    assert_eq!(cells[0], b"5");
                    assert_eq!(
                        cells[6],
                        if hz == 0 {
                            b"".as_slice()
                        } else {
                            b"145.000000"
                        }
                    );
                    assert_eq!(cells[10], if mode <= 1 { b"TONE".as_slice() } else { b"" });
                    assert_eq!(
                        cells[11],
                        if mode <= 1 { b"88.5Hz".as_slice() } else { b"" }
                    );
                    assert_eq!(tone_calls, if mode <= 1 { 2 } else { 0 });
                    if hz != 0 && mode == 0 {
                        assert_eq!(out,b"5,G,NAME,SUB,CALL,GATE,145.000000,DUP-,0.600000,FM,TONE,88.5Hz,YES,Approximate,0.000000,0.000000,+1:00\r\n");
                    }
                }
            }
        }
    }

    #[test]
    fn memory_gate_remaps_low_byte_without_changing_full_mode_or_split_boundaries() {
        let mut bytes = [0; 616];
        bytes[12] = 1;
        for (mode, fm, dv) in [
            (0x0d, true, false),
            (0x0e, true, false),
            (0x10, false, true),
            (0x11, false, false),
            (0x1000d, true, false),
            (0x10010, false, true),
            (9, false, false),
        ] {
            let mode: u32 = mode;
            bytes[0x58..0x5c].copy_from_slice(&mode.to_le_bytes());
            let output = build(Kind::Memory, Holder::new(&bytes).unwrap(), context(), |r| {
                if r.adapter == Adapter::Mode {
                    assert_eq!(r.source, &mode.to_le_bytes());
                }
                Ok(format!("{:?}", r.adapter).into_bytes())
            })
            .unwrap();
            let cells = crate::csv::split_cells(&output[..output.len() - 2], b';');
            assert_eq!(cells.len(), 18);
            assert_eq!(!cells[8].is_empty(), fm);
            assert_eq!(!cells[13].is_empty(), dv);
        }
        bytes[12] = 0;
        let invalid = build(
            Kind::Memory,
            Holder::new(&bytes).unwrap(),
            context(),
            |_| panic!(),
        )
        .unwrap();
        assert_eq!(
            crate::csv::split_cells(&invalid[..invalid.len() - 2], b';').len(),
            18
        );
        bytes[12] = 1;
        bytes[0x58..0x5c].copy_from_slice(&0x0e_u32.to_le_bytes());
        let mut c = context();
        c.memory_pair = true;
        let base = build(Kind::Memory, Holder::new(&bytes).unwrap(), c, |_| {
            Ok(b"X".to_vec())
        })
        .unwrap();
        assert!(!base.ends_with(b";"));
        assert!(!base.ends_with(b"\r\n"));
        bytes[0x24] = 1;
        let other = build(Kind::Memory, Holder::new(&bytes).unwrap(), c, |_| {
            Ok(b"X".to_vec())
        })
        .unwrap();
        assert_eq!(other[0], b';');
        assert!(other.ends_with(b"\r\n"));
    }
}
