//! Explicit ANSI native-output boundaries, not Unicode/host-locale substitutes.
//! LCMapStringA's result is not a success/length gate: its whole 2n destination
//! is cleaned, compacted, then n bytes are copied back into the shared source.
//! The preceding 476E4 sanitation belongs to its caller and must already have
//! happened before map_copy_back; this function does not claim that whole editor.
//!
//! ```text
//! shared[n] -> src[2n] + dst[2n], both initially space-filled
//!           -> native write -> clean ALL dst[2n] -> copy first n to shared
//!           -> optional Upper on a SEPARATE n-byte copy
//! ```
//!
//! An unwritten mapping result therefore copies spaces, while an unwritten
//! Upper result retains its input copy. API return zero does not prove either
//! buffer is unwritten. Closure errors preserve earlier shared sanitation.

use super::{Error, clip_dbcs, is_lead, trim_fixed_spaces};

// csv.rs binds this file with #[path], so name the child from this directory.
#[cfg(windows)]
#[path = "nls/native.rs"]
mod native;

/// 476E4 walks logical bytes, not a C-prefix. Valid DBCS pairs are kept without
/// checking assigned CP932 codepoints. An invalid lead/trail pair replaces BOTH
/// bytes; isolated leads and invalid single bytes replace one. Return indicates
/// a replacement path, not a native full-EAX result or an accepted editor event.
pub fn sanitize(bytes: &mut [u8], replacement: u8) -> bool {
    let mut at = 0;
    let mut changed = false;
    while at < bytes.len() {
        let byte = bytes[at];
        if is_lead(byte) {
            if at + 1 == bytes.len() {
                bytes[at] = replacement;
                changed = true;
                at += 1;
            } else if matches!(bytes[at+1],0x40..=0x7e|0x80..=0xfc) {
                at += 2;
            } else {
                bytes[at] = replacement;
                bytes[at + 1] = replacement;
                changed = true;
                at += 2;
            }
        } else {
            if !matches!(byte,0x20..=0x7e|0xa1..=0xdf) {
                bytes[at] = replacement;
                changed = true;
            }
            at += 1;
        }
    }
    changed
}

/// Mode 1 whole-name grammar, not CSV B70E4: sanitize before trim/overwidth-only
/// clipping, so even an exact-width isolated lead is repaired here.
pub fn name_candidate(shared: &mut [u8], maximum: usize) -> Result<Vec<u8>, Error> {
    if shared.len() > 1024 * 1024 {
        return Err(Error::Limit("name source exceeds 1 MiB".into()));
    }
    sanitize(shared, b' ');
    let candidate = trim_candidate(shared);
    Ok(clip_dbcs(&candidate, maximum).0.to_vec())
}

/// LCID 0411/flags 600000 request with explicit 2n capacities. `native` can return
/// zero after partial writes; its result is retained but never gates copy-back.
/// An Err skips copy-back without undoing the caller's prior source sanitation.
pub fn map_copy_back(
    shared: &mut [u8],
    mut native: impl FnMut(&[u8], &mut [u8]) -> Result<i32, Error>,
) -> Result<Option<i32>, Error> {
    if shared.is_empty() {
        return Ok(None);
    }
    if shared.len() > 1024 * 1024 {
        return Err(Error::Limit("ANSI mapping source exceeds 1 MiB".into()));
    }
    let n = shared.len();
    let mut source = vec![b' '; 2 * n];
    let mut destination = vec![b' '; 2 * n];
    source[..n].copy_from_slice(shared);
    let result = native(&source, &mut destination)?;
    let mut at = 0;
    while at < destination.len() {
        if is_lead(destination[at]) {
            if at + 1 == destination.len() {
                return Err(Error::SourceUnavailable(
                    "ANSI destination trailing lead requires native boundary byte".into(),
                ));
            }
            destination[at] = 0;
            destination[at + 1] = 0;
            at += 2;
        } else {
            at += 1;
        }
    }
    compact_zero(&mut destination);
    shared.copy_from_slice(&destination[..n]);
    Ok(Some(result))
}
fn compact_zero(bytes: &mut [u8]) {
    let mut target = 0;
    for source in 0..bytes.len() {
        if bytes[source] != 0 {
            bytes[target] = bytes[source];
            target += 1;
        }
    }
    bytes[target..].fill(b' ');
}

/// Additional CharUpperBuffA uses a distinct n-byte copy. Native zero/unwritten
/// keeps this copy, unlike LCMap's zero/unwritten destination of spaces.
pub fn uppercase_copy(
    source: &[u8],
    mut native: impl FnMut(&mut [u8]) -> Result<u32, Error>,
) -> Result<Vec<u8>, Error> {
    if source.len() > 1024 * 1024 {
        return Err(Error::Limit("ANSI uppercase source exceeds 1 MiB".into()));
    }
    let mut destination = source.to_vec();
    let _ignored = native(&mut destination)?;
    Ok(destination)
}

