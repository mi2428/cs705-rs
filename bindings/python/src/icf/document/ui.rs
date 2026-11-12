//! UI ownership and calculations only; no native window operation is simulated.
use super::{
    Document, Session, Target,
    initial::InitialBoundary,
    lifecycle::RecentFiles,
    menu::{MenuItem, NativeView},
};
use crate::{callbacks::CallbackError, conversion::*, errors, owner::Owner};
use cs705_core::icf::document::lifecycle as core;
use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple};
use pyo3::{PyTraverseError, PyVisit};
use std::sync::Arc;

/// Constructed only from a native state; native Clone retains its private asset.
#[pyclass(frozen, module = "cs705.icf.document.lifecycle")]
pub struct NativeState {
    target: Target<core::NativeState>,
}
impl NativeState {
    pub fn from_core(value: core::NativeState) -> Self {
        Self {
            target: Target::new(value),
        }
    }
}
#[pymethods]
impl NativeState {
    fn clone(&self) -> PyResult<Self> {
        self.target.with_ref(|v| Ok(Self::from_core(v.clone())))
    }
    #[getter]
    fn show_split(&self) -> PyResult<u32> {
        self.target.with_ref(|v| Ok(v.show_split))
    }
    #[setter]
    fn set_show_split(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u32(value)?;
        self.target.with_mut(|v| {
            v.show_split = value;
            Ok(())
        })
    }
    fn factories_snapshot(&self) -> PyResult<Vec<crate::printing::native::Factory>> {
        self.target.with_ref(|v| {
            Ok(v.factories
                .iter()
                .cloned()
                .map(crate::printing::native::Factory::from_core)
                .collect())
        })
    }
    fn set_factories(&self, values: Vec<Py<crate::printing::native::Factory>>) -> PyResult<()> {
        let values = values
            .iter()
            .map(|v| v.get().with_ref(|v| Ok(v.clone())))
            .collect::<PyResult<Vec<_>>>()?;
        self.target.with_mut(|v| {
            v.factories = values;
            Ok(())
        })
    }
    fn active_snapshot(&self) -> PyResult<Option<NativeView>> {
        self.target
            .with_ref(|v| Ok(v.active.clone().map(NativeView::from_core)))
    }
    fn set_active(&self, value: Option<&NativeView>) -> PyResult<()> {
        let value = value
            .map(|v| v.target.with_ref(|v| Ok(v.clone())))
            .transpose()?;
        self.target.with_mut(|v| {
            v.active = value;
            Ok(())
        })
    }
    fn resources(&self) -> PyResult<crate::display::Resources> {
        let source = self
            .target
            .with_ref(|v| Ok(v.resources().map(|r| r.source_pe().to_vec())))?
            .map_err(errors::io)?;
        crate::display::Resources::from_unbound_source(source)
    }
}

