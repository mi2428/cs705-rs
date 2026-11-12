use super::*;
use std::cell::Cell;
use std::rc::Rc;

pub(crate) fn config() -> SessionConfig {
    SessionConfig {
        prepare_delay_ms: 0,
        poll_ms: 1,
        range_gap_ms: 1,
        ..SessionConfig::default()
    }
}
fn profile(n: u8) -> Profile {
    Profile::try_from(n).unwrap()
}
pub(crate) fn response(selector: u8) -> Vec<u8> {
    // Independent E1 literal: no production encoder, no BCD conversion.
    let mut r = vec![0xfe, 0xfe, 0xef, 0xee, 0xe1, 0x40, 0x75, 0, 1, 33, selector];
    r.extend_from_slice(&[0; 31]);
    r.push(0xfd);
    r
}
pub(crate) fn packet(address: u32, payload: &[u8]) -> Vec<u8> {
    // Independent transcript recipe; intentionally does not call Frame.
    let mut body = address.to_be_bytes().to_vec();
    body.push(payload.len() as u8);
    for b in payload {
        body.push(b.wrapping_add(128));
    }
    let total: usize = body.iter().map(|b| usize::from(*b)).sum();
    body.push((256 - total % 256) as u8);
    let mut out = vec![254, 254, 239, 238, 228];
    for b in body {
        if b > 249 {
            out.extend_from_slice(&[255, b % 16]);
        } else {
            out.push(b);
        }
    }
    out.push(253);
    out
}
pub(crate) fn end(count: usize) -> Vec<u8> {
    let mut e = vec![254, 254, 239, 238, 229];
    e.extend_from_slice(format!("Icom Inc.{:02X}", count % 256).as_bytes());
    e.push(253);
    e
}

