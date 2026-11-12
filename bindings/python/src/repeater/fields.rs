//! Native repeater field metadata and type transformations.
use super::staging::Record;
use crate::{callbacks::CallbackError, owner::Owner, scoped::Frame};
use crate::{conversion::*, errors};
use cs705_core::repeater::fields as core;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use std::cell::RefCell;

pub(crate) fn add_enum(m: &Bound<'_, PyModule>, name: &str, variants: &[&str]) -> PyResult<()> {
    let py = m.py();
    let entries = PyDict::new(py);
    for (n, v) in variants.iter().enumerate() {
        entries.set_item(v, n)?;
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item("module", format!("cs705.repeater.{}", m.name()?.to_str()?))?;
    m.add(
        name,
        py.import("enum")?
            .getattr("Enum")?
            .call((name, entries), Some(&kwargs))?,
    )
}
pub(crate) fn field(value: &Bound<'_, PyAny>) -> PyResult<core::Field> {
    use core::Field::*;
    Ok(
        match crate::icf::enum_value(value, "cs705.repeater.fields", "Field")? {
            0 => Mode,
            1 => Duplex,
            2 => Tone,
            3 => ToneFrequency,
            _ => return Err(pyo3::exceptions::PyValueError::new_err("invalid Field")),
        },
    )
}
#[pyclass(module = "cs705.repeater.fields")]
pub struct TypeSource {
    pub(crate) inner: core::TypeSource,
}
#[pymethods]
impl TypeSource {
    #[new]
    fn new(placeholder: &Bound<'_, PyAny>, gateway_marker: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::TypeSource {
                placeholder: fixed_bytes(placeholder)?,
                gateway_marker: fixed_bytes(gateway_marker)?,
            },
        })
    }
    #[getter]
    fn placeholder<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.placeholder)
    }
    #[setter]
    fn set_placeholder(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.placeholder = fixed_bytes(value)?;
        Ok(())
    }
    #[getter]
    fn gateway_marker<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.gateway_marker)
    }
    #[setter]
    fn set_gateway_marker(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.gateway_marker = fixed_bytes(value)?;
        Ok(())
    }
}
#[pyclass(module = "cs705.repeater.fields")]
pub struct Selection {
    pub(crate) inner: core::Selection,
}
#[pymethods]
impl Selection {
    #[new]
    fn new(accepted: &Bound<'_, PyAny>, candidate: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Selection {
                accepted: strict_bool(accepted)?,
                candidate: strict_u32(candidate)?,
            },
        })
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
    #[getter]
    fn candidate(&self) -> u32 {
        self.inner.candidate
    }
    #[setter]
    fn set_candidate(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.candidate = strict_u32(v)?;
        Ok(())
    }
}
#[pyclass(module = "cs705.repeater.fields", frozen)]
pub struct Effects {
    pub(crate) inner: Owner<core::Effects>,
}
macro_rules! effects {
    ($($name:ident, $setter:ident => $convert:ident: $ty:ty),* $(,)?) => {
        #[pymethods]
        impl Effects {
            #[new]
            fn new() -> Self { Self { inner: Owner::new(core::Effects::default()) } }
            fn __richcmp__(&self,other:&Self,op:pyo3::basic::CompareOp)->PyResult<bool>{
                let equal=if std::ptr::eq(self,other){self.inner.with_ref(|_|Ok(true))?}else{self.inner.with_ref(|a|other.inner.with_ref(|b|Ok(a==b)))?};
                super::equality(equal,op)
            }
            $(#[getter] fn $name(&self) -> PyResult<$ty> { self.inner.with_ref(|e| Ok(e.$name)) }
              #[setter] fn $setter(&self, v: &Bound<'_, PyAny>) -> PyResult<()> { let v = $convert(v)?; self.inner.with_mut(|e| { e.$name = v; Ok(()) }) })*
        }
    }
}
effects!(changed, set_changed => strict_bool: bool, flags, set_flags => strict_u32: u32,
    warning_flags, set_warning_flags => strict_u32: u32, save_requested, set_save_requested => strict_bool: bool,
    save_returned, set_save_returned => strict_bool: bool);
#[pyclass(module = "cs705.repeater.fields")]
pub struct PaintMeta {
    pub(crate) inner: core::PaintMeta,
}
#[pymethods]
impl PaintMeta {
    #[new]
    fn new(gate: &Bound<'_, PyAny>, gray: &Bound<'_, PyAny>) -> PyResult<Self> {
        let gate = match crate::icf::enum_value(gate, "cs705.repeater.fields", "Gate")? {
            0 => core::Gate::Open,
            1 => core::Gate::DisplayOnly,
            2 => core::Gate::Placeholder,
            _ => return Err(pyo3::exceptions::PyValueError::new_err("invalid Gate")),
        };
        Ok(Self {
            inner: core::PaintMeta {
                gate,
                gray: strict_bool(gray)?,
            },
        })
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
    #[getter]
    fn gate<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        crate::icf::enum_result(py, "cs705.repeater.fields", "Gate", self.inner.gate as i64)
    }
    #[setter]
    fn set_gate(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.gate = match crate::icf::enum_value(v, "cs705.repeater.fields", "Gate")? {
            0 => core::Gate::Open,
            1 => core::Gate::DisplayOnly,
            2 => core::Gate::Placeholder,
            _ => return Err(pyo3::exceptions::PyValueError::new_err("invalid Gate")),
        };
        Ok(())
    }
    #[getter]
    fn gray(&self) -> bool {
        self.inner.gray
    }
    #[setter]
    fn set_gray(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.gray = strict_bool(v)?;
        Ok(())
    }
    fn placeholder<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.inner.placeholder().map(|v| PyBytes::new(py, v))
    }
}
#[pyfunction]
fn transform_type(
    row: &Record,
    candidate: &Bound<'_, PyAny>,
    source: &TypeSource,
) -> PyResult<bool> {
    let candidate = strict_u32(candidate)?;
    row.inner
        .with_mut(|r| Ok(core::transform_type(r, candidate, source.inner)))
}
#[pyfunction]
fn candidate_allowed(
    row: &Record,
    field: &Bound<'_, PyAny>,
    candidate: &Bound<'_, PyAny>,
    current: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let (field, candidate, current) = (
        self::field(field)?,
        strict_u32(candidate)?,
        strict_u32(current)?,
    );
    row.inner
        .with_ref(|r| core::candidate_allowed(r, field, candidate, current).map_err(errors::csv))
}
#[pyfunction]
fn quantize_offset_hz(value: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(core::quantize_offset_hz(strict_u32(value)?))
}
#[pyfunction]
fn suppress_mode_code(value: &Bound<'_, PyAny>) -> PyResult<i32> {
    let mut value = strict_i32(value)?;
    core::suppress_mode_code(&mut value);
    Ok(value)
}
#[pyfunction]
fn paint_meta(
    holder: &crate::icf::record::DecodedRecord,
    key: &Bound<'_, PyAny>,
) -> PyResult<PaintMeta> {
    let key = strict_u16(key)?;
    holder.with_ref(|h| {
        Ok(PaintMeta {
            inner: core::paint_meta(h, key).map_err(errors::csv)?,
        })
    })
}

