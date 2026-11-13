//! Retained native edit runtime and expiring mutable dialog projections.
use super::*;
use crate::{
    clone::Handle,
    scoped::{Frame, Loan},
};
use cs705_core::csv::date_input as d;

#[pyclass(module = "cs705.csv.date_input", frozen)]
pub struct Clock {
    inner: Handle<d::Clock>,
}
impl Clock {
    pub(crate) fn native(&self) -> PyResult<d::Clock> {
        self.inner.with(|v| Ok(*v))
    }
}
#[pymethods]
impl Clock {
    #[setter]
    fn set_year(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u16(value)?;
        self.inner.with(|v| {
            v.year = value;
            Ok(())
        })
    }
    #[setter]
    fn set_month(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u16(value)?;
        self.inner.with(|v| {
            v.month = value;
            Ok(())
        })
    }
    #[setter]
    fn set_day(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u16(value)?;
        self.inner.with(|v| {
            v.day = value;
            Ok(())
        })
    }
    #[setter]
    fn set_hour(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u16(value)?;
        self.inner.with(|v| {
            v.hour = value;
            Ok(())
        })
    }
    #[setter]
    fn set_minute(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u16(value)?;
        self.inner.with(|v| {
            v.minute = value;
            Ok(())
        })
    }
    #[setter]
    fn set_second(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u16(value)?;
        self.inner.with(|v| {
            v.second = value;
            Ok(())
        })
    }
    #[setter]
    fn set_millisecond(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u16(value)?;
        self.inner.with(|v| {
            v.millisecond = value;
            Ok(())
        })
    }
    fn __eq__(&self, other: &Self) -> PyResult<bool> {
        Ok(self.native()? == other.native()?)
    }
    #[new]
    fn new(
        year: &Bound<'_, PyAny>,
        month: &Bound<'_, PyAny>,
        day: &Bound<'_, PyAny>,
        hour: &Bound<'_, PyAny>,
        minute: &Bound<'_, PyAny>,
        second: &Bound<'_, PyAny>,
        millisecond: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Handle::new(d::Clock {
                year: strict_u16(year)?,
                month: strict_u16(month)?,
                day: strict_u16(day)?,
                hour: strict_u16(hour)?,
                minute: strict_u16(minute)?,
                second: strict_u16(second)?,
                millisecond: strict_u16(millisecond)?,
            }),
        })
    }
    fn datetime(&self) -> PyResult<(super::datetime::Date, super::datetime::Time)> {
        let (date, time) = self.native()?.datetime().map_err(crate::errors::csv)?;
        Ok((
            super::datetime::Date::from_core(date),
            super::datetime::Time { inner: time },
        ))
    }
    #[getter]
    fn year(&self) -> PyResult<u16> {
        self.inner.with(|v| Ok(v.year))
    }
    #[getter]
    fn month(&self) -> PyResult<u16> {
        self.inner.with(|v| Ok(v.month))
    }
    #[getter]
    fn day(&self) -> PyResult<u16> {
        self.inner.with(|v| Ok(v.day))
    }
    #[getter]
    fn hour(&self) -> PyResult<u16> {
        self.inner.with(|v| Ok(v.hour))
    }
    #[getter]
    fn minute(&self) -> PyResult<u16> {
        self.inner.with(|v| Ok(v.minute))
    }
    #[getter]
    fn second(&self) -> PyResult<u16> {
        self.inner.with(|v| Ok(v.second))
    }
    #[getter]
    fn millisecond(&self) -> PyResult<u16> {
        self.inner.with(|v| Ok(v.millisecond))
    }
}
/// Owned masked-edit runtime; native locale changes survive failed edits.
#[pyclass(module = "cs705.csv.date_input", frozen)]
pub struct Runtime {
    owner: Handle<d::OwnedRuntime>,
}
impl Runtime {
    fn with_runtime<T>(&self, f: impl FnOnce(&mut d::Runtime<'_>) -> PyResult<T>) -> PyResult<T> {
        self.owner.with(|r| r.with_runtime(f))
    }
}
#[pymethods]
impl Runtime {
    #[new]
    fn new(
        locale: &super::datetime::Context,
        time_format: &Bound<'_, PyAny>,
        date_order: &Bound<'_, PyAny>,
        clock: Option<&Clock>,
    ) -> PyResult<Self> {
        Ok(Self {
            owner: Handle::new(d::OwnedRuntime {
                locale: locale.to_core()?,
                time_format: strict_bytes(time_format)?.to_vec(),
                date_order: strict_u32(date_order)?,
                clock: clock.map(Clock::native).transpose()?,
            }),
        })
    }
    #[getter]
    fn locale(&self) -> PyResult<super::datetime::Context> {
        Ok(super::datetime::Context::from_handle(
            self.owner.project(|r| Ok(&mut r.locale)),
        ))
    }
    #[setter]
    fn set_locale(&self, value: &super::datetime::Context) -> PyResult<()> {
        let value = value.to_core()?;
        self.owner.edit(|r| {
            r.locale = value;
            Ok(())
        })
    }
    #[getter]
    fn time_format<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.owner.with(|r| Ok(PyBytes::new(py, &r.time_format)))
    }
    #[setter]
    fn set_time_format(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bytes(value)?;
        self.owner.with(|r| {
            r.time_format = value.to_vec();
            Ok(())
        })
    }
    #[getter]
    fn date_order(&self) -> PyResult<u32> {
        self.owner.with(|r| Ok(r.date_order))
    }
    #[setter]
    fn set_date_order(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u32(value)?;
        self.owner.with(|r| {
            r.date_order = value;
            Ok(())
        })
    }
    #[getter]
    fn clock(&self) -> PyResult<Option<Clock>> {
        if !self.owner.with(|r| Ok(r.clock.is_some()))? {
            return Ok(None);
        }
        Ok(Some(Clock {
            inner: self.owner.project(|r| {
                r.clock.as_mut().ok_or_else(|| {
                    pyo3::exceptions::PyReferenceError::new_err("clock field was removed")
                })
            }),
        }))
    }
    #[setter]
    fn set_clock(&self, value: Option<&Clock>) -> PyResult<()> {
        let value = value.map(Clock::native).transpose()?;
        self.owner.edit(|r| {
            r.clock = value;
            Ok(())
        })
    }
    fn into_owned(slf: Py<Self>) -> Py<Self> {
        slf
    }
}
enum DialogState {
    Owned(Handle<d::DateDialog>),
    Scoped(Loan<d::DateDialog>),
}
#[pyclass(module = "cs705.csv.date_input", frozen)]
pub struct DateDialog {
    inner: DialogState,
}
impl DateDialog {
    fn with_ref<T>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&d::DateDialog) -> PyResult<T>,
    ) -> PyResult<T> {
        match &self.inner {
            DialogState::Owned(o) => o.with(|v| f(v)),
            DialogState::Scoped(l) => l.with_ref(py, f),
        }
    }
    fn with_mut<T>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut d::DateDialog) -> PyResult<T>,
    ) -> PyResult<T> {
        match &self.inner {
            DialogState::Owned(o) => o.with(f),
            DialogState::Scoped(l) => l.with_mut(py, f),
        }
    }
}
#[pymethods]
impl DateDialog {
    fn __eq__(&self, py: Python<'_>, other: &Self) -> PyResult<bool> {
        let value = self.with_ref(py, |d| Ok(d.clone()))?;
        other.with_ref(py, |other| Ok(value == *other))
    }
    #[new]
    fn from_fields(
        py: Python<'_>,
        editor: &super::number::CoordinateDialog,
        enabled: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let enabled = strict_bool(enabled)?;
        editor.with_ref(py, |e| {
            Ok(Self {
                inner: DialogState::Owned(Handle::new(d::DateDialog {
                    editor: e.clone(),
                    enabled,
                })),
            })
        })
    }
    #[staticmethod]
    fn new(
        raw: &Bound<'_, PyAny>,
        mode_value: &Bound<'_, PyAny>,
        offset: &Bound<'_, PyAny>,
        runtime: &Runtime,
        first: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let (raw, mode, offset, first) = (
            fixed_bytes(raw)?,
            strict_u8(mode_value)?,
            strict_i16(offset)?,
            first.map(strict_u8).transpose()?,
        );
        runtime.with_runtime(|r| {
            Ok(Self {
                inner: DialogState::Owned(Handle::new(
                    d::DateDialog::new(raw, mode, offset, r, first).map_err(crate::errors::csv)?,
                )),
            })
        })
    }
    fn replace_masked(
        &self,
        py: Python<'_>,
        source: &Bound<'_, PyAny>,
        runtime: &Runtime,
    ) -> PyResult<()> {
        let source = strict_bytes(source)?;
        self.with_mut(py, |d| {
            runtime.with_runtime(|r| d.replace_masked(source, r).map_err(crate::errors::csv))
        })
    }
    fn valid(&self, py: Python<'_>, runtime: &Runtime) -> PyResult<bool> {
        self.with_ref(py, |d| {
            runtime.with_runtime(|r| d.valid(r).map_err(crate::errors::csv))
        })
    }
    fn event<'py>(
        &self,
        py: Python<'py>,
        event: &Bound<'_, PyAny>,
        runtime: &Runtime,
    ) -> PyResult<Option<Bound<'py, PyBytes>>> {
        let event = super::number::coordinate_event(event)?;
        self.with_mut(py, |d| {
            runtime.with_runtime(|r| {
                Ok(d.event(event, r)
                    .map_err(crate::errors::csv)?
                    .map(|v| PyBytes::new(py, &v)))
            })
        })
    }
    fn apply_event(
        &self,
        py: Python<'_>,
        raw: &MutableBytes,
        offset: &Bound<'_, PyAny>,
        event: &Bound<'_, PyAny>,
        runtime: &Runtime,
        effect: &Effect,
    ) -> PyResult<bool> {
        let offset = strict_i16(offset)?;
        let event = super::number::coordinate_event(event)?;
        raw.with_mut(|raw| {
            self.with_mut(py, |d| {
                runtime.with_runtime(|r| {
                    effect.owner.edit(|e| {
                        d.apply_event(raw6(raw)?, offset, event, r, e)
                            .map_err(crate::errors::csv)
                    })
                })
            })
        })
    }
    #[getter]
    fn enabled(&self, py: Python<'_>) -> PyResult<bool> {
        self.with_ref(py, |d| Ok(d.enabled))
    }
    #[setter]
    fn set_enabled(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.with_mut(py, |d| {
            d.enabled = value;
            Ok(())
        })
    }
    #[setter]
    fn set_editor(&self, py: Python<'_>, value: &super::number::CoordinateDialog) -> PyResult<()> {
        let value = value.with_ref(py, |e| Ok(e.clone()))?;
        match &self.inner {
            DialogState::Owned(h) => h.edit(|d| {
                d.editor = value;
                Ok(())
            }),
            DialogState::Scoped(l) => l.with_mut(py, |d| {
                d.editor = value;
                Ok(())
            }),
        }
    }
    #[getter]
    fn editor(&self, py: Python<'_>) -> PyResult<super::number::CoordinateDialog> {
        match &self.inner {
            DialogState::Owned(h) => Ok(super::number::CoordinateDialog::from_handle(
                h.project(|d| Ok(&mut d.editor)),
            )),
            DialogState::Scoped(l) => Ok(super::number::CoordinateDialog::from_loan(
                l.project_mut(py, |d| &d.editor, |d| &mut d.editor)?,
            )),
        }
    }
    fn with_editor<'py>(
        &self,
        py: Python<'py>,
        callback: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.with_mut(py, |d| {
            Frame::with_mut(py, &mut d.editor, |loan| {
                callback.call1((super::number::CoordinateDialog::from_loan(loan),))
            })
        })
    }
}
#[pyclass(module = "cs705.csv.date_input", frozen)]
pub struct Effect {
    owner: Handle<d::Effect>,
}
#[pymethods]
impl Effect {
    #[setter]
    fn set_changed(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.owner.with(|e| {
            e.changed = value;
            Ok(())
        })
    }
    #[setter]
    fn set_state_out(&self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let value = value.map(strict_u32).transpose()?;
        self.owner.with(|e| {
            e.state_out = value;
            Ok(())
        })
    }
    #[setter]
    fn set_display(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bytes(value)?.to_vec();
        self.owner.with(|e| {
            e.display = value;
            Ok(())
        })
    }
    #[setter]
    fn set_selection(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = value.cast::<pyo3::types::PyTuple>()?;
        if value.len() != 2 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "expected (start, length)",
            ));
        }
        let value = (
            strict_usize(&value.get_item(0)?)?,
            strict_usize(&value.get_item(1)?)?,
        );
        self.owner.with(|e| {
            e.selection = value;
            Ok(())
        })
    }
    #[setter]
    fn set_dialog(&self, py: Python<'_>, value: Option<&DateDialog>) -> PyResult<()> {
        let value = value
            .map(|v| v.with_ref(py, |d| Ok(d.clone())))
            .transpose()?;
        self.owner.edit(|e| {
            e.dialog = value;
            Ok(())
        })
    }
    #[new]
    fn new(
        py: Python<'_>,
        changed: &Bound<'_, PyAny>,
        state_out: Option<&Bound<'_, PyAny>>,
        display: &Bound<'_, PyAny>,
        selection: &Bound<'_, PyAny>,
        dialog: Option<&DateDialog>,
    ) -> PyResult<Self> {
        let selection = selection.cast::<pyo3::types::PyTuple>()?;
        if selection.len() != 2 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "expected (start, length)",
            ));
        }
        let selection = (
            strict_usize(&selection.get_item(0)?)?,
            strict_usize(&selection.get_item(1)?)?,
        );
        let dialog = dialog
            .map(|d| d.with_ref(py, |d| Ok(d.clone())))
            .transpose()?;
        Ok(Self {
            owner: Handle::new(d::Effect {
                changed: strict_bool(changed)?,
                state_out: state_out.map(strict_u32).transpose()?,
                display: strict_bytes(display)?.to_vec(),
                selection,
                dialog,
            }),
        })
    }
    #[staticmethod]
    fn default() -> Self {
        Self {
            owner: Handle::new(d::Effect::default()),
        }
    }
    #[getter]
    fn changed(&self) -> PyResult<bool> {
        self.owner.with(|e| Ok(e.changed))
    }
    #[getter]
    fn state_out(&self) -> PyResult<Option<u32>> {
        self.owner.with(|e| Ok(e.state_out))
    }
    #[getter]
    fn display<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.owner.with(|e| Ok(PyBytes::new(py, &e.display)))
    }
    #[getter]
    fn selection(&self) -> PyResult<(usize, usize)> {
        self.owner.with(|e| Ok(e.selection))
    }
    #[getter]
    fn dialog(&self) -> PyResult<Option<DateDialog>> {
        if !self.owner.with(|e| Ok(e.dialog.is_some()))? {
            return Ok(None);
        }
        Ok(Some(DateDialog {
            inner: DialogState::Owned(self.owner.project(|e| {
                e.dialog.as_mut().ok_or_else(|| {
                    pyo3::exceptions::PyReferenceError::new_err("dialog field was removed")
                })
            })),
        }))
    }
    fn with_dialog<'py>(
        &self,
        py: Python<'py>,
        callback: &Bound<'py, PyAny>,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.owner.with(|e| {
            e.dialog
                .as_mut()
                .map(|d| {
                    Frame::with_mut(py, d, |loan| {
                        callback.call1((DateDialog {
                            inner: DialogState::Scoped(loan),
                        },))
                    })
                })
                .transpose()
        })
    }
}
#[pyclass(module = "cs705.csv.date_input", frozen)]
pub enum Input {
    Display {},
    Inline {},
    Begin { first: Option<CheckedByte> },
    Text { bytes: CheckedBytes },
    Cancel {},
}
impl Input {
    fn native(&self) -> d::Input<'_> {
        match self {
            Self::Display {} => d::Input::Display,
            Self::Inline {} => d::Input::Inline,
            Self::Begin { first } => d::Input::Begin(first.map(|v| v.0)),
            Self::Text { bytes } => d::Input::Text(&bytes.0),
            Self::Cancel {} => d::Input::Cancel,
        }
    }
}
fn raw6(raw: &mut [u8]) -> PyResult<&mut [u8; 6]> {
    raw.try_into()
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("expected exactly 6 bytes"))
}
#[pyfunction(name = "format")]
fn date_format<'py>(
    py: Python<'py>,
    raw: &Bound<'_, PyAny>,
    mode_value: &Bound<'_, PyAny>,
    offset: &Bound<'_, PyAny>,
    edit: &Bound<'_, PyAny>,
    runtime: &Runtime,
) -> PyResult<Bound<'py, PyBytes>> {
    let (raw, mode, offset, edit) = (
        fixed_bytes(raw)?,
        strict_u8(mode_value)?,
        strict_i16(offset)?,
        strict_bool(edit)?,
    );
    runtime.with_runtime(|r| {
        Ok(PyBytes::new(
            py,
            &d::format(raw, mode, offset, edit, r).map_err(crate::errors::csv)?,
        ))
    })
}
#[pyfunction]
fn phase2(
    raw: &MutableBytes,
    mode_value: &Bound<'_, PyAny>,
    offset: &Bound<'_, PyAny>,
    source: &Bound<'_, PyAny>,
    runtime: &Runtime,
) -> PyResult<()> {
    let (mode, offset, source) = (
        strict_u8(mode_value)?,
        strict_i16(offset)?,
        strict_bytes(source)?,
    );
    raw.with_mut(|raw| {
        runtime.with_runtime(|r| {
            d::phase2(raw6(raw)?, mode, offset, source, r).map_err(crate::errors::csv)
        })
    })
}
#[pyfunction]
fn input(
    raw: &MutableBytes,
    mode_value: &Bound<'_, PyAny>,
    offset: &Bound<'_, PyAny>,
    input: &Input,
    runtime: &Runtime,
    effect: &Effect,
) -> PyResult<()> {
    let (mode, offset) = (strict_u8(mode_value)?, strict_i16(offset)?);
    raw.with_mut(|raw| {
        runtime.with_runtime(|r| {
            effect.owner.edit(|e| {
                d::input(raw6(raw)?, mode, offset, input.native(), r, e).map_err(crate::errors::csv)
            })
        })
    })
}
#[pyfunction]
fn edit_masked(
    raw: &MutableBytes,
    mode_value: &Bound<'_, PyAny>,
    offset: &Bound<'_, PyAny>,
    text: &Bound<'_, PyAny>,
    runtime: &Runtime,
    effect: &Effect,
) -> PyResult<()> {
    let (mode, offset, text) = (
        strict_u8(mode_value)?,
        strict_i16(offset)?,
        strict_bytes(text)?,
    );
    raw.with_mut(|raw| {
        runtime.with_runtime(|r| {
            effect.owner.edit(|e| {
                d::edit_masked(raw6(raw)?, mode, offset, text, r, e).map_err(crate::errors::csv)
            })
        })
    })
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "date_input")?;
    m.add_class::<Clock>()?;
    m.add_class::<Runtime>()?;
    m.add_class::<DateDialog>()?;
    m.add_class::<Effect>()?;
    m.add_class::<Input>()?;
    m.add("OwnedRuntime", m.getattr("Runtime")?)?;
    m.add_function(wrap_pyfunction!(date_format, &m)?)?;
    m.add_function(wrap_pyfunction!(phase2, &m)?)?;
    m.add_function(wrap_pyfunction!(input, &m)?)?;
    m.add_function(wrap_pyfunction!(edit_masked, &m)?)?;
    root.add_submodule(&m)
}
