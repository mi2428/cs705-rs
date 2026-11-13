//! Captured B range owner and live Q sources remain distinct.
use super::context::Context;
use crate::{conversion::*, errors, owner::Owner};
use cs705_core::icf::frequency as core;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};
use pyo3::{PyTraverseError, PyVisit};

#[pyclass(module = "cs705.icf.frequency")]
pub struct FrequencyContext {
    common_kind: i32,
    pub l30: Option<Py<PyList>>,
    pub l58: Option<Py<PyList>>,
    pub l120: Option<Py<PyList>>,
    source: Option<Py<Ranges>>,
    borrowed: [bool; 3],
}
/// Readonly container projection retaining the canonical B owner.
#[pyclass(module = "cs705.icf.frequency", frozen)]
pub struct RangeSource {
    parent: Py<Ranges>,
    which: core::Container,
}
#[pymethods]
impl RangeSource {
    fn __len__(&self) -> PyResult<usize> {
        self.parent.get().with_ref(|b| Ok(b.list(self.which).len()))
    }
    fn items_snapshot(&self) -> PyResult<Vec<FrequencyRange>> {
        self.parent.get().with_ref(|b| {
            Ok(b.list(self.which)
                .iter()
                .map(|v| FrequencyRange { inner: *v })
                .collect())
        })
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.parent)
    }
}
impl FrequencyContext {
    pub fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::FrequencyContext<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let read = |values: &Option<Py<PyList>>| -> PyResult<Option<Vec<core::FrequencyRange>>> {
            values
                .as_ref()
                .map(|values| {
                    values
                        .bind(py)
                        .iter()
                        .map(|value| Ok(value.extract::<PyRef<'_, FrequencyRange>>()?.inner))
                        .collect::<PyResult<Vec<_>>>()
                })
                .transpose()
        };
        let l30 = read(&self.l30)?;
        let l58 = read(&self.l58)?;
        let l120 = read(&self.l120)?;
        let manual = core::FrequencyContext {
            common_kind: self.common_kind,
            l30: l30.as_deref(),
            l58: l58.as_deref(),
            l120: l120.as_deref(),
        };
        match &self.source {
            None => f(manual),
            Some(source) => source.get().with_ref(|b| {
                let mut context = b.frequency_context(self.common_kind);
                if !self.borrowed[0] {
                    context.l30 = manual.l30;
                }
                if !self.borrowed[1] {
                    context.l58 = manual.l58;
                }
                if !self.borrowed[2] {
                    context.l120 = manual.l120;
                }
                f(context)
            }),
        }
    }
}
macro_rules! context_lists {
    ([$($name:ident,$set:ident,$index:literal,$which:ident;)*] methods {$($methods:tt)*}) => {
        #[pymethods] impl FrequencyContext {
            $($methods)*
            $(#[getter] fn $name(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
                if self.borrowed[$index] {
                    let parent = self.source.as_ref().ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("missing B owner"))?.clone_ref(py);
                    Ok(Py::new(py, RangeSource { parent, which: core::Container::$which })?.into_any())
                } else { Ok(self.$name.as_ref().map(|v| v.clone_ref(py).into_any()).unwrap_or_else(|| py.None())) }
            }
            #[setter] fn $set(&mut self, value: Option<Py<PyList>>) {
                self.$name = value;
                self.borrowed[$index] = false;
            })*
        }
    }
}
context_lists! { [l30,set_l30,0,L30; l58,set_l58,1,L58; l120,set_l120,2,L120;] methods {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.l30)?;
        visit.call(&self.l58)?;
        visit.call(&self.l120)?;
        visit.call(&self.source)
    }
    #[new]
    #[pyo3(signature=(common_kind,l30,l58,l120))]
    fn new(
        common_kind: &Bound<'_, PyAny>,
        l30: Option<Py<PyList>>,
        l58: Option<Py<PyList>>,
        l120: Option<Py<PyList>>,
    ) -> PyResult<Self> {
        Ok(Self {
            common_kind: strict_i32(common_kind)?,
            l30,
            l58,
            l120,
            source: None,
            borrowed: [false; 3],
        })
    }
    #[getter]
    fn common_kind(&self) -> i32 {
        self.common_kind
    }
    #[setter]
    fn set_common_kind(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.common_kind = strict_i32(value)?;
        Ok(())
    }
}}

