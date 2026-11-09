//! Original ICF text-reader semantics, separate from canonical serialization.
//!
//! RVA 0x137EC reads CRT `fgets(768)` fragments. Metadata is mutable reader
//! state, checked only before the first data fragment. A nonzero status restores
//! image bytes, not metadata. CD is an ordinary ignored comment in this reader;
//! use `Icf::parse` when checksum verification is explicitly required.
//!
//! This models the accepted C-locale byte grammar and flat address map. It does
//! not execute the original CRT, callbacks, files or devices. `read_fragments`
//! exposes NULL reads directly; `read` provides the normal regular-file text
//! translation, including the unusual CR-block lookahead.

use super::{Error, digit};

/// Original reader/coverage status codes, not a generic success boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Status {
    /// Reader completed, possibly after a body NULL read.
    Success = 0,
    /// Header model mismatch.
    Model = 3,
    /// Header file revision mismatch.
    FileRevision = 4,
    /// First-data metadata callback rejected MapRev.
    MapRevision = 5,
    /// NULL header, malformed header, data grammar or fragment length.
    Syntax = 6,
    /// Missing coverage, or an explicit NULL coverage pointer.
    MissingCoverage = 7,
    /// Excess coverage (also takes precedence over missing bits), or bad address.
    ExcessCoverage = 8,
    /// First-data metadata callback rejected EtcData integrity.
    ExtraData = 10,
}

/// Exact coverage comparison from RVA 0x13ED8.
///
/// Equal bitmaps succeed. Any excess bit yields 8 even if other bits are missing;
/// a strict subset yields 7. Either explicit NULL pointer yields 7. Unequal buffer
/// lengths are a caller contract error, not an emulated native memory access.
pub fn coverage_status(observed: Option<&[u8]>, required: Option<&[u8]>) -> Result<Status, Error> {
    let (Some(observed), Some(required)) = (observed, required) else {
        return Ok(Status::MissingCoverage);
    };
    if observed.len() != required.len() {
        return Err(invalid("coverage bitmap sizes differ"));
    }
    Ok(if observed == required {
        Status::Success
    } else if observed
        .iter()
        .zip(required)
        .any(|(&got, &need)| got & !need != 0)
    {
        Status::ExcessCoverage
    } else {
        Status::MissingCoverage
    })
}

/// Metadata and header binding persist between reader invocations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reader {
    /// Expected model, normally hexadecimal 4075.
    pub model: u16,
    /// Expected file revision, normally 1.
    pub file_revision: u16,
    /// Raw MapRev; constructor value FFFFFFFF, not reset on each read.
    pub map_revision: u32,
    /// Raw final EtcData, independently mutable after the metadata callback.
    pub extra_data: u32,
    /// Install the normal document MapRev/EtcData metadata checks.
    pub check_metadata: bool,
}

/// Observable reader result; coverage remains available even after image restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Original status; only zero commits image changes.
    pub status: Status,
    /// Attempted data coverage in the bound flat map, low bit first.
    pub coverage: Vec<u8>,
    /// Metadata seen by the one-time first-data check, if reached.
    pub metadata_before_data: Option<(u32, u32)>,
    /// Number of complete data records decoded before stopping.
    pub records: usize,
    /// Number of bytes written before stopping, including duplicate addresses.
    pub bytes_written: usize,
    /// Observed #Comment payload for lossless document metadata. It does not
    /// participate in native parsing/gates; the native reader ignores this key.
    pub source_comment: Vec<u8>,
}

impl Default for Reader {
    fn default() -> Self {
        Self {
            model: 0x4075,
            file_revision: 1,
            map_revision: u32::MAX,
            extra_data: 0,
            check_metadata: true,
        }
    }
}

impl Reader {
    /// Read physical bytes through the normal 512-byte CRT text translation.
    pub fn read(
        &mut self,
        image: &mut [u8],
        source: &[u8],
        required: Option<&[u8]>,
    ) -> Result<Report, Error> {
        let fragments = fragments(source);
        self.read_fragments(
            image,
            &fragments
                .iter()
                .map(|s| Some(s.as_slice()))
                .collect::<Vec<_>>(),
            required,
        )
    }

