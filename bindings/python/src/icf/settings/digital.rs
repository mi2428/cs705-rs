//! Direct digital editors over the shared native image and text inputs.
use super::extended::{ChoiceInput, FixedTextInput, TextEffect};
use crate::icf::values::{Input, Output};
use crate::{callbacks::CallbackError, conversion::*, icf::Icf, owner::Owner};
use cs705_core::{csv, icf::settings::digital as core};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};
use std::cell::RefCell;

const MODULE: &str = "cs705.icf.settings.digital";

fn selection(value: &Bound<'_, PyAny>) -> PyResult<core::SelectionField> {
    match crate::icf::enum_value(value, MODULE, "SelectionField")? {
        0 => Ok(core::SelectionField::MyCall),
        1 => Ok(core::SelectionField::TxMessage),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "invalid SelectionField",
        )),
    }
}
fn text(value: &Bound<'_, PyAny>) -> PyResult<core::TextField> {
    match crate::icf::enum_value(value, MODULE, "TextField")? {
        0 => Ok(core::TextField::MyCall),
        1 => Ok(core::TextField::MyCallSuffix),
        2 => Ok(core::TextField::TxMessage),
        _ => Err(pyo3::exceptions::PyValueError::new_err("invalid TextField")),
    }
}

macro_rules! metadata {
    ($function:ident, $convert:ident, $method:ident, $ty:ty) => {
        #[pyfunction]
        fn $function(field: &Bound<'_, PyAny>) -> PyResult<$ty> {
            Ok($convert(field)?.$method())
        }
    };
}
metadata!(selection_native_name, selection, name, &'static str);
metadata!(selection_feature_id, selection, feature_id, &'static str);
metadata!(selection_key, selection, key, u16);
metadata!(selection_count, selection, count, usize);
metadata!(selection_active_base, selection, active_base, u8);
metadata!(text_native_name, text, name, &'static str);
metadata!(text_feature_id, text, feature_id, &'static str);
metadata!(text_key, text, key, u16);
metadata!(text_count, text, count, usize);
metadata!(text_width, text, width, usize);

#[pyfunction]
fn selection_find<'py>(py: Python<'py>, name_or_id: &str) -> PyResult<Bound<'py, PyAny>> {
    let field = core::SelectionField::find(name_or_id).map_err(crate::errors::csv)?;
    crate::icf::enum_result(py, MODULE, "SelectionField", field as i64)
}
#[pyfunction]
fn text_find<'py>(py: Python<'py>, name_or_id: &str) -> PyResult<Bound<'py, PyAny>> {
    let field = core::TextField::find(name_or_id).map_err(crate::errors::csv)?;
    crate::icf::enum_result(py, MODULE, "TextField", field as i64)
}
#[pyfunction]
fn selection_active_raw(field: &Bound<'_, PyAny>, image: &Icf) -> PyResult<u8> {
    let field = selection(field)?;
    image.with_ref(|image| Ok(field.active_raw(image)))
}
#[pyfunction]
fn selection_selected(
    field: &Bound<'_, PyAny>,
    image: &Icf,
    index: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let (field, index) = (selection(field)?, strict_usize(index)?);
    image.with_ref(|image| field.selected(image, index).map_err(crate::errors::csv))
}
#[pyfunction]
fn selection_edit(
    py: Python<'_>,
    field: &Bound<'_, PyAny>,
    image: &Icf,
    index: &Bound<'_, PyAny>,
    input: &ChoiceInput,
) -> PyResult<SelectionEffect> {
    let (field, index, input) = (selection(field)?, strict_usize(index)?, input.to_core(py)?);
    let result = image.with_mut(|image| {
        py.detach(|| field.edit(image, index, input).map_err(crate::errors::csv))
    })?;
    Ok(SelectionEffect::from_core(result))
}
#[pyfunction]
fn text_raw<'py>(
    py: Python<'py>,
    field: &Bound<'_, PyAny>,
    image: &Icf,
    index: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let (field, index) = (text(field)?, strict_usize(index)?);
    let value = image.with_ref(|image| {
        field
            .raw(image, index)
            .map(<[u8]>::to_vec)
            .map_err(crate::errors::csv)
    })?;
    Ok(PyBytes::new(py, &value))
}
#[pyfunction]
fn text_display<'py>(
    py: Python<'py>,
    field: &Bound<'_, PyAny>,
    image: &Icf,
    index: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let (field, index) = (text(field)?, strict_usize(index)?);
    let value = image.with_ref(|image| field.display(image, index).map_err(crate::errors::csv))?;
    Ok(PyBytes::new(py, &value))
}
#[pyfunction]
fn text_store_encoded_message(
    field: &Bound<'_, PyAny>,
    image: &Icf,
    index: &Bound<'_, PyAny>,
    bytes: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let (field, index, bytes) = (text(field)?, strict_usize(index)?, strict_bytes(bytes)?);
    image.with_mut(|image| {
        field
            .store_encoded_message(image, index, bytes)
            .map_err(crate::errors::csv)
    })
}

