//! Native Hz paint profiles, not CSV output or a record-validity gate.

use super::{Error, frequency_text};

/// A522C trim0 profiles currently used by native callers: scale=digits=0/1/3/6.
/// IDIV truncates toward zero; the two native32 abs operations wrap at MIN.
/// An optional sign prefix precedes the integer field, outside its space width.
/// force_plus enables both signs, but zero has neither sign. signed_bounds
/// enables only the negative prefix. No locale, unit, gate or clamp is applied.
pub fn scalar(
    value: i32,
    scale: u32,
    fractional_digits: u32,
    integer_width: usize,
    signed_bounds: bool,
    force_plus: bool,
) -> Result<Vec<u8>, Error> {
    if scale != fractional_digits || !matches!(scale, 0 | 1 | 3 | 6) {
        return Err(Error::Invalid(
            "native scalar requires scale=digits=0/1/3/6",
        ));
    }
    let divisor = 10_i32.pow(scale);
    let integer = (value / divisor).wrapping_abs();
    let fraction = (value % divisor).wrapping_abs();
    let mut out = Vec::new();
    if (signed_bounds || force_plus) && value < 0 {
        out.push(b'-');
    } else if force_plus && value > 0 {
        out.push(b'+');
    }
    out.extend_from_slice(format!("{integer:>integer_width$}").as_bytes());
    if fractional_digits != 0 {
        out.push(b'.');
        out.extend_from_slice(
            format!("{fraction:0width$}", width = fractional_digits as usize).as_bytes(),
        );
    }
    Ok(out)
}

fn normal(hz: u32, width: usize, signed_bounds: bool) -> Vec<u8> {
    scalar(hz as i32, 6, 6, width, signed_bounds, false).expect("fixed native Hz scalar profile")
}

/// A5134/A5184 -> A522C: scale6, fractional digits6, integer width4, trim0.
/// Normal/call frequency+offset, compact frequency, scan/user and Rpt frequency.
/// Padding spaces and all six fractional digits remain logical output bytes.
pub fn channel(hz: u32) -> Vec<u8> {
    normal(hz, 4, false)
}

/// Compact type4/5 offset's FD60 MAX/MAX bounds scale back to signed -1.
/// Unlike normal/call offset bounds, this enables A522C's negative sign prefix.
pub fn compact_offset(hz: u32) -> Vec<u8> {
    normal(hz, 4, true)
}

/// Rpt offset alone uses integer width3, still fractional digits6 and trim0.
pub fn repeater_offset(hz: u32) -> Vec<u8> {
    normal(hz, 3, false)
}

