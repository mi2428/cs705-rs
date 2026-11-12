//! Document values and owned inputs; borrowed native requests exist only per call.
use crate::{conversion::*, errors};
use core::{
    ActionContext as CoreActionContext, CloneControls as CoreCloneControls,
    ValidationView as CoreValidationView,
};
use cs705_core::icf::document as core;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use pyo3::{PyTraverseError, PyVisit};

#[pyclass(module = "cs705.icf.document")]
pub struct ImportResult {
    #[pyo3(get)]
    pub reader_called: bool,
    #[pyo3(get)]
    pub reader_result: Option<Py<PyAny>>,
    #[pyo3(get)]
    pub stage9: Option<Py<crate::icf::stage9::Report>>,
}
#[pymethods]
impl ImportResult {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.reader_result)?;
        visit.call(&self.stage9)
    }
    fn __clear__(&mut self) {
        self.reader_result = None;
        self.stage9 = None;
    }
}

macro_rules! scalar_value {
    ($name:ident, $native:ident, $module:literal, {$($field:ident, $setter:ident: $ty:ty => $convert:ident),* $(,)?}) => {
        #[pyclass(module = $module)]
        #[derive(Clone)]
        pub struct $name { pub inner: $native }
        #[pymethods]
        impl $name {
            #[new]
            fn new($($field: &Bound<'_, PyAny>),*) -> PyResult<Self> {
                Ok(Self { inner: $native { $($field: $convert($field)?),* } })
            }
            $(#[getter] fn $field(&self) -> $ty { self.inner.$field }
              #[setter] fn $setter(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
                  self.inner.$field = $convert(value)?; Ok(())
              })*
        }
    };
}
pub(crate) use scalar_value;
macro_rules! scalar_owner {
    ($name:ident, $native:ident, $module:literal, {$($field:ident, $setter:ident: $ty:ty => $convert:ident),* $(,)?} $(methods {$($methods:tt)*})?) => {
        #[pyclass(frozen, module = $module)]
        #[derive(Clone)]
        pub struct $name { pub target: super::Target<$native> }
        impl $name {
            pub fn from_core(value: $native) -> Self { Self { target: super::Target::new(value) } }
            pub fn from_loan(loan: crate::scoped::Loan<$native>) -> Self { Self { target: super::Target::Scoped(loan) } }
            pub fn with_ref<R>(&self, f: impl FnOnce(&$native) -> PyResult<R>) -> PyResult<R> { self.target.with_ref(f) }
            pub fn with_mut<R>(&self, f: impl FnOnce(&mut $native) -> PyResult<R>) -> PyResult<R> { self.target.with_mut(f) }
        }
        #[pymethods]
        impl $name {
            #[new]
            fn new($($field: &Bound<'_, PyAny>),*) -> PyResult<Self> {
                Ok(Self::from_core($native { $($field: $convert($field)?),* }))
            }
            $(#[getter] fn $field(&self) -> PyResult<$ty> { self.with_ref(|v| Ok(v.$field)) }
              #[setter] fn $setter(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
                  let value = $convert(value)?;
                  self.with_mut(|v| { v.$field = value; Ok(()) })
              })*
            $($($methods)*)?
        }
    };
}
pub(crate) use scalar_owner;
scalar_owner!(ActionContext, CoreActionContext, "cs705.icf.document", {
    action, set_action: u32 => strict_u32,
    state, set_state: u32 => strict_u32,
    phase, set_phase: u32 => strict_u32,
    callback_address, set_callback_address: u32 => strict_u32,
});
scalar_owner!(CloneControls, CoreCloneControls, "cs705.icf.document", {
    controller_busy, set_controller_busy: u32 => strict_u32,
    wrapper_state, set_wrapper_state: u32 => strict_u32,
    form_flag57, set_form_flag57: u8 => strict_u8,
});
scalar_value!(ValidationView, CoreValidationView, "cs705.icf.document", {
    kind, set_kind: u32 => strict_u32, group, set_group: u32 => strict_u32,
    first, set_first: i32 => strict_i32, last, set_last: i32 => strict_i32,
    count, set_count: i32 => strict_i32, include_side1, set_include_side1: bool => strict_bool,
    show_both, set_show_both: u32 => strict_u32,
});

