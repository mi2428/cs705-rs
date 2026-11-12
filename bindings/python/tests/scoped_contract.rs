#![deny(unsafe_code)]

#[path = "../src/owner.rs"]
mod owner;

#[path = "../src/callbacks.rs"]
mod callbacks;

#[allow(unsafe_code)]
#[path = "../src/scoped.rs"]
mod scoped;

use pyo3::prelude::*;
use pyo3::types::PyDict;

#[pyclass(frozen)]
struct BytesView {
    loan: scoped::Loan<[u8]>,
}

#[pymethods]
impl BytesView {
    fn get(&self, py: Python<'_>) -> PyResult<u8> {
        self.loan.with_ref(py, |bytes| Ok(bytes[0]))
    }
    fn set(&self, py: Python<'_>, value: u8) -> PyResult<()> {
        self.loan.with_mut(py, |bytes| {
            bytes[0] = value;
            Ok(())
        })
    }
    fn call(&self, py: Python<'_>, callback: &Bound<'_, PyAny>) -> PyResult<()> {
        let before = self.loan.with_ref(py, |bytes| Ok(bytes[0]))?;
        callback.call1((before,))?;
        Ok(())
    }
    fn nested(&self, py: Python<'_>, callback: &Bound<'_, PyAny>) -> PyResult<()> {
        self.loan.with_mut(py, |bytes| {
            scoped::Frame::with_mut(py, bytes, |loan| {
                callback.call1((Py::new(py, Self { loan })?,))?;
                Ok(())
            })
        })
    }
}

#[test]
fn python_reentry_and_failure_keep_live_prefix() {
    Python::initialize();
    Python::attach(|py| {
        let owner = owner::Owner::new(vec![1u8]);
        let locals = PyDict::new(py);
        let error = owner.with_mut(|bytes| scoped::Frame::with_mut(py, &mut bytes[..], |loan| {
            locals.set_item("view", Py::new(py, BytesView { loan })?)?;
            py.run(c"view.set(10)\nview.call(lambda before: view.set(before + 1))\nassert view.get() == 11\nsaved = []\ndef edit(child):\n    child.set(12)\n    saved.append(child)\n    try: view.get()\n    except RuntimeError: pass\n    else: raise AssertionError('outer borrow lost')\nview.nested(edit)\nassert view.get() == 12\ntry: saved[0].get()\nexcept ReferenceError: pass\nelse: raise AssertionError('nested view survived')\ndef fail(child):\n    child.set(13)\n    raise ValueError('partial write')\nview.nested(fail)\n", Some(&locals), Some(&locals))
        })).unwrap_err();
        assert!(error.is_instance_of::<pyo3::exceptions::PyValueError>(py));
        assert_eq!(owner.with_ref(|bytes| Ok(bytes[0])).unwrap(), 13);
        let expired = locals
            .get_item("view")
            .unwrap()
            .unwrap()
            .call_method0("get")
            .unwrap_err();
        assert!(expired.is_instance_of::<pyo3::exceptions::PyReferenceError>(py));
    });
}

#[test]
fn retained_menu_query_records_the_current_invocation() {
    use cs705_core::icf::document::lifecycle::{
        MenuContext, MenuDescriptor, MenuQuery, RecentFiles,
    };
    Python::initialize();
    Python::attach(|py| {
        let locals = PyDict::new(py);
        py.run(
            c"def callback(view):\n    view.set(23)\n    raise ValueError('menu primary')\n",
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
        let holder =
            callbacks::CallbackHolder::new(py, &locals.get_item("callback").unwrap().unwrap())
                .unwrap();
        let link = callbacks::CallbackHolder::link(holder.bind(py));
        // The Send + Sync native closure owns only a weak CallbackLink.
        let query = MenuQuery::new(move |_, descriptor| {
            Python::attach(|py| {
                link.invoke(py, |callback| {
                    scoped::Frame::with_mut(py, &mut descriptor.caption[..], |loan| {
                        callback.call1((Py::new(py, BytesView { loan })?,))?;
                        Ok(())
                    })
                })
            })
            .map_err(|_| cs705_core::icf::Error::Operation("Python menu query failed".into()))
        });
        let mut document = cs705_core::icf::document::Document::from_current(vec![], vec![], 6);
        let mut session = cs705_core::icf::document::Session::new(
            cs705_core::icf::context::Context::before_registry(),
            None,
            0,
            [0, 0],
        );
        let settings = cs705_core::display::Settings {
            language: 0,
            system_language: 0,
            font_percent: 100,
            toolbar: 0,
            split: 0,
            callsign_dialog: 0,
            auto_correct: 0,
            fixed_name: 0,
            column_bits: 0,
            column_bits_high: 0,
        };
        let recent = RecentFiles::default();
        let mut context = MenuContext {
            document: &mut document,
            session: &mut session,
            settings: &settings,
            active: None,
            recent: &recent,
        };
        let mut descriptor = MenuDescriptor {
            group: 0,
            key: 0,
            index: 0,
            name: vec![],
            caption: vec![1],
            flags1d: 0,
            flags1e: 0,
            shortcut18: 0,
        };
        let (errors, native) = callbacks::CallbackError::scoped(py, || {
            py.detach(|| query.clone().call(&mut context, &mut descriptor))
        })
        .unwrap();
        let error = errors
            .finish(
                py,
                native.map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("native Operation")),
            )
            .unwrap_err();
        assert!(error.is_instance_of::<pyo3::exceptions::PyValueError>(py));
        assert!(error.traceback(py).is_some());
        assert!(error.context(py).is_some());
        assert_eq!(descriptor.caption, [23]);
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            callbacks::CallbackError::scoped::<()>(py, || panic!("invocation unwind"))
        }));
        assert!(unwind.is_err());
        descriptor.caption[0] = 1;
        assert!(query.call(&mut context, &mut descriptor).is_err());
        assert_eq!(descriptor.caption, [1]);
    });
}

