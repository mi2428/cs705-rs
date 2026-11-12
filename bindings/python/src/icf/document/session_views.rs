//! Field views resolve the same session on each call, never a mutable snapshot.
use super::{Session, Target};
use crate::icf::{
    context::Context,
    frequency::{FrequencyRange, Ranges},
};
use crate::{conversion::*, errors};
use cs705_core::icf::{context, document as core, frequency};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

#[pyclass(frozen, module = "cs705.icf.document")]
pub struct SessionContext {
    pub target: Target<core::Session>,
}
impl SessionContext {
    pub fn with_ref<R>(&self, f: impl FnOnce(&context::Context) -> PyResult<R>) -> PyResult<R> {
        self.target.with_ref(|s| f(&s.context))
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut context::Context) -> PyResult<R>) -> PyResult<R> {
        self.target.with_mut(|s| f(&mut s.context))
    }
}
macro_rules! context_queries {
    ($($name:ident -> $ty:ty;)*) => {
        #[pymethods]
        impl SessionContext {
            fn snapshot(&self) -> PyResult<Context> { self.with_ref(|q| Ok(Context::from_core(*q))) }
            fn fields(&self) -> PyResult<crate::clone::ContextFields> { self.with_ref(|q| Ok(crate::clone::ContextFields::from_core(q.fields()))) }
            fn adopt(&self, fields: &crate::clone::ContextFields) -> PyResult<()> {
                let fields = fields.with_ref(|f| Ok(*f))?;
                self.with_mut(|q| { q.adopt(fields); Ok(()) })
            }
            fn as_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> { self.with_ref(|q| Ok(PyBytes::new(py, q.as_bytes()))) }
            fn decode_packed(&self, packed: &Bound<'_, PyAny>) -> PyResult<bool> { let packed = strict_u32(packed)?; self.with_mut(|q| Ok(q.decode_packed(packed))) }
            fn encode_packed(&self) -> PyResult<u32> { self.with_ref(|q| Ok(q.encode_packed())) }
            fn adopt_registry_packed(&self, packed: &Bound<'_, PyAny>) -> PyResult<bool> { let packed = strict_u32(packed)?; self.with_mut(|q| Ok(q.adopt_registry_packed(packed))) }
            #[pyo3(signature = (system_langid))]
            fn source_selector(&self, system_langid: Option<&Bound<'_, PyAny>>) -> PyResult<u8> { let id = system_langid.map(strict_u16).transpose()?; self.with_ref(|q| Ok(q.source_selector(id)))?.map_err(errors::icf) }
            fn condition_58c78(&self) -> PyResult<bool> { self.with_ref(|q| Ok(q.condition_58c78())) }
            $(fn $name(&self) -> PyResult<$ty> { self.with_ref(|q| Ok(q.$name()))?.map_err(errors::icf) })*
        }
    };
}
context_queries!(selector -> u8; effective_f -> u8; effective_u -> u8; condition_58bdc -> bool; condition_58c14 -> bool; condition_58c5c -> bool;);

#[pyclass(frozen, module = "cs705.icf.document")]
pub struct SessionRanges {
    pub target: Target<core::Session>,
}
impl SessionRanges {
    pub fn with_ref<R>(&self, f: impl FnOnce(&frequency::Ranges) -> PyResult<R>) -> PyResult<R> {
        self.target.with_ref(|s| f(&s.ranges))
    }
    pub fn with_mut<R>(
        &self,
        f: impl FnOnce(&mut frequency::Ranges) -> PyResult<R>,
    ) -> PyResult<R> {
        self.target.with_mut(|s| f(&mut s.ranges))
    }
}
macro_rules! range_queries {
    ($($name:ident -> $ty:ty;)*) => {
        #[pymethods]
        impl SessionRanges {
            fn snapshot(&self) -> PyResult<Ranges> { self.with_ref(|r| Ok(Ranges::from_core(r.clone()))) }
            fn selected(&self) -> PyResult<u32> { self.with_ref(|r| Ok(r.selected())) }
            fn list(&self, which: &Bound<'_, PyAny>) -> PyResult<Vec<FrequencyRange>> {
                let code = crate::icf::enum_value(which, "cs705.icf.frequency", "Container")?;
                let which = frequency::POPULATION_ORDER.get(usize::try_from(code).map_err(|_| pyo3::exceptions::PyValueError::new_err("invalid Container"))?).copied().ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid Container"))?;
                self.with_ref(|r| Ok(r.list(which).iter().copied().map(|inner| FrequencyRange { inner }).collect()))
            }
            fn is_complete(&self) -> PyResult<bool> { self.with_ref(|r| Ok(r.is_complete())) }
            fn source_q<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyBytes>>> { self.with_ref(|r| Ok(r.source_q().map(|q| PyBytes::new(py, q)))) }
            #[pyo3(signature = (selected, q, system_langid))]
            fn regenerate(&self, selected: &Bound<'_, PyAny>, q: &Context, system_langid: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
                let selected = strict_u32(selected)?; let id = system_langid.map(strict_u16).transpose()?;
                let q = q.with_ref(|q| Ok(*q))?;
                self.with_mut(|r| Ok(r.regenerate(selected, &q, id)))?.map_err(errors::icf)
            }
            #[pyo3(signature = (system_langid))]
            fn default_source_selector(&self, system_langid: Option<&Bound<'_, PyAny>>) -> PyResult<usize> { let id = system_langid.map(strict_u16).transpose()?; self.with_ref(|r| Ok(r.default_source_selector(id)))?.map_err(errors::icf) }
            fn eb64(&self, q: &Context, frequency: &Bound<'_, PyAny>) -> PyResult<bool> { let f = strict_u32(frequency)?; let q = q.with_ref(|q| Ok(*q))?; self.with_ref(|r| Ok(r.eb64(&q, f))) }
            fn f32c(&self, q: &Context, frequency: &Bound<'_, PyAny>) -> PyResult<bool> { let f = strict_u32(frequency)?; let q = q.with_ref(|q| Ok(*q))?; self.with_ref(|r| Ok(r.f32c(&q, f))) }
            fn ed3c(&self, q: &Context, frequency: &Bound<'_, PyAny>) -> PyResult<bool> { let f = strict_u32(frequency)?; let q = q.with_ref(|q| Ok(*q))?; self.with_ref(|r| Ok(r.ed3c(&q, f))) }
            fn ecdc(&self, q: &Context, frequency: &Bound<'_, PyAny>) -> PyResult<bool> { let f = strict_u32(frequency)?; let q = q.with_ref(|q| Ok(*q))?; self.with_ref(|r| Ok(r.ecdc(&q, f))) }
            fn edf0(&self, kind: &Bound<'_, PyAny>, frequency: &Bound<'_, PyAny>) -> PyResult<i32> { let kind = strict_u32(kind)?; let f = strict_u32(frequency)?; self.with_ref(|r| Ok(r.edf0(kind, f))) }
            fn f760(&self, kind: &Bound<'_, PyAny>, index: &Bound<'_, PyAny>, frequency: &Bound<'_, PyAny>) -> PyResult<FrequencyRange> { let kind = strict_u32(kind)?; let index = strict_i32(index)?; let f = strict_u32(frequency)?; self.with_ref(|r| Ok(FrequencyRange { inner: r.f760(kind, index, f) })) }
            fn f5c4(&self, index: &Bound<'_, PyAny>, frequency: &Bound<'_, PyAny>) -> PyResult<FrequencyRange> { let index = strict_i32(index)?; let f = strict_u32(frequency)?; self.with_ref(|r| Ok(FrequencyRange { inner: r.f5c4(index, f) })) }
            $(fn $name(&self, frequency: &Bound<'_, PyAny>) -> PyResult<$ty> { let f = strict_u32(frequency)?; self.with_ref(|r| Ok(r.$name(f))) })*
        }
    };
}
range_queries!(eb98 -> bool; ed70 -> u32; eecc -> u32; ef4c -> u32; f094 -> u32; f014 -> i32; efcc -> i32;);

impl Session {
    pub fn context_view(&self) -> SessionContext {
        SessionContext {
            target: self.target.clone(),
        }
    }
    pub fn ranges_view(&self) -> SessionRanges {
        SessionRanges {
            target: self.target.clone(),
        }
    }
}
