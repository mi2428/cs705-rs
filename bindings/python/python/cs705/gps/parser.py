"""Lazy native GPS parsing and explicit finalization."""
from .._native import gps as _gps

Options = _gps.parser.Options
Rows = _gps.parser.Rows
Draft = _gps.parser.Draft
valid_coordinate = _gps.parser.valid_coordinate
