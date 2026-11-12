//! Shared, move-out owners for document and global session state.
pub mod adapters;
pub mod clone_write;
pub mod initial;
pub mod lifecycle;
pub mod lifecycle_values;
pub mod menu;
pub mod native_read;
pub mod options;
pub mod preparation;
pub mod reports;
pub mod session_views;
pub mod ui;
pub mod ui_values;
pub mod values;
use crate::{conversion::*, errors, owner::Owner};
use cs705_core::icf::document as core;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};
use std::sync::Arc;

/// Scoped closures perform native work only and must not invoke Python again.
pub enum Target<T: 'static> {
    Owned(Arc<Owner<T>>),
    Scoped(crate::scoped::Loan<T>),
}
impl<T: 'static> Clone for Target<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Owned(owner) => Self::Owned(owner.clone()),
            Self::Scoped(loan) => Self::Scoped(loan.clone()),
        }
    }
}
impl<T: 'static> Target<T> {
    pub fn new(value: T) -> Self {
        Self::Owned(Arc::new(Owner::new(value)))
    }
    pub fn take(&self) -> PyResult<crate::owner::Taken<'_, T>> {
        match self {
            Self::Owned(owner) => owner.take(),
            Self::Scoped(_) => Err(pyo3::exceptions::PyRuntimeError::new_err(
                "this operation requires an ordinary owner, not a callback view",
            )),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&T) -> PyResult<R>) -> PyResult<R> {
        match self {
            Self::Owned(owner) => owner.with_ref(f),
            Self::Scoped(loan) => Python::attach(|py| loan.with_ref(py, f)),
        }
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> PyResult<R>) -> PyResult<R> {
        match self {
            Self::Owned(owner) => owner.with_mut(f),
            Self::Scoped(loan) => Python::attach(|py| loan.with_mut(py, f)),
        }
    }
}

#[pyclass(frozen, module = "cs705.icf.document")]
#[derive(Clone)]
pub struct Document {
    target: Target<core::Document>,
}
impl Document {
    pub fn take(&self) -> PyResult<crate::owner::Taken<'_, core::Document>> {
        self.target.take()
    }
    pub fn from_core(value: core::Document) -> Self {
        Self {
            target: Target::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::Document) -> PyResult<R>) -> PyResult<R> {
        self.target.with_ref(f)
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut core::Document) -> PyResult<R>) -> PyResult<R> {
        self.target.with_mut(f)
    }
    pub fn from_loan(loan: crate::scoped::Loan<core::Document>) -> Self {
        Self {
            target: Target::Scoped(loan),
        }
    }
}