pub(crate) struct Mock {
    now: u64,
    chunk: usize,
    max_write: usize,
    echo: bool,
    selector: u8,
    incoming: VecDeque<u8>,
    transmitted: Vec<u8>,
    partial: Vec<u8>,
    pub(crate) commands: Vec<u8>,
    pub(crate) read_data: Vec<u8>,
    pub(crate) query_response: Option<Vec<u8>>,
    ack: Option<Vec<u8>>,
    ack_delay: u64,
    ack_at: Option<u64>,
    delayed: Vec<u8>,
    early_ack: bool,
    fail_after: Option<usize>,
    zero_write: bool,
    fail_read: bool,
    pending: usize,
    stuck_after_ack: bool,
    close_failure: bool,
    clock_failure: bool,
    pub(crate) closes: usize,
    shared_sent: Rc<Cell<usize>>,
    reads: usize,
    configurations: Vec<Vec<u8>>,
    reconfigure_failure: bool,
}
impl Mock {
    pub(crate) fn new(selector: u8) -> Self {
        Self {
            now: 0,
            chunk: 8192,
            max_write: usize::MAX,
            echo: false,
            selector,
            incoming: VecDeque::new(),
            transmitted: Vec::new(),
            partial: Vec::new(),
            commands: Vec::new(),
            read_data: Vec::new(),
            query_response: None,
            ack: Some(vec![254, 254, 239, 238, 230, 0, 253]),
            ack_delay: 0,
            ack_at: None,
            delayed: Vec::new(),
            early_ack: false,
            fail_after: None,
            zero_write: false,
            fail_read: false,
            pending: 0,
            stuck_after_ack: false,
            close_failure: false,
            clock_failure: false,
            closes: 0,
            shared_sent: Rc::new(Cell::new(0)),
            reads: 0,
            configurations: Vec::new(),
            reconfigure_failure: false,
        }
    }
    fn complete(&mut self, p: Vec<u8>) {
        let prefix = p.iter().take_while(|&&b| b == 254).count();
        if p.len() <= prefix + 2 {
            return;
        }
        let command = p[prefix + 2];
        self.commands.push(command);
        match command {
            224 => self.incoming.extend(
                self.query_response
                    .clone()
                    .unwrap_or_else(|| response(self.selector)),
            ),
            226 => self.incoming.extend(&self.read_data),
            227 if self.early_ack => self.incoming.extend([254, 254, 239, 238, 230, 0, 253]),
            229 => {
                if let Some(ack) = &self.ack {
                    self.delayed = ack.clone();
                    self.ack_at = Some(self.now + self.ack_delay);
                }
                if self.stuck_after_ack {
                    self.pending = 1;
                }
            }
            _ => {}
        }
    }
}
impl Read for Mock {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        if self.fail_read && !self.commands.is_empty() {
            return Err(io::ErrorKind::NotConnected.into());
        }
        if self.ack_at.is_some_and(|at| self.now >= at) {
            self.incoming.extend(std::mem::take(&mut self.delayed));
            self.ack_at = None;
        }
        let n = out.len().min(self.chunk).min(self.incoming.len());
        for byte in &mut out[..n] {
            *byte = self.incoming.pop_front().unwrap();
        }
        Ok(n)
    }
}
impl Write for Mock {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.zero_write {
            return Ok(0);
        }
        let remaining = self
            .fail_after
            .map_or(usize::MAX, |n| n.saturating_sub(self.transmitted.len()));
        if remaining == 0 {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let n = bytes.len().min(self.max_write).min(remaining);
        self.transmitted.extend_from_slice(&bytes[..n]);
        self.shared_sent.set(self.transmitted.len());
        if self.echo {
            self.incoming.extend(&bytes[..n]);
        }
        for &b in &bytes[..n] {
            self.partial.push(b);
            if b == 253 {
                let p = std::mem::take(&mut self.partial);
                self.complete(p);
            }
        }
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Transport for Mock {
    fn now_ms(&self) -> io::Result<u64> {
        if self.clock_failure {
            Err(io::ErrorKind::PermissionDenied.into())
        } else {
            Ok(self.now)
        }
    }
    fn wait_ms(&mut self, ms: u64) -> io::Result<()> {
        self.now += ms;
        Ok(())
    }
    fn pending_output(&mut self) -> io::Result<usize> {
        Ok(self.pending)
    }
    fn close(&mut self) -> io::Result<()> {
        self.closes += 1;
        if self.close_failure {
            Err(io::ErrorKind::Other.into())
        } else {
            Ok(())
        }
    }
    fn reconfigure(&mut self, token: &[u8]) -> io::Result<()> {
        self.configurations.push(token.to_vec());
        if self.reconfigure_failure {
            Err(io::ErrorKind::InvalidInput.into())
        } else {
            Ok(())
        }
    }
}

#[test]
fn entry_clock_and_cancel_errors_close_once_and_keep_the_written_prefix() {
    let mut transport = Mock::new(6);
    transport.clock_failure = true;
    transport.close_failure = true;
    let outcome = Session::query(config()).unwrap().run(&mut transport, || {
        panic!("cancellation must not run after entry clock failure")
    });
    assert_eq!(
        outcome.failure,
        Some(Failure::Io(io::ErrorKind::PermissionDenied))
    );
    assert_eq!(outcome.close_error, Some(io::ErrorKind::Other));
    assert_eq!(transport.closes, 1);
    assert_eq!(outcome.transmitted_wire_bytes, 0);

    let mut transport = Mock::new(6);
    transport.max_write = 3;
    transport.close_failure = true;
    let sent = transport.shared_sent.clone();
    let outcome = Session::query(config()).unwrap().run(&mut transport, || {
        if sent.get() != 0 {
            Err(io::ErrorKind::PermissionDenied.into())
        } else {
            Ok(false)
        }
    });
    assert_eq!(
        outcome.failure,
        Some(Failure::Io(io::ErrorKind::PermissionDenied))
    );
    assert_eq!(outcome.close_error, Some(io::ErrorKind::Other));
    assert_eq!(transport.closes, 1);
    assert_eq!(outcome.transmitted_wire_bytes, 3);
    assert_eq!(transport.transmitted.len(), 3);
    assert!(outcome.requested_wire_bytes > outcome.transmitted_wire_bytes);
    assert!(outcome.identity.is_none());
}

#[test]
fn literal_query_all_chunk_sizes_and_partial_write_prefixes() {
    for echo in [false, true] {
        for chunk in 1..=54 {
            for max_write in [1, 3, usize::MAX] {
                let mut t = Mock::new(6);
                t.echo = echo;
                t.chunk = chunk;
                t.max_write = max_write;
                let out = Session::query(config()).unwrap().run(&mut t, || Ok(false));
                assert_eq!(
                    out.failure, None,
                    "echo={echo}, chunk={chunk}, write={max_write}"
                );
                assert_eq!(out.echo, if echo { Echo::Known } else { Echo::None });
                assert_eq!(t.transmitted, [254, 254, 238, 239, 224, 64, 117, 0, 0, 253]);
                assert_eq!(t.closes, 1);
                assert_eq!(out.identity.unwrap().tail[1], 6);
            }
        }
    }
}

#[test]
fn all_profiles_end_to_end_read_no_second_query_and_write_one_ack() {
    for selector in 1..=6 {
        for echo in [false, true] {
            let mut t = Mock::new(selector);
            t.echo = echo;
            let end_address = read_range(selector).unwrap().end;
            let mut count = 0;
            for a in (0..end_address).step_by(64) {
                let bytes: Vec<_> = (a..(a + 64).min(end_address))
                    .map(|n| (n.wrapping_mul(7) >> 3) as u8)
                    .collect();
                t.read_data.extend(packet(a, &bytes));
                count += 1;
            }
            t.read_data.extend(end(count));
            let out = Session::read(config(), profile(selector))
                .unwrap()
                .run(&mut t, || Ok(false));
            assert_eq!(out.failure, None, "selector {selector}, echo {echo}");
            assert_eq!(t.commands, [224, 226]); // E0/E1 -> E2/DATA/END, never E0 twice.
            assert_eq!(out.blocks, count as u32);
            assert_eq!(out.payload_bytes, u64::from(end_address));
            for a in 0..end_address as usize {
                assert!(out.acquired(a));
                assert_eq!(out.image[a], ((a as u32).wrapping_mul(7) >> 3) as u8);
            }
            assert!(!out.acquired(end_address as usize));
            assert_eq!(t.closes, 1);
            let image = vec![0x7f; IMAGE_CAPACITY];
            let mut t = Mock::new(selector);
            t.echo = echo;
            let out = Session::write(config(), profile(selector), 0, &image)
                .unwrap()
                .run(&mut t, || Ok(false));
            assert_eq!(out.failure, None, "write selector {selector}, echo {echo}");
            assert_eq!(out.write_disposition, WriteDisposition::Acknowledged);
            let plan = write_blocks_with_cap(selector, 0, 64).unwrap();
            assert_eq!(out.blocks, plan.len() as u32);
            assert_eq!(t.commands.first(), Some(&224));
            assert_eq!(t.commands[1], 227);
            assert_eq!(t.commands.last(), Some(&229));
            assert_eq!(t.commands.iter().filter(|&&c| c == 224).count(), 1);
            assert_eq!(t.closes, 1);
        }
    }
}

#[test]
fn read_failure_prefix_overlap_and_per_packet_coverage_length() {
    let mut t = Mock::new(1);
    t.read_data = packet(0, &[1, 2, 3, 4]);
    t.read_data.extend(packet(1, &[9, 8])); // Last-write wins; not strict overlap rejection.
    t.read_data.extend(packet(0x8000_0000, &[7, 7]));
    let out = Session::read(config(), profile(1))
        .unwrap()
        .run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::OutsideImage));
    assert_eq!(&out.image[..4], &[1, 9, 8, 4]);
    assert_eq!(out.blocks, 2);
    assert!(out.acquired(3));
    let mut t = Mock::new(1);
    t.read_data = packet(16, &[1, 2, 3, 4]);
    t.read_data.extend(packet(0, &[7, 8]));
    t.read_data.extend(end(2));
    let out = Session::read(config(), profile(1))
        .unwrap()
        .run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::MissingCoverage));
    assert_eq!(&out.image[16..20], &[1, 2, 3, 4]);
    assert!(!out.acquired(16));
    assert!(out.acquired(1));
    let mut t = Mock::new(1);
    t.read_data = packet(0, &[1, 2]);
    t.read_data.extend(end(2));
    let out = Session::read(config(), profile(1))
        .unwrap()
        .run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::EndCount));
    assert!(out.acquired(1));
}

