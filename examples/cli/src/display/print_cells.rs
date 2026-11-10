//! Native resource nodes -> existing typed readers. No image or text codec.

use crate::{AppError, Result};
use cs705::{
    icf::{
        self, Icf, MemoryCallsign,
        record::{self, ChannelField as C, RepeaterField as R, RepeaterText as T},
        record_image,
        settings::{self, digital, extended as e, gps},
    },
    printing::native::{CellNode, Class, Factory, Resources},
};

pub fn print_condition(name: &[u8], runtime: &crate::records::Runtime) -> Result<bool> {
    super::resource_condition(
        name,
        runtime.display_context()?,
        runtime.display_system_langid(),
    )
}

fn index(node: &CellNode) -> Result<usize> {
    usize::try_from(node.index).map_err(|_| AppError::invalid("negative native cell index"))
}
fn number(value: impl std::fmt::Display) -> Vec<u8> {
    value.to_string().into_bytes()
}
fn prefix(bytes: &[u8]) -> Vec<u8> {
    bytes.split(|b| *b == 0).next().unwrap().to_vec()
}
fn native_text(logical: &[u8], runtime: &crate::records::Runtime) -> Result<Vec<u8>> {
    Ok(cs705::csv::history::paint::display_text(
        logical,
        &cs705::csv::history::paint::Context {
            gui_language: runtime.display_input_options().gui_language,
            system_langid: runtime.display_system_langid(),
            ..Default::default()
        },
    )?)
}
fn native_coordinate(
    raw: [u8; 4],
    mode: u8,
    current: &[u8],
    runtime: &crate::records::Runtime,
) -> Result<Vec<u8>> {
    let logical = cs705::csv::number::paint::format(
        raw,
        mode,
        current,
        runtime.display_input_options().gui_language,
        runtime.display_system_langid(),
    )?;
    native_text(&logical, runtime)
}
fn history_number(hz: u32, runtime: &crate::records::Runtime) -> Result<Vec<u8>> {
    let context = runtime.display_format_context()?;
    Ok(cs705::csv::number::field::history(
        hz,
        context.native_byte("rtl_decimal")?,
        |index| {
            context.native_decimal_copy(index).map_err(|_| {
                cs705::csv::Error::SourceUnavailable("native copied decimal byte input")
            })
        },
    )?)
}
fn window(bytes: &[u8], at: usize, width: usize) -> Result<&[u8]> {
    bytes
        .get(
            at..at
                .checked_add(width)
                .ok_or_else(|| AppError::invalid("native cell address overflow"))?,
        )
        .ok_or_else(|| AppError::invalid("native cell source window unavailable"))
}
fn word(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        window(bytes, at, 2)?.try_into().unwrap(),
    ))
}
fn unsupported(node: &CellNode) -> AppError {
    AppError::invalid(format!(
        "native cell group={} key={:X} index={} has no registered typed display binding",
        node.group, node.key, node.index
    ))
}

// Key -> selector source schema at 64D66, not copied selector captions or codes.
fn selector(key: u32) -> Option<&'static str> {
    Some(match key {
        0x336..=0x338 => "Agc",
        0x37c => "AutoRpt_Dup",
        0x2c3 => "Common_001NumberStyle",
        0x2c4 => "Common_001TriggerCh",
        0x18d => "Common_AutoReply",
        0x23b => "Common_Auto_Power_OFF",
        0x2ce => "Common_BKIN",
        0x235 => "Common_BandEdgeBeep",
        0x22e => "Common_Baro_Units",
        0x2b3 => "Common_BtAfOut",
        0x2b4 => "Common_BtAutoDiscon",
        0x2bb => "Common_BtSerialFunc",
        0x29e => "Common_CWNormalSide",
        0x2cb => "Common_CW_KeyPolarity",
        0x2cc => "Common_CW_KeyType",
        0x2cd => "Common_CW_MICUDKeyEnable",
        0x259 => "Common_ChannelSelectionGroup",
        0x1c5 => "Common_Compass",
        0x2ee | 0x2f3 | 0x38c => "Common_Con_AFIF",
        0x2f0 | 0x2f4 | 0x38e => "Common_Con_AF_SQL",
        0x287 | 0x289 => "Common_CsvDate",
        0x286 | 0x288 => "Common_CsvSepdec",
        0x1e1 | 0x1ee | 0x1ff => "Common_Data_Extension",
        0x252 => "Common_DialAutoTS",
        0x192 => "Common_DigiMoni",
        0x193 => "Common_DigiRpt",
        0x214 => "Common_Display_Font",
        0x213 => "Common_Display_Type",
        0x1cf => "Common_DprsTxFmt",
        0x18e => "Common_DvDataTX",
        0x357 => "Common_FFTScope_ATT",
        0x358 => "Common_FFTScope_WaveformType",
        0x190 => "Common_FastData_GPSSpd",
        0x191 => "Common_FastData_PTTDelay",
        0x26a => "Common_FrontKey_AUTOTUNE",
        0x269 => "Common_FrontKey_VOX",
        0x276 => "Common_FullKeyBoardLayout",
        0x311 => "Common_GPSOut",
        0x1bf => "Common_GPS_IntPSave",
        0x1c6 => "Common_GPS_PosRxSel",
        0x1c0 => "Common_GPS_SateInfoOut",
        0x1bc => "Common_GPS_Select",
        0x1ab => "Common_Gateway_CallSignSet",
        0x1a6 => "Common_Gateway_Connection_Type",
        0x1a9 => "Common_Gateway_Type",
        0x1cd => "Common_GpsAutoTX",
        0x1d9 => "Common_GpsCommentCh",
        0x1d0 => "Common_GpsSymCh",
        0x1cc => "Common_GpsTX",
        0x1c9 => "Common_Gps_Alarm_Type",
        0x20f => "Common_Gps_Log_Interval",
        0x25a => "Common_GroupDispSel",
        0x2b5 => "Common_HsMode",
        0x1f8 => "Common_ItmType",
        0x275 => "Common_KeyBoardType",
        0x224 | 0x225 => "Common_Language",
        0x22b => "Common_Linear_Measure",
        0x250 => "Common_LockFunction",
        0x258 => "Common_MICUDSpeed",
        0x299 => "Common_MNotchQ",
        0x2f8 | 0x2f9 | 0x391 | 0x393 => "Common_MOD",
        0x2a5 => "Common_MainDialOpe",
        0x28e => "Common_MaxTxPowerBattery",
        0x28d => "Common_MaxTxPowerDcIn",
        0x251 => "Common_MemoryPadCh",
        0x216 => "Common_MeterType",
        0x29b | 0x29c => "Common_NOTCHKey",
        0x320 => "Common_Network_AccessPoint_OffTimer",
        0x317 => "Common_Network_ConnectionType",
        0x327 => "Common_Network_InternetAccessLine",
        0x323 => "Common_Network_PowerOFFSetting",
        0x31a => "Common_Network_SubnetMask",
        0x298 => "Common_NotchFilter",
        0x1f6 => "Common_ObjTimeStamp",
        0x1e7 => "Common_ObjType",
        0x35b => "Common_OscScope_Level",
        0x35c => "Common_OscScope_SweepTime",
        0x1c8 => "Common_POS_Area2",
        0x2a1 => "Common_PauseTimer",
        0x1e4 | 0x1f3 | 0x204 => "Common_PhgdGain",
        0x1e2 | 0x1f1 | 0x202 => "Common_PhgdPow",
        0x283 => "Common_Play_Skip_Time",
        0x23c => "Common_Power_Save",
        0x282 => "Common_PreRec_PttAutoRec",
        0x2da => "Common_RTTY_DecodeLineFeedCode",
        0x2d7 => "Common_RTTY_FFTScopeAveraging",
        0x2d3 => "Common_RTTY_FileType",
        0x2e3 => "Common_RTTY_KeyPolarity",
        0x2e2 => "Common_RTTY_Shift",
        0x2d5 => "Common_RTTY_TimeStamp_Time",
        0x2e1 => "Common_RTTY_Tone",
        0x1a1 => "Common_RXHis_CsName_Chg",
        0x22f => "Common_Rain_Units",
        0x27e => "Common_RecCondition",
        0x27a => "Common_RecMicSel",
        0x26b | 0x2ba => "Common_RemoteMicKey_Nor",
        0x1a4 => "Common_RepeaterMonitorTimer",
        0x2a2 => "Common_ResumeTimer",
        0x198 => "Common_RxCallDisp",
        0x19b => "Common_RxPos_Disp",
        0x19c => "Common_RxPos_Disp_Timer",
        0x195 => "Common_RxRecord",
        0x29f => "Common_ScanSpeed",
        0x2a8 => "Common_Scan_Memory",
        0x368 => "Common_Scope_Averaging",
        0x365 => "Common_Scope_CENTERTypeDisplay",
        0x366 => "Common_Scope_MarkerPosition",
        0x364 => "Common_Scope_MaxHold",
        0x35e => "Common_Scope_Mode",
        0x361 => "Common_Scope_SweepSpeed",
        0x367 => "Common_Scope_VBW",
        0x370 => "Common_Scope_WaterfallPeakColorLevel",
        0x36f => "Common_Scope_WaterfallSizeExpand",
        0x36e => "Common_Scope_WaterfallSpeed",
        0x369 => "Common_Scope_WaveformType",
        0x274 => "Common_ScreenCaptureFileType",
        0x21e | 0x21f => "Common_ScrnSaver",
        0x21d => "Common_Scroll",
        0x2e5 => "Common_SpJackFunc",
        0x24c => "Common_Speech_Alphabet",
        0x24b => "Common_Speech_Language",
        0x246 => "Common_Speech_RxCall",
        0x24d => "Common_Speech_Speed",
        0x24f => "Common_Speech_Switch",
        0x22c => "Common_Speed_Units",
        0x1a2 => "Common_Standby_Beep",
        0x262 | 0x263 | 0x395 => "Common_TBW",
        0x290 => "Common_TOT",
        0x19f => "Common_TXCallDisp",
        0x2a3 => "Common_TempSkipTimer",
        0x1df | 0x20b => "Common_Time_Stamp",
        0x242 => "Common_TunerSelect",
        0x28f => "Common_TxDelay",
        0x27d => "Common_TxRecAudio",
        0x30f => "Common_USB2Func",
        0x2fb | 0x2fc | 0x2fd | 0x397 | 0x398 | 0x399 => "Common_UsbSendKey",
        0x25f => "Common_VOXVoiceDelay",
        0x230 => "Common_WidSpd_Units",
        0x32a | 0x32d => "Common_YESNO",
        0x125 => "DR_DSQL",
        0x120 => "DTMF_SPEED",
        0x330..=0x333 | 0x38a | 0x38b => "Filter_Type",
        0x1bb => "GPS_Alarm",
        0x112 => "Mch_DSQL",
        0x110 => "Mch_DTCS",
        0x111 => "Mch_DTCS_Polarity",
        0x10b | 0x17e | 0x186 => "Mch_Dup",
        0x10a | 0x121 | 0x122 | 0x388 => "Mch_Filter",
        0x108 | 0x15d => "Mch_Mode",
        0x117 => "Mch_Scan_Sel",
        0x14e => "Mch_TFreq",
        0x10d => "Mch_TSQL",
        0x228 => "Misc_AutoOff",
        0x123 | 0x18f | 0x194 | 0x19a | 0x19d | 0x19e | 0x1aa | 0x1bd | 0x1be | 0x1e0 | 0x20e
        | 0x212 | 0x215 | 0x217 | 0x218 | 0x21a | 0x21b | 0x21c | 0x220 | 0x221 | 0x222 | 0x223
        | 0x226 | 0x232 | 0x233 | 0x234 | 0x23f | 0x241 | 0x243 | 0x244 | 0x247 | 0x248 | 0x249
        | 0x24a | 0x253 | 0x255 | 0x25b | 0x260 | 0x264 | 0x266 | 0x267 | 0x268 | 0x26c | 0x26d
        | 0x26e | 0x26f | 0x270 | 0x271 | 0x272 | 0x273 | 0x279 | 0x27f | 0x281 | 0x284 | 0x285
        | 0x291 | 0x292 | 0x296 | 0x29d | 0x2a0 | 0x2aa | 0x2b1 | 0x2b2 | 0x2b6 | 0x2b7 | 0x2b8
        | 0x2b9 | 0x2c7 | 0x2d2 | 0x2d4 | 0x2d6 | 0x2d9 | 0x2dc | 0x2e0 | 0x2f1 | 0x2fa | 0x2ff
        | 0x300 | 0x301 | 0x308 | 0x30c | 0x315 | 0x318 | 0x322 | 0x350 | 0x35a | 0x363 | 0x36d
        | 0x371 | 0x394 | 0x39b | 0x39c => "Misc_OnOff",
        0x106 | 0x109 | 0x11c => "Misc_OnOff2",
        0x134 => "My_Call_Sign_No_Sel",
        0x11d => "Pch_Filter",
        0x11b => "Pch_Mode",
        0x1b1 => "PicQty",
        0x387 => "Preset_Mode",
        0x14b => "Rpt_Dup",
        0x146 => "Rpt_Mode",
        0x150 => "Rpt_Position",
        0x14f => "Rpt_Rpt1use",
        0x14d => "Rpt_Tone",
        0x143 => "Rpt_Type",
        0x156 => "RxHis_Type",
        0x1de | 0x1f5 | 0x206 | 0x209 => "SSID",
        0x356 => "SWR_Graph_BAR",
        0x355 => "SWR_Graph_STEP",
        0x33f..=0x341 => "TS",
        0x344 => "ToneControl_Hpf",
        0x345 => "ToneControl_Lpf",
        0x13b => "Tx_Message_Ch_No_Sel",
        0x316 => "WLAN",
        _ => return None,
    })
}

