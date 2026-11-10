//! Offline lifecycle roots. Gestures and OS observations are distinct inputs.

use crate::{AppError, Result, args::Write, documents, io};
use clap::{Args as ClapArgs, Subcommand};
use cs705::icf::{
    self,
    document::{Document, FileError, SaveOutcome, ValidationSource, lifecycle::*},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};

#[derive(ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    action: Action,
    #[command(flatten)]
    options: Options,
}

#[derive(ClapArgs)]
pub struct RecentArgs {
    index: usize,
    #[arg(long)]
    profile_choice: Option<i32>,
    #[command(flatten)]
    options: Options,
}

#[derive(ClapArgs)]
struct Options {
    #[command(flatten)]
    session: documents::SessionArgs,
    /// Cancel before reading any source or publishing anything.
    #[arg(long)]
    cancel: bool,
    /// Current native path bytes, independently of the next Open selection.
    #[arg(long, default_value = "")]
    document_path_hex: String,
    #[arg(long, default_value_t = 6)]
    target_revision: u32,
    /// Apply actual setting edits before the root, making the document dirty.
    #[arg(long = "set")]
    settings: Vec<String>,
    /// Ordered user gestures, not save outcomes or GUI-success callbacks.
    #[arg(long)]
    gestures: Option<String>,
    /// Ordered CompareStringA observations with raw left/right paths.
    #[arg(long)]
    path_comparisons: Option<String>,
    /// Ten raw-path hex strings, in native Recent0..Recent9 order.
    #[arg(long)]
    recent_input: Option<String>,
    /// Persist all ten raw-path slots using safe atomic publication.
    #[arg(long)]
    recent_output: Option<String>,
    /// Actual GetWindowPlacement result object, or null for BOOL false.
    #[arg(long)]
    placement: Option<String>,
    #[arg(long)]
    validation_views: Option<String>,
    #[arg(long)]
    show_both: Option<u32>,
    #[arg(long)]
    home_cache: Option<u8>,
    /// Explicitly authorize file publication selected by the gestures.
    #[arg(long)]
    allow_file_write: bool,
    #[arg(long)]
    overwrite: bool,
    /// Capture form+58; zero refuses Close before any prompt or placement read.
    #[arg(long, default_value_t = 1)]
    form_enabled: u8,
    /// Preserve an incoming false CloseQuery flag.
    #[arg(long)]
    incoming_false: bool,
}

#[derive(Subcommand)]
enum Action {
    New,
    Open {
        /// Chosen raw filesystem path; absence is a file-dialog cancellation.
        #[arg(long)]
        path_hex: Option<String>,
        #[arg(long)]
        profile_choice: Option<i32>,
    },
    Save,
    SaveAs,
    Recent {
        index: usize,
        #[arg(long)]
        profile_choice: Option<i32>,
    },
    Close,
}

fn image_error(reason: &'static str) -> icf::Error {
    icf::Error::Invalid { line: 0, reason }
}
fn file_error(error: FileError) -> AppError {
    match error {
        FileError::Image(e) => e.into(),
        FileError::Output(e) => e.into(),
    }
}
fn value(path: &str) -> Result<Value> {
    serde_json::from_slice(&io::read(path)?)
        .map_err(|_| AppError::invalid("invalid lifecycle source JSON"))
}
fn exact(v: &Value, keys: &[&str]) -> Result<()> {
    let object = v
        .as_object()
        .ok_or_else(|| AppError::invalid("lifecycle source requires an object"))?;
    if object.len() != keys.len() || keys.iter().any(|key| !object.contains_key(*key)) {
        return Err(AppError::invalid(
            "lifecycle source has missing or unexpected fields",
        ));
    }
    Ok(())
}
fn integer(v: &Value) -> Result<i32> {
    v.as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| AppError::invalid("source requires i32"))
}
fn unsigned(v: &Value) -> Result<u32> {
    v.as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| AppError::invalid("source requires u32"))
}
fn hex(v: &Value) -> Result<Vec<u8>> {
    io::decode_hex(
        v.as_str()
            .ok_or_else(|| AppError::invalid("raw byte source requires hex string"))?,
    )
}

