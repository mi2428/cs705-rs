//! Original byte-CSV roots. Library modules own grammar, holders and storage.
//! Publication is a separate boundary from an import's already-mutated prefix.

use crate::{AppError, Result, args::*, confirm, io, values::ValueCodec};
use cs705::{
    clone::IMAGE_CAPACITY,
    csv as data, gps,
    icf::{
        self, Icf,
        document::{Document, Session},
    },
    repeater,
};
use serde_json::{Value, json};

#[derive(clap::Args, Default)]
#[group(id = "CsvOptions")]
pub struct Options {
    /// Explicit portable context and optional captured native date tokens; never rows.
    #[arg(long)]
    pub csv_context: Option<String>,
    /// Actual file/confirmation boundary; inner cancellation still normalizes.
    #[arg(long, value_enum, default_value = "read")]
    pub csv_gate: Gate,
}

#[derive(clap::ValueEnum, Clone, Copy, Default, PartialEq, Eq)]
pub enum Gate {
    #[default]
    Read,
    InnerCancel,
    FileCancel,
}

fn invalid(error: impl std::fmt::Display) -> AppError {
    AppError::invalid(error.to_string())
}

pub(crate) struct Context(Value);
impl Context {
    pub(crate) fn read(path: &str) -> Result<Self> {
        let value: Value = serde_json::from_slice(&io::read(path)?)
            .map_err(|_| AppError::invalid("invalid CSV context JSON"))?;
        let object = value
            .as_object()
            .ok_or_else(|| AppError::invalid("CSV context requires an object"))?;
        for key in object.keys() {
            if ![
                "profile",
                "decimal",
                "rtl_decimal",
                "separator",
                "short_date",
                "date_separator",
                "time_separator",
                "current_year",
                "two_digit_window",
                "clock",
                "gps_hemispheres",
                "gps_group_name",
                "seed_icf",
                "range",
                "live_q_hex",
                "generation_q_hex",
                "system_langid",
                "crt_decimal",
                "repeater_group_name",
                "symbol_asset",
                "native_date_tokens",
                "native_date_order",
                "native_clock_date",
                "native_time_format",
                "native_decimal_copy",
                "document_generation",
                "document_generation_selector",
                "previous_reload_guard",
                "document_flags",
                "repeater_unwritten",
            ]
            .contains(&key.as_str())
            {
                return Err(AppError::invalid("unknown CSV context member"));
            }
        }
        let context = Self(value);
        if context.text("profile")? != "portable-ascii" {
            return Err(AppError::invalid("CSV profile requires portable-ascii"));
        }
        Ok(context)
    }
    fn text(&self, key: &str) -> Result<&str> {
        self.0[key]
            .as_str()
            .ok_or_else(|| AppError::invalid(format!("CSV context requires string {key}")))
    }
    pub(crate) fn byte(&self, key: &str) -> Result<u8> {
        match self.text(key)?.as_bytes() {
            [byte] if byte.is_ascii() => Ok(*byte),
            _ => Err(AppError::invalid(format!(
                "CSV context {key} requires one ASCII byte"
            ))),
        }
    }
    fn native_byte_value(value: &Value) -> Result<u8> {
        if let Value::String(text) = value {
            return match text.as_bytes() {
                [byte] if byte.is_ascii() => Ok(*byte),
                _ => Err(AppError::invalid(
                    "native byte requires one ASCII byte or a u8 value",
                )),
            };
        }
        u8::parse(value)
    }
    /// Native ANSI observations retain all byte values without widening CSV grammar.
    pub(crate) fn native_byte(&self, key: &str) -> Result<u8> {
        Self::native_byte_value(&self.0[key])
    }
    pub(crate) fn native_decimal_copy(&self, index: usize) -> Result<u8> {
        let value = &self.0["native_decimal_copy"];
        let value = if let Value::Array(reads) = value {
            reads
                .get(index)
                .ok_or_else(|| AppError::invalid("native copied decimal observations exhausted"))?
        } else {
            value
        };
        Self::native_byte_value(value)
    }
    fn word(&self, key: &str) -> Result<u16> {
        self.0[key]
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .ok_or_else(|| AppError::invalid(format!("CSV context requires u16 {key}")))
    }
    fn locale(&self) -> Result<data::Locale> {
        Ok(data::Locale::copied(
            self.byte("decimal")?,
            self.byte("separator")?,
        ))
    }
    fn q(&self, key: &str) -> Result<icf::context::Context> {
        let raw = io::decode_hex(self.text(key)?)?
            .try_into()
            .map_err(|_| AppError::invalid("CSV Q context requires exactly24 bytes"))?;
        Ok(icf::context::Context::from_bytes(raw))
    }
    fn langid(&self) -> Result<Option<u16>> {
        self.0
            .get("system_langid")
            .filter(|v| !v.is_null())
            .map(|_| self.word("system_langid"))
            .transpose()
    }
    fn repeater_defaults(&self) -> Result<repeater::working::DefaultContext> {
        Ok(icf::record::repeater_default_context(
            &self.q("live_q_hex")?,
            self.langid()?,
        )?)
    }
    fn session(&self) -> Result<Session> {
        let langid = self.langid()?;
        let flags = io::decode_hex(self.text("document_flags")?)?
            .try_into()
            .map_err(|_| AppError::invalid("CSV document_flags requires exactly two bytes"))?;
        let mut session = Session::new(
            self.q("live_q_hex")?,
            langid,
            u32::parse(&self.0["previous_reload_guard"])?,
            flags,
        );
        session.ranges = icf::frequency::Ranges::generate(&self.q("generation_q_hex")?, langid)?;
        session.adopt_captured_defaults(
            io::read(self.text("document_generation")?)?,
            u8::parse(&self.0["document_generation_selector"])?,
        )?;
        Ok(session)
    }
    fn name_seed(&self, key: &str) -> Result<&[u8; 16]> {
        let name = self.text(key)?.as_bytes();
        if name.len() != 16 || !name.iter().all(|b| (0x20..=0x7e).contains(b)) {
            return Err(AppError::invalid(format!(
                "CSV {key} requires16 printable ASCII bytes"
            )));
        }
        Ok(name.try_into().expect("checked seed width"))
    }
    pub(crate) fn datetime(&self) -> Result<data::datetime::Context<'_>> {
        let year = self.word("current_year")?;
        if !(1..=9999).contains(&year) {
            return Err(AppError::invalid("CSV current_year outside 1..9999"));
        }
        Ok(data::datetime::Context {
            short_date: self.text("short_date")?.as_bytes(),
            date_separator: self.byte("date_separator")?,
            time_separator: self.byte("time_separator")?,
            decimal_separator: self.byte("rtl_decimal")?,
            am: b"AM",
            pm: b"PM",
            current_year: year,
            two_digit_window: self.word("two_digit_window")?,
            primary_language: 9,
            eras: &[],
        })
    }
    /// Actual GUI date-order word, not inferred from the CSV short-date pattern.
    pub(crate) fn native_date_order(&self) -> Result<u32> {
        u32::parse(&self.0["native_date_order"])
    }
    /// GetLocalTime's actual calendar fields, acquired only by a reached caller.
    pub(crate) fn native_clock_date(&self) -> Result<data::datetime::Date> {
        let fields = self.0["native_clock_date"]
            .as_array()
            .filter(|fields| fields.len() == 3)
            .ok_or_else(|| AppError::invalid("native_clock_date requires year/month/day fields"))?;
        let date = data::datetime::Date {
            year: u16::parse(&fields[0])?,
            month: u8::parse(&fields[1])?,
            day: u8::parse(&fields[2])?,
        };
        if !data::datetime::valid_date(date) {
            return Err(AppError::invalid(
                "native_clock_date contains invalid calendar fields",
            ));
        }
        Ok(date)
    }
    pub(crate) fn native_time_format(&self) -> Result<&[u8]> {
        Ok(self.text("native_time_format")?.as_bytes())
    }
    pub(crate) fn native_clock(&self) -> Result<data::date_input::Clock> {
        let date = self.native_clock_date()?;
        let time = self.clock()?;
        Ok(data::date_input::Clock {
            year: date.year,
            month: u16::from(date.month),
            day: u16::from(date.day),
            hour: u16::from(time.hour),
            minute: u16::from(time.minute),
            second: u16::from(time.second),
            millisecond: time.millisecond,
        })
    }
    /// A legacy time-only context is not a complete GetLocalTime observation.
    pub(crate) fn optional_native_clock(&self) -> Result<Option<data::date_input::Clock>> {
        self.0
            .get("native_clock_date")
            .filter(|date| !date.is_null())
            .map(|_| self.native_clock())
            .transpose()
    }
    pub(crate) fn clock(&self) -> Result<data::datetime::Time> {
        data::datetime::parse_time(self.text("clock")?.as_bytes(), &self.datetime()?)
            .map_err(invalid)
    }
    pub(crate) fn native_date(
        &self,
        cursor: &mut usize,
        request: data::datetime::DateToken,
    ) -> std::result::Result<Vec<u8>, data::Error> {
        let captures = self
            .0
            .get("native_date_tokens")
            .ok_or(data::Error::SourceUnavailable(
                "native DateToken source unavailable",
            ))?
            .as_array()
            .ok_or(data::Error::Invalid("native_date_tokens requires an array"))?;
        let entry = captures
            .get(*cursor)
            .ok_or(data::Error::SourceUnavailable(
                "native DateToken capture exhausted",
            ))?
            .as_object()
            .filter(|entry| {
                entry.len() == 4
                    && entry
                        .keys()
                        .all(|key| ["date", "letter", "count", "output"].contains(&key.as_str()))
            })
            .ok_or(data::Error::Invalid(
                "native DateToken requires date/letter/count/output only",
            ))?;
        if entry["date"] != json!([request.date.year, request.date.month, request.date.day])
            || entry["letter"] != json!(char::from(request.letter))
            || entry["count"] != json!(request.count)
        {
            return Err(data::Error::Invalid("native DateToken request mismatch"));
        }
        let output = match &entry["output"] {
            Value::String(hex) => io::decode_hex(hex)
                .map_err(|_| data::Error::Invalid("native DateToken output hex"))?,
            Value::Array(bytes) => bytes
                .iter()
                .map(|byte| {
                    u8::parse(byte)
                        .map_err(|_| data::Error::Invalid("native DateToken output byte"))
                })
                .collect::<std::result::Result<Vec<_>, _>>()?,
            _ => {
                return Err(data::Error::Invalid(
                    "native DateToken output requires hex or byte array",
                ));
            }
        };
        *cursor += 1;
        Ok(output)
    }
    fn gps_options(&self) -> Result<gps::parser::Options<'_>> {
        let hemispheres = match self.text("gps_hemispheres")?.as_bytes() {
            [ns @ (b'N' | b'S'), ew @ (b'E' | b'W')] => [*ns, *ew],
            _ => {
                return Err(AppError::invalid(
                    "GPS hemispheres requires NE, NW, SE or SW",
                ));
            }
        };
        Ok(gps::parser::Options {
            locale: self.locale()?,
            datetime: self.datetime()?,
            hemispheres,
            group_name_seed: *self.name_seed("gps_group_name")?,
        })
    }
    fn row_range(&self, limit: u32, number_base: u32) -> Result<Option<data::channel::Selection>> {
        let Some(value) = self.0.get("range") else {
            return Ok(None);
        };
        let row = value
            .as_array()
            .filter(|a| a.len() == 2)
            .ok_or_else(|| AppError::invalid("CSV context range requires [first,last]"))?;
        let number = |i: usize| {
            row[i]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| AppError::invalid("CSV range requires unsigned row indices"))
        };
        let (first, last) = (number(0)?, number(1)?);
        if first > last || last >= limit {
            return Err(AppError::invalid("CSV context range outside sheet"));
        }
        Ok(Some(data::channel::Selection {
            first,
            last,
            number_base,
        }))
    }
    fn callsign_selection(&self, group: Option<usize>) -> Result<Option<data::channel::Selection>> {
        if group.is_some() {
            return Err(AppError::invalid(
                "UR has no named groups; use context range",
            ));
        }
        self.row_range(300, 1)
    }
    fn user_edge_selection(
        &self,
        group: Option<usize>,
    ) -> Result<(data::channel::Selection, bool)> {
        if group.is_some() {
            return Err(AppError::invalid(
                "user-edge has no named groups; use context range",
            ));
        }
        match self.row_range(30, 1)? {
            Some(selection) => Ok((selection, false)),
            None => Ok((
                data::channel::Selection {
                    first: 0,
                    last: 29,
                    number_base: 1,
                },
                true,
            )),
        }
    }
    fn scan_selection(&self, group: Option<usize>) -> Result<data::channel::Selection> {
        if group.is_some() {
            return Err(AppError::invalid(
                "scan has no named groups; use context range",
            ));
        }
        Ok(self.row_range(25, 0)?.unwrap_or(data::channel::Selection {
            first: 0,
            last: 24,
            number_base: 0,
        }))
    }
}

