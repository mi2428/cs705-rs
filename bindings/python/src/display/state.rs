//! Live software display state; this module does not deliver native events.
use super::*;
use cs705_core::display as core;
use pyo3::types::PyBytes;
owner!(Grid, core::Grid);
owner!(State, core::State);
macro_rules! grid {
    ($($field:ident:$ty:ty=>$set:ident,$parse:ident);* $(;)?)=>{
        #[pymethods] impl Grid {
            $(#[getter] fn $field(&self)->PyResult<$ty>{self.with_ref(|v|Ok(v.$field))}
            #[setter] fn $set(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let value=$parse(value)?;self.with_mut(|v|{v.$field=value;Ok(())})})*
            fn columns_snapshot(&self)->PyResult<Vec<Column>>{Ok(self.with_ref(|v|Ok(v.columns.clone()))?.into_iter().map(Column::from_core).collect())}
            fn set_columns(&self,values:Vec<Py<Column>>)->PyResult<()>{let values=values.iter().map(|v|v.get().with_ref(|v|Ok(v.clone()))).collect::<PyResult<Vec<_>>>()?;self.with_mut(|v|{v.columns=values;Ok(())})}
            fn rows_snapshot(&self)->PyResult<Vec<Row>>{Ok(self.with_ref(|v|Ok(v.rows.clone()))?.into_iter().map(Row::from_core).collect())}
            fn set_rows(&self,values:Vec<Py<Row>>)->PyResult<()>{let values=values.iter().map(|v|v.get().with_ref(|v|Ok(v.clone()))).collect::<PyResult<Vec<_>>>()?;self.with_mut(|v|{v.rows=values;Ok(())})}
            fn column_positions(&self,horizontal_scroll:&Bound<'_,PyAny>)->PyResult<Vec<i64>>{let scroll=strict_i32(horizontal_scroll)?;self.with_ref(|v|Ok(v.column_positions(scroll)))}
        }
    }
}
grid!(view_id:usize=>set_view_id,strict_usize;sql_mask:u32=>set_sql_mask,strict_u32;frozen_columns:usize=>set_frozen_columns,strict_usize;fixed_origin:usize=>set_fixed_origin,strict_usize;selected_column:usize=>set_selected_column,strict_usize;font_height:i32=>set_font_height,strict_i32;language:u32=>set_language,strict_u32;rebuilds:u64=>set_rebuilds,strict_u64);

/// The immutable source is owned; native borrowed resources are reconstructed
/// only by the existing native parser for the captured display generation.
#[pyclass(module = "cs705.display", frozen)]
pub struct Resources {
    asset: Vec<u8>,
    binding: Option<(u32, bool)>,
}
impl Resources {
    pub fn from_unbound_source(asset: Vec<u8>) -> PyResult<Self> {
        cs705_core::printing::native::Resources::from_pe(&asset).map_err(crate::errors::io)?;
        Ok(Self {
            asset,
            binding: None,
        })
    }
    /// Own the native source with the caller's explicit language binding;
    /// the native Resources selection is private and must not be inferred.
    pub fn from_core(
        value: &cs705_core::printing::native::Resources<'_>,
        language: u32,
        system_japanese: bool,
    ) -> Self {
        Self {
            asset: value.source_pe().to_vec(),
            binding: Some((language, system_japanese)),
        }
    }
    pub fn from_source(asset: Vec<u8>, language: u32, system_japanese: bool) -> PyResult<Self> {
        cs705_core::printing::native::Resources::from_pe(&asset).map_err(crate::errors::io)?;
        Ok(Self {
            asset,
            binding: Some((language, system_japanese)),
        })
    }
    pub fn with_core<R>(
        &self,
        f: impl FnOnce(&cs705_core::printing::native::Resources<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        self.with_ref(f)
    }
    pub fn with_ref<R>(
        &self,
        f: impl FnOnce(&cs705_core::printing::native::Resources<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let resources = cs705_core::printing::native::Resources::from_pe(&self.asset)
            .map_err(crate::errors::io)?;
        let resources = match self.binding {
            None => resources,
            Some((language, system)) => resources.with_language(language, system),
        };
        f(&resources)
    }
}
#[pymethods]
impl Resources {
    #[pyo3(signature=(field,gui_language,system_langid))]
    fn metadata_empty_caption<'py>(
        &self,
        py: Python<'py>,
        field: &Bound<'_, PyAny>,
        gui_language: Option<&Bound<'_, PyAny>>,
        system_langid: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let code = crate::icf::enum_value(field, "cs705.icf.settings.extended", "MetadataField")?;
        let field = match code {
            0 => cs705_core::icf::settings::extended::MetadataField::TransmitPicture,
            1 => cs705_core::icf::settings::extended::MetadataField::ConnectionIpList,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "invalid MetadataField",
                ));
            }
        };
        let gui = gui_language.map(strict_u32).transpose()?;
        let system = system_langid.map(strict_u16).transpose()?;
        let value = self.with_core(|r| {
            Ok(r.metadata_empty_caption(field, gui, system)
                .map_err(crate::errors::io)?
                .to_vec())
        })?;
        Ok(PyBytes::new(py, &value))
    }
    fn tree(
        &self,
        memory_groups: &Bound<'_, PyAny>,
    ) -> PyResult<Vec<crate::printing::native::Factory>> {
        let n = strict_usize(memory_groups)?;
        Ok(self
            .with_core(|r| r.tree(n).map_err(crate::errors::io))?
            .into_iter()
            .map(crate::printing::native::Factory::from_core)
            .collect())
    }
    fn registered(
        &self,
        image: &crate::icf::Icf,
    ) -> PyResult<Vec<crate::printing::native::Factory>> {
        Ok(image
            .with_ref(|image| self.with_core(|r| r.registered(image).map_err(crate::errors::io)))?
            .into_iter()
            .map(crate::printing::native::Factory::from_core)
            .collect())
    }
    fn selector_cells(
        &self,
        resource: &Bound<'_, PyAny>,
    ) -> PyResult<Vec<crate::printing::native::CellNode>> {
        let resource = strict_bytes(resource)?;
        Ok(self
            .with_core(|r| r.selector_cells(resource).map_err(crate::errors::io))?
            .into_iter()
            .map(crate::printing::native::CellNode::from_core)
            .collect())
    }
    #[pyo3(signature=(name,index,gui_language,system_langid))]
    fn resource_line<'py>(
        &self,
        py: Python<'py>,
        name: &Bound<'_, PyAny>,
        index: &Bound<'_, PyAny>,
        gui_language: Option<&Bound<'_, PyAny>>,
        system_langid: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let name = strict_bytes(name)?;
        let index = strict_usize(index)?;
        let gui = gui_language.map(strict_u32).transpose()?;
        let system = system_langid.map(strict_u16).transpose()?;
        let value = self.with_core(|r| {
            Ok(r.resource_line(name, index, gui, system)
                .map_err(crate::errors::io)?
                .to_vec())
        })?;
        Ok(PyBytes::new(py, &value))
    }
    #[pyo3(signature=(resource,selector_key,value,gui_language,system_langid))]
    fn selector_caption<'py>(
        &self,
        py: Python<'py>,
        resource: &Bound<'_, PyAny>,
        selector_key: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
        gui_language: Option<&Bound<'_, PyAny>>,
        system_langid: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let resource = strict_bytes(resource)?;
        let key = strict_u32(selector_key)?;
        let value = strict_i64(value)?;
        let gui = gui_language.map(strict_u32).transpose()?;
        let system = system_langid.map(strict_u16).transpose()?;
        let result = self.with_core(|r| {
            r.selector_caption(resource, key, value, gui, system)
                .map_err(crate::errors::io)
        })?;
        Ok(PyBytes::new(py, &result))
    }
    fn no_print(&self, factory: &crate::printing::native::Factory) -> PyResult<bool> {
        factory
            .with_ref(|factory| self.with_core(|r| r.no_print(factory).map_err(crate::errors::io)))
    }
    fn eligible(
        &self,
        current: &Bound<'_, PyAny>,
        factory: &crate::printing::native::Factory,
        show_split: &Bound<'_, PyAny>,
        views: &Views,
    ) -> PyResult<bool> {
        let current = strict_bytes(current)?;
        let split = strict_u32(show_split)?;
        factory.with_ref(|factory| {
            views.with_mut(|views| {
                self.with_core(|r| {
                    r.eligible(current, factory, split, views)
                        .map_err(crate::errors::io)
                })
            })
        })
    }
    fn commands(
        &self,
        resource: &Bound<'_, PyAny>,
    ) -> PyResult<Vec<crate::printing::native::Command>> {
        let resource = strict_bytes(resource)?;
        Ok(self
            .with_core(|r| r.commands(resource).map_err(crate::errors::io))?
            .into_iter()
            .map(crate::printing::native::Command::from_core)
            .collect())
    }
    fn commands_for_language(
        &self,
        resource: &Bound<'_, PyAny>,
        gui_language: &Bound<'_, PyAny>,
        system_japanese: &Bound<'_, PyAny>,
    ) -> PyResult<Vec<crate::printing::native::Command>> {
        let resource = strict_bytes(resource)?;
        let gui = strict_u32(gui_language)?;
        let system = strict_bool(system_japanese)?;
        Ok(self
            .with_core(|r| {
                r.commands_for_language(resource, gui, system)
                    .map_err(crate::errors::io)
            })?
            .into_iter()
            .map(crate::printing::native::Command::from_core)
            .collect())
    }
    fn schema(
        &self,
        py: Python<'_>,
        factory: &crate::printing::native::Factory,
        indices: &Bound<'_, PyAny>,
        condition: Py<PyAny>,
    ) -> PyResult<crate::printing::Grid> {
        crate::printing::resources::schema(py, self, factory, indices, condition)
    }
    fn instantiate(
        &self,
        py: Python<'_>,
        image: Py<crate::icf::Icf>,
        current: &Bound<'_, PyAny>,
        factory: &crate::printing::native::Factory,
        show_split: &Bound<'_, PyAny>,
        condition: Py<PyAny>,
    ) -> PyResult<crate::printing::Grid> {
        crate::printing::resources::instantiate(
            py, self, image, current, factory, show_split, condition, None,
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Native grid production keeps image, factory, split, condition and paint inputs explicit."
    )]
    fn produce(
        &self,
        py: Python<'_>,
        image: Py<crate::icf::Icf>,
        current: &Bound<'_, PyAny>,
        factory: &crate::printing::native::Factory,
        show_split: &Bound<'_, PyAny>,
        condition: Py<PyAny>,
        display: Py<PyAny>,
    ) -> PyResult<crate::printing::Grid> {
        crate::printing::resources::instantiate(
            py,
            self,
            image,
            current,
            factory,
            show_split,
            condition,
            Some(display),
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Native rendering borrows separate grid, layout, views, job and output owners."
    )]
    fn render(
        &self,
        py: Python<'_>,
        current: &Bound<'_, PyAny>,
        grid: &crate::printing::Grid,
        layout: &crate::printing::Layout,
        views: &Views,
        job: &crate::printing::Job,
        paper: &crate::printing::Paper,
        metrics: &crate::printing::Metrics,
        html: &crate::printing::resources::HtmlBuffer,
        display: Py<PyAny>,
    ) -> PyResult<()> {
        crate::printing::resources::render(
            py, self, current, grid, layout, views, job, paper, metrics, html, display,
        )
    }
    #[staticmethod]
    fn from_pe(pe: &Bound<'_, PyAny>) -> PyResult<Self> {
        Self::from_unbound_source(crate::conversion::strict_bytes(pe)?.to_vec())
    }
    fn with_language(
        &self,
        gui_language: &Bound<'_, PyAny>,
        system_japanese: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            asset: self.asset.clone(),
            binding: Some((
                crate::conversion::strict_u32(gui_language)?,
                crate::conversion::strict_bool(system_japanese)?,
            )),
        })
    }
    fn source_pe<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.asset)
    }
}
pub(crate) fn context(
    py: Python<'_>,
    q: Option<Py<crate::icf::context::Context>>,
) -> PyResult<Option<cs705_core::icf::context::Context>> {
    q.map(|v| v.bind(py).try_borrow()?.with_ref(|context| Ok(*context)))
        .transpose()
}
fn valid_view(state: &core::State) -> PyResult<()> {
    if state
        .current
        .as_ref()
        .is_some_and(|v| v.view_id >= state.views.states.len())
    {
        Err(pyo3::exceptions::PyIndexError::new_err(
            "grid view_id is outside the live view collection",
        ))
    } else {
        Ok(())
    }
}
#[pymethods]
impl State {
    #[new]
    fn new(
        settings: &Settings,
        metrics: &FontMetrics,
        placement: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let settings = settings.with_ref(|v| Ok(v.clone()))?;
        let metrics = metrics.with_ref(|v| Ok(*v))?;
        Ok(Self::from_core(
            core::State::new(settings, metrics, values::rect(placement)?)
                .map_err(crate::errors::io)?,
        ))
    }
    fn settings_snapshot(&self) -> PyResult<Settings> {
        self.with_ref(|v| Ok(Settings::from_core(v.settings.clone())))
    }
    fn set_settings(&self, value: &Settings) -> PyResult<()> {
        let value = value.with_ref(|v| Ok(v.clone()))?;
        self.with_mut(|v| {
            v.settings = value;
            Ok(())
        })
    }
    fn metrics_snapshot(&self) -> PyResult<FontMetrics> {
        self.with_ref(|v| Ok(FontMetrics::from_core(v.metrics)))
    }
    fn set_metrics(&self, value: &FontMetrics) -> PyResult<()> {
        let value = value.with_ref(|v| Ok(*v))?;
        self.with_mut(|v| {
            v.metrics = value;
            Ok(())
        })
    }
    fn toolbar_snapshot(&self) -> PyResult<Toolbar> {
        self.with_ref(|v| Ok(Toolbar::from_core(v.toolbar.clone())))
    }
    fn set_toolbar(&self, value: &Toolbar) -> PyResult<()> {
        let value = value.with_ref(|v| Ok(v.clone()))?;
        self.with_mut(|v| {
            v.toolbar = value;
            Ok(())
        })
    }
    fn current_snapshot(&self) -> PyResult<Option<Grid>> {
        self.with_ref(|v| Ok(v.current.clone().map(Grid::from_core)))
    }
    fn set_current(&self, value: Option<&Grid>) -> PyResult<()> {
        let value = value.map(|v| v.with_ref(|v| Ok(v.clone()))).transpose()?;
        self.with_mut(|v| {
            if value
                .as_ref()
                .is_some_and(|g| g.view_id >= v.views.states.len())
            {
                return Err(pyo3::exceptions::PyIndexError::new_err(
                    "grid view_id outside views",
                ));
            }
            v.current = value;
            Ok(())
        })
    }
    #[getter]
    /// Live projection of the parent's views, including its callback lifetime.
    /// Assigning ordinary Views transfers its owner and invalidates the supplied wrapper.
    fn views(&self) -> Views {
        Views::from_state(self.target.clone())
    }
    #[setter]
    fn set_views(&self, value: &Views) -> PyResult<()> {
        self.with_mut(|v| {
            let incoming = value.take()?;
            if v.current
                .as_ref()
                .is_some_and(|g| g.view_id >= incoming.states.len())
            {
                return Err(pyo3::exceptions::PyIndexError::new_err(
                    "new views would invalidate current grid",
                ));
            }
            v.views = incoming.consume();
            Ok(())
        })
    }
    fn queue_snapshot(&self) -> PyResult<Vec<Queued>> {
        self.with_ref(|v| Ok(v.queue.iter().copied().map(Queued::from_core).collect()))
    }
    fn set_queue(&self, values: Vec<Py<Queued>>) -> PyResult<()> {
        let values = values
            .iter()
            .map(|v| v.get().with_ref(|v| Ok(*v)))
            .collect::<PyResult<std::collections::VecDeque<_>>>()?;
        self.with_mut(|v| {
            v.queue = values;
            Ok(())
        })
    }
    #[getter]
    fn font_changed(&self) -> PyResult<bool> {
        self.with_ref(|v| Ok(v.font_changed))
    }
    #[setter]
    fn set_font_changed(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.with_mut(|v| {
            v.font_changed = value;
            Ok(())
        })
    }
    #[getter]
    fn resource_language(&self) -> PyResult<u32> {
        self.with_ref(|v| Ok(v.resource_language))
    }
    #[setter]
    fn set_resource_language(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u32(value)?;
        self.with_mut(|v| {
            v.resource_language = value;
            Ok(())
        })
    }
    #[getter]
    fn resource_generation(&self) -> PyResult<u64> {
        self.with_ref(|v| Ok(v.resource_generation))
    }
    #[setter]
    fn set_resource_generation(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u64(value)?;
        self.with_mut(|v| {
            v.resource_generation = value;
            Ok(())
        })
    }
    fn resources(&self, asset: &Bound<'_, PyAny>) -> PyResult<Resources> {
        let asset = strict_bytes(asset)?;
        self.with_ref(|v| {
            let resources = v.resources(asset).map_err(crate::errors::io)?;
            Ok(Resources::from_core(
                &resources,
                v.resource_language,
                v.settings.system_japanese(),
            ))
        })
    }
    #[pyo3(signature=(current,kind,group,q))]
    fn open(
        &self,
        py: Python<'_>,
        current: &Bound<'_, PyAny>,
        kind: &Bound<'_, PyAny>,
        group: &Bound<'_, PyAny>,
        q: Option<Py<crate::icf::context::Context>>,
    ) -> PyResult<()> {
        let current = strict_bytes(current)?;
        let kind = views::kind(kind)?;
        let group = strict_i32(group)?;
        let q = context(py, q)?;
        self.with_mut(|v| {
            valid_view(v)?;
            v.open(current, kind, group, q.as_ref())
                .map_err(crate::errors::io)
        })
    }
    #[pyo3(signature=(current,q))]
    fn show(
        &self,
        py: Python<'_>,
        current: &Bound<'_, PyAny>,
        q: Option<Py<crate::icf::context::Context>>,
    ) -> PyResult<()> {
        let current = strict_bytes(current)?;
        let q = context(py, q)?;
        self.with_mut(|v| {
            valid_view(v)?;
            v.show(current, q.as_ref()).map_err(crate::errors::io)
        })
    }
    #[pyo3(signature=(current,q))]
    fn drain(
        &self,
        py: Python<'_>,
        current: &Bound<'_, PyAny>,
        q: Option<Py<crate::icf::context::Context>>,
    ) -> PyResult<()> {
        let current = strict_bytes(current)?;
        let q = context(py, q)?;
        self.with_mut(|v| {
            valid_view(v)?;
            v.drain(current, q.as_ref()).map_err(crate::errors::io)
        })
    }
    #[pyo3(signature=(current,key,event,index,flags,disabled,confirmation))]
    #[expect(
        clippy::too_many_arguments,
        reason = "The native menu event keeps its source, command coordinates, flags and confirmation separate."
    )]
    fn menu(
        &self,
        current: &Bound<'_, PyAny>,
        key: &Bound<'_, PyAny>,
        event: &Bound<'_, PyAny>,
        index: &Bound<'_, PyAny>,
        flags: &Bound<'_, PyAny>,
        disabled: &Bound<'_, PyAny>,
        confirmation: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<(u8, u8)> {
        let current = strict_bytes(current)?;
        let (key, event, index) = (strict_u32(key)?, strict_u32(event)?, strict_u32(index)?);
        let (mut flags, mut disabled) = (strict_u8(flags)?, strict_u8(disabled)?);
        let confirmation = confirmation.map(strict_i32).transpose()?;
        self.with_mut(|v| {
            valid_view(v)?;
            v.menu(
                current,
                key,
                event,
                index,
                &mut flags,
                &mut disabled,
                confirmation,
            )
            .map_err(crate::errors::io)?;
            Ok((flags, disabled))
        })
    }
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Grid>()?;
    m.add_class::<State>()?;
    m.add_class::<Resources>()
}