fn selected_label(
    resources: &Resources<'_>,
    source: &str,
    value: i64,
    runtime: &crate::records::Runtime,
) -> Result<Vec<u8>> {
    Ok(resources.selector_caption(
        format!("GI_Sheet_Sel_{source}").as_bytes(),
        1,
        value,
        runtime.display_input_options().gui_language,
        runtime.display_system_langid(),
    )?)
}
fn scalar(
    value: i64,
    node: &CellNode,
    resources: &Resources<'_>,
    runtime: &crate::records::Runtime,
) -> Result<Vec<u8>> {
    if (0x339..=0x33e).contains(&node.key) {
        return selected_label(
            resources,
            if node.index == 3 {
                "Agc_Delay_AM"
            } else {
                "Agc_Delay"
            },
            value,
            runtime,
        );
    }
    if matches!(node.key, 0x34a..=0x34d | 0x392 | 0x396) {
        return selected_label(
            resources,
            if node.index == 0 {
                "ToneControl_Tbw_Low"
            } else {
                "ToneControl_Tbw_High"
            },
            value,
            runtime,
        );
    }
    match selector(node.key) {
        Some(source) => selected_label(resources, source, value, runtime),
        None => Ok(number(value)),
    }
}

fn special_edge(
    current: &[u8],
    node: &CellNode,
    runtime: &crate::records::Runtime,
    resources: &Resources<'_>,
) -> Result<Vec<u8>> {
    let row = index(node)?;
    if node.name.starts_with(b"CL_BandEdge_") {
        let holder = icf::special::UserEdge::read(current, row)?;
        if !holder.valid {
            return Ok(Vec::new());
        }
        return match node.name.as_slice() {
            b"CL_BandEdge_Freq_Low" => Ok(cs705::csv::number::field::channel(holder.low_hz)),
            b"CL_BandEdge_Freq_High" => Ok(cs705::csv::number::field::channel(holder.high_hz)),
            _ => Err(unsupported(node)),
        };
    }
    let holder = icf::special::ScanEdge::read(current, row)?;
    if !holder.valid {
        return Ok(Vec::new());
    }
    match node.name.as_slice() {
        b"CL_PgSrch_Name" => native_text(&prefix(&holder.name), runtime),
        b"CL_PgSrch_Freq_Low" => Ok(cs705::csv::number::field::channel(holder.low_hz)),
        b"CL_PgSrch_Freq_High" => Ok(cs705::csv::number::field::channel(holder.high_hz)),
        b"CL_PgSrch_Mode" => scalar(i64::from(holder.mode), node, resources, runtime),
        b"CL_PgSrch_Data" => {
            selected_label(resources, "Misc_OnOff2", i64::from(holder.data), runtime)
        }
        b"CL_PgSrch_Filter" => {
            selected_label(resources, "Pch_Filter", i64::from(holder.filter), runtime)
        }
        _ => Err(unsupported(node)),
    }
}

fn group_cell(
    image: &Icf<'_>,
    current: &[u8],
    node: &CellNode,
    runtime: &crate::records::Runtime,
) -> Result<Vec<u8>> {
    let row = index(node)?;
    match node.name.as_slice() {
        b"CL_Group_Name" if row < 50 => {
            native_text(&prefix(window(current, 0x2ea3b + 16 * row, 16)?), runtime)
        }
        b"CL_Gps_Bank_Name" if (1..27).contains(&row) => {
            native_text(&prefix(window(current, 0x353f1 + 16 * row, 16)?), runtime)
        }
        b"CL_Mch_GrpOpe_Name" => {
            let groups = image.memory_groups()?;
            let group = groups.get(row).ok_or_else(|| unsupported(node))?;
            native_text(&prefix(group.name_bytes()), runtime)
        }
        b"CL_Mch_GrpOpe_Used" | b"CL_Mch_GrpOpe_Total" | b"CL_Mch_GrpOpe_Remine" => {
            let model = icf::rows::NormalMemory::from_working(current.to_vec())?;
            Ok(number(match node.name.as_slice() {
                b"CL_Mch_GrpOpe_Used" => model.group_used_slots(row)?,
                b"CL_Mch_GrpOpe_Total" => model.used_slots(),
                _ => 500 - model.used_slots(),
            }))
        }
        _ => Err(unsupported(node)),
    }
}

fn compact(
    current: &[u8],
    kind: icf::special::CompactKind,
    node: &CellNode,
    resources: &Resources<'_>,
    runtime: &crate::records::Runtime,
) -> Result<Vec<u8>> {
    use icf::special::{CompactField as F, DisplayValue as V};
    let field = [
        F::Frequency,
        F::Duplex,
        F::Offset,
        F::Mode,
        F::Data,
        F::Filter,
        F::Tone,
        F::TxTone,
        F::RxTone,
        F::Dtcs,
        F::Polarity,
        F::DigitalSquelch,
        F::DigitalCode,
        F::Your,
        F::Repeater1,
        F::Repeater2,
    ]
    .into_iter()
    .find(|field| u32::from(field.key()) == node.key)
    .ok_or_else(|| unsupported(node))?;
    let holder = icf::special::CompactChannel::read(current, kind, index(node)?)?;
    Ok(match holder.display(field) {
        V::Blank => Vec::new(),
        V::Placeholder => b"---".to_vec(),
        V::Number(value) => match field {
            F::Frequency => cs705::csv::number::field::channel(value),
            F::Offset => cs705::csv::number::field::compact_offset(value),
            _ => cs705::display::channel_value_paint(
                node.key,
                value,
                runtime.display_context_source(),
                |name, key, value| {
                    resources.selector_caption(
                        name,
                        key,
                        i64::from(value),
                        runtime.display_input_options().gui_language,
                        runtime.display_system_langid(),
                    )
                },
            )?,
        },
        V::Callsign(call) => prefix(&call),
    })
}

fn repeater(
    current: &[u8],
    factory: &Factory,
    node: &CellNode,
    runtime: &crate::records::Runtime,
    resources: &Resources<'_>,
) -> Result<Vec<u8>> {
    let groups = cs705::repeater::groups::Groups::from_working(current)?;
    let group = usize::try_from(factory.group)
        .map_err(|_| AppError::invalid("negative native repeater group"))?;
    let absolute = groups
        .start(group)?
        .checked_add(u32::try_from(index(node)?).map_err(|_| unsupported(node))?)
        .ok_or_else(|| unsupported(node))?;
    let mut holder = record::reset_holder(0x0c, absolute, 0);
    cs705::repeater::working::load_into(current, absolute as usize, &mut holder)?;
    if !holder.valid() {
        return Ok(Vec::new());
    }
    if (0x144..=0x153).contains(&node.key)
        && let Some(text) =
            cs705::repeater::fields::paint_meta(&holder, node.key as u16)?.placeholder()
    {
        return Ok(text.to_vec());
    }
    let n = node.name.as_slice();
    Ok(match n {
        b"CL_Repeater_List_Name" => native_text(&prefix(holder.name()), runtime)?,
        b"CL_Repeater_List_SubName" => {
            native_text(&prefix(holder.repeater_text(T::SubName)), runtime)?
        }
        b"CL_Repeater_List_R_Callsign_Code" => prefix(holder.repeater_text(T::Call)),
        b"CL_Repeater_List_G_Callsign_Code" => prefix(holder.repeater_text(T::Gateway)),
        b"CL_Repeater_List_Freq" => {
            let hz = holder.channel_field(C::FrequencyHz);
            let mut text = cs705::csv::number::field::channel(hz);
            if hz == 0 || cs705::repeater::predicate::numeric_frequency_bad(&holder) {
                text.clear();
            }
            text
        }
        b"CL_Repeater_List_Dup" => scalar(
            i64::from(holder.channel_field(C::Duplex)),
            node,
            resources,
            runtime,
        )?,
        b"CL_Repeater_List_Offset_Freq" => {
            cs705::csv::number::field::repeater_offset(holder.offset_hz())
        }
        b"CL_Repeater_List_Mode" => scalar(
            i64::from(holder.repeater_field(R::Mode)),
            node,
            resources,
            runtime,
        )?,
        b"CL_Repeater_List_Type" => scalar(
            i64::from(cs705::repeater::staging::classify_bytes(holder.as_bytes())),
            node,
            resources,
            runtime,
        )?,
        b"CL_Repeater_List_Tone" => scalar(
            i64::from(holder.repeater_field(R::Tone)),
            node,
            resources,
            runtime,
        )?,
        b"CL_Repeater_List_Tone_Freq" => scalar(
            i64::from(holder.repeater_field(R::ToneIndex)),
            node,
            resources,
            runtime,
        )?,
        b"CL_Repeater_List_Rpt1use" => scalar(
            i64::from(holder.repeater_field(R::Use)),
            node,
            resources,
            runtime,
        )?,
        b"CL_Repeater_List_Position" => scalar(
            i64::from(holder.repeater_field(R::Position)),
            node,
            resources,
            runtime,
        )?,
        b"CL_Repeater_List_UtcOffset" => {
            cs705::csv::payload::utc_text(holder.repeater_field(R::UtcOffset) as i32)?
        }
        b"CL_Repeater_List_Latitude" | b"CL_Repeater_List_Longitude" => {
            let longitude = n == b"CL_Repeater_List_Longitude";
            native_coordinate(
                *holder.coordinate_bytes(longitude),
                5 + u8::from(longitude),
                current,
                runtime,
            )?
        }
        _ => return Err(unsupported(node)),
    })
}