#[pyclass(module = "cs705.icf.document")]
#[derive(Clone)]
pub struct PendingName {
    pub inner: core::PendingName,
}
#[pymethods]
impl PendingName {
    #[new]
    fn new(
        active: &Bound<'_, PyAny>,
        index: &Bound<'_, PyAny>,
        name: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::PendingName {
                active: strict_u8(active)?,
                index: strict_i32(index)?,
                name: strict_bytes(name)?.to_vec(),
            },
        })
    }
    #[getter]
    fn active(&self) -> u8 {
        self.inner.active
    }
    #[setter]
    fn set_active(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.active = strict_u8(value)?;
        Ok(())
    }
    #[getter]
    fn index(&self) -> i32 {
        self.inner.index
    }
    #[setter]
    fn set_index(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.index = strict_i32(value)?;
        Ok(())
    }
    #[getter]
    fn name<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.name)
    }
    #[setter]
    fn set_name(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.name = strict_bytes(value)?.to_vec();
        Ok(())
    }
}

#[pyclass(module = "cs705.icf.document")]
pub struct ActionEffects {
    pub inner: core::ActionEffects,
}
#[pymethods]
impl ActionEffects {
    #[new]
    #[pyo3(signature = (callback_invoked, timer_ms, dirty))]
    fn new(
        callback_invoked: &Bound<'_, PyAny>,
        timer_ms: Option<&Bound<'_, PyAny>>,
        dirty: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::ActionEffects {
                callback_invoked: strict_bool(callback_invoked)?,
                timer_ms: timer_ms.map(strict_u32).transpose()?,
                dirty: strict_u32(dirty)?,
            },
        })
    }
    #[getter]
    fn callback_invoked(&self) -> bool {
        self.inner.callback_invoked
    }
    #[setter]
    fn set_callback_invoked(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.callback_invoked = strict_bool(value)?;
        Ok(())
    }
    #[getter]
    fn timer_ms(&self) -> Option<u32> {
        self.inner.timer_ms
    }
    #[setter]
    fn set_timer_ms(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.inner.timer_ms = value.map(strict_u32).transpose()?;
        Ok(())
    }
    #[getter]
    fn dirty(&self) -> u32 {
        self.inner.dirty
    }
    #[setter]
    fn set_dirty(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.dirty = strict_u32(value)?;
        Ok(())
    }
}

#[pyclass(module = "cs705.icf.document")]
#[derive(Clone)]
pub struct DefaultSource {
    pub selector: u8,
    pub prefix: Vec<u8>,
    pub japanese_mode_pairs: Option<[u8; 120]>,
    pub restricted_band_stack: Option<[u8; 108]>,
}
impl DefaultSource {
    pub fn as_core(&self) -> core::DefaultSource<'_> {
        core::DefaultSource {
            selector: self.selector,
            prefix: &self.prefix,
            japanese_mode_pairs: self.japanese_mode_pairs.as_ref(),
            restricted_band_stack: self.restricted_band_stack.as_ref(),
        }
    }
}
#[pymethods]
impl DefaultSource {
    #[new]
    #[pyo3(signature = (selector, prefix, japanese_mode_pairs=None, restricted_band_stack=None))]
    fn new(
        selector: &Bound<'_, PyAny>,
        prefix: &Bound<'_, PyAny>,
        japanese_mode_pairs: Option<&Bound<'_, PyAny>>,
        restricted_band_stack: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            selector: strict_u8(selector)?,
            prefix: strict_bytes(prefix)?.to_vec(),
            japanese_mode_pairs: japanese_mode_pairs.map(fixed_bytes::<120>).transpose()?,
            restricted_band_stack: restricted_band_stack.map(fixed_bytes::<108>).transpose()?,
        })
    }
    #[getter]
    fn selector(&self) -> u8 {
        self.selector
    }
    #[setter]
    fn set_selector(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.selector = strict_u8(value)?;
        Ok(())
    }
    #[getter]
    fn prefix<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.prefix)
    }
    #[setter]
    fn set_prefix(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.prefix = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[getter]
    fn japanese_mode_pairs<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.japanese_mode_pairs
            .as_ref()
            .map(|b| PyBytes::new(py, b))
    }
    #[setter]
    fn set_japanese_mode_pairs(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.japanese_mode_pairs = value.map(fixed_bytes::<120>).transpose()?;
        Ok(())
    }
    #[getter]
    fn restricted_band_stack<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.restricted_band_stack
            .as_ref()
            .map(|b| PyBytes::new(py, b))
    }
    #[setter]
    fn set_restricted_band_stack(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.restricted_band_stack = value.map(fixed_bytes::<108>).transpose()?;
        Ok(())
    }
}

