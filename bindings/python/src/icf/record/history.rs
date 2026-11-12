//! History sequencing and readonly projections over the canonical holder.
use super::*;
use crate::errors;
use crate::icf::{Icf, rows::WorkingUr};
use crate::repeater::working::Working;
use crate::{
    callbacks::{CallbackError, CallbackHolder, CallbackLink},
    scoped::Frame,
};
use pyo3::{PyTraverseError, PyVisit};

#[pyclass(module = "cs705.icf.record", frozen)]
pub struct HistoryQuery {
    inner: core::HistoryQuery,
}
#[pymethods]
#[allow(non_snake_case)]
impl HistoryQuery {
    #[staticmethod]
    fn RepeaterCount() -> Self {
        Self {
            inner: core::HistoryQuery::RepeaterCount,
        }
    }
    #[staticmethod]
    fn FromDv(call: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::HistoryQuery::FromDv {
                call: fixed_bytes(call)?,
            },
        })
    }
    #[staticmethod]
    fn FromFm(
        call: &Bound<'_, PyAny>,
        frequency_hz: &Bound<'_, PyAny>,
        limit: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::HistoryQuery::FromFm {
                call: fixed_bytes(call)?,
                frequency_hz: strict_u32(frequency_hz)?,
                limit: strict_i32(limit)?,
            },
        })
    }
    #[staticmethod]
    fn Ur(call: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::HistoryQuery::Ur {
                call: fixed_bytes(call)?,
            },
        })
    }
    #[staticmethod]
    fn UpperUr(call: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::HistoryQuery::UpperUr {
                call: fixed_bytes(call)?,
            },
        })
    }
    #[staticmethod]
    fn ToRepeater(call: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::HistoryQuery::ToRepeater {
                call: fixed_bytes(call)?,
            },
        })
    }
    #[getter]
    fn variant(&self) -> &'static str {
        match self.inner {
            core::HistoryQuery::RepeaterCount => "RepeaterCount",
            core::HistoryQuery::FromDv { .. } => "FromDv",
            core::HistoryQuery::FromFm { .. } => "FromFm",
            core::HistoryQuery::Ur { .. } => "Ur",
            core::HistoryQuery::UpperUr { .. } => "UpperUr",
            core::HistoryQuery::ToRepeater { .. } => "ToRepeater",
        }
    }
    #[getter]
    fn call<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        match self.inner {
            core::HistoryQuery::RepeaterCount => None,
            core::HistoryQuery::FromDv { call }
            | core::HistoryQuery::FromFm { call, .. }
            | core::HistoryQuery::Ur { call }
            | core::HistoryQuery::UpperUr { call }
            | core::HistoryQuery::ToRepeater { call } => Some(PyBytes::new(py, &call)),
        }
    }
    #[getter]
    fn frequency_hz(&self) -> Option<u32> {
        match self.inner {
            core::HistoryQuery::FromFm { frequency_hz, .. } => Some(frequency_hz),
            _ => None,
        }
    }
    #[getter]
    fn limit(&self) -> Option<i32> {
        match self.inner {
            core::HistoryQuery::FromFm { limit, .. } => Some(limit),
            _ => None,
        }
    }
}
#[pyclass(module = "cs705.icf.record", frozen)]
pub struct HistoryLoad {
    inner: core::HistoryLoad,
}
#[pymethods]
#[allow(non_snake_case)]
impl HistoryLoad {
    #[staticmethod]
    fn Repeater(index: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::HistoryLoad::Repeater(strict_i32(index)?),
        })
    }
    #[staticmethod]
    fn Ur(index: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::HistoryLoad::Ur(strict_u32(index)?),
        })
    }
    #[getter]
    fn variant(&self) -> &'static str {
        match self.inner {
            core::HistoryLoad::Repeater(_) => "Repeater",
            core::HistoryLoad::Ur(_) => "Ur",
        }
    }
    #[getter]
    fn index(&self) -> i64 {
        match self.inner {
            core::HistoryLoad::Repeater(v) => i64::from(v),
            core::HistoryLoad::Ur(v) => i64::from(v),
        }
    }
}

