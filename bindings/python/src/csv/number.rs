//! Native numeric conversion and formatting.
use super::*;
use cs705_core::csv::number as n;

pub(crate) fn coordinate_event(value: &Bound<'_, PyAny>) -> PyResult<n::CoordinateEvent> {
    Ok(
        match enum_index(value, "cs705.csv.number", "CoordinateEvent")? {
            0 => n::CoordinateEvent::Accept,
            1 => n::CoordinateEvent::Cancel,
            2 => n::CoordinateEvent::EscapeKeyPress,
            3 => n::CoordinateEvent::Deactivate,
            4 => n::CoordinateEvent::Close,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "invalid CoordinateEvent",
                ));
            }
        },
    )
}
enum DialogState {
    Owned(crate::clone::Handle<n::CoordinateDialog>),
    Scoped(crate::scoped::Loan<n::CoordinateDialog>),
}
#[pyclass(module = "cs705.csv.number", frozen)]
pub struct CoordinateDialog {
    inner: DialogState,
}
impl CoordinateDialog {
    pub(crate) fn from_core(value: n::CoordinateDialog) -> Self {
        Self {
            inner: DialogState::Owned(crate::clone::Handle::new(value)),
        }
    }
    pub(crate) fn from_handle(value: crate::clone::Handle<n::CoordinateDialog>) -> Self {
        Self {
            inner: DialogState::Owned(value),
        }
    }
    pub(crate) fn from_loan(value: crate::scoped::Loan<n::CoordinateDialog>) -> Self {
        Self {
            inner: DialogState::Scoped(value),
        }
    }
    pub(crate) fn with_ref<T>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&n::CoordinateDialog) -> PyResult<T>,
    ) -> PyResult<T> {
        match &self.inner {
            DialogState::Owned(o) => o.with(|v| f(v)),
            DialogState::Scoped(l) => l.with_ref(py, f),
        }
    }
    pub(crate) fn with_mut<T>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut n::CoordinateDialog) -> PyResult<T>,
    ) -> PyResult<T> {
        match &self.inner {
            DialogState::Owned(o) => o.with(f),
            DialogState::Scoped(l) => l.with_mut(py, f),
        }
    }
}
#[pymethods]
impl CoordinateDialog {
    #[setter]
    fn set_mode(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u8(value)?;
        self.with_mut(py, |d| {
            d.mode = value;
            Ok(())
        })
    }
    #[setter]
    fn set_mask(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bytes(value)?.to_vec();
        self.with_mut(py, |d| {
            d.mask = std::borrow::Cow::Owned(value);
            Ok(())
        })
    }
    #[setter]
    fn set_seed(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bytes(value)?.to_vec();
        self.with_mut(py, |d| {
            d.seed = value;
            Ok(())
        })
    }
    #[setter]
    fn set_control(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bytes(value)?.to_vec();
        self.with_mut(py, |d| {
            d.control = value;
            Ok(())
        })
    }
    #[setter]
    fn set_phase2(&self, py: Python<'_>, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let value = value
            .map(|v| Ok::<_, PyErr>(strict_bytes(v)?.to_vec()))
            .transpose()?;
        self.with_mut(py, |d| {
            d.phase2 = value;
            Ok(())
        })
    }
    #[setter]
    fn set_cancel(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.with_mut(py, |d| {
            d.cancel = value;
            Ok(())
        })
    }
    #[setter]
    fn set_done(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.with_mut(py, |d| {
            d.done = value;
            Ok(())
        })
    }
    #[setter]
    fn set_release_requested(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.with_mut(py, |d| {
            d.release_requested = value;
            Ok(())
        })
    }
    fn __eq__(&self, py: Python<'_>, other: &Self) -> PyResult<bool> {
        let value = self.with_ref(py, |d| Ok(d.clone()))?;
        other.with_ref(py, |other| Ok(value == *other))
    }
    #[new]
    fn new(
        raw: &Bound<'_, PyAny>,
        longitude: &Bound<'_, PyAny>,
        current: &Bound<'_, PyAny>,
        first: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        n::CoordinateDialog::new(
            fixed_bytes(raw)?,
            strict_bool(longitude)?,
            strict_bytes(current)?,
            first.map(strict_u8).transpose()?,
        )
        .map(Self::from_core)
        .map_err(crate::errors::csv)
    }
    #[staticmethod]
    fn area(
        raw: &Bound<'_, PyAny>,
        current: &Bound<'_, PyAny>,
        first: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        n::CoordinateDialog::area(
            fixed_bytes(raw)?,
            strict_bytes(current)?,
            first.map(strict_u8).transpose()?,
        )
        .map(Self::from_core)
        .map_err(crate::errors::csv)
    }
    fn replace(&self, py: Python<'_>, text: &Bound<'_, PyAny>) -> PyResult<()> {
        let text = strict_bytes(text)?;
        self.with_mut(py, |d| {
            d.replace(text);
            Ok(())
        })
    }
    fn candidate<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(py, |d| Ok(PyBytes::new(py, &d.candidate())))
    }
    fn valid(&self, py: Python<'_>) -> PyResult<bool> {
        self.with_ref(py, |d| Ok(d.valid()))
    }
    fn event<'py>(
        &self,
        py: Python<'py>,
        event: &Bound<'_, PyAny>,
    ) -> PyResult<Option<Bound<'py, PyBytes>>> {
        let event = coordinate_event(event)?;
        self.with_mut(py, |d| Ok(d.event(event).map(|v| PyBytes::new(py, &v))))
    }
    fn dispatched(&self, py: Python<'_>) -> PyResult<()> {
        self.with_mut(py, |d| {
            d.dispatched();
            Ok(())
        })
    }
    #[getter]
    fn mode(&self, py: Python<'_>) -> PyResult<u8> {
        self.with_ref(py, |d| Ok(d.mode))
    }
    #[getter]
    fn mask<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(py, |d| Ok(PyBytes::new(py, &d.mask)))
    }
    #[getter]
    fn seed<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(py, |d| Ok(PyBytes::new(py, &d.seed)))
    }
    #[getter]
    fn control<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(py, |d| Ok(PyBytes::new(py, &d.control)))
    }
    #[getter]
    fn phase2<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyBytes>>> {
        self.with_ref(py, |d| Ok(d.phase2.as_ref().map(|v| PyBytes::new(py, v))))
    }
    #[getter]
    fn cancel(&self, py: Python<'_>) -> PyResult<bool> {
        self.with_ref(py, |d| Ok(d.cancel))
    }
    #[getter]
    fn done(&self, py: Python<'_>) -> PyResult<bool> {
        self.with_ref(py, |d| Ok(d.done))
    }
    #[getter]
    fn release_requested(&self, py: Python<'_>) -> PyResult<bool> {
        self.with_ref(py, |d| Ok(d.release_requested))
    }
}

