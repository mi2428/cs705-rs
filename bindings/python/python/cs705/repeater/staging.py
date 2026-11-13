"""Decoded staging, commit callbacks and source identity."""
from .._native import repeater as _repeater

Record = _repeater.staging.Record
Staging = _repeater.staging.Staging
Commit = _repeater.staging.Commit
PreserveSource = _repeater.staging.PreserveSource
Records = _repeater.staging.Records
same = _repeater.staging.same
classify = _repeater.staging.classify
classify_bytes = _repeater.staging.classify_bytes
restored_reference = _repeater.staging.restored_reference
restore_use = _repeater.staging.restore_use
