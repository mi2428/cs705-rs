//! U11 IPv4/prefix linked edits (keys319..31D), not network/device operations.
//!
//! This intentionally preserves nonstandard byte parsing and bounded correction
//! loops. Unknown prefix bytes are not pre-clamped; x86 masks the shift count.
//! Gray DHCP presentation does not prohibit edits. Source: A4 BA02 IPv4 contract.

use std::net::Ipv4Addr;

use super::super::{Icf, text::decimal_prefix};

/// The four address fields; prefix uses its own selected-event entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Address,
    Gateway,
    PrimaryDns,
    SecondaryDns,
}

impl Field {
    fn offset(self) -> usize {
        match self {
            Self::Address => 0x3935b,
            Self::Gateway => 0x39360,
            Self::PrimaryDns => 0x39364,
            Self::SecondaryDns => 0x39368,
        }
    }
}

/// Exact stored values; all prefix bytes remain representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Configuration {
    pub address: Ipv4Addr,
    pub prefix: u8,
    pub gateway: Ipv4Addr,
    pub primary_dns: Ipv4Addr,
    pub secondary_dns: Ipv4Addr,
}

/// Native case-body effects, not the common grid's initial flag1 or dirty state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    pub stored: bool,
    pub notify_flags: u32,
    pub gray: bool,
}

/// 9C8B4: discard non-digit/non-dot bytes, then inspect at most15 bytes.
/// Empty/missing fields are zero; fifth and later fields are ignored. Each value
/// wraps to low16 before unsigned255 clamp. None means length rejection, not an
/// invalid RFC address. Empty IP input becomes0.0.0.1; empty GW/DNS remains zero.
pub fn parse_typed(source: &[u8], allow_zero: bool) -> Option<Ipv4Addr> {
    let source = source.split(|&b| b == 0).next().unwrap();
    let filtered: Vec<_> = source
        .iter()
        .copied()
        .filter(|b| b.is_ascii_digit() || *b == b'.')
        .collect();
    if filtered.len() > 15 {
        return None;
    }
    let mut fields = filtered.split(|&b| b == b'.');
    let mut bytes: [u8; 4] = std::array::from_fn(|_| {
        (decimal_prefix(fields.next().unwrap_or_default()) as u16).min(255) as u8
    });
    if !allow_zero && bytes == [0; 4] {
        bytes[3] = 1;
    }
    Some(Ipv4Addr::from(bytes))
}

/// 96E78 mask: prefix0 is special; other bytes use the x86 low-five shift bits.
pub fn prefix_mask(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        !((1_u32 << (32_u32.wrapping_sub(u32::from(prefix)) & 31)).wrapping_sub(1))
    }
}

fn address_bad(address: u32, mask: u32) -> u8 {
    if mask == 0 {
        3
    } else if address & !mask == 0 {
        2
    } else if address & !mask == !mask {
        1
    } else {
        0
    }
}

/// 9CC88 mode0. Returns the corrected value; it does not promise a valid host.
pub fn correct_address(address: Ipv4Addr, prefix: u8) -> Ipv4Addr {
    let mut address = u32::from(address);
    let mask = prefix_mask(prefix);
    for _ in 0..255 {
        if address_bad(address, mask) == 0 {
            break;
        }
        if address as u8 == 255 {
            address = address.wrapping_sub(1);
            break;
        }
        address = address.wrapping_add(1);
    }
    Ipv4Addr::from(address)
}

fn gateway_bad(address: u32, gateway: u32, mask: u32) -> u8 {
    if mask == 0 {
        3
    } else if address & mask != gateway & mask {
        4
    } else if gateway & !mask == 0 {
        2
    } else if gateway & !mask == !mask {
        1
    } else if address & !mask == gateway & !mask {
        5
    } else {
        0
    }
}

