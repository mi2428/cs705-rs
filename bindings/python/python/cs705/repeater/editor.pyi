from enum import Enum
from collections.abc import Callable, Iterable
from typing import TypeAlias
from cs705.icf import Icf
from cs705.icf.context import Context
from cs705.icf.frequency import Ranges
from cs705.icf.record import DecodedRecord
from cs705.icf.settings.extended import ChoiceInput, NumberInput, TextInput
from cs705.csv.nls import Bytes
from cs705.csv.number import CoordinateDialog, CoordinateEvent
from .working import Working
from .fields import Effects, TypeSource

Map: TypeAlias = Callable[[bytes, Bytes], int]
TYPE_SOURCE: TypeSource
class ChoiceField(Enum):
    Type = 0
    Mode = 1
    Duplex = 2
    Tone = 3
    Use = 4
    Position = 5
class TextField(Enum):
    Name = 0
    SubName = 1
    Call = 2
    Gateway = 3
class NumberField(Enum):
    Frequency = 0
    Offset = 1
    ToneFrequency = 2
    UtcOffset = 3
class Runtime:
    ranges: Ranges
    live_q: Context
    system_langid: int | None
    generation: bytes | None
    align_callsign: bool
    crt_decimal: int
    def __init__(self, ranges: Ranges, live_q: Context, system_langid: int | None, generation: bytes | None, align_callsign: bool, crt_decimal: int) -> None: ...
class Effect:
    def native_snapshot(self) -> Effects: ...
    def set_native(self, v: Effects) -> None: ...
    state_out: int
    popup_requested: bool
    blocked: bool
    gray: bool
    display: bytes
    source_error: str | None
    def notifications_snapshot(self) -> list[tuple[int, bool]]: ...
    def set_notifications(self, v: Iterable[tuple[int, bool]]) -> None: ...
    inserted: bool
    helper_accepted: bool
    candidate: int | None
    shared_input: bytes
    group_label: tuple[int, bytes] | None
    refresh_if_binding_matches: bool
    def coordinate_dialog_snapshot(self) -> CoordinateDialog | None: ...
    def set_coordinate_dialog(self, value: CoordinateDialog | None) -> None: ...
    def __init__(self) -> None: ...
class Nls:
    map: Map
    upper: Callable[[Bytes], int]
    def __init__(self, map: Map, upper: Callable[[Bytes], int]) -> None: ...
def group_name(image: Icf, group: int, input: TextInput, effect: Effect) -> None: ...
def begin_type(w: Working, holder: DecodedRecord, group: int, relative: int, r: Runtime, effect: Effect) -> None: ...
def choice(w: Working, holder: DecodedRecord, field: ChoiceField, input: ChoiceInput, r: Runtime, effect: Effect) -> None: ...
def number(w: Working, holder: DecodedRecord, field: NumberField, input: NumberInput, r: Runtime, effect: Effect, nls: Nls) -> None: ...
def text(w: Working, holder: DecodedRecord, field: TextField, input: TextInput, r: Runtime, effect: Effect, nls: Nls) -> None: ...
def coordinate(w: Working, holder: DecodedRecord, longitude: bool, input: TextInput, r: Runtime, effect: Effect) -> None: ...
def coordinate_input(w: Working, holder: DecodedRecord, longitude: bool, input: TextInput, events: Iterable[CoordinateEvent], r: Runtime, effect: Effect) -> None: ...
