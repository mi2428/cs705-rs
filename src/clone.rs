//! CS-705 1.30 clone bytes, profiles, image conversion and synchronous sessions.
//!
//! No port is enumerated or opened here. [`Transport`] adapts an already opened
//! caller-owned connection. Static source fidelity is not hardware validation:
//! E6 success does not establish flash persistence, physical compatibility,
//! safe power-off, recovery or permission to operate a radio.
//! Wire parsing preserves the source's permissive FF escape rule; checked image
//! bounds and explicit profiles are separate safety boundaries. See [`Extractor`]
//! for the original last-completed-frame query/ACK semantics.

use std::fmt;
use std::ops::Range;

mod plan;
pub use plan::{read_range, write_blocks, write_blocks_with_cap};
mod profile;
pub use profile::{ContextFields, IMAGE_CAPACITY, Identity, IdentityGate, Profile};
mod image;
pub use image::convert_image;
mod session;
pub use session::{
    Echo, Failure, Operation, Outcome, Session, SessionConfig, State, Transport, TransportSettings,
    WriteDisposition,
};
pub mod search;
mod stream;
pub use stream::{Extractor, decode_hex_script, script_token};

// Static evidence: CS-705 1.30 CS705.exe SHA-256
// e9439e1413e1258be77e6ef2188b72276dd714642e3858f4690e5d45653aeca3.
// Request RVAs 0x1696A/0x16DC4/0x1706D -> queue 0x15528 -> WriteFile 0x15772.
// E4: builder 0x172E2..0x1770A, escape 0x1581C, decoder 0x180DC..0x1837D.
// E5: 0x1785C/0x17F7E; E6: 0x1799B..0x17AA6. These are not live captures.
const MAX_FRAME_BYTES: usize = 6 + 2 * (4 + 1 + 255 + 1);
const REQUEST_ID: [u8; 4] = [0x40, 0x75, 0, 0];
const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// The required wire direction, independent of the frame's content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// PC to radio: destination EE, source EF.
    ToRadio,
    /// Radio to PC: destination EF, source EE.
    FromRadio,
}

impl Direction {
    fn addresses(self) -> [u8; 2] {
        match self {
            Self::ToRadio => [0xee, 0xef],
            Self::FromRadio => [0xef, 0xee],
        }
    }
}

/// Invalid, unsupported or unsuccessful clone-frame bytes. No payload is logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A structural boundary, direction, escape, length or address is invalid.
    Invalid(&'static str),
    /// A command outside the recovered clone command family was encountered.
    UnsupportedCommand(u8),
    /// The unescaped E4 body does not sum to zero modulo 256.
    ChecksumMismatch,
    /// A decoded byte window overflows the u32 address representation.
    AddressOverflow,
    /// The original decoder would access beyond its decoded vector.
    SourceBounds,
    /// An E6 response contained a nonzero status; it is not an acknowledgement.
    Rejected(u8),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(reason) => write!(f, "invalid clone frame: {reason}"),
            Self::UnsupportedCommand(command) => {
                write!(f, "unsupported clone command {command:02X}")
            }
            Self::ChecksumMismatch => f.write_str("clone data checksum mismatch"),
            Self::AddressOverflow => f.write_str("clone data address overflow"),
            Self::SourceBounds => f.write_str("clone decoded vector access boundary"),
            Self::Rejected(status) => write!(f, "clone E6 status {status:02X}"),
        }
    }
}

impl std::error::Error for Error {}

/// One clone frame's content, not a validated device operation or transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// E0 request with the fixed identity bytes 40 75 00 00; PC to radio only.
    Identify,
    /// E2 request with the same fixed identity; PC to radio only.
    Read,
    /// E3 request with the same fixed identity; PC to radio only.
    Write,
    /// E1 raw big-endian model/revision and opaque tail; no BCD rejection.
    Response(Identity),
    /// E4 data in either direction. The payload contains untransformed memory.
    Data {
        /// A byte address, encoded big-endian on the wire.
        address: u32,
        /// Zero to 255 memory bytes, before XOR 80 and escaping. Normal sends
        /// use cap 64; the original receive decoder has no <=64 gate.
        payload: Vec<u8>,
    },
    /// E5 with the fixed brand and an uppercase two-character hex block count.
    End {
        /// The E4 block count modulo 256, not the number of payload bytes.
        block_count: u8,
    },
    /// Source C30==0: brand without the optional ASCII count.
    EndUncounted,
    /// Exactly E6 00, radio to PC only; not proof of settings or power state.
    Acknowledgement,
}

