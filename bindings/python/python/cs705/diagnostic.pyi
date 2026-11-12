from enum import Enum
from .presentation import Language
class Domain(Enum):
    Icf = 0
    Csv = 1
    GpsCsv = 2
    RepeaterCsv = 3
    Clone = 4
    Presentation = 5
    Io = 6
class Kind(Enum):
    Invalid = 0
    Unsupported = 1
    Limit = 2
    Conversion = 3
    SourceUnavailable = 4
    Operation = 5
    ChecksumMismatch = 6
    RowIndex = 7
    UnsupportedCommand = 8
    AddressOverflow = 9
    SourceBounds = 10
    Rejected = 11
    UnsupportedLanguage = 12
    UnregisteredField = 13
    InvalidIndex = 14
    InvalidSymbolWidth = 15
    AmbiguousField = 16
class Diagnostic:
    domain: Domain
    kind: Kind
    line: int | None
    index: int | None
    value: int | None
    def technical_detail(self) -> str | None: ...
    def summary(self, language: Language) -> str: ...
