//! Independent English views, plus an explicit native-state analysis oracle.

use crate::{
    AppError, Result,
    args::{Sheet, Write},
    io,
    values::ValueCodec,
};
use cs705::display::{self as lib, InputOptions, Settings};
use serde_json::{Value, json};

mod print_cells;
pub use print_cells::{print_cell, print_condition};
pub mod english;

/// Apply the original option initializer, then explicit application preferences.
/// Missing GUI language is not guessed from a host locale or record Q context.
pub fn input_options(preferences: &Value) -> Result<InputOptions> {
    let object = preferences
        .as_object()
        .ok_or_else(|| AppError::invalid("configuration must be an object"))?;
    let mut options = object
        .get("native-display")
        .map(settings)
        .transpose()?
        .map(|s| s.input_options())
        .unwrap_or_default();
    for (key, target) in [
        ("callsign-dialog", &mut options.callsign_dialog),
        ("auto-correct", &mut options.auto_correct),
    ] {
        if let Some(value) = object.get(key) {
            *target = u32::from(value.as_bool().ok_or_else(|| {
                AppError::invalid("callsign-dialog/auto-correct require booleans")
            })?);
        }
    }
    if let Some(language) = object.get("language") {
        options.gui_language = Some(match language.as_str() {
            Some("English") => 0,
            Some("Japanese") => 1,
            _ => return Err(AppError::invalid("language requires English or Japanese")),
        });
    }
    Ok(options)
}

fn exact<'a>(value: &'a Value, keys: &[&str]) -> Result<&'a serde_json::Map<String, Value>> {
    value
        .as_object()
        .filter(|o| o.len() == keys.len() && keys.iter().all(|k| o.contains_key(*k)))
        .ok_or_else(|| {
            AppError::invalid("native display source requires exactly its documented scalar fields")
        })
}

pub fn settings(value: &Value) -> Result<Settings> {
    exact(
        value,
        &[
            "c00", "c04", "c10", "c14", "c18", "c34", "c38", "c90", "c94", "c98",
        ],
    )?;
    Ok(Settings {
        language: u32::parse(&value["c00"])?,
        system_language: u32::parse(&value["c04"])?,
        font_percent: u32::parse(&value["c10"])?,
        toolbar: u32::parse(&value["c14"])?,
        split: u32::parse(&value["c18"])?,
        callsign_dialog: u32::parse(&value["c34"])?,
        auto_correct: u32::parse(&value["c38"])?,
        fixed_name: u32::parse(&value["c90"])?,
        column_bits: u32::parse(&value["c94"])?,
        column_bits_high: u32::parse(&value["c98"])?,
    })
}
fn settings_value(s: &Settings) -> Value {
    json!({"c00":s.language,"c04":s.system_language,"c10":s.font_percent,"c14":s.toolbar,"c18":s.split,"c34":s.callsign_dialog,"c38":s.auto_correct,"c90":s.fixed_name,"c94":s.column_bits,"c98":s.column_bits_high})
}
fn metrics(value: &Value) -> Result<lib::FontMetrics> {
    exact(
        value,
        &["em", "advance", "line", "dpi", "padding", "explicit_height"],
    )?;
    let metrics = lib::FontMetrics {
        em: i32::parse(&value["em"])?,
        advance: i32::parse(&value["advance"])?,
        line: i32::parse(&value["line"])?,
        dpi: i32::parse(&value["dpi"])?,
        padding: i32::parse(&value["padding"])?,
        explicit_height: i32::parse(&value["explicit_height"])?,
    };
    metrics.validate()?;
    Ok(metrics)
}
fn read_value(path: &str) -> Result<Value> {
    serde_json::from_slice(&io::read(path)?)
        .map_err(|_| AppError::invalid("invalid native display JSON source"))
}

