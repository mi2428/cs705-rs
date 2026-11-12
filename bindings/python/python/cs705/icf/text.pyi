from enum import Enum
from .. import MutableBytes

class Status(Enum):
    Success = 0
    Model = 3
    FileRevision = 4
    MapRevision = 5
    Syntax = 6
    MissingCoverage = 7
    ExcessCoverage = 8
    ExtraData = 10

class Report:
    status: Status
    coverage: bytes
    metadata_before_data: tuple[int, int] | None
    records: int
    bytes_written: int
    source_comment: bytes
    def __init__(self, status: Status, coverage: bytes, metadata_before_data: tuple[int, int] | None, records: int, bytes_written: int, source_comment: bytes) -> None: ...

class Reader:
    model: int
    file_revision: int
    map_revision: int
    extra_data: int
    check_metadata: bool
    def __init__(self) -> None: ...
    def read(self, image: MutableBytes, source: bytes, required: bytes | None) -> Report: ...
    def read_fragments(self, image: MutableBytes, fragments: list[bytes | None], required: bytes | None) -> Report: ...

def coverage_status(observed: bytes | None, required: bytes | None) -> Status: ...
def extra_data_valid(word: int) -> bool: ...
def decimal_prefix(bytes: bytes) -> int: ...
def hexadecimal_prefix(bytes: bytes) -> int: ...
def fragments(source: bytes) -> list[bytes]: ...
