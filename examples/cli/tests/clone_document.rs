//! Native clone gates and real CLI consumers. No device/OS port is opened.
use cs705::{
    clone::{self, Direction, Frame, Identity, SessionConfig, Transport},
    icf::{
        self,
        document::*,
        normalizer::ResetState,
        record::{self, ChannelField},
        record_image,
    },
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

fn source_fields() -> Vec<u8> {
    let mut m = vec![0; 0x3c9c0];
    m[0x355a1] = 6;
    for g in 0..100 {
        let a = 0x355c0 + 19 * g;
        m[a] = if g == 99 { 255 } else { g as u8 + 1 };
        m[a + 1..a + 3].fill(255);
        m[a + 3..a + 19].fill(b' ');
    }
    for r in [
        0x35d2c..0x36628,
        0x3ab82..0x3bf6e,
        0x3c0a7..0x3c2ff,
        0x3c385..0x3c613,
    ] {
        m[r].fill(255);
    }
    m[0x3662d..0x36821].fill(1);
    m[0x395ad..0x3969d].fill(b' ');
    for i in 0..45 {
        m[0x368eb + 36 * i..0x368ef + 36 * i].fill(255);
    }
    for i in 0..11 {
        m[0x36f54 + 36 * i..0x36f58 + 36 * i].fill(255);
    }
    m[0xa60b..0xa60f].fill(255);
    m[0x3c84b] = 255;
    m[0x3c8b9..0x3c8bb].fill(255);
    m[0x399c1..0x399dd].copy_from_slice(b"0123456789ABCDEFGHIJKLMNOPQR");
    m[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    m
}
fn fixture() -> (Session, Document) {
    let mut raw = [0; 24];
    raw[0] = 1;
    let mut session = Session::new(icf::context::Context::from_bytes(raw), None, 1, [1, 0]);
    session
        .regenerate_defaults(
            DefaultSource {
                selector: 1,
                prefix: &source_fields(),
                japanese_mode_pairs: None,
                restricted_band_stack: None,
            },
            &vec![0; clone::IMAGE_CAPACITY],
        )
        .unwrap();
    let doc = Document::from_current(
        session.defaults().unwrap().to_vec(),
        b"ORIGINAL".to_vec(),
        6,
    );
    (session, doc)
}
fn set_call(doc: &mut Document, hz: u32) {
    let mut image = doc.current().to_vec();
    let mut h = record::reset_holder(3, 0, 1);
    h.set_channel_field(ChannelField::FrequencyHz, hz);
    h.set_mode(13);
    record_image::store_into(&mut image, 3, 0, &mut h).unwrap();
    doc.replace_current(image);
}
fn call(image: &[u8]) -> u32 {
    let mut h = record::reset_holder(3, 0, 1);
    record_image::load_into(image, 3, 0, &mut h).unwrap();
    h.channel_field(ChannelField::FrequencyHz)
}
fn view() -> ValidationView {
    ValidationView {
        kind: 3,
        group: 100,
        first: 0,
        last: 0,
        count: 1,
        include_side1: true,
        show_both: 1,
    }
}
fn controls() -> CloneControls {
    CloneControls {
        controller_busy: 0,
        wrapper_state: 0,
        form_flag57: 1,
    }
}
fn reset() -> ResetState {
    ResetState {
        show_both: 1,
        home_invalidated: 0xa7,
    }
}
fn config() -> SessionConfig {
    let mut c = SessionConfig::default();
    c.settings.echo = clone::Echo::None;
    c
}
fn operation() -> CloneWrite {
    CloneWrite::new(config(), 0, reset()).unwrap()
}

fn read_events(image: &[u8], selector: u8) -> Vec<Value> {
    // Document read owns the sole identity probe; its read session starts at E2.
    let mut events = query_events(&[identity(selector)]);
    events.push(json!({"write_hex":hex(&Frame::Read.encode(Direction::ToRadio).unwrap())}));
    let end = clone::read_range(selector).unwrap().end as usize;
    let mut blocks = 0u8;
    for (index, bytes) in image[..end].chunks(128).enumerate() {
        events.push(json!({"read_hex":hex(&Frame::Data { address: (index*128) as u32, payload: bytes.to_vec() }.encode(Direction::FromRadio).unwrap())}));
        blocks = blocks.wrapping_add(1);
    }
    events.push(json!({"read_hex":hex(&Frame::End { block_count: blocks }.encode(Direction::FromRadio).unwrap())}));
    events
}

#[test]
fn logic_phase_initial_lowbyte_saved_bl_and_native_read_callbacks() {
    let (mut session, mut doc) = fixture();
    let old = doc.clone();
    let mut initial = InitialState::default();
    let mut id = identity(6);
    id.tail[0x13] = 0;
    doc.receive_initial_identity(
        &mut session,
        &mut initial,
        &id,
        InitialRequest {
            mode: 2,
            notification: 0x101,
            additional: 0,
        },
        None,
        |_, _, _| panic!("notification must not adopt"),
    )
    .unwrap();
    assert_eq!(doc, old);
    assert_eq!(session.context.fields().0[0], 1);
    assert_eq!(initial.saved_context.unwrap().0[0], 0);
    assert_eq!(initial.saved_selector, 6);

    let fields = source_fields();
    let source = || DefaultSource {
        selector: 1,
        prefix: &fields,
        japanese_mode_pairs: None,
        restricted_band_stack: None,
    };
    let mut doc = Document::from_current(old.current().to_vec(), Vec::new(), 5);
    doc.begin_pending_name(9, b"LIVE".to_vec());
    let mut boundaries = Vec::new();
    doc.receive_initial_identity(
        &mut session,
        &mut initial,
        &identity(6),
        InitialRequest {
            mode: 2,
            notification: 0x100,
            additional: 0,
        },
        Some(source()),
        |event, doc, _| {
            boundaries.push(event);
            if event == InitialBoundary::Sync(0x25) {
                doc.patch_current(&[(0x49979, &[0xa7])])?;
                assert_eq!(doc.compare_dirty(), 1);
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(doc.current(), session.defaults().unwrap());
    assert_eq!(doc.baseline(), doc.current());
    assert_eq!(doc.dirty(), 0);
    assert_eq!(doc.target_revision(), 5);
    assert!(doc.pending_matches(9));
    assert_eq!(boundaries.last(), Some(&InitialBoundary::Title));

    let baseline = doc.baseline().to_vec();
    let mut received = fields.clone();
    received.resize(clone::IMAGE_CAPACITY, 0);
    received[0x399a2] = 0xa7;
    let mut wire = Wire::new(read_events(&received, 6));
    let mut read = CloneRead::new(config(), 6).unwrap();
    let mut reply = CloneReadReply::Continue;
    loop {
        match read.advance(&session, controls(), reply).unwrap() {
            CloneReadStep::Query(query) => {
                reply = CloneReadReply::Probe(query.run(&mut wire, || false))
            }
            CloneReadStep::Sync(_) => reply = CloneReadReply::Continue,
            CloneReadStep::Read(read) => {
                reply = CloneReadReply::Read(read.run(&mut wire, || false))
            }
            CloneReadStep::Apply => break,
            CloneReadStep::Stopped(stop) => {
                assert_eq!(stop, CloneReadStop::Completed);
                break;
            }
        }
    }
    let outcome = read.outcomes.last().unwrap();
    assert_eq!(outcome.result_code(), 0);
    assert_eq!(
        read.finish_document(
            &mut doc,
            &mut session,
            Some(source()),
            ValidationSource::NoTree,
            |_, _, _| Ok(())
        )
        .unwrap(),
        CloneReadStop::Completed
    );
    assert_eq!(doc.current()[0x399a2], 0xa7);
    assert_eq!(doc.baseline(), baseline);
    assert_eq!(doc.dirty(), 1);
    assert_eq!(doc.target_revision(), 6);
    assert!(doc.path().is_empty());
    assert!(doc.pending_matches(9));
    wire.finish();

    set_call(&mut doc, 1);
    let before = doc.clone();
    let mut bad = identity(6);
    bad.revision = 2;
    let mut wire = Wire::new(query_events(&[bad]));
    let failed = clone::Session::read(config(), 6.try_into().unwrap())
        .unwrap()
        .run(&mut wire, || false);
    let mut events = Vec::new();
    let result = doc
        .receive_clone_read(
            &mut session,
            &read.initial,
            6,
            &failed,
            None,
            ValidationSource::Views(&[view()]),
            |event, _, _| {
                events.push(event);
                Ok(())
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(result.errors, 1);
    assert_eq!(result.flags, [12]);
    assert_eq!(events, [InitialBoundary::Sync(0x11)]);
    assert_eq!(doc, before);
    wire.finish();

    received[0x355a1] = 0;
    let mut wire = Wire::new(read_events(&received, 6));
    let mut rejected = CloneRead::new(config(), 6).unwrap();
    let mut reply = CloneReadReply::Continue;
    loop {
        match rejected.advance(&session, controls(), reply).unwrap() {
            CloneReadStep::Query(query) => {
                reply = CloneReadReply::Probe(query.run(&mut wire, || false))
            }
            CloneReadStep::Read(read) => {
                reply = CloneReadReply::Read(read.run(&mut wire, || false))
            }
            CloneReadStep::Sync(_) => reply = CloneReadReply::Continue,
            CloneReadStep::Apply => break,
            _ => panic!("unexpected image validation stop"),
        }
    }
    assert_eq!(rejected.outcomes.last().unwrap().result_code(), 15);
    assert_eq!(
        rejected
            .finish_document(
                &mut doc,
                &mut session,
                None,
                ValidationSource::Views(&[view()]),
                |_, _, _| Ok(())
            )
            .unwrap(),
        CloneReadStop::ReadFailed
    );
    assert_eq!(rejected.validation.unwrap().errors, 1);
    assert_eq!(doc, before);
    wire.finish();
}

#[test]
fn logic_phase_option_producer_and_saved_q_are_live_not_numeric_snapshots() {
    let mut options = CloneOptions::open(0xa1);
    assert_eq!(options.word, 0xa0);
    options.initialize(0).unwrap();
    options.word |= 2;
    options.initialize(1).unwrap();
    assert_eq!(options.states[..2], [0, 1]);
    options.states = [1, 2, 1, 0, 1];
    assert!(!options.press(CloneOptionButton::Cancel).unwrap());
    assert_eq!(options.word, 0xb5);
    assert_eq!(options.mask(), 21);
    let error = options.save(CloneOptionButton::Ok, |index, state| {
        if index == 2 {
            return Err(icf::Error::Invalid {
                line: 0,
                reason: "synthetic checkbox failure",
            });
        }
        if index == 1 {
            assert_eq!(state.word & 1, 0);
        }
        Ok(0x100)
    });
    assert!(error.is_err());
    assert_eq!(options.word, 0xb4);

    for missing in [false, true] {
        let (mut session, mut doc) = fixture();
        let mut op = operation();
        let mut saved = InitialState::default();
        doc.receive_initial_identity(
            &mut session,
            &mut saved,
            &identity(6),
            InitialRequest {
                mode: 2,
                notification: 1,
                additional: 0,
            },
            None,
            |_, _, _| panic!("saved notification cannot adopt"),
        )
        .unwrap();
        op.saved_context = saved.saved_context;
        let mut wire = Wire::new(query_events(&[identity(6)]));
        let mut reply = CloneReply::Continue;
        loop {
            match op
                .advance(
                    &mut doc,
                    &session,
                    controls(),
                    ValidationSource::NoTree,
                    reply,
                )
                .unwrap()
            {
                CloneStep::Sync(_) => reply = CloneReply::Continue,
                CloneStep::Query(q) => reply = CloneReply::Probe(q.run(&mut wire, || false)),
                CloneStep::Confirm { .. } => break,
                _ => panic!("unexpected phase"),
            }
        }
        op.fd = 1;
        session.context.decode_packed(0);
        if missing {
            op.saved_context = None;
        }
        let result = op.advance(
            &mut doc,
            &session,
            controls(),
            ValidationSource::NoTree,
            CloneReply::Confirmation(1),
        );
        if missing {
            assert!(result.is_err());
        } else {
            let CloneStep::Query(q) = result.unwrap() else {
                panic!("expected saved-Q query")
            };
            let mut second = Wire::new(query_events(&[identity(6)]));
            assert_eq!(q.run(&mut second, || false).result_code(), 0);
            second.finish();
        }
        wire.finish();
    }
}

#[test]
fn logic_phase_read_mismatch_saves_then_reprobes_and_keeps_callback_failure_prefix() {
    let (mut session, mut doc) = fixture();
    session.context.decode_packed(0);
    let old = doc.clone();
    let old_g = session.defaults().unwrap().to_vec();
    let mut image = source_fields();
    image.resize(clone::IMAGE_CAPACITY, 0);
    image[0x399a2] = 0xa7;
    let mut events = query_events(&[identity(6)]);
    events.extend(read_events(&image, 6));
    let mut wire = Wire::new(events);
    let mut read = CloneRead::new(config(), 6).unwrap();
    let mut reply = CloneReadReply::Continue;
    loop {
        match read.advance(&session, controls(), reply).unwrap() {
            CloneReadStep::Query(query) => {
                reply = CloneReadReply::Probe(query.run(&mut wire, || false))
            }
            CloneReadStep::Read(read) => {
                reply = CloneReadReply::Read(read.run(&mut wire, || false))
            }
            CloneReadStep::Sync(0x2a) => {
                let id = read.outcomes.last().unwrap().identity.as_ref().unwrap();
                let request = InitialRequest::information(&session, id, 1)
                    .unwrap()
                    .unwrap();
                doc.receive_initial_identity(
                    &mut session,
                    &mut read.initial,
                    id,
                    request,
                    None,
                    |_, _, _| panic!("saved notification cannot adopt"),
                )
                .unwrap();
                assert_eq!(session.context.fields().0[0], 0);
                assert_eq!(session.defaults().unwrap(), old_g);
                assert_eq!(doc, old);
                reply = CloneReadReply::Continue;
            }
            CloneReadStep::Apply => break,
            _ => panic!("unexpected read phase"),
        }
    }
    assert_eq!(
        (read.fc, read.fd, read.selector, read.initial.saved_selector),
        (0, 1, 6, 6)
    );
    assert_eq!(read.outcomes.len(), 3);
    let fields = source_fields();
    let result = read.finish_document(
        &mut doc,
        &mut session,
        Some(DefaultSource {
            selector: 1,
            prefix: &fields,
            japanese_mode_pairs: None,
            restricted_band_stack: None,
        }),
        ValidationSource::NoTree,
        |event, _, _| {
            if event == InitialBoundary::Title {
                return Err(icf::Error::Invalid {
                    line: 0,
                    reason: "synthetic title failure",
                });
            }
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(session.context.fields().0[0], 1);
    assert_eq!(doc.current()[0x399a2], 0xa7);
    assert_eq!(doc.dirty(), 1);
    assert_eq!(doc.target_revision(), 6);
    assert_eq!(doc.path(), old.path());
    assert_eq!(doc.baseline(), old.baseline());
    wire.finish();
}
fn identity(selector: u8) -> Identity {
    let mut tail = vec![0; 33];
    tail[0] = 33;
    tail[1] = selector;
    tail[0x13] = 8;
    Identity {
        model: 0x4075,
        revision: 1,
        tail,
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}
fn query_events(ids: &[Identity]) -> Vec<Value> {
    ids.iter().flat_map(|id|[
        json!({"write_hex":"FEFEEEEFE040750000FD"}),
        json!({"read_hex":hex(&Frame::Response(id.clone()).encode(Direction::FromRadio).unwrap())})
    ]).collect()
}

// One event deque spans all probe generations. Each low-level Session close is
// observable; only finish checks the sequence's unconsumed-event guard.
struct Wire {
    events: VecDeque<Value>,
    now: u64,
    closes: usize,
}
impl Wire {
    fn new(events: Vec<Value>) -> Self {
        Self {
            events: events.into(),
            now: 0,
            closes: 0,
        }
    }
    fn finish(&self) {
        assert!(
            self.events.is_empty(),
            "{} remaining events",
            self.events.len()
        );
    }
}
fn bytes(value: &Value, key: &str) -> Vec<u8> {
    let s = value[key].as_str().unwrap();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
impl Read for Wire {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let Some(event) = self.events.front() else {
            return Ok(0);
        };
        if event.get("read_hex").is_none() {
            return Ok(0);
        }
        let data = bytes(event, "read_hex");
        assert!(out.len() >= data.len());
        out[..data.len()].copy_from_slice(&data);
        self.events.pop_front();
        Ok(data.len())
    }
}
impl Write for Wire {
    fn write(&mut self, out: &[u8]) -> std::io::Result<usize> {
        let e = self.events.pop_front().expect("unexpected send");
        if e.get("write_error").is_some() {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        assert_eq!(bytes(&e, "write_hex"), out);
        Ok(out.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl Transport for Wire {
    fn now_ms(&self) -> u64 {
        self.now
    }
    fn wait_ms(&mut self, ms: u64) -> std::io::Result<()> {
        self.now += ms;
        Ok(())
    }
    fn pending_output(&mut self) -> std::io::Result<usize> {
        Ok(0)
    }
    fn close(&mut self) -> std::io::Result<()> {
        self.closes += 1;
        Ok(())
    }
    fn reconfigure(&mut self, _: &[u8]) -> std::io::Result<()> {
        panic!("no reconfiguration expected")
    }
}
fn run(
    op: &mut CloneWrite,
    doc: &mut Document,
    session: &Session,
    wire: &mut Wire,
    source: ValidationSource<'_>,
    response: i32,
    mut boundary: impl FnMut(u32, &mut Document, &mut CloneControls),
) -> CloneStop {
    let mut reply = CloneReply::Continue;
    let mut c = controls();
    loop {
        match op.advance(doc, session, c, source, reply).unwrap() {
            CloneStep::Sync(event) => {
                boundary(event, doc, &mut c);
                reply = CloneReply::Continue;
            }
            CloneStep::Query(q) => reply = CloneReply::Probe(q.run(wire, || false)),
            CloneStep::Confirm { .. } => reply = CloneReply::Confirmation(response),
            CloneStep::Write(w) => reply = CloneReply::Written(w.run(wire, || false)),
            CloneStep::Finalize => return op.finalize(&mut c, |_, _| Ok(())).unwrap(),
            CloneStep::Stopped(stop) => return stop,
        }
    }
}

#[test]
fn native_bit8_precedence_and_clone_response1_not_file_yes6() {
    let (session, mut doc) = fixture();
    set_call(&mut doc, 1);
    let mut wire = Wire::new(query_events(&[identity(6)]));
    let mut op = operation();
    assert_eq!(
        run(
            &mut op,
            &mut doc,
            &session,
            &mut wire,
            ValidationSource::Views(&[view()]),
            1,
            |_, _, _| {}
        ),
        CloneStop::InvalidItems
    );
    wire.finish();
    let v = op.validation.unwrap();
    assert_eq!((v.errors, v.warnings, v.flags), (1, 0, vec![12]));
    assert_eq!((op.saved_selector, op.fc, op.fd), (6, 1, 0));
    assert!(op.preparation.image.is_empty());
    for response in [0, 2, 6] {
        let mut op = operation();
        let mut wire = Wire::new(query_events(&[identity(6)]));
        assert_eq!(
            run(
                &mut op,
                &mut doc,
                &session,
                &mut wire,
                ValidationSource::NoTree,
                response,
                |_, _, _| {}
            ),
            CloneStop::Declined
        );
        wire.finish();
        assert_eq!(op.validation.unwrap().errors, 0);
        assert_eq!(op.saved_selector, 6);
        assert_eq!(op.fc, 1);
    }
}

#[test]
fn native_busy_compare_and_live_sync_boundaries_do_not_prepare_or_rollback() {
    for event in [0x1b, 0x2a, 0x11] {
        let (session, mut doc) = fixture();
        let original = doc.current().to_vec();
        let mut op = operation();
        let mut wire = Wire::new(if event == 0x1b {
            vec![]
        } else {
            query_events(&[identity(6)])
        });
        let stop = run(
            &mut op,
            &mut doc,
            &session,
            &mut wire,
            ValidationSource::NoTree,
            1,
            |e, doc, c| {
                if e == event {
                    doc.patch_current(&[(0x49979, &[0xa7])]).unwrap();
                    if e == 0x1b {
                        c.controller_busy = 1
                    } else {
                        c.wrapper_state = 1
                    }
                }
            },
        );
        assert_eq!(
            stop,
            if event == 0x1b {
                CloneStop::Busy
            } else {
                CloneStop::WrapperState
            }
        );
        wire.finish();
        assert_eq!(doc.current()[0x49979], 0xa7);
        assert_eq!(doc.baseline(), original);
        assert!(op.preparation.image.is_empty());
    }
    let (mut session, mut doc) = fixture();
    session.context.decode_packed(0);
    let mut op = operation();
    let mut wire = Wire::new(query_events(&[identity(6)]));
    assert_eq!(
        run(
            &mut op,
            &mut doc,
            &session,
            &mut wire,
            ValidationSource::NoTree,
            1,
            |_, _, _| {}
        ),
        CloneStop::ContextMismatch
    );
    wire.finish();
    assert_eq!(op.saved_selector, 6);
}

#[test]
fn actual_second_third_probe_errors_preserve_private_and_wire_prefix_and_dirty_baseline() {
    for failed_probe in [1, 2, 3] {
        let (session, mut doc) = fixture();
        set_call(&mut doc, 123_000_000);
        doc.compare_dirty();
        let baseline = doc.baseline().to_vec();
        let mut ids = vec![identity(6); failed_probe];
        ids[failed_probe - 1].revision = 2;
        let mut wire = Wire::new(query_events(&ids));
        let mut op = operation();
        let mut sync = vec![];
        assert_eq!(
            run(
                &mut op,
                &mut doc,
                &session,
                &mut wire,
                ValidationSource::Views(&[view()]),
                1,
                |event, _, _| sync.push(event)
            ),
            CloneStop::ProbeFailed
        );
        wire.finish();
        assert_eq!(op.outcomes.len(), failed_probe);
        assert_eq!(doc.baseline(), baseline);
        assert_eq!(doc.path(), b"ORIGINAL");
        assert_eq!(doc.dirty(), 1);
        if failed_probe == 3 {
            assert_eq!(sync, [0x1b, 0x2a, 0x38, 0x11]);
            assert!(op.preparation.metadata_complete);
            assert_eq!(call(doc.current()), 123_000_000);
            assert_eq!(call(&op.preparation.image), 0);
            assert_eq!(call(&op.wire_image), 0);
            // This kind 3 correction copies F; no typed/list deletion sets AL.
            assert_eq!(
                op.preparation.correction.unwrap().lists,
                icf::normalizer::ResetLists::default()
            );
            for (kind, first, last) in icf::normalizer::TYPED_SLOTS {
                if !matches!(kind, 2 | 8 | 0x15 | 0x18) {
                    continue;
                }
                for index in first..=last {
                    let mut before = record::reset_holder(kind, index, 1);
                    let mut after = record::reset_holder(kind, index, 1);
                    record_image::load_into(doc.current(), kind, index, &mut before).unwrap();
                    record_image::load_into(&op.preparation.image, kind, index, &mut after)
                        .unwrap();
                    assert!(
                        !before.valid() || after.valid(),
                        "unexpected typed deletion {kind}/{index}"
                    );
                }
            }
            assert_eq!(op.wire_image.len(), clone::IMAGE_CAPACITY);
        } else {
            assert!(op.preparation.image.is_empty());
            assert!(op.wire_image.is_empty());
        }
    }
}

#[test]
fn late_stage9_error_keeps_current_prefix_private_allocation_and_previous_wire_buffer() {
    let (mut session, mut doc) = fixture();
    let mut g = session.defaults().unwrap().to_vec();
    g[0x3a8ca] = 1;
    g[0x3c8b3] = 255;
    session.adopt_captured_defaults(g, 1).unwrap();
    let mut image = doc.current().to_vec();
    image[0x3a8ca] = 1;
    image[0x3c8b3] = 255;
    image[0x3c800] = 255;
    image[0x3c84a] = 255;
    image[0x3c9b0..0x3c9c0].fill(0xcc);
    doc.replace_current(image);
    doc.compare_dirty();
    let baseline = doc.baseline().to_vec();
    let mut op = operation();
    op.wire_image = vec![0xa7; clone::IMAGE_CAPACITY];
    let mut wire = Wire::new(query_events(&[identity(6), identity(6)]));
    let mut reply = CloneReply::Continue;
    loop {
        match op.advance(
            &mut doc,
            &session,
            controls(),
            ValidationSource::NoTree,
            reply,
        ) {
            Ok(CloneStep::Sync(_)) => reply = CloneReply::Continue,
            Ok(CloneStep::Query(q)) => reply = CloneReply::Probe(q.run(&mut wire, || false)),
            Ok(CloneStep::Confirm { .. }) => reply = CloneReply::Confirmation(1),
            Err(CloneError::Image(_)) => break,
            _ => panic!("preparation failure must not reach final probe/write"),
        }
    }
    wire.finish();
    assert_eq!(doc.current()[0x3c800] & 3, 0);
    assert_eq!(doc.current()[0x3c84a] & 7, 0);
    assert_eq!(&doc.current()[0x3c9b0..0x3c9c0], &[0xcc; 16]);
    assert_eq!(doc.baseline(), baseline);
    assert_eq!(doc.dirty(), 1);
    assert_eq!(op.preparation.stage, ClonePreparationStage::CurrentStage9);
    assert!(!op.preparation.metadata_complete);
    assert!(op.preparation.image.iter().all(|b| *b == 0));
    assert_eq!(op.wire_image, vec![0xa7; clone::IMAGE_CAPACITY]);
    assert!(
        op.advance(
            &mut doc,
            &session,
            controls(),
            ValidationSource::NoTree,
            CloneReply::Continue
        )
        .is_err()
    );
}

#[test]
fn actual_probe_selector_drives_all_six_shared_maprev_routes() {
    for selector in 1..=6 {
        let (session, mut doc) = fixture();
        let mut op = operation();
        let mut ids = vec![identity(selector); 3];
        ids[2].revision = 2;
        let mut wire = Wire::new(query_events(&ids));
        assert_eq!(
            run(
                &mut op,
                &mut doc,
                &session,
                &mut wire,
                ValidationSource::NoTree,
                1,
                |_, _, _| {}
            ),
            CloneStop::ProbeFailed
        );
        wire.finish();
        assert_eq!(op.saved_selector, selector);
        assert!(op.preparation.metadata_complete);
        let expected = clone::convert_image(
            &op.preparation.image,
            6.try_into().unwrap(),
            selector.try_into().unwrap(),
            Some(session.defaults().unwrap()),
        )
        .unwrap();
        assert_eq!(op.wire_image, expected);
    }
}

#[test]
fn actual_selector_context_gate_and_controller_busy_stop_at_callback_generation() {
    let (session, mut doc) = fixture();
    let mut op = operation();
    let mut wire = Wire::new(query_events(&[identity(6), identity(5)]));
    assert_eq!(
        run(
            &mut op,
            &mut doc,
            &session,
            &mut wire,
            ValidationSource::NoTree,
            1,
            |_, _, _| {}
        ),
        CloneStop::ProbeFailed
    );
    wire.finish();
    assert_eq!(op.outcomes[1].result_code(), 13);
    assert_eq!(op.copied_tail[1], 5);
    assert_eq!(op.saved_selector, 6);
    assert!(op.preparation.image.is_empty());
    let mut op = operation();
    let mut wire = Wire::new(query_events(&[identity(6)]));
    assert!(matches!(
        op.advance(
            &mut doc,
            &session,
            controls(),
            ValidationSource::NoTree,
            CloneReply::Continue
        )
        .unwrap(),
        CloneStep::Sync(0x1b)
    ));
    let CloneStep::Query(q) = op
        .advance(
            &mut doc,
            &session,
            controls(),
            ValidationSource::NoTree,
            CloneReply::Continue,
        )
        .unwrap()
    else {
        panic!()
    };
    let out = q.run(&mut wire, || false);
    let mut busy = controls();
    busy.controller_busy = 1;
    assert!(matches!(
        op.advance(
            &mut doc,
            &session,
            busy,
            ValidationSource::NoTree,
            CloneReply::Probe(out)
        )
        .unwrap(),
        CloneStep::Stopped(CloneStop::Busy)
    ));
    wire.finish();
    assert_eq!(op.saved_selector, 0);
    let mut op = operation();
    let mut wire = Wire::new(query_events(&[identity(6)]));
    let mut reply = CloneReply::Continue;
    loop {
        match op.advance(
            &mut doc,
            &session,
            controls(),
            ValidationSource::NoTree,
            reply,
        ) {
            Ok(CloneStep::Sync(_)) => reply = CloneReply::Continue,
            Ok(CloneStep::Query(q)) => reply = CloneReply::Probe(q.run(&mut wire, || false)),
            Ok(CloneStep::Confirm { .. }) => {
                op.fd = 1;
                reply = CloneReply::Confirmation(1);
            }
            Err(error) => {
                assert!(
                    error
                        .to_string()
                        .contains("saved clone comparison source unavailable")
                );
                break;
            }
            _ => panic!("missing saved-Q source must not silently compare live Q"),
        }
    }
    wire.finish();
    assert_eq!((op.fc, op.fd, op.saved_selector), (0, 1, 6));
}

static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn process_early_stop_checks_the_same_transcript_tail_after_each_probe() {
    let files = Files::new();
    let (session, doc) = fixture();
    for probe in 1..=3 {
        for leftover in [false, true] {
            let mut ids = vec![identity(6); probe];
            ids[probe - 1].revision = 2;
            let mut events = query_events(&ids);
            if leftover {
                events.push(json!({"write_hex":"A7"}));
            }
            let mock = files.write(
                &format!("probe-{probe}-tail-{leftover}.json"),
                &serde_json::to_vec(&json!({"events":events})).unwrap(),
            );
            let mut args = options(&files, &session, &doc, &mock);
            args.extend(["--no-tree".into(), "-o".into(), "-".into()]);
            let out = cli(&args, &[]);
            assert_eq!(out.status.code(), Some(5));
            assert!(out.stdout.is_empty());
            let r = report(&out);
            assert_eq!(r["status"], "ProbeFailed");
            assert_eq!(r["sessions"].as_array().unwrap().len(), probe);
            assert_eq!(r["sessions"][probe - 1]["result_code"], 14);
            assert_eq!(r["transmitted_wire_bytes"], 10 * probe);
            assert_eq!(r["transport_result_code"], if leftover { 17 } else { 14 });
            assert_eq!(
                r["close_error"],
                if leftover {
                    json!("InvalidData")
                } else {
                    Value::Null
                }
            );
            assert_eq!(r["metadata_complete"], probe == 3);
            assert_eq!(
                r["wire_image_bytes"],
                if probe == 3 { clone::IMAGE_CAPACITY } else { 0 }
            );
            assert_eq!(r["baseline_committed"], false);
            assert_eq!(fs::read(files.path("current.raw")).unwrap(), doc.current());
        }
    }
    for leftover in [false, true] {
        let events = if leftover {
            vec![json!({"write_hex":"A7"})]
        } else {
            vec![]
        };
        let mock = files.write(
            &format!("before-send-tail-{leftover}.json"),
            &serde_json::to_vec(&json!({"events":events})).unwrap(),
        );
        let mut args = options(&files, &session, &doc, &mock);
        args.extend([
            "--no-tree".into(),
            "--cancel-after-ms".into(),
            "0".into(),
            "-o".into(),
            "-".into(),
        ]);
        let out = cli(&args, &[]);
        assert_eq!(out.status.code(), Some(5));
        let r = report(&out);
        assert!(out.stdout.is_empty());
        assert_eq!(r["sessions"][0]["failure"], "Cancelled");
        assert_eq!(r["sessions"][0]["result_code"], 255);
        assert_eq!(r["transmitted_wire_bytes"], 0);
        assert_eq!(r["transport_result_code"], if leftover { 17 } else { 255 });
        assert_eq!(
            r["close_error"],
            if leftover {
                json!("InvalidData")
            } else {
                Value::Null
            }
        );
    }
}

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "clone-document-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.path(name);
        fs::write(&p, bytes).unwrap();
        p
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}
fn cli(args: &[String], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cs705"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}
fn report(out: &Output) -> Value {
    serde_json::from_slice(&out.stderr)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&out.stderr)))
}
fn options(files: &Files, session: &Session, doc: &Document, mock: &Path) -> Vec<String> {
    let current = files.write("current.raw", doc.current());
    let g = files.write("captured-g.raw", session.defaults().unwrap());
    let raw = hex(session.context.as_bytes());
    let ctx = files.write(
        "context.json",
        &serde_json::to_vec(&json!({"generation_q_hex":raw,"live_q_hex":raw})).unwrap(),
    );
    [
        "clone",
        "write",
        "--current",
        p(&current),
        "--range-context",
        p(&ctx),
        "--generation",
        p(&g),
        "--generation-selector",
        "1",
        "--previous-reload-guard",
        "1",
        "--document-flags",
        "0100",
        "--show-both",
        "1",
        "--home-cache",
        "167",
        "--mock",
        p(mock),
        "--echo",
        "none",
        "--yes",
    ]
    .map(str::to_owned)
    .to_vec()
}
fn complete_events(session: &Session, doc: &Document, mask: u8) -> Vec<Value> {
    let mut doc = doc.clone();
    let mut prep = ClonePreparation::default();
    prep.prepare(&mut doc, session, &mut reset()).unwrap();
    // The third document probe authorizes E3; the write session does not probe again.
    let mut events = query_events(&vec![identity(6); 3]);
    events.push(json!({"write_hex":hex(&Frame::Write.encode(Direction::ToRadio).unwrap())}));
    let blocks = clone::write_blocks(6, mask).unwrap();
    for r in &blocks {
        events.push(json!({"write_hex":hex(&Frame::Data{address:r.start,payload:prep.image[r.start as usize..r.end as usize].to_vec()}.encode(Direction::ToRadio).unwrap())}));
    }
    events.push(json!({"write_hex":hex(&Frame::End{block_count:blocks.len() as u8}.encode(Direction::ToRadio).unwrap())}));
    events.push(
        json!({"read_hex":hex(&Frame::Acknowledgement.encode(Direction::FromRadio).unwrap())}),
    );
    events
}

#[test]
fn logic_phase_write_final_callback_releases_live_mask_only_after_normal_return() {
    for mode in 0..4 {
        let (session, mut doc) = fixture();
        let original = doc.clone();
        let mut op = operation();
        let mut options = CloneOptions::open(0);
        for index in 0..5 {
            options.initialize(index).unwrap();
        }
        assert!(options.press(CloneOptionButton::Ok).unwrap());
        op.options = Some(options);
        let events = if mode == 3 {
            let mut events = query_events(&[identity(6), identity(6), identity(6)]);
            events.push(json!({"write_error":true}));
            events
        } else {
            complete_events(&session, &doc, 21)
        };
        let mut wire = Wire::new(events);
        let mut reply = CloneReply::Continue;
        loop {
            match op
                .advance(
                    &mut doc,
                    &session,
                    controls(),
                    ValidationSource::NoTree,
                    reply,
                )
                .unwrap()
            {
                CloneStep::Sync(_) => reply = CloneReply::Continue,
                CloneStep::Query(q) => {
                    if op.outcomes.len() == 2 {
                        op.options.as_mut().unwrap().word = 21;
                    }
                    reply = CloneReply::Probe(q.run(&mut wire, || false));
                }
                CloneStep::Confirm { .. } => reply = CloneReply::Confirmation(1),
                CloneStep::Write(w) => reply = CloneReply::Written(w.run(&mut wire, || false)),
                CloneStep::Finalize => break,
                _ => panic!("unexpected write stop"),
            }
        }
        assert!(op.active_mask.is_some());
        assert_eq!(
            op.active_mask.as_ref().unwrap(),
            &clone::write_blocks(6, 21).unwrap()
        );
        let mut live = controls();
        if mode == 2 {
            live.wrapper_state = 7;
        }
        let result = op.finalize(&mut live, |op, live| {
            assert_ne!(mode, 2);
            assert_ne!(mode, 3);
            if mode == 1 {
                return Err(icf::Error::Invalid {
                    line: 0,
                    reason: "synthetic information failure",
                });
            }
            op.active_mask = Some(std::iter::once(7..9).collect());
            live.wrapper_state = 99;
            Ok(())
        });
        if mode == 1 {
            assert!(result.is_err());
            assert!(op.active_mask.is_some());
        } else {
            assert_eq!(
                result.unwrap(),
                if mode == 3 {
                    CloneStop::WriteFailed
                } else {
                    CloneStop::Completed
                }
            );
            assert!(op.active_mask.is_none());
            assert_eq!(live.wrapper_state, 0);
        }
        assert_eq!(doc.baseline(), original.baseline());
        assert_eq!(doc.path(), original.path());
        assert_eq!(doc.dirty(), original.dirty());
        assert_eq!(doc.target_revision(), original.target_revision());
        wire.finish();
    }
}

#[test]
fn logic_phase_process_read_and_option_producer_publish_only_completed_documents() {
    let files = Files::new();
    let (session, doc) = fixture();
    let mut image = source_fields();
    image.resize(clone::IMAGE_CAPACITY, 0);
    image[0x399a2] = 0xa7;
    let mock = files.write(
        "read.json",
        &serde_json::to_vec(&json!({"events":read_events(&image,6)})).unwrap(),
    );
    let asset = files.write("asset.raw", &source_fields());
    let mut args = options(&files, &session, &doc, &mock);
    args[1] = "read".into();
    for key in ["--show-both", "--home-cache"] {
        let at = args.iter().position(|v| v == key).unwrap();
        args.drain(at..at + 2);
    }
    args.retain(|v| v != "--yes");
    let output = files.path("read.icf");
    args.extend([
        "--source-asset".into(),
        format!("1={}", p(&asset)),
        "--no-tree".into(),
        "-o".into(),
        p(&output).into(),
    ]);
    let result = cli(&args, &[]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let r = report(&result);
    assert_eq!(r["status"], "Completed");
    assert_eq!(r["dirty"], 1);
    assert_eq!(r["baseline_committed"], false);
    assert_eq!(r["software_receivers"], true);
    assert_eq!(r["title"]["path"]["hex"], "");
    assert_eq!(r["title"]["dirty"], 1);
    assert_eq!(r["title"]["target"], 6);
    assert_eq!(r["sessions"].as_array().unwrap().len(), 2);
    assert_eq!(r["sessions"][1]["transmitted_wire_bytes"], 10);
    let bytes = fs::read(&output).unwrap();
    assert_eq!(icf::Icf::parse(&bytes).unwrap().memory()[0x399a2], 0xa7);

    let mut malformed = Frame::Data {
        address: 0,
        payload: vec![0],
    }
    .encode(Direction::FromRadio)
    .unwrap();
    let checksum = malformed.len() - 2;
    malformed[checksum] ^= 1;
    let mut events = query_events(&[identity(6)]);
    events.push(json!({"write_hex":hex(&Frame::Read.encode(Direction::ToRadio).unwrap())}));
    events.push(json!({"read_hex":hex(&malformed)}));
    let mock = files.write(
        "failed-read.json",
        &serde_json::to_vec(&json!({"events":events})).unwrap(),
    );
    let at = args.iter().position(|v| v == "--mock").unwrap();
    args[at + 1] = p(&mock).into();
    let no_output = files.path("failed.icf");
    let at = args.iter().position(|v| v == "-o").unwrap();
    args[at + 1] = p(&no_output).into();
    let result = cli(&args, &[]);
    assert_eq!(result.status.code(), Some(5));
    assert_eq!(report(&result)["sessions"][1]["result_code"], 6);
    assert_eq!(report(&result)["validation"]["errors"], 0);
    assert!(!no_output.exists());

    let mut invalid = image.clone();
    invalid[0x355a1] = 0;
    let mock = files.write(
        "invalid-read-image.json",
        &serde_json::to_vec(&json!({"events":read_events(&invalid,6)})).unwrap(),
    );
    let at = args.iter().position(|v| v == "--mock").unwrap();
    args[at + 1] = p(&mock).into();
    let result = cli(&args, &[]);
    assert_eq!(result.status.code(), Some(5));
    assert_eq!(report(&result)["sessions"][1]["result_code"], 15);
    assert_eq!(report(&result)["validation"]["errors"], 0);
    assert!(!no_output.exists());

    let options_json = files.write(
        "options.json",
        br#"{"word":161,"checks":[1,2,1,0,1],"button":"cancel"}"#,
    );
    let result = cli(
        &[
            "clone".into(),
            "options".into(),
            p(&options_json).into(),
            "-o".into(),
            "-".into(),
        ],
        &[],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(report(&result)["option_word"], 0xb5);
    assert_eq!(report(&result)["clone_requested"], false);

    let options_json = files.write(
        "write-options.json",
        br#"{"word":31,"checks":[1,0,1,0,1],"button":"ok"}"#,
    );
    let mock = files.write(
        "option-write.json",
        &serde_json::to_vec(&json!({"events":complete_events(&session,&doc,21)})).unwrap(),
    );
    let mut args = options(&files, &session, &doc, &mock);
    args.extend([
        "--option-input".into(),
        p(&options_json).into(),
        "--no-tree".into(),
        "-o".into(),
        "-".into(),
    ]);
    let result = cli(&args, &[]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(report(&result)["option_word"], 21);
    assert_eq!(report(&result)["mask_released"], true);
    assert_eq!(report(&result)["information_requested"], true);
    assert_eq!(report(&result)["information_displayed"], true);
    assert_eq!(
        report(&result)["information"]["heading"],
        "Device information"
    );
    assert_eq!(report(&result)["information"]["model"], 0x4075);
}

#[test]
fn native_success_never_accepts_saved_baseline_or_applies_private_correction_to_current() {
    let (session, mut doc) = fixture();
    set_call(&mut doc, 123_000_000);
    doc.compare_dirty();
    let baseline = doc.baseline().to_vec();
    let mut op = CloneWrite::new(config(), 31, reset()).unwrap();
    let mut wire = Wire::new(complete_events(&session, &doc, 31));
    assert_eq!(
        run(
            &mut op,
            &mut doc,
            &session,
            &mut wire,
            ValidationSource::Views(&[view()]),
            1,
            |_, _, _| {}
        ),
        CloneStop::Completed
    );
    wire.finish();
    assert_eq!(wire.closes, 4);
    assert_eq!(op.outcomes.len(), 4);
    assert_eq!(doc.baseline(), baseline);
    assert_eq!(doc.path(), b"ORIGINAL");
    assert_eq!(doc.dirty(), 1);
    assert_eq!(call(doc.current()), 123_000_000);
    assert_eq!(call(&op.wire_image), 0);
}

#[test]
fn process_native_x_refuses_before_transport_and_warning_reaches_all_actual_queries_and_corrected_write()
 {
    let files = Files::new();
    let (session, mut doc) = fixture();
    set_call(&mut doc, 1);
    let empty = files.write("empty.json", br#"{"events":[]}"#);
    let views=files.write("views.json",br#"[{"kind":3,"group":100,"first":0,"last":0,"count":1,"include_side1":true,"show_both":1}]"#);
    let mut args = options(&files, &session, &doc, &empty);
    args.extend([
        "--validation-views".into(),
        p(&views).into(),
        "-o".into(),
        "-".into(),
    ]);
    let out = cli(&args, &[]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r = report(&out);
    assert_eq!(r["stage"], "before-transport");
    assert_eq!(r["validation"]["flags"], json!([12]));
    assert_eq!(r["transmitted_wire_bytes"], 0);
    set_call(&mut doc, 123_000_000);
    let events = complete_events(&session, &doc, 0);
    let expected_transmitted: u64 = events
        .iter()
        .filter_map(|e| e["write_hex"].as_str())
        .map(|hex| hex.len() as u64 / 2)
        .sum();
    let mock = files.write(
        "complete.json",
        &serde_json::to_vec(&json!({"events":events})).unwrap(),
    );
    let output = files.path("report.json");
    let mut args = options(&files, &session, &doc, &mock);
    args.extend([
        "--validation-views".into(),
        p(&views).into(),
        "-o".into(),
        p(&output).into(),
    ]);
    let out = cli(&args, &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r = report(&out);
    assert_eq!(r["status"], "Completed");
    assert_eq!(r["validation"]["flags"], json!([4]));
    assert_eq!(r["sessions"].as_array().unwrap().len(), 4);
    assert_eq!(r["transmitted_wire_bytes"], expected_transmitted);
    assert_eq!(r["sync_requests"], json!([0x1b, 0x2a, 0x38, 0x11]));
    assert_eq!(r["close_error"], Value::Null);
    assert_eq!(r["baseline_committed"], false);
    assert!(r["sessions"][3]["payload_bytes"].as_u64().unwrap() > 0);
    assert!(output.exists());
    assert_eq!(fs::read(files.path("current.raw")).unwrap(), doc.current());
}

#[test]
fn process_cancel_dryrun_stdin_no_clobber_and_missing_native_sources_never_open_a_port() {
    let files = Files::new();
    let (session, doc) = fixture();
    let empty = files.write("empty.json", br#"{"events":[]}"#);
    let mut args = options(&files, &session, &doc, &empty);
    args.push("--no-tree".into());
    let out = cli(
        &[
            "clone",
            "write",
            "--cancel",
            "--port",
            "NEVER_OPENED",
            "-o",
            "-",
        ]
        .map(str::to_owned),
        &[],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(report(&out)["inputs_read"], false);
    let mut dry = args.clone();
    dry.push("--dry-run".into());
    let index = dry.iter().position(|s| s == "--mock").unwrap();
    dry[index] = "--port".into();
    dry[index + 1] = "NEVER_OPENED".into();
    let out = cli(&dry, &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(report(&out)["metadata_complete"], true);
    assert_eq!(report(&out)["transport_opened"], false);
    assert_eq!(report(&out)["target_selector"], Value::Null);
    let mut invalid_baud = dry.clone();
    invalid_baud.extend(["--baud".into(), "1".into()]);
    assert_eq!(cli(&invalid_baud, &[]).status.code(), Some(2));
    let mut missing = args.clone();
    let index = missing.iter().position(|s| s == "--generation").unwrap();
    missing[index + 1] = p(&files.path("missing-g")).into();
    missing.push("--dry-run".into());
    assert_eq!(cli(&missing, &[]).status.code(), Some(4));
    let mut duplicate = args.clone();
    duplicate.push("--dry-run".into());
    let i = duplicate.iter().position(|s| s == "--current").unwrap();
    duplicate[i + 1] = "-".into();
    let i = duplicate.iter().position(|s| s == "--mock").unwrap();
    duplicate[i + 1] = "-".into();
    let out = cli(&duplicate, &[]);
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("stdin"));
    let existing = files.write("existing", b"PRESERVED");
    let mut no_clobber = args.clone();
    no_clobber.extend(["-o".into(), p(&existing).into()]);
    let out = cli(&no_clobber, &[]);
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(fs::read(existing).unwrap(), b"PRESERVED");
    let mut unknown = args.clone();
    unknown.retain(|s| s != "--no-tree");
    assert_eq!(cli(&unknown, &[]).status.code(), Some(2));
    let mut no_yes = args;
    no_yes.retain(|s| s != "--yes");
    no_yes.extend(["-o".into(), "-".into()]);
    assert_eq!(cli(&no_yes, &[]).status.code(), Some(6));
}

#[test]
fn process_late_probe_stop_and_terminal_mock_guard_are_not_raw_write_success() {
    let files = Files::new();
    let (mut session, doc) = fixture();
    let mut ids = vec![identity(6); 3];
    ids[2].model = 0;
    let mock = files.write(
        "late.json",
        &serde_json::to_vec(&json!({"events":query_events(&ids)})).unwrap(),
    );
    let mut args = options(&files, &session, &doc, &mock);
    let output = files.path("must-not-publish");
    args.extend(["--no-tree".into(), "-o".into(), p(&output).into()]);
    let out = cli(&args, &[]);
    assert_eq!(out.status.code(), Some(5));
    let r = report(&out);
    assert_eq!(r["status"], "ProbeFailed");
    assert_eq!(r["stage"], "after-private-preparation");
    assert_eq!(r["sessions"].as_array().unwrap().len(), 3);
    assert!(!output.exists());
    assert!(r["transmitted_wire_bytes"].as_u64().unwrap() > 0);
    let mut events = complete_events(&session, &doc, 0);
    events.push(json!({"write_hex":"A7"}));
    let mock = files.write(
        "leftover.json",
        &serde_json::to_vec(&json!({"events":events})).unwrap(),
    );
    let mut args = options(&files, &session, &doc, &mock);
    args.extend(["--no-tree".into(), "-o".into(), p(&output).into()]);
    let out = cli(&args, &[]);
    assert_eq!(out.status.code(), Some(5));
    assert_eq!(report(&out)["close_error"], "InvalidData");
    assert_eq!(report(&out)["transport_result_code"], 17);
    assert!(!output.exists());
    let mut bad = doc.clone();
    let mut g = session.defaults().unwrap().to_vec();
    g[0x3a8ca] = 1;
    g[0x3c8b3] = 255;
    session.adopt_captured_defaults(g, 1).unwrap();
    bad.patch_current(&[
        (0x3a8ca, &[1]),
        (0x3c8b3, &[255]),
        (0x3c800, &[255]),
        (0x3c84a, &[255]),
    ])
    .unwrap();
    let mock = files.write(
        "stage9-error.json",
        &serde_json::to_vec(&json!({"events":query_events(&[identity(6),identity(6)])})).unwrap(),
    );
    let mut args = options(&files, &session, &bad, &mock);
    args.extend(["--no-tree".into(), "-o".into(), p(&output).into()]);
    let out = cli(&args, &[]);
    assert_eq!(out.status.code(), Some(5));
    let r = report(&out);
    assert_eq!(r["private_preparation_stage"], "CurrentStage9");
    assert_eq!(r["sessions"].as_array().unwrap().len(), 2);
    assert!(r["error"].is_string());
    assert!(!output.exists());
    assert_eq!(fs::read(files.path("current.raw")).unwrap(), bad.current());
}

#[test]
fn process_all_stdin_owners_are_checked_before_any_input_read_and_single_owners_work() {
    let files = Files::new();
    let (session, doc) = fixture();
    let empty = files.write("empty.json", br#"{"events":[]}"#);
    let views = files.write("views.json", b"[]");
    let mut base = options(&files, &session, &doc, &empty);
    base.extend([
        "--dry-run".into(),
        "--validation-views".into(),
        p(&views).into(),
        "--startup-script".into(),
        "UNREAD".into(),
        "--japanese-mode-pairs".into(),
        "UNREAD".into(),
        "--restricted-band-stack".into(),
        "UNREAD".into(),
        "--source-asset".into(),
        "1=UNREAD".into(),
    ]);
    let owners = [
        "--current",
        "--range-context",
        "--generation",
        "--validation-views",
        "--mock",
        "--startup-script",
        "--japanese-mode-pairs",
        "--restricted-band-stack",
        "--source-asset",
    ];
    for (n, first) in owners.iter().enumerate() {
        for second in &owners[n + 1..] {
            let mut args = base.clone();
            for owner in [first, second] {
                let i = args.iter().position(|s| s == owner).unwrap();
                args[i + 1] = if *owner == "--source-asset" {
                    "1=-"
                } else {
                    "-"
                }
                .into();
            }
            let out = cli(&args, &[]);
            assert_eq!(
                out.status.code(),
                Some(3),
                "{first}/{second}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(String::from_utf8_lossy(&out.stderr).contains("stdin"));
            assert!(out.stdout.is_empty());
        }
    }
    for owner in &owners[..6] {
        let mut args = options(&files, &session, &doc, &empty);
        args.extend(["--dry-run".into()]);
        if *owner == "--validation-views" {
            args.extend([(*owner).into(), "-".into()]);
        } else {
            args.push("--no-tree".into());
            if *owner == "--startup-script" {
                args.extend([(*owner).into(), "-".into()]);
            } else {
                let i = args.iter().position(|s| s == owner).unwrap();
                args[i + 1] = "-".into();
            }
        }
        let input = match *owner {
            "--current" => doc.current().to_vec(),
            "--generation" => session.defaults().unwrap().to_vec(),
            "--range-context" => fs::read(files.path("context.json")).unwrap(),
            "--validation-views" => b"[]".to_vec(),
            "--mock" => br#"{"events":[]}"#.to_vec(),
            _ => vec![],
        };
        let out = cli(&args, &input);
        assert!(
            out.status.success(),
            "{owner}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(report(&out)["transport_opened"], false);
    }
    let mut regenerated = options(&files, &session, &doc, &empty);
    for option in ["--generation", "--generation-selector"] {
        let i = regenerated.iter().position(|s| s == option).unwrap();
        regenerated.drain(i..i + 2);
    }
    regenerated.extend([
        "--regenerate-defaults".into(),
        "--source-asset".into(),
        "1=-".into(),
        "--no-tree".into(),
        "--dry-run".into(),
    ]);
    let out = cli(&regenerated, &source_fields());
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(report(&out)["metadata_complete"], true);
    let mut args = options(&files, &session, &doc, &empty);
    args.extend([
        "--no-tree".into(),
        "-o".into(),
        "-".into(),
        "--cancel-after-ms".into(),
        "0".into(),
    ]);
    let out = cli(&args, &[]);
    assert_eq!(out.status.code(), Some(5));
    let r = report(&out);
    assert_eq!(r["sessions"][0]["failure"], "Cancelled");
    assert_eq!(r["transmitted_wire_bytes"], 0);
    assert!(out.stdout.is_empty());
}
