//! Thin offline adapters for CS special-memory operation cutpoints.
//!
//! Shared I/O owns stdin and atomic publication. Library code owns storage,
//! dependent fields and source-like partial mutation. A failed offline command
//! reports an in-memory prefix but never publishes it. GUI requests are machine
//! reports, not device or native GUI calls. Names use explicit printable ASCII.

use crate::{
    AppError, Result,
    args::{Format, Kind, Read, Write},
    confirm, fields, io,
    native::{read as native_context, report as native_report},
    render, values,
};
use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use cs705::icf::{self, Icf, special as lib};
use serde_json::{Value, json};
use std::ops::Range;

#[derive(ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Display/copy/reset band stacking. There is no normal field setter.
    Bandstack(CompactArgs),
    /// Display/copy/reset memopad rows 1..10; reserved row zero is not shown.
    Memopad(CompactArgs),
    /// Program scan fields, decoded clipboard, rows and selection.
    Scan(ScanArgs),
    /// User band edge pairs; only their frequencies are editable.
    UserEdge(UserArgs),
    /// Preset registration, not radio recall or application.
    Preset(PresetArgs),
    /// Show the preset key/mask catalog and accepted CLI field spellings.
    Fields,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum Sheet {
    Bandstack,
    Memopad,
    Scan,
    UserEdge,
    Preset,
}

#[derive(ClapArgs)]
struct View {
    #[arg(long, default_value_t = 0)]
    start: usize,
    #[arg(long, default_value_t = 100)]
    count: usize,
}
#[derive(ClapArgs)]
struct Rows {
    start: usize,
    #[arg(long, default_value_t = 1)]
    count: usize,
}
impl Rows {
    fn range(&self) -> Result<Range<usize>> {
        if self.count == 0 {
            return Err(AppError::invalid("row count must be positive"));
        }
        Ok(self.start
            ..self
                .start
                .checked_add(self.count)
                .ok_or_else(|| AppError::invalid("row range overflow"))?)
    }
}
#[derive(ClapArgs)]
struct Edit {
    index: usize,
    /// Repeat FIELD=VALUE in operation order; names are printable ASCII.
    #[arg(long = "set", required = true)]
    edits: Vec<String>,
    /// Use false/ESC helper results and publish no payload.
    #[arg(long)]
    cancel: bool,
    /// Explicit completed raw generation G (0x4997a bytes), when needed.
    #[arg(long)]
    seed: Option<String>,
    #[command(flatten)]
    write: Write,
}

#[derive(ClapArgs)]
struct CompactArgs {
    #[command(flatten)]
    read: Read,
    #[command(subcommand)]
    action: CompactAction,
}
#[derive(Subcommand)]
enum CompactAction {
    Show(View),
    /// Export a decoded source clipboard; normal/call paste is a separate consumer.
    Copy {
        #[command(flatten)]
        rows: Rows,
        /// Include all band rows rather than the original 27..29 exclusion.
        #[arg(long)]
        only: bool,
        #[command(flatten)]
        write: Write,
    },
    Reset {
        #[arg(long)]
        seed: String,
        #[arg(long)]
        only: bool,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        write: Write,
    },
}

#[derive(ClapArgs)]
struct ScanArgs {
    #[command(flatten)]
    read: Read,
    #[command(subcommand)]
    action: ScanAction,
}
#[derive(ClapArgs)]
struct ScanEdit {
    #[command(flatten)]
    edit: Edit,
    /// Explicit generation-Q/live-Q/SYSTEM LANGID JSON; required for Hz fields.
    #[arg(long)]
    range_context: Option<String>,
}
#[derive(Subcommand)]
enum ScanAction {
    Show(View),
    /// Edit name, mode (decoded code), data, filter, low-hz or high-hz.
    Edit(ScanEdit),
    Copy {
        #[command(flatten)]
        rows: Rows,
        #[command(flatten)]
        write: Write,
    },
    Paste {
        index: usize,
        #[arg(long)]
        clipboard: String,
        #[arg(long)]
        seed: Option<String>,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        write: Write,
    },
    Move {
        #[command(flatten)]
        rows: Rows,
        #[arg(value_enum)]
        direction: Direction,
        #[command(flatten)]
        write: Write,
    },
    Clear(RowReset),
    Delete(RowReset),
    Insert(RowReset),
    /// Accept a typed eligible scan row, without the frequency editor's count 1 gate.
    Select {
        index: usize,
        #[arg(long)]
        seed: Option<String>,
        #[command(flatten)]
        write: Write,
    },
}
#[derive(ValueEnum, Clone, Copy)]
enum Direction {
    Up,
    Down,
}
#[derive(ClapArgs)]
struct RowReset {
    #[command(flatten)]
    rows: Rows,
    #[arg(long)]
    seed: String,
    #[arg(long)]
    yes: bool,
    #[command(flatten)]
    write: Write,
}
#[derive(ClapArgs)]
struct UserArgs {
    #[command(flatten)]
    read: Read,
    #[command(subcommand)]
    action: UserAction,
}
#[derive(Subcommand)]
enum UserAction {
    Show(View),
    Edit(UserEdit),
}
#[derive(ClapArgs)]
struct UserEdit {
    index: usize,
    #[arg(long = "set", required = true)]
    edits: Vec<String>,
    #[arg(long)]
    cancel: bool,
    /// Explicit generation-Q/live-Q/SYSTEM LANGID JSON, never inferred from G.
    #[arg(long)]
    range_context: String,
    #[command(flatten)]
    write: Write,
}

