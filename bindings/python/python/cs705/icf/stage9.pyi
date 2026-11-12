from .. import MutableBytes
from .context import Context
from .frequency import Ranges
from .normalizer import ReferenceImage, TypedCounts

class Runtime:
    def __init__(self, context: Context, ranges: Ranges, system_langid: int | None, previous_reload_guard: int, document_flags: bytes) -> None: ...
    context: Context
    ranges: Ranges
    system_langid: int | None
    previous_reload_guard: int
    document_flags: bytes

class Report:
    def __init__(self, typed: TypedCounts, deleted_repeaters: int) -> None: ...
    typed: TypedCounts
    deleted_repeaters: int

def normalize(image: MutableBytes, defaults: ReferenceImage, current: ReferenceImage, late: ReferenceImage, r: Runtime) -> Report: ...
