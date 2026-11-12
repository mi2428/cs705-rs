//! Pure-file information/COM driver using software receivers and captured I/O.

use crate::{AppError, Result, args::Write, io, transport, values::ValueCodec};
use cs705::{
    clone::{self, information as lib, search},
    icf,
};
use icf::document::{
    self,
    lifecycle::{Lifecycle, RecentFiles, UiState},
};
use serde_json::{Map, Value, json};

fn captions() -> [Vec<u8>; 2] {
    [b"Device information".to_vec(), b"Stored label".to_vec()]
}

#[derive(clap::Args)]
pub struct Args {
    /// Source observations and controller events, or - for stdin.
    #[arg(long)]
    pub script: String,
    /// Restore only a saved controller state; never run its old requests again.
    #[arg(long)]
    pub state: Option<String>,
    /// Optional read-only ICF input; no controller event modifies it.
    #[arg(long)]
    pub image: Option<String>,
    #[command(flatten)]
    pub write: Write,
}

fn invalid(message: &str) -> AppError {
    AppError::invalid(message)
}
fn object<'a>(value: &'a Value, allowed: &[&str]) -> Result<&'a Map<String, Value>> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("information source requires object"))?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid("unknown information source member"));
    }
    Ok(object)
}
fn bytes(value: &Value) -> Result<Vec<u8>> {
    match value {
        Value::String(text) => io::decode_hex(text),
        Value::Array(list) => list.iter().map(u8::parse).collect(),
        _ => Err(invalid("information bytes require hex or byte array")),
    }
}
fn list(value: &Value) -> Result<&[Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| invalid("information source requires array"))
}
fn optional_list<T>(value: Option<&Value>, parse: impl Fn(&Value) -> Result<T>) -> Result<Vec<T>> {
    value
        .map(|v| list(v)?.iter().map(parse).collect())
        .unwrap_or_else(|| Ok(Vec::new()))
}
fn read_mode(value: Option<&Value>) -> lib::ReadMode {
    match value {
        None => lib::ReadMode::Current,
        Some(Value::Array(values)) => lib::ReadMode::Captured(
            values
                .iter()
                .map(|v| {
                    i32::parse(v)
                        .map_err(|_| clone::Error::Invalid("invalid captured read value".into()))
                })
                .collect(),
        ),
        Some(_) => lib::ReadMode::Captured(vec![Err(clone::Error::Invalid(
            "captured reads require an array".into(),
        ))]),
    }
}
fn native_returns(value: Option<&Value>) -> Result<Vec<lib::NativeReturn>> {
    optional_list(value, |v| {
        if v == "exception" {
            Ok(lib::NativeReturn::Exception)
        } else {
            Ok(lib::NativeReturn::Returned(i64::parse(v)?))
        }
    })
}
fn life(value: &Value) -> Result<lib::Life> {
    match value.as_str() {
        Some("absent") => Ok(lib::Life::Absent),
        Some("open") => Ok(lib::Life::Open),
        Some("release_requested") => Ok(lib::Life::ReleaseRequested),
        Some("destroyed") => Ok(lib::Life::Destroyed),
        _ => Err(invalid("invalid controller lifetime")),
    }
}
fn life_json(value: lib::Life) -> &'static str {
    match value {
        lib::Life::Absent => "absent",
        lib::Life::Open => "open",
        lib::Life::ReleaseRequested => "release_requested",
        lib::Life::Destroyed => "destroyed",
    }
}
fn candidate_json(value: &search::Candidate) -> Value {
    json!({"label":value.label,"number":value.number})
}

