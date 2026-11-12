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
use pyo3::prelude::*;
#[test]
fn presentation_smoke() {
    Python::initialize();
    Python::attach(|py| {
        let root = pyo3::types::PyModule::new(py, "cs705").unwrap();
        errors::register(&root).unwrap();
        icf::register(&root).unwrap();
        csv::register(&root).unwrap();
        gps::register(&root).unwrap();
        repeater::register(&root).unwrap();
        presentation::register(&root).unwrap();
        diagnostic::register(&root).unwrap();
        item_help::register(&root).unwrap();
        error_values::register(&root).unwrap();
        let modules = py.import("sys").unwrap().getattr("modules").unwrap();
        modules.set_item("cs705", &root).unwrap();
        modules.set_item("cs705._native", &root).unwrap();
        modules
            .set_item("cs705.icf", root.getattr("icf").unwrap())
            .unwrap();
        for name in ["presentation", "diagnostic", "item_help"] {
            modules
                .set_item(format!("cs705.{name}"), root.getattr(name).unwrap())
                .unwrap();
        }
        for (name, source) in [
            (
                "presentation",
                include_str!("../python/cs705/presentation.py"),
            ),
            ("diagnostic", include_str!("../python/cs705/diagnostic.py")),
            ("item_help", include_str!("../python/cs705/item_help.py")),
        ] {
            let module = pyo3::types::PyModule::new(py, &format!("cs705.{name}")).unwrap();
            module.setattr("__package__", "cs705").unwrap();
            py.run(
                &std::ffi::CString::new(source).unwrap(),
                Some(&module.dict()),
                Some(&module.dict()),
            )
            .unwrap();
            modules.set_item(format!("cs705.{name}"), &module).unwrap();
        }
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
        py.run(c"presenter = p.Presenter(p.Language.Japanese)\nassert presenter.message(p.Message.Enabled()) == '有効'\nassert p.language_code(p.parse_language('de')) == 'de'\nassert d.line == 37 and d.technical_detail() == 'original detail'\nassert '37' in presenter.diagnostic(d)\nassert 'original detail' not in presenter.diagnostic(d)\nstate = h.State(0,0,19,77,88)\napi = h.Api()\neffects = h.Effects()\ntry: h.resolve(b'', h.Input.Contents(), 0, b'')\nexcept Exception: pass\nelse: raise AssertionError('missing source accepted')\ntry: state.module = True\nexcept TypeError: pass\nelse: raise AssertionError('bool accepted')\n",Some(&locals),Some(&locals)).unwrap();
        py.run(c"request = h.Request(None,None,None,b'',b'',-1,b'help.chm',b'help.chm')\napi.load_library = 11\ntry: h.drive(request,state,api,effects)\nexcept Exception as error: assert error.kind == 'SourceUnavailable'\nelse: raise AssertionError('missing observation accepted')\nassert state.module == 11 and state.cookie == 19\nassert [call.api for call in effects.calls] == ['LoadLibraryA(HHCTRL.OCX)', 'GetProcAddress(HtmlHelpA)']\napi.get_proc_address = 22\napi.initialize_cookie = 33\napi.display_window = 0\nstate.module = 0\nassert h.drive(request,state,api,effects) == 1\nassert state.d184 == 0 and state.d188 == 77\nassert effects.message_resource_index == 0\neffects.calls = effects.calls[:1]\nassert len(effects.calls) == 1\napi.cursor = (1,2)\nassert api.cursor == (1,2)\nassert p.Message.RowsProcessed(2).count == 2\nassert p.Message.Code(-3).value == -3\nassert presenter.channel_label(p.ChannelField.FrequencyHz)\n",Some(&locals),Some(&locals)).unwrap();
        py.run(c"value = p.Value.Agc(icf.Agc.Fast)\nassert value.payload is icf.Agc.Fast\nassert presenter.value(value)\nfor lang in p.Language:\n    assert p.Presenter(lang).message(p.Message.RowsProcessed(2))\ntry: p.Value.Agc(p.Language.English)\nexcept TypeError: pass\nelse: raise AssertionError('foreign enum accepted')\n",Some(&locals),Some(&locals)).unwrap();
        py.run(
            &std::ffi::CString::new(include_str!("presentation_fields.py")).unwrap(),
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}
