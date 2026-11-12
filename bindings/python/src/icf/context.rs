//! Live Q context; no implicit registry or OS source.
use crate::{conversion::*, errors};
use cs705_core::icf::context as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

#[pyclass(module = "cs705.icf.context", frozen)]
pub struct Context {
    owner: crate::owner::Owner<core::Context>,
}
impl Context {
    pub fn from_core(inner: core::Context) -> Self {
        Self {
            owner: crate::owner::Owner::new(inner),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::Context) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_ref(f)
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut core::Context) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_mut(f)
    }
    pub fn take(&self) -> PyResult<crate::owner::Taken<'_, core::Context>> {
        self.owner.take()
    }
}
#[pymethods]
impl Context {
    #[staticmethod]
    fn from_bytes(raw: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(core::Context::from_bytes(fixed_bytes(
            raw,
        )?)))
    }
    #[staticmethod]
    fn before_registry() -> Self {
        Self::from_core(core::Context::before_registry())
    }
    fn as_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(|v| Ok(PyBytes::new(py, v.as_bytes())))
    }
    fn fields(&self) -> PyResult<crate::clone::ContextFields> {
        self.with_ref(|v| Ok(crate::clone::ContextFields::from_core(v.fields())))
    }
    fn adopt(&self, fields: &crate::clone::ContextFields) -> PyResult<()> {
        fields.with_ref(|fields| {
            self.with_mut(|v| {
                v.adopt(*fields);
                Ok(())
            })
        })
    }
    fn decode_packed(&self, packed: &Bound<'_, PyAny>) -> PyResult<bool> {
        let packed = strict_u32(packed)?;
        self.with_mut(|v| Ok(v.decode_packed(packed)))
    }
    fn encode_packed(&self) -> PyResult<u32> {
        self.with_ref(|v| Ok(v.encode_packed()))
    }
    fn adopt_registry_packed(&self, packed: &Bound<'_, PyAny>) -> PyResult<bool> {
        let packed = strict_u32(packed)?;
        self.with_mut(|v| Ok(v.adopt_registry_packed(packed)))
    }
    fn selector(&self) -> PyResult<u8> {
        self.with_ref(|v| v.selector().map_err(errors::icf))
    }
    #[pyo3(signature = (system_langid))]
    fn source_selector(&self, system_langid: Option<&Bound<'_, PyAny>>) -> PyResult<u8> {
        let id = system_langid.map(strict_u16).transpose()?;
        self.with_ref(|v| v.source_selector(id).map_err(errors::icf))
    }
    fn effective_f(&self) -> PyResult<u8> {
        self.with_ref(|v| v.effective_f().map_err(errors::icf))
    }
    fn effective_u(&self) -> PyResult<u8> {
        self.with_ref(|v| v.effective_u().map_err(errors::icf))
    }
    fn condition_58bdc(&self) -> PyResult<bool> {
        self.with_ref(|v| v.condition_58bdc().map_err(errors::icf))
    }
    fn condition_58c14(&self) -> PyResult<bool> {
        self.with_ref(|v| v.condition_58c14().map_err(errors::icf))
    }
    fn condition_58c5c(&self) -> PyResult<bool> {
        self.with_ref(|v| v.condition_58c5c().map_err(errors::icf))
    }
    fn condition_58c78(&self) -> PyResult<bool> {
        self.with_ref(|v| Ok(v.condition_58c78()))
    }
}
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "context")?;
    m.add_class::<Context>()?;
    parent.add_submodule(&m)
}