/// Resource parser asks whether rows are enabled; native58Bxx returns SKIP.
/// Q is reborrowed per request. Only selector16/JPN reads SYSTEM LANGID.
fn resource_condition(
    name: &[u8],
    q: &cs705::icf::context::Context,
    langid: Option<u16>,
) -> Result<bool> {
    let raw = q.as_bytes();
    Ok(match name {
        b"Only_M_REP" => raw[0x15] == 1,
        b"Only_ARPT_Permit" => raw[0x13] == 1,
        b"Only_50M_Enable" => q.effective_f()? != 0,
        b"Only_TX50M_Enable" => {
            q.effective_f()? != 0
                && !(q.selector()? == 4 && u32::from_le_bytes(raw[8..12].try_into().unwrap()) == 3)
        }
        b"Only_TX70M_Enable" => q.effective_f()? != 0 && q.effective_u()? != 0,
        b"Only_TX430M_Enable" => {
            !(q.selector()? == 4 && u32::from_le_bytes(raw[8..12].try_into().unwrap()) == 3)
        }
        b"Only_FMradio_Air_Enable" => !q.condition_58c5c()?,
        b"Only_GENE_Enable" => raw[0x10] != 0,
        b"Only_USA" => q.selector()? == 1,
        b"Only_JPN" | b"Hide_IsJPN" => {
            let selector = q.selector()?;
            let japanese = selector == 0 || selector == 16 && q.source_selector(langid)? == 0;
            if name == b"Hide_IsJPN" {
                !japanese
            } else {
                japanese
            }
        }
        _ => return Err(AppError::invalid("unknown native resource condition")),
    })
}

#[derive(clap::Args)]
pub struct Args {
    /// Source ICF. Not needed for catalog or choice listings.
    pub input: Option<String>,
    /// Replay native display state for analysis, not the independent CLI UI.
    #[arg(long)]
    pub oracle: bool,
    #[arg(long, requires = "oracle")]
    pub working_image: bool,
    /// Exact live C00/C04/C10/C14/C18/C34/C38/C90/C94/C98 words, not defaults.
    #[arg(long, requires = "oracle")]
    pub state: Option<String>,
    /// Actual declared fixed-pitch em/advance/line/DPI source, not cell sizes.
    #[arg(long, requires = "oracle")]
    pub metrics: Option<String>,
    #[arg(long, value_enum)]
    pub sheet: Option<Sheet>,
    /// Read semantic field paths instead of a dataset view.
    #[arg(long, conflicts_with_all = ["sheet", "list_fields", "choices", "oracle"])]
    pub field: Vec<String>,
    /// List semantic paths and their domains without opening an image.
    #[arg(long, conflicts_with_all = ["sheet", "choices", "oracle"])]
    pub list_fields: bool,
    /// List semantic choices or the numeric input range for a field.
    #[arg(long, conflicts_with_all = ["sheet", "oracle"])]
    pub choices: Option<String>,
    #[arg(long, value_enum, default_value = "json", conflicts_with = "oracle")]
    pub format: Format,
    /// Independent SVG page dimensions, in pixels.
    #[arg(long, default_value_t = 1200)]
    pub page_width: i32,
    #[arg(long, default_value_t = 800)]
    pub page_height: i32,
    #[arg(long, default_value_t = 0)]
    pub group: i32,
    #[command(flatten)]
    pub record_options: crate::records::Options,
    /// Optional read-only PE source, used only by the analysis oracle.
    #[arg(long, requires = "oracle")]
    pub asset: Option<String>,
    /// Real parent cell: PARENT/KEY/INDEX/CANVAS (agc/filter/ts/tone-control).
    #[arg(long, requires = "oracle")]
    pub notice: Vec<String>,
    /// Additional actual registered receivers, in registration order: SHEET/GROUP.
    #[arg(long, requires = "oracle")]
    pub receiver: Vec<String>,
    /// User menu gesture JSON: key,event,index,flags,disabled,confirmation.
    #[arg(long, requires = "oracle")]
    pub menu: Vec<String>,
    /// Actual toolbar left/top/width/height; this is not a saved page rectangle.
    #[arg(
        long,
        value_delimiter = ',',
        allow_hyphen_values = true,
        requires = "oracle"
    )]
    pub toolbar_placement: Vec<i32>,
    /// Deliver queued software display events after all supplied gestures.
    #[arg(long, requires = "oracle")]
    pub drain: bool,
    /// Show/recreate an invalidated font view using the new measured dimensions.
    #[arg(long, requires = "oracle")]
    pub show: bool,
    /// Explicit current data-row coordinate, before native fixed header rows.
    #[arg(long, default_value_t = 0)]
    pub current_row: i32,
    #[arg(long, default_value_t = 0)]
    pub scroll_x: i32,
    #[arg(long, default_value_t = 0)]
    pub start: usize,
    #[arg(long, default_value_t = 100)]
    pub count: usize,
    #[command(flatten)]
    pub write: Write,
}

#[derive(clap::ValueEnum, Clone, Copy)]
pub enum Format {
    Json,
    Text,
    Svg,
}

