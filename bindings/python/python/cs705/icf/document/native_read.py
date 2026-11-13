"""Native read phases yielding the real consuming clone engines."""
from ..._native import icf as _icf

CloneRead = _icf.document.native_read.CloneRead
CloneReadReply = _icf.document.native_read.CloneReadReply
CloneReadStep = _icf.document.native_read.CloneReadStep
CloneReadStop = _icf.document.native_read.CloneReadStop

__all__ = ["CloneRead", "CloneReadReply", "CloneReadStep", "CloneReadStop"]
