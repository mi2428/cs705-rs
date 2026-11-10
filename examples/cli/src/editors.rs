//! Offline native editor adapters; the library owns grammar, storage and effects.
//! Shared CLI I/O owns stdin and atomic publication. No GUI or device is opened.

mod digital;
mod gps;

use crate::{
    AppError, Result,
    args::{Kind, Read, Write},
    confirm, fields, io, native, render,
    values::{self, ValueCodec},
};
use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use cs705::icf::{Icf, settings::extended as lib};
use serde_json::{Value, json};

#[derive(ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Native digital selections and text fields, separate from GPS settings.
    Digital(digital::Args),
    /// English Auto Repeater row low/high edges or DUP, not the on/off setting.
    AutoRepeater {
        #[command(flatten)]
        read: Read,
        /// Literal native row 0..11, unchanged by resource Only_* visibility.
        row: usize,
        #[arg(value_enum)]
        field: AutoRepeaterField,
        #[command(flatten)]
        value: InputValue,
        /// Actual generated B and live Q; required for low/high only, never G.
        #[arg(long)]
        range_context: Option<String>,
        /// Fresh native invocation code 0; nonzero retains the parent request.
        #[arg(long, default_value_t = 0)]
        native_code: u32,
        #[arg(long, default_value_t = 0)]
        native_argument: u32,
        #[arg(long, default_value_t = 0)]
        warning_bits: u8,
        #[command(flatten)]
        publication: Publication,
    },
    /// Native GPS common settings, separate from GPS memory records.
    Gps(gps::Args),
    /// Native fixed text/keyer editor, with explicit ANSI native output support.
    Text(TextArgs),
    /// Native remote-microphone/Bluetooth key assignment with live Q11 filter.
    Key {
        #[command(flatten)]
        args: NumberArgs,
        /// microphone_keys or bluetooth_keys (or its feature ID).
        name: String,
        #[arg(long)]
        range_context: String,
    },
    /// Original J/U/R selector branch, not a country or UI-language guess.
    Conditional {
        #[command(flatten)]
        read: Read,
        #[arg(value_enum)]
        field: Conditional,
        #[arg(long)]
        range_context: String,
        /// Actual VA 8645FC DWORD; needed only for coordinates/temperature.
        #[arg(long)]
        language_descriptor: Option<u32>,
        #[arg(long, allow_hyphen_values = true)]
        value: String,
        #[command(flatten)]
        publication: Publication,
    },
    /// PAMP/ATT native cases; gray PAMP and fixed ATT rows remain distinct.
    Rf {
        #[command(flatten)]
        args: NumberArgs,
        #[arg(value_enum)]
        field: Amplifier,
        /// Optional resource-generation Q; body edits do not invent a layout Q.
        #[arg(long)]
        resource_context: Option<String>,
    },
    /// DR filter/SQL/code setting and real kind 1B dependent record save/reload.
    Dr {
        #[command(flatten)]
        read: Read,
        #[arg(value_enum)]
        field: Dr,
        #[arg(long, allow_hyphen_values = true)]
        value: String,
        #[arg(long, default_value_t = 0)]
        node_flags: u32,
        #[command(flatten)]
        publication: Publication,
    },
    /// Native Home MR CH with previous-split odd gate and sticky warnings.
    HomeChannel {
        #[command(flatten)]
        read: Read,
        #[arg(long)]
        current_working: String,
        #[arg(long)]
        range_context: String,
        /// Actual VA 864614 DWORD; no implicit show-both preference.
        #[arg(long)]
        show_both: u32,
        #[arg(long, default_value_t = 0)]
        warning_bits: u32,
        #[arg(long, allow_hyphen_values = true)]
        value: String,
        #[command(flatten)]
        publication: Publication,
    },
    HomeDrGroup(HomeDrArgs),
    HomeDrMemory {
        #[command(flatten)]
        args: HomeDrArgs,
        #[arg(
            long,
            required_unless_present = "current_source",
            conflicts_with = "current_source"
        )]
        current_working: Option<String>,
        /// Explicitly use the supplied I generation for the global-current role.
        #[arg(long)]
        current_source: bool,
        #[arg(long, default_value_t = 0)]
        warning_bits: u32,
    },
    /// Native AA658 program scan selection and conditional source-G reset.
    ProgramScan {
        #[command(flatten)]
        read: Read,
        #[arg(
            long,
            required_unless_present = "current_source",
            conflicts_with = "current_source"
        )]
        current_working: Option<String>,
        /// Explicitly use the edited I image for the global-current scan role.
        #[arg(long)]
        current_source: bool,
        /// Actual raw G generation; used only if the original reset guard fires.
        #[arg(long)]
        defaults: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        value: String,
        #[command(flatten)]
        publication: Publication,
    },
    /// Native BW operation in Hz, including original PBT reset and typed grid.
    Bw(NumberArgs),
    /// Native PBT operation; corrupt old raw can clip even on display.
    Pbt {
        #[command(flatten)]
        args: NumberArgs,
        #[arg(value_enum)]
        side: PbtSide,
    },
    /// Native FIX low/high operation for one of the 68 physical rows.
    Fix {
        #[command(flatten)]
        args: NumberArgs,
        #[arg(value_enum)]
        side: EdgeSide,
        #[arg(long)]
        range_context: String,
    },
    /// Native Home VFO same-source two-pass frequency editor.
    HomeVfo {
        #[command(flatten)]
        read: Read,
        #[arg(long, allow_hyphen_values = true)]
        value: String,
        #[arg(long)]
        range_context: String,
        /// A fresh invocation starts at 0; continuing callers supply prior bits.
        #[arg(long, default_value_t = 0)]
        warning_bits: u32,
        #[command(flatten)]
        publication: Publication,
    },
    /// Home MEMO group selection with the real expanded normal map/counts.
    HomeGroup {
        #[command(flatten)]
        read: Read,
        #[arg(long)]
        normal_working: String,
        #[arg(
            long,
            required_unless_present = "initial_catalog",
            conflicts_with = "initial_catalog"
        )]
        catalog: Option<String>,
        /// Explicit normal PE initial catalog: empty 255, call 100.
        #[arg(long)]
        initial_catalog: bool,
        #[arg(long, allow_hyphen_values = true)]
        value: String,
        #[command(flatten)]
        publication: Publication,
    },
    /// Read-only attachment metadata and its separate explicit clear command.
    Metadata(MetadataArgs),
}