fn state_json(c: &lib::Controller) -> Value {
    json!({"com":c.com,"text":c.text,"com_mode":c.com_mode,"form":life_json(c.form),"popup":life_json(c.popup),"retired_popups":c.retired_popups.iter().copied().map(life_json).collect::<Vec<_>>(),
        "information":life_json(c.information),"pair_registered":c.pair_registered,"selector":c.selector,"fc":c.fc,"fd":c.fd,
        "identity_model":c.identity.cached_model,"identity_tail":c.identity.cached_tail.as_ref().map(|v|v.as_slice()),
        "identity_result":c.identity_result,"captions":c.captions,"dropdown":c.dropdown,
        "candidates":c.candidates.iter().map(candidate_json).collect::<Vec<_>>(),"generation":c.generation,"previous":c.previous,
        "selected":c.selected,"last_item_index":c.last_item_index,"search_result":c.search_result,"reidentify_requested":c.reidentify_requested})
}
fn restore(value: &Value) -> Result<lib::Controller> {
    let root = object(
        value,
        &[
            "version",
            "state",
            "events",
            "information_rows",
            "dropdown_text",
            "source_script",
            "read_only_image",
            "native_execution",
            "software",
        ],
    )?;
    if root.get("version") != Some(&json!(1)) {
        return Err(invalid("information state version must be1"));
    }
    if root.get("software").is_some_and(|v| !v.is_null()) {
        return Err(invalid(
            "software receivers cannot use a controller-only snapshot",
        ));
    }
    let v = value
        .get("state")
        .ok_or_else(|| invalid("information snapshot requires state"))?;
    let fields = [
        "com",
        "text",
        "com_mode",
        "form",
        "popup",
        "retired_popups",
        "information",
        "pair_registered",
        "selector",
        "fc",
        "fd",
        "identity_model",
        "identity_tail",
        "identity_result",
        "captions",
        "dropdown",
        "candidates",
        "generation",
        "previous",
        "selected",
        "last_item_index",
        "search_result",
        "reidentify_requested",
    ];
    if object(v, &fields)?.len() != fields.len() {
        return Err(invalid("incomplete captured information state"));
    }
    let mut c = lib::Controller::new(
        i32::parse(&v["com"])?,
        Vec::new(),
        [
            u8::parse(&v["selector"])?,
            u8::parse(&v["fc"])?,
            u8::parse(&v["fd"])?,
        ],
    );
    c.text = if v["text"].is_null() {
        None
    } else {
        Some(bytes(&v["text"])?)
    };
    c.com_mode = Option::<u8>::parse(&v["com_mode"])?;
    c.form = life(&v["form"])?;
    c.popup = life(&v["popup"])?;
    c.retired_popups = list(&v["retired_popups"])?
        .iter()
        .map(life)
        .collect::<Result<Vec<_>>>()?;
    c.information = life(&v["information"])?;
    c.pair_registered = bool::parse(&v["pair_registered"])?;
    c.selector = u8::parse(&v["selector"])?;
    c.fc = u8::parse(&v["fc"])?;
    c.fd = u8::parse(&v["fd"])?;
    c.identity.cached_model = Option::<u16>::parse(&v["identity_model"])?;
    c.identity.cached_tail = if v["identity_tail"].is_null() {
        None
    } else {
        Some(
            bytes(&v["identity_tail"])?
                .try_into()
                .map_err(|_| invalid("captured identity tail requires33 bytes"))?,
        )
    };
    c.identity_result = Option::<u8>::parse(&v["identity_result"])?;
    c.captions = if v["captions"].is_null() {
        None
    } else {
        let _: [Vec<u8>; 2] = list(&v["captions"])?
            .iter()
            .map(bytes)
            .collect::<Result<Vec<_>>>()?
            .try_into()
            .map_err(|_| invalid("captured captions require two strings"))?;
        Some(captions())
    };
    c.dropdown = list(&v["dropdown"])?
        .iter()
        .map(i32::parse)
        .collect::<Result<Vec<_>>>()?;
    c.candidates = list(&v["candidates"])?
        .iter()
        .map(|v| {
            object(v, &["label", "number"])?;
            Ok(search::Candidate {
                label: bytes(&v["label"])?,
                number: u32::parse(&v["number"])?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    c.generation = v["generation"]
        .as_u64()
        .ok_or_else(|| invalid("captured generation requires u64"))?;
    c.previous = u32::parse(&v["previous"])?;
    c.selected = Option::<usize>::parse(&v["selected"])?;
    c.last_item_index = Option::<i32>::parse(&v["last_item_index"])?;
    c.search_result = i32::parse(&v["search_result"])?;
    c.reidentify_requested = bool::parse(&v["reidentify_requested"])?;
    if c.com_mode.is_some_and(|m| m != 0)
        || c.selected.is_some_and(|i| i >= c.candidates.len())
        || (c.pair_registered && c.form != lib::Life::Open)
    {
        return Err(invalid("inconsistent captured information state"));
    }
    for candidate in &c.candidates {
        search::label_number(&candidate.label)?;
    }
    c.information_rows()?;
    Ok(c)
}

struct RegistryData {
    values: Vec<std::result::Result<Vec<u8>, clone::Error>>,
    point: Option<u8>,
}
impl RegistryData {
    fn parse(v: &Value) -> Result<Self> {
        object(v, &["profile", "crt_point", "opened", "values"])?;
        let mut values = Vec::new();
        if bool::parse(&v["opened"])? {
            for entry in list(&v["values"])? {
                object(entry, &["name", "opened", "exists", "text", "exception"])?;
                if entry.get("exception") == Some(&Value::Bool(true)) {
                    values.push(Err(clone::Error::Invalid(
                        "captured registry read exception".into(),
                    )));
                    continue;
                }
                bytes(&entry["name"])?;
                let text = if bool::parse(&entry["opened"])? && bool::parse(&entry["exists"])? {
                    bytes(&entry["text"])?
                } else {
                    Vec::new()
                };
                values.push(Ok(text));
            }
        }
        let point =
            if values.first().is_some_and(|value| value.is_ok()) && v["profile"] == "ascii-c" {
                v.get("crt_point")
                    .map(|v| {
                        let point = String::parse(v)?;
                        match point.as_bytes() {
                            [byte] if byte.is_ascii() => Ok(*byte),
                            _ => Err(invalid("CRT point requires one ASCII byte")),
                        }
                    })
                    .transpose()?
            } else {
                None
            };
        Ok(Self { values, point })
    }
    fn source(&self) -> lib::RegistrySource<'_> {
        lib::RegistrySource {
            values: &self.values,
            crt_point: self.point,
        }
    }
}
struct SearchData {
    ready: u8,
    opened: Option<bool>,
    filters: Vec<search::Filter>,
    observations: Vec<search::Observation>,
    prefix: Vec<u8>,
    suffixes: Vec<Vec<u8>>,
}
impl SearchData {
    fn parse(v: &Value) -> Result<Self> {
        object(
            v,
            &[
                "ready",
                "opened",
                "filters",
                "observations",
                "prefix",
                "suffixes",
            ],
        )?;
        let mut out = Self {
            ready: u8::parse(&v["ready"])?,
            opened: None,
            filters: Vec::new(),
            observations: Vec::new(),
            prefix: Vec::new(),
            suffixes: Vec::new(),
        };
        if out.ready != 1 {
            return Ok(out);
        }
        out.opened = v.get("opened").map(bool::parse).transpose()?;
        if out.opened != Some(true) {
            return Ok(out);
        }
        out.prefix = bytes(&v["prefix"])?;
        out.suffixes = list(&v["suffixes"])?
            .iter()
            .map(bytes)
            .collect::<Result<Vec<_>>>()?;
        for f in list(&v["filters"])? {
            object(f, &["hub", "audio", "com", "id", "cid"])?;
            out.filters.push(search::Filter::from_ini(
                &bytes(&f["hub"])?,
                &bytes(&f["audio"])?,
                &bytes(&f["com"])?,
                &bytes(&f["id"])?,
                &bytes(&f["cid"])?,
            )?);
        }
        for o in list(&v["observations"])? {
            object(
                o,
                &[
                    "key",
                    "base",
                    "com",
                    "hub",
                    "audio",
                    "parent_path",
                    "ports",
                    "wave_in",
                    "wave_out",
                ],
            )?;
            let mut ports = Vec::new();
            for p in list(&o["ports"])? {
                object(
                    p,
                    &[
                        "instance_id",
                        "parent_id",
                        "prefix",
                        "mi",
                        "number",
                        "description",
                    ],
                )?;
                ports.push(search::Port {
                    instance_id: bytes(&p["instance_id"])?,
                    parent_id: bytes(&p["parent_id"])?,
                    prefix: bytes(&p["prefix"])?,
                    mi: i32::parse(&p["mi"])?,
                    number: u32::parse(&p["number"])?,
                    description: bytes(&p["description"])?,
                });
            }
            out.observations.push(search::Observation {
                key: bytes(&o["key"])?,
                base: bytes(&o["base"])?,
                com: bytes(&o["com"])?,
                hub: bytes(&o["hub"])?,
                audio: bytes(&o["audio"])?,
                parent_path: bool::parse(&o["parent_path"])?,
                ports,
                wave_in: i32::parse(&o["wave_in"])?,
                wave_out: i32::parse(&o["wave_out"])?,
            });
        }
        Ok(out)
    }
    fn source(&self) -> lib::SearchSource<'_> {
        lib::SearchSource {
            ready: self.ready,
            opened: self.opened,
            filters: &self.filters,
            observations: &self.observations,
            prefix: &self.prefix,
            suffixes: &self.suffixes,
        }
    }
}
fn context_fields(v: &Value) -> Result<clone::ContextFields> {
    Ok(icf::context::Context::from_bytes(
        bytes(v)?
            .try_into()
            .map_err(|_| invalid("Q source requires24 bytes"))?,
    )
    .fields())
}

struct Software {
    document: document::Document,
    session: document::Session,
    initial: document::InitialState,
    lifecycle: Lifecycle,
}
impl Software {
    fn new(v: &Value, c: &lib::Controller) -> Result<Self> {
        object(
            v,
            &[
                "current",
                "path",
                "q",
                "system_langid",
                "previous_reload_guard",
                "document_flags",
                "visible57",
            ],
        )?;
        let current = match v.get("current") {
            Some(path) => io::read(&String::parse(path)?)?,
            None => vec![0; clone::IMAGE_CAPACITY],
        };
        if current.len() != clone::IMAGE_CAPACITY {
            return Err(invalid("software current requires0x4997a bytes"));
        }
        let q = match v.get("q") {
            Some(q) => icf::context::Context::from_bytes(
                bytes(q)?
                    .try_into()
                    .map_err(|_| invalid("software Q requires24 bytes"))?,
            ),
            None => icf::context::Context::before_registry(),
        };
        let path = v.get("path").map(bytes).transpose()?.unwrap_or_default();
        let document = document::Document::from_current(current, path, 6);
        let session = document::Session::new(
            q,
            v.get("system_langid")
                .map(Option::<u16>::parse)
                .transpose()?
                .flatten(),
            u32::parse(&v["previous_reload_guard"])?,
            <[u8; 2]>::parse(&v["document_flags"])?,
        );
        let recent = RecentFiles::default();
        let mut ui = UiState::headless();
        if let Some(visible) = v.get("visible57") {
            ui.form.as_mut().unwrap().visible57 = u8::parse(visible)?;
        }
        ui.operation40 = u32::from(c.selector);
        ui.fc = c.fc;
        ui.fd = c.fd;
        Ok(Self {
            document,
            session,
            initial: document::InitialState::default(),
            lifecycle: Lifecycle { ui, recent },
        })
    }
    fn receiver(&mut self) -> lib::DocumentReceiver<'_> {
        lib::DocumentReceiver {
            document: &mut self.document,
            session: &mut self.session,
            initial: &mut self.initial,
            ui: &mut self.lifecycle.ui,
            recent: &self.lifecycle.recent,
        }
    }
    fn query(&mut self, c: &mut lib::Controller, v: &Value) -> Result<()> {
        object(v, &["mock", "profile_return", "defaults", "close_returns"])?;
        let mut transport = transport::Mock::from_file(&String::parse(&v["mock"])?)?;
        let outcome = c
            .query_document(
                clone::SessionConfig::default(),
                &mut transport,
                || false,
                &mut self.receiver(),
            )
            .map_err(|e| invalid(&e.to_string()))?;
        let request = if outcome.failure.is_none() {
            document::InitialRequest::information(
                &self.session,
                outcome
                    .identity
                    .as_ref()
                    .ok_or_else(|| invalid("successful query has no identity"))?,
                0,
            )?
        } else {
            None
        };
        let visible = self
            .lifecycle
            .ui
            .form
            .as_ref()
            .ok_or_else(|| invalid("main form is absent"))?
            .visible57;
        let profile_return = if request.is_some() && visible != 0 {
            v.get("profile_return").map(u32::parse).transpose()?
        } else {
            None
        };
        let defaults = if request.is_some()
            && (visible == 0 || profile_return.is_some_and(|n| n as u8 == 1))
        {
            v.get("defaults")
                .map(|v| DefaultData::read(v, outcome.identity.as_ref().unwrap(), self))
                .transpose()?
        } else {
            None
        };
        let close_returns = if outcome.failure.is_none() && visible != 1 && c.pair_registered {
            native_returns(v.get("close_returns"))?
        } else {
            Vec::new()
        };
        c.return_query(
            &outcome,
            &mut self.receiver(),
            lib::ReturnSource {
                defaults: defaults.as_ref().map(DefaultData::source),
                profile_return,
                captions: Some(captions()),
                close_returns: &close_returns,
            },
        )
        .map_err(|e| invalid(&e.to_string()))?;
        if let Some(failure) = outcome.failure {
            return Err(invalid(&format!("information query failed: {failure:?}")));
        }
        Ok(())
    }
    fn json(&self) -> Value {
        let ui = &self.lifecycle.ui;
        json!({"q":self.session.context.as_bytes(), "default_selector":self.session.default_selector(),
            "document":{"current_bytes":self.document.current().len(), "baseline_equal":self.document.current()==self.document.baseline(),
                "current_matches_defaults":self.session.defaults().is_ok_and(|g|g==self.document.current()),
                "path":self.document.path(),"dirty":self.document.dirty(),
                "pending_name":self.document.pending_name().map(|p|json!({"active":p.active,"index":p.index,"name":p.name}))},
            "ui":{"application5b":ui.application5b,"operation40":ui.operation40,"fc":ui.fc,"fd":ui.fd,
                "profile_ready":ui.profile_ready,"menu_items":ui.menu.len(),
                "form":ui.form.as_ref().map(|f|json!({"visible57":f.visible57,"enabled58":f.enabled58,"focused":f.focused})),
                "view_present":ui.view.is_some(),"tree_present":ui.tree.is_some(),
                "configuration":ui.configuration.map(|q|q.0),
                "title":ui.title.as_ref().map(|t|json!({"path":t.path,"dirty":t.dirty,"target":t.target}))}})
    }
}

struct DefaultData {
    selector: u8,
    prefix: Vec<u8>,
    japanese: Option<[u8; 120]>,
    restricted: Option<[u8; 108]>,
}
impl DefaultData {
    fn read(v: &Value, identity: &clone::Identity, software: &Software) -> Result<Self> {
        object(
            v,
            &["assets", "japanese_mode_pairs", "restricted_band_stack"],
        )?;
        let mut q = software.session.context;
        q.adopt(identity.context()?);
        let selector = q.source_selector(software.session.system_langid)?;
        let path = String::parse(&v["assets"][selector.to_string()])?;
        let read_array = |key: &str| -> Result<Vec<u8>> { io::read(&String::parse(&v[key])?) };
        Ok(Self {
            selector,
            prefix: io::read(&path)?,
            japanese: if selector == 0 {
                Some(
                    read_array("japanese_mode_pairs")?
                        .try_into()
                        .map_err(|_| invalid("Japanese source requires120 bytes"))?,
                )
            } else {
                None
            },
            restricted: if matches!(selector, 2 | 3) && q.effective_f()? == 0 {
                Some(
                    read_array("restricted_band_stack")?
                        .try_into()
                        .map_err(|_| invalid("restricted source requires108 bytes"))?,
                )
            } else {
                None
            },
        })
    }
    fn source(&self) -> document::DefaultSource<'_> {
        document::DefaultSource {
            selector: self.selector,
            prefix: &self.prefix,
            japanese_mode_pairs: self.japanese.as_ref(),
            restricted_band_stack: self.restricted.as_ref(),
        }
    }
}

