//! Strict shared boundary conversions; no domain defaults.
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyByteArray, PyBytes, PyInt, PyRange};

macro_rules! integer {
    ($($name:ident: $ty:ty),* $(,)?) => {$ (
        pub fn $name(value: &Bound<'_, PyAny>) -> PyResult<$ty> {
            if value.is_instance_of::<PyBool>() || !value.is_instance_of::<PyInt>() {
                return Err(PyTypeError::new_err("expected an integer, not bool"));
            }
            value.extract::<$ty>()
        }
    )*};
}
integer!(strict_u8: u8, strict_u16: u16, strict_u32: u32, strict_u64: u64,
    strict_usize: usize, strict_i8: i8, strict_i16: i16, strict_i32: i32,
    strict_i64: i64, strict_isize: isize);

pub fn strict_bool(value: &Bound<'_, PyAny>) -> PyResult<bool> {
    value.cast::<PyBool>()?.extract()
}
pub fn strict_bytes<'a>(value: &'a Bound<'_, PyAny>) -> PyResult<&'a [u8]> {
    Ok(value.cast::<PyBytes>()?.as_bytes())
}
pub fn fixed_bytes<const N: usize>(value: &Bound<'_, PyAny>) -> PyResult<[u8; N]> {
    strict_bytes(value)?
        .try_into()
        .map_err(|_| PyValueError::new_err(format!("expected exactly {N} bytes")))
}

pub fn strict_range(value: &Bound<'_, PyAny>) -> PyResult<std::ops::Range<usize>> {
    let range = value.cast::<PyRange>()?;
    if strict_isize(&range.getattr("step")?)? != 1 {
        return Err(PyValueError::new_err("expected range with step 1"));
    }
    Ok(strict_usize(&range.getattr("start")?)?..strict_usize(&range.getattr("stop")?)?)
}

/// Owned bytes; construction from bytes or bytearray copies the input.
/// to_bytes() returns a snapshot, not an in-place Python output buffer. No buffer
/// protocol is exposed. Busy access raises RuntimeError; completed writes remain
/// visible after a callback or native operation fails.
#[pyclass(module = "cs705", frozen)]
pub struct MutableBytes {
    owner: crate::owner::Owner<Vec<u8>>,
}
impl MutableBytes {
    pub fn from_core(data: Vec<u8>) -> Self {
        Self {
            owner: crate::owner::Owner::new(data),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&[u8]) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_ref(|data| f(data.as_slice()))
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut [u8]) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_mut(|data| f(data.as_mut_slice()))
    }
    pub fn take(&self) -> PyResult<crate::owner::Taken<'_, Vec<u8>>> {
        self.owner.take()
    }
}
#[pymethods]
impl MutableBytes {
    #[new]
    fn new(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let data = if let Ok(bytes) = value.cast::<PyBytes>() {
            bytes.as_bytes().to_vec()
        } else if let Ok(bytes) = value.cast::<PyByteArray>() {
            bytes.to_vec()
        } else {
            return Err(PyTypeError::new_err("expected bytes or bytearray"));
        };
        Ok(Self::from_core(data))
    }
    fn __len__(&self) -> PyResult<usize> {
        self.with_ref(|data| Ok(data.len()))
    }
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.with_ref(|data| Ok(PyBytes::new(py, data)))
    }
}

#[pyclass(module = "cs705", frozen)]
/// Mutable scalar accepting only real bool values, never integer substitutes.
/// Busy access raises RuntimeError; completed changes survive operation failure.
pub struct Bool {
    owner: crate::owner::Owner<bool>,
}
impl Bool {
    pub fn from_core(value: bool) -> Self {
        Self {
            owner: crate::owner::Owner::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&bool) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_ref(f)
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut bool) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_mut(f)
    }
    pub fn take(&self) -> PyResult<crate::owner::Taken<'_, bool>> {
        self.owner.take()
    }
}
#[pymethods]
impl Bool {
    #[new]
    fn new(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(strict_bool(value)?))
    }
    #[getter]
    fn value(&self) -> PyResult<bool> {
        self.with_ref(|value| Ok(*value))
    }
    #[setter]
    fn set_value(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.with_mut(|target| {
            *target = value;
            Ok(())
        })
    }
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MutableBytes>()?;
    m.add_class::<Bool>()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundary_contract() {
        Python::initialize();
        Python::attach(|py| {
            assert!(strict_u8(PyBool::new(py, true).as_any()).is_err());
            assert!(strict_u8(&256u16.into_pyobject(py).unwrap().into_any()).is_err());
            let input = PyBytes::new(py, b"ab");
            assert_eq!(fixed_bytes::<2>(input.as_any()).unwrap(), *b"ab");
            assert!(fixed_bytes::<1>(input.as_any()).is_err());
            let holder = MutableBytes::new(input.as_any()).unwrap();
            let result: PyResult<()> = holder.with_mut(|data| {
                data[0] = b'z';
                assert!(holder.with_ref(|_| Ok(())).is_err());
                Err(PyValueError::new_err("injected mutation failure"))
            });
            assert!(result.is_err());
            assert_eq!(holder.with_ref(|data| Ok(data.to_vec())).unwrap(), b"zb");
            let mut taken = holder.take().unwrap();
            assert!(holder.to_bytes(py).is_err());
            taken[1] = b'y';
            drop(taken);
            assert_eq!(holder.to_bytes(py).unwrap().as_bytes(), b"zy");
            assert_eq!(input.as_bytes(), b"ab");
            let flag = Bool::new(PyBool::new(py, true).as_any()).unwrap();
            assert!(Bool::new(&1u8.into_pyobject(py).unwrap().into_any()).is_err());
            let mut taken = flag.take().unwrap();
            assert!(flag.value().is_err());
            *taken = false;
            drop(taken);
            assert!(!flag.value().unwrap());
            let result: PyResult<()> = flag.with_mut(|value| {
                *value = true;
                Err(PyValueError::new_err("injected bool failure"))
            });
            assert!(result.is_err());
            assert!(flag.value().unwrap());
            let builtins = PyModule::import(py, "builtins").unwrap();
            let range = builtins.getattr("range").unwrap();
            assert_eq!(strict_range(&range.call1((2, 4)).unwrap()).unwrap(), 2..4);
            assert!(strict_range(&range.call1((0, 4, 2)).unwrap()).is_err());
        });
    }
}
