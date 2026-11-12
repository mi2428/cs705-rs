"""Native normal-memory working maps and UR storage."""
from . import native, ur
from ..._native import icf as _icf

NormalMemory = _icf.rows.NormalMemory
WorkingUr = _icf.rows.WorkingUr
Allocation = _icf.rows.Allocation
ur_default = _icf.rows.ur_default
allocate_into = _icf.rows.allocate_into
reset_home_into = _icf.rows.reset_home_into
release_into = _icf.rows.release_into
delete_ur_into = _icf.rows.delete_ur_into
expand_map = _icf.rows.expand_map
expand_names = _icf.rows.expand_names
rebuild_metadata = _icf.rows.rebuild_metadata
expand_ur_count = _icf.rows.expand_ur_count
expand_ur_data = _icf.rows.expand_ur_data
rebuild_ur_metadata = _icf.rows.rebuild_ur_metadata
