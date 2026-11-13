//! Observable normalization reports and caller-owned reset controls.
use super::values::{Input, Output};
use crate::conversion::*;
use cs705_core::icf::normalizer as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::cell::RefCell;

/// Explicit image role: Working reads the live target; Separate retains bytes.
/// Equal contents do not imply aliasing. Later reads observe earlier writes only
/// for Working; no default image or current-image source is synthesized.
#[pyclass(module = "cs705.icf.normalizer", frozen)]
pub struct ReferenceImage {
    source: Option<Py<PyBytes>>,
}
impl ReferenceImage {
    pub fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::ReferenceImage<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        f(match &self.source {
            None => core::ReferenceImage::Working,
            Some(source) => core::ReferenceImage::Separate(source.bind(py).as_bytes()),
        })
    }
}
#[pymethods]
impl ReferenceImage {
    #[classattr]
    #[pyo3(name = "Working")]
    fn working() -> Self {
        Self { source: None }
    }
    #[staticmethod]
    #[pyo3(name = "Separate")]
    fn separate(source: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            source: Some(source.cast::<PyBytes>()?.clone().unbind()),
        })
    }
    #[getter]
    fn kind(&self) -> &'static str {
        if self.source.is_some() {
            "Separate"
        } else {
            "Working"
        }
    }
    #[getter]
    fn source<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.source.as_ref().map(|source| source.bind(py).clone())
    }
}

