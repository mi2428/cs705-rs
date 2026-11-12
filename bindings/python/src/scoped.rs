//! Scoped native loans. Only this module may erase pointers; Python gets tokens.
use pyo3::exceptions::{PyReferenceError, PyRuntimeError};
use pyo3::prelude::*;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, ThreadId};

static NEXT_FRAME: AtomicU64 = AtomicU64::new(1);
thread_local! {
    static FRAMES: RefCell<HashMap<u64, Vec<Box<dyn Any>>>> = RefCell::new(HashMap::new());
}

#[derive(Default)]
struct Access {
    readers: usize,
    writer: bool,
}

struct Target<T: ?Sized> {
    read: Rc<dyn Fn() -> *const T>,
    write: Option<Rc<dyn Fn() -> *mut T>>,
    access: Rc<RefCell<Access>>,
    children: Rc<Cell<usize>>,
    ancestors: Vec<Rc<Cell<usize>>>,
}

/// An opaque token, with no address or reference to native state.
pub struct Loan<T: ?Sized + 'static> {
    frame: u64,
    target: usize,
    thread: ThreadId,
    interpreter: i64,
    marker: PhantomData<fn() -> T>,
}

impl<T: ?Sized> Clone for Loan<T> {
    fn clone(&self) -> Self {
        Self {
            frame: self.frame,
            target: self.target,
            thread: self.thread,
            interpreter: self.interpreter,
            marker: PhantomData,
        }
    }
}

pub(crate) fn interpreter(_py: Python<'_>) -> PyResult<i64> {
    // SAFETY: the Python token proves that this thread is attached.
    let id = unsafe { pyo3::ffi::PyInterpreterState_GetID(pyo3::ffi::PyInterpreterState_Get()) };
    if id < 0 {
        Err(PyRuntimeError::new_err(
            "cannot identify the current interpreter",
        ))
    } else {
        Ok(id)
    }
}

fn next_frame(counter: &AtomicU64) -> PyResult<u64> {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .map_err(|_| PyRuntimeError::new_err("scoped frame identifiers exhausted"))
}

/// Namespace for synchronous scopes; the actual expiration guard is private.
pub struct Frame;

struct IcfTag;

/// A lifetime-polymorphic loan, never an Icf<'static> cast or snapshot.
#[derive(Clone)]
pub struct IcfLoan {
    token: Loan<IcfTag>,
}

impl Frame {
    pub fn with_icf_ref<R>(
        py: Python<'_>,
        value: &cs705_core::icf::Icf<'_>,
        f: impl FnOnce(IcfLoan) -> PyResult<R>,
    ) -> PyResult<R> {
        let pointer = std::ptr::from_ref(value).cast::<IcfTag>();
        Self::run(
            py,
            Target {
                read: Rc::new(move || pointer),
                write: None,
                access: Rc::default(),
                children: Rc::default(),
                ancestors: Vec::new(),
            },
            |token| f(IcfLoan { token }),
        )
    }

    pub fn with_icf_mut<R>(
        py: Python<'_>,
        value: &mut cs705_core::icf::Icf<'_>,
        f: impl FnOnce(IcfLoan) -> PyResult<R>,
    ) -> PyResult<R> {
        let pointer = std::ptr::from_mut(value).cast::<IcfTag>();
        Self::run(
            py,
            Target {
                read: Rc::new(move || pointer.cast_const()),
                write: Some(Rc::new(move || pointer)),
                access: Rc::default(),
                children: Rc::default(),
                ancestors: Vec::new(),
            },
            |token| f(IcfLoan { token }),
        )
    }

    pub fn with_ref<T: ?Sized + 'static, R>(
        py: Python<'_>,
        value: &T,
        f: impl FnOnce(Loan<T>) -> PyResult<R>,
    ) -> PyResult<R> {
        let pointer = std::ptr::from_ref(value);
        Self::run(
            py,
            Target {
                read: Rc::new(move || pointer),
                write: None,
                access: Rc::default(),
                children: Rc::default(),
                ancestors: Vec::new(),
            },
            f,
        )
    }

    pub fn with_mut<T: ?Sized + 'static, R>(
        py: Python<'_>,
        value: &mut T,
        f: impl FnOnce(Loan<T>) -> PyResult<R>,
    ) -> PyResult<R> {
        let pointer = std::ptr::from_mut(value);
        Self::run(
            py,
            Target {
                read: Rc::new(move || pointer.cast_const()),
                write: Some(Rc::new(move || pointer)),
                access: Rc::default(),
                children: Rc::default(),
                ancestors: Vec::new(),
            },
            f,
        )
    }

    fn run<T: ?Sized + 'static, R>(
        py: Python<'_>,
        target: Target<T>,
        f: impl FnOnce(Loan<T>) -> PyResult<R>,
    ) -> PyResult<R> {
        let interpreter = interpreter(py)?;
        let frame = next_frame(&NEXT_FRAME)?;
        FRAMES.with_borrow_mut(|frames| {
            frames.insert(frame, vec![Box::new(Rc::new(target))]);
        });
        let _expiration = Expiration(frame);
        f(Loan {
            frame,
            target: 0,
            thread: thread::current().id(),
            interpreter,
            marker: PhantomData,
        })
    }
}

