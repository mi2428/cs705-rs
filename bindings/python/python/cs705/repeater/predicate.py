"""Native predicates over explicit image, B and Q inputs."""
from .._native import repeater as _repeater

callsign_bad = _repeater.predicate.callsign_bad
frequency_bad = _repeater.predicate.frequency_bad
duplex_bad = _repeater.predicate.duplex_bad
numeric_frequency_bad = _repeater.predicate.numeric_frequency_bad
row_bad = _repeater.predicate.row_bad
group_bad = _repeater.predicate.group_bad
all_groups_bad = _repeater.predicate.all_groups_bad
home_usable = _repeater.predicate.home_usable
home_usable_count = _repeater.predicate.home_usable_count
home_row_usable = _repeater.predicate.home_row_usable
