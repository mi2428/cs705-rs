//! Owned native templates preserve their private storage and selector state.
use super::*;
use crate::{
    clone::Handle,
    icf::{Icf, context::Context},
};
use pyo3::types::PyTuple;

#[pyclass(module = "cs705.icf.settings.extended", frozen)]
pub struct ChoiceField {
    root: Handle<core::ChoiceField>,
}
impl ChoiceField {
    pub fn from_core(value: core::ChoiceField) -> Self {
        Self {
            root: Handle::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::ChoiceField) -> PyResult<R>) -> PyResult<R> {
        self.root.with(|v| f(v))
    }
    pub fn with_mut<R>(
        &self,
        f: impl FnOnce(&mut core::ChoiceField) -> PyResult<R>,
    ) -> PyResult<R> {
        self.root.with(f)
    }
}
#[pymethods]
impl ChoiceField {
    #[new]
    fn new(
        template: &ChoiceField,
        first_index: &Bound<'_, PyAny>,
        count: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let mut value = template.with_ref(|v| Ok(v.clone()))?;
        value.first_index = strict_usize(first_index)?;
        value.count = strict_usize(count)?;
        Ok(Self::from_core(value))
    }
    #[getter]
    fn first_index(&self) -> PyResult<usize> {
        self.with_ref(|v| Ok(v.first_index))
    }
    #[setter]
    fn set_first_index(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_usize(v)?;
        self.with_mut(|f| {
            f.first_index = v;
            Ok(())
        })
    }
    #[getter]
    fn count(&self) -> PyResult<usize> {
        self.with_ref(|v| Ok(v.count))
    }
    #[setter]
    fn set_count(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_usize(v)?;
        self.with_mut(|f| {
            f.count = v;
            Ok(())
        })
    }
    fn feature_id(&self) -> PyResult<String> {
        self.with_ref(|v| Ok(v.feature_id().to_owned()))
    }
    fn name(&self) -> PyResult<String> {
        self.with_ref(|v| Ok(v.name().to_owned()))
    }
    fn key(&self) -> PyResult<u16> {
        self.with_ref(|v| Ok(v.key()))
    }
    fn domain(&self) -> PyResult<crate::icf::settings::Domain> {
        self.with_ref(|v| Ok(crate::icf::settings::Domain::from_core(v.domain())))
    }
    fn accepts_index(&self, index: &Bound<'_, PyAny>) -> PyResult<bool> {
        let index = strict_usize(index)?;
        self.with_ref(|v| Ok(v.accepts_index(index)))
    }
    fn fixed_value<'py>(
        &self,
        py: Python<'py>,
        index: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let index = strict_usize(index)?;
        self.with_ref(|v| v.fixed_value(index).map_err(crate::errors::icf))?
            .output(py)
    }
    fn read(&self, image: &Icf, index: &Bound<'_, PyAny>) -> PyResult<i64> {
        let index = strict_usize(index)?;
        self.with_ref(|v| image.with_ref(|image| v.read(image, index).map_err(crate::errors::icf)))
    }
    fn edit(
        &self,
        py: Python<'_>,
        image: &Icf,
        index: &Bound<'_, PyAny>,
        input: &ChoiceInput,
    ) -> PyResult<ChoiceEffect> {
        let index = strict_usize(index)?;
        input.with_core(py, |input| {
            self.with_ref(|v| {
                image.with_mut(|image| {
                    v.edit(image, index, input)
                        .map(ChoiceEffect::from_core)
                        .map_err(crate::errors::icf)
                })
            })
        })
    }
}
#[pyfunction]
fn choice_fields() -> Vec<ChoiceField> {
    core::choice_fields()
        .iter()
        .cloned()
        .map(ChoiceField::from_core)
        .collect()
}
#[pyfunction]
fn rf_choice_fields() -> Vec<ChoiceField> {
    core::rf_choice_fields()
        .iter()
        .cloned()
        .map(ChoiceField::from_core)
        .collect()
}
#[pyfunction]
fn key_assignment_fields() -> Vec<ChoiceField> {
    core::key_assignment_fields()
        .iter()
        .cloned()
        .map(ChoiceField::from_core)
        .collect()
}
#[pyfunction]
fn key_assignment_choices(
    image: &Icf,
    name: &str,
    index: &Bound<'_, PyAny>,
    live_q: &Context,
) -> PyResult<Vec<i64>> {
    let index = strict_usize(index)?;
    live_q.with_ref(|q| {
        image.with_ref(|image| {
            core::key_assignment_choices(image, name, index, q).map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
fn edit_key_assignment(
    py: Python<'_>,
    image: &Icf,
    name: &str,
    index: &Bound<'_, PyAny>,
    input: &ChoiceInput,
    live_q: &Context,
) -> PyResult<ChoiceEffect> {
    let index = strict_usize(index)?;
    input.with_core(py, |input| {
        live_q.with_ref(|q| {
            image.with_mut(|image| {
                core::edit_key_assignment(image, name, index, input, q)
                    .map(ChoiceEffect::from_core)
                    .map_err(crate::errors::icf)
            })
        })
    })
}

#[pyclass(module = "cs705.icf.settings.extended")]
pub struct SelectorRuntime {
    system_langid: u16,
    descriptor_8645fc: u32,
    #[pyo3(get, set)]
    q: Py<Context>,
}
impl SelectorRuntime {
    pub fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::SelectorRuntime<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        self.q.try_borrow(py)?.with_ref(|q| {
            f(core::SelectorRuntime {
                system_langid: self.system_langid,
                descriptor_8645fc: self.descriptor_8645fc,
                q,
            })
        })
    }
}
#[pymethods]
impl SelectorRuntime {
    #[new]
    fn new(
        system_langid: &Bound<'_, PyAny>,
        descriptor_8645fc: &Bound<'_, PyAny>,
        q: Py<Context>,
    ) -> PyResult<Self> {
        Ok(Self {
            system_langid: strict_u16(system_langid)?,
            descriptor_8645fc: strict_u32(descriptor_8645fc)?,
            q,
        })
    }
    #[getter]
    fn system_langid(&self) -> u16 {
        self.system_langid
    }
    #[setter]
    fn set_system_langid(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.system_langid = strict_u16(v)?;
        Ok(())
    }
    #[getter]
    fn descriptor_8645fc(&self) -> u32 {
        self.descriptor_8645fc
    }
    #[setter]
    fn set_descriptor_8645fc(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.descriptor_8645fc = strict_u32(v)?;
        Ok(())
    }
}
#[pyclass(module = "cs705.icf.settings.extended", frozen)]
pub struct ConditionalCatalog {
    root: Handle<core::ConditionalCatalog>,
}
impl ConditionalCatalog {
    fn from_core(value: core::ConditionalCatalog) -> Self {
        Self {
            root: Handle::new(value),
        }
    }
}
#[pymethods]
impl ConditionalCatalog {
    #[new]
    fn new(template: &ConditionalCatalog, selector: &str) -> PyResult<Self> {
        let mut value = template.root.with(|v| Ok(v.clone()))?;
        value.selector = std::borrow::Cow::Owned(selector.to_owned());
        Ok(Self::from_core(value))
    }
    #[getter]
    fn selector(&self) -> PyResult<String> {
        self.root.with(|v| Ok(v.selector.to_string()))
    }
    #[setter]
    fn set_selector(&self, v: &str) -> PyResult<()> {
        self.root.with(|field| {
            field.selector = std::borrow::Cow::Owned(v.to_owned());
            Ok(())
        })
    }
    fn values<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        let values = self.root.with(|v| Ok(v.values()))?;
        PyTuple::new(py, values.iter().copied())
    }
    fn field<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.root.with(|v| Ok(v.field()))?.output(py)
    }
    fn edit(&self, py: Python<'_>, image: &Icf, input: &ChoiceInput) -> PyResult<ChoiceEffect> {
        input.with_core(py, |input| {
            self.root.with(|v| {
                image.with_mut(|image| {
                    v.edit(image, input)
                        .map(ChoiceEffect::from_core)
                        .map_err(crate::errors::icf)
                })
            })
        })
    }
}
#[pyfunction]
fn catalog(
    py: Python<'_>,
    field: &Bound<'_, PyAny>,
    runtime: &SelectorRuntime,
) -> PyResult<ConditionalCatalog> {
    let field = core::ConditionalField::input(field)?;
    runtime.with_core(py, |runtime| {
        field
            .catalog(runtime)
            .map(ConditionalCatalog::from_core)
            .map_err(crate::errors::icf)
    })
}
#[pyfunction]
#[pyo3(signature=(field,gui_language,system_langid,q))]
fn catalog_sources(
    field: &Bound<'_, PyAny>,
    gui_language: Option<&Bound<'_, PyAny>>,
    system_langid: Option<&Bound<'_, PyAny>>,
    q: Option<&Context>,
) -> PyResult<ConditionalCatalog> {
    let field = core::ConditionalField::input(field)?;
    let gui = gui_language.map(strict_u32).transpose()?;
    let system = system_langid.map(strict_u16).transpose()?;
    match q {
        None => field
            .catalog_sources(gui, system, None)
            .map(ConditionalCatalog::from_core)
            .map_err(crate::errors::icf),
        Some(q) => q.with_ref(|q| {
            field
                .catalog_sources(gui, system, Some(q))
                .map(ConditionalCatalog::from_core)
                .map_err(crate::errors::icf)
        }),
    }
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ChoiceField>()?;
    m.add_class::<ConditionalCatalog>()?;
    m.add_class::<SelectorRuntime>()?;
    m.add_function(wrap_pyfunction!(choice_fields, m)?)?;
    m.add_function(wrap_pyfunction!(rf_choice_fields, m)?)?;
    m.add_function(wrap_pyfunction!(key_assignment_fields, m)?)?;
    m.add_function(wrap_pyfunction!(key_assignment_choices, m)?)?;
    m.add_function(wrap_pyfunction!(edit_key_assignment, m)?)?;
    super::enums::method(
        m,
        "ConditionalField",
        "catalog",
        wrap_pyfunction!(catalog, m)?.into_any(),
    )?;
    super::enums::method(
        m,
        "ConditionalField",
        "catalog_sources",
        wrap_pyfunction!(catalog_sources, m)?.into_any(),
    )?;
    Ok(())
}