#[derive(ValueEnum, Clone, Copy)]
enum Conditional {
    Coordinates,
    Temperature,
    AutoRepeater,
    ScopeSpan,
}
impl Conditional {
    fn field(self) -> lib::ConditionalField {
        match self {
            Self::Coordinates => lib::ConditionalField::Coordinates,
            Self::Temperature => lib::ConditionalField::Temperature,
            Self::AutoRepeater => lib::ConditionalField::AutoRepeater,
            Self::ScopeSpan => lib::ConditionalField::ScopeSpan,
        }
    }
}
#[derive(ValueEnum, Clone, Copy)]
enum Amplifier {
    Preamp,
    Attenuator,
}
impl Amplifier {
    fn field(self) -> lib::AmplifierField {
        match self {
            Self::Preamp => lib::AmplifierField::Preamp,
            Self::Attenuator => lib::AmplifierField::Attenuator,
        }
    }
}
#[derive(ValueEnum, Clone, Copy)]
enum Dr {
    DvFilter,
    FmFilter,
    DigitalSquelch,
    DigitalCode,
}
impl Dr {
    fn field(self) -> lib::DrField {
        match self {
            Self::DvFilter => lib::DrField::DvFilter,
            Self::FmFilter => lib::DrField::FmFilter,
            Self::DigitalSquelch => lib::DrField::DigitalSquelch,
            Self::DigitalCode => lib::DrField::DigitalCode,
        }
    }
}
#[derive(ClapArgs)]
struct HomeDrArgs {
    #[command(flatten)]
    read: Read,
    #[arg(long)]
    source_working: String,
    #[arg(long)]
    range_context: String,
    #[arg(long, allow_hyphen_values = true)]
    value: String,
    #[command(flatten)]
    publication: Publication,
}

#[derive(ClapArgs)]
struct InputValue {
    #[arg(
        long,
        allow_hyphen_values = true,
        required_unless_present = "value_file",
        conflicts_with = "value_file"
    )]
    value: Option<String>,
    /// Text/action JSON file, or - for the single owned stdin stream.
    #[arg(long)]
    value_file: Option<String>,
}
impl InputValue {
    fn read(&self) -> Result<String> {
        if let Some(text) = &self.value {
            Ok(text.clone())
        } else {
            String::from_utf8(io::read(self.value_file.as_deref().ok_or_else(||AppError::invalid("editor value is required"))?)?)
                .map_err(|_|AppError::invalid("editor action requires UTF-8 JSON or ASCII text; use explicit hex for ANSI bytes"))
        }
    }
}
#[derive(ValueEnum, Clone, Copy)]
enum TextField {
    GatewayAddress,
    GatewayCallsign,
    NtpAddress,
    Comment,
    PictureRecipient,
    NetworkName,
    Ssid,
    ApPassword,
    NetworkRadioName,
    Keyer,
    User1Id,
    User1Password,
    User2Id,
    User2Password,
}
impl TextField {
    fn field(self) -> lib::FixedTextField {
        match self {
            Self::GatewayAddress => lib::FixedTextField::GatewayAddress,
            Self::GatewayCallsign => lib::FixedTextField::GatewayCallsign,
            Self::NtpAddress => lib::FixedTextField::NtpAddress,
            Self::Comment => lib::FixedTextField::Comment,
            Self::PictureRecipient => lib::FixedTextField::PictureRecipient,
            Self::NetworkName => lib::FixedTextField::NetworkName,
            Self::Ssid => lib::FixedTextField::Ssid,
            Self::ApPassword => lib::FixedTextField::ApPassword,
            Self::NetworkRadioName => lib::FixedTextField::NetworkRadioName,
            Self::Keyer => lib::FixedTextField::Keyer,
            Self::User1Id => lib::FixedTextField::User1Id,
            Self::User1Password => lib::FixedTextField::User1Password,
            Self::User2Id => lib::FixedTextField::User2Id,
            Self::User2Password => lib::FixedTextField::User2Password,
        }
    }
}
#[derive(ClapArgs)]
struct TextArgs {
    #[command(flatten)]
    read: Read,
    #[arg(value_enum)]
    field: TextField,
    /// Native row: Keyer 0..7, other fixed fields 0 only.
    index: usize,
    #[command(flatten)]
    value: InputValue,
    /// Actual VA 864634 DWORD; required only for phase 1 callsign/recipient text.
    #[arg(long)]
    alignment: Option<u32>,
    /// Captured LCMapStringA 2n and optional CharUpperBuffA n outputs/return values.
    #[arg(long)]
    nls_context: Option<String>,
    #[command(flatten)]
    publication: Publication,
}

