//! Sheet presentation and command adapters. Library code owns topology and CSV.

use crate::{
    AppError, Result,
    args::*,
    confirm, fields, io,
    values::{self, ValueCodec},
};
use cs705::icf::{
    GpsAltitude, GpsCoordinate, Icf, RepeaterCoordinate, RepeaterDuplex, RepeaterMode,
    RepeaterPosition, RepeaterTone,
};
use serde_json::{Map, Value, json};

pub fn groups(image: &Icf<'_>, sheet: Sheet) -> Result<Value> {
    let memory = if sheet == Sheet::Memory {
        Some(cs705::icf::rows::NormalMemory::from_icf(image)?)
    } else {
        None
    };
    Ok(Value::Array(match sheet {
        Sheet::Memory => image.memory_groups()?.iter().enumerate().map(|(index,g)|Ok(json!({"index":index,"descriptor":g.raw_descriptor(),"name":io::bytes_value(g.name_bytes()),"physical_slots":g.slots().as_slice(),"used":memory.as_ref().unwrap().group_used_slots(index)?}))).collect::<Result<Vec<_>>>()?,
        Sheet::Gps => image.gps_groups()?.iter().enumerate().map(|(index,g)|json!({"index":index,"name":g.name_bytes().map(io::bytes_value),"physical_slots":g.physical_slots()})).collect(),
        Sheet::Repeater => image.repeater_groups()?.iter().enumerate().map(|(index,g)|json!({"index":index,"name":io::bytes_value(g.name_bytes()),"physical_slots":g.physical_slots()})).collect(),
        _ => return Err(AppError::invalid("sheet does not have named groups")),
    }))
}

/// Native placeholder creation precedes the name helper, even when it declines.
/// Safe CLI publication remains separate from those internal mutations.
pub fn edit_memory_group_name(
    model: &mut cs705::icf::rows::NormalMemory,
    index: usize,
    value: &str,
) -> Result<bool> {
    use cs705::icf::settings::extended::TextInput;
    let before = model.group_count();
    crate::repeaters::text_input(value, |input| {
        let mut source = match input {
            TextInput::Text(bytes) => Some(bytes.to_vec()),
            _ => None,
        };
        let accepted = model.edit_group_name(index, source.as_deref_mut())?;
        io::report(
            &json!({"operation":"native-memory-group-name","index":index,"helper_accepted":accepted,"group_count_before":before,"group_count_after":model.group_count(),"state_out":if matches!(input,TextInput::Begin(_)) {2} else {0},"shared_input":source.as_deref().map(io::bytes_value)}),
        )?;
        Ok(accepted)
    })
}

pub fn edit_gps_group_name(image: &mut Icf<'_>, index: usize, value: &str) -> Result<bool> {
    crate::repeaters::text_input(value, |input| {
        let effect = cs705::gps::editor::group_name(image, index, input)
            .map_err(|error| AppError::invalid(error.to_string()))?;
        io::report(
            &json!({"operation":"gps-group-name","group":index,"stored":effect.stored,"state_out":effect.state_out,"display":io::bytes_value(&effect.display),"label_request":effect.label.as_ref().map(|(id,text)|json!({"id":id,"text":io::bytes_value(text)})),"refresh_if_binding_matches":effect.refresh_if_binding_matches,"shared_source_after":io::bytes_value(&effect.shared_input),"notification_delivery":"request-only"}),
        )?;
        Ok(effect.stored)
    })
}

