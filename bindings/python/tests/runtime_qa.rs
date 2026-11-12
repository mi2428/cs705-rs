#![deny(unsafe_code)]

#[path = "../src/callbacks.rs"]
pub mod callbacks;
#[path = "../src/clone/mod.rs"]
pub mod clone;
#[path = "../src/conversion.rs"]
pub mod conversion;
#[path = "../src/csv/mod.rs"]
pub mod csv;
#[path = "../src/diagnostic.rs"]
pub mod diagnostic;
#[path = "../src/display/mod.rs"]
pub mod display;
#[path = "../src/error_values.rs"]
pub mod error_values;
#[path = "../src/errors.rs"]
pub mod errors;
#[path = "../src/gps.rs"]
pub mod gps;
#[path = "../src/icf/mod.rs"]
pub mod icf;
#[path = "../src/owner.rs"]
pub mod owner;
#[path = "../src/presentation.rs"]
pub mod presentation;
#[path = "../src/printing/mod.rs"]
pub mod printing;
#[path = "../src/repeater/mod.rs"]
pub mod repeater;
#[allow(unsafe_code)]
#[path = "../src/scoped.rs"]
pub mod scoped;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyModule};
use std::collections::VecDeque;

// A bounded in-memory provider, not an OS port or a production codec oracle.
#[pyclass(subclass)]
struct ProbeTransport {
    now: u64,
    sent: Vec<u8>,
    incoming: VecDeque<u8>,
    replied: bool,
    #[pyo3(get)]
    closes: usize,
    #[pyo3(get)]
    events: Vec<&'static str>,
}

#[pymethods]
impl ProbeTransport {
    #[new]
    fn new() -> Self {
        Self {
            now: 0,
            sent: Vec::new(),
            incoming: VecDeque::new(),
            replied: false,
            closes: 0,
            events: Vec::new(),
        }
    }
    #[getter]
    fn wire<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.sent)
    }
    fn now_ms(&mut self) -> u64 {
        self.events.push("clock");
        self.now
    }
    fn wait_ms(&mut self, ms: u64) {
        self.events.push("wait");
        self.now = self.now.checked_add(ms).unwrap();
    }
    fn read<'py>(&mut self, py: Python<'py>, maximum: usize) -> Bound<'py, PyBytes> {
        self.events.push("read");
        let data: Vec<_> = self
            .incoming
            .drain(..maximum.min(self.incoming.len()))
            .collect();
        PyBytes::new(py, &data)
    }
    fn write(&mut self, data: &Bound<'_, PyBytes>) -> usize {
        self.events.push("write");
        let count = data.as_bytes().len().min(3);
        self.sent.extend_from_slice(&data.as_bytes()[..count]);
        if !self.replied && self.sent.last() == Some(&0xfd) {
            // NOTES 8.3: E1 model/revision plus a 33-byte tail, selector 6.
            self.incoming
                .extend([0xfe, 0xfe, 0xef, 0xee, 0xe1, 0x40, 0x75, 0, 1, 33, 6]);
            self.incoming.extend([0; 31]);
            self.incoming.push_back(0xfd);
            self.replied = true;
        }
        count
    }
    fn flush(&mut self) {
        self.events.push("flush");
    }
    fn pending_output(&mut self) -> usize {
        self.events.push("pending");
        0
    }
    fn reconfigure(&mut self, _token: &Bound<'_, PyBytes>) {
        self.events.push("reconfigure");
    }
    fn close(&mut self) {
        self.events.push("close");
        self.closes += 1;
    }
}

