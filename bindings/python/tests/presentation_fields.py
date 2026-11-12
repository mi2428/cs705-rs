"""Check native payload retention and explicit public-field assignment."""
from cs705 import Error, icf, presentation as p, diagnostic as d, item_help as h

try:
    icf.Icf.parse(b"")
except Error as error:
    diagnostic = error.diagnostic
else:
    raise AssertionError("invalid source accepted")

detail = diagnostic.technical_detail()
diagnostic.domain = d.Domain.Io
diagnostic.kind = d.Kind.Limit
diagnostic.line = 0
diagnostic.index = 17
diagnostic.value = 0xFFFFFFFF
assert diagnostic.domain is d.Domain.Io and diagnostic.kind is d.Kind.Limit
assert (diagnostic.line, diagnostic.index, diagnostic.value) == (0, 17, 0xFFFFFFFF)
assert diagnostic.technical_detail() == detail
presenter = p.Presenter(p.Language.English)
assert not hasattr(p, "Setting")
assert not hasattr(p, "ChoiceField")
choice = icf.settings.extended.choice_fields()[0]
assert presenter.choice(choice, choice.first_index, 0).in_domain
assert presenter.choice_label(choice, choice.first_index)
settings = icf.settings
setting = settings.find("lcd_backlight_raw")
assert isinstance(setting, settings.Setting)
setting.name = "owned\0設定"
field_text = presenter.setting(setting)
assert (field_text.key, field_text.name) == (setting.key, "owned\0設定")
setting.name = "changed"
assert field_text.name == "owned\0設定"
setting.domain.min, setting.domain.max, setting.domain.step = 5, 9, 2
setting.count = 2
assert presenter.setting_value(setting, 1, 7).in_domain
assert not presenter.setting_value(setting, 0, 6).in_domain
custom = settings.Setting(setting, "custom", "feature", setting.key,
                          setting.selection_key, setting.domain, 1, setting.kind)
assert presenter.setting(custom).name == "custom"
assert presenter.setting_value(custom, 0, 9).in_domain
assert settings.find("lcd_backlight_raw").name == "lcd_backlight_raw"
try:
    presenter.setting_value(custom, 1, 7)
except Error:
    pass
else:
    raise AssertionError("invalid setting index accepted")
assert presenter.diagnostic(diagnostic) == diagnostic.summary(p.Language.English)
assert "17" in presenter.diagnostic(diagnostic)
for field, invalid in [
    ("domain", d.Kind.Invalid), ("kind", d.Domain.Icf),
    ("domain", 0), ("kind", True),
    ("line", True), ("line", -1), ("line", 1 << 128),
    ("index", True), ("index", -1), ("index", 1 << 128),
    ("value", True), ("value", -1), ("value", 1 << 32),
]:
    before = getattr(diagnostic, field)
    try:
        setattr(diagnostic, field, invalid)
    except (TypeError, OverflowError, ValueError):
        assert getattr(diagnostic, field) == before
    else:
        raise AssertionError((field, invalid))
for field in ("line", "index", "value"):
    setattr(diagnostic, field, None)
    assert getattr(diagnostic, field) is None
assert diagnostic.technical_detail() == detail

call = h.Call("Unlisted\0API名", None)
assert call.api == "Unlisted\0API名" and call.command is None
call.api = "Another API"
call.command = 0xFFFFFFFF
effects = h.Effects()
effects.calls = [call]
assert effects.calls[0].api == call.api and effects.calls[0].command == 0xFFFFFFFF
call.api = "Changed API"
effects.calls = [call]
assert effects.calls[0].api == "Changed API"
call.command = None
assert call.command is None
for invalid in (True, -1, 1 << 32):
    try:
        h.Call("API", invalid)
    except (TypeError, OverflowError, ValueError):
        pass
    else:
        raise AssertionError(invalid)
    try:
        call.command = invalid
    except (TypeError, OverflowError, ValueError):
        assert call.command is None
    else:
        raise AssertionError(invalid)
try:
    call.api = b"bytes are not str"
except TypeError:
    assert call.api == "Changed API"
else:
    raise AssertionError("bytes accepted")

placement = h.Placement(0xFFFFFFFF, -(1 << 31), (1 << 31) - 1, 0, 0xFFFFFFFF)
assert (placement.window, placement.x, placement.y, placement.style, placement.flags) == (
    0xFFFFFFFF, -(1 << 31), (1 << 31) - 1, 0, 0xFFFFFFFF)
for field, value in (("window", 1), ("x", -2), ("y", 3), ("style", 4), ("flags", 5)):
    setattr(placement, field, value)
    assert getattr(placement, field) == value
effects.placement = placement
assert effects.placement.x == -2 and effects.placement.flags == 5
for field, invalid in [("window", True), ("window", -1), ("style", 1 << 32),
                       ("flags", -1), ("x", True), ("x", 1 << 31), ("y", -(1 << 31) - 1)]:
    before = getattr(placement, field)
    try:
        setattr(placement, field, invalid)
    except (TypeError, OverflowError, ValueError):
        assert getattr(placement, field) == before
    else:
        raise AssertionError((field, invalid))
try:
    h.Placement(True, 0, 0, 0, 0)
except TypeError:
    pass
else:
    raise AssertionError("bool constructor accepted")
field = p.FieldText(1, "field", "label")
field.key, field.name, field.label = 65535, "updated", "new label"
assert (field.key, field.name, field.label) == (65535, "updated", "new label")
value = p.ValueText(-1, "raw", False)
value.raw, value.text, value.in_domain = 7, "seven", True
assert (value.raw, value.text, value.in_domain) == (7, "seven", True)
for instance, name, invalid in [(field, "key", True), (field, "key", 65536),
                                 (value, "raw", True), (value, "in_domain", 1)]:
    before = getattr(instance, name)
    try:
        setattr(instance, name, invalid)
    except (TypeError, OverflowError):
        assert getattr(instance, name) == before
    else:
        raise AssertionError((name, invalid))
request = h.Request(None, None, None, b"", b"", -1, b"help", b"target")
for name, value in [("key", 1), ("index", 2), ("group", 3), ("topic", 4),
                    ("original_name", b"original"), ("name", b"name"),
                    ("chm", b"file"), ("target", b"window")]:
    setattr(request, name, value)
    assert getattr(request, name) == value
print("presentation public fields: passed")
