//! Headless C1BEC/BE8BC callsign dialogs. Inputs are source images and gestures,
//! never completed candidate packets. Saving and notification remain external.

use super::{
    Error, MemoryCallsign,
    frequency::Ranges,
    record::{self, ChannelField as C, DecodedRecord, RepeaterField as R, RepeaterText as T},
    record_image,
};
use crate::repeater::{groups::Groups, working};

mod dr;
pub use dr::{Action, Dialog, DialogOutcome, apply_requests, prepare_metadata};

pub struct Sources<'a> {
    pub current: &'a [u8],
    pub defaults: &'a [u8],
    pub ranges: &'a Ranges,
    pub system_langid: Option<u16>,
    pub language: u32,
}

/// Native uninitialized/read-past-string inputs, needed only if actually read.
/// None never silently supplies spaces, a row sentinel or a successful result.
#[derive(Default)]
pub struct ReadInputs<'a> {
    pub ur_name_tail: Option<[u8; 8]>,
    pub blank_source_index: Option<u32>,
    pub initial_first_marker: Option<i32>,
    pub string_tail: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListSelection {
    pub category: i32,
    pub name: i32,
    pub call: i32,
    pub modal_result: u32,
    pub checkbox: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    pub row: u32,
    pub text: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub tag: u32,
    pub text: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListModel {
    pub categories: Vec<Category>,
    pub names: Vec<ListItem>,
    pub calls: Vec<Vec<u8>>,
    pub tag: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListOutcome {
    pub modal_result: u32,
    pub save: bool,
    pub copied: bool,
    pub checkbox: bool,
    pub mode: u32,
}

fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 0, reason }
}
fn window(bytes: &[u8], at: usize, len: usize) -> Result<&[u8], Error> {
    bytes
        .get(
            at..at
                .checked_add(len)
                .ok_or(invalid("dialog offset overflow"))?,
        )
        .ok_or(invalid("dialog source window unavailable"))
}
fn word(h: &DecodedRecord, at: usize) -> u32 {
    u32::from_le_bytes(h.as_bytes()[at..at + 4].try_into().unwrap())
}
fn put(h: &mut DecodedRecord, at: usize, value: u32) {
    h.as_mut_bytes()[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn blank(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| *b == b' ')
}
fn prefix(bytes: &[u8]) -> &[u8] {
    bytes.split(|b| *b == 0).next().unwrap()
}
fn padded<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut out = [b' '; N];
    let n = bytes.len().min(N);
    out[..n].copy_from_slice(&bytes[..n]);
    out
}
fn trim_right(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().rposition(|b| *b > b' ').map_or(0, |i| i + 1)]
}
fn fixed_string<const N: usize>(text: &[u8], tail: &[u8]) -> Result<[u8; N], Error> {
    if text.len() >= N {
        return Ok(text[..N].try_into().unwrap());
    }
    let mut span = text.to_vec();
    span.push(0);
    span.extend_from_slice(tail);
    span.get(..N)
        .ok_or(invalid("native selected string tail unavailable"))?
        .try_into()
        .map_err(|_| invalid("native selected string width"))
}
fn c_normalized(bytes: &[u8], reads: &ReadInputs<'_>) -> Result<[u8; 16], Error> {
    if bytes.contains(&0) {
        return Ok(padded(prefix(bytes)));
    }
    if !reads.string_tail.contains(&0) {
        return Err(invalid("native strlen tail NUL unavailable"));
    }
    // strlen reads the tail even when the eventual copy is limited to sixteen.
    Ok(padded(bytes))
}
fn repeater(s: &Sources<'_>, row: u32) -> Result<DecodedRecord, Error> {
    let mut h = record::reset_holder(0x0c, row, 0);
    working::load_into(s.current, row as usize, &mut h)?;
    Ok(h)
}
fn ur(s: &Sources<'_>, row: u32) -> Result<DecodedRecord, Error> {
    let mut h = record::reset_holder(0x0d, row, 0);
    record_image::load_callsign_into(s.current, row, &mut h)?;
    Ok(h)
}
fn total(s: &Sources<'_>) -> Result<u32, Error> {
    Ok(Groups::from_working(s.current)?.total())
}
fn ur_count(s: &Sources<'_>) -> Result<u32, Error> {
    record_image::callsign_count(s.current, 0x0d)
}
fn derived_mode(mode: u32) -> u32 {
    if mode == 0x11 { 3 } else { 2 }
}

// These are the two actual UI resource languages, not factory image data.
fn internet(language: u32) -> &'static [u8] {
    if language == 1 {
        b"\x83C\x83\x93\x83^\x81[\x83l\x83b\x83g\x90\xda"
    } else {
        b"Internet"
    }
}
fn rpt_token(language: u32, gateway: bool) -> &'static [u8] {
    match (language == 1, gateway) {
        (true, false) => b"\x8eR\x8a|\x82\xaf",
        (true, true) => b"\x83Q\x81[\x83g\x83E\x83F\x83C",
        (false, false) => b"Same Repeater",
        (false, true) => b"Gateway",
    }
}
const CQ: [u8; 8] = *b"CQCQCQ  ";

