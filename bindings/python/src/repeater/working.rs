//! Owned expanded repeater images, distinct from stored ICF memory.
use crate::{
    callbacks::CallbackError,
    conversion::*,
    errors,
    icf::document::adapters::attach_checked,
    icf::{Icf, context::Context, frequency::Ranges, record::DecodedRecord},
    owner::Owner,
    scoped::{Frame, Loan},
};
use cs705_core::repeater::working as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::cell::RefCell;

#[pyclass(module = "cs705.repeater.working")]
pub struct DefaultContext {
    pub(crate) inner: core::DefaultContext,
}
#[pymethods]
impl DefaultContext {
    #[new]
    fn new(five_mhz: &Bound<'_, PyAny>, hemispheres: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::DefaultContext {
                five_mhz: strict_bool(five_mhz)?,
                hemispheres: fixed_bytes(hemispheres)?,
            },
        })
    }
    #[staticmethod]
    #[pyo3(signature=(context,system_langid))]
    fn from_context(context: &Context, system_langid: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let context = context.with_ref(|q| Ok(*q))?;
        Ok(Self {
            inner: core::DefaultContext::from_context(
                &context,
                system_langid.map(strict_u16).transpose()?,
            )
            .map_err(errors::icf)?,
        })
    }
    #[getter]
    fn five_mhz(&self) -> bool {
        self.inner.five_mhz
    }
    #[setter]
    fn set_five_mhz(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.five_mhz = strict_bool(v)?;
        Ok(())
    }
    #[getter]
    fn hemispheres<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.hemispheres)
    }
    #[setter]
    fn set_hemispheres(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.hemispheres = fixed_bytes(v)?;
        Ok(())
    }
    fn holder(
        &self,
        group: &Bound<'_, PyAny>,
        binding: &Bound<'_, PyAny>,
    ) -> PyResult<DecodedRecord> {
        Ok(DecodedRecord::from_core(
            self.inner.holder(strict_u32(group)?, strict_u32(binding)?),
        ))
    }
}
#[pyclass(module = "cs705.repeater.working")]
pub struct UseRange {
    pub(crate) inner: core::UseRange,
}
#[pymethods]
impl UseRange {
    #[new]
    fn new(first: &Bound<'_, PyAny>, last: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::UseRange {
                first: strict_i32(first)?,
                last: strict_i32(last)?,
            },
        })
    }
    #[getter]
    fn first(&self) -> i32 {
        self.inner.first
    }
    #[setter]
    fn set_first(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.first = strict_i32(v)?;
        Ok(())
    }
    #[getter]
    fn last(&self) -> i32 {
        self.inner.last
    }
    #[setter]
    fn set_last(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.last = strict_i32(v)?;
        Ok(())
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
#[pyclass(module = "cs705.repeater.working", frozen)]
pub struct Working {
    pub(crate) inner: WorkingState,
}
pub(crate) enum WorkingState {
    Owned(Owner<core::Working>),
    Scoped(Loan<core::Working>),
}
impl WorkingState {
    pub(crate) fn with_ref<R>(&self, f: impl FnOnce(&core::Working) -> PyResult<R>) -> PyResult<R> {
        match self {
            Self::Owned(o) => o.with_ref(f),
            Self::Scoped(l) => Python::attach(|py| l.with_ref(py, f)),
        }
    }
    pub(crate) fn with_mut<R>(
        &self,
        f: impl FnOnce(&mut core::Working) -> PyResult<R>,
    ) -> PyResult<R> {
        match self {
            Self::Owned(o) => o.with_mut(f),
            Self::Scoped(l) => Python::attach(|py| l.with_mut(py, f)),
        }
    }
}
impl Working {
    fn run<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(
            &mut core::Working,
            &RefCell<CallbackError>,
            i64,
        ) -> Result<R, cs705_core::icf::Error>
        + Send,
    ) -> PyResult<R> {
        let interpreter = crate::scoped::interpreter(py)?;
        let (value, error) = self.inner.with_mut(|working| {
            Ok(py.detach(|| {
                let error = RefCell::new(CallbackError::default());
                let value = f(working, &error, interpreter);
                (value, error.into_inner())
            }))
        })?;
        error.finish(py, value.map_err(errors::icf))
    }
    pub(crate) fn from_core(w: core::Working) -> Self {
        Self {
            inner: WorkingState::Owned(Owner::new(w)),
        }
    }
    fn from_loan(w: Loan<core::Working>) -> Self {
        Self {
            inner: WorkingState::Scoped(w),
        }
    }
}
#[pymethods]
impl Working {
    #[staticmethod]
    fn from_working(memory: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(
            core::Working::from_working(strict_bytes(memory)?.to_vec()).map_err(errors::icf)?,
        ))
    }
    #[staticmethod]
    fn from_icf(py: Python<'_>, image: &Icf, context: &DefaultContext) -> PyResult<Self> {
        let context = context.inner;
        image.with_ref(|image| {
            Ok(Self::from_core(
                py.detach(|| core::Working::from_icf(image, context))
                    .map_err(errors::icf)?,
            ))
        })
    }
    fn working<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.inner.with_ref(|w| Ok(PyBytes::new(py, w.working())))
    }
    fn into_working<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let WorkingState::Owned(owner) = &self.inner else {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "cannot consume a scoped working image",
            ));
        };
        Ok(PyBytes::new(py, &owner.take()?.consume().into_working()))
    }
    fn groups(&self) -> PyResult<super::groups::Groups> {
        self.inner.with_ref(|w| {
            Ok(super::groups::Groups {
                inner: w.groups().map_err(errors::icf)?,
            })
        })
    }
    #[pyo3(signature=(group))]
    fn bulk_use_range(&self, group: Option<&Bound<'_, PyAny>>) -> PyResult<UseRange> {
        let group = group.map(strict_usize).transpose()?;
        self.inner.with_ref(|w| {
            Ok(UseRange {
                inner: w.bulk_use_range(group).map_err(errors::icf)?,
            })
        })
    }
    fn rename_group_raw(&self, group: &Bound<'_, PyAny>, name: &Bound<'_, PyAny>) -> PyResult<()> {
        let (group, name) = (strict_usize(group)?, fixed_bytes(name)?);
        self.inner
            .with_mut(|w| w.rename_group_raw(group, name).map_err(errors::icf))
    }
    fn rebuild_storage(&self, py: Python<'_>) -> PyResult<()> {
        self.inner
            .with_mut(|w| py.detach(|| w.rebuild_storage()).map_err(errors::icf))
    }
    fn to_icf(&self, py: Python<'_>) -> PyResult<Icf> {
        self.inner.with_ref(|w| {
            Ok(Icf::from_core(
                py.detach(|| w.to_icf().map(|i| i.into_owned()))
                    .map_err(errors::icf)?,
            ))
        })
    }
    fn load_into(&self, index: &Bound<'_, PyAny>, holder: &DecodedRecord) -> PyResult<()> {
        let index = strict_usize(index)?;
        self.inner
            .with_ref(|w| holder.with_mut(|h| w.load_into(index, h).map_err(errors::icf)))
    }
    fn save(&self, index: &Bound<'_, PyAny>, holder: &DecodedRecord) -> PyResult<()> {
        let index = strict_usize(index)?;
        self.inner
            .with_mut(|w| holder.with_ref(|h| w.save(index, h).map_err(errors::icf)))
    }
    fn save_reload(&self, holder: &DecodedRecord) -> PyResult<()> {
        self.inner
            .with_mut(|w| holder.with_mut(|h| w.save_reload(h).map_err(errors::icf)))
    }
    fn copy(
        &self,
        group: &Bound<'_, PyAny>,
        first: &Bound<'_, PyAny>,
        last: &Bound<'_, PyAny>,
        seed: &DecodedRecord,
    ) -> PyResult<Vec<DecodedRecord>> {
        let (g, f, l) = (
            strict_usize(group)?,
            strict_usize(first)?,
            strict_usize(last)?,
        );
        self.inner.with_ref(|w| {
            seed.with_ref(|h| {
                Ok(w.copy(g, f, l, h)
                    .map_err(errors::icf)?
                    .into_iter()
                    .map(DecodedRecord::from_core)
                    .collect())
            })
        })
    }
    fn paste(
        &self,
        py: Python<'_>,
        group: &Bound<'_, PyAny>,
        first: &Bound<'_, PyAny>,
        source: &Bound<'_, PyAny>,
        reset: Py<PyAny>,
    ) -> PyResult<usize> {
        let (g, f) = (strict_usize(group)?, strict_usize(first)?);
        let source = holders(source)?;
        self.run(py, |w, errors, interpreter| {
            w.paste(g, f, &source, |w, n| {
                callback(
                    errors,
                    attach_checked(interpreter, |py| call_reset(py, w, n, &reset)),
                )
            })
        })
    }
    fn clear(
        &self,
        py: Python<'_>,
        group: &Bound<'_, PyAny>,
        first: &Bound<'_, PyAny>,
        last: &Bound<'_, PyAny>,
        default: Py<PyAny>,
    ) -> PyResult<()> {
        let (g, f, l) = (
            strict_usize(group)?,
            strict_usize(first)?,
            strict_usize(last)?,
        );
        self.run(py, |w, errors, interpreter| {
            w.clear(g, f, l, |w| {
                callback(
                    errors,
                    attach_checked(interpreter, |py| call_default(py, w, &default)),
                )
            })
        })
    }
    fn carry(
        &self,
        py: Python<'_>,
        group: &Bound<'_, PyAny>,
        first: &Bound<'_, PyAny>,
        last: &Bound<'_, PyAny>,
        direction: &Bound<'_, PyAny>,
        seed: &DecodedRecord,
        reset: Py<PyAny>,
    ) -> PyResult<bool> {
        let (g, f, l, d) = (
            strict_usize(group)?,
            strict_usize(first)?,
            strict_usize(last)?,
            strict_i32(direction)?,
        );
        let seed = seed.with_ref(|h| Ok(h.clone()))?;
        self.run(py, |w, errors, interpreter| {
            w.carry(g, f, l, d, &seed, |w, n| {
                callback(
                    errors,
                    attach_checked(interpreter, |py| call_reset(py, w, n, &reset)),
                )
            })
        })
    }
    fn delete_one(
        &self,
        py: Python<'_>,
        group: &Bound<'_, PyAny>,
        relative: &Bound<'_, PyAny>,
        seed: &DecodedRecord,
        default: Py<PyAny>,
        reset: Py<PyAny>,
    ) -> PyResult<()> {
        let (g, r) = (strict_usize(group)?, strict_usize(relative)?);
        let seed = seed.with_ref(|h| Ok(h.clone()))?;
        self.run(py, |w, errors, interpreter| {
            w.delete_one(
                g,
                r,
                &seed,
                |w| {
                    callback(
                        errors,
                        attach_checked(interpreter, |py| call_default(py, w, &default)),
                    )
                },
                |w, n| {
                    callback(
                        errors,
                        attach_checked(interpreter, |py| call_reset(py, w, n, &reset)),
                    )
                },
            )
        })
    }
    fn insert_one(
        &self,
        py: Python<'_>,
        group: &Bound<'_, PyAny>,
        relative: &Bound<'_, PyAny>,
        seed: &DecodedRecord,
        default: Py<PyAny>,
        reset: Py<PyAny>,
    ) -> PyResult<bool> {
        let (g, r) = (strict_usize(group)?, strict_usize(relative)?);
        let seed = seed.with_ref(|h| Ok(h.clone()))?;
        self.run(py, |w, errors, interpreter| {
            w.insert_one(
                g,
                r,
                &seed,
                |w| {
                    callback(
                        errors,
                        attach_checked(interpreter, |py| call_default(py, w, &default)),
                    )
                },
                |w, n| {
                    callback(
                        errors,
                        attach_checked(interpreter, |py| call_reset(py, w, n, &reset)),
                    )
                },
            )
        })
    }
    fn bulk_use(
        &self,
        py: Python<'_>,
        range: Py<UseRange>,
        value: &Bound<'_, PyAny>,
        binding: &Bound<'_, PyAny>,
        live: Py<PyAny>,
        enqueue: Py<PyAny>,
    ) -> PyResult<()> {
        let (range, value, binding) = (
            range.try_borrow(py)?.inner,
            strict_bool(value)?,
            strict_u32(binding)?,
        );
        self.run(py, |w, errors, interpreter| {
            w.bulk_use(
                range,
                value,
                binding,
                || {
                    let result = attach_checked(interpreter, |py| {
                        live.bind(py).call0().and_then(|v| {
                            let (b, q): (Py<Ranges>, Py<Context>) = v.extract()?;
                            let b = b.get().with_ref(|r| Ok(r.clone()))?;
                            let q = q.get().with_ref(|q| Ok(*q))?;
                            Ok((b, q))
                        })
                    });
                    errors.borrow_mut().capture(result).map_err(|_| {
                        cs705_core::icf::Error::Operation(
                            "repeater live B/Q provider failed".into(),
                        )
                    })
                },
                |n| {
                    let result = attach_checked(interpreter, |py| {
                        enqueue
                            .bind(py)
                            .call1((crate::icf::special::Notification::from_core(n),))
                            .map(|_| ())
                    });
                    errors.borrow_mut().capture(result).map_err(|_| {
                        cs705_core::icf::Error::Operation(
                            "repeater notification callback failed".into(),
                        )
                    })
                },
            )
        })
    }
}
fn holders(value: &Bound<'_, PyAny>) -> PyResult<Vec<cs705_core::icf::record::DecodedRecord>> {
    value
        .try_iter()?
        .map(|v| {
            v?.extract::<PyRef<'_, DecodedRecord>>()?
                .with_ref(|h| Ok(h.clone()))
        })
        .collect()
}
fn callback<T>(
    errors: &RefCell<CallbackError>,
    result: PyResult<T>,
) -> Result<T, cs705_core::icf::Error> {
    errors
        .borrow_mut()
        .capture(result)
        .map_err(|_| cs705_core::icf::Error::Operation("repeater working callback failed".into()))
}
fn call_default(
    py: Python<'_>,
    w: &mut core::Working,
    default: &Py<PyAny>,
) -> PyResult<cs705_core::icf::record::DecodedRecord> {
    Frame::with_mut(py, w, |loan| {
        default
            .bind(py)
            .call1((Working::from_loan(loan),))?
            .extract::<PyRef<'_, DecodedRecord>>()?
            .with_ref(|h| Ok(h.clone()))
    })
}
fn call_reset(py: Python<'_>, w: &core::Working, n: usize, reset: &Py<PyAny>) -> PyResult<u16> {
    Frame::with_ref(py, w, |loan| {
        strict_u16(&reset.bind(py).call1((Working::from_loan(loan), n))?)
    })
}
#[pyfunction]
fn load_into(
    image: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
) -> PyResult<()> {
    let (image, index) = (strict_bytes(image)?, strict_usize(index)?);
    holder.with_mut(|h| core::load_into(image, index, h).map_err(errors::icf))
}
#[pyfunction]
fn save_into(
    image: &MutableBytes,
    index: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
) -> PyResult<()> {
    let index = strict_usize(index)?;
    image.with_mut(|bytes| {
        holder.with_ref(|h| core::save_into(bytes, index, h).map_err(errors::icf))
    })
}
#[pyfunction]
fn delete_into(
    py: Python<'_>,
    image: Py<MutableBytes>,
    group: &Bound<'_, PyAny>,
    relative: &Bound<'_, PyAny>,
    seed: &DecodedRecord,
    default: Py<PyAny>,
    reset: Py<PyAny>,
) -> PyResult<()> {
    let (group, relative) = (strict_usize(group)?, strict_usize(relative)?);
    let seed = seed.with_ref(|h| Ok(h.clone()))?;
    let mut memory = image.get().take()?;
    let interpreter = crate::scoped::interpreter(py)?;
    let (native, error) = py.detach(|| {
        let error = RefCell::new(CallbackError::default());
        let native = core::delete_into(
            &mut memory,
            group,
            relative,
            &seed,
            |bytes| {
                let result = attach_checked(interpreter, |py| {
                    Frame::with_mut(py, bytes, |loan| {
                        default
                            .bind(py)
                            .call1((crate::csv::nls::Bytes::from_loan(loan),))?
                            .extract::<PyRef<'_, DecodedRecord>>()?
                            .with_ref(|h| Ok(h.clone()))
                    })
                });
                callback(&error, result)
            },
            |bytes, which| {
                let result = attach_checked(interpreter, |py| {
                    Frame::with_ref(py, bytes, |loan| {
                        strict_u16(
                            &reset
                                .bind(py)
                                .call1((crate::csv::nls::Bytes::from_loan(loan), which))?,
                        )
                    })
                });
                callback(&error, result)
            },
        );
        (native, error.into_inner())
    });
    error.finish(py, native.map_err(errors::icf))
}
#[pyfunction]
fn expand_counts(image: &MutableBytes) -> PyResult<()> {
    image.with_mut(|bytes| core::expand_counts(bytes).map_err(errors::icf))
}
#[pyfunction]
fn expand_rows(py: Python<'_>, image: &MutableBytes, context: &DefaultContext) -> PyResult<()> {
    let context = context.inner;
    image.with_mut(|bytes| {
        py.detach(|| core::expand_rows(bytes, context))
            .map_err(errors::icf)
    })
}
#[pyfunction]
fn rebuild_storage_into(py: Python<'_>, image: &MutableBytes) -> PyResult<()> {
    image.with_mut(|bytes| {
        py.detach(|| core::rebuild_storage_into(bytes))
            .map_err(errors::icf)
    })
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "working")?;
    m.add_class::<DefaultContext>()?;
    m.add_class::<UseRange>()?;
    m.add_class::<Working>()?;
    m.add_function(wrap_pyfunction!(expand_counts, &m)?)?;
    m.add_function(wrap_pyfunction!(expand_rows, &m)?)?;
    m.add_function(wrap_pyfunction!(rebuild_storage_into, &m)?)?;
    m.add_function(wrap_pyfunction!(load_into, &m)?)?;
    m.add_function(wrap_pyfunction!(save_into, &m)?)?;
    m.add_function(wrap_pyfunction!(delete_into, &m)?)?;
    root.add_submodule(&m)
}