/// FromDV/FM frequency+offset and Rx15C: unsigned u32, fixed6, no width/trim.
/// The normal-CW finite proof recovers exact div/rem after native scaling/RTL.
/// live_decimal is F4140's one RTL read; zero emits no separator, not a NUL.
/// D571C rereads the copied decimal's first byte at EACH logical output byte.
/// The callback receives that zero-based read index and may fail on unavailable
/// state. Digit/high-byte collisions are replaced too. Return only after every
/// replacement; the caller owns its preceding empty assign and final ctx assign.
pub fn history(
    hz: u32,
    live_decimal: u8,
    mut copied_decimal: impl FnMut(usize) -> Result<u8, Error>,
) -> Result<Vec<u8>, Error> {
    let mut out = frequency_text(hz, live_decimal);
    if live_decimal == 0 {
        out.remove(out.len() - 7);
    }
    for (index, byte) in out.iter_mut().enumerate() {
        if *byte == copied_decimal(index)? {
            *byte = b'.';
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finite_hz_profiles_keep_padding_signed_split_and_live_copy_bytes() {
        for (hz, normal, history_text) in [
            (0, b"   0.000000".as_slice(), b"0.000000".as_slice()),
            (1, b"   0.000001", b"0.000001"),
            (145_500_000, b" 145.500000", b"145.500000"),
            (0x8000_0000, b"2147.483648", b"2147.483648"),
            (0x8000_0001, b"2147.483647", b"2147.483649"),
            (u32::MAX, b"   0.000001", b"4294.967295"),
        ] {
            assert_eq!(channel(hz), normal);
            assert_eq!(history(hz, b',', |_| Ok(b',')).unwrap(), history_text);
        }
        assert_eq!(repeater_offset(0), b"  0.000000");
        assert_eq!(repeater_offset(9_999_900), b"  9.999900");
        assert_eq!(compact_offset(9_999_900), b"   9.999900");
        assert_eq!(compact_offset(u32::MAX), b"-   0.000001");
        assert_eq!(compact_offset(0x8000_0000), b"-2147.483648");
        assert_eq!(channel(145_000_000), b" 145.000000");
        assert_eq!(history(1_000_001, b',', |_| Ok(b'.')).unwrap(), b"1,000001");
        assert_eq!(history(1_000_001, b',', |_| Ok(b'1')).unwrap(), b".,00000.");
        assert_eq!(history(1, b'.', |_| Ok(b'0')).unwrap(), b".......1");
        assert_eq!(history(1_000_001, 0, |_| Ok(0)).unwrap(), b"1000001");
        assert_eq!(history(0, 0, |_| Ok(0)).unwrap(), b"0000000");
        assert_eq!(history(1_000_001, 0, |_| Ok(b'1')).unwrap(), b".00000.");
        assert_eq!(history(1, 0xdf, |_| Ok(b'.')).unwrap(), b"0\xdf000001");
        assert_eq!(history(1, 0xdf, |_| Ok(0xdf)).unwrap(), b"0.000001");
        let mut reads = Vec::new();
        let text = history(1_000_001, b',', |index| {
            reads.push(index);
            Ok(if index == 0 { b'1' } else { b'x' })
        })
        .unwrap();
        assert_eq!(text, b".,000001");
        assert_eq!(reads, (0..8).collect::<Vec<_>>());
        let mut reads = 0;
        assert!(
            history(1, b'.', |_| {
                reads += 1;
                if reads == 3 {
                    Err(Error::SourceUnavailable("copied decimal read unavailable"))
                } else {
                    Ok(b'.')
                }
            })
            .is_err()
        );
        assert_eq!(reads, 3);
        for (scale, zero, negative_tick, minimum) in [
            (
                0,
                b"0".as_slice(),
                b"-1".as_slice(),
                b"--2147483648".as_slice(),
            ),
            (1, b"0.0", b"-0.1", b"-214748364.8"),
            (3, b"0.000", b"-0.001", b"-2147483.648"),
            (6, b"0.000000", b"-0.000001", b"-2147.483648"),
        ] {
            assert_eq!(scalar(0, scale, scale, 1, true, true).unwrap(), zero);
            assert_eq!(
                scalar(-1, scale, scale, 1, false, true).unwrap(),
                negative_tick
            );
            assert_eq!(
                scalar(i32::MIN, scale, scale, 1, true, false).unwrap(),
                minimum
            );
        }
        assert_eq!(scalar(12345, 3, 3, 1, true, true).unwrap(), b"+12.345");
        assert_eq!(scalar(255, 1, 1, 1, false, false).unwrap(), b"25.5");
        assert_eq!(scalar(255, 1, 1, 1, true, true).unwrap(), b"+25.5");
        assert_eq!(scalar(-1, 1, 1, 3, true, false).unwrap(), b"-  0.1");
        assert_eq!(scalar(1, 1, 1, 3, false, true).unwrap(), b"+  0.1");
        assert_eq!(scalar(-1, 1, 1, 1, false, false).unwrap(), b"0.1");
        assert_eq!(scalar(1, 1, 1, 1, true, false).unwrap(), b"0.1");
        assert_eq!(
            scalar(i32::MIN, 0, 0, 1, false, false).unwrap(),
            b"-2147483648"
        );
        assert_eq!(
            scalar(i32::MAX, 0, 0, 1, false, true).unwrap(),
            b"+2147483647"
        );
        assert!(scalar(0, 2, 2, 1, false, false).is_err());
        assert!(scalar(0, 3, 1, 1, false, false).is_err());
        assert!(scalar(0, u32::MAX, u32::MAX, 1, false, false).is_err());
    }
}
