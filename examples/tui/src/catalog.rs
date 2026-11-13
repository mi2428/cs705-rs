//! UI projections over existing typed library APIs, never raw-address editing.

use crate::{
    Result,
    encoding::TextEncoding,
    model::{self, Editor},
};
use cs705::{
    icf::{
        self, Icf, record,
        settings::{self, digital, extended, network},
    },
    presentation::{Language, Presenter},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Memory,
    Repeater,
    GpsMemory,
    Ur,
    Calls,
    Display,
    Sound,
    Function,
    Scan,
    Bluetooth,
    Connections,
    Network,
    Cw,
    Rtty,
    Dv,
    Gps,
    Rf,
    Scope,
}

impl Category {
    pub fn title(self) -> &'static str {
        match self {
            Self::Memory => "Memory CH",
            Self::Repeater => "Repeater list",
            Self::GpsMemory => "GPS memory",
            Self::Ur => "Your call signs",
            Self::Calls => "My call / TX messages",
            Self::Display => "Display",
            Self::Sound => "Sound / Speech",
            Self::Function => "Function / Power",
            Self::Scan => "Scan",
            Self::Bluetooth => "Bluetooth",
            Self::Connections => "Connections",
            Self::Network => "WLAN / Network",
            Self::Cw => "CW / Keyer",
            Self::Rtty => "RTTY",
            Self::Dv => "DV / Gateway",
            Self::Gps => "GPS settings",
            Self::Rf => "RF / Filters",
            Self::Scope => "Scope",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Memory => "Channel names, RX frequencies and explicit copies",
            Self::Repeater => "Existing repeater records by group",
            Self::GpsMemory => "Stored names and coordinates by group",
            Self::Ur => "Destination callsigns and names",
            Self::Calls => "My callsigns, suffixes and TX messages",
            Self::Display => "Screen, meter, language and NTP settings",
            Self::Sound => "Beep and speech settings",
            Self::Function => "Power, recording and operating settings",
            Self::Scan => "Scan speed, pause and resume settings",
            Self::Bluetooth => "Headset, audio and serial settings",
            Self::Connections => "USB, CI-V and external connections",
            Self::Network => "WLAN, IPv4, radio names and user IDs",
            Self::Cw => "Keyer, pitch, speed and message memories",
            Self::Rtty => "RTTY decoding and operating settings",
            Self::Dv => "DV operation and gateway settings",
            Self::Gps => "GPS operation and units",
            Self::Rf => "RF, filter and AGC settings",
            Self::Scope => "Spectrum, waterfall and audio scope settings",
        }
    }
}

fn category(key: u16) -> Category {
    match key {
        0x120 => Category::Function,
        0x18d..=0x1b1 => Category::Dv,
        0x1b2..=0x20f | 0x229..=0x230 => Category::Gps,
        0x211..=0x228 | 0x273..=0x274 => Category::Display,
        0x231..=0x235 | 0x246..=0x24f => Category::Sound,
        0x29f..=0x2a5 => Category::Scan,
        0x2b1..=0x2bb => Category::Bluetooth,
        0x2c1..=0x2ce => Category::Cw,
        0x2d2..=0x2e3 => Category::Rtty,
        0x2e5..=0x315 => Category::Connections,
        0x316..=0x32f => Category::Network,
        0x330..=0x34d => Category::Rf,
        0x357..=0x371 => Category::Scope,
        _ => Category::Function,
    }
}