#[pymethods]
impl Document {
    #[staticmethod]
    fn from_current(
        current: &Bound<'_, PyAny>,
        path: &Bound<'_, PyAny>,
        target_revision: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self::from_core(core::Document::from_current(
            strict_bytes(current)?.to_vec(),
            strict_bytes(path)?.to_vec(),
            strict_u32(target_revision)?,
        )))
    }
    fn share(&self) -> Self {
        self.clone()
    }
    fn finish_action(
        &self,
        py: Python<'_>,
        context: &values::ActionContext,
        notify: &Bound<'_, PyAny>,
        flags: &Bound<'_, PyAny>,
        callback: Py<PyAny>,
    ) -> PyResult<values::ActionEffects> {
        let interpreter = crate::scoped::interpreter(py)?;
        let notify = strict_bool(notify)?;
        let flags = strict_u32(flags)?;
        let mut document = self.target.take()?;
        let mut context = context.target.take()?;
        let (failures, result) = py.detach(move || {
            let mut failures = crate::callbacks::CallbackError::default();
            let result =
                document.finish_action(&mut context, notify, flags, |document, context, flags| {
                    let result = adapters::attach_checked(interpreter, |py| {
                        crate::scoped::Frame::with_mut(py, document, |document| {
                            crate::scoped::Frame::with_mut(py, context, |context| {
                                let document = Py::new(py, Self::from_loan(document))?;
                                let context =
                                    Py::new(py, values::ActionContext::from_loan(context))?;
                                adapters::require_none(
                                    callback.bind(py).call1((document, context, flags))?,
                                )
                            })
                        })
                    });
                    failures.capture(result).map_err(|_| {
                        cs705_core::icf::Error::Operation("Python document action failed".into())
                    })
                });
            (failures, result)
        });
        failures.finish(
            py,
            result
                .map(|inner| values::ActionEffects { inner })
                .map_err(errors::icf),
        )
    }
    fn current<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(|d| Ok(PyBytes::new(py, d.current())))
    }
    fn baseline<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(|d| Ok(PyBytes::new(py, d.baseline())))
    }
    fn path<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(|d| Ok(PyBytes::new(py, d.path())))
    }
    fn directory<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(|d| Ok(PyBytes::new(py, d.directory())))
    }
    fn comment_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(|d| Ok(PyBytes::new(py, d.comment_bytes())))
    }
    fn extra_data(&self) -> PyResult<u32> {
        self.with_ref(|d| Ok(d.extra_data()))
    }
    fn current_icf(&self) -> PyResult<crate::icf::Icf> {
        self.with_ref(|d| {
            Ok(d.current_icf()
                .map(|v| crate::icf::Icf::from_core(v.into_owned())))
        })?
        .map_err(errors::icf)
    }
    fn edit_setting(
        &self,
        name: &str,
        index: &Bound<'_, PyAny>,
        value: &crate::icf::settings::Value,
    ) -> PyResult<()> {
        let index = strict_usize(index)?;
        let value = value.inner.clone();
        self.with_mut(|d| Ok(d.edit_setting(name, index, &value)))?
            .map_err(errors::icf)
    }
    fn normalize(&self, py: Python<'_>, session: &Session) -> PyResult<crate::icf::stage9::Report> {
        let mut document = self.target.take()?;
        let session = session.target.take()?;
        let result = py
            .detach(move || document.normalize(&session))
            .map_err(errors::icf)?;
        crate::icf::stage9::Report::from_core(py, result)
    }
    fn reset_operating_state(
        &self,
        py: Python<'_>,
        session: &Session,
        state: Py<crate::icf::normalizer::ResetState>,
    ) -> PyResult<reports::ResetEffects> {
        let mut state = state.get().take()?;
        let mut document = self.take()?;
        let session = session.take()?;
        let result = py.detach(move || document.reset_operating_state(&session, &mut state));
        reports::ResetEffects::from_core(py, result.map_err(errors::icf)?)
    }
    fn import_csv(
        &self,
        py: Python<'_>,
        session: &Session,
        gate: &crate::csv::control::ImportGate,
        reader: Py<PyAny>,
    ) -> PyResult<values::ImportResult> {
        let interpreter = crate::scoped::interpreter(py)?;
        use crate::csv::control::ImportGate;
        let gate = match gate {
            ImportGate::FileCancelled {} => cs705_core::csv::control::ImportGate::FileCancelled,
            ImportGate::InnerCancelled {} => cs705_core::csv::control::ImportGate::InnerCancelled,
            ImportGate::Header { status } => cs705_core::csv::control::ImportGate::Header(status.0),
        };
        let mut document = self.take()?;
        let session = session.take()?;
        let (failures, result) = py.detach(move || {
            let mut failures = crate::callbacks::CallbackError::default();
            let result = document.import_csv(&session, gate, |document| {
                let result = adapters::attach_checked(interpreter, |py| {
                    crate::scoped::Frame::with_mut(py, document, |document| {
                        let document = Py::new(py, Self::from_loan(document))?;
                        Ok(reader.bind(py).call1((document,))?.unbind())
                    })
                });
                failures.capture(result).map_err(|_| {
                    cs705_core::icf::Error::Operation("Python document CSV reader failed".into())
                })
            });
            (failures, result)
        });
        let (reader_result, report) = failures.finish(py, result.map_err(errors::icf))?;
        Ok(values::ImportResult {
            reader_called: reader_result.is_some(),
            reader_result,
            stage9: report
                .map(|r| Py::new(py, crate::icf::stage9::Report::from_core(py, r)?))
                .transpose()?,
        })
    }
    #[pyo3(signature = (bytes, path, session, source, boundary))]
    fn initial_from_icf(
        &self,
        py: Python<'_>,
        bytes: &Bound<'_, PyAny>,
        path: &Bound<'_, PyAny>,
        session: &Session,
        source: Option<Py<values::DefaultSource>>,
        boundary: Py<PyAny>,
    ) -> PyResult<crate::icf::format::Status> {
        let interpreter = crate::scoped::interpreter(py)?;
        let bytes = strict_bytes(bytes)?.to_vec();
        let path = strict_bytes(path)?.to_vec();
        let source = source
            .map(|s| Ok::<_, PyErr>(s.try_borrow(py)?.clone()))
            .transpose()?;
        let mut document = self.target.take()?;
        let mut session = session.target.take()?;
        let (failures, result) = py.detach(move || {
            let mut failures = crate::callbacks::CallbackError::default();
            let result = document.initial_from_icf(
                &bytes,
                &path,
                &mut session,
                source.as_ref().map(values::DefaultSource::as_core),
                |event, document, session| {
                    initial::invoke_boundary(
                        &boundary,
                        &mut failures,
                        event,
                        document,
                        session,
                        interpreter,
                    )
                },
            );
            (failures, result)
        });
        failures.finish(
            py,
            result
                .map(|inner| crate::icf::format::Status { inner })
                .map_err(errors::icf),
        )
    }
    #[pyo3(signature = (session, state, identity, request, source, boundary))]
    fn receive_initial_identity(
        &self,
        py: Python<'_>,
        session: &Session,
        state: &initial::InitialState,
        identity: &crate::clone::Identity,
        request: &initial::InitialRequest,
        source: Option<Py<values::DefaultSource>>,
        boundary: Py<PyAny>,
    ) -> PyResult<()> {
        let interpreter = crate::scoped::interpreter(py)?;
        let identity = identity.inner.with(|v| Ok(v.clone()))?;
        let request = request.with_ref(|r| Ok(*r))?;
        let source = source
            .map(|s| Ok::<_, PyErr>(s.try_borrow(py)?.clone()))
            .transpose()?;
        let mut document = self.target.take()?;
        let mut session = session.target.take()?;
        let mut state = state.target.take()?;
        let (failures, result) = py.detach(move || {
            let mut failures = crate::callbacks::CallbackError::default();
            let result = document.receive_initial_identity(
                &mut session,
                &mut state,
                &identity,
                request,
                source.as_ref().map(values::DefaultSource::as_core),
                |event, document, session| {
                    initial::invoke_boundary(
                        &boundary,
                        &mut failures,
                        event,
                        document,
                        session,
                        interpreter,
                    )
                },
            );
            (failures, result)
        });
        failures.finish(py, result.map_err(errors::icf))
    }
    #[pyo3(signature = (session, initial_state, selector, outcome, source, validation, boundary))]
    fn receive_clone_read(
        &self,
        py: Python<'_>,
        session: &Session,
        initial_state: &initial::InitialState,
        selector: &Bound<'_, PyAny>,
        outcome: &crate::clone::session::Outcome,
        source: Option<Py<values::DefaultSource>>,
        validation: &values::ValidationSource,
        boundary: Py<PyAny>,
    ) -> PyResult<Option<values::ValidationReport>> {
        let interpreter = crate::scoped::interpreter(py)?;
        let initial_state = initial_state.with_ref(|s| Ok(s.clone()))?;
        let selector = strict_u8(selector)?;
        let outcome = outcome.with_ref(|o| Ok(o.clone()))?;
        let source = source
            .map(|s| Ok::<_, PyErr>(s.try_borrow(py)?.clone()))
            .transpose()?;
        let validation = validation.values(py)?;
        let mut document = self.take()?;
        let mut session = session.take()?;
        let (failures, result) = py.detach(move || {
            let mut failures = crate::callbacks::CallbackError::default();
            let result = document.receive_clone_read(
                &mut session,
                &initial_state,
                selector,
                &outcome,
                source.as_ref().map(values::DefaultSource::as_core),
                values::validation(&validation),
                |event, d, s| {
                    initial::invoke_boundary(&boundary, &mut failures, event, d, s, interpreter)
                },
            );
            (failures, result)
        });
        failures.finish(
            py,
            result
                .map(|r| r.map(|inner| values::ValidationReport { inner }))
                .map_err(errors::icf),
        )
    }
    fn pending_name(&self) -> PyResult<Option<values::PendingName>> {
        self.with_ref(|d| {
            Ok(d.pending_name()
                .cloned()
                .map(|inner| values::PendingName { inner }))
        })
    }
    fn set_pending_name(&self, pending: Option<&values::PendingName>) -> PyResult<()> {
        let pending = pending.map(|v| v.inner.clone());
        self.with_mut(|d| {
            d.set_pending_name(pending);
            Ok(())
        })
    }
    fn validate_views(
        &self,
        py: Python<'_>,
        session: &Session,
        source: &values::ValidationSource,
    ) -> PyResult<values::ValidationReport> {
        let views = source.values(py)?;
        self.with_ref(|d| {
            session.with_ref(|s| Ok(d.validate_views(s, values::validation(&views))))
        })?
        .map(|inner| values::ValidationReport { inner })
        .map_err(errors::icf)
    }
    #[pyo3(signature = (source, path, prompt_result, profile_choice, session, default_source))]
    fn read(
        &self,
        py: Python<'_>,
        source: Option<&Bound<'_, PyAny>>,
        path: &Bound<'_, PyAny>,
        prompt_result: &Bound<'_, PyAny>,
        profile_choice: Option<&Bound<'_, PyAny>>,
        session: &Session,
        default_source: Option<&values::DefaultSource>,
    ) -> PyResult<values::OpenStatus> {
        let source = source.map(strict_bytes).transpose()?.map(<[u8]>::to_vec);
        let path = strict_bytes(path)?.to_vec();
        let prompt_result = strict_i32(prompt_result)?;
        let profile_choice = profile_choice.map(strict_i32).transpose()?;
        let default_source = default_source.cloned();
        let mut document = self.target.take()?;
        let mut session = session.target.take()?;
        py.detach(move || {
            document.read(
                source.as_deref(),
                path,
                prompt_result,
                profile_choice,
                &mut session,
                default_source.as_ref().map(values::DefaultSource::as_core),
            )
        })
        .map(|inner| values::OpenStatus { inner })
        .map_err(errors::icf)
    }
    fn save(
        &self,
        py: Python<'_>,
        session: &Session,
        path: &Bound<'_, PyAny>,
        target_revision: &Bound<'_, PyAny>,
        legacy_choice: &Bound<'_, PyAny>,
        output: Py<PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let interpreter = crate::scoped::interpreter(py)?;
        let path = strict_bytes(path)?.to_vec();
        let target = strict_u32(target_revision)?;
        let legacy = strict_i32(legacy_choice)?;
        let mut document = self.target.take()?;
        let mut session = session.target.take()?;
        let (failures, result) = py.detach(move || {
            let failures = std::cell::RefCell::new(crate::callbacks::CallbackError::default());
            let result = document.save(
                &mut session,
                path,
                target,
                legacy,
                &mut adapters::BinaryWriter {
                    object: output,
                    errors: &failures,
                    interpreter,
                },
            );
            (failures.into_inner(), result)
        });
        let status = failures.finish(py, result.map_err(adapters::file_error))?;
        Ok(values::enum_out(
            py,
            "cs705.icf.document",
            "SaveStatus",
            match status {
                core::SaveStatus::Saved => 0,
                core::SaveStatus::Cancelled => 1,
                core::SaveStatus::RetrySaveAs => 2,
            },
        )?
        .unbind())
    }
    #[pyo3(signature = (session, request, state, output))]
    fn save_with_validation(
        &self,
        py: Python<'_>,
        session: &Session,
        request: Py<values::SaveRequest>,
        state: Option<Py<crate::icf::normalizer::ResetState>>,
        output: Py<PyAny>,
    ) -> PyResult<reports::SaveReport> {
        let interpreter = crate::scoped::interpreter(py)?;
        let (path, target_revision, legacy_choice, warning_choice, views) = {
            let request = request.try_borrow(py)?;
            (
                request.path.clone(),
                request.target_revision,
                request.legacy_choice,
                request.warning_choice,
                request.validation.get().values(py)?,
            )
        };
        let mut state = state.as_ref().map(|s| s.get().take()).transpose()?;
        let mut document = self.take()?;
        let mut session = session.take()?;
        let (failures, result) = py.detach(move || {
            let failures = std::cell::RefCell::new(crate::callbacks::CallbackError::default());
            let result = document.save_with_validation(
                &mut session,
                core::SaveRequest {
                    path,
                    target_revision,
                    legacy_choice,
                    warning_choice,
                    validation: values::validation(&views),
                },
                state.as_deref_mut(),
                &mut adapters::BinaryWriter {
                    object: output,
                    errors: &failures,
                    interpreter,
                },
            );
            (failures.into_inner(), result)
        });
        let report = failures.finish(py, result.map_err(adapters::file_error))?;
        reports::SaveReport::from_core(py, report)
    }
    fn target_revision(&self) -> PyResult<u32> {
        self.with_ref(|d| Ok(d.target_revision()))
    }
    fn dirty(&self) -> PyResult<u32> {
        self.with_ref(|d| Ok(d.dirty()))
    }
    fn timer_ms(&self) -> PyResult<Option<u32>> {
        self.with_ref(|d| Ok(d.timer_ms()))
    }
    fn replace_current(&self, current: &Bound<'_, PyAny>) -> PyResult<()> {
        let bytes = strict_bytes(current)?.to_vec();
        self.with_mut(|d| {
            d.replace_current(bytes);
            Ok(())
        })
    }
    fn patch_current(&self, patches: &Bound<'_, PyAny>) -> PyResult<()> {
        let patches = patches
            .try_iter()?
            .map(|pair| {
                let pair = pair?.cast_into::<PyTuple>()?;
                if pair.len() != 2 {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "expected (offset, bytes)",
                    ));
                }
                Ok((
                    strict_usize(&pair.get_item(0)?)?,
                    strict_bytes(&pair.get_item(1)?)?.to_vec(),
                ))
            })
            .collect::<PyResult<Vec<_>>>()?;
        let borrowed: Vec<_> = patches
            .iter()
            .map(|(at, bytes)| (*at, bytes.as_slice()))
            .collect();
        self.with_mut(|d| Ok(d.patch_current(&borrowed)))?
            .map_err(errors::icf)
    }
    fn compare_dirty(&self) -> PyResult<u32> {
        self.with_mut(|d| Ok(d.compare_dirty()))
    }
    fn begin_pending_name(
        &self,
        index: &Bound<'_, PyAny>,
        name: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let index = strict_i32(index)?;
        let name = strict_bytes(name)?.to_vec();
        self.with_mut(|d| {
            d.begin_pending_name(index, name);
            Ok(())
        })
    }
    fn reset_pending_name(&self) -> PyResult<()> {
        self.with_mut(|d| {
            d.reset_pending_name();
            Ok(())
        })
    }
    fn pending_matches(&self, index: &Bound<'_, PyAny>) -> PyResult<bool> {
        let index = strict_i32(index)?;
        self.with_ref(|d| Ok(d.pending_matches(index)))
    }
    fn take_pending_nonempty<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyBytes>>> {
        self.with_mut(|d| {
            Ok(d.take_pending_nonempty()
                .map(|bytes| PyBytes::new(py, &bytes)))
        })
    }
    fn new_from_defaults(
        &self,
        defaults: &Bound<'_, PyAny>,
        prompt_result: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        let defaults = strict_bytes(defaults)?;
        let prompt_result = strict_i32(prompt_result)?;
        self.with_mut(|d| Ok(d.new_from_defaults(defaults, prompt_result)))
    }
    fn accept_saved(
        &self,
        path: &Bound<'_, PyAny>,
        target_revision: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let path = strict_bytes(path)?.to_vec();
        let target = strict_u32(target_revision)?;
        self.with_mut(|d| {
            d.accept_saved(path, target);
            Ok(())
        })
    }
    fn new_file(&self, session: &Session, prompt_result: &Bound<'_, PyAny>) -> PyResult<bool> {
        let prompt = strict_i32(prompt_result)?;
        self.with_mut(|d| session.with_ref(|s| Ok(d.new_file(s, prompt))))?
            .map_err(errors::icf)
    }
    fn current_bytes<'py>(
        &self,
        py: Python<'py>,
        session: &Session,
    ) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(|d| session.with_ref(|s| Ok(d.current_bytes(s))))?
            .map(|b| PyBytes::new(py, &b))
            .map_err(errors::icf)
    }
}

