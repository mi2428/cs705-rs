"""In-process bindings to the cs705 Rust library.

MutableBytes owns a copy of its input; to_bytes() returns an independent snapshot.
Buffers stay bytes, with no implicit text decoding. Callback loans expire on
return; retain explicit snapshots instead. Native failures can preserve written
prefixes, so an exception does not imply rollback.
"""
from ._native import Bool, MutableBytes, Error, IoErrorData, __version__
from . import clone, icf, csv, gps, repeater, presentation, diagnostic, item_help, display, printing
from .icf import Icf

__all__ = ["Bool", "MutableBytes", "Error", "IoErrorData", "__version__", "clone", "csv", "gps", "repeater", "icf", "Icf", "presentation", "diagnostic", "item_help", "display", "printing"]