#[derive(Clone, Copy)]
pub enum Target {
    Setting(&'static settings::Setting, usize),
    Choice(&'static extended::ChoiceField, usize),
    GpsChoice(&'static settings::gps::ChoiceField),
    Digital(digital::TextField, usize),
    Text(extended::FixedTextField, usize),
    Network(network::Field),
    Prefix,
    Gps(usize),
    Repeater(usize),
    Ur(usize),
}

pub struct Entry {
    pub label: String,
    pub value: String,
    pub original: String,
    pub help: String,
    pub target: Target,
    pub editable: bool,
}

pub struct RowGroup {
    pub index: usize,
    pub name: String,
    pub slots: Vec<usize>,
}

pub fn groups(editor: &Editor, category: Category) -> Result<Vec<RowGroup>> {
    Ok(match category {
        Category::Memory => (0..editor.draft.group_count())
            .map(|index| RowGroup {
                index,
                name: editor.group(index),
                slots: Vec::new(),
            })
            .collect(),
        Category::Repeater => editor
            .settings
            .repeater_groups()?
            .iter()
            .enumerate()
            .map(|(index, group)| RowGroup {
                index,
                name: editor.text(group.name_bytes()),
                slots: group.physical_slots().to_vec(),
            })
            .collect(),
        Category::GpsMemory => editor
            .settings
            .gps_groups()?
            .iter()
            .enumerate()
            .map(|(index, group)| RowGroup {
                index,
                name: group
                    .name_bytes()
                    .map(|bytes| editor.text(bytes))
                    .unwrap_or_else(|| "Ungrouped".into()),
                slots: group.physical_slots().to_vec(),
            })
            .collect(),
        _ => return Err("category has no row groups".into()),
    })
}

pub fn grouped_entries(
    editor: &Editor,
    category: Category,
    group: Option<usize>,
) -> Result<Vec<Entry>> {
    let mut entries = entries(editor, category)?;
    if let Some(index) = group {
        let groups = groups(editor, category)?;
        let group = groups
            .iter()
            .find(|g| g.index == index)
            .ok_or("selected group is outside the current directory")?;
        entries.retain(|e| match e.target {
            Target::Repeater(slot) | Target::Gps(slot) => group.slots.contains(&slot),
            _ => false,
        });
    }
    Ok(entries)
}
impl Entry {
    pub fn changed(&self) -> bool {
        self.value != self.original
    }
}

pub struct Field {
    pub label: String,
    pub value: String,
    pub choices: Vec<String>,
    pub max_len: usize,
    pub choice_key: Option<u16>,
}
impl Field {
    pub fn text(label: impl Into<String>, value: impl Into<String>, max_len: usize) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            choices: Vec::new(),
            max_len,
            choice_key: None,
        }
    }
    fn choice(
        key: u16,
        label: impl Into<String>,
        value: i64,
        choices: impl IntoIterator<Item = i64>,
    ) -> Self {
        Self {
            label: label.into(),
            value: value.to_string(),
            choices: choices.into_iter().map(|n| n.to_string()).collect(),
            max_len: 20,
            choice_key: Some(key),
        }
    }

    pub fn display(&self) -> String {
        match self.choice_key {
            Some(key) => self
                .value
                .parse()
                .map(|n| selection_text(key, n))
                .unwrap_or_else(|_| self.value.clone()),
            None => self.value.clone(),
        }
    }
}

fn label(key: u16) -> Result<String> {
    Ok(Presenter::new(Language::English)
        .field_label(key)?
        .to_owned())
}
fn number(image: &Icf<'_>, s: &settings::Setting, index: usize) -> Result<i64> {
    let settings::Value::Number(n) = s.read(image, index)?;
    Ok(n)
}

pub fn entries(editor: &Editor, selected: Category) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    let image = &editor.settings;
    let original = &editor.base_settings;
    let mut push = |target, name: String, help: String, editable| -> Result<()> {
        entries.push(Entry {
            label: name,
            value: value(image, target, editor.encoding)?,
            original: value(original, target, editor.encoding)?,
            help,
            target,
            editable,
        });
        Ok(())
    };
    for s in settings::catalog()
        .iter()
        .filter(|s| category(s.key) == selected)
    {
        for index in 0..s.count {
            let name = indexed(label(s.key)?, index, s.count);
            let help = match &s.kind {
                settings::Kind::Selector => format!(
                    "Choose a value with Left/Right or Space. Stored codes {}..{}. Known semantic captions are shown; other codes are left explicit. Unknown current values are retained until an explicit edit.",
                    s.domain.min, s.domain.max
                ),
                settings::Kind::Integer(unit) => format!(
                    "{}..{} {unit}; step {}. Only an explicit valid replacement is stored.",
                    s.domain.min, s.domain.max, s.domain.step
                ),
            };
            push(
                Target::Setting(s, index),
                name,
                help,
                s.control_state(image, index, false)?.helper_enabled,
            )?;
        }
    }
    for s in extended::choice_fields()
        .iter()
        .chain(extended::rf_choice_fields())
        .filter(|s| category(s.key()) == selected)
    {
        for index in s.first_index..s.first_index + s.count {
            if !s.accepts_index(index) {
                continue;
            }
            let name = Presenter::new(Language::English).choice_label(s, index)?;
            let fixed = s.fixed_value(index)?.is_some();
            push(
                Target::Choice(s, index),
                indexed(name, index, s.count),
                if fixed {
                    "Fixed by this mode; not an editable value.".into()
                } else {
                    format!(
                        "Stored selection code {}..{}. Related field corrections are included in the draft.",
                        s.domain().min,
                        s.domain().max
                    )
                },
                !fixed,
            )?;
        }
    }
    if selected == Category::Gps {
        for s in settings::gps::choice_fields() {
            // A language-dependent direction list needs a real language source.
            let ready = s.selector(image, gps_resources()).is_ok();
            push(
                Target::GpsChoice(s),
                label(s.setting.key)?,
                if ready {
                    "GPS selection. Values are stored codes; no live positioning is started.".into()
                } else {
                    "Read only: this selector requires an actual language descriptor, not a guessed default.".into()
                },
                ready,
            )?;
        }
    }
    if selected == Category::Calls {
        for field in digital::TextField::ALL {
            for index in 0..field.count() {
                push(
                    Target::Digital(field, index),
                    indexed(label(field.key())?, index, field.count()),
                    format!(
                        "At most {} encoded bytes. Callsigns remain ASCII; TX messages use the selected encoding. Editing does not change the active selection.",
                        field.width()
                    ),
                    true,
                )?;
            }
        }
    }
    for (field, group) in [
        (extended::FixedTextField::NtpAddress, Category::Display),
        (extended::FixedTextField::Comment, Category::Function),
        (extended::FixedTextField::NetworkName, Category::Network),
        (extended::FixedTextField::Ssid, Category::Network),
        (
            extended::FixedTextField::NetworkRadioName,
            Category::Network,
        ),
        (extended::FixedTextField::User1Id, Category::Network),
        (extended::FixedTextField::User2Id, Category::Network),
        (extended::FixedTextField::GatewayAddress, Category::Dv),
        (extended::FixedTextField::GatewayCallsign, Category::Dv),
        (extended::FixedTextField::Keyer, Category::Cw),
    ] {
        if group != selected {
            continue;
        }
        for index in 0..field.count() {
            push(
                Target::Text(field, index),
                indexed(label(field.key())?, index, field.count()),
                format!(
                    "At most {} stored bytes. Free text uses the selected encoding; protocol tokens and keyers require ASCII. Unchanged bytes are preserved. Passwords are not edited here.",
                    field.width()
                ),
                true,
            )?;
        }
    }
    if selected == Category::Network {
        for field in [
            network::Field::Address,
            network::Field::Gateway,
            network::Field::PrimaryDns,
            network::Field::SecondaryDns,
        ] {
            push(Target::Network(field), Presenter::new(Language::English).network_label(field)?.to_owned(), "IPv4 address. Library subnet/gateway corrections are part of the same draft. No network connection is opened.".into(), true)?;
        }
        push(
            Target::Prefix,
            "Subnet prefix".into(),
            "CIDR prefix 1..30; address and gateway corrections are included in the draft.".into(),
            true,
        )?;
    }
    if selected == Category::GpsMemory {
        for (g, group) in image.gps_groups()?.iter().enumerate() {
            for (row, &slot) in group.physical_slots().iter().enumerate() {
                push(Target::Gps(slot), format!("{g:02}/{row:03} {}", group.name_bytes().map(|bytes| editor.text(bytes)).unwrap_or_else(|| "Ungrouped".into())), "Edit name and coordinates. Coordinate format: degrees minutes.mmm N/S or E/W, for example 35 40.123 N. Dates, UTC and altitude are preserved.".into(), true)?;
            }
        }
    }
    if selected == Category::Repeater {
        for (g, group) in image.repeater_groups()?.iter().enumerate() {
            for (row, &slot) in group.physical_slots().iter().enumerate() {
                let record = image
                    .physical_repeater(slot)?
                    .ok_or("repeater directory points to empty slot")?;
                let editable = matches!(record.mode, icf::RepeaterMode::Fm | icf::RepeaterMode::Dv)
                    && record.duplex != icf::RepeaterDuplex::Rps;
                push(Target::Repeater(slot), format!("{g:02}/{row:03} {}", editor.text(group.name_bytes())), "Edit name, sub-name and receive frequency of an existing row. Callsigns, duplex, tones and coordinates are preserved. FM-N, DD and RPS rows are read only.".into(), editable)?;
            }
        }
    }
    if selected == Category::Ur {
        for (index, slot) in digital::ur_physical_slots(image)?.into_iter().enumerate() {
            push(Target::Ur(slot), format!("Your call {:03}", index + 1), "Edit an existing destination callsign and name. Callsigns use A-Z, 0-9, slash and spaces; at most eight bytes. No list allocation or reference replacement.".into(), true)?;
        }
    }
    Ok(entries)
}

