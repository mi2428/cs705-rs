//! Native repeater editor requests and observable effects.
use super::{fields::Effects, working::Working};
use crate::icf::settings::extended::{ChoiceInput, NumberInput, TextInput};
use crate::{
    callbacks::{CallbackError, CallbackHolder},
    conversion::*,
    errors,
    icf::{context::Context, frequency::Ranges, record::DecodedRecord},
    owner::Owner,
};
use cs705_core::repeater::editor as core;
use pyo3::types::PyBytes;
use pyo3::{PyTraverseError, PyVisit, prelude::*};
use std::cell::RefCell;

#[pyclass(module = "cs705.repeater.editor")]
pub struct Runtime {
    ranges: Py<Ranges>,
    live_q: Py<Context>,
    system_langid: Option<u16>,
    generation: Option<Vec<u8>>,
    align_callsign: bool,
    crt_decimal: u8,
}
impl Runtime {
    fn with_core<R>(
        &self,
        _py: Python<'_>,
        f: impl FnOnce(&core::Runtime<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let q = self.live_q.get().take()?;
        self.ranges.get().with_ref(|ranges| {
            f(&core::Runtime {
                ranges,
                live_q: &q,
                system_langid: self.system_langid,
                generation: self.generation.as_deref(),
                align_callsign: self.align_callsign,
                crt_decimal: self.crt_decimal,
            })
        })
    }
}
#[pymethods]
impl Runtime {
    #[new]
    #[pyo3(signature=(ranges,live_q,system_langid,generation,align_callsign,crt_decimal))]
    fn new(
        ranges: Py<Ranges>,
        live_q: Py<Context>,
        system_langid: Option<&Bound<'_, PyAny>>,
        generation: Option<&Bound<'_, PyAny>>,
        align_callsign: &Bound<'_, PyAny>,
        crt_decimal: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            ranges,
            live_q,
            system_langid: system_langid.map(strict_u16).transpose()?,
            generation: generation
                .map(|v| Ok::<_, PyErr>(strict_bytes(v)?.to_vec()))
                .transpose()?,
            align_callsign: strict_bool(align_callsign)?,
            crt_decimal: strict_u8(crt_decimal)?,
        })
    }
    #[getter]
    fn ranges(&self, py: Python<'_>) -> Py<Ranges> {
        self.ranges.clone_ref(py)
    }
    #[setter]
    fn set_ranges(&mut self, v: Py<Ranges>) {
        self.ranges = v;
    }
    #[getter]
    fn live_q(&self, py: Python<'_>) -> Py<Context> {
        self.live_q.clone_ref(py)
    }
    #[setter]
    fn set_live_q(&mut self, v: Py<Context>) {
        self.live_q = v;
    }
    #[getter]
    fn system_langid(&self) -> Option<u16> {
        self.system_langid
    }
    #[setter]
    fn set_system_langid(&mut self, v: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.system_langid = v.map(strict_u16).transpose()?;
        Ok(())
    }
    #[getter]
    fn generation<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.generation.as_ref().map(|v| PyBytes::new(py, v))
    }
    #[setter]
    fn set_generation(&mut self, v: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.generation = v
            .map(|v| Ok::<_, PyErr>(strict_bytes(v)?.to_vec()))
            .transpose()?;
        Ok(())
    }
    #[getter]
    fn align_callsign(&self) -> bool {
        self.align_callsign
    }
    #[setter]
    fn set_align_callsign(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.align_callsign = strict_bool(v)?;
        Ok(())
    }
    #[getter]
    fn crt_decimal(&self) -> u8 {
        self.crt_decimal
    }
    #[setter]
    fn set_crt_decimal(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.crt_decimal = strict_u8(v)?;
        Ok(())
    }
}