#[derive(Clone, Copy)]
enum Projection {
    Raw(u32),
    Field(usize),
}
enum ByteSource {
    Cleared,
    Holder(Py<DecodedRecord>, Projection),
    Owned(Py<PyBytes>),
}
#[pyclass(module = "cs705.icf.record")]
pub struct HistoryBytes {
    source: ByteSource,
}
impl HistoryBytes {
    fn with_ref<R>(&self, py: Python<'_>, f: impl FnOnce(&[u8]) -> PyResult<R>) -> PyResult<R> {
        match &self.source {
            ByteSource::Cleared => Err(pyo3::exceptions::PyReferenceError::new_err(
                "history byte view was cleared",
            )),
            ByteSource::Owned(v) => f(v.bind(py).as_bytes()),
            ByteSource::Holder(holder, projection) => {
                holder.get().with_ref(|holder| match projection {
                    Projection::Raw(key) => {
                        f(core::receive_history_raw(holder, *key).map_err(errors::icf)?)
                    }
                    Projection::Field(index) => {
                        match core::receive_history_fields(holder).into_iter().nth(*index) {
                            Some((_, core::HistoryValue::Bytes(v))) => f(v),
                            _ => Err(pyo3::exceptions::PyTypeError::new_err(
                                "history field is not a byte projection",
                            )),
                        }
                    }
                })
            }
        }
    }
}
#[pymethods]
impl HistoryBytes {
    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        self.with_ref(py, |v| Ok(v.len()))
    }
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(py, |v| Ok(PyBytes::new(py, v)))
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        match &self.source {
            ByteSource::Holder(v, _) => visit.call(v),
            ByteSource::Owned(v) => visit.call(v),
            ByteSource::Cleared => Ok(()),
        }
    }
    fn __clear__(&mut self) {
        self.source = ByteSource::Cleared;
    }
}
enum Value {
    Cleared,
    Unsigned(u32),
    Signed(i32),
    Bytes(Py<HistoryBytes>),
}
#[pyclass(module = "cs705.icf.record")]
pub struct HistoryValue {
    inner: Value,
}
#[pymethods]
#[allow(non_snake_case)]
impl HistoryValue {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        match &self.inner {
            Value::Bytes(v) => visit.call(v),
            _ => Ok(()),
        }
    }
    fn __clear__(&mut self) {
        self.inner = Value::Cleared;
    }
    #[staticmethod]
    fn Unsigned(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: Value::Unsigned(strict_u32(value)?),
        })
    }
    #[staticmethod]
    fn Signed(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: Value::Signed(strict_i32(value)?),
        })
    }
    #[staticmethod]
    fn Bytes(py: Python<'_>, source: Py<PyBytes>) -> PyResult<Self> {
        Ok(Self {
            inner: Value::Bytes(Py::new(
                py,
                HistoryBytes {
                    source: ByteSource::Owned(source),
                },
            )?),
        })
    }
    #[getter]
    fn variant(&self) -> PyResult<&'static str> {
        Ok(match self.inner {
            Value::Cleared => {
                return Err(pyo3::exceptions::PyReferenceError::new_err(
                    "history value was cleared",
                ));
            }
            Value::Unsigned(_) => "Unsigned",
            Value::Signed(_) => "Signed",
            Value::Bytes(_) => "Bytes",
        })
    }
    #[getter]
    fn unsigned(&self) -> PyResult<Option<u32>> {
        Ok(match self.inner {
            Value::Cleared => {
                return Err(pyo3::exceptions::PyReferenceError::new_err(
                    "history value was cleared",
                ));
            }
            Value::Unsigned(v) => Some(v),
            _ => None,
        })
    }
    #[getter]
    fn signed(&self) -> PyResult<Option<i32>> {
        Ok(match self.inner {
            Value::Cleared => {
                return Err(pyo3::exceptions::PyReferenceError::new_err(
                    "history value was cleared",
                ));
            }
            Value::Signed(v) => Some(v),
            _ => None,
        })
    }
    #[getter]
    fn bytes(&self, py: Python<'_>) -> PyResult<Option<Py<HistoryBytes>>> {
        match &self.inner {
            Value::Bytes(v) => {
                v.try_borrow(py)?.with_ref(py, |_| Ok(()))?;
                Ok(Some(v.clone_ref(py)))
            }
            Value::Cleared => Err(pyo3::exceptions::PyReferenceError::new_err(
                "history value was cleared",
            )),
            _ => Ok(None),
        }
    }
}

