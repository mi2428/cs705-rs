//! Native clone write: explicit current/Q/B/G sources, no raw-only fallback.

use crate::{
    AppError, Result,
    args::{TransportArgs, Write},
    confirm, documents, io, transport,
};
use clap::Args as ClapArgs;
use cs705::{
    clone::Transport,
    icf::document::{
        CloneControls, CloneOptionButton, CloneOptions, ClonePreparation, CloneReply, CloneStep,
        CloneStop, CloneWrite, Document, Session, ValidationSource,
    },
};
use serde_json::json;
use std::collections::BTreeMap;

#[derive(ClapArgs)]
#[command(mut_arg("output", |arg| arg.required_unless_present_any(["dry_run", "cancel"])))]
pub(crate) struct Args {
    #[command(flatten)]
    session: documents::SessionArgs,
    #[command(flatten)]
    transport: TransportArgs,
    /// Explicit native tree=NULL. Missing descriptor input is never NoTree.
    #[arg(long, required_unless_present_any = ["validation_views", "cancel"], conflicts_with = "validation_views")]
    no_tree: bool,
    /// Actual typed views, in the shared document descriptor JSON format.
    #[arg(long)]
    validation_views: Option<String>,
    /// Actual caller-owned show-both DWORD for private slot 10.
    #[arg(long, required_unless_present = "cancel")]
    show_both: Option<u32>,
    /// Actual caller-owned home-cache byte for private slot 10.
    #[arg(long, required_unless_present = "cancel")]
    home_cache: Option<u8>,
    #[arg(long, conflicts_with = "option_input", value_parser = clap::value_parser!(u8).range(0..=31))]
    mask: Option<u8>,
    /// Actual option word/checkbox-state JSON, passed through the native producer.
    #[arg(long)]
    option_input: Option<String>,
    /// Explicit approval maps to clone response 1, never file Yes 6.
    #[arg(long)]
    yes: bool,
    /// Cancel before reading any native source or opening a transport.
    #[arg(long)]
    cancel: bool,
    #[command(flatten)]
    write: Write,
}

fn controls() -> CloneControls {
    // This headless sequence owns an idle controller, no outstanding W3C,
    // and the clone form's exact flag57=1 context guard.
    CloneControls {
        controller_busy: 0,
        wrapper_state: 0,
        form_flag57: 1,
    }
}
fn validation(
    views: Option<&[cs705::icf::document::ValidationView]>,
    no_tree: bool,
) -> Result<ValidationSource<'_>> {
    match (views, no_tree) {
        (Some(views), false) => Ok(ValidationSource::Views(views)),
        (None, true) => Ok(ValidationSource::NoTree),
        _ => Err(AppError::invalid(
            "native clone requires explicit --no-tree or --validation-views",
        )),
    }
}