fn indexed(name: String, index: usize, count: usize) -> String {
    if count > 1 {
        format!("{name} [{}]", index + 1)
    } else {
        name
    }
}
fn gps_resources() -> settings::gps::Resources {
    settings::gps::Resources {
        system_langid: None,
        language_descriptor: None,
    }
}

pub fn value(image: &Icf<'_>, target: Target, encoding: TextEncoding) -> Result<String> {
    Ok(match target {
        Target::Setting(s, i) => match &s.kind {
            settings::Kind::Selector => selection_text(s.key, number(image, s, i)?),
            settings::Kind::Integer(unit) => format!("{} {unit}", number(image, s, i)?),
        },
        Target::Choice(s, i) => {
            if s.fixed_value(i)?.is_some() {
                Presenter::new(Language::English)
                    .choice(s, i, s.read(image, i)?)?
                    .text
            } else {
                selection_text(s.key(), s.read(image, i)?)
            }
        }
        Target::GpsChoice(s) => selection_text(s.setting.key, number(image, &s.setting, 0)?),
        Target::Digital(field, i) => encoding.display(&field.display(image, i)?),
        Target::Text(field, i) => encoding.display(field.raw(image, i)?),
        Target::Network(field) => network_value(image, field).to_string(),
        Target::Prefix => format!("/{}", image.network_configuration().prefix),
        Target::Gps(p) => format!(
            "{} | {} | {}",
            encoding.display(image.physical_gps_name_raw(p)?.ok_or("empty GPS row")?),
            coordinate_text(
                image
                    .physical_gps_latitude(p)?
                    .ok_or("missing GPS latitude")?,
                true
            ),
            coordinate_text(
                image
                    .physical_gps_longitude(p)?
                    .ok_or("missing GPS longitude")?,
                false
            )
        ),
        Target::Repeater(p) => {
            let r = image.physical_repeater(p)?.ok_or("empty repeater row")?;
            format!(
                "{} / {} | {} MHz | {:?} | {}",
                encoding.display(&r.name),
                encoding.display(&r.sub_name),
                model::mhz_text(r.frequency_hz),
                r.mode,
                encoding.display(&r.repeater_call)
            )
        }
        Target::Ur(p) => {
            let h = ur(image, p)?;
            format!(
                "{} | {}",
                encoding.display(h.callsign(icf::MemoryCallsign::Your)),
                encoding.display(h.name())
            )
        }
    })
}