pub fn view(
    image: &Icf<'_>,
    sheet: Sheet,
    start: usize,
    count: usize,
    runtime: Option<&crate::records::Runtime>,
) -> Result<Value> {
    if sheet == Sheet::All {
        start
            .checked_add(count)
            .ok_or_else(|| AppError::invalid("row range overflow"))?;
        let mut result = Map::new();
        for (name, sheet, limit) in [
            ("memory", Sheet::Memory, 500),
            ("call", Sheet::Call, 8),
            ("bandstack", Sheet::Bandstack, 45),
            ("memopad", Sheet::Memopad, 11),
            ("scan", Sheet::Scan, 25),
            ("user-edge", Sheet::UserEdge, 30),
            ("gps", Sheet::Gps, 300),
            ("repeater", Sheet::Repeater, 2500),
            ("from-dv", Sheet::FromDv, 10),
            ("from-fm", Sheet::FromFm, 10),
            ("to", Sheet::To, 20),
            ("rx", Sheet::Rx, 50),
            ("ur", Sheet::Ur, 300),
            ("preset", Sheet::Preset, 5),
        ] {
            result.insert(
                name.to_owned(),
                view(image, sheet, start.min(limit), count, runtime)?,
            );
        }
        return Ok(Value::Object(result));
    }
    if sheet == Sheet::Repeater || matches!(sheet, Sheet::FromDv | Sheet::FromFm | Sheet::To) {
        let runtime = runtime.ok_or_else(|| {
            AppError::safety("native repeater/history view requires explicit record context")
        })?;
        return if sheet == Sheet::Repeater {
            crate::records::repeater_view(image, start, count, runtime)
        } else {
            crate::records::history_view(image, sheet, start, count, runtime)
        };
    }
    if matches!(sheet, Sheet::Call | Sheet::Ur | Sheet::Rx) {
        return crate::records::view(image, sheet, start, count);
    }
    let special = match sheet {
        Sheet::Bandstack => Some(crate::special::Sheet::Bandstack),
        Sheet::Memopad => Some(crate::special::Sheet::Memopad),
        Sheet::Scan => Some(crate::special::Sheet::Scan),
        Sheet::UserEdge => Some(crate::special::Sheet::UserEdge),
        Sheet::Preset => Some(crate::special::Sheet::Preset),
        _ => None,
    };
    if let Some(sheet) = special {
        return crate::special::view(image, sheet, start, count);
    }
    let end = start
        .checked_add(count)
        .ok_or_else(|| AppError::invalid("row range overflow"))?;
    let max = match sheet {
        Sheet::Memory => 500,
        Sheet::Gps => 300,
        _ => return Err(pending("semantic sheet view")),
    };
    if start > max {
        return Err(AppError::invalid("start is outside the sheet"));
    }
    let mut rows = Vec::new();
    for slot in start..end.min(max) {
        let row = match sheet {
            Sheet::Memory => {
                let Some(name) = image.physical_memory_name_raw(slot)? else {
                    continue;
                };
                let mut row = Map::new();
                row.insert("physical_slot".into(), json!(slot));
                row.insert("name".into(), io::bytes_value(name));
                for field in [
                    "physical-memory-frequency-hz",
                    "physical-memory-filter",
                    "physical-memory-duplex",
                    "physical-memory-offset-hz",
                ] {
                    let key = format!("{field}/{slot}/Main");
                    let value = match fields::get(image, &key) {
                        Ok(value) => value,
                        Err(error) => json!({"error":error.message}),
                    };
                    row.insert(field.into(), value);
                }
                let mode = image.physical_memory_mode(slot, cs705::icf::MemorySide::Main);
                row.insert(
                    "mode".into(),
                    match &mode {
                        Ok(mode) => mode.json(),
                        Err(error) => json!({"error":error.to_string()}),
                    },
                );
                row.insert(
                    "data".into(),
                    json!(image.physical_memory_data(slot, cs705::icf::MemorySide::Main)?),
                );
                let sql = match mode {
                    Ok(cs705::icf::MemoryMode::Fm) => &[
                        "physical-memory-tone",
                        "physical-memory-tx-tone-index",
                        "physical-memory-rx-tone-index",
                        "physical-memory-dtcs-index",
                        "physical-memory-dtcs-polarity",
                    ][..],
                    Ok(cs705::icf::MemoryMode::Dv) => &[
                        "physical-memory-digital-squelch",
                        "physical-memory-digital-code",
                    ][..],
                    _ => &[],
                };
                for field in sql {
                    let key = format!("{field}/{slot}/Main");
                    row.insert(
                        (*field).into(),
                        match fields::get(image, &key) {
                            Ok(value) => value,
                            Err(error) => json!({"error":error.message}),
                        },
                    );
                }
                let split = image.physical_memory_split(slot)?;
                row.insert("split".into(), json!(split));
                if split {
                    row.insert(
                        "split-frequency-hz".into(),
                        json!(
                            image.physical_memory_frequency_hz(
                                slot,
                                cs705::icf::MemorySide::Split
                            )?
                        ),
                    );
                }
                Value::Object(row)
            }
            Sheet::Gps => {
                let Some(raw) = image.physical_gps_record_raw(slot)? else {
                    continue;
                };
                json!({"physical_slot":slot,"name":image.physical_gps_name_raw(slot)?.map(io::bytes_value),"record":io::bytes_value(raw),"date":image.physical_gps_date(slot)?.map(|v|v.json()),"time":image.physical_gps_time(slot)?.map(|v|v.json()),"latitude":image.physical_gps_latitude(slot)?.map(|v|v.json()),"longitude":image.physical_gps_longitude(slot)?.map(|v|v.json()),"altitude":image.physical_gps_altitude(slot)?.map(|v|v.json())})
            }
            _ => unreachable!(),
        };
        rows.push(row);
    }
    Ok(json!(rows))
}