enum Gesture {
    Choice(PromptKind, i32),
    SaveAs(Option<SaveSelection>),
}
#[derive(PartialEq, Eq)]
enum PromptKind {
    Dirty,
    Legacy,
    Warning,
}
struct UserInput(VecDeque<Gesture>);
impl UserInput {
    fn load(path: Option<&str>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self(VecDeque::new()));
        };
        let value = value(path)?;
        let rows = value
            .as_array()
            .ok_or_else(|| AppError::invalid("gestures require an ordered array"))?;
        let mut gestures = VecDeque::new();
        for row in rows {
            match row["kind"].as_str() {
                Some("save-as") => {
                    exact(row, &["kind", "path_hex", "filter_index"])?;
                    let selection = if row["path_hex"].is_null() {
                        if !row["filter_index"].is_null() {
                            return Err(AppError::invalid(
                                "cancelled dialog has no selected filter",
                            ));
                        }
                        None
                    } else {
                        Some(SaveSelection {
                            path: hex(&row["path_hex"])?,
                            filter_index: unsigned(&row["filter_index"])?,
                        })
                    };
                    gestures.push_back(Gesture::SaveAs(selection));
                }
                Some(kind @ ("dirty" | "legacy" | "warning")) => {
                    exact(row, &["kind", "choice"])?;
                    let kind = match kind {
                        "dirty" => PromptKind::Dirty,
                        "legacy" => PromptKind::Legacy,
                        _ => PromptKind::Warning,
                    };
                    gestures.push_back(Gesture::Choice(kind, integer(&row["choice"])?));
                }
                _ => return Err(AppError::invalid("unknown lifecycle gesture")),
            }
        }
        Ok(Self(gestures))
    }
}
impl Dialogs for UserInput {
    fn save_as(
        &mut self,
        _initial: SaveDialog,
    ) -> std::result::Result<Option<SaveSelection>, icf::Error> {
        match self.0.pop_front() {
            Some(Gesture::SaveAs(selection)) => Ok(selection),
            _ => Err(image_error("missing or out-of-order Save As user gesture")),
        }
    }
    fn choice(&mut self, prompt: Prompt) -> std::result::Result<i32, icf::Error> {
        let expected = match prompt {
            Prompt::Dirty => PromptKind::Dirty,
            Prompt::Legacy { .. } => PromptKind::Legacy,
            Prompt::Warning { .. } => PromptKind::Warning,
        };
        match self.0.pop_front() {
            Some(Gesture::Choice(kind, choice)) if kind == expected => Ok(choice),
            _ => Err(image_error("missing or out-of-order native prompt gesture")),
        }
    }
}

struct Comparison {
    left: Vec<u8>,
    right: Vec<u8>,
    result: i32,
}
struct Comparisons(VecDeque<Comparison>);
impl Comparisons {
    fn load(path: Option<&str>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self(VecDeque::new()));
        };
        let value = value(path)?;
        let rows = value
            .as_array()
            .ok_or_else(|| AppError::invalid("path comparisons require an ordered array"))?;
        let mut out = VecDeque::new();
        for row in rows {
            exact(row, &["left_hex", "right_hex", "locale", "flags", "result"])?;
            let result = integer(&row["result"])?;
            if unsigned(&row["locale"])? != 0x400
                || unsigned(&row["flags"])? != 1
                || !(0..=3).contains(&result)
            {
                return Err(AppError::invalid(
                    "path comparison requires locale0x400/flags1/result0..3",
                ));
            }
            out.push_back(Comparison {
                left: hex(&row["left_hex"])?,
                right: hex(&row["right_hex"])?,
                result,
            });
        }
        Ok(Self(out))
    }
}
impl PathComparison for Comparisons {
    fn compare(&mut self, left: &[u8], right: &[u8]) -> std::result::Result<i32, icf::Error> {
        match self.0.pop_front() {
            Some(c) if c.left == left && c.right == right => Ok(c.result),
            _ => Err(image_error(
                "missing or mismatched raw CompareStringA observation",
            )),
        }
    }
}

