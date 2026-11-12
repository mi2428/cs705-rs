"""Headless display calculations with explicit images, metrics and assets.

Copied nested values use *_snapshot() and explicit set_*() methods. State.views
is a live owner view; assigning ordinary Views transfers its native owner.
No native GUI events or OS calls are delivered.
Library bookkeeping counters reject unrepresentable advances with OSError
before changing that operation's state or invoking its callback.
"""
from .._native import display as _d
from . import filter
Settings = _d.Settings
InputOptions = _d.InputOptions
FontMetrics = _d.FontMetrics
Cell = _d.Cell
Column = _d.Column
Row = _d.Row
Grid = _d.Grid
Queued = _d.Queued
Toolbar = _d.Toolbar
State = _d.State
NoticeParent = _d.NoticeParent
NoticeCell = _d.NoticeCell
ViewKind = _d.ViewKind
ViewState = _d.ViewState
Views = _d.Views
Resources = _d.Resources
sql_mask = _d.sql_mask
column_hidden = _d.column_hidden
channel_cell = _d.channel_cell
channel_value_paint = _d.channel_value_paint
channel_paint = _d.channel_paint
notice_cell = _d.notice_cell
__all__ = ["Settings", "InputOptions", "FontMetrics", "Cell", "Column", "Row", "Grid",
           "Queued", "Toolbar", "State", "NoticeParent", "NoticeCell", "ViewKind",
           "ViewState", "Views", "Resources", "sql_mask", "column_hidden", "channel_cell",
           "channel_value_paint", "channel_paint", "notice_cell", "filter"]
