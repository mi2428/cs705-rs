//! Lazy native repeater parser inputs and results.
use super::{fields::TypeSource, finalize::OffsetProfile, staging::Record};
use crate::{
    callbacks::{CallbackError, CallbackHolder},
    conversion::*,
    csv::{Locale, datetime::Context},
    errors,
    owner::Owner,
};
use cs705_core::repeater::parser as core;
use pyo3::types::PyBytes;
use pyo3::{PyTraverseError, PyVisit, prelude::*};
use std::cell::RefCell;

pub(crate) fn call_nls(
    py: Python<'_>,
    source: &[u8],
    destination: &mut [u8],
    nls: &Py<PyAny>,
) -> PyResult<i32> {
    crate::csv::nls::map_call(py, nls.bind(py), source, destination)
}

pub(crate) struct Inputs {
    pub(crate) locale: cs705_core::csv::Locale,
    pub(crate) datetime: Py<Context>,
    pub(crate) default: Py<Record>,
    pub(crate) types: cs705_core::repeater::fields::TypeSource,
    pub(crate) group_name_seed: [u8; 16],
    pub(crate) crt_decimal: u8,
    pub(crate) frequency: crate::callbacks::CallbackLink,
    // Keep the GC-visible holder alive while the native closure uses a weak link.
    _holder: Py<CallbackHolder>,
}
impl Inputs {
    fn profile(
        &self,
        py: Python<'_>,
        hz: u32,
        errors: &RefCell<CallbackError>,
    ) -> Result<core::FrequencyProfile, cs705_core::csv::Error> {
        let result = self.frequency.with_callable(py, |c| {
            Ok(c.call1((hz,))?
                .extract::<PyRef<'_, FrequencyProfile>>()?
                .inner)
        });
        errors.borrow_mut().capture(result).map_err(|_| {
            cs705_core::csv::Error::Operation("repeater frequency profile callback failed".into())
        })
    }
    pub(crate) fn from_options(py: Python<'_>, options: &Py<Options>) -> PyResult<Self> {
        let o = options.try_borrow(py)?;
        let holder = o
            .frequency
            .as_ref()
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("repeater parser callback was cleared")
            })?
            .clone_ref(py);
        let types = o.types.try_borrow(py)?.inner;
        Ok(Self {
            locale: o.locale,
            datetime: o.datetime.clone_ref(py),
            default: o.default.clone_ref(py),
            types,
            group_name_seed: o.group_name_seed,
            crt_decimal: o.crt_decimal,
            frequency: CallbackHolder::link(holder.bind(py)),
            _holder: holder,
        })
    }
    pub(crate) fn with_core<R>(
        &self,
        py: Python<'_>,
        errors: &RefCell<CallbackError>,
        f: impl FnOnce(&core::Options<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let frequency = |hz| self.profile(py, hz, errors);
        self.default.get().inner.with_ref(|default| {
            self.datetime.get().with_context(|datetime| {
                f(&core::Options {
                    locale: self.locale,
                    datetime,
                    default,
                    types: self.types,
                    group_name_seed: self.group_name_seed,
                    crt_decimal: self.crt_decimal,
                    frequency: &frequency,
                })
            })
        })
    }
    pub(crate) fn detached<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&core::Options<'_>, &RefCell<CallbackError>) -> R + Send,
    ) -> PyResult<(R, CallbackError)> {
        self.default.get().inner.with_ref(|default| {
            self.datetime.get().with_context(|datetime| {
                Ok(py.detach(|| {
                    let errors = RefCell::new(CallbackError::default());
                    let frequency = |hz| Python::attach(|py| self.profile(py, hz, &errors));
                    let value = f(
                        &core::Options {
                            locale: self.locale,
                            datetime,
                            default,
                            types: self.types,
                            group_name_seed: self.group_name_seed,
                            crt_decimal: self.crt_decimal,
                            frequency: &frequency,
                        },
                        &errors,
                    );
                    (value, errors.into_inner())
                }))
            })
        })
    }
}

