//! Explicit file/protocol adapters; Python failures stop native work immediately.
use crate::{callbacks::CallbackError, conversion::*};
use cs705_core::icf::{Error, document::lifecycle};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::{
    cell::RefCell,
    io::{self, Write},
};

pub(crate) use crate::errors::file as file_error;

pub(crate) fn attach_checked<R>(
    interpreter: i64,
    call: impl FnOnce(Python<'_>) -> PyResult<R>,
) -> PyResult<R> {
    Python::attach(|py| {
        if crate::scoped::interpreter(py)? != interpreter {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "document callback belongs to another interpreter",
            ));
        }
        call(py)
    })
}

fn io_result<T>(
    failures: &RefCell<CallbackError>,
    interpreter: i64,
    result: PyResult<T>,
) -> io::Result<T> {
    result.map_err(|error| {
        let native = Python::attach(|py| {
            if !crate::scoped::interpreter(py).is_ok_and(|id| id == interpreter) {
                // Do not normalize or clone a Python exception in the wrong interpreter.
                return io::Error::other("document callback interpreter mismatch");
            }
            let native = io::Error::from(error.clone_ref(py));
            // Python exceptions are terminal, not Write::write_all retry requests.
            if native.kind() == io::ErrorKind::Interrupted {
                io::Error::other(error.clone_ref(py))
            } else {
                native
            }
        });
        let _ = failures.borrow_mut().capture::<()>(Err(error));
        native
    })
}

pub(crate) struct BinaryWriter<'a> {
    pub object: Py<PyAny>,
    pub errors: &'a RefCell<CallbackError>,
    pub interpreter: i64,
}
impl Write for BinaryWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = attach_checked(self.interpreter, |py| {
            let count = strict_usize(
                &self
                    .object
                    .bind(py)
                    .call_method1("write", (PyBytes::new(py, bytes),))?,
            )?;
            if count > bytes.len() {
                return Err(PyValueError::new_err(
                    "write returned more bytes than supplied",
                ));
            }
            Ok(count)
        });
        io_result(self.errors, self.interpreter, result)
    }
    fn flush(&mut self) -> io::Result<()> {
        let result = attach_checked(self.interpreter, |py| {
            require_none(self.object.bind(py).call_method0("flush")?)
        });
        io_result(self.errors, self.interpreter, result)
    }
}
pub(crate) fn require_none(value: Bound<'_, PyAny>) -> PyResult<()> {
    if !value.is_none() {
        return Err(PyTypeError::new_err("protocol operation must return None"));
    }
    Ok(())
}
pub(crate) struct Protocol<'a> {
    pub object: Py<PyAny>,
    pub errors: &'a RefCell<CallbackError>,
    pub interpreter: i64,
}
impl Protocol<'_> {
    fn call<R>(
        &self,
        call: impl FnOnce(Python<'_>, &Bound<'_, PyAny>) -> PyResult<R>,
    ) -> PyResult<R> {
        attach_checked(self.interpreter, |py| call(py, self.object.bind(py)))
    }
    fn image<T>(&self, result: PyResult<T>) -> Result<T, Error> {
        self.errors
            .borrow_mut()
            .capture(result)
            .map_err(|_| Error::Operation("Python document protocol failed".into()))
    }
    fn io<T>(&self, result: PyResult<T>) -> io::Result<T> {
        io_result(self.errors, self.interpreter, result)
    }
}
impl lifecycle::PathComparison for Protocol<'_> {
    fn compare(&mut self, left: &[u8], right: &[u8]) -> Result<i32, Error> {
        let result = self.call(|py, object| {
            strict_i32(
                &object
                    .call_method1("compare", (PyBytes::new(py, left), PyBytes::new(py, right)))?,
            )
        });
        self.image(result)
    }
}
impl lifecycle::RecentStore for Protocol<'_> {
    fn read(&mut self, index: usize) -> io::Result<Vec<u8>> {
        let result = self
            .call(|_, object| Ok(strict_bytes(&object.call_method1("read", (index,))?)?.to_vec()));
        self.io(result)
    }
    fn write(&mut self, index: usize, path: &[u8]) -> io::Result<()> {
        let result = self.call(|py, object| {
            require_none(object.call_method1("write", (index, PyBytes::new(py, path)))?)
        });
        self.io(result)
    }
}
impl lifecycle::Files for Protocol<'_> {
    fn read(&mut self, path: &[u8]) -> io::Result<Vec<u8>> {
        let result = self.call(|py, object| {
            Ok(strict_bytes(&object.call_method1("read", (PyBytes::new(py, path),))?)?.to_vec())
        });
        self.io(result)
    }
    fn publish(&mut self, path: &[u8], bytes: &[u8]) -> io::Result<()> {
        let result = self.call(|py, object| {
            require_none(
                object
                    .call_method1("publish", (PyBytes::new(py, path), PyBytes::new(py, bytes)))?,
            )
        });
        self.io(result)
    }
}
impl lifecycle::Dialogs for Protocol<'_> {
    fn save_as(
        &mut self,
        initial: lifecycle::SaveDialog,
    ) -> Result<Option<lifecycle::SaveSelection>, Error> {
        let result = self.call(|py, object| {
            let request = Py::new(py, super::lifecycle_values::SaveDialog { inner: initial })?;
            let selected = object.call_method1("save_as", (request,))?;
            if selected.is_none() {
                return Ok(None);
            }
            Ok(Some(
                selected
                    .extract::<PyRef<'_, super::lifecycle_values::SaveSelection>>()?
                    .inner
                    .clone(),
            ))
        });
        self.image(result)
    }
    fn choice(&mut self, prompt: lifecycle::Prompt) -> Result<i32, Error> {
        let result = self.call(|py, object| {
            let prompt = Py::new(py, super::lifecycle_values::Prompt { inner: prompt })?;
            strict_i32(&object.call_method1("choice", (prompt,))?)
        });
        self.image(result)
    }
}
impl lifecycle::PlacementSource for Protocol<'_> {
    fn placement(&mut self) -> Result<Option<lifecycle::Placement>, Error> {
        let result = self.call(|_, object| {
            let value = object.call_method0("placement")?;
            if value.is_none() {
                return Ok(None);
            }
            Ok(Some(
                value
                    .extract::<PyRef<'_, super::lifecycle_values::Placement>>()?
                    .inner,
            ))
        });
        self.image(result)
    }
}

#[cfg(test)]
mod checks {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn interpreter_mismatch_rejects_before_invoking_python() {
        Python::initialize();
        Python::attach(|py| {
            let foreign = crate::scoped::interpreter(py)
                .unwrap()
                .checked_add(1)
                .unwrap();
            let called = Cell::new(false);
            let result = attach_checked(foreign, |_| {
                called.set(true);
                Ok(())
            });
            assert!(
                result
                    .unwrap_err()
                    .is_instance_of::<pyo3::exceptions::PyRuntimeError>(py)
            );
            assert!(!called.get());
            let errors = RefCell::new(CallbackError::default());
            let mut writer = BinaryWriter {
                object: py.None(),
                errors: &errors,
                interpreter: foreign,
            };
            assert!(writer.write(b"never bound").is_err());
            assert!(
                errors
                    .into_inner()
                    .finish(py, Ok(()))
                    .unwrap_err()
                    .is_instance_of::<pyo3::exceptions::PyRuntimeError>(py)
            );
        });
    }
}
