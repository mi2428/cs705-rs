//! Information controllers retain native state and explicit source observations.
use super::search::{Candidate, Filter, Observation};
use super::session::{ErrorData, Outcome, SessionConfig};
use super::*;
use cs705_core::clone::information as native;
use pyo3::exceptions::{PyReferenceError, PyValueError};

fn life_in(value: &Bound<'_, PyAny>) -> PyResult<native::Life> {
    Ok(match enum_in(value, "cs705.clone.information", "Life")? {
        0 => native::Life::Absent,
        1 => native::Life::Open,
        2 => native::Life::ReleaseRequested,
        3 => native::Life::Destroyed,
        _ => return Err(PyValueError::new_err("invalid Life")),
    })
}
fn captions(value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<[Vec<u8>; 2]>> {
    value
        .map(|v| {
            let values = v
                .try_iter()?
                .map(|v| Ok(strict_bytes(&v?)?.to_vec()))
                .collect::<PyResult<Vec<_>>>()?;
            values
                .try_into()
                .map_err(|_| PyValueError::new_err("expected two caption byte strings"))
        })
        .transpose()
}
fn bools(value: &Bound<'_, PyAny>) -> PyResult<Vec<bool>> {
    value.try_iter()?.map(|v| strict_bool(&v?)).collect()
}

#[pyclass(module = "cs705.clone.information", frozen)]
#[derive(Clone)]
pub struct Event {
    inner: Handle<native::Event>,
}
#[pymethods]
impl Event {
    fn __copy__(&self) -> PyResult<Self> {
        self.inner.with(|v| {
            Ok(Self {
                inner: Handle::new(v.clone()),
            })
        })
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.__copy__()
    }
    #[new]
    fn new(name: String, argument: &Bound<'_, PyAny>, bytes: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: Handle::new(native::Event {
                name: name.into(),
                argument: strict_i64(argument)?,
                bytes: strict_bytes(bytes)?.to_vec(),
            }),
        })
    }
    #[getter]
    fn name(&self) -> PyResult<String> {
        self.inner.with(|v| Ok(v.name.to_string()))
    }
    #[setter]
    fn set_name(&self, value: String) -> PyResult<()> {
        self.inner.with(|v| {
            v.name = value.into();
            Ok(())
        })
    }
    #[getter]
    fn argument(&self) -> PyResult<i64> {
        self.inner.with(|v| Ok(v.argument))
    }
    #[setter]
    fn set_argument(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_i64(value)?;
        self.inner.with(|v| {
            v.argument = n;
            Ok(())
        })
    }
    #[getter]
    fn bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.inner.with(|v| Ok(PyBytes::new(py, &v.bytes)))
    }
    #[setter]
    fn set_bytes(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_bytes(value)?.to_vec();
        self.inner.with(|v| {
            v.bytes = n;
            Ok(())
        })
    }
    fn __eq__(&self, other: &Self) -> PyResult<bool> {
        let a = self.inner.with(|v| Ok(v.clone()))?;
        other.inner.with(|v| Ok(a == *v))
    }
}
#[pyclass(module = "cs705.clone.information", frozen)]
#[derive(Clone)]
pub struct Cell {
    inner: Handle<native::Cell>,
}
macro_rules! cell_impl {
    ($($field:ident,$set:ident:$ty:ty=>$conv:ident),* $(,)?)=>{
        #[pymethods] impl Cell {
            fn __copy__(&self)->PyResult<Self>{self.inner.with(|v|Ok(Self{inner:Handle::new(v.clone())}))}
            fn __deepcopy__(&self,_memo:&Bound<'_,PyAny>)->PyResult<Self>{self.__copy__()}
            #[new] fn new($($field:&Bound<'_,PyAny>,)*text:&Bound<'_,PyAny>)->PyResult<Self>{Ok(Self{inner:Handle::new(native::Cell{$($field:$conv($field)?,)*text:strict_bytes(text)?.to_vec()})})}
            $(#[getter] fn $field(&self)->PyResult<$ty>{self.inner.with(|v|Ok(v.$field))}
              #[setter] fn $set(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let n=$conv(value)?;self.inner.with(|v|{v.$field=n;Ok(())})})*
            #[getter] fn text<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyBytes>>{self.inner.with(|v|Ok(PyBytes::new(py,&v.text)))}
            #[setter] fn set_text(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_bytes(value)?.to_vec();self.inner.with(|v|{v.text=n;Ok(())})}
            fn __richcmp__(&self,other:&Self,op:pyo3::basic::CompareOp,py:Python<'_>)->PyResult<Py<PyAny>>{
                let a=self.inner.with(|v|Ok(v.clone()))?;
                let equal=other.inner.with(|v|Ok(a==*v))?;
                Ok(match op{
                    pyo3::basic::CompareOp::Eq=>pyo3::types::PyBool::new(py,equal).to_owned().into_any().unbind(),
                    pyo3::basic::CompareOp::Ne=>pyo3::types::PyBool::new(py,!equal).to_owned().into_any().unbind(),
                    _=>py.NotImplemented(),
                })
            }
        }
    }
}
cell_impl!(kind,set_kind:u8=>strict_u8,key,set_key:u16=>strict_u16,span,set_span:bool=>strict_bool,selectable,set_selectable:bool=>strict_bool);
#[pyclass(module = "cs705.clone.information", frozen)]
#[derive(Clone, Copy)]
pub struct NativeReturn {
    pub(crate) inner: native::NativeReturn,
}
#[pymethods]
#[allow(non_snake_case)]
impl NativeReturn {
    fn __copy__(&self) -> Self {
        *self
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> Self {
        *self
    }
    #[staticmethod]
    fn Returned(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: native::NativeReturn::Returned(strict_i64(value)?),
        })
    }
    #[staticmethod]
    fn Exception() -> Self {
        Self {
            inner: native::NativeReturn::Exception,
        }
    }
    /// Return a replacement Returned value; this value is not mutated.
    fn with_value(&self, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if !matches!(self.inner, native::NativeReturn::Returned(_)) {
            return Err(PyValueError::new_err(
                "with_value requires NativeReturn.Returned",
            ));
        }
        Self::Returned(value)
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self.inner {
            native::NativeReturn::Returned(_) => "Returned",
            native::NativeReturn::Exception => "Exception",
        }
    }
    #[getter]
    fn value(&self) -> Option<i64> {
        match self.inner {
            native::NativeReturn::Returned(v) => Some(v),
            _ => None,
        }
    }
}
#[pyclass(module = "cs705.clone.information", frozen)]
#[derive(Clone)]
pub struct ReadMode {
    inner: Handle<native::ReadMode>,
}
#[pymethods]
#[allow(non_snake_case)]
impl ReadMode {
    fn __copy__(&self) -> PyResult<Self> {
        self.inner.with(|v| {
            Ok(Self {
                inner: Handle::new(v.clone()),
            })
        })
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.__copy__()
    }
    #[staticmethod]
    fn Current() -> Self {
        Self {
            inner: Handle::new(native::ReadMode::Current),
        }
    }
    #[staticmethod]
    fn Captured(values: &Bound<'_, PyAny>) -> PyResult<Self> {
        let values = values
            .try_iter()?
            .map(|v| {
                let v = v?;
                if let Ok(error) = v.cast::<ErrorData>() {
                    Ok(Err(error.get().inner.clone()))
                } else {
                    Ok(Ok(strict_i32(&v)?))
                }
            })
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            inner: Handle::new(native::ReadMode::Captured(values)),
        })
    }
    /// Return a replacement Captured value; this value is not mutated.
    fn with_values(&self, values: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.inner.with(|value| {
            if !matches!(value, native::ReadMode::Captured(_)) {
                return Err(PyValueError::new_err(
                    "with_values requires ReadMode.Captured",
                ));
            }
            Ok(())
        })?;
        Self::Captured(values)
    }
    #[getter]
    fn kind(&self) -> PyResult<&'static str> {
        self.inner.with(|v| {
            Ok(match v {
                native::ReadMode::Current => "Current",
                native::ReadMode::Captured(_) => "Captured",
            })
        })
    }
    #[getter]
    fn values(&self, py: Python<'_>) -> PyResult<Option<Vec<Py<PyAny>>>> {
        self.inner.with(|v| match v {
            native::ReadMode::Current => Ok(None),
            native::ReadMode::Captured(v) => Ok(Some(
                v.iter()
                    .map(|v| match v {
                        Ok(v) => Ok(v.into_pyobject(py)?.into_any().unbind()),
                        Err(e) => Ok(Py::new(py, ErrorData { inner: e.clone() })?.into_any()),
                    })
                    .collect::<PyResult<Vec<_>>>()?,
            )),
        })
    }
}

