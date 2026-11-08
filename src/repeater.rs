//! Lossless explicit column edits of a limited repeater CSV format.
//!
//! Only the exact 17-column export header, comma separators, unquoted cells and
//! CRLF-terminated physical lines are supported. BOMs, quotes, semicolons, ASCII
//! control bytes and DEL are rejected. Every data row must have 17 cells, its
//! Name must fit in 16 bytes, and there may be at most 2500 data rows.
//!
//! Existing cell bytes are opaque, not decoded as UTF-8 or validated as CP932.
//! Parsing does not check numeric formats, modes, groups, units or radio suitability.
//! New Name/Sub Name values use narrower ASCII-only limits of 16/8 bytes.
//! Frequency/Offset edits accept integer Hz and emit exactly six decimal MHz
//! places using integer division/remainder, not the original float/locale codec.
//! Named enum/UTC setters edit one raw column only; they do not reproduce source
//! row-export coupling, clear companion cells or certify radio suitability.
//! Group Name/callsign edits are local ASCII replacements, not group renames,
//! ICF gateway allocation/unlink or original-OS character normalization.
//! Group No and typed-coordinate setters also replace one cell only. Coordinate
//! output is an integer canonical subset, not a general decimal/RTL float codec.
//! Sub Name, Hz, enum and UTC edits have not been compared with the original GUI.
//! No source application normalization, truncation, padding or defaulting is performed.
//! Successful parsing establishes structure only, not general CS-705
//! compatibility. For one fixed native export, DV/FM Name edits matched native
//! GUI edits and native import/re-export byte-for-byte. Other inputs and fields
//! are not covered by that comparison.

use crate::icf::{
    RepeaterCoordinate, RepeaterDuplex, RepeaterMode, RepeaterPosition, RepeaterTone,
};
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

// Public frequencies in tenths of Hz, sorted from the Icom IC-705 Advanced
// Manual, printed pages 2-14 and 2-17 (Selectable tone frequencies).
const REPEATER_TONES: [u16; 50] = [
    670, 693, 719, 744, 770, 797, 825, 854, 885, 915, 948, 974, 1000, 1035, 1072, 1109, 1148, 1188,
    1230, 1273, 1318, 1365, 1413, 1462, 1514, 1567, 1598, 1622, 1655, 1679, 1713, 1738, 1773, 1799,
    1835, 1862, 1899, 1928, 1966, 1995, 2035, 2065, 2107, 2181, 2257, 2291, 2336, 2418, 2503, 2541,
];

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