fn kind(sheet: Sheet) -> Result<cs705::printing::ViewKind> {
    use cs705::printing::ViewKind as V;
    match sheet {
        Sheet::Memory => Ok(V::Memory),
        Sheet::Call => Ok(V::Call),
        Sheet::Bandstack => Ok(V::Bandstack),
        Sheet::Memopad => Ok(V::Memopad),
        Sheet::Rx => Ok(V::Rx),
        _ => Err(AppError::invalid(
            "native display supports memory/call/bandstack/memopad/rx",
        )),
    }
}

pub fn run(args: Args, preferences: &Value) -> Result<()> {
    if args.oracle {
        return run_oracle(args, preferences);
    }
    let value = if args.list_fields {
        english::catalog()
    } else if let Some(key) = &args.choices {
        let bytes = args.input.as_deref().map(io::read).transpose()?;
        let image = bytes.as_deref().map(cs705::icf::Icf::parse).transpose()?;
        english::choices(key, image.as_ref())?
    } else {
        let input = args
            .input
            .as_deref()
            .ok_or_else(|| AppError::invalid("display needs an input image"))?;
        let runtime = args
            .record_options
            .load([input])?
            .with_input_options(input_options(preferences)?);
        let bytes = io::read(input)?;
        let image = cs705::icf::Icf::parse(&bytes)?;
        if !args.field.is_empty() {
            english::read(&image, &args.field, &runtime)?
        } else {
            let sheet = args.sheet.unwrap_or(Sheet::Memory);
            if sheet == Sheet::All {
                crate::sheets::view(&image, sheet, args.start, args.count, Some(&runtime))?
            } else {
                let mut value = crate::printing::view_rows(&image, sheet, args.group, &runtime)?;
                let end = args
                    .start
                    .checked_add(args.count)
                    .ok_or_else(|| AppError::invalid("display row range overflow"))?;
                let rows = value.as_array_mut().expect("typed view rows");
                if args.start > rows.len() {
                    return Err(AppError::invalid("display start is outside the view"));
                }
                *rows = rows[args.start..end.min(rows.len())].to_vec();
                crate::sheets::present(value, &[], Some(sheet), preferences)?
            }
        }
    };
    let bytes = match args.format {
        Format::Json => io::json_bytes(&value)?,
        Format::Text => format!(
            "{}\n",
            serde_json::to_string_pretty(&value)
                .map_err(|_| AppError::invalid("display serialization failed"))?
        )
        .into_bytes(),
        Format::Svg => crate::printing::svg_document(
            "Configuration view",
            &value,
            args.page_width,
            args.page_height,
        )?,
    };
    io::publish(&bytes, &args.write)
}

