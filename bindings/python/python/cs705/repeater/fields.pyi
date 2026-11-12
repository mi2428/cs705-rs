from enum import Enum
from collections.abc import Callable
from cs705.icf.record import DecodedRecord
from .staging import Record

class Field(Enum):
    Mode = 0
    Duplex = 1
    Tone = 2
    ToneFrequency = 3
class Gate(Enum):
    Open = 0
    DisplayOnly = 1
    Placeholder = 2
class Outcome(Enum):
    Blocked = 0
    DisplayOnly = 1
    Skipped = 2
    Saved = 3
class TypeSource:
    placeholder: bytes
    gateway_marker: bytes
    def __init__(self, placeholder: bytes, gateway_marker: bytes) -> None: ...
class Selection:
    accepted: bool
    candidate: int
    def __init__(self, accepted: bool, candidate: int) -> None: ...
class Effects:
    changed: bool
    flags: int
    warning_flags: int
    save_requested: bool
    save_returned: bool
    def __init__(self) -> None: ...
class PaintMeta:
    gate: Gate
    gray: bool
    def __init__(self, gate: Gate, gray: bool) -> None: ...
    def placeholder(self) -> bytes | None: ...
def transform_type(row: Record, candidate: int, source: TypeSource) -> bool: ...
def candidate_allowed(row: Record, field: Field, candidate: int, current: int) -> bool: ...
def quantize_offset_hz(value: int) -> int: ...
def suppress_mode_code(value: int) -> int: ...
def paint_meta(holder: DecodedRecord, key: int) -> PaintMeta: ...
def select_type(row: Record, selection: Selection, source: TypeSource, effects: Effects, defaults: Callable[[], Record], save_reload: Callable[[Record], None]) -> None: ...
def select(row: Record, field: Field, selection: Selection, effects: Effects, save_reload: Callable[[Record], None], style: Callable[[Record], None]) -> Outcome: ...
def select_offset(row: Record, blocked: bool, native_result: int, candidate_hz: int, effects: Effects, style: Callable[[Record], None], save_reload: Callable[[Record], None]) -> Outcome: ...
def select_utc_offset(row: Record, native_result: int, candidate_minutes: int, effects: Effects, save_reload: Callable[[Record], None]) -> Outcome: ...
