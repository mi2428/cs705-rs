//! Explicit byte locale and live owner projections; no host calendar parser.
use super::*;
use crate::{
    clone::Handle,
    scoped::{Frame, Loan},
};
use cs705_core::csv::datetime as d;

#[pyclass(module = "cs705.csv.datetime", frozen)]
pub struct Date {
    inner: Handle<d::Date>,
}
impl Date {
    pub(crate) fn from_core(value: d::Date) -> Self {
        Self {
            inner: Handle::new(value),
        }
    }
    fn from_handle(inner: Handle<d::Date>) -> Self {
        Self { inner }
    }
    pub(crate) fn native(&self) -> PyResult<d::Date> {
        self.inner.with(|v| Ok(*v))
    }
}
#[pymethods]
impl Date {
    #[new]
    fn new(
        year: &Bound<'_, PyAny>,
        month: &Bound<'_, PyAny>,
        day: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self::from_core(d::Date {
            year: strict_u16(year)?,
            month: strict_u8(month)?,
            day: strict_u8(day)?,
        }))
    }
    fn __eq__(&self, other: &Self) -> PyResult<bool> {
        Ok(self.native()? == other.native()?)
    }
    #[getter]
    fn year(&self) -> PyResult<u16> {
        self.inner.with(|v| Ok(v.year))
    }
    #[getter]
    fn month(&self) -> PyResult<u8> {
        self.inner.with(|v| Ok(v.month))
    }
    #[getter]
    fn day(&self) -> PyResult<u8> {
        self.inner.with(|v| Ok(v.day))
    }
    #[setter]
    fn set_year(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_u16(v)?;
        self.inner.with(|d| {
            d.year = v;
            Ok(())
        })
    }
    #[setter]
    fn set_month(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_u8(v)?;
        self.inner.with(|d| {
            d.month = v;
            Ok(())
        })
    }
    #[setter]
    fn set_day(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_u8(v)?;
        self.inner.with(|d| {
            d.day = v;
            Ok(())
        })
    }
}
#[pyclass(module = "cs705.csv.datetime")]
#[derive(Clone, Copy)]
pub struct Time {
    pub(crate) inner: d::Time,
}
#[pymethods]
impl Time {
    #[new]
    fn new(
        hour: &Bound<'_, PyAny>,
        minute: &Bound<'_, PyAny>,
        second: &Bound<'_, PyAny>,
        millisecond: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: d::Time {
                hour: strict_u8(hour)?,
                minute: strict_u8(minute)?,
                second: strict_u8(second)?,
                millisecond: strict_u16(millisecond)?,
            },
        })
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
    #[getter]
    fn hour(&self) -> u8 {
        self.inner.hour
    }
    #[getter]
    fn minute(&self) -> u8 {
        self.inner.minute
    }
    #[getter]
    fn second(&self) -> u8 {
        self.inner.second
    }
    #[getter]
    fn millisecond(&self) -> u16 {
        self.inner.millisecond
    }
    #[setter]
    fn set_hour(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.hour = strict_u8(v)?;
        Ok(())
    }
    #[setter]
    fn set_minute(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.minute = strict_u8(v)?;
        Ok(())
    }
    #[setter]
    fn set_second(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.second = strict_u8(v)?;
        Ok(())
    }
    #[setter]
    fn set_millisecond(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.millisecond = strict_u16(v)?;
        Ok(())
    }
}
enum EraTarget {
    Owned(Handle<d::OwnedEra>),
    Scoped {
        source: Loan<Vec<d::OwnedEra>>,
        index: usize,
    },
}
#[pyclass(module = "cs705.csv.datetime", frozen)]
pub struct Era {
    target: EraTarget,
}
impl Era {
    fn with_ref<T>(&self, f: impl FnOnce(&d::OwnedEra) -> PyResult<T>) -> PyResult<T> {
        match &self.target {
            EraTarget::Owned(h) => h.with(|e| f(e)),
            EraTarget::Scoped { source, index } => Python::attach(|py| {
                source.with_ref(py, |v| {
                    f(v.get(*index).ok_or_else(|| {
                        pyo3::exceptions::PyReferenceError::new_err("era field was removed")
                    })?)
                })
            }),
        }
    }
    fn with_mut<T>(&self, f: impl FnOnce(&mut d::OwnedEra) -> PyResult<T>) -> PyResult<T> {
        match &self.target {
            EraTarget::Owned(h) => h.with(f),
            EraTarget::Scoped { source, index } => Python::attach(|py| {
                source.with_mut(py, |v| {
                    f(v.get_mut(*index).ok_or_else(|| {
                        pyo3::exceptions::PyReferenceError::new_err("era field was removed")
                    })?)
                })
            }),
        }
    }
    fn native(&self) -> PyResult<d::OwnedEra> {
        self.with_ref(|e| Ok(e.clone()))
    }
}
#[pymethods]
impl Era {
    #[new]
    fn new(name: &Bound<'_, PyAny>, offset: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            target: EraTarget::Owned(Handle::new(d::OwnedEra {
                name: if name.is_none() {
                    None
                } else {
                    Some(strict_bytes(name)?.to_vec())
                },
                offset: strict_i32(offset)?,
            })),
        })
    }
    #[getter]
    fn name<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyBytes>>> {
        self.with_ref(|v| Ok(v.name.as_ref().map(|v| PyBytes::new(py, v))))
    }
    #[getter]
    fn offset(&self) -> PyResult<i32> {
        self.with_ref(|v| Ok(v.offset))
    }
    #[setter]
    fn set_name(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = if v.is_none() {
            None
        } else {
            Some(strict_bytes(v)?.to_vec())
        };
        self.with_mut(|e| {
            e.name = v;
            Ok(())
        })
    }
    #[setter]
    fn set_offset(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_i32(v)?;
        self.with_mut(|e| {
            e.offset = v;
            Ok(())
        })
    }
    fn into_owned(&self) -> PyResult<Self> {
        Ok(Self {
            target: EraTarget::Owned(Handle::new(self.native()?)),
        })
    }
}
enum ContextTarget {
    Owned(Handle<d::OwnedContext>),
    Scoped(Loan<d::OwnedContext>),
}
#[pyclass(module = "cs705.csv.datetime", frozen)]
pub struct Context {
    target: ContextTarget,
}
impl Context {
    pub fn from_core(value: d::OwnedContext) -> Self {
        Self::from_handle(Handle::new(value))
    }
    pub(crate) fn from_handle(value: Handle<d::OwnedContext>) -> Self {
        Self {
            target: ContextTarget::Owned(value),
        }
    }
    fn with_ref<T>(&self, f: impl FnOnce(&d::OwnedContext) -> PyResult<T>) -> PyResult<T> {
        match &self.target {
            ContextTarget::Owned(h) => h.with(|v| f(v)),
            ContextTarget::Scoped(l) => Python::attach(|py| l.with_ref(py, f)),
        }
    }
    fn with_mut<T>(&self, f: impl FnOnce(&mut d::OwnedContext) -> PyResult<T>) -> PyResult<T> {
        match &self.target {
            ContextTarget::Owned(h) => h.with(f),
            ContextTarget::Scoped(l) => Python::attach(|py| l.with_mut(py, f)),
        }
    }
    pub fn to_core(&self) -> PyResult<d::OwnedContext> {
        self.with_ref(|v| Ok(v.clone()))
    }
    pub fn with_context<T>(&self, f: impl FnOnce(&d::Context<'_>) -> PyResult<T>) -> PyResult<T> {
        self.with_ref(|v| v.with_context(f))
    }
}
#[pymethods]
impl Context {
    #[new]
    fn new(
        short_date: &Bound<'_, PyAny>,
        date_separator: &Bound<'_, PyAny>,
        time_separator: &Bound<'_, PyAny>,
        decimal_separator: &Bound<'_, PyAny>,
        am: &Bound<'_, PyAny>,
        pm: &Bound<'_, PyAny>,
        current_year: &Bound<'_, PyAny>,
        two_digit_window: &Bound<'_, PyAny>,
        primary_language: &Bound<'_, PyAny>,
        eras: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let eras = eras
            .try_iter()?
            .map(|e| e?.extract::<PyRef<'_, Era>>()?.native())
            .collect::<PyResult<_>>()?;
        Ok(Self::from_core(d::OwnedContext {
            short_date: strict_bytes(short_date)?.to_vec(),
            date_separator: strict_u8(date_separator)?,
            time_separator: strict_u8(time_separator)?,
            decimal_separator: strict_u8(decimal_separator)?,
            am: strict_bytes(am)?.to_vec(),
            pm: strict_bytes(pm)?.to_vec(),
            current_year: strict_u16(current_year)?,
            two_digit_window: strict_u16(two_digit_window)?,
            primary_language: strict_u16(primary_language)?,
            eras,
        }))
    }
    #[getter]
    fn short_date<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(|v| Ok(PyBytes::new(py, &v.short_date)))
    }
    #[getter]
    fn am<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(|v| Ok(PyBytes::new(py, &v.am)))
    }
    #[getter]
    fn pm<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(|v| Ok(PyBytes::new(py, &v.pm)))
    }
    #[getter]
    fn date_separator(&self) -> PyResult<u8> {
        self.with_ref(|v| Ok(v.date_separator))
    }
    #[getter]
    fn time_separator(&self) -> PyResult<u8> {
        self.with_ref(|v| Ok(v.time_separator))
    }
    #[getter]
    fn decimal_separator(&self) -> PyResult<u8> {
        self.with_ref(|v| Ok(v.decimal_separator))
    }
    #[getter]
    fn current_year(&self) -> PyResult<u16> {
        self.with_ref(|v| Ok(v.current_year))
    }
    #[getter]
    fn two_digit_window(&self) -> PyResult<u16> {
        self.with_ref(|v| Ok(v.two_digit_window))
    }
    #[getter]
    fn primary_language(&self) -> PyResult<u16> {
        self.with_ref(|v| Ok(v.primary_language))
    }
    #[setter]
    fn set_short_date(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_bytes(v)?.to_vec();
        self.with_mut(|c| {
            c.short_date = v;
            Ok(())
        })
    }
    #[setter]
    fn set_am(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_bytes(v)?.to_vec();
        self.with_mut(|c| {
            c.am = v;
            Ok(())
        })
    }
    #[setter]
    fn set_pm(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_bytes(v)?.to_vec();
        self.with_mut(|c| {
            c.pm = v;
            Ok(())
        })
    }
    #[setter]
    fn set_date_separator(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_u8(v)?;
        self.with_mut(|c| {
            c.date_separator = v;
            Ok(())
        })
    }
    #[setter]
    fn set_time_separator(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_u8(v)?;
        self.with_mut(|c| {
            c.time_separator = v;
            Ok(())
        })
    }
    #[setter]
    fn set_decimal_separator(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_u8(v)?;
        self.with_mut(|c| {
            c.decimal_separator = v;
            Ok(())
        })
    }
    #[setter]
    fn set_current_year(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_u16(v)?;
        self.with_mut(|c| {
            c.current_year = v;
            Ok(())
        })
    }
    #[setter]
    fn set_two_digit_window(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_u16(v)?;
        self.with_mut(|c| {
            c.two_digit_window = v;
            Ok(())
        })
    }
    #[setter]
    fn set_primary_language(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_u16(v)?;
        self.with_mut(|c| {
            c.primary_language = v;
            Ok(())
        })
    }
    #[getter]
    fn eras<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyTuple>> {
        let count = self.with_ref(|v| Ok(v.eras.len()))?;
        let mut values = Vec::with_capacity(count);
        for index in 0..count {
            let target = match &self.target {
                ContextTarget::Owned(h) => EraTarget::Owned(h.project(move |v| {
                    v.eras.get_mut(index).ok_or_else(|| {
                        pyo3::exceptions::PyReferenceError::new_err("era field was removed")
                    })
                })),
                ContextTarget::Scoped(l) => EraTarget::Scoped {
                    source: l.project_ref(py, |v| &v.eras)?,
                    index,
                },
            };
            values.push(Py::new(py, Era { target })?);
        }
        pyo3::types::PyTuple::new(py, values)
    }
    #[setter]
    fn set_eras(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let eras = value
            .try_iter()?
            .map(|v| v?.extract::<PyRef<'_, Era>>()?.native())
            .collect::<PyResult<_>>()?;
        match &self.target {
            ContextTarget::Owned(h) => h.edit(|v| {
                v.eras = eras;
                Ok(())
            }),
            ContextTarget::Scoped(l) => Python::attach(|py| {
                l.with_mut(py, |v| {
                    v.eras = eras;
                    Ok(())
                })
            }),
        }
    }
    fn into_owned(&self) -> PyResult<Self> {
        Ok(Self::from_core(self.to_core()?))
    }
    #[pyo3(name = "with_context")]
    fn py_with_context<'py>(
        &self,
        py: Python<'py>,
        call: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.with_ref(|v| {
            Frame::with_ref(py, v, |loan| {
                call.call1((Self {
                    target: ContextTarget::Scoped(loan),
                },))
            })
        })
    }
}
#[pyclass(module = "cs705.csv.datetime", frozen)]
pub struct DateToken {
    inner: Handle<d::DateToken>,
}
impl DateToken {
    pub(crate) fn from_core(value: d::DateToken) -> Self {
        Self {
            inner: Handle::new(value),
        }
    }
}
#[pymethods]
impl DateToken {
    #[new]
    fn new(date: &Date, letter: &Bound<'_, PyAny>, count: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(d::DateToken {
            date: date.native()?,
            letter: strict_u8(letter)?,
            count: strict_usize(count)?,
        }))
    }
    fn __eq__(&self, other: &Self) -> PyResult<bool> {
        Ok(self.inner.with(|v| Ok(*v))? == other.inner.with(|v| Ok(*v))?)
    }
    #[getter]
    fn date(&self) -> Date {
        Date::from_handle(self.inner.project(|v| Ok(&mut v.date)))
    }
    #[getter]
    fn letter(&self) -> PyResult<u8> {
        self.inner.with(|v| Ok(v.letter))
    }
    #[getter]
    fn count(&self) -> PyResult<usize> {
        self.inner.with(|v| Ok(v.count))
    }
    #[setter]
    fn set_date(&self, value: &Date) -> PyResult<()> {
        let value = value.native()?;
        self.inner.edit(|v| {
            v.date = value;
            Ok(())
        })
    }
    #[setter]
    fn set_letter(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u8(value)?;
        self.inner.with(|v| {
            v.letter = value;
            Ok(())
        })
    }
    #[setter]
    fn set_count(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_usize(value)?;
        self.inner.with(|v| {
            v.count = value;
            Ok(())
        })
    }
}
#[pyfunction]
fn valid_date(date: &Date) -> PyResult<bool> {
    Ok(d::valid_date(date.native()?))
}
#[pyfunction]
fn valid_time(time: &Time) -> bool {
    d::valid_time(time.inner)
}
#[pyfunction]
fn parse_date(source: &Bound<'_, PyAny>, context: &Context) -> PyResult<Date> {
    let source = strict_bytes(source)?;
    context.with_context(|c| {
        d::parse_date(source, c)
            .map(Date::from_core)
            .map_err(crate::errors::csv)
    })
}
#[pyfunction]
fn parse_time(source: &Bound<'_, PyAny>, context: &Context) -> PyResult<Time> {
    let source = strict_bytes(source)?;
    context.with_context(|c| {
        d::parse_time(source, c)
            .map(|inner| Time { inner })
            .map_err(crate::errors::csv)
    })
}
#[pyfunction]
fn parse_date_with(
    py: Python<'_>,
    source: &Bound<'_, PyAny>,
    context: &Context,
    equal: &Bound<'_, PyAny>,
) -> PyResult<Date> {
    let source = strict_bytes(source)?;
    let mut errors = crate::callbacks::CallbackError::default();
    let result = context.with_context(|c| {
        d::parse_date_with(source, c, |a, b| {
            callback(
                &mut errors,
                equal
                    .call1((PyBytes::new(py, a), PyBytes::new(py, b)))
                    .and_then(|v| strict_bool(&v)),
            )
        })
        .map(Date::from_core)
        .map_err(crate::errors::csv)
    });
    errors.finish(py, result)
}
#[pyfunction]
fn parse_time_with(
    py: Python<'_>,
    source: &Bound<'_, PyAny>,
    context: &Context,
    equal: &Bound<'_, PyAny>,
) -> PyResult<Time> {
    let source = strict_bytes(source)?;
    let mut errors = crate::callbacks::CallbackError::default();
    let result = context.with_context(|c| {
        d::parse_time_with(source, c, |a, b| {
            callback(
                &mut errors,
                equal
                    .call1((PyBytes::new(py, a), PyBytes::new(py, b)))
                    .and_then(|v| strict_bool(&v)),
            )
        })
        .map(|inner| Time { inner })
        .map_err(crate::errors::csv)
    });
    errors.finish(py, result)
}
#[pyfunction]
fn format_date<'py>(
    py: Python<'py>,
    date: &Date,
    format: &Bound<'_, PyAny>,
    context: &Context,
    native: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let (date, format) = (date.native()?, strict_bytes(format)?);
    let mut errors = crate::callbacks::CallbackError::default();
    let result = context.with_context(|c| {
        d::format_date(date, format, c, |token| {
            callback(
                &mut errors,
                native
                    .call1((DateToken::from_core(token),))
                    .and_then(|v| Ok(strict_bytes(&v)?.to_vec())),
            )
        })
        .map_err(crate::errors::csv)
    });
    errors.finish(py, result).map(|v| PyBytes::new(py, &v))
}
#[pyfunction]
fn gps_date<'py>(
    py: Python<'py>,
    raw: &Bound<'_, PyAny>,
    offset: &Bound<'_, PyAny>,
    context: &Context,
    failed_time: &Time,
    native: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let (raw, offset) = (fixed_bytes(raw)?, strict_i16(offset)?);
    let mut errors = crate::callbacks::CallbackError::default();
    let result = context.with_context(|c| {
        d::gps_date(raw, offset, c, failed_time.inner, |token| {
            callback(
                &mut errors,
                native
                    .call1((DateToken::from_core(token),))
                    .and_then(|v| Ok(strict_bytes(&v)?.to_vec())),
            )
        })
        .map_err(crate::errors::csv)
    });
    errors.finish(py, result).map(|v| PyBytes::new(py, &v))
}
#[pyfunction]
fn shift_minutes(date: &Date, time: &Time, minutes: &Bound<'_, PyAny>) -> PyResult<(Date, Time)> {
    let (d, t) = d::shift_minutes(date.native()?, time.inner, strict_i16(minutes)?)
        .map_err(crate::errors::csv)?;
    Ok((Date::from_core(d), Time { inner: t }))
}
#[pyfunction]
fn decode_bcd(byte: &Bound<'_, PyAny>) -> PyResult<u8> {
    Ok(d::decode_bcd(strict_u8(byte)?))
}
#[pyfunction]
fn encode_bcd(byte: &Bound<'_, PyAny>) -> PyResult<u8> {
    Ok(d::encode_bcd(strict_u8(byte)?))
}
#[pyfunction]
fn decode_raw(raw: &Bound<'_, PyAny>) -> PyResult<(Date, Time)> {
    let (d, t) = d::decode_raw(fixed_bytes(raw)?);
    Ok((Date::from_core(d), Time { inner: t }))
}
#[pyfunction]
fn import_raw(
    raw: &MutableBytes,
    date: &Bound<'_, PyAny>,
    time: &Bound<'_, PyAny>,
    context: &Context,
) -> PyResult<()> {
    let date = if date.is_none() {
        None
    } else {
        Some(strict_bytes(date)?)
    };
    let time = if time.is_none() {
        None
    } else {
        Some(strict_bytes(time)?)
    };
    raw.with_mut(|raw| {
        let raw = raw
            .try_into()
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("expected exactly 6 bytes"))?;
        context.with_context(|c| {
            d::import_raw(raw, date, time, c);
            Ok(())
        })
    })
}
#[pyfunction]
fn effective_offset(minutes: &Bound<'_, PyAny>) -> PyResult<i16> {
    d::effective_offset(strict_i16(minutes)?).map_err(crate::errors::csv)
}
#[pyfunction]
fn finalize_gps_raw(raw: &MutableBytes, minutes: &Bound<'_, PyAny>) -> PyResult<bool> {
    let minutes = strict_i16(minutes)?;
    raw.with_mut(|raw| {
        d::finalize_gps_raw(
            raw.try_into()
                .map_err(|_| pyo3::exceptions::PyValueError::new_err("expected exactly 6 bytes"))?,
            minutes,
        )
        .map_err(crate::errors::csv)
    })
}
#[pyfunction]
fn fixed_time<'py>(
    py: Python<'py>,
    time: &Time,
    separator: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &d::fixed_time(time.inner, strict_u8(separator)?),
    ))
}
#[pyfunction]
fn fixed_time_separators<'py>(
    py: Python<'py>,
    time: &Time,
    separators: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &d::fixed_time_separators(time.inner, fixed_bytes(separators)?),
    ))
}
#[pyfunction]
fn gps_time<'py>(
    py: Python<'py>,
    raw: &Bound<'_, PyAny>,
    offset: &Bound<'_, PyAny>,
    context: &Context,
    invalid_format_time: &Time,
) -> PyResult<Bound<'py, PyBytes>> {
    let (raw, offset) = (fixed_bytes(raw)?, strict_i16(offset)?);
    context.with_context(|c| {
        Ok(PyBytes::new(
            py,
            &d::gps_time(raw, offset, c, invalid_format_time.inner).map_err(crate::errors::csv)?,
        ))
    })
}
#[pyfunction]
fn timestamp<'py>(
    py: Python<'py>,
    raw: &Bound<'_, PyAny>,
    offset: &Bound<'_, PyAny>,
    separators: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &d::timestamp(
            fixed_bytes(raw)?,
            strict_i16(offset)?,
            fixed_bytes(separators)?,
        ),
    ))
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "datetime")?;
    m.add_class::<Date>()?;
    m.add_class::<Time>()?;
    m.add_class::<Era>()?;
    m.add_class::<Context>()?;
    m.add_class::<DateToken>()?;
    m.add("OwnedEra", m.getattr("Era")?)?;
    m.add("OwnedContext", m.getattr("Context")?)?;
    m.add_function(wrap_pyfunction!(valid_date, &m)?)?;
    m.add_function(wrap_pyfunction!(valid_time, &m)?)?;
    m.add_function(wrap_pyfunction!(parse_date, &m)?)?;
    m.add_function(wrap_pyfunction!(parse_time, &m)?)?;
    m.add_function(wrap_pyfunction!(parse_date_with, &m)?)?;
    m.add_function(wrap_pyfunction!(parse_time_with, &m)?)?;
    m.add_function(wrap_pyfunction!(format_date, &m)?)?;
    m.add_function(wrap_pyfunction!(gps_date, &m)?)?;
    m.add_function(wrap_pyfunction!(shift_minutes, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_bcd, &m)?)?;
    m.add_function(wrap_pyfunction!(encode_bcd, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_raw, &m)?)?;
    m.add_function(wrap_pyfunction!(import_raw, &m)?)?;
    m.add_function(wrap_pyfunction!(effective_offset, &m)?)?;
    m.add_function(wrap_pyfunction!(finalize_gps_raw, &m)?)?;
    m.add_function(wrap_pyfunction!(fixed_time, &m)?)?;
    m.add_function(wrap_pyfunction!(fixed_time_separators, &m)?)?;
    m.add_function(wrap_pyfunction!(gps_time, &m)?)?;
    m.add_function(wrap_pyfunction!(timestamp, &m)?)?;
    root.add_submodule(&m)
}