impl Frame {
    /// Encode exactly one canonical frame. This performs no I/O.
    ///
    /// # Errors
    ///
    /// Rejects incompatible directions, data lengths above 255 and an
    /// exclusive data end that overflows u32. It does not infer allowed ranges.
    pub fn encode(&self, direction: Direction) -> Result<Vec<u8>, Error> {
        let command = match self {
            Self::Identify => 0xe0,
            Self::Read => 0xe2,
            Self::Write => 0xe3,
            Self::Response(_) => 0xe1,
            Self::Data { .. } => 0xe4,
            Self::End { .. } | Self::EndUncounted => 0xe5,
            Self::Acknowledgement => 0xe6,
        };
        check_direction(command, direction)?;
        let mut wire = vec![0xfe, 0xfe];
        wire.extend_from_slice(&direction.addresses());
        wire.push(command);
        match self {
            Self::Identify | Self::Read | Self::Write => wire.extend_from_slice(&REQUEST_ID),
            Self::Response(identity) => {
                wire.extend_from_slice(&identity.model.to_be_bytes());
                wire.extend_from_slice(&identity.revision.to_be_bytes());
                wire.extend_from_slice(&identity.tail);
            }
            Self::Data { address, payload } => {
                block_end(*address, payload.len())?;
                let mut body = address.to_be_bytes().to_vec();
                body.push(payload.len() as u8);
                body.extend(payload.iter().map(|byte| byte ^ 0x80));
                body.push(0u8.wrapping_sub(sum(&body)));
                for byte in body {
                    if byte >= 0xfa {
                        wire.extend_from_slice(&[0xff, byte & 0x0f]);
                    } else {
                        wire.push(byte);
                    }
                }
            }
            Self::End { block_count } => {
                wire.extend_from_slice(b"Icom Inc.");
                wire.push(HEX[usize::from(block_count >> 4)]);
                wire.push(HEX[usize::from(block_count & 15)]);
            }
            Self::EndUncounted => wire.extend_from_slice(b"Icom Inc."),
            Self::Acknowledgement => wire.push(0),
        }
        wire.push(0xfd);
        Ok(wire)
    }

    /// Decode a complete wire frame, including optional additional FE preambles.
    ///
    /// # Errors
    ///
    /// Repeated FF bytes keep escape pending; the next non-FF is ORed with F0.
    /// A trailing FF is discarded, as at RVA 180DC. Structural parsing accepts
    /// short/zero-length blocks; the source read-state length gate is separate.
    /// Bounds, checksum, direction and request identity remain checked.
    pub fn decode(wire: &[u8], direction: Direction) -> Result<Self, Error> {
        let preambles = wire.iter().take_while(|&&b| b == 0xfe).count();
        if preambles < 2 {
            return Err(Error::Invalid("frame prefix"));
        }
        let wire = &wire[preambles - 2..];
        if !(6..=MAX_FRAME_BYTES).contains(&wire.len()) {
            return Err(Error::Invalid("frame length"));
        }
        if !wire.starts_with(&[0xfe, 0xfe])
            || wire.last() != Some(&0xfd)
            || wire[2..wire.len() - 1]
                .iter()
                .any(|byte| matches!(byte, 0xfe | 0xfd))
        {
            return Err(Error::Invalid("frame boundary"));
        }
        if wire[2..4] != direction.addresses() {
            return Err(Error::Invalid("direction"));
        }
        let command = wire[4];
        check_direction(command, direction)?;
        let body = &wire[5..wire.len() - 1];
        match command {
            0xe0 | 0xe2 | 0xe3 => {
                if body != REQUEST_ID {
                    return Err(Error::Invalid("fixed request identity or length"));
                }
                Ok(match command {
                    0xe0 => Self::Identify,
                    0xe2 => Self::Read,
                    _ => Self::Write,
                })
            }
            0xe4 => decode_data_body(body),
            0xe1 if body.len() >= 4 => Ok(Self::Response(Identity {
                model: u16::from_be_bytes([body[0], body[1]]),
                revision: u16::from_be_bytes([body[2], body[3]]),
                tail: body[4..].to_vec(),
            })),
            0xe1 => Err(Error::Invalid("identity response length")),
            0xe5 => {
                if body == b"Icom Inc." {
                    return Ok(Self::EndUncounted);
                }
                let count = body
                    .strip_prefix(b"Icom Inc.")
                    .filter(|count| count.len() == 2)
                    .ok_or(Error::Invalid("end brand or length"))?;
                Ok(Self::End {
                    block_count: (hex_digit(count[0])? << 4) | hex_digit(count[1])?,
                })
            }
            0xe6 => match body {
                [0] => Ok(Self::Acknowledgement),
                [status] => Err(Error::Rejected(*status)),
                _ => Err(Error::Invalid("E6 status length")),
            },
            _ => Err(Error::UnsupportedCommand(command)),
        }
    }

