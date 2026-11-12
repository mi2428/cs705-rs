//! Native ICF bindings. The owning image wrapper is shared by all domains.
pub mod callsign_dialog;
#[cfg(test)]
mod checks;
pub mod context;
mod direct;
pub mod document;
pub mod domain_values;
pub mod format;
pub mod frequency;
pub mod normalizer;
pub mod record;
pub mod record_image;
pub mod rows;
pub mod settings;
pub mod special;
pub mod stage9;
pub mod text;
mod values;

use cs705_core::icf::*;
use pyo3::prelude::*;
use pyo3::types::PyDict;

#[pyclass(module = "cs705.icf", frozen)]
pub struct Icf {
    state: IcfState,
}

enum IcfState {
    Owned(crate::owner::Owner<cs705_core::icf::Icf<'static>>),
    Scoped(crate::scoped::IcfLoan),
}

impl Icf {
    pub fn take(&self) -> PyResult<crate::owner::Taken<'_, cs705_core::icf::Icf<'static>>> {
        match &self.state {
            IcfState::Owned(owner) => owner.take(),
            IcfState::Scoped(_) => Err(pyo3::exceptions::PyRuntimeError::new_err(
                "this operation requires an ordinary Icf owner, not a scoped view",
            )),
        }
    }
    pub fn from_core(value: cs705_core::icf::Icf<'static>) -> Self {
        Self {
            state: IcfState::Owned(crate::owner::Owner::new(value)),
        }
    }

    pub fn from_loan(loan: crate::scoped::IcfLoan) -> Self {
        Self {
            state: IcfState::Scoped(loan),
        }
    }

    pub fn with_ref<R>(
        &self,
        f: impl for<'data> FnOnce(&cs705_core::icf::Icf<'data>) -> PyResult<R>,
    ) -> PyResult<R> {
        match &self.state {
            IcfState::Owned(owner) => owner.with_ref(f),
            IcfState::Scoped(loan) => Python::attach(|py| loan.with_ref(py, f)),
        }
    }

    pub fn with_mut<R>(
        &self,
        f: impl for<'data> FnOnce(&mut cs705_core::icf::Icf<'data>) -> PyResult<R>,
    ) -> PyResult<R> {
        match &self.state {
            IcfState::Owned(owner) => owner.with_mut(f),
            IcfState::Scoped(loan) => Python::attach(|py| loan.with_mut(py, f)),
        }
    }
}

