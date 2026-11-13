//! Full-driver report with identity-preserving public nested state.
use super::normalizer::TypedCounts;
use crate::conversion::*;
use cs705_core::icf::stage9 as core;
use pyo3::prelude::*;

#[pyclass(module = "cs705.icf.stage9")]
pub struct Runtime {
    #[pyo3(get, set)]
    pub context: Py<super::context::Context>,
    #[pyo3(get, set)]
    pub ranges: Py<super::frequency::Ranges>,
    system_langid: Option<u16>,
    previous_reload_guard: u32,
    document_flags: [u8; 2],
}
impl Runtime {
    pub fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::Runtime<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let q = self.context.get().take()?;
        let ranges = self.ranges.try_borrow(py)?;
        ranges.with_ref(|ranges| {
            f(core::Runtime {
                context: &q,
                ranges,
                system_langid: self.system_langid,
                previous_reload_guard: self.previous_reload_guard,
                document_flags: self.document_flags,
            })
        })
    }
}
#[pymethods]
impl Runtime {
    #[new]
    #[pyo3(signature = (context, ranges, system_langid, previous_reload_guard, document_flags))]
    fn new(
        context: Py<super::context::Context>,
        ranges: Py<super::frequency::Ranges>,
        system_langid: Option<&Bound<'_, PyAny>>,
        previous_reload_guard: &Bound<'_, PyAny>,
        document_flags: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            context,
            ranges,
            system_langid: system_langid.map(strict_u16).transpose()?,
            previous_reload_guard: strict_u32(previous_reload_guard)?,
            document_flags: fixed_bytes(document_flags)?,
        })
    }
    #[getter]
    fn system_langid(&self) -> Option<u16> {
        self.system_langid
    }
    #[setter]
    fn set_system_langid(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.system_langid = value.map(strict_u16).transpose()?;
        Ok(())
    }
    #[getter]
    fn previous_reload_guard(&self) -> u32 {
        self.previous_reload_guard
    }
    #[setter]
    fn set_previous_reload_guard(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.previous_reload_guard = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn document_flags<'py>(&self, py: Python<'py>) -> Bound<'py, pyo3::types::PyBytes> {
        pyo3::types::PyBytes::new(py, &self.document_flags)
    }
    #[setter]
    fn set_document_flags(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.document_flags = fixed_bytes(value)?;
        Ok(())
    }
}

#[pyclass(module = "cs705.icf.stage9")]
pub struct Report {
    #[pyo3(get, set)]
    pub typed: Py<TypedCounts>,
    deleted_repeaters: u32,
}
impl Report {
    pub fn from_core(py: Python<'_>, value: core::Report) -> PyResult<Self> {
        Ok(Self {
            typed: Py::new(py, TypedCounts::from_core(value.typed))?,
            deleted_repeaters: value.deleted_repeaters,
        })
    }
    pub fn to_core(&self, py: Python<'_>) -> PyResult<core::Report> {
        Ok(core::Report {
            typed: self.typed.try_borrow(py)?.inner,
            deleted_repeaters: self.deleted_repeaters,
        })
    }
}
#[pyfunction]
fn normalize(
    py: Python<'_>,
    image: &MutableBytes,
    defaults: &super::normalizer::ReferenceImage,
    current: &super::normalizer::ReferenceImage,
    late: &super::normalizer::ReferenceImage,
    r: &Runtime,
) -> PyResult<Report> {
    let value = defaults.with_core(py, |defaults| {
        current.with_core(py, |current| {
            late.with_core(py, |late| {
                r.with_core(py, |r| {
                    image.with_mut(|image| {
                        core::normalize(image, defaults, current, late, r)
                            .map_err(crate::errors::icf)
                    })
                })
            })
        })
    })?;
    Report::from_core(py, value)
}
#[pymethods]
impl Report {
    #[new]
    fn new(typed: Py<TypedCounts>, deleted_repeaters: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            typed,
            deleted_repeaters: strict_u32(deleted_repeaters)?,
        })
    }
    #[getter]
    fn deleted_repeaters(&self) -> u32 {
        self.deleted_repeaters
    }
    #[setter]
    fn set_deleted_repeaters(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.deleted_repeaters = strict_u32(value)?;
        Ok(())
    }
}
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "stage9")?;
    m.add_class::<Report>()?;
    m.add_class::<Runtime>()?;
    m.add_function(wrap_pyfunction!(normalize, &m)?)?;
    parent.add_submodule(&m)
}