    /// Check an E4 block against a caller-supplied half-open byte range.
    ///
    /// The caller must establish this range independently; an ICF's file length
    /// is not a wire capacity. No profile, allocation or transfer is inferred.
    ///
    /// ```
    /// use cs705::clone::{Direction, Frame};
    /// # fn main() -> Result<(), cs705::clone::Error> {
    /// let block = Frame::Data { address: 0x1000, payload: vec![0x7a, 0x80] };
    /// block.check_data_range(0x1000..0x2000)?; // Synthetic caller policy only.
    /// let wire = block.encode(Direction::ToRadio)?;
    /// assert_eq!(Frame::decode(&wire, Direction::ToRadio)?, block);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Rejects non-data frames, invalid blocks and any byte outside `allowed`.
    pub fn check_data_range(&self, allowed: Range<u32>) -> Result<(), Error> {
        let Self::Data { address, payload } = self else {
            return Err(Error::Invalid("not a data frame"));
        };
        let end = block_end(*address, payload.len())?;
        if *address < allowed.start || end > allowed.end {
            return Err(Error::Invalid("data outside caller range"));
        }
        Ok(())
    }
}

fn check_direction(command: u8, direction: Direction) -> Result<(), Error> {
    match (command, direction) {
        (0xe0 | 0xe2 | 0xe3, Direction::FromRadio) | (0xe1 | 0xe6, Direction::ToRadio) => {
            Err(Error::Invalid("command direction"))
        }
        _ => Ok(()),
    }
}

fn block_end(address: u32, count: usize) -> Result<u32, Error> {
    if count > 255 {
        return Err(Error::Invalid("data length outside 0..=255"));
    }
    address
        .checked_add(count as u32)
        .ok_or(Error::AddressOverflow)
}

// Shared by complete-wire and source processed-vector entry points. Header
// normalization is caller-specific; payload escaping/checksum must not drift.
fn decode_data_body(body: &[u8]) -> Result<Frame, Error> {
    let body = unescape(body)?;
    if body.len() < 5 {
        return Err(Error::SourceBounds);
    }
    if body.len() < 6 || body.len() != usize::from(body[4]) + 6 {
        return Err(Error::Invalid("data length"));
    }
    let address = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
    if sum(&body) != 0 {
        return Err(Error::ChecksumMismatch);
    }
    block_end(address, usize::from(body[4]))?;
    Ok(Frame::Data {
        address,
        payload: body[5..body.len() - 1].iter().map(|b| b ^ 0x80).collect(),
    })
}

fn sum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte))
}

fn unescape(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    let mut escaped = false;
    for &byte in bytes {
        if byte == 0xff {
            escaped = true;
            continue;
        }
        body.push(if escaped { byte | 0xf0 } else { byte });
        escaped = false;
    }
    Ok(body)
}

fn hex_digit(byte: u8) -> Result<u8, Error> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(Error::Invalid("uppercase hex block count")),
    }
}

/// A bounded, byte-at-a-time collector of canonical wire frames, not a session.
///
/// Each completed frame is returned separately, so concatenated frames are not
/// dropped. It does not decode commands: pass each result to [`Frame::decode`].
/// Noise and extra FE bytes are errors, not silently skipped synchronization.
#[derive(Debug, Default)]
pub struct Framer {
    partial: Vec<u8>,
}

