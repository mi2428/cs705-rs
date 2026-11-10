//! A02AC normal paint (edit flag0), not the fixed-B0 dialog seed or CSV text.

use super::{Error, native_decimal_fraction, native_dms};

/// Shared48B28 degree byte. Only the non-Japanese-GUI branch reads SYSTEM LANGID.
pub fn degree(gui_language: Option<u32>, system_langid: Option<u16>) -> Result<u8, Error> {
    let japanese = gui_language.ok_or(Error::SourceUnavailable(
        "native coordinate GUI language C00",
    ))? == 1;
    Ok(
        if japanese
            || system_langid.ok_or(Error::SourceUnavailable("native coordinate SYSTEM LANGID"))?
                & 0x3ff
                == 0x11
        {
            0xdf
        } else {
            0xb0
        },
    )
}

/// Modes5..10 choose latitude/longitude by parity but reread live396B4 for U.
/// Modes11..13 carry the actual area mode and do not read current/U again.
/// C00==1 skips SYSTEM LANGID; otherwise48B28 needs its actual low10 bits.
/// The unused caller argument is omitted. Logical NUL/high hemisphere bytes
/// remain bytes; a later47CF0/C-prefix sink belongs to the display caller.
pub fn format(
    raw: [u8; 4],
    mode: u8,
    current: &[u8],
    gui_language: Option<u32>,
    system_langid: Option<u16>,
) -> Result<Vec<u8>, Error> {
    let degree = degree(gui_language, system_langid)?;
    let area = match mode {
        5..=10 => false,
        11..=13 => true,
        _ => return Err(Error::Invalid("native coordinate paint mode outside5..13")),
    };
    let unit = if area {
        match mode {
            11 => 1,
            12 => 0,
            _ => 2,
        }
    } else {
        *current.get(0x396b4).ok_or(Error::SourceUnavailable(
            "native coordinate current DMSType byte unavailable",
        ))?
    };
    if unit > 2 {
        return Ok(Vec::new());
    }
    let mut out = if area {
        Vec::new()
    } else if raw[0] == 0xff {
        if mode.is_multiple_of(2) {
            b"---".to_vec()
        } else {
            b"--".to_vec()
        }
    } else {
        format!(
            "{:0width$}",
            raw[0],
            width = if mode.is_multiple_of(2) { 3 } else { 2 }
        )
        .into_bytes()
    };
    let q = u32::from(u16::from_be_bytes([raw[1], raw[2]]));
    if unit == 2 {
        if area {
            out.push(b'0');
        }
        out.push(b'.');
        if !area && raw[0] == 0xff {
            out.extend_from_slice(b"----");
        } else {
            let fraction = native_decimal_fraction(q);
            out.extend_from_slice(
                format!("{:04}", if area { fraction.max(14) } else { fraction }).as_bytes(),
            );
        }
        out.push(degree);
    } else {
        if !area {
            out.push(degree);
        }
        let separator = if unit == 1 { b'\'' } else { b'.' };
        let suffix = if unit == 1 { b'"' } else { b'\'' };
        if !area && raw[0] == 0xff {
            out.extend_from_slice(&[b'-', b'-', separator, b'-', b'-', suffix]);
        } else {
            let (first, second) = if unit == 1 {
                native_dms(q)
            } else {
                (q / 1000, q % 1000 / 10)
            };
            out.extend_from_slice(format!("{first:02}").as_bytes());
            out.push(separator);
            out.extend_from_slice(format!("{second:02}").as_bytes());
            out.push(suffix);
        }
    }
    if !area {
        out.push(if raw[0] == 0xff { b'-' } else { raw[3] });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_modes_use_live_units_glyphs_and_scratch_without_edit_seed() {
        assert_eq!(degree(Some(1), None).unwrap(), 0xdf);
        assert_eq!(degree(Some(1), Some(0x409)).unwrap(), 0xdf);
        assert_eq!(degree(Some(0), Some(0x411)).unwrap(), 0xdf);
        assert_eq!(degree(Some(2), Some(0x811)).unwrap(), 0xdf);
        assert_eq!(degree(Some(0), Some(0x409)).unwrap(), 0xb0);
        assert!(degree(Some(0), None).is_err());
        assert!(degree(None, Some(0x411)).is_err());
        let mut current = vec![0; 0x396b5];
        let q = |d, q: u16, h| {
            let [hi, lo] = q.to_be_bytes();
            [d, hi, lo, h]
        };
        for mode in [5, 7, 9] {
            assert_eq!(
                format(q(3, 12345, b'N'), mode, &current, Some(0), Some(0x409)).unwrap(),
                b"03\xb012.34'N"
            );
        }
        for mode in [6, 8, 10] {
            assert_eq!(
                format(q(3, 12345, 0xff), mode, &current, Some(1), None).unwrap(),
                b"003\xdf12.34'\xff"
            );
        }
        assert_eq!(
            format([255; 4], 5, &current, Some(0), Some(0x411)).unwrap(),
            b"--\xdf--.--'-"
        );
        current[0x396b4] = 1;
        assert_eq!(
            format(q(254, 59999, 0), 5, &current, Some(1), None).unwrap(),
            b"254\xdf59'59\"\0"
        );
        assert_eq!(
            format(q(3, 60000, b'S'), 7, &current, Some(0), Some(0x409)).unwrap(),
            b"03\xb060'00\"S"
        );
        assert_eq!(
            format(q(3, 65535, b'N'), 9, &current, Some(1), None).unwrap(),
            b"03\xdf65'32\"N"
        );
        assert_eq!(
            format([255; 4], 6, &current, Some(1), None).unwrap(),
            b"---\xdf--'--\"-"
        );
        current[0x396b4] = 2;
        assert_eq!(
            format(q(3, 1, b'W'), 10, &current, Some(0), Some(0x409)).unwrap(),
            b"003.0000\xb0W"
        );
        assert_eq!(
            format(q(3, 59997, b'N'), 5, &current, Some(1), None).unwrap(),
            b"03.9999\xdfN"
        );
        assert_eq!(
            format([255; 4], 5, &current, Some(0), Some(0x409)).unwrap(),
            b"--.----\xb0-"
        );
        assert_eq!(
            format([255; 4], 10, &current, Some(1), None).unwrap(),
            b"---.----\xdf-"
        );
        current[0x396b4] = 3;
        assert!(
            format([255; 4], 5, &current, Some(1), None)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            format(q(255, 12345, 255), 11, &[], Some(1), None).unwrap(),
            b"12'21\""
        );
        assert_eq!(
            format(q(255, 12345, 255), 12, &[], Some(1), None).unwrap(),
            b"12.34'"
        );
        for ticks in [0, 1, 80, 81] {
            assert_eq!(
                format(q(255, ticks, 255), 13, &[], Some(0), Some(0x409)).unwrap(),
                b"0.0014\xb0"
            );
        }
        assert_eq!(
            format(q(255, 87, 255), 13, &[], Some(1), None).unwrap(),
            b"0.0015\xdf"
        );
        assert_eq!(
            format(q(255, 65535, 255), 13, &[], Some(1), None).unwrap(),
            b"0.9999\xdf"
        );
        assert!(format([0; 4], 5, &[], Some(1), None).is_err());
        assert!(format([0; 4], 5, &current, None, Some(0x411)).is_err());
        assert!(format([0; 4], 11, &[], Some(0), None).is_err());
        assert!(format([0; 4], 4, &current, Some(1), None).is_err());
    }
}