pub(super) fn run(args: Args) -> Result<u8> {
    if args.cancel {
        io::report(
            &json!({"operation":"clone-write","status":"cancelled","stage":"before-input","inputs_read":false,"transport_opened":false,"transmitted_wire_bytes":0,"published":false}),
        )?;
        return Ok(0);
    }
    let assets = documents::assets(&args.session)?;
    // Check the ENTIRE ownership set before config/mock/context/current reads.
    io::single_stdin(
        [
            args.session.range_context.as_deref(),
            args.session.current.as_deref(),
            args.session.generation.as_deref(),
            args.session.japanese_mode_pairs.as_deref(),
            args.session.restricted_band_stack.as_deref(),
            args.validation_views.as_deref(),
            args.option_input.as_deref(),
            args.transport.mock.as_deref(),
            args.transport.startup_script.as_deref(),
        ]
        .into_iter()
        .flatten()
        .chain(assets.values().copied()),
    )?;
    io::preflight(&args.write)?;
    let options = args.option_input.as_deref().map(option_input).transpose()?;
    if let Some((options, false)) = &options {
        io::report(
            &json!({"operation":"clone-options","status":"saved-without-clone","option_word":options.word,"mask":options.mask(),"transmitted_wire_bytes":0,"published":false}),
        )?;
        return Ok(0);
    }
    confirm(args.yes)?;
    let reopen_token = format!("{},N,8,1", args.transport.baud).into_bytes();
    let config = super::config(&args.transport, None)?;
    let mut mock = args
        .transport
        .mock
        .as_deref()
        .map(transport::Mock::from_file)
        .transpose()?;
    let mut doc = Document::from_current(documents::current(&args.session)?, Vec::new(), 6);
    let session = documents::session(&args.session, &assets, doc.current(), &mut BTreeMap::new())?;
    session.defaults()?;
    let views = documents::validation_views(args.validation_views.as_deref())?;
    let source = validation(views.as_deref(), args.no_tree)?;
    let preflight = doc.validate_views(&session, source)?;
    if preflight.errors as i32 > 0 {
        io::report(
            &json!({"operation":"clone-write","status":"InvalidItems","stage":"before-transport","validation":{"errors":preflight.errors,"warnings":preflight.warnings,"flags":preflight.flags},"transport_opened":false,"transmitted_wire_bytes":0,"published":false}),
        )?;
        return Ok(3);
    }
    let reset = documents::reset_state(args.show_both, args.home_cache)?;
    let mut operation = CloneWrite::new(config, args.mask.unwrap_or(0), reset)
        .map_err(|e| AppError::invalid(e.to_string()))?;
    operation.options = options.map(|(options, _)| options);
    if args.write.dry_run {
        let mut preparation = ClonePreparation::default();
        preparation.prepare(&mut doc, &session, &mut operation.reset)?;
        io::report(
            &json!({"operation":"clone-write","dry_run":true,"stage":"private-metadata-prepared","validation":{"errors":preflight.errors,"warnings":preflight.warnings,"flags":preflight.flags},"metadata_complete":preparation.metadata_complete,"target_selector":null,"probe_required":true,"transport_opened":false,"transmitted_wire_bytes":0,"baseline_committed":false,"published":false}),
        )?;
        return Ok(0);
    }
    let result = if let Some(mock) = mock.as_mut() {
        drive(
            &mut operation,
            &mut doc,
            &session,
            source,
            mock,
            &args.transport,
            &reopen_token,
        )
    } else {
        let mut serial = transport::Serial::open(&args.transport)?;
        drive(
            &mut operation,
            &mut doc,
            &session,
            source,
            &mut serial,
            &args.transport,
            &reopen_token,
        )
    };
    let SequenceResult {
        stop,
        sync,
        close_error,
        reopen_error,
        error,
        information,
    } = result;
    let completed = stop == Some(CloneStop::Completed) && close_error.is_none() && error.is_none();
    let report = json!({
        "operation":"clone-write", "status":stop.map(|s|format!("{s:?}")), "error":error,
        "stage":if operation.outcomes.is_empty(){"before-query-result"}else if operation.preparation.metadata_complete{"after-private-preparation"}else{"after-probe"},
        "private_preparation_stage":format!("{:?}",operation.preparation.stage),
        "ok":completed, "transport_opened":true,
        "transport_lifecycle":if args.transport.mock.is_some(){"one retained transcript; logical Session close; terminal event guard"}else{"same prepared endpoint; Session close then reconfigure/reopen before next Session"},
        "close_error":close_error.map(|e|format!("{e:?}")),
        "reopen_error":reopen_error.map(|e|format!("{e:?}")),
        "transport_result_code":if close_error.is_some() || reopen_error.is_some(){Some(17)}else{operation.outcomes.last().map(|o|o.result_code())},
        "transmitted_wire_bytes":operation.outcomes.iter().map(|o|o.transmitted_wire_bytes).sum::<u64>(),
        "sync_requests":sync, "notifications_delivered":false,
        "confirmation_mapping":"--yes => clone response1 (not file Yes6)",
        "saved_selector":operation.saved_selector, "fc":operation.fc, "fd":operation.fd,
        "validation":operation.validation.as_ref().map(|v|json!({"errors":v.errors,"warnings":v.warnings,"flags":v.flags})),
        "correction":operation.preparation.correction.map(|c|json!({"typed_saved":c.typed_saved,"saved_repeaters":c.lists.saved_repeaters,"deleted_repeaters":c.lists.deleted_repeaters,"deleted_ur":c.lists.deleted_ur,"deleted_allowed":c.lists.deleted_allowed})),
        "metadata_complete":operation.preparation.metadata_complete,
        "private_image_bytes":operation.preparation.image.len(), "wire_image_bytes":operation.wire_image.len(),
        "home_cache":operation.reset.home_invalidated, "dirty":doc.dirty(), "baseline_committed":false,
        "information_requested":operation.information_requested,"information_displayed":information.is_some(),"information":information,"mask_released":matches!(stop,Some(CloneStop::Completed | CloneStop::WriteFailed)) && operation.active_mask.is_none(),
        "option_word":operation.options.as_ref().map(|o|o.word),
        "sessions":operation.outcomes.iter().map(|o|super::report(o,false)).collect::<Vec<_>>(), "published":completed
    });
    if completed {
        io::publish(&io::json_bytes(&report)?, &args.write)?;
    }
    io::report(&report)?;
    Ok(if completed { 0 } else { 5 })
}

