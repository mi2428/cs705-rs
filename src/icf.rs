//! Strict parsing and serialization of canonical IC-705 MapRev 6 ICF images.
//!
//! Unknown layouts are rejected, not repaired. Uninterpreted data is preserved;
//! a successful parse is not authorization to write the image to a radio.

use std::fmt;
use std::io::Write;

mod common;

pub use common::{AutoPowerOff, BandEdgeBeep, PowerSave};

// CS-705 1.30 CS705.exe SHA-256:
// e9439e1413e1258be77e6ef2188b72276dd714642e3858f4690e5d45653aeca3.
// RVA 0x4C8E4 sets the image end, model and seed. Save RVA 0x13408 hashes
// the six decoded seed bytes, then each decoded address/length/payload record;
// helper RVA 0x13158 feeds MD5 at RVA 0x455A8. Metadata is not authenticated.
// Independently matched the official 705_JPN_260823.icf, SHA-256:
// 5f0fc615f2147c15077fecf083cd5b85593c5b4f8a3860388c70170f600cd484.
const IMAGE_BYTES: usize = 0x3c9c0;
const RECORD_BYTES: usize = 32;
const SEED: [u8; 6] = [0x44, 0x00, 0x77, 0x55, 0x40, 0x75];
// Document validation RVA 0x23690 checks the profile's internal revision and
// 16-byte signature; MapRev 6 field offsets come from RVA 0x45B79C.
const INTERNAL_MAP_REV: usize = 0x355a1;
const IMAGE_SIGNATURE: &[u8; 16] = b"IcomCloneFormat3";
// Normal memory key 0x105 -> RVA 0x689FD; RVA 0x8360..0x83FC builds
// the physical record/name pointers. RVA 0xB35C0 scans 500 empty-flag bytes;
// bit 0 is set for empty slots. Name copies at RVA 0x62F0 and 0x78AD
// preserve 16 raw bytes; RVA 0x47FD0 pads new names with spaces.
const MEMORY_CHANNEL_COUNT: usize = 500;
const MEMORY_CHANNEL_BYTES: usize = 83;
const MEMORY_EMPTY_FLAGS: usize = 0x3662d;
const MEMORY_NAME_OFFSET: usize = 0x43;
const MEMORY_NAME_BYTES: usize = 16;
// A fixed used/split-OFF slot 0 TEST_A -> TEST_B edit matched original-GUI
// output, Rust output and original-application resave, SHA-256:
// f27bb11051cf013599ce853495f23cb24fb93ccf25eabf90c546b5703c23580e.

/// An invalid or unsupported ICF input or setting. Line numbers are one-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The input violates its declared structure.
    Invalid {
        /// Location of the invalid input.
        line: usize,
        /// Diagnostic context; never contains configuration contents.
        reason: &'static str,
    },
    /// The format or setting is outside the independently established subset.
    Unsupported {
        /// Location of the unsupported input.
        line: usize,
        /// The unsupported format property.
        reason: &'static str,
    },
    /// The recorded checksum does not match the decoded records.
    ChecksumMismatch {
        /// Location of the checksum field.
        line: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { line, reason } => write!(f, "invalid ICF at line {line}: {reason}"),
            Self::Unsupported { line, reason } => {
                write!(f, "unsupported ICF at line {line}: {reason}")
            }
            Self::ChecksumMismatch { line } => write!(f, "ICF checksum mismatch at line {line}"),
        }
    }
}

impl std::error::Error for Error {}

/// A verified canonical image, borrowing its unchanged source text.
///
/// The model is 4075, file revision 0001, MapRev 6. The format's MD5 detects
/// accidental record changes; it provides no authenticity or metadata integrity.
///
/// Setting edits change stored configuration only, never radio operation. All
/// setters preserve other memory bytes, raw metadata and [`Self::original_bytes`];
/// use [`Self::to_bytes`] to serialize edits. The five additional common-setting
/// mappings are statically traced from CS-705 1.30. Five independent edits of a
/// fixed reference image matched original-GUI output and resave byte-for-byte;
/// other values/images are supported by static evidence and synthetic checks,
/// not those original-application comparisons.
pub struct Icf<'a> {
    source: &'a [u8],
    memory: Vec<u8>,
    comment: &'a [u8],
    extra_data: u32,
}

