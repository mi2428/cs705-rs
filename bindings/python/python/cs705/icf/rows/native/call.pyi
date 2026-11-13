from collections.abc import Callable
from .... import MutableBytes
from ...frequency import Ranges
from ...normalizer import ReferenceImage
from ...record import ChannelWrite
from . import Clipboard, Effects, Outcome, Confirmation

class Selection:
    active: int
    first: int
    last: int
    merged: bool
    show_both: bool
    def __init__(self, active: int, first: int, last: int, merged: bool, show_both: bool) -> None: ...

def copy(image: MutableBytes, selection: Selection, clipboard: Clipboard, effects: Effects) -> None: ...
def paste(image: MutableBytes, selection: Selection, clipboard: Clipboard, ranges: Ranges, effects: Effects, confirm: Callable[[Confirmation], int], before_store: Callable[[int, ChannelWrite], None]) -> Outcome: ...
def carry(image: MutableBytes, selection: Selection, direction: int, ranges: Ranges, effects: Effects, before_store: Callable[[int, ChannelWrite], None]) -> Outcome: ...
def reset_pair(image: MutableBytes, selection: Selection, generation: ReferenceImage, confirmation: int, effects: Effects, before_store: Callable[[int, ChannelWrite], None]) -> Outcome: ...