fn transmit_history(
    image: &Icf<'_>,
    node: &CellNode,
    runtime: &crate::records::Runtime,
    resources: &Resources<'_>,
) -> Result<Vec<u8>> {
    use crate::args::Sheet;
    let holder = match node.key {
        0x17b..=0x17f => {
            crate::records::history_holder(image, Sheet::FromDv, index(node)?, runtime)?
        }
        0x183..=0x187 => {
            crate::records::history_holder(image, Sheet::FromFm, index(node)?, runtime)?
        }
        0x188 | 0x189 => crate::records::history_holder(image, Sheet::To, index(node)?, runtime)?,
        0x18c => {
            let mut holder = record::reset_holder(0x14, index(node)? as u32, 0);
            record::load_reflector_storage_into(image, index(node)? as u32, &mut holder)?;
            holder
        }
        _ => return Err(unsupported(node)),
    };
    if !record::history_field_visible(&holder, node.key)? {
        return Ok(Vec::new());
    }
    match node.key {
        0x17b | 0x183 | 0x188 | 0x18c => Ok(prefix(holder.history_call())),
        0x17c | 0x184 | 0x189 => native_text(&prefix(holder.name()), runtime),
        0x17d | 0x185 => history_number(holder.channel_field(C::FrequencyHz), runtime),
        0x17e | 0x186 => scalar(
            i64::from(holder.channel_field(C::Duplex)),
            node,
            resources,
            runtime,
        ),
        0x17f | 0x187 => history_number(holder.offset_hz(), runtime),
        _ => Err(unsupported(node)),
    }
}

fn receive_history(
    image: &Icf<'_>,
    current: &[u8],
    node: &CellNode,
    runtime: &crate::records::Runtime,
    resources: &Resources<'_>,
) -> Result<Vec<u8>> {
    use cs705::csv::{
        datetime,
        history::paint,
        payload::{Adapter as A, Request},
    };
    let mut holder = record::reset_holder(0x0e, index(node)? as u32, 0);
    record::load_receive_history_storage_into(image, index(node)? as u32, &mut holder)?;
    if !holder.valid() {
        return Ok(Vec::new());
    }
    let source = record::receive_history_raw(&holder, node.key)?;
    let mut context = paint::Context {
        gui_language: runtime.display_input_options().gui_language,
        system_langid: runtime.display_system_langid(),
        ..Default::default()
    };
    let adapter = match node.key {
        0x156 => return scalar(i64::from(source[0]), node, resources, runtime),
        0x157..=0x15b => return Ok(prefix(source)),
        0x15c => {
            let hz = u32::from_le_bytes(source.try_into().unwrap());
            return if hz == 0 {
                Ok(Vec::new())
            } else {
                history_number(hz, runtime)
            };
        }
        0x15e => A::Message,
        0x160 => A::HistoryDate,
        0x163 | 0x164 => {
            return native_coordinate(
                source.try_into().unwrap(),
                if node.key == 0x163 { 5 } else { 6 },
                current,
                runtime,
            );
        }
        // Symbol uses the independent raw-pair formatter, not the resource oracle.
        0x167 => A::Symbol,
        0x16a => A::Timestamp,
        0x17a => A::GpsMessage,
        0x15f | 0x161 | 0x162 | 0x165 | 0x166 | 0x168 | 0x169 | 0x16b..=0x179 => {
            return Ok(paint::format_field(
                node.key,
                source,
                current,
                &context,
                Some(resources.source_pe()),
                |name, index| {
                    resources
                        .resource_line(name, index, context.gui_language, context.system_langid)
                        .map(|caption| caption.to_vec())
                        .map_err(|_| {
                            cs705::csv::Error::SourceUnavailable("native Rx named resource source")
                        })
                },
            )?);
        }
        _ => return Err(unsupported(node)),
    };
    let offset = if matches!(adapter, A::HistoryDate | A::Timestamp) {
        word(current, 0x396ff)? as i16
    } else {
        0
    };
    let needs_separator = match adapter {
        A::HistoryDate => {
            datetime::effective_offset(offset)?;
            context.date_order = Some(runtime.inline_date_order()?);
            let (date, time) = datetime::decode_raw(source.try_into().unwrap());
            !datetime::valid_date(date) || !datetime::valid_time(time)
        }
        A::Timestamp => !datetime::timestamp(source.try_into().unwrap(), offset, [0; 2]).is_empty(),
        _ => false,
    };
    if needs_separator {
        context.time_separators =
            Some([runtime.display_format_context()?.byte("time_separator")?; 2]);
    }
    Ok(paint::format(
        Request {
            adapter,
            source,
            utc_offset_minutes: offset,
        },
        &context,
        || {
            let source = runtime.display_format_context().map_err(|_| {
                cs705::csv::Error::SourceUnavailable("native Rx actual Now context")
            })?;
            let date = source
                .native_clock_date()
                .map_err(|_| cs705::csv::Error::Invalid("native Rx actual Now calendar fields"))?;
            let time = source
                .clock()
                .map_err(|_| cs705::csv::Error::Invalid("native Rx actual Now time fields"))?;
            Ok((date, time))
        },
    )?)
}

fn settings_cell(
    image: &Icf<'_>,
    current: &[u8],
    node: &CellNode,
    runtime: &crate::records::Runtime,
    resources: &Resources<'_>,
) -> Result<Vec<u8>> {
    let index = index(node)?;
    if e::number_fields()
        .iter()
        .any(|field| u32::from(field.key) == node.key)
    {
        let seconds_unit = if matches!(node.key, 0x25e | 0x278 | 0x2c8) {
            Some(resources.resource_line(
                b"GI_SecUnit",
                0,
                runtime.display_input_options().gui_language,
                runtime.display_system_langid(),
            )?)
        } else {
            None
        };
        return Ok(e::paint_number_at(
            image,
            node.key as u16,
            index,
            seconds_unit,
            runtime.display_system_langid(),
        )?);
    }
    if let Some(field) = e::ConditionalField::ALL
        .into_iter()
        .find(|field| u32::from(field.key()) == node.key)
    {
        let q = if field == e::ConditionalField::AutoRepeater {
            Some(runtime.display_context()?)
        } else {
            None
        };
        let catalog = field.catalog_sources(
            runtime.display_input_options().gui_language,
            runtime.display_system_langid(),
            q,
        )?;
        let value = catalog
            .edit(&mut image.clone(), e::ChoiceInput::Display)?
            .value;
        return selected_label(
            resources,
            catalog.selector.strip_prefix("GI_Sheet_Sel_").unwrap(),
            value,
            runtime,
        );
    }
    if let Some(field) = digital::TextField::ALL
        .into_iter()
        .find(|field| u32::from(field.key()) == node.key)
    {
        return Ok(field.display(image, index)?);
    }
    if let Some(field) = digital::SelectionField::ALL
        .into_iter()
        .find(|field| u32::from(field.key()) == node.key)
    {
        return scalar(
            i64::from(field.selected(image, index)?),
            node,
            resources,
            runtime,
        );
    }
    if let Some(field) = e::FixedTextField::ALL
        .into_iter()
        .find(|field| u32::from(field.key()) == node.key)
    {
        return Ok(e::edit_fixed_text(
            &mut image.clone(),
            field,
            index,
            e::FixedTextInput::Display,
            0,
        )?
        .text
        .display);
    }
    if let Some(field) = e::MetadataField::ALL
        .into_iter()
        .find(|field| u32::from(field.key()) == node.key)
    {
        let view = field.read(image);
        if !view.attached {
            return native_text(
                resources.metadata_empty_caption(
                    field,
                    runtime.display_input_options().gui_language,
                    runtime.display_system_langid(),
                )?,
                runtime,
            );
        }
        return native_text(&view.stem, runtime);
    }
    if let Some(field) = gps::text_fields()
        .iter()
        .find(|field| u32::from(field.key) == node.key)
    {
        return Ok(field.read(image, index)?);
    }
    if let Some(field) = gps::symbol_fields()
        .iter()
        .find(|field| u32::from(field.key) == node.key)
    {
        return Ok(field.read(image, index)?.to_vec());
    }
    if let Some(field) = gps::coordinate_fields()
        .iter()
        .chain(std::iter::once(&gps::ALARM_AREA))
        .find(|field| u32::from(field.key) == node.key)
    {
        if index != 0 {
            return Err(unsupported(node));
        }
        let mode = if field.area {
            cs705::csv::number::area_native_mode(current)?
        } else {
            5 + u8::from(field.longitude)
        };
        return native_coordinate(field.read(image), mode, current, runtime);
    }
    if let Some(field) = gps::AlarmField::ALL
        .into_iter()
        .find(|field| u32::from(field.key()) == node.key)
    {
        if index != 0 {
            return Err(unsupported(node));
        }
        if node.key == 0x1ca {
            return Ok(gps::paint_alarm_group(image, |name, value| {
                resources
                    .selector_caption(
                        name,
                        1,
                        value as i64,
                        runtime.display_input_options().gui_language,
                        runtime.display_system_langid(),
                    )
                    .map_err(|_| icf::Error::Invalid {
                        line: 0,
                        reason: "native GPS alarm selector source unavailable",
                    })
            })?);
        }
        return scalar(i64::from(field.read(image)), node, resources, runtime);
    }
    if let Some(field) = gps::SentenceField::ALL
        .into_iter()
        .find(|field| u32::from(field.key()) == node.key)
    {
        if index != 0 {
            return Err(unsupported(node));
        }
        return Ok(field.paint(image, |name, index| {
            resources
                .resource_line(
                    name,
                    index,
                    runtime.display_input_options().gui_language,
                    runtime.display_system_langid(),
                )
                .map(<[u8]>::to_vec)
                .map_err(|_| icf::Error::Invalid {
                    line: 0,
                    reason: "native GPS sentence resource source unavailable",
                })
        })?);
    }
    if let Some(field) = gps::choice_fields()
        .iter()
        .find(|field| u32::from(field.setting.key) == node.key)
    {
        let settings::Value::Number(value) = field.setting.read(image, index)?;
        if let Some(source) = selector(node.key) {
            return selected_label(resources, source, value, runtime);
        }
        let source = field.selector(
            image,
            gps::Resources {
                system_langid: runtime.display_system_langid(),
                language_descriptor: runtime.display_input_options().gui_language,
            },
        )?;
        return selected_label(
            resources,
            source
                .strip_prefix("GI_Sheet_Sel_")
                .ok_or_else(|| unsupported(node))?,
            value,
            runtime,
        );
    }
    if let Some(field) = gps::number_fields()
        .iter()
        .find(|field| u32::from(field.setting.key) == node.key)
    {
        if index != 0 {
            return Err(unsupported(node));
        }
        return Ok(field.paint(
            image,
            gps::Resources {
                language_descriptor: runtime.display_input_options().gui_language,
                system_langid: runtime.display_system_langid(),
            },
        )?);
    }
    if let Some(field) = [e::AmplifierField::Preamp, e::AmplifierField::Attenuator]
        .into_iter()
        .find(|field| u32::from(field.key()) == node.key)
    {
        let effect = field.edit(&mut image.clone(), index, e::ChoiceInput::Display)?;
        if let Some(value) = effect.fixed_value {
            return Ok(value.source_label().as_bytes().to_vec());
        }
        return selected_label(
            resources,
            if field == e::AmplifierField::Preamp && index < 13 {
                "PAMP_ATT_PAMP"
            } else {
                "Misc_OnOff"
            },
            effect.value,
            runtime,
        );
    }
    if let Some(field) = e::DrField::ALL
        .into_iter()
        .find(|field| u32::from(field.key()) == node.key)
    {
        let value = field.read(image);
        if node.key == 0x126 {
            return Ok(cs705::display::channel_value_paint(
                node.key,
                u32::from(value),
                runtime.display_context_source(),
                |name, key, value| {
                    resources.selector_caption(
                        name,
                        key,
                        i64::from(value),
                        runtime.display_input_options().gui_language,
                        runtime.display_system_langid(),
                    )
                },
            )?);
        }
        return scalar(i64::from(value), node, resources, runtime);
    }
    if let Some(field) = e::key_assignment_fields()
        .iter()
        .find(|field| u32::from(field.key()) == node.key && field.accepts_index(index))
    {
        return scalar(field.read(image, index)?, node, resources, runtime);
    }
    if let Some(field) = e::choice_fields()
        .iter()
        .chain(e::rf_choice_fields())
        .find(|field| u32::from(field.key()) == node.key && field.accepts_index(index))
    {
        if let Some(value) = field.fixed_value(index)? {
            return Ok(value.source_label().as_bytes().to_vec());
        }
        let value = field.read(image, index)?;
        if node.key == 0x1b0 {
            let langid = runtime.display_system_langid().ok_or_else(|| {
                AppError::safety("picture size selector requires actual SYSTEM LANGID")
            })?;
            return selected_label(
                resources,
                if langid & 0x3ff == 0x11 {
                    "PicSizeJpn"
                } else {
                    "PicSize"
                },
                value,
                runtime,
            );
        }
        return scalar(value, node, resources, runtime);
    }
    if let Some(field) = e::PackedField::ALL
        .into_iter()
        .find(|field| u32::from(field.key()) == node.key)
    {
        if index != 0 {
            return Err(unsupported(node));
        }
        let value = field.read(image)?;
        if field == e::PackedField::DeltaFSpan {
            let langid = runtime.display_system_langid().ok_or_else(|| {
                AppError::safety("delta span selector requires actual SYSTEM LANGID")
            })?;
            let source = field.selector(langid).ok_or_else(|| unsupported(node))?;
            return Ok(resources.selector_caption(
                source.as_bytes(),
                1,
                value,
                runtime.display_input_options().gui_language,
                runtime.display_system_langid(),
            )?);
        }
        return scalar(value, node, resources, runtime);
    }
    if let Some(field) = settings::catalog()
        .iter()
        .find(|field| u32::from(field.key) == node.key && index < field.count)
    {
        if node.key == 0x229 {
            return Ok(e::utc_offset_text(
                field.read_numeric_raw(image, index)? as i16
            ));
        }
        let settings::Value::Number(value) = field.read(image, index)?;
        return scalar(value, node, resources, runtime);
    }
    Err(unsupported(node))
}

