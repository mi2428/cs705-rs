//! Semantic dispatch only: validation and stored representation belong to Icf.
//! Paths use kebab-case public getter names followed by explicit /arguments.

use crate::{
    AppError, Result,
    io::bytes_value,
    values::{self, ValueCodec},
};
use cs705::icf::*;
use serde_json::{Value, json};

macro_rules! fields {
    ($($get:ident => $set:ident ($($arg:ident: $argty:ty),*) -> $ty:ty;)+) => {
        pub fn list() -> Value {
            let mut list = vec![$(json!({"key": stringify!($get).replace('_', "-"),
                "arguments": [$({"name": stringify!($arg), "domain": <$argty>::domain()}),*],
                "value": <$ty>::domain()})),+];
            list.extend(settings::catalog().iter().map(|s| json!({"key":format!("setting/{}/0",s.name),"feature_id":s.feature_id,"source_key":s.key,"selection_key":s.selection_key,"count":s.count,"value":{"min":s.domain.min,"max":s.domain.max,"step":s.domain.step}})));
            list.extend(crate::records::fields());
            list.extend(settings::extended::number_fields().into_iter().map(|field|json!({"key":format!("number/{}",field.name),"arguments":[{"name":"index","min":0,"max":field.count-1,"default":0}],"feature_id":field.feature_id,"source_key":field.key,"min":field.minimum,"max":field.maximum,"step":field.step,"unit":field.unit,"input":"native ASCII editor text or action JSON; decimal defaults to dot"})));
            list.push(json!({"key":"number/utc_offset_minutes","feature_id":"A2-H54848-144","input":"signed hours:minutes or action JSON"}));
            list.extend(settings::extended::choice_fields().iter().chain(settings::extended::rf_choice_fields()).map(|field|json!({"key":format!("choice/{}/{}",field.name(),field.first_index),"feature_id":field.feature_id(),"source_key":field.key(),"indices":(field.first_index..field.first_index+field.count).filter(|&i|field.accepts_index(i)).collect::<Vec<_>>(),"menu_domain":{"min":field.domain().min,"max":field.domain().max},"input":"i32 selected candidate or action JSON"})));
            list.extend(settings::extended::PackedField::ALL.into_iter().map(|field|json!({"key":format!("choice/{}/0",field.feature_id()),"feature_id":field.feature_id(),"source_key":field.key(),"input":"i32 selected candidate or action JSON"})));
            list.extend(settings::extended::ColorField::ALL.into_iter().map(|field|json!({"key":format!("color/{}",field.feature_id()),"feature_id":field.feature_id(),"source_key":field.key(),"value":"RGB byte array or null to cancel"})));
            list.push(json!({"key":"filter-view/<index>","indices":{"min":0,"max":14},"read_only":true,"unit":"Hz"}));
            list.extend([("dtmf",16),("rtty",8)].into_iter().map(|(name,count)|json!({"key":format!("text/{name}/<index>"),"indices":{"min":0,"max":count-1},"input":"ASCII editor text or action JSON"})));
            list.extend(["ntp-server","clone-comment","memory-name/<slot>","gps-name/<slot>","keyer-memory/<channel>","rtty-memory/<channel>","dtmf-memory/<channel>"].into_iter().map(|key|json!({"key":key,"value":"text"})));
            list.push(json!({"key":"repeater/<slot>","value":RepeaterRecord::domain()}));
            list.push(json!({"key":"filter-passband-hz/<mode>/<slot>","read_only":true,"value":"integer Hz"}));
            list.push(json!({"key":"filter-shift-hz/<mode>/<slot>","read_only":true,"value":"integer Hz"}));
            list.extend([("address","A2-H56202-05"),("gateway","A2-H56202-07"),("primary-dns","A2-H56202-08"),("secondary-dns","A2-H56202-09")].into_iter().map(|(name,id)|json!({"key":format!("network/{name}"),"feature_id":id,"value":"ASCII address editor text"})));
            list.push(json!({"key":"network/prefix","feature_id":"A2-H56202-06","value":{"min":0,"max":255,"unit":"raw prefix code"},"selection_key":2}));
            Value::Array(list)
        }
        pub fn get(image: &Icf<'_>, key: &str) -> Result<Value> {
            if let Some(path)=key.strip_prefix("text/") {return text_editor(&mut image.clone(),path,Some("display"),"").map(|(value,_)| value);}
            if let Some(path) = key.strip_prefix("choice/") { return choice_edit(&mut image.clone(),path,settings::extended::ChoiceInput::Display).map(|(value,_)| value); }
            if let Some(id) = key.strip_prefix("color/") { return color_edit(&mut image.clone(),id,None).map(|(value,_)| value); }
            if let Some(index) = key.strip_prefix("filter-view/") {
                let view = settings::extended::filter_view(image,values::parse(index)?)?;
                return Ok(json!({"bandwidth_hz":view.bandwidth_hz,"pbt_hz":view.pbt_hz,"passband_hz":view.passband_hz,"shift_hz":view.shift_hz}));
            }
            if let Some(name) = key.strip_prefix("number/") {
                // Display helpers may perform an unconditional same-value store.
                // A read-only CLI query isolates that native case-body effect.
                let effect = apply_number(&mut image.clone(),name,settings::extended::NumberInput::Display,b'.')?;
                return Ok(json!({"value":effect.candidate,"raw":effect.raw_candidate,"suffix":bytes_value(&effect.suffix),"gray":effect.gray}));
            }
            if let Some(value) = crate::records::get(image,key)? { return Ok(value); }
            if let Some(name) = key.strip_prefix("network/") { return network_get(image,name); }
            if key.starts_with("setting/") {
                let (setting,index)=setting_path(key)?;
                return Ok(match setting.read(image,index)? {settings::Value::Number(n)=>json!(n)});
            }
            let mut parts = key.split('/');
            let name = parts.next().ok_or_else(|| AppError::invalid("empty field path"))?;
            $(if name == stringify!($get).replace('_', "-") {
                $(let $arg = values::parse::<$argty>(parts.next().ok_or_else(|| AppError::invalid("missing field argument"))?)?;)*
                if parts.next().is_some() { return Err(AppError::invalid("too many field arguments")); }
                return Ok(image.$get($($arg),*)?.json());
            })+
            text_get(image, key)
        }
        /// Return actual approval, not merely a handled path or a native store.
        pub fn set(image: &mut Icf<'_>, key: &str, value: &str) -> Result<bool> {
            if let Some(path)=key.strip_prefix("text/") {return report_edit(text_editor(image,path,None,value)?);}
            if let Some(path) = key.strip_prefix("choice/") { return report_edit(choice_edit(image,path,choice_input(value)?)?); }
            if let Some(id) = key.strip_prefix("color/") { return report_edit(color_edit(image,id,values::parse(value)?)?); }
            if let Some(name) = key.strip_prefix("number/") { return number_edit(image,name,value); }
            // Remaining paths are explicit checked assignments, not editor events.
            if crate::records::edit(image,key,value)? { return Ok(true); }
            if let Some(name) = key.strip_prefix("network/") { return network_set(image,name,value); }
            if key.starts_with("setting/") {
                let (setting,index)=setting_path(key)?;
                setting.write(image,index,&settings::Value::Number(values::parse(value)?))?;
                return Ok(true);
            }
            let mut parts = key.split('/');
            let name = parts.next().ok_or_else(|| AppError::invalid("empty field path"))?;
            $(if name == stringify!($get).replace('_', "-") {
                $(let $arg = values::parse::<$argty>(parts.next().ok_or_else(|| AppError::invalid("missing field argument"))?)?;)*
                if parts.next().is_some() { return Err(AppError::invalid("too many field arguments")); }
                image.$set($($arg,)* values::parse::<$ty>(value)?)?;
                return Ok(true);
            })+
            text_set(image, key, value)
        }
    };
}

