from .fields import TypeSource
from .staging import Record

class OffsetProfile:
    # Rust's `class` field uses a Python-keyword-safe spelling.
    class_: int
    default_hz: int
    def __init__(self, class_: int, default_hz: int) -> None: ...
class Outcome:
    accepted: bool
    corrections: int
    def __init__(self, accepted: bool, corrections: int) -> None: ...
def utc_minutes(value: int, default: int) -> int: ...
def finalize(row: Record, initial: int, default: Record, offset: OffsetProfile, types: TypeSource) -> Outcome: ...
