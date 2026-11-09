//! Complete normal ICF outer reader and layout-aware text writer.
//!
//! Normal open binds a zeroed flat 49E3A-byte image. It reads with mask NULL,
//! checks the final metadata-selected internal revision/signature, then requires
//! exact coverage of that layout's storage prefix. Only after all three gates
//! does it snapshot/reset/copy to M=4997A bytes. A body NULL after full coverage
//! can therefore succeed. CD remains an ignored comment in the original reader.
//!
//! Layout facts: A1 prep03-icf-provider (corrected excess=8/missing=7), A2
//! reader-and-generation BA02, A7 normal-file-selector-components BA06. The
//! six getter layouts were independently checked against the fixed-source
//! provider; required masks each contain exactly one continuous storage range.
//! No compiled arrays, factory image, original application or device is needed.

use std::io::Write;

use crate::clone::{IMAGE_CAPACITY, Profile, convert_image};

use super::{Error, IMAGE_SIGNATURE, RECORD_BYTES, SEED, text};

const OPEN_CAPACITY: usize = 0x49e3a;

/// Converter-local layout selector, distinct from the reader's raw MapRev.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout(u8);

impl Layout {
    /// DF4EC: unsigned 1..6 is kept; every other explicit value selects 6 locally.
    /// This never writes the corrected value back to reader/document metadata.
    pub fn from_map_revision(value: u32) -> Self {
        Self(if (1..=6).contains(&value) {
            value as u8
        } else {
            6
        })
    }

    /// Normalized local selector, not a region or firmware declaration.
    pub fn selector(self) -> u8 {
        self.0
    }

    /// Exact serialized payload length; converter objects have a different size.
    pub fn storage_len(self) -> usize {
        match self.0 {
            1 => 0x3ad00,
            2 => 0x3b100,
            _ => 0x3c9c0,
        }
    }

    /// Internal revision byte selected by the final metadata's layout.
    pub fn revision_offset(self) -> usize {
        match self.0 {
            1 => 0x3516f,
            2 | 3 => 0x3556f,
            _ => 0x355a1,
        }
    }

    /// Sixteen signature bytes; any following byte is outside this comparison.
    pub fn signature_offset(self) -> usize {
        self.storage_len() - 16
    }
}

/// Native outer-gate result. Reader success alone is not file adoption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Reader, signature and exact required coverage all passed.
    Success,
    /// Reader failed and restored its bound image, but not metadata.
    Reader(text::Status),
    /// Internal version is not 1..6, or the selected signature prefix differs.
    Signature,
    /// Exact coverage failed after signature succeeded.
    Coverage(text::Status),
}

/// Explicit normal-open result, retaining final metadata and attempted coverage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileImage {
    /// Original reader status, including its separate metadata check snapshot.
    pub report: text::Report,
    /// Raw final metadata; post-data comments can differ from the first-data check.
    pub reader: text::Reader,
    /// Outer adoption status; do not treat a populated image as accepted by itself.
    pub status: Status,
    /// Selected layout if the reader completed and the signature gate was reached.
    pub layout: Option<Layout>,
    /// M bytes on success; initial 49E3A-byte bound object on a failed outer gate.
    pub image: Vec<u8>,
}

impl FileImage {
    /// Convert an accepted normal-open image using the shared ten-step codec.
    /// Source remains immutable. `seed` is the actual G generation for this caller;
    /// routes needing it fail visibly when it is absent, never invent defaults.
    pub fn convert_to(
        &self,
        destination: Layout,
        seed: Option<&[u8]>,
    ) -> Result<Vec<u8>, crate::clone::Error> {
        if self.status != Status::Success {
            return Err(crate::clone::Error::Invalid(
                "ICF outer gates did not accept the image",
            ));
        }
        let source = self.layout.ok_or(crate::clone::Error::Invalid(
            "accepted ICF image has no layout",
        ))?;
        convert_image(
            &self.image,
            Profile::try_from(source.selector())?,
            Profile::try_from(destination.selector())?,
            seed,
        )
    }
}