fields! {
    rtty_waveform_color_raw => set_rtty_waveform_color_raw () -> [u8;3];
    rtty_receive_font_color_raw => set_rtty_receive_font_color_raw () -> [u8;3];
    rtty_transmit_font_color_raw => set_rtty_transmit_font_color_raw () -> [u8;3];
    audio_fft_waveform_color => set_audio_fft_waveform_color () -> [u8;3];
    audio_osc_waveform_color => set_audio_osc_waveform_color () -> [u8;3];
    scope_waveform_color_current => set_scope_waveform_color_current () -> [u8;3];
    scope_waveform_color_line => set_scope_waveform_color_line () -> [u8;3];
    scope_waveform_color_max_hold => set_scope_waveform_color_max_hold () -> [u8;3];
    lcd_backlight_auto_adjust => set_lcd_backlight_auto_adjust () -> bool;
    vox => set_vox () -> bool;
    one_hz_mode => set_one_hz_mode () -> bool;
    cw_manual_notch_width => set_cw_manual_notch_width () -> ManualNotchWidth;
    rtty_manual_notch_width => set_rtty_manual_notch_width () -> ManualNotchWidth;
    am_manual_notch_width => set_am_manual_notch_width () -> ManualNotchWidth;
    delta_f_span => set_delta_f_span () -> DeltaFSpan;
    select_memory_scan => set_select_memory_scan () -> SelectMemoryScan;
    scope_during_tx_center => set_scope_during_tx_center () -> bool;
    memory_pad_channels => set_memory_pad_channels () -> MemoryPadChannels;
    display_language => set_display_language () -> Language;
    coordinate_display => set_coordinate_display () -> CoordinateDisplay;
    temperature_unit => set_temperature_unit () -> TemperatureUnit;
    ssb_manual_notch_width => set_ssb_manual_notch_width () -> ManualNotchWidth;
    cw_rise_time => set_cw_rise_time () -> CwRiseTime;
    scope_span => set_scope_span () -> ScopeSpan;
    scope_center_display => set_scope_center_display () -> ScopeCenterDisplay;
    scope_marker_position => set_scope_marker_position () -> ScopeMarkerPosition;
    home_frequency_hz => set_home_frequency_hz () -> Option<u32>;
    key_touch_beep => set_key_touch_beep () -> bool;
    beep_level_limit => set_beep_level_limit () -> bool;
    home_channel_beep => set_home_channel_beep () -> bool;
    band_edge_beep => set_band_edge_beep () -> BandEdgeBeep;
    auto_power_off => set_auto_power_off () -> AutoPowerOff;
    power_save => set_power_save () -> PowerSave;
    quick_split => set_quick_split () -> bool;
    split_lock => set_split_lock () -> bool;
    received_callsign_speech => set_received_callsign_speech () -> ReceivedCallsignSpeech;
    rx_cs_speech => set_rx_cs_speech () -> bool;
    mic_up_down_speech => set_mic_up_down_speech () -> bool;
    s_meter_level_speech => set_s_meter_level_speech () -> bool;
    mode_speech => set_mode_speech () -> bool;
    speech_language => set_speech_language () -> SpeechLanguage;
    speech_alphabet => set_speech_alphabet () -> SpeechAlphabet;
    speech_speed => set_speech_speed () -> SpeechSpeed;
    scope_mode => set_scope_mode () -> ScopeMode;
    swr_graph_step_index => set_swr_graph_step_index (slot: usize) -> u8;
    swr_graph_bar_index => set_swr_graph_bar_index (slot: usize) -> u8;
    scope_reference_level_tenths_db => set_scope_reference_level_tenths_db (slot: usize) -> i32;
    filter_bandwidth_hz => set_filter_bandwidth_hz (mode: FilterMode, slot: FilterSlot) -> u32;
    filter_pbt_hz => set_filter_pbt_hz (mode: FilterMode, slot: FilterSlot, side: PbtSide) -> i32;
    preamp => set_preamp (band: usize) -> Preamp;
    attenuator => set_attenuator (band: usize) -> bool;
    filter_shape => set_filter_shape (band: FilterBandGroup, slot: FilterShapeSlot) -> FilterShape;
    tuning_step_code => set_tuning_step_code (mode: StepAgcMode) -> u8;
    air_tuning_step_code => set_air_tuning_step_code () -> u8;
    agc => set_agc (mode: StepAgcMode) -> Agc;
    agc_delay_code => set_agc_delay_code (mode: StepAgcMode, state: Agc) -> u8;
    receive_high_pass_code => set_receive_high_pass_code (mode: ToneMode) -> u8;
    receive_low_pass_code => set_receive_low_pass_code (mode: ToneMode) -> u8;
    receive_bass => set_receive_bass (mode: ToneMode) -> i8;
    receive_treble => set_receive_treble (mode: ToneMode) -> i8;
    transmit_bass => set_transmit_bass (mode: ToneMode) -> i8;
    transmit_treble => set_transmit_treble (mode: ToneMode) -> i8;
    transmit_bandwidth_low => set_transmit_bandwidth_low (preset: TransmitBandwidthPreset) -> TransmitLow;
    transmit_bandwidth_high => set_transmit_bandwidth_high (preset: TransmitBandwidthPreset) -> TransmitHigh;
    physical_memory_frequency_hz => set_physical_memory_frequency_hz (slot: usize, side: MemorySide) -> u32;
    physical_memory_filter => set_physical_memory_filter (slot: usize, side: MemorySide) -> MemoryFilter;
    physical_memory_split => set_physical_memory_split (slot: usize) -> bool;
    physical_memory_selection => set_physical_memory_selection (slot: usize) -> MemorySelection;
    physical_memory_duplex => set_physical_memory_duplex (slot: usize, side: MemorySide) -> MemoryDuplex;
    physical_memory_offset_hz => set_physical_memory_offset_hz (slot: usize, side: MemorySide) -> u32;
    physical_memory_dtcs_polarity => set_physical_memory_dtcs_polarity (slot: usize, side: MemorySide) -> (bool,bool);
    physical_memory_callsign_raw => set_physical_memory_callsign_raw (slot: usize, side: MemorySide, which: MemoryCallsign) -> [u8;8];
    physical_memory_tone => set_physical_memory_tone (slot: usize, side: MemorySide) -> MemoryTone;
    physical_memory_tx_tone_index => set_physical_memory_tx_tone_index (slot: usize, side: MemorySide) -> u8;
    physical_memory_rx_tone_index => set_physical_memory_rx_tone_index (slot: usize, side: MemorySide) -> u8;
    physical_memory_dtcs_index => set_physical_memory_dtcs_index (slot: usize, side: MemorySide) -> u8;
    physical_memory_digital_squelch => set_physical_memory_digital_squelch (slot: usize, side: MemorySide) -> MemoryDigitalSquelch;
    physical_memory_digital_code => set_physical_memory_digital_code (slot: usize, side: MemorySide) -> u8;
    physical_gps_date => set_physical_gps_date (slot: usize) -> GpsDate;
    physical_gps_time => set_physical_gps_time (slot: usize) -> GpsTime;
    physical_gps_latitude => set_physical_gps_latitude (slot: usize) -> GpsCoordinate;
    physical_gps_longitude => set_physical_gps_longitude (slot: usize) -> GpsCoordinate;
    physical_gps_altitude => set_physical_gps_altitude (slot: usize) -> GpsAltitude;
    gps_alarm => set_gps_alarm () -> GpsAlarm;
}

