//! Repeater CSV transfer over the shared ICF and expanded Working owners.
use super::working::{DefaultContext, Working};
use crate::callbacks::{CallbackError, CallbackHolder, CallbackLink};
use crate::{
    conversion::*,
    csv::{Locale, control::Counts},
    errors,
    icf::Icf,
    icf::document::adapters::attach_checked,
};
use cs705_core::repeater::transfer as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3::{PyTraverseError, PyVisit};
use std::cell::RefCell;

fn native_error<T>(
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

#[pyclass(module = "cs705.repeater.transfer")]
pub struct UnwrittenSource {
    source: Py<super::staging::PreserveSource>,
    read_budget: usize,
    read: Option<Py<CallbackHolder>>,
}
#[pymethods]
impl UnwrittenSource {
    #[new]
    fn new(
        py: Python<'_>,
        source: Py<super::staging::PreserveSource>,
        read_budget: &Bound<'_, PyAny>,
        read: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            source,
            read_budget: strict_usize(read_budget)?,
            read: Some(CallbackHolder::new(py, read)?),
        })
    }
    #[getter]
    fn source(&self, py: Python<'_>) -> Py<super::staging::PreserveSource> {
        self.source.clone_ref(py)
    }
    #[setter]
    fn set_source(&mut self, v: Py<super::staging::PreserveSource>) {
        self.source = v;
    }
    #[getter]
    fn read_budget(&self) -> usize {
        self.read_budget
    }
    #[setter]
    fn set_read_budget(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.read_budget = strict_usize(v)?;
        Ok(())
    }
    #[getter]
    fn read(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        CallbackHolder::link(
            self.read
                .as_ref()
                .ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("unwritten reader was cleared")
                })?
                .bind(py),
        )
        .with_callable(py, |c| Ok(c.clone().unbind()))
    }
    #[setter]
    fn set_read(&mut self, py: Python<'_>, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.read = Some(CallbackHolder::new(py, v)?);
        Ok(())
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.source)?;
        visit.call(&self.read)
    }
    fn __clear__(&mut self) {
        self.read = None;
    }
}
/// Context and reset_words are explicit snapshots with replacement setters.
#[pyclass(module = "cs705.repeater.transfer")]
pub struct ImportOptions {
    mode: core::ImportMode,
    parser: Py<super::parser::Options>,
    context: cs705_core::repeater::working::DefaultContext,
    reset_words: [u16; 2],
    unwritten: Option<Py<UnwrittenSource>>,
}
fn import_mode(v: &Bound<'_, PyAny>) -> PyResult<core::ImportMode> {
    Ok(
        match crate::icf::enum_value(v, "cs705.repeater.transfer", "ImportMode")? {
            0 => core::ImportMode::Merge,
            1 => core::ImportMode::PreserveUse,
            2 => core::ImportMode::Replace,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "invalid ImportMode",
                ));
            }
        },
    )
}
#[pymethods]
impl ImportOptions {
    #[new]
    #[pyo3(signature=(mode,parser,context,reset_words,unwritten))]
    fn new(
        mode: &Bound<'_, PyAny>,
        parser: Py<super::parser::Options>,
        context: &DefaultContext,
        reset_words: &Bound<'_, PyAny>,
        unwritten: Option<Py<UnwrittenSource>>,
    ) -> PyResult<Self> {
        Ok(Self {
            mode: import_mode(mode)?,
            parser,
            context: context.inner,
            reset_words: super::groups::words(reset_words)?,
            unwritten,
        })
    }
    #[getter]
    fn mode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        crate::icf::enum_result(
            py,
            "cs705.repeater.transfer",
            "ImportMode",
            self.mode as i64,
        )
    }
    #[setter]
    fn set_mode(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.mode = import_mode(v)?;
        Ok(())
    }
    #[getter]
    fn parser(&self, py: Python<'_>) -> Py<super::parser::Options> {
        self.parser.clone_ref(py)
    }
    #[setter]
    fn set_parser(&mut self, v: Py<super::parser::Options>) {
        self.parser = v;
    }
    fn context_snapshot(&self) -> DefaultContext {
        DefaultContext {
            inner: self.context,
        }
    }
    fn set_context(&mut self, v: &DefaultContext) {
        self.context = v.inner;
    }
    fn reset_words_snapshot(&self) -> Vec<u16> {
        self.reset_words.to_vec()
    }
    fn set_reset_words(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.reset_words = super::groups::words(v)?;
        Ok(())
    }
    #[getter]
    fn unwritten(&self, py: Python<'_>) -> Option<Py<UnwrittenSource>> {
        self.unwritten.as_ref().map(|v| v.clone_ref(py))
    }
    #[setter]
    fn set_unwritten(&mut self, v: Option<Py<UnwrittenSource>>) {
        self.unwritten = v;
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.parser)?;
        visit.call(&self.unwritten)
    }
    fn __clear__(&mut self) {
        self.unwritten = None;
    }
}
struct ImportInputs {
    mode: core::ImportMode,
    parser: super::parser::Inputs,
    context: cs705_core::repeater::working::DefaultContext,
    reset: [u16; 2],
    unwritten: Option<(
        cs705_core::repeater::staging::PreserveSource,
        usize,
        CallbackLink,
        Py<CallbackHolder>,
    )>,
}
impl ImportInputs {
    fn from_options(py: Python<'_>, options: &Py<ImportOptions>) -> PyResult<Self> {
        let (mode, parser, context, reset, unwritten) = {
            let o = options.try_borrow(py)?;
            (
                o.mode,
                o.parser.clone_ref(py),
                o.context,
                o.reset_words,
                o.unwritten.as_ref().map(|v| v.clone_ref(py)),
            )
        };
        let unwritten = unwritten
            .map(|u| {
                let u = u.try_borrow(py)?;
                let source = u.source.try_borrow(py)?.inner;
                let holder = u
                    .read
                    .as_ref()
                    .ok_or_else(|| {
                        pyo3::exceptions::PyRuntimeError::new_err("unwritten reader was cleared")
                    })?
                    .clone_ref(py);
                Ok::<_, PyErr>((
                    source,
                    u.read_budget,
                    CallbackHolder::link(holder.bind(py)),
                    holder,
                ))
            })
            .transpose()?;
        Ok(Self {
            mode,
            parser: super::parser::Inputs::from_options(py, &parser)?,
            context,
            reset,
            unwritten,
        })
    }
    fn detached<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut core::ImportOptions<'_, '_>, &RefCell<CallbackError>) -> R + Send,
    ) -> PyResult<(R, CallbackError)> {
        self.parser.detached(py, |parser, errors| {
            let mut read = |raw, at| {
                let result = Python::attach(|py| {
                    self.unwritten
                        .as_ref()
                        .ok_or_else(|| {
                            pyo3::exceptions::PyRuntimeError::new_err(
                                "missing explicit unwritten reader",
                            )
                        })
                        .and_then(|(_, _, link, _)| {
                            link.with_callable(py, |c| {
                                c.call1((raw, at))?
                                    .extract::<PyRef<'_, super::staging::Record>>()?
                                    .inner
                                    .with_ref(|r| Ok(r.clone()))
                            })
                        })
                });
                errors.borrow_mut().capture(result).map_err(|_| {
                    cs705_core::csv::Error::Operation(
                        "repeater unwritten reader callback failed".into(),
                    )
                })
            };
            let unwritten =
                self.unwritten
                    .as_ref()
                    .map(|(source, budget, _, _)| core::UnwrittenSource {
                        source: *source,
                        read_budget: *budget,
                        read: &mut read
                            as &mut dyn FnMut(
                                u32,
                                u32,
                            ) -> Result<
                                cs705_core::repeater::staging::Record,
                                cs705_core::csv::Error,
                            >,
                    });
            f(
                &mut core::ImportOptions {
                    mode: self.mode,
                    parser,
                    context: self.context,
                    reset_words: self.reset,
                    unwritten,
                },
                errors,
            )
        })
    }
}

