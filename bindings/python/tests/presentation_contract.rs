use crate::diagnostic;
use pyo3::prelude::*;
#[test]
fn presentation_smoke() {
    let _gc = super::GC_TESTS.lock().unwrap();
    Python::initialize();
    Python::attach(|py| {
        let root = super::package(py);
        let modules = py.import("sys").unwrap().getattr("modules").unwrap();
        let locals = pyo3::types::PyDict::new(py);
        locals
            .set_item("icf", root.getattr("icf").unwrap())
            .unwrap();
        locals
            .set_item("p", root.getattr("presentation").unwrap())
            .unwrap();
        locals
            .set_item("h", root.getattr("item_help").unwrap())
            .unwrap();
        locals
            .set_item(
                "d",
                Py::new(
                    py,
                    diagnostic::Diagnostic::from_icf(cs705_core::icf::Error::Invalid {
                        line: 37,
                        reason: "original detail".into(),
                    }),
                )
                .unwrap(),
            )
            .unwrap();
        py.run(c"presenter = p.Presenter(p.Language.Japanese)\nassert presenter.message(p.Message.Enabled()) == '\\u6709\\u52b9'\nassert p.language_code(p.parse_language('de')) == 'de'\nassert d.line == 37 and d.technical_detail() == 'original detail'\nassert '37' in presenter.diagnostic(d)\nassert 'original detail' not in presenter.diagnostic(d)\nstate = h.State(0,0,19,77,88)\napi = h.Api()\neffects = h.Effects()\ntry: h.resolve(b'', h.Input.Contents(), 0, b'')\nexcept Exception: pass\nelse: raise AssertionError('missing source accepted')\ntry: state.module = True\nexcept TypeError: pass\nelse: raise AssertionError('bool accepted')\n",Some(&locals),Some(&locals)).unwrap();
        py.run(c"request = h.Request(None,None,None,b'',b'',-1,b'help.chm',b'help.chm')\napi.load_library = 11\ntry: h.drive(request,state,api,effects)\nexcept Exception as error: assert error.kind == 'SourceUnavailable'\nelse: raise AssertionError('missing observation accepted')\nassert state.module == 11 and state.cookie == 19\nassert [call.api for call in effects.calls] == ['LoadLibraryA(HHCTRL.OCX)', 'GetProcAddress(HtmlHelpA)']\napi.get_proc_address = 22\napi.initialize_cookie = 33\napi.display_window = 0\nstate.module = 0\nassert h.drive(request,state,api,effects) == 1\nassert state.d184 == 0 and state.d188 == 77\nassert effects.message_resource_index == 0\neffects.calls = effects.calls[:1]\nassert len(effects.calls) == 1\napi.cursor = (1,2)\nassert api.cursor == (1,2)\nassert p.Message.RowsProcessed(2).count == 2\nassert p.Message.Code(-3).value == -3\nassert presenter.channel_label(p.ChannelField.FrequencyHz)\n",Some(&locals),Some(&locals)).unwrap();
        py.run(c"value = p.Value.Agc(icf.Agc.Fast)\nassert value.payload is icf.Agc.Fast\nassert presenter.value(value)\nfor lang in p.Language:\n    assert p.Presenter(lang).message(p.Message.RowsProcessed(2))\ntry: p.Value.Agc(p.Language.English)\nexcept TypeError: pass\nelse: raise AssertionError('foreign enum accepted')\n",Some(&locals),Some(&locals)).unwrap();
        py.run(
            &std::ffi::CString::new(include_str!("presentation_fields.py")).unwrap(),
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
        let settings = root.getattr("icf").unwrap().getattr("settings").unwrap();
        let cases = pyo3::types::PyDict::new(py);
        for (name, module, source) in [
            (
                "settings",
                settings.clone(),
                include_str!("../python/cs705/icf/settings/__init__.pyi"),
            ),
            (
                "extended",
                settings.getattr("extended").unwrap(),
                include_str!("../python/cs705/icf/settings/extended.pyi"),
            ),
            (
                "gps",
                modules.get_item("cs705.icf.settings.gps").unwrap(),
                include_str!("../python/cs705/icf/settings/gps.pyi"),
            ),
            (
                "digital",
                modules.get_item("cs705.icf.settings.digital").unwrap(),
                include_str!("../python/cs705/icf/settings/digital.pyi"),
            ),
            (
                "network",
                settings.getattr("network").unwrap(),
                include_str!("../python/cs705/icf/settings/network.pyi"),
            ),
            (
                "presentation",
                modules.get_item("cs705.presentation").unwrap(),
                include_str!("../python/cs705/presentation.pyi"),
            ),
            (
                "item_help",
                modules.get_item("cs705.item_help").unwrap(),
                include_str!("../python/cs705/item_help.pyi"),
            ),
            (
                "display",
                root.getattr("display").unwrap(),
                include_str!("../python/cs705/display/__init__.pyi"),
            ),
            (
                "filter",
                root.getattr("display").unwrap().getattr("filter").unwrap(),
                include_str!("../python/cs705/display/filter.pyi"),
            ),
            (
                "printing",
                root.getattr("printing").unwrap(),
                include_str!("../python/cs705/printing/__init__.pyi"),
            ),
            (
                "native",
                root.getattr("printing").unwrap().getattr("native").unwrap(),
                include_str!("../python/cs705/printing/native/__init__.pyi"),
            ),
            (
                "menu",
                root.getattr("printing")
                    .unwrap()
                    .getattr("native")
                    .unwrap()
                    .getattr("menu")
                    .unwrap(),
                include_str!("../python/cs705/printing/native/menu.pyi"),
            ),
        ] {
            cases.set_item(name, (module, source)).unwrap();
        }
        locals.set_item("qa_api_cases", cases).unwrap();
        py.run(cr#"
import ast
import inspect
qa_checked = 0
def qa_signature(path, function, target, bound):
    global qa_checked
    arguments = function.args
    positional = arguments.posonlyargs + arguments.args
    defaults = [inspect.Parameter.empty] * (len(positional) - len(arguments.defaults))
    defaults += [ast.literal_eval(value) for value in arguments.defaults]
    expected = [(arg.arg, inspect.Parameter.POSITIONAL_ONLY if i < len(arguments.posonlyargs) else inspect.Parameter.POSITIONAL_OR_KEYWORD, default) for i, (arg, default) in enumerate(zip(positional, defaults))]
    if expected and expected[0][0] == 'self':
        expected = expected[1:]
    expected += [(arg.arg, inspect.Parameter.KEYWORD_ONLY, inspect.Parameter.empty if default is None else ast.literal_eval(default)) for arg, default in zip(arguments.kwonlyargs, arguments.kw_defaults)]
    assert arguments.vararg is None and arguments.kwarg is None, path
    actual = inspect.signature(target)
    right = list(actual.parameters.values())[int(bound):]
    assert expected == [(p.name, p.kind, p.default) for p in right], (path, expected, actual)
    qa_checked += 1
for label, (module, source) in qa_api_cases.items():
    tree = ast.parse(source)
    for entry in tree.body:
        if isinstance(entry, ast.ClassDef):
            if entry.name == 'ReadSeek':
                continue
            cls = getattr(module, entry.name)
            for member in entry.body:
                if isinstance(member, ast.AnnAssign):
                    assert hasattr(cls, member.target.id), (label, entry.name, member.target.id)
                elif isinstance(member, ast.FunctionDef):
                    if any(not isinstance(d, ast.Name) or d.id == 'property' for d in member.decorator_list):
                        assert hasattr(cls, member.name), (label, entry.name, member.name)
                        continue
                    if member.name.startswith('__') and member.name != '__init__':
                        continue
                    constructor = member.name == '__init__'
                    static = any(isinstance(d, ast.Name) and d.id == 'staticmethod' for d in member.decorator_list)
                    target = cls if constructor else getattr(cls, member.name)
                    qa_signature(f'{label}.{entry.name}.{member.name}', member, target, not static and not constructor)
        elif isinstance(entry, ast.FunctionDef):
            qa_signature(f'{label}.{entry.name}', entry, getattr(module, entry.name), False)
assert qa_checked > 100
assert 'readonly' in qa_api_cases['printing'][0].sync_editor.__doc__
assert 'readonly' in qa_api_cases['native'][0].sync_editor.__doc__
assert 'Never closes' in qa_api_cases['native'][0].read_source.__doc__
"#, Some(&locals), Some(&locals)).unwrap();
        py.run(
            cr#"
import gc
from cs705 import Error
from cs705.icf import settings
def qa_reject(call, error=Error):
    try:
        call()
    except error:
        pass
    else:
        raise AssertionError('invalid descriptor accepted')
memory = bytearray(0x3c9c0)
memory[0x355a1] = 6
memory[0x39346] = 1
memory[-16:] = b'IcomCloneFormat3'
image = icf.Icf.from_memory(bytes(memory))
field = settings.find('lcd_backlight_raw')
field.name, field.feature_id, field.key = 'custom\0\u8a2d', 'custom', 0xffff
field.write(image, 0, settings.Value.Number(73))
assert settings.find('lcd_backlight_raw').read(image, 0).number == 73
assert settings.find('lcd_backlight_raw').name == 'lcd_backlight_raw'
before = image.memory()
for step in (0, -1):
    field.domain.step = step
    qa_reject(lambda: field.write(image, 0, settings.Value.Number(1)))
field.domain = settings.Domain(-(1 << 63), (1 << 63) - 1, 1)
qa_reject(lambda: field.write(image, 0, settings.Value.Number((1 << 63) - 1)))
field.write(image, 0, settings.Value.Number(73))
field.domain = settings.Domain(0, 511, 1)
qa_reject(lambda: field.write(image, 0, settings.Value.Number(256)))
for index in (True, -1, 1 << 128):
    qa_reject(lambda: field.read(image, index), (TypeError, OverflowError))
assert image.memory() == before
selector = settings.catalog()[0]
selector.domain = settings.Domain(1, 5, 2)
assert selector.choices(image, 0) == [1, 3, 5]
selector.domain.min = 6
qa_reject(lambda: selector.cycle(image, 0, settings.Step.Next))
selector.domain = settings.Domain(-(1 << 63), (1 << 63) - 1, 1)
qa_reject(lambda: selector.choices(image, 0))
selector.key = 0x2fc
selector.domain = settings.Domain(1, 1, 1)
assert selector.choices(image, 0) == []
qa_reject(lambda: selector.cycle(image, 0, settings.Step.Next))
assert image.memory() == before
choice = next(f for f in settings.extended.rf_choice_fields() if f.key() == 0x336)
outside = choice.first_index + choice.count
choice.count = (1 << 64) - 1
assert not choice.accepts_index(outside)
qa_reject(lambda: choice.fixed_value(outside))
assert p.FixedValue is settings.extended.FixedValue
assert p.ChannelField is icf.record.ChannelField
assert p.RepeaterField is icf.record.RepeaterField
record = icf.record.reset_holder(2, 0, 0)
record.channel_field(p.ChannelField.FrequencyHz)
for channel_field in icf.record.ChannelField:
    assert presenter.channel_label(channel_field)
for repeater_field in icf.record.RepeaterField:
    assert presenter.repeater_label(repeater_field)
for descriptor in (settings.gps.symbol_fields()[0], settings.gps.text_fields()[0]):
    descriptor.count = (1 << 64) - 1
    for index in ((1 << 64) - 2, 0x3c9c0):
        qa_reject(lambda: descriptor.read(image, index))
        qa_reject(lambda: descriptor.edit(image, index, settings.extended.FixedTextInput.Cancel))
text = settings.gps.text_fields()[0]
text.width = (1 << 64) - 1
qa_reject(lambda: text.read(image, 0))
assert image.memory() == before
projection = field.domain
del field
gc.collect()
projection.min = 7
assert projection.min == 7
for input, kind in ((h.Input.Contents(), 'SourceUnavailable'),
                    (h.Input.Item(-1, -(1 << 31), b'\0\xff'), 'Conversion')):
    try:
        h.resolve(b'', input, 0xffffffff, b'')
    except Error as error:
        assert error.kind == kind, (kind, error.kind)
    else:
        raise AssertionError('missing help asset accepted')
"#,
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}