fn setting_path(key: &str) -> Result<(&'static settings::Setting, usize)> {
    let mut parts = key.split('/');
    let _ = parts.next();
    let name = parts
        .next()
        .ok_or_else(|| AppError::invalid("setting requires a property name or feature ID"))?;
    let index = values::parse(
        parts
            .next()
            .ok_or_else(|| AppError::invalid("setting requires an explicit index"))?,
    )?;
    if parts.next().is_some() {
        return Err(AppError::invalid("too many setting path components"));
    }
    Ok((settings::find(name)?, index))
}

fn indexed_path(path: &str, default: Option<usize>) -> Result<(&str, usize)> {
    let mut parts = path.split('/');
    let name = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::invalid("missing editor field name"))?;
    let index = parts
        .next()
        .map(values::parse)
        .transpose()?
        .or(default)
        .ok_or_else(|| AppError::invalid("editor field requires an explicit index"))?;
    if parts.next().is_some() {
        return Err(AppError::invalid("too many editor field arguments"));
    }
    Ok((name, index))
}

fn report_edit((value, accepted): (Value, bool)) -> Result<bool> {
    crate::io::report(&value)?;
    Ok(accepted)
}

fn text_editor(
    image: &mut Icf<'_>,
    path: &str,
    action: Option<&str>,
    text: &str,
) -> Result<(Value, bool)> {
    use settings::extended::{self as e, TextInput};
    let (name, index) = indexed_path(path, None)?;
    let value: Value = if let Some(action) = action {
        json!({"action":action})
    } else if values::json_view(text).starts_with('{') {
        serde_json::from_str(text).map_err(|_| AppError::invalid("invalid text editor JSON"))?
    } else {
        json!({"action":"text","text":values::parse::<String>(text)?})
    };
    let object = value
        .as_object()
        .ok_or_else(|| AppError::invalid("text editor requires an object"))?;
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid("text editor requires an action"))?;
    let input = match action {
        "text" if object.len() == 2 => TextInput::Text(
            object
                .get("text")
                .and_then(Value::as_str)
                .filter(|s| s.is_ascii())
                .ok_or_else(|| AppError::invalid("text editor requires ASCII text"))?
                .as_bytes(),
        ),
        "display" if object.len() == 1 => TextInput::Display,
        "cancel" if object.len() == 1 => TextInput::Cancel,
        "begin" if object.len() == 1 => TextInput::Begin(None),
        "begin" if object.len() == 2 => {
            let first = u8::parse(
                object
                    .get("first")
                    .ok_or_else(|| AppError::invalid("text begin accepts only first"))?,
            )?;
            if !first.is_ascii() {
                return Err(AppError::invalid("text begin requires an ASCII byte"));
            }
            TextInput::Begin(Some(first))
        }
        _ => {
            return Err(AppError::invalid(
                "unknown text action or unexpected members",
            ));
        }
    };
    let effect = match name {
        "dtmf" => e::edit_dtmf(image, index, input)?,
        "rtty" => e::edit_rtty(image, index, input)?,
        _ => return Err(AppError::invalid("unknown text editor")),
    };
    Ok((
        json!({"setting":format!("text/{path}"),"stored":effect.stored,"state_out":effect.state_out,"notify_flags":effect.notify_flags,"display":bytes_value(&effect.display)}),
        effect.stored,
    ))
}

