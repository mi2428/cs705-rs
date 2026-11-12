//! Direct working-image load/store entry points; preserve native partial writes.
use super::record::DecodedRecord;
use super::values::{Input, Output};

impl Input<'_> for core::ChannelKind {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(
            match super::enum_value(value, "cs705.icf.record_image", "ChannelKind")? {
                0 => Self::Normal,
                1 => Self::Call,
                _ => {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "invalid ChannelKind",
                    ));
                }
            },
        )
    }
}
impl Output for core::ChannelKind {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        super::enum_result(py, "cs705.icf.record_image", "ChannelKind", self as i64)
    }
}
#[pyclass(module = "cs705.icf.record_image")]
#[derive(Clone, Copy)]
pub struct ChannelAddress {
    pub inner: core::ChannelAddress,
}
impl ChannelAddress {
    pub fn from_core(inner: core::ChannelAddress) -> Self {
        Self { inner }
    }
}
macro_rules! address_fields {
    ($($field:ident:$ty:ty=>$set:ident;)*)=>{#[pymethods] impl ChannelAddress {
        #[new] fn new($($field:&Bound<'_,PyAny>),*)->PyResult<Self>{Ok(Self::from_core(core::ChannelAddress{$($field:<$ty as Input>::input($field)?),*}))}
        $(#[getter] fn $field<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{self.inner.$field.output(py)}
        #[setter] fn $set(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{self.inner.$field=<$ty as Input>::input(value)?;Ok(())})*
    }};
}
address_fields!(group:u32=>set_group;row:u32=>set_row;side:usize=>set_side;physical:u32=>set_physical;record_offset:usize=>set_record_offset;flag_offset:Option<usize> => set_flag_offset;);
#[pyfunction]
fn channel_address(
    image: &Bound<'_, PyAny>,
    kind: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
) -> PyResult<Option<ChannelAddress>> {
    core::channel_address(
        strict_bytes(image)?,
        core::ChannelKind::input(kind)?,
        strict_u32(index)?,
    )
    .map(|value| value.map(ChannelAddress::from_core))
    .map_err(errors::icf)
}
#[pyfunction]
fn load_channel_into(
    image: &Bound<'_, PyAny>,
    kind: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
) -> PyResult<bool> {
    let image = strict_bytes(image)?;
    let kind = core::ChannelKind::input(kind)?;
    let index = strict_u32(index)?;
    holder.with_mut(|h| core::load_channel_into(image, kind, index, h).map_err(errors::icf))
}
macro_rules! load_kind {
    ($($name:ident),*)=>{$(#[pyfunction]fn $name(image:&Bound<'_,PyAny>,kind:&Bound<'_,PyAny>,index:&Bound<'_,PyAny>,holder:&DecodedRecord)->PyResult<bool>{let image=strict_bytes(image)?;let kind=strict_u32(kind)?;let index=strict_u32(index)?;holder.with_mut(|h|core::$name(image,kind,index,h).map_err(errors::icf))})*};
}
load_kind!(load_range_into, load_compact_into, load_paired_into);
macro_rules! load_index {
    ($($name:ident),*)=>{$(#[pyfunction]fn $name(image:&Bound<'_,PyAny>,index:&Bound<'_,PyAny>,holder:&DecodedRecord)->PyResult<bool>{let image=strict_bytes(image)?;let index=strict_u32(index)?;holder.with_mut(|h|core::$name(image,index,h).map_err(errors::icf))})*};
}
load_index!(load_scan_into, load_callsign_into);

#[pyfunction]
fn store_channel_into(
    py: Python<'_>,
    image: Py<MutableBytes>,
    kind: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    holder: Py<DecodedRecord>,
    before_store: Py<PyAny>,
) -> PyResult<()> {
    let kind = core::ChannelKind::input(kind)?;
    let index = strict_u32(index)?;
    let mut image = image.get().take()?;
    let mut holder = holder.get().take()?;
    let mut failures = crate::callbacks::CallbackError::default();
    let result = core::store_channel_into(&mut image, kind, index, &mut holder, |event| {
        let result = super::record::with_channel_write(py, event, |event| {
            super::document::adapters::require_none(before_store.bind(py).call1((event,))?)
        });
        failures.capture(result).map_err(|_| {
            cs705_core::icf::Error::Operation("Python channel pre-store callback failed".into())
        })
    })
    .map_err(errors::icf);
    failures.finish(py, result)
}

macro_rules! channel_metadata {
    ($name:ident { $($field:ident:$ty:ty=>$setter:ident;)* }) => {
        #[pyclass(module="cs705.icf.record_image")]
        pub struct $name { inner: core::$name }
        #[pymethods] impl $name {
            #[new] fn new($($field:&Bound<'_,PyAny>),*)->PyResult<Self> {
                Ok(Self { inner: core::$name { $($field:<$ty as Input>::input($field)?,)* } })
            }
            $(#[getter] fn $field<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> { self.inner.$field.output(py) }
            #[setter] fn $setter(&mut self,value:&Bound<'_,PyAny>)->PyResult<()> { self.inner.$field=<$ty as Input>::input(value)?; Ok(()) })*
        }
    };
}
channel_metadata!(ChannelView {group:u32=>set_group;first:i32=>set_first;last:i32=>set_last;count:i32=>set_count;});
channel_metadata!(ChannelFilter {broad:bool=>set_broad;include_side1:bool=>set_include_side1;count_all:bool=>set_count_all;show_both:u32=>set_show_both;});

#[pyfunction]
fn print_channel_row(
    image: &Bound<'_, PyAny>,
    kind: &Bound<'_, PyAny>,
    group: &Bound<'_, PyAny>,
    row: &Bound<'_, PyAny>,
) -> PyResult<Option<bool>> {
    core::print_channel_row(
        strict_bytes(image)?,
        strict_u32(kind)?,
        strict_i32(group)?,
        strict_u32(row)?,
    )
    .map_err(errors::icf)
}
#[pyfunction]
fn channel_bad_count(
    image: &Bound<'_, PyAny>,
    view: &ChannelView,
    filter: &ChannelFilter,
    ranges: &super::frequency::Ranges,
    q: &super::context::Context,
    binding: &Bound<'_, PyAny>,
) -> PyResult<u32> {
    let image = strict_bytes(image)?;
    let binding = strict_u32(binding)?;
    q.with_ref(|q| {
        ranges.with_ref(|ranges| {
            core::channel_bad_count(image, view.inner, filter.inner, ranges, q, binding)
                .map_err(errors::icf)
        })
    })
}

macro_rules! store_kind {
    ($($name:ident),*) => {$(
        #[pyfunction]
        fn $name(py: Python<'_>, image: Py<MutableBytes>, kind: &Bound<'_, PyAny>, index: &Bound<'_, PyAny>, holder: Py<DecodedRecord>, before_store: Py<PyAny>) -> PyResult<()> {
            let kind = strict_u32(kind)?;
            let index = strict_u32(index)?;
            let mut image = image.get().take()?;
            let mut holder = holder.get().take()?;
            let mut failures = crate::callbacks::CallbackError::default();
            let result = core::$name(&mut image, kind, index, &mut holder, |offset, bytes| {
                failures.capture(before_store.bind(py).call1((offset, pyo3::types::PyBytes::new(py, bytes)))
                    .and_then(super::document::adapters::require_none))
                    .map_err(|_| cs705_core::icf::Error::Operation("Python record-image pre-store callback failed".into()))
            }).map_err(errors::icf);
            failures.finish(py, result)
        }
    )*};
}
store_kind!(store_range_into, store_compact_into, store_paired_into);

#[pyfunction]
fn store_scan_into(
    py: Python<'_>,
    image: Py<MutableBytes>,
    index: &Bound<'_, PyAny>,
    holder: Py<DecodedRecord>,
    before_store: Py<PyAny>,
) -> PyResult<()> {
    let index = strict_u32(index)?;
    let mut image = image.get().take()?;
    let mut holder = holder.get().take()?;
    let mut failures = crate::callbacks::CallbackError::default();
    let result = core::store_scan_into(&mut image, index, &mut holder, |offset, bytes| {
        failures
            .capture(
                before_store
                    .bind(py)
                    .call1((offset, pyo3::types::PyBytes::new(py, bytes)))
                    .and_then(super::document::adapters::require_none),
            )
            .map_err(|_| {
                cs705_core::icf::Error::Operation(
                    "Python record-image pre-store callback failed".into(),
                )
            })
    })
    .map_err(errors::icf);
    failures.finish(py, result)
}
#[pyfunction]
fn store_callsign_into(
    image: &MutableBytes,
    index: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
) -> PyResult<()> {
    let index = strict_u32(index)?;
    holder.with_ref(|h| {
        image.with_mut(|image| core::store_callsign_into(image, index, h).map_err(errors::icf))
    })
}
use crate::{conversion::*, errors};
use cs705_core::icf::record_image as core;
use pyo3::prelude::*;

#[pyfunction]
fn load_into(
    image: &Bound<'_, PyAny>,
    kind: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
) -> PyResult<bool> {
    let image = strict_bytes(image)?;
    let kind = strict_u32(kind)?;
    let index = strict_u32(index)?;
    holder.with_mut(|h| core::load_into(image, kind, index, h).map_err(errors::icf))
}
#[pyfunction]
fn store_into(
    image: &MutableBytes,
    kind: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
) -> PyResult<()> {
    let kind = strict_u32(kind)?;
    let index = strict_u32(index)?;
    holder.with_mut(|h| {
        image.with_mut(|image| core::store_into(image, kind, index, h).map_err(errors::icf))
    })
}
#[pyfunction]
fn callsign_count(image: &Bound<'_, PyAny>, kind: &Bound<'_, PyAny>) -> PyResult<u32> {
    core::callsign_count(strict_bytes(image)?, strict_u32(kind)?).map_err(errors::icf)
}
#[pyfunction]
fn print_dataset_count(
    image: &Bound<'_, PyAny>,
    kind: &Bound<'_, PyAny>,
    group: &Bound<'_, PyAny>,
) -> PyResult<u32> {
    core::print_dataset_count(strict_bytes(image)?, strict_u32(kind)?, strict_i32(group)?)
        .map_err(errors::icf)
}
#[pyfunction]
fn callsign_bad(image: &Bound<'_, PyAny>, holder: &DecodedRecord) -> PyResult<bool> {
    let image = strict_bytes(image)?;
    holder.with_ref(|h| core::callsign_bad(image, h).map_err(errors::icf))
}
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "record_image")?;
    m.add_function(wrap_pyfunction!(load_into, &m)?)?;
    m.add_function(wrap_pyfunction!(store_into, &m)?)?;
    m.add_function(wrap_pyfunction!(callsign_count, &m)?)?;
    m.add_function(wrap_pyfunction!(print_dataset_count, &m)?)?;
    m.add_function(wrap_pyfunction!(callsign_bad, &m)?)?;
    m.add_class::<ChannelAddress>()?;
    m.add_class::<ChannelView>()?;
    m.add_class::<ChannelFilter>()?;
    m.add_function(wrap_pyfunction!(store_channel_into, &m)?)?;
    m.add_function(wrap_pyfunction!(print_channel_row, &m)?)?;
    m.add_function(wrap_pyfunction!(channel_bad_count, &m)?)?;
    super::add_enum(&m, "ChannelKind", &[("Normal", 0), ("Call", 1)])?;
    m.add_function(wrap_pyfunction!(channel_address, &m)?)?;
    m.add_function(wrap_pyfunction!(load_channel_into, &m)?)?;
    m.add_function(wrap_pyfunction!(load_range_into, &m)?)?;
    m.add_function(wrap_pyfunction!(load_compact_into, &m)?)?;
    m.add_function(wrap_pyfunction!(load_paired_into, &m)?)?;
    m.add_function(wrap_pyfunction!(load_scan_into, &m)?)?;
    m.add_function(wrap_pyfunction!(load_callsign_into, &m)?)?;
    m.add_function(wrap_pyfunction!(store_callsign_into, &m)?)?;
    m.add_function(wrap_pyfunction!(store_range_into, &m)?)?;
    m.add_function(wrap_pyfunction!(store_compact_into, &m)?)?;
    m.add_function(wrap_pyfunction!(store_paired_into, &m)?)?;
    m.add_function(wrap_pyfunction!(store_scan_into, &m)?)?;
    parent.add_submodule(&m)
}