fn apply(c: &mut lib::Controller, v: &Value, mut software: Option<&mut Software>) -> Result<()> {
    let action = v["action"]
        .as_str()
        .ok_or_else(|| invalid("information event requires action"))?;
    match action {
        "startup" => {
            object(v, &["action", "source"])?;
            if c.com == 0 {
                let s = SearchData::parse(&v["source"])?;
                c.startup(Some(&s.source()))?;
            } else {
                c.startup(None)?;
            }
        }
        "construct-com" => {
            object(v, &["action", "registry", "com_reads"])?;
            let r = RegistryData::parse(&v["registry"])?;
            c.construct_com(&r.source(), &read_mode(v.get("com_reads")))?;
        }
        "dropdown" => {
            object(v, &["action", "registry"])?;
            let r = RegistryData::parse(&v["registry"])?;
            c.refresh(&r.source())?;
        }
        "input" => {
            object(v, &["action", "text"])?;
            c.input_text(&bytes(&v["text"])?)?;
        }
        "open-search" | "rescan" => {
            object(v, &["action", "source"])?;
            let s = SearchData::parse(&v["source"])?;
            if action == "rescan" {
                c.rescan(&s.source())?;
            } else {
                c.open_search(&s.source())?;
            }
        }
        "select" => {
            object(v, &["action", "index", "index_reads"])?;
            if let Some(index) = v.get("index") {
                c.choose(i32::parse(index)?)?;
            }
            c.select(&read_mode(v.get("index_reads")))?;
        }
        "escape" => {
            object(v, &["action"])?;
            c.escape()?;
        }
        "popup-close" => {
            object(v, &["action"])?;
            c.popup_close()?;
        }
        "return-search" => {
            object(v, &["action", "registry"])?;
            let r = RegistryData::parse(&v["registry"])?;
            c.return_search(&r.source())?;
        }
        "manual-commit" => {
            object(v, &["action", "text", "busy_reads", "query"])?;
            let busy = optional_list(v.get("busy_reads"), bool::parse)?;
            let text = v.get("text").map(bytes).transpose()?;
            if let Some(software) = software.as_mut() {
                c.manual_commit_document(text.as_deref(), &busy, &mut software.receiver())?;
                if c.reidentify_requested {
                    software.query(c, &v["query"])?;
                }
            } else {
                if v.get("query").is_some() {
                    return Err(invalid("query requires software receivers"));
                }
                match text {
                    Some(text) => c.manual_commit(&text, &busy)?,
                    None => c.manual_commit_current(&busy)?,
                }
            }
        }
        "reidentify" => {
            object(v, &["action", "busy_reads", "query"])?;
            let software = software
                .as_mut()
                .ok_or_else(|| invalid("reidentify requires software receivers"))?;
            let busy = optional_list(v.get("busy_reads"), bool::parse)?;
            c.request_reidentify_document(&busy, &mut software.receiver())?;
            if c.reidentify_requested {
                software.query(c, &v["query"])?;
            }
        }
        "close" | "cancel" => {
            object(v, &["action", "returns"])?;
            let values = native_returns(v.get("returns"))?;
            if let Some(software) = software.as_mut() {
                c.close_com_document(action == "cancel", &values, &mut software.receiver())?;
            } else {
                c.close_com(action == "cancel", &values)?;
            }
        }
        "identify" => {
            if software.is_some() {
                return Err(invalid("software identify uses the Session query driver"));
            }
            object(
                v,
                &[
                    "action",
                    "wire",
                    "gate_q",
                    "gate_flag",
                    "display_q",
                    "display_flag",
                    "operation",
                ],
            )?;
            let clone::Frame::Response(identity) =
                clone::Frame::decode(&bytes(&v["wire"])?, clone::Direction::FromRadio)?
            else {
                return Err(invalid("information identify requires actual E1 frame"));
            };
            c.identify(
                &identity,
                lib::InformationSource {
                    gate_context: v.get("gate_q").map(context_fields).transpose()?,
                    gate_flag: u8::parse(&v["gate_flag"])?,
                    display_context: v.get("display_q").map(context_fields).transpose()?,
                    display_flag: u8::parse(&v["display_flag"])?,
                    operation: u32::parse(&v["operation"])?,
                    captions: Some(captions()),
                },
            )?;
        }
        "information-click" | "information-close" => {
            object(v, &["action"])?;
            c.information_close(action == "information-click")?;
        }
        "destroy-retired-search" => {
            object(v, &["action", "index"])?;
            c.destroy_retired_popup(usize::parse(&v["index"])?)?;
        }
        "destroy" => {
            object(v, &["action", "target"])?;
            let target = v["target"]
                .as_str()
                .ok_or_else(|| invalid("destroy requires target"))?;
            if let Some(software) = software.as_mut() {
                c.destroy_document(target, &mut software.receiver())?;
            } else {
                c.destroy(target)?;
            }
        }
        _ => {
            return Err(invalid(
                "unknown information action; device enumeration is not supported",
            ));
        }
    }
    Ok(())
}

