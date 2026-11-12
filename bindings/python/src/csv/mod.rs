//! Byte-preserving native CSV bindings.
pub mod channel;
pub mod control;
pub mod date_input;
pub mod datetime;
pub mod dispatch;
pub mod history;
pub mod nls;
pub mod number;
pub mod payload;
pub mod table;

use crate::conversion::*;
use cs705_core::csv as core;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

#[derive(Clone, Copy, PartialEq, Eq, IntoPyObject, IntoPyObjectRef)]
#[pyo3(transparent)]
pub struct CheckedByte(pub u8);
impl<'a, 'py> FromPyObject<'a, 'py> for CheckedByte {
    type Error = PyErr;
    fn extract(value: pyo3::Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        strict_u8(&value).map(Self)
    }
}
#[derive(Clone, IntoPyObject, IntoPyObjectRef)]
#[pyo3(transparent)]
pub struct CheckedBytes(pub Vec<u8>);
impl<'a, 'py> FromPyObject<'a, 'py> for CheckedBytes {
    type Error = PyErr;
    fn extract(value: pyo3::Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        Ok(Self(strict_bytes(&value)?.to_vec()))
    }
}

pub(crate) fn callback<T>(
    errors: &mut crate::callbacks::CallbackError,
    result: PyResult<T>,
) -> Result<T, core::Error> {
    errors
        .capture(result)
        .map_err(|_| core::Error::Operation("Python CSV callback failed".into()))
}

pub(crate) fn bytes_list<'py>(
    py: Python<'py>,
    values: impl IntoIterator<Item = impl AsRef<[u8]>>,
) -> PyResult<Bound<'py, PyList>> {
    PyList::new(py, values.into_iter().map(|v| PyBytes::new(py, v.as_ref())))
}
pub(crate) fn byte_vectors(value: &Bound<'_, PyAny>) -> PyResult<Vec<Vec<u8>>> {
    value
        .try_iter()?
        .map(|v| Ok(strict_bytes(&v?)?.to_vec()))
        .collect()
}
pub(crate) fn indices(value: &Bound<'_, PyAny>) -> PyResult<Vec<usize>> {
    value.try_iter()?.map(|v| strict_usize(&v?)).collect()
}
pub(crate) fn add_enum(m: &Bound<'_, PyModule>, name: &str, variants: &[&str]) -> PyResult<()> {
    let entries = PyDict::new(m.py());
    for (index, variant) in variants.iter().enumerate() {
        entries.set_item(*variant, index)?;
    }
    let kwargs = PyDict::new(m.py());
    kwargs.set_item("module", format!("cs705.csv.{}", m.name()?.to_str()?))?;
    m.add(
        name,
        m.py()
            .import("enum")?
            .getattr("Enum")?
            .call((name, entries), Some(&kwargs))?,
    )
}
pub(crate) fn enum_index(value: &Bound<'_, PyAny>, module: &str, name: &str) -> PyResult<usize> {
    let ty = value.py().import(module)?.getattr(name)?;
    if !value.is_instance(&ty)? {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "expected {module}.{name}"
        )));
    }
    strict_usize(&value.getattr("value")?)
}
pub(crate) fn enum_result<'py>(
    py: Python<'py>,
    module: &str,
    name: &str,
    index: usize,
) -> PyResult<Bound<'py, PyAny>> {
    py.import(module)?.getattr(name)?.call1((index,))
}

