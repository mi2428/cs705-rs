from . import digital as digital, extended as extended, gps as gps, network as network
from enum import Enum
from typing import ClassVar
from .. import Icf

class Kind:
    Selector: ClassVar[Kind]
    @staticmethod
    def Integer(unit: str) -> Kind: ...
    @property
    def kind(self) -> str: ...
    @property
    def unit(self) -> str | None: ...

class Setting:
    name: str
    feature_id: str
    key: int
    selection_key: int
    domain: Domain
    count: int
    kind: Kind
    def __init__(self, template: Setting, name: str, feature_id: str, key: int, selection_key: int, domain: Domain, count: int, kind: Kind) -> None: ...
    def read(self, image: Icf, index: int) -> Value: ...
    def read_numeric_raw(self, image: Icf, index: int) -> int: ...
    def store_numeric_raw(self, image: Icf, index: int, raw: int) -> None: ...
    def write(self, image: Icf, index: int, value: Value) -> None: ...
    def select(self, image: Icf, index: int, selection: Selection) -> Effect: ...
    def cycle(self, image: Icf, index: int, step: Step) -> Effect: ...
    def choices(self, image: Icf, index: int) -> list[int]: ...
    def control_state(self, image: Icf, index: int, marker_flag: bool) -> ControlState: ...

def find(name: str) -> Setting: ...
def catalog() -> list[Setting]: ...

class Domain:
    min: int
    max: int
    step: int
    def __init__(self, min: int, max: int, step: int) -> None: ...

class Effect:
    stored: bool
    notify_flags: int
    def __init__(self, stored: bool, notify_flags: int) -> None: ...

class ControlState:
    helper_enabled: bool
    gray: bool
    marked: bool
    def __init__(self, helper_enabled: bool, gray: bool, marked: bool) -> None: ...

class Value:
    @staticmethod
    def Number(value: int) -> Value: ...
    @property
    def kind(self) -> str: ...
    @property
    def number(self) -> int: ...

class Step(Enum):
    Next = 0
    Previous = 1

class Selection:
    key: int
    phase: int
    candidate: int
    accepted: bool
    def __init__(self, key: int, phase: int, candidate: int, accepted: bool) -> None: ...
