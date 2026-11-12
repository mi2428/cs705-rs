"""Stored settings; no device access or inferred factory defaults.

Catalog descriptors are detached owners. Their domain and kind properties are
live nested projections; template construction preserves private coordinates.
Checked writes validate storage width; selected-event bypasses retain raw codes.
"""
from . import digital, extended, gps, network
from ..._native import icf as _icf

Selection = _icf.settings.Selection
Setting = _icf.settings.Setting
Kind = _icf.settings.Kind
find = _icf.settings.find
catalog = _icf.settings.catalog
Domain = _icf.settings.Domain
Effect = _icf.settings.Effect
ControlState = _icf.settings.ControlState
Value = _icf.settings.Value
Step = _icf.settings.Step

__all__ = ["Selection", "Setting", "Kind", "find", "catalog", "Domain", "Effect", "ControlState", "Value", "Step", "digital", "extended", "gps", "network"]