fn network_value(image: &Icf<'_>, field: network::Field) -> std::net::Ipv4Addr {
    let c = image.network_configuration();
    match field {
        network::Field::Address => c.address,
        network::Field::Gateway => c.gateway,
        network::Field::PrimaryDns => c.primary_dns,
        network::Field::SecondaryDns => c.secondary_dns,
    }
}
fn ur(image: &Icf<'_>, p: usize) -> Result<record::DecodedRecord> {
    let mut h = record::reset_holder(0x0d, p as u32, 0);
    record::load_callsign_storage_into(image, p, &mut h)?;
    Ok(h)
}

pub fn fields(image: &Icf<'_>, target: Target, encoding: TextEncoding) -> Result<Vec<Field>> {
    Ok(match target {
        Target::Setting(s, i) => vec![match &s.kind {
            settings::Kind::Selector => Field::choice(
                s.key,
                "Selection",
                number(image, s, i)?,
                s.choices(image, i)?,
            ),
            settings::Kind::Integer(unit) => Field::text(
                format!("Value ({unit})"),
                number(image, s, i)?.to_string(),
                20,
            ),
        }],
        Target::Choice(s, i) => vec![Field::choice(
            s.key(),
            "Selection",
            s.read(image, i)?,
            s.domain().min..=s.domain().max,
        )],
        Target::GpsChoice(s) => vec![Field::choice(
            s.setting.key,
            "Selection",
            number(image, &s.setting, 0)?,
            s.setting.choices(image, 0)?,
        )],
        Target::Digital(f, i) => vec![Field::text(
            label(f.key())?,
            encoding.display(&f.display(image, i)?),
            f.width(),
        )],
        Target::Text(f, i) => vec![Field::text(
            label(f.key())?,
            encoding.display(f.raw(image, i)?),
            f.width(),
        )],
        Target::Network(f) => vec![Field::text(
            "IPv4 address",
            network_value(image, f).to_string(),
            15,
        )],
        Target::Prefix => vec![Field::text(
            "CIDR prefix (1..30)",
            image.network_configuration().prefix.to_string(),
            2,
        )],
        Target::Gps(p) => vec![
            Field::text(
                "Name (16 encoded bytes)",
                encoding.display(image.physical_gps_name_raw(p)?.ok_or("empty GPS row")?),
                16,
            ),
            Field::text(
                "Latitude (degrees minutes.mmm N/S)",
                coordinate_text(
                    image.physical_gps_latitude(p)?.ok_or("missing latitude")?,
                    true,
                ),
                24,
            ),
            Field::text(
                "Longitude (degrees minutes.mmm E/W)",
                coordinate_text(
                    image
                        .physical_gps_longitude(p)?
                        .ok_or("missing longitude")?,
                    false,
                ),
                24,
            ),
        ],
        Target::Repeater(p) => {
            let r = image.physical_repeater(p)?.ok_or("empty repeater row")?;
            vec![
                Field::text("Name (16 encoded bytes)", encoding.display(&r.name), 16),
                Field::text(
                    "Sub-name (8 encoded bytes)",
                    encoding.display(&r.sub_name),
                    8,
                ),
                Field::text(
                    "Receive frequency (MHz)",
                    model::mhz_text(r.frequency_hz),
                    14,
                ),
            ]
        }
        Target::Ur(p) => {
            let h = ur(image, p)?;
            vec![
                Field::text(
                    "Callsign (8 ASCII bytes)",
                    encoding.display(h.callsign(icf::MemoryCallsign::Your)),
                    8,
                ),
                Field::text("Name (16 encoded bytes)", encoding.display(h.name()), 16),
            ]
        }
    })
}