#[pyclass(frozen, module = "cs705.icf.document")]
#[derive(Clone)]
pub struct Session {
    target: Target<core::Session>,
}
impl Session {
    pub fn take(&self) -> PyResult<crate::owner::Taken<'_, core::Session>> {
        self.target.take()
    }
    pub fn from_core(value: core::Session) -> Self {
        Self {
            target: Target::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::Session) -> PyResult<R>) -> PyResult<R> {
        self.target.with_ref(f)
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut core::Session) -> PyResult<R>) -> PyResult<R> {
        self.target.with_mut(f)
    }
    pub fn from_loan(loan: crate::scoped::Loan<core::Session>) -> Self {
        Self {
            target: Target::Scoped(loan),
        }
    }
}
#[pymethods]
impl Session {
    #[new]
    #[pyo3(signature = (context, system_langid, previous_reload_guard, document_flags))]
    fn new(
        context: &crate::icf::context::Context,
        system_langid: Option<&Bound<'_, PyAny>>,
        previous_reload_guard: &Bound<'_, PyAny>,
        document_flags: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self::from_core(core::Session::new(
            context.with_ref(|q| Ok(*q))?,
            system_langid.map(strict_u16).transpose()?,
            strict_u32(previous_reload_guard)?,
            fixed_bytes::<2>(document_flags)?,
        )))
    }
    fn share(&self) -> Self {
        self.clone()
    }
    #[getter]
    fn context(&self) -> session_views::SessionContext {
        self.context_view()
    }
    #[setter]
    fn set_context(&self, context: &crate::icf::context::Context) -> PyResult<()> {
        let context = context.with_ref(|q| Ok(*q))?;
        self.with_mut(|s| {
            s.context = context;
            Ok(())
        })
    }
    #[getter]
    fn ranges(&self) -> session_views::SessionRanges {
        self.ranges_view()
    }
    #[setter]
    fn set_ranges(&self, ranges: &crate::icf::frequency::Ranges) -> PyResult<()> {
        let ranges = ranges.with_ref(|r| Ok(r.clone()))?;
        self.with_mut(|s| {
            s.ranges = ranges;
            Ok(())
        })
    }
    fn regenerate_defaults(
        &self,
        py: Python<'_>,
        source: &values::DefaultSource,
        current: &Bound<'_, PyAny>,
    ) -> PyResult<crate::icf::stage9::Report> {
        let source = source.clone();
        let current = strict_bytes(current)?.to_vec();
        let mut session = self.target.take()?;
        let report = py
            .detach(move || session.regenerate_defaults(source.as_core(), &current))
            .map_err(errors::icf)?;
        crate::icf::stage9::Report::from_core(py, report)
    }
    fn defaults<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(|s| Ok(s.defaults().map(|b| PyBytes::new(py, b))))?
            .map_err(errors::icf)
    }
    fn default_selector(&self) -> PyResult<Option<u8>> {
        self.with_ref(|s| Ok(s.default_selector()))
    }
    fn adopt_captured_defaults(
        &self,
        image: &Bound<'_, PyAny>,
        selector: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let image = strict_bytes(image)?.to_vec();
        let selector = strict_u8(selector)?;
        self.with_mut(|s| Ok(s.adopt_captured_defaults(image, selector)))?
            .map_err(errors::icf)
    }
    #[getter]
    fn system_langid(&self) -> PyResult<Option<u16>> {
        self.with_ref(|s| Ok(s.system_langid))
    }
    #[setter]
    fn set_system_langid(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = if value.is_none() {
            None
        } else {
            Some(strict_u16(value)?)
        };
        self.with_mut(|s| {
            s.system_langid = value;
            Ok(())
        })
    }
    #[getter]
    fn previous_reload_guard(&self) -> PyResult<u32> {
        self.with_ref(|s| Ok(s.previous_reload_guard))
    }
    #[setter]
    fn set_previous_reload_guard(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u32(value)?;
        self.with_mut(|s| {
            s.previous_reload_guard = value;
            Ok(())
        })
    }
    #[getter]
    fn document_flags<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(|s| Ok(PyBytes::new(py, &s.document_flags)))
    }
    #[setter]
    fn set_document_flags(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = fixed_bytes::<2>(value)?;
        self.with_mut(|s| {
            s.document_flags = value;
            Ok(())
        })
    }
}

pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "document")?;
    m.add_class::<Document>()?;
    m.add_class::<Session>()?;
    m.add_class::<session_views::SessionContext>()?;
    m.add_class::<session_views::SessionRanges>()?;
    values::register(&m)?;
    reports::register(&m)?;
    lifecycle::register(&m)?;
    options::register(&m)?;
    initial::register(&m)?;
    native_read::register(&m)?;
    clone_write::register(&m)?;
    preparation::register(&m)?;
    root.add_submodule(&m)
}
