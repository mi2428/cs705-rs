use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::ops::Range;

use super::{
    ContextFields, Direction, Error, Extractor, Frame, IMAGE_CAPACITY, Identity, IdentityGate,
    Profile, decode_hex_script, read_range, script_token, write_blocks_with_cap,
};

/// An already opened transport. Read must be nonblocking/bounded: Ok(0) means
/// no data, not disconnect; report disconnect as an explicit I/O error instead.
/// Write returns an actual prefix count. Only that known prefix is consumed;
/// Err/zero stops the operation, never retries an entire uncertain frame.
/// `pending_output` includes every adapter/OS unsent byte. All calls, including
/// close, must be bounded by the adapter; protocol deadlines cannot bound a
/// blocked OS call. The adapter alone implements OS access. Reconfiguration
/// must retain the same explicitly selected port; it must never search ports.
pub trait Transport: Read + Write {
    fn now_ms(&self) -> u64;
    fn wait_ms(&mut self, milliseconds: u64) -> io::Result<()>;
    fn pending_output(&mut self) -> io::Result<usize>;
    fn close(&mut self) -> io::Result<()>;
    /// S3 parenthesized raw ANSI token: close/reopen/config the same port.
    /// Return an explicit error for unsupported settings. Four-token normal
    /// baud/parity/bits/stop changes preserve the existing DTR/RTS controls.
    fn reconfigure(&mut self, settings_token: &[u8]) -> io::Result<()>;
}

#[cfg(test)]
#[path = "session_tests.rs"]
pub(crate) mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Echo {
    None,
    Known,
    Auto,
}

/// Normal 571E0/57540 settings, published as data only. Nonzero wrapper mode's
/// uninitialized echo local is NOT assigned a default or called USB support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportSettings {
    pub baud: u32,
    pub data_bits: u8,
    pub parity: u8,
    pub stop_bits: u8,
    pub dtr_control: u8,
    pub rts_control: u8,
    pub extra_preamble: usize,
    pub data_preamble: usize,
    pub echo: Echo,
}

impl Default for TransportSettings {
    fn default() -> Self {
        Self {
            baud: 115200,
            data_bits: 8,
            parity: b'N',
            stop_bits: 1,
            dtr_control: 0,
            rts_control: 0,
            extra_preamble: 0,
            data_preamble: 0,
            echo: Echo::Auto,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub settings: TransportSettings,
    /// Initial constructor C68=16; actual common setup overwrites it to 64.
    pub block_cap: i32,
    pub counted_end: bool,
    pub timeout_ms: u64,
    pub ack_timeout_ms: u64,
    pub drain_timeout_ms: u64,
    pub prepare_delay_ms: u64,
    pub range_gap_ms: u64,
    pub poll_ms: u64,
    /// S3 interleaves permissive hex tokens and parenthesized raw settings.
    /// Settings go to the caller adapter; they never select a different port.
    pub startup_script: Vec<u8>,
    /// None means the original FC bypass. Otherwise this is the selected
    /// saved/common generation, chosen by the caller, not a synthesized Q.
    pub expected_context: Option<ContextFields>,
    pub saved_selector: u8,
    pub flag57: u8,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            settings: TransportSettings::default(),
            block_cap: 64,
            counted_end: true,
            timeout_ms: 1000,
            ack_timeout_ms: 10000,
            drain_timeout_ms: 1000,
            prepare_delay_ms: 200,
            range_gap_ms: 10,
            poll_ms: 10,
            startup_script: Vec::new(),
            expected_context: None,
            saved_selector: 0,
            flag57: 0,
        }
    }
}