fn callback<T>(
    link: &CallbackLink,
    f: impl FnOnce(Python<'_>, &Bound<'_, PyAny>) -> PyResult<T>,
) -> Result<T, cs705_core::icf::Error> {
    Python::attach(|py| link.invoke(py, |call| f(py, call)))
        .map_err(|_| cs705_core::icf::Error::Operation("Python history callback failed".into()))
}
#[pyfunction]
#[pyo3(signature=(holder,kind,raw,query,load))]
fn decode_history_into(
    py: Python<'_>,
    holder: Py<DecodedRecord>,
    kind: &Bound<'_, PyAny>,
    raw: Option<Py<PyBytes>>,
    query: Py<PyAny>,
    load: Py<PyAny>,
) -> PyResult<()> {
    let kind = strict_u32(kind)?;
    let query_holder = CallbackHolder::new(py, query.bind(py))?;
    let query = CallbackHolder::link(query_holder.bind(py));
    let load_holder = CallbackHolder::new(py, load.bind(py))?;
    let load = CallbackHolder::link(load_holder.bind(py));
    let (errors, result) = CallbackError::scoped(py, || {
        let mut holder = holder.get().take()?;
        let raw = raw.as_ref().map(|v| v.bind(py).as_bytes());
        Ok(py.detach(|| {
            core::decode_history_into(
                &mut holder,
                kind,
                raw,
                |inner| {
                    callback(&query, |py, call| {
                        strict_i32(&call.call1((Py::new(py, HistoryQuery { inner })?,))?)
                    })
                },
                |inner, holder| {
                    callback(&load, |py, call| {
                        Frame::with_mut(py, holder, |loan| {
                            crate::icf::document::adapters::require_none(call.call1((
                                Py::new(py, HistoryLoad { inner })?,
                                Py::new(py, DecodedRecord::from_loan(loan))?,
                            ))?)
                        })
                    })
                },
            )
        }))
    })?;
    errors.finish(py, result.and_then(|v| v.map_err(errors::icf)))
}
#[pyfunction]
fn resolve_history_query(
    py: Python<'_>,
    repeaters: &Working,
    ur: &WorkingUr,
    query: &HistoryQuery,
) -> PyResult<i32> {
    let query = query.inner;
    repeaters.inner.with_ref(|repeaters| {
        ur.with_ref(|ur| {
            py.detach(|| core::resolve_history_query(repeaters, ur, query).map_err(errors::icf))
        })
    })
}
#[pyfunction]
fn load_history_storage_into(
    py: Python<'_>,
    image: Py<Icf>,
    kind: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    repeaters: &Working,
    ur: &WorkingUr,
    holder: Py<DecodedRecord>,
) -> PyResult<()> {
    let kind = strict_u32(kind)?;
    let index = strict_usize(index)?;
    let image = image.get().take()?;
    let mut holder = holder.get().take()?;
    repeaters.inner.with_ref(|repeaters| {
        ur.with_ref(|ur| {
            py.detach(|| {
                core::load_history_storage_into(&image, kind, index, repeaters, ur, &mut holder)
                    .map_err(errors::icf)
            })
        })
    })
}
macro_rules! loader{($($name:ident),*)=>{$(#[pyfunction]fn $name(py:Python<'_>,image:Py<Icf>,index:&Bound<'_,PyAny>,holder:Py<DecodedRecord>)->PyResult<()>{let index=strict_u32(index)?;let image=image.get().take()?;let mut holder=holder.get().take()?;py.detach(||core::$name(&image,index,&mut holder).map_err(errors::icf))})*}}
loader!(
    load_reflector_storage_into,
    load_receive_history_storage_into
);
#[pyfunction]
fn history_selection_present(
    py: Python<'_>,
    image: Py<Icf>,
    kind: &Bound<'_, PyAny>,
    first: &Bound<'_, PyAny>,
    last: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let (kind, first, last) = (strict_u32(kind)?, strict_usize(first)?, strict_usize(last)?);
    let image = image.get().take()?;
    py.detach(|| core::history_selection_present(&image, kind, first, last).map_err(errors::icf))
}
#[pyfunction]
fn delete_history_storage(
    py: Python<'_>,
    image: Py<Icf>,
    kind: &Bound<'_, PyAny>,
    first: &Bound<'_, PyAny>,
    last: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let (kind, first, last) = (strict_u32(kind)?, strict_usize(first)?, strict_usize(last)?);
    let mut image = image.get().take()?;
    py.detach(|| core::delete_history_storage(&mut image, kind, first, last).map_err(errors::icf))
}
#[pyfunction]
fn receive_history_raw(
    holder: Py<DecodedRecord>,
    key: &Bound<'_, PyAny>,
) -> PyResult<HistoryBytes> {
    let key = strict_u32(key)?;
    holder.get().with_ref(|v| {
        core::receive_history_raw(v, key)
            .map(|_| ())
            .map_err(errors::icf)
    })?;
    Ok(HistoryBytes {
        source: ByteSource::Holder(holder, Projection::Raw(key)),
    })
}
#[pyfunction]
fn history_field_visible(holder: &DecodedRecord, key: &Bound<'_, PyAny>) -> PyResult<bool> {
    let key = strict_u32(key)?;
    holder.with_ref(|v| core::history_field_visible(v, key).map_err(errors::icf))
}
#[pyfunction]
fn receive_history_fields(
    py: Python<'_>,
    holder: Py<DecodedRecord>,
) -> PyResult<Vec<(String, HistoryValue)>> {
    enum Captured {
        Unsigned(u32),
        Signed(i32),
        Bytes(usize),
    }
    let fields = holder.get().with_ref(|v| {
        Ok(core::receive_history_fields(v)
            .into_iter()
            .enumerate()
            .map(|(index, (name, value))| {
                (
                    name.to_owned(),
                    match value {
                        core::HistoryValue::Unsigned(v) => Captured::Unsigned(v),
                        core::HistoryValue::Signed(v) => Captured::Signed(v),
                        core::HistoryValue::Bytes(_) => Captured::Bytes(index),
                    },
                )
            })
            .collect::<Vec<_>>())
    })?;
    fields
        .into_iter()
        .map(|(name, value)| {
            Ok((
                name,
                HistoryValue {
                    inner: match value {
                        Captured::Unsigned(v) => Value::Unsigned(v),
                        Captured::Signed(v) => Value::Signed(v),
                        Captured::Bytes(index) => Value::Bytes(Py::new(
                            py,
                            HistoryBytes {
                                source: ByteSource::Holder(
                                    holder.clone_ref(py),
                                    Projection::Field(index),
                                ),
                            },
                        )?),
                    },
                },
            ))
        })
        .collect()
}
pub fn register(record: &Bound<'_, PyModule>) -> PyResult<()> {
    record.add_class::<HistoryQuery>()?;
    record.add_class::<HistoryLoad>()?;
    record.add_class::<HistoryValue>()?;
    record.add_class::<HistoryBytes>()?;
    record.add_function(wrap_pyfunction!(decode_history_into, record)?)?;
    record.add_function(wrap_pyfunction!(resolve_history_query, record)?)?;
    record.add_function(wrap_pyfunction!(load_history_storage_into, record)?)?;
    record.add_function(wrap_pyfunction!(load_reflector_storage_into, record)?)?;
    record.add_function(wrap_pyfunction!(load_receive_history_storage_into, record)?)?;
    record.add_function(wrap_pyfunction!(history_selection_present, record)?)?;
    record.add_function(wrap_pyfunction!(delete_history_storage, record)?)?;
    record.add_function(wrap_pyfunction!(receive_history_raw, record)?)?;
    record.add_function(wrap_pyfunction!(history_field_visible, record)?)?;
    record.add_function(wrap_pyfunction!(receive_history_fields, record)?)?;
    Ok(())
}
