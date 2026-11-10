//! CS-705 CLI. Offline edits publish only after success and actual approval.
//! Source-like partial imports are separate and must explicitly accept a prefix.

#![forbid(unsafe_code)]

mod args;
mod csv;
mod display;
mod documents;
mod editors;
mod fields;
mod information;
mod inline_fields;
mod io;
mod item_help;
mod lifecycle;
mod native;
mod printing;
mod records;
mod repeaters;
mod sheets;
mod special;
mod transport;
mod values;
mod wire;

use args::*;
use clap::Parser;
use cs705::icf::{self, Icf};
use serde_json::{Value, json};
use std::process::ExitCode;

type Result<T> = std::result::Result<T, AppError>;

/// Stable process codes: 0 success, 2 grammar, 3 input, 4 I/O, 5 partial, 6 safety.
#[derive(Debug)]
struct AppError {
    code: u8,
    message: String,
}
impl AppError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: 3,
            message: message.into(),
        }
    }
    fn io(message: impl Into<String>) -> Self {
        Self {
            code: 4,
            message: message.into(),
        }
    }
    fn safety(message: impl Into<String>) -> Self {
        Self {
            code: 6,
            message: message.into(),
        }
    }
}
impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::io(format!("I/O failure: {}", error.kind()))
    }
}
macro_rules! data_error {
    ($($ty:ty),+) => {$ (impl From<$ty> for AppError {
        fn from(error: $ty) -> Self { Self::invalid(error.to_string()) }
    })+};
}
data_error!(
    icf::Error,
    cs705::csv::Error,
    cs705::gps::Error,
    cs705::repeater::Error,
    cs705::clone::Error
);

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            let _ = io::report(&json!({"error":error.message,"exit_code":error.code}));
            ExitCode::from(error.code)
        }
    }
}

fn render(value: &Value, format: Format) -> Result<()> {
    match format {
        Format::Json => io::stdout(&io::json_bytes(value)?),
        // JSON quoting is also the terminal escaping boundary. Stored ESC/CR/NUL
        // can never inject terminal controls or silently disappear from a report.
        Format::Text => {
            if let Some(rows) = value.as_array() {
                for row in rows {
                    io::stdout(format!("{row}\n").as_bytes())?;
                }
                Ok(())
            } else {
                io::stdout(format!("{value}\n").as_bytes())
            }
        }
    }
}

fn summary(bytes: &[u8], kind: Kind) -> Result<Value> {
    match kind {
        Kind::Icf => {
            let image = Icf::parse(bytes)?;
            Ok(
                json!({"kind":"icf","memory_bytes":image.memory().len(),"comment":io::bytes_value(image.comment_bytes()),"extra_data":image.extra_data(),"validation":"document structure and checksum; not all settings or radio safety"}),
            )
        }
        Kind::Memory => {
            let image = Icf::from_memory(bytes.to_vec())?;
            Ok(json!({"kind":"memory","memory_bytes":image.memory().len()}))
        }
        Kind::GpsCsv => Ok(
            json!({"kind":"gps-csv","rows":cs705::gps::Csv::parse(bytes)?.rows().len(),"validation":"limited lossless CSV structure"}),
        ),
        Kind::RepeaterCsv => Ok(
            json!({"kind":"repeater-csv","rows":cs705::repeater::Csv::parse(bytes)?.rows().len(),"validation":"limited lossless CSV structure"}),
        ),
    }
}

