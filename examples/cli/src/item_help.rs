//! Built-in English field help and an optional, read-only PE analysis oracle.
use crate::{
    AppError, Result,
    args::Write,
    fields, io,
    values::{self, ValueCodec},
};
use clap::{Args as ClapArgs, Subcommand};
use cs705::{icf::Icf, item_help as lib};
use serde_json::{Map, Value, json};

#[derive(ClapArgs)]
pub struct Args {
    /// Optional PE analysis oracle; built-in help never reads application assets.
    #[arg(long, requires_all = ["capture", "gui_language", "application_dir_hex"])]
    asset: Option<String>,
    /// Captured native helper state and reached Win32 API outputs; never run PE.
    #[arg(long, requires = "asset")]
    capture: Option<String>,
    /// Actual GUI C+0 DWORD. Exactly1 selects Japanese; system locale is unused.
    #[arg(long, allow_hyphen_values = true, requires = "asset")]
    gui_language: Option<String>,
    /// Actual native application-directory bytes, including its trailing slash.
    #[arg(long, requires = "asset")]
    application_dir_hex: Option<String>,
    /// Optional read-only ICF validation source; no radio fields are modified.
    #[arg(long)]
    image: Option<String>,
    #[command(flatten)]
    write: Write,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Describe a semantic field, or resolve a node in the explicit PE oracle.
    Topic {
        #[arg(long, allow_hyphen_values = true)]
        key: Option<String>,
        /// Existing field catalog ID/path with one unambiguous source_key.
        #[arg(long)]
        field: Option<String>,
        /// Raw node JSON: key, index, context58_hex. No name/topic/url accepted.
        #[arg(long)]
        context: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        index: Option<String>,
        /// Actual context58 bytes before/after a comma; no precomputed group.
        #[arg(long, allow_hyphen_values = true)]
        group: Option<String>,
    },
    /// List built-in field documentation, or the explicit oracle contents route.
    Contents,
}

fn invalid(error: impl std::fmt::Display) -> AppError {
    AppError::invalid(error.to_string())
}
fn object<'a>(value: &'a Value, allowed: &[&str]) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .filter(|v| v.keys().all(|key| allowed.contains(&key.as_str())))
        .ok_or_else(|| invalid("item-help source requires an object with only documented members"))
}
fn number<T: ValueCodec>(o: &Map<String, Value>, key: &str) -> Result<T> {
    T::parse(
        o.get(key)
            .ok_or_else(|| invalid(format!("item-help source missing {key}")))?,
    )
}
fn optional<T: ValueCodec>(o: &Map<String, Value>, key: &str) -> Result<Option<T>> {
    o.get(key).map(T::parse).transpose()
}
fn array<const N: usize>(o: &Map<String, Value>, key: &str) -> Result<Option<[i32; N]>> {
    o.get(key)
        .map(|value| {
            let a = value
                .as_array()
                .filter(|a| a.len() == N)
                .ok_or_else(|| invalid("native POINT/RECT width"))?;
            let mut out = [0; N];
            for (out, value) in out.iter_mut().zip(a) {
                *out = i32::parse(value)?;
            }
            Ok(out)
        })
        .transpose()
}

fn capture(path: &str) -> Result<(lib::State, lib::Api)> {
    let value: Value = serde_json::from_slice(&io::read(path)?).map_err(invalid)?;
    let root = object(&value, &["state", "api"])?;
    let state = object(
        root.get("state")
            .ok_or_else(|| invalid("native helper state required"))?,
        &["module", "procedure", "cookie", "d188", "d184"],
    )?;
    let state = lib::State {
        module: number(state, "module")?,
        procedure: number(state, "procedure")?,
        cookie: number(state, "cookie")?,
        d188: number(state, "d188")?,
        d184: number(state, "d184")?,
    };
    let api = object(
        root.get("api")
            .ok_or_else(|| invalid("native API observations required"))?,
        &[
            "load_library",
            "get_proc_address",
            "initialize_cookie",
            "window_before",
            "old_rect",
            "display_window",
            "file_attributes",
            "window_after",
            "window_info_pointer",
            "window_info_rect",
            "cursor",
            "work_area",
            "style",
        ],
    )?;
    Ok((
        state,
        lib::Api {
            load_library: optional(api, "load_library")?,
            get_proc_address: optional(api, "get_proc_address")?,
            initialize_cookie: optional(api, "initialize_cookie")?,
            window_before: optional(api, "window_before")?,
            old_rect: array(api, "old_rect")?,
            display_window: optional(api, "display_window")?,
            file_attributes: optional(api, "file_attributes")?,
            window_after: optional(api, "window_after")?,
            window_info_pointer: optional(api, "window_info_pointer")?,
            window_info_rect: array(api, "window_info_rect")?,
            cursor: array(api, "cursor")?,
            work_area: array(api, "work_area")?,
            style: optional(api, "style")?,
        },
    ))
}