/// BE054 preserves first full-sixteen-byte name match, including NULs.
pub fn lookup_ur_name(s: &Sources<'_>, query: &[u8; 16]) -> Result<u32, Error> {
    for row in 0..ur_count(s)? {
        if ur(s, row)?.name() == query {
            return Ok(row);
        }
    }
    Ok(300)
}
fn lookup_ur_call(s: &Sources<'_>, query: &[u8; 8]) -> Result<u32, Error> {
    if blank(query) {
        return Ok(300);
    }
    for row in 0..ur_count(s)? {
        if ur(s, row)?.callsign(MemoryCallsign::Your) == query {
            return Ok(row);
        }
    }
    Ok(300)
}
fn lookup_ur(s: &Sources<'_>, query: &[u8], reads: &ReadInputs<'_>) -> Result<u32, Error> {
    let name = if query.len() == 8 {
        let mut n = [0; 16];
        n[..8].copy_from_slice(query);
        n[8..].copy_from_slice(
            &reads
                .ur_name_tail
                .ok_or(invalid("BD53C incoming U8 unavailable"))?,
        );
        n
    } else {
        query
            .get(..16)
            .ok_or(invalid("UR query sixteen-byte span unavailable"))?
            .try_into()
            .unwrap()
    };
    let row = lookup_ur_name(s, &name)?;
    if row < 300 {
        return Ok(row);
    }
    lookup_ur_call(s, query[..8].try_into().unwrap())
}
pub fn lookup_repeater_name(s: &Sources<'_>, query: &[u8; 16], mode: u32) -> Result<u32, Error> {
    if !matches!(mode, 2 | 3) {
        return Ok(0xffff);
    }
    for row in 0..total(s)? {
        let h = repeater(s, row)?;
        if h.repeater_field(R::Mode) == mode && h.name() == query {
            return Ok(row);
        }
    }
    Ok(0xffff)
}
fn module_alias(a: u8, b: u8) -> bool {
    a == b || (matches!(a, b' ' | b'A') && matches!(b, b' ' | b'A'))
}

/// BD6C4. Selector truncates to a byte; mode does not. Gateway preference and
/// optional-prefix ABI are distinct from slash and ordinary raw-body searches.
pub fn lookup_repeater(
    s: &Sources<'_>,
    query: &[u8],
    optional_prefix: Option<&[u8; 8]>,
    selector: u32,
    mode: u32,
) -> Result<u32, Error> {
    if query.len() < 8 || !blank(&query[8..]) || !matches!(mode, 2 | 3) {
        return Ok(0xffff);
    }
    let selector = selector as u8;
    let end = total(s)? as u16;
    let mut fallback = 0xffff;
    for row in 0..end {
        let row = u32::from(row);
        if selector == 2 {
            if query[7] != b'G' {
                return Ok(0xffff);
            }
            let h = repeater(s, row)?;
            let call = h.repeater_text(T::Call);
            let gateway = h.repeater_text(T::Gateway);
            if h.repeater_field(R::Mode) != mode
                || blank(gateway)
                || (mode == 2 && blank(call))
                || (mode == 3 && h.channel_field(C::Duplex) != 3)
                || gateway != &query[..8]
            {
                continue;
            }
            if let Some(p) = optional_prefix {
                if p[..7] == call[..7] {
                    return Ok(row);
                }
            } else if call[..7] == query[..7] {
                if matches!(call[7], b' ' | b'A') {
                    return Ok(row);
                }
                if fallback == 0xffff {
                    fallback = row;
                }
            }
            continue;
        }
        let call = if selector == 1 && mode == 2 {
            let h = repeater(s, row)?;
            if h.repeater_field(R::Mode) != 2 || blank(h.repeater_text(T::Call)) {
                continue;
            }
            *h.repeater_text(T::Call)
        } else {
            let at = 0xaa0f + 52 * row as usize;
            let raw = window(s.current, at, 52)?;
            if (mode == 2 && raw[10] >> 6 != 0) || (mode == 3 && raw[10] >> 4 != 15) {
                continue;
            }
            let call = super::memory::unpack_callsign(&raw[11..18]);
            if mode == 2 && blank(&call) {
                continue;
            }
            call
        };
        if selector == 1 {
            if mode == 3 && query[0] != b'/' {
                return Ok(0xffff);
            }
            if query[1..7] == call[..6] && module_alias(query[7], call[7]) {
                return Ok(row);
            }
        } else if query[..7] == call[..7] && module_alias(query[7], call[7]) {
            return Ok(row);
        }
    }
    Ok(fallback)
}
fn lookup_call(s: &Sources<'_>, query: &[u8], mode: u32) -> Result<u32, Error> {
    if query.len() < 8 || blank(&query[..8]) {
        return Ok(0xffff);
    }
    lookup_repeater(s, query, None, if query[7] == b'G' { 2 } else { 0 }, mode)
}
pub fn lookup_rpt2(
    s: &Sources<'_>,
    prefix: &[u8; 8],
    query: &[u8],
    flag: u32,
    mode: u32,
) -> Result<u32, Error> {
    let optional = if query.get(7) == Some(&b'G') && !blank(prefix) && flag as u8 == 1 {
        Some(prefix)
    } else {
        None
    };
    if query.len() < 8 || blank(&query[..8]) {
        return Ok(0xffff);
    }
    lookup_repeater(
        s,
        query,
        optional,
        if query[7] == b'G' { 2 } else { 0 },
        mode,
    )
}
fn lookup_slash(s: &Sources<'_>, query: &[u8], mode: u32) -> Result<u32, Error> {
    if query.first() != Some(&b'/') {
        return Ok(0xffff);
    }
    lookup_repeater(s, query, None, 1, mode)
}
fn named_or_call(s: &Sources<'_>, query: &[u8; 16], mode: u32) -> Result<u32, Error> {
    let row = lookup_repeater_name(s, query, mode)?;
    if row < 2500 {
        Ok(row)
    } else {
        lookup_repeater(s, query, None, 0, mode)
    }
}

