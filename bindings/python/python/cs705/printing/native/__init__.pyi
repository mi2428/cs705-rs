from enum import Enum
from typing import Callable, Protocol
from ...display import Resources as Resources, ViewState
from .. import Grid, Editor
from . import menu as menu
class Class(Enum):
    Base = 0
    Gps = 1
    Common = 2
    Preset = 3
    ColCut = 4
    Simplex = 5
    Split = 6
    Satellite = 7
    RxHistory = 8
    Groups = 9
    Repeater = 10
    Ur = 11
    Allowed = 12
class Factory:
    class_: Class
    resource: str
    node: int
    group: int
    parameter: bytes
    def __init__(self, class_: Class, resource: str, node: int, group: int, parameter: bytes) -> None: ...
    @staticmethod
    def parse(target: bytes) -> Factory: ...
    def view(self, current: bytes, show_split: int) -> ViewState | None: ...
    def context_type(self) -> int: ...
class CellNode:
    group: int
    key: int
    index: int
    name: bytes
    flags1c: int
    flags1d: int
    comb: int
    above: int
    below: int
    merge: bool
    typed_index: int | None
    record_visible: bool
    literal: bytes
    alias: bytes | None
    checked: bool | None
    def __init__(self, group: int, key: int, index: int, name: bytes, flags1c: int, flags1d: int, comb: int, above: int, below: int, merge: bool, typed_index: int | None, record_visible: bool, literal: bytes, alias: bytes | None, checked: bool | None) -> None: ...
    @staticmethod
    def default() -> CellNode: ...
class InlineFrame:
    column: int
    row: int
    input_code: int
    layout: int
    warning_bits: int
    focus: int
    action: int
    command: int
    phase: int
    notify: int
    initial_flags: int
    grid4: bool
    grid8: bool
    callback: int
    text: bytes
    def __init__(self, column: int, row: int, node: CellNode, factory: Factory, input_code: int, layout: int, warning_bits: int, focus: int, action: int, command: int, phase: int, notify: int, initial_flags: int, grid4: bool, grid8: bool, callback: int, text: bytes) -> None: ...
    def node_snapshot(self) -> CellNode: ...
    def set_node(self, v: CellNode) -> None: ...
    def factory_snapshot(self) -> Factory: ...
    def set_factory(self, v: Factory) -> None: ...
    @staticmethod
    def commit(grid: Grid, column: int, row: int, text: bytes) -> InlineFrame: ...
class ReadSeek(Protocol):
    def read(self, size: int, /) -> bytes: ...
    def seek(self, offset: int, whence: int, /) -> int: ...
def read_source(reader: ReadSeek) -> bytes: ...
def byte_text(bytes: bytes) -> str: ...
def sync_editor(editor: Editor, grid: Grid, column: int, row: int, dispatch: Callable[[InlineFrame], None]) -> None: ...
