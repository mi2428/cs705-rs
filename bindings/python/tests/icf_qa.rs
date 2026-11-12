#![deny(unsafe_code)]
#![allow(dead_code)]

#[path = "../src/callbacks.rs"]
mod callbacks;
#[path = "../src/clone/mod.rs"]
mod clone;
#[path = "../src/conversion.rs"]
mod conversion;
#[path = "../src/csv/mod.rs"]
mod csv;
#[path = "../src/diagnostic.rs"]
mod diagnostic;
#[path = "../src/display/mod.rs"]
mod display;
#[path = "../src/error_values.rs"]
mod error_values;
#[path = "../src/errors.rs"]
mod errors;
#[path = "../src/gps.rs"]
mod gps;
#[path = "../src/icf/mod.rs"]
mod icf;
#[path = "../src/item_help.rs"]
mod item_help;
#[path = "../src/owner.rs"]
mod owner;
#[path = "../src/presentation.rs"]
mod presentation;
#[path = "../src/printing/mod.rs"]
mod printing;
#[path = "../src/repeater/mod.rs"]
mod repeater;
#[allow(unsafe_code)]
#[path = "../src/scoped.rs"]
mod scoped;

use pyo3::{prelude::*, types::PyDict};

fn image() -> cs705_core::icf::Icf<'static> {
    let mut memory = vec![0; 0x3c9c0];
    memory[0x355a1] = 6;
    memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    cs705_core::icf::Icf::from_memory(memory).unwrap()
}