impl<'a> Icf<'a> {
    /// Parse a complete image and verify its profile, address coverage and checksum.
    ///
    /// Only contiguous 32-byte records covering the established image are
    /// accepted, with the exact image signature and internal revision 6. EtcData
    /// integrity bits are checked independently of CD. Other revisions, sparse
    /// images and additional metadata are deliberately unsupported. These checks
    /// identify the format, not the validity of every setting. No I/O occurs.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("settings.icf")?;
    /// let icf = cs705::icf::Icf::parse(&bytes)?;
    /// assert_eq!(icf.original_bytes(), bytes);
    /// println!("{} memory bytes", icf.memory().len());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, incomplete, inconsistent, unsupported or
    /// checksum-invalid input. Unknown values are never replaced by defaults.
    pub fn parse(source: &'a [u8]) -> Result<Self, Error> {
        let body = source.strip_suffix(b"\n").unwrap_or(source);
        let mut lines = body
            .split(|byte| *byte == b'\n')
            .enumerate()
            .map(|(i, line)| (i + 1, line.strip_suffix(b"\r").unwrap_or(line)));
        if lines.next().map(|(_, line)| line) != Some(b"40750001".as_slice()) {
            return Err(Error::Unsupported {
                line: 1,
                reason: "model or file revision",
            });
        }

        let mut metadata = [b"".as_slice(); 3];
        for (index, prefix) in [b"#Comment=".as_slice(), b"#MapRev=", b"#EtcData="]
            .into_iter()
            .enumerate()
        {
            let number = index + 2;
            let (_, text) = lines.next().ok_or(Error::Invalid {
                line: number,
                reason: "missing metadata",
            })?;
            metadata[index] = text.strip_prefix(prefix).ok_or(Error::Invalid {
                line: number,
                reason: "unexpected metadata field or order",
            })?;
        }
        if metadata[1] != b"6" {
            return Err(Error::Unsupported {
                line: 3,
                reason: "MapRev other than 6",
            });
        }
        if metadata[0].iter().any(|byte| matches!(byte, 0 | b'\r')) {
            return Err(Error::Unsupported {
                line: 2,
                reason: "NUL or embedded carriage return in comment",
            });
        }
        let extra_data = u32::from_be_bytes(hex::<4>(metadata[2]).ok_or(Error::Invalid {
            line: 4,
            reason: "EtcData must contain eight hexadecimal characters",
        })?);
        // EtcData callback RVA 0x25AA4 -> 0x56680: bits 0, 3 and 6 encode
        // the population count of the other 29 bits modulo eight, low bit first.
        let check = (extra_data & 1) | ((extra_data >> 3 & 1) << 1) | ((extra_data >> 6 & 1) << 2);
        if check != (extra_data & !0x49).count_ones() % 8 {
            return Err(Error::Invalid {
                line: 4,
                reason: "EtcData integrity bits",
            });
        }

        let mut memory = Vec::with_capacity(IMAGE_BYTES);
        let mut digest = md5::Context::new();
        digest.consume(SEED);
        while let Some((number, text)) = lines.next() {
            if let Some(checksum) = text.strip_prefix(b"#CD=") {
                let expected = hex::<16>(checksum).ok_or(Error::Invalid {
                    line: number,
                    reason: "CD must contain 32 hexadecimal characters",
                })?;
                if memory.len() != IMAGE_BYTES {
                    return Err(Error::Invalid {
                        line: number,
                        reason: "incomplete image",
                    });
                }
                if digest.finalize().0 != expected {
                    return Err(Error::ChecksumMismatch { line: number });
                }
                if lines.next().is_some() {
                    return Err(Error::Invalid {
                        line: number + 1,
                        reason: "data after checksum",
                    });
                }
                if &memory[IMAGE_BYTES - IMAGE_SIGNATURE.len()..] != IMAGE_SIGNATURE {
                    return Err(Error::Invalid {
                        line: 5 + (IMAGE_BYTES - IMAGE_SIGNATURE.len()) / RECORD_BYTES,
                        reason: "image signature",
                    });
                }
                // The original reader allows internal revisions 1..6, not
                // necessarily equal to metadata MapRev. Only internal 6 has an
                // established editing layout here; do not convert or repair it.
                if memory[INTERNAL_MAP_REV] != 6 {
                    return Err(Error::Unsupported {
                        line: 5 + INTERNAL_MAP_REV / RECORD_BYTES,
                        reason: "internal MapRev other than 6",
                    });
                }
                return Ok(Self {
                    source,
                    memory,
                    comment: metadata[0],
                    extra_data,
                });
            }
            let header = text.get(..10).and_then(hex::<5>).ok_or(Error::Invalid {
                line: number,
                reason: "invalid record header",
            })?;
            let count = usize::from(header[4]);
            if text.len() != 10 + count * 2 {
                return Err(Error::Invalid {
                    line: number,
                    reason: "record length mismatch",
                });
            }
            if count != RECORD_BYTES {
                return Err(Error::Unsupported {
                    line: number,
                    reason: "record size other than 32",
                });
            }
            let record = hex::<37>(text).ok_or(Error::Invalid {
                line: number,
                reason: "invalid hexadecimal payload",
            })?;
            let address = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
            if memory.len() >= IMAGE_BYTES || address != memory.len() as u32 {
                return Err(Error::Invalid {
                    line: number,
                    reason: "address gap, overlap or excess data",
                });
            }
            digest.consume(record);
            memory.extend_from_slice(&record[5..]);
        }
        Err(Error::Invalid {
            line: source.split(|byte| *byte == b'\n').count(),
            reason: "missing checksum",
        })
    }