/// 9CD08: preserve gateway0, clamp subnet bytes, combine host bits, then run the
/// original255-iteration bad-host loop. /0,/32 and unknown prefix bytes can still
/// end invalid; no fallback gateway or additional validation is invented.
pub fn correct_gateway(address: Ipv4Addr, gateway: Ipv4Addr, prefix: u8) -> Ipv4Addr {
    if gateway.is_unspecified() {
        return gateway;
    }
    let address = u32::from(address);
    let mask = prefix_mask(prefix);
    let network = address & mask;
    let low = network.to_be_bytes();
    let high = (network | !mask).to_be_bytes();
    let clamped = std::array::from_fn(|i| gateway.octets()[i].clamp(low[i], high[i]));
    let mut gateway = network | (u32::from_be_bytes(clamped) & !mask);
    for _ in 0..255 {
        let bad = gateway_bad(address, gateway, mask);
        if bad == 0 {
            break;
        }
        if gateway as u8 == 255 || bad == 1 {
            gateway = gateway.wrapping_sub(1);
            if gateway & !mask == address & !mask {
                gateway = gateway.wrapping_sub(1);
            }
            break;
        }
        gateway = gateway.wrapping_add(1);
    }
    Ipv4Addr::from(gateway)
}

impl Icf<'_> {
    /// Read all stored network bytes, with no host validation or implicit repair.
    pub fn network_configuration(&self) -> Configuration {
        Configuration {
            address: get(self, 0x3935b),
            prefix: self.memory[0x3935f],
            gateway: get(self, 0x39360),
            primary_dns: get(self, 0x39364),
            secondary_dns: get(self, 0x39368),
        }
    }

    /// Apply a typed address entry. None is helper cancellation/display; over15
    /// filtered bytes also skips the raw copy. Other accepted text, even empty,
    /// uses the original parser and ordered field-specific linked corrections.
    pub fn edit_network_address(&mut self, field: Field, input: Option<&[u8]>) -> Effect {
        let gray = self.memory[0x3935a] != 0;
        let Some(value) = input.and_then(|source| parse_typed(source, field != Field::Address))
        else {
            return Effect {
                stored: false,
                notify_flags: 0,
                gray,
            };
        };
        put(self, field.offset(), value);
        let notify_flags = match field {
            Field::Address => {
                let prefix = self.memory[0x3935f];
                put(self, 0x3935b, correct_address(value, prefix));
                let old = get(self, 0x39360);
                let new = correct_gateway(get(self, 0x3935b), old, prefix);
                put(self, 0x39360, new);
                if old != new { 0x20 } else { 0 }
            }
            Field::Gateway => {
                let corrected = correct_gateway(get(self, 0x3935b), value, self.memory[0x3935f]);
                put(self, field.offset(), corrected);
                0
            }
            Field::PrimaryDns | Field::SecondaryDns => {
                if u32::from(value) == u32::MAX {
                    put(self, field.offset(), Ipv4Addr::from(u32::MAX - 1));
                }
                0
            }
        };
        Effect {
            stored: true,
            notify_flags,
            gray,
        }
    }

    /// Prefix helper's selected key is2, not its outer key31A or Flex0=1.
    /// Store the candidate's low byte before address/gateway corrections. Prefix
    /// itself is never clamped. Correction of either linked field adds flag20.
    pub fn select_network_prefix(
        &mut self,
        key: u16,
        phase: u32,
        candidate: i32,
        accepted: bool,
    ) -> Effect {
        let gray = self.memory[0x3935a] != 0;
        if key != 2 || phase != 2 || !accepted {
            return Effect {
                stored: false,
                notify_flags: 0,
                gray,
            };
        }
        self.memory[0x3935f] = candidate as u8;
        let old_ip = get(self, 0x3935b);
        let old_gw = get(self, 0x39360);
        let new_ip = correct_address(old_ip, candidate as u8);
        let new_gw = correct_gateway(new_ip, old_gw, candidate as u8);
        put(self, 0x3935b, new_ip);
        put(self, 0x39360, new_gw);
        Effect {
            stored: true,
            notify_flags: if old_ip != new_ip || old_gw != new_gw {
                0x20
            } else {
                0
            },
            gray,
        }
    }
}