#[derive(ClapArgs)]
struct PresetArgs {
    #[command(flatten)]
    read: Read,
    #[command(subcommand)]
    action: PresetAction,
}
#[derive(Subcommand)]
enum PresetAction {
    Show(View),
    /// Edit catalog fields. Numeric fields accept editor text or action JSON.
    Edit(Edit),
    Toggle {
        index: usize,
        #[arg(value_enum)]
        field: PresetField,
        #[arg(long)]
        seed: Option<String>,
        #[command(flatten)]
        write: Write,
    },
    Name {
        index: usize,
        name: String,
        #[arg(long)]
        seed: Option<String>,
        #[arg(long)]
        cancel: bool,
        #[command(flatten)]
        write: Write,
    },
    Reset {
        index: usize,
        #[arg(long)]
        seed: String,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        write: Write,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum PresetField {
    Mode,
    Filter,
    BandwidthHz,
    FilterTypeHf,
    FilterType50m,
    Comp,
    DataOffMod,
    DataOnMod,
    Tbw,
    SsbLow,
    SsbHigh,
    SsbDataLow,
    SsbDataHigh,
    UsbOutput,
    UsbModLevel,
    UsbAfLevel,
    UsbAfSquelch,
    UsbIfLevel,
    UsbSend,
    UsbCw,
    UsbRtty,
    CivAddress,
    CivTransceive,
    UsbEcho,
}
impl PresetField {
    fn key_side(self) -> (u16, usize) {
        match self {
            Self::Mode => (0x387, 0),
            Self::Filter => (0x388, 0),
            Self::BandwidthHz => (0x389, 0),
            Self::FilterTypeHf => (0x38a, 0),
            Self::FilterType50m => (0x38b, 0),
            Self::Comp => (0x394, 0),
            Self::DataOffMod => (0x393, 0),
            Self::DataOnMod => (0x391, 0),
            Self::Tbw => (0x395, 0),
            Self::SsbLow => (0x396, 0),
            Self::SsbHigh => (0x396, 1),
            Self::SsbDataLow => (0x392, 0),
            Self::SsbDataHigh => (0x392, 1),
            Self::UsbOutput => (0x38c, 0),
            Self::UsbModLevel => (0x390, 0),
            Self::UsbAfLevel => (0x38d, 0),
            Self::UsbAfSquelch => (0x38e, 0),
            Self::UsbIfLevel => (0x38f, 0),
            Self::UsbSend => (0x397, 0),
            Self::UsbCw => (0x398, 0),
            Self::UsbRtty => (0x399, 0),
            Self::CivAddress => (0x39a, 0),
            Self::CivTransceive => (0x39b, 0),
            Self::UsbEcho => (0x39c, 0),
        }
    }
    fn descriptor(self) -> &'static lib::PresetField {
        lib::PRESET_CATALOG
            .iter()
            .find(|f| f.key == self.key_side().0)
            .expect("CLI fields match library catalog")
    }
    fn parse_choice(self, text: &str, image: &Icf<'_>, p: usize) -> Result<i32> {
        let value = values::parse::<i32>(text)?;
        let count = self.descriptor().choices;
        if count != 0 && !(0..count as i32).contains(&value) {
            return Err(AppError::invalid(
                "preset value is outside its catalog choices",
            ));
        }
        match self {
            Self::UsbCw | Self::UsbRtty => {
                let h = lib::Preset::read(image.memory(), p)?;
                if !h.keying_candidate_allowed(
                    usize::from(matches!(self, Self::UsbRtty)),
                    value as u32,
                )? {
                    return Err(AppError::invalid(
                        "keying candidate conflicts with enabled USB SEND",
                    ));
                }
            }
            _ => {}
        }
        Ok(value)
    }
}

/// Reusable show --sheet adapter. Zero is skipped for memopad, not remapped to
/// a visible row. Unknown stored fields remain decoded numbers and escaped bytes.
pub fn view(image: &Icf<'_>, sheet: Sheet, start: usize, count: usize) -> Result<Value> {
    let (low, high) = match sheet {
        Sheet::Bandstack => (0, 45),
        Sheet::Memopad => (1, 11),
        Sheet::Scan => (0, 25),
        Sheet::UserEdge => (0, 30),
        Sheet::Preset => (0, 5),
    };
    let start = start.max(low);
    if start > high {
        return Err(AppError::invalid("start is outside the special sheet"));
    }
    let end = start
        .checked_add(count)
        .ok_or_else(|| AppError::invalid("row range overflow"))?
        .min(high);
    let mut rows = Vec::new();
    for row in start..end {
        rows.push(match sheet {
            Sheet::Bandstack|Sheet::Memopad=>compact_view(row,&lib::CompactChannel::read(image.memory(),
                if matches!(sheet,Sheet::Bandstack) {lib::CompactKind::BandStack} else {lib::CompactKind::Memopad},row)?),
            Sheet::Scan=>scan_json(row,&lib::ScanEdge::read(image.memory(),row)?),
            Sheet::UserEdge=>{let h=lib::UserEdge::read(image.memory(),row)?;json!({"index":row,"valid":h.valid,"low_hz":h.low_hz,"high_hz":h.high_hz})},
            Sheet::Preset=>{let h=lib::Preset::read(image.memory(),row)?;
                let fields=PresetField::value_variants().iter().map(|&f| {
                    let (key,side)=f.key_side();let d=f.descriptor();
                    let value=if key==0x387 {json!(h.combined_mode())} else if key==0x389 {
                        h.bandwidth_hz().map_or_else(|e|json!({"error":e.to_string()}),|v|json!(v))
                    } else {json!(h.body[d.offset+side])};
                    (f.to_possible_value().expect("CLI field spelling").get_name().to_owned(),value)
                }).collect::<serde_json::Map<_,_>>();
                json!({"index":row,"fields":fields,"masks":h.masks.as_slice(),"name":io::bytes_value(&h.body[25..]),"has_enabled_fields":h.has_enabled_fields()?})},
        });
    }
    Ok(json!(rows))
}
fn compact_json(index: usize, h: &lib::CompactChannel) -> Value {
    json!({"index":index,"valid":h.valid,"frequency_hz":h.frequency_hz,"duplex":h.duplex,
        "offset_hz":h.offset_hz,"mode":h.mode,"data":h.data,"filter":h.filter,"tone":h.tone,
        "tx_tone":h.tx_tone,"rx_tone":h.rx_tone,"dtcs":h.dtcs,"polarity":h.polarity,
        "digital_squelch":h.digital_squelch,"digital_code":h.digital_code,
        "callsigns":h.callsigns.iter().map(|c|io::bytes_value(c)).collect::<Vec<_>>()})
}
fn compact_view(index: usize, h: &lib::CompactChannel) -> Value {
    use lib::CompactField::*;
    let mut value = compact_json(index, h);
    let display = [
        Frequency,
        Duplex,
        Offset,
        Mode,
        Data,
        Filter,
        Tone,
        TxTone,
        RxTone,
        Dtcs,
        Polarity,
        DigitalSquelch,
        DigitalCode,
        Your,
        Repeater1,
        Repeater2,
    ]
    .into_iter()
    .map(|field| {
        let v = match h.display(field) {
            lib::DisplayValue::Blank => Value::Null,
            lib::DisplayValue::Placeholder => json!({"unavailable":true}),
            lib::DisplayValue::Number(n) => json!(n),
            lib::DisplayValue::Callsign(bytes) => io::bytes_value(&bytes),
        };
        (format!("{:03X}", field.key()), v)
    })
    .collect::<serde_json::Map<_, _>>();
    value["display"] = json!(display);
    value
}
fn scan_json(index: usize, h: &lib::ScanEdge) -> Value {
    json!({"index":index,"valid":h.valid,"low_hz":h.low_hz,"high_hz":h.high_hz,
        "mode":h.mode,"data":h.data,"filter":h.filter,"name":io::bytes_value(&h.name)})
}
fn event_json(n: &lib::Notification) -> Value {
    json!({"event":n.event,"synchronous":n.synchronous,"first":n.first,"second":n.second,"payload":io::bytes_value(&n.payload),"delivery":"terminal-report-only"})
}
fn effect_json(e: &lib::Effects) -> Value {
    json!({"writes":e.writes,"callback_flags":e.callback_flags,"phase":e.phase,"warning_bits":e.warning_bits,
        "gray":e.gray,"blocked":e.blocked,"node_style_clear":e.node_style_clear,
        "notifications":e.notifications.iter().map(event_json).collect::<Vec<_>>()})
}
fn state_json(s: &lib::RowState) -> Value {
    json!({"bits":s.bits,"start":s.start,"end":s.end,"active":s.active,"state":s.state})
}
fn inputs(read: &Read, extra: impl IntoIterator<Item = impl AsRef<str>>) -> Result<()> {
    let paths = extra
        .into_iter()
        .map(|s| s.as_ref().to_owned())
        .collect::<Vec<_>>();
    io::single_stdin(std::iter::once(read.input.as_str()).chain(paths.iter().map(String::as_str)))
}
fn load(bytes: &[u8], kind: Kind) -> Result<Icf<'_>> {
    match kind {
        Kind::Icf => Ok(Icf::parse(bytes)?),
        Kind::Memory => Ok(Icf::from_memory(bytes.to_vec())?),
        _ => Err(AppError::invalid(
            "special commands require --kind icf or memory",
        )),
    }
}
fn seed(path: Option<&str>) -> Result<Vec<u8>> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let bytes = io::read(path)?;
    if bytes.len() != cs705::clone::IMAGE_CAPACITY {
        return Err(AppError::invalid(
            "--seed requires a completed raw generation of exactly 0x4997a bytes",
        ));
    }
    Icf::from_memory(bytes[..0x3c9c0].to_vec())?;
    Ok(bytes)
}
fn name(text: &str) -> Result<Vec<u8>> {
    if text.len() > 16 || !text.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return Err(AppError::invalid(
            "name requires at most 16 printable ASCII bytes; no implicit ACP conversion",
        ));
    }
    Ok(icf::normalizer::normalize_name(
        &mut text.as_bytes().to_vec(),
        16,
    ))
}
fn assignment(text: &str) -> Result<(&str, &str)> {
    text.split_once('=')
        .ok_or_else(|| AppError::invalid("special edit requires FIELD=VALUE"))
}
fn checked<T>(
    result: std::result::Result<T, icf::Error>,
    image: &Icf<'_>,
    before: &[u8],
) -> Result<T> {
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            io::report(
                &json!({"published":false,"partial_in_memory":image.memory()!=before,"operation_error":e.to_string()}),
            )?;
            Err(e.into())
        }
    }
}
fn publish(image: &Icf<'_>, kind: Kind, write: &Write, report: Value, cancel: bool) -> Result<u8> {
    io::report(&report)?;
    if cancel {
        io::report(&json!({"cancelled":true,"published":false}))?;
        return Ok(0);
    }
    match kind {
        Kind::Icf => io::publish(&image.to_bytes(), write)?,
        Kind::Memory => io::publish(image.memory(), write)?,
        _ => unreachable!("validated kind"),
    }
    Ok(0)
}