enum TextAction {
    Text(Vec<u8>, u32),
    Display,
    Cancel,
    Begin,
}
fn text_action(text: &str) -> Result<TextAction> {
    if !values::json_view(text).starts_with('{') {
        let text = if values::json_view(text).starts_with('"') {
            values::parse::<String>(text)?
        } else {
            text.to_owned()
        };
        if !text.is_ascii() {
            return Err(AppError::invalid(
                "native text requires ASCII or explicit ANSI hex; no Unicode conversion",
            ));
        }
        return Ok(TextAction::Text(text.into_bytes(), 1));
    }
    let value: Value = serde_json::from_str(text)
        .map_err(|_| AppError::invalid("invalid fixed text action JSON"))?;
    let object = value
        .as_object()
        .ok_or_else(|| AppError::invalid("fixed text action requires an object"))?;
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid("fixed text requires action"))?;
    match action {
        "display" | "cancel" | "begin" if object.len() == 1 => Ok(match action {
            "display" => TextAction::Display,
            "cancel" => TextAction::Cancel,
            _ => TextAction::Begin,
        }),
        "text" => {
            if object
                .keys()
                .any(|key| !matches!(key.as_str(), "action" | "text" | "hex" | "phase"))
                || object.contains_key("text") == object.contains_key("hex")
            {
                return Err(AppError::invalid(
                    "fixed text requires exactly one text/hex and optional phase",
                ));
            }
            let phase = object
                .get("phase")
                .map(u32::parse)
                .transpose()?
                .unwrap_or(1);
            if !matches!(phase, 1 | 2) {
                return Err(AppError::invalid("fixed text requires phase1 or2"));
            }
            let bytes = if let Some(value) = object.get("text") {
                let text = String::parse(value)?;
                if !text.is_ascii() {
                    return Err(AppError::invalid("use explicit ANSI hex, not Unicode text"));
                }
                text.into_bytes()
            } else {
                io::decode_hex(
                    object["hex"]
                        .as_str()
                        .ok_or_else(|| AppError::invalid("fixed text hex requires a string"))?,
                )?
            };
            Ok(TextAction::Text(bytes, phase))
        }
        _ => Err(AppError::invalid(
            "unsupported fixed text action or members",
        )),
    }
}

struct NlsOutput<T> {
    result: T,
    bytes: Vec<u8>,
}
struct NlsOutputs {
    map: NlsOutput<i32>,
    upper: Option<NlsOutput<u32>>,
}
fn nls_output<T: ValueCodec>(value: &Value, width: usize) -> Result<NlsOutput<T>> {
    let object = value
        .as_object()
        .filter(|o| o.len() == 2 && o.contains_key("result") && o.contains_key("output_hex"))
        .ok_or_else(|| AppError::invalid("NLS output requires result/output_hex only"))?;
    let bytes = io::decode_hex(
        object["output_hex"]
            .as_str()
            .ok_or_else(|| AppError::invalid("NLS output_hex requires a string"))?,
    )?;
    if bytes.len() != width {
        return Err(AppError::invalid(
            "NLS output width does not match the actual helper buffer",
        ));
    }
    Ok(NlsOutput {
        result: T::parse(&object["result"])?,
        bytes,
    })
}
fn nls_outputs(path: &str, n: usize) -> Result<NlsOutputs> {
    let value: Value = serde_json::from_slice(&io::read(path)?)
        .map_err(|_| AppError::invalid("invalid NLS context JSON"))?;
    let object = value
        .as_object()
        .filter(|o| {
            o.contains_key("lcmap")
                && o.keys()
                    .all(|key| matches!(key.as_str(), "lcmap" | "uppercase"))
        })
        .ok_or_else(|| {
            AppError::invalid("NLS context requires lcmap and optional uppercase outputs only")
        })?;
    let twice = n
        .checked_mul(2)
        .ok_or_else(|| AppError::invalid("NLS destination length overflow"))?;
    Ok(NlsOutputs {
        map: nls_output(&object["lcmap"], twice)?,
        upper: object
            .get("uppercase")
            .map(|value| nls_output(value, n))
            .transpose()?,
    })
}

fn text(args: TextArgs) -> Result<u8> {
    io::single_stdin(
        std::iter::once(args.read.input.as_str())
            .chain(args.value.value_file.as_deref())
            .chain(args.nls_context.as_deref()),
    )?;
    if args.publication.accept_partial {
        confirm(args.publication.yes)?;
    }
    let action = text_action(&args.value.read()?)?;
    let field = args.field.field();
    let alignment = match action {
        TextAction::Text(_, 1)
            if matches!(
                field,
                lib::FixedTextField::GatewayCallsign | lib::FixedTextField::PictureRecipient
            ) =>
        {
            args.alignment.ok_or_else(|| {
                AppError::invalid("phase1 callsign/recipient requires the actual --alignment DWORD")
            })?
        }
        _ => args.alignment.unwrap_or(0),
    };
    if args.nls_context.is_some() && !matches!(action, TextAction::Text(..)) {
        return Err(AppError::invalid(
            "NLS context is only read by a text invocation",
        ));
    }
    let outputs =
        if let (Some(path), TextAction::Text(source, _)) = (args.nls_context.as_deref(), &action) {
            Some(nls_outputs(path, source.len())?)
        } else {
            None
        };
    let bytes = io::read(&args.read.input)?;
    let mut image = load(&bytes, args.read.kind)?;
    let before = image.memory().to_vec();
    let typed = matches!(action, TextAction::Text(..));
    let mut source = Vec::new();
    let input = match action {
        TextAction::Text(value, phase) => {
            source = value;
            lib::FixedTextInput::Text {
                source: &mut source,
                phase,
            }
        }
        TextAction::Display => lib::FixedTextInput::Display,
        TextAction::Cancel => lib::FixedTextInput::Cancel,
        TextAction::Begin => lib::FixedTextInput::Begin,
    };
    let result = if let Some(outputs) = outputs {
        let stage = std::cell::Cell::new(0);
        lib::edit_fixed_text_with_nls(
            &mut image,
            field,
            args.index,
            input,
            alignment,
            |_, destination| {
                if stage.get() != 0 || destination.len() != outputs.map.bytes.len() {
                    return Err(cs705::csv::Error::Invalid(
                        "LCMap output stage/width mismatch",
                    ));
                }
                destination.copy_from_slice(&outputs.map.bytes);
                stage.set(1);
                Ok(outputs.map.result)
            },
            |destination| {
                let upper = outputs
                    .upper
                    .as_ref()
                    .ok_or(cs705::csv::Error::SourceUnavailable(
                        "explicit CharUpperBuffA output required",
                    ))?;
                if stage.get() != 1 || destination.len() != upper.bytes.len() {
                    return Err(cs705::csv::Error::Invalid(
                        "uppercase output stage/width mismatch",
                    ));
                }
                destination.copy_from_slice(&upper.bytes);
                stage.set(2);
                Ok(upper.result)
            },
        )
    } else {
        lib::edit_fixed_text(&mut image, field, args.index, input, alignment)
    };
    if !source.is_empty() {
        let visible = if matches!(
            field,
            lib::FixedTextField::ApPassword
                | lib::FixedTextField::User1Password
                | lib::FixedTextField::User2Password
        ) {
            json!({"redacted":true})
        } else {
            io::bytes_value(&source)
        };
        io::report(&json!({"shared_source_after":visible}))?;
    }
    let result=result.map_err(|error|AppError::invalid(error.to_string())).and_then(|effect|{
        let report=json!({"stored":effect.text.stored,"state_out":effect.text.state_out,"notify_flags":effect.text.notify_flags,"display":io::bytes_value(&effect.text.display),
            "context60":effect.context60,"warning_bits":effect.warning_bits,"marked":effect.marked});
        if typed&&!effect.text.stored {io::report(&json!({"editor":report}))?;return Err(AppError::invalid("native fixed text did not accept the text"));}
        Ok((report,typed && effect.text.stored))
    });
    finish(&image, &args.read, &args.publication, &before, result)
}

