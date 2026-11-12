//! Stored menu callbacks keep a visible Python edge and only a native weak link.
use super::{Document, Session, Target, lifecycle::RecentFiles};
use crate::{
    callbacks::{CallbackError, CallbackHolder, CallbackLink},
    conversion::*,
    errors,
    scoped::{Frame, Loan},
};
use cs705_core::icf::{Error, document::lifecycle as core};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3::{PyTraverseError, PyVisit};

#[pyclass(frozen, module = "cs705.icf.document.lifecycle")]
pub struct MenuDescriptor {
    pub target: Target<core::MenuDescriptor>,
}
impl MenuDescriptor {
    pub fn from_core(value: core::MenuDescriptor) -> Self {
        Self {
            target: Target::new(value),
        }
    }
    pub fn from_loan(loan: Loan<core::MenuDescriptor>) -> Self {
        Self {
            target: Target::Scoped(loan),
        }
    }
}
macro_rules! descriptor {
    ($($field:ident, $set:ident: $ty:ty => $strict:ident);*) => {
        #[pymethods]
        impl MenuDescriptor {
            #[new]
            fn new(group: &Bound<'_, PyAny>, key: &Bound<'_, PyAny>, index: &Bound<'_, PyAny>, name: &Bound<'_, PyAny>, caption: &Bound<'_, PyAny>, flags1d: &Bound<'_, PyAny>, flags1e: &Bound<'_, PyAny>, shortcut18: &Bound<'_, PyAny>) -> PyResult<Self> {
                Ok(Self::from_core(core::MenuDescriptor { group: strict_u32(group)?, key: strict_u32(key)?, index: strict_i32(index)?, name: strict_bytes(name)?.to_vec(), caption: strict_bytes(caption)?.to_vec(), flags1d: strict_u8(flags1d)?, flags1e: strict_u8(flags1e)?, shortcut18: strict_u32(shortcut18)? }))
            }
            $(#[getter] fn $field(&self) -> PyResult<$ty> { self.target.with_ref(|v| Ok(v.$field)) }
            #[setter] fn $set(&self, value: &Bound<'_, PyAny>) -> PyResult<()> { let value = $strict(value)?; self.target.with_mut(|v| { v.$field = value; Ok(()) }) })*
            #[getter] fn name<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> { self.target.with_ref(|v| Ok(PyBytes::new(py, &v.name))) }
            #[setter] fn set_name(&self, value: &Bound<'_, PyAny>) -> PyResult<()> { let value = strict_bytes(value)?.to_vec(); self.target.with_mut(|v| { v.name = value; Ok(()) }) }
            #[getter] fn caption<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> { self.target.with_ref(|v| Ok(PyBytes::new(py, &v.caption))) }
            #[setter] fn set_caption(&self, value: &Bound<'_, PyAny>) -> PyResult<()> { let value = strict_bytes(value)?.to_vec(); self.target.with_mut(|v| { v.caption = value; Ok(()) }) }
        }
    };
}
descriptor!(group, set_group: u32 => strict_u32; key, set_key: u32 => strict_u32; index, set_index: i32 => strict_i32; flags1d, set_flags1d: u8 => strict_u8; flags1e, set_flags1e: u8 => strict_u8; shortcut18, set_shortcut18: u32 => strict_u32);

#[pyclass(frozen, module = "cs705.icf.document.lifecycle")]
pub struct NativeView {
    pub target: Target<core::NativeView>,
}
impl NativeView {
    pub fn from_core(value: core::NativeView) -> Self {
        Self {
            target: Target::new(value),
        }
    }
    pub fn from_loan(loan: Loan<core::NativeView>) -> Self {
        Self {
            target: Target::Scoped(loan),
        }
    }
}
#[pymethods]
impl NativeView {
    #[new]
    fn new(
        factory: &crate::printing::native::Factory,
        base_bc: &Bound<'_, PyAny>,
        dataset: Option<&crate::display::ViewState>,
    ) -> PyResult<Self> {
        let factory = factory.with_ref(|v| Ok(v.clone()))?;
        let base_bc = strict_u8(base_bc)?;
        let dataset = dataset.map(|v| v.with_ref(|v| Ok(v.clone()))).transpose()?;
        Ok(Self::from_core(core::NativeView {
            factory,
            base_bc,
            dataset,
        }))
    }
    #[getter]
    fn base_bc(&self) -> PyResult<u8> {
        self.target.with_ref(|v| Ok(v.base_bc))
    }
    #[setter]
    fn set_base_bc(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u8(value)?;
        self.target.with_mut(|v| {
            v.base_bc = value;
            Ok(())
        })
    }
    fn factory_snapshot(&self) -> PyResult<crate::printing::native::Factory> {
        self.target.with_ref(|v| {
            Ok(crate::printing::native::Factory::from_core(
                v.factory.clone(),
            ))
        })
    }
    fn set_factory(&self, value: &crate::printing::native::Factory) -> PyResult<()> {
        let value = value.with_ref(|v| Ok(v.clone()))?;
        self.target.with_mut(|v| {
            v.factory = value;
            Ok(())
        })
    }
    fn dataset_snapshot(&self) -> PyResult<Option<crate::display::ViewState>> {
        self.target
            .with_ref(|v| Ok(v.dataset.clone().map(crate::display::ViewState::from_core)))
    }
    fn set_dataset(&self, value: Option<&crate::display::ViewState>) -> PyResult<()> {
        let value = value.map(|v| v.with_ref(|v| Ok(v.clone()))).transpose()?;
        self.target.with_mut(|v| {
            v.dataset = value;
            Ok(())
        })
    }
}

#[pyclass(frozen, module = "cs705.icf.document.lifecycle")]
pub struct MenuContext {
    #[pyo3(get)]
    pub document: Py<Document>,
    #[pyo3(get)]
    pub session: Py<Session>,
    #[pyo3(get)]
    pub settings: Py<crate::display::Settings>,
    #[pyo3(get)]
    pub active: Option<Py<NativeView>>,
    #[pyo3(get)]
    pub recent: Py<RecentFiles>,
}
#[pymethods]
impl MenuContext {
    #[new]
    #[pyo3(signature = (document, session, settings, active, recent))]
    fn new(
        document: Py<Document>,
        session: Py<Session>,
        settings: Py<crate::display::Settings>,
        active: Option<Py<NativeView>>,
        recent: Py<RecentFiles>,
    ) -> Self {
        Self {
            document,
            session,
            settings,
            active,
            recent,
        }
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.document)?;
        visit.call(&self.session)?;
        visit.call(&self.settings)?;
        visit.call(&self.active)?;
        visit.call(&self.recent)
    }
}

