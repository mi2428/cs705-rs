"""Small runnable contract check: python -m cs705.icf.settings._digital_check."""
from enum import Enum
from cs705 import Error, Icf, MutableBytes, _native
from cs705.csv.nls import Bytes
from cs705.icf.settings import Selection, digital as d
from cs705.icf.settings.extended import ChoiceInput, FixedTextInput, TextEffect


def image():
    memory = bytearray(b'\xa6' * 0x3c9c0)
    memory[0x355a1] = 6
    memory[0x3c9b0:] = b'IcomCloneFormat3'
    memory[0x395ad:0x3969d] = b' ' * 240
    memory[0x3c0a7:0x3c2ff] = b'\xff' * 600
    return Icf.from_memory(bytes(memory))


def rejects(exception, call):
    try:
        call()
    except exception:
        return
    raise AssertionError(f'expected {exception.__name__}')


def check():
    native = _native.icf.settings.digital
    for name in d.__all__:
        assert getattr(d, name) is getattr(native, name)
    assert issubclass(d.SelectionField, Enum) and issubclass(d.TextField, Enum)
    for cls, count in ((d.SelectionField, 2), (d.TextField, 3)):
        assert len(cls.ALL) == count and cls.ALL == tuple(cls)
        for field in cls.ALL:
            assert field.name in ('MyCall', 'MyCallSuffix', 'TxMessage')
            assert cls.find(field.native_name()) is field
            assert cls.find(field.feature_id()) is field
            assert isinstance(field.key(), int) and field.count() in (5, 6)
        rejects(Error, lambda: cls.find('not-a-field'))
    img = image()
    selection = d.SelectionField.MyCall
    assert selection.active_base() == 0
    assert d.SelectionField.TxMessage.active_base() == 1
    result = selection.edit(img, 5, ChoiceInput.Selected(Selection(1, 2, 1, True)))
    assert result.helper_accepted and result.stored and result.candidate == 1
    assert selection.active_raw(img) == 5 and selection.selected(img, 5)
    assert result.active_raw == 5 and result.notify_flags == 0x20
    assert not result.popup_requested
    rejects(Error, lambda: selection.selected(img, selection.count()))
    rejects(TypeError, lambda: selection.selected(img, True))
    rejects(TypeError, lambda: native.selection_count(d.TextField.MyCall))

    field = d.TextField.MyCall
    raw = MutableBytes(b'ab cd')
    result = field.edit(img, 5, FixedTextInput.Text(raw, 1), 1)
    assert isinstance(result, TextEffect) and result.stored
    assert result.display == b'AB    CD' and field.width() == 8
    assert field.raw(img, 5) == b'AB    CD' and field.display(img, 5) == b'AB    CD'
    message = d.TextField.TxMessage
    message.store_encoded_message(img, 4, b'\x82\xa0')
    assert message.raw(img, 4) == b'\x82\xa0' + b' ' * 18
    before = message.raw(img, 4)
    rejects(Error, lambda: message.store_encoded_message(img, 4, b'x' * 21))
    assert message.raw(img, 4) == before
    rejects(Error, lambda: field.store_encoded_message(img, 0, b'x'))
    rejects(TypeError, lambda: message.store_encoded_message(img, 4, bytearray(b'x')))
    assert d.ur_physical_slots(img) == []
    assert d.allowed_view(img, 0).bad

    calls, views = [], []
    def mapping(source, destination):
        assert isinstance(source, bytes) and isinstance(destination, Bytes)
        calls.append('map')
        views.append(destination)
        destination[:len(source)] = source
        return len(source)
    def upper(destination):
        calls.append('upper')
        destination[:] = destination.to_bytes().upper()
        return len(destination)
    result = field.edit_with_nls(img, 0, FixedTextInput.Text(MutableBytes(b'ab'), 2), 0, mapping, upper)
    assert result.display == b'AB      ' and calls == ['map', 'upper']
    rejects(ReferenceError, lambda: views[0].to_bytes())

    # Insertion, notification and sanitation prefixes survive callback failure.
    img = image()
    raw = MutableBytes(b'a\0b')
    effect = d.AllowedEffect.default()
    marker = LookupError('digital callback')
    calls.clear()
    def failing(source, destination):
        calls.append('fail')
        assert source.startswith(b'a b')
        destination[0] = 90
        views.append(destination)
        rejects(RuntimeError, lambda: d.allowed_view(img, 0))
        rejects(RuntimeError, lambda: raw.to_bytes())
        rejects(RuntimeError, lambda: effect.notifications)
        raise marker
    try:
        d.edit_allowed_with_nls(img, 0, 13, FixedTextInput.Text(raw, 1), 0, effect, failing, upper)
    except LookupError as error:
        assert error is marker
        assert any(frame.tb_frame.f_code.co_name == 'failing' for frame in traceback_frames(error))
    else:
        raise AssertionError('callback failure was swallowed')
    assert calls == ['fail'] and raw.to_bytes() == b'a b'
    assert effect.inserted and not effect.stored and effect.notifications == (0x40, 0x39)
    assert d.allowed_view(img, 0).raw == b'CALLSIGN'
    rejects(ReferenceError, lambda: views[-1].to_bytes())
    d.edit_allowed(img, 0, 0, FixedTextInput.Begin, 0, effect)
    assert effect.state_out == 2 and not effect.inserted
    assert d.reflector_view(img, 0).__class__ is d.ReflectorView

    # Every public struct field has strict construction and whole-field writes.
    values = [
        (d.SelectionEffect(True, False, None, 1, 2, False),
         'helper_accepted stored candidate active_raw notify_flags popup_requested'),
        (d.AllowedView(0, True, False, -1, b'12345678', b''),
         'prefix_count bad duplicate_warning parent_enable_raw raw display'),
        (d.AllowedEffect(False, False, False, True, False, 0, 0, [1, 2], b''),
         'inserted stored full_warning view_bad duplicate_warning state_out notify_flags notifications display'),
        (d.ReflectorView(True, b'12345678', b'123'), 'valid raw display'),
    ]
    for value, fields in values:
        for name in fields.split():
            setattr(value, name, getattr(value, name))
    rejects(ValueError, lambda: setattr(values[1][0], 'raw', b'short'))
    rejects(TypeError, lambda: setattr(effect, 'notifications', [True]))
    rejects(OverflowError, lambda: setattr(effect, 'notify_flags', -1))
    rejects(TypeError, lambda: setattr(values[0][0], 'candidate', True))
    print('Digital check: OK (2 enums, 4 structs / 24 fields, native edits and NLS failure prefix)')


def traceback_frames(error):
    frame = error.__traceback__
    while frame:
        yield frame
        frame = frame.tb_next


if __name__ == '__main__':
    check()