#[derive(Clone)]
struct SearchData {
    ready: u8,
    opened: Option<bool>,
    filters: Vec<cs705_core::clone::search::Filter>,
    observations: Vec<cs705_core::clone::search::Observation>,
    prefix: Vec<u8>,
    suffixes: Vec<Vec<u8>>,
}
impl SearchData {
    fn native(&self) -> native::SearchSource<'_> {
        native::SearchSource {
            ready: self.ready,
            opened: self.opened,
            filters: &self.filters,
            observations: &self.observations,
            prefix: &self.prefix,
            suffixes: &self.suffixes,
        }
    }
}
#[pyclass(module = "cs705.clone.information", frozen)]
#[derive(Clone)]
pub struct SearchSource {
    inner: Handle<SearchData>,
}
#[pymethods]
impl SearchSource {
    #[new]
    #[pyo3(signature=(ready,opened,filters,observations,prefix,suffixes))]
    fn new(
        py: Python<'_>,
        ready: &Bound<'_, PyAny>,
        opened: Option<&Bound<'_, PyAny>>,
        filters: Vec<Py<Filter>>,
        observations: Vec<Py<Observation>>,
        prefix: &Bound<'_, PyAny>,
        suffixes: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let suffixes = suffixes
            .try_iter()?
            .map(|v| Ok(strict_bytes(&v?)?.to_vec()))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            inner: Handle::new(SearchData {
                ready: strict_u8(ready)?,
                opened: opened.map(strict_bool).transpose()?,
                filters: super::search::filters(py, filters)?,
                observations: super::search::observations(observations)?,
                prefix: strict_bytes(prefix)?.to_vec(),
                suffixes,
            }),
        })
    }
    #[getter]
    fn ready(&self) -> PyResult<u8> {
        self.inner.with(|v| Ok(v.ready))
    }
    #[setter]
    fn set_ready(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_u8(v)?;
        self.inner.with(|v| {
            v.ready = n;
            Ok(())
        })
    }
    #[getter]
    fn opened(&self) -> PyResult<Option<bool>> {
        self.inner.with(|v| Ok(v.opened))
    }
    #[setter]
    fn set_opened(&self, v: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let n = v.map(strict_bool).transpose()?;
        self.inner.with(|v| {
            v.opened = n;
            Ok(())
        })
    }
    #[getter]
    fn filters(&self) -> PyResult<Vec<Filter>> {
        self.inner.with(|v| {
            Ok((0..v.filters.len())
                .map(|index| Filter {
                    inner: self.inner.project(move |v| {
                        v.filters
                            .get_mut(index)
                            .ok_or_else(|| PyReferenceError::new_err("filter was removed"))
                    }),
                })
                .collect())
        })
    }
    #[setter]
    fn set_filters(&self, py: Python<'_>, values: Vec<Py<Filter>>) -> PyResult<()> {
        let n = super::search::filters(py, values)?;
        self.inner.edit(|v| {
            v.filters = n;
            Ok(())
        })
    }
    #[getter]
    fn observations(&self) -> PyResult<Vec<Observation>> {
        self.inner.with(|v| {
            Ok((0..v.observations.len())
                .map(|index| Observation {
                    inner: self.inner.project(move |v| {
                        v.observations
                            .get_mut(index)
                            .ok_or_else(|| PyReferenceError::new_err("observation was removed"))
                    }),
                })
                .collect())
        })
    }
    #[setter]
    fn set_observations(&self, values: Vec<Py<Observation>>) -> PyResult<()> {
        let n = super::search::observations(values)?;
        self.inner.edit(|v| {
            v.observations = n;
            Ok(())
        })
    }
    #[getter]
    fn prefix<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.inner.with(|v| Ok(PyBytes::new(py, &v.prefix)))
    }
    #[setter]
    fn set_prefix(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_bytes(v)?.to_vec();
        self.inner.with(|v| {
            v.prefix = n;
            Ok(())
        })
    }
    #[getter]
    fn suffixes<'py>(&self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyBytes>>> {
        self.inner
            .with(|v| Ok(v.suffixes.iter().map(|v| PyBytes::new(py, v)).collect()))
    }
    #[setter]
    fn set_suffixes(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = v
            .try_iter()?
            .map(|v| Ok(strict_bytes(&v?)?.to_vec()))
            .collect::<PyResult<Vec<_>>>()?;
        self.inner.edit(|v| {
            v.suffixes = n;
            Ok(())
        })
    }
}
#[derive(Clone)]
struct RegistryData {
    values: Vec<Result<Vec<u8>, core::Error>>,
    crt_point: Option<u8>,
}
impl RegistryData {
    fn native(&self) -> native::RegistrySource<'_> {
        native::RegistrySource {
            values: &self.values,
            crt_point: self.crt_point,
        }
    }
}
fn registry_values(values: &Bound<'_, PyAny>) -> PyResult<Vec<Result<Vec<u8>, core::Error>>> {
    values
        .try_iter()?
        .map(|v| {
            let v = v?;
            if let Ok(error) = v.cast::<ErrorData>() {
                Ok(Err(error.get().inner.clone()))
            } else {
                Ok(Ok(strict_bytes(&v)?.to_vec()))
            }
        })
        .collect()
}
#[pyclass(module = "cs705.clone.information", frozen)]
#[derive(Clone)]
pub struct RegistrySource {
    inner: Handle<RegistryData>,
}
#[pymethods]
impl RegistrySource {
    #[new]
    #[pyo3(signature=(values,crt_point))]
    fn new(values: &Bound<'_, PyAny>, crt_point: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self {
            inner: Handle::new(RegistryData {
                values: registry_values(values)?,
                crt_point: crt_point.map(strict_u8).transpose()?,
            }),
        })
    }
    #[getter]
    fn values(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner.with(|v| {
            v.values
                .iter()
                .map(|v| match v {
                    Ok(v) => Ok(PyBytes::new(py, v).into_any().unbind()),
                    Err(e) => Ok(Py::new(py, ErrorData { inner: e.clone() })?.into_any()),
                })
                .collect()
        })
    }
    #[setter]
    fn set_values(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = registry_values(v)?;
        self.inner.edit(|v| {
            v.values = n;
            Ok(())
        })
    }
    #[getter]
    fn crt_point(&self) -> PyResult<Option<u8>> {
        self.inner.with(|v| Ok(v.crt_point))
    }
    #[setter]
    fn set_crt_point(&self, v: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let n = v.map(strict_u8).transpose()?;
        self.inner.with(|v| {
            v.crt_point = n;
            Ok(())
        })
    }
}
#[pyclass(module = "cs705.clone.information", frozen)]
pub struct InformationSource {
    inner: Handle<native::InformationSource>,
}
#[pymethods]
impl InformationSource {
    #[new]
    #[pyo3(signature=(gate_context,gate_flag,display_context,display_flag,operation,captions))]
    fn new(
        gate_context: Option<&ContextFields>,
        gate_flag: &Bound<'_, PyAny>,
        display_context: Option<&ContextFields>,
        display_flag: &Bound<'_, PyAny>,
        operation: &Bound<'_, PyAny>,
        captions: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Handle::new(native::InformationSource {
                gate_context: gate_context.map(|v| v.inner.with(|v| Ok(*v))).transpose()?,
                gate_flag: strict_u8(gate_flag)?,
                display_context: display_context
                    .map(|v| v.inner.with(|v| Ok(*v)))
                    .transpose()?,
                display_flag: strict_u8(display_flag)?,
                operation: strict_u32(operation)?,
                captions: self::captions(captions)?,
            }),
        })
    }
    #[getter]
    fn gate_context(&self) -> PyResult<Option<ContextFields>> {
        self.inner.with(|v| {
            Ok(v.gate_context.map(|_| ContextFields {
                inner: self.inner.project(|v| {
                    v.gate_context
                        .as_mut()
                        .ok_or_else(|| PyReferenceError::new_err("gate context was removed"))
                }),
            }))
        })
    }
    #[setter]
    fn set_gate_context(&self, v: Option<&ContextFields>) -> PyResult<()> {
        let n = v.map(|v| v.inner.with(|v| Ok(*v))).transpose()?;
        self.inner.edit(|v| {
            v.gate_context = n;
            Ok(())
        })
    }
    #[getter]
    fn display_context(&self) -> PyResult<Option<ContextFields>> {
        self.inner.with(|v| {
            Ok(v.display_context.map(|_| ContextFields {
                inner: self.inner.project(|v| {
                    v.display_context
                        .as_mut()
                        .ok_or_else(|| PyReferenceError::new_err("display context was removed"))
                }),
            }))
        })
    }
    #[setter]
    fn set_display_context(&self, v: Option<&ContextFields>) -> PyResult<()> {
        let n = v.map(|v| v.inner.with(|v| Ok(*v))).transpose()?;
        self.inner.edit(|v| {
            v.display_context = n;
            Ok(())
        })
    }
    #[getter]
    fn gate_flag(&self) -> PyResult<u8> {
        self.inner.with(|v| Ok(v.gate_flag))
    }
    #[setter]
    fn set_gate_flag(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_u8(v)?;
        self.inner.with(|v| {
            v.gate_flag = n;
            Ok(())
        })
    }
    #[getter]
    fn display_flag(&self) -> PyResult<u8> {
        self.inner.with(|v| Ok(v.display_flag))
    }
    #[setter]
    fn set_display_flag(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_u8(v)?;
        self.inner.with(|v| {
            v.display_flag = n;
            Ok(())
        })
    }
    #[getter]
    fn operation(&self) -> PyResult<u32> {
        self.inner.with(|v| Ok(v.operation))
    }
    #[setter]
    fn set_operation(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_u32(v)?;
        self.inner.with(|v| {
            v.operation = n;
            Ok(())
        })
    }
    #[getter]
    fn captions<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Option<(Bound<'py, PyBytes>, Bound<'py, PyBytes>)>> {
        self.inner.with(|v| {
            Ok(v.captions
                .as_ref()
                .map(|v| (PyBytes::new(py, &v[0]), PyBytes::new(py, &v[1]))))
        })
    }
    #[setter]
    fn set_captions(&self, v: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let n = captions(v)?;
        self.inner.with(|v| {
            v.captions = n;
            Ok(())
        })
    }
}