struct Expiration(u64);
impl Drop for Expiration {
    fn drop(&mut self) {
        // Drop registrations outside the registry borrow (including on unwind).
        let removed = FRAMES.with_borrow_mut(|frames| frames.remove(&self.0));
        drop(removed);
    }
}

struct Borrow {
    access: Rc<RefCell<Access>>,
    write: bool,
}
impl Drop for Borrow {
    fn drop(&mut self) {
        let mut access = self.access.borrow_mut();
        if self.write {
            access.writer = false;
        } else {
            access.readers -= 1;
        }
    }
}

impl<T: ?Sized + 'static> Loan<T> {
    fn resolve(&self, py: Python<'_>) -> PyResult<Rc<Target<T>>> {
        // These checks precede even looking up a pointer-bearing registration.
        if thread::current().id() != self.thread {
            return Err(PyRuntimeError::new_err(
                "scoped view belongs to another thread",
            ));
        }
        if interpreter(py)? != self.interpreter {
            return Err(PyRuntimeError::new_err(
                "scoped view belongs to another interpreter",
            ));
        }
        FRAMES.with_borrow(|frames| {
            frames
                .get(&self.frame)
                .and_then(|targets| targets.get(self.target))
                .and_then(|target| target.downcast_ref::<Rc<Target<T>>>())
                .cloned()
                .ok_or_else(|| PyReferenceError::new_err("scoped view has expired"))
        })
    }

    pub fn with_ref<R>(&self, py: Python<'_>, f: impl FnOnce(&T) -> PyResult<R>) -> PyResult<R> {
        self.read_pointer(py, |pointer| {
            // SAFETY: read_pointer holds the common shared borrow reservation.
            f(unsafe { &*pointer })
        })
    }

    fn read_pointer<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(*const T) -> PyResult<R>,
    ) -> PyResult<R> {
        let target = self.resolve(py)?;
        {
            let mut access = target.access.borrow_mut();
            if access.writer {
                return Err(PyRuntimeError::new_err("overlapping native borrow"));
            }
            access.readers = access
                .readers
                .checked_add(1)
                .ok_or_else(|| PyRuntimeError::new_err("native borrow count exhausted"))?;
        }
        let _borrow = Borrow {
            access: target.access.clone(),
            write: false,
        };
        let pointer = (target.read)();
        f(pointer)
    }

    pub fn with_mut<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut T) -> PyResult<R>,
    ) -> PyResult<R> {
        self.write_pointer(py, |pointer| {
            // SAFETY: write_pointer holds the exclusive borrow reservation.
            f(unsafe { &mut *pointer })
        })
    }

    fn write_pointer<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(*mut T) -> PyResult<R>,
    ) -> PyResult<R> {
        let target = self.resolve(py)?;
        let write = target
            .write
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("scoped view is read-only"))?;
        if target.children.get() != 0 {
            return Err(PyRuntimeError::new_err(
                "mutation could invalidate a child view",
            ));
        }
        {
            let mut access = target.access.borrow_mut();
            if access.writer || access.readers != 0 {
                return Err(PyRuntimeError::new_err("overlapping native borrow"));
            }
            access.writer = true;
        }
        let _borrow = Borrow {
            access: target.access.clone(),
            write: true,
        };
        let pointer = write();
        f(pointer)
    }

    pub fn project_ref<U: ?Sized + 'static>(
        &self,
        py: Python<'_>,
        read: fn(&T) -> &U,
    ) -> PyResult<Loan<U>> {
        self.project(py, read, None)
    }

    pub fn project_mut<U: ?Sized + 'static>(
        &self,
        py: Python<'_>,
        read: fn(&T) -> &U,
        write: fn(&mut T) -> &mut U,
    ) -> PyResult<Loan<U>> {
        self.project(py, read, Some(write))
    }

    fn project<U: ?Sized + 'static>(
        &self,
        py: Python<'_>,
        read: fn(&T) -> &U,
        write: Option<fn(&mut T) -> &mut U>,
    ) -> PyResult<Loan<U>> {
        let parent = self.resolve(py)?;
        let write = if let Some(project) = write {
            let resolve = parent
                .write
                .clone()
                .ok_or_else(|| PyRuntimeError::new_err("scoped view is read-only"))?;
            // SAFETY: called only while the shared authority is exclusively held.
            Some(
                Rc::new(move || project(unsafe { &mut *resolve() }) as *mut U)
                    as Rc<dyn Fn() -> *mut U>,
            )
        } else {
            None
        };
        let resolve = parent.read.clone();
        // SAFETY: called only under a checked shared/exclusive authority.
        let read = Rc::new(move || read(unsafe { &*resolve() }) as *const U);
        self.insert_child(parent, read, write)
    }

    fn insert_child<U: ?Sized + 'static>(
        &self,
        parent: Rc<Target<T>>,
        read: Rc<dyn Fn() -> *const U>,
        write: Option<Rc<dyn Fn() -> *mut U>>,
    ) -> PyResult<Loan<U>> {
        if parent.access.borrow().writer {
            return Err(PyRuntimeError::new_err("overlapping native borrow"));
        }
        let mut ancestors = parent.ancestors.clone();
        ancestors.push(parent.children.clone());
        if ancestors.iter().any(|count| count.get() == usize::MAX) {
            return Err(PyRuntimeError::new_err("child view count exhausted"));
        }
        let child = Target {
            read,
            write,
            access: parent.access.clone(),
            children: Rc::default(),
            ancestors: ancestors.clone(),
        };
        let index = FRAMES.with_borrow_mut(|frames| {
            let targets = frames.get_mut(&self.frame).expect("validated live frame");
            let index = targets.len();
            targets.push(Box::new(Rc::new(child)));
            index
        });
        for count in ancestors {
            count.set(count.get() + 1);
        }
        Ok(Loan {
            frame: self.frame,
            target: index,
            thread: self.thread,
            interpreter: self.interpreter,
            marker: PhantomData,
        })
    }
}

