"""Typed special-sheet values and explicit native operations."""
from .._native import icf as _icf

FrequencyInput = _icf.special.FrequencyInput
Notification = _icf.special.Notification
Notifications = _icf.special.Notifications
Effects = _icf.special.Effects
PresetNumberEffect = _icf.special.PresetNumberEffect
PresetBandwidthEffect = _icf.special.PresetBandwidthEffect
RowState = _icf.special.RowState
ScanField = _icf.special.ScanField
ScanRowAction = _icf.special.ScanRowAction
PresetField = _icf.special.PresetField
PRESET_CATALOG = _icf.special.PRESET_CATALOG
EdgeSide = _icf.special.EdgeSide
scan_suitable_rows = _icf.special.scan_suitable_rows
ScanEdge = _icf.special.ScanEdge
UserEdge = _icf.special.UserEdge
Preset = _icf.special.Preset
CompactChannel = _icf.special.CompactChannel
CompactClipboard = _icf.special.CompactClipboard
ScanClipboard = _icf.special.ScanClipboard
PasteApproval = _icf.special.PasteApproval
DisplayValue = _icf.special.DisplayValue
CompactKind = _icf.special.CompactKind
CompactField = _icf.special.CompactField

def _offset(self, row):
    return _icf.special._compact_kind_offset(self, row)

def _displayed_rows(self):
    return _icf.special._compact_kind_displayed_rows(self)

def _key(self):
    return _icf.special._compact_field_key(self)

CompactKind.offset = _offset
CompactKind.displayed_rows = _displayed_rows
CompactField.key = _key
selected_scan_raw = _icf.special.selected_scan_raw
select_scan = _icf.special.select_scan
scan_reset_enabled = _icf.special.scan_reset_enabled
decode_bandwidth = _icf.special.decode_bandwidth
encode_bandwidth = _icf.special.encode_bandwidth
reset_compact = _icf.special.reset_compact
reset_preset = _icf.special.reset_preset
toggle_preset = _icf.special.toggle_preset
commit_preset = _icf.special.commit_preset
edit_scan_field = _icf.special.edit_scan_field
select_scan_raw = _icf.special.select_scan_raw
scan_rows = _icf.special.scan_rows
edit_scan_frequency = _icf.special.edit_scan_frequency
edit_user_frequency = _icf.special.edit_user_frequency
rename_preset = _icf.special.rename_preset
bandwidth_input = _icf.special.bandwidth_input
edit_preset_bandwidth = _icf.special.edit_preset_bandwidth
edit_preset_number = _icf.special.edit_preset_number
