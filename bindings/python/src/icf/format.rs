//! Layout-aware image gates and serialization; no document lifecycle behavior.
use crate::{conversion::*, errors};
use cs705_core::icf::format as native;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

#[pyclass(module = "cs705.icf.format", frozen)]
#[derive(Clone, Copy)]
pub struct Layout {
    pub inner: native::Layout,
}
#[pymethods]
impl Layout {
    #[staticmethod]
    fn from_map_revision(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: native::Layout::from_map_revision(strict_u32(value)?),
        })
    }
    fn selector(&self) -> u8 {
        self.inner.selector()
    }
    fn storage_len(&self) -> usize {
        self.inner.storage_len()
    }
    fn revision_offset(&self) -> usize {
        self.inner.revision_offset()
    }
    fn signature_offset(&self) -> usize {
        self.inner.signature_offset()
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

#[pyclass(module = "cs705.icf.format", frozen)]
#[derive(Clone, Copy)]
pub struct Status {
    pub inner: native::Status,
}
#[pymethods]
impl Status {
    #[classattr]
    #[pyo3(name = "Success")]
    fn success() -> Self {
        Self {
            inner: native::Status::Success,
        }
    }
    #[classattr]
    #[pyo3(name = "Signature")]
    fn signature() -> Self {
        Self {
            inner: native::Status::Signature,
        }
    }
    #[staticmethod]
    #[pyo3(name = "Reader")]
    fn reader(status: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: native::Status::Reader(super::text::status_in(status)?),
        })
    }
    #[staticmethod]
    #[pyo3(name = "Coverage")]
    fn coverage(status: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: native::Status::Coverage(super::text::status_in(status)?),
        })
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self.inner {
            native::Status::Success => "Success",
            native::Status::Signature => "Signature",
            native::Status::Reader(_) => "Reader",
            native::Status::Coverage(_) => "Coverage",
        }
    }
    #[getter]
    fn status<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        match self.inner {
            native::Status::Reader(s) | native::Status::Coverage(s) => {
                Ok(Some(super::text::status(py, s)?))
            }
            _ => Ok(None),
        }
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

// Nested public fields are Python owners, not detached mutable getters.
#[pyclass(module = "cs705.icf.format")]
pub struct FileImage {
    #[pyo3(get, set)]
    pub report: Py<super::text::Report>,
    #[pyo3(get, set)]
    pub reader: Py<super::text::Reader>,
    pub status: native::Status,
    pub layout: Option<native::Layout>,
    pub image: Vec<u8>,
}
impl FileImage {
    pub fn from_core(py: Python<'_>, value: native::FileImage) -> PyResult<Self> {
        Ok(Self {
            report: Py::new(
                py,
                super::text::Report {
                    inner: value.report,
                },
            )?,
            reader: Py::new(
                py,
                super::text::Reader {
                    inner: value.reader,
                },
            )?,
            status: value.status,
            layout: value.layout,
            image: value.image,
        })
    }
    pub fn to_core(&self, py: Python<'_>) -> PyResult<native::FileImage> {
        Ok(native::FileImage {
            report: self.report.try_borrow(py)?.inner.clone(),
            reader: self.reader.try_borrow(py)?.inner.clone(),
            status: self.status,
            layout: self.layout,
            image: self.image.clone(),
        })
    }
}
#[pymethods]
impl FileImage {
    #[new]
    #[pyo3(signature = (report, reader, status, layout, image))]
    fn new(
        report: Py<super::text::Report>,
        reader: Py<super::text::Reader>,
        status: &Status,
        layout: Option<PyRef<'_, Layout>>,
        image: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            report,
            reader,
            status: status.inner,
            layout: layout.map(|v| v.inner),
            image: strict_bytes(image)?.to_vec(),
        })
    }
    #[getter]
    fn status(&self) -> Status {
        Status { inner: self.status }
    }
    #[setter]
    fn set_status(&mut self, value: &Status) {
        self.status = value.inner;
    }
    #[getter]
    fn layout(&self) -> Option<Layout> {
        self.layout.map(|inner| Layout { inner })
    }
    #[setter]
    fn set_layout(&mut self, value: Option<PyRef<'_, Layout>>) {
        self.layout = value.map(|v| v.inner);
    }
    #[getter]
    fn image<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.image)
    }
    #[setter]
    fn set_image(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.image = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[pyo3(signature = (destination, seed))]
    fn convert_to<'py>(
        &self,
        py: Python<'py>,
        destination: &Layout,
        seed: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let result = self
            .to_core(py)?
            .convert_to(destination.inner, seed.map(strict_bytes).transpose()?)
            .map_err(errors::clone)?;
        Ok(PyBytes::new(py, &result))
    }
}

#[pyfunction]
fn read(py: Python<'_>, source: &Bound<'_, PyAny>) -> PyResult<FileImage> {
    FileImage::from_core(
        py,
        native::read(strict_bytes(source)?).map_err(errors::icf)?,
    )
}
#[pyfunction]
fn read_bound(
    py: Python<'_>,
    source: &Bound<'_, PyAny>,
    image: &Bound<'_, PyAny>,
) -> PyResult<FileImage> {
    FileImage::from_core(
        py,
        native::read_bound(strict_bytes(source)?, strict_bytes(image)?.to_vec())
            .map_err(errors::icf)?,
    )
}
#[pyfunction]
fn read_fragments(py: Python<'_>, fragments: Vec<Option<Bound<'_, PyAny>>>) -> PyResult<FileImage> {
    let fragments = fragments
        .iter()
        .map(|s| s.as_ref().map(strict_bytes).transpose())
        .collect::<PyResult<Vec<_>>>()?;
    FileImage::from_core(py, native::read_fragments(&fragments).map_err(errors::icf)?)
}
#[pyfunction]
fn validate_image(image: &Bound<'_, PyAny>, layout: &Layout) -> PyResult<bool> {
    native::validate_image(strict_bytes(image)?, layout.inner).map_err(errors::icf)
}
#[pyfunction]
fn write_storage<'py>(
    py: Python<'py>,
    image: &Bound<'_, PyAny>,
    layout: &Layout,
    comment: &Bound<'_, PyAny>,
    extra_data: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let result = native::write_storage(
        strict_bytes(image)?,
        layout.inner,
        strict_bytes(comment)?,
        strict_u32(extra_data)?,
    )
    .map_err(errors::icf)?;
    Ok(PyBytes::new(py, &result))
}
#[pyfunction]
fn write_storage_with_checksum<'py>(
    py: Python<'py>,
    image: &Bound<'_, PyAny>,
    layout: &Layout,
    comment: &Bound<'_, PyAny>,
    extra_data: &Bound<'_, PyAny>,
    checksum: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let result = native::write_storage_with_checksum(
        strict_bytes(image)?,
        layout.inner,
        strict_bytes(comment)?,
        strict_u32(extra_data)?,
        strict_bool(checksum)?,
    )
    .map_err(errors::icf)?;
    Ok(PyBytes::new(py, &result))
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "format")?;
    m.add_class::<Layout>()?;
    m.add_class::<Status>()?;
    m.add_class::<FileImage>()?;
    m.add_function(wrap_pyfunction!(read, &m)?)?;
    m.add_function(wrap_pyfunction!(read_bound, &m)?)?;
    m.add_function(wrap_pyfunction!(read_fragments, &m)?)?;
    m.add_function(wrap_pyfunction!(validate_image, &m)?)?;
    m.add_function(wrap_pyfunction!(write_storage, &m)?)?;
    m.add_function(wrap_pyfunction!(write_storage_with_checksum, &m)?)?;
    parent.add_submodule(&m)
}
