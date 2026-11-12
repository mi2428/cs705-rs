//! Move native state out before invoking callbacks or detaching from Python.
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use std::ops::{Deref, DerefMut};
use std::sync::{Mutex, MutexGuard};

enum State<T> {
    Ready(T),
    Busy,
    Consumed,
}

pub struct Owner<T> {
    state: Mutex<State<T>>,
}

impl<T> Owner<T> {
    pub fn new(value: T) -> Self {
        Self {
            state: Mutex::new(State::Ready(value)),
        }
    }

    fn lock(&self) -> PyResult<MutexGuard<'_, State<T>>> {
        self.state
            .lock()
            .map_err(|_| PyRuntimeError::new_err("native owner is poisoned"))
    }

    pub fn take(&self) -> PyResult<Taken<'_, T>> {
        let mut state = self.lock()?;
        match &*state {
            State::Busy => return Err(PyRuntimeError::new_err("native owner is busy")),
            State::Consumed => return Err(PyRuntimeError::new_err("native owner was consumed")),
            State::Ready(_) => {}
        }
        let State::Ready(value) = std::mem::replace(&mut *state, State::Busy) else {
            unreachable!()
        };
        Ok(Taken {
            owner: self,
            value: Some(value),
        })
    }

    pub fn with_ref<R>(&self, f: impl FnOnce(&T) -> PyResult<R>) -> PyResult<R> {
        f(&*self.take()?)
    }

    pub fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> PyResult<R>) -> PyResult<R> {
        f(&mut *self.take()?)
    }
}

/// Restores the owner on success, error, and unwinding; holds no mutex guard.
pub struct Taken<'a, T> {
    owner: &'a Owner<T>,
    value: Option<T>,
}

impl<T> Taken<'_, T> {
    /// Permanently marks the original owner consumed.
    pub fn consume(mut self) -> T {
        *self.owner.state.lock().expect("native owner is poisoned") = State::Consumed;
        self.value.take().expect("taken state is present")
    }
}

impl<T> Deref for Taken<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value.as_ref().expect("taken state is present")
    }
}

impl<T> DerefMut for Taken<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value.as_mut().expect("taken state is present")
    }
}

impl<T> Drop for Taken<'_, T> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            *self.owner.state.lock().expect("native owner is poisoned") = State::Ready(value);
        }
    }
}
