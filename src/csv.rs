//! CS-705 1.30 byte CSV boundaries, not RFC 4180 or a Unicode CSV dialect.
//!
//! RVAs 1BD44/E81F0 load CR/LF/CRLF lines up to the first NUL; 46970 splits
//! without recognizing quotes, skipping one byte after project DBCS leads.
//! No BOM removal, CP932 decoding, quote escaping or host locale lookup occurs.
//! Native language/era/RTL values are explicit inputs, independent of the copied
//! CSV decimal/list bytes. The older gps::Csv/repeater::Csv remain local editors.

#[path = "csv/channel.rs"]
pub mod channel;
#[path = "csv/control.rs"]
pub mod control;
#[path = "csv/datetime.rs"]
pub mod datetime;
#[path = "csv/date_input.rs"]
pub mod date_input;
#[path = "csv/dispatch.rs"]
pub mod dispatch;
#[path = "csv/history.rs"]
pub mod history;
#[path = "csv/nls.rs"]
pub mod nls;
#[path = "csv/number.rs"]
pub mod number;
#[path = "csv/payload.rs"]
pub mod payload;
#[path = "csv/table.rs"]
pub mod table;

use std::fmt;

/// A checked safe-API boundary. No diagnostic contains configuration bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A bounded input or output exceeds the supported storage limit.
    Limit(&'static str),
    /// A required field, index or structural invariant is invalid.
    Invalid(&'static str),
    /// The original conversion fails rather than returning a silent default.
    Conversion(&'static str),
    /// Native execution would read unavailable or unwritten memory.
    SourceUnavailable(&'static str),
    /// An explicitly supplied native operation failed after prior mutations.
    Operation(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (kind, reason) = match self {
            Self::Limit(s) => ("limit", s),
            Self::Invalid(s) => ("invalid", s),
            Self::Conversion(s) => ("conversion", s),
            Self::SourceUnavailable(s) => ("source unavailable", s),
            Self::Operation(s) => ("operation", s),
        };
        write!(f, "CSV {kind}: {reason}")
    }
}
impl std::error::Error for Error {}

/// Copied CSV bytes (22674), not live RTL/CRT/ACP state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locale {
    /// Decimal byte, normalized only at the explicit copy operation.
    pub decimal: u8,
    /// List separator byte, normalized only at the explicit copy operation.
    pub list: u8,
}
impl Locale {
    /// Reproduce 22674's copy/repair, including decimal/list collision.
    pub fn copied(decimal: u8, list: u8) -> Self {
        let decimal = if matches!(decimal, b'.' | b',') {
            decimal
        } else {
            b'.'
        };
        let list = if matches!(list, b',' | b';') {
            list
        } else {
            b','
        };
        Self {
            decimal,
            list: if decimal == list { b';' } else { list },
        }
    }
}

/// Trim the RTL byte domain, not Unicode whitespace or CP932 full-width space.
pub fn trim(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(|b| *b <= 0x20) {
        value = &value[1..];
    }
    trim_right(value)
}
fn trim_right(mut value: &[u8]) -> &[u8] {
    while value.last().is_some_and(|b| *b <= 0x20) {
        value = &value[..value.len() - 1];
    }
    value
}
/// Project lead test (4729C), deliberately excluding F0..FC.
pub fn is_lead(byte: u8) -> bool {
    matches!(byte, 0x81..=0x9f | 0xe0..=0xef)
}

/// Split logical bytes, preserving quotes, empty tails and illegal DBCS trails.
pub fn split_cells(line: &[u8], separator: u8) -> Vec<&[u8]> {
    Cells::new(line, separator).collect()
}

struct Cells<'a> {
    line: &'a [u8],
    separator: u8,
    start: usize,
    at: usize,
    done: bool,
}
impl<'a> Cells<'a> {
    fn new(line: &'a [u8], separator: u8) -> Self {
        Self {
            line,
            separator,
            start: 0,
            at: 0,
            done: false,
        }
    }
}
impl<'a> Iterator for Cells<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        while self.at < self.line.len() {
            if is_lead(self.line[self.at]) {
                self.at += 2;
            } else {
                let at = self.at;
                self.at += 1;
                if self.line[at] == self.separator {
                    let cell = &self.line[self.start..at];
                    self.start = self.at;
                    return Some(cell);
                }
            }
        }
        self.done = true;
        Some(&self.line[self.start..])
    }
}

