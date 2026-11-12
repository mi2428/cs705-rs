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

use cs705_core::icf::document as core;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};

// These embedding tests install canonical module paths in one interpreter.
static PUBLIC_MODULES: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn ready_document() -> (core::Document, core::Session, Vec<u8>) {
    // Synthetic source fields only; no captured image or application asset.
    let mut fields = vec![0; 0x3c9c0];
    fields[0x355a1] = 6;
    for group in 0..100 {
        let at = 0x355c0 + 19 * group;
        fields[at] = if group == 99 { 255 } else { (group + 1) as u8 };
        fields[at + 1..at + 3].fill(255);
        fields[at + 3..at + 19].fill(b' ');
    }
    for range in [
        0x35d2c..0x36628,
        0x3ab82..0x3bf6e,
        0x3c0a7..0x3c2ff,
        0x3c385..0x3c613,
        0x3c8b9..0x3c8bb,
        0xa60b..0xa60f,
    ] {
        fields[range].fill(255);
    }
    fields[0x3662d..0x36821].fill(1);
    for row in 0..45 {
        let at = cs705_core::icf::special::CompactKind::BandStack
            .offset(row)
            .unwrap();
        fields[at..at + 4].fill(255);
    }
    for row in 0..=10 {
        fields[0x36f54 + 36 * row..0x36f58 + 36 * row].fill(255);
    }
    fields[0x3c84b] = 255;
    fields[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
    let mut context = cs705_core::icf::context::Context::before_registry();
    assert!(context.decode_packed(0x40001));
    let mut session = core::Session::new(context, None, 1, [1, 0]);
    let mut document = core::Document::from_current(
        vec![0; cs705_core::clone::IMAGE_CAPACITY],
        b"OLD".to_vec(),
        6,
    );
    session
        .regenerate_defaults(
            core::DefaultSource {
                selector: 1,
                prefix: &fields,
                japanese_mode_pairs: None,
                restricted_band_stack: None,
            },
            document.current(),
        )
        .unwrap();
    document.new_file(&session, 7).unwrap();
    document.accept_saved(b"OLD".to_vec(), 6);
    (document, session, fields)
}

fn environment(py: Python<'_>) -> Bound<'_, PyDict> {
    let root = PyModule::new(py, "cs705").unwrap();
    errors::register(&root).unwrap();
    conversion::register(&root).unwrap();
    icf::register(&root).unwrap();
    display::register(&root).unwrap();
    csv::register(&root).unwrap();
    gps::register(&root).unwrap();
    repeater::register(&root).unwrap();
    clone::register(&root).unwrap();
    presentation::register(&root).unwrap();
    error_values::register(&root).unwrap();
    let locals = PyDict::new(py);
    let image = root.getattr("icf").unwrap();
    let document = image.getattr("document").unwrap();
    let lifecycle = document.getattr("lifecycle").unwrap();
    let modules = py.import("sys").unwrap().getattr("modules").unwrap();
    for (path, module) in [
        ("cs705", root.clone().into_any()),
        ("cs705.icf", image.clone()),
        ("cs705.icf.document", document.clone()),
        ("cs705.icf.document.lifecycle", lifecycle.clone()),
        (
            "cs705.icf.document.options",
            document.getattr("options").unwrap(),
        ),
        (
            "cs705.icf.document.initial",
            document.getattr("initial").unwrap(),
        ),
        (
            "cs705.icf.document.native_read",
            document.getattr("native_read").unwrap(),
        ),
        ("cs705.icf.frequency", image.getattr("frequency").unwrap()),
        ("cs705.clone", root.getattr("clone").unwrap()),
        (
            "cs705.csv.control",
            root.getattr("csv").unwrap().getattr("control").unwrap(),
        ),
    ] {
        modules.set_item(path, module).unwrap();
    }
    let (doc, session, fields) = ready_document();
    locals
        .set_item("c", root.getattr("clone").unwrap())
        .unwrap();
    locals.set_item("d", document).unwrap();
    locals.set_item("l", lifecycle).unwrap();
    locals
        .set_item(
            "doc",
            Py::new(py, icf::document::Document::from_core(doc)).unwrap(),
        )
        .unwrap();
    locals
        .set_item(
            "session",
            Py::new(py, icf::document::Session::from_core(session)).unwrap(),
        )
        .unwrap();
    locals
        .set_item("Bool", root.getattr("Bool").unwrap())
        .unwrap();
    locals
        .set_item(
            "default_source",
            Py::new(
                py,
                icf::document::values::DefaultSource {
                    selector: 1,
                    prefix: fields,
                    japanese_mode_pairs: None,
                    restricted_band_stack: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
    locals
        .set_item(
            "reset",
            Py::new(
                py,
                icf::normalizer::ResetState::from_core(cs705_core::icf::normalizer::ResetState {
                    show_both: 0,
                    home_invalidated: 0,
                }),
            )
            .unwrap(),
        )
        .unwrap();
    locals
        .set_item(
            "gate",
            root.getattr("csv")
                .unwrap()
                .getattr("control")
                .unwrap()
                .getattr("ImportGate")
                .unwrap(),
        )
        .unwrap();
    locals
        .set_item(
            "settings",
            Py::new(
                py,
                display::Settings::from_core(cs705_core::display::Settings {
                    language: 0,
                    system_language: 0,
                    font_percent: 100,
                    toolbar: 1,
                    split: 0,
                    callsign_dialog: 0,
                    auto_correct: 0,
                    fixed_name: 0,
                    column_bits: 0,
                    column_bits_high: 0,
                }),
            )
            .unwrap(),
        )
        .unwrap();
    locals
}

#[test]
fn shared_document_and_session_views_keep_one_borrow_authority() {
    use icf::document::{Document, Session};
    Python::initialize();
    Python::attach(|py| {
        let (document, session, _) = ready_document();
        let document = Document::from_core(document);
        let session = Session::from_core(session);
        let alias = document.clone();
        let shared = session.clone();
        let context = shared.context_view();
        let ranges = shared.ranges_view();
        session
            .with_mut(|session| {
                session.document_flags = [7, 9];
                assert!(context.with_ref(|_| Ok(())).is_err());
                assert!(ranges.with_mut(|_| Ok(())).is_err());
                Ok(())
            })
            .unwrap();
        assert_eq!(shared.with_ref(|s| Ok(s.document_flags)).unwrap(), [7, 9]);
        let retained = document
            .with_mut(|document| {
                assert!(alias.with_ref(|_| Ok(())).is_err());
                scoped::Frame::with_mut(py, document, |loan| {
                    let view = Document::from_loan(loan);
                    let view_alias = view.clone();
                    view.with_mut(|document| {
                        assert!(view_alias.with_ref(|_| Ok(())).is_err());
                        session.with_ref(|session| {
                            document.normalize(session).map(|_| ()).map_err(errors::icf)
                        })
                    })?;
                    Ok(view_alias)
                })
            })
            .unwrap();
        assert!(
            retained
                .with_ref(|_| Ok(()))
                .unwrap_err()
                .is_instance_of::<pyo3::exceptions::PyReferenceError>(py)
        );
        session
            .with_ref(|session| {
                scoped::Frame::with_ref(py, session, |loan| {
                    let readonly = Session::from_loan(loan);
                    assert!(readonly.context_view().with_mut(|_| Ok(())).is_err());
                    assert!(readonly.ranges_view().with_mut(|_| Ok(())).is_err());
                    assert!(readonly.take().is_err());
                    Ok(())
                })
            })
            .unwrap();
    });
}

#[test]
fn document_boundary_contracts() {
    let _modules = PUBLIC_MODULES.lock().unwrap();
    Python::initialize();
    Python::attach(|py| {
        let locals = environment(py);
        py.run(c"
def rejects(kind, call):
    try: call()
    except kind: return
    raise AssertionError('operation unexpectedly succeeded')

other = d.Document.from_current(doc.current(), b'OTHER', 6)
alias = doc.share()
shared = session.share()
q = shared.context
ranges = shared.ranges
q.decode_packed(0x80001)
assert session.context.encode_packed() == 0x80001
assert other.path() == b'OTHER'
q.decode_packed(0x40001)
before_ranges = ranges.source_q()
q.decode_packed(0x80001)
assert ranges.source_q() == before_ranges
q.decode_packed(0x40001)

recent = l.RecentFiles()
ui = l.UiState.headless()
ui.set_display_settings(settings)
life = l.Lifecycle(recent, ui)
receiver_ui = l.UiState.headless()
receiver = l.Lifecycle(recent.share(), receiver_ui)
saved = []
primary = ValueError('menu primary')
def query(context, descriptor):
    saved.extend((context.document.share(), context.session.context, context.settings, context.recent, descriptor))
    context.session.regenerate_defaults(default_source, context.document.current())
    context.document.reset_operating_state(context.session, reset)
    file = context.document.current_bytes(context.session)
    assert context.document.read(file, b'OLD', 7, None, context.session, None).kind == 'Opened'
    receiver.initial_boundary(d.InitialBoundary.Title, context.document, context.session)
    receiver_ui.initial_boundary(d.InitialBoundary.Reconfigure, context.document, context.session)
    context.document.patch_current([(0x37158, b'a')])
    context.session.document_flags = b'\x03\x05'
    descriptor.caption = b'PREFIX'
    rejects(RuntimeError, alias.current)
    rejects(RuntimeError, lambda: shared.document_flags)
    rejects(RuntimeError, lambda: setattr(context.settings, 'toolbar', 0))
    other.begin_pending_name(4, b'UNRELATED')
    context.document.normalize(context.session)
    raise primary
query_object = l.MenuQuery(query)
item = l.MenuItem(l.MenuDescriptor(0,0,0,b'NAME',b'',0,0,0), query_object, False,False,False,True,0,b'')
ui.set_menu([item])
try: life.sync02(doc, session)
except ValueError as error:
    assert error is primary
    import traceback
    assert traceback.extract_tb(error.__traceback__)[-1].name == 'query'
else: raise AssertionError('menu failure was hidden')
assert alias.current()[0x37158] == ord('A')
assert session.document_flags == b'\x03\x05'
assert ui.menu[0].descriptor.caption == b'PREFIX'
assert receiver_ui.title.path == b'OLD'
for view, call in zip(saved, (lambda v:v.current(), lambda v:v.as_bytes(), lambda v:v.toolbar, lambda v:v.paths(), lambda v:v.caption)):
    rejects(ReferenceError, lambda: call(view))
", Some(&locals), Some(&locals)).unwrap();
        py.run(
            c"
ui.set_menu([])
baseline = doc.baseline()
doc.patch_current([(0x37158, b'b')])
assert doc.compare_dirty() == 1
doc.patch_current([(0x37158, baseline[0x37158:0x37159])])
assert doc.compare_dirty() == 1
doc.patch_current([(0x37158, b'c')])
primary = InterruptedError('terminal writer failure')
class Writer:
    def __init__(self): self.calls = 0
    def write(self, data):
        self.calls += 1
        rejects(RuntimeError, alias.current)
        rejects(RuntimeError, lambda: shared.defaults())
        assert data
        raise primary
    def flush(self): raise AssertionError('unsolicited flush')
writer = Writer()
try: doc.save(session, b'NEW', 5, 6, writer)
except InterruptedError as error:
    assert error is primary
    assert error.__traceback__ is not None
else: raise AssertionError('writer failure was hidden')
assert writer.calls == 1
assert doc.current()[0x37158] == ord('C')
assert doc.path() == b'OLD' and doc.baseline() == baseline
assert doc.dirty() == 1
assert other.path() == b'OTHER'

class Compare:
    def compare(self, left, right): return 2 if left == right else 1
compare = Compare()
recent.insert(b'OLD', compare)
events = []
primary = OSError('publication failed')
class Files:
    def publish(self, path, data):
        events.append('publish')
        assert path == b'OLD' and data
        raise primary
    def read(self, path):
        events.append('read')
        raise AssertionError('read after cancelled prompt')
class Dialogs:
    def choice(self, prompt):
        events.append(prompt.kind)
        return 6
    def save_as(self, initial): raise AssertionError('unexpected dialog')
class Placement:
    def placement(self): raise AssertionError('placement after failed save')
options = l.SaveOptions(d.ValidationSource.NoTree)
incoming = Bool(True)
try: life.close(doc, session, incoming, options, Dialogs(), Files(), compare, Placement())
except OSError as error:
    assert error is primary
    outcomes = [e.args[1] for e in error.__context__.exceptions if e.args[0] == 'native outcome']
    assert len(outcomes) == 1 and outcomes[0].result == 2
    assert outcomes[0].save.attempts[0].publication_error is primary
else: raise AssertionError('publication failure was hidden')
assert not incoming.value
assert events == ['Dirty', 'publish']
assert ui.form is not None and ui.task_enabled
assert doc.path() == b'OLD' and doc.baseline() == baseline
assert recent.paths() == [b'OLD']

events.clear()
class Cancel(Dialogs):
    def choice(self, prompt):
        events.append(prompt.kind)
        return 2
prompt, status = life.open(doc, session, b'NEW', None, None, options, Cancel(), Files(), compare)
assert status.kind == 'Cancelled' and prompt.result == 2
assert events == ['Dirty']
assert doc.path() == b'OLD' and doc.baseline() == baseline

primary = ValueError('CSV reader failed')
saved_reader = []
def reader(view):
    saved_reader.append(view)
    view.patch_current([(0x37158, b'd')])
    raise primary
try: doc.import_csv(session, gate.Header(1), reader)
except ValueError as error: assert error is primary
else: raise AssertionError('CSV failure was hidden')
assert doc.current()[0x37158] == ord('d')
rejects(ReferenceError, saved_reader[0].current)
rejects(TypeError, lambda: doc.patch_current([(True, b'x')]))
",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
        py.run(
            c"
primary = ValueError('action failed')
action = d.ActionContext(1, 0, 0, 0x10000)
saved_action = []
before_timer = doc.timer_ms()
def fail_action(view, context, flags):
    saved_action.extend((view, context))
    view.patch_current([(0x37158, b'e')])
    context.phase = 9
    rejects(RuntimeError, lambda: doc.finish_action(action, True, 1, fail_action))
    raise primary
try: doc.finish_action(action, True, 1, fail_action)
except ValueError as error: assert error is primary
else: raise AssertionError('action failure was hidden')
assert doc.current()[0x37158] == ord('e')
assert action.phase == 9 and doc.timer_ms() == before_timer
rejects(ReferenceError, saved_action[0].current)
rejects(ReferenceError, lambda: saved_action[1].phase)

import gc, weakref
def make_cycle():
    owner = l.UiState.headless()
    owner.set_display_settings(settings)
    class Query:
        def __init__(self): self.owner = owner
        def __call__(self, context, descriptor): descriptor.caption = b'GC-LIVE'
    callback = Query()
    query = l.MenuQuery(callback)
    item = l.MenuItem(l.MenuDescriptor(0,0,0,b'',b'',0,0,0), query, False,False,False,True,0,b'')
    owner.set_menu([item])
    return owner.share(), owner.menu[0], weakref.ref(callback)
shared_ui, snapshot, weak = make_cycle()
gc.collect()
assert weak() is not None
shared_ui.sync02(doc, session, recent)
assert shared_ui.menu[0].caption == b'GC-LIVE'
del shared_ui
gc.collect()
assert weak() is not None
query_clone = snapshot.query.clone()
del snapshot
gc.collect()
assert weak() is not None
del query_clone
gc.collect()
assert weak() is None
",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}

#[test]
fn document_python_signatures_match_stubs() {
    let _modules = PUBLIC_MODULES.lock().unwrap();
    Python::initialize();
    Python::attach(|py| {
        let locals = environment(py);
        let sources = PyDict::new(py);
        for (name, source) in [
            (
                "document",
                include_str!("../python/cs705/icf/document/__init__.pyi"),
            ),
            (
                "lifecycle",
                include_str!("../python/cs705/icf/document/lifecycle.pyi"),
            ),
            (
                "initial",
                include_str!("../python/cs705/icf/document/initial.pyi"),
            ),
            (
                "native_read",
                include_str!("../python/cs705/icf/document/native_read.pyi"),
            ),
            (
                "options",
                include_str!("../python/cs705/icf/document/options.pyi"),
            ),
        ] {
            sources.set_item(name, source).unwrap();
        }
        locals.set_item("stubs", sources).unwrap();
        locals
            .set_item(
                "lifecycle_facade",
                include_str!("../python/cs705/icf/document/lifecycle.py"),
            )
            .unwrap();
        py.run(c"
import ast, inspect
failures = []
checked = 0
def parameters(arguments, receiver):
    positional = arguments.posonlyargs + arguments.args
    required = len(positional) - len(arguments.defaults)
    result = []
    for index, argument in enumerate(positional):
        default = None if index < required else ast.literal_eval(arguments.defaults[index-required])
        kind = inspect.Parameter.POSITIONAL_ONLY if index < len(arguments.posonlyargs) else inspect.Parameter.POSITIONAL_OR_KEYWORD
        result.append((argument.arg, kind, index >= required, default))
    if receiver: result = result[1:]
    return result
for name, source in stubs.items():
    module = d if name == 'document' else getattr(d, name)
    for node in ast.parse(source).body:
        if isinstance(node, ast.ClassDef):
            target = getattr(module, node.name, None)
            if target is None:
                assert any(ast.unparse(base) == 'Protocol' for base in node.bases), (name, node.name)
                continue
            methods = [m for m in node.body if isinstance(m, ast.FunctionDef)]
        elif isinstance(node, ast.FunctionDef):
            target, methods = module, [node]
        else: continue
        for method in methods:
            decorators = [ast.unparse(v) for v in method.decorator_list]
            if any(v == 'property' or v.endswith('.setter') for v in decorators): continue
            if method.name.startswith('__') and method.name != '__init__': continue
            constructor = method.name == '__init__'
            actual = target if constructor else getattr(target, method.name)
            expected = parameters(method.args, isinstance(node, ast.ClassDef) and 'staticmethod' not in decorators)
            signature = inspect.signature(actual)
            native = list(signature.parameters.values())
            if isinstance(node, ast.ClassDef) and not constructor and 'staticmethod' not in decorators:
                native = native[1:]
            observed = [(p.name, p.kind, p.default is not inspect.Parameter.empty, None if p.default is inspect.Parameter.empty else p.default) for p in native]
            checked += 1
            if expected != observed:
                failures.append(f'{name}.{node.name}.{method.name}: {expected!r} != {observed!r}')
assert checked > 120, checked
assert not failures, '\\n'.join(failures)
assert d.CloneOptions is d.options.CloneOptions
assert d.CloneOptionButton is d.options.CloneOptionButton
assert d.InitialState is d.initial.InitialState
assert d.InitialBoundary is d.initial.InitialBoundary
assert d.CloneRead is d.native_read.CloneRead
assert d.CloneReadStop is d.native_read.CloneReadStop
assert 'opaque bytes' in d.Document.__doc__
assert 'expire' in l.MenuContext.__doc__
assert 'not flushed or closed' in d.Document.save.__doc__
for sequence in ([], ()):
    assert d.ValidationSource.Views(sequence).views == []
    l.UiState.headless().set_menu(sequence)
for construct in (d.ValidationSource.Views, l.UiState.headless().set_menu):
    try: construct(item for item in ())
    except TypeError: pass
    else: raise AssertionError('sequence-only input accepted a generator')
ui = l.UiState.headless()
ui.image_list = (item for item in (7, 9))
assert ui.image_list == (7, 9)
placement = l.Placement(0, 0, (0,0), (0,0), (0,0,0,0))
placement.minimum = (item for item in (2, 3))
assert placement.minimum == (2, 3)
state = d.InitialState()
state.saved_context = session.context.fields()
view = state.saved_context
values = view.values
values[0] = 7
assert view.values[0] != 7
view.values = (value for value in values)
assert view.values[0] == 7
import sys, typing
sys.modules['cs705._native'] = sys.modules['cs705']
facade = {'__name__': 'cs705.icf.document.lifecycle', '__package__': 'cs705.icf.document'}
exec(compile(lifecycle_facade, 'document/lifecycle.py', 'exec'), facade)
assert facade['SaveDialog'] is l.SaveDialog
assert facade['Placement'] is l.Placement
assert typing.get_type_hints(facade['Dialogs'].save_as)['return'] == l.SaveSelection | None
assert typing.get_type_hints(facade['PlacementSource'].placement)['return'] == l.Placement | None
", Some(&locals), Some(&locals)).unwrap();
        let checked: usize = locals
            .get_item("checked")
            .unwrap()
            .unwrap()
            .extract()
            .unwrap();
        eprintln!("checked {checked} document signatures");
    });
}

#[test]
fn document_nested_callbacks_reborrow_scoped_inputs() {
    let _modules = PUBLIC_MODULES.lock().unwrap();
    Python::initialize();
    Python::attach(|py| {
        let locals = environment(py);
        py.run(c"
def rejects(kind, call):
    try: call()
    except kind: return
    raise AssertionError('operation unexpectedly succeeded')
action = d.ActionContext(1, 0, 0, 0x10000)
saved = []
primary = ValueError('nested action')
def outer(view, context, flags):
    def inner(child, child_context, child_flags):
        saved.extend((child, child_context))
        rejects(RuntimeError, view.current)
        rejects(RuntimeError, lambda: context.phase)
        child.patch_current([(0x37158, b'z')])
        child_context.phase = 9
        raise primary
    try: view.finish_action(context, True, flags, inner)
    except ValueError as error: assert error is primary
    else: raise AssertionError('nested exception hidden')
    assert view.current()[0x37158] == ord('z') and context.phase == 9
    rejects(ReferenceError, saved[0].current)
doc.finish_action(action, True, 1, outer)
assert doc.timer_ms() == 250 and action.phase == 9
assert doc.current()[0x37158] == ord('z')
rejects(ReferenceError, lambda: saved[1].phase)

recent = l.RecentFiles()
outer_ui, inner_ui = l.UiState.headless(), l.UiState.headless()
outer_ui.set_display_settings(settings)
inner_ui.set_display_settings(settings)
outer_context = []
outer_descriptors = []
def inner_query(context, descriptor):
    rejects(RuntimeError, outer_context[-1].document.current)
    saved.append(context.document)
    context.document.begin_pending_name(8, b'NESTED')
    descriptor.caption = b'INNER'
    if descriptor.name == b'FAIL':
        rejects(RuntimeError, lambda: outer_descriptors[-1].caption)
        raise primary
def item(callback):
    return l.MenuItem(l.MenuDescriptor(0,0,0,b'',b'',0,0,0), l.MenuQuery(callback), False,False,False,True,0,b'')
inner_ui.set_menu([item(inner_query)])
def outer_query(context, descriptor):
    outer_context.append(context)
    outer_descriptors.append(descriptor)
    nested = l.Lifecycle(context.recent, inner_ui)
    nested.initial_boundary(d.InitialBoundary.Sync(2), context.document, context.session)
    assert context.document.pending_matches(8)
    rejects(ReferenceError, saved[-1].current)
    query = l.MenuQuery(inner_query)
    descriptor.name = b'FAIL'
    try: query.call(context, descriptor)
    except ValueError as error: assert error is primary
    else: raise AssertionError('nested menu failure hidden')
    assert descriptor.caption == b'INNER'
    assert context.document.pending_matches(8)
    rejects(ReferenceError, saved[-1].current)
outer_ui.set_menu([item(outer_query)])
outer_ui.sync02(doc, session, recent)
assert inner_ui.menu[0].caption == b'INNER'
assert doc.pending_matches(8)
", Some(&locals), Some(&locals)).unwrap();
    });
}

#[test]
fn document_nested_io_options_and_initial_boundaries_keep_prefixes() {
    let _modules = PUBLIC_MODULES.lock().unwrap();
    Python::initialize();
    Python::attach(|py| {
        let locals = environment(py);
        py.run(c"
def rejects(kind, call):
    try: call()
    except kind: return
    raise AssertionError('operation unexpectedly succeeded')
primary = ValueError('nested producer')
saved = []
events = []
def outer(view, context, flags):
    def reader(child):
        saved.append(child)
        rejects(RuntimeError, view.current)
        child.patch_current([(0x37158, b'a')])
        rejects(RuntimeError, lambda: setattr(session_view, 'document_flags', bytes(2)))
        raise primary
    def query(menu, descriptor):
        nonlocal session_view
        session_view = menu.session
        try: menu.document.import_csv(menu.session, gate.Header(1), reader)
        except ValueError as error: assert error is primary
        else: raise AssertionError('reader failure hidden')
        assert menu.document.current()[0x37158] == ord('a')
        rejects(ReferenceError, saved[-1].current)
        baseline = menu.document.baseline()
        class Writer:
            def __init__(self): self.prefix = b''
            def write(self, data):
                rejects(RuntimeError, menu.document.current)
                if not self.prefix:
                    self.prefix = data[:7]
                    return 7
                raise primary
        for save in (
            lambda writer: menu.document.save(menu.session, b'NEW', 6, 0, writer),
            lambda writer: menu.document.save_with_validation(menu.session, d.SaveRequest(b'NEW',6,0,None,d.ValidationSource.NoTree), None, writer),
        ):
            writer = Writer()
            try: save(writer)
            except ValueError as error: assert error is primary
            else: raise AssertionError('writer failure hidden')
            assert len(writer.prefix) == 7
            assert menu.document.path() == b'OLD' and menu.document.baseline() == baseline
        assert menu.document.current()[0x37158] == ord('A')
        class Store:
            def write(self, index, path):
                events.append(index)
                rejects(RuntimeError, lambda: menu.recent.insert(b'NO', None))
        menu.recent.persist(Store())
        assert events == list(range(10))
        class Compare:
            def compare(self, left, right):
                rejects(RuntimeError, menu.document.current)
                return 1
        recent = l.RecentFiles()
        recent.insert(b'PRIOR', Compare())
        receiver = l.UiState.headless()
        life = l.Lifecycle(recent, receiver)
        class Dialogs:
            def choice(self, prompt): raise AssertionError('unexpected prompt')
            def save_as(self, initial): raise AssertionError('unexpected dialog')
        class Files:
            def publish(self, path, data):
                rejects(RuntimeError, menu.document.current)
                rejects(RuntimeError, lambda: menu.session.document_flags)
                assert path == b'OLD' and data
        flow = life.save(menu.document, menu.session, l.SaveMode.Save, l.SaveOptions(d.ValidationSource.NoTree), Dialogs(), Files(), Compare())
        assert flow.outcome is d.SaveOutcome.Saved
        assert recent.paths()[0] == b'OLD'
        assert menu.document.baseline() == menu.document.current()
        source = menu.document.current_bytes(menu.session)
        def boundary(event, child, shared):
            saved.extend((child, shared))
            rejects(RuntimeError, menu.document.current)
            rejects(RuntimeError, lambda: menu.session.document_flags)
            child.patch_current([(0x37158, b'i')])
            raise primary
        try: menu.document.initial_from_icf(source, b'NEW', menu.session, default_source, boundary)
        except ValueError as error: assert error is primary
        else: raise AssertionError('initial failure hidden')
        assert menu.document.current()[0x37158] == ord('i')
        rejects(ReferenceError, saved[-2].current)
        tail = bytearray(33)
        tail[0:2] = bytes((33,6))
        tail[19] = 8
        identity = c.Identity(0x4075, 1, bytes(tail))
        state = d.InitialState()
        try: menu.document.receive_initial_identity(menu.session, state, identity, d.InitialRequest(2,0,0), default_source, boundary)
        except ValueError as error: assert error is primary
        else: raise AssertionError('identity failure hidden')
        assert menu.document.current()[0x37158] == ord('i')
        rejects(ReferenceError, saved[-2].current)
    session_view = None
    query_object = l.MenuQuery(query)
    descriptor = l.MenuDescriptor(0,0,0,b'',b'',0,0,0)
    query_object.call(l.MenuContext(view, session, settings, None, l.RecentFiles()), descriptor)
action = d.ActionContext(1,0,0,0x10000)
doc.finish_action(action, True, 1, outer)
assert doc.current()[0x37158] == ord('i')

options = d.CloneOptions.open(0)
def first(index, view):
    if index: return 0
    def second(child_index, child):
        rejects(RuntimeError, lambda: view.word)
        saved.append(child)
        child.word = 123
        raise primary
    try: view.save(d.CloneOptionButton.Ok, second)
    except ValueError as error: assert error is primary
    else: raise AssertionError('option failure hidden')
    assert view.word == 123
    rejects(ReferenceError, lambda: saved[-1].word)
    return 1
options.save(d.CloneOptionButton.Ok, first)
", Some(&locals), Some(&locals)).unwrap();
    });
}

#[test]
fn mutable_recent_loan_preserves_deduplication_before_callback_failure() {
    let _modules = PUBLIC_MODULES.lock().unwrap();
    Python::initialize();
    Python::attach(|py| {
        let locals = environment(py);
        let mut recent = core::lifecycle::RecentFiles::default();
        let expired = scoped::Frame::with_mut(py, &mut recent, |loan| {
            let view = Py::new(py, icf::document::lifecycle::RecentFiles::from_loan(loan))?;
            locals.set_item("recent", &view)?;
            py.run(
                c"
primary = ValueError('deduplication')
class Compare:
    def compare(self, left, right):
        try: recent.paths()
        except RuntimeError: pass
        else: raise AssertionError('old loan escaped reservation')
        return 1
recent.insert(b'A', Compare())
recent.insert(b'B', Compare())
class Fail(Compare):
    def __init__(self): self.calls = 0
    def compare(self, left, right):
        super().compare(left, right)
        self.calls += 1
        if self.calls == 1: return 2
        raise primary
try: recent.insert(b'C', Fail())
except ValueError as error: assert error is primary
else: raise AssertionError('deduplication failure hidden')
assert recent.paths() == [b'B']
",
                Some(&locals),
                Some(&locals),
            )?;
            Ok(view)
        })
        .unwrap();
        assert_eq!(recent.paths(), &[b"B".to_vec()]);
        assert!(
            expired
                .bind(py)
                .call_method0("paths")
                .unwrap_err()
                .is_instance_of::<pyo3::exceptions::PyReferenceError>(py)
        );
    });
}
