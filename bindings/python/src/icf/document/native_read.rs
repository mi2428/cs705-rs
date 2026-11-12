//! Native read phases retain their private phase and native query proofs.
use super::{
    Document, Session, Target,
    initial::{self, InitialState},
    values::{self, CloneControls, DefaultSource, ValidationReport, ValidationSource},
};
use crate::{callbacks::CallbackError, conversion::*};
use cs705_core::icf::document as core;
use pyo3::prelude::*;

pub(crate) fn clone_error(error: core::CloneError) -> PyErr {
    crate::error_values::clone_error(error)
}
fn stop_code(stop: core::CloneReadStop) -> usize {
    match stop {
        core::CloneReadStop::Cancelled => 0,
        core::CloneReadStop::Busy => 1,
        core::CloneReadStop::ProbeFailed => 2,
        core::CloneReadStop::ContextMismatch => 3,
        core::CloneReadStop::ReadFailed => 4,
        core::CloneReadStop::Completed => 5,
    }
}
#[pyclass(frozen, module = "cs705.icf.document.native_read")]
pub struct CloneReadReply {
    kind: u8,
    outcome: Option<Py<crate::clone::session::Outcome>>,
}
impl CloneReadReply {
    fn to_core(&self) -> PyResult<core::CloneReadReply> {
        if self.kind == 0 {
            return Ok(core::CloneReadReply::Continue);
        }
        let outcome = self
            .outcome
            .as_ref()
            .expect("private reply payload")
            .get()
            .with_ref(|o| Ok(o.clone()))?;
        Ok(if self.kind == 1 {
            core::CloneReadReply::Probe(outcome)
        } else {
            core::CloneReadReply::Read(outcome)
        })
    }
}
#[pymethods]
impl CloneReadReply {
    #[classattr]
    #[pyo3(name = "Continue")]
    fn continue_() -> Self {
        Self {
            kind: 0,
            outcome: None,
        }
    }
    #[staticmethod]
    #[pyo3(name = "Probe")]
    fn probe(outcome: Py<crate::clone::session::Outcome>) -> Self {
        Self {
            kind: 1,
            outcome: Some(outcome),
        }
    }
    #[staticmethod]
    #[pyo3(name = "Read")]
    fn read(outcome: Py<crate::clone::session::Outcome>) -> Self {
        Self {
            kind: 2,
            outcome: Some(outcome),
        }
    }
    #[getter]
    fn outcome(&self, py: Python<'_>) -> Option<Py<crate::clone::session::Outcome>> {
        self.outcome.as_ref().map(|v| v.clone_ref(py))
    }
    #[getter]
    fn kind(&self) -> &'static str {
        ["Continue", "Probe", "Read"][self.kind as usize]
    }
}
#[pyclass(frozen, module = "cs705.icf.document.native_read")]
pub struct CloneReadStep {
    #[pyo3(get)]
    kind: &'static str,
    #[pyo3(get)]
    session: Option<Py<crate::clone::session::Session>>,
    #[pyo3(get)]
    event: Option<u32>,
    stop: Option<core::CloneReadStop>,
}
impl CloneReadStep {
    fn from_core(py: Python<'_>, step: core::CloneReadStep) -> PyResult<Self> {
        let mut result = Self {
            kind: "Apply",
            session: None,
            event: None,
            stop: None,
        };
        match step {
            core::CloneReadStep::Query(s) => {
                result.kind = "Query";
                result.session = Some(Py::new(py, crate::clone::session::Session::from_core(s))?);
            }
            core::CloneReadStep::Read(s) => {
                result.kind = "Read";
                result.session = Some(Py::new(py, crate::clone::session::Session::from_core(s))?);
            }
            core::CloneReadStep::Sync(event) => {
                result.kind = "Sync";
                result.event = Some(event);
            }
            core::CloneReadStep::Apply => {}
            core::CloneReadStep::Stopped(stop) => {
                result.kind = "Stopped";
                result.stop = Some(stop);
            }
        }
        Ok(result)
    }
}
#[pymethods]
impl CloneReadStep {
    #[staticmethod]
    #[pyo3(name = "Query")]
    fn query(session: Py<crate::clone::session::Session>) -> Self {
        Self {
            kind: "Query",
            session: Some(session),
            event: None,
            stop: None,
        }
    }
    #[staticmethod]
    #[pyo3(name = "Read")]
    fn read(session: Py<crate::clone::session::Session>) -> Self {
        Self {
            kind: "Read",
            session: Some(session),
            event: None,
            stop: None,
        }
    }
    #[staticmethod]
    #[pyo3(name = "Sync")]
    fn sync(event: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            kind: "Sync",
            session: None,
            event: Some(strict_u32(event)?),
            stop: None,
        })
    }
    #[classattr]
    #[pyo3(name = "Apply")]
    fn apply() -> Self {
        Self {
            kind: "Apply",
            session: None,
            event: None,
            stop: None,
        }
    }
    #[staticmethod]
    #[pyo3(name = "Stopped")]
    fn stopped(stop: &Bound<'_, PyAny>) -> PyResult<Self> {
        let stop = match values::enum_in(stop, "cs705.icf.document.native_read", "CloneReadStop")? {
            0 => core::CloneReadStop::Cancelled,
            1 => core::CloneReadStop::Busy,
            2 => core::CloneReadStop::ProbeFailed,
            3 => core::CloneReadStop::ContextMismatch,
            4 => core::CloneReadStop::ReadFailed,
            5 => core::CloneReadStop::Completed,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "invalid CloneReadStop",
                ));
            }
        };
        Ok(Self {
            kind: "Stopped",
            session: None,
            event: None,
            stop: Some(stop),
        })
    }
    #[getter]
    fn stop<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.stop
            .map(|s| {
                values::enum_out(
                    py,
                    "cs705.icf.document.native_read",
                    "CloneReadStop",
                    stop_code(s),
                )
            })
            .transpose()
    }
}
/// Native read controller yielding separate consuming clone.Session engines.
/// Phase transitions and completed prefixes are not rolled back after errors.
#[pyclass(frozen, module = "cs705.icf.document.native_read")]
pub struct CloneRead {
    pub target: Target<core::CloneRead>,
}
#[pymethods]
impl CloneRead {
    #[new]
    fn new(
        config: &crate::clone::session::SessionConfig,
        prompt_result: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let config = config.inner.with(|c| Ok(c.clone()))?;
        Ok(Self {
            target: Target::new(
                core::CloneRead::new(config, strict_i32(prompt_result)?).map_err(clone_error)?,
            ),
        })
    }
    fn advance(
        &self,
        py: Python<'_>,
        session: &Session,
        controls: &CloneControls,
        reply: &CloneReadReply,
    ) -> PyResult<CloneReadStep> {
        let reply = reply.to_core()?;
        let controls = controls.with_ref(|c| Ok(*c))?;
        let mut operation = self.target.take()?;
        let step = session
            .with_ref(|session| Ok(py.detach(move || operation.advance(session, controls, reply))))?
            .map_err(clone_error)?;
        CloneReadStep::from_core(py, step)
    }
    #[pyo3(signature = (document, session, source, validation, boundary))]
    /// Apply a pending read through scoped document/session boundary callbacks.
    /// The pending Apply phase is consumed even if a boundary callback raises.
    fn finish_document(
        &self,
        py: Python<'_>,
        document: &Document,
        session: &Session,
        source: Option<Py<DefaultSource>>,
        validation: &ValidationSource,
        boundary: Py<PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let interpreter = crate::scoped::interpreter(py)?;
        let source = source
            .map(|s| Ok::<_, PyErr>(s.try_borrow(py)?.clone()))
            .transpose()?;
        let validation = validation.values(py)?;
        let mut operation = self.target.take()?;
        let (failures, result) = document.with_mut(|document| {
            session.with_mut(|session| {
                Ok(py.detach(move || {
                    let mut failures = CallbackError::default();
                    let result = operation.finish_document(
                        document,
                        session,
                        source.as_ref().map(DefaultSource::as_core),
                        values::validation(&validation),
                        |event, d, s| {
                            initial::invoke_boundary(
                                &boundary,
                                &mut failures,
                                event,
                                d,
                                s,
                                interpreter,
                            )
                        },
                    );
                    (failures, result)
                }))
            })
        })?;
        let stop = failures.finish(py, result.map_err(clone_error))?;
        Ok(values::enum_out(
            py,
            "cs705.icf.document.native_read",
            "CloneReadStop",
            stop_code(stop),
        )?
        .unbind())
    }
    fn initial_snapshot(&self) -> PyResult<InitialState> {
        self.target
            .with_ref(|r| Ok(InitialState::from_core(r.initial.clone())))
    }
    fn set_initial(&self, value: &InitialState) -> PyResult<()> {
        let value = value.with_ref(|s| Ok(s.clone()))?;
        self.target.with_mut(|r| {
            r.initial = value;
            Ok(())
        })
    }
    fn outcomes_snapshot(&self) -> PyResult<Vec<crate::clone::session::Outcome>> {
        self.target.with_ref(|r| {
            Ok(r.outcomes
                .iter()
                .cloned()
                .map(crate::clone::session::Outcome::from_core)
                .collect())
        })
    }
    fn set_outcomes(&self, outcomes: Vec<Py<crate::clone::session::Outcome>>) -> PyResult<()> {
        let outcomes = outcomes
            .iter()
            .map(|o| o.get().with_ref(|o| Ok(o.clone())))
            .collect::<PyResult<Vec<_>>>()?;
        self.target.with_mut(|r| {
            r.outcomes = outcomes;
            Ok(())
        })
    }
    fn set_validation(&self, py: Python<'_>, value: Option<Py<ValidationReport>>) -> PyResult<()> {
        let value = value
            .map(|v| Ok::<_, PyErr>(v.try_borrow(py)?.inner.clone()))
            .transpose()?;
        self.target.with_mut(|r| {
            r.validation = value;
            Ok(())
        })
    }
    fn validation_snapshot(&self) -> PyResult<Option<ValidationReport>> {
        self.target
            .with_ref(|r| Ok(r.validation.clone().map(|inner| ValidationReport { inner })))
    }
    #[getter]
    fn selector(&self) -> PyResult<u8> {
        self.target.with_ref(|r| Ok(r.selector))
    }
    #[setter]
    fn set_selector(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_u8(v)?;
        self.target.with_mut(|r| {
            r.selector = v;
            Ok(())
        })
    }
    #[getter]
    fn fc(&self) -> PyResult<u8> {
        self.target.with_ref(|r| Ok(r.fc))
    }
    #[setter]
    fn set_fc(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_u8(v)?;
        self.target.with_mut(|r| {
            r.fc = v;
            Ok(())
        })
    }
    #[getter]
    fn fd(&self) -> PyResult<u8> {
        self.target.with_ref(|r| Ok(r.fd))
    }
    #[setter]
    fn set_fd(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_u8(v)?;
        self.target.with_mut(|r| {
            r.fd = v;
            Ok(())
        })
    }
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "native_read")?;
    values::add_enum(
        &m,
        "cs705.icf.document.native_read",
        "CloneReadStop",
        &[
            "Cancelled",
            "Busy",
            "ProbeFailed",
            "ContextMismatch",
            "ReadFailed",
            "Completed",
        ],
    )?;
    m.add_class::<CloneRead>()?;
    m.add_class::<CloneReadStep>()?;
    m.add_class::<CloneReadReply>()?;
    for name in [
        "CloneRead",
        "CloneReadStep",
        "CloneReadReply",
        "CloneReadStop",
    ] {
        root.add(name, m.getattr(name)?)?;
    }
    root.add_submodule(&m)
}
