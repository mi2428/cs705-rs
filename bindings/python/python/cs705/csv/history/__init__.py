"""Native receive-history logical field formatting."""
from ..._native import csv as _csv
from . import paint

format = _csv.history.format
format_with_date = _csv.history.format_with_date
symbol = _csv.history.symbol
symbol_from_source = _csv.history.symbol_from_source
