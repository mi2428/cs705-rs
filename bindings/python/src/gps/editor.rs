//! GPS row editing over the shared ICF/decoded-record owners.
use crate::icf::settings::extended::{
    ChoiceEffect, ChoiceInput, NumberEffect, NumberInput, TextInput,
};
use crate::{
    conversion::*,
    errors,
    icf::{Icf, record::DecodedRecord},
};
use cs705_core::gps::editor as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

#[pyclass(module = "cs705.gps.editor", frozen)]
pub struct RowAlarmChoice {
    inner: core::RowAlarmChoice,
}
#[pymethods]
#[allow(non_snake_case)]
impl RowAlarmChoice {
    #[classattr]
    fn Off() -> Self {
        Self {
            inner: core::RowAlarmChoice::Off,
        }
    }
    #[classattr]
    fn Memory() -> Self {
        Self {
            inner: core::RowAlarmChoice::Memory,
        }
    }
    #[classattr]
    fn Receive() -> Self {
        Self {
            inner: core::RowAlarmChoice::Receive,
        }
    }
    #[classattr]
    fn All() -> Self {
        Self {
            inner: core::RowAlarmChoice::All,
        }
    }
    #[classattr]
    fn ALL() -> Vec<Self> {
        core::RowAlarmChoice::ALL
            .into_iter()
            .map(|inner| Self { inner })
            .collect()
    }
    #[staticmethod]
    fn Group(group: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::RowAlarmChoice::Group(strict_u8(group)?),
        })
    }
    fn index(&self) -> PyResult<u8> {
        self.inner.index().map_err(errors::icf)
    }
    #[getter]
    fn variant(&self) -> &'static str {
        match self.inner {
            core::RowAlarmChoice::Off => "Off",
            core::RowAlarmChoice::Memory => "Memory",
            core::RowAlarmChoice::Receive => "Receive",
            core::RowAlarmChoice::All => "All",
            core::RowAlarmChoice::Group(_) => "Group",
        }
    }
    #[getter]
    fn group(&self) -> Option<u8> {
        if let core::RowAlarmChoice::Group(g) = self.inner {
            Some(g)
        } else {
            None
        }
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
#[pyclass(module = "cs705.gps.editor")]
pub struct AlarmView {
    pub(crate) inner: core::AlarmView,
}
#[pymethods]
impl AlarmView {
    #[new]
    fn new(
        group: &Bound<'_, PyAny>,
        start: &Bound<'_, PyAny>,
        active: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::AlarmView {
                group: strict_u8(group)?,
                start: strict_u32(start)?,
                active: strict_u32(active)?,
            },
        })
    }
    #[getter]
    fn group(&self) -> u8 {
        self.inner.group
    }
    #[setter]
    fn set_group(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.group = strict_u8(v)?;
        Ok(())
    }
    #[getter]
    fn start(&self) -> u32 {
        self.inner.start
    }
    #[setter]
    fn set_start(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.start = strict_u32(v)?;
        Ok(())
    }
    #[getter]
    fn active(&self) -> u32 {
        self.inner.active
    }
    #[setter]
    fn set_active(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.active = strict_u32(v)?;
        Ok(())
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
#[pyclass(module = "cs705.gps.editor")]
pub struct RowAlarmEffect {
    #[pyo3(get, set)]
    choice: Py<ChoiceEffect>,
    marked: bool,
    notify_39: bool,
}
impl RowAlarmEffect {
    fn from_core(py: Python<'_>, value: core::RowAlarmEffect) -> PyResult<Self> {
        Ok(Self {
            choice: Py::new(py, ChoiceEffect::from_core(value.choice))?,
            marked: value.marked,
            notify_39: value.notify_39,
        })
    }
}
#[pymethods]
impl RowAlarmEffect {
    #[new]
    fn new(
        choice: Py<ChoiceEffect>,
        marked: &Bound<'_, PyAny>,
        notify_39: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            choice,
            marked: strict_bool(marked)?,
            notify_39: strict_bool(notify_39)?,
        })
    }
    #[getter]
    fn marked(&self) -> bool {
        self.marked
    }
    #[setter]
    fn set_marked(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.marked = strict_bool(v)?;
        Ok(())
    }
    #[getter]
    fn notify_39(&self) -> bool {
        self.notify_39
    }
    #[setter]
    fn set_notify_39(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.notify_39 = strict_bool(v)?;
        Ok(())
    }
    fn __eq__(&self, other: &Self) -> PyResult<bool> {
        let a = self.choice.get().with_ref(|c| Ok(*c))?;
        let b = other.choice.get().with_ref(|c| Ok(*c))?;
        Ok(a == b && self.marked == other.marked && self.notify_39 == other.notify_39)
    }
}
#[pyclass(module = "cs705.gps.editor")]
pub struct GroupNameEffect {
    inner: core::GroupNameEffect,
}
#[pymethods]
impl GroupNameEffect {
    #[new]
    #[pyo3(signature=(stored,state_out,display,label,refresh_if_binding_matches,shared_input))]
    fn new(
        stored: &Bound<'_, PyAny>,
        state_out: &Bound<'_, PyAny>,
        display: &Bound<'_, PyAny>,
        label: Option<(Bound<'_, PyAny>, Bound<'_, PyAny>)>,
        refresh_if_binding_matches: &Bound<'_, PyAny>,
        shared_input: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::GroupNameEffect {
                stored: strict_bool(stored)?,
                state_out: strict_u32(state_out)?,
                display: strict_bytes(display)?.to_vec(),
                label: label
                    .map(|(id, b)| Ok::<_, PyErr>((strict_u32(&id)?, strict_bytes(&b)?.to_vec())))
                    .transpose()?,
                refresh_if_binding_matches: strict_bool(refresh_if_binding_matches)?,
                shared_input: strict_bytes(shared_input)?.to_vec(),
            },
        })
    }
    #[getter]
    fn stored(&self) -> bool {
        self.inner.stored
    }
    #[setter]
    fn set_stored(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.stored = strict_bool(v)?;
        Ok(())
    }
    #[getter]
    fn state_out(&self) -> u32 {
        self.inner.state_out
    }
    #[setter]
    fn set_state_out(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.state_out = strict_u32(v)?;
        Ok(())
    }
    #[getter]
    fn display<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.display)
    }
    #[setter]
    fn set_display(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.display = strict_bytes(v)?.to_vec();
        Ok(())
    }
    #[getter]
    fn shared_input<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.shared_input)
    }
    #[setter]
    fn set_shared_input(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.shared_input = strict_bytes(v)?.to_vec();
        Ok(())
    }
    #[getter]
    fn label<'py>(&self, py: Python<'py>) -> Option<(u32, Bound<'py, PyBytes>)> {
        self.inner
            .label
            .as_ref()
            .map(|(id, b)| (*id, PyBytes::new(py, b)))
    }
    #[setter]
    fn set_label(&mut self, v: Option<(Bound<'_, PyAny>, Bound<'_, PyAny>)>) -> PyResult<()> {
        self.inner.label = v
            .map(|(id, b)| Ok::<_, PyErr>((strict_u32(&id)?, strict_bytes(&b)?.to_vec())))
            .transpose()?;
        Ok(())
    }
    #[getter]
    fn refresh_if_binding_matches(&self) -> bool {
        self.inner.refresh_if_binding_matches
    }
    #[setter]
    fn set_refresh_if_binding_matches(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.refresh_if_binding_matches = strict_bool(v)?;
        Ok(())
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
#[pyfunction]
fn row_alarm(
    py: Python<'_>,
    image: &Icf,
    view: &AlarmView,
    input: &ChoiceInput,
) -> PyResult<RowAlarmEffect> {
    let input = input.to_core(py)?;
    image.with_mut(|image| {
        RowAlarmEffect::from_core(
            py,
            core::row_alarm(image, view.inner, input).map_err(errors::icf)?,
        )
    })
}
#[pyfunction]
fn altitude(
    py: Python<'_>,
    image: &Icf,
    absolute: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
    input: &NumberInput,
    point: &Bound<'_, PyAny>,
) -> PyResult<NumberEffect> {
    let (a, p) = (strict_usize(absolute)?, strict_u8(point)?);
    input.with_core(py, |input| {
        image.with_mut(|image| {
            holder.with_mut(|h| {
                Ok(NumberEffect::from_core(
                    core::altitude(image, a, h, input, p).map_err(errors::icf)?,
                ))
            })
        })
    })
}
#[pyfunction]
fn group_name(
    py: Python<'_>,
    image: &Icf,
    group: &Bound<'_, PyAny>,
    input: &TextInput,
) -> PyResult<GroupNameEffect> {
    let group = strict_usize(group)?;
    input.with_core(py, |input| {
        image.with_mut(|image| {
            Ok(GroupNameEffect {
                inner: core::group_name(image, group, input).map_err(errors::csv)?,
            })
        })
    })
}
#[pyfunction]
fn load(
    image: &Icf,
    absolute: &Bound<'_, PyAny>,
    binding: &Bound<'_, PyAny>,
) -> PyResult<DecodedRecord> {
    let (a, b) = (strict_usize(absolute)?, strict_u32(binding)?);
    image.with_ref(|image| {
        Ok(DecodedRecord::from_core(
            core::load(image, a, b).map_err(errors::icf)?,
        ))
    })
}
#[pyfunction]
fn save(image: &Icf, absolute: &Bound<'_, PyAny>, holder: &DecodedRecord) -> PyResult<bool> {
    let a = strict_usize(absolute)?;
    image.with_mut(|image| holder.with_ref(|h| core::save(image, a, h).map_err(errors::icf)))
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "editor")?;
    m.add_class::<RowAlarmChoice>()?;
    m.add_class::<AlarmView>()?;
    m.add_class::<RowAlarmEffect>()?;
    m.add_class::<GroupNameEffect>()?;
    m.add_function(wrap_pyfunction!(row_alarm, &m)?)?;
    m.add_function(wrap_pyfunction!(altitude, &m)?)?;
    m.add_function(wrap_pyfunction!(group_name, &m)?)?;
    m.add_function(wrap_pyfunction!(load, &m)?)?;
    m.add_function(wrap_pyfunction!(save, &m)?)?;
    root.add_submodule(&m)
}