struct FileAccess {
    allow: bool,
    overwrite: bool,
    publications: usize,
    failed: bool,
}
fn os_path(path: &[u8]) -> std::io::Result<&str> {
    // Existing bounded CLI I/O accepts UTF-8 filesystem names. Preserve every byte
    // or refuse; do not lossy-decode an ANSI path into a different destination.
    let path = std::str::from_utf8(path)
        .map_err(|_| std::io::Error::other("CLI file access requires an exact UTF-8 path"))?;
    if path.is_empty() || path == "-" || path.contains('\0') {
        return Err(std::io::Error::other(
            "lifecycle file access requires a nonempty regular-file path",
        ));
    }
    Ok(path)
}
impl Files for FileAccess {
    fn read(&mut self, path: &[u8]) -> std::io::Result<Vec<u8>> {
        io::read(os_path(path)?).map_err(|e| std::io::Error::other(e.message))
    }
    fn publish(&mut self, path: &[u8], bytes: &[u8]) -> std::io::Result<()> {
        let result = (|| {
            if !self.allow {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "file publication requires --allow-file-write",
                ));
            }
            let write = Write {
                output: Some(os_path(path)?.to_owned()),
                overwrite: self.overwrite,
                dry_run: false,
            };
            io::publish(bytes, &write).map_err(|e| std::io::Error::other(e.message))
        })();
        match result {
            Ok(()) => {
                self.publications += 1;
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }
}

struct Slots([Vec<u8>; RECENT_LIMIT]);
impl RecentStore for Slots {
    fn read(&mut self, index: usize) -> std::io::Result<Vec<u8>> {
        Ok(self.0[index].clone())
    }
    fn write(&mut self, index: usize, path: &[u8]) -> std::io::Result<()> {
        self.0[index] = path.to_vec();
        Ok(())
    }
}
impl Slots {
    fn load(path: Option<&str>, config: Option<&Value>) -> Result<Self> {
        let mut slots: [Vec<u8>; RECENT_LIMIT] = std::array::from_fn(|_| Vec::new());
        if let Some(path) = path {
            let value = value(path)?;
            let rows = value
                .as_array()
                .filter(|r| r.len() == RECENT_LIMIT)
                .ok_or_else(|| {
                    AppError::invalid("recent store requires exactly ten raw-path hex strings")
                })?;
            for (slot, row) in slots.iter_mut().zip(rows) {
                *slot = hex(row)?;
            }
        } else if let Some(config) = config {
            if let Some(rows) = config["recent-files"].as_array() {
                for (slot, row) in slots.iter_mut().zip(rows) {
                    *slot = row
                        .as_str()
                        .ok_or_else(|| AppError::invalid("config recent path requires a string"))?
                        .as_bytes()
                        .to_vec();
                }
            }
        }
        Ok(Self(slots))
    }
}

struct PlacementInput(Option<String>);
impl PlacementSource for PlacementInput {
    fn placement(&mut self) -> std::result::Result<Option<Placement>, icf::Error> {
        let path = self.0.as_deref().ok_or(image_error(
            "Close requires an actual placement observation",
        ))?;
        let parse = || -> Result<Option<Placement>> {
            let v = value(path)?;
            if v.is_null() {
                return Ok(None);
            }
            exact(&v, &["flags", "show", "minimum", "maximum", "normal"])?;
            fn coordinates<const N: usize>(v: &Value) -> Result<[i32; N]> {
                let rows = v
                    .as_array()
                    .filter(|r| r.len() == N)
                    .ok_or_else(|| AppError::invalid("placement coordinate count mismatch"))?;
                let mut out = [0; N];
                for (out, value) in out.iter_mut().zip(rows) {
                    *out = integer(value)?;
                }
                Ok(out)
            }
            Ok(Some(Placement {
                flags: unsigned(&v["flags"])?,
                show: unsigned(&v["show"])?,
                minimum: coordinates(&v["minimum"])?,
                maximum: coordinates(&v["maximum"])?,
                normal: coordinates(&v["normal"])?,
            }))
        };
        parse().map_err(|_| image_error("invalid or unavailable placement observation"))
    }
}

fn save_report(flow: &SaveFlow) -> Value {
    json!({"native_return": if flow.outcome == SaveOutcome::Saved {0} else {1}, "dialog_cancelled":flow.dialog_cancelled,
        "attempts":flow.attempts.iter().map(|a| json!({"path":io::bytes_value(&a.selection.path),"filter_index":a.selection.filter_index,
            "legacy_choice":a.legacy_choice,"warning_choice":a.warning_choice,"status":a.report.as_ref().map(|r|format!("{:?}",r.status)),
            "publication_error":a.publication_error.as_ref().map(|e|e.kind().to_string()),
            "validation":a.report.as_ref().map(|r|json!({"errors":r.validation.errors,"warnings":r.validation.warnings,"checksum_written":r.checksum_written}))})).collect::<Vec<_>>()})
}
fn prompt_report(prompt: &DirtyPrompt) -> Value {
    json!({"choice":prompt.choice,"result":prompt.result,"save":prompt.save.as_ref().map(save_report)})
}

