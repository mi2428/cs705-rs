//! Shared raw/decoded record codecs from the CS-705 1.30 holder boundary.
//!
//! The lookup helpers deliberately do not use the strict public radio enums.
//! RVAs 0x8998/0x89C8 scan ordered (stored, decoded) byte pairs, stop at the
//! first match, and return AL=FF on a miss. The six wrappers below truncate
//! their input to AL; callers separately decide when a full DWORD gates a
//! mode-specific field. Decoded bytes are zero-extended here, never sign-extended.

use super::Error;
use super::memory::{pack_callsign, unpack_callsign};

/// Original holder size, including its 12-byte binding header.
pub const DECODED_BYTES: usize = 616;
/// The body begins with flags at +0C and includes all 604 trailing bytes.
pub const BODY_BYTES: usize = DECODED_BYTES - 12;

/// Exact decoded holder bytes, not a fresh default record or an image pointer.
///
/// ```text
/// +00 type DWORD | +04 index DWORD | +08 binding token DWORD
/// +0C flags ... +20 group ... +3C frequency ... +58 mode
/// +5C DATA | +60 filter ... +6C TX tone | +70 RX tone ... +267
/// <-------------- header 12 ------><------ body 604 -------->
/// ```
///
/// The binding word is an opaque original 32-bit token: it is never dereferenced
/// as a host pointer. Construction requires every incoming byte. Loaders replace
/// only their documented windows, keeping residual flags, fields and headers.
#[derive(Clone, PartialEq, Eq)]
pub struct DecodedRecord {
    bytes: [u8; DECODED_BYTES],
}

/// Common channel DWORD members not already named by the stage9 accessors.
/// These are decoded fields, NOT ICF byte offsets or strict radio enums.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelField {
    FrequencyHz,
    Duplex,
    Selection,
    Tone,
    Dtcs,
    Polarity,
    DigitalSquelch,
    DigitalCode,
}

/// Decoded repeater-only DWORDs, separate from common RF channel mode/tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeaterField {
    Mode,
    Tone,
    ToneIndex,
    Use,
    Position,
    UtcOffset,
    GatewayIndex,
}
impl RepeaterField {
    fn offset(self) -> usize {
        match self {
            Self::Mode => 0x15c,
            Self::Tone => 0x174,
            Self::ToneIndex => 0x178,
            Self::Use => 0x17c,
            Self::Position => 0x180,
            Self::UtcOffset => 0x184,
            Self::GatewayIndex => 0x188,
        }
    }
}
/// Eight-byte repeater text members. No ACP editor behavior is implied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeaterText {
    SubName,
    Call,
    Gateway,
}
impl RepeaterText {
    fn offset(self) -> usize {
        match self {
            Self::SubName => 0x154,
            Self::Call => 0x160,
            Self::Gateway => 0x168,
        }
    }
}
impl ChannelField {
    fn offset(self) -> usize {
        match self {
            Self::FrequencyHz => 0x3c,
            Self::Duplex => 0x44,
            Self::Selection => 0x54,
            Self::Tone => 0x68,
            Self::Dtcs => 0x74,
            Self::Polarity => 0x78,
            Self::DigitalSquelch => 0x80,
            Self::DigitalCode => 0x84,
        }
    }
}

impl DecodedRecord {
    pub fn history_call(&self) -> &[u8; 8] {
        self.bytes[0x18c..0x194]
            .try_into()
            .expect("fixed history call")
    }
    pub fn history_supplemented(&self) -> bool {
        self.bytes[0x194] != 0
    }
    pub fn repeater_field(&self, field: RepeaterField) -> u32 {
        self.word(field.offset())
    }
    pub fn set_repeater_field(&mut self, field: RepeaterField, value: u32) {
        self.put_word(field.offset(), value);
    }
    pub fn repeater_text(&self, field: RepeaterText) -> &[u8; 8] {
        let at = field.offset();
        self.bytes[at..at + 8]
            .try_into()
            .expect("fixed repeater text")
    }
    pub fn set_repeater_text(&mut self, field: RepeaterText, value: &[u8; 8]) {
        let at = field.offset();
        self.bytes[at..at + 8].copy_from_slice(value);
    }
    pub fn coordinate_bytes(&self, longitude: bool) -> &[u8; 4] {
        let at = if longitude { 0x25e } else { 0x25a };
        self.bytes[at..at + 4]
            .try_into()
            .expect("fixed decoded coordinate")
    }
    pub fn set_coordinate_bytes(&mut self, longitude: bool, value: &[u8; 4]) {
        let at = if longitude { 0x25e } else { 0x25a };
        self.bytes[at..at + 4].copy_from_slice(value);
    }
    /// Adopt caller-supplied holder bytes without normalization or zero filling.
    pub fn from_bytes(bytes: [u8; DECODED_BYTES]) -> Self {
        Self { bytes }
    }
    /// Borrow all header/body bytes, including unknown stored values.
    pub fn as_bytes(&self) -> &[u8; DECODED_BYTES] {
        &self.bytes
    }
    /// Mutate decoded scratch bytes, not an unchecked mutable ICF image window.
    pub fn as_mut_bytes(&mut self) -> &mut [u8; DECODED_BYTES] {
        &mut self.bytes
    }
    /// Copy the original 604-byte body while preserving all three header words.
    pub fn copy_body_from(&mut self, source: &Self) {
        self.bytes[12..].copy_from_slice(&source.bytes[12..]);
    }
    /// Original type DWORD, not a validated Rust enum.
    pub fn kind(&self) -> u32 {
        self.word(0)
    }
    /// Original index bits; negative source indices remain their two's complement.
    pub fn index(&self) -> u32 {
        self.word(4)
    }
    /// Original opaque image-binding token, never a dereferenceable host pointer.
    pub fn binding(&self) -> u32 {
        self.word(8)
    }
    /// Replace the explicit binding header without touching its 604-byte body.
    pub fn set_header(&mut self, kind: u32, index: u32, binding: u32) {
        self.put_word(0, kind);
        self.put_word(4, index);
        self.put_word(8, binding);
    }
    /// Validity is flags bit zero, not a test that every decoded field is known.
    pub fn valid(&self) -> bool {
        self.bytes[12] & 1 != 0
    }
    /// Replace only validity bit zero, preserving the other flag bits/bytes.
    pub fn set_valid(&mut self, valid: bool) {
        self.bytes[12] = self.bytes[12] & !1 | u8::from(valid);
    }
    /// Logical group DWORD supplied by the owning dataset adapter.
    pub fn group(&self) -> u32 {
        self.word(0x20)
    }
    /// Replace only the logical group, not type/index/binding or physical order.
    pub fn set_group(&mut self, group: u32) {
        self.put_word(0x20, group);
    }
    /// Full decoded mode DWORD. Lookup truncation and mode guards are separate.
    pub fn mode(&self) -> u32 {
        self.word(0x58)
    }
    /// Replace decoded mode without applying a radio/UI mode transition.
    pub fn set_mode(&mut self, mode: u32) {
        self.put_word(0x58, mode);
    }
    /// Decoded DATA DWORD, not an already-masked physical bit.
    pub fn data(&self) -> u32 {
        self.word(0x5c)
    }
    /// Replace decoded DATA without changing mode/filter or other cached fields.
    pub fn set_data(&mut self, data: u32) {
        self.put_word(0x5c, data);
    }
    /// Decoded filter DWORD, including FF and other unknown codes.
    pub fn filter(&self) -> u32 {
        self.word(0x60)
    }
    /// Replace only the decoded filter field.
    pub fn set_filter(&mut self, filter: u32) {
        self.put_word(0x60, filter);
    }
    /// Unclamped decoded offset in Hz; record-specific savers apply their rules.
    pub fn offset_hz(&self) -> u32 {
        self.word(0x48)
    }
    /// Replace decoded offset; this is not a source input-editor quantization.
    pub fn set_offset_hz(&mut self, hz: u32) {
        self.put_word(0x48, hz);
    }
    /// Decoded TX tone selector/code, not inferred physical Hertz.
    pub fn tx_tone(&self) -> u32 {
        self.word(0x6c)
    }
    /// Replace only the decoded TX tone selector/code.
    pub fn set_tx_tone(&mut self, tone: u32) {
        self.put_word(0x6c, tone);
    }
    /// Decoded RX tone selector/code, not inferred physical Hertz.
    pub fn rx_tone(&self) -> u32 {
        self.word(0x70)
    }
    /// Replace only the decoded RX tone selector/code.
    pub fn set_rx_tone(&mut self, tone: u32) {
        self.put_word(0x70, tone);
    }

    /// Read a common decoded DWORD without narrowing unknown values.
    pub fn channel_field(&self, field: ChannelField) -> u32 {
        self.word(field.offset())
    }
    /// Set only a decoded scratch member; native saving/normalization is separate.
    pub fn set_channel_field(&mut self, field: ChannelField, value: u32) {
        self.put_word(field.offset(), value);
    }
    /// Common fixed name, retaining embedded NUL/high bytes from the source.
    pub fn name(&self) -> &[u8; 16] {
        self.bytes[0x26..0x36]
            .try_into()
            .expect("fixed holder name")
    }
    /// Replace the accepted fixed-width decoded name, not an ACP editor operation.
    pub fn set_name(&mut self, name: &[u8; 16]) {
        self.bytes[0x26..0x36].copy_from_slice(name);
    }
    /// Original decoded side byte, independent of a later save destination.
    pub fn side(&self) -> u8 {
        self.bytes[0x24]
    }
    /// Original decoded split byte, before the native saver's low-bit projection.
    pub fn split(&self) -> u8 {
        self.bytes[0x25]
    }
    /// Replace only decoded split; synchronization/notifications remain external.
    pub fn set_split(&mut self, split: u8) {
        self.bytes[0x25] = split;
    }
    /// Common decoded callsign bytes; native packing drops high bits on save.
    pub fn callsign(&self, which: super::MemoryCallsign) -> &[u8; 8] {
        let at = match which {
            super::MemoryCallsign::Your => 0x88,
            super::MemoryCallsign::Repeater1 => 0xa4,
            super::MemoryCallsign::Repeater2 => 0xc4,
        };
        self.bytes[at..at + 8]
            .try_into()
            .expect("fixed holder callsign")
    }
    /// Replace one accepted decoded callsign, without adding text postprocessing.
    pub fn set_callsign(&mut self, which: super::MemoryCallsign, call: &[u8; 8]) {
        let at = match which {
            super::MemoryCallsign::Your => 0x88,
            super::MemoryCallsign::Repeater1 => 0xa4,
            super::MemoryCallsign::Repeater2 => 0xc4,
        };
        self.bytes[at..at + 8].copy_from_slice(call);
    }

    fn word(&self, at: usize) -> u32 {
        u32::from_le_bytes(
            self.bytes[at..at + 4]
                .try_into()
                .expect("fixed holder DWORD"),
        )
    }
    fn put_word(&mut self, at: usize, value: u32) {
        self.bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
}

/// Original 5F64 reset followed by 610C's explicit kind/index assignments.
///
/// Unlike the lower in-place loaders, 5F64 DOES zero all 616 bytes, restore the
/// binding, set valid, fill its exact text windows, then set group=-1, tones=8,
/// polarity=2 and UR="CQCQCQ  ". Those are instruction/string constants, not a
/// generated G image or a guessed factory source. No clock/image is consulted.
pub fn reset_holder(kind: u32, index: u32, binding: u32) -> DecodedRecord {
    let mut holder = DecodedRecord::from_bytes([0; DECODED_BYTES]);
    holder.set_header(kind, index, binding);
    holder.set_valid(true);
    for (at, count) in [
        (0x0d, 16),
        (0x26, 16),
        (0x88, 8),
        (0x90, 16),
        (0xa4, 8),
        (0xac, 17),
        (0xc4, 8),
        (0xcc, 17),
        (0x244, 16),
        (0x154, 8),
        (0x160, 8),
        (0x168, 8),
        (0x18c, 8),
    ] {
        holder.bytes[at..at + count].fill(b' ');
    }
    holder.set_group(u32::MAX);
    holder.set_tx_tone(8);
    holder.set_rx_tone(8);
    holder.put_word(0x78, 2);
    holder.bytes[0x88..0x90].copy_from_slice(b"CQCQCQ  ");
    holder
}

/// Read a canonical document's existing normal/call channel through the one
/// working-map adapter. This does not initialize an empty row or run an editor.
pub fn load_channel_image_into(
    image: &super::Icf<'_>,
    kind: super::record_image::ChannelKind,
    index: u32,
    holder: &mut DecodedRecord,
) -> Result<bool, Error> {
    if kind == super::record_image::ChannelKind::Normal {
        let model = super::rows::NormalMemory::from_icf(image)?;
        super::record_image::load_channel_into(model.working(), kind, index, holder)
    } else {
        super::record_image::load_channel_into(image.memory(), kind, index, holder)
    }
}

/// Save to an EXISTING mapped normal/call side while retaining the ICF's comment,
/// EtcData and source bytes. The existing record_image codec supplies every
/// address/store; no map or raw83 equation is duplicated. Each native store is
/// immediately committed via Icf's checked writer, preserving a failing prefix.
/// Allocation/release/reference/GUI effects are separate operation boundaries.
pub fn store_channel_image_into(
    image: &mut super::Icf<'_>,
    kind: super::record_image::ChannelKind,
    index: u32,
    holder: &mut DecodedRecord,
) -> Result<(), Error> {
    let mut working = if kind == super::record_image::ChannelKind::Normal {
        super::rows::NormalMemory::from_icf(image)?
            .working()
            .to_vec()
    } else {
        image.memory().to_vec()
    };
    let address =
        super::record_image::channel_address(&working, kind, index)?.ok_or(Error::Invalid {
            line: 5,
            reason: "channel destination has no allocated physical slot",
        })?;
    super::record_image::store_channel_into(
        &mut working,
        kind,
        index,
        holder,
        |write| match write {
            ChannelWrite::Body { offset, bytes, .. } => {
                image.patch_memory(&[(address.record_offset + offset, bytes)])
            }
            ChannelWrite::Flag { byte, .. } => {
                image.patch_memory(&[(address.flag_offset.expect("normal flag event"), &[*byte])])
            }
        },
    )
}

/// Actual569FC/10420 producer for type22. Q selector0 enables5MHz; selector16
/// uses SYSTEM LANGID's Japanese primary ID.10420 uses Context::source_selector
/// and569C8: selector1 picks NW, every other reachable selector picks NE.
/// The immutable Q/LANGID pair is an explicit command snapshot, not host locale
/// or inferred ICF metadata. No G image or clock is generated by this helper.
pub fn repeater_default_context(
    context: &super::context::Context,
    system_langid: Option<u16>,
) -> Result<crate::repeater::working::DefaultContext, Error> {
    crate::repeater::working::DefaultContext::from_context(context, system_langid)
}

/// Read one ACTUAL captured G/global default word only at the reset callback.
/// Do not pre-read both words, repair an invalid old reference or substitute0.
pub fn repeater_reset_word(generation: &[u8], which: usize) -> Result<u16, Error> {
    let at = match which {
        0 => 0x3c8b9,
        1 => 0x3c903,
        _ => {
            return Err(Error::Invalid {
                line: 5,
                reason: "unknown repeater global reset word",
            });
        }
    };
    let bytes = generation.get(at..at + 2).ok_or(Error::Invalid {
        line: 5,
        reason: "repeater global reset source unavailable",
    })?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

const MODE: [(u8, u8); 11] = [
    (0, 0),
    (1, 1),
    (2, 2),
    (3, 3),
    (4, 6),
    (5, 7),
    (6, 8),
    (7, 13),
    (8, 15),
    (9, 16),
    (255, 25),
];
const TONE: [(u8, u8); 8] = [
    (0, 0),
    (1, 1),
    (2, 2),
    (3, 4),
    (4, 6),
    (5, 7),
    (6, 8),
    (7, 9),
];
const DIGITAL_SQUELCH: [(u8, u8); 3] = [(0, 0), (1, 2), (2, 4)];
const POLARITY: [(u8, u8); 4] = [(0, 2), (1, 3), (2, 4), (3, 5)];
// 8AE8/8AD0: repeater type, NOT the polarity table at 19047E.
const REPEATER_TYPE: [(u8, u8); 4] = [(0, 2), (1, 0), (2, 1), (3, 3)];

fn lookup(pairs: &[(u8, u8)], value: u8, encode: bool) -> u8 {
    for &(raw, decoded) in pairs {
        if (if encode { decoded } else { raw }) == value {
            return if encode { raw } else { decoded };
        }
    }
    0xff
}

/// RVA 0x8A40: stored mode byte to zero-extended decoded code. A stored FF
/// is a real table entry producing 25; other unrecognized bytes produce 255.
pub fn decode_mode(raw: u8) -> u32 {
    u32::from(lookup(&MODE, raw, false))
}

/// RVA 0x8A28: encode only the decoded DWORD's low byte, returning FF on a
/// miss. This does not apply a destination nibble mask or a mode/DATA guard.
pub fn encode_mode(decoded: u32) -> u8 {
    lookup(&MODE, decoded as u8, true)
}

/// RVA 0x8A70: stored tone byte to decoded sparse code, or zero-extended FF.
pub fn decode_tone(raw: u8) -> u32 {
    u32::from(lookup(&TONE, raw, false))
}

/// RVA 0x8A58: low-byte decoded tone lookup; full-DWORD mode gates belong to
/// the enclosing record saver, not this ordered first-match byte helper.
pub fn encode_tone(decoded: u32) -> u8 {
    lookup(&TONE, decoded as u8, true)
}

/// RVA 0x8AB8: stored digital-squelch byte to 0/2/4, or zero-extended FF.
pub fn decode_digital_squelch(raw: u8) -> u32 {
    u32::from(lookup(&DIGITAL_SQUELCH, raw, false))
}

/// RVA 0x8AA0: low-byte decoded digital-squelch code to its stored byte.
/// Unknown codes are FF, not an error or an implicit OFF/default value.
pub fn encode_digital_squelch(decoded: u32) -> u8 {
    lookup(&DIGITAL_SQUELCH, decoded as u8, true)
}

fn channel_side(side: usize) -> Result<usize, Error> {
    if side < 2 {
        Ok(1 + 33 * side)
    } else {
        Err(Error::Invalid {
            line: 5,
            reason: "channel side must be zero or one",
        })
    }
}

/// Load one normal/call side (RVA 0x62F0..0x6546), not the outer 610C reset.
///
/// `side` selects raw83's +1/+34 window. Some(flag) supplies the normal slot's
/// SEL bits; None selects call semantics (SEL=0). Allocation, validity, logical
/// group and source-pointer metadata remain the owning adapter's responsibility.
/// The binding header and every unrelated holder byte survive this in-place load.
/// Side33 is NOT the compact36 representation used by band stack/memopad.
pub fn decode_channel_into(
    holder: &mut DecodedRecord,
    raw: &[u8; 83],
    side: usize,
    normal_flag: Option<u8>,
) -> Result<(), Error> {
    let at = channel_side(side)?;
    holder.bytes[0x24] = side as u8;
    holder.bytes[0x26..0x36].copy_from_slice(&raw[67..83]);
    holder.put_word(0x54, u32::from(normal_flag.map_or(0, |b| b >> 6)));
    holder.bytes[0x25] = raw[0] >> 1 & 1;
    let source = &raw[at..at + 33];
    holder.bytes[0x3c..0x40].copy_from_slice(&source[..4]);
    let mode = decode_mode(source[4] >> 4);
    holder.set_mode(mode);
    holder.set_filter(u32::from(source[4] & 15));
    holder.set_data(u32::from(source[5] & 1));
    let duplex = source[5] >> 1 & 3;
    holder.put_word(
        0x44,
        if mode == 17 && matches!(duplex, 1 | 2) {
            u32::from(duplex) + 2
        } else {
            u32::from(duplex)
        },
    );
    holder.bytes[0x48..0x4c].copy_from_slice(&source[29..33]);
    match mode {
        13 | 14 => {
            holder.put_word(0x68, decode_tone(source[6]));
            holder.set_tx_tone(u32::from(source[9]));
            holder.set_rx_tone(u32::from(source[10]));
            holder.put_word(0x74, u32::from(source[11]));
            let polarity = source[7].wrapping_add(source[8].wrapping_mul(2));
            holder.put_word(0x78, u32::from(lookup(&POLARITY, polarity, false)));
        }
        16 | 17 => {
            if mode == 16 {
                holder.put_word(0x80, decode_digital_squelch(source[6]));
                holder.put_word(0x84, u32::from(source[7]));
            }
            for (from, to) in [(8, 0x88), (15, 0xa4), (22, 0xc4)] {
                holder.bytes[to..to + 8].copy_from_slice(&unpack_callsign(&source[from..from + 7]));
            }
        }
        _ => {}
    }
    Ok(())
}

/// Native normal/call stores. Repeated bitfield writes are separate stages;
/// combining Mode/Filter or DATA/Duplex would erase observable failure prefixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelStage {
    Empty,
    Name,
    Select,
    Split,
    Frequency,
    Mode,
    Data,
    Filter,
    Duplex,
    Offset,
    Tone,
    TxTone,
    RxTone,
    Dtcs,
    TxPolarity,
    RxPolarity,
    DigitalSquelch,
    DigitalCode,
    YourCall,
    Repeater1,
    Repeater2,
    CorrectRepeater1,
}

/// Immediate native store. Body offsets are raw83-relative, never holder-side
/// relative. Flag is the separate normal physical slot byte; call emits none.
pub enum ChannelWrite<'a> {
    Body {
        stage: ChannelStage,
        offset: usize,
        bytes: &'a [u8],
    },
    Flag {
        stage: ChannelStage,
        byte: u8,
    },
}
impl ChannelWrite<'_> {
    /// Source stage of this one immediate store.
    pub fn stage(&self) -> ChannelStage {
        match self {
            Self::Body { stage, .. } | Self::Flag { stage, .. } => *stage,
        }
    }
}

