//! Mapped ICF storage and native initializers with explicit captured sources.
use super::DecodedRecord;
use crate::csv::date_input::Clock;
use crate::icf::{Icf, context::Context, values::Input};
use crate::{conversion::*, errors};
use cs705_core::icf::{record as core, record_image::ChannelKind};
use pyo3::prelude::*;

#[pyfunction]
fn load_channel_image_into(
    py: Python<'_>,
    image: &Icf,
    kind: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
) -> PyResult<bool> {
    let (kind, index) = (ChannelKind::input(kind)?, strict_u32(index)?);
    let image = image.take()?;
    let mut holder = holder.take()?;
    py.detach(|| {
        core::load_channel_image_into(&image, kind, index, &mut holder).map_err(errors::icf)
    })
}

#[pyfunction]
fn store_channel_image_into(
    py: Python<'_>,
    image: &Icf,
    kind: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
) -> PyResult<()> {
    let (kind, index) = (ChannelKind::input(kind)?, strict_u32(index)?);
    let mut image = image.take()?;
    let mut holder = holder.take()?;
    py.detach(|| {
        core::store_channel_image_into(&mut image, kind, index, &mut holder).map_err(errors::icf)
    })
}

#[pyfunction]
#[pyo3(signature=(context, system_langid))]
fn repeater_default_context(
    context: &Context,
    system_langid: Option<&Bound<'_, PyAny>>,
) -> PyResult<crate::repeater::working::DefaultContext> {
    let context = context.with_ref(|context| Ok(*context))?;
    let langid = system_langid.map(strict_u16).transpose()?;
    Ok(crate::repeater::working::DefaultContext {
        inner: core::repeater_default_context(&context, langid).map_err(errors::icf)?,
    })
}

#[pyfunction]
fn repeater_reset_word(generation: &Bound<'_, PyAny>, which: &Bound<'_, PyAny>) -> PyResult<u16> {
    core::repeater_reset_word(strict_bytes(generation)?, strict_usize(which)?).map_err(errors::icf)
}

#[pyfunction]
fn initial_normal_channel_into(
    generation: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    source_selector: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
) -> PyResult<()> {
    let generation = strict_bytes(generation)?;
    let (index, source_selector) = (strict_u32(index)?, strict_usize(source_selector)?);
    holder.with_mut(|holder| {
        core::initial_normal_channel_into(generation, index, source_selector, holder)
            .map_err(errors::icf)
    })
}

#[pyfunction(name = "holder")]
#[pyo3(signature=(image, q, system_langid, index, binding, clock))]
fn gps_holder(
    image: &Icf,
    q: &Context,
    system_langid: Option<&Bound<'_, PyAny>>,
    index: &Bound<'_, PyAny>,
    binding: &Bound<'_, PyAny>,
    clock: &Clock,
) -> PyResult<DecodedRecord> {
    let q = q.with_ref(|q| Ok(*q))?;
    let langid = system_langid.map(strict_u16).transpose()?;
    let (index, binding, clock) = (strict_u32(index)?, strict_u32(binding)?, clock.native()?);
    image.with_ref(|image| {
        core::gps_initial::holder(image, &q, langid, index, binding, clock)
            .map(DecodedRecord::from_core)
            .map_err(errors::icf)
    })
}

#[pyfunction(name = "seed")]
#[pyo3(signature=(image, q, system_langid, clock))]
fn gps_seed(
    image: &Icf,
    q: &Context,
    system_langid: Option<&Bound<'_, PyAny>>,
    clock: &Clock,
) -> PyResult<crate::gps::Record> {
    let q = q.with_ref(|q| Ok(*q))?;
    let langid = system_langid.map(strict_u16).transpose()?;
    let clock = clock.native()?;
    image.with_ref(|image| {
        core::gps_initial::seed(image, &q, langid, clock)
            .map(|inner| crate::gps::Record { inner })
            .map_err(errors::icf)
    })
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(load_channel_image_into, m)?)?;
    m.add_function(wrap_pyfunction!(store_channel_image_into, m)?)?;
    m.add_function(wrap_pyfunction!(repeater_default_context, m)?)?;
    m.add_function(wrap_pyfunction!(repeater_reset_word, m)?)?;
    m.add_function(wrap_pyfunction!(initial_normal_channel_into, m)?)?;
    let gps = PyModule::new(m.py(), "gps_initial")?;
    gps.add_function(wrap_pyfunction!(gps_holder, &gps)?)?;
    gps.add_function(wrap_pyfunction!(gps_seed, &gps)?)?;
    m.add_submodule(&gps)
}
