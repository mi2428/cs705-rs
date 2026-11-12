//! Strict parsing and serialization of canonical IC-705 MapRev 6 ICF images.
//! Explicit strict MapRev 5-to-6 byte export is available separately from parsing.
//!
//! Unknown layouts are rejected, not repaired. Uninterpreted data is preserved;
//! a successful parse is not authorization to write the image to a radio.
//!
//! Canonical records use ASCII hex for this decoded byte sequence:
//!
//! ```text
//! address: 4 bytes, big-endian | count: 1 byte | payload: count bytes
//! ```
//!
//! Addresses are image byte offsets, not file offsets or typed row indices. CD
//! hashes the six-byte seed followed by complete decoded records, including each
//! address and count. Comments and metadata are outside that digest. The native
//! reader in [`text`] deliberately accepts a wider grammar and ignores CD; its
//! successful read still needs [`mod@format`]'s signature and exact-coverage gates.

use std::{borrow::Cow, fmt};

pub mod callsign_dialog;
mod common;
pub mod context;
pub mod document;
mod dsp;
pub mod format;
pub mod frequency;
mod gps;
mod groups;
mod image;
mod memory;
mod messages;
pub mod normalizer;
pub mod record;
pub mod record_image;
mod repeaters;
pub mod rows;
pub mod settings;
pub mod stage9;
pub mod text;

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
pub use common::{AudioFftAttenuator, AudioFftWaveform, AudioOscLevel, AudioOscSweepTime};
pub use common::{
    CwBreakIn, CwKeyType, CwMicKeyer, CwNumberStyle, CwRiseTime, DtmfSpeed, KeyPolarity,
    RttyFftAveraging, RttyLinefeed, RttyLogFormat, RttyLogTime, RttyMark, RttyShift,
};
pub use common::{
    ScopeAveraging, ScopeCenterDisplay, ScopeMarkerPosition, ScopeMaxHold, ScopeMode, ScopeSpan,
    ScopeSweepSpeed, ScopeVbw, ScopeWaterfallPeak, ScopeWaterfallSize, ScopeWaterfallSpeed,
    ScopeWaveform,
};
pub use dsp::{
    Agc, FilterBandGroup, FilterMode, FilterShape, FilterShapeSlot, FilterSlot, PbtSide, Preamp,
    StepAgcMode, ToneMode, TransmitBandwidthPreset, TransmitHigh, TransmitLow,
};
pub use gps::{GpsAlarm, GpsAltitude, GpsCoordinate, GpsDate, GpsGroup, GpsTime};
pub use groups::MemoryGroup;
pub use memory::special;
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
        reason: Cow<'static, str>,
    },
    /// The format or setting is outside the independently established subset.
    Unsupported {
        /// Location of the unsupported input.
        line: usize,
        /// The unsupported format property.
        reason: Cow<'static, str>,
    },
    /// The recorded checksum does not match the decoded records.
    ChecksumMismatch {
        /// Location of the checksum field.
        line: usize,
    },
    /// An explicitly supplied operation failed; preceding changes remain stored.
    Operation(Cow<'static, str>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { line, reason } => write!(f, "invalid ICF at line {line}: {reason}"),
            Self::Unsupported { line, reason } => {
                write!(f, "unsupported ICF at line {line}: {reason}")
            }
            Self::ChecksumMismatch { line } => write!(f, "ICF checksum mismatch at line {line}"),
            Self::Operation(reason) => write!(f, "ICF operation failed: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

/// A verified canonical image, borrowing or owning its unchanged source text.
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
#[derive(Clone)]
pub struct Icf<'a> {
    source: Cow<'a, [u8]>,
    memory: Vec<u8>,
    comment: Cow<'a, [u8]>,
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
        source: Cow::Borrowed(source),
        memory,
        comment: Cow::Borrowed(comment),
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
            source: Cow::Borrowed(source),
            memory,
            comment: Cow::Borrowed(comment),
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
                reason: "model or file revision".into(),
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
                reason: "missing metadata".into(),
            })?;
            metadata[index] = text.strip_prefix(prefix).ok_or(Error::Invalid {
                line: number,
                reason: "unexpected metadata field or order".into(),
            })?;
        }
        if metadata[1] != [b'0' + revision] {
            return Err(Error::Unsupported {
                line: 3,
                reason: if revision == 5 {
                    "MapRev other than 5".into()
                } else {
                    "MapRev other than 6".into()
                },
            });
        }
        if metadata[0].iter().any(|byte| matches!(byte, 0 | b'\r')) {
            return Err(Error::Unsupported {
                line: 2,
                reason: "NUL or embedded carriage return in comment".into(),
            });
        }
        let extra_data = u32::from_be_bytes(hex::<4>(metadata[2]).ok_or(Error::Invalid {
            line: 4,
            reason: "EtcData must contain eight hexadecimal characters".into(),
        })?);
        // EtcData callback RVA 0x25AA4 -> 0x56680: bits 0, 3 and 6 encode
        // the population count of the other 29 bits modulo eight, low bit first.
        let check = (extra_data & 1) | ((extra_data >> 3 & 1) << 1) | ((extra_data >> 6 & 1) << 2);
        if check != (extra_data & !0x49).count_ones() % 8 {
            return Err(Error::Invalid {
                line: 4,
                reason: "EtcData integrity bits".into(),
            });
        }

        let mut memory = Vec::with_capacity(IMAGE_BYTES);
        let mut digest = md5::Context::new();
        digest.consume(SEED);
        while let Some((number, text)) = lines.next() {
            if let Some(checksum) = text.strip_prefix(b"#CD=") {
                let expected = hex::<16>(checksum).ok_or(Error::Invalid {
                    line: number,
                    reason: "CD must contain 32 hexadecimal characters".into(),
                })?;
                if memory.len() != IMAGE_BYTES {
                    return Err(Error::Invalid {
                        line: number,
                        reason: "incomplete image".into(),
                    });
                }
                if digest.finalize().0 != expected {
                    return Err(Error::ChecksumMismatch { line: number });
                }
                if lines.next().is_some() {
                    return Err(Error::Invalid {
                        line: number + 1,
                        reason: "data after checksum".into(),
                    });
                }
                if &memory[IMAGE_BYTES - IMAGE_SIGNATURE.len()..] != IMAGE_SIGNATURE {
                    return Err(Error::Invalid {
                        line: 5 + (IMAGE_BYTES - IMAGE_SIGNATURE.len()) / RECORD_BYTES,
                        reason: "image signature".into(),
                    });
                }
                // The original reader allows internal revisions 1..6, not
                // necessarily equal to metadata MapRev. Each entry point here
                // requires its exact internal revision; do not repair it.
                if memory[INTERNAL_MAP_REV] != revision {
                    return Err(Error::Unsupported {
                        line: 5 + INTERNAL_MAP_REV / RECORD_BYTES,
                        reason: if revision == 5 {
                            "internal MapRev other than 5".into()
                        } else {
                            "internal MapRev other than 6".into()
                        },
                    });
                }
                return Ok((memory, metadata[0], extra_data));
            }
            let header = text.get(..10).and_then(hex::<5>).ok_or(Error::Invalid {
                line: number,
                reason: "invalid record header".into(),
            })?;
            let count = usize::from(header[4]);
            if text.len() != 10 + count * 2 {
                return Err(Error::Invalid {
                    line: number,
                    reason: "record length mismatch".into(),
                });
            }
            if count != RECORD_BYTES {
                return Err(Error::Unsupported {
                    line: number,
                    reason: "record size other than 32".into(),
                });
            }
            let record = hex::<37>(text).ok_or(Error::Invalid {
                line: number,
                reason: "invalid hexadecimal payload".into(),
            })?;
            let address = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
            if memory.len() >= IMAGE_BYTES || address != memory.len() as u32 {
                return Err(Error::Invalid {
                    line: number,
                    reason: "address gap, overlap or excess data".into(),
                });
            }
            digest.consume(record);
            memory.extend_from_slice(&record[5..]);
        }
        Err(Error::Invalid {
            line: source.split(|byte| *byte == b'\n').count(),
            reason: "missing checksum".into(),
        })
    }

    /// Own the original source and comment without serializing current edits.
    pub fn into_owned(self) -> Icf<'static> {
        Icf {
            source: Cow::Owned(self.source.into_owned()),
            memory: self.memory,
            comment: Cow::Owned(self.comment.into_owned()),
            extra_data: self.extra_data,
        }
    }

    /// Return every current memory byte, including uninterpreted regions, in order.
    pub fn memory(&self) -> &[u8] {
        &self.memory
    }

    /// Return the exact original file, without normalization or subsequent edits.
    pub fn original_bytes(&self) -> &[u8] {
        &self.source
    }

    /// Serialize the verified image using uppercase hexadecimal and CRLF lines.
    ///
    /// All memory, comment and EtcData bytes are preserved. Unlike
    /// [`Self::original_bytes`], this normalizes the text and recomputes CD.
    /// It performs no I/O and does not establish radio compatibility or safety.
    pub fn to_bytes(&self) -> Vec<u8> {
        format::write_storage(
            &self.memory,
            format::Layout::from_map_revision(6),
            &self.comment,
            self.extra_data,
        )
        .expect("Icf always owns an exact MapRev 6 storage image")
    }

    /// Return the raw comment bytes without assuming UTF-8 or changing CP932 data.
    pub fn comment_bytes(&self) -> &[u8] {
        &self.comment
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
                reason: "normal physical memory slot outside 0..500".into(),
            });
        }
        if self.memory[MEMORY_EMPTY_FLAGS + slot] & 1 != 0 {
            return Ok(None);
        }
        let offset = slot * MEMORY_CHANNEL_BYTES + MEMORY_NAME_OFFSET;
        Ok(Some(&self.memory[offset..offset + MEMORY_NAME_BYTES]))
    }

    /// Apply the normal name editor's bounded byte normalization to an occupied
    /// physical slot. Empty names are legal; `None` means cancellation.
    ///
    /// The shared input is sanitized before candidate trimming/clipping. This is
    /// the name text/storage component, not the complete decoded side re-encode
    /// or empty logical-row pending-name operation.
    pub fn edit_physical_memory_name(
        &mut self,
        slot: usize,
        input: Option<&mut [u8]>,
    ) -> Result<bool, Error> {
        if self.physical_memory_name_raw(slot)?.is_none() {
            return Err(Error::Unsupported {
                line: 5 + (MEMORY_EMPTY_FLAGS + slot) / RECORD_BYTES,
                reason: "physical memory slot is empty".into(),
            });
        }
        let Some(input) = input else {
            return Ok(false);
        };
        let candidate = normalizer::normalize_name(input, MEMORY_NAME_BYTES);
        let at = slot * MEMORY_CHANNEL_BYTES + MEMORY_NAME_OFFSET;
        self.memory[at..at + MEMORY_NAME_BYTES].fill(b' ');
        self.memory[at..at + candidate.len()].copy_from_slice(&candidate);
        Ok(true)
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
                reason: "cannot rename an empty physical memory slot".into(),
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
                reason: "memory name requires 1..=16 bytes in A-Z, 0-9, _ or -".into(),
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
                reason: "raw memory group descriptor outside 0..100".into(),
            });
        }
        self.raw_memory_group_order()?;
        let base = RAW_MEMORY_GROUPS + RAW_MEMORY_GROUP_BYTES * descriptor;
        let head = u16::from_le_bytes([self.memory[base + 1], self.memory[base + 2]]);
        if head != u16::MAX && usize::from(head) >= MEMORY_CHANNEL_COUNT {
            return Err(Error::Unsupported {
                line: 5 + (base + 1) / RECORD_BYTES,
                reason: "raw memory group head outside physical slots or FFFF".into(),
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
                    reason: "raw memory group order must visit all 100 descriptors once".into(),
                });
            }
            seen[next] = true;
            *descriptor = next;
            link = RAW_MEMORY_GROUPS + RAW_MEMORY_GROUP_BYTES * next;
        }
        if self.memory[link] != 0xff {
            return Err(Error::Unsupported {
                line: 5 + link / RECORD_BYTES,
                reason: "raw memory group order must end with FF after 100 descriptors".into(),
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
                reason: "cannot rename an empty raw memory group descriptor".into(),
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
                reason: "raw memory group name requires 1..=16 bytes in A-Z, 0-9, _ or -".into(),
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
mod tests;
