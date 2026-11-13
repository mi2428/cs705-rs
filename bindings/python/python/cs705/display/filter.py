"""Readonly Filter FIX text using an explicit source asset when required."""
from .._native import display as _d
paint = _d.filter.paint
dd_rows_visible = _d.filter.dd_rows_visible
__all__ = ["paint", "dd_rows_visible"]