fn gps_memory(
    current: &[u8],
    factory: &Factory,
    node: &CellNode,
    runtime: &crate::records::Runtime,
    resources: &Resources<'_>,
) -> Result<Vec<u8>> {
    let group = usize::try_from(factory.group).map_err(|_| unsupported(node))?;
    if group >= 27 {
        return Err(unsupported(node));
    }
    let row = index(node)?;
    if row >= usize::from(gps::gps_group_count_source(current, group as u8)?) {
        return Ok(Vec::new());
    }
    let start = usize::from(word(current, 0x3c5dd + 2 * group)?);
    let physical = usize::from(word(current, 0x3c385 + 2 * (start + row))?);
    if physical >= 300 {
        return Err(AppError::invalid(
            "native GPS physical source outside dataset",
        ));
    }
    let raw = window(current, 0x32c19 + 34 * physical, 34)?;
    let mut holder = record::reset_holder(0x0a, row as u32, 0);
    record::decode_gps_into(&mut holder, raw.try_into().unwrap(), group as u32);
    holder.set_valid(true);
    match node.name.as_slice() {
        b"CL_Gps_Name" => native_text(&prefix(&holder.as_bytes()[0x244..0x254]), runtime),
        b"CL_Gps_Latitude" | b"CL_Gps_Longitude" => {
            let longitude = node.name == b"CL_Gps_Longitude";
            native_coordinate(
                *holder.coordinate_bytes(longitude),
                5 + u8::from(longitude),
                current,
                runtime,
            )
        }
        b"CL_Gps_Altitude" => {
            let old = i32::from_le_bytes(holder.as_bytes()[0x264..0x268].try_into().unwrap());
            Ok(gps::paint_altitude_raw(
                old,
                window(current, 0x396b5, 1)?[0] == 0,
            ))
        }
        b"CL_Gps_Date" | b"CL_Gps_Time" => {
            let context = runtime.display_format_context()?;
            let datetime = context.datetime()?;
            let raw = holder.as_bytes()[0x254..0x25a].try_into().unwrap();
            let (date, time) = cs705::csv::datetime::decode_raw(raw);
            let failed_time = if cs705::csv::datetime::valid_time(time)
                || node.key == 0x1b6 && !cs705::csv::datetime::valid_date(date)
            {
                time
            } else {
                context.clock()?
            };
            let offset = word(current, 0x396ff)? as i16;
            Ok(if node.key == 0x1b6 {
                cs705::csv::datetime::gps_date(raw, offset, &datetime, failed_time, |request| {
                    runtime.display_native_date(request)
                })?
            } else {
                cs705::csv::datetime::gps_time(raw, offset, &datetime, failed_time)?
            })
        }
        b"CL_Gps_Alarm" => selected_label(
            resources,
            selector(node.key).ok_or_else(|| unsupported(node))?,
            i64::from(
                window(current, 0x3c622, 2)? == [3, group as u8]
                    && usize::from(word(current, 0x3c624)?) == row,
            ),
            runtime,
        ),
        _ => Err(unsupported(node)),
    }
}

fn preset(
    current: &[u8],
    factory: &Factory,
    node: &CellNode,
    runtime: &crate::records::Runtime,
    resources: &Resources<'_>,
) -> Result<Vec<u8>> {
    let p = usize::try_from(factory.group).map_err(|_| unsupported(node))?;
    let holder = icf::special::Preset::read(current, p)?;
    if node.name == b"CL_Common_Preset_Name" {
        return native_text(&prefix(&holder.body[25..]), runtime);
    }
    if node.key == 0x385 {
        return Ok(holder
            .checkbox_paint(u32::try_from(index(node)?).map_err(|_| unsupported(node))?)?
            .0);
    }
    if node.key == 0x389 {
        return Ok(holder.bandwidth_paint()?);
    }
    let field = icf::special::PRESET_CATALOG
        .iter()
        .find(|field| u32::from(field.key) == node.key)
        .ok_or_else(|| unsupported(node))?;
    if holder.incompatible(field.field)? {
        return Ok(b"---".to_vec());
    }
    if matches!(node.key, 0x38d | 0x38f | 0x390 | 0x39a) {
        let raw = holder.paint_byte(node.key)?;
        return Ok(if node.key == 0x39a {
            e::paint_hex_byte(raw)
        } else {
            e::paint_level(raw)
        });
    }
    let value = match node.key {
        0x387 => i64::from(holder.combined_mode()),
        _ => i64::from(
            *holder
                .body
                .get(field.offset + index(node)?)
                .ok_or_else(|| unsupported(node))?,
        ),
    };
    if matches!(node.key, 0x392 | 0x396) {
        return selected_label(
            resources,
            if node.index == 0 {
                "ToneControl_Tbw_Low"
            } else {
                "ToneControl_Tbw_High"
            },
            value,
            runtime,
        );
    }
    scalar(value, node, resources, runtime)
}