fn runtime_module(py: Python<'_>) -> Bound<'_, PyDict> {
    let root = PyModule::new(py, "cs705").unwrap();
    errors::register(&root).unwrap();
    conversion::register(&root).unwrap();
    clone::register(&root).unwrap();
    let modules = py.import("sys").unwrap().getattr("modules").unwrap();
    modules.set_item("cs705", &root).unwrap();
    let native = root.getattr("clone").unwrap();
    modules.set_item("cs705.clone", &native).unwrap();
    for name in ["search", "information"] {
        modules
            .set_item(format!("cs705.clone.{name}"), native.getattr(name).unwrap())
            .unwrap();
    }
    let locals = PyDict::new(py);
    locals.set_item("c", native).unwrap();
    locals
        .set_item("ProbeTransport", py.get_type::<ProbeTransport>())
        .unwrap();
    locals
}

#[test]
fn runtime_error_metadata_preserves_hostile_exception() {
    Python::initialize();
    Python::attach(|py| {
        let locals = PyDict::new(py);
        locals
            .set_item("FileError", py.get_type::<error_values::FileErrorData>())
            .unwrap();
        py.run(
            c"class HostileError(OSError):
    def __setattr__(self, name, value):
        if name == 'native': raise RuntimeError('metadata hook')
        super().__setattr__(name, value)
primary = HostileError(13, 'source failure')
try: raise primary
except HostileError: original_traceback = primary.__traceback__
carrier = FileError.Output(primary)
assert carrier.exception() is primary
assert primary.__traceback__ is original_traceback
assert primary.native is carrier
assert carrier.output.exception() is primary
import gc, weakref
weak = weakref.ref(primary)
del primary, carrier, original_traceback
gc.collect()
assert weak() is None
class DescriptorError(OSError):
    def __getattribute__(self, name):
        if name == '__dict__': raise RuntimeError('dictionary hook')
        return super().__getattribute__(name)
    @property
    def native(self): raise RuntimeError('descriptor hook')
    @native.setter
    def native(self, value): raise RuntimeError('descriptor hook')
primary = DescriptorError(13, 'descriptor source')
carrier = FileError.Output(primary)
assert carrier.exception() is primary
assert BaseException.__dict__['__dict__'].__get__(primary)['native'] is carrier
",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}

#[test]
fn runtime_transport_failure_identity_cleanup_and_proof() {
    Python::initialize();
    Python::attach(|py| {
        let locals = runtime_module(py);
        py.run(
            c"config = c.SessionConfig()
config.prepare_delay_ms = 0
config.poll_ms = 1
config.timeout_ms = 10
config.drain_timeout_ms = 10
def outcome(error):
    return next(e.args[1] for e in error.__context__.exceptions
                if len(e.args) == 2 and isinstance(e.args[1], c.Outcome))
primary = ValueError('provider failure')
cleanup = OSError('close failure')
class Broken(ProbeTransport):
    def now_ms(self): raise primary
    def close(self):
        super().close()
        raise cleanup
t = Broken()
engine = c.Session.query(config)
try: engine.run(t, lambda: False)
except ValueError as error:
    assert error is primary and error.__traceback__ is not None
    assert cleanup in error.__context__.exceptions
    failed = outcome(error)
else: raise AssertionError('clock exception lost')
assert t.events == ['close'] and t.closes == 1
assert failed.failure.kind == 'Io' and failed.close_error is c.ErrorKind.Other
try: engine.run(t, lambda: False)
except RuntimeError: pass
else: raise AssertionError('failed session was reusable')
assert t.closes == 1
class CloseFailure(ProbeTransport):
    def close(self):
        super().close()
        raise cleanup
t = CloseFailure()
engine = c.Session.query(config)
def cancel_prefix():
    if t.wire: raise primary
    return False
try: engine.run(t, cancel_prefix)
except ValueError as error:
    assert error is primary
    failed = outcome(error)
else: raise AssertionError('cancel exception lost')
assert t.closes == 1 and len(t.wire) == 3
assert failed.transmitted_wire_bytes == 3
assert failed.requested_wire_bytes == 10
assert t.events[-1] == 'close'
class LateClockFailure(ProbeTransport):
    def now_ms(self):
        if self.wire: raise primary
        return super().now_ms()
t = LateClockFailure()
try: c.Session.query(config).run(t, lambda: False)
except ValueError as error:
    assert error is primary
    failed = outcome(error)
else: raise AssertionError('late clock exception lost')
assert t.closes == 1 and len(t.wire) == 10
assert t.events.count('read') == 1 and t.events[-1] == 'close'
t = ProbeTransport()
engine = c.Session.query(config)
def reenter():
    try: engine.run(t, lambda: False)
    except RuntimeError: pass
    else: raise AssertionError('recursive run accepted')
    return False
query = engine.run(t, reenter)
assert query.result_code() == 0 and query.close_error is None
assert t.closes == 1 and t.wire == bytes.fromhex('FEFEEEEFE040750000FD')
assert query.requested_wire_bytes == query.transmitted_wire_bytes == 10
assert query.received_wire_bytes == 43
assert t.events[-2:] == ['pending', 'close']
profile = c.Profile(6)
c.Session.read_after_query(config, profile, query)
import copy, pickle
copied = copy.copy(query)
c.Session.read_after_query(config, profile, copied)
copied.identity.tail = bytes([33, 5]) + bytes(31)
try: c.Session.read_after_query(config, profile, copied)
except Exception: pass
else: raise AssertionError('edited proof accepted')
failed.operation = query.operation
failed.failure = failed.close_error = None
failed.terminal_phase = query.terminal_phase
failed.identity = query.identity
failed.identity_gate = query.identity_gate
failed.echo = query.echo
try: c.Session.read_after_query(config, profile, failed)
except Exception: pass
else: raise AssertionError('forged proof accepted')
for action in (lambda: c.Outcome(), lambda: pickle.dumps(query)):
    try: action()
    except TypeError: pass
    else: raise AssertionError('proof restoration exposed')
",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}

#[test]
fn runtime_transport_rejects_invalid_provider_results() {
    Python::initialize();
    Python::attach(|py| {
        let locals = runtime_module(py);
        py.run(
            c"config = c.SessionConfig()
config.prepare_delay_ms = 0
config.poll_ms = 1
config.timeout_ms = 10
for method, callback, expected in (
    ('read', lambda self, maximum: b'xx', ValueError),
    ('read', lambda self, maximum: bytearray(), TypeError),
    ('write', lambda self, data: len(data) + 1, ValueError),
    ('write', lambda self, data: True, TypeError),
    ('write', lambda self, data: -1, OverflowError),
    ('now_ms', lambda self: True, TypeError),
    ('now_ms', lambda self: -1, OverflowError),
    ('now_ms', lambda self: 1 << 64, OverflowError),
    ('wait_ms', lambda self, ms: 1, TypeError),
    ('pending_output', lambda self: True, TypeError),
    ('close', lambda self: 1, TypeError),
):
    calls = []
    def invalid(self, *args):
        calls.append(method)
        return callback(self, *args)
    t = type('InvalidProvider', (ProbeTransport,), {method: invalid})()
    try: c.Session.query(config).run(t, lambda: False)
    except expected: pass
    else: raise AssertionError(method)
    assert calls == [method]
    assert t.closes == (0 if method == 'close' else 1)
t = ProbeTransport()
try: c.Session.query(config).run(t, lambda: 1)
except TypeError: pass
else: raise AssertionError('integer cancellation accepted')
assert t.closes == 1 and not t.wire
class Empty(ProbeTransport):
    def read(self, maximum): return b''
t = Empty()
result = c.Session.query(config).run(t, lambda: False)
assert result.failure.kind == 'Timeout' and t.closes == 1
class ZeroWrite(ProbeTransport):
    def write(self, data): return 0
t = ZeroWrite()
result = c.Session.query(config).run(t, lambda: False)
assert result.failure.error_kind is c.ErrorKind.WriteZero and t.closes == 1
t = ProbeTransport()
result = c.Session.query(config).run(t, lambda: True)
assert result.failure.kind == 'Cancelled' and t.closes == 1 and not t.wire
class BadReconfigure(ProbeTransport):
    def reconfigure(self, token): return 1
config.startup_script = b'(9600,N,8,1)'
t = BadReconfigure()
try: c.Session.query(config).run(t, lambda: False)
except TypeError: pass
else: raise AssertionError('non-None reconfigure accepted')
assert t.closes == 1 and not t.wire
",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}

#[test]
fn runtime_nested_owner_views_survive_their_own_field_rebuild() {
    Python::initialize();
    Python::attach(|py| {
        let locals = runtime_module(py);
        py.run(
            c"s = c.search
context = s.SearchContext()
context.results = [s.SearchRecord(1, 2, [s.Slot(b'old', 3)])]
record = context.results[0]
old = record.slots[0]
alias = context.results[0].slots[0]
record.slots = [s.Slot(b'new', 4)]
assert record.wave_in == 1 and record.slots[0].number == 4
for stale in (old, alias):
    try: stale.number
    except ReferenceError: pass
    else: raise AssertionError('old element retargeted')
record.slots = [s.Slot(b'newer', 5)]
assert record.slots[0].number == context.results[0].slots[0].number == 5
context.results = [s.SearchRecord(6, 7, [])]
try: record.wave_in
except ReferenceError: pass
else: raise AssertionError('replaced record retargeted')
",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}

#[test]
fn runtime_python_surface_matches_facades_stubs_and_native_signatures() {
    Python::initialize();
    Python::attach(|py| {
        let locals = runtime_module(py);
        let root = py.import("cs705").unwrap();
        diagnostic::register(&root).unwrap();
        let bridge = PyModule::new(py, "cs705._native").unwrap();
        bridge.add("clone", root.getattr("clone").unwrap()).unwrap();
        bridge
            .add("diagnostic", root.getattr("diagnostic").unwrap())
            .unwrap();
        py.import("sys")
            .unwrap()
            .getattr("modules")
            .unwrap()
            .set_item("cs705._native", &bridge)
            .unwrap();
        locals.set_item("native", bridge).unwrap();
        locals
            .set_item("IoErrorData", py.get_type::<error_values::IoErrorData>())
            .unwrap();
        let facades = PyDict::new(py);
        let stubs = PyDict::new(py);
        for (name, facade, stub) in [
            (
                "cs705.clone",
                include_str!("../python/cs705/clone/__init__.py"),
                include_str!("../python/cs705/clone/__init__.pyi"),
            ),
            (
                "cs705.clone.search",
                include_str!("../python/cs705/clone/search.py"),
                include_str!("../python/cs705/clone/search.pyi"),
            ),
            (
                "cs705.clone.information",
                include_str!("../python/cs705/clone/information.py"),
                include_str!("../python/cs705/clone/information.pyi"),
            ),
            (
                "cs705.diagnostic",
                include_str!("../python/cs705/diagnostic.py"),
                include_str!("../python/cs705/diagnostic.pyi"),
            ),
        ] {
            facades.set_item(name, facade).unwrap();
            stubs.set_item(name, stub).unwrap();
        }
        locals.set_item("facades", facades).unwrap();
        locals.set_item("stubs", stubs).unwrap();
        py.run(
            c"import ast, enum, inspect, sys, types
sys.modules['cs705'].__path__ = []
native_modules = {
    'cs705.clone': native.clone,
    'cs705.clone.search': native.clone.search,
    'cs705.clone.information': native.clone.information,
    'cs705.diagnostic': native.diagnostic,
}
for name, source in facades.items():
    module = types.ModuleType(name)
    parent, _, leaf = name.rpartition('.')
    module.__package__ = name if name == 'cs705.clone' else parent
    if name == 'cs705.clone': module.__path__ = []
    sys.modules[name] = module
    setattr(sys.modules[parent], leaf, module)
    exec(compile(source, name, 'exec'), module.__dict__)
    for exported in module.__all__:
        if exported in ('Transport', 'search', 'information'): continue
        assert getattr(module, exported) is getattr(native_modules[name], exported)
def check_signature(node, value):
    expected = [(a.arg, inspect.Parameter.POSITIONAL_ONLY) for a in node.args.posonlyargs]
    expected += [(a.arg, inspect.Parameter.POSITIONAL_OR_KEYWORD) for a in node.args.args]
    expected += [(a.arg, inspect.Parameter.KEYWORD_ONLY) for a in node.args.kwonlyargs]
    if expected and expected[0][0] == 'self': expected.pop(0)
    actual = list(inspect.signature(value).parameters.values())
    if actual and actual[0].name == 'self': actual.pop(0)
    assert expected == [(p.name, p.kind) for p in actual], (node.name, expected, actual)
    assert not node.args.defaults and not node.args.kw_defaults
    assert all(p.default is inspect.Parameter.empty for p in actual)
for name, source in stubs.items():
    module = sys.modules[name]
    namespace = {'Enum': enum.Enum}
    for node in ast.parse(source).body:
        if isinstance(node, ast.Assign) and isinstance(node.value, ast.Call):
            if isinstance(node.value.func, ast.Name) and node.value.func.id == 'Enum':
                exec(compile(ast.Module(body=[node], type_ignores=[]), name, 'exec'), namespace)
                declared = namespace[node.targets[0].id]
                actual = getattr(module, node.targets[0].id)
                assert [(n, e.value) for n, e in declared.__members__.items()] == [(n, e.value) for n, e in actual.__members__.items()]
        elif isinstance(node, ast.ClassDef):
            actual = getattr(module, node.name)
            if any(isinstance(b, ast.Name) and b.id == 'Enum' for b in node.bases):
                exec(compile(ast.Module(body=[node], type_ignores=[]), name, 'exec'), namespace)
                assert [(n, e.value) for n, e in namespace[node.name].__members__.items()] == [(n, e.value) for n, e in actual.__members__.items()]
                assert actual.__module__ == name
            else:
                for method in node.body:
                    if not isinstance(method, ast.FunctionDef): continue
                    if method.name == '__init__': check_signature(method, actual)
                    elif not method.name.startswith('__') and not any(isinstance(d, ast.Attribute) or isinstance(d, ast.Name) and d.id == 'property' for d in method.decorator_list):
                        check_signature(method, getattr(actual, method.name))
        elif isinstance(node, ast.FunctionDef): check_signature(node, getattr(module, node.name))
assert sys.modules['cs705.clone'].Echo['None'].value == 0
assert 'monotonic' in sys.modules['cs705.clone'].Transport.__doc__
assert 'one-shot' in c.Session.__doc__ and 'close once' in c.Session.run.__doc__
assert 'snapshot' in c.search.SearchRecord.__doc__
assert 'original error text' in native.diagnostic.Diagnostic.__doc__
assert 'copies' in sys.modules['cs705'].MutableBytes.__doc__
assert 'real bool' in sys.modules['cs705'].Bool.__doc__
assert 'Shared I/O error payload' in IoErrorData.__doc__
assert 'OSError object' in IoErrorData.from_os_error.__doc__
assert 'same Python' in IoErrorData.exception.__doc__
assert 'not rolled back' in sys.modules['cs705'].Error.__doc__
source = bytearray(b'ab')
owned = sys.modules['cs705'].MutableBytes(source)
source[0] = 0
assert owned.to_bytes() == b'ab'
try: c.search.SearchRecord(0, 0, iter(()))
except TypeError: pass
else: raise AssertionError('Vec input unexpectedly accepted an iterator')
assert c.search.SearchRecord(0, 0, ()).slots == []
assert c.ContextFields(iter(range(12))).values == list(range(12))
",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}

#[test]
fn runtime_document_loans_allow_checked_nested_native_callbacks() {
    use icf::document::{Document, adapters::attach_checked};

    Python::initialize();
    Python::attach(|py| {
        let locals = PyDict::new(py);
        py.run(
            c"saved = []
primary = ValueError('nested callback failure')
def edit(child):
    child.begin_pending_name(7, b'kept')
    saved.append(child)
    try: outer.current()
    except RuntimeError: pass
    else: raise AssertionError('outer exclusive reservation lost')
def fail(child):
    edit(child)
    child.begin_pending_name(8, b'prefix')
    raise primary
def readonly(child):
    assert child.pending_matches(8) and outer.pending_matches(8)
    for value in (child, outer):
        try: value.reset_pending_name()
        except RuntimeError: pass
        else: raise AssertionError('shared reborrow upgraded')
    saved.append(child)
",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
        let interpreter = scoped::interpreter(py).unwrap();
        let mut native = cs705_core::icf::document::Document::from_current(vec![1], vec![], 6);
        scoped::Frame::with_mut(py, &mut native, |outer| {
            locals.set_item("outer", Py::new(py, Document::from_loan(outer.clone()))?)?;
            let invoke = |name| {
                let callback = locals.get_item(name)?.unwrap().unbind();
                outer.with_mut(py, |document| {
                    py.detach(move || {
                        attach_checked(interpreter, |py| {
                            scoped::Frame::with_mut(py, document, |child| {
                                callback
                                    .bind(py)
                                    .call1((Py::new(py, Document::from_loan(child))?,))?;
                                Ok(())
                            })
                        })
                    })
                })
            };
            invoke("edit")?;
            assert!(outer.with_ref(py, |value| Ok(value.pending_matches(7)))?);
            let error = invoke("fail").unwrap_err();
            assert!(error.value(py).is(&locals.get_item("primary")?.unwrap()));
            assert!(error.traceback(py).is_some());
            assert!(outer.with_ref(py, |value| Ok(value.pending_matches(8)))?);
            let callback = locals.get_item("readonly")?.unwrap().unbind();
            outer.with_ref(py, |document| {
                py.detach(move || {
                    attach_checked(interpreter, |py| {
                        scoped::Frame::with_ref(py, document, |child| {
                            callback
                                .bind(py)
                                .call1((Py::new(py, Document::from_loan(child))?,))?;
                            Ok(())
                        })
                    })
                })
            })?;
            py.run(
                c"for child in saved:
    try: child.current()
    except ReferenceError: pass
    else: raise AssertionError('nested frame survived native callback')
outer.reset_pending_name()
",
                Some(&locals),
                Some(&locals),
            )
        })
        .unwrap();
        assert!(native.pending_name().is_none());
    });
}

#[test]
fn runtime_receiver_reborrows_scoped_owners_without_consuming_them() {
    use cs705_core::icf::document as core;
    use icf::document::{Document, Session, lifecycle::RecentFiles};

    Python::initialize();
    Python::attach(|py| {
        let locals = runtime_module(py);
        for (name, class) in [
            ("Settings", py.get_type::<display::Settings>()),
            (
                "MenuDescriptor",
                py.get_type::<icf::document::menu::MenuDescriptor>(),
            ),
            ("MenuQuery", py.get_type::<icf::document::menu::MenuQuery>()),
            ("MenuItem", py.get_type::<icf::document::menu::MenuItem>()),
            (
                "InitialState",
                py.get_type::<icf::document::initial::InitialState>(),
            ),
            ("UiState", py.get_type::<icf::document::ui::UiState>()),
        ] {
            locals.set_item(name, class).unwrap();
        }
        let mut document = core::Document::from_current(vec![1], b"keep".to_vec(), 6);
        let mut session = core::Session::new(
            cs705_core::icf::context::Context::before_registry(),
            None,
            0,
            [0, 0],
        );
        let recent = core::lifecycle::RecentFiles::default();
        scoped::Frame::with_mut(py, &mut document, |d| {
            scoped::Frame::with_mut(py, &mut session, |s| {
                scoped::Frame::with_ref(py, &recent, |r| {
                    locals.set_item("d", Py::new(py, Document::from_loan(d.clone()))?)?;
                    locals.set_item("s", Py::new(py, Session::from_loan(s))?)?;
                    locals.set_item("r", Py::new(py, RecentFiles::from_loan(r))?)?;
                    py.run(
                        c"initial = InitialState()
ui = UiState.headless()
ui.set_display_settings(Settings(0, 0, 100, 0, 0, 0, 0, 0, 0, 0))
receiver = c.information.DocumentReceiver(d, s, initial, ui, r)
controller = c.information.Controller(1, b'explicit', bytes([6, 1, 0]))
other = c.information.Controller(2, b'explicit', bytes([6, 1, 0]))
primary = ValueError('receiver callback')
saved = []
fail = False
def callback(context, descriptor):
    context.document.begin_pending_name(7, b'prefix')
    context.session.previous_reload_guard += 1
    assert context.recent.paths() == r.paths() == []
    try: context.recent.insert(b'forbidden', None)
    except RuntimeError: pass
    else: raise AssertionError('readonly recent upgraded')
    for action in (d.current, lambda: s.previous_reload_guard,
                   lambda: other.request_reidentify_document([True], receiver),
                   lambda: setattr(receiver, 'document', d)):
        try: action()
        except RuntimeError: pass
        else: raise AssertionError('busy alias accepted')
    saved.extend((context.document, context.session, context.recent))
    descriptor.caption = b'prefix'
    if fail: raise primary
ui.set_menu([MenuItem(MenuDescriptor(0, 0, 0, b'explicit', b'', 0, 0, 0),
                      MenuQuery(callback), False, False, False, True, 0, b'')])
returns = [c.information.NativeReturn.Returned(0)] * 3
controller.form = c.information.Life.Open
controller.close_com_document(False, returns, receiver)
assert d.pending_matches(7) and s.previous_reload_guard == 1
assert controller.form is c.information.Life.ReleaseRequested
fail = True
controller.form = c.information.Life.Open
try: controller.close_com_document(False, returns, receiver)
except ValueError as error:
    assert error is primary and error.__traceback__ is not None
else: raise AssertionError('callback exception lost')
assert d.pending_matches(7) and s.previous_reload_guard == 2
assert controller.form is c.information.Life.Open
assert ui.application5b == 1 and initial.saved_selector == 0 and r.paths() == []
for action in (saved[0].current, lambda: saved[1].previous_reload_guard, saved[2].paths):
    try: action()
    except ReferenceError: pass
    else: raise AssertionError('nested receiver loan survived')
",
                        Some(&locals), Some(&locals),
                    )?;
                    d.with_ref(py, |document| {
                        scoped::Frame::with_ref(py, document, |readonly| {
                            locals.set_item("readonly", Py::new(py, Document::from_loan(readonly))?)?;
                            py.run(
                                c"readonly_receiver = c.information.DocumentReceiver(readonly, s, initial, ui, r)
try: other.request_reidentify_document([True], readonly_receiver)
except RuntimeError: pass
else: raise AssertionError('readonly document upgraded')
",
                                Some(&locals), Some(&locals),
                            )
                        })
                    })
                })
            })
        }).unwrap();
        assert_eq!(session.previous_reload_guard, 2);
        assert_eq!(document.pending_name().unwrap().name, b"prefix");
        py.run(
            c"try: other.request_reidentify_document([True], receiver)
except ReferenceError: pass
else: raise AssertionError('expired receiver target used')
",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}
