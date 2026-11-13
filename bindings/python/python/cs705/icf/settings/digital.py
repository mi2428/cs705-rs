"""Byte-oriented digital editors; native_name() is distinct from Enum.name.

NLS callbacks receive scoped buffers. Sanitation and insertion can survive a
later exception; these operations are not transactional document edits.
"""
from functools import wraps
from ..._native import icf as _icf

_digital = _icf.settings.digital
SelectionField = _digital.SelectionField
TextField = _digital.TextField
SelectionEffect = _digital.SelectionEffect
AllowedView = _digital.AllowedView
AllowedEffect = _digital.AllowedEffect
ReflectorView = _digital.ReflectorView
ur_physical_slots = _digital.ur_physical_slots
allowed_view = _digital.allowed_view
edit_allowed = _digital.edit_allowed
edit_allowed_with_nls = _digital.edit_allowed_with_nls
reflector_view = _digital.reflector_view


def _method(native):
    @wraps(native)
    def call(self, *args, **kwargs):
        return native(self, *args, **kwargs)
    return call


for _enum, _prefix, _methods in (
    (SelectionField, 'selection', ('native_name', 'feature_id', 'key', 'count',
                                 'active_base', 'active_raw', 'selected', 'edit')),
    (TextField, 'text', ('native_name', 'feature_id', 'key', 'count', 'width',
                       'raw', 'store_encoded_message', 'display', 'edit', 'edit_with_nls')),
):
    for _name in _methods:
        setattr(_enum, _name, _method(getattr(_digital, f'{_prefix}_{_name}')))
    _enum.find = staticmethod(getattr(_digital, f'{_prefix}_find'))
    _enum.ALL = tuple(_enum)

__all__ = ['SelectionField', 'TextField', 'SelectionEffect', 'AllowedView',
           'AllowedEffect', 'ReflectorView', 'ur_physical_slots', 'allowed_view',
           'edit_allowed', 'edit_allowed_with_nls', 'reflector_view']
