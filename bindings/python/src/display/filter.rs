//! Public Filter FIX calculations, with explicit source bytes.
use super::*;
#[pyfunction]
fn dd_rows_visible() -> bool {
    core::filter::dd_rows_visible()
}
#[pyfunction]
#[pyo3(signature=(image,key,index,source_pe))]
fn paint<'py>(
    py: Python<'py>,
    image: &crate::icf::Icf,
    key: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    source_pe: Option<&Bound<'_, PyAny>>,
) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
    let key = strict_u16(key)?;
    let index = strict_usize(index)?;
    let source = source_pe.map(strict_bytes).transpose()?;
    let bytes = image.with_ref(|image| {
        core::filter::paint(image, key, index, source).map_err(crate::errors::io)
    })?;
    Ok(pyo3::types::PyBytes::new(py, &bytes))
}
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "filter")?;
    m.add_function(wrap_pyfunction!(paint, &m)?)?;
    m.add_function(wrap_pyfunction!(dd_rows_visible, &m)?)?;
    parent.add_submodule(&m)
}
