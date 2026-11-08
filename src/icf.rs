//! Strict parsing and serialization of canonical IC-705 MapRev 6 ICF images.
//! Explicit strict MapRev 5-to-6 byte export is available separately from parsing.
//!
//! Unknown layouts are rejected, not repaired. Uninterpreted data is preserved;
//! a successful parse is not authorization to write the image to a radio.

use std::fmt;
use std::io::Write;

mod common;
mod dsp;
mod groups;
mod memory;
mod repeaters;

pub use common::{
    AltitudeDistanceUnit, AutoPowerOff, AutoTuneRxCallsignKeyFunction, BandEdgeBeep,
    BatteryPowerLimit, BluetoothAudioOutput, BluetoothAutoDisconnect, BluetoothHeadsetFunction,
    BluetoothSerialFunction, CoordinateDisplay, CsvDateFormat, CsvSeparatorDecimal, CwBfoSide,
    DeltaFSpan, DialAutoTs, ExternalPowerLimit, KeyboardInput, KeyboardLayout, Language,
    LockFunction, ManualNotchWidth, MemoryChannelSelectionScope, MemoryGroupDisplay,
    MemoryPadChannels, MicUpDownSpeed, NotchFilter, NotchKeyFunction, PlaybackSkipTime, PowerSave,
    PreRecordingTime, PressureUnit, RainfallUnit, ReceivedCallsignSpeech, RecordingCondition,
    RecordingMicrophone, RemotePowerOffMode, ScanMainDial, ScanPauseTime, ScanRestartTime,
    ScanSpeed, ScreenCaptureFileType, ScreenSaverDelay, ScrollSpeed, SelectMemoryScan,
    SpeechAlphabet, SpeechLanguage, SpeechLockKey, SpeechSpeed, SpeedUnit, TemperatureUnit,
    TemporarySkipTime, TransmitBandwidth, TransmitMeter, TransmitTimeout, TunerSelection,
    TxRecordingAudio, VoxBreakInKeyFunction, VoxVoiceDelay, WindSpeedUnit,
};
pub use dsp::{
    Agc, FilterBandGroup, FilterMode, FilterShape, FilterShapeSlot, FilterSlot, PbtSide, Preamp,
    StepAgcMode, ToneMode, TransmitBandwidthPreset, TransmitHigh, TransmitLow,
};
pub use groups::MemoryGroup;
pub use memory::{
    MemoryCallsign, MemoryDigitalSquelch, MemoryDuplex, MemoryFilter, MemoryMode, MemorySelection,
    MemorySide, MemoryTone,
};
pub use repeaters::{
    RepeaterCoordinate, RepeaterDuplex, RepeaterGroup, RepeaterMode, RepeaterPosition,
    RepeaterRecord, RepeaterTone,
};

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
// Raw group key 0x101 -> RVA 0x686B7 reads/writes descriptor name +3.
// RVA 0xB5170 traverses root/next; RVA 0xB5334 copies raw d names into
// working ordinal g order. RVA 0xB540C rebuilds links on original save.
// This raw API does neither transformation and requires a full 100-entry chain.
const RAW_MEMORY_GROUP_COUNT: usize = 100;
const RAW_MEMORY_GROUPS: usize = 0x355c0;
const RAW_MEMORY_GROUP_BYTES: usize = 19;
const RAW_MEMORY_GROUP_ROOT: usize = 0x36628;
const RAW_MEMORY_GROUP_NAME_BYTES: usize = 16;

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
/// Split and speech settings are statically traced; comparisons of their edits
/// with original-software saves remain pending.
pub struct Icf<'a> {
    source: &'a [u8],
    memory: Vec<u8>,
    comment: &'a [u8],
    extra_data: u32,
}