impl SessionConfig {
    /// 15E5C: only signed-positive input replaces the existing value.
    pub fn set_block_cap(&mut self, cap: i32) {
        if cap > 0 {
            self.block_cap = cap;
        }
    }
    fn validate(&self) -> Result<(), Error> {
        if !(1..=255).contains(&self.block_cap)
            || self.poll_ms == 0
            || self.timeout_ms == 0
            || self.ack_timeout_ms == 0
            || self.drain_timeout_ms == 0
            || self.settings.extra_preamble > 512
            || self.settings.data_preamble > 512
        {
            return Err(Error::Invalid("session configuration bounds"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Query,
    Read,
    Write,
}

/// Logical phases; send sites correspond to source S3/S6/S8/S10/S11/S12.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Startup,
    QuerySend,
    QueryWait,
    ReadSend,
    ReadWait,
    WriteStart,
    WriteData,
    WriteEnd,
    AckWait,
    Drain,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteDisposition {
    NotStarted,
    /// Some E3/data/end bytes may have reached the target; no automatic retry.
    Unconfirmed,
    /// E6 status 0 only. NOT flash verification, recovery or physical approval.
    Acknowledged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Protocol(Error),
    MalformedAck,
    Identity(u8),
    Io(io::ErrorKind),
    Reconfigure(io::ErrorKind),
    Cancelled,
    Timeout,
    EchoTimeout,
    EchoMismatch,
    Truncated,
    EarlyResponse,
    EndCount,
    MissingCoverage,
    /// Native read completion rejected the internal revision/signature image.
    ImageValidation,
    OutsideImage,
    OutsideCoverage,
    DrainTimeout,
    ClockRegressed,
    /// The original would enter its vector access exception path, not return
    /// a normal packet status. Native bounds checks fail visibly instead.
    SourceBounds,
}

impl Failure {
    pub fn result_code(&self) -> u8 {
        match self {
            Self::Identity(n) => *n,
            Self::Io(_) => 17,
            Self::Reconfigure(_) => 2,
            Self::EchoTimeout | Self::EchoMismatch => 4,
            Self::Timeout | Self::DrainTimeout => 5,
            Self::EndCount => 7,
            Self::MissingCoverage => 8,
            Self::ImageValidation => 15,
            Self::OutsideImage | Self::OutsideCoverage => 9,
            Self::Protocol(Error::Rejected(_)) => 10,
            Self::MalformedAck => 10,
            // Cancellation and monotonic-clock violations are native API
            // outcomes, not invented original wire result codes.
            Self::Cancelled | Self::ClockRegressed | Self::SourceBounds => 0xff,
            _ => 6,
        }
    }
}

/// Owned terminal result, including failure-prefix image and coverage. Bytes
/// whose coverage bit is clear are NOT acquired data even if the byte is zero.
#[derive(Debug)]
pub struct Outcome {
    pub operation: Operation,
    pub failure: Option<Failure>,
    pub close_error: Option<io::ErrorKind>,
    pub identity: Option<Identity>,
    pub identity_gate: IdentityGate,
    pub image: Vec<u8>,
    pub coverage: Vec<u8>,
    pub blocks: u32,
    pub payload_bytes: u64,
    pub requested_wire_bytes: u64,
    pub transmitted_wire_bytes: u64,
    pub received_wire_bytes: u64,
    /// C6C pair index and C70 byte index are construction-time values.
    pub range_index: usize,
    pub next_address: u32,
    pub echo: Echo,
    pub write_disposition: WriteDisposition,
    pub terminal_phase: State,
}

impl Outcome {
    pub fn result_code(&self) -> u8 {
        self.failure.as_ref().map_or(0, Failure::result_code)
    }
    pub fn acquired(&self, address: usize) -> bool {
        self.coverage
            .get(address / 8)
            .is_some_and(|b| b & (1 << (address % 8)) != 0)
    }
}

/// Source coverage vector keeps stale bytes on within-capacity regrowth.
/// Keeping logical length separate reproduces shrink/re-expand without unsafe
/// uninitialized memory. Newly allocated storage is explicitly zero-filled.
#[derive(Default)]
struct Coverage {
    bytes: Vec<u8>,
    len: usize,
}
impl Coverage {
    fn resize(&mut self, n: usize) {
        if n > self.bytes.len() {
            let mut bytes = vec![0; n];
            bytes[..self.len].copy_from_slice(&self.bytes[..self.len]);
            self.bytes = bytes;
        }
        self.len = n;
    }
    fn apply(&mut self, address: usize, count: usize) {
        self.resize((address + count).div_ceil(8));
        for a in address..address + count {
            self.bytes[a / 8] |= 1 << (a % 8);
        }
    }
    fn check(&mut self, required: Range<u32>) -> Result<(), Failure> {
        let n = IMAGE_CAPACITY.div_ceil(8);
        if self.len > n {
            return Err(Failure::OutsideCoverage);
        }
        self.resize(n);
        let mut missing = false;
        for a in 0..IMAGE_CAPACITY {
            let actual = self.bytes[a / 8] & (1 << (a % 8)) != 0;
            let expected = required.contains(&(a as u32));
            if actual && !expected {
                return Err(Failure::OutsideCoverage);
            }
            missing |= expected && !actual;
        }
        if missing {
            Err(Failure::MissingCoverage)
        } else {
            Ok(())
        }
    }
}

/// One-use synchronous session. `run` consumes the engine, so late callbacks
/// cannot restart a completed write. Start validation performs no I/O. Unlike
/// the original potentially unbounded S14 drain, the native close boundary has
/// an explicit finite timeout. No abort frame, reset or retry is invented.
pub struct Session {
    config: SessionConfig,
    profile: Option<Profile>,
    blocks: Vec<Range<u32>>,
    out: Outcome,
    coverage: Coverage,
    mirror: VecDeque<u8>,
    receive: VecDeque<u8>,
    extractor: Extractor,
    read_partial: Vec<u8>,
    phase: State,
    deadline: u64,
    last_now: u64,
    block_index: usize,
    pending_frame: Option<Vec<u8>>,
    pace_until: u64,
}

impl Session {
    pub fn query(config: SessionConfig) -> Result<Self, Error> {
        Self::new(config, Operation::Query, None, &[], 0)
    }
    pub fn read(config: SessionConfig, profile: Profile) -> Result<Self, Error> {
        Self::new(config, Operation::Read, Some(profile), &[], 0)
    }
    pub fn write(
        config: SessionConfig,
        profile: Profile,
        mask: u8,
        image: &[u8],
    ) -> Result<Self, Error> {
        if image.len() != IMAGE_CAPACITY {
            return Err(Error::Invalid("write image length"));
        }
        Self::new(config, Operation::Write, Some(profile), image, mask)
    }
    fn new(
        config: SessionConfig,
        operation: Operation,
        profile: Option<Profile>,
        image: &[u8],
        mask: u8,
    ) -> Result<Self, Error> {
        config.validate()?;
        let blocks = if operation == Operation::Write {
            write_blocks_with_cap(
                profile.expect("write profile").selector(),
                mask,
                config.block_cap as u32,
            )?
        } else {
            Vec::new()
        };
        let out = Outcome {
            operation,
            failure: None,
            close_error: None,
            identity: None,
            identity_gate: IdentityGate::default(),
            image: if operation == Operation::Read {
                vec![0; IMAGE_CAPACITY]
            } else {
                image.to_vec()
            },
            coverage: Vec::new(),
            blocks: 0,
            payload_bytes: 0,
            requested_wire_bytes: 0,
            transmitted_wire_bytes: 0,
            received_wire_bytes: 0,
            range_index: 0,
            next_address: 0,
            echo: config.settings.echo,
            write_disposition: WriteDisposition::NotStarted,
            terminal_phase: State::Startup,
        };
        Ok(Self {
            config,
            profile,
            blocks,
            out,
            coverage: Coverage::default(),
            mirror: VecDeque::new(),
            receive: VecDeque::new(),
            extractor: Extractor::default(),
            read_partial: Vec::new(),
            phase: State::Startup,
            deadline: 0,
            last_now: 0,
            block_index: 0,
            pending_frame: None,
            pace_until: 0,
        })
    }

    pub fn run(
        mut self,
        transport: &mut impl Transport,
        mut cancelled: impl FnMut() -> bool,
    ) -> Outcome {
        self.last_now = transport.now_ms();
        self.deadline = self.last_now.saturating_add(self.config.timeout_ms);
        let result = self.drive(transport, &mut cancelled);
        if let Err(error) = result {
            self.out.failure = Some(error);
        }
        self.out.terminal_phase = self.phase;
        // Close is attempted once on every terminal path, including cancel and
        // partial send. A cleanup error does not erase the primary failure.
        if let Err(e) = transport.close() {
            self.out.close_error = Some(e.kind());
            if self.out.failure.is_none() {
                self.out.failure = Some(Failure::Io(e.kind()));
            }
        }
        self.phase = State::Closed;
        self.out.coverage = self.coverage.bytes[..self.coverage.len].to_vec();
        self.out
    }

    fn transition(&mut self, state: State, now: u64) {
        self.phase = state;
        let duration = match state {
            State::AckWait => self.config.ack_timeout_ms,
            State::Drain => self.config.drain_timeout_ms,
            _ => self.config.timeout_ms,
        };
        self.deadline = now.saturating_add(duration);
    }

    fn send(
        &mut self,
        t: &mut impl Transport,
        bytes: &[u8],
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), Failure> {
        self.out.requested_wire_bytes += bytes.len() as u64;
        let mut offset = 0;
        while offset < bytes.len() {
            if cancelled() {
                return Err(Failure::Cancelled);
            }
            // Any E3-or-later write attempt may have an unobservable prefix
            // even when the OS returns Err. Do not retry the frame on error.
            if matches!(
                self.phase,
                State::WriteStart | State::WriteData | State::WriteEnd
            ) {
                self.out.write_disposition = WriteDisposition::Unconfirmed;
            }
            let n = t
                .write(&bytes[offset..])
                .map_err(|e| Failure::Io(e.kind()))?;
            if n == 0 {
                return Err(Failure::Io(io::ErrorKind::WriteZero));
            }
            if n > bytes.len() - offset {
                return Err(Failure::Io(io::ErrorKind::InvalidData));
            }
            offset += n;
            self.out.transmitted_wire_bytes += n as u64;
        }
        // Original six sites copy expected echo AFTER queue/send, regardless
        // of actual physical delivery. Failed sends above are not called safe.
        self.mirror.extend(bytes);
        Ok(())
    }

    fn wire(&self, frame: Frame) -> Result<Vec<u8>, Failure> {
        let preamble = match frame {
            Frame::Data { .. } => self.config.settings.data_preamble,
            Frame::End { .. } | Frame::EndUncounted => 0,
            _ => self.config.settings.extra_preamble,
        };
        let mut wire = vec![0xfe; preamble];
        wire.extend(
            frame
                .encode(Direction::ToRadio)
                .map_err(Failure::Protocol)?,
        );
        Ok(wire)
    }

    fn receive(&mut self, t: &mut impl Transport) -> Result<usize, Failure> {
        let mut bytes = [0; 0x2000];
        let n = t.read(&mut bytes).map_err(|e| Failure::Io(e.kind()))?;
        if n > bytes.len() {
            return Err(Failure::Io(io::ErrorKind::InvalidData));
        }
        self.out.received_wire_bytes += n as u64;
        self.receive.extend(&bytes[..n]);
        if self.out.echo == Echo::Auto {
            if self.receive.len() < self.mirror.len() {
                return Ok(n);
            }
            if self.receive.iter().zip(&self.mirror).all(|(a, b)| a == b) {
                for _ in 0..self.mirror.len() {
                    self.receive.pop_front();
                }
                self.out.echo = Echo::Known;
            } else {
                self.out.echo = Echo::None;
            }
            self.mirror.clear();
        } else if self.out.echo == Echo::Known {
            while let (Some(&actual), Some(&expected)) = (self.receive.front(), self.mirror.front())
            {
                if actual != expected {
                    return Err(Failure::EchoMismatch);
                }
                self.receive.pop_front();
                self.mirror.pop_front();
            }
        } else {
            self.mirror.clear();
        }
        Ok(n)
    }

    fn processed(&mut self) -> Vec<u8> {
        if self.out.echo == Echo::Auto {
            return Vec::new();
        }
        self.extractor
            .push(&self.receive.drain(..).collect::<Vec<_>>());
        self.extractor.extract()
    }

    fn query_response(&mut self, p: &[u8]) -> Result<(), Failure> {
        if p.len() < 8 || p[0] != 0xfe {
            return Err(Failure::Protocol(Error::Invalid("query response")));
        }
        if self.out.echo == Echo::None && p[1..4] == [0xee, 0xef, 0xe0] {
            return Err(Failure::Identity(5));
        }
        if p[1..4] != [0xef, 0xee, 0xe1] {
            return Err(Failure::Protocol(Error::Invalid(
                "query direction or command",
            )));
        }
        // 16D29/16D57/16D7A -> 16D87: raw BE stores/tail first, result0
        // last, including invalid BCD. Do not make the old strict gate normative.
        let identity = Identity {
            model: u16::from_be_bytes([p[4], p[5]]),
            revision: u16::from_be_bytes([p[6], p[7]]),
            tail: p[8..].to_vec(),
        };
        let result = self
            .out
            .identity_gate
            .apply(
                &identity,
                self.config.expected_context,
                self.config.saved_selector,
                self.config.flag57,
            )
            .map_err(Failure::Protocol)?;
        self.out.identity = Some(identity);
        if result != 0 {
            return Err(Failure::Identity(result));
        }
        if let Some(profile) = self.profile
            && self.out.identity.as_ref().expect("identity set").tail[1] != profile.selector()
        {
            return Err(Failure::Identity(14));
        }
        if self.out.operation == Operation::Write {
            // 56C80 -> 27518 -> 27A4A: only write adopts E1 bit2 into
            // W40/C18/C1C. Read retains its existing data preamble setting.
            self.config.settings.data_preamble =
                if self.out.identity.as_ref().expect("identity set").tail[0x18] & 4 != 0 {
                    8
                } else {
                    0
                };
        }
        Ok(())
    }

    fn read_packet(&mut self, p: &[u8]) -> Result<bool, Failure> {
        // Original state9 17EBC length gate; it differs from pure Frame decode.
        if !(12..=0x20a).contains(&p.len()) || p[..3] != [0xfe, 0xef, 0xee] {
            return Err(Failure::Protocol(Error::Invalid(
                "read response header or length",
            )));
        }
        if p[3] == 0xe5 {
            // Processed vector is FE EF EE E5 | payload. 17F7E receives [4:],
            // NOT [5:] (the second wire FE has already been collapsed).
            let payload = &p[4..];
            if !payload.starts_with(b"Icom Inc.") {
                return Err(Failure::Protocol(Error::Invalid("end brand")));
            }
            let expected = if self.config.counted_end {
                format!("Icom Inc.{:02X}", self.out.blocks as u8).into_bytes()
            } else {
                b"Icom Inc.".to_vec()
            };
            if payload.len() != expected.len() {
                return Err(Failure::Protocol(Error::Invalid("end length")));
            }
            if payload != expected {
                return Err(Failure::EndCount);
            }
            self.coverage.check(
                read_range(self.profile.expect("read profile").selector())
                    .map_err(Failure::Protocol)?,
            )?;
            return Ok(true);
        }
        if p[3] != 0xe4 {
            return Err(Failure::Protocol(Error::Invalid("read command")));
        }
        let Frame::Data { address, payload } =
            super::decode_data_body(&p[4..]).map_err(|e| match e {
                Error::SourceBounds => Failure::SourceBounds,
                Error::AddressOverflow => Failure::OutsideImage,
                e => Failure::Protocol(e),
            })?
        else {
            return Err(Failure::Protocol(Error::Invalid("read data")));
        };
        let start = address as usize;
        let end = start
            .checked_add(payload.len())
            .ok_or(Failure::OutsideImage)?;
        // Every window is validated BEFORE copying any byte. Unlike a strict
        // order policy, duplicates/overlap are accepted and later bytes win.
        if address > i32::MAX as u32 || end > self.out.image.len() {
            return Err(Failure::OutsideImage);
        }
        self.out.image[start..end].copy_from_slice(&payload);
        self.coverage.apply(start, payload.len());
        self.out.blocks = self.out.blocks.wrapping_add(1);
        self.out.payload_bytes += payload.len() as u64;
        Ok(false)
    }

    fn drive(
        &mut self,
        t: &mut impl Transport,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), Failure> {
        if cancelled() {
            return Err(Failure::Cancelled);
        }
        let script = self.config.startup_script.clone();
        let mut remaining = script.as_slice();
        while !remaining.is_empty() {
            if cancelled() {
                return Err(Failure::Cancelled);
            }
            let (token, consumed, settings) = script_token(remaining);
            if settings {
                t.reconfigure(token)
                    .map_err(|e| Failure::Reconfigure(e.kind()))?;
                self.drain_input(t, cancelled)?;
            } else {
                self.send(t, &decode_hex_script(token), cancelled)?;
                let deadline = t.now_ms().saturating_add(self.config.timeout_ms);
                loop {
                    self.receive(t)?;
                    if t.pending_output().map_err(|e| Failure::Io(e.kind()))? == 0
                        && (self.out.echo != Echo::Known || self.mirror.is_empty())
                    {
                        break;
                    }
                    if cancelled() {
                        return Err(Failure::Cancelled);
                    }
                    if t.now_ms() >= deadline {
                        return Err(if self.out.echo == Echo::Known && !self.mirror.is_empty() {
                            Failure::EchoTimeout
                        } else {
                            Failure::Timeout
                        });
                    }
                    t.wait_ms(self.config.poll_ms)
                        .map_err(|e| Failure::Io(e.kind()))?;
                }
            }
            if consumed == 0 || consumed >= remaining.len() {
                break;
            }
            remaining = &remaining[consumed..];
        }
        // S4/S5: discard startup input before the new E0 receive epoch. Echo
        // auto-detection is for E0, not proof of a startup command response.
        t.wait_ms(self.config.prepare_delay_ms)
            .map_err(|e| Failure::Io(e.kind()))?;
        self.drain_input(t, cancelled)?;
        self.receive.clear();
        self.mirror.clear();
        self.out.echo = self.config.settings.echo;
        self.transition(State::QuerySend, t.now_ms());
        loop {
            if cancelled() {
                return Err(Failure::Cancelled);
            }
            let now = t.now_ms();
            if now < self.last_now {
                return Err(Failure::ClockRegressed);
            }
            self.last_now = now;
            match self.phase {
                State::QuerySend => {
                    self.send(t, &self.wire(Frame::Identify)?, cancelled)?;
                    self.transition(State::QueryWait, now);
                }
                State::QueryWait => {
                    self.receive(t)?;
                    let p = self.processed();
                    if !p.is_empty() {
                        self.query_response(&p)?;
                        self.transition(
                            match self.out.operation {
                                Operation::Query => State::Drain,
                                Operation::Read => State::ReadSend,
                                Operation::Write => State::WriteStart,
                            },
                            now,
                        );
                        continue;
                    }
                }
                State::ReadSend => {
                    self.send(t, &self.wire(Frame::Read)?, cancelled)?;
                    self.transition(State::ReadWait, now);
                }
                State::ReadWait => {
                    // State9 processes its existing queue only on positive read;
                    // query/ACK Extractor does not have this gate.
                    if self.receive(t)? > 0 && self.out.echo != Echo::Auto {
                        while let Some(b) = self.receive.pop_front() {
                            if b == 0xfe {
                                self.read_partial.clear();
                                self.read_partial.push(b);
                            } else if !self.read_partial.is_empty() {
                                if b == 0xfd {
                                    let p = std::mem::take(&mut self.read_partial);
                                    if self.read_packet(&p)? {
                                        self.transition(State::Drain, now);
                                        break;
                                    }
                                    self.deadline = now.saturating_add(self.config.timeout_ms);
                                } else {
                                    self.read_partial.push(b);
                                    if self.read_partial.len() > 0x209 {
                                        return Err(Failure::Protocol(Error::Invalid(
                                            "read frame overflow",
                                        )));
                                    }
                                }
                            }
                        }
                    }
                }
                State::WriteStart => {
                    self.send(t, &self.wire(Frame::Write)?, cancelled)?;
                    self.transition(State::WriteData, now);
                }
                State::WriteData => {
                    self.receive(t)?;
                    if !self.processed().is_empty() {
                        return Err(Failure::EarlyResponse);
                    }
                    for _ in 0..16 {
                        // Echo hold at source S11 is >128, not an ACK window.
                        if self.out.echo == Echo::Known && self.mirror.len() > 128 {
                            break;
                        }
                        if self.pending_frame.is_none() {
                            let Some(block) = self.blocks.get(self.block_index).cloned() else {
                                if t.pending_output().map_err(|e| Failure::Io(e.kind()))? == 0
                                    && (self.out.echo != Echo::Known || self.mirror.is_empty())
                                {
                                    self.transition(State::WriteEnd, now);
                                }
                                break;
                            };
                            if self.block_index > 0
                                && self.blocks[self.block_index - 1].end != block.start
                            {
                                if t.pending_output().map_err(|e| Failure::Io(e.kind()))? != 0 {
                                    break;
                                }
                                self.out.range_index += 1;
                                self.pace_until = now.saturating_add(self.config.range_gap_ms);
                            }
                            let wire = self.wire(Frame::Data {
                                address: block.start,
                                payload: self.out.image[block.start as usize..block.end as usize]
                                    .to_vec(),
                            })?;
                            self.out.next_address = block.end;
                            self.out.blocks = self.out.blocks.wrapping_add(1);
                            self.out.payload_bytes += u64::from(block.end - block.start);
                            self.pending_frame = Some(wire);
                            self.block_index += 1;
                        }
                        let pending = t.pending_output().map_err(|e| Failure::Io(e.kind()))?;
                        let len = self.pending_frame.as_ref().expect("built frame").len();
                        // 177AE: source compares signed pending and wrapped 4L.
                        if now < self.pace_until
                            || (pending as u32 as i32) > ((len as u32).wrapping_mul(4) as i32)
                        {
                            break;
                        }
                        let wire = self.pending_frame.take().expect("built frame");
                        self.send(t, &wire, cancelled)?;
                        self.deadline = now.saturating_add(self.config.timeout_ms);
                    }
                }
                State::WriteEnd => {
                    // Consume/inspect anything already available before the end
                    // request; an early ACK is not credit for a later write.
                    self.receive(t)?;
                    if !self.processed().is_empty() {
                        return Err(Failure::EarlyResponse);
                    }
                    let end = if self.config.counted_end {
                        Frame::End {
                            block_count: self.out.blocks as u8,
                        }
                    } else {
                        Frame::EndUncounted
                    };
                    self.send(t, &self.wire(end)?, cancelled)?;
                    self.transition(State::AckWait, now);
                }
                State::AckWait => {
                    self.receive(t)?;
                    let p = self.processed();
                    // Original ACK wins over timeout in the same tick.
                    if !p.is_empty() {
                        if p != [0xfe, 0xef, 0xee, 0xe6, 0] {
                            return Err(if p.len() == 5 && p[..4] == [0xfe, 0xef, 0xee, 0xe6] {
                                Failure::Protocol(Error::Rejected(p[4]))
                            } else {
                                Failure::MalformedAck
                            });
                        }
                        self.out.write_disposition = WriteDisposition::Acknowledged;
                        self.transition(State::Drain, now);
                        continue;
                    }
                }
                State::Drain => {
                    if t.pending_output().map_err(|e| Failure::Io(e.kind()))? == 0 {
                        return Ok(());
                    }
                    if now >= self.deadline {
                        return Err(Failure::DrainTimeout);
                    }
                }
                _ => unreachable!("active phase"),
            }
            if now >= self.deadline {
                return Err(if self.out.echo == Echo::Known && !self.mirror.is_empty() {
                    Failure::EchoTimeout
                } else if !self.read_partial.is_empty() || !self.extractor.partial().is_empty() {
                    Failure::Truncated
                } else {
                    Failure::Timeout
                });
            }
            t.wait_ms(self.config.poll_ms)
                .map_err(|e| Failure::Io(e.kind()))?;
        }
    }

    fn drain_input(
        &mut self,
        t: &mut impl Transport,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), Failure> {
        // 15EF0: 100ms wait, then at most 65535 one-byte reads until count0.
        t.wait_ms(100).map_err(|e| Failure::Io(e.kind()))?;
        let mut byte = [0];
        for _ in 0..65535 {
            if cancelled() {
                return Err(Failure::Cancelled);
            }
            match t.read(&mut byte) {
                Ok(0) => break,
                Ok(1) => {}
                Ok(_) => return Err(Failure::Io(io::ErrorKind::InvalidData)),
                Err(e) => return Err(Failure::Io(e.kind())),
            }
        }
        Ok(())
    }
}