#[test]
fn post_probe_read_sends_e2_without_a_second_e0() {
    for echo in [false, true] {
        let mut t = Mock::new(1);
        t.echo = echo;
        t.chunk = 7;
        let mut response = response(1);
        response[9 + 0x18] = 4;
        t.query_response = Some(response);
        let query = Session::query(config()).unwrap().run(&mut t, || Ok(false));
        assert_eq!(query.failure, None);
        t.commands.clear();
        let end_address = read_range(1).unwrap().end;
        let mut count = 0;
        for address in (0..end_address).step_by(64) {
            t.read_data.extend(packet(address, &[7; 64]));
            count += 1;
        }
        t.read_data.extend(end(count));
        let mut c = config();
        c.settings.data_preamble = 13;
        let read = Session::read_after_query(c, profile(1), &query).unwrap();
        assert_eq!(read.config.settings.data_preamble, 13);
        let out = read.run(&mut t, || Ok(false));
        assert_eq!(out.failure, None);
        assert_eq!(t.commands, [226]);
        assert_eq!(out.echo, query.echo);
        assert_eq!(out.identity, query.identity);
        assert_eq!(out.payload_bytes, u64::from(end_address));
        assert_eq!(t.closes, 2);
    }
}

#[test]
fn post_probe_transfers_reject_failed_edited_and_mismatched_query_outcomes() {
    let image = vec![0; IMAGE_CAPACITY];
    let reject = |c: SessionConfig, p: Profile, query: &Outcome| {
        assert!(Session::read_after_query(c.clone(), p, query).is_err());
        assert!(Session::write_after_query(c, p, 0, &image, query).is_err());
    };
    let mut t = Mock::new(1);
    let mut query = Session::query(config()).unwrap().run(&mut t, || Ok(false));
    reject(config(), profile(2), &query);
    let mut changed = config();
    changed.expected_context = Some(ContextFields([1; 12]));
    changed.flag57 = 1;
    reject(changed.clone(), profile(1), &query);
    changed.flag57 = 2;
    assert!(Session::write_after_query(changed, profile(1), 0, &image, &query).is_ok());
    query.identity.as_mut().unwrap().tail[1] = 2;
    reject(config(), profile(2), &query);
    query.identity.as_mut().unwrap().tail[1] = 1;
    query.echo = Echo::Known;
    reject(config(), profile(1), &query);
    query.echo = Echo::None;
    query.identity_gate.cached_tail.as_mut().unwrap()[1] = 2;
    reject(config(), profile(1), &query);
    query.identity_gate.cached_tail.as_mut().unwrap()[1] = 1;
    query.terminal_phase = State::Closed;
    reject(config(), profile(1), &query);
    query.terminal_phase = State::Drain;
    query.operation = Operation::Read;
    reject(config(), profile(1), &query);

    let mut t = Mock::new(1);
    t.close_failure = true;
    let mut failed = Session::query(config()).unwrap().run(&mut t, || Ok(false));
    failed.failure = None;
    failed.close_error = None;
    reject(config(), profile(1), &failed);
    let mut t = Mock::new(1);
    t.query_response = Some(Vec::new());
    let mut failed = Session::query(config()).unwrap().run(&mut t, || Ok(false));
    failed.failure = None;
    reject(config(), profile(1), &failed);
}