pub fn choice_input(text: &str) -> Result<settings::extended::ChoiceInput> {
    use settings::extended::ChoiceInput;
    if !values::json_view(text).starts_with('{') {
        return Ok(ChoiceInput::Selected(settings::Selection {
            key: 1,
            phase: 2,
            candidate: values::parse(text)?,
            accepted: true,
        }));
    }
    let value: Value =
        serde_json::from_str(text).map_err(|_| AppError::invalid("invalid choice editor JSON"))?;
    let object = value
        .as_object()
        .ok_or_else(|| AppError::invalid("choice editor requires an object"))?;
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid("choice editor requires an action"))?;
    let input = match action {
        "selected" if object.len() == 5 => {
            let member = |key| {
                object
                    .get(key)
                    .ok_or_else(|| AppError::invalid("missing choice editor member"))
            };
            ChoiceInput::Selected(settings::Selection {
                key: u16::parse(member("key")?)?,
                phase: u32::parse(member("phase")?)?,
                candidate: i32::parse(member("candidate")?)?,
                accepted: bool::parse(member("accepted")?)?,
            })
        }
        "display" if object.len() == 1 => ChoiceInput::Display,
        "cancel" if object.len() == 1 => ChoiceInput::Cancel,
        "begin" if object.len() == 1 => ChoiceInput::Begin,
        "next" if object.len() == 1 => ChoiceInput::Next,
        "previous" if object.len() == 1 => ChoiceInput::Previous,
        _ => {
            return Err(AppError::invalid(
                "unknown choice action or unexpected members",
            ));
        }
    };
    Ok(input)
}