direct::direct_methods! {
Icf
existing {
    #[staticmethod]
    fn parse(source: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(cs705_core::icf::Icf::parse(crate::conversion::strict_bytes(source)?).map_err(crate::errors::icf)?.into_owned()))
    }

    #[staticmethod]
    fn from_memory(memory: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(cs705_core::icf::Icf::from_memory(crate::conversion::strict_bytes(memory)?.to_vec()).map_err(crate::errors::icf)?))
    }

    fn memory<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
        self.with_ref(|value| Ok(pyo3::types::PyBytes::new(py, value.memory())))
    }

    fn original_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
        self.with_ref(|value| Ok(pyo3::types::PyBytes::new(py, value.original_bytes())))
    }

    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
        self.with_ref(|value| Ok(pyo3::types::PyBytes::new(py, &value.to_bytes())))
    }

    fn comment_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
        self.with_ref(|value| Ok(pyo3::types::PyBytes::new(py, value.comment_bytes())))
    }

    fn extra_data(&self) -> PyResult<u32> {
        self.with_ref(|value| Ok(value.extra_data()))
    }

    fn into_owned(&self) -> PyResult<Self> {
        match &self.state {
            IcfState::Owned(owner) => Ok(Self::from_core(owner.take()?.consume().into_owned())),
            IcfState::Scoped(loan) => Python::attach(|py| loan.with_ref(py, |_| Err(pyo3::exceptions::PyRuntimeError::new_err("a scoped Icf cannot be consumed")))),
        }
    }

    fn delete_gps_rows(&self, rows: Vec<(Bound<'_, PyAny>, Bound<'_, PyAny>)>) -> PyResult<()> {
        let rows = rows.iter().map(|(group,index)| Ok((crate::conversion::strict_usize(group)?, crate::conversion::strict_usize(index)?))).collect::<PyResult<Vec<_>>>()?;
        self.with_mut(|core| core.delete_gps_rows(&rows).map_err(crate::errors::icf))
    }

    fn patch_memory(&self, patches: Vec<(Bound<'_, PyAny>, Bound<'_, PyAny>)>) -> PyResult<()> {
        let patches = patches.iter().map(|(at,bytes)| Ok((crate::conversion::strict_usize(at)?, crate::conversion::strict_bytes(bytes)?))).collect::<PyResult<Vec<_>>>()?;
        self.with_mut(|core| core.patch_memory(&patches).map_err(crate::errors::icf))
    }

    fn set_physical_repeater(&self, py: Python<'_>, slot: &Bound<'_, PyAny>, value: &domain_values::RepeaterRecord) -> PyResult<()> {
        let slot = crate::conversion::strict_usize(slot)?; let value = value.to_core(py)?;
        self.with_mut(|core| core.set_physical_repeater(slot, &value).map_err(crate::errors::icf))
    }

    #[pyo3(signature = (slot, input))]
    fn edit_physical_memory_name(&self, slot: &Bound<'_, PyAny>, input: Option<&crate::conversion::MutableBytes>) -> PyResult<bool> {
        let slot = crate::conversion::strict_usize(slot)?;
        let mut input=input.map(crate::conversion::MutableBytes::take).transpose()?;
        self.with_mut(|core| core.edit_physical_memory_name(slot, input.as_mut().map(|value| value.as_mut_slice())).map_err(crate::errors::icf))
    }
}
methods {
    delta_f_span_raw() [with_ref, plain];
    set_delta_f_span_raw(candidate: i32) [with_mut, plain];
    scope_mode_raw() [with_ref, plain];
    set_scope_mode_raw(candidate: i32) [with_mut, plain];
    agc(mode: StepAgcMode) [with_ref, fallible];
    agc_delay_code(mode: StepAgcMode, state: Agc) [with_ref, fallible];
    air_tuning_step_code() [with_ref, fallible];
    altitude_distance_unit() [with_ref, fallible];
    am_manual_notch_width() [with_ref, fallible];
    am_notch_key_function() [with_ref, fallible];
    anti_vox_gain_raw() [with_ref, fallible];
    attenuator(band: usize) [with_ref, fallible];
    attenuator_raw(band: usize) [with_ref, fallible];
    audio_fft_attenuator() [with_ref, fallible];
    audio_fft_waterfall_display() [with_ref, fallible];
    audio_fft_waveform() [with_ref, fallible];
    audio_fft_waveform_color() [with_ref, fallible];
    audio_osc_level() [with_ref, fallible];
    audio_osc_sweep_time() [with_ref, fallible];
    audio_osc_waveform_color() [with_ref, fallible];
    auto_power_off() [with_ref, fallible];
    auto_tune_rx_callsign_key_function() [with_ref, fallible];
    band_edge_beep() [with_ref, fallible];
    battery_pack_selection_display() [with_ref, fallible];
    battery_power_limit() [with_ref, fallible];
    battery_screen_saver() [with_ref, fallible];
    beep_level_limit() [with_ref, fallible];
    beep_level_raw() [with_ref, fallible];
    bluetooth() [with_ref, fallible];
    bluetooth_audio_output() [with_ref, fallible];
    bluetooth_auto_connect() [with_ref, fallible];
    bluetooth_auto_disconnect() [with_ref, fallible];
    bluetooth_custom_key_beep() [with_ref, fallible];
    bluetooth_headset_function() [with_ref, fallible];
    bluetooth_one_touch_ptt() [with_ref, fallible];
    bluetooth_power_save() [with_ref, fallible];
    bluetooth_ptt_beep() [with_ref, fallible];
    bluetooth_serial_function() [with_ref, fallible];
    canonicalize_memory_layout() [with_mut, fallible];
    carry_gps_rows(group: usize, first: usize, last: usize, delta: i8) [with_mut, fallible];
    charging_when_power_on() [with_ref, fallible];
    clone_comment_raw() [with_ref, plain];
    compressor_off_transmit_bandwidth() [with_ref, fallible];
    compressor_on_transmit_bandwidth() [with_ref, fallible];
    coordinate_display() [with_ref, fallible];
    cw_bfo_side() [with_ref, fallible];
    cw_break_in() [with_ref, fallible];
    cw_break_in_delay_tenths() [with_ref, fallible];
    cw_contest_number() [with_ref, fallible];
    cw_count_up_trigger_channel() [with_ref, fallible];
    cw_dot_dash_ratio_tenths() [with_ref, fallible];
    cw_key_type() [with_ref, fallible];
    cw_manual_notch_width() [with_ref, fallible];
    cw_mic_up_down_keyer() [with_ref, fallible];
    cw_number_style() [with_ref, fallible];
    cw_paddle_polarity() [with_ref, fallible];
    cw_pitch_hz() [with_ref, fallible];
    cw_repeat_seconds() [with_ref, fallible];
    cw_rise_time() [with_ref, fallible];
    cw_side_tone_level_raw() [with_ref, fallible];
    cw_side_tone_limiter() [with_ref, fallible];
    cw_speed_raw() [with_ref, fallible];
    delta_f_span() [with_ref, fallible];
    dial_auto_ts() [with_ref, fallible];
    display_language() [with_ref, fallible];
    dtmf_memory_raw(channel: usize) [with_ref, fallible];
    dtmf_speed() [with_ref, fallible];
    external_power_limit() [with_ref, fallible];
    external_power_screen_saver() [with_ref, fallible];
    filter_bandwidth_hz(mode: FilterMode, slot: FilterSlot) [with_ref, fallible];
    filter_passband_hz(mode: FilterMode, slot: FilterSlot) [with_ref, fallible];
    filter_pbt_hz(mode: FilterMode, slot: FilterSlot, side: PbtSide) [with_ref, fallible];
    filter_shape(band: FilterBandGroup, slot: FilterShapeSlot) [with_ref, fallible];
    filter_shift_hz(mode: FilterMode, slot: FilterSlot) [with_ref, fallible];
    gps_time_auto_correction() [with_ref, fallible];
    home_channel_beep() [with_ref, fallible];
    home_frequency_hz() [with_ref, fallible];
    key_touch_beep() [with_ref, fallible];
    keyboard_input() [with_ref, fallible];
    keyboard_layout() [with_ref, fallible];
    keyer_memory_raw(channel: usize) [with_ref, fallible];
    lcd_backlight_auto_adjust() [with_ref, fallible];
    lcd_backlight_raw() [with_ref, fallible];
    lock_function() [with_ref, fallible];
    log_csv_date_format() [with_ref, fallible];
    log_csv_separator_decimal() [with_ref, fallible];
    manual_notch_position() [with_ref, fallible];
    memory_channel_selection_scope() [with_ref, fallible];
    memory_group_display() [with_ref, fallible];
    memory_group_name_popup() [with_ref, fallible];
    memory_name_display() [with_ref, fallible];
    memory_pad_channels() [with_ref, fallible];
    meter_peak_hold() [with_ref, fallible];
    mic_up_down_speech() [with_ref, fallible];
    mic_up_down_speed() [with_ref, fallible];
    microphone_gain_raw() [with_ref, fallible];
    mode_speech() [with_ref, fallible];
    monitor() [with_ref, fallible];
    monitor_level_raw() [with_ref, fallible];
    noise_blanker() [with_ref, fallible];
    noise_blanker_depth() [with_ref, fallible];
    noise_blanker_level_raw() [with_ref, fallible];
    noise_blanker_width_raw() [with_ref, fallible];
    noise_reduction() [with_ref, fallible];
    noise_reduction_level() [with_ref, fallible];
    notch_filter() [with_ref, fallible];
    ntp() [with_ref, fallible];
    ntp_server_raw() [with_ref, plain];
    one_hz_mode() [with_ref, fallible];
    opening_message() [with_ref, fallible];
    physical_gps_name_raw(p: usize) [with_ref, fallible];
    physical_gps_record_raw(p: usize) [with_ref, fallible];
    physical_memory_data(slot: usize, side: MemorySide) [with_ref, fallible];
    physical_memory_digital_code(slot: usize, side: MemorySide) [with_ref, fallible];
    physical_memory_digital_squelch(slot: usize, side: MemorySide) [with_ref, fallible];
    physical_memory_dtcs_index(slot: usize, side: MemorySide) [with_ref, fallible];
    physical_memory_duplex(slot: usize, side: MemorySide) [with_ref, fallible];
    physical_memory_filter(slot: usize, side: MemorySide) [with_ref, fallible];
    physical_memory_frequency_hz(slot: usize, side: MemorySide) [with_ref, fallible];
    physical_memory_mode(slot: usize, side: MemorySide) [with_ref, fallible];
    physical_memory_name_raw(slot: usize) [with_ref, fallible];
    physical_memory_offset_hz(slot: usize, side: MemorySide) [with_ref, fallible];
    physical_memory_rx_tone_index(slot: usize, side: MemorySide) [with_ref, fallible];
    physical_memory_selection(slot: usize) [with_ref, fallible];
    physical_memory_split(slot: usize) [with_ref, fallible];
    physical_memory_tone(slot: usize, side: MemorySide) [with_ref, fallible];
    physical_memory_tx_tone_index(slot: usize, side: MemorySide) [with_ref, fallible];
    physical_repeater_record_raw(slot: usize) [with_ref, fallible];
    physical_repeater_use(slot: usize) [with_ref, fallible];
    playback_skip_time() [with_ref, fallible];
    power_key_screen_off() [with_ref, fallible];
    power_on_check() [with_ref, fallible];
    power_save() [with_ref, fallible];
    pre_recording_time() [with_ref, fallible];
    preamp(band: usize) [with_ref, fallible];
    preamp_raw(band: usize) [with_ref, fallible];
    pressure_unit() [with_ref, fallible];
    ptt_auto_record() [with_ref, fallible];
    ptt_lock() [with_ref, fallible];
    ptt_tune() [with_ref, fallible];
    qso_log() [with_ref, fallible];
    quarter_tuning() [with_ref, fallible];
    quick_split() [with_ref, fallible];
    rainfall_unit() [with_ref, fallible];
    raw_memory_group_name(descriptor: usize) [with_ref, fallible];
    receive_bass(mode: ToneMode) [with_ref, fallible];
    receive_high_pass_code(mode: ToneMode) [with_ref, fallible];
    receive_history_log() [with_ref, fallible];
    receive_led() [with_ref, fallible];
    receive_low_pass_code(mode: ToneMode) [with_ref, fallible];
    receive_recording_condition() [with_ref, fallible];
    receive_treble(mode: ToneMode) [with_ref, fallible];
    received_callsign_speech() [with_ref, fallible];
    recording_file_split() [with_ref, fallible];
    recording_microphone() [with_ref, fallible];
    recording_microphone_gain_raw() [with_ref, fallible];
    reference_frequency_adjustment() [with_ref, fallible];
    remote_power_off_mode() [with_ref, fallible];
    repeater_csv_date_format() [with_ref, fallible];
    repeater_csv_separator_decimal() [with_ref, fallible];
    rtty_decode_log() [with_ref, fallible];
    rtty_decode_usos() [with_ref, fallible];
    rtty_fft_averaging() [with_ref, fallible];
    rtty_key_polarity() [with_ref, fallible];
    rtty_linefeed() [with_ref, fallible];
    rtty_log_format() [with_ref, fallible];
    rtty_log_frequency() [with_ref, fallible];
    rtty_log_time() [with_ref, fallible];
    rtty_log_timestamp() [with_ref, fallible];
    rtty_manual_notch_width() [with_ref, fallible];
    rtty_mark() [with_ref, fallible];
    rtty_memory_raw(channel: usize) [with_ref, fallible];
    rtty_receive_font_color_raw() [with_ref, fallible];
    rtty_shift() [with_ref, fallible];
    rtty_threshold() [with_ref, fallible];
    rtty_transmit_font_color_raw() [with_ref, fallible];
    rtty_transmit_usos() [with_ref, fallible];
    rtty_twin_peak() [with_ref, fallible];
    rtty_waveform_color_raw() [with_ref, fallible];
    rx_cs_speech() [with_ref, fallible];
    s_meter_level_speech() [with_ref, fallible];
    scan_main_dial() [with_ref, fallible];
    scan_pause_time() [with_ref, fallible];
    scan_restart_time() [with_ref, fallible];
    scan_resume() [with_ref, fallible];
    scan_speed() [with_ref, fallible];
    scope_averaging() [with_ref, fallible];
    scope_center_display() [with_ref, fallible];
    scope_during_tx_center() [with_ref, fallible];
    scope_marker_position() [with_ref, fallible];
    scope_max_hold() [with_ref, fallible];
    scope_mode() [with_ref, fallible];
    scope_reference_level_tenths_db(slot: usize) [with_ref, fallible];
    scope_span() [with_ref, fallible];
    scope_sweep_speed() [with_ref, fallible];
    scope_vbw() [with_ref, fallible];
    scope_waterfall_display() [with_ref, fallible];
    scope_waterfall_marker_autohide() [with_ref, fallible];
    scope_waterfall_peak_color_level() [with_ref, fallible];
    scope_waterfall_size_expand() [with_ref, fallible];
    scope_waterfall_speed() [with_ref, fallible];
    scope_waveform() [with_ref, fallible];
    scope_waveform_color_current() [with_ref, fallible];
    scope_waveform_color_line() [with_ref, fallible];
    scope_waveform_color_max_hold() [with_ref, fallible];
    screen_capture_file_type() [with_ref, fallible];
    screen_capture_power_key() [with_ref, fallible];
    scroll_speed() [with_ref, fallible];
    select_memory_scan() [with_ref, fallible];
    set_agc(mode: StepAgcMode, value: Agc) [with_mut, fallible];
    set_agc_delay_code(mode: StepAgcMode, state: Agc, value: u8) [with_mut, fallible];
    set_air_tuning_step_code(value: u8) [with_mut, fallible];
    set_altitude_distance_unit(value: AltitudeDistanceUnit) [with_mut, fallible];
    set_am_manual_notch_width(value: ManualNotchWidth) [with_mut, fallible];
    set_am_notch_key_function(value: NotchKeyFunction) [with_mut, fallible];
    set_anti_vox_gain_raw(value: u8) [with_mut, fallible];
    set_attenuator(band: usize, value: bool) [with_mut, fallible];
    set_attenuator_raw(band: usize, candidate: i32) [with_mut, fallible];
    set_audio_fft_attenuator(value: AudioFftAttenuator) [with_mut, fallible];
    set_audio_fft_waterfall_display(value: bool) [with_mut, fallible];
    set_audio_fft_waveform(value: AudioFftWaveform) [with_mut, fallible];
    set_audio_fft_waveform_color(value: [u8;3]) [with_mut, fallible];
    set_audio_osc_level(value: AudioOscLevel) [with_mut, fallible];
    set_audio_osc_sweep_time(value: AudioOscSweepTime) [with_mut, fallible];
    set_audio_osc_waveform_color(value: [u8;3]) [with_mut, fallible];
    set_auto_power_off(selection: AutoPowerOff) [with_mut, fallible];
    set_auto_tune_rx_callsign_key_function(value: AutoTuneRxCallsignKeyFunction) [with_mut, fallible];
    set_band_edge_beep(selection: BandEdgeBeep) [with_mut, fallible];
    set_battery_pack_selection_display(value: bool) [with_mut, fallible];
    set_battery_power_limit(value: BatteryPowerLimit) [with_mut, fallible];
    set_battery_screen_saver(value: ScreenSaverDelay) [with_mut, fallible];
    set_beep_level_limit(enabled: bool) [with_mut, fallible];
    set_beep_level_raw(value: u8) [with_mut, fallible];
    set_bluetooth(value: bool) [with_mut, fallible];
    set_bluetooth_audio_output(value: BluetoothAudioOutput) [with_mut, fallible];
    set_bluetooth_auto_connect(value: bool) [with_mut, fallible];
    set_bluetooth_auto_disconnect(value: BluetoothAutoDisconnect) [with_mut, fallible];
    set_bluetooth_custom_key_beep(value: bool) [with_mut, fallible];
    set_bluetooth_headset_function(value: BluetoothHeadsetFunction) [with_mut, fallible];
    set_bluetooth_one_touch_ptt(value: bool) [with_mut, fallible];
    set_bluetooth_power_save(value: bool) [with_mut, fallible];
    set_bluetooth_ptt_beep(value: bool) [with_mut, fallible];
    set_bluetooth_serial_function(value: BluetoothSerialFunction) [with_mut, fallible];
    set_charging_when_power_on(value: bool) [with_mut, fallible];
    set_clone_comment_canonical_ascii(value: &str) [with_mut, fallible];
    set_compressor_off_transmit_bandwidth(value: TransmitBandwidth) [with_mut, fallible];
    set_compressor_on_transmit_bandwidth(value: TransmitBandwidth) [with_mut, fallible];
    set_coordinate_display(value: CoordinateDisplay) [with_mut, fallible];
    set_cw_bfo_side(value: CwBfoSide) [with_mut, fallible];
    set_cw_break_in(value: CwBreakIn) [with_mut, fallible];
    set_cw_break_in_delay_tenths(value: u8) [with_mut, fallible];
    set_cw_contest_number(value: u16) [with_mut, fallible];
    set_cw_count_up_trigger_channel(value: u8) [with_mut, fallible];
    set_cw_dot_dash_ratio_tenths(value: u8) [with_mut, fallible];
    set_cw_key_type(value: CwKeyType) [with_mut, fallible];
    set_cw_manual_notch_width(value: ManualNotchWidth) [with_mut, fallible];
    set_cw_mic_up_down_keyer(value: CwMicKeyer) [with_mut, fallible];
    set_cw_number_style(value: CwNumberStyle) [with_mut, fallible];
    set_cw_paddle_polarity(value: KeyPolarity) [with_mut, fallible];
    set_cw_pitch_hz(value: u16) [with_mut, fallible];
    set_cw_repeat_seconds(value: u8) [with_mut, fallible];
    set_cw_rise_time(value: CwRiseTime) [with_mut, fallible];
    set_cw_side_tone_level_raw(value: u8) [with_mut, fallible];
    set_cw_side_tone_limiter(value: bool) [with_mut, fallible];
    set_cw_speed_raw(value: u8) [with_mut, fallible];
    set_delta_f_span(value: DeltaFSpan) [with_mut, fallible];
    set_dial_auto_ts(value: DialAutoTs) [with_mut, fallible];
    set_display_language(value: Language) [with_mut, fallible];
    set_dtmf_memory(channel: usize, value: &[u8]) [with_mut, fallible];
    set_dtmf_speed(value: DtmfSpeed) [with_mut, fallible];
    set_external_power_limit(value: ExternalPowerLimit) [with_mut, fallible];
    set_external_power_screen_saver(value: ScreenSaverDelay) [with_mut, fallible];
    set_filter_bandwidth_hz(mode: FilterMode, slot: FilterSlot, value: u32) [with_mut, fallible];
    set_filter_pbt_hz(mode: FilterMode, slot: FilterSlot, side: PbtSide, value: i32) [with_mut, fallible];
    set_filter_shape(band: FilterBandGroup, slot: FilterShapeSlot, value: FilterShape) [with_mut, fallible];
    set_gps_group_name_ascii(group: usize, value: &str) [with_mut, fallible];
    set_gps_time_auto_correction(value: bool) [with_mut, fallible];
    set_home_channel_beep(enabled: bool) [with_mut, fallible];
    set_home_frequency_hz(value: Option<u32>) [with_mut, fallible];
    set_key_touch_beep(enabled: bool) [with_mut, fallible];
    set_keyboard_input(value: KeyboardInput) [with_mut, fallible];
    set_keyboard_layout(value: KeyboardLayout) [with_mut, fallible];
    set_keyer_memory(channel: usize, value: &[u8]) [with_mut, fallible];
    set_lcd_backlight_auto_adjust(value: bool) [with_mut, fallible];
    set_lcd_backlight_raw(value: u8) [with_mut, fallible];
    set_lock_function(value: LockFunction) [with_mut, fallible];
    set_log_csv_date_format(value: CsvDateFormat) [with_mut, fallible];
    set_log_csv_separator_decimal(value: CsvSeparatorDecimal) [with_mut, fallible];
    set_manual_notch_position(value: i8) [with_mut, fallible];
    set_memory_channel_selection_scope(value: MemoryChannelSelectionScope) [with_mut, fallible];
    set_memory_group_display(value: MemoryGroupDisplay) [with_mut, fallible];
    set_memory_group_name_popup(value: bool) [with_mut, fallible];
    set_memory_name_display(value: bool) [with_mut, fallible];
    set_memory_pad_channels(value: MemoryPadChannels) [with_mut, fallible];
    set_meter_peak_hold(value: bool) [with_mut, fallible];
    set_mic_up_down_speech(enabled: bool) [with_mut, fallible];
    set_mic_up_down_speed(value: MicUpDownSpeed) [with_mut, fallible];
    set_microphone_gain_raw(value: u8) [with_mut, fallible];
    set_mode_speech(enabled: bool) [with_mut, fallible];
    set_monitor(value: bool) [with_mut, fallible];
    set_monitor_level_raw(value: u8) [with_mut, fallible];
    set_noise_blanker(value: bool) [with_mut, fallible];
    set_noise_blanker_depth(value: u8) [with_mut, fallible];
    set_noise_blanker_level_raw(value: u8) [with_mut, fallible];
    set_noise_blanker_width_raw(value: u8) [with_mut, fallible];
    set_noise_reduction(value: bool) [with_mut, fallible];
    set_noise_reduction_level(value: u8) [with_mut, fallible];
    set_notch_filter(value: NotchFilter) [with_mut, fallible];
    set_ntp(value: bool) [with_mut, fallible];
    set_ntp_server_ascii(value: &str) [with_mut, fallible];
    set_one_hz_mode(value: bool) [with_mut, fallible];
    set_opening_message(value: bool) [with_mut, fallible];
    set_physical_gps_name_ascii(p: usize, value: &str) [with_mut, fallible];
    set_physical_memory_digital_code(slot: usize, side: MemorySide, value: u8) [with_mut, fallible];
    set_physical_memory_digital_squelch(slot: usize, side: MemorySide, value: MemoryDigitalSquelch) [with_mut, fallible];
    set_physical_memory_dtcs_index(slot: usize, side: MemorySide, value: u8) [with_mut, fallible];
    set_physical_memory_duplex(slot: usize, side: MemorySide, value: MemoryDuplex) [with_mut, fallible];
    set_physical_memory_filter(slot: usize, side: MemorySide, value: MemoryFilter) [with_mut, fallible];
    set_physical_memory_frequency_hz(slot: usize, side: MemorySide, value: u32) [with_mut, fallible];
    set_physical_memory_name_ascii(slot: usize, name: &str) [with_mut, fallible];
    set_physical_memory_offset_hz(slot: usize, side: MemorySide, value: u32) [with_mut, fallible];
    set_physical_memory_rx_tone_index(slot: usize, side: MemorySide, value: u8) [with_mut, fallible];
    set_physical_memory_selection(slot: usize, value: MemorySelection) [with_mut, fallible];
    set_physical_memory_split(slot: usize, value: bool) [with_mut, fallible];
    set_physical_memory_tone(slot: usize, side: MemorySide, value: MemoryTone) [with_mut, fallible];
    set_physical_memory_tx_tone_index(slot: usize, side: MemorySide, value: u8) [with_mut, fallible];
    set_physical_repeater_frequency_hz(slot: usize, hz: u32) [with_mut, fallible];
    set_physical_repeater_use(slot: usize, value: bool) [with_mut, fallible];
    set_playback_skip_time(value: PlaybackSkipTime) [with_mut, fallible];
    set_power_key_screen_off(value: bool) [with_mut, fallible];
    set_power_on_check(value: bool) [with_mut, fallible];
    set_power_save(selection: PowerSave) [with_mut, fallible];
    set_pre_recording_time(value: PreRecordingTime) [with_mut, fallible];
    set_preamp(band: usize, value: Preamp) [with_mut, fallible];
    set_preamp_raw(band: usize, candidate: i32) [with_mut, fallible];
    set_pressure_unit(value: PressureUnit) [with_mut, fallible];
    set_ptt_auto_record(value: bool) [with_mut, fallible];
    set_ptt_lock(value: bool) [with_mut, fallible];
    set_ptt_tune(value: bool) [with_mut, fallible];
    set_qso_log(value: bool) [with_mut, fallible];
    set_quarter_tuning(value: bool) [with_mut, fallible];
    set_quick_split(enabled: bool) [with_mut, fallible];
    set_rainfall_unit(value: RainfallUnit) [with_mut, fallible];
    set_raw_memory_group_name_ascii(descriptor: usize, name: &str) [with_mut, fallible];
    set_receive_bass(mode: ToneMode, value: i8) [with_mut, fallible];
    set_receive_high_pass_code(mode: ToneMode, value: u8) [with_mut, fallible];
    set_receive_history_log(value: bool) [with_mut, fallible];
    set_receive_led(value: bool) [with_mut, fallible];
    set_receive_low_pass_code(mode: ToneMode, value: u8) [with_mut, fallible];
    set_receive_recording_condition(value: RecordingCondition) [with_mut, fallible];
    set_receive_treble(mode: ToneMode, value: i8) [with_mut, fallible];
    set_received_callsign_speech(selection: ReceivedCallsignSpeech) [with_mut, fallible];
    set_recording_file_split(value: bool) [with_mut, fallible];
    set_recording_microphone(value: RecordingMicrophone) [with_mut, fallible];
    set_recording_microphone_gain_raw(value: u8) [with_mut, fallible];
    set_reference_frequency_adjustment(value: u16) [with_mut, fallible];
    set_remote_power_off_mode(value: RemotePowerOffMode) [with_mut, fallible];
    set_repeater_csv_date_format(value: CsvDateFormat) [with_mut, fallible];
    set_repeater_csv_separator_decimal(value: CsvSeparatorDecimal) [with_mut, fallible];
    set_repeater_group_name_ascii(group: usize, value: &str) [with_mut, fallible];
    set_rtty_decode_log(value: bool) [with_mut, fallible];
    set_rtty_decode_usos(value: bool) [with_mut, fallible];
    set_rtty_fft_averaging(value: RttyFftAveraging) [with_mut, fallible];
    set_rtty_key_polarity(value: KeyPolarity) [with_mut, fallible];
    set_rtty_linefeed(value: RttyLinefeed) [with_mut, fallible];
    set_rtty_log_format(value: RttyLogFormat) [with_mut, fallible];
    set_rtty_log_frequency(value: bool) [with_mut, fallible];
    set_rtty_log_time(value: RttyLogTime) [with_mut, fallible];
    set_rtty_log_timestamp(value: bool) [with_mut, fallible];
    set_rtty_manual_notch_width(value: ManualNotchWidth) [with_mut, fallible];
    set_rtty_mark(value: RttyMark) [with_mut, fallible];
    set_rtty_memory(channel: usize, value: &[u8]) [with_mut, fallible];
    set_rtty_receive_font_color_raw(value: [u8;3]) [with_mut, fallible];
    set_rtty_shift(value: RttyShift) [with_mut, fallible];
    set_rtty_threshold(value: u8) [with_mut, fallible];
    set_rtty_transmit_font_color_raw(value: [u8;3]) [with_mut, fallible];
    set_rtty_transmit_usos(value: bool) [with_mut, fallible];
    set_rtty_twin_peak(value: bool) [with_mut, fallible];
    set_rtty_waveform_color_raw(value: [u8;3]) [with_mut, fallible];
    set_rx_cs_speech(enabled: bool) [with_mut, fallible];
    set_s_meter_level_speech(enabled: bool) [with_mut, fallible];
    set_scan_main_dial(value: ScanMainDial) [with_mut, fallible];
    set_scan_pause_time(value: ScanPauseTime) [with_mut, fallible];
    set_scan_restart_time(value: ScanRestartTime) [with_mut, fallible];
    set_scan_resume(value: bool) [with_mut, fallible];
    set_scan_speed(value: ScanSpeed) [with_mut, fallible];
    set_scope_averaging(value: ScopeAveraging) [with_mut, fallible];
    set_scope_center_display(value: ScopeCenterDisplay) [with_mut, fallible];
    set_scope_during_tx_center(value: bool) [with_mut, fallible];
    set_scope_marker_position(value: ScopeMarkerPosition) [with_mut, fallible];
    set_scope_max_hold(value: ScopeMaxHold) [with_mut, fallible];
    set_scope_mode(value: ScopeMode) [with_mut, fallible];
    set_scope_reference_level_tenths_db(slot: usize, value: i32) [with_mut, fallible];
    set_scope_span(value: ScopeSpan) [with_mut, fallible];
    set_scope_sweep_speed(value: ScopeSweepSpeed) [with_mut, fallible];
    set_scope_vbw(value: ScopeVbw) [with_mut, fallible];
    set_scope_waterfall_display(value: bool) [with_mut, fallible];
    set_scope_waterfall_marker_autohide(value: bool) [with_mut, fallible];
    set_scope_waterfall_peak_color_level(value: ScopeWaterfallPeak) [with_mut, fallible];
    set_scope_waterfall_size_expand(value: ScopeWaterfallSize) [with_mut, fallible];
    set_scope_waterfall_speed(value: ScopeWaterfallSpeed) [with_mut, fallible];
    set_scope_waveform(value: ScopeWaveform) [with_mut, fallible];
    set_scope_waveform_color_current(value: [u8;3]) [with_mut, fallible];
    set_scope_waveform_color_line(value: [u8;3]) [with_mut, fallible];
    set_scope_waveform_color_max_hold(value: [u8;3]) [with_mut, fallible];
    set_screen_capture_file_type(value: ScreenCaptureFileType) [with_mut, fallible];
    set_screen_capture_power_key(value: bool) [with_mut, fallible];
    set_scroll_speed(value: ScrollSpeed) [with_mut, fallible];
    set_select_memory_scan(value: SelectMemoryScan) [with_mut, fallible];
    set_speech_alphabet(selection: SpeechAlphabet) [with_mut, fallible];
    set_speech_compression_level(value: u8) [with_mut, fallible];
    set_speech_compressor(value: bool) [with_mut, fallible];
    set_speech_language(selection: SpeechLanguage) [with_mut, fallible];
    set_speech_level_raw(value: u8) [with_mut, fallible];
    set_speech_lock_key(value: SpeechLockKey) [with_mut, fallible];
    set_speech_speed(selection: SpeechSpeed) [with_mut, fallible];
    set_speed_unit(value: SpeedUnit) [with_mut, fallible];
    set_split_lock(enabled: bool) [with_mut, fallible];
    set_split_offset_khz(value: i16) [with_mut, fallible];
    set_ssb_cw_frequency_shift(value: bool) [with_mut, fallible];
    set_ssb_manual_notch_width(value: ManualNotchWidth) [with_mut, fallible];
    set_ssb_notch_key_function(value: NotchKeyFunction) [with_mut, fallible];
    set_swr_graph_bar_index(slot: usize, value: u8) [with_mut, fallible];
    set_swr_graph_step_index(slot: usize, value: u8) [with_mut, fallible];
    set_system_language(value: Language) [with_mut, fallible];
    set_temperature_unit(value: TemperatureUnit) [with_mut, fallible];
    set_temporary_skip_time(value: TemporarySkipTime) [with_mut, fallible];
    set_transmit_bandwidth_high(preset: TransmitBandwidthPreset, value: TransmitHigh) [with_mut, fallible];
    set_transmit_bandwidth_low(preset: TransmitBandwidthPreset, value: TransmitLow) [with_mut, fallible];
    set_transmit_bass(mode: ToneMode, value: i8) [with_mut, fallible];
    set_transmit_meter(value: TransmitMeter) [with_mut, fallible];
    set_transmit_recording_audio(value: TxRecordingAudio) [with_mut, fallible];
    set_transmit_timeout(value: TransmitTimeout) [with_mut, fallible];
    set_transmit_treble(mode: ToneMode, value: i8) [with_mut, fallible];
    set_tuner_memory(value: bool) [with_mut, fallible];
    set_tuner_selection(value: TunerSelection) [with_mut, fallible];
    set_tuning_step_code(mode: StepAgcMode, value: u8) [with_mut, fallible];
    set_usb_power_input(value: bool) [with_mut, fallible];
    set_utc_offset_minutes(value: i16) [with_mut, fallible];
    set_voice_auto_monitor(value: bool) [with_mut, fallible];
    set_voice_repeat_seconds(value: u8) [with_mut, fallible];
    set_voice_transmit_level_raw(value: u8) [with_mut, fallible];
    set_vox(value: bool) [with_mut, fallible];
    set_vox_break_in_key_function(value: VoxBreakInKeyFunction) [with_mut, fallible];
    set_vox_delay_deciseconds(value: u8) [with_mut, fallible];
    set_vox_gain_raw(value: u8) [with_mut, fallible];
    set_vox_voice_delay(value: VoxVoiceDelay) [with_mut, fallible];
    set_wind_speed_unit(value: WindSpeedUnit) [with_mut, fallible];
    speech_alphabet() [with_ref, fallible];
    speech_compression_level() [with_ref, fallible];
    speech_compressor() [with_ref, fallible];
    speech_language() [with_ref, fallible];
    speech_level_raw() [with_ref, fallible];
    speech_lock_key() [with_ref, fallible];
    speech_speed() [with_ref, fallible];
    speed_unit() [with_ref, fallible];
    split_lock() [with_ref, fallible];
    split_offset_khz() [with_ref, fallible];
    ssb_cw_frequency_shift() [with_ref, fallible];
    ssb_manual_notch_width() [with_ref, fallible];
    ssb_notch_key_function() [with_ref, fallible];
    swr_graph_bar_index(slot: usize) [with_ref, fallible];
    swr_graph_step_index(slot: usize) [with_ref, fallible];
    system_language() [with_ref, fallible];
    temperature_unit() [with_ref, fallible];
    temporary_skip_time() [with_ref, fallible];
    transmit_bandwidth_high(preset: TransmitBandwidthPreset) [with_ref, fallible];
    transmit_bandwidth_low(preset: TransmitBandwidthPreset) [with_ref, fallible];
    transmit_bass(mode: ToneMode) [with_ref, fallible];
    transmit_meter() [with_ref, fallible];
    transmit_recording_audio() [with_ref, fallible];
    transmit_timeout() [with_ref, fallible];
    transmit_treble(mode: ToneMode) [with_ref, fallible];
    tuner_memory() [with_ref, fallible];
    tuner_selection() [with_ref, fallible];
    tuning_step_code(mode: StepAgcMode) [with_ref, fallible];
    usb_power_input() [with_ref, fallible];
    utc_offset_minutes() [with_ref, fallible];
    voice_auto_monitor() [with_ref, fallible];
    voice_repeat_seconds() [with_ref, fallible];
    voice_transmit_level_raw() [with_ref, fallible];
    vox() [with_ref, fallible];
    vox_break_in_key_function() [with_ref, fallible];
    vox_delay_deciseconds() [with_ref, fallible];
    vox_gain_raw() [with_ref, fallible];
    vox_voice_delay() [with_ref, fallible];
    wind_speed_unit() [with_ref, fallible];
    physical_gps_date(p: usize) [with_ref, fallible];
    set_physical_gps_date(p: usize, value: GpsDate) [with_mut, fallible];
    physical_gps_time(p: usize) [with_ref, fallible];
    set_physical_gps_time(p: usize, value: GpsTime) [with_mut, fallible];
    physical_gps_latitude(p: usize) [with_ref, fallible];
    set_physical_gps_latitude(p: usize, value: GpsCoordinate) [with_mut, fallible];
    physical_gps_longitude(p: usize) [with_ref, fallible];
    set_physical_gps_longitude(p: usize, value: GpsCoordinate) [with_mut, fallible];
    physical_gps_altitude(p: usize) [with_ref, fallible];
    set_physical_gps_altitude(p: usize, value: GpsAltitude) [with_mut, fallible];
    physical_memory_callsign_raw(slot: usize, side: MemorySide, which: MemoryCallsign) [with_ref, fallible];
    set_physical_memory_callsign_raw(slot: usize, side: MemorySide, which: MemoryCallsign, value: [u8;8]) [with_mut, fallible];
    set_physical_gps_name_bytes(p: usize, value: &[u8;16]) [with_mut, fallible];
    set_physical_repeater_name_bytes(slot: usize, name: &[u8;16], subname: &[u8;8]) [with_mut, fallible];
    physical_memory_dtcs_polarity(slot: usize, side: MemorySide) [with_ref, fallible];
    set_physical_memory_dtcs_polarity(slot: usize, side: MemorySide, value: (bool,bool)) [with_mut, fallible];
    gps_alarm() [with_ref, fallible];
    set_gps_alarm(value: GpsAlarm) [with_mut, fallible];
    physical_repeater(slot: usize) [with_ref, fallible];
    memory_groups() [with_ref, fallible];
    gps_groups() [with_ref, fallible];
    repeater_groups() [with_ref, fallible];
    network_configuration() [with_ref, plain];
    edit_network_address(field: cs705_core::icf::settings::network::Field, input: Option<&[u8]>) [with_mut, plain];
    select_network_prefix(key: u16, phase: u32, candidate: i32, accepted: bool) [with_mut, plain];
}
}

