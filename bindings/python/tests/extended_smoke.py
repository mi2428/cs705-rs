"""Explicit native sources, owned descriptors and ordered callback failures."""
import struct
from enum import Enum
from cs705 import MutableBytes, icf, presentation
from cs705.icf.settings import extended as e

raw = bytearray(0x3C9C0)
raw[0x355A1] = 6
raw[-16:] = b"IcomCloneFormat3"
image = icf.Icf.from_memory(bytes(raw))
for cls in (e.ColorField,e.PackedField,e.AmplifierField,e.FixedTextField,
            e.ConditionalField,e.MetadataField,e.DrField,e.RfLayout,e.EdgeSide):
    assert issubclass(cls,Enum) and cls.__module__ == "cs705.icf.settings.extended"
for cls in (e.ColorField,e.PackedField,e.AmplifierField,e.FixedTextField,
            e.ConditionalField,e.MetadataField,e.DrField):
    assert tuple(cls) == cls.ALL
    assert all(field.feature_id() and field.key() for field in cls)
assert e.FixedValue.AgcFast.source_label() == "FAST"
assert e.paint_hex_byte(0xAB) == b"ABh"
assert e.utc_offset_text(-90) == b"-1:30"
assert e.edit_number(image,"lcd_backlight_raw",e.NumberInput.Text(b"42"),46).stored
assert icf.settings.find("lcd_backlight_raw").read(image,0).number == 42
assert e.edit_utc_offset(image,e.NumberInput.Text(b"+1:32")).candidate == 90
assert e.edit_dtmf(image,0,e.TextInput.Text(b"12EF")).display == b"12*#"
assert e.edit_rtty(image,0,e.TextInput.Text(b"ab\\cd")).display == b"AB\\CD"
assert e.edit_color(image,e.ColorField.ScopeCurrent,b"\1\2\3").value == b"\1\2\3"
assert e.PackedField.Vox.edit(image,e.ChoiceInput.Next).stored
assert e.AmplifierField.Attenuator.edit(image,13,e.ChoiceInput.Next).fixed_value is e.FixedValue.NotApplicable
fields=e.choice_fields()
assert fields and e.rf_choice_fields() and e.key_assignment_fields() and e.number_fields()
field=fields[0]
copy=e.ChoiceField(field,field.first_index,field.count)
assert copy.read(image,copy.first_index) == field.read(image,field.first_index)
assert copy.feature_id() == field.feature_id() and copy.domain().step > 0
assert not hasattr(presentation,"ChoiceField")
presenter=presentation.Presenter(presentation.Language.English)
assert presenter.choice(copy,copy.first_index,0).in_domain
assert presenter.choice_label(copy,copy.first_index)
catalog=e.ConditionalField.Coordinates.catalog_sources(1,None,None)
assert catalog.field() is e.ConditionalField.Coordinates and catalog.values() == (0,1,2)
custom=e.ConditionalCatalog(catalog,"caller\0selector")
assert custom.selector == "caller\0selector" and custom.values() == catalog.values()
custom.selector="changed"
assert custom.edit(image,e.ChoiceInput.Next).stored
assert e.MetadataField.TransmitPicture.read(image).read_only
assert not e.MetadataField.TransmitPicture.clear(image,0).stored
assert e.MetadataField.TransmitPicture.clear(image,e.MetadataField.TransmitPicture.clear_command()).stored
shared=MutableBytes(b" AB     ")
e.align_callsign(shared)
assert shared.to_bytes() == b"      AB"

sentinel=ValueError("extended native callback failed")
saved=[]
calls=[]
source=MutableBytes(b"A\0B")
request=e.FixedTextInput.Text(source,1)
assert request.source is source
before=image.memory()
def fail_map(text,destination):
    saved.append(destination)
    calls.append("map")
    assert text == b"A B   " and len(destination) == 6
    destination[0]=ord("Z")
    raise sentinel
def never_upper(destination):
    raise AssertionError("map failure must stop")
try:
    e.edit_fixed_text_with_nls(image,e.FixedTextField.Comment,0,request,0,fail_map,never_upper)
except ValueError as error:
    assert error is sentinel and calls == ["map"] and error.__traceback__ is not None
else:
    raise AssertionError("NLS exception lost")
assert source.to_bytes() == b"A B" and image.memory() == before
try:
    saved[0].to_bytes()
except ReferenceError:
    pass
else:
    raise AssertionError("NLS destination survived")
def map_text(text,destination):
    destination[:len(text)]=text
    return len(text)
def upper_text(destination):
    destination[:]=destination.to_bytes().upper()
    return len(destination)
source=MutableBytes(b"ab cd")
out=e.edit_fixed_text_with_nls(image,e.FixedTextField.PictureRecipient,0,e.FixedTextInput.Text(source,1),1,map_text,upper_text)
assert out.text.stored and e.FixedTextField.PictureRecipient.raw(image,0) == b"AB    CD"

q=icf.context.Context.before_registry()
ranges=icf.frequency.Ranges.generate(q,0x411)
frequency=e.FrequencyRuntime(ranges,q,0x1234,46)
assert frequency.ranges is ranges and frequency.live_q is q
assert e.SelectorRuntime(0x411,1,q).q is q
hz=ranges.f760(0x16,0,0).low+10000
text=f"{hz/1000000:.6f}".encode("ascii")
before=image.memory()
saved.clear()
calls.clear()
def cut_store(offset,data):
    saved.append(data)
    calls.append(offset)
    assert data.to_bytes()
    for access in (image.memory,q.as_bytes,ranges.is_complete):
        try:
            access()
        except RuntimeError:
            pass
        else:
            raise AssertionError("busy native owner reentry accepted")
    if offset == 4:
        raise sentinel
try:
    e.edit_fixed_edge_with_store(image,frequency,0,e.EdgeSide.Low,e.NumberInput.Text(text),cut_store)
except ValueError as error:
    assert error is sentinel and calls == [0,4]
else:
    raise AssertionError("late store failure lost")
expected=bytearray(before)
struct.pack_into("<I",expected,0x3973A,hz)
assert image.memory() == bytes(expected)
try:
    saved[0].to_bytes()
except ReferenceError:
    pass
else:
    raise AssertionError("store view survived")
assert e.edit_fixed_edge(image,frequency,0,e.EdgeSide.Low,e.NumberInput.Text(text)).stored
assert e.rf_row_visible(q,e.RfLayout.Reference,0)
assert e.auto_repeater_row_visible(q,0)
home=e.edit_home_vfo(image,ranges,q,e.NumberInput.Text(b""),46,0)
child=home.number
child.candidate=7
assert home.number.candidate == 7
replacement=e.NumberEffect(False,False,9,9,0,0,b"",False)
home.number=replacement
assert child.candidate == 9
scan_source=MutableBytes(b" not a number ")
scan=e.ProgramScanInput.Text(scan_source)
assert scan.source is scan_source
e.edit_program_scan(image,image.memory(),b"",scan)
assert scan_source.to_bytes() == b""
for construct in (lambda:e.ColorEffect(False,b"1234",0),
                  lambda:e.FilterView(0,(True,0),0,0),
                  lambda:e.FrequencyRuntime(ranges,q,True,46)):
    try:
        construct()
    except (TypeError,ValueError):
        pass
    else:
        raise AssertionError("invalid boundary accepted")
print("extended descriptor/editor/NLS/failed-store smoke: passed")
