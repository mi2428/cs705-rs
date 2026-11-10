//! General decoded-record adapters. The library owns all image/holder offsets.
//! This module accepts explicit decoded saver values, not a simulated GUI editor
//! or generated G context. Failed offline operations never publish a payload.

use crate::{
    AppError, Result,
    args::{RowAction, Sheet, Write},
    confirm, io, values,
};
use cs705::icf::{
    Icf, MemoryCallsign, normalizer,
    record::{self, ChannelField, DecodedRecord},
    record_image::ChannelKind,
};
use cs705::repeater::working::{DefaultContext, Working};
use serde_json::{Value, json};

/// Explicit runtime sources. Clap flattening belongs to the existing root
/// router. Default means sources ABSENT, never a synthesized native Q or G.
#[derive(clap::Args, Clone, Default)]
pub struct Options {
    /// Exact24-byte captured Q context; not an ICF or factory image.
    #[arg(long)]
    pub record_context: Option<String>,
    /// Actual SYSTEM LANGID; required only when Q selects the host branch.
    #[arg(long)]
    pub system_langid: Option<u16>,
    /// Completed expanded G generation for conditional native reference reset.
    #[arg(long)]
    pub generation: Option<String>,
    /// Actual captured T image for the UR reference-reset read; not repeater G.
    #[arg(long)]
    pub reference_defaults: Option<String>,
    /// Publish an actual in-memory prefix on a later native operation failure.
    #[arg(long)]
    pub accept_partial: bool,
    /// Inclusive selection end in the same coordinate system as INDEX.
    #[arg(long)]
    pub end_index: Option<usize>,
    /// Use side-view selection; memory indices become typed sides0..199.
    #[arg(long)]
    pub side_view: bool,
    /// Explicit captured show-both state used by native copy/paste/carry.
    #[arg(long,num_args=1,action=clap::ArgAction::Set)]
    pub show_both: Option<bool>,
    /// Captured native editor caches, pending name and completed B generation.
    #[arg(long)]
    pub normal_state: Option<String>,
    /// Actual unit formatter bytes for a decimal native key, KEY=HEX.
    #[arg(long, value_name = "KEY=HEX")]
    pub inline_unit_output: Vec<String>,
    /// Existing portable format/date-token context, shared without another parser.
    #[arg(long)]
    pub native_format_context: Option<String>,
    /// Explicit generation-Q/live-Q inputs for the one captured B owner.
    #[arg(long,conflicts_with_all=["normal_state","record_context","system_langid"])]
    pub native_range_context: Option<String>,
}
pub struct Runtime {
    context: Option<cs705::icf::context::Context>,
    system_langid: Option<u16>,
    generation: Option<Vec<u8>>,
    reference_defaults: Option<Vec<u8>>,
    accept_partial: bool,
    normal: std::cell::RefCell<Option<NormalState>>,
    action: std::cell::RefCell<Option<NativeAction>>,
    input_options: cs705::display::InputOptions,
    inline_units: std::collections::BTreeMap<u16, Vec<u8>>,
    format_context: Option<crate::csv::Context>,
    date_cursor: std::cell::Cell<usize>,
    inline_notifications: std::cell::RefCell<Vec<cs705::icf::rows::native::Notification>>,
}

/// Collector output for the parent's single Document::finish_action consumer.
/// It never grants publication, even when a cancelled gesture requests a timer.
#[derive(Debug, Clone, Copy)]
pub struct NativeAction {
    pub context: cs705::icf::document::ActionContext,
    pub flags: u32,
}
impl Runtime {
    pub(crate) fn inline_clock(&self) -> Result<cs705::csv::date_input::Clock> {
        self.display_format_context()?.native_clock()
    }
    pub(crate) fn with_date_input_runtime<T>(
        &self,
        mode: u8,
        edit: impl FnOnce(&mut cs705::csv::date_input::Runtime<'_>) -> Result<T>,
    ) -> Result<T> {
        if mode > 3 {
            return Err(AppError::invalid("native date input mode outside0..3"));
        }
        let context = self.display_format_context()?;
        let mut runtime = cs705::csv::date_input::Runtime {
            locale: context.datetime()?,
            time_format: context.native_time_format()?,
            // Mode3 does not read date_order; it must not require that source.
            date_order: if mode < 3 {
                context.native_date_order()?
            } else {
                u32::from(mode)
            },
            clock: context.optional_native_clock()?,
        };
        edit(&mut runtime)
    }
    pub(crate) fn inline_notification(&self, event: u32, synchronous: bool) -> Result<()> {
        self.inline_notifications
            .borrow_mut()
            .push(cs705::icf::rows::native::Notification {
                id: event,
                synchronous,
                arg1: 0,
                arg2: 0,
            });
        Ok(())
    }
    pub(crate) fn inline_decimal(&self) -> Result<u8> {
        self.display_format_context()?.byte("crt_decimal")
    }
    pub(crate) fn inline_date_order(&self) -> Result<u32> {
        self.display_format_context()?.native_date_order()
    }
    pub(crate) fn display_format_context(&self) -> Result<&crate::csv::Context> {
        self.format_context.as_ref().ok_or_else(|| {
            AppError::safety("native formatting requires explicit --native-format-context")
        })
    }
    pub(crate) fn display_native_date(
        &self,
        request: cs705::csv::datetime::DateToken,
    ) -> std::result::Result<Vec<u8>, cs705::csv::Error> {
        let context = self
            .format_context
            .as_ref()
            .ok_or(cs705::csv::Error::SourceUnavailable(
                "native format context unavailable",
            ))?;
        let mut cursor = self.date_cursor.get();
        let result = context.native_date(&mut cursor, request);
        self.date_cursor.set(cursor);
        result
    }
    pub(crate) fn inline_unit_output(&self, key: u16) -> Option<&[u8]> {
        self.inline_units.get(&key).map(Vec::as_slice)
    }
    pub(crate) fn generation(&self) -> Option<&[u8]> {
        self.generation.as_deref()
    }
    pub(crate) fn reference_defaults(&self) -> Option<&[u8]> {
        self.reference_defaults.as_deref()
    }
    pub(crate) fn with_captured_ranges<T>(
        &self,
        edit: impl FnOnce(&cs705::icf::frequency::Ranges) -> Result<T>,
    ) -> Result<T> {
        let captured = self.normal.borrow();
        let captured = captured.as_ref().ok_or_else(|| {
            AppError::safety(
                "native input requires --native-range-context or restored --normal-state B",
            )
        })?;
        edit(&captured.ranges)
    }
    pub fn display_input_options(&self) -> cs705::display::InputOptions {
        self.input_options
    }
    pub fn display_context(&self) -> Result<&cs705::icf::context::Context> {
        self.display_context_source().ok_or_else(|| {
            AppError::safety("native display condition requires explicit live --record-context Q")
        })
    }
    pub fn display_context_source(&self) -> Option<&cs705::icf::context::Context> {
        self.context.as_ref()
    }
    pub fn display_system_langid(&self) -> Option<u16> {
        self.system_langid
    }
    pub fn with_input_options(mut self, options: cs705::display::InputOptions) -> Self {
        self.input_options = options;
        self
    }
    pub fn take_native_action(&self) -> Option<NativeAction> {
        self.action.borrow_mut().take()
    }
    fn native_action_flags(&self, flags: u32) {
        if let Some(action) = self.action.borrow_mut().as_mut() {
            action.flags |= flags;
        }
    }
}
impl Options {
    pub fn normal_options_present(&self) -> bool {
        self.end_index.is_some() || self.side_view || self.show_both.is_some()
    }
    /// Call BEFORE reading any other command input. The entire stdin ownership
    /// set includes Q, G, the document and any clipboard/template source.
    pub fn load<'a>(&'a self, other_inputs: impl IntoIterator<Item = &'a str>) -> Result<Runtime> {
        io::single_stdin(
            other_inputs
                .into_iter()
                .chain(self.record_context.as_deref())
                .chain(self.generation.as_deref())
                .chain(self.reference_defaults.as_deref())
                .chain(self.normal_state.as_deref())
                .chain(self.native_format_context.as_deref())
                .chain(self.native_range_context.as_deref()),
        )?;
        if self.native_range_context.is_some()
            && (self.normal_state.is_some()
                || self.record_context.is_some()
                || self.system_langid.is_some())
        {
            return Err(AppError::invalid(
                "native-range-context is the single B/live-Q/LANGID source; do not combine it with normal-state/record-context/system-langid",
            ));
        }
        let mut inline_units = std::collections::BTreeMap::new();
        for source in &self.inline_unit_output {
            let (key, hex) = source
                .split_once('=')
                .ok_or_else(|| AppError::invalid("inline unit source requires decimal KEY=HEX"))?;
            let key = values::parse::<u16>(key)?;
            if inline_units.insert(key, io::decode_hex(hex)?).is_some() {
                return Err(AppError::invalid("duplicate native inline unit source key"));
            }
        }
        let mut context = self
            .record_context
            .as_deref()
            .map(|path| -> Result<_> {
                let bytes = io::read(path)?;
                let raw = bytes.try_into().map_err(|_| {
                    AppError::invalid("record context must contain exactly24 raw Q bytes")
                })?;
                let context = cs705::icf::context::Context::from_bytes(raw);
                context.selector()?;
                Ok(context)
            })
            .transpose()?;
        let mut system_langid = self.system_langid;
        let generation = self
            .generation
            .as_deref()
            .map(|path| {
                let bytes = io::read(path)?;
                if bytes.len() != cs705::clone::IMAGE_CAPACITY {
                    return Err(AppError::invalid(
                        "generation requires a completed expanded G image",
                    ));
                }
                Ok(bytes)
            })
            .transpose()?;
        let mut normal = self
            .normal_state
            .as_deref()
            .map(|path| parse_normal_state(&io::read(path)?))
            .transpose()?;
        if let Some(path) = &self.native_range_context {
            let captured = crate::native::read(path)?;
            let q =
                cs705::icf::context::Context::from_bytes(*captured.ranges.source_q().ok_or_else(
                    || AppError::invalid("native range producer did not capture generation Q"),
                )?);
            let selected = captured.ranges.selected();
            let source = captured
                .ranges
                .default_source_selector(captured.system_langid)?;
            context = Some(captured.live_q);
            system_langid = captured.system_langid;
            normal = Some(NormalState {
                q,
                selected,
                langid: captured.system_langid,
                source,
                ranges: captured.ranges,
                editor: cs705::icf::rows::native::EditorState::initialized(),
                notification: None,
            });
        }
        Ok(Runtime {
            context,
            system_langid,
            generation,
            reference_defaults: self
                .reference_defaults
                .as_deref()
                .map(io::read)
                .transpose()?,
            accept_partial: self.accept_partial,
            normal: std::cell::RefCell::new(normal),
            action: std::cell::RefCell::new(None),
            input_options: cs705::display::InputOptions::default(),
            inline_units,
            format_context: self
                .native_format_context
                .as_deref()
                .map(crate::csv::Context::read)
                .transpose()?,
            date_cursor: std::cell::Cell::new(0),
            inline_notifications: std::cell::RefCell::new(Vec::new()),
        })
    }
}

struct NormalState {
    q: cs705::icf::context::Context,
    selected: u32,
    langid: Option<u16>,
    source: usize,
    ranges: cs705::icf::frequency::Ranges,
    editor: cs705::icf::rows::native::EditorState,
    notification: Option<[u8; 24]>,
}
fn parse_normal_state(bytes: &[u8]) -> Result<NormalState> {
    use cs705::icf::rows::native::{EditorState, PendingName};
    use values::ValueCodec;
    let v: Value = serde_json::from_slice(bytes)
        .map_err(|_| AppError::invalid("invalid normal editor state JSON"))?;
    if !v.as_object().is_some_and(|o| {
        o.len() == 11
            && [
                "version",
                "schema",
                "range_context",
                "range_selected",
                "range_langid",
                "range_source",
                "mode_cache",
                "filter_cache",
                "filter_enabled",
                "pending",
                "notification",
            ]
            .iter()
            .all(|k| o.contains_key(*k))
    }) || v["version"] != 1
        || v["schema"] != "cs705.normal-editor"
    {
        return Err(AppError::invalid(
            "normal editor requires exact version1 cs705.normal-editor state",
        ));
    }
    let q = cs705::icf::context::Context::from_bytes(<[u8; 24]>::parse(&v["range_context"])?);
    let selected = u32::parse(&v["range_selected"])?;
    if selected >= 5 {
        return Err(AppError::invalid(
            "restored completed normal state requires range_selected0..4; create a new generation to resolve a host source",
        ));
    }
    let langid = if v["range_langid"].is_null() {
        None
    } else {
        Some(u16::parse(&v["range_langid"])?)
    };
    let source = usize::from(u8::parse(&v["range_source"])?);
    let mut ranges = cs705::icf::frequency::Ranges::unpopulated();
    ranges.regenerate(selected, &q, langid)?;
    let selected = ranges.selected();
    if source != ranges.default_source_selector(langid)? {
        return Err(AppError::invalid(
            "normal state source differs from captured B.selected",
        ));
    }
    let pending = &v["pending"];
    if !pending.as_object().is_some_and(|o| {
        o.len() == 3
            && ["active", "index", "bytes"]
                .iter()
                .all(|k| o.contains_key(*k))
    }) {
        return Err(AppError::invalid(
            "normal pending requires exact active/index/bytes",
        ));
    }
    let pending_bytes = pending["bytes"]
        .as_array()
        .filter(|b| b.len() <= 16)
        .ok_or_else(|| AppError::invalid("normal pending exceeds its accepted16-byte producer"))?
        .iter()
        .map(u8::parse)
        .collect::<Result<Vec<_>>>()?;
    Ok(NormalState {
        q,
        selected,
        langid,
        source,
        ranges,
        editor: EditorState {
            mode_cache: DecodedRecord::from_bytes(<[u8; 616]>::parse(&v["mode_cache"])?),
            filter_cache: DecodedRecord::from_bytes(<[u8; 616]>::parse(&v["filter_cache"])?),
            filter_enabled: u8::parse(&v["filter_enabled"])?,
            pending: PendingName {
                active: bool::parse(&pending["active"])?,
                index: u32::parse(&pending["index"])?,
                bytes: pending_bytes,
            },
        },
        notification: if v["notification"].is_null() {
            None
        } else {
            Some(<[u8; 24]>::parse(&v["notification"])?)
        },
    })
}
fn initialize_normal_state(text: &str) -> Result<NormalState> {
    use values::ValueCodec;
    let v: Value = serde_json::from_str(text).map_err(|_| {
        AppError::invalid("normal state initialization requires JSON scalar sources")
    })?;
    if !v.as_object().is_some_and(|o| {
        ["range_context", "range_selected", "range_langid"]
            .iter()
            .all(|k| o.contains_key(*k))
            && o.keys().all(|k| {
                [
                    "range_context",
                    "range_selected",
                    "range_langid",
                    "notification",
                ]
                .contains(&k.as_str())
            })
    }) {
        return Err(AppError::invalid(
            "normal state initialization requires range_context/range_selected/range_langid; notification is only a low-level captured cutpoint",
        ));
    }
    let q = cs705::icf::context::Context::from_bytes(<[u8; 24]>::parse(&v["range_context"])?);
    let selected = u32::parse(&v["range_selected"])?;
    let langid = if v["range_langid"].is_null() {
        None
    } else {
        Some(u16::parse(&v["range_langid"])?)
    };
    let mut ranges = cs705::icf::frequency::Ranges::unpopulated();
    ranges.regenerate(selected, &q, langid)?;
    let selected = ranges.selected();
    let source = ranges.default_source_selector(langid)?;
    Ok(NormalState {
        q,
        selected,
        langid,
        source,
        ranges,
        editor: cs705::icf::rows::native::EditorState::initialized(),
        notification: if v["notification"].is_null() {
            None
        } else {
            Some(<[u8; 24]>::parse(&v["notification"])?)
        },
    })
}
fn normal_state_value(s: &NormalState) -> Value {
    json!({"version":1,"schema":"cs705.normal-editor","range_context":s.q.as_bytes(),"range_selected":s.selected,"range_langid":s.langid,"range_source":s.source,
        "mode_cache":s.editor.mode_cache.as_bytes().as_slice(),"filter_cache":s.editor.filter_cache.as_bytes().as_slice(),"filter_enabled":s.editor.filter_enabled,
        "pending":{"active":s.editor.pending.active,"index":s.editor.pending.index,"bytes":s.editor.pending.bytes},"notification":s.notification})
}

const NATIVE_CHANNEL_FIELDS: [&str; 19] = [
    "name",
    "split",
    "frequency-hz",
    "mode",
    "data",
    "filter",
    "duplex",
    "offset-hz",
    "tone",
    "tx-tone",
    "rx-tone",
    "dtcs",
    "polarity",
    "digital-squelch",
    "digital-code",
    "your-call",
    "repeater1",
    "repeater2",
    "selection",
];

/// Commit a generated phase1 grid frame through the same canonical collectors.
/// A pending editor does not turn a selection-only cell into an accepted value.
pub fn native_inline_edit(
    image: &mut Icf<'_>,
    frame: &cs705::printing::native::InlineFrame,
    runtime: &Runtime,
) -> Result<bool> {
    let result = native_inline_dispatch(image, frame, runtime);
    {
        let requests = runtime.inline_notifications.borrow();
        if !requests.is_empty() {
            io::report(
                &json!({"native_inline_requests":requests.iter().map(|n|json!({"id":n.id,"synchronous":n.synchronous})).collect::<Vec<_>>(),
                "handler_returned_normally":result.is_ok(),"representation":"actual inline notification requests, including error prefixes; not OS delivery"}),
            )?;
        }
    }
    runtime.inline_notifications.borrow_mut().clear();
    result
}