fn compact(args: CompactArgs, kind: lib::CompactKind) -> Result<u8> {
    let CompactArgs { read, action } = args;
    let path = match &action {
        CompactAction::Reset { seed, .. } => Some(seed.as_str()),
        _ => None,
    };
    inputs(&read, path)?;
    if let CompactAction::Reset { yes, .. } = &action {
        confirm(*yes)?;
    }
    let bytes = io::read(&read.input)?;
    let mut image = load(&bytes, read.kind)?;
    match action {
        CompactAction::Show(v) => render(
            &view(
                &image,
                if kind == lib::CompactKind::BandStack {
                    Sheet::Bandstack
                } else {
                    Sheet::Memopad
                },
                v.start,
                v.count,
            )?,
            read.format,
        )?,
        CompactAction::Copy { rows, only, write } => {
            let mut clip = lib::CompactClipboard::default();
            clip.copy(image.memory(), kind, rows.range()?, only)?;
            io::publish(
                &io::json_bytes(
                    &json!({"version":1,"sheet":if kind==lib::CompactKind::BandStack {"bandstack"} else {"memopad"},"reserve_count":clip.reserve_count,"source_rows":clip.source_rows,
                "records":clip.channels.iter().enumerate().map(|(i,h)|compact_json(i,h)).collect::<Vec<_>>()}),
                )?,
                &write,
            )?;
        }
        CompactAction::Reset {
            seed: source,
            only,
            write,
            ..
        } => {
            let g = seed(Some(&source))?;
            let before = image.memory().to_vec();
            let result = lib::reset_compact(&mut image, &g, kind, 1, only);
            let e = checked(result, &image, &before)?;
            return publish(&image, read.kind, &write, effect_json(&e), false);
        }
    }
    Ok(0)
}

