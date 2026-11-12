//! Invocation-local failures and GC-visible ownership for retained callables.
use pyo3::exceptions::{PyBaseExceptionGroup, PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::{PyTraverseError, PyVisit};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, Weak};

struct ActiveInvocation {
    interpreter: i64,
    errors: Rc<RefCell<CallbackError>>,
}

thread_local! {
    static INVOCATIONS: RefCell<Vec<ActiveInvocation>> = const { RefCell::new(Vec::new()) };
}

struct InvocationGuard;
impl Drop for InvocationGuard {
    fn drop(&mut self) {
        let removed = INVOCATIONS.with_borrow_mut(|stack| stack.pop());
        drop(removed);
    }
}

struct CallableState {
    interpreter: i64,
    callable: Mutex<Option<Py<PyAny>>>,
}

/// The enclosing Python object must own and traverse a Py<CallbackHolder>.
#[pyclass(frozen, module = "cs705")]
pub struct CallbackHolder {
    state: Arc<CallableState>,
}

impl CallbackHolder {
    pub fn new(py: Python<'_>, callable: &Bound<'_, PyAny>) -> PyResult<Py<Self>> {
        if !callable.is_callable() {
            return Err(PyTypeError::new_err("expected a callable"));
        }
        Py::new(
            py,
            Self {
                state: Arc::new(CallableState {
                    interpreter: crate::scoped::interpreter(py)?,
                    callable: Mutex::new(Some(callable.clone().unbind())),
                }),
            },
        )
    }

    pub fn link(holder: &Bound<'_, Self>) -> CallbackLink {
        CallbackLink {
            state: Arc::downgrade(&holder.get().state),
        }
    }
}

#[pymethods]
impl CallbackHolder {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        let callable = self
            .state
            .callable
            .lock()
            .expect("callback state is poisoned");
        visit.call(&*callable)
    }

    fn __clear__(&self) {
        let callable = self
            .state
            .callable
            .lock()
            .expect("callback state is poisoned")
            .take();
        // DECREF may invoke Python; the mutex has already been released.
        drop(callable);
    }
}

/// Native closures capture this weak link, never a strong Python reference.
#[derive(Clone)]
pub struct CallbackLink {
    state: Weak<CallableState>,
}

impl CallbackLink {
    /// Retained callbacks record into the innermost invocation, not their holder.
    pub fn invoke<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<R>,
    ) -> PyResult<R> {
        self.invoke_kind(py, f, false)
    }

    pub fn invoke_cleanup<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<R>,
    ) -> PyResult<R> {
        self.invoke_kind(py, f, true)
    }

    fn invoke_kind<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<R>,
        cleanup: bool,
    ) -> PyResult<R> {
        let interpreter = crate::scoped::interpreter(py)?;
        let errors = INVOCATIONS
            .with_borrow(|stack| {
                stack
                    .last()
                    .filter(|invocation| invocation.interpreter == interpreter)
                    .map(|invocation| invocation.errors.clone())
            })
            .ok_or_else(|| {
                PyRuntimeError::new_err("retained callback requires an active invocation")
            })?;
        if !cleanup && errors.borrow().failed() {
            return Err(PyRuntimeError::new_err(
                "callback invocation already failed",
            ));
        }
        let result = self.with_callable(py, f);
        if let Err(error) = &result {
            let error = error.clone_ref(py);
            if cleanup {
                let _ = errors.borrow_mut().capture_cleanup::<()>(Err(error));
            } else {
                let _ = errors.borrow_mut().capture::<()>(Err(error));
            }
        }
        result
    }

    pub fn with_callable<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<R>,
    ) -> PyResult<R> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("callback holder is unavailable"))?;
        if crate::scoped::interpreter(py)? != state.interpreter {
            return Err(PyRuntimeError::new_err(
                "callback belongs to another interpreter",
            ));
        }
        let callable = state
            .callable
            .lock()
            .map_err(|_| PyRuntimeError::new_err("callback state is poisoned"))?
            .as_ref()
            .map(|callable| callable.clone_ref(py))
            .ok_or_else(|| PyRuntimeError::new_err("callback holder was cleared"))?;
        f(callable.bind(py))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CallbackFailed;

/// Create one carrier per native invocation, including recursive invocations.
#[derive(Default)]
pub struct CallbackError {
    primary: Option<PyErr>,
    cleanup: Vec<PyErr>,
}

impl CallbackError {
    /// Wrap a native entry which may invoke a stored MenuQuery or other closure.
    /// The native result remains separate so adapters can attach an Outcome.
    pub fn scoped<T>(py: Python<'_>, f: impl FnOnce() -> T) -> PyResult<(Self, T)> {
        let interpreter = crate::scoped::interpreter(py)?;
        let errors = Rc::new(RefCell::new(Self::default()));
        INVOCATIONS.with_borrow_mut(|stack| {
            stack.push(ActiveInvocation {
                interpreter,
                errors: errors.clone(),
            })
        });
        let guard = InvocationGuard;
        let result = f();
        drop(guard);
        let errors = std::mem::take(&mut *errors.borrow_mut());
        Ok((errors, result))
    }

    pub fn invoke<T>(&mut self, f: impl FnOnce() -> PyResult<T>) -> Result<T, CallbackFailed> {
        if self.failed() {
            return Err(CallbackFailed);
        }
        self.capture(f())
    }

    pub fn capture<T>(&mut self, result: PyResult<T>) -> Result<T, CallbackFailed> {
        result.map_err(|error| {
            if self.primary.is_none() {
                self.primary = Some(error);
            } else {
                self.cleanup.push(error);
            }
            CallbackFailed
        })
    }

    pub fn failed(&self) -> bool {
        self.primary.is_some()
    }

    /// Required cleanup still runs, without displacing a native primary error.
    pub fn capture_cleanup<T>(&mut self, result: PyResult<T>) -> Result<T, CallbackFailed> {
        result.map_err(|error| {
            self.cleanup.push(error);
            CallbackFailed
        })
    }

    pub fn finish<T>(mut self, py: Python<'_>, native: PyResult<T>) -> PyResult<T> {
        let primary = match (self.primary.take(), native) {
            (None, Ok(value)) if self.cleanup.is_empty() => return Ok(value),
            (None, Ok(_)) => self.cleanup.remove(0),
            (None, Err(error)) => error,
            (Some(primary), native) => {
                if let Err(error) = native {
                    if !error.value(py).is(primary.value(py)) {
                        self.cleanup.push(error);
                    }
                }
                primary
            }
        };
        if !self.cleanup.is_empty() {
            if let Some(context) = primary.context(py) {
                self.cleanup.insert(0, context);
            }
            let errors: Vec<_> = self
                .cleanup
                .into_iter()
                .map(|error| error.into_value(py))
                .collect();
            primary.set_context(
                py,
                Some(PyBaseExceptionGroup::new_err((
                    "native callback cleanup failures",
                    errors,
                ))),
            );
        }
        Err(primary)
    }

    /// A normal native Outcome stays available in the exception context's args.
    pub fn finish_with_outcome<T>(
        mut self,
        py: Python<'_>,
        native: PyResult<T>,
        outcome: Py<PyAny>,
    ) -> PyResult<T> {
        if self.failed() || !self.cleanup.is_empty() || native.is_err() {
            self.cleanup
                .push(PyRuntimeError::new_err(("native outcome", outcome)));
        }
        self.finish(py, native)
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<CallbackHolder>()
}
