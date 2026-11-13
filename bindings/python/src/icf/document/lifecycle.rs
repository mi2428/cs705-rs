//! Live MRU owner and explicit native protocol values.
use super::Target;
use super::adapters::{Protocol, file_error};
use super::values::{add_enum, scalar_value};
use crate::{callbacks::CallbackError, conversion::*, errors};
use cs705_core::icf::document::lifecycle as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3::{PyTraverseError, PyVisit};
use std::cell::RefCell;

#[pyclass(module = "cs705.icf.document.lifecycle")]
pub struct SaveOptions {
    #[pyo3(get, set)]
    pub validation: Py<super::values::ValidationSource>,
    #[pyo3(get, set)]
    pub reset: Option<Py<crate::icf::normalizer::ResetState>>,
}
impl SaveOptions {
    fn snapshot(py: Python<'_>, options: &Py<Self>) -> PyResult<Self> {
        let options = options.try_borrow(py)?;
        Ok(Self {
            validation: options.validation.clone_ref(py),
            reset: options.reset.as_ref().map(|r| r.clone_ref(py)),
        })
    }
}
#[pymethods]
impl SaveOptions {
    #[new]
    #[pyo3(signature = (validation, reset=None))]
    fn new(
        validation: Py<super::values::ValidationSource>,
        reset: Option<Py<crate::icf::normalizer::ResetState>>,
    ) -> Self {
        Self { validation, reset }
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.validation)?;
        visit.call(&self.reset)
    }
}

#[pyclass(frozen, module = "cs705.icf.document.lifecycle")]
#[derive(Clone)]
pub struct RecentFiles {
    pub target: Target<core::RecentFiles>,
}

