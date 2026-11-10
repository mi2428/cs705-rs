//! Special-memory, scan-edge and preset operations recovered from CS-705 1.30.
//!
//! Bytes, decoded holder values, accepted editor results and document events are
//! distinct. These APIs do not open a radio or claim preset recall on a radio.
//! G is the caller's completed CURRENT default generation, never a factory
//! image built here. Sequential stores intentionally survive a later error.
//!
//! Storage windows (offsets hex, sizes decimal):
//! ```text
//! compact36: freq4 | mode:filter1 | flags1 | polarity1 | tone3 | off4 | code1 | calls21
//! scan27:    high4 | low4 | mode1 | data1 | filter1 | name16    + NOT-valid bitmap
//! user8:     low4 | high4                                    (either zero = empty)
//! preset:    body41 (name16 at +19h) + masks22                (00 != 01 != FF)
//! ```
//! Entry RVAs: 6550/7B2B compact load/save; 6858/7DA3 scan;
//! 7415/80B7 user; CBE44 reset; CAFBC/CC03C/CCAFC/CDECC scan rows;
//! 7B928/7BDB3/7BF41..7DA1F preset fields; B086C preset reset.
//! Shared lookup and callsign codecs are reused; stage-9 offset rounding is
//! intentionally NOT a paste operation. GUI editor and document tails are
//! caller-owned cutpoints, detailed in the special-api handoff.

use super::super::{Error, Icf};
use super::{pack_callsign, unpack_callsign};
use crate::icf::record::{
    decode_digital_squelch, decode_mode, decode_tone, encode_digital_squelch, encode_mode,
    encode_tone,
};
use std::ops::Range;

fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 5, reason }
}
fn window(bytes: &[u8], at: usize, n: usize) -> Result<&[u8], Error> {
    bytes
        .get(
            at..at
                .checked_add(n)
                .ok_or(invalid("special window overflow"))?,
        )
        .ok_or(invalid("special image window outside supplied image"))
}
fn byte(bytes: &[u8], at: usize) -> Result<u8, Error> {
    Ok(window(bytes, at, 1)?[0])
}
fn word32(bytes: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        window(bytes, at, 4)?.try_into().expect("checked DWORD"),
    ))
}
fn put(image: &mut Icf<'_>, at: usize, bytes: &[u8]) -> Result<(), Error> {
    image.patch_memory(&[(at, bytes)])
}
fn put_byte(image: &mut Icf<'_>, at: usize, value: u8) -> Result<(), Error> {
    put(image, at, &[value])
}
fn index(index: usize, count: usize) -> Result<usize, Error> {
    if index < count {
        Ok(index)
    } else {
        Err(invalid("special record index outside domain"))
    }
}

/// An ordered notification request. Queue/synchronous delivery and native
/// receiver success are caller-owned, not implied by creating this value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub event: u32,
    pub synchronous: bool,
    pub first: u32,
    pub second: u32,
    pub payload: Vec<u8>,
}
impl Notification {
    fn queue(event: u32, first: u32, second: u32) -> Self {
        Self {
            event,
            synchronous: false,
            first,
            second,
            payload: Vec::new(),
        }
    }
}

