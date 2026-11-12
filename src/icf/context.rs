//! Live Q context fields and their original adoption/packed-decoding boundaries.
//!
//! Q is 24 bytes, not a factory image or a generated range owner. Known constructor
//! constants are generated here; registry/OS/header values remain explicit input.
//! Source: A1 producer-contract and RVAs 5435C/562E4/565A4/100F4/56CC8.

use super::Error;
use crate::clone::ContextFields;

const OFFSETS: [usize; 12] = [0, 4, 5, 8, 12, 13, 14, 15, 16, 17, 18, 19];

/// Exact Q bytes, including padding and the three non-comparison option bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    raw: [u8; 24],
}

impl Context {
    /// Explicit raw adoption, without recognizing a radio/region or filling gaps.
    pub fn from_bytes(raw: [u8; 24]) -> Self {
        Self { raw }
    }
    pub fn as_bytes(&self) -> &[u8; 24] {
        &self.raw
    }

    /// 5435C's real pre-registry initialization. This is not implicit Default,
    /// not a substitute for registry data and not the generated default image G.
    pub fn before_registry() -> Self {
        let mut context = Self { raw: [0; 24] };
        context.adopt(ContextFields([16, 1, 1, 3, 1, 1, 0, 1, 1, 0, 1, 0]));
        context.raw[0x14] = 1;
        context
    }

    /// The 12 compared fields in the shared clone/header representation.
    pub fn fields(&self) -> ContextFields {
        ContextFields(std::array::from_fn(|i| {
            let at = OFFSETS[i];
            if matches!(i, 0 | 3) {
                u32::from_le_bytes(self.raw[at..at + 4].try_into().unwrap())
            } else {
                u32::from(self.raw[at])
            }
        }))
    }

    /// 562E4: clamp only unsigned kind outside 0..16, copy the semantic fields,
    /// and preserve padding 6/7 and options 14..17. Byte fields retain low 8 bits.
    pub fn adopt(&mut self, fields: ContextFields) {
        for (i, mut value) in fields.0.into_iter().enumerate() {
            if i == 0 && value > 16 {
                value = 16;
            }
            let at = OFFSETS[i];
            if matches!(i, 0 | 3) {
                self.raw[at..at + 4].copy_from_slice(&value.to_le_bytes());
            } else {
                self.raw[at] = value as u8;
            }
        }
    }

    /// 565A4: clear Q first, decode all fields, THEN return checksum equality.
    /// Even false has already replaced the context. Registry's later kind 16
    /// override is deliberately separate so other callers can observe this state.
    pub fn decode_packed(&mut self, packed: u32) -> bool {
        self.raw.fill(0);
        self.raw[..4].copy_from_slice(&((packed >> 18) & 31).to_le_bytes());
        self.raw[8..12].copy_from_slice(&((packed >> 16) & 3).to_le_bytes());
        for (at, bit) in [
            (4, 25),
            (5, 24),
            (12, 8),
            (13, 7),
            (14, 29),
            (15, 26),
            (16, 27),
            (17, 15),
            (18, 14),
            (19, 11),
        ] {
            self.raw[at] = ((packed >> bit) & 1) as u8;
        }
        let check = (packed & 1) | ((packed >> 2) & 2) | ((packed >> 4) & 4);
        check == (packed & !0x49).count_ones() % 8
    }

    /// 5641C's native metadata producer. Full DWORD/byte values are shifted and
    /// ADDED with 32 bit wrap; unknown bytes are not bool-masked. Checksum uses
    /// only kind's low 5/C's low 2 bits, but the full ten semantic byte values.
    /// Padding and the three non-comparison option bytes are not serialized.
    pub fn encode_packed(&self) -> u32 {
        let kind = u32::from_le_bytes(self.raw[..4].try_into().unwrap());
        let c = u32::from_le_bytes(self.raw[8..12].try_into().unwrap());
        let mut check = (kind & 31).count_ones() + (c & 3).count_ones();
        let mut packed = (kind << 18).wrapping_add(c << 16);
        for (at, bit) in [
            (4, 25),
            (5, 24),
            (12, 8),
            (13, 7),
            (14, 29),
            (15, 26),
            (16, 27),
            (17, 15),
            (18, 14),
            (19, 11),
        ] {
            let value = u32::from(self.raw[at]);
            check += value;
            packed = packed.wrapping_add(value << bit);
        }
        check &= 7;
        packed.wrapping_add((check & 1) | ((check & 2) << 2) | ((check & 4) << 4))
    }

