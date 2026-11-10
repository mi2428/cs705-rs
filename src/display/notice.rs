//! The two shared help notices are references to real parent field consumers.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeParent {
    Agc,
    Filter,
    Ts,
    ToneControl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeCell {
    pub topic: u32,
    pub key: u32,
    pub index: i32,
    pub text: Vec<u8>,
    pub foreground: Option<u32>,
    pub ascii_event_disabled: bool,
    pub editable: bool,
}

fn c_string(pe: &[u8], rva: u32) -> io::Result<&[u8]> {
    let tail =
        crate::csv::history::pe_tail(pe, rva).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    let end = tail
        .iter()
        .take(4097)
        .position(|b| *b == 0)
        .ok_or_else(|| invalid("notice source string unavailable"))?;
    Ok(&tail[..end])
}

/// Actual common-group25, literal parent indices. A caller cannot turn an
/// arbitrary editable field into FIX merely by supplying one of these topics.
/// The PE is needed ONLY for reached compiled Filter BW/PBW captions.
pub fn notice_cell(
    pe: Option<&[u8]>,
    parent: NoticeParent,
    key: u32,
    index: i32,
    canvas: bool,
) -> io::Result<Option<NoticeCell>> {
    use NoticeParent::*;
    let fixed_agc = matches!(index, 4..=6);
    let fixed_filter = matches!(index, 36..=38 | 42 | 45..=47);
    let (topic, text, gray, disable) = match (parent, key) {
        (Agc, 0x337) if fixed_agc => (56459, b"FAST".to_vec(), false, false),
        (Agc, 0x33c) if fixed_agc => (56459, b"0.1sec".to_vec(), false, false),
        (Agc, 0x33d | 0x33e) if fixed_agc => (56460, b"---".to_vec(), false, false),
        (Filter, 0x12f | 0x130) if fixed_filter => {
            let pe = pe.ok_or_else(|| invalid("fixed Filter caption needs original PE asset"))?;
            let at = 0x3f3a2c + index as u32 * 4;
            let scalar = crate::csv::history::pe_tail(pe, at)
                .map_err(|e| Error::new(ErrorKind::InvalidData, e))?
                .get(..4)
                .ok_or_else(|| invalid("fixed Filter bandwidth scalar unavailable"))?;
            let value = i32::from_le_bytes(scalar.try_into().unwrap()).to_string();
            let format = c_string(pe, 0x3f90d0)?;
            let at = format
                .windows(2)
                .position(|s| s == b"%d")
                .ok_or_else(|| invalid("fixed Filter caption format unavailable"))?;
            let mut text = format[..at].to_vec();
            text.extend_from_slice(value.as_bytes());
            text.extend_from_slice(&format[at + 2..]);
            (56459, text, false, false)
        }
        (Filter, 0x132) => (56459, b"0".to_vec(), false, false),
        (Filter, 0x131) => (56460, b"---".to_vec(), false, false),
        (Filter, 0x32f) | (ToneControl, 0x343) => (56460, b"---".to_vec(), canvas, false),
        (Ts, 0x342) if matches!(index, 0..=2 | 4..=6) => (56460, b"---".to_vec(), canvas, true),
        _ => return Ok(None),
    };
    Ok(Some(NoticeCell {
        topic,
        key,
        index,
        text,
        foreground: gray.then_some(0x8080),
        ascii_event_disabled: disable,
        editable: false,
    }))
}
