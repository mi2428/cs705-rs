//! Public save reports keep typed nested Python owners and inspectable errors.
use super::{
    lifecycle_values::SaveSelection,
    values::{self, ValidationReport},
};
use crate::conversion::*;
use cs705_core::icf::document::{self as core, lifecycle};
use pyo3::prelude::*;
use pyo3::types::PyList;
use pyo3::{PyTraverseError, PyVisit};

pub(crate) fn finish_report<T: pyo3::PyTypeInfo>(
    py: Python<'_>,
    failures: crate::callbacks::CallbackError,
    result: PyResult<Py<T>>,
) -> PyResult<Py<T>> {
    match result {
        Ok(report) => {
            let returned = report.clone_ref(py);
            failures.finish_with_outcome(py, Ok(returned), report.into_any())
        }
        Err(error) => failures.finish(py, Err(error)),
    }
}

#[pyclass(module = "cs705.icf.document")]
pub struct ResetEffects {
    #[pyo3(get, set)]
    pub correction: Py<crate::icf::normalizer::ResetReport>,
    #[pyo3(get, set)]
    pub notifications: Py<PyList>,
}
impl ResetEffects {
    pub fn from_core(py: Python<'_>, value: core::ResetEffects) -> PyResult<Self> {
        let notifications = value
            .notifications
            .into_iter()
            .map(|n| Py::new(py, crate::icf::special::Notification::from_core(n)))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            correction: Py::new(
                py,
                crate::icf::normalizer::ResetReport::from_core(py, value.correction)?,
            )?,
            notifications: PyList::new(py, notifications)?.unbind(),
        })
    }
    pub fn to_core(&self, py: Python<'_>) -> PyResult<core::ResetEffects> {
        let correction = self.correction.try_borrow(py)?.to_core(py)?;
        let notifications = self
            .notifications
            .bind(py)
            .iter()
            .map(|v| {
                v.extract::<PyRef<'_, crate::icf::special::Notification>>()?
                    .with_ref(|n| Ok(n.clone()))
            })
            .collect::<PyResult<_>>()?;
        Ok(core::ResetEffects {
            correction,
            notifications,
        })
    }
}
#[pymethods]
impl ResetEffects {
    #[new]
    fn new(
        py: Python<'_>,
        correction: Py<crate::icf::normalizer::ResetReport>,
        notifications: Vec<Py<crate::icf::special::Notification>>,
    ) -> PyResult<Self> {
        Ok(Self {
            correction,
            notifications: PyList::new(py, notifications)?.unbind(),
        })
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.correction)?;
        visit.call(&self.notifications)
    }
}
#[pyclass(module = "cs705.icf.document")]
pub struct SaveReport {
    pub status: core::SaveStatus,
    #[pyo3(get, set)]
    pub validation: Py<ValidationReport>,
    #[pyo3(get, set)]
    pub correction: Option<Py<ResetEffects>>,
    checksum_written: bool,
}
impl SaveReport {
    pub fn from_core(py: Python<'_>, value: core::SaveReport) -> PyResult<Self> {
        Ok(Self {
            status: value.status,
            validation: Py::new(
                py,
                ValidationReport {
                    inner: value.validation,
                },
            )?,
            correction: value
                .correction
                .map(|v| Py::new(py, ResetEffects::from_core(py, v)?))
                .transpose()?,
            checksum_written: value.checksum_written,
        })
    }
    pub fn to_core(&self, py: Python<'_>) -> PyResult<core::SaveReport> {
        Ok(core::SaveReport {
            status: self.status,
            validation: self.validation.try_borrow(py)?.inner.clone(),
            correction: self
                .correction
                .as_ref()
                .map(|c| c.try_borrow(py)?.to_core(py))
                .transpose()?,
            checksum_written: self.checksum_written,
        })
    }
}
fn save_status(value: &Bound<'_, PyAny>) -> PyResult<core::SaveStatus> {
    Ok(
        match values::enum_in(value, "cs705.icf.document", "SaveStatus")? {
            0 => core::SaveStatus::Saved,
            1 => core::SaveStatus::Cancelled,
            _ => core::SaveStatus::RetrySaveAs,
        },
    )
}
#[pymethods]
impl SaveReport {
    #[new]
    #[pyo3(signature = (status, validation, correction, checksum_written))]
    fn new(
        status: &Bound<'_, PyAny>,
        validation: Py<ValidationReport>,
        correction: Option<Py<ResetEffects>>,
        checksum_written: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            status: save_status(status)?,
            validation,
            correction,
            checksum_written: strict_bool(checksum_written)?,
        })
    }
    #[getter]
    fn status<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        values::enum_out(
            py,
            "cs705.icf.document",
            "SaveStatus",
            match self.status {
                core::SaveStatus::Saved => 0,
                core::SaveStatus::Cancelled => 1,
                core::SaveStatus::RetrySaveAs => 2,
            },
        )
    }
    #[setter]
    fn set_status(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.status = save_status(value)?;
        Ok(())
    }
    #[getter]
    fn checksum_written(&self) -> bool {
        self.checksum_written
    }
    #[setter]
    fn set_checksum_written(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.checksum_written = strict_bool(value)?;
        Ok(())
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.validation)?;
        visit.call(&self.correction)
    }
}

