//! Headless help resolution and explicit OS-observation replay, never OS calls.
use crate::conversion::*;
use cs705_core::item_help as core;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

#[pyclass(module = "cs705.item_help", frozen)]
pub struct Input {
    item: Option<(i32, i32, Vec<u8>)>,
}
#[pymethods]
impl Input {
    #[getter]
    fn variant(&self) -> &'static str {
        if self.item.is_some() {
            "Item"
        } else {
            "Contents"
        }
    }
    #[getter]
    fn key(&self) -> Option<i32> {
        self.item.as_ref().map(|v| v.0)
    }
    #[getter]
    fn index(&self) -> Option<i32> {
        self.item.as_ref().map(|v| v.1)
    }
    #[getter]
    fn context58<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.item.as_ref().map(|v| PyBytes::new(py, &v.2))
    }
    #[staticmethod]
    #[allow(non_snake_case)]
    fn Contents() -> Self {
        Self { item: None }
    }
    #[staticmethod]
    #[allow(non_snake_case)]
    fn Item(
        key: &Bound<'_, PyAny>,
        index: &Bound<'_, PyAny>,
        context58: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            item: Some((
                strict_i32(key)?,
                strict_i32(index)?,
                strict_bytes(context58)?.to_vec(),
            )),
        })
    }
}
impl Input {
    fn native(&self) -> core::Input<'_> {
        match &self.item {
            None => core::Input::Contents,
            Some((key, index, context58)) => core::Input::Item {
                key: *key,
                index: *index,
                context58,
            },
        }
    }
}

#[pyclass(module = "cs705.item_help")]
pub struct Request {
    inner: core::Request,
}
impl Request {
    pub fn from_core(inner: core::Request) -> Self {
        Self { inner }
    }
}
macro_rules! request {
    ($($bytes:ident=>$set:ident),*) => { #[pymethods] impl Request {
        #[new] fn new(key:&Bound<'_,PyAny>,index:&Bound<'_,PyAny>,group:&Bound<'_,PyAny>,original_name:&Bound<'_,PyAny>,name:&Bound<'_,PyAny>,topic:&Bound<'_,PyAny>,chm:&Bound<'_,PyAny>,target:&Bound<'_,PyAny>)->PyResult<Self>{
            let optional=|v:&Bound<'_,PyAny>|->PyResult<Option<i32>>{if v.is_none(){Ok(None)}else{Ok(Some(strict_i32(v)?))}};
            Ok(Self {inner:core::Request{key:optional(key)?,index:optional(index)?,group:optional(group)?,original_name:strict_bytes(original_name)?.to_vec(),name:strict_bytes(name)?.to_vec(),topic:strict_i32(topic)?,chm:strict_bytes(chm)?.to_vec(),target:strict_bytes(target)?.to_vec()}})
        }
        #[getter] fn key(&self)->Option<i32>{self.inner.key}
        #[getter] fn index(&self)->Option<i32>{self.inner.index}
        #[getter] fn group(&self)->Option<i32>{self.inner.group}
        #[getter] fn topic(&self)->i32{self.inner.topic}
        #[setter] fn set_key(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{self.inner.key=if value.is_none(){None}else{Some(strict_i32(value)?)};Ok(())}
        #[setter] fn set_index(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{self.inner.index=if value.is_none(){None}else{Some(strict_i32(value)?)};Ok(())}
        #[setter] fn set_group(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{self.inner.group=if value.is_none(){None}else{Some(strict_i32(value)?)};Ok(())}
        #[setter] fn set_topic(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{self.inner.topic=strict_i32(value)?;Ok(())}
        $(#[getter] fn $bytes<'py>(&self,py:Python<'py>)->Bound<'py,PyBytes>{PyBytes::new(py,&self.inner.$bytes)}
        #[setter] fn $set(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{self.inner.$bytes=strict_bytes(value)?.to_vec();Ok(())})*
    }};
}
request!(original_name=>set_original_name,name=>set_name,chm=>set_chm,target=>set_target);
#[pyfunction]
fn resolve(
    pe: &Bound<'_, PyAny>,
    input: &Input,
    gui_language: &Bound<'_, PyAny>,
    application_dir: &Bound<'_, PyAny>,
) -> PyResult<Request> {
    Ok(Request {
        inner: core::resolve(
            strict_bytes(pe)?,
            input.native(),
            strict_u32(gui_language)?,
            strict_bytes(application_dir)?,
        )
        .map_err(crate::errors::csv)?,
    })
}

