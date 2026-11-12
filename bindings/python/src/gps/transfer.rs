//! Native GPS transfer result values.
use super::Record;
use crate::callbacks::CallbackError;
use crate::icf::document::adapters::attach_checked;
use crate::{conversion::*, csv::control::Counts, errors, icf::Icf};
use cs705_core::gps::transfer as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

pub(super) fn native_error<T>(
    value: &Bound<'_, PyAny>,
    read: fn(&crate::diagnostic::Diagnostic) -> Option<T>,
) -> PyResult<T> {
    let diagnostic = crate::error_values::diagnostic_from_value(value)?;
    let diagnostic = diagnostic.try_borrow()?;
    read(&diagnostic).ok_or_else(|| {
        pyo3::exceptions::PyTypeError::new_err("native error payload has the wrong source domain")
    })
}

fn finish_report<T: pyo3::PyClass>(
    py: Python<'_>,
    error: CallbackError,
    result: PyResult<Py<T>>,
) -> PyResult<Py<T>> {
    match result {
        Ok(out) => error.finish_with_outcome(py, Ok(out.clone_ref(py)), out.into_any()),
        Err(allocation) => error.finish(py, Err(allocation)),
    }
}

/// Record is read as a snapshot and replaced with set_record.
#[pyclass(module = "cs705.gps.transfer")]
pub struct Parsed {
    pub(crate) inner: core::Parsed,
}
#[pymethods]
impl Parsed {
    #[new]
    fn new(
        record: &Record,
        group: &Bound<'_, PyAny>,
        group_name: &Bound<'_, PyAny>,
        corrections: &Bound<'_, PyAny>,
        alarm: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::Parsed {
                record: record.inner,
                group: strict_usize(group)?,
                group_name: fixed_bytes(group_name)?,
                corrections: strict_u32(corrections)?,
                alarm: strict_bool(alarm)?,
            },
        })
    }
    fn record_snapshot(&self) -> Record {
        Record {
            inner: self.inner.record,
        }
    }
    fn set_record(&mut self, v: &Record) {
        self.inner.record = v.inner;
    }
    #[getter]
    fn group(&self) -> usize {
        self.inner.group
    }
    #[setter]
    fn set_group(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.group = strict_usize(v)?;
        Ok(())
    }
    #[getter]
    fn group_name<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.group_name)
    }
    #[setter]
    fn set_group_name(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.group_name = fixed_bytes(v)?;
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
    #[getter]
    fn alarm(&self) -> bool {
        self.inner.alarm
    }
    #[setter]
    fn set_alarm(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.alarm = strict_bool(v)?;
        Ok(())
    }
}
#[pyclass(module = "cs705.gps.transfer", frozen)]
#[derive(Clone, Copy)]
pub struct Scope {
    pub(crate) inner: core::Scope,
}
#[pymethods]
#[allow(non_snake_case)]
impl Scope {
    #[classattr]
    fn Whole() -> Self {
        Self {
            inner: core::Scope::Whole,
        }
    }
    #[staticmethod]
    fn Group(group: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Scope::Group(strict_usize(group)?),
        })
    }
    #[getter]
    fn group(&self) -> Option<usize> {
        match self.inner {
            core::Scope::Whole => None,
            core::Scope::Group(g) => Some(g),
        }
    }
    #[getter]
    fn variant(&self) -> &'static str {
        match self.inner {
            core::Scope::Whole => "Whole",
            core::Scope::Group(_) => "Group",
        }
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
/// Counts and failure are explicit snapshots with replacement setters.
#[pyclass(module = "cs705.gps.transfer")]
pub struct Import {
    pub(crate) inner: core::Import,
}
#[pyclass(module = "cs705.gps.transfer")]
pub struct Failure {
    inner: core::Failure,
}
#[pymethods]
#[allow(non_snake_case)]
impl Failure {
    #[staticmethod]
    fn Text(error: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Failure::Text(native_error(
                error,
                crate::diagnostic::Diagnostic::csv_error,
            )?),
        })
    }
    #[staticmethod]
    fn Image(error: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Failure::Image(native_error(
                error,
                crate::diagnostic::Diagnostic::icf_error,
            )?),
        })
    }
    #[getter]
    fn variant(&self) -> &'static str {
        match self.inner {
            core::Failure::Text(_) => "Text",
            core::Failure::Image(_) => "Image",
        }
    }
    #[getter]
    fn error(&self, py: Python<'_>) -> Py<PyAny> {
        match &self.inner {
            core::Failure::Text(e) => errors::csv(e.clone()),
            core::Failure::Image(e) => errors::icf(e.clone()),
        }
        .into_value(py)
        .into_any()
    }
    #[setter]
    fn set_error(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner = match self.inner {
            core::Failure::Text(_) => core::Failure::Text(native_error(
                value,
                crate::diagnostic::Diagnostic::csv_error,
            )?),
            core::Failure::Image(_) => core::Failure::Image(native_error(
                value,
                crate::diagnostic::Diagnostic::icf_error,
            )?),
        };
        Ok(())
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
#[pymethods]
impl Import {
    #[new]
    #[pyo3(signature=(counts,rows_read,applied,failure))]
    fn new(
        counts: &Counts,
        rows_read: &Bound<'_, PyAny>,
        applied: &Bound<'_, PyAny>,
        failure: Option<PyRef<'_, Failure>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::Import {
                counts: counts.inner,
                rows_read: strict_usize(rows_read)?,
                applied: strict_usize(applied)?,
                failure: failure.map(|f| f.inner.clone()),
            },
        })
    }
    fn counts_snapshot(&self) -> Counts {
        Counts::from_core(self.inner.counts)
    }
    fn set_counts(&mut self, v: &Counts) {
        self.inner.counts = v.inner;
    }
    #[getter]
    fn rows_read(&self) -> usize {
        self.inner.rows_read
    }
    #[setter]
    fn set_rows_read(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.rows_read = strict_usize(v)?;
        Ok(())
    }
    #[getter]
    fn applied(&self) -> usize {
        self.inner.applied
    }
    #[setter]
    fn set_applied(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.applied = strict_usize(v)?;
        Ok(())
    }
    fn failure_snapshot(&self) -> Option<Failure> {
        self.inner.failure.clone().map(|inner| Failure { inner })
    }
    fn set_failure(&mut self, value: Option<PyRef<'_, Failure>>) {
        self.inner.failure = value.map(|f| f.inner.clone());
    }
}
/// Failure is an explicit snapshot with a replacement setter.
#[pyclass(module = "cs705.gps.transfer")]
pub struct Export {
    pub(crate) inner: core::Export,
}
#[pymethods]
impl Export {
    #[new]
    #[pyo3(signature=(bytes,rows,failure))]
    fn new(
        bytes: &Bound<'_, PyAny>,
        rows: &Bound<'_, PyAny>,
        failure: Option<PyRef<'_, Failure>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::Export {
                bytes: strict_bytes(bytes)?.to_vec(),
                rows: strict_usize(rows)?,
                failure: failure.map(|f| f.inner.clone()),
            },
        })
    }
    #[getter]
    fn bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.bytes)
    }
    #[setter]
    fn set_bytes(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.bytes = strict_bytes(v)?.to_vec();
        Ok(())
    }
    #[getter]
    fn rows(&self) -> usize {
        self.inner.rows
    }
    #[setter]
    fn set_rows(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.rows = strict_usize(v)?;
        Ok(())
    }
    fn failure_snapshot(&self) -> Option<Failure> {
        self.inner.failure.clone().map(|inner| Failure { inner })
    }
    fn set_failure(&mut self, value: Option<PyRef<'_, Failure>>) {
        self.inner.failure = value.map(|f| f.inner.clone());
    }
}
#[pyfunction]
/// Callback failure raises the same exception with the partial Export in its context.
fn export_csv(
    py: Python<'_>,
    image: &Icf,
    scope: &Scope,
    locale: &crate::csv::Locale,
    native: Py<PyAny>,
) -> PyResult<Py<Export>> {
    let (scope, locale) = (scope.inner, locale.native()?);
    let interpreter = crate::scoped::interpreter(py)?;
    let mut error = CallbackError::default();
    let out = image.with_ref(|image| {
        Ok(py.detach(|| {
            core::export_csv(image, scope, locale, |request| {
                let result = attach_checked(interpreter, |py| {
                    native
                        .bind(py)
                        .call1((crate::csv::payload::Request::from_core(request),))
                        .and_then(|value| Ok(strict_bytes(&value)?.to_vec()))
                });
                error.capture(result).map_err(|_| {
                    cs705_core::csv::Error::Operation("GPS field formatter callback failed".into())
                })
            })
        }))
    })?;
    finish_report(py, error, Py::new(py, Export { inner: out }))
}
#[pyfunction]
fn export_csv_formatted(
    py: Python<'_>,
    image: &Icf,
    scope: &Scope,
    locale: &crate::csv::Locale,
    context: &crate::csv::datetime::Context,
    failed_time: Py<crate::csv::datetime::Time>,
    native: Py<PyAny>,
) -> PyResult<Py<Export>> {
    let (scope, locale, time) = (
        scope.inner,
        locale.native()?,
        failed_time.try_borrow(py)?.inner,
    );
    let interpreter = crate::scoped::interpreter(py)?;
    let mut error = CallbackError::default();
    let out = image.with_ref(|image| {
        context.with_context(|context| {
            Ok(py.detach(|| {
                core::export_csv_formatted(image, scope, locale, context, time, |token| {
                    let result = attach_checked(interpreter, |py| {
                        native
                            .bind(py)
                            .call1((crate::csv::datetime::DateToken::from_core(token),))
                            .and_then(|value| Ok(strict_bytes(&value)?.to_vec()))
                    });
                    error.capture(result).map_err(|_| {
                        cs705_core::csv::Error::Operation(
                            "GPS date formatter callback failed".into(),
                        )
                    })
                })
            }))
        })
    })?;
    finish_report(py, error, Py::new(py, Export { inner: out }))
}
#[pyfunction]
fn import_csv(
    py: Python<'_>,
    image: &Icf,
    scope: &Scope,
    source: &Bound<'_, PyAny>,
    options: &super::parser::Options,
    alarm_seen: &Bound<'_, PyAny>,
) -> PyResult<Import> {
    let (source, alarm_seen, scope) = (
        strict_bytes(source)?.to_vec(),
        strict_bool(alarm_seen)?,
        scope.inner,
    );
    options.inner.with_ref(|o| {
        image.with_mut(|image| {
            Ok(Import {
                inner: py.detach(|| {
                    o.with_core(|options| {
                        core::import_csv(image, scope, &source, options, alarm_seen)
                    })
                }),
            })
        })
    })
}
#[pyfunction]
/// Clear the target before requesting the first row; iterator failure retains applied rows.
fn import_decoded(
    py: Python<'_>,
    image: &Icf,
    scope: &Scope,
    rows: &Bound<'_, PyAny>,
    alarm_seen: &Bound<'_, PyAny>,
) -> PyResult<Py<Import>> {
    let (scope, alarm_seen) = (scope.inner, strict_bool(alarm_seen)?);
    let mut iterator = rows.try_iter()?;
    let mut error = CallbackError::default();
    let native = image.with_mut(|image| {
        Ok(core::import_decoded(
            image,
            scope,
            std::iter::from_fn(|| {
                iterator.next().map(|item| {
                    let result =
                        item.and_then(|value| Ok(value.extract::<PyRef<'_, Parsed>>()?.inner));
                    error.capture(result).map_err(|_| {
                        cs705_core::csv::Error::Operation(
                            "GPS decoded iterator callback failed".into(),
                        )
                    })
                })
            }),
            alarm_seen,
        ))
    })?;
    finish_report(py, error, Py::new(py, Import { inner: native }))
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "transfer")?;
    m.add_class::<Parsed>()?;
    m.add_class::<Scope>()?;
    m.add_class::<Import>()?;
    m.add_class::<Export>()?;
    m.add_class::<Failure>()?;
    m.add_function(wrap_pyfunction!(import_csv, &m)?)?;
    m.add_function(wrap_pyfunction!(import_decoded, &m)?)?;
    m.add_function(wrap_pyfunction!(export_csv_formatted, &m)?)?;
    m.add_function(wrap_pyfunction!(export_csv, &m)?)?;
    root.add_submodule(&m)
}