pub fn run(args: Args) -> Result<u8> {
    io::single_stdin(
        std::iter::once(args.script.as_str())
            .chain(args.state.as_deref())
            .chain(args.image.as_deref()),
    )?;
    let source: Value = serde_json::from_slice(&io::read(&args.script)?)
        .map_err(|_| invalid("invalid information script JSON"))?;
    object(&source, &["initial", "events"])?;
    let mut inputs = vec![args.script.as_str()];
    inputs.extend(args.state.as_deref());
    inputs.extend(args.image.as_deref());
    inputs.extend(source["initial"]["software"]["current"].as_str());
    for event in list(&source["events"])? {
        inputs.extend(event["query"]["mock"].as_str());
        let defaults = &event["query"]["defaults"];
        inputs.extend(defaults["japanese_mode_pairs"].as_str());
        inputs.extend(defaults["restricted_band_stack"].as_str());
        if let Some(assets) = defaults["assets"].as_object() {
            inputs.extend(assets.values().filter_map(Value::as_str));
        }
    }
    io::single_stdin(inputs)?;
    let mut controller = if let Some(path) = &args.state {
        if source.get("initial").is_some() {
            return Err(invalid(
                "captured restore cannot also construct initial state",
            ));
        }
        restore(
            &serde_json::from_slice(&io::read(path)?)
                .map_err(|_| invalid("invalid information state JSON"))?,
        )?
    } else {
        object(&source["initial"], &["com", "text", "document", "software"])?;
        lib::Controller::new(
            i32::parse(&source["initial"]["com"])?,
            source["initial"]
                .get("text")
                .map(bytes)
                .transpose()?
                .unwrap_or_default(),
            <[u8; 3]>::parse(&source["initial"]["document"])?,
        )
    };
    let mut software = source["initial"]
        .get("software")
        .map(|v| Software::new(v, &controller))
        .transpose()?;
    let image = args.image.as_deref().map(io::read).transpose()?;
    let image = image.as_deref().map(icf::Icf::parse).transpose()?;
    let mut failure = None;
    for event in list(&source["events"])? {
        if let Err(error) = apply(&mut controller, event, software.as_mut()) {
            failure = Some(error);
            break;
        }
    }
    let rows=controller.information_rows()?.map(|rows|rows.map(|row|row.map(|cell|json!({"type":cell.kind,"key":cell.key,"span":cell.span,"selectable":cell.selectable,"text":cell.text}))));
    let value = json!({"version":1,"native_execution":false,"dropdown_text":controller.dropdown.iter().map(ToString::to_string).collect::<Vec<_>>(),"state":state_json(&controller),"events":controller.events.iter().map(|e|json!({"request":e.name,"argument":e.argument,"bytes":e.bytes})).collect::<Vec<_>>(),
        "information_rows":rows,"software":software.as_ref().map(Software::json),"source_script":source,"read_only_image":image.map(|i|json!({"bytes":i.memory().len(),"comment":i.comment_bytes(),"extra_data":i.extra_data()}))});
    if let Some(error) = failure {
        io::report(&json!({"failure":error.message,"prefix":value,"published":false}))?;
        return Err(error);
    }
    io::publish(&io::json_bytes(&value)?, &args.write)?;
    Ok(0)
}