/// This call is unreachable by contract: broad-only suitability never invokes
/// a specific band predicate. It is not a fabricated all-frequency permission.
fn specific_disabled(_: u32) -> bool {
    unreachable!("specific predicate disabled by library call flags")
}

fn scan(args: ScanArgs) -> Result<u8> {
    let ScanArgs { read, action } = args;
    let extra = match &action {
        ScanAction::Edit(e) => e
            .edit
            .seed
            .iter()
            .chain(e.range_context.iter())
            .map(String::as_str)
            .collect(),
        ScanAction::Paste {
            clipboard, seed, ..
        } => std::iter::once(clipboard.as_str())
            .chain(seed.as_deref())
            .collect(),
        ScanAction::Clear(a) | ScanAction::Delete(a) | ScanAction::Insert(a) => {
            vec![a.seed.as_str()]
        }
        ScanAction::Select { seed, .. } => seed.iter().map(String::as_str).collect(),
        _ => Vec::new(),
    };
    inputs(&read, extra)?;
    match &action {
        ScanAction::Paste { yes, .. } => confirm(*yes)?,
        ScanAction::Clear(a) | ScanAction::Delete(a) | ScanAction::Insert(a) => confirm(a.yes)?,
        _ => {}
    }
    let bytes = io::read(&read.input)?;
    let mut image = load(&bytes, read.kind)?;
    let before = image.memory().to_vec();
    let mut state = lib::RowState::default();
    match action {
        ScanAction::Show(v) => render(&view(&image, Sheet::Scan, v.start, v.count)?, read.format)?,
        ScanAction::Edit(ScanEdit {
            edit,
            range_context,
        }) => {
            let g = seed(edit.seed.as_deref())?;
            let native = range_context.as_deref().map(native_context).transpose()?;
            let mut effects = Vec::new();
            for text in &edit.edits {
                let (field, value) = assignment(text)?;
                let h = lib::ScanEdge::read(image.memory(), edit.index)?;
                let (field, value, name) = match field {
                    "name" => (lib::ScanField::Name, None, Some(name(value)?)),
                    "mode" => {
                        let n = values::parse::<u32>(value)?;
                        if icf::record::decode_mode(icf::record::encode_mode(n)) != n
                            || !h.mode_candidate_allowed(n)
                        {
                            return Err(AppError::invalid(
                                "mode is not an allowed scan selector candidate",
                            ));
                        }
                        (lib::ScanField::Mode, Some(n), None)
                    }
                    "data" => {
                        let n = values::parse::<u32>(value)?;
                        if n > 1 {
                            return Err(AppError::invalid("scan data requires 0 or 1"));
                        }
                        (lib::ScanField::Data, Some(n), None)
                    }
                    "filter" => {
                        let n = values::parse::<u32>(value)?;
                        if !matches!(n, 0 | 1 | 2 | 255) || !h.filter_candidate_allowed(n) {
                            return Err(AppError::invalid(
                                "filter requires an allowed 0/1/2/255 candidate",
                            ));
                        }
                        (lib::ScanField::Filter, Some(n), None)
                    }
                    "low-hz" | "high-hz" => {
                        let candidate = values::parse::<u32>(value)?;
                        let context = native.as_ref().ok_or_else(|| {
                            AppError::invalid(
                                "--range-context is required for scan frequency edits",
                            )
                        })?;
                        let mut input = context.ranges.frequency_input(8, candidate)?;
                        input.first_accepted = !edit.cancel;
                        input.second_accepted = !edit.cancel;
                        let reset = lib::scan_reset_enabled(image.memory(), 0)?;
                        let result = lib::edit_scan_frequency(
                            &mut image,
                            &g,
                            edit.index,
                            if field == "low-hz" {
                                lib::EdgeSide::Low
                            } else {
                                lib::EdgeSide::High
                            },
                            input,
                            reset,
                            |f| context.ranges.eb64(&context.live_q, f),
                        );
                        let e = checked(result, &image, &before)?;
                        effects.push(effect_json(&e));
                        continue;
                    }
                    _ => {
                        return Err(AppError::invalid(
                            "unknown scan field; use name/mode/data/filter/low-hz/high-hz",
                        ));
                    }
                };
                if !h.valid {
                    return Err(AppError::invalid(
                        "empty scan row requires a frequency edit before other fields",
                    ));
                }
                let result = lib::edit_scan_field(
                    &mut image,
                    edit.index,
                    field,
                    if edit.cancel { None } else { value },
                    if edit.cancel { None } else { name.as_deref() },
                );
                let e = checked(result, &image, &before)?;
                if e.blocked {
                    io::report(&effect_json(&e))?;
                    return Err(AppError::invalid("scan field is blocked by its mode"));
                }
                effects.push(effect_json(&e));
            }
            return publish(
                &image,
                read.kind,
                &edit.write,
                json!({"effects":effects,"frequency_context":native.as_ref().map(native_report)}),
                edit.cancel,
            );
        }
        ScanAction::Copy { rows, write } => {
            let mut clip = lib::ScanClipboard::default();
            clip.copy(image.memory(), rows.range()?, &mut state)?;
            io::report(&state_json(&state))?;
            io::publish(
                &io::json_bytes(
                    &json!({"version":1,"sheet":"scan","transfer_count":clip.transfer_count,
                "records":clip.rows.iter().map(|h|json!({"valid":h.valid,"low_hz":h.low_hz,"high_hz":h.high_hz,"mode":h.mode,"data":h.data,"filter":h.filter,"name_hex":io::bytes_value(&h.name)["hex"]})).collect::<Vec<_>>()}),
                )?,
                &write,
            )?;
        }
        ScanAction::Paste {
            index,
            clipboard,
            seed: source,
            write,
            ..
        } => {
            let mut clip = scan_clipboard(&clipboard)?;
            let g = seed(source.as_deref())?;
            let reset = lib::scan_reset_enabled(image.memory(), 0)?;
            let result = clip.paste(
                &mut image,
                &g,
                index,
                lib::PasteApproval {
                    tail: true,
                    overwrite: true,
                },
                &mut state,
                reset,
            );
            let e = checked(result, &image, &before)?;
            return publish(
                &image,
                read.kind,
                &write,
                json!({"effects":effect_json(&e),"row_state":state_json(&state)}),
                false,
            );
        }
        ScanAction::Move {
            rows,
            direction,
            write,
        } => {
            let result = lib::scan_rows(
                &mut image,
                &[],
                rows.range()?,
                match direction {
                    Direction::Up => lib::ScanRowAction::Up,
                    Direction::Down => lib::ScanRowAction::Down,
                },
                true,
                &mut state,
                false,
            );
            let e = checked(result, &image, &before)?;
            return publish(
                &image,
                read.kind,
                &write,
                json!({"effects":effect_json(&e),"row_state":state_json(&state)}),
                false,
            );
        }
        ScanAction::Select {
            index,
            seed: source,
            write,
        } => {
            let h = lib::ScanEdge::read(image.memory(), index)?;
            if !h.suitable(false, false, false, specific_disabled) {
                return Err(AppError::invalid(
                    "typed scan selection requires a nonempty, unequal, broad-range row",
                ));
            }
            let reset = lib::scan_reset_enabled(image.memory(), 0)?;
            let g = seed(source.as_deref())?;
            let result = lib::select_scan(&mut image, &g, index, reset);
            checked(result, &image, &before)?;
            return publish(
                &image,
                read.kind,
                &write,
                json!({"selected":index,"reset":reset,"callback_flags":32}),
                false,
            );
        }
        action => {
            let (a, kind) = match action {
                ScanAction::Clear(a) => (a, lib::ScanRowAction::Clear),
                ScanAction::Delete(a) => (a, lib::ScanRowAction::Delete),
                ScanAction::Insert(a) => (a, lib::ScanRowAction::Insert),
                _ => unreachable!(),
            };
            let g = seed(Some(&a.seed))?;
            let reset = lib::scan_reset_enabled(image.memory(), 0)?;
            let result = lib::scan_rows(
                &mut image,
                &g,
                a.rows.range()?,
                kind,
                true,
                &mut state,
                reset,
            );
            let e = checked(result, &image, &before)?;
            return publish(
                &image,
                read.kind,
                &a.write,
                json!({"effects":effect_json(&e),"row_state":state_json(&state)}),
                false,
            );
        }
    }
    Ok(0)
}

