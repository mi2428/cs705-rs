"""Explicit initial-adoption requests and synchronous native boundaries."""
from ..._native import icf as _icf

InitialBoundary = _icf.document.initial.InitialBoundary
InitialRequest = _icf.document.initial.InitialRequest
InitialState = _icf.document.initial.InitialState
InitialContext = _icf.document.initial.InitialContext

__all__ = ["InitialBoundary", "InitialRequest", "InitialState", "InitialContext"]
