//! Command grammar. No argument parsing opens a file, port or radio.

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "cs705",
    version,
    about = "Native CS-705 configuration tools",
    long_about = "Native CS-705 configuration tools.\nInspect and edit files offline, or use clone subcommands for radio transfers. Indices are zero-based."
)]
pub struct Cli {
    /// Explicit application preference file; no implicit home-directory state.
    #[arg(long, global = true)]
    pub config: Option<String>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Browse independent English help; inspect original resources only as an optional oracle.
    ItemHelp(crate::item_help::Args),
    /// Drive information and COM controls from explicit source observations.
    Information(crate::information::Args),
    /// Print typed sheets and fields with independent English labels; an oracle is optional.
    Print(crate::printing::Args),
    /// Execute native Document open, New, normalization and Save operations.
    Document(crate::documents::Args),
    /// Drive native save dialogs, recent files and document lifecycle state.
    Lifecycle(crate::lifecycle::Args),
    /// Show typed views and display settings; source-state operations are an optional oracle.
    Display(crate::display::Args),
    /// Invoke native compound setting editors with explicit runtime sources.
    Editor(crate::editors::Args),
    /// Invoke native Repeater field handlers with actual Q/B/G sources.
    Repeater(crate::repeaters::Args),
    /// Edit and inspect special-memory, scan-edge and preset records.
    Special(crate::special::Args),
    /// Open a zero-based entry from the explicit config's recent-files list.
    Recent(crate::lifecycle::RecentArgs),
    /// Parse an input document and report its structure.
    Parse {
        #[command(flatten)]
        read: Read,
        /// Use the original permissive reader, not canonical checksum validation.
        #[arg(long, requires = "seed")]
        source_reader: bool,
        /// Explicit initial raw reader destination; required for --source-reader.
        #[arg(long, requires = "source_reader")]
        seed: Option<String>,
    },
    /// Validate multiple documents, reporting every failure (exit 5 if mixed).
    Check {
        #[arg(required = true)]
        inputs: Vec<String>,
        #[arg(long, value_enum, default_value = "icf")]
        kind: Kind,
        #[arg(long, value_enum, default_value = "json")]
        format: Format,
    },
    /// Show semantic fields or a bounded sheet view; unknown values stay errors.
    Show {
        #[command(flatten)]
        read: Read,
        /// Field path, e.g. setting/beep_level_raw/0 or physical-memory-filter/0/Main.
        #[arg(long, conflicts_with = "list_fields")]
        key: Vec<String>,
        /// List available typed field paths and their argument/value domains.
        #[arg(long)]
        list_fields: bool,
        #[arg(long, value_enum)]
        #[arg(conflicts_with = "key")]
        sheet: Option<Sheet>,
        /// Comma-separated columns in requested order; all are shown if absent.
        #[arg(long, value_delimiter = ',')]
        columns: Vec<String>,
        #[arg(long, default_value_t = 0)]
        start: usize,
        #[arg(long, default_value_t = 100)]
        count: usize,
        #[command(flatten)]
        record_options: crate::records::Options,
    },
    /// Apply semantic edits; publish only after actual approval and no failures.
    Edit {
        #[command(flatten)]
        read: Read,
        /// Repeat KEY[/ARG...]=VALUE; values accept JSON or unquoted enum/text.
        #[arg(long = "set", required_unless_present = "events")]
        assignments: Vec<String>,
        /// Apply setting, network/prefix, number, choice or text editor events.
        #[arg(long = "event", required_unless_present = "assignments")]
        events: Vec<String>,
        #[command(flatten)]
        record_options: crate::records::Options,
        #[command(flatten)]
        write: Write,
    },
    /// Create a native document from an explicit source generation and state.
    New(crate::documents::NewArgs),
    /// Convert an explicitly selected representation or revision.
    Convert {
        input: String,
        #[arg(long, value_enum)]
        conversion: Conversion,
        #[arg(long)]
        source_profile: Option<u8>,
        #[arg(long)]
        destination_profile: Option<u8>,
        /// Explicit generation G for conversions requiring factory/context data.
        #[arg(long)]
        seed: Option<String>,
        #[command(flatten)]
        write: Write,
    },
    /// Import CSV with explicit target and partial-update publication policy.
    Import {
        input: String,
        #[arg(long)]
        csv: String,
        #[arg(long, value_enum)]
        sheet: Sheet,
        #[arg(long)]
        group: Option<usize>,
        #[arg(long, value_enum, default_value = "replace")]
        mode: ImportMode,
        #[arg(long)]
        accept_partial: bool,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        csv_options: crate::csv::Options,
        #[command(flatten)]
        write: Write,
    },
    /// Export a sheet as CSV or a print-ready terminal/JSON report.
    Export {
        input: String,
        #[arg(long, value_enum)]
        sheet: Sheet,
        #[arg(long)]
        group: Option<usize>,
        #[arg(long, value_enum, default_value = "csv")]
        format: ExportFormat,
        #[command(flatten)]
        csv_options: crate::csv::Options,
        #[command(flatten)]
        record_options: crate::records::Options,
        #[command(flatten)]
        write: Write,
    },
    /// List/rename logical groups without confusing them with physical slots.
    Group {
        input: String,
        #[arg(long, value_enum)]
        sheet: Sheet,
        #[command(subcommand)]
        action: GroupAction,
    },
    /// Copy/paste/move/clear/delete/insert rows using explicit sheet coordinates.
    Row {
        input: String,
        #[arg(long, value_enum)]
        sheet: Sheet,
        #[command(flatten)]
        record_options: crate::records::Options,
        #[command(subcommand)]
        action: RowAction,
    },
    /// Clone tools. Live I/O requires explicit port and safety acknowledgement.
    Clone {
        #[command(subcommand)]
        action: CloneAction,
    },
    /// Explicit application preferences, stored only in the requested file.
    Config {
        file: String,
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Args)]
pub struct Read {
    /// Input path, or - for stdin.
    pub input: String,
    #[arg(long, value_enum, default_value = "icf")]
    pub kind: Kind,
    #[arg(long, value_enum, default_value = "json")]
    pub format: Format,
}

