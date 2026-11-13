"""Native classifications with separate lossless technical details."""
from ._native import diagnostic as _d
Diagnostic = _d.Diagnostic
Domain = _d.Domain
Kind = _d.Kind
__all__ = ["Diagnostic", "Domain", "Kind"]
