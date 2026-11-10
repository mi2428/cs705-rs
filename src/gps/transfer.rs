//! GPS 8451C/8409C image transfer. Physical 34 -> decoded 616 projection is field
//! movement only; all ICF bit/enum codecs remain in core. Native date/time text
//! is supplied per field/read, not silently replaced with ISO or host locale.
//! Import consumes finalized decoded rows, after clearing the target dataset.

use super::operations::{self, Record};
use crate::csv::{self, control::Counts, payload};
use crate::icf::record::{DecodedRecord, decode_gps_into};
use crate::icf::{Error as ImageError, Icf};

const ALARM: usize = 0x3c622;
const NAMES: usize = 0x353f1;

/// Selected group or the actual whole dataset, not the current GUI cell range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Whole,
    Group(usize),
}
/// Text and image failures are kept distinct while sharing typed report state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Text(csv::Error),
    Image(ImageError),
}
impl From<ImageError> for Failure {
    fn from(e: ImageError) -> Self {
        Self::Image(e)
    }
}

/// Original prefix is returned even after a later native formatter failure.
#[derive(Debug)]
pub struct Export {
    pub bytes: Vec<u8>,
    pub rows: usize,
    pub failure: Option<Failure>,
}

/// Export all/current-group rows. No invalid synthetic row is made for an empty
/// group. Locale bytes are copied CSV state; formatter receives live offset.
pub fn export_csv(
    image: &Icf<'_>,
    scope: Scope,
    locale: csv::Locale,
    mut native: impl FnMut(payload::Request<'_>) -> Result<Vec<u8>, csv::Error>,
) -> Export {
    let header = [
        b"Group".to_vec(),
        b"Group Name".to_vec(),
        b"Name".to_vec(),
        b"Date".to_vec(),
        b"Time".to_vec(),
        b"Latitude".to_vec(),
        b"Longitude".to_vec(),
        b"Altitude".to_vec(),
        b"Alarm".to_vec(),
    ];
    let header = match csv::join(&header, locale.list, true) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Export {
                bytes: Vec::new(),
                rows: 0,
                failure: Some(Failure::Text(error)),
            };
        }
    };
    let mut out = Export {
        bytes: header,
        rows: 0,
        failure: None,
    };
    let result = (|| -> Result<(), Failure> {
        if let Scope::Group(g) = scope
            && g >= 27
        {
            return Err(Failure::Text(csv::Error::Invalid(
                "GPS export group outside 0..27",
            )));
        }
        let groups = image.gps_groups()?;
        let mut absolute = 0;
        for (g, group) in groups.iter().enumerate() {
            for (relative, &p) in group.physical_slots().iter().enumerate() {
                if matches!(scope,Scope::Group(selected)if selected!=g) {
                    absolute += 1;
                    continue;
                }
                let raw = image.physical_gps_record_raw(p)?.ok_or(Failure::Text(
                    csv::Error::SourceUnavailable("GPS exported physical record disappeared"),
                ))?;
                let raw: &[u8; 34] = raw.try_into().map_err(|_| {
                    Failure::Text(csv::Error::Invalid(
                        "GPS physical record must have 34 bytes",
                    ))
                })?;
                let mut h = DecodedRecord::from_bytes([0; 616]);
                h.set_valid(true);
                decode_gps_into(&mut h, raw, g as u32);
                let memory = image.memory();
                let offset = i16::from_le_bytes([memory[0x396ff], memory[0x39700]]);
                let alarm = memory[ALARM] == 3
                    && usize::from(memory[ALARM + 1]) == g
                    && usize::from(u16::from_le_bytes([memory[ALARM + 2], memory[ALARM + 3]]))
                        == relative;
                let c = payload::Context {
                    separator: locale.list,
                    decimal: locale.decimal,
                    row: absolute as u32,
                    number_base: 0,
                    group: g as u32,
                    group_name: group.name_bytes().unwrap_or(b""),
                    alarm,
                    utc_offset_minutes: offset,
                    memory_group_columns: false,
                    memory_selection_column: false,
                    memory_pair: false,
                };
                let bytes = payload::build_portable(
                    payload::Kind::Gps,
                    payload::Holder::new(h.as_bytes()).map_err(Failure::Text)?,
                    c,
                    |r| {
                        // Date and Time read the live offset independently.
                        let memory = image.memory();
                        native(payload::Request {
                            utc_offset_minutes: i16::from_le_bytes([
                                memory[0x396ff],
                                memory[0x39700],
                            ]),
                            ..r
                        })
                    },
                )
                .map_err(Failure::Text)?;
                out.bytes.extend_from_slice(&bytes);
                out.rows += 1;
                absolute += 1;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        out.failure = Some(error);
    }
    out
}

/// Portable finite-locale formatter variant. Native era/textual date tokens and
/// failed TryEncodeTime's retained value remain explicit inputs. This stable
/// context variant does not claim arbitrary reentrant locale/clock equivalence.
pub fn export_csv_formatted(
    image: &Icf<'_>,
    scope: Scope,
    locale: csv::Locale,
    context: &csv::datetime::Context<'_>,
    failed_time: csv::datetime::Time,
    mut native: impl FnMut(csv::datetime::DateToken) -> Result<Vec<u8>, csv::Error>,
) -> Export {
    export_csv(image, scope, locale, |r| {
        let raw: [u8; 6] = r
            .source
            .try_into()
            .map_err(|_| csv::Error::Invalid("GPS datetime source width"))?;
        match r.adapter {
            payload::Adapter::GpsDate => csv::datetime::gps_date(
                raw,
                r.utc_offset_minutes,
                context,
                failed_time,
                &mut native,
            ),
            payload::Adapter::GpsTime => {
                csv::datetime::gps_time(raw, r.utc_offset_minutes, context, failed_time)
            }
            _ => Err(csv::Error::Invalid("unexpected GPS formatter request")),
        }
    })
}

/// Successful reader/finalizer output. `valid=false` is a rejected reader row;
/// corrections and alarm are written only for a valid row by the parser seam.
#[derive(Debug, Clone, Copy)]
pub struct Parsed {
    pub record: Record,
    pub group: usize,
    pub group_name: [u8; 16],
    pub corrections: u32,
    pub alarm: bool,
}
/// Four ABI counts plus actual applied prefix; neither is inferred from the other.
#[derive(Debug)]
pub struct Import {
    pub counts: Counts,
    pub rows_read: usize,
    pub applied: usize,
    pub failure: Option<Failure>,
}

/// 8409C decoded root. The iterator is first advanced AFTER target deletion/name
/// clear, so parser/native failure cannot restore old rows. Capacity overwrites
/// C4=1 and stops reading; source-invalid rows increment C2. No stage9 is run.
pub fn import_decoded(
    image: &mut Icf<'_>,
    scope: Scope,
    rows: impl IntoIterator<Item = Result<Parsed, csv::Error>>,
    alarm_seen: bool,
) -> Import {
    let mut rows = rows.into_iter();
    import_rows(image, scope, |_| rows.next(), alarm_seen)
}

/// Real CSV byte -> value -> 83FDC finalized row -> image path. Header failure
/// precedes target deletion; advancing rows/finalizers follows the deletion.
pub fn import_csv(
    image: &mut Icf<'_>,
    scope: Scope,
    source: &[u8],
    options: super::parser::Options<'_>,
    alarm_seen: bool,
) -> Import {
    let mut rows = match super::parser::Rows::new(source, options) {
        Ok(rows) => rows,
        Err(error) => {
            return Import {
                counts: Counts::default(),
                rows_read: 0,
                applied: 0,
                failure: Some(Failure::Text(error)),
            };
        }
    };
    let mut reader_alarm_seen = alarm_seen;
    import_rows(
        image,
        scope,
        |image| {
            rows.next().map(|draft| {
                let mut draft = draft?;
                let offset = i16::from_le_bytes([image.memory()[0x396ff], image.memory()[0x39700]]);
                draft.finalize(
                    matches!(scope, Scope::Whole),
                    offset,
                    &mut reader_alarm_seen,
                )
            })
        },
        alarm_seen,
    )
}

fn import_rows(
    image: &mut Icf<'_>,
    scope: Scope,
    mut next: impl FnMut(&Icf<'_>) -> Option<Result<Parsed, csv::Error>>,
    mut alarm_seen: bool,
) -> Import {
    let mut out = Import {
        counts: Counts::default(),
        rows_read: 0,
        applied: 0,
        failure: None,
    };
    let result = (|| -> Result<(), Failure> {
        let groups: Vec<usize> = match scope {
            Scope::Whole => (0..27).collect(),
            Scope::Group(g) if g < 27 => vec![g],
            _ => {
                return Err(Failure::Text(csv::Error::Invalid(
                    "GPS import group outside 0..27",
                )));
            }
        };
        for g in groups {
            let (_, count) = operations::range(image, g)?;
            for _ in 0..count {
                operations::delete_one(image, g, 0)?;
            }
            if g != 0 {
                image.patch_memory(&[(NAMES + 16 * (g - 1), &[b' '; 16])])?;
            }
        }
        while let Some(row) = next(image) {
            out.rows_read += 1;
            let row = row.map_err(Failure::Text)?;
            if !row.record.valid {
                out.counts.c2 = out.counts.c2.wrapping_add(1);
                continue;
            }
            let g = match scope {
                Scope::Whole => row.group,
                Scope::Group(g) => g,
            };
            let (start, count) = operations::range(image, g)?;
            let total: usize = image
                .gps_groups()?
                .iter()
                .map(|g| g.physical_slots().len())
                .sum();
            if (total as u16) >= 300 {
                let status = if matches!(scope, Scope::Group(_)) && (count as u16) < 300 {
                    1
                } else {
                    2
                };
                out.counts.capacity_marker(status);
                break;
            }
            if !operations::allocate(image, g, count)? {
                return Err(Failure::Text(csv::Error::Operation(
                    "GPS capacity changed before insert",
                )));
            }
            operations::save(image, start + count, row.record)?;
            out.applied += 1;
            let mut corrections = row.corrections;
            if row.alarm {
                if alarm_seen {
                    corrections = corrections.wrapping_add(1);
                } else {
                    let q = (count as u16).to_le_bytes();
                    image.patch_memory(&[(ALARM, &[3, g as u8, q[0], q[1]])])?;
                    alarm_seen = true;
                }
            }
            out.counts.c3 = out.counts.c3.wrapping_add(corrections);
            out.counts.c4 = out.counts.c4.wrapping_add(1);
            if g != 0 {
                let at = NAMES + 16 * (g - 1);
                if image.memory()[at..at + 16] == [b' '; 16] {
                    image.patch_memory(&[(at, &row.group_name)])?;
                }
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        out.failure = Some(error);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> Icf<'static> {
        let mut m = vec![0; 0x3c9c0];
        m[0x355a1] = 6;
        m[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        m[0x3c385..0x3c5dd].fill(255);
        m[0x3c5dd..0x3c613].fill(255);
        Icf::from_memory(m).unwrap()
    }
    fn parsed(name: u8) -> Parsed {
        let mut bytes = [0; 34];
        bytes[3] = b'N';
        bytes[7] = b'E';
        bytes[8..14].fill(0x99);
        bytes[14..18].copy_from_slice(&i32::MAX.to_le_bytes());
        bytes[18..].fill(name);
        Parsed {
            record: Record { valid: true, bytes },
            group: 1,
            group_name: [b'G'; 16],
            corrections: 0,
            alarm: false,
        }
    }
    #[test]
    fn csv_bytes_finalize_before_image_apply_and_header_failure_does_not_clear() {
        let mut image = image();
        image
            .patch_memory(&[(0x396ff, &60_i16.to_le_bytes())])
            .unwrap();
        let options = super::super::parser::Options {
            locale: csv::Locale::copied(b'.', b';'),
            datetime: csv::datetime::Context {
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
            },
            hemispheres: *b"NE",
            group_name_seed: [b' '; 16],
        };
        let input=b"Group;Group Name;Name;Date;Time;Latitude;Longitude;Altitude;Alarm\nA;G;FIRST;2000/01/02;0:30:00;0;0;1;ON\nA;G;BAD;;; ;0;;ON\nA;G;12345678901234567;;;0;0;;ON\n";
        let out = import_csv(&mut image, Scope::Whole, input, options.clone(), false);
        assert!(out.failure.is_none());
        assert_eq!(
            (
                out.rows_read,
                out.applied,
                out.counts.c2,
                out.counts.c3,
                out.counts.c4
            ),
            (3, 2, 1, 3, 2)
        );
        let rows = operations::copy(&image, 1, 0, 1).unwrap();
        assert_eq!(&rows[0].bytes[8..14], &[1, 1, 0, 0x23, 0x30, 0]);
        assert_eq!(&image.memory()[ALARM..ALARM + 4], &[3, 1, 0, 0]);
        let before = image.memory().to_vec();
        let out = import_csv(&mut image, Scope::Whole, b"Name\nX\n", options, false);
        assert!(out.failure.is_some());
        assert_eq!(image.memory(), before);
    }
    #[test]
    fn original_import_clears_first_and_error_keeps_applied_prefix() {
        let mut i = image();
        let first = import_decoded(
            &mut i,
            Scope::Whole,
            [Ok(parsed(b'A')), Ok(parsed(b'B'))],
            false,
        );
        assert_eq!(first.applied, 2);
        let second = import_decoded(
            &mut i,
            Scope::Whole,
            [
                Ok(parsed(b'C')),
                Err(csv::Error::Conversion("injected parser error")),
            ],
            false,
        );
        assert_eq!(
            (second.applied, second.rows_read, second.counts.c4),
            (1, 2, 1)
        );
        assert!(second.failure.is_some());
        assert_eq!(operations::copy(&i, 1, 0, 0).unwrap()[0].bytes[18], b'C');
        let empty = import_decoded(&mut i, Scope::Whole, [], false);
        assert_eq!(empty.applied, 0);
        assert!(
            i.gps_groups()
                .unwrap()
                .iter()
                .all(|g| g.physical_slots().is_empty())
        );
    }
    #[test]
    fn image_gps_export_uses_relative_alarm_and_preserves_formatter_failure_prefix() {
        let mut i = image();
        let mut a = parsed(b'A');
        a.alarm = true;
        import_decoded(&mut i, Scope::Whole, [Ok(a), Ok(parsed(b'B'))], false);
        let mut calls = 0;
        let out = export_csv(&i, Scope::Whole, csv::Locale::copied(b'.', b','), |r| {
            calls += 1;
            if calls == 3 {
                return Err(csv::Error::Operation("injected formatter failure"));
            }
            assert!(matches!(
                r.adapter,
                payload::Adapter::GpsDate | payload::Adapter::GpsTime
            ));
            Ok(Vec::new())
        });
        assert_eq!(out.rows, 1);
        assert!(out.failure.is_some());
        assert!(out.bytes.ends_with(b",,,0.000000,0.000000,,ON\r\n"));
        let empty = export_csv(
            &i,
            Scope::Group(0),
            csv::Locale::copied(b'.', b','),
            |_| panic!(),
        );
        assert_eq!(empty.rows, 0);
        assert!(empty.failure.is_none());
    }
}