#[pyclass(module = "cs705.repeater.transfer", frozen)]
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
    fn variant(&self) -> &'static str {
        match self.inner {
            core::Scope::Whole => "Whole",
            core::Scope::Group(_) => "Group",
        }
    }
    #[getter]
    fn group(&self) -> Option<usize> {
        match self.inner {
            core::Scope::Whole => None,
            core::Scope::Group(g) => Some(g),
        }
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
#[pyclass(module = "cs705.repeater.transfer")]
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
/// Failure is an explicit snapshot with a replacement setter.
#[pyclass(module = "cs705.repeater.transfer")]
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
/// Counts, group_notifications and failure are explicit snapshots with setters.
#[pyclass(module = "cs705.repeater.transfer")]
pub struct Import {
    pub(crate) inner: core::Import,
}
#[pymethods]
impl Import {
    #[new]
    #[pyo3(signature=(counts,rows_read,applied,group_notifications,failure))]
    fn new(
        counts: &Counts,
        rows_read: &Bound<'_, PyAny>,
        applied: &Bound<'_, PyAny>,
        group_notifications: &Bound<'_, PyAny>,
        failure: Option<PyRef<'_, Failure>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::Import {
                counts: counts.inner,
                rows_read: strict_usize(rows_read)?,
                applied: strict_usize(applied)?,
                group_notifications: group_notifications
                    .try_iter()?
                    .map(|v| strict_usize(&v?))
                    .collect::<PyResult<_>>()?,
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
    fn group_notifications_snapshot(&self) -> Vec<usize> {
        self.inner.group_notifications.clone()
    }
    fn set_group_notifications(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.group_notifications = v
            .try_iter()?
            .map(|v| strict_usize(&v?))
            .collect::<PyResult<_>>()?;
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
fn export_csv(
    py: Python<'_>,
    image: &Icf,
    scope: &Scope,
    locale: &Locale,
    context: &DefaultContext,
) -> PyResult<Export> {
    let (scope, locale, context) = (scope.inner, locale.native()?, context.inner);
    image.with_ref(|image| {
        Ok(Export {
            inner: py.detach(|| core::export_csv(image, scope, locale, context)),
        })
    })
}
#[pyfunction]
fn export_working_csv(
    py: Python<'_>,
    image: &Working,
    scope: &Scope,
    locale: &Locale,
) -> PyResult<Export> {
    let (scope, locale) = (scope.inner, locale.native()?);
    image.inner.with_ref(|image| {
        Ok(Export {
            inner: py.detach(|| core::export_working_csv(image, scope, locale)),
        })
    })
}
#[pyfunction]
fn import_csv(
    py: Python<'_>,
    image: &Icf,
    scope: &Scope,
    source: &Bound<'_, PyAny>,
    options: Py<ImportOptions>,
) -> PyResult<Py<Import>> {
    let source = strict_bytes(source)?.to_vec();
    let inputs = ImportInputs::from_options(py, &options)?;
    let scope = scope.inner;
    let (native, errors) = image.with_mut(|image| {
        inputs.detached(py, |options, _| {
            core::import_csv(image, scope, &source, options)
        })
    })?;
    finish_report(py, errors, Py::new(py, Import { inner: native }))
}
#[pyfunction]
fn import_working_csv(
    py: Python<'_>,
    image: &Working,
    scope: &Scope,
    source: &Bound<'_, PyAny>,
    options: Py<ImportOptions>,
    nls: Py<PyAny>,
) -> PyResult<Py<Import>> {
    let interpreter = crate::scoped::interpreter(py)?;
    let source = strict_bytes(source)?.to_vec();
    let inputs = ImportInputs::from_options(py, &options)?;
    let scope = scope.inner;
    let (native, errors) = image.inner.with_mut(|image| {
        inputs.detached(py, |options, errors| {
            core::import_working_csv(image, scope, &source, options, |s, d| {
                let result =
                    attach_checked(interpreter, |py| super::parser::call_nls(py, s, d, &nls));
                errors.borrow_mut().capture(result).map_err(|_| {
                    cs705_core::csv::Error::Operation("repeater import NLS callback failed".into())
                })
            })
        })
    })?;
    finish_report(py, errors, Py::new(py, Import { inner: native }))
}
#[pyfunction]
fn import_csv_merge(
    py: Python<'_>,
    image: &Working,
    scope: &Scope,
    source: &Bound<'_, PyAny>,
    options: Py<super::parser::Options>,
    reset: &Bound<'_, PyAny>,
    nls: Py<PyAny>,
) -> PyResult<Py<Import>> {
    let interpreter = crate::scoped::interpreter(py)?;
    let (source, reset) = (strict_bytes(source)?.to_vec(), super::groups::words(reset)?);
    let inputs = super::parser::Inputs::from_options(py, &options)?;
    let scope = scope.inner;
    let (native, errors) = image.inner.with_mut(|image| {
        inputs.detached(py, |options, errors| {
            core::import_csv_merge(image, scope, &source, options, reset, |s, d| {
                let result =
                    attach_checked(interpreter, |py| super::parser::call_nls(py, s, d, &nls));
                errors.borrow_mut().capture(result).map_err(|_| {
                    cs705_core::csv::Error::Operation("repeater merge NLS callback failed".into())
                })
            })
        })
    })?;
    finish_report(py, errors, Py::new(py, Import { inner: native }))
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "transfer")?;
    m.add_class::<Scope>()?;
    m.add_class::<Failure>()?;
    m.add_class::<Export>()?;
    m.add_class::<Import>()?;
    m.add_class::<ImportOptions>()?;
    m.add_class::<UnwrittenSource>()?;
    super::fields::add_enum(&m, "ImportMode", &["Merge", "PreserveUse", "Replace"])?;
    m.add_function(wrap_pyfunction!(export_csv, &m)?)?;
    m.add_function(wrap_pyfunction!(export_working_csv, &m)?)?;
    m.add_function(wrap_pyfunction!(import_csv, &m)?)?;
    m.add_function(wrap_pyfunction!(import_working_csv, &m)?)?;
    m.add_function(wrap_pyfunction!(import_csv_merge, &m)?)?;
    root.add_submodule(&m)
}