pub fn run(args: Args) -> Result<u8> {
    execute(args.options, args.action, None, None)
}

/// The existing top-level Recent root must call this instead of read->summary.
/// The same session, dirty prompt, profile and MRU drivers are used by Open.
pub fn recent(args: RecentArgs, preferences: &Value, config_path: Option<&str>) -> Result<u8> {
    execute(
        args.options,
        Action::Recent {
            index: args.index,
            profile_choice: args.profile_choice,
        },
        Some(preferences),
        config_path,
    )
}

fn execute(
    args: Options,
    action: Action,
    preferences: Option<&Value>,
    config_path: Option<&str>,
) -> Result<u8> {
    if args.cancel {
        io::report(&json!({"status":"cancelled","inputs_read":false,"published":false}))?;
        return Ok(0);
    }
    let assets = documents::assets(&args.session)?;
    if [
        args.gestures.as_deref(),
        args.path_comparisons.as_deref(),
        args.recent_input.as_deref(),
        args.placement.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|path| path == "-")
    {
        return Err(AppError::invalid(
            "lifecycle gesture/OS/store sources require regular files, not stdin",
        ));
    }
    documents::stdin_inputs(
        &args.session,
        None,
        args.validation_views.as_deref(),
        &assets,
    )?;
    let mut document = Document::from_current(
        documents::current(&args.session)?,
        io::decode_hex(&args.document_path_hex)?,
        args.target_revision,
    );
    let mut cache = BTreeMap::new();
    let mut session = documents::session(&args.session, &assets, document.current(), &mut cache)?;
    for setting in &args.settings {
        let (key, value) = setting
            .split_once('=')
            .ok_or_else(|| AppError::invalid("setting requires NAME[/INDEX]=integer"))?;
        let (name, index) = key.split_once('/').map_or(Ok((key, 0)), |(name, index)| {
            index
                .parse::<usize>()
                .map(|index| (name, index))
                .map_err(|_| AppError::invalid("invalid setting index"))
        })?;
        let value = value
            .parse::<i64>()
            .map_err(|_| AppError::invalid("setting value requires integer"))?;
        document.edit_setting(name, index, &icf::settings::Value::Number(value))?;
    }
    let mut dialogs = UserInput::load(args.gestures.as_deref())?;
    let mut comparisons = Comparisons::load(args.path_comparisons.as_deref())?;
    let mut slots = Slots::load(args.recent_input.as_deref(), preferences)?;
    let recent = RecentFiles::load(&mut slots, &mut comparisons).map_err(file_error)?;
    let mut lifecycle = Lifecycle {
        recent,
        ui: UiState::headless(),
    };
    lifecycle.ui.owner_enabled(args.form_enabled)?;
    let mut files = FileAccess {
        allow: args.allow_file_write,
        overwrite: args.overwrite,
        publications: 0,
        failed: false,
    };
    let views = documents::validation_views(args.validation_views.as_deref())?;
    let mut reset = if args.show_both.is_some() || args.home_cache.is_some() {
        Some(documents::reset_state(args.show_both, args.home_cache)?)
    } else {
        None
    };
    let mut options = SaveOptions {
        validation: views
            .as_deref()
            .map_or(ValidationSource::NoTree, ValidationSource::Views),
        reset: reset.as_mut(),
    };
    let mut incoming = !args.incoming_false;
    let mut placement = PlacementInput(args.placement);
    let effect = match &action {
        Action::Save | Action::SaveAs => save_report(
            &lifecycle
                .save(
                    &mut document,
                    &mut session,
                    if matches!(action, Action::SaveAs) {
                        SaveMode::SaveAs
                    } else {
                        SaveMode::Save
                    },
                    &mut options,
                    &mut dialogs,
                    &mut files,
                    &mut comparisons,
                )
                .map_err(file_error)?,
        ),
        Action::New => prompt_report(
            &lifecycle
                .new_file(
                    &mut document,
                    &mut session,
                    &mut options,
                    &mut dialogs,
                    &mut files,
                    &mut comparisons,
                )
                .map_err(file_error)?,
        ),
        Action::Close => {
            let prompt = lifecycle
                .close(
                    &mut document,
                    &mut session,
                    &mut incoming,
                    &mut options,
                    &mut dialogs,
                    &mut files,
                    &mut comparisons,
                    &mut placement,
                )
                .map_err(file_error)?;
            json!({"incoming":incoming,"prompt":prompt.as_ref().map(prompt_report)})
        }
        Action::Open { profile_choice, .. } | Action::Recent { profile_choice, .. } => {
            let path = match &action {
                Action::Recent { index, .. } => Some(lifecycle.recent.selected(*index)?.to_vec()),
                Action::Open { path_hex, .. } => {
                    path_hex.as_deref().map(io::decode_hex).transpose()?
                }
                _ => unreachable!(),
            };
            let prompt = lifecycle
                .dirty_prompt(
                    &mut document,
                    &mut session,
                    &mut options,
                    &mut dialogs,
                    &mut files,
                    &mut comparisons,
                )
                .map_err(file_error)?;
            let selected = if prompt.result == 2 {
                None
            } else {
                path.map(|path| files.read(&path).map(|bytes| OpenFile { path, bytes }))
                    .transpose()?
            };
            // Resolve the actual adopted-Q source recipe only when that path's
            // file has a successful reader and the user selected native Yes.
            let adopted = if *profile_choice == Some(6) {
                if let Some(selected) = &selected {
                    let file = icf::format::read(&selected.bytes)?;
                    let mut q = icf::context::Context::before_registry();
                    q.decode_packed(file.reader.extra_data);
                    if file.status == icf::format::Status::Success
                        && q.fields() != session.context.fields()
                    {
                        Some(documents::source(
                            &args.session,
                            &assets,
                            &q,
                            session.system_langid,
                            &mut cache,
                        )?)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let (prompt, status) = lifecycle
                .open_after_prompt(
                    &mut document,
                    &mut session,
                    prompt,
                    selected,
                    *profile_choice,
                    adopted.as_ref().map(documents::Source::borrow),
                    &mut comparisons,
                )
                .map_err(file_error)?;
            json!({"prompt":prompt_report(&prompt),"open_status":format!("{status:?}")})
        }
    };
    lifecycle.recent.persist(&mut slots)?;
    if let Some(path) = args.recent_output {
        let values: Vec<_> = slots
            .0
            .iter()
            .map(|p| io::bytes_value(p)["hex"].clone())
            .collect();
        files.publish(path.as_bytes(), &io::json_bytes(&json!(values))?)?;
    }
    if let (Some(config), Some(path)) = (preferences, config_path) {
        let mut config = config.clone();
        config["recent-files"] = Value::Array(
            slots
                .0
                .iter()
                .map(|p| {
                    String::from_utf8(p.clone())
                        .map(Value::String)
                        .map_err(|_| {
                            AppError::invalid(
                                "config cannot store a non-UTF8 recent path; use recent-output",
                            )
                        })
                })
                .collect::<Result<Vec<_>>>()?,
        );
        files.publish(path.as_bytes(), &io::json_bytes(&config)?)?;
    }
    io::stdout(&io::json_bytes(
        &json!({"effect":effect,"path":io::bytes_value(document.path()),"target_revision":document.target_revision(),"dirty":document.dirty(),
        "baseline_matches_current":document.baseline()==document.current(),"publications":files.publications,"unused_gestures":dialogs.0.len(),"unused_comparisons":comparisons.0.len(),
        "recent":lifecycle.recent.paths().iter().map(|p|io::bytes_value(p)).collect::<Vec<_>>(),"visible_recent":lifecycle.recent.visible().len(),
        "form_present":lifecycle.ui.form.is_some(),"view_present":lifecycle.ui.view.is_some(),"tree_present":lifecycle.ui.tree.is_some(),"task_enabled":lifecycle.ui.task_enabled,
        "task_period":lifecycle.ui.task_period,"placement_saved":lifecycle.ui.saved_placement.is_some(),"native_delivery":false}),
    )?)?;
    Ok(if files.failed { 4 } else { 0 })
}
