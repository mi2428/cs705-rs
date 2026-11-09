//! 84A58..84DB8 in native field order. The completed live type22/default profile
//! is explicit source data; it is not synthesized from a saved factory image.

use super::{
    fields::{self, TypeSource},
    staging::{Record, classify},
};
use crate::{csv::nls, gps::parser::valid_coordinate, icf::record};

/// Actual DB98/EDF0 and DB98/F20C outputs for this parsed frequency. Identity's
/// F094 class remains the separate Record.frequency_class; do not conflate them.
#[derive(Debug, Clone, Copy)]
pub struct OffsetProfile {
    pub class: i32,
    pub default_hz: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub accepted: bool,
    pub corrections: u32,
}

/// BAE84 uses signed DWORD abs/NEG and IDIV, including the INT_MIN anomaly.
pub fn utc_minutes(value: u32, default: u32) -> u32 {
    if value == 0xffff {
        return default;
    }
    let signed = value as i32;
    let mut magnitude = if signed < 0 {
        signed.wrapping_neg()
    } else {
        signed
    };
    if magnitude > 840 {
        return default;
    }
    magnitude = magnitude.wrapping_sub(magnitude % 5);
    if signed < 0 {
        magnitude.wrapping_neg() as u32
    } else {
        magnitude as u32
    }
}

/// Acquire the current type22 source immediately before calling this finalizer,
/// after the CSV value phase. Fatal does not mean an untouched holder: its late
/// DD/blank-call branch runs shared 5F64, then still runs C7340 after accounting.
pub fn finalize(
    row: &mut Record,
    initial: u32,
    default: &Record,
    offset: OffsetProfile,
    types: TypeSource,
) -> Outcome {
    let mut local = initial;
    local = local.wrapping_add(u32::from(nls::sanitize(&mut row.bytes[0x26..0x36], b' ')));
    local = local.wrapping_add(u32::from(nls::sanitize(&mut row.bytes[0x154..0x15c], b' ')));
    if row.group() >= 50 {
        row.put(0x20, 0);
        local = local.wrapping_add(1);
    }
    if row.dword(0x15c) == 0xffff {
        row.put(0x15c, default.dword(0x15c));
        local = local.wrapping_add(1);
    } else if row.dword(0x15c) == 1 {
        row.put(0x15c, 0);
        local = local.wrapping_add(1);
    }
    let maximum = if offset.class == 3 {
        99_999_900
    } else {
        9_999_900
    };
    let old = row.dword(0x48);
    if old <= maximum {
        let value = fields::quantize_offset_hz(old);
        if value != old {
            row.put(0x48, value);
            local = local.wrapping_add(1);
        }
    } else {
        row.put(0x48, offset.default_hz);
        row.put(0x44, u32::from(offset.default_hz != 0));
        local = local.wrapping_add(1);
    }
    if row.dword(0x44) == 0xffff {
        row.put(0x44, u32::from(row.dword(0x48) != 0));
        local = local.wrapping_add(1);
    } else if row.dword(0x15c) == 3 {
        if matches!(row.dword(0x44), 1 | 2) {
            row.put(0x44, 3);
            local = local.wrapping_add(1);
        }
    } else if row.dword(0x44) == 3 {
        row.put(0x44, 0);
        local = local.wrapping_add(1);
    }
    let kind = classify(row);
    let mut gateway: [u8; 8] = row.bytes[0x168..0x170].try_into().expect("gateway width");
    let mut changed = nls::fold_callsign(&mut gateway);
    if kind == 4 {
        if gateway[7] != b'G' {
            gateway[7] = b'G';
            changed = true;
        }
    } else if !matches!(gateway[7], b'G' | b' ') {
        gateway[7] = b' ';
        changed = true;
    }
    row.bytes[0x168..0x170].copy_from_slice(&gateway);
    local = local.wrapping_add(u32::from(changed));
    for at in [0x174, 0x178, 0x17c, 0x180] {
        if row.dword(at) == 0xffff {
            row.put(at, default.dword(at));
            local = local.wrapping_add(1);
        }
        if at == 0x178 && row.dword(at) == 50 {
            row.put(at, 24);
            local = local.wrapping_add(1);
        }
    }
    for (at, latitude) in [(0x25a, true), (0x25e, false)] {
        if !valid_coordinate(
            row.bytes[at..at + 4].try_into().expect("coordinate width"),
            latitude,
        ) {
            row.bytes[at..at + 4].copy_from_slice(&default.bytes[at..at + 4]);
            local = local.wrapping_add(1);
        }
    }
    let utc = utc_minutes(row.dword(0x184), default.dword(0x184));
    if utc != row.dword(0x184) {
        row.put(0x184, utc);
        local = 1;
    }
    let fatal = classify(row) == 4 && row.bytes[0x160..0x168] == [b' '; 8];
    if fatal {
        let binding = row.dword(8);
        row.bytes
            .copy_from_slice(record::reset_holder(0, 0, binding).as_bytes());
        local = 0;
    }
    let corrections = initial.wrapping_add(u32::from(local != 0));
    let kind = classify(row);
    fields::transform_type(row, kind, types);
    Outcome {
        accepted: !fatal,
        corrections,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row() -> Record {
        let mut bytes = *record::reset_holder(0x22, 0, 0x12345678).as_bytes();
        bytes[0x25d] = b'N';
        bytes[0x261] = b'E';
        let mut row = Record {
            bytes: Box::new(bytes),
            frequency_class: 1,
        };
        row.put(0x20, 2);
        row.put(0x3c, 145000000);
        row.put(0x44, 1);
        row.put(0x48, 600000);
        row
    }
    fn types() -> TypeSource {
        TypeSource {
            placeholder: *b"PLACEHLD",
            gateway_marker: *b"GATEWAY ",
        }
    }
    #[test]
    fn finalizer_order_compresses_counts_but_utc_overwrites_local_and_fatal_resets() {
        let default = row();
        let profile = OffsetProfile {
            class: 0,
            default_hz: 600000,
        };
        let mut r = row();
        r.put(0x15c, 1);
        assert_eq!(
            finalize(&mut r, 1, &default, profile, types()),
            Outcome {
                accepted: true,
                corrections: 2
            }
        );
        assert_eq!(r.dword(0x15c), 0);
        let mut r = row();
        r.put(0x15c, 1);
        assert_eq!(
            finalize(&mut r, u32::MAX, &default, profile, types()).corrections,
            u32::MAX
        );
        let mut r = row();
        r.put(0x15c, 1);
        r.put(0x184, 7);
        assert_eq!(
            finalize(&mut r, u32::MAX, &default, profile, types()).corrections,
            0
        );
        assert_eq!(r.dword(0x184), 5);
        let mut r = row();
        r.put(0x15c, 3);
        r.put(0x44, 1);
        let result = finalize(&mut r, 3, &default, profile, types());
        assert_eq!(
            result,
            Outcome {
                accepted: false,
                corrections: 3
            }
        );
        assert_eq!(
            (r.dword(0), r.dword(4), r.dword(8), r.dword(0x15c)),
            (0, 0, 0x12345678, 0)
        );
        assert_eq!(utc_minutes(0x80000000, 0), 0x7ffffffd);
        assert_eq!(utc_minutes(0xffff, 123), 123);
    }
}
