//! Digital native field adapters; shared CLI helpers own input and publication.
use super::{InputValue, Publication, TextAction, finish, load, nls_outputs, text_action};
use crate::{
    AppError, Result,
    args::{Format, Read},
    confirm, fields, io, render, values,
};
use clap::{Args as ClapArgs, Subcommand};
use cs705::icf::settings::{
    digital as lib,
    extended::{ChoiceInput, FixedTextInput},
};
use serde_json::json;

#[derive(ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Fields {
        #[arg(long, value_enum, default_value = "json")]
        format: Format,
    },
    /// Explicit machine path: my-call-selected/0..5 or tx-message-selected/0..4.
    #[command(name = "read")]
    Inspect {
        #[command(flatten)]
        read: Read,
        field: String,
    },
    Select {
        #[command(flatten)]
        read: Read,
        field: String,
        #[command(flatten)]
        value: InputValue,
        #[command(flatten)]
        publication: Publication,
    },
    /// Shared native phase 1/2 text input, not a raw byte setter.
    Edit {
        #[command(flatten)]
        read: Read,
        field: String,
        #[command(flatten)]
        value: InputValue,
        /// Actual VA 864634 DWORD, required for phase 1 main/allowed callsigns.
        #[arg(long)]
        alignment: Option<u32>,
        /// Actual native input code; required only for allowed-call.
        #[arg(long, allow_hyphen_values = true)]
        input_code: Option<i32>,
        /// Actual LCMap 2n and optional Upper n API outputs, not a field candidate.
        #[arg(long)]
        nls_context: Option<String>,
        #[command(flatten)]
        publication: Publication,
    },
}
fn path(path: &str) -> Result<(&str, usize)> {
    let (name, index) = path
        .rsplit_once('/')
        .ok_or_else(|| AppError::invalid("digital path requires explicit native /index"))?;
    Ok((name, values::parse(index)?))
}
fn allowed_index(index: usize) -> Result<u32> {
    u32::try_from(index).map_err(|_| AppError::invalid("allowed index exceeds native DWORD"))
}
fn native_error(error: cs705::csv::Error) -> AppError {
    AppError::invalid(error.to_string())
}
fn allowed_json(view: &lib::AllowedView) -> serde_json::Value {
    json!({"feature_id":"A2-H55994-01","prefix_count":view.prefix_count,"view_bad":view.bad,"duplicate_warning":view.duplicate_warning,"parent_enable_raw":view.parent_enable_raw,"parent_enable_is_edit_gate":false,"raw":io::bytes_value(&view.raw),"display":io::bytes_value(&view.display)})
}
pub fn run(args: Args) -> Result<u8> {
    match args.command {
        Command::Fields { format } => {
            let mut entries:Vec<_>=lib::SelectionField::ALL.into_iter().map(|f|json!({"feature_id":f.feature_id(),"path":format!("{}/<index>",f.name()),"native_key":f.key(),"selection_key":1,"index_base":0,"count":f.count(),"active_byte_base":f.active_base(),"operation_connected":true})).collect();
            entries.extend(lib::TextField::ALL.into_iter().map(|f|json!({"feature_id":f.feature_id(),"path":format!("{}/<index>",f.name()),"native_key":f.key(),"index_base":0,"count":f.count(),"width":f.width(),"operation_connected":true})));
            entries.push(json!({"feature_id":"A2-H55994-01","path":"allowed-call/<index>","native_key":0x1ae,"index_base":0,"capacity":30,"additional_row":"current prefix count, including row30 when full","width":8,"operation_connected":true}));
            entries.push(json!({"feature_id":"A2-H55830-01","path":"reflector/<index>","native_key":0x18c,"index_base":0,"count":5,"width":8,"read_only":true,"source":"current image kind14 history","operation_connected":true}));
            render(&json!(entries), format)?;
            Ok(0)
        }
        Command::Inspect { read, field } => {
            io::single_stdin([read.input.as_str()])?;
            let bytes = io::read(&read.input)?;
            let image = load(&bytes, read.kind)?;
            let (name, index) = path(&field)?;
            let mut view = if name == "allowed-call" || name == "A2-H55994-01" {
                allowed_json(
                    &lib::allowed_view(&image, allowed_index(index)?).map_err(native_error)?,
                )
            } else if matches!(name, "reflector" | "A2-H55830-01") {
                let view = lib::reflector_view(&image, allowed_index(index)?)?;
                json!({"feature_id":"A2-H55830-01","valid":view.valid,"raw":view.raw.as_ref().map(|raw|io::bytes_value(raw)),"display":io::bytes_value(&view.display)})
            } else if let Ok(field) = lib::SelectionField::find(name) {
                json!({"feature_id":field.feature_id(),"selected":field.selected(&image,index).map_err(native_error)?,"active_raw":field.active_raw(&image),"active_byte_base":field.active_base()})
            } else {
                let field = lib::TextField::find(name).map_err(native_error)?;
                json!({"feature_id":field.feature_id(),"raw":io::bytes_value(field.raw(&image,index).map_err(native_error)?),"display":io::bytes_value(&field.display(&image,index).map_err(native_error)?)})
            };
            view["native_index"] = json!(index);
            view["read_only"] = json!(true);
            render(&view, read.format)?;
            Ok(0)
        }
        Command::Select {
            read,
            field,
            value,
            publication,
        } => {
            io::single_stdin(
                std::iter::once(read.input.as_str()).chain(value.value_file.as_deref()),
            )?;
            if publication.accept_partial {
                confirm(publication.yes)?;
            }
            let text = value.read()?;
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let result = (|| {
                let (name, index) = path(&field)?;
                let field = lib::SelectionField::find(name).map_err(native_error)?;
                let input = fields::choice_input(&text)?;
                let effect = field.edit(&mut image, index, input).map_err(native_error)?;
                let report = json!({"feature_id":field.feature_id(),"native_index":index,"helper_accepted":effect.helper_accepted,"stored":effect.stored,"candidate":effect.candidate,"active_raw":effect.active_raw,"active_byte_base":field.active_base(),"notify_flags":effect.notify_flags,"popup_requested":effect.popup_requested});
                if !effect.helper_accepted
                    && !matches!(
                        input,
                        ChoiceInput::Cancel | ChoiceInput::Display | ChoiceInput::Begin
                    )
                {
                    io::report(&json!({"editor":report,"published":false}))?;
                    return Err(AppError::invalid(
                        "native digital selection event was not accepted",
                    ));
                }
                Ok((report, effect.helper_accepted))
            })();
            finish(&image, &read, &publication, &before, result)
        }
        Command::Edit {
            read,
            field,
            value,
            alignment,
            input_code,
            nls_context,
            publication,
        } => {
            io::single_stdin(
                std::iter::once(read.input.as_str())
                    .chain(value.value_file.as_deref())
                    .chain(nls_context.as_deref()),
            )?;
            if publication.accept_partial {
                confirm(publication.yes)?;
            }
            let (name, index) = path(&field)?;
            let allowed = matches!(name, "allowed-call" | "A2-H55994-01");
            let field = if allowed {
                None
            } else {
                Some(lib::TextField::find(name).map_err(native_error)?)
            };
            let action = text_action(&value.read()?)?;
            let alignment = if matches!(action, TextAction::Text(_, 1))
                && (allowed || field == Some(lib::TextField::MyCall))
            {
                alignment.ok_or_else(|| {
                    AppError::invalid(
                        "phase1 main/allowed callsign requires actual --alignment DWORD",
                    )
                })?
            } else {
                alignment.unwrap_or(0)
            };
            let code = if allowed {
                Some(input_code.ok_or_else(|| {
                    AppError::invalid("allowed-call requires actual --input-code")
                })?)
            } else {
                if input_code.is_some() {
                    return Err(AppError::invalid(
                        "input-code is only consumed by allowed-call",
                    ));
                }
                None
            };
            let typed = matches!(action, TextAction::Text(..));
            if nls_context.is_some() && !typed {
                return Err(AppError::invalid("NLS context requires a text invocation"));
            }
            let outputs = if let (Some(path), TextAction::Text(source, _)) =
                (nls_context.as_deref(), &action)
            {
                Some(nls_outputs(path, source.len())?)
            } else {
                None
            };
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let mut source = Vec::new();
            let input = match action {
                TextAction::Text(bytes, phase) => {
                    source = bytes;
                    FixedTextInput::Text {
                        source: &mut source,
                        phase,
                    }
                }
                TextAction::Display => FixedTextInput::Display,
                TextAction::Cancel => FixedTextInput::Cancel,
                TextAction::Begin => FixedTextInput::Begin,
            };
            let stage = std::cell::Cell::new(0);
            let map = |from: &[u8], to: &mut [u8]| {
                if let Some(outputs) = &outputs {
                    if stage.get() != 0 || to.len() != outputs.map.bytes.len() {
                        return Err(cs705::csv::Error::Invalid(
                            "LCMap output stage/width mismatch".into(),
                        ));
                    }
                    to.copy_from_slice(&outputs.map.bytes);
                    stage.set(1);
                    Ok(outputs.map.result)
                } else {
                    cs705::csv::nls::platform_map(from, to)
                }
            };
            let upper = |to: &mut [u8]| {
                if let Some(outputs) = &outputs {
                    let upper =
                        outputs
                            .upper
                            .as_ref()
                            .ok_or(cs705::csv::Error::SourceUnavailable(
                                "actual CharUpperBuffA output required".into(),
                            ))?;
                    if stage.get() != 1 || to.len() != upper.bytes.len() {
                        return Err(cs705::csv::Error::Invalid(
                            "Upper output stage/width mismatch".into(),
                        ));
                    }
                    to.copy_from_slice(&upper.bytes);
                    stage.set(2);
                    Ok(upper.result)
                } else {
                    cs705::csv::nls::platform_uppercase(to)
                }
            };
            let result = if let Some(code) = code {
                let mut effect = lib::AllowedEffect::default();
                let result = lib::edit_allowed_with_nls(
                    &mut image,
                    allowed_index(index)?,
                    code,
                    input,
                    alignment,
                    &mut effect,
                    map,
                    upper,
                );
                let report = json!({"feature_id":"A2-H55994-01","native_index":index,"stored":effect.stored,"inserted":effect.inserted,"full_warning":effect.full_warning,"view_bad":effect.view_bad,"duplicate_warning":effect.duplicate_warning,"notify_flags":effect.notify_flags,"notifications":effect.notifications,"notifications_delivered":false,"state_out":effect.state_out,"display":io::bytes_value(&effect.display)});
                io::report(&json!({"native_effect":report}))?;
                result
                    .map_err(|error| AppError::invalid(error.to_string()))
                    .and_then(|()| {
                        if typed && !effect.stored {
                            return Err(AppError::invalid(
                                "native allowed callsign editor did not accept text",
                            ));
                        }
                        Ok((report, typed && effect.stored))
                    })
            } else {
                let field = field.unwrap();
                field.edit_with_nls(&mut image,index,input,alignment,map,upper).map_err(|error|AppError::invalid(error.to_string())).map(|effect| {
                    (json!({"feature_id":field.feature_id(),"native_index":index,"stored":effect.stored,"notify_flags":effect.notify_flags,"state_out":effect.state_out,"display":io::bytes_value(&effect.display)}),typed && effect.stored)
                })
            };
            if typed {
                io::report(&json!({"shared_source_after":io::bytes_value(&source)}))?;
            }
            finish(&image, &read, &publication, &before, result)
        }
    }
}