/// D51E4 with an embedded NUL returns its exact prefix, including prefix-tail
/// spaces; it must not trim that prefix a second time. The unread private heap
/// suffix does not affect the returned prefix and is not invented or zeroed.
pub fn trim_candidate(source: &[u8]) -> Vec<u8> {
    if let Some(nul) = source.iter().position(|b| *b == 0) {
        source[..nul].to_vec()
    } else {
        trim_fixed_spaces(source).to_vec()
    }
}

/// BA9B0's observable low-byte changed flag. Caller consumes AL==1, not full EAX.
/// All eight logical slots are processed, including bytes after an embedded NUL.
pub fn fold_callsign(raw: &mut [u8; 8]) -> bool {
    let mut changed = false;
    for byte in raw {
        let next = if byte.is_ascii_lowercase() {
            byte.to_ascii_uppercase()
        } else if byte.is_ascii_uppercase() || byte.is_ascii_digit() || matches!(*byte, b'/' | b' ')
        {
            *byte
        } else {
            b' '
        };
        changed |= next != *byte;
        *byte = next;
    }
    changed
}

/// B812C's first flag uses n>8, not a snapshot comparison with pre-NLS bytes.
/// The native copy-back already changed the by-value argument's shared buffer.
pub fn callsign_fixed(
    shared: &mut [u8],
    native: impl FnMut(&[u8], &mut [u8]) -> Result<i32, Error>,
) -> Result<([u8; 8], bool), Error> {
    let clipped = shared.len() > 8;
    sanitize(shared, 0);
    compact_zero(shared);
    let _native_result = map_copy_back(shared, native)?;
    let mut raw = [b' '; 8];
    let n = shared.len().min(8);
    raw[..n].copy_from_slice(&shared[..n]);
    Ok((raw, clipped))
}

/// LCMap's portable ASCII domain only. High bytes still require actual NLS
/// output; they are not UTF-8 decoded or silently passed through as ASCII.
pub fn ascii_map(source: &[u8], destination: &mut [u8]) -> Result<i32, Error> {
    if !source.is_ascii() {
        return Err(Error::SourceUnavailable(
            "callsign NLS mapping for non-ASCII bytes".into(),
        ));
    }
    if destination.len() < source.len() {
        return Err(Error::Invalid("callsign mapping destination width".into()));
    }
    destination[..source.len()].copy_from_slice(source);
    i32::try_from(source.len()).map_err(|_| Error::Limit("callsign NLS result length".into()))
}

/// Default native producer on Windows, portable ASCII elsewhere. The Windows
/// request uses the caller's distinct, equal-sized 2n buffers with explicit counts,
/// not NUL termination. A native zero and partial destination writes are retained.
/// This does not change ascii_map's explicitly portable domain.
/// Windows rejects empty, odd, unequal or non-INT-sized buffers before FFI;
/// other platforms retain ascii_map's ASCII and destination-capacity checks.
pub fn platform_map(source: &[u8], destination: &mut [u8]) -> Result<i32, Error> {
    #[cfg(windows)]
    {
        native::map(source, destination)
    }
    #[cfg(not(windows))]
    {
        ascii_map(source, destination)
    }
}

/// Windows CharUpperBuffA on the separate n-byte copy; ASCII-only elsewhere.
/// The native count is observable, never a gate or a replacement output length.
pub fn platform_uppercase(bytes: &mut [u8]) -> Result<u32, Error> {
    #[cfg(windows)]
    {
        native::uppercase(bytes)
    }
    #[cfg(not(windows))]
    {
        if !bytes.is_ascii() {
            return Err(Error::SourceUnavailable(
                "non-ASCII CharUpperBuffA output required".into(),
            ));
        }
        let count = u32::try_from(bytes.len())
            .map_err(|_| Error::Limit("ANSI uppercase count exceeds DWORD".into()))?;
        bytes.make_ascii_uppercase();
        Ok(count)
    }
}

// Pure preflight is shared with boundary tests that never call Windows APIs.
#[cfg(any(windows, test))]
fn native_map_count(source_bytes: usize, destination_bytes: usize) -> Result<i32, Error> {
    if source_bytes == 0 || !source_bytes.is_multiple_of(2) || source_bytes != destination_bytes {
        return Err(Error::Invalid(
            "ANSI mapping requires distinct equal nonempty2n buffers".into(),
        ));
    }
    i32::try_from(source_bytes).map_err(|_| Error::Limit("ANSI mapping count exceeds INT".into()))
}

