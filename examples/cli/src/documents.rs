//! Explicit offline Document operations. Assets, old current and captured G are
//! distinct inputs; native normalization is not text canonicalization.

use crate::{
    AppError, Result,
    args::{Format, Kind, Read, Write},
    io, native,
};
use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use cs705::{
    clone::IMAGE_CAPACITY,
    icf::{
        self,
        document::{
            DefaultSource, Document, OpenStatus, ResetEffects, SaveRequest, SaveStatus, Session,
            ValidationSource, ValidationView,
        },
    },
};
use serde_json::json;
use std::collections::BTreeMap;

#[derive(ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Adopt initial information from ICF, generating new Q/B/G from real assets.
    Initial(FileArgs),
    /// Read through the native reader/profile/stage8/stage9 document driver.
    Open(FileArgs),
    /// Adopt completed G, optionally generated from explicit pre24A20 assets.
    New(NewArgs),
    /// Open, then execute actual full stage9, not text canonicalization.
    Normalize(FileArgs),
    /// Open, apply semantic settings, then execute the real Save body.
    Save {
        #[command(flatten)]
        file: FileArgs,
        #[arg(long, default_value_t = 6)]
        target_revision: u32,
        #[arg(long, value_enum)]
        legacy_choice: Option<Choice>,
        /// Repeat NAME[/INDEX]=integer; uses the library setting registry.
        #[arg(long = "set")]
        settings: Vec<String>,
        /// Ordered actual typed views; absence is a real headless no-tree source.
        #[arg(long)]
        validation_views: Option<String>,
        /// Native warning response; No/Cancel continue without correction/CD.
        #[arg(long, value_enum)]
        warning_choice: Option<Choice>,
        #[arg(long)]
        show_both: Option<u32>,
        #[arg(long)]
        home_cache: Option<u8>,
    },
    /// Open, then run slot10 correction and report its actual ordered requests.
    Reset {
        #[command(flatten)]
        file: FileArgs,
        #[arg(long, required_unless_present = "cancel")]
        show_both: Option<u32>,
        #[arg(long, required_unless_present = "cancel")]
        home_cache: Option<u8>,
    },
}

enum FileOperation {
    Initial,
    Open,
    Normalize,
    Reset {
        show_both: Option<u32>,
        home_cache: Option<u8>,
    },
    Save {
        target: u32,
        legacy: Option<Choice>,
        settings: Vec<String>,
        views: Option<String>,
        warning: Option<Choice>,
        show_both: Option<u32>,
        home_cache: Option<u8>,
    },
}

#[derive(ValueEnum, Clone, Copy)]
enum Choice {
    Yes,
    No,
    Cancel,
}
impl Choice {
    fn native(self) -> i32 {
        match self {
            Self::Yes => 6,
            Self::No => 7,
            Self::Cancel => 2,
        }
    }
}

#[derive(ClapArgs)]
#[command(mut_arg("output", |arg| arg.required_unless_present_any(["dry_run", "cancel"])))]
struct FileArgs {
    /// Input path, or - for stdin; cancellation needs no dummy path.
    #[arg(required_unless_present = "cancel")]
    input: Option<String>,
    #[arg(long, value_enum, default_value = "icf")]
    kind: Kind,
    #[arg(long, value_enum, default_value = "json")]
    format: Format,
    #[command(flatten)]
    session: SessionArgs,
    /// Native profile mismatch choice; absent is an error only on mismatch.
    #[arg(long, value_enum)]
    profile_choice: Option<Choice>,
    /// Cancel the operation before opening any input or output file.
    #[arg(long)]
    cancel: bool,
    #[command(flatten)]
    write: Write,
}

#[derive(ClapArgs)]
#[command(mut_arg("output", |arg| arg.required_unless_present_any(["dry_run", "cancel"])))]
pub struct NewArgs {
    #[command(flatten)]
    session: SessionArgs,
    #[arg(long)]
    cancel: bool,
    #[command(flatten)]
    write: Write,
}