fn choice_edit(
    image: &mut Icf<'_>,
    path: &str,
    input: settings::extended::ChoiceInput,
) -> Result<(Value, bool)> {
    use settings::extended as e;
    let (name, index) = indexed_path(path, None)?;
    let mut fields = e::choice_fields()
        .iter()
        .chain(e::rf_choice_fields())
        .filter(|field| {
            (field.name() == name || field.feature_id() == name) && field.accepts_index(index)
        });
    let field = fields.next();
    if fields.next().is_some() {
        return Err(AppError::invalid(
            "choice feature ID is ambiguous; use the field name",
        ));
    }
    let effect = if let Some(field) = field {
        field.edit(image, index, input)?
    } else if let Some(field) = e::PackedField::ALL
        .into_iter()
        .find(|f| f.feature_id() == name)
    {
        if index != 0 {
            return Err(AppError::invalid("packed choice requires index zero"));
        }
        field.edit(image, input)?
    } else {
        return Err(AppError::invalid(
            "unknown choice field or invalid native index",
        ));
    };
    Ok((
        json!({"setting":format!("choice/{path}"),"stored":effect.stored,"value":effect.value,
        "notify_flags":effect.notify_flags,"gray":effect.gray,"popup_requested":effect.popup_requested,"fixed_label":effect.fixed_value.map(|value| value.source_label())}),
        effect.stored,
    ))
}

