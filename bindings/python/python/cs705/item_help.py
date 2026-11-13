"""Help requests and captured API observations; no help window is launched.

Paths and resource names are bytes with no inferred host encoding. drive may
fail after changing state/effects. Collection effects are detached snapshots;
assign replacements to update the owner.
"""
from ._native import item_help as _h
Api = _h.Api
Call = _h.Call
Effects = _h.Effects
Input = _h.Input
Placement = _h.Placement
Request = _h.Request
State = _h.State
drive = _h.drive
resolve = _h.resolve
__all__ = ["Api", "Call", "Effects", "Input", "Placement", "Request", "State", "drive", "resolve"]