#[pyclass(module = "cs705.clone.information", frozen)]
pub struct Controller {
    pub(crate) inner: Handle<native::Controller>,
}
macro_rules! controller_impl{
    ({$($scalar:ident,$set_scalar:ident:$ty:ty=>$conv:ident),*},{$($life:ident,$set_life:ident),*},{$($optional:ident,$set_optional:ident:$optional_ty:ty=>$optional_conv:ident),*})=>{
        #[pymethods]impl Controller{
            fn __copy__(&self)->PyResult<Self>{self.inner.with(|v|Ok(Self{inner:Handle::new(v.clone())}))}
            fn __deepcopy__(&self,_memo:&Bound<'_,PyAny>)->PyResult<Self>{self.__copy__()}
            #[new]fn new(com:&Bound<'_,PyAny>,resource_text:&Bound<'_,PyAny>,document:&Bound<'_,PyAny>)->PyResult<Self>{Ok(Self{inner:Handle::new(native::Controller::new(strict_i32(com)?,strict_bytes(resource_text)?.to_vec(),fixed_bytes(document)?))})}
            $(#[getter]fn $scalar(&self)->PyResult<$ty>{self.inner.with(|v|Ok(v.$scalar))}
              #[setter]fn $set_scalar(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=$conv(v)?;self.inner.with(|v|{v.$scalar=n;Ok(())})})*
            $(#[getter]fn $life<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{self.inner.with(|v|enum_out(py,"cs705.clone.information","Life",v.$life as usize))}
              #[setter]fn $set_life(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=life_in(v)?;self.inner.with(|v|{v.$life=n;Ok(())})})*
            $(#[getter]fn $optional(&self)->PyResult<Option<$optional_ty>>{self.inner.with(|v|Ok(v.$optional))}
              #[setter]fn $set_optional(&self,v:Option<&Bound<'_,PyAny>>)->PyResult<()>{let n=v.map($optional_conv).transpose()?;self.inner.with(|v|{v.$optional=n;Ok(())})})*
            #[getter]fn text<'py>(&self,py:Python<'py>)->PyResult<Option<Bound<'py,PyBytes>>>{self.inner.with(|v|Ok(v.text.as_ref().map(|v|PyBytes::new(py,v))))}
            #[setter]fn set_text(&self,v:Option<&Bound<'_,PyAny>>)->PyResult<()>{let n=v.map(strict_bytes).transpose()?.map(<[u8]>::to_vec);self.inner.with(|v|{v.text=n;Ok(())})}
            #[getter]fn identity(&self)->IdentityGate{IdentityGate{inner:self.inner.project(|v|Ok(&mut v.identity))}}
            #[setter]fn set_identity(&self,v:&IdentityGate)->PyResult<()>{let n=v.inner.with(|v|Ok(v.clone()))?;self.inner.edit(|v|{v.identity=n;Ok(())})}
            #[getter]fn captions<'py>(&self,py:Python<'py>)->PyResult<Option<(Bound<'py,PyBytes>,Bound<'py,PyBytes>)>>{self.inner.with(|v|Ok(v.captions.as_ref().map(|v|(PyBytes::new(py,&v[0]),PyBytes::new(py,&v[1])))))}
            #[setter]fn set_captions(&self,v:Option<&Bound<'_,PyAny>>)->PyResult<()>{let n=captions(v)?;self.inner.with(|v|{v.captions=n;Ok(())})}
            #[getter]fn dropdown(&self)->PyResult<Vec<i32>>{self.inner.with(|v|Ok(v.dropdown.clone()))}
            #[setter]fn set_dropdown(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=v.try_iter()?.map(|v|strict_i32(&v?)).collect::<PyResult<Vec<_>>>()?;self.inner.edit(|v|{v.dropdown=n;Ok(())})}
            #[getter]fn retired_popups<'py>(&self,py:Python<'py>)->PyResult<Vec<Bound<'py,PyAny>>>{self.inner.with(|v|v.retired_popups.iter().map(|v|enum_out(py,"cs705.clone.information","Life",*v as usize)).collect())}
            #[setter]fn set_retired_popups(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=v.try_iter()?.map(|v|life_in(&v?)).collect::<PyResult<Vec<_>>>()?;self.inner.edit(|v|{v.retired_popups=n;Ok(())})}
            #[getter]fn candidates(&self)->PyResult<Vec<Candidate>>{self.inner.with(|v|Ok((0..v.candidates.len()).map(|index|Candidate{inner:self.inner.project(move|v|v.candidates.get_mut(index).ok_or_else(||PyReferenceError::new_err("candidate was removed")))}).collect()))}
            #[setter]fn set_candidates(&self,values:Vec<Py<Candidate>>)->PyResult<()>{let n=values.into_iter().map(|v|v.get().inner.with(|v|Ok(v.clone()))).collect::<PyResult<Vec<_>>>()?;self.inner.edit(|v|{v.candidates=n;Ok(())})}
            #[getter]fn events(&self)->PyResult<Vec<Event>>{self.inner.with(|v|Ok((0..v.events.len()).map(|index|Event{inner:self.inner.project(move|v|v.events.get_mut(index).ok_or_else(||PyReferenceError::new_err("event was removed")))}).collect()))}
            #[setter]fn set_events(&self,values:Vec<Py<Event>>)->PyResult<()>{let n=values.into_iter().map(|v|v.get().inner.with(|v|Ok(v.clone()))).collect::<PyResult<Vec<_>>>()?;self.inner.edit(|v|{v.events=n;Ok(())})}
            fn choose(&self,index:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_i32(index)?;self.inner.edit(|v|v.choose(n).map_err(errors::clone))}
            fn input_text(&self,text:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_bytes(text)?.to_vec();self.inner.edit(|v|v.input_text(&n).map_err(errors::clone))}
            fn refresh(&self,source:&RegistrySource)->PyResult<()>{let n=source.inner.with(|v|Ok(v.clone()))?;self.inner.edit(|v|v.refresh(&n.native()).map_err(errors::clone))}
            fn construct_com(&self,source:&RegistrySource,reads:&ReadMode)->PyResult<()>{let(n,r)=(source.inner.with(|v|Ok(v.clone()))?,reads.inner.with(|v|Ok(v.clone()))?);self.inner.edit(|v|v.construct_com(&n.native(),&r).map_err(errors::clone))}
            fn rescan(&self,source:&SearchSource)->PyResult<()>{let n=source.inner.with(|v|Ok(v.clone()))?;self.inner.edit(|v|v.rescan(&n.native()).map_err(errors::clone))}
            fn startup(&self,source:Option<&SearchSource>)->PyResult<()>{let n=source.map(|v|v.inner.with(|v|Ok(v.clone()))).transpose()?;self.inner.edit(|v|v.startup(n.as_ref().map(|v|v.native()).as_ref()).map_err(errors::clone))}
            fn open_search(&self,source:&SearchSource)->PyResult<()>{let n=source.inner.with(|v|Ok(v.clone()))?;self.inner.edit(|v|v.open_search(&n.native()).map_err(errors::clone))}
            fn select(&self,reads:&ReadMode)->PyResult<()>{let n=reads.inner.with(|v|Ok(v.clone()))?;self.inner.edit(|v|v.select(&n).map_err(errors::clone))}
            fn popup_close(&self)->PyResult<()>{self.inner.edit(|v|v.popup_close().map_err(errors::clone))}
            fn escape(&self)->PyResult<()>{self.inner.edit(|v|v.escape().map_err(errors::clone))}
            fn return_search(&self,registry:&RegistrySource)->PyResult<()>{let n=registry.inner.with(|v|Ok(v.clone()))?;self.inner.edit(|v|v.return_search(&n.native()).map_err(errors::clone))}
            fn manual_commit_current(&self,busy_reads:&Bound<'_,PyAny>)->PyResult<()>{let n=bools(busy_reads)?;self.inner.edit(|v|v.manual_commit_current(&n).map_err(errors::clone))}
            fn manual_commit(&self,getter_text:&Bound<'_,PyAny>,busy_reads:&Bound<'_,PyAny>)->PyResult<()>{let(t,n)=(strict_bytes(getter_text)?.to_vec(),bools(busy_reads)?);self.inner.edit(|v|v.manual_commit(&t,&n).map_err(errors::clone))}
            fn close_com(&self,cancel:&Bound<'_,PyAny>,returns:Vec<Py<NativeReturn>>)->PyResult<()>{let(c,n)=(strict_bool(cancel)?,returns.into_iter().map(|v|v.get().inner).collect::<Vec<_>>());self.inner.edit(|v|v.close_com(c,&n).map_err(errors::clone))}
            fn identify(&self,identity:&Identity,source:&InformationSource)->PyResult<()>{let id=identity.inner.with(|v|Ok(v.clone()))?;let s=source.inner.with(|v|Ok(native::InformationSource{gate_context:v.gate_context,gate_flag:v.gate_flag,display_context:v.display_context,display_flag:v.display_flag,operation:v.operation,captions:v.captions.clone()}))?;self.inner.edit(|v|v.identify(&id,s).map_err(errors::clone))}
            fn query_requested(&self,py:Python<'_>,config:&SessionConfig,transport:Py<PyAny>,cancelled_callback:Py<PyAny>)->PyResult<Py<Outcome>>{
                if !cancelled_callback.bind(py).is_callable(){return Err(pyo3::exceptions::PyTypeError::new_err("cancelled must be callable"));}
                let config=config.inner.with(|v|Ok(v.clone()))?;let interpreter=crate::scoped::interpreter(py)?;
                let(native,failures)=self.inner.edit(|v|Ok(py.detach(move||{let failures=std::cell::RefCell::new(crate::callbacks::CallbackError::default());let mut adapter=super::session::Adapter{object:&transport,failures:&failures,interpreter};let result=v.query_requested(config,&mut adapter,||super::session::cancelled(&cancelled_callback,&failures,interpreter));(result,failures.into_inner())})))?;
                match native{Ok(native)=>{let outcome=Py::new(py,Outcome::from_core(native))?;failures.finish_with_outcome(py,Ok(outcome.clone_ref(py)),outcome.into_any())},Err(error)=>failures.finish(py,Err(errors::clone(error)))}
            }
            fn information_rows(&self)->PyResult<Option<Vec<Vec<Cell>>>>{self.inner.with(|v|Ok(v.information_rows().map_err(errors::clone)?.map(|rows|rows.into_iter().map(|row|row.into_iter().map(|v|Cell{inner:Handle::new(v)}).collect()).collect())))}
            fn information_selection(&self,unit_locked:&Bound<'_,PyAny>,live_flags:&Bound<'_,PyAny>,previous:&Bound<'_,PyAny>)->PyResult<bool>{let(u,l,p)=(strict_u32(unit_locked)?,strict_u8(live_flags)?,strict_bool(previous)?);self.inner.with(|v|Ok(v.information_selection(u,l,p)))}
            fn information_close(&self,click:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_bool(click)?;self.inner.edit(|v|v.information_close(n).map_err(errors::clone))}
            fn destroy_retired_popup(&self,index:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_usize(index)?;self.inner.edit(|v|v.destroy_retired_popup(n).map_err(errors::clone))}
            fn destroy(&self,target:&str)->PyResult<()>{self.inner.edit(|v|v.destroy(target).map_err(errors::clone))}
            fn __richcmp__(&self,other:&Self,op:pyo3::basic::CompareOp,py:Python<'_>)->PyResult<Py<PyAny>>{let value=self.inner.with(|v|Ok(v.clone()))?;let equal=other.inner.with(|v|Ok(value==*v))?;Ok(match op{pyo3::basic::CompareOp::Eq=>pyo3::types::PyBool::new(py,equal).to_owned().into_any().unbind(),pyo3::basic::CompareOp::Ne=>pyo3::types::PyBool::new(py,!equal).to_owned().into_any().unbind(),_=>py.NotImplemented()})}
            #[pyo3(signature=(getter_text,busy_reads,receiver))]
            fn manual_commit_document(&self,py:Python<'_>,getter_text:Option<&Bound<'_,PyAny>>,busy_reads:&Bound<'_,PyAny>,receiver:Py<super::receiver::DocumentReceiver>)->PyResult<()>{let(t,b)=(getter_text.map(strict_bytes).transpose()?.map(<[u8]>::to_vec),bools(busy_reads)?);let(e,r)=self.inner.edit(|v|super::receiver::run(py,&receiver,move|r|v.manual_commit_document(t.as_deref(),&b,r).map_err(errors::clone)))?;e.finish(py,r)}
            fn request_reidentify_document(&self,py:Python<'_>,busy_reads:&Bound<'_,PyAny>,receiver:Py<super::receiver::DocumentReceiver>)->PyResult<()>{let b=bools(busy_reads)?;let(e,r)=self.inner.edit(|v|super::receiver::run(py,&receiver,move|r|v.request_reidentify_document(&b,r).map_err(errors::clone)))?;e.finish(py,r)}
            fn close_com_document(&self,py:Python<'_>,cancel:&Bound<'_,PyAny>,returns:Vec<Py<NativeReturn>>,receiver:Py<super::receiver::DocumentReceiver>)->PyResult<()>{let(c,n)=(strict_bool(cancel)?,returns.into_iter().map(|v|v.get().inner).collect::<Vec<_>>());let(e,r)=self.inner.edit(|v|super::receiver::run(py,&receiver,move|r|v.close_com_document(c,&n,r).map_err(errors::clone)))?;e.finish(py,r)}
            fn query_document(&self,py:Python<'_>,config:&SessionConfig,transport:Py<PyAny>,cancelled_callback:Py<PyAny>,receiver:Py<super::receiver::DocumentReceiver>)->PyResult<Py<Outcome>>{
                if !cancelled_callback.bind(py).is_callable(){return Err(pyo3::exceptions::PyTypeError::new_err("cancelled must be callable"));}
                let config=config.inner.with(|v|Ok(v.clone()))?;let interpreter=crate::scoped::interpreter(py)?;
                let(retained,result)=self.inner.edit(|v|super::receiver::run(py,&receiver,move|r|{let failures=std::cell::RefCell::new(crate::callbacks::CallbackError::default());let mut adapter=super::session::Adapter{object:&transport,failures:&failures,interpreter};let out=v.query_document(config,&mut adapter,||super::session::cancelled(&cancelled_callback,&failures,interpreter),r);Ok((out,failures.into_inner()))}))?;
                let(native,failures)=retained.finish(py,result)?;
                match native{Ok(native)=>{let outcome=Py::new(py,Outcome::from_core(native))?;failures.finish_with_outcome(py,Ok(outcome.clone_ref(py)),outcome.into_any())},Err(error)=>failures.finish(py,Err(super::receiver::error(error)))}
            }
            fn return_query(&self,py:Python<'_>,outcome:&Outcome,receiver:Py<super::receiver::DocumentReceiver>,source:&ReturnSource)->PyResult<()>{let source=source.inner.with(|v|Ok(v.clone()))?;let outcome=outcome.inner.with(|v|Ok(v.clone()))?;let(e,r)=self.inner.edit(|v|super::receiver::run(py,&receiver,move|r|v.return_query(&outcome,r,source.native()).map_err(super::receiver::error)))?;e.finish(py,r)}
            fn sync49_document(&self,py:Python<'_>,receiver:Py<super::receiver::DocumentReceiver>,close_returns:Vec<Py<NativeReturn>>)->PyResult<bool>{let n=close_returns.into_iter().map(|v|v.get().inner).collect::<Vec<_>>();let(e,r)=self.inner.edit(|v|super::receiver::run(py,&receiver,move|r|v.sync49_document(r,&n).map_err(errors::clone)))?;e.finish(py,r)}
            fn destroy_document(&self,py:Python<'_>,target:String,receiver:Py<super::receiver::DocumentReceiver>)->PyResult<()>{let(e,r)=self.inner.edit(|v|super::receiver::run(py,&receiver,move|r|v.destroy_document(&target,r).map_err(errors::clone)))?;e.finish(py,r)}
        }
    }
}
controller_impl!({com,set_com:i32=>strict_i32,pair_registered,set_pair_registered:bool=>strict_bool,selector,set_selector:u8=>strict_u8,fc,set_fc:u8=>strict_u8,fd,set_fd:u8=>strict_u8,generation,set_generation:u64=>strict_u64,previous,set_previous:u32=>strict_u32,search_result,set_search_result:i32=>strict_i32,reidentify_requested,set_reidentify_requested:bool=>strict_bool},{form,set_form,popup,set_popup,information,set_information},{com_mode,set_com_mode:u8=>strict_u8,identity_result,set_identity_result:u8=>strict_u8,selected,set_selected:usize=>strict_usize,last_item_index,set_last_item_index:i32=>strict_i32});

#[derive(Clone)]
struct ReturnData {
    defaults: Option<crate::icf::document::values::DefaultSource>,
    profile_return: Option<u32>,
    captions: Option<[Vec<u8>; 2]>,
    close_returns: Vec<native::NativeReturn>,
}
impl ReturnData {
    fn native(&self) -> native::ReturnSource<'_> {
        native::ReturnSource {
            defaults: self.defaults.as_ref().map(|v| v.as_core()),
            profile_return: self.profile_return,
            captions: self.captions.clone(),
            close_returns: &self.close_returns,
        }
    }
}
#[pyclass(module = "cs705.clone.information", frozen)]
pub struct ReturnSource {
    inner: Handle<ReturnData>,
}
#[pymethods]
impl ReturnSource {
    #[new]
    #[pyo3(signature=(defaults,profile_return,captions,close_returns))]
    fn new(
        defaults: Option<&crate::icf::document::values::DefaultSource>,
        profile_return: Option<&Bound<'_, PyAny>>,
        captions: Option<&Bound<'_, PyAny>>,
        close_returns: Vec<Py<NativeReturn>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Handle::new(ReturnData {
                defaults: defaults.cloned(),
                profile_return: profile_return.map(strict_u32).transpose()?,
                captions: self::captions(captions)?,
                close_returns: close_returns.into_iter().map(|v| v.get().inner).collect(),
            }),
        })
    }
    #[getter]
    fn defaults(&self) -> PyResult<Option<crate::icf::document::values::DefaultSource>> {
        self.inner.with(|v| Ok(v.defaults.clone()))
    }
    #[setter]
    fn set_defaults(
        &self,
        v: Option<&crate::icf::document::values::DefaultSource>,
    ) -> PyResult<()> {
        let n = v.cloned();
        self.inner.with(|v| {
            v.defaults = n;
            Ok(())
        })
    }
    #[getter]
    fn profile_return(&self) -> PyResult<Option<u32>> {
        self.inner.with(|v| Ok(v.profile_return))
    }
    #[setter]
    fn set_profile_return(&self, v: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let n = v.map(strict_u32).transpose()?;
        self.inner.with(|v| {
            v.profile_return = n;
            Ok(())
        })
    }
    #[getter]
    fn captions<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Option<(Bound<'py, PyBytes>, Bound<'py, PyBytes>)>> {
        self.inner.with(|v| {
            Ok(v.captions
                .as_ref()
                .map(|v| (PyBytes::new(py, &v[0]), PyBytes::new(py, &v[1]))))
        })
    }
    #[setter]
    fn set_captions(&self, v: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let n = captions(v)?;
        self.inner.with(|v| {
            v.captions = n;
            Ok(())
        })
    }
    #[getter]
    fn close_returns(&self) -> PyResult<Vec<NativeReturn>> {
        self.inner.with(|v| {
            Ok(v.close_returns
                .iter()
                .map(|v| NativeReturn { inner: *v })
                .collect())
        })
    }
    #[setter]
    fn set_close_returns(&self, v: Vec<Py<NativeReturn>>) -> PyResult<()> {
        let n = v.into_iter().map(|v| v.get().inner).collect();
        self.inner.with(|v| {
            v.close_returns = n;
            Ok(())
        })
    }
}

pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "information")?;
    add_enum(
        &m,
        "Life",
        &["Absent", "Open", "ReleaseRequested", "Destroyed"],
    )?;
    m.add_class::<Event>()?;
    m.add_class::<Cell>()?;
    m.add_class::<NativeReturn>()?;
    m.add_class::<ReadMode>()?;
    m.add_class::<SearchSource>()?;
    m.add_class::<RegistrySource>()?;
    m.add_class::<InformationSource>()?;
    m.add_class::<Controller>()?;
    m.add_class::<ReturnSource>()?;
    m.add_class::<super::receiver::DocumentReceiver>()?;
    root.add_submodule(&m)
}
