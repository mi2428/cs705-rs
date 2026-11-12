//! Read/Seek adapter for the native sparse-source reader; never closes input.
use super::*;
use crate::callbacks::CallbackError;
use crate::icf::document::adapters::attach_checked;
use std::{
    cell::RefCell,
    io::{Read, Seek, SeekFrom},
};
struct Reader<'a> {
    object: Py<PyAny>,
    interpreter: i64,
    errors: &'a RefCell<CallbackError>,
}
impl Reader<'_> {
    fn capture<T>(&self, result: PyResult<T>) -> std::io::Result<T> {
        self.errors
            .borrow_mut()
            .capture(result)
            .map_err(|_| std::io::Error::other("Python source reader failed"))
    }
}
impl Read for Reader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let result = attach_checked(self.interpreter, |py| {
            let value = self.object.bind(py).call_method1("read", (output.len(),))?;
            let value = strict_bytes(&value)?;
            if value.len() > output.len() {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "read returned more bytes than requested",
                ));
            }
            Ok(value.to_vec())
        });
        let bytes = self.capture(result)?;
        output[..bytes.len()].copy_from_slice(&bytes);
        Ok(bytes.len())
    }
}
impl Seek for Reader<'_> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let (offset, whence) = match position {
            SeekFrom::Start(v) => (i128::from(v), 0),
            SeekFrom::Current(v) => (i128::from(v), 1),
            SeekFrom::End(v) => (i128::from(v), 2),
        };
        let result = attach_checked(self.interpreter, |py| {
            strict_u64(
                &self
                    .object
                    .bind(py)
                    .call_method1("seek", (offset, whence))?,
            )
        });
        self.capture(result)
    }
}
#[pyfunction]
/// Capture bounded PE data windows from a synchronous binary read/seek object.
/// read must return bytes and seek an integer offset. Never closes the reader; exceptions propagate.
pub fn read_source<'py>(py: Python<'py>, reader: Py<PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    let interpreter = crate::scoped::interpreter(py)?;
    let (errors, result) = py.detach(move || {
        let errors = RefCell::new(CallbackError::default());
        let result = core::native::read_source(Reader {
            object: reader,
            interpreter,
            errors: &errors,
        });
        (errors.into_inner(), result)
    });
    let source = errors.finish(py, result.map_err(crate::errors::io))?;
    Ok(PyBytes::new(py, &source))
}
