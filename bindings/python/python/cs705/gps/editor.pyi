from typing import ClassVar
from cs705.icf import Icf
from cs705.icf.record import DecodedRecord
from cs705.icf.settings.extended import ChoiceInput, ChoiceEffect, NumberInput, NumberEffect, TextInput

class RowAlarmChoice:
    Off: ClassVar[RowAlarmChoice]
    Memory: ClassVar[RowAlarmChoice]
    Receive: ClassVar[RowAlarmChoice]
    All: ClassVar[RowAlarmChoice]
    ALL: ClassVar[list[RowAlarmChoice]]
    @staticmethod
    def Group(group: int) -> RowAlarmChoice: ...
    def index(self) -> int: ...
    @property
    def variant(self) -> str: ...
    @property
    def group(self) -> int | None: ...
class AlarmView:
    group: int
    start: int
    active: int
    def __init__(self, group: int, start: int, active: int) -> None: ...
def load(image: Icf, absolute: int, binding: int) -> DecodedRecord: ...
def save(image: Icf, absolute: int, holder: DecodedRecord) -> bool: ...
class RowAlarmEffect:
    choice: ChoiceEffect
    marked: bool
    notify_39: bool
    def __init__(self, choice: ChoiceEffect, marked: bool, notify_39: bool) -> None: ...
class GroupNameEffect:
    stored: bool
    state_out: int
    display: bytes
    label: tuple[int, bytes] | None
    refresh_if_binding_matches: bool
    shared_input: bytes
    def __init__(self, stored: bool, state_out: int, display: bytes, label: tuple[int, bytes] | None, refresh_if_binding_matches: bool, shared_input: bytes) -> None: ...
def row_alarm(image: Icf, view: AlarmView, input: ChoiceInput) -> RowAlarmEffect: ...
def altitude(image: Icf, absolute: int, holder: DecodedRecord, input: NumberInput, point: int) -> NumberEffect: ...
def group_name(image: Icf, group: int, input: TextInput) -> GroupNameEffect: ...