/// None is an absent header column; a present short row returns Some(empty).
pub fn column(line: &[u8], index: Option<usize>, separator: u8) -> Option<&[u8]> {
    index.map(|index| Cells::new(line, separator).nth(index).unwrap_or(b""))
}

/// Byte header matching with caller-supplied CharLowerBuffA-equivalent output.
/// Native output is not silently replaced by Unicode or host case folding.
pub fn find_column_with(
    header: &[u8],
    label: &[u8],
    separator: u8,
    mut lower: impl FnMut(&[u8]) -> Result<Vec<u8>, Error>,
) -> Result<Option<usize>, Error> {
    let label = lower(trim(label))?;
    let header = lower(header)?;
    Ok(Cells::new(&header, separator).position(|cell| trim(cell) == label))
}

/// ASCII-only case mapping is explicitly selected, not a CP932/NLS claim.
pub fn find_column_ascii(header: &[u8], label: &[u8], separator: u8) -> Option<usize> {
    Cells::new(header, separator).position(|cell| trim(cell).eq_ignore_ascii_case(trim(label)))
}

/// Bounded loaded text. Empty physical rows are skipped only after the header.
pub struct Reader<'a> {
    header: &'a [u8],
    rows: Vec<&'a [u8]>,
    locale: Locale,
}
impl<'a> Reader<'a> {
    /// Safe bounds are16 MiB and100000 nonempty data rows; the original allocator
    /// had no corresponding limits. Empty physical rows do not allocate entries.
    pub fn new(source: &'a [u8], locale: Locale) -> Result<Self, Error> {
        if source.len() > 16 * 1024 * 1024 {
            return Err(Error::Limit("input exceeds 16 MiB"));
        }
        if !matches!(locale.list, b',' | b';') || !matches!(locale.decimal, b'.' | b',') {
            return Err(Error::Invalid("uncopied CSV locale"));
        }
        let source = source.split(|b| *b == 0).next().unwrap_or(b"");
        let mut header = None;
        let mut rows = Vec::new();
        let mut push_line = |line: &'a [u8]| -> Result<(), Error> {
            if header.is_none() {
                header = Some(line);
            } else if !line.is_empty() {
                if rows.len() == 100000 {
                    return Err(Error::Limit("more than 100000 nonempty CSV rows"));
                }
                rows.push(line);
            }
            Ok(())
        };
        let (mut start, mut at) = (0, 0);
        while at < source.len() {
            if matches!(source[at], b'\r' | b'\n') {
                push_line(&source[start..at])?;
                if source[at] == b'\r' && source.get(at + 1) == Some(&b'\n') {
                    at += 1;
                }
                at += 1;
                start = at;
            } else {
                at += 1;
            }
        }
        if start < source.len() {
            push_line(&source[start..])?;
        }
        let header = header.ok_or(Error::Invalid("missing header"))?;
        Ok(Self {
            header,
            rows,
            locale,
        })
    }
    /// Exact first physical line, including a possible BOM.
    pub fn header(&self) -> &'a [u8] {
        self.header
    }
    /// Nonempty rows in reader order, not physical-line parity.
    pub fn rows(&self) -> &[&'a [u8]] {
        &self.rows
    }
    /// Copied separators used by this explicit reader invocation.
    pub fn locale(&self) -> Locale {
        self.locale
    }
}

fn trim_fixed_spaces(mut source: &[u8]) -> &[u8] {
    loop {
        if let Some(prefix) = source.strip_suffix(b" ") {
            source = prefix;
        } else if let Some(prefix) = source.strip_suffix(b"\x81\x40") {
            source = prefix;
        } else {
            break;
        }
    }
    source
}
/// B70E4/B6184 fixed import: trim ASCII/full-width trailing spaces first, then
/// DBCS-safe byte clipping only when over width. Clipping alone is counted.
pub fn fixed_text<const N: usize>(source: &[u8]) -> ([u8; N], bool) {
    let source = trim_fixed_spaces(source);
    let (source, clipped) = clip_dbcs(source, N);
    let mut output = [b' '; N];
    output[..source.len()].copy_from_slice(source);
    (output, clipped)
}

fn clip_dbcs(source: &[u8], maximum: usize) -> (&[u8], bool) {
    let clipped = source.len() > maximum;
    let mut len = source.len().min(maximum);
    if clipped {
        let mut at = 0;
        while at < len {
            if is_lead(source[at]) {
                if at + 1 == len {
                    len -= 1;
                    break;
                }
                at += 2;
            } else {
                at += 1;
            }
        }
    }
    (&source[..len], clipped)
}

