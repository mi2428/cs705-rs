//! Clone-write gates yield actual consuming engines, not fabricated success states.
use super::{
    Document, Session, Target,
    native_read::clone_error,
    options::CloneOptions,
    values::{self, CloneControls, ValidationReport, ValidationSource},
};
use crate::{callbacks::CallbackError, conversion::*, scoped::Frame};
use cs705_core::icf::document as core;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

fn stop_code(stop: core::CloneStop) -> usize {
    match stop {
        core::CloneStop::Busy => 0,
        core::CloneStop::ProbeFailed => 1,
        core::CloneStop::ContextMismatch => 2,
        core::CloneStop::InvalidItems => 3,
        core::CloneStop::WrapperState => 4,
        core::CloneStop::Declined => 5,
        core::CloneStop::Completed => 6,
        core::CloneStop::WriteFailed => 7,
    }
}

#[pyclass(frozen, module = "cs705.icf.document")]
pub struct CloneReply {
    kind: u8,
    outcome: Option<Py<crate::clone::session::Outcome>>,
    choice: i32,
}
impl CloneReply {
    fn to_core(&self) -> PyResult<core::CloneReply> {
        match self.kind {
            0 => Ok(core::CloneReply::Continue),
            2 => Ok(core::CloneReply::Confirmation(self.choice)),
            _ => {
                let outcome = self
                    .outcome
                    .as_ref()
                    .expect("private reply payload")
                    .get()
                    .with_ref(|o| Ok(o.clone()))?;
                Ok(if self.kind == 1 {
                    core::CloneReply::Probe(outcome)
                } else {
                    core::CloneReply::Written(outcome)
                })
            }
        }
    }
}
#[pymethods]
impl CloneReply {
    #[classattr]
    #[pyo3(name = "Continue")]
    fn continue_() -> Self {
        Self {
            kind: 0,
            outcome: None,
            choice: 0,
        }
    }
    #[staticmethod]
    #[pyo3(name = "Probe")]
    fn probe(outcome: Py<crate::clone::session::Outcome>) -> Self {
        Self {
            kind: 1,
            outcome: Some(outcome),
            choice: 0,
        }
    }
    #[staticmethod]
    #[pyo3(name = "Confirmation")]
    fn confirmation(choice: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            kind: 2,
            outcome: None,
            choice: strict_i32(choice)?,
        })
    }
    #[staticmethod]
    #[pyo3(name = "Written")]
    fn written(outcome: Py<crate::clone::session::Outcome>) -> Self {
        Self {
            kind: 3,
            outcome: Some(outcome),
            choice: 0,
        }
    }
    #[getter]
    fn kind(&self) -> &'static str {
        ["Continue", "Probe", "Confirmation", "Written"][self.kind as usize]
    }
    #[getter]
    fn outcome(&self, py: Python<'_>) -> Option<Py<crate::clone::session::Outcome>> {
        self.outcome.as_ref().map(|o| o.clone_ref(py))
    }
    #[getter]
    fn choice(&self) -> Option<i32> {
        (self.kind == 2).then_some(self.choice)
    }
}
#[pyclass(frozen, module = "cs705.icf.document")]
pub struct CloneStep {
    #[pyo3(get)]
    kind: &'static str,
    #[pyo3(get)]
    session: Option<Py<crate::clone::session::Session>>,
    #[pyo3(get)]
    event: Option<u32>,
    #[pyo3(get)]
    warning: Option<bool>,
    #[pyo3(get)]
    selector: Option<u8>,
    stop: Option<core::CloneStop>,
}
impl CloneStep {
    fn from_core(py: Python<'_>, step: core::CloneStep) -> PyResult<Self> {
        let mut result = Self {
            kind: "Finalize",
            session: None,
            event: None,
            warning: None,
            selector: None,
            stop: None,
        };
        match step {
            core::CloneStep::Query(s) => {
                result.kind = "Query";
                result.session = Some(Py::new(py, crate::clone::session::Session::from_core(s))?);
            }
            core::CloneStep::Write(s) => {
                result.kind = "Write";
                result.session = Some(Py::new(py, crate::clone::session::Session::from_core(s))?);
            }
            core::CloneStep::Sync(event) => {
                result.kind = "Sync";
                result.event = Some(event);
            }
            core::CloneStep::Confirm { warning, selector } => {
                result.kind = "Confirm";
                result.warning = Some(warning);
                result.selector = Some(selector);
            }
            core::CloneStep::Finalize => {}
            core::CloneStep::Stopped(stop) => {
                result.kind = "Stopped";
                result.stop = Some(stop);
            }
        }
        Ok(result)
    }
}
#[pymethods]
impl CloneStep {
    #[staticmethod]
    #[pyo3(name = "Query")]
    fn query(session: Py<crate::clone::session::Session>) -> Self {
        Self {
            kind: "Query",
            session: Some(session),
            event: None,
            warning: None,
            selector: None,
            stop: None,
        }
    }
    #[staticmethod]
    #[pyo3(name = "Write")]
    fn write(session: Py<crate::clone::session::Session>) -> Self {
        Self {
            kind: "Write",
            session: Some(session),
            event: None,
            warning: None,
            selector: None,
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
            warning: None,
            selector: None,
            stop: None,
        })
    }
    #[staticmethod]
    #[pyo3(name = "Confirm")]
    fn confirm(warning: &Bound<'_, PyAny>, selector: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            kind: "Confirm",
            session: None,
            event: None,
            warning: Some(strict_bool(warning)?),
            selector: Some(strict_u8(selector)?),
            stop: None,
        })
    }
    #[classattr]
    #[pyo3(name = "Finalize")]
    fn finalize_() -> Self {
        Self {
            kind: "Finalize",
            session: None,
            event: None,
            warning: None,
            selector: None,
            stop: None,
        }
    }
    #[staticmethod]
    #[pyo3(name = "Stopped")]
    fn stopped(stop: &Bound<'_, PyAny>) -> PyResult<Self> {
        let stop = match values::enum_in(stop, "cs705.icf.document", "CloneStop")? {
            0 => core::CloneStop::Busy,
            1 => core::CloneStop::ProbeFailed,
            2 => core::CloneStop::ContextMismatch,
            3 => core::CloneStop::InvalidItems,
            4 => core::CloneStop::WrapperState,
            5 => core::CloneStop::Declined,
            6 => core::CloneStop::Completed,
            7 => core::CloneStop::WriteFailed,
            _ => return Err(pyo3::exceptions::PyValueError::new_err("invalid CloneStop")),
        };
        Ok(Self {
            kind: "Stopped",
            session: None,
            event: None,
            warning: None,
            selector: None,
            stop: Some(stop),
        })
    }
    #[getter]
    fn stop<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.stop
            .map(|s| values::enum_out(py, "cs705.icf.document", "CloneStop", stop_code(s)))
            .transpose()
    }
}
/// Native write controller yielding separate consuming clone.Session engines.
/// Snapshot getters copy state; callback loans expire on callback return.
#[pyclass(frozen, module = "cs705.icf.document")]
pub struct CloneWrite {
    pub target: Target<core::CloneWrite>,
}
macro_rules! write_methods {
    ($($field:ident, $set:ident: $ty:ty => $strict:ident);*) => {
        #[pymethods]
        impl CloneWrite {
            #[new]
            fn new(config: &crate::clone::session::SessionConfig, mask: &Bound<'_, PyAny>, reset: Py<crate::icf::normalizer::ResetState>) -> PyResult<Self> {
                let config = config.inner.with(|c| Ok(c.clone()))?;
                let reset = reset.get().with_ref(|r| Ok(*r))?;
                Ok(Self { target: Target::new(core::CloneWrite::new(config, strict_u8(mask)?, reset).map_err(clone_error)?) })
            }
            fn advance(&self, py: Python<'_>, document: &Document, session: &Session, controls: &CloneControls, validation: &ValidationSource, reply: &CloneReply) -> PyResult<CloneStep> {
                let controls = controls.with_ref(|c| Ok(*c))?;
                let validation = validation.values(py)?;
                let reply = reply.to_core()?;
                let step = self.target.with_mut(|operation| {
                    document.with_mut(|document| {
                        session.with_ref(|session| {
                            Ok(py.detach(move || operation.advance(
                                document, session, controls, values::validation(&validation), reply,
                            )))
                        })
                    })
                })?.map_err(clone_error)?;
                CloneStep::from_core(py, step)
            }
            /// A raised information callback retains its writes, skips mask release
            /// and wrapper reset, and leaves this controller in a failed phase.
            fn finalize(&self, py: Python<'_>, controls: &CloneControls, request_information: Py<PyAny>) -> PyResult<Py<PyAny>> {
                let interpreter = crate::scoped::interpreter(py)?;
                let (failures, result) = self.target.with_mut(|operation| {
                    controls.with_mut(|controls| {
                        Ok(py.detach(move || {
                            let mut failures = CallbackError::default();
                            let result = operation.finalize(controls, |operation, controls| {
                                let result = super::adapters::attach_checked(interpreter, |py| {
                                    Frame::with_mut(py, operation, |operation| {
                                        Frame::with_mut(py, controls, |controls| {
                                            let operation = Py::new(py, Self { target: Target::Scoped(operation) })?;
                                            let controls = Py::new(py, CloneControls::from_loan(controls))?;
                                            super::adapters::require_none(request_information.bind(py).call1((operation, controls))?)
                                        })
                                    })
                                });
                                failures.capture(result).map_err(|_| cs705_core::icf::Error::Operation("Python clone final callback failed".into()))
                            });
                            (failures, result)
                        }))
                    })
                })?;
                let stop = failures.finish(py, result.map_err(clone_error))?;
                Ok(values::enum_out(py, "cs705.icf.document", "CloneStop", stop_code(stop))?.unbind())
            }
            $(#[getter] fn $field(&self) -> PyResult<$ty> { self.target.with_ref(|o| Ok(o.$field)) }
            #[setter] fn $set(&self, value: &Bound<'_, PyAny>) -> PyResult<()> { let value = $strict(value)?; self.target.with_mut(|o| { o.$field = value; Ok(()) }) })*
            #[getter] fn copied_tail<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> { self.target.with_ref(|o| Ok(PyBytes::new(py, &o.copied_tail))) }
            #[setter] fn set_copied_tail(&self, value: &Bound<'_, PyAny>) -> PyResult<()> { let value = strict_bytes(value)?.to_vec(); self.target.with_mut(|o| { o.copied_tail = value; Ok(()) }) }
            #[getter] fn wire_image<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> { self.target.with_ref(|o| Ok(PyBytes::new(py, &o.wire_image))) }
            #[setter] fn set_wire_image(&self, value: &Bound<'_, PyAny>) -> PyResult<()> { let value = strict_bytes(value)?.to_vec(); self.target.with_mut(|o| { o.wire_image = value; Ok(()) }) }
            fn outcomes_snapshot(&self) -> PyResult<Vec<crate::clone::session::Outcome>> { self.target.with_ref(|o| Ok(o.outcomes.iter().cloned().map(crate::clone::session::Outcome::from_core).collect())) }
            fn set_outcomes(&self, outcomes: Vec<Py<crate::clone::session::Outcome>>) -> PyResult<()> { let outcomes=outcomes.iter().map(|o| o.get().with_ref(|o|Ok(o.clone()))).collect::<PyResult<Vec<_>>>()?; self.target.with_mut(|o| {o.outcomes=outcomes;Ok(())}) }
            fn validation_snapshot(&self) -> PyResult<Option<ValidationReport>> { self.target.with_ref(|o| Ok(o.validation.clone().map(|inner| ValidationReport { inner }))) }
            fn set_validation(&self, py:Python<'_>, value:Option<Py<ValidationReport>>)->PyResult<()>{let value=value.map(|v|Ok::<_,PyErr>(v.try_borrow(py)?.inner.clone())).transpose()?;self.target.with_mut(|o|{o.validation=value;Ok(())})}
            fn reset_snapshot(&self) -> PyResult<crate::icf::normalizer::ResetState> { self.target.with_ref(|o| Ok(crate::icf::normalizer::ResetState::from_core(o.reset))) }
            fn set_reset(&self, value: &crate::icf::normalizer::ResetState) -> PyResult<()> { let value = value.with_ref(|r| Ok(*r))?; self.target.with_mut(|o| { o.reset = value; Ok(()) }) }
            fn saved_context_snapshot(&self) -> PyResult<Option<crate::clone::ContextFields>> { self.target.with_ref(|o| Ok(o.saved_context.map(crate::clone::ContextFields::from_core))) }
            fn set_saved_context(&self, value: Option<&crate::clone::ContextFields>) -> PyResult<()> { let value = value.map(|v| v.with_ref(|c| Ok(*c))).transpose()?; self.target.with_mut(|o| { o.saved_context = value; Ok(()) }) }
            fn options_snapshot(&self) -> PyResult<Option<CloneOptions>> { self.target.with_ref(|o| Ok(o.options.clone().map(CloneOptions::from_core))) }
            fn set_options(&self, value: Option<&CloneOptions>) -> PyResult<()> { let value = value.map(|v| v.with_ref(|c| Ok(c.clone()))).transpose()?; self.target.with_mut(|o| { o.options = value; Ok(()) }) }
            fn preparation_snapshot(&self) -> PyResult<super::preparation::ClonePreparation> { self.target.with_ref(|o| Ok(super::preparation::ClonePreparation::from_core(super::preparation::ClonePreparation::snapshot(&o.preparation)))) }
            fn set_preparation(&self, value: &super::preparation::ClonePreparation) -> PyResult<()> { let value = value.target.with_ref(|p| Ok(super::preparation::ClonePreparation::snapshot(p)))?; self.target.with_mut(|o| { o.preparation = value; Ok(()) }) }
            #[getter] fn active_mask<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyTuple>>> {
                let mask = self.target.with_ref(|o| Ok(o.active_mask.clone()))?;
                mask.map(|ranges| { let range = py.import("builtins")?.getattr("range")?; let ranges = ranges.iter().map(|r| range.call1((r.start, r.end))).collect::<PyResult<Vec<_>>>()?; PyTuple::new(py, ranges) }).transpose()
            }
            #[setter] fn set_active_mask(&self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
                let ranges = value.map(|v| v.try_iter()?.map(|r| { let r = strict_range(&r?)?; Ok(u32::try_from(r.start).map_err(|_| pyo3::exceptions::PyOverflowError::new_err("range start exceeds u32"))?..u32::try_from(r.end).map_err(|_| pyo3::exceptions::PyOverflowError::new_err("range stop exceeds u32"))?) }).collect::<PyResult<Vec<_>>>()).transpose()?;
                self.target.with_mut(|o| { o.active_mask = ranges; Ok(()) })
            }
        }
    };
}
write_methods!(saved_selector, set_saved_selector: u8 => strict_u8; fc, set_fc: u8 => strict_u8; fd, set_fd: u8 => strict_u8; information_requested, set_information_requested: bool => strict_bool);

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    values::add_enum(
        m,
        "cs705.icf.document",
        "CloneStop",
        &[
            "Busy",
            "ProbeFailed",
            "ContextMismatch",
            "InvalidItems",
            "WrapperState",
            "Declined",
            "Completed",
            "WriteFailed",
        ],
    )?;
    m.add_class::<CloneWrite>()?;
    m.add_class::<CloneReply>()?;
    m.add_class::<CloneStep>()?;
    Ok(())
}