#[derive(ClapArgs)]
pub(crate) struct SessionArgs {
    /// Shared generation-Q/live-Q/SYSTEM LANGID JSON, not inferred from ICF.
    #[arg(long, required_unless_present = "cancel")]
    pub(crate) range_context: Option<String>,
    /// Actual previous current expanded raw image, exactly0x4997a bytes.
    #[arg(long, conflicts_with = "constructor_current")]
    pub(crate) current: Option<String>,
    /// Explicitly select the known initial zero-allocation current state.
    #[arg(long)]
    constructor_current: bool,
    /// Actual captured completed G; does not regenerate B or follow live Q.
    #[arg(
        long,
        requires = "generation_selector",
        conflicts_with = "regenerate_defaults"
    )]
    pub(crate) generation: Option<String>,
    #[arg(long, requires = "generation")]
    generation_selector: Option<u8>,
    /// Explicitly run Q->B->pre24A20 asset->stage7/8/9, replacing G.
    #[arg(long)]
    regenerate_defaults: bool,
    /// Repeat SELECTOR=PATH for pre24A20 source assets, never completed G.
    #[arg(long = "source-asset")]
    source_assets: Vec<String>,
    /// Actual120-byte Japanese early mode-pair source; read only on that branch.
    #[arg(long)]
    pub(crate) japanese_mode_pairs: Option<String>,
    /// Actual108-byte restricted band-stack source; read only on that branch.
    #[arg(long)]
    pub(crate) restricted_band_stack: Option<String>,
    /// Actual captured VA864614, not an inferred predicate result.
    #[arg(long, required_unless_present = "cancel")]
    previous_reload_guard: Option<u32>,
    /// Actual D+1AC/1AD as exactly two hexadecimal bytes.
    #[arg(long, required_unless_present = "cancel")]
    document_flags: Option<String>,
}

pub(crate) fn assets(options: &SessionArgs) -> Result<BTreeMap<u8, &str>> {
    let mut out = BTreeMap::new();
    for item in &options.source_assets {
        let (selector, path) = item
            .split_once('=')
            .ok_or_else(|| AppError::invalid("source asset requires SELECTOR=PATH"))?;
        let selector: u8 = selector
            .parse()
            .map_err(|_| AppError::invalid("source asset selector requires0..4"))?;
        if selector >= 5 || path.is_empty() || out.insert(selector, path).is_some() {
            return Err(AppError::invalid(
                "source asset selector must be unique0..4 with a path",
            ));
        }
    }
    Ok(out)
}

pub(crate) fn stdin_inputs<'a>(
    options: &'a SessionArgs,
    input: Option<&'a str>,
    views: Option<&'a str>,
    assets: &BTreeMap<u8, &'a str>,
) -> Result<()> {
    io::single_stdin(
        input
            .into_iter()
            .chain(views)
            .chain(
                [
                    options.range_context.as_deref(),
                    options.current.as_deref(),
                    options.generation.as_deref(),
                    options.japanese_mode_pairs.as_deref(),
                    options.restricted_band_stack.as_deref(),
                ]
                .into_iter()
                .flatten(),
            )
            .chain(assets.values().copied()),
    )
}

pub(crate) fn current(options: &SessionArgs) -> Result<Vec<u8>> {
    let image = if let Some(path) = &options.current {
        io::read(path)?
    } else if options.constructor_current {
        vec![0; IMAGE_CAPACITY]
    } else {
        return Err(AppError::invalid(
            "supply actual --current or explicit --constructor-current",
        ));
    };
    if image.len() != IMAGE_CAPACITY {
        return Err(AppError::invalid(
            "previous current requires exactly0x4997a bytes",
        ));
    }
    Ok(image)
}

