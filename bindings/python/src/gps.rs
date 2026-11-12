//! Owned GPS domain values and direct native operations.

use crate::{conversion::*, errors};
use cs705_core::gps as core;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

#[path = "gps/editor.rs"]
pub mod editor;
#[path = "gps/operations.rs"]
pub mod operations;
#[path = "gps/parser.rs"]
pub mod parser;
#[path = "gps/transfer.rs"]
pub mod transfer;

#[pyclass(module = "cs705.gps")]
pub struct Csv {
    inner: core::Csv<'static>,
}

#[pyclass(module = "cs705.gps", frozen)]
pub struct Row {
    cells: [Vec<u8>; 9],
}

#[pymethods]
impl Row {
    fn cells<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        PyTuple::new(py, self.cells.iter().map(|cell| PyBytes::new(py, cell)))
    }
    fn into_owned(slf: PyRef<'_, Self>) -> Py<Self> {
        slf.into()
    }
}

#[pymethods]
impl Csv {
    #[staticmethod]
    fn parse(source: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Csv::parse(strict_bytes(source)?)
                .map_err(errors::gps)?
                .into_owned(),
        })
    }
    fn rows(&self) -> Vec<Row> {
        self.inner
            .rows()
            .iter()
            .map(|row| Row {
                cells: row.cells().map(<[u8]>::to_vec),
            })
            .collect()
    }
    fn into_owned(slf: PyRef<'_, Self>) -> Py<Self> {
        slf.into()
    }
    fn set_group(&mut self, index: &Bound<'_, PyAny>, group: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner
            .set_group(strict_usize(index)?, strict_u8(group)?)
            .map_err(errors::gps)
    }
    fn set_group_name(
        &mut self,
        index: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.inner
            .set_group_name(strict_usize(index)?, strict_bytes(value)?)
            .map_err(errors::gps)
    }
    fn set_name(&mut self, index: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner
            .set_name(strict_usize(index)?, strict_bytes(value)?)
            .map_err(errors::gps)
    }
    fn set_alarm(&mut self, index: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner
            .set_alarm(strict_usize(index)?, strict_bool(value)?)
            .map_err(errors::gps)
    }
    fn set_latitude(&mut self, index: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner
            .set_latitude(strict_usize(index)?, coordinate(value)?)
            .map_err(errors::gps)
    }
    fn set_longitude(
        &mut self,
        index: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.inner
            .set_longitude(strict_usize(index)?, coordinate(value)?)
            .map_err(errors::gps)
    }
    fn set_altitude(
        &mut self,
        index: &Bound<'_, PyAny>,
        value: &crate::icf::domain_values::GpsAltitude,
    ) -> PyResult<()> {
        self.inner
            .set_altitude(strict_usize(index)?, value.inner)
            .map_err(errors::gps)
    }
    fn to_bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.to_bytes())
    }
}

fn coordinate(value: &Bound<'_, PyAny>) -> PyResult<cs705_core::icf::GpsCoordinate> {
    Ok(value
        .extract::<PyRef<'_, crate::icf::domain_values::GpsCoordinate>>()?
        .inner)
}

#[pyclass(module = "cs705.gps.operations")]
#[derive(Clone)]
pub struct Record {
    pub(crate) inner: core::operations::Record,
}

#[pymethods]
impl Record {
    #[new]
    fn new(valid: &Bound<'_, PyAny>, bytes: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::operations::Record {
                valid: strict_bool(valid)?,
                bytes: fixed_bytes(bytes)?,
            },
        })
    }
    #[getter]
    fn valid(&self) -> bool {
        self.inner.valid
    }
    #[setter]
    fn set_valid(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.valid = strict_bool(value)?;
        Ok(())
    }
    #[getter]
    fn bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.bytes)
    }
    #[setter]
    fn set_bytes(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.bytes = fixed_bytes(value)?;
        Ok(())
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

#[pyclass(module = "cs705.gps.operations")]
pub struct Event {
    pub(crate) inner: core::operations::Event,
}

#[pymethods]
impl Event {
    #[new]
    fn new(
        id: &Bound<'_, PyAny>,
        synchronous: &Bound<'_, PyAny>,
        arg1: &Bound<'_, PyAny>,
        arg2: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::operations::Event {
                id: strict_u32(id)?,
                synchronous: strict_bool(synchronous)?,
                arg1: strict_u32(arg1)?,
                arg2: strict_u32(arg2)?,
            },
        })
    }
    #[getter]
    fn id(&self) -> u32 {
        self.inner.id
    }
    #[setter]
    fn set_id(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.id = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn synchronous(&self) -> bool {
        self.inner.synchronous
    }
    #[setter]
    fn set_synchronous(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.synchronous = strict_bool(value)?;
        Ok(())
    }
    #[getter]
    fn arg1(&self) -> u32 {
        self.inner.arg1
    }
    #[setter]
    fn set_arg1(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.arg1 = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn arg2(&self) -> u32 {
        self.inner.arg2
    }
    #[setter]
    fn set_arg2(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.arg2 = strict_u32(value)?;
        Ok(())
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

#[pyclass(module = "cs705.gps.operations")]
pub struct Outcome {
    pub(crate) inner: core::operations::Outcome,
}

#[pymethods]
impl Outcome {
    #[new]
    fn new(
        completed: &Bound<'_, PyAny>,
        capacity_stopped: &Bound<'_, PyAny>,
        selection: (Bound<'_, PyAny>, Bound<'_, PyAny>),
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::operations::Outcome {
                completed: strict_usize(completed)?,
                capacity_stopped: strict_bool(capacity_stopped)?,
                selection: (strict_usize(&selection.0)?, strict_usize(&selection.1)?),
            },
        })
    }
    #[getter]
    fn completed(&self) -> usize {
        self.inner.completed
    }
    #[setter]
    fn set_completed(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.completed = strict_usize(value)?;
        Ok(())
    }
    #[getter]
    fn capacity_stopped(&self) -> bool {
        self.inner.capacity_stopped
    }
    #[setter]
    fn set_capacity_stopped(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.capacity_stopped = strict_bool(value)?;
        Ok(())
    }
    #[getter]
    fn selection(&self) -> (usize, usize) {
        self.inner.selection
    }
    #[setter]
    fn set_selection(&mut self, value: (Bound<'_, PyAny>, Bound<'_, PyAny>)) -> PyResult<()> {
        self.inner.selection = (strict_usize(&value.0)?, strict_usize(&value.1)?);
        Ok(())
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

#[pyfunction]
fn valid_coordinate(raw: &Bound<'_, PyAny>, latitude: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(core::parser::valid_coordinate(
        fixed_bytes(raw)?,
        strict_bool(latitude)?,
    ))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let gps = PyModule::new(m.py(), "gps")?;
    gps.add("Error", m.py().get_type::<errors::Error>())?;
    gps.add_class::<Csv>()?;
    gps.add_class::<Row>()?;
    let operations = PyModule::new(m.py(), "operations")?;
    operations.add_class::<Record>()?;
    operations.add_class::<Event>()?;
    operations.add_class::<Outcome>()?;
    self::operations::register(&operations)?;
    gps.add_submodule(&operations)?;
    parser::register(&gps)?;
    transfer::register(&gps)?;
    editor::register(&gps)?;
    m.add_submodule(&gps)
}
