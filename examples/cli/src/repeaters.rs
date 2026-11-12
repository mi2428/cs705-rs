//! Dedicated native repeater editor. No row/CSV/transport implementation lives here.
use crate::{
    AppError, Result,
    args::Write,
    confirm, fields, io, native,
    values::{self, ValueCodec},
};
use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use cs705::{
    csv,
    icf::{
        Icf,
        record::{self, ChannelField as C, DecodedRecord, RepeaterField as R, RepeaterText as T},
        settings::extended::{ChoiceInput, TextInput},
    },
    repeater::{
        editor::{self as lib, ChoiceField as S, NumberField as N, TextField as X},
        working::{DefaultContext, Working},
    },
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::VecDeque};

#[derive(ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// The 17 native grid fields; Index is read-only, Type's menu is 0..3.
    Fields,
    /// Read one absolute working row through saved order and indirect gateways.
    Read {
        input: String,
        index: usize,
        #[arg(long)]
        range_context: String,
    },
    /// Set USE through the shared typed bulk operation and live B/Q getters.
    BulkUse {
        input: String,
        #[arg(value_enum)]
        value: Use,
        #[arg(long)]
        range_context: String,
        /// Owning view group; omission uses the native zero descriptor.
        #[arg(long)]
        group: Option<usize>,
        /// Cancel before reading inputs or touching the output.
        #[arg(long)]
        cancel: bool,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        write: Write,
    },
    /// Execute repeated FIELD=INPUT in native order, then compress once.
    Edit {
        input: String,
        index: usize,
        #[arg(long)]
        range_context: String,
        /// Owning native view group, needed for an empty Type Enter insertion.
        #[arg(long)]
        group: Option<usize>,
        #[arg(long = "set", required = true)]
        assignments: Vec<String>,
        /// Actual expanded G; only reached reference callbacks read its words.
        #[arg(long)]
        generation: Option<String>,
        /// Optional ordered captured LCMap/CharUpper outputs, never holder data.
        #[arg(long)]
        nls_source: Option<String>,
        /// Actual 864634 option for last-word callsign placement.
        #[arg(long)]
        align_callsign: bool,
        #[arg(long, requires = "yes")]
        accept_partial: bool,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        write: Write,
    },
}