/// Read physical file bytes with the original CRT translation and outer gates.
/// No source checksum verification or document normalization is implied.
pub fn read(source: &[u8]) -> Result<FileImage, Error> {
    let fragments = text::fragments(source);
    read_fragments(
        &fragments
            .iter()
            .map(|line| Some(line.as_slice()))
            .collect::<Vec<_>>(),
    )
}

/// Original normal-open path with explicit NULL `fgets` results for failure tests.
/// Byte grammar lives in `text::Reader`; no second parser or stricter substitute
/// is introduced here. All image mutations and status gates remain visible.
pub fn read_fragments(fragments: &[Option<&[u8]>]) -> Result<FileImage, Error> {
    let mut reader = text::Reader::default();
    let mut image = vec![0; OPEN_CAPACITY];
    let report = reader.read_fragments(&mut image, fragments, None)?;
    let mut result = FileImage {
        status: Status::Reader(report.status),
        report,
        reader,
        layout: None,
        image,
    };
    if result.report.status != text::Status::Success {
        return Ok(result);
    }
    let layout = Layout::from_map_revision(result.reader.map_revision);
    result.layout = Some(layout);
    if !(1..=6).contains(&result.image[layout.revision_offset()])
        || &result.image[layout.signature_offset()..layout.signature_offset() + 16]
            != IMAGE_SIGNATURE
    {
        result.status = Status::Signature;
        return Ok(result);
    }
    let mut required = vec![0; OPEN_CAPACITY.div_ceil(8)];
    required[..layout.storage_len() / 8].fill(0xff);
    // Every established storage length is a multiple of 32, hence also of 8.
    let coverage = text::coverage_status(Some(&result.report.coverage), Some(&required))?;
    if coverage != text::Status::Success {
        result.status = Status::Coverage(coverage);
        return Ok(result);
    }
    result.image.truncate(IMAGE_CAPACITY);
    result.status = Status::Success;
    Ok(result)
}

/// Write exactly one storage image in the requested layout, recomputing CD.
///
/// This is the writer codec, not document Save's normalization/round-trip/commit.
/// Payload bytes are not silently repaired. Comment uses its C-prefix and CRT
/// newline translation; EtcData is the caller's raw DWORD, not newly inferred Q.
pub fn write_storage(
    image: &[u8],
    layout: Layout,
    comment: &[u8],
    extra_data: u32,
) -> Result<Vec<u8>, Error> {
    write_storage_with_checksum(image, layout, comment, extra_data, true)
}