fn context(input: &str, csv: Option<&str>, options: &Options) -> Result<Context> {
    let path = options.csv_context.as_deref().ok_or_else(|| {
        AppError::invalid("CSV requires --csv-context with an explicit portable profile")
    })?;
    io::single_stdin(std::iter::once(input).chain(csv).chain([path]))?;
    let context = Context::read(path)?;
    let mut sources = ["seed_icf", "symbol_asset", "document_generation"]
        .into_iter()
        .filter(|key| context.0.get(*key).is_some())
        .map(|key| context.text(key))
        .collect::<Result<Vec<_>>>()?;
    if let Some(source) = context.0.get("repeater_unwritten") {
        sources.push(source["memory"].as_str().ok_or_else(|| {
            AppError::invalid("repeater_unwritten requires a memory byte-source path")
        })?);
    }
    io::single_stdin(
        std::iter::once(input)
            .chain(csv)
            .chain([path])
            .chain(sources),
    )?;
    Ok(context)
}

/// Apply the library's ordered storage-to-working roots, retaining error prefixes.
fn expand_current(current: &mut Vec<u8>, context: &Context) -> Result<()> {
    current.resize(IMAGE_CAPACITY, 0);
    icf::rows::expand_map(current)?;
    repeater::working::expand_counts(current)?;
    icf::rows::expand_ur_count(current)?;
    icf::rows::expand_names(current)?;
    repeater::working::expand_rows(current, context.repeater_defaults()?)?;
    icf::rows::expand_ur_data(current)?;
    Ok(())
}

