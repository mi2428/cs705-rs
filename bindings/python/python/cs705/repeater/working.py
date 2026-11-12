"""Expanded repeater owner and raw codec operations."""
from .._native import repeater as _repeater

DefaultContext = _repeater.working.DefaultContext
UseRange = _repeater.working.UseRange
Working = _repeater.working.Working
expand_counts = _repeater.working.expand_counts
expand_rows = _repeater.working.expand_rows
rebuild_storage_into = _repeater.working.rebuild_storage_into
load_into = _repeater.working.load_into
save_into = _repeater.working.save_into
delete_into = _repeater.working.delete_into
