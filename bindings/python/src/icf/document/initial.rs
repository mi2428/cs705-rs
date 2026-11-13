//! Initial adoption boundaries borrow the actual document and shared session.
use super::{Document, Session, values::scalar_owner};
use crate::{callbacks::CallbackError, conversion::*, scoped::Frame};
use core::InitialRequest as CoreInitialRequest;
use cs705_core::icf::{Error, document as core};
use pyo3::prelude::*;

scalar_owner!(InitialRequest, CoreInitialRequest, "cs705.icf.document.initial", {
    mode, set_mode: u32 => strict_u32,
    notification, set_notification: u32 => strict_u32,
    additional, set_additional: u32 => strict_u32,
} methods {
    #[staticmethod]
    fn information(session: &Session, identity: &crate::clone::Identity, scalar1: &Bound<'_, PyAny>) -> PyResult<Option<Self>> {
        let scalar1 = strict_u32(scalar1)?;
        let identity = identity.inner.with(|i| Ok(i.clone()))?;
        session.with_ref(|s| Ok(core::InitialRequest::information(s, &identity, scalar1)))?
            .map(|r| r.map(Self::from_core)).map_err(crate::errors::icf)
    }
});

#[pyclass(frozen, module = "cs705.icf.document.initial")]
pub struct InitialState {
    pub target: super::Target<core::InitialState>,
}
impl InitialState {
    pub fn from_loan(loan: crate::scoped::Loan<core::InitialState>) -> Self {
        Self {
            target: super::Target::Scoped(loan),
        }
    }
    pub fn from_core(value: core::InitialState) -> Self {
        Self {
            target: super::Target::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::InitialState) -> PyResult<R>) -> PyResult<R> {
        self.target.with_ref(f)
    }
    pub fn with_mut<R>(
        &self,
        f: impl FnOnce(&mut core::InitialState) -> PyResult<R>,
    ) -> PyResult<R> {
        self.target.with_mut(f)
    }
}
#[pymethods]
impl InitialState {
    #[new]
    fn new() -> Self {
        Self::from_core(core::InitialState::default())
    }
    #[getter]
    fn saved_selector(&self) -> PyResult<u8> {
        self.with_ref(|s| Ok(s.saved_selector))
    }
    #[setter]
    fn set_saved_selector(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u8(value)?;
        self.with_mut(|s| {
            s.saved_selector = value;
            Ok(())
        })
    }
    #[getter]
    fn saved_context(&self) -> PyResult<Option<InitialContext>> {
        self.with_ref(|s| {
            Ok(s.saved_context.map(|_| InitialContext {
                target: self.target.clone(),
            }))
        })
    }
    #[setter]
    fn set_saved_context(&self, value: Option<&crate::clone::ContextFields>) -> PyResult<()> {
        let value = value.map(|v| v.with_ref(|v| Ok(*v))).transpose()?;
        self.with_mut(|s| {
            s.saved_context = value;
            Ok(())
        })
    }
}
/// Live optional saved-context field. values returns a list copy; assign values
/// to replace all twelve fields. Access fails while the parent field is absent.
#[pyclass(frozen, module = "cs705.icf.document.initial")]
pub struct InitialContext {
    target: super::Target<core::InitialState>,
}
#[pymethods]
impl InitialContext {
    fn snapshot(&self) -> PyResult<crate::clone::ContextFields> {
        self.target.with_ref(|s| {
            s.saved_context
                .map(crate::clone::ContextFields::from_core)
                .ok_or_else(|| {
                    pyo3::exceptions::PyReferenceError::new_err("saved context was removed")
                })
        })
    }
    #[getter]
    fn values(&self) -> PyResult<Vec<u32>> {
        self.target.with_ref(|s| {
            s.saved_context.map(|v| v.0.to_vec()).ok_or_else(|| {
                pyo3::exceptions::PyReferenceError::new_err("saved context was removed")
            })
        })
    }
    #[setter]
    fn set_values(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let values = value
            .try_iter()?
            .map(|v| strict_u32(&v?))
            .collect::<PyResult<Vec<_>>>()?;
        let values = cs705_core::clone::ContextFields(values.try_into().map_err(|_| {
            pyo3::exceptions::PyValueError::new_err("expected twelve context fields")
        })?);
        self.target.with_mut(|s| {
            *s.saved_context.as_mut().ok_or_else(|| {
                pyo3::exceptions::PyReferenceError::new_err("saved context was removed")
            })? = values;
            Ok(())
        })
    }
}
#[pyclass(frozen, module = "cs705.icf.document.initial")]
#[derive(Clone, Copy)]
pub struct InitialBoundary {
    pub inner: core::InitialBoundary,
}
#[pymethods]
impl InitialBoundary {
    #[classattr]
    #[pyo3(name = "Reconfigure")]
    fn reconfigure() -> Self {
        Self {
            inner: core::InitialBoundary::Reconfigure,
        }
    }
    #[classattr]
    #[pyo3(name = "Additional")]
    fn additional() -> Self {
        Self {
            inner: core::InitialBoundary::Additional,
        }
    }
    #[classattr]
    #[pyo3(name = "Rebuild")]
    fn rebuild() -> Self {
        Self {
            inner: core::InitialBoundary::Rebuild,
        }
    }
    #[classattr]
    #[pyo3(name = "ReleaseView")]
    fn release_view() -> Self {
        Self {
            inner: core::InitialBoundary::ReleaseView,
        }
    }
    #[classattr]
    #[pyo3(name = "Title")]
    fn title() -> Self {
        Self {
            inner: core::InitialBoundary::Title,
        }
    }
    #[staticmethod]
    #[pyo3(name = "Sync")]
    fn sync(event: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::InitialBoundary::Sync(strict_u32(event)?),
        })
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self.inner {
            core::InitialBoundary::Reconfigure => "Reconfigure",
            core::InitialBoundary::Additional => "Additional",
            core::InitialBoundary::Rebuild => "Rebuild",
            core::InitialBoundary::ReleaseView => "ReleaseView",
            core::InitialBoundary::Sync(_) => "Sync",
            core::InitialBoundary::Title => "Title",
        }
    }
    #[getter]
    fn event(&self) -> Option<u32> {
        if let core::InitialBoundary::Sync(event) = self.inner {
            Some(event)
        } else {
            None
        }
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
pub(crate) fn invoke_boundary(
    callback: &Py<PyAny>,
    errors: &mut CallbackError,
    boundary: core::InitialBoundary,
    document: &mut core::Document,
    session: &mut core::Session,
    interpreter: i64,
) -> Result<(), Error> {
    let result = super::adapters::attach_checked(interpreter, |py| {
        Frame::with_mut(py, document, |document| {
            Frame::with_mut(py, session, |session| {
                let boundary = Py::new(py, InitialBoundary { inner: boundary })?;
                let document = Py::new(py, Document::from_loan(document))?;
                let session = Py::new(py, Session::from_loan(session))?;
                super::adapters::require_none(
                    callback.bind(py).call1((boundary, document, session))?,
                )
            })
        })
    });
    errors
        .capture(result)
        .map_err(|_| Error::Operation("Python initial boundary failed".into()))
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "initial")?;
    m.add_class::<InitialBoundary>()?;
    m.add_class::<InitialRequest>()?;
    m.add_class::<InitialState>()?;
    m.add_class::<InitialContext>()?;
    root.add("InitialBoundary", m.getattr("InitialBoundary")?)?;
    root.add("InitialRequest", m.getattr("InitialRequest")?)?;
    root.add("InitialState", m.getattr("InitialState")?)?;
    root.add_submodule(&m)
}
