"""Native type-20 initializers using explicit Q, image, LANGID and clock."""
from ..._native import icf as _icf

holder = _icf.record.gps_initial.holder
seed = _icf.record.gps_initial.seed

__all__ = ["holder", "seed"]
