"""Native GPS dataset operations."""
from .._native import gps as _gps

Record = _gps.operations.Record
Event = _gps.operations.Event
Outcome = _gps.operations.Outcome
first_free_physical = _gps.operations.first_free_physical
copy = _gps.operations.copy
paste = _gps.operations.paste
clear = _gps.operations.clear
rename_group_raw = _gps.operations.rename_group_raw
insert = _gps.operations.insert
insert_initialized = _gps.operations.insert_initialized
delete = _gps.operations.delete
carry = _gps.operations.carry