fn color_edit(image: &mut Icf<'_>, id: &str, choice: Option<[u8; 3]>) -> Result<(Value, bool)> {
    let field = settings::extended::ColorField::ALL
        .into_iter()
        .find(|field| field.feature_id() == id)
        .ok_or_else(|| AppError::invalid("unknown color feature ID"))?;
    let effect = settings::extended::edit_color(image, field, choice)?;
    Ok((
        json!({"setting":format!("color/{id}"),"stored":effect.stored,"value":effect.value,"notify_flags":effect.notify_flags}),
        effect.stored,
    ))
}

/// Return helper approval; display, cancellation and unmatched events are false.
pub fn select(image: &mut Icf<'_>, assignment: &str) -> Result<bool> {
    let (path, text) = assignment
        .split_once('=')
        .ok_or_else(|| AppError::invalid("event requires FIELD_PATH=JSON"))?;
    if let Some(name) = path.strip_prefix("number/") {
        return number_edit(image, name, text);
    }
    if let Some(path) = path.strip_prefix("choice/") {
        return report_edit(choice_edit(image, path, choice_input(text)?)?);
    }
    if let Some(path) = path.strip_prefix("text/") {
        return report_edit(text_editor(image, path, None, text)?);
    }
    if !path.starts_with("setting/") && path != "network/prefix" {
        return Err(AppError::invalid(
            "event path requires setting/NAME/INDEX or network/prefix",
        ));
    }
    let value: Value = serde_json::from_str(text)
        .map_err(|_| AppError::invalid("invalid selection event JSON"))?;
    let object = value
        .as_object()
        .filter(|o| o.len() == 4)
        .ok_or_else(|| AppError::invalid("event requires key, phase, candidate and accepted"))?;
    let get = |name: &str| {
        object
            .get(name)
            .ok_or_else(|| AppError::invalid("missing selection event member"))
    };
    let event = settings::Selection {
        key: u16::parse(get("key")?)?,
        phase: u32::parse(get("phase")?)?,
        candidate: i32::parse(get("candidate")?)?,
        accepted: bool::parse(get("accepted")?)?,
    };
    if path == "network/prefix" {
        let effect =
            image.select_network_prefix(event.key, event.phase, event.candidate, event.accepted);
        return network_report("prefix", effect);
    }
    let (setting, index) = setting_path(path)?;
    let effect = setting.select(image, index, event)?;
    crate::io::report(
        &json!({"setting":setting.name,"stored":effect.stored,"notify_flags":effect.notify_flags}),
    )?;
    Ok(effect.stored)
}

fn apply_number(
    image: &mut Icf<'_>,
    name: &str,
    input: settings::extended::NumberInput<'_>,
    decimal: u8,
) -> Result<settings::extended::NumberEffect> {
    let (name, index) = indexed_path(name, Some(0))?;
    Ok(if name == "utc_offset_minutes" {
        if index != 0 {
            return Err(AppError::invalid("UTC editor requires index zero"));
        }
        settings::extended::edit_utc_offset(image, input)?
    } else {
        settings::extended::edit_number_at(image, name, index, input, decimal)?
    })
}