/// Mutable nested values are explicit snapshots with replacement setters.
#[pyclass(module = "cs705.repeater.editor", frozen)]
pub struct Effect {
    inner: Owner<core::Effect>,
}
macro_rules! effect_fields {
    ($($name:ident,$setter:ident=>$ty:ty,$strict:ident);* $(;)?)=>{
        #[pymethods] impl Effect {
            #[new] fn new()->Self{Self {inner:Owner::new(core::Effect::default())}}
            fn __richcmp__(&self,other:&Self,op:pyo3::basic::CompareOp)->PyResult<bool>{let equal=if std::ptr::eq(self,other){self.inner.with_ref(|_|Ok(true))?}else{self.inner.with_ref(|a|other.inner.with_ref(|b|Ok(a==b)))?};super::equality(equal,op)}
            $(#[getter] fn $name(&self)->PyResult<$ty>{self.inner.with_ref(|e|Ok(e.$name))}
            #[setter] fn $setter(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let v=$strict(v)?;self.inner.with_mut(|e|{e.$name=v;Ok(())})})*
            fn native_snapshot(&self)->PyResult<Effects>{self.inner.with_ref(|e|Ok(Effects {inner:Owner::new(e.native)}))}
            fn set_native(&self,v:&Effects)->PyResult<()>{let v=v.inner.with_ref(|e|Ok(*e))?;self.inner.with_mut(|e|{e.native=v;Ok(())})}
            #[getter] fn display<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyBytes>>{self.inner.with_ref(|e|Ok(PyBytes::new(py,&e.display)))}
            #[setter] fn set_display(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let v=strict_bytes(v)?.to_vec();self.inner.with_mut(|e|{e.display=v;Ok(())})}
            #[getter] fn shared_input<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyBytes>>{self.inner.with_ref(|e|Ok(PyBytes::new(py,&e.shared_input)))}
            #[setter] fn set_shared_input(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let v=strict_bytes(v)?.to_vec();self.inner.with_mut(|e|{e.shared_input=v;Ok(())})}
            #[getter] fn source_error(&self)->PyResult<Option<String>>{self.inner.with_ref(|e|Ok(e.source_error.clone()))}
            #[setter] fn set_source_error(&self,v:Option<String>)->PyResult<()>{self.inner.with_mut(|e|{e.source_error=v;Ok(())})}
            #[getter] fn candidate(&self)->PyResult<Option<i64>>{self.inner.with_ref(|e|Ok(e.candidate))}
            #[setter] fn set_candidate(&self,v:Option<&Bound<'_,PyAny>>)->PyResult<()>{let v=v.map(strict_i64).transpose()?;self.inner.with_mut(|e|{e.candidate=v;Ok(())})}
            fn notifications_snapshot(&self)->PyResult<Vec<(u32,bool)>>{self.inner.with_ref(|e|Ok(e.notifications.clone()))}
            fn set_notifications(&self,v:&Bound<'_,PyAny>)->PyResult<()>{
                let v=v.try_iter()?.map(|v|{let(a,b):(Bound<'_,PyAny>,Bound<'_,PyAny>)=v?.extract()?;Ok((strict_u32(&a)?,strict_bool(&b)?))}).collect::<PyResult<_>>()?;
                self.inner.with_mut(|e|{e.notifications=v;Ok(())})
            }
            #[getter] fn group_label<'py>(&self,py:Python<'py>)->PyResult<Option<(u32,Bound<'py,PyBytes>)>>{self.inner.with_ref(|e|Ok(e.group_label.as_ref().map(|(n,b)|(*n,PyBytes::new(py,b)))))}
            #[setter] fn set_group_label(&self,v:Option<(Bound<'_,PyAny>,Bound<'_,PyAny>)>)->PyResult<()>{let v=v.map(|(a,b)|Ok::<_,PyErr>((strict_u32(&a)?,strict_bytes(&b)?.to_vec()))).transpose()?;self.inner.with_mut(|e|{e.group_label=v;Ok(())})}
            fn coordinate_dialog_snapshot(&self)->PyResult<Option<crate::csv::number::CoordinateDialog>>{self.inner.with_ref(|e|Ok(e.coordinate_dialog.clone().map(crate::csv::number::CoordinateDialog::from_core)))}
            fn set_coordinate_dialog(&self,py:Python<'_>,value:Option<Py<crate::csv::number::CoordinateDialog>>)->PyResult<()>{
                let value=value.map(|v|v.get().with_ref(py,|d|Ok(d.clone()))).transpose()?;
                self.inner.with_mut(|e|{e.coordinate_dialog=value;Ok(())})
            }
        }
    }
}
effect_fields!(state_out,set_state_out=>u32,strict_u32;popup_requested,set_popup_requested=>bool,strict_bool;blocked,set_blocked=>bool,strict_bool;gray,set_gray=>bool,strict_bool;inserted,set_inserted=>bool,strict_bool;helper_accepted,set_helper_accepted=>bool,strict_bool;refresh_if_binding_matches,set_refresh_if_binding_matches=>bool,strict_bool);

