use crate::{csv, icf, repeater, scoped};
use pyo3::prelude::*;
use pyo3::types::PyDict;

#[test]
fn python_scoped_record_equality_checks_expiration() {
    Python::initialize();
    Python::attach(|py| {
        let row = cs705_core::repeater::staging::Record {
            bytes: Box::new([0; 616]),
            frequency_class: 0,
        };
        let saved = scoped::Frame::with_ref(py, &row, |loan| {
            let view = Py::new(py, repeater::staging::Record::from_loan(loan))?;
            assert!(
                view.bind(py)
                    .call_method1("__eq__", (&view,))?
                    .extract::<bool>()?
            );
            let cross_thread = view.clone_ref(py);
            py.detach(move || {
                std::thread::spawn(move || {
                    Python::attach(|py| {
                        let error = cross_thread
                            .bind(py)
                            .call_method1("__eq__", (&cross_thread,))
                            .unwrap_err();
                        assert!(error.is_instance_of::<pyo3::exceptions::PyRuntimeError>(py));
                    })
                })
                .join()
                .unwrap()
            });
            Ok(view)
        })
        .unwrap();
        let error = saved
            .bind(py)
            .call_method1("__eq__", (&saved,))
            .unwrap_err();
        assert!(error.is_instance_of::<pyo3::exceptions::PyReferenceError>(py));
    });
}

#[test]
fn python_dialog_equality_accepts_same_owner_projections() {
    Python::initialize();
    Python::attach(|py| {
        let root = super::package(py);
        let locals = PyDict::new(py);
        locals.set_item("c", root.getattr("csv").unwrap()).unwrap();
        py.run(c"coordinate = c.number.CoordinateDialog.area(bytes(4), bytes(0x396b5), None)\ndialog = c.date_input.DateDialog(coordinate, True)\neffect = c.date_input.Effect(False, None, b'', (0, 0), dialog)\nassert effect.dialog == effect.dialog\nassert effect.dialog.editor == effect.dialog.editor\n", Some(&locals), Some(&locals))
            .inspect_err(|error| error.print(py)).unwrap();
    });
}

#[test]
fn python_native_nls_producers_accept_callback_destinations() {
    Python::initialize();
    Python::attach(|py| {
        let root = super::package(py);
        let locals = PyDict::new(py);
        locals.set_item("c", root.getattr("csv").unwrap()).unwrap();
        locals
            .set_item("MutableBytes", root.getattr("MutableBytes").unwrap())
            .unwrap();
        py.run(c"shared = MutableBytes(b'ab')\nassert c.nls.map_copy_back(shared, c.nls.ascii_map) == 4\nassert shared.to_bytes() == b'ab'\nassert c.nls.map_copy_back(shared, c.nls.platform_map) == 4\nassert c.nls.uppercase_copy(b'ab', c.nls.platform_uppercase) == b'AB'\nassert c.nls.ascii_map(b'xy', shared) == 2\nassert shared.to_bytes() == b'xy'\n", Some(&locals), Some(&locals))
            .inspect_err(|error| error.print(py)).unwrap();
        let nls = root.getattr("csv").unwrap().getattr("nls").unwrap();
        let view = scoped::Frame::with_ref(py, b"ab".as_slice(), |loan| {
            let view = Py::new(py, csv::nls::Bytes::from_loan(loan))?;
            let error = nls
                .getattr("platform_uppercase")?
                .call1((&view,))
                .unwrap_err();
            assert!(error.is_instance_of::<pyo3::exceptions::PyRuntimeError>(py));
            Ok(view)
        })
        .unwrap();
        let error = nls
            .getattr("platform_uppercase")
            .unwrap()
            .call1((view,))
            .unwrap_err();
        assert!(error.is_instance_of::<pyo3::exceptions::PyReferenceError>(py));
    });
}