#[cfg(any(windows, test))]
fn native_uppercase_count(bytes: usize) -> Result<u32, Error> {
    u32::try_from(bytes).map_err(|_| Error::Limit("ANSI uppercase count exceeds DWORD".into()))
}

/// Captured descriptor flags relevant to 96308, not keyboard/acceptance gates.
#[derive(Debug, Clone, Copy)]
pub struct Profile<'a> {
    pub uppercase: bool,
    pub keep_kana: bool,
    pub keep_spaces: bool,
    pub punctuation_exceptions: &'a [u8],
    pub maximum: usize,
}

fn profile_bytes(bytes: &mut [u8], profile: Profile<'_>) {
    let replacement = if profile.keep_spaces { b' ' } else { 0 };
    if !profile.keep_kana {
        for byte in bytes.iter_mut() {
            if (0xa1..=0xdf).contains(byte) {
                *byte = replacement;
            }
        }
        if replacement == 0 {
            compact_zero(bytes);
        }
    }
    if !profile.keep_spaces {
        for byte in bytes.iter_mut() {
            if *byte == b' ' {
                *byte = 0;
            }
        }
        compact_zero(bytes);
    }
    for byte in bytes.iter_mut() {
        if byte.is_ascii_punctuation() && !profile.punctuation_exceptions.contains(byte) {
            *byte = replacement;
        }
    }
    if replacement == 0 {
        compact_zero(bytes);
    }
}

/// Mode 0's ordered normalizer. LCMap copy-back mutates the shared input before
/// any later failure. Additional Upper separates a new n-byte buffer; without
/// Upper, later byte filters still mutate the caller's shared allocation.
pub fn whole_text(
    shared: &mut [u8],
    profile: Profile<'_>,
    map: impl FnMut(&[u8], &mut [u8]) -> Result<i32, Error>,
    upper: impl FnMut(&mut [u8]) -> Result<u32, Error>,
) -> Result<Vec<u8>, Error> {
    if shared.len() > 1024 * 1024 {
        return Err(Error::Limit("whole text source exceeds 1 MiB".into()));
    }
    let replacement = if profile.keep_spaces { b' ' } else { 0 };
    sanitize(shared, replacement);
    if replacement == 0 {
        compact_zero(shared);
    }
    // 47B20/47BE8 has its own zero-fill sanitation before native mapping.
    sanitize(shared, 0);
    compact_zero(shared);
    let _result = map_copy_back(shared, map)?;
    let candidate = if profile.uppercase {
        let mut own = uppercase_copy(shared, upper)?;
        profile_bytes(&mut own, profile);
        trim_candidate(&own)
    } else {
        profile_bytes(shared, profile);
        trim_candidate(shared)
    };
    Ok(clip_dbcs(&candidate, profile.maximum).0.to_vec())
}