/// A CSV document borrowing unchanged cells and retaining explicit column edits.
///
/// Rows cannot be added, removed or reordered through this API. All bytes outside
/// explicitly edited cells, including empty cells and CRLFs, are retained.
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

    /// Replace only Frequency (zero-based column 6) with canonical six-place MHz.
    ///
    /// Accepts integer Hz in 30000..=1399999999. Empty/unknown old numeric cells
    /// may be replaced. No float parsing, locale, rounding or companion correction
    /// occurs; this is not a claim of original-GUI float-formatter compatibility.
    ///
    /// # Errors
    /// Row bounds take priority over value validation. Unsupported Hz leave every
    /// cell unchanged. This edit has not been compared with the original GUI.
    pub fn set_frequency_hz(&mut self, index: usize, hz: u32) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        if !(30_000..=1_399_999_999).contains(&hz) {
            return Err(Error::Unsupported {
                line: index + 2,
                reason: "Frequency requires 30000..=1399999999 integer Hz",
            });
        }
        row.cells[6] = Cow::Owned(format!("{}.{:06}", hz / 1_000_000, hz % 1_000_000).into_bytes());
        Ok(())
    }

    /// Replace only Offset (zero-based column 8) with canonical six-place MHz.
    ///
    /// Accepts 0..=9999900 integer Hz in steps of 100. Empty/unknown old cells may
    /// be replaced. No band inference, default, quantization or float codec is used.
    ///
    /// # Errors
    /// Row bounds take priority over value validation. Unsupported Hz leave every
    /// cell unchanged. This edit has not been compared with the original GUI.
    pub fn set_offset_hz(&mut self, index: usize, hz: u32) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        if hz > 9_999_900 || !hz.is_multiple_of(100) {
            return Err(Error::Unsupported {
                line: index + 2,
                reason: "Offset requires 0..=9999900 integer Hz in steps of 100",
            });
        }
        row.cells[8] = Cow::Owned(format!("{}.{:06}", hz / 1_000_000, hz % 1_000_000).into_bytes());
        Ok(())
    }

    /// Replace only column 9 with FM or DV. No tone/companion-cell correction.
    /// This raw column subset is not the original row exporter or radio validation.
    /// # Errors
    /// Row bounds take priority; FM-N/DD are Unsupported. Errors preserve all cells.
    pub fn set_mode(&mut self, index: usize, value: RepeaterMode) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        let text: &[u8] = match value {
            RepeaterMode::Fm => b"FM",
            RepeaterMode::Dv => b"DV",
            _ => {
                return Err(Error::Unsupported {
                    line: index + 2,
                    reason: "CSV Mode edits support FM or DV only",
                });
            }
        };
        row.cells[9] = Cow::Borrowed(text);
        Ok(())
    }

    /// Replace only column 7 with OFF/DUP-/DUP+, without mode/offset inference.
    /// This is a raw column edit, not the original row's coupled normalization.
    /// # Errors
    /// Row bounds take priority; RPS is Unsupported. No cell changes on error.
    pub fn set_duplex(&mut self, index: usize, value: RepeaterDuplex) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        let text: &[u8] = match value {
            RepeaterDuplex::Off => b"OFF",
            RepeaterDuplex::Minus => b"DUP-",
            RepeaterDuplex::Plus => b"DUP+",
            RepeaterDuplex::Rps => {
                return Err(Error::Unsupported {
                    line: index + 2,
                    reason: "CSV Dup edits do not support RPS",
                });
            }
        };
        row.cells[7] = Cow::Borrowed(text);
        Ok(())
    }

    /// Replace only column 10 with OFF/TONE/TSQL, even when Mode is opaque/DV.
    /// No mode decoding, blanking or tone-index correction occurs.
    /// # Errors
    /// Rejects an invalid row index before any edit.
    pub fn set_tone(&mut self, index: usize, value: RepeaterTone) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        let text: &[u8] = match value {
            RepeaterTone::Off => b"OFF",
            RepeaterTone::Tone => b"TONE",
            RepeaterTone::ToneSquelch => b"TSQL",
        };
        row.cells[10] = Cow::Borrowed(text);
        Ok(())
    }

    /// Replace only Repeater Tone (column 11) with the saved index's canonical Hz text.
    ///
    /// Indices 0..=49 select the ascending public manual frequencies. This raw
    /// local edit does not decode the old tone, check FM suitability or reproduce
    /// GUI mode coupling, unknown-tone dialogs or special/default corrections.
    /// Mode, TONE and all other cells remain unchanged, even for DV/DD/opaque Mode.
    /// # Errors
    /// Row bounds take priority; unsupported new indices leave all cells intact.
    pub fn set_repeater_tone_index(&mut self, index: usize, tone_index: u8) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        let tone = REPEATER_TONES
            .get(usize::from(tone_index))
            .ok_or(Error::Unsupported {
                line: index + 2,
                reason: "CSV Repeater Tone requires a saved index in 0..=49",
            })?;
        row.cells[11] = Cow::Owned(format!("{}.{}Hz", tone / 10, tone % 10).into_bytes());
        Ok(())
    }

    /// Replace only column 12 with NO/YES; no callsign or group linkage is changed.
    /// # Errors
    /// Rejects an invalid row index before any edit.
    pub fn set_rpt1_use(&mut self, index: usize, value: bool) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        let text: &[u8] = if value { b"YES" } else { b"NO" };
        row.cells[12] = Cow::Borrowed(text);
        Ok(())
    }

    /// Replace only column 13 with None/Approximate/Exact. None never clears coordinates.
    /// This is a local raw-column preference, not full row/radio validation.
    /// # Errors
    /// Rejects an invalid row index before any edit.
    pub fn set_position(&mut self, index: usize, value: RepeaterPosition) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        let text: &[u8] = match value {
            RepeaterPosition::None => b"None",
            RepeaterPosition::Approximate => b"Approximate",
            RepeaterPosition::Exact => b"Exact",
        };
        row.cells[13] = Cow::Borrowed(text);
        Ok(())
    }

    /// Replace only column 16 with canonical signed h:mm from integer minutes.
    /// Accepts -840..=840 in 5-minute steps. Zero is 0:00, nonzero has a sign,
    /// hours are unpadded and minutes have two digits. No rounding/default/locale
    /// or interpretation of the old cell occurs; this is not a full CSV importer.
    /// # Errors
    /// Row bounds take priority; noncanonical input is Unsupported without changes.
    pub fn set_utc_offset_minutes(&mut self, index: usize, minutes: i16) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        if !(-840..=840).contains(&minutes) || minutes % 5 != 0 {
            return Err(Error::Unsupported {
                line: index + 2,
                reason: "CSV UTC Offset requires -840..=840 minutes in steps of 5",
            });
        }
        let magnitude = minutes.unsigned_abs();
        let sign = if minutes == 0 {
            ""
        } else if minutes < 0 {
            "-"
        } else {
            "+"
        };
        row.cells[16] =
            Cow::Owned(format!("{sign}{}:{:02}", magnitude / 60, magnitude % 60).into_bytes());
        Ok(())
    }

    /// Replace only this row's Group Name (column 1) with 0..=16 safe ASCII bytes.
    ///
    /// Leading/interior spaces and case are retained; trailing space, comma,
    /// semicolon and quote are unsupported. No trim/clip/pad/OS conversion occurs.
    /// This is a local raw edit, not a group rename or original-GUI compatibility:
    /// Group No, ICF groups and other rows with the same Group No remain unchanged.
    /// Unknown/overlength old cells may be explicitly replaced.
    /// # Errors
    /// Row bounds take priority over new input validation. Errors change no cells.
    pub fn set_group_name(&mut self, index: usize, name: &[u8]) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        if name.len() > 16
            || name
                .iter()
                .any(|b| !(b' '..=b'~').contains(b) || b",;\"".contains(b))
            || name.ends_with(b" ")
        {
            return Err(Error::Unsupported {
                line: index + 2,
                reason: "Group Name requires at most 16 safe ASCII bytes without trailing spaces",
            });
        }
        row.cells[1] = Cow::Owned(name.to_vec());
        Ok(())
    }

    /// Replace only Repeater Call Sign (column 4) with 0..=8 canonical ASCII bytes.
    ///
    /// Accepts space, slash, digits and A-Z, without trailing spaces. No uppercase,
    /// trim/clip/pad or OS character correction occurs. This raw text edit does
    /// not validate radio references, change mode or allocate ICF gateway links.
    /// Unknown/overlength old cells may be explicitly replaced.
    /// # Errors
    /// Row bounds take priority; unsupported new bytes/length leave all cells intact.
    pub fn set_repeater_call_sign(&mut self, index: usize, value: &[u8]) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        if value.len() > 8
            || !value
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || matches!(b, b' ' | b'/'))
            || value.ends_with(b" ")
        {
            return Err(Error::Unsupported {
                line: index + 2,
                reason: "Repeater Call Sign requires at most 8 canonical ASCII bytes without trailing spaces",
            });
        }
        row.cells[4] = Cow::Owned(value.to_vec());
        Ok(())
    }

    /// Replace only Gateway Call Sign (column 5) with 0..=8 canonical ASCII bytes.
    ///
    /// Accepts space, slash, digits and A-Z, without trailing spaces. Empty text
    /// clears this CSV cell only, not an ICF shared link. No allocation/unlink,
    /// companion mutation, trim/clip/pad/uppercase or original-OS mapping occurs.
    /// This local raw subset is not full GUI input or gateway-reference validation.
    /// # Errors
    /// Row bounds take priority; unsupported new bytes/length leave all cells intact.
    pub fn set_gateway_call_sign(&mut self, index: usize, value: &[u8]) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        if value.len() > 8
            || !value
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || matches!(b, b' ' | b'/'))
            || value.ends_with(b" ")
        {
            return Err(Error::Unsupported {
                line: index + 2,
                reason: "Gateway Call Sign requires at most 8 canonical ASCII bytes without trailing spaces",
            });
        }
        row.cells[5] = Cow::Owned(value.to_vec());
        Ok(())
    }

    /// Replace only Group No (column 0) with decimal 1..=50.
    /// This raw cell edit does not move rows, rename groups, propagate Group Name
    /// or change ICF group/reference metadata. Old cell contents are not decoded.
    /// # Errors
    /// Row bounds take priority; out-of-range new numbers are Unsupported.
    pub fn set_group_number(&mut self, index: usize, number: u8) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        if !(1..=50).contains(&number) {
            return Err(Error::Unsupported {
                line: index + 2,
                reason: "CSV Group No requires 1..=50",
            });
        }
        row.cells[0] = Cow::Owned(number.to_string().into_bytes());
        Ok(())
    }

    /// Replace only Latitude (column 14) using typed integer components.
    ///
    /// Degrees are 0..=90, ticks below 60000 (0.001 minute), and a pole requires
    /// zero ticks. Fractional microdegrees use (ticks*50+1)/3: integer nearest
    /// six-place output with no half ties. Negative zero retains a minus prefix.
    /// No Position/longitude decoding or correction, decimal parser, float/locale
    /// formatter or original-RTL/GUI boundary equivalence is claimed.
    /// # Errors
    /// Row bounds take priority; invalid new components are Unsupported, with no changes.
    pub fn set_latitude(&mut self, index: usize, value: RepeaterCoordinate) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        if value.degrees > 90
            || value.thousandths_of_minute >= 60000
            || (value.degrees == 90 && value.thousandths_of_minute != 0)
        {
            return Err(Error::Unsupported {
                line: index + 2,
                reason: "CSV Latitude outside degree/tick domain",
            });
        }
        let fraction = (u32::from(value.thousandths_of_minute) * 50 + 1) / 3;
        let sign = if value.negative { "-" } else { "" };
        row.cells[14] = Cow::Owned(format!("{sign}{}.{fraction:06}", value.degrees).into_bytes());
        Ok(())
    }

    /// Replace only Longitude (column 15), preserving negative zero and other cells.
    ///
    /// Degrees are 0..=180; ticks are below 60000 and zero at a pole. Integer
    /// (ticks*50+1)/3 gives nearest six-place fractional degrees, without a carry
    /// to the next degree. This intentional canonical subset does not reproduce
    /// arbitrary decimal input, float/locale/RTL rounding or GUI row correction.
    /// # Errors
    /// Row bounds take priority; invalid new components are Unsupported, with no changes.
    pub fn set_longitude(&mut self, index: usize, value: RepeaterCoordinate) -> Result<(), Error> {
        let row = self
            .rows
            .get_mut(index)
            .ok_or(Error::RowIndexOutOfBounds { index })?;
        if value.degrees > 180
            || value.thousandths_of_minute >= 60000
            || (value.degrees == 180 && value.thousandths_of_minute != 0)
        {
            return Err(Error::Unsupported {
                line: index + 2,
                reason: "CSV Longitude outside degree/tick domain",
            });
        }
        let fraction = (u32::from(value.thousandths_of_minute) * 50 + 1) / 3;
        let sign = if value.negative { "-" } else { "" };
        row.cells[15] = Cow::Owned(format!("{sign}{}.{fraction:06}", value.degrees).into_bytes());
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

    fn csv_coordinate(degrees: u8, ticks: u16, negative: bool) -> RepeaterCoordinate {
        RepeaterCoordinate {
            degrees,
            thousandths_of_minute: ticks,
            negative,
        }
    }

    fn coordinate_csv_literal(changes: &[(usize, &[u8])]) -> Vec<u8> {
        // Literal expected cells, not a product row/setter. Position is None
        // while both old coordinate cells and the old Group No are opaque.
        let cells: [&[u8]; 17] = [
            b"\x81\x40No",
            b"\x83\x5cGroup",
            b"\x82\xa0",
            b"Sub",
            b"CALL",
            b"GATE",
            b"freq",
            b"dup",
            b"offset",
            b"mode",
            b"tone",
            b"rtone",
            b"use",
            b"None",
            b"unknown-lat",
            b"\x82\xa2lon",
            b"utc",
        ];
        let mut text = HEADER.to_vec();
        for (column, &cell) in cells.iter().enumerate() {
            if column != 0 {
                text.push(b',');
            }
            text.extend_from_slice(
                changes
                    .iter()
                    .find(|(c, _)| *c == column)
                    .map_or(cell, |(_, value)| *value),
            );
        }
        text.extend_from_slice(b"\r\n1,Keep Group,Keep Name,Keep Sub,OLD CALL,OLD GATE,oldfreq,olddup,oldoffset,oldmode,oldtone,oldrtone,olduse,None,oldlat,oldlon,oldutc\r\n");
        text
    }

    #[test]
    fn csv_integer_coordinates_and_group_number_match_literals_and_all_tick_rounding() {
        let source = coordinate_csv_literal(&[]);
        let original = source.clone();
        macro_rules! check {
            ($column:literal,$method:ident,$cases:expr) => {
                for (value, text) in $cases {
                    let mut csv = Csv::parse(&source).unwrap();
                    let pointers: Vec<_> = csv
                        .rows()
                        .iter()
                        .map(|r| r.cells().map(|c| c.as_ptr()))
                        .collect();
                    csv.$method(0, value).unwrap();
                    let expected = coordinate_csv_literal(&[($column, text)]);
                    assert_eq!(csv.to_bytes(), expected);
                    for (r, row) in csv.rows().iter().enumerate() {
                        for (c, cell) in row.cells().iter().enumerate() {
                            if (r, c) != (0, $column) {
                                assert_eq!(cell.as_ptr(), pointers[r][c]);
                            }
                        }
                    }
                    assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
                }
            };
        }
        check!(0, set_group_number, [(1, b"1".as_slice()), (50, b"50")]);
        check!(
            14,
            set_latitude,
            [
                (csv_coordinate(0, 0, false), b"0.000000".as_slice()),
                (csv_coordinate(0, 0, true), b"-0.000000"),
                (csv_coordinate(90, 0, false), b"90.000000"),
                (csv_coordinate(90, 0, true), b"-90.000000"),
                (csv_coordinate(12, 1, false), b"12.000017"),
                (csv_coordinate(12, 2, false), b"12.000033"),
                (csv_coordinate(12, 3, true), b"-12.000050"),
                (csv_coordinate(89, 59999, false), b"89.999983"),
            ]
        );
        check!(
            15,
            set_longitude,
            [
                (csv_coordinate(0, 0, false), b"0.000000".as_slice()),
                (csv_coordinate(0, 0, true), b"-0.000000"),
                (csv_coordinate(180, 0, false), b"180.000000"),
                (csv_coordinate(180, 0, true), b"-180.000000"),
                (csv_coordinate(179, 59999, true), b"-179.999983"),
            ]
        );
        let mut csv = Csv::parse(&source).unwrap();
        let pointers: Vec<_> = csv
            .rows()
            .iter()
            .map(|r| r.cells().map(|c| c.as_ptr()))
            .collect();
        // One parse, then all raw ticks. Quotient/remainder rounding is independent
        // of the product's biased numerator. This proves only the integer grid.
        for ticks in 0..60000_u32 {
            let numerator = ticks * 50;
            let quotient = numerator / 3;
            let remainder = numerator % 3;
            assert_ne!(2 * remainder, 3);
            let nearest = quotient + u32::from(2 * remainder > 3);
            assert!(nearest < 1000000);
            assert_eq!((6 * nearest + 3) / 100, ticks);
            let negative = ticks % 2 != 0;
            csv.set_latitude(0, csv_coordinate(37, ticks as u16, negative))
                .unwrap();
            csv.set_longitude(0, csv_coordinate(139, ticks as u16, !negative))
                .unwrap();
            let lat = format!("{}37.{nearest:06}", if negative { "-" } else { "" });
            let lon = format!("{}139.{nearest:06}", if negative { "" } else { "-" });
            assert_eq!(csv.rows()[0].cells()[14], lat.as_bytes());
            assert_eq!(csv.rows()[0].cells()[15], lon.as_bytes());
        }
        for (r, row) in csv.rows().iter().enumerate() {
            for (c, cell) in row.cells().iter().enumerate() {
                if r != 0 || ![14, 15].contains(&c) {
                    assert_eq!(cell.as_ptr(), pointers[r][c]);
                }
            }
        }
        let mut csv = Csv::parse(&source).unwrap();
        csv.set_group_number(0, 50).unwrap();
        csv.set_latitude(0, csv_coordinate(0, 0, true)).unwrap();
        csv.set_longitude(0, csv_coordinate(179, 59999, true))
            .unwrap();
        csv.set_group_name(0, b"Local Group").unwrap();
        csv.set_repeater_call_sign(0, b" CALL/1").unwrap();
        csv.set_gateway_call_sign(0, b" GATE/2").unwrap();
        csv.set_name(0, b"Name").unwrap();
        csv.set_sub_name(0, b"Sub").unwrap();
        csv.set_frequency_hz(0, 145000000).unwrap();
        csv.set_offset_hz(0, 600000).unwrap();
        csv.set_mode(0, RepeaterMode::Dv).unwrap();
        csv.set_duplex(0, RepeaterDuplex::Off).unwrap();
        csv.set_tone(0, RepeaterTone::ToneSquelch).unwrap();
        csv.set_rpt1_use(0, true).unwrap();
        csv.set_position(0, RepeaterPosition::None).unwrap();
        csv.set_utc_offset_minutes(0, 330).unwrap();
        let expected=[HEADER,b"50,Local Group,Name,Sub, CALL/1, GATE/2,145.000000,OFF,0.600000,DV,TSQL,rtone,YES,None,-0.000000,-179.999983,+5:30\r\n",b"1,Keep Group,Keep Name,Keep Sub,OLD CALL,OLD GATE,oldfreq,olddup,oldoffset,oldmode,oldtone,oldrtone,olduse,None,oldlat,oldlon,oldutc\r\n"].concat();
        assert_eq!(csv.to_bytes(), expected);
        assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
        assert_eq!(source, original);
    }

    #[test]
    fn csv_group_coordinate_last_row_domains_and_bounds_are_atomic() {
        let row =
            b"1,Keep Group,Name,Sub,CALL,GATE,f,d,o,m,t,r,u,None,unknown-lat,unknown-lon,utc\r\n";
        let mut source = HEADER.to_vec();
        for _ in 0..2500 {
            source.extend_from_slice(row);
        }
        let mut csv = Csv::parse(&source).unwrap();
        let pointers: Vec<_> = csv
            .rows()
            .iter()
            .map(|r| r.cells().map(|c| c.as_ptr()))
            .collect();
        csv.set_group_number(2499, 50).unwrap();
        csv.set_latitude(2499, csv_coordinate(90, 0, true)).unwrap();
        csv.set_longitude(2499, csv_coordinate(180, 0, false))
            .unwrap();
        let mut expected = HEADER.to_vec();
        for _ in 0..2499 {
            expected.extend_from_slice(row);
        }
        expected.extend_from_slice(
            b"50,Keep Group,Name,Sub,CALL,GATE,f,d,o,m,t,r,u,None,-90.000000,180.000000,utc\r\n",
        );
        assert_eq!(csv.to_bytes(), expected);
        for (r, row) in csv.rows().iter().enumerate() {
            for (c, cell) in row.cells().iter().enumerate() {
                if r != 2499 || ![0, 14, 15].contains(&c) {
                    assert_eq!(cell.as_ptr(), pointers[r][c]);
                }
            }
        }
        for index in [0, 2499] {
            for number in [0, 51, 255] {
                assert!(
                    matches!(csv.set_group_number(index,number),Err(Error::Unsupported {line,..}) if line==index+2)
                );
                assert_eq!(csv.to_bytes(), expected);
            }
            for value in [
                csv_coordinate(91, 0, false),
                csv_coordinate(90, 1, true),
                csv_coordinate(0, 60000, false),
                csv_coordinate(0, u16::MAX, true),
                csv_coordinate(u8::MAX, 0, true),
            ] {
                assert!(
                    matches!(csv.set_latitude(index,value),Err(Error::Unsupported {line,..}) if line==index+2)
                );
                assert_eq!(csv.to_bytes(), expected);
            }
            for value in [
                csv_coordinate(181, 0, false),
                csv_coordinate(180, 1, true),
                csv_coordinate(0, 60000, false),
                csv_coordinate(0, u16::MAX, true),
                csv_coordinate(u8::MAX, 0, true),
            ] {
                assert!(
                    matches!(csv.set_longitude(index,value),Err(Error::Unsupported {line,..}) if line==index+2)
                );
                assert_eq!(csv.to_bytes(), expected);
            }
        }
        let invalid = csv_coordinate(255, 65535, true);
        for index in [2500, usize::MAX] {
            let error = Err(Error::RowIndexOutOfBounds { index });
            assert_eq!(csv.set_group_number(index, 0), error);
            assert_eq!(csv.set_latitude(index, invalid), error);
            assert_eq!(csv.set_longitude(index, invalid), error);
            assert_eq!(csv.to_bytes(), expected);
        }
        let mut empty = Csv::parse(HEADER).unwrap();
        let error = Err(Error::RowIndexOutOfBounds { index: 0 });
        assert_eq!(empty.set_group_number(0, 255), error);
        assert_eq!(empty.set_latitude(0, invalid), error);
        assert_eq!(empty.set_longitude(0, invalid), error);
        assert_eq!(empty.to_bytes(), HEADER);
        assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
    }

    fn text_csv_literal(change: Option<(usize, &[u8])>) -> Vec<u8> {
        // Independent cell literals, including overlength/opaque old text and
        // a second row with the same Group No. No product row/setter is an oracle.
        let cells: [&[u8]; 17] = [
            b"1",
            b"\x83\x5c-opaque-group-over16",
            b"\x82\xa2",
            b"opaque-long",
            b"\x82\xa0-old-repeater-long",
            b"\x81\x40-old-gateway-long",
            b"freq",
            b"dup",
            b"offset",
            b"mode",
            b"tone",
            b"rtone",
            b"use",
            b"pos",
            b"lat",
            b"lon",
            b"utc",
        ];
        let mut text = HEADER.to_vec();
        for (c, &cell) in cells.iter().enumerate() {
            if c != 0 {
                text.push(b',');
            }
            text.extend_from_slice(match change {
                Some((column, value)) if column == c => value,
                _ => cell,
            });
        }
        text.extend_from_slice(b"\r\n1,Keep Group,Keep Name,Keep Sub,OLD CALL,OLD GATE,oldfreq,olddup,oldoffset,oldmode,oldtone,oldrtone,olduse,oldpos,oldlat,oldlon,oldutc\r\n");
        text
    }

    #[test]
    fn csv_ascii_text_edits_preserve_other_cells_rows_and_raw_borrows() {
        let source = text_csv_literal(None);
        let original = source.clone();
        assert_eq!(Csv::parse(&source).unwrap().to_bytes(), source);
        macro_rules! check {
            ($column:literal,$method:ident,$values:expr) => {
                for value in $values {
                    let mut csv = Csv::parse(&source).unwrap();
                    let pointers: Vec<_> = csv
                        .rows()
                        .iter()
                        .map(|r| r.cells().map(|c| c.as_ptr()))
                        .collect();
                    csv.$method(0, value).unwrap();
                    let expected = text_csv_literal(Some(($column, value)));
                    assert_eq!(csv.to_bytes(), expected);
                    assert_eq!(csv.rows()[0].cells()[$column], value);
                    for (r, row) in csv.rows().iter().enumerate() {
                        for (c, cell) in row.cells().iter().enumerate() {
                            if (r, c) != (0, $column) {
                                assert_eq!(cell.as_ptr(), pointers[r][c]);
                            }
                        }
                    }
                    let parsed = Csv::parse(&expected).unwrap();
                    assert_eq!(parsed.to_bytes(), expected);
                }
            };
        }
        check!(
            1,
            set_group_name,
            [
                b"".as_slice(),
                b"0123456789AbCdEf",
                b" a B",
                b"!#$%&'()*+-./:<?"
            ]
        );
        check!(
            4,
            set_repeater_call_sign,
            [b"".as_slice(), b"A0/B1C2D", b" A/B0Z", b" A B"]
        );
        check!(
            5,
            set_gateway_call_sign,
            [b"".as_slice(), b"A0/B1C2G", b" A/B0Z", b" A B"]
        );
        for byte in 0..=255_u8 {
            let value = [byte, b'Z'];
            let group_ok = (32..=126).contains(&byte) && !b",;\"".contains(&byte);
            let call_ok =
                byte.is_ascii_uppercase() || byte.is_ascii_digit() || b" /".contains(&byte);
            for column in [1, 4, 5] {
                let mut csv = Csv::parse(&source).unwrap();
                let result = match column {
                    1 => csv.set_group_name(0, &value),
                    4 => csv.set_repeater_call_sign(0, &value),
                    _ => csv.set_gateway_call_sign(0, &value),
                };
                if if column == 1 { group_ok } else { call_ok } {
                    result.unwrap();
                    assert_eq!(csv.to_bytes(), text_csv_literal(Some((column, &value))));
                } else {
                    assert!(matches!(result, Err(Error::Unsupported { line: 2, .. })));
                    assert_eq!(csv.to_bytes(), source);
                }
            }
        }
        let mut csv = Csv::parse(&source).unwrap();
        csv.set_group_name(0, b"Local Group").unwrap();
        csv.set_repeater_call_sign(0, b" CALL/1").unwrap();
        csv.set_gateway_call_sign(0, b" GATE/2").unwrap();
        csv.set_name(0, b"Name").unwrap();
        csv.set_sub_name(0, b"Sub").unwrap();
        csv.set_frequency_hz(0, 30000).unwrap();
        csv.set_offset_hz(0, 100).unwrap();
        csv.set_mode(0, RepeaterMode::Dv).unwrap();
        csv.set_duplex(0, RepeaterDuplex::Off).unwrap();
        csv.set_tone(0, RepeaterTone::ToneSquelch).unwrap();
        csv.set_rpt1_use(0, true).unwrap();
        csv.set_position(0, RepeaterPosition::None).unwrap();
        csv.set_utc_offset_minutes(0, -330).unwrap();
        let expected=[HEADER,b"1,Local Group,Name,Sub, CALL/1, GATE/2,0.030000,OFF,0.000100,DV,TSQL,rtone,YES,None,lat,lon,-5:30\r\n",b"1,Keep Group,Keep Name,Keep Sub,OLD CALL,OLD GATE,oldfreq,olddup,oldoffset,oldmode,oldtone,oldrtone,olduse,oldpos,oldlat,oldlon,oldutc\r\n"].concat();
        assert_eq!(csv.to_bytes(), expected);
        assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
        assert_eq!(source, original);
    }

    #[test]
    fn csv_ascii_text_bounds_late_errors_and_last_row_are_atomic() {
        let row=b"1,OLD-GROUP-LONG-OPAQUE,Name,Sub,OLD-REPEATER-LONG,OLD-GATEWAY-LONG,f,d,o,m,t,r,u,p,lat,lon,utc\r\n";
        let mut source = HEADER.to_vec();
        for _ in 0..2500 {
            source.extend_from_slice(row);
        }
        let mut csv = Csv::parse(&source).unwrap();
        let pointers: Vec<_> = csv
            .rows()
            .iter()
            .map(|r| r.cells().map(|c| c.as_ptr()))
            .collect();
        csv.set_group_name(2499, b"0123456789ABCDEF").unwrap();
        csv.set_repeater_call_sign(2499, b"A0/B1C2D").unwrap();
        csv.set_gateway_call_sign(2499, b"A0/B1C2G").unwrap();
        let mut expected = HEADER.to_vec();
        for _ in 0..2499 {
            expected.extend_from_slice(row);
        }
        expected.extend_from_slice(
            b"1,0123456789ABCDEF,Name,Sub,A0/B1C2D,A0/B1C2G,f,d,o,m,t,r,u,p,lat,lon,utc\r\n",
        );
        assert_eq!(csv.to_bytes(), expected);
        assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
        for (r, row) in csv.rows().iter().enumerate() {
            for (c, cell) in row.cells().iter().enumerate() {
                if r != 2499 || ![1, 4, 5].contains(&c) {
                    assert_eq!(cell.as_ptr(), pointers[r][c]);
                }
            }
        }
        for index in [0, 2499] {
            for value in [
                b"0123456789ABCDEFG".as_slice(),
                b"GOOD ",
                b" GOOD ",
                b"GOOD,",
                b"GOOD;",
                b"GOOD\"",
                b"GOOD\0",
                b"GOOD\x7f",
                b"GOOD\xff",
            ] {
                assert!(
                    matches!(csv.set_group_name(index,value),Err(Error::Unsupported {line,..}) if line==index+2)
                );
                assert_eq!(csv.to_bytes(), expected);
            }
            for value in [
                b"ABCDEFGHI".as_slice(),
                b"CALL ",
                b" CALL ",
                b"CALLa",
                b"CALL_",
                b"CALL*",
                b"CALL#",
                b"CALL,",
                b"CALL;",
                b"CALL\"",
                b"CALL\0",
                b"CALL\x7f",
                b"CALL\xff",
            ] {
                assert!(
                    matches!(csv.set_repeater_call_sign(index,value),Err(Error::Unsupported {line,..}) if line==index+2)
                );
                assert_eq!(csv.to_bytes(), expected);
                assert!(
                    matches!(csv.set_gateway_call_sign(index,value),Err(Error::Unsupported {line,..}) if line==index+2)
                );
                assert_eq!(csv.to_bytes(), expected);
            }
        }
        for index in [2500, usize::MAX] {
            let error = Err(Error::RowIndexOutOfBounds { index });
            assert_eq!(csv.set_group_name(index, b"\xffinvalid-overlength"), error);
            assert_eq!(
                csv.set_repeater_call_sign(index, b"\xffinvalid-long"),
                error
            );
            assert_eq!(csv.set_gateway_call_sign(index, b"\xffinvalid-long"), error);
            assert_eq!(csv.to_bytes(), expected);
        }
        let mut empty = Csv::parse(HEADER).unwrap();
        let error = Err(Error::RowIndexOutOfBounds { index: 0 });
        assert_eq!(empty.set_group_name(0, b"\0"), error);
        assert_eq!(empty.set_repeater_call_sign(0, b"a"), error);
        assert_eq!(empty.set_gateway_call_sign(0, b"a"), error);
        assert_eq!(empty.to_bytes(), HEADER);
    }

    #[test]
    fn repeater_tone_indices_match_public_frequencies_and_edit_only_one_cell() {
        // Independently transcribed decimal labels from the public manual's
        // printed 2-14/2-17 tables, not formatted from REPEATER_TONES.
        let expected_tones: [&[u8]; 50] = [
            b"67.0Hz", b"69.3Hz", b"71.9Hz", b"74.4Hz", b"77.0Hz", b"79.7Hz", b"82.5Hz", b"85.4Hz",
            b"88.5Hz", b"91.5Hz", b"94.8Hz", b"97.4Hz", b"100.0Hz", b"103.5Hz", b"107.2Hz",
            b"110.9Hz", b"114.8Hz", b"118.8Hz", b"123.0Hz", b"127.3Hz", b"131.8Hz", b"136.5Hz",
            b"141.3Hz", b"146.2Hz", b"151.4Hz", b"156.7Hz", b"159.8Hz", b"162.2Hz", b"165.5Hz",
            b"167.9Hz", b"171.3Hz", b"173.8Hz", b"177.3Hz", b"179.9Hz", b"183.5Hz", b"186.2Hz",
            b"189.9Hz", b"192.8Hz", b"196.6Hz", b"199.5Hz", b"203.5Hz", b"206.5Hz", b"210.7Hz",
            b"218.1Hz", b"225.7Hz", b"229.1Hz", b"233.6Hz", b"241.8Hz", b"250.3Hz", b"254.1Hz",
        ];
        let source = enum_csv_literal(None);
        let original = source.clone();
        for (tone_index, text) in expected_tones.iter().enumerate() {
            let mut csv = Csv::parse(&source).unwrap();
            let pointers: Vec<_> = csv
                .rows()
                .iter()
                .map(|r| r.cells().map(|c| c.as_ptr()))
                .collect();
            csv.set_repeater_tone_index(0, tone_index as u8).unwrap();
            let expected = enum_csv_literal(Some((11, text)));
            assert_eq!(csv.to_bytes(), expected);
            assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
            for (r, row) in csv.rows().iter().enumerate() {
                for (c, cell) in row.cells().iter().enumerate() {
                    if (r, c) != (0, 11) {
                        assert_eq!(cell.as_ptr(), pointers[r][c]);
                    }
                }
            }
        }
        assert_eq!(source, original);
        // Full row literals also exercise empty, unknown, long and opaque old
        // tones with DV/DD/unknown modes, without interpreting or clearing them.
        for (tone_index, old_row, new_row) in [
            (0, b"1,G,Name,Sub,CALL,GATE,f,d,o,DV,TSQL,\x82\xa2Hz,u,p,lat,lon,utc\r\n".as_slice(), b"1,G,Name,Sub,CALL,GATE,f,d,o,DV,TSQL,67.0Hz,u,p,lat,lon,utc\r\n".as_slice()),
            (8, b"1,G,Name,Sub,CALL,GATE,f,d,o,DD,TSQL,,u,p,lat,lon,utc\r\n", b"1,G,Name,Sub,CALL,GATE,f,d,o,DD,TSQL,88.5Hz,u,p,lat,lon,utc\r\n"),
            (24, b"1,G,Name,Sub,CALL,GATE,f,d,o,unknown,TSQL,overlength-old-tone-value,u,p,lat,lon,utc\r\n", b"1,G,Name,Sub,CALL,GATE,f,d,o,unknown,TSQL,151.4Hz,u,p,lat,lon,utc\r\n"),
            (49, b"1,G,Name,Sub,CALL,GATE,f,d,o,DV,TSQL,unknown,u,p,lat,lon,utc\r\n", b"1,G,Name,Sub,CALL,GATE,f,d,o,DV,TSQL,254.1Hz,u,p,lat,lon,utc\r\n"),
        ] {
            let source = [HEADER, old_row].concat();
            let mut csv = Csv::parse(&source).unwrap();
            csv.set_repeater_tone_index(0, tone_index).unwrap();
            let expected = [HEADER, new_row].concat();
            assert_eq!(csv.to_bytes(), expected);
            assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
            assert_eq!(source, [HEADER, old_row].concat());
        }

        let row = b"1,G,Name,Sub,CALL,GATE,f,d,o,DD,TONE,opaque-old-tone,u,p,lat,lon,utc\r\n";
        let mut source = HEADER.to_vec();
        for _ in 0..2500 {
            source.extend_from_slice(row);
        }
        let original = source.clone();
        let mut csv = Csv::parse(&source).unwrap();
        let pointers: Vec<_> = csv
            .rows()
            .iter()
            .map(|r| r.cells().map(|c| c.as_ptr()))
            .collect();
        csv.set_repeater_tone_index(2499, 49).unwrap();
        let mut expected = HEADER.to_vec();
        for _ in 0..2499 {
            expected.extend_from_slice(row);
        }
        expected
            .extend_from_slice(b"1,G,Name,Sub,CALL,GATE,f,d,o,DD,TONE,254.1Hz,u,p,lat,lon,utc\r\n");
        assert_eq!(csv.to_bytes(), expected);
        for (r, row) in csv.rows().iter().enumerate() {
            for (c, cell) in row.cells().iter().enumerate() {
                if (r, c) != (2499, 11) {
                    assert_eq!(cell.as_ptr(), pointers[r][c]);
                }
            }
        }
        // All 17 local column setters can coexist; none is a row-level import.
        csv.set_group_number(2499, 50).unwrap();
        csv.set_group_name(2499, b"Local").unwrap();
        csv.set_name(2499, b"N").unwrap();
        csv.set_sub_name(2499, b"S").unwrap();
        csv.set_repeater_call_sign(2499, b"C").unwrap();
        csv.set_gateway_call_sign(2499, b"G").unwrap();
        csv.set_frequency_hz(2499, 145000000).unwrap();
        csv.set_duplex(2499, RepeaterDuplex::Minus).unwrap();
        csv.set_offset_hz(2499, 600000).unwrap();
        csv.set_mode(2499, RepeaterMode::Dv).unwrap();
        csv.set_tone(2499, RepeaterTone::ToneSquelch).unwrap();
        csv.set_rpt1_use(2499, true).unwrap();
        csv.set_position(2499, RepeaterPosition::None).unwrap();
        csv.set_latitude(2499, csv_coordinate(0, 0, true)).unwrap();
        csv.set_longitude(2499, csv_coordinate(180, 0, false))
            .unwrap();
        csv.set_utc_offset_minutes(2499, 330).unwrap();
        let mut expected = HEADER.to_vec();
        for _ in 0..2499 {
            expected.extend_from_slice(row);
        }
        expected.extend_from_slice(b"50,Local,N,S,C,G,145.000000,DUP-,0.600000,DV,TSQL,254.1Hz,YES,None,-0.000000,180.000000,+5:30\r\n");
        assert_eq!(csv.to_bytes(), expected);
        let before: Vec<_> = csv
            .rows()
            .iter()
            .map(|r| r.cells().map(|c| c.as_ptr()))
            .collect();
        for index in [0, 2499] {
            for tone_index in [50, 51, 255] {
                assert!(
                    matches!(csv.set_repeater_tone_index(index, tone_index), Err(Error::Unsupported { line, .. }) if line == index + 2)
                );
                assert_eq!(csv.to_bytes(), expected);
                assert_eq!(
                    csv.rows()
                        .iter()
                        .map(|r| r.cells().map(|c| c.as_ptr()))
                        .collect::<Vec<_>>(),
                    before
                );
            }
        }
        for index in [2500, usize::MAX] {
            for tone_index in [0, 50, 51, 255] {
                assert_eq!(
                    csv.set_repeater_tone_index(index, tone_index),
                    Err(Error::RowIndexOutOfBounds { index })
                );
                assert_eq!(csv.to_bytes(), expected);
            }
        }
        let mut empty = Csv::parse(HEADER).unwrap();
        for tone_index in [0, 50, 51, 255] {
            assert_eq!(
                empty.set_repeater_tone_index(0, tone_index),
                Err(Error::RowIndexOutOfBounds { index: 0 })
            );
            assert_eq!(empty.to_bytes(), HEADER);
        }
        assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
        assert_eq!(source, original);
    }

    fn enum_csv_literal(change: Option<(usize, &[u8])>) -> Vec<u8> {
        // Independent raw cell literals; never built from a product setter/row.
        let cells: [&[u8]; 17] = [
            b"1",
            b"\x83\x5c",
            b"\x82\xa0",
            b"opaque-long",
            b"CALL-opaque",
            b"GATE-opaque",
            b"",
            b"\x81\x40dup",
            b"",
            b"",
            b"unknown-tone",
            b"\x82\xa2Hz",
            b"",
            b"unknown-position",
            b"opaque-lat",
            b"opaque-lon",
            b"--:--",
        ];
        let mut text = HEADER.to_vec();
        for (column, &cell) in cells.iter().enumerate() {
            if column != 0 {
                text.push(b',');
            }
            text.extend_from_slice(match change {
                Some((target, value)) if target == column => value,
                _ => cell,
            });
        }
        text.extend_from_slice(b"\r\n2,G,Keep,Sub,Call,Gate,0.1,OLD-DUP,OLD-OFFSET,OLD-MODE,OLD-TONE,OLD-RTONE,OLD-USE,OLD-POS,OLD-LAT,OLD-LON,OLD-UTC\r\n");
        text
    }

    #[test]
    fn enum_and_integer_utc_edits_match_literals_and_preserve_other_borrows() {
        let source = enum_csv_literal(None);
        macro_rules! check {
            ($column:literal,$method:ident,$cases:expr) => {
                for &(value, text) in $cases {
                    let mut csv = Csv::parse(&source).unwrap();
                    let pointers: Vec<_> = csv
                        .rows()
                        .iter()
                        .map(|r| r.cells().map(|c| c.as_ptr()))
                        .collect();
                    csv.$method(0, value).unwrap();
                    let expected = enum_csv_literal(Some(($column, text)));
                    assert_eq!(csv.to_bytes(), expected);
                    assert_eq!(csv.rows()[0].cells()[$column], text);
                    for (r, row) in csv.rows().iter().enumerate() {
                        for (c, cell) in row.cells().iter().enumerate() {
                            if (r, c) != (0, $column) {
                                assert_eq!(cell.as_ptr(), pointers[r][c]);
                            }
                        }
                    }
                    let parsed = Csv::parse(&expected).unwrap();
                    assert_eq!(parsed.to_bytes(), expected);
                    assert_eq!(parsed.rows()[0].cells()[$column], text);
                }
            };
        }
        check!(
            9,
            set_mode,
            &[
                (RepeaterMode::Fm, b"FM".as_slice()),
                (RepeaterMode::Dv, b"DV")
            ]
        );
        check!(
            7,
            set_duplex,
            &[
                (RepeaterDuplex::Off, b"OFF".as_slice()),
                (RepeaterDuplex::Minus, b"DUP-"),
                (RepeaterDuplex::Plus, b"DUP+")
            ]
        );
        check!(
            10,
            set_tone,
            &[
                (RepeaterTone::Off, b"OFF".as_slice()),
                (RepeaterTone::Tone, b"TONE"),
                (RepeaterTone::ToneSquelch, b"TSQL")
            ]
        );
        check!(
            12,
            set_rpt1_use,
            &[(false, b"NO".as_slice()), (true, b"YES")]
        );
        check!(
            13,
            set_position,
            &[
                (RepeaterPosition::None, b"None".as_slice()),
                (RepeaterPosition::Approximate, b"Approximate"),
                (RepeaterPosition::Exact, b"Exact")
            ]
        );
        check!(
            16,
            set_utc_offset_minutes,
            &[
                (-840, b"-14:00".as_slice()),
                (-330, b"-5:30"),
                (-5, b"-0:05"),
                (0, b"0:00"),
                (5, b"+0:05"),
                (330, b"+5:30"),
                (840, b"+14:00"),
                (15, b"+0:15"),
                (60, b"+1:00"),
                (-60, b"-1:00")
            ]
        );
        assert_eq!(source, enum_csv_literal(None));
        let mut csv = Csv::parse(&source).unwrap();
        csv.set_mode(0, RepeaterMode::Dv).unwrap();
        csv.set_tone(0, RepeaterTone::ToneSquelch).unwrap();
        csv.set_position(0, RepeaterPosition::None).unwrap();
        csv.set_duplex(0, RepeaterDuplex::Plus).unwrap();
        csv.set_rpt1_use(0, true).unwrap();
        csv.set_utc_offset_minutes(0, -5).unwrap();
        csv.set_name(0, b"New Name").unwrap();
        csv.set_sub_name(0, b"New Sub").unwrap();
        csv.set_frequency_hz(0, 145125000).unwrap();
        csv.set_offset_hz(0, 600000).unwrap();
        let expected=[HEADER,b"1,\x83\x5c,New Name,New Sub,CALL-opaque,GATE-opaque,145.125000,DUP+,0.600000,DV,TSQL,\x82\xa2Hz,YES,None,opaque-lat,opaque-lon,-0:05\r\n",b"2,G,Keep,Sub,Call,Gate,0.1,OLD-DUP,OLD-OFFSET,OLD-MODE,OLD-TONE,OLD-RTONE,OLD-USE,OLD-POS,OLD-LAT,OLD-LON,OLD-UTC\r\n"].concat();
        assert_eq!(csv.to_bytes(), expected);
        assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
    }

    #[test]
    fn csv_enum_utc_bounds_last_row_and_late_errors_are_atomic() {
        let row=b"1,G,Name,Sub,Call,Gate,unknown,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,opaque,--:--\r\n";
        let mut source = HEADER.to_vec();
        for _ in 0..2500 {
            source.extend_from_slice(row);
        }
        let mut csv = Csv::parse(&source).unwrap();
        let pointers: Vec<_> = csv
            .rows()
            .iter()
            .map(|r| r.cells().map(|c| c.as_ptr()))
            .collect();
        csv.set_mode(2499, RepeaterMode::Fm).unwrap();
        csv.set_duplex(2499, RepeaterDuplex::Minus).unwrap();
        csv.set_tone(2499, RepeaterTone::Tone).unwrap();
        csv.set_rpt1_use(2499, false).unwrap();
        csv.set_position(2499, RepeaterPosition::Exact).unwrap();
        csv.set_utc_offset_minutes(2499, 840).unwrap();
        let mut expected = HEADER.to_vec();
        for _ in 0..2499 {
            expected.extend_from_slice(row);
        }
        expected.extend_from_slice(b"1,G,Name,Sub,Call,Gate,unknown,DUP-,opaque,FM,TONE,opaque,NO,Exact,opaque,opaque,+14:00\r\n");
        assert_eq!(csv.to_bytes(), expected);
        for (r, row) in csv.rows().iter().enumerate() {
            for (c, cell) in row.cells().iter().enumerate() {
                if r != 2499 || ![7, 9, 10, 12, 13, 16].contains(&c) {
                    assert_eq!(cell.as_ptr(), pointers[r][c]);
                }
            }
        }
        for index in [0, 2499] {
            for mode in [RepeaterMode::FmNarrow, RepeaterMode::Dd] {
                assert!(
                    matches!(csv.set_mode(index,mode),Err(Error::Unsupported {line,..}) if line==index+2)
                );
                assert_eq!(csv.to_bytes(), expected);
            }
            assert!(
                matches!(csv.set_duplex(index,RepeaterDuplex::Rps),Err(Error::Unsupported {line,..}) if line==index+2)
            );
            assert_eq!(csv.to_bytes(), expected);
            for value in [-845, 845, -841, 841, -1, 1, i16::MIN, i16::MAX, 32766] {
                assert!(
                    matches!(csv.set_utc_offset_minutes(index,value),Err(Error::Unsupported {line,..}) if line==index+2)
                );
                assert_eq!(csv.to_bytes(), expected);
            }
        }
        for index in [2500, usize::MAX] {
            let error = Err(Error::RowIndexOutOfBounds { index });
            assert_eq!(csv.set_mode(index, RepeaterMode::Dd), error);
            assert_eq!(csv.set_duplex(index, RepeaterDuplex::Rps), error);
            assert_eq!(csv.set_tone(index, RepeaterTone::Off), error);
            assert_eq!(csv.set_rpt1_use(index, true), error);
            assert_eq!(csv.set_position(index, RepeaterPosition::None), error);
            assert_eq!(csv.set_utc_offset_minutes(index, i16::MIN), error);
            assert_eq!(csv.to_bytes(), expected);
        }
        let mut empty = Csv::parse(HEADER).unwrap();
        let error = Err(Error::RowIndexOutOfBounds { index: 0 });
        assert_eq!(empty.set_mode(0, RepeaterMode::FmNarrow), error);
        assert_eq!(empty.set_duplex(0, RepeaterDuplex::Rps), error);
        assert_eq!(empty.set_tone(0, RepeaterTone::Tone), error);
        assert_eq!(empty.set_rpt1_use(0, false), error);
        assert_eq!(empty.set_position(0, RepeaterPosition::None), error);
        assert_eq!(empty.set_utc_offset_minutes(0, i16::MAX), error);
        assert_eq!(empty.to_bytes(), HEADER);
        assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
    }

    #[test]
    fn integer_hz_edits_match_literal_csv_and_keep_other_cells_borrowed() {
        let source = [HEADER,
            b"1,\x83\x5c,\x82\xa0,opaque-long,,,,dup,offset,mode,tone,rtone,use,pos,lat,lon,utc\r\n",
            b"2,G,Keep,Sub,TEST01 B,TEST01 G,unknown,dup,,mode,tone,rtone,use,pos,lat,lon,utc\r\n",
        ].concat();
        let original = source.clone();
        // Independent decimal literals, not output from a product formatter.
        for (frequency, frequency_text, offset, offset_text) in [
            (30_000, b"0.030000".as_slice(), 0, b"0.000000".as_slice()),
            (145_125_000, b"145.125000", 100, b"0.000100"),
            (1_399_999_999, b"1399.999999", 9_999_900, b"9.999900"),
        ] {
            let mut csv = Csv::parse(&source).unwrap();
            let borrowed: Vec<_> = csv
                .rows()
                .iter()
                .map(|row| row.cells().map(|cell| cell.as_ptr()))
                .collect();
            csv.set_frequency_hz(0, frequency).unwrap();
            let frequency_only = [HEADER,
                b"1,\x83\x5c,\x82\xa0,opaque-long,,,", frequency_text,
                b",dup,offset,mode,tone,rtone,use,pos,lat,lon,utc\r\n",
                b"2,G,Keep,Sub,TEST01 B,TEST01 G,unknown,dup,,mode,tone,rtone,use,pos,lat,lon,utc\r\n",
            ].concat();
            assert_eq!(csv.to_bytes(), frequency_only);
            csv.set_offset_hz(1, offset).unwrap();
            let expected = [
                HEADER,
                b"1,\x83\x5c,\x82\xa0,opaque-long,,,",
                frequency_text,
                b",dup,offset,mode,tone,rtone,use,pos,lat,lon,utc\r\n",
                b"2,G,Keep,Sub,TEST01 B,TEST01 G,unknown,dup,",
                offset_text,
                b",mode,tone,rtone,use,pos,lat,lon,utc\r\n",
            ]
            .concat();
            assert_eq!(csv.to_bytes(), expected);
            for (r, row) in csv.rows().iter().enumerate() {
                for (c, cell) in row.cells().iter().enumerate() {
                    if (r, c) != (0, 6) && (r, c) != (1, 8) {
                        assert_eq!(cell.as_ptr(), borrowed[r][c]);
                    }
                }
            }
            let parsed = Csv::parse(&expected).unwrap();
            assert_eq!(parsed.rows()[0].cells()[6], frequency_text);
            assert_eq!(parsed.rows()[1].cells()[8], offset_text);
            assert_eq!(parsed.to_bytes(), expected);
            csv.set_name(0, b"New Name").unwrap();
            csv.set_sub_name(0, b"New Sub").unwrap();
            let mixed = [
                HEADER,
                b"1,\x83\x5c,New Name,New Sub,,,",
                frequency_text,
                b",dup,offset,mode,tone,rtone,use,pos,lat,lon,utc\r\n",
                b"2,G,Keep,Sub,TEST01 B,TEST01 G,unknown,dup,",
                offset_text,
                b",mode,tone,rtone,use,pos,lat,lon,utc\r\n",
            ]
            .concat();
            assert_eq!(csv.to_bytes(), mixed);
            assert_eq!(Csv::parse(&mixed).unwrap().to_bytes(), mixed);
            assert_eq!(source, original);
        }
    }

    #[test]
    fn integer_hz_last_row_errors_and_bounds_preserve_all_state() {
        let row = b"1,G,\x82\xa0,LONG-NAME,,,f,dup,offset,mode,tone,rtone,use,pos,lat,lon,utc\r\n";
        let mut source = HEADER.to_vec();
        for _ in 0..2500 {
            source.extend_from_slice(row);
        }
        let original = source.clone();
        let mut csv = Csv::parse(&source).unwrap();
        csv.set_frequency_hz(2499, 1_399_999_999).unwrap();
        csv.set_offset_hz(2499, 9_999_900).unwrap();
        let mut expected = HEADER.to_vec();
        for _ in 0..2499 {
            expected.extend_from_slice(row);
        }
        expected.extend_from_slice(b"1,G,\x82\xa0,LONG-NAME,,,1399.999999,dup,9.999900,mode,tone,rtone,use,pos,lat,lon,utc\r\n");
        assert_eq!(csv.to_bytes(), expected);
        assert_eq!(Csv::parse(&expected).unwrap().to_bytes(), expected);
        fn pointers(csv: &Csv<'_>) -> Vec<(*const u8, bool)> {
            csv.rows
                .iter()
                .flat_map(|row| {
                    row.cells
                        .iter()
                        .map(|cell| (cell.as_ptr(), matches!(cell, Cow::Borrowed(_))))
                })
                .collect()
        }
        let before_pointers = pointers(&csv);
        for index in [0, 2499] {
            for hz in [0, 1, 29_999, 1_400_000_000, u32::MAX] {
                assert!(matches!(csv.set_frequency_hz(index, hz),
                    Err(Error::Unsupported { line, .. }) if line == index + 2));
                assert_eq!(csv.to_bytes(), expected);
                assert_eq!(pointers(&csv), before_pointers);
                assert_eq!(source, original);
            }
            for hz in [1, 99, 101, 9_999_901, 10_000_000, u32::MAX] {
                assert!(matches!(csv.set_offset_hz(index, hz),
                    Err(Error::Unsupported { line, .. }) if line == index + 2));
                assert_eq!(csv.to_bytes(), expected);
                assert_eq!(pointers(&csv), before_pointers);
                assert_eq!(source, original);
            }
        }
        for index in [2500, usize::MAX] {
            assert_eq!(
                csv.set_frequency_hz(index, 0),
                Err(Error::RowIndexOutOfBounds { index })
            );
            assert_eq!(
                csv.set_offset_hz(index, 1),
                Err(Error::RowIndexOutOfBounds { index })
            );
            assert_eq!(csv.to_bytes(), expected);
            assert_eq!(pointers(&csv), before_pointers);
        }
        let mut empty = Csv::parse(HEADER).unwrap();
        assert_eq!(
            empty.set_frequency_hz(0, 0),
            Err(Error::RowIndexOutOfBounds { index: 0 })
        );
        assert_eq!(
            empty.set_offset_hz(0, 1),
            Err(Error::RowIndexOutOfBounds { index: 0 })
        );
        assert_eq!(empty.to_bytes(), HEADER);
        assert_eq!(source, original);
    }

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
