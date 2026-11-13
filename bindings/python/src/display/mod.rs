//! Headless display state and explicitly supplied geometry and resources.
pub mod filter;
mod state;
mod values;
mod views;
use crate::callbacks::CallbackError;
use crate::{conversion::*, icf::document::Target};
use cs705_core::display as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
pub use state::Resources;
pub use values::{Row, Toolbar};
pub use views::{ViewState, Views};

pub(crate) fn optional_u32(v: &Bound<'_, PyAny>) -> PyResult<Option<u32>> {
    if v.is_none() {
        Ok(None)
    } else {
        Ok(Some(strict_u32(v)?))
    }
}

macro_rules! owner {
    ($name:ident, $native:ty) => {
        #[pyclass(module = "cs705.display", frozen)]
        pub struct $name {
            pub(crate) target: Target<$native>,
        }
        impl $name {
            pub fn from_core(value: $native) -> Self {
                Self {
                    target: Target::new(value),
                }
            }
            pub fn from_loan(loan: crate::scoped::Loan<$native>) -> Self {
                Self {
                    target: Target::Scoped(loan),
                }
            }
            pub fn with_ref<R>(&self, f: impl FnOnce(&$native) -> PyResult<R>) -> PyResult<R> {
                self.target.with_ref(f)
            }
            pub fn with_mut<R>(&self, f: impl FnOnce(&mut $native) -> PyResult<R>) -> PyResult<R> {
                self.target.with_mut(f)
            }
        }
    };
}
pub(crate) use owner;