pub(crate) struct Source {
    selector: u8,
    prefix: Vec<u8>,
    japanese: Option<[u8; 120]>,
    restricted: Option<[u8; 108]>,
}
impl Source {
    pub(crate) fn borrow(&self) -> DefaultSource<'_> {
        DefaultSource {
            selector: self.selector,
            prefix: &self.prefix,
            japanese_mode_pairs: self.japanese.as_ref(),
            restricted_band_stack: self.restricted.as_ref(),
        }
    }
}
fn asset_bytes(path: &str, cache: &mut BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>> {
    if !cache.contains_key(path) {
        cache.insert(path.to_owned(), io::read(path)?);
    }
    Ok(cache[path].clone())
}
fn branch<const N: usize>(
    path: &Option<String>,
    reason: &str,
    cache: &mut BTreeMap<String, Vec<u8>>,
) -> Result<[u8; N]> {
    asset_bytes(
        path.as_deref().ok_or_else(|| AppError::invalid(reason))?,
        cache,
    )?
    .try_into()
    .map_err(|_| AppError::invalid(reason))
}
pub(crate) fn source(
    options: &SessionArgs,
    assets: &BTreeMap<u8, &str>,
    q: &icf::context::Context,
    langid: Option<u16>,
    cache: &mut BTreeMap<String, Vec<u8>>,
) -> Result<Source> {
    let selector = q.source_selector(langid)?;
    let path = assets.get(&selector).ok_or_else(|| {
        AppError::invalid("no pre24A20 source asset for the actual captured selector")
    })?;
    Ok(Source {
        selector,
        prefix: asset_bytes(path, cache)?,
        japanese: if selector == 0 {
            Some(branch(
                &options.japanese_mode_pairs,
                "Japanese branch requires exactly120 source bytes",
                cache,
            )?)
        } else {
            None
        },
        restricted: if matches!(selector, 2 | 3) && q.effective_f()? == 0 {
            Some(branch(
                &options.restricted_band_stack,
                "restricted branch requires exactly108 source bytes",
                cache,
            )?)
        } else {
            None
        },
    })
}

pub(crate) fn session(
    options: &SessionArgs,
    assets: &BTreeMap<u8, &str>,
    current: &[u8],
    cache: &mut BTreeMap<String, Vec<u8>>,
) -> Result<Session> {
    let context = native::read(
        options
            .range_context
            .as_deref()
            .ok_or_else(|| AppError::invalid("range context is required"))?,
    )?;
    let flags: [u8; 2] = io::decode_hex(
        options
            .document_flags
            .as_deref()
            .ok_or_else(|| AppError::invalid("document flags are required"))?,
    )?
    .try_into()
    .map_err(|_| AppError::invalid("document flags require exactly two bytes"))?;
    let mut session = Session::new(
        context.live_q,
        context.system_langid,
        options
            .previous_reload_guard
            .ok_or_else(|| AppError::invalid("previous reload guard is required"))?,
        flags,
    );
    session.ranges = context.ranges;
    if let Some(path) = &options.generation {
        session.adopt_captured_defaults(
            io::read(path)?,
            options
                .generation_selector
                .ok_or_else(|| AppError::invalid("captured G requires its actual selector"))?,
        )?;
    }
    if options.regenerate_defaults {
        let source = source(
            options,
            assets,
            &session.context,
            session.system_langid,
            cache,
        )?;
        session.regenerate_defaults(source.borrow(), current)?;
    }
    Ok(session)
}

fn cancelled() -> Result<u8> {
    io::report(&json!({"status":"cancelled","published":false,"inputs_read":false}))?;
    Ok(0)
}
fn publish(
    doc: &Document,
    bytes: &[u8],
    write: &Write,
    status: &str,
    session: &Session,
    effects: serde_json::Value,
) -> Result<u8> {
    io::publish(bytes, write)?;
    io::report(
        &json!({"status":if write.dry_run {"prepared"} else {status},"published":!write.dry_run,"dry_run":write.dry_run,"dirty":doc.dirty(),"timer_ms":doc.timer_ms(),"target_revision":doc.target_revision(),"path":io::bytes_value(doc.path()),"default_selector":session.default_selector(),"range_source_selector":session.ranges.selected(),"live_kind":session.context.fields().0[0],"automatic_refresh":false,"effects":effects}),
    )?;
    Ok(0)
}

pub fn new(args: NewArgs) -> Result<u8> {
    if args.cancel {
        return cancelled();
    }
    let assets = assets(&args.session)?;
    stdin_inputs(&args.session, None, None, &assets)?;
    io::preflight(&args.write)?;
    let mut doc = Document::from_current(current(&args.session)?, Vec::new(), 6);
    let session = session(&args.session, &assets, doc.current(), &mut BTreeMap::new())?;
    doc.new_file(&session, 6)?; // Known clean initial document, no implicit Yes/Save.
    publish(
        &doc,
        &doc.current_bytes(&session)?,
        &args.write,
        "new",
        &session,
        json!({"generation":if args.session.regenerate_defaults {"recipe"}else{"captured"}}),
    )
}

pub(crate) fn reset_state(
    show_both: Option<u32>,
    home_cache: Option<u8>,
) -> Result<icf::normalizer::ResetState> {
    Ok(icf::normalizer::ResetState {
        show_both: show_both
            .ok_or_else(|| AppError::invalid("correction requires actual --show-both"))?,
        home_invalidated: home_cache
            .ok_or_else(|| AppError::invalid("correction requires actual --home-cache"))?,
    })
}
fn reset_effects(e: &ResetEffects, state: &icf::normalizer::ResetState) -> serde_json::Value {
    json!({"typed_save_calls":e.correction.typed_saved,"saved_repeaters":e.correction.lists.saved_repeaters,"deleted_repeaters":e.correction.lists.deleted_repeaters,"deleted_ur":e.correction.lists.deleted_ur,"deleted_allowed":e.correction.lists.deleted_allowed,"home_cache":state.home_invalidated,"notification_requests":e.notifications.iter().map(|n|json!({"event":n.event,"synchronous":n.synchronous,"first":n.first,"second":n.second})).collect::<Vec<_>>(),"notifications_delivered":false})
}
pub(crate) fn validation_views(path: Option<&str>) -> Result<Option<Vec<ValidationView>>> {
    let Some(path) = path else { return Ok(None) };
    let value: serde_json::Value = serde_json::from_slice(&io::read(path)?)
        .map_err(|_| AppError::invalid("invalid validation view JSON"))?;
    let rows = value
        .as_array()
        .ok_or_else(|| AppError::invalid("validation views require an ordered array"))?;
    rows.iter()
        .map(|v| {
            let object = v
                .as_object()
                .ok_or_else(|| AppError::invalid("validation view requires an object"))?;
            const KEYS: [&str; 7] = [
                "kind",
                "group",
                "first",
                "last",
                "count",
                "include_side1",
                "show_both",
            ];
            if object.len() != KEYS.len() || object.keys().any(|k| !KEYS.contains(&k.as_str())) {
                return Err(AppError::invalid(
                    "validation view requires exact typed coordinates, not warning flags",
                ));
            }
            let unsigned = |k: &str| {
                v[k].as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| AppError::invalid("validation unsigned field requires u32"))
            };
            let signed = |k: &str| {
                v[k].as_i64()
                    .and_then(|n| i32::try_from(n).ok())
                    .ok_or_else(|| AppError::invalid("validation signed field requires i32"))
            };
            Ok(ValidationView {
                kind: unsigned("kind")?,
                group: unsigned("group")?,
                first: signed("first")?,
                last: signed("last")?,
                count: signed("count")?,
                include_side1: v["include_side1"]
                    .as_bool()
                    .ok_or_else(|| AppError::invalid("validation side flag requires bool"))?,
                show_both: unsigned("show_both")?,
            })
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

fn file(args: FileArgs, operation: FileOperation) -> Result<u8> {
    if args.cancel {
        return cancelled();
    }
    if let FileOperation::Save {
        target,
        legacy: None,
        ..
    } = &operation
        && *target != 6
    {
        return Err(AppError::invalid(
            "non-six target requires explicit --legacy-choice yes|no|cancel",
        ));
    }
    let read = Read {
        input: args
            .input
            .ok_or_else(|| AppError::invalid("document input is required"))?,
        kind: args.kind,
        format: args.format,
    };
    if read.kind != Kind::Icf {
        return Err(AppError::invalid("document operations require --kind icf"));
    }
    let assets = assets(&args.session)?;
    let views = match &operation {
        FileOperation::Save { views, .. } => views.as_deref(),
        _ => None,
    };
    stdin_inputs(&args.session, Some(&read.input), views, &assets)?;
    io::preflight(&args.write)?;
    let mut doc = Document::from_current(current(&args.session)?, Vec::new(), 6);
    let mut cache = BTreeMap::new();
    let mut session = session(&args.session, &assets, doc.current(), &mut cache)?;
    let bytes = io::read(&read.input)?;
    // Only an actual Yes on an actual mismatch reaches a second asset recipe.
    let incoming = icf::format::read(&bytes)?;
    let mut q = icf::context::Context::before_registry();
    q.decode_packed(incoming.reader.extra_data);
    let adopted = if incoming.status == icf::format::Status::Success
        && (matches!(operation, FileOperation::Initial)
            || (q.fields() != session.context.fields()
                && args
                    .profile_choice
                    .is_some_and(|c| matches!(c, Choice::Yes))))
    {
        Some(source(
            &args.session,
            &assets,
            &q,
            session.system_langid,
            &mut cache,
        )?)
    } else {
        None
    };
    if matches!(operation, FileOperation::Initial) {
        let mut requests = Vec::new();
        let status = doc.initial_from_icf(
            &bytes,
            read.input.as_bytes(),
            &mut session,
            adopted.as_ref().map(Source::borrow),
            |event, _, _| {
                requests.push(format!("{event:?}"));
                Ok(())
            },
        )?;
        if status != icf::format::Status::Success {
            return Err(AppError::invalid(format!(
                "initial ICF rejected: {status:?}"
            )));
        }
        return publish(
            &doc,
            &doc.current_bytes(&session)?,
            &args.write,
            "initial-adopted",
            &session,
            json!({"requests":requests,"notifications_delivered":false}),
        );
    }
    let status = doc.read(
        Some(&bytes),
        read.input.as_bytes().to_vec(),
        6,
        args.profile_choice.map(Choice::native),
        &mut session,
        adopted.as_ref().map(Source::borrow),
    )?;
    match status {
        OpenStatus::Cancelled => {
            io::report(&json!({"status":"cancelled","published":false}))?;
            return Ok(0);
        }
        OpenStatus::Rejected(status) => {
            return Err(AppError::invalid(format!(
                "document read rejected: {status:?}"
            )));
        }
        OpenStatus::Opened { .. } => {}
    }
    let (output, label, effects) = match operation {
        FileOperation::Save {
            target,
            legacy,
            settings,
            views,
            warning,
            show_both,
            home_cache,
        } => {
            let legacy = legacy.map_or(0, Choice::native); // Target6 never requests this prompt.
            for setting in settings {
                let (key, value) = setting
                    .split_once('=')
                    .ok_or_else(|| AppError::invalid("setting requires NAME[/INDEX]=integer"))?;
                let (name, index) = key.split_once('/').map_or(Ok((key, 0)), |(name, index)| {
                    index
                        .parse::<usize>()
                        .map(|i| (name, i))
                        .map_err(|_| AppError::invalid("invalid setting index"))
                })?;
                let value = value
                    .parse::<i64>()
                    .map_err(|_| AppError::invalid("setting requires an integer value"))?;
                doc.edit_setting(name, index, &icf::settings::Value::Number(value))?;
            }
            if target != 6 && matches!(legacy, 7 | 2) {
                let status = doc
                    .save(&mut session, Vec::new(), target, legacy, &mut Vec::new())
                    .map_err(|e| match e {
                        icf::document::FileError::Image(e) => AppError::from(e),
                        icf::document::FileError::Output(e) => AppError::from(e),
                    })?;
                io::report(
                    &json!({"status":format!("{status:?}"),"published":false,"dirty":doc.dirty()}),
                )?;
                return Ok(0);
            }
            let mut output = Vec::new();
            let path = args
                .write
                .output
                .as_deref()
                .unwrap_or("")
                .as_bytes()
                .to_vec();
            let views = validation_views(views.as_deref())?;
            let source = views
                .as_deref()
                .map_or(ValidationSource::NoTree, ValidationSource::Views);
            let mut state = if show_both.is_some() || home_cache.is_some() {
                Some(reset_state(show_both, home_cache)?)
            } else {
                None
            };
            let report = doc
                .save_with_validation(
                    &mut session,
                    SaveRequest {
                        path,
                        target_revision: target,
                        legacy_choice: legacy,
                        warning_choice: warning.map(Choice::native),
                        validation: source,
                    },
                    state.as_mut(),
                    &mut output,
                )
                .map_err(|e| match e {
                    icf::document::FileError::Image(e) => AppError::from(e),
                    icf::document::FileError::Output(e) => AppError::from(e),
                })?;
            match report.status {
                SaveStatus::Saved => (
                    output,
                    "saved",
                    json!({"tree":views.is_some(),"validation":{"errors":report.validation.errors,"warnings":report.validation.warnings,"flags":report.validation.flags},"checksum_written":report.checksum_written,"correction":report.correction.as_ref().zip(state.as_ref()).map(|(e,s)|reset_effects(e,s))}),
                ),
                status => {
                    io::report(
                        &json!({"status":format!("{status:?}"),"published":false,"dirty":doc.dirty()}),
                    )?;
                    return Ok(0);
                }
            }
        }
        FileOperation::Normalize => {
            let report = doc.normalize(&session)?;
            doc.compare_dirty();
            (
                doc.current_bytes(&session)?,
                "normalized",
                json!({"typed_save_calls":report.typed.saved,"deleted_repeaters":report.deleted_repeaters}),
            )
        }
        FileOperation::Reset {
            show_both,
            home_cache,
        } => {
            let mut state = reset_state(show_both, home_cache)?;
            let report = doc.reset_operating_state(&session, &mut state)?;
            doc.compare_dirty();
            (
                doc.current_bytes(&session)?,
                "reset",
                reset_effects(&report, &state),
            )
        }
        FileOperation::Initial => unreachable!(),
        FileOperation::Open => (
            doc.current_bytes(&session)?,
            "opened",
            json!({"open_status":format!("{status:?}")}),
        ),
    };
    publish(&doc, &output, &args.write, label, &session, effects)
}

pub fn run(args: Args) -> Result<u8> {
    match args.action {
        Action::Initial(args) => file(args, FileOperation::Initial),
        Action::New(args) => new(args),
        Action::Open(args) => file(args, FileOperation::Open),
        Action::Normalize(args) => file(args, FileOperation::Normalize),
        Action::Save {
            file: args,
            target_revision,
            legacy_choice,
            settings,
            validation_views,
            warning_choice,
            show_both,
            home_cache,
        } => file(
            args,
            FileOperation::Save {
                target: target_revision,
                legacy: legacy_choice,
                settings,
                views: validation_views,
                warning: warning_choice,
                show_both,
                home_cache,
            },
        ),
        Action::Reset {
            file: args,
            show_both,
            home_cache,
        } => file(
            args,
            FileOperation::Reset {
                show_both,
                home_cache,
            },
        ),
    }
}