/// Effects at the field/row cutpoint, before core's common document tail.
/// A false editor result need not mean an unchanged context or warning state.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Effects {
    /// Completed logical row/body/mask/selection stores, not byte differences
    /// or the number of patch_memory calls. Notifications are separate.
    pub writes: usize,
    pub callback_flags: u32,
    pub phase: Option<u32>,
    pub warning_bits: u32,
    pub gray: bool,
    pub blocked: bool,
    pub notifications: Vec<Notification>,
    pub node_style_clear: u32,
}
impl Effects {
    /// Stop on receiver failure without rolling back already-stored image
    /// bytes or already-delivered notifications. No catch-and-continue path.
    pub fn dispatch(
        &self,
        mut send: impl FnMut(&Notification) -> Result<(), Error>,
    ) -> Result<(), Error> {
        for event in &self.notifications {
            send(event)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactKind {
    BandStack,
    Memopad,
}
impl CompactKind {
    pub fn displayed_rows(self) -> Range<usize> {
        match self {
            Self::BandStack => 0..45,
            Self::Memopad => 1..11,
        }
    }
    /// 8230/82F0/8339. The 15-entry permutation is represented by its two
    /// transpositions, not copied factory data. Memopad zero is reserved but
    /// remains a readable raw slot; only displayed_rows excludes it.
    pub fn offset(self, row: usize) -> Result<usize, Error> {
        Ok(match self {
            Self::BandStack => {
                index(row, 45)?;
                let band = match row / 3 {
                    10 => 12,
                    11 => 13,
                    12 => 10,
                    13 => 11,
                    n => n,
                };
                0x368eb + 108 * band + 36 * (row % 3)
            }
            Self::Memopad => 0x36f54 + 36 * index(row, 11)?,
        })
    }
}

/// Sixteen display fields in H55670/H55710 suffix order. Normal keyboard
/// edits are blocked at 64609; no public per-field special-sheet setter exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompactField {
    Frequency,
    Duplex,
    Offset,
    Mode,
    Data,
    Filter,
    Tone,
    TxTone,
    RxTone,
    Dtcs,
    Polarity,
    DigitalSquelch,
    DigitalCode,
    Your,
    Repeater1,
    Repeater2,
}
impl CompactField {
    pub fn key(self) -> u16 {
        match self {
            Self::Frequency => 0x107,
            Self::Duplex => 0x10b,
            Self::Offset => 0x10c,
            Self::Mode => 0x108,
            Self::Data => 0x109,
            Self::Filter => 0x10a,
            Self::Tone => 0x10d,
            Self::TxTone => 0x10e,
            Self::RxTone => 0x10f,
            Self::Dtcs => 0x110,
            Self::Polarity => 0x111,
            Self::DigitalSquelch => 0x112,
            Self::DigitalCode => 0x113,
            Self::Your => 0x114,
            Self::Repeater1 => 0x115,
            Self::Repeater2 => 0x116,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayValue {
    Blank,
    Placeholder,
    Number(u32),
    Callsign([u8; 8]),
}

/// Decoded compact 36-byte holder projection, not an 83-byte normal side.
/// mode/tone/D.SQL are source holder codes, NOT strict MemoryMode discriminants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactChannel {
    pub valid: bool,
    pub frequency_hz: u32,
    pub duplex: u32,
    pub offset_hz: u32,
    pub mode: u32,
    pub data: u32,
    pub filter: u32,
    pub tone: u32,
    pub tx_tone: u32,
    pub rx_tone: u32,
    pub dtcs: u32,
    pub polarity: u32,
    pub digital_squelch: u32,
    pub digital_code: u32,
    pub callsigns: [[u8; 8]; 3],
}
impl CompactChannel {
    fn empty() -> Self {
        Self {
            valid: false,
            frequency_hz: u32::MAX,
            duplex: 0,
            offset_hz: 0,
            mode: 0,
            data: 0,
            filter: 0,
            tone: 0,
            tx_tone: 8,
            rx_tone: 8,
            dtcs: 0,
            polarity: 2,
            digital_squelch: 0,
            digital_code: 0,
            callsigns: [[b' '; 8]; 3],
        }
    }
    pub fn read(source: &[u8], kind: CompactKind, row: usize) -> Result<Self, Error> {
        let at = kind.offset(row)?;
        Self::decode(
            window(source, at, 36)?
                .try_into()
                .expect("checked compact window"),
        )
    }
    /// Decode one exact raw36 window. Address, dataset and generation selection
    /// remain caller-owned; this is the same codec used by read, not a new path.
    pub fn decode(b: &[u8; 36]) -> Result<Self, Error> {
        Self::decode_valid(b, word32(b, 0)? != u32::MAX)
    }
    /// Shared raw36 entry with caller-owned validity, including a valid MAX
    /// frequency. All field decoding remains in the single bound decoder.
    pub fn decode_valid(raw: &[u8; 36], valid: bool) -> Result<Self, Error> {
        Self::decode_bound(Some(raw), valid)
    }
    /// Checked shared-kind19/1A/1B entry: validity comes from the caller's real
    /// external flag, not frequency. Invalid skips the source entirely; valid
    /// requires a supplied raw36 even when frequency itself is FFFFFFFF.
    /// The same field decoder serves normal compact and shared records.
    pub fn decode_bound(source: Option<&[u8; 36]>, valid: bool) -> Result<Self, Error> {
        if !valid {
            return Ok(Self::empty());
        }
        let b = source.ok_or(invalid("valid compact record requires its raw36 source"))?;
        let mut h = Self::empty();
        h.frequency_hz = word32(b, 0)?;
        h.valid = true;
        h.mode = decode_mode(b[4] >> 4);
        h.filter = u32::from(b[4] & 15);
        h.data = u32::from(b[5] & 1);
        h.duplex = u32::from(b[5] >> 6);
        if h.mode == 0x11 {
            h.duplex = match h.duplex {
                1 => 3,
                2 => 4,
                n => n,
            };
        }
        h.offset_hz = word32(b, 10)?;
        h.tone = decode_tone((b[5] >> 1) & 7);
        h.tx_tone = u32::from(b[7]);
        h.rx_tone = u32::from(b[8]);
        h.dtcs = u32::from(b[9]);
        h.polarity = u32::from(b[6] & 3) + 2;
        h.digital_squelch = decode_digital_squelch((b[5] >> 4) & 3);
        h.digital_code = u32::from(b[14]);
        for (i, c) in h.callsigns.iter_mut().enumerate() {
            *c = unpack_callsign(&b[15 + 7 * i..22 + 7 * i]);
        }
        Ok(h)
    }
    pub fn display(&self, field: CompactField) -> DisplayValue {
        use CompactField::*;
        if !self.valid {
            return DisplayValue::Blank;
        }
        let n = match field {
            Frequency => self.frequency_hz,
            Duplex => self.duplex,
            Offset => self.offset_hz,
            Mode => self.mode,
            Data if matches!(self.mode, 0 | 1 | 8 | 0xd) => self.data,
            Data => return DisplayValue::Placeholder,
            Filter if matches!(self.mode as u8, 0xf | 0x11) => 0,
            Filter => self.filter,
            Tone | TxTone | RxTone | Dtcs | Polarity if !matches!(self.mode, 0xd | 0xe) => {
                return DisplayValue::Placeholder;
            }
            Tone => self.tone,
            TxTone => self.tx_tone,
            RxTone => self.rx_tone,
            Dtcs => self.dtcs,
            Polarity => self.polarity,
            DigitalSquelch | DigitalCode if self.mode != 0x10 => return DisplayValue::Placeholder,
            DigitalSquelch => self.digital_squelch,
            DigitalCode => self.digital_code,
            Your | Repeater1 | Repeater2 => {
                if !matches!(self.mode, 0x10 | 0x11) {
                    return DisplayValue::Placeholder;
                }
                let i = match field {
                    Your => 0,
                    Repeater1 => 1,
                    _ => 2,
                };
                return DisplayValue::Callsign(self.callsigns[i]);
            }
        };
        DisplayValue::Number(n)
    }
    /// 64609 normal display/input gate. The special code is a necessary
    /// low-level condition only, not proof that a normal GUI sender exists.
    pub fn input_blocked(&self, code: u32, command: u32) -> bool {
        command == 0xdc
            || (code != 0 && !(code == 0x10001 && self.valid))
            || (code == 0 && !self.valid)
    }
    fn normalize_paste(&mut self) {
        if !matches!(self.mode, 0 | 1 | 8 | 0xd) {
            self.data = 0;
        }
        if matches!(self.mode as u8, 0xf | 0x11) {
            self.filter = 0;
        }
    }
    fn save(&mut self, image: &mut Icf<'_>, kind: CompactKind, row: usize) -> Result<(), Error> {
        let at = kind.offset(row)?;
        let raw: [u8; 36] = window(image.memory(), at, 36)?
            .try_into()
            .expect("compact window");
        self.write_to(&raw, |offset, bytes| put(image, at + offset, bytes))
    }
    /// Original raw36 saver with relative offsets for a checked caller target.
    /// Frequency (offset0/4 bytes) commits before valid; a valid record then
    /// commits offset4/32 bytes. Abort on the first callback error without
    /// undoing prior writes or holder callsign correction. No DATA/filter paste
    /// normalization is implied. old is the actual target's reserved-bit source.
    pub fn write_to(
        &mut self,
        old: &[u8; 36],
        mut write: impl FnMut(usize, &[u8]) -> Result<(), Error>,
    ) -> Result<(), Error> {
        // 7B3F: frequency precedes valid; an empty memopad reset preserves
        // ALL trailing 32 current bytes, rather than copying G's residuals.
        write(0, &self.frequency_hz.to_le_bytes())?;
        if !self.valid {
            return Ok(());
        }
        let mut b = *old;
        b[4] = ((encode_mode(self.mode) & 15) << 4) | (self.filter as u8 & 15);
        let dup = match self.duplex {
            3 => 1,
            4 => 2,
            n => n,
        } as u8;
        b[5] = (self.data as u8 & 1)
            | ((encode_tone(self.tone) & 7) << 1)
            | ((encode_digital_squelch(self.digital_squelch) & 3) << 4)
            | ((dup & 3) << 6);
        b[6] = (b[6] & !3)
            | match self.polarity {
                3 => 1,
                4 => 2,
                5 => 3,
                _ => 0,
            };
        b[7] = self.tx_tone as u8;
        b[8] = self.rx_tone as u8;
        b[9] = self.dtcs as u8;
        b[10..14].copy_from_slice(&self.offset_hz.min(9_999_900).to_le_bytes());
        b[14] = self.digital_code as u8;
        for (i, c) in self.callsigns.iter().enumerate() {
            let seven = c.map(|b| b & 0x7f);
            b[15 + 7 * i..22 + 7 * i].copy_from_slice(&pack_callsign(&seven));
        }
        if self.callsigns[2] != [b' '; 8] {
            self.callsigns[1] = unpack_callsign(&b[22..29]);
            if self.callsigns[1][7] == b'G' {
                self.callsigns[1][7] = b' ';
                b[22..29].copy_from_slice(&pack_callsign(&self.callsigns[1]));
            }
        }
        write(4, &b[4..])
    }
}

/// A060/9EEC decoded clipboard. reserve_count records the original count-3
/// calculation separately from actual decoded rows. Invalid low-level crossing
/// selections fail before an out-of-bounds write, retaining the captured prefix.
#[derive(Debug, Default, Clone)]
pub struct CompactClipboard {
    pub reserve_count: i32,
    pub source_rows: Vec<usize>,
    pub channels: Vec<CompactChannel>,
}
impl CompactClipboard {
    pub fn copy(
        &mut self,
        source: &[u8],
        kind: CompactKind,
        range: Range<usize>,
        only: bool,
    ) -> Result<(), Error> {
        let domain = kind.displayed_rows();
        if range.start < domain.start || range.end > domain.end || range.start > range.end {
            return Err(invalid("compact copy range"));
        }
        self.channels.clear();
        self.source_rows.clear();
        let skip = kind == CompactKind::BandStack && !only && range.start < 27 && range.end > 27;
        self.reserve_count = (range.len() as i32) - if skip { 3 } else { 0 };
        if self.reserve_count < 0 {
            return Err(invalid("band copy negative reserve count"));
        }
        for row in range {
            if skip && row / 3 == 9 {
                continue;
            }
            if self.channels.len() >= self.reserve_count as usize {
                return Err(invalid("band copy reserve/read mismatch"));
            }
            self.channels.push(CompactChannel::read(source, kind, row)?);
            self.source_rows.push(row);
        }
        Ok(())
    }
}

/// D0AFC/CBE44: confirm1 -> G decoded copy -> normalized sequential stores ->
/// planned row14 notifications -> queue11/3C. A band exclusion produces 42
/// contiguous writes but 45 row notifications; reserved memopad row0 is untouched.
pub fn reset_compact(
    image: &mut Icf<'_>,
    g: &[u8],
    kind: CompactKind,
    confirmation: i32,
    only: bool,
) -> Result<Effects, Error> {
    if confirmation != 1 {
        return Ok(Effects::default());
    }
    let range = kind.displayed_rows();
    let mut clip = CompactClipboard::default();
    clip.copy(g, kind, range.clone(), only)?;
    let mut effects = Effects::default();
    for (i, h) in clip.channels.iter_mut().enumerate() {
        h.normalize_paste();
        h.save(image, kind, range.start + i)?;
        effects.writes += 1;
    }
    effects
        .notifications
        .extend(range.map(|i| Notification::queue(0x14, 0, i as u32)));
    effects.notifications.push(Notification::queue(0x11, 0, 0));
    effects.notifications.push(Notification::queue(0x3c, 1, 0));
    Ok(effects)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeSide {
    Low,
    High,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanEdge {
    pub valid: bool,
    pub low_hz: u32,
    pub high_hz: u32,
    pub mode: u32,
    pub data: u32,
    pub filter: u32,
    pub name: [u8; 16],
}
impl ScanEdge {
    fn normalize_paste(&mut self) {
        if !matches!(self.mode, 0 | 1 | 8 | 0xd) {
            self.data = 0;
        }
        if matches!(self.mode as u8, 0xf | 0x11) {
            self.filter = 0xff;
        }
    }
    pub fn mode_candidate_allowed(&self, candidate: u32) -> bool {
        candidate == self.mode || !self.valid || candidate != 0x18
    }
    pub fn filter_candidate_allowed(&self, candidate: u32) -> bool {
        candidate == self.filter
            || !self.valid
            || !matches!(self.mode as u8, 0xf | 0x11)
            || matches!(candidate, 0 | 1 | 2 | 0xff)
    }
    pub fn read(source: &[u8], row: usize) -> Result<Self, Error> {
        index(row, 25)?;
        let mut h = Self {
            valid: false,
            low_hz: 0,
            high_hz: 0,
            mode: 0,
            data: 0,
            filter: 0,
            name: [b' '; 16],
        };
        if byte(source, 0xa60b + row / 8)? & (1 << (row % 8)) != 0 {
            return Ok(h);
        }
        let b = window(source, 0xa368 + 27 * row, 27)?;
        h.valid = true;
        h.high_hz = word32(b, 0)?;
        h.low_hz = word32(b, 4)?;
        h.mode = decode_mode(b[8]);
        h.data = u32::from(b[9]);
        h.filter = u32::from(b[10]);
        h.name.copy_from_slice(&b[11..27]);
        Ok(h)
    }
    /// Save through Icf's checked patch boundary. Unlike invalid compact rows,
    /// invalid scan rows still write the complete defined payload after bitmap.
    pub fn save(&mut self, image: &mut Icf<'_>, row: usize) -> Result<(), Error> {
        index(row, 25)?;
        let at = 0xa60b + row / 8;
        let flag = byte(image.memory(), at)?;
        self.save_with(row, flag, |at, bytes| put(image, at, bytes))
    }
    /// Expanded-image adapter. Caller supplies the current bitmap byte and a
    /// checked absolute-offset writer. row is validated BEFORE callbacks;
    /// bitmap commits BEFORE invalid holder clearing and the raw27 commit.
    pub fn save_with(
        &mut self,
        row: usize,
        flag: u8,
        mut write: impl FnMut(usize, &[u8]) -> Result<(), Error>,
    ) -> Result<(), Error> {
        index(row, 25)?;
        let bit = 1 << (row % 8);
        write(
            0xa60b + row / 8,
            &[if self.valid { flag & !bit } else { flag | bit }],
        )?;
        if !self.valid {
            self.low_hz = 0;
            self.high_hz = 0;
            self.name = [b' '; 16];
        }
        let mut b = [0; 27];
        b[..4].copy_from_slice(&self.high_hz.to_le_bytes());
        b[4..8].copy_from_slice(&self.low_hz.to_le_bytes());
        b[8] = encode_mode(self.mode);
        b[9] = self.data as u8;
        b[10] = self.filter as u8;
        b[11..].copy_from_slice(&self.name);
        write(0xa368 + 27 * row, &b)
    }
    pub fn warnings(&self, side: EdgeSide, mut bad: impl FnMut(u32) -> bool) -> u32 {
        if !self.valid {
            return 0;
        }
        let value = match side {
            EdgeSide::Low => self.low_hz,
            EdgeSide::High => self.high_hz,
        };
        let specific = match side {
            EdgeSide::Low => bad(value),
            EdgeSide::High => bad(value) || (!bad(self.low_hz) && self.low_hz > self.high_hz),
        };
        (u32::from(specific) << 2) | (u32::from(!(30_000..=470_000_000).contains(&value)) << 3)
    }
    pub fn suitable(
        &self,
        allow_empty: bool,
        allow_equal: bool,
        check_specific: bool,
        mut bad: impl FnMut(u32) -> bool,
    ) -> bool {
        (allow_empty || self.valid)
            && (allow_equal || !self.valid || self.low_hz != self.high_hz)
            && (!self.valid
                || ((30_000..=470_000_000).contains(&self.low_hz)
                    && (30_000..=470_000_000).contains(&self.high_hz)
                    && (!check_specific
                        || (!bad(self.high_hz)
                            && !bad(self.low_hz)
                            && self.low_hz <= self.high_hz
                            && self.mode != 0x18))))
    }
}

/// An accepted first parse and the result of reparsing the SAME original text
/// against the second range. second_accepted is retained for diagnostics but
/// deliberately not consulted by the original consumer (6A562/76B86).
#[derive(Debug, Clone, Copy)]
pub struct FrequencyInput {
    pub first_accepted: bool,
    pub second_value: u32,
    pub second_accepted: bool,
}

/// 99994/999FC: existence and equality use source, warnings use live_current.
/// The warning image is read only after the source-side guards pass.
pub fn scan_suitable_rows(
    source: &[u8],
    live_current: &[u8],
    allow_empty: bool,
    allow_equal: bool,
    check_specific: bool,
    mut bad: impl FnMut(u32) -> bool,
) -> Result<Vec<usize>, Error> {
    let mut rows = Vec::new();
    for row in 0..25 {
        let h = ScanEdge::read(source, row)?;
        if (allow_empty || h.valid)
            && (allow_equal || !h.valid || h.low_hz != h.high_hz)
            && ScanEdge::read(live_current, row)?.suitable(true, true, check_specific, &mut bad)
        {
            rows.push(row);
        }
    }
    Ok(rows)
}

/// AA658's stored selection includes low5 values25..31 outside the menu rows.
pub fn selected_scan_raw(image: &[u8]) -> Result<u8, Error> {
    Ok((byte(image, 0x37802)? >> 2) & 31)
}

pub fn select_scan(image: &mut Icf<'_>, g: &[u8], row: usize, reset: bool) -> Result<(), Error> {
    index(row, 25)?;
    select_scan_raw(image, g, row as i32, reset)
}

/// AA658's accepted selected candidate bypasses the normal25-row menu domain.
/// Its low5 bits are stored before any conditional reset or G source read.
pub fn select_scan_raw(
    image: &mut Icf<'_>,
    g: &[u8],
    candidate: i32,
    reset: bool,
) -> Result<(), Error> {
    let old = byte(image.memory(), 0x37802)?;
    put_byte(
        image,
        0x37802,
        (old & !0x7c) | ((candidate as u8 & 31) << 2),
    )?;
    if reset {
        let old = byte(image.memory(), 0x37807)?;
        put_byte(image, 0x37807, old & !1)?;
        put(image, 0x37810, window(g, 0x37810, 4)?)?;
    }
    Ok(())
}

/// C5D88/C5DE0. Index is truncated to its low byte, not clamped to the normal
/// caller's zero. Short-circuit reads preserve the original guard order.
pub fn scan_reset_enabled(source: &[u8], index: u32) -> Result<bool, Error> {
    let i = usize::from(index as u8);
    Ok(byte(source, 0x3a8ca)? == 0
        && byte(source, 0x3a8c9 + i)? == 0
        && byte(source, 0x3c84a + 5 * i)? & 3 == 0
        && byte(source, 0x37807 + 2 * i)? & 1 != 0
        && matches!((byte(source, 0x37806 + 2 * i)? >> 3) & 31, 1 | 5))
}

pub fn edit_scan_frequency(
    image: &mut Icf<'_>,
    g: &[u8],
    row: usize,
    side: EdgeSide,
    input: FrequencyInput,
    reset: bool,
    mut bad: impl FnMut(u32) -> bool,
) -> Result<Effects, Error> {
    let mut h = ScanEdge::read(image.memory(), row)?;
    let mut effects = Effects {
        callback_flags: 8,
        ..Effects::default()
    };
    if input.first_accepted {
        if !h.valid {
            h = ScanEdge {
                valid: true,
                low_hz: input.second_value,
                high_hz: input.second_value,
                mode: 0x19,
                data: 0,
                filter: 0xff,
                name: [b' '; 16],
            };
        }
        match side {
            EdgeSide::Low => h.low_hz = input.second_value,
            EdgeSide::High => h.high_hz = input.second_value,
        }
        h.save(image, row)?;
        effects.writes = 1;
        let suitable = scan_suitable_rows(
            image.memory(),
            image.memory(),
            false,
            false,
            false,
            &mut bad,
        )?;
        if suitable.len() == 1 {
            select_scan(image, g, suitable[0], reset)?;
        }
        h = ScanEdge::read(image.memory(), row)?;
    }
    effects.warning_bits = h.warnings(side, bad);
    Ok(effects)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanField {
    Name,
    Mode,
    Data,
    Filter,
}
pub fn edit_scan_field(
    image: &mut Icf<'_>,
    row: usize,
    field: ScanField,
    accepted: Option<u32>,
    name: Option<&[u8]>,
) -> Result<Effects, Error> {
    let mut h = ScanEdge::read(image.memory(), row)?;
    let blocked = match field {
        ScanField::Data => !h.valid || !matches!(h.mode, 0 | 1 | 8 | 0xd),
        ScanField::Filter => h.valid && matches!(h.mode as u8, 0xf | 0x11),
        _ => false,
    };
    let mut e = Effects {
        callback_flags: 8,
        blocked,
        ..Effects::default()
    };
    if blocked {
        return Ok(e);
    }
    if field == ScanField::Name {
        e.node_style_clear = 3;
        // Caller supplies the shared editor's already-sanitized byte string.
        if let Some(name) = name {
            if name.len() > 16 {
                return Err(invalid("scan name exceeds sixteen bytes"));
            }
            h.name = [b' '; 16];
            h.name[..name.len()].copy_from_slice(name);
            h.save(image, row)?;
            e.writes = 1;
        }
    } else {
        if let Some(value) = accepted {
            match field {
                ScanField::Mode => h.mode = value,
                ScanField::Data => h.data = value,
                ScanField::Filter => h.filter = value,
                ScanField::Name => unreachable!(),
            }
            h.save(image, row)?;
            e.writes = 1;
        }
        if field != ScanField::Data {
            e.phase = Some(2);
        }
    }
    h = ScanEdge::read(image.memory(), row)?;
    // 6A912/6AAD3 re-evaluate ONLY their mode/filter predicate. These are
    // not frequency field handlers; importing broad/band warnings is wrong.
    e.warning_bits = match field {
        ScanField::Mode if h.valid && h.mode == 0x18 => 4,
        ScanField::Filter
            if h.valid
                && matches!(h.mode as u8, 0xf | 0x11)
                && !matches!(h.filter, 0 | 1 | 2 | 255) =>
        {
            4
        }
        _ => 0,
    };
    Ok(e)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserEdge {
    pub valid: bool,
    pub low_hz: u32,
    pub high_hz: u32,
}
impl UserEdge {
    pub fn read(source: &[u8], row: usize) -> Result<Self, Error> {
        let at = 0x378c0 + 8 * index(row, 30)?;
        let low = word32(source, at)?;
        let high = word32(source, at + 4)?;
        Ok(if low == 0 || high == 0 {
            Self {
                valid: false,
                low_hz: 0,
                high_hz: 0,
            }
        } else {
            Self {
                valid: true,
                low_hz: low,
                high_hz: high,
            }
        })
    }
    fn bad(
        &self,
        side: EdgeSide,
        prior: u32,
        bad: &mut impl FnMut(u32) -> bool,
        classify: &mut impl FnMut(u32) -> u32,
    ) -> bool {
        let f = match side {
            EdgeSide::Low => self.low_hz,
            EdgeSide::High => self.high_hz,
        };
        bad(f)
            || (side == EdgeSide::High
                && !bad(self.low_hz)
                && (self.low_hz > self.high_hz || classify(self.low_hz) != classify(self.high_hz)))
            || f <= prior
    }
    pub fn warnings(
        source: &[u8],
        row: usize,
        side: EdgeSide,
        mut bad: impl FnMut(u32) -> bool,
        mut classify: impl FnMut(u32) -> u32,
    ) -> Result<(u32, bool), Error> {
        let h = Self::read(source, row)?;
        if !h.valid {
            return Ok((0, false));
        }
        let f = match side {
            EdgeSide::Low => h.low_hz,
            EdgeSide::High => h.high_hz,
        };
        let mut specific = h.bad(side, 0, &mut bad, &mut classify);
        let mut prior = 0;
        if !specific {
            for i in 0..row {
                let p = Self::read(source, i)?;
                if p.valid
                    && !p.bad(EdgeSide::Low, prior, &mut bad, &mut classify)
                    && !p.bad(EdgeSide::High, prior, &mut bad, &mut classify)
                {
                    if p.high_hz >= f {
                        specific = true;
                        break;
                    }
                    prior = p.high_hz;
                }
            }
        }
        Ok((
            (u32::from(specific) << 2) | (u32::from(!(30_000..=470_000_000).contains(&f)) << 3),
            side == EdgeSide::High && h.low_hz == h.high_hz,
        ))
    }
}
pub fn edit_user_frequency(
    image: &mut Icf<'_>,
    row: usize,
    side: EdgeSide,
    input: FrequencyInput,
    bad: impl FnMut(u32) -> bool,
    classify: impl FnMut(u32) -> u32,
) -> Result<Effects, Error> {
    let mut h = UserEdge::read(image.memory(), row)?;
    let mut e = Effects {
        callback_flags: 8,
        ..Effects::default()
    };
    if input.first_accepted {
        e.callback_flags |= 0x20;
        if (input.second_value as i32) >= 0 {
            if !h.valid {
                h.low_hz = input.second_value;
                h.high_hz = input.second_value;
            }
            match side {
                EdgeSide::Low => h.low_hz = input.second_value,
                EdgeSide::High => h.high_hz = input.second_value,
            }
            let mut bytes = [0; 8];
            bytes[..4].copy_from_slice(&h.low_hz.to_le_bytes());
            bytes[4..].copy_from_slice(&h.high_hz.to_le_bytes());
            put(image, 0x378c0 + 8 * row, &bytes)?;
            e.writes = 1;
        }
    }
    (e.warning_bits, e.gray) = UserEdge::warnings(image.memory(), row, side, bad, classify)?;
    Ok(e)
}

/// Post-row context, separate from image bytes and actual transfer counts.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RowState {
    pub bits: u32,
    pub start: usize,
    pub end: usize,
    pub active: usize,
    pub state: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanRowAction {
    Clear,
    Delete,
    Insert,
    Up,
    Down,
}
pub fn scan_rows(
    image: &mut Icf<'_>,
    g: &[u8],
    range: Range<usize>,
    action: ScanRowAction,
    confirmed: bool,
    state: &mut RowState,
    reset: bool,
) -> Result<Effects, Error> {
    if range.start >= range.end || range.end > 25 {
        return Err(invalid("scan row range"));
    }
    if !confirmed {
        return Ok(Effects::default());
    }
    let n = range.len();
    let mut effects = Effects {
        callback_flags: 8,
        ..Effects::default()
    };
    let saved = (0..25)
        .map(|i| ScanEdge::read(image.memory(), i))
        .collect::<Result<Vec<_>, _>>()?;
    let mut plan: Vec<(usize, ScanEdge)> = Vec::new();
    match action {
        ScanRowAction::Clear => {}
        ScanRowAction::Delete => {
            for dst in range.start..25 - n {
                plan.push((dst, saved[dst + n].clone()));
            }
        }
        ScanRowAction::Insert => {
            for dst in (range.end..25).rev() {
                plan.push((dst, saved[dst - n].clone()));
            }
        }
        ScanRowAction::Up | ScanRowAction::Down => {
            let up = action == ScanRowAction::Up;
            if up && range.start == 0 || !up && range.end == 25 {
                return Ok(Effects::default());
            }
            let outside = if up { range.start - 1 } else { range.end };
            let destination = if up { range.end - 1 } else { range.start };
            for src in range.clone() {
                plan.push((if up { src - 1 } else { src + 1 }, saved[src].clone()));
            }
            plan.push((destination, saved[outside].clone()));
        }
    }
    let defaults = match action {
        ScanRowAction::Clear | ScanRowAction::Insert => range.clone(),
        ScanRowAction::Delete => 25 - n..25,
        _ => 0..0,
    };
    // Movement precedes the default-source read. A later missing G window
    // must not erase or prevent the already-completed movement prefix.
    for (row, mut h) in plan {
        h.normalize_paste();
        h.save(image, row)?;
        effects.writes += 1;
    }
    let mut defaults_plan = Vec::new();
    // CBE44 captures all G holders before any DEFAULT write. Valid0 retains
    // its decoded mode/DATA/filter, unlike fresh type1F initialization.
    for row in defaults.clone() {
        let mut h = ScanEdge::read(g, row)?;
        h.valid = false;
        defaults_plan.push((row, h));
    }
    for (row, mut h) in defaults_plan {
        h.normalize_paste();
        h.save(image, row)?;
        effects.writes += 1;
    }
    effects
        .notifications
        .extend(defaults.map(|i| Notification::queue(0x14, 0, i as u32)));
    scan_reference(image, g, Some(action), range.clone(), reset, false)?;
    match action {
        ScanRowAction::Delete => {
            state.state = 1;
            state.bits &= !4;
            state.active = range.start;
            effects.notifications.push(Notification::queue(0x11, 0, 0));
            effects.notifications.push(Notification::queue(0x3c, 1, 0));
        }
        ScanRowAction::Insert => {
            state.state = 1;
            state.bits |= 4;
            state.start = range.start;
            state.end = range.end - 1;
            effects.notifications.push(Notification::queue(0x3c, 1, 0));
            effects.notifications.push(Notification::queue(0x11, 0, 0));
        }
        ScanRowAction::Up | ScanRowAction::Down => {
            state.state = 1;
            let d = if action == ScanRowAction::Up { -1 } else { 1 };
            state.bits |= 4;
            state.start = range.start.saturating_add_signed(d);
            state.end = (range.end - 1).saturating_add_signed(d);
            state.active = if d < 0 { state.start } else { state.end };
        }
        ScanRowAction::Clear => {
            // CC202/CC215: unlike move, clear queues 11 then 3C after D7.
            // This path does not store context+70 or replace its selection.
            effects.notifications.push(Notification::queue(0x11, 0, 0));
            effects.notifications.push(Notification::queue(0x3c, 1, 0));
        }
    }
    Ok(effects)
}

fn scan_reference(
    image: &mut Icf<'_>,
    g: &[u8],
    action: Option<ScanRowAction>,
    range: Range<usize>,
    reset: bool,
    paste: bool,
) -> Result<(), Error> {
    let old = byte(image.memory(), 0x37802)?;
    let selected = usize::from(selected_scan_raw(image.memory())?);
    // 999FC flags0/0/0 uses broad bounds only. Its bad callback is never called.
    let suitable = scan_suitable_rows(image.memory(), image.memory(), false, false, false, |_| {
        unreachable!("specific disabled")
    })?;
    if suitable.is_empty() && selected == 0 {
        return Ok(());
    }
    let n = range.len();
    let mut delta = 0i32;
    let mut reselect = false;
    if paste {
        reselect = selected < 25 && !ScanEdge::read(image.memory(), selected)?.valid;
    } else {
        match action.expect("row action") {
            ScanRowAction::Clear => reselect = range.contains(&selected),
            ScanRowAction::Delete => {
                if range.contains(&selected) {
                    reselect = true;
                } else if selected >= range.end {
                    delta = -(n as i32);
                }
            }
            ScanRowAction::Insert => {
                if selected >= range.start {
                    if selected + n > 24 {
                        reselect = true;
                    } else {
                        delta = n as i32;
                    }
                }
            }
            ScanRowAction::Up => {
                if range.contains(&selected) {
                    delta = -1;
                } else if selected + 1 == range.start {
                    delta = n as i32;
                }
            }
            ScanRowAction::Down => {
                if range.contains(&selected) {
                    delta = 1;
                } else if selected == range.end {
                    delta = -(n as i32);
                }
            }
        }
    }
    if reselect {
        select_scan(image, g, suitable.first().copied().unwrap_or(0), reset)?;
    } else if delta != 0 {
        let next = (selected as u32).wrapping_add(delta as u32) as u8;
        put_byte(image, 0x37802, (old & !0x7c) | ((next & 31) << 2))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct ScanClipboard {
    pub rows: Vec<ScanEdge>,
    pub transfer_count: usize,
}
#[derive(Debug, Clone, Copy)]
pub struct PasteApproval {
    pub tail: bool,
    pub overwrite: bool,
}
impl ScanClipboard {
    pub fn copy(
        &mut self,
        source: &[u8],
        range: Range<usize>,
        state: &mut RowState,
    ) -> Result<(), Error> {
        if range.start >= range.end || range.end > 25 {
            return Err(invalid("scan copy range"));
        }
        self.rows.clear();
        self.transfer_count = 0;
        for row in range.clone() {
            self.rows.push(ScanEdge::read(source, row)?);
        }
        self.transfer_count = self.rows.len();
        state.bits |= 4;
        state.start = range.start;
        state.end = range.end - 1;
        Ok(())
    }
    pub fn paste(
        &mut self,
        image: &mut Icf<'_>,
        g: &[u8],
        start: usize,
        approval: PasteApproval,
        state: &mut RowState,
        reset: bool,
    ) -> Result<Effects, Error> {
        index(start, 25)?;
        if self.transfer_count == 0 || self.transfer_count > self.rows.len() {
            return Err(invalid("scan clipboard count"));
        }
        let capacity = 25 - start;
        if self.rows.len() > capacity && !approval.tail {
            return Ok(Effects::default());
        }
        let count = capacity.min(self.transfer_count);
        if !approval.overwrite {
            for i in start..start + count {
                if ScanEdge::read(image.memory(), i)?.valid {
                    return Ok(Effects::default());
                }
            }
        }
        let mut e = Effects {
            callback_flags: 8,
            ..Effects::default()
        };
        for (i, h) in self.rows.iter_mut().take(count).enumerate() {
            // A77C mutates clipboard DATA/filter; no immutable-source fiction.
            h.normalize_paste();
            h.save(image, start + i)?;
            e.writes += 1;
        }
        scan_reference(image, g, None, start..start + count, reset, true)?;
        state.bits |= 4;
        state.start = start;
        state.end = start + count - 1;
        state.state = 1;
        Ok(e)
    }
}

/// Preset body/mask are deliberately public raw arrays: unknown stored values
/// are readable and unrelated bytes are never rejected merely to edit a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    pub body: [u8; 41],
    pub masks: [u8; 22],
}
fn preset_at(p: usize) -> Result<(usize, usize), Error> {
    index(p, 5)?;
    Ok((0xa60f + 41 * p, 0xa6dc + 22 * p))
}
impl Preset {
    pub fn read(source: &[u8], p: usize) -> Result<Self, Error> {
        let (a, m) = preset_at(p)?;
        Ok(Self {
            body: window(source, a, 41)?.try_into().expect("preset body"),
            masks: window(source, m, 22)?.try_into().expect("preset masks"),
        })
    }
    /// 8B50 and 8BD4 operate on the stored mode byte, not the compact holder code.
    pub fn bandwidth_class(&self) -> u32 {
        match self.body[0] {
            0..=2 => u32::from(self.body[1] != 0),
            3 | 4 => 2,
            5 | 6 => 3,
            7 => 4,
            8 => 5,
            9 => 6,
            10 => 7,
            _ => 9,
        }
    }
    pub fn combined_mode(&self) -> u32 {
        let data = u32::from(self.body[1] != 0);
        match self.body[0] {
            0 => 0,
            1 => 1 + data,
            2 => 3 + data,
            3 => 5,
            4 => 6,
            5 => 7,
            6 => 8,
            7 => 9 + data,
            8 => 11 + data,
            9 => 14,
            10 => 13,
            _ => 15,
        }
    }
    pub fn bandwidth_hz(&self) -> Result<i32, Error> {
        decode_bandwidth(self.bandwidth_class(), self.body[3])
    }
    /// Native385 logical text and optional CANVAS check, not a mask number.
    /// A normal checkbox has empty text; only exact mask00 draws the check.
    pub fn checkbox_paint(&self, field: u32) -> Result<(Vec<u8>, Option<bool>), Error> {
        if self.incompatible(field)? {
            return Ok((b"--".to_vec(), None));
        }
        let mask = PRESET_FIELDS
            .iter()
            .position(|&f| f == field)
            .ok_or(invalid("unknown preset checkbox field"))?;
        Ok((Vec::new(), Some(self.masks[mask] == 0)))
    }
    /// Native389 uses unpadded integer Hz and a suffix with a trailing space.
    /// Own maskFF is only a color change, not a reason to hide the value.
    pub fn bandwidth_paint(&self) -> Result<Vec<u8>, Error> {
        if self.incompatible(3)? || self.bandwidth_class() > 4 {
            return Ok(b"---".to_vec());
        }
        Ok(format!("{}Hz ", self.bandwidth_hz()?).into_bytes())
    }
    /// Native level/address source bytes, before the shared raw-value formatter.
    /// These are PRESET body fields, never the similarly named global settings.
    pub fn paint_byte(&self, key: u32) -> Result<u8, Error> {
        if !matches!(key, 0x38d | 0x38f | 0x390 | 0x39a) {
            return Err(invalid("key is not a preset level or address"));
        }
        let field = PRESET_CATALOG
            .iter()
            .find(|f| u32::from(f.key) == key)
            .ok_or(invalid("unknown preset paint key"))?;
        Ok(self.body[field.offset])
    }
    pub fn incompatible(&self, field: u32) -> Result<bool, Error> {
        let c = self.combined_mode();
        let m = &self.masks;
        Ok(match field {
            0 => true,
            1 => false,
            2 => m[0] == 255 || c == 14,
            3 => m[0] == 255 || m[1] == 255 || matches!(c, 11..=14),
            4 | 0x16 => {
                let class = self.bandwidth_class();
                m[0] == 255
                    || m[2] == 255
                    || self.incompatible(3)?
                    || !(class == 2 || (class <= 1 && self.bandwidth_hz()? >= 600))
            }
            0xf => m[8] == 255,
            5..=0x15 => false,
            _ => true,
        })
    }
    pub fn has_enabled_fields(&self) -> Result<bool, Error> {
        for (i, &field) in PRESET_FIELDS.iter().enumerate() {
            if self.masks[i] == 0 && !self.incompatible(field)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub fn keying_candidate_allowed(&self, which: usize, candidate: u32) -> Result<bool, Error> {
        index(which, 2)?;
        let at = 20 + which;
        Ok(candidate == u32::from(self.body[at])
            || candidate == 0
            || self.masks[17 + which] != 0
            || self.masks[16] != 0
            || candidate != u32::from(self.body[19]))
    }
}

// B091C/C8BE4 field IDs, in the 22 mask positions. Field0 is placeholder;
// field16h is the second filter type, not a 23rd mask byte.
const PRESET_FIELDS: [u32; 22] = [
    1, 2, 3, 4, 0x16, 5, 6, 7, 8, 0xf, 9, 0xa, 0xb, 0xc, 0xd, 0xe, 0x10, 0x11, 0x12, 0x13, 0x14,
    0x15,
];

/// B008C/B0100 piecewise signed arithmetic. The five coefficient records reduce
/// to two distinct formulae (class3 only differs in an unused limit). No clamp,
/// rounding-to-nearest or strict known-raw gate is added.
pub fn decode_bandwidth(class: u32, raw: u8) -> Result<i32, Error> {
    let (a, b, d, e) = bandwidth_coefficients(class)?;
    let k = b / d - 1;
    let z = i32::from(raw);
    Ok(if z <= k { a + d * z } else { b + e * (z - k) })
}
pub fn encode_bandwidth(class: u32, width: i32) -> Result<u8, Error> {
    let (a, b, d, e) = bandwidth_coefficients(class)?;
    let k = b / d - 1;
    let n = i64::from(width.wrapping_sub(if width < b { a } else { b }));
    let z = if width < b {
        n / i64::from(d)
    } else {
        n / i64::from(e) + i64::from(k)
    };
    Ok(z as u8)
}
fn bandwidth_coefficients(class: u32) -> Result<(i32, i32, i32, i32), Error> {
    match class {
        0..=3 => Ok((50, 500, 50, 100)),
        4 => Ok((200, 10_000, 200, 200)),
        _ => Err(invalid("preset bandwidth class outside 0..=4")),
    }
}

/// Registry descriptors expose exact key/index/field/domain, not a claim that
/// a value patch implements the original selection or checkbox operation.
#[derive(Debug, Clone, Copy)]
pub struct PresetField {
    pub key: u16,
    pub offset: usize,
    pub mask: usize,
    pub field: u32,
    pub choices: u32,
}
pub const PRESET_CATALOG: [PresetField; 22] = [
    PresetField {
        key: 0x387,
        offset: 0,
        mask: 0,
        field: 1,
        choices: 15,
    },
    PresetField {
        key: 0x388,
        offset: 2,
        mask: 1,
        field: 2,
        choices: 3,
    },
    PresetField {
        key: 0x389,
        offset: 3,
        mask: 2,
        field: 3,
        choices: 0,
    },
    PresetField {
        key: 0x38a,
        offset: 4,
        mask: 3,
        field: 4,
        choices: 2,
    },
    PresetField {
        key: 0x38b,
        offset: 5,
        mask: 4,
        field: 0x16,
        choices: 2,
    },
    PresetField {
        key: 0x394,
        offset: 6,
        mask: 5,
        field: 5,
        choices: 2,
    },
    PresetField {
        key: 0x393,
        offset: 7,
        mask: 6,
        field: 6,
        choices: 4,
    },
    PresetField {
        key: 0x391,
        offset: 8,
        mask: 7,
        field: 7,
        choices: 4,
    },
    PresetField {
        key: 0x395,
        offset: 9,
        mask: 8,
        field: 8,
        choices: 3,
    },
    PresetField {
        key: 0x396,
        offset: 10,
        mask: 9,
        field: 0xf,
        choices: 4,
    },
    PresetField {
        key: 0x392,
        offset: 12,
        mask: 10,
        field: 9,
        choices: 4,
    },
    PresetField {
        key: 0x38c,
        offset: 14,
        mask: 11,
        field: 0xa,
        choices: 2,
    },
    PresetField {
        key: 0x390,
        offset: 15,
        mask: 12,
        field: 0xb,
        choices: 0,
    },
    PresetField {
        key: 0x38d,
        offset: 16,
        mask: 13,
        field: 0xc,
        choices: 0,
    },
    PresetField {
        key: 0x38e,
        offset: 17,
        mask: 14,
        field: 0xd,
        choices: 2,
    },
    PresetField {
        key: 0x38f,
        offset: 18,
        mask: 15,
        field: 0xe,
        choices: 0,
    },
    PresetField {
        key: 0x397,
        offset: 19,
        mask: 16,
        field: 0x10,
        choices: 5,
    },
    PresetField {
        key: 0x398,
        offset: 20,
        mask: 17,
        field: 0x11,
        choices: 5,
    },
    PresetField {
        key: 0x399,
        offset: 21,
        mask: 18,
        field: 0x12,
        choices: 5,
    },
    PresetField {
        key: 0x39a,
        offset: 22,
        mask: 19,
        field: 0x13,
        choices: 0,
    },
    PresetField {
        key: 0x39b,
        offset: 23,
        mask: 20,
        field: 0x14,
        choices: 2,
    },
    PresetField {
        key: 0x39c,
        offset: 24,
        mask: 21,
        field: 0x15,
        choices: 2,
    },
];

fn preset_finish(
    image: &mut Icf<'_>,
    g: &[u8],
    live_preset: usize,
    e: &mut Effects,
) -> Result<(), Error> {
    // 7FE9A compares LIVE node-derived p, not the name editor's row q.
    if usize::from(byte(image.memory(), 0xa78c)?) == live_preset {
        put_byte(image, 0xa78c, byte(g, 0xa78c)?)?;
        e.writes += 1;
    }
    Ok(())
}
pub fn reset_preset(
    image: &mut Icf<'_>,
    g: &[u8],
    p: usize,
    confirmation: i32,
) -> Result<Effects, Error> {
    let (a, m) = preset_at(p)?;
    if confirmation != 1 {
        return Ok(Effects::default());
    }
    put(image, a, window(g, a, 41)?)?;
    put(image, m, window(g, m, 22)?)?;
    Ok(Effects {
        writes: 2,
        notifications: vec![
            Notification::queue(0x11, 0, 0),
            Notification::queue(0x3c, 1, 0),
        ],
        ..Effects::default()
    })
}
/// Store a shared-editor-approved name, then issue 3E/39/11 in order through
/// notify before the live selection-byte tail. notify must queue asynchronous
/// requests and complete synchronous requests inline. A failure stops here
/// without rollback. Returned Effects contain no already-issued notifications.
pub fn rename_preset(
    image: &mut Icf<'_>,
    g: &[u8],
    q: usize,
    live_preset: usize,
    name: Option<&[u8]>,
    label: impl FnOnce(&[u8; 16]) -> Result<Vec<u8>, Error>,
    mut notify: impl FnMut(&Notification) -> Result<(), Error>,
) -> Result<Effects, Error> {
    let (a, _) = preset_at(q)?;
    let Some(name) = name else {
        return Ok(Effects::default());
    };
    if name.len() > 16 {
        return Err(invalid("preset name exceeds sixteen bytes"));
    }
    let mut bytes = [b' '; 16];
    bytes[..name.len()].copy_from_slice(name);
    put(image, a + 25, &bytes)?;
    let display = label(&bytes)?; // Label failure preserves the earlier name store.
    let mut e = Effects {
        writes: 1,
        notifications: vec![
            Notification {
                event: 0x3e,
                synchronous: false,
                first: 0x1a6 + q as u32,
                second: 0,
                payload: display,
            },
            Notification {
                event: 0x39,
                synchronous: true,
                first: 0,
                second: 0,
                payload: Vec::new(),
            },
            Notification::queue(0x11, 0, 0),
        ],
        ..Effects::default()
    };
    // 3E enqueue -> 39 synchronous -> 11 enqueue precede the common-tail
    // selection store. Consumed requests are not returned as pending work.
    for event in e.notifications.drain(..) {
        notify(&event)?;
    }
    preset_finish(image, g, live_preset, &mut e)?;
    Ok(e)
}

pub fn toggle_preset(
    image: &mut Icf<'_>,
    g: &[u8],
    p: usize,
    field: u32,
    grid_present: bool,
    code: u32,
    live_preset: usize,
) -> Result<Effects, Error> {
    let (_, m) = preset_at(p)?;
    let h = Preset::read(image.memory(), p)?;
    let Some(mask) = PRESET_FIELDS.iter().position(|&f| f == field) else {
        return Ok(Effects {
            blocked: true,
            ..Effects::default()
        });
    };
    if h.incompatible(field)? {
        return Ok(Effects {
            blocked: true,
            ..Effects::default()
        });
    }
    if !grid_present || !matches!(code, 0x20 | 8 | 13) {
        return Ok(Effects::default());
    }
    let new = if h.masks[mask] == 0 { 255 } else { 0 };
    put_byte(image, m + mask, new)?;
    let (a, _) = preset_at(p)?;
    if h.masks[mask] == 255 && new == 0 {
        match mask {
            16 => {
                if h.masks[17] == 0 && byte(image.memory(), a + 19)? == h.body[20] {
                    put_byte(image, a + 19, 0)?;
                }
                if h.masks[18] == 0 && byte(image.memory(), a + 19)? == h.body[21] {
                    put_byte(image, a + 19, 0)?;
                }
            }
            17 | 18 => {
                let at = mask + 3;
                if h.masks[16] == 0 && h.body[19] == h.body[at] {
                    put_byte(image, a + at, 0)?;
                }
            }
            _ => {}
        }
    }
    let mut e = Effects {
        writes: 1,
        callback_flags: 0x20,
        ..Effects::default()
    };
    preset_finish(image, g, live_preset, &mut e)?;
    Ok(e)
}

fn correct_filter_types(image: &mut Icf<'_>, g: &[u8], p: usize) -> Result<(), Error> {
    let (a, _) = preset_at(p)?;
    let h = Preset::read(image.memory(), p)?;
    let c = h.bandwidth_class();
    let width = if c <= 2 { h.bandwidth_hz()? } else { 0 };
    if c == 2 || (c <= 1 && width >= 600) {
        let column = if c == 2 {
            if width < 600 { 2 } else { 3 }
        } else {
            c as usize
        };
        for side in 0..2 {
            if byte(image.memory(), a + 4 + side)? == 255 {
                put_byte(image, a + 4 + side, byte(g, 0x3785f + 4 * side + column)?)?;
            }
        }
    } else {
        put_byte(image, a + 4, 255)?;
        put_byte(image, a + 5, 255)?;
    }
    Ok(())
}

/// Native numeric helper output and the existing Preset commit tail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetNumberEffect {
    pub editor: crate::icf::settings::extended::NumberEffect,
    pub effects: Effects,
}

/// Shared AFD74/A4A8C input, separate from global/Preset storage and PBT tails.
/// Both text passes use numeric_text; the second consumes the FIRST integer.
/// This is the existing native grid over decode/encode_bandwidth, not a codec.
pub fn bandwidth_input(
    class: u32,
    old_raw: u8,
    input: crate::icf::settings::extended::NumberInput<'_>,
    crt_point: u8,
) -> Result<crate::icf::settings::extended::BandwidthEffect, Error> {
    use crate::icf::settings::extended::{
        BandwidthEffect, NumberEffect, NumberInput, numeric_text,
    };
    let old = i64::from(decode_bandwidth(class, old_raw)?);
    let min = i64::from(decode_bandwidth(class, 0)?);
    let max = [3600, 3600, 3600, 2700, 10000][class as usize];
    let step = |width: i64, previous: bool| -> Result<i64, Error> {
        let small = i64::from(decode_bandwidth(class, 1)? - decode_bandwidth(class, 0)?);
        let raw =
            encode_bandwidth(class, (width - if previous { small } else { 0 }) as i32)?.min(254);
        Ok(i64::from(
            decode_bandwidth(class, raw + 1)? - decode_bandwidth(class, raw)?,
        ))
    };
    let mut rewritten_text = None;
    let (candidate, accepted) = match input {
        NumberInput::Selected(event) if event.accepted && event.key == 1 && event.phase == 2 => {
            (i64::from(event.candidate), true)
        }
        NumberInput::Text(text) => {
            let (first, accepted) = numeric_text(text, crt_point, 0, min, max, 1)?;
            if accepted {
                let integer = first.to_string().into_bytes();
                let value = numeric_text(&integer, crt_point, 0, min, max, step(first, false)?)?.0;
                rewritten_text = Some(integer);
                (value, true)
            } else {
                (first, false)
            }
        }
        NumberInput::Next | NumberInput::Previous => {
            let step = step(old, input == NumberInput::Previous)?;
            let value = old.clamp(min, max)
                + if input == NumberInput::Next {
                    step
                } else {
                    -step
                };
            (
                if value < min {
                    max
                } else if value > max {
                    min
                } else {
                    value
                },
                true,
            )
        }
        _ => (old, false),
    };
    Ok(BandwidthEffect {
        number: NumberEffect {
            stored: false,
            accepted,
            candidate,
            raw_candidate: i32::from(encode_bandwidth(class, candidate as i32)?),
            state_out: 0,
            notify_flags: 0,
            suffix: Vec::new(),
            gray: false,
        },
        popup_requested: input == NumberInput::Begin,
        rewritten_text,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetBandwidthEffect {
    /// None means the actual case guard skipped the helper entirely.
    pub editor: Option<crate::icf::settings::extended::BandwidthEffect>,
    pub effects: Effects,
}

/// Preset389: guard before the shared BW helper, then preserve commit_preset's
/// sequential BW/filter-type/G-selection stores. Own maskFF only grays output.
pub fn edit_preset_bandwidth(
    image: &mut Icf<'_>,
    g: &[u8],
    p: usize,
    input: crate::icf::settings::extended::NumberInput<'_>,
    crt_point: u8,
    live_preset: usize,
) -> Result<PresetBandwidthEffect, Error> {
    let old = Preset::read(image.memory(), p)?;
    let class = old.bandwidth_class();
    let blocked = old.incompatible(3)? || class > 4;
    if blocked {
        return Ok(PresetBandwidthEffect {
            editor: None,
            effects: Effects {
                gray: old.masks[2] == 255,
                blocked: true,
                ..Effects::default()
            },
        });
    }
    let mut editor = bandwidth_input(class, old.body[3], input, crt_point)?;
    let accepted = editor.number.accepted;
    let effects = commit_preset(
        image,
        g,
        p,
        0x389,
        0,
        accepted.then_some(editor.number.candidate as i32),
        live_preset,
    )?;
    editor.number.stored = accepted;
    editor.number.notify_flags = effects.callback_flags;
    editor.number.gray = effects.gray;
    Ok(PresetBandwidthEffect {
        editor: Some(editor),
        effects,
    })
}

/// 7C891/7CB24/7CD14/7D8BD: parse the actual Preset old byte through the
/// shared A4870/A4E2C grammar before commit_preset. MaskFF only grays the
/// result; it does not block input. The level suffix uses the PRE-edit byte.
/// Cancel/false preserves both the field and related G selection byte. A later
/// commit error still preserves the original byte-store prefix.
pub fn edit_preset_number(
    image: &mut Icf<'_>,
    g: &[u8],
    p: usize,
    key: u16,
    input: crate::icf::settings::extended::NumberInput<'_>,
    crt_point: u8,
    live_preset: usize,
) -> Result<PresetNumberEffect, Error> {
    use crate::icf::settings::extended::{NumberEffect, NumberInput, numeric_text};
    if !matches!(key, 0x38d | 0x38f | 0x390 | 0x39a) {
        return Err(invalid("key is not a preset numeric editor"));
    }
    let old = i64::from(Preset::read(image.memory(), p)?.paint_byte(u32::from(key))?);
    let (min, max) = if key == 0x39a { (2, 223) } else { (0, 255) };
    let (candidate, accepted, state_out) = match input {
        NumberInput::Display | NumberInput::Cancel | NumberInput::Selected(_) => (old, false, 0),
        NumberInput::Begin => (old, false, 2),
        NumberInput::Next | NumberInput::Previous => {
            let value = old + if input == NumberInput::Next { 1 } else { -1 };
            (
                if value < min {
                    max
                } else if value > max {
                    min
                } else {
                    value
                },
                true,
                0,
            )
        }
        NumberInput::Text(text) if key == 0x39a => {
            (i64::from(crate::csv::number::civ_address(text)), true, 0)
        }
        NumberInput::Text(text) => {
            let (value, accepted) = numeric_text(text, crt_point, 0, min, max, 1)?;
            (if accepted { value } else { old }, accepted, 0)
        }
    };
    let effects = commit_preset(
        image,
        g,
        p,
        key,
        0,
        accepted.then_some(candidate as i32),
        live_preset,
    )?;
    let suffix = if state_out == 2 {
        Vec::new()
    } else if key == 0x39a {
        b"h".to_vec()
    } else {
        format!(" ({}%)", old * 100 / 255).into_bytes()
    };
    Ok(PresetNumberEffect {
        editor: NumberEffect {
            stored: accepted,
            accepted,
            candidate,
            raw_candidate: candidate as i32,
            state_out,
            notify_flags: effects.callback_flags,
            suffix,
            gray: effects.gray,
        },
        effects,
    })
}

/// Commit a helper-accepted full candidate, after the shared editor's input
/// and predicate gate. `None` is its false/ESC result. No post-hoc strict enum
/// repair is added. In particular current keying conflicts remain selectable.
pub fn commit_preset(
    image: &mut Icf<'_>,
    g: &[u8],
    p: usize,
    key: u16,
    side: usize,
    accepted: Option<i32>,
    live_preset: usize,
) -> Result<Effects, Error> {
    let (a, _) = preset_at(p)?;
    let f = *PRESET_CATALOG
        .iter()
        .find(|f| f.key == key)
        .ok_or(invalid("unknown preset key"))?;
    let old = Preset::read(image.memory(), p)?;
    let c = old.bandwidth_class();
    let blocked = match key {
        0x388 | 0x38a | 0x38b | 0x396 => old.incompatible(f.field)?,
        0x389 => old.incompatible(3)? || c > 4,
        _ => false,
    };
    let mut e = Effects {
        gray: old.masks[f.mask] == 255,
        blocked,
        ..Effects::default()
    };
    if blocked {
        return Ok(e);
    }
    let Some(value) = accepted else {
        return Ok(e);
    };
    let full = value as u32;
    if matches!(key, 0x392 | 0x396) {
        index(side, 2)?;
    }
    match key {
        0x387 => {
            let (mode, data) = match full {
                0 => (0, 0),
                1 => (1, 0),
                2 => (1, 1),
                3 => (2, 0),
                4 => (2, 1),
                5 => (3, 0),
                6 => (4, 0),
                7 => (5, 0),
                8 => (6, 0),
                9 => (7, 0),
                10 => (7, 1),
                11 => (8, 0),
                12 => (8, 1),
                13 => (10, 0),
                14 => (9, 0),
                _ => (11, old.body[1]),
            };
            put_byte(image, a, mode)?;
            put_byte(image, a + 1, data)?;
            let nc = Preset::read(image.memory(), p)?.bandwidth_class();
            if c != nc {
                if nc < 9 {
                    let filter = byte(g, 0x37823 + nc as usize)?;
                    put_byte(image, a + 2, filter)?;
                    let width = if nc < 5 {
                        byte(g, 0x37850 + 3 * nc as usize + usize::from(filter))?
                    } else {
                        0
                    };
                    put_byte(image, a + 3, width)?;
                }
                correct_filter_types(image, g, p)?;
                e.callback_flags |= 0x20;
            }
        }
        0x388 => {
            put_byte(image, a + 2, full as u8)?;
            if c < 9 {
                let w = if c < 5 {
                    byte(g, 0x37850 + 3 * c as usize + usize::from(full as u8))?
                } else {
                    0
                };
                put_byte(image, a + 3, w)?;
                correct_filter_types(image, g, p)?;
            }
            e.callback_flags |= 0x20;
        }
        0x389 => {
            put_byte(image, a + 3, encode_bandwidth(c, value)?)?;
            correct_filter_types(image, g, p)?;
            if c <= 1 {
                e.callback_flags |= 0x20;
            }
        }
        0x395 => {
            put_byte(image, a + 9, full as u8)?;
            if full != u32::from(old.body[9]) {
                let at = (full as usize)
                    .checked_mul(2)
                    .and_then(|n| n.checked_add(0x392ea))
                    .ok_or(invalid("TBW source address overflow"))?;
                put_byte(image, a + 10, byte(g, at)?)?;
                put_byte(image, a + 11, byte(g, at + 1)?)?;
                e.callback_flags |= 0x20;
            }
        }
        0x392 | 0x396 => put_byte(image, a + f.offset + side, full as u8)?,
        0x397 => {
            put_byte(image, a + 19, full as u8)?;
            if old.masks[16] == 0 {
                for j in 0..2 {
                    if old.masks[17 + j] == 0 && old.body[20 + j] == full as u8 {
                        put_byte(image, a + 20 + j, 0)?;
                        e.callback_flags |= 0x20;
                    }
                }
            }
        }
        _ => put_byte(image, a + f.offset, full as u8)?,
    }
    e.writes += 1;
    preset_finish(image, g, live_preset, &mut e)?;
    Ok(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> Icf<'static> {
        let mut m = vec![0xa5; 0x3c9c0];
        m[0x355a1] = 6;
        m[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        m[0xa60b..0xa60f].fill(255);
        m[0xa78c] = 255;
        Icf::from_memory(m).unwrap()
    }
    fn dword(m: &mut [u8], at: usize, n: u32) {
        m[at..at + 4].copy_from_slice(&n.to_le_bytes());
    }
    fn preset_fixture(p: usize) -> Icf<'static> {
        let mut i = image();
        let a = 0xa60f + 41 * p;
        let m = 0xa6dc + 22 * p;
        let mut b = [0; 41];
        b[0] = 1;
        b[4] = 255;
        b[5] = 255;
        b[25..].fill(b' ');
        i.patch_memory(&[(a, &b), (m, &[0; 22])]).unwrap();
        i
    }
    #[test]
    fn compact_all_windows_and_sixteen_display_fields_are_distinct_from_normal_memory() {
        let mut i = image();
        let a = 0x36f54 + 36 * 10;
        let mut literal = [0; 36];
        literal[..4].copy_from_slice(&145_500_000u32.to_le_bytes());
        literal[4] = 0x71;
        literal[5] = 0x93;
        literal[6] = 0xa5;
        literal[7..10].copy_from_slice(&[49, 50, 103]);
        literal[10..14].copy_from_slice(&0x1020_3040u32.to_le_bytes());
        literal[14] = 99;
        literal[15..22].copy_from_slice(&pack_callsign(b"CQCQCQ  "));
        literal[22..29].copy_from_slice(&pack_callsign(b"ABCDEFGG"));
        literal[29..36].copy_from_slice(&pack_callsign(b"ABCDEFG "));
        i.patch_memory(&[(a, &literal)]).unwrap();
        let h = CompactChannel::read(i.memory(), CompactKind::Memopad, 10).unwrap();
        assert_eq!(
            (h.mode, h.filter, h.duplex, h.data, h.polarity),
            (0xd, 1, 2, 1, 3)
        );
        assert_eq!(
            (h.tx_tone, h.rx_tone, h.dtcs, h.digital_code),
            (49, 50, 103, 99)
        );
        assert_eq!(h.offset_hz, 0x1020_3040);
        assert_eq!(h.callsigns[1], *b"ABCDEFGG");
        let fields = [
            CompactField::Frequency,
            CompactField::Duplex,
            CompactField::Offset,
            CompactField::Mode,
            CompactField::Data,
            CompactField::Filter,
            CompactField::Tone,
            CompactField::TxTone,
            CompactField::RxTone,
            CompactField::Dtcs,
            CompactField::Polarity,
            CompactField::DigitalSquelch,
            CompactField::DigitalCode,
            CompactField::Your,
            CompactField::Repeater1,
            CompactField::Repeater2,
        ];
        for field in fields {
            assert!(!matches!(h.display(field), DisplayValue::Blank));
        }
        assert_eq!(h.display(CompactField::Your), DisplayValue::Placeholder);
        let mut dv = h.clone();
        dv.mode = 0x10;
        assert_eq!(
            dv.display(CompactField::Your),
            DisplayValue::Callsign(*b"CQCQCQ  ")
        );
        assert_eq!(dv.display(CompactField::Tone), DisplayValue::Placeholder);
        assert_eq!(dv.display(CompactField::Data), DisplayValue::Placeholder);
        dv.mode = 0x11;
        assert_eq!(dv.display(CompactField::Filter), DisplayValue::Number(0));
        for code in [1, 8, 13, 32, 0x7100] {
            assert!(h.input_blocked(code, 0));
        }
        assert!(!h.input_blocked(0, 0));
        assert!(!h.input_blocked(0x10001, 0));
        assert!(h.input_blocked(0, 0xdc));
        let before = i.memory().to_vec();
        let mut h = h;
        h.save(&mut i, CompactKind::Memopad, 10).unwrap();
        let mut expected = before.clone();
        expected[a + 10..a + 14].copy_from_slice(&9_999_900u32.to_le_bytes());
        expected[a + 22..a + 29].copy_from_slice(&pack_callsign(b"ABCDEFG "));
        assert_eq!(&i.memory()[a..a + 36], &expected[a..a + 36]);
        assert!(i.memory() == expected);
        assert_eq!(i.memory()[a + 6] & 0xfc, 0xa4);
        let mut offsets = Vec::new();
        for row in 0..45 {
            offsets.push(CompactKind::BandStack.offset(row).unwrap());
        }
        offsets.sort_unstable();
        assert_eq!(
            offsets,
            (0..45).map(|r| 0x368eb + 36 * r).collect::<Vec<_>>()
        );
        assert_eq!(
            CompactKind::BandStack.offset(30).unwrap(),
            0x368eb + 108 * 12
        );
        assert_eq!(CompactKind::BandStack.offset(44).unwrap() + 36, 0x36f3f);
        assert_eq!(CompactKind::Memopad.offset(0).unwrap(), 0x36f54);
    }
    #[test]
    fn compact_g_reset_empty_tail_preservation_band_copy_and_planned_notifications() {
        let mut i = image();
        let before = i.memory().to_vec();
        let mut g = vec![0x5a; 0x4997a];
        for r in 1..11 {
            dword(&mut g, 0x36f54 + 36 * r, u32::MAX);
        }
        let e = reset_compact(&mut i, &g, CompactKind::Memopad, 1, true).unwrap();
        let mut expected = before;
        for r in 1..11 {
            dword(&mut expected, 0x36f54 + 36 * r, u32::MAX);
        }
        assert_eq!(i.memory(), expected);
        assert_eq!(e.writes, 10);
        assert_eq!(e.notifications.len(), 12);
        for r in 0..45 {
            let a = CompactKind::BandStack.offset(r).unwrap();
            g[a..a + 36].fill(0);
            g[a + 4] = 0x10;
            dword(&mut g, a, 30_000 + r as u32);
        }
        let mut clip = CompactClipboard::default();
        clip.copy(&g, CompactKind::BandStack, 0..45, false).unwrap();
        assert_eq!(clip.reserve_count, 42);
        assert_eq!(clip.source_rows, (0..27).chain(30..45).collect::<Vec<_>>());
        let e = reset_compact(&mut i, &g, CompactKind::BandStack, 1, false).unwrap();
        assert_eq!(e.writes, 42);
        assert_eq!(e.notifications.len(), 47);
        for (dst, src) in (0..27).chain(30..45).enumerate() {
            assert_eq!(
                CompactChannel::read(i.memory(), CompactKind::BandStack, dst)
                    .unwrap()
                    .frequency_hz,
                30_000 + src as u32
            );
        }
        let before = i.memory().to_vec();
        assert_eq!(
            reset_compact(&mut i, &[], CompactKind::BandStack, 0, false)
                .unwrap()
                .writes,
            0
        );
        assert_eq!(i.memory(), before);
        assert!(
            clip.copy(&g, CompactKind::BandStack, 24..28, false)
                .is_err()
        );
        assert_eq!(clip.reserve_count, 1);
        assert_eq!(clip.source_rows, vec![24]);
        let visible: Vec<_> = (0..27).chain(30..45).collect();
        for only in [false, true] {
            let visible = if only {
                (0..45).collect::<Vec<_>>()
            } else {
                visible.clone()
            };
            for &a in &visible {
                for &b in &visible {
                    clip.copy(&g, CompactKind::BandStack, a.min(b)..a.max(b) + 1, only)
                        .unwrap();
                    assert_eq!(clip.reserve_count as usize, clip.channels.len());
                }
            }
        }
    }
    #[test]
    fn scan_literal_initialization_reparse_selection_and_failure_prefix() {
        let mut i = image();
        let mut g = vec![0; 0x4997a];
        g[0x37810..0x37814].copy_from_slice(&[0x11, 0x22, 0x33, 0x44]);
        i.patch_memory(&[(0x37802, &[0x83]), (0x37807, &[255])])
            .unwrap();
        let input = FrequencyInput {
            first_accepted: true,
            second_value: 30_000,
            second_accepted: false,
        };
        let e = edit_scan_frequency(&mut i, &[], 7, EdgeSide::Low, input, true, |_| false).unwrap();
        assert_eq!(e.warning_bits, 0);
        let a = 0xa368 + 27 * 7;
        let mut literal = vec![0x30, 0x75, 0, 0, 0x30, 0x75, 0, 0, 255, 0, 255];
        literal.extend_from_slice(&[b' '; 16]);
        assert_eq!(&i.memory()[a..a + 27], literal);
        let e = edit_scan_frequency(
            &mut i,
            &g,
            7,
            EdgeSide::High,
            FrequencyInput {
                second_value: 60_000,
                ..input
            },
            true,
            |_| false,
        )
        .unwrap();
        assert_eq!(e.writes, 1);
        assert_eq!(i.memory()[0x37802], 0x9f);
        assert_eq!(i.memory()[0x37807], 254);
        assert_eq!(&i.memory()[0x37810..0x37814], &[0x11, 0x22, 0x33, 0x44]);
        let before = i.memory().to_vec();
        let e = edit_scan_frequency(
            &mut i,
            &[],
            7,
            EdgeSide::High,
            FrequencyInput {
                first_accepted: false,
                ..input
            },
            true,
            |_| false,
        )
        .unwrap();
        assert_eq!(e.writes, 0);
        assert_eq!(e.callback_flags, 8);
        assert_eq!(i.memory(), before);
        let err = edit_scan_frequency(
            &mut i,
            &[],
            7,
            EdgeSide::High,
            FrequencyInput {
                second_value: 70_000,
                ..input
            },
            true,
            |_| false,
        );
        assert!(err.is_err());
        assert_eq!(ScanEdge::read(i.memory(), 7).unwrap().high_hz, 70_000);
        // Selection and flag stores occur before a failing G-word read.
        assert_eq!(i.memory()[0x37807] & 1, 0);
        let mut h = ScanEdge::read(i.memory(), 7).unwrap();
        h.low_hz = 100;
        h.high_hz = 50;
        assert_eq!(h.warnings(EdgeSide::High, |f| f == 100), 8); // Bad low skips high ordering.
        assert_eq!(h.warnings(EdgeSide::High, |_| false), 12);
        assert!(h.mode_candidate_allowed(h.mode));
        assert!(!h.mode_candidate_allowed(0x18));
        h.mode = 0xf;
        assert!(h.filter_candidate_allowed(255));
        assert!(!h.filter_candidate_allowed(3));
    }
    #[test]
    fn scan_fields_raw_width_empty_name_cancel_and_cross_generation_suitability() {
        let mut i = image();
        let before = i.memory().to_vec();
        let e = edit_scan_field(&mut i, 0, ScanField::Name, None, None).unwrap();
        assert_eq!(e.node_style_clear, 3);
        assert_eq!(i.memory(), before);
        let e = edit_scan_field(&mut i, 0, ScanField::Name, None, Some(b"X")).unwrap();
        assert_eq!(e.writes, 1);
        assert_eq!(&i.memory()[0xa368 + 11..0xa368 + 27], &[b' '; 16]);
        let a = 0xa368;
        i.patch_memory(&[(0xa60b, &[254]), (a, &[1, 0, 0, 0, 1, 0, 0, 0, 7, 1, 2])])
            .unwrap();
        let e = edit_scan_field(&mut i, 0, ScanField::Data, Some(0x101), None).unwrap();
        assert!(!e.blocked);
        assert_eq!(i.memory()[a + 9], 1);
        let e = edit_scan_field(&mut i, 0, ScanField::Filter, None, None).unwrap();
        assert_eq!(e.phase, Some(2));
        i.patch_memory(&[(a + 8, &[8])]).unwrap(); // stored WFM -> holder F, blocked filter helper result ignored
        let before = i.memory().to_vec();
        let e = edit_scan_field(&mut i, 0, ScanField::Filter, Some(2), None).unwrap();
        assert!(e.blocked);
        assert_eq!(i.memory(), before);
        let mut source = image();
        let mut h = ScanEdge {
            valid: true,
            low_hz: 30_000,
            high_hz: 60_000,
            mode: 0x19,
            data: 0,
            filter: 255,
            name: [b' '; 16],
        };
        h.save(&mut source, 4).unwrap();
        let mut live = source.memory().to_vec();
        dword(&mut live, 0xa368 + 27 * 4, 1);
        assert!(
            scan_suitable_rows(source.memory(), &live, false, false, false, |_| false)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            scan_suitable_rows(
                source.memory(),
                source.memory(),
                false,
                false,
                false,
                |_| false
            )
            .unwrap(),
            [4]
        );
    }
    #[test]
    fn user_signed_skip_zero_reload_accepted_chain_and_equal_gray() {
        let mut i = image();
        for row in 0..30 {
            put(&mut i, 0x378c0 + 8 * row, &[0; 8]).unwrap();
        }
        let input = FrequencyInput {
            first_accepted: true,
            second_value: u32::MAX,
            second_accepted: false,
        };
        let before = i.memory().to_vec();
        let e = edit_user_frequency(&mut i, 2, EdgeSide::Low, input, |_| false, |_| 0).unwrap();
        assert_eq!(e.callback_flags, 0x28);
        assert_eq!(e.writes, 0);
        assert_eq!(i.memory(), before);
        let input = FrequencyInput {
            second_value: 50_000,
            ..input
        };
        let e = edit_user_frequency(&mut i, 2, EdgeSide::Low, input, |_| false, |_| 0).unwrap();
        assert!(!e.gray);
        assert_eq!(
            UserEdge::read(i.memory(), 2).unwrap(),
            UserEdge {
                valid: true,
                low_hz: 50_000,
                high_hz: 50_000
            }
        );
        assert!(
            UserEdge::warnings(i.memory(), 2, EdgeSide::High, |_| false, |_| 0)
                .unwrap()
                .1
        );
        for (row, lo, hi) in [
            (0, 30_000, 40_000),
            (1, 35_000, 9_000_000),
            (2, 50_000, 60_000),
        ] {
            let a = 0x378c0 + 8 * row;
            put(
                &mut i,
                a,
                &[lo, hi]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        }
        assert_eq!(
            UserEdge::warnings(i.memory(), 2, EdgeSide::Low, |_| false, |_| 0)
                .unwrap()
                .0,
            0
        );
        put(&mut i, 0x378c0 + 8 * 2, &0u32.to_le_bytes()).unwrap();
        assert!(!UserEdge::read(i.memory(), 2).unwrap().valid);
        assert_eq!(word32(i.memory(), 0x378c0 + 8 * 2 + 4).unwrap(), 60_000);
    }
    #[test]
    fn preset_native_paint_keeps_marker_units_and_raw_sources_read_only() {
        for p in 0..5 {
            let i = preset_fixture(p);
            let memory = i.memory().to_vec();
            let mut h = Preset::read(i.memory(), p).unwrap();
            assert_eq!(h.checkbox_paint(0).unwrap(), (b"--".to_vec(), None));
            assert_eq!(h.checkbox_paint(23).unwrap(), (b"--".to_vec(), None));
            for mask in [0, 1, 255] {
                h.masks[0] = mask;
                let before = h.clone();
                assert_eq!(h.checkbox_paint(1).unwrap(), (Vec::new(), Some(mask == 0)));
                assert_eq!(h, before);
            }
            assert_eq!(h.checkbox_paint(2).unwrap(), (b"--".to_vec(), None));
            h.masks.fill(0);
            h.body[3] = 10;
            for mask in [0, 1, 255] {
                h.masks[4] = mask;
                assert_eq!(
                    h.checkbox_paint(0x16).unwrap(),
                    (Vec::new(), Some(mask == 0))
                );
            }
            h.masks[2] = 255;
            assert_eq!(h.bandwidth_paint().unwrap(), b"600Hz ");
            assert_eq!(h.checkbox_paint(3).unwrap(), (Vec::new(), Some(false)));
            for (mode, raw, expected) in [
                (1, 0, b"50Hz ".as_slice()),
                (1, 9, b"500Hz "),
                (1, 10, b"600Hz "),
                (1, 255, b"25100Hz "),
                (7, 0, b"200Hz "),
                (7, 49, b"10000Hz "),
                (7, 255, b"51200Hz "),
                (8, 255, b"---"),
                (255, 255, b"---"),
            ] {
                h.body[0] = mode;
                h.body[3] = raw;
                let before = h.clone();
                assert_eq!(h.bandwidth_paint().unwrap(), expected);
                assert_eq!(h, before);
            }
            h.body[0] = 1;
            h.masks[1] = 255;
            assert_eq!(h.bandwidth_paint().unwrap(), b"---");
            h.masks.fill(255);
            for raw in 0..=u8::MAX {
                h.body[15] = raw;
                h.body[16] = raw.wrapping_add(1);
                h.body[18] = raw.wrapping_add(2);
                h.body[22] = raw.wrapping_add(3);
                let before = h.clone();
                assert_eq!(h.paint_byte(0x390).unwrap(), raw);
                assert_eq!(h.paint_byte(0x38d).unwrap(), raw.wrapping_add(1));
                assert_eq!(h.paint_byte(0x38f).unwrap(), raw.wrapping_add(2));
                assert_eq!(h.paint_byte(0x39a).unwrap(), raw.wrapping_add(3));
                assert_eq!(h, before);
            }
            for key in [0x385, 0x389, 0x38c, 0x39b, u32::MAX] {
                assert!(h.paint_byte(key).is_err());
            }
            assert!(Preset::read(i.memory(), 5).is_err());
            assert!(Preset::read(&i.memory()[..0xa60f], 0).is_err());
            assert_eq!(i.memory(), memory);
        }
    }
    #[test]
    fn preset_all_parts_domains_mask_semantics_and_usb_directions() {
        let independent = [
            (0x387, 0, 0),
            (0x388, 2, 1),
            (0x389, 3, 2),
            (0x38a, 4, 3),
            (0x38b, 5, 4),
            (0x394, 6, 5),
            (0x393, 7, 6),
            (0x391, 8, 7),
            (0x395, 9, 8),
            (0x396, 10, 9),
            (0x392, 12, 10),
            (0x38c, 14, 11),
            (0x390, 15, 12),
            (0x38d, 16, 13),
            (0x38e, 17, 14),
            (0x38f, 18, 15),
            (0x397, 19, 16),
            (0x398, 20, 17),
            (0x399, 21, 18),
            (0x39a, 22, 19),
            (0x39b, 23, 20),
            (0x39c, 24, 21),
        ];
        for (f, (key, offset, mask)) in PRESET_CATALOG.iter().zip(independent) {
            assert_eq!((f.key, f.offset, f.mask), (key, offset, mask));
        }
        for p in 0..5 {
            let mut i = preset_fixture(p);
            let a = 0xa60f + 41 * p;
            let m = 0xa6dc + 22 * p;
            put(&mut i, a + 19, &[2, 2, 2]).unwrap();
            put_byte(&mut i, m + 16, 255).unwrap();
            toggle_preset(&mut i, &[], p, 0x10, true, 13, 99).unwrap();
            assert_eq!(&i.memory()[a + 19..a + 22], &[0, 2, 2]);
            put(&mut i, a + 19, &[2, 2, 2]).unwrap();
            put_byte(&mut i, m + 16, 1).unwrap();
            toggle_preset(&mut i, &[], p, 0x10, true, 13, 99).unwrap();
            assert_eq!(&i.memory()[a + 19..a + 22], &[2, 2, 2]);
            put_byte(&mut i, m + 18, 1).unwrap();
            commit_preset(&mut i, &[], p, 0x397, 0, Some(2), 99).unwrap();
            assert_eq!(&i.memory()[a + 19..a + 22], &[2, 0, 2]);
            let h = Preset::read(i.memory(), p).unwrap();
            assert!(h.keying_candidate_allowed(0, 0).unwrap());
            assert!(!h.keying_candidate_allowed(0, 2).unwrap());
            assert!(h.keying_candidate_allowed(1, 2).unwrap());
            put_byte(&mut i, m, 1).unwrap();
            let h = Preset::read(i.memory(), p).unwrap();
            assert!(!h.incompatible(2).unwrap());
            put_byte(&mut i, m, 255).unwrap();
            assert!(
                Preset::read(i.memory(), p)
                    .unwrap()
                    .incompatible(2)
                    .unwrap()
            );
            let before = i.memory().to_vec();
            let e = commit_preset(&mut i, &[], p, 0x388, 0, Some(0), 99).unwrap();
            assert!(e.blocked);
            assert_eq!(i.memory(), before);
        }
    }
    #[test]
    fn preset_linked_order_same_class_same_filter_bw_tbw_and_partial_g_failure() {
        let mut i = preset_fixture(4);
        let a = 0xa60f + 41 * 4;
        let mut g = vec![0; 0x4997a];
        g[0x37825] = 2;
        g[0x37858] = 10;
        g[0x37862] = 7;
        g[0x37866] = 8;
        put_byte(&mut i, a + 5, 12).unwrap();
        let e = commit_preset(&mut i, &g, 4, 0x387, 0, Some(5), 99).unwrap();
        assert_eq!(&i.memory()[a..a + 6], &[3, 0, 2, 10, 7, 12]);
        assert_eq!(e.callback_flags, 0x20);
        put_byte(&mut i, a + 3, 99).unwrap();
        commit_preset(&mut i, &g, 4, 0x388, 0, Some(2), 99).unwrap();
        assert_eq!(i.memory()[a + 3], 10);
        let e = commit_preset(&mut i, &g, 4, 0x389, 0, Some(550), 99).unwrap();
        assert_eq!(i.memory()[a + 3], 9);
        assert_eq!(e.callback_flags, 0);
        g[0x392ee..0x392f0].copy_from_slice(&[3, 2]);
        commit_preset(&mut i, &g, 4, 0x395, 0, Some(2), 99).unwrap();
        assert_eq!(&i.memory()[a + 9..a + 12], &[2, 3, 2]);
        commit_preset(&mut i, &[], 4, 0x395, 0, Some(2), 99).unwrap();
        let mut i = preset_fixture(0);
        commit_preset(&mut i, &[], 0, 0x387, 0, Some(3), 99).unwrap();
        assert_eq!(&i.memory()[0xa60f..0xa611], &[2, 0]); // Same class skips G entirely.
        assert!(commit_preset(&mut i, &[], 0, 0x387, 0, Some(5), 99).is_err());
        assert_eq!(&i.memory()[0xa60f..0xa611], &[3, 0]); // Mode/data prefix survives failed filter source.
        for class in 0..5 {
            for raw in 0..=255 {
                let z = i32::from(raw);
                let expected = if class == 4 {
                    200 * (z + 1)
                } else {
                    if z <= 9 {
                        50 * (z + 1)
                    } else {
                        500 + 100 * (z - 9)
                    }
                };
                assert_eq!(decode_bandwidth(class, raw).unwrap(), expected);
                assert_eq!(encode_bandwidth(class, expected).unwrap(), raw);
            }
        }
        assert_eq!(encode_bandwidth(0, 1).unwrap(), 0);
        assert_eq!(encode_bandwidth(0, -1).unwrap(), 255);
    }
    #[test]
    fn preset_reset_rename_cancel_live_node_and_dispatch_failure_prefix() {
        let mut i = preset_fixture(0);
        let mut g = vec![0x5a; 0x4997a];
        g[0xa78c] = 4;
        let before = i.memory().to_vec();
        assert_eq!(reset_preset(&mut i, &[], 0, 6).unwrap().writes, 0);
        assert_eq!(i.memory(), before);
        let e = reset_preset(&mut i, &g, 0, 1).unwrap();
        assert_eq!(&i.memory()[0xa60f..0xa638], &[0x5a; 41]);
        assert_eq!(&i.memory()[0xa6dc..0xa6f2], &[0x5a; 22]);
        assert_eq!(i.memory()[0xa78c], 255);
        assert_eq!(
            e.notifications.iter().map(|n| n.event).collect::<Vec<_>>(),
            [0x11, 0x3c]
        );
        let mut i = preset_fixture(0);
        put_byte(&mut i, 0xa78c, 3).unwrap();
        let mut delivered = Vec::new();
        let e = rename_preset(
            &mut i,
            &g,
            0,
            3,
            Some(b"ABC"),
            |s| Ok(s.to_vec()),
            |n| {
                delivered.push((n.event, n.synchronous));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(i.memory()[0xa78c], 4);
        assert_eq!(&i.memory()[0xa628..0xa638], b"ABC             ");
        assert_eq!(delivered, [(0x3e, false), (0x39, true), (0x11, false)]);
        assert!(e.notifications.is_empty());
        let mut seen = Vec::new();
        put_byte(&mut i, 0xa78c, 3).unwrap();
        assert!(
            rename_preset(
                &mut i,
                &g,
                0,
                3,
                Some(b"ABC"),
                |s| Ok(s.to_vec()),
                |n| {
                    seen.push(n.event);
                    if n.event == 0x39 {
                        Err(invalid("injected receiver error"))
                    } else {
                        Ok(())
                    }
                }
            )
            .is_err()
        );
        assert_eq!(seen, [0x3e, 0x39]);
        assert_eq!(&i.memory()[0xa628..0xa62b], b"ABC");
        assert_eq!(i.memory()[0xa78c], 3); // The tail after failed synchronous 39 was not reached.
        assert!(
            rename_preset(
                &mut i,
                &g,
                0,
                0,
                Some(b"DEF"),
                |_| Err(invalid("injected label error")),
                |_| Ok(())
            )
            .is_err()
        );
        assert_eq!(&i.memory()[0xa628..0xa62b], b"DEF");
        let mut i = preset_fixture(0);
        let before = i.memory().to_vec();
        assert!(reset_preset(&mut i, &g[..0xa638], 0, 1).is_err());
        assert_eq!(&i.memory()[0xa60f..0xa638], &[0x5a; 41]);
        assert_eq!(&i.memory()[0xa6dc..0xa6f2], &before[0xa6dc..0xa6f2]);
    }
    #[test]
    fn scan_row_order_reference_context_paste_and_cancel() {
        let mut i = image();
        let g = image().memory().to_vec();
        for r in 0..25 {
            ScanEdge {
                valid: true,
                low_hz: 30_000 + r as u32 * 1000,
                high_hz: 40_000 + r as u32 * 1000,
                mode: 0x19,
                data: 0,
                filter: 255,
                name: [r as u8; 16],
            }
            .save(&mut i, r)
            .unwrap();
        }
        put_byte(&mut i, 0x37802, 5 << 2).unwrap();
        let mut state = RowState::default();
        let e = scan_rows(
            &mut i,
            &g,
            2..4,
            ScanRowAction::Delete,
            true,
            &mut state,
            false,
        )
        .unwrap();
        assert_eq!(ScanEdge::read(i.memory(), 2).unwrap().name, [4; 16]);
        assert!(!ScanEdge::read(i.memory(), 24).unwrap().valid);
        assert_eq!((i.memory()[0x37802] >> 2) & 31, 3);
        assert_eq!(state.active, 2);
        assert_eq!(state.bits & 4, 0);
        assert_eq!(
            e.notifications
                .iter()
                .rev()
                .take(2)
                .map(|n| n.event)
                .collect::<Vec<_>>(),
            [0x3c, 0x11]
        );
        let e = scan_rows(
            &mut i,
            &g,
            1..3,
            ScanRowAction::Insert,
            true,
            &mut state,
            false,
        )
        .unwrap();
        assert!(!ScanEdge::read(i.memory(), 1).unwrap().valid);
        assert_eq!(state.bits & 4, 4);
        assert_eq!(
            e.notifications
                .iter()
                .rev()
                .take(2)
                .map(|n| n.event)
                .collect::<Vec<_>>(),
            [0x11, 0x3c]
        );
        let before = i.memory().to_vec();
        let old = state.clone();
        scan_rows(
            &mut i,
            &[],
            0..1,
            ScanRowAction::Up,
            true,
            &mut state,
            false,
        )
        .unwrap();
        assert_eq!(i.memory(), before);
        assert_eq!(state, old);
        scan_rows(
            &mut i,
            &[],
            0..1,
            ScanRowAction::Clear,
            false,
            &mut state,
            false,
        )
        .unwrap();
        assert_eq!(i.memory(), before);
        let mut clip = ScanClipboard::default();
        clip.copy(i.memory(), 3..6, &mut state).unwrap();
        let e = clip
            .paste(
                &mut i,
                &g,
                24,
                PasteApproval {
                    tail: false,
                    overwrite: true,
                },
                &mut state,
                false,
            )
            .unwrap();
        assert_eq!(e.writes, 0);
        let e = clip
            .paste(
                &mut i,
                &g,
                24,
                PasteApproval {
                    tail: true,
                    overwrite: true,
                },
                &mut state,
                false,
            )
            .unwrap();
        assert_eq!(e.writes, 1);
        assert_eq!((state.start, state.end), (24, 24));
    }
    #[test]
    fn scan_default_source_failure_keeps_moved_prefix_and_source_guards_short_circuit() {
        let mut original = image();
        for row in 0..25 {
            ScanEdge {
                valid: true,
                low_hz: 30_000,
                high_hz: 60_000,
                mode: 0x19,
                data: 0,
                filter: 255,
                name: [row as u8; 16],
            }
            .save(&mut original, row)
            .unwrap();
        }
        for (action, at, source) in [
            (ScanRowAction::Delete, 2, 4),
            (ScanRowAction::Insert, 24, 22),
        ] {
            let mut i = Icf::from_memory(original.memory().to_vec()).unwrap();
            let mut state = RowState::default();
            assert!(scan_rows(&mut i, &[], 2..4, action, true, &mut state, false).is_err());
            assert_eq!(ScanEdge::read(i.memory(), at).unwrap().name, [source; 16]);
            assert_eq!(state, RowState::default());
        }
        assert!(
            scan_suitable_rows(image().memory(), &[], false, false, false, |_| false)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn scan_move_matches_slice_rotation_and_selected_row_identity() {
        let mut original = image();
        for row in 0..25 {
            ScanEdge {
                valid: true,
                low_hz: 30_000,
                high_hz: 60_000,
                mode: 0x19,
                data: 0,
                filter: 255,
                name: [row as u8; 16],
            }
            .save(&mut original, row)
            .unwrap();
        }
        for up in [true, false] {
            for start in 0..25 {
                for end in start + 1..=25 {
                    if up && start == 0 || !up && end == 25 {
                        continue;
                    }
                    let mut expected = (0u8..25).collect::<Vec<_>>();
                    if up {
                        expected[start - 1..end].rotate_left(1);
                    } else {
                        expected[start..end + 1].rotate_right(1);
                    }
                    let selected = if up { start - 1 } else { end };
                    let mut i = Icf::from_memory(original.memory().to_vec()).unwrap();
                    put_byte(&mut i, 0x37802, (selected as u8) << 2).unwrap();
                    let mut state = RowState::default();
                    let e = scan_rows(
                        &mut i,
                        &[],
                        start..end,
                        if up {
                            ScanRowAction::Up
                        } else {
                            ScanRowAction::Down
                        },
                        true,
                        &mut state,
                        false,
                    )
                    .unwrap();
                    for (row, &old) in expected.iter().enumerate() {
                        assert_eq!(ScanEdge::read(i.memory(), row).unwrap().name, [old; 16]);
                    }
                    assert_eq!(
                        usize::from((i.memory()[0x37802] >> 2) & 31),
                        expected
                            .iter()
                            .position(|&old| usize::from(old) == selected)
                            .unwrap()
                    );
                    assert!(e.notifications.is_empty());
                    assert_eq!(state.state, 1);
                }
            }
        }
    }
    #[test]
    fn public_raw_adapters_preserve_ordered_failure_prefixes_without_a_second_codec() {
        let mut old = [0; 36];
        old[..4].copy_from_slice(&123u32.to_le_bytes());
        old[4] = 0x71;
        old[6] = 0xa5;
        assert!(!CompactChannel::decode_bound(None, false).unwrap().valid);
        assert!(CompactChannel::decode_bound(None, true).is_err());
        let mut shared = old;
        shared[..4].fill(255);
        assert!(!CompactChannel::decode(&shared).unwrap().valid);
        let forced = CompactChannel::decode_bound(Some(&shared), true).unwrap();
        assert_eq!(CompactChannel::decode_valid(&shared, true).unwrap(), forced);
        assert!(!CompactChannel::decode_valid(&shared, false).unwrap().valid);
        assert!(forced.valid);
        assert_eq!(
            (forced.frequency_hz, forced.mode, forced.filter),
            (u32::MAX, 13, 1)
        );
        for fail in 0..=2 {
            let mut h = CompactChannel::decode(&old).unwrap();
            h.frequency_hz = 456;
            let mut target = old;
            let mut stage = 0;
            let result = h.write_to(&old, |at, bytes| {
                let this = stage;
                stage += 1;
                if this == fail {
                    return Err(invalid("injected compact callback failure"));
                }
                target[at..at + bytes.len()].copy_from_slice(bytes);
                Ok(())
            });
            assert_eq!(result.is_ok(), fail == 2);
            let expected_frequency = 456u32.to_le_bytes();
            assert_eq!(
                &target[..4],
                if fail == 0 {
                    &old[..4]
                } else {
                    &expected_frequency
                }
            );
            if fail < 2 {
                assert_eq!(&target[4..], &old[4..]);
            }
            assert_eq!(target[6] & 0xfc, 0xa4);
        }
        for fail in 0..=2 {
            let mut h = ScanEdge {
                valid: false,
                low_hz: 123,
                high_hz: 456,
                mode: 25,
                data: 3,
                filter: 4,
                name: [b'X'; 16],
            };
            let mut target = vec![0xa4; 0x4997a];
            let mut stage = 0;
            let result = h.save_with(24, 0xa4, |at, bytes| {
                let this = stage;
                stage += 1;
                if this == fail {
                    return Err(invalid("injected scan callback failure"));
                }
                target[at..at + bytes.len()].copy_from_slice(bytes);
                Ok(())
            });
            assert_eq!(result.is_ok(), fail == 2);
            assert_eq!(target[0xa60e], if fail == 0 { 0xa4 } else { 0xa5 });
            assert_eq!(h.low_hz, if fail == 0 { 123 } else { 0 });
            let at = 0xa368 + 27 * 24;
            if fail < 2 {
                assert_eq!(&target[at..at + 27], &[0xa4; 27]);
            } else {
                assert_eq!(&target[at..at + 11], &[0, 0, 0, 0, 0, 0, 0, 0, 255, 3, 4]);
                assert_eq!(&target[at + 11..at + 27], &[b' '; 16]);
            }
        }
        let mut h = ScanEdge::read(image().memory(), 0).unwrap();
        assert!(
            h.save_with(25, 0, |_, _| panic!("invalid row reached callback"))
                .is_err()
        );
    }
    #[test]
    fn checked_indices_reject_before_mutation_and_unknown_values_remain_readable() {
        let mut i = preset_fixture(0);
        let before = i.memory().to_vec();
        for r in [25, usize::MAX] {
            assert!(edit_scan_field(&mut i, r, ScanField::Mode, Some(0), None).is_err());
        }
        assert!(
            edit_user_frequency(
                &mut i,
                30,
                EdgeSide::Low,
                FrequencyInput {
                    first_accepted: true,
                    second_value: 1,
                    second_accepted: true
                },
                |_| false,
                |_| 0
            )
            .is_err()
        );
        assert!(reset_preset(&mut i, &[], 5, 1).is_err());
        assert_eq!(i.memory(), before);
        assert!(CompactKind::BandStack.offset(45).is_err());
        assert!(CompactKind::Memopad.offset(11).is_err());
        put_byte(&mut i, 0xa60f + 6, 0xf3).unwrap();
        put_byte(&mut i, 0xa6dc + 5, 1).unwrap();
        let h = Preset::read(i.memory(), 0).unwrap();
        assert_eq!((h.body[6], h.masks[5]), (0xf3, 1));
        let before = i.memory().to_vec();
        commit_preset(&mut i, &[], 0, 0x394, 0, None, 99).unwrap();
        assert_eq!(i.memory(), before);
        commit_preset(&mut i, &[], 0, 0x394, 0, Some(1), 99).unwrap();
        assert_eq!(i.memory()[0xa60f + 6], 1);
        assert_eq!(i.memory()[0xa6dc + 5], 1);
    }

    #[test]
    fn preset_number_text_uses_literal_fields_masks_and_related_g_tail() {
        use crate::icf::settings::extended::NumberInput;
        let mut g = vec![0; 0xa78d];
        g[0xa78c] = 0x6b;
        // BA02d: AF/IF/MOD/+address use offsets10/12/0F/16 and masks13/15/12/19.
        for p in 0..5 {
            for (key, offset, mask, old, text, value, suffix) in [
                (
                    0x38d,
                    0x10,
                    13,
                    127,
                    b".".as_slice(),
                    0,
                    b" (49%)".as_slice(),
                ),
                (0x38f, 0x12, 15, 127, b".", 0, b" (49%)"),
                (0x390, 0x0f, 12, 127, b".", 0, b" (49%)"),
                (0x39a, 0x16, 19, 70, b"", 2, b"h"),
            ] {
                for mask_value in [0, 1, 255] {
                    for selected in [p as u8, 0x7f] {
                        let mut image = preset_fixture(p);
                        let at = 0xa60f + 41 * p + offset;
                        image
                            .patch_memory(&[
                                (at, &[old]),
                                (0xa6dc + 22 * p + mask, &[mask_value]),
                                (0xa78c, &[selected]),
                            ])
                            .unwrap();
                        let mut expected = image.memory().to_vec();
                        let out = edit_preset_number(
                            &mut image,
                            &g,
                            p,
                            key,
                            NumberInput::Text(text),
                            b'.',
                            p,
                        )
                        .unwrap();
                        expected[at] = value;
                        if selected == p as u8 {
                            expected[0xa78c] = 0x6b;
                        }
                        assert_eq!(image.memory(), expected, "p{p} key{key:X} mask{mask_value}");
                        assert!(out.editor.accepted && out.editor.stored);
                        assert_eq!(out.editor.candidate, i64::from(value));
                        assert_eq!(out.editor.raw_candidate, i32::from(value));
                        assert_eq!(out.editor.suffix, suffix);
                        assert_eq!((out.editor.state_out, out.editor.notify_flags), (0, 0));
                        assert_eq!(
                            (out.editor.gray, out.effects.gray),
                            (mask_value == 255, mask_value == 255)
                        );
                        assert_eq!(out.effects.writes, 1 + usize::from(selected == p as u8));
                        assert!(!out.effects.blocked);
                    }
                }
            }
        }
    }

    #[test]
    fn preset_number_false_cancel_begin_boundaries_and_failure_prefix_are_distinct() {
        use crate::icf::settings::{Selection, extended::NumberInput};
        for (key, offset) in [(0x38d, 0x10), (0x38f, 0x12), (0x390, 0x0f), (0x39a, 0x16)] {
            let mut image = preset_fixture(0);
            image
                .patch_memory(&[(0xa60f + offset, &[127]), (0xa78c, &[0])])
                .unwrap();
            let before = image.memory().to_vec();
            for input in [
                NumberInput::Display,
                NumberInput::Cancel,
                NumberInput::Begin,
                NumberInput::Selected(Selection {
                    key: 1,
                    phase: 2,
                    candidate: 99,
                    accepted: true,
                }),
            ] {
                let out = edit_preset_number(&mut image, &[], 0, key, input, b'.', 0).unwrap();
                assert!(!out.editor.accepted && !out.editor.stored);
                assert_eq!(out.editor.candidate, 127);
                assert_eq!(
                    out.editor.state_out,
                    2 * u32::from(input == NumberInput::Begin)
                );
                assert_eq!(out.effects.writes, 0);
                assert_eq!(image.memory(), before);
            }
            if key != 0x39a {
                for text in [b"foo".as_slice(), b"", b"-", b"--"] {
                    let out = edit_preset_number(
                        &mut image,
                        &[],
                        0,
                        key,
                        NumberInput::Text(text),
                        b'.',
                        0,
                    )
                    .unwrap();
                    assert!(!out.editor.accepted && !out.editor.stored);
                    assert_eq!(out.editor.candidate, 127);
                    assert_eq!(image.memory(), before);
                }
                let out = edit_preset_number(
                    &mut image,
                    &[],
                    0,
                    key,
                    NumberInput::Text(b"128"),
                    b'.',
                    99,
                )
                .unwrap();
                assert_eq!(out.editor.suffix, b" (49%)");
                assert_eq!(image.memory()[0xa60f + offset], 128);
            }
        }
        let mut image = preset_fixture(0);
        for (key, old, input, value) in [
            (0x38d, 255, NumberInput::Next, 0),
            (0x38f, 0, NumberInput::Previous, 255),
            (0x390, 127, NumberInput::Text(b"127.5"), 128),
            (0x38d, 0, NumberInput::Text(b"999"), 255),
            (0x38d, 0, NumberInput::Text(b"-12"), 0),
            (0x39a, 223, NumberInput::Next, 2),
            (0x39a, 2, NumberInput::Previous, 223),
            (0x39a, 255, NumberInput::Text(b"foo"), 15),
        ] {
            let offset = match key {
                0x38d => 0x10,
                0x38f => 0x12,
                0x390 => 0x0f,
                _ => 0x16,
            };
            image.patch_memory(&[(0xa60f + offset, &[old])]).unwrap();
            let out = edit_preset_number(&mut image, &[], 0, key, input, b'.', 99).unwrap();
            assert!(out.editor.accepted);
            assert_eq!(image.memory()[0xa60f + offset], value);
        }
        let before = image.memory().to_vec();
        assert!(
            edit_preset_number(&mut image, &[], 5, 0x38d, NumberInput::Text(b"."), b'.', 99)
                .is_err()
        );
        assert!(
            edit_preset_number(&mut image, &[], 0, 0x389, NumberInput::Text(b"."), b'.', 99)
                .is_err()
        );
        assert_eq!(image.memory(), before);
        image
            .patch_memory(&[(0xa78c, &[0]), (0xa61f, &[127])])
            .unwrap();
        let mut expected = image.memory().to_vec();
        expected[0xa61f] = 0;
        let error = edit_preset_number(&mut image, &[], 0, 0x38d, NumberInput::Text(b"."), b'.', 0)
            .unwrap_err();
        assert!(matches!(
            error,
            Error::Invalid {
                reason: "special image window outside supplied image",
                ..
            }
        ));
        assert_eq!(image.memory(), expected);
    }

    #[test]
    fn preset_bandwidth_text_reuses_grid_guard_and_ordered_commit_prefix() {
        use crate::icf::settings::extended::NumberInput;
        let mut image = preset_fixture(0);
        image
            .patch_memory(&[(0xa6de, &[255]), (0xa78c, &[0])])
            .unwrap();
        let mut g = vec![0; 0x37867];
        g[0x3785f] = 0x11;
        g[0x37863] = 0x22;
        g[0xa78c] = 0x77;
        let mut expected = image.memory().to_vec();
        let out =
            edit_preset_bandwidth(&mut image, &g, 0, NumberInput::Text(b"550"), b'.', 0).unwrap();
        let editor = out.editor.unwrap();
        assert_eq!(editor.rewritten_text.as_deref(), Some(b"550".as_slice()));
        assert_eq!(
            (editor.number.candidate, editor.number.raw_candidate),
            (600, 10)
        );
        assert!(editor.number.accepted && editor.number.stored && editor.number.gray);
        assert_eq!(out.effects.callback_flags, 0x20);
        expected[0xa612..0xa615].copy_from_slice(&[10, 0x11, 0x22]);
        expected[0xa78c] = 0x77;
        assert_eq!(image.memory(), expected);
        for input in [
            NumberInput::Cancel,
            NumberInput::Text(b"foo"),
            NumberInput::Begin,
        ] {
            let out = edit_preset_bandwidth(&mut image, &[], 0, input, b'.', 0).unwrap();
            let editor = out.editor.unwrap();
            assert!(!editor.number.accepted && !editor.number.stored);
            assert_eq!(editor.popup_requested, input == NumberInput::Begin);
            assert_eq!(out.effects.writes, 0);
            assert_eq!(image.memory(), expected);
        }
        image.patch_memory(&[(0xa6dc, &[255])]).unwrap();
        let before = image.memory().to_vec();
        let blocked =
            edit_preset_bandwidth(&mut image, &[], 0, NumberInput::Text(b"."), b'.', 0).unwrap();
        assert!(blocked.editor.is_none() && blocked.effects.blocked);
        assert_eq!(image.memory(), before);
        let mut image = preset_fixture(0);
        let mut expected = image.memory().to_vec();
        expected[0xa612] = 10;
        assert!(
            edit_preset_bandwidth(&mut image, &[], 0, NumberInput::Text(b"550"), b'.', 99).is_err()
        );
        assert_eq!(image.memory(), expected);
    }

    #[test]
    fn shared_bandwidth_input_matches_existing_global_helper_without_storage_tail() {
        use crate::icf::settings::{
            Selection,
            extended::{NumberInput, edit_bandwidth},
        };
        for class in 0..=4 {
            for raw in [0, 9, 10, 255] {
                for input in [
                    NumberInput::Display,
                    NumberInput::Cancel,
                    NumberInput::Begin,
                    NumberInput::Next,
                    NumberInput::Previous,
                    NumberInput::Text(b"."),
                    NumberInput::Text(b"foo"),
                    NumberInput::Text(b"550.4"),
                    NumberInput::Selected(Selection {
                        key: 1,
                        phase: 2,
                        candidate: -7,
                        accepted: true,
                    }),
                ] {
                    let mut image = image();
                    image
                        .patch_memory(&[(0x37850 + 3 * class, &[raw])])
                        .unwrap();
                    let global = edit_bandwidth(&mut image, 3 * class, input, b'.').unwrap();
                    let shared = bandwidth_input(class as u32, raw, input, b'.').unwrap();
                    assert_eq!(
                        (
                            shared.number.accepted,
                            shared.number.candidate,
                            shared.number.raw_candidate
                        ),
                        (
                            global.number.accepted,
                            global.number.candidate,
                            global.number.raw_candidate
                        )
                    );
                    assert_eq!(shared.rewritten_text, global.rewritten_text);
                    assert_eq!(shared.popup_requested, global.popup_requested);
                    assert!(!shared.number.stored);
                }
            }
        }
        assert!(bandwidth_input(5, 0, NumberInput::Text(b"."), b'.').is_err());
        assert!(bandwidth_input(u32::MAX, 0, NumberInput::Text(b"."), b'.').is_err());
    }

    #[test]
    fn every_preset_key_side_and_index_preserves_all_unrelated_windows() {
        let g = vec![0; 0x4997a];
        for p in 0..5 {
            for field in PRESET_CATALOG {
                for side in 0..if matches!(field.key, 0x392 | 0x396) {
                    2
                } else {
                    1
                } {
                    let mut i = preset_fixture(p);
                    let a = 0xa60f + 41 * p;
                    put_byte(&mut i, a + 3, 10).unwrap(); // 600Hz makes both filter-type fields eligible.
                    let before = i.memory().to_vec();
                    let candidate = match field.key {
                        0x389 => 600,
                        0x38d | 0x38f | 0x390 => 127,
                        0x39a => 0x7c,
                        _ => 1,
                    };
                    let e =
                        commit_preset(&mut i, &g, p, field.key, side, Some(candidate), p).unwrap();
                    assert!(!e.blocked, "key {:X}", field.key);
                    for (at, (&old, &new)) in before.iter().zip(i.memory()).enumerate() {
                        if !(a..a + 25).contains(&at) {
                            assert_eq!(old, new, "key {:X}, at {at:X}", field.key);
                        }
                    }
                    let before = i.memory().to_vec();
                    let e = commit_preset(&mut i, &[], p, field.key, side, None, p).unwrap();
                    assert_eq!(e.writes, 0);
                    assert_eq!(i.memory(), before);
                }
            }
        }
        let mut h = Preset::read(preset_fixture(0).memory(), 0).unwrap();
        h.masks.fill(255);
        h.masks[0] = 1;
        assert!(!h.has_enabled_fields().unwrap());
        h.masks[1] = 0;
        assert!(h.has_enabled_fields().unwrap()); // Non-FF dependency != selected mask.
    }

    #[test]
    fn scan_reset_guard_raw_discriminators_and_notification_argument_order() {
        let mut source = vec![0; 0x3c9c0];
        source[0x37807] = 1;
        for discriminator in 0..32 {
            source[0x37806] = discriminator << 3;
            assert_eq!(
                scan_reset_enabled(&source, 0).unwrap(),
                matches!(discriminator, 1 | 5)
            );
        }
        source[0x37806] = 8;
        for (at, mask) in [(0x3a8ca, 1), (0x3a8c9, 1), (0x3c84a, 1), (0x3c84a, 2)] {
            source[at] |= mask;
            assert!(!scan_reset_enabled(&source, 0).unwrap());
            source[at] &= !mask;
        }
        assert!(scan_reset_enabled(&source, 0x100).unwrap());
        let mut i = image();
        let g = i.memory().to_vec();
        let mut state = RowState {
            state: 7,
            ..RowState::default()
        };
        let old_state = state.clone();
        let e = scan_rows(
            &mut i,
            &g,
            2..3,
            ScanRowAction::Clear,
            true,
            &mut state,
            false,
        )
        .unwrap();
        assert_eq!(
            e.notifications[0],
            Notification {
                event: 0x14,
                synchronous: false,
                first: 0,
                second: 2,
                payload: vec![]
            }
        );
        assert_eq!(state, old_state);
        assert_eq!(
            e.notifications
                .iter()
                .map(|n| (n.event, n.first, n.second))
                .collect::<Vec<_>>(),
            [(0x14, 0, 2), (0x11, 0, 0), (0x3c, 1, 0)]
        );
    }
}
