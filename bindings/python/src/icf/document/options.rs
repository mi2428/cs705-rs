//! Option words retain partial native changes; no widget or dialog is fabricated.
use super::Target;
use super::values::{add_enum, enum_in};
use crate::{callbacks::CallbackError, conversion::*, errors, scoped::Frame};
use cs705_core::icf::document::options as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

#[pyclass(frozen, module = "cs705.icf.document.options")]
pub struct CloneOptions {
    pub target: Target<core::CloneOptions>,
}
impl CloneOptions {
    pub fn from_core(value: core::CloneOptions) -> Self {
        Self {
            target: Target::new(value),
        }
    }
    pub fn with_mut<R>(
        &self,
        f: impl FnOnce(&mut core::CloneOptions) -> PyResult<R>,
    ) -> PyResult<R> {
        self.target.with_mut(f)
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::CloneOptions) -> PyResult<R>) -> PyResult<R> {
        self.target.with_ref(f)
    }
    pub fn from_loan(loan: crate::scoped::Loan<core::CloneOptions>) -> Self {
        Self {
            target: Target::Scoped(loan),
        }
    }
}
#[pymethods]
impl CloneOptions {
    #[new]
    fn new(word: &Bound<'_, PyAny>, states: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(core::CloneOptions {
            word: strict_u32(word)?,
            states: fixed_bytes::<5>(states)?,
        }))
    }
    #[staticmethod]
    fn open(word: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(core::CloneOptions::open(strict_u32(word)?)))
    }
    #[getter]
    fn word(&self) -> PyResult<u32> {
        self.with_ref(|v| Ok(v.word))
    }
    #[setter]
    fn set_word(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u32(value)?;
        self.with_mut(|v| {
            v.word = value;
            Ok(())
        })
    }
    #[getter]
    fn states<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(|v| Ok(PyBytes::new(py, &v.states)))
    }
    #[setter]
    fn set_states(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = fixed_bytes::<5>(value)?;
        self.with_mut(|v| {
            v.states = value;
            Ok(())
        })
    }
    fn initialize(&self, index: &Bound<'_, PyAny>) -> PyResult<()> {
        let index = strict_usize(index)?;
        self.with_mut(|v| Ok(v.initialize(index)))?
            .map_err(errors::icf)
    }
    fn press(&self, button: &Bound<'_, PyAny>) -> PyResult<bool> {
        let button = match enum_in(button, "cs705.icf.document.options", "CloneOptionButton")? {
            0 => core::CloneOptionButton::Ok,
            1 => core::CloneOptionButton::Cancel,
            _ => core::CloneOptionButton::FormEvent,
        };
        self.with_mut(|v| Ok(v.press(button)))?.map_err(errors::icf)
    }
    fn mask(&self) -> PyResult<u8> {
        self.with_ref(|v| Ok(v.mask()))
    }
    /// Each getter receives a mutable options loan that expires on callback return.
    /// A getter exception retains earlier option changes and stops later getters.
    fn save(&self, py: Python<'_>, button: &Bound<'_, PyAny>, getter: Py<PyAny>) -> PyResult<bool> {
        let interpreter = crate::scoped::interpreter(py)?;
        let button = match enum_in(button, "cs705.icf.document.options", "CloneOptionButton")? {
            0 => core::CloneOptionButton::Ok,
            1 => core::CloneOptionButton::Cancel,
            _ => core::CloneOptionButton::FormEvent,
        };
        let (errors, result) = self.with_mut(|options| {
            Ok(py.detach(move || {
                let mut errors = CallbackError::default();
                let result = options.save(button, |index, state| {
                    let result = super::adapters::attach_checked(interpreter, |py| {
                        Frame::with_mut(py, state, |loan| {
                            let view = Py::new(py, Self::from_loan(loan))?;
                            strict_u32(&getter.bind(py).call1((index, view))?)
                        })
                    });
                    errors.capture(result).map_err(|_| {
                        cs705_core::icf::Error::Operation("Python option getter failed".into())
                    })
                });
                (errors, result)
            }))
        })?;
        errors.finish(py, result.map_err(errors::icf))
    }
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "options")?;
    add_enum(
        &m,
        "cs705.icf.document.options",
        "CloneOptionButton",
        &["Ok", "Cancel", "FormEvent"],
    )?;
    m.add_class::<CloneOptions>()?;
    root.add("CloneOptionButton", m.getattr("CloneOptionButton")?)?;
    root.add("CloneOptions", m.getattr("CloneOptions")?)?;
    root.add_submodule(&m)
}