#[pyclass(module = "cs705.icf.document.lifecycle")]
pub struct Lifecycle {
    recent: Option<Py<RecentFiles>>,
    ui: Option<Py<super::ui::UiState>>,
}
impl Lifecycle {
    fn parts(slf: &Bound<'_, Self>) -> PyResult<(Py<RecentFiles>, Py<super::ui::UiState>)> {
        let value = slf.try_borrow()?;
        let recent = value
            .recent
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("lifecycle was cleared"))?
            .clone_ref(slf.py());
        let ui = value
            .ui
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("lifecycle was cleared"))?
            .clone_ref(slf.py());
        Ok((recent, ui))
    }
    fn run<R: Send>(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        document: &super::Document,
        session: &super::Session,
        call: impl FnOnce(
            &mut core::Lifecycle<&mut core::RecentFiles, &mut core::UiState>,
            &mut cs705_core::icf::document::Document,
            &mut cs705_core::icf::document::Session,
        ) -> R
        + Send,
    ) -> PyResult<R> {
        let (recent, ui) = Self::parts(slf)?;
        recent.get().with_mut(|recent| {
            ui.get().with_mut(|ui| {
                document.with_mut(|document| {
                    session.with_mut(|session| {
                        Ok(py.detach(move || {
                            call(&mut core::Lifecycle { recent, ui }, document, session)
                        }))
                    })
                })
            })
        })
    }
    fn finish_prompt(
        py: Python<'_>,
        failures: CallbackError,
        result: Result<Option<core::DirtyPrompt>, cs705_core::icf::document::FileError>,
    ) -> PyResult<Option<Py<super::reports::DirtyPrompt>>> {
        match result.map_err(file_error) {
            Ok(Some(prompt)) => super::reports::finish_report(
                py,
                failures,
                super::reports::DirtyPrompt::from_core(py, prompt).and_then(|r| Py::new(py, r)),
            )
            .map(Some),
            Ok(None) => failures.finish(py, Ok(None)),
            Err(error) => failures.finish(py, Err(error)),
        }
    }
}
#[pymethods]
impl Lifecycle {
    #[new]
    fn new(recent: Py<RecentFiles>, ui: Py<super::ui::UiState>) -> Self {
        Self {
            recent: Some(recent),
            ui: Some(ui),
        }
    }
    #[getter]
    fn recent(&self, py: Python<'_>) -> PyResult<Py<RecentFiles>> {
        self.recent
            .as_ref()
            .map(|r| r.clone_ref(py))
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("lifecycle was cleared"))
    }
    #[setter]
    fn set_recent(&mut self, recent: Py<RecentFiles>) {
        self.recent = Some(recent);
    }
    #[getter]
    fn ui(&self, py: Python<'_>) -> PyResult<Py<super::ui::UiState>> {
        self.ui
            .as_ref()
            .map(|r| r.clone_ref(py))
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("lifecycle was cleared"))
    }
    #[setter]
    fn set_ui(&mut self, ui: Py<super::ui::UiState>) {
        self.ui = Some(ui);
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Native Save keeps document/session owners and dialog, file, comparison sources explicit."
    )]
    fn save(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        document: &super::Document,
        session: &super::Session,
        mode: &Bound<'_, PyAny>,
        options: Py<SaveOptions>,
        dialogs: Py<PyAny>,
        files: Py<PyAny>,
        compare: Py<PyAny>,
    ) -> PyResult<Py<super::reports::SaveFlow>> {
        let interpreter = crate::scoped::interpreter(py)?;
        let mode = if super::values::enum_in(mode, "cs705.icf.document.lifecycle", "SaveMode")? == 0
        {
            core::SaveMode::Save
        } else {
            core::SaveMode::SaveAs
        };
        let options = SaveOptions::snapshot(py, &options)?;
        let views = options.validation.get().values(py)?;
        let mut reset = options.reset.as_ref().map(|r| r.get().take()).transpose()?;
        let (failures, result) = Self::run(
            slf,
            py,
            document,
            session,
            move |life, document, session| {
                let failures = RefCell::new(CallbackError::default());
                let result = life.save(
                    document,
                    session,
                    mode,
                    &mut core::SaveOptions {
                        validation: super::values::validation(&views),
                        reset: reset.as_deref_mut(),
                    },
                    &mut Protocol {
                        object: dialogs,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: files,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: compare,
                        errors: &failures,
                        interpreter,
                    },
                );
                (failures.into_inner(), result)
            },
        )?;
        super::reports::finish_report(
            py,
            failures,
            result
                .map_err(file_error)
                .and_then(|r| Py::new(py, super::reports::SaveFlow::from_core(py, r)?)),
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "The dirty gate forwards separate native save sources without an implicit publisher."
    )]
    fn dirty_prompt(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        document: &super::Document,
        session: &super::Session,
        options: Py<SaveOptions>,
        dialogs: Py<PyAny>,
        files: Py<PyAny>,
        compare: Py<PyAny>,
    ) -> PyResult<Py<super::reports::DirtyPrompt>> {
        let interpreter = crate::scoped::interpreter(py)?;
        let options = SaveOptions::snapshot(py, &options)?;
        let views = options.validation.get().values(py)?;
        let mut reset = options.reset.as_ref().map(|r| r.get().take()).transpose()?;
        let (failures, result) = Self::run(
            slf,
            py,
            document,
            session,
            move |life, document, session| {
                let failures = RefCell::new(CallbackError::default());
                let result = life.dirty_prompt(
                    document,
                    session,
                    &mut core::SaveOptions {
                        validation: super::values::validation(&views),
                        reset: reset.as_deref_mut(),
                    },
                    &mut Protocol {
                        object: dialogs,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: files,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: compare,
                        errors: &failures,
                        interpreter,
                    },
                );
                (failures.into_inner(), result)
            },
        )?;
        super::reports::finish_report(
            py,
            failures,
            result
                .map_err(file_error)
                .and_then(|r| Py::new(py, super::reports::DirtyPrompt::from_core(py, r)?)),
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "New forwards the independent pending-save sources before replacing current."
    )]
    fn new_file(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        document: &super::Document,
        session: &super::Session,
        options: Py<SaveOptions>,
        dialogs: Py<PyAny>,
        files: Py<PyAny>,
        compare: Py<PyAny>,
    ) -> PyResult<Py<super::reports::DirtyPrompt>> {
        let interpreter = crate::scoped::interpreter(py)?;
        let options = SaveOptions::snapshot(py, &options)?;
        let views = options.validation.get().values(py)?;
        let mut reset = options.reset.as_ref().map(|r| r.get().take()).transpose()?;
        let (failures, result) = Self::run(
            slf,
            py,
            document,
            session,
            move |life, document, session| {
                let failures = RefCell::new(CallbackError::default());
                let result = life.new_file(
                    document,
                    session,
                    &mut core::SaveOptions {
                        validation: super::values::validation(&views),
                        reset: reset.as_deref_mut(),
                    },
                    &mut Protocol {
                        object: dialogs,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: files,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: compare,
                        errors: &failures,
                        interpreter,
                    },
                );
                (failures.into_inner(), result)
            },
        )?;
        super::reports::finish_report(
            py,
            failures,
            result
                .map_err(file_error)
                .and_then(|r| Py::new(py, super::reports::DirtyPrompt::from_core(py, r)?)),
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Close preserves the mutable gate and independently fallible save and placement sources."
    )]
    fn close_query(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        document: &super::Document,
        session: &super::Session,
        incoming: &crate::conversion::Bool,
        options: Py<SaveOptions>,
        dialogs: Py<PyAny>,
        files: Py<PyAny>,
        compare: Py<PyAny>,
        placement: Py<PyAny>,
    ) -> PyResult<Option<Py<super::reports::DirtyPrompt>>> {
        let interpreter = crate::scoped::interpreter(py)?;
        let options = SaveOptions::snapshot(py, &options)?;
        let views = options.validation.get().values(py)?;
        let mut reset = options.reset.as_ref().map(|r| r.get().take()).transpose()?;
        let mut incoming = incoming.take()?;
        let (failures, result) = Self::run(
            slf,
            py,
            document,
            session,
            move |life, document, session| {
                let failures = RefCell::new(CallbackError::default());
                let result = life.close_query(
                    document,
                    session,
                    &mut incoming,
                    &mut core::SaveOptions {
                        validation: super::values::validation(&views),
                        reset: reset.as_deref_mut(),
                    },
                    &mut Protocol {
                        object: dialogs,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: files,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: compare,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: placement,
                        errors: &failures,
                        interpreter,
                    },
                );
                (failures.into_inner(), result)
            },
        )?;
        Self::finish_prompt(py, failures, result)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Close forwards the native gate, publication protocols, and placement producer unchanged."
    )]
    fn close(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        document: &super::Document,
        session: &super::Session,
        incoming: &crate::conversion::Bool,
        options: Py<SaveOptions>,
        dialogs: Py<PyAny>,
        files: Py<PyAny>,
        compare: Py<PyAny>,
        placement: Py<PyAny>,
    ) -> PyResult<Option<Py<super::reports::DirtyPrompt>>> {
        let interpreter = crate::scoped::interpreter(py)?;
        let options = SaveOptions::snapshot(py, &options)?;
        let views = options.validation.get().values(py)?;
        let mut reset = options.reset.as_ref().map(|r| r.get().take()).transpose()?;
        let mut incoming = incoming.take()?;
        let (failures, result) = Self::run(
            slf,
            py,
            document,
            session,
            move |life, document, session| {
                let failures = RefCell::new(CallbackError::default());
                let result = life.close(
                    document,
                    session,
                    &mut incoming,
                    &mut core::SaveOptions {
                        validation: super::values::validation(&views),
                        reset: reset.as_deref_mut(),
                    },
                    &mut Protocol {
                        object: dialogs,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: files,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: compare,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: placement,
                        errors: &failures,
                        interpreter,
                    },
                );
                (failures.into_inner(), result)
            },
        )?;
        Self::finish_prompt(py, failures, result)
    }
    #[pyo3(signature = (document, session, path, profile_choice, defaults, options, dialogs, files, compare))]
    #[expect(
        clippy::too_many_arguments,
        reason = "Open keeps path, profile decision, default assets, and pending-save protocols independent."
    )]
    fn open(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        document: &super::Document,
        session: &super::Session,
        path: Option<&Bound<'_, PyAny>>,
        profile_choice: Option<&Bound<'_, PyAny>>,
        defaults: Option<Py<super::values::DefaultSource>>,
        options: Py<SaveOptions>,
        dialogs: Py<PyAny>,
        files: Py<PyAny>,
        compare: Py<PyAny>,
    ) -> PyResult<Py<pyo3::types::PyTuple>> {
        let interpreter = crate::scoped::interpreter(py)?;
        let path = path.map(strict_bytes).transpose()?.map(<[u8]>::to_vec);
        let profile_choice = profile_choice.map(strict_i32).transpose()?;
        let defaults = defaults
            .map(|s| Ok::<_, PyErr>(s.try_borrow(py)?.clone()))
            .transpose()?;
        let options = SaveOptions::snapshot(py, &options)?;
        let views = options.validation.get().values(py)?;
        let mut reset = options.reset.as_ref().map(|r| r.get().take()).transpose()?;
        let (failures, result) = Self::run(
            slf,
            py,
            document,
            session,
            move |life, document, session| {
                let failures = RefCell::new(CallbackError::default());
                let result = life.open(
                    document,
                    session,
                    path,
                    profile_choice,
                    defaults.as_ref().map(super::values::DefaultSource::as_core),
                    &mut core::SaveOptions {
                        validation: super::values::validation(&views),
                        reset: reset.as_deref_mut(),
                    },
                    &mut Protocol {
                        object: dialogs,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: files,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: compare,
                        errors: &failures,
                        interpreter,
                    },
                );
                (failures.into_inner(), result)
            },
        )?;
        let result = result.map_err(file_error).and_then(|(prompt, status)| {
            let prompt = Py::new(py, super::reports::DirtyPrompt::from_core(py, prompt)?)?;
            let status = Py::new(py, super::values::OpenStatus { inner: status })?;
            pyo3::types::PyTuple::new(py, [prompt.into_any(), status.into_any()]).map(Bound::unbind)
        });
        super::reports::finish_report(py, failures, result)
    }
    #[pyo3(signature = (document, session, index, profile_choice, defaults, options, dialogs, files, compare))]
    #[expect(
        clippy::too_many_arguments,
        reason = "MRU Open forwards the same native profile, asset, and pending-save sources."
    )]
    fn open_recent(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        document: &super::Document,
        session: &super::Session,
        index: &Bound<'_, PyAny>,
        profile_choice: Option<&Bound<'_, PyAny>>,
        defaults: Option<Py<super::values::DefaultSource>>,
        options: Py<SaveOptions>,
        dialogs: Py<PyAny>,
        files: Py<PyAny>,
        compare: Py<PyAny>,
    ) -> PyResult<Py<pyo3::types::PyTuple>> {
        let interpreter = crate::scoped::interpreter(py)?;
        let index = strict_usize(index)?;
        let profile_choice = profile_choice.map(strict_i32).transpose()?;
        let defaults = defaults
            .map(|s| Ok::<_, PyErr>(s.try_borrow(py)?.clone()))
            .transpose()?;
        let options = SaveOptions::snapshot(py, &options)?;
        let views = options.validation.get().values(py)?;
        let mut reset = options.reset.as_ref().map(|r| r.get().take()).transpose()?;
        let (failures, result) = Self::run(
            slf,
            py,
            document,
            session,
            move |life, document, session| {
                let failures = RefCell::new(CallbackError::default());
                let result = life.open_recent(
                    document,
                    session,
                    index,
                    profile_choice,
                    defaults.as_ref().map(super::values::DefaultSource::as_core),
                    &mut core::SaveOptions {
                        validation: super::values::validation(&views),
                        reset: reset.as_deref_mut(),
                    },
                    &mut Protocol {
                        object: dialogs,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: files,
                        errors: &failures,
                        interpreter,
                    },
                    &mut Protocol {
                        object: compare,
                        errors: &failures,
                        interpreter,
                    },
                );
                (failures.into_inner(), result)
            },
        )?;
        let result = result.map_err(file_error).and_then(|(prompt, status)| {
            let prompt = Py::new(py, super::reports::DirtyPrompt::from_core(py, prompt)?)?;
            let status = Py::new(py, super::values::OpenStatus { inner: status })?;
            pyo3::types::PyTuple::new(py, [prompt.into_any(), status.into_any()]).map(Bound::unbind)
        });
        super::reports::finish_report(py, failures, result)
    }
    fn sync02(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        document: &super::Document,
        session: &super::Session,
    ) -> PyResult<()> {
        let (recent, ui) = Self::parts(slf)?;
        ui.get().sync02(py, document, session, recent.get())
    }
    fn initial_boundary(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        boundary: &super::initial::InitialBoundary,
        document: &super::Document,
        session: &super::Session,
    ) -> PyResult<()> {
        let boundary = boundary.inner;
        if boundary == cs705_core::icf::document::InitialBoundary::Sync(2) {
            return Self::sync02(slf, py, document, session);
        }
        let (_, ui) = Self::parts(slf)?;
        ui.get()
            .with_mut(|ui| {
                document.with_mut(|document| {
                    session.with_mut(|session| {
                        Ok(py.detach(|| ui.initial_boundary(boundary, document, session)))
                    })
                })
            })?
            .map_err(errors::icf)
    }
    #[pyo3(signature = (document, session, prompt, file, profile_choice, defaults, compare))]
    #[expect(
        clippy::too_many_arguments,
        reason = "The completed prompt and acquired file/default assets are distinct native inputs."
    )]
    fn open_after_prompt(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        document: &super::Document,
        session: &super::Session,
        prompt: Py<super::reports::DirtyPrompt>,
        file: Option<Py<super::lifecycle_values::OpenFile>>,
        profile_choice: Option<&Bound<'_, PyAny>>,
        defaults: Option<Py<super::values::DefaultSource>>,
        compare: Py<PyAny>,
    ) -> PyResult<(super::reports::DirtyPrompt, super::values::OpenStatus)> {
        let interpreter = crate::scoped::interpreter(py)?;
        let prompt = prompt.try_borrow(py)?.to_core(py)?;
        let file = file
            .map(|f| {
                let f = f.try_borrow(py)?;
                Ok::<_, PyErr>(core::OpenFile {
                    path: f.inner.path.clone(),
                    bytes: f.inner.bytes.clone(),
                })
            })
            .transpose()?;
        let profile_choice = profile_choice.map(strict_i32).transpose()?;
        let defaults = defaults
            .map(|s| Ok::<_, PyErr>(s.try_borrow(py)?.clone()))
            .transpose()?;
        let (failures, result) = Self::run(
            slf,
            py,
            document,
            session,
            move |life, document, session| {
                let failures = RefCell::new(CallbackError::default());
                let result = life.open_after_prompt(
                    document,
                    session,
                    prompt,
                    file,
                    profile_choice,
                    defaults.as_ref().map(super::values::DefaultSource::as_core),
                    &mut Protocol {
                        object: compare,
                        errors: &failures,
                        interpreter,
                    },
                );
                (failures.into_inner(), result)
            },
        )?;
        let result = failures.finish(py, result.map_err(file_error))?;
        Ok((
            super::reports::DirtyPrompt::from_core(py, result.0)?,
            super::values::OpenStatus { inner: result.1 },
        ))
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.ui)?;
        visit.call(&self.recent)
    }
    fn __clear__(&mut self) {
        self.ui = None;
        self.recent = None;
    }
}
impl RecentFiles {
    pub fn from_core(value: core::RecentFiles) -> Self {
        Self {
            target: Target::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::RecentFiles) -> PyResult<R>) -> PyResult<R> {
        self.target.with_ref(f)
    }
    pub fn with_mut<R>(
        &self,
        f: impl FnOnce(&mut core::RecentFiles) -> PyResult<R>,
    ) -> PyResult<R> {
        self.target.with_mut(f)
    }
    pub fn from_loan(loan: crate::scoped::Loan<core::RecentFiles>) -> Self {
        Self {
            target: Target::Scoped(loan),
        }
    }
}
#[pymethods]
impl RecentFiles {
    #[new]
    fn new() -> Self {
        Self::from_core(core::RecentFiles::default())
    }
    fn share(&self) -> Self {
        self.clone()
    }
    fn paths<'py>(&self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyBytes>>> {
        self.with_ref(|r| Ok(r.paths().iter().map(|p| PyBytes::new(py, p)).collect()))
    }
    fn visible<'py>(&self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyBytes>>> {
        self.with_ref(|r| Ok(r.visible().iter().map(|p| PyBytes::new(py, p)).collect()))
    }
    fn selected<'py>(
        &self,
        py: Python<'py>,
        index: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let index = strict_usize(index)?;
        self.with_ref(|r| Ok(r.selected(index).map(|p| PyBytes::new(py, p))))?
            .map_err(errors::icf)
    }
    fn insert(&self, py: Python<'_>, path: &Bound<'_, PyAny>, compare: Py<PyAny>) -> PyResult<()> {
        let interpreter = crate::scoped::interpreter(py)?;
        let path = strict_bytes(path)?.to_vec();
        let (errors, result) = self.with_mut(|recent| {
            Ok(py.detach(move || {
                let errors = RefCell::new(CallbackError::default());
                let result = recent.insert(
                    path,
                    &mut Protocol {
                        object: compare,
                        errors: &errors,
                        interpreter,
                    },
                );
                (errors.into_inner(), result)
            }))
        })?;
        errors.finish(py, result.map_err(errors::icf))
    }
    #[staticmethod]
    fn load(py: Python<'_>, store: Py<PyAny>, compare: Py<PyAny>) -> PyResult<Self> {
        let interpreter = crate::scoped::interpreter(py)?;
        let (errors, result) = py.detach(move || {
            let errors = RefCell::new(CallbackError::default());
            let result = core::RecentFiles::load(
                &mut Protocol {
                    object: store,
                    errors: &errors,
                    interpreter,
                },
                &mut Protocol {
                    object: compare,
                    errors: &errors,
                    interpreter,
                },
            );
            (errors.into_inner(), result)
        });
        errors.finish(py, result.map(Self::from_core).map_err(file_error))
    }
    fn persist(&self, py: Python<'_>, store: Py<PyAny>) -> PyResult<()> {
        let interpreter = crate::scoped::interpreter(py)?;
        let (errors, result) = self.with_ref(|recent| {
            Ok(py.detach(move || {
                let errors = RefCell::new(CallbackError::default());
                let result = recent.persist(&mut Protocol {
                    object: store,
                    errors: &errors,
                    interpreter,
                });
                (errors.into_inner(), result)
            }))
        })?;
        errors.finish(py, result.map_err(PyErr::from))
    }
}

