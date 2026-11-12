//! Ordered tables keep duplicate labels and values; corrections are live state.
use super::*;
use cs705_core::csv::table as t;

#[pyclass(module = "cs705.csv.table")]
pub struct Entry {
    label: Vec<u8>,
    value: u32,
}
#[pymethods]
impl Entry {
    #[setter]
    fn set_label(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.label = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[setter]
    fn set_value(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.value = strict_u32(value)?;
        Ok(())
    }
    #[new]
    fn new(label: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            label: strict_bytes(label)?.to_vec(),
            value: strict_u32(value)?,
        })
    }
    #[getter]
    fn label<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.label)
    }
    #[getter]
    fn value(&self) -> u32 {
        self.value
    }
}
fn entries<'py>(value: &Bound<'py, PyAny>) -> PyResult<Vec<PyRef<'py, Entry>>> {
    value.try_iter()?.map(|v| Ok(v?.extract()?)).collect()
}
fn native_entries<'a>(values: &'a [PyRef<'_, Entry>]) -> Vec<t::Entry<'a>> {
    values
        .iter()
        .map(|v| t::Entry {
            label: &v.label,
            value: v.value,
        })
        .collect()
}
#[pyclass(module = "cs705.csv.table")]
pub struct Corrections {
    value: u32,
}
#[pymethods]
impl Corrections {
    #[new]
    fn new(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            value: strict_u32(value)?,
        })
    }
    #[getter]
    fn value(&self) -> u32 {
        self.value
    }
    #[setter]
    fn set_value(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.value = strict_u32(value)?;
        Ok(())
    }
}
#[pyfunction]
fn import_ascii(source: &Bound<'_, PyAny>, entries: &Bound<'_, PyAny>) -> PyResult<Option<u32>> {
    Ok(t::import_ascii(
        strict_bytes(source)?,
        &native_entries(&self::entries(entries)?),
    ))
}
#[pyfunction]
fn import_with(
    py: Python<'_>,
    source: &Bound<'_, PyAny>,
    entries: &Bound<'_, PyAny>,
    lower: &Bound<'_, PyAny>,
) -> PyResult<Option<u32>> {
    let entries = self::entries(entries)?;
    let mut errors = crate::callbacks::CallbackError::default();
    let result = t::import_with(strict_bytes(source)?, &native_entries(&entries), |v| {
        callback(
            &mut errors,
            lower
                .call1((PyBytes::new(py, v),))
                .and_then(|v| Ok(strict_bytes(&v)?.to_vec())),
        )
    });
    errors.finish(py, result.map_err(crate::errors::csv))
}
#[pyfunction]
fn export<'py>(
    py: Python<'py>,
    value: &Bound<'_, PyAny>,
    entries: &Bound<'_, PyAny>,
    not_found: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        t::export(
            strict_u32(value)?,
            &native_entries(&self::entries(entries)?),
            strict_bytes(not_found)?,
        ),
    ))
}
#[pyfunction]
fn mode_ascii(
    source: &Bound<'_, PyAny>,
    primary: &Bound<'_, PyAny>,
    secondary: &Bound<'_, PyAny>,
    mut corrections: PyRefMut<'_, Corrections>,
) -> PyResult<Option<u32>> {
    Ok(t::mode_ascii(
        strict_bytes(source)?,
        &native_entries(&entries(primary)?),
        &native_entries(&entries(secondary)?),
        &mut corrections.value,
    ))
}
#[pyfunction]
fn dtcs_ascii(source: &Bound<'_, PyAny>, entries: &Bound<'_, PyAny>) -> PyResult<Option<u32>> {
    Ok(t::dtcs_ascii(
        strict_bytes(source)?,
        &native_entries(&self::entries(entries)?),
    ))
}
#[pyfunction]
fn data_ascii(
    source: &Bound<'_, PyAny>,
    entries: &Bound<'_, PyAny>,
    mut corrections: PyRefMut<'_, Corrections>,
) -> PyResult<Option<u32>> {
    Ok(t::data_ascii(
        strict_bytes(source)?,
        &native_entries(&self::entries(entries)?),
        &mut corrections.value,
    ))
}
#[pyfunction]
fn on_ascii(
    source: &Bound<'_, PyAny>,
    entries: &Bound<'_, PyAny>,
    mut corrections: PyRefMut<'_, Corrections>,
) -> PyResult<Option<u32>> {
    Ok(t::on_ascii(
        strict_bytes(source)?,
        &native_entries(&self::entries(entries)?),
        &mut corrections.value,
    ))
}
#[pyfunction]
fn gps_group_ascii(source: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(t::gps_group_ascii(strict_bytes(source)?))
}
#[pyfunction]
fn gps_group_text<'py>(py: Python<'py>, group: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &t::gps_group_text(strict_u32(group)?).map_err(crate::errors::csv)?,
    ))
}
#[pyfunction]
fn portable_label<'py>(
    py: Python<'py>,
    kind: &Bound<'_, PyAny>,
    request: &super::payload::Request,
    decimal: &Bound<'_, PyAny>,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    Ok(t::portable_label(
        super::payload::kind(kind)?,
        &request.native(),
        strict_u8(decimal)?,
    )
    .map_err(crate::errors::csv)?
    .map(|v| PyBytes::new(py, &v)))
}
#[pyfunction]
fn builtin_import_ascii(
    kind: &Bound<'_, PyAny>,
    adapter: &Bound<'_, PyAny>,
    source: &Bound<'_, PyAny>,
) -> PyResult<Option<u32>> {
    Ok(t::builtin_import_ascii(
        super::payload::kind(kind)?,
        super::payload::adapter(adapter)?,
        strict_bytes(source)?,
    ))
}
#[pyfunction]
fn portable_import(
    adapter: &Bound<'_, PyAny>,
    source: &Bound<'_, PyAny>,
    decimal: &Bound<'_, PyAny>,
    crt_decimal: &Bound<'_, PyAny>,
    mut corrections: PyRefMut<'_, Corrections>,
) -> PyResult<u32> {
    t::portable_import(
        super::payload::adapter(adapter)?,
        strict_bytes(source)?,
        strict_u8(decimal)?,
        strict_u8(crt_decimal)?,
        &mut corrections.value,
    )
    .map_err(crate::errors::csv)
}
#[pyfunction]
fn tone_builtin_ascii(
    source: &Bound<'_, PyAny>,
    mtone: &Bound<'_, PyAny>,
    crt_point: &Bound<'_, PyAny>,
    mut corrections: PyRefMut<'_, Corrections>,
) -> PyResult<u32> {
    t::tone_builtin_ascii(
        strict_bytes(source)?,
        strict_bool(mtone)?,
        strict_u8(crt_point)?,
        &mut corrections.value,
    )
    .map_err(crate::errors::csv)
}
#[pyclass(module = "cs705.csv.table")]
pub struct ToneCandidate {
    value: u32,
    label: Vec<u8>,
}
#[pymethods]
impl ToneCandidate {
    #[setter]
    fn set_label(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.label = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[setter]
    fn set_value(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.value = strict_u32(value)?;
        Ok(())
    }
    #[new]
    fn new(value: &Bound<'_, PyAny>, label: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            value: strict_u32(value)?,
            label: strict_bytes(label)?.to_vec(),
        })
    }
    #[getter]
    fn value(&self) -> u32 {
        self.value
    }
    #[getter]
    fn label<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.label)
    }
}
#[pyfunction]
fn tone_ascii(
    py: Python<'_>,
    source: &Bound<'_, PyAny>,
    aliases: &Bound<'_, PyAny>,
    candidates: &Bound<'_, PyAny>,
    mut corrections: PyRefMut<'_, Corrections>,
    crt: &Bound<'_, PyAny>,
) -> PyResult<u32> {
    let aliases = entries(aliases)?;
    let candidates = candidates
        .try_iter()?
        .map(|v| Ok(v?.extract::<PyRef<'_, ToneCandidate>>()?))
        .collect::<PyResult<Vec<_>>>()?;
    let candidates = candidates
        .iter()
        .map(|c| t::ToneCandidate {
            value: c.value,
            label: &c.label,
        })
        .collect::<Vec<_>>();
    let mut errors = crate::callbacks::CallbackError::default();
    let result = t::tone_ascii(
        strict_bytes(source)?,
        &native_entries(&aliases),
        &candidates,
        &mut corrections.value,
        |v| {
            callback(
                &mut errors,
                crt.call1((PyBytes::new(py, v),))
                    .and_then(|v| Ok(v.cast::<pyo3::types::PyFloat>()?.extract::<f64>()?)),
            )
        },
    );
    errors.finish(py, result.map_err(crate::errors::csv))
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "table")?;
    m.add(
        "DTCS_CODES",
        pyo3::types::PyTuple::new(root.py(), t::DTCS_CODES)?,
    )?;
    m.add_class::<Entry>()?;
    m.add_class::<Corrections>()?;
    m.add_class::<ToneCandidate>()?;
    m.add_function(wrap_pyfunction!(import_ascii, &m)?)?;
    m.add_function(wrap_pyfunction!(import_with, &m)?)?;
    m.add_function(wrap_pyfunction!(export, &m)?)?;
    m.add_function(wrap_pyfunction!(mode_ascii, &m)?)?;
    m.add_function(wrap_pyfunction!(dtcs_ascii, &m)?)?;
    m.add_function(wrap_pyfunction!(data_ascii, &m)?)?;
    m.add_function(wrap_pyfunction!(on_ascii, &m)?)?;
    m.add_function(wrap_pyfunction!(gps_group_ascii, &m)?)?;
    m.add_function(wrap_pyfunction!(gps_group_text, &m)?)?;
    m.add_function(wrap_pyfunction!(portable_label, &m)?)?;
    m.add_function(wrap_pyfunction!(builtin_import_ascii, &m)?)?;
    m.add_function(wrap_pyfunction!(portable_import, &m)?)?;
    m.add_function(wrap_pyfunction!(tone_builtin_ascii, &m)?)?;
    m.add_function(wrap_pyfunction!(tone_ascii, &m)?)?;
    root.add_submodule(&m)
}