#[pyfunction]
#[pyo3(signature=(context,frequency))]
fn common_bad(
    py: Python<'_>,
    context: Option<&FrequencyContext>,
    frequency: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let frequency = strict_u32(frequency)?;
    match context {
        Some(context) => context.with_core(py, |context| {
            core::common_bad(Some(&context), frequency).map_err(errors::icf)
        }),
        None => core::common_bad(None, frequency).map_err(errors::icf),
    }
}
#[pyfunction]
#[pyo3(signature=(holder,context))]
fn previous_channel_bad(
    py: Python<'_>,
    holder: &super::record::DecodedRecord,
    context: Option<&FrequencyContext>,
) -> PyResult<bool> {
    match context {
        Some(context) => context.with_core(py, |context| {
            holder.with_ref(|h| core::previous_channel_bad(h, Some(&context)).map_err(errors::icf))
        }),
        None => holder.with_ref(|h| core::previous_channel_bad(h, None).map_err(errors::icf)),
    }
}

#[pyclass(module = "cs705.icf.frequency")]
#[derive(Clone, Copy)]
pub struct FrequencyRange {
    pub inner: core::FrequencyRange,
}
#[pymethods]
impl FrequencyRange {
    #[new]
    fn new(
        low: &Bound<'_, PyAny>,
        high: &Bound<'_, PyAny>,
        flag: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::FrequencyRange {
                low: strict_u32(low)?,
                high: strict_u32(high)?,
                flag: strict_u8(flag)?,
            },
        })
    }
    #[getter]
    fn low(&self) -> u32 {
        self.inner.low
    }
    #[setter]
    fn set_low(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.low = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn high(&self) -> u32 {
        self.inner.high
    }
    #[setter]
    fn set_high(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.high = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn flag(&self) -> u8 {
        self.inner.flag
    }
    #[setter]
    fn set_flag(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.flag = strict_u8(value)?;
        Ok(())
    }
}

#[pyclass(module = "cs705.icf.frequency", frozen)]
pub struct Ranges {
    owner: Owner<core::Ranges>,
}
impl Ranges {
    pub fn take(&self) -> PyResult<crate::owner::Taken<'_, core::Ranges>> {
        self.owner.take()
    }
    pub fn from_core(value: core::Ranges) -> Self {
        Self {
            owner: Owner::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::Ranges) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_ref(f)
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut core::Ranges) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_mut(f)
    }
}
macro_rules! range_methods {
    ($($method:ident -> $out:ty;)*) => {
        #[pymethods]
        impl Ranges {
            #[staticmethod]
            fn unpopulated() -> Self { Self::from_core(core::Ranges::unpopulated()) }
            #[staticmethod]
            #[pyo3(signature = (q, system_langid))]
            fn generate(q: &Context, system_langid: Option<&Bound<'_, PyAny>>) -> PyResult<Self> { let id=system_langid.map(strict_u16).transpose()?;q.with_ref(|q|core::Ranges::generate(q,id).map(Self::from_core).map_err(errors::icf)) }
            #[pyo3(signature = (selected, q, system_langid))]
            fn regenerate(&self, selected: &Bound<'_, PyAny>, q: &Context, system_langid: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
                let selected = strict_u32(selected)?; let system_langid = system_langid.map(strict_u16).transpose()?;
                q.with_ref(|q|self.with_mut(|ranges| ranges.regenerate(selected, q, system_langid).map_err(errors::icf)))
            }
            #[pyo3(signature = (selected, q, system_langid, before_clear))]
            fn regenerate_with(&self, py: Python<'_>, selected: &Bound<'_, PyAny>, q: &Context, system_langid: Option<&Bound<'_, PyAny>>, before_clear: Py<PyAny>) -> PyResult<()> {
                let selected = strict_u32(selected)?; let id = system_langid.map(strict_u16).transpose()?;
                let mut failures = crate::callbacks::CallbackError::default();
                let q=q.take()?;
                let result = self.with_mut(|ranges| ranges.regenerate_with(selected, &q, id, |which| {
                    let result = (|| {
                        let which = super::enum_result(py, "cs705.icf.frequency", "Container", which as i64)?;
                        super::document::adapters::require_none(before_clear.bind(py).call1((which,))?)?;
                        Ok(())
                    })();
                    failures.capture(result).map_err(|_| cs705_core::icf::Error::Operation("Python range callback failed".into()))
                }).map_err(errors::icf));
                failures.finish(py, result)
            }
            fn list(&self, which: &Bound<'_, PyAny>) -> PyResult<Vec<FrequencyRange>> {
                let code = super::enum_value(which, "cs705.icf.frequency", "Container")?;
                let which = *core::POPULATION_ORDER.get(usize::try_from(code).map_err(|_| pyo3::exceptions::PyValueError::new_err("invalid Container"))?).ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid Container"))?;
                self.with_ref(|r| Ok(r.list(which).iter().map(|inner| FrequencyRange { inner: *inner }).collect()))
            }
            fn db98(slf: PyRef<'_, Self>, frequency: &Bound<'_, PyAny>) -> PyResult<Py<Self>> { let frequency = strict_u32(frequency)?; slf.with_ref(|r| { r.db98(frequency); Ok(()) })?; Ok(slf.into()) }
            fn selected(&self) -> PyResult<u32> { self.with_ref(|ranges| Ok(ranges.selected())) }
            fn frequency_context(slf: Py<Self>, py: Python<'_>, live_kind: &Bound<'_, PyAny>) -> PyResult<FrequencyContext> {
                let live_kind = strict_i32(live_kind)?;
                slf.get().with_ref(|b| { let _ = b.frequency_context(live_kind); Ok(()) })?;
                Ok(FrequencyContext { common_kind: live_kind, l30: None, l58: None, l120: None, source: Some(slf.clone_ref(py)), borrowed: [true; 3] })
            }
            fn is_complete(&self) -> PyResult<bool> { self.with_ref(|ranges| Ok(ranges.is_complete())) }
            fn source_q<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyBytes>>> { self.with_ref(|ranges| Ok(ranges.source_q().map(|q| PyBytes::new(py, q)))) }
            #[pyo3(signature = (system_langid))]
            fn default_source_selector(&self, system_langid: Option<&Bound<'_, PyAny>>) -> PyResult<usize> { let id = system_langid.map(strict_u16).transpose()?; self.with_ref(|ranges| ranges.default_source_selector(id).map_err(errors::icf)) }
            fn eb64(&self, live_q: &Context, frequency: &Bound<'_, PyAny>) -> PyResult<bool> { let frequency = strict_u32(frequency)?;live_q.with_ref(|q| self.with_ref(|r| Ok(r.eb64(q, frequency)))) }
            fn f32c(&self, live_q: &Context, frequency: &Bound<'_, PyAny>) -> PyResult<bool> { let frequency = strict_u32(frequency)?;live_q.with_ref(|q| self.with_ref(|r| Ok(r.f32c(q, frequency)))) }
            fn ed3c(&self, live_q: &Context, frequency: &Bound<'_, PyAny>) -> PyResult<bool> { let frequency = strict_u32(frequency)?;live_q.with_ref(|q| self.with_ref(|r| Ok(r.ed3c(q, frequency)))) }
            fn ecdc(&self, live_q: &Context, frequency: &Bound<'_, PyAny>) -> PyResult<bool> { let frequency = strict_u32(frequency)?;live_q.with_ref(|q| self.with_ref(|r| Ok(r.ecdc(q, frequency)))) }
            $(fn $method(&self, frequency: &Bound<'_, PyAny>) -> PyResult<$out> { let frequency = strict_u32(frequency)?; self.with_ref(|r| Ok(r.$method(frequency))) })*
            fn edf0(&self, kind: &Bound<'_, PyAny>, frequency: &Bound<'_, PyAny>) -> PyResult<i32> { let kind = strict_u32(kind)?; let frequency = strict_u32(frequency)?; self.with_ref(|r| Ok(r.edf0(kind, frequency))) }
            fn f760(&self, kind: &Bound<'_, PyAny>, index: &Bound<'_, PyAny>, frequency: &Bound<'_, PyAny>) -> PyResult<FrequencyRange> { let kind = strict_u32(kind)?; let index = strict_i32(index)?; let frequency = strict_u32(frequency)?; self.with_ref(|r| Ok(FrequencyRange { inner: r.f760(kind, index, frequency) })) }
            fn f5c4(&self, index: &Bound<'_, PyAny>, frequency: &Bound<'_, PyAny>) -> PyResult<FrequencyRange> { let index = strict_i32(index)?; let frequency = strict_u32(frequency)?; self.with_ref(|r| Ok(FrequencyRange { inner: r.f5c4(index, frequency) })) }
            fn frequency_input(&self,kind:&Bound<'_,PyAny>,candidate:&Bound<'_,PyAny>)->PyResult<super::special::FrequencyInput>{let kind=strict_u32(kind)?;let candidate=strict_u32(candidate)?;self.with_ref(|r|r.frequency_input(kind,candidate).map(super::special::FrequencyInput::from_core).map_err(errors::icf))}
            fn frequency_text_input(&self,kind:&Bound<'_,PyAny>,text:&Bound<'_,PyAny>,point:&Bound<'_,PyAny>)->PyResult<super::special::FrequencyInput>{let kind=strict_u32(kind)?;let text=strict_bytes(text)?;let point=strict_u8(point)?;self.with_ref(|r|r.frequency_text_input(kind,text,point).map(super::special::FrequencyInput::from_core).map_err(errors::icf))}
        }
    }
}
range_methods!(eb98 -> bool; ed70 -> u32; eecc -> u32; ef4c -> u32; f094 -> u32; f014 -> i32; efcc -> i32;);

#[pyfunction]
fn relationship(r: &FrequencyRange, e: &FrequencyRange) -> u8 {
    core::relationship(r.inner, e.inner)
}

// These pure native operations edit a value vector; no Python callbacks run
// between reading the list and replacing its contents, including error prefixes.
fn edit_list<R>(
    list: &Bound<'_, PyList>,
    edit: impl FnOnce(&mut Vec<core::FrequencyRange>) -> Result<R, cs705_core::icf::Error>,
) -> PyResult<R> {
    let mut values = list
        .iter()
        .map(|value| Ok(value.extract::<PyRef<'_, FrequencyRange>>()?.inner))
        .collect::<PyResult<Vec<_>>>()?;
    let before = values.clone();
    let result = edit(&mut values).map_err(errors::icf);
    if values == before {
        return result;
    }
    let values = values
        .into_iter()
        .map(|inner| Py::new(list.py(), FrequencyRange { inner }))
        .collect::<PyResult<Vec<_>>>()?;
    let replacement = PyList::new(list.py(), values)?;
    list.set_slice(0, list.len(), &replacement)?;
    result
}
#[pyfunction]
fn sort_native(list: &Bound<'_, PyList>) -> PyResult<()> {
    edit_list(list, |values| {
        core::sort_native(values);
        Ok(())
    })
}
#[pyfunction]
fn add(list: &Bound<'_, PyList>, pending: &FrequencyRange) -> PyResult<bool> {
    let pending = pending.inner;
    edit_list(list, |values| core::add(values, pending))
}
#[pyfunction]
fn cut(list: &Bound<'_, PyList>, input: &FrequencyRange) -> PyResult<bool> {
    let input = input.inner;
    edit_list(list, |values| core::cut(values, input))
}
#[pyfunction]
fn join_adjacent(list: &Bound<'_, PyList>) -> PyResult<()> {
    edit_list(list, |values| {
        core::join_adjacent(values);
        Ok(())
    })
}
#[pyfunction]
#[pyo3(signature = (list, input))]
fn optional(list: &Bound<'_, PyList>, input: Option<PyRef<'_, FrequencyRange>>) -> PyResult<()> {
    let input = input.map(|v| v.inner);
    edit_list(list, |values| core::optional(values, input))
}
#[pyfunction]
fn intersection(a: &FrequencyRange, b: &FrequencyRange) -> FrequencyRange {
    FrequencyRange {
        inner: core::intersection(a.inner, b.inner),
    }
}
#[pyfunction]
fn offset_for_source(selected: &Bound<'_, PyAny>, class_: &Bound<'_, PyAny>) -> PyResult<u32> {
    core::offset_for_source(strict_u8(selected)?, strict_i32(class_)?).map_err(errors::icf)
}
#[pyfunction]
#[pyo3(signature = (ranges, system_langid, class_))]
fn f20c(
    ranges: &Ranges,
    system_langid: Option<&Bound<'_, PyAny>>,
    class_: &Bound<'_, PyAny>,
) -> PyResult<u32> {
    let id = system_langid.map(strict_u16).transpose()?;
    let class = strict_i32(class_)?;
    ranges.with_ref(|r| core::f20c(r, id, class).map_err(errors::icf))
}

#[pyfunction]
fn channel_frequency_bad(
    holder: &super::record::DecodedRecord,
    ranges: &Ranges,
    live_q: &Context,
) -> PyResult<bool> {
    holder.with_ref(|h| {
        live_q.with_ref(|q| {
            ranges.with_ref(|r| core::channel_frequency_bad(h, r, q).map_err(errors::icf))
        })
    })
}
#[pyfunction]
fn channel_bad(
    holder: &super::record::DecodedRecord,
    ranges: &Ranges,
    live_q: &Context,
    broad: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let broad = strict_bool(broad)?;
    holder.with_ref(|h| {
        live_q.with_ref(|q| {
            ranges.with_ref(|r| core::channel_bad(h, r, q, broad).map_err(errors::icf))
        })
    })
}
#[pyfunction]
#[pyo3(signature = (holder, ranges, live_q, numeric_only, system_langid))]
fn auto_repeater_bad(
    holder: &super::record::DecodedRecord,
    ranges: &Ranges,
    live_q: &Context,
    numeric_only: &Bound<'_, PyAny>,
    system_langid: Option<&Bound<'_, PyAny>>,
) -> PyResult<bool> {
    let numeric_only = strict_bool(numeric_only)?;
    let id = system_langid.map(strict_u16).transpose()?;
    holder.with_ref(|h| {
        live_q.with_ref(|q| {
            ranges.with_ref(|r| {
                core::auto_repeater_bad(h, r, q, numeric_only, id).map_err(errors::icf)
            })
        })
    })
}
#[pyfunction]
#[pyo3(signature = (holder, ranges, live_q, low, numeric_only, system_langid))]
fn auto_repeater_edge_bad(
    holder: &super::record::DecodedRecord,
    ranges: &Ranges,
    live_q: &Context,
    low: &Bound<'_, PyAny>,
    numeric_only: &Bound<'_, PyAny>,
    system_langid: Option<&Bound<'_, PyAny>>,
) -> PyResult<bool> {
    let low = strict_bool(low)?;
    let numeric_only = strict_bool(numeric_only)?;
    let id = system_langid.map(strict_u16).transpose()?;
    holder.with_ref(|h| {
        live_q.with_ref(|q| {
            ranges.with_ref(|r| {
                core::auto_repeater_edge_bad(h, r, q, low, numeric_only, id).map_err(errors::icf)
            })
        })
    })
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "frequency")?;
    m.add_class::<Ranges>()?;
    m.add_class::<FrequencyContext>()?;
    m.add_class::<FrequencyRange>()?;
    m.add_class::<RangeSource>()?;
    super::add_enum(
        &m,
        "Container",
        &[
            ("L8", 0),
            ("L30", 1),
            ("L58", 2),
            ("LA8", 3),
            ("LD0", 4),
            ("LF8", 5),
            ("L120", 6),
            ("L148", 7),
            ("L80", 8),
        ],
    )?;
    let order = core::POPULATION_ORDER
        .iter()
        .map(|which| m.getattr("Container")?.call1((*which as usize,)))
        .collect::<PyResult<Vec<_>>>()?;
    m.add(
        "POPULATION_ORDER",
        pyo3::types::PyTuple::new(m.py(), order)?,
    )?;
    m.add_function(wrap_pyfunction!(relationship, &m)?)?;
    m.add_function(wrap_pyfunction!(sort_native, &m)?)?;
    m.add_function(wrap_pyfunction!(add, &m)?)?;
    m.add_function(wrap_pyfunction!(cut, &m)?)?;
    m.add_function(wrap_pyfunction!(join_adjacent, &m)?)?;
    m.add_function(wrap_pyfunction!(optional, &m)?)?;
    m.add_function(wrap_pyfunction!(intersection, &m)?)?;
    m.add_function(wrap_pyfunction!(offset_for_source, &m)?)?;
    m.add_function(wrap_pyfunction!(f20c, &m)?)?;
    m.add_function(wrap_pyfunction!(channel_frequency_bad, &m)?)?;
    m.add_function(wrap_pyfunction!(common_bad, &m)?)?;
    m.add_function(wrap_pyfunction!(previous_channel_bad, &m)?)?;
    m.add_function(wrap_pyfunction!(channel_bad, &m)?)?;
    m.add_function(wrap_pyfunction!(auto_repeater_bad, &m)?)?;
    m.add_function(wrap_pyfunction!(auto_repeater_edge_bad, &m)?)?;
    parent.add_submodule(&m)
}