#[pyfunction]
fn decimal(source: &Bound<'_, PyAny>, point: &Bound<'_, PyAny>) -> PyResult<f64> {
    n::decimal(strict_bytes(source)?, strict_u8(point)?).map_err(crate::errors::csv)
}
#[pyfunction]
fn int32(source: &Bound<'_, PyAny>) -> PyResult<i32> {
    n::int32(strict_bytes(source)?).map_err(crate::errors::csv)
}
#[pyfunction]
fn civ_address(source: &Bound<'_, PyAny>) -> PyResult<u8> {
    Ok(n::civ_address(strict_bytes(source)?))
}
#[pyfunction]
fn coordinate<'py>(
    py: Python<'py>,
    source: &Bound<'_, PyAny>,
    latitude: &Bound<'_, PyAny>,
    point: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &n::coordinate(
            strict_bytes(source)?,
            strict_bool(latitude)?,
            strict_u8(point)?,
        ),
    ))
}
#[pyfunction]
fn coordinate_text<'py>(
    py: Python<'py>,
    value: &Bound<'_, PyAny>,
    latitude: &Bound<'_, PyAny>,
    point: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &n::coordinate_text(
            fixed_bytes(value)?,
            strict_bool(latitude)?,
            strict_u8(point)?,
        ),
    ))
}
#[pyfunction]
fn altitude(source: &Bound<'_, PyAny>, point: &Bound<'_, PyAny>) -> PyResult<i32> {
    n::altitude(strict_bytes(source)?, strict_u8(point)?).map_err(crate::errors::csv)
}
#[pyfunction]
fn altitude_text<'py>(
    py: Python<'py>,
    value: &Bound<'_, PyAny>,
    point: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &n::altitude_text(strict_i32(value)?, strict_u8(point)?),
    ))
}
#[pyfunction]
fn frequency_text<'py>(
    py: Python<'py>,
    hz: &Bound<'_, PyAny>,
    point: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &n::frequency_text(strict_u32(hz)?, strict_u8(point)?),
    ))
}
#[pyfunction]
fn frequency_hz(source: &Bound<'_, PyAny>, point: &Bound<'_, PyAny>) -> PyResult<u32> {
    n::frequency_hz(strict_bytes(source)?, strict_u8(point)?).map_err(crate::errors::csv)
}
#[pyfunction]
fn offset_hz(source: &Bound<'_, PyAny>, point: &Bound<'_, PyAny>) -> PyResult<u32> {
    n::offset_hz(strict_bytes(source)?, strict_u8(point)?).map_err(crate::errors::csv)
}
#[pyfunction]
fn crt_decimal_prefix(source: &Bound<'_, PyAny>, point: &Bound<'_, PyAny>) -> PyResult<f64> {
    n::crt_decimal_prefix(strict_bytes(source)?, strict_u8(point)?).map_err(crate::errors::csv)
}
#[pyfunction]
fn area_native_mode(current: &Bound<'_, PyAny>) -> PyResult<u8> {
    n::area_native_mode(strict_bytes(current)?).map_err(crate::errors::csv)
}
#[pyfunction]
fn area_native_phase2(
    destination: &MutableBytes,
    source: &Bound<'_, PyAny>,
    mode: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let (source, mode) = (strict_bytes(source)?, strict_u8(mode)?);
    destination.with_mut(|destination| {
        let destination: &mut [u8; 4] = destination
            .try_into()
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("expected exactly 4 bytes"))?;
        n::area_native_phase2(destination, source, mode).map_err(crate::errors::csv)
    })
}
#[pyfunction]
fn coordinate_native_phase2(
    destination: &MutableBytes,
    source: &Bound<'_, PyAny>,
    current: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let (source, current) = (strict_bytes(source)?, strict_bytes(current)?);
    destination.with_mut(|destination| {
        let destination: &mut [u8; 4] = destination
            .try_into()
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("expected exactly 4 bytes"))?;
        n::coordinate_native_phase2(destination, source, current).map_err(crate::errors::csv)
    })
}
#[pyfunction]
fn scalar<'py>(
    py: Python<'py>,
    value: &Bound<'_, PyAny>,
    scale: &Bound<'_, PyAny>,
    fractional_digits: &Bound<'_, PyAny>,
    integer_width: &Bound<'_, PyAny>,
    signed_bounds: &Bound<'_, PyAny>,
    force_plus: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &n::field::scalar(
            strict_i32(value)?,
            strict_u32(scale)?,
            strict_u32(fractional_digits)?,
            strict_usize(integer_width)?,
            strict_bool(signed_bounds)?,
            strict_bool(force_plus)?,
        )
        .map_err(crate::errors::csv)?,
    ))
}
#[pyfunction]
fn channel<'py>(py: Python<'py>, hz: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(py, &n::field::channel(strict_u32(hz)?)))
}
#[pyfunction]
fn compact_offset<'py>(py: Python<'py>, hz: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(py, &n::field::compact_offset(strict_u32(hz)?)))
}
#[pyfunction]
fn repeater_offset<'py>(py: Python<'py>, hz: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &n::field::repeater_offset(strict_u32(hz)?),
    ))
}
#[pyfunction]
fn history<'py>(
    py: Python<'py>,
    hz: &Bound<'_, PyAny>,
    live_decimal: &Bound<'_, PyAny>,
    copied_decimal: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let mut errors = crate::callbacks::CallbackError::default();
    let result = n::field::history(strict_u32(hz)?, strict_u8(live_decimal)?, |index| {
        callback(
            &mut errors,
            copied_decimal.call1((index,)).and_then(|v| strict_u8(&v)),
        )
    });
    errors
        .finish(py, result.map_err(crate::errors::csv))
        .map(|v| PyBytes::new(py, &v))
}
#[pyfunction]
fn degree(
    gui_language: Option<&Bound<'_, PyAny>>,
    system_langid: Option<&Bound<'_, PyAny>>,
) -> PyResult<u8> {
    n::paint::degree(
        gui_language.map(strict_u32).transpose()?,
        system_langid.map(strict_u16).transpose()?,
    )
    .map_err(crate::errors::csv)
}
#[pyfunction(name = "format")]
fn paint_format<'py>(
    py: Python<'py>,
    raw: &Bound<'_, PyAny>,
    mode: &Bound<'_, PyAny>,
    current: &Bound<'_, PyAny>,
    gui_language: Option<&Bound<'_, PyAny>>,
    system_langid: Option<&Bound<'_, PyAny>>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &n::paint::format(
            fixed_bytes(raw)?,
            strict_u8(mode)?,
            strict_bytes(current)?,
            gui_language.map(strict_u32).transpose()?,
            system_langid.map(strict_u16).transpose()?,
        )
        .map_err(crate::errors::csv)?,
    ))
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "number")?;
    add_enum(
        &m,
        "CoordinateEvent",
        &["Accept", "Cancel", "EscapeKeyPress", "Deactivate", "Close"],
    )?;
    m.add_class::<CoordinateDialog>()?;
    m.add_function(wrap_pyfunction!(decimal, &m)?)?;
    m.add_function(wrap_pyfunction!(int32, &m)?)?;
    m.add_function(wrap_pyfunction!(civ_address, &m)?)?;
    m.add_function(wrap_pyfunction!(coordinate, &m)?)?;
    m.add_function(wrap_pyfunction!(coordinate_text, &m)?)?;
    m.add_function(wrap_pyfunction!(altitude, &m)?)?;
    m.add_function(wrap_pyfunction!(altitude_text, &m)?)?;
    m.add_function(wrap_pyfunction!(frequency_text, &m)?)?;
    m.add_function(wrap_pyfunction!(frequency_hz, &m)?)?;
    m.add_function(wrap_pyfunction!(offset_hz, &m)?)?;
    m.add_function(wrap_pyfunction!(crt_decimal_prefix, &m)?)?;
    m.add_function(wrap_pyfunction!(area_native_mode, &m)?)?;
    m.add_function(wrap_pyfunction!(area_native_phase2, &m)?)?;
    m.add_function(wrap_pyfunction!(coordinate_native_phase2, &m)?)?;
    let field = PyModule::new(root.py(), "field")?;
    field.add_function(wrap_pyfunction!(scalar, &field)?)?;
    field.add_function(wrap_pyfunction!(self::channel, &field)?)?;
    field.add_function(wrap_pyfunction!(compact_offset, &field)?)?;
    field.add_function(wrap_pyfunction!(repeater_offset, &field)?)?;
    field.add_function(wrap_pyfunction!(self::history, &field)?)?;
    m.add_submodule(&field)?;
    let paint = PyModule::new(root.py(), "paint")?;
    paint.add_function(wrap_pyfunction!(degree, &paint)?)?;
    paint.add_function(wrap_pyfunction!(paint_format, &paint)?)?;
    m.add_submodule(&paint)?;
    root.add_submodule(&m)
}