#[test]
fn post_probe_write_sends_e3_and_adopts_the_probe_preamble() {
    let image = vec![0; IMAGE_CAPACITY];
    for echo in [false, true] {
        for flag in [0, 4] {
            let mut t = Mock::new(6);
            t.echo = echo;
            t.chunk = 7;
            t.max_write = 3;
            let mut response = response(6);
            response[9 + 0x18] = flag;
            t.query_response = Some(response);
            let query = Session::query(config()).unwrap().run(&mut t, || Ok(false));
            assert_eq!(query.failure, None);
            let sent = t.transmitted.len();
            t.commands.clear();
            let mut c = config();
            c.settings.extra_preamble = 5;
            c.settings.data_preamble = 13;
            let out = Session::write_after_query(c, profile(6), 0, &image, &query)
                .unwrap()
                .run(&mut t, || Ok(false));
            assert_eq!(out.failure, None);
            assert_eq!(t.commands.first(), Some(&227));
            assert!(!t.commands.contains(&224));
            assert_eq!(out.identity, query.identity);
            assert_eq!(out.echo, query.echo);
            assert_eq!(out.write_disposition, WriteDisposition::Acknowledged);
            let frames: Vec<_> = t.transmitted[sent..]
                .split_inclusive(|b| *b == 253)
                .collect();
            let preambles = |frame: &[u8]| frame.iter().take_while(|b| **b == 254).count();
            assert_eq!(preambles(frames[0]), 7);
            assert_eq!(preambles(frames[1]), if flag == 4 { 10 } else { 2 });
            assert_eq!(preambles(frames.last().unwrap()), 2);
            assert_eq!(
                out.transmitted_wire_bytes,
                (t.transmitted.len() - sent) as u64
            );
            assert_eq!(t.closes, 2);
        }
    }
}