fn field_key(id: &str) -> Result<i32> {
    let list = fields::list();
    let mut keys = std::collections::BTreeSet::new();
    for field in list.as_array().expect("compiled field catalog") {
        if field["feature_id"].as_str() == Some(id) || field["key"].as_str() == Some(id) {
            let key = field["source_key"]
                .as_u64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(|| {
                    invalid("catalog field has no item-help source key; use actual --key")
                })?;
            keys.insert(key);
        }
    }
    if keys.len() != 1 {
        return Err(invalid(
            "field must identify one catalog source key; use actual --key",
        ));
    }
    Ok(*keys.first().unwrap())
}

fn state_json(state: lib::State) -> Value {
    json!({"module":state.module,"procedure":state.procedure,"cookie":state.cookie,"d188":state.d188,"d184":state.d184})
}
fn effects_json(effects: &lib::Effects) -> Value {
    json!({"calls":effects.calls.iter().map(|c|json!({"api":c.api,"command":c.command})).collect::<Vec<_>>(),
        "show_window_requests":effects.show_window,"placement_request":effects.placement.map(|p|json!({"window":p.window,"x":p.x,"y":p.y,"style":p.style,"set_window_pos_flags":p.flags})),
        "display_window_observed":effects.display_window_observed,"message_resource":effects.message_resource_index.map(|index|json!({"key":"GI_Sys_HtmlHelp_Error","index":index})),"message_icon":effects.message_icon,
        "os_calls_performed":false,"notifications_delivered":false})
}

pub fn run(args: Args) -> Result<u8> {
    if args.asset.is_none() {
        return built_in(args);
    }
    run_oracle(args)
}

fn built_in(args: Args) -> Result<u8> {
    let default = Command::Contents;
    let command = args.command.as_ref().unwrap_or(&default);
    if let Some(path) = &args.image {
        Icf::parse(&io::read(path)?)?;
    }
    let value = match command {
        Command::Contents => json!({"operation":"item-help","language":"English",
            "title":"Configuration field guide",
            "read":"Use display INPUT --field PATH to inspect values without editing the input.",
            "choices":"Use display --choices PATH to inspect schema codes and input ranges.",
            "print":"Use print INPUT --field PATH --yes to approve a print document. No operating-system printer is opened.",
            "safety":"Read-only commands preserve stored values. Edits and publication have separate approval rules.",
            "fields":crate::display::english::catalog()}),
        Command::Topic {
            key,
            field,
            index,
            context,
            group,
        } => {
            if context.is_some()
                || group.is_some()
                || usize::from(key.is_some()) + usize::from(field.is_some()) != 1
            {
                return Err(invalid(
                    "built-in topic requires exactly one --field or --key",
                ));
            }
            let fields = if let Some(field) = field {
                vec![crate::display::english::describe(field)?]
            } else {
                let key: u32 = values::parse(key.as_deref().unwrap())?;
                let catalog = crate::display::english::catalog();
                let matches: Vec<_> = catalog
                    .as_array()
                    .expect("compiled field catalog")
                    .iter()
                    .filter(|f| f["source_key"].as_u64() == Some(u64::from(key)))
                    .cloned()
                    .collect();
                if matches.is_empty() {
                    return Err(invalid("no semantic field has this source key"));
                }
                matches
            };
            let index = index.as_deref().map(values::parse::<usize>).transpose()?;
            json!({"operation":"item-help","language":"English","title":"Field reference","fields":fields,"index":index,
                "read":"Use the listed path with display --field. Supply each required path argument.",
                "choices":"Use display --choices with the listed catalog path. Numeric codes are not translated into guessed captions.",
                "publication":"Inspecting a value does not approve an edit or publish a radio image.",
                "image_modified":false,"application_assets_used":false})
        }
    };
    io::publish(&io::json_bytes(&value)?, &args.write)?;
    Ok(0)
}

