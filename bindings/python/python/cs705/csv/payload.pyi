from collections.abc import Callable
from enum import Enum
from ..icf import Icf

class Kind(Enum):
    Memory = 0
    Scan = 1
    Gps = 2
    ReceiveHistory = 3
    FromHistory = 4
    FromDataHistory = 5
    ToHistory = 6
    Reflector = 7
    UserEdge = 8
    Repeater = 9
    Callsign = 10
class Adapter(Enum):
    Selection = 0
    Duplex = 1
    Mode = 2
    Data = 3
    Filter = 4
    Tone = 5
    ToneFrequency = 6
    Dtcs = 7
    DtcsPolarity = 8
    DigitalSquelch = 9
    Split = 10
    Group = 11
    Alarm = 12
    RepeaterDuplex = 13
    RepeaterMode = 14
    RepeaterUse = 15
    Position = 16
    UtcOffsetSentinel = 17
    RxType = 18
    RxStatus = 19
    Symbol = 20
    Directivity = 21
    DataType = 22
    GpsDate = 23
    GpsTime = 24
    HistoryDate = 25
    Timestamp = 26
    Message = 27
    GpsMessage = 28
class Holder:
    def __init__(self, bytes: bytes) -> None: ...
class Context:
    def __init__(self, separator: int, decimal: int, row: int, number_base: int, group: int, group_name: bytes, alarm: bool, utc_offset_minutes: int, memory_group_columns: bool, memory_selection_column: bool, memory_pair: bool) -> None: ...
    separator: int
    decimal: int
    row: int
    number_base: int
    group: int
    group_name: bytes
    alarm: bool
    utc_offset_minutes: int
    memory_group_columns: bool
    memory_selection_column: bool
    memory_pair: bool
class Request:
    def __init__(self, adapter: Adapter, source: bytes, utc_offset_minutes: int) -> None: ...
    adapter: Adapter
    source: bytes
    utc_offset_minutes: int
def build(kind: Kind, h: Holder, c: Context, native: Callable[[Request], bytes]) -> bytes: ...
def build_portable(kind: Kind, h: Holder, c: Context, runtime: Callable[[Request], bytes]) -> bytes: ...
def callsign_row(image: Icf, index: int, c: Context) -> bytes: ...
def utc_text(minutes: int) -> bytes: ...
