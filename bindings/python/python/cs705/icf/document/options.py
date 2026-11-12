"""Native clone-option calculations; no widget or dialog is delivered."""
from ..._native import icf as _icf

CloneOptions = _icf.document.options.CloneOptions
CloneOptionButton = _icf.document.options.CloneOptionButton

__all__ = ["CloneOptions", "CloneOptionButton"]