#[test]
fn scoped_alias_expiration_and_thread() {
    Python::initialize();
    Python::attach(|py| {
        let mut data = vec![1usize, 2];
        let retained = scoped::Frame::with_mut(py, &mut data, |root| {
            root.with_mut(py, |value| {
                assert!(root.with_ref(py, |_| Ok(())).is_err());
                assert!(root.project_ref(py, |v| &v[0]).is_err());
                // Native nested callbacks explicitly register the supplied reborrow.
                scoped::Frame::with_mut(py, value, |nested| {
                    assert!(root.with_mut(py, |_| Ok(())).is_err());
                    nested.with_mut(py, |value| {
                        value[0] = 4;
                        Ok(())
                    })
                })
            })?;
            let child = root.project_mut(py, |value| &value[0], |value| &mut value[0])?;
            assert!(
                root.with_mut(py, |value| {
                    value.clear();
                    Ok(())
                })
                .is_err()
            );
            child.with_mut(py, |value| {
                assert!(root.with_ref(py, |_| Ok(())).is_err());
                *value = 5;
                Ok(())
            })?;
            root.with_ref(py, |_| {
                assert!(child.with_mut(py, |_| Ok(())).is_err());
                child.with_ref(py, |value| {
                    assert_eq!(*value, 5);
                    Ok(())
                })
            })?;
            let cross_thread = child.clone();
            py.detach(|| {
                std::thread::spawn(move || {
                    Python::attach(|py| {
                        assert!(
                            cross_thread
                                .with_ref(py, |_| Ok(()))
                                .unwrap_err()
                                .is_instance_of::<pyo3::exceptions::PyRuntimeError>(py)
                        );
                    })
                })
                .join()
                .unwrap()
            });
            Ok(child)
        })
        .unwrap();
        assert_eq!(data, [5, 2]);
        assert!(
            retained
                .with_ref(py, |_| Ok(()))
                .unwrap_err()
                .is_instance_of::<pyo3::exceptions::PyReferenceError>(py)
        );
        scoped::Frame::with_ref(py, &data, |loan| {
            assert!(loan.with_mut(py, |_| Ok(())).is_err());
            assert!(loan.project_mut(py, |v| &v[0], |v| &mut v[0]).is_err());
            let child = loan.project_ref(py, |v| &v[0])?;
            assert_eq!(child.with_ref(py, |value| Ok(*value))?, 5);
            Ok(())
        })
        .unwrap();
    });
}

