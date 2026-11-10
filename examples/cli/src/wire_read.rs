//! Native query/read sequence using the shared transport and Document sources.

use crate::{
    AppError, Result,
    args::{TransportArgs, Write},
    documents, io, transport,
};
use clap::Args as ClapArgs;
use cs705::{
    clone::Transport,
    icf::{
        context::Context,
        document::{
            CloneControls, CloneRead, CloneReadReply, CloneReadStep, CloneReadStop, Document,
            ValidationSource, lifecycle::UiState,
        },
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
    #[arg(long, required_unless_present_any = ["validation_views", "cancel"], conflicts_with = "validation_views")]
    no_tree: bool,
    #[arg(long)]
    validation_views: Option<String>,
    /// Approve a different identity at event 2A. This saves D28/D40 only; live
    /// Q/B/G adoption still occurs in the successful borrowed-result callback.
    #[arg(long)]
    accept_context: bool,
    #[arg(long)]
    cancel: bool,
    #[command(flatten)]
    write: Write,
}

pub(super) fn run(args: Args) -> Result<u8> {
    if args.cancel {
        io::report(
            &json!({"operation":"clone-read","status":"cancelled","inputs_read":false,"transport_opened":false,"published":false}),
        )?;
        return Ok(0);
    }
    let assets = documents::assets(&args.session)?;
    io::single_stdin(
        [
            args.session.range_context.as_deref(),
            args.session.current.as_deref(),
            args.session.generation.as_deref(),
            args.session.japanese_mode_pairs.as_deref(),
            args.session.restricted_band_stack.as_deref(),
            args.validation_views.as_deref(),
            args.transport.mock.as_deref(),
            args.transport.startup_script.as_deref(),
        ]
        .into_iter()
        .flatten()
        .chain(assets.values().copied()),
    )?;
    io::preflight(&args.write)?;
    let config = super::config(&args.transport, None)?;
    let mut mock = args
        .transport
        .mock
        .as_deref()
        .map(transport::Mock::from_file)
        .transpose()?;
    let mut doc = Document::from_current(documents::current(&args.session)?, Vec::new(), 6);
    let mut cache = BTreeMap::new();
    let mut session = documents::session(&args.session, &assets, doc.current(), &mut cache)?;
    let views = documents::validation_views(args.validation_views.as_deref())?;
    let validation = match (views.as_deref(), args.no_tree) {
        (Some(views), false) => ValidationSource::Views(views),
        (None, true) => ValidationSource::NoTree,
        _ => {
            return Err(AppError::invalid(
                "native read requires --no-tree or --validation-views",
            ));
        }
    };
    let mut operation = CloneRead::new(config, 6).map_err(|e| AppError::invalid(e.to_string()))?;
    if args.write.dry_run {
        io::report(
            &json!({"operation":"clone-read","dry_run":true,"probe_required":true,"transport_opened":false,"published":false}),
        )?;
        return Ok(0);
    }
    let mut requests = Vec::new();
    let mut ui = UiState::headless();
    let mut run = |transport: &mut dyn Transport| -> Result<_> {
        let mut reply = CloneReadReply::Continue;
        let mut ran = false;
        let start = std::time::Instant::now();
        let mut cancelled = || {
            args.transport
                .cancel_after_ms
                .is_some_and(|limit| start.elapsed().as_millis() >= u128::from(limit))
        };
        let token = format!("{},N,8,1", args.transport.baud).into_bytes();
        let controls = CloneControls {
            controller_busy: 0,
            wrapper_state: 0,
            form_flag57: 1,
        };
        let result = (|| {
            loop {
                let step = operation
                    .advance(&session, controls, reply)
                    .map_err(|e| AppError::invalid(e.to_string()))?;
                match step {
                    CloneReadStep::Sync(event) => {
                        requests.push(format!("Sync({event})"));
                        if event == 0x2a && args.accept_context {
                            let identity = operation
                                .outcomes
                                .last()
                                .and_then(|o| o.identity.as_ref())
                                .ok_or_else(|| AppError::invalid("read identity unavailable"))?;
                            if let Some(request) =
                                cs705::icf::document::InitialRequest::information(
                                    &session, identity, 1,
                                )?
                            {
                                doc.receive_initial_identity(
                                    &mut session,
                                    &mut operation.initial,
                                    identity,
                                    request,
                                    None,
                                    |event, doc, session| {
                                        requests.push(format!("{event:?}"));
                                        ui.initial_boundary(event, doc, session)
                                    },
                                )?;
                            }
                        }
                        reply = CloneReadReply::Continue;
                    }
                    CloneReadStep::Query(query) => {
                        reply = CloneReadReply::Probe(super::sequence_session(
                            query,
                            transport,
                            &args.transport,
                            &token,
                            &mut ran,
                            &mut cancelled,
                        )?)
                    }
                    CloneReadStep::Read(read) => {
                        reply = CloneReadReply::Read(super::sequence_session(
                            read,
                            transport,
                            &args.transport,
                            &token,
                            &mut ran,
                            &mut cancelled,
                        )?)
                    }
                    CloneReadStep::Apply => {
                        if let Some(outcome) = operation.outcomes.last() {
                            let source = if outcome.result_code() == 0 {
                                let mut q = Context::from_bytes(*session.context.as_bytes());
                                q.adopt(operation.initial.saved_context.ok_or_else(|| {
                                    AppError::invalid("saved read context unavailable")
                                })?);
                                Some(documents::source(
                                    &args.session,
                                    &assets,
                                    &q,
                                    session.system_langid,
                                    &mut cache,
                                )?)
                            } else {
                                None
                            };
                            return operation
                                .finish_document(
                                    &mut doc,
                                    &mut session,
                                    source.as_ref().map(documents::Source::borrow),
                                    validation,
                                    |event, doc, session| {
                                        requests.push(format!("{event:?}"));
                                        ui.initial_boundary(event, doc, session)
                                    },
                                )
                                .map_err(|e| AppError::invalid(e.to_string()));
                        }
                        return Err(AppError::invalid("native read outcome unavailable"));
                    }
                    CloneReadStep::Stopped(stop) => return Ok(stop),
                }
            }
        })();
        let close = transport.close().err().map(|e| format!("{e:?}"));
        Ok((result, close))
    };
    let (result, close_error) = if let Some(mock) = mock.as_mut() {
        run(mock)?
    } else {
        run(&mut transport::Serial::open(&args.transport)?)?
    };
    let stop = result.as_ref().ok().copied();
    let completed = stop == Some(CloneReadStop::Completed) && close_error.is_none();
    let report = json!({"operation":"clone-read","status":stop.map(|s|format!("{s:?}")),"error":result.err().map(|e:AppError|e.message),"close_error":close_error,"published":completed,"dirty":doc.dirty(),"path":io::bytes_value(doc.path()),"target_revision":doc.target_revision(),"baseline_committed":false,"sync_requests":requests,"notifications_delivered":false,"software_receivers":true,"title":ui.title.as_ref().map(|t|json!({"path":io::bytes_value(&t.path),"dirty":t.dirty,"target":t.target})),"validation":operation.validation.as_ref().map(|v|json!({"errors":v.errors,"warnings":v.warnings,"flags":v.flags})),"sessions":operation.outcomes.iter().map(|o|super::report(o,false)).collect::<Vec<_>>()});
    if completed {
        io::publish(&doc.current_bytes(&session)?, &args.write)?;
    }
    io::report(&report)?;
    Ok(if completed { 0 } else { 5 })
}
