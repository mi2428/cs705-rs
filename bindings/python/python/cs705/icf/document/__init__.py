"""Shared document/session owners and explicit file, validation, and UI inputs."""
from ..._native import icf as _icf
from typing import Protocol

class BinaryWriter(Protocol):
    def write(self, data: bytes) -> int: ...
    def flush(self) -> None: ...

Document = _icf.document.Document
FileError = _icf.document.FileError
CloneError = _icf.document.CloneError
Session = _icf.document.Session
SessionContext = _icf.document.SessionContext
SessionRanges = _icf.document.SessionRanges
ActionContext = _icf.document.ActionContext
ActionEffects = _icf.document.ActionEffects
PendingName = _icf.document.PendingName
DefaultSource = _icf.document.DefaultSource
ValidationView = _icf.document.ValidationView
ValidationSource = _icf.document.ValidationSource
ValidationReport = _icf.document.ValidationReport
SaveRequest = _icf.document.SaveRequest
SaveStatus = _icf.document.SaveStatus
SaveOutcome = _icf.document.SaveOutcome
OpenStatus = _icf.document.OpenStatus
CloneOptions = _icf.document.CloneOptions
CloneOptionButton = _icf.document.CloneOptionButton
resolve_prompt = _icf.document.resolve_prompt
InitialBoundary = _icf.document.InitialBoundary
InitialRequest = _icf.document.InitialRequest
InitialState = _icf.document.InitialState
CloneControls = _icf.document.CloneControls
CloneRead = _icf.document.CloneRead
CloneReadReply = _icf.document.CloneReadReply
CloneReadStep = _icf.document.CloneReadStep
CloneReadStop = _icf.document.CloneReadStop
CloneWrite = _icf.document.CloneWrite
CloneReply = _icf.document.CloneReply
CloneStep = _icf.document.CloneStep
CloneStop = _icf.document.CloneStop
SaveReport = _icf.document.SaveReport
ResetEffects = _icf.document.ResetEffects
ImportResult = _icf.document.ImportResult
ClonePreparation = _icf.document.ClonePreparation
ClonePreparationStage = _icf.document.ClonePreparationStage

from . import initial, lifecycle, native_read, options, ui_resources

__all__ = [
    "Document", "FileError", "CloneError", "Session", "SessionContext", "SessionRanges", "ActionContext",
    "ActionEffects", "PendingName", "DefaultSource", "ValidationView",
    "ValidationSource", "ValidationReport", "SaveRequest", "SaveStatus",
    "SaveOutcome", "OpenStatus", "CloneOptions", "CloneOptionButton", "resolve_prompt",
    "InitialBoundary", "InitialRequest",
    "InitialState", "CloneControls",
    "BinaryWriter",
    "CloneRead", "CloneReadReply", "CloneReadStep", "CloneReadStop",
    "CloneWrite", "CloneReply", "CloneStep", "CloneStop",
    "SaveReport", "ResetEffects",
    "ImportResult",
    "ClonePreparation", "ClonePreparationStage",
    "initial", "lifecycle", "native_read", "options", "ui_resources",
]