fn run_oracle(args: Args, preferences: &Value) -> Result<()> {
    let input = args
        .input
        .as_deref()
        .ok_or_else(|| AppError::invalid("display oracle needs an input image"))?;
    let metric = args
        .metrics
        .as_deref()
        .ok_or_else(|| AppError::invalid("display oracle needs --metrics"))?;
    let sheet = args
        .sheet
        .ok_or_else(|| AppError::invalid("display oracle needs --sheet"))?;
    io::single_stdin(
        [
            Some(input),
            args.state.as_deref(),
            Some(metric),
            args.record_options.record_context.as_deref(),
            args.asset.as_deref(),
        ]
        .into_iter()
        .flatten(),
    )?;
    args.start
        .checked_add(args.count)
        .ok_or_else(|| AppError::invalid("native display row range overflow"))?;
    let bytes = io::read(input)?;
    let current = if args.working_image {
        bytes
    } else {
        let image = cs705::icf::Icf::parse(&bytes)?;
        let mut working = cs705::icf::rows::NormalMemory::from_icf(&image)?
            .working()
            .to_vec();
        cs705::repeater::working::expand_counts(&mut working)?;
        working
    };
    let q = args
        .record_options
        .record_context
        .as_deref()
        .map(|path| -> Result<_> {
            Ok(cs705::icf::context::Context::from_bytes(
                io::read(path)?
                    .try_into()
                    .map_err(|_| AppError::invalid("record context needs24 raw bytes"))?,
            ))
        })
        .transpose()?;
    let placement: [i32; 4] = args
        .toolbar_placement
        .try_into()
        .map_err(|_| AppError::invalid("toolbar placement needs four explicit scalars"))?;
    let mut configuration = if let Some(path) = args.state.as_deref() {
        settings(&read_value(path)?)?
    } else {
        settings(preferences.get("native-display").ok_or_else(|| {
            AppError::invalid("display needs --state or explicit --config native-display source")
        })?)?
    };
    // Application preferences apply to the owned controls, not terminal spacing.
    for (key, target) in [
        ("fixed-name", &mut configuration.fixed_name),
        ("show-split", &mut configuration.split),
        ("toolbar", &mut configuration.toolbar),
    ] {
        if let Some(value) = preferences.get(key) {
            *target = u32::from(bool::parse(value)?);
        }
    }
    if let Some(value) = preferences.get("show-sql") {
        configuration.column_bits = configuration.column_bits & !1 | u32::from(bool::parse(value)?);
    }
    if let Some(value) = preferences.get("font-percent") {
        configuration.font_percent = u32::parse(value)?;
    }
    let options = input_options(preferences)?;
    if preferences.get("callsign-dialog").is_some() {
        configuration.callsign_dialog = options.callsign_dialog;
    }
    if preferences.get("auto-correct").is_some() {
        configuration.auto_correct = options.auto_correct;
    }
    if preferences.get("language").is_some() {
        configuration.language = options.gui_language.unwrap();
    }
    let mut state = lib::State::new(configuration, metrics(&read_value(metric)?)?, placement)?;
    let asset = args.asset.as_deref().map(io::read).transpose()?;
    let mut notices = Vec::new();
    for request in &args.notice {
        let parts: Vec<_> = request.split('/').collect();
        let [parent, key, index, canvas] = parts.as_slice() else {
            return Err(AppError::invalid("notice needs PARENT/KEY/INDEX/CANVAS"));
        };
        let parent = match *parent {
            "agc" => lib::NoticeParent::Agc,
            "filter" => lib::NoticeParent::Filter,
            "ts" => lib::NoticeParent::Ts,
            "tone-control" => lib::NoticeParent::ToneControl,
            _ => return Err(AppError::invalid("unknown notice parent")),
        };
        let notice = lib::notice_cell(
            asset.as_deref(),
            parent,
            crate::values::parse(key)?,
            crate::values::parse(index)?,
            crate::values::parse(canvas)?,
        )?
        .ok_or_else(|| AppError::invalid("cell is not a native shared-notice reference"))?;
        notices.push(json!({"topic":notice.topic,"key":notice.key,"index":notice.index,"text":io::bytes_value(&notice.text),"foreground":notice.foreground,"ascii_event_disabled":notice.ascii_event_disabled,"editable":notice.editable}));
    }
    state.open(&current, kind(sheet)?, args.group, q.as_ref())?;
    let id = state.current.as_ref().unwrap().view_id;
    state.views.states[id].current_row = args
        .current_row
        .wrapping_add(state.views.states[id].fixed_rows as i32);
    for receiver in args.receiver {
        let (sheet, group) = receiver
            .split_once('/')
            .ok_or_else(|| AppError::invalid("receiver needs SHEET/GROUP"))?;
        let kind = match sheet {
            "memory" => cs705::printing::ViewKind::Memory,
            "call" => cs705::printing::ViewKind::Call,
            _ => return Err(AppError::invalid("receiver needs memory or call")),
        };
        let view = cs705::printing::ViewState::from_image_with_split(
            &current,
            kind,
            group
                .parse()
                .map_err(|_| AppError::invalid("invalid receiver group"))?,
            state.settings.split,
        )?;
        state.views.register(view);
    }
    let mut menus = Vec::new();
    for menu in args.menu {
        let menu: Value = serde_json::from_str(&menu)
            .map_err(|_| AppError::invalid("menu requires JSON gesture"))?;
        exact(
            &menu,
            &["key", "event", "index", "flags", "disabled", "confirmation"],
        )?;
        let key = u32::parse(&menu["key"])?;
        let event = u32::parse(&menu["event"])?;
        let mut flags = u8::parse(&menu["flags"])?;
        let mut disabled = u8::parse(&menu["disabled"])?;
        state.menu(
            &current,
            key,
            event,
            u32::parse(&menu["index"])?,
            &mut flags,
            &mut disabled,
            if menu["confirmation"].is_null() {
                None
            } else {
                Some(i32::parse(&menu["confirmation"])?)
            },
        )?;
        menus.push(json!({"key":key,"event":event,"flags":flags,"disabled":disabled,"checked":flags & 128 != 0}));
    }
    if args.drain {
        state.drain(&current, q.as_ref())?;
    }
    if args.show {
        state.show(&current, q.as_ref())?;
    }
    let grid = state.current.as_ref().map(|grid| json!({
        "sql_mask":grid.sql_mask,"frozen_columns":grid.frozen_columns,"selected_column":grid.selected_column,
        "font_height":grid.font_height,"language":grid.language,"rebuilds":grid.rebuilds,
        "row_count":grid.rows.len(),"current_row":state.views.states[grid.view_id].current_row,
        "columns":grid.columns.iter().zip(grid.column_positions(args.scroll_x)).map(|(c,x)|json!({"key":c.key,"width":c.width,"measured_width":c.measured_width,"hidden":c.hidden,"x":x})).collect::<Vec<_>>(),
        "rows":grid.rows.iter().skip(args.start).take(args.count).map(|r|json!({"display_index":r.display_index,"typed_index":r.typed_index,"side":r.side,"valid":r.valid,"height":r.height,"cells":r.cells.iter().map(|c|json!({"key":c.key,"text":io::bytes_value(&c.text),"width":c.width,"height":c.height})).collect::<Vec<_>>()})).collect::<Vec<_>>()
    }));
    let value = json!({"operation":"native-display","settings":settings_value(&state.settings),"menus":menus,"notices":notices,"font_changed":state.font_changed,
        "resource_language":state.resource_language,"resource_generation":state.resource_generation,
        "toolbar":{"visible":state.toolbar.visible,"placement":state.toolbar.placement,"occupied_height":state.toolbar.occupied_height(),"visible_changes":state.toolbar.visible_changes},
        "queued":state.queue.iter().map(|e|json!({"event":e.event,"argument":e.argument,"value":e.value})).collect::<Vec<_>>(),"grid":grid,
        "measurement_source":"explicit-fixed-pitch; not native GDI or OS delivery","cells":"typed values; native caption/locale formatting is not replaced"});
    io::publish(&io::json_bytes(&value)?, &args.write)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resource_conditions_use_live_q_and_only_reached_system_language() {
        use cs705::icf::context::Context;
        let mut bytes = [0; 24];
        bytes[0x10] = 1;
        bytes[0x13] = 1;
        bytes[0x15] = 1;
        let q = Context::from_bytes(bytes);
        for (name, expected) in [
            (b"Only_M_REP".as_slice(), true),
            (b"Only_ARPT_Permit", true),
            (b"Only_50M_Enable", false),
            (b"Only_TX50M_Enable", false),
            (b"Only_TX70M_Enable", false),
            (b"Only_TX430M_Enable", true),
            (b"Only_FMradio_Air_Enable", true),
            (b"Only_GENE_Enable", true),
            (b"Only_USA", false),
            (b"Only_JPN", true),
            (b"Hide_IsJPN", false),
        ] {
            assert_eq!(resource_condition(name, &q, None).unwrap(), expected);
        }
        bytes[..4].copy_from_slice(&16_u32.to_le_bytes());
        let q = Context::from_bytes(bytes);
        assert!(resource_condition(b"Only_JPN", &q, None).is_err());
        assert!(!resource_condition(b"Only_USA", &q, None).unwrap());
        assert!(resource_condition(b"Only_JPN", &q, Some(0x411)).unwrap());
        assert!(resource_condition(b"Hide_IsJPN", &q, Some(0x409)).unwrap());
        assert!(resource_condition(b"unknown", &q, None).is_err());
        bytes[..4].copy_from_slice(&5_u32.to_le_bytes());
        bytes[4] = 1;
        bytes[8] = 3;
        bytes[14] = 1;
        bytes[15] = 1;
        let q = Context::from_bytes(bytes);
        for name in [
            b"Only_50M_Enable".as_slice(),
            b"Only_TX50M_Enable",
            b"Only_TX70M_Enable",
            b"Only_TX430M_Enable",
            b"Only_FMradio_Air_Enable",
        ] {
            assert!(!resource_condition(name, &q, None).unwrap());
        }
        bytes[4] = 0;
        bytes[8] = 0;
        let q = Context::from_bytes(bytes);
        assert!(resource_condition(b"Only_TX50M_Enable", &q, None).unwrap());
        assert!(resource_condition(b"Only_TX70M_Enable", &q, None).unwrap());
    }
}