#[test]
fn owned_cell_cursor_preserves_every_boundary() {
    use cs705_core::csv::Cells;
    let alphabet = [b',', b'x', 0, 0x81, 0x9f, 0xe0, 0xef, 0xf0];
    for size in 0..=4 {
        for mut code in 0..alphabet.len().pow(size) {
            let source: Vec<_> = (0..size)
                .map(|_| {
                    let byte = alphabet[code % alphabet.len()];
                    code /= alphabet.len();
                    byte
                })
                .collect();
            let expected = Cells::new(&source, b',').collect::<Vec<_>>();
            for consumed in 0..=expected.len() + 1 {
                let mut borrowed = Cells::new(&source, b',');
                for _ in 0..consumed {
                    borrowed.next();
                }
                let mut owned = borrowed.into_owned();
                for cell in expected.iter().skip(consumed) {
                    assert_eq!(owned.next_cell(), Some(*cell));
                }
                assert_eq!(owned.next_cell(), None);
                assert_eq!(owned.next_cell(), None);
            }
        }
    }
}

#[test]
fn python_data_boundaries_and_callback_prefixes() {
    let _gc = super::GC_TESTS.lock().unwrap();
    Python::initialize();
    Python::attach(|py| {
        let root = super::package(py);
        let locals = PyDict::new(py);
        locals.set_item("c", root.getattr("csv").unwrap()).unwrap();
        locals.set_item("g", root.getattr("gps").unwrap()).unwrap();
        locals
            .set_item("r", root.getattr("repeater").unwrap())
            .unwrap();
        locals.set_item("i", root.getattr("icf").unwrap()).unwrap();
        locals
            .set_item("MutableBytes", root.getattr("MutableBytes").unwrap())
            .unwrap();
        locals
            .set_item(
                "stub_root",
                concat!(env!("CARGO_MANIFEST_DIR"), "/python/cs705"),
            )
            .unwrap();
        py.run(cr#"
import ast, inspect, pathlib, sys
mismatches, unchecked = [], []
checked = 0
def compare(body, owner, prefix, is_class=False):
    global checked
    for node in body:
        name = getattr(node, 'name', None)
        if isinstance(node, ast.ClassDef):
            compare(node.body, getattr(owner, name), prefix + '.' + name, True)
        elif is_class and isinstance(node, ast.AnnAssign):
            assert hasattr(owner, node.target.id), prefix + '.' + node.target.id
        elif isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
            if isinstance(node.value, ast.Constant) and type(node.value.value) is int:
                assert getattr(owner, node.targets[0].id).value == node.value.value
            elif isinstance(node.value, ast.Name):
                assert getattr(owner, node.targets[0].id) is getattr(owner, node.value.id)
        elif isinstance(node, ast.FunctionDef):
            if name.startswith('__') and name != '__init__': continue
            if any(isinstance(d, ast.Name) and d.id == 'property' or
                   isinstance(d, ast.Attribute) and d.attr in ('setter', 'getter')
                   for d in node.decorator_list):
                assert hasattr(owner, name), prefix + '.' + name
                continue
            target = owner if name == '__init__' else getattr(owner, name)
            path = prefix + '.' + name
            try: parameters = list(inspect.signature(target).parameters.values())
            except (ValueError, TypeError):
                unchecked.append(path)
                continue
            if any(p.kind in (p.VAR_POSITIONAL, p.VAR_KEYWORD) for p in parameters):
                unchecked.append(path)
                continue
            static = any(isinstance(d, ast.Name) and d.id == 'staticmethod' for d in node.decorator_list)
            if parameters and (parameters[0].name in ('self', 'cls') or
                               is_class and name != '__init__' and not static): parameters.pop(0)
            args = node.args.posonlyargs + node.args.args
            defaults = [False] * (len(args) - len(node.args.defaults)) + [True] * len(node.args.defaults)
            expected = [(a.arg, default, inspect.Parameter.POSITIONAL_ONLY if index < len(node.args.posonlyargs)
                         else inspect.Parameter.POSITIONAL_OR_KEYWORD)
                        for index, (a, default) in enumerate(zip(args, defaults))]
            expected.extend((a.arg, default is not None, inspect.Parameter.KEYWORD_ONLY)
                            for a, default in zip(node.args.kwonlyargs, node.args.kw_defaults))
            if is_class and expected and expected[0][0] in ('self', 'cls'): expected.pop(0)
            actual = [(p.name, p.default is not p.empty, p.kind) for p in parameters]
            checked += 1
            if actual != expected: mismatches.append((path, expected, actual))
for domain in ('csv', 'gps', 'repeater'):
    for path in sorted((pathlib.Path(stub_root) / domain).rglob('*.pyi')):
        parts = path.relative_to(stub_root).with_suffix('').parts
        if parts[-1] == '__init__': parts = parts[:-1]
        name = 'cs705.' + '.'.join(parts)
        compare(ast.parse(path.read_text()).body, sys.modules[name], name)
assert checked >= 200, checked
assert not mismatches, mismatches
"#, Some(&locals), Some(&locals)).inspect_err(|error| error.print(py)).unwrap();
        println!(
            "native signature checks: {}; uninspectable: {:?}",
            locals
                .get_item("checked")
                .unwrap()
                .unwrap()
                .extract::<usize>()
                .unwrap(),
            locals
                .get_item("unchecked")
                .unwrap()
                .unwrap()
                .extract::<Vec<String>>()
                .unwrap()
        );
        let mut memory = vec![0; 0x4997a];
        memory[0x49914..0x49916].copy_from_slice(&1_u16.to_le_bytes());
        let mut working = cs705_core::repeater::working::Working::from_working(memory).unwrap();
        let mut holder = cs705_core::icf::record::reset_holder(0x0c, 0, 0);
        holder.set_valid(true);
        holder.set_repeater_field(cs705_core::icf::record::RepeaterField::Mode, 2);
        holder.set_repeater_text(cs705_core::icf::record::RepeaterText::Call, b"TEST   B");
        holder.set_channel_field(cs705_core::icf::record::ChannelField::Duplex, 1);
        holder.set_channel_field(
            cs705_core::icf::record::ChannelField::FrequencyHz,
            150_000_000,
        );
        working.save(0, &holder).unwrap();
        locals
            .set_item(
                "working",
                Py::new(py, repeater::working::Working::from_core(working)).unwrap(),
            )
            .unwrap();
        let mut q = cs705_core::icf::context::Context::before_registry();
        let mut fields = q.fields();
        fields.0[0] = 0;
        q.adopt(fields);
        locals
            .set_item(
                "q",
                Py::new(py, icf::context::Context::from_core(q)).unwrap(),
            )
            .unwrap();
        locals
            .set_item(
                "b",
                Py::new(
                    py,
                    icf::frequency::Ranges::from_core(
                        cs705_core::icf::frequency::Ranges::generate(&q, None).unwrap(),
                    ),
                )
                .unwrap(),
            )
            .unwrap();
        py.run(cr#"
import gc, sys, weakref
def raises(kind, call):
    try: call()
    except kind as error: return error
    raise AssertionError('expected ' + kind.__name__)
locale = c.Locale.copied(46, 44)
dispatch = c.dispatch.Dispatch(0, c.payload.Kind.Memory, c.dispatch.ImportKind.Memory, True)
assert dispatch.import_kind is c.dispatch.ImportKind.Memory
dispatch.import_kind = None
assert dispatch.import_kind is None
assert 'expire' in c.nls.Bytes.__doc__ and 'Consume' in r.working.Working.into_working.__doc__
source = b'\xef\xbb\xbfH\r\nx,\x81,,z\r\n\n \n\0ignored\n'
reader = c.Reader(source, locale)
assert reader.original_bytes() == source and reader.header() == b'\xef\xbb\xbfH'
iterator = reader.rows()
del reader
gc.collect()
assert list(iterator) == [b'x,\x81,,z', b' ']
assert c.split_cells(b'\"a,b\",\x81,,', 44) == [b'\"a', b'b\"', b'\x81,', b'']
assert c.fixed_text(b'AB\x81', 3) == (b'AB\x81', False)
assert c.fixed_text(b'AB\x81Z', 3) == (b'AB ', True)
assert c.export_text(b'a,b;c\0tail', 100) == b'a b c'
assert c.export_call(b'a,b;c\0tail', 100) == b'a,b;c'
assert c.column(b'x', None, 44) is None and c.column(b'x', 8, 44) == b''
for bad in (True, -1, 256): raises((TypeError, OverflowError), lambda: c.is_lead(bad))
for bad in ('bytes', bytearray(b'bytes'), memoryview(b'bytes')):
    raises(TypeError, lambda: c.Cells(bad, 44))
dt = c.datetime
context = dt.Context(b'yyyy/mm/dd', 47, 58, 46, b'AM', b'PM', 2026, 50, 0,
                     [dt.Era(None, 0), dt.Era(b'', 1)])
options = g.parser.Options(locale, context, b'NE', b' ' * 16)
context.am = b'changed'
assert options.datetime_snapshot().am == b'AM'
rows = g.parser.Rows(b'Group,Bank,Latitude,Longitude,Altitude\n,A,1,2,0\n,A,3,4,bad\n,A,5,6,0\n', options)
first = next(rows)
assert first.group_code == 0xffff
assert next(rows.into_owned()).altitude_error.kind == 'Conversion'
last = next(rows)
assert last.record_snapshot().bytes[0] == 5
raises(StopIteration, lambda: next(rows))
runtime = c.date_input.Runtime(context, b'hh:nn:ss', 1, None)
raw = MutableBytes(bytes.fromhex('290224233040'))
error = raises(Exception, lambda: c.date_input.phase2(raw, 0, 0, b'1' * 4097, runtime))
assert error.kind == 'Limit' and raw.to_bytes() == bytes.fromhex('290224233040')
assert runtime.locale.short_date == b'yyyy/mm/dd' and runtime.locale.date_separator == 47
assert runtime.time_format == b'hh:nn:ss'
gps_header = b'Group,Group Name,Name,Date,Time,Latitude,Longitude,Altitude,Alarm\r\n'
repeater_header = b'Group No,Group Name,Name,Sub Name,Repeater Call Sign,Gateway Call Sign,Frequency,Dup,Offset,Mode,TONE,Repeater Tone,RPT1USE,Position,Latitude,Longitude,UTC Offset\r\n'
for header, module, cells in ((gps_header, g, [b''] * 9), (repeater_header, r, [b''] * 17)):
    cells[2] = b'opaque\x81'
    source = header + b','.join(cells) + b'\r\n'
    document = module.Csv.parse(source)
    old_row = document.rows()[0]
    document.set_name(0, b'edited')
    assert old_row.cells()[2] == b'opaque\x81'
    assert document.into_owned().rows()[0].cells()[2] == b'edited'
    assert source.endswith(b'\r\n') and b'edited' not in source
    before = document.to_bytes()
    raises(TypeError, lambda: document.set_name(True, b'wrong'))
    raises(Exception, lambda: document.set_name(0, b'x' * 17))
    assert document.to_bytes() == before
original = ValueError('callback identity')
retained = []
state = []
row_result, tail_result = object(), object()
def gated_rows(state):
    state.append('rows')
    return row_result
def gated_tail(state):
    state.append('tail')
    return tail_result
assert c.control.import_with_gate(state, c.control.ImportGate.FileCancelled(), gated_rows, gated_tail) == (None, None)
assert state == []
result = c.control.import_with_gate(state, c.control.ImportGate.Header(1), gated_rows, gated_tail)
assert isinstance(result, tuple) and result[0] is row_result and result[1] is tail_result
assert state == ['rows', 'tail']
for wrong in (0, 1, None, b'yes'):
    raises(TypeError, lambda: dt.parse_time_with(b'AM 1:00', context, lambda a, b: wrong))
for wrong in (True, 0, 'text', bytearray(b'x')):
    raises(TypeError, lambda: dt.format_date(dt.Date(2024, 2, 29), b'mmm', context, lambda token: wrong))
def fail_map(source, destination):
    retained.append(destination)
    destination[0] = 90
    raises(IndexError, lambda: destination.write(sys.maxsize * 2 + 1, b'xx'))
    raise original
shared = MutableBytes(b'\x01a')
assert raises(ValueError, lambda: c.nls.callsign_fixed(shared, fail_map)) is original
assert shared.to_bytes() == b'a '
raises(ReferenceError, lambda: retained.pop().to_bytes())
shared = MutableBytes(b'ab')
def mapping(source, destination):
    destination[:2] = b'xy'
    return 0
def fail_upper(destination):
    retained.append(destination)
    destination[:] = b'ZZ'
    raise original
assert raises(ValueError, lambda: c.nls.whole_text(shared,
    c.nls.Profile(True, True, True, b'/', 8), mapping, fail_upper)) is original
assert shared.to_bytes() == b'xy'
raises(ReferenceError, lambda: retained.pop().to_bytes())
default = r.staging.Record(r.working.DefaultContext(False, b'NE').holder(0, 0).as_bytes(), 0)
frequency = lambda hz: r.parser.FrequencyProfile(0, r.finalize.OffsetProfile(0, 0))
rp = r.parser.Options(locale, context, default, r.editor.TYPE_SOURCE, b' ' * 16, 46, frequency)
rr = r.parser.Rows(b'Repeater Call Sign\nFIRST\nSECOND\nTHIRD\n', locale)
def ascii_map(source, destination):
    destination[:] = source
    return len(source)
one = rr.next_with(default, rp, ascii_map)
assert one.record_snapshot().bytes[0x160:0x168] == b'FIRST   '
before = default.bytes
assert raises(ValueError, lambda: rr.next_with(default, rp, fail_map)) is original
assert default.bytes == before
assert rr.into_owned().next_with(default, rp, ascii_map).record_snapshot().bytes[0x160:0x168] == b'THIRD   '
assert rr.next_with(default, rp, ascii_map) is None
def make_cycle():
    class Callback:
        def __call__(self, *args): return 0
    callback = Callback()
    owner = r.editor.Nls(callback, callback)
    callback.owner = owner
    return weakref.ref(callback)
weak = make_cycle()
gc.collect()
assert weak() is None
reads, events = [], []
def live():
    reads.append(len(reads))
    if len(reads) == 2:
        fields = q.fields()
        values = fields.values
        values[0] = 16
        fields.values = values
        q.adopt(fields)
    return b, q
working.bulk_use(r.working.UseRange(0, 0), True, 0, live, events.append)
assert len(reads) == 2 and working.working()[0x3bf6e] & 1 == 1, (len(reads), working.working()[0x3bf6e])
assert [event.event for event in events] == [0x11, 0x13]
def fail_queue(event):
    events.append(event)
    if event.event == 0x13: raise original
events.clear()
assert raises(ValueError, lambda: working.bulk_use(r.working.UseRange(0, 0), False, 0, live, fail_queue)) is original
assert len(reads) == 2 and working.working()[0x3bf6e] & 1 == 0
assert [event.event for event in events] == [0x11, 0x13]
events.clear()
raises(TypeError, lambda: working.bulk_use(r.working.UseRange(0, 0), True, 0, lambda: (b, object()), events.append))
assert events == [] and working.working()[0x3bf6e] & 1 == 0
memory = bytearray(0x3c9c0)
memory[0x355a1] = 6
memory[0x3c9b0:] = b'IcomCloneFormat3'
memory[0x3c385:0x3c613] = b'\xff' * (0x3c613 - 0x3c385)
image = i.Icf.from_memory(bytes(memory))
record = g.operations.Record(True, b'\x01\0\0N\x02\0\0E' + b'\x99' * 6 + b'\xff\xff\xff\x7f' + b' ' * 16)
parsed = g.transfer.Parsed(record, 0, b' ' * 16, 0, False)
def decoded():
    yield parsed
    raise original
error = raises(ValueError, lambda: g.transfer.import_decoded(image, g.transfer.Scope.Whole, decoded(), False))
assert error is original
outcome = error.__context__.exceptions[-1].args[1]
assert outcome.rows_read == 2 and outcome.applied == 1
assert image.gps_groups()[0].physical_slots() == [0]
calls = []
def format_field(request):
    calls.append(request.adapter)
    if len(calls) == 2: raise original
    return b'date'
error = raises(ValueError, lambda: g.transfer.export_csv(image, g.transfer.Scope.Whole, locale, format_field))
assert error is original
outcome = error.__context__.exceptions[-1].args[1]
assert outcome.rows == 0 and outcome.bytes.endswith(b'Alarm\r\n')
assert outcome.failure_snapshot().variant == 'Text'
staging = r.staging.Staging.preload([one.record_snapshot()], [b' ' * 16] * 50, 0xffff)
commits = []
staging.flush_selected(0, 0, 0, commits.append)
borrowed = next(commit.record for commit in commits if commit.variant == 'Insert')
raises(ReferenceError, lambda: borrowed.bytes)
"#, Some(&locals), Some(&locals)).inspect_err(|error| error.print(py)).unwrap();
        py.run(
            &std::ffi::CString::new(include_str!("../python/cs705/csv/_smoke.py")).unwrap(),
            Some(&locals),
            Some(&locals),
        )
        .inspect_err(|error| error.print(py))
        .unwrap();
        py.run(c"main()", Some(&locals), Some(&locals))
            .inspect_err(|error| error.print(py))
            .unwrap();
    });
}
