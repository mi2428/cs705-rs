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
#[path = "../src/errors.rs"]
mod errors;
#[path = "../src/error_values.rs"]
mod error_values;
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
