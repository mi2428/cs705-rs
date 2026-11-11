//! Explicit storage encoding; terminal strings are always UTF-8.
//! Neither decoding nor encoding may insert replacement characters.

use crate::Result;
use encoding_rs::{Encoding, SHIFT_JIS};
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextEncoding(&'static Encoding);
impl Default for TextEncoding {
    fn default() -> Self {
        Self(SHIFT_JIS)
    }
}
impl fmt::Display for TextEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.name())
    }
}
impl FromStr for TextEncoding {
    type Err = String;
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let encoding = if value.eq_ignore_ascii_case("cp932") {
            SHIFT_JIS
        } else {
            Encoding::for_label(value.as_bytes())
                .ok_or_else(|| format!("unknown encoding: {value}"))?
        };
        if !encoding.is_ascii_compatible() || encoding == encoding_rs::REPLACEMENT {
            return Err("storage encoding must be ASCII-compatible (for example shift_jis, utf-8 or windows-1252)".into());
        }
        Ok(Self(encoding))
    }
}
impl TextEncoding {
    pub fn display(self, bytes: &[u8]) -> String {
        let bytes = trim_padding(bytes);
        match self
            .0
            .decode_without_bom_handling_and_without_replacement(bytes)
        {
            Some(text) => safe_text(&text),
            None => format!("[Invalid {self}: {}]", bytes.escape_ascii()),
        }
    }
    pub fn encode(self, value: &str, capacity: usize) -> Result<Vec<u8>> {
        if value.chars().any(char::is_control) {
            return Err("text must not contain control characters".into());
        }
        let (bytes, _, errors) = self.0.encode(value);
        if errors {
            return Err(
                format!("text cannot be represented in {self}; no replacement was stored").into(),
            );
        }
        if bytes.len() > capacity {
            return Err(format!(
                "text needs {} encoded bytes in {self}; field capacity is {capacity} bytes",
                bytes.len()
            )
            .into());
        }
        // Reject aliases that would reopen as different Unicode text.
        if self
            .0
            .decode_without_bom_handling_and_without_replacement(&bytes)
            .as_deref()
            != Some(value)
        {
            return Err(format!("text does not round-trip exactly through {self}").into());
        }
        Ok(bytes.into_owned())
    }
    pub fn fixed<const N: usize>(self, value: &str, original: &[u8; N]) -> Result<[u8; N]> {
        if value == self.display(original) {
            return Ok(*original);
        }
        let bytes = self.encode(value, N)?;
        let mut result = [b' '; N];
        result[..bytes.len()].copy_from_slice(&bytes);
        Ok(result)
    }
}
fn trim_padding(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .rposition(|b| !matches!(b, b' ' | 0))
        .map_or(0, |i| i + 1);
    &bytes[..end]
}
pub fn safe_text(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cp932_roundtrip_capacity_and_invalid_input_are_explicit() {
        let encoding: TextEncoding = "cp932".parse().unwrap();
        let value = "\u{6771}\u{4eac}FM";
        let bytes = encoding.encode(value, 6).unwrap();
        assert_eq!(encoding.display(&bytes), value);
        assert!(encoding.encode(value, 5).is_err());
        assert!(encoding.encode("\u{1f600}", 16).is_err());
        assert!(encoding.display(&[0x82]).starts_with("[Invalid Shift_JIS:"));
        let original = [0x82; 16];
        assert_eq!(
            encoding
                .fixed(&encoding.display(&original), &original)
                .unwrap(),
            original
        );
        assert!("utf-16".parse::<TextEncoding>().is_err());
        assert_eq!(safe_text("\u{6771}\u{4eac}\x1b"), "\u{6771}\u{4eac}\\u{1b}");
    }
}
