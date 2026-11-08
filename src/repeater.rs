//! Lossless raw Name and Sub Name editing of a limited repeater CSV format.
//!
//! Only the exact 17-column export header, comma separators, unquoted cells and
//! CRLF-terminated physical lines are supported. BOMs, quotes, semicolons, ASCII
//! control bytes and DEL are rejected. Every data row must have 17 cells, its
//! Name must fit in 16 bytes, and there may be at most 2500 data rows.
//!
//! Existing cell bytes are opaque, not decoded as UTF-8 or validated as CP932.
//! Numeric formats, modes, groups, units and radio suitability are not checked.
//! New Name/Sub Name values use narrower ASCII-only limits of 16/8 bytes.
//! Sub Name edits have not been compared with the original GUI. No source
//! application normalization, truncation, padding or defaulting is performed.
//! Successful parsing establishes structure only, not general CS-705
//! compatibility. For one fixed native export, DV/FM Name edits matched native
//! GUI edits and native import/re-export byte-for-byte. Other inputs and fields
//! are not covered by that comparison.

use std::borrow::Cow;
use std::fmt;

// CS-705 1.30 CS705.exe SHA-256:
// e9439e1413e1258be77e6ef2188b72276dd714642e3858f4690e5d45653aeca3.
// Header RVA 0x8E50C, raw-cell split RVA 0x46970, Name width RVA 0x8F385.
// Output RVA 0x8FC40 uses source-specific converters. This editor preserves bytes
// instead; its exact header, CRLF and ASCII-edit rules are intentional limits,
// not claims about every input accepted or rejected by the source application.
// Fixed native 2411-row export SHA-256:
// ce0941044054eba474ef5b489a2585f68385a93dc4e8f972384a722f195f5e75.
// Native GUI edits of two Name cells, Rust output and native re-export matched:
// acbac158dc80f13a28b6925dce74aacb026bb41a0cf464d0f6c07b2425bee2aa.
const HEADER: &[u8] = b"Group No,Group Name,Name,Sub Name,Repeater Call Sign,Gateway Call Sign,Frequency,Dup,Offset,Mode,TONE,Repeater Tone,RPT1USE,Position,Latitude,Longitude,UTC Offset\r\n";
const NAME: usize = 2;
const SUB_NAME: usize = 3;
const MAX_ROWS: usize = 2500;

/// Invalid or unsupported CSV structure or editing input.
///
/// Physical line numbers are one-based; data-row indices are zero-based.
/// Reasons are static diagnostics and never include configuration contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A required structural bound or cell count is violated.
    Invalid {
        /// The physical line containing the invalid input.
        line: usize,
        /// The structural violation.
        reason: &'static str,
    },
    /// Input or an edit lies outside this deliberately limited format.
    Unsupported {
        /// The physical line containing the unsupported input or edit.
        line: usize,
        /// The unsupported property.
        reason: &'static str,
    },
    /// The requested zero-based data-row index does not exist.
    RowIndexOutOfBounds {
        /// The requested index.
        index: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { line, reason } => {
                write!(f, "invalid repeater CSV at line {line}: {reason}")
            }
            Self::Unsupported { line, reason } => {
                write!(f, "unsupported repeater CSV at line {line}: {reason}")
            }
            Self::RowIndexOutOfBounds { index } => {
                write!(f, "repeater CSV row index {index} is out of bounds")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A structurally checked row with opaque cell bytes, not validated radio values.
pub struct Row<'a> {
    cells: [Cow<'a, [u8]>; 17],
}

impl Row<'_> {
    /// Return all 17 raw cells in fixed header order, including the current Name.
    pub fn cells(&self) -> [&[u8]; 17] {
        self.cells.each_ref().map(|cell| cell.as_ref())
    }

    /// Return Name bytes without decoding, trimming or normalizing them.
    pub fn name_bytes(&self) -> &[u8] {
        &self.cells[NAME]
    }

    /// Borrow Sub Name bytes without decoding, trimming or enforcing edit limits.
    pub fn sub_name_bytes(&self) -> &[u8] {
        &self.cells[SUB_NAME]
    }
}

/// A CSV document borrowing unchanged cells and owning only edited names.
///
/// Rows cannot be added, removed or reordered through this API. All bytes other
/// than an explicitly edited Name/Sub Name, including empty cells and CRLFs, are retained.
pub struct Csv<'a> {
    rows: Vec<Row<'a>>,
}

