//! Original-source grammar, metadata, and mutable reader state.
use crate::{conversion::*, errors};
use cs705_core::icf::text as native;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

pub(crate) fn status<'py>(py: Python<'py>, value: native::Status) -> PyResult<Bound<'py, PyAny>> {
    super::enum_result(py, "cs705.icf.text", "Status", value as i64)
}

pub(crate) fn status_in(value: &Bound<'_, PyAny>) -> PyResult<native::Status> {
    use native::Status::*;
    Ok(
        match super::enum_value(value, "cs705.icf.text", "Status")? {
            0 => Success,
            3 => Model,
            4 => FileRevision,
            5 => MapRevision,
            6 => Syntax,
            7 => MissingCoverage,
            8 => ExcessCoverage,
            10 => ExtraData,
            _ => return Err(pyo3::exceptions::PyValueError::new_err("invalid Status")),
        },
    )
}

#[pyclass(module = "cs705.icf.text")]
#[derive(Clone)]
pub struct Report {
    pub inner: native::Report,
}
#[pymethods]
impl Report {
    #[new]
    #[pyo3(signature = (status, coverage, metadata_before_data, records, bytes_written, source_comment))]
    fn new(
        status: &Bound<'_, PyAny>,
        coverage: &Bound<'_, PyAny>,
        metadata_before_data: Option<(Bound<'_, PyAny>, Bound<'_, PyAny>)>,
        records: &Bound<'_, PyAny>,
        bytes_written: &Bound<'_, PyAny>,
        source_comment: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: native::Report {
                status: status_in(status)?,
                coverage: strict_bytes(coverage)?.to_vec(),
                metadata_before_data: metadata_before_data
                    .map(|(a, b)| Ok::<_, PyErr>((strict_u32(&a)?, strict_u32(&b)?)))
                    .transpose()?,
                records: strict_usize(records)?,
                bytes_written: strict_usize(bytes_written)?,
                source_comment: strict_bytes(source_comment)?.to_vec(),
            },
        })
    }
    #[getter]
    fn status<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        status(py, self.inner.status)
    }
    #[setter]
    fn set_status(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.status = status_in(value)?;
        Ok(())
    }
    #[getter]
    fn coverage<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.coverage)
    }
    #[setter]
    fn set_coverage(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.coverage = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[getter]
    fn source_comment<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.source_comment)
    }
    #[setter]
    fn set_source_comment(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.source_comment = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[getter]
    fn metadata_before_data(&self) -> Option<(u32, u32)> {
        self.inner.metadata_before_data
    }
    #[setter]
    fn set_metadata_before_data(
        &mut self,
        value: Option<(Bound<'_, PyAny>, Bound<'_, PyAny>)>,
    ) -> PyResult<()> {
        self.inner.metadata_before_data = value
            .map(|(a, b)| Ok::<_, PyErr>((strict_u32(&a)?, strict_u32(&b)?)))
            .transpose()?;
        Ok(())
    }
    #[getter]
    fn records(&self) -> usize {
        self.inner.records
    }
    #[setter]
    fn set_records(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.records = strict_usize(value)?;
        Ok(())
    }
    #[getter]
    fn bytes_written(&self) -> usize {
        self.inner.bytes_written
    }
    #[setter]
    fn set_bytes_written(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.bytes_written = strict_usize(value)?;
        Ok(())
    }
}

#[pyclass(module = "cs705.icf.text")]
#[derive(Clone)]
pub struct Reader {
    pub inner: native::Reader,
}
#[pymethods]
impl Reader {
    #[new]
    fn new() -> Self {
        Self {
            inner: native::Reader::default(),
        }
    }
    #[getter]
    fn model(&self) -> u16 {
        self.inner.model
    }
    #[setter]
    fn set_model(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.model = strict_u16(value)?;
        Ok(())
    }
    #[getter]
    fn file_revision(&self) -> u16 {
        self.inner.file_revision
    }
    #[setter]
    fn set_file_revision(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.file_revision = strict_u16(value)?;
        Ok(())
    }
    #[getter]
    fn map_revision(&self) -> u32 {
        self.inner.map_revision
    }
    #[setter]
    fn set_map_revision(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.map_revision = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn extra_data(&self) -> u32 {
        self.inner.extra_data
    }
    #[setter]
    fn set_extra_data(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.extra_data = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn check_metadata(&self) -> bool {
        self.inner.check_metadata
    }
    #[setter]
    fn set_check_metadata(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.check_metadata = strict_bool(value)?;
        Ok(())
    }
    #[pyo3(signature = (image, source, required))]
    fn read(
        &mut self,
        image: &MutableBytes,
        source: &Bound<'_, PyAny>,
        required: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Report> {
        let source = strict_bytes(source)?;
        let required = required.map(strict_bytes).transpose()?;
        let result = image.with_mut(|image| {
            self.inner
                .read(image, source, required)
                .map_err(errors::icf)
        })?;
        Ok(Report { inner: result })
    }
    #[pyo3(signature = (image, fragments, required))]
    fn read_fragments(
        &mut self,
        image: &MutableBytes,
        fragments: Vec<Option<Bound<'_, PyAny>>>,
        required: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Report> {
        let fragments = fragments
            .iter()
            .map(|s| s.as_ref().map(strict_bytes).transpose())
            .collect::<PyResult<Vec<_>>>()?;
        let required = required.map(strict_bytes).transpose()?;
        let inner = image.with_mut(|image| {
            self.inner
                .read_fragments(image, &fragments, required)
                .map_err(errors::icf)
        })?;
        Ok(Report { inner })
    }
}

#[pyfunction]
#[pyo3(signature = (observed, required))]
fn coverage_status<'py>(
    py: Python<'py>,
    observed: Option<&Bound<'_, PyAny>>,
    required: Option<&Bound<'_, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    status(
        py,
        native::coverage_status(
            observed.map(strict_bytes).transpose()?,
            required.map(strict_bytes).transpose()?,
        )
        .map_err(errors::icf)?,
    )
}
#[pyfunction]
fn extra_data_valid(word: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(native::extra_data_valid(strict_u32(word)?))
}
#[pyfunction]
fn decimal_prefix(bytes: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(native::decimal_prefix(strict_bytes(bytes)?))
}
#[pyfunction]
fn hexadecimal_prefix(bytes: &Bound<'_, PyAny>) -> PyResult<i32> {
    Ok(native::hexadecimal_prefix(strict_bytes(bytes)?))
}
#[pyfunction]
fn fragments<'py>(
    py: Python<'py>,
    source: &Bound<'_, PyAny>,
) -> PyResult<Vec<Bound<'py, PyBytes>>> {
    Ok(native::fragments(strict_bytes(source)?)
        .iter()
        .map(|s| PyBytes::new(py, s))
        .collect())
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "text")?;
    super::add_enum(
        &m,
        "Status",
        &[
            ("Success", 0),
            ("Model", 3),
            ("FileRevision", 4),
            ("MapRevision", 5),
            ("Syntax", 6),
            ("MissingCoverage", 7),
            ("ExcessCoverage", 8),
            ("ExtraData", 10),
        ],
    )?;
    m.add_class::<Reader>()?;
    m.add_class::<Report>()?;
    m.add_function(wrap_pyfunction!(coverage_status, &m)?)?;
    m.add_function(wrap_pyfunction!(extra_data_valid, &m)?)?;
    m.add_function(wrap_pyfunction!(decimal_prefix, &m)?)?;
    m.add_function(wrap_pyfunction!(hexadecimal_prefix, &m)?)?;
    m.add_function(wrap_pyfunction!(fragments, &m)?)?;
    parent.add_submodule(&m)
}