#[pyclass(frozen, module = "cs705.icf.document.lifecycle")]
/// Software UI owner. Metadata getters are read-only snapshots; replace fields whole.
pub struct UiState {
    pub owner: Arc<Owner<core::UiState>>,
    // One explicit edge to the exact queries adopted by the native menu snapshot.
    retained: Py<PyList>,
}
impl UiState {
    /// Native-only construction. Python menu holders are installed by set_menu.
    pub fn from_core(py: Python<'_>, value: core::UiState) -> PyResult<Self> {
        Ok(Self {
            owner: Arc::new(Owner::new(value)),
            retained: PyList::new(py, [py.None()])?.unbind(),
        })
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::UiState) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_ref(f)
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut core::UiState) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_mut(f)
    }
}
macro_rules! ui_methods {
    ($($field:ident, $set:ident: $ty:ty => $strict:ident);*) => {
#[pymethods]
impl UiState {
    $(#[getter] fn $field(&self) -> PyResult<$ty> { self.with_ref(|ui| Ok(ui.$field)) }
    #[setter] fn $set(&self, value: &Bound<'_, PyAny>) -> PyResult<()> { let value = $strict(value)?; self.with_mut(|ui| { ui.$field = value; Ok(()) }) })*
    #[staticmethod]
    fn headless(py: Python<'_>) -> PyResult<Self> {
        Self::from_core(py, core::UiState::headless())
    }
    fn share(&self, py: Python<'_>) -> Self {
        Self {
            owner: self.owner.clone(),
            retained: self.retained.clone_ref(py),
        }
    }
    fn set_menu(&self, py: Python<'_>, items: Vec<Py<MenuItem>>) -> PyResult<()> {
        let mut menu = Vec::with_capacity(items.len());
        let mut queries = Vec::new();
        for item in &items {
            let item = item.try_borrow(py)?;
            menu.push(item.to_core(py)?);
            if let Some(query) = &item.query { queries.push(query.clone_ref(py)); }
        }
        let retained = PyList::new(py, queries)?;
        let mut ui = self.owner.take()?;
        self.retained.bind(py).set_item(0, retained)?;
        ui.menu = menu;
        Ok(())
    }
    /// Read-only metadata snapshots; replace the complete field to edit it.
    #[getter]
    fn menu<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        let menu = self.with_ref(|ui| Ok(ui.menu.clone()))?;
        let adopted = self.retained.bind(py).get_item(0)?;
        let queries = if adopted.is_none() { None } else { Some(adopted.cast::<PyList>()?.iter().map(|q| Ok(q.extract::<Py<super::menu::MenuQuery>>()?)).collect::<PyResult<Vec<_>>>()?) };
        if queries.as_ref().is_some_and(|q| q.len() != menu.iter().filter(|item| item.query.is_some()).count()) {
            return Err(pyo3::exceptions::PyRuntimeError::new_err("native menu and retained query ownership disagree"));
        }
        let mut adopted = queries.map(|q| q.into_iter());
        let values = menu.into_iter().map(|item| {
            let query = match &item.query {
                None => None,
                Some(query) => Some(match &mut adopted { Some(queries) => queries.next().expect("validated query count"), None => Py::new(py, super::menu::MenuQuery::from_core(query.clone()))? }),
            };
            Py::new(py, super::ui_values::MenuItemSnapshot { inner: item, query })
        }).collect::<PyResult<Vec<_>>>()?;
        PyTuple::new(py, values)
    }
    #[setter(menu)] fn assign_menu(&self, py: Python<'_>, items: Vec<Py<MenuItem>>) -> PyResult<()> { self.set_menu(py, items) }
    #[getter] fn image_list<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> { let values = self.with_ref(|ui| Ok(ui.image_list.clone()))?; PyTuple::new(py, values) }
    #[setter] fn set_image_list(&self, value: &Bound<'_, PyAny>) -> PyResult<()> { let value = value.try_iter()?.map(|v| strict_u32(&v?)).collect::<PyResult<Vec<_>>>()?; self.with_mut(|ui| { ui.image_list = value; Ok(()) }) }
    #[getter] fn form(&self) -> PyResult<Option<super::ui_values::FormStateSnapshot>> { self.with_ref(|ui| Ok(ui.form.clone().map(|inner| super::ui_values::FormStateSnapshot { inner }))) }
    #[setter] fn set_form(&self, py: Python<'_>, value: Option<Py<super::lifecycle::FormState>>) -> PyResult<()> { let value = value.map(|v| Ok::<_, PyErr>(v.try_borrow(py)?.inner.clone())).transpose()?; self.with_mut(|ui| { ui.form = value; Ok(()) }) }
    #[getter] fn tree(&self) -> PyResult<Option<super::ui_values::TreeStateSnapshot>> { self.with_ref(|ui| Ok(ui.tree.clone().map(|inner| super::ui_values::TreeStateSnapshot { inner }))) }
    #[setter] fn set_tree(&self, py: Python<'_>, value: Option<Py<super::ui_values::TreeState>>) -> PyResult<()> { let value = value.map(|v| Ok::<_, PyErr>(v.try_borrow(py)?.inner.clone())).transpose()?; self.with_mut(|ui| { ui.tree = value; Ok(()) }) }
    #[getter] fn view(&self) -> PyResult<Option<super::ui_values::ViewStateSnapshot>> { self.with_ref(|ui| Ok(ui.view.clone().map(|inner| super::ui_values::ViewStateSnapshot { inner }))) }
    #[setter] fn set_view(&self, py: Python<'_>, value: Option<Py<super::ui_values::ViewState>>) -> PyResult<()> { let value = value.map(|v| Ok::<_, PyErr>(v.try_borrow(py)?.inner.clone())).transpose()?; self.with_mut(|ui| { ui.view = value; Ok(()) }) }
    #[getter] fn tree_source(&self) -> PyResult<Option<super::ui_values::TreeStateSnapshot>> { self.with_ref(|ui| Ok(ui.tree_source.clone().map(|inner| super::ui_values::TreeStateSnapshot { inner }))) }
    #[setter] fn set_tree_source(&self, py: Python<'_>, value: Option<Py<super::ui_values::TreeState>>) -> PyResult<()> { let value = value.map(|v| Ok::<_, PyErr>(v.try_borrow(py)?.inner.clone())).transpose()?; self.with_mut(|ui| { ui.tree_source = value; Ok(()) }) }
    #[getter] fn view_sources<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> { let values = self.with_ref(|ui| Ok(ui.view_sources.clone()))?; let values = values.into_iter().map(|inner| Py::new(py, super::ui_values::ViewStateSnapshot { inner })).collect::<PyResult<Vec<_>>>()?; PyTuple::new(py, values) }
    #[setter] fn set_view_sources(&self, py: Python<'_>, values: Vec<Py<super::ui_values::ViewState>>) -> PyResult<()> { let values = values.iter().map(|v| Ok(v.try_borrow(py)?.inner.clone())).collect::<PyResult<Vec<_>>>()?; self.with_mut(|ui| { ui.view_sources = values; Ok(()) }) }
    #[getter] fn title(&self) -> PyResult<Option<super::ui_values::TitleStateSnapshot>> { self.with_ref(|ui| Ok(ui.title.clone().map(|inner| super::ui_values::TitleStateSnapshot { inner }))) }
    #[setter] fn set_title(&self, py: Python<'_>, value: Option<Py<super::ui_values::TitleState>>) -> PyResult<()> { let value = value.map(|v| Ok::<_, PyErr>(v.try_borrow(py)?.inner.clone())).transpose()?; self.with_mut(|ui| { ui.title = value; Ok(()) }) }
    #[getter] fn saved_placement(&self) -> PyResult<Option<super::ui_values::PlacementSnapshot>> { self.with_ref(|ui| Ok(ui.saved_placement.map(|inner| super::ui_values::PlacementSnapshot { inner }))) }
    #[setter] fn set_saved_placement(&self, py: Python<'_>, value: Option<Py<super::lifecycle_values::Placement>>) -> PyResult<()> { let value = value.map(|v| Ok::<_, PyErr>(v.try_borrow(py)?.inner)).transpose()?; self.with_mut(|ui| { ui.saved_placement = value; Ok(()) }) }
    #[getter] fn validation(&self) -> PyResult<super::ui_values::ValidationReportSnapshot> { self.with_ref(|ui| Ok(super::ui_values::ValidationReportSnapshot { inner: ui.validation.clone() })) }
    #[setter] fn set_validation(&self, py: Python<'_>, value: Py<super::values::ValidationReport>) -> PyResult<()> { let value = value.try_borrow(py)?.inner.clone(); self.with_mut(|ui| { ui.validation = value; Ok(()) }) }
    #[getter] fn configuration(&self) -> PyResult<Option<super::ui_values::ContextFieldsSnapshot>> { self.with_ref(|ui| Ok(ui.configuration.map(|inner| super::ui_values::ContextFieldsSnapshot { inner }))) }
    #[setter] fn set_configuration(&self, value: Option<&crate::clone::ContextFields>) -> PyResult<()> { let value = value.map(|v| v.with_ref(|v| Ok(*v))).transpose()?; self.with_mut(|ui| { ui.configuration = value; Ok(()) }) }
    fn display_settings_snapshot(&self) -> PyResult<Option<crate::display::Settings>> { let value = self.with_ref(|ui| Ok(ui.display_settings.clone()))?; Ok(value.map(crate::display::Settings::from_core)) }
    fn native_snapshot(&self) -> PyResult<Option<NativeState>> { self.with_ref(|ui| Ok(ui.native.clone().map(NativeState::from_core))) }
    fn set_native(&self, value: Option<&NativeState>) -> PyResult<()> {
        let value = value.map(|v| v.target.with_ref(|v| Ok(v.clone()))).transpose()?;
        self.with_mut(|ui| { ui.native = value; Ok(()) })
    }
    fn set_display_settings(&self, settings: &crate::display::Settings) -> PyResult<()> {
        let settings = settings.with_ref(|s| Ok(s.clone()))?;
        self.with_mut(|ui| {
            ui.display_settings = Some(settings);
            Ok(())
        })
    }
    fn clear_display_settings(&self) -> PyResult<()> {
        self.with_mut(|ui| {
            ui.display_settings = None;
            Ok(())
        })
    }
    fn owner_enabled(&self, enabled: &Bound<'_, PyAny>) -> PyResult<()> {
        let enabled = strict_u8(enabled)?;
        self.with_mut(|ui| Ok(ui.owner_enabled(enabled)))?
            .map_err(errors::icf)
    }
    fn owner_destroyed(&self) -> PyResult<()> {
        self.with_mut(|ui| Ok(ui.owner_destroyed()))?
            .map_err(errors::icf)
    }
    fn sync28(&self, document: &Document, busy: &Bound<'_, PyAny>) -> PyResult<bool> {
        let busy = strict_bool(busy)?;
        self.with_mut(|ui| document.with_mut(|d| Ok(ui.sync28(d, busy))))
    }
    fn sync29(&self, document: &Document) -> PyResult<()> {
        self.with_mut(|ui| document.with_mut(|d| Ok(ui.sync29(d))))?
            .map_err(errors::icf)
    }
    fn sync2f(&self, document: &Document) -> PyResult<()> {
        self.with_mut(|ui| document.with_mut(|d| Ok(ui.sync2f(d))))?
            .map_err(errors::icf)
    }
    fn sync25(&self, document: &Document, session: &Session) -> PyResult<()> {
        self.with_mut(|ui| document.with_mut(|d| session.with_mut(|s| Ok(ui.sync25(d, s)))))?
            .map_err(errors::icf)
    }
    fn sync02(
        &self,
        py: Python<'_>,
        document: &Document,
        session: &Session,
        recent: &RecentFiles,
    ) -> PyResult<()> {
        let mut ui = self.owner.take()?;
        let mut document = document.target.take()?;
        let mut session = session.target.take()?;
        let recent = recent.target.take()?;
        let (failures, result) = CallbackError::scoped(py, || {
            py.detach(move || ui.sync02(&mut document, &mut session, &recent))
        })?;
        failures.finish(py, result.map_err(errors::icf))
    }
    fn initial_boundary(
        &self,
        py: Python<'_>,
        boundary: &InitialBoundary,
        document: &Document,
        session: &Session,
    ) -> PyResult<()> {
        let boundary = boundary.inner;
        let mut ui = self.owner.take()?;
        let mut document = document.target.take()?;
        let mut session = session.target.take()?;
        let (failures, result) = CallbackError::scoped(py, || {
            py.detach(move || ui.initial_boundary(boundary, &mut document, &mut session))
        })?;
        failures.finish(py, result.map_err(errors::icf))
    }
    fn destroy_view(&self, document: &Document) -> PyResult<()> {
        self.with_mut(|ui| {
            document.with_mut(|d| {
                ui.destroy_view(d);
                Ok(())
            })
        })
    }
    fn finish_close(&self, document: &Document) -> PyResult<()> {
        self.with_mut(|ui| {
            document.with_mut(|d| {
                ui.finish_close(d);
                Ok(())
            })
        })
    }
    #[staticmethod]
    fn from_native_source(
        py: Python<'_>,
        asset: &Bound<'_, PyAny>,
        document: &Document,
        session: &Session,
        show_split: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let asset: Arc<[u8]> = strict_bytes(asset)?.into();
        let show_split = strict_u32(show_split)?;
        let ui = document
            .with_mut(|d| {
                session.with_mut(|s| Ok(core::UiState::from_native_source(asset, d, s, show_split)))
            })?
            .map_err(errors::icf)?;
        Self::from_core(py, ui)
    }
    #[staticmethod]
    fn from_native_resources(py: Python<'_>, asset: &Bound<'_, PyAny>, document: &Document, session: &Session, settings: &crate::display::Settings, recent: &RecentFiles) -> PyResult<Self> {
        let asset: Arc<[u8]> = strict_bytes(asset)?.into();
        let settings = settings.with_ref(|s| Ok(s.clone()))?;
        let mut document = document.take()?;
        let mut session = session.take()?;
        let recent = recent.target.take()?;
        let ui = py.detach(move || core::UiState::from_native_resources(asset, &mut document, &mut session, settings, &recent)).map_err(errors::icf)?;
        Self::from_core(py, ui)
    }
    fn register_native_commands(&self, py: Python<'_>, resources: &crate::display::Resources, settings: &crate::display::Settings) -> PyResult<()> {
        let settings = settings.with_ref(|s| Ok(s.clone()))?;
        let result = resources.with_core(|r| self.with_mut(|ui| Ok(ui.register_native_commands(r, settings))))?;
        // The native registration clears the old menu before any fallible work.
        self.retained.bind(py).set_item(0, py.None())?;
        result.map_err(errors::icf)
    }
    fn select_native(
        &self,
        node: &Bound<'_, PyAny>,
        document: &Document,
        session: &Session,
    ) -> PyResult<()> {
        let node = strict_i32(node)?;
        self.with_mut(|ui| {
            document.with_mut(|d| session.with_mut(|s| Ok(ui.select_native(node, d, s))))
        })?
        .map_err(errors::icf)
    }
    fn select_native_index(
        &self,
        index: &Bound<'_, PyAny>,
        document: &Document,
        session: &Session,
    ) -> PyResult<()> {
        let index = strict_usize(index)?;
        self.with_mut(|ui| {
            document.with_mut(|d| session.with_mut(|s| Ok(ui.select_native_index(index, d, s))))
        })?
        .map_err(errors::icf)
    }
    fn form2_profile(&self, session: &Session) -> PyResult<bool> {
        self.with_mut(|ui| session.with_ref(|s| Ok(ui.form2_profile(s))))
    }
    fn sync2b(&self, document: &Document, session: &Session) -> PyResult<()> {
        self.with_mut(|ui| document.with_mut(|d| session.with_mut(|s| Ok(ui.sync2b(d, s)))))?
            .map_err(errors::icf)
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.retained)
    }
    fn __clear__(&self, py: Python<'_>) -> PyResult<()> {
        self.retained.bind(py).set_item(0, py.None())
    }
}
    };
}
ui_methods!(task_period,set_task_period:i32=>strict_i32; task_enabled,set_task_enabled:bool=>strict_bool; application5b,set_application5b:u8=>strict_u8; profile_ready,set_profile_ready:u8=>strict_u8; operation40,set_operation40:u32=>strict_u32; fc,set_fc:u8=>strict_u8; fd,set_fd:u8=>strict_u8);

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<NativeState>()?;
    m.add_class::<UiState>()
}