#[test]
fn borrowed_icf_and_slice_are_live() {
    Python::initialize();
    Python::attach(|py| {
        let mut memory = vec![0u8; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9c0 - 16..].copy_from_slice(b"IcomCloneFormat3");
        let source = cs705_core::icf::Icf::from_memory(memory)
            .unwrap()
            .to_bytes();
        let mut icf = cs705_core::icf::Icf::parse(&source).unwrap();
        let expired = scoped::Frame::with_icf_mut(py, &mut icf, |loan| {
            loan.with_mut(py, |value| {
                value.patch_memory(&[(0, &[8])]).unwrap();
                Ok(())
            })?;
            let bytes = loan.project_ref(py, |value| value.memory())?;
            assert_eq!(bytes.with_ref(py, |bytes| Ok(bytes[0]))?, 8);
            assert!(loan.with_mut(py, |_| Ok(())).is_err());
            assert!(
                loan.project_mut(py, |value| value.memory(), |_| &mut [])
                    .is_ok()
            );
            assert_eq!(loan.with_ref(py, |value| Ok(value.memory()[0]))?, 8);
            Ok(loan)
        })
        .unwrap();
        assert_eq!(icf.memory()[0], 8);
        scoped::Frame::with_icf_ref(py, &icf, |loan| {
            assert!(loan.with_mut(py, |_| Ok(())).is_err());
            loan.with_ref(py, |value| {
                assert_eq!(value.original_bytes(), source);
                Ok(())
            })
        })
        .unwrap();
        drop(icf);
        drop(source);
        assert!(
            expired
                .with_ref(py, |_| Ok(()))
                .unwrap_err()
                .is_instance_of::<pyo3::exceptions::PyReferenceError>(py)
        );
        let mut bytes = [1u8, 2];
        scoped::Frame::with_mut(py, &mut bytes[..], |loan| {
            loan.with_mut(py, |bytes| {
                bytes[0] = 9;
                Ok(())
            })
        })
        .unwrap();
        assert_eq!(bytes, [9, 2]);
    });
}

#[test]
fn scoped_readonly_aliases_and_reborrow_unwind_fail_closed() {
    Python::initialize();
    Python::attach(|py| {
        let mut data = vec![1usize];
        scoped::Frame::with_mut(py, &mut data, |root| {
            let readonly = root.project_ref(py, |value| value)?;
            assert!(readonly.project_mut(py, |v| &v[0], |v| &mut v[0]).is_err());
            let reader = readonly.project_ref(py, |v| &v[0])?;
            let writer = root.project_mut(py, |v| &v[0], |v| &mut v[0])?;
            reader.with_ref(py, |_| {
                assert!(
                    writer
                        .with_mut::<()>(py, |_| panic!("must reject alias"))
                        .is_err()
                );
                Ok(())
            })?;
            let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                writer.with_mut::<()>(py, |value| {
                    *value = 8;
                    panic!("reborrow unwind")
                })
            }));
            assert!(unwind.is_err());
            // A caught callback unwind releases the reservation in this still-live
            // frame; it must neither roll back the prefix nor poison later access.
            assert_eq!(reader.with_ref(py, |value| Ok(*value))?, 8);
            writer.with_mut(py, |value| {
                *value = 9;
                Ok(())
            })
        })
        .unwrap();
        assert_eq!(data, [9]);
    });
}

#[test]
fn detach_and_unwind_restore_without_copyback() {
    Python::initialize();
    Python::attach(|py| {
        let owner = owner::Owner::new(vec![1usize]);
        let mut taken = owner.take().unwrap();
        py.detach(|| {
            assert!(owner.take().is_err());
            Python::attach(|py| {
                scoped::Frame::with_mut(py, &mut *taken, |loan| {
                    loan.with_mut(py, |value| {
                        value[0] = 6;
                        Ok(())
                    })
                })
            })
            .unwrap();
        });
        drop(taken);
        assert_eq!(owner.with_ref(|value| Ok(value[0])).unwrap(), 6);
        let retained = std::cell::RefCell::new(None);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            owner
                .with_mut::<()>(|value| {
                    scoped::Frame::with_mut(py, value, |loan| {
                        retained.replace(Some(loan.clone()));
                        loan.with_mut::<()>(py, |value| {
                            value[0] = 7;
                            panic!("check unwind")
                        })
                    })
                })
                .unwrap();
        }));
        assert!(result.is_err());
        assert_eq!(owner.with_ref(|value| Ok(value[0])).unwrap(), 7);
        assert!(
            retained
                .into_inner()
                .unwrap()
                .with_ref(py, |_| Ok(()))
                .unwrap_err()
                .is_instance_of::<pyo3::exceptions::PyReferenceError>(py)
        );
    });
}