fn channel_body(
    raw: &mut [u8; 83],
    stage: ChannelStage,
    at: usize,
    bytes: &[u8],
    write: &mut impl FnMut(ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    raw[at..at + bytes.len()].copy_from_slice(bytes);
    write(ChannelWrite::Body {
        stage,
        offset: at,
        bytes: &raw[at..at + bytes.len()],
    })
}

/// Save normal/call raw83 in RVA 0x7836..0x7B18 order, preserving target bytes.
///
/// The caller supplies the DESTINATION side; holder+24 is source metadata and
/// is deliberately not consulted. Some(flag) is the old normal physical flag:
/// Empty is written before valid0 exits. None denotes call and valid0 writes
/// nothing. Both raw and flag are scratch including the attempted failed store;
/// each successful callback must immediately commit its one image window.
///
/// Mode-specific branches use mode's low byte, DUP3/4 and polarity use the full
/// DWORD. Offset clamps a local unsigned copy, not holder+48. The only holder
/// mutation is native post-packed7 RPT1 decode/G correction when RPT2 is not
/// exactly eight ASCII spaces. A later error never rolls back a prior store or
/// that holder correction. This function performs no reload or UI transition.
pub fn encode_channel_into(
    holder: &mut DecodedRecord,
    raw: &mut [u8; 83],
    side: usize,
    mut normal_flag: Option<&mut u8>,
    mut write: impl FnMut(ChannelWrite<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    use ChannelStage as S;
    let at = channel_side(side)?;
    if let Some(flag) = normal_flag.as_deref_mut() {
        *flag = (*flag & !1) | u8::from(!holder.valid());
        write(ChannelWrite::Flag {
            stage: S::Empty,
            byte: *flag,
        })?;
    }
    if !holder.valid() {
        return Ok(());
    }
    channel_body(raw, S::Name, 67, &holder.bytes[0x26..0x36], &mut write)?;
    if let Some(flag) = normal_flag {
        *flag = (*flag & 0x3f) | ((holder.bytes[0x54] & 3) << 6);
        write(ChannelWrite::Flag {
            stage: S::Select,
            byte: *flag,
        })?;
    }
    let split = (raw[0] & !2) | ((holder.bytes[0x25] & 1) << 1);
    channel_body(raw, S::Split, 0, &[split], &mut write)?;
    channel_body(raw, S::Frequency, at, &holder.bytes[0x3c..0x40], &mut write)?;
    let mode = (raw[at + 4] & 15) | ((encode_mode(holder.mode()) & 15) << 4);
    channel_body(raw, S::Mode, at + 4, &[mode], &mut write)?;
    let data = (raw[at + 5] & !1) | (holder.bytes[0x5c] & 1);
    channel_body(raw, S::Data, at + 5, &[data], &mut write)?;
    let filter = (raw[at + 4] & 0xf0) | (holder.bytes[0x60] & 15);
    channel_body(raw, S::Filter, at + 4, &[filter], &mut write)?;
    let duplex = match holder.word(0x44) {
        3 => 1,
        4 => 2,
        other => other,
    };
    let duplex = (raw[at + 5] & !6) | (((duplex as u8) & 3) << 1);
    channel_body(raw, S::Duplex, at + 5, &[duplex], &mut write)?;
    let offset = holder.offset_hz().min(9_999_900).to_le_bytes();
    channel_body(raw, S::Offset, at + 29, &offset, &mut write)?;
    match holder.mode() as u8 {
        13 | 14 => {
            channel_body(
                raw,
                S::Tone,
                at + 6,
                &[encode_tone(holder.word(0x68))],
                &mut write,
            )?;
            channel_body(raw, S::TxTone, at + 9, &[holder.bytes[0x6c]], &mut write)?;
            channel_body(raw, S::RxTone, at + 10, &[holder.bytes[0x70]], &mut write)?;
            channel_body(raw, S::Dtcs, at + 11, &[holder.bytes[0x74]], &mut write)?;
            let (tx, rx) = match holder.word(0x78) {
                3 => (1, 0),
                4 => (0, 1),
                5 => (1, 1),
                _ => (0, 0),
            };
            channel_body(raw, S::TxPolarity, at + 7, &[tx], &mut write)?;
            channel_body(raw, S::RxPolarity, at + 8, &[rx], &mut write)?;
        }
        16 | 17 => {
            if holder.mode() as u8 == 16 {
                channel_body(
                    raw,
                    S::DigitalSquelch,
                    at + 6,
                    &[encode_digital_squelch(holder.word(0x80))],
                    &mut write,
                )?;
                channel_body(
                    raw,
                    S::DigitalCode,
                    at + 7,
                    &[holder.bytes[0x84]],
                    &mut write,
                )?;
            }
            for (stage, from, to) in [
                (S::YourCall, 0x88, 8),
                (S::Repeater1, 0xa4, 15),
                (S::Repeater2, 0xc4, 22),
            ] {
                let call: [u8; 8] = holder.bytes[from..from + 8]
                    .try_into()
                    .expect("fixed channel callsign");
                channel_body(
                    raw,
                    stage,
                    at + to,
                    &pack_callsign(&call.map(|b| b & 0x7f)),
                    &mut write,
                )?;
            }
            if holder.bytes[0xc4..0xcc] != [b' '; 8] {
                holder.bytes[0xa4..0xac].copy_from_slice(&unpack_callsign(&raw[at + 15..at + 22]));
                if holder.bytes[0xab] == b'G' {
                    holder.bytes[0xab] = b' ';
                    let call = holder.bytes[0xa4..0xac]
                        .try_into()
                        .expect("fixed corrected callsign");
                    channel_body(
                        raw,
                        S::CorrectRepeater1,
                        at + 15,
                        &pack_callsign(&call),
                        &mut write,
                    )?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Type0A's six raw34 fields (69BF..6A95), without order/group lookup or reset.
/// Date/time/coordinates are opaque stored bytes: no host clock, BCD validator,
/// floating-point conversion or geographic bounds are added at this boundary.
/// The caller supplies existence/validity and the current logical group.
pub fn decode_gps_into(holder: &mut DecodedRecord, raw: &[u8; 34], group: u32) {
    holder.bytes[0x244..0x254].copy_from_slice(&raw[18..34]);
    holder.bytes[0x254..0x25a].copy_from_slice(&raw[8..14]);
    holder.bytes[0x25a..0x262].copy_from_slice(&raw[..8]);
    holder.bytes[0x264..0x268].copy_from_slice(&raw[14..18]);
    holder.set_group(group);
}

/// Type0A save order is name/date/time/latitude/longitude/altitude (7E31..7EE7).
/// Valid0 emits nothing, unlike the repeater and UR savers. Offsets are raw34
/// relative; callbacks commit immediately. Scratch includes a failed attempt,
/// but successful image writes are never rolled back after a later error.
pub fn encode_gps_into(
    holder: &DecodedRecord,
    raw: &mut [u8; 34],
    mut write: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    if !holder.valid() {
        return Ok(());
    }
    for (at, from, count) in [
        (18, 0x244, 16),
        (8, 0x254, 3),
        (11, 0x257, 3),
        (0, 0x25a, 4),
        (4, 0x25e, 4),
        (14, 0x264, 4),
    ] {
        raw[at..at + count].copy_from_slice(&holder.bytes[from..from + count]);
        write(at, &raw[at..at + count])?;
    }
    Ok(())
}

/// UR/type0D's 75CC in-place load. UR's call is holder+88, NOT the history call
/// at +18C. The arrays are separate raw16/raw8, not a packed7 or combined record.
/// Even inactive physical tail rows load both fields; count/validity is external.
pub fn decode_callsign_into(holder: &mut DecodedRecord, name: &[u8; 16], call: &[u8; 8]) {
    holder.bytes[0x26..0x36].copy_from_slice(name);
    holder.bytes[0x88..0x90].copy_from_slice(call);
}

/// UR's two separate immediate stores at 8072..80B2.
pub enum CallsignWrite<'a> {
    Name(&'a [u8; 16]),
    Call(&'a [u8; 8]),
}

/// Type0D always saves name then call, including valid0. A call failure leaves
/// the successful name store committed. No trim, casefold, NUL or high-bit rule
/// belongs to this fixed-width copy; accepted editor processing is upstream.
pub fn encode_callsign_into(
    holder: &DecodedRecord,
    mut write: impl FnMut(CallsignWrite<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    write(CallsignWrite::Name(
        holder.bytes[0x26..0x36].try_into().expect("fixed UR name"),
    ))?;
    write(CallsignWrite::Call(
        holder.bytes[0x88..0x90].try_into().expect("fixed UR call"),
    ))
}

/// Read the same raw UR list slot as CSV 8E37C, with no invented count/text
/// validity gate. A typed 610C caller supplies count-derived validity separately.
pub fn load_callsign_storage_into(
    image: &super::Icf<'_>,
    index: usize,
    holder: &mut DecodedRecord,
) -> Result<(), Error> {
    if index >= 300 {
        return Err(Error::Invalid {
            line: 5,
            reason: "UR slot outside 0..300",
        });
    }
    let name = image
        .memory()
        .get(0x2f703 + 16 * index..0x2f703 + 16 * (index + 1))
        .ok_or(Error::Invalid {
            line: 5,
            reason: "UR name window unavailable",
        })?;
    let call = image
        .memory()
        .get(0x2eda3 + 8 * index..0x2eda3 + 8 * (index + 1))
        .ok_or(Error::Invalid {
            line: 5,
            reason: "UR call window unavailable",
        })?;
    decode_callsign_into(
        holder,
        name.try_into().expect("checked UR name width"),
        call.try_into().expect("checked UR call width"),
    );
    Ok(())
}

/// Original UR saver always stores both fields, even when the typed holder is
/// invalid. Its two checked document stores preserve a later failure prefix.
pub fn store_callsign_storage_into(
    image: &mut super::Icf<'_>,
    index: usize,
    holder: &DecodedRecord,
) -> Result<(), Error> {
    if index >= 300 {
        return Err(Error::Invalid {
            line: 5,
            reason: "UR slot outside 0..300",
        });
    }
    encode_callsign_into(holder, |write| match write {
        CallsignWrite::Name(bytes) => image.patch_memory(&[(0x2f703 + 16 * index, bytes)]),
        CallsignWrite::Call(bytes) => image.patch_memory(&[(0x2eda3 + 8 * index, bytes)]),
    })
}

/// Raw8 pair for kind15 (7415) or kind16 (7462). Raw+0 maps to holder+138,
/// raw+4 to holder+3C. Kind15 zeroes BOTH holder values/validity if either is
/// zero. Kind16 is always valid and sets +38=trunc(signed header index/4).
/// No range/class predicate is folded into these stored DWORDs.
pub fn decode_pair_into(holder: &mut DecodedRecord, raw: &[u8; 8], kind: u32) -> Result<(), Error> {
    if !matches!(kind, 0x15 | 0x16) {
        return Err(Error::Invalid {
            line: 5,
            reason: "unknown frequency pair kind",
        });
    }
    holder.bytes[0x138..0x13c].copy_from_slice(&raw[..4]);
    holder.bytes[0x3c..0x40].copy_from_slice(&raw[4..8]);
    holder.set_valid(true);
    if kind == 0x15 && (holder.word(0x138) == 0 || holder.word(0x3c) == 0) {
        holder.put_word(0x138, 0);
        holder.put_word(0x3c, 0);
        holder.set_valid(false);
    }
    if kind == 0x16 {
        holder.put_word(0x38, ((holder.index() as i32) / 4) as u32);
    }
    Ok(())
}

/// Raw8 saver for kind15/16 in 80B7/80ED order: +138 first, then +3C. Kind15
/// valid0 clears those holder fields BEFORE the first store. Kind16 does not
/// gate or clear on valid0. Callback offsets are local raw8 and commit singly.
pub fn encode_pair_into(
    holder: &mut DecodedRecord,
    raw: &mut [u8; 8],
    kind: u32,
    mut write: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    if !matches!(kind, 0x15 | 0x16) {
        return Err(Error::Invalid {
            line: 5,
            reason: "unknown frequency pair kind",
        });
    }
    if kind == 0x15 && !holder.valid() {
        holder.put_word(0x138, 0);
        holder.put_word(0x3c, 0);
    }
    for (at, from) in [(0, 0x138), (4, 0x3c)] {
        raw[at..at + 4].copy_from_slice(&holder.bytes[from..from + 4]);
        write(at, &raw[at..at + 4])?;
    }
    Ok(())
}

/// Kind18/raw10 load (7514..7569): raw mode byte0 -> holder+44, raw+2 -> +138,
/// raw+6 -> +3C, +38=trunc(signed index/3), valid=1. It does NOT use mode+58's
/// lookup or touch the opaque raw byte1. Header and unrelated fields survive.
pub fn decode_mode_pair_into(holder: &mut DecodedRecord, raw: &[u8; 10]) {
    holder.bytes[0x138..0x13c].copy_from_slice(&raw[2..6]);
    holder.bytes[0x3c..0x40].copy_from_slice(&raw[6..10]);
    holder.put_word(0x44, u32::from(raw[0]));
    holder.put_word(0x38, ((holder.index() as i32) / 3) as u32);
    holder.set_valid(true);
}

/// Kind18 save (816C..81C7) stores +138/raw2, +3C/raw6, then low8 +44/raw0.
/// Valid0 clears all THREE holder fields before the first store. Opaque byte1
/// remains the target's old byte, including after an injected late failure.
pub fn encode_mode_pair_into(
    holder: &mut DecodedRecord,
    raw: &mut [u8; 10],
    mut write: impl FnMut(usize, &[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    if !holder.valid() {
        holder.put_word(0x138, 0);
        holder.put_word(0x3c, 0);
        holder.put_word(0x44, 0);
    }
    for (at, from, count) in [(2, 0x138, 4), (6, 0x3c, 4), (0, 0x44, 1)] {
        raw[at..at + count].copy_from_slice(&holder.bytes[from..from + count]);
        write(at, &raw[at..at + count])?;
    }
    Ok(())
}

/// Project special's EXISTING decoded raw36 type into the common holder.
/// No raw36 field masks/offsets are decoded here. The adapter supplies kind,
/// side/group/index metadata and shared split byte. Invalid kind04/05 retain
/// the native frequency-sentinel write; other invalid compact kinds skip body.
pub fn decode_compact_into(
    holder: &mut DecodedRecord,
    source: &super::special::CompactChannel,
    split: u8,
) {
    holder.set_valid(source.valid);
    if !source.valid {
        if matches!(holder.kind(), 4 | 5) {
            holder.put_word(0x3c, source.frequency_hz);
        }
        return;
    }
    holder.put_word(0x54, 0);
    holder.bytes[0x25] = if matches!(holder.kind(), 0x19..=0x1b) {
        split & 1
    } else {
        0
    };
    for (at, value) in [
        (0x3c, source.frequency_hz),
        (0x44, source.duplex),
        (0x48, source.offset_hz),
        (0x58, source.mode),
        (0x5c, source.data),
        (0x60, source.filter),
        (0x68, source.tone),
        (0x6c, source.tx_tone),
        (0x70, source.rx_tone),
        (0x74, source.dtcs),
        (0x78, source.polarity),
        (0x80, source.digital_squelch),
        (0x84, source.digital_code),
    ] {
        holder.put_word(at, value);
    }
    for (at, call) in [0x88, 0xa4, 0xc4].into_iter().zip(&source.callsigns) {
        holder.bytes[at..at + 8].copy_from_slice(call);
    }
}

/// Project the holder to special's decoded type for its one raw36 saver. This
/// is NOT a second byte codec or a save/reload operation; validity, full DWORD
/// values and high-bit callsign bytes remain unchanged for the native saver.
pub fn compact_values(holder: &DecodedRecord) -> super::special::CompactChannel {
    super::special::CompactChannel {
        valid: holder.valid(),
        frequency_hz: holder.word(0x3c),
        duplex: holder.word(0x44),
        offset_hz: holder.offset_hz(),
        mode: holder.mode(),
        data: holder.data(),
        filter: holder.filter(),
        tone: holder.word(0x68),
        tx_tone: holder.tx_tone(),
        rx_tone: holder.rx_tone(),
        dtcs: holder.word(0x74),
        polarity: holder.word(0x78),
        digital_squelch: holder.word(0x80),
        digital_code: holder.word(0x84),
        callsigns: [0x88, 0xa4, 0xc4].map(|at| {
            holder.bytes[at..at + 8]
                .try_into()
                .expect("fixed compact holder call")
        }),
    }
}

/// Project special's EXISTING raw27 decoder output into kind08's holder.
/// Valid0 only clears validity, preserving all incoming body fields. Native
/// reset and scan-specific initialization are separate, explicit operations.
pub fn decode_scan_into(holder: &mut DecodedRecord, source: &super::special::ScanEdge) {
    holder.set_valid(source.valid);
    if !source.valid {
        return;
    }
    holder.put_word(0x3c, source.high_hz);
    holder.put_word(0x138, source.low_hz);
    holder.set_mode(source.mode);
    holder.set_data(source.data);
    holder.set_filter(source.filter);
    holder.bytes[0x26..0x36].copy_from_slice(&source.name);
}

/// Build special's decoded scan type for its one raw27 saver. In particular,
/// invalid scan clearing belongs to that saver AFTER its bitmap store, not to
/// this projection or an atomic preflight in the caller.
pub fn scan_values(holder: &DecodedRecord) -> super::special::ScanEdge {
    super::special::ScanEdge {
        valid: holder.valid(),
        high_hz: holder.word(0x3c),
        low_hz: holder.word(0x138),
        mode: holder.mode(),
        data: holder.data(),
        filter: holder.filter(),
        name: holder.bytes[0x26..0x36]
            .try_into()
            .expect("fixed scan holder name"),
    }
}

/// Load only the history map-result payload, before current-list supplements.
///
/// kind0F/12/14 use raw8; kind10 uses raw12 (call8/frequency4). None is the map's
/// FF miss, not an invented empty payload. kind11/13 have NO decode branch and
/// preserve every incoming byte, including validity and +194. Unknown selector
/// or wrong raw width fails before mutation. The adapter must run original
/// current UR/repeater resolution AFTER this base load; no stale name/default
/// is fabricated here. History 0E..14 savers have no image stores (81C9), while
/// their outer 77B0 entry still updates the holder's target index.
pub fn decode_history_base_into(
    holder: &mut DecodedRecord,
    kind: u32,
    raw: Option<&[u8]>,
) -> Result<(), Error> {
    let width = match kind {
        0x11 | 0x13 => return Ok(()),
        0x0f | 0x12 | 0x14 => 8,
        0x10 => 12,
        _ => {
            return Err(Error::Invalid {
                line: 5,
                reason: "unknown base history record kind",
            });
        }
    };
    if raw.is_some_and(|bytes| bytes.len() != width) {
        return Err(Error::Invalid {
            line: 5,
            reason: "base history record has wrong width",
        });
    }
    holder.set_valid(raw.is_some());
    if kind != 0x14 {
        holder.bytes[0x194] = 0;
    }
    if let Some(raw) = raw {
        holder.bytes[0x18c..0x194].copy_from_slice(&raw[..8]);
        if kind == 0x10 {
            holder.bytes[0x3c..0x40].copy_from_slice(&raw[8..12]);
        }
    }
    Ok(())
}

/// The concrete current-list lookups used by history 71F7/7282/7328. Each call
/// uses the LIVE image; a count must not be cached across two Count requests.
/// Results retain original DWORD bits as i32, including negative resolver IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryQuery {
    /// C6624: current repeater total (signed comparison at the history caller).
    RepeaterCount,
    /// BD6C4(I,call,0,8,0,2), after the raw FromDV call has been loaded.
    FromDv { call: [u8; 8] },
    /// BDE94(I,call,frequency,0,limit), with the first current C6624 total.
    FromFm {
        call: [u8; 8],
        frequency_hz: u32,
        limit: i32,
    },
    /// BE4A4(I,call,8), the first ToDV UR search. Only EXACT result300 requests
    /// UpperUr; another miss value must not be rewritten to300 first.
    Ur { call: [u8; 8] },
    /// BE3A4(I,call,8), used only after Ur returned exactly300.
    UpperUr { call: [u8; 8] },
    /// BD5B0(I,call,8,2), after the final UR result failed unsigned<300.
    ToRepeater { call: [u8; 8] },
}

/// Field-only supplement loader, NOT a new outer 610C typed load. The adapter
/// must preserve the history kind/index/binding header. Repeater uses the same
/// 7614 field codec as decode_repeater_into; UR uses decode_callsign_into/75CC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryLoad {
    Repeater(i32),
    Ur(u32),
}

/// Complete the history BASE + current-list supplement sequence, without
/// duplicating any list search or raw field codec. Map/physical source reads
/// still belong to the image adapter. Raw widths/misses match the base loader.
///
/// FromDV does resolver -> current count; FromFM does count -> resolver -> NEW
/// count. ToDV tries UR, exactly300 -> upper UR, unsigned<300 -> UR load; only
/// otherwise does it try repeater -> count with a SIGNED comparison. Negative
/// repeater IDs are not silently rejected. Native loader input validity and
/// bounds are the adapter's explicit responsibility, not a fabricated miss.
///
/// H194=1 is written BEFORE a chosen supplement load. A query/load failure
/// preserves all preceding base fields and that flag; no zero/default/rollback
/// path replaces a failing current-list read. DD/common-exit and reflector
/// perform no query/load. This function never writes a history image record.
pub fn decode_history_into(
    holder: &mut DecodedRecord,
    kind: u32,
    raw: Option<&[u8]>,
    mut query: impl FnMut(HistoryQuery) -> Result<i32, Error>,
    mut load: impl FnMut(HistoryLoad, &mut DecodedRecord) -> Result<(), Error>,
) -> Result<(), Error> {
    decode_history_base_into(holder, kind, raw)?;
    if matches!(kind, 0x11 | 0x13 | 0x14) || !holder.valid() {
        return Ok(());
    }
    let call = holder.bytes[0x18c..0x194]
        .try_into()
        .expect("fixed history call");
    let repeater = match kind {
        0x0f => query(HistoryQuery::FromDv { call })?,
        0x10 => {
            let limit = query(HistoryQuery::RepeaterCount)?;
            query(HistoryQuery::FromFm {
                call,
                frequency_hz: holder.word(0x3c),
                limit,
            })?
        }
        0x12 => {
            let mut ur = query(HistoryQuery::Ur { call })?;
            if ur == 300 {
                ur = query(HistoryQuery::UpperUr { call })?;
            }
            if (ur as u32) < 300 {
                holder.bytes[0x194] = 1;
                return load(HistoryLoad::Ur(ur as u32), holder);
            }
            query(HistoryQuery::ToRepeater { call })?
        }
        _ => unreachable!("base decoder checked history kind"),
    };
    let count = query(HistoryQuery::RepeaterCount)?;
    if repeater < count {
        holder.bytes[0x194] = 1;
        load(HistoryLoad::Repeater(repeater), holder)?;
    }
    Ok(())
}

/// Decode a physical Rx record after its REVERSE rank-map lookup (6C57..71F2).
/// State, raw48, message20 and GPS92 are separate same-physical-row sources.
/// This codec does not set validity/header or perform the adapter's first-match
/// map search. Explicit union missing values ARE native writes, not a fallback:
/// unknown tags reset the documented union fields while unrelated bytes survive.
/// The matching history saver has no image writes; there is no inverse codec.
pub fn decode_receive_history_into(
    holder: &mut DecodedRecord,
    basic: &[u8; 48],
    message: &[u8; 20],
    state: u8,
    gps: &[u8; 92],
) {
    holder.bytes[0x195] = basic[47];
    holder.bytes[0x198..0x19c].copy_from_slice(&basic[36..40]);
    holder.bytes[0x19c] = decode_mode(basic[40]) as u8;
    for (from, to, count) in [
        (0, 0x19d, 8),
        (8, 0x1a5, 4),
        (28, 0x1a9, 8),
        (12, 0x1b1, 8),
        (20, 0x1b9, 8),
        (41, 0x1d8, 6),
    ] {
        holder.bytes[to..to + count].copy_from_slice(&basic[from..from + count]);
    }
    holder.bytes[0x1c1..0x1d5].copy_from_slice(message);
    holder.bytes[0x1d5] = match state & 7 {
        1 => 1,
        2 => 2,
        _ if state & 0x40 != 0 => 3,
        _ => 0,
    };
    holder.bytes[0x1d6] = u8::from(state & 0x20 != 0);
    holder.bytes[0x1d7] = u8::from(state & 8 != 0);
    for (from, to, count) in [
        (0, 0x1de, 8),
        (8, 0x1e8, 4),
        (14, 0x1ec, 2),
        (12, 0x1ee, 2),
        (25, 0x1f4, 4),
        (49, 0x219, 43),
    ] {
        holder.bytes[to..to + count].copy_from_slice(&gps[from..from + count]);
    }
    holder.bytes[0x1f0..0x1f4].fill(255);
    holder.bytes[0x1f8..0x1fc].fill(255);
    holder.bytes[0x1fc..0x205].fill(b' ');
    holder.bytes[0x205..0x219].fill(255);
    holder.bytes[0x207] = 0x7f;
    match gps[29] {
        1 | 6 => holder.bytes[0x1f0..0x1f4].copy_from_slice(&gps[30..34]),
        2 => holder.bytes[0x1f8..0x1fc].copy_from_slice(&gps[30..34]),
        3 | 4 => {
            holder.bytes[0x1fc..0x205].copy_from_slice(&gps[30..39]);
            holder.bytes[0x205] = gps[39];
            match gps[40] {
                1 => holder.bytes[0x1f0..0x1f4].copy_from_slice(&gps[41..45]),
                2 => holder.bytes[0x1f8..0x1fc].copy_from_slice(&gps[41..45]),
                _ => {}
            }
        }
        5 => {
            for (from, to, count) in [
                (30, 0x20e, 6),
                (36, 0x206, 8),
                (44, 0x218, 1),
                (45, 0x214, 4),
            ] {
                holder.bytes[to..to + count].copy_from_slice(&gps[from..from + count]);
            }
        }
        _ => {}
    }
}

fn history_map(kind: u32) -> Result<(usize, usize), Error> {
    match kind {
        0x0e => Ok((0x3c2ff, 50)),
        0x0f => Ok((0x3c331, 10)),
        0x10 => Ok((0x3c33b, 10)),
        0x11 => Ok((0x3c345, 10)),
        0x12 => Ok((0x3c34f, 20)),
        0x13 => Ok((0x3c368, 20)),
        0x14 => Ok((0x3c363, 5)),
        _ => Err(Error::Invalid {
            line: 5,
            reason: "unknown history map kind",
        }),
    }
}

/// Native history delete pre-confirmation test. Rx searches rank VALUES; other
/// histories inspect the logical map cells. Empty selections need no prompt.
pub fn history_selection_present(
    image: &super::Icf<'_>,
    kind: u32,
    first: usize,
    last: usize,
) -> Result<bool, Error> {
    let (base, count) = history_map(kind)?;
    if first > last || last >= count {
        return Err(Error::Invalid {
            line: 5,
            reason: "history selection outside dataset",
        });
    }
    let map = &image.memory()[base..base + count];
    Ok(if kind == 0x0e {
        map.iter()
            .any(|&rank| (first..=last).contains(&usize::from(rank)))
    } else {
        map[first..=last].iter().any(|&cell| cell != 255)
    })
}

/// CDBD8/CDCE0 history delete. The caller confirms ONLY a nonempty selection.
/// Physical records/message/GPS arrays never move or clear. Rx updates each
/// physical rank; From/To/ref shift the selected FULL width, including holes.
/// This safe adapter bounds the real UI selection before the first map write.
pub fn delete_history_storage(
    image: &mut super::Icf<'_>,
    kind: u32,
    first: usize,
    last: usize,
) -> Result<bool, Error> {
    if !history_selection_present(image, kind, first, last)? {
        return Ok(false);
    }
    let (base, count) = history_map(kind)?;
    let width = last - first + 1;
    if kind == 0x0e {
        for at in 0..count {
            let rank = image.memory()[base + at];
            let n = usize::from(rank);
            let new = if (first..=last).contains(&n) {
                255
            } else if rank != 255 && n > first {
                rank.wrapping_sub(width as u8)
            } else {
                rank
            };
            image.patch_memory(&[(base + at, &[new])])?;
        }
    } else {
        let tail = image.memory()[base + last + 1..base + count].to_vec();
        image.patch_memory(&[(base + first, &tail)])?;
        for at in count - width..count {
            image.patch_memory(&[(base + at, &[255])])?;
        }
    }
    Ok(true)
}

/// One decoded, read-only history field. Byte strings remain byte strings; no
/// host ACP/UTF-8/calendar/locale conversion is smuggled into their projection.
pub enum HistoryValue<'a> {
    Unsigned(u32),
    Signed(i32),
    Bytes(&'a [u8]),
}

/// Projection of the Rx decoder's fields, not another raw48/raw92 codec. Date
/// and time stay separate here (CSV joins them through its existing formatter).
pub fn receive_history_fields(holder: &DecodedRecord) -> Vec<(&'static str, HistoryValue<'_>)> {
    use HistoryValue::{Bytes as B, Signed as S, Unsigned as U};
    let b = &holder.bytes;
    let word = |at| u16::from_le_bytes([b[at], b[at + 1]]);
    vec![
        ("type", U(u32::from(b[0x195]))),
        ("frequency-hz", U(holder.word(0x198))),
        ("mode", U(u32::from(b[0x19c]))),
        ("caller", B(&b[0x19d..0x1a5])),
        ("caller2", B(&b[0x1a5..0x1a9])),
        ("called", B(&b[0x1a9..0x1b1])),
        ("repeater1", B(&b[0x1b1..0x1b9])),
        ("repeater2", B(&b[0x1b9..0x1c1])),
        ("message", B(&b[0x1c1..0x1d5])),
        ("status", U(u32::from(b[0x1d5]))),
        ("bk", U(u32::from(b[0x1d6]))),
        ("emr", U(u32::from(b[0x1d7]))),
        ("date", B(&b[0x1d8..0x1db])),
        ("time", B(&b[0x1db..0x1de])),
        ("latitude", B(&b[0x1de..0x1e2])),
        ("longitude", B(&b[0x1e2..0x1e6])),
        ("altitude-tenths-ft", S(holder.word(0x1e8) as i32)),
        ("ssid", B(&b[0x1ec..0x1ee])),
        ("symbol", B(&b[0x1ee..0x1f0])),
        ("course", U(u32::from(word(0x1f0)))),
        ("speed", U(u32::from(word(0x1f2)))),
        ("gps-timestamp", B(&b[0x1f4..0x1f8])),
        ("power", U(u32::from(b[0x1f8]))),
        ("height", U(u32::from(b[0x1f9]))),
        ("gain", U(u32::from(b[0x1fa]))),
        ("directivity", U(u32::from(b[0x1fb]))),
        ("object-name", B(&b[0x1fc..0x205])),
        ("data-type", U(u32::from(b[0x205]))),
        ("temperature-f", S(i32::from(word(0x206) as i16))),
        ("rain", U(u32::from(word(0x208)))),
        ("rain24", U(u32::from(word(0x20a)))),
        ("rain-midnight", U(u32::from(word(0x20c)))),
        ("wind-direction", U(u32::from(word(0x20e)))),
        ("wind-speed", U(u32::from(word(0x210)))),
        ("gust-speed", U(u32::from(word(0x212)))),
        ("pressure", U(holder.word(0x214))),
        ("humidity", U(u32::from(b[0x218]))),
        ("gps-message", B(&b[0x219..0x244])),
    ]
}

/// Canonical Rx image read through 9C87C's reverse rank map. Duplicates choose
/// the FIRST physical slot. This safe document adapter admits only the actual
/// logical UI rows0..49; raw logical255 behavior is not silently normalized.
pub fn load_receive_history_storage_into(
    image: &super::Icf<'_>,
    index: u32,
    holder: &mut DecodedRecord,
) -> Result<(), Error> {
    if index >= 50 {
        return Err(Error::Invalid {
            line: 5,
            reason: "Rx history logical row outside 0..50",
        });
    }
    let memory = image.memory();
    let physical = memory[0x3c2ff..0x3c331]
        .iter()
        .position(|&rank| u32::from(rank) == index);
    holder.set_header(0x0e, index, holder.binding());
    holder.set_valid(false);
    let Some(physical) = physical else {
        return Ok(());
    };
    let base = 0x309c3;
    let basic = memory[base + 0x32 + 48 * physical..base + 0x32 + 48 * (physical + 1)]
        .try_into()
        .expect("bounded Rx basic record");
    let message = memory[base + 0x992 + 20 * physical..base + 0x992 + 20 * (physical + 1)]
        .try_into()
        .expect("bounded Rx message");
    let gps = memory[base + 0xd7a + 92 * physical..base + 0xd7a + 92 * (physical + 1)]
        .try_into()
        .expect("bounded Rx GPS record");
    holder.set_valid(true);
    decode_receive_history_into(holder, basic, message, memory[base + physical], gps);
    Ok(())
}

/// Load the raw type-0C windows at RVA 0x7614..0x7795 into an existing holder.
///
/// `gateway` is the same physical row's seven-byte auxiliary record, NOT an
/// indirect lookup through raw[8..10]. Dataset existence/order/group and USE
/// extraction belong to the caller. This low-level loader does not change the
/// binding header, valid bit, common mode/DATA/filter or unrelated body fields.
/// For decoded repeater type >=2 it also leaves +178 (tone index) untouched.
pub fn decode_repeater_into(
    holder: &mut DecodedRecord,
    raw: &[u8; 52],
    gateway: &[u8; 7],
    use_value: u32,
    group: u32,
) {
    holder.bytes[0x26..0x36].copy_from_slice(&raw[18..34]);
    holder.bytes[0x154..0x15c].copy_from_slice(&raw[34..42]);
    let mode = lookup(&REPEATER_TYPE, raw[10] >> 6, false);
    holder.put_word(0x15c, u32::from(mode));
    holder.bytes[0x160..0x168].copy_from_slice(&unpack_callsign(&raw[11..18]));
    holder.bytes[0x168..0x170].copy_from_slice(&unpack_callsign(gateway));
    if mode < 2 {
        holder.put_word(0x178, u32::from(raw[48]));
    }
    holder.bytes[0x3c..0x40].copy_from_slice(&raw[..4]);
    holder.bytes[0x48..0x4c].copy_from_slice(&raw[4..8]);
    holder.put_word(0x44, u32::from(raw[10] >> 4 & 3));
    holder.put_word(0x174, u32::from(raw[10] >> 2 & 3));
    holder.put_word(0x17c, use_value);
    holder.put_word(0x180, u32::from(raw[10] & 3));
    holder.bytes[0x25a..0x25d].copy_from_slice(&raw[42..45]);
    holder.bytes[0x25d] = if raw[51] & 0x80 != 0 { b'S' } else { b'N' };
    holder.bytes[0x25e..0x261].copy_from_slice(&raw[45..48]);
    holder.bytes[0x261] = if raw[51] & 0x40 != 0 { b'W' } else { b'E' };
    holder.put_word(
        0x184,
        i32::from(i16::from_le_bytes([raw[49], raw[50]])) as u32,
    );
    holder.set_group(group);
    holder.put_word(0x188, u32::from(u16::from_le_bytes([raw[8], raw[9]])));
}

/// Original type-0C saver stages, in their native order. ToneIndex is absent
/// for a full decoded type DWORD outside 0/1; it is not an emitted no-op write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeaterStage {
    Name,
    SubName,
    Type,
    RepeaterCall,
    GatewayCall,
    ToneIndex,
    Frequency,
    Offset,
    Duplex,
    Tone,
    Use,
    Position,
    Latitude,
    South,
    Longitude,
    West,
    UtcOffset,
    GatewayIndex,
}

/// One immediate write requested by the native saver. Body offsets are local
/// to raw52; Gateway uses the same physical row's raw7. USE retains its full
/// DWORD so the dataset can apply D65E0's nonzero test and preserve sibling bits.
/// The final raw8..10 word is a separate metadata store, not a gateway resolver.
pub enum RepeaterWrite<'a> {
    Body {
        stage: RepeaterStage,
        offset: usize,
        bytes: &'a [u8],
    },
    Gateway([u8; 7]),
    Use(u32),
}
impl RepeaterWrite<'_> {
    /// Identify the source stage without copying payloads into a diagnostic log.
    pub fn stage(&self) -> RepeaterStage {
        match self {
            Self::Body { stage, .. } => *stage,
            Self::Gateway(_) => RepeaterStage::GatewayCall,
            Self::Use(_) => RepeaterStage::Use,
        }
    }
}

fn repeater_body(
    raw: &mut [u8; 52],
    stage: RepeaterStage,
    at: usize,
    bytes: &[u8],
    write: &mut impl FnMut(RepeaterWrite<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    raw[at..at + bytes.len()].copy_from_slice(bytes);
    write(RepeaterWrite::Body {
        stage,
        offset: at,
        bytes: &raw[at..at + bytes.len()],
    })
}

/// Execute the 18-stage raw type-0C saver (RVA 0x7EFE..0x8072).
///
/// `raw` starts with the target's old bytes; it retains opaque byte51 low6 and
/// conditionally retains byte48. Every callback is immediate, never an atomic
/// preflight/rollback transaction. On callback failure all earlier commits
/// remain; `raw` includes the attempted body write at the failing stage, whereas
/// the callback's destination may have only its preceding committed prefix.
///
/// Native source writes gateway raw7 at stage5 and USE at stage11. There is no
/// gateway search/registration in this low-level saver: the dataset adapter must
/// not move stage5 after the final index store or resolve it as an indirect link.
/// Validity does not gate these raw writes. Type selection uses low8 lookup, but
/// the optional tone-index branch tests the full DWORD unsigned value <2.
pub fn encode_repeater_into(
    holder: &DecodedRecord,
    raw: &mut [u8; 52],
    mut write: impl FnMut(RepeaterWrite<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    use RepeaterStage as S;
    repeater_body(raw, S::Name, 18, &holder.bytes[0x26..0x36], &mut write)?;
    repeater_body(raw, S::SubName, 34, &holder.bytes[0x154..0x15c], &mut write)?;
    let ty = (raw[10] & 0x3f) | ((lookup(&REPEATER_TYPE, holder.word(0x15c) as u8, true) & 3) << 6);
    repeater_body(raw, S::Type, 10, &[ty], &mut write)?;
    let callsign: [u8; 8] = holder.bytes[0x160..0x168]
        .try_into()
        .expect("fixed callsign");
    repeater_body(
        raw,
        S::RepeaterCall,
        11,
        &pack_callsign(&callsign.map(|b| b & 0x7f)),
        &mut write,
    )?;
    let callsign: [u8; 8] = holder.bytes[0x168..0x170]
        .try_into()
        .expect("fixed gateway callsign");
    write(RepeaterWrite::Gateway(pack_callsign(
        &callsign.map(|b| b & 0x7f),
    )))?;
    if holder.word(0x15c) < 2 {
        repeater_body(raw, S::ToneIndex, 48, &[holder.bytes[0x178]], &mut write)?;
    }
    repeater_body(raw, S::Frequency, 0, &holder.bytes[0x3c..0x40], &mut write)?;
    repeater_body(raw, S::Offset, 4, &holder.bytes[0x48..0x4c], &mut write)?;
    let duplex = (raw[10] & !0x30) | ((holder.bytes[0x44] & 3) << 4);
    repeater_body(raw, S::Duplex, 10, &[duplex], &mut write)?;
    let tone = (raw[10] & !0x0c) | ((holder.bytes[0x174] & 3) << 2);
    repeater_body(raw, S::Tone, 10, &[tone], &mut write)?;
    write(RepeaterWrite::Use(holder.word(0x17c)))?;
    let position = (raw[10] & !3) | (holder.bytes[0x180] & 3);
    repeater_body(raw, S::Position, 10, &[position], &mut write)?;
    repeater_body(
        raw,
        S::Latitude,
        42,
        &holder.bytes[0x25a..0x25d],
        &mut write,
    )?;
    let south = (raw[51] & 0x7f) | (u8::from(holder.bytes[0x25d] == b'S') << 7);
    repeater_body(raw, S::South, 51, &[south], &mut write)?;
    repeater_body(
        raw,
        S::Longitude,
        45,
        &holder.bytes[0x25e..0x261],
        &mut write,
    )?;
    let west = (raw[51] & !0x40) | (u8::from(holder.bytes[0x261] == b'W') << 6);
    repeater_body(raw, S::West, 51, &[west], &mut write)?;
    repeater_body(
        raw,
        S::UtcOffset,
        49,
        &holder.bytes[0x184..0x186],
        &mut write,
    )?;
    repeater_body(
        raw,
        S::GatewayIndex,
        8,
        &holder.bytes[0x188..0x18a],
        &mut write,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_all_bytes_and_dword_aliases_match_independent_literal_cases() {
        for byte in 0..=255u8 {
            let mode = match byte {
                0..=3 => u32::from(byte),
                4 => 6,
                5 => 7,
                6 => 8,
                7 => 13,
                8 => 15,
                9 => 16,
                255 => 25,
                _ => 255,
            };
            let tone = match byte {
                0..=2 => u32::from(byte),
                3 => 4,
                4 => 6,
                5 => 7,
                6 => 8,
                7 => 9,
                _ => 255,
            };
            let digital = match byte {
                0 => 0,
                1 => 2,
                2 => 4,
                _ => 255,
            };
            assert_eq!(decode_mode(byte), mode, "mode raw {byte}");
            assert_eq!(decode_tone(byte), tone, "tone raw {byte}");
            assert_eq!(decode_digital_squelch(byte), digital, "digital raw {byte}");
            let mode = match byte {
                0..=3 => byte,
                6 => 4,
                7 => 5,
                8 => 6,
                13 => 7,
                15 => 8,
                16 => 9,
                _ => 255,
            };
            let tone = match byte {
                0..=2 => byte,
                4 => 3,
                6..=9 => byte - 2,
                _ => 255,
            };
            let digital = match byte {
                0 => 0,
                2 => 1,
                4 => 2,
                _ => 255,
            };
            for high in [0, 0x100, 0x8000_0000, 0xffff_ff00] {
                let decoded = high | u32::from(byte);
                assert_eq!(encode_mode(decoded), mode, "mode decoded {decoded}");
                assert_eq!(encode_tone(decoded), tone, "tone decoded {decoded}");
                assert_eq!(
                    encode_digital_squelch(decoded),
                    digital,
                    "digital decoded {decoded}"
                );
            }
        }
        assert_eq!(encode_mode(25), 255);
        assert_eq!(decode_mode(255), 25);
        assert_eq!(encode_mode(17), 255);
        assert_eq!(decode_mode(15), 255);
        assert_eq!(encode_digital_squelch(0x104), 2);
    }

    #[test]
    fn ordered_lookup_keeps_first_match_and_distinguishes_both_miss_directions() {
        let pairs = [(7, 2), (7, 4), (9, 2)];
        assert_eq!(lookup(&pairs, 7, false), 2);
        assert_eq!(lookup(&pairs, 2, true), 7);
        assert_eq!(lookup(&pairs, 9, false), 2);
        assert_eq!(lookup(&pairs, 4, true), 7);
        assert_eq!(lookup(&pairs, 2, false), 255);
        assert_eq!(lookup(&pairs, 7, true), 255);
        assert_eq!(lookup(&[], 0, false), 255);
        assert_eq!(lookup(&[], 0, true), 255);
    }

    #[test]
    fn reset_matches_all_616_instruction_constant_bytes_without_an_implicit_default() {
        let h = reset_holder(0x1234_5678, u32::MAX, 0xdead_beef);
        let header = [
            0x78, 0x56, 0x34, 0x12, 255, 255, 255, 255, 0xef, 0xbe, 0xad, 0xde,
        ];
        for (at, &actual) in h.bytes.iter().enumerate() {
            let expected = match at {
                0..=11 => header[at],
                12 => 1,
                0x20..=0x23 => 255,
                0x6c | 0x70 => 8,
                0x78 => 2,
                0x88..=0x8f => b"CQCQCQ  "[at - 0x88],
                0x0d..=0x1c
                | 0x26..=0x35
                | 0x90..=0x9f
                | 0xa4..=0xbc
                | 0xc4..=0xdc
                | 0x154..=0x15b
                | 0x160..=0x16f
                | 0x18c..=0x193
                | 0x244..=0x253 => b' ',
                _ => 0,
            };
            assert_eq!(actual, expected, "reset byte {at:x}");
        }
        assert_eq!(
            (h.kind(), h.index(), h.binding()),
            (0x1234_5678, u32::MAX, 0xdead_beef)
        );
        assert_eq!(
            (h.valid(), h.group(), h.tx_tone(), h.rx_tone()),
            (true, u32::MAX, 8, 8)
        );
        // A DD common exit preserves this actual constructor state, not invalid.
        let mut dd = h.clone();
        decode_history_base_into(&mut dd, 0x11, None).unwrap();
        assert!(dd == h);
    }

    #[test]
    fn pair_kinds_keep_zero_validity_signed_metadata_and_all_failure_prefixes() {
        for kind in [0x15, 0x16] {
            for index in [0, 1, 29, 30, 67, 68, u32::MAX, 0xffff_fffc, 0x8000_0000] {
                for (first, second) in [(0, 0), (0, 7), (7, 0), (0x1122_3344, 0xaabb_ccdd)] {
                    let mut h = DecodedRecord::from_bytes([0xa4; 616]);
                    h.set_header(kind, index, 0xdead_beef);
                    let mut raw = [0; 8];
                    raw[..4].copy_from_slice(&u32::to_le_bytes(first));
                    raw[4..].copy_from_slice(&u32::to_le_bytes(second));
                    let mut expected = *h.as_bytes();
                    let valid = kind == 0x16 || (first != 0 && second != 0);
                    expected[12] = if valid { 0xa5 } else { 0xa4 };
                    expected[0x138..0x13c]
                        .copy_from_slice(&if valid { first } else { 0 }.to_le_bytes());
                    expected[0x3c..0x40]
                        .copy_from_slice(&if valid { second } else { 0 }.to_le_bytes());
                    if kind == 0x16 {
                        expected[0x38..0x3c].copy_from_slice(&((index as i32) / 4).to_le_bytes());
                    }
                    decode_pair_into(&mut h, &raw, kind).unwrap();
                    assert_eq!(h.as_bytes(), &expected);
                }
            }
            for valid in [false, true] {
                for stop in 0..=2 {
                    let mut h = reset_holder(kind, 0, 0);
                    h.set_valid(valid);
                    h.put_word(0x138, 0x1122_3344);
                    h.put_word(0x3c, 0xaabb_ccdd);
                    let mut raw = [0x93; 8];
                    let mut committed = raw;
                    let mut seen = Vec::new();
                    let result = encode_pair_into(&mut h, &mut raw, kind, |at, bytes| {
                        seen.push(at);
                        if seen.len() - 1 == stop {
                            return Err(Error::Invalid {
                                line: 0,
                                reason: "injected raw8 failure",
                            });
                        }
                        committed[at..at + 4].copy_from_slice(bytes);
                        Ok(())
                    });
                    assert_eq!(result.is_ok(), stop == 2);
                    assert_eq!(seen, [0, 4][..(stop + 1).min(2)]);
                    let cleared = kind == 0x15 && !valid;
                    let mut expected = [0x93; 8];
                    if stop > 0 {
                        expected[..4].copy_from_slice(&if cleared {
                            [0; 4]
                        } else {
                            [0x44, 0x33, 0x22, 0x11]
                        });
                    }
                    if stop > 1 {
                        expected[4..].copy_from_slice(&if cleared {
                            [0; 4]
                        } else {
                            [0xdd, 0xcc, 0xbb, 0xaa]
                        });
                    }
                    assert_eq!(committed, expected);
                    assert_eq!(
                        (h.word(0x138), h.word(0x3c)),
                        if cleared {
                            (0, 0)
                        } else {
                            (0x1122_3344, 0xaabb_ccdd)
                        }
                    );
                }
            }
        }
        let mut h = reset_holder(0, 0, 0);
        let before = *h.as_bytes();
        let mut raw = [0x93; 8];
        assert!(decode_pair_into(&mut h, &raw, 0x18).is_err());
        assert!(
            encode_pair_into(&mut h, &mut raw, 0x18, |_, _| panic!(
                "bad kind must not store"
            ))
            .is_err()
        );
        assert_eq!(h.as_bytes(), &before);
        assert_eq!(raw, [0x93; 8]);
    }

    #[test]
    fn mode_pair_all_bytes_preserve_byte1_and_valid_zero_clears_before_first_store() {
        for index in [0, 1, 11, 12, u32::MAX, 0xffff_fffd, 0x8000_0000] {
            let mut h = DecodedRecord::from_bytes([0xa4; 616]);
            h.set_header(0x18, index, 7);
            let before = *h.as_bytes();
            decode_mode_pair_into(
                &mut h,
                &[0xfe, 0x93, 0x44, 0x33, 0x22, 0x11, 0xdd, 0xcc, 0xbb, 0xaa],
            );
            let mut expected = before;
            expected[12] |= 1;
            expected[0x138..0x13c].copy_from_slice(&[0x44, 0x33, 0x22, 0x11]);
            expected[0x3c..0x40].copy_from_slice(&[0xdd, 0xcc, 0xbb, 0xaa]);
            expected[0x44..0x48].copy_from_slice(&[0xfe, 0, 0, 0]);
            expected[0x38..0x3c].copy_from_slice(&((index as i32) / 3).to_le_bytes());
            assert_eq!(h.as_bytes(), &expected);
        }
        for mode in 0..=255u8 {
            for opaque in 0..=255u8 {
                let mut h = reset_holder(0x18, 11, 7);
                h.put_word(0x138, 0x1122_3344);
                h.put_word(0x3c, 0xaabb_ccdd);
                h.put_word(0x44, 0x100 | u32::from(mode));
                let mut raw = [0x93; 10];
                raw[1] = opaque;
                encode_mode_pair_into(&mut h, &mut raw, |_, _| Ok(())).unwrap();
                assert_eq!(
                    raw,
                    [mode, opaque, 0x44, 0x33, 0x22, 0x11, 0xdd, 0xcc, 0xbb, 0xaa]
                );
            }
        }
        for valid in [false, true] {
            for stop in 0..=3 {
                let mut h = reset_holder(0x18, 11, 7);
                h.set_valid(valid);
                h.put_word(0x138, 0x1122_3344);
                h.put_word(0x3c, 0xaabb_ccdd);
                h.put_word(0x44, 0x1fe);
                let mut raw = [0x93; 10];
                let mut committed = raw;
                let mut seen = Vec::new();
                let result = encode_mode_pair_into(&mut h, &mut raw, |at, bytes| {
                    seen.push(at);
                    if seen.len() - 1 == stop {
                        return Err(Error::Invalid {
                            line: 0,
                            reason: "injected raw10 failure",
                        });
                    }
                    committed[at..at + bytes.len()].copy_from_slice(bytes);
                    Ok(())
                });
                assert_eq!(result.is_ok(), stop == 3);
                assert_eq!(seen, [2, 6, 0][..(stop + 1).min(3)]);
                let mut expected = [0x93; 10];
                if stop > 0 {
                    expected[2..6].copy_from_slice(&if valid {
                        [0x44, 0x33, 0x22, 0x11]
                    } else {
                        [0; 4]
                    });
                }
                if stop > 1 {
                    expected[6..10].copy_from_slice(&if valid {
                        [0xdd, 0xcc, 0xbb, 0xaa]
                    } else {
                        [0; 4]
                    });
                }
                if stop > 2 {
                    expected[0] = if valid { 0xfe } else { 0 };
                }
                assert_eq!(committed, expected);
                assert_eq!(
                    (h.word(0x138), h.word(0x3c), h.word(0x44)),
                    if valid {
                        (0x1122_3344, 0xaabb_ccdd, 0x1fe)
                    } else {
                        (0, 0, 0)
                    }
                );
            }
        }
    }

    #[test]
    fn special_decoded_type_projections_do_not_duplicate_raw36_or_raw27_codecs() {
        let source = super::super::special::CompactChannel {
            valid: true,
            frequency_hz: 0x1122_3344,
            duplex: 0x104,
            offset_hz: u32::MAX,
            mode: 0x111,
            data: 0x100,
            filter: 255,
            tone: 0x109,
            tx_tone: 50,
            rx_tone: 51,
            dtcs: 0x1ff,
            polarity: 0x103,
            digital_squelch: 0x104,
            digital_code: 0x1fe,
            callsigns: [[0xc1; 8], [0xc2; 8], [0xc3; 8]],
        };
        for kind in [4, 5, 0x19, 0x1a, 0x1b] {
            let mut h = DecodedRecord::from_bytes([0xa5; 616]);
            h.set_header(kind, 1, 7);
            let mut expected = *h.as_bytes();
            expected[0x25] = if kind >= 0x19 { 1 } else { 0 };
            expected[0x54..0x58].fill(0);
            for (at, bytes) in [
                (0x3c, [0x44, 0x33, 0x22, 0x11]),
                (0x44, [4, 1, 0, 0]),
                (0x48, [255; 4]),
                (0x58, [17, 1, 0, 0]),
                (0x5c, [0, 1, 0, 0]),
                (0x60, [255, 0, 0, 0]),
                (0x68, [9, 1, 0, 0]),
                (0x6c, [50, 0, 0, 0]),
                (0x70, [51, 0, 0, 0]),
                (0x74, [255, 1, 0, 0]),
                (0x78, [3, 1, 0, 0]),
                (0x80, [4, 1, 0, 0]),
                (0x84, [254, 1, 0, 0]),
            ] {
                expected[at..at + 4].copy_from_slice(&bytes);
            }
            expected[0x88..0x90].fill(0xc1);
            expected[0xa4..0xac].fill(0xc2);
            expected[0xc4..0xcc].fill(0xc3);
            decode_compact_into(&mut h, &source, 0xff);
            assert_eq!(h.as_bytes(), &expected);
            assert_eq!(compact_values(&h), source);
            let mut invalid = source.clone();
            invalid.valid = false;
            invalid.frequency_hz = u32::MAX;
            let before = *h.as_bytes();
            decode_compact_into(&mut h, &invalid, 0);
            let mut expected = before;
            expected[12] &= !1;
            if matches!(kind, 4 | 5) {
                expected[0x3c..0x40].fill(255);
            }
            assert_eq!(h.as_bytes(), &expected);
        }
        let scan = super::super::special::ScanEdge {
            valid: true,
            low_hz: 0x1122_3344,
            high_hz: 0xaabb_ccdd,
            mode: 0x119,
            data: 0x100,
            filter: 255,
            name: *b"AB\0DEFGHIJKLMNOP",
        };
        let mut h = DecodedRecord::from_bytes([0xa4; 616]);
        let mut expected = *h.as_bytes();
        expected[12] |= 1;
        expected[0x138..0x13c].copy_from_slice(&[0x44, 0x33, 0x22, 0x11]);
        expected[0x3c..0x40].copy_from_slice(&[0xdd, 0xcc, 0xbb, 0xaa]);
        expected[0x58..0x5c].copy_from_slice(&[25, 1, 0, 0]);
        expected[0x5c..0x60].copy_from_slice(&[0, 1, 0, 0]);
        expected[0x60..0x64].copy_from_slice(&[255, 0, 0, 0]);
        expected[0x26..0x36].copy_from_slice(b"AB\0DEFGHIJKLMNOP");
        decode_scan_into(&mut h, &scan);
        assert_eq!(h.as_bytes(), &expected);
        assert_eq!(scan_values(&h), scan);
        let invalid = super::super::special::ScanEdge {
            valid: false,
            ..scan
        };
        expected[12] &= !1;
        decode_scan_into(&mut h, &invalid);
        assert_eq!(h.as_bytes(), &expected);
    }

    #[test]
    fn repeater_context_producer_and_named_fields_match_native_finite_branches() {
        for (kind, override_byte, langid, five, hemispheres) in [
            (0, 0, None, true, *b"NE"),
            (1, 0, None, false, *b"NW"),
            (2, 0, None, false, *b"NE"),
            (3, 0, None, false, *b"NE"),
            (4, 0, None, false, *b"NE"),
            (5, 0, None, false, *b"NE"),
            (6, 0, None, true, *b"NE"),
            (15, 0, None, true, *b"NE"),
            (0, 2, None, false, *b"NW"),
            (16, 0, Some(0x411), true, *b"NE"),
            (16, 0, Some(0x409), false, *b"NW"),
            (16, 2, Some(0x811), true, *b"NE"),
        ] {
            let mut bytes = [0xa5; 24];
            bytes[..4].copy_from_slice(&i32::to_le_bytes(kind));
            bytes[22] = override_byte;
            let q = super::super::context::Context::from_bytes(bytes);
            let result = repeater_default_context(&q, langid).unwrap();
            assert_eq!(result.five_mhz, five);
            assert_eq!(result.hemispheres, hemispheres);
        }
        let mut bytes = [0; 24];
        bytes[..4].copy_from_slice(&16i32.to_le_bytes());
        assert!(
            repeater_default_context(&super::super::context::Context::from_bytes(bytes), None)
                .is_err()
        );
        bytes[..4].copy_from_slice(&(-1i32).to_le_bytes());
        assert!(
            repeater_default_context(&super::super::context::Context::from_bytes(bytes), None)
                .is_err()
        );
        let mut holder = DecodedRecord::from_bytes([0xa5; 616]);
        let mut expected = *holder.as_bytes();
        for (field, at) in [
            (RepeaterField::Mode, 0x15c),
            (RepeaterField::Tone, 0x174),
            (RepeaterField::ToneIndex, 0x178),
            (RepeaterField::Use, 0x17c),
            (RepeaterField::Position, 0x180),
            (RepeaterField::UtcOffset, 0x184),
            (RepeaterField::GatewayIndex, 0x188),
        ] {
            holder.set_repeater_field(field, 0x1122_3344);
            expected[at..at + 4].copy_from_slice(&[0x44, 0x33, 0x22, 0x11]);
            assert_eq!(holder.repeater_field(field), 0x1122_3344);
        }
        for (field, at) in [
            (RepeaterText::SubName, 0x154),
            (RepeaterText::Call, 0x160),
            (RepeaterText::Gateway, 0x168),
        ] {
            holder.set_repeater_text(field, &[0xc1; 8]);
            expected[at..at + 8].fill(0xc1);
            assert_eq!(holder.repeater_text(field), &[0xc1; 8]);
        }
        holder.set_coordinate_bytes(false, &[1, 2, 3, b'S']);
        holder.set_coordinate_bytes(true, &[4, 5, 6, b'W']);
        expected[0x25a..0x262].copy_from_slice(&[1, 2, 3, b'S', 4, 5, 6, b'W']);
        assert_eq!(holder.as_bytes(), &expected);
        assert_eq!(holder.coordinate_bytes(false), &[1, 2, 3, b'S']);
        assert_eq!(holder.coordinate_bytes(true), &[4, 5, 6, b'W']);
        let mut g = vec![0xa5; 0x3c8bb];
        g[0x3c8b9..].copy_from_slice(&[0x34, 0x12]);
        assert_eq!(repeater_reset_word(&g, 0).unwrap(), 0x1234);
        assert!(repeater_reset_word(&g, 1).is_err());
        assert!(repeater_reset_word(&g, 2).is_err());
    }

    #[test]
    fn cli_member_and_document_adapters_preserve_opaque_bytes_and_document_metadata() {
        let mut h = DecodedRecord::from_bytes([0xa5; 616]);
        let mut expected = *h.as_bytes();
        for (field, at) in [
            (ChannelField::FrequencyHz, 0x3c),
            (ChannelField::Duplex, 0x44),
            (ChannelField::Selection, 0x54),
            (ChannelField::Tone, 0x68),
            (ChannelField::Dtcs, 0x74),
            (ChannelField::Polarity, 0x78),
            (ChannelField::DigitalSquelch, 0x80),
            (ChannelField::DigitalCode, 0x84),
        ] {
            h.set_channel_field(field, 0x1122_3344);
            expected[at..at + 4].copy_from_slice(&[0x44, 0x33, 0x22, 0x11]);
            assert_eq!(h.channel_field(field), 0x1122_3344);
            assert_eq!(h.as_bytes(), &expected);
        }
        h.set_name(b"A\0CDEFGHIJKLMNOP");
        expected[0x26..0x36].copy_from_slice(b"A\0CDEFGHIJKLMNOP");
        h.set_split(0xff);
        expected[0x25] = 255;
        for (which, at) in [
            (super::super::MemoryCallsign::Your, 0x88),
            (super::super::MemoryCallsign::Repeater1, 0xa4),
            (super::super::MemoryCallsign::Repeater2, 0xc4),
        ] {
            h.set_callsign(which, &[0xc1; 8]);
            expected[at..at + 8].fill(0xc1);
            assert_eq!(h.callsign(which), &[0xc1; 8]);
        }
        assert_eq!(h.side(), 0xa5);
        assert_eq!(h.split(), 255);
        assert_eq!(h.name(), b"A\0CDEFGHIJKLMNOP");
        assert_eq!(h.as_bytes(), &expected);
        let mut memory = vec![0xa6; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        let mut image = super::super::Icf::from_memory(memory).unwrap();
        image.comment = b"retained comment";
        image.extra_data = 99;
        image.source = b"retained source token";
        let mut channel = reset_holder(3, 0, 7);
        load_channel_image_into(
            &image,
            super::super::record_image::ChannelKind::Call,
            0,
            &mut channel,
        )
        .unwrap();
        channel.set_name(b"DOCUMENT NAME   ");
        store_channel_image_into(
            &mut image,
            super::super::record_image::ChannelKind::Call,
            1,
            &mut channel,
        )
        .unwrap();
        let mut reread = reset_holder(3, 1, 7);
        load_channel_image_into(
            &image,
            super::super::record_image::ChannelKind::Call,
            1,
            &mut reread,
        )
        .unwrap();
        assert_eq!(reread.name(), b"DOCUMENT NAME   ");
        channel.set_valid(false);
        store_callsign_storage_into(&mut image, 299, &channel).unwrap();
        load_callsign_storage_into(&image, 299, &mut reread).unwrap();
        assert_eq!(reread.name(), b"DOCUMENT NAME   ");
        assert_eq!(image.comment_bytes(), b"retained comment");
        assert_eq!(image.extra_data(), 99);
        assert_eq!(image.original_bytes(), b"retained source token");
        let before = image.memory().to_vec();
        let holder_before = reread.clone();
        assert!(load_callsign_storage_into(&image, 300, &mut reread).is_err());
        assert!(reread == holder_before);
        assert!(store_callsign_storage_into(&mut image, usize::MAX, &channel).is_err());
        assert_eq!(image.memory(), before);
    }

    #[test]
    fn history_document_reverse_map_projection_and_delete_leave_physical_payloads_untouched() {
        let mut memory = vec![0xa6; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        memory[0x3c2ff..0x3c331].fill(255);
        memory[0x3c2ff..0x3c305].copy_from_slice(&[3, 3, 1, 255, 6, 0]);
        let basic = 0x309c3 + 0x32;
        memory[basic..basic + 8].copy_from_slice(b"FIRST  B");
        memory[basic + 36..basic + 40].copy_from_slice(&123_456_789u32.to_le_bytes());
        memory[basic + 40] = 255;
        memory[basic + 48..basic + 56].copy_from_slice(b"SECOND B");
        let gps = 0x309c3 + 0xd7a;
        memory[gps + 29] = 255;
        memory[gps + 49..gps + 92].fill(0x1b);
        memory[0x3c331..0x3c33b].copy_from_slice(&[4, 255, 7, 1, 255, 255, 255, 255, 255, 255]);
        let mut image = super::super::Icf::from_memory(memory).unwrap();
        let mut h = reset_holder(0x0e, 3, 0xdead_beef);
        load_receive_history_storage_into(&image, 3, &mut h).unwrap();
        assert!(h.valid());
        let fields = receive_history_fields(&h);
        let mut names = std::collections::BTreeSet::new();
        for (name, _) in &fields {
            assert!(names.insert(*name));
        }
        assert_eq!(fields.len(), 38);
        match &fields.iter().find(|(name, _)| *name == "caller").unwrap().1 {
            HistoryValue::Bytes(bytes) => assert_eq!(*bytes, b"FIRST  B"),
            _ => panic!("caller must be bytes"),
        }
        match &fields
            .iter()
            .find(|(name, _)| *name == "frequency-hz")
            .unwrap()
            .1
        {
            HistoryValue::Unsigned(n) => assert_eq!(*n, 123_456_789),
            _ => panic!("frequency must be unsigned"),
        }
        match &fields.iter().find(|(name, _)| *name == "mode").unwrap().1 {
            HistoryValue::Unsigned(n) => assert_eq!(*n, 25),
            _ => panic!("mode must be unsigned"),
        }
        drop(fields);
        load_receive_history_storage_into(&image, 2, &mut h).unwrap();
        assert!(!h.valid());
        let before = h.clone();
        assert!(load_receive_history_storage_into(&image, 50, &mut h).is_err());
        assert!(h == before);
        let mut expected = image.memory().to_vec();
        expected[0x3c331..0x3c33b].copy_from_slice(&[7, 1, 255, 255, 255, 255, 255, 255, 255, 255]);
        assert!(delete_history_storage(&mut image, 0x0f, 0, 1).unwrap());
        assert_eq!(image.memory(), expected);
        expected[0x3c2ff..0x3c305].copy_from_slice(&[1, 1, 255, 255, 4, 0]);
        assert!(delete_history_storage(&mut image, 0x0e, 1, 2).unwrap());
        assert_eq!(image.memory(), expected);
        assert!(!history_selection_present(&image, 0x0e, 2, 2).unwrap());
        assert!(!delete_history_storage(&mut image, 0x0e, 2, 2).unwrap());
        assert!(delete_history_storage(&mut image, 0x0e, 0, 50).is_err());
        assert_eq!(image.memory(), expected);
    }

    #[test]
    fn channel_literal_load_preserves_unwritten_fields_and_handles_wrapped_polarity() {
        let mut raw = [0xa5; 83];
        raw[0] = 0xfb;
        raw[34..38].copy_from_slice(&[0x78, 0x56, 0x34, 0x12]);
        raw[38..46].copy_from_slice(&[0x7b, 0xdd, 4, 250, 3, 55, 56, 57]);
        raw[63..67].copy_from_slice(&[0xff, 0xff, 0xff, 0xff]);
        raw[67..83].copy_from_slice(b"AB\0DEFGHIJKLMNOP");
        let mut h = DecodedRecord::from_bytes([0x93; 616]);
        let mut expected = *h.as_bytes();
        for (at, bytes) in [
            (0x24, &[1, 1][..]),
            (0x26, b"AB\0DEFGHIJKLMNOP"),
            (0x54, &[3, 0, 0, 0]),
            (0x3c, &[0x78, 0x56, 0x34, 0x12]),
            (0x58, &[13, 0, 0, 0]),
            (0x60, &[11, 0, 0, 0]),
            (0x5c, &[1, 0, 0, 0]),
            (0x44, &[2, 0, 0, 0]),
            (0x48, &[255; 4]),
            (0x68, &[6, 0, 0, 0]),
            (0x6c, &[55, 0, 0, 0]),
            (0x70, &[56, 0, 0, 0]),
            (0x74, &[57, 0, 0, 0]),
            (0x78, &[2, 0, 0, 0]),
        ] {
            expected[at..at + bytes.len()].copy_from_slice(bytes);
        }
        decode_channel_into(&mut h, &raw, 1, Some(0xd5)).unwrap();
        assert_eq!(h.as_bytes(), &expected);
        raw[38] = 0x94;
        raw[40] = 2;
        raw[41] = 255;
        raw[42..49].copy_from_slice(&[0x83, 0x0a, 0x1c, 0x48, 0xb1, 0xa3, 0xc8]);
        raw[49..56].copy_from_slice(&[0xa9, 0x16, 0x9d, 0x46, 0x0c, 0x50, 0x42]);
        raw[56..63].copy_from_slice(&[0; 7]);
        decode_channel_into(&mut h, &raw, 1, None).unwrap();
        assert_eq!((h.mode(), h.filter(), h.word(0x54)), (16, 4, 0));
        assert_eq!((h.word(0x80), h.word(0x84)), (4, 255));
        assert_eq!(&h.bytes[0x88..0x90], b"ABCDEFGH");
        assert_eq!(&h.bytes[0xa4..0xac], b"TEST01 B");
        assert_eq!(&h.bytes[0xc4..0xcc], &[0; 8]);
        assert_eq!(&h.bytes[0x68..0x7c], &expected[0x68..0x7c]);
        let before = *h.as_bytes();
        assert!(decode_channel_into(&mut h, &raw, 2, None).is_err());
        assert_eq!(h.as_bytes(), &before);
    }

    fn channel_saving_holder() -> DecodedRecord {
        let mut h = DecodedRecord::from_bytes([0x93; 616]);
        h.bytes[0x24] = 0; // Source side must not override destination side1.
        h.bytes[0x25] = 3;
        h.bytes[0x26..0x36].copy_from_slice(b"AB\0DEFGHIJKLMNOP");
        h.put_word(0x54, 1);
        h.put_word(0x3c, 0x1234_5678);
        h.set_mode(0x10d);
        h.set_data(0x100);
        h.set_filter(0x1a);
        h.put_word(0x44, 0x103);
        h.set_offset_hz(u32::MAX);
        h.put_word(0x68, 0x108);
        h.set_tx_tone(0x1fe);
        h.set_rx_tone(0x102);
        h.put_word(0x74, 0x123);
        h.put_word(0x78, 0x103);
        h
    }

    #[test]
    fn channel_saver_checks_each_literal_failure_prefix_and_destination_side() {
        use ChannelStage as S;
        let stages = [
            S::Empty,
            S::Name,
            S::Select,
            S::Split,
            S::Frequency,
            S::Mode,
            S::Data,
            S::Filter,
            S::Duplex,
            S::Offset,
            S::Tone,
            S::TxTone,
            S::RxTone,
            S::Dtcs,
            S::TxPolarity,
            S::RxPolarity,
        ];
        let stores: [(usize, usize, &[u8]); 14] = [
            (1, 67, b"AB\0DEFGHIJKLMNOP"),
            (3, 0, &[0xa7]),
            (4, 34, &[0x78, 0x56, 0x34, 0x12]),
            (5, 38, &[0x75]),
            (6, 39, &[0xa4]),
            (7, 38, &[0x7a]),
            (8, 39, &[0xa6]),
            (9, 63, &[0x1c, 0x96, 0x98, 0]),
            (10, 40, &[6]),
            (11, 43, &[0xfe]),
            (12, 44, &[2]),
            (13, 45, &[0x23]),
            (14, 41, &[0]),
            (15, 42, &[0]),
        ];
        for stop in 0..=16 {
            let mut h = channel_saving_holder();
            let before = *h.as_bytes();
            let mut raw = [0xa5; 83];
            let mut flag = 0xb5;
            let mut committed = raw;
            let mut committed_flag = flag;
            let mut seen = Vec::new();
            let result = encode_channel_into(&mut h, &mut raw, 1, Some(&mut flag), |event| {
                seen.push(event.stage());
                if seen.len() - 1 == stop {
                    return Err(Error::Invalid {
                        line: 0,
                        reason: "injected channel failure",
                    });
                }
                match event {
                    ChannelWrite::Body { offset, bytes, .. } => {
                        committed[offset..offset + bytes.len()].copy_from_slice(bytes)
                    }
                    ChannelWrite::Flag { byte, .. } => committed_flag = byte,
                }
                Ok(())
            });
            assert_eq!(result.is_ok(), stop == 16);
            assert_eq!(seen, stages[..(stop + 1).min(16)]);
            let mut expected = [0xa5; 83];
            for (stage, at, bytes) in stores {
                if stage < stop {
                    expected[at..at + bytes.len()].copy_from_slice(bytes);
                }
            }
            assert_eq!(committed, expected, "committed channel prefix {stop}");
            assert_eq!(
                committed_flag,
                if stop > 2 {
                    0x74
                } else if stop > 0 {
                    0xb4
                } else {
                    0xb5
                }
            );
            assert_eq!(h.as_bytes(), &before);
            if stop == 16 {
                assert_eq!(raw, expected);
            }
        }
    }

    #[test]
    fn channel_all_mode_bytes_keep_unused_windows_and_full_dword_controls() {
        for mode in 0..=255u8 {
            for high in [0, 0x100, 0x8000_0000, 0xffff_ff00] {
                let mut h = channel_saving_holder();
                h.set_mode(high | u32::from(mode));
                h.put_word(0x44, 4);
                h.put_word(0x78, 5);
                h.bytes[0xc4..0xcc].fill(b' ');
                let mut raw = [0xa5; 83];
                encode_channel_into(&mut h, &mut raw, 0, None, |_| Ok(())).unwrap();
                let stored_mode = match mode {
                    0..=3 => mode,
                    6 => 4,
                    7 => 5,
                    8 => 6,
                    13 => 7,
                    15 => 8,
                    16 => 9,
                    _ => 15,
                };
                assert_eq!(raw[5], stored_mode * 16 + 10);
                assert_eq!(raw[6], 0xa4);
                assert_eq!(&raw[34..67], &[0xa5; 33]);
                assert_eq!(h.mode(), high | u32::from(mode));
                assert_eq!(h.offset_hz(), u32::MAX);
                match mode {
                    13 | 14 => {
                        assert_eq!(&raw[7..13], &[6, 1, 1, 0xfe, 2, 0x23]);
                        assert_eq!(&raw[13..30], &[0xa5; 17]);
                    }
                    16 => {
                        assert_eq!(raw[7], 255);
                        assert_eq!(raw[8], 0x93);
                    }
                    17 => assert_eq!(&raw[7..9], &[0xa5; 2]),
                    _ => assert_eq!(&raw[7..30], &[0xa5; 23]),
                }
            }
        }
        for polarity in [0, 2, 3, 4, 5, 0x103, u32::MAX] {
            for duplex in [0, 1, 2, 3, 4, 0x103, 0x104, u32::MAX] {
                let mut h = channel_saving_holder();
                h.put_word(0x78, polarity);
                h.put_word(0x44, duplex);
                let mut raw = [0xa5; 83];
                encode_channel_into(&mut h, &mut raw, 0, None, |_| Ok(())).unwrap();
                assert_eq!(
                    &raw[8..10],
                    match polarity {
                        3 => &[1, 0],
                        4 => &[0, 1],
                        5 => &[1, 1],
                        _ => &[0, 0],
                    }
                );
                let expected = match duplex {
                    3 => 1,
                    4 => 2,
                    other => other % 4,
                };
                assert_eq!((raw[6] >> 1) & 3, expected as u8);
            }
        }
    }

    #[test]
    fn channel_valid_zero_and_bad_side_do_not_write_body() {
        for normal in [false, true] {
            let mut h = channel_saving_holder();
            h.set_valid(false);
            let mut raw = [0xa5; 83];
            let mut flag = 0xb4;
            let mut count = 0;
            encode_channel_into(&mut h, &mut raw, 0, normal.then_some(&mut flag), |event| {
                assert_eq!(event.stage(), ChannelStage::Empty);
                count += 1;
                Ok(())
            })
            .unwrap();
            assert_eq!(raw, [0xa5; 83]);
            assert_eq!(count, usize::from(normal));
            assert_eq!(flag, if normal { 0xb5 } else { 0xb4 });
            let before = *h.as_bytes();
            assert!(
                encode_channel_into(&mut h, &mut raw, usize::MAX, Some(&mut flag), |_| panic!(
                    "invalid side must not emit"
                ))
                .is_err()
            );
            assert_eq!(h.as_bytes(), &before);
            assert_eq!(raw, [0xa5; 83]);
        }
    }

    #[test]
    fn channel_dv_post_pack_correction_is_ordered_and_mutates_only_rpt1() {
        for mode in [16, 17, 0x110, 0x111] {
            for rpt2 in [[b' '; 8], [0xa0; 8], *b"TEST01 B"] {
                for last in *b"GB" {
                    let mut h = channel_saving_holder();
                    h.set_mode(mode);
                    h.bytes[0x88..0x90].copy_from_slice(b"ABCDEFGH");
                    h.bytes[0xa4..0xac].copy_from_slice(&[
                        0xd4,
                        0xc5,
                        0xd3,
                        0xd4,
                        0xb0,
                        0xb1,
                        0xa0,
                        last | 0x80,
                    ]);
                    h.bytes[0xc4..0xcc].copy_from_slice(&rpt2);
                    let before = *h.as_bytes();
                    let mut raw = [0xa5; 83];
                    let mut seen = Vec::new();
                    encode_channel_into(&mut h, &mut raw, 0, None, |event| {
                        seen.push(event.stage());
                        Ok(())
                    })
                    .unwrap();
                    let corrected = rpt2 != [b' '; 8];
                    let mut expected = before;
                    if corrected {
                        expected[0xa4..0xac].copy_from_slice(b"TEST01 B");
                        expected[0xab] = if last == b'G' { b' ' } else { b'B' };
                    }
                    assert_eq!(h.as_bytes(), &expected);
                    assert_eq!(
                        seen.contains(&ChannelStage::CorrectRepeater1),
                        corrected && last == b'G'
                    );
                    assert_eq!(&raw[9..16], &[0x83, 0x0a, 0x1c, 0x48, 0xb1, 0xa3, 0xc8]);
                    if mode as u8 == 17 {
                        assert_eq!(&raw[7..9], &[0xa5; 2]);
                    }
                }
            }
        }
        let mut h = channel_saving_holder();
        h.set_mode(16);
        h.bytes[0xc4..0xcc].fill(b'A');
        h.bytes[0xa4..0xac].copy_from_slice(b"TEST01 G");
        let mut raw = [0xa5; 83];
        let mut committed = raw;
        let result = encode_channel_into(&mut h, &mut raw, 0, None, |event| {
            if event.stage() == ChannelStage::CorrectRepeater1 {
                return Err(Error::Invalid {
                    line: 0,
                    reason: "injected post-pack failure",
                });
            }
            if let ChannelWrite::Body { offset, bytes, .. } = event {
                committed[offset..offset + bytes.len()].copy_from_slice(bytes);
            }
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(&h.bytes[0xa4..0xac], b"TEST01  ");
        assert_eq!(unpack_callsign(&committed[16..23]), *b"TEST01 G");
        assert_eq!(unpack_callsign(&raw[16..23]), *b"TEST01  ");
    }

    #[test]
    fn gps_opaque_windows_ordered_failure_prefixes_and_valid_zero_are_distinct() {
        let raw: [u8; 34] = std::array::from_fn(|i| i as u8);
        let mut h = DecodedRecord::from_bytes([0xa5; 616]);
        decode_gps_into(&mut h, &raw, 26);
        let mut expected = [0xa5; 616];
        expected[0x244..0x254].copy_from_slice(&[
            18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33,
        ]);
        expected[0x254..0x25a].copy_from_slice(&[8, 9, 10, 11, 12, 13]);
        expected[0x25a..0x262].copy_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7]);
        expected[0x264..0x268].copy_from_slice(&[14, 15, 16, 17]);
        expected[0x20..0x24].copy_from_slice(&[26, 0, 0, 0]);
        assert_eq!(h.as_bytes(), &expected);
        let stages = [(18, 16), (8, 3), (11, 3), (0, 4), (4, 4), (14, 4)];
        for stop in 0..=6 {
            let mut scratch = [0x93; 34];
            let mut committed = scratch;
            let mut seen = Vec::new();
            let result = encode_gps_into(&h, &mut scratch, |at, bytes| {
                seen.push((at, bytes.len()));
                if seen.len() - 1 == stop {
                    return Err(Error::Invalid {
                        line: 0,
                        reason: "injected GPS failure",
                    });
                }
                committed[at..at + bytes.len()].copy_from_slice(bytes);
                Ok(())
            });
            assert_eq!(result.is_ok(), stop == 6);
            assert_eq!(seen, stages[..(stop + 1).min(6)]);
            let mut expected = [0x93; 34];
            for (at, count) in &stages[..stop] {
                expected[*at..at + count].copy_from_slice(&raw[*at..at + count]);
            }
            assert_eq!(committed, expected);
        }
        h.set_valid(false);
        let before = *h.as_bytes();
        let mut raw = [0x93; 34];
        encode_gps_into(&h, &mut raw, |_, _| panic!("invalid GPS must not store")).unwrap();
        assert_eq!(raw, [0x93; 34]);
        assert_eq!(h.as_bytes(), &before);
    }

    #[test]
    fn ur_copies_plus88_not_history_plus18c_and_saves_even_valid_zero() {
        let mut h = DecodedRecord::from_bytes([0xa5; 616]);
        let name = *b"AB\0DEFGHIJKLMNOP";
        let call = [0xc1, 0xc2, 0xc3, 0xc4, 0, 0x20, 0x20, 0xc7];
        decode_callsign_into(&mut h, &name, &call);
        let mut expected = [0xa5; 616];
        expected[0x26..0x36].copy_from_slice(&name);
        expected[0x88..0x90].copy_from_slice(&call);
        assert_eq!(h.as_bytes(), &expected);
        for valid in [false, true] {
            h.set_valid(valid);
            for stop in 0..=2 {
                let mut out_name = [0x93; 16];
                let mut out_call = [0x93; 8];
                let mut count = 0;
                let result = encode_callsign_into(&h, |event| {
                    count += 1;
                    if count - 1 == stop {
                        return Err(Error::Invalid {
                            line: 0,
                            reason: "injected UR failure",
                        });
                    }
                    match event {
                        CallsignWrite::Name(bytes) => {
                            assert_eq!(count, 1);
                            out_name = *bytes;
                        }
                        CallsignWrite::Call(bytes) => {
                            assert_eq!(count, 2);
                            out_call = *bytes;
                        }
                    }
                    Ok(())
                });
                assert_eq!(result.is_ok(), stop == 2);
                assert_eq!(out_name, if stop > 0 { name } else { [0x93; 16] });
                assert_eq!(out_call, if stop > 1 { call } else { [0x93; 8] });
                assert_eq!(&h.bytes[0x18c..0x194], &[0xa5; 8]);
            }
        }
    }

    #[test]
    fn history_base_misses_dd_common_exit_and_raw_fm_frequency_keep_incoming_bytes() {
        for kind in [0x0f, 0x10, 0x11, 0x12, 0x13, 0x14] {
            for present in [false, true] {
                let mut h = DecodedRecord::from_bytes([0xa5; 616]);
                let raw = if kind == 0x10 {
                    &b"TEST01 B\x78\x56\x34\x12"[..]
                } else {
                    &b"TEST01 B"[..]
                };
                let mut expected = [0xa5; 616];
                if !matches!(kind, 0x11 | 0x13) {
                    expected[12] = if present { 0xa5 } else { 0xa4 };
                    if kind != 0x14 {
                        expected[0x194] = 0;
                    }
                    if present {
                        expected[0x18c..0x194].copy_from_slice(b"TEST01 B");
                        if kind == 0x10 {
                            expected[0x3c..0x40].copy_from_slice(&[0x78, 0x56, 0x34, 0x12]);
                        }
                    }
                }
                decode_history_base_into(&mut h, kind, present.then_some(raw)).unwrap();
                assert_eq!(h.as_bytes(), &expected);
            }
        }
        for (kind, source) in [
            (0x0f, &[0; 7][..]),
            (0x10, &[0; 8][..]),
            (0x12, &[0; 9][..]),
            (0x14, &[0; 12][..]),
            (0x0e, &[0; 8][..]),
            (u32::MAX, &[][..]),
        ] {
            let mut h = DecodedRecord::from_bytes([0xa5; 616]);
            assert!(decode_history_base_into(&mut h, kind, Some(source)).is_err());
            assert_eq!(h.as_bytes(), &[0xa5; 616]);
        }
        for kind in [0x11, 0x13] {
            let mut h = DecodedRecord::from_bytes([0x92; 616]);
            decode_history_base_into(&mut h, kind, Some(&[])).unwrap();
            assert_eq!(h.as_bytes(), &[0x92; 616]);
        }
    }

    #[test]
    fn history_supplement_native_query_order_live_count_and_signed_comparisons() {
        use HistoryLoad as L;
        use HistoryQuery as Q;
        let call = *b"RAW CALL";
        let from_dv = Q::FromDv { call };
        let from_fm = |limit| Q::FromFm {
            call,
            frequency_hz: 0x1234_5678,
            limit,
        };
        let ur = Q::Ur { call };
        let upper = Q::UpperUr { call };
        let to = Q::ToRepeater { call };
        let count = Q::RepeaterCount;
        let cases = [
            (0x0f, vec![(from_dv, 5), (count, 6)], Some(L::Repeater(5))),
            (0x0f, vec![(from_dv, 5), (count, 5)], None),
            (0x0f, vec![(from_dv, -1), (count, 0)], Some(L::Repeater(-1))),
            (
                0x10,
                vec![(count, 2), (from_fm(2), 3), (count, 4)],
                Some(L::Repeater(3)),
            ),
            (0x10, vec![(count, 8), (from_fm(8), 3), (count, 3)], None),
            (
                0x10,
                vec![(count, -1), (from_fm(-1), 0), (count, 1)],
                Some(L::Repeater(0)),
            ),
            (0x12, vec![(ur, 299)], Some(L::Ur(299))),
            (0x12, vec![(ur, 0)], Some(L::Ur(0))),
            (0x12, vec![(ur, 300), (upper, 299)], Some(L::Ur(299))),
            (
                0x12,
                vec![(ur, 300), (upper, 300), (to, 4), (count, 5)],
                Some(L::Repeater(4)),
            ),
            (0x12, vec![(ur, 301), (to, 5), (count, 5)], None),
            (
                0x12,
                vec![(ur, -1), (to, -1), (count, 0)],
                Some(L::Repeater(-1)),
            ),
            (
                0x12,
                vec![(ur, 300), (upper, -1), (to, 0), (count, 1)],
                Some(L::Repeater(0)),
            ),
        ];
        for (kind, replies, expected_load) in cases {
            let mut h = DecodedRecord::from_bytes([0xa5; 616]);
            h.set_header(kind, 9, 0xdead_beef);
            let header: [u8; 12] = h.bytes[..12].try_into().unwrap();
            let mut replies = replies.into_iter();
            let mut loads = Vec::new();
            let fm = b"RAW CALL\x78\x56\x34\x12";
            let raw = if kind == 0x10 { &fm[..] } else { &call[..] };
            decode_history_into(
                &mut h,
                kind,
                Some(raw),
                |request| {
                    let (expected, value) =
                        replies.next().expect("unexpected native history query");
                    assert_eq!(request, expected);
                    Ok(value)
                },
                |target, holder| {
                    assert_eq!(&holder.bytes[..12], &header);
                    assert_eq!(
                        holder.bytes[0x194], 1,
                        "supplement flag precedes native load"
                    );
                    loads.push(target);
                    match target {
                        L::Ur(_) => decode_callsign_into(holder, b"UR SOURCE NAME  ", b"UR CALL "),
                        L::Repeater(_) => {
                            let mut row = [0x93; 52];
                            row[..4].copy_from_slice(&[4, 3, 2, 1]);
                            row[18..34].copy_from_slice(b"REPEATER SOURCE ");
                            decode_repeater_into(holder, &row, &[0; 7], 0x100, 26);
                        }
                    }
                    Ok(())
                },
            )
            .unwrap();
            assert!(replies.next().is_none());
            assert_eq!(loads, expected_load.into_iter().collect::<Vec<_>>());
            assert_eq!(&h.bytes[..12], &header);
            assert!(h.valid());
            assert_eq!(&h.bytes[0x18c..0x194], b"RAW CALL");
            assert_eq!(h.bytes[0x194], u8::from(expected_load.is_some()));
            match expected_load {
                Some(L::Ur(_)) => {
                    assert_eq!(&h.bytes[0x26..0x36], b"UR SOURCE NAME  ");
                    assert_eq!(&h.bytes[0x88..0x90], b"UR CALL ");
                    assert_eq!(h.word(0x3c), 0xa5a5_a5a5);
                }
                Some(L::Repeater(_)) => {
                    assert_eq!(&h.bytes[0x26..0x36], b"REPEATER SOURCE ");
                    assert_eq!(h.word(0x3c), 0x0102_0304);
                    assert_eq!(h.group(), 26);
                }
                None => {
                    assert_eq!(&h.bytes[0x26..0x36], &[0xa5; 16]);
                    assert_eq!(
                        h.word(0x3c),
                        if kind == 0x10 {
                            0x1234_5678
                        } else {
                            0xa5a5_a5a5
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn history_supplement_failures_keep_base_flag_and_partial_native_load_prefix() {
        use HistoryQuery as Q;
        let call = *b"RAW CALL";
        let count = Q::RepeaterCount;
        let cases = [
            (
                0x0f,
                vec![(Q::FromDv { call }, 2), (count, 3)],
                HistoryLoad::Repeater(2),
            ),
            (
                0x10,
                vec![
                    (count, 4),
                    (
                        Q::FromFm {
                            call,
                            frequency_hz: 0x1234_5678,
                            limit: 4,
                        },
                        2,
                    ),
                    (count, 3),
                ],
                HistoryLoad::Repeater(2),
            ),
            (
                0x12,
                vec![
                    (Q::Ur { call }, 300),
                    (Q::UpperUr { call }, 300),
                    (Q::ToRepeater { call }, 2),
                    (count, 3),
                ],
                HistoryLoad::Repeater(2),
            ),
            (
                0x12,
                vec![(Q::Ur { call }, 300), (Q::UpperUr { call }, 299)],
                HistoryLoad::Ur(299),
            ),
        ];
        for (kind, replies, target) in cases {
            for stop in 0..=replies.len() {
                let mut h = DecodedRecord::from_bytes([0xa4; 616]);
                h.set_header(kind, 9, 0xdead_beef);
                let raw = if kind == 0x10 {
                    &b"RAW CALL\x78\x56\x34\x12"[..]
                } else {
                    &call[..]
                };
                let mut base = h.clone();
                decode_history_base_into(&mut base, kind, Some(raw)).unwrap();
                let mut queried = 0;
                let mut loads = 0;
                let result = decode_history_into(
                    &mut h,
                    kind,
                    Some(raw),
                    |request| {
                        let (expected, value) = replies[queried];
                        assert_eq!(request, expected);
                        queried += 1;
                        if queried - 1 == stop {
                            return Err(Error::Invalid {
                                line: 0,
                                reason: "injected history query failure",
                            });
                        }
                        Ok(value)
                    },
                    |actual, holder| {
                        assert_eq!(actual, target);
                        assert_eq!(holder.bytes[0x194], 1);
                        loads += 1;
                        holder.bytes[0x26..0x2b].copy_from_slice(b"FIRST");
                        Err(Error::Invalid {
                            line: 0,
                            reason: "injected history load failure",
                        })
                    },
                );
                assert!(result.is_err());
                assert_eq!(queried, (stop + 1).min(replies.len()));
                let mut expected = *base.as_bytes();
                if stop == replies.len() {
                    expected[0x194] = 1;
                    expected[0x26..0x2b].copy_from_slice(b"FIRST");
                }
                assert_eq!(loads, usize::from(stop == replies.len()));
                assert_eq!(
                    h.as_bytes(),
                    &expected,
                    "history {kind:x} failure stage {stop}"
                );
            }
        }
        for kind in [0x0f, 0x10, 0x12, 0x14, 0x11, 0x13] {
            let mut h = DecodedRecord::from_bytes([0xa5; 616]);
            decode_history_into(
                &mut h,
                kind,
                None,
                |_| panic!("map miss/DD must not query"),
                |_, _| panic!("map miss/DD must not load"),
            )
            .unwrap();
            assert_eq!(h.valid(), matches!(kind, 0x11 | 0x13));
        }
        let mut reflector = DecodedRecord::from_bytes([0xa5; 616]);
        decode_history_into(
            &mut reflector,
            0x14,
            Some(&call),
            |_| panic!("reflector must not query"),
            |_, _| panic!("reflector must not load"),
        )
        .unwrap();
        assert_eq!(&reflector.bytes[0x18c..0x194], &call);
        assert_eq!(reflector.bytes[0x194], 0xa5);
        let mut bad = DecodedRecord::from_bytes([0xa5; 616]);
        assert!(
            decode_history_into(
                &mut bad,
                0x10,
                Some(&call),
                |_| panic!("bad width must not query"),
                |_, _| panic!("bad width must not load")
            )
            .is_err()
        );
        assert_eq!(bad.as_bytes(), &[0xa5; 616]);
    }

    #[test]
    fn receive_history_literal_windows_all_tags_subtypes_and_state_precedence() {
        let mut basic = [0x61; 48];
        basic[..8].copy_from_slice(b"CALLER B");
        basic[8..12].copy_from_slice(b"SFX ");
        basic[12..20].copy_from_slice(b"RPTONE B");
        basic[20..28].copy_from_slice(b"RPTTWO G");
        basic[28..36].copy_from_slice(b"CALLED B");
        basic[36..40].copy_from_slice(&[0x78, 0x56, 0x34, 0x12]);
        basic[40] = 255;
        basic[41..48].copy_from_slice(&[0x31, 0x12, 0x26, 0x23, 0x59, 0x58, 0xfe]);
        let message = *b"0123456789ABCDEFGHIJ";
        let mut gps: [u8; 92] = std::array::from_fn(|i| (i * 73 + 19) as u8);
        gps[29] = 255;
        let mut expected = [0xa5; 616];
        for (at, bytes) in [
            (0x195, &[0xfe][..]),
            (0x198, &[0x78, 0x56, 0x34, 0x12, 25]),
            (0x19d, b"CALLER BSFX CALLED BRPTONE BRPTTWO G"),
            (0x1c1, &message),
            (0x1d5, &[1, 1, 1, 0x31, 0x12, 0x26, 0x23, 0x59, 0x58]),
        ] {
            expected[at..at + bytes.len()].copy_from_slice(bytes);
        }
        expected[0x1de..0x1e6].copy_from_slice(&[19, 92, 165, 238, 55, 128, 201, 18]);
        expected[0x1e8..0x1f0].copy_from_slice(&[91, 164, 237, 54, 17, 90, 127, 200]);
        expected[0x1f0..0x1f4].fill(255);
        expected[0x1f4..0x1f8].copy_from_slice(&[52, 125, 198, 15]);
        expected[0x1f8..0x1fc].fill(255);
        expected[0x1fc..0x205].fill(b' ');
        expected[0x205..0x219].fill(255);
        expected[0x207] = 0x7f;
        expected[0x219..0x244].copy_from_slice(&gps[49..92]);
        let mut h = DecodedRecord::from_bytes([0xa5; 616]);
        decode_receive_history_into(&mut h, &basic, &message, 0x69, &gps);
        assert_eq!(h.as_bytes(), &expected);
        for tag in 0..=255u8 {
            for subtype in 0..=255u8 {
                gps[29] = tag;
                gps[40] = subtype;
                let mut want = expected;
                match tag {
                    1 | 6 => want[0x1f0..0x1f4].copy_from_slice(&[161, 234, 51, 124]),
                    2 => want[0x1f8..0x1fc].copy_from_slice(&[161, 234, 51, 124]),
                    3 | 4 => {
                        want[0x1fc..0x205]
                            .copy_from_slice(&[161, 234, 51, 124, 197, 14, 87, 160, 233]);
                        want[0x205] = 50;
                        match subtype {
                            1 => want[0x1f0..0x1f4].copy_from_slice(&[196, 13, 86, 159]),
                            2 => want[0x1f8..0x1fc].copy_from_slice(&[196, 13, 86, 159]),
                            _ => {}
                        }
                    }
                    5 => {
                        want[0x20e..0x214].copy_from_slice(&[161, 234, 51, 124, 197, 14]);
                        want[0x206..0x20e]
                            .copy_from_slice(&[87, 160, 233, 50, subtype, 196, 13, 86]);
                        want[0x214..0x219].copy_from_slice(&[232, 49, 122, 195, 159]);
                    }
                    _ => {}
                }
                decode_receive_history_into(&mut h, &basic, &message, 0x69, &gps);
                assert_eq!(h.as_bytes(), &want, "Rx tag {tag} subtype {subtype}");
            }
        }
        for state in 0..=255u8 {
            decode_receive_history_into(&mut h, &basic, &message, state, &gps);
            let expected = match state & 7 {
                1 => 1,
                2 => 2,
                _ if state & 64 != 0 => 3,
                _ => 0,
            };
            assert_eq!(
                &h.bytes[0x1d5..0x1d8],
                &[expected, (state >> 5) & 1, (state >> 3) & 1]
            );
        }
    }

    #[test]
    fn holder_header_body_and_named_fields_have_independent_literal_windows() {
        let source = DecodedRecord::from_bytes(std::array::from_fn(|i| (i * 73 + 19) as u8));
        let mut target = DecodedRecord::from_bytes([0xa6; 616]);
        target.set_header(0x1122_3344, u32::MAX, 0xdead_beef);
        let header = [
            0x44, 0x33, 0x22, 0x11, 0xff, 0xff, 0xff, 0xff, 0xef, 0xbe, 0xad, 0xde,
        ];
        target.copy_body_from(&source);
        assert_eq!(&target.as_bytes()[..12], header);
        assert_eq!(&target.as_bytes()[12..], &source.as_bytes()[12..]);
        assert_eq!(
            (target.kind(), target.index(), target.binding()),
            (0x1122_3344, u32::MAX, 0xdead_beef)
        );
        let mut expected = *target.as_bytes();
        target.set_valid(false);
        expected[12] &= !1;
        assert!(!target.valid());
        assert_eq!(target.as_bytes(), &expected);
        target.set_valid(true);
        expected[12] |= 1;
        assert!(target.valid());
        assert_eq!(target.as_bytes(), &expected);
        target.set_group(1);
        target.set_mode(0x106);
        target.set_data(0x100);
        target.set_filter(255);
        target.set_offset_hz(u32::MAX);
        target.set_tx_tone(0x1122_3344);
        target.set_rx_tone(0x5566_7788);
        for (at, bytes) in [
            (0x20, [1, 0, 0, 0]),
            (0x58, [6, 1, 0, 0]),
            (0x5c, [0, 1, 0, 0]),
            (0x60, [255, 0, 0, 0]),
            (0x48, [255; 4]),
            (0x6c, [0x44, 0x33, 0x22, 0x11]),
            (0x70, [0x88, 0x77, 0x66, 0x55]),
        ] {
            expected[at..at + 4].copy_from_slice(&bytes);
        }
        assert_eq!(target.as_bytes(), &expected);
        assert_eq!(
            (
                target.group(),
                target.mode(),
                target.data(),
                target.filter()
            ),
            (1, 0x106, 0x100, 255)
        );
        assert_eq!(
            (target.offset_hz(), target.tx_tone(), target.rx_tone()),
            (u32::MAX, 0x1122_3344, 0x5566_7788)
        );
        assert_eq!(BODY_BYTES, 604);
    }

    #[test]
    fn repeater_decode_literal_windows_keep_header_flags_common_mode_and_unwritten_tone() {
        let mut raw = [0x57; 52];
        raw[..4].copy_from_slice(&[0x44, 0x33, 0x22, 0x11]);
        raw[4..8].copy_from_slice(&[1, 0xef, 0xcd, 0xab]);
        raw[8..10].copy_from_slice(&[0x12, 0xfe]);
        raw[10] = 0x9e;
        raw[11..18].copy_from_slice(&[0x83, 0x0a, 0x1c, 0x48, 0xb1, 0xa3, 0xc8]);
        raw[18..34].copy_from_slice(b"0123456789ABCDEF");
        raw[34..42].copy_from_slice(b"sub-name");
        raw[42..48].copy_from_slice(&[35, 1, 2, 139, 3, 4]);
        raw[49..51].copy_from_slice(&[0xb8, 0xfc]);
        raw[51] = 0xc5;
        let gateway = [0xa9, 0x16, 0x9d, 0x46, 0x0c, 0x50, 0x42];
        let mut holder = DecodedRecord::from_bytes([0xa7; 616]);
        let mut expected = *holder.as_bytes();
        for (at, bytes) in [
            (0x26, b"0123456789ABCDEF".as_slice()),
            (0x154, b"sub-name"),
            (0x15c, &[1, 0, 0, 0]),
            (0x160, b"ABCDEFGH"),
            (0x168, b"TEST01 B"),
            (0x178, &[0x57, 0, 0, 0]),
            (0x3c, &[0x44, 0x33, 0x22, 0x11]),
            (0x48, &[1, 0xef, 0xcd, 0xab]),
            (0x44, &[1, 0, 0, 0]),
            (0x174, &[3, 0, 0, 0]),
            (0x17c, &[0, 1, 0, 0]),
            (0x180, &[2, 0, 0, 0]),
            (0x25a, &[35, 1, 2, b'S']),
            (0x25e, &[139, 3, 4, b'W']),
            (0x184, &[0xb8, 0xfc, 0xff, 0xff]),
            (0x20, &[42, 0, 0, 0]),
            (0x188, &[0x12, 0xfe, 0, 0]),
        ] {
            expected[at..at + bytes.len()].copy_from_slice(bytes);
        }
        decode_repeater_into(&mut holder, &raw, &gateway, 0x100, 42);
        assert_eq!(holder.as_bytes(), &expected);
        assert!(holder.valid());
        assert_eq!(holder.mode(), 0xa7a7_a7a7);
        raw[10] = 0; // Stored DV -> decoded type2; tone index is not written.
        holder = DecodedRecord::from_bytes([0x63; 616]);
        decode_repeater_into(&mut holder, &raw, &gateway, 0, 49);
        assert_eq!(&holder.as_bytes()[0x178..0x17c], &[0x63; 4]);
        assert_eq!(holder.word(0x15c), 2);
        assert_eq!(holder.group(), 49);
        assert_eq!(&holder.as_bytes()[..13], &[0x63; 13]);
    }

    fn saving_holder() -> DecodedRecord {
        let mut h = DecodedRecord::from_bytes([0x47; 616]);
        h.bytes[0x26..0x36].copy_from_slice(b"0123456789ABCDEF");
        h.bytes[0x154..0x15c].copy_from_slice(b"sub-name");
        h.put_word(0x15c, 0);
        h.bytes[0x160..0x168].copy_from_slice(b"ABCDEFGH");
        h.bytes[0x168..0x170].copy_from_slice(b"TEST01 B");
        h.put_word(0x178, 58);
        h.bytes[0x3c..0x40].copy_from_slice(&[0x78, 0x56, 0x34, 0x12]);
        h.put_word(0x48, u32::MAX);
        h.put_word(0x44, 1);
        h.put_word(0x174, 3);
        h.put_word(0x17c, 0x100);
        h.put_word(0x180, 2);
        h.bytes[0x25a..0x262].copy_from_slice(&[35, 1, 2, b'N', 139, 3, 4, b'W']);
        h.put_word(0x184, 0xffff_fcb8);
        h.put_word(0x188, 0x1234_fffe);
        h
    }

    // Independent literal native stages, including each intermediate shared
    // byte10 value. A test derived from product events would miss a reordering.
    fn literal_body_writes() -> [(RepeaterStage, usize, &'static [u8]); 16] {
        use RepeaterStage as S;
        [
            (S::Name, 18, b"0123456789ABCDEF"),
            (S::SubName, 34, b"sub-name"),
            (S::Type, 10, &[0x65]),
            (
                S::RepeaterCall,
                11,
                &[0x83, 0x0a, 0x1c, 0x48, 0xb1, 0xa3, 0xc8],
            ),
            (S::ToneIndex, 48, &[58]),
            (S::Frequency, 0, &[0x78, 0x56, 0x34, 0x12]),
            (S::Offset, 4, &[255; 4]),
            (S::Duplex, 10, &[0x55]),
            (S::Tone, 10, &[0x5d]),
            (S::Position, 10, &[0x5e]),
            (S::Latitude, 42, &[35, 1, 2]),
            (S::South, 51, &[0x25]),
            (S::Longitude, 45, &[139, 3, 4]),
            (S::West, 51, &[0x65]),
            (S::UtcOffset, 49, &[0xb8, 0xfc]),
            (S::GatewayIndex, 8, &[0xfe, 0xff]),
        ]
    }

    #[test]
    fn repeater_ordered_saver_preserves_every_failure_prefix_and_uses_full_use_dword() {
        use RepeaterStage as S;
        let stages = [
            S::Name,
            S::SubName,
            S::Type,
            S::RepeaterCall,
            S::GatewayCall,
            S::ToneIndex,
            S::Frequency,
            S::Offset,
            S::Duplex,
            S::Tone,
            S::Use,
            S::Position,
            S::Latitude,
            S::South,
            S::Longitude,
            S::West,
            S::UtcOffset,
            S::GatewayIndex,
        ];
        let holder = saving_holder();
        for stop in 0..=18 {
            let mut scratch = [0xa5; 52];
            let mut actual = scratch;
            let mut aux = [0x92; 7];
            let mut use_byte = 0x65;
            let mut visited = Vec::new();
            let result = encode_repeater_into(&holder, &mut scratch, |event| {
                let stage = event.stage();
                visited.push(stage);
                if visited.len() - 1 == stop {
                    return Err(Error::Invalid {
                        line: 0,
                        reason: "injected ordered write failure",
                    });
                }
                match event {
                    RepeaterWrite::Body { offset, bytes, .. } => {
                        actual[offset..offset + bytes.len()].copy_from_slice(bytes)
                    }
                    RepeaterWrite::Gateway(bytes) => aux = bytes,
                    RepeaterWrite::Use(value) => {
                        assert_eq!(value, 0x100);
                        use_byte = (use_byte & !8) | (u8::from(value != 0) * 8);
                    }
                }
                Ok(())
            });
            assert_eq!(visited, &stages[..(stop + 1).min(18)]);
            assert_eq!(result.is_ok(), stop == 18);
            let mut expected = [0xa5; 52];
            for (stage, at, bytes) in literal_body_writes() {
                if stages.iter().position(|s| *s == stage).unwrap() < stop {
                    expected[at..at + bytes.len()].copy_from_slice(bytes);
                }
            }
            assert_eq!(actual, expected, "committed raw prefix at stage {stop}");
            assert_eq!(
                aux,
                if stop > 4 {
                    [0xa9, 0x16, 0x9d, 0x46, 0x0c, 0x50, 0x42]
                } else {
                    [0x92; 7]
                }
            );
            assert_eq!(use_byte, if stop > 10 { 0x6d } else { 0x65 });
            if stop == 18 {
                assert_eq!(scratch, expected);
            }
        }
    }

    #[test]
    fn repeater_all_packed_bits_roundtrip_without_touching_unrelated_holder_bytes() {
        let gateway = [0xa9, 0x16, 0x9d, 0x46, 0x0c, 0x50, 0x42];
        for flags in 0..=255u8 {
            for signs in 0..=255u8 {
                let mut raw = std::array::from_fn(|i| (i * 73 + 19) as u8);
                raw[10] = flags;
                raw[51] = signs;
                let mut holder = DecodedRecord::from_bytes([0xc7; 616]);
                decode_repeater_into(&mut holder, &raw, &gateway, 1, 49);
                assert_eq!(holder.word(0x15c), [2, 0, 1, 3][usize::from(flags >> 6)]);
                assert_eq!(holder.word(0x174), u32::from((flags / 4) % 4));
                assert_eq!(holder.word(0x44), u32::from((flags / 16) % 4));
                assert_eq!(holder.word(0x180), u32::from(flags % 4));
                assert_eq!(&holder.as_bytes()[..12], &[0xc7; 12]);
                let mut target = raw;
                let mut out_gateway = None;
                let mut use_value = None;
                encode_repeater_into(&holder, &mut target, |event| {
                    match event {
                        RepeaterWrite::Gateway(bytes) => out_gateway = Some(bytes),
                        RepeaterWrite::Use(value) => use_value = Some(value),
                        _ => {}
                    }
                    Ok(())
                })
                .unwrap();
                assert_eq!(target, raw);
                assert_eq!(out_gateway, Some(gateway));
                assert_eq!(use_value, Some(1));
            }
        }
    }

    #[test]
    fn repeater_full_dword_tone_gate_is_distinct_from_type_low_byte_and_validity() {
        for valid in [false, true] {
            for (ty, stored, has_tone) in [
                (0, 1, true),
                (1, 2, true),
                (2, 0, false),
                (3, 3, false),
                (0x100, 1, false),
                (0x101, 2, false),
                (0x102, 0, false),
                (0x103, 3, false),
                (255, 3, false),
                (u32::MAX, 3, false),
            ] {
                let mut holder = saving_holder();
                holder.put_word(0x15c, ty);
                holder.set_valid(valid);
                // High bytes are masked before the existing packed7 helper.
                holder.bytes[0x160..0x168]
                    .copy_from_slice(&[0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8]);
                let before = *holder.as_bytes();
                let mut raw = [0xa5; 52];
                let mut events = Vec::new();
                encode_repeater_into(&holder, &mut raw, |event| {
                    events.push(event.stage());
                    Ok(())
                })
                .unwrap();
                assert_eq!(raw[10] >> 6, stored);
                assert_eq!(raw[48], if has_tone { 58 } else { 0xa5 });
                assert_eq!(events.len(), if has_tone { 18 } else { 17 });
                assert_eq!(events.contains(&RepeaterStage::ToneIndex), has_tone);
                assert_eq!(&raw[11..18], &[0x83, 0x0a, 0x1c, 0x48, 0xb1, 0xa3, 0xc8]);
                assert_eq!(holder.as_bytes(), &before);
                assert_eq!(raw[51] & 0x3f, 0x25);
            }
        }
    }
}