pub fn edit(editor: &mut Editor, target: Target, fields: &[Field]) -> Result<()> {
    let encoding = editor.encoding;
    let current = self::fields(&editor.settings, target, encoding)?;
    if fields.len() != current.len() {
        return Err("form fields do not match the selected setting".into());
    }
    // Enter/Ctrl+S without a replacement is not a native selected event. In
    // particular, do not validate unknown old codes or run linked corrections.
    if fields.iter().zip(&current).all(|(a, b)| a.value == b.value) {
        return Ok(());
    }
    editor.edit_settings(|image| {
        let v = &fields[0].value;
        match target {
            Target::Setting(s, i) => s.write(image, i, &settings::Value::Number(v.parse()?))?,
            Target::Choice(s, i) => {
                let value: i64 = v.parse()?;
                if !(s.domain().min..=s.domain().max).contains(&value) {
                    return Err("selection outside allowed domain".into());
                }
                let effect = s.edit(
                    image,
                    i,
                    extended::ChoiceInput::Selected(settings::Selection {
                        key: 1,
                        phase: 2,
                        candidate: value.try_into()?,
                        accepted: true,
                    }),
                )?;
                if !effect.stored {
                    return Err("choice is read only in the current context".into());
                }
            }
            Target::GpsChoice(s) => {
                let value: i64 = v.parse()?;
                if !s.setting.choices(image, 0)?.contains(&value) {
                    return Err("GPS selection outside allowed domain".into());
                }
                let effect = s.edit(
                    image,
                    extended::ChoiceInput::Selected(settings::Selection {
                        key: s.setting.selection_key,
                        phase: 2,
                        candidate: value.try_into()?,
                        accepted: true,
                    }),
                    gps_resources(),
                )?;
                if !effect.stored {
                    return Err("GPS field was not stored".into());
                }
            }
            Target::Digital(f, i) => {
                if v == &encoding.display(&f.display(image, i)?) {
                    return Ok(());
                }
                if f == digital::TextField::TxMessage {
                    f.store_encoded_message(image, i, &encoding.encode(v, f.width())?)?;
                } else {
                    ascii(v, f.width())?;
                    let mut bytes = v.as_bytes().to_vec();
                    let effect = f.edit(
                        image,
                        i,
                        extended::FixedTextInput::Text {
                            source: &mut bytes,
                            phase: 2,
                        },
                        0,
                    )?;
                    if !effect.stored {
                        return Err("digital text was not stored".into());
                    }
                    exact_text(v, &effect.display)?;
                }
            }
            Target::Text(f, i) => {
                if v == &encoding.display(f.raw(image, i)?) {
                    return Ok(());
                }
                if matches!(
                    f,
                    extended::FixedTextField::Comment
                        | extended::FixedTextField::Ssid
                        | extended::FixedTextField::NetworkRadioName
                ) {
                    f.store_encoded_text(image, i, &encoding.encode(v, f.width())?)?;
                } else {
                    ascii(v, f.width())?;
                    let mut bytes = v.as_bytes().to_vec();
                    let effect = extended::edit_fixed_text(
                        image,
                        f,
                        i,
                        extended::FixedTextInput::Text {
                            source: &mut bytes,
                            phase: 2,
                        },
                        0,
                    )?;
                    if !effect.text.stored {
                        return Err("text setting was not stored".into());
                    }
                    exact_text(v, &effect.text.display)?;
                }
            }
            Target::Network(f) => {
                let ip: std::net::Ipv4Addr = v.parse()?;
                if f == network::Field::Address && (ip.is_unspecified() || ip.is_broadcast()) {
                    return Err("host address cannot be unspecified or broadcast".into());
                }
                if !image
                    .edit_network_address(f, Some(ip.to_string().as_bytes()))
                    .stored
                {
                    return Err("network address was not stored".into());
                }
            }
            Target::Prefix => {
                let prefix: u8 = v.parse()?;
                if !(1..=30).contains(&prefix) {
                    return Err("prefix must be 1..30".into());
                }
                if !image
                    .select_network_prefix(2, 2, prefix.into(), true)
                    .stored
                {
                    return Err("network prefix was not stored".into());
                }
            }
            Target::Gps(p) => {
                let old: &[u8; 16] = image
                    .physical_gps_name_raw(p)?
                    .ok_or("empty GPS row")?
                    .try_into()
                    .expect("GPS name width");
                let name = encoding.fixed(v, old)?;
                image.set_physical_gps_name_bytes(p, &name)?;
                if fields[1].value != current[1].value {
                    image.set_physical_gps_latitude(p, coordinate(&fields[1].value, true)?)?;
                }
                if fields[2].value != current[2].value {
                    image.set_physical_gps_longitude(p, coordinate(&fields[2].value, false)?)?;
                }
            }
            Target::Repeater(p) => {
                let r = image.physical_repeater(p)?.ok_or("empty repeater row")?;
                let name = encoding.fixed(v, &r.name)?;
                let sub_name = encoding.fixed(&fields[1].value, &r.sub_name)?;
                image.set_physical_repeater_name_bytes(p, &name, &sub_name)?;
                if fields[2].value != current[2].value {
                    image.set_physical_repeater_frequency_hz(
                        p,
                        model::frequency(&fields[2].value)?,
                    )?;
                }
            }
            Target::Ur(p) => {
                let mut h = ur(image, p)?;
                let call = encoding.fixed(v, h.callsign(icf::MemoryCallsign::Your))?;
                if v != &current[0].value
                    && !call.iter().all(|b| {
                        b.is_ascii_uppercase() || b.is_ascii_digit() || matches!(b, b'/' | b' ')
                    })
                {
                    return Err("callsign requires A-Z, 0-9, slash or space".into());
                }
                let name = encoding.fixed(&fields[1].value, h.name())?;
                h.set_callsign(icf::MemoryCallsign::Your, &call);
                h.set_name(&name);
                record::store_callsign_storage_into(image, p, &h)?;
            }
        }
        Ok(())
    })?;
    let category = match target {
        Target::Setting(s, _) => category(s.key),
        Target::Choice(s, _) => category(s.key()),
        Target::GpsChoice(_) => Category::Gps,
        Target::Digital(..) => Category::Calls,
        Target::Text(field, _) => match field {
            extended::FixedTextField::NtpAddress => Category::Display,
            extended::FixedTextField::Comment => Category::Function,
            extended::FixedTextField::GatewayAddress
            | extended::FixedTextField::GatewayCallsign => Category::Dv,
            extended::FixedTextField::Keyer => Category::Cw,
            _ => Category::Network,
        },
        Target::Network(_) | Target::Prefix => Category::Network,
        Target::Gps(_) => Category::GpsMemory,
        Target::Repeater(_) => Category::Repeater,
        Target::Ur(_) => Category::Ur,
    };
    if !editor.edited_categories.contains(&category) {
        editor.edited_categories.push(category);
    }
    Ok(())
}