#[test]
fn post_probe_write_failure_keeps_its_prefix_and_closes_once() {
    let image = vec![0; IMAGE_CAPACITY];
    for cancel in [false, true] {
        let mut t = Mock::new(6);
        let query = Session::query(config()).unwrap().run(&mut t, || Ok(false));
        let sent = t.transmitted.len();
        t.fail_after = Some(sent + 1);
        t.close_failure = true;
        let out = Session::write_after_query(config(), profile(6), 0, &image, &query)
            .unwrap()
            .run(&mut t, || Ok(cancel));
        assert_eq!(out.close_error, Some(io::ErrorKind::Other));
        assert_eq!(t.closes, 2);
        if cancel {
            assert_eq!(out.failure, Some(Failure::Cancelled));
            assert_eq!(out.write_disposition, WriteDisposition::NotStarted);
            assert_eq!(t.transmitted.len(), sent);
        } else {
            assert_eq!(out.failure, Some(Failure::Io(io::ErrorKind::BrokenPipe)));
            assert_eq!(out.write_disposition, WriteDisposition::Unconfirmed);
            assert_eq!(out.transmitted_wire_bytes, 1);
            assert_eq!(t.transmitted.len(), sent + 1);
        }
    }
}

#[test]
fn timeout_truncation_echo_nak_early_ack_and_deadline_priority() {
    let mut t = Mock::new(6);
    t.query_response = Some(vec![]);
    let out = Session::query(config()).unwrap().run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::Timeout));
    assert_eq!(t.closes, 1);
    let mut t = Mock::new(6);
    t.query_response = Some(vec![254, 254, 239]);
    // Auto cannot decide on a prefix shorter than the expected echo.
    let mut c = config();
    c.settings.echo = Echo::None;
    let out = Session::query(c).unwrap().run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::Truncated));
    let mut t = Mock::new(6);
    let mut c = config();
    c.settings.echo = Echo::Known;
    let out = Session::query(c).unwrap().run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::EchoMismatch));
    let image = vec![0; IMAGE_CAPACITY];
    let mut t = Mock::new(6);
    t.early_ack = true;
    let out = Session::write(config(), profile(6), 0, &image)
        .unwrap()
        .run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::EarlyResponse));
    assert_eq!(out.blocks, 0);
    let mut t = Mock::new(6);
    t.ack = Some(vec![254, 254, 239, 238, 230, 9, 253]);
    let out = Session::write(config(), profile(6), 0, &image)
        .unwrap()
        .run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::Protocol(Error::Rejected(9))));
    assert_eq!(out.write_disposition, WriteDisposition::Unconfirmed);
    let mut t = Mock::new(6);
    t.ack_delay = 10_000;
    let out = Session::write(config(), profile(6), 0, &image)
        .unwrap()
        .run(&mut t, || Ok(false));
    assert_eq!(out.failure, None);
    assert_eq!(out.write_disposition, WriteDisposition::Acknowledged);
    let mut t = Mock::new(6);
    t.stuck_after_ack = true;
    let out = Session::write(config(), profile(6), 0, &image)
        .unwrap()
        .run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::DrainTimeout));
    assert_eq!(t.closes, 1);
}

