"""Explicit publication and MRU protocols. Paths are raw bytes, not Unicode."""
from typing import TYPE_CHECKING, Protocol
from ..._native import icf as _icf

RecentFiles = _icf.document.lifecycle.RecentFiles
FormState = _icf.document.lifecycle.FormState
SaveMode = _icf.document.lifecycle.SaveMode
if TYPE_CHECKING:
    from .lifecycle import Placement, Prompt, SaveDialog, SaveSelection
else:
    SaveDialog = _icf.document.lifecycle.SaveDialog
    SaveSelection = _icf.document.lifecycle.SaveSelection
    Prompt = _icf.document.lifecycle.Prompt
    Placement = _icf.document.lifecycle.Placement
OpenFile = _icf.document.lifecycle.OpenFile
MenuQuery = _icf.document.lifecycle.MenuQuery
MenuContext = _icf.document.lifecycle.MenuContext
MenuDescriptor = _icf.document.lifecycle.MenuDescriptor
MenuItem = _icf.document.lifecycle.MenuItem
NativeView = _icf.document.lifecycle.NativeView
NativeState = _icf.document.lifecycle.NativeState
UiState = _icf.document.lifecycle.UiState
Lifecycle = _icf.document.lifecycle.Lifecycle
SaveAttempt = _icf.document.lifecycle.SaveAttempt
SaveFlow = _icf.document.lifecycle.SaveFlow
DirtyPrompt = _icf.document.lifecycle.DirtyPrompt
SaveOptions = _icf.document.lifecycle.SaveOptions
TreeState = _icf.document.lifecycle.TreeState
ViewState = _icf.document.lifecycle.ViewState
TitleState = _icf.document.lifecycle.TitleState
FormStateSnapshot = _icf.document.lifecycle.FormStateSnapshot
TreeStateSnapshot = _icf.document.lifecycle.TreeStateSnapshot
ViewStateSnapshot = _icf.document.lifecycle.ViewStateSnapshot
TitleStateSnapshot = _icf.document.lifecycle.TitleStateSnapshot
ValidationViewSnapshot = _icf.document.lifecycle.ValidationViewSnapshot
ValidationReportSnapshot = _icf.document.lifecycle.ValidationReportSnapshot
PlacementSnapshot = _icf.document.lifecycle.PlacementSnapshot
ContextFieldsSnapshot = _icf.document.lifecycle.ContextFieldsSnapshot
MenuDescriptorSnapshot = _icf.document.lifecycle.MenuDescriptorSnapshot
MenuItemSnapshot = _icf.document.lifecycle.MenuItemSnapshot
RECENT_LIMIT = _icf.document.lifecycle.RECENT_LIMIT
RECENT_VISIBLE = _icf.document.lifecycle.RECENT_VISIBLE
filter_target = _icf.document.lifecycle.filter_target
target_filter = _icf.document.lifecycle.target_filter

class PathComparison(Protocol):
    """Return a supplied native code in 0..3; only 2 denotes equality."""
    def compare(self, left: bytes, right: bytes, /) -> int: ...

class RecentStore(Protocol):
    def read(self, index: int, /) -> bytes: ...
    def write(self, index: int, path: bytes, /) -> None: ...

class Files(Protocol):
    def read(self, path: bytes, /) -> bytes: ...
    def publish(self, path: bytes, data: bytes, /) -> None:
        """Complete actual publication before returning; staging alone is not a save."""
        ...

class Dialogs(Protocol):
    def save_as(self, initial: SaveDialog, /) -> SaveSelection | None: ...
    def choice(self, prompt: Prompt, /) -> int: ...

class PlacementSource(Protocol):
    def placement(self) -> Placement | None: ...

__all__ = [
    "RecentFiles", "FormState", "SaveMode", "RECENT_LIMIT", "RECENT_VISIBLE",
    "filter_target", "target_filter", "PathComparison", "RecentStore", "Files",
    "SaveDialog", "SaveSelection", "Prompt", "Placement", "OpenFile", "Dialogs",
    "PlacementSource",
    "MenuQuery", "MenuContext", "MenuDescriptor", "MenuItem",
    "NativeView", "NativeState", "UiState",
    "Lifecycle", "SaveAttempt", "SaveFlow", "DirtyPrompt",
    "SaveOptions",
    "TreeState", "ViewState", "TitleState", "FormStateSnapshot", "TreeStateSnapshot",
    "ViewStateSnapshot", "TitleStateSnapshot", "ValidationViewSnapshot",
    "ValidationReportSnapshot", "PlacementSnapshot", "ContextFieldsSnapshot",
    "MenuDescriptorSnapshot", "MenuItemSnapshot",
]
