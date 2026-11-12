from . import icf as icf
from . import presentation as presentation, diagnostic as diagnostic, item_help as item_help
from . import display as display
from . import printing as printing
from .icf import Icf as Icf
from . import csv as csv, gps as gps, repeater as repeater

__version__: str

class Error(Exception):
    domain: str
    kind: str
    diagnostic: diagnostic.Diagnostic
    native: icf.ErrorData | csv.ErrorData | gps.ErrorData | repeater.ErrorData | presentation.ErrorData | clone.ErrorData | icf.document.FileError | icf.document.CloneError

class IoErrorData:
    """Explicit carrier of the original I/O payload; share() does not clone it."""
    @staticmethod
    def from_os_error(error: OSError) -> IoErrorData: ...
    def share(self) -> IoErrorData: ...
    @property
    def error_kind(self) -> str: ...
    @property
    def raw_os_error(self) -> int | None: ...
    def exception(self) -> BaseException: ...
    def __str__(self) -> str: ...

class MutableBytes:
    def __init__(self, value: bytes | bytearray) -> None: ...
    def __len__(self) -> int: ...
    def to_bytes(self) -> bytes: ...

class Bool:
    def __init__(self, value: bool) -> None: ...
    value: bool
from . import clone as clone