/// Convert strict MapRev 5 ICF bytes to canonical MapRev 6 bytes.
///
/// This explicit, non-lossless export updates five payload positions and normalizes
/// text using static CS-705 1.30 evidence (RVA 0xDF9F0..0xDFA36). It has not been
/// compared with the original GUI. It is not normal document open/save, performs
/// no file or device I/O, and does not authorize radio access. Input is unchanged.
///
/// # Errors
///
/// Rejects invalid structure, checksum, EtcData or signature, and any metadata
/// or internal revision other than exactly 5, before changing any payload byte.
pub fn convert_map_rev5_to6(source: &[u8]) -> Result<Vec<u8>, Error> {
    let (mut memory, comment, extra_data) = Icf::parse_image(source, 5)?;
    memory[INTERNAL_MAP_REV] = 6;
    memory[0x39724] = 0;
    memory[0x3a318..0x3a31b].fill(0);
    Ok(Icf {
        source,
        memory,
        comment,
        extra_data,
    }
    .to_bytes())
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
        let (memory, comment, extra_data) = Self::parse_image(source, 6)?;
        Ok(Self {
            source,
            memory,
            comment,
            extra_data,
        })
    }

    fn parse_image(source: &'a [u8], revision: u8) -> Result<(Vec<u8>, &'a [u8], u32), Error> {
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
        if metadata[1] != [b'0' + revision] {
            return Err(Error::Unsupported {
                line: 3,
                reason: if revision == 5 {
                    "MapRev other than 5"
                } else {
                    "MapRev other than 6"
                },
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
                // necessarily equal to metadata MapRev. Each entry point here
                // requires its exact internal revision; do not repair it.
                if memory[INTERNAL_MAP_REV] != revision {
                    return Err(Error::Unsupported {
                        line: 5 + INTERNAL_MAP_REV / RECORD_BYTES,
                        reason: if revision == 5 {
                            "internal MapRev other than 5"
                        } else {
                            "internal MapRev other than 6"
                        },
                    });
                }
                return Ok((memory, metadata[0], extra_data));
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

    /// Return an existing raw memory group descriptor's exact 16-byte name.
    ///
    /// `descriptor` is the stored zero-based index `0..100` in the unexpanded
    /// ICF image, not a GUI group ordinal or CS-705 working-buffer index. Names
    /// are borrowed without decoding, trimming or replacing NUL/non-ASCII bytes.
    /// Nonidentity stored orders are preserved, not reordered.
    ///
    /// This deliberately narrow subset requires the root/next chain to visit
    /// all 100 descriptors exactly once and then end with FF. Only the selected
    /// head is checked: it must be FFFF or a physical slot below 500. A head of
    /// FFFF with 16 spaces returns `None`; a named empty group still exists.
    /// Slot-next links, row maps, empty flags, the free list and other heads are
    /// not validated. This is not the original application's full acceptance
    /// policy or proof of complete group consistency or GUI compatibility.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for an out-of-range descriptor, a chain
    /// outside this full-100 subset, or an unsupported selected head. A parameter
    /// error is reported at line 5; stored-link/head errors identify their record.
    pub fn raw_memory_group_name(&self, descriptor: usize) -> Result<Option<&[u8]>, Error> {
        if descriptor >= RAW_MEMORY_GROUP_COUNT {
            return Err(Error::Unsupported {
                line: 5,
                reason: "raw memory group descriptor outside 0..100",
            });
        }
        self.raw_memory_group_order()?;
        let base = RAW_MEMORY_GROUPS + RAW_MEMORY_GROUP_BYTES * descriptor;
        let head = u16::from_le_bytes([self.memory[base + 1], self.memory[base + 2]]);
        if head != u16::MAX && usize::from(head) >= MEMORY_CHANNEL_COUNT {
            return Err(Error::Unsupported {
                line: 5 + (base + 1) / RECORD_BYTES,
                reason: "raw memory group head outside physical slots or FFFF",
            });
        }
        let name = &self.memory[base + 3..base + 3 + RAW_MEMORY_GROUP_NAME_BYTES];
        Ok(
            if head == u16::MAX && name.iter().all(|byte| *byte == b' ') {
                None
            } else {
                Some(name)
            },
        )
    }

    fn raw_memory_group_order(&self) -> Result<[usize; 100], Error> {
        let mut order = [0; RAW_MEMORY_GROUP_COUNT];
        let mut seen = [false; RAW_MEMORY_GROUP_COUNT];
        let mut link = RAW_MEMORY_GROUP_ROOT;
        for descriptor in &mut order {
            let next = usize::from(self.memory[link]);
            if next >= RAW_MEMORY_GROUP_COUNT || seen[next] {
                return Err(Error::Unsupported {
                    line: 5 + link / RECORD_BYTES,
                    reason: "raw memory group order must visit all 100 descriptors once",
                });
            }
            seen[next] = true;
            *descriptor = next;
            link = RAW_MEMORY_GROUPS + RAW_MEMORY_GROUP_BYTES * next;
        }
        if self.memory[link] != 0xff {
            return Err(Error::Unsupported {
                line: 5 + link / RECORD_BYTES,
                reason: "raw memory group order must end with FF after 100 descriptors",
            });
        }
        Ok(order)
    }

    /// Replace only an existing raw descriptor's name with `[A-Z0-9_-]{1,16}`.
    ///
    /// Short names are padded with spaces. This is an explicit ASCII subset,
    /// not the native name codec: no conversion, truncation or empty input is
    /// accepted. The checks in [`Self::raw_memory_group_name`] run first; no
    /// group is created, deleted, reordered or resolved to GUI coordinates.
    ///
    /// All validation precedes one 16-byte update. Errors leave all state
    /// unchanged; other memory, links, heads, raw metadata and the borrowed
    /// original input remain untouched. [`Self::to_bytes`] recomputes CD.
    /// CS-705 copies raw names to working ordinal order on load and rebuilds
    /// links on save; this method does neither. Original-GUI group-name edits
    /// and save/reload behavior have not been dynamically compared.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for a failed raw-descriptor check, an empty
    /// descriptor, or a new name outside the nonempty 1-to-16-byte ASCII subset.
    pub fn set_raw_memory_group_name_ascii(
        &mut self,
        descriptor: usize,
        name: &str,
    ) -> Result<(), Error> {
        let exists = self.raw_memory_group_name(descriptor)?.is_some();
        let offset = RAW_MEMORY_GROUPS + RAW_MEMORY_GROUP_BYTES * descriptor + 3;
        if !exists {
            return Err(Error::Unsupported {
                line: 5 + offset / RECORD_BYTES,
                reason: "cannot rename an empty raw memory group descriptor",
            });
        }
        if name.is_empty()
            || name.len() > RAW_MEMORY_GROUP_NAME_BYTES
            || !name.bytes().all(|byte| {
                byte.is_ascii_uppercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
            })
        {
            return Err(Error::Unsupported {
                line: 5 + offset / RECORD_BYTES,
                reason: "raw memory group name requires 1..=16 bytes in A-Z, 0-9, _ or -",
            });
        }
        let mut bytes = [b' '; RAW_MEMORY_GROUP_NAME_BYTES];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        self.memory[offset..offset + RAW_MEMORY_GROUP_NAME_BYTES].copy_from_slice(&bytes);
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

    fn rev5_memory() -> Vec<u8> {
        let mut memory: Vec<u8> = (0..IMAGE_BYTES).map(|i| (73 * i + 19) as u8).collect();
        memory[INTERNAL_MAP_REV] = 5;
        memory[IMAGE_BYTES - 16..].copy_from_slice(IMAGE_SIGNATURE);
        memory
    }

    fn rev5_fixture(memory: &[u8], revision: &str, comment: &[u8]) -> Vec<u8> {
        let mut text = b"40750001\r\n#Comment=".to_vec();
        text.extend_from_slice(comment);
        write!(text, "\r\n#MapRev={revision}\r\n#EtcData=12345639\r\n").unwrap();
        let mut digest = md5::Context::new();
        digest.consume(SEED);
        for (index, payload) in memory.chunks(32).enumerate() {
            let address = (index * 32) as u32;
            digest.consume(address.to_be_bytes());
            digest.consume([32]);
            digest.consume(payload);
            write!(text, "{address:08X}20").unwrap();
            for byte in payload {
                write!(text, "{byte:02X}").unwrap();
            }
            text.extend_from_slice(b"\r\n");
        }
        write!(text, "#CD={:X}\r\n", digest.finalize()).unwrap();
        text
    }

    #[test]
    fn converts_rev5_with_independent_expected_delta_and_checksums() {
        let memory = rev5_memory();
        let source = rev5_fixture(&memory, "5", b"R1 synthetic");
        let before = source.clone();
        // Independent Python hashlib oracle, not product serializer output.
        assert!(source.ends_with(b"#CD=EF17E362C508948229A775AA2629D996\r\n"));
        assert_eq!(source.len(), 589_709);
        let output = convert_map_rev5_to6(&source).unwrap();
        assert_eq!(source, before);
        assert_ne!(source.as_ptr(), output.as_ptr());
        assert_eq!(output.len(), 589_709);
        assert!(output.ends_with(b"#CD=8BFD5FD581A396CFAE06546AC1541CA5\r\n"));
        let image = Icf::parse(&output).unwrap();
        let differences: Vec<_> = memory
            .iter()
            .zip(image.memory())
            .enumerate()
            .filter_map(|(i, (old, new))| (old != new).then_some(i))
            .collect();
        assert_eq!(differences, [0x355a1, 0x39724, 0x3a318, 0x3a319, 0x3a31a]);
        for (offset, value) in [
            (0x355a1, 6),
            (0x39724, 0),
            (0x3a318, 0),
            (0x3a319, 0),
            (0x3a31a, 0),
            (0x399bb, 0x66),
            (0x399c0, 0xd3),
        ] {
            assert_eq!(image.memory()[offset], value);
        }
        assert_eq!(image.comment_bytes(), b"R1 synthetic");
        assert_eq!(image.extra_data(), 0x12345639);
        assert_eq!(image.to_bytes(), output);
        assert_eq!(
            convert_map_rev5_to6(&output),
            Err(Error::Unsupported {
                line: 3,
                reason: "MapRev other than 5",
            })
        );
        let lf: Vec<_> = source.iter().copied().filter(|b| *b != b'\r').collect();
        let mut lower = source.clone();
        // Lowercase only hex digits, not field names or the raw comment.
        let first = lower.windows(10).position(|s| s == b"0000000020").unwrap();
        lower[first..].make_ascii_lowercase();
        let cd = lower.len() - 38;
        lower[cd..cd + 4].copy_from_slice(b"#CD=");
        for variant in [&lf[..], &source[..source.len() - 2], &lower[..]] {
            assert_eq!(convert_map_rev5_to6(variant).unwrap(), output);
        }
        let raw = rev5_fixture(&memory, "5", &[0x82, 0xa0]);
        let converted = convert_map_rev5_to6(&raw).unwrap();
        assert_eq!(
            Icf::parse(&converted).unwrap().comment_bytes(),
            [0x82, 0xa0]
        );
    }

    #[test]
    fn rejects_invalid_rev5_before_conversion_without_mutating_input() {
        let memory = rev5_memory();
        let source = rev5_fixture(&memory, "5", b"R1 synthetic");
        let reject = |bytes: Vec<u8>, expected: Error| {
            let before = bytes.clone();
            assert_eq!(convert_map_rev5_to6(&bytes), Err(expected));
            assert_eq!(bytes, before);
        };
        for revision in ["0", "1", "2", "3", "4", "6", "7", "255", "05"] {
            reject(
                rev5_fixture(&memory, revision, b"R1 synthetic"),
                Error::Unsupported {
                    line: 3,
                    reason: "MapRev other than 5",
                },
            );
        }
        // Recompute CD for invalid internal versions/signatures so checksum
        // failures cannot hide missing profile validation.
        for revision in [0, 1, 2, 3, 4, 6, 7, 255] {
            let mut changed = memory.clone();
            changed[INTERNAL_MAP_REV] = revision;
            reject(
                rev5_fixture(&changed, "5", b"R1 synthetic"),
                Error::Unsupported {
                    line: 6834,
                    reason: "internal MapRev other than 5",
                },
            );
        }
        let mut changed = memory.clone();
        changed[IMAGE_BYTES - 1] = b'4';
        reject(
            rev5_fixture(&changed, "5", b"R1 synthetic"),
            Error::Invalid {
                line: 7762,
                reason: "image signature",
            },
        );
        let mut changed = source.clone();
        let cd = changed.len() - 34;
        changed[cd] = b'F';
        reject(changed, Error::ChecksumMismatch { line: 7763 });
        let mut changed = source.clone();
        let extra = changed.windows(8).position(|s| s == b"12345639").unwrap();
        changed[extra + 7] = b'8';
        reject(
            changed,
            Error::Invalid {
                line: 4,
                reason: "EtcData integrity bits",
            },
        );
        let mut changed = source.clone();
        let first = changed
            .windows(10)
            .position(|s| s == b"0000000020")
            .unwrap();
        changed[first + 10] = b'0';
        reject(changed, Error::ChecksumMismatch { line: 7763 });
        for end in [0, 8, first, first + 74, source.len() - 38] {
            let bytes = &source[..end];
            let before = bytes.to_vec();
            assert!(convert_map_rev5_to6(bytes).is_err());
            assert_eq!(bytes, before);
        }
    }

    fn scalar_memory_fixture() -> (Vec<u8>, Vec<u8>) {
        let mut memory = synthetic_memory();
        for (i, value) in memory[..IMAGE_BYTES - 16].iter_mut().enumerate() {
            *value = (73 * i + 19) as u8;
        }
        memory[INTERNAL_MAP_REV] = 6;
        for slot in 0..500 {
            let b = 83 * slot;
            memory[b] = 0x3f;
            memory[0x3662d + slot] = 0xfe;
            for at in [b + 1, b + 0x22] {
                memory[at..at + 4].copy_from_slice(&144_000_000_u32.to_le_bytes());
                memory[at + 4] = 0x70;
                memory[at + 5] = 0xf9;
                memory[at + 6..at + 12].fill(0);
                memory[at + 29..at + 33].fill(0);
            }
        }
        // Independent Python struct/hashlib: pattern (73*i+19)%256, all 500
        // occupied records, both sides FM, literal header/control bytes above.
        let text = fixture_with_memory(&memory, "6CEFD2C31EE2F114F5F868F9200960AB");
        (text, memory)
    }

    fn scalar_error(
        image: &mut Icf<'_>,
        invalid: bool,
        operation: impl FnOnce(&mut Icf<'_>) -> Result<(), Error>,
    ) -> Error {
        let before = image.memory().to_vec();
        let source = image.source;
        let error = operation(image).unwrap_err();
        assert!(if invalid {
            matches!(error, Error::Invalid { .. })
        } else {
            matches!(error, Error::Unsupported { .. })
        });
        assert_setting_state(image, source, &before);
        error
    }

    #[test]
    fn scalar_fm_codec_sweeps_all_500_slots_both_sides_and_preserves_windows() {
        let (source, mut expected) = scalar_memory_fixture();
        assert_eq!(
            format!("{:X}", md5::compute(&source)),
            "8EA14C5D34DD7F6CCE9723470265917E"
        );
        let mut image = Icf::parse(&source).unwrap();
        let tones = [
            MemoryTone::Off,
            MemoryTone::Tone,
            MemoryTone::ToneSquelch,
            MemoryTone::Dtcs,
            MemoryTone::DtcsTransmit,
            MemoryTone::ToneTransmitDtcsReceive,
            MemoryTone::DtcsTransmitToneSquelchReceive,
            MemoryTone::ToneTransmitToneSquelchReceive,
        ];
        for slot in 0..500 {
            let b = 83 * slot;
            image
                .set_physical_memory_selection(slot, MemorySelection::Three)
                .unwrap();
            expected[b] = 0xff;
            assert_setting_state(&image, &source, &expected);
            assert_eq!(
                image.physical_memory_selection(slot),
                Ok(MemorySelection::Three)
            );
            for (number, side) in [MemorySide::Main, MemorySide::Split]
                .into_iter()
                .enumerate()
            {
                let at = b + if number == 0 { 1 } else { 0x22 };
                let hz = 30_000 + slot as u32 * 1000 + number as u32;
                image
                    .set_physical_memory_frequency_hz(slot, side, hz)
                    .unwrap();
                expected[at..at + 4].copy_from_slice(&hz.to_le_bytes());
                assert_setting_state(&image, &source, &expected);
                assert_eq!(image.physical_memory_frequency_hz(slot, side), Ok(hz));
                assert_eq!(image.physical_memory_mode(slot, side), Ok(MemoryMode::Fm));
                assert_eq!(image.physical_memory_data(slot, side), Ok(true));
                image
                    .set_physical_memory_filter(slot, side, MemoryFilter::Filter3)
                    .unwrap();
                expected[at + 4] = 0x72;
                assert_setting_state(&image, &source, &expected);
                assert_eq!(
                    image.physical_memory_filter(slot, side),
                    Ok(MemoryFilter::Filter3)
                );
                image
                    .set_physical_memory_duplex(slot, side, MemoryDuplex::Plus)
                    .unwrap();
                expected[at + 5] = 0xfd;
                assert_setting_state(&image, &source, &expected);
                assert_eq!(
                    image.physical_memory_duplex(slot, side),
                    Ok(MemoryDuplex::Plus)
                );
                let offset = (slot as u32 * 2 + number as u32) * 100;
                image
                    .set_physical_memory_offset_hz(slot, side, offset)
                    .unwrap();
                expected[at + 29..at + 33].copy_from_slice(&offset.to_le_bytes());
                assert_setting_state(&image, &source, &expected);
                assert_eq!(image.physical_memory_offset_hz(slot, side), Ok(offset));
                // Grey UI conditions are not storage guards: tone OFF still
                // permits the saved TX/RX indices and polarity to be edited.
                for value in [0, 49] {
                    image
                        .set_physical_memory_tx_tone_index(slot, side, value)
                        .unwrap();
                    expected[at + 9] = value;
                    assert_setting_state(&image, &source, &expected);
                    assert_eq!(image.physical_memory_tx_tone_index(slot, side), Ok(value));
                    image
                        .set_physical_memory_rx_tone_index(slot, side, value)
                        .unwrap();
                    expected[at + 10] = value;
                    assert_setting_state(&image, &source, &expected);
                    assert_eq!(image.physical_memory_rx_tone_index(slot, side), Ok(value));
                }
                for value in [0, 103] {
                    image
                        .set_physical_memory_dtcs_index(slot, side, value)
                        .unwrap();
                    expected[at + 11] = value;
                    assert_setting_state(&image, &source, &expected);
                    assert_eq!(image.physical_memory_dtcs_index(slot, side), Ok(value));
                }
                for (raw, value) in tones.into_iter().enumerate() {
                    image.set_physical_memory_tone(slot, side, value).unwrap();
                    expected[at + 6] = raw as u8;
                    assert_setting_state(&image, &source, &expected);
                    assert_eq!(image.physical_memory_tone(slot, side), Ok(value));
                }
                for (rx, tx) in [(false, false), (false, true), (true, true), (true, false)] {
                    image
                        .set_physical_memory_dtcs_polarity(slot, side, (rx, tx))
                        .unwrap();
                    expected[at + 7..at + 9].copy_from_slice(&[u8::from(rx), u8::from(tx)]);
                    assert_setting_state(&image, &source, &expected);
                    assert_eq!(
                        image.physical_memory_dtcs_polarity(slot, side),
                        Ok((rx, tx))
                    );
                }
            }
            image.set_physical_memory_split(slot, false).unwrap();
            expected[b] = 0xfd;
            assert_setting_state(&image, &source, &expected);
            assert!(
                image
                    .physical_memory_frequency_hz(slot, MemorySide::Split)
                    .is_err()
            );
            image.set_physical_memory_split(slot, true).unwrap();
            expected[b] = 0xff;
            assert_setting_state(&image, &source, &expected);
        }
        let output = image.to_bytes();
        assert!(output.ends_with(b"#CD=6268052E7F747D0B9F65B85CBB178D67\r\n"));
        assert_eq!(
            format!("{:X}", md5::compute(&output)),
            "DD6C4770A2E85D0F7E7951A3EAE668C2"
        );
        let reparsed = Icf::parse(&output).unwrap();
        assert_setting_state(&reparsed, &output, &expected);
        assert_eq!(reparsed.to_bytes(), output);
    }

    #[test]
    fn scalar_dv_overlay_and_literal_callsign_packing_cover_both_sides_all_slots() {
        let (source, mut expected) = scalar_memory_fixture();
        let mut image = Icf::parse(&source).unwrap();
        let space = [0x40, 0x81, 2, 4, 8, 0x10, 0x20];
        let first = [0x83, 0x0a, 0x1c, 0x48, 0xb1, 0xa3, 0xc8];
        let second = [0xa9, 0x16, 0x9d, 0x46, 0x0c, 0x50, 0x42];
        for slot in 0..500 {
            for (side, at) in [
                (MemorySide::Main, 83 * slot + 1),
                (MemorySide::Split, 83 * slot + 0x22),
            ] {
                // Test setup only: the API deliberately has no mode/DATA setter.
                image.memory[at + 4] = 0x90;
                image.memory[at + 6..at + 8].fill(0);
                for start in [at + 8, at + 15, at + 22] {
                    image.memory[start..start + 7].copy_from_slice(&space);
                }
                expected[at + 4] = 0x90;
                expected[at + 6..at + 8].fill(0);
                for start in [at + 8, at + 15, at + 22] {
                    expected[start..start + 7].copy_from_slice(&space);
                }
                for value in [0, 99] {
                    image
                        .set_physical_memory_digital_code(slot, side, value)
                        .unwrap();
                    expected[at + 7] = value;
                    assert_setting_state(&image, &source, &expected);
                    assert_eq!(image.physical_memory_digital_code(slot, side), Ok(value));
                }
                for (raw, value) in [
                    MemoryDigitalSquelch::Off,
                    MemoryDigitalSquelch::Callsign,
                    MemoryDigitalSquelch::Code,
                ]
                .into_iter()
                .enumerate()
                {
                    image
                        .set_physical_memory_digital_squelch(slot, side, value)
                        .unwrap();
                    expected[at + 6] = raw as u8;
                    assert_setting_state(&image, &source, &expected);
                    assert_eq!(image.physical_memory_digital_squelch(slot, side), Ok(value));
                }
                for (which, text, packed, start) in [
                    (MemoryCallsign::Your, *b"ABCDEFGH", first, at + 8),
                    (MemoryCallsign::Repeater1, *b"TEST01 B", second, at + 15),
                    (MemoryCallsign::Repeater2, *b"ABCDEFGH", first, at + 22),
                ] {
                    image
                        .set_physical_memory_callsign_raw(slot, side, which, text)
                        .unwrap();
                    expected[start..start + 7].copy_from_slice(&packed);
                    assert_setting_state(&image, &source, &expected);
                    assert_eq!(
                        image.physical_memory_callsign_raw(slot, side, which),
                        Ok(text)
                    );
                }
                assert_eq!(image.physical_memory_mode(slot, side), Ok(MemoryMode::Dv));
            }
        }
        let output = image.to_bytes();
        assert!(output.ends_with(b"#CD=9A7AA4B2DC71EF4A49C2F92C431D5CCD\r\n"));
        assert_eq!(
            format!("{:X}", md5::compute(&output)),
            "5CD23559245AAD4BDCCF9667926396F3"
        );
        let reparsed = Icf::parse(&output).unwrap();
        assert_setting_state(&reparsed, &output, &expected);
        assert_eq!(reparsed.to_bytes(), output);
    }

    #[test]
    fn scalar_guards_ranges_modes_and_masks_reject_without_state_changes() {
        let (source, _) = scalar_memory_fixture();
        let mut image = Icf::parse(&source).unwrap();
        let side = MemorySide::Main;
        for slot in [500, usize::MAX] {
            let e = scalar_error(&mut image, false, |i| {
                i.set_physical_memory_frequency_hz(slot, side, 30_000)
            });
            assert!(matches!(e, Error::Unsupported { line: 5, .. }));
            scalar_error(&mut image, true, |i| {
                i.set_physical_memory_frequency_hz(slot, side, 0)
            });
            scalar_error(&mut image, false, |i| {
                i.set_physical_memory_split(slot, true)
            });
        }
        image.memory[0x3662d] = 0xff;
        let e = scalar_error(&mut image, false, |i| {
            i.set_physical_memory_frequency_hz(0, side, 30_000)
        });
        assert!(matches!(e,Error::Unsupported {line,..} if line==5+0x3662d/32));
        image.memory[0x3662d] = 0xfe;
        image.set_physical_memory_split(0, false).unwrap();
        scalar_error(&mut image, false, |i| {
            i.set_physical_memory_frequency_hz(0, MemorySide::Split, 30_000)
        });
        image.set_physical_memory_split(0, true).unwrap();
        for raw in [0, u32::MAX] {
            image.memory[1..5].copy_from_slice(&raw.to_le_bytes());
            assert_eq!(image.physical_memory_frequency_hz(0, side), Ok(raw));
            image
                .set_physical_memory_frequency_hz(0, side, 30_000)
                .unwrap();
        }
        for value in [0, 29_999, 470_000_001, u32::MAX] {
            scalar_error(&mut image, true, |i| {
                i.set_physical_memory_frequency_hz(0, side, value)
            });
        }
        for value in [30_000, 470_000_000] {
            image
                .set_physical_memory_frequency_hz(0, side, value)
                .unwrap();
        }
        for value in [0, 100, 9_999_900] {
            image.set_physical_memory_offset_hz(0, side, value).unwrap();
            assert_eq!(image.physical_memory_offset_hz(0, side), Ok(value));
        }
        for value in [1, 49, 50, 51, 9_999_901, 10_000_000, u32::MAX] {
            scalar_error(&mut image, true, |i| {
                i.set_physical_memory_offset_hz(0, side, value)
            });
            image.memory[30..34].copy_from_slice(&value.to_le_bytes());
            scalar_error(&mut image, false, |i| {
                i.set_physical_memory_offset_hz(0, side, 0)
            });
            scalar_error(&mut image, true, |i| {
                i.set_physical_memory_offset_hz(0, side, 1)
            });
            image.memory[30..34].fill(0);
        }
        let modes = [
            MemoryMode::Lsb,
            MemoryMode::Usb,
            MemoryMode::Cw,
            MemoryMode::CwReverse,
            MemoryMode::Rtty,
            MemoryMode::RttyReverse,
            MemoryMode::Am,
            MemoryMode::Fm,
            MemoryMode::Wfm,
            MemoryMode::Dv,
        ];
        for raw in 0..16 {
            image.memory[5] = (raw << 4) | 2;
            assert_eq!(
                image.physical_memory_filter(0, side),
                Ok(MemoryFilter::Filter3)
            );
            if raw < 10 {
                assert_eq!(image.physical_memory_mode(0, side), Ok(modes[raw as usize]));
            } else {
                scalar_error(&mut image, false, |i| {
                    i.physical_memory_mode(0, side).map(|_| ())
                });
            }
            if raw == 8 || raw >= 10 {
                scalar_error(&mut image, false, |i| {
                    i.set_physical_memory_filter(0, side, MemoryFilter::Filter1)
                });
            } else {
                image
                    .set_physical_memory_filter(0, side, MemoryFilter::Filter1)
                    .unwrap();
                assert_eq!(image.memory[5], raw << 4);
            }
            if raw != 7 {
                scalar_error(&mut image, false, |i| {
                    i.set_physical_memory_tone(0, side, MemoryTone::Off)
                });
            }
            if raw != 9 {
                scalar_error(&mut image, false, |i| {
                    i.set_physical_memory_digital_squelch(0, side, MemoryDigitalSquelch::Off)
                });
            }
            // DUP/offset do not acquire a mode validation guard.
            image.memory[6] = 0xf9;
            image
                .set_physical_memory_duplex(0, side, MemoryDuplex::Minus)
                .unwrap();
            image.set_physical_memory_offset_hz(0, side, 100).unwrap();
        }
        image.memory[5] = 0x70;
        for (raw, value) in [
            MemoryFilter::Filter1,
            MemoryFilter::Filter2,
            MemoryFilter::Filter3,
        ]
        .into_iter()
        .enumerate()
        {
            image.set_physical_memory_filter(0, side, value).unwrap();
            assert_eq!(image.memory[5], 0x70 | raw as u8);
            assert_eq!(image.physical_memory_filter(0, side), Ok(value));
        }
        for raw in 3..16 {
            image.memory[5] = 0x70 | raw;
            scalar_error(&mut image, false, |i| {
                i.set_physical_memory_filter(0, side, MemoryFilter::Filter1)
            });
        }
        image.memory[5] = 0x70;
        for raw in 0..=255 {
            image.memory[6] = raw;
            assert_eq!(image.physical_memory_data(0, side), Ok(raw & 1 != 0));
            if (raw >> 1) & 3 == 3 {
                scalar_error(&mut image, false, |i| {
                    i.set_physical_memory_duplex(0, side, MemoryDuplex::Off)
                });
            } else {
                let values = [MemoryDuplex::Off, MemoryDuplex::Minus, MemoryDuplex::Plus];
                assert_eq!(
                    image.physical_memory_duplex(0, side),
                    Ok(values[((raw >> 1) & 3) as usize])
                );
                image
                    .set_physical_memory_duplex(0, side, MemoryDuplex::Plus)
                    .unwrap();
                assert_eq!(image.memory[6], (raw & !6) | 4);
            }
            image.memory[0] = raw;
            assert_eq!(image.physical_memory_split(0), Ok(raw & 2 != 0));
            let selections = [
                MemorySelection::Off,
                MemorySelection::One,
                MemorySelection::Two,
                MemorySelection::Three,
            ];
            assert_eq!(
                image.physical_memory_selection(0),
                Ok(selections[(raw >> 6) as usize])
            );
            image
                .set_physical_memory_selection(0, MemorySelection::Two)
                .unwrap();
            assert_eq!(image.memory[0], (raw & 0x3f) | 0x80);
            image.set_physical_memory_split(0, true).unwrap();
            assert_eq!(image.memory[0], (raw & 0x3d) | 0x82);
        }
    }

    #[test]
    fn scalar_overlay_boundaries_raw_alphabets_and_callsign_coupling_are_strict() {
        let (source, _) = scalar_memory_fixture();
        let mut image = Icf::parse(&source).unwrap();
        for (side, t) in [(MemorySide::Main, 1), (MemorySide::Split, 0x22)] {
            for bad in [8, 255] {
                image.memory[t + 6] = bad;
                scalar_error(&mut image, false, |i| {
                    i.set_physical_memory_tone(0, side, MemoryTone::Off)
                });
            }
            image.memory[t + 6] = 0;
            for (at, max) in [(9, 49), (10, 49), (11, 103)] {
                for bad in [max + 1, 255] {
                    image.memory[t + at] = bad;
                    match at {
                        9 => {
                            scalar_error(&mut image, false, |i| {
                                i.set_physical_memory_tx_tone_index(0, side, 0)
                            });
                            scalar_error(&mut image, true, |i| {
                                i.set_physical_memory_tx_tone_index(0, side, bad)
                            });
                        }
                        10 => {
                            scalar_error(&mut image, false, |i| {
                                i.set_physical_memory_rx_tone_index(0, side, 0)
                            });
                            scalar_error(&mut image, true, |i| {
                                i.set_physical_memory_rx_tone_index(0, side, bad)
                            });
                        }
                        _ => {
                            scalar_error(&mut image, false, |i| {
                                i.set_physical_memory_dtcs_index(0, side, 0)
                            });
                            scalar_error(&mut image, true, |i| {
                                i.set_physical_memory_dtcs_index(0, side, bad)
                            });
                        }
                    }
                }
                image.memory[t + at] = 0;
            }
            for at in [7, 8] {
                for bad in [2, 255] {
                    image.memory[t + at] = bad;
                    scalar_error(&mut image, false, |i| {
                        i.set_physical_memory_dtcs_polarity(0, side, (false, false))
                    });
                }
                image.memory[t + at] = 0;
            }
            image.memory[t + 4] = 0x90;
            for bad in [3, 255] {
                image.memory[t + 6] = bad;
                scalar_error(&mut image, false, |i| {
                    i.set_physical_memory_digital_squelch(0, side, MemoryDigitalSquelch::Off)
                });
            }
            image.memory[t + 6] = 0;
            for bad in [100, 255] {
                image.memory[t + 7] = bad;
                scalar_error(&mut image, false, |i| {
                    i.set_physical_memory_digital_code(0, side, 0)
                });
                scalar_error(&mut image, true, |i| {
                    i.set_physical_memory_digital_code(0, side, bad)
                });
            }
            image.memory[t + 7] = 0;
            // Every 7-bit stored alphabet remains readable, including NUL/DEL.
            for raw in 0..128_u8 {
                let bits = (0..8).fold(0_u64, |acc, _| acc << 7 | u64::from(raw));
                image.memory[t + 8..t + 15].copy_from_slice(&bits.to_be_bytes()[1..]);
                assert_eq!(
                    image.physical_memory_callsign_raw(0, side, MemoryCallsign::Your),
                    Ok([raw; 8])
                );
            }
            let space = [0x40, 0x81, 2, 4, 8, 0x10, 0x20];
            image.memory[t + 15..t + 22].copy_from_slice(&space);
            image.memory[t + 22..t + 29].copy_from_slice(&space);
            for which in [
                MemoryCallsign::Your,
                MemoryCallsign::Repeater1,
                MemoryCallsign::Repeater2,
            ] {
                for bad in [*b"lower   ", [0; 8], [127; 8], [128; 8], *b"BAD_    "] {
                    scalar_error(&mut image, true, |i| {
                        i.set_physical_memory_callsign_raw(0, side, which, bad)
                    });
                }
            }
            image
                .set_physical_memory_callsign_raw(0, side, MemoryCallsign::Your, *b"/A0    /")
                .unwrap();
            image
                .set_physical_memory_callsign_raw(0, side, MemoryCallsign::Repeater1, *b"RPT123 G")
                .unwrap();
            scalar_error(&mut image, true, |i| {
                i.set_physical_memory_callsign_raw(0, side, MemoryCallsign::Repeater2, *b"ABCDEFGH")
            });
            // Seed an inconsistent stored pair; changing Your must not bypass
            // the shared post-edit validator, but either repeater can resolve it.
            image.memory[t + 22..t + 29]
                .copy_from_slice(&[0x83, 0x0a, 0x1c, 0x48, 0xb1, 0xa3, 0xc8]);
            scalar_error(&mut image, true, |i| {
                i.set_physical_memory_callsign_raw(0, side, MemoryCallsign::Your, *b"ABCDEFGH")
            });
            image
                .set_physical_memory_callsign_raw(0, side, MemoryCallsign::Repeater2, [b' '; 8])
                .unwrap();
            image.memory[t + 22..t + 29]
                .copy_from_slice(&[0x83, 0x0a, 0x1c, 0x48, 0xb1, 0xa3, 0xc8]);
            image
                .set_physical_memory_callsign_raw(0, side, MemoryCallsign::Repeater1, *b"RPT123 B")
                .unwrap();
            scalar_error(&mut image, true, |i| {
                i.set_physical_memory_callsign_raw(0, side, MemoryCallsign::Repeater1, *b"RPT123 G")
            });
            // Preserve an unrelated nonalphabetic raw Your window on a repeater edit.
            image.memory[t + 8..t + 15].fill(0xff);
            image
                .set_physical_memory_callsign_raw(0, side, MemoryCallsign::Repeater2, [b' '; 8])
                .unwrap();
            assert_eq!(&image.memory[t + 8..t + 15], &[0xff; 7]);
            image.memory[t + 4] = 0x70;
            scalar_error(&mut image, false, |i| {
                i.set_physical_memory_callsign_raw(0, side, MemoryCallsign::Your, *b"ABCDEFGH")
            });
            scalar_error(&mut image, true, |i| {
                i.set_physical_memory_callsign_raw(0, side, MemoryCallsign::Your, [0; 8])
            });
        }
    }

    #[test]
    fn canonical_home_ntp_and_payload_comment_preserve_raw_metadata_and_neighbors() {
        let mut expected = synthetic_memory();
        for (i, byte) in expected[..IMAGE_BYTES - 16].iter_mut().enumerate() {
            *byte = (73 * i + 19) as u8;
        }
        expected[0x355a1] = 6;
        expected[0x392f6..0x392fa].fill(0);
        expected[0x396be..0x396fe].fill(0xff);
        expected[0x35591..0x355a1].fill(0xff);
        // Independent Python struct/hashlib, not product setters or serializer:
        // pattern (73*i+19)%256, internal6/signature, Home0 and raw FF text.
        // Final splices are LE470000000, b'time.Example-0.test'.ljust(64,b' '),
        // b'CQ / test! 01'.ljust(16,b' '); header raw comment remains 82 A0.
        let source = fixture_with_memory(&expected, "A2B67E1FF0A95BEC262171A1B6238964");
        assert_eq!(
            format!("{:X}", md5::compute(&source)),
            "4BC27EAEE87DC4A0C4980A35F49BC52E"
        );
        let mut image = Icf::parse(&source).unwrap();
        assert_eq!(image.home_frequency_hz(), Ok(None));
        assert_eq!(image.ntp_server_raw(), &[0xff; 64]);
        assert_eq!(image.clone_comment_raw(), &[0xff; 16]);
        assert_eq!(
            image.ntp_server_raw().as_ptr(),
            image.memory()[0x396be..].as_ptr()
        );
        assert_eq!(
            image.clone_comment_raw().as_ptr(),
            image.memory()[0x35591..].as_ptr()
        );
        macro_rules! rejected {
            ($operation:expr,$kind:ident,$offset:literal) => {{
                let before=image.memory().to_vec();
                let serialized=image.to_bytes();
                let error=$operation.unwrap_err();
                assert!(matches!(error,Error::$kind {line,..} if line==5+$offset/32));
                assert_setting_state(&image,&source,&before);
                assert_eq!(image.to_bytes(),serialized);
            }};
        }
        for (value, bytes) in [
            (None, [0, 0, 0, 0]),
            (Some(30_000), [0x30, 0x75, 0, 0]),
            (Some(199_999_999), [0xff, 0xc1, 0xeb, 0x0b]),
            (Some(400_000_000), [0, 0x84, 0xd7, 0x17]),
            (Some(470_000_000), [0x80, 0xa1, 3, 0x1c]),
            (None, [0, 0, 0, 0]),
        ] {
            image.set_home_frequency_hz(value).unwrap();
            expected[0x392f6..0x392fa].copy_from_slice(&bytes);
            assert_eq!(image.home_frequency_hz(), Ok(value));
            assert_setting_state(&image, &source, &expected);
        }
        for bad in [
            0,
            1,
            29_999,
            200_000_000,
            399_999_999,
            470_000_001,
            u32::MAX,
        ] {
            rejected!(image.set_home_frequency_hz(Some(bad)), Invalid, 0x392f6);
        }
        for bad in [1, 29_999, 200_000_000, 399_999_999, 470_000_001, u32::MAX] {
            // Test setup only; the public setter never repairs unknown Home Hz.
            image.memory[0x392f6..0x392fa].copy_from_slice(&bad.to_le_bytes());
            rejected!(image.home_frequency_hz(), Unsupported, 0x392f6);
            rejected!(image.set_home_frequency_hz(None), Unsupported, 0x392f6);
            rejected!(
                image.set_home_frequency_hz(Some(30_000)),
                Unsupported,
                0x392f6
            );
            rejected!(image.set_home_frequency_hz(Some(0)), Invalid, 0x392f6);
            image.memory[0x392f6..0x392fa].fill(0);
        }
        let full_ntp = "A".repeat(64);
        for text in ["", full_ntp.as_str(), "-.", "time.Example-0.test", ""] {
            image.set_ntp_server_ascii(text).unwrap();
            expected[0x396be..0x396fe].fill(b' ');
            expected[0x396be..0x396be + text.len()].copy_from_slice(text.as_bytes());
            assert_eq!(image.ntp_server_raw(), &expected[0x396be..0x396fe]);
            assert_setting_state(&image, &source, &expected);
        }
        let over_ntp = "A".repeat(65);
        for text in [
            over_ntp.as_str(),
            "with space",
            "bad_",
            "a/b",
            "é",
            "a\0b",
            "\n",
        ] {
            rejected!(image.set_ntp_server_ascii(text), Invalid, 0x396be);
        }
        for text in ["", "ABCD EFGH-01/234", "A  B!", ""] {
            image.set_clone_comment_canonical_ascii(text).unwrap();
            expected[0x35591..0x355a1].fill(b' ');
            expected[0x35591..0x35591 + text.len()].copy_from_slice(text.as_bytes());
            assert_eq!(image.clone_comment_raw(), &expected[0x35591..0x355a1]);
            assert_setting_state(&image, &source, &expected);
        }
        let over_comment = "A".repeat(17);
        for text in [
            over_comment.as_str(),
            " leading",
            "trailing ",
            " ",
            "\0",
            "a\nb",
            "é",
            "\x7f",
        ] {
            rejected!(
                image.set_clone_comment_canonical_ascii(text),
                Invalid,
                0x35591
            );
        }
        // Explicit text replacement accepts unknown current bytes without decoding.
        image.memory[0x396be..0x396fe].fill(0xff);
        image.memory[0x35591..0x355a1].fill(0xff);
        expected[0x396be..0x396fe].fill(0xff);
        expected[0x35591..0x355a1].fill(0xff);
        image.set_ntp_server_ascii("time.Example-0.test").unwrap();
        expected[0x396be..0x396fe].fill(b' ');
        expected[0x396be..0x396be + 19].copy_from_slice(b"time.Example-0.test");
        assert_setting_state(&image, &source, &expected);
        image
            .set_clone_comment_canonical_ascii("CQ / test! 01")
            .unwrap();
        expected[0x35591..0x355a1].fill(b' ');
        expected[0x35591..0x35591 + 13].copy_from_slice(b"CQ / test! 01");
        assert_setting_state(&image, &source, &expected);
        image.set_home_frequency_hz(Some(470_000_000)).unwrap();
        expected[0x392f6..0x392fa].copy_from_slice(&[0x80, 0xa1, 3, 0x1c]);
        assert_setting_state(&image, &source, &expected);
        assert_eq!(
            [
                image.memory()[0x392fa],
                image.memory()[0x396fe],
                image.memory()[0x396ff],
                image.memory()[0x39700],
                image.memory()[0x355a1]
            ],
            [0x5d, 0x81, 0xca, 0x13, 6]
        );
        let output = image.to_bytes();
        assert_eq!(output.len(), 589_699);
        assert!(output.ends_with(b"#CD=0C5B263FB7916A5EA19D7D1FFF5F58AC\r\n"));
        assert_eq!(
            format!("{:X}", md5::compute(&output)),
            "405055D3E819CEE09C9B074C916CADBC"
        );
        let reparsed = Icf::parse(&output).unwrap();
        assert_setting_state(&reparsed, &output, &expected);
        assert_eq!(reparsed.home_frequency_hz(), Ok(Some(470_000_000)));
        assert_eq!(reparsed.ntp_server_raw(), image.ntp_server_raw());
        assert_eq!(reparsed.clone_comment_raw(), image.clone_comment_raw());
        assert_eq!(reparsed.to_bytes(), output);
    }

    fn repeater_map_fixture() -> (Vec<u8>, Vec<u8>) {
        let mut memory = synthetic_memory();
        for (i, byte) in memory[..IMAGE_BYTES - 16].iter_mut().enumerate() {
            *byte = (73 * i + 19) as u8;
        }
        memory[0x355a1] = 6;
        memory[0x3ab82..0x3bf0a].fill(255);
        memory[0x3ab82..0x3ab88].copy_from_slice(&[2, 0, 0, 0, 1, 0]);
        memory[0x3bf0a..0x3bf6e].fill(255);
        memory[0x3bf0a..0x3bf0c].fill(0);
        memory[0x3bf0a + 98..0x3bf0a + 100].copy_from_slice(&[2, 0]);
        memory[0x2ea3b..0x2ea3e].copy_from_slice(&[0x82, 0, 255]);
        memory[0xaa0f..0xaa0f + 52].fill(255);
        memory[0xaa0f + 104..0xaa0f + 156].fill(0);
        memory[0x3bf6e..0x3bf6e + 313].fill(0xa0);
        memory[0x3bf6e + 312] = 0xf8;
        // Independent Python struct/hashlib: pattern (73*i+19)%256, signature,
        // internal6, order [2,0,1], group0 start0/group49 start2, USE A0/F8.
        let source = fixture_with_memory(&memory, "EBD5DA6CD00A0F9AA10336569DFB79E9");
        (source, memory)
    }

    #[test]
    fn repeater_raw_map_names_records_and_use_preserve_all_opaque_bytes() {
        let (source, mut expected) = repeater_map_fixture();
        assert_eq!(
            format!("{:X}", md5::compute(&source)),
            "647EA2B150A5624C26EC96C397BA3CE7"
        );
        let mut image = Icf::parse(&source).unwrap();
        {
            let groups = image.repeater_groups().unwrap();
            assert_eq!(groups.len(), 50);
            assert_eq!(groups[0].physical_slots(), &[2, 0]);
            assert_eq!(groups[49].physical_slots(), &[1]);
            assert!(groups[1..49].iter().all(|g| g.physical_slots().is_empty()));
            for (g, group) in groups.iter().enumerate() {
                assert_eq!(
                    group.name_bytes(),
                    &expected[0x2ea3b + 16 * g..0x2ea3b + 16 * (g + 1)]
                );
                assert_eq!(
                    group.name_bytes().as_ptr(),
                    image.memory()[0x2ea3b + 16 * g..].as_ptr()
                );
            }
            assert_eq!(&groups[0].name_bytes()[..3], &[0x82, 0, 255]);
        }
        assert_eq!(
            image.physical_repeater_record_raw(2).unwrap(),
            Some([0; 52].as_slice())
        );
        assert_eq!(
            image.physical_repeater_record_raw(0).unwrap(),
            Some([255; 52].as_slice())
        );
        assert_eq!(image.physical_repeater_record_raw(2499).unwrap(), None);
        assert_eq!(image.physical_repeater_use(2499).unwrap(), None); // USE bit is set, but unused.
        scalar_error(&mut image, false, |i| {
            i.set_physical_repeater_use(2499, true)
        });
        for (p, value) in [(2, true), (0, false), (1, true)] {
            image.set_physical_repeater_use(p, value).unwrap();
            let at = 0x3bf6e + (p >> 3);
            let mask = 1 << (p & 7);
            expected[at] = (expected[at] & !mask) | if value { mask } else { 0 };
            assert_setting_state(&image, &source, &expected);
            assert_eq!(image.physical_repeater_use(p).unwrap(), Some(value));
        }
        for (g, text) in [(0, "A Test / Group!!"), (1, " raw "), (1, ""), (49, "last")] {
            image.set_repeater_group_name_ascii(g, text).unwrap();
            expected[0x2ea3b + 16 * g..0x2ea3b + 16 * (g + 1)].fill(b' ');
            expected[0x2ea3b + 16 * g..0x2ea3b + 16 * g + text.len()]
                .copy_from_slice(text.as_bytes());
            assert_setting_state(&image, &source, &expected);
        }
        let small = image.to_bytes();
        assert!(small.ends_with(b"#CD=4F758A8B5960356FCD00E6D0FF57A44A\r\n"));
        assert_eq!(
            format!("{:X}", md5::compute(&small)),
            "D36F1EA98784BA60D65153C0390093FC"
        );
        assert_eq!(Icf::parse(&small).unwrap().to_bytes(), small);
        // Full-permutation test setup, not a public allocator or map editor.
        let order: Vec<usize> = (0..2500).map(|j| (73 * j + 19) % 2500).collect();
        for (row, p) in order.iter().enumerate() {
            let bytes = (*p as u16).to_le_bytes();
            image.memory[0x3ab82 + 2 * row..0x3ab84 + 2 * row].copy_from_slice(&bytes);
            expected[0x3ab82 + 2 * row..0x3ab84 + 2 * row].copy_from_slice(&bytes);
        }
        image.memory[0x3bf0a..0x3bf6e].fill(255);
        expected[0x3bf0a..0x3bf6e].fill(255);
        for (g, bytes) in [(1, [0, 0]), (49, [0xc3, 9])] {
            // LE2499
            image.memory[0x3bf0a + 2 * g..0x3bf0c + 2 * g].copy_from_slice(&bytes);
            expected[0x3bf0a + 2 * g..0x3bf0c + 2 * g].copy_from_slice(&bytes);
        }
        {
            let groups = image.repeater_groups().unwrap();
            assert_eq!(groups.len(), 50);
            assert!(groups[0].physical_slots().is_empty());
            assert_eq!(groups[1].physical_slots(), &order[..2499]);
            assert_eq!(groups[49].physical_slots(), &[2446]); // Independent last permutation element.
            assert_eq!(
                groups
                    .iter()
                    .map(|g| g.physical_slots().len())
                    .sum::<usize>(),
                2500
            );
        }
        for p in 0..2500 {
            let record = image.physical_repeater_record_raw(p).unwrap().unwrap();
            assert_eq!(record, &expected[0xaa0f + 52 * p..0xaa0f + 52 * (p + 1)]);
            assert_eq!(record.as_ptr(), image.memory()[0xaa0f + 52 * p..].as_ptr());
            let at = 0x3bf6e + (p >> 3);
            let mask = 1 << (p & 7);
            let old = expected[at] & mask != 0;
            assert_eq!(image.physical_repeater_use(p).unwrap(), Some(old));
            for value in [!old, old, p.is_multiple_of(2)] {
                image.set_physical_repeater_use(p, value).unwrap();
                expected[at] = (expected[at] & !mask) | if value { mask } else { 0 };
                assert_setting_state(&image, &source, &expected);
                assert_eq!(image.physical_repeater_use(p).unwrap(), Some(value));
            }
        }
        assert_eq!(image.memory()[0x3bf6e + 312], 0xf5); // p2499 is bit3; opaque upper nibble stays F.
        let output = image.to_bytes();
        assert_eq!(output.len(), 589_699);
        assert!(output.ends_with(b"#CD=FA3C6E27C39F0548F2D35015D02953E6\r\n"));
        assert_eq!(
            format!("{:X}", md5::compute(&output)),
            "A10A7F7FEEC0409836D9E661B942A93B"
        );
        let reparsed = Icf::parse(&output).unwrap();
        assert_setting_state(&reparsed, &output, &expected);
        assert_eq!(reparsed.to_bytes(), output);
        assert_eq!(
            reparsed.repeater_groups().unwrap()[49].physical_slots(),
            &[2446]
        );
    }

    #[test]
    fn repeater_order_start_and_parameter_errors_are_strict_and_state_preserving() {
        let (source, baseline) = repeater_map_fixture();
        let mut image = Icf::parse(&source).unwrap();
        for p in [2500, usize::MAX] {
            for e in [
                scalar_error(&mut image, false, |i| {
                    i.physical_repeater_record_raw(p).map(|_| ())
                }),
                scalar_error(&mut image, false, |i| {
                    i.physical_repeater_use(p).map(|_| ())
                }),
                scalar_error(&mut image, false, |i| i.set_physical_repeater_use(p, false)),
            ] {
                assert!(matches!(e, Error::Unsupported { line: 5, .. }));
            }
        }
        for g in [50, usize::MAX] {
            let e = scalar_error(&mut image, false, |i| {
                i.set_repeater_group_name_ascii(g, "\0")
            });
            assert!(matches!(e, Error::Unsupported { line: 5, .. }));
        }
        let long = "X".repeat(17);
        for text in [long.as_str(), "bad\0", "\n", "\x7f", "é"] {
            let e = scalar_error(&mut image, true, |i| {
                i.set_repeater_group_name_ascii(49, text)
            });
            assert!(matches!(e,Error::Invalid {line,..} if line==5+(0x2ea3b+16*49)/32));
        }
        for (at, bytes, error_at) in [
            (0x3ab82, [0xfe, 0xff], 0x3ab82),
            (0x3ab82, [0xc4, 9], 0x3ab82), // LE2500
            (0x3ab84, [2, 0], 0x3ab84),
            (0x3ab84, [255, 255], 0x3ab86), // A hole before a later live row.
            (0x3ab82 + 40, [3, 0], 0x3ab82 + 40),
        ] {
            image.memory.copy_from_slice(&baseline);
            image.memory[at..at + 2].copy_from_slice(&bytes);
            for e in [
                scalar_error(&mut image, false, |i| i.repeater_groups().map(|_| ())),
                scalar_error(&mut image, false, |i| {
                    i.physical_repeater_record_raw(0).map(|_| ())
                }),
                scalar_error(&mut image, false, |i| {
                    i.physical_repeater_use(0).map(|_| ())
                }),
                scalar_error(&mut image, false, |i| i.set_physical_repeater_use(0, true)),
            ] {
                assert!(matches!(e,Error::Unsupported {line,..} if line==5+error_at/32));
            }
        }
        for (changes, error_at) in [
            (vec![(0x3bf0a, [1, 0])], 0x3bf0a),
            (vec![(0x3bf0a + 98, [0, 0])], 0x3bf0a + 98),
            (vec![(0x3bf0a + 98, [3, 0])], 0x3bf0a + 98),
            (vec![(0x3bf0a + 98, [0xc4, 9])], 0x3bf0a + 98),
            (
                vec![(0x3bf0a + 2, [2, 0]), (0x3bf0a + 98, [1, 0])],
                0x3bf0a + 98,
            ),
            (vec![(0x3bf0a, [255, 255])], 0x3bf0a + 98),
        ] {
            image.memory.copy_from_slice(&baseline);
            for (at, bytes) in changes {
                image.memory[at..at + 2].copy_from_slice(&bytes);
            }
            let e = scalar_error(&mut image, false, |i| i.repeater_groups().map(|_| ()));
            assert!(matches!(e,Error::Unsupported {line,..} if line==5+error_at/32));
            // Physical record/USE views intentionally do not validate group starts.
            assert_eq!(
                image.physical_repeater_record_raw(0).unwrap(),
                Some([255; 52].as_slice())
            );
            let before = image.memory().to_vec();
            image.set_physical_repeater_use(0, true).unwrap();
            let mut expected = before;
            expected[0x3bf6e] |= 1;
            assert_setting_state(&image, &source, &expected);
        }
        image.memory.copy_from_slice(&baseline);
        image.memory[0x3bf0a..0x3bf6e].fill(255);
        scalar_error(&mut image, false, |i| i.repeater_groups().map(|_| ())); // N>0, no start zero.
        image.memory[0x3ab82..0x3bf0a].fill(255);
        image.memory[0x3bf0a..0x3bf0c].fill(0);
        scalar_error(&mut image, false, |i| i.repeater_groups().map(|_| ())); // N0 requires all FFFF.
        image.memory[0x3bf0a..0x3bf6e].fill(255);
        let mut expected = image.memory().to_vec();
        let groups = image.repeater_groups().unwrap();
        assert_eq!(groups.len(), 50);
        assert!(groups.iter().all(|g| g.physical_slots().is_empty()));
        assert_eq!(image.physical_repeater_record_raw(0).unwrap(), None);
        assert_eq!(image.physical_repeater_use(2499).unwrap(), None);
        scalar_error(&mut image, false, |i| i.set_physical_repeater_use(0, false));
        image.set_repeater_group_name_ascii(49, "").unwrap();
        expected[0x2ea3b + 16 * 49..0x2ea3b + 16 * 50].fill(b' ');
        assert_setting_state(&image, &source, &expected);
        let output = image.to_bytes();
        let reparsed = Icf::parse(&output).unwrap();
        assert_setting_state(&reparsed, &output, &expected);
        assert!(
            reparsed
                .repeater_groups()
                .unwrap()
                .iter()
                .all(|g| g.physical_slots().is_empty())
        );
        assert_eq!(reparsed.to_bytes(), output);
    }

    fn typed_repeater_fixture() -> (Vec<u8>, Vec<u8>) {
        let mut memory: Vec<u8> = (0..IMAGE_BYTES).map(|i| (91 * i + 7) as u8).collect();
        memory[IMAGE_BYTES - 16..].copy_from_slice(b"IcomCloneFormat3");
        memory[INTERNAL_MAP_REV] = 6;
        memory[0x3bf0a..0x3bf6e].fill(255);
        memory[0x3bf0a..0x3bf0c].fill(0);
        memory[0x3bf6e..0x3bf6e + 313].fill(0xa5);
        for p in 0..2500 {
            let r = 0xaa0f + 52 * p;
            let k = (p + 17) % 2500;
            memory[0x3ab82 + 2 * p..0x3ab84 + 2 * p]
                .copy_from_slice(&(((p + 37) % 2500) as u16).to_le_bytes());
            memory[r..r + 4].copy_from_slice(&(144_000_000 + p as u32).to_le_bytes());
            memory[r + 4..r + 8].copy_from_slice(&600_000_u32.to_le_bytes());
            memory[r + 8..r + 10].copy_from_slice(&(k as u16).to_le_bytes());
            memory[r + 10] = 0x5a;
            memory[r + 11..r + 18].copy_from_slice(&[0x83, 0x0a, 0x1c, 0x48, 0xb1, 0xa3, 0xc8]);
            memory[r + 18..r + 42].copy_from_slice(b"ABCDEFGHIJKLMNOPQRSTUVWX");
            memory[r + 42..r + 52]
                .copy_from_slice(&[35, 0xea, 0x5f, 139, 0x30, 0x39, 49, 0x1c, 2, p as u8]);
            let a = 0x2a5df + 7 * k;
            memory[a..a + 7].copy_from_slice(&[0xa9, 0x16, 0x9d, 0x46, 0x0c, 0x50, 0x47]);
        }
        // Independent Python struct/hashlib, pattern (91*i+7)%256; rotated
        // order p=j+37 and gateway k=p+17, not identity maps. No setter oracle.
        let source = fixture_with_memory(&memory, "3D2470724F92F088E0CF43FCBF288C7D");
        (source, memory)
    }

    fn typed_repeater_value() -> RepeaterRecord {
        RepeaterRecord {
            name: *b"ZYXWVUTSRQPONMLK",
            sub_name: *b"jihgfedc",
            repeater_call: *b"TEST01 B",
            gateway_call: *b"ABCDEF G",
            frequency_hz: 30_000,
            offset_hz: 0,
            mode: RepeaterMode::Dv,
            duplex: RepeaterDuplex::Plus,
            tone: RepeaterTone::Tone,
            tone_index: None,
            use_repeater: true,
            position: RepeaterPosition::Approximate,
            latitude: RepeaterCoordinate {
                degrees: 89,
                thousandths_of_minute: 59_999,
                negative: true,
            },
            longitude: RepeaterCoordinate {
                degrees: 180,
                thousandths_of_minute: 0,
                negative: false,
            },
            utc_offset_minutes: -840,
        }
    }

    #[test]
    fn typed_repeater_all_2500_fields_match_independent_windows_and_digests() {
        let (source, mut expected) = typed_repeater_fixture();
        assert_eq!(
            format!("{:X}", md5::compute(&source)),
            "D3BD90DDD8254630913D8FEEF38F5C7F"
        );
        let mut image = Icf::parse(&source).unwrap();
        assert_eq!(image.repeater_groups().unwrap()[0].physical_slots()[0], 37);
        for p in 0..2500 {
            let row = image.physical_repeater(p).unwrap().unwrap();
            assert_eq!(
                row,
                RepeaterRecord {
                    name: *b"ABCDEFGHIJKLMNOP",
                    sub_name: *b"QRSTUVWX",
                    repeater_call: *b"ABCDEFGH",
                    gateway_call: *b"TEST01 G",
                    frequency_hz: 144_000_000 + p as u32,
                    offset_hz: 600_000,
                    mode: RepeaterMode::Fm,
                    duplex: RepeaterDuplex::Minus,
                    tone: RepeaterTone::ToneSquelch,
                    tone_index: Some(49),
                    use_repeater: 0xa5 & (1 << (p & 7)) != 0,
                    position: RepeaterPosition::Exact,
                    latitude: RepeaterCoordinate {
                        degrees: 35,
                        thousandths_of_minute: 59_999,
                        negative: p & 128 != 0
                    },
                    longitude: RepeaterCoordinate {
                        degrees: 139,
                        thousandths_of_minute: 12_345,
                        negative: p & 64 != 0
                    },
                    utc_offset_minutes: 540,
                }
            );
            let mut value = typed_repeater_value();
            value.frequency_hz += p as u32;
            value.offset_hz = p as u32 * 100;
            value.use_repeater = p % 2 == 0;
            image.set_physical_repeater(p, &value).unwrap();
            assert_eq!(image.physical_repeater(p).unwrap(), Some(value));
            let r = 0xaa0f + 52 * p;
            expected[r..r + 4].copy_from_slice(&(30_000 + p as u32).to_le_bytes());
            expected[r + 4..r + 8].copy_from_slice(&(p as u32 * 100).to_le_bytes());
            expected[r + 10..r + 18]
                .copy_from_slice(&[0x25, 0xa9, 0x16, 0x9d, 0x46, 0x0c, 0x50, 0x42]);
            expected[r + 18..r + 42].copy_from_slice(b"ZYXWVUTSRQPONMLKjihgfedc");
            expected[r + 42..r + 48].copy_from_slice(&[89, 0xea, 0x5f, 180, 0, 0]);
            // DV keeps the old FM tone byte (49); sign low six bits survive.
            expected[r + 49..r + 52].copy_from_slice(&[0xb8, 0xfc, (p as u8 & 63) | 128]);
            let a = 0x2a5df + 7 * ((p + 17) % 2500);
            expected[a..a + 7].copy_from_slice(&[0x83, 0x0a, 0x1c, 0x48, 0xb1, 0x90, 0x47]);
            let u = 0x3bf6e + p / 8;
            let mask = 1 << (p & 7);
            expected[u] = expected[u] & !mask | if p % 2 == 0 { mask } else { 0 };
        }
        assert_setting_state(&image, &source, &expected);
        let output = image.to_bytes();
        assert_eq!(
            output,
            fixture_with_memory(&expected, "E385553EB7A7E2C3A14CC063E564B557")
        );
        assert_eq!(
            format!("{:X}", md5::compute(&output)),
            "B5B66A66A9479BBDBC7B134898C657FE"
        );
        let reparsed = Icf::parse(&output).unwrap();
        assert_setting_state(&reparsed, &output, &expected);
        assert_eq!(reparsed.to_bytes(), output);
    }

    #[test]
    fn typed_repeater_gateway_aliases_and_errors_are_atomic() {
        let (source, baseline) = typed_repeater_fixture();
        let mut image = Icf::parse(&source).unwrap();
        let p = 2499;
        let r = 0xaa0f + 52 * p;
        let other = 0xaa0f + 52 * 2;
        // An unused physical row still aliases k=16; logical j=2462 maps p2499.
        image.memory[0x3ab82..0x3bf0a].fill(255);
        image.memory[0x3ab82..0x3ab84].copy_from_slice(&2499_u16.to_le_bytes());
        image.memory[other + 8..other + 10].copy_from_slice(&16_u16.to_le_bytes());
        assert_eq!(image.physical_repeater(2).unwrap(), None);
        let mut value = image.physical_repeater(p).unwrap().unwrap();
        let before = image.memory.clone();
        image.set_physical_repeater(p, &value).unwrap();
        assert_setting_state(&image, &source, &before);
        value.gateway_call = *b"ABCDEF G";
        scalar_error(&mut image, false, |i| i.set_physical_repeater(p, &value));
        // Restore full occupancy: an occupied alias must also reject changes.
        image.memory[0x3ab82..0x3bf0a].copy_from_slice(&baseline[0x3ab82..0x3bf0a]);
        scalar_error(&mut image, false, |i| i.set_physical_repeater(p, &value));
        value.gateway_call = [b' '; 8];
        let mut expected = image.memory.clone();
        expected[r + 8..r + 10].fill(255);
        image.set_physical_repeater(p, &value).unwrap();
        assert_setting_state(&image, &source, &expected);
        image.set_physical_repeater(p, &value).unwrap();
        assert_setting_state(&image, &source, &expected);
        value.gateway_call = *b"ABCDEF G";
        scalar_error(&mut image, false, |i| i.set_physical_repeater(p, &value));
        // Invalid current links cannot be hidden by a new blank gateway.
        for k in [2500_u16, 65534] {
            image.memory[r + 8..r + 10].copy_from_slice(&k.to_le_bytes());
            assert!(matches!(
                image.physical_repeater(p),
                Err(Error::Unsupported { .. })
            ));
            value.gateway_call = [b' '; 8];
            scalar_error(&mut image, false, |i| i.set_physical_repeater(p, &value));
        }
        image.memory.copy_from_slice(&baseline);
        // Packed blank at a real k is not automatically unlinked.
        let a = 0x2a5df + 7 * 16;
        image.memory[a..a + 7].copy_from_slice(&[0x40, 0x81, 2, 4, 8, 0x10, 0x20]);
        value = image.physical_repeater(p).unwrap().unwrap();
        let before = image.memory.clone();
        image.set_physical_repeater(p, &value).unwrap();
        assert_setting_state(&image, &source, &before);
        // Later invalid fields must not partially write a unique aux or unlink.
        for gateway_call in [*b"ABCDEF G", [b' '; 8]] {
            let mut bad = typed_repeater_value();
            bad.gateway_call = gateway_call;
            bad.utc_offset_minutes = 7;
            scalar_error(&mut image, true, |i| i.set_physical_repeater(p, &bad));
        }
        for slot in [2500, usize::MAX] {
            assert!(matches!(
                image.physical_repeater(slot),
                Err(Error::Unsupported { line: 5, .. })
            ));
            let error = scalar_error(&mut image, false, |i| i.set_physical_repeater(slot, &value));
            assert!(matches!(error, Error::Unsupported { line: 5, .. }));
        }
        image.memory[0x3ab82..0x3bf0a].fill(255);
        assert_eq!(image.physical_repeater(p).unwrap(), None);
        scalar_error(&mut image, false, |i| i.set_physical_repeater(p, &value));
        image.memory[0x3ab84..0x3ab86].copy_from_slice(&2499_u16.to_le_bytes());
        assert!(image.physical_repeater(p).is_err());
        scalar_error(&mut image, false, |i| i.set_physical_repeater(p, &value));
    }

    #[test]
    fn typed_repeater_raw_domains_canonical_limits_and_shared_seven_bit_codec() {
        let (source, baseline) = typed_repeater_fixture();
        let mut image = Icf::parse(&source).unwrap();
        let r = 0xaa0f;
        let good = typed_repeater_value();
        // Raw views retain all integer/text widths and all stored mode/duplex codes.
        image.memory[r..r + 8].fill(255);
        image.memory[r + 11..r + 48].fill(255);
        image.memory[0x2a5df + 7 * 17..0x2a5df + 7 * 18].fill(255);
        image.memory[r + 48..r + 52].copy_from_slice(&[255, 255, 127, 255]);
        for (code, mode) in [
            RepeaterMode::Dv,
            RepeaterMode::Fm,
            RepeaterMode::FmNarrow,
            RepeaterMode::Dd,
        ]
        .into_iter()
        .enumerate()
        {
            for (dup, duplex) in [
                RepeaterDuplex::Off,
                RepeaterDuplex::Minus,
                RepeaterDuplex::Plus,
                RepeaterDuplex::Rps,
            ]
            .into_iter()
            .enumerate()
            {
                image.memory[r + 10] = ((code as u8) << 6) | ((dup as u8) << 4);
                let raw = image.physical_repeater(0).unwrap().unwrap();
                assert_eq!((raw.mode, raw.duplex), (mode, duplex));
                assert_eq!((raw.frequency_hz, raw.offset_hz), (u32::MAX, u32::MAX));
                assert_eq!(raw.name, [255; 16]);
                assert_eq!(raw.sub_name, [255; 8]);
                assert_eq!(raw.repeater_call, [127; 8]);
                assert_eq!(raw.gateway_call, [127; 8]);
                assert_eq!(
                    raw.latitude,
                    RepeaterCoordinate {
                        degrees: 255,
                        thousandths_of_minute: 65535,
                        negative: true
                    }
                );
                assert_eq!(raw.longitude, raw.latitude);
                assert_eq!(raw.utc_offset_minutes, 32767);
                assert_eq!(
                    raw.tone_index,
                    if code == 1 || code == 2 {
                        Some(255)
                    } else {
                        None
                    }
                );
            }
        }
        for flags in [0x4c, 0x43] {
            image.memory[r + 10] = flags;
            assert!(matches!(
                image.physical_repeater(0),
                Err(Error::Unsupported { .. })
            ));
        }
        // Replacing all fields does not require decoding obsolete raw fields.
        image.set_physical_repeater(0, &good).unwrap();
        assert_eq!(image.physical_repeater(0).unwrap(), Some(good.clone()));
        assert_eq!(image.memory[r + 48], 255);
        image.memory.copy_from_slice(&baseline);
        for frequency_hz in [30_000, 1_399_999_999] {
            for (offset_hz, utc_offset_minutes, tone_index) in [(0, -840, 0), (9_999_900, 840, 49)]
            {
                let mut v = good.clone();
                v.frequency_hz = frequency_hz;
                v.offset_hz = offset_hz;
                v.utc_offset_minutes = utc_offset_minutes;
                v.mode = RepeaterMode::Fm;
                v.tone_index = Some(tone_index);
                v.tone = RepeaterTone::Off;
                v.position = RepeaterPosition::None;
                v.latitude = RepeaterCoordinate {
                    degrees: 0,
                    thousandths_of_minute: 0,
                    negative: true,
                };
                v.longitude = v.latitude;
                image.set_physical_repeater(0, &v).unwrap();
                assert_eq!(image.physical_repeater(0).unwrap(), Some(v));
                assert_eq!(image.memory[r + 51], 0xc0);
            }
        }
        let mut pole = good.clone();
        pole.name[0] = b' ';
        pole.name[15] = b'~';
        pole.sub_name[0] = b' ';
        pole.sub_name[7] = b'~';
        pole.repeater_call = *b"/0123 AZ";
        pole.gateway_call = *b"/0123 A ";
        pole.latitude.degrees = 90;
        pole.latitude.thousandths_of_minute = 0;
        pole.longitude.degrees = 179;
        pole.longitude.thousandths_of_minute = 59_999;
        image.set_physical_repeater(0, &pole).unwrap();
        assert_eq!(image.physical_repeater(0).unwrap(), Some(pole));
        for mode in [RepeaterMode::FmNarrow, RepeaterMode::Dd] {
            let mut v = good.clone();
            v.mode = mode;
            scalar_error(&mut image, false, |i| i.set_physical_repeater(0, &v));
        }
        let mut v = good.clone();
        v.duplex = RepeaterDuplex::Rps;
        scalar_error(&mut image, false, |i| i.set_physical_repeater(0, &v));
        let mut invalid_values = Vec::new();
        for n in [0, 29_999, 1_400_000_000, u32::MAX] {
            let mut v = good.clone();
            v.frequency_hz = n;
            invalid_values.push(v);
        }
        for n in [1, 9_999_901, 10_000_000, u32::MAX] {
            let mut v = good.clone();
            v.offset_hz = n;
            invalid_values.push(v);
        }
        for n in [-841, 841, 7, 32767] {
            let mut v = good.clone();
            v.utc_offset_minutes = n;
            invalid_values.push(v);
        }
        for n in [0, 31, 127, 128, 255] {
            let mut v = good.clone();
            v.name[15] = n;
            invalid_values.push(v);
            let mut v = good.clone();
            v.sub_name[7] = n;
            invalid_values.push(v);
        }
        for n in [b'a', b'@', 0, 127, 128, 255] {
            let mut v = good.clone();
            v.repeater_call[7] = n;
            invalid_values.push(v);
            let mut v = good.clone();
            v.gateway_call[0] = n;
            invalid_values.push(v);
        }
        let mut v = good.clone();
        v.gateway_call[7] = b'B';
        invalid_values.push(v);
        for (mode, tone_index) in [
            (RepeaterMode::Fm, None),
            (RepeaterMode::Fm, Some(50)),
            (RepeaterMode::Fm, Some(255)),
            (RepeaterMode::Dv, Some(0)),
        ] {
            let mut v = good.clone();
            v.mode = mode;
            v.tone_index = tone_index;
            invalid_values.push(v);
        }
        for (lat, degrees, minutes) in [
            (true, 91, 0),
            (true, 90, 1),
            (true, 0, 60_000),
            (false, 181, 0),
            (false, 180, 1),
            (false, 0, 60_000),
        ] {
            let mut v = good.clone();
            v.position = RepeaterPosition::None;
            let c = if lat {
                &mut v.latitude
            } else {
                &mut v.longitude
            };
            c.degrees = degrees;
            c.thousandths_of_minute = minutes;
            invalid_values.push(v);
        }
        let mut v = good.clone();
        v.mode = RepeaterMode::Fm;
        v.tone_index = Some(0);
        v.duplex = RepeaterDuplex::Off;
        invalid_values.push(v.clone());
        v.repeater_call = [b' '; 8];
        invalid_values.push(v);
        let mut v = good.clone();
        v.repeater_call = [b' '; 8];
        invalid_values.push(v.clone());
        v.gateway_call = [b' '; 8];
        invalid_values.push(v);
        for v in invalid_values {
            scalar_error(&mut image, true, |i| i.set_physical_repeater(0, &v));
        }
        let mut bad_sub = good.clone();
        bad_sub.sub_name[7] = 0;
        let error = scalar_error(&mut image, true, |i| i.set_physical_repeater(0, &bad_sub));
        assert!(matches!(error, Error::Invalid { line, .. } if line == 5 + (r + 34) / 32));
        for mode in [RepeaterMode::Fm, RepeaterMode::Dv] {
            let mut v = good.clone();
            v.mode = mode;
            v.tone_index = if mode == RepeaterMode::Fm {
                Some(0)
            } else {
                None
            };
            v.duplex = RepeaterDuplex::Off;
            v.repeater_call = [b' '; 8];
            v.gateway_call = [b' '; 8];
            image.set_physical_repeater(0, &v).unwrap();
            assert_eq!(image.physical_repeater(0).unwrap(), Some(v));
        }
        // Unknown tone indices 50/FF are getter values, not setter defaults.
        for index in [50, 255] {
            image.memory[r + 10] = 0x40;
            image.memory[r + 48] = index;
            assert_eq!(
                image.physical_repeater(0).unwrap().unwrap().tone_index,
                Some(index)
            );
        }
        use super::memory::{pack_callsign, unpack_callsign};
        for (call, packed) in [
            (*b"ABCDEFGH", [0x83, 0x0a, 0x1c, 0x48, 0xb1, 0xa3, 0xc8]),
            (*b"TEST01 B", [0xa9, 0x16, 0x9d, 0x46, 0x0c, 0x50, 0x42]),
            ([0; 8], [0; 7]),
            ([127; 8], [255; 7]),
        ] {
            assert_eq!(pack_callsign(&call), packed);
            assert_eq!(unpack_callsign(&packed), call);
        }
        for bit in 0..56 {
            let mut packed = [0; 7];
            packed[bit / 8] = 1 << (7 - bit % 8);
            let mut call = [0; 8];
            call[bit / 7] = 1 << (6 - bit % 7);
            assert_eq!(pack_callsign(&call), packed);
            assert_eq!(unpack_callsign(&packed), call);
        }
    }

    const DSP_FILTER_MODES: [FilterMode; 5] = [
        FilterMode::Ssb,
        FilterMode::SsbData,
        FilterMode::Cw,
        FilterMode::Rtty,
        FilterMode::Am,
    ];
    const DSP_FILTER_SLOTS: [FilterSlot; 3] = [FilterSlot::One, FilterSlot::Two, FilterSlot::Three];
    const DSP_STEP_MODES: [StepAgcMode; 7] = [
        StepAgcMode::Ssb,
        StepAgcMode::Cw,
        StepAgcMode::Rtty,
        StepAgcMode::Am,
        StepAgcMode::Fm,
        StepAgcMode::Wfm,
        StepAgcMode::Dv,
    ];
    const DSP_TONE_MODES: [ToneMode; 7] = [
        ToneMode::Ssb,
        ToneMode::Am,
        ToneMode::Fm,
        ToneMode::Wfm,
        ToneMode::Dv,
        ToneMode::Cw,
        ToneMode::Rtty,
    ];
    const DSP_AGC: [Agc; 3] = [Agc::Fast, Agc::Mid, Agc::Slow];
    const DSP_PRESETS: [TransmitBandwidthPreset; 4] = [
        TransmitBandwidthPreset::Wide,
        TransmitBandwidthPreset::Mid,
        TransmitBandwidthPreset::Narrow,
        TransmitBandwidthPreset::Data,
    ];
    const DSP_LOW: [TransmitLow; 4] = [
        TransmitLow::Hz100,
        TransmitLow::Hz200,
        TransmitLow::Hz300,
        TransmitLow::Hz500,
    ];
    const DSP_HIGH: [TransmitHigh; 4] = [
        TransmitHigh::Hz2500,
        TransmitHigh::Hz2700,
        TransmitHigh::Hz2800,
        TransmitHigh::Hz2900,
    ];

    fn dsp_fixture() -> (Vec<u8>, Vec<u8>) {
        let mut memory: Vec<u8> = (0..IMAGE_BYTES).map(|i| (113 * i + 29) as u8).collect();
        memory[IMAGE_BYTES - 16..].copy_from_slice(b"IcomCloneFormat3");
        memory[INTERNAL_MAP_REV] = 6;
        memory[0x37850..0x378c0].fill(0xfb);
        memory[0x392c0..0x392f2].fill(0xfb);
        memory[0x378af..0x378c0].fill(0xa7);
        // Independent Python struct/hashlib, pattern (113*i+29)%256, raw FB
        // RF/DSP windows and A7 band bytes. Not generated by a product setter.
        let source = fixture_with_memory(&memory, "28EDC313B60F9461FF80B9331733A64E");
        (source, memory)
    }

    fn dsp_change(
        image: &mut Icf<'_>,
        expected: &mut [u8],
        at: usize,
        raw: u8,
        operation: impl FnOnce(&mut Icf<'_>) -> Result<(), Error>,
    ) {
        operation(image).unwrap();
        expected[at] = raw;
        assert_setting_state(image, image.source, expected);
    }

    fn dsp_roundtrip(image: &Icf<'_>, expected: &[u8], cd: &str, file_md5: &str) {
        let output = image.to_bytes();
        assert_eq!(output, fixture_with_memory(expected, cd));
        assert_eq!(format!("{:X}", md5::compute(&output)), file_md5);
        let reparsed = Icf::parse(&output).unwrap();
        assert_setting_state(&reparsed, &output, expected);
        assert_eq!(reparsed.to_bytes(), output);
    }

    #[test]
    fn dsp_all_contexts_mix_single_byte_edits_with_independent_hashes() {
        let (source, mut expected) = dsp_fixture();
        assert_eq!(
            format!("{:X}", md5::compute(&source)),
            "3397D151A260AB3C1035EED17EF12BBF"
        );
        let mut image = Icf::parse(&source).unwrap();
        for (m, mode) in DSP_FILTER_MODES.into_iter().enumerate() {
            let widths = if m == 4 {
                [200, 600, 10000]
            } else if m == 3 {
                [500, 600, 2700]
            } else {
                [500, 600, 3600]
            };
            let codes = if m == 4 {
                [0, 2, 49]
            } else if m == 3 {
                [9, 10, 31]
            } else {
                [9, 10, 40]
            };
            // Independent literal PBT endpoints, including CW's asymmetric range.
            let endpoints = if m == 4 {
                [(0, 0), (-200, 200), (-5000, 5000)]
            } else if m == 2 {
                [(-250, 250), (-300, 300), (-3300, 300)]
            } else if m == 3 {
                [(-250, 250), (-300, 300), (-1350, 1350)]
            } else {
                [(-250, 250), (-300, 300), (-1800, 1800)]
            };
            for (f, slot) in DSP_FILTER_SLOTS.into_iter().enumerate() {
                dsp_change(
                    &mut image,
                    &mut expected,
                    0x37850 + 3 * m + f,
                    codes[f],
                    |i| i.set_filter_bandwidth_hz(mode, slot, widths[f]),
                );
                assert_eq!(image.filter_bandwidth_hz(mode, slot).unwrap(), widths[f]);
                for (s, side) in [PbtSide::One, PbtSide::Two].into_iter().enumerate() {
                    let hz = if s == 0 {
                        endpoints[f].0
                    } else {
                        endpoints[f].1
                    };
                    let step = if m == 4 { 200 } else { 50 };
                    dsp_change(
                        &mut image,
                        &mut expected,
                        0x37867 + 6 * m + 2 * f + s,
                        (hz / step) as i8 as u8,
                        |i| i.set_filter_pbt_hz(mode, slot, side, hz),
                    );
                    assert_eq!(image.filter_pbt_hz(mode, slot, side).unwrap(), hz);
                }
                assert_eq!(
                    image.filter_passband_hz(mode, slot).unwrap(),
                    widths[f] as i32 - (endpoints[f].0 - endpoints[f].1).abs()
                );
                assert_eq!(
                    image.filter_shift_hz(mode, slot).unwrap(),
                    (endpoints[f].0 + endpoints[f].1) / 2
                );
            }
        }
        for (b, band) in [FilterBandGroup::Hf, FilterBandGroup::Mhz50To430]
            .into_iter()
            .enumerate()
        {
            for (s, slot) in [
                FilterShapeSlot::Ssb,
                FilterShapeSlot::SsbData,
                FilterShapeSlot::CwNarrow,
                FilterShapeSlot::CwWide,
            ]
            .into_iter()
            .enumerate()
            {
                let shape = if (b + s) % 2 == 0 {
                    FilterShape::Sharp
                } else {
                    FilterShape::Soft
                };
                dsp_change(
                    &mut image,
                    &mut expected,
                    0x3785f + 4 * b + s,
                    ((b + s) % 2) as u8,
                    |i| i.set_filter_shape(band, slot, shape),
                );
                assert_eq!(image.filter_shape(band, slot).unwrap(), shape);
            }
        }
        dsp_roundtrip(
            &image,
            &expected,
            "12D529ACEDEAD15C701252F817FB7895",
            "40141CD08335937BFCE6F2595D697D23",
        );
        for b in 0..17 {
            let code = b % if b <= 12 { 3 } else { 2 };
            let preamp = [Preamp::Off, Preamp::Amp1, Preamp::Amp2][code];
            let raw = (expected[0x378af + b] & !3) | code as u8;
            dsp_change(&mut image, &mut expected, 0x378af + b, raw, |i| {
                i.set_preamp(b, preamp)
            });
            assert_eq!(image.preamp(b).unwrap(), preamp);
            if b <= 12 {
                let raw = (expected[0x378af + b] & !4) | if b % 2 != 0 { 4 } else { 0 };
                dsp_change(&mut image, &mut expected, 0x378af + b, raw, |i| {
                    i.set_attenuator(b, b % 2 != 0)
                });
                assert_eq!(image.attenuator(b).unwrap(), b % 2 != 0);
            }
        }
        for (g, mode) in DSP_STEP_MODES.into_iter().enumerate() {
            dsp_change(&mut image, &mut expected, 0x37885 + g, (g + 5) as u8, |i| {
                i.set_tuning_step_code(mode, (g + 5) as u8)
            });
            assert_eq!(image.tuning_step_code(mode).unwrap(), (g + 5) as u8);
            if g < 4 {
                dsp_change(&mut image, &mut expected, 0x378a7 + g, (g % 3) as u8, |i| {
                    i.set_agc(mode, DSP_AGC[g % 3])
                });
                assert_eq!(image.agc(mode).unwrap(), DSP_AGC[g % 3]);
                for (a, state) in DSP_AGC.into_iter().enumerate() {
                    let code = (3 * g + a) as u8;
                    dsp_change(&mut image, &mut expected, 0x3788f + 3 * g + a, code, |i| {
                        i.set_agc_delay_code(mode, state, code)
                    });
                    assert_eq!(image.agc_delay_code(mode, state).unwrap(), code);
                }
            }
        }
        dsp_change(&mut image, &mut expected, 0x3788d, 12, |i| {
            i.set_air_tuning_step_code(12)
        });
        assert_eq!(image.air_tuning_step_code().unwrap(), 12);
        dsp_roundtrip(
            &image,
            &expected,
            "C4AAE524779A8688B7A58CF14A03BCA0",
            "1A81ECD061AB5C9816C6D5264BA50F77",
        );
        for (t, mode) in DSP_TONE_MODES.into_iter().enumerate() {
            if t != 3 {
                dsp_change(&mut image, &mut expected, 0x392da + t, (3 * t) as u8, |i| {
                    i.set_receive_high_pass_code(mode, (3 * t) as u8)
                });
                dsp_change(
                    &mut image,
                    &mut expected,
                    0x392e2 + t,
                    (25 - t) as u8,
                    |i| i.set_receive_low_pass_code(mode, (25 - t) as u8),
                );
                assert_eq!(image.receive_high_pass_code(mode).unwrap(), (3 * t) as u8);
                assert_eq!(image.receive_low_pass_code(mode).unwrap(), (25 - t) as u8);
            }
            if t <= 4 {
                let level = 2 * t as i8 - 5;
                dsp_change(&mut image, &mut expected, 0x392ca + t, (2 * t) as u8, |i| {
                    i.set_receive_bass(mode, level)
                });
                dsp_change(&mut image, &mut expected, 0x392d2 + t, (2 * t) as u8, |i| {
                    i.set_receive_treble(mode, level)
                });
                assert_eq!(image.receive_bass(mode).unwrap(), level);
                assert_eq!(image.receive_treble(mode).unwrap(), level);
                if t != 3 {
                    let level = 5 - 2 * t as i8;
                    dsp_change(
                        &mut image,
                        &mut expected,
                        0x392c0 + t,
                        (10 - 2 * t) as u8,
                        |i| i.set_transmit_bass(mode, level),
                    );
                    dsp_change(
                        &mut image,
                        &mut expected,
                        0x392c5 + t,
                        (10 - 2 * t) as u8,
                        |i| i.set_transmit_treble(mode, level),
                    );
                    assert_eq!(image.transmit_bass(mode).unwrap(), level);
                    assert_eq!(image.transmit_treble(mode).unwrap(), level);
                }
            }
        }
        for (p, preset) in DSP_PRESETS.into_iter().enumerate() {
            dsp_change(&mut image, &mut expected, 0x392ea + 2 * p, p as u8, |i| {
                i.set_transmit_bandwidth_low(preset, DSP_LOW[p])
            });
            dsp_change(
                &mut image,
                &mut expected,
                0x392eb + 2 * p,
                (3 - p) as u8,
                |i| i.set_transmit_bandwidth_high(preset, DSP_HIGH[3 - p]),
            );
            assert_eq!(image.transmit_bandwidth_low(preset).unwrap(), DSP_LOW[p]);
            assert_eq!(
                image.transmit_bandwidth_high(preset).unwrap(),
                DSP_HIGH[3 - p]
            );
        }
        dsp_roundtrip(
            &image,
            &expected,
            "87CB20CD16DE4185FFE61C7A1572C01F",
            "03F78A1D4952C143C46656CFBA278F90",
        );
        assert_setting_state(&image, &source, &expected);
    }

    #[test]
    fn dsp_filter_grids_raw_signed_views_bounds_and_full_byte_shapes() {
        let (source, mut expected) = dsp_fixture();
        let mut image = Icf::parse(&source).unwrap();
        for (m, mode) in DSP_FILTER_MODES.into_iter().enumerate() {
            let max = if m == 4 {
                49
            } else if m == 3 {
                31
            } else {
                40
            };
            for (f, slot) in DSP_FILTER_SLOTS.into_iter().enumerate() {
                let at = 0x37850 + 3 * m + f;
                scalar_error(&mut image, false, |i| {
                    i.filter_bandwidth_hz(mode, slot).map(|_| ())
                });
                scalar_error(&mut image, false, |i| {
                    i.set_filter_pbt_hz(mode, slot, PbtSide::One, 0)
                });
                scalar_error(&mut image, false, |i| {
                    i.filter_passband_hz(mode, slot).map(|_| ())
                });
                for c in 0..=max {
                    let hz = if m == 4 {
                        200 * (c + 1)
                    } else if c < 10 {
                        50 * (c + 1)
                    } else {
                        500 + 100 * (c - 9)
                    };
                    dsp_change(&mut image, &mut expected, at, c as u8, |i| {
                        i.set_filter_bandwidth_hz(mode, slot, hz)
                    });
                    assert_eq!(image.filter_bandwidth_hz(mode, slot).unwrap(), hz);
                }
                for hz in [
                    0,
                    49,
                    51,
                    550,
                    if m == 4 {
                        10001
                    } else if m == 3 {
                        2800
                    } else {
                        3700
                    },
                    u32::MAX,
                ] {
                    scalar_error(&mut image, true, |i| {
                        i.set_filter_bandwidth_hz(mode, slot, hz)
                    });
                }
                for raw in [(max + 1) as u8, 255] {
                    image.memory[at] = raw;
                    expected[at] = raw;
                    let error = scalar_error(&mut image, false, |i| {
                        i.filter_bandwidth_hz(mode, slot).map(|_| ())
                    });
                    assert!(
                        matches!(error, Error::Unsupported { line, .. } if line == 5 + at / 32)
                    );
                    scalar_error(&mut image, false, |i| {
                        i.set_filter_pbt_hz(mode, slot, PbtSide::Two, 0)
                    });
                }
                // Every i8 remains readable even with unknown BW; no GUI clipping.
                image.memory[at] = 255;
                expected[at] = 255;
                for raw in 0..=255_u8 {
                    let pbt = 0x37867 + 6 * m + 2 * f;
                    image.memory[pbt] = raw;
                    expected[pbt] = raw;
                    assert_eq!(
                        image.filter_pbt_hz(mode, slot, PbtSide::One).unwrap(),
                        i32::from(raw as i8) * if m == 4 { 200 } else { 50 }
                    );
                }
            }
        }
        let mode = FilterMode::Ssb;
        let slot = FilterSlot::One;
        dsp_change(&mut image, &mut expected, 0x37850, 0, |i| {
            i.set_filter_bandwidth_hz(mode, slot, 50)
        });
        image.memory[0x37867..0x37869].copy_from_slice(&[0x80, 0x7f]);
        expected[0x37867..0x37869].copy_from_slice(&[0x80, 0x7f]);
        assert_eq!(image.filter_passband_hz(mode, slot).unwrap(), -12700);
        assert_eq!(image.filter_shift_hz(mode, slot).unwrap(), -25);
        for hz in [-50, 25, 50, i32::MIN, i32::MAX] {
            scalar_error(&mut image, true, |i| {
                i.set_filter_pbt_hz(mode, slot, PbtSide::One, hz)
            });
        }
        dsp_change(&mut image, &mut expected, 0x37867, 0, |i| {
            i.set_filter_pbt_hz(mode, slot, PbtSide::One, 0)
        });
        for (mode, bw, low, high) in [
            (FilterMode::Cw, 500, -250, 250),
            (FilterMode::Cw, 600, -300, 300),
            (FilterMode::Cw, 3600, -3300, 300),
            (FilterMode::Am, 10000, -5000, 5000),
        ] {
            let m = mode as usize;
            let step = if mode == FilterMode::Am { 200 } else { 50 };
            image.set_filter_bandwidth_hz(mode, slot, bw).unwrap();
            expected[0x37850 + 3 * m] = if bw == 500 {
                9
            } else if bw == 600 {
                10
            } else if bw == 3600 {
                40
            } else {
                49
            };
            for (side, hz) in [(PbtSide::One, low), (PbtSide::Two, high)] {
                dsp_change(
                    &mut image,
                    &mut expected,
                    0x37867 + 6 * m + side as usize,
                    (hz / step) as i8 as u8,
                    |i| i.set_filter_pbt_hz(mode, slot, side, hz),
                );
            }
            for hz in [low - step, high + step, 1] {
                scalar_error(&mut image, true, |i| {
                    i.set_filter_pbt_hz(mode, slot, PbtSide::Two, hz)
                });
            }
        }
        for (b, band) in [FilterBandGroup::Hf, FilterBandGroup::Mhz50To430]
            .into_iter()
            .enumerate()
        {
            for (s, slot) in [
                FilterShapeSlot::Ssb,
                FilterShapeSlot::SsbData,
                FilterShapeSlot::CwNarrow,
                FilterShapeSlot::CwWide,
            ]
            .into_iter()
            .enumerate()
            {
                let at = 0x3785f + 4 * b + s;
                image.memory[at] = 0xa5;
                expected[at] = 0xa5;
                scalar_error(&mut image, false, |i| {
                    i.filter_shape(band, slot).map(|_| ())
                });
                image.memory[at] = 2;
                expected[at] = 2;
                scalar_error(&mut image, false, |i| {
                    i.filter_shape(band, slot).map(|_| ())
                });
                for (shape, raw) in [(FilterShape::Sharp, 0), (FilterShape::Soft, 1)] {
                    dsp_change(&mut image, &mut expected, at, raw, |i| {
                        i.set_filter_shape(band, slot, shape)
                    });
                    assert_eq!(image.filter_shape(band, slot).unwrap(), shape);
                }
            }
        }
        assert_setting_state(&image, &source, &expected);
    }

    #[test]
    fn dsp_rf_masks_step_codes_agc_domains_and_unknown_replacement() {
        let (source, mut expected) = dsp_fixture();
        let mut image = Icf::parse(&source).unwrap();
        for b in 0..17 {
            scalar_error(&mut image, false, |i| i.preamp(b).map(|_| ()));
            if b <= 12 {
                assert!(image.attenuator(b).unwrap());
                dsp_change(&mut image, &mut expected, 0x378af + b, 0xa3, |i| {
                    i.set_attenuator(b, false)
                });
                dsp_change(&mut image, &mut expected, 0x378af + b, 0xa7, |i| {
                    i.set_attenuator(b, true)
                });
                dsp_change(&mut image, &mut expected, 0x378af + b, 0xa5, |i| {
                    i.set_preamp(b, Preamp::Amp1)
                });
            } else {
                scalar_error(&mut image, false, |i| i.attenuator(b).map(|_| ()));
                scalar_error(&mut image, false, |i| i.set_attenuator(b, false));
                scalar_error(&mut image, true, |i| i.set_preamp(b, Preamp::Amp2));
            }
            for (raw, preamp) in [(0, Preamp::Off), (1, Preamp::Amp1), (2, Preamp::Amp2)] {
                if b > 12 && raw == 2 {
                    continue;
                }
                let byte = (expected[0x378af + b] & !3) | raw;
                dsp_change(&mut image, &mut expected, 0x378af + b, byte, |i| {
                    i.set_preamp(b, preamp)
                });
                assert_eq!(image.preamp(b).unwrap(), preamp);
            }
        }
        for b in [17, usize::MAX] {
            let e = scalar_error(&mut image, false, |i| i.preamp(b).map(|_| ()));
            assert!(matches!(e, Error::Unsupported { line: 5, .. }));
            scalar_error(&mut image, false, |i| i.set_preamp(b, Preamp::Off));
            scalar_error(&mut image, false, |i| i.attenuator(b).map(|_| ()));
            scalar_error(&mut image, false, |i| i.set_attenuator(b, true));
        }
        for (g, mode) in DSP_STEP_MODES.into_iter().enumerate() {
            scalar_error(&mut image, false, |i| i.tuning_step_code(mode).map(|_| ()));
            for code in 0..=12 {
                dsp_change(&mut image, &mut expected, 0x37885 + g, code, |i| {
                    i.set_tuning_step_code(mode, code)
                });
                assert_eq!(image.tuning_step_code(mode).unwrap(), code);
            }
            for code in [13, 255] {
                scalar_error(&mut image, true, |i| i.set_tuning_step_code(mode, code));
            }
            if g >= 4 {
                scalar_error(&mut image, false, |i| i.agc(mode).map(|_| ()));
                scalar_error(&mut image, false, |i| i.set_agc(mode, Agc::Fast));
                for state in DSP_AGC {
                    scalar_error(&mut image, false, |i| {
                        i.agc_delay_code(mode, state).map(|_| ())
                    });
                    scalar_error(&mut image, false, |i| i.set_agc_delay_code(mode, state, 0));
                }
            } else {
                scalar_error(&mut image, false, |i| i.agc(mode).map(|_| ()));
                for (a, state) in DSP_AGC.into_iter().enumerate() {
                    dsp_change(&mut image, &mut expected, 0x378a7 + g, a as u8, |i| {
                        i.set_agc(mode, state)
                    });
                    assert_eq!(image.agc(mode).unwrap(), state);
                    scalar_error(&mut image, false, |i| {
                        i.agc_delay_code(mode, state).map(|_| ())
                    });
                    for code in 0..=13 {
                        dsp_change(&mut image, &mut expected, 0x3788f + 3 * g + a, code, |i| {
                            i.set_agc_delay_code(mode, state, code)
                        });
                        assert_eq!(image.agc_delay_code(mode, state).unwrap(), code);
                    }
                    for code in [14, 255] {
                        scalar_error(&mut image, true, |i| {
                            i.set_agc_delay_code(mode, state, code)
                        });
                    }
                }
            }
        }
        scalar_error(&mut image, false, |i| i.air_tuning_step_code().map(|_| ()));
        for code in 0..=12 {
            dsp_change(&mut image, &mut expected, 0x3788d, code, |i| {
                i.set_air_tuning_step_code(code)
            });
            assert_eq!(image.air_tuning_step_code().unwrap(), code);
        }
        for code in [13, 255] {
            scalar_error(&mut image, true, |i| i.set_air_tuning_step_code(code));
        }
        assert_setting_state(&image, &source, &expected);
    }

    #[test]
    fn dsp_tone_domains_off_codes_levels_and_independent_transmit_endpoints() {
        let (source, mut expected) = dsp_fixture();
        let mut image = Icf::parse(&source).unwrap();
        for (t, mode) in DSP_TONE_MODES.into_iter().enumerate() {
            scalar_error(&mut image, false, |i| {
                i.receive_high_pass_code(mode).map(|_| ())
            });
            scalar_error(&mut image, false, |i| {
                i.receive_low_pass_code(mode).map(|_| ())
            });
            if t == 3 {
                scalar_error(&mut image, false, |i| i.set_receive_high_pass_code(mode, 0));
                scalar_error(&mut image, false, |i| i.set_receive_low_pass_code(mode, 25));
            } else {
                for code in 0..=20 {
                    dsp_change(&mut image, &mut expected, 0x392da + t, code, |i| {
                        i.set_receive_high_pass_code(mode, code)
                    });
                    assert_eq!(image.receive_high_pass_code(mode).unwrap(), code);
                }
                for code in 5..=25 {
                    dsp_change(&mut image, &mut expected, 0x392e2 + t, code, |i| {
                        i.set_receive_low_pass_code(mode, code)
                    });
                    assert_eq!(image.receive_low_pass_code(mode).unwrap(), code);
                }
                for code in [21, 255] {
                    scalar_error(&mut image, true, |i| {
                        i.set_receive_high_pass_code(mode, code)
                    });
                }
                for code in [0, 4, 26, 255] {
                    scalar_error(&mut image, true, |i| {
                        i.set_receive_low_pass_code(mode, code)
                    });
                }
            }
            scalar_error(&mut image, false, |i| i.receive_bass(mode).map(|_| ()));
            scalar_error(&mut image, false, |i| i.receive_treble(mode).map(|_| ()));
            scalar_error(&mut image, false, |i| i.transmit_bass(mode).map(|_| ()));
            scalar_error(&mut image, false, |i| i.transmit_treble(mode).map(|_| ()));
            if t <= 4 {
                for level in -5..=5 {
                    dsp_change(
                        &mut image,
                        &mut expected,
                        0x392ca + t,
                        (level + 5) as u8,
                        |i| i.set_receive_bass(mode, level),
                    );
                    dsp_change(
                        &mut image,
                        &mut expected,
                        0x392d2 + t,
                        (level + 5) as u8,
                        |i| i.set_receive_treble(mode, level),
                    );
                    assert_eq!(image.receive_bass(mode).unwrap(), level);
                    assert_eq!(image.receive_treble(mode).unwrap(), level);
                    if t != 3 {
                        dsp_change(
                            &mut image,
                            &mut expected,
                            0x392c0 + t,
                            (level + 5) as u8,
                            |i| i.set_transmit_bass(mode, level),
                        );
                        dsp_change(
                            &mut image,
                            &mut expected,
                            0x392c5 + t,
                            (level + 5) as u8,
                            |i| i.set_transmit_treble(mode, level),
                        );
                        assert_eq!(image.transmit_bass(mode).unwrap(), level);
                        assert_eq!(image.transmit_treble(mode).unwrap(), level);
                    }
                }
                for level in [-128, -6, 6, 127] {
                    scalar_error(&mut image, true, |i| i.set_receive_bass(mode, level));
                    scalar_error(&mut image, true, |i| i.set_receive_treble(mode, level));
                    if t != 3 {
                        scalar_error(&mut image, true, |i| i.set_transmit_bass(mode, level));
                        scalar_error(&mut image, true, |i| i.set_transmit_treble(mode, level));
                    }
                }
            } else {
                scalar_error(&mut image, false, |i| i.set_receive_bass(mode, 0));
                scalar_error(&mut image, false, |i| i.set_receive_treble(mode, 0));
            }
            if t == 3 || t > 4 {
                scalar_error(&mut image, false, |i| i.set_transmit_bass(mode, 0));
                scalar_error(&mut image, false, |i| i.set_transmit_treble(mode, 0));
            }
        }
        // Contradictory/unknown partner values are preserved by each scalar.
        for (h, l, b, tr) in [(2, 10, 2, 8), (255, 0, 251, 251), (0, 25, 5, 5)] {
            for (at, raw) in [(0x392da, h), (0x392e2, l), (0x392ca, b), (0x392d2, tr)] {
                image.memory[at] = raw;
                expected[at] = raw;
            }
            dsp_change(&mut image, &mut expected, 0x392ca, 0, |i| {
                i.set_receive_bass(ToneMode::Ssb, -5)
            });
            image.memory[0x392ca] = b;
            expected[0x392ca] = b;
            dsp_change(&mut image, &mut expected, 0x392e2, 25, |i| {
                i.set_receive_low_pass_code(ToneMode::Ssb, 25)
            });
            image.memory[0x392e2] = l;
            expected[0x392e2] = l;
            dsp_change(&mut image, &mut expected, 0x392da, 0, |i| {
                i.set_receive_high_pass_code(ToneMode::Ssb, 0)
            });
            dsp_change(&mut image, &mut expected, 0x392e2, 25, |i| {
                i.set_receive_low_pass_code(ToneMode::Ssb, 25)
            });
            dsp_change(&mut image, &mut expected, 0x392ca, 0, |i| {
                i.set_receive_bass(ToneMode::Ssb, -5)
            });
            dsp_change(&mut image, &mut expected, 0x392d2, 10, |i| {
                i.set_receive_treble(ToneMode::Ssb, 5)
            });
        }
        for (p, preset) in DSP_PRESETS.into_iter().enumerate() {
            scalar_error(&mut image, false, |i| {
                i.transmit_bandwidth_low(preset).map(|_| ())
            });
            scalar_error(&mut image, false, |i| {
                i.transmit_bandwidth_high(preset).map(|_| ())
            });
            for code in 0..4 {
                dsp_change(
                    &mut image,
                    &mut expected,
                    0x392ea + 2 * p,
                    code as u8,
                    |i| i.set_transmit_bandwidth_low(preset, DSP_LOW[code]),
                );
                dsp_change(
                    &mut image,
                    &mut expected,
                    0x392eb + 2 * p,
                    code as u8,
                    |i| i.set_transmit_bandwidth_high(preset, DSP_HIGH[code]),
                );
                assert_eq!(image.transmit_bandwidth_low(preset).unwrap(), DSP_LOW[code]);
                assert_eq!(
                    image.transmit_bandwidth_high(preset).unwrap(),
                    DSP_HIGH[code]
                );
            }
        }
        assert_setting_state(&image, &source, &expected);
    }

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
        fixture_with_bytes(&[(offset, value)], checksum)
    }

    fn fixture_with_bytes(changes: &[(usize, u8)], checksum: &str) -> Vec<u8> {
        let mut memory = synthetic_memory();
        for &(offset, value) in changes {
            memory[offset] = value;
        }
        fixture_with_memory(&memory, checksum)
    }

    fn fixture_with_memory(memory: &[u8], checksum: &str) -> Vec<u8> {
        let mut text = b"40750001\r\n#Comment=".to_vec();
        text.extend_from_slice(&[0x82, 0xa0]);
        // 12345639 preserves the old fixture's non-check bits and encodes
        // popcount(0x12345678 & ~0x49) % 8 = 3 in bits 0/3/6.
        text.extend_from_slice(b"\r\n#MapRev=6\r\n#EtcData=12345639\r\n");
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

    fn raw_group_fixture(
        reverse: bool,
        changes: &[(usize, &[u8])],
        checksum: &str,
    ) -> (Vec<u8>, Vec<u8>) {
        let mut memory = synthetic_memory();
        memory[0x36628] = if reverse { 99 } else { 0 };
        for descriptor in 0..100 {
            let base = 0x355c0 + 19 * descriptor;
            memory[base..base + 19].fill(b' ');
            memory[base] = if reverse {
                if descriptor == 0 {
                    255
                } else {
                    (descriptor - 1) as u8
                }
            } else if descriptor == 99 {
                255
            } else {
                (descriptor + 1) as u8
            };
            memory[base + 1..base + 3].copy_from_slice(&[255, 255]);
        }
        memory[0x355c1..0x355c3].copy_from_slice(&[0xf3, 1]);
        memory[0x355c3..0x355d3].copy_from_slice(b"\x82\0\xffRAW_GROUP    ");
        memory[0x355e7..0x355e9].copy_from_slice(&[0, 0]);
        memory[0x355fc] = 0;
        memory[0x35d1c..0x35d2c].copy_from_slice(b"LAST_GROUP      ");
        for &(offset, bytes) in changes {
            memory[offset..offset + bytes.len()].copy_from_slice(bytes);
        }
        // Reuse only the synthetic text envelope, never Icf::to_bytes(). Python
        // hashlib independently supplies CD and the full-file hashes below.
        let mut source = fixture(None);
        let first = source.windows(10).position(|s| s == b"0000000020").unwrap();
        for (address, &byte) in memory.iter().enumerate() {
            let at = first + (address / 32) * 76 + 10 + (address % 32) * 2;
            source[at] = b"0123456789ABCDEF"[usize::from(byte >> 4)];
            source[at + 1] = b"0123456789ABCDEF"[usize::from(byte & 15)];
        }
        let cd = source.len() - 34;
        source[cd..cd + 32].copy_from_slice(checksum.as_bytes());
        (source, memory)
    }

    fn logical_group_fixture() -> (Vec<u8>, Vec<u8>) {
        let mut memory = synthetic_memory();
        memory[0x36628] = 99;
        for d in 0..100 {
            let base = 0x355c0 + 19 * d;
            memory[base..base + 19].fill(b' ');
            memory[base] = if d == 99 {
                0
            } else if d == 98 {
                255
            } else {
                (d + 1) as u8
            };
            memory[base + 1..base + 3].fill(255);
        }
        memory[0x35d2c..0x35d2c + 1000].fill(255);
        memory[0x36114..0x36114 + 1300].fill(0);
        for g in 0..100 {
            memory[0x36114 + 13 * g + 12] = 0xf0;
        }
        memory[0x36114] = 5;
        memory[0x3662d..0x3662d + 500].fill(255);
        for p in [499, 1] {
            memory[0x3662d + p] = 0xfe;
        }
        let base = 0x355c0 + 19 * 99;
        memory[base + 1..base + 3].copy_from_slice(&499_u16.to_le_bytes());
        memory[base + 3..base + 19].copy_from_slice(b"\x82\0\xffLOGICAL      ");
        memory[0x35d2c + 2 * 499..0x35d2c + 2 * 499 + 2].copy_from_slice(&1_u16.to_le_bytes());
        memory[0x355c0 + 19 + 3..0x355c0 + 19 + 19].copy_from_slice(b"NAMED_EMPTY     ");
        // Python stdlib independently constructed this pattern and hashed
        // seed || BE32(address) || 0x20 || payload32 for every record.
        raw_group_fixture(false, &[(0, &memory)], "8ED97592F097E67838197F30627A0694")
    }

    fn check_logical_groups_unchanged(image: &Icf<'_>, check: impl FnOnce(&Icf<'_>)) {
        let memory = image.memory().to_vec();
        let source = image.original_bytes().to_vec();
        let serialized = image.to_bytes();
        check(image);
        assert_setting_state(image, image.original_bytes(), &memory);
        assert_eq!(image.original_bytes(), source);
        assert_eq!(image.to_bytes(), serialized);
    }

    #[test]
    fn reads_logical_groups_in_nonidentity_order_without_mutating_bytes() {
        let (source, expected) = logical_group_fixture();
        let mut image = Icf::parse(&source).unwrap();
        assert_eq!(image.memory(), expected);
        check_logical_groups_unchanged(&image, |image| {
            let groups = image.memory_groups().unwrap();
            assert_eq!(groups.len(), 3);
            assert_eq!(
                groups
                    .iter()
                    .map(|g| g.raw_descriptor())
                    .collect::<Vec<_>>(),
                [99, 0, 1]
            );
            assert_eq!(groups[0].name_bytes(), b"\x82\0\xffLOGICAL      ");
            assert_eq!(
                groups[0].name_bytes().as_ptr(),
                image.memory()[0x35d1c..].as_ptr()
            );
            let mut slots = [None; 100];
            slots[1] = Some(499);
            slots[3] = Some(1);
            assert_eq!(groups[0].slots(), &slots);
            assert_eq!(groups[1].slots(), &[None; 100]);
            assert_eq!(groups[1].name_bytes(), b"                ");
            assert_eq!(groups[2].slots(), &[None; 100]);
            assert_eq!(groups[2].name_bytes(), b"NAMED_EMPTY     ");
        });
        let base = 0x355c0 + 19 * 98;
        image.memory[base + 1..base + 3].copy_from_slice(&2_u16.to_le_bytes());
        image.memory[0x3662d + 2] = 0xfe;
        check_logical_groups_unchanged(&image, |image| {
            let groups = image.memory_groups().unwrap();
            assert_eq!(groups.len(), 100);
            let order: Vec<_> = std::iter::once(99).chain(0..99).collect();
            assert_eq!(
                groups
                    .iter()
                    .map(|g| g.raw_descriptor())
                    .collect::<Vec<_>>(),
                order
            );
            let mut slots = [None; 100];
            slots[0] = Some(2);
            assert_eq!(groups[99].slots(), &slots);
        });
        for d in 0..100 {
            let base = 0x355c0 + 19 * d;
            image.memory[base + 1..base + 3].fill(255);
            image.memory[base + 3..base + 19].fill(b' ');
        }
        check_logical_groups_unchanged(&image, |image| {
            assert!(image.memory_groups().unwrap().is_empty())
        });
    }

    #[test]
    fn rejects_invalid_logical_links_and_flags_without_mutation() {
        let (source, _) = logical_group_fixture();
        let base = 0x355c0 + 19 * 99;
        for (offset, bytes, reason) in [
            (
                0x36628,
                b"\xff".as_slice(),
                "raw memory group order must visit all 100 descriptors once",
            ),
            (
                0x36628,
                b"\x64",
                "raw memory group order must visit all 100 descriptors once",
            ),
            (
                0x355c0,
                b"\xff",
                "raw memory group order must visit all 100 descriptors once",
            ),
            (
                0x355c0,
                b"\0",
                "raw memory group order must visit all 100 descriptors once",
            ),
            (
                0x355c0 + 19 * 98,
                b"\0",
                "raw memory group order must end with FF after 100 descriptors",
            ),
            (
                base + 1,
                b"\xf4\x01",
                "raw memory group head outside physical slots or FFFF",
            ),
            (
                0x35d2c + 2 * 499,
                b"\xf4\x01",
                "logical memory next outside physical slots or FFFF",
            ),
            (
                0x35d2c + 2 * 499,
                b"\xf3\x01",
                "logical memory groups repeat a physical slot",
            ),
            (
                0x355c0 + 1,
                b"\xf3\x01",
                "logical memory groups repeat a physical slot",
            ),
            (
                0x3662d + 499,
                b"\xff",
                "logical memory slot has empty physical flag",
            ),
            (
                0x36114,
                b"\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff",
                "logical memory link remains after 100 rows",
            ),
        ] {
            let mut image = Icf::parse(&source).unwrap();
            image.memory[offset..offset + bytes.len()].copy_from_slice(bytes);
            check_logical_groups_unchanged(&image, |image| {
                let error = image.memory_groups().err().unwrap();
                assert_eq!(
                    error,
                    Error::Unsupported {
                        line: 5 + if offset == 0x36114 { base + 1 } else { offset } / 32,
                        reason
                    }
                );
            });
        }
        let mut image = Icf::parse(&source).unwrap();
        image.memory[base + 1..base + 3].fill(0);
        image.memory[0x36114..0x36114 + 13].fill(0);
        for p in 0..101 {
            image.memory[0x3662d + p] = 0xfe;
            image.memory[0x35d2c + 2 * p..0x35d2c + 2 * p + 2]
                .copy_from_slice(&((p + 1) as u16).to_le_bytes());
        }
        check_logical_groups_unchanged(&image, |image| {
            assert_eq!(
                image.memory_groups().err(),
                Some(Error::Unsupported {
                    line: 5 + (0x35d2c + 2 * 99) / 32,
                    reason: "logical memory link remains after 100 rows",
                })
            );
        });
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

    // Independent literal offsets/last codes; no production constants or registry.
    const EXPANDED_SETTINGS: [(usize, u8); 29] = [
        (0x39968, 1),
        (0x39969, 1),
        (0x3969e, 1),
        (0x396b0, 1),
        (0x396b1, 1),
        (0x396a1, 1),
        (0x396a2, 1),
        (0x396a3, 1),
        (0x396b2, 1),
        (0x396b3, 1),
        (0x39a02, 1),
        (0x39a03, 1),
        (0x39a04, 1),
        (0x3932c, 1),
        (0x39966, 1),
        (0x39967, 1),
        (0x3996a, 3),
        (0x3996c, 3),
        (0x396ad, 1),
        (0x3931f, 1),
        (0x39320, 1),
        (0x39321, 1),
        (0x39322, 2),
        (0x39323, 1),
        (0x37788, 1),
        (0x37789, 1),
        (0x3932e, 1),
        (0x3932f, 2),
        (0x3932d, 1),
    ];

    fn expanded_settings_fixture() -> (Vec<u8>, Vec<u8>) {
        let mut memory = synthetic_memory();
        for (i, byte) in memory[..IMAGE_BYTES - 16].iter_mut().enumerate() {
            *byte = (73 * i + 19) as u8;
        }
        memory[INTERNAL_MAP_REV] = 6;
        memory[0x3996b] = 0x5a;
        for &(offset, _) in &EXPANDED_SETTINGS {
            memory[offset] = 0;
        }
        // Independent Python stdlib: pattern (73*i+19)%256, exact signature,
        // internal 6, literal targets zero, hole 3996B=5A. Hash seed followed
        // by struct.pack('>IB', address, 32) + each payload32, then render text.
        let source = fixture_with_memory(&memory, "C0FEDD2B888F620EF9F199143589080D");
        (source, memory)
    }

    fn check_expanded_setting<T: Copy + fmt::Debug + PartialEq>(
        image: &mut Icf<'_>,
        offset: usize,
        values: &[T],
        read: impl Fn(&Icf<'_>) -> Result<T, Error>,
        write: impl Fn(&mut Icf<'_>, T) -> Result<(), Error>,
    ) {
        let source = image.source;
        let mut expected = image.memory().to_vec();
        for (raw, &value) in values.iter().enumerate() {
            write(image, value).unwrap();
            expected[offset] = raw as u8;
            assert_eq!(read(image), Ok(value));
            assert_setting_state(image, source, &expected);
            write(image, values[0]).unwrap();
            expected[offset] = 0;
            assert_setting_state(image, source, &expected);
        }
        for unknown in [values.len() as u8, 255] {
            image.memory[offset] = unknown;
            expected[offset] = unknown;
            let before = image.to_bytes();
            let error = read(image).unwrap_err();
            assert!(matches!(error, Error::Unsupported { line, .. } if line == 5 + offset / 32));
            for &value in values {
                assert_eq!(write(image, value), Err(error.clone()));
                assert_setting_state(image, source, &expected);
            }
            assert_eq!(image.to_bytes(), before);
        }
        image.memory[offset] = 0;
        write(image, *values.last().unwrap()).unwrap();
        expected[offset] = (values.len() - 1) as u8;
        assert_setting_state(image, source, &expected);
    }

    #[test]
    fn edits_all_29_stored_settings_and_rejects_unknown_current_codes() {
        let (source, mut expected) = expanded_settings_fixture();
        assert_eq!(
            format!("{:X}", md5::compute(&source)),
            "4A6D731756C1F37A20128C30C0AE91A4"
        );
        let mut image = Icf::parse(&source).unwrap();
        macro_rules! check {
            ($offset:literal, $get:ident, $set:ident, [$($value:expr),+]) => {
                check_expanded_setting(&mut image, $offset, &[$($value),+],
                    |image| image.$get(), |image, value| image.$set(value));
            };
        }
        check!(
            0x39968,
            recording_file_split,
            set_recording_file_split,
            [false, true]
        );
        check!(0x39969, ptt_auto_record, set_ptt_auto_record, [false, true]);
        check!(
            0x3969e,
            lcd_backlight_auto_adjust,
            set_lcd_backlight_auto_adjust,
            [false, true]
        );
        check!(
            0x396b0,
            power_key_screen_off,
            set_power_key_screen_off,
            [false, true]
        );
        check!(0x396b1, receive_led, set_receive_led, [false, true]);
        check!(0x396a1, meter_peak_hold, set_meter_peak_hold, [false, true]);
        check!(
            0x396a2,
            memory_name_display,
            set_memory_name_display,
            [false, true]
        );
        check!(
            0x396a3,
            memory_group_name_popup,
            set_memory_group_name_popup,
            [false, true]
        );
        check!(0x396b2, opening_message, set_opening_message, [false, true]);
        check!(0x396b3, power_on_check, set_power_on_check, [false, true]);
        check!(
            0x39a02,
            battery_pack_selection_display,
            set_battery_pack_selection_display,
            [false, true]
        );
        check!(
            0x39a03,
            charging_when_power_on,
            set_charging_when_power_on,
            [false, true]
        );
        check!(0x39a04, usb_power_input, set_usb_power_input, [false, true]);
        check!(
            0x3932c,
            screen_capture_power_key,
            set_screen_capture_power_key,
            [false, true]
        );
        check!(
            0x39966,
            transmit_recording_audio,
            set_transmit_recording_audio,
            [TxRecordingAudio::Direct, TxRecordingAudio::Monitor]
        );
        check!(
            0x39967,
            receive_recording_condition,
            set_receive_recording_condition,
            [
                RecordingCondition::Always,
                RecordingCondition::SquelchLinked
            ]
        );
        check!(
            0x3996a,
            pre_recording_time,
            set_pre_recording_time,
            [
                PreRecordingTime::Off,
                PreRecordingTime::Seconds5,
                PreRecordingTime::Seconds10,
                PreRecordingTime::Seconds15
            ]
        );
        check!(
            0x3996c,
            playback_skip_time,
            set_playback_skip_time,
            [
                PlaybackSkipTime::Seconds3,
                PlaybackSkipTime::Seconds5,
                PlaybackSkipTime::Seconds10,
                PlaybackSkipTime::Seconds30
            ]
        );
        check!(
            0x396ad,
            scroll_speed,
            set_scroll_speed,
            [ScrollSpeed::Slow, ScrollSpeed::Fast]
        );
        check!(
            0x3931f,
            speech_lock_key,
            set_speech_lock_key,
            [SpeechLockKey::SpeechLock, SpeechLockKey::LockSpeech]
        );
        check!(
            0x39320,
            lock_function,
            set_lock_function,
            [LockFunction::MainDial, LockFunction::Panel]
        );
        check!(
            0x39321,
            memory_pad_channels,
            set_memory_pad_channels,
            [MemoryPadChannels::Five, MemoryPadChannels::Ten]
        );
        check!(
            0x39322,
            dial_auto_ts,
            set_dial_auto_ts,
            [DialAutoTs::Off, DialAutoTs::Low, DialAutoTs::High]
        );
        check!(
            0x39323,
            mic_up_down_speed,
            set_mic_up_down_speed,
            [MicUpDownSpeed::Slow, MicUpDownSpeed::Fast]
        );
        check!(
            0x37788,
            memory_channel_selection_scope,
            set_memory_channel_selection_scope,
            [
                MemoryChannelSelectionScope::CurrentGroup,
                MemoryChannelSelectionScope::AllGroups
            ]
        );
        check!(
            0x37789,
            memory_group_display,
            set_memory_group_display,
            [
                MemoryGroupDisplay::RegisteredOnly,
                MemoryGroupDisplay::AllGroups
            ]
        );
        check!(
            0x3932e,
            keyboard_input,
            set_keyboard_input,
            [KeyboardInput::TenKey, KeyboardInput::Full]
        );
        check!(
            0x3932f,
            keyboard_layout,
            set_keyboard_layout,
            [
                KeyboardLayout::JapanUsUk,
                KeyboardLayout::Germany,
                KeyboardLayout::France
            ]
        );
        check!(
            0x3932d,
            screen_capture_file_type,
            set_screen_capture_file_type,
            [ScreenCaptureFileType::Png, ScreenCaptureFileType::Bmp]
        );
        for &(offset, value) in &EXPANDED_SETTINGS {
            expected[offset] = value;
        }
        assert_setting_state(&image, &source, &expected);
        let output = image.to_bytes();
        assert_eq!(output.len(), 589_699);
        assert!(output.ends_with(b"#CD=9ACCF6D3CC4A0D88E4710B66685DD85B\r\n"));
        assert_eq!(
            format!("{:X}", md5::compute(&output)),
            "FC922D25BD93E368A71CC95AC3A49D0A"
        );
        let reparsed = Icf::parse(&output).unwrap();
        assert_setting_state(&reparsed, &output, &expected);
        assert_eq!(reparsed.to_bytes(), output);
        assert_eq!(
            reparsed.pre_recording_time(),
            Ok(PreRecordingTime::Seconds15)
        );
        assert_eq!(reparsed.keyboard_layout(), Ok(KeyboardLayout::France));
    }

    #[test]
    fn preserves_literal_windows_and_disabled_parent_settings() {
        let (source, _) = expanded_settings_fixture();
        let mut image = Icf::parse(&source).unwrap();
        image.memory[0x39966..0x3996e].copy_from_slice(&[0xa6, 0, 1, 0, 0, 0x5a, 0, 0xb7]);
        image.memory[0x37787..0x3778b].copy_from_slice(&[0xa6, 0, 1, 0xb7]);
        image.memory[0x3932b..0x39331].copy_from_slice(&[0xa6, 0, 0, 0, 0, 0xb7]);
        let mut expected = image.memory().to_vec();
        image
            .set_pre_recording_time(PreRecordingTime::Seconds15)
            .unwrap();
        expected[0x3996a] = 3;
        assert_setting_state(&image, &source, &expected);
        assert_eq!(
            &image.memory()[0x39966..0x3996e],
            &[0xa6, 0, 1, 0, 3, 0x5a, 0, 0xb7]
        );
        assert_eq!(image.ptt_auto_record(), Ok(false));
        image
            .set_memory_channel_selection_scope(MemoryChannelSelectionScope::AllGroups)
            .unwrap();
        expected[0x37788] = 1;
        assert_setting_state(&image, &source, &expected);
        assert_eq!(&image.memory()[0x37787..0x3778b], &[0xa6, 1, 1, 0xb7]);
        image.set_keyboard_layout(KeyboardLayout::France).unwrap();
        expected[0x3932f] = 2;
        assert_setting_state(&image, &source, &expected);
        assert_eq!(&image.memory()[0x3932b..0x39331], &[0xa6, 0, 0, 0, 2, 0xb7]);
    }

    const NUMERIC_RAW_OFFSETS: [usize; 11] = [
        0x3969d, 0x392f2, 0x39998, 0x3999c, 0x39964, 0x37815, 0x399ab, 0x399ac, 0x3931e, 0x399a1,
        0x399a3,
    ];

    fn numeric_settings_fixture() -> (Vec<u8>, Vec<u8>) {
        let (_, mut memory) = expanded_settings_fixture();
        for offset in NUMERIC_RAW_OFFSETS {
            memory[offset] = 0;
        }
        for offset in [0x3999a, 0x399ad, 0x399a2, 0x399a4] {
            memory[offset] = 0;
        }
        memory[0x39963] = 1;
        memory[0x399a6] = 128;
        for offset in [0x3930c, 0x396ff, 0x39330] {
            memory[offset..offset + 2].fill(0);
        }
        // Independent Python hashlib/struct over the expanded pattern, with
        // these 20 literal fields set to valid zero/minimum values. Expected
        // edits below use explicit byte windows, never product serialization.
        let source = fixture_with_memory(&memory, "1ACB6B2BF7D6C73F410B7388309F5941");
        (source, memory)
    }

    fn check_numeric_values<T: Copy + fmt::Debug + PartialEq, const N: usize>(
        image: &mut Icf<'_>,
        offset: usize,
        values: &[(T, [u8; N])],
        read: impl Fn(&Icf<'_>) -> Result<T, Error>,
        write: impl Fn(&mut Icf<'_>, T) -> Result<(), Error>,
    ) {
        let source = image.source;
        let mut expected = image.memory().to_vec();
        // Repeat edits to prove setters accept every supported current value.
        for _ in 0..2 {
            for &(value, bytes) in values {
                write(image, value).unwrap();
                expected[offset..offset + N].copy_from_slice(&bytes);
                assert_eq!(read(image), Ok(value));
                assert_setting_state(image, source, &expected);
            }
        }
    }

    #[test]
    fn edits_20_numeric_settings_with_independent_bytes_and_hashes() {
        let (source, mut expected) = numeric_settings_fixture();
        assert_eq!(
            format!("{:X}", md5::compute(&source)),
            "D5E20831C1A4D8B153B01056B4CBCB74"
        );
        let mut image = Icf::parse(&source).unwrap();
        macro_rules! check {
            ($offset:literal, $get:ident, $set:ident, $values:expr) => {
                check_numeric_values(
                    &mut image,
                    $offset,
                    $values,
                    |image| image.$get(),
                    |image, value| image.$set(value),
                );
            };
        }
        let raw: Vec<_> = (0..=255).map(|v| (v, [v])).collect();
        check!(0x3969d, lcd_backlight_raw, set_lcd_backlight_raw, &raw);
        check!(0x392f2, beep_level_raw, set_beep_level_raw, &raw);
        check!(0x39998, microphone_gain_raw, set_microphone_gain_raw, &raw);
        check!(0x3999c, monitor_level_raw, set_monitor_level_raw, &raw);
        check!(
            0x39964,
            voice_transmit_level_raw,
            set_voice_transmit_level_raw,
            &raw
        );
        check!(
            0x37815,
            recording_microphone_gain_raw,
            set_recording_microphone_gain_raw,
            &raw
        );
        check!(0x399ab, vox_gain_raw, set_vox_gain_raw, &raw);
        check!(0x399ac, anti_vox_gain_raw, set_anti_vox_gain_raw, &raw);
        check!(0x3931e, speech_level_raw, set_speech_level_raw, &raw);
        check!(
            0x399a1,
            noise_blanker_level_raw,
            set_noise_blanker_level_raw,
            &raw
        );
        check!(
            0x399a3,
            noise_blanker_width_raw,
            set_noise_blanker_width_raw,
            &raw
        );
        check!(
            0x3999a,
            speech_compression_level,
            set_speech_compression_level,
            &[(0, [0]), (5, [5]), (10, [10])]
        );
        check!(
            0x39963,
            voice_repeat_seconds,
            set_voice_repeat_seconds,
            &[(1, [1]), (8, [8]), (15, [15])]
        );
        check!(
            0x399ad,
            vox_delay_deciseconds,
            set_vox_delay_deciseconds,
            &[(0, [0]), (10, [10]), (20, [20])]
        );
        check!(
            0x399a2,
            noise_blanker_depth,
            set_noise_blanker_depth,
            &[(1, [0]), (5, [4]), (10, [9])]
        );
        check!(
            0x399a4,
            noise_reduction_level,
            set_noise_reduction_level,
            &[(0, [0]), (7, [7]), (15, [15])]
        );
        check!(
            0x399a6,
            manual_notch_position,
            set_manual_notch_position,
            &[
                (-127, [1]),
                (-1, [127]),
                (0, [128]),
                (1, [129]),
                (127, [255])
            ]
        );
        check!(
            0x3930c,
            split_offset_khz,
            set_split_offset_khz,
            &[
                (-9999, [0xf1, 0xd8]),
                (-1, [255, 255]),
                (0, [0, 0]),
                (9999, [0x0f, 0x27])
            ]
        );
        check!(
            0x396ff,
            utc_offset_minutes,
            set_utc_offset_minutes,
            &[
                (-840, [0xb8, 0xfc]),
                (-5, [0xfb, 0xff]),
                (0, [0, 0]),
                (5, [5, 0]),
                (840, [0x48, 3])
            ]
        );
        check!(
            0x39330,
            reference_frequency_adjustment,
            set_reference_frequency_adjustment,
            &[(0, [0, 0]), (255, [255, 0]), (256, [0, 1]), (511, [255, 1])]
        );
        for offset in NUMERIC_RAW_OFFSETS {
            expected[offset] = 255;
        }
        for (offset, value) in [
            (0x3999a, 10),
            (0x39963, 15),
            (0x399ad, 20),
            (0x399a2, 9),
            (0x399a4, 15),
            (0x399a6, 255),
        ] {
            expected[offset] = value;
        }
        for (offset, bytes) in [
            (0x3930c, [0x0f, 0x27]),
            (0x396ff, [0x48, 3]),
            (0x39330, [255, 1]),
        ] {
            expected[offset..offset + 2].copy_from_slice(&bytes);
        }
        assert_setting_state(&image, &source, &expected);
        assert_eq!(
            [
                image.memory()[0x3996b],
                image.memory()[0x37788],
                image.memory()[0x39332],
                image.memory()[0x39701]
            ],
            [0x5a, 0, 0x55, 0x5c]
        );
        assert_eq!(image.lcd_backlight_auto_adjust(), Ok(false));
        let output = image.to_bytes();
        assert_eq!(output.len(), 589_699);
        assert!(output.ends_with(b"#CD=970C0CA090D00251FED3B8E9D21A2849\r\n"));
        assert_eq!(
            format!("{:X}", md5::compute(&output)),
            "51EC1EEB85C3087F3AB47A7F9D0EFD25"
        );
        let reparsed = Icf::parse(&output).unwrap();
        assert_setting_state(&reparsed, &output, &expected);
        assert_eq!(reparsed.to_bytes(), output);
        assert_eq!(reparsed.noise_blanker_depth(), Ok(10));
        assert_eq!(reparsed.manual_notch_position(), Ok(127));
        assert_eq!(reparsed.utc_offset_minutes(), Ok(840));
        assert_eq!(reparsed.reference_frequency_adjustment(), Ok(511));
    }

    fn check_numeric_errors<T: Copy + fmt::Debug + PartialEq, const N: usize>(
        image: &mut Icf<'_>,
        offset: usize,
        valid: T,
        invalid: &[T],
        current: &[[u8; N]],
        read: impl Fn(&Icf<'_>) -> Result<T, Error>,
        write: impl Fn(&mut Icf<'_>, T) -> Result<(), Error>,
    ) {
        let source = image.source;
        let original = image.memory().to_vec();
        let serialized = image.to_bytes();
        for &value in invalid {
            assert!(
                matches!(write(image,value), Err(Error::Invalid {line,..}) if line==5+offset/32)
            );
            assert_setting_state(image, source, &original);
        }
        assert_eq!(image.to_bytes(), serialized);
        for bytes in current {
            let mut expected = original.clone();
            expected[offset..offset + N].copy_from_slice(bytes);
            image.memory[offset..offset + N].copy_from_slice(bytes);
            let serialized = image.to_bytes();
            let error = read(image).unwrap_err();
            assert!(matches!(error,Error::Unsupported {line,..} if line==5+offset/32));
            assert_eq!(write(image, valid), Err(error));
            assert_setting_state(image, source, &expected);
            // Invalid new input wins even when the stored current value is invalid.
            for &value in invalid {
                assert!(
                    matches!(write(image,value),Err(Error::Invalid {line,..}) if line==5+offset/32)
                );
                assert_setting_state(image, source, &expected);
            }
            assert_eq!(image.to_bytes(), serialized);
        }
        image.memory[offset..offset + N].copy_from_slice(&original[offset..offset + N]);
    }

    #[test]
    fn rejects_numeric_input_then_current_without_changing_any_other_byte() {
        let (source, _) = numeric_settings_fixture();
        let mut image = Icf::parse(&source).unwrap();
        macro_rules! reject {
            ($offset:literal,$get:ident,$set:ident,$valid:expr,$invalid:expr,$current:expr) => {
                check_numeric_errors(
                    &mut image,
                    $offset,
                    $valid,
                    $invalid,
                    $current,
                    |image| image.$get(),
                    |image, value| image.$set(value),
                );
            };
        }
        reject!(
            0x3999a,
            speech_compression_level,
            set_speech_compression_level,
            0,
            &[11, 255],
            &[[11], [255]]
        );
        reject!(
            0x39963,
            voice_repeat_seconds,
            set_voice_repeat_seconds,
            1,
            &[0, 16, 255],
            &[[0], [16], [255]]
        );
        reject!(
            0x399ad,
            vox_delay_deciseconds,
            set_vox_delay_deciseconds,
            0,
            &[21, 255],
            &[[21], [255]]
        );
        reject!(
            0x399a2,
            noise_blanker_depth,
            set_noise_blanker_depth,
            1,
            &[0, 11, 255],
            &[[10], [255]]
        );
        reject!(
            0x399a4,
            noise_reduction_level,
            set_noise_reduction_level,
            0,
            &[16, 255],
            &[[16], [255]]
        );
        reject!(
            0x399a6,
            manual_notch_position,
            set_manual_notch_position,
            0,
            &[-128],
            &[[0]]
        );
        reject!(
            0x3930c,
            split_offset_khz,
            set_split_offset_khz,
            0,
            &[-10000, 10000, i16::MIN, i16::MAX],
            &[[0xf0, 0xd8], [0x10, 0x27], [0, 0x80], [0xff, 0x7f]]
        );
        reject!(
            0x396ff,
            utc_offset_minutes,
            set_utc_offset_minutes,
            0,
            &[-845, 845, -1, 1, i16::MIN, i16::MAX],
            &[
                [0xb3, 0xfc],
                [0x4d, 3],
                [255, 255],
                [1, 0],
                [0, 0x80],
                [255, 0x7f]
            ]
        );
        reject!(
            0x39330,
            reference_frequency_adjustment,
            set_reference_frequency_adjustment,
            0,
            &[512, 65535],
            &[[0, 2], [255, 255]]
        );
    }

    // Independent RC IDs, offsets, last codes, masks and shifts. No product registry.
    const SELECTORS: [(u16, usize, u8, u8, u8); 69] = [
        (7, 0x399dd, 1, 0xff, 0),
        (8, 0x399de, 1, 0xff, 0),
        (14, 0x39701, 1, 0xff, 0),
        (15, 0x39702, 1, 0xff, 0),
        (19, 0x39705, 1, 0xff, 0),
        (20, 0x39706, 1, 0xff, 0),
        (21, 0x39707, 1, 0xff, 0),
        (22, 0x39708, 1, 0xff, 0),
        (55, 0x39999, 1, 0xff, 0),
        (59, 0x3999b, 1, 0xff, 0),
        (62, 0x39962, 1, 0xff, 0),
        (66, 0x37781, 1, 0x80, 7),
        (76, 0x3930a, 1, 0xff, 0),
        (81, 0x39310, 1, 0xff, 0),
        (82, 0x39311, 1, 0xff, 0),
        (97, 0x37780, 1, 0x02, 1),
        (98, 0x37784, 1, 0xff, 0),
        (102, 0x37782, 1, 0xff, 0),
        (106, 0x37783, 1, 0xff, 0),
        (116, 0x39326, 1, 0xff, 0),
        (119, 0x3998a, 1, 0xff, 0),
        (141, 0x396bd, 1, 0xff, 0),
        (143, 0x396fe, 1, 0xff, 0),
        (9, 0x399df, 2, 0xff, 0),
        (10, 0x399e0, 2, 0xff, 0),
        (16, 0x39703, 1, 0xff, 0),
        (17, 0x39721, 11, 0xff, 0),
        (18, 0x39704, 2, 0xff, 0),
        (24, 0x39709, 2, 0xff, 0),
        (27, 0x396ae, 6, 0xff, 0),
        (28, 0x396af, 6, 0xff, 0),
        (32, 0x37786, 5, 0xff, 0),
        (38, 0x396bb, 1, 0xff, 0),
        (39, 0x396bc, 1, 0xff, 0),
        (40, 0x396b4, 2, 0xff, 0),
        (41, 0x396b5, 1, 0xff, 0),
        (42, 0x396b6, 2, 0xff, 0),
        (43, 0x396b7, 1, 0xff, 0),
        (44, 0x396b8, 3, 0xff, 0),
        (45, 0x396b9, 1, 0xff, 0),
        (46, 0x396ba, 3, 0xff, 0),
        (57, 0x399b0, 2, 0xff, 0),
        (58, 0x399b1, 2, 0xff, 0),
        (64, 0x37816, 1, 0xff, 0),
        (70, 0x399ae, 3, 0xff, 0),
        (72, 0x39302, 3, 0xff, 0),
        (73, 0x39303, 4, 0xff, 0),
        (75, 0x39309, 5, 0xff, 0),
        (80, 0x3930f, 1, 0xff, 0),
        (108, 0x399a5, 2, 0xff, 0),
        (109, 0x399a7, 2, 0xff, 0),
        (110, 0x399a8, 2, 0xff, 0),
        (111, 0x399a9, 2, 0xff, 0),
        (112, 0x399aa, 2, 0xff, 0),
        (114, 0x39324, 2, 0xff, 0),
        (115, 0x39325, 2, 0xff, 0),
        (117, 0x39327, 1, 0xff, 0),
        (118, 0x39989, 1, 0xff, 0),
        (120, 0x3998b, 10, 0xff, 0),
        (121, 0x3998c, 6, 0xff, 0),
        (122, 0x3998d, 4, 0xff, 0),
        (123, 0x3998e, 1, 0xff, 0),
        (124, 0x37804, 6, 0x0e, 1),
        (126, 0x37802, 3, 0x03, 0),
        (132, 0x39328, 1, 0xff, 0),
        (133, 0x39332, 17, 0xff, 0),
        (134, 0x39333, 4, 0xff, 0),
        (145, 0x399e1, 2, 0xff, 0),
        (146, 0x399e2, 2, 0xff, 0),
    ];

    fn selectors_fixture() -> (Vec<u8>, Vec<u8>) {
        let mut memory = synthetic_memory();
        for &(_, offset, _, mask, _) in &SELECTORS {
            memory[offset] &= !mask;
        }
        // Python hashlib/struct oracle: incremental synthetic memory, literal
        // targets cleared under their masks, internal revision 6 and signature.
        let source = fixture_with_memory(&memory, "B8C52F5234C99BFD2BEC9FB2AF622C42");
        (source, memory)
    }

    #[test]
    fn edits_all_69_selectors_with_independent_bytes_and_hashes() {
        let (source, mut expected) = selectors_fixture();
        assert_eq!(
            format!("{:X}", md5::compute(&source)),
            "33009E5A8B06174553ECA407B6333DAA"
        );
        let mut image = Icf::parse(&source).unwrap();
        let mut seen = Vec::new();
        macro_rules! check {
            ($id:literal, $offset:literal, $get:ident, $set:ident, [$($value:expr),+]) => {
                seen.push(($id, $offset));
                check_expanded_setting(&mut image, $offset, &[$($value),+],
                    |image| image.$get(), |image, value| image.$set(value));
            };
        }
        check!(7, 0x399dd, qso_log, set_qso_log, [false, true]);
        check!(
            8,
            0x399de,
            receive_history_log,
            set_receive_history_log,
            [false, true]
        );
        check!(14, 0x39701, bluetooth, set_bluetooth, [false, true]);
        check!(
            15,
            0x39702,
            bluetooth_auto_connect,
            set_bluetooth_auto_connect,
            [false, true]
        );
        check!(
            19,
            0x39705,
            bluetooth_power_save,
            set_bluetooth_power_save,
            [false, true]
        );
        check!(
            20,
            0x39706,
            bluetooth_one_touch_ptt,
            set_bluetooth_one_touch_ptt,
            [false, true]
        );
        check!(
            21,
            0x39707,
            bluetooth_ptt_beep,
            set_bluetooth_ptt_beep,
            [false, true]
        );
        check!(
            22,
            0x39708,
            bluetooth_custom_key_beep,
            set_bluetooth_custom_key_beep,
            [false, true]
        );
        check!(
            55,
            0x39999,
            speech_compressor,
            set_speech_compressor,
            [false, true]
        );
        check!(59, 0x3999b, monitor, set_monitor, [false, true]);
        check!(
            62,
            0x39962,
            voice_auto_monitor,
            set_voice_auto_monitor,
            [false, true]
        );
        check!(76, 0x3930a, ptt_lock, set_ptt_lock, [false, true]);
        check!(81, 0x39310, tuner_memory, set_tuner_memory, [false, true]);
        check!(82, 0x39311, ptt_tune, set_ptt_tune, [false, true]);
        check!(
            98,
            0x37784,
            quarter_tuning,
            set_quarter_tuning,
            [false, true]
        );
        check!(
            102,
            0x37782,
            noise_blanker,
            set_noise_blanker,
            [false, true]
        );
        check!(
            106,
            0x37783,
            noise_reduction,
            set_noise_reduction,
            [false, true]
        );
        check!(
            116,
            0x39326,
            ssb_cw_frequency_shift,
            set_ssb_cw_frequency_shift,
            [false, true]
        );
        check!(119, 0x3998a, scan_resume, set_scan_resume, [false, true]);
        check!(141, 0x396bd, ntp, set_ntp, [false, true]);
        check!(
            143,
            0x396fe,
            gps_time_auto_correction,
            set_gps_time_auto_correction,
            [false, true]
        );
        check!(
            9,
            0x399df,
            log_csv_separator_decimal,
            set_log_csv_separator_decimal,
            [
                CsvSeparatorDecimal::CommaDot,
                CsvSeparatorDecimal::SemicolonDot,
                CsvSeparatorDecimal::SemicolonComma
            ]
        );
        check!(
            10,
            0x399e0,
            log_csv_date_format,
            set_log_csv_date_format,
            [
                CsvDateFormat::YearMonthDay,
                CsvDateFormat::MonthDayYear,
                CsvDateFormat::DayMonthYear
            ]
        );
        check!(
            16,
            0x39703,
            bluetooth_audio_output,
            set_bluetooth_audio_output,
            [
                BluetoothAudioOutput::HeadsetOnly,
                BluetoothAudioOutput::HeadsetAndSpeaker
            ]
        );
        check!(
            17,
            0x39721,
            bluetooth_auto_disconnect,
            set_bluetooth_auto_disconnect,
            [
                BluetoothAutoDisconnect::Off,
                BluetoothAutoDisconnect::Seconds0,
                BluetoothAutoDisconnect::Seconds1,
                BluetoothAutoDisconnect::Seconds2,
                BluetoothAutoDisconnect::Seconds3,
                BluetoothAutoDisconnect::Seconds4,
                BluetoothAutoDisconnect::Seconds5,
                BluetoothAutoDisconnect::Seconds6,
                BluetoothAutoDisconnect::Seconds7,
                BluetoothAutoDisconnect::Seconds8,
                BluetoothAutoDisconnect::Seconds9,
                BluetoothAutoDisconnect::Seconds10
            ]
        );
        check!(
            18,
            0x39704,
            bluetooth_headset_function,
            set_bluetooth_headset_function,
            [
                BluetoothHeadsetFunction::Normal,
                BluetoothHeadsetFunction::Microphone,
                BluetoothHeadsetFunction::Ptt
            ]
        );
        check!(
            24,
            0x39709,
            bluetooth_serial_function,
            set_bluetooth_serial_function,
            [
                BluetoothSerialFunction::CivNoEcho,
                BluetoothSerialFunction::CivEcho,
                BluetoothSerialFunction::DvData
            ]
        );
        check!(
            27,
            0x396ae,
            battery_screen_saver,
            set_battery_screen_saver,
            [
                ScreenSaverDelay::Off,
                ScreenSaverDelay::Minutes1,
                ScreenSaverDelay::Minutes2,
                ScreenSaverDelay::Minutes5,
                ScreenSaverDelay::Minutes15,
                ScreenSaverDelay::Minutes30,
                ScreenSaverDelay::Minutes60
            ]
        );
        check!(
            28,
            0x396af,
            external_power_screen_saver,
            set_external_power_screen_saver,
            [
                ScreenSaverDelay::Off,
                ScreenSaverDelay::Minutes1,
                ScreenSaverDelay::Minutes2,
                ScreenSaverDelay::Minutes5,
                ScreenSaverDelay::Minutes15,
                ScreenSaverDelay::Minutes30,
                ScreenSaverDelay::Minutes60
            ]
        );
        check!(
            32,
            0x37786,
            transmit_meter,
            set_transmit_meter,
            [
                TransmitMeter::Po,
                TransmitMeter::Swr,
                TransmitMeter::Alc,
                TransmitMeter::Comp,
                TransmitMeter::Vd,
                TransmitMeter::Id
            ]
        );
        check!(
            38,
            0x396bb,
            display_language,
            set_display_language,
            [Language::English, Language::Japanese]
        );
        check!(
            39,
            0x396bc,
            system_language,
            set_system_language,
            [Language::English, Language::Japanese]
        );
        check!(
            40,
            0x396b4,
            coordinate_display,
            set_coordinate_display,
            [
                CoordinateDisplay::DegreesDecimalMinutes,
                CoordinateDisplay::DegreesMinutesSeconds,
                CoordinateDisplay::DecimalDegrees
            ]
        );
        check!(
            41,
            0x396b5,
            altitude_distance_unit,
            set_altitude_distance_unit,
            [
                AltitudeDistanceUnit::Meters,
                AltitudeDistanceUnit::FeetMiles
            ]
        );
        check!(
            42,
            0x396b6,
            speed_unit,
            set_speed_unit,
            [
                SpeedUnit::KilometersPerHour,
                SpeedUnit::MilesPerHour,
                SpeedUnit::Knots
            ]
        );
        check!(
            43,
            0x396b7,
            temperature_unit,
            set_temperature_unit,
            [TemperatureUnit::Celsius, TemperatureUnit::Fahrenheit]
        );
        check!(
            44,
            0x396b8,
            pressure_unit,
            set_pressure_unit,
            [
                PressureUnit::Hectopascals,
                PressureUnit::Millibars,
                PressureUnit::MillimetersMercury,
                PressureUnit::InchesMercury
            ]
        );
        check!(
            45,
            0x396b9,
            rainfall_unit,
            set_rainfall_unit,
            [RainfallUnit::Millimeters, RainfallUnit::Inches]
        );
        check!(
            46,
            0x396ba,
            wind_speed_unit,
            set_wind_speed_unit,
            [
                WindSpeedUnit::MetersPerSecond,
                WindSpeedUnit::KilometersPerHour,
                WindSpeedUnit::MilesPerHour,
                WindSpeedUnit::Knots
            ]
        );
        check!(
            57,
            0x399b0,
            compressor_off_transmit_bandwidth,
            set_compressor_off_transmit_bandwidth,
            [
                TransmitBandwidth::Wide,
                TransmitBandwidth::Mid,
                TransmitBandwidth::Narrow
            ]
        );
        check!(
            58,
            0x399b1,
            compressor_on_transmit_bandwidth,
            set_compressor_on_transmit_bandwidth,
            [
                TransmitBandwidth::Wide,
                TransmitBandwidth::Mid,
                TransmitBandwidth::Narrow
            ]
        );
        check!(
            64,
            0x37816,
            recording_microphone,
            set_recording_microphone,
            [
                RecordingMicrophone::HandMicrophone,
                RecordingMicrophone::BluetoothMicrophone
            ]
        );
        check!(
            70,
            0x399ae,
            vox_voice_delay,
            set_vox_voice_delay,
            [
                VoxVoiceDelay::Off,
                VoxVoiceDelay::Short,
                VoxVoiceDelay::Mid,
                VoxVoiceDelay::Long
            ]
        );
        check!(
            72,
            0x39302,
            battery_power_limit,
            set_battery_power_limit,
            [
                BatteryPowerLimit::Milliwatts500,
                BatteryPowerLimit::Watts1,
                BatteryPowerLimit::Milliwatts2500,
                BatteryPowerLimit::Watts5
            ]
        );
        check!(
            73,
            0x39303,
            external_power_limit,
            set_external_power_limit,
            [
                ExternalPowerLimit::Milliwatts500,
                ExternalPowerLimit::Watts1,
                ExternalPowerLimit::Milliwatts2500,
                ExternalPowerLimit::Watts5,
                ExternalPowerLimit::Watts10
            ]
        );
        check!(
            75,
            0x39309,
            transmit_timeout,
            set_transmit_timeout,
            [
                TransmitTimeout::Off,
                TransmitTimeout::Minutes3,
                TransmitTimeout::Minutes5,
                TransmitTimeout::Minutes10,
                TransmitTimeout::Minutes20,
                TransmitTimeout::Minutes30
            ]
        );
        check!(
            80,
            0x3930f,
            tuner_selection,
            set_tuner_selection,
            [TunerSelection::Ah705, TunerSelection::Other]
        );
        check!(
            108,
            0x399a5,
            notch_filter,
            set_notch_filter,
            [NotchFilter::Off, NotchFilter::Auto, NotchFilter::Manual]
        );
        check!(
            109,
            0x399a7,
            ssb_manual_notch_width,
            set_ssb_manual_notch_width,
            [
                ManualNotchWidth::Wide,
                ManualNotchWidth::Mid,
                ManualNotchWidth::Narrow
            ]
        );
        check!(
            110,
            0x399a8,
            cw_manual_notch_width,
            set_cw_manual_notch_width,
            [
                ManualNotchWidth::Wide,
                ManualNotchWidth::Mid,
                ManualNotchWidth::Narrow
            ]
        );
        check!(
            111,
            0x399a9,
            rtty_manual_notch_width,
            set_rtty_manual_notch_width,
            [
                ManualNotchWidth::Wide,
                ManualNotchWidth::Mid,
                ManualNotchWidth::Narrow
            ]
        );
        check!(
            112,
            0x399aa,
            am_manual_notch_width,
            set_am_manual_notch_width,
            [
                ManualNotchWidth::Wide,
                ManualNotchWidth::Mid,
                ManualNotchWidth::Narrow
            ]
        );
        check!(
            114,
            0x39324,
            ssb_notch_key_function,
            set_ssb_notch_key_function,
            [
                NotchKeyFunction::Auto,
                NotchKeyFunction::Manual,
                NotchKeyFunction::AutoManual
            ]
        );
        check!(
            115,
            0x39325,
            am_notch_key_function,
            set_am_notch_key_function,
            [
                NotchKeyFunction::Auto,
                NotchKeyFunction::Manual,
                NotchKeyFunction::AutoManual
            ]
        );
        check!(
            117,
            0x39327,
            cw_bfo_side,
            set_cw_bfo_side,
            [CwBfoSide::LowerSideband, CwBfoSide::UpperSideband]
        );
        check!(
            118,
            0x39989,
            scan_speed,
            set_scan_speed,
            [ScanSpeed::Slow, ScanSpeed::Fast]
        );
        check!(
            120,
            0x3998b,
            scan_pause_time,
            set_scan_pause_time,
            [
                ScanPauseTime::Seconds2,
                ScanPauseTime::Seconds4,
                ScanPauseTime::Seconds6,
                ScanPauseTime::Seconds8,
                ScanPauseTime::Seconds10,
                ScanPauseTime::Seconds12,
                ScanPauseTime::Seconds14,
                ScanPauseTime::Seconds16,
                ScanPauseTime::Seconds18,
                ScanPauseTime::Seconds20,
                ScanPauseTime::Hold
            ]
        );
        check!(
            121,
            0x3998c,
            scan_restart_time,
            set_scan_restart_time,
            [
                ScanRestartTime::Seconds0,
                ScanRestartTime::Seconds1,
                ScanRestartTime::Seconds2,
                ScanRestartTime::Seconds3,
                ScanRestartTime::Seconds4,
                ScanRestartTime::Seconds5,
                ScanRestartTime::Hold
            ]
        );
        check!(
            122,
            0x3998d,
            temporary_skip_time,
            set_temporary_skip_time,
            [
                TemporarySkipTime::Minutes5,
                TemporarySkipTime::Minutes10,
                TemporarySkipTime::Minutes15,
                TemporarySkipTime::WhileScanning,
                TemporarySkipTime::UntilPowerOff
            ]
        );
        check!(
            123,
            0x3998e,
            scan_main_dial,
            set_scan_main_dial,
            [ScanMainDial::Stop, ScanMainDial::UpDown]
        );
        check!(
            132,
            0x39328,
            remote_power_off_mode,
            set_remote_power_off_mode,
            [
                RemotePowerOffMode::ShutdownOnly,
                RemotePowerOffMode::StandbyShutdown
            ]
        );
        check!(
            133,
            0x39332,
            vox_break_in_key_function,
            set_vox_break_in_key_function,
            [
                VoxBreakInKeyFunction::Transmit,
                VoxBreakInKeyFunction::Tuner,
                VoxBreakInKeyFunction::VoxBreakIn,
                VoxBreakInKeyFunction::PreampAttenuator,
                VoxBreakInKeyFunction::Notch,
                VoxBreakInKeyFunction::NoiseBlanker,
                VoxBreakInKeyFunction::NoiseReduction,
                VoxBreakInKeyFunction::Split,
                VoxBreakInKeyFunction::VfoAOrB,
                VoxBreakInKeyFunction::VfoOrMemory,
                VoxBreakInKeyFunction::Cd,
                VoxBreakInKeyFunction::Preset,
                VoxBreakInKeyFunction::HomeChannel,
                VoxBreakInKeyFunction::TemporarySkip,
                VoxBreakInKeyFunction::VoiceKeyerRttyMemory1,
                VoxBreakInKeyFunction::VoiceKeyerRttyMemory2,
                VoxBreakInKeyFunction::VoiceKeyerRttyMemory3,
                VoxBreakInKeyFunction::VoiceKeyerRttyMemory4
            ]
        );
        check!(
            134,
            0x39333,
            auto_tune_rx_callsign_key_function,
            set_auto_tune_rx_callsign_key_function,
            [
                AutoTuneRxCallsignKeyFunction::AutoTuneRxCallsign,
                AutoTuneRxCallsignKeyFunction::CdRxCallsign,
                AutoTuneRxCallsignKeyFunction::PresetRxCallsign,
                AutoTuneRxCallsignKeyFunction::HomeChannelRxCallsign,
                AutoTuneRxCallsignKeyFunction::TemporarySkipRxCallsign
            ]
        );
        check!(
            145,
            0x399e1,
            repeater_csv_separator_decimal,
            set_repeater_csv_separator_decimal,
            [
                CsvSeparatorDecimal::CommaDot,
                CsvSeparatorDecimal::SemicolonDot,
                CsvSeparatorDecimal::SemicolonComma
            ]
        );
        check!(
            146,
            0x399e2,
            repeater_csv_date_format,
            set_repeater_csv_date_format,
            [
                CsvDateFormat::YearMonthDay,
                CsvDateFormat::MonthDayYear,
                CsvDateFormat::DayMonthYear
            ]
        );
        // The four masked fields are swept over every starting byte below.
        image.set_vox(true).unwrap();
        image.set_one_hz_mode(true).unwrap();
        image.set_delta_f_span(DeltaFSpan::Mhz1).unwrap();
        image
            .set_select_memory_scan(SelectMemoryScan::ThreeStars)
            .unwrap();
        seen.extend([(66, 0x37781), (97, 0x37780), (124, 0x37804), (126, 0x37802)]);
        seen.sort_unstable();
        let mut targets: Vec<_> = SELECTORS
            .iter()
            .map(|&(id, at, _, _, _)| (id, at))
            .collect();
        targets.sort_unstable();
        assert_eq!(seen, targets);
        assert!(targets.windows(2).all(|w| w[0].0 != w[1].0));
        for &(_, offset, last, mask, shift) in &SELECTORS {
            expected[offset] = (expected[offset] & !mask) | (last << shift);
        }
        assert_setting_state(&image, &source, &expected);
        assert_eq!(
            [
                expected[0x37780],
                expected[0x37781],
                expected[0x37802],
                expected[0x37804]
            ],
            [0x82, 0x81, 0x03, 0x0c]
        );
        let output = image.to_bytes();
        assert_eq!(output.len(), 589_699);
        assert!(output.ends_with(b"#CD=27D099B9DAAD804BF73C1A0A4E8B6496\r\n"));
        assert_eq!(
            format!("{:X}", md5::compute(&output)),
            "172FA265602BBA9E01DBB7CB318AB5AE"
        );
        let reparsed = Icf::parse(&output).unwrap();
        assert_setting_state(&reparsed, &output, &expected);
        assert_eq!(reparsed.to_bytes(), output);
    }

    fn check_masked_selector<T: Copy + fmt::Debug + PartialEq>(
        image: &mut Icf<'_>,
        field: (usize, u8, u8),
        values: &[T],
        read: impl Fn(&Icf<'_>) -> Result<T, Error>,
        write: impl Fn(&mut Icf<'_>, T) -> Result<(), Error>,
    ) {
        let (offset, mask, shift) = field;
        let source = image.source;
        let mut expected = image.memory().to_vec();
        for original in 0..=u8::MAX {
            let code = usize::from((original & mask) >> shift);
            image.memory[offset] = original;
            expected[offset] = original;
            let error = if code < values.len() {
                assert_eq!(read(image), Ok(values[code]));
                None
            } else {
                let error = read(image).unwrap_err();
                assert!(
                    matches!(error, Error::Unsupported { line, .. } if line == 5 + offset / 32)
                );
                Some(error)
            };
            let before = error.as_ref().map(|_| image.to_bytes());
            for (new_code, &value) in values.iter().enumerate() {
                // Each new value is checked against this exact original byte.
                image.memory[offset] = original;
                if let Some(ref error) = error {
                    assert_eq!(write(image, value), Err(error.clone()));
                    expected[offset] = original;
                } else {
                    write(image, value).unwrap();
                    expected[offset] = (original & !mask) | ((new_code as u8) << shift);
                    assert_eq!(read(image), Ok(value));
                }
                assert_setting_state(image, source, &expected);
            }
            if let Some(before) = before {
                assert_eq!(image.to_bytes(), before);
            }
        }
    }

    #[test]
    fn preserves_all_four_masks_for_every_byte_and_inactive_stored_values() {
        let (source, _) = selectors_fixture();
        let mut image = Icf::parse(&source).unwrap();
        check_masked_selector(
            &mut image,
            (0x37781, 0x80, 7),
            &[false, true],
            |i| i.vox(),
            |i, v| i.set_vox(v),
        );
        check_masked_selector(
            &mut image,
            (0x37780, 0x02, 1),
            &[false, true],
            |i| i.one_hz_mode(),
            |i, v| i.set_one_hz_mode(v),
        );
        check_masked_selector(
            &mut image,
            (0x37804, 0x0e, 1),
            &[
                DeltaFSpan::Khz5,
                DeltaFSpan::Khz10,
                DeltaFSpan::Khz20,
                DeltaFSpan::Khz50,
                DeltaFSpan::Khz100,
                DeltaFSpan::Khz500,
                DeltaFSpan::Mhz1,
            ],
            |i| i.delta_f_span(),
            |i, v| i.set_delta_f_span(v),
        );
        check_masked_selector(
            &mut image,
            (0x37802, 0x03, 0),
            &[
                SelectMemoryScan::AllStars,
                SelectMemoryScan::OneStar,
                SelectMemoryScan::TwoStars,
                SelectMemoryScan::ThreeStars,
            ],
            |i| i.select_memory_scan(),
            |i, v| i.set_select_memory_scan(v),
        );
        // Parent OFF does not erase or block independently stored child values.
        let mut image = Icf::parse(&source).unwrap();
        let mut expected = image.memory().to_vec();
        image
            .set_bluetooth_auto_disconnect(BluetoothAutoDisconnect::Seconds10)
            .unwrap();
        image.set_bluetooth_power_save(true).unwrap();
        image
            .set_compressor_on_transmit_bandwidth(TransmitBandwidth::Narrow)
            .unwrap();
        image.set_vox_voice_delay(VoxVoiceDelay::Long).unwrap();
        for (at, value) in [(0x39721, 11), (0x39705, 1), (0x399b1, 2), (0x399ae, 3)] {
            expected[at] = value;
        }
        assert_eq!(image.bluetooth(), Ok(false));
        assert_eq!(image.speech_compressor(), Ok(false));
        assert_eq!(image.vox(), Ok(false));
        assert_setting_state(&image, &source, &expected);
        let output = image.to_bytes();
        let reparsed = Icf::parse(&output).unwrap();
        assert_setting_state(&reparsed, &output, &expected);
        assert_eq!(
            reparsed.bluetooth_auto_disconnect(),
            Ok(BluetoothAutoDisconnect::Seconds10)
        );
        assert_eq!(
            reparsed.compressor_on_transmit_bandwidth(),
            Ok(TransmitBandwidth::Narrow)
        );
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
    fn edits_split_speech_settings_and_rejects_every_unknown_current_value() {
        // Independent Python hashlib/raw-splice recipe, as for the older fields:
        // image = bytearray(i % 256 for i in range(0x3c9c0))
        // image[0x355a1] = 6; image[-16:] = b"IcomCloneFormat3"; image[offset] = raw
        // h = hashlib.md5(bytes.fromhex("440077554075"))
        // for a in range(0, len(image), 32):
        //     h.update(struct.pack(">IB", a, 32) + image[a:a+32])
        // Fixed digests below are not generated from product output.
        check_common_setting(
            0x3930b,
            &[
                (0, false, "419606C01C486CC75B4E26CEC396F641"),
                (1, true, "6A863B5ED7B664798A724864497CC474"),
            ],
            "3A4AA85B7E60E01232E077B2094951F6",
            Error::Unsupported {
                line: 7325,
                reason: "quick-split value other than OFF or ON",
            },
            |image| image.quick_split(),
            |image, value| image.set_quick_split(value),
        );
        check_common_setting(
            0x3930e,
            &[
                (0, false, "7AB37FC106438F57D7CC16EF693167B8"),
                (1, true, "0B2D84502D34F4CC54B31181252A9D18"),
            ],
            "8F564D7A79F0ED4C5175F8F8AA345A56",
            Error::Unsupported {
                line: 7325,
                reason: "split-lock value other than OFF or ON",
            },
            |image| image.split_lock(),
            |image, value| image.set_split_lock(value),
        );
        check_common_setting(
            0x39319,
            &[
                (
                    0,
                    ReceivedCallsignSpeech::Off,
                    "659AC62413D2F17366BE2E07D17BBFBD",
                ),
                (
                    1,
                    ReceivedCallsignSpeech::Kerchunk,
                    "2051A951920A82925C4401F0B24E34B0",
                ),
                (
                    2,
                    ReceivedCallsignSpeech::All,
                    "1F790AF9E4DA19C6D96BE9E6E8313EA1",
                ),
            ],
            "B4D4B00FDE7A7AF383892616E6452F9B",
            Error::Unsupported {
                line: 7325,
                reason: "received callsign speech selection outside 0 through 2",
            },
            |image| image.received_callsign_speech(),
            |image, value| image.set_received_callsign_speech(value),
        );
        check_common_setting(
            0x3931a,
            &[
                (0, false, "C177FAEEEFB5E123EB81C2015D52FF2B"),
                (1, true, "230613C22A5162203E51F98712CD5CE8"),
            ],
            "55A1B22FE54F866D1E4F1FF4A1963E18",
            Error::Unsupported {
                line: 7325,
                reason: "RX-to-CS speech value other than OFF or ON",
            },
            |image| image.rx_cs_speech(),
            |image, value| image.set_rx_cs_speech(value),
        );
        check_common_setting(
            0x3931c,
            &[
                (0, false, "7CDADA231493A038D7DA3669CCBC2AF9"),
                (1, true, "77E089B86BD121F70751CFBBE9E371FC"),
            ],
            "93614F7F028691D292C80FC439E4810E",
            Error::Unsupported {
                line: 7325,
                reason: "MIC Up/Down speech value other than OFF or ON",
            },
            |image| image.mic_up_down_speech(),
            |image, value| image.set_mic_up_down_speech(value),
        );
        check_common_setting(
            0x3931b,
            &[
                (0, false, "D97F4344065EF9BF72BF4AAFFD984793"),
                (1, true, "97C966FF8544EC34415A689933004079"),
            ],
            "57D2C2802A250384820ABAE71F326765",
            Error::Unsupported {
                line: 7325,
                reason: "S-meter level speech value other than OFF or ON",
            },
            |image| image.s_meter_level_speech(),
            |image, value| image.set_s_meter_level_speech(value),
        );
        check_common_setting(
            0x3931d,
            &[
                (0, false, "DAEB353E5966CD9934E6738175C7FA80"),
                (1, true, "3415D48D225284DC5FEFB3CE3AF16AEE"),
            ],
            "D3D9A18FE74837AECEB0DBA66D175035",
            Error::Unsupported {
                line: 7325,
                reason: "mode speech value other than OFF or ON",
            },
            |image| image.mode_speech(),
            |image, value| image.set_mode_speech(value),
        );
        check_common_setting(
            0x39316,
            &[
                (
                    0,
                    SpeechLanguage::English,
                    "575C57FDB07DFAED5FB4BBF41B54F466",
                ),
                (
                    1,
                    SpeechLanguage::Japanese,
                    "B9D13EAC94C01523FAD9F3688D367A88",
                ),
            ],
            "14040B1BCC5DF1541DB5A6996405C388",
            Error::Unsupported {
                line: 7325,
                reason: "speech language selection outside 0 through 1",
            },
            |image| image.speech_language(),
            |image, value| image.set_speech_language(value),
        );
        check_common_setting(
            0x39317,
            &[
                (
                    0,
                    SpeechAlphabet::Normal,
                    "29C79CA70A1AA59BD9DB245F7D23682E",
                ),
                (
                    1,
                    SpeechAlphabet::Phonetic,
                    "05AC8EF08ED9BA7A2B8458F73B999598",
                ),
            ],
            "F8C3E5BF1AFFB2EDDE1874941800E6E4",
            Error::Unsupported {
                line: 7325,
                reason: "speech alphabet selection outside 0 through 1",
            },
            |image| image.speech_alphabet(),
            |image, value| image.set_speech_alphabet(value),
        );
        check_common_setting(
            0x39318,
            &[
                (0, SpeechSpeed::Slow, "34E1EB28639ACBE65B65D22A50F0577E"),
                (1, SpeechSpeed::Fast, "4A4AC9931913D8A6A90CE8670BB9454D"),
            ],
            "12A654387530F5BDD0AB642ABFC690EE",
            Error::Unsupported {
                line: 7325,
                reason: "speech speed selection outside 0 through 1",
            },
            |image| image.speech_speed(),
            |image, value| image.set_speech_speed(value),
        );
    }

    #[test]
    fn consecutive_split_speech_edits_preserve_unrelated_unknown_data() {
        // Explicit synthetic starting values, not defaults supplied by the parser.
        let initial = [
            (0x3930b, 0),
            (0x3930e, 0),
            (0x39319, 0),
            (0x3931a, 0),
            (0x3931c, 0),
            (0x3931b, 0),
            (0x3931d, 0),
            (0x39316, 0),
            (0x39317, 0),
            (0x39318, 0),
        ];
        // The same independent Python recipe, applying all raw splices before MD5.
        let source = fixture_with_bytes(&initial, "6A07F433BB8B905E942D575557C0AF9C");
        assert_eq!(
            format!("{:X}", md5::compute(&source)),
            "D271EBCD7D8E9AF57D3C4AC8FF5023D8"
        );
        let mut image = Icf::parse(&source).unwrap();
        assert!(image.key_touch_beep().is_err());
        image.set_quick_split(true).unwrap();
        image.set_split_lock(true).unwrap();
        image
            .set_received_callsign_speech(ReceivedCallsignSpeech::All)
            .unwrap();
        image.set_rx_cs_speech(true).unwrap();
        image.set_mic_up_down_speech(true).unwrap();
        image.set_s_meter_level_speech(true).unwrap();
        image.set_mode_speech(true).unwrap();
        image.set_speech_language(SpeechLanguage::Japanese).unwrap();
        image.set_speech_alphabet(SpeechAlphabet::Phonetic).unwrap();
        image.set_speech_speed(SpeechSpeed::Fast).unwrap();
        image.set_quick_split(false).unwrap();
        let final_values = [
            (0x3930b, 0),
            (0x3930e, 1),
            (0x39319, 2),
            (0x3931a, 1),
            (0x3931c, 1),
            (0x3931b, 1),
            (0x3931d, 1),
            (0x39316, 1),
            (0x39317, 1),
            (0x39318, 1),
        ];
        let mut expected = synthetic_memory();
        for (offset, value) in final_values {
            expected[offset] = value;
        }
        assert_setting_state(&image, &source, &expected);
        assert_eq!(&image.memory()[0x3930c..0x3930e], &[0x0c, 0x0d]);
        assert!(image.key_touch_beep().is_err());
        let serialized = image.to_bytes();
        assert_eq!(
            serialized,
            fixture_with_bytes(&final_values, "42190843018BFB6BA418FE4DA031C061")
        );
        assert_eq!(
            format!("{:X}", md5::compute(&serialized)),
            "83AF0A19CD1B392770EBD741B0116730"
        );
        let reparsed = Icf::parse(&serialized).unwrap();
        assert_setting_state(&reparsed, &serialized, &expected);
        assert_eq!(
            reparsed.speech_language().unwrap(),
            SpeechLanguage::Japanese
        );
        assert_eq!(
            reparsed.received_callsign_speech().unwrap(),
            ReceivedCallsignSpeech::All
        );
        assert!(reparsed.mic_up_down_speech().unwrap());
        assert!(reparsed.s_meter_level_speech().unwrap());
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

    #[test]
    fn reads_raw_group_descriptors_and_edits_only_their_name_windows() {
        for (reverse, checksum, baseline_md5, edit_md5) in [
            (
                false,
                "E1C1750FBE655BA68E44B82F8F3D6654",
                "A605BCC6F3E9EF3AECB671EED70C3BCD",
                [
                    "0305A6B0F2F914F99DA09DF0FBB159EF",
                    "8DD057668387631DE1AA609773A684D6",
                    "11BBB159C5535E8444A9746E240EBB43",
                ],
            ),
            (
                true,
                "147BA5A83AEF431704436D62AE2574C6",
                "6F4F48710EAEE7095142DFFF65D9FD1E",
                [
                    "3DBACF270DD0E58D2F07884925240E03",
                    "C1A56B1784A81290A0E84800897FF3ED",
                    "2095A4BFFAEFBFA587E1C0096548F23C",
                ],
            ),
        ] {
            let (source, mut expected) = raw_group_fixture(reverse, &[], checksum);
            assert_eq!(format!("{:X}", md5::compute(&source)), baseline_md5);
            let mut image = Icf::parse(&source).unwrap();
            assert_setting_state(&image, &source, &expected);
            assert_eq!(image.raw_memory_group_name(1).unwrap(), None);
            // Descriptor 2 exists despite an all-space name (head 0); descriptor
            // 3 exists despite head FFFF (NUL is not an all-space sentinel).
            for (descriptor, offset) in [(0, 0x355c3), (2, 0x355e9), (3, 0x355fc), (99, 0x35d1c)] {
                let name = image.raw_memory_group_name(descriptor).unwrap().unwrap();
                assert_eq!(name, &expected[offset..offset + 16]);
                assert_eq!(name.as_ptr(), image.memory()[offset..].as_ptr());
            }
            for ((descriptor, offset, name, padded), file_md5) in [
                (0, 0x355c3, "A", b"A               "),
                (99, 0x35d1c, "AZ09_-AZ09_-AZ09", b"AZ09_-AZ09_-AZ09"),
                (2, 0x355e9, "LINKED_0", b"LINKED_0        "),
            ]
            .into_iter()
            .zip(edit_md5)
            {
                image
                    .set_raw_memory_group_name_ascii(descriptor, name)
                    .unwrap();
                expected[offset..offset + 16].copy_from_slice(padded);
                assert_setting_state(&image, &source, &expected);
                assert_eq!(
                    image.raw_memory_group_name(descriptor).unwrap(),
                    Some(padded.as_slice())
                );
                let serialized = image.to_bytes();
                assert_eq!(serialized.len(), 589_699);
                assert_eq!(format!("{:X}", md5::compute(&serialized)), file_md5);
                let reparsed = Icf::parse(&serialized).unwrap();
                assert_setting_state(&reparsed, &serialized, &expected);
                assert_eq!(
                    reparsed.raw_memory_group_name(descriptor).unwrap(),
                    Some(padded.as_slice())
                );
            }
            let serialized = image.to_bytes();
            image.set_raw_memory_group_name_ascii(0, "SECOND").unwrap();
            image.set_raw_memory_group_name_ascii(0, "A").unwrap();
            assert_setting_state(&image, &source, &expected);
            assert_eq!(image.to_bytes(), serialized);
        }

        // Only the selected head is guarded. An unrelated head 500 and all
        // uninterpreted slot/row/flag bytes are neither validated nor repaired.
        let (source, mut expected) = raw_group_fixture(
            false,
            &[(0x355d4, b"\xf4\x01")],
            "D75452CD9745E2FEF3161167009FD339",
        );
        let mut image = Icf::parse(&source).unwrap();
        assert!(matches!(
            image.raw_memory_group_name(1),
            Err(Error::Unsupported { .. })
        ));
        image.set_raw_memory_group_name_ascii(0, "A").unwrap();
        expected[0x355c3..0x355d3].copy_from_slice(b"A               ");
        assert_setting_state(&image, &source, &expected);
        assert_eq!(
            format!("{:X}", md5::compute(image.to_bytes())),
            "C6C45367606CC66D3A79FDCF5123C182"
        );
    }

    #[test]
    fn rejects_unsupported_raw_group_edits_without_changing_state() {
        // Every malformed chain/head enters through parse with independently
        // computed CD, so rejection belongs to the raw-group API, not parse.
        for (descriptor, offset, value, checksum) in [
            (
                0,
                0x36628,
                b"\xff".as_slice(),
                "325015A25F65C3DF191D1F8E6D7711DC",
            ),
            (0, 0x36628, b"\x64", "A2A87E93B128D2B0D85D523C1CAEE7A6"),
            (0, 0x36628, b"\xfe", "841C4746799D82C7ECE4E8EE5360A6F9"),
            (0, 0x355c0, b"\xff", "34658673E7ABE5750D4FD235D39C7840"),
            (0, 0x35d06, b"\xff", "D3E445748790111CE5A4D9EC1E52D06C"),
            (0, 0x35976, b"\x32", "5B3FE7FF6611831884E1AB7D7C769A5D"),
            (0, 0x35976, b"\x31", "DDE5501236458BBF9128C7E7908BF1FE"),
            (0, 0x35976, b"\x64", "2608B4D32FF398C4B180F03C28079110"),
            (0, 0x35976, b"\xfe", "CABBB1F63B49136715A2D130FB6EFAEF"),
            (0, 0x35d19, b"\0", "403187659A7ED6336E7A4E32AD679B68"),
            (0, 0x35d19, b"\x64", "ECC68FE40CBDC0894AC5B9D6BE74FBCE"),
            (0, 0x355c1, b"\xf4\x01", "729582B41C488C74B855A038F8664FBC"),
            (0, 0x355c1, b"\xfe\xff", "E986B057EDEEFFC09840EA2B1E180FD4"),
            (99, 0x35d1a, b"\xf4\x01", "AF49FBAE3B337D263506E2D6BA13507E"),
        ] {
            let (source, expected) = raw_group_fixture(false, &[(offset, value)], checksum);
            let mut image = Icf::parse(&source).unwrap();
            assert!(matches!(
                image.raw_memory_group_name(descriptor),
                Err(Error::Unsupported { .. })
            ));
            assert_setting_state(&image, &source, &expected);
            assert!(matches!(
                image.set_raw_memory_group_name_ascii(descriptor, "VALID"),
                Err(Error::Unsupported { .. })
            ));
            assert_setting_state(&image, &source, &expected);
            assert_eq!(image.to_bytes(), source);
        }

        let (source, mut expected) =
            raw_group_fixture(false, &[], "E1C1750FBE655BA68E44B82F8F3D6654");
        let mut image = Icf::parse(&source).unwrap();
        image.set_raw_memory_group_name_ascii(0, "RENAMED").unwrap();
        expected[0x355c3..0x355d3].copy_from_slice(b"RENAMED         ");
        let serialized = image.to_bytes();
        for (descriptor, name) in [
            (1, "VALID"),
            (100, "VALID"),
            (usize::MAX, "VALID"),
            (0, ""),
            (0, "ABCDEFGHIJKLMNOPQ"),
            (0, "a"),
            (0, "@"),
            (0, "["),
            (0, "/"),
            (0, ":"),
            (0, "A B"),
            (0, "A\0B"),
            (0, "A\nB"),
            (0, "\u{7f}"),
            (99, "\u{ff21}"),
            (99, "AAAAAAAAAAAAAA\u{e9}"),
            (99, "AAAAAAAAAAAAAAA\u{e9}"),
            (usize::MAX, ""),
        ] {
            assert!(matches!(
                image.set_raw_memory_group_name_ascii(descriptor, name),
                Err(Error::Unsupported { .. })
            ));
            assert_setting_state(&image, &source, &expected);
        }
        for descriptor in [100, usize::MAX] {
            assert!(matches!(
                image.raw_memory_group_name(descriptor),
                Err(Error::Unsupported { line: 5, .. })
            ));
            assert_setting_state(&image, &source, &expected);
        }
        assert_eq!(image.to_bytes(), serialized);
        image
            .set_raw_memory_group_name_ascii(0, "RETRY_OK")
            .unwrap();
        expected[0x355c3..0x355d3].copy_from_slice(b"RETRY_OK        ");
        assert_setting_state(&image, &source, &expected);
        assert_eq!(
            image.raw_memory_group_name(0).unwrap(),
            Some(b"RETRY_OK        ".as_slice())
        );
    }
}
