#![deny(unsafe_code)]

pub mod callbacks;
pub mod clone;
pub mod conversion;
pub mod csv;
pub mod diagnostic;
pub mod display;
mod error_values;
pub mod errors;
pub mod gps;
pub mod icf;
pub mod item_help;
pub mod owner;
pub mod presentation;
pub mod printing;
pub mod repeater;
#[allow(unsafe_code)]
pub mod scoped;

use pyo3::prelude::*;

#[pymodule(gil_used = false)]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    errors::register(m)?;
    conversion::register(m)?;
    callbacks::register(m)?;
    icf::register(m)?;
    csv::register(m)?;
    gps::register(m)?;
    repeater::register(m)?;
    clone::register(m)?;
    presentation::register(m)?;
    diagnostic::register(m)?;
    item_help::register(m)?;
    display::register(m)?;
    printing::register(m)?;
    error_values::register(m)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::PyDict;

    #[test]
    fn root_holder_construction_and_native_signatures() {
        Python::initialize();
        Python::attach(|py| {
            let locals = PyDict::new(py);
            locals
                .set_item("native", pyo3::wrap_pymodule!(_native)(py))
                .unwrap();
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
}