    /// Read supplied CRT fragments; `None` is a NULL `fgets` result, not empty text.
    ///
    /// Header NULL is status 6. Body NULL and exhaustion both run the optional
    /// coverage comparison; they do not imply failure. No mask means status 0.
    /// A fragment must fit the actual 767-byte return bound. On nonzero status
    /// every image byte is restored, while final metadata and coverage survive.
    pub fn read_fragments(
        &mut self,
        image: &mut [u8],
        fragments: &[Option<&[u8]>],
        required: Option<&[u8]>,
    ) -> Result<Report, Error> {
        if required.is_some_and(|mask| mask.len() != image.len().div_ceil(8)) {
            return Err(invalid("coverage bitmap does not match bound image"));
        }
        if fragments.iter().flatten().any(|s| s.len() > 767) {
            return Err(invalid("fgets fragment exceeds 767 bytes"));
        }
        let snapshot = image.to_vec();
        let mut report = Report {
            status: Status::Success,
            coverage: vec![0; image.len().div_ceil(8)],
            metadata_before_data: None,
            records: 0,
            bytes_written: 0,
            source_comment: Vec::new(),
        };
        report.status = self.read_inner(image, fragments, required, &mut report);
        if report.status != Status::Success {
            image.copy_from_slice(&snapshot);
        }
        Ok(report)
    }

    fn read_inner(
        &mut self,
        image: &mut [u8],
        fragments: &[Option<&[u8]>],
        required: Option<&[u8]>,
        report: &mut Report,
    ) -> Status {
        let Some(Some(first)) = fragments.first() else {
            return Status::Syntax;
        };
        let header = c_prefix(first);
        if header.len() != 9 || !header[..8].iter().all(u8::is_ascii_digit) {
            return Status::Syntax;
        }
        let model = header[..4]
            .iter()
            .fold(0_u16, |n, b| n * 16 + u16::from(b - b'0'));
        let revision = header[4..8]
            .iter()
            .fold(0_u16, |n, b| n * 16 + u16::from(b - b'0'));
        if model != self.model {
            return Status::Model;
        }
        if revision != self.file_revision {
            return Status::FileRevision;
        }
        for fragment in &fragments[1..] {
            let Some(fragment) = fragment else {
                break;
            };
            let line = c_prefix(fragment);
            if line.first() == Some(&b'#') {
                if let Some(comment) = line.strip_prefix(b"#Comment=") {
                    report.source_comment = comment.strip_suffix(b"\n").unwrap_or(comment).to_vec();
                }
                if contains(&line[1..], b"MapRev=") {
                    self.map_revision = decimal_prefix(line.get(8..).unwrap_or_default());
                } else if contains(&line[1..], b"EtcData=") {
                    self.extra_data = hexadecimal_prefix(line.get(9..).unwrap_or_default()) as u32;
                }
                continue;
            }
            if report.metadata_before_data.is_none() {
                report.metadata_before_data = Some((self.map_revision, self.extra_data));
                if self.check_metadata {
                    if !(1..=6).contains(&self.map_revision) {
                        return Status::MapRevision;
                    }
                    if !extra_data_valid(self.extra_data) {
                        return Status::ExtraData;
                    }
                }
            }
            // The original deletes the last C-prefix byte, not specifically LF.
            let Some(data) = line.get(..line.len().saturating_sub(1)) else {
                return Status::Syntax;
            };
            if !(12..=520).contains(&data.len()) || !data.iter().all(u8::is_ascii_hexdigit) {
                return Status::Syntax;
            }
            let number = |text: &[u8]| {
                text.iter()
                    .fold(0_u32, |n, b| n * 16 + u32::from(digit(*b).unwrap()))
            };
            let address = number(&data[..8]) as usize;
            let count = number(&data[8..10]) as usize;
            if 10 + 2 * count != data.len() {
                return Status::Syntax;
            }
            if address > image.len() || count > image.len() - address {
                return Status::ExcessCoverage;
            }
            for (i, pair) in data[10..].as_chunks::<2>().0.iter().enumerate() {
                image[address + i] = number(pair) as u8;
                report.coverage[(address + i) / 8] |= 1 << ((address + i) % 8);
            }
            report.records += 1;
            report.bytes_written += count;
        }
        required.map_or(Status::Success, |mask| {
            coverage_status(Some(&report.coverage), Some(mask)).unwrap()
        })
    }
}

