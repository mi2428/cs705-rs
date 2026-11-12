"""Explicit ANSI operations; callback buffer views expire on return."""
from .._native import csv as _csv

Bytes = _csv.nls.Bytes
Profile = _csv.nls.Profile
sanitize = _csv.nls.sanitize
name_candidate = _csv.nls.name_candidate
trim_candidate = _csv.nls.trim_candidate
fold_callsign = _csv.nls.fold_callsign
ascii_map = _csv.nls.ascii_map
platform_map = _csv.nls.platform_map
platform_uppercase = _csv.nls.platform_uppercase
map_copy_back = _csv.nls.map_copy_back
uppercase_copy = _csv.nls.uppercase_copy
callsign_fixed = _csv.nls.callsign_fixed
whole_text = _csv.nls.whole_text
callsign_correction_count = _csv.nls.callsign_correction_count