macro_rules! scalars {
    ($(#[$constructor:meta])* $name:ident, $native:path; $($field:ident:$ty:ty=>$set:ident,$parse:ident);*; @extra {$($extra:tt)*}) => {
        owner!($name,$native);
        #[pymethods] impl $name {
            $(#[$constructor])* #[new] fn new($($field:&Bound<'_,PyAny>),*)->PyResult<Self>{use $native as Native;Ok(Self::from_core(Native{$($field:$parse($field)?),*}))}
            $(#[getter] fn $field(&self)->PyResult<$ty>{self.with_ref(|v|Ok(v.$field))}
            #[setter] fn $set(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let value=$parse(value)?;self.with_mut(|v|{v.$field=value;Ok(())})})*
            $($extra)*
        }
    }
}
scalars!(#[expect(clippy::too_many_arguments, reason = "The constructor exposes all ten independent native display words.")] Settings,core::Settings;
language:u32=>set_language,strict_u32;system_language:u32=>set_system_language,strict_u32;
font_percent:u32=>set_font_percent,strict_u32;toolbar:u32=>set_toolbar,strict_u32;split:u32=>set_split,strict_u32;
callsign_dialog:u32=>set_callsign_dialog,strict_u32;auto_correct:u32=>set_auto_correct,strict_u32;
fixed_name:u32=>set_fixed_name,strict_u32;column_bits:u32=>set_column_bits,strict_u32;column_bits_high:u32=>set_column_bits_high,strict_u32;
@extra {
    fn system_japanese(&self)->PyResult<bool>{self.with_ref(|v|Ok(v.system_japanese()))}
    fn input_options(&self)->PyResult<InputOptions>{self.with_ref(|v|Ok(InputOptions::from_core(v.input_options())))}
    fn registry_loaded(&self)->PyResult<Self>{self.with_ref(|v|Ok(Self::from_core(v.clone().registry_loaded())))}
    fn bit(&self,index:&Bound<'_,PyAny>)->PyResult<bool>{let index=strict_u32(index)?;self.with_ref(|v|Ok(v.bit(index)))}
    fn toggle_bit(&self,index:&Bound<'_,PyAny>)->PyResult<()>{let index=strict_u32(index)?;self.with_mut(|v|v.toggle_bit(index).map_err(crate::errors::io))}
    fn query_menu(&self,key:&Bound<'_,PyAny>,index:&Bound<'_,PyAny>,flags1d:&Bound<'_,PyAny>,flags1e:&Bound<'_,PyAny>)->PyResult<(u8,u8)>{
        let (key,index,mut a,mut b)=(strict_u32(key)?,strict_u32(index)?,strict_u8(flags1d)?,strict_u8(flags1e)?);
        self.with_ref(|v|{v.query_menu(key,index,&mut a,&mut b).map_err(crate::errors::io)?;Ok((a,b))})
    }
});
scalars!(FontMetrics,core::FontMetrics;em:i32=>set_em,strict_i32;advance:i32=>set_advance,strict_i32;line:i32=>set_line,strict_i32;dpi:i32=>set_dpi,strict_i32;padding:i32=>set_padding,strict_i32;explicit_height:i32=>set_explicit_height,strict_i32;
    @extra {fn validate(&self)->PyResult<()>{self.with_ref(|v|v.validate().map_err(crate::errors::io))}});
scalars!(InputOptions,core::InputOptions;callsign_dialog:u32=>set_callsign_dialog,strict_u32;auto_correct:u32=>set_auto_correct,strict_u32;gui_language:Option<u32> =>set_gui_language,optional_u32;
@extra {
    #[staticmethod] fn default()->Self{Self::from_core(core::InputOptions::default())}
    fn callsign_dialog_enabled(&self)->PyResult<bool>{self.with_ref(|v|Ok(v.callsign_dialog_enabled()))}
    fn auto_correct_enabled(&self)->PyResult<bool>{self.with_ref(|v|Ok(v.auto_correct_enabled()))}
    fn japanese(&self)->PyResult<bool>{self.with_ref(|v|v.japanese().map_err(crate::errors::io))}
});
scalars!(Queued,core::Queued;event:u32=>set_event,strict_u32;argument:u32=>set_argument,strict_u32;value:u32=>set_value,strict_u32;@extra {});
scalars!(Column,core::Column;key:u32=>set_key,strict_u32;width:i32=>set_width,strict_i32;measured_width:i32=>set_measured_width,strict_i32;hidden:bool=>set_hidden,strict_bool;boundary:bool=>set_boundary,strict_bool;x:i32=>set_x,strict_i32;@extra {});

#[pyfunction]
fn column_hidden(
    key: &Bound<'_, PyAny>,
    rx: &Bound<'_, PyAny>,
    settings: &Settings,
    mask: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let key = strict_u32(key)?;
    let rx = strict_bool(rx)?;
    let mask = strict_u32(mask)?;
    settings.with_ref(|s| Ok(core::column_hidden(key, rx, s, mask)))
}
#[pyfunction]
#[pyo3(signature=(current,node,settings,views,q))]
fn sql_mask(
    py: Python<'_>,
    current: &Bound<'_, PyAny>,
    node: &Bound<'_, PyAny>,
    settings: &Settings,
    views: &Views,
    q: Option<Py<crate::icf::context::Context>>,
) -> PyResult<u32> {
    let current = strict_bytes(current)?;
    let node = strict_u32(node)?;
    let q = state::context(py, q)?;
    settings.with_ref(|settings| {
        views.with_mut(|views| {
            core::sql_mask(current, node, settings, views, q.as_ref()).map_err(crate::errors::io)
        })
    })
}
#[pyfunction]
fn channel_cell<'py>(
    py: Python<'py>,
    holder: &crate::icf::record::DecodedRecord,
    key: &Bound<'_, PyAny>,
    row: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let key = strict_u32(key)?;
    let row = strict_u32(row)?;
    let bytes = holder.with_ref(|v| core::channel_cell(v, key, row).map_err(crate::errors::io))?;
    Ok(PyBytes::new(py, &bytes))
}
fn label_call(
    py: Python<'_>,
    label: &Py<PyAny>,
    errors: &mut CallbackError,
    source: &[u8],
    key: u32,
    value: u32,
) -> std::io::Result<Vec<u8>> {
    let result = (|| {
        let result = label
            .bind(py)
            .call1((PyBytes::new(py, source), key, value))?;
        Ok(strict_bytes(&result)?.to_vec())
    })();
    errors
        .capture(result)
        .map_err(|_| std::io::Error::other("Python display label callback failed"))
}
#[pyfunction]
#[pyo3(signature=(key,value,q,label))]
fn channel_value_paint<'py>(
    py: Python<'py>,
    key: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
    q: Option<Py<crate::icf::context::Context>>,
    label: Py<PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let key = strict_u32(key)?;
    let value = strict_u32(value)?;
    let q = state::context(py, q)?;
    let mut errors = CallbackError::default();
    let result = core::channel_value_paint(key, value, q.as_ref(), |s, k, v| {
        label_call(py, &label, &mut errors, s, k, v)
    });
    let bytes = errors.finish(py, result.map_err(crate::errors::io))?;
    Ok(PyBytes::new(py, &bytes))
}
#[pyfunction]
#[pyo3(signature=(holder,key,q,label))]
fn channel_paint<'py>(
    py: Python<'py>,
    holder: Py<crate::icf::record::DecodedRecord>,
    key: &Bound<'_, PyAny>,
    q: Option<Py<crate::icf::context::Context>>,
    label: Py<PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let key = strict_u32(key)?;
    let q = state::context(py, q)?;
    let holder = holder.get().take()?;
    let mut errors = CallbackError::default();
    let result = core::channel_paint(&holder, key, q.as_ref(), |s, k, v| {
        label_call(py, &label, &mut errors, s, k, v)
    });
    drop(holder);
    let bytes = errors.finish(py, result.map_err(crate::errors::io))?;
    Ok(PyBytes::new(py, &bytes))
}

pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "display")?;
    m.add_class::<Settings>()?;
    m.add_class::<FontMetrics>()?;
    m.add_class::<InputOptions>()?;
    m.add_class::<Queued>()?;
    m.add_class::<Column>()?;
    values::register(&m)?;
    views::register(&m)?;
    state::register(&m)?;
    filter::register(&m)?;
    m.add_function(wrap_pyfunction!(column_hidden, &m)?)?;
    m.add_function(wrap_pyfunction!(sql_mask, &m)?)?;
    m.add_function(wrap_pyfunction!(channel_cell, &m)?)?;
    m.add_function(wrap_pyfunction!(channel_value_paint, &m)?)?;
    m.add_function(wrap_pyfunction!(channel_paint, &m)?)?;
    root.add_submodule(&m)
}