#[derive(ValueEnum, Clone, Copy)]
enum PbtSide {
    One,
    Two,
}
#[derive(ValueEnum, Clone, Copy)]
enum EdgeSide {
    Low,
    High,
}

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
enum AutoRepeaterField {
    Low,
    High,
    Dup,
}

fn auto_repeater_json(effect: &lib::AutoRepeaterEffect) -> Value {
    json!({"stored":effect.stored,"helper_accepted":effect.helper_accepted,"candidate":effect.candidate,
        "pair_hz":effect.pair_hz,"dup":effect.dup,"valid":effect.valid,"state_out":effect.state_out,
        "notify_flags":effect.notify_flags,"warning_bits":effect.warning_bits,"gray":effect.gray,
        "popup_requested":effect.popup_requested,"blocked":effect.blocked,"move_to_low":effect.move_to_low,
        "redispatch_code":effect.redispatch_code,"parent_requests_executed":false})
}

#[derive(ClapArgs)]
struct Publication {
    #[command(flatten)]
    write: Write,
    /// Publish a real changed prefix after an operation error, returning exit 5.
    #[arg(long, requires = "yes")]
    accept_partial: bool,
    #[arg(long, requires = "accept_partial")]
    yes: bool,
}

#[derive(ClapArgs)]
struct NumberArgs {
    #[command(flatten)]
    read: Read,
    index: usize,
    /// Shared numeric editor text/action JSON; negative input is not re-parsed.
    #[arg(long, allow_hyphen_values = true)]
    value: String,
    #[command(flatten)]
    publication: Publication,
}

#[derive(ValueEnum, Clone, Copy)]
enum Metadata {
    Picture,
    ConnectionIpList,
}
impl Metadata {
    fn field(self) -> lib::MetadataField {
        match self {
            Self::Picture => lib::MetadataField::TransmitPicture,
            Self::ConnectionIpList => lib::MetadataField::ConnectionIpList,
        }
    }
}
#[derive(ClapArgs)]
struct MetadataArgs {
    #[command(flatten)]
    read: Read,
    #[arg(value_enum)]
    field: Metadata,
    #[command(subcommand)]
    action: MetadataAction,
}
#[derive(Subcommand)]
enum MetadataAction {
    Show,
    Clear {
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        write: Write,
    },
}

