#![deny(unsafe_code)]
#![allow(dead_code, unused_imports)]
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
fn printing_smoke() {
    Python::initialize();
    Python::attach(|py| {
        let root = pyo3::types::PyModule::new(py, "cs705").unwrap();
        errors::register(&root).unwrap();
        icf::register(&root).unwrap();
        display::register(&root).unwrap();
        printing::register(&root).unwrap();
        let locals = pyo3::types::PyDict::new(py);
        locals
            .set_item("p", root.getattr("printing").unwrap())
            .unwrap();
        let modules = py.import("sys").unwrap().getattr("modules").unwrap();
        modules.set_item("cs705", &root).unwrap();
        let icf = root.getattr("icf").unwrap();
        let settings = icf.getattr("settings").unwrap();
        modules.set_item("cs705.icf", &icf).unwrap();
        modules.set_item("cs705.icf.settings", &settings).unwrap();
        modules
            .set_item("cs705.icf.settings.extended", settings.getattr("extended").unwrap())
            .unwrap();
        modules
            .set_item("cs705.printing", root.getattr("printing").unwrap())
            .unwrap();
        modules
            .set_item(
                "cs705.printing.native",
                root.getattr("printing").unwrap().getattr("native").unwrap(),
            )
            .unwrap();
        py.run(c"assert p.axis_spans([10,10,10,10],1,[False]*4,35)==b'\\x00\\x02\\x00\\x01'\nm=p.Metrics(-12,12,6,12,1)\nm.validate()\ng=p.Grid('name',[['fixed','x'],['row','value']],1,1,[False,False],[False,False],None,None,0)\nlayout=p.layout(g,p.Paper(200,200),m)\nassert layout.pages_snapshot()\nassert '<svg' in p.html_page(g,layout.pages_snapshot()[0],p.Paper(200,200),m,1)\n",Some(&locals),Some(&locals)).unwrap();
        py.run(
            &std::ffi::CString::new(include_str!("printing_smoke.py")).unwrap(),
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}
