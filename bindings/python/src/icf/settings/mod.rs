//! Stored setting helpers; no device or host network access.
pub mod digital;
pub mod extended;
pub mod gps;
mod metadata;
pub mod network;
use super::values::{Input, Output};
use crate::conversion::*;
use cs705_core::icf::settings as core;
pub use metadata::{Domain, Kind, Setting};

macro_rules! value {
    ($name:ident { $($field:ident:$ty:ty=>$set:ident;)* })=>{
        #[pyclass(module="cs705.icf.settings")]
        #[derive(Clone,Copy)] pub struct $name {pub inner:core::$name}
        impl $name {pub fn from_core(inner:core::$name)->Self{Self{inner}}}
        #[pymethods] impl $name {
            #[new] fn new($($field:&Bound<'_,PyAny>),*)->PyResult<Self>{Ok(Self::from_core(core::$name{$($field:<$ty as Input>::input($field)?),*}))}
            $(#[getter] fn $field<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{self.inner.$field.output(py)}
            #[setter] fn $set(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{self.inner.$field=<$ty as Input>::input(value)?;Ok(())})*
        }
    }
}
value!(Effect {stored:bool=>set_stored;notify_flags:u32=>set_notify_flags;});
value!(ControlState {helper_enabled:bool=>set_helper_enabled;gray:bool=>set_gray;marked:bool=>set_marked;});

#[pyclass(module = "cs705.icf.settings", frozen)]
pub struct Value {
    pub inner: core::Value,
}
impl Value {
    pub fn from_core(inner: core::Value) -> Self {
        Self { inner }
    }
}
#[pymethods]
#[allow(non_snake_case)]
impl Value {
    #[staticmethod]
    fn Number(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(core::Value::Number(strict_i64(value)?)))
    }
    #[getter]
    fn kind(&self) -> &'static str {
        "Number"
    }
    #[getter]
    fn number(&self) -> i64 {
        let core::Value::Number(value) = self.inner;
        value
    }
}
impl Input<'_> for core::Step {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(
            match super::enum_value(value, "cs705.icf.settings", "Step")? {
                0 => Self::Next,
                1 => Self::Previous,
                _ => return Err(pyo3::exceptions::PyValueError::new_err("invalid Step")),
            },
        )
    }
}
use pyo3::prelude::*;

#[pyclass(module = "cs705.icf.settings")]
#[derive(Clone, Copy)]
pub struct Selection {
    pub inner: cs705_core::icf::settings::Selection,
}
impl Selection {
    pub fn from_core(inner: cs705_core::icf::settings::Selection) -> Self {
        Self { inner }
    }
}
#[pymethods]
impl Selection {
    #[new]
    fn new(
        key: &Bound<'_, PyAny>,
        phase: &Bound<'_, PyAny>,
        candidate: &Bound<'_, PyAny>,
        accepted: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: cs705_core::icf::settings::Selection {
                key: strict_u16(key)?,
                phase: strict_u32(phase)?,
                candidate: strict_i32(candidate)?,
                accepted: strict_bool(accepted)?,
            },
        })
    }
    #[getter]
    fn key(&self) -> u16 {
        self.inner.key
    }
    #[setter]
    fn set_key(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.key = strict_u16(value)?;
        Ok(())
    }
    #[getter]
    fn phase(&self) -> u32 {
        self.inner.phase
    }
    #[setter]
    fn set_phase(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.phase = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn candidate(&self) -> i32 {
        self.inner.candidate
    }
    #[setter]
    fn set_candidate(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.candidate = strict_i32(value)?;
        Ok(())
    }
    #[getter]
    fn accepted(&self) -> bool {
        self.inner.accepted
    }
    #[setter]
    fn set_accepted(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.accepted = strict_bool(value)?;
        Ok(())
    }
}
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "settings")?;
    network::register(&m)?;
    m.add_class::<Selection>()?;
    m.add_class::<Domain>()?;
    m.add_class::<Kind>()?;
    m.add_class::<Setting>()?;
    metadata::register(&m)?;
    m.add_class::<Effect>()?;
    m.add_class::<ControlState>()?;
    m.add_class::<Value>()?;
    super::add_enum(&m, "Step", &[("Next", 0), ("Previous", 1)])?;
    extended::register(&m)?;
    digital::register(&m)?;
    gps::register(&m)?;
    parent.add_submodule(&m)
}
