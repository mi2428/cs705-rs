//! Lossless local edits of a deliberately limited GPS nine-column CSV document.
//!
//! Only the exact header, comma-separated unquoted cells, CRLF physical lines
//! and at most 300 data rows are supported. BOM headers, quotes, semicolons,
//! ASCII controls and DEL are rejected. Existing cells, including high bytes,
//! overlength names and Date/Time, remain opaque: parsing checks structure only.
//! New names use safe ASCII, coordinates use an integer six-place degree subset,
//! and typed altitude is converted from tenths of a foot to one-place metres.
//! These edits have not been compared with the original GUI. They are not a
//! general decimal/RTL float codec, locale date/time conversion or full import.
//! No ICF apply, group allocation/move/rename, global alarm update or implicit
//! clearing/defaulting occurs. Rows cannot be added, removed or reordered.

use crate::icf::{GpsAltitude, GpsCoordinate};
use std::borrow::Cow;
use std::fmt;

pub mod editor;
pub mod operations;
pub mod parser;
pub mod transfer;

const HEADER: &[u8] = b"Group,Group Name,Name,Date,Time,Latitude,Longitude,Altitude,Alarm\r\n";

/// Structural or editing errors; reasons never include user cell contents.
/// Physical lines are one-based, while data-row indices are zero-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A required structural bound or cell count is violated.
    Invalid {
        /// One-based physical line.
        line: usize,
        /// Static diagnostic.
        reason: &'static str,
    },
    /// Syntax or a new value is outside the limited subset.
    Unsupported {
        /// One-based physical line.
        line: usize,
        /// Static diagnostic.
        reason: &'static str,
    },
    /// The requested data row does not exist.
    RowIndexOutOfBounds {
        /// Zero-based requested index.
        index: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { line, reason } => write!(f, "invalid GPS CSV at line {line}: {reason}"),
            Self::Unsupported { line, reason } => {
                write!(f, "unsupported GPS CSV at line {line}: {reason}")
            }
            Self::RowIndexOutOfBounds { index } => {
                write!(f, "GPS CSV row index {index} is out of bounds")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Nine structurally checked cells, not decoded or validated GPS values.
pub struct Row<'a> {
    cells: [Cow<'a, [u8]>; 9],
}

impl Row<'_> {
    /// Borrow all nine current cells in exact header order.
    pub fn cells(&self) -> [&[u8]; 9] {
        self.cells.each_ref().map(|cell| cell.as_ref())
    }
}

/// A document borrowing unchanged cells; only explicit cell edits allocate.
pub struct Csv<'a> {
    rows: Vec<Row<'a>>,
}

impl<'a> Csv<'a> {
    /// Parse the exact header and zero to 300 nine-cell CRLF-terminated rows.
    /// Old values, encoding, name widths and Date/Time are not validated.
    /// # Errors
    /// Rejects unsupported syntax, a wrong cell count or excessive rows.
    pub fn parse(source: &'a [u8]) -> Result<Self, Error> {
        let mut lines = source.split_inclusive(|byte| *byte == b'\n');
        let header = lines.next().ok_or(Error::Invalid {
            line: 1,
            reason: "missing header",
        })?;
        if header != HEADER {
            return Err(Error::Unsupported {
                line: 1,
                reason: "expected the exact GPS nine-column header and CRLF",
            });
        }
        let mut rows = Vec::new();
        for (index, physical) in lines.enumerate() {
            let line = index + 2;
            if rows.len() == 300 {
                return Err(Error::Invalid {
                    line,
                    reason: "more than 300 GPS data rows",
                });
            }
            let text = physical.strip_suffix(b"\r\n").ok_or(Error::Unsupported {
                line,
                reason: "every physical line must end in CRLF",
            })?;
            if text
                .iter()
                .any(|byte| matches!(*byte, 0..=0x1f | 0x7f | b'"' | b';'))
            {
                return Err(Error::Unsupported {
                    line,
                    reason: "quote, semicolon or control byte",
                });
            }
            let mut cells = [b"".as_slice(); 9];
            let mut parts = text.split(|byte| *byte == b',');
            for cell in &mut cells {
                *cell = parts.next().ok_or(Error::Invalid {
                    line,
                    reason: "expected exactly nine GPS cells",
                })?;
            }
            if parts.next().is_some() {
                return Err(Error::Invalid {
                    line,
                    reason: "expected exactly nine GPS cells",
                });
            }
            rows.push(Row {
                cells: cells.map(Cow::Borrowed),
            });
        }
        Ok(Self { rows })
    }

