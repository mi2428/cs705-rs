"""Native fixed call-pair operations; not normal-memory row operations."""
from ...._native import icf as _icf

Selection = _icf.rows.native.call.Selection
copy = _icf.rows.native.call.copy
paste = _icf.rows.native.call.paste
carry = _icf.rows.native.call.carry
reset_pair = _icf.rows.native.call.reset_pair
__all__ = ["Selection", "copy", "paste", "carry", "reset_pair"]