fn header_gate(source: &[u8], sheet: Sheet, context: &Context) -> Result<bool> {
    use data::dispatch::{Binding, ImportKind};
    let kind = match sheet {
        Sheet::Memory => ImportKind::Memory,
        Sheet::Call => ImportKind::FixedPair,
        Sheet::Gps => ImportKind::Gps,
        Sheet::Repeater => ImportKind::Repeater,
        Sheet::Ur => ImportKind::Callsign,
        Sheet::Scan => ImportKind::Scan,
        _ => ImportKind::UserEdge,
    };
    let locale = context.locale()?;
    let Ok(reader) = data::Reader::new(source, locale) else {
        return Ok(false);
    };
    let required = kind.required_labels();
    Ok(Binding::builtin(reader.header(), locale.list, required, kind)?.accepted)
}

fn gps_scope(group: Option<usize>) -> Result<gps::transfer::Scope> {
    match group {
        None => Ok(gps::transfer::Scope::Whole),
        Some(g) if g < 27 => Ok(gps::transfer::Scope::Group(g)),
        _ => Err(AppError::invalid("GPS group outside 0..27")),
    }
}

fn unavailable(sheet: Sheet, operation: &str) -> AppError {
    AppError::safety(format!(
        "{sheet:?} CSV {operation}: original dispatch/library root is not yet connected; no data published"
    ))
}