pub(crate) fn add_enum(
    m: &Bound<'_, PyModule>,
    name: &str,
    variants: &[(&str, i64)],
) -> PyResult<()> {
    let py = m.py();
    let entries = PyDict::new(py);
    for (variant, value) in variants {
        entries.set_item(*variant, *value)?;
    }
    let kwargs = PyDict::new(py);
    let module_name = m.name()?;
    let leaf = module_name.to_str()?.rsplit('.').next().unwrap();
    kwargs.set_item(
        "module",
        if leaf == "icf" {
            "cs705.icf".to_owned()
        } else {
            format!("cs705.icf.{leaf}")
        },
    )?;
    let ty = py
        .import("enum")?
        .getattr("Enum")?
        .call((name, entries), Some(&kwargs))?;
    m.add(name, ty)
}

pub(crate) fn enum_value(value: &Bound<'_, PyAny>, module: &str, name: &str) -> PyResult<i64> {
    let ty = value.py().import(module)?.getattr(name)?;
    if !value.is_instance(&ty)? {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "expected {module}.{name}"
        )));
    }
    value.getattr("value")?.extract()
}

pub(crate) fn enum_result<'py>(
    py: Python<'py>,
    module: &str,
    name: &str,
    value: i64,
) -> PyResult<Bound<'py, PyAny>> {
    py.import(module)?.getattr(name)?.call1((value,))
}

#[pyfunction]
fn convert_map_rev5_to6<'py>(
    py: Python<'py>,
    source: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
    let value = cs705_core::icf::convert_map_rev5_to6(crate::conversion::strict_bytes(source)?)
        .map_err(crate::errors::icf)?;
    Ok(pyo3::types::PyBytes::new(py, &value))
}

pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "icf")?;
    m.add_class::<Icf>()?;
    m.add("Error", root.getattr("Error")?)?;
    m.add_function(wrap_pyfunction!(self::convert_map_rev5_to6, &m)?)?;
    values::register(&m)?;
    domain_values::register(&m)?;
    text::register(&m)?;
    format::register(&m)?;
    context::register(&m)?;
    frequency::register(&m)?;
    normalizer::register(&m)?;
    stage9::register(&m)?;
    record::register(&m)?;
    record_image::register(&m)?;
    settings::register(&m)?;
    rows::register(&m)?;
    special::register(&m)?;
    callsign_dialog::register(&m)?;
    document::register(&m)?;
    root.add_submodule(&m)
}
