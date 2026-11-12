use pyo3::{prelude::*, sync::PyOnceLock, types::PyDict};

mod data_qa;
mod display_contract;
mod document_qa;
mod icf_qa;
mod presentation_contract;
mod printing_contract;
mod runtime_qa;
mod scoped_contract;

// Collection and weakref assertions share CPython's process-wide collector.
// GC/weakref tests take this lock before attaching, including their callback checks.
static GC_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn package(py: Python<'_>) -> Bound<'_, PyModule> {
    static PACKAGE: PyOnceLock<Py<PyModule>> = PyOnceLock::new();
    PACKAGE
        .get_or_init(py, || {
            // One real extension and facade tree keep Enum identities stable across tests.
            let native = pyo3::wrap_pymodule!(crate::_native)(py);
            let sys = py.import("sys").unwrap();
            sys.getattr("modules")
                .unwrap()
                .set_item("cs705._native", native)
                .unwrap();
            sys.getattr("path")
                .unwrap()
                .call_method1(
                    "insert",
                    (0, concat!(env!("CARGO_MANIFEST_DIR"), "/python")),
                )
                .unwrap();
            py.import("cs705").unwrap().unbind()
        })
        .bind(py)
        .clone()
}

fn native(py: Python<'_>) -> Bound<'_, PyModule> {
    package(py);
    py.import("cs705._native").unwrap()
}

#[test]
fn root_holder_construction_and_native_signatures() {
    Python::initialize();
    Python::attach(|py| {
        let locals = PyDict::new(py);
        locals.set_item("native", native(py)).unwrap();
        py.run(
            cr#"
import inspect
source = bytearray(b'ab')
holder = native.MutableBytes(value=source)
source[0] = 0
assert holder.to_bytes() == b'ab' and len(holder) == 2
assert native.Bool(value=True).value is True
for cls in (native.MutableBytes, native.Bool, native.IoErrorData):
    try: type('Derived', (cls,), {})
    except TypeError: pass
    else: raise AssertionError('native holder became subclassable')
original = OSError(13, 'source')
carrier = native.IoErrorData.from_os_error(error=original)
shared = carrier.share()
assert shared is not carrier and shared.exception() is original
for function in (native.MutableBytes.to_bytes, native.MutableBytes.__len__,
                 native.IoErrorData.share, native.IoErrorData.exception):
    assert inspect.signature(function).parameters['self'].kind is inspect.Parameter.POSITIONAL_ONLY
"#,
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}
