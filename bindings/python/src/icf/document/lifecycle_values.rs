//! Typed protocol requests and responses; raw path/resource bytes stay bytes.
use crate::conversion::*;
use cs705_core::icf::document::lifecycle as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

#[pyclass(module = "cs705.icf.document.lifecycle")]
#[derive(Clone)]
pub struct SaveDialog {
    pub inner: core::SaveDialog,
}
#[pymethods]
impl SaveDialog {
    #[new]
    fn new(
        path: &Bound<'_, PyAny>,
        directory: &Bound<'_, PyAny>,
        filter_index: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::SaveDialog {
                path: strict_bytes(path)?.to_vec(),
                directory: strict_bytes(directory)?.to_vec(),
                filter_index: strict_u32(filter_index)?,
            },
        })
    }
    #[getter]
    fn path<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.path)
    }
    #[setter]
    fn set_path(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.path = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[getter]
    fn directory<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.directory)
    }
    #[setter]
    fn set_directory(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.directory = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[getter]
    fn filter_index(&self) -> u32 {
        self.inner.filter_index
    }
    #[setter]
    fn set_filter_index(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.filter_index = strict_u32(value)?;
        Ok(())
    }
}
#[pyclass(module = "cs705.icf.document.lifecycle")]
#[derive(Clone)]
pub struct SaveSelection {
    pub inner: core::SaveSelection,
}
#[pymethods]
impl SaveSelection {
    #[new]
    fn new(path: &Bound<'_, PyAny>, filter_index: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::SaveSelection {
                path: strict_bytes(path)?.to_vec(),
                filter_index: strict_u32(filter_index)?,
            },
        })
    }
    #[getter]
    fn path<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.path)
    }
    #[setter]
    fn set_path(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.path = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[getter]
    fn filter_index(&self) -> u32 {
        self.inner.filter_index
    }
    #[setter]
    fn set_filter_index(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.filter_index = strict_u32(value)?;
        Ok(())
    }
}
#[pyclass(frozen, module = "cs705.icf.document.lifecycle")]
#[derive(Clone, Copy)]
pub struct Prompt {
    pub inner: core::Prompt,
}
#[pymethods]
impl Prompt {
    #[classattr]
    #[pyo3(name = "Dirty")]
    fn dirty() -> Self {
        Self {
            inner: core::Prompt::Dirty,
        }
    }
    #[staticmethod]
    #[pyo3(name = "Legacy")]
    fn legacy(target: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Prompt::Legacy {
                target: strict_u32(target)?,
            },
        })
    }
    #[staticmethod]
    #[pyo3(name = "Warning")]
    fn warning(count: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Prompt::Warning {
                count: strict_u32(count)?,
            },
        })
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self.inner {
            core::Prompt::Dirty => "Dirty",
            core::Prompt::Legacy { .. } => "Legacy",
            core::Prompt::Warning { .. } => "Warning",
        }
    }
    #[getter]
    fn target(&self) -> Option<u32> {
        if let core::Prompt::Legacy { target } = self.inner {
            Some(target)
        } else {
            None
        }
    }
    #[getter]
    fn count(&self) -> Option<u32> {
        if let core::Prompt::Warning { count } = self.inner {
            Some(count)
        } else {
            None
        }
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
fn array<const N: usize>(value: &Bound<'_, PyAny>) -> PyResult<[i32; N]> {
    let values = value
        .try_iter()?
        .map(|item| strict_i32(&item?))
        .collect::<PyResult<Vec<_>>>()?;
    values.try_into().map_err(|_| {
        pyo3::exceptions::PyValueError::new_err(format!("expected exactly {N} integers"))
    })
}
#[pyclass(module = "cs705.icf.document.lifecycle")]
#[derive(Clone, Copy)]
pub struct Placement {
    pub inner: core::Placement,
}
#[pymethods]
impl Placement {
    #[new]
    fn new(
        flags: &Bound<'_, PyAny>,
        show: &Bound<'_, PyAny>,
        minimum: &Bound<'_, PyAny>,
        maximum: &Bound<'_, PyAny>,
        normal: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::Placement {
                flags: strict_u32(flags)?,
                show: strict_u32(show)?,
                minimum: array(minimum)?,
                maximum: array(maximum)?,
                normal: array(normal)?,
            },
        })
    }
    #[getter]
    fn flags(&self) -> u32 {
        self.inner.flags
    }
    #[setter]
    fn set_flags(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.flags = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn show(&self) -> u32 {
        self.inner.show
    }
    #[setter]
    fn set_show(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.show = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn minimum(&self) -> (i32, i32) {
        self.inner.minimum.into()
    }
    #[setter]
    fn set_minimum(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.minimum = array(value)?;
        Ok(())
    }
    #[getter]
    fn maximum(&self) -> (i32, i32) {
        self.inner.maximum.into()
    }
    #[setter]
    fn set_maximum(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.maximum = array(value)?;
        Ok(())
    }
    #[getter]
    fn normal(&self) -> (i32, i32, i32, i32) {
        self.inner.normal.into()
    }
    #[setter]
    fn set_normal(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.normal = array(value)?;
        Ok(())
    }
}
#[pyclass(module = "cs705.icf.document.lifecycle")]
pub struct OpenFile {
    pub inner: core::OpenFile,
}
#[pymethods]
impl OpenFile {
    #[new]
    fn new(path: &Bound<'_, PyAny>, bytes: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::OpenFile {
                path: strict_bytes(path)?.to_vec(),
                bytes: strict_bytes(bytes)?.to_vec(),
            },
        })
    }
    #[getter]
    fn path<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.path)
    }
    #[setter]
    fn set_path(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.path = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[getter]
    fn bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.bytes)
    }
    #[setter]
    fn set_bytes(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.bytes = strict_bytes(value)?.to_vec();
        Ok(())
    }
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SaveDialog>()?;
    m.add_class::<SaveSelection>()?;
    m.add_class::<Prompt>()?;
    m.add_class::<Placement>()?;
    m.add_class::<OpenFile>()?;
    Ok(())
}
