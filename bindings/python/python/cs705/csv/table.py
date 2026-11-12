"""Ordered native alias tables; duplicate entries remain meaningful."""
from .._native import csv as _csv

Entry = _csv.table.Entry
Corrections = _csv.table.Corrections
ToneCandidate = _csv.table.ToneCandidate
DTCS_CODES = _csv.table.DTCS_CODES
import_ascii = _csv.table.import_ascii
import_with = _csv.table.import_with
export = _csv.table.export
mode_ascii = _csv.table.mode_ascii
dtcs_ascii = _csv.table.dtcs_ascii
data_ascii = _csv.table.data_ascii
on_ascii = _csv.table.on_ascii
gps_group_ascii = _csv.table.gps_group_ascii
gps_group_text = _csv.table.gps_group_text
portable_label = _csv.table.portable_label
builtin_import_ascii = _csv.table.builtin_import_ascii
portable_import = _csv.table.portable_import
tone_builtin_ascii = _csv.table.tone_builtin_ascii
tone_ascii = _csv.table.tone_ascii