/// 84708's final fold is C + bool(C + name_changed), with DWORD wrapping.
pub fn callsign_correction_count(c: u32, name_changed: bool) -> u32 {
    c.wrapping_add(u32::from(c.wrapping_add(u32::from(name_changed)) != 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_request_counts_reject_invalid_bounds_without_large_allocations() {
        assert_eq!(native_map_count(2, 2).unwrap(), 2);
        assert_eq!(
            native_map_count(2 * 1024 * 1024, 2 * 1024 * 1024).unwrap(),
            2 * 1024 * 1024
        );
        for (source, destination) in [(0, 0), (1, 1), (2, 1), (2, 3)] {
            assert!(matches!(
                native_map_count(source, destination),
                Err(Error::Invalid(_))
            ));
        }
        let too_long = i32::MAX as usize + 1;
        assert!(matches!(
            native_map_count(too_long, too_long),
            Err(Error::Limit(_))
        ));
        assert_eq!(native_uppercase_count(0).unwrap(), 0);
        assert_eq!(native_uppercase_count(u32::MAX as usize).unwrap(), u32::MAX);
        if let Some(too_long) = (u32::MAX as usize).checked_add(1) {
            assert!(matches!(
                native_uppercase_count(too_long),
                Err(Error::Limit(_))
            ));
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn platform_nls_nonwindows_keeps_ascii_and_fail_closed_source_boundaries() {
        let mut destination = [b'!'; 4];
        assert_eq!(platform_map(b"ab  ", &mut destination).unwrap(), 4);
        assert_eq!(destination, *b"ab  ");
        let before = destination;
        assert!(matches!(
            platform_map(b"\xa1 ", &mut destination),
            Err(Error::SourceUnavailable(_))
        ));
        assert_eq!(destination, before);
        assert!(platform_map(b"ab", &mut [0; 1]).is_err());
        let mut text = *b"a\0bz";
        assert_eq!(platform_uppercase(&mut text).unwrap(), 4);
        assert_eq!(text, *b"A\0BZ");
        let mut foreign = [b'a', 0xa1];
        assert!(matches!(
            platform_uppercase(&mut foreign),
            Err(Error::SourceUnavailable(_))
        ));
        assert_eq!(foreign, [b'a', 0xa1]);
    }

    #[test]
    fn map_result_zero_partial_write_and_alias_count_have_distinct_meanings() {
        let mut source = b"abcd".to_vec();
        let result = map_copy_back(&mut source, |src, dst| {
            assert_eq!(src, b"abcd    ");
            dst.copy_from_slice(b"A\0BCDE  ");
            Ok(0)
        })
        .unwrap();
        assert_eq!(result, Some(0));
        assert_eq!(source, b"ABCD");
        let _ = map_copy_back(&mut source, |_, _| Ok(0)).unwrap();
        assert_eq!(source, b"    ");
        let mut shared = b"ja1abc".to_vec();
        let (raw, first_changed) = callsign_fixed(&mut shared, |_, dst| {
            dst[..6].copy_from_slice(b"JA1ABC");
            Ok(6)
        })
        .unwrap();
        assert_eq!(shared, b"JA1ABC");
        assert_eq!(raw, *b"JA1ABC  ");
        assert!(!first_changed);
        let original = shared.clone();
        assert!(
            map_copy_back(&mut shared, |_, _| Err(Error::Operation(
                "injected native failure".into()
            )))
            .is_err()
        );
        assert_eq!(shared, original);
    }
    #[test]
    fn lead_cleanup_uppercase_new_buffer_and_nul_prefix_are_not_common_trim() {
        let mut shared = b"ABCD".to_vec();
        let _ = map_copy_back(&mut shared, |_, dst| {
            dst.copy_from_slice(b"\x81ZAB\0CDE");
            Ok(0)
        })
        .unwrap();
        assert_eq!(shared, b"ABCD");
        let upper = uppercase_copy(&shared, |dst| {
            dst.copy_from_slice(b"A \0D");
            Ok(0)
        })
        .unwrap();
        assert_eq!(shared, b"ABCD");
        assert_eq!(trim_candidate(&upper), b"A ");
        assert_eq!(trim_candidate(b"A   "), b"A");
        let mut call = *b"a\0b\xff/9Z ";
        assert!(fold_callsign(&mut call));
        assert_eq!(call, *b"A B /9Z ");
        assert_eq!(callsign_correction_count(2, false), 3);
        assert_eq!(callsign_correction_count(u32::MAX, true), u32::MAX);
    }

    #[test]
    fn mode1_name_repairs_exact_width_lead_but_csv_fixed_text_does_not() {
        let mut input = *b"A\x81";
        assert_eq!(name_candidate(&mut input, 2).unwrap(), b"A");
        assert_eq!(input, *b"A ");
        assert_eq!(super::super::fixed_text::<2>(b"A\x81"), (*b"A\x81", false));
        let mut raw = *b"A\x82\0TAIL\x80";
        assert!(sanitize(&mut raw, b' '));
        assert_eq!(raw, *b"A  TAIL ");
        let mut raw = [0x81, 0x40, 0x81, 0x7f, 0xa1, 0xf0];
        sanitize(&mut raw, b' ');
        assert_eq!(raw, [0x81, 0x40, b' ', b' ', 0xa1, b' ']);
    }
    #[test]
    fn descriptor_filters_and_native_failure_preserve_shared_update_boundaries() {
        let profile = Profile {
            uppercase: true,
            keep_kana: false,
            keep_spaces: true,
            punctuation_exceptions: b"/",
            maximum: 8,
        };
        let mut shared = *b"ab!c";
        let candidate = whole_text(
            &mut shared,
            profile,
            |source, dst| {
                dst.copy_from_slice(source);
                Ok(0)
            },
            |dst| {
                dst.copy_from_slice(b"AB!C");
                Ok(0)
            },
        )
        .unwrap();
        assert_eq!(candidate, b"AB C");
        assert_eq!(shared, *b"ab!c");
        let mut shared = *b"A\0B";
        assert!(
            whole_text(
                &mut shared,
                profile,
                |_, _| Err(Error::Operation("injected map failure".into())),
                |_| panic!()
            )
            .is_err()
        );
        assert_eq!(shared, *b"A B");
        let mut shared = *b"ab\\ c";
        let profile = Profile {
            uppercase: false,
            keep_kana: false,
            keep_spaces: false,
            punctuation_exceptions: b"/",
            maximum: 16,
        };
        assert_eq!(
            whole_text(
                &mut shared,
                profile,
                |source, dst| {
                    dst.copy_from_slice(source);
                    Ok(0)
                },
                |_| panic!()
            )
            .unwrap(),
            b"abc"
        );
        assert_eq!(shared, *b"abc  ");
    }
}