fn repeater_scope(context: &Context, group: Option<usize>) -> Result<repeater::transfer::Scope> {
    if context.0.get("range").is_some() {
        return Err(AppError::invalid("Repeater uses --group, not a row range"));
    }
    match group {
        None => Ok(repeater::transfer::Scope::Whole),
        Some(g) if g < 50 => Ok(repeater::transfer::Scope::Group(g)),
        _ => Err(AppError::invalid("Repeater group outside 0..50")),
    }
}

fn import_repeater(
    image: &mut Icf<'_>,
    source: &[u8],
    group: Option<usize>,
    mode: ImportMode,
    context: &Context,
) -> Result<repeater::transfer::Import> {
    let scope = repeater_scope(context, group)?;
    let generation = context.q("generation_q_hex")?;
    let live = context.q("live_q_hex")?;
    let langid = context.langid()?;
    let ranges = icf::frequency::Ranges::generate(&generation, langid)?;
    let defaults = icf::record::repeater_default_context(&live, langid)?;
    let group = repeater::groups::Groups::from_icf(image)?.group_of(0) as u32;
    let holder = defaults.holder(group, 0);
    let default = repeater::staging::Record {
        frequency_class: ranges.f094(holder.channel_field(icf::record::ChannelField::FrequencyHz)),
        bytes: Box::new(*holder.as_bytes()),
    };
    let frequency = |hz| -> std::result::Result<repeater::parser::FrequencyProfile, data::Error> {
        let class = ranges.edf0(12, hz);
        let default_hz = icf::frequency::f20c(&ranges, langid, class).map_err(|_| {
            data::Error::SourceUnavailable("F20C captured B/class source unavailable")
        })?;
        Ok(repeater::parser::FrequencyProfile {
            identity_class: ranges.f094(hz),
            offset: repeater::finalize::OffsetProfile { class, default_hz },
        })
    };
    let datetime = context.datetime()?;
    let parser = repeater::parser::Options {
        locale: context.locale()?,
        datetime: &datetime,
        default: &default,
        types: repeater::fields::TypeSource {
            placeholder: *holder.repeater_text(icf::record::RepeaterText::Call),
            // CSV transform_type never reads the editor's gateway marker.
            gateway_marker: *holder.repeater_text(icf::record::RepeaterText::Gateway),
        },
        group_name_seed: *context.name_seed("repeater_group_name")?,
        crt_decimal: context.byte("crt_decimal")?,
        frequency: &frequency,
    };
    let seed_bytes = io::read(context.text("seed_icf")?)?;
    let seed = Icf::parse(&seed_bytes)?;
    let unwritten = context.0.get("repeater_unwritten");
    let mut captured_memory = None;
    let mut read_unwritten = |raw: u32, index: u32| {
        let source = unwritten.ok_or(data::Error::SourceUnavailable(
            "unwritten repeater byte source unavailable",
        ))?;
        let number = |key: &str| {
            u32::parse(&source[key])
                .map_err(|_| data::Error::Invalid("unwritten source requires u32 fields"))
        };
        let address = raw.wrapping_add(index.wrapping_mul(616));
        let offset = address
            .checked_sub(number("base")?)
            .ok_or(data::Error::SourceUnavailable(
                "unwritten source address precedes byte source",
            ))? as usize;
        if captured_memory.is_none() {
            captured_memory = Some(
                io::read(
                    source["memory"]
                        .as_str()
                        .ok_or(data::Error::Invalid("unwritten source memory path"))?,
                )
                .map_err(|_| {
                    data::Error::SourceUnavailable("unwritten repeater memory read failed")
                })?,
            );
        }
        let bytes: [u8; 616] = captured_memory
            .as_ref()
            .expect("source acquired")
            .get(
                offset
                    ..offset
                        .checked_add(616)
                        .ok_or(data::Error::Invalid("unwritten source address overflow"))?,
            )
            .ok_or(data::Error::SourceUnavailable(
                "unwritten repeater byte window unavailable",
            ))?
            .try_into()
            .expect("checked record width");
        let hz = u32::from_le_bytes(bytes[0x3c..0x40].try_into().expect("frequency width"));
        Ok(repeater::staging::Record {
            bytes: Box::new(bytes),
            frequency_class: ranges.f094(hz),
        })
    };
    let unwritten = unwritten
        .map(|source| -> Result<_> {
            let object = source
                .as_object()
                .filter(|object| {
                    object.len() == 6
                        && object.keys().all(|key| {
                            ["raw", "first", "last", "base", "read_budget", "memory"]
                                .contains(&key.as_str())
                        })
                })
                .ok_or_else(|| {
                    AppError::invalid(
                        "repeater_unwritten requires raw/first/last/base/read_budget/memory only",
                    )
                })?;
            let _ = u32::parse(&object["base"])?;
            Ok(repeater::transfer::UnwrittenSource {
                source: repeater::staging::PreserveSource {
                    raw: u32::parse(&object["raw"])?,
                    first: u32::parse(&object["first"])?,
                    last: u32::parse(&object["last"])?,
                    unwritten: true,
                },
                read_budget: source["read_budget"]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| {
                        AppError::invalid("unwritten read_budget requires an unsigned size")
                    })?,
                read: &mut read_unwritten
                    as &mut dyn FnMut(
                        u32,
                        u32,
                    ) -> std::result::Result<
                        repeater::staging::Record,
                        data::Error,
                    >,
            })
        })
        .transpose()?;
    let mut options = repeater::transfer::ImportOptions {
        mode: match mode {
            ImportMode::PreserveOrderUse => repeater::transfer::ImportMode::Merge,
            ImportMode::PreserveUse => repeater::transfer::ImportMode::PreserveUse,
            ImportMode::Replace => repeater::transfer::ImportMode::Replace,
            ImportMode::Cancel => unreachable!("cancel exits before reading CSV sources"),
        },
        parser: &parser,
        context: defaults,
        reset_words: [
            icf::record::repeater_reset_word(seed.memory(), 0)?,
            icf::record::repeater_reset_word(seed.memory(), 1)?,
        ],
        unwritten,
    };
    Ok(repeater::transfer::import_csv(
        image,
        scope,
        source,
        &mut options,
    ))
}

