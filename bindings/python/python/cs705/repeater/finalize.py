"""Native correction and acceptance phase."""
from .._native import repeater as _repeater

OffsetProfile = _repeater.finalize.OffsetProfile
Outcome = _repeater.finalize.Outcome
utc_minutes = _repeater.finalize.utc_minutes
finalize = _repeater.finalize.finalize