    /// Borrow rows in their original order.
    pub fn rows(&self) -> &[Row<'a>] {
        &self.rows
    }

    /// Replace only Group: 0 emits OFF, 1..=26 emit A..Z.
    /// Does not clear/propagate Group Name, move rows or allocate an ICF record.
    /// # Errors
    /// Row bounds take priority; unsupported new values leave all state intact.
    pub fn set_group(&mut self, index: usize, group: u8) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        let bytes = match group {
            0 => b"OFF".to_vec(),
            1..=26 => vec![b'A' + group - 1],
            _ => {
                return Err(Error::Unsupported {
                    line: index + 2,
                    reason: "GPS Group requires 0..=26",
                });
            }
        };
        row.cells[0] = Cow::Owned(bytes);
        Ok(())
    }

    /// Replace only Group Name with 0..=16 safe printable ASCII bytes.
    /// Leading/interior spaces and case are retained, with no group propagation.
    /// No ICF name record is created, including for Group OFF.
    /// # Errors
    /// Bounds take priority; rejects delimiter/quote, trailing space or unsafe bytes.
    pub fn set_group_name(&mut self, index: usize, value: &[u8]) -> Result<(), Error> {
        self.set_text(index, 1, value)
    }

    /// Replace only Name with 0..=16 safe printable ASCII bytes, without trim/pad.
    /// Leading/interior spaces and case are retained; old names stay opaque.
    /// # Errors
    /// Bounds take priority; rejects delimiter/quote, trailing space or unsafe bytes.
    pub fn set_name(&mut self, index: usize, value: &[u8]) -> Result<(), Error> {
        self.set_text(index, 2, value)
    }

    fn set_text(&mut self, index: usize, column: usize, value: &[u8]) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        if value.len() > 16
            || value
                .iter()
                .any(|b| !(b' '..=b'~').contains(b) || b",;\"".contains(b))
            || value.ends_with(b" ")
        {
            return Err(Error::Unsupported {
                line: index + 2,
                reason: "GPS names require at most 16 safe ASCII bytes without trailing spaces",
            });
        }
        row.cells[column] = Cow::Owned(value.to_vec());
        Ok(())
    }

    /// Replace only Latitude with six-place integer canonical degrees.
    /// Degrees <=90, ticks <60000, and endpoint ticks zero are required.
    /// Negative zero is retained; Date/Time and longitude are not interpreted.
    /// # Errors
    /// Bounds take priority; invalid components leave all cells unchanged.
    pub fn set_latitude(&mut self, index: usize, value: GpsCoordinate) -> Result<(), Error> {
        self.set_coordinate(index, 5, 90, value)
    }

    /// Replace only Longitude with six-place integer canonical degrees.
    /// Degrees <=180, ticks <60000, and endpoint ticks zero are required.
    /// Negative zero is retained; no arbitrary decimal/RTL equivalence is claimed.
    /// # Errors
    /// Bounds take priority; invalid components leave all cells unchanged.
    pub fn set_longitude(&mut self, index: usize, value: GpsCoordinate) -> Result<(), Error> {
        self.set_coordinate(index, 6, 180, value)
    }

    fn set_coordinate(
        &mut self,
        index: usize,
        column: usize,
        limit: u8,
        value: GpsCoordinate,
    ) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        if value.degrees > limit
            || value.thousandths_of_minute >= 60000
            || (value.degrees == limit && value.thousandths_of_minute != 0)
        {
            return Err(Error::Unsupported {
                line: index + 2,
                reason: "GPS coordinate outside degree/tick domain",
            });
        }
        let ticks = value.thousandths_of_minute.to_be_bytes();
        let direction = match (column == 5, value.negative) {
            (true, false) => b'N',
            (true, true) => b'S',
            (false, false) => b'E',
            (false, true) => b'W',
        };
        row.cells[column] = Cow::Owned(crate::csv::number::coordinate_text(
            [value.degrees, ticks[0], ticks[1], direction],
            column == 5,
            b'.',
        ));
        Ok(())
    }

    /// Replace only Altitude with suffix-free one-place metres, or explicit empty Unknown.
    /// Accepts -328084..=328084 tenths of a foot. Magnitude conversion is
    /// (abs(raw)*3048+5000)/10000; sign is applied after conversion, so -1 emits 0.0.
    /// This is output conversion, not the CSV negative-fraction import rule.
    /// # Errors
    /// Bounds take priority; out-of-domain raw values leave all cells unchanged.
    pub fn set_altitude(&mut self, index: usize, value: GpsAltitude) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        let raw = match value {
            GpsAltitude::Unknown => i32::MAX,
            GpsAltitude::TenthsOfFoot(n) if (-328084..=328084).contains(&n) => n,
            _ => {
                return Err(Error::Unsupported {
                    line: index + 2,
                    reason: "GPS altitude outside tenths-foot domain",
                });
            }
        };
        row.cells[7] = Cow::Owned(crate::csv::number::altitude_text(raw, b'.'));
        Ok(())
    }

    /// Replace only this row's Alarm with OFF/ON, not global GpsAlarm state.
    /// # Errors
    /// An invalid row index leaves the document unchanged.
    pub fn set_alarm(&mut self, index: usize, value: bool) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        row.cells[8] = Cow::Owned(if value {
            b"ON".to_vec()
        } else {
            b"OFF".to_vec()
        });
        Ok(())
    }

    /// Serialize without normalization, preserving every unedited byte and row order.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = HEADER.to_vec();
        for row in &self.rows {
            for (column, cell) in row.cells.iter().enumerate() {
                if column != 0 {
                    bytes.push(b',');
                }
                bytes.extend_from_slice(cell);
            }
            bytes.extend_from_slice(b"\r\n");
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coordinate(degrees: u8, ticks: u16, negative: bool) -> GpsCoordinate {
        GpsCoordinate {
            degrees,
            thousandths_of_minute: ticks,
            negative,
        }
    }

    fn literal(changes: &[(usize, &[u8])]) -> Vec<u8> {
        // Independent raw literals, including long names and uninterpreted dates.
        let cells: [&[u8]; 9] = [
            b"opaque-group",
            b"\x83\x5c-group-over-sixteen",
            b"\x82\xa0-name-over-sixteen",
            b"not-a-date\xff",
            b"opaque-time",
            b"old-lat",
            b"\x81\x40lon",
            b"unknown-alt",
            b"unknown-alarm",
        ];
        let mut bytes = HEADER.to_vec();
        for (column, cell) in cells.iter().enumerate() {
            if column != 0 {
                bytes.push(b',');
            }
            bytes.extend_from_slice(
                changes
                    .iter()
                    .find(|(c, _)| *c == column)
                    .map_or(*cell, |(_, value)| *value),
            );
        }
        bytes.extend_from_slice(b"\r\nopaque-group,Keep Group,Keep Name,old-date,old-time,old-lat,old-lon,old-alt,old-alarm\r\n");
        bytes
    }

    #[test]
    fn structure_is_strict_but_old_cells_are_opaque_and_lossless() {
        assert!(Csv::parse(HEADER).unwrap().rows().is_empty());
        assert_eq!(Csv::parse(HEADER).unwrap().to_bytes(), HEADER);
        let source = literal(&[]);
        let csv = Csv::parse(&source).unwrap();
        assert_eq!(csv.to_bytes(), source);
        for row in csv.rows() {
            for cell in row.cells() {
                assert!(cell.as_ptr() >= source.as_ptr());
                assert!(
                    (cell.as_ptr() as usize) + cell.len()
                        <= source.as_ptr() as usize + source.len()
                );
            }
        }
        assert!(matches!(
            Csv::parse(b""),
            Err(Error::Invalid { line: 1, .. })
        ));
        for header in [
            b"\xef\xbb\xbf".as_slice(),
            b"Group,Name\r\n",
            &HEADER[..HEADER.len() - 2],
            &HEADER[..HEADER.len() - 1],
        ] {
            assert!(matches!(
                Csv::parse(header),
                Err(Error::Unsupported { line: 1, .. })
            ));
        }
        let bom = [b"\xef\xbb\xbf".as_slice(), HEADER].concat();
        assert!(matches!(
            Csv::parse(&bom),
            Err(Error::Unsupported { line: 1, .. })
        ));
        for row in [
            b"a,b,c,d,e,f,g,h\r\n".as_slice(),
            b"a,b,c,d,e,f,g,h,i,j\r\n",
            b"\r\n",
        ] {
            let bytes = [HEADER, row].concat();
            assert!(matches!(
                Csv::parse(&bytes),
                Err(Error::Invalid { line: 2, .. })
            ));
        }
        for row in [
            b",,,,,,,,".as_slice(),
            b",,,,,,,,\n",
            b",,,,,,,,\r",
            b",,,,,,,,\r\n\n",
            b",,,,,,,,\r\n\r\n",
        ] {
            let bytes = [HEADER, row].concat();
            assert!(Csv::parse(&bytes).is_err());
        }
        let empties = [HEADER, b",,,,,,,,\r\n"].concat();
        assert_eq!(Csv::parse(&empties).unwrap().to_bytes(), empties);
        for byte in 0..=255_u8 {
            let row = [
                byte, b',', b',', b',', b',', b',', b',', b',', b',', b'\r', b'\n',
            ];
            let bytes = [HEADER, &row].concat();
            if matches!(byte, 0..=0x1f | 0x7f | b'"' | b';' | b',') {
                assert!(Csv::parse(&bytes).is_err(), "byte {byte}");
            } else {
                assert_eq!(Csv::parse(&bytes).unwrap().to_bytes(), bytes);
            }
        }
        let row = b"A,G,N,D,T,L,L,A,A\r\n";
        let mut bytes = HEADER.to_vec();
        for _ in 0..300 {
            bytes.extend_from_slice(row);
        }
        assert_eq!(Csv::parse(&bytes).unwrap().rows().len(), 300);
        assert_eq!(Csv::parse(&bytes).unwrap().to_bytes(), bytes);
        bytes.extend_from_slice(row);
        assert!(matches!(
            Csv::parse(&bytes),
            Err(Error::Invalid { line: 302, .. })
        ));
        assert_eq!(
            Error::RowIndexOutOfBounds { index: 300 }.to_string(),
            "GPS CSV row index 300 is out of bounds"
        );
        assert_eq!(
            Error::Invalid {
                line: 2,
                reason: "test"
            }
            .to_string(),
            "invalid GPS CSV at line 2: test"
        );
        assert_eq!(
            Error::Unsupported {
                line: 3,
                reason: "test"
            }
            .to_string(),
            "unsupported GPS CSV at line 3: test"
        );
    }

    #[test]
    fn seven_local_setters_match_literals_and_preserve_all_other_bytes() {
        let source = literal(&[]);
        let original = source.clone();
        macro_rules! check {
            ($column:literal, $method:ident, $cases:expr) => {
                for (value, text) in $cases {
                    let mut csv = Csv::parse(&source).unwrap();
                    let pointers: Vec<_> = csv
                        .rows()
                        .iter()
                        .map(|r| r.cells().map(|c| c.as_ptr()))
                        .collect();
                    csv.$method(0, value).unwrap();
                    let expected = literal(&[($column, text)]);
                    assert_eq!(csv.to_bytes(), expected);
                    assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
                    for (r, row) in csv.rows().iter().enumerate() {
                        for (c, cell) in row.cells().iter().enumerate() {
                            if (r, c) != (0, $column) {
                                assert_eq!(cell.as_ptr(), pointers[r][c]);
                            }
                        }
                    }
                }
            };
        }
        check!(
            0,
            set_group,
            [(0, b"OFF".as_slice()), (1, b"A"), (26, b"Z")]
        );
        check!(
            1,
            set_group_name,
            [
                (b"".as_slice(), b"".as_slice()),
                (b"0123456789AbCdEf", b"0123456789AbCdEf"),
                (b" a B", b" a B"),
                (b"!#$%&'()*+-./:<?", b"!#$%&'()*+-./:<?")
            ]
        );
        check!(
            2,
            set_name,
            [
                (b"".as_slice(), b"".as_slice()),
                (b"0123456789AbCdEf", b"0123456789AbCdEf"),
                (b" a B", b" a B")
            ]
        );
        check!(
            5,
            set_latitude,
            [
                (coordinate(0, 0, false), b"0.000000".as_slice()),
                (coordinate(0, 0, true), b"-0.000000"),
                (coordinate(90, 0, false), b"90.000000"),
                (coordinate(90, 0, true), b"-90.000000"),
                (coordinate(12, 1, false), b"12.000017"),
                (coordinate(12, 2, true), b"-12.000033"),
                (coordinate(12, 3, false), b"12.000050"),
                (coordinate(89, 59999, true), b"-89.999983"),
            ]
        );
        check!(
            6,
            set_longitude,
            [
                (coordinate(0, 0, false), b"0.000000".as_slice()),
                (coordinate(0, 0, true), b"-0.000000"),
                (coordinate(180, 0, false), b"180.000000"),
                (coordinate(180, 0, true), b"-180.000000"),
                (coordinate(179, 59999, false), b"179.999983"),
            ]
        );
        check!(
            7,
            set_altitude,
            [
                (GpsAltitude::Unknown, b"".as_slice()),
                (GpsAltitude::TenthsOfFoot(0), b"0.0"),
                (GpsAltitude::TenthsOfFoot(-1), b"0.0"),
                (GpsAltitude::TenthsOfFoot(1), b"0.0"),
                (GpsAltitude::TenthsOfFoot(-2), b"-0.1"),
                (GpsAltitude::TenthsOfFoot(2), b"0.1"),
                (GpsAltitude::TenthsOfFoot(-39), b"-1.2"),
                (GpsAltitude::TenthsOfFoot(39), b"1.2"),
                (GpsAltitude::TenthsOfFoot(328080), b"9999.9"),
                (GpsAltitude::TenthsOfFoot(-328084), b"-10000.0"),
                (GpsAltitude::TenthsOfFoot(328084), b"10000.0"),
            ]
        );
        check!(8, set_alarm, [(false, b"OFF".as_slice()), (true, b"ON")]);
        for byte in 0..=255_u8 {
            let value = [byte, b'Z'];
            let valid = (32..=126).contains(&byte) && !b",;\"".contains(&byte);
            let mut csv = Csv::parse(&source).unwrap();
            assert_eq!(csv.set_name(0, &value).is_ok(), valid);
            assert_eq!(csv.set_group_name(0, &value).is_ok(), valid);
            let expected = if valid {
                literal(&[(1, &value), (2, &value)])
            } else {
                source.clone()
            };
            assert_eq!(csv.to_bytes(), expected);
        }
        let mut csv = Csv::parse(&source).unwrap();
        csv.set_group(0, 0).unwrap();
        csv.set_group_name(0, b"Local Group").unwrap();
        csv.set_name(0, b" Name").unwrap();
        csv.set_latitude(0, coordinate(0, 0, true)).unwrap();
        csv.set_longitude(0, coordinate(180, 0, false)).unwrap();
        csv.set_altitude(0, GpsAltitude::TenthsOfFoot(-39)).unwrap();
        csv.set_alarm(0, true).unwrap();
        let expected = [HEADER, b"OFF,Local Group, Name,not-a-date\xff,opaque-time,-0.000000,180.000000,-1.2,ON\r\n", b"opaque-group,Keep Group,Keep Name,old-date,old-time,old-lat,old-lon,old-alt,old-alarm\r\n"].concat();
        assert_eq!(csv.to_bytes(), expected);
        assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
        assert_eq!(source, original);
    }

    #[test]
    fn last_row_domains_bounds_priority_and_late_errors_are_atomic() {
        let row = b"A,Keep Group,Keep Name,Date,Time,lat,lon,alt,alarm\r\n";
        let mut source = HEADER.to_vec();
        for _ in 0..300 {
            source.extend_from_slice(row);
        }
        let original = source.clone();
        let mut csv = Csv::parse(&source).unwrap();
        let pointers: Vec<_> = csv
            .rows()
            .iter()
            .map(|r| r.cells().map(|c| c.as_ptr()))
            .collect();
        csv.set_group(299, 26).unwrap();
        csv.set_group_name(299, b"0123456789ABCDEF").unwrap();
        csv.set_name(299, b"").unwrap();
        csv.set_latitude(299, coordinate(90, 0, true)).unwrap();
        csv.set_longitude(299, coordinate(180, 0, true)).unwrap();
        csv.set_altitude(299, GpsAltitude::Unknown).unwrap();
        csv.set_alarm(299, false).unwrap();
        let mut expected = HEADER.to_vec();
        for _ in 0..299 {
            expected.extend_from_slice(row);
        }
        expected
            .extend_from_slice(b"Z,0123456789ABCDEF,,Date,Time,-90.000000,-180.000000,,OFF\r\n");
        assert_eq!(csv.to_bytes(), expected);
        for (r, row) in csv.rows().iter().enumerate() {
            for (c, cell) in row.cells().iter().enumerate() {
                if r != 299 || [3, 4].contains(&c) {
                    assert_eq!(cell.as_ptr(), pointers[r][c]);
                }
            }
        }
        for index in [0, 299] {
            let before: Vec<_> = csv
                .rows()
                .iter()
                .map(|r| r.cells().map(|c| c.as_ptr()))
                .collect();
            for group in [27, 255] {
                assert!(
                    matches!(csv.set_group(index, group), Err(Error::Unsupported { line, .. }) if line == index + 2)
                );
                assert_eq!(csv.to_bytes(), expected);
            }
            for value in [
                b"0123456789ABCDEFG".as_slice(),
                b" ",
                b"GOOD ",
                b"GOOD,",
                b"GOOD;",
                b"GOOD\"",
                b"GOOD\0",
                b"GOOD\x7f",
                b"GOOD\xff",
            ] {
                assert!(
                    matches!(csv.set_name(index, value), Err(Error::Unsupported { line, .. }) if line == index + 2)
                );
                assert!(
                    matches!(csv.set_group_name(index, value), Err(Error::Unsupported { line, .. }) if line == index + 2)
                );
                assert_eq!(csv.to_bytes(), expected);
            }
            for value in [
                coordinate(91, 0, false),
                coordinate(90, 1, true),
                coordinate(0, 60000, false),
                coordinate(0, u16::MAX, true),
                coordinate(255, 0, false),
            ] {
                assert!(
                    matches!(csv.set_latitude(index, value), Err(Error::Unsupported { line, .. }) if line == index + 2)
                );
                assert_eq!(csv.to_bytes(), expected);
            }
            for value in [
                coordinate(181, 0, false),
                coordinate(180, 1, true),
                coordinate(0, 60000, false),
                coordinate(0, u16::MAX, true),
                coordinate(255, 0, false),
            ] {
                assert!(
                    matches!(csv.set_longitude(index, value), Err(Error::Unsupported { line, .. }) if line == index + 2)
                );
                assert_eq!(csv.to_bytes(), expected);
            }
            for value in [i32::MIN, i32::MAX, -328085, 328085] {
                assert!(
                    matches!(csv.set_altitude(index, GpsAltitude::TenthsOfFoot(value)), Err(Error::Unsupported { line, .. }) if line == index + 2)
                );
                assert_eq!(csv.to_bytes(), expected);
            }
            assert_eq!(
                csv.rows()
                    .iter()
                    .map(|r| r.cells().map(|c| c.as_ptr()))
                    .collect::<Vec<_>>(),
                before
            );
        }
        let mut empty = Csv::parse(HEADER).unwrap();
        for (document, indices) in [(&mut csv, [300, usize::MAX]), (&mut empty, [0, usize::MAX])] {
            let before = document.to_bytes();
            for index in indices {
                let error = Err(Error::RowIndexOutOfBounds { index });
                assert_eq!(document.set_group(index, 255), error);
                assert_eq!(
                    document.set_group_name(index, b"\xffinvalid-name-too-long"),
                    error
                );
                assert_eq!(
                    document.set_name(index, b"\xffinvalid-name-too-long"),
                    error
                );
                assert_eq!(
                    document.set_latitude(index, coordinate(255, 65535, true)),
                    error
                );
                assert_eq!(
                    document.set_longitude(index, coordinate(255, 65535, true)),
                    error
                );
                assert_eq!(
                    document.set_altitude(index, GpsAltitude::TenthsOfFoot(i32::MIN)),
                    error
                );
                assert_eq!(document.set_alarm(index, true), error);
                assert_eq!(document.to_bytes(), before);
            }
        }
        assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
        assert_eq!(source, original);
    }
}