/// B71E0/B62E8: precision-s C-prefix, both delimiters to spaces, TrimRight.
/// Quotes, embedded line breaks and high bytes are not escaped or repaired.
pub fn export_text(source: &[u8], maximum: usize) -> Vec<u8> {
    let len = source.len().min(maximum);
    let source = source[..len].split(|b| *b == 0).next().unwrap_or(b"");
    let mut output: Vec<_> = source
        .iter()
        .map(|b| if matches!(b, b',' | b';') { b' ' } else { *b })
        .collect();
    output.truncate(trim_right(&output).len());
    output
}
/// B8214 callsign precision-s/TrimRight, without delimiter replacement.
pub fn export_call(source: &[u8], maximum: usize) -> Vec<u8> {
    let len = source.len().min(maximum);
    trim_right(source[..len].split(|b| *b == 0).next().unwrap_or(b"")).to_vec()
}

/// Join logical cell bytes, retaining embedded NULs and delimiters (no quoting).
pub fn join(cells: &[Vec<u8>], separator: u8, crlf: bool) -> Result<Vec<u8>, Error> {
    let mut len = cells.len().saturating_sub(1) + if crlf { 2 } else { 0 };
    for cell in cells {
        len = len
            .checked_add(cell.len())
            .ok_or(Error::Limit("row size overflow"))?;
    }
    if len > 16 * 1024 * 1024 {
        return Err(Error::Limit("row exceeds 16 MiB"));
    }
    let mut output = Vec::with_capacity(len);
    for (at, cell) in cells.iter().enumerate() {
        if at != 0 {
            output.push(separator);
        }
        output.extend_from_slice(cell);
    }
    if crlf {
        output.extend_from_slice(b"\r\n");
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_grammar_and_dbcs_boundaries() {
        let locale = Locale::copied(b',', b',');
        assert_eq!(
            locale,
            Locale {
                decimal: b',',
                list: b';'
            }
        );
        let r = Reader::new(
            b"\xef\xbb\xbfHead;X\r\n\r\na;\x81;z\n \r\0ignored\n",
            locale,
        )
        .unwrap();
        assert_eq!(r.header(), b"\xef\xbb\xbfHead;X");
        assert_eq!(r.rows(), [b"a;\x81;z".as_slice(), b" "]);
        assert_eq!(
            split_cells(b"\"a,b\",c", b','),
            [b"\"a".as_slice(), b"b\"", b"c"]
        );
        assert_eq!(
            split_cells(b"\x81,,x,", b','),
            [b"\x81,".as_slice(), b"x", b""]
        );
        assert_eq!(split_cells(b"\xf0,,", b','), [b"\xf0".as_slice(), b"", b""]);
        assert_eq!(
            find_column_ascii(b" Name ,NAME, latitude", b"name", b','),
            Some(0)
        );
        assert_eq!(column(b"a", Some(10), b','), Some(b"".as_slice()));
        assert_eq!(column(b"a", None, b','), None);
        assert!(Reader::new(b"", locale).is_err());
        assert_eq!(Reader::new(b"\n", locale).unwrap().header(), b"");
        let empty_lines = b"\n".repeat(1_000_001);
        assert!(Reader::new(&empty_lines, locale).unwrap().rows().is_empty());
        let excess = [b"H\n".as_slice(), &b"x\n".repeat(100001)].concat();
        assert!(matches!(Reader::new(&excess, locale), Err(Error::Limit(_))));
        assert_eq!(
            column(b"a,b,c", Some(usize::MAX), b','),
            Some(b"".as_slice())
        );
    }
    #[test]
    fn clipping_counts_and_export_are_not_rfc_csv() {
        assert_eq!(fixed_text::<3>(b"X \x81\x40"), (*b"X  ", false));
        assert_eq!(fixed_text::<3>(b"AB\x81"), (*b"AB\x81", false));
        assert_eq!(fixed_text::<3>(b"AB\x81Z"), (*b"AB ", true));
        assert_eq!(fixed_text::<3>(b"\x81\x30XQ"), (*b"\x81\x30X", true));
        assert_eq!(export_text(b"A,B;C\"\r\nZ\0tail", 16), b"A B C\"\r\nZ");
        assert_eq!(export_call(b"X,Y;Z  ", 8), b"X,Y;Z");
        assert_eq!(
            join(&[b"a\0b".to_vec(), b"c;d".to_vec()], b';', true).unwrap(),
            b"a\0b;c;d\r\n"
        );
    }
}