fn ascii(value: &str, max: usize) -> Result<()> {
    if value.len() > max || !value.bytes().all(|b| (32..=126).contains(&b)) {
        return Err(format!("at most {max} printable ASCII bytes required").into());
    }
    Ok(())
}

fn exact_text(input: &str, accepted: &[u8]) -> Result<()> {
    // Native editors sanitize and uppercase. This form promises strict input,
    // so reject those transformations inside Editor's rollback boundary.
    if input.as_bytes().trim_ascii_end() != accepted.trim_ascii_end() {
        return Err(
            "text would be normalized; enter its exact accepted spelling and characters".into(),
        );
    }
    Ok(())
}
fn coordinate_text(c: icf::GpsCoordinate, latitude: bool) -> String {
    format!(
        "{} {:02}.{:03} {}",
        c.degrees,
        c.thousandths_of_minute / 1000,
        c.thousandths_of_minute % 1000,
        match (latitude, c.negative) {
            (true, false) => "N",
            (true, true) => "S",
            (false, false) => "E",
            (false, true) => "W",
        }
    )
}
fn coordinate(value: &str, latitude: bool) -> Result<icf::GpsCoordinate> {
    let parts: Vec<_> = value.split_ascii_whitespace().collect();
    if parts.len() != 3 {
        return Err("coordinate requires degrees minutes.mmm hemisphere".into());
    }
    let degrees: u8 = parts[0].parse()?;
    let (minutes, fraction) = parts[1]
        .split_once('.')
        .ok_or("minutes require three decimal places")?;
    if fraction.len() != 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return Err("minutes require exactly three decimal digits".into());
    }
    let minutes: u16 = minutes.parse()?;
    if minutes >= 60 || degrees > if latitude { 90 } else { 180 } {
        return Err("coordinate is outside latitude/longitude bounds".into());
    }
    let thousandths = minutes * 1000 + fraction.parse::<u16>()?;
    if degrees == if latitude { 90 } else { 180 } && thousandths != 0 {
        return Err("pole/meridian endpoint requires zero minutes".into());
    }
    let negative = match (latitude, parts[2]) {
        (true, "N") | (false, "E") => false,
        (true, "S") | (false, "W") => true,
        _ => return Err("hemisphere must be N/S for latitude or E/W for longitude".into()),
    };
    Ok(icf::GpsCoordinate {
        degrees,
        thousandths_of_minute: thousandths,
        negative,
    })
}

pub fn changes(editor: &Editor) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for &category in &editor.edited_categories {
        for entry in entries(editor, category)?
            .into_iter()
            .filter(Entry::changed)
        {
            result.push(format!(
                "{} / {}\n  Original: {}\n  Draft: {}",
                category.title(),
                entry.label,
                entry.original,
                entry.value
            ));
        }
    }
    Ok(result)
}

