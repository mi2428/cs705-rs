"""Native information controllers with explicit resource observations."""
from .._native import clone as _clone

Life = _clone.information.Life
Event = _clone.information.Event
Cell = _clone.information.Cell
NativeReturn = _clone.information.NativeReturn
ReadMode = _clone.information.ReadMode
SearchSource = _clone.information.SearchSource
RegistrySource = _clone.information.RegistrySource
InformationSource = _clone.information.InformationSource
Controller = _clone.information.Controller
DocumentReceiver = _clone.information.DocumentReceiver
ReturnSource = _clone.information.ReturnSource

__all__ = ["Life", "Event", "Cell", "NativeReturn", "ReadMode", "SearchSource", "RegistrySource", "InformationSource", "Controller", "DocumentReceiver", "ReturnSource"]
