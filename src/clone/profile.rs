use super::Error;

/// Internal object length (DF694), not the on-disk MapRev6 length 3C9C0.
pub const IMAGE_CAPACITY: usize = 0x4997a;

/// Explicit raw layout selector. It is not a firmware/region allowlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile(u8);

impl Profile {
    pub fn selector(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Profile {
    type Error = Error;
    fn try_from(value: u8) -> Result<Self, Error> {
        if (1..=6).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Error::Invalid("selector outside 1..=6"))
        }
    }
}

/// E1 fields remain raw BE16, not BCD-decoded decimal numbers.
/// At 16CA6 invalid BCD first stores result 6, then 16D87 overwrites it
/// with 0 after successful field/tail copies. No strict BCD gate is added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub model: u16,
    pub revision: u16,
    pub tail: Vec<u8>,
}

impl Identity {
    /// Upper 27EA4 model/revision/length/selector gate, without document context
    /// adoption. Extended tails up to 255 bytes are kept, not silently trimmed.
    pub fn validate(&self) -> Result<Profile, Error> {
        if self.model != 0x4075 {
            return Err(Error::Invalid("identity model"));
        }
        if self.revision != 1 {
            return Err(Error::Invalid("identity revision"));
        }
        if self.tail.len() < 33 || self.tail.len() != usize::from(self.tail[0]) {
            return Err(Error::Invalid("identity tail length"));
        }
        Profile::try_from(self.tail[1])
    }

    /// 56A4C -> 56154. Inputs are *not* nibble-masked before their wrapping
    /// sum, so carries from values above 0F are observable. The J marker only
    /// clears bits 1..4; no region name is inferred.
    pub fn context(&self) -> Result<ContextFields, Error> {
        ContextFields::from_tail(&self.tail)
    }
}

/// Twelve compared Q fields in offset order:
/// 00,04,05,08,0C,0D,0E,0F,10,11,12,13. Padding/Only are not compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextFields(pub [u32; 12]);

impl ContextFields {
    pub fn from_tail(tail: &[u8]) -> Result<Self, Error> {
        if tail.len() < 33 {
            return Err(Error::Invalid("context tail length"));
        }
        let mut v = 0u32;
        for (i, &b) in tail[0x12..0x18].iter().enumerate() {
            v = v.wrapping_add(u32::from(b) << (20 - 4 * i));
        }
        let mut u = (v & 0xff_ffff).reverse_bits() >> 8;
        if tail[0x1e] == b'J' {
            u &= !0x1e;
        }
        let bit = |n: u32| (u >> n) & 1u32;
        Ok(Self([
            8 * bit(1) + 4 * bit(2) + 2 * bit(3) + bit(4),
            bit(5),
            bit(6),
            2 * bit(7) + bit(8),
            bit(9),
            bit(10),
            u32::from((tail[0x18] >> 1) & 1),
            bit(0),
            u32::from(tail[0x18] & 1),
            bit(13),
            bit(11),
            bit(14),
        ]))
    }
}

/// Explicit upper-gate partial state (27EA4). `apply` preserves early model
/// and first-33-tail stores even when a later check fails. Context error 13
/// can overwrite selector error 14; only an exact flag57 value 1 activates it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IdentityGate {
    pub cached_model: Option<u16>,
    pub cached_tail: Option<[u8; 33]>,
}

impl IdentityGate {
    pub fn apply(
        &mut self,
        identity: &Identity,
        context: Option<ContextFields>,
        saved_selector: u8,
        flag57: u8,
    ) -> Result<u8, Error> {
        if identity.model != 0x4075 {
            return Ok(5);
        }
        self.cached_model = Some(identity.model);
        if identity.revision != 1 {
            return Ok(14);
        }
        if identity.tail.len() < 33 || identity.tail.len() != usize::from(identity.tail[0]) {
            return Ok(11);
        }
        self.cached_tail = Some(identity.tail[..33].try_into().expect("checked tail"));
        let mut result = if Profile::try_from(identity.tail[1]).is_ok() {
            0
        } else {
            14
        };
        if let Some(context) = context {
            let differs = identity.context()? != context
                || (saved_selector != 0 && saved_selector != identity.tail[1]);
            if differs && flag57 == 1 {
                result = 13;
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extended_tail_gate_prefix_and_unmasked_carry() {
        let mut id = Identity {
            model: 0x4075,
            revision: 1,
            tail: vec![0; 34],
        };
        id.tail[0] = 34;
        id.tail[1] = 6;
        assert_eq!(id.validate().unwrap().selector(), 6);
        id.tail[0x13] = 0x10;
        assert_eq!(id.context().unwrap().0[0], 2);
        id.tail[0x1e] = b'J';
        assert_eq!(id.context().unwrap().0[0], 0);
        let mut gate = IdentityGate::default();
        id.tail[1] = 0;
        assert_eq!(
            gate.apply(&id, Some(ContextFields([1; 12])), 6, 1).unwrap(),
            13
        );
        assert_eq!(gate.cached_model, Some(0x4075));
        assert_eq!(gate.cached_tail.unwrap()[0], 34);
        assert_eq!(
            gate.apply(&id, Some(ContextFields([1; 12])), 6, 2).unwrap(),
            14
        );
        id.revision = 2;
        id.tail[0] = 0;
        assert_eq!(gate.apply(&id, None, 0, 1).unwrap(), 14);
        assert_eq!(gate.cached_tail.unwrap()[0], 34);
        for selector in 0..=255 {
            assert_eq!(
                Profile::try_from(selector).is_ok(),
                (1..=6).contains(&selector)
            );
        }
    }
}