    /// Registry caller 5565B's actual failure correction: only kind becomes 16.
    pub fn adopt_registry_packed(&mut self, packed: u32) -> bool {
        let valid = self.decode_packed(packed);
        if !valid {
            self.raw[..4].copy_from_slice(&16_u32.to_le_bytes());
        }
        valid
    }

    /// 56CFC/100F4 selector. The finite match is the original 16 kind schema,
    /// not a copied factory array. Negative kind/override 0 would read before the
    /// original table; this safe API reports that unavailable source explicitly.
    pub fn selector(&self) -> Result<u8, Error> {
        let kind = i32::from_le_bytes(self.raw[..4].try_into().unwrap());
        if kind >= 16 {
            return Ok(16);
        }
        if self.raw[0x16] != 0 {
            return Ok(1);
        }
        match kind {
            0 | 6..=15 => Ok(0),
            1 => Ok(1),
            2 | 3 => Ok(2),
            4 => Ok(3),
            5 => Ok(4),
            _ => Err(invalid(
                "negative context kind requires table-before source",
            )),
        }
    }

    /// 56CC8: only selector>=5 reads the explicit SYSTEM LANGID. This is not the
    /// UI language or process locale. Missing OS input is not default English.
    pub fn source_selector(&self, system_langid: Option<u16>) -> Result<u8, Error> {
        let selector = self.selector()?;
        if selector < 5 {
            return Ok(selector);
        }
        let langid = system_langid.ok_or(invalid("system LANGID source unavailable"))?;
        Ok(if langid & 0x3ff == 0x11 { 0 } else { 1 })
    }

    /// 56DA4's raw byte result: F unless selector 4 and A!=0, otherwise 0.
    pub fn effective_f(&self) -> Result<u8, Error> {
        Ok(if self.selector()? == 4 && self.raw[4] != 0 {
            0
        } else {
            self.raw[15]
        })
    }
    /// 56D70's raw U result, not its low bit or a normalized boolean.
    pub fn effective_u(&self) -> Result<u8, Error> {
        let c = u32::from_le_bytes(self.raw[8..12].try_into().unwrap());
        Ok(if self.selector()? == 4 && matches!(c, 2 | 3) {
            0
        } else {
            self.raw[14]
        })
    }
    /// Actual 58BDC/58C5C/58C78 predicates used by stage9's central copies.
    pub fn condition_58bdc(&self) -> Result<bool, Error> {
        Ok(self.effective_f()? == 0)
    }
    /// 58C14's short-circuit pair: false only when both raw F and U are nonzero.
    pub fn condition_58c14(&self) -> Result<bool, Error> {
        Ok(self.effective_f()? == 0 || self.effective_u()? == 0)
    }
    pub fn condition_58c5c(&self) -> Result<bool, Error> {
        Ok(self.selector()? == 4 && self.raw[4] != 0)
    }
    pub fn condition_58c78(&self) -> bool {
        self.raw[16] == 0
    }
}

