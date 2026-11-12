//! Checked projections share the existing Owner, never a copied mutable field.
use crate::owner::Owner;
use pyo3::exceptions::{PyReferenceError, PyRuntimeError};
use pyo3::prelude::*;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

type Access<T> = dyn Fn(&mut dyn FnMut(&mut T) -> PyResult<()>) -> PyResult<()> + Send + Sync;
pub(crate) struct Handle<T> {
    access: Arc<Access<T>>,
    epoch: Arc<AtomicU64>,
}
impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            access: self.access.clone(),
            epoch: self.epoch.clone(),
        }
    }
}
impl<T: Send + 'static> Handle<T> {
    pub(crate) fn new(value: T) -> Self {
        let owner = Owner::new(value);
        Self {
            access: Arc::new(move |f| owner.with_mut(f)),
            epoch: Arc::new(AtomicU64::new(0)),
        }
    }
    pub(crate) fn with<R>(&self, f: impl FnOnce(&mut T) -> PyResult<R>) -> PyResult<R> {
        let mut f = Some(f);
        let mut result = None;
        (self.access)(&mut |value| {
            result = Some(f.take().expect("single owner access")(value)?);
            Ok(())
        })?;
        Ok(result.expect("owner access completed"))
    }
    pub(crate) fn edit<R>(&self, f: impl FnOnce(&mut T) -> PyResult<R>) -> PyResult<R> {
        self.with(|value| {
            self.epoch
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1))
                .map_err(|_| PyRuntimeError::new_err("clone view generations exhausted"))?;
            f(value)
        })
    }
    pub(crate) fn project<U: Send + 'static>(
        &self,
        project: impl Fn(&mut T) -> PyResult<&mut U> + Send + Sync + 'static,
    ) -> Handle<U> {
        let parent = self.clone();
        let epoch = self.epoch.clone();
        let expected = epoch.load(Ordering::SeqCst);
        Handle {
            access: Arc::new(move |f| {
                parent.with(|value| {
                    if parent.epoch.load(Ordering::SeqCst) != expected {
                        return Err(PyReferenceError::new_err(
                            "clone field view was invalidated",
                        ));
                    }
                    f(project(value)?)
                })
            }),
            epoch,
        }
    }
}