fn native_inline_dispatch(
    image: &mut Icf<'_>,
    frame: &cs705::printing::native::InlineFrame,
    runtime: &Runtime,
) -> Result<bool> {
    use cs705::printing::native::Class;
    *runtime.action.borrow_mut() = None;
    if frame.phase != 1
        || frame.action != 0
        || frame.command != 0
        || frame.input_code != 13
        || !frame.grid4
        || !frame.grid8
    {
        return Err(AppError::invalid(
            "native inline dispatcher requires its generated phase1 text-commit frame",
        ));
    }
    let call = frame.factory.class == Class::Split && frame.factory.node == 103;
    let normal = frame.factory.class == Class::Split && (2..102).contains(&frame.factory.node);
    if !call && !normal {
        let (stored, flags) = crate::inline_fields::edit_non_channel(image, frame, runtime)?;
        let holder = matches!(
            frame.factory.class,
            Class::Gps | Class::Repeater | Class::Ur | Class::Allowed
        );
        *runtime.action.borrow_mut() = Some(NativeAction {
            context: cs705::icf::document::ActionContext {
                action: frame.action,
                phase: frame.command,
                state: frame.phase,
                callback_address: frame.callback,
            },
            flags: flags | 1 | if holder { 8 } else { 0 },
        });
        return Ok(stored);
    }
    if frame.node.group != if call { 4 } else { 3 } {
        return Err(AppError::invalid(
            "native inline resource context differs from its dataset",
        ));
    }
    let index = frame.node.typed_index.ok_or_else(|| {
        AppError::invalid("native inline channel requires its generated typed side index")
    })?;
    if normal && u32::try_from(frame.factory.group).ok() != Some(index / 200) {
        return Err(AppError::invalid(
            "native inline group differs from its generated typed index",
        ));
    }
    let name = frame
        .node
        .key
        .checked_sub(0x105)
        .and_then(|key| NATIVE_CHANNEL_FIELDS.get(key as usize))
        .ok_or_else(|| AppError::invalid("native inline channel key has no owning field"))?;
    let text = json!({"action":"text","hex":io::bytes_value(&frame.text)["hex"]});
    let flags = u8::from(frame.node.merge);
    let edit = match *name {
        "name" => {
            json!({"input":text,"layout":frame.layout,"node_flags":flags,"node_index":frame.node.index as u32})
        }
        "your-call" | "repeater1" | "repeater2" => json!({"input":text}),
        "frequency-hz" | "offset-hz" | "tone" | "tx-tone" | "rx-tone" | "dtcs" | "digital-code" => {
            let source = std::str::from_utf8(&frame.text)
                .ok()
                .filter(|s| s.is_ascii())
                .ok_or_else(|| AppError::invalid("native numeric inline text requires ASCII"))?;
            let mut incoming = record::reset_holder(if call { 3 } else { 2 }, index, 0);
            record::load_channel_image_into(
                image,
                if call {
                    ChannelKind::Call
                } else {
                    ChannelKind::Normal
                },
                index,
                &mut incoming,
            )?;
            let entry = cs705::icf::rows::native::channel_entry(
                &incoming,
                frame.node.key,
                cs705::icf::rows::native::ChannelCaller {
                    code: frame.action as i32,
                    command: frame.command,
                    blocked: false,
                    node_index: frame.node.index,
                    node_back: i32::try_from(frame.node.above).map_err(|_| {
                        AppError::invalid("native inline node back exceeds signed source")
                    })?,
                },
                runtime.input_options.callsign_dialog_enabled(),
            )?;
            let uses_decimal = entry == cs705::icf::rows::native::ChannelEntry::Edit
                && match *name {
                    "frequency-hz" | "offset-hz" => true,
                    "tx-tone" | "rx-tone" | "dtcs" => {
                        !incoming.valid() || matches!(incoming.mode(), 13 | 14)
                    }
                    _ => false,
                };
            let number = if uses_decimal {
                json!({"action":"text","text":source,"decimal":char::from(runtime.inline_decimal()?).to_string()})
            } else {
                json!(source)
            };
            if *name == "frequency-hz" {
                json!({"input":number,"warning_bits":frame.warning_bits,"focus":frame.focus})
            } else if *name == "digital-code" {
                json!({"input":source,"node_flags":flags})
            } else {
                number
            }
        }
        _ => {
            return Err(AppError::invalid(
                "selection-only channel cell has no phase1 text collector",
            ));
        }
    };
    let source=json!({"edit":edit,"caller":{"code":frame.action,"command":frame.command,"state":frame.phase,
        "blocked":false,"node_index":frame.node.index,"node_back":frame.node.above}}).to_string();
    let key = if call {
        format!("record/native-call/{index}/{name}")
    } else {
        format!(
            "record/native-memory/{}/{}/{name}",
            index / 200,
            index % 200
        )
    };
    let stored = if call {
        native_call_edit(image, &key, &source, runtime)?
    } else {
        native_memory_edit(image, &key, &source, runtime)?
    };
    let mut action = runtime.action.borrow_mut();
    let action = action
        .as_mut()
        .ok_or_else(|| AppError::invalid("native inline collector did not return its action"))?;
    action.context.callback_address = frame.callback;
    Ok(stored)
}