#[test]
fn cancel_io_and_close_preserve_uncertain_write_without_retry() {
    let image = vec![0; IMAGE_CAPACITY];
    let mut t = Mock::new(6);
    t.fail_after = Some(25);
    t.max_write = 2;
    let out = Session::write(config(), profile(6), 0, &image)
        .unwrap()
        .run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::Io(io::ErrorKind::BrokenPipe)));
    assert_eq!(out.transmitted_wire_bytes, 25);
    assert_eq!(t.transmitted.len(), 25);
    assert_eq!(out.write_disposition, WriteDisposition::Unconfirmed);
    assert_eq!(out.blocks, 1);
    assert_eq!(out.next_address, 64);
    assert_eq!(t.closes, 1);
    let mut t = Mock::new(6);
    let sent = t.shared_sent.clone();
    let out = Session::write(config(), profile(6), 0, &image)
        .unwrap()
        .run(&mut t, || Ok(sent.get() > 20));
    assert_eq!(out.failure, Some(Failure::Cancelled));
    assert_eq!(t.closes, 1);
    assert!(!t.commands.contains(&229));
    assert_eq!(out.write_disposition, WriteDisposition::Unconfirmed);
    let mut t = Mock::new(6);
    t.close_failure = true;
    let out = Session::query(config()).unwrap().run(&mut t, || Ok(true));
    assert_eq!(out.failure, Some(Failure::Cancelled));
    assert_eq!(out.close_error, Some(io::ErrorKind::Other));
    assert!(t.transmitted.is_empty());
    assert_eq!(t.closes, 1);
    let mut t = Mock::new(6);
    t.fail_read = true;
    let out = Session::query(config()).unwrap().run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::Io(io::ErrorKind::NotConnected)));
    assert_eq!(t.closes, 1);
    let mut t = Mock::new(6);
    t.zero_write = true;
    let out = Session::query(config()).unwrap().run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::Io(io::ErrorKind::WriteZero)));
}

#[test]
fn source_query_invalid_bcd_still_stores_raw_fields_and_upper_failure_prefix() {
    let mut s = Session::query(config()).unwrap();
    let mut p = vec![254, 239, 238, 225, 0x4a, 0x75, 0, 1];
    p.extend_from_slice(&[33, 6]);
    p.extend_from_slice(&[0; 31]);
    assert_eq!(s.query_response(&p), Err(Failure::Identity(5)));
    assert_eq!(s.out.identity.as_ref().unwrap().model, 0x4a75);
    assert_eq!(s.out.identity.as_ref().unwrap().tail.len(), 33);
    assert_eq!(s.out.identity_gate.cached_model, None);
    let mut c = config();
    c.block_cap = 16;
    c.set_block_cap(0);
    assert_eq!(c.block_cap, 16);
    c.set_block_cap(64);
    c.set_block_cap(-1);
    assert_eq!(c.block_cap, 64);
    let mut c = config();
    c.block_cap = 256;
    assert!(Session::query(c).is_err());
}

#[test]
fn startup_script_sends_settings_drains_and_preserves_failure_prefix() {
    let mut t = Mock::new(6);
    let mut c = config();
    c.startup_script = b"FD(115200,N,8,1)FD(38400,N,8,1)".to_vec();
    let out = Session::query(c.clone()).unwrap().run(&mut t, || Ok(false));
    assert_eq!(out.failure, None);
    assert_eq!(
        t.configurations,
        [b"115200,N,8,1".to_vec(), b"38400,N,8,1".to_vec()]
    );
    assert_eq!(&t.transmitted[..2], &[253, 253]);
    assert_eq!(t.commands, [224]);
    assert_eq!(out.requested_wire_bytes, 12);
    assert_eq!(t.closes, 1);
    let mut t = Mock::new(6);
    t.reconfigure_failure = true;
    let out = Session::query(c).unwrap().run(&mut t, || Ok(false));
    assert_eq!(
        out.failure,
        Some(Failure::Reconfigure(io::ErrorKind::InvalidInput))
    );
    assert_eq!(out.result_code(), 2);
    assert_eq!(t.transmitted, [253]);
    assert_eq!(t.closes, 1);
}