/// Share one lexical/action boundary with handlers that need additional runtime
/// sources. Never parse bare native text as a JSON number before calling them.
pub fn with_number_input<T>(
    text: &str,
    edit: impl FnOnce(settings::extended::NumberInput<'_>, u8) -> Result<T>,
) -> Result<(T, bool)> {
    use settings::extended::NumberInput;
    let (effect, typed) = if values::json_view(text).starts_with('{') {
        let value: Value = serde_json::from_str(text)
            .map_err(|_| AppError::invalid("invalid number editor JSON"))?;
        let object = value
            .as_object()
            .ok_or_else(|| AppError::invalid("number editor requires an object"))?;
        let action = object
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::invalid("number editor requires an action"))?;
        let allowed: &[&str] = match action {
            "text" => &["action", "text", "decimal"],
            "selected" => &["action", "key", "phase", "candidate", "accepted"],
            "display" | "cancel" | "begin" | "next" | "previous" => &["action"],
            _ => return Err(AppError::invalid("unknown number editor action")),
        };
        if object.keys().any(|key| !allowed.contains(&key.as_str())) {
            return Err(AppError::invalid("unknown member in number editor action"));
        }
        let member = |key: &str| {
            object
                .get(key)
                .ok_or_else(|| AppError::invalid("missing number editor member"))
        };
        let decimal = match object.get("decimal") {
            None => b'.',
            Some(Value::String(s)) if matches!(s.as_str(), "." | ",") => s.as_bytes()[0],
            _ => {
                return Err(AppError::invalid(
                    "number editor decimal must be dot or comma",
                ));
            }
        };
        let input = match action {
            "text" => {
                let source = member("text")?
                    .as_str()
                    .filter(|s| s.is_ascii())
                    .ok_or_else(|| AppError::invalid("number editor text requires ASCII"))?;
                NumberInput::Text(source.as_bytes())
            }
            "selected" => NumberInput::Selected(settings::Selection {
                key: u16::parse(member("key")?)?,
                phase: u32::parse(member("phase")?)?,
                candidate: i32::parse(member("candidate")?)?,
                accepted: bool::parse(member("accepted")?)?,
            }),
            "display" => NumberInput::Display,
            "cancel" => NumberInput::Cancel,
            "begin" => NumberInput::Begin,
            "next" => NumberInput::Next,
            "previous" => NumberInput::Previous,
            _ => unreachable!("action validated above"),
        };
        (edit(input, decimal)?, action == "text")
    } else {
        // Preserve lexical input such as 1e-3. Parsing and reformatting a JSON
        // number before the native filter would change the editor's grammar.
        let source = if values::json_view(text).starts_with('"') {
            values::parse::<String>(text)?
        } else {
            text.to_owned()
        };
        if !source.is_ascii() {
            return Err(AppError::invalid("number editor text requires ASCII"));
        }
        (edit(NumberInput::Text(source.as_bytes()), b'.')?, true)
    };
    Ok((effect, typed))
}

fn number_edit(image: &mut Icf<'_>, name: &str, text: &str) -> Result<bool> {
    let (effect, typed) = with_number_input(text, |input, decimal| {
        apply_number(image, name, input, decimal)
    })?;
    crate::io::report(
        &json!({"setting":format!("number/{name}"),"stored":effect.stored,
        "accepted":effect.accepted,"candidate":effect.candidate,"raw_candidate":effect.raw_candidate,
        "state_out":effect.state_out,"notify_flags":effect.notify_flags,"suffix":bytes_value(&effect.suffix),"gray":effect.gray}),
    )?;
    if typed && !effect.accepted {
        return Err(AppError::invalid("number editor did not accept the text"));
    }
    Ok(effect.accepted)
}

fn network_get(image: &Icf<'_>, name: &str) -> Result<Value> {
    let c = image.network_configuration();
    Ok(match name {
        "address" => json!(c.address.to_string()),
        "prefix" => json!(c.prefix),
        "gateway" => json!(c.gateway.to_string()),
        "primary-dns" => json!(c.primary_dns.to_string()),
        "secondary-dns" => json!(c.secondary_dns.to_string()),
        _ => return Err(AppError::invalid("unknown network field")),
    })
}

fn network_report(name: &str, effect: settings::network::Effect) -> Result<bool> {
    crate::io::report(
        &json!({"setting":format!("network/{name}"),"stored":effect.stored,
        "notify_flags":effect.notify_flags,"gray":effect.gray}),
    )?;
    Ok(effect.stored)
}