/// Decode EtcData's three population-count check bits (RVA 0x565A4).
pub fn extra_data_valid(word: u32) -> bool {
    let stored = (word & 1) | ((word >> 3 & 1) << 1) | ((word >> 6 & 1) << 2);
    stored == (word & !0x49).count_ones() % 8
}

fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 0, reason }
}
fn c_prefix(bytes: &[u8]) -> &[u8] {
    bytes.split(|&b| b == 0).next().unwrap()
}
fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|s| s == needle)
}
fn space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 11 | 12)
}
fn sign_prefix(bytes: &[u8]) -> (bool, &[u8]) {
    let bytes = &bytes[bytes.iter().take_while(|&&b| space(b)).count()..];
    match bytes.first() {
        Some(b'-') => (true, &bytes[1..]),
        Some(b'+') => (false, &bytes[1..]),
        _ => (false, bytes),
    }
}

/// RTL `atoi` decimal prefix, including wrapping arithmetic and suffix tolerance.
pub fn decimal_prefix(bytes: &[u8]) -> u32 {
    let bytes = c_prefix(bytes);
    let bytes = &bytes[bytes.iter().take_while(|&&b| space(b)).count()..];
    let (negative, bytes) = sign_prefix(bytes);
    let n = bytes
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .fold(0_u32, |n, b| {
            n.wrapping_mul(10).wrapping_add(u32::from(b - b'0'))
        });
    if negative { n.wrapping_neg() } else { n }
}

/// Original base-16 scanner's signed saturation, not unsigned full-hex parsing.
pub fn hexadecimal_prefix(bytes: &[u8]) -> i32 {
    let (negative, mut bytes) = sign_prefix(c_prefix(bytes));
    if bytes.starts_with(b"0x") || bytes.starts_with(b"0X") {
        bytes = &bytes[2..];
    }
    let limit = if negative {
        0x8000_0000_u64
    } else {
        0x7fff_ffff
    };
    let mut n = 0_u64;
    for &byte in bytes {
        let Some(digit) = digit(byte) else {
            break;
        };
        n = n * 16 + u64::from(digit);
        if n > limit {
            return if negative { i32::MIN } else { i32::MAX };
        }
    }
    if negative {
        (n as u32).wrapping_neg() as i32
    } else {
        n as i32
    }
}