#[derive(ValueEnum, Clone, Copy)]
enum Use {
    Yes,
    No,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum Field {
    Index,
    Type,
    Name,
    SubName,
    RepeaterCall,
    GatewayCall,
    Frequency,
    Duplex,
    Offset,
    Mode,
    Tone,
    ToneFrequency,
    Use,
    Position,
    Latitude,
    Longitude,
    UtcOffset,
}
impl Field {
    fn selector(self) -> Option<S> {
        Some(match self {
            Self::Type => S::Type,
            Self::Mode => S::Mode,
            Self::Duplex => S::Duplex,
            Self::Tone => S::Tone,
            Self::Use => S::Use,
            Self::Position => S::Position,
            _ => return None,
        })
    }
    fn numeric(self) -> Option<N> {
        Some(match self {
            Self::Frequency => N::Frequency,
            Self::Offset => N::Offset,
            Self::ToneFrequency => N::ToneFrequency,
            Self::UtcOffset => N::UtcOffset,
            _ => return None,
        })
    }
    fn text(self) -> Option<X> {
        Some(match self {
            Self::Name => X::Name,
            Self::SubName => X::SubName,
            Self::RepeaterCall => X::Call,
            Self::GatewayCall => X::Gateway,
            _ => return None,
        })
    }
}
fn catalog() -> Value {
    json!(Field::value_variants().iter().map(|&f|json!({"field":f.to_possible_value().expect("field").get_name(),"read_only":matches!(f,Field::Index),
    "input":if f.selector().is_some(){"i32 selected candidate or choice action JSON"}else if f.numeric().is_some(){"native editor text or number action JSON"}else{"ASCII text, exact input hex or text action JSON"},
    "type_menu":if matches!(f,Field::Type){Some([0,1,2,3])}else{None},
    "stage":if matches!(f,Field::Latitude|Field::Longitude){"native ASCII keyboard mask/validator/modal callbacks and phase2/save; explicit phase2 action is low-level"}else if matches!(f,Field::Index){"read-only native grid index, not a CSV Group No"}else{"native field invocation and typed save/reload; GUI delivery is request-only"},
    "frequency_units":if matches!(f,Field::Frequency|Field::Offset){Some("MHz input / Hz stored")}else{None}})).collect::<Vec<_>>())
}
fn row(w: &Working, index: usize) -> Result<DecodedRecord> {
    let mut h = record::reset_holder(0x0c, index as u32, 0);
    w.load_into(index, &mut h)?;
    Ok(h)
}
fn view(h: &DecodedRecord) -> Value {
    json!({"index":h.index(),"group":h.group(),"valid":h.valid(),"type":cs705::repeater::staging::classify_bytes(h.as_bytes()),
    "name":io::bytes_value(h.name()),"sub-name":io::bytes_value(h.repeater_text(T::SubName)),"repeater-call":io::bytes_value(h.repeater_text(T::Call)),"gateway-call":io::bytes_value(h.repeater_text(T::Gateway)),
    "frequency":h.channel_field(C::FrequencyHz),"duplex":h.channel_field(C::Duplex),"offset":h.offset_hz(),"mode":h.repeater_field(R::Mode),"tone":h.repeater_field(R::Tone),"tone-frequency":h.repeater_field(R::ToneIndex),
    "use":h.repeater_field(R::Use),"position":h.repeater_field(R::Position),"latitude":io::bytes_value(h.coordinate_bytes(false)),"longitude":io::bytes_value(h.coordinate_bytes(true)),"utc-offset":h.repeater_field(R::UtcOffset) as i32})
}
fn effect(e: &lib::Effect) -> Value {
    json!({"changed":e.native.changed,"flags":e.native.flags,"warnings":e.native.warning_flags,"save_requested":e.native.save_requested,"save_returned":e.native.save_returned,
    "state_out":e.state_out,"popup_requested":e.popup_requested,"blocked":e.blocked,"gray":e.gray,"display":io::bytes_value(&e.display),"source_error":e.source_error,"inserted":e.inserted,
    "helper_accepted":e.helper_accepted,"candidate":e.candidate,"shared_input":io::bytes_value(&e.shared_input),
    "group_label":e.group_label.as_ref().map(|(node,text)|json!({"node":node,"text":io::bytes_value(text),"delivery":"request-only"})),
    "refresh_if_binding_matches":e.refresh_if_binding_matches,
    "coordinate_dialog":e.coordinate_dialog.as_ref().map(|d|json!({"mode":d.mode,"mask":io::bytes_value(&d.mask),"seed":io::bytes_value(&d.seed),"control":io::bytes_value(&d.control),"valid":d.valid(),"phase2":d.phase2.as_ref().map(|text|io::bytes_value(text)),"cancel":d.cancel,"done":d.done,"release_requested":d.release_requested,"delivery":"request-only"})),
    "notifications":e.notifications.iter().map(|&(id,synchronous)|json!({"id":id,"synchronous":synchronous,"delivery":"request-only"})).collect::<Vec<_>>()})
}

pub(crate) fn text_input<T>(
    text: &str,
    apply: impl FnOnce(TextInput<'_>) -> Result<T>,
) -> Result<T> {
    if !values::json_view(text).starts_with('{') {
        let s = values::parse::<String>(text)?;
        if !s.is_ascii() {
            return Err(AppError::invalid(
                "native text requires ASCII or explicit input hex",
            ));
        }
        return apply(TextInput::Text(s.as_bytes()));
    }
    let v: Value = serde_json::from_str(text)
        .map_err(|_| AppError::invalid("invalid native text action JSON"))?;
    let object = v
        .as_object()
        .ok_or_else(|| AppError::invalid("text action requires object"))?;
    let action = v["action"]
        .as_str()
        .ok_or_else(|| AppError::invalid("text action requires action"))?;
    let allowed: &[&str] = match action {
        "text" => &["action", "text", "hex"],
        "begin" => &["action", "first"],
        "display" | "cancel" => &["action"],
        _ => return Err(AppError::invalid("unknown text action")),
    };
    if object.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(AppError::invalid("unknown text action member"));
    }
    match action {
        "text" => match (v.get("text"), v.get("hex")) {
            (Some(Value::String(s)), None) if s.is_ascii() => apply(TextInput::Text(s.as_bytes())),
            (None, Some(Value::String(s))) => {
                let bytes = io::decode_hex(s)?;
                apply(TextInput::Text(&bytes))
            }
            _ => Err(AppError::invalid(
                "text requires exactly one ASCII text or hex input",
            )),
        },
        "begin" => {
            let first = v.get("first").map(u8::parse).transpose()?;
            if first.is_some_and(|b| !b.is_ascii()) {
                return Err(AppError::invalid("begin first requires ASCII byte"));
            }
            apply(TextInput::Begin(first))
        }
        "display" => apply(TextInput::Display),
        "cancel" => apply(TextInput::Cancel),
        _ => unreachable!(),
    }
}

/// Delegate the existing group/repeater/rename command; no second command family.
pub fn rename_group(image: &mut Icf<'_>, group: usize, value: &str) -> Result<bool> {
    let mut e = lib::Effect::default();
    text_input(value, |input| {
        lib::group_name(image, group, input, &mut e)
            .map_err(|error| AppError::invalid(error.to_string()))
    })?;
    io::report(
        &json!({"operation":"native-repeater-group-name","group":group,"effect":effect(&e)}),
    )?;
    Ok(e.native.save_returned)
}

fn coordinate_edit(
    w: &mut Working,
    h: &mut DecodedRecord,
    longitude: bool,
    value: &str,
    r: &lib::Runtime<'_>,
    e: &mut lib::Effect,
) -> Result<()> {
    use csv::number::CoordinateEvent;
    let mut events = vec![CoordinateEvent::Accept];
    let mut low_level = false;
    let normalized = if values::json_view(value).starts_with('{') {
        let mut v: Value = serde_json::from_str(value)
            .map_err(|_| AppError::invalid("invalid coordinate action JSON"))?;
        if v["action"] == "phase2" {
            low_level = true;
            v["action"] = json!("text");
        } else if v["action"] == "dialog" {
            let object = v
                .as_object_mut()
                .ok_or_else(|| AppError::invalid("coordinate action requires object"))?;
            let list = object
                .remove("events")
                .ok_or_else(|| AppError::invalid("coordinate dialog requires events"))?;
            events = list
                .as_array()
                .ok_or_else(|| AppError::invalid("coordinate events require array"))?
                .iter()
                .map(|event| {
                    Ok(match event.as_str() {
                        Some("accept") => CoordinateEvent::Accept,
                        Some("cancel") => CoordinateEvent::Cancel,
                        Some("escape-keypress") => CoordinateEvent::EscapeKeyPress,
                        Some("deactivate") => CoordinateEvent::Deactivate,
                        Some("close") => CoordinateEvent::Close,
                        _ => return Err(AppError::invalid("unknown coordinate event")),
                    })
                })
                .collect::<Result<_>>()?;
            v["action"] = json!("text");
        }
        Some(
            serde_json::to_string(&v)
                .map_err(|_| AppError::invalid("coordinate action serialization"))?,
        )
    } else {
        None
    };
    text_input(normalized.as_deref().unwrap_or(value), |input| {
        if low_level {
            lib::coordinate(w, h, longitude, input, r, e)
        } else {
            lib::coordinate_input(w, h, longitude, input, &events, r, e)
        }
        .map_err(|error| AppError::invalid(error.to_string()))
    })
}

struct NlsSource {
    events: Option<VecDeque<Value>>,
    used: usize,
}
impl NlsSource {
    fn read(path: Option<&str>) -> Result<Self> {
        let events = path
            .map(|path| -> Result<_> {
                let v: Value = serde_json::from_slice(&io::read(path)?)
                    .map_err(|_| AppError::invalid("invalid NLS source JSON"))?;
                let a = v
                    .as_array()
                    .ok_or_else(|| AppError::invalid("NLS source requires ordered event array"))?;
                for e in a {
                    let o = e
                        .as_object()
                        .filter(|o| {
                            o.len() == 4
                                && ["api", "input_hex", "written_hex", "return"]
                                    .iter()
                                    .all(|k| o.contains_key(*k))
                        })
                        .ok_or_else(|| {
                            AppError::invalid(
                                "NLS event requires api/input_hex/written_hex/return only",
                            )
                        })?;
                    if !matches!(o["api"].as_str(), Some("lcmap" | "upper")) {
                        return Err(AppError::invalid("NLS api requires lcmap or upper"));
                    }
                    for k in ["input_hex", "written_hex"] {
                        io::decode_hex(
                            o[k].as_str()
                                .ok_or_else(|| AppError::invalid("NLS bytes require hex string"))?,
                        )?;
                    }
                    if o["return"].as_i64().is_none() {
                        return Err(AppError::invalid("NLS return requires integer"));
                    }
                }
                Ok(a.iter().cloned().collect())
            })
            .transpose()?;
        Ok(Self { events, used: 0 })
    }
    fn captured(
        &mut self,
        api: &str,
        input: &[u8],
        out: &mut [u8],
    ) -> std::result::Result<Option<i64>, csv::Error> {
        let Some(events) = self.events.as_mut() else {
            return Ok(None);
        };
        let e = events.pop_front().ok_or(csv::Error::SourceUnavailable(
            "captured NLS event exhausted".into(),
        ))?;
        self.used += 1;
        if e["api"] != api {
            return Err(csv::Error::Invalid(
                "captured NLS API order mismatch".into(),
            ));
        }
        let decode = |key| {
            io::decode_hex(e[key].as_str().expect("validated source"))
                .map_err(|_| csv::Error::Invalid("captured NLS hex".into()))
        };
        if decode("input_hex")? != input {
            return Err(csv::Error::Invalid(
                "captured NLS actual input mismatch".into(),
            ));
        }
        let written = decode("written_hex")?;
        if written.len() > out.len() {
            return Err(csv::Error::Invalid(
                "captured NLS output exceeds actual capacity".into(),
            ));
        }
        out[..written.len()].copy_from_slice(&written);
        Ok(e["return"].as_i64())
    }
    fn map(&mut self, input: &[u8], out: &mut [u8]) -> std::result::Result<i32, csv::Error> {
        match self.captured("lcmap", input, out)? {
            Some(n) => {
                i32::try_from(n).map_err(|_| csv::Error::Invalid("LCMap return outside i32".into()))
            }
            None => csv::nls::ascii_map(input, out),
        }
    }
    fn upper(&mut self, out: &mut [u8]) -> std::result::Result<u32, csv::Error> {
        let input = out.to_vec();
        match self.captured("upper", &input, out)? {
            Some(n) => u32::try_from(n)
                .map_err(|_| csv::Error::Invalid("CharUpper return outside u32".into())),
            None => {
                if !out.is_ascii() {
                    return Err(csv::Error::SourceUnavailable(
                        "actual non-ASCII CharUpper source unavailable".into(),
                    ));
                }
                out.make_ascii_uppercase();
                Ok(out.len() as u32)
            }
        }
    }
}

pub fn run(args: Args) -> Result<u8> {
    if let Command::BulkUse {
        input,
        value,
        range_context,
        group,
        cancel,
        yes,
        write,
    } = &args.command
    {
        if *cancel {
            io::report(&json!({"cancelled":true,"published":false,"inputs_read":false}))?;
            return Ok(0);
        }
        confirm(*yes)?;
        io::single_stdin([input.as_str(), range_context.as_str()])?;
        let context = native::read(range_context)?;
        let bytes = io::read(input)?;
        let image = Icf::parse(&bytes)?;
        let mut working = Working::from_icf(
            &image,
            DefaultContext::from_context(&context.live_q, context.system_langid)?,
        )?;
        let range = working.bulk_use_range(*group)?;
        let mut requests = Vec::new();
        let mut getters = 0;
        let result = working.bulk_use(
            range,
            matches!(value, Use::Yes),
            0,
            || {
                getters += 1;
                Ok((&context.ranges, &context.live_q))
            },
            |notification| {
                requests.push(notification);
                Ok(())
            },
        );
        io::report(&json!({
            "operation":"bulk-use","value":matches!(value,Use::Yes),
            "range":{"first":range.first,"last":range.last},"live_getter_calls":getters,
            "publication_authorized":result.is_ok(),"partial":result.is_err(),
            "failure":result.as_ref().err().map(|error|error.to_string()),
            "notifications":requests.iter().map(|n|json!({"id":n.event,"synchronous":n.synchronous,"first":n.first,"second":n.second,"delivery":"request-only"})).collect::<Vec<_>>(),
            "native":native::report(&context),"dry_run":write.dry_run,
        }))?;
        result?;
        io::publish(&working.to_icf()?.to_bytes(), write)?;
        return Ok(0);
    }
    if matches!(&args.command, Command::Fields) {
        io::publish(
            &io::json_bytes(&catalog())?,
            &Write {
                output: Some("-".into()),
                overwrite: false,
                dry_run: false,
            },
        )?;
        return Ok(0);
    }
    let Command::Edit {
        input,
        index,
        range_context,
        group,
        assignments,
        generation,
        nls_source,
        align_callsign,
        accept_partial,
        yes,
        write,
    } = args.command
    else {
        let Command::Read {
            input,
            index,
            range_context,
        } = args.command
        else {
            unreachable!()
        };
        io::single_stdin([input.as_str(), range_context.as_str()])?;
        let native = native::read(&range_context)?;
        let bytes = io::read(&input)?;
        let image = Icf::parse(&bytes)?;
        let w = Working::from_icf(
            &image,
            DefaultContext::from_context(&native.live_q, native.system_langid)?,
        )?;
        io::publish(
            &io::json_bytes(&view(&row(&w, index)?))?,
            &Write {
                output: Some("-".into()),
                overwrite: false,
                dry_run: false,
            },
        )?;
        return Ok(0);
    };
    if accept_partial {
        confirm(yes)?;
    }
    io::single_stdin(
        [
            Some(input.as_str()),
            Some(range_context.as_str()),
            generation.as_deref(),
            nls_source.as_deref(),
        ]
        .into_iter()
        .flatten(),
    )?;
    let native = native::read(&range_context)?;
    let bytes = io::read(&input)?;
    let image = Icf::parse(&bytes)?;
    let mut working = Working::from_icf(
        &image,
        DefaultContext::from_context(&native.live_q, native.system_langid)?,
    )?;
    let g = generation.as_deref().map(io::read).transpose()?;
    let source = RefCell::new(NlsSource::read(nls_source.as_deref())?);
    let r = lib::Runtime {
        ranges: &native.ranges,
        live_q: &native.live_q,
        system_langid: native.system_langid,
        generation: g.as_deref(),
        align_callsign,
        crt_decimal: b'.',
    };
    let mut reports = Vec::new();
    let mut writes = false;
    let mut prefix = false;
    for assignment in assignments {
        let mut e = lib::Effect::default();
        let mut h = if index < 2500 {
            row(&working, index)?
        } else if index == 2500 {
            let mut h = record::reset_holder(0x0c, index as u32, 0);
            h.set_valid(false);
            h
        } else {
            return Err(AppError::invalid("native repeater index outside0..2500"));
        };
        let result = (|| -> Result<()> {
            let (name, input) = assignment
                .split_once('=')
                .ok_or_else(|| AppError::invalid("native edit requires FIELD=INPUT"))?;
            let f = Field::from_str(name, false)
                .map_err(|_| AppError::invalid("unknown native repeater field"))?;
            if matches!(f, Field::Index) {
                return Err(AppError::invalid("native row index is read-only"));
            }
            if let Some(field) = f.selector() {
                let mut action = fields::choice_input(input)?;
                // The shared scalar shorthand has no explicit native key.
                // Preserve explicit JSON key/phase, including mismatches.
                if !values::json_view(input).starts_with('{')
                    && let ChoiceInput::Selected(event) = &mut action
                {
                    event.key = match field {
                        S::Mode => 10,
                        S::Duplex => 11,
                        _ => 1,
                    };
                }
                if !h.valid() {
                    if field == S::Type && action == ChoiceInput::Begin {
                        let group = group.ok_or_else(|| {
                            AppError::invalid("empty Type Enter requires actual --group")
                        })?;
                        let groups = working.groups()?;
                        let start = groups.start(group)? as usize;
                        let relative = index
                            .checked_sub(start)
                            .ok_or_else(|| AppError::invalid("Type index precedes view group"))?;
                        lib::begin_type(&mut working, &mut h, group, relative, &r, &mut e)
                            .map_err(|e| AppError::invalid(e.to_string()))?;
                        if e.blocked {
                            return Ok(());
                        }
                    } else {
                        return Err(AppError::invalid(
                            "inactive native row requires Type begin before editing",
                        ));
                    }
                }
                lib::choice(&mut working, &mut h, field, action, &r, &mut e)
                    .map_err(|e| AppError::invalid(e.to_string()))?;
            } else if let Some(field) = f.numeric() {
                if !h.valid() {
                    return Err(AppError::invalid("inactive native row requires Type begin"));
                }
                fields::with_number_input(input, |input, point| {
                    let r = lib::Runtime {
                        crt_decimal: point,
                        ..r
                    };
                    lib::number(
                        &mut working,
                        &mut h,
                        field,
                        input,
                        &r,
                        &mut e,
                        &mut lib::Nls {
                            map: &mut |a, b| source.borrow_mut().map(a, b),
                            upper: &mut |b| source.borrow_mut().upper(b),
                        },
                    )
                    .map_err(|e| AppError::invalid(e.to_string()))
                })?;
            } else {
                if !h.valid() {
                    return Err(AppError::invalid("inactive native row requires Type begin"));
                }
                if let Some(field) = f.text() {
                    text_input(input, |input| {
                        lib::text(
                            &mut working,
                            &mut h,
                            field,
                            input,
                            &r,
                            &mut e,
                            &mut lib::Nls {
                                map: &mut |a, b| source.borrow_mut().map(a, b),
                                upper: &mut |b| source.borrow_mut().upper(b),
                            },
                        )
                        .map_err(|e| AppError::invalid(e.to_string()))
                    })?;
                } else {
                    coordinate_edit(
                        &mut working,
                        &mut h,
                        matches!(f, Field::Longitude),
                        input,
                        &r,
                        &mut e,
                    )?;
                }
            }
            Ok(())
        })();
        writes |= e.native.save_returned;
        prefix |= e.native.save_returned || e.inserted;
        reports.push(json!({"assignment":assignment,"effect":effect(&e),"row":view(&h)}));
        if let Err(error) = result {
            let publish = accept_partial && prefix;
            io::report(
                &json!({"operations":reports,"partial":true,"publication_authorized":publish,"error":error.message,"native":native::report(&native)}),
            )?;
            if !publish {
                return Err(error);
            }
            io::publish(&working.to_icf()?.to_bytes(), &write)?;
            return Ok(5);
        }
    }
    io::report(
        &json!({"operations":reports,"partial":false,"publication_authorized":writes,"native":native::report(&native),"NLS_requests":source.borrow().used}),
    )?;
    if writes {
        io::publish(&working.to_icf()?.to_bytes(), &write)?;
    }
    Ok(0)
}
