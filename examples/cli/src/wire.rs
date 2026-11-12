//! Wire command boundary. Session adapters are separate from frame diagnostics.

use crate::{AppError, Result, args::*, confirm, io, transport};
use cs705::clone::{self, Direction, Echo, Frame, Framer, Outcome, Session, SessionConfig};
use serde_json::{Value, json};

#[path = "wire_document.rs"]
pub(crate) mod document;
#[path = "wire_read.rs"]
pub(crate) mod native_read;

fn direction(value: WireDirection) -> Direction {
    match value {
        WireDirection::ToRadio => Direction::ToRadio,
        WireDirection::FromRadio => Direction::FromRadio,
    }
}
fn frame_value(frame: Frame) -> Value {
    match frame {
        Frame::Data { address, payload } => {
            json!({"frame":"data","address":address,"payload":io::bytes_value(&payload)})
        }
        Frame::End { block_count } => json!({"frame":"end","block_count":block_count}),
        other => json!({"frame":format!("{other:?}")}),
    }
}

pub fn run(action: CloneAction) -> Result<u8> {
    match action {
        CloneAction::Decode {
            input,
            direction: dir,
        } => {
            let source = io::read(&input)?;
            let mut framer = Framer::default();
            let mut frames = Vec::new();
            for byte in source {
                if let Some(wire) = framer.push(byte)? {
                    frames.push(frame_value(Frame::decode(&wire, direction(dir))?));
                }
            }
            framer.finish()?;
            io::stdout(&io::json_bytes(&json!(frames))?)?;
        }
        CloneAction::Encode {
            frame,
            direction: dir,
            address,
            payload_hex,
            block_count,
            write,
        } => {
            if !matches!(frame, FrameKind::Data) && (address.is_some() || payload_hex.is_some()) {
                return Err(AppError::invalid(
                    "address/payload-hex require --frame data",
                ));
            }
            if !matches!(frame, FrameKind::End) && block_count.is_some() {
                return Err(AppError::invalid("block-count requires --frame end"));
            }
            let frame =
                match frame {
                    FrameKind::Identify => Frame::Identify,
                    FrameKind::Read => Frame::Read,
                    FrameKind::Write => Frame::Write,
                    FrameKind::Acknowledgement => Frame::Acknowledgement,
                    FrameKind::End => Frame::End {
                        block_count: block_count
                            .ok_or_else(|| AppError::invalid("end frame requires --block-count"))?,
                    },
                    FrameKind::Data => Frame::Data {
                        address: address
                            .ok_or_else(|| AppError::invalid("data frame requires --address"))?,
                        payload: io::decode_hex(&payload_hex.ok_or_else(|| {
                            AppError::invalid("data frame requires --payload-hex")
                        })?)?,
                    },
                };
            io::publish(&frame.encode(direction(dir))?, &write)?;
        }
        CloneAction::Plan {
            profile,
            write,
            mask,
        } => {
            let range = clone::read_range(profile)?;
            let blocks = if write {
                clone::write_blocks(profile, mask)?
            } else {
                vec![range]
            };
            io::stdout(&io::json_bytes(
                &json!({"profile":profile,"write":write,"blocks":blocks.iter().map(|r|json!({"start":r.start,"end":r.end})).collect::<Vec<_>>(),"authorization":false}),
            )?)?;
        }
        CloneAction::Query { transport } => {
            let session = Session::query(config(&transport, None)?)?;
            let outcome = execute(session, &transport)?;
            io::stdout(&io::json_bytes(&report(&outcome, false))?)?;
            return Ok(if outcome.failure.is_none() { 0 } else { 5 });
        }
        CloneAction::Read(args) => return native_read::run(args),
        CloneAction::RawRead {
            transport,
            profile,
            accept_partial,
            write,
        } => {
            io::preflight(&write)?;
            let session = Session::read(config(&transport, None)?, profile.try_into()?)?;
            if write.dry_run {
                validate_mock(&transport)?;
                io::report(
                    &json!({"dry_run":true,"operation":"clone-read","profile":profile,"transport_opened":false}),
                )?;
                return Ok(0);
            }
            let outcome = execute(session, &transport)?;
            io::report(&report(&outcome, false))?;
            let failed = outcome.failure.is_some();
            if !failed || accept_partial {
                io::publish(&io::json_bytes(&report(&outcome, true))?, &write)?;
            }
            return Ok(if failed { 5 } else { 0 });
        }
        CloneAction::Write(args) => return document::run(args),
        CloneAction::Options { input, write } => return document::options(input, write),
        CloneAction::RawWrite {
            input,
            transport,
            profile,
            mask,
            yes,
            write,
        } => {
            confirm(yes)?;
            io::preflight(&write)?;
            let config = config(&transport, Some(input.as_str()))?;
            let bytes = io::read(&input)?;
            let session = Session::write(config, profile.try_into()?, mask, &bytes)?;
            if write.dry_run {
                validate_mock(&transport)?;
                io::report(
                    &json!({"dry_run":true,"operation":"clone-raw-write","profile":profile,"mask":mask,"transport_opened":false}),
                )?;
                return Ok(0);
            }
            let outcome = execute(session, &transport)?;
            let report = report(&outcome, false);
            io::report(&report)?;
            io::publish(&io::json_bytes(&report)?, &write)?;
            return Ok(if outcome.failure.is_none() { 0 } else { 5 });
        }
        CloneAction::Search {
            list,
            port,
            mock,
            allow_device_io,
            acknowledge_line_risk,
            yes,
        } => {
            if list {
                if !allow_device_io {
                    return Err(AppError::safety(
                        "port enumeration requires --allow-device-io",
                    ));
                }
                let ports = serialport::available_ports()
                    .map_err(|_| AppError::io("serial port enumeration failed"))?;
                io::stdout(&io::json_bytes(&json!(
                    ports
                        .iter()
                        .map(|p| json!({"port":p.port_name,"kind":format!("{:?}",p.port_type)}))
                        .collect::<Vec<_>>()
                ))?)?;
            } else {
                confirm(yes)?;
                if port.is_empty() && mock.is_none() {
                    return Err(AppError::invalid(
                        "search requires --list, --port or --mock",
                    ));
                }
                let candidates: Vec<_> = if let Some(mock) = mock {
                    vec![TransportArgs {
                        mock: Some(mock),
                        port: None,
                        allow_device_io: false,
                        acknowledge_line_risk: false,
                        baud: 115200,
                        dtr: false,
                        rts: false,
                        echo: EchoMode::Auto,
                        timeout_ms: 1000,
                        startup_script: None,
                        cancel_after_ms: None,
                    }]
                } else {
                    port.into_iter()
                        .map(|port| TransportArgs {
                            mock: None,
                            port: Some(port),
                            allow_device_io,
                            acknowledge_line_risk,
                            baud: 115200,
                            dtr: false,
                            rts: false,
                            echo: EchoMode::Auto,
                            timeout_ms: 1000,
                            startup_script: None,
                            cancel_after_ms: None,
                        })
                        .collect()
                };
                let mut results = Vec::new();
                let mut failures = 0;
                for (index, args) in candidates.iter().enumerate() {
                    match execute(Session::query(config(args, None)?)?, args) {
                        Ok(outcome) => {
                            if outcome.failure.is_some() {
                                failures += 1;
                            }
                            results
                                .push(json!({"candidate":index,"result":report(&outcome,false)}));
                        }
                        Err(error) => {
                            failures += 1;
                            results.push(json!({"candidate":index,"error":error.message,"exit_code":error.code}));
                        }
                    }
                }
                io::stdout(&io::json_bytes(
                    &json!({"candidates":results,"failed":failures}),
                )?)?;
                return Ok(if failures == 0 { 0 } else { 5 });
            }
        }
    }
    Ok(0)
}