impl IcfLoan {
    pub fn with_ref<R>(
        &self,
        py: Python<'_>,
        f: impl for<'data> FnOnce(&cs705_core::icf::Icf<'data>) -> PyResult<R>,
    ) -> PyResult<R> {
        self.token.read_pointer(py, |pointer| {
            // SAFETY: registration supplies the original Icf pointer. The HRTB
            // hides its data lifetime and prevents it escaping through R.
            f(unsafe { &*pointer.cast::<cs705_core::icf::Icf<'_>>() })
        })
    }

    pub fn with_mut<R>(
        &self,
        py: Python<'_>,
        f: impl for<'data> FnOnce(&mut cs705_core::icf::Icf<'data>) -> PyResult<R>,
    ) -> PyResult<R> {
        self.token.write_pointer(py, |pointer| {
            // SAFETY: exclusive authority; the universally quantified callback
            // cannot install data borrowed for less than the original lifetime.
            f(unsafe { &mut *pointer.cast::<cs705_core::icf::Icf<'_>>() })
        })
    }

    pub fn project_ref<U: ?Sized + 'static>(
        &self,
        py: Python<'_>,
        read: for<'a, 'data> fn(&'a cs705_core::icf::Icf<'data>) -> &'a U,
    ) -> PyResult<Loan<U>> {
        self.project(py, read, None)
    }

    pub fn project_mut<U: ?Sized + 'static>(
        &self,
        py: Python<'_>,
        read: for<'a, 'data> fn(&'a cs705_core::icf::Icf<'data>) -> &'a U,
        write: for<'a, 'data> fn(&'a mut cs705_core::icf::Icf<'data>) -> &'a mut U,
    ) -> PyResult<Loan<U>> {
        self.project(py, read, Some(write))
    }

    fn project<U: ?Sized + 'static>(
        &self,
        py: Python<'_>,
        read: for<'a, 'data> fn(&'a cs705_core::icf::Icf<'data>) -> &'a U,
        write: Option<for<'a, 'data> fn(&'a mut cs705_core::icf::Icf<'data>) -> &'a mut U>,
    ) -> PyResult<Loan<U>> {
        let parent = self.token.resolve(py)?;
        let write = if let Some(project) = write {
            let resolve = parent
                .write
                .clone()
                .ok_or_else(|| PyRuntimeError::new_err("scoped view is read-only"))?;
            // SAFETY: exclusive reservation and a lifetime-polymorphic projector.
            Some(Rc::new(move || {
                project(unsafe { &mut *resolve().cast::<cs705_core::icf::Icf<'_>>() }) as *mut U
            }) as Rc<dyn Fn() -> *mut U>)
        } else {
            None
        };
        let resolve = parent.read.clone();
        // SAFETY: shared reservation and a lifetime-polymorphic projector.
        let read = Rc::new(move || {
            read(unsafe { &*resolve().cast::<cs705_core::icf::Icf<'_>>() }) as *const U
        });
        self.token.insert_child(parent, read, write)
    }
}

#[cfg(test)]
mod checks {
    use super::*;
    #[test]
    fn identifiers_fail_closed() {
        let counter = AtomicU64::new(u64::MAX - 1);
        assert_eq!(next_frame(&counter).unwrap(), u64::MAX - 1);
        assert!(next_frame(&counter).is_err());
        assert!(next_frame(&counter).is_err());
    }

    #[test]
    fn interpreter_mismatch_rejects_access() {
        Python::initialize();
        Python::attach(|py| {
            Frame::with_mut(py, &mut 1usize, |mut loan| {
                loan.interpreter = -1;
                let error = loan
                    .with_mut::<()>(py, |_| panic!("must not access target"))
                    .unwrap_err();
                assert!(error.is_instance_of::<PyRuntimeError>(py));
                Ok(())
            })
            .unwrap()
        });
    }
}