struct SequenceResult {
    stop: Option<CloneStop>,
    sync: Vec<u32>,
    close_error: Option<std::io::ErrorKind>,
    reopen_error: Option<std::io::ErrorKind>,
    error: Option<String>,
    information: Option<serde_json::Value>,
}

fn drive(
    operation: &mut CloneWrite,
    doc: &mut Document,
    session: &Session,
    validation: ValidationSource<'_>,
    transport: &mut impl Transport,
    args: &TransportArgs,
    reopen_token: &[u8],
) -> SequenceResult {
    let start = std::time::Instant::now();
    let mut cancelled = || {
        args.cancel_after_ms
            .is_some_and(|limit| start.elapsed().as_millis() >= u128::from(limit))
    };
    let mut sync = Vec::new();
    let mut live_controls = controls();
    let mut reply = CloneReply::Continue;
    let mut ran_session = false;
    let mut reopen_error = None;
    let mut information = None;
    let result = loop {
        match operation.advance(doc, session, live_controls, validation, reply) {
            Ok(CloneStep::Sync(event)) => {
                sync.push(event);
                reply = CloneReply::Continue;
            }
            Ok(CloneStep::Confirm { .. }) => reply = CloneReply::Confirmation(1),
            Ok(CloneStep::Finalize) => {
                break operation
                    .finalize(&mut live_controls, |operation, _| {
                        let identity = operation.outcomes.last().and_then(|o| o.identity.as_ref())
                            .ok_or(cs705::icf::Error::Invalid { line: 0, reason: "completed clone identity is unavailable" })?;
                        let label = identity.tail.get(2..18)
                            .ok_or(cs705::icf::Error::Invalid { line: 0, reason: "completed clone label is unavailable" })?;
                        information = Some(json!({"heading":"Device information","model":identity.model,"revision":identity.revision,"stored_label":io::bytes_value(label.split(|b|*b==0).next().unwrap())}));
                        Ok(())
                    })
                    .map_err(|e| e.to_string());
            }
            Ok(step @ (CloneStep::Query(_) | CloneStep::Write(_))) => {
                let (session, write) = match step {
                    CloneStep::Query(s) => (s, false),
                    CloneStep::Write(s) => (s, true),
                    _ => unreachable!(),
                };
                let outcome = match super::sequence_session(
                    session,
                    transport,
                    args,
                    reopen_token,
                    &mut ran_session,
                    &mut cancelled,
                ) {
                    Ok(outcome) => outcome,
                    Err(e) => {
                        reopen_error = Some(e.kind());
                        break Err(format!("same-endpoint reopen failed: {e}"));
                    }
                };
                reply = if write {
                    CloneReply::Written(outcome)
                } else {
                    CloneReply::Probe(outcome)
                };
            }
            Ok(CloneStep::Stopped(stop)) => break Ok(stop),
            Err(error) => break Err(error.to_string()),
        }
    };
    // Never skip the underlying close or the strict Mock remaining-events guard.
    let close_error = transport.close().err().map(|e| e.kind());
    let (stop, error) = match result {
        Ok(stop) => (Some(stop), None),
        Err(error) => (None, Some(error)),
    };
    SequenceResult {
        stop,
        sync,
        close_error,
        reopen_error,
        error,
        information,
    }
}

fn option_input(path: &str) -> Result<(CloneOptions, bool)> {
    let value: serde_json::Value = serde_json::from_slice(&io::read(path)?)
        .map_err(|_| AppError::invalid("invalid clone option JSON"))?;
    if value.as_object().is_none_or(|v| {
        v.keys()
            .any(|k| !matches!(k.as_str(), "word" | "checks" | "button"))
    }) {
        return Err(AppError::invalid(
            "clone options require only word/checks/button",
        ));
    }
    let word = value["word"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| AppError::invalid("option word requires u32"))?;
    let mut options = CloneOptions::open(word);
    for index in 0..5 {
        options.initialize(index)?;
    }
    if let Some(checks) = value.get("checks") {
        let checks = checks
            .as_array()
            .filter(|v| v.len() == 5)
            .ok_or_else(|| AppError::invalid("checks require five checkbox states"))?;
        for (index, state) in checks.iter().enumerate() {
            options.states[index] = state
                .as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .filter(|n| *n <= 2)
                .ok_or_else(|| AppError::invalid("checkbox state requires0..2"))?;
        }
    }
    let button = match value["button"].as_str() {
        Some("ok") => CloneOptionButton::Ok,
        Some("cancel") => CloneOptionButton::Cancel,
        Some("form-event") => CloneOptionButton::FormEvent,
        _ => {
            return Err(AppError::invalid(
                "option button requires ok|cancel|form-event",
            ));
        }
    };
    let start = options.press(button)?;
    Ok((options, start))
}

