//! Lossless repeater CSV and native working-domain operations.
use crate::{conversion::*, errors};
use cs705_core::{icf, repeater as core};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

fn equality(equal: bool, op: pyo3::basic::CompareOp) -> PyResult<bool> {
    match op {
        pyo3::basic::CompareOp::Eq => Ok(equal),
        pyo3::basic::CompareOp::Ne => Ok(!equal),
        _ => Err(pyo3::exceptions::PyTypeError::new_err(
            "values have no ordering",
        )),
    }
}

pub mod editor;
pub mod fields;
pub mod finalize;
pub mod groups;
pub mod parser;
pub mod predicate;
pub mod staging;
pub mod transfer;
pub mod working;

#[pyclass(module = "cs705.repeater")]
pub struct Csv {
    inner: core::Csv<'static>,
}
#[pyclass(module = "cs705.repeater", frozen)]
pub struct Row {
    cells: [Vec<u8>; 17],
}
#[pymethods]
impl Row {
    fn cells<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        PyTuple::new(py, self.cells.iter().map(|cell| PyBytes::new(py, cell)))
    }
    fn name_bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.cells[2])
    }
    fn sub_name_bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.cells[3])
    }
    fn into_owned(slf: PyRef<'_, Self>) -> Py<Self> {
        slf.into()
    }
}

macro_rules! csv_methods {
    ($($name:ident => $convert:expr),* $(,)?) => {
        #[pymethods]
        impl Csv {
            #[staticmethod]
            fn parse(source: &Bound<'_, PyAny>) -> PyResult<Self> {
                Ok(Self { inner: core::Csv::parse(strict_bytes(source)?).map_err(errors::repeater)?.into_owned() })
            }
            fn rows(&self) -> Vec<Row> {
                self.inner.rows().iter().map(|row| Row { cells: row.cells().map(<[u8]>::to_vec) }).collect()
            }
            fn into_owned(slf: PyRef<'_, Self>) -> Py<Self> { slf.into() }
            fn to_bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> { PyBytes::new(py, &self.inner.to_bytes()) }
            $(fn $name(&mut self, index: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<()> {
                self.inner.$name(strict_usize(index)?, ($convert)(value)?).map_err(errors::repeater)
            })*
        }
    }
}
csv_methods! {
    set_name => strict_bytes,
    set_sub_name => strict_bytes,
    set_group_name => strict_bytes,
    set_repeater_call_sign => strict_bytes,
    set_gateway_call_sign => strict_bytes,
    set_group_number => strict_u8,
    set_frequency_hz => strict_u32,
    set_offset_hz => strict_u32,
    set_repeater_tone_index => strict_u8,
    set_rpt1_use => strict_bool,
    set_utc_offset_minutes => strict_i16,
    set_latitude => coordinate,
    set_longitude => coordinate,
    set_mode => mode,
    set_duplex => duplex,
    set_tone => tone,
    set_position => position,
}
fn coordinate(value: &Bound<'_, PyAny>) -> PyResult<icf::RepeaterCoordinate> {
    Ok(value
        .extract::<PyRef<'_, crate::icf::domain_values::RepeaterCoordinate>>()?
        .inner)
}
macro_rules! enum_input {
    ($function:ident, $name:ident, $($variant:ident = $value:literal),+ $(,)?) => {
        fn $function(value: &Bound<'_, PyAny>) -> PyResult<icf::$name> {
            match crate::icf::enum_value(value, "cs705.icf", stringify!($name))? {
                $($value => Ok(icf::$name::$variant),)+
                _ => Err(pyo3::exceptions::PyValueError::new_err("invalid enum value")),
            }
        }
    }
}
enum_input!(mode, RepeaterMode, Dv = 0, Fm = 1, FmNarrow = 2, Dd = 3);
enum_input!(
    duplex,
    RepeaterDuplex,
    Off = 0,
    Minus = 1,
    Plus = 2,
    Rps = 3
);
enum_input!(tone, RepeaterTone, Off = 0, Tone = 1, ToneSquelch = 2);
enum_input!(
    position,
    RepeaterPosition,
    None = 0,
    Approximate = 1,
    Exact = 2
);

pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "repeater")?;
    m.add("Error", root.py().get_type::<errors::Error>())?;
    m.add_class::<Csv>()?;
    m.add_class::<Row>()?;
    fields::register(&m)?;
    editor::register(&m)?;
    finalize::register(&m)?;
    groups::register(&m)?;
    staging::register(&m)?;
    working::register(&m)?;
    predicate::register(&m)?;
    parser::register(&m)?;
    transfer::register(&m)?;
    root.add_submodule(&m)
}