/// Native13394/1370D writer option. The canonical writer above retains its
/// original CD behavior; only an explicit native Save decision can omit CD.
/// This does not alter any reader coverage/revision/signature gate.
pub fn write_storage_with_checksum(
    image: &[u8],
    layout: Layout,
    comment: &[u8],
    extra_data: u32,
    checksum: bool,
) -> Result<Vec<u8>, Error> {
    if image.len() != layout.storage_len() {
        return Err(Error::Invalid {
            line: 0,
            reason: "writer requires the exact layout storage length",
        });
    }
    let mut output = b"40750001\r\n#Comment=".to_vec();
    for &byte in comment.split(|&b| b == 0).next().unwrap() {
        if byte == b'\n' {
            output.push(b'\r');
        }
        output.push(byte);
    }
    write!(
        output,
        "\r\n#MapRev={}\r\n#EtcData={extra_data:08X}\r\n",
        layout.selector()
    )
    .expect("Vec write cannot fail");
    let mut digest = md5::Context::new();
    digest.consume(SEED);
    for (index, payload) in image.as_chunks::<RECORD_BYTES>().0.iter().enumerate() {
        let address = (index * RECORD_BYTES) as u32;
        digest.consume(address.to_be_bytes());
        digest.consume([RECORD_BYTES as u8]);
        digest.consume(payload);
        write!(output, "{address:08X}20").expect("Vec write cannot fail");
        for byte in payload {
            write!(output, "{byte:02X}").expect("Vec write cannot fail");
        }
        output.extend_from_slice(b"\r\n");
    }
    if checksum {
        write!(output, "#CD={:X}\r\n", digest.finalize()).expect("Vec write cannot fail");
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage(selector: u32) -> Vec<u8> {
        let (length, revision) = match selector {
            1 => (0x3ad00, 0x3516f),
            2 => (0x3b100, 0x3556f),
            3 => (0x3c9c0, 0x3556f),
            _ => (0x3c9c0, 0x355a1),
        };
        let mut image: Vec<_> = (0..length).map(|i| (i * 73 + 19) as u8).collect();
        image[revision] = selector as u8;
        image[length - 16..].copy_from_slice(b"IcomCloneFormat3");
        image
    }

    #[test]
    fn all_six_layouts_use_final_metadata_prefix_signature_and_exact_storage_coverage() {
        for selector in 1..=6 {
            let image = storage(selector);
            let layout = Layout::from_map_revision(selector);
            let file = write_storage(&image, layout, b"RAW", 0).unwrap();
            let parsed = read(&file).unwrap();
            assert_eq!(parsed.status, Status::Success);
            assert_eq!(parsed.image.len(), 0x4997a);
            assert_eq!(&parsed.image[..image.len()], image);
            assert!(parsed.image[image.len()..].iter().all(|&b| b == 0));
            assert_eq!(parsed.reader.map_revision, selector);
            let copied = parsed.convert_to(layout, None).unwrap();
            assert_eq!(copied, parsed.image);
        }
    }

    #[test]
    fn excess_missing_mixed_and_full_prefix_before_null_keep_outer_gate_order() {
        let image = storage(6);
        let file = write_storage(&image, Layout::from_map_revision(6), b"", 0).unwrap();
        let physical = text::fragments(&file);
        let all: Vec<_> = physical.iter().map(|line| Some(line.as_slice())).collect();
        let first_data = 4;
        let mut partial = all.clone();
        partial.remove(first_data);
        assert_eq!(
            read_fragments(&partial).unwrap().status,
            Status::Coverage(text::Status::MissingCoverage)
        );
        let mut excess = all.clone();
        excess.push(Some(b"0003C9C001AB\n"));
        assert_eq!(
            read_fragments(&excess).unwrap().status,
            Status::Coverage(text::Status::ExcessCoverage)
        );
        partial.push(Some(b"0003C9C001AB\n"));
        assert_eq!(
            read_fragments(&partial).unwrap().status,
            Status::Coverage(text::Status::ExcessCoverage)
        );
        let mut null = all.clone();
        null.pop(); // CD is not a reader gate.
        null.push(None);
        assert_eq!(read_fragments(&null).unwrap().status, Status::Success);
        let mut corrupt = image;
        corrupt[0x3c9bf] = b'X';
        let invalid = write_storage(&corrupt, Layout::from_map_revision(6), b"", 0).unwrap();
        assert_eq!(read(&invalid).unwrap().status, Status::Signature);
    }

    #[test]
    fn internal_revision_need_not_equal_metadata_and_late_revision_does_not_recheck_callbacks() {
        let mut image = storage(6);
        image[0x355a1] = 2;
        let mut file = write_storage(&image, Layout::from_map_revision(6), b"", 0).unwrap();
        file.extend_from_slice(b"#MapRev=4294967295\r\n#EtcData=-80000000\r\n");
        let parsed = read(&file).unwrap();
        assert_eq!(parsed.status, Status::Success);
        assert_eq!(parsed.reader.map_revision, u32::MAX);
        assert_eq!(parsed.layout.unwrap().selector(), 6);
        assert_eq!(parsed.reader.extra_data, 0x80000000);
        assert_eq!(parsed.report.metadata_before_data, Some((6, 0)));
        assert!(
            write_storage(
                &image[..image.len() - 1],
                Layout::from_map_revision(6),
                b"",
                0
            )
            .is_err()
        );
        image[0x355a1] = 0;
        assert_eq!(
            read(&write_storage(&image, Layout::from_map_revision(6), b"", 0).unwrap())
                .unwrap()
                .status,
            Status::Signature
        );
    }
}
