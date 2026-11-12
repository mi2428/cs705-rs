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
    path: Vec<Arc<AtomicU64>>,
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            access: self.access.clone(),
            epoch: self.epoch.clone(),
            path: self.path.clone(),
        }
    }
}
impl<T: Send + 'static> Handle<T> {
    pub(crate) fn new(value: T) -> Self {
        let owner = Owner::new(value);
        Self {
            access: Arc::new(move |f| owner.with_mut(f)),
            epoch: Arc::new(AtomicU64::new(0)),
            path: Vec::new(),
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
            let previous = self
                .epoch
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1))
                .map_err(|_| PyRuntimeError::new_err("clone view generations exhausted"))?;
            // Rebuilding a field invalidates descendants and other aliases, not
            // the edited object or its ancestors. The root Owner is exclusive
            // here, so this path can advance before the next native mutation.
            for expected in &self.path {
                expected.store(previous + 1, Ordering::SeqCst);
            }
            f(value)
        })
    }
    pub(crate) fn project<U: Send + 'static>(
        &self,
        project: impl Fn(&mut T) -> PyResult<&mut U> + Send + Sync + 'static,
    ) -> Handle<U> {
        let parent = self.clone();
        let epoch = self.epoch.clone();
        let expected = Arc::new(AtomicU64::new(epoch.load(Ordering::SeqCst)));
        let mut path = self.path.clone();
        path.push(expected.clone());
        Handle {
            access: Arc::new(move |f| {
                parent.with(|value| {
                    if parent.epoch.load(Ordering::SeqCst) != expected.load(Ordering::SeqCst) {
                        return Err(PyReferenceError::new_err(
                            "clone field view was invalidated",
                        ));
                    }
                    f(project(value)?)
                })
            }),
            epoch,
            path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_exhaustion_precedes_mutation() {
        let handle = Handle::new(1usize);
        handle.epoch.store(u64::MAX, Ordering::SeqCst);
        for _ in 0..2 {
            assert!(handle.edit::<()>(|_| panic!("must not mutate")).is_err());
        }
        assert_eq!(handle.with(|value| Ok(*value)).unwrap(), 1);
    }
}
