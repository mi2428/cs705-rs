use super::{fields::TypeSource, staging::Record};
use crate::conversion::*;
use cs705_core::repeater::finalize as core;
use pyo3::prelude::*;

#[pyclass(module = "cs705.repeater.finalize")]
pub struct OffsetProfile {
    pub(crate) inner: core::OffsetProfile,
}
#[pymethods]
impl OffsetProfile {
    #[new]
    fn new(class_: &Bound<'_, PyAny>, default_hz: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::OffsetProfile {
                class: strict_i32(class_)?,
                default_hz: strict_u32(default_hz)?,
            },
        })
    }
    #[getter]
    fn class_(&self) -> i32 {
        self.inner.class
    }
    #[setter]
    fn set_class_(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.class = strict_i32(v)?;
        Ok(())
    }
    #[getter]
    fn default_hz(&self) -> u32 {
        self.inner.default_hz
    }
    #[setter]
    fn set_default_hz(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.default_hz = strict_u32(v)?;
        Ok(())
    }
}
#[pyclass(module = "cs705.repeater.finalize")]
pub struct Outcome {
    inner: core::Outcome,
}
#[pymethods]
impl Outcome {
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
    #[new]
    fn new(accepted: &Bound<'_, PyAny>, corrections: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Outcome {
                accepted: strict_bool(accepted)?,
                corrections: strict_u32(corrections)?,
            },
        })
    }
    #[getter]
    fn accepted(&self) -> bool {
        self.inner.accepted
    }
    #[setter]
    fn set_accepted(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.accepted = strict_bool(v)?;
        Ok(())
    }
    #[getter]
    fn corrections(&self) -> u32 {
        self.inner.corrections
    }
    #[setter]
    fn set_corrections(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.corrections = strict_u32(v)?;
        Ok(())
    }
}
#[pyfunction]
fn utc_minutes(value: &Bound<'_, PyAny>, default: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(core::utc_minutes(strict_u32(value)?, strict_u32(default)?))
}
#[pyfunction]
fn finalize(
    row: &Record,
    initial: &Bound<'_, PyAny>,
    default: &Record,
    offset: &OffsetProfile,
    types: &TypeSource,
) -> PyResult<Outcome> {
    let initial = strict_u32(initial)?;
    // Native &mut / & aliasing is impossible even when both Python arguments are equal.
    row.inner.with_mut(|r| {
        default.inner.with_ref(|d| {
            Ok(Outcome {
                inner: core::finalize(r, initial, d, offset.inner, types.inner),
            })
        })
    })
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "finalize")?;
    m.add_class::<OffsetProfile>()?;
    m.add_class::<Outcome>()?;
    m.add_function(wrap_pyfunction!(utc_minutes, &m)?)?;
    m.add_function(wrap_pyfunction!(finalize, &m)?)?;
    root.add_submodule(&m)
}