#[allow(clippy::too_many_arguments)]
pub fn import(
    input: &str,
    csv: &str,
    sheet: Sheet,
    group: Option<usize>,
    mode: ImportMode,
    accept_partial: bool,
    yes: bool,
    write: &Write,
    options: &Options,
) -> Result<u8> {
    if mode == ImportMode::Cancel || options.csv_gate == Gate::FileCancel {
        io::report(&json!({"cancelled":true,"changed":false,"published":false}))?;
        return Ok(0);
    }
    if options.csv_gate == Gate::Read {
        confirm(yes)?;
    }
    if matches!(sheet, Sheet::FromDv | Sheet::FromFm | Sheet::To | Sheet::Rx) {
        return Err(AppError::invalid(
            "original history CSV dispatch has NULL import callbacks; export only",
        ));
    }
    if matches!(sheet, Sheet::Bandstack | Sheet::Memopad | Sheet::Preset) {
        return Err(AppError::invalid(
            "original normal CSV UI does not support this sheet; slot0 default is not a registered parent",
        ));
    }
    let context = context(input, Some(csv), options)?;
    if !matches!(
        sheet,
        Sheet::Gps
            | Sheet::Memory
            | Sheet::Call
            | Sheet::UserEdge
            | Sheet::Ur
            | Sheet::Scan
            | Sheet::Repeater
    ) {
        return Err(unavailable(sheet, "import"));
    }
    if sheet != Sheet::Repeater && mode != ImportMode::Replace {
        return Err(AppError::invalid(
            "this sheet import has no order/USE preservation mode",
        ));
    }
    let bytes = io::read(input)?;
    let mut image = Icf::parse(&bytes)?;
    let source = if options.csv_gate == Gate::Read {
        io::read(csv)?
    } else {
        Vec::new()
    };
    let session = context.session()?;
    let mut current = image.memory().to_vec();
    expand_current(&mut current, &context)?;
    let mut document = Document::from_current(current, input.as_bytes().to_vec(), 6);
    let gate = if options.csv_gate == Gate::InnerCancel {
        data::control::ImportGate::InnerCancelled
    } else {
        data::control::ImportGate::Header(u8::from(header_gate(&source, sheet, &context)?))
    };
    let mut memory_effects = None;
    let mut reader_report = None;
    let mut reader_error = None;
    let result = document.import_csv(&session, gate, |document| {
        let read = (|| -> Result<_> {
    let report = if sheet == Sheet::Gps {
        if context.0.get("range").is_some() {
            return Err(AppError::invalid("GPS uses --group, not a user-edge range"));
        }
        let report = gps::transfer::import_csv(
            &mut image,
            gps_scope(group)?,
            &source,
            context.gps_options()?,
            false,
        );
        (
            report.counts,
            report.rows_read,
            report.applied,
            report.failure.as_ref().map(|f| format!("{f:?}")),
        )
    } else if matches!(sheet, Sheet::Memory | Sheet::Call) {
        let (kind, limit, base) = if sheet == Sheet::Memory {
            (data::channel::Kind::Memory, 100, 0)
        } else {
            (data::channel::Kind::FixedPair, 4, 1)
        };
        let generation = context
            .0
            .get("seed_icf")
            .map(|_| io::read(context.text("seed_icf")?))
            .transpose()?;
        let generation = generation.as_deref().map(Icf::parse).transpose()?;
        let q = context
            .0
            .get("generation_q_hex")
            .map(|_| context.q("generation_q_hex"))
            .transpose()?;
        let langid = context.langid()?;
        let ranges = q
            .as_ref()
            .map(|q| icf::frequency::Ranges::generate(q, langid))
            .transpose()?;
        let live = context.q("live_q_hex")?;
        let options = data::channel::MemoryOptions {
            locale: context.locale()?,
            crt_decimal: context.byte("crt_decimal")?,
            generation: generation.as_ref().map_or(&[], Icf::memory),
            ranges: ranges.as_ref(),
            live_q: &live,
            system_langid: langid,
        };
        let report = data::channel::import_memory(
            &mut image,
            &source,
            kind,
            group,
            context.row_range(limit, base)?,
            &options,
        );
        memory_effects = Some(
            json!({"home_invalidated":report.home_invalidated,"call_invalidated":report.call_invalidated}),
        );
        let report = report.report;
        (
            report.counts,
            report.rows_read,
            report.applied,
            report.failure.as_ref().map(|f| format!("{f:?}")),
        )
    } else if sheet == Sheet::Repeater {
        let report = import_repeater(&mut image, &source, group, mode, &context)?;
        (
            report.counts,
            report.rows_read,
            report.applied,
            report.failure.as_ref().map(|f| format!("{f:?}")),
        )
    } else if sheet == Sheet::Ur {
        let selection = context.callsign_selection(group)?;
        let seed = io::read(context.text("seed_icf")?)?;
        let defaults = Icf::parse(&seed)?;
        let report = data::channel::import_callsign(
            &mut image,
            &source,
            selection,
            context.locale()?,
            &defaults,
        );
        (
            report.counts,
            report.rows_read,
            report.applied,
            report.failure.as_ref().map(|f| format!("{f:?}")),
        )
    } else if sheet == Sheet::Scan {
        let seed = context
            .0
            .get("seed_icf")
            .map(|_| io::read(context.text("seed_icf")?))
            .transpose()?;
        let defaults = seed.as_deref().map(Icf::parse).transpose()?;
        let report = data::channel::import_scan(
            &mut image,
            &source,
            context.scan_selection(group)?,
            context.0.get("range").is_none(),
            context.locale()?,
            defaults.as_ref(),
        );
        (
            report.counts,
            report.rows_read,
            report.applied,
            report.failure.as_ref().map(|f| format!("{f:?}")),
        )
    } else {
        let (selection, whole) = context.user_edge_selection(group)?;
        let report = data::channel::import_user_edge(
            &mut image,
            &source,
            selection,
            whole,
            context.locale()?,
        );
        (
            report.counts,
            report.rows_read,
            report.applied,
            report.failure.as_ref().map(|f| format!("{f:?}")),
        )
    };
            Ok(report)
        })();
        let mut current = image.memory().to_vec();
        let expanded = expand_current(&mut current, &context);
        document.replace_current(current);
        match read {
            Ok(report) => {
                reader_report = Some(report);
                expanded.map_err(|error| {
                    reader_error = Some(error.message);
                    icf::Error::Invalid { line: 0, reason: "CSV expansion failed" }
                })
            }
            Err(error) => {
                reader_error = Some(error.message);
                Err(icf::Error::Invalid { line: 0, reason: "CSV reader failed" })
            }
        }
    });
    let reader_returned = reader_report.is_some();
    let (counts, rows_read, applied, mut failure) = reader_report.unwrap_or_default();
    let stage9 = match result {
        Ok((_, report)) => report,
        Err(error) => {
            let tail_error = reader_error.unwrap_or_else(|| error.to_string());
            failure = Some(match failure {
                Some(error) => format!("{error}; {tail_error}"),
                None => tail_error,
            });
            None
        }
    };
    if gate == data::control::ImportGate::Header(0) {
        let rejected = "CSV header rejected; row reader not entered";
        failure = Some(match failure {
            Some(error) => format!("{rejected}; {error}"),
            None => rejected.into(),
        });
    }
    let partial = failure.is_some() || counts.s != 0 || counts.c2 != 0;
    let changed = document.current() != document.baseline();
    let publish = gate.reaches_rows() && reader_returned && (!partial || accept_partial);
    io::report(&json!({
        "sheet":if sheet==Sheet::Gps {"gps"}else if sheet==Sheet::Memory{"memory"}else if sheet==Sheet::Call{"call"}else if sheet==Sheet::Ur{"ur"}else if sheet==Sheet::Scan{"scan"}else if sheet==Sheet::Repeater{"repeater"}else{"user-edge"}, "rows_read":rows_read, "applied":applied,
        "counts":{"s":counts.s,"c2":counts.c2,"c3":counts.c3,"c4":counts.c4},
        "failure":failure,
        "changed":changed, "partial":partial, "publication_authorized":publish,
        "dry_run":write.dry_run,
        "memory_effects":memory_effects,
        "cancelled":options.csv_gate==Gate::InnerCancel,
        "stage9":stage9.map(|report|json!({"typed_save_calls":report.typed.saved,"deleted_repeaters":report.deleted_repeaters})),
        "stage9_completed":stage9.is_some(),"reader_returned":reader_returned,
    }))?;
    if options.csv_gate == Gate::InnerCancel && !partial {
        return Ok(0);
    }
    if !publish {
        return Err(AppError::invalid(
            "CSV import incomplete; in-memory prefix not published; use --accept-partial explicitly",
        ));
    }
    // Use checked Document storage while retaining the input ICF envelope.
    let stored = document.current_bytes(&session)?;
    let stored = Icf::parse(&stored)?;
    image.patch_memory(&[(0, stored.memory())])?;
    io::publish(&image.to_bytes(), write)?;
    Ok(if partial { 5 } else { 0 })
}