fn config(args: &TransportArgs, image: Option<&str>) -> Result<SessionConfig> {
    io::single_stdin(
        image
            .into_iter()
            .chain(args.startup_script.as_deref())
            .chain(args.mock.as_deref()),
    )?;
    let mut config = SessionConfig {
        timeout_ms: args.timeout_ms,
        ..SessionConfig::default()
    };
    config.settings.baud = args.baud;
    config.settings.dtr_control = u8::from(args.dtr);
    config.settings.rts_control = u8::from(args.rts);
    config.settings.echo = match args.echo {
        EchoMode::None => Echo::None,
        EchoMode::Known => Echo::Known,
        EchoMode::Auto => Echo::Auto,
    };
    if let Some(path) = &args.startup_script {
        config.startup_script = io::read(path)?;
    }
    Ok(config)
}

fn validate_mock(args: &TransportArgs) -> Result<()> {
    if let Some(path) = &args.mock {
        // Pure input validation; neither run a transport nor publish a payload.
        transport::Mock::from_file(path)?;
    }
    Ok(())
}

fn execute(session: Session, args: &TransportArgs) -> Result<Outcome> {
    let start = std::time::Instant::now();
    let cancelled = || {
        args.cancel_after_ms
            .is_some_and(|limit| start.elapsed().as_millis() >= u128::from(limit))
    };
    if let Some(path) = &args.mock {
        let mut mock = transport::Mock::from_file(path)?;
        Ok(session.run(&mut mock, || Ok(cancelled())))
    } else {
        let mut serial = transport::Serial::open(args)?;
        Ok(session.run(&mut serial, || Ok(cancelled())))
    }
}
fn sequence_session(
    session: Session,
    transport: &mut dyn clone::Transport,
    args: &TransportArgs,
    reopen_token: &[u8],
    ran: &mut bool,
    cancelled: &mut impl FnMut() -> bool,
) -> std::io::Result<Outcome> {
    let retain = args.mock.is_some();
    if *ran && !retain {
        transport.reconfigure(reopen_token)?;
    }
    let outcome = session.run(
        &mut transport::SequenceLease::new(transport, retain),
        || Ok(cancelled()),
    );
    *ran = true;
    Ok(outcome)
}
fn report(outcome: &Outcome, image: bool) -> Value {
    let identity=outcome.identity.as_ref().map(|id|json!({"model":id.model,"revision":id.revision,"tail":io::bytes_value(&id.tail),"profile":id.validate().ok().map(|p|p.selector())}));
    let mut report = json!({"operation":format!("{:?}",outcome.operation),"ok":outcome.failure.is_none(),"failure":outcome.failure.as_ref().map(|f|format!("{f:?}")),"result_code":outcome.result_code(),"close_error":outcome.close_error.map(|e|format!("{e:?}")),"identity":identity,"blocks":outcome.blocks,"payload_bytes":outcome.payload_bytes,"requested_wire_bytes":outcome.requested_wire_bytes,"transmitted_wire_bytes":outcome.transmitted_wire_bytes,"received_wire_bytes":outcome.received_wire_bytes,"write_disposition":format!("{:?}",outcome.write_disposition),"terminal_phase":format!("{:?}",outcome.terminal_phase),"automatic_retry":false});
    if image {
        report["image"] = io::bytes_value(&outcome.image);
        report["coverage"] = io::bytes_value(&outcome.coverage);
        report["image_contract"] = json!(
            "Only addresses with set coverage bits are acquired; other bytes are not radio data."
        );
    }
    report
}