#[derive(Args, Clone)]
#[group(skip)]
pub struct Write {
    /// Output path, or - for stdout. Required unless --dry-run.
    #[arg(short, long, required_unless_present = "dry_run")]
    pub output: Option<String>,
    /// Atomically replace an existing regular file; symlinks are rejected.
    #[arg(long)]
    pub overwrite: bool,
    /// Run all validation and in-memory changes without publishing payload.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Icf,
    RepeaterCsv,
    GpsCsv,
    Memory,
}
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Json,
    Text,
}
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Json,
    Text,
}
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conversion {
    /// Canonicalize the ICF text representation without native stage9.
    Canonicalize,
    Rev5To6,
    IcfToMemory,
    MemoryToIcf,
    Profile,
}
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportMode {
    Replace,
    PreserveOrderUse,
    PreserveUse,
    Cancel,
}
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sheet {
    Memory,
    Call,
    Bandstack,
    Memopad,
    Scan,
    UserEdge,
    Gps,
    Repeater,
    FromDv,
    FromFm,
    To,
    Rx,
    Ur,
    Preset,
    All,
}

#[derive(Subcommand)]
pub enum GroupAction {
    List,
    /// Add a normal-memory group; no name means cancel the name editor.
    Add {
        /// Printable ASCII name, at most 16 bytes; no implicit ACP conversion.
        #[arg(long)]
        name: Option<String>,
        #[command(flatten)]
        write: Write,
    },
    Rename {
        index: usize,
        /// Printable ASCII name, at most 16 bytes; no implicit ACP conversion.
        name: String,
        #[command(flatten)]
        write: Write,
    },
}

#[derive(Subcommand)]
pub enum RowAction {
    Copy {
        group: usize,
        index: usize,
        #[command(flatten)]
        write: Write,
    },
    Paste {
        group: usize,
        index: usize,
        #[arg(long)]
        clipboard: String,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        write: Write,
    },
    Move {
        group: usize,
        index: usize,
        #[arg(long)]
        destination_group: usize,
        #[arg(long)]
        destination_index: usize,
        #[command(flatten)]
        write: Write,
    },
    Clear {
        group: usize,
        index: usize,
        #[arg(long)]
        yes: bool,
        /// Explicit decoded template clipboard for GPS clear.
        #[arg(long)]
        seed: Option<String>,
        #[command(flatten)]
        write: Write,
    },
    Delete {
        group: usize,
        index: usize,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        write: Write,
    },
    Insert {
        group: usize,
        index: usize,
        /// Required when an occupied tail row would be released.
        #[arg(long)]
        yes: bool,
        /// Explicit decoded type 20 template clipboard for GPS insertion.
        #[arg(long)]
        seed: Option<String>,
        #[command(flatten)]
        write: Write,
    },
}

