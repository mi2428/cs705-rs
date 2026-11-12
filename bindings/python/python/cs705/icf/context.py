"""Live Q context with explicit source adoption."""
from .._native import icf as _icf

Context = _icf.context.Context
__all__ = ["Context"]
