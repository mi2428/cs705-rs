//! Lazy GPS parsing and explicit finalization; no host date/locale defaults.
use super::Record;
use crate::{
    callbacks::CallbackError,
    conversion::*,
    csv::{Locale, datetime::Context, dispatch::Binding},
    errors,
    owner::Owner,
};
use cs705_core::gps::parser as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

pub(crate) struct OptionsData {
    locale: cs705_core::csv::Locale,
    datetime: cs705_core::csv::datetime::OwnedContext,
    hemispheres: [u8; 2],
    group_name_seed: [u8; 16],
}
impl OptionsData {
    pub(crate) fn with_core<R>(&self, f: impl FnOnce(core::Options<'_>) -> R) -> R {
        self.datetime.with_context(|datetime| {
            f(core::Options {
                locale: self.locale,
                datetime: datetime.clone(),
                hemispheres: self.hemispheres,
                group_name_seed: self.group_name_seed,
            })
        })
    }
}
/// Locale and datetime are read as snapshots and replaced with explicit setters.
#[pyclass(module = "cs705.gps.parser", frozen)]
pub struct Options {
    pub(crate) inner: Owner<OptionsData>,
}
#[pymethods]
impl Options {
    #[new]
    fn new(
        locale: &Locale,
        datetime: &Context,
        hemispheres: &Bound<'_, PyAny>,
        group_name_seed: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Owner::new(OptionsData {
                locale: locale.native()?,
                datetime: datetime.to_core()?,
                hemispheres: fixed_bytes(hemispheres)?,
                group_name_seed: fixed_bytes(group_name_seed)?,
            }),
        })
    }
    /// Return an independent value; use set_locale to replace the native field.
    fn locale_snapshot(&self) -> PyResult<Locale> {
        self.inner.with_ref(|v| Ok(Locale::from_core(v.locale)))
    }
    fn set_locale(&self, value: &Locale) -> PyResult<()> {
        self.inner.with_mut(|o| {
            o.locale = value.native()?;
            Ok(())
        })
    }
    /// Return an independent value; use set_datetime to replace the native field.
    fn datetime_snapshot(&self) -> PyResult<Context> {
        self.inner
            .with_ref(|v| Ok(Context::from_core(v.datetime.clone())))
    }
    fn set_datetime(&self, value: &Context) -> PyResult<()> {
        self.inner.with_mut(|o| {
            o.datetime = value.to_core()?;
            Ok(())
        })
    }
    #[getter]
    fn hemispheres<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.inner
            .with_ref(|v| Ok(PyBytes::new(py, &v.hemispheres)))
    }
    #[setter]
    fn set_hemispheres(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = fixed_bytes(v)?;
        self.inner.with_mut(|o| {
            o.hemispheres = value;
            Ok(())
        })
    }
    #[getter]
    fn group_name_seed<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.inner
            .with_ref(|v| Ok(PyBytes::new(py, &v.group_name_seed)))
    }
    #[setter]
    fn set_group_name_seed(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = fixed_bytes(v)?;
        self.inner.with_mut(|o| {
            o.group_name_seed = value;
            Ok(())
        })
    }
}

