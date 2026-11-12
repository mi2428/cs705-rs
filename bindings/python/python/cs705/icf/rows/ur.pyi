from enum import Enum
from collections.abc import Callable, Iterable
from cs705.csv.nls import Bytes
from cs705.icf.normalizer import ReferenceImage
from cs705.icf.settings.extended import FixedTextInput
from . import WorkingUr

class Field(Enum):
    Name = 0
    Call = 1
    def key(self) -> int: ...
class Frame:
    code: int
    command: int
    blocked: bool
    view_last: int
    view_enabled: int
    node_flags: int
    warning_bits: int
    notify_flags: int
    ur_changed: bool
    def __init__(self, code: int, command: int, blocked: bool, view_last: int, view_enabled: int, node_flags: int, warning_bits: int, notify_flags: int, ur_changed: bool) -> None: ...
class Request:
    index: int
    field: Field
    input: FixedTextInput
    alignment: int
    generation: ReferenceImage
    def __init__(self, index: int, field: Field, input: FixedTextInput, alignment: int, generation: ReferenceImage) -> None: ...
class Effect:
    stored: bool
    inserted: bool
    full_warning: bool
    blocked: bool
    view_bad: bool
    redirect: int | None
    state_out: int
    display: bytes | None
    def __init__(self, stored: bool, inserted: bool, full_warning: bool, blocked: bool, view_bad: bool, redirect: int | None, state_out: int, display: bytes | None, notifications: Iterable[int]) -> None: ...
    @staticmethod
    def default() -> Effect: ...
    def notifications_snapshot(self) -> list[int]: ...
    def set_notifications(self, value: Iterable[int]) -> None: ...
def native_text(model: WorkingUr, request: Request, frame: Frame, effect: Effect) -> None: ...
def native_text_with_nls(model: WorkingUr, request: Request, frame: Frame, effect: Effect, map: Callable[[bytes, Bytes], int], upper: Callable[[Bytes], int]) -> None: ...