fn scan_clipboard(path: &str) -> Result<lib::ScanClipboard> {
    let value: Value = serde_json::from_slice(&io::read(path)?)
        .map_err(|_| AppError::invalid("invalid scan clipboard JSON"))?;
    if !value.as_object().is_some_and(|o| {
        o.len() == 4
            && o.contains_key("version")
            && o.contains_key("sheet")
            && o.contains_key("records")
            && o.contains_key("transfer_count")
    }) || value["version"] != 1
        || value["sheet"] != "scan"
    {
        return Err(AppError::invalid("expected version1 scan clipboard"));
    }
    let records = value["records"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 25)
        .ok_or_else(|| AppError::invalid("scan clipboard requires 1..25 records"))?;
    let mut rows = Vec::new();
    for r in records {
        if !r.as_object().is_some_and(|o| {
            o.len() == 7
                && [
                    "valid", "low_hz", "high_hz", "mode", "data", "filter", "name_hex",
                ]
                .iter()
                .all(|k| o.contains_key(*k))
        }) {
            return Err(AppError::invalid(
                "scan clipboard record has missing or unknown members",
            ));
        }
        let number = |key: &str| -> Result<u32> {
            r[key]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| AppError::invalid("scan clipboard requires unsigned DWORD fields"))
        };
        let bytes = io::decode_hex(
            r["name_hex"]
                .as_str()
                .ok_or_else(|| AppError::invalid("scan clipboard requires name_hex"))?,
        )?;
        let name: [u8; 16] = bytes
            .try_into()
            .map_err(|_| AppError::invalid("scan clipboard name requires sixteen bytes"))?;
        let mode = number("mode")?;
        let data = number("data")?;
        let filter = number("filter")?;
        if mode > 255 || data > 255 || filter > 255 {
            return Err(AppError::invalid(
                "scan clipboard decoded byte field exceeds 255",
            ));
        }
        rows.push(lib::ScanEdge {
            valid: r["valid"]
                .as_bool()
                .ok_or_else(|| AppError::invalid("scan clipboard requires valid boolean"))?,
            low_hz: number("low_hz")?,
            high_hz: number("high_hz")?,
            mode,
            data,
            filter,
            name,
        });
    }
    let count = value["transfer_count"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .filter(|&n| n > 0 && n <= rows.len())
        .ok_or_else(|| AppError::invalid("invalid scan clipboard transfer_count"))?;
    Ok(lib::ScanClipboard {
        rows,
        transfer_count: count,
    })
}