#[pyclass(module = "cs705.icf.document")]
#[derive(Clone)]
pub struct ValidationReport {
    pub inner: core::ValidationReport,
}
#[pymethods]
impl ValidationReport {
    #[new]
    fn new(
        errors: &Bound<'_, PyAny>,
        warnings: &Bound<'_, PyAny>,
        flags: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::ValidationReport {
                errors: strict_u32(errors)?,
                warnings: strict_u32(warnings)?,
                flags: strict_bytes(flags)?.to_vec(),
            },
        })
    }
    #[getter]
    fn errors(&self) -> u32 {
        self.inner.errors
    }
    #[setter]
    fn set_errors(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.errors = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn warnings(&self) -> u32 {
        self.inner.warnings
    }
    #[setter]
    fn set_warnings(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.warnings = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn flags<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.flags)
    }
    #[setter]
    fn set_flags(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.flags = strict_bytes(value)?.to_vec();
        Ok(())
    }
}

#[pyclass(frozen, module = "cs705.icf.document")]
pub struct ValidationSource {
    pub views: Option<Vec<Py<ValidationView>>>,
}
impl ValidationSource {
    pub fn values(&self, py: Python<'_>) -> PyResult<Option<Vec<core::ValidationView>>> {
        self.views
            .as_ref()
            .map(|views| {
                views
                    .iter()
                    .map(|view| Ok(view.try_borrow(py)?.inner))
                    .collect()
            })
            .transpose()
    }
}
pub fn validation(values: &Option<Vec<core::ValidationView>>) -> core::ValidationSource<'_> {
    values.as_deref().map_or(
        core::ValidationSource::NoTree,
        core::ValidationSource::Views,
    )
}
#[pymethods]
impl ValidationSource {
    #[classattr]
    #[pyo3(name = "NoTree")]
    fn no_tree() -> Self {
        Self { views: None }
    }
    #[staticmethod]
    #[pyo3(name = "Views")]
    fn from_views(views: Vec<Py<ValidationView>>) -> Self {
        Self { views: Some(views) }
    }
    #[getter]
    fn views(&self, py: Python<'_>) -> Option<Vec<Py<ValidationView>>> {
        self.views
            .as_ref()
            .map(|v| v.iter().map(|x| x.clone_ref(py)).collect())
    }
    #[getter]
    fn kind(&self) -> &'static str {
        if self.views.is_some() {
            "Views"
        } else {
            "NoTree"
        }
    }
}

