//! Owned metadata keeps native private storage state and projects nested fields.
use super::{ControlState, Effect, Value};
use crate::{clone::Handle, conversion::*, errors};
use cs705_core::icf::settings as core;
use pyo3::prelude::*;
use std::borrow::Cow;

#[pyclass(module = "cs705.icf.settings", frozen)]
pub struct Domain {
    root: Handle<core::Domain>,
}
impl Domain {
    pub fn from_core(value: core::Domain) -> Self {
        Self {
            root: Handle::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::Domain) -> PyResult<R>) -> PyResult<R> {
        self.root.with(|value| f(value))
    }
}
#[pymethods]
impl Domain {
    #[new]
    fn new(
        min: &Bound<'_, PyAny>,
        max: &Bound<'_, PyAny>,
        step: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self::from_core(core::Domain {
            min: strict_i64(min)?,
            max: strict_i64(max)?,
            step: strict_i64(step)?,
        }))
    }
    #[getter]
    fn min(&self) -> PyResult<i64> {
        self.with_ref(|v| Ok(v.min))
    }
    #[setter]
    fn set_min(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_i64(v)?;
        self.root.with(|d| {
            d.min = v;
            Ok(())
        })
    }
    #[getter]
    fn max(&self) -> PyResult<i64> {
        self.with_ref(|v| Ok(v.max))
    }
    #[setter]
    fn set_max(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_i64(v)?;
        self.root.with(|d| {
            d.max = v;
            Ok(())
        })
    }
    #[getter]
    fn step(&self) -> PyResult<i64> {
        self.with_ref(|v| Ok(v.step))
    }
    #[setter]
    fn set_step(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_i64(v)?;
        self.root.with(|d| {
            d.step = v;
            Ok(())
        })
    }
}

#[pyclass(module = "cs705.icf.settings", frozen)]
pub struct Kind {
    root: Handle<core::Kind>,
}
impl Kind {
    pub fn from_core(value: core::Kind) -> Self {
        Self {
            root: Handle::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::Kind) -> PyResult<R>) -> PyResult<R> {
        self.root.with(|value| f(value))
    }
}
#[pymethods]
#[allow(non_snake_case)]
impl Kind {
    #[classattr]
    fn Selector() -> Self {
        Self::from_core(core::Kind::Selector)
    }
    #[staticmethod]
    fn Integer(unit: &str) -> Self {
        Self::from_core(core::Kind::Integer(Cow::Owned(unit.to_owned())))
    }
    #[getter]
    fn kind(&self) -> PyResult<&'static str> {
        self.with_ref(|v| {
            Ok(match v {
                core::Kind::Selector => "Selector",
                core::Kind::Integer(_) => "Integer",
            })
        })
    }
    #[getter]
    fn unit(&self) -> PyResult<Option<String>> {
        self.with_ref(|v| {
            Ok(match v {
                core::Kind::Selector => None,
                core::Kind::Integer(unit) => Some(unit.to_string()),
            })
        })
    }
}