/// Offset is read as a snapshot and replaced with an explicit setter.
#[pyclass(module = "cs705.repeater.parser")]
pub struct FrequencyProfile {
    pub(crate) inner: core::FrequencyProfile,
}
#[pymethods]
impl FrequencyProfile {
    #[new]
    fn new(identity_class: &Bound<'_, PyAny>, offset: &OffsetProfile) -> PyResult<Self> {
        Ok(Self {
            inner: core::FrequencyProfile {
                identity_class: strict_u32(identity_class)?,
                offset: offset.inner,
            },
        })
    }
    #[getter]
    fn identity_class(&self) -> u32 {
        self.inner.identity_class
    }
    #[setter]
    fn set_identity_class(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.identity_class = strict_u32(v)?;
        Ok(())
    }
    fn offset_snapshot(&self) -> OffsetProfile {
        OffsetProfile {
            inner: self.inner.offset,
        }
    }
    fn set_offset(&mut self, value: &OffsetProfile) {
        self.inner.offset = value.inner;
    }
}
/// Locale is an explicit snapshot; referenced sources retain their owners.
#[pyclass(module = "cs705.repeater.parser")]
pub struct Options {
    pub(crate) locale: cs705_core::csv::Locale,
    pub(crate) datetime: Py<Context>,
    pub(crate) default: Py<Record>,
    pub(crate) types: Py<TypeSource>,
    pub(crate) group_name_seed: [u8; 16],
    pub(crate) crt_decimal: u8,
    pub(crate) frequency: Option<Py<CallbackHolder>>,
}
#[pymethods]
impl Options {
    #[new]
    fn new(
        py: Python<'_>,
        locale: &Locale,
        datetime: Py<Context>,
        default: Py<Record>,
        types: Py<TypeSource>,
        group_name_seed: &Bound<'_, PyAny>,
        crt_decimal: &Bound<'_, PyAny>,
        frequency: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            locale: locale.native()?,
            datetime,
            default,
            types,
            group_name_seed: fixed_bytes(group_name_seed)?,
            crt_decimal: strict_u8(crt_decimal)?,
            frequency: Some(CallbackHolder::new(py, frequency)?),
        })
    }
    fn locale_snapshot(&self) -> Locale {
        Locale::from_core(self.locale)
    }
    fn set_locale(&mut self, value: &Locale) -> PyResult<()> {
        self.locale = value.native()?;
        Ok(())
    }
    #[getter]
    fn datetime(&self, py: Python<'_>) -> Py<Context> {
        self.datetime.clone_ref(py)
    }
    #[setter]
    fn set_datetime(&mut self, v: Py<Context>) {
        self.datetime = v;
    }
    #[getter]
    fn default(&self, py: Python<'_>) -> Py<Record> {
        self.default.clone_ref(py)
    }
    #[setter]
    fn set_default(&mut self, v: Py<Record>) {
        self.default = v;
    }
    #[getter]
    fn types(&self, py: Python<'_>) -> Py<TypeSource> {
        self.types.clone_ref(py)
    }
    #[setter]
    fn set_types(&mut self, v: Py<TypeSource>) {
        self.types = v;
    }
    #[getter]
    fn group_name_seed<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.group_name_seed)
    }
    #[setter]
    fn set_group_name_seed(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.group_name_seed = fixed_bytes(v)?;
        Ok(())
    }
    #[getter]
    fn crt_decimal(&self) -> u8 {
        self.crt_decimal
    }
    #[setter]
    fn set_crt_decimal(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.crt_decimal = strict_u8(v)?;
        Ok(())
    }
    #[getter]
    fn frequency(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let holder = self.frequency.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("repeater parser callback was cleared")
        })?;
        CallbackHolder::link(holder.bind(py)).with_callable(py, |c| Ok(c.clone().unbind()))
    }
    #[setter]
    fn set_frequency(&mut self, py: Python<'_>, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.frequency = Some(CallbackHolder::new(py, v)?);
        Ok(())
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.datetime)?;
        visit.call(&self.default)?;
        visit.call(&self.types)?;
        visit.call(&self.frequency)
    }
    fn __clear__(&mut self) {
        self.frequency = None;
    }
}
/// Record is read as a snapshot and replaced with set_record.
#[pyclass(module = "cs705.repeater.parser")]
pub struct Parsed {
    pub(crate) inner: core::Parsed,
}
#[pymethods]
impl Parsed {
    #[new]
    fn new(
        record: &Record,
        group_name: &Bound<'_, PyAny>,
        corrections: &Bound<'_, PyAny>,
        accepted: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        record.inner.with_ref(|r| {
            Ok(Self {
                inner: core::Parsed {
                    record: r.clone(),
                    group_name: fixed_bytes(group_name)?,
                    corrections: strict_u32(corrections)?,
                    accepted: strict_bool(accepted)?,
                },
            })
        })
    }
    fn record_snapshot(&self) -> Record {
        Record::from_core(self.inner.record.clone())
    }
    fn set_record(&mut self, v: &Record) -> PyResult<()> {
        v.inner.with_ref(|r| {
            self.inner.record = r.clone();
            Ok(())
        })
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
    fn accepted(&self) -> bool {
        self.inner.accepted
    }
    #[setter]
    fn set_accepted(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.accepted = strict_bool(v)?;
        Ok(())
    }
}
#[pyclass(module = "cs705.repeater.parser", frozen)]
pub struct Rows {
    pub(crate) inner: Owner<core::Rows<'static>>,
}
#[pymethods]
impl Rows {
    #[new]
    fn new(source: &Bound<'_, PyAny>, locale: &Locale) -> PyResult<Self> {
        Ok(Self {
            inner: Owner::new(
                core::Rows::new(strict_bytes(source)?, locale.native()?)
                    .map_err(errors::csv)?
                    .into_owned(),
            ),
        })
    }
    fn into_owned(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn next_with(
        &self,
        py: Python<'_>,
        row: &Record,
        options: Py<Options>,
        nls: Py<PyAny>,
    ) -> PyResult<Option<Parsed>> {
        let row = row.inner.with_ref(|r| Ok(r.clone()))?;
        let inputs = Inputs::from_options(py, &options)?;
        let errors = RefCell::new(CallbackError::default());
        let result = inputs.with_core(py, &errors, |options| {
            self.inner.with_mut(|rows| {
                rows.next_with(row, options, |source, destination| {
                    let result = call_nls(py, source, destination, &nls);
                    errors.borrow_mut().capture(result).map_err(|_| {
                        cs705_core::csv::Error::Operation("repeater NLS callback failed".into())
                    })
                })
                .map(|r| r.map(|inner| Parsed { inner }).map_err(errors::csv))
                .transpose()
            })
        });
        errors.into_inner().finish(py, result)
    }
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "parser")?;
    m.add_class::<Options>()?;
    m.add_class::<FrequencyProfile>()?;
    m.add_class::<Parsed>()?;
    m.add_class::<Rows>()?;
    root.add_submodule(&m)
}