fn with_active<R>(
    py: Python<'_>,
    active: Option<&core::NativeView>,
    f: impl FnOnce(Option<Loan<core::NativeView>>) -> PyResult<R>,
) -> PyResult<R> {
    match active {
        Some(active) => Frame::with_ref(py, active, |loan| f(Some(loan))),
        None => f(None),
    }
}
fn invoke(
    link: &CallbackLink,
    context: &mut core::MenuContext<'_>,
    descriptor: &mut core::MenuDescriptor,
) -> Result<(), Error> {
    Python::attach(|py| {
        link.invoke(py, |callable| {
            Frame::with_mut(py, context.document, |document| {
                Frame::with_mut(py, context.session, |session| {
                    Frame::with_ref(py, context.settings, |settings| {
                        Frame::with_ref(py, context.recent, |recent| {
                            with_active(py, context.active, |active| {
                                Frame::with_mut(py, descriptor, |descriptor| {
                                    let context = Py::new(
                                        py,
                                        MenuContext {
                                            document: Py::new(py, Document::from_loan(document))?,
                                            session: Py::new(py, Session::from_loan(session))?,
                                            settings: Py::new(
                                                py,
                                                crate::display::Settings::from_loan(settings),
                                            )?,
                                            active: active
                                                .map(|loan| {
                                                    Py::new(py, NativeView::from_loan(loan))
                                                })
                                                .transpose()?,
                                            recent: Py::new(py, RecentFiles::from_loan(recent))?,
                                        },
                                    )?;
                                    let descriptor =
                                        Py::new(py, MenuDescriptor::from_loan(descriptor))?;
                                    super::adapters::require_none(
                                        callable.call1((context, descriptor))?,
                                    )
                                })
                            })
                        })
                    })
                })
            })
        })
    })
    .map_err(|_| Error::Operation("Python menu query failed".into()))
}

#[pyclass(module = "cs705.icf.document.lifecycle")]
pub struct MenuQuery {
    pub inner: core::MenuQuery,
    pub holder: Option<Py<CallbackHolder>>,
}
impl MenuQuery {
    pub fn from_core(inner: core::MenuQuery) -> Self {
        Self {
            inner,
            holder: None,
        }
    }
    pub fn clone_ref(&self, py: Python<'_>) -> Self {
        Self {
            inner: self.inner.clone(),
            holder: self.holder.as_ref().map(|h| h.clone_ref(py)),
        }
    }
}