pub fn project(value: Value, columns: &[String]) -> Result<Value> {
    if columns.is_empty() {
        return Ok(value);
    }
    let rows = value
        .as_array()
        .ok_or_else(|| AppError::invalid("column selection requires a sheet view"))?;
    let mut result = Vec::new();
    for row in rows {
        let mut selected = Vec::new();
        for column in columns {
            selected.push(json!({"column":column,"value":row.get(column).ok_or_else(||AppError::invalid("unknown selected column"))?}));
        }
        result.push(Value::Array(selected));
    }
    Ok(Value::Array(result))
}

pub fn load_config(file: &str) -> Result<Value> {
    let value: Value = serde_json::from_slice(&io::read(file)?)
        .map_err(|_| AppError::invalid("invalid configuration JSON"))?;
    validate_config(&value)?;
    Ok(value)
}

/// GUI column switches become explicit terminal/machine projections. Fixed-name
/// is column ordering, not a claim that a terminal has a frozen scrolling grid.
pub fn present(
    mut value: Value,
    columns: &[String],
    sheet: Option<Sheet>,
    preferences: &Value,
) -> Result<Value> {
    if let Some(rows) = value.as_array_mut() {
        for row in rows {
            if let Some(object) = row.as_object_mut() {
                object.retain(|key, _| {
                    !(preferences["show-split"] == false
                        && (key == "split" || key.starts_with("split-")))
                        && !(preferences["show-sql"] == false
                            && (key.contains("tone")
                                || key.contains("squelch")
                                || key.contains("dtcs")
                                || key.ends_with("digital-code")))
                });
            }
        }
    }
    let configured = preferences
        .get(if sheet == Some(Sheet::Rx) {
            "rx-columns"
        } else {
            "columns"
        })
        .and_then(Value::as_array);
    let selected = if columns.is_empty() {
        configured
            .map(|a| {
                a.iter()
                    .map(|v| v.as_str().expect("validated config column").to_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        columns.to_vec()
    };
    if !selected.is_empty() {
        return project(value, &selected);
    }
    if preferences["fixed-name"] == true
        && let Some(rows) = value.as_array()
    {
        let ordered = rows
            .iter()
            .map(|row| {
                let object = row
                    .as_object()
                    .ok_or_else(|| AppError::invalid("fixed-name requires object rows"))?;
                let mut keys: Vec<_> = object.keys().collect();
                keys.sort_by_key(|key| (!matches!(key.as_str(), "name" | "Name"), key.as_str()));
                Ok(json!(
                    keys.into_iter()
                        .map(|key| json!({"column":key,"value":object[key]}))
                        .collect::<Vec<_>>()
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        return Ok(Value::Array(ordered));
    }
    Ok(value)
}

pub fn toolbar(preferences: &Value) -> Result<()> {
    if preferences["toolbar"] == true {
        io::report(
            &json!({"commands":["parse","check","show","edit","new","convert","import","export","group","row","clone","config"]}),
        )?;
    }
    Ok(())
}
pub fn render_text(value: &Value, preferences: &Value) -> Result<()> {
    let scale = preferences
        .get("text-scale")
        .and_then(Value::as_u64)
        .unwrap_or(1) as usize;
    let spacing = "\n".repeat(scale);
    if let Some(rows) = value.as_array() {
        for row in rows {
            io::stdout(format!("{row}{spacing}").as_bytes())?;
        }
        Ok(())
    } else {
        io::stdout(format!("{value}{spacing}").as_bytes())
    }
}

pub fn csv_view(bytes: &[u8], kind: Kind, start: usize, count: usize) -> Result<Value> {
    let rows: Vec<Vec<Vec<u8>>> = match kind {
        Kind::GpsCsv => cs705::gps::Csv::parse(bytes)?
            .rows()
            .iter()
            .map(|r| r.cells().into_iter().map(<[u8]>::to_vec).collect())
            .collect(),
        Kind::RepeaterCsv => cs705::repeater::Csv::parse(bytes)?
            .rows()
            .iter()
            .map(|r| r.cells().into_iter().map(<[u8]>::to_vec).collect())
            .collect(),
        _ => return Err(AppError::invalid("expected a CSV kind")),
    };
    if start > rows.len() {
        return Err(AppError::invalid("start is outside CSV rows"));
    }
    let header = bytes
        .split(|b| *b == b'\n')
        .next()
        .ok_or_else(|| AppError::invalid("missing header"))?;
    let columns: Vec<_> = header
        .strip_suffix(b"\r")
        .unwrap_or(header)
        .split(|b| *b == b',')
        .map(|c| std::str::from_utf8(c).map(str::to_owned))
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| AppError::invalid("invalid header text"))?;
    Ok(json!(
        rows.iter()
            .enumerate()
            .skip(start)
            .take(count)
            .map(|(index, row)| {
                let mut object = Map::new();
                object.insert("index".into(), json!(index));
                for (key, value) in columns.iter().zip(row) {
                    object.insert(key.clone(), io::bytes_value(value));
                }
                Value::Object(object)
            })
            .collect::<Vec<_>>()
    ))
}

pub fn edit_csv(bytes: &[u8], kind: Kind, assignments: &[String]) -> Result<Vec<u8>> {
    let edits = assignments
        .iter()
        .map(|s| {
            let (path, value) = s
                .split_once('=')
                .ok_or_else(|| AppError::invalid("assignment requires ROW/FIELD=VALUE"))?;
            let (row, field) = path
                .split_once('/')
                .ok_or_else(|| AppError::invalid("CSV field path requires ROW/FIELD"))?;
            Ok((values::parse::<usize>(row)?, field, value))
        })
        .collect::<Result<Vec<_>>>()?;
    match kind {
        Kind::GpsCsv => {
            let mut csv = cs705::gps::Csv::parse(bytes)?;
            for (row, field, value) in edits {
                match field {
                    "group" => csv.set_group(row, values::parse(value)?)?,
                    "group-name" => {
                        csv.set_group_name(row, values::parse::<String>(value)?.as_bytes())?
                    }
                    "name" => csv.set_name(row, values::parse::<String>(value)?.as_bytes())?,
                    "latitude" => csv.set_latitude(row, values::parse::<GpsCoordinate>(value)?)?,
                    "longitude" => {
                        csv.set_longitude(row, values::parse::<GpsCoordinate>(value)?)?
                    }
                    "altitude" => csv.set_altitude(row, values::parse::<GpsAltitude>(value)?)?,
                    "alarm" => csv.set_alarm(row, values::parse(value)?)?,
                    _ => return Err(AppError::invalid("unknown or read-only GPS CSV field")),
                }
            }
            Ok(csv.to_bytes())
        }
        Kind::RepeaterCsv => {
            let mut csv = cs705::repeater::Csv::parse(bytes)?;
            for (row, field, value) in edits {
                match field {
                    "group" => csv.set_group_number(row, values::parse(value)?)?,
                    "group-name" => {
                        csv.set_group_name(row, values::parse::<String>(value)?.as_bytes())?
                    }
                    "name" => csv.set_name(row, values::parse::<String>(value)?.as_bytes())?,
                    "sub-name" => {
                        csv.set_sub_name(row, values::parse::<String>(value)?.as_bytes())?
                    }
                    "repeater-call" => {
                        csv.set_repeater_call_sign(row, values::parse::<String>(value)?.as_bytes())?
                    }
                    "gateway-call" => {
                        csv.set_gateway_call_sign(row, values::parse::<String>(value)?.as_bytes())?
                    }
                    "frequency-hz" => csv.set_frequency_hz(row, values::parse(value)?)?,
                    "offset-hz" => csv.set_offset_hz(row, values::parse(value)?)?,
                    "mode" => csv.set_mode(row, values::parse::<RepeaterMode>(value)?)?,
                    "duplex" => csv.set_duplex(row, values::parse::<RepeaterDuplex>(value)?)?,
                    "tone" => csv.set_tone(row, values::parse::<RepeaterTone>(value)?)?,
                    "tone-index" => csv.set_repeater_tone_index(row, values::parse(value)?)?,
                    "use" => csv.set_rpt1_use(row, values::parse(value)?)?,
                    "position" => {
                        csv.set_position(row, values::parse::<RepeaterPosition>(value)?)?
                    }
                    "latitude" => {
                        csv.set_latitude(row, values::parse::<RepeaterCoordinate>(value)?)?
                    }
                    "longitude" => {
                        csv.set_longitude(row, values::parse::<RepeaterCoordinate>(value)?)?
                    }
                    "utc-offset-minutes" => {
                        csv.set_utc_offset_minutes(row, values::parse(value)?)?
                    }
                    _ => return Err(AppError::invalid("unknown repeater CSV field")),
                }
            }
            Ok(csv.to_bytes())
        }
        _ => Err(AppError::invalid("CSV edit requires a CSV kind")),
    }
}

pub fn row(input: &str, sheet: Sheet, action: RowAction) -> Result<()> {
    if sheet == Sheet::Call
        || (matches!(sheet, Sheet::FromDv | Sheet::FromFm | Sheet::To | Sheet::Rx)
            && matches!(&action, RowAction::Delete { .. }))
    {
        return crate::records::row(input, sheet, action);
    }
    Err(pending("row operation"))
}

fn gps_source_error(reason: &'static str) -> cs705::icf::Error {
    cs705::icf::Error::Invalid { line: 0, reason }
}

fn gps_notify(event: cs705::gps::operations::Event) -> std::result::Result<(), cs705::icf::Error> {
    io::report(&json!({"notification_requested":{"id":event.id,"synchronous":event.synchronous,"arg1":event.arg1,"arg2":event.arg2},"delivery":"request-only"}))
        .map_err(|_| gps_source_error("GPS notification report failed"))
}

fn gps_live_seed(
    image: &Icf<'_>,
    runtime: &crate::records::Runtime,
) -> std::result::Result<cs705::gps::operations::Record, cs705::icf::Error> {
    let q = runtime
        .display_context()
        .map_err(|_| gps_source_error("GPS initializer requires live record context"))?;
    let clock = runtime
        .inline_clock()
        .map_err(|_| gps_source_error("GPS initializer requires a valid local clock source"))?;
    cs705::icf::record::gps_initial::seed(image, q, runtime.display_system_langid(), clock)
}

/// Keep Q/Clock reads after each metadata allocation, including their failures.
pub(crate) fn gps_insert_live(
    image: &mut Icf<'_>,
    group: usize,
    index: usize,
    requested: usize,
    selection: (usize, usize),
    runtime: &crate::records::Runtime,
) -> Result<cs705::gps::operations::Outcome> {
    Ok(cs705::gps::operations::insert(
        image,
        group,
        index,
        requested,
        selection,
        |image| gps_live_seed(image, runtime),
        |_, event| gps_notify(event),
    )?)
}

pub fn gps_row(input: &str, action: RowAction, options: &crate::records::Options) -> Result<u8> {
    use cs705::gps::operations as gps;
    let extra = match &action {
        RowAction::Paste { clipboard, .. } => Some(clipboard.as_str()),
        RowAction::Clear { seed, .. } | RowAction::Insert { seed, .. } => seed.as_deref(),
        _ => None,
    };
    let runtime = options.load(std::iter::once(input).chain(extra))?;
    if options.side_view || options.show_both.is_some() {
        return Err(AppError::invalid(
            "GPS rows do not have normal-memory side views",
        ));
    }
    let bytes = io::read(input)?;
    let mut image = Icf::parse(&bytes)?;
    let before = image.memory().to_vec();
    let write = match &action {
        RowAction::Copy { write, .. }
        | RowAction::Paste { write, .. }
        | RowAction::Move { write, .. }
        | RowAction::Clear { write, .. }
        | RowAction::Delete { write, .. }
        | RowAction::Insert { write, .. } => write,
    };
    let yes = match &action {
        RowAction::Paste { yes, .. }
        | RowAction::Clear { yes, .. }
        | RowAction::Delete { yes, .. }
        | RowAction::Insert { yes, .. } => *yes,
        _ => false,
    };
    if options.accept_partial {
        confirm(yes)?;
    }
    if let RowAction::Copy { group, index, .. } = &action {
        let records = gps::copy(&image, *group, *index, options.end_index.unwrap_or(*index))?;
        io::publish(
            &io::json_bytes(
                &json!({"version":1,"sheet":"gps","records":records.iter().map(ValueCodec::json).collect::<Vec<_>>()}),
            )?,
            write,
        )?;
        return Ok(0);
    }
    let result = (|| -> Result<bool> {
        match &action {
            RowAction::Delete {
                group, index, yes, ..
            } => {
                confirm(*yes)?;
                let end = options.end_index.unwrap_or(*index);
                let requested = end
                    .checked_sub(*index)
                    .and_then(|n| n.checked_add(1))
                    .ok_or_else(|| AppError::invalid("invalid GPS deletion range"))?;
                gps::delete(&mut image, *group, *index, requested, |_, event| {
                    gps_notify(event)
                })?;
                Ok(true)
            }
            RowAction::Move {
                group,
                index,
                destination_group,
                destination_index,
                ..
            } => {
                if group != destination_group || index.abs_diff(*destination_index) != 1 {
                    return Err(AppError::invalid(
                        "GPS move currently requires one adjacent row in the same group",
                    ));
                }
                gps::carry(
                    &mut image,
                    *group,
                    *index,
                    options.end_index.unwrap_or(*index),
                    if destination_index < index { -1 } else { 1 },
                    |_, event| gps_notify(event),
                )
                .map_err(Into::into)
            }
            RowAction::Copy { .. } => unreachable!("copy was handled before image editing"),
            RowAction::Paste {
                group,
                index,
                clipboard,
                yes,
                ..
            } => {
                confirm(*yes)?;
                let records = gps_clipboard(clipboard)?;
                let limit = if let Some(end) = options.end_index {
                    end.checked_sub(*index)
                        .and_then(|n| n.checked_add(1))
                        .ok_or_else(|| AppError::invalid("invalid GPS paste range"))?
                } else {
                    records.len()
                };
                gps::paste(&mut image, *group, *index, &records, limit)?;
                Ok(true)
            }
            RowAction::Clear {
                group,
                index,
                yes,
                seed,
                ..
            } => {
                confirm(*yes)?;
                let seed = if seed.is_some() {
                    gps_seed(seed.as_deref())?
                } else {
                    gps_live_seed(&image, &runtime)?
                };
                gps::clear(
                    &mut image,
                    *group,
                    *index,
                    options.end_index.unwrap_or(*index),
                    seed,
                )?;
                Ok(true)
            }
            RowAction::Insert {
                group, index, seed, ..
            } => {
                let end = options.end_index.unwrap_or(*index);
                let requested = end
                    .checked_sub(*index)
                    .and_then(|n| n.checked_add(1))
                    .ok_or_else(|| AppError::invalid("invalid GPS insertion range"))?;
                let outcome = if seed.is_some() {
                    let seed = gps_seed(seed.as_deref())?;
                    gps::insert(
                        &mut image,
                        *group,
                        *index,
                        requested,
                        (*index, end),
                        |_| Ok(seed),
                        |_, event| gps_notify(event),
                    )?
                } else {
                    gps_insert_live(
                        &mut image,
                        *group,
                        *index,
                        requested,
                        (*index, end),
                        &runtime,
                    )?
                };
                io::report(
                    &json!({"completed":outcome.completed,"capacity_stopped":outcome.capacity_stopped,"selection":outcome.selection}),
                )?;
                if outcome.capacity_stopped {
                    return Err(AppError {
                        code: 5,
                        message: "GPS insertion stopped at capacity; no output was published"
                            .into(),
                    });
                }
                Ok(outcome.completed != 0)
            }
        }
    })();
    match result {
        Ok(approved) => {
            io::report(&json!({"approved":approved,"partial_in_memory":image.memory()!=before}))?;
            if approved {
                io::publish(&image.to_bytes(), write)?;
            }
            Ok(0)
        }
        Err(error) => {
            let changed = image.memory() != before;
            io::report(
                &json!({"operation_error":error.message,"partial_in_memory":changed,"published":false}),
            )?;
            if options.accept_partial && changed {
                io::publish(&image.to_bytes(), write)?;
                io::report(&json!({"accepted_partial":true,"exit_code":5}))?;
                Ok(5)
            } else {
                Err(error)
            }
        }
    }
}

fn gps_clipboard(path: &str) -> Result<Vec<cs705::gps::operations::Record>> {
    let value: Value = serde_json::from_slice(&io::read(path)?)
        .map_err(|_| AppError::invalid("invalid clipboard JSON"))?;
    if !value.as_object().is_some_and(|o| o.len() == 3)
        || value["version"] != 1
        || value["sheet"] != "gps"
    {
        return Err(AppError::invalid(
            "clipboard requires version 1 and GPS records",
        ));
    }
    let records = value["records"]
        .as_array()
        .filter(|r| r.len() <= 300)
        .ok_or_else(|| AppError::invalid("clipboard requires at most 300 GPS records"))?;
    records
        .iter()
        .map(cs705::gps::operations::Record::parse)
        .collect()
}
fn gps_seed(path: Option<&str>) -> Result<cs705::gps::operations::Record> {
    let path = path.ok_or_else(|| {
        AppError::invalid("GPS seed path is required for explicit template input")
    })?;
    let records = gps_clipboard(path)?;
    if records.len() != 1 {
        return Err(AppError::invalid("GPS seed requires exactly one record"));
    }
    Ok(records[0])
}

pub fn export(
    input: &str,
    sheet: Sheet,
    group: Option<usize>,
    format: ExportFormat,
    write: &Write,
    options: &crate::records::Options,
) -> Result<()> {
    let runtime = options.load([input])?;
    let bytes = io::read(input)?;
    let image = Icf::parse(&bytes)?;
    if format == ExportFormat::Csv {
        return Err(AppError::invalid(
            "CSV export requires its dedicated command path",
        ));
    }
    let mut view = view(&image, sheet, 0, 2500, Some(&runtime))?;
    if let Some(group) = group {
        let slots = match sheet {
            Sheet::Memory => image
                .memory_groups()?
                .get(group)
                .ok_or_else(|| AppError::invalid("memory group is outside the visible groups"))?
                .slots()
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<usize>>(),
            Sheet::Gps => image
                .gps_groups()?
                .get(group)
                .ok_or_else(|| AppError::invalid("GPS group is outside the dataset"))?
                .physical_slots()
                .to_vec(),
            Sheet::Repeater if group < 50 => Vec::new(),
            _ => {
                return Err(AppError::invalid(
                    "group-scoped export requires a valid named-group sheet",
                ));
            }
        };
        view.as_array_mut()
            .ok_or_else(|| AppError::invalid("group-scoped view requires rows"))?
            .retain(|row| {
                if sheet == Sheet::Repeater {
                    row["group"].as_u64() == Some(group as u64)
                } else {
                    row["physical_slot"]
                        .as_u64()
                        .is_some_and(|slot| slots.contains(&(slot as usize)))
                }
            });
    }
    let bytes = match format {
        ExportFormat::Json => io::json_bytes(&view)?,
        ExportFormat::Text => format!("{view}\n").into_bytes(),
        ExportFormat::Csv => unreachable!(),
    };
    io::publish(&bytes, write)
}

pub fn config(file: &str, action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Show => {
            let value = load_config(file)?;
            io::stdout(&io::json_bytes(&value)?)
        }
        ConfigAction::Set { assignments, write } => {
            let missing = if file == "-" {
                false
            } else {
                match std::fs::metadata(file) {
                    Ok(_) => false,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
                    Err(e) => return Err(e.into()),
                }
            };
            let mut value = if missing {
                json!({})
            } else {
                serde_json::from_slice(&io::read(file)?)
                    .map_err(|_| AppError::invalid("invalid configuration JSON"))?
            };
            validate_config(&value)?;
            let object = value
                .as_object_mut()
                .ok_or_else(|| AppError::invalid("configuration must be an object"))?;
            for assignment in assignments {
                let (key, text) = assignment.split_once('=').ok_or_else(|| {
                    AppError::invalid("configuration assignment requires KEY=VALUE")
                })?;
                let value: Value = serde_json::from_str(text)
                    .map_err(|_| AppError::invalid("configuration values require JSON"))?;
                object.insert(key.into(), value);
            }
            validate_config(&value)?;
            io::publish(&io::json_bytes(&value)?, &write)
        }
    }
}

fn validate_config(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| AppError::invalid("configuration must be an object"))?;
    for (key, value) in object {
        let valid = match key.as_str() {
            "fixed-name" | "show-split" | "show-sql" | "toolbar" | "callsign-dialog"
            | "auto-correct" => value.is_boolean(),
            "columns" | "rx-columns" | "recent-files" => value
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string)),
            "text-scale" => value.as_u64().is_some_and(|n| (1..=4).contains(&n)),
            "font-percent" => value.as_u64().is_some_and(|n| n <= u64::from(u32::MAX)),
            "language" => matches!(value.as_str(), Some("English" | "Japanese")),
            "native-display" => crate::display::settings(value).is_ok(),
            "port" => value.is_string(),
            _ => false,
        };
        if !valid {
            return Err(AppError::invalid(
                "unknown configuration key or invalid value type",
            ));
        }
    }
    Ok(())
}

fn pending(capability: &str) -> AppError {
    AppError::safety(format!(
        "{capability} requires the pending semantic library API; no output was published"
    ))
}