/// BB068 / BB0EC use Delphi one-based Delete/Insert, then a fixed eight read.
pub fn slash_call(call: &[u8; 8], reads: &ReadInputs<'_>) -> Result<[u8; 8], Error> {
    if blank(call) {
        return Ok(*call);
    }
    let mut text = prefix(call).to_vec();
    if text.len() > 6 {
        text.remove(6);
    }
    text.insert(0, b'/');
    fixed_string(&text, reads.string_tail)
}
fn unslash_call(call: &[u8; 8], reads: &ReadInputs<'_>) -> Result<[u8; 8], Error> {
    let mut text = prefix(call).to_vec();
    if !text.is_empty() {
        text.remove(0);
    }
    let pos = text.len().min(6);
    text.insert(pos, b' ');
    fixed_string(&text, reads.string_tail)
}

fn category_tags(list_mode: u32, mode: u32) -> Vec<u32> {
    let mut tags = if matches!(list_mode, 0 | 3) {
        vec![0, 1]
    } else {
        vec![2]
    };
    tags.extend(3..54);
    if list_mode == 3 && mode == 3 {
        tags.push(54);
    }
    tags
}
fn category_text(s: &Sources<'_>, tag: u32) -> Result<Vec<u8>, Error> {
    Ok(match tag {
        0 => b"CQCQCQ".to_vec(),
        1 => {
            if s.language == 1 {
                b"\x91\x8a\x8e\xe8\x8b\xc7\x83R\x81[\x83\x8b\x83T\x83C\x83\x93".to_vec()
            } else {
                b"Your Call Sign".to_vec()
            }
        }
        2 => {
            if s.language == 1 {
                b"\x8eg\x97p\x82\xb5\x82\xc8\x82\xa2".to_vec()
            } else {
                b"Not Used".to_vec()
            }
        }
        3 => {
            if s.language == 1 {
                b"\x83\x8c\x83s\x81[\x83^\x83\x8a\x83X\x83g".to_vec()
            } else {
                b"Repeater List".to_vec()
            }
        }
        4..=53 => {
            let mut caption = format!("{:02}: ", tag - 3).into_bytes();
            caption.extend_from_slice(prefix(window(
                s.current,
                0x2ea3b + 16 * (tag - 4) as usize,
                16,
            )?));
            caption
        }
        54 => {
            if s.language == 1 {
                b"\x83C\x83\x93\x83^\x81[\x83l\x83b\x83g\x90\xda\x91\xb1".to_vec()
            } else {
                b"Internet".to_vec()
            }
        }
        _ => return Err(invalid("unregistered callsign category")),
    })
}
fn eligible(h: &DecodedRecord, mode: u32) -> bool {
    match mode {
        2 => h.repeater_field(R::Mode) == 2 && !blank(h.repeater_text(T::Call)),
        3 => h.repeater_field(R::Mode) == 3 && h.channel_field(C::Duplex) == 3,
        _ => false,
    }
}
fn row_label(h: &DecodedRecord) -> Vec<u8> {
    if blank(h.name()) {
        prefix(h.repeater_text(T::Call)).to_vec()
    } else {
        prefix(h.name()).to_vec()
    }
}
fn selected<Ty>(items: &[Ty], index: i32) -> Result<&Ty, Error> {
    usize::try_from(index)
        .ok()
        .and_then(|i| items.get(i))
        .ok_or(invalid("native list selected index unavailable"))
}