fn run_oracle(args: Args) -> Result<u8> {
    let asset = args
        .asset
        .as_deref()
        .ok_or_else(|| invalid("PE oracle source required"))?;
    let capture_path = args
        .capture
        .as_deref()
        .ok_or_else(|| invalid("PE oracle needs --capture"))?;
    let language = args
        .gui_language
        .as_deref()
        .ok_or_else(|| invalid("PE oracle needs --gui-language"))?;
    let directory = args
        .application_dir_hex
        .as_deref()
        .ok_or_else(|| invalid("PE oracle needs --application-dir-hex"))?;
    let default = Command::Contents;
    let command = args.command.as_ref().unwrap_or(&default);
    let context_path = match command {
        Command::Topic { context, .. } => context.as_deref(),
        _ => None,
    };
    io::single_stdin(
        std::iter::once(asset)
            .chain([capture_path])
            .chain(args.image.as_deref())
            .chain(context_path),
    )?;
    let pe = io::read(asset)?;
    if let Some(path) = &args.image {
        Icf::parse(&io::read(path)?)?;
    }
    let (key, index, group) = match command {
        Command::Contents => (None, 0, Vec::new()),
        Command::Topic {
            key,
            field,
            context,
            index,
            group,
        } => {
            if usize::from(key.is_some())
                + usize::from(field.is_some())
                + usize::from(context.is_some())
                != 1
            {
                return Err(invalid(
                    "item-help topic requires exactly one --key/--field/--context source",
                ));
            }
            if let Some(path) = context {
                if index.is_some() || group.is_some() {
                    return Err(invalid("node context cannot be overridden by index/group"));
                }
                let value: Value = serde_json::from_slice(&io::read(path)?).map_err(invalid)?;
                let o = object(&value, &["key", "index", "context58_hex"])?;
                let bytes = o
                    .get("context58_hex")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("node context58_hex required"))?;
                (
                    Some(number::<i32>(o, "key")?),
                    number::<i32>(o, "index")?,
                    io::decode_hex(bytes)?,
                )
            } else {
                let key = if let Some(key) = key {
                    values::parse(key)?
                } else {
                    field_key(field.as_deref().unwrap())?
                };
                let index = values::parse(
                    index
                        .as_deref()
                        .ok_or_else(|| invalid("actual node --index required"))?,
                )?;
                let group = group
                    .as_deref()
                    .ok_or_else(|| invalid("actual context58 --group required; empty is valid"))?;
                if !group.is_ascii() {
                    return Err(invalid(
                        "non-ASCII node context requires captured context58_hex",
                    ));
                }
                (Some(key), index, group.as_bytes().to_vec())
            }
        }
    };
    let input = if let Some(key) = key {
        lib::Input::Item {
            key,
            index,
            context58: &group,
        }
    } else {
        lib::Input::Contents
    };
    let request = lib::resolve(
        &pe,
        input,
        values::parse(language)?,
        &io::decode_hex(directory)?,
    )
    .map_err(invalid)?;
    let (mut state, api) = capture(capture_path)?;
    let before = state;
    let mut effects = lib::Effects::default();
    let status = lib::drive(&request, &mut state, &api, &mut effects);
    let report = json!({"operation":"A2-O046","key":request.key,"index":request.index,"group":request.group,"original_name":io::bytes_value(&request.original_name),"name":io::bytes_value(&request.name),
        "topic":request.topic,"chm":io::bytes_value(&request.chm),"html_help_target":io::bytes_value(&request.target),"status":status.as_ref().ok(),
        "state_before":state_json(before),"state_after":state_json(state),"effects":effects_json(&effects),"read_only_image_supplied":args.image.is_some(),"image_modified":false,"help_window_displayed_by_cli":false,"dry_run":args.write.dry_run});
    match status {
        Ok(_) => {
            io::publish(&io::json_bytes(&report)?, &args.write)?;
            Ok(0)
        }
        Err(error) => {
            io::report(
                &json!({"item_help_prefix":report,"operation_error":error.to_string(),"published":false}),
            )?;
            Err(invalid(error))
        }
    }
}
