"""Decoded native holder and explicit codecs."""
from ..._native import icf as _icf
ChannelWrite = _icf.record.ChannelWrite
CallsignWrite = _icf.record.CallsignWrite
RepeaterWrite = _icf.record.RepeaterWrite
encode_callsign_into = _icf.record.encode_callsign_into
encode_channel_into = _icf.record.encode_channel_into
encode_repeater_into = _icf.record.encode_repeater_into
load_callsign_storage_into = _icf.record.load_callsign_storage_into
store_callsign_storage_into = _icf.record.store_callsign_storage_into

DecodedRecord = _icf.record.DecodedRecord
RecordBytes = _icf.record.RecordBytes
ChannelField = _icf.record.ChannelField
RepeaterField = _icf.record.RepeaterField
RepeaterText = _icf.record.RepeaterText
ChannelStage = _icf.record.ChannelStage
RepeaterStage = _icf.record.RepeaterStage
DECODED_BYTES = _icf.record.DECODED_BYTES
BODY_BYTES = _icf.record.BODY_BYTES
reset_holder = _icf.record.reset_holder
decode_mode = _icf.record.decode_mode
encode_mode = _icf.record.encode_mode
decode_tone = _icf.record.decode_tone
encode_tone = _icf.record.encode_tone
decode_digital_squelch = _icf.record.decode_digital_squelch
encode_digital_squelch = _icf.record.encode_digital_squelch
compact_values = _icf.record.compact_values
scan_values = _icf.record.scan_values
decode_compact_into = _icf.record.decode_compact_into
decode_scan_into = _icf.record.decode_scan_into
decode_channel_into = _icf.record.decode_channel_into
decode_gps_into = _icf.record.decode_gps_into
decode_callsign_into = _icf.record.decode_callsign_into
decode_pair_into = _icf.record.decode_pair_into
decode_mode_pair_into = _icf.record.decode_mode_pair_into
decode_repeater_into = _icf.record.decode_repeater_into
decode_history_base_into = _icf.record.decode_history_base_into
decode_receive_history_into = _icf.record.decode_receive_history_into
encode_gps_into = _icf.record.encode_gps_into
encode_pair_into = _icf.record.encode_pair_into
encode_mode_pair_into = _icf.record.encode_mode_pair_into
mode_filter_class = _icf.record.mode_filter_class
channel_mode_bad = _icf.record.channel_mode_bad

HistoryQuery = _icf.record.HistoryQuery
HistoryLoad = _icf.record.HistoryLoad
HistoryValue = _icf.record.HistoryValue
HistoryBytes = _icf.record.HistoryBytes
decode_history_into = _icf.record.decode_history_into
resolve_history_query = _icf.record.resolve_history_query
load_history_storage_into = _icf.record.load_history_storage_into
load_reflector_storage_into = _icf.record.load_reflector_storage_into
load_receive_history_storage_into = _icf.record.load_receive_history_storage_into
history_selection_present = _icf.record.history_selection_present
delete_history_storage = _icf.record.delete_history_storage
receive_history_raw = _icf.record.receive_history_raw
history_field_visible = _icf.record.history_field_visible
receive_history_fields = _icf.record.receive_history_fields
load_channel_image_into = _icf.record.load_channel_image_into
store_channel_image_into = _icf.record.store_channel_image_into
repeater_default_context = _icf.record.repeater_default_context
repeater_reset_word = _icf.record.repeater_reset_word
initial_normal_channel_into = _icf.record.initial_normal_channel_into

from . import gps_initial