fn public_icf(py: Python<'_>) -> Bound<'_, PyModule> {
    let root = PyModule::new(py, "cs705").unwrap();
    errors::register(&root).unwrap();
    conversion::register(&root).unwrap();
    icf::register(&root).unwrap();
    let modules = py.import("sys").unwrap().getattr("modules").unwrap();
    modules.set_item("cs705", &root).unwrap();
    modules.set_item("cs705._native", &root).unwrap();
    let icf = root.getattr("icf").unwrap();
    modules.set_item("cs705.icf", &icf).unwrap();
    for name in [
        "context",
        "record",
        "record_image",
        "frequency",
        "normalizer",
        "special",
        "rows",
    ] {
        modules
            .set_item(format!("cs705.icf.{name}"), icf.getattr(name).unwrap())
            .unwrap();
    }
    let rows = icf.getattr("rows").unwrap();
    let native = rows.getattr("native").unwrap();
    modules.set_item("cs705.icf.rows.native", &native).unwrap();
    modules
        .set_item(
            "cs705.icf.rows.native.call",
            native.getattr("call").unwrap(),
        )
        .unwrap();
    modules
        .set_item("cs705.icf.rows.ur", rows.getattr("ur").unwrap())
        .unwrap();
    for (module, name, source) in [
        (
            native,
            "cs705.icf.rows.native",
            include_str!("../python/cs705/icf/rows/native/__init__.py"),
        ),
        (
            rows.getattr("ur").unwrap(),
            "cs705.icf.rows",
            include_str!("../python/cs705/icf/rows/ur.py"),
        ),
        (
            icf.getattr("special").unwrap(),
            "cs705.icf",
            include_str!("../python/cs705/icf/special.py"),
        ),
    ] {
        module.setattr("__package__", name).unwrap();
        let globals = module.cast::<PyModule>().unwrap().dict();
        py.run(
            &std::ffi::CString::new(source).unwrap(),
            Some(&globals),
            Some(&globals),
        )
        .unwrap();
    }
    icf.cast_into::<PyModule>().unwrap()
}

#[test]
fn qa_scoped_icf_storage_operations() {
    Python::initialize();
    Python::attach(|py| {
        let module = public_icf(py);
        let locals = PyDict::new(py);
        locals.set_item("icf", &module).unwrap();
        let mut native = image();
        scoped::Frame::with_icf_mut(py, &mut native, |loan| {
            locals.set_item("live", Py::new(py, icf::Icf::from_loan(loan))?)?;
            py.run(c"h = icf.record.reset_holder(3, 0, 0)\nassert icf.record.load_channel_image_into(live, icf.record_image.ChannelKind.Call, 0, h)\nh.set_name(b'LIVE'.ljust(16))\nicf.record.store_channel_image_into(live, icf.record_image.ChannelKind.Call, 0, h)\nassert live.memory()[0xa25f:0xa26f] == b'LIVE'.ljust(16)\nicf.record.load_reflector_storage_into(live, 0, h)\nicf.record.load_receive_history_storage_into(live, 0, h)\nassert icf.record.history_selection_present(live, 0x14, 0, 0)\nassert icf.record.delete_history_storage(live, 0x14, 0, 0)\n", Some(&locals), Some(&locals))
        }).unwrap();
        assert_eq!(&native.memory()[0xa25f..0xa26f], b"LIVE            ");
        scoped::Frame::with_icf_ref(py, &native, |loan| {
            locals.set_item("live", Py::new(py, icf::Icf::from_loan(loan))?)?;
            py.run(c"assert icf.record.load_channel_image_into(live, icf.record_image.ChannelKind.Call, 0, h)\ntry: icf.record.store_channel_image_into(live, icf.record_image.ChannelKind.Call, 0, h)\nexcept RuntimeError: pass\nelse: raise AssertionError('readonly Icf upgraded')\ntry: icf.record.delete_history_storage(live, 0x14, 0, 0)\nexcept RuntimeError: pass\nelse: raise AssertionError('readonly history upgraded')\n", Some(&locals), Some(&locals))
        }).unwrap();
        py.run(c"for call in [lambda: icf.record.load_channel_image_into(live, icf.record_image.ChannelKind.Call, 0, h), lambda: icf.record.load_reflector_storage_into(live, 0, h), lambda: icf.record.load_receive_history_storage_into(live, 0, h), lambda: icf.record.history_selection_present(live, 0x14, 0, 0), lambda: icf.record.delete_history_storage(live, 0x14, 0, 0)]:\n try: call()\n except ReferenceError: pass\n else: raise AssertionError('expired Icf accepted')\n", Some(&locals), Some(&locals)).unwrap();
    });
}

#[test]
fn qa_image_record_bytes_and_integer_boundaries() {
    Python::initialize();
    Python::attach(|py| {
        let locals = PyDict::new(py);
        locals.set_item("icf", public_icf(py)).unwrap();
        let source = cs705_core::icf::format::write_storage(
            image().memory(),
            cs705_core::icf::format::Layout::from_map_revision(6),
            b"raw\x82\xa0",
            0,
        )
        .unwrap();
        locals
            .set_item("source", pyo3::types::PyBytes::new(py, &source))
            .unwrap();
        py.run(
            cr"
def rejects(error, call):
    try: call()
    except error: return
    raise AssertionError('invalid input accepted')
v = icf.Icf.parse(source)
original = v.original_bytes()
v.set_scope_mode_raw(2)
assert v.scope_mode_raw() == 2 and v.original_bytes() == original == source
assert v.comment_bytes() == b'raw\x82\xa0'
assert icf.Icf.parse(v.to_bytes()).memory() == v.memory()
before = v.memory()
rejects(TypeError, lambda: v.patch_memory(iter([(0,b'x')])))
assert v.memory() == before
assert 'consum' in icf.Icf.into_owned.__doc__.lower()
assert '616' in icf.record.RecordBytes.__doc__
assert 'Separate' in icf.normalizer.ReferenceImage.__doc__
for patches in [[(1,b'x'),(2**64-1,b'')], [(1,b'x'),(len(before),b'x')]]:
    rejects(Exception, lambda: v.patch_memory(patches))
    assert v.memory() == before
v.patch_memory([(len(before),b''),(0,b'ab'),(1,b'Z')])
assert v.memory()[:2] == b'aZ'
for bad in [True, -1, 2**64, 1.0]:
    rejects((TypeError, OverflowError), lambda: v.physical_memory_name_raw(bad))
    rejects((TypeError, OverflowError), lambda: icf.record.reset_holder(bad,0,0))
rejects(TypeError, lambda: v.set_monitor(1))
rejects(TypeError, lambda: v.set_scope_mode(icf.MemorySide.Main))
rejects(TypeError, lambda: icf.Icf.from_memory(bytearray(before)))
h = icf.record.DecodedRecord.from_bytes(bytes(616))
rejects(ValueError, lambda: icf.record.DecodedRecord.from_bytes(bytes(615)))
b = h.as_mut_bytes()
b[0:4] = (3).to_bytes(4,'little')
assert h.kind() == 3 and len(b) == icf.record.DECODED_BYTES == 616
h.set_mode(17)
assert int.from_bytes(b[0x58:0x5c], 'little') == 17
for key,value in [(True,0),(0,True),(616,0),(-617,0),(slice(0,2),b'x'),(slice(None,None,0),b'')]:
    old = b.to_bytes()
    rejects((TypeError, ValueError, IndexError), lambda: b.__setitem__(key,value))
    assert b.to_bytes() == old
rejects(IndexError, lambda: b.write(2**64-1,b'xx'))
b.write(616,b'')
rejects(TypeError, lambda: memoryview(b))
del h
b[-1] = 77
assert b[-1] == 77
rejects(TypeError, lambda: icf.record.RecordBytes())
owned = v.into_owned()
rejects(RuntimeError, v.memory)
assert owned.original_bytes() == original and owned.memory()[:2] == b'aZ'
",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}

#[test]
fn qa_record_writer_failure_keeps_scratch_and_expires_payloads() {
    Python::initialize();
    Python::attach(|py| {
        let locals = PyDict::new(py);
        locals.set_item("icf", public_icf(py)).unwrap();
        locals
            .set_item("MutableBytes", py.get_type::<conversion::MutableBytes>())
            .unwrap();
        py.run(
            cr"
r = icf.record
h = r.reset_holder(2,0,0)
h.set_valid(True)
h.set_name(b'changed'.ljust(16))
h.set_channel_field(r.ChannelField.FrequencyHz,145500000)
raw = MutableBytes(b'\xa5'*83)
flag = MutableBytes(b'\xff')
saved = []
failure = ValueError('stop at frequency')
def writer(event):
    for owner in [h, raw, flag]:
        try: owner.as_bytes() if owner is h else owner.to_bytes()
        except RuntimeError: pass
        else: raise AssertionError('busy owner allowed')
    if saved:
        try: saved[-1].stage()
        except ReferenceError: pass
        else: raise AssertionError('previous payload survived callback')
    saved.append(event)
    if event.bytes is not None:
        try: event.bytes[0] = 0
        except (TypeError, RuntimeError): pass
        else: raise AssertionError('writer payload is mutable')
    if event.stage() is r.ChannelStage.Frequency: raise failure
try: r.encode_channel_into(h,raw,0,flag,writer)
except ValueError as error: assert error is failure and error.__traceback__ is not None
else: raise AssertionError('writer failure swallowed')
actual = raw.to_bytes()
assert actual[1:5] == (145500000).to_bytes(4,'little')
assert actual[5:67] == b'\xa5'*62 and actual[67:] == b'changed'.ljust(16)
assert flag.to_bytes() == b'>'
assert len(saved) == 5
for event in saved:
    try: event.stage()
    except ReferenceError: pass
    else: raise AssertionError('writer payload retained')
h.set_mode(13)
for side in [True,-1,2**64,2]:
    old = raw.to_bytes()
    try: r.encode_channel_into(h,raw,side,flag,lambda event: None)
    except (TypeError,OverflowError,Exception): pass
    else: raise AssertionError('invalid side accepted')
    assert raw.to_bytes() == old
",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}

#[test]
fn qa_live_range_generation_and_input_iterator_failures() {
    Python::initialize();
    Python::attach(|py| {
        let locals = PyDict::new(py);
        locals.set_item("icf", public_icf(py)).unwrap();
        locals
            .set_item("MutableBytes", py.get_type::<conversion::MutableBytes>())
            .unwrap();
        py.run(
            cr"
f = icf.frequency
n = icf.normalizer
assert n.FrequencyRange is f.FrequencyRange and n.FrequencyContext is f.FrequencyContext
assert n.previous_channel_bad is f.previous_channel_bad
assert [n.round_offset_hz(x) for x in [149,150,0xffffffff]] == [100,200,0]
image = MutableBytes(b'\xa5'*0x379b0)
events = []
failure = LookupError('source failed')
def count():
    events.append('count')
    return 2
def get(index):
    events.append(index)
    if index == 1: raise failure
    return f.FrequencyRange(10,20,0)
try: n.stage7_live(image,count,get)
except LookupError as error: assert error is failure
else: raise AssertionError('source failure swallowed')
assert events == ['count',0,0,'count',1]
assert image.to_bytes()[0x378c0:0x378c8] == b'\x0a\0\0\0\x14\0\0\0'
assert image.to_bytes()[0x378c8:] == b'\xa5'*(0x379b0-0x378c8)
before = image.to_bytes()
class BrokenSequence:
    def __len__(self): return 2
    def __getitem__(self,index):
        if index == 0: return f.FrequencyRange(1,2,0)
        raise failure
try: n.stage7(image,BrokenSequence())
except LookupError as error: assert error is failure
else: raise AssertionError('iterator failure swallowed')
assert image.to_bytes() == before
q = icf.context.Context.from_bytes(bytes(24))
b = f.Ranges.unpopulated()
context = b.frequency_context(0)
live = context.l30
before = [(r.low,r.high,r.flag) for r in live.items_snapshot()]
seen = []
def stop(which):
    seen.append(which)
    if which is f.Container.L58: raise failure
try: b.regenerate_with(1,q,None,stop)
except LookupError as error: assert error is failure
else: raise AssertionError('range producer failure swallowed')
assert seen == list(f.POPULATION_ORDER[:3]) and not b.is_complete()
assert [(r.low,r.high,r.flag) for r in live.items_snapshot()] != before
manual = [f.FrequencyRange(10,20,0)]
context.l30 = manual
context.l58 = manual
assert not f.common_bad(context,15)
manual[0].high = 12
assert f.common_bad(context,15)
try: f.common_bad(None,15)
except Exception: pass
else: raise AssertionError('missing source fabricated')
",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}

#[test]
fn qa_row_enums_are_canonical_python_enums() {
    Python::initialize();
    Python::attach(|py| {
        let module = public_icf(py);
        let locals = PyDict::new(py);
        locals.set_item("icf", &module).unwrap();
        py.run(cr"
from enum import Enum
for module, names in [(icf.rows.native, ['Confirmation','ChannelChoice','ChannelEnum','ToneField']), (icf.rows.ur,['Field'])]:
    for name in names:
        cls = getattr(module,name)
        assert issubclass(cls,Enum), name
        for member in cls:
            assert cls(member.value) is member
            assert member != member.value
        assert cls.__module__ == ('cs705.icf.rows.ur' if name == 'Field' else 'cs705.icf.rows.native')
assert icf.rows.native.ChannelChoice.Duplex.key() == 0x10b
assert icf.rows.native.ChannelEnum.Mode.key() == 0x108
assert icf.rows.native.ToneField.Tx.key() == 0x10e
assert icf.rows.ur.Field.Call.key() == 0x13f
holder = icf.record.reset_holder(3,0,0)
assert icf.rows.native.ChannelChoice.Duplex.choices(holder)
input = icf.settings.extended.ChoiceInput.Display
assert not icf.rows.native.channel_enum_input(holder,icf.rows.native.ChannelEnum.Mode,input).stored
context = icf.rows.native.EditorContext(icf.frequency.Ranges.unpopulated(),icf.normalizer.ReferenceImage.Working)
request = icf.rows.native.ChoiceRequest(input,context,None,icf.rows.native.ChannelChoice.Duplex)
assert request.field is icf.rows.native.ChannelChoice.Duplex
request.field = icf.rows.native.ChannelChoice.Selection
assert request.field is icf.rows.native.ChannelChoice.Selection
ur = icf.rows.ur.Request(0,icf.rows.ur.Field.Name,icf.settings.extended.FixedTextInput.Display,0,icf.normalizer.ReferenceImage.Working)
assert ur.field is icf.rows.ur.Field.Name
ur.field = icf.rows.ur.Field.Call
assert ur.field is icf.rows.ur.Field.Call
try: icf.rows.native.tone_choices(icf.rows.native.ChannelEnum.Mode,None)
except TypeError: pass
else: raise AssertionError('foreign row enum accepted')
", Some(&locals), Some(&locals)).unwrap();
    });
}

#[test]
fn qa_special_sources_fields_and_readonly_notifications() {
    Python::initialize();
    Python::attach(|py| {
        let locals = PyDict::new(py);
        locals.set_item("icf", public_icf(py)).unwrap();
        locals
            .set_item("MutableBytes", py.get_type::<conversion::MutableBytes>())
            .unwrap();
        py.run(
            cr"
s = icf.special
n = icf.normalizer
assert s.decode_bandwidth(class_=0,raw=0) == s.decode_bandwidth(0,0)
assert s.encode_bandwidth(class_=0,width=2400) == s.encode_bandwidth(0,2400)
row = s.ScanEdge(True,30000,60000,13,0,1,b'row'.ljust(16))
failure = ArithmeticError('predicate failed')
calls = []
def bad(frequency):
    calls.append(frequency)
    raise failure
try: row.suitable(False,False,True,bad)
except ArithmeticError as error: assert error is failure
else: raise AssertionError('predicate failure swallowed')
assert calls == [60000]
try: row.suitable(False,False,True,lambda hz: 1)
except TypeError: pass
else: raise AssertionError('predicate accepted int instead of bool')
clip = s.ScanClipboard([row],1)
def broken():
    yield row
    raise failure
try: clip.set_rows(broken())
except ArithmeticError as error: assert error is failure
else: raise AssertionError('iterator failure swallowed')
assert len(clip.rows_snapshot()) == 1
effect = s.Effects.default()
effect.notifications.append(s.Notification(1,True,2,3,b'payload'))
view = effect.notifications[0]
view.first = 42
assert effect.notifications[0].first == 42
saved = []
def send(event):
    saved.append(event)
    assert event.first == 42
    try: event.first = 1
    except RuntimeError: pass
    else: raise AssertionError('notification reference is mutable')
    raise failure
try: effect.dispatch(send)
except ArithmeticError as error: assert error is failure
else: raise AssertionError('dispatch failure swallowed')
try: saved[0].event
except ReferenceError: pass
else: raise AssertionError('notification survived callback')
del effect.notifications[0]
try: view.event
except ReferenceError: pass
else: raise AssertionError('removed notification view retargeted')
raw = bytearray(b' '*0x3c9c0)
raw[0x37158:0x3715d] = b'a\0*?*'
raw[0x37158+70] = 42
working = MutableBytes(bytes(raw))
n.stage9_keyer(working,n.ReferenceImage.Working)
assert working.to_bytes()[0x37158:0x3715d] == b'A *? '
assert working.to_bytes()[0x37158+70] == 32
separate = MutableBytes(bytes(raw))
n.stage9_keyer(separate,n.ReferenceImage.Separate(bytes(raw)))
assert separate.to_bytes()[0x37158+70] == 42
partial = MutableBytes(bytes(raw))
try: n.stage9_keyer(partial,n.ReferenceImage.Separate(b''))
except Exception: pass
else: raise AssertionError('missing current source fabricated')
assert partial.to_bytes()[0x37158] == ord('A')
",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}

#[test]
fn qa_rows_bounds_and_non_atomic_native_write_prefix() {
    Python::initialize();
    Python::attach(|py| {
        let locals = PyDict::new(py);
        locals.set_item("icf", public_icf(py)).unwrap();
        locals
            .set_item("MutableBytes", py.get_type::<conversion::MutableBytes>())
            .unwrap();
        py.run(
            cr"
raw = bytearray(0x4997a)
raw[0x355a1] = 6
raw[0x3c9b0:0x3c9c0] = b'IcomCloneFormat3'
raw[0x3662d:0x36821] = bytes([1])*500
raw[0x3fb40:0x3fb44] = (1).to_bytes(4,'little')
raw[0x3fcd4:0x49914] = b'\xff\xff\0\0'*10000
model = icf.rows.NormalMemory.from_working(bytes(raw))
assert model.group_count() == 1 and model.slot(0,0) is None
for group,rows in [(True,range(1)),(-1,range(1)),(1,range(1)),(0,range(101)),(0,range(0,10,2))]:
    before = model.working()
    try: model.allocate(group,rows)
    except Exception: pass
    else: raise AssertionError('invalid allocation accepted')
    assert model.working() == before
result = model.allocate(0,range(2))
assert result.requested == 2 and result.allocated == 2 and not result.exhausted
assert model.slot(0,0) == 0 and model.slot(0,1) == 1
assert model.release(0,range(1)) == 1 and model.slot(0,0) is None
native = icf.rows.native
selection = native.Selection(0,2,2,2,True,True)
seen = []
def cancel(reason):
    assert reason is native.Confirmation.Clear
    seen.append(reason)
    return 0
before = model.working()
assert not native.native_clear(model,selection,icf.normalizer.ReferenceImage.Separate(b''),native.Effects.default(),cancel).processed
assert seen == [native.Confirmation.Clear] and model.working() == before
call_image = MutableBytes(bytes(raw))
clip = native.Clipboard(3,1,1,1,[icf.record.reset_holder(3,0,0)])
def cancel_call(reason):
    assert reason is native.Confirmation.Overwrite
    return 0
assert not native.call.paste(call_image,native.call.Selection(0,0,0,False,False),clip,icf.frequency.Ranges.unpopulated(),native.Effects.default(),cancel_call,lambda index,event: None).processed
# The raw leaf can fail after a map/flag commit but before its count store.
short = MutableBytes(bytes(raw[:0x3fcd8]))
try: icf.rows.allocate_into(short,0,range(2))
except Exception: pass
else: raise AssertionError('truncated map accepted')
after = short.to_bytes()
assert after[0x3662d] == 0 and after[0x3662e] == 1
assert after[0x3fcd4:0x3fcd8] == bytes(4)
assert after[0x3fb44:0x3fb48] == bytes(4)
",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}

#[test]
fn qa_public_stub_signatures_match_native_calling_conventions() {
    Python::initialize();
    Python::attach(|py| {
        let locals = PyDict::new(py);
        locals.set_item("icf", public_icf(py)).unwrap();
        let sources = PyDict::new(py);
        macro_rules! stub {
            ($($module:literal => $path:literal),* $(,)?) => {$(
                sources.set_item($module, include_str!($path)).unwrap();
            )*};
        }
        stub! {
            "" => "../python/cs705/icf/__init__.pyi",
            "context" => "../python/cs705/icf/context.pyi",
            "format" => "../python/cs705/icf/format.pyi",
            "frequency" => "../python/cs705/icf/frequency.pyi",
            "normalizer" => "../python/cs705/icf/normalizer.pyi",
            "record_image" => "../python/cs705/icf/record_image.pyi",
            "record" => "../python/cs705/icf/record/__init__.pyi",
            "record.gps_initial" => "../python/cs705/icf/record/gps_initial.pyi",
            "rows" => "../python/cs705/icf/rows/__init__.pyi",
            "rows.native" => "../python/cs705/icf/rows/native/__init__.pyi",
            "rows.native.call" => "../python/cs705/icf/rows/native/call.pyi",
            "rows.ur" => "../python/cs705/icf/rows/ur.pyi",
            "special" => "../python/cs705/icf/special.pyi",
            "stage9" => "../python/cs705/icf/stage9.pyi",
            "text" => "../python/cs705/icf/text.pyi",
            "callsign_dialog" => "../python/cs705/icf/callsign_dialog.pyi",
        }
        locals.set_item("sources", sources).unwrap();
        py.run(cr"
import ast, inspect
errors = []
checked = 0
def check(owner, node, path, constructor=False):
    global checked
    if any(isinstance(d, ast.Name) and d.id == 'overload' for d in node.decorator_list): return
    if not constructor and any((isinstance(d,ast.Name) and d.id == 'property') or (isinstance(d,ast.Attribute) and d.attr == 'setter') for d in node.decorator_list): return
    target = owner if constructor else getattr(owner,node.name,None)
    if target is None:
        errors.append(path + ': absent')
        return
    expected = [a.arg for a in node.args.posonlyargs + node.args.args]
    if constructor: expected = expected[1:]
    try: actual = inspect.signature(target)
    except ValueError:
        errors.append(path + ': signature unavailable')
        return
    checked += 1
    if len(actual.parameters) != len(expected):
        errors.append(path + ': ' + str(expected) + ' != ' + str(actual))
        return
    defaults = [False]*(len(node.args.posonlyargs)+len(node.args.args)-len(node.args.defaults)) + [True]*len(node.args.defaults)
    if constructor: defaults = defaults[1:]
    kinds = [inspect.Parameter.POSITIONAL_ONLY]*len(node.args.posonlyargs) + [inspect.Parameter.POSITIONAL_OR_KEYWORD]*len(node.args.args)
    if constructor: kinds = kinds[1:]
    for parameter, name, kind, optional in zip(actual.parameters.values(),expected,kinds,defaults):
        if name == 'self': continue
        if parameter.kind != kind or (kind is not inspect.Parameter.POSITIONAL_ONLY and parameter.name != name):
            errors.append(path + ': parameter mismatch ' + str(actual))
        if (parameter.default is not inspect.Parameter.empty) != optional:
            errors.append(path + ': default mismatch ' + str(actual))
for module, source in sources.items():
    owner = icf
    for part in module.split('.') if module else []: owner = getattr(owner,part)
    for node in ast.parse(source).body:
        if isinstance(node,ast.FunctionDef): check(owner,node,module + '.' + node.name)
        if isinstance(node,ast.ClassDef) and node.name != 'ErrorData':
            cls = getattr(owner,node.name)
            for method in node.body:
                if isinstance(method,ast.FunctionDef): check(cls,method,module + '.' + node.name + '.' + method.name,method.name == '__init__')
assert checked > 650, checked
assert not errors, (len(errors), '\n'.join(errors[:20]))
", Some(&locals), Some(&locals)).unwrap();
    });
}
