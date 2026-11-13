//! Strict canonical message storage, not GUI sanitization or a transmit engine.

use super::{Error, Icf, RECORD_BYTES};

pub(super) fn keyer_byte(byte: u8) -> bool {
    byte.is_ascii_uppercase() || byte.is_ascii_digit() || b" *,./?@^".contains(&byte)
}

fn record(channel: usize, count: usize, base: usize, width: usize) -> Result<usize, Error> {
    if channel >= count {
        return Err(Error::Unsupported {
            line: 5,
            reason: "message channel outside supported storage range".into(),
        });
    }
    Ok(base + width * channel)
}

fn invalid(at: usize, reason: &'static str) -> Error {
    Error::Invalid {
        line: 5 + at / RECORD_BYTES,
        reason: reason.into(),
    }
}

impl Icf<'_> {
    // Key 0x2c0: handler RVA 0x76cf4..0x76d9f; store RVA 0x47fd0.
    /// Borrow all 70 bytes of keyer channel 0..7, including NULs and unknown tails.
    /// No trimming, display-time repair, counter expansion or I/O occurs.
    /// # Errors
    /// Rejects an out-of-range channel before address arithmetic.
    pub fn keyer_memory_raw(&self, channel: usize) -> Result<&[u8], Error> {
        let at = record(channel, 8, 0x37158, 70)?;
        Ok(&self.memory[at..at + 70])
    }

    /// Store at most 70 canonical bytes, space-padded, into one keyer record.
    /// Accepts A-Z, digits, spaces and `*,./?@^`. A new `*` requires no other
    /// channel to contain `*` before its first NUL within its 70-byte prefix.
    /// Unknown current/sibling bytes are not decoded or repaired. No I/O occurs.
    /// # Errors
    /// Rejects invalid channels, alphabet/length or asterisk conflicts before storage.
    pub fn set_keyer_memory(&mut self, channel: usize, value: &[u8]) -> Result<(), Error> {
        let at = record(channel, 8, 0x37158, 70)?;
        if value.len() > 70 || !value.iter().copied().all(keyer_byte) {
            return Err(invalid(
                at,
                "keyer message outside canonical length or alphabet",
            ));
        }
        let stars = value.iter().filter(|&&b| b == b'*').count();
        if stars > 1 {
            return Err(invalid(at, "keyer message contains multiple asterisks"));
        }
        if stars == 1 {
            for sibling in 0..8 {
                if sibling != channel
                    && self
                        .keyer_memory_raw(sibling)?
                        .iter()
                        .take_while(|&&b| b != 0)
                        .any(|&b| b == b'*')
                {
                    return Err(invalid(
                        at,
                        "keyer asterisk conflicts with another channel prefix",
                    ));
                }
            }
        }
        let mut bytes = [b' '; 70];
        bytes[..value.len()].copy_from_slice(value);
        self.memory[at..at + 70].copy_from_slice(&bytes);
        Ok(())
    }

    // Key 0x2d1: handler RVA 0x7753b..0x77614; store RVA 0x775d0 -> 0x9a124.
    /// Borrow all 70 bytes in the safe eight-slot RTTY storage window (0..7).
    /// This bounded subset does not establish the full original UI channel domain.
    /// Unknown bytes, CR and tails are returned unchanged. No I/O occurs.
    /// # Errors
    /// Rejects channels outside the safe window before address arithmetic.
    pub fn rtty_memory_raw(&self, channel: usize) -> Result<&[u8], Error> {
        let at = record(channel, 8, 0x37388, 70)?;
        Ok(&self.memory[at..at + 70])
    }

    /// Store up to 70 canonical RTTY bytes with space padding in safe slot 0..7.
    /// The full original UI domain is not established. Accepts A-Z, digits, spaces,
    /// `!$&?"'-/.,:;()` and CR. CR is one byte; no LF is added. No clip, uppercase,
    /// backslash conversion, current-value normalization or transmission occurs.
    /// # Errors
    /// Rejects invalid channels or new length/alphabet before changing any bytes.
    pub fn set_rtty_memory(&mut self, channel: usize, value: &[u8]) -> Result<(), Error> {
        let at = record(channel, 8, 0x37388, 70)?;
        if value.len() > 70
            || !value.iter().all(|b| {
                b.is_ascii_uppercase() || b.is_ascii_digit() || b" !$&?\"'-/.,:;()\r".contains(b)
            })
        {
            return Err(invalid(
                at,
                "RTTY message outside canonical length or alphabet",
            ));
        }
        let mut bytes = [b' '; 70];
        bytes[..value.len()].copy_from_slice(value);
        self.memory[at..at + 70].copy_from_slice(&bytes);
        Ok(())
    }

    // Key 0x11f: handler RVA 0x6abb3..0x6ac83; encode/store RVA 0x9a252..0x9a28e.
    /// Borrow all 24 raw tone-code bytes of channel 0..15, including FF/tails.
    /// Does not mask low nibbles, decode display text, trim or transmit.
    /// # Errors
    /// Rejects an out-of-range channel before address arithmetic.
    pub fn dtmf_memory_raw(&self, channel: usize) -> Result<&[u8], Error> {
        let at = record(channel, 16, 0x375b8, 24)?;
        Ok(&self.memory[at..at + 24])
    }

    /// Store 0..24 canonical symbols `0123456789ABCD*#` as codes 0..15.
    /// Unused bytes become FF; full-width text has no extra terminator. Digit 0
    /// is a tone, not an end marker. No E/F aliases, filtering or timing engine.
    /// # Errors
    /// Rejects invalid channels, length or symbols before one record copy.
    pub fn set_dtmf_memory(&mut self, channel: usize, value: &[u8]) -> Result<(), Error> {
        let at = record(channel, 16, 0x375b8, 24)?;
        if value.len() > 24 {
            return Err(invalid(at, "DTMF message exceeds 24 symbols"));
        }
        let mut bytes = [0xff; 24];
        for (out, &b) in bytes.iter_mut().zip(value) {
            *out = match b {
                b'0'..=b'9' => b - b'0',
                b'A'..=b'D' => b - b'A' + 10,
                b'*' => 14,
                b'#' => 15,
                _ => return Err(invalid(at, "DTMF message contains a noncanonical symbol")),
            };
        }
        self.memory[at..at + 24].copy_from_slice(&bytes);
        Ok(())
    }
}