/// Normal physical-file CRT translation and `fgets(768)` fragmentation.
///
/// All CR bytes are removed. CTRL-Z stops a normal block, but a byte read into
/// its trailing-CR lookahead is not tested again for CTRL-Z (RVA 0x17A98F).
pub fn fragments(source: &[u8]) -> Vec<Vec<u8>> {
    let mut translated = Vec::with_capacity(source.len());
    let mut position = 0;
    while position < source.len() {
        let end = source.len().min(position + 512);
        let block = &source[position..end];
        position = end;
        let marker = block.iter().position(|&b| b == 0x1a);
        let mut block = block[..marker.unwrap_or(block.len())].to_vec();
        if block.last() == Some(&b'\r') && position < source.len() {
            *block.last_mut().unwrap() = source[position];
            position += 1;
        }
        translated.extend(block.into_iter().filter(|&b| b != b'\r'));
        if marker.is_some() {
            break;
        }
    }
    let mut output = Vec::new();
    let mut remaining = translated.as_slice();
    while !remaining.is_empty() {
        let bound = remaining.len().min(767);
        let length = remaining[..bound]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(bound, |p| p + 1);
        output.push(remaining[..length].to_vec());
        remaining = &remaining[length..];
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_exact_extra_missing_mixed_and_null_use_original_polarity() {
        for (got, need, expected) in [
            (6, 6, Status::Success),
            (7, 6, Status::ExcessCoverage),
            (2, 6, Status::MissingCoverage),
            (3, 6, Status::ExcessCoverage),
        ] {
            assert_eq!(
                coverage_status(Some(&[got]), Some(&[need])).unwrap(),
                expected
            );
        }
        assert_eq!(
            coverage_status(None, Some(&[0])).unwrap(),
            Status::MissingCoverage
        );
        assert_eq!(
            coverage_status(Some(&[0]), None).unwrap(),
            Status::MissingCoverage
        );
        assert!(coverage_status(Some(&[]), Some(&[0])).is_err());
    }

    #[test]
    fn duplicate_records_late_metadata_null_and_failure_restore_are_distinct() {
        let mut reader = Reader::default();
        let mut image = [0x55; 8];
        let source = b"40750001X";
        assert_eq!(
            reader.read(&mut image, source, None).unwrap().status,
            Status::Success
        );
        let fragments: &[Option<&[u8]>] = &[
            Some(b"40750001X"),
            Some(b"#MapRev=4294967302suffix\n"),
            Some(b"0000000102ABCD\n"),
            Some(b"0000000201EF\n"),
            Some(b"#MapRev=-1\n"),
            Some(b"#EtcData=-80000000tail\n"),
            None,
            Some(b"000000000100\n"),
        ];
        let report = reader
            .read_fragments(&mut image, fragments, Some(&[6]))
            .unwrap();
        assert_eq!(report.status, Status::Success);
        assert_eq!(report.metadata_before_data, Some((6, 0)));
        assert_eq!((report.records, report.bytes_written), (2, 3));
        assert_eq!(image, [0x55, 0xab, 0xef, 0x55, 0x55, 0x55, 0x55, 0x55]);
        assert_eq!(
            (reader.map_revision, reader.extra_data),
            (u32::MAX, 0x80000000)
        );
        let before = image;
        let bad: &[Option<&[u8]>] = &[
            Some(b"40750001\n"),
            Some(b"#MapRev=6\n"),
            Some(b"#EtcData=0\n"),
            Some(b"000000010199\n"),
            Some(b"\n"),
        ];
        assert_eq!(
            reader.read_fragments(&mut image, bad, None).unwrap().status,
            Status::Syntax
        );
        assert_eq!(image, before);
        assert_eq!((reader.map_revision, reader.extra_data), (6, 0));
        assert_eq!(
            reader
                .read_fragments(&mut image, &[None], None)
                .unwrap()
                .status,
            Status::Syntax
        );
        assert_eq!(
            reader
                .read_fragments(&mut image, &[Some(b"40750001\n"), None], None)
                .unwrap()
                .status,
            Status::Success
        );
    }

    #[test]
    fn literal_grammar_boundaries_and_crt_block_lookahead() {
        assert_eq!(decimal_prefix(b" \t-4294967297tail"), u32::MAX);
        assert_eq!(hexadecimal_prefix(b"FFFFFFFF"), i32::MAX);
        assert_eq!(hexadecimal_prefix(b"-80000000"), i32::MIN);
        assert_eq!(hexadecimal_prefix(b"0x"), 0);
        assert_eq!(
            fragments(b"A\rB\r\nC\x1aD"),
            [b"AB\n".to_vec(), b"C".to_vec()]
        );
        let mut raw = vec![b'A'; 511];
        raw.extend_from_slice(b"\r\x1aZ\n");
        let out = fragments(&raw);
        assert_eq!(&out[0][511..], b"\x1aZ\n");
        assert_eq!(
            fragments(&vec![b'A'; 1535])
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            [767, 767, 1]
        );
        for source in [
            b"40750001".as_slice(),
            b"\xef\xbb\xbf40750001\n",
            b"40750001\n#xMapRev=6\n000000000100\n",
            b"40750001\n#MapRev=6\n000000000100",
        ] {
            let mut reader = Reader::default();
            let mut image = [0xaa];
            assert_ne!(
                reader.read(&mut image, source, None).unwrap().status,
                Status::Success
            );
            assert_eq!(image, [0xaa]);
        }
    }
}