fn callback<T>(
    errors: &RefCell<CallbackError>,
    result: PyResult<T>,
) -> Result<T, cs705_core::csv::Error> {
    errors
        .borrow_mut()
        .capture(result)
        .map_err(|_| cs705_core::csv::Error::Operation("repeater field callback failed".into()))
}
fn call_save(
    py: Python<'_>,
    row: &mut cs705_core::repeater::staging::Record,
    save: &Py<PyAny>,
) -> PyResult<()> {
    Frame::with_mut(py, row, |loan| {
        save.bind(py).call1((Record::from_loan(loan),))?;
        Ok(())
    })
}
fn call_style(
    py: Python<'_>,
    row: &cs705_core::repeater::staging::Record,
    style: &Py<PyAny>,
) -> PyResult<()> {
    Frame::with_ref(py, row, |loan| {
        style.bind(py).call1((Record::from_loan(loan),))?;
        Ok(())
    })
}
#[pyfunction]
fn select_type(
    py: Python<'_>,
    row: &Record,
    selection: Py<Selection>,
    source: Py<TypeSource>,
    effects: &Effects,
    defaults: Py<PyAny>,
    save_reload: Py<PyAny>,
) -> PyResult<()> {
    let (selection, source) = (
        selection.try_borrow(py)?.inner,
        source.try_borrow(py)?.inner,
    );
    let errors = RefCell::new(CallbackError::default());
    let result = row.inner.with_mut(|row| {
        effects.inner.with_mut(|effects| {
            core::select_type(
                row,
                selection,
                source,
                effects,
                || {
                    callback(
                        &errors,
                        defaults.bind(py).call0().and_then(|v| {
                            v.extract::<PyRef<'_, Record>>()?
                                .inner
                                .with_ref(|r| Ok(r.clone()))
                        }),
                    )
                },
                |row| callback(&errors, call_save(py, row, &save_reload)),
            )
            .map_err(errors::csv)
        })
    });
    errors.into_inner().finish(py, result)
}
#[pyfunction]
fn select<'py>(
    py: Python<'py>,
    row: &Record,
    field: &Bound<'_, PyAny>,
    selection: Py<Selection>,
    effects: &Effects,
    save_reload: Py<PyAny>,
    style: Py<PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let (field, selection) = (self::field(field)?, selection.try_borrow(py)?.inner);
    let errors = RefCell::new(CallbackError::default());
    let result = row.inner.with_mut(|row| {
        effects.inner.with_mut(|effects| {
            core::select(
                row,
                field,
                selection,
                effects,
                |row| callback(&errors, call_save(py, row, &save_reload)),
                |row| callback(&errors, call_style(py, row, &style)),
            )
            .map_err(errors::csv)
        })
    });
    let out = errors.into_inner().finish(py, result)?;
    crate::icf::enum_result(py, "cs705.repeater.fields", "Outcome", out as i64)
}
#[pyfunction]
fn select_offset<'py>(
    py: Python<'py>,
    row: &Record,
    blocked: &Bound<'_, PyAny>,
    native_result: &Bound<'_, PyAny>,
    candidate_hz: &Bound<'_, PyAny>,
    effects: &Effects,
    style: Py<PyAny>,
    save_reload: Py<PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let (blocked, result, hz) = (
        strict_bool(blocked)?,
        strict_u32(native_result)?,
        strict_u32(candidate_hz)?,
    );
    let errors = RefCell::new(CallbackError::default());
    let native = row.inner.with_mut(|row| {
        effects.inner.with_mut(|effects| {
            core::select_offset(
                row,
                blocked,
                result,
                hz,
                effects,
                |row| callback(&errors, call_style(py, row, &style)),
                |row| callback(&errors, call_save(py, row, &save_reload)),
            )
            .map_err(errors::csv)
        })
    });
    let out = errors.into_inner().finish(py, native)?;
    crate::icf::enum_result(py, "cs705.repeater.fields", "Outcome", out as i64)
}
#[pyfunction]
fn select_utc_offset<'py>(
    py: Python<'py>,
    row: &Record,
    native_result: &Bound<'_, PyAny>,
    candidate_minutes: &Bound<'_, PyAny>,
    effects: &Effects,
    save_reload: Py<PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let (result, minutes) = (strict_u32(native_result)?, strict_i16(candidate_minutes)?);
    let errors = RefCell::new(CallbackError::default());
    let native = row.inner.with_mut(|row| {
        effects.inner.with_mut(|effects| {
            core::select_utc_offset(row, result, minutes, effects, |row| {
                callback(&errors, call_save(py, row, &save_reload))
            })
            .map_err(errors::csv)
        })
    });
    let out = errors.into_inner().finish(py, native)?;
    crate::icf::enum_result(py, "cs705.repeater.fields", "Outcome", out as i64)
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "fields")?;
    m.add_class::<TypeSource>()?;
    m.add_class::<Selection>()?;
    m.add_class::<Effects>()?;
    m.add_class::<PaintMeta>()?;
    add_enum(&m, "Field", &["Mode", "Duplex", "Tone", "ToneFrequency"])?;
    add_enum(&m, "Gate", &["Open", "DisplayOnly", "Placeholder"])?;
    add_enum(
        &m,
        "Outcome",
        &["Blocked", "DisplayOnly", "Skipped", "Saved"],
    )?;
    m.add_function(wrap_pyfunction!(transform_type, &m)?)?;
    m.add_function(wrap_pyfunction!(candidate_allowed, &m)?)?;
    m.add_function(wrap_pyfunction!(quantize_offset_hz, &m)?)?;
    m.add_function(wrap_pyfunction!(suppress_mode_code, &m)?)?;
    m.add_function(wrap_pyfunction!(paint_meta, &m)?)?;
    m.add_function(wrap_pyfunction!(select_type, &m)?)?;
    m.add_function(wrap_pyfunction!(select, &m)?)?;
    m.add_function(wrap_pyfunction!(select_offset, &m)?)?;
    m.add_function(wrap_pyfunction!(select_utc_offset, &m)?)?;
    root.add_submodule(&m)
}