fn assignments(
    image: &mut Icf<'_>,
    assignments: &[String],
    runtime: &records::Runtime,
) -> Result<bool> {
    let mut publish_allowed = false;
    let mut document =
        icf::document::Document::from_current(image.memory().to_vec(), Vec::new(), 6);
    for assignment in assignments {
        let (key, value) = assignment
            .split_once('=')
            .ok_or_else(|| AppError::invalid("assignment requires KEY=VALUE"))?;
        if key.starts_with("record/native-call/") {
            publish_allowed |= records::native_call_edit(image, key, value, runtime)?;
        } else if key.starts_with("record/native-memory/") {
            if key.ends_with("/split") {
                publish_allowed |= records::native_memory_split_edit(image, key, value, runtime)?;
            } else {
                publish_allowed |= records::native_memory_edit(image, key, value, runtime)?;
            }
        } else if key.starts_with("record/native-ur/") {
            publish_allowed |= records::native_ur_edit(image, key, value, runtime)?;
        } else if key.starts_with("record/repeater/") {
            records::repeater_edit(image, key, value, runtime)?;
            publish_allowed = true;
        } else {
            publish_allowed |= fields::set(image, key, value)?;
        }
        if let Some(mut action) = runtime.take_native_action() {
            document.replace_current(image.memory().to_vec());
            let effects =
                document.finish_action(&mut action.context, true, action.flags, |_, _, _| {
                    Err(icf::Error::Invalid {
                        line: 0,
                        reason: "native callback receiver unavailable in headless edit",
                    })
                })?;
            io::report(&json!({
                "document_tail": {
                    "dirty": effects.dirty,
                    "timer_ms": effects.timer_ms,
                    "callback_invoked": effects.callback_invoked,
                    "publication_authorized": publish_allowed,
                }
            }))?;
        }
    }
    Ok(publish_allowed)
}

fn has_record_options(options: &records::Options) -> bool {
    options.record_context.is_some()
        || options.system_langid.is_some()
        || options.generation.is_some()
        || options.reference_defaults.is_some()
        || options.accept_partial
        || options.normal_state.is_some()
        || options.native_range_context.is_some()
        || options.native_format_context.is_some()
        || !options.inline_unit_output.is_empty()
        || options.normal_options_present()
}