/// Owned lazy cursor; into_owned() returns the same cursor without advancing it.
#[pyclass(module = "cs705.gps.parser", frozen)]
pub struct Rows {
    inner: Owner<core::Rows<'static, 'static>>,
}
#[pymethods]
impl Rows {
    #[new]
    fn new(source: &Bound<'_, PyAny>, options: &Options) -> PyResult<Self> {
        let source = strict_bytes(source)?;
        options.inner.with_ref(|o| {
            o.with_core(|options| {
                Ok(Self {
                    inner: Owner::new(
                        core::Rows::new(source, options)
                            .map_err(errors::csv)?
                            .into_owned(),
                    ),
                })
            })
        })
    }
    #[staticmethod]
    fn with_lower(
        py: Python<'_>,
        source: &Bound<'_, PyAny>,
        options: &Options,
        lower: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let source = strict_bytes(source)?;
        let mut error = CallbackError::default();
        let native = options.inner.with_ref(|o| {
            o.with_core(|options| {
                core::Rows::with_lower(source, options, |bytes| {
                    let result = lower
                        .call1((PyBytes::new(py, bytes),))
                        .and_then(|v| Ok(strict_bytes(&v)?.to_vec()));
                    error.capture(result).map_err(|_| {
                        cs705_core::csv::Error::Operation(
                            "GPS header lowercase callback failed".into(),
                        )
                    })
                })
                .map(|rows| Self {
                    inner: Owner::new(rows.into_owned()),
                })
                .map_err(errors::csv)
            })
        });
        error.finish(py, native)
    }
    fn binding(&self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<Py<PyAny>> {
        self.inner.with_ref(|rows| {
            crate::scoped::Frame::with_ref(py, rows.binding(), |loan| {
                Ok(callback
                    .bind(py)
                    .call1((Binding::from_loan(loan),))?
                    .unbind())
            })
        })
    }
    fn into_owned(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&self) -> PyResult<Option<Draft>> {
        self.inner.with_mut(|rows| {
            rows.next()
                .map(|r| r.map(|inner| Draft { inner }).map_err(errors::csv))
                .transpose()
        })
    }
}

/// Record is read as a snapshot and replaced with set_record.
#[pyclass(module = "cs705.gps.parser")]
pub struct Draft {
    pub(crate) inner: core::Draft,
}
#[pymethods]
impl Draft {
    #[new]
    #[pyo3(signature=(record, group_code, group_name, text_corrections, alarm_code, altitude_error))]
    fn new(
        record: &Record,
        group_code: &Bound<'_, PyAny>,
        group_name: &Bound<'_, PyAny>,
        text_corrections: &Bound<'_, PyAny>,
        alarm_code: Option<&Bound<'_, PyAny>>,
        altitude_error: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::Draft {
                record: record.inner,
                group_code: strict_u32(group_code)?,
                group_name: fixed_bytes(group_name)?,
                text_corrections: strict_u32(text_corrections)?,
                alarm_code: alarm_code.map(strict_u32).transpose()?,
                altitude_error: altitude_error
                    .map(|value| {
                        super::transfer::native_error(
                            value,
                            crate::diagnostic::Diagnostic::csv_error,
                        )
                    })
                    .transpose()?,
            },
        })
    }
    fn record_snapshot(&self) -> Record {
        Record {
            inner: self.inner.record,
        }
    }
    fn set_record(&mut self, record: &Record) {
        self.inner.record = record.inner;
    }
    #[getter]
    fn group_code(&self) -> u32 {
        self.inner.group_code
    }
    #[setter]
    fn set_group_code(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.group_code = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn group_name<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.group_name)
    }
    #[setter]
    fn set_group_name(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.group_name = fixed_bytes(value)?;
        Ok(())
    }
    #[getter]
    fn text_corrections(&self) -> u32 {
        self.inner.text_corrections
    }
    #[setter]
    fn set_text_corrections(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.text_corrections = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn alarm_code(&self) -> Option<u32> {
        self.inner.alarm_code
    }
    #[setter]
    fn set_alarm_code(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.inner.alarm_code = value.map(strict_u32).transpose()?;
        Ok(())
    }
    #[getter]
    fn altitude_error(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.inner
            .altitude_error
            .clone()
            .map(|e| errors::csv(e).into_value(py).into_any())
    }
    #[setter]
    fn set_altitude_error(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.inner.altitude_error = value
            .map(|value| {
                super::transfer::native_error(value, crate::diagnostic::Diagnostic::csv_error)
            })
            .transpose()?;
        Ok(())
    }
    fn finalize(
        &mut self,
        whole: &Bound<'_, PyAny>,
        offset: &Bound<'_, PyAny>,
        alarm_seen: &Bool,
    ) -> PyResult<super::transfer::Parsed> {
        let (whole, offset) = (strict_bool(whole)?, strict_i16(offset)?);
        alarm_seen.with_mut(|seen| {
            Ok(super::transfer::Parsed {
                inner: self
                    .inner
                    .finalize(whole, offset, seen)
                    .map_err(errors::csv)?,
            })
        })
    }
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "parser")?;
    m.add_class::<Draft>()?;
    m.add_class::<Options>()?;
    m.add_class::<Rows>()?;
    m.add_function(wrap_pyfunction!(super::valid_coordinate, &m)?)?;
    root.add_submodule(&m)
}
