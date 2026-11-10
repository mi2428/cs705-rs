//! Thin GPS settings commands; shared I/O and the library own all storage.

use super::{
    InputValue, Publication, TextAction, choice_json, finish, load, nls_outputs, number_action,
    number_json, text_action,
};
use crate::{
    AppError, Result,
    args::{Format, Read},
    confirm, fields, io, render, values,
};
use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use cs705::{
    gps::editor as record,
    icf::settings::{Value, extended::ChoiceInput, gps as lib},
};
use serde_json::json;

#[derive(ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Edit a GPS memory name or altitude through the live record loader/saver.
    Record {
        #[command(flatten)]
        read: Read,
        group: usize,
        index: usize,
        #[arg(value_enum)]
        field: RecordField,
        #[command(flatten)]
        value: InputValue,
        #[command(flatten)]
        options: crate::records::Options,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        write: crate::args::Write,
    },
    /// Select a GPS row alarm using its captured group/start/active view.
    RowAlarm {
        #[command(flatten)]
        read: Read,
        group: u8,
        start: u32,
        active: u32,
        #[command(flatten)]
        value: InputValue,
        #[command(flatten)]
        publication: Publication,
    },
    /// Native D-PRS symbol menu or whole-text direct editor, not a raw pair setter.
    Symbol {
        #[command(flatten)]
        read: Read,
        field: String,
        #[arg(long, default_value_t = 0)]
        index: usize,
        #[command(flatten)]
        value: InputValue,
        #[arg(long, requires = "symbol_resource")]
        menu: bool,
        /// Actual native 148-byte lookup resource, not a completed symbol pair.
        #[arg(long, requires = "menu")]
        symbol_resource: Option<String>,
        #[arg(long, conflicts_with = "menu")]
        nls_context: Option<String>,
        #[command(flatten)]
        publication: Publication,
    },
    /// Native coordinate/area mask frame, or explicit native phase 2 bytes.
    #[command(alias = "area")]
    Coordinate {
        #[command(flatten)]
        read: Read,
        field: String,
        #[command(flatten)]
        value: InputValue,
        #[arg(
            long,
            required_unless_present = "current_source",
            conflicts_with = "current_source"
        )]
        current_working: Option<String>,
        /// Explicitly bind global-current to this input image, not an implicit alias.
        #[arg(long)]
        current_source: bool,
        /// Ordered actual frame callbacks; normal text defaults to Accept.
        #[arg(long = "event", value_enum, default_value = "accept")]
        events: Vec<CoordinateEvent>,
        #[command(flatten)]
        publication: Publication,
    },
    /// Native checkbox frame steps, in order; indices are the original 1..6/1..4.
    Sentences {
        #[command(flatten)]
        read: Read,
        field: String,
        /// choose:N, unchoose:N, ok, cancel, close, deactivate, owner-destroyed.
        #[arg(long = "step", required = true)]
        steps: Vec<String>,
        #[command(flatten)]
        publication: Publication,
    },
    /// List connected native GPS setting fields and their original feature IDs.
    Fields {
        #[arg(long, value_enum, default_value = "json")]
        format: Format,
    },
    /// Read stored values without invoking a native display/edit operation.
    #[command(name = "read")]
    Inspect {
        #[command(flatten)]
        read: Read,
        field: String,
        #[arg(long, default_value_t = 0)]
        index: usize,
    },
    /// Invoke a native GPS field operation with explicit input or action JSON.
    Edit {
        #[command(flatten)]
        read: Read,
        field: String,
        #[command(flatten)]
        value: InputValue,
        #[arg(long, default_value_t = 0)]
        index: usize,
        #[arg(long)]
        nls_context: Option<String>,
        /// Optional captured unit/degree bytes for explicit numeric analysis.
        #[arg(long)]
        unit_output_hex: Option<String>,
        /// Actual locale DWORD; required by course degrees and directivity.
        #[arg(long)]
        language_descriptor: Option<u32>,
        /// Actual SYSTEM LANGID; unused when the original short-circuit skips it.
        #[arg(long)]
        system_langid: Option<u16>,
        #[command(flatten)]
        publication: Publication,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum RecordField {
    Name,
    Altitude,
}

fn choice_approved(input: ChoiceInput) -> bool {
    matches!(input, ChoiceInput::Next | ChoiceInput::Previous)
        || matches!(input, ChoiceInput::Selected(event) if event.accepted && event.key == 1 && event.phase == 2)
}

fn edit_record(
    image: &mut cs705::icf::Icf<'_>,
    group: usize,
    index: usize,
    field: RecordField,
    value: &str,
    runtime: &crate::records::Runtime,
) -> Result<(serde_json::Value, bool)> {
    let groups = image.gps_groups()?;
    let count = groups
        .get(group)
        .ok_or_else(|| AppError::invalid("GPS record group outside 0..26"))?
        .physical_slots()
        .len();
    if index > count {
        return Err(AppError::invalid("GPS record row outside the group"));
    }
    let absolute = groups[..group]
        .iter()
        .map(|group| group.physical_slots().len())
        .sum::<usize>()
        + index;
    let added = index == count;
    if added {
        if !matches!(field, RecordField::Name) {
            return Err(AppError::invalid(
                "an empty GPS row accepts Name input first",
            ));
        }
        let outcome =
            crate::sheets::gps_insert_live(image, group, index, 1, (index, index), runtime)?;
        if outcome.completed == 0 {
            return Ok((
                json!({"operation":"gps-record","capacity_stopped":true}),
                false,
            ));
        }
    }
    let mut holder = record::load(image, absolute, 1)?;
    // The native add branch clears validity after reload. This invocation can
    // approve Name text without storing it; a later invocation reloads valid 1.
    if added {
        holder.set_valid(false);
    }
    match field {
        RecordField::Name => crate::repeaters::text_input(value, |input| {
            use cs705::icf::settings::extended::TextInput;
            let mut shared = Vec::new();
            let mut stored = false;
            if let TextInput::Text(source) = input {
                shared = source.to_vec();
                let mut name = cs705::csv::nls::name_candidate(&mut shared, 16)
                    .map_err(|error| AppError::invalid(error.to_string()))?;
                name.resize(16, b' ');
                holder.as_mut_bytes()[0x244..0x254].copy_from_slice(&name);
                stored = record::save(image, absolute, &holder)?;
                holder = record::load(image, absolute, holder.binding())?;
            }
            let display = if added {
                Vec::new()
            } else {
                match input {
                    TextInput::Begin(Some(first)) => vec![first],
                    _ => holder.as_bytes()[0x244..0x254].to_vec(),
                }
            };
            Ok((
                json!({"operation":"gps-record-name","group":group,"index":index,"absolute":absolute,"stored":stored,"state_out":if matches!(input,TextInput::Begin(_)){2}else{0},"display":io::bytes_value(&display),"shared_source_after":io::bytes_value(&shared)}),
                matches!(input, TextInput::Text(_)),
            ))
        }),
        RecordField::Altitude => number_action(value, |input, point| {
            let effect = record::altitude(image, absolute, &mut holder, input, point)?;
            Ok((
                json!({"operation":"gps-record-altitude","group":group,"index":index,"absolute":absolute,"effect":number_json(&effect)}),
                effect.accepted,
            ))
        }),
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum CoordinateEvent {
    Accept,
    Cancel,
    EscapeKeyPress,
    Deactivate,
    Close,
}
impl CoordinateEvent {
    fn native(self) -> cs705::csv::number::CoordinateEvent {
        use cs705::csv::number::CoordinateEvent as E;
        match self {
            Self::Accept => E::Accept,
            Self::Cancel => E::Cancel,
            Self::EscapeKeyPress => E::EscapeKeyPress,
            Self::Deactivate => E::Deactivate,
            Self::Close => E::Close,
        }
    }
}
pub fn run(args: Args) -> Result<u8> {
    match args.command {
        Command::Record {
            read,
            group,
            index,
            field,
            value,
            options,
            yes,
            write,
        } => {
            if options.normal_options_present() {
                return Err(AppError::invalid(
                    "GPS record editing requires a single whole row",
                ));
            }
            let runtime = options
                .load(std::iter::once(read.input.as_str()).chain(value.value_file.as_deref()))?;
            let publication = Publication {
                write,
                accept_partial: options.accept_partial,
                yes,
            };
            if publication.accept_partial {
                confirm(yes)?;
            }
            let value = value.read()?;
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let result = edit_record(&mut image, group, index, field, &value, &runtime);
            finish(&image, &read, &publication, &before, result)
        }
        Command::RowAlarm {
            read,
            group,
            start,
            active,
            value,
            publication,
        } => {
            io::single_stdin(
                std::iter::once(read.input.as_str()).chain(value.value_file.as_deref()),
            )?;
            if publication.accept_partial {
                confirm(publication.yes)?;
            }
            let value = value.read()?;
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let result = (|| {
                let input = fields::choice_input(&value)?;
                let effect = record::row_alarm(
                    &mut image,
                    record::AlarmView {
                        group,
                        start,
                        active,
                    },
                    input,
                )?;
                Ok((
                    json!({"operation":"gps-row-alarm","effect":choice_json(&effect.choice),"marked":effect.marked,"notify_39_requested":effect.notify_39,"choices":record::RowAlarmChoice::ALL.map(|choice| {
                        let index = choice.index().expect("valid alarm producer choice");
                        json!({"value":index,"meaning":match choice {record::RowAlarmChoice::Off=>"disabled",record::RowAlarmChoice::Memory=>"selected row",record::RowAlarmChoice::Receive=>"received position",record::RowAlarmChoice::All=>"every group",record::RowAlarmChoice::Group(_)=>"one group"},"group":match choice {record::RowAlarmChoice::Group(wire)=>Some(wire-1),_=>None}})
                    }),"notification_delivery":"request-only"}),
                    choice_approved(input),
                ))
            })();
            finish(&image, &read, &publication, &before, result)
        }
        Command::Fields { format } => {
            let mut fields=lib::choice_fields().iter().map(|field|{let s=&field.setting;json!({"feature_id":s.feature_id,"name":s.name,"key":s.key,"index":0,"kind":"choice","min":s.domain.min,"max":s.domain.max,"step":s.domain.step,"operation_connected":true})}).collect::<Vec<_>>();
            fields.extend(lib::text_fields().iter().map(|field|json!({"feature_id":field.feature_id,"name":field.name,"key":field.key,"kind":"text","count":field.count,"width":field.width,"operation_connected":true})));
            fields.extend(lib::SentenceField::ALL.map(|field|json!({"feature_id":field.feature_id(),"name":field.name(),"key":field.key(),"kind":"sentences","first_index":1,"count":field.count(),"operation_connected":true})));
            fields.extend(lib::coordinate_fields().iter().chain(std::iter::once(&lib::ALARM_AREA)).map(|field|json!({"feature_id":field.feature_id,"name":field.name,"key":field.key,"kind":if field.area {"area"}else{"coordinate"},"longitude":field.longitude,"count":1,"operation_connected":true})));
            fields.extend(lib::AlarmField::ALL.map(|field|json!({"feature_id":field.feature_id(),"name":field.name(),"key":field.key(),"kind":"alarm-choice","count":1,"operation_connected":true})));
            fields.extend(lib::number_fields().iter().map(|field|json!({"feature_id":field.setting.feature_id,"name":field.setting.name,"key":field.setting.key,"kind":"number","count":1,"unit_output_required":false,"degree_context_required":matches!(field.setting.key,0x1ef|0x200),"operation_connected":true})));
            fields.extend(lib::symbol_fields().iter().map(|field|json!({"feature_id":field.feature_id,"name":field.name,"key":field.key,"kind":"symbol","count":field.count,"operation_connected":true})));
            render(&json!(fields), format)?;
            Ok(0)
        }
        Command::Symbol {
            read,
            field,
            index,
            value,
            menu,
            symbol_resource,
            nls_context,
            publication,
        } => {
            io::single_stdin(
                std::iter::once(read.input.as_str())
                    .chain(value.value_file.as_deref())
                    .chain(symbol_resource.as_deref())
                    .chain(nls_context.as_deref()),
            )?;
            if publication.accept_partial {
                confirm(publication.yes)?;
            }
            let value = value.read()?;
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let result = (|| {
                let field = lib::find_symbol(&field)?;
                if menu {
                    let bytes =
                        io::read(symbol_resource.as_deref().ok_or_else(|| {
                            AppError::invalid("native symbol resource required")
                        })?)?;
                    let resource = lib::NativeSymbolPairs::read(&bytes)?;
                    let input = fields::choice_input(&value)?;
                    let effect = field.select(&mut image, index, input, &resource)?;
                    Ok((
                        json!({"feature_id":field.feature_id,"index":index,"effect":choice_json(&effect),"raw":field.read(&image,index)?,"resource_input":"actual native74pair window"}),
                        choice_approved(input),
                    ))
                } else {
                    use cs705::icf::settings::extended::FixedTextInput;
                    let action = text_action(&value)?;
                    let typed = matches!(action, TextAction::Text(..));
                    if nls_context.is_some() && !typed {
                        return Err(AppError::invalid("symbol NLS context requires text input"));
                    }
                    let width = match &action {
                        TextAction::Text(value, _) => value.len(),
                        _ => 0,
                    };
                    let nls = nls_context
                        .as_deref()
                        .map(|p| nls_outputs(p, width))
                        .transpose()?;
                    let mut source = Vec::new();
                    let input = match action {
                        TextAction::Text(value, phase) => {
                            source = value;
                            FixedTextInput::Text {
                                source: &mut source,
                                phase,
                            }
                        }
                        TextAction::Begin => FixedTextInput::Begin,
                        TextAction::Cancel => FixedTextInput::Cancel,
                        TextAction::Display => FixedTextInput::Display,
                    };
                    let effect = if let Some(nls) = nls {
                        field.edit_with_nls(&mut image, index, input, |_, out| {
                            if out.len() != nls.map.bytes.len() {
                                return Err(cs705::csv::Error::Invalid("symbol LCMap width"));
                            }
                            out.copy_from_slice(&nls.map.bytes);
                            Ok(nls.map.result)
                        })
                    } else {
                        field.edit(&mut image, index, input)
                    };
                    if typed {
                        io::report(&json!({"shared_source_after":io::bytes_value(&source)}))?;
                    }
                    let effect = effect.map_err(|e| AppError::invalid(e.to_string()))?;
                    let report = json!({"feature_id":field.feature_id,"index":index,"stored":effect.stored,"state_out":effect.state_out,"notify_flags":effect.notify_flags,"raw":field.read(&image,index)?});
                    if typed && !effect.stored {
                        io::report(&json!({"editor":report}))?;
                        return Err(AppError::invalid(
                            "native symbol editor did not store the text",
                        ));
                    }
                    Ok((report, effect.stored))
                }
            })();
            finish(&image, &read, &publication, &before, result)
        }
        Command::Coordinate {
            read,
            field,
            value,
            current_working,
            current_source: _,
            events,
            publication,
        } => {
            io::single_stdin(
                std::iter::once(read.input.as_str())
                    .chain(value.value_file.as_deref())
                    .chain(current_working.as_deref()),
            )?;
            if publication.accept_partial {
                confirm(publication.yes)?;
            }
            let value = value.read()?;
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let current = if let Some(path) = current_working {
                io::read(&path)?
            } else {
                before.clone()
            };
            let result = (|| {
                let field = lib::find_coordinate(&field)?;
                let action = text_action(&value)?;
                let begin = matches!(action, TextAction::Begin);
                let mut dialog = None;
                let effect = match action {
                    TextAction::Text(source, 2) => field.phase2(&mut image, &source, &current),
                    TextAction::Text(source, _) => {
                        let mut frame = field
                            .dialog(&image, &current, None)
                            .map_err(|e| AppError::invalid(e.to_string()))?;
                        frame.replace(&source);
                        let mut result = lib::CoordinateEffect {
                            stored: false,
                            raw: field.read(&image),
                            gray: false,
                        };
                        let mut stored = false;
                        for event in events {
                            result = field
                                .apply_dialog(&mut image, &mut frame, event.native(), &current)
                                .map_err(|e| AppError::invalid(e.to_string()))?;
                            stored |= result.stored;
                        }
                        result.stored = stored;
                        dialog = Some(frame);
                        Ok(result)
                    }
                    TextAction::Begin => {
                        dialog = Some(
                            field
                                .dialog(&image, &current, None)
                                .map_err(|e| AppError::invalid(e.to_string()))?,
                        );
                        Ok(lib::CoordinateEffect {
                            stored: false,
                            raw: field.read(&image),
                            gray: false,
                        })
                    }
                    TextAction::Cancel | TextAction::Display => Ok(lib::CoordinateEffect {
                        stored: false,
                        raw: field.read(&image),
                        gray: false,
                    }),
                }
                .map_err(|e| AppError::invalid(e.to_string()))?;
                let frame=dialog.as_ref().map(|d|json!({"mode":d.mode,"mask":io::bytes_value(d.mask),"seed":io::bytes_value(&d.seed),"control":io::bytes_value(&d.control),
                    "valid":d.valid(),"done":d.done,"cancel":d.cancel,"release_requested":d.release_requested,"phase2":d.phase2.as_ref().map(|b|io::bytes_value(b))}));
                Ok((
                    json!({"feature_id":field.feature_id,"stored":effect.stored,"raw":effect.raw,"gray":effect.gray,"dialog":frame,"state_out":if begin{1}else{0}}),
                    effect.stored,
                ))
            })();
            finish(&image, &read, &publication, &before, result)
        }
        Command::Sentences {
            read,
            field,
            steps,
            publication,
        } => {
            io::single_stdin([read.input.as_str()])?;
            if publication.accept_partial {
                confirm(publication.yes)?;
            }
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let result = (|| {
                let field = lib::find_sentence(&field)?;
                let mut dialog = lib::SentenceDialog::new(&image, field);
                let mut effects = Vec::new();
                let mut approved = false;
                for step in &steps {
                    let event = match step.as_str() {
                        "ok" => lib::SentenceEvent::Ok,
                        "cancel" => lib::SentenceEvent::Cancel,
                        "close" => lib::SentenceEvent::Close,
                        "deactivate" => lib::SentenceEvent::Deactivate,
                        "owner-destroyed" => lib::SentenceEvent::OwnerDestroyed,
                        _ => {
                            let (action, index) = step
                                .split_once(':')
                                .ok_or_else(|| AppError::invalid("invalid GPS sentence step"))?;
                            let selected = match action {
                                "choose" => true,
                                "unchoose" => false,
                                _ => {
                                    return Err(AppError::invalid(
                                        "invalid GPS sentence choice step",
                                    ));
                                }
                            };
                            lib::SentenceEvent::Choose {
                                index: values::parse(index)?,
                                selected,
                            }
                        }
                    };
                    let effect = dialog.apply(&mut image, event)?;
                    approved |= effect.helper_changed && effect.valid;
                    effects.push(json!({"step":step,"stored":effect.stored,"helper_changed":effect.helper_changed,"valid":effect.valid,"selected_nodes":effect.selected}));
                }
                Ok((
                    json!({"feature_id":field.feature_id(),"steps":effects,"done":dialog.done,"cancel_flag":dialog.cancelled,"release_requested":dialog.release_requested,"native_gui":false}),
                    approved,
                ))
            })();
            finish(&image, &read, &publication, &before, result)
        }
        Command::Inspect { read, field, index } => {
            io::single_stdin([read.input.as_str()])?;
            let bytes = io::read(&read.input)?;
            let image = load(&bytes, read.kind)?;
            let value = if let Some(field) = lib::choice_fields()
                .iter()
                .find(|f| f.setting.name == field || f.setting.feature_id == field)
            {
                if index != 0 {
                    return Err(AppError::invalid("GPS choice requires index0"));
                }
                let Value::Number(value) = field.setting.read(&image, 0)?;
                json!({"feature_id":field.setting.feature_id,"name":field.setting.name,"value":value,"query_only":true})
            } else if let Some(field) = lib::text_fields()
                .iter()
                .find(|f| f.name == field || f.feature_id == field)
            {
                json!({"feature_id":field.feature_id,"name":field.name,"value":io::bytes_value(&field.read(&image,index)?),"budget":field.budget(&image),"query_only":true})
            } else if let Some(field) = lib::symbol_fields()
                .iter()
                .find(|f| f.name == field || f.feature_id == field)
            {
                json!({"feature_id":field.feature_id,"name":field.name,"raw":field.read(&image,index)?,"query_only":true})
            } else if let Some(field) = lib::number_fields()
                .iter()
                .find(|f| f.setting.name == field || f.setting.feature_id == field)
            {
                if index != 0 {
                    return Err(AppError::invalid("GPS number query requires index0"));
                }
                let Value::Number(value) = field.setting.read(&image, 0)?;
                json!({"feature_id":field.setting.feature_id,"value":value,"bounds":field.bounds(&image),"query_only":true})
            } else if let Some(field) = lib::AlarmField::ALL
                .into_iter()
                .find(|f| f.name() == field || f.feature_id() == field)
            {
                if index != 0 {
                    return Err(AppError::invalid("GPS alarm query requires index0"));
                }
                json!({"feature_id":field.feature_id(),"value":field.read(&image),"choices":field.choices(&image),"query_only":true})
            } else if let Some(field) = lib::coordinate_fields()
                .iter()
                .chain(std::iter::once(&lib::ALARM_AREA))
                .find(|f| f.name == field || f.feature_id == field)
            {
                if index != 0 {
                    return Err(AppError::invalid("GPS coordinate query requires index0"));
                }
                json!({"feature_id":field.feature_id,"name":field.name,"raw":field.read(&image),"query_only":true})
            } else {
                if index != 0 {
                    return Err(AppError::invalid("GPS sentence query requires index0"));
                }
                let field = lib::find_sentence(&field)?;
                let dialog = lib::SentenceDialog::new(&image, field);
                json!({"feature_id":field.feature_id(),"name":field.name(),"selected_nodes":dialog.selected(),"valid":dialog.valid(),"query_only":true})
            };
            render(&value, read.format)?;
            Ok(0)
        }
        Command::Edit {
            read,
            field,
            value,
            index,
            nls_context,
            unit_output_hex,
            language_descriptor,
            system_langid,
            publication,
        } => {
            io::single_stdin(
                std::iter::once(read.input.as_str())
                    .chain(value.value_file.as_deref())
                    .chain(nls_context.as_deref()),
            )?;
            let value = value.read()?;
            if publication.accept_partial {
                confirm(publication.yes)?;
            }
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let result = (|| {
                if let Some(field) = lib::AlarmField::ALL
                    .into_iter()
                    .find(|f| f.name() == field || f.feature_id() == field)
                {
                    if index != 0 || nls_context.is_some() || unit_output_hex.is_some() {
                        return Err(AppError::invalid(
                            "GPS alarm choice requires index0 and no text/unit context",
                        ));
                    }
                    let input = fields::choice_input(&value)?;
                    let approved = !field.choices(&image).is_empty() && choice_approved(input);
                    let effect = field.edit(&mut image, input)?;
                    Ok((
                        json!({"feature_id":field.feature_id(),"effect":choice_json(&effect),"choices":field.choices(&image)}),
                        approved,
                    ))
                } else if let Some(field) = lib::number_fields()
                    .iter()
                    .find(|f| f.setting.name == field || f.setting.feature_id == field)
                {
                    if index != 0 || nls_context.is_some() {
                        return Err(AppError::invalid(
                            "GPS number requires index0 and no text NLS context",
                        ));
                    }
                    let unit = unit_output_hex.as_deref().map(io::decode_hex).transpose()?;
                    number_action(&value, |input, point| {
                        let effect = if let Some(unit) = &unit {
                            field.edit(&mut image, input, point, Some(unit))?
                        } else {
                            field.edit_live(
                                &mut image,
                                input,
                                point,
                                lib::Resources {
                                    system_langid,
                                    language_descriptor,
                                },
                            )?
                        };
                        Ok((
                            json!({"feature_id":field.setting.feature_id,"effect":number_json(&effect),"bounds":field.bounds(&image)}),
                            effect.accepted,
                        ))
                    })
                } else if let Some(field) = lib::choice_fields()
                    .iter()
                    .find(|f| f.setting.name == field || f.setting.feature_id == field)
                {
                    if index != 0 || nls_context.is_some() || unit_output_hex.is_some() {
                        return Err(AppError::invalid(
                            "GPS choice requires index0 and no NLS context",
                        ));
                    }
                    let input = fields::choice_input(&value)?;
                    let r = lib::Resources {
                        system_langid,
                        language_descriptor,
                    };
                    let effect = field.edit(&mut image, input, r)?;
                    Ok((
                        json!({"feature_id":field.setting.feature_id,"stored":effect.stored,"value":effect.value,"gray":effect.gray,"notify_flags":effect.notify_flags,
                        "popup_requested":effect.popup_requested,"selector":field.selector(&image,r)?,"choices":field.setting.choices(&image,0)?}),
                        choice_approved(input),
                    ))
                } else {
                    if unit_output_hex.is_some() {
                        return Err(AppError::invalid(
                            "GPS unit output belongs to numeric wrappers only",
                        ));
                    }
                    let field = lib::find_text(&field)?;
                    let action = text_action(&value)?;
                    let typed = matches!(action, TextAction::Text(..));
                    if nls_context.is_some() && !typed {
                        return Err(AppError::invalid(
                            "NLS context requires a GPS text invocation",
                        ));
                    }
                    let width = match &action {
                        TextAction::Text(value, _) => value.len(),
                        _ => 0,
                    };
                    let output = nls_context
                        .as_deref()
                        .map(|path| nls_outputs(path, width))
                        .transpose()?;
                    let mut source = Vec::new();
                    let input = match action {
                        TextAction::Text(value, phase) => {
                            source = value;
                            cs705::icf::settings::extended::FixedTextInput::Text {
                                source: &mut source,
                                phase,
                            }
                        }
                        TextAction::Display => {
                            cs705::icf::settings::extended::FixedTextInput::Display
                        }
                        TextAction::Cancel => {
                            cs705::icf::settings::extended::FixedTextInput::Cancel
                        }
                        TextAction::Begin => cs705::icf::settings::extended::FixedTextInput::Begin,
                    };
                    let effect = if let Some(output) = output {
                        field.edit_with_nls(&mut image, index, input, |_, destination| {
                            if destination.len() != output.map.bytes.len() {
                                return Err(cs705::csv::Error::Invalid("GPS LCMap output width"));
                            }
                            destination.copy_from_slice(&output.map.bytes);
                            Ok(output.map.result)
                        })
                    } else {
                        field.edit(&mut image, index, input)
                    };
                    if typed {
                        io::report(&json!({"shared_source_after":io::bytes_value(&source)}))?;
                    }
                    let effect = effect.map_err(|error| AppError::invalid(error.to_string()))?;
                    Ok((
                        json!({"feature_id":field.feature_id,"index":index,"stored":effect.stored,"state_out":effect.state_out,"notify_flags":effect.notify_flags,
                        "display":io::bytes_value(&effect.display),"budget":field.budget(&image)}),
                        effect.stored,
                    ))
                }
            })();
            finish(&image, &read, &publication, &before, result)
        }
    }
}
