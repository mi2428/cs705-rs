"""Native numeric byte parsers; not host locale conversions."""
from ..._native import csv as _csv
from . import field, paint

CoordinateEvent = _csv.number.CoordinateEvent
CoordinateDialog = _csv.number.CoordinateDialog

decimal = _csv.number.decimal
int32 = _csv.number.int32
civ_address = _csv.number.civ_address
coordinate = _csv.number.coordinate
coordinate_text = _csv.number.coordinate_text
coordinate_native_phase2 = _csv.number.coordinate_native_phase2
area_native_mode = _csv.number.area_native_mode
area_native_phase2 = _csv.number.area_native_phase2
altitude = _csv.number.altitude
altitude_text = _csv.number.altitude_text
frequency_text = _csv.number.frequency_text
frequency_hz = _csv.number.frequency_hz
offset_hz = _csv.number.offset_hz
crt_decimal_prefix = _csv.number.crt_decimal_prefix