fn invalid(reason: &'static str) -> Error {
    Error::Invalid {
        line: 0,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_pack_uses_live_full_fields_wrapping_addition_and_original_checksum_inputs() {
        for (kind, c, at, value, expected) in [
            (1_u32, 0_u32, 4, 0, 0x0004_0001_u32),
            (31, 0, 4, 0, 0x007c_0041),
            (0, 3, 4, 0, 0x0003_0008),
            (0, 0, 4, 2, 0x0400_0008),
            (0, 0, 19, 255, 0x0007_f849),
            (0xffff_ffff, 0, 4, 0, 0xfffc_0041),
        ] {
            let mut raw = [0; 24];
            raw[..4].copy_from_slice(&kind.to_le_bytes());
            raw[8..12].copy_from_slice(&c.to_le_bytes());
            raw[at] = value;
            raw[6] = 0xa6;
            raw[7] = 0x7f;
            raw[20..].fill(255);
            assert_eq!(Context::from_bytes(raw).encode_packed(), expected);
        }
        for packed in [0, 0x40001, 0x40808, 0x40001149, 0x3fff_ffff] {
            let mut q = Context::before_registry();
            q.decode_packed(packed);
            let canonical = q.encode_packed();
            let mut roundtrip = Context::before_registry();
            assert!(roundtrip.decode_packed(canonical));
            assert_eq!(roundtrip.fields(), q.fields());
        }
    }

    #[test]
    fn constructor_and_header_adoption_keep_known_initialization_and_opaque_options_separate() {
        let context = Context::before_registry();
        assert_eq!(
            context.as_bytes(),
            &[
                16, 0, 0, 0, 1, 1, 0, 0, 3, 0, 0, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 0, 0, 0
            ]
        );
        assert!(context.source_selector(None).is_err());
        assert_eq!(context.source_selector(Some(0x411)).unwrap(), 0);
        assert_eq!(context.source_selector(Some(0x409)).unwrap(), 1);
        let mut context = Context::from_bytes([0xe7; 24]);
        context.adopt(ContextFields([
            31, 0x105, 0x180, 0x12345678, 1, 2, 3, 4, 5, 6, 7, 8,
        ]));
        assert_eq!(
            context.fields(),
            ContextFields([16, 5, 128, 0x12345678, 1, 2, 3, 4, 5, 6, 7, 8])
        );
        assert_eq!(&context.as_bytes()[6..8], &[0xe7; 2]);
        assert_eq!(&context.as_bytes()[20..24], &[0xe7; 4]);
    }

    #[test]
    fn packed_false_keeps_decoded_fields_and_only_registry_overrides_kind() {
        let mut decoded = Context::from_bytes([0xe7; 24]);
        assert!(decoded.decode_packed(0x12345639));
        assert_eq!(
            decoded.fields(),
            ContextFields([13, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0])
        );
        assert_eq!(&decoded.as_bytes()[20..], &[0; 4]);
        let mut failed = Context::before_registry();
        assert!(!failed.decode_packed(0x12345639 ^ (1 << 23)));
        let mut registry = Context::before_registry();
        assert!(!registry.adopt_registry_packed(0x12345639 ^ (1 << 23)));
        assert_eq!(&registry.as_bytes()[4..], &failed.as_bytes()[4..]);
        assert_eq!(&registry.as_bytes()[..4], &16_u32.to_le_bytes());
    }

    #[test]
    fn selector_schema_and_full_byte_capabilities_do_not_guess_locale_or_mask_to_bit0() {
        let mut context = Context::before_registry();
        for (kind, selector) in [
            (0_i32, 0),
            (1, 1),
            (2, 2),
            (3, 2),
            (4, 3),
            (5, 4),
            (6, 0),
            (15, 0),
            (16, 16),
            (31, 16),
        ] {
            context.raw[..4].copy_from_slice(&kind.to_le_bytes());
            assert_eq!(context.selector().unwrap(), selector);
        }
        context.raw[..4].copy_from_slice(&(-1_i32).to_le_bytes());
        assert!(context.selector().is_err());
        context.raw[22] = 2;
        assert_eq!(context.selector().unwrap(), 1);
        context.raw[22] = 0;
        context.raw[..4].copy_from_slice(&5_i32.to_le_bytes());
        assert_eq!(context.source_selector(None).unwrap(), 4);
        context.raw[15] = 0x80;
        context.raw[4] = 0;
        assert_eq!(context.effective_f().unwrap(), 0x80);
        context.raw[4] = 2;
        assert_eq!(context.effective_f().unwrap(), 0);
        assert!(context.condition_58bdc().unwrap());
        assert!(context.condition_58c5c().unwrap());
        context.raw[14] = 0x80;
        context.raw[8..12].copy_from_slice(&2_u32.to_le_bytes());
        assert_eq!(context.effective_u().unwrap(), 0);
        context.raw[8..12].copy_from_slice(&4_u32.to_le_bytes());
        assert_eq!(context.effective_u().unwrap(), 0x80);
        context.raw[16] = 0x80;
        assert!(!context.condition_58c78());
    }
}