#[pyclass(module = "cs705.csv", frozen)]
pub struct Locale {
    inner: crate::clone::Handle<core::Locale>,
}
impl Locale {
    pub fn from_core(value: core::Locale) -> Self {
        Self {
            inner: crate::clone::Handle::new(value),
        }
    }
    pub(crate) fn from_handle(inner: crate::clone::Handle<core::Locale>) -> Self {
        Self { inner }
    }
    pub fn native(&self) -> PyResult<core::Locale> {
        self.inner.with(|v| Ok(*v))
    }
}
#[pymethods]
impl Locale {
    #[setter]
    fn set_decimal(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u8(value)?;
        self.inner.with(|v| {
            v.decimal = value;
            Ok(())
        })
    }
    #[setter]
    fn set_list(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u8(value)?;
        self.inner.with(|v| {
            v.list = value;
            Ok(())
        })
    }
    fn __eq__(&self, other: &Self) -> PyResult<bool> {
        Ok(self.native()? == other.native()?)
    }
    #[new]
    fn new(decimal: &Bound<'_, PyAny>, list: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(core::Locale {
            decimal: strict_u8(decimal)?,
            list: strict_u8(list)?,
        }))
    }
    #[staticmethod]
    fn copied(decimal: &Bound<'_, PyAny>, list: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(core::Locale::copied(
            strict_u8(decimal)?,
            strict_u8(list)?,
        )))
    }
    #[getter]
    fn decimal(&self) -> PyResult<u8> {
        self.inner.with(|v| Ok(v.decimal))
    }
    #[getter]
    fn list(&self) -> PyResult<u8> {
        self.inner.with(|v| Ok(v.list))
    }
}

#[pyclass(module = "cs705.csv")]
pub struct Cells {
    inner: core::OwnedCells,
}
#[pymethods]
impl Cells {
    #[new]
    fn new(line: &Bound<'_, PyAny>, separator: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Cells::new(strict_bytes(line)?, strict_u8(separator)?).into_owned(),
        })
    }
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__<'py>(&mut self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.inner.next_cell().map(|v| PyBytes::new(py, v))
    }
    fn next_cell<'py>(&mut self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.__next__(py)
    }
    fn into_owned(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
}

#[pyclass(module = "cs705.csv", frozen)]
pub struct Reader {
    inner: core::Reader<'static>,
}
#[pymethods]
impl Reader {
    #[new]
    fn new(source: &Bound<'_, PyAny>, locale: &Locale) -> PyResult<Self> {
        Ok(Self {
            inner: core::Reader::new(strict_bytes(source)?, locale.native()?)
                .map_err(crate::errors::csv)?
                .into_owned(),
        })
    }
    fn header<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, self.inner.header())
    }
    fn original_bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, self.inner.original_bytes())
    }
    fn row<'py>(
        &self,
        py: Python<'py>,
        index: &Bound<'_, PyAny>,
    ) -> PyResult<Option<Bound<'py, PyBytes>>> {
        Ok(self
            .inner
            .row(strict_usize(index)?)
            .map(|v| PyBytes::new(py, v)))
    }
    fn rows(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<ReaderRows>> {
        Py::new(
            py,
            ReaderRows {
                reader: slf,
                next: 0,
            },
        )
    }
    fn locale(&self) -> Locale {
        Locale::from_core(self.inner.locale())
    }
    fn into_owned(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
}

#[pyclass(module = "cs705.csv")]
pub struct ReaderRows {
    reader: Py<Reader>,
    next: usize,
}
#[pymethods]
impl ReaderRows {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__<'py>(&mut self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        let reader = self.reader.borrow(py);
        let value = reader.inner.row(self.next)?;
        self.next += 1;
        Some(PyBytes::new(py, value))
    }
}