/// Build all three real lists at the requested display selection. No name/call
/// bytes supplied by a client can replace these row producers.
pub fn list_model(
    s: &Sources<'_>,
    list_mode: u32,
    mode: u32,
    selection: ListSelection,
    reads: &ReadInputs<'_>,
) -> Result<ListModel, Error> {
    if list_mode >= 3 {
        return Err(invalid("upper list requires the dialog source context"));
    }
    list_model_with_source(s, list_mode, mode, &[b' '; 16], selection, reads)
}
fn list_model_with_source(
    s: &Sources<'_>,
    list_mode: u32,
    mode: u32,
    source: &[u8; 16],
    selection: ListSelection,
    reads: &ReadInputs<'_>,
) -> Result<ListModel, Error> {
    if list_mode > 4 {
        return Err(invalid("callsign list mode outside 0..4"));
    }
    let categories: Vec<_> = category_tags(list_mode, mode)
        .into_iter()
        .map(|tag| {
            Ok(Category {
                tag,
                text: category_text(s, tag)?,
            })
        })
        .collect::<Result<_, Error>>()?;
    let tag = if selection.category < 0 {
        categories[0].tag
    } else {
        selected(&categories, selection.category)?.tag
    };
    let mut names = Vec::new();
    if tag == 1 {
        for row in 0..ur_count(s)? {
            let h = ur(s, row)?;
            let name = prefix(h.name());
            names.push(ListItem {
                row,
                text: if trim_right(name).is_empty() {
                    prefix(h.callsign(MemoryCallsign::Your)).to_vec()
                } else {
                    name.to_vec()
                },
            });
        }
    } else if (3..54).contains(&tag) {
        let groups = Groups::from_working(s.current)?;
        let (start, end) = if tag == 3 {
            (0, groups.total())
        } else {
            let g = (tag - 4) as usize;
            let start = groups.start(g)?;
            (start, start + u32::from(groups.counts[g]))
        };
        for row in start..end {
            let h = repeater(s, row)?;
            let include = if list_mode == 4 {
                matches!(dr::source_class(s, row, 1)?, 2 | 3)
                    && matches!(h.repeater_field(R::Mode), 2 | 3)
            } else if list_mode == 3 {
                let q = if blank(h.name()) {
                    padded(h.repeater_text(T::Call))
                } else {
                    *h.name()
                };
                let mut value = q;
                let mut source_class = 0;
                let target_class =
                    dr::classify(s, source, &mut value, mode, 1, reads, &mut source_class)?;
                dr::preview_allowed(source_class, target_class) && eligible(&h, mode)
            } else {
                eligible(&h, mode)
            };
            if include {
                names.push(ListItem {
                    row,
                    text: row_label(&h),
                });
            }
        }
    }
    let mut calls = Vec::new();
    if !names.is_empty() && selection.name >= 0 {
        let row = selected(&names, selection.name)?.row;
        if tag == 1 {
            calls.push(prefix(ur(s, row)?.callsign(MemoryCallsign::Your)).to_vec());
        } else {
            let h = repeater(s, row)?;
            let call = *h.repeater_text(T::Call);
            calls.push(
                prefix(&if list_mode == 0 {
                    slash_call(&call, reads)?
                } else {
                    call
                })
                .to_vec(),
            );
            let gateway = h.repeater_text(T::Gateway);
            if matches!(list_mode, 1 | 2) && gateway[7] == b'G' && gateway[..7] == call[..7] {
                calls.push(prefix(gateway).to_vec());
            }
        }
    }
    Ok(ListModel {
        categories,
        names,
        calls,
        tag,
    })
}

fn list_value<const N: usize>(
    model: &ListModel,
    selection: ListSelection,
    reads: &ReadInputs<'_>,
    language: u32,
) -> Result<Option<[u8; N]>, Error> {
    Ok(match model.tag {
        0 => Some(padded(b"CQCQCQ")),
        2 => Some([b' '; N]),
        54 => Some(padded(internet(language))),
        1 | 3..=53 => {
            let text = if N == 8 {
                selected(&model.calls, selection.call)?
            } else {
                &selected(&model.names, selection.name)?.text
            };
            Some(fixed_string(text, reads.string_tail)?)
        }
        _ => None,
    })
}

/// D1's full-DWORD return gate is deliberately not an AL or ==1 test.
pub fn list_into(
    sources: &Sources<'_>,
    holder: &mut DecodedRecord,
    field: MemoryCallsign,
    selection: ListSelection,
    reads: &ReadInputs<'_>,
) -> Result<ListOutcome, Error> {
    let list_mode = match field {
        MemoryCallsign::Your => 0,
        MemoryCallsign::Repeater1 => 1,
        MemoryCallsign::Repeater2 => 2,
    };
    let mode = derived_mode(holder.mode());
    let model = list_model(sources, list_mode, mode, selection, reads)?;
    let mut copied = false;
    if selection.modal_result != 0 {
        if let Some(value) = list_value::<8>(&model, selection, reads, sources.language)? {
            holder.set_callsign(field, &value);
            copied = true;
        }
    }
    Ok(ListOutcome {
        modal_result: selection.modal_result,
        save: selection.modal_result != 0,
        copied,
        checkbox: selection.checkbox,
        mode,
    })
}