pub fn print_cell(
    image: &Icf<'_>,
    current: &[u8],
    factory: &Factory,
    node: &CellNode,
    runtime: &crate::records::Runtime,
    resources: &Resources<'_>,
) -> Result<Vec<u8>> {
    if node.key == 0x3ae {
        return Ok(node.literal.clone());
    }
    if node.key == 0xed {
        return Ok(Vec::new());
    }
    let index = index(node)?;
    if matches!(node.name.as_slice(), b"CL_SP1" | b"CL_SP2" | b"CL_SP3") {
        return Ok(node.literal.clone());
    }
    match node.name.as_slice() {
        b"CL_Common_Network_IPAddress"
        | b"CL_Common_Network_DefaultGateway"
        | b"CL_Common_Network_PrimaryDNS"
        | b"CL_Common_Network_SecondaryDNS"
        | b"CL_Common_Network_SubnetMask" => {
            let config = image.network_configuration();
            return Ok(match node.name.as_slice() {
                b"CL_Common_Network_IPAddress" => number(config.address),
                b"CL_Common_Network_DefaultGateway" => number(config.gateway),
                b"CL_Common_Network_PrimaryDNS" => number(config.primary_dns),
                b"CL_Common_Network_SecondaryDNS" => number(config.secondary_dns),
                _ => selected_label(
                    resources,
                    "Common_Network_SubnetMask",
                    i64::from(config.prefix),
                    runtime,
                )?,
            });
        }
        b"CL_Common_Network_User1ID" | b"CL_Common_Network_User2ID" => {
            return Ok(prefix(window(
                current,
                if node.name == b"CL_Common_Network_User1ID" {
                    0x39383
                } else {
                    0x393a4
                },
                16,
            )?));
        }
        b"CL_Common_Network_User1Password" | b"CL_Common_Network_User2Password" => {
            let raw = window(
                current,
                if node.name == b"CL_Common_Network_User1Password" {
                    0x39393
                } else {
                    0x393b4
                },
                16,
            )?;
            let mut scratch = image.clone();
            let mut source = [0; 63];
            source[..16].copy_from_slice(raw);
            scratch.patch_memory(&[(0x39a27, &source)])?;
            return Ok(e::edit_fixed_text(
                &mut scratch,
                e::FixedTextField::ApPassword,
                0,
                e::FixedTextInput::Display,
                0,
            )?
            .text
            .display);
        }
        _ => (),
    }
    match node.name.as_slice() {
        b"CL_Misc_ChNo_0Base" => return Ok(format!("{index:02}").into_bytes()),
        b"CL_Misc_ChNo_1Base"
        | b"CL_Misc_ChNo_New"
        | b"CL_DTMF_ChNo"
        | b"CL_Keyer_ChNo"
        | b"CL_RttyTxmem_ChNo"
        | b"CL_My_Callsign_ChNo"
        | b"CL_Tx_Message_ChNo"
        | b"CL_You_Callsign_ChNo"
        | b"CL_Dvgw_Allowcs_ChNo"
        | b"CL_Gps_ChNo"
        | b"CL_Repeater_List_ChNo" => return Ok(format!("{:02}", index + 1).into_bytes()),
        b"CL_Misc_MPadNo_1Base" => return Ok(format!("{index:02}").into_bytes()),
        b"CL_Misc_GroupNm" => return Ok(format!("{:02}", index + 1).into_bytes()),
        b"CL_Misc_BankNm" => return Ok(cs705::csv::table::gps_group_text(index as u32)?),
        b"CL_Misc_CchNo" => {
            return Ok(number(
                node.typed_index.ok_or_else(|| unsupported(node))? / 2 + 1,
            ));
        }
        _ => (),
    }
    if node.name.starts_with(b"CL_Mch_GrpOpe_")
        || matches!(node.name.as_slice(), b"CL_Group_Name" | b"CL_Gps_Bank_Name")
    {
        return group_cell(image, current, node, runtime);
    }
    if node.name.starts_with(b"CL_BandEdge_") || node.name.starts_with(b"CL_PgSrch_") {
        return special_edge(current, node, runtime, resources);
    }
    if (0x17b..=0x18c).contains(&node.key) {
        return transmit_history(image, node, runtime, resources);
    }
    if (0x156..=0x17a).contains(&node.key) {
        return receive_history(image, current, node, runtime, resources);
    }
    let parent = match factory.resource.as_str() {
        "GI_Sheet_AGC" => Some(cs705::display::NoticeParent::Agc),
        "GI_Sheet_Filter" => Some(cs705::display::NoticeParent::Filter),
        "GI_Sheet_TS" => Some(cs705::display::NoticeParent::Ts),
        "GI_Sheet_ToneControl" => Some(cs705::display::NoticeParent::ToneControl),
        _ => None,
    };
    if let Some(parent) = parent {
        if factory.resource == "GI_Sheet_Filter" && (0x12f..=0x132).contains(&node.key) {
            return Ok(cs705::display::filter::paint(
                image,
                node.key as u16,
                index,
                Some(resources.source_pe()),
            )?);
        }
        if let Some(notice) = cs705::display::notice_cell(
            Some(resources.source_pe()),
            parent,
            node.key,
            node.index,
            false,
        )? {
            return Ok(notice.text);
        }
    }
    match factory.class {
        Class::Split => {
            let kind = if factory.node == 103 {
                record_image::ChannelKind::Call
            } else if (2..=101).contains(&factory.node) {
                record_image::ChannelKind::Normal
            } else {
                return Err(unsupported(node));
            };
            let typed = node.typed_index.ok_or_else(|| {
                AppError::invalid("native split cell needs generated typed index")
            })?;
            let mut holder = record::reset_holder(
                if kind == record_image::ChannelKind::Normal {
                    2
                } else {
                    3
                },
                typed,
                0,
            );
            record_image::load_channel_into(current, kind, typed, &mut holder)?;
            if node.key == 0x105 && holder.valid() {
                return native_text(&prefix(holder.name()), runtime);
            }
            if holder.valid() {
                if matches!(node.key, 0x106 | 0x108..=0x10b | 0x10d..=0x113 | 0x117) {
                    return Ok(cs705::display::channel_paint(
                        &holder,
                        node.key,
                        runtime.display_context_source(),
                        |name, key, value| {
                            resources.selector_caption(
                                name,
                                key,
                                i64::from(value),
                                runtime.display_input_options().gui_language,
                                runtime.display_system_langid(),
                            )
                        },
                    )?);
                }
                match node.key {
                    0x107 => {
                        return Ok(cs705::csv::number::field::channel(
                            holder.channel_field(C::FrequencyHz),
                        ));
                    }
                    0x10c => return Ok(cs705::csv::number::field::channel(holder.offset_hz())),
                    _ => (),
                }
            }
            return Ok(cs705::display::channel_cell(
                &holder,
                node.key,
                typed % 200 / 2,
            )?);
        }
        Class::Simplex if factory.node == 401 => {
            return compact(
                current,
                icf::special::CompactKind::BandStack,
                node,
                resources,
                runtime,
            );
        }
        Class::Simplex if factory.node == 402 => {
            return compact(
                current,
                icf::special::CompactKind::Memopad,
                node,
                resources,
                runtime,
            );
        }
        Class::Ur | Class::Allowed => {
            let mut holder = record::reset_holder(
                if factory.class == Class::Ur {
                    0x0d
                } else {
                    0x17
                },
                index as u32,
                0,
            );
            record_image::load_into(current, holder.kind(), index as u32, &mut holder)?;
            if !holder.valid() {
                return Ok(Vec::new());
            }
            return match node.name.as_slice() {
                b"CL_You_Callsign_Name" => native_text(&prefix(holder.name()), runtime),
                b"CL_You_Callsign_Code" | b"CL_Dvgw_Allowcs_CallSign" => {
                    Ok(prefix(holder.callsign(MemoryCallsign::Your)))
                }
                _ => Err(unsupported(node)),
            };
        }
        Class::Repeater => return repeater(current, factory, node, runtime, resources),
        Class::Gps => return gps_memory(current, factory, node, runtime, resources),
        Class::Preset => return preset(current, factory, node, runtime, resources),
        _ => (),
    }
    settings_cell(image, current, node, runtime, resources)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource_fixture() -> Vec<u8> {
        let mut bytes = vec![0; 0x463400];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        bytes[0x84..0x86].copy_from_slice(&0x14c_u16.to_le_bytes());
        bytes[0x86..0x88].copy_from_slice(&1_u16.to_le_bytes());
        bytes[0x94..0x96].copy_from_slice(&96_u16.to_le_bytes());
        bytes[0x98..0x9a].copy_from_slice(&0x10b_u16.to_le_bytes());
        bytes[0xb4..0xb8].copy_from_slice(&0x400000_u32.to_le_bytes());
        let section = 0x98 + 96;
        bytes[section + 12..section + 16].copy_from_slice(&0x190000_u32.to_le_bytes());
        bytes[section + 16..section + 20].copy_from_slice(&0x2d4600_u32.to_le_bytes());
        bytes[section + 20..section + 24].copy_from_slice(&0x18ee00_u32.to_le_bytes());
        let at = |rva: usize| rva - 0x1200;
        bytes[at(0x190001)..at(0x190007)].copy_from_slice(b"Flex0\0");
        bytes[at(0x190007)..at(0x19000c)].copy_from_slice(b"Mode\0");
        bytes[at(0x190020)..at(0x19002c)].copy_from_slice(b"#End_Of_Ovl\0");
        for (rva, count) in [(0x3f2ae0, 0x19), (0x3f2b44, 0x3ae)] {
            for i in 0..count {
                bytes[at(rva + 4 * i)..at(rva + 4 * i + 4)]
                    .copy_from_slice(&0x590000_u32.to_le_bytes());
            }
        }
        bytes[at(0x3f2ae0 + 4)..at(0x3f2ae0 + 8)].copy_from_slice(&0x590001_u32.to_le_bytes());
        bytes[at(0x3f2ae0 + 7 * 4)..at(0x3f2ae0 + 8 * 4)]
            .copy_from_slice(&0x590007_u32.to_le_bytes());
        fn add_selector(lines: &mut Vec<Vec<u8>>, name: &str, key: &str, rows: &[(u32, &str)]) {
            lines.extend([
                format!("#GI_Sheet_Sel_{name}_Style").into_bytes(),
                b"(FixedRows=0)(FixedCols=0)".to_vec(),
                format!("#GI_Sheet_Sel_{name}_BasicCell").into_bytes(),
                b"{($Basic)}".to_vec(),
                format!("#GI_Sheet_Sel_{name}_Cells").into_bytes(),
            ]);
            lines.extend(
                rows.iter()
                    .map(|(value, text)| format!("{{S(%{key}[{value}])}}{text}").into_bytes()),
            );
        }
        for (source, (vector, count)) in [(0x301918, 6084), (0x307828, 6084), (0x30d738, 6088)]
            .into_iter()
            .enumerate()
        {
            let mut lines = vec![b"#CellModel_Basic".to_vec(), Vec::new()];
            for (name, rows) in [
                ("Misc_OnOff", &[(0, "OFF"), (1, "ON")][..]),
                ("Misc_OnOff2", &[(0, "OFF2"), (1, "ON2")]),
                ("MenuMode_GpsFormat_JPN", &[(0, "actual-JPN")]),
                ("Rpt_Dup", &[(3, "actual-RPS")]),
                ("Mch_Filter", &[(0, "filter0"), (2, "filter2")]),
                ("Mch_Dup", &[(0, "dup0")]),
                ("Mch_TSQL", &[(0, "tone0")]),
                ("Mch_DTCS_Polarity", &[(2, "polarity2")]),
                ("Mch_DSQL", &[(0, "dsql0")]),
                ("Mch_Scan_Sel", &[(0, "selection0")]),
                (
                    "Common_Gps_Alarm_Bank_Group",
                    &[(0, "group0"), (27, "group27")],
                ),
                (
                    "Common_Gps_Alarm_Bank_CH",
                    &[(0, if source == 1 { "bank0-system" } else { "bank0" })],
                ),
                ("PicSizeJpn", &[(0, "picture-jpn0")]),
                ("PicSize", &[(0, "picture0")]),
                ("Common_dFSpanJpn", &[(0, "span-jpn0")]),
                ("Common_dFSpan", &[(0, "span0")]),
            ] {
                add_selector(&mut lines, name, "Flex0", rows);
            }
            add_selector(
                &mut lines,
                "Mch_Mode",
                "Mode",
                &[(13, "actual-mode13"), (0, "mode0")],
            );
            add_selector(
                &mut lines,
                "GPS_Alarm",
                "Flex0",
                &[
                    (0, "actual-alarm0"),
                    (
                        1,
                        if source == 2 {
                            "other-alarm1"
                        } else {
                            "actual-alarm1"
                        },
                    ),
                ],
            );
            if source == 2 {
                lines.push(b"{S(%Flex0[2]:)}ignored".to_vec());
            }
            lines.extend([
                b"#GI_SecUnit".to_vec(),
                if source == 2 {
                    b"alt".to_vec()
                } else {
                    b"actual-sec".to_vec()
                },
                b"#GI_CsvExp_RxHis_HeaderRptInfo".to_vec(),
                Vec::new(),
                b"ABCD".to_vec(),
                b"EFG".to_vec(),
                b"HIJKLM".to_vec(),
                b"#GI_GpsLogSentence".to_vec(),
                b"log0".to_vec(),
                if source == 2 {
                    b"other-log1".to_vec()
                } else {
                    b"log1".to_vec()
                },
                b"log2".to_vec(),
                b"log3".to_vec(),
                b"#".to_vec(),
            ]);
            assert!(lines.len() + 1 < count);
            let mut rva = 0x200000 + source * 0x10000;
            for i in 0..count {
                let pointer = if i == count - 1 {
                    0x590020_u32
                } else if let Some(line) = lines.get(i) {
                    assert!(!line.contains(&0) && line.len() < 4096);
                    bytes[at(rva)..at(rva) + line.len()].copy_from_slice(line);
                    let pointer = 0x400000 + rva as u32;
                    rva += line.len() + 1;
                    pointer
                } else {
                    // The standalone header closes the dictionary before padding.
                    0x590000
                };
                bytes[at(vector + i * 4)..at(vector + i * 4 + 4)]
                    .copy_from_slice(&pointer.to_le_bytes());
            }
        }
        bytes[at(0x3f90d0)..at(0x3f90d6)].copy_from_slice(b"%dHz \0");
        bytes[at(0x3f3a2c + 4 * 36)..at(0x3f3a30 + 4 * 36)]
            .copy_from_slice(&(-125_i32).to_le_bytes());
        bytes
    }

    #[test]
    fn print_cells_use_typed_sources_selectors_and_secret_masking() {
        let pe = resource_fixture();
        let resources = Resources::from_pe(&pe).unwrap();
        let mut raw = vec![0; 0x3c9c0];
        raw[0x355a1] = 6;
        raw[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        let mut image = Icf::from_memory(raw).unwrap();
        image.set_vox(true).unwrap();
        image.patch_memory(&[(0x39a27, b"private\0")]).unwrap();
        let runtime = crate::records::Options::default()
            .load(std::iter::empty::<&str>())
            .unwrap();
        let factory = Factory::parse(b"GI_Sheet_Common,0").unwrap();
        let mut node = CellNode {
            key: 0x25b,
            name: b"CL_Common_VOX".to_vec(),
            ..CellNode::default()
        };
        assert_eq!(
            print_cell(
                &image,
                image.memory(),
                &factory,
                &node,
                &runtime,
                &resources
            )
            .unwrap(),
            b"ON"
        );
        node.key = 0x31f;
        node.name = b"CL_Common_Network_AccessPoint_Password".to_vec();
        assert_eq!(
            print_cell(
                &image,
                image.memory(),
                &factory,
                &node,
                &runtime,
                &resources
            )
            .unwrap(),
            b"*******"
        );
        node.key = 0x3ae;
        node.index = -1;
        node.literal = b"literal\0tail".to_vec();
        assert_eq!(
            print_cell(
                &image,
                image.memory(),
                &factory,
                &node,
                &runtime,
                &resources
            )
            .unwrap(),
            b"literal\0tail"
        );
        node.key = 0xffff;
        node.index = 0;
        node.name = b"UNKNOWN".to_vec();
        assert!(
            print_cell(
                &image,
                image.memory(),
                &factory,
                &node,
                &runtime,
                &resources
            )
            .is_err()
        );
        assert!(print_condition(b"unknown", &runtime).is_err());
        assert_eq!(image.memory()[0x39a27..0x39a2f], *b"private\0");

        node.key = 0x167;
        node.name = b"CL_RxHis_Symbol".to_vec();
        for (raw, expected) in [
            (b"/>".as_slice(), b"APRS 2F3E".as_slice()),
            (b"\xff>", b"No symbol"),
        ] {
            image.patch_memory(&[(0x31749, raw)]).unwrap();
            assert_eq!(
                print_cell(
                    &image,
                    image.memory(),
                    &factory,
                    &node,
                    &runtime,
                    &resources
                )
                .unwrap(),
                expected
            );
            assert_eq!(&image.memory()[0x31749..0x3174b], raw);
        }

        let mut message = [b' '; 20];
        message[..4].copy_from_slice(b"A,B;");
        image.patch_memory(&[(0x31355, &message)]).unwrap();
        let japanese = crate::records::Options::default()
            .load(std::iter::empty::<&str>())
            .unwrap()
            .with_input_options(cs705::display::InputOptions {
                gui_language: Some(1),
                ..Default::default()
            });
        assert_eq!(
            native_text(&prefix(b"\x81\x40\xa1 \0tail"), &japanese).unwrap(),
            b"\x81\x40\xa1 "
        );
        assert!(native_text(b"name", &runtime).is_err());
        node.key = 0x15e;
        node.name = b"CL_RxHis_Message".to_vec();
        assert_eq!(
            print_cell(
                &image,
                image.memory(),
                &factory,
                &node,
                &japanese,
                &resources
            )
            .unwrap(),
            b"A,B;"
        );

        let context_path =
            std::env::temp_dir().join(format!("cs705-native-print-{}.json", std::process::id()));
        let mut format_source = serde_json::json!({"profile":"portable-ascii","short_date":"dd/mm/yyyy","date_separator":"-","time_separator":".","rtl_decimal":".","current_year":2026,"two_digit_window":50,"clock":"12.00.00","native_date_order":1});
        std::fs::write(&context_path, serde_json::to_vec(&format_source).unwrap()).unwrap();
        let format_options = crate::records::Options {
            native_format_context: Some(context_path.to_str().unwrap().to_owned()),
            ..Default::default()
        };
        let format_runtime = format_options.load(std::iter::empty::<&str>()).unwrap();
        format_source["native_clock_date"] = serde_json::json!([2026, 11, 10]);
        format_source["native_decimal_copy"] = serde_json::json!(".");
        std::fs::write(&context_path, serde_json::to_vec(&format_source).unwrap()).unwrap();
        let clock_runtime = format_options.load(std::iter::empty::<&str>()).unwrap();
        std::fs::remove_file(context_path).unwrap();
        image
            .patch_memory(&[(0x30a1e, &[0x08, 0x11, 0x26, 0x23, 0x30, 0x45])])
            .unwrap();
        let mut current = image.memory().to_vec();
        current[0x3c385..0x3c5dd].fill(255);
        current[0x3c385..0x3c387].fill(0);
        current[0x3c5dd..0x3c613].fill(255);
        current[0x3c5dd..0x3c5df].fill(0);
        current[0x32c21..0x32c27].copy_from_slice(&[0x08, 0x11, 0x26, 0x23, 0x30, 0x45]);
        current[0x396ff..0x39701].copy_from_slice(&60_i16.to_le_bytes());
        let gps = Factory::parse(b"[St_ChGps]GI_Sheet_Gps,252").unwrap();
        node.key = 0x1b6;
        node.name = b"CL_Gps_Date".to_vec();
        assert!(print_cell(&image, &current, &gps, &node, &runtime, &resources).is_err());
        assert_eq!(
            print_cell(&image, &current, &gps, &node, &format_runtime, &resources).unwrap(),
            b"09-11-2026"
        );
        node.key = 0x1b7;
        node.name = b"CL_Gps_Time".to_vec();
        assert_eq!(
            print_cell(&image, &current, &gps, &node, &format_runtime, &resources).unwrap(),
            b"00.30.45"
        );
        current[0x32c24..0x32c27].fill(0x99);
        assert_eq!(
            print_cell(&image, &current, &gps, &node, &format_runtime, &resources).unwrap(),
            b""
        );
        node.key = 0x160;
        node.name = b"CL_RxHis_Date".to_vec();
        assert_eq!(
            print_cell(
                &image,
                &current,
                &factory,
                &node,
                &format_runtime,
                &resources
            )
            .unwrap(),
            b"11/09/2026 00:30:45"
        );
        image.patch_memory(&[(0x30a1e, &[0xff; 6])]).unwrap();
        assert!(
            print_cell(
                &image,
                &current,
                &factory,
                &node,
                &format_runtime,
                &resources
            )
            .is_err()
        );
        assert_eq!(
            print_cell(
                &image,
                &current,
                &factory,
                &node,
                &clock_runtime,
                &resources
            )
            .unwrap(),
            b"--/--/---- --.--.--"
        );

        current[0x32c19..0x32c1d].copy_from_slice(&[3, 0x30, 0x39, b'N']);
        node.key = 0x1b8;
        node.name = b"CL_Gps_Latitude".to_vec();
        current[0x396b4] = 1;
        assert_eq!(
            print_cell(&image, &current, &gps, &node, &japanese, &resources).unwrap(),
            b"03\xdf12'21\"N"
        );
        current[0x396b4] = 3;
        assert_eq!(
            print_cell(&image, &current, &gps, &node, &japanese, &resources).unwrap(),
            b""
        );
        current[0x396b4] = 2;
        node.key = 0x1c7;
        assert_eq!(
            settings_cell(&image, &current, &node, &japanese, &resources).unwrap(),
            b"0.0014\xdf"
        );
        assert_eq!(&image.memory()[0x3c626..0x3c628], &[0, 0]);

        let mut name = [b' '; 16];
        name[..9].copy_from_slice(b"\x81\x40\xa1 \0tail");
        current[0x32c2b..0x32c3b].copy_from_slice(&name);
        node.key = 0x1b5;
        node.name = b"CL_Gps_Name".to_vec();
        assert_eq!(
            print_cell(&image, &current, &gps, &node, &japanese, &resources).unwrap(),
            b"\x81\x40\xa1 "
        );
        current.resize(cs705::clone::IMAGE_CAPACITY, 0);
        current[0x2f703..0x2f713].copy_from_slice(&name);
        current[0x49978..0x4997a].copy_from_slice(&1_u16.to_le_bytes());
        let ur = Factory::parse(b"[St_UrList]GI_Sheet_YouCallsign,0").unwrap();
        node.key = 0x13f;
        node.name = b"CL_You_Callsign_Name".to_vec();
        assert_eq!(
            print_cell(&image, &current, &ur, &node, &japanese, &resources).unwrap(),
            b"\x81\x40\xa1 "
        );
        current[0x49978..0x4997a].fill(0);
        assert_eq!(
            print_cell(&image, &current, &ur, &node, &runtime, &resources).unwrap(),
            b""
        );

        node.key = 0x22a;
        node.name = b"CL_Common_GpsFormat".to_vec();
        assert_eq!(
            print_cell(&image, &current, &factory, &node, &japanese, &resources).unwrap(),
            b"actual-JPN"
        );

        current[0x49914..0x49916].copy_from_slice(&1_u16.to_le_bytes());
        let mut holder = record::reset_holder(0x0c, 0, 0);
        holder.set_valid(true);
        holder.set_repeater_field(R::Mode, 3);
        holder.set_channel_field(C::Duplex, 3);
        cs705::repeater::working::save_into(&mut current, 0, &holder).unwrap();
        let rpt = Factory::parse(b"[St_RptList]GI_Sheet_Repeater,123").unwrap();
        node.key = 0x14b;
        node.name = b"CL_Repeater_List_Dup".to_vec();
        assert_eq!(
            print_cell(&image, &current, &rpt, &node, &runtime, &resources).unwrap(),
            b"actual-RPS"
        );
        holder.set_repeater_field(R::Mode, 2);
        holder.set_repeater_text(T::Call, b"CALLSIGN");
        cs705::repeater::working::save_into(&mut current, 0, &holder).unwrap();
        node.key = 0x14d;
        node.name = b"CL_Repeater_List_Tone".to_vec();
        assert_eq!(
            print_cell(&image, &current, &rpt, &node, &runtime, &resources).unwrap(),
            b"---"
        );

        node.key = 0x1a5;
        node.name = b"CL_Common_ConnectionIPList".to_vec();
        image.patch_memory(&[(0x399c1, b".lst\0")]).unwrap();
        assert_eq!(
            print_cell(&image, &current, &factory, &node, &japanese, &resources).unwrap(),
            b""
        );
        image.patch_memory(&[(0x399c1, b"\0")]).unwrap();
        assert!(print_cell(&image, &current, &factory, &node, &japanese, &resources).is_err());

        node.key = 0x14c;
        node.name = b"CL_Repeater_List_Offset_Freq".to_vec();
        assert_eq!(
            print_cell(&image, &current, &rpt, &node, &runtime, &resources).unwrap(),
            b"  0.000000"
        );
        let compact_at = icf::special::CompactKind::BandStack.offset(0).unwrap();
        current[compact_at..compact_at + 4].fill(0);
        current[compact_at + 10..compact_at + 14].copy_from_slice(&u32::MAX.to_le_bytes());
        let bandstack = Factory::parse(b"[St_ColCut_Simplex]GI_Sheet_BandStack,401").unwrap();
        node.key = 0x10c;
        node.name = b"CL_Mch_Offset_Freq".to_vec();
        assert_eq!(
            print_cell(&image, &current, &bandstack, &node, &runtime, &resources).unwrap(),
            b"-   0.000001"
        );
        image
            .patch_memory(&[(0x30a19, &u32::MAX.to_le_bytes())])
            .unwrap();
        node.key = 0x15c;
        node.name = b"CL_RxHis_Freq".to_vec();
        assert!(
            print_cell(
                &image,
                &current,
                &factory,
                &node,
                &format_runtime,
                &resources
            )
            .is_err()
        );
        assert_eq!(
            print_cell(
                &image,
                &current,
                &factory,
                &node,
                &clock_runtime,
                &resources
            )
            .unwrap(),
            b"4294.967295"
        );

        node.key = 0x307;
        node.name = b"CL_Common_CIVAddress".to_vec();
        assert_eq!(
            print_cell(&image, &current, &factory, &node, &runtime, &resources).unwrap(),
            b"00h"
        );
        node.key = 0x25e;
        assert!(print_cell(&image, &current, &factory, &node, &runtime, &resources).is_err());
        assert!(
            print_cell(&image, &current, &factory, &node, &japanese, &resources)
                .unwrap()
                .ends_with(b"actual-sec")
        );
        let other_language = crate::records::Options::default()
            .load(std::iter::empty::<&str>())
            .unwrap()
            .with_input_options(cs705::display::InputOptions {
                gui_language: Some(0),
                ..Default::default()
            });
        assert!(
            print_cell(
                &image,
                &current,
                &factory,
                &node,
                &other_language,
                &resources
            )
            .unwrap()
            .ends_with(b"alt")
        );
        node.key = 0x362;
        image
            .patch_memory(&[(0x377db, &201_u16.to_le_bytes())])
            .unwrap();
        assert!(print_cell(&image, &current, &factory, &node, &runtime, &resources).is_ok());
        image
            .patch_memory(&[(0x377db, &200_u16.to_le_bytes())])
            .unwrap();
        assert!(print_cell(&image, &current, &factory, &node, &runtime, &resources).is_err());

        image
            .patch_memory(&[(0x3c778, &102_u16.to_le_bytes()), (0x396b6, &[0])])
            .unwrap();
        node.key = 0x1f0;
        assert_eq!(
            print_cell(&image, &current, &factory, &node, &runtime, &resources).unwrap(),
            b"19km/h"
        );
        image
            .patch_memory(&[(0x3c778, &u16::MAX.to_le_bytes())])
            .unwrap();
        assert_eq!(
            print_cell(&image, &current, &factory, &node, &runtime, &resources).unwrap(),
            b"6554km/h"
        );
        image
            .patch_memory(&[(0x3c776, &u16::MAX.to_le_bytes())])
            .unwrap();
        node.key = 0x1ef;
        assert_eq!(
            print_cell(&image, &current, &factory, &node, &japanese, &resources).unwrap(),
            b"65535\xdf"
        );
        node.key = 0x1cb;
        assert_eq!(
            print_cell(&image, &current, &factory, &node, &runtime, &resources).unwrap(),
            b"--"
        );
        image
            .patch_memory(&[(0x3c622, &[3]), (0x3c624, &u16::MAX.to_le_bytes())])
            .unwrap();
        assert_eq!(
            print_cell(&image, &current, &factory, &node, &runtime, &resources).unwrap(),
            b"65535"
        );
        current[0x32c27..0x32c2b].copy_from_slice(&i32::MAX.to_le_bytes());
        current[0x396b5] = 1;
        node.key = 0x1ba;
        node.name = b"CL_Gps_Altitude".to_vec();
        assert_eq!(
            print_cell(&image, &current, &gps, &node, &runtime, &resources).unwrap(),
            b"------ft"
        );

        image.patch_memory(&[(0x309c3, &[2])]).unwrap();
        node.key = 0x15f;
        node.name = b"CL_RxHis_Status".to_vec();
        assert_eq!(
            print_cell(&image, &current, &factory, &node, &runtime, &resources).unwrap(),
            b"EFG"
        );
        image.patch_memory(&[(0x309c3, &[0])]).unwrap();
        assert_eq!(
            print_cell(&image, &current, &factory, &node, &runtime, &resources).unwrap(),
            b""
        );

        node.key = 0x14a;
        node.name = b"CL_Repeater_List_Freq".to_vec();
        assert_eq!(
            print_cell(&image, &current, &rpt, &node, &runtime, &resources).unwrap(),
            b""
        );
        holder.set_channel_field(C::FrequencyHz, 145_000_000);
        holder.set_channel_field(C::Duplex, 0);
        holder.set_repeater_field(R::Use, 1);
        cs705::repeater::working::save_into(&mut current, 0, &holder).unwrap();
        assert_eq!(
            print_cell(&image, &current, &rpt, &node, &runtime, &resources).unwrap(),
            b" 145.000000"
        );
        holder.set_channel_field(C::FrequencyHz, 29_999);
        cs705::repeater::working::save_into(&mut current, 0, &holder).unwrap();
        assert_eq!(
            print_cell(&image, &current, &rpt, &node, &runtime, &resources).unwrap(),
            b""
        );

        let q_path =
            std::env::temp_dir().join(format!("cs705-print-history-q-{}.bin", std::process::id()));
        std::fs::write(&q_path, [0; 24]).unwrap();
        let history_options = crate::records::Options {
            record_context: Some(q_path.to_str().unwrap().to_owned()),
            ..Default::default()
        };
        let history_runtime = history_options.load(std::iter::empty::<&str>()).unwrap();
        std::fs::write(
            format_options.native_format_context.as_ref().unwrap(),
            serde_json::to_vec(&format_source).unwrap(),
        )
        .unwrap();
        let formatted_history = crate::records::Options {
            native_format_context: format_options.native_format_context.clone(),
            ..history_options.clone()
        }
        .load(std::iter::empty::<&str>())
        .unwrap();
        std::fs::remove_file(q_path).unwrap();
        std::fs::remove_file(format_options.native_format_context.as_ref().unwrap()).unwrap();
        let mut from_fm = *b"NOCALL  \0\0\0\0";
        image
            .patch_memory(&[
                (0x3ab82, &[255; 5000]),
                (0x3bf0a, &[255; 100]),
                (0x3c0a7, &[255; 600]),
                (0x3c33b, &[0]),
                (0x32985, &from_fm),
            ])
            .unwrap();
        node.key = 0x185;
        node.name = b"CL_TxHisFromFm_Freq".to_vec();
        assert_eq!(
            print_cell(
                &image,
                &current,
                &factory,
                &node,
                &history_runtime,
                &resources
            )
            .unwrap(),
            b""
        );

        let mut channel = record::reset_holder(3, 0, 0);
        channel.set_channel_field(C::FrequencyHz, 145_000_000);
        channel.set_mode(13);
        channel.set_data(1);
        channel.set_filter(2);
        record_image::store_channel_into(
            &mut current,
            record_image::ChannelKind::Call,
            0,
            &mut channel,
            |_| Ok(()),
        )
        .unwrap();
        let call = Factory::parse(b"[St_ColCut_Split]GI_Sheet_CallCH,103").unwrap();
        node.key = 0x108;
        node.name = b"CL_Mch_Mode".to_vec();
        node.typed_index = Some(0);
        assert_eq!(
            print_cell(&image, &current, &call, &node, &runtime, &resources).unwrap(),
            b"actual-mode13"
        );
        let normal = Factory::parse(b"[St_ColCut_Split]GI_Sheet_MemoryCH_Split,2").unwrap();
        current[0x3fcd4..0x3fcd8].copy_from_slice(&0xffff_u32.to_le_bytes());
        assert_eq!(
            print_cell(&image, &current, &normal, &node, &runtime, &resources).unwrap(),
            b""
        );
        current[0x3c622..0x3c624].copy_from_slice(&[3, 0]);
        current[0x3c624..0x3c626].fill(0);
        node.key = 0x1bb;
        node.name = b"CL_Gps_Alarm".to_vec();
        assert!(print_cell(&image, &current, &gps, &node, &runtime, &resources).is_err());
        assert_eq!(
            print_cell(&image, &current, &gps, &node, &japanese, &resources).unwrap(),
            b"actual-alarm1"
        );
        assert_eq!(
            print_cell(&image, &current, &gps, &node, &other_language, &resources).unwrap(),
            b"other-alarm1"
        );
        current[0x3c624..0x3c626].copy_from_slice(&1_u16.to_le_bytes());
        assert_eq!(
            print_cell(&image, &current, &gps, &node, &japanese, &resources).unwrap(),
            b"actual-alarm0"
        );
        node.key = 0x185;
        node.name = b"CL_TxHisFromFm_Freq".to_vec();
        from_fm[8..].copy_from_slice(&145_500_000_u32.to_le_bytes());
        image.patch_memory(&[(0x32985, &from_fm)]).unwrap();
        assert_eq!(
            print_cell(
                &image,
                &current,
                &factory,
                &node,
                &formatted_history,
                &resources
            )
            .unwrap(),
            b"145.500000"
        );
        node.key = 0x186;
        node.name = b"CL_TxHisFromFm_Dup".to_vec();
        assert_eq!(
            print_cell(
                &image,
                &current,
                &factory,
                &node,
                &history_runtime,
                &resources
            )
            .unwrap(),
            b""
        );
        assert_eq!(
            resources
                .selector_caption(b"GI_Sheet_Sel_GPS_Alarm", 1, 2, Some(0), None)
                .unwrap(),
            b""
        );
        assert!(
            resources
                .selector_caption(b"GI_Sheet_Sel_Mch_Mode", 1, 13, None, None)
                .is_err()
        );
        assert!(
            resources
                .selector_caption(b"GI_Sheet_Sel_Mch_Mode", 7, 0x1_0000_0000, None, None)
                .is_err()
        );

        channel.set_tx_tone(0);
        channel.set_rx_tone(0);
        channel.set_channel_field(C::Dtcs, 0);
        channel.set_channel_field(C::Polarity, 2);
        channel.set_channel_field(C::Tone, 0);
        record_image::store_channel_into(
            &mut current,
            record_image::ChannelKind::Call,
            0,
            &mut channel,
            |_| Ok(()),
        )
        .unwrap();
        for (key, expected) in [
            (0x106, b"OFF2".as_slice()),
            (0x108, b"actual-mode13"),
            (0x109, b"ON2"),
            (0x10a, b"filter2"),
            (0x10b, b"dup0"),
            (0x10d, b"tone0"),
            (0x110, b"023"),
            (0x111, b"polarity2"),
            (0x112, b"---"),
            (0x113, b"---"),
            (0x117, b"selection0"),
        ] {
            node.key = key;
            assert_eq!(
                print_cell(&image, &current, &call, &node, &runtime, &resources).unwrap(),
                expected
            );
        }
        for key in [0x10e, 0x10f] {
            node.key = key;
            assert!(print_cell(&image, &current, &call, &node, &runtime, &resources).is_err());
            assert_eq!(
                print_cell(&image, &current, &call, &node, &history_runtime, &resources).unwrap(),
                b"67.0"
            );
        }
        channel.set_mode(16);
        channel.set_channel_field(C::DigitalSquelch, 0);
        channel.set_channel_field(C::DigitalCode, 7);
        record_image::store_channel_into(
            &mut current,
            record_image::ChannelKind::Call,
            0,
            &mut channel,
            |_| Ok(()),
        )
        .unwrap();
        node.key = 0x112;
        assert_eq!(
            print_cell(&image, &current, &call, &node, &runtime, &resources).unwrap(),
            b"dsql0"
        );
        node.key = 0x113;
        assert_eq!(
            print_cell(&image, &current, &call, &node, &runtime, &resources).unwrap(),
            b"07"
        );
        node.key = 0x10e;
        assert_eq!(
            print_cell(&image, &current, &call, &node, &runtime, &resources).unwrap(),
            b"---"
        );

        current[compact_at + 4] = 0x70;
        current[compact_at + 7] = 0;
        node.key = 0x10e;
        assert!(print_cell(&image, &current, &bandstack, &node, &runtime, &resources).is_err());
        assert_eq!(
            print_cell(
                &image,
                &current,
                &bandstack,
                &node,
                &history_runtime,
                &resources
            )
            .unwrap(),
            b"67.0"
        );
        current[compact_at + 4] = 0;
        assert_eq!(
            print_cell(&image, &current, &bandstack, &node, &runtime, &resources).unwrap(),
            b"---"
        );
        current[compact_at..compact_at + 4].fill(255);
        assert_eq!(
            print_cell(&image, &current, &bandstack, &node, &runtime, &resources).unwrap(),
            b""
        );

        let system_jp = crate::records::Options {
            system_langid: Some(0x411),
            ..Default::default()
        }
        .load(std::iter::empty::<&str>())
        .unwrap()
        .with_input_options(cs705::display::InputOptions {
            gui_language: Some(1),
            ..Default::default()
        });
        let system_other = crate::records::Options {
            system_langid: Some(0x409),
            ..Default::default()
        }
        .load(std::iter::empty::<&str>())
        .unwrap()
        .with_input_options(cs705::display::InputOptions {
            gui_language: Some(1),
            ..Default::default()
        });
        image
            .patch_memory(&[
                (0x3c8b6, &[7]),
                (0x3c622, &[2, 27]),
                (0x399ea, &[1, 0, 1, 0, 0, 0]),
                (0x399f3, &[0x30]),
                (0x399f7, &[0]),
            ])
            .unwrap();
        image.set_delta_f_span_raw(0);
        node.name = b"CL_Common_Value".to_vec();
        for (key, expected) in [
            (0x126, b"07".as_slice()),
            (0x1ca, b"group27"),
            (0x20c, b"RMC, GLL          "),
            (0x210, b"log2, log3\n"),
        ] {
            node.key = key;
            assert_eq!(
                print_cell(&image, &current, &factory, &node, &runtime, &resources).unwrap(),
                expected
            );
        }
        image.patch_memory(&[(0x399f3, &[0x40])]).unwrap();
        node.key = 0x210;
        assert!(print_cell(&image, &current, &factory, &node, &runtime, &resources).is_err());
        assert_eq!(
            print_cell(&image, &current, &factory, &node, &japanese, &resources).unwrap(),
            b"log1\n"
        );
        assert_eq!(
            print_cell(
                &image,
                &current,
                &factory,
                &node,
                &other_language,
                &resources
            )
            .unwrap(),
            b"other-log1\n"
        );
        image.patch_memory(&[(0x3c622, &[3, 0])]).unwrap();
        node.key = 0x1ca;
        assert!(print_cell(&image, &current, &factory, &node, &japanese, &resources).is_err());
        assert_eq!(
            print_cell(&image, &current, &factory, &node, &system_jp, &resources).unwrap(),
            b"bank0"
        );
        assert_eq!(
            print_cell(&image, &current, &factory, &node, &system_other, &resources).unwrap(),
            b"bank0-system"
        );
        for (key, jp, other) in [
            (0x1b0, b"picture-jpn0".as_slice(), b"picture0".as_slice()),
            (0x2a6, b"span-jpn0", b"span0"),
        ] {
            node.key = key;
            assert!(print_cell(&image, &current, &factory, &node, &japanese, &resources).is_err());
            assert_eq!(
                print_cell(&image, &current, &factory, &node, &system_jp, &resources).unwrap(),
                jp
            );
            assert_eq!(
                print_cell(&image, &current, &factory, &node, &system_other, &resources).unwrap(),
                other
            );
        }
        image.patch_memory(&[(0x399f7, &[255])]).unwrap();
        node.key = 0x1b0;
        assert!(print_cell(&image, &current, &factory, &node, &system_jp, &resources).is_err());
        image.set_delta_f_span_raw(7);
        node.key = 0x2a6;
        assert!(print_cell(&image, &current, &factory, &node, &system_jp, &resources).is_err());
        node.key = 0x229;
        for (raw, expected) in [
            (0x7fff_i16, b"--:--".as_slice()),
            (-90, b"-1:30"),
            (15360, b"0:00"),
        ] {
            image
                .patch_memory(&[(0x396ff, &raw.to_le_bytes())])
                .unwrap();
            assert_eq!(
                print_cell(&image, &current, &factory, &node, &runtime, &resources).unwrap(),
                expected
            );
        }
        let filter = Factory::parse(b"GI_Sheet_Filter,0").unwrap();
        node.index = 36;
        for (key, expected) in [
            (0x12f, b"-125Hz ".as_slice()),
            (0x130, b"-125Hz "),
            (0x131, b"---"),
            (0x132, b"0"),
        ] {
            node.key = key;
            assert_eq!(
                print_cell(&image, &current, &filter, &node, &runtime, &resources).unwrap(),
                expected
            );
        }
        node.key = 0x133;
        node.name = b"CL_My_Callsign_ChNo".to_vec();
        assert_eq!(
            print_cell(&image, &current, &filter, &node, &runtime, &resources).unwrap(),
            b"37"
        );

        let preset_factory = Factory::parse(b"[St_Preset]GI_Sheet_Preset,422").unwrap();
        node.key = 0x385;
        node.name = b"CL_Common_Preset_CheckBox".to_vec();
        node.index = 1;
        for mask in [0, 1, 255] {
            current[0xa6dc] = mask;
            assert_eq!(
                print_cell(
                    &image,
                    &current,
                    &preset_factory,
                    &node,
                    &runtime,
                    &resources
                )
                .unwrap(),
                b""
            );
        }
        node.index = 0;
        assert_eq!(
            print_cell(
                &image,
                &current,
                &preset_factory,
                &node,
                &runtime,
                &resources
            )
            .unwrap(),
            b"--"
        );
        current[0xa6dc..0xa6de].fill(0);
        current[0xa6de] = 255;
        current[0xa60f + 3] = 0;
        current[0xa60f + 15] = 255;
        current[0xa60f + 16] = 128;
        current[0xa60f + 18] = 0;
        current[0xa60f + 22] = 255;
        let before = (image.memory().to_vec(), current.clone());
        for (key, expected) in [
            (0x389, b"50Hz ".as_slice()),
            (0x38d, b"128 (50%)"),
            (0x38f, b"0 (0%)"),
            (0x390, b"255 (100%)"),
            (0x39a, b"FFh"),
        ] {
            node.key = key;
            node.name = b"CL_Common_Preset_Value".to_vec();
            assert_eq!(
                print_cell(
                    &image,
                    &current,
                    &preset_factory,
                    &node,
                    &runtime,
                    &resources
                )
                .unwrap(),
                expected
            );
        }
        for key in [
            0x1a3, 0x211, 0x231, 0x24e, 0x25c, 0x25d, 0x265, 0x277, 0x27b, 0x28a, 0x293, 0x2ab,
            0x2c6, 0x2ef, 0x2f2, 0x2f6, 0x2f7, 0x124, 0x261, 0x294, 0x295, 0x297, 0x29a, 0x2db,
            0x2e6, 0x346, 0x347, 0x348, 0x349, 0x2ad, 0x278, 0x2c8, 0x2ca, 0x2c2, 0x2c1, 0x2c5,
            0x2a9, 0x324, 0x325, 0x326,
        ] {
            node.key = key;
            let unit = matches!(key, 0x278 | 0x2c8).then_some(b"actual-sec".as_slice());
            assert_eq!(
                print_cell(&image, &current, &factory, &node, &system_jp, &resources).unwrap(),
                e::paint_number_at(&image, key as u16, 0, unit, Some(0x411)).unwrap()
            );
        }
        assert_eq!(image.memory(), before.0);
        assert_eq!(current, before.1);
    }
}