impl Framer {
    /// Consume a byte, returning the completed frame including its delimiters.
    ///
    /// # Errors
    ///
    /// Rejects missing/extra FE, premature FD and frames over 528 bytes. An error
    /// clears the partial frame and consumes the offending byte. Stop corrupt input;
    /// resume only at a new complete two-FE prefix. A completed invalid command
    /// is instead reported by `Frame::decode`; it does not leave partial bytes.
    pub fn push(&mut self, byte: u8) -> Result<Option<Vec<u8>>, Error> {
        if self.partial.len() >= MAX_FRAME_BYTES
            || (self.partial.len() < 2 && byte != 0xfe)
            || (self.partial.len() >= 2 && byte == 0xfe)
            || (byte == 0xfd && self.partial.len() < 5)
        {
            self.partial.clear();
            return Err(Error::Invalid("frame boundary or length"));
        }
        self.partial.push(byte);
        if byte == 0xfd {
            Ok(Some(std::mem::take(&mut self.partial)))
        } else {
            Ok(None)
        }
    }

    /// End an input sequence and clear its partial frame.
    ///
    /// # Errors
    ///
    /// Returns an error for a truncated frame. The collector can then be reused
    /// only with a fresh complete frame; no partial input is retained.
    pub fn finish(&mut self) -> Result<(), Error> {
        let incomplete = !self.partial.is_empty();
        self.partial.clear();
        if incomplete {
            Err(Error::Invalid("truncated frame"))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Independent Python/struct expectations, not output from this Rust codec.
    // Four 64-byte blocks cover every possible memory byte and escape boundary.
    const BYTE_ORACLE: [&str; 4] = [
        "FEFEEEEFE40000000040808182838485868788898A8B8C8D8E8F909192939495969798999A9B9C9D9E9FA0A1A2A3A4A5A6A7A8A9AAABACADAEAFB0B1B2B3B4B5B6B7B8B9BABBBCBDBEBFE0FD",
        "FEFEEEEFE40000123440C0C1C2C3C4C5C6C7C8C9CACBCCCDCECFD0D1D2D3D4D5D6D7D8D9DADBDCDDDEDFE0E1E2E3E4E5E6E7E8E9EAEBECEDEEEFF0F1F2F3F4F5F6F7F8F9FF0AFF0BFF0CFF0DFF0EFF0F9AFD",
        "FEFEEEEFE400FF0AFF0DFF0E40000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F202122232425262728292A2B2C2D2E2F303132333435363738393A3B3C3D3E3FEBFD",
        "FEFEEEEFE40102030440404142434445464748494A4B4C4D4E4F505152535455565758595A5B5C5D5E5F606162636465666768696A6B6C6D6E6F707172737475767778797A7B7C7D7E7FD6FD",
    ];

    // Wrong direction, zero/65-byte lengths, exclusive-end overflow, checksum,
    // and a request incorrectly using the file revision as its second field.
    const INVALID_ORACLE: [&str; 6] = [
        "FEFEEFEEE4000012340480FF0AFF0F003DFD",
        "FEFEEEEFE4000000000000FD",
        "FEFEEEEFE4000000004180808080808080808080808080808080808080808080808080808080808080808080808080808080808080808080808080808080808080808080808080808080803FFD",
        "FEFEEEEFE4FF0FFF0FFF0FFF0F010003FD",
        "FEFEEEEFE4000012340480FF0AFF0F003CFD",
        "FEFEEEEFE040750001FD",
    ];

    fn hex(text: &str) -> Vec<u8> {
        assert!(text.len().is_multiple_of(2));
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn literal_controls_end_status_and_short_data() {
        for (frame, expected, direction) in [
            (Frame::Identify, "FEFEEEEFE040750000FD", Direction::ToRadio),
            (Frame::Read, "FEFEEEEFE240750000FD", Direction::ToRadio),
            (Frame::Write, "FEFEEEEFE340750000FD", Direction::ToRadio),
            (
                Frame::End { block_count: 1 },
                "FEFEEEEFE549636F6D20496E632E3031FD",
                Direction::ToRadio,
            ),
            (
                Frame::End { block_count: 255 },
                "FEFEEFEEE549636F6D20496E632E4646FD",
                Direction::FromRadio,
            ),
            (
                Frame::Acknowledgement,
                "FEFEEFEEE600FD",
                Direction::FromRadio,
            ),
            (
                Frame::Data {
                    address: 0x1234,
                    payload: vec![0, 0x7a, 0x7f, 0x80],
                },
                "FEFEEEEFE4000012340480FF0AFF0F003DFD",
                Direction::ToRadio,
            ),
            (
                Frame::Data {
                    address: 0,
                    payload: vec![0x80],
                },
                "FEFEEEEFE4000000000100FF0FFD",
                Direction::ToRadio,
            ),
            // Structurally valid; the original's receive-length gate rejects it.
            (
                Frame::Data {
                    address: 0,
                    payload: vec![0xff],
                },
                "FEFEEFEEE400000000017F80FD",
                Direction::FromRadio,
            ),
        ] {
            let expected = hex(expected);
            assert_eq!(frame.encode(direction).unwrap(), expected);
            assert_eq!(Frame::decode(&expected, direction).unwrap(), frame);
        }
        for frame in [Frame::Identify, Frame::Read, Frame::Write] {
            assert!(frame.encode(Direction::FromRadio).is_err());
        }
        assert!(Frame::Acknowledgement.encode(Direction::ToRadio).is_err());
        assert_eq!(
            Frame::decode(&hex("FEFEEFEEE601FD"), Direction::FromRadio),
            Err(Error::Rejected(1))
        );
        for status in 1..=255 {
            assert!(
                Frame::decode(
                    &[0xfe, 0xfe, 0xef, 0xee, 0xe6, status, 0xfd],
                    Direction::FromRadio
                )
                .is_err()
            );
        }
    }

    #[test]
    fn all_memory_bytes_match_independent_oracles_in_both_directions() {
        for (i, (expected, address)) in BYTE_ORACLE
            .into_iter()
            .zip([0, 0x1234, 0x00fafdfe, 0x01020304])
            .enumerate()
        {
            let frame = Frame::Data {
                address,
                payload: (0..64).map(|offset| (i * 64 + offset) as u8).collect(),
            };
            let mut expected = hex(expected);
            for (direction, addresses) in [
                (Direction::ToRadio, [0xee, 0xef]),
                (Direction::FromRadio, [0xef, 0xee]),
            ] {
                expected[2..4].copy_from_slice(&addresses);
                assert_eq!(frame.encode(direction).unwrap(), expected);
                assert_eq!(Frame::decode(&expected, direction).unwrap(), frame);
            }
            assert!(frame.check_data_range(address..address + 64).is_ok());
            assert!(frame.check_data_range(address + 1..address + 64).is_err());
            assert!(frame.check_data_range(address..address + 63).is_err());
        }
    }

    #[test]
    fn rejects_boundaries_lengths_identity_checksum_and_address_overflow() {
        // Source accepts 0 and 65-byte blocks. Those two old strict-only cases
        // are not protocol defects; image windows are checked by the session.
        for invalid in [
            INVALID_ORACLE[0],
            INVALID_ORACLE[3],
            INVALID_ORACLE[4],
            INVALID_ORACLE[5],
        ] {
            assert!(Frame::decode(&hex(invalid), Direction::ToRadio).is_err());
        }
        let request = hex("FEFEEEEFE040750000FD");
        for length in 0..request.len() {
            assert!(Frame::decode(&request[..length], Direction::ToRadio).is_err());
        }
        for invalid in [
            [request.clone(), vec![0]].concat(),
            [request.clone(), request].concat(),
            vec![0; MAX_FRAME_BYTES + 1],
        ] {
            assert!(Frame::decode(&invalid, Direction::ToRadio).is_err());
        }
        assert_eq!(
            Frame::decode(&hex("FEFEEFEEE100FD"), Direction::FromRadio),
            Err(Error::Invalid("identity response length"))
        );
        assert!(Frame::decode(&hex("FEFEEFEEE040750000FD"), Direction::FromRadio).is_err());
        assert!(Frame::decode(&hex("FEFEEEEFE600FD"), Direction::ToRadio).is_err());
        for text in [
            "FEFEEFEEE6FD",
            "FEFEEFEEE60000FD",
            "FEFEEEEFE549636F6D20496E632E3061FD",
            "FEFEEEEFE549636F6D20496E632E303100FD",
        ] {
            let direction = if text.starts_with("FEFEEFEE") {
                Direction::FromRadio
            } else {
                Direction::ToRadio
            };
            assert!(Frame::decode(&hex(text), direction).is_err());
        }
        for (address, count) in [(0, 256), (u32::MAX, 1), (u32::MAX - 63, 64)] {
            let frame = Frame::Data {
                address,
                payload: vec![0; count],
            };
            assert!(frame.encode(Direction::ToRadio).is_err());
            assert!(frame.check_data_range(0..u32::MAX).is_err());
        }
        let edge = Frame::Data {
            address: u32::MAX - 1,
            payload: vec![0],
        };
        assert!(edge.check_data_range(u32::MAX - 1..u32::MAX).is_ok());
        assert!(Frame::Identify.check_data_range(0..1).is_err());
    }

    #[test]
    fn permissive_source_escapes_are_not_a_strict_grammar() {
        for byte in 0..=255 {
            if byte != 0xff {
                assert_eq!(unescape(&[byte]).unwrap(), [byte]);
            } else {
                assert!(unescape(&[byte]).unwrap().is_empty());
            }
            if byte != 0xff {
                assert_eq!(unescape(&[0xff, byte]).unwrap(), [0xf0 | byte]);
            } else {
                assert!(unescape(&[0xff, byte]).unwrap().is_empty());
            }
        }
        assert_eq!(unescape(&[0xff, 0xff, 0, 0xff]).unwrap(), [0xf0]);
        // Old strict-only reject oracles are now affirmative source checks.
        for (i, n) in [(1, 0), (2, 65)] {
            assert_eq!(
                Frame::decode(&hex(INVALID_ORACLE[i]), Direction::ToRadio).unwrap(),
                Frame::Data {
                    address: 0,
                    payload: vec![0; n]
                }
            );
        }
        let wire = hex("FEFEEEEFE549636F6D20496E632EFD");
        assert_eq!(
            Frame::EndUncounted.encode(Direction::ToRadio).unwrap(),
            wire
        );
        assert_eq!(
            Frame::decode(&wire, Direction::ToRadio).unwrap(),
            Frame::EndUncounted
        );
    }

    #[test]
    fn framer_keeps_every_frame_at_every_split_boundary() {
        let mut expected = vec![hex("FEFEEEEFE040750000FD")];
        expected.extend(BYTE_ORACLE.into_iter().map(hex));
        expected.push(hex("FEFEEEEFE549636F6D20496E632E3031FD"));
        expected.push(hex("FEFEEFEEE600FD"));
        let stream = expected.concat();
        for split in 0..=stream.len() {
            let mut framer = Framer::default();
            let mut actual = Vec::new();
            for chunk in [&stream[..split], &stream[split..]] {
                for &byte in chunk {
                    if let Some(frame) = framer.push(byte).unwrap() {
                        actual.push(frame);
                    }
                }
            }
            framer.finish().unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn framer_errors_clear_partial_input_and_allow_a_fresh_frame() {
        let mut framer = Framer::default();
        assert!(framer.push(0).is_err());
        assert_eq!(framer.push(0xfe).unwrap(), None);
        assert!(framer.finish().is_err());
        assert_eq!(framer.push(0xfe).unwrap(), None);
        assert_eq!(framer.push(0xfe).unwrap(), None);
        assert!(framer.push(0xfe).is_err());
        assert_eq!(framer.push(0xfe).unwrap(), None);
        assert_eq!(framer.push(0xfe).unwrap(), None);
        assert!(framer.push(0xfd).is_err());
        for byte in [0xfe, 0xfe]
            .into_iter()
            .chain(std::iter::repeat_n(0, MAX_FRAME_BYTES - 2))
        {
            assert_eq!(framer.push(byte).unwrap(), None);
        }
        assert!(framer.push(0).is_err());
        assert!(framer.partial.is_empty());
        let request = hex("FEFEEEEFE040750000FD");
        let mut actual = None;
        for &byte in &request {
            actual = framer.push(byte).unwrap();
        }
        assert_eq!(actual, Some(request));
        framer.finish().unwrap();
    }
}