impl<'a> Csv<'a> {
    /// Parse the exact limited format described in this module without I/O.
    ///
    /// Zero to 2500 data rows are accepted. All cells, including non-ASCII bytes,
    /// are preserved as opaque bytes; only structure and Name length are checked.
    /// The last physical line must also end in CRLF.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing or noncanonical header, unsupported syntax,
    /// wrong cell count, excessive row count or a Name longer than 16 bytes.
    pub fn parse(source: &'a [u8]) -> Result<Self, Error> {
        let mut lines = source.split_inclusive(|byte| *byte == b'\n');
        let header = lines.next().ok_or(Error::Invalid {
            line: 1,
            reason: "missing header",
        })?;
        if header != HEADER {
            return Err(Error::Unsupported {
                line: 1,
                reason: "expected the exact 17-column header and CRLF",
            });
        }

        let mut rows = Vec::new();
        for (index, physical) in lines.enumerate() {
            let line = index + 2;
            if rows.len() == MAX_ROWS {
                return Err(Error::Invalid {
                    line,
                    reason: "more than 2500 data rows",
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
            let mut cells = [b"".as_slice(); 17];
            let mut parts = text.split(|byte| *byte == b',');
            for cell in &mut cells {
                *cell = parts.next().ok_or(Error::Invalid {
                    line,
                    reason: "expected exactly 17 cells",
                })?;
            }
            if parts.next().is_some() {
                return Err(Error::Invalid {
                    line,
                    reason: "expected exactly 17 cells",
                });
            }
            check_name_length(cells[NAME], line)?;
            rows.push(Row {
                cells: cells.map(Cow::Borrowed),
            });
        }
        Ok(Self { rows })
    }

    /// Borrow rows in original order; other columns remain uninterpreted bytes.
    pub fn rows(&self) -> &[Row<'a>] {
        &self.rows
    }

    /// Replace one Name, copying its bytes without altering any other cell.
    ///
    /// An empty name or at most 16 printable ASCII bytes are supported, excluding
    /// comma, semicolon and double quote, with no trailing space. Non-ASCII edits
    /// are unsupported rather than guessing CP932 normalization. Existing opaque
    /// names, including non-ASCII names, can remain untouched.
    ///
    /// ```
    /// # fn edit(source: &[u8]) -> Result<Vec<u8>, cs705::repeater::Error> {
    /// let mut csv = cs705::repeater::Csv::parse(source)?;
    /// csv.set_name(0, b"New name")?;
    /// Ok(csv.to_bytes())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid row index or an unsupported name. Failed
    /// edits leave the entire document unchanged; names are never truncated.
    pub fn set_name(&mut self, index: usize, name: &[u8]) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        let line = index + 2;
        check_name_length(name, line)?;
        if name
            .iter()
            .any(|byte| !matches!(*byte, b' '..=b'~') || matches!(*byte, b',' | b';' | b'"'))
            || name.ends_with(b" ")
        {
            return Err(Error::Unsupported {
                line,
                reason: "edited names require safe ASCII without trailing spaces",
            });
        }
        row.cells[NAME] = Cow::Owned(name.to_vec());
        Ok(())
    }

    /// Replace only Sub Name with 0..=8 safe printable ASCII bytes, without padding.
    ///
    /// Comma, semicolon, double quote and trailing space are unsupported. Leading
    /// and interior spaces and case are preserved. An opaque or overlength old
    /// cell can remain untouched or be explicitly replaced; parsing is unchanged.
    /// This narrow raw edit has not been compared with the original GUI.
    ///
    /// # Errors
    /// Row bounds are checked before the new value. Unsupported edits leave every
    /// cell unchanged; no truncation, normalization or value conversion occurs.
    pub fn set_sub_name(&mut self, index: usize, name: &[u8]) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        let line = index + 2;
        if name.len() > 8 {
            return Err(Error::Unsupported {
                line,
                reason: "Sub Name exceeds 8 bytes",
            });
        }
        if name
            .iter()
            .any(|byte| !matches!(*byte, b' '..=b'~') || matches!(*byte, b',' | b';' | b'"'))
            || name.ends_with(b" ")
        {
            return Err(Error::Unsupported {
                line,
                reason: "edited sub names require safe ASCII without trailing spaces",
            });
        }
        row.cells[SUB_NAME] = Cow::Owned(name.to_vec());
        Ok(())
    }

    /// Serialize without normalization, preserving every unedited byte and order.
    ///
    /// No file or device is accessed. The result has the same limited syntax as
    /// the parsed input and is not certified for a radio or the source software.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut output = HEADER.to_vec();
        for row in &self.rows {
            for (index, cell) in row.cells.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                output.extend_from_slice(cell);
            }
            output.extend_from_slice(b"\r\n");
        }
        output
    }
}

