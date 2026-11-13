"""Native scalar formatting profiles."""
from ..._native import csv as _csv

scalar = _csv.number.field.scalar
channel = _csv.number.field.channel
compact_offset = _csv.number.field.compact_offset
repeater_offset = _csv.number.field.repeater_offset
history = _csv.number.field.history