fn native_channel_entry(
    image: &mut Icf<'_>,
    key: &str,
    text: &str,
    runtime: &Runtime,
    kind: ChannelKind,
    edit: impl FnOnce(&mut Icf<'_>, &str, &str, &Runtime) -> Result<bool>,
) -> Result<bool> {
    use cs705::icf::rows::native::{ChannelCaller, ChannelEntry, channel_entry};
    use values::ValueCodec;
    *runtime.action.borrow_mut() = None;
    let prefix = if kind == ChannelKind::Normal {
        "record/native-memory/"
    } else {
        "record/native-call/"
    };
    let tail = key
        .strip_prefix(prefix)
        .ok_or_else(|| AppError::invalid("native channel path required"))?;
    if tail == "state" || tail == "reset-pair" || tail.starts_with("accepted/") {
        return edit(image, key, text, runtime);
    }
    let decoded = format!(
        "record/{}/{tail}",
        if kind == ChannelKind::Normal {
            "memory"
        } else {
            "call"
        }
    );
    let (_, index, name) =
        path(&decoded)?.ok_or_else(|| AppError::invalid("native channel field path required"))?;
    if kind == ChannelKind::Call && name == "selection" {
        return Err(AppError::invalid("call SEL is read-only"));
    }
    let Some(field) = NATIVE_CHANNEL_FIELDS
        .iter()
        .position(|field| *field == name)
    else {
        return edit(image, key, text, runtime);
    };
    let field = 0x105 + field as u32;
    let mut h = record::reset_holder(if kind == ChannelKind::Normal { 2 } else { 3 }, index, 0);
    record::load_channel_image_into(image, kind, index, &mut h)?;
    let value = serde_json::from_str::<Value>(text).ok();
    let envelope = value
        .as_ref()
        .filter(|v| v.get("caller").is_some() || v.get("edit").is_some());
    let mut source = None;
    let caller = if let Some(v) = envelope {
        if !v
            .as_object()
            .is_some_and(|o| o.len() == 2 && o.contains_key("edit") && o.contains_key("caller"))
        {
            return Err(AppError::invalid(
                "native action envelope requires only edit/caller",
            ));
        }
        let c = &v["caller"];
        if matches!(field, 0x10c..=0x110) && v["edit"].is_number() {
            return Err(AppError::invalid(
                "native numeric action envelope must retain lexical text, not a decoded JSON number",
            ));
        }
        let fields = [
            "code",
            "command",
            "state",
            "blocked",
            "node_index",
            "node_back",
        ];
        if !c
            .as_object()
            .is_some_and(|o| o.len() == fields.len() && fields.iter().all(|k| o.contains_key(*k)))
        {
            return Err(AppError::invalid(
                "caller requires code/command/state/blocked/node_index/node_back scalars",
            ));
        }
        source = Some(v["edit"].to_string());
        Some((
            ChannelCaller {
                code: i32::parse(&c["code"])?,
                command: u32::parse(&c["command"])?,
                blocked: bool::parse(&c["blocked"])?,
                node_index: i32::parse(&c["node_index"])?,
                node_back: i32::parse(&c["node_back"])?,
            },
            u32::parse(&c["state"])?,
        ))
    } else {
        None
    };
    let dialog = runtime.input_options.callsign_dialog_enabled();
    let unavailable = (!h.valid() || h.channel_field(ChannelField::FrequencyHz) > 470_000_000)
        && !matches!(field, 0x105 | 0x107)
        && !(dialog && matches!(field, 0x114..=0x116));
    if unavailable && caller.is_none() {
        return Err(AppError::safety(
            "empty/out-of-band channel input requires live caller scalars for frequency redirection",
        ));
    }
    let entry = if let Some((caller, state)) = caller {
        *runtime.action.borrow_mut() = Some(NativeAction {
            context: cs705::icf::document::ActionContext {
                action: caller.code as u32,
                phase: caller.command,
                state,
                ..Default::default()
            },
            flags: 9,
        });
        channel_entry(&h, field, caller, dialog)?
    } else {
        ChannelEntry::Edit
    };
    let result = match entry {
        ChannelEntry::Edit => edit(image, key, source.as_deref().unwrap_or(text), runtime),
        ChannelEntry::Blocked => {
            io::report(&json!({"stored":false,"blocked":true,"key":field}))?;
            Ok(false)
        }
        ChannelEntry::Redirect { key, index, code } => {
            io::report(
                &json!({"stored":false,"redirect":{"key":key,"index":index,"code":code},"representation":"native frequency move/replay request; original field editor skipped"}),
            )?;
            Ok(false)
        }
    };
    if result.is_err() {
        *runtime.action.borrow_mut() = None;
    }
    result
}

/// Native input router; accepted/ is an explicit separate low-level boundary.
/// The caller routes only record/native-memory paths here; no fallback follows.
pub fn native_memory_edit(
    image: &mut Icf<'_>,
    key: &str,
    text: &str,
    runtime: &Runtime,
) -> Result<bool> {
    native_channel_entry(
        image,
        key,
        text,
        runtime,
        ChannelKind::Normal,
        native_memory_field_edit,
    )
}

fn native_memory_field_edit(
    image: &mut Icf<'_>,
    key: &str,
    text: &str,
    runtime: &Runtime,
) -> Result<bool> {
    use cs705::icf::rows::{
        NormalMemory,
        native::{CacheWrite, EditorContext, Effects},
    };
    if key == "record/native-memory/state" {
        if runtime.normal.borrow().is_some() {
            return Err(AppError::invalid(
                "native memory state is already initialized or restored; constructor cannot replace it",
            ));
        }
        let state = initialize_normal_state(text)?;
        io::report(
            &json!({"normal_state":normal_state_value(&state),"representation":"native document constructor4C5CB..4C648; no GUI execution"}),
        )?;
        *runtime.normal.borrow_mut() = Some(state);
        return Ok(true);
    }
    let tail = key
        .strip_prefix("record/native-memory/")
        .ok_or_else(|| AppError::invalid("native memory path required"))?;
    let (accepted, tail) = tail
        .strip_prefix("accepted/")
        .map_or((false, tail), |tail| (true, tail));
    let decoded_path = format!("record/memory/{tail}");
    let (_, index, name) = path(&decoded_path)?
        .ok_or_else(|| AppError::invalid("native memory requires GROUP/typed-SIDE/FIELD"))?;
    if accepted && let Some(which) = call(name) {
        return native_channel_callsign_text_edit(
            image,
            ChannelKind::Normal,
            index,
            which,
            text,
            runtime,
            true,
        );
    }
    if accepted && !matches!(name, "name" | "frequency-hz" | "mode" | "data" | "filter") {
        return Err(AppError::invalid(
            "accepted cutpoint has no handler for this field",
        ));
    }
    if !accepted
        && matches!(
            name,
            "selection" | "duplex" | "polarity" | "digital-squelch"
        )
    {
        return native_memory_choice_edit(image, index, name, text, runtime);
    }
    if !accepted && name == "digital-code" {
        return native_memory_code_edit(image, index, text, runtime);
    }
    if !accepted && let Some(which) = call(name) {
        return native_channel_callsign_text_edit(
            image,
            ChannelKind::Normal,
            index,
            which,
            text,
            runtime,
            false,
        );
    }
    if !accepted && name == "name" {
        return native_memory_name_edit(image, index, text, runtime);
    }
    if !accepted && name == "frequency-hz" {
        return native_memory_frequency_edit(image, index, text, runtime);
    }
    if !accepted && name == "offset-hz" {
        return native_memory_offset_edit(image, index, text, runtime);
    }
    if !accepted && matches!(name, "tone" | "tx-tone" | "rx-tone" | "dtcs") {
        return native_memory_tone_edit(image, index, name, text, runtime);
    }
    if !accepted && matches!(name, "mode" | "data" | "filter") {
        return native_memory_enum_edit(image, index, name, text, runtime);
    }
    if !matches!(name, "name" | "frequency-hz" | "mode" | "data" | "filter") {
        return Err(AppError::invalid(
            "native memory field has no connected accepted editor handler",
        ));
    }
    let mut state = runtime.normal.borrow_mut();
    let captured = state.as_mut().ok_or_else(|| {
        AppError::safety(
            "initialize record/native-memory/state or restore --normal-state before a native edit",
        )
    })?;
    let mut model = NormalMemory::from_icf(image)?;
    let mut h = record::reset_holder(2, index, 0);
    cs705::icf::record_image::load_channel_into(
        model.working(),
        ChannelKind::Normal,
        index,
        &mut h,
    )?;
    let previous = h.clone();
    let first = !h.valid();
    let mut effects = Effects::default();
    let mut pending_request = None;
    let generation =
        normalizer::ReferenceImage::Separate(runtime.generation.as_deref().unwrap_or(&[]));
    let source = captured.source;
    let notification = captured.notification;
    let NormalState { ranges, editor, .. } = captured;
    let context = EditorContext { ranges, generation };
    let key = match name {
        "name" => 0x105,
        "frequency-hz" => 0x107,
        "mode" => 0x108,
        "data" => 0x109,
        _ => 0x10a,
    };
    if name == "name" {
        apply(&mut h, name, text)?;
        if first {
            let name = h.name().trim_ascii_end();
            if !name.is_empty() && model.used_slots() < 500 && notification.is_none() {
                return Err(AppError::safety(
                    "pending name requires its actual24-byte notification context",
                ));
            }
            if model.native_pending_name(&previous, name, &mut editor.pending)? {
                pending_request = notification;
            }
        }
    } else if name == "frequency-hz" {
        let candidate = values::parse::<u32>(text)?;
        let initial_index = ranges.edf0(2, candidate) as u32;
        if !model.native_prepare_registration(
            &mut h,
            candidate,
            (initial_index, source),
            context,
            &mut editor.pending,
        )? {
            io::report(
                &json!({"normal_state":normal_state_value(captured),"processed":false,"reason":"capacity"}),
            )?;
            return Ok(true);
        }
        editor.filter_enabled = 1;
        editor.filter_cache.set_valid(false);
        editor
            .filter_cache
            .set_header(2, index, editor.filter_cache.binding());
    } else {
        if !h.valid() {
            return Err(AppError::safety(
                "native field requires an existing row; name/frequency have separate first-entry handlers",
            ));
        }
        let candidate = values::parse::<u32>(text)?;
        if name == "mode" && candidate == 0x18
            || name == "filter" && matches!(h.mode() as u8, 0xf | 0x11)
            || name == "data" && !matches!(h.mode(), 0 | 1 | 8 | 13)
        {
            io::report(
                &json!({"processed":false,"representation":"rejected native candidate; no saver"}),
            )?;
            return Ok(true);
        }
        apply(&mut h, name, text)?;
    }
    if !(name == "name" && first) {
        model.native_save_cached(
            &mut h,
            editor,
            CacheWrite {
                key,
                previous: if matches!(key, 0x108 | 0x109) {
                    Some(&previous)
                } else {
                    None
                },
            },
            context,
            &mut effects,
            |_, _| Ok(()),
        )?;
        if name == "frequency-hz" && first {
            let mut other = record::reset_holder(2, index ^ 1, h.binding());
            other.copy_body_from(&h);
            model.store_channel(index ^ 1, &mut other)?;
        }
        image.patch_memory(&[(0, model.to_icf()?.memory())])?;
    }
    io::report(
        &json!({"normal_state":normal_state_value(captured),"effects":normal_effects(&effects),"requested_pending":pending_request.map(|payload|json!({"id":0x32,"payload":payload})),"requested_queue3c":matches!(name,"mode"|"frequency-hz"),"representation":"accepted native editor cutpoint; not OS dispatch"}),
    )?;
    Ok(true)
}
/// Native call field consumer. It borrows the real fixed call image directly;
/// normal allocation/maps and raw decoded setter fallback are never used here.
pub fn native_call_edit(
    image: &mut Icf<'_>,
    key: &str,
    text: &str,
    runtime: &Runtime,
) -> Result<bool> {
    native_channel_entry(
        image,
        key,
        text,
        runtime,
        ChannelKind::Call,
        native_call_field_edit,
    )
}

fn native_call_field_edit(
    image: &mut Icf<'_>,
    key: &str,
    text: &str,
    runtime: &Runtime,
) -> Result<bool> {
    use cs705::icf::rows::native::{
        self as n, ChannelChoice, ChannelEnum, ChoiceRequest, CodeRequest, EditorContext, Effects,
        EnumRequest, FrequencyRequest, NameEffect, NameRequest, OffsetRequest, SplitRequest,
        ToneEffect, ToneField, ToneInput, ToneRequest,
    };
    if key == "record/native-call/state" {
        return native_memory_field_edit(image, "record/native-memory/state", text, runtime);
    }
    if key == "record/native-call/reset-pair" {
        return native_call_reset(image, text, runtime);
    }
    let tail = key
        .strip_prefix("record/native-call/")
        .ok_or_else(|| AppError::invalid("native call path required"))?;
    let (accepted, tail) = tail
        .strip_prefix("accepted/")
        .map_or((false, tail), |tail| (true, tail));
    let decoded_path = format!("record/call/{tail}");
    let (_, index, name) = path(&decoded_path)?
        .ok_or_else(|| AppError::invalid("native call requires typed SIDE/FIELD"))?;
    if accepted && call(name).is_none() {
        return Err(AppError::invalid(
            "call accepted cutpoint only exposes direct callsign input",
        ));
    }
    if name == "selection" {
        return Err(AppError::invalid("call SEL is read-only"));
    }
    if let Some(which) = call(name) {
        return native_channel_callsign_text_edit(
            image,
            ChannelKind::Call,
            index,
            which,
            text,
            runtime,
            accepted,
        );
    }
    if !matches!(
        name,
        "duplex"
            | "polarity"
            | "digital-squelch"
            | "mode"
            | "data"
            | "filter"
            | "offset-hz"
            | "tone"
            | "tx-tone"
            | "rx-tone"
            | "dtcs"
            | "name"
            | "digital-code"
            | "frequency-hz"
            | "split"
    ) {
        return Err(AppError::invalid(
            "native call input family is not connected for this field",
        ));
    }
    let mut state = runtime.normal.borrow_mut();
    let captured=state.as_mut().ok_or_else(||AppError::safety("initialize the document editor state or restore --normal-state before native call input"))?;
    let mut w = image.memory().to_vec();
    let mut h = record::reset_holder(3, index, 0);
    cs705::icf::record_image::load_channel_into(&w, ChannelKind::Call, index, &mut h)?;
    let generation =
        normalizer::ReferenceImage::Separate(runtime.generation.as_deref().unwrap_or(&[]));
    let NormalState { ranges, editor, .. } = captured;
    let context = EditorContext { ranges, generation };
    let mut effects = Effects::default();
    let (authorized, mut report) = match name {
        "split" => {
            let (input, mut frame) = split_input(text)?;
            let out = n::channel_split_input_into(
                &mut w,
                &mut h,
                editor,
                SplitRequest {
                    input,
                    context,
                    frame: &mut frame,
                },
                &mut effects,
                |_, _| Ok(()),
                |_, _| Ok(()),
            )?;
            runtime.native_action_flags(frame.notify_flags);
            (
                out.stored,
                json!({"candidate":out.candidate,"popup_requested":out.popup_requested,"split_frame":split_frame_value(&frame),"choice_values":[0,1],"key":0x106}),
            )
        }
        "duplex" | "polarity" | "digital-squelch" => {
            let field = match name {
                "duplex" => ChannelChoice::Duplex,
                "polarity" => ChannelChoice::Polarity,
                _ => ChannelChoice::DigitalSquelch,
            };
            let choices = field.choices(&h);
            let out = n::call_choice_input_into(
                &mut w,
                &mut h,
                editor,
                ChoiceRequest {
                    field,
                    input: crate::fields::choice_input(text)?,
                    context,
                    selection: None,
                },
                &mut effects,
                |_, _| Ok(()),
            )?;
            (
                out.stored,
                json!({"candidate":out.candidate,"mode_blocked":out.mode_blocked,"popup_requested":out.popup_requested,"choice_values":choices,"key":field.key()}),
            )
        }
        "mode" | "data" | "filter" => {
            let field = match name {
                "mode" => ChannelEnum::Mode,
                "data" => ChannelEnum::Data,
                _ => ChannelEnum::Filter,
            };
            let (input, warning_bits) = native_enum_input(text, field)?;
            let choices = field.choices(&h);
            let out = n::call_enum_input_into(
                &mut w,
                &mut h,
                editor,
                EnumRequest {
                    field,
                    input,
                    context,
                    warning_bits,
                },
                &mut effects,
                |_, _| Ok(()),
            )?;
            (
                out.choice.stored,
                json!({"candidate":out.choice.candidate,"mode_blocked":out.choice.mode_blocked,"popup_requested":out.choice.popup_requested,"context_state":out.context_state,"warning_bits":out.warning_bits,"choice_values":choices,"key":field.key()}),
            )
        }
        "name" => {
            let (source, mut frame, _) = native_name_source(text)?;
            let mut effect = NameEffect::default();
            crate::repeaters::text_input(&source, |input| {
                Ok(n::call_name_input_into(
                    &mut w,
                    &mut h,
                    editor,
                    NameRequest {
                        input,
                        context,
                        frame: &mut frame,
                        notification: None,
                    },
                    &mut effect,
                    &mut effects,
                    |_, _| Ok(()),
                )?)
            })?;
            (
                effect.stored,
                json!({"state_out":effect.state_out,"display":io::bytes_value(&effect.display),
                "shared_input":io::bytes_value(&effect.shared_input),"name_frame":{"layout":frame.layout,"node_flags":frame.node_flags},"key":0x105}),
            )
        }
        "digital-code" => {
            let (source, mut flags, first) = native_code_source(text)?;
            let (out, _) = crate::fields::with_number_input(&source, |input, _| {
                Ok(n::call_code_input_into(
                    &mut w,
                    &mut h,
                    editor,
                    CodeRequest {
                        input,
                        first,
                        node_flags: &mut flags,
                        context,
                    },
                    &mut effects,
                    |_, _| Ok(()),
                )?)
            })?;
            (
                out.stored,
                json!({"candidate":out.candidate,"mode_blocked":out.mode_blocked,"state_out":out.state_out,
                "display":io::bytes_value(&out.display),"selection":out.selection,"code_node_flags":flags,"key":0x113}),
            )
        }
        "frequency-hz" => {
            let (source, warning_bits, focus) = native_frequency_source(text)?;
            let live_q = runtime.context.as_ref().ok_or_else(|| {
                AppError::safety("call frequency warnings require explicit live --record-context Q")
            })?;
            let (out, _) = crate::fields::with_number_input(&source, |input, point| {
                Ok(n::call_frequency_input_into(
                    &mut w,
                    &mut h,
                    editor,
                    FrequencyRequest {
                        input,
                        point,
                        context,
                        live_q,
                        warning_bits,
                        focus,
                    },
                    &mut effects,
                    |_, _| Ok(()),
                )?)
            })?;
            runtime.native_action_flags(out.notify_flags);
            (
                out.stored,
                json!({"first":{"candidate":out.first.candidate,"accepted":out.first.accepted,"state_out":out.first.state_out},
                "second":out.second.map(|s|json!({"candidate":s.candidate,"accepted":s.accepted,"state_out":s.state_out})),
                "notify_flags":out.notify_flags,"warning_bits":out.warning_bits,"focus":out.focus,"key":0x107}),
            )
        }
        "tone" | "tx-tone" | "rx-tone" | "dtcs" => {
            let field = match name {
                "tone" => ToneField::Tone,
                "tx-tone" => ToneField::Tx,
                "rx-tone" => ToneField::Rx,
                _ => ToneField::Dtcs,
            };
            let (source, inline, first) = native_tone_source(text)?;
            let mut effect = ToneEffect::default();
            crate::fields::with_number_input(&source, |input, point| {
                Ok(n::call_tone_input_into(
                    &mut w,
                    &mut h,
                    editor,
                    ToneRequest {
                        input: ToneInput {
                            field,
                            input,
                            point,
                            live_q: runtime.context.as_ref(),
                            inline,
                            first,
                            upper: &mut tone_ascii_upper,
                        },
                        context,
                    },
                    &mut effect,
                    &mut effects,
                    |_, _| Ok(()),
                )?)
            })?;
            (
                effect.stored,
                json!({"candidate":effect.candidate,"mode_blocked":effect.mode_blocked,"popup_requested":effect.popup_requested,
                "state_out":effect.state_out,"shared_input":io::bytes_value(&effect.shared_input),
                "inline_text":effect.inline_text.as_ref().map(|s|io::bytes_value(s)),"key":field.key()}),
            )
        }
        _ => {
            let (out, _) = crate::fields::with_number_input(text, |input, point| {
                Ok(n::call_offset_input_into(
                    &mut w,
                    &mut h,
                    editor,
                    OffsetRequest {
                        input,
                        point,
                        context,
                    },
                    &mut effects,
                    |_, _| Ok(()),
                )?)
            })?;
            (
                out.accepted,
                json!({"candidate":out.candidate,"state_out":out.state_out,"offset_hz":h.offset_hz(),"key":0x10c}),
            )
        }
    };
    if authorized {
        image.patch_memory(&[(0, &w)])?;
    }
    report["normal_state"] = normal_state_value(captured);
    report["effects"] = normal_effects(&effects);
    report["stored"] = json!(authorized);
    report["kind"] = json!(3);
    report["group"] = json!(h.group());
    report["index"] = json!(index);
    report["range_index"] = json!(h.range_index());
    report["representation"] =
        json!("native call input/shared field engine/cache; Home/DR effects are requests");
    io::report(&report)?;
    Ok(authorized)
}

/// Explicit O044 input, separate from the forbidden Call row clear operation.
fn native_call_reset(image: &mut Icf<'_>, text: &str, runtime: &Runtime) -> Result<bool> {
    use cs705::icf::rows::native::{Effects, call};
    use values::ValueCodec;
    let value: Value = serde_json::from_str(text)
        .map_err(|_| AppError::invalid("call reset requires selection/confirmation JSON"))?;
    if !value.as_object().is_some_and(|o| {
        o.len() == 2 && o.contains_key("selection") && o.contains_key("confirmation")
    }) {
        return Err(AppError::invalid(
            "call reset requires only selection/confirmation",
        ));
    }
    let s = &value["selection"];
    let fields = ["active", "first", "last", "merged", "show_both"];
    if !s
        .as_object()
        .is_some_and(|o| o.len() == fields.len() && fields.iter().all(|key| o.contains_key(*key)))
    {
        return Err(AppError::invalid(
            "call reset requires actual typed selection scalars",
        ));
    }
    let selection = call::Selection {
        active: usize::from(u8::parse(&s["active"])?),
        first: usize::from(u8::parse(&s["first"])?),
        last: usize::from(u8::parse(&s["last"])?),
        merged: bool::parse(&s["merged"])?,
        show_both: bool::parse(&s["show_both"])?,
    };
    let mut working = image.memory().to_vec();
    let mut effects = Effects::default();
    let result = call::reset_pair(
        &mut working,
        selection,
        normalizer::ReferenceImage::Separate(runtime.generation.as_deref().unwrap_or(&[])),
        i32::parse(&value["confirmation"])?,
        &mut effects,
        |_, _| Ok(()),
    );
    image.patch_memory(&[(0, &working)])?;
    io::report(
        &json!({"operation":"native-call-reset-pair","effects":normal_effects(&effects),
        "stored":result.as_ref().is_ok_and(|out|out.processed),"partial":result.is_err(),
        "representation":"native pair reset; notifications are requests, not OS delivery"}),
    )?;
    Ok(result?.processed)
}

/// BA04 Name/Call entry over logical UR rows, retaining insertion/cancel prefixes.
pub fn native_ur_edit(
    image: &mut Icf<'_>,
    key: &str,
    text: &str,
    runtime: &Runtime,
) -> Result<bool> {
    use cs705::icf::{
        rows::{WorkingUr, ur},
        settings::extended::{FixedTextInput, TextInput},
    };
    use values::ValueCodec;
    *runtime.action.borrow_mut() = None;
    let parts: Vec<_> = key.split('/').collect();
    let ["record", "native-ur", index, field] = parts.as_slice() else {
        return Err(AppError::invalid(
            "native UR path requires record/native-ur/INDEX/name|call",
        ));
    };
    let index = values::parse::<u32>(index)?;
    if index >= 300 {
        return Err(AppError::invalid("native UR index outside0..299"));
    }
    let field = match *field {
        "name" => ur::Field::Name,
        "call" => ur::Field::Call,
        _ => return Err(AppError::invalid("native UR field requires name or call")),
    };
    let value: Value = serde_json::from_str(text)
        .map_err(|_| AppError::invalid("native UR requires input/phase/alignment/frame JSON"))?;
    let fields = ["input", "phase", "alignment", "frame"];
    if !value
        .as_object()
        .is_some_and(|o| o.len() == fields.len() && fields.iter().all(|key| o.contains_key(*key)))
    {
        return Err(AppError::invalid(
            "native UR requires only input/phase/alignment/frame",
        ));
    }
    let f = &value["frame"];
    let fields = [
        "code",
        "command",
        "blocked",
        "view_last",
        "view_enabled",
        "node_flags",
        "warning_bits",
        "notify_flags",
        "ur_changed",
    ];
    if !f
        .as_object()
        .is_some_and(|o| o.len() == fields.len() && fields.iter().all(|key| o.contains_key(*key)))
    {
        return Err(AppError::invalid(
            "native UR requires all live caller/view/flag scalars",
        ));
    }
    let mut frame = ur::Frame {
        code: i32::parse(&f["code"])?,
        command: u32::parse(&f["command"])?,
        blocked: bool::parse(&f["blocked"])?,
        view_last: i32::parse(&f["view_last"])?,
        view_enabled: u32::parse(&f["view_enabled"])?,
        node_flags: u8::parse(&f["node_flags"])?,
        warning_bits: u8::parse(&f["warning_bits"])?,
        notify_flags: u32::parse(&f["notify_flags"])?,
        ur_changed: bool::parse(&f["ur_changed"])?,
    };
    let phase = u32::parse(&value["phase"])?;
    let alignment = u32::parse(&value["alignment"])?;
    let mut working = WorkingUr::from_icf(image)?;
    let mut effect = ur::Effect::default();
    let mut source = Vec::new();
    let result = crate::repeaters::text_input(&value["input"].to_string(), |input| {
        let input = match input {
            TextInput::Text(bytes) => {
                source.extend_from_slice(bytes);
                FixedTextInput::Text {
                    source: &mut source,
                    phase,
                }
            }
            TextInput::Begin(None) => FixedTextInput::Begin,
            TextInput::Begin(Some(_)) => {
                return Err(AppError::invalid(
                    "native UR Begin uses the live caller code, not a synthetic first byte",
                ));
            }
            TextInput::Display => FixedTextInput::Display,
            TextInput::Cancel => FixedTextInput::Cancel,
        };
        Ok(working.native_text(
            ur::Request {
                index,
                field,
                input,
                alignment,
                generation: normalizer::ReferenceImage::Separate(
                    runtime.reference_defaults.as_deref().unwrap_or(&[]),
                ),
            },
            &mut frame,
            &mut effect,
        )?)
    });
    // Insertion precedes text acceptance. Preserve it for a later accepted edit,
    // but it cannot grant publication by itself after cancellation or failure.
    image.patch_memory(&[(0, working.to_icf()?.memory())])?;
    io::report(
        &json!({"operation":"native-ur-input","index":index,"key":field.key(),
        "stored":effect.stored,"inserted":effect.inserted,"full_warning":effect.full_warning,
        "blocked":effect.blocked,"view_bad":effect.view_bad,"redirect":effect.redirect,
        "state_out":effect.state_out,"display":effect.display.as_ref().map(|s|io::bytes_value(s)),
        "shared_input":io::bytes_value(&source),"requested_notifications":effect.notifications,
        "frame":{"code":frame.code,"command":frame.command,"blocked":frame.blocked,
            "view_last":frame.view_last,"view_enabled":frame.view_enabled,"node_flags":frame.node_flags,
            "warning_bits":frame.warning_bits,"notify_flags":frame.notify_flags,"ur_changed":frame.ur_changed},
        "partial":result.is_err(),"representation":"native UR input; notifications are requests, not OS delivery"}),
    )?;
    result?;
    *runtime.action.borrow_mut() = Some(NativeAction {
        context: cs705::icf::document::ActionContext {
            action: frame.code as u32,
            phase: frame.command,
            state: phase,
            ..Default::default()
        },
        flags: frame.notify_flags | 9,
    });
    Ok(effect.stored)
}

/// Canonical C38 comes from InputOptions; accepted/ is an explicit raw boundary.
fn native_channel_callsign_text_edit(
    image: &mut Icf<'_>,
    kind: ChannelKind,
    index: u32,
    field: MemoryCallsign,
    text: &str,
    runtime: &Runtime,
    lowlevel: bool,
) -> Result<bool> {
    use cs705::icf::rows::native::{self as n, CallsignTextRequest, EditorContext, Effects};
    use cs705::icf::settings::extended::TextInput;
    use values::ValueCodec;
    let v: Value = serde_json::from_str(text)
        .map_err(|_| AppError::invalid("UR/RPT input envelope requires JSON"))?;
    if !v.as_object().is_some_and(|o| {
        o.contains_key("input")
            && o.keys().all(|k| {
                ["input", "right_word", "upper", "modal", "list", "reads"].contains(&k.as_str())
            })
    }) {
        return Err(AppError::invalid(
            "UR/RPT requires input with optional direct right_word/upper or actual modal/list/reads gestures",
        ));
    }
    let (begin, typed) = crate::repeaters::text_input(&v["input"].to_string(), |input| {
        Ok((
            matches!(input, TextInput::Begin(_)),
            matches!(input, TextInput::Text(_)),
        ))
    })?;
    if begin {
        if lowlevel {
            return Err(AppError::invalid(
                "accepted callsign cutpoint has no modal/list branch",
            ));
        }
        return native_channel_callsign_modal_edit(image, kind, index, field, &v, runtime);
    }
    if ["modal", "list", "reads"]
        .iter()
        .any(|k| v.get(*k).is_some())
    {
        return Err(AppError::invalid(
            "modal/list sources require an actual Begin gesture",
        ));
    }
    if !lowlevel && v.get("right_word").is_some() {
        return Err(AppError::invalid(
            "canonical callsign input reads auto-correct; right_word belongs to accepted/ direct input",
        ));
    }
    if typed
        && !lowlevel
        && runtime
            .action
            .borrow()
            .as_ref()
            .is_some_and(|action| action.context.state != 1)
    {
        return Err(AppError::invalid(
            "canonical callsign text requires the actual phase1 caller state",
        ));
    }
    let explicit_right_word = v.get("right_word").map(bool::parse).transpose()?;
    let right_word = if !typed {
        false
    } else if lowlevel {
        explicit_right_word.ok_or_else(|| {
            AppError::invalid("accepted direct callsign input requires right_word")
        })?
    } else {
        runtime.input_options.auto_correct_enabled()
    };
    let upper = v
        .get("upper")
        .map(|value| {
            value
                .as_array()
                .filter(|a| a.len() <= 8)
                .ok_or_else(|| {
                    AppError::invalid("upper requires at most8 signed source/full-EAX pairs")
                })?
                .iter()
                .map(<[i32; 2]>::parse)
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?;
    let mut state = runtime.normal.borrow_mut();
    let captured = state.as_mut().ok_or_else(|| {
        AppError::safety("initialize or restore native channel state before UR/RPT input")
    })?;
    let mut w = if kind == ChannelKind::Normal {
        cs705::icf::rows::NormalMemory::from_icf(image)?
            .working()
            .to_vec()
    } else {
        image.memory().to_vec()
    };
    let code = if kind == ChannelKind::Normal { 2 } else { 3 };
    let mut h = record::reset_holder(code, index, 0);
    cs705::icf::record_image::load_channel_into(&w, kind, index, &mut h)?;
    let generation =
        normalizer::ReferenceImage::Separate(runtime.generation.as_deref().unwrap_or(&[]));
    let NormalState { ranges, editor, .. } = captured;
    let mut effects = Effects::default();
    let stored = crate::repeaters::text_input(&v["input"].to_string(), |input| {
        let count = if let TextInput::Text(bytes) = input {
            bytes.len().min(8)
        } else {
            0
        };
        if upper.as_ref().is_some_and(|calls| calls.len() != count) {
            return Err(AppError::invalid(
                "upper must match exactly the consumed logical input bytes",
            ));
        }
        let mut at = 0;
        let mut native_upper = |source: i32| {
            let error = |reason| cs705::icf::Error::Invalid { line: 5, reason };
            if let Some(calls) = &upper {
                let [expected, result] = calls[at];
                at += 1;
                if expected != source {
                    return Err(error("captured CRT upper signed source mismatch"));
                }
                Ok(result)
            } else if source == -1 {
                Ok(-1)
            } else if (0..=127).contains(&source) {
                Ok(i32::from((source as u8).to_ascii_uppercase()))
            } else {
                Err(error("actual non-ASCII CRT upper full-EAX output required"))
            }
        };
        Ok(n::channel_callsign_text_input_into(
            &mut w,
            &mut h,
            editor,
            CallsignTextRequest {
                field,
                input,
                right_word,
                upper: &mut native_upper,
                context: EditorContext { ranges, generation },
            },
            &mut effects,
            |_, _| Ok(()),
        )?)
    })?;
    if stored {
        if kind == ChannelKind::Normal {
            let model = cs705::icf::rows::NormalMemory::from_working(w)?;
            image.patch_memory(&[(0, model.to_icf()?.memory())])?;
        } else {
            image.patch_memory(&[(0, &w)])?;
        }
    }
    let mut effects_value = normal_effects(&effects);
    effects_value["dr_invalidated"] = json!(effects.dr_invalidated);
    io::report(
        &json!({"normal_state":normal_state_value(captured),"stored":stored,"effects":effects_value,"kind":code,
        "group":h.group(),"index":index,"range_index":h.range_index(),"callsign":io::bytes_value(h.callsign(field)),
        "representation":"native UR/RPT direct-text collector; modal/list use the same cache and saver"}),
    )?;
    Ok(stored)
}

fn native_list_selection(value: &Value) -> Result<cs705::icf::callsign_dialog::ListSelection> {
    use values::ValueCodec;
    let fields = ["category", "name", "call", "modal_result", "checkbox"];
    if !value
        .as_object()
        .is_some_and(|o| o.len() == fields.len() && fields.iter().all(|k| o.contains_key(*k)))
    {
        return Err(AppError::invalid(
            "list selection requires category/name/call/modal_result/checkbox",
        ));
    }
    Ok(cs705::icf::callsign_dialog::ListSelection {
        category: i32::parse(&value["category"])?,
        name: i32::parse(&value["name"])?,
        call: i32::parse(&value["call"])?,
        modal_result: u32::parse(&value["modal_result"])?,
        checkbox: bool::parse(&value["checkbox"])?,
    })
}

enum NativeDialogAction {
    Text(u8, Vec<u8>),
    SourceList(cs705::icf::callsign_dialog::ListSelection),
    UrList(cs705::icf::callsign_dialog::ListSelection),
    Designation(i32, bool),
    Mode(u32),
}
fn native_dialog_action(value: &Value) -> Result<NativeDialogAction> {
    use values::ValueCodec;
    let kind = value["action"]
        .as_str()
        .ok_or_else(|| AppError::invalid("modal action name required"))?;
    let fields: &[&str] = match kind {
        "source-text" | "ur-text" | "repeater-text" => &["action", "input"],
        "source-list" | "ur-list" => &["action", "selection"],
        "designation" => &["action", "selected", "accept"],
        "mode" => &["action", "value"],
        _ => return Err(AppError::invalid("unknown DR modal action")),
    };
    if !value
        .as_object()
        .is_some_and(|o| o.len() == fields.len() && fields.iter().all(|k| o.contains_key(*k)))
    {
        return Err(AppError::invalid(
            "unexpected or missing DR modal action source",
        ));
    }
    Ok(match kind {
        "source-text" | "ur-text" | "repeater-text" => {
            let bytes = crate::repeaters::text_input(&value["input"].to_string(), |input| {
                if let cs705::icf::settings::extended::TextInput::Text(bytes) = input {
                    Ok(bytes.to_vec())
                } else {
                    Err(AppError::invalid(
                        "DR text action requires actual text/hex input",
                    ))
                }
            })?;
            NativeDialogAction::Text(
                if kind == "source-text" {
                    0
                } else if kind == "ur-text" {
                    1
                } else {
                    2
                },
                bytes,
            )
        }
        "source-list" => {
            NativeDialogAction::SourceList(native_list_selection(&value["selection"])?)
        }
        "ur-list" => NativeDialogAction::UrList(native_list_selection(&value["selection"])?),
        "designation" => NativeDialogAction::Designation(
            i32::parse(&value["selected"])?,
            bool::parse(&value["accept"])?,
        ),
        _ => NativeDialogAction::Mode(u32::parse(&value["value"])?),
    })
}

fn native_channel_callsign_modal_edit(
    image: &mut Icf<'_>,
    kind: ChannelKind,
    index: u32,
    field: MemoryCallsign,
    value: &Value,
    runtime: &Runtime,
) -> Result<bool> {
    use cs705::icf::{
        callsign_dialog::{self as d, Action},
        rows::{
            self,
            native::{self as n, CallsignModal, CallsignModalRequest, EditorContext, Effects},
        },
    };
    use values::ValueCodec;
    if value.get("upper").is_some() || value.get("right_word").is_some() {
        return Err(AppError::invalid(
            "direct CRT upper/right_word sources do not describe a modal gesture",
        ));
    }
    let action = runtime.action.borrow().as_ref().copied().ok_or_else(|| {
        AppError::safety("callsign Begin requires actual caller code/command/state scalars")
    })?;
    let mut incoming =
        record::reset_holder(if kind == ChannelKind::Normal { 2 } else { 3 }, index, 0);
    record::load_channel_image_into(image, kind, index, &mut incoming)?;
    let dialog_enabled = runtime.input_options.callsign_dialog_enabled();
    let dr = match action.context.phase {
        0xd1 => false,
        0xd2 => true,
        _ if dialog_enabled && (action.context.action == 13 || !incoming.valid()) => true,
        _ => {
            if ["modal", "list", "reads"]
                .iter()
                .any(|k| value.get(*k).is_some())
            {
                return Err(AppError::invalid(
                    "this ordinary Begin gesture requests inline editing, not a modal/list result",
                ));
            }
            let out = n::callsign_inline(&incoming, field, action.context.action as i32)?;
            io::report(
                &json!({"stored":false,"state_out":out.state_out,"inline_text":io::bytes_value(&out.display),
                "cursor":out.cursor,"selection":out.selection,"width":out.width,"kind":incoming.kind(),"index":index,
                "representation":"actual UR/RPT inline editor request; no saver or OS editor delivery"}),
            )?;
            return Ok(false);
        }
    };
    let mut actions = Vec::new();
    let mut checkbox = false;
    let modal = if dr {
        if value.get("list").is_some() {
            return Err(AppError::invalid(
                "DR option requires modal actions, not a generic list result",
            ));
        }
        let modal = &value["modal"];
        let fields = ["modal_result", "checkbox", "actions"];
        if !modal
            .as_object()
            .is_some_and(|o| o.len() == fields.len() && fields.iter().all(|k| o.contains_key(*k)))
        {
            return Err(AppError::invalid(
                "DR modal requires modal_result/checkbox/actions",
            ));
        }
        actions = modal["actions"]
            .as_array()
            .ok_or_else(|| AppError::invalid("DR modal actions must be an array"))?
            .iter()
            .map(native_dialog_action)
            .collect::<Result<_>>()?;
        checkbox = bool::parse(&modal["checkbox"])?;
        CallsignModal::Dr {
            modal_result: u32::parse(&modal["modal_result"])?,
        }
    } else {
        if value.get("modal").is_some() {
            return Err(AppError::invalid(
                "generic list mode does not accept DR actions",
            ));
        }
        CallsignModal::List(native_list_selection(&value["list"])?)
    };
    let reads_value = value.get("reads");
    if let Some(reads) = reads_value
        && !reads.as_object().is_some_and(|o| {
            o.keys().all(|k| {
                [
                    "ur_name_tail",
                    "blank_source_index",
                    "string_tail",
                    "initial_first_marker",
                ]
                .contains(&k.as_str())
            })
        })
    {
        return Err(AppError::invalid(
            "modal reads accept only actual ur_name_tail/blank_source_index/string_tail/initial_first_marker",
        ));
    }
    let tail = reads_value
        .and_then(|v| v.get("string_tail"))
        .map(|v| {
            v.as_array()
                .ok_or_else(|| AppError::invalid("string_tail requires actual bytes"))?
                .iter()
                .map(u8::parse)
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let reads = d::ReadInputs {
        initial_first_marker: reads_value
            .and_then(|v| v.get("initial_first_marker"))
            .map(i32::parse)
            .transpose()?,
        ur_name_tail: reads_value
            .and_then(|v| v.get("ur_name_tail"))
            .map(<[u8; 8]>::parse)
            .transpose()?,
        blank_source_index: reads_value
            .and_then(|v| v.get("blank_source_index"))
            .map(u32::parse)
            .transpose()?,
        string_tail: &tail,
    };
    let language = runtime.input_options.gui_language.ok_or_else(|| {
        AppError::safety("modal sources require explicit GUI language in --config")
    })?;
    let mut state = runtime.normal.borrow_mut();
    let captured = state.as_mut().ok_or_else(|| {
        AppError::safety("initialize or restore channel state before callsign modal input")
    })?;
    let mut w = if kind == ChannelKind::Normal {
        rows::NormalMemory::from_icf(image)?.working().to_vec()
    } else {
        image.memory().to_vec()
    };
    let code = if kind == ChannelKind::Normal { 2 } else { 3 };
    let mut h = record::reset_holder(code, index, 0);
    cs705::icf::record_image::load_channel_into(&w, kind, index, &mut h)?;
    let mut current = w.clone();
    current.resize(cs705::clone::IMAGE_CAPACITY, 0);
    cs705::repeater::working::expand_counts(&mut current)?;
    cs705::repeater::working::expand_rows(&mut current, runtime.defaults()?)?;
    rows::expand_ur_count(&mut current)?;
    rows::expand_ur_data(&mut current)?;
    let sources = d::Sources {
        current: &current,
        defaults: runtime.reference_defaults.as_deref().unwrap_or(&[]),
        ranges: &captured.ranges,
        system_langid: runtime.system_langid,
        language,
    };
    let context = EditorContext {
        ranges: &captured.ranges,
        generation: normalizer::ReferenceImage::Separate(
            runtime.generation.as_deref().unwrap_or(&[]),
        ),
    };
    let mut effects = Effects::default();
    let mut preview = Value::Null;
    let out = n::channel_callsign_modal_input_into(
        &mut w,
        &mut h,
        &mut captured.editor,
        CallsignModalRequest {
            field,
            modal,
            sources: &sources,
            reads: &reads,
            context,
        },
        &mut effects,
        |dialog| {
            for action in &actions {
                let action = match action {
                    NativeDialogAction::Text(0, bytes) => Action::SourceText(bytes),
                    NativeDialogAction::Text(1, bytes) => Action::UrText(bytes),
                    NativeDialogAction::Text(_, bytes) => Action::RepeaterText(bytes),
                    NativeDialogAction::SourceList(selection) => Action::SourceList(*selection),
                    NativeDialogAction::UrList(selection) => Action::UrList(*selection),
                    NativeDialogAction::Designation(selected, accept) => Action::Designation {
                        selected: *selected,
                        accept: *accept,
                    },
                    NativeDialogAction::Mode(mode) => Action::Mode(*mode),
                };
                dialog.action(&sources, action, &reads)?;
            }
            dialog.checkbox = checkbox;
            preview = json!({"callsigns":dialog.preview.iter().map(|s|io::bytes_value(s)).collect::<Vec<_>>(),
            "mode":dialog.mode,"source_class":dialog.source_class,"your_class":dialog.your_class,
            "designation":dialog.selected,"designation_candidates":dialog.candidates.iter().map(|s|io::bytes_value(s)).collect::<Vec<_>>()});
            Ok(())
        },
        |_, _| Ok(()),
    )?;
    runtime.native_action_flags(if out.al { 0x10 } else { 0 });
    if out.stored {
        if kind == ChannelKind::Normal {
            let model = rows::NormalMemory::from_working(w)?;
            image.patch_memory(&[(0, model.to_icf()?.memory())])?;
        } else {
            image.patch_memory(&[(0, &w)])?;
        }
    }
    io::report(
        &json!({"normal_state":normal_state_value(captured),"effects":normal_effects(&effects),"stored":out.stored,
        "accepted":out.accepted,"al":out.al,"checkbox":out.checkbox,"preview":preview,"kind":code,"index":index,
        "initial_selection":out.initial_selection.map(|s|json!({"category":s.category,"name":s.name,"call":s.call})),
        "callsigns":([MemoryCallsign::Your,MemoryCallsign::Repeater1,MemoryCallsign::Repeater2].map(|f|io::bytes_value(h.callsign(f)))),
        "representation":"actual current-list/DR modal source gestures; existing ordered saver; no OS dialog delivery"}),
    )?;
    Ok(out.stored)
}

fn native_enum_input(
    text: &str,
    field: cs705::icf::rows::native::ChannelEnum,
) -> Result<(cs705::icf::settings::extended::ChoiceInput, Option<u8>)> {
    use cs705::icf::rows::native::ChannelEnum;
    use values::ValueCodec;
    if field == ChannelEnum::Mode {
        let v: Value = serde_json::from_str(text)
            .map_err(|_| AppError::invalid("mode requires input/warning_bits JSON"))?;
        if !v.as_object().is_some_and(|o| {
            o.len() == 2 && o.contains_key("input") && o.contains_key("warning_bits")
        }) {
            return Err(AppError::invalid(
                "mode requires only input and actual warning_bits",
            ));
        }
        Ok((
            crate::fields::choice_input(&v["input"].to_string())?,
            Some(u8::parse(&v["warning_bits"])?),
        ))
    } else {
        Ok((crate::fields::choice_input(text)?, None))
    }
}

fn native_memory_enum_edit(
    image: &mut Icf<'_>,
    index: u32,
    name: &str,
    text: &str,
    runtime: &Runtime,
) -> Result<bool> {
    use cs705::icf::rows::{
        NormalMemory,
        native::{ChannelEnum, EditorContext, Effects, EnumRequest},
    };
    let field = match name {
        "mode" => ChannelEnum::Mode,
        "data" => ChannelEnum::Data,
        "filter" => ChannelEnum::Filter,
        _ => return Err(AppError::invalid("unknown native enum field")),
    };
    let (input, warning_bits) = native_enum_input(text, field)?;
    let mut state = runtime.normal.borrow_mut();
    let captured = state
        .as_mut()
        .ok_or_else(|| AppError::safety("initialize or restore normal state before enum input"))?;
    let mut model = NormalMemory::from_icf(image)?;
    let mut h = record::reset_holder(2, index, 0);
    cs705::icf::record_image::load_channel_into(
        model.working(),
        ChannelKind::Normal,
        index,
        &mut h,
    )?;
    let choices = field.choices(&h);
    let generation =
        normalizer::ReferenceImage::Separate(runtime.generation.as_deref().unwrap_or(&[]));
    let NormalState { ranges, editor, .. } = captured;
    let mut effects = Effects::default();
    let out = model.native_enum_input(
        &mut h,
        editor,
        EnumRequest {
            field,
            input,
            context: EditorContext { ranges, generation },
            warning_bits,
        },
        &mut effects,
        |_, _| Ok(()),
    )?;
    if out.choice.stored {
        image.patch_memory(&[(0, model.to_icf()?.memory())])?;
    }
    io::report(
        &json!({"normal_state":normal_state_value(captured),"effects":normal_effects(&effects),"stored":out.choice.stored,"candidate":out.choice.candidate,
        "mode_blocked":out.choice.mode_blocked,"popup_requested":out.choice.popup_requested,"context_state":out.context_state,"warning_bits":out.warning_bits,
        "enum_key":field.key(),"choice_values":choices,"representation":"native mode/DATA/filter input; shared predicates/choice/cache/codec; OS delivery separate"}),
    )?;
    Ok(out.choice.stored)
}

fn native_number_source(value: &Value) -> Result<String> {
    if !value.is_string() && !value.is_object() {
        return Err(AppError::invalid(
            "native number input must retain its text or action object, not a decoded JSON scalar",
        ));
    }
    Ok(value.to_string())
}

fn native_tone_source(text: &str) -> Result<(String, bool, Option<u8>)> {
    use values::ValueCodec;
    Ok(if values::json_view(text).starts_with('{') {
        let v: Value = serde_json::from_str(text)
            .map_err(|_| AppError::invalid("invalid tone action/envelope JSON"))?;
        if v.get("input").is_some() {
            if !v.as_object().is_some_and(|o| {
                o.keys()
                    .all(|k| ["input", "inline", "first"].contains(&k.as_str()))
            }) {
                return Err(AppError::invalid(
                    "tone envelope accepts only input/inline/first",
                ));
            }
            (
                native_number_source(&v["input"])?,
                v.get("inline")
                    .map(bool::parse)
                    .transpose()?
                    .unwrap_or(false),
                v.get("first").map(u8::parse).transpose()?,
            )
        } else {
            (text.to_owned(), false, None)
        }
    } else {
        (text.to_owned(), false, None)
    })
}

fn tone_ascii_upper(bytes: &mut [u8]) -> std::result::Result<(), cs705::csv::Error> {
    if !bytes.is_ascii() {
        return Err(cs705::csv::Error::SourceUnavailable(
            "actual non-ASCII tone uppercase output required",
        ));
    }
    bytes.make_ascii_uppercase();
    Ok(())
}

fn native_memory_tone_edit(
    image: &mut Icf<'_>,
    index: u32,
    name: &str,
    text: &str,
    runtime: &Runtime,
) -> Result<bool> {
    use cs705::icf::rows::{
        NormalMemory,
        native::{EditorContext, Effects, ToneEffect, ToneField, ToneInput, ToneRequest},
    };
    let field = match name {
        "tone" => ToneField::Tone,
        "tx-tone" => ToneField::Tx,
        "rx-tone" => ToneField::Rx,
        "dtcs" => ToneField::Dtcs,
        _ => return Err(AppError::invalid("unknown native tone field")),
    };
    let (source, inline, first) = native_tone_source(text)?;
    let mut state = runtime.normal.borrow_mut();
    let captured = state
        .as_mut()
        .ok_or_else(|| AppError::safety("initialize or restore normal state before tone input"))?;
    let mut model = NormalMemory::from_icf(image)?;
    let mut h = record::reset_holder(2, index, 0);
    cs705::icf::record_image::load_channel_into(
        model.working(),
        ChannelKind::Normal,
        index,
        &mut h,
    )?;
    let generation =
        normalizer::ReferenceImage::Separate(runtime.generation.as_deref().unwrap_or(&[]));
    let NormalState { ranges, editor, .. } = captured;
    let mut effect = ToneEffect::default();
    let mut effects = Effects::default();
    crate::fields::with_number_input(&source, |input, point| {
        Ok(model.native_tone_input(
            &mut h,
            editor,
            ToneRequest {
                input: ToneInput {
                    field,
                    input,
                    point,
                    live_q: runtime.context.as_ref(),
                    inline,
                    first,
                    upper: &mut tone_ascii_upper,
                },
                context: EditorContext { ranges, generation },
            },
            &mut effect,
            &mut effects,
            |_, _| Ok(()),
        )?)
    })?;
    if effect.stored {
        image.patch_memory(&[(0, model.to_icf()?.memory())])?;
    }
    io::report(
        &json!({"normal_state":normal_state_value(captured),"effects":normal_effects(&effects),"stored":effect.stored,"candidate":effect.candidate,
        "mode_blocked":effect.mode_blocked,"popup_requested":effect.popup_requested,"state_out":effect.state_out,"shared_input":io::bytes_value(&effect.shared_input),
        "inline_text":effect.inline_text.as_ref().map(|t|io::bytes_value(t)),"tone_key":field.key(),
        "representation":"native A5EDC input with generated source/resource order; effects are requests"}),
    )?;
    Ok(effect.stored)
}

fn native_name_source(
    text: &str,
) -> Result<(String, cs705::icf::rows::native::ChoiceFrame, Option<u32>)> {
    use cs705::icf::rows::native::ChoiceFrame;
    use values::ValueCodec;
    let v: Value = serde_json::from_str(text)
        .map_err(|_| AppError::invalid("name requires input/layout/node_flags JSON"))?;
    if !v.as_object().is_some_and(|o| {
        ["input", "layout", "node_flags"]
            .iter()
            .all(|k| o.contains_key(*k))
            && o.keys()
                .all(|k| ["input", "layout", "node_flags", "node_index"].contains(&k.as_str()))
    }) {
        return Err(AppError::invalid(
            "name requires input/layout/node_flags and optional live node_index for pending notification",
        ));
    }
    Ok((
        v["input"].to_string(),
        ChoiceFrame {
            layout: u32::parse(&v["layout"])?,
            node_flags: u8::parse(&v["node_flags"])?,
        },
        v.get("node_index").map(u32::parse).transpose()?,
    ))
}

fn native_memory_name_edit(
    image: &mut Icf<'_>,
    index: u32,
    text: &str,
    runtime: &Runtime,
) -> Result<bool> {
    use cs705::icf::rows::{
        NormalMemory,
        native::{EditorContext, Effects, NameEffect, NameRequest},
    };
    let (source, mut frame, node_index) = native_name_source(text)?;
    let mut state = runtime.normal.borrow_mut();
    let captured = state
        .as_mut()
        .ok_or_else(|| AppError::safety("initialize or restore normal state before name input"))?;
    let mut model = NormalMemory::from_icf(image)?;
    let mut h = record::reset_holder(2, index, 0);
    cs705::icf::record_image::load_channel_into(
        model.working(),
        ChannelKind::Normal,
        index,
        &mut h,
    )?;
    let generation =
        normalizer::ReferenceImage::Separate(runtime.generation.as_deref().unwrap_or(&[]));
    let notification = node_index
        .map(|index| cs705::icf::rows::native::name_notification(&h, index))
        .transpose()?;
    let NormalState { ranges, editor, .. } = captured;
    let mut effect = NameEffect::default();
    let mut effects = Effects::default();
    crate::repeaters::text_input(&source, |input| {
        Ok(model.native_name_input(
            &mut h,
            editor,
            NameRequest {
                input,
                context: EditorContext { ranges, generation },
                frame: &mut frame,
                notification,
            },
            &mut effect,
            &mut effects,
            |_, _| Ok(()),
        )?)
    })?;
    if effect.stored {
        image.patch_memory(&[(0, model.to_icf()?.memory())])?;
    }
    io::report(
        &json!({"normal_state":normal_state_value(captured),"effects":normal_effects(&effects),"stored":effect.stored,
        "state_out":effect.state_out,"display":io::bytes_value(&effect.display),"shared_input":io::bytes_value(&effect.shared_input),
        "name_frame":{"layout":frame.layout,"node_flags":frame.node_flags},"requested_pending":effect.pending_requested.map(|payload|json!({"id":0x32,"payload":payload})),
        "representation":"native name byte collector; notification requests are not OS delivery"}),
    )?;
    Ok(effect.stored || effect.pending_requested.is_some())
}

fn native_memory_offset_edit(
    image: &mut Icf<'_>,
    index: u32,
    text: &str,
    runtime: &Runtime,
) -> Result<bool> {
    use cs705::icf::rows::{
        NormalMemory,
        native::{EditorContext, Effects, OffsetRequest},
    };
    let mut state = runtime.normal.borrow_mut();
    let captured = state.as_mut().ok_or_else(|| {
        AppError::safety("initialize or restore normal state before offset input")
    })?;
    let mut model = NormalMemory::from_icf(image)?;
    let mut h = record::reset_holder(2, index, 0);
    cs705::icf::record_image::load_channel_into(
        model.working(),
        ChannelKind::Normal,
        index,
        &mut h,
    )?;
    let generation =
        normalizer::ReferenceImage::Separate(runtime.generation.as_deref().unwrap_or(&[]));
    let NormalState { ranges, editor, .. } = captured;
    let mut effects = Effects::default();
    let (out, _) = crate::fields::with_number_input(text, |input, point| {
        Ok(model.native_offset_input(
            &mut h,
            editor,
            OffsetRequest {
                input,
                point,
                context: EditorContext { ranges, generation },
            },
            &mut effects,
            |_, _| Ok(()),
        )?)
    })?;
    if out.accepted {
        image.patch_memory(&[(0, model.to_icf()?.memory())])?;
    }
    io::report(
        &json!({"normal_state":normal_state_value(captured),"effects":normal_effects(&effects),"stored":out.accepted,"candidate":out.candidate,"state_out":out.state_out,"offset_hz":h.offset_hz(),"representation":"native offset scale6 input then shared unsigned100Hz rounding and codec"}),
    )?;
    Ok(out.accepted)
}

fn native_frequency_source(text: &str) -> Result<(String, u8, u8)> {
    use values::ValueCodec;
    let v: Value = serde_json::from_str(text)
        .map_err(|_| AppError::invalid("frequency requires input/warning_bits/focus JSON"))?;
    if !v.as_object().is_some_and(|o| {
        o.len() == 3
            && ["input", "warning_bits", "focus"]
                .iter()
                .all(|k| o.contains_key(*k))
    }) {
        return Err(AppError::invalid(
            "frequency requires only input and actual warning_bits/focus",
        ));
    }
    Ok((
        native_number_source(&v["input"])?,
        u8::parse(&v["warning_bits"])?,
        u8::parse(&v["focus"])?,
    ))
}

fn native_memory_frequency_edit(
    image: &mut Icf<'_>,
    index: u32,
    text: &str,
    runtime: &Runtime,
) -> Result<bool> {
    use cs705::icf::rows::{
        NormalMemory,
        native::{EditorContext, Effects, FrequencyRequest},
    };
    let (source, warning_bits, focus) = native_frequency_source(text)?;
    let live_q = runtime.context.as_ref().ok_or_else(|| {
        AppError::safety("normal frequency warnings require explicit live --record-context Q")
    })?;
    let mut state = runtime.normal.borrow_mut();
    let captured = state.as_mut().ok_or_else(|| {
        AppError::safety("initialize or restore normal state before frequency input")
    })?;
    let mut model = NormalMemory::from_icf(image)?;
    let mut h = record::reset_holder(2, index, 0);
    cs705::icf::record_image::load_channel_into(
        model.working(),
        ChannelKind::Normal,
        index,
        &mut h,
    )?;
    let generation =
        normalizer::ReferenceImage::Separate(runtime.generation.as_deref().unwrap_or(&[]));
    let NormalState { ranges, editor, .. } = captured;
    let mut effects = Effects::default();
    let (out, _) = crate::fields::with_number_input(&source, |input, point| {
        Ok(model.native_frequency_input(
            &mut h,
            editor,
            FrequencyRequest {
                input,
                point,
                context: EditorContext { ranges, generation },
                live_q,
                warning_bits,
                focus,
            },
            &mut effects,
            |_, _| Ok(()),
        )?)
    })?;
    runtime.native_action_flags(out.notify_flags);
    if out.stored {
        image.patch_memory(&[(0, model.to_icf()?.memory())])?;
    }
    io::report(
        &json!({"normal_state":normal_state_value(captured),"effects":normal_effects(&effects),"stored":out.stored,
        "first":{"candidate":out.first.candidate,"accepted":out.first.accepted,"state_out":out.first.state_out},
        "second":out.second.map(|s|json!({"candidate":s.candidate,"accepted":s.accepted,"state_out":s.state_out})),
        "notify_flags":out.notify_flags,"warning_bits":out.warning_bits,"focus":out.focus,
        "representation":"native frequency SAME-text two-pass input; captured B and separate live Q; effects are requests"}),
    )?;
    Ok(out.stored)
}

fn native_code_source(text: &str) -> Result<(String, u8, Option<u8>)> {
    use values::ValueCodec;
    let v: Value = serde_json::from_str(text)
        .map_err(|_| AppError::invalid("code requires input/node_flags JSON"))?;
    if !v.as_object().is_some_and(|o| {
        o.contains_key("input")
            && o.contains_key("node_flags")
            && o.keys()
                .all(|k| ["input", "node_flags", "first"].contains(&k.as_str()))
    }) {
        return Err(AppError::invalid(
            "code requires input and actual node_flags; optional first is a Begin digit",
        ));
    }
    Ok((
        native_number_source(&v["input"])?,
        u8::parse(&v["node_flags"])?,
        v.get("first").map(u8::parse).transpose()?,
    ))
}

fn native_memory_code_edit(
    image: &mut Icf<'_>,
    index: u32,
    text: &str,
    runtime: &Runtime,
) -> Result<bool> {
    use cs705::icf::rows::{
        NormalMemory,
        native::{CodeRequest, EditorContext, Effects},
    };
    let (source, mut flags, first) = native_code_source(text)?;
    let mut state = runtime.normal.borrow_mut();
    let captured = state.as_mut().ok_or_else(|| {
        AppError::safety(
            "initialize record/native-memory/state or restore --normal-state before native code",
        )
    })?;
    let mut model = NormalMemory::from_icf(image)?;
    let mut h = record::reset_holder(2, index, 0);
    cs705::icf::record_image::load_channel_into(
        model.working(),
        ChannelKind::Normal,
        index,
        &mut h,
    )?;
    let generation =
        normalizer::ReferenceImage::Separate(runtime.generation.as_deref().unwrap_or(&[]));
    let NormalState { ranges, editor, .. } = captured;
    let mut effects = Effects::default();
    let (out, _) = crate::fields::with_number_input(&source, |input, _point| {
        Ok(model.native_code_input(
            &mut h,
            editor,
            CodeRequest {
                input,
                first,
                node_flags: &mut flags,
                context: EditorContext { ranges, generation },
            },
            &mut effects,
            |_, _| Ok(()),
        )?)
    })?;
    if out.stored {
        image.patch_memory(&[(0, model.to_icf()?.memory())])?;
    }
    io::report(
        &json!({"normal_state":normal_state_value(captured),"effects":normal_effects(&effects),"stored":out.stored,"candidate":out.candidate,
        "mode_blocked":out.mode_blocked,"state_out":out.state_out,"display":io::bytes_value(&out.display),"selection":out.selection,"code_node_flags":flags,
        "representation":"native code9A468 handler; dedicated C-prefix grammar, no selected-node adoption"}),
    )?;
    Ok(out.stored)
}

fn native_memory_choice_edit(
    image: &mut Icf<'_>,
    index: u32,
    name: &str,
    text: &str,
    runtime: &Runtime,
) -> Result<bool> {
    use cs705::icf::rows::{
        NormalMemory,
        native::{ChannelChoice, ChoiceFrame, ChoiceRequest, EditorContext, Effects},
    };
    use values::ValueCodec;
    let field = match name {
        "selection" => ChannelChoice::Selection,
        "duplex" => ChannelChoice::Duplex,
        "polarity" => ChannelChoice::Polarity,
        "digital-squelch" => ChannelChoice::DigitalSquelch,
        _ => return Err(AppError::invalid("unknown native channel choice")),
    };
    let (input, mut frame) = if field == ChannelChoice::Selection {
        let v: Value = serde_json::from_str(text)
            .map_err(|_| AppError::invalid("SEL requires input/layout/node_flags JSON"))?;
        if !v.as_object().is_some_and(|o| {
            o.len() == 3
                && ["input", "layout", "node_flags"]
                    .iter()
                    .all(|k| o.contains_key(*k))
        }) {
            return Err(AppError::invalid(
                "SEL requires only input and actual layout/node_flags",
            ));
        }
        (
            crate::fields::choice_input(&v["input"].to_string())?,
            Some(ChoiceFrame {
                layout: u32::parse(&v["layout"])?,
                node_flags: u8::parse(&v["node_flags"])?,
            }),
        )
    } else {
        (crate::fields::choice_input(text)?, None)
    };
    let mut state = runtime.normal.borrow_mut();
    let captured=state.as_mut().ok_or_else(||AppError::safety("initialize record/native-memory/state or restore --normal-state before a native choice"))?;
    let mut model = NormalMemory::from_icf(image)?;
    let mut h = record::reset_holder(2, index, 0);
    cs705::icf::record_image::load_channel_into(
        model.working(),
        ChannelKind::Normal,
        index,
        &mut h,
    )?;
    let choices = field.choices(&h);
    let generation =
        normalizer::ReferenceImage::Separate(runtime.generation.as_deref().unwrap_or(&[]));
    let NormalState { ranges, editor, .. } = captured;
    let mut effects = Effects::default();
    let result = model.native_choice_input(
        &mut h,
        editor,
        ChoiceRequest {
            field,
            input,
            context: EditorContext { ranges, generation },
            selection: frame.as_mut(),
        },
        &mut effects,
        |_, _| Ok(()),
    )?;
    if result.stored {
        image.patch_memory(&[(0, model.to_icf()?.memory())])?;
    }
    io::report(
        &json!({"normal_state":normal_state_value(captured),"effects":normal_effects(&effects),
        "stored":result.stored,"candidate":result.candidate,"mode_blocked":result.mode_blocked,"popup_requested":result.popup_requested,
        "selector":field.selector(),"choice_values":choices,"selection_frame":frame.map(|f|json!({"layout":f.layout,"node_flags":f.node_flags})),
        "representation":"native existing-row choice handler; common document/OS dispatch is separate"}),
    )?;
    Ok(result.stored)
}

fn split_frame_value(frame: &cs705::icf::rows::native::SplitFrame) -> Value {
    json!({"layout":frame.layout,"pre_step":frame.pre_step,"selection_first":frame.selection_first,
        "node_index":frame.node_index,"node_flags":frame.node_flags,"notify_flags":frame.notify_flags,
        "active":frame.active,"display_mode":frame.display_mode})
}

fn split_input(
    text: &str,
) -> Result<(
    cs705::icf::settings::extended::ChoiceInput,
    cs705::icf::rows::native::SplitFrame,
)> {
    use values::ValueCodec;
    let value: Value = serde_json::from_str(text)
        .map_err(|_| AppError::invalid("split requires input/frame JSON"))?;
    if !value
        .as_object()
        .is_some_and(|o| o.len() == 2 && o.contains_key("input") && o.contains_key("frame"))
    {
        return Err(AppError::invalid(
            "split requires only input and actual caller frame",
        ));
    }
    let frame = &value["frame"];
    if !frame.as_object().is_some_and(|o| {
        o.len() == 8
            && [
                "layout",
                "pre_step",
                "selection_first",
                "node_index",
                "node_flags",
                "notify_flags",
                "active",
                "display_mode",
            ]
            .iter()
            .all(|k| o.contains_key(*k))
    }) {
        return Err(AppError::invalid(
            "split requires all eight actual caller frame scalars",
        ));
    }
    Ok((
        crate::fields::choice_input(&value["input"].to_string())?,
        cs705::icf::rows::native::SplitFrame {
            layout: u32::parse(&frame["layout"])?,
            pre_step: i32::parse(&frame["pre_step"])?,
            selection_first: i32::parse(&frame["selection_first"])?,
            node_index: i32::parse(&frame["node_index"])?,
            node_flags: u8::parse(&frame["node_flags"])?,
            notify_flags: u32::parse(&frame["notify_flags"])?,
            active: i32::parse(&frame["active"])?,
            display_mode: i32::parse(&frame["display_mode"])?,
        },
    ))
}

/// Separate split family. Existing native editor/constructor bodies stay frozen
/// for independent QA; this consumes their REAL initialized/restored live state.
pub fn native_memory_split_edit(
    image: &mut Icf<'_>,
    key: &str,
    text: &str,
    runtime: &Runtime,
) -> Result<bool> {
    native_channel_entry(
        image,
        key,
        text,
        runtime,
        ChannelKind::Normal,
        native_memory_split_field_edit,
    )
}

fn native_memory_split_field_edit(
    image: &mut Icf<'_>,
    key: &str,
    text: &str,
    runtime: &Runtime,
) -> Result<bool> {
    use cs705::icf::rows::{
        NormalMemory,
        native::{EditorContext, Effects, SplitRequest},
    };
    let tail = key
        .strip_prefix("record/native-memory/")
        .ok_or_else(|| AppError::invalid("native memory split path required"))?;
    let decoded_path = format!("record/memory/{tail}");
    let (_, index, name) = path(&decoded_path)?
        .ok_or_else(|| AppError::invalid("native memory split requires GROUP/typed-SIDE/split"))?;
    if name != "split" {
        return Err(AppError::invalid(
            "split family does not handle another field",
        ));
    }
    let (input, mut frame) = split_input(text)?;
    let mut state = runtime.normal.borrow_mut();
    let captured = state.as_mut().ok_or_else(|| {
        AppError::safety(
            "initialize record/native-memory/state or restore --normal-state before split",
        )
    })?;
    let mut model = NormalMemory::from_icf(image)?;
    let mut h = record::reset_holder(2, index, 0);
    cs705::icf::record_image::load_channel_into(
        model.working(),
        ChannelKind::Normal,
        index,
        &mut h,
    )?;
    let generation =
        normalizer::ReferenceImage::Separate(runtime.generation.as_deref().unwrap_or(&[]));
    let NormalState { ranges, editor, .. } = captured;
    let mut effects = Effects::default();
    let result = model.native_split_input(
        &mut h,
        editor,
        SplitRequest {
            input,
            context: EditorContext { ranges, generation },
            frame: &mut frame,
        },
        &mut effects,
        |_, _| Ok(()),
        |_, _| Ok(()),
    )?;
    runtime.native_action_flags(frame.notify_flags);
    if result.stored {
        image.patch_memory(&[(0, model.to_icf()?.memory())])?;
    }
    io::report(
        &json!({"normal_state":normal_state_value(captured),"split_frame":split_frame_value(&frame),
        "effects":normal_effects(&effects),"stored":result.stored,"candidate":result.candidate,
        "popup_requested":result.popup_requested,"selector":"GI_Sheet_Sel_Misc_OnOff2","choice_values":[0,1],
        "representation":"native split case; display/queue effects are requests, not OS delivery"}),
    )?;
    Ok(result.stored)
}

impl Runtime {
    fn defaults(&self) -> Result<DefaultContext> {
        let context=self.context.as_ref().ok_or_else(||AppError::safety("repeater expansion requires --record-context; current Q cannot be inferred from an ICF"))?;
        Ok(record::repeater_default_context(
            context,
            self.system_langid,
        )?)
    }
    fn reset_word(&self, which: usize) -> std::result::Result<u16, cs705::icf::Error> {
        let generation = self
            .generation
            .as_deref()
            .ok_or(cs705::icf::Error::Invalid {
                line: 5,
                reason: "explicit global G reset source unavailable",
            })?;
        record::repeater_reset_word(generation, which)
    }
}

fn native_clipboard(clipboard: &cs705::icf::rows::native::Clipboard) -> Value {
    json!({"version":2,"schema":"cs705.native-clipboard","kind":clipboard.source_kind,"mode":clipboard.mode,
        "declared_count":clipboard.declared_count,"logical_count":clipboard.logical_count,
        "records":clipboard.records.iter().map(|h|json!({"bytes":h.as_bytes().as_slice()})).collect::<Vec<_>>()})
}
fn parse_native_clipboard(bytes: &[u8]) -> Result<cs705::icf::rows::native::Clipboard> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| AppError::invalid("invalid native clipboard JSON"))?;
    let object = value.as_object().filter(|o| o.len() == 7).ok_or_else(|| {
        AppError::invalid("native clipboard requires exact version/schema/kind/mode/counts/records")
    })?;
    if value["version"] != 2
        || value["schema"] != "cs705.native-clipboard"
        || !["kind", "mode", "declared_count", "logical_count", "records"]
            .iter()
            .all(|key| object.contains_key(*key))
    {
        return Err(AppError::invalid(
            "native clipboard requires version2 and actual K/N/M; decoded616 v1 is not a native clipboard",
        ));
    }
    let number = |key: &str| {
        value[key]
            .as_u64()
            .filter(|&n| n <= 20000)
            .map(|n| n as usize)
            .ok_or_else(|| {
                AppError::invalid("native clipboard numeric metadata outside its bounded domain")
            })
    };
    let rows = value["records"]
        .as_array()
        .filter(|rows| !rows.is_empty() && rows.len() <= 20000)
        .ok_or_else(|| AppError::invalid("native clipboard requires1..20000 snapshots"))?;
    let records = rows
        .iter()
        .map(|row| {
            if !row
                .as_object()
                .is_some_and(|o| o.len() == 1 && o.contains_key("bytes"))
            {
                return Err(AppError::invalid(
                    "native clipboard row requires only exact616 bytes",
                ));
            }
            Ok(DecodedRecord::from_bytes(
                <[u8; 616] as values::ValueCodec>::parse(&row["bytes"])?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let clipboard = cs705::icf::rows::native::Clipboard {
        source_kind: number("kind")? as u32,
        mode: number("mode")? as u32,
        declared_count: number("declared_count")?,
        logical_count: number("logical_count")?,
        records,
    };
    clipboard.validate()?;
    Ok(clipboard)
}
fn normal_effects(e: &cs705::icf::rows::native::Effects) -> Value {
    json!({"home_invalidated":e.home_invalidated,"dr_invalidated":e.dr_invalidated,"context_flags":e.context_flags,"selection":e.selection,"active":e.active,"display_mode":e.display_mode,"cache_invalidated":e.cache_invalidated,
        "requested_notifications":e.notifications.iter().map(|n|json!({"id":n.id,"synchronous":n.synchronous,"arg1":n.arg1,"arg2":n.arg2})).collect::<Vec<_>>()})
}
/// Native normal row root; band/memopad Copy feeds this SAME destination engine.
pub fn normal_row(input: &str, sheet: Sheet, action: RowAction, options: &Options) -> Result<u8> {
    if options.normal_state.is_some() {
        return Err(AppError::invalid(
            "normal-state requires an explicit native memory editor, not a row operation",
        ));
    }
    use cs705::icf::rows::{
        NormalMemory,
        native::{Clipboard, CopySource, Effects, Selection},
    };
    let runtime = options.load(std::iter::once(input).chain(action_extra(&action)))?;
    if options.reference_defaults.is_some() {
        return Err(AppError::invalid(
            "normal rows use --generation G, not UR T",
        ));
    }
    let (group, index) = match &action {
        RowAction::Copy { group, index, .. }
        | RowAction::Paste { group, index, .. }
        | RowAction::Move { group, index, .. }
        | RowAction::Clear { group, index, .. }
        | RowAction::Delete { group, index, .. }
        | RowAction::Insert { group, index, .. } => (*group, *index),
    };
    let write = action_write(&action);
    let end = options.end_index.unwrap_or(index);
    if index > end {
        return Err(AppError::invalid("native selection end precedes its start"));
    }
    let bytes = io::read(input)?;
    let image = Icf::parse(&bytes)?;
    if matches!(sheet, Sheet::Bandstack | Sheet::Memopad) {
        if group != 0
            || !matches!(&action, RowAction::Copy { .. })
            || options.side_view
            || options.show_both.is_some()
        {
            return Err(AppError::invalid(
                "special native source supports group0 Copy with native row coordinates only",
            ));
        }
        let kind = if sheet == Sheet::Bandstack { 4 } else { 5 };
        let (low, high) = if kind == 4 { (0, 44) } else { (1, 10) };
        if index < low || end > high {
            return Err(AppError::invalid(
                "special copy selection outside its actual sheet range",
            ));
        }
        let enabled = if kind == 4 && index < 27 && end >= 27 {
            Some(
                runtime
                    .context
                    .as_ref()
                    .ok_or_else(|| {
                        AppError::safety("band copy crossing27 requires actual --record-context Q")
                    })?
                    .effective_f()?
                    != 0,
            )
        } else {
            None
        };
        let mut clipboard = Clipboard::default();
        clipboard.copy_from(
            image.memory(),
            CopySource {
                kind,
                first: index as u32,
                count: end - index + 1,
                request: 0,
                expanded: false,
                binding: 0,
                band_enabled: enabled,
            },
        )?;
        io::report(
            &json!({"context_flags":4,"selection":[index,end],"cache_invalidated":true,"representation":"native-copy-requests"}),
        )?;
        io::publish(&io::json_bytes(&native_clipboard(&clipboard))?, write)?;
        return Ok(0);
    }
    if sheet != Sheet::Memory {
        return Err(AppError::invalid(
            "normal native row requires memory or a supported special Copy source",
        ));
    }
    let limit = if options.side_view { 199 } else { 99 };
    if end > limit {
        return Err(AppError::invalid(
            "normal selection exceeds its coordinate range",
        ));
    }
    let show_both = if matches!(
        &action,
        RowAction::Copy { .. } | RowAction::Paste { .. } | RowAction::Move { .. }
    ) {
        options.show_both.ok_or_else(|| {
            AppError::safety("native copy/paste/carry requires explicit --show-both true|false")
        })?
    } else {
        false
    };
    let first = if options.side_view { index } else { 2 * index };
    let last = if options.side_view { end } else { 2 * end + 1 };
    let selection = Selection {
        group,
        active: first,
        first,
        last,
        merged: !options.side_view,
        show_both,
    };
    let mut model = NormalMemory::from_icf(&image)?;
    let mut effects = Effects::default();
    if let RowAction::Copy { .. } = &action {
        let mut clipboard = Clipboard::default();
        model.native_copy(selection, &mut clipboard, &mut effects)?;
        io::report(&normal_effects(&effects))?;
        io::publish(&io::json_bytes(&native_clipboard(&clipboard))?, write)?;
        return Ok(0);
    }
    // No bytes are supplied for an absent G. A reached parent reset read fails
    // at its actual cutpoint; branches that never read G do not invent one.
    let generation = match runtime.generation.as_deref() {
        Some(g) => normalizer::ReferenceImage::Separate(g),
        None => normalizer::ReferenceImage::Separate(&[]),
    };
    let yes = match &action {
        RowAction::Paste { yes, .. }
        | RowAction::Clear { yes, .. }
        | RowAction::Delete { yes, .. }
        | RowAction::Insert { yes, .. } => *yes,
        _ => false,
    };
    let mut declined = false;
    let mut confirm_native = |_| {
        if yes {
            Ok(1)
        } else {
            declined = true;
            Err(cs705::icf::Error::Invalid {
                line: 0,
                reason: "native row confirmation declined",
            })
        }
    };
    let outcome = match &action {
        RowAction::Paste { clipboard, .. } => {
            let mut clipboard = parse_native_clipboard(&io::read(clipboard)?)?;
            model.native_paste(
                selection,
                &mut clipboard,
                generation,
                &mut effects,
                &mut confirm_native,
                |_, _| Ok(()),
            )
        }
        RowAction::Clear { seed: None, .. } => {
            model.native_clear(selection, generation, &mut effects, &mut confirm_native)
        }
        RowAction::Delete { .. } => {
            model.native_delete(selection, generation, &mut effects, &mut confirm_native)
        }
        RowAction::Insert { seed: None, .. } => {
            model.native_insert(selection, generation, &mut effects, &mut confirm_native)
        }
        RowAction::Move {
            destination_group,
            destination_index,
            ..
        } => {
            if *destination_group != group || *destination_index == index {
                return Err(AppError::invalid(
                    "native carry requires a direction in the same group",
                ));
            }
            let direction = if *destination_index < index { -1 } else { 1 };
            let planned = model.native_carry_target(selection, direction)?;
            if let Some((target, _)) = planned {
                let expected = if options.side_view {
                    target
                } else {
                    target / 2
                };
                if *destination_index != expected {
                    return Err(AppError::invalid(
                        "destination differs from the native projected carry target",
                    ));
                }
            }
            model.native_carry(
                selection,
                direction,
                generation,
                &mut effects,
                |_, _| Ok(()),
            )
        }
        _ => {
            return Err(AppError::invalid(
                "normal native row does not consume a holder seed",
            ));
        }
    };
    match outcome {
        Ok(outcome) => {
            if outcome.exhausted && !runtime.accept_partial {
                io::report(
                    &json!({"effects":normal_effects(&effects),"partial":true,"published":false}),
                )?;
                return Ok(5);
            }
            if outcome.processed {
                io::publish(&model.to_icf()?.to_bytes(), write)?;
            }
            io::report(
                &json!({"effects":normal_effects(&effects),"written":outcome.written,"native_count":outcome.native_count,"processed":outcome.processed,"partial":outcome.exhausted,"published":outcome.processed&&!write.dry_run}),
            )?;
            Ok(if outcome.exhausted { 5 } else { 0 })
        }
        Err(error) => {
            if declined {
                return Err(AppError::safety(
                    "native row confirmation declined; use --yes",
                ));
            }
            if !runtime.accept_partial {
                io::report(
                    &json!({"effects":normal_effects(&effects),"partial":true,"published":false}),
                )?;
                return Err(error.into());
            }
            io::publish(&model.to_icf()?.to_bytes(), write)?;
            io::report(
                &json!({"effects":normal_effects(&effects),"partial":true,"published":!write.dry_run,"error":error.to_string()}),
            )?;
            Ok(5)
        }
    }
}

/// Call indices retain their typed-side0..7 boundary; side-view disables merge.
pub fn call_row(input: &str, action: RowAction, options: &Options) -> Result<u8> {
    use cs705::icf::rows::native::{Clipboard, Effects, call};
    let runtime = options.load(std::iter::once(input).chain(action_extra(&action)))?;
    let (group, index) = match &action {
        RowAction::Copy { group, index, .. }
        | RowAction::Paste { group, index, .. }
        | RowAction::Move { group, index, .. } => (*group, *index),
        _ => {
            return Err(AppError::invalid(
                "call row clear/delete/insert is forbidden by native metadata; use record/native-call/reset-pair for initialization",
            ));
        }
    };
    if options.reference_defaults.is_some() || options.generation.is_some() {
        return Err(AppError::invalid(
            "call copy/paste/carry does not consume G or UR T",
        ));
    }
    let end = options.end_index.unwrap_or(index);
    let limit = 7;
    if group != 100 || index > end || end > limit {
        return Err(AppError::invalid(
            "call selection requires group100 and typed sides0..7",
        ));
    }
    let show_both = options.show_both.ok_or_else(|| {
        AppError::safety("native copy/paste/carry requires explicit --show-both true|false")
    })?;
    let first = index;
    let last = end;
    let selection = call::Selection {
        active: first,
        first,
        last,
        merged: !options.side_view,
        show_both,
    };
    let bytes = io::read(input)?;
    let image = Icf::parse(&bytes)?;
    let write = action_write(&action);
    let mut effects = Effects::default();
    if let RowAction::Copy { .. } = &action {
        let mut clipboard = Clipboard::default();
        call::copy(image.memory(), selection, &mut clipboard, &mut effects)?;
        io::report(&normal_effects(&effects))?;
        io::publish(&io::json_bytes(&native_clipboard(&clipboard))?, write)?;
        return Ok(0);
    }
    let mut working = image.memory().to_vec();
    let mut declined = false;
    let result = runtime.with_captured_ranges(|ranges| {
        Ok(match &action {
            RowAction::Paste { clipboard, yes, .. } => {
                let mut clipboard = parse_native_clipboard(&io::read(clipboard)?)?;
                call::paste(
                    &mut working,
                    selection,
                    &mut clipboard,
                    ranges,
                    &mut effects,
                    |_| {
                        if *yes {
                            Ok(1)
                        } else {
                            declined = true;
                            Err(cs705::icf::Error::Invalid {
                                line: 0,
                                reason: "native row confirmation declined; use --yes",
                            })
                        }
                    },
                    |_, _| Ok(()),
                )
            }
            RowAction::Move {
                destination_group,
                destination_index,
                ..
            } => {
                if *destination_group != group
                    || *destination_index > limit
                    || *destination_index == index
                {
                    return Err(AppError::invalid(
                        "call carry requires a direction in the same group",
                    ));
                }
                let direction = if *destination_index < index { -1 } else { 1 };
                let active = load(&image, ChannelKind::Call, first as u32)?;
                let step = if first == last
                    && (direction < 0 && active.side() != 0
                        || direction > 0
                            && active.side() == 0
                            && (active.split() != 0 || show_both))
                {
                    1
                } else {
                    2
                };
                let start = if step == 2 { first & !1 } else { first };
                let target = start as i32 + direction * step;
                if (0..8).contains(&target) {
                    let expected = target as usize;
                    if *destination_index != expected {
                        return Err(AppError::invalid(
                            "destination differs from the native projected call carry target",
                        ));
                    }
                }
                call::carry(
                    &mut working,
                    selection,
                    direction,
                    ranges,
                    &mut effects,
                    |_, _| Ok(()),
                )
            }
            _ => unreachable!(),
        })
    })?;
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            if declined {
                return Err(AppError::safety(
                    "native row confirmation declined; use --yes",
                ));
            }
            if runtime.accept_partial {
                let mut image = image;
                image.patch_memory(&[(0, &working)])?;
                io::publish(&image.to_bytes(), write)?;
            }
            io::report(
                &json!({"effects":normal_effects(&effects),"partial":true,"published":runtime.accept_partial&&!write.dry_run,"error":error.to_string()}),
            )?;
            return if runtime.accept_partial {
                Ok(5)
            } else {
                Err(error.into())
            };
        }
    };
    if outcome.processed {
        let mut image = image;
        image.patch_memory(&[(0, &working)])?;
        io::publish(&image.to_bytes(), write)?;
    }
    io::report(
        &json!({"effects":normal_effects(&effects),"processed":outcome.processed,
        "written":outcome.written,"native_count":outcome.native_count,"partial":false,
        "published":outcome.processed&&!write.dry_run}),
    )?;
    Ok(0)
}

fn repeater_field(name: &str) -> Option<record::RepeaterField> {
    use record::RepeaterField as F;
    match name {
        "mode" => Some(F::Mode),
        "tone" => Some(F::Tone),
        "tone-index" => Some(F::ToneIndex),
        "use" => Some(F::Use),
        "position" => Some(F::Position),
        "utc-offset-minutes" => Some(F::UtcOffset),
        "gateway-index" => Some(F::GatewayIndex),
        _ => None,
    }
}
fn repeater_text(name: &str) -> Option<record::RepeaterText> {
    match name {
        "sub-name" => Some(record::RepeaterText::SubName),
        "repeater-call" => Some(record::RepeaterText::Call),
        "gateway-call" => Some(record::RepeaterText::Gateway),
        _ => None,
    }
}
fn repeater_value(holder: &DecodedRecord, name: &str) -> Result<Value> {
    if let Some(field) = repeater_field(name) {
        let n = holder.repeater_field(field);
        return Ok(if field == record::RepeaterField::UtcOffset {
            json!(n as i32)
        } else {
            json!(n)
        });
    }
    if let Some(field) = repeater_text(name) {
        return Ok(io::bytes_value(holder.repeater_text(field)));
    }
    match name {
        "latitude-bytes" => Ok(io::bytes_value(holder.coordinate_bytes(false))),
        "longitude-bytes" => Ok(io::bytes_value(holder.coordinate_bytes(true))),
        _ => value(holder, name),
    }
}
const REPEATER_FIELDS: &[&str] = &[
    "valid",
    "name",
    "sub-name",
    "repeater-call",
    "gateway-call",
    "frequency-hz",
    "duplex",
    "offset-hz",
    "mode",
    "tone",
    "tone-index",
    "use",
    "position",
    "latitude-bytes",
    "longitude-bytes",
    "utc-offset-minutes",
    "gateway-index",
];
fn repeater_holder(working: &Working, index: usize) -> Result<DecodedRecord> {
    let mut holder = record::reset_holder(0x0c, index as u32, 0);
    working.load_into(index, &mut holder)?;
    Ok(holder)
}
pub fn repeater_view(
    image: &Icf<'_>,
    start: usize,
    count: usize,
    runtime: &Runtime,
) -> Result<Value> {
    let end = start
        .checked_add(count)
        .ok_or_else(|| AppError::invalid("repeater row range overflow"))?;
    if start > 2500 {
        return Err(AppError::invalid("repeater row start outside dataset"));
    }
    let working = Working::from_icf(image, runtime.defaults()?)?;
    (start..end.min(2500)).map(|index|{
        let holder=repeater_holder(&working,index)?;let mut row=json!({"index":index,"group":holder.group(),"representation":"decoded-working-repeater"});
        for &name in REPEATER_FIELDS{row[name]=repeater_value(&holder,name)?;}Ok(row)
    }).collect::<Result<Vec<_>>>().map(Value::Array)
}
fn repeater_path(key: &str) -> Result<(usize, &str)> {
    let parts: Vec<_> = key.split('/').collect();
    match parts.as_slice() {
        ["record", "repeater", index, name] => {
            let index = values::parse::<usize>(index)?;
            if index >= 2500 || !REPEATER_FIELDS.contains(name) {
                return Err(AppError::invalid(
                    "repeater path requires INDEX0..2499 and a published field",
                ));
            }
            Ok((index, *name))
        }
        _ => Err(AppError::invalid(
            "repeater path requires record/repeater/INDEX/FIELD",
        )),
    }
}
pub fn repeater_get(image: &Icf<'_>, key: &str, runtime: &Runtime) -> Result<Value> {
    let (index, name) = repeater_path(key)?;
    let working = Working::from_icf(image, runtime.defaults()?)?;
    repeater_value(&repeater_holder(&working, index)?, name)
}
const FROM_FIELDS: &[&str] = &[
    "valid",
    "call",
    "supplemented",
    "name",
    "frequency-hz",
    "mode",
    "duplex",
    "offset-hz",
];
const TO_FIELDS: &[&str] = &["valid", "call", "supplemented", "name"];
fn history_fields(sheet: Sheet) -> &'static [&'static str] {
    if sheet == Sheet::To {
        TO_FIELDS
    } else {
        FROM_FIELDS
    }
}
fn history_field_value(holder: &DecodedRecord, sheet: Sheet, name: &str) -> Result<Value> {
    if !history_fields(sheet).contains(&name) {
        return Err(AppError::invalid("unknown From/To history field"));
    }
    Ok(match name {
        "valid" => json!(holder.valid()),
        "call" => {
            if holder.valid() {
                io::bytes_value(holder.history_call())
            } else {
                Value::Null
            }
        }
        "supplemented" => json!(holder.history_supplemented()),
        "frequency-hz" if sheet == Sheet::FromFm && holder.valid() => {
            json!(holder.channel_field(ChannelField::FrequencyHz))
        }
        _ if !holder.history_supplemented() => Value::Null,
        "mode" => json!(holder.repeater_field(record::RepeaterField::Mode)),
        _ => value(holder, name)?,
    })
}
pub fn history_view(
    image: &Icf<'_>,
    sheet: Sheet,
    start: usize,
    count: usize,
    runtime: &Runtime,
) -> Result<Value> {
    let kind = history_kind(sheet)
        .filter(|kind| *kind != 0x0e)
        .ok_or_else(|| AppError::invalid("history view requires FromDV/FromFM/To"))?;
    let limit = if sheet == Sheet::To { 20 } else { 10 };
    let end = start
        .checked_add(count)
        .ok_or_else(|| AppError::invalid("history row range overflow"))?;
    if start > limit {
        return Err(AppError::invalid("history row start outside dataset"));
    }
    let repeaters = Working::from_icf(image, runtime.defaults()?)?;
    let ur = cs705::icf::rows::WorkingUr::from_icf(image)?;
    (start..end.min(limit))
        .map(|index| {
            let mut holder = record::reset_holder(kind, index as u32, 0);
            record::load_history_storage_into(image, kind, index, &repeaters, &ur, &mut holder)?;
            let mut row = json!({"index":index,"representation":"decoded-current-list-history"});
            for &name in history_fields(sheet) {
                row[name] = history_field_value(&holder, sheet, name)?;
            }
            Ok(row)
        })
        .collect::<Result<Vec<_>>>()
        .map(Value::Array)
}
pub(crate) fn history_holder(
    image: &Icf<'_>,
    sheet: Sheet,
    index: usize,
    runtime: &Runtime,
) -> Result<DecodedRecord> {
    let kind = history_kind(sheet)
        .filter(|kind| *kind != 0x0e)
        .ok_or_else(|| AppError::invalid("history holder requires FromDV/FromFM/To"))?;
    let repeaters = Working::from_icf(image, runtime.defaults()?)?;
    let ur = cs705::icf::rows::WorkingUr::from_icf(image)?;
    let mut holder = record::reset_holder(kind, index as u32, 0);
    record::load_history_storage_into(image, kind, index, &repeaters, &ur, &mut holder)?;
    Ok(holder)
}
pub fn history_get(image: &Icf<'_>, key: &str, runtime: &Runtime) -> Result<Value> {
    let parts: Vec<_> = key.split('/').collect();
    let (sheet, index, name) = match parts.as_slice() {
        ["record", sheet, index, name] => {
            let sheet = match *sheet {
                "from-dv" => Sheet::FromDv,
                "from-fm" => Sheet::FromFm,
                "to" => Sheet::To,
                _ => return Err(AppError::invalid("unknown From/To history namespace")),
            };
            (sheet, values::parse::<usize>(index)?, *name)
        }
        _ => {
            return Err(AppError::invalid(
                "history path requires record/SHEET/INDEX/FIELD",
            ));
        }
    };
    history_field_value(&history_holder(image, sheet, index, runtime)?, sheet, name)
}
pub fn repeater_edit(image: &mut Icf<'_>, key: &str, text: &str, runtime: &Runtime) -> Result<()> {
    let (index, name) = repeater_path(key)?;
    if matches!(name, "valid" | "gateway-index") {
        return Err(AppError::invalid(
            "repeater validity and gateway metadata are read-only",
        ));
    }
    let mut working = Working::from_icf(image, runtime.defaults()?)?;
    let mut holder = repeater_holder(&working, index)?;
    if let Some(field) = repeater_field(name) {
        let n = if field == record::RepeaterField::UtcOffset {
            values::parse::<i32>(text)? as u32
        } else {
            values::parse::<u32>(text)?
        };
        holder.set_repeater_field(field, n);
    } else if let Some(field) = repeater_text(name) {
        holder.set_repeater_text(field, &values::parse::<[u8; 8]>(text)?);
    } else if matches!(name, "latitude-bytes" | "longitude-bytes") {
        holder.set_coordinate_bytes(name == "longitude-bytes", &values::parse::<[u8; 4]>(text)?);
    } else {
        apply(&mut holder, name, text)?;
    }
    working.save(index, &holder)?;
    let result = working.to_icf()?;
    // The storage rebuilder owns all compression/links; only its complete image
    // is adopted. Icf's existing metadata remains on the document being edited.
    image.patch_memory(&[(0, result.memory())])?;
    Ok(())
}

fn action_extra(action: &RowAction) -> Option<&str> {
    match action {
        RowAction::Paste { clipboard, .. } => Some(clipboard),
        RowAction::Clear { seed, .. } | RowAction::Insert { seed, .. } => seed.as_deref(),
        _ => None,
    }
}
fn action_write(action: &RowAction) -> &Write {
    match action {
        RowAction::Copy { write, .. }
        | RowAction::Paste { write, .. }
        | RowAction::Move { write, .. }
        | RowAction::Clear { write, .. }
        | RowAction::Delete { write, .. }
        | RowAction::Insert { write, .. } => write,
    }
}
pub fn repeater_row(input: &str, action: RowAction, options: &Options) -> Result<u8> {
    if options.normal_state.is_some() {
        return Err(AppError::invalid(
            "normal-state requires an explicit native memory editor",
        ));
    }
    if options.normal_options_present() {
        return Err(AppError::invalid(
            "normal selection options require a native memory row operation",
        ));
    }
    if options.reference_defaults.is_some() {
        return Err(AppError::invalid(
            "repeater rows use --generation G, not UR --reference-defaults T",
        ));
    }
    if matches!(&action, RowAction::Clear { .. }) {
        return Err(AppError::invalid(
            "the registered repeater row menu has no Clear command",
        ));
    }
    let runtime = options.load(std::iter::once(input).chain(action_extra(&action)))?;
    let bytes = io::read(input)?;
    let image = Icf::parse(&bytes)?;
    let defaults = runtime.defaults()?;
    let mut working = Working::from_icf(&image, defaults)?;
    let groups = working.groups()?;
    let (group, index) = match &action {
        RowAction::Copy { group, index, .. }
        | RowAction::Paste { group, index, .. }
        | RowAction::Move { group, index, .. }
        | RowAction::Delete { group, index, .. }
        | RowAction::Insert { group, index, .. }
        | RowAction::Clear { group, index, .. } => (*group, *index),
    };
    let start = groups.start(group)? as usize;
    let count = usize::from(groups.counts[group]);
    if index > count || (index == count && !matches!(&action, RowAction::Insert { .. })) {
        return Err(AppError::invalid("repeater relative row outside group"));
    }
    let seed = record::reset_holder(0x0c, 0, 0);
    let write = action_write(&action).clone();
    if let RowAction::Copy { .. } = &action {
        let holders = working.copy(group, index, index, &seed)?;
        io::publish(&io::json_bytes(&clipboard(&holders, 0x0c))?, &write)?;
        return Ok(0);
    }
    let pasted = if let RowAction::Paste { clipboard, yes, .. } = &action {
        confirm(*yes)?;
        let holders = parse_clipboard(&io::read(clipboard)?)?;
        if holders.iter().any(|h| h.kind() != 0x0c) {
            return Err(AppError::invalid(
                "repeater paste requires same-kind decoded clipboard records",
            ));
        }
        if index
            .checked_add(holders.len())
            .is_none_or(|end| end > count)
        {
            return Err(AppError::invalid("repeater clipboard exceeds target group"));
        }
        Some(holders)
    } else {
        None
    };
    match &action {
        RowAction::Delete { yes, .. } => confirm(*yes)?,
        RowAction::Insert { seed: Some(_), .. } => {
            return Err(AppError::invalid(
                "repeater type22 is generated from Q/LANGID, not a holder seed",
            ));
        }
        RowAction::Move {
            destination_group,
            destination_index,
            ..
        } if *destination_group != group
            || index.abs_diff(*destination_index) != 1
            || *destination_index >= count =>
        {
            return Err(AppError::invalid(
                "repeater move requires an adjacent row in the same group",
            ));
        }
        _ => {}
    }
    let result: Result<usize> = match action {
        RowAction::Paste { .. } => working
            .paste(
                group,
                index,
                pasted.as_deref().expect("validated paste"),
                |_, which| runtime.reset_word(which),
            )
            .map_err(AppError::from),
        RowAction::Delete { .. } => working
            .delete_one(
                group,
                index,
                &seed,
                |w| Ok(defaults.holder(w.groups()?.group_of(2499) as u32, 0)),
                |_, which| runtime.reset_word(which),
            )
            .map(|()| 1)
            .map_err(AppError::from),
        RowAction::Insert { .. } => working
            .insert_one(
                group,
                index,
                &seed,
                |w| Ok(defaults.holder(w.groups()?.group_of((start + index) as i32) as u32, 0)),
                |_, which| runtime.reset_word(which),
            )
            .map(usize::from)
            .map_err(AppError::from),
        RowAction::Move {
            destination_index, ..
        } => working
            .carry(
                group,
                index,
                index,
                if destination_index < index { -1 } else { 1 },
                &seed,
                |_, which| runtime.reset_word(which),
            )
            .map(usize::from)
            .map_err(AppError::from),
        _ => unreachable!("copy/clear handled before mutation"),
    };
    match result {
        Ok(rows) => {
            io::report(&json!({"rows":rows,"partial":false}))?;
            io::publish(&working.to_icf()?.to_bytes(), &write)?;
            Ok(0)
        }
        Err(error) => {
            if !runtime.accept_partial {
                io::report(&json!({"error":error.message,"partial":true,"published":false}))?;
                return Err(error);
            }
            io::publish(&working.to_icf()?.to_bytes(), &write)?;
            io::report(&json!({"error":error.message,"partial":true,"published":!write.dry_run}))?;
            Ok(5)
        }
    }
}

/// UR row shifts share the core's one expanded-count/order/typed-codec owner.
/// T is mandatory; its actual word window is checked at the post-count read.
pub fn ur_row(input: &str, action: RowAction, options: &Options) -> Result<u8> {
    if options.normal_state.is_some() {
        return Err(AppError::invalid(
            "normal-state requires an explicit native memory editor",
        ));
    }
    if options.normal_options_present() {
        return Err(AppError::invalid(
            "normal selection options require a native memory row operation",
        ));
    }
    let runtime = options.load(std::iter::once(input).chain(action_extra(&action)))?;
    if options.record_context.is_some()
        || options.system_langid.is_some()
        || options.generation.is_some()
    {
        return Err(AppError::invalid(
            "UR rows use --reference-defaults T, not repeater Q/LANGID/G",
        ));
    }
    let (index, delete) = match &action {
        RowAction::Delete {
            group: 0, index, ..
        } => (*index, true),
        RowAction::Insert {
            group: 0,
            index,
            seed: None,
            ..
        } => (*index, false),
        _ => {
            return Err(AppError::invalid(
                "UR row operation requires group0 insert without seed or delete",
            ));
        }
    };
    let bytes = io::read(input)?;
    let image = Icf::parse(&bytes)?;
    let mut working = cs705::icf::rows::WorkingUr::from_icf(&image)?;
    if index >= 300
        || index > usize::from(working.count())
        || (delete && index == usize::from(working.count()))
    {
        return Err(AppError::invalid(
            "UR row outside the active list/insertion boundary",
        ));
    }
    if let RowAction::Delete { yes, .. } = &action {
        confirm(*yes)?;
    }
    if !delete && working.count() == 300 {
        io::publish(&working.to_icf()?.to_bytes(), action_write(&action))?;
        io::report(&json!({"changed":false,"count":300,"partial":false}))?;
        return Ok(0);
    }
    let defaults = runtime.reference_defaults.as_deref().ok_or_else(|| {
        AppError::safety(
            "UR insert/delete requires --reference-defaults with the actual captured T source",
        )
    })?;
    let defaults = normalizer::ReferenceImage::Separate(defaults);
    let result = if delete {
        working.delete_one(index as u32, defaults).map(|()| true)
    } else {
        working.insert_one(index as u32, defaults)
    };
    let write = action_write(&action);
    match result {
        Ok(changed) => {
            io::publish(&working.to_icf()?.to_bytes(), write)?;
            io::report(&json!({"changed":changed,"count":working.count(),"partial":false}))?;
            Ok(0)
        }
        Err(error) => {
            if !runtime.accept_partial {
                io::report(&json!({"partial":true,"published":false,"count":working.count()}))?;
                return Err(error.into());
            }
            io::publish(&working.to_icf()?.to_bytes(), write)?;
            io::report(
                &json!({"partial":true,"published":!write.dry_run,"count":working.count(),"error":error.to_string()}),
            )?;
            Ok(5)
        }
    }
}

const CHANNEL_FIELDS: &[&str] = &[
    "valid",
    "name",
    "side",
    "split",
    "frequency-hz",
    "mode",
    "data",
    "filter",
    "duplex",
    "offset-hz",
    "selection",
    "tone",
    "tx-tone",
    "rx-tone",
    "dtcs",
    "polarity",
    "digital-squelch",
    "digital-code",
    "your-call",
    "repeater1",
    "repeater2",
];

/// Metadata for the same recognized record paths; no second setter registry.
/// Numeric domains are decoded saver inputs, not invented GUI choices. Native
/// masks/mode-specific omissions still belong to the one shared library codec.
pub fn fields() -> Vec<Value> {
    let mut fields = Vec::new();
    for normal in [false, true] {
        let (prefix, arguments) = if normal {
            (
                "record/memory/GROUP/INDEX",
                json!([{"name":"group","min":0,"max":99},{"name":"index","min":0,"max":199}]),
            )
        } else {
            (
                "record/call/INDEX",
                json!([{"name":"index","min":0,"max":7}]),
            )
        };
        for &name in CHANNEL_FIELDS {
            let domain = match name {
                "valid" => json!({"type":"boolean"}),
                "name" => {
                    json!({"type":"ASCII editor text","output_width":16,"normalization":"shared normalize_name"})
                }
                "your-call" | "repeater1" | "repeater2" => {
                    json!({"type":"byte-array","length":8,"element":{"min":0,"max":255},"save":"native low7 packing"})
                }
                "side" => json!({"type":"u8","min":0,"max":1}),
                "split" => json!({"type":"u8","min":0,"max":255,"save":"native lowbit projection"}),
                _ => json!({"type":"u32","min":0,"max":u32::MAX,"boundary":"decoded-saver-value"}),
            };
            fields.push(
                json!({"key":format!("{prefix}/{name}"),"arguments":arguments,"value":domain,
                "read_only":matches!(name,"valid"|"side")||(!normal&&name=="selection"),
                "operation":"decoded saver cutpoint; not the G/cache/editor handler"}),
            );
        }
    }
    for name in ["name", "call"] {
        fields.push(json!({"key":format!("record/ur/INDEX/{name}"),"arguments":[{"name":"index","min":0,"max":299}],
            "read_only":false,"coordinate":"raw-ur-list-slot (count validity is not inferred)",
            "value":if name=="name"{json!({"type":"ASCII editor text","output_width":16})}else{json!({"type":"byte-array","length":8,"element":{"min":0,"max":255}})}}));
    }
    fields.push(json!({"key":"record/ur-list/count","arguments":[],"read_only":true,"value":{"type":"u16","min":0,"max":300},"coordinate":"expanded cached count from saved order"}));
    for name in ["name", "call", "valid"] {
        fields.push(json!({"key":format!("record/ur-list/INDEX/{name}"),"arguments":[{"name":"index","min":0,"max":299}],"read_only":name=="valid","coordinate":"expanded logical UR list","value":match name{"name"=>json!({"type":"ASCII editor text","output_width":16}),"call"=>json!({"type":"byte-array","length":8}),_=>json!({"type":"boolean"})}}));
    }
    let rx = record::reset_holder(0x0e, 0, 0);
    for (name, value) in record::receive_history_fields(&rx) {
        let domain = match value {
            record::HistoryValue::Unsigned(_) => json!({"type":"u32"}),
            record::HistoryValue::Signed(_) => json!({"type":"i32"}),
            record::HistoryValue::Bytes(bytes) => {
                json!({"type":"stored-bytes","length":bytes.len()})
            }
        };
        fields.push(json!({"key":format!("record/rx/INDEX/{name}"),"arguments":[{"name":"index","min":0,"max":49}],"value":domain,"read_only":true}));
    }
    fields.push(json!({"key":"record/rx/INDEX/valid","arguments":[{"name":"index","min":0,"max":49}],"value":{"type":"boolean"},"read_only":true}));
    for &name in REPEATER_FIELDS {
        let domain = if name == "valid" {
            json!({"type":"boolean"})
        } else if name == "name" {
            json!({"type":"ASCII editor text","output_width":16})
        } else if repeater_text(name).is_some() {
            json!({"type":"byte-array","length":8,"element":{"min":0,"max":255}})
        } else if matches!(name, "latitude-bytes" | "longitude-bytes") {
            json!({"type":"byte-array","length":4,"element":{"min":0,"max":255}})
        } else if name == "utc-offset-minutes" {
            json!({"type":"i32","min":i32::MIN,"max":i32::MAX})
        } else {
            json!({"type":"u32","min":0,"max":u32::MAX})
        };
        fields.push(json!({"key":format!("record/repeater/INDEX/{name}"),"arguments":[{"name":"index","min":0,"max":2499}],"value":domain,"read_only":matches!(name,"valid"|"gateway-index"),"requires":"record-context; SYSTEM LANGID only on selector16","operation":if name=="mode"{"decoded repeater kind, not RF mode or registered Type-selector candidate"}else{"decoded working saver cutpoint"}}));
    }
    for (prefix, sheet) in [
        ("from-dv", Sheet::FromDv),
        ("from-fm", Sheet::FromFm),
        ("to", Sheet::To),
    ] {
        for &name in history_fields(sheet) {
            let domain = match name {
                "valid" | "supplemented" => json!({"type":"boolean"}),
                "call" => json!({"type":"stored-bytes","length":8}),
                "name" => json!({"type":"stored-bytes","length":16}),
                _ => json!({"type":"u32"}),
            };
            fields.push(json!({"key":format!("record/{prefix}/INDEX/{name}"),"arguments":[{"name":"index","min":0,"max":if sheet==Sheet::To{19}else{9}}],"value":domain,"read_only":true,"requires":"record-context; SYSTEM LANGID only on selector16"}));
        }
    }
    fields
}

fn address(sheet: Sheet, group: usize, index: usize) -> Result<(ChannelKind, u32)> {
    match sheet {
        Sheet::Call if group == 100 && index < 8 => Ok((ChannelKind::Call, index as u32)),
        Sheet::Memory if group < 100 && index < 200 => {
            Ok((ChannelKind::Normal, (200 * group + index) as u32))
        }
        _ => Err(AppError::invalid(
            "typed channel requires call group100/index0..7 or memory group0..99/index0..199",
        )),
    }
}
fn load(image: &Icf<'_>, kind: ChannelKind, index: u32) -> Result<DecodedRecord> {
    let mut holder = record::reset_holder(if kind == ChannelKind::Call { 3 } else { 2 }, index, 0);
    record::load_channel_image_into(image, kind, index, &mut holder)?;
    Ok(holder)
}
fn field(name: &str) -> Option<ChannelField> {
    Some(match name {
        "frequency-hz" => ChannelField::FrequencyHz,
        "duplex" => ChannelField::Duplex,
        "selection" => ChannelField::Selection,
        "tone" => ChannelField::Tone,
        "dtcs" => ChannelField::Dtcs,
        "polarity" => ChannelField::Polarity,
        "digital-squelch" => ChannelField::DigitalSquelch,
        "digital-code" => ChannelField::DigitalCode,
        _ => return None,
    })
}
fn call(name: &str) -> Option<MemoryCallsign> {
    match name {
        "your-call" => Some(MemoryCallsign::Your),
        "repeater1" => Some(MemoryCallsign::Repeater1),
        "repeater2" => Some(MemoryCallsign::Repeater2),
        _ => None,
    }
}
fn history_kind(sheet: Sheet) -> Option<u32> {
    match sheet {
        Sheet::FromDv => Some(0x0f),
        Sheet::FromFm => Some(0x10),
        Sheet::To => Some(0x12),
        Sheet::Rx => Some(0x0e),
        _ => None,
    }
}
fn value(holder: &DecodedRecord, name: &str) -> Result<Value> {
    if let Some(field) = field(name) {
        return Ok(json!(holder.channel_field(field)));
    }
    if let Some(which) = call(name) {
        return Ok(io::bytes_value(holder.callsign(which)));
    }
    Ok(match name {
        "valid" => json!(holder.valid()),
        "name" => io::bytes_value(holder.name()),
        "side" => json!(holder.side()),
        "split" => json!(holder.split()),
        "mode" => json!(holder.mode()),
        "data" => json!(holder.data()),
        "filter" => json!(holder.filter()),
        "offset-hz" => json!(holder.offset_hz()),
        "tx-tone" => json!(holder.tx_tone()),
        "rx-tone" => json!(holder.rx_tone()),
        _ => return Err(AppError::invalid("unknown decoded channel field")),
    })
}
pub(crate) fn channel_json(holder: &DecodedRecord) -> Result<Value> {
    let mut row = json!({"kind":holder.kind(),"index":holder.index(),"group":holder.group(),"representation":"decoded-saver-values"});
    for &name in CHANNEL_FIELDS {
        row[name] = value(holder, name)?;
    }
    Ok(row)
}

/// Typed side-view rows. Existing physical-memory/GPS views remain in sheets.
pub fn view(image: &Icf<'_>, sheet: Sheet, start: usize, count: usize) -> Result<Value> {
    if sheet == Sheet::Rx {
        let end = start
            .checked_add(count)
            .ok_or_else(|| AppError::invalid("Rx row range overflow"))?;
        if start > 50 {
            return Err(AppError::invalid("Rx row start outside list"));
        }
        return (start..end.min(50)).map(|index|{
            let h=load_rx(image,index)?;let mut row=json!({"index":index,"valid":h.valid(),"representation":"decoded-read-only-values"});
            if h.valid(){for(name,value)in record::receive_history_fields(&h){row[name]=history_value(value);}}
            Ok(row)
        }).collect::<Result<Vec<_>>>().map(Value::Array);
    }
    if sheet == Sheet::Ur {
        let end = start
            .checked_add(count)
            .ok_or_else(|| AppError::invalid("UR row range overflow"))?;
        if start > 300 {
            return Err(AppError::invalid("UR row start outside list"));
        }
        return (start..end.min(300)).map(|index| {
            let holder=load_ur(image,index)?;
            Ok(json!({"index":index,"coordinate":"raw-ur-list-slot","name":io::bytes_value(holder.name()),
                "call":io::bytes_value(holder.callsign(MemoryCallsign::Your))}))
        }).collect::<Result<Vec<_>>>().map(Value::Array);
    }
    let (kind, limit) = match sheet {
        Sheet::Call => (ChannelKind::Call, 8),
        Sheet::Memory => (ChannelKind::Normal, 20000),
        _ => {
            return Err(AppError::safety(
                "this record view requires its pending image adapter",
            ));
        }
    };
    let end = start
        .checked_add(count)
        .ok_or_else(|| AppError::invalid("row range overflow"))?;
    if start > limit {
        return Err(AppError::invalid("typed row start outside sheet"));
    }
    (start..end.min(limit))
        .map(|index| channel_json(&load(image, kind, index as u32)?))
        .collect::<Result<Vec<_>>>()
        .map(Value::Array)
}

fn path(key: &str) -> Result<Option<(ChannelKind, u32, &str)>> {
    if !key.starts_with("record/") {
        return Ok(None);
    }
    let parts: Vec<_> = key.split('/').collect();
    let (sheet, group, index, name) = match parts.as_slice() {
        ["record", "call", index, name] => {
            (Sheet::Call, 100, values::parse::<usize>(index)?, *name)
        }
        ["record", "memory", group, index, name] => (
            Sheet::Memory,
            values::parse::<usize>(group)?,
            values::parse::<usize>(index)?,
            *name,
        ),
        _ => {
            return Err(AppError::invalid(
                "record path requires call/INDEX/FIELD or memory/GROUP/INDEX/FIELD",
            ));
        }
    };
    let (kind, index) = address(sheet, group, index)?;
    Ok(Some((kind, index, name)))
}
fn ur_path(key: &str) -> Result<Option<(usize, &str)>> {
    if !key.starts_with("record/ur/") {
        return Ok(None);
    }
    let parts: Vec<_> = key.split('/').collect();
    match parts.as_slice() {
        ["record", "ur", index, name] => {
            let index = values::parse::<usize>(index)?;
            if index >= 300 || !matches!(*name, "name" | "call") {
                return Err(AppError::invalid(
                    "UR path requires INDEX0..299 and name/call",
                ));
            }
            Ok(Some((index, *name)))
        }
        _ => Err(AppError::invalid("UR path requires record/ur/INDEX/FIELD")),
    }
}
fn load_ur(image: &Icf<'_>, index: usize) -> Result<DecodedRecord> {
    let mut holder = record::reset_holder(0x0d, index as u32, 0);
    record::load_callsign_storage_into(image, index, &mut holder)?;
    Ok(holder)
}
fn ur_list_path(key: &str) -> Result<Option<(usize, &str)>> {
    if !key.starts_with("record/ur-list/") {
        return Ok(None);
    }
    let parts: Vec<_> = key.split('/').collect();
    match parts.as_slice() {
        ["record", "ur-list", index, name] => {
            let index = values::parse::<usize>(index)?;
            if index >= 300 || !matches!(*name, "name" | "call" | "valid") {
                return Err(AppError::invalid(
                    "UR list requires INDEX0..299 and name/call/valid",
                ));
            }
            Ok(Some((index, *name)))
        }
        _ => Err(AppError::invalid(
            "UR list path requires INDEX/FIELD or count",
        )),
    }
}
fn rx_path(key: &str) -> Result<Option<(usize, &str)>> {
    if !key.starts_with("record/rx/") {
        return Ok(None);
    }
    let parts: Vec<_> = key.split('/').collect();
    match parts.as_slice() {
        ["record", "rx", index, name] => {
            let index = values::parse::<usize>(index)?;
            if index >= 50 {
                return Err(AppError::invalid("Rx history index outside0..49"));
            }
            Ok(Some((index, *name)))
        }
        _ => Err(AppError::invalid("Rx path requires record/rx/INDEX/FIELD")),
    }
}
fn load_rx(image: &Icf<'_>, index: usize) -> Result<DecodedRecord> {
    let mut holder = record::reset_holder(0x0e, index as u32, 0);
    record::load_receive_history_storage_into(image, index as u32, &mut holder)?;
    Ok(holder)
}
fn history_value(value: record::HistoryValue<'_>) -> Value {
    match value {
        record::HistoryValue::Unsigned(n) => json!(n),
        record::HistoryValue::Signed(n) => json!(n),
        record::HistoryValue::Bytes(bytes) => io::bytes_value(bytes),
    }
}
pub fn get(image: &Icf<'_>, key: &str) -> Result<Option<Value>> {
    if key == "record/ur-list/count" {
        return Ok(Some(json!(
            cs705::icf::rows::WorkingUr::from_icf(image)?.count()
        )));
    }
    if let Some((index, name)) = ur_list_path(key)? {
        let working = cs705::icf::rows::WorkingUr::from_icf(image)?;
        let mut holder = record::reset_holder(0x0d, index as u32, 0);
        working.load_into(index as u32, &mut holder)?;
        return Ok(Some(match name {
            "valid" => json!(holder.valid()),
            "name" => io::bytes_value(holder.name()),
            _ => io::bytes_value(holder.callsign(MemoryCallsign::Your)),
        }));
    }
    if let Some((index, name)) = rx_path(key)? {
        let holder = load_rx(image, index)?;
        if name == "valid" {
            return Ok(Some(json!(holder.valid())));
        }
        let fields = record::receive_history_fields(&holder);
        let value = fields
            .into_iter()
            .find(|(field, _)| *field == name)
            .ok_or_else(|| AppError::invalid("unknown Rx history field"))?
            .1;
        return Ok(Some(if holder.valid() {
            history_value(value)
        } else {
            Value::Null
        }));
    }
    if let Some((index, name)) = ur_path(key)? {
        let holder = load_ur(image, index)?;
        return Ok(Some(io::bytes_value(if name == "name" {
            holder.name().as_slice()
        } else {
            holder.callsign(MemoryCallsign::Your).as_slice()
        })));
    }
    let Some((kind, index, name)) = path(key)? else {
        return Ok(None);
    };
    value(&load(image, kind, index)?, name).map(Some)
}
fn apply(holder: &mut DecodedRecord, name: &str, text: &str) -> Result<()> {
    if let Some(field) = field(name) {
        holder.set_channel_field(field, values::parse(text)?);
        return Ok(());
    }
    if let Some(which) = call(name) {
        holder.set_callsign(which, &values::parse::<[u8; 8]>(text)?);
        return Ok(());
    }
    match name {
        "name" => {
            let text = values::parse::<String>(text)?;
            if !text.is_ascii() {
                return Err(AppError::invalid(
                    "name requires explicit ASCII editor text; UTF-8 is not ACP",
                ));
            }
            let mut bytes = text.into_bytes();
            let name = normalizer::normalize_name(&mut bytes, 16);
            let mut fixed = [b' '; 16];
            fixed[..name.len()].copy_from_slice(&name);
            holder.set_name(&fixed);
        }
        "split" => holder.set_split(values::parse(text)?),
        "mode" => holder.set_mode(values::parse(text)?),
        "data" => holder.set_data(values::parse(text)?),
        "filter" => holder.set_filter(values::parse(text)?),
        "offset-hz" => holder.set_offset_hz(values::parse(text)?),
        "tx-tone" => holder.set_tx_tone(values::parse(text)?),
        "rx-tone" => holder.set_rx_tone(values::parse(text)?),
        _ => {
            return Err(AppError::invalid(
                "unknown or read-only decoded channel field",
            ));
        }
    }
    Ok(())
}

/// Accepted decoded saver fields. This does not impersonate mode/cache/G-driven
/// GUI editing. Empty normal registration requires an explicit library operation.
pub fn edit(image: &mut Icf<'_>, key: &str, text: &str) -> Result<bool> {
    if ["record/from-dv/", "record/from-fm/", "record/to/"]
        .iter()
        .any(|prefix| key.starts_with(prefix))
    {
        return Err(AppError::invalid("From/To history fields are read-only"));
    }
    if key == "record/ur-list/count" {
        return Err(AppError::invalid(
            "UR cached count is read-only; use an explicit row operation",
        ));
    }
    if let Some((index, name)) = ur_list_path(key)? {
        if name == "valid" {
            return Err(AppError::invalid("UR validity is read-only"));
        }
        let mut working = cs705::icf::rows::WorkingUr::from_icf(image)?;
        let mut holder = record::reset_holder(0x0d, index as u32, 0);
        working.load_into(index as u32, &mut holder)?;
        if !holder.valid() {
            return Err(AppError::safety(
                "UR logical edit does not implicitly insert a new row",
            ));
        }
        if name == "call" {
            holder.set_callsign(MemoryCallsign::Your, &values::parse::<[u8; 8]>(text)?);
        } else {
            apply(&mut holder, "name", text)?;
        }
        working.save(index as u32, &holder)?;
        image.patch_memory(&[(0, working.to_icf()?.memory())])?;
        return Ok(true);
    }
    if rx_path(key)?.is_some() {
        return Err(AppError::invalid(
            "Rx history fields are read-only; native typed saver writes no image fields",
        ));
    }
    if let Some((index, name)) = ur_path(key)? {
        let mut holder = load_ur(image, index)?;
        if name == "call" {
            holder.set_callsign(MemoryCallsign::Your, &values::parse::<[u8; 8]>(text)?);
        } else {
            apply(&mut holder, "name", text)?;
        }
        record::store_callsign_storage_into(image, index, &holder)?;
        return Ok(true);
    }
    let Some((kind, index, name)) = path(key)? else {
        return Ok(false);
    };
    let mut holder = load(image, kind, index)?;
    if !holder.valid() {
        return Err(AppError::safety(
            "empty normal registration needs the G-backed allocation/editor operation",
        ));
    }
    if kind == ChannelKind::Call && name == "selection" {
        return Err(AppError::invalid(
            "call selection is read-only; native call saver has no SEL store",
        ));
    }
    apply(&mut holder, name, text)?;
    record::store_channel_image_into(image, kind, index, &mut holder)?;
    Ok(true)
}

fn clipboard(holders: &[DecodedRecord], kind: u32) -> Value {
    json!({"version":1,"schema":"cs705.decoded616","kind":kind,
        "records":holders.iter().map(|h|json!({"bytes":h.as_bytes().as_slice()})).collect::<Vec<_>>()})
}
fn parse_clipboard(bytes: &[u8]) -> Result<Vec<DecodedRecord>> {
    let v: Value = serde_json::from_slice(bytes)
        .map_err(|_| AppError::invalid("invalid decoded clipboard JSON"))?;
    if !v.as_object().is_some_and(|o| o.len() == 4)
        || v["version"] != 1
        || v["schema"] != "cs705.decoded616"
    {
        return Err(AppError::invalid(
            "clipboard requires version1/schema/kind/records only",
        ));
    }
    let kind = v["kind"]
        .as_u64()
        .filter(|n| *n == 0x0c)
        .ok_or_else(|| AppError::invalid("decoded clipboard is reserved for repeater kind0C"))?
        as u32;
    let limit = 2500;
    let rows = v["records"]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= limit)
        .ok_or_else(|| AppError::invalid("clipboard record count outside source dataset"))?;
    rows.iter()
        .map(|v| {
            if !v
                .as_object()
                .is_some_and(|o| o.len() == 1 && o.contains_key("bytes"))
            {
                return Err(AppError::invalid(
                    "clipboard row requires only exact616 bytes",
                ));
            }
            let bytes = <[u8; 616] as values::ValueCodec>::parse(&v["bytes"])?;
            let holder = DecodedRecord::from_bytes(bytes);
            if holder.kind() != kind || holder.index() >= limit as u32 {
                return Err(AppError::invalid(
                    "clipboard header/side does not match declared source kind",
                ));
            }
            Ok(holder)
        })
        .collect()
}

/// History deletion only. Channel rows use their native v2 operation roots.
pub fn row(input: &str, sheet: Sheet, action: RowAction) -> Result<()> {
    let extra = action_extra(&action);
    io::single_stdin(std::iter::once(input).chain(extra))?;
    let kind = history_kind(sheet)
        .ok_or_else(|| AppError::safety("channel rows require their native operation router"))?;
    let RowAction::Delete {
        group,
        index,
        yes,
        write,
    } = action
    else {
        return Err(AppError::invalid(
            "history fields are read-only; its native row menu supports delete only",
        ));
    };
    if group != 0 {
        return Err(AppError::invalid("history has no named groups; use group0"));
    }
    let bytes = io::read(input)?;
    let mut image = Icf::parse(&bytes)?;
    if record::history_selection_present(&image, kind, index, index)? {
        confirm(yes)?;
    }
    let changed = record::delete_history_storage(&mut image, kind, index, index)?;
    io::report(
        &json!({"deleted":changed,"events":if changed{vec![0x11]}else{Vec::<u32>::new()},"physical_records_erased":false}),
    )?;
    io::publish(&image.to_bytes(), &write)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_holder_matches_the_getter_source_and_preserves_null_boundaries() {
        let mut memory = vec![0; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        memory[0x3ab82..0x3bf6e].fill(255);
        memory[0x3c0a7..0x3c2ff].fill(255);
        memory[0x3c33b..0x3c345].fill(255);
        memory[0x3c33b] = 0;
        memory[0x32985..0x3298d].copy_from_slice(b"UNKNOWN ");
        memory[0x3298d..0x32991].copy_from_slice(&123_456_789u32.to_le_bytes());
        let image = Icf::from_memory(memory.clone()).unwrap();
        let mut runtime = Options::default().load([]).unwrap();
        let mut q = [0; 24];
        q[0] = 1;
        runtime.context = Some(cs705::icf::context::Context::from_bytes(q));
        let repeaters = Working::from_icf(&image, runtime.defaults().unwrap()).unwrap();
        let ur = cs705::icf::rows::WorkingUr::from_icf(&image).unwrap();
        for index in 0..2 {
            let mut original = record::reset_holder(0x10, index as u32, 0);
            record::load_history_storage_into(&image, 0x10, index, &repeaters, &ur, &mut original)
                .unwrap();
            let shared = history_holder(&image, Sheet::FromFm, index, &runtime).unwrap();
            assert!(shared == original);
            assert_eq!(shared.valid(), index == 0);
            assert!(!shared.history_supplemented());
            for &name in history_fields(Sheet::FromFm) {
                assert_eq!(
                    history_get(&image, &format!("record/from-fm/{index}/{name}"), &runtime)
                        .unwrap(),
                    history_field_value(&shared, Sheet::FromFm, name).unwrap()
                );
            }
            assert_eq!(
                history_field_value(&shared, Sheet::FromFm, "name").unwrap(),
                Value::Null
            );
            assert_eq!(
                history_field_value(&shared, Sheet::FromFm, "frequency-hz").unwrap(),
                if index == 0 {
                    json!(123_456_789)
                } else {
                    Value::Null
                }
            );
        }
        assert_eq!(image.memory(), memory);
    }
    #[test]
    fn c38_full_dword_controls_canonical_phase1_and_keeps_raw_right_word_explicit() {
        let mut memory = vec![0; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        let mut h = record::reset_holder(3, 0, 0);
        h.set_valid(true);
        h.set_mode(16);
        h.set_channel_field(ChannelField::FrequencyHz, 145_000_000);
        cs705::icf::record_image::store_channel_into(
            &mut memory,
            ChannelKind::Call,
            0,
            &mut h,
            |_| Ok(()),
        )
        .unwrap();
        for (option, expected) in [(0, b" AB     "), (1, b"      AB"), (0x100, b"      AB")] {
            let runtime = Options::default().load([]).unwrap().with_input_options(
                cs705::display::InputOptions {
                    auto_correct: option,
                    ..Default::default()
                },
            );
            *runtime.normal.borrow_mut() = Some(
                initialize_normal_state(
                    &json!({"range_context":([0u8;24]),"range_selected":0,"range_langid":null})
                        .to_string(),
                )
                .unwrap(),
            );
            let mut image = Icf::from_memory(memory.clone()).unwrap();
            assert!(
                native_call_edit(
                    &mut image,
                    "record/native-call/0/your-call",
                    r#"{"input":" ab"}"#,
                    &runtime
                )
                .unwrap()
            );
            record::load_channel_image_into(&image, ChannelKind::Call, 0, &mut h).unwrap();
            assert_eq!(h.callsign(MemoryCallsign::Your), expected);
            let before = image.memory().to_vec();
            assert!(
                !native_call_edit(
                    &mut image,
                    "record/native-call/0/your-call",
                    r#"{"input":{"action":"cancel"}}"#,
                    &runtime
                )
                .unwrap()
            );
            assert_eq!(image.memory(), before);
            assert!(
                native_call_edit(
                    &mut image,
                    "record/native-call/0/your-call",
                    r#"{"input":" ab","right_word":false}"#,
                    &runtime
                )
                .is_err()
            );
            assert_eq!(image.memory(), before);
            assert!(native_call_edit(&mut image,"record/native-call/0/your-call",&json!({"edit":{"input":" ab"},"caller":{"code":0,"command":0,"state":2,"blocked":false,"node_index":0,"node_back":0}}).to_string(),&runtime).is_err());
            assert_eq!(image.memory(), before);
            assert!(
                native_call_edit(
                    &mut image,
                    "record/native-call/accepted/0/your-call",
                    r#"{"input":" ab","right_word":false}"#,
                    &runtime
                )
                .unwrap()
            );
            record::load_channel_image_into(&image, ChannelKind::Call, 0, &mut h).unwrap();
            assert_eq!(h.callsign(MemoryCallsign::Your), b" AB     ");
        }
    }
    #[test]
    fn decoded_clipboard_checks_exact_schema_header_width_and_preserves_residual_bytes() {
        let mut h = record::reset_holder(0x0c, 0, 0xdead_beef);
        h.set_mode(0x100);
        h.set_split(0xff);
        let good = clipboard(&[h.clone()], 0x0c);
        let parsed = parse_clipboard(&serde_json::to_vec(&good).unwrap()).unwrap();
        assert!(parsed[0] == h);
        let mut bad = good.clone();
        bad["extra"] = json!(true);
        assert!(parse_clipboard(&serde_json::to_vec(&bad).unwrap()).is_err());
        let mut bad = good.clone();
        bad["kind"] = json!(2);
        assert!(parse_clipboard(&serde_json::to_vec(&bad).unwrap()).is_err());
        let mut bad = good.clone();
        bad["records"][0]["bytes"].as_array_mut().unwrap().pop();
        assert!(parse_clipboard(&serde_json::to_vec(&bad).unwrap()).is_err());
        let mut bad = good;
        bad["records"][0]["bytes"][0] = json!(256);
        assert!(parse_clipboard(&serde_json::to_vec(&bad).unwrap()).is_err());
        assert!(address(Sheet::Call, 0, 0).is_err());
        assert!(address(Sheet::Call, 100, 8).is_err());
        assert!(address(Sheet::Memory, 100, 0).is_err());
    }

    #[test]
    fn native_clipboard_roundtrip_retains_unknown_bytes_and_refuses_empty_or_legacy() {
        use cs705::icf::rows::native::Clipboard;
        let mut h = record::reset_holder(3, 0, 0xdead_beef);
        h.set_mode(0x100);
        h.as_mut_bytes()[615] = 0xa5;
        let clipboard = Clipboard {
            source_kind: 3,
            mode: 2,
            declared_count: 1,
            logical_count: 1,
            records: vec![h.clone()],
        };
        let value = native_clipboard(&clipboard);
        let parsed = parse_native_clipboard(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(parsed.mode, 2);
        assert!(parsed.records[0] == h);
        let empty = Clipboard::default();
        assert!(
            parse_native_clipboard(&serde_json::to_vec(&native_clipboard(&empty)).unwrap())
                .is_err()
        );
        assert!(
            parse_native_clipboard(&serde_json::to_vec(&super::clipboard(&[h], 3)).unwrap())
                .is_err()
        );
    }

    #[test]
    fn field_metadata_covers_exact_recognized_names_and_decoded_domains() {
        let fields = fields();
        assert_eq!(
            fields.len(),
            2 * CHANNEL_FIELDS.len()
                + 3
                + record::receive_history_fields(&record::reset_holder(0x0e, 0, 0)).len()
                + REPEATER_FIELDS.len()
                + 4
                + 2 * FROM_FIELDS.len()
                + TO_FIELDS.len()
        );
        for prefix in ["record/call/INDEX/", "record/memory/GROUP/INDEX/"] {
            for &name in CHANNEL_FIELDS {
                let matches: Vec<_> = fields
                    .iter()
                    .filter(|v| v["key"] == format!("{prefix}{name}"))
                    .collect();
                assert_eq!(matches.len(), 1);
                let value = &matches[0]["value"];
                if field(name).is_some() {
                    assert_eq!(value["max"], u32::MAX);
                }
                if call(name).is_some() {
                    assert_eq!(value["length"], 8);
                }
            }
        }
        assert!(
            fields
                .iter()
                .any(|v| v["key"] == "record/call/INDEX/selection" && v["read_only"] == true)
        );
        assert!(
            fields
                .iter()
                .any(|v| v["key"] == "record/memory/GROUP/INDEX/selection"
                    && v["read_only"] == false)
        );
    }
}