// This invocation owns callbacks only until native work and cleanup finish.
struct Nls {
    map: Py<PyAny>,
    upper: Py<PyAny>,
    interpreter: i64,
    errors: RefCell<CallbackError>,
}
impl Nls {
    fn new(py: Python<'_>, map: &Bound<'_, PyAny>, upper: &Bound<'_, PyAny>) -> PyResult<Self> {
        if !map.is_callable() || !upper.is_callable() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "expected NLS callables",
            ));
        }
        Ok(Self {
            map: map.clone().unbind(),
            upper: upper.clone().unbind(),
            interpreter: crate::scoped::interpreter(py)?,
            errors: RefCell::default(),
        })
    }
    fn call<T>(&self, call: impl FnOnce(Python<'_>) -> PyResult<T>) -> Result<T, csv::Error> {
        self.errors
            .borrow_mut()
            .invoke(|| {
                Python::attach(|py| {
                    if crate::scoped::interpreter(py)? != self.interpreter {
                        return Err(pyo3::exceptions::PyRuntimeError::new_err(
                            "NLS callback belongs to another interpreter",
                        ));
                    }
                    call(py)
                })
            })
            .map_err(|_| csv::Error::Operation("Python digital NLS callback failed".into()))
    }
    fn map(&self, source: &[u8], destination: &mut [u8]) -> Result<i32, csv::Error> {
        self.call(|py| crate::csv::nls::map_call(py, self.map.bind(py), source, destination))
    }
    fn upper(&self, destination: &mut [u8]) -> Result<u32, csv::Error> {
        self.call(|py| crate::csv::nls::upper_call(py, self.upper.bind(py), destination))
    }
}

#[pyfunction]
fn text_edit(
    py: Python<'_>,
    field: &Bound<'_, PyAny>,
    image: Py<Icf>,
    index: &Bound<'_, PyAny>,
    input: Py<FixedTextInput>,
    alignment: &Bound<'_, PyAny>,
) -> PyResult<TextEffect> {
    let (field, index, alignment) = (text(field)?, strict_usize(index)?, strict_u32(alignment)?);
    let value = {
        let mut image = image.get().take()?;
        input.get().with_core(py, |input| {
            py.detach(|| {
                field
                    .edit(&mut image, index, input, alignment)
                    .map_err(crate::errors::csv)
            })
        })?
    };
    Ok(TextEffect::from_core(value))
}
#[pyfunction]
/// Normalize the shared source through callback-scoped NLS buffers.
/// Input sanitation can survive later failure; Python callback exceptions propagate unchanged.
#[expect(
    clippy::too_many_arguments,
    reason = "The native text editor receives independent map and uppercase callbacks in addition to its edit inputs."
)]
fn text_edit_with_nls(
    py: Python<'_>,
    field: &Bound<'_, PyAny>,
    image: Py<Icf>,
    index: &Bound<'_, PyAny>,
    input: Py<FixedTextInput>,
    alignment: &Bound<'_, PyAny>,
    map: &Bound<'_, PyAny>,
    upper: &Bound<'_, PyAny>,
) -> PyResult<TextEffect> {
    let (field, index, alignment) = (text(field)?, strict_usize(index)?, strict_u32(alignment)?);
    let nls = Nls::new(py, map, upper)?;
    let (errors, result) = {
        let mut image = image.get().take()?;
        input.get().with_core(py, |input| {
            Ok(py.detach(move || {
                let result = field.edit_with_nls(
                    &mut image,
                    index,
                    input,
                    alignment,
                    |s, d| nls.map(s, d),
                    |d| nls.upper(d),
                );
                (nls.errors.into_inner(), result)
            }))
        })?
    };
    errors
        .finish(py, result.map_err(crate::errors::csv))
        .map(TextEffect::from_core)
}