macro_rules! fields {
    ($name:ident, $($field:ident : $ty:ty => $set:ident, $strict:ident);* $(;)?) => {
        #[pyclass(module = "cs705.icf.normalizer")]
        #[derive(Clone, Copy)]
        pub struct $name { pub inner: core::$name }
        impl $name { pub fn from_core(inner: core::$name) -> Self { Self { inner } } }
        #[pymethods]
        impl $name {
            #[new]
            fn new($($field: &Bound<'_, PyAny>),*) -> PyResult<Self> { Ok(Self { inner: core::$name { $($field: $strict($field)?),* } }) }
            $(#[getter] fn $field(&self) -> $ty { self.inner.$field }
            #[setter] fn $set(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> { self.inner.$field = $strict(value)?; Ok(()) })*
        }
    }
}
fields!(TypedCounts, saved: u32 => set_saved, strict_u32; memopad_valid: u32 => set_memopad_valid, strict_u32);
fields!(ChannelValues, mode:u32=>set_mode,strict_u32; data:u32=>set_data,strict_u32; filter:u32=>set_filter,strict_u32; offset_hz:u32=>set_offset_hz,strict_u32; tx_tone:u32=>set_tx_tone,strict_u32; rx_tone:u32=>set_rx_tone,strict_u32);

#[pyclass(module = "cs705.icf.normalizer")]
pub struct ChannelNormalization {
    inner: core::ChannelNormalization,
}
#[pymethods]
impl ChannelNormalization {
    #[new]
    fn new(
        kind: &Bound<'_, PyAny>,
        index: &Bound<'_, PyAny>,
        working_binding: &Bound<'_, PyAny>,
        default_binding: &Bound<'_, PyAny>,
        q12: &Bound<'_, PyAny>,
        previous_reload_guard: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::ChannelNormalization {
                kind: Input::input(kind)?,
                index: strict_u32(index)?,
                working_binding: strict_u32(working_binding)?,
                default_binding: strict_u32(default_binding)?,
                q12: strict_u8(q12)?,
                previous_reload_guard: strict_u32(previous_reload_guard)?,
            },
        })
    }
    #[getter]
    fn kind<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.inner.kind.output(py)
    }
    #[setter]
    fn set_kind(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.kind = Input::input(value)?;
        Ok(())
    }
    #[getter]
    fn index(&self) -> u32 {
        self.inner.index
    }
    #[setter]
    fn set_index(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.index = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn working_binding(&self) -> u32 {
        self.inner.working_binding
    }
    #[setter]
    fn set_working_binding(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.working_binding = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn default_binding(&self) -> u32 {
        self.inner.default_binding
    }
    #[setter]
    fn set_default_binding(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.default_binding = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn q12(&self) -> u8 {
        self.inner.q12
    }
    #[setter]
    fn set_q12(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.q12 = strict_u8(value)?;
        Ok(())
    }
    #[getter]
    fn previous_reload_guard(&self) -> u32 {
        self.inner.previous_reload_guard
    }
    #[setter]
    fn set_previous_reload_guard(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.previous_reload_guard = strict_u32(value)?;
        Ok(())
    }
}

struct Callbacks {
    origin: i64,
    errors: RefCell<crate::callbacks::CallbackError>,
}
#[pyfunction]
fn normalize_channel_at(
    py: Python<'_>,
    image: Py<MutableBytes>,
    defaults: &ReferenceImage,
    input: &ChannelNormalization,
    previous_bad: Py<PyAny>,
    before_store: Py<PyAny>,
) -> PyResult<Option<super::record::DecodedRecord>> {
    let input = input.inner;
    let callbacks = Callbacks::new(py)?;
    let (errors, result) = {
        let mut image = image.get().take()?;
        defaults.with_core(py, |defaults| {
            Ok(py.detach(move || {
                let result = core::normalize_channel_at(
                    &mut image,
                    defaults,
                    input,
                    |source, holder| {
                        callbacks.call(|py| {
                            crate::scoped::Frame::with_ref(py, source, |source| {
                                crate::scoped::Frame::with_ref(py, holder, |holder| {
                                    strict_bool(&previous_bad.bind(py).call1((
                                        crate::csv::nls::Bytes::from_loan(source),
                                        super::record::DecodedRecord::from_loan(holder),
                                    ))?)
                                })
                            })
                        })
                    },
                    |write| {
                        callbacks.call(|py| {
                            super::record::with_channel_write(py, write, |write| {
                                super::document::adapters::require_none(
                                    before_store.bind(py).call1((write,))?,
                                )
                            })
                        })
                    },
                );
                (callbacks.errors.into_inner(), result)
            }))
        })?
    };
    errors
        .finish(py, result.map_err(crate::errors::icf))
        .map(|value| value.map(super::record::DecodedRecord::from_core))
}
impl Callbacks {
    fn new(py: Python<'_>) -> PyResult<Self> {
        Ok(Self {
            origin: crate::scoped::interpreter(py)?,
            errors: RefCell::default(),
        })
    }
    fn call<T>(
        &self,
        f: impl FnOnce(Python<'_>) -> PyResult<T>,
    ) -> Result<T, cs705_core::icf::Error> {
        self.errors
            .borrow_mut()
            .invoke(|| {
                Python::attach(|py| {
                    if crate::scoped::interpreter(py)? != self.origin {
                        return Err(pyo3::exceptions::PyRuntimeError::new_err(
                            "normalizer callback belongs to another interpreter",
                        ));
                    }
                    f(py)
                })
            })
            .map_err(|_| {
                cs705_core::icf::Error::Operation("Python normalizer callback failed".into())
            })
    }
}
#[pyfunction]
/// Read count on each iteration and get twice per range, for low then high.
/// Callback exceptions stop immediately and retain completed output pairs.
fn stage7_live(
    py: Python<'_>,
    image: Py<MutableBytes>,
    count: Py<PyAny>,
    get: Py<PyAny>,
) -> PyResult<()> {
    let callbacks = Callbacks::new(py)?;
    let (errors, result) = {
        let mut image = image.get().take()?;
        py.detach(move || {
            let result = core::stage7_live(
                &mut image,
                || callbacks.call(|py| strict_i32(&count.bind(py).call0()?)),
                |index| {
                    callbacks.call(|py| {
                        Ok(get
                            .bind(py)
                            .call1((index,))?
                            .extract::<PyRef<'_, super::frequency::FrequencyRange>>()?
                            .inner)
                    })
                },
            );
            (callbacks.errors.into_inner(), result)
        })
    };
    errors.finish(py, result.map_err(crate::errors::icf))
}
#[pyfunction]
fn normalize_range_at(
    py: Python<'_>,
    image: Py<MutableBytes>,
    defaults: &ReferenceImage,
    kind: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    bindings: (Bound<'_, PyAny>, Bound<'_, PyAny>),
    before_store: Py<PyAny>,
) -> PyResult<Option<super::record::DecodedRecord>> {
    let (kind, index, bindings) = (
        strict_u32(kind)?,
        strict_u32(index)?,
        (strict_u32(&bindings.0)?, strict_u32(&bindings.1)?),
    );
    let callbacks = Callbacks::new(py)?;
    let (errors, result) = {
        let mut image = image.get().take()?;
        defaults.with_core(py, |defaults| {
            Ok(py.detach(move || {
                let result = core::normalize_range_at(
                    &mut image,
                    defaults,
                    kind,
                    index,
                    bindings,
                    |at, bytes| {
                        callbacks.call(|py| {
                            super::document::adapters::require_none(
                                before_store.bind(py).call1((at, PyBytes::new(py, bytes)))?,
                            )
                        })
                    },
                );
                (callbacks.errors.into_inner(), result)
            }))
        })?
    };
    errors
        .finish(py, result.map_err(crate::errors::icf))
        .map(|value| value.map(super::record::DecodedRecord::from_core))
}
#[pyfunction]
#[expect(
    clippy::too_many_arguments,
    reason = "The native normalizer requires distinct W/F images, binding pair, Q12 and writer"
)]
fn normalize_special_at(
    py: Python<'_>,
    image: Py<MutableBytes>,
    defaults: &ReferenceImage,
    kind: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    bindings: (Bound<'_, PyAny>, Bound<'_, PyAny>),
    q12: &Bound<'_, PyAny>,
    before_store: Py<PyAny>,
) -> PyResult<Option<super::record::DecodedRecord>> {
    let (kind, index, bindings, q12) = (
        strict_u32(kind)?,
        strict_u32(index)?,
        (strict_u32(&bindings.0)?, strict_u32(&bindings.1)?),
        strict_u8(q12)?,
    );
    let callbacks = Callbacks::new(py)?;
    let (errors, result) = {
        let mut image = image.get().take()?;
        defaults.with_core(py, |defaults| {
            Ok(py.detach(move || {
                let result = core::normalize_special_at(
                    &mut image,
                    defaults,
                    kind,
                    index,
                    bindings,
                    q12,
                    |at, bytes| {
                        callbacks.call(|py| {
                            super::document::adapters::require_none(
                                before_store.bind(py).call1((at, PyBytes::new(py, bytes)))?,
                            )
                        })
                    },
                );
                (callbacks.errors.into_inner(), result)
            }))
        })?
    };
    errors
        .finish(py, result.map_err(crate::errors::icf))
        .map(|value| value.map(super::record::DecodedRecord::from_core))
}
#[pyfunction]
fn stage9_memopad(
    py: Python<'_>,
    image: Py<MutableBytes>,
    defaults: &ReferenceImage,
    bindings: (Bound<'_, PyAny>, Bound<'_, PyAny>),
    before_store: Py<PyAny>,
) -> PyResult<()> {
    let bindings = (strict_u32(&bindings.0)?, strict_u32(&bindings.1)?);
    let callbacks = Callbacks::new(py)?;
    let (errors, result) = {
        let mut image = image.get().take()?;
        defaults.with_core(py, |defaults| {
            Ok(py.detach(move || {
                let result =
                    core::stage9_memopad(&mut image, defaults, bindings, |index, at, bytes| {
                        callbacks.call(|py| {
                            super::document::adapters::require_none(
                                before_store.bind(py).call1((
                                    index,
                                    at,
                                    PyBytes::new(py, bytes),
                                ))?,
                            )
                        })
                    });
                (callbacks.errors.into_inner(), result)
            }))
        })?
    };
    errors.finish(py, result.map_err(crate::errors::icf))
}
#[pyfunction]
#[pyo3(signature=(image, defaults, context, q13, system_langid))]
fn stage9_settings_tail(
    py: Python<'_>,
    image: Py<MutableBytes>,
    defaults: &ReferenceImage,
    context: &super::context::Context,
    q13: &Bound<'_, PyAny>,
    system_langid: Option<&Bound<'_, PyAny>>,
) -> PyResult<()> {
    let (q13, id, context) = (
        strict_u8(q13)?,
        system_langid.map(strict_u16).transpose()?,
        context.with_ref(|value| Ok(*value))?,
    );
    let mut image = image.get().take()?;
    defaults.with_core(py, |defaults| {
        py.detach(|| {
            core::stage9_settings_tail(&mut image, defaults, &context, q13, id)
                .map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
#[pyo3(signature=(image, holder, numeric_only, q, ranges, system_langid))]
fn typed_bad(
    image: &Bound<'_, PyAny>,
    holder: &super::record::DecodedRecord,
    numeric_only: &Bound<'_, PyAny>,
    q: &super::context::Context,
    ranges: &super::frequency::Ranges,
    system_langid: Option<&Bound<'_, PyAny>>,
) -> PyResult<bool> {
    let (image, numeric_only, id) = (
        strict_bytes(image)?,
        strict_bool(numeric_only)?,
        system_langid.map(strict_u16).transpose()?,
    );
    let q = q.with_ref(|value| Ok(*value))?;
    ranges.with_ref(|ranges| {
        holder.with_ref(|holder| {
            core::typed_bad(image, holder, numeric_only, &q, ranges, id).map_err(crate::errors::icf)
        })
    })
}
#[pyclass(module = "cs705.icf.normalizer", frozen)]
pub struct ResetState {
    owner: crate::owner::Owner<core::ResetState>,
}
impl ResetState {
    pub fn from_core(value: core::ResetState) -> Self {
        Self {
            owner: crate::owner::Owner::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::ResetState) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_ref(f)
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut core::ResetState) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_mut(f)
    }
    pub fn take(&self) -> PyResult<crate::owner::Taken<'_, core::ResetState>> {
        self.owner.take()
    }
}
#[pymethods]
impl ResetState {
    #[new]
    fn new(show_both: &Bound<'_, PyAny>, home_invalidated: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(core::ResetState {
            show_both: strict_u32(show_both)?,
            home_invalidated: strict_u8(home_invalidated)?,
        }))
    }
    #[getter]
    fn show_both(&self) -> PyResult<u32> {
        self.with_ref(|value| Ok(value.show_both))
    }
    #[setter]
    fn set_show_both(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u32(value)?;
        self.with_mut(|state| {
            state.show_both = value;
            Ok(())
        })
    }
    #[getter]
    fn home_invalidated(&self) -> PyResult<u8> {
        self.with_ref(|value| Ok(value.home_invalidated))
    }
    #[setter]
    fn set_home_invalidated(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u8(value)?;
        self.with_mut(|state| {
            state.home_invalidated = value;
            Ok(())
        })
    }
}
fields!(ResetLists, saved_repeaters: u32 => set_saved_repeaters, strict_u32; deleted_repeaters: u32 => set_deleted_repeaters, strict_u32; deleted_ur: u32 => set_deleted_ur, strict_u32; deleted_allowed: u32 => set_deleted_allowed, strict_u32);

#[pyclass(module = "cs705.icf.normalizer")]
pub struct ResetReport {
    typed_saved: u32,
    #[pyo3(get, set)]
    pub lists: Py<ResetLists>,
}
impl ResetReport {
    pub fn from_core(py: Python<'_>, inner: core::ResetReport) -> PyResult<Self> {
        Ok(Self {
            typed_saved: inner.typed_saved,
            lists: Py::new(py, ResetLists::from_core(inner.lists))?,
        })
    }
    pub fn to_core(&self, py: Python<'_>) -> PyResult<core::ResetReport> {
        Ok(core::ResetReport {
            typed_saved: self.typed_saved,
            lists: self.lists.try_borrow(py)?.inner,
        })
    }
}
#[pymethods]
impl ResetReport {
    #[new]
    fn new(typed_saved: &Bound<'_, PyAny>, lists: Py<ResetLists>) -> PyResult<Self> {
        Ok(Self {
            typed_saved: strict_u32(typed_saved)?,
            lists,
        })
    }
    #[getter]
    fn typed_saved(&self) -> u32 {
        self.typed_saved
    }
    #[setter]
    fn set_typed_saved(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.typed_saved = strict_u32(value)?;
        Ok(())
    }
}
#[pyfunction]
fn round_offset_hz(value: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(core::round_offset_hz(strict_u32(value)?))
}
#[pyfunction]
fn normalize_channel_values(
    mut values: PyRefMut<'_, ChannelValues>,
    scan: &Bound<'_, PyAny>,
    remove_mton: &Bound<'_, PyAny>,
) -> PyResult<()> {
    core::normalize_channel_values(
        &mut values.inner,
        strict_bool(scan)?,
        strict_bool(remove_mton)?,
    );
    Ok(())
}
#[pyfunction]
fn sanitize_fixed(bytes: &MutableBytes, replacement: &Bound<'_, PyAny>) -> PyResult<()> {
    let replacement = strict_u8(replacement)?;
    bytes.with_mut(|bytes| {
        core::sanitize_fixed(bytes, replacement);
        Ok(())
    })
}
#[pyfunction]
fn remove_dbcs_fixed(bytes: &MutableBytes, replacement: &Bound<'_, PyAny>) -> PyResult<()> {
    let replacement = strict_u8(replacement)?;
    bytes.with_mut(|bytes| {
        core::remove_dbcs_fixed(bytes, replacement);
        Ok(())
    })
}
#[pyfunction]
fn normalize_keyer_fixed(bytes: &MutableBytes, disallow_star: &Bound<'_, PyAny>) -> PyResult<()> {
    let disallow_star = strict_bool(disallow_star)?;
    bytes.with_mut(|bytes| {
        core::normalize_keyer_fixed(bytes, disallow_star);
        Ok(())
    })
}
#[pyfunction]
fn stage7(
    image: &MutableBytes,
    ranges: Vec<PyRef<'_, super::frequency::FrequencyRange>>,
) -> PyResult<()> {
    let ranges = ranges.iter().map(|r| r.inner).collect::<Vec<_>>();
    image.with_mut(|image| core::stage7(image, &ranges).map_err(crate::errors::icf))
}
#[pyfunction]
fn stage9_fixed_text(image: &MutableBytes) -> PyResult<()> {
    image.with_mut(|image| core::stage9_fixed_text(image).map_err(crate::errors::icf))
}
#[pyfunction]
fn normalize_name<'py>(
    py: Python<'py>,
    shared: &MutableBytes,
    width: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let width = strict_usize(width)?;
    let value = shared.with_mut(|shared| Ok(core::normalize_name(shared, width)))?;
    Ok(PyBytes::new(py, &value))
}
#[pyfunction]
fn delete_allowed_into(image: &MutableBytes, index: &Bound<'_, PyAny>) -> PyResult<()> {
    let index = strict_u32(index)?;
    image.with_mut(|image| core::delete_allowed_into(image, index).map_err(crate::errors::icf))
}
#[pyfunction]
fn late_callsign_list_bad(
    current: &Bound<'_, PyAny>,
    binding: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    core::late_callsign_list_bad(strict_bytes(current)?, strict_u32(binding)?)
        .map_err(crate::errors::icf)
}
#[pyfunction]
fn stage9_keyer(py: Python<'_>, image: &MutableBytes, current: &ReferenceImage) -> PyResult<()> {
    current.with_core(py, |current| {
        image.with_mut(|image| core::stage9_keyer(image, current).map_err(crate::errors::icf))
    })
}
#[pyfunction]
fn stage9_control_bits(
    py: Python<'_>,
    image: &MutableBytes,
    defaults: &ReferenceImage,
) -> PyResult<()> {
    defaults.with_core(py, |defaults| {
        image.with_mut(|image| {
            core::stage9_control_bits(image, defaults).map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
fn stage9_profile_prefix(
    py: Python<'_>,
    image: &MutableBytes,
    defaults: &ReferenceImage,
    context: &super::context::Context,
) -> PyResult<()> {
    let context = context.with_ref(|value| Ok(*value))?;
    defaults.with_core(py, |defaults| {
        image.with_mut(|image| {
            core::stage9_profile_prefix(image, defaults, &context).map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
fn stage9_filter_bits(
    py: Python<'_>,
    image: &MutableBytes,
    defaults: &ReferenceImage,
    context: &super::context::Context,
) -> PyResult<()> {
    let context = context.with_ref(|value| Ok(*value))?;
    defaults.with_core(py, |defaults| {
        image.with_mut(|image| {
            core::stage9_filter_bits(image, defaults, &context).map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
#[pyo3(signature = (image, defaults, q, ranges, system_langid, previous_reload_guard))]
fn typed_loop(
    py: Python<'_>,
    image: &MutableBytes,
    defaults: &ReferenceImage,
    q: &super::context::Context,
    ranges: &super::frequency::Ranges,
    system_langid: Option<&Bound<'_, PyAny>>,
    previous_reload_guard: &Bound<'_, PyAny>,
) -> PyResult<TypedCounts> {
    let id = system_langid.map(strict_u16).transpose()?;
    let guard = strict_u32(previous_reload_guard)?;
    let q = q.with_ref(|value| Ok(*value))?;
    defaults.with_core(py, |defaults| {
        ranges.with_ref(|ranges| {
            image.with_mut(|image| {
                core::typed_loop(image, defaults, &q, ranges, id, guard)
                    .map(TypedCounts::from_core)
                    .map_err(crate::errors::icf)
            })
        })
    })
}
#[pyfunction]
#[pyo3(signature = (image, defaults, q, ranges, system_langid))]
fn reset_lists(
    py: Python<'_>,
    image: &MutableBytes,
    defaults: &ReferenceImage,
    q: &super::context::Context,
    ranges: &super::frequency::Ranges,
    system_langid: Option<&Bound<'_, PyAny>>,
) -> PyResult<ResetLists> {
    let id = system_langid.map(strict_u16).transpose()?;
    let q = q.with_ref(|value| Ok(*value))?;
    defaults.with_core(py, |defaults| {
        ranges.with_ref(|ranges| {
            image.with_mut(|image| {
                core::reset_lists(image, defaults, &q, ranges, id)
                    .map(ResetLists::from_core)
                    .map_err(crate::errors::icf)
            })
        })
    })
}
#[pyfunction]
#[pyo3(signature = (image, defaults, q, ranges, system_langid, state))]
fn reset_operating_state(
    py: Python<'_>,
    image: &MutableBytes,
    defaults: &ReferenceImage,
    q: &super::context::Context,
    ranges: &super::frequency::Ranges,
    system_langid: Option<&Bound<'_, PyAny>>,
    state: &ResetState,
) -> PyResult<ResetReport> {
    let id = system_langid.map(strict_u16).transpose()?;
    let q = q.with_ref(|value| Ok(*value))?;
    let mut state = state.take()?;
    let value = defaults.with_core(py, |defaults| {
        ranges.with_ref(|ranges| {
            image.with_mut(|image| {
                core::reset_operating_state(image, defaults, &q, ranges, id, &mut state)
                    .map_err(crate::errors::icf)
            })
        })
    })?;
    ResetReport::from_core(py, value)
}
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "normalizer")?;
    m.add_class::<TypedCounts>()?;
    m.add_class::<ResetState>()?;
    m.add_class::<ResetLists>()?;
    m.add_class::<ResetReport>()?;
    m.add_class::<ReferenceImage>()?;
    m.add_class::<ChannelValues>()?;
    m.add_class::<ChannelNormalization>()?;
    m.add_function(wrap_pyfunction!(normalize_channel_at, &m)?)?;
    m.add_function(wrap_pyfunction!(stage7_live, &m)?)?;
    m.add_function(wrap_pyfunction!(normalize_range_at, &m)?)?;
    m.add_function(wrap_pyfunction!(normalize_special_at, &m)?)?;
    m.add_function(wrap_pyfunction!(stage9_memopad, &m)?)?;
    m.add_function(wrap_pyfunction!(stage9_settings_tail, &m)?)?;
    m.add_function(wrap_pyfunction!(typed_bad, &m)?)?;
    m.add(
        "FrequencyRange",
        parent.getattr("frequency")?.getattr("FrequencyRange")?,
    )?;
    m.add(
        "FrequencyContext",
        parent.getattr("frequency")?.getattr("FrequencyContext")?,
    )?;
    m.add(
        "previous_channel_bad",
        parent
            .getattr("frequency")?
            .getattr("previous_channel_bad")?,
    )?;
    m.add_function(wrap_pyfunction!(normalize_channel_values, &m)?)?;
    m.add(
        "TYPED_SLOTS",
        pyo3::types::PyTuple::new(m.py(), core::TYPED_SLOTS)?,
    )?;
    m.add_function(wrap_pyfunction!(round_offset_hz, &m)?)?;
    m.add_function(wrap_pyfunction!(sanitize_fixed, &m)?)?;
    m.add_function(wrap_pyfunction!(remove_dbcs_fixed, &m)?)?;
    m.add_function(wrap_pyfunction!(normalize_keyer_fixed, &m)?)?;
    m.add_function(wrap_pyfunction!(stage7, &m)?)?;
    m.add_function(wrap_pyfunction!(stage9_fixed_text, &m)?)?;
    m.add_function(wrap_pyfunction!(normalize_name, &m)?)?;
    m.add_function(wrap_pyfunction!(delete_allowed_into, &m)?)?;
    m.add_function(wrap_pyfunction!(late_callsign_list_bad, &m)?)?;
    m.add_function(wrap_pyfunction!(stage9_keyer, &m)?)?;
    m.add_function(wrap_pyfunction!(stage9_control_bits, &m)?)?;
    m.add_function(wrap_pyfunction!(stage9_profile_prefix, &m)?)?;
    m.add_function(wrap_pyfunction!(stage9_filter_bits, &m)?)?;
    m.add_function(wrap_pyfunction!(typed_loop, &m)?)?;
    m.add_function(wrap_pyfunction!(reset_lists, &m)?)?;
    m.add_function(wrap_pyfunction!(reset_operating_state, &m)?)?;
    parent.add_submodule(&m)
}