fn load(bytes: &[u8], kind: Kind) -> Result<Icf<'_>> {
    match kind {
        Kind::Icf => Ok(Icf::parse(bytes)?),
        Kind::Memory => Ok(Icf::from_memory(bytes.to_vec())?),
        _ => Err(AppError::invalid("editor requires --kind icf or memory")),
    }
}
pub(crate) fn number_json(effect: &lib::NumberEffect) -> Value {
    json!({"stored":effect.stored,"accepted":effect.accepted,"candidate":effect.candidate,
        "raw_candidate":effect.raw_candidate,"state_out":effect.state_out,"notify_flags":effect.notify_flags,
        "suffix":io::bytes_value(&effect.suffix),"gray":effect.gray})
}
fn choice_json(effect: &lib::ChoiceEffect) -> Value {
    json!({"stored":effect.stored,"value":effect.value,"notify_flags":effect.notify_flags,"gray":effect.gray,"popup_requested":effect.popup_requested,"fixed_label":effect.fixed_value.map(|value| value.source_label())})
}
fn dr_home_json(effect: &lib::HomeDrEffect) -> Value {
    json!({"accepted":effect.accepted,"stored":effect.stored,"candidate":effect.candidate,"reference":effect.reference,"notify_flags":effect.notify_flags,"warning_bits":effect.warning_bits,"state_out":effect.state_out,"popup_requested":effect.popup_requested})
}
fn working(path: &str) -> Result<Vec<u8>> {
    let bytes = io::read(path)?;
    if bytes.len() != cs705::clone::IMAGE_CAPACITY {
        return Err(AppError::invalid(
            "working source requires exactly0x4997a actual bytes",
        ));
    }
    Ok(bytes)
}
fn number_action(
    text: &str,
    edit: impl FnOnce(lib::NumberInput<'_>, u8) -> Result<(Value, bool)>,
) -> Result<(Value, bool)> {
    // Display repairs are observable in memory, not approval to publish them.
    let ((value, accepted), typed) = fields::with_number_input(text, |input, point| {
        let editable = match input {
            lib::NumberInput::Text(_) | lib::NumberInput::Next | lib::NumberInput::Previous => true,
            lib::NumberInput::Selected(event) => event.accepted && event.phase == 2,
            _ => false,
        };
        let (value, accepted) = edit(input, point)?;
        Ok((value, accepted && editable))
    })?;
    if typed && !accepted {
        io::report(&json!({"editor":value,"typed":true,"accepted":false}))?;
        return Err(AppError::invalid("native editor did not accept the text"));
    }
    Ok((value, accepted))
}
fn publish(image: &Icf<'_>, kind: Kind, write: &Write) -> Result<()> {
    match kind {
        Kind::Icf => io::publish(&image.to_bytes(), write),
        Kind::Memory => io::publish(image.memory(), write),
        _ => unreachable!("validated editor kind"),
    }
}
fn finish(
    image: &Icf<'_>,
    read: &Read,
    publication: &Publication,
    before: &[u8],
    result: Result<(Value, bool)>,
) -> Result<u8> {
    match result {
        Ok((effect, approved)) => {
            io::report(
                &json!({"editor":effect,"approved":approved,"partial_in_memory":image.memory()!=before,"native_gui":false,"device_io":false}),
            )?;
            if !approved {
                io::report(&json!({"published":false}))?;
                return Ok(0);
            }
            publish(image, read.kind, &publication.write)?;
            Ok(0)
        }
        Err(error) => {
            let changed = image.memory() != before;
            io::report(
                &json!({"operation_error":error.message,"partial_in_memory":changed,"published":false}),
            )?;
            if publication.accept_partial && changed {
                confirm(publication.yes)?;
                publish(image, read.kind, &publication.write)?;
                io::report(&json!({"accepted_partial":true,"exit_code":5}))?;
                Ok(5)
            } else {
                Err(error)
            }
        }
    }
}

pub fn run(args: Args) -> Result<u8> {
    match args.command {
        Command::Digital(args) => digital::run(args),
        Command::AutoRepeater {
            read,
            row,
            field,
            value,
            range_context,
            native_code,
            native_argument,
            warning_bits,
            publication,
        } => {
            io::single_stdin(
                std::iter::once(read.input.as_str())
                    .chain(value.value_file.as_deref())
                    .chain(range_context.as_deref()),
            )?;
            if publication.accept_partial {
                confirm(publication.yes)?;
            }
            let value = value.read()?;
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let invocation = lib::AutoRepeaterInvocation {
                code: native_code,
                argument: native_argument,
                warning_bits,
            };
            let result = (|| {
                if field == AutoRepeaterField::Dup {
                    if range_context.is_some() {
                        return Err(AppError::invalid(
                            "Auto Repeater DUP does not consume range context",
                        ));
                    }
                    let input = fields::choice_input(&value)?;
                    let effect =
                        lib::edit_auto_repeater_dup(&mut image, row, input, 1, invocation)?;
                    Ok((
                        json!({"feature_id":"A2-O077","row":row,"key":0x37c,"effect":auto_repeater_json(&effect),"resource_visibility_evaluated":false}),
                        effect.stored,
                    ))
                } else {
                    let context = native::read(range_context.as_deref().ok_or_else(|| {
                        AppError::invalid("Auto Repeater low/high requires --range-context")
                    })?)?;
                    let side = if field == AutoRepeaterField::Low {
                        lib::EdgeSide::Low
                    } else {
                        lib::EdgeSide::High
                    };
                    number_action(&value, |input, point| {
                        let runtime = lib::AutoRepeaterRuntime {
                            frequency: lib::FrequencyRuntime {
                                ranges: &context.ranges,
                                live_q: &context.live_q,
                                binding: 1,
                                crt_point: point,
                            },
                            system_langid: context.system_langid,
                        };
                        let effect = lib::edit_auto_repeater_edge(
                            &mut image, runtime, row, side, input, invocation,
                        )?;
                        let resource_q = cs705::icf::context::Context::from_bytes(
                            *context.ranges.source_q().ok_or_else(|| {
                                AppError::invalid("Auto Repeater resource generation Q unavailable")
                            })?,
                        );
                        Ok((
                            json!({"feature_id":"A2-O076","row":row,"key":if field==AutoRepeaterField::Low {0x37a} else {0x37b},"effect":auto_repeater_json(&effect),
                            "context":native::report(&context),"resource_visible":lib::auto_repeater_row_visible(&resource_q,row)?}),
                            effect.helper_accepted,
                        ))
                    })
                }
            })();
            finish(&image, &read, &publication, &before, result)
        }
        Command::Gps(args) => gps::run(args),
        Command::Text(args) => text(args),
        Command::Key {
            args,
            name,
            range_context,
        } => {
            io::single_stdin([args.read.input.as_str(), range_context.as_str()])?;
            let input = fields::choice_input(&args.value)?;
            if args.publication.accept_partial {
                confirm(args.publication.yes)?;
            }
            let bytes = io::read(&args.read.input)?;
            let mut image = load(&bytes, args.read.kind)?;
            let before = image.memory().to_vec();
            let (live_q, system_langid) = native::read_live(&range_context)?;
            let result = (|| {
                let choices = lib::key_assignment_choices(&image, &name, args.index, &live_q)?;
                let effect =
                    lib::edit_key_assignment(&mut image, &name, args.index, input, &live_q)?;
                Ok((
                    json!({"choice":choice_json(&effect),"choices":choices,"context":{"live_q":io::bytes_value(live_q.as_bytes()),"system_langid":system_langid,"automatic_refresh":false}}),
                    effect.stored,
                ))
            })();
            finish(&image, &args.read, &args.publication, &before, result)
        }
        Command::Conditional {
            read,
            field,
            range_context,
            language_descriptor,
            value,
            publication,
        } => {
            io::single_stdin([read.input.as_str(), range_context.as_str()])?;
            let input = fields::choice_input(&value)?;
            if publication.accept_partial {
                confirm(publication.yes)?;
            }
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let (live_q, system_langid) = native::read_live(&range_context)?;
            let field = field.field();
            let result = (|| {
                let catalog =
                    field.catalog_sources(language_descriptor, system_langid, Some(&live_q))?;
                let effect = catalog.edit(&mut image, input)?;
                Ok((
                    json!({"choice":choice_json(&effect),"selector":catalog.selector,"choices":catalog.values(),"context":{"live_q":io::bytes_value(live_q.as_bytes()),"system_langid":system_langid,"automatic_refresh":false}}),
                    effect.stored,
                ))
            })();
            finish(&image, &read, &publication, &before, result)
        }
        Command::Rf {
            args,
            field,
            resource_context,
        } => {
            io::single_stdin(
                std::iter::once(args.read.input.as_str()).chain(resource_context.as_deref()),
            )?;
            let input = fields::choice_input(&args.value)?;
            if args.publication.accept_partial {
                confirm(args.publication.yes)?;
            }
            let bytes = io::read(&args.read.input)?;
            let mut image = load(&bytes, args.read.kind)?;
            let before = image.memory().to_vec();
            let resource_q = resource_context
                .as_deref()
                .map(native::read_generation)
                .transpose()?;
            let result = (|| {
                let visible = if let Some(resource_q) = &resource_q {
                    Some(lib::rf_row_visible(
                        resource_q,
                        lib::RfLayout::PreampAttenuator,
                        args.index,
                    )?)
                } else {
                    None
                };
                let effect = field.field().edit(&mut image, args.index, input)?;
                Ok((
                    json!({"choice":choice_json(&effect),"resource_visible":visible}),
                    effect.stored,
                ))
            })();
            finish(&image, &args.read, &args.publication, &before, result)
        }
        Command::Dr {
            read,
            field,
            value,
            mut node_flags,
            publication,
        } => {
            io::single_stdin([read.input.as_str()])?;
            if publication.accept_partial {
                confirm(publication.yes)?;
            }
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let result = number_action(&value, |input, point| {
                let effect =
                    lib::edit_dr(&mut image, field.field(), input, 1, point, &mut node_flags)?;
                Ok((
                    json!({"stored":effect.stored,"synchronized":effect.synchronized,"candidate":effect.candidate,"state_out":effect.state_out,
                    "gray":effect.gray,"notify_flags":effect.notify_flags,"display":io::bytes_value(&effect.display)}),
                    effect.stored,
                ))
            });
            io::report(&json!({"node_flags_after":node_flags}))?;
            finish(&image, &read, &publication, &before, result)
        }
        Command::HomeChannel {
            read,
            current_working,
            range_context,
            show_both,
            warning_bits,
            value,
            publication,
        } => {
            io::single_stdin([
                read.input.as_str(),
                current_working.as_str(),
                range_context.as_str(),
            ])?;
            if publication.accept_partial {
                confirm(publication.yes)?;
            }
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let current = working(&current_working)?;
            let context = native::read(&range_context)?;
            let result = number_action(&value, |input, point| {
                let runtime = lib::HomeChannelRuntime {
                    frequency: lib::FrequencyRuntime {
                        ranges: &context.ranges,
                        live_q: &context.live_q,
                        binding: 1,
                        crt_point: point,
                    },
                    current: &current,
                    show_both_864614: show_both,
                };
                let choices = if input == lib::NumberInput::Begin {
                    Some(lib::home_channel_choices(&image, runtime, true)?)
                } else {
                    None
                };
                let effect = lib::edit_home_channel(&mut image, runtime, input, warning_bits)?;
                Ok((
                    json!({"stored":effect.stored,"candidate":effect.candidate,"state_out":effect.state_out,"popup_requested":effect.popup_requested,
                    "notify_flags":effect.notify_flags,"warning_bits":effect.warning_bits,"choices":choices,"context":native::report(&context)}),
                    effect.stored,
                ))
            });
            finish(&image, &read, &publication, &before, result)
        }
        Command::HomeDrGroup(args) => {
            io::single_stdin([
                args.read.input.as_str(),
                args.source_working.as_str(),
                args.range_context.as_str(),
            ])?;
            if args.publication.accept_partial {
                confirm(args.publication.yes)?;
            }
            let bytes = io::read(&args.read.input)?;
            let mut image = load(&bytes, args.read.kind)?;
            let before = image.memory().to_vec();
            let source = working(&args.source_working)?;
            let context = native::read(&args.range_context)?;
            let result = number_action(&args.value, |input, point| {
                // The group operation reads only source; the current role is unused.
                let runtime = lib::HomeDrRuntime {
                    frequency: lib::FrequencyRuntime {
                        ranges: &context.ranges,
                        live_q: &context.live_q,
                        binding: 1,
                        crt_point: point,
                    },
                    source: &source,
                    current: &source,
                };
                let choices = if input == lib::NumberInput::Begin {
                    Some(lib::home_dr_group_choices(&image, runtime)?)
                } else {
                    None
                };
                let effect = lib::edit_home_dr_group(&mut image, runtime, input)?;
                Ok((
                    json!({"home":dr_home_json(&effect),"choices":choices,"context":native::report(&context)}),
                    effect.accepted,
                ))
            });
            finish(&image, &args.read, &args.publication, &before, result)
        }
        Command::HomeDrMemory {
            args,
            current_working,
            current_source: _,
            warning_bits,
        } => {
            io::single_stdin(
                [
                    args.read.input.as_str(),
                    args.source_working.as_str(),
                    args.range_context.as_str(),
                ]
                .into_iter()
                .chain(current_working.as_deref()),
            )?;
            if args.publication.accept_partial {
                confirm(args.publication.yes)?;
            }
            let bytes = io::read(&args.read.input)?;
            let mut image = load(&bytes, args.read.kind)?;
            let before = image.memory().to_vec();
            let source = working(&args.source_working)?;
            let current = current_working.as_deref().map(working).transpose()?;
            let context = native::read(&args.range_context)?;
            let result = number_action(&args.value, |input, point| {
                let runtime = lib::HomeDrRuntime {
                    frequency: lib::FrequencyRuntime {
                        ranges: &context.ranges,
                        live_q: &context.live_q,
                        binding: 1,
                        crt_point: point,
                    },
                    source: &source,
                    current: current.as_deref().unwrap_or(&source),
                };
                let choices = if input == lib::NumberInput::Begin {
                    Some(lib::home_dr_memory_choices(&image, runtime)?)
                } else {
                    None
                };
                let mut source_text = Vec::new();
                let action = match input {
                    lib::NumberInput::Display => lib::HomeDrMemoryInput::Display,
                    lib::NumberInput::Cancel => lib::HomeDrMemoryInput::Cancel,
                    lib::NumberInput::Begin => lib::HomeDrMemoryInput::Popup,
                    lib::NumberInput::Next => lib::HomeDrMemoryInput::Next,
                    lib::NumberInput::Previous => lib::HomeDrMemoryInput::Previous,
                    lib::NumberInput::Selected(event) => lib::HomeDrMemoryInput::Selected(event),
                    lib::NumberInput::Text(text) => {
                        source_text = text.to_vec();
                        lib::HomeDrMemoryInput::Text(&mut source_text)
                    }
                };
                let effect = lib::edit_home_dr_memory(&mut image, runtime, action, warning_bits);
                if matches!(input, lib::NumberInput::Text(_)) {
                    io::report(&json!({"shared_source_after":io::bytes_value(&source_text)}))?;
                }
                let effect = effect?;
                Ok((
                    json!({"home":dr_home_json(&effect),"choices":choices,"context":native::report(&context)}),
                    effect.accepted,
                ))
            });
            finish(&image, &args.read, &args.publication, &before, result)
        }
        Command::ProgramScan {
            read,
            current_working,
            current_source: _,
            defaults,
            value,
            publication,
        } => {
            io::single_stdin(
                std::iter::once(read.input.as_str())
                    .chain(current_working.as_deref())
                    .chain(defaults.as_deref()),
            )?;
            if publication.accept_partial {
                confirm(publication.yes)?;
            }
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let current = current_working.as_deref().map(working).transpose()?;
            let g = defaults.as_deref().map(io::read).transpose()?;
            let result = number_action(&value, |input, _| {
                let current = current.as_deref().unwrap_or(&before);
                let choices = if input == lib::NumberInput::Begin {
                    Some(lib::program_scan_choices(&image, current)?)
                } else {
                    None
                };
                let mut source_text = Vec::new();
                let action = match input {
                    lib::NumberInput::Display => lib::ProgramScanInput::Display,
                    lib::NumberInput::Cancel => lib::ProgramScanInput::Cancel,
                    lib::NumberInput::Begin => lib::ProgramScanInput::Popup,
                    lib::NumberInput::Next => lib::ProgramScanInput::Next,
                    lib::NumberInput::Previous => lib::ProgramScanInput::Previous,
                    lib::NumberInput::Selected(event) => lib::ProgramScanInput::Selected(event),
                    lib::NumberInput::Text(text) => {
                        source_text = text.to_vec();
                        lib::ProgramScanInput::Text(&mut source_text)
                    }
                };
                // Absent G is not a generated default: the required late read fails.
                let effect = lib::edit_program_scan(
                    &mut image,
                    current,
                    g.as_deref().unwrap_or(&[]),
                    action,
                );
                if matches!(input, lib::NumberInput::Text(_)) {
                    io::report(&json!({"shared_source_after":io::bytes_value(&source_text)}))?;
                }
                let effect = effect?;
                Ok((
                    json!({"accepted":effect.accepted,"stored":effect.stored,"candidate":effect.candidate,"selected":effect.selected,
                    "notify_flags":effect.notify_flags,"popup_requested":effect.popup_requested,"choices":choices}),
                    effect.accepted,
                ))
            });
            finish(&image, &read, &publication, &before, result)
        }
        Command::Bw(args) => {
            io::single_stdin([args.read.input.as_str()])?;
            if args.publication.accept_partial {
                confirm(args.publication.yes)?;
            }
            let bytes = io::read(&args.read.input)?;
            let mut image = load(&bytes, args.read.kind)?;
            let before = image.memory().to_vec();
            let result = number_action(&args.value, |input, point| {
                let effect = lib::edit_bandwidth(&mut image, args.index, input, point)?;
                Ok((
                    json!({"number":number_json(&effect.number),"popup_requested":effect.popup_requested,
                    "rewritten_text":effect.rewritten_text.as_deref().map(io::bytes_value)}),
                    effect.number.accepted,
                ))
            });
            finish(&image, &args.read, &args.publication, &before, result)
        }
        Command::Pbt { args, side } => {
            io::single_stdin([args.read.input.as_str()])?;
            if args.publication.accept_partial {
                confirm(args.publication.yes)?;
            }
            let bytes = io::read(&args.read.input)?;
            let mut image = load(&bytes, args.read.kind)?;
            let before = image.memory().to_vec();
            let side = match side {
                PbtSide::One => cs705::icf::PbtSide::One,
                PbtSide::Two => cs705::icf::PbtSide::Two,
            };
            let result = number_action(&args.value, |input, point| {
                let effect = lib::edit_pbt(&mut image, args.index, side, input, point)?;
                Ok((
                    number_json(&effect),
                    effect.accepted && !matches!(input, lib::NumberInput::Selected(_)),
                ))
            });
            finish(&image, &args.read, &args.publication, &before, result)
        }
        Command::Fix {
            args,
            side,
            range_context,
        } => {
            io::single_stdin([args.read.input.as_str(), range_context.as_str()])?;
            if args.publication.accept_partial {
                confirm(args.publication.yes)?;
            }
            let bytes = io::read(&args.read.input)?;
            let mut image = load(&bytes, args.read.kind)?;
            let before = image.memory().to_vec();
            let context = native::read(&range_context)?;
            let side = match side {
                EdgeSide::Low => lib::EdgeSide::Low,
                EdgeSide::High => lib::EdgeSide::High,
            };
            let result = number_action(&args.value, |input, point| {
                // Binding 1 denotes this CLI-owned input object, never a host pointer.
                let runtime = lib::FrequencyRuntime {
                    ranges: &context.ranges,
                    live_q: &context.live_q,
                    binding: 1,
                    crt_point: point,
                };
                let effect = lib::edit_fixed_edge(&mut image, runtime, args.index, side, input)?;
                let resource_q = cs705::icf::context::Context::from_bytes(
                    *context
                        .ranges
                        .source_q()
                        .ok_or_else(|| AppError::invalid("resource generation Q is unavailable"))?,
                );
                let visible =
                    lib::rf_row_visible(&resource_q, lib::RfLayout::FixedEdges, args.index)?;
                Ok((
                    json!({"stored":effect.stored,"helper_accepted":effect.helper_accepted,"candidate_hz":effect.candidate_hz,"raw_candidate_hz":effect.raw_candidate_hz,
                    "pair_hz":effect.pair_hz,"range_bad":effect.range_bad,"absolute_bad":effect.absolute_bad,"state_out":effect.state_out,"notify_flags":effect.notify_flags,
                    "context":native::report(&context),"resource_visible":visible}),
                    effect.stored,
                ))
            });
            finish(&image, &args.read, &args.publication, &before, result)
        }
        Command::HomeVfo {
            read,
            value,
            range_context,
            warning_bits,
            publication,
        } => {
            io::single_stdin([read.input.as_str(), range_context.as_str()])?;
            if publication.accept_partial {
                confirm(publication.yes)?;
            }
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let context = native::read(&range_context)?;
            let result = number_action(&value, |input, point| {
                let effect = lib::edit_home_vfo(
                    &mut image,
                    &context.ranges,
                    &context.live_q,
                    input,
                    point,
                    warning_bits,
                )?;
                Ok((
                    json!({"number":number_json(&effect.number),"warning_bits":effect.warning_bits,"empty_marker":effect.empty_marker,"outside_marker":effect.outside_marker,"context":native::report(&context)}),
                    effect.number.accepted,
                ))
            });
            finish(&image, &read, &publication, &before, result)
        }
        Command::HomeGroup {
            read,
            normal_working,
            catalog,
            initial_catalog: _,
            value,
            publication,
        } => {
            io::single_stdin(
                std::iter::once(read.input.as_str())
                    .chain(std::iter::once(normal_working.as_str()))
                    .chain(catalog.as_deref()),
            )?;
            if publication.accept_partial {
                confirm(publication.yes)?;
            }
            let bytes = io::read(&read.input)?;
            let mut image = load(&bytes, read.kind)?;
            let before = image.memory().to_vec();
            let normal = cs705::icf::rows::NormalMemory::from_working(io::read(&normal_working)?)?;
            let catalog = if let Some(path) = catalog {
                let value: Value = serde_json::from_slice(&io::read(&path)?)
                    .map_err(|_| AppError::invalid("invalid Home catalog JSON"))?;
                let obj = value
                    .as_object()
                    .filter(|o| o.len() == 2 && o.contains_key("empty") && o.contains_key("call"))
                    .ok_or_else(|| AppError::invalid("Home catalog requires empty/call only"))?;
                lib::HomeGroupCatalog {
                    empty: i32::parse(&obj["empty"])?,
                    call: i32::parse(&obj["call"])?,
                }
            } else {
                lib::HomeGroupCatalog::initial()
            };
            let result = number_action(&value, |input, _| {
                let effect = lib::edit_home_group(&mut image, &normal, catalog, input)?;
                Ok((
                    json!({"stored":effect.stored,"candidate":effect.candidate,"group_raw":effect.group_raw,"channel_raw":effect.channel_raw,"state_out":effect.state_out,"notify_flags":effect.notify_flags}),
                    effect.stored,
                ))
            });
            finish(&image, &read, &publication, &before, result)
        }
        Command::Metadata(args) => {
            io::single_stdin([args.read.input.as_str()])?;
            if let MetadataAction::Clear { yes, .. } = &args.action {
                confirm(*yes)?;
            }
            let bytes = io::read(&args.read.input)?;
            let mut image = load(&bytes, args.read.kind)?;
            let field = args.field.field();
            match args.action {
                MetadataAction::Show => {
                    let view = field.read(&image);
                    render(
                        &json!({"feature_id":field.feature_id(),"read_only":view.read_only,
                    "attached":view.attached,"name":io::bytes_value(&view.name),"stem":io::bytes_value(&view.stem)}),
                        args.read.format,
                    )?;
                    Ok(0)
                }
                MetadataAction::Clear { write, .. } => {
                    let effect = field.clear(&mut image, field.clear_command())?;
                    io::report(
                        &json!({"stored":effect.stored,"notify_flags":effect.notify_flags,"command":field.clear_command(),"file_import":false}),
                    )?;
                    publish(&image, args.read.kind, &write)?;
                    Ok(0)
                }
            }
        }
    }
}