fn check_name_length(name: &[u8], line: usize) -> Result<(), Error> {
    if name.len() > 16 {
        return Err(Error::Unsupported {
            line,
            reason: "Name exceeds 16 bytes",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic A4 examples, not copied from an original application export.
    const SAMPLE: &[u8] = concat!(
        "Group No,Group Name,Name,Sub Name,Repeater Call Sign,Gateway Call Sign,Frequency,Dup,Offset,Mode,TONE,Repeater Tone,RPT1USE,Position,Latitude,Longitude,UTC Offset\r\n",
        "1,GROUP1,FM test,FM,,,145.000000,DUP-,0.600000,FM,TONE,88.5Hz,NO,Approximate,-35.500000,139.750000,0:00\r\n",
        "50,GROUP50,DV test,DV,TEST01 B,TEST01 G,439.000000,DUP-,5.000000,DV,,,YES,Exact,35.500000,139.750000,+9:00\r\n",
    ).as_bytes();

    #[test]
    fn raw_sub_name_edits_preserve_every_other_csv_byte() {
        let source = [HEADER,
            b"1,\x83\x5c,\x82\xa0\x95\x5c,\x82\xa0opaque-long,,,not-a-frequency,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque\r\n",
            b"2,Group,Old Name,untouched,,,other,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque\r\n",
        ].concat();
        let mut csv = Csv::parse(&source).unwrap();
        let at = HEADER.len() + b"1,\x83\x5c,\x82\xa0\x95\x5c,".len();
        assert_eq!(csv.rows()[0].sub_name_bytes(), b"\x82\xa0opaque-long");
        assert_eq!(
            csv.rows()[0].sub_name_bytes().as_ptr(),
            source[at..].as_ptr()
        );
        assert_eq!(
            csv.rows()[0].sub_name_bytes().as_ptr(),
            csv.rows()[0].cells()[3].as_ptr()
        );
        assert_eq!(csv.to_bytes(), source);
        csv.set_sub_name(0, b"AbCd1234").unwrap();
        csv.set_sub_name(1, b"").unwrap();
        let expected = [HEADER,
            b"1,\x83\x5c,\x82\xa0\x95\x5c,AbCd1234,,,not-a-frequency,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque\r\n",
            b"2,Group,Old Name,,,,other,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque\r\n",
        ].concat();
        assert_eq!(csv.to_bytes(), expected);
        assert_eq!(csv.rows()[0].name_bytes(), b"\x82\xa0\x95\x5c");
        assert_eq!(csv.rows()[1].sub_name_bytes(), b"");
        let parsed = Csv::parse(&expected).unwrap();
        assert_eq!(parsed.rows()[0].sub_name_bytes(), b"AbCd1234");
        assert_eq!(parsed.to_bytes(), expected);
        // Exhaustive single-byte boundary; expected output is built from literals,
        // never by using a product edit as an oracle.
        for byte in 0..=255_u8 {
            let mut csv = Csv::parse(&source).unwrap();
            if (33..=126).contains(&byte) && !b",;\"".contains(&byte) {
                csv.set_sub_name(0, &[byte]).unwrap();
                let expected=[HEADER,b"1,\x83\x5c,\x82\xa0\x95\x5c,",&[byte],b",,,not-a-frequency,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque\r\n",b"2,Group,Old Name,untouched,,,other,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque\r\n"].concat();
                assert_eq!(csv.to_bytes(), expected);
            } else {
                assert!(matches!(
                    csv.set_sub_name(0, &[byte]),
                    Err(Error::Unsupported { line: 2, .. })
                ));
                assert_eq!(csv.to_bytes(), source);
            }
        }
        csv.set_sub_name(0, b" A b").unwrap();
        assert_eq!(csv.rows()[0].sub_name_bytes(), b" A b");
        let before = csv.to_bytes();
        for value in [
            b"123456789".as_slice(),
            b"tail ",
            b" A ",
            b"ok\0",
            b"ok\x7f",
            b"ok\xff",
            b"ok,",
            b"ok;",
            b"ok\"",
        ] {
            assert!(matches!(
                csv.set_sub_name(0, value),
                Err(Error::Unsupported { line: 2, .. })
            ));
            assert_eq!(csv.to_bytes(), before);
        }
    }

    #[test]
    fn sub_name_last_row_and_error_priority_preserve_opaque_cells() {
        let row = b"1,G,\x82\xa0,LONG-NAME,,,f,dup,offset,mode,tone,rtone,use,pos,lat,lon,utc\r\n";
        let mut source = HEADER.to_vec();
        for _ in 0..2500 {
            source.extend_from_slice(row);
        }
        let mut csv = Csv::parse(&source).unwrap();
        assert_eq!(csv.rows()[2499].sub_name_bytes(), b"LONG-NAME");
        let at = HEADER.len() + 2499 * row.len() + b"1,G,\x82\xa0,".len();
        assert_eq!(
            csv.rows()[2499].sub_name_bytes().as_ptr(),
            source[at..].as_ptr()
        );
        csv.set_sub_name(2499, b" LAST x").unwrap();
        let mut expected = HEADER.to_vec();
        for _ in 0..2499 {
            expected.extend_from_slice(row);
        }
        expected.extend_from_slice(
            b"1,G,\x82\xa0, LAST x,,,f,dup,offset,mode,tone,rtone,use,pos,lat,lon,utc\r\n",
        );
        assert_eq!(csv.to_bytes(), expected);
        assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
        assert!(matches!(
            csv.set_sub_name(2499, b"123456789"),
            Err(Error::Unsupported { line: 2501, .. })
        ));
        for index in [2500, usize::MAX] {
            assert_eq!(
                csv.set_sub_name(index, b"\xffinvalid-long"),
                Err(Error::RowIndexOutOfBounds { index })
            );
            assert_eq!(csv.to_bytes(), expected);
        }
        let mut empty = Csv::parse(HEADER).unwrap();
        assert_eq!(
            empty.set_sub_name(0, b"\0"),
            Err(Error::RowIndexOutOfBounds { index: 0 })
        );
        assert_eq!(empty.to_bytes(), HEADER);
    }

    #[test]
    fn lossless_name_edit_matches_independent_bytes() {
        let mut csv = Csv::parse(SAMPLE).unwrap();
        assert_eq!(csv.rows().len(), 2);
        assert_eq!(csv.rows()[0].name_bytes(), b"FM test");
        assert_eq!(csv.rows()[1].cells()[4], b"TEST01 B");
        assert_eq!(csv.to_bytes(), SAMPLE);

        csv.set_name(0, b"New name").unwrap();
        let expected = concat!(
            "Group No,Group Name,Name,Sub Name,Repeater Call Sign,Gateway Call Sign,Frequency,Dup,Offset,Mode,TONE,Repeater Tone,RPT1USE,Position,Latitude,Longitude,UTC Offset\r\n",
            "1,GROUP1,New name,FM,,,145.000000,DUP-,0.600000,FM,TONE,88.5Hz,NO,Approximate,-35.500000,139.750000,0:00\r\n",
            "50,GROUP50,DV test,DV,TEST01 B,TEST01 G,439.000000,DUP-,5.000000,DV,,,YES,Exact,35.500000,139.750000,+9:00\r\n",
        ).as_bytes();
        assert_eq!(csv.to_bytes(), expected);
        assert_eq!(
            Csv::parse(expected).unwrap().rows()[0].name_bytes(),
            b"New name"
        );
        assert_eq!(csv.rows()[0].cells()[NAME], b"New name");
    }

    #[test]
    fn opaque_cp932_survives_and_failed_edits_are_atomic() {
        let source = [
            HEADER,
            b"1,\x83\x5c,\x82\xa0\x95\x5c,FM,,,145.000000,DUP-,0.600000,FM,TONE,88.5Hz,NO,Approximate,-35.500000,139.750000,0:00\r\n",
            b"50,GROUP50,DV test,DV,TEST01 B,TEST01 G,439.000000,DUP-,5.000000,DV,,,YES,Exact,35.500000,139.750000,+9:00\r\n",
        ].concat();
        let mut csv = Csv::parse(&source).unwrap();
        assert_eq!(csv.rows()[0].name_bytes(), b"\x82\xa0\x95\x5c");
        assert_eq!(csv.rows()[0].cells()[1], b"\x83\x5c");
        assert_eq!(csv.to_bytes(), source);
        csv.set_name(1, b"1234567890123456").unwrap();
        let expected = [
            HEADER,
            b"1,\x83\x5c,\x82\xa0\x95\x5c,FM,,,145.000000,DUP-,0.600000,FM,TONE,88.5Hz,NO,Approximate,-35.500000,139.750000,0:00\r\n",
            b"50,GROUP50,1234567890123456,DV,TEST01 B,TEST01 G,439.000000,DUP-,5.000000,DV,,,YES,Exact,35.500000,139.750000,+9:00\r\n",
        ].concat();
        assert_eq!(csv.to_bytes(), expected);
        for name in [
            b"12345678901234567".as_slice(),
            b"\x82\xa0",
            b"\0",
            b"\r",
            b"\n",
            b"\t",
            b"\x7f",
            b"a,b",
            b"a;b",
            b"a\"b",
            b"tail ",
        ] {
            assert!(matches!(
                csv.set_name(0, name),
                Err(Error::Unsupported { line: 2, .. })
            ));
            assert_eq!(csv.to_bytes(), expected);
        }
        assert_eq!(
            csv.set_name(usize::MAX, b"x"),
            Err(Error::RowIndexOutOfBounds { index: usize::MAX })
        );
        assert_eq!(csv.to_bytes(), expected);
        csv.set_name(0, b"").unwrap();
        assert!(csv.rows()[0].name_bytes().is_empty());
    }

    #[test]
    fn rejects_unsupported_or_malformed_structure() {
        assert!(matches!(
            Csv::parse(b""),
            Err(Error::Invalid { line: 1, .. })
        ));
        for header in [
            b"\xef\xbb\xbf".as_slice(),
            b"\xff\xfe",
            b"Group Name,Group No,Name\r\n",
            &HEADER[..HEADER.len() - 2],
        ] {
            assert!(matches!(
                Csv::parse(header),
                Err(Error::Unsupported { line: 1, .. })
            ));
        }
        let lf: Vec<u8> = SAMPLE
            .iter()
            .copied()
            .filter(|byte| *byte != b'\r')
            .collect();
        assert!(matches!(
            Csv::parse(&lf),
            Err(Error::Unsupported { line: 1, .. })
        ));
        for row in [
            b"\r\n".as_slice(),
            b"1,GROUP1,name\r\n",
            b"1,GROUP1,name,FM,,,145,DUP-,0.6,FM,TONE,88.5Hz,NO,Exact,35,139,0:00,extra\r\n",
        ] {
            let input = [HEADER, row].concat();
            assert!(matches!(
                Csv::parse(&input),
                Err(Error::Invalid { line: 2, .. })
            ));
        }
        for name in [
            b"\"quoted\"".as_slice(),
            b"a;b",
            b"a\0b",
            b"a\tb",
            b"a\x7fb",
            b"a\rb",
            b"a\nb",
            b"12345678901234567",
        ] {
            let input = [
                HEADER,
                b"1,GROUP1,",
                name,
                b",FM,,,145,DUP-,0.6,FM,TONE,88.5Hz,NO,Exact,35,139,0:00\r\n",
            ]
            .concat();
            assert!(matches!(
                Csv::parse(&input),
                Err(Error::Unsupported { line: 2, .. })
            ));
        }
        for row in [
            b"1,group,name\n".as_slice(),
            b"1,group,name\r",
            b"1,group,name",
        ] {
            let input = [HEADER, row].concat();
            assert!(matches!(
                Csv::parse(&input),
                Err(Error::Unsupported { line: 2, .. })
            ));
        }
        let extra_blank = [SAMPLE, b"\r\n"].concat();
        assert!(matches!(
            Csv::parse(&extra_blank),
            Err(Error::Invalid { line: 4, .. })
        ));
    }

    #[test]
    fn enforces_row_and_raw_name_byte_limits_without_value_conversion() {
        let empty = Csv::parse(HEADER).unwrap();
        assert!(empty.rows().is_empty());
        assert_eq!(empty.to_bytes(), HEADER);
        let row = b"1,opaque,\x82\xa0\x82\xa0\x82\xa0\x82\xa0\x82\xa0\x82\xa0\x82\xa0\x82\xa0,,,,unchanged,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque\r\n";
        let mut source = HEADER.to_vec();
        for _ in 0..2500 {
            source.extend_from_slice(row);
        }
        let csv = Csv::parse(&source).unwrap();
        assert_eq!(csv.rows().len(), 2500);
        assert_eq!(csv.rows()[0].name_bytes().len(), 16);
        assert_eq!(csv.rows()[0].cells()[6], b"unchanged");
        assert_eq!(csv.to_bytes(), source);
        source.extend_from_slice(row);
        assert!(matches!(
            Csv::parse(&source),
            Err(Error::Invalid { line: 2502, .. })
        ));
    }
}
