//! A receiver retains the actual Python owners, never native state snapshots.
use super::*;
use crate::{
    callbacks::CallbackError,
    icf::document::{
        Document, Session, initial::InitialState, lifecycle::RecentFiles, ui::UiState,
    },
};
use cs705_core::clone::information as native;
use pyo3::{PyTraverseError, PyVisit};
use std::sync::Arc;

struct References {
    document: Py<Document>,
    session: Py<Session>,
    initial: Py<InitialState>,
    ui: Py<UiState>,
    recent: Py<RecentFiles>,
    busy: Arc<crate::owner::Owner<()>>,
}
#[pyclass(module = "cs705.clone.information")]
pub struct DocumentReceiver {
    document: Option<Py<Document>>,
    session: Option<Py<Session>>,
    initial: Option<Py<InitialState>>,
    ui: Option<Py<UiState>>,
    recent: Option<Py<RecentFiles>>,
    busy: Arc<crate::owner::Owner<()>>,
}
impl DocumentReceiver {
    fn references(&self, py: Python<'_>) -> PyResult<References> {
        Ok(References {
            document: self.document.as_ref().ok_or_else(cleared)?.clone_ref(py),
            session: self.session.as_ref().ok_or_else(cleared)?.clone_ref(py),
            initial: self.initial.as_ref().ok_or_else(cleared)?.clone_ref(py),
            ui: self.ui.as_ref().ok_or_else(cleared)?.clone_ref(py),
            recent: self.recent.as_ref().ok_or_else(cleared)?.clone_ref(py),
            busy: self.busy.clone(),
        })
    }
}
fn cleared() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err("document receiver was cleared")
}
macro_rules! receiver_impl{
    ($($field:ident,$set:ident:$ty:ty),* $(,)?)=>{
        #[pymethods]impl DocumentReceiver{
            #[new]fn new($($field:Py<$ty>),*)->Self{Self{$($field:Some($field),)*busy:Arc::new(crate::owner::Owner::new(()))}}
            $(#[getter]fn $field(&self,py:Python<'_>)->PyResult<Py<$ty>>{Ok(self.$field.as_ref().ok_or_else(cleared)?.clone_ref(py))}
              #[setter]fn $set(&mut self,value:Py<$ty>)->PyResult<()>{self.busy.with_ref(|_|Ok(()))?;self.$field=Some(value);Ok(())})*
            fn __traverse__(&self,visit:PyVisit<'_>)->Result<(),PyTraverseError>{$ (visit.call(&self.$field)?;)*Ok(())}
            fn __clear__(&mut self){let removed=($(self.$field.take(),)*);drop(removed);}
        }
    }
}
receiver_impl!(document,set_document:Document,session,set_session:Session,initial,set_initial:InitialState,ui,set_ui:UiState,recent,set_recent:RecentFiles);

pub(crate) fn run<R: Send>(
    py: Python<'_>,
    receiver: &Py<DocumentReceiver>,
    f: impl FnOnce(&mut native::DocumentReceiver<'_>) -> PyResult<R> + Send,
) -> PyResult<(CallbackError, PyResult<R>)> {
    let refs = receiver.bind(py).try_borrow()?.references(py)?;
    let _busy = refs.busy.take()?;
    let mut document = refs.document.get().take()?;
    let mut session = refs.session.get().take()?;
    let mut initial = refs.initial.get().target.take()?;
    let mut ui = refs.ui.get().owner.take()?;
    let recent = refs.recent.get().target.take()?;
    CallbackError::scoped(py, || {
        py.detach(move || {
            f(&mut native::DocumentReceiver {
                document: &mut document,
                session: &mut session,
                initial: &mut initial,
                ui: &mut ui,
                recent: &recent,
            })
        })
    })
}
pub(crate) fn error(value: cs705_core::icf::document::CloneError) -> PyErr {
    match value {
        cs705_core::icf::document::CloneError::Image(e) => errors::icf(e),
        cs705_core::icf::document::CloneError::Protocol(e) => errors::clone(e),
    }
}
