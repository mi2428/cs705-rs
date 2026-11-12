"""Stored IPv4 configuration and native linked-edit results."""
from ..._native import icf as _icf

Field = _icf.settings.network.Field
Configuration = _icf.settings.network.Configuration
Effect = _icf.settings.network.Effect
parse_typed = _icf.settings.network.parse_typed
prefix_mask = _icf.settings.network.prefix_mask
correct_address = _icf.settings.network.correct_address
correct_gateway = _icf.settings.network.correct_gateway
