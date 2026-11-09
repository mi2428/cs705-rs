use std::collections::VecDeque;

/// Original 17D40 query/ACK extractor. FE restarts with one FE; FD is omitted.
/// Each call consumes all available bytes and returns only the LAST complete
/// frame. Overflow returns [0] and leaves the unconsumed suffix in the queue.
/// This is intentionally different from the canonical [`super::Framer`].
#[derive(Debug, Default)]
pub struct Extractor {
    partial: Vec<u8>,
    queue: VecDeque<u8>,
}

impl Extractor {
    pub fn push(&mut self, bytes: &[u8]) {
        self.queue.extend(bytes);
    }
    pub fn partial(&self) -> &[u8] {
        &self.partial
    }
    pub fn queued(&self) -> usize {
        self.queue.len()
    }
    pub fn extract(&mut self) -> Vec<u8> {
        let mut output = Vec::new();
        while let Some(byte) = self.queue.pop_front() {
            if byte == 0xfe {
                self.partial.clear();
                self.partial.push(byte);
            } else if !self.partial.is_empty() {
                if byte == 0xfd {
                    output.clone_from(&self.partial);
                    self.partial.clear();
                } else if self.partial.len() >= 0x209 {
                    self.partial.clear();
                    return vec![0];
                } else {
                    self.partial.push(byte);
                }
            }
        }
        output
    }
}

/// 46F08: C-NUL terminates, nonhex bytes are skipped, and an odd last digit
/// is a high nibble followed by zero. It is not a strict script grammar.
pub fn decode_hex_script(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut high = None;
    for &b in text.iter().take_while(|&&b| b != 0) {
        let n = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => continue,
        };
        if let Some(h) = high.take() {
            out.push(h | n);
        } else {
            high = Some(n << 4);
        }
    }
    if let Some(h) = high {
        out.push(h);
    }
    out
}

/// 46970/47C94 token byte boundary, not Unicode character splitting.
/// Returns (token, consumed, is_settings). DBCS lead ranges are 81..9F/E0..EF;
/// only a single-byte delimiter is recognized. Settings consume strlen+2 even
/// if no closing delimiter exists, matching the caller's original arithmetic.
pub fn script_token(script: &[u8]) -> (&[u8], usize, bool) {
    let c_len = script.iter().position(|&b| b == 0).unwrap_or(script.len());
    let s = &script[..c_len];
    let settings = s.first() == Some(&b'(');
    let start = usize::from(settings);
    let delimiter = if settings { b')' } else { b'(' };
    let mut i = start;
    while i < s.len() && s[i] != delimiter {
        i += if matches!(s[i], 0x81..=0x9f | 0xe0..=0xef) && i + 1 < s.len() {
            2
        } else {
            1
        };
    }
    (
        &s[start..i],
        i - start + if settings { 2 } else { 0 },
        settings,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extractor_last_partial_overflow_and_all_splits() {
        let wire = [
            0, 0xfe, 0xfe, 0xef, 0xee, 0xe6, 1, 0xfd, 0xfe, 0xfe, 0xef, 0xee, 0xe6, 0, 0xfd, 0xfe,
            1,
        ];
        let mut x = Extractor::default();
        x.push(&wire);
        assert_eq!(x.extract(), [0xfe, 0xef, 0xee, 0xe6, 0]);
        assert_eq!(x.partial(), [0xfe, 1]);
        for split in 0..=7 {
            let w = [0xfe, 0xfe, 0xef, 0xee, 0xe6, 0, 0xfd];
            let mut x = Extractor::default();
            x.push(&w[..split]);
            let a = x.extract();
            x.push(&w[split..]);
            let b = x.extract();
            assert_eq!(
                if b.is_empty() { a } else { b },
                [0xfe, 0xef, 0xee, 0xe6, 0]
            );
        }
        let mut x = Extractor::default();
        x.push(&[0xfe]);
        x.push(&vec![1; 0x208]);
        x.push(&[2, 3, 4]);
        assert_eq!(x.extract(), [0]);
        assert_eq!(x.queued(), 2);
        let mut x = Extractor::default();
        x.push(&[0xfe]);
        x.push(&vec![1; 0x208]);
        x.push(&[0xfd]);
        assert_eq!(x.extract().len(), 0x209);
    }
    #[test]
    fn permissive_hex_and_dbcs_tokens() {
        assert_eq!(decode_hex_script(b"A -z b C\0FF"), [0xab, 0xc0]);
        assert_eq!(decode_hex_script(b"xyz"), []);
        assert_eq!(script_token(b"FE(115200,N,8,1)"), (&b"FE"[..], 2, false));
        assert_eq!(script_token(b"(x)FE"), (&b"x"[..], 3, true));
        assert_eq!(script_token(b"(x"), (&b"x"[..], 3, true));
        assert_eq!(script_token(&[0x81, b'(', b'A', b'(']).1, 3);
    }
}