#[pyclass(module = "cs705.icf.document")]
pub struct SaveRequest {
    pub path: Vec<u8>,
    pub target_revision: u32,
    pub legacy_choice: i32,
    pub warning_choice: Option<i32>,
    #[pyo3(get, set)]
    pub validation: Py<ValidationSource>,
}
#[pymethods]
impl SaveRequest {
    #[new]
    #[pyo3(signature = (path, target_revision, legacy_choice, warning_choice, validation))]
    fn new(
        path: &Bound<'_, PyAny>,
        target_revision: &Bound<'_, PyAny>,
        legacy_choice: &Bound<'_, PyAny>,
        warning_choice: Option<&Bound<'_, PyAny>>,
        validation: Py<ValidationSource>,
    ) -> PyResult<Self> {
        Ok(Self {
            path: strict_bytes(path)?.to_vec(),
            target_revision: strict_u32(target_revision)?,
            legacy_choice: strict_i32(legacy_choice)?,
            warning_choice: warning_choice.map(strict_i32).transpose()?,
            validation,
        })
    }
    #[getter]
    fn path<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.path)
    }
    #[setter]
    fn set_path(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.path = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[getter]
    fn target_revision(&self) -> u32 {
        self.target_revision
    }
    #[setter]
    fn set_target_revision(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.target_revision = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn legacy_choice(&self) -> i32 {
        self.legacy_choice
    }
    #[setter]
    fn set_legacy_choice(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.legacy_choice = strict_i32(value)?;
        Ok(())
    }
    #[getter]
    fn warning_choice(&self) -> Option<i32> {
        self.warning_choice
    }
    #[setter]
    fn set_warning_choice(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.warning_choice = value.map(strict_i32).transpose()?;
        Ok(())
    }
}

#[pyclass(frozen, module = "cs705.icf.document")]
#[derive(Clone, Copy)]
pub struct OpenStatus {
    pub inner: core::OpenStatus,
}
#[pymethods]
impl OpenStatus {
    #[classattr]
    #[pyo3(name = "Cancelled")]
    fn cancelled() -> Self {
        Self {
            inner: core::OpenStatus::Cancelled,
        }
    }
    #[staticmethod]
    #[pyo3(name = "Rejected")]
    fn rejected(status: &super::super::format::Status) -> Self {
        Self {
            inner: core::OpenStatus::Rejected(status.inner),
        }
    }
    #[staticmethod]
    #[pyo3(name = "Opened")]
    fn opened(
        adopted_context: &Bound<'_, PyAny>,
        foreign_context: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::OpenStatus::Opened {
                adopted_context: strict_bool(adopted_context)?,
                foreign_context: strict_bool(foreign_context)?,
            },
        })
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self.inner {
            core::OpenStatus::Cancelled => "Cancelled",
            core::OpenStatus::Rejected(_) => "Rejected",
            core::OpenStatus::Opened { .. } => "Opened",
        }
    }
    #[getter]
    fn status(&self) -> Option<super::super::format::Status> {
        match self.inner {
            core::OpenStatus::Rejected(inner) => Some(super::super::format::Status { inner }),
            _ => None,
        }
    }
    #[getter]
    fn adopted_context(&self) -> Option<bool> {
        match self.inner {
            core::OpenStatus::Opened {
                adopted_context, ..
            } => Some(adopted_context),
            _ => None,
        }
    }
    #[getter]
    fn foreign_context(&self) -> Option<bool> {
        match self.inner {
            core::OpenStatus::Opened {
                foreign_context, ..
            } => Some(foreign_context),
            _ => None,
        }
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

pub(crate) fn add_enum(
    m: &Bound<'_, PyModule>,
    module: &str,
    name: &str,
    variants: &[&str],
) -> PyResult<()> {
    let py = m.py();
    let entries = PyDict::new(py);
    for (value, name) in variants.iter().enumerate() {
        entries.set_item(*name, value)?;
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item("module", module)?;
    m.add(
        name,
        py.import("enum")?
            .getattr("Enum")?
            .call((name, entries), Some(&kwargs))?,
    )
}
pub(crate) fn enum_in(value: &Bound<'_, PyAny>, module: &str, name: &str) -> PyResult<usize> {
    let ty = value.py().import(module)?.getattr(name)?;
    if !value.is_instance(&ty)? {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "expected {module}.{name}"
        )));
    }
    let code = strict_usize(&value.getattr("value")?)?;
    let member = ty
        .getattr("__members__")?
        .call_method0("values")?
        .try_iter()?
        .nth(code)
        .transpose()?;
    if !member.is_some_and(|member| member.is(value)) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "invalid enum member value",
        ));
    }
    Ok(code)
}
pub(crate) fn enum_out<'py>(
    py: Python<'py>,
    module: &str,
    name: &str,
    value: usize,
) -> PyResult<Bound<'py, PyAny>> {
    py.import(module)?.getattr(name)?.call1((value,))
}
#[pyfunction]
fn resolve_prompt(
    dirty: &Bound<'_, PyAny>,
    choice: &Bound<'_, PyAny>,
    save: Option<&Bound<'_, PyAny>>,
) -> PyResult<i32> {
    let save = save
        .map(|s| {
            enum_in(s, "cs705.icf.document", "SaveOutcome").map(|v| {
                if v == 0 {
                    core::SaveOutcome::Saved
                } else {
                    core::SaveOutcome::NotSaved
                }
            })
        })
        .transpose()?;
    core::resolve_prompt(strict_u32(dirty)?, strict_i32(choice)?, save).map_err(errors::icf)
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    for (name, variants) in [
        ("SaveOutcome", &["Saved", "NotSaved"][..]),
        ("SaveStatus", &["Saved", "Cancelled", "RetrySaveAs"][..]),
    ] {
        add_enum(m, "cs705.icf.document", name, variants)?;
    }
    m.add_class::<ActionContext>()?;
    m.add_class::<ImportResult>()?;
    m.add_class::<CloneControls>()?;
    m.add_class::<ActionEffects>()?;
    m.add_class::<PendingName>()?;
    m.add_class::<DefaultSource>()?;
    m.add_class::<ValidationView>()?;
    m.add_class::<ValidationReport>()?;
    m.add_class::<ValidationSource>()?;
    m.add_class::<SaveRequest>()?;
    m.add_class::<OpenStatus>()?;
    m.add_function(wrap_pyfunction!(resolve_prompt, m)?)?;
    Ok(())
}
