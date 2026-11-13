//! UR runners over the canonical WorkingUr and caller-owned text buffer.
use super::{WorkingUr, native::Value};
use crate::icf::{
    document::adapters::attach_checked,
    normalizer::ReferenceImage,
    settings::extended::FixedTextInput,
    values::{Input, Output},
};
use crate::{callbacks::CallbackError, conversion::*, errors};
use cs705_core::icf::rows::ur as core;
use pyo3::{PyTraverseError, PyVisit, prelude::*, types::PyList};
use std::cell::RefCell;

impl Input<'_> for core::Field {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        match crate::icf::enum_value(value, "cs705.icf.rows.ur", "Field")? {
            0 => Ok(Self::Name),
            1 => Ok(Self::Call),
            _ => Err(pyo3::exceptions::PyValueError::new_err("invalid UR field")),
        }
    }
}
impl Output for core::Field {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        crate::icf::enum_result(py, "cs705.icf.rows.ur", "Field", self as i64)
    }
}
#[pyfunction]
fn _field_key(value: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(core::Field::input(value)?.key())
}
macro_rules! value{($name:ident{$($field:ident:$ty:ty=>$set:ident,)*}snapshots{$($nested:ident:$nty:ty=>$get:ident,$nset:ident,)*}methods{$($methods:tt)*})=>{
    #[pyclass(module="cs705.icf.rows.ur",frozen)]pub struct $name{state:Value<core::$name>}
    impl $name{fn from_core(v:core::$name)->Self{Self{state:Value::new(v)}}}
    #[pymethods]impl $name{
        #[new]
        #[expect(clippy::too_many_arguments, reason = "UR frame and effect constructors expose all independent native editor fields")]
        fn new($($field:&Bound<'_,PyAny>,)*$($nested:&Bound<'_,PyAny>,)*)->PyResult<Self>{Ok(Self::from_core(core::$name{$($field:<$ty as Input>::input($field)?,)*$($nested:<$nty as Input>::input($nested)?,)*}))}
        $(#[getter]fn $field<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{let v=self.state.read(py,|v|Ok(v.$field.clone()))?;v.output(py)}
        #[setter]fn $set(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()>{let v=<$ty as Input>::input(value)?;self.state.edit(py,|o|{o.$field=v;Ok(())})})*
        $(fn $get<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{let v=self.state.read(py,|v|Ok(v.$nested.clone()))?;v.output(py)}
        fn $nset(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()>{let v=<$nty as Input>::input(value)?;self.state.edit(py,|o|{o.$nested=v;Ok(())})})*
        $($methods)*
    }
}}
impl Input<'_> for Vec<u32> {
    fn input(v: &Bound<'_, PyAny>) -> PyResult<Self> {
        v.try_iter()?.map(|v| strict_u32(&v?)).collect()
    }
}
impl Output for Vec<u32> {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(PyList::new(py, self)?.into_any())
    }
}
value!(Frame{code:i32=>set_code,command:u32=>set_command,blocked:bool=>set_blocked,view_last:i32=>set_view_last,view_enabled:u32=>set_view_enabled,node_flags:u8=>set_node_flags,warning_bits:u8=>set_warning_bits,notify_flags:u32=>set_notify_flags,ur_changed:bool=>set_ur_changed,}snapshots{}methods{});
value!(Effect{stored:bool=>set_stored,inserted:bool=>set_inserted,full_warning:bool=>set_full_warning,blocked:bool=>set_blocked,view_bad:bool=>set_view_bad,redirect:Option<u32> =>set_redirect,state_out:u32=>set_state_out,display:Option<Vec<u8>> =>set_display,}snapshots{notifications:Vec<u32> =>notifications_snapshot,set_notifications,}methods{#[staticmethod]fn default()->Self{Self::from_core(core::Effect::default())}});

#[pyclass(module = "cs705.icf.rows.ur")]
pub struct Request {
    index: u32,
    field: core::Field,
    alignment: u32,
    #[pyo3(get, set)]
    input: Py<FixedTextInput>,
    #[pyo3(get, set)]
    generation: Py<ReferenceImage>,
}
#[pymethods]
impl Request {
    #[new]
    fn new(
        index: &Bound<'_, PyAny>,
        field: &Bound<'_, PyAny>,
        input: Py<FixedTextInput>,
        alignment: &Bound<'_, PyAny>,
        generation: Py<ReferenceImage>,
    ) -> PyResult<Self> {
        Ok(Self {
            index: strict_u32(index)?,
            field: core::Field::input(field)?,
            input,
            alignment: strict_u32(alignment)?,
            generation,
        })
    }
    #[getter]
    fn index(&self) -> u32 {
        self.index
    }
    #[setter]
    fn set_index(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.index = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn field<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.field.output(py)
    }
    #[setter]
    fn set_field(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.field = core::Field::input(value)?;
        Ok(())
    }
    #[getter]
    fn alignment(&self) -> u32 {
        self.alignment
    }
    #[setter]
    fn set_alignment(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.alignment = strict_u32(value)?;
        Ok(())
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.input)?;
        visit.call(&self.generation)
    }
}
impl Request {
    fn inputs(&self, py: Python<'_>) -> Self {
        Self {
            index: self.index,
            field: self.field,
            input: self.input.clone_ref(py),
            alignment: self.alignment,
            generation: self.generation.clone_ref(py),
        }
    }
}

fn run(
    py: Python<'_>,
    model: Py<WorkingUr>,
    request: Py<Request>,
    frame: Py<Frame>,
    effect: Py<Effect>,
    nls: Option<(Py<PyAny>, Py<PyAny>)>,
) -> PyResult<()> {
    let interpreter = crate::scoped::interpreter(py)?;
    let r = request.try_borrow(py)?.inputs(py);
    let (result, errors) = {
        let mut frame = frame.get().state.take()?;
        let mut effect = effect.get().state.take()?;
        let mut model = model.get().take()?;
        r.generation.get().with_core(py, |generation| {
            r.input.get().with_core(py, |input| {
                Ok(py.detach(|| {
                    let errors = RefCell::new(CallbackError::default());
                    let request = core::Request {
                        index: r.index,
                        field: r.field,
                        input,
                        alignment: r.alignment,
                        generation,
                    };
                    let result = match nls {
                        None => model.native_text(request, &mut frame, &mut effect),
                        Some((map, upper)) => model.native_text_with_nls(
                            request,
                            &mut frame,
                            &mut effect,
                            |source, dest| {
                                let result = attach_checked(interpreter, |py| {
                                    crate::csv::nls::map_call(py, map.bind(py), source, dest)
                                });
                                errors.borrow_mut().capture(result).map_err(|_| {
                                    cs705_core::csv::Error::Operation(
                                        "UR mapping callback failed".into(),
                                    )
                                })
                            },
                            |dest| {
                                let result = attach_checked(interpreter, |py| {
                                    crate::csv::nls::upper_call(py, upper.bind(py), dest)
                                });
                                errors.borrow_mut().capture(result).map_err(|_| {
                                    cs705_core::csv::Error::Operation(
                                        "UR uppercase callback failed".into(),
                                    )
                                })
                            },
                        ),
                    };
                    (result, errors.into_inner())
                }))
            })
        })?
    };
    errors.finish(py, result.map_err(errors::icf))
}
#[pyfunction]
fn native_text(
    py: Python<'_>,
    model: Py<WorkingUr>,
    request: Py<Request>,
    frame: Py<Frame>,
    effect: Py<Effect>,
) -> PyResult<()> {
    run(py, model, request, frame, effect, None)
}
#[pyfunction]
fn native_text_with_nls(
    py: Python<'_>,
    model: Py<WorkingUr>,
    request: Py<Request>,
    frame: Py<Frame>,
    effect: Py<Effect>,
    map: Py<PyAny>,
    upper: Py<PyAny>,
) -> PyResult<()> {
    run(py, model, request, frame, effect, Some((map, upper)))
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "ur")?;
    crate::icf::add_enum(&m, "Field", &[("Name", 0), ("Call", 1)])?;
    m.getattr("Field")?
        .setattr("__module__", "cs705.icf.rows.ur")?;
    m.add_function(wrap_pyfunction!(_field_key, &m)?)?;
    m.add_class::<Frame>()?;
    m.add_class::<Request>()?;
    m.add_class::<Effect>()?;
    m.add_function(wrap_pyfunction!(native_text, &m)?)?;
    m.add_function(wrap_pyfunction!(native_text_with_nls, &m)?)?;
    parent.add_submodule(&m)
}
