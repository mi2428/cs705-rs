//! Synchronous print tasks and editor dispatch over scoped native references.
use super::*;
use crate::icf::document::adapters::{attach_checked, require_none};
use crate::{
    callbacks::CallbackError,
    scoped::{Frame, Loan},
};
use pyo3::types::PyTuple;
owner!(Editor, core::Editor);
#[pymethods]
impl Editor {
    #[new]
    fn new(
        validation_guard: &Bound<'_, PyAny>,
        visible: &Bound<'_, PyAny>,
        text: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self::from_core(core::Editor {
            validation_guard: strict_u32(validation_guard)?,
            visible: strict_bool(visible)?,
            text: bytes(text)?,
        }))
    }
    #[getter]
    fn validation_guard(&self) -> PyResult<u32> {
        self.with_ref(|v| Ok(v.validation_guard))
    }
    #[setter]
    fn set_validation_guard(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_u32(v)?;
        self.with_mut(|v| {
            v.validation_guard = n;
            Ok(())
        })
    }
    #[getter]
    fn visible(&self) -> PyResult<bool> {
        self.with_ref(|v| Ok(v.visible))
    }
    #[setter]
    fn set_visible(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_bool(v)?;
        self.with_mut(|v| {
            v.visible = n;
            Ok(())
        })
    }
    #[getter]
    fn text<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let v = self.with_ref(|v| Ok(v.text.clone()))?;
        Ok(PyBytes::new(py, &v))
    }
    #[setter]
    fn set_text(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = bytes(v)?;
        self.with_mut(|v| {
            v.text = n;
            Ok(())
        })
    }
}
#[pyclass(module = "cs705.printing", frozen)]
pub struct TextView {
    loan: Loan<[u8]>,
}
#[pymethods]
impl TextView {
    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        self.loan.with_ref(py, |v| Ok(v.len()))
    }
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let v = self.loan.with_ref(py, |v| Ok(v.to_vec()))?;
        Ok(PyBytes::new(py, &v))
    }
}
#[pyfunction]
fn sync_editor(py: Python<'_>, editor: Py<Editor>, dispatch_phase1: Py<PyAny>) -> PyResult<()> {
    let mut taken = editor.get().target.take()?;
    let interpreter = crate::scoped::interpreter(py)?;
    let (errors, native) = py.detach(move || {
        let mut errors = CallbackError::default();
        let native = core::sync_editor(&mut taken, |text| {
            let result = attach_checked(interpreter, |py| {
                Frame::with_ref(py, text, |loan| {
                    require_none(
                        dispatch_phase1
                            .bind(py)
                            .call1((Py::new(py, TextView { loan })?,))?,
                    )
                })
            });
            errors
                .capture(result)
                .map_err(|_| std::io::Error::other("Python editor dispatch failed"))
        });
        drop(taken);
        (errors, native)
    });
    errors.finish(py, native.map_err(crate::errors::io))
}
#[pyclass(module = "cs705.printing")]
pub struct Event {
    pub(crate) inner: core::Event,
}
macro_rules! events {($($name:ident),*)=>{#[pymethods]impl Event{
 $(#[staticmethod]#[allow(non_snake_case)]fn $name()->Self{Self{inner:core::Event::$name}})*
 #[staticmethod]#[allow(non_snake_case)]fn Tick(due_ms:&Bound<'_,PyAny>,name:String)->PyResult<Self>{Ok(Self{inner:core::Event::Tick{due_ms:strict_u64(due_ms)?,name}})}
 #[getter]fn variant(&self)->&'static str{match self.inner{$(core::Event::$name=>stringify!($name),)* core::Event::Tick{..}=>"Tick"}}
  #[getter]fn due_ms(&self)->Option<u64>{match self.inner{core::Event::Tick{due_ms,..}=>Some(due_ms),_=>None}}
  #[setter]fn set_due_ms(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{let value=strict_u64(value)?;match &mut self.inner{core::Event::Tick{due_ms,..}=>{*due_ms=value;Ok(())},_=>Err(pyo3::exceptions::PyTypeError::new_err("due_ms requires Event.Tick"))}}
  #[getter]fn name(&self)->Option<String>{match &self.inner{core::Event::Tick{name,..}=>Some(name.clone()),_=>None}}
  #[setter]fn set_name(&mut self,value:String)->PyResult<()>{match &mut self.inner{core::Event::Tick{name,..}=>{*name=value;Ok(())},_=>Err(pyo3::exceptions::PyTypeError::new_err("name requires Event.Tick"))}}
}}}
events!(BeginDoc, NewPage, Page, EndDoc, ClearNames);
owner!(Job, core::Job);
#[pymethods]
impl Job {
    #[staticmethod]
    fn begin(names: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(
            core::Job::begin(strings(names)?).map_err(crate::errors::io)?,
        ))
    }
    #[getter]
    fn cancelled(&self) -> PyResult<bool> {
        self.with_ref(|v| Ok(v.cancelled))
    }
    #[setter]
    fn set_cancelled(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_bool(v)?;
        self.with_mut(|v| {
            v.cancelled = n;
            Ok(())
        })
    }
    #[getter]
    fn pages(&self) -> PyResult<usize> {
        self.with_ref(|v| Ok(v.pages))
    }
    #[setter]
    fn set_pages(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_usize(v)?;
        self.with_mut(|v| {
            v.pages = n;
            Ok(())
        })
    }
    #[getter]
    fn ended(&self) -> PyResult<bool> {
        self.with_ref(|v| Ok(v.ended))
    }
    #[setter]
    fn set_ended(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_bool(v)?;
        self.with_mut(|v| {
            v.ended = n;
            Ok(())
        })
    }
    fn events_snapshot(&self) -> PyResult<Vec<Event>> {
        Ok(self
            .with_ref(|v| Ok(v.events.clone()))?
            .into_iter()
            .map(|inner| Event { inner })
            .collect())
    }
    fn set_events(&self, py: Python<'_>, v: Vec<Py<Event>>) -> PyResult<()> {
        let n = v
            .iter()
            .map(|v| Ok(v.try_borrow(py)?.inner.clone()))
            .collect::<PyResult<Vec<_>>>()?;
        self.with_mut(|v| {
            v.events = n;
            Ok(())
        })
    }
    fn begin_page(&self) -> PyResult<()> {
        self.with_mut(|v| {
            v.begin_page();
            Ok(())
        })
    }
    fn page_drawn(&self) -> PyResult<()> {
        self.with_mut(|v| {
            if v.pages == usize::MAX {
                return Err(pyo3::exceptions::PyOverflowError::new_err(
                    "page count exhausted",
                ));
            }
            v.page_drawn();
            Ok(())
        })
    }
    fn finish(&self) -> PyResult<()> {
        self.with_mut(|v| {
            v.finish();
            Ok(())
        })
    }
    fn remaining_names<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        let names = self.with_ref(|v| Ok(v.remaining_names().to_vec()))?;
        PyTuple::new(py, names)
    }
    fn tick(&self, py: Python<'_>, render: Py<PyAny>) -> PyResult<bool> {
        let mut job = self.target.take()?;
        if !job.ended && job.remaining_names().is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "active print job has no remaining name",
            ));
        }
        let interpreter = crate::scoped::interpreter(py)?;
        let (errors, native) = py.detach(move || {
            let mut errors = CallbackError::default();
            let native = job.tick(|name, job| {
                let result = attach_checked(interpreter, |py| {
                    Frame::with_mut(py, job, |loan| {
                        require_none(
                            render
                                .bind(py)
                                .call1((name, Py::new(py, Self::from_loan(loan))?))?,
                        )
                    })
                });
                errors
                    .capture(result)
                    .map_err(|_| std::io::Error::other("Python print renderer failed"))
            });
            drop(job);
            (errors, native)
        });
        errors.finish(py, native.map_err(crate::errors::io))
    }
}
#[pyfunction]
fn eligible(
    py: Python<'_>,
    bc: &Bound<'_, PyAny>,
    order: &Bound<'_, PyAny>,
    dataset: Py<PyAny>,
) -> PyResult<bool> {
    let bc = strict_u8(bc)?;
    let order = match crate::icf::enum_value(order, "cs705.printing", "PredicateOrder")? {
        0 => core::PredicateOrder::BaseFirst,
        1 => core::PredicateOrder::DatasetFirst,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "invalid PredicateOrder",
            ));
        }
    };
    let interpreter = crate::scoped::interpreter(py)?;
    let mut errors = CallbackError::default();
    let native = core::eligible(bc, order, || {
        let result = attach_checked(interpreter, |py| strict_bool(&dataset.bind(py).call0()?));
        errors
            .capture(result)
            .map_err(|_| std::io::Error::other("Python print dataset predicate failed"))
    });
    errors.finish(py, native.map_err(crate::errors::io))
}
#[pyfunction]
fn split_index(
    current: &Bound<'_, PyAny>,
    view: &crate::display::ViewState,
    index: &Bound<'_, PyAny>,
    expand: &Bound<'_, PyAny>,
) -> PyResult<i32> {
    let current = strict_bytes(current)?;
    let index = strict_i32(index)?;
    let expand = strict_bool(expand)?;
    view.with_ref(|v| core::split_index(current, v, index, expand).map_err(crate::errors::io))
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Editor>()?;
    m.add_class::<Job>()?;
    m.add_class::<Event>()?;
    m.add_class::<TextView>()?;
    crate::presentation::add_enum(
        m,
        "cs705.printing",
        "PredicateOrder",
        &["BaseFirst", "DatasetFirst"],
    )?;
    m.add_function(wrap_pyfunction!(sync_editor, m)?)?;
    m.add_function(wrap_pyfunction!(eligible, m)?)?;
    m.add_function(wrap_pyfunction!(split_index, m)?)?;
    Ok(())
}
