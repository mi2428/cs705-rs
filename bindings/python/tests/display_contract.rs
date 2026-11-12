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
use pyo3::prelude::*;
#[test]
fn display_smoke() {
    Python::initialize();
    Python::attach(|py| {
        let root = pyo3::types::PyModule::new(py, "cs705").unwrap();
        errors::register(&root).unwrap();
        icf::register(&root).unwrap();
        display::register(&root).unwrap();
        let modules = py.import("sys").unwrap().getattr("modules").unwrap();
        modules.set_item("cs705", &root).unwrap();
        modules
            .set_item("cs705.display", root.getattr("display").unwrap())
            .unwrap();
        let locals = pyo3::types::PyDict::new(py);
        locals
            .set_item("d", root.getattr("display").unwrap())
            .unwrap();
        py.run(c"s=d.Settings(0,1,100,0,0,1,1,0,15,0)\nassert s.system_japanese()\ns.toggle_bit(33)\nassert s.bit(33)\nassert s.query_menu(0x40,0,0,0)==(128,0)\nm=d.FontMetrics(16,8,16,96,1,0)\nm.validate()\ntry: s.language=True\nexcept TypeError: pass\nelse: raise AssertionError('bool accepted')\n",Some(&locals),Some(&locals)).unwrap();
        py.run(
            &std::ffi::CString::new(include_str!("display_smoke.py")).unwrap(),
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
        py.run(
            cr#"
import gc
bits = (s.column_bits, s.column_bits_high)
for index in (64, 0xffffffff):
    assert not s.bit(index)
    try:
        s.toggle_bit(index)
    except OSError:
        pass
    else:
        raise AssertionError('outside bit accepted')
assert (s.column_bits, s.column_bits_high) == bits
for index in (True, -1, 1 << 32):
    try:
        s.bit(index)
    except (TypeError, OverflowError):
        pass
    else:
        raise AssertionError('invalid bit type accepted')
live_views = state.views
del state
gc.collect()
assert live_views.states_snapshot()
def qa_io_error(call):
    try:
        call()
    except OSError:
        pass
    else:
        raise AssertionError('exhausted bookkeeping accepted')
overflow = d.Toolbar(False, (0, 0, 1, 1), (1 << 64) - 1)
def qa_no_notify(_):
    raise AssertionError('exhausted or unchanged toolbar notified')
qa_io_error(lambda: overflow.set_visible(True, qa_no_notify))
assert not overflow.visible and overflow.visible_changes == (1 << 64) - 1
overflow.set_visible(False, qa_no_notify)
state = d.State(s, metrics, (0, 0, 1, 1))
state.set_toolbar(overflow)
qa_io_error(lambda: state.menu(b'', 0x38, 0x0d, 0, 0x25, 0x42, None))
assert state.settings_snapshot().toolbar == 0 and not state.toolbar_snapshot().visible
state.resource_generation = (1 << 64) - 1
state.resource_language = 17
state.set_queue([d.Queued(0x19, 0, 0), d.Queued(0x26, 0, 0)])
qa_io_error(lambda: state.drain(b'', None))
assert (state.resource_language, state.resource_generation) == (17, (1 << 64) - 1)
assert [event.event for event in state.queue_snapshot()] == [0x26]
"#,
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
        let diagnostic = Py::new(
            py,
            diagnostic::Diagnostic::from_icf(cs705_core::icf::Error::Invalid {
                line: 37,
                reason: "payload".into(),
            }),
        )
        .unwrap();
        diagnostic.bind(py).setattr("line", 99).unwrap();
        assert!(matches!(
            diagnostic.borrow(py).icf_error(),
            Some(cs705_core::icf::Error::Invalid {
                line: 37,
                reason
            }) if reason == "payload"
        ));
        assert!(diagnostic.borrow(py).csv_error().is_none());
        let diagnostic = diagnostic::Diagnostic::from_csv(cs705_core::csv::Error::Operation(
            "original CSV failure".into(),
        ));
        assert!(matches!(
            diagnostic.csv_error(),
            Some(cs705_core::csv::Error::Operation(reason)) if reason == "original CSV failure"
        ));
        assert!(diagnostic.icf_error().is_none());
    });
}