#[pyclass(module = "cs705.icf.document.lifecycle")]
pub struct SaveAttempt {
    #[pyo3(get, set)]
    pub selection: Py<SaveSelection>,
    legacy_choice: i32,
    warning_choice: Option<i32>,
    #[pyo3(get, set)]
    pub report: Option<Py<SaveReport>>,
    #[pyo3(get, set)]
    pub publication_error: Option<Py<pyo3::exceptions::PyBaseException>>,
}
impl SaveAttempt {
    pub fn from_core(py: Python<'_>, value: lifecycle::SaveAttempt) -> PyResult<Self> {
        Ok(Self {
            selection: Py::new(
                py,
                SaveSelection {
                    inner: value.selection,
                },
            )?,
            legacy_choice: value.legacy_choice,
            warning_choice: value.warning_choice,
            report: value
                .report
                .map(|v| Py::new(py, SaveReport::from_core(py, v)?))
                .transpose()?,
            publication_error: value
                .publication_error
                .map(|error| PyErr::from(error).into_value(py)),
        })
    }
    pub fn to_core(&self, py: Python<'_>) -> PyResult<lifecycle::SaveAttempt> {
        Ok(lifecycle::SaveAttempt {
            selection: self.selection.try_borrow(py)?.inner.clone(),
            legacy_choice: self.legacy_choice,
            warning_choice: self.warning_choice,
            report: self
                .report
                .as_ref()
                .map(|r| r.try_borrow(py)?.to_core(py))
                .transpose()?,
            publication_error: self
                .publication_error
                .as_ref()
                .map(|e| std::io::Error::from(PyErr::from_value(e.bind(py).clone().into_any()))),
        })
    }
}
#[pymethods]
impl SaveAttempt {
    #[new]
    #[pyo3(signature = (selection, legacy_choice, warning_choice, report, publication_error))]
    fn new(
        selection: Py<SaveSelection>,
        legacy_choice: &Bound<'_, PyAny>,
        warning_choice: Option<&Bound<'_, PyAny>>,
        report: Option<Py<SaveReport>>,
        publication_error: Option<Py<pyo3::exceptions::PyBaseException>>,
    ) -> PyResult<Self> {
        Ok(Self {
            selection,
            legacy_choice: strict_i32(legacy_choice)?,
            warning_choice: warning_choice.map(strict_i32).transpose()?,
            report,
            publication_error,
        })
    }
    #[getter]
    fn legacy_choice(&self) -> i32 {
        self.legacy_choice
    }
    #[setter]
    fn set_legacy_choice(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.legacy_choice = strict_i32(v)?;
        Ok(())
    }
    #[getter]
    fn warning_choice(&self) -> Option<i32> {
        self.warning_choice
    }
    #[setter]
    fn set_warning_choice(&mut self, v: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.warning_choice = v.map(strict_i32).transpose()?;
        Ok(())
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.selection)?;
        visit.call(&self.report)?;
        visit.call(&self.publication_error)
    }
}
#[pyclass(module = "cs705.icf.document.lifecycle")]
pub struct SaveFlow {
    outcome: core::SaveOutcome,
    #[pyo3(get, set)]
    pub attempts: Py<PyList>,
    dialog_cancelled: bool,
}
impl SaveFlow {
    pub fn from_core(py: Python<'_>, value: lifecycle::SaveFlow) -> PyResult<Self> {
        let attempts = value
            .attempts
            .into_iter()
            .map(|a| Py::new(py, SaveAttempt::from_core(py, a)?))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            outcome: value.outcome,
            attempts: PyList::new(py, attempts)?.unbind(),
            dialog_cancelled: value.dialog_cancelled,
        })
    }
    pub fn to_core(&self, py: Python<'_>) -> PyResult<lifecycle::SaveFlow> {
        let attempts = self
            .attempts
            .bind(py)
            .iter()
            .map(|v| v.extract::<PyRef<'_, SaveAttempt>>()?.to_core(py))
            .collect::<PyResult<_>>()?;
        Ok(lifecycle::SaveFlow {
            outcome: self.outcome,
            attempts,
            dialog_cancelled: self.dialog_cancelled,
        })
    }
}
#[pymethods]
impl SaveFlow {
    #[new]
    fn new(
        py: Python<'_>,
        outcome: &Bound<'_, PyAny>,
        attempts: Vec<Py<SaveAttempt>>,
        dialog_cancelled: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            outcome: if values::enum_in(outcome, "cs705.icf.document", "SaveOutcome")? == 0 {
                core::SaveOutcome::Saved
            } else {
                core::SaveOutcome::NotSaved
            },
            attempts: PyList::new(py, attempts)?.unbind(),
            dialog_cancelled: strict_bool(dialog_cancelled)?,
        })
    }
    #[getter]
    fn outcome<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        values::enum_out(
            py,
            "cs705.icf.document",
            "SaveOutcome",
            if self.outcome == core::SaveOutcome::Saved {
                0
            } else {
                1
            },
        )
    }
    #[setter]
    fn set_outcome(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.outcome = if values::enum_in(v, "cs705.icf.document", "SaveOutcome")? == 0 {
            core::SaveOutcome::Saved
        } else {
            core::SaveOutcome::NotSaved
        };
        Ok(())
    }
    #[getter]
    fn dialog_cancelled(&self) -> bool {
        self.dialog_cancelled
    }
    #[setter]
    fn set_dialog_cancelled(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.dialog_cancelled = strict_bool(v)?;
        Ok(())
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.attempts)
    }
}
#[pyclass(module = "cs705.icf.document.lifecycle")]
pub struct DirtyPrompt {
    choice: Option<i32>,
    result: i32,
    #[pyo3(get, set)]
    pub save: Option<Py<SaveFlow>>,
}
impl DirtyPrompt {
    pub fn from_core(py: Python<'_>, value: lifecycle::DirtyPrompt) -> PyResult<Self> {
        Ok(Self {
            choice: value.choice,
            result: value.result,
            save: value
                .save
                .map(|v| Py::new(py, SaveFlow::from_core(py, v)?))
                .transpose()?,
        })
    }
    pub fn to_core(&self, py: Python<'_>) -> PyResult<lifecycle::DirtyPrompt> {
        Ok(lifecycle::DirtyPrompt {
            choice: self.choice,
            result: self.result,
            save: self
                .save
                .as_ref()
                .map(|s| s.try_borrow(py)?.to_core(py))
                .transpose()?,
        })
    }
}
#[pymethods]
impl DirtyPrompt {
    #[new]
    #[pyo3(signature = (choice, result, save))]
    fn new(
        choice: Option<&Bound<'_, PyAny>>,
        result: &Bound<'_, PyAny>,
        save: Option<Py<SaveFlow>>,
    ) -> PyResult<Self> {
        Ok(Self {
            choice: choice.map(strict_i32).transpose()?,
            result: strict_i32(result)?,
            save,
        })
    }
    #[getter]
    fn choice(&self) -> Option<i32> {
        self.choice
    }
    #[setter]
    fn set_choice(&mut self, v: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.choice = v.map(strict_i32).transpose()?;
        Ok(())
    }
    #[getter]
    fn result(&self) -> i32 {
        self.result
    }
    #[setter]
    fn set_result(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.result = strict_i32(v)?;
        Ok(())
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.save)
    }
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    root.add_class::<ResetEffects>()?;
    root.add_class::<SaveReport>()
}
pub fn register_lifecycle(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SaveAttempt>()?;
    m.add_class::<SaveFlow>()?;
    m.add_class::<DirtyPrompt>()
}