#[pyfunction]
fn trim<'py>(py: Python<'py>, value: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(py, core::trim(strict_bytes(value)?)))
}
#[pyfunction]
fn is_lead(byte: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(core::is_lead(strict_u8(byte)?))
}
#[pyfunction]
fn split_cells<'py>(
    py: Python<'py>,
    line: &Bound<'_, PyAny>,
    separator: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyList>> {
    bytes_list(
        py,
        core::split_cells(strict_bytes(line)?, strict_u8(separator)?),
    )
}
#[pyfunction]
fn column<'py>(
    py: Python<'py>,
    line: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    separator: &Bound<'_, PyAny>,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    let index = if index.is_none() {
        None
    } else {
        Some(strict_usize(index)?)
    };
    Ok(
        core::column(strict_bytes(line)?, index, strict_u8(separator)?)
            .map(|v| PyBytes::new(py, v)),
    )
}
#[pyfunction]
fn find_column_ascii(
    header: &Bound<'_, PyAny>,
    label: &Bound<'_, PyAny>,
    separator: &Bound<'_, PyAny>,
) -> PyResult<Option<usize>> {
    Ok(core::find_column_ascii(
        strict_bytes(header)?,
        strict_bytes(label)?,
        strict_u8(separator)?,
    ))
}
#[pyfunction]
fn find_column_with(
    py: Python<'_>,
    header: &Bound<'_, PyAny>,
    label: &Bound<'_, PyAny>,
    separator: &Bound<'_, PyAny>,
    lower: &Bound<'_, PyAny>,
) -> PyResult<Option<usize>> {
    let mut errors = crate::callbacks::CallbackError::default();
    let result = core::find_column_with(
        strict_bytes(header)?,
        strict_bytes(label)?,
        strict_u8(separator)?,
        |v| {
            callback(
                &mut errors,
                lower
                    .call1((PyBytes::new(py, v),))
                    .and_then(|v| Ok(strict_bytes(&v)?.to_vec())),
            )
        },
    );
    errors.finish(py, result.map_err(crate::errors::csv))
}
#[pyfunction]
fn fixed_text<'py>(
    py: Python<'py>,
    source: &Bound<'_, PyAny>,
    width: &Bound<'_, PyAny>,
) -> PyResult<(Bound<'py, PyBytes>, bool)> {
    let (bytes, clipped) = core::fixed_text_dynamic(strict_bytes(source)?, strict_usize(width)?)
        .map_err(crate::errors::csv)?;
    Ok((PyBytes::new(py, &bytes), clipped))
}
#[pyfunction]
fn fixed_text_into(source: &Bound<'_, PyAny>, output: &MutableBytes) -> PyResult<bool> {
    let source = strict_bytes(source)?;
    output.with_mut(|output| Ok(core::fixed_text_into(source, output)))
}
#[pyfunction]
fn export_text<'py>(
    py: Python<'py>,
    source: &Bound<'_, PyAny>,
    maximum: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &core::export_text(strict_bytes(source)?, strict_usize(maximum)?),
    ))
}
#[pyfunction]
fn export_call<'py>(
    py: Python<'py>,
    source: &Bound<'_, PyAny>,
    maximum: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &core::export_call(strict_bytes(source)?, strict_usize(maximum)?),
    ))
}
#[pyfunction]
fn join<'py>(
    py: Python<'py>,
    cells: &Bound<'_, PyAny>,
    separator: &Bound<'_, PyAny>,
    crlf: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &core::join(
            &byte_vectors(cells)?,
            strict_u8(separator)?,
            strict_bool(crlf)?,
        )
        .map_err(crate::errors::csv)?,
    ))
}

pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "csv")?;
    m.add("Error", root.getattr("Error")?)?;
    m.add_class::<Locale>()?;
    m.add_class::<Cells>()?;
    m.add("OwnedCells", m.getattr("Cells")?)?;
    m.add_class::<Reader>()?;
    m.add_class::<ReaderRows>()?;
    m.add_function(wrap_pyfunction!(trim, &m)?)?;
    m.add_function(wrap_pyfunction!(is_lead, &m)?)?;
    m.add_function(wrap_pyfunction!(split_cells, &m)?)?;
    m.add_function(wrap_pyfunction!(column, &m)?)?;
    m.add_function(wrap_pyfunction!(find_column_ascii, &m)?)?;
    m.add_function(wrap_pyfunction!(find_column_with, &m)?)?;
    m.add_function(wrap_pyfunction!(fixed_text, &m)?)?;
    m.add("fixed_text_dynamic", m.getattr("fixed_text")?)?;
    m.add_function(wrap_pyfunction!(fixed_text_into, &m)?)?;
    m.add_function(wrap_pyfunction!(export_text, &m)?)?;
    m.add_function(wrap_pyfunction!(export_call, &m)?)?;
    m.add_function(wrap_pyfunction!(join, &m)?)?;
    channel::register(&m)?;
    control::register(&m)?;
    datetime::register(&m)?;
    dispatch::register(&m)?;
    number::register(&m)?;
    payload::register(&m)?;
    table::register(&m)?;
    history::register(&m)?;
    nls::register(&m)?;
    date_input::register(&m)?;
    root.add_submodule(&m)
}
