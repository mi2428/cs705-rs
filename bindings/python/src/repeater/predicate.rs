//! Native repeater predicates with explicit image, captured B and live Q inputs.
use crate::{
    conversion::*,
    errors,
    icf::{context::Context, frequency::Ranges, record::DecodedRecord},
};
use cs705_core::repeater::predicate as core;
use pyo3::prelude::*;

#[pyfunction]
fn callsign_bad(
    image: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
    ranges: &Ranges,
) -> PyResult<bool> {
    let image = strict_bytes(image)?;
    holder.with_ref(|h| ranges.with_ref(|b| core::callsign_bad(image, h, b).map_err(errors::icf)))
}
#[pyfunction]
fn frequency_bad(holder: &DecodedRecord, ranges: &Ranges, q: &Context) -> PyResult<bool> {
    let q = q.with_ref(|q| Ok(*q))?;
    holder.with_ref(|h| ranges.with_ref(|b| Ok(core::frequency_bad(h, b, &q))))
}
#[pyfunction]
fn duplex_bad(holder: &DecodedRecord) -> PyResult<bool> {
    holder.with_ref(|h| Ok(core::duplex_bad(h)))
}
#[pyfunction]
fn numeric_frequency_bad(holder: &DecodedRecord) -> PyResult<bool> {
    holder.with_ref(|h| Ok(core::numeric_frequency_bad(h)))
}
#[pyfunction]
fn row_bad(
    image: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
    numeric_only: &Bound<'_, PyAny>,
    ranges: &Ranges,
    q: &Context,
) -> PyResult<bool> {
    let (image, numeric) = (strict_bytes(image)?, strict_bool(numeric_only)?);
    let q = q.with_ref(|q| Ok(*q))?;
    holder.with_ref(|h| {
        ranges.with_ref(|b| core::row_bad(image, h, numeric, b, &q).map_err(errors::icf))
    })
}
#[pyfunction]
fn group_bad(
    image: &Bound<'_, PyAny>,
    group: &Bound<'_, PyAny>,
    numeric_only: &Bound<'_, PyAny>,
    ranges: &Ranges,
    q: &Context,
    binding: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let (image, g, n, binding) = (
        strict_bytes(image)?,
        strict_usize(group)?,
        strict_bool(numeric_only)?,
        strict_u32(binding)?,
    );
    let q = q.with_ref(|q| Ok(*q))?;
    ranges.with_ref(|b| core::group_bad(image, g, n, b, &q, binding).map_err(errors::icf))
}
#[pyfunction]
fn all_groups_bad(
    image: &Bound<'_, PyAny>,
    ranges: &Ranges,
    q: &Context,
    binding: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let (image, binding) = (strict_bytes(image)?, strict_u32(binding)?);
    let q = q.with_ref(|q| Ok(*q))?;
    ranges.with_ref(|b| core::all_groups_bad(image, b, &q, binding).map_err(errors::icf))
}
#[pyfunction]
fn home_usable(
    image: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
    ranges: &Ranges,
    q: &Context,
) -> PyResult<bool> {
    let image = strict_bytes(image)?;
    let q = q.with_ref(|q| Ok(*q))?;
    holder
        .with_ref(|h| ranges.with_ref(|b| core::home_usable(image, h, b, &q).map_err(errors::icf)))
}
#[pyfunction]
fn home_usable_count(
    image: &Bound<'_, PyAny>,
    group: &Bound<'_, PyAny>,
    ranges: &Ranges,
    q: &Context,
    binding: &Bound<'_, PyAny>,
) -> PyResult<u32> {
    let (image, g, binding) = (
        strict_bytes(image)?,
        strict_usize(group)?,
        strict_u32(binding)?,
    );
    let q = q.with_ref(|q| Ok(*q))?;
    ranges.with_ref(|b| core::home_usable_count(image, g, b, &q, binding).map_err(errors::icf))
}
#[pyfunction]
fn home_row_usable(
    image: &Bound<'_, PyAny>,
    group: &Bound<'_, PyAny>,
    relative: &Bound<'_, PyAny>,
    ranges: &Ranges,
    q: &Context,
    binding: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let (image, g, r, binding) = (
        strict_bytes(image)?,
        strict_usize(group)?,
        strict_i32(relative)?,
        strict_u32(binding)?,
    );
    let q = q.with_ref(|q| Ok(*q))?;
    ranges.with_ref(|b| core::home_row_usable(image, g, r, b, &q, binding).map_err(errors::icf))
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "predicate")?;
    m.add_function(wrap_pyfunction!(callsign_bad, &m)?)?;
    m.add_function(wrap_pyfunction!(frequency_bad, &m)?)?;
    m.add_function(wrap_pyfunction!(duplex_bad, &m)?)?;
    m.add_function(wrap_pyfunction!(numeric_frequency_bad, &m)?)?;
    m.add_function(wrap_pyfunction!(row_bad, &m)?)?;
    m.add_function(wrap_pyfunction!(group_bad, &m)?)?;
    m.add_function(wrap_pyfunction!(all_groups_bad, &m)?)?;
    m.add_function(wrap_pyfunction!(home_usable, &m)?)?;
    m.add_function(wrap_pyfunction!(home_usable_count, &m)?)?;
    m.add_function(wrap_pyfunction!(home_row_usable, &m)?)?;
    root.add_submodule(&m)
}
