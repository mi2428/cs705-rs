//! Explicit native ANSI mapping with expiring views of native destinations.
use super::*;
use crate::scoped::{Frame, Loan};
use cs705_core::csv::nls as n;
use pyo3::types::{PySlice, PySliceMethods, slice::PySliceIndices};

enum ByteIndex {
    One(usize),
    Slice(PySliceIndices),
}
fn byte_index(index: &Bound<'_, PyAny>, length: usize) -> PyResult<ByteIndex> {
    if let Ok(slice) = index.cast::<PySlice>() {
        for field in ["start", "stop", "step"] {
            let value = slice.getattr(field)?;
            if !value.is_none() {
                strict_isize(&value)?;
            }
        }
        return Ok(ByteIndex::Slice(slice.indices(length as isize)?));
    }
    let index = strict_isize(index)?;
    let index = if index < 0 {
        length as isize + index
    } else {
        index
    };
    if index < 0 || index as usize >= length {
        return Err(pyo3::exceptions::PyIndexError::new_err(
            "byte index outside native buffer",
        ));
    }
    Ok(ByteIndex::One(index as usize))
}

#[pyclass(module = "cs705.csv.nls", frozen)]
pub struct Bytes {
    loan: Loan<[u8]>,
}
impl Bytes {
    pub(crate) fn from_loan(loan: crate::scoped::Loan<[u8]>) -> Self {
        Self { loan }
    }
}
#[pymethods]
impl Bytes {
    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        self.loan.with_ref(py, |v| Ok(v.len()))
    }
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.loan.with_ref(py, |v| Ok(PyBytes::new(py, v)))
    }
    fn __getitem__<'py>(
        &self,
        py: Python<'py>,
        index: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let index = byte_index(index, self.loan.with_ref(py, |v| Ok(v.len()))?)?;
        self.loan.with_ref(py, |v| match index {
            ByteIndex::One(at) => Ok(v[at].into_pyobject(py)?.into_any()),
            ByteIndex::Slice(s) => {
                let bytes: Vec<u8> = (0..s.slicelength)
                    .map(|i| v[(s.start + s.step * i as isize) as usize])
                    .collect();
                Ok(PyBytes::new(py, &bytes).into_any())
            }
        })
    }
    fn __setitem__(
        &self,
        py: Python<'_>,
        index: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        match byte_index(index, self.loan.with_ref(py, |v| Ok(v.len()))?)? {
            ByteIndex::One(at) => {
                let value = strict_u8(value)?;
                self.loan.with_mut(py, |v| {
                    v[at] = value;
                    Ok(())
                })
            }
            ByteIndex::Slice(s) => {
                let value = strict_bytes(value)?;
                if value.len() != s.slicelength {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "native slice assignment cannot change length",
                    ));
                }
                self.loan.with_mut(py, |v| {
                    for (i, byte) in value.iter().enumerate() {
                        v[(s.start + s.step * i as isize) as usize] = *byte;
                    }
                    Ok(())
                })
            }
        }
    }
    fn write(
        &self,
        py: Python<'_>,
        start: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let start = strict_usize(start)?;
        let value = strict_bytes(value)?;
        self.loan.with_mut(py, |v| {
            let end = start.checked_add(value.len()).ok_or_else(|| {
                pyo3::exceptions::PyIndexError::new_err("native buffer range overflow")
            })?;
            v.get_mut(start..end)
                .ok_or_else(|| {
                    pyo3::exceptions::PyIndexError::new_err("write outside native buffer")
                })?
                .copy_from_slice(value);
            Ok(())
        })
    }
}
pub(crate) fn map_call(
    py: Python<'_>,
    native: &Bound<'_, PyAny>,
    source: &[u8],
    destination: &mut [u8],
) -> PyResult<i32> {
    Frame::with_mut(py, destination, |loan| {
        strict_i32(&native.call1((PyBytes::new(py, source), Bytes::from_loan(loan)))?)
    })
}
pub(crate) fn upper_call(
    py: Python<'_>,
    native: &Bound<'_, PyAny>,
    destination: &mut [u8],
) -> PyResult<u32> {
    Frame::with_mut(py, destination, |loan| {
        strict_u32(&native.call1((Bytes::from_loan(loan),))?)
    })
}
#[pyfunction]
fn sanitize(bytes: &MutableBytes, replacement: &Bound<'_, PyAny>) -> PyResult<bool> {
    let replacement = strict_u8(replacement)?;
    bytes.with_mut(|bytes| Ok(n::sanitize(bytes, replacement)))
}
#[pyfunction]
fn name_candidate<'py>(
    py: Python<'py>,
    shared: &MutableBytes,
    maximum: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let maximum = strict_usize(maximum)?;
    shared.with_mut(|shared| {
        Ok(PyBytes::new(
            py,
            &n::name_candidate(shared, maximum).map_err(crate::errors::csv)?,
        ))
    })
}
#[pyfunction]
fn trim_candidate<'py>(
    py: Python<'py>,
    source: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(py, &n::trim_candidate(strict_bytes(source)?)))
}
#[pyfunction]
fn fold_callsign(raw: &MutableBytes) -> PyResult<bool> {
    raw.with_mut(|raw| {
        let raw: &mut [u8; 8] = raw
            .try_into()
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("expected exactly 8 bytes"))?;
        Ok(n::fold_callsign(raw))
    })
}
#[pyfunction]
fn ascii_map(source: &Bound<'_, PyAny>, destination: &MutableBytes) -> PyResult<i32> {
    let source = strict_bytes(source)?;
    destination.with_mut(|d| n::ascii_map(source, d).map_err(crate::errors::csv))
}
#[pyfunction]
fn platform_map(source: &Bound<'_, PyAny>, destination: &MutableBytes) -> PyResult<i32> {
    let source = strict_bytes(source)?;
    destination.with_mut(|d| n::platform_map(source, d).map_err(crate::errors::csv))
}
#[pyfunction]
fn platform_uppercase(bytes: &MutableBytes) -> PyResult<u32> {
    bytes.with_mut(|bytes| n::platform_uppercase(bytes).map_err(crate::errors::csv))
}
#[pyfunction]
fn map_copy_back(
    py: Python<'_>,
    shared: Py<MutableBytes>,
    native: &Bound<'_, PyAny>,
) -> PyResult<Option<i32>> {
    let mut errors = crate::callbacks::CallbackError::default();
    let result = {
        let mut shared = shared.get().take()?;
        n::map_copy_back(shared.as_mut_slice(), |s, d| {
            callback(&mut errors, map_call(py, native, s, d))
        })
        .map_err(crate::errors::csv)
    };
    errors.finish(py, result)
}
#[pyfunction]
fn uppercase_copy<'py>(
    py: Python<'py>,
    source: &Bound<'_, PyAny>,
    native: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let mut errors = crate::callbacks::CallbackError::default();
    let result = n::uppercase_copy(strict_bytes(source)?, |d| {
        callback(&mut errors, upper_call(py, native, d))
    });
    errors
        .finish(py, result.map_err(crate::errors::csv))
        .map(|v| PyBytes::new(py, &v))
}
#[pyfunction]
fn callsign_fixed<'py>(
    py: Python<'py>,
    shared: Py<MutableBytes>,
    native: &Bound<'_, PyAny>,
) -> PyResult<(Bound<'py, PyBytes>, bool)> {
    let mut errors = crate::callbacks::CallbackError::default();
    let result = {
        let mut shared = shared.get().take()?;
        n::callsign_fixed(shared.as_mut_slice(), |s, d| {
            callback(&mut errors, map_call(py, native, s, d))
        })
        .map_err(crate::errors::csv)
    };
    errors
        .finish(py, result)
        .map(|(v, b)| (PyBytes::new(py, &v), b))
}
#[pyclass(module = "cs705.csv.nls")]
pub struct Profile {
    uppercase: bool,
    keep_kana: bool,
    keep_spaces: bool,
    punctuation_exceptions: Vec<u8>,
    maximum: usize,
}
#[pymethods]
impl Profile {
    #[setter]
    fn set_uppercase(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.uppercase = strict_bool(value)?;
        Ok(())
    }
    #[setter]
    fn set_keep_kana(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.keep_kana = strict_bool(value)?;
        Ok(())
    }
    #[setter]
    fn set_keep_spaces(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.keep_spaces = strict_bool(value)?;
        Ok(())
    }
    #[setter]
    fn set_punctuation_exceptions(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.punctuation_exceptions = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[setter]
    fn set_maximum(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.maximum = strict_usize(value)?;
        Ok(())
    }
    #[new]
    fn new(
        uppercase: &Bound<'_, PyAny>,
        keep_kana: &Bound<'_, PyAny>,
        keep_spaces: &Bound<'_, PyAny>,
        punctuation_exceptions: &Bound<'_, PyAny>,
        maximum: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            uppercase: strict_bool(uppercase)?,
            keep_kana: strict_bool(keep_kana)?,
            keep_spaces: strict_bool(keep_spaces)?,
            punctuation_exceptions: strict_bytes(punctuation_exceptions)?.to_vec(),
            maximum: strict_usize(maximum)?,
        })
    }
    #[getter]
    fn uppercase(&self) -> bool {
        self.uppercase
    }
    #[getter]
    fn keep_kana(&self) -> bool {
        self.keep_kana
    }
    #[getter]
    fn keep_spaces(&self) -> bool {
        self.keep_spaces
    }
    #[getter]
    fn punctuation_exceptions<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.punctuation_exceptions)
    }
    #[getter]
    fn maximum(&self) -> usize {
        self.maximum
    }
}
#[pyfunction]
fn whole_text<'py>(
    py: Python<'py>,
    shared: Py<MutableBytes>,
    profile: &Profile,
    map: &Bound<'_, PyAny>,
    upper: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let errors = std::cell::RefCell::new(crate::callbacks::CallbackError::default());
    let profile = n::Profile {
        uppercase: profile.uppercase,
        keep_kana: profile.keep_kana,
        keep_spaces: profile.keep_spaces,
        punctuation_exceptions: &profile.punctuation_exceptions,
        maximum: profile.maximum,
    };
    let result = {
        let mut shared = shared.get().take()?;
        n::whole_text(
            shared.as_mut_slice(),
            profile,
            |s, d| callback(&mut errors.borrow_mut(), map_call(py, map, s, d)),
            |d| callback(&mut errors.borrow_mut(), upper_call(py, upper, d)),
        )
        .map_err(crate::errors::csv)
    };
    errors
        .into_inner()
        .finish(py, result)
        .map(|v| PyBytes::new(py, &v))
}
#[pyfunction]
fn callsign_correction_count(
    c: &Bound<'_, PyAny>,
    name_changed: &Bound<'_, PyAny>,
) -> PyResult<u32> {
    Ok(n::callsign_correction_count(
        strict_u32(c)?,
        strict_bool(name_changed)?,
    ))
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "nls")?;
    m.add_class::<Bytes>()?;
    m.add_class::<Profile>()?;
    m.add_function(wrap_pyfunction!(sanitize, &m)?)?;
    m.add_function(wrap_pyfunction!(name_candidate, &m)?)?;
    m.add_function(wrap_pyfunction!(trim_candidate, &m)?)?;
    m.add_function(wrap_pyfunction!(fold_callsign, &m)?)?;
    m.add_function(wrap_pyfunction!(ascii_map, &m)?)?;
    m.add_function(wrap_pyfunction!(platform_map, &m)?)?;
    m.add_function(wrap_pyfunction!(platform_uppercase, &m)?)?;
    m.add_function(wrap_pyfunction!(map_copy_back, &m)?)?;
    m.add_function(wrap_pyfunction!(uppercase_copy, &m)?)?;
    m.add_function(wrap_pyfunction!(callsign_fixed, &m)?)?;
    m.add_function(wrap_pyfunction!(whole_text, &m)?)?;
    m.add_function(wrap_pyfunction!(callsign_correction_count, &m)?)?;
    root.add_submodule(&m)
}