pub fn export(
    input: &str,
    sheet: Sheet,
    group: Option<usize>,
    write: &Write,
    options: &Options,
) -> Result<()> {
    if options.csv_gate != Gate::Read {
        return Err(AppError::invalid("--csv-gate applies only to import"));
    }
    if sheet == Sheet::All {
        return Err(AppError::invalid(
            "CSV All has no single native schema; select one sheet",
        ));
    }
    if matches!(sheet, Sheet::Bandstack | Sheet::Memopad | Sheet::Preset) {
        return Err(AppError::invalid(
            "original normal CSV UI does not support this sheet; slot0 default is not a registered parent",
        ));
    }
    let context = context(input, None, options)?;
    if !matches!(
        sheet,
        Sheet::Gps
            | Sheet::Memory
            | Sheet::Call
            | Sheet::UserEdge
            | Sheet::Ur
            | Sheet::Scan
            | Sheet::Repeater
            | Sheet::FromDv
            | Sheet::FromFm
            | Sheet::To
            | Sheet::Rx
    ) {
        return Err(unavailable(sheet, "export"));
    }
    let locale = context.locale()?;
    let bytes = io::read(input)?;
    let image = Icf::parse(&bytes)?;
    let (bytes, rows, failure) = if sheet == Sheet::Gps {
        if context.0.get("range").is_some() {
            return Err(AppError::invalid("GPS uses --group, not a user-edge range"));
        }
        let mut cursor = 0;
        let report = gps::transfer::export_csv_formatted(
            &image,
            gps_scope(group)?,
            locale,
            &context.datetime()?,
            context.clock()?,
            |request| context.native_date(&mut cursor, request),
        );
        (
            report.bytes,
            report.rows,
            report.failure.as_ref().map(|f| format!("{f:?}")),
        )
    } else if matches!(sheet, Sheet::Memory | Sheet::Call) {
        if sheet == Sheet::Call && group.is_some() {
            return Err(AppError::invalid(
                "Call has no named groups; use context range",
            ));
        }
        let (kind, limit, base) = if sheet == Sheet::Memory {
            (data::channel::Kind::Memory, 200, 0)
        } else {
            (data::channel::Kind::FixedPair, 8, 1)
        };
        let report = data::channel::export_memory(
            &image,
            kind,
            group,
            context.row_range(limit, base)?,
            locale,
        );
        (
            report.bytes,
            report.rows,
            report.failure.as_ref().map(|f| format!("{f:?}")),
        )
    } else if sheet == Sheet::Ur {
        let report =
            data::channel::export_callsign(&image, context.callsign_selection(group)?, locale);
        (
            report.bytes,
            report.rows,
            report.failure.as_ref().map(|f| format!("{f:?}")),
        )
    } else if sheet == Sheet::Rx {
        if group.is_some() {
            return Err(AppError::invalid(
                "Rx has no named groups; use context rank range",
            ));
        }
        let symbol = context
            .0
            .get("symbol_asset")
            .map(|_| io::read(context.text("symbol_asset")?))
            .transpose()?;
        let mut cursor = 0;
        let report = data::channel::export_receive_history_with_date(
            &image,
            context.row_range(50, 0)?,
            locale,
            &context.datetime()?,
            symbol.as_deref(),
            |request| context.native_date(&mut cursor, request),
        );
        (
            report.bytes,
            report.rows,
            report.failure.as_ref().map(|f| format!("{f:?}")),
        )
    } else if matches!(sheet, Sheet::FromDv | Sheet::FromFm | Sheet::To) {
        if group.is_some() {
            return Err(AppError::invalid(
                "history has no named groups; use context range",
            ));
        }
        let (kind, limit) = match sheet {
            Sheet::FromDv => (0x0f, 10),
            Sheet::FromFm => (0x10, 10),
            _ => (0x12, 20),
        };
        let report = data::channel::export_history(
            &image,
            kind,
            context.row_range(limit, 0)?,
            locale,
            context.repeater_defaults()?,
        );
        (
            report.bytes,
            report.rows,
            report.failure.as_ref().map(|f| format!("{f:?}")),
        )
    } else if sheet == Sheet::Repeater {
        let report = repeater::transfer::export_csv(
            &image,
            repeater_scope(&context, group)?,
            locale,
            context.repeater_defaults()?,
        );
        (
            report.bytes,
            report.rows,
            report.failure.as_ref().map(|f| format!("{f:?}")),
        )
    } else if sheet == Sheet::Scan {
        let report = data::channel::export_scan(&image, context.scan_selection(group)?, locale);
        (
            report.bytes,
            report.rows,
            report.failure.as_ref().map(|f| format!("{f:?}")),
        )
    } else {
        let (selection, _) = context.user_edge_selection(group)?;
        let report = data::channel::export_user_edge(&image, selection, locale);
        (
            report.bytes,
            report.rows,
            report.failure.as_ref().map(|f| format!("{f:?}")),
        )
    };
    io::report(
        &json!({"sheet":match sheet {Sheet::Gps=>"gps",Sheet::Memory=>"memory",Sheet::Call=>"call",Sheet::Ur=>"ur",Sheet::Scan=>"scan",Sheet::Repeater=>"repeater",Sheet::FromDv=>"from-dv",Sheet::FromFm=>"from-fm",Sheet::To=>"to",Sheet::Rx=>"rx",_=>"user-edge"},"rows":rows,
        "failure":failure,"prefix_bytes":bytes.len(),"dry_run":write.dry_run}),
    )?;
    if failure.is_some() {
        return Err(AppError::invalid(
            "CSV export formatter failed; generated prefix not published",
        ));
    }
    io::publish(&bytes, write)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_clock_sources_never_invent_missing_calendar_fields() {
        let context = Context(json!({"clock":"12:34:56"}));
        assert!(context.optional_native_clock().unwrap().is_none());
        assert!(context.native_clock().is_err());
        let mut context = Context(json!({
            "current_year":2026,"short_date":"yyyy/mm/dd","date_separator":"/",
            "time_separator":":","rtl_decimal":".","two_digit_window":50,
            "clock":"12:34:56","native_clock_date":[2026,11,10],
            "native_date_order":4294967295u32,"native_time_format":"hh:nn:ss"
        }));
        let clock = context.optional_native_clock().unwrap().unwrap();
        assert_eq!((clock.year, clock.month, clock.day), (2026, 11, 10));
        assert_eq!((clock.hour, clock.minute, clock.second), (12, 34, 56));
        assert_eq!(context.native_date_order().unwrap(), u32::MAX);
        assert_eq!(context.native_time_format().unwrap(), b"hh:nn:ss");
        context.0["native_clock_date"] = json!([2026, 2, 30]);
        assert!(context.optional_native_clock().is_err());
        context.0["native_clock_date"] = json!(null);
        assert!(context.optional_native_clock().unwrap().is_none());
        context.0["rtl_decimal"] = json!(223);
        assert_eq!(context.native_byte("rtl_decimal").unwrap(), 223);
        assert!(context.byte("rtl_decimal").is_err());
        assert!(context.native_decimal_copy(0).is_err());
        context.0["native_decimal_copy"] = json!([44, "1", 0]);
        assert_eq!(context.native_decimal_copy(0).unwrap(), b',');
        assert_eq!(context.native_decimal_copy(1).unwrap(), b'1');
        assert_eq!(context.native_decimal_copy(2).unwrap(), 0);
        assert!(context.native_decimal_copy(3).is_err());
        context.0["native_decimal_copy"] = json!(".");
        assert_eq!(context.native_decimal_copy(99).unwrap(), b'.');
        context.0["native_decimal_copy"] = json!(256);
        assert!(context.native_decimal_copy(0).is_err());
    }
}