/// C2F20's requested initial projection, separate from a native setter return.
pub fn initial_selection(
    s: &Sources<'_>,
    holder: &DecodedRecord,
    list_mode: u32,
    query: &[u8],
    other_mode: u32,
    reads: &ReadInputs<'_>,
) -> Result<ListSelection, Error> {
    let mode = derived_mode(holder.mode());
    if list_mode >= 3 {
        return Err(invalid(
            "upper list initial projection requires dialog context",
        ));
    }
    initial_selection_with_source(
        s,
        holder,
        list_mode,
        mode,
        query,
        other_mode,
        &[b' '; 16],
        reads,
    )
}
fn initial_selection_with_source(
    s: &Sources<'_>,
    holder: &DecodedRecord,
    list_mode: u32,
    mode: u32,
    query: &[u8],
    other_mode: u32,
    source: &[u8; 16],
    reads: &ReadInputs<'_>,
) -> Result<ListSelection, Error> {
    let tags = category_tags(list_mode, mode);
    let mut tag = tags[0];
    let (mut name, mut call) = (0, 0);
    if !blank(query) {
        let mut row = 0xffff;
        if query.len() == 16 && mode == 3 && query == padded::<16>(internet(s.language)) {
            tag = 54;
        } else {
            if list_mode >= 3 {
                row = lookup_repeater_name(
                    s,
                    query
                        .try_into()
                        .map_err(|_| invalid("upper list seed requires sixteen bytes"))?,
                    mode,
                )?;
            }
            if row >= 2500 {
                row = match list_mode {
                    0 => lookup_slash(s, query, mode)?,
                    2 => lookup_rpt2(
                        s,
                        holder.callsign(MemoryCallsign::Repeater1),
                        query,
                        0,
                        mode,
                    )?,
                    _ => lookup_call(s, query, mode)?,
                };
            }
            if row >= 2500 && list_mode == 3 {
                row = lookup_slash(s, query, mode)?;
            }
            if row < 2500 {
                if list_mode == 3 {
                    let mut value = padded(query);
                    let mut source_class = 0;
                    let target =
                        dr::classify(s, source, &mut value, mode, 1, reads, &mut source_class)?;
                    if !dr::preview_allowed(source_class, target) {
                        return Ok(ListSelection {
                            category: 0,
                            name: 0,
                            call: 0,
                            modal_result: 0,
                            checkbox: false,
                        });
                    }
                }
                let groups = Groups::from_working(s.current)?;
                let g = groups.group_of(row as i32);
                tag = g as u32 + 4;
                let start = groups.start(g)?;
                name = 0;
                for prior in start..row {
                    let h = repeater(s, prior)?;
                    let include = if list_mode == 4 {
                        matches!(dr::source_class(s, prior, 1)?, 2 | 3)
                    } else if list_mode == 3 {
                        let already_eligible = eligible(&h, derived_mode(other_mode));
                        let mut value = if blank(h.name()) {
                            padded(h.repeater_text(T::Call))
                        } else {
                            *h.name()
                        };
                        let mut source_class = 0;
                        let target =
                            dr::classify(s, source, &mut value, mode, 1, reads, &mut source_class)?;
                        already_eligible || dr::preview_allowed(source_class, target)
                    } else {
                        eligible(&h, mode)
                    };
                    if include {
                        name += 1;
                    }
                }
                let h = repeater(s, row)?;
                if matches!(list_mode, 1 | 2)
                    && query[..8] == *h.repeater_text(T::Gateway)
                    && query[7] == b'G'
                {
                    call = 1;
                }
            } else if matches!(list_mode, 0 | 3) {
                let row = lookup_ur(s, query, reads)?;
                if row < 300 {
                    tag = 1;
                    name = row as i32;
                }
            }
        }
    }
    Ok(ListSelection {
        category: tags.iter().position(|x| *x == tag).map_or(-1, |i| i as i32),
        name,
        call,
        modal_result: 0,
        checkbox: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dr_group49_scalar_first_dd_ur_list_cancel_and_index9_failure() {
        let mut image = vec![0; 0x4997a];
        image[0x49914..0x49916].copy_from_slice(&1_u16.to_le_bytes());
        image[0x49976..0x49978].copy_from_slice(&2_u16.to_le_bytes());
        for (row, name, call, gateway) in [
            (0, b"REMOTE          ", b"JA2DEF A", b"JA2DEF G"),
            (1, b"NEIGHBOR        ", b"JA1ABC A", b"JA1ABC G"),
            (2, b"SOURCE          ", b"JA1ABC B", b"JA1ABC G"),
        ] {
            let mut h = record::reset_holder(0x0c, row, 0);
            h.set_name(name);
            h.set_repeater_field(R::Mode, 3);
            h.set_repeater_text(T::Call, call);
            h.set_repeater_text(T::Gateway, gateway);
            h.set_channel_field(C::FrequencyHz, 145_000_000 + 100_000 * row);
            h.set_channel_field(C::Duplex, 3);
            h.set_offset_hz(700_000);
            working::save_into(&mut image, row as usize, &h).unwrap();
        }
        let ranges = Ranges::generate(
            &super::super::context::Context::before_registry(),
            Some(0x411),
        )
        .unwrap();
        let sources = Sources {
            current: &image,
            defaults: &image,
            ranges: &ranges,
            system_langid: Some(0x411),
            language: 0,
        };
        let reads = ReadInputs {
            ur_name_tail: Some([b' '; 8]),
            blank_source_index: Some(2500),
            initial_first_marker: Some(0),
            string_tail: &[0; 16],
        };
        let mut h = record::reset_holder(2, 17, 0x9876);
        h.set_valid(true);
        h.set_mode(0x11);
        h.set_offset_hz(123_400);
        let header = h.as_bytes()[..12].to_vec();
        let mut d = Dialog::open(&sources, &mut h, &reads).unwrap();
        let after_open = h.clone();
        assert_eq!(d.mode, 4);
        // Source mode4 categories start with Not Used, then all/group0..49.
        let source_selection = ListSelection {
            category: 51,
            name: 1,
            call: 0,
            modal_result: 1,
            checkbox: true,
        };
        let model = d
            .list_model(&sources, true, source_selection, &reads)
            .unwrap();
        assert_eq!(model.tag, 53);
        assert_eq!(
            model.names.iter().map(|n| n.row).collect::<Vec<_>>(),
            [1, 2]
        );
        d.action(&sources, Action::SourceList(source_selection), &reads)
            .unwrap();
        assert_eq!((d.mode, d.source_class, d.your_class), (3, 3, 5));
        assert_eq!(d.source, *b"SOURCE          ");
        assert_eq!(d.holder.channel_field(C::FrequencyHz), 145_200_000);
        assert_eq!(d.holder.offset_hz(), 123_400);
        assert_eq!(d.holder.mode(), 0x11);
        let ur_selection = ListSelection {
            category: 3,
            name: 0,
            call: 0,
            modal_result: 0x100,
            checkbox: false,
        };
        let model = d.list_model(&sources, false, ur_selection, &reads).unwrap();
        assert_eq!(model.tag, 4);
        assert_eq!(model.names[0].row, 0);
        d.action(&sources, Action::UrList(ur_selection), &reads)
            .unwrap();
        assert_eq!((d.source_class, d.your_class), (3, 4));
        assert_eq!(
            d.preview,
            [
                b"/JA2DEFA".to_vec(),
                b"JA1ABC B".to_vec(),
                b"JA1ABC G".to_vec()
            ]
        );
        let preview = d.holder.clone();
        d.action(
            &sources,
            Action::UrList(ListSelection {
                modal_result: 0,
                checkbox: true,
                ..ur_selection
            }),
            &reads,
        )
        .unwrap();
        assert!(d.holder == preview);
        assert!(d.checkbox);
        // Only outer result1 adopts the clone. Metadata written by open remains.
        let outcome = d.finish(&mut h, 2);
        assert!(!outcome.accepted && outcome.save);
        assert!(h == after_open);
        assert_eq!(&h.as_bytes()[..12], header);

        // A NUL at source call+8 makes native AnsiString index9 unavailable.
        // Scalar source fields have already changed; all calls remain untouched.
        let mut remote = record::reset_holder(0x0c, 0, 0);
        working::load_into(&image, 0, &mut remote).unwrap();
        remote.set_repeater_text(T::Gateway, &[0; 8]);
        working::save_into(&mut image, 0, &remote).unwrap();
        let sources = Sources {
            current: &image,
            defaults: &image,
            ranges: &ranges,
            system_langid: Some(0x411),
            language: 0,
        };
        let old_calls = [
            *h.callsign(MemoryCallsign::Your),
            *h.callsign(MemoryCallsign::Repeater1),
            *h.callsign(MemoryCallsign::Repeater2),
        ];
        let error = apply_requests(&sources, &mut h, &preview, 2, 0, 2500, &[b' '; 16], &reads)
            .unwrap_err();
        assert!(matches!(
            error,
            Error::Invalid {
                reason: "Area CQ native AnsiString index9 unavailable",
                ..
            }
        ));
        assert_eq!(h.channel_field(C::FrequencyHz), 145_200_000);
        assert_eq!(h.mode(), 0x11);
        assert_eq!(h.offset_hz(), 123_400);
        assert_eq!(
            [
                *h.callsign(MemoryCallsign::Your),
                *h.callsign(MemoryCallsign::Repeater1),
                *h.callsign(MemoryCallsign::Repeater2)
            ],
            old_calls
        );
        assert_eq!(&h.as_bytes()[..12], header);
    }

    #[test]
    fn modal_sources_generate_lists_scalar_first_preview_and_cancel_prefix() {
        let mut image = vec![0; 0x4997a];
        image[0x49914..0x49916].copy_from_slice(&4_u16.to_le_bytes());
        image[0x49978..0x4997a].copy_from_slice(&1_u16.to_le_bytes());
        for (row, name, call, gateway) in [
            (0, b"SOURCE B        ", b"JA1ABC B", b"JA1ABC G"),
            (1, b"SOURCE A        ", b"JA1ABC A", b"JA1ABC G"),
            (2, b"REMOTE A        ", b"JA2DEF A", b"JA2DEF G"),
        ] {
            let mut h = record::reset_holder(0x0c, row, 0);
            h.set_name(name);
            h.set_repeater_field(R::Mode, 2);
            h.set_repeater_text(T::Call, call);
            h.set_repeater_text(T::Gateway, gateway);
            h.set_channel_field(C::FrequencyHz, 145_000_000 + 100_000 * row);
            h.set_channel_field(C::Duplex, 1);
            h.set_offset_hz(600_000);
            working::save_into(&mut image, row as usize, &h).unwrap();
        }
        let mut dd = record::reset_holder(0x0c, 3, 0);
        dd.set_name(b"DD SOURCE       ");
        dd.set_repeater_field(R::Mode, 3);
        dd.set_repeater_text(T::Call, b"JA3GHI A");
        dd.set_repeater_text(T::Gateway, b"JA3GHI G");
        dd.set_channel_field(C::FrequencyHz, 145_300_000);
        dd.set_channel_field(C::Duplex, 3);
        dd.set_offset_hz(700_000);
        working::save_into(&mut image, 3, &dd).unwrap();
        image[0x2f703..0x2f713].copy_from_slice(b"YOUR NAME       ");
        image[0x2eda3..0x2edab].copy_from_slice(b"YOURCALL");
        let ranges = Ranges::generate(
            &super::super::context::Context::before_registry(),
            Some(0x411),
        )
        .unwrap();
        let sources = Sources {
            current: &image,
            defaults: &image,
            ranges: &ranges,
            system_langid: Some(0x411),
            language: 0,
        };
        let reads = ReadInputs {
            ur_name_tail: Some([b' '; 8]),
            blank_source_index: Some(2500),
            initial_first_marker: Some(0),
            string_tail: &[0; 16],
        };
        assert_eq!(
            lookup_repeater(&sources, b"JA1ABC G", None, 2, 2).unwrap(),
            1
        );
        assert_eq!(
            lookup_repeater(&sources, b"JA1ABC  ", None, 0, 2).unwrap(),
            1
        );
        assert_eq!(
            lookup_repeater(&sources, b"/JA1ABCA", None, 1, 2).unwrap(),
            1
        );
        assert_eq!(
            lookup_repeater(&sources, b"JA1ABC A", None, 0, 0x102).unwrap(),
            0xffff
        );
        assert_eq!(
            lookup_rpt2(&sources, b"JA1ABC A", b"JA1ABC G", 1, 2).unwrap(),
            0
        );
        assert_eq!(
            lookup_rpt2(&sources, b"JA1ABC A", b"JA1ABC G", 0x100, 2).unwrap(),
            1
        );
        let name_tail = ReadInputs {
            ur_name_tail: Some(*b"E       "),
            ..reads
        };
        assert_eq!(lookup_ur(&sources, b"YOUR NAM", &name_tail).unwrap(), 0);
        assert_eq!(lookup_ur(&sources, b"YOUR NAM", &reads).unwrap(), 300);
        assert_eq!(fixed_string::<8>(b"AB", b"12345").unwrap(), *b"AB\x0012345");

        let mut h = record::reset_holder(2, 7, 0x1234);
        h.set_mode(0x10);
        let select = ListSelection {
            category: 3,
            name: 2,
            call: 0,
            modal_result: 0x100,
            checkbox: true,
        };
        let outcome = list_into(&sources, &mut h, MemoryCallsign::Your, select, &reads).unwrap();
        assert!(outcome.save && outcome.copied);
        assert_eq!(outcome.modal_result, 0x100);
        assert_eq!(h.callsign(MemoryCallsign::Your), b"/JA2DEFA");
        let before = h.clone();
        let cancel = ListSelection {
            modal_result: 0,
            ..select
        };
        assert!(
            !list_into(&sources, &mut h, MemoryCallsign::Your, cancel, &reads)
                .unwrap()
                .save
        );
        assert!(h == before);
        let your_list = ListSelection {
            category: 1,
            name: 0,
            call: 0,
            modal_result: 1,
            checkbox: false,
        };
        list_into(&sources, &mut h, MemoryCallsign::Your, your_list, &reads).unwrap();
        assert_eq!(h.callsign(MemoryCallsign::Your), b"YOURCALL");
        let rpt_list = ListSelection {
            category: 1,
            name: 1,
            call: 1,
            modal_result: 1,
            checkbox: false,
        };
        list_into(
            &sources,
            &mut h,
            MemoryCallsign::Repeater2,
            rpt_list,
            &reads,
        )
        .unwrap();
        assert_eq!(h.callsign(MemoryCallsign::Repeater2), b"JA1ABC G");
        let initial = initial_selection(&sources, &h, 1, b"JA1ABC G", 0x10, &reads).unwrap();
        assert_eq!((initial.category, initial.name, initial.call), (2, 1, 1));

        h.set_valid(false);
        h.as_mut_bytes()[0x230] = 0xad;
        let header = h.as_bytes()[..12].to_vec();
        let mut other = record::reset_holder(2, 0, 0);
        other.set_mode(0x10);
        other.set_filter(3);
        apply_requests(
            &sources,
            &mut h,
            &other,
            1,
            2501,
            2500,
            &padded(b"CQCQCQ"),
            &reads,
        )
        .unwrap();
        assert_eq!(&h.as_bytes()[..12], header);
        assert!(h.valid());
        assert_eq!(h.as_bytes()[0x230], 0);
        assert_eq!(h.channel_field(C::FrequencyHz), 145_100_000);
        assert_eq!(h.channel_field(C::Duplex), 1);
        assert_eq!(h.mode(), 0x10);
        assert_eq!(h.offset_hz(), 600_000);
        assert_eq!(h.filter(), 2);
        assert_eq!(h.callsign(MemoryCallsign::Repeater1), b"JA1ABC A");
        let mut gateway_label = h.clone();
        gateway_label.set_callsign(MemoryCallsign::Repeater1, b"JA1ABC G");
        prepare_metadata(&sources, &mut gateway_label).unwrap();
        assert_eq!(&gateway_label.as_bytes()[0xac..0xbc], b"SOURCE A        ");
        assert_eq!(gateway_label.as_bytes()[0xbc], b'G');
        assert_eq!(
            gateway_label.callsign(MemoryCallsign::Repeater1),
            b"JA1ABC G"
        );
        let mut dd_target = h.clone();
        dd_target.set_offset_hz(123_400);
        apply_requests(
            &sources,
            &mut dd_target,
            &other,
            3,
            2501,
            2500,
            &padded(b"CQCQCQ"),
            &reads,
        )
        .unwrap();
        assert_eq!(dd_target.mode(), 0x11);
        assert_eq!(dd_target.channel_field(C::Duplex), 3);
        assert_eq!(dd_target.offset_hz(), 123_400);
        assert_eq!(dd_target.filter(), 0);
        let dd_model = list_model(
            &sources,
            1,
            3,
            ListSelection {
                name: 0,
                ..rpt_list
            },
            &reads,
        )
        .unwrap();
        assert_eq!(dd_model.names.len(), 1);
        let mut scalar_prefix = h.clone();
        let failed = apply_requests(
            &sources,
            &mut scalar_prefix,
            &other,
            2,
            2502,
            0xffff,
            &padded(b"YOURCALL"),
            &reads,
        );
        assert!(failed.is_err());
        assert_eq!(scalar_prefix.channel_field(C::FrequencyHz), 145_200_000);

        let original = h.clone();
        let dialog = Dialog::open(&sources, &mut h, &reads).unwrap();
        assert!(h != original);
        assert_eq!(dialog.preview[0], b"CQCQCQ  ");
        let after_open = h.clone();
        assert!(dialog.finish(&mut h, 0).save);
        assert!(h == after_open);

        let mut empty = record::reset_holder(2, 9, 0x5678);
        empty.set_valid(false);
        let no_marker = ReadInputs {
            initial_first_marker: None,
            ..reads
        };
        let mut boundary = empty.clone();
        let mut boundary_dialog = Dialog::open(&sources, &mut boundary, &no_marker).unwrap();
        assert!(
            boundary_dialog
                .action(
                    &sources,
                    Action::SourceList(ListSelection {
                        category: 1,
                        name: 1,
                        call: 0,
                        modal_result: 1,
                        checkbox: false
                    }),
                    &no_marker
                )
                .is_err()
        );
        assert_eq!(boundary_dialog.source, *b"SOURCE A        ");
        let mut dialog = Dialog::open(&sources, &mut empty, &reads).unwrap();
        assert_eq!(dialog.mode, 4);
        dialog
            .action(
                &sources,
                Action::SourceList(ListSelection {
                    category: 1,
                    name: 1,
                    call: 0,
                    modal_result: 1,
                    checkbox: true,
                }),
                &reads,
            )
            .unwrap();
        assert_eq!(dialog.mode, 2);
        assert_eq!(dialog.holder.channel_field(C::FrequencyHz), 145_100_000);
        assert_eq!(
            dialog.holder.callsign(MemoryCallsign::Repeater1),
            b"JA1ABC A"
        );
        dialog
            .action(
                &sources,
                Action::UrList(ListSelection {
                    category: 2,
                    name: 2,
                    call: 0,
                    modal_result: 1,
                    checkbox: false,
                }),
                &reads,
            )
            .unwrap();
        assert_eq!(dialog.holder.callsign(MemoryCallsign::Your), b"/JA2DEFA");
        assert_eq!(dialog.preview[0], b"/JA2DEFA");
        assert!(dialog.finish(&mut empty, 1).accepted);
        assert_eq!(empty.binding(), 0x5678);
        assert_eq!(empty.callsign(MemoryCallsign::Your), b"/JA2DEFA");
    }
}