    /// Return every current memory byte, including uninterpreted regions, in order.
    pub fn memory(&self) -> &[u8] {
        &self.memory
    }

    /// Return the exact original file, without normalization or subsequent edits.
    pub fn original_bytes(&self) -> &'a [u8] {
        self.source
    }

    /// Serialize the verified image using uppercase hexadecimal and CRLF lines.
    ///
    /// All memory, comment and EtcData bytes are preserved. Unlike
    /// [`Self::original_bytes`], this normalizes the text and recomputes CD.
    /// It performs no I/O and does not establish radio compatibility or safety.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut output = b"40750001\r\n#Comment=".to_vec();
        output.extend_from_slice(self.comment);
        // The original Save RVA 0x13408 uses these fields in this order;
        // Windows text-mode output converts its LF format strings to CRLF.
        write!(
            output,
            "\r\n#MapRev=6\r\n#EtcData={:08X}\r\n",
            self.extra_data
        )
        .expect("writing to Vec cannot fail");
        let mut digest = md5::Context::new();
        digest.consume(SEED);
        for (index, payload) in self.memory.as_chunks::<RECORD_BYTES>().0.iter().enumerate() {
            let address = (index * RECORD_BYTES) as u32;
            digest.consume(address.to_be_bytes());
            digest.consume([RECORD_BYTES as u8]);
            digest.consume(payload);
            write!(output, "{address:08X}20").expect("writing to Vec cannot fail");
            for byte in payload {
                write!(output, "{byte:02X}").expect("writing to Vec cannot fail");
            }
            output.extend_from_slice(b"\r\n");
        }
        write!(output, "#CD={:X}\r\n", digest.finalize()).expect("writing to Vec cannot fail");
        output
    }

    /// Return the raw comment bytes without assuming UTF-8 or changing CP932 data.
    pub fn comment_bytes(&self) -> &'a [u8] {
        self.comment
    }

    /// Return EtcData unchanged; integrity bits are checked, other bits uninterpreted.
    pub fn extra_data(&self) -> u32 {
        self.extra_data
    }

    /// Return the exact 16-byte name of an occupied normal physical memory slot.
    ///
    /// Slots are zero-based, `0..500`; an empty slot returns `None`, regardless
    /// of its residual name bytes. Names are not decoded, trimmed or replaced,
    /// even if they contain non-ASCII bytes or NULs.
    ///
    /// This raw physical view uses only the stored empty flag. It does not map
    /// GUI groups/rows or validate group links or other channel fields. Parsing
    /// verifies the format profile/structure/checksum, not every field's meaning.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for a slot outside `0..500`. That parameter
    /// error is reported at the start of the memory records (line 5).
    pub fn physical_memory_name_raw(&self, slot: usize) -> Result<Option<&[u8]>, Error> {
        if slot >= MEMORY_CHANNEL_COUNT {
            return Err(Error::Unsupported {
                line: 5,
                reason: "normal physical memory slot outside 0..500",
            });
        }
        if self.memory[MEMORY_EMPTY_FLAGS + slot] & 1 != 0 {
            return Ok(None);
        }
        let offset = slot * MEMORY_CHANNEL_BYTES + MEMORY_NAME_OFFSET;
        Ok(Some(&self.memory[offset..offset + MEMORY_NAME_BYTES]))
    }

    /// Replace only an occupied physical slot's name with narrow ASCII text.
    ///
    /// Accepts only `[A-Z0-9_-]{1,16}` and pads shorter names with `0x20`.
    /// An existing raw name is replaced only by this explicit call. All other
    /// memory bytes, flags, links, metadata and the original input are preserved;
    /// this does not create, delete, reorder or assign slots to groups. See
    /// [`Self::physical_memory_name_raw`] for the physical-view limitations.
    ///
    /// All validation precedes the single 16-byte update. Any error leaves the
    /// entire image unchanged. Use [`Self::to_bytes`] to serialize the edit and
    /// recompute CD; [`Self::original_bytes`] still returns the untouched input.
    /// A fixed used/split-OFF slot 0 edit matched original-GUI output and resave
    /// byte-for-byte. Other slots, names and images were not dynamically compared.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn rename(bytes: &[u8]) -> Result<Vec<u8>, cs705::icf::Error> {
    /// let mut image = cs705::icf::Icf::parse(bytes)?;
    /// image.set_physical_memory_name_ascii(0, "LOCAL_0")?;
    /// assert_eq!(image.original_bytes(), bytes);
    /// # Ok(image.to_bytes())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for an out-of-range slot, an empty slot or
    /// a name outside this deliberately limited ASCII profile. No truncation or
    /// character conversion is performed; this is not CS-705's full name codec.
    pub fn set_physical_memory_name_ascii(&mut self, slot: usize, name: &str) -> Result<(), Error> {
        if self.physical_memory_name_raw(slot)?.is_none() {
            return Err(Error::Unsupported {
                line: 5 + (MEMORY_EMPTY_FLAGS + slot) / RECORD_BYTES,
                reason: "cannot rename an empty physical memory slot",
            });
        }
        let offset = slot * MEMORY_CHANNEL_BYTES + MEMORY_NAME_OFFSET;
        if name.is_empty()
            || name.len() > MEMORY_NAME_BYTES
            || !name.bytes().all(|byte| {
                byte.is_ascii_uppercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
            })
        {
            return Err(Error::Unsupported {
                line: 5 + offset / RECORD_BYTES,
                reason: "memory name requires 1..=16 bytes in A-Z, 0-9, _ or -",
            });
        }
        let mut bytes = [b' '; MEMORY_NAME_BYTES];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        self.memory[offset..offset + MEMORY_NAME_BYTES].copy_from_slice(&bytes);
        Ok(())
    }
}