#[pyclass(module = "cs705.icf.settings", frozen)]
pub struct Setting {
    root: Handle<core::Setting>,
}
impl Setting {
    pub fn from_core(value: core::Setting) -> Self {
        Self {
            root: Handle::new(value),
        }
    }
    pub(crate) fn from_projection(root: Handle<core::Setting>) -> Self {
        Self { root }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::Setting) -> PyResult<R>) -> PyResult<R> {
        self.root.with(|value| f(value))
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut core::Setting) -> PyResult<R>) -> PyResult<R> {
        self.root.with(f)
    }
}
#[pymethods]
impl Setting {
    /// A native template preserves private storage coordinates; all public metadata is explicit.
    #[new]
    #[expect(
        clippy::too_many_arguments,
        reason = "The template retains private coordinates while all seven public metadata replacements remain explicit."
    )]
    fn new(
        template: &Setting,
        name: &str,
        feature_id: &str,
        key: &Bound<'_, PyAny>,
        selection_key: &Bound<'_, PyAny>,
        domain: &Domain,
        count: &Bound<'_, PyAny>,
        kind: &Kind,
    ) -> PyResult<Self> {
        let mut value = template.with_ref(|v| Ok(v.clone()))?;
        value.name = Cow::Owned(name.to_owned());
        value.feature_id = Cow::Owned(feature_id.to_owned());
        value.key = strict_u16(key)?;
        value.selection_key = strict_u16(selection_key)?;
        value.domain = domain.with_ref(|v| Ok(*v))?;
        value.count = strict_usize(count)?;
        value.kind = kind.with_ref(|v| Ok(v.clone()))?;
        Ok(Self::from_core(value))
    }
    #[getter]
    fn name(&self) -> PyResult<String> {
        self.with_ref(|v| Ok(v.name.to_string()))
    }
    #[setter]
    fn set_name(&self, v: &str) -> PyResult<()> {
        self.with_mut(|d| {
            d.name = Cow::Owned(v.to_owned());
            Ok(())
        })
    }
    #[getter]
    fn feature_id(&self) -> PyResult<String> {
        self.with_ref(|v| Ok(v.feature_id.to_string()))
    }
    #[setter]
    fn set_feature_id(&self, v: &str) -> PyResult<()> {
        self.with_mut(|d| {
            d.feature_id = Cow::Owned(v.to_owned());
            Ok(())
        })
    }
    #[getter]
    fn key(&self) -> PyResult<u16> {
        self.with_ref(|v| Ok(v.key))
    }
    #[setter]
    fn set_key(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_u16(v)?;
        self.with_mut(|d| {
            d.key = v;
            Ok(())
        })
    }
    #[getter]
    fn selection_key(&self) -> PyResult<u16> {
        self.with_ref(|v| Ok(v.selection_key))
    }
    #[setter]
    fn set_selection_key(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_u16(v)?;
        self.with_mut(|d| {
            d.selection_key = v;
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
        self.with_mut(|d| {
            d.count = v;
            Ok(())
        })
    }
    #[getter]
    fn domain(&self) -> PyResult<Domain> {
        self.with_ref(|_| Ok(()))?;
        Ok(Domain {
            root: self.root.project(|v| Ok(&mut v.domain)),
        })
    }
    #[setter]
    fn set_domain(&self, v: &Domain) -> PyResult<()> {
        let v = v.with_ref(|v| Ok(*v))?;
        self.with_mut(|d| {
            d.domain = v;
            Ok(())
        })
    }
    #[getter]
    fn kind(&self) -> PyResult<Kind> {
        self.with_ref(|_| Ok(()))?;
        Ok(Kind {
            root: self.root.project(|v| Ok(&mut v.kind)),
        })
    }
    #[setter]
    fn set_kind(&self, v: &Kind) -> PyResult<()> {
        let v = v.with_ref(|v| Ok(v.clone()))?;
        self.with_mut(|d| {
            d.kind = v;
            Ok(())
        })
    }
    fn read(&self, image: &super::super::Icf, index: &Bound<'_, PyAny>) -> PyResult<Value> {
        let index = strict_usize(index)?;
        self.with_ref(|v| {
            image.with_ref(|i| v.read(i, index).map(Value::from_core).map_err(errors::icf))
        })
    }
    fn read_numeric_raw(
        &self,
        image: &super::super::Icf,
        index: &Bound<'_, PyAny>,
    ) -> PyResult<i32> {
        let index = strict_usize(index)?;
        self.with_ref(|v| image.with_ref(|i| v.read_numeric_raw(i, index).map_err(errors::icf)))
    }
    fn store_numeric_raw(
        &self,
        image: &super::super::Icf,
        index: &Bound<'_, PyAny>,
        raw: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let index = strict_usize(index)?;
        let raw = strict_i32(raw)?;
        self.with_ref(|v| {
            image.with_mut(|i| v.store_numeric_raw(i, index, raw).map_err(errors::icf))
        })
    }
    fn write(
        &self,
        image: &super::super::Icf,
        index: &Bound<'_, PyAny>,
        value: &Value,
    ) -> PyResult<()> {
        let index = strict_usize(index)?;
        self.with_ref(|v| image.with_mut(|i| v.write(i, index, &value.inner).map_err(errors::icf)))
    }
    fn select(
        &self,
        image: &super::super::Icf,
        index: &Bound<'_, PyAny>,
        selection: &super::Selection,
    ) -> PyResult<Effect> {
        let index = strict_usize(index)?;
        self.with_ref(|v| {
            image.with_mut(|i| {
                v.select(i, index, selection.inner)
                    .map(Effect::from_core)
                    .map_err(errors::icf)
            })
        })
    }
    fn cycle(
        &self,
        image: &super::super::Icf,
        index: &Bound<'_, PyAny>,
        step: &Bound<'_, PyAny>,
    ) -> PyResult<Effect> {
        use super::super::values::Input;
        let index = strict_usize(index)?;
        let step = core::Step::input(step)?;
        self.with_ref(|v| {
            image.with_mut(|i| {
                v.cycle(i, index, step)
                    .map(Effect::from_core)
                    .map_err(errors::icf)
            })
        })
    }
    fn choices(&self, image: &super::super::Icf, index: &Bound<'_, PyAny>) -> PyResult<Vec<i64>> {
        let index = strict_usize(index)?;
        self.with_ref(|v| image.with_ref(|i| v.choices(i, index).map_err(errors::icf)))
    }
    fn control_state(
        &self,
        image: &super::super::Icf,
        index: &Bound<'_, PyAny>,
        marker_flag: &Bound<'_, PyAny>,
    ) -> PyResult<ControlState> {
        let index = strict_usize(index)?;
        let marker = strict_bool(marker_flag)?;
        self.with_ref(|v| {
            image.with_ref(|i| {
                v.control_state(i, index, marker)
                    .map(ControlState::from_core)
                    .map_err(errors::icf)
            })
        })
    }
}

#[pyfunction]
fn find(name: &str) -> PyResult<Setting> {
    core::find(name)
        .cloned()
        .map(Setting::from_core)
        .map_err(errors::icf)
}
#[pyfunction]
fn catalog() -> Vec<Setting> {
    core::catalog()
        .iter()
        .cloned()
        .map(Setting::from_core)
        .collect()
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(find, m)?)?;
    m.add_function(wrap_pyfunction!(catalog, m)?)
}
