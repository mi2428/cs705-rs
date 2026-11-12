from . import DecodedRecord
from .. import Icf
from ..context import Context
from ...csv.date_input import Clock
from ...gps.operations import Record

def holder(image: Icf, q: Context, system_langid: int | None, index: int, binding: int, clock: Clock) -> DecodedRecord: ...
def seed(image: Icf, q: Context, system_langid: int | None, clock: Clock) -> Record: ...