macro_rules! value {
    ($name:ident { $($field:ident: $ty:ty => $set:ident;)* }) => {
        value!($name { $($field: $ty => $set;)* } {
            #[new] fn new($($field: &Bound<'_, PyAny>),*) -> PyResult<Self> {
                Ok(Self::from_core(core::$name { $($field: <$ty as Input>::input($field)?),* }))
            }
        });
    };
    ($name:ident { $($field:ident: $ty:ty => $set:ident;)* } { $($method:item)* }) => {
        #[pyclass(module = "cs705.icf.settings.digital", frozen)]
        pub struct $name { inner: Owner<core::$name> }
        impl $name {
            pub fn from_core(value: core::$name) -> Self { Self { inner: Owner::new(value) } }
        }
        #[pymethods]
        impl $name {
            $($method)*
            $(#[getter] fn $field<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
                let value = self.inner.with_ref(|v| Ok(v.$field.clone()))?;
                value.output(py)
            }
            #[setter] fn $set(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
                let value = <$ty as Input>::input(value)?;
                self.inner.with_mut(|v| { v.$field = value; Ok(()) })
            })*
        }
    };
}
value!(SelectionEffect { helper_accepted: bool => set_helper_accepted; stored: bool => set_stored; candidate: Option<i32> => set_candidate; active_raw: u8 => set_active_raw; notify_flags: u32 => set_notify_flags; popup_requested: bool => set_popup_requested; });
value!(AllowedView { prefix_count: u32 => set_prefix_count; bad: bool => set_bad; duplicate_warning: bool => set_duplicate_warning; parent_enable_raw: i64 => set_parent_enable_raw; raw: [u8; 8] => set_raw; display: Vec<u8> => set_display; });
value!(ReflectorView { valid: bool => set_valid; raw: Option<[u8; 8]> => set_raw; display: Vec<u8> => set_display; });

fn notifications(value: &Bound<'_, PyAny>) -> PyResult<Vec<u32>> {
    value.try_iter()?.map(|value| strict_u32(&value?)).collect()
}
value!(AllowedEffect { inserted: bool => set_inserted; stored: bool => set_stored; full_warning: bool => set_full_warning; view_bad: bool => set_view_bad; duplicate_warning: bool => set_duplicate_warning; state_out: u32 => set_state_out; notify_flags: u32 => set_notify_flags; display: Vec<u8> => set_display; } {
    #[new]
    #[expect(clippy::too_many_arguments, reason = "The constructor exposes each native allowed-call effect and its ordered notification list.")]
    fn new(
        inserted: &Bound<'_, PyAny>,
        stored: &Bound<'_, PyAny>,
        full_warning: &Bound<'_, PyAny>,
        view_bad: &Bound<'_, PyAny>,
        duplicate_warning: &Bound<'_, PyAny>,
        state_out: &Bound<'_, PyAny>,
        notify_flags: &Bound<'_, PyAny>,
        notifications: &Bound<'_, PyAny>,
        display: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self::from_core(core::AllowedEffect {
            inserted: strict_bool(inserted)?,
            stored: strict_bool(stored)?,
            full_warning: strict_bool(full_warning)?,
            view_bad: strict_bool(view_bad)?,
            duplicate_warning: strict_bool(duplicate_warning)?,
            state_out: strict_u32(state_out)?,
            notify_flags: strict_u32(notify_flags)?,
            notifications: self::notifications(notifications)?,
            display: strict_bytes(display)?.to_vec(),
        }))
    }
    #[staticmethod]
    fn default() -> Self {
        Self::from_core(core::AllowedEffect::default())
    }
    #[getter]
    fn notifications<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        let value = self.inner.with_ref(|v| Ok(v.notifications.clone()))?;
        PyTuple::new(py, value)
    }
    #[setter]
    fn set_notifications(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = notifications(value)?;
        self.inner.with_mut(|v| {
            v.notifications = value;
            Ok(())
        })
    }
});