use core::FormState as CoreFormState;
scalar_value!(FormState, CoreFormState, "cs705.icf.document.lifecycle", {
    visible57, set_visible57: u8 => strict_u8,
    enabled58, set_enabled58: u8 => strict_u8,
    focused, set_focused: bool => strict_bool,
});
#[pyfunction]
fn filter_target(index: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(core::filter_target(strict_u32(index)?))
}
#[pyfunction]
fn target_filter(target: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(core::target_filter(strict_u32(target)?))
}

pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "lifecycle")?;
    super::lifecycle_values::register(&m)?;
    super::menu::register(&m)?;
    super::ui::register(&m)?;
    super::ui_values::register(&m)?;
    super::reports::register_lifecycle(&m)?;
    m.add_class::<RecentFiles>()?;
    m.add_class::<Lifecycle>()?;
    m.add_class::<SaveOptions>()?;
    m.add_class::<FormState>()?;
    m.add("RECENT_LIMIT", core::RECENT_LIMIT)?;
    m.add("RECENT_VISIBLE", core::RECENT_VISIBLE)?;
    add_enum(
        &m,
        "cs705.icf.document.lifecycle",
        "SaveMode",
        &["Save", "SaveAs"],
    )?;
    m.add_function(wrap_pyfunction!(filter_target, &m)?)?;
    m.add_function(wrap_pyfunction!(target_filter, &m)?)?;
    root.add_submodule(&m)
}