fn run(cli: Cli) -> Result<u8> {
    if cli.config.as_deref() == Some("-") {
        return Err(AppError::invalid(
            "--config requires a regular preference file; use config - show for piped configuration",
        ));
    }
    let preferences = cli
        .config
        .as_deref()
        .map(sheets::load_config)
        .transpose()?
        .unwrap_or_else(|| json!({}));
    let unused_normal_state = match &cli.command {
        Command::Edit {
            assignments,
            record_options,
            ..
        } => {
            record_options.normal_state.is_some()
                && !assignments.iter().any(|assignment| {
                    assignment.split_once('=').is_some_and(|(key, _)| {
                        ["record/native-memory/", "record/native-call/"]
                            .iter()
                            .any(|prefix| key.starts_with(prefix))
                    })
                })
        }
        Command::Row {
            sheet,
            action,
            record_options,
            ..
        } => {
            record_options.normal_state.is_some()
                && !(*sheet == Sheet::Call
                    && matches!(action, RowAction::Paste { .. } | RowAction::Move { .. }))
        }
        Command::Show { record_options, .. } | Command::Export { record_options, .. } => {
            record_options.normal_state.is_some()
        }
        _ => false,
    };
    if unused_normal_state {
        return Err(AppError::invalid(
            "normal-state requires a native channel field edit or call row paste/carry",
        ));
    }
    match cli.command {
        Command::Document(args) => return documents::run(args),
        Command::Lifecycle(args) => return lifecycle::run(args),
        Command::Display(args) => {
            display::run(args, &preferences)?;
            return Ok(0);
        }
        Command::New(args) => return documents::new(args),
        Command::Editor(args) => return editors::run(args),
        Command::Repeater(args) => return repeaters::run(args),
        Command::Special(args) => return special::run(args),
        Command::Recent(args) => {
            return lifecycle::recent(args, &preferences, cli.config.as_deref());
        }
        Command::Parse {
            read,
            source_reader,
            seed,
        } => {
            if source_reader {
                if read.kind != Kind::Icf {
                    return Err(AppError::invalid("source reader requires --kind icf"));
                }
                let seed =
                    seed.ok_or_else(|| AppError::invalid("source reader requires --seed"))?;
                io::single_stdin([read.input.as_str(), seed.as_str()])?;
                let mut image = io::read(&seed)?;
                if image.len() != cs705::clone::IMAGE_CAPACITY {
                    return Err(AppError::invalid(
                        "source reader seed requires exactly 0x4997a bytes",
                    ));
                }
                let mut reader = icf::text::Reader::default();
                let report = reader.read(&mut image, &io::read(&read.input)?, None)?;
                render(
                    &json!({"reader":"source","status":format!("{:?}",report.status),"source_status":report.status as u8,"records":report.records,"bytes_written":report.bytes_written,"coverage":io::bytes_value(&report.coverage),"model":reader.model,"file_revision":reader.file_revision,"map_revision":reader.map_revision,"extra_data":reader.extra_data,"checksum_validated":false}),
                    read.format,
                )?;
                return Ok(if report.status == icf::text::Status::Success {
                    0
                } else {
                    3
                });
            } else {
                render(&summary(&io::read(&read.input)?, read.kind)?, read.format)?;
            }
        }
        Command::Check {
            inputs,
            kind,
            format,
        } => {
            io::single_stdin(inputs.iter().map(String::as_str))?;
            let mut failures = 0;
            let mut failure_codes = std::collections::BTreeSet::new();
            let mut results = Vec::new();
            for (index, input) in inputs.iter().enumerate() {
                match io::read(input).and_then(|b| summary(&b, kind)) {
                    Ok(result) => {
                        results.push(json!({"input_index":index,"ok":true,"result":result}))
                    }
                    Err(error) => {
                        failures += 1;
                        failure_codes.insert(error.code);
                        results.push(json!({"input_index":index,"ok":false,"error":error.message,"exit_code":error.code}));
                    }
                }
            }
            render(
                &json!({"results":results,"passed":inputs.len()-failures,"failed":failures}),
                format,
            )?;
            return Ok(if failures == 0 {
                0
            } else if failures < inputs.len() {
                5
            } else if failure_codes.len() == 1 {
                *failure_codes
                    .first()
                    .expect("failed inputs have an error code")
            } else {
                // Mixed failure classes remain distinct from a uniform data/I/O error.
                5
            });
        }
        Command::Show {
            read,
            key,
            list_fields,
            sheet,
            columns,
            start,
            count,
            record_options,
        } => {
            if record_options.normal_options_present() {
                return Err(AppError::invalid(
                    "normal selection options require a native row operation",
                ));
            }
            if record_options.accept_partial {
                return Err(AppError::invalid(
                    "accept-partial requires a native repeater or UR row operation",
                ));
            }
            if list_fields {
                render(&fields::list(), read.format)?;
            } else {
                let runtime = record_options.load([read.input.as_str()])?;
                let bytes = io::read(&read.input)?;
                if key.is_empty() {
                    let view = match read.kind {
                        Kind::Icf | Kind::Memory => {
                            if let Some(sheet) = sheet {
                                let image = if read.kind == Kind::Icf {
                                    Icf::parse(&bytes)?
                                } else {
                                    Icf::from_memory(bytes.clone())?
                                };
                                sheets::view(&image, sheet, start, count, Some(&runtime))?
                            } else {
                                summary(&bytes, read.kind)?
                            }
                        }
                        _ => sheets::csv_view(&bytes, read.kind, start, count)?,
                    };
                    let view = sheets::present(view, &columns, sheet, &preferences)?;
                    sheets::toolbar(&preferences)?;
                    if read.format == Format::Text {
                        sheets::render_text(&view, &preferences)?;
                    } else {
                        render(&view, read.format)?;
                    }
                } else {
                    let image = match read.kind {
                        Kind::Icf => Icf::parse(&bytes)?,
                        Kind::Memory => Icf::from_memory(bytes.clone())?,
                        _ => {
                            return Err(AppError::invalid(
                                "field paths require --kind icf or memory",
                            ));
                        }
                    };
                    let mut results = Vec::new();
                    let mut failures = 0;
                    for key in key {
                        let result = if key.starts_with("record/repeater/") {
                            records::repeater_get(&image, &key, &runtime)
                        } else if ["record/from-dv/", "record/from-fm/", "record/to/"]
                            .iter()
                            .any(|prefix| key.starts_with(prefix))
                        {
                            records::history_get(&image, &key, &runtime)
                        } else {
                            fields::get(&image, &key)
                        };
                        match result {
                            Ok(value) => results.push(json!({"key":key,"value":value})),
                            Err(error) => {
                                failures += 1;
                                results.push(json!({"key":key,"error":error.message}));
                            }
                        }
                    }
                    render(&json!(results), read.format)?;
                    return Ok(if failures == 0 { 0 } else { 5 });
                }
            }
        }
        Command::Edit {
            read,
            assignments: edits,
            events,
            write,
            record_options,
        } => {
            if record_options.normal_options_present() {
                return Err(AppError::invalid(
                    "normal selection options require a native row operation",
                ));
            }
            if record_options.accept_partial {
                return Err(AppError::invalid(
                    "edit never publishes a partial image; accept-partial requires a native row operation",
                ));
            }
            let runtime = record_options
                .load([read.input.as_str()])?
                .with_input_options(display::input_options(&preferences)?);
            let bytes = io::read(&read.input)?;
            let result = match read.kind {
                Kind::Icf => {
                    let mut image = Icf::parse(&bytes)?;
                    let mut publish_allowed = assignments(&mut image, &edits, &runtime)?;
                    for event in &events {
                        publish_allowed |= fields::select(&mut image, event)?;
                    }
                    if !publish_allowed {
                        return Ok(0);
                    }
                    image.to_bytes()
                }
                Kind::Memory => {
                    let mut image = Icf::from_memory(bytes)?;
                    let mut publish_allowed = assignments(&mut image, &edits, &runtime)?;
                    for event in &events {
                        publish_allowed |= fields::select(&mut image, event)?;
                    }
                    if !publish_allowed {
                        return Ok(0);
                    }
                    image.memory().to_vec()
                }
                _ => {
                    if !events.is_empty() {
                        return Err(AppError::invalid(
                            "selector events require an ICF or memory image",
                        ));
                    }
                    sheets::edit_csv(&bytes, read.kind, &edits)?
                }
            };
            io::publish(&result, &write)?;
        }
        Command::Convert {
            input,
            conversion,
            source_profile,
            destination_profile,
            seed,
            write,
        } => {
            if conversion != Conversion::Profile
                && (source_profile.is_some() || destination_profile.is_some() || seed.is_some())
            {
                return Err(AppError::invalid(
                    "profile/seed arguments require --conversion profile",
                ));
            }
            io::single_stdin(std::iter::once(input.as_str()).chain(seed.as_deref()))?;
            let bytes = io::read(&input)?;
            let result = match conversion {
                Conversion::Canonicalize => Icf::parse(&bytes)?.to_bytes(),
                Conversion::Rev5To6 => icf::convert_map_rev5_to6(&bytes)?,
                Conversion::IcfToMemory => Icf::parse(&bytes)?.memory().to_vec(),
                Conversion::MemoryToIcf => Icf::from_memory(bytes)?.to_bytes(),
                Conversion::Profile => {
                    let from = source_profile.ok_or_else(|| {
                        AppError::invalid("profile conversion requires --source-profile")
                    })?;
                    let to = destination_profile.ok_or_else(|| {
                        AppError::invalid("profile conversion requires --destination-profile")
                    })?;
                    let seed = seed.as_deref().map(io::read).transpose()?;
                    cs705::clone::convert_image(
                        &bytes,
                        from.try_into()?,
                        to.try_into()?,
                        seed.as_deref(),
                    )?
                }
            };
            io::publish(&result, &write)?;
        }
        Command::Group {
            input,
            sheet,
            action,
        } => {
            let bytes = io::read(&input)?;
            let mut image = Icf::parse(&bytes)?;
            match action {
                GroupAction::List => render(&sheets::groups(&image, sheet)?, Format::Json)?,
                GroupAction::Add { name, write } => {
                    if sheet != Sheet::Memory {
                        return Err(AppError::invalid("group add requires the memory sheet"));
                    }
                    let mut model = icf::rows::NormalMemory::from_icf(&image)?;
                    let index = model.group_count();
                    if let Some(name) = name {
                        if !sheets::edit_memory_group_name(&mut model, index, &name)? {
                            return Ok(0);
                        }
                    } else {
                        if !model.add_group() {
                            return Ok(0);
                        }
                    }
                    io::publish(&model.to_icf()?.to_bytes(), &write)?;
                }
                GroupAction::Rename { index, name, write } => {
                    match sheet {
                        Sheet::Memory => {
                            let mut model = icf::rows::NormalMemory::from_icf(&image)?;
                            if !sheets::edit_memory_group_name(&mut model, index, &name)? {
                                return Ok(0);
                            }
                            io::publish(&model.to_icf()?.to_bytes(), &write)?;
                            return Ok(0);
                        }
                        Sheet::Gps => {
                            if !sheets::edit_gps_group_name(&mut image, index, &name)? {
                                return Ok(0);
                            }
                        }
                        Sheet::Repeater => {
                            if !repeaters::rename_group(&mut image, index, &name)? {
                                return Ok(0);
                            }
                        }
                        _ => return Err(AppError::invalid("sheet does not have named groups")),
                    }
                    io::publish(&image.to_bytes(), &write)?;
                }
            }
        }
        Command::Row {
            input,
            sheet,
            action,
            mut record_options,
        } => {
            if sheet == Sheet::Memory
                || matches!(sheet, Sheet::Bandstack | Sheet::Memopad)
                    && matches!(&action, RowAction::Copy { .. })
            {
                return records::normal_row(&input, sheet, action, &record_options);
            }
            if sheet == Sheet::Repeater {
                return records::repeater_row(&input, action, &record_options);
            }
            if sheet == Sheet::Call {
                return records::call_row(&input, action, &record_options);
            }
            if sheet == Sheet::Gps {
                return sheets::gps_row(&input, action, &record_options);
            }
            if sheet == Sheet::Ur {
                return records::ur_row(&input, action, &record_options);
            }
            let end_index =
                if matches!(sheet, Sheet::FromDv | Sheet::FromFm | Sheet::To | Sheet::Rx)
                    && matches!(&action, RowAction::Delete { .. })
                {
                    record_options.end_index.take()
                } else {
                    None
                };
            if has_record_options(&record_options) {
                return Err(AppError::invalid(
                    "record runtime options require a supported native row operation",
                ));
            }
            sheets::row(&input, sheet, action, end_index)?;
        }
        Command::Import {
            input,
            csv,
            sheet,
            group,
            mode,
            accept_partial,
            yes,
            write,
            csv_options,
        } => {
            return crate::csv::import(
                &input,
                &csv,
                sheet,
                group,
                mode,
                accept_partial,
                yes,
                &write,
                &csv_options,
            );
        }
        Command::Export {
            input,
            sheet,
            group,
            format,
            write,
            csv_options,
            record_options,
        } => {
            if format == ExportFormat::Csv {
                if has_record_options(&record_options) {
                    return Err(AppError::invalid(
                        "CSV runtime sources belong in --csv-context",
                    ));
                }
                crate::csv::export(&input, sheet, group, &write, &csv_options)?;
            } else {
                if record_options.normal_options_present() {
                    return Err(AppError::invalid(
                        "normal selection options require a native row operation",
                    ));
                }
                if csv_options.csv_context.is_some() {
                    return Err(AppError::invalid("csv-context requires --format csv"));
                }
                if record_options.accept_partial {
                    return Err(AppError::invalid("export never publishes a partial view"));
                }
                sheets::export(&input, sheet, group, format, &write, &record_options)?;
            }
        }
        Command::Print(args) => return printing::run(args, &preferences),
        Command::Information(args) => return information::run(args),
        Command::ItemHelp(args) => return item_help::run(args),
        Command::Clone { action } => return wire::run(action),
        Command::Config { file, action } => sheets::config(&file, action)?,
    }
    Ok(0)
}

fn confirm(yes: bool) -> Result<()> {
    if yes {
        Ok(())
    } else {
        Err(AppError::safety(
            "destructive operation requires --yes; no interactive stdin confirmation is used",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_option_detection_includes_inline_sources() {
        assert!(!has_record_options(&records::Options::default()));
        for options in [
            records::Options {
                native_range_context: Some("unused".into()),
                ..Default::default()
            },
            records::Options {
                native_format_context: Some("unused".into()),
                ..Default::default()
            },
            records::Options {
                inline_unit_output: vec!["1=41".into()],
                ..Default::default()
            },
        ] {
            assert!(has_record_options(&options));
        }
    }
}
