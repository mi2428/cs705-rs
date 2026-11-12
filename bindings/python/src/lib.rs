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