pub(super) fn options(input: String, write: Write) -> Result<u8> {
    io::preflight(&write)?;
    let (options, start) = option_input(&input)?;
    let report = json!({"operation":"clone-options","option_word":options.word,"mask":options.mask(),"clone_requested":start,"clone_started":false,"notifications_delivered":false});
    io::publish(&io::json_bytes(&report)?, &write)?;
    io::report(&report)?;
    Ok(0)
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use cs705::clone::{self, Direction, Frame, Identity};
    use std::io::{self as stdio, Cursor, Read, Write};

    struct Endpoint {
        response: Cursor<Vec<u8>>,
        responses: std::collections::VecDeque<Vec<u8>>,
        open: bool,
        now: u64,
        calls: Vec<&'static str>,
        terminal_close: usize,
        close_count: usize,
        fail_reopen: bool,
    }
    impl Endpoint {
        fn new(revision: u16, terminal_close: usize, fail_reopen: bool) -> Self {
            let mut tail = vec![0; 33];
            tail[0] = 33;
            tail[1] = 6;
            Self {
                response: Cursor::new(Vec::new()),
                responses: [Frame::Response(Identity {
                    model: 0x4075,
                    revision,
                    tail,
                })
                .encode(Direction::FromRadio)
                .unwrap()]
                .into(),
                open: true,
                now: 0,
                calls: Vec::new(),
                terminal_close,
                close_count: 0,
                fail_reopen,
            }
        }
    }
    impl Read for Endpoint {
        fn read(&mut self, bytes: &mut [u8]) -> stdio::Result<usize> {
            assert!(self.open);
            let n = self.response.read(bytes)?;
            if n != 0 {
                self.calls.push("read:selected");
            }
            Ok(n)
        }
    }
    impl Write for Endpoint {
        fn write(&mut self, bytes: &[u8]) -> stdio::Result<usize> {
            assert!(self.open);
            assert_eq!(bytes, Frame::Identify.encode(Direction::ToRadio).unwrap());
            assert_eq!(
                self.response.position(),
                self.response.get_ref().len() as u64
            );
            self.response = Cursor::new(
                self.responses
                    .pop_front()
                    .expect("unexpected query generation"),
            );
            self.calls.push("write:selected");
            Ok(bytes.len())
        }
        fn flush(&mut self) -> stdio::Result<()> {
            assert!(self.open);
            Ok(())
        }
    }
    impl Transport for Endpoint {
        fn now_ms(&self) -> u64 {
            self.now
        }
        fn wait_ms(&mut self, ms: u64) -> stdio::Result<()> {
            self.now += ms;
            Ok(())
        }
        fn pending_output(&mut self) -> stdio::Result<usize> {
            assert!(self.open);
            Ok(0)
        }
        fn close(&mut self) -> stdio::Result<()> {
            self.calls.push("close:selected");
            self.open = false;
            self.close_count += 1;
            if self.close_count == self.terminal_close {
                Err(stdio::ErrorKind::ConnectionReset.into())
            } else {
                Ok(())
            }
        }
        fn reconfigure(&mut self, token: &[u8]) -> stdio::Result<()> {
            assert!(!self.open);
            assert_eq!(token, b"115200,N,8,1");
            self.calls.push("reopen:selected");
            if self.fail_reopen {
                Err(stdio::ErrorKind::PermissionDenied.into())
            } else {
                self.open = true;
                Ok(())
            }
        }
    }
    fn state(retain: bool) -> (CloneWrite, Document, Session, TransportArgs) {
        let mut config = clone::SessionConfig::default();
        config.settings.echo = clone::Echo::None;
        let op = CloneWrite::new(
            config,
            0,
            cs705::icf::normalizer::ResetState {
                show_both: 1,
                home_invalidated: 0xa7,
            },
        )
        .unwrap();
        let doc = Document::from_current(vec![0xa7; clone::IMAGE_CAPACITY], b"OLD".to_vec(), 6);
        let session = Session::new(
            cs705::icf::context::Context::from_bytes([0; 24]),
            None,
            1,
            [1, 0],
        );
        let args = TransportArgs {
            mock: retain.then(|| "unused-transcript".into()),
            port: (!retain).then(|| "selected".into()),
            allow_device_io: false,
            acknowledge_line_risk: false,
            baud: 115200,
            dtr: false,
            rts: false,
            echo: crate::args::EchoMode::None,
            timeout_ms: 1000,
            startup_script: None,
            cancel_after_ms: None,
        };
        (op, doc, session, args)
    }
    #[test]
    fn native_query_boundary_closes_and_reopens_the_same_endpoint_in_order() {
        let mut endpoint = Endpoint::new(1, usize::MAX, false);
        let mut tail = vec![0; 33];
        tail[0] = 33;
        tail[1] = 5;
        endpoint.responses.push_back(
            Frame::Response(Identity {
                model: 0x4075,
                revision: 1,
                tail,
            })
            .encode(Direction::FromRadio)
            .unwrap(),
        );
        let mut config = clone::SessionConfig::default();
        config.settings.echo = clone::Echo::None;
        for selector in [6, 5] {
            if selector == 5 {
                endpoint.reconfigure(b"115200,N,8,1").unwrap();
            }
            let out = clone::Session::query(config.clone()).unwrap().run(
                &mut transport::SequenceLease::new(&mut endpoint, false),
                || false,
            );
            assert_eq!(out.result_code(), 0);
            assert_eq!(out.identity_gate.cached_tail.unwrap()[1], selector);
            assert!(!endpoint.open);
        }
        endpoint.close().unwrap();
        assert!(endpoint.responses.is_empty());
        assert_eq!(
            endpoint.calls,
            [
                "write:selected",
                "read:selected",
                "close:selected",
                "reopen:selected",
                "write:selected",
                "read:selected",
                "close:selected",
                "close:selected"
            ]
        );
    }
    #[test]
    fn terminal_close_error_keeps_prior_probe_result_and_prefix_in_both_modes() {
        for retain in [true, false] {
            let (mut op, mut doc, session, args) = state(retain);
            let before = doc.current().to_vec();
            let mut endpoint = Endpoint::new(2, if retain { 1 } else { 2 }, false);
            let result = drive(
                &mut op,
                &mut doc,
                &session,
                ValidationSource::NoTree,
                &mut endpoint,
                &args,
                b"115200,N,8,1",
            );
            assert_eq!(result.stop, Some(CloneStop::ProbeFailed));
            assert_eq!(result.close_error, Some(stdio::ErrorKind::ConnectionReset));
            assert_eq!(result.reopen_error, None);
            assert_eq!(result.error, None);
            assert_eq!(op.outcomes.len(), 1);
            let out = &op.outcomes[0];
            assert_eq!(out.result_code(), 14);
            assert_eq!(out.transmitted_wire_bytes, 10);
            assert_eq!(out.identity_gate.cached_model, Some(0x4075));
            assert_eq!(out.identity.as_ref().unwrap().revision, 2);
            assert_eq!(doc.current(), before);
            assert_eq!(doc.baseline(), before);
            assert_eq!(doc.path(), b"OLD");
            assert!(op.wire_image.is_empty());
            assert_eq!(
                endpoint.calls,
                if retain {
                    vec!["write:selected", "read:selected", "close:selected"]
                } else {
                    vec![
                        "write:selected",
                        "read:selected",
                        "close:selected",
                        "close:selected",
                    ]
                }
            );
        }
    }
    #[test]
    fn same_endpoint_reopen_failure_stops_before_next_query_without_retry() {
        let (mut op, mut doc, session, args) = state(false);
        let before = doc.current().to_vec();
        let mut endpoint = Endpoint::new(1, usize::MAX, true);
        let result = drive(
            &mut op,
            &mut doc,
            &session,
            ValidationSource::NoTree,
            &mut endpoint,
            &args,
            b"115200,N,8,1",
        );
        assert_eq!(result.stop, None);
        assert_eq!(
            result.reopen_error,
            Some(stdio::ErrorKind::PermissionDenied)
        );
        assert_eq!(result.close_error, None);
        assert!(
            result
                .error
                .unwrap()
                .contains("same-endpoint reopen failed")
        );
        assert_eq!(result.sync, [0x1b, 0x2a, 0x38, 0x11]);
        assert_eq!(op.outcomes.len(), 1);
        assert_eq!(op.outcomes[0].result_code(), 0);
        assert_eq!(op.outcomes[0].transmitted_wire_bytes, 10);
        assert_eq!((op.saved_selector, op.fc, op.fd), (6, 0, 0));
        assert!(op.preparation.image.is_empty());
        assert_eq!(doc.current(), before);
        assert_eq!(doc.baseline(), before);
        assert_eq!(
            endpoint.calls,
            [
                "write:selected",
                "read:selected",
                "close:selected",
                "reopen:selected",
                "close:selected"
            ]
        );
    }
}
