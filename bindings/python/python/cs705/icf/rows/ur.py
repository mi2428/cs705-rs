"""Native UR Name/Call runners over the canonical WorkingUr owner."""
from ..._native import icf as _icf

Field = _icf.rows.ur.Field

def _key(self):
    return _icf.rows.ur._field_key(self)

Field.key = _key

Frame = _icf.rows.ur.Frame
Request = _icf.rows.ur.Request
Effect = _icf.rows.ur.Effect
native_text = _icf.rows.ur.native_text
native_text_with_nls = _icf.rows.ur.native_text_with_nls
__all__ = ["Field", "Frame", "Request", "Effect", "native_text", "native_text_with_nls"]