#[pyfunction]
fn ur_physical_slots(image: &Icf) -> PyResult<Vec<usize>> {
    image.with_ref(|image| core::ur_physical_slots(image).map_err(crate::errors::csv))
}
#[pyfunction]
fn allowed_view(image: &Icf, index: &Bound<'_, PyAny>) -> PyResult<AllowedView> {
    let index = strict_u32(index)?;
    image.with_ref(|image| {
        core::allowed_view(image, index)
            .map(AllowedView::from_core)
            .map_err(crate::errors::csv)
    })
}
#[pyfunction]
fn reflector_view(image: &Icf, index: &Bound<'_, PyAny>) -> PyResult<ReflectorView> {
    let index = strict_u32(index)?;
    image.with_ref(|image| {
        core::reflector_view(image, index)
            .map(ReflectorView::from_core)
            .map_err(crate::errors::icf)
    })
}
#[pyfunction]
fn edit_allowed(
    py: Python<'_>,
    image: Py<Icf>,
    index: &Bound<'_, PyAny>,
    input_code: &Bound<'_, PyAny>,
    input: Py<FixedTextInput>,
    alignment: &Bound<'_, PyAny>,
    effect: Py<AllowedEffect>,
) -> PyResult<()> {
    let (index, input_code, alignment) = (
        strict_u32(index)?,
        strict_i32(input_code)?,
        strict_u32(alignment)?,
    );
    let mut image = image.get().take()?;
    let mut effect = effect.get().inner.take()?;
    input.get().with_core(py, |input| {
        py.detach(|| {
            core::edit_allowed(&mut image, index, input_code, input, alignment, &mut effect)
                .map_err(crate::errors::csv)
        })
    })
}
#[pyfunction]
/// Edit an allowed-call row with shared NLS callbacks; insertion may precede later failure.
/// The effect owner and image retain the committed prefix, not a rollback snapshot.
#[expect(
    clippy::too_many_arguments,
    reason = "The allowed-call operation keeps mutable input, effect, alignment and both NLS callbacks explicit."
)]
fn edit_allowed_with_nls(
    py: Python<'_>,
    image: Py<Icf>,
    index: &Bound<'_, PyAny>,
    input_code: &Bound<'_, PyAny>,
    input: Py<FixedTextInput>,
    alignment: &Bound<'_, PyAny>,
    effect: Py<AllowedEffect>,
    map: &Bound<'_, PyAny>,
    upper: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let (index, input_code, alignment) = (
        strict_u32(index)?,
        strict_i32(input_code)?,
        strict_u32(alignment)?,
    );
    let nls = Nls::new(py, map, upper)?;
    let (errors, result) = {
        let mut image = image.get().take()?;
        let mut effect = effect.get().inner.take()?;
        input.get().with_core(py, |input| {
            Ok(py.detach(move || {
                let result = core::edit_allowed_with_nls(
                    &mut image,
                    index,
                    input_code,
                    input,
                    alignment,
                    &mut effect,
                    |s, d| nls.map(s, d),
                    |d| nls.upper(d),
                );
                (nls.errors.into_inner(), result)
            }))
        })?
    };
    errors.finish(py, result.map_err(crate::errors::csv))
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "digital")?;
    crate::icf::add_enum(&m, "SelectionField", &[("MyCall", 0), ("TxMessage", 1)])?;
    crate::icf::add_enum(
        &m,
        "TextField",
        &[("MyCall", 0), ("MyCallSuffix", 1), ("TxMessage", 2)],
    )?;
    for name in ["SelectionField", "TextField"] {
        m.getattr(name)?.setattr("__module__", MODULE)?;
    }
    m.add_class::<SelectionEffect>()?;
    m.add_class::<AllowedView>()?;
    m.add_class::<AllowedEffect>()?;
    m.add_class::<ReflectorView>()?;
    macro_rules! functions { ($($name:ident),* $(,)?) => { $(m.add_function(wrap_pyfunction!($name, &m)?)?;)* }; }
    functions!(
        selection_native_name,
        selection_feature_id,
        selection_key,
        selection_count,
        selection_active_base,
        selection_find,
        selection_active_raw,
        selection_selected,
        selection_edit,
        text_native_name,
        text_feature_id,
        text_key,
        text_count,
        text_width,
        text_find,
        text_raw,
        text_display,
        text_store_encoded_message,
        text_edit,
        text_edit_with_nls,
        ur_physical_slots,
        allowed_view,
        reflector_view,
        edit_allowed,
        edit_allowed_with_nls
    );
    parent.add_submodule(&m)
}