fn user(args: UserArgs) -> Result<u8> {
    let UserArgs { read, action } = args;
    inputs(
        &read,
        match &action {
            UserAction::Edit(e) => Some(e.range_context.as_str()),
            _ => None,
        },
    )?;
    let bytes = io::read(&read.input)?;
    let mut image = load(&bytes, read.kind)?;
    match action {
        UserAction::Show(v) => render(
            &view(&image, Sheet::UserEdge, v.start, v.count)?,
            read.format,
        )?,
        UserAction::Edit(edit) => {
            lib::UserEdge::read(image.memory(), edit.index)?;
            let native = native_context(&edit.range_context)?;
            let before = image.memory().to_vec();
            let mut effects = Vec::new();
            for text in &edit.edits {
                let (field, value) = assignment(text)?;
                if !matches!(field, "low-hz" | "high-hz") {
                    return Err(AppError::invalid(
                        "user edge only has low-hz and high-hz fields",
                    ));
                }
                let candidate = values::parse::<u32>(value)?;
                let mut input = native.ranges.frequency_input(0x15, candidate)?;
                input.first_accepted = !edit.cancel;
                input.second_accepted = !edit.cancel;
                let result = lib::edit_user_frequency(
                    &mut image,
                    edit.index,
                    if field == "low-hz" {
                        lib::EdgeSide::Low
                    } else {
                        lib::EdgeSide::High
                    },
                    input,
                    |f| native.ranges.f32c(&native.live_q, f),
                    |f| native.ranges.ed70(f),
                );
                effects.push(effect_json(&checked(result, &image, &before)?));
            }
            return publish(
                &image,
                read.kind,
                &edit.write,
                json!({"effects":effects,"frequency_context":native_report(&native)}),
                edit.cancel,
            );
        }
    }
    Ok(0)
}