#[test]
fn callbacks_preserve_exception_and_collect_holder_cycle() {
    Python::initialize();
    Python::attach(|py| {
        let locals = PyDict::new(py);
        callbacks::register(&pyo3::types::PyModule::new(py, "checks").unwrap()).unwrap();
        py.run(
            c"class Callable:\n    def __call__(self): return 42\ncallable = Callable()\n",
            None,
            Some(&locals),
        )
        .unwrap();
        let callable = locals.get_item("callable").unwrap().unwrap();
        let holder = callbacks::CallbackHolder::new(py, &callable).unwrap();
        let link = callbacks::CallbackHolder::link(holder.bind(py));
        let _cloned_link = link.clone();
        let referents = pyo3::types::PyModule::import(py, "gc")
            .unwrap()
            .call_method1("get_referents", (holder.bind(py),))
            .unwrap();
        assert_eq!(
            referents
                .try_iter()
                .unwrap()
                .filter(|v| v.as_ref().unwrap().is(&callable))
                .count(),
            1
        );
        drop(referents);
        assert_eq!(
            link.with_callable(py, |f| f.call0()?.extract::<usize>())
                .unwrap(),
            42
        );
        assert!(link.invoke(py, |_| Ok(())).is_err());
        let failing = py.eval(c"lambda: 1 / 0", None, None).unwrap();
        let failing_holder = callbacks::CallbackHolder::new(py, &failing).unwrap();
        let failing_link = callbacks::CallbackHolder::link(failing_holder.bind(py));
        let (outer, native) = callbacks::CallbackError::scoped(py, || {
            let (inner, native) = callbacks::CallbackError::scoped(py, || {
                failing_link.invoke(py, |f| f.call0().map(|_| ()))
            })
            .unwrap();
            let error = inner.finish(py, native).unwrap_err();
            assert!(error.traceback(py).is_some());
            // The handled recursive failure must not poison its parent invocation.
            py.detach(|| Python::attach(|py| link.invoke(py, |f| f.call0()?.extract::<usize>())))
        })
        .unwrap();
        assert_eq!(outer.finish(py, native).unwrap(), 42);
        let (errors, native) = callbacks::CallbackError::scoped(py, || {
            let original = failing_link
                .invoke(py, |f| f.call0().map(|_| ()))
                .unwrap_err();
            assert!(
                failing_link
                    .invoke::<()>(py, |_| panic!("must stop after primary failure"))
                    .is_err()
            );
            assert!(
                link.invoke_cleanup::<()>(py, |_| Err(pyo3::exceptions::PyValueError::new_err(
                    "close"
                )))
                .is_err()
            );
            Err::<(), _>(original)
        })
        .unwrap();
        let error = errors.finish(py, native).unwrap_err();
        assert!(error.is_instance_of::<pyo3::exceptions::PyZeroDivisionError>(py));
        assert!(error.traceback(py).is_some());
        assert!(error.context(py).is_some());
        callable.setattr("holder", holder.bind(py)).unwrap();
        let weak = pyo3::types::PyModule::import(py, "weakref")
            .unwrap()
            .getattr("ref")
            .unwrap()
            .call1((&callable,))
            .unwrap();
        drop(holder);
        drop(callable);
        locals.clear();
        pyo3::types::PyModule::import(py, "gc")
            .unwrap()
            .call_method0("collect")
            .unwrap();
        assert!(weak.call0().unwrap().is_none());
        assert!(link.with_callable(py, |_| Ok(())).is_err());

        let original = pyo3::exceptions::PyValueError::new_err("primary");
        let identity = original.value(py).clone().unbind();
        let mut errors = callbacks::CallbackError::default();
        assert!(errors.capture::<()>(Err(original)).is_err());
        assert!(
            errors
                .invoke::<()>(|| panic!("failed callbacks must not be invoked again"))
                .is_err()
        );
        assert!(
            errors
                .capture::<()>(Err(pyo3::exceptions::PyOSError::new_err("cleanup")))
                .is_err()
        );
        let error = errors
            .finish_with_outcome::<()>(
                py,
                Ok(()),
                7usize.into_pyobject(py).unwrap().into_any().unbind(),
            )
            .unwrap_err();
        assert!(error.value(py).is(identity.bind(py)));
        let context = error.context(py).unwrap();
        assert_eq!(
            context
                .value(py)
                .getattr("exceptions")
                .unwrap()
                .len()
                .unwrap(),
            2
        );
        assert!(!callbacks::CallbackError::default().failed());

        let native = pyo3::exceptions::PyOSError::new_err("native primary");
        let identity = native.value(py).clone().unbind();
        let mut cleanup = callbacks::CallbackError::default();
        assert!(
            cleanup
                .capture_cleanup::<()>(Err(pyo3::exceptions::PyValueError::new_err("close")))
                .is_err()
        );
        let error = cleanup.finish::<()>(py, Err(native)).unwrap_err();
        assert!(error.value(py).is(identity.bind(py)));
        assert!(error.context(py).is_some());
    });
}

#[test]
fn owner_busy_restore_consume() {
    let owner = owner::Owner::new(1usize);
    {
        let mut taken = owner.take().unwrap();
        assert!(owner.take().is_err());
        *taken = 2;
    }
    assert_eq!(owner.with_ref(|value| Ok(*value)).unwrap(), 2);
    let result: pyo3::PyResult<()> = owner.with_mut(|value| {
        *value = 3;
        Err(pyo3::exceptions::PyValueError::new_err("stop"))
    });
    assert!(result.is_err());
    assert_eq!(owner.take().unwrap().consume(), 3);
    assert!(owner.take().is_err());
}
