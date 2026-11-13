from collections.abc import Callable, Iterable
from enum import Enum
from .payload import Kind

class ImportKind(Enum):
    Memory = 0
    FixedPair = 1
    AlternatingPair = 2
    Gps = 3
    Repeater = 4
    Callsign = 5
    Scan = 6
    UserEdge = 7
    def required_labels(self) -> list[bytes]: ...
class Binding:
    def __init__(self, positions: Iterable[int | None], accepted: bool, separator: int) -> None: ...
    @staticmethod
    def new(header: bytes, separator: int, labels: Iterable[bytes], required: Iterable[int], lower: Callable[[bytes], bytes]) -> Binding: ...
    @staticmethod
    def ascii(header: bytes, separator: int, labels: Iterable[bytes], required: Iterable[int]) -> Binding: ...
    @staticmethod
    def builtin(header: bytes, separator: int, labels: Iterable[bytes], kind: ImportKind) -> Binding: ...
    def rebind(self, header: bytes, labels: Iterable[bytes], required: Iterable[int], lower: Callable[[bytes], bytes]) -> bool: ...
    positions: tuple[int | None, ...]
    accepted: bool
    separator: int
    def cell(self, line: bytes, field: int) -> bytes | None: ...
class Dispatch:
    def __init__(self, slot: int, export: Kind, import_kind: ImportKind | None, skip_invalid_export: bool) -> None: ...
    slot: int
    export: Kind
    import_kind: ImportKind | None
    skip_invalid_export: bool
def for_context(context_type: int) -> Dispatch: ...
def required_labels(kind: ImportKind) -> list[bytes]: ...