fn preset(args: PresetArgs) -> Result<u8> {
    let PresetArgs { read, action } = args;
    let extra = match &action {
        PresetAction::Edit(e) => e.seed.as_deref(),
        PresetAction::Toggle { seed, .. } | PresetAction::Name { seed, .. } => seed.as_deref(),
        PresetAction::Reset { seed, .. } => Some(seed.as_str()),
        _ => None,
    };
    inputs(&read, extra)?;
    if let PresetAction::Reset { yes, .. } = &action {
        confirm(*yes)?;
    }
    let bytes = io::read(&read.input)?;
    let mut image = load(&bytes, read.kind)?;
    let before = image.memory().to_vec();
    match action {
        PresetAction::Show(v) => {
            render(&view(&image, Sheet::Preset, v.start, v.count)?, read.format)?
        }
        PresetAction::Edit(edit) => {
            let g = seed(edit.seed.as_deref())?;
            let mut effects = Vec::new();
            let mut approved = false;
            for text in &edit.edits {
                let (field, value) = assignment(text)?;
                let field = PresetField::from_str(field, false)
                    .map_err(|_| AppError::invalid("unknown preset field; see special fields"))?;
                let (key, side) = field.key_side();
                if matches!(key, 0x389 | 0x38d | 0x38f | 0x390 | 0x39a) {
                    let ((e, editor, accepted), typed) = fields::with_number_input(
                        value,
                        |input, point| {
                            let input = if edit.cancel {
                                icf::settings::extended::NumberInput::Cancel
                            } else {
                                input
                            };
                            if key == 0x389 {
                                let result = lib::edit_preset_bandwidth(
                                    &mut image, &g, edit.index, input, point, edit.index,
                                );
                                let out = checked(result, &image, &before)?;
                                let accepted = out
                                    .editor
                                    .as_ref()
                                    .is_some_and(|editor| editor.number.accepted);
                                let editor = out.editor.as_ref().map(|editor| json!({"number":crate::editors::number_json(&editor.number),"popup_requested":editor.popup_requested,"rewritten_text":editor.rewritten_text.as_deref().map(io::bytes_value)}));
                                Ok((out.effects, json!(editor), accepted))
                            } else {
                                let result = lib::edit_preset_number(
                                    &mut image, &g, edit.index, key, input, point, edit.index,
                                );
                                let out = checked(result, &image, &before)?;
                                let accepted = out.editor.accepted;
                                Ok((
                                    out.effects,
                                    crate::editors::number_json(&out.editor),
                                    accepted,
                                ))
                            }
                        },
                    )?;
                    io::report(
                        &json!({"key":key,"editor":editor,"effects":effect_json(&e),"approved":accepted}),
                    )?;
                    if e.blocked || (typed && !accepted && !edit.cancel) {
                        return Err(AppError::invalid("preset editor did not accept the input"));
                    }
                    approved |= accepted;
                    effects.push(effect_json(&e));
                    continue;
                }
                let value = field.parse_choice(value, &image, edit.index)?;
                let result = lib::commit_preset(
                    &mut image,
                    &g,
                    edit.index,
                    key,
                    side,
                    (!edit.cancel).then_some(value),
                    edit.index,
                );
                let e = checked(result, &image, &before)?;
                if e.blocked {
                    io::report(&effect_json(&e))?;
                    return Err(AppError::invalid(
                        "preset field is blocked by its dependencies",
                    ));
                }
                approved |= !edit.cancel && e.writes != 0;
                effects.push(effect_json(&e));
            }
            return publish(
                &image,
                read.kind,
                &edit.write,
                json!({"effects":effects,"approved":approved}),
                !approved,
            );
        }
        PresetAction::Toggle {
            index,
            field,
            seed: source,
            write,
        } => {
            let g = seed(source.as_deref())?;
            let result = lib::toggle_preset(
                &mut image,
                &g,
                index,
                field.descriptor().field,
                true,
                13,
                index,
            );
            let e = checked(result, &image, &before)?;
            if e.blocked {
                io::report(&effect_json(&e))?;
                return Err(AppError::invalid(
                    "preset checkbox is blocked by its dependencies",
                ));
            }
            return publish(&image, read.kind, &write, effect_json(&e), false);
        }
        PresetAction::Name {
            index,
            name: text,
            seed: source,
            cancel,
            write,
        } => {
            let g = seed(source.as_deref())?;
            let bytes = name(&text)?;
            let mut events = Vec::new();
            let result = lib::rename_preset(
                &mut image,
                &g,
                index,
                index,
                (!cancel).then_some(bytes.as_slice()),
                |n| Ok(n.to_vec()),
                |n| {
                    events.push(event_json(n));
                    Ok(())
                },
            );
            if result.is_err() {
                io::report(
                    &json!({"notifications":events,"published":false,"delivery":"terminal-report-only"}),
                )?;
            }
            let e = checked(result, &image, &before)?;
            return publish(
                &image,
                read.kind,
                &write,
                json!({"effects":effect_json(&e),"notifications":events,"label_context":"terminal ASCII name; no native tree"}),
                cancel,
            );
        }
        PresetAction::Reset {
            index,
            seed: source,
            write,
            ..
        } => {
            let g = seed(Some(&source))?;
            let result = lib::reset_preset(&mut image, &g, index, 1);
            let e = checked(result, &image, &before)?;
            return publish(&image, read.kind, &write, effect_json(&e), false);
        }
    }
    Ok(0)
}

pub fn run(args: Args) -> Result<u8> {
    match args.command {
        Command::Bandstack(a) => compact(a, lib::CompactKind::BandStack),
        Command::Memopad(a) => compact(a, lib::CompactKind::Memopad),
        Command::Scan(a) => scan(a),
        Command::UserEdge(a) => user(a),
        Command::Preset(a) => preset(a),
        Command::Fields => {
            render(&json!(PresetField::value_variants().iter().map(|f| {
            let d=f.descriptor();json!({"field":f.to_possible_value().expect("field spelling").get_name(),"key":d.key,"mask_index":d.mask,"field_id":d.field,"side":f.key_side().1,"choices":d.choices})
        }).collect::<Vec<_>>()),Format::Json)?;
            Ok(0)
        }
    }
}