#[derive(Subcommand)]
pub enum CloneAction {
    /// Decode a binary file/stdin stream; no serial transport is opened.
    Decode {
        input: String,
        #[arg(long, value_enum)]
        direction: WireDirection,
    },
    /// Encode one explicit frame to a binary file/stdout.
    Encode {
        #[arg(long, value_enum)]
        frame: FrameKind,
        #[arg(long, value_enum)]
        direction: WireDirection,
        #[arg(long)]
        address: Option<u32>,
        #[arg(long)]
        payload_hex: Option<String>,
        #[arg(long)]
        block_count: Option<u8>,
        #[command(flatten)]
        write: Write,
    },
    /// Query radio identity using an explicit serial port or mock transcript.
    Query {
        #[command(flatten)]
        transport: TransportArgs,
    },
    /// List candidate ports or probe explicitly selected ports.
    Search {
        /// List candidate ports without opening them.
        #[arg(long, conflicts_with_all = ["port", "mock"])]
        list: bool,
        /// Probe only these explicitly named ports, in order.
        #[arg(long, conflicts_with = "mock")]
        port: Vec<String>,
        /// Exercise discovery through a mock transcript, never the OS.
        #[arg(long)]
        mock: Option<String>,
        /// Allow device I/O on the explicitly selected ports.
        #[arg(long)]
        allow_device_io: bool,
        /// Acknowledge that opening a port may transiently assert DTR/RTS.
        #[arg(long)]
        acknowledge_line_risk: bool,
        /// Confirm probing the explicitly selected ports.
        #[arg(long)]
        yes: bool,
    },
    /// Native query/read and actual Document adoption/error callbacks.
    Read(crate::wire::native_read::Args),
    /// Explicit low-level raw read without Document adoption.
    RawRead {
        #[command(flatten)]
        transport: TransportArgs,
        #[arg(long)]
        profile: u8,
        /// Publish only a documented acquired prefix on failure.
        #[arg(long)]
        accept_partial: bool,
        #[command(flatten)]
        write: Write,
    },
    /// Native document validation, private correction and actual clone write.
    Write(crate::wire::document::Args),
    /// Run the actual option producer without starting a transport operation.
    Options {
        input: String,
        #[command(flatten)]
        write: Write,
    },
    /// Low-level raw protocol write, without native document validation.
    RawWrite {
        input: String,
        #[command(flatten)]
        transport: TransportArgs,
        #[arg(long)]
        profile: u8,
        /// Optional regions: reference=1, WLAN=2, CI-V=4, own=8, gateway=16.
        #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=31))]
        mask: u8,
        /// Confirm the radio write; failed or unconfirmed writes are not retried automatically.
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        write: Write,
    },
    /// Show explicit read/write address plans; this is not radio authorization.
    Plan {
        #[arg(long)]
        profile: u8,
        #[arg(long)]
        write: bool,
        #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=31))]
        mask: u8,
    },
}

#[derive(Args)]
pub struct TransportArgs {
    /// Use a recorded transcript instead of opening a serial port.
    #[arg(long, conflicts_with = "port", required_unless_present = "port")]
    pub mock: Option<String>,
    /// Serial device path; never selected or opened implicitly.
    ///
    /// Live I/O requires --allow-device-io and --acknowledge-line-risk.
    #[arg(long, conflicts_with = "mock", required_unless_present = "mock")]
    pub port: Option<String>,
    /// Allow device I/O on the explicitly selected serial port.
    #[arg(long)]
    pub allow_device_io: bool,
    /// OS open may transiently assert DTR/RTS despite requested inactive levels.
    #[arg(long)]
    pub acknowledge_line_risk: bool,
    #[arg(long, default_value_t = 115200, value_parser = clap::value_parser!(u32).range(300..=4_000_000))]
    pub baud: u32,
    #[arg(long)]
    pub dtr: bool,
    #[arg(long)]
    pub rts: bool,
    #[arg(long, value_enum, default_value = "auto")]
    pub echo: EchoMode,
    #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u64).range(1..=60000))]
    pub timeout_ms: u64,
    /// Startup script bytes from an explicit file (hex/settings tokens).
    #[arg(long)]
    pub startup_script: Option<String>,
    /// Cancel after this many monotonic wall-clock milliseconds (zero is immediate).
    #[arg(long, value_parser = clap::value_parser!(u64).range(0..=600000))]
    pub cancel_after_ms: Option<u64>,
}
#[derive(ValueEnum, Clone, Copy)]
pub enum EchoMode {
    None,
    Known,
    Auto,
}
#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum WireDirection {
    ToRadio,
    FromRadio,
}
#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum FrameKind {
    Identify,
    Read,
    Write,
    Data,
    End,
    Acknowledgement,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    Show,
    Set {
        #[arg(required = true)]
        assignments: Vec<String>,
        #[command(flatten)]
        write: Write,
    },
}