fn hex<const N: usize>(text: &[u8]) -> Option<[u8; N]> {
    if text.len() != N * 2 {
        return None;
    }
    let mut output = [0; N];
    for (byte, pair) in output.iter_mut().zip(text.as_chunks::<2>().0) {
        *byte = digit(pair[0])? * 16 + digit(pair[1])?;
    }
    Some(output)
}

fn digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_memory() -> Vec<u8> {
        let mut memory: Vec<u8> = (0..IMAGE_BYTES).map(|byte| byte as u8).collect();
        memory[INTERNAL_MAP_REV] = 6;
        memory[IMAGE_BYTES - 16..].copy_from_slice(b"IcomCloneFormat3");
        memory
    }

    fn fixture(key_touch_beep: Option<bool>) -> Vec<u8> {
        // Independent Python hashlib results for the incrementing synthetic image
        // with memory[0x355a1] = 6 and memory[-16:] = b"IcomCloneFormat3":
        // md5(bytes.fromhex("440077554075") + b"".join(
        // struct.pack(">IB", a, 32) + memory[a:a+32]
        // for a in range(0, 0x3c9c0, 32))).
        // The two flag-specific digests use the same Python recipe with only
        // memory[0x392f4] changed to 0 or 1 before constructing the records.
        let checksum = match key_touch_beep {
            None => "E5CCFE4749DCDFCA01BF2754C5535BC2",
            Some(false) => "871EFBC3C283177C2B1BDEC2903B5186",
            Some(true) => "06D9BCEE682DAEA10C26F424785AD4B1",
        };
        fixture_with_byte(
            0x392f4,
            key_touch_beep.map(u8::from).unwrap_or(0xf4),
            checksum,
        )
    }

    fn fixture_with_byte(offset: usize, value: u8, checksum: &str) -> Vec<u8> {
        let mut text = b"40750001\r\n#Comment=".to_vec();
        text.extend_from_slice(&[0x82, 0xa0]);
        // 12345639 preserves the old fixture's non-check bits and encodes
        // popcount(0x12345678 & ~0x49) % 8 = 3 in bits 0/3/6.
        text.extend_from_slice(b"\r\n#MapRev=6\r\n#EtcData=12345639\r\n");
        let mut memory = synthetic_memory();
        memory[offset] = value;
        for address in (0..IMAGE_BYTES).step_by(RECORD_BYTES) {
            write!(text, "{address:08X}20").unwrap();
            for value in &memory[address..address + RECORD_BYTES] {
                write!(text, "{value:02X}").unwrap();
            }
            text.extend_from_slice(b"\r\n");
        }
        write!(text, "#CD={checksum}\r\n").unwrap();
        text
    }

    fn memory_name_fixture() -> Vec<u8> {
        let mut source = fixture(None);
        let first = source.windows(10).position(|s| s == b"0000000020").unwrap();
        // Keep the incremental image, making slot 0 occupied with every upper
        // flag bit set and slot 2 empty with the same upper bits set.
        for (address, value) in [(0x3662d, b"FE"), (0x3662f, b"FF")] {
            let at = first + (address / 32) * 76 + 10 + (address % 32) * 2;
            source[at..at + 2].copy_from_slice(value);
        }
        // Independent Python hashlib over the raw synthetic records, not output
        // generated by this crate. The edit hashes below use fixed raw windows
        // at 0x43, 0x96 and 0xa20c, padded to 16 bytes with b" ".
        let cd = source.len() - 34;
        source[cd..cd + 32].copy_from_slice(b"D244B25769D85295AD06D4A61CA780D4");
        source
    }

    #[test]
    fn reads_independent_fixture_and_rejects_corruption() {
        let source = fixture(None);
        let parsed = Icf::parse(&source).unwrap();
        let expected = synthetic_memory();
        assert_eq!(parsed.memory(), expected);
        assert_eq!(parsed.original_bytes(), source);
        assert_eq!(parsed.comment_bytes(), [0x82, 0xa0]);
        assert_eq!(parsed.extra_data(), 0x12345639);
        assert_eq!(parsed.to_bytes(), source);

        let lf_source: Vec<u8> = source.iter().copied().filter(|b| *b != b'\r').collect();
        let parsed_lf = Icf::parse(&lf_source).unwrap();
        assert_eq!(parsed_lf.original_bytes(), lf_source);
        assert_eq!(parsed_lf.to_bytes(), source);

        for byte in [0, b'\r'] {
            let mut ambiguous = source.clone();
            ambiguous[b"40750001\r\n#Comment=".len()] = byte;
            assert!(matches!(
                Icf::parse(&ambiguous),
                Err(Error::Unsupported { line: 2, .. })
            ));
        }

        let first = source.windows(10).position(|s| s == b"0000000020").unwrap();
        let mut changed = source.clone();
        changed[first + 10] = b'1';
        assert!(matches!(
            Icf::parse(&changed),
            Err(Error::ChecksumMismatch { .. })
        ));

        changed = source.clone();
        changed[first + 10] = b'G';
        assert!(matches!(Icf::parse(&changed), Err(Error::Invalid { .. })));

        changed = source.clone();
        changed[first + 7] = b'1';
        assert!(matches!(Icf::parse(&changed), Err(Error::Invalid { .. })));

        changed = source.clone();
        changed.extend_from_slice(b"#CD=E5CCFE4749DCDFCA01BF2754C5535BC2\r\n");
        assert!(matches!(Icf::parse(&changed), Err(Error::Invalid { .. })));

        for end in [0, 7, 20, first, first + 74, source.len() - 3] {
            assert!(Icf::parse(&source[..end]).is_err());
        }
        changed = source.clone();
        changed[0] = b'5';
        assert!(matches!(
            Icf::parse(&changed),
            Err(Error::Unsupported { line: 1, .. })
        ));
        changed = source.clone();
        let revision = changed.windows(9).position(|s| s == b"#MapRev=6").unwrap();
        changed[revision + 8] = b'5';
        assert!(matches!(
            Icf::parse(&changed),
            Err(Error::Unsupported { line: 3, .. })
        ));
    }

    #[test]
    fn rejects_checksum_valid_foreign_profiles_and_corrupt_metadata() {
        // Independently recomputed Python MD5 values, not serializer output.
        for (revision, checksum) in [
            (0, "40B4D0DD6E0AA69C2882DCB2F22A79BC"),
            (5, "F54281571930AEFEFFAE206EBB3E78C2"),
            (7, "B6AC976EB8D65C5A829AF13608A36FCD"),
        ] {
            let source = fixture_with_byte(0x355a1, revision, checksum);
            assert_eq!(
                Icf::parse(&source).err(),
                Some(Error::Unsupported {
                    line: 6834,
                    reason: "internal MapRev other than 6",
                })
            );
        }
        let source = fixture_with_byte(0x3c9bf, b'4', "746EF8D45F70EC17608877D2F7511B67");
        assert_eq!(
            Icf::parse(&source).err(),
            Some(Error::Invalid {
                line: 7762,
                reason: "image signature",
            })
        );

        let source = fixture(None);
        let start = source.windows(9).position(|s| s == b"#EtcData=").unwrap() + 9;
        for bit in 0..32 {
            let mut changed = source.clone();
            let extra = format!("{:08X}", 0x12345639_u32 ^ (1 << bit));
            changed[start..start + 8].copy_from_slice(extra.as_bytes());
            assert_eq!(
                Icf::parse(&changed).err(),
                Some(Error::Invalid {
                    line: 4,
                    reason: "EtcData integrity bits",
                })
            );
        }
        let mut changed = source.clone();
        changed[start..start + 8].copy_from_slice(b"00000000");
        let image = Icf::parse(&changed).unwrap();
        assert_eq!(image.extra_data(), 0);
        assert_eq!(image.to_bytes(), changed);
    }

    #[test]
    fn edits_only_the_supported_confirmation_beep_byte() {
        let source = fixture(Some(false));
        let mut image = Icf::parse(&source).unwrap();
        assert!(!image.key_touch_beep().unwrap());
        image.set_key_touch_beep(true).unwrap();
        assert!(image.key_touch_beep().unwrap());
        assert_eq!(image.to_bytes(), fixture(Some(true)));
        assert_eq!(image.original_bytes(), source);
        image.set_key_touch_beep(false).unwrap();
        assert_eq!(image.to_bytes(), source);

        let unknown = fixture(None);
        let mut image = Icf::parse(&unknown).unwrap();
        assert!(matches!(
            image.key_touch_beep(),
            Err(Error::Unsupported { .. })
        ));
        assert!(image.set_key_touch_beep(false).is_err());
        assert_eq!(image.to_bytes(), unknown);
    }

    fn assert_setting_state(image: &Icf<'_>, source: &[u8], memory: &[u8]) {
        assert_eq!(image.memory(), memory);
        assert_eq!(image.memory().len(), 0x3c9c0);
        assert_eq!(image.original_bytes(), source);
        assert_eq!(image.original_bytes().as_ptr(), source.as_ptr());
        assert_eq!(image.comment_bytes(), [0x82, 0xa0]);
        assert_eq!(image.extra_data(), 0x12345639);
    }

    fn check_common_setting<T: Copy + fmt::Debug + PartialEq>(
        offset: usize,
        values: &[(u8, T, &str)],
        unknown_checksum: &str,
        unsupported: Error,
        read: impl Fn(&Icf<'_>) -> Result<T, Error>,
        write: impl Fn(&mut Icf<'_>, T) -> Result<(), Error>,
    ) {
        let fixtures: Vec<_> = values
            .iter()
            .map(|&(raw, _, checksum)| fixture_with_byte(offset, raw, checksum))
            .collect();
        let mut expected = synthetic_memory();
        for (old_index, &(old_raw, old_value, _)) in values.iter().enumerate() {
            let source = &fixtures[old_index];
            let mut image = Icf::parse(source).unwrap();
            assert_eq!(read(&image), Ok(old_value));
            for (new_index, &(raw, value, _)) in values.iter().enumerate() {
                write(&mut image, value).unwrap();
                expected[offset] = raw;
                assert_eq!(read(&image), Ok(value));
                assert_setting_state(&image, source, &expected);
                let serialized = image.to_bytes();
                assert_eq!(serialized, fixtures[new_index]);
                assert_eq!(serialized.len(), 589_699); // Independent Python text length.
                let reparsed = Icf::parse(&serialized).unwrap();
                assert_eq!(read(&reparsed), Ok(value));
                assert_setting_state(&reparsed, &serialized, &expected);
                write(&mut image, old_value).unwrap();
                expected[offset] = old_raw;
                assert_setting_state(&image, source, &expected);
                assert_eq!(image.to_bytes(), *source);
            }
        }

        // Reuse a verified image to sweep every unknown byte without file I/O
        // or repeated parsing. Other unsupported fields are never normalized.
        let source = &fixtures[0];
        let mut image = Icf::parse(source).unwrap();
        for raw in 0..=u8::MAX {
            if values.iter().any(|&(known, _, _)| raw == known) {
                continue;
            }
            image.memory[offset] = raw;
            expected[offset] = raw;
            assert_eq!(read(&image), Err(unsupported.clone()));
            for &(_, value, _) in values {
                assert_eq!(write(&mut image, value), Err(unsupported.clone()));
                assert_setting_state(&image, source, &expected);
            }
        }

        // Also exercise an unknown current value through the actual parse boundary.
        let source = fixture_with_byte(offset, 255, unknown_checksum);
        let mut image = Icf::parse(&source).unwrap();
        expected[offset] = 255;
        assert_eq!(read(&image), Err(unsupported.clone()));
        for &(_, value, _) in values {
            assert_eq!(write(&mut image, value), Err(unsupported.clone()));
            assert_setting_state(&image, &source, &expected);
        }
        assert_eq!(image.to_bytes(), source);
    }

    #[test]
    fn edits_common_settings_and_rejects_every_unknown_current_value() {
        // Independent Python hashlib expectations for each (offset, raw) below:
        // image = bytearray(i % 256 for i in range(0x3c9c0))
        // image[0x355a1] = 6; image[-16:] = b"IcomCloneFormat3"; image[offset] = raw
        // h = hashlib.md5(bytes.fromhex("440077554075"))
        // for a in range(0, len(image), 32):
        //     h.update(struct.pack(">IB", a, 32) + image[a:a+32])
        // h.hexdigest().upper()
        // No product output was used to generate fixtures or checksums.
        check_common_setting(
            0x392f3,
            &[
                (0, false, "B34A3641CC4ABBCB0B9065F5F39DE183"),
                (1, true, "9FA2FC0ADCDA5C59F3548183CE865030"),
            ],
            "A7D39FCCDFB74B874D4FA6780B294292",
            Error::Unsupported {
                line: 7324,
                reason: "beep-level limit value other than OFF or ON",
            },
            |image| image.beep_level_limit(),
            |image, value| image.set_beep_level_limit(value),
        );
        check_common_setting(
            0x392f5,
            &[
                (0, false, "89FAF1DDC816AB111E486D1B25C66C65"),
                (1, true, "168D2D4E5D5D4A82F284C4FCAB9ECE27"),
            ],
            "CF82DEF9D62FB7462650E4B5AB0BBA79",
            Error::Unsupported {
                line: 7324,
                reason: "home-channel beep value other than OFF or ON",
            },
            |image| image.home_channel_beep(),
            |image, value| image.set_home_channel_beep(value),
        );
        check_common_setting(
            0x392ff,
            &[
                (0, BandEdgeBeep::Off, "F08A5DF41CCEE16673E0352A4D5AA67D"),
                (
                    1,
                    BandEdgeBeep::DefaultEdges,
                    "DBC9C4B17E4135CD81B3D920AC6D2562",
                ),
                (
                    2,
                    BandEdgeBeep::UserEdges,
                    "D74134227FAC1A22E23FB71891CB8EB5",
                ),
                (
                    3,
                    BandEdgeBeep::UserEdgesWithTransmitLimit,
                    "92CFDFF025881276604AE6A83FA74DAB",
                ),
            ],
            "E5CCFE4749DCDFCA01BF2754C5535BC2",
            Error::Unsupported {
                line: 7324,
                reason: "band-edge beep selection outside 0 through 3",
            },
            |image| image.band_edge_beep(),
            |image, value| image.set_band_edge_beep(value),
        );
        check_common_setting(
            0x39300,
            &[
                (0, AutoPowerOff::Off, "E5CCFE4749DCDFCA01BF2754C5535BC2"),
                (
                    1,
                    AutoPowerOff::Minutes30,
                    "EE9C781F382BBAB3F48DB3022170C704",
                ),
                (
                    2,
                    AutoPowerOff::Minutes60,
                    "E8BFE6EB27DBDAC00077415E8BDDB2A7",
                ),
                (
                    3,
                    AutoPowerOff::Minutes90,
                    "B491D17A3D3B54E3E71738605E363E13",
                ),
                (
                    4,
                    AutoPowerOff::Minutes120,
                    "9D6FFCD0D2E95A26493BAEC3E1DD5D82",
                ),
            ],
            "2635A6B6CD5E53A3886D920395BAF954",
            Error::Unsupported {
                line: 7325,
                reason: "auto power-off selection outside 0 through 4",
            },
            |image| image.auto_power_off(),
            |image, value| image.set_auto_power_off(value),
        );
        check_common_setting(
            0x39301,
            &[
                (0, PowerSave::Off, "9B4D838B99B5B9855FF58D7E9C22BDBC"),
                (1, PowerSave::AutoShort, "E5CCFE4749DCDFCA01BF2754C5535BC2"),
                (2, PowerSave::AutoMedium, "9CAC18C74A2958DBFBAB30A20F8B527A"),
                (3, PowerSave::AutoLong, "8A4D22251978BA2E3222E5725512041F"),
            ],
            "CA308E2BEEB3478BDF8EF26F6B3AF0F8",
            Error::Unsupported {
                line: 7325,
                reason: "power-save selection outside 0 through 3",
            },
            |image| image.power_save(),
            |image, value| image.set_power_save(value),
        );
    }

    #[test]
    fn reads_raw_physical_names_and_preserves_every_other_byte_on_edit() {
        let source = memory_name_fixture();
        assert_eq!(
            format!("{:X}", md5::compute(&source)),
            "C6705836C25816893AA82532F0416922"
        );
        let mut image = Icf::parse(&source).unwrap();
        let mut expected = synthetic_memory();
        expected[0x3662d] = 0xfe;
        expected[0x3662f] = 0xff;
        assert_eq!(image.memory(), expected);
        // Slot 215 crosses FF -> 00, covering non-ASCII bytes and embedded NULs.
        for (slot, offset) in [(0, 0x43), (1, 0x96), (215, 0x45f8), (499, 0xa20c)] {
            assert_eq!(
                image.physical_memory_name_raw(slot).unwrap(),
                Some(&expected[offset..offset + 16])
            );
        }
        assert_eq!(image.physical_memory_name_raw(2).unwrap(), None);
        assert_eq!(image.memory()[0x3662d], 0xfe);
        assert_eq!(image.memory()[0x3662f], 0xff);

        for (slot, offset, name, padded, file_md5) in [
            (
                0,
                0x43,
                "A",
                b"A               ",
                "4D9C939D1C4FB6589B6CF4C7D06CCD3C",
            ),
            (
                1,
                0x96,
                "AZ09_-AZ09_-AZ09",
                b"AZ09_-AZ09_-AZ09",
                "D893E7F2C9BBAB70794F3BCDA415A0D5",
            ),
            (
                499,
                0xa20c,
                "LAST_499",
                b"LAST_499        ",
                "0512D7AC20CB3884221AA1B99CA425EC",
            ),
        ] {
            image.set_physical_memory_name_ascii(slot, name).unwrap();
            expected[offset..offset + 16].copy_from_slice(padded);
            assert_eq!(image.memory(), expected);
            assert_eq!(
                image.physical_memory_name_raw(slot).unwrap(),
                Some(padded.as_slice())
            );
            assert_eq!(image.original_bytes(), source);
            assert_eq!(image.comment_bytes(), [0x82, 0xa0]);
            assert_eq!(image.extra_data(), 0x12345639);
            let output = image.to_bytes();
            assert_eq!(output.len(), source.len());
            assert_eq!(format!("{:X}", md5::compute(&output)), file_md5);
            let reparsed = Icf::parse(&output).unwrap();
            assert_eq!(reparsed.memory(), expected);
            assert_eq!(reparsed.comment_bytes(), [0x82, 0xa0]);
            assert_eq!(reparsed.extra_data(), 0x12345639);
        }

        let before = image.to_bytes();
        image.set_physical_memory_name_ascii(0, "SECOND").unwrap();
        image.set_physical_memory_name_ascii(0, "A").unwrap();
        image
            .set_physical_memory_name_ascii(499, "LAST_499")
            .unwrap();
        assert_eq!(image.memory(), expected);
        assert_eq!(image.to_bytes(), before);
        assert_eq!(image.original_bytes(), source);
    }

    #[test]
    fn rejects_unsupported_physical_name_edits_without_changing_any_state() {
        let source = memory_name_fixture();
        let mut image = Icf::parse(&source).unwrap();
        image.set_physical_memory_name_ascii(0, "RENAMED").unwrap();
        let before = image.memory().to_vec();
        let serialized = image.to_bytes();
        for (slot, name) in [
            (2, "VALID"),
            (500, "VALID"),
            (usize::MAX, "VALID"),
            (0, ""),
            (0, "ABCDEFGHIJKLMNOPQ"),
            (1, "a"),
            (1, "@"),
            (1, "["),
            (1, "/"),
            (1, ":"),
            (1, "A B"),
            (1, "A\0B"),
            (1, "A\nB"),
            (499, "\u{ff21}"),
            (499, "AAAAAAAAAAAAAA\u{e9}"),
            (499, "AAAAAAAAAAAAAAA\u{e9}"),
            (usize::MAX, ""),
        ] {
            assert!(matches!(
                image.set_physical_memory_name_ascii(slot, name),
                Err(Error::Unsupported { .. })
            ));
            assert_eq!(image.memory(), before);
            assert_eq!(image.original_bytes(), source);
            assert_eq!(image.comment_bytes(), [0x82, 0xa0]);
            assert_eq!(image.extra_data(), 0x12345639);
        }
        for slot in [500, usize::MAX] {
            assert!(matches!(
                image.physical_memory_name_raw(slot),
                Err(Error::Unsupported { line: 5, .. })
            ));
        }
        assert_eq!(image.to_bytes(), serialized);
        image.set_physical_memory_name_ascii(0, "RETRY_OK").unwrap();
        assert_eq!(
            image.physical_memory_name_raw(0).unwrap(),
            Some(b"RETRY_OK        ".as_slice())
        );
        assert_eq!(image.original_bytes(), source);
    }
}