#[pyclass(module = "cs705.icf.document.lifecycle")]
pub struct MenuItem {
    #[pyo3(get, set)]
    pub descriptor: Option<Py<MenuDescriptor>>,
    #[pyo3(get, set)]
    pub query: Option<Py<MenuQuery>>,
    property19a: bool,
    property1ae: bool,
    property1a1: bool,
    enabled: bool,
    shortcut: u32,
    caption: Vec<u8>,
}
impl MenuItem {
    pub fn to_core(&self, py: Python<'_>) -> PyResult<core::MenuItem> {
        let descriptor = self
            .descriptor
            .as_ref()
            .map(|v| v.get().target.with_ref(|d| Ok(d.clone())))
            .transpose()?;
        let query = self
            .query
            .as_ref()
            .map(|q| Ok::<_, PyErr>(q.try_borrow(py)?.inner.clone()))
            .transpose()?;
        Ok(core::MenuItem {
            descriptor,
            query,
            property19a: self.property19a,
            property1ae: self.property1ae,
            property1a1: self.property1a1,
            enabled: self.enabled,
            shortcut: self.shortcut,
            caption: self.caption.clone(),
        })
    }
}
macro_rules! item_fields {
    ($($field:ident, $set:ident: $ty:ty => $strict:ident);*) => {
        #[pymethods]
        impl MenuItem {
            #[new]
            #[pyo3(signature = (descriptor, query, property19a, property1ae, property1a1, enabled, shortcut, caption))]
            fn new(descriptor: Option<Py<MenuDescriptor>>, query: Option<Py<MenuQuery>>, property19a: &Bound<'_, PyAny>, property1ae: &Bound<'_, PyAny>, property1a1: &Bound<'_, PyAny>, enabled: &Bound<'_, PyAny>, shortcut: &Bound<'_, PyAny>, caption: &Bound<'_, PyAny>) -> PyResult<Self> {
                Ok(Self { descriptor, query, property19a: strict_bool(property19a)?, property1ae: strict_bool(property1ae)?, property1a1: strict_bool(property1a1)?, enabled: strict_bool(enabled)?, shortcut: strict_u32(shortcut)?, caption: strict_bytes(caption)?.to_vec() })
            }
            $(#[getter] fn $field(&self) -> $ty { self.$field }
            #[setter] fn $set(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> { self.$field = $strict(value)?; Ok(()) })*
            #[getter] fn caption<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> { PyBytes::new(py, &self.caption) }
            #[setter] fn set_caption(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> { self.caption = strict_bytes(value)?.to_vec(); Ok(()) }
            fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> { visit.call(&self.descriptor)?; visit.call(&self.query) }
            fn __clear__(&mut self) { self.descriptor = None; self.query = None; }
        }
    };
}
item_fields!(property19a, set_property19a: bool => strict_bool; property1ae, set_property1ae: bool => strict_bool; property1a1, set_property1a1: bool => strict_bool; enabled, set_enabled: bool => strict_bool; shortcut, set_shortcut: u32 => strict_u32);
#[pymethods]
impl MenuQuery {
    #[new]
    fn new(py: Python<'_>, callable: &Bound<'_, PyAny>) -> PyResult<Self> {
        let holder = CallbackHolder::new(py, callable)?;
        let link = CallbackHolder::link(holder.bind(py));
        Ok(Self {
            inner: core::MenuQuery::new(move |context, descriptor| {
                invoke(&link, context, descriptor)
            }),
            holder: Some(holder),
        })
    }
    fn clone(&self, py: Python<'_>) -> Self {
        self.clone_ref(py)
    }
    fn call(
        slf: &Bound<'_, Self>,
        context: &MenuContext,
        descriptor: &MenuDescriptor,
    ) -> PyResult<()> {
        let py = slf.py();
        // Keep the exact holder alive without a PyO3 borrow during callbacks.
        let query = slf.try_borrow()?.clone_ref(py);
        let mut document = context.document.get().take()?;
        let mut session = context.session.get().take()?;
        let settings = context.settings.get().target.take()?;
        let active = context
            .active
            .as_ref()
            .map(|v| v.get().target.take())
            .transpose()?;
        let recent = context.recent.get().target.take()?;
        let mut descriptor = descriptor.target.take()?;
        let (failures, result) = CallbackError::scoped(py, || {
            py.detach(move || {
                query.inner.call(
                    &mut core::MenuContext {
                        document: &mut document,
                        session: &mut session,
                        settings: &settings,
                        active: active.as_deref(),
                        recent: &recent,
                    },
                    &mut descriptor,
                )
            })
        })?;
        failures.finish(py, result.map_err(errors::icf))
    }
    fn __repr__(&self) -> &'static str {
        "MenuQuery(..)"
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.holder)
    }
    fn __clear__(&mut self) {
        self.holder = None;
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MenuQuery>()?;
    m.add_class::<MenuContext>()?;
    m.add_class::<MenuDescriptor>()?;
    m.add_class::<NativeView>()?;
    m.add_class::<MenuItem>()?;
    Ok(())
}