/// Captions for stored semantics already defined in icf::common. Unknown or
/// uncatalogued code meanings are never guessed from a two-element domain.
fn selection_text(key: u16, code: i64) -> String {
    let captions: Option<&[&str]> = match key {
        0x212
        | 0x215
        | 0x217
        | 0x218
        | 0x220..=0x223
        | 0x226
        | 0x228
        | 0x232..=0x234
        | 0x23f
        | 0x241
        | 0x243
        | 0x244
        | 0x247..=0x24a
        | 0x255
        | 0x260
        | 0x264
        | 0x266..=0x268
        | 0x273
        | 0x279
        | 0x27f
        | 0x281
        | 0x284
        | 0x285
        | 0x291
        | 0x292
        | 0x296
        | 0x29d
        | 0x2a0
        | 0x2b1
        | 0x2b2
        | 0x2b6..=0x2b9 => Some(&["Off", "On"]),
        0x216 => Some(&["Power", "SWR", "ALC", "Compression", "Voltage", "Current"]),
        0x21d | 0x24d | 0x258 | 0x29f => Some(&["Slow", "Fast"]),
        0x21e | 0x21f => Some(&[
            "Off", "1 min", "2 min", "5 min", "15 min", "30 min", "60 min",
        ]),
        0x224 | 0x225 | 0x24b => Some(&["English", "Japanese"]),
        0x22a => Some(&[
            "Degrees / decimal minutes",
            "Degrees / minutes / seconds",
            "Decimal degrees",
        ]),
        0x22b => Some(&["Meters", "Feet / miles"]),
        0x22c => Some(&["km/h", "mph", "knots"]),
        0x22d => Some(&["Celsius", "Fahrenheit"]),
        0x22e => Some(&["hPa", "mbar", "mmHg", "inHg"]),
        0x22f => Some(&["Millimeters", "Inches"]),
        0x230 => Some(&["m/s", "km/h", "mph", "knots"]),
        0x235 => Some(&[
            "Off",
            "Default edges",
            "User edges",
            "User edges + TX limit",
        ]),
        0x23b => Some(&["Off", "30 min", "60 min", "90 min", "120 min"]),
        0x23c => Some(&["Off", "Auto short", "Auto medium", "Auto long"]),
        0x242 => Some(&["AH-705", "Other"]),
        0x246 => Some(&["Off", "Kerchunk", "All"]),
        0x24c => Some(&["Normal", "Phonetic"]),
        0x24f => Some(&["Speech then lock", "Lock then speech"]),
        0x250 => Some(&["Main dial", "Panel"]),
        0x252 => Some(&["Off", "Low", "High"]),
        0x259 => Some(&["Current group", "All groups"]),
        0x25a => Some(&["Registered only", "All groups"]),
        0x25f => Some(&["Off", "Short", "Medium", "Long"]),
        0x262 | 0x263 | 0x299 => Some(&["Wide", "Medium", "Narrow"]),
        0x274 => Some(&["PNG", "BMP"]),
        0x275 => Some(&["Ten-key", "Full keyboard"]),
        0x276 => Some(&["Japan / US / UK", "Germany", "France"]),
        0x27a => Some(&["Hand microphone", "Bluetooth microphone"]),
        0x27d => Some(&["Direct", "Monitor"]),
        0x27e => Some(&["Always", "Squelch linked"]),
        0x282 => Some(&["Off", "5 s", "10 s", "15 s"]),
        0x283 => Some(&["3 s", "5 s", "10 s", "30 s"]),
        0x286 | 0x288 => Some(&[
            "Comma / decimal dot",
            "Semicolon / decimal dot",
            "Semicolon / decimal comma",
        ]),
        0x287 | 0x289 => Some(&["Year-month-day", "Month-day-year", "Day-month-year"]),
        0x28d => Some(&["0.5 W", "1 W", "2.5 W", "5 W", "10 W"]),
        0x28e => Some(&["0.5 W", "1 W", "2.5 W", "5 W"]),
        0x290 => Some(&["Off", "3 min", "5 min", "10 min", "20 min", "30 min"]),
        0x298 => Some(&["Off", "Auto", "Manual"]),
        0x29b | 0x29c => Some(&["Auto", "Manual", "Auto / manual"]),
        0x29e => Some(&["LSB", "USB"]),
        0x2a1 => Some(&[
            "2 s", "4 s", "6 s", "8 s", "10 s", "12 s", "14 s", "16 s", "18 s", "20 s", "Hold",
        ]),
        0x2a2 => Some(&["0 s", "1 s", "2 s", "3 s", "4 s", "5 s", "Hold"]),
        0x2a3 => Some(&[
            "5 min",
            "10 min",
            "15 min",
            "While scanning",
            "Until power off",
        ]),
        0x2a5 => Some(&["Stop", "Up / down"]),
        0x2b3 => Some(&["Headset only", "Headset + speaker"]),
        0x2b4 => Some(&[
            "Off", "0 s", "1 s", "2 s", "3 s", "4 s", "5 s", "6 s", "7 s", "8 s", "9 s", "10 s",
        ]),
        0x2b5 => Some(&["Normal", "Microphone", "PTT"]),
        0x2bb => Some(&["CI-V without echo", "CI-V with echo", "DV data"]),
        0x323 => Some(&["Shutdown only", "Standby / shutdown"]),
        _ => None,
    };
    match captions {
        Some(captions) => usize::try_from(code)
            .ok()
            .and_then(|i| captions.get(i))
            .map(|s| format!("{s} ({code})"))
            .unwrap_or_else(|| format!("Unknown code {code}")),
        None => format!("Code {code}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tests::snapshot;

    #[test]
    fn unchanged_forms_preserve_unknown_values_and_linked_fields() {
        let mut editor = Editor::new(snapshot(6)).unwrap();
        for category in [
            Category::Display,
            Category::Sound,
            Category::Function,
            Category::Scan,
            Category::Bluetooth,
            Category::Connections,
            Category::Network,
            Category::Cw,
            Category::Rtty,
            Category::Dv,
            Category::Gps,
            Category::Rf,
            Category::Scope,
            Category::Calls,
        ] {
            for entry in entries(&editor, category)
                .unwrap()
                .into_iter()
                .filter(|e| e.editable)
            {
                let before = editor.settings.memory().to_vec();
                let fields = fields(&editor.settings, entry.target, editor.encoding).unwrap();
                edit(&mut editor, entry.target, &fields)
                    .unwrap_or_else(|error| panic!("{}: {error}", entry.label));
                assert_eq!(editor.settings.memory(), before, "{}", entry.label);
                assert!(!editor.dirty);
            }
        }
    }

    #[test]
    fn rejected_text_and_late_field_errors_do_not_commit_a_prefix() {
        for (target, text) in [
            (Target::Digital(digital::TextField::MyCall, 5), "bad!call"),
            (Target::Text(extended::FixedTextField::Keyer, 7), "A**"),
            (Target::Text(extended::FixedTextField::User1Id, 0), "A B"),
            (Target::Text(extended::FixedTextField::Ssid, 0), ""),
            (
                Target::Digital(digital::TextField::TxMessage, 4),
                "123456789012345678901",
            ),
        ] {
            let mut source = snapshot(6);
            source.native[0x39a07] = b'X';
            let mut editor = Editor::new(source).unwrap();
            let before = editor.settings.memory().to_vec();
            let mut fields = fields(&editor.settings, target, editor.encoding).unwrap();
            fields[0].value = text.into();
            assert!(edit(&mut editor, target, &fields).is_err());
            assert_eq!(editor.settings.memory(), before);
            assert!(!editor.dirty && editor.edited_categories.is_empty());
        }
    }

    #[test]
    fn reading_keyer_text_does_not_display_a_repaired_copy() {
        let mut source = snapshot(6);
        source.native[0x37158..0x37158 + 70].fill(b' ');
        source.native[0x37158..0x3715b].copy_from_slice(b"A**");
        let mut editor = Editor::new(source).unwrap();
        let target = Target::Text(extended::FixedTextField::Keyer, 0);
        let before = editor.settings.memory().to_vec();
        let fields = fields(&editor.settings, target, editor.encoding).unwrap();
        assert_eq!(fields[0].value, "A**");
        edit(&mut editor, target, &fields).unwrap();
        assert_eq!(editor.settings.memory(), before);
        assert!(!editor.dirty);
    }

    #[test]
    fn sparse_ur_edits_use_the_stored_physical_slot_not_the_visible_ordinal() {
        let mut source = snapshot(6);
        source.native[0x3c0a7..0x3c2ff].fill(0xff);
        source.native[0x3c0a7..0x3c0a9].copy_from_slice(&299_u16.to_le_bytes());
        let call_at = 0x2eda3 + 8 * 299;
        let name_at = 0x2f703 + 16 * 299;
        source.native[call_at..call_at + 8].copy_from_slice(b"AB1/C   ");
        source.native[name_at..name_at + 16].copy_from_slice(b"Last            ");
        let mut editor = Editor::new(source).unwrap();
        let entries = entries(&editor, Category::Ur).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].target, Target::Ur(299)));
        let mut form = fields(&editor.settings, entries[0].target, editor.encoding).unwrap();
        assert_eq!(form[0].value, "AB1/C");
        form[1].value = "Renamed".into();
        let mut expected = editor.settings.memory().to_vec();
        expected[name_at..name_at + 16].copy_from_slice(b"Renamed         ");
        edit(&mut editor, entries[0].target, &form).unwrap();
        assert_eq!(editor.settings.memory(), expected);
    }

    #[test]
    fn gps_name_edit_preserves_coordinates_and_late_failure_is_atomic() {
        let mut source = snapshot(6);
        source.native[0x3c385..0x3c613].fill(0xff);
        source.native[0x3c385..0x3c387].copy_from_slice(&299_u16.to_le_bytes());
        source.native[0x3c5dd..0x3c5df].fill(0);
        let at = 0x32c19 + 34 * 299;
        source.native[at..at + 8].copy_from_slice(&[0, 0, 0, b'S', 180, 0, 0, b'W']);
        source.native[at + 18..at + 34].fill(b' ');
        let mut editor = Editor::new(source).unwrap();
        let target = Target::Gps(299);
        let mut form = fields(&editor.settings, target, editor.encoding).unwrap();
        form[0].value = "New".into();
        form[2].value = "180 00.001 W".into();
        let before = editor.settings.memory().to_vec();
        assert!(edit(&mut editor, target, &form).is_err());
        assert_eq!(editor.settings.memory(), before);
        assert!(!editor.dirty);
        form[2].value = "180 00.000 W".into();
        edit(&mut editor, target, &form).unwrap();
        let mut expected = before;
        expected[at + 18..at + 21].copy_from_slice(b"New");
        assert_eq!(editor.settings.memory(), expected);
    }
}