#[pyclass(module = "cs705.repeater.editor")]
pub struct Nls {
    map: Option<Py<CallbackHolder>>,
    upper: Option<Py<CallbackHolder>>,
}
#[pymethods]
impl Nls {
    #[new]
    fn new(py: Python<'_>, map: &Bound<'_, PyAny>, upper: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            map: Some(CallbackHolder::new(py, map)?),
            upper: Some(CallbackHolder::new(py, upper)?),
        })
    }
    #[getter]
    fn map(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        callable(py, &self.map)
    }
    #[setter]
    fn set_map(&mut self, py: Python<'_>, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.map = Some(CallbackHolder::new(py, v)?);
        Ok(())
    }
    #[getter]
    fn upper(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        callable(py, &self.upper)
    }
    #[setter]
    fn set_upper(&mut self, py: Python<'_>, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.upper = Some(CallbackHolder::new(py, v)?);
        Ok(())
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.map)?;
        visit.call(&self.upper)
    }
    fn __clear__(&mut self) {
        self.map = None;
        self.upper = None;
    }
}
fn choice_field(v: &Bound<'_, PyAny>) -> PyResult<core::ChoiceField> {
    use core::ChoiceField::*;
    Ok(
        match crate::icf::enum_value(v, "cs705.repeater.editor", "ChoiceField")? {
            0 => Type,
            1 => Mode,
            2 => Duplex,
            3 => Tone,
            4 => Use,
            5 => Position,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "invalid ChoiceField",
                ));
            }
        },
    )
}
fn number_field(v: &Bound<'_, PyAny>) -> PyResult<core::NumberField> {
    use core::NumberField::*;
    Ok(
        match crate::icf::enum_value(v, "cs705.repeater.editor", "NumberField")? {
            0 => Frequency,
            1 => Offset,
            2 => ToneFrequency,
            3 => UtcOffset,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "invalid NumberField",
                ));
            }
        },
    )
}
fn text_field(v: &Bound<'_, PyAny>) -> PyResult<core::TextField> {
    use core::TextField::*;
    Ok(
        match crate::icf::enum_value(v, "cs705.repeater.editor", "TextField")? {
            0 => Name,
            1 => SubName,
            2 => Call,
            3 => Gateway,
            _ => return Err(pyo3::exceptions::PyValueError::new_err("invalid TextField")),
        },
    )
}
#[pyfunction]
fn group_name(
    py: Python<'_>,
    image: &crate::icf::Icf,
    group: &Bound<'_, PyAny>,
    input: &TextInput,
    effect: &Effect,
) -> PyResult<()> {
    let group = strict_usize(group)?;
    input.with_core(py, |input| {
        image.with_mut(|image| {
            effect.inner.with_mut(|effect| {
                core::group_name(image, group, input, effect).map_err(errors::csv)
            })
        })
    })
}
#[pyfunction]
fn choice(
    py: Python<'_>,
    w: &Working,
    holder: &DecodedRecord,
    field: &Bound<'_, PyAny>,
    input: &ChoiceInput,
    r: &Runtime,
    effect: &Effect,
) -> PyResult<()> {
    let field = choice_field(field)?;
    let input = input.to_core(py)?;
    r.with_core(py, |r| {
        w.inner.with_mut(|w| {
            holder.with_mut(|h| {
                effect
                    .inner
                    .with_mut(|e| core::choice(w, h, field, input, r, e).map_err(errors::csv))
            })
        })
    })
}
#[pyfunction]
#[expect(
    clippy::too_many_arguments,
    reason = "Preserves native number editor arguments, mutable effects and explicit NLS callbacks."
)]
fn number(
    py: Python<'_>,
    w: &Working,
    holder: &DecodedRecord,
    field: &Bound<'_, PyAny>,
    input: &NumberInput,
    r: &Runtime,
    effect: &Effect,
    nls: Py<Nls>,
) -> PyResult<()> {
    let field = number_field(field)?;
    let errors = RefCell::new(CallbackError::default());
    let native = input.with_core(py, |input| {
        r.with_core(py, |r| {
            Nls::with_core(py, &nls, &errors, |nls| {
                w.inner.with_mut(|w| {
                    holder.with_mut(|h| {
                        effect.inner.with_mut(|e| {
                            core::number(w, h, field, input, r, e, nls).map_err(errors::csv)
                        })
                    })
                })
            })
        })
    });
    errors.into_inner().finish(py, native)
}
#[pyfunction]
#[expect(
    clippy::too_many_arguments,
    reason = "Preserves native text editor arguments, mutable effects and explicit NLS callbacks."
)]
fn text(
    py: Python<'_>,
    w: &Working,
    holder: &DecodedRecord,
    field: &Bound<'_, PyAny>,
    input: &TextInput,
    r: &Runtime,
    effect: &Effect,
    nls: Py<Nls>,
) -> PyResult<()> {
    let field = text_field(field)?;
    let errors = RefCell::new(CallbackError::default());
    let native = input.with_core(py, |input| {
        r.with_core(py, |r| {
            Nls::with_core(py, &nls, &errors, |nls| {
                w.inner.with_mut(|w| {
                    holder.with_mut(|h| {
                        effect.inner.with_mut(|e| {
                            core::text(w, h, field, input, r, e, nls).map_err(errors::csv)
                        })
                    })
                })
            })
        })
    });
    errors.into_inner().finish(py, native)
}
#[pyfunction]
fn coordinate(
    py: Python<'_>,
    w: &Working,
    holder: &DecodedRecord,
    longitude: &Bound<'_, PyAny>,
    input: &TextInput,
    r: &Runtime,
    effect: &Effect,
) -> PyResult<()> {
    let longitude = strict_bool(longitude)?;
    input.with_core(py, |input| {
        r.with_core(py, |r| {
            w.inner.with_mut(|w| {
                holder.with_mut(|h| {
                    effect.inner.with_mut(|e| {
                        core::coordinate(w, h, longitude, input, r, e).map_err(errors::csv)
                    })
                })
            })
        })
    })
}
#[pyfunction]
#[expect(
    clippy::too_many_arguments,
    reason = "Preserves native coordinate input arguments and the ordered modal event sequence."
)]
fn coordinate_input(
    py: Python<'_>,
    w: &Working,
    holder: &DecodedRecord,
    longitude: &Bound<'_, PyAny>,
    input: &TextInput,
    events: &Bound<'_, PyAny>,
    r: &Runtime,
    effect: &Effect,
) -> PyResult<()> {
    let longitude = strict_bool(longitude)?;
    let events = events
        .try_iter()?
        .map(|v| crate::csv::number::coordinate_event(&v?))
        .collect::<PyResult<Vec<_>>>()?;
    input.with_core(py, |input| {
        r.with_core(py, |r| {
            w.inner.with_mut(|w| {
                holder.with_mut(|h| {
                    effect.inner.with_mut(|e| {
                        core::coordinate_input(w, h, longitude, input, &events, r, e)
                            .map_err(errors::csv)
                    })
                })
            })
        })
    })
}
fn callable(py: Python<'_>, holder: &Option<Py<CallbackHolder>>) -> PyResult<Py<PyAny>> {
    CallbackHolder::link(
        holder
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("NLS callback was cleared"))?
            .bind(py),
    )
    .with_callable(py, |c| Ok(c.clone().unbind()))
}
impl Nls {
    fn with_core<R>(
        py: Python<'_>,
        value: &Py<Self>,
        errors: &RefCell<CallbackError>,
        f: impl FnOnce(&mut core::Nls<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let (map, upper) = {
            let value = value.try_borrow(py)?;
            (
                value.map.as_ref().map(|v| v.clone_ref(py)),
                value.upper.as_ref().map(|v| v.clone_ref(py)),
            )
        };
        let map = map.ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("NLS map callback was cleared")
        })?;
        let upper = upper.ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("NLS upper callback was cleared")
        })?;
        let (map, upper) = (
            CallbackHolder::link(map.bind(py)),
            CallbackHolder::link(upper.bind(py)),
        );
        let mut map = |source: &[u8], destination: &mut [u8]| {
            let result = map.with_callable(py, |c| {
                crate::csv::nls::map_call(py, c, source, destination)
            });
            errors.borrow_mut().capture(result).map_err(|_| {
                cs705_core::csv::Error::Operation("repeater editor NLS map callback failed".into())
            })
        };
        let mut upper = |destination: &mut [u8]| {
            let result =
                upper.with_callable(py, |c| crate::csv::nls::upper_call(py, c, destination));
            errors.borrow_mut().capture(result).map_err(|_| {
                cs705_core::csv::Error::Operation(
                    "repeater editor NLS upper callback failed".into(),
                )
            })
        };
        f(&mut core::Nls {
            map: &mut map,
            upper: &mut upper,
        })
    }
}
#[pyfunction]
fn begin_type(
    py: Python<'_>,
    w: &Working,
    holder: &DecodedRecord,
    group: &Bound<'_, PyAny>,
    relative: &Bound<'_, PyAny>,
    r: &Runtime,
    effect: &Effect,
) -> PyResult<()> {
    let (g, i) = (strict_usize(group)?, strict_usize(relative)?);
    r.with_core(py, |r| {
        w.inner.with_mut(|w| {
            holder.with_mut(|h| {
                effect
                    .inner
                    .with_mut(|e| core::begin_type(w, h, g, i, r, e).map_err(errors::csv))
            })
        })
    })
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "editor")?;
    m.add_class::<Runtime>()?;
    m.add_class::<Effect>()?;
    m.add_class::<Nls>()?;
    super::fields::add_enum(
        &m,
        "ChoiceField",
        &["Type", "Mode", "Duplex", "Tone", "Use", "Position"],
    )?;
    super::fields::add_enum(&m, "TextField", &["Name", "SubName", "Call", "Gateway"])?;
    super::fields::add_enum(
        &m,
        "NumberField",
        &["Frequency", "Offset", "ToneFrequency", "UtcOffset"],
    )?;
    m.add(
        "TYPE_SOURCE",
        super::fields::TypeSource {
            inner: core::TYPE_SOURCE,
        },
    )?;
    m.add_function(wrap_pyfunction!(begin_type, &m)?)?;
    m.add_function(wrap_pyfunction!(group_name, &m)?)?;
    m.add_function(wrap_pyfunction!(choice, &m)?)?;
    m.add_function(wrap_pyfunction!(number, &m)?)?;
    m.add_function(wrap_pyfunction!(text, &m)?)?;
    m.add_function(wrap_pyfunction!(coordinate, &m)?)?;
    m.add_function(wrap_pyfunction!(coordinate_input, &m)?)?;
    root.add_submodule(&m)
}