#[test]
fn write_preamble_is_tail_bit2_and_end_never_has_extra_fe() {
    let mut t = Mock::new(6);
    let mut r = response(6);
    r[9 + 0x18] = 4;
    t.query_response = Some(r);
    let mut c = config();
    c.settings.extra_preamble = 12;
    let out = Session::write(c, profile(6), 0, &vec![0; IMAGE_CAPACITY])
        .unwrap()
        .run(&mut t, || Ok(false));
    assert_eq!(out.failure, None);
    let mut frames = t.transmitted.split_inclusive(|&b| b == 253);
    for command in [224, 227, 228] {
        let frame = frames.next().unwrap();
        let n = frame.iter().take_while(|&&b| b == 254).count();
        assert_eq!(n, if command == 228 { 10 } else { 14 });
        assert_eq!(frame[n + 2], command);
    }
    let end = t.transmitted.rsplit(|&b| b == 253).nth(1).unwrap();
    assert_eq!(end.iter().take_while(|&&b| b == 254).count(), 2);
}

#[test]
fn outside_required_coverage_is_detected_after_image_copy_and_source_bounds_are_not_status6() {
    let mut t = Mock::new(1);
    let a = read_range(1).unwrap().end;
    t.read_data = packet(a, &[7, 8]);
    t.read_data.extend(end(1));
    let out = Session::read(config(), profile(1))
        .unwrap()
        .run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::OutsideCoverage));
    assert_eq!(&out.image[a as usize..a as usize + 2], &[7, 8]);
    assert!(out.acquired(a as usize));
    let mut s = Session::read(config(), profile(1)).unwrap();
    // Repeated FF does not add decoded bytes; original vector[index4] would
    // throw, rather than returning packet status 6. Native fails explicitly.
    let p = [254, 239, 238, 228, 255, 255, 255, 255, 255, 255, 255, 255];
    assert_eq!(s.read_packet(&p), Err(Failure::SourceBounds));
    assert_eq!(Failure::SourceBounds.result_code(), 255);
    let p = packet(u32::MAX, &[1, 2]);
    assert_eq!(
        s.read_packet(&p[1..p.len() - 1]),
        Err(Failure::OutsideImage)
    );
}

#[test]
fn last_ack_in_one_read_wins_but_late_ack_after_close_is_not_accepted() {
    let image = vec![0; IMAGE_CAPACITY];
    let mut t = Mock::new(6);
    t.ack = Some(vec![
        254, 254, 239, 238, 230, 1, 253, 254, 254, 239, 238, 230, 0, 253,
    ]);
    let out = Session::write(config(), profile(6), 0, &image)
        .unwrap()
        .run(&mut t, || Ok(false));
    assert_eq!(out.failure, None);
    let mut t = Mock::new(6);
    t.ack_delay = 10_001;
    let out = Session::write(config(), profile(6), 0, &image)
        .unwrap()
        .run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::Timeout));
    assert_eq!(t.closes, 1);
    assert_eq!(out.write_disposition, WriteDisposition::Unconfirmed);
    let n = t.transmitted.len();
    t.now += 1;
    let mut bytes = [0; 32];
    assert_eq!(t.read(&mut bytes).unwrap(), 7);
    assert_eq!(t.transmitted.len(), n); // No engine exists to credit a late event or retry.
    let mut t = Mock::new(6);
    t.query_response = Some(vec![]);
    let mut c = config();
    c.settings.echo = Echo::Known;
    let out = Session::query(c).unwrap().run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::EchoTimeout));
    let mut t = Mock::new(6);
    t.ack = Some(vec![254, 254, 238, 239, 230, 0, 253]);
    let out = Session::write(config(), profile(6), 0, &image)
        .unwrap()
        .run(&mut t, || Ok(false));
    assert_eq!(out.failure, Some(Failure::MalformedAck));
    assert_eq!(out.result_code(), 10);
}