fn get(image: &Icf<'_>, offset: usize) -> Ipv4Addr {
    Ipv4Addr::from(<[u8; 4]>::try_from(&image.memory[offset..offset + 4]).unwrap())
}
fn put(image: &mut Icf<'_>, offset: usize, value: Ipv4Addr) {
    image.memory[offset..offset + 4].copy_from_slice(&value.octets());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(bytes: [u8; 4]) -> Ipv4Addr {
        Ipv4Addr::from(bytes)
    }
    fn image() -> Icf<'static> {
        let mut bytes = vec![0xa6; 0x3c9c0];
        bytes[0x355a1] = 6;
        bytes[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        Icf::from_memory(bytes).unwrap()
    }

    #[test]
    fn literal_octet_wrap_filter_empty_and_first_four_fields_match_original_grammar() {
        for (input, expected) in [
            (b"65536.1.2.3".as_slice(), [0, 1, 2, 3]),
            (b"999.1", [255, 1, 0, 0]),
            (b"1e2.3.4.5", [12, 3, 4, 5]),
            (b"-1", [1, 0, 0, 0]),
            (b"1.2.3.4.5", [1, 2, 3, 4]),
            (b"4294967297", [1, 0, 0, 0]),
            (b"", [0, 0, 0, 1]),
        ] {
            assert_eq!(parse_typed(input, false), Some(ip(expected)));
        }
        assert_eq!(parse_typed(b"", true), Some(ip([0; 4])));
        assert_eq!(parse_typed(b"1234567890123456", true), None);
        assert_eq!(prefix_mask(33), 0x80000000);
        assert_eq!(prefix_mask(64), u32::MAX);
        assert_eq!(prefix_mask(0), 0);
        for prefix in 1..=255 {
            assert_eq!(prefix_mask(prefix), prefix_mask((prefix - 1) % 32 + 1));
        }
    }

    #[test]
    fn bounded_corrections_preserve_noncanonical_prefixes_and_asymmetric_endpoints() {
        assert_eq!(correct_address(ip([192, 0, 2, 0]), 24), ip([192, 0, 2, 1]));
        assert_eq!(
            correct_address(ip([192, 0, 2, 255]), 24),
            ip([192, 0, 2, 254])
        );
        assert_eq!(correct_address(ip([0; 4]), 0), ip([0, 0, 0, 255]));
        assert_eq!(
            correct_gateway(ip([192, 0, 2, 1]), ip([192, 0, 2, 1]), 24),
            ip([192, 0, 2, 2])
        );
        assert_eq!(
            correct_gateway(ip([192, 0, 2, 254]), ip([192, 0, 2, 255]), 24),
            ip([192, 0, 2, 253])
        );
        assert_eq!(
            correct_gateway(ip([192, 0, 2, 1]), ip([198, 51, 100, 7]), 24),
            ip([192, 0, 2, 7])
        );
        assert_eq!(
            correct_gateway(ip([192, 0, 2, 1]), ip([0; 4]), 24),
            ip([0; 4])
        );
    }

    #[test]
    fn image_caller_store_order_gray_and_additional_notification_bits_are_not_unified() {
        let mut image = image();
        image.memory[0x3935a] = 1;
        image.memory[0x3935f] = 24;
        put(&mut image, 0x3935b, ip([192, 0, 2, 1]));
        put(&mut image, 0x39360, ip([192, 0, 2, 7]));
        let before = image.memory().to_vec();
        assert_eq!(
            image.edit_network_address(Field::Address, None),
            Effect {
                stored: false,
                notify_flags: 0,
                gray: true
            }
        );
        assert_eq!(image.memory(), before);
        assert_eq!(
            image.edit_network_address(Field::Address, Some(b"192.0.2.0")),
            Effect {
                stored: true,
                notify_flags: 0,
                gray: true
            }
        );
        assert_eq!(image.network_configuration().address, ip([192, 0, 2, 1]));
        assert_eq!(
            image
                .edit_network_address(Field::Address, Some(b"198.51.100.0"))
                .notify_flags,
            0x20
        );
        assert_eq!(image.network_configuration().gateway, ip([198, 51, 100, 7]));
        assert_eq!(
            image
                .edit_network_address(Field::Gateway, Some(b"198.51.100.1"))
                .notify_flags,
            0
        );
        assert_eq!(image.network_configuration().gateway, ip([198, 51, 100, 2]));
        assert_eq!(
            image
                .edit_network_address(Field::PrimaryDns, Some(b"255.255.255.255"))
                .notify_flags,
            0
        );
        assert_eq!(
            image.network_configuration().primary_dns,
            ip([255, 255, 255, 254])
        );
        assert!(!image.select_network_prefix(1, 2, 33, true).stored);
        assert!(image.select_network_prefix(2, 2, 33, true).stored);
        assert_eq!(image.network_configuration().prefix, 33);
        for (at, (&old, &new)) in before.iter().zip(image.memory()).enumerate() {
            if !(0x3935b..0x3936c).contains(&at) {
                assert_eq!(old, new);
            }
        }
    }
}