fn network_set(image: &mut Icf<'_>, name: &str, text: &str) -> Result<bool> {
    use settings::network::Field;
    let effect = if name == "prefix" {
        let prefix = values::parse::<u8>(text)
            .map_err(|error| AppError::invalid(format!("network prefix: {}", error.message)))?;
        image.select_network_prefix(2, 2, i32::from(prefix), true)
    } else {
        let field = match name {
            "address" => Field::Address,
            "gateway" => Field::Gateway,
            "primary-dns" => Field::PrimaryDns,
            "secondary-dns" => Field::SecondaryDns,
            _ => return Err(AppError::invalid("unknown network field")),
        };
        let input = values::parse::<String>(text)?;
        if !input.is_ascii() {
            return Err(AppError::invalid(
                "network editor input requires ASCII text",
            ));
        }
        image.edit_network_address(field, Some(input.as_bytes()))
    };
    if !effect.stored {
        return Err(AppError::invalid(
            "network editor rejected the input length",
        ));
    }
    network_report(name, effect)
}

fn text_get(image: &Icf<'_>, key: &str) -> Result<Value> {
    let parts: Vec<_> = key.split('/').collect();
    match parts.as_slice() {
        ["ntp-server"] => Ok(bytes_value(image.ntp_server_raw())),
        ["gps-name", slot] => Ok(image
            .physical_gps_name_raw(values::parse(slot)?)?
            .map_or(Value::Null, bytes_value)),
        ["repeater", slot] => Ok(image
            .physical_repeater(values::parse(slot)?)?
            .map_or(Value::Null, |r| r.json())),
        ["clone-comment"] => Ok(bytes_value(image.clone_comment_raw())),
        ["memory-name", slot] => Ok(image
            .physical_memory_name_raw(values::parse(slot)?)?
            .map_or(Value::Null, bytes_value)),
        ["keyer-memory", channel] => Ok(bytes_value(
            image.keyer_memory_raw(values::parse(channel)?)?,
        )),
        ["rtty-memory", channel] => {
            Ok(bytes_value(image.rtty_memory_raw(values::parse(channel)?)?))
        }
        ["dtmf-memory", channel] => {
            Ok(bytes_value(image.dtmf_memory_raw(values::parse(channel)?)?))
        }
        ["filter-passband-hz", mode, slot] => Ok(json!(
            image.filter_passband_hz(values::parse(mode)?, values::parse(slot)?)?
        )),
        ["filter-shift-hz", mode, slot] => Ok(json!(
            image.filter_shift_hz(values::parse(mode)?, values::parse(slot)?)?
        )),
        _ => Err(AppError::invalid(
            "unknown field path; use show --list-fields",
        )),
    }
}
fn text_set(image: &mut Icf<'_>, key: &str, value: &str) -> Result<bool> {
    let parts: Vec<_> = key.split('/').collect();
    if let ["repeater", slot] = parts.as_slice() {
        image.set_physical_repeater(
            values::parse(slot)?,
            &values::parse::<RepeaterRecord>(value)?,
        )?;
        return Ok(true);
    }
    let value = values::parse::<String>(value)?;
    match parts.as_slice() {
        ["ntp-server"] => image.set_ntp_server_ascii(&value)?,
        ["gps-name", slot] => image.set_physical_gps_name_ascii(values::parse(slot)?, &value)?,
        ["clone-comment"] => image.set_clone_comment_canonical_ascii(&value)?,
        ["memory-name", slot] => {
            image.set_physical_memory_name_ascii(values::parse(slot)?, &value)?
        }
        ["keyer-memory", channel] => {
            image.set_keyer_memory(values::parse(channel)?, value.as_bytes())?
        }
        ["rtty-memory", channel] => {
            image.set_rtty_memory(values::parse(channel)?, value.as_bytes())?
        }
        ["dtmf-memory", channel] => {
            image.set_dtmf_memory(values::parse(channel)?, value.as_bytes())?
        }
        _ => {
            return Err(AppError::invalid(
                "unknown or read-only field path; use show --list-fields",
            ));
        }
    }
    Ok(true)
}
