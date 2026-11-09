//! Ordered CSV alias tables (B5F30/B60B8), kept as caller-supplied resource data.
//! A label->value map would destroy first-match collisions such as RPS3/RPS4.
//! Table bytes are not Unicode text and are never copied from factory payloads.

use super::{Error, trim};

/// One source table entry. Duplicate labels and duplicate values are meaningful.
#[derive(Debug, Clone, Copy)]
pub struct Entry<'a> {
    pub label: &'a [u8],
    pub value: u32,
}

/// Normalize with actual CharLowerBuffA-equivalent output, in original order.
pub fn import_with(
    source: &[u8],
    entries: &[Entry<'_>],
    mut lower: impl FnMut(&[u8]) -> Result<Vec<u8>, Error>,
) -> Result<Option<u32>, Error> {
    let source = lower(trim(source))?;
    for entry in entries {
        if lower(entry.label)? == source {
            return Ok(Some(entry.value));
        }
    }
    Ok(None)
}
/// Explicit ASCII mapping for known ASCII resource contexts, not all NLS.
pub fn import_ascii(source: &[u8], entries: &[Entry<'_>]) -> Option<u32> {
    entries
        .iter()
        .find(|entry| trim(source).eq_ignore_ascii_case(entry.label))
        .map(|entry| entry.value)
}
/// B60B8 returns the first matching value's label; missing output is an explicit
/// resource input, not an invented empty/default success.
pub fn export<'a>(value: u32, entries: &[Entry<'a>], not_found: &'a [u8]) -> &'a [u8] {
    entries
        .iter()
        .find(|entry| entry.value == value)
        .map_or(not_found, |entry| entry.label)
}
/// B6AB4 searches the secondary mode table only on primary miss. Only a secondary
/// success increments the correction counter; canonical alias changes do not.
pub fn mode_ascii(
    source: &[u8],
    primary: &[Entry<'_>],
    secondary: &[Entry<'_>],
    corrections: &mut u32,
) -> Option<u32> {
    if let Some(value) = import_ascii(source, primary) {
        return Some(value);
    }
    let value = import_ascii(source, secondary)?;
    *corrections = corrections.wrapping_add(1);
    Some(value)
}
/// B7BD8 adds exactly one leading zero to trimmed strings of length <=2. It does
/// not pad to width3 or accept arbitrary octal codes absent from the source table.
pub fn dtcs_ascii(source: &[u8], entries: &[Entry<'_>]) -> Option<u32> {
    let source = trim(source);
    if source.len() <= 2 {
        let mut text = vec![b'0'];
        text.extend_from_slice(source);
        import_ascii(&text, entries)
    } else {
        import_ascii(source, entries)
    }
}
/// B6D31 fallback is a four-byte DATA prefix; B7355 fallback requires full ON.
pub fn data_ascii(source: &[u8], entries: &[Entry<'_>], corrections: &mut u32) -> Option<u32> {
    if let Some(value) = import_ascii(source, entries) {
        return Some(value);
    }
    if trim(source)
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"data"))
    {
        *corrections = corrections.wrapping_add(1);
        Some(1)
    } else {
        None
    }
}
pub fn on_ascii(source: &[u8], entries: &[Entry<'_>], corrections: &mut u32) -> Option<u32> {
    if let Some(value) = import_ascii(source, entries) {
        return Some(value);
    }
    if trim(source).eq_ignore_ascii_case(b"on") {
        *corrections = corrections.wrapping_add(1);
        Some(1)
    } else {
        None
    }
}

/// ASCII Group/Bank normal table computed from its public ordinal schema.
/// Missing header is represented by None by the caller, not by this function.
pub fn gps_group_ascii(source: &[u8]) -> u32 {
    let source = trim(source);
    if source.eq_ignore_ascii_case(b"off") {
        0
    } else if source.len() == 1 && source[0].is_ascii_alphabetic() {
        u32::from(source[0].to_ascii_uppercase() - b'A' + 1)
    } else {
        0xffff
    }
}
pub fn gps_group_text(group: u32) -> Result<Vec<u8>, Error> {
    match group {
        0 => Ok(b"OFF".to_vec()),
        1..=26 => Ok(vec![b'A' + group as u8 - 1]),
        _ => Err(Error::SourceUnavailable(
            "GPS group NOT_FOUND resource required",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_collision_secondary_count_and_non_numeric_short_codes() {
        let dup = [
            Entry {
                label: b"RPS",
                value: 3,
            },
            Entry {
                label: b"RPS",
                value: 4,
            },
        ];
        assert_eq!(export(4, &dup, b"missing"), b"RPS");
        assert_eq!(import_ascii(b" rps ", &dup), Some(3));
        let primary = [
            Entry {
                label: b"CANON",
                value: 1,
            },
            Entry {
                label: b"alias",
                value: 1,
            },
        ];
        let secondary = [Entry {
            label: b"old",
            value: 1,
        }];
        let mut c = u32::MAX;
        assert_eq!(mode_ascii(b"alias", &primary, &secondary, &mut c), Some(1));
        assert_eq!(c, u32::MAX);
        assert_eq!(mode_ascii(b"OLD", &primary, &secondary, &mut c), Some(1));
        assert_eq!(c, 0);
        let dtcs = [Entry {
            label: b"023",
            value: 0,
        }];
        assert_eq!(dtcs_ascii(b"23", &dtcs), Some(0));
        assert_eq!(dtcs_ascii(b"3", &dtcs), None);
        assert_eq!(dtcs_ascii(b"000", &dtcs), None);
        assert_eq!(data_ascii(b"database", &[], &mut c), Some(1));
        assert_eq!(on_ascii(b"Onward", &[], &mut c), None);
        assert_eq!(gps_group_ascii(b"01"), 0xffff);
        assert_eq!(gps_group_ascii(b"z"), 26);
    }
}