#[pyclass(module = "cs705.item_help")]
pub struct State {
    inner: core::State,
}
macro_rules! state {
    ($($field:ident=>$set:ident),*)=> { #[pymethods] impl State {
        #[new] fn new($($field:&Bound<'_,PyAny>),*)->PyResult<Self>{Ok(Self{inner:core::State{$($field:strict_u32($field)?),*}})}
        $(#[getter] fn $field(&self)->u32{self.inner.$field}
        #[setter] fn $set(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{self.inner.$field=strict_u32(value)?;Ok(())})*
    }};
}
state!(module=>set_module,procedure=>set_procedure,cookie=>set_cookie,d188=>set_d188,d184=>set_d184);

fn array<const N: usize>(value: &Bound<'_, PyAny>) -> PyResult<[i32; N]> {
    let tuple = value.cast::<PyTuple>()?;
    if tuple.len() != N {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "expected {N} integers"
        )));
    }
    let mut result = [0; N];
    for (index, item) in tuple.iter().enumerate() {
        result[index] = strict_i32(&item)?;
    }
    Ok(result)
}
#[pyclass(module = "cs705.item_help")]
pub struct Api {
    inner: core::Api,
}
macro_rules! api {
    ($($field:ident=>$set:ident),*; $($rect:ident=>$set_rect:ident:$n:literal),*)=> { #[pymethods] impl Api {
        #[new] fn new()->Self{Self{inner:core::Api::default()}}
        $(#[getter] fn $field(&self)->Option<u32>{self.inner.$field}
        #[setter] fn $set(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{self.inner.$field=if value.is_none(){None}else{Some(strict_u32(value)?)};Ok(())})*
        $(#[getter] fn $rect<'py>(&self,py:Python<'py>)->PyResult<Option<Bound<'py,PyTuple>>>{self.inner.$rect.map(|v|PyTuple::new(py,v)).transpose()}
        #[setter] fn $set_rect(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{self.inner.$rect=if value.is_none(){None}else{Some(array::<$n>(value)?)};Ok(())})*
    }};
}
api!(load_library=>set_load_library,get_proc_address=>set_get_proc_address,initialize_cookie=>set_initialize_cookie,window_before=>set_window_before,display_window=>set_display_window,file_attributes=>set_file_attributes,window_after=>set_window_after,window_info_pointer=>set_window_info_pointer,style=>set_style;old_rect=>set_old_rect:4,window_info_rect=>set_window_info_rect:4,cursor=>set_cursor:2,work_area=>set_work_area:4);

#[pyclass(module = "cs705.item_help")]
pub struct Call {
    inner: core::Call,
}
#[pymethods]
impl Call {
    #[new]
    fn new(api: &str, command: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Call {
                api: api.to_owned().into(),
                command: if command.is_none() {
                    None
                } else {
                    Some(strict_u32(command)?)
                },
            },
        })
    }
    #[getter]
    fn api(&self) -> &str {
        self.inner.api.as_ref()
    }
    #[setter]
    fn set_api(&mut self, value: &str) {
        self.inner.api = value.to_owned().into();
    }
    #[getter]
    fn command(&self) -> Option<u32> {
        self.inner.command
    }
    #[setter]
    fn set_command(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.command = if value.is_none() {
            None
        } else {
            Some(strict_u32(value)?)
        };
        Ok(())
    }
}
#[pyclass(module = "cs705.item_help")]
pub struct Placement {
    window: u32,
    x: i32,
    y: i32,
    style: u32,
    flags: u32,
}
macro_rules! placement {
    ($($field:ident:$ty:ty=>$set:ident,$strict:ident);* $(;)?)=>{
        #[pymethods] impl Placement {
            #[new] fn new($($field:&Bound<'_,PyAny>),*)->PyResult<Self>{Ok(Self{$($field:$strict($field)?),*})}
            $(#[getter] fn $field(&self)->$ty{self.$field}
            #[setter] fn $set(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{self.$field=$strict(value)?;Ok(())})*
        }
    }
}
placement!(window:u32=>set_window,strict_u32;x:i32=>set_x,strict_i32;y:i32=>set_y,strict_i32;style:u32=>set_style,strict_u32;flags:u32=>set_flags,strict_u32);
#[pyclass(module = "cs705.item_help")]
pub struct Effects {
    inner: core::Effects,
}
#[pymethods]
impl Effects {
    #[new]
    fn new() -> Self {
        Self {
            inner: core::Effects::default(),
        }
    }
    #[getter]
    fn calls(&self) -> Vec<Call> {
        self.inner
            .calls
            .iter()
            .map(|v| Call { inner: v.clone() })
            .collect()
    }
    #[setter]
    fn set_calls(&mut self, values: Vec<PyRef<'_, Call>>) {
        self.inner.calls = values.iter().map(|v| v.inner.clone()).collect();
    }
    #[getter]
    fn show_window(&self) -> Vec<(u32, i32)> {
        self.inner.show_window.clone()
    }
    #[setter]
    fn set_show_window(&mut self, values: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut out = Vec::new();
        for value in values.try_iter()? {
            let value = value?;
            let pair = value.cast::<PyTuple>()?;
            if pair.len() != 2 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "expected window and command",
                ));
            }
            out.push((
                strict_u32(&pair.get_item(0)?)?,
                strict_i32(&pair.get_item(1)?)?,
            ));
        }
        self.inner.show_window = out;
        Ok(())
    }
    #[getter]
    fn placement(&self) -> Option<Placement> {
        self.inner.placement.map(|v| Placement {
            window: v.window,
            x: v.x,
            y: v.y,
            style: v.style,
            flags: v.flags,
        })
    }
    #[setter]
    fn set_placement(&mut self, value: Option<&Placement>) {
        self.inner.placement = value.map(|v| core::Placement {
            window: v.window,
            x: v.x,
            y: v.y,
            style: v.style,
            flags: v.flags,
        });
    }
    #[getter]
    fn display_window_observed(&self) -> Option<u32> {
        self.inner.display_window_observed
    }
    #[setter]
    fn set_display_window_observed(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.display_window_observed = if v.is_none() {
            None
        } else {
            Some(strict_u32(v)?)
        };
        Ok(())
    }
    #[getter]
    fn message_resource_index(&self) -> Option<u32> {
        self.inner.message_resource_index
    }
    #[setter]
    fn set_message_resource_index(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.message_resource_index = if v.is_none() {
            None
        } else {
            Some(strict_u32(v)?)
        };
        Ok(())
    }
    #[getter]
    fn message_icon(&self) -> Option<u32> {
        self.inner.message_icon
    }
    #[setter]
    fn set_message_icon(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.message_icon = if v.is_none() {
            None
        } else {
            Some(strict_u32(v)?)
        };
        Ok(())
    }
}
#[pyfunction]
fn drive(
    request: &Request,
    mut state: PyRefMut<'_, State>,
    api: &Api,
    mut effects: PyRefMut<'_, Effects>,
) -> PyResult<u32> {
    let result = core::drive(
        &request.inner,
        &mut state.inner,
        &api.inner,
        &mut effects.inner,
    );
    drop(state);
    drop(effects);
    result.map_err(crate::errors::csv)
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "item_help")?;
    m.add_class::<Input>()?;
    m.add_class::<Request>()?;
    m.add_class::<State>()?;
    m.add_class::<Api>()?;
    m.add_class::<Call>()?;
    m.add_class::<Placement>()?;
    m.add_class::<Effects>()?;
    m.add_function(wrap_pyfunction!(resolve, &m)?)?;
    m.add_function(wrap_pyfunction!(drive, &m)?)?;
    root.add_submodule(&m)
}
