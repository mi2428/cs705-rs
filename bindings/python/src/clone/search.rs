//! Search logic consumes explicit observations; it never discovers a device.
use super::*;
use cs705_core::clone::search as native;
use pyo3::exceptions::PyReferenceError;

macro_rules! flat {
    ($name:ident, {$($byte:ident,$set_byte:ident),*}, {$($field:ident,$set:ident:$ty:ty=>$conv:ident),*}, {$($extra:item)*})=>{
        #[pyclass(module="cs705.clone.search",frozen)] #[derive(Clone)]
        pub struct $name { pub(crate) inner:Handle<native::$name> }
        #[pymethods] impl $name {
            fn __copy__(&self)->PyResult<Self>{self.inner.with(|v|Ok(Self{inner:Handle::new(v.clone())}))}
            fn __deepcopy__(&self,_memo:&Bound<'_,PyAny>)->PyResult<Self>{self.__copy__()}
            #[new] fn new($($byte:&Bound<'_,PyAny>,)*$($field:&Bound<'_,PyAny>),*)->PyResult<Self>{Ok(Self{inner:Handle::new(native::$name{$($byte:strict_bytes($byte)?.to_vec(),)*$($field:$conv($field)?),*})})}
            $(#[getter] fn $byte<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyBytes>>{self.inner.with(|v|Ok(PyBytes::new(py,&v.$byte)))}
              #[setter] fn $set_byte(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_bytes(v)?.to_vec();self.inner.with(|v|{v.$byte=n;Ok(())})})*
            $(#[getter] fn $field(&self)->PyResult<$ty>{self.inner.with(|v|Ok(v.$field))}
              #[setter] fn $set(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=$conv(v)?;self.inner.with(|v|{v.$field=n;Ok(())})})*
            fn __eq__(&self,other:&Self)->PyResult<bool>{let a=self.inner.with(|v|Ok(v.clone()))?;other.inner.with(|b|Ok(a==*b))}
            $($extra)*
        }
    }
}
flat!(Filter,{hub,set_hub,audio,set_audio,com,set_com,id,set_id,cid,set_cid},{mi,set_mi:i32=>strict_i32},{
    #[staticmethod] fn from_ini(hub:&Bound<'_,PyAny>,audio:&Bound<'_,PyAny>,com:&Bound<'_,PyAny>,id:&Bound<'_,PyAny>,cid:&Bound<'_,PyAny>)->PyResult<Self>{Ok(Self{inner:Handle::new(native::Filter::from_ini(strict_bytes(hub)?,strict_bytes(audio)?,strict_bytes(com)?,strict_bytes(id)?,strict_bytes(cid)?).map_err(errors::clone)?)})}
});
flat!(Port,{instance_id,set_instance_id,parent_id,set_parent_id,prefix,set_prefix,description,set_description},{mi,set_mi:i32=>strict_i32,number,set_number:u32=>strict_u32},{});
flat!(Slot,{name,set_name},{number,set_number:u32=>strict_u32},{});
flat!(Candidate,{label,set_label},{number,set_number:u32=>strict_u32},{});

pub(crate) fn filters(_py: Python<'_>, values: Vec<Py<Filter>>) -> PyResult<Vec<native::Filter>> {
    values
        .into_iter()
        .map(|v| v.get().inner.with(|v| Ok(v.clone())))
        .collect()
}
pub(crate) fn observations(values: Vec<Py<Observation>>) -> PyResult<Vec<native::Observation>> {
    values
        .into_iter()
        .map(|v| v.get().inner.with(|v| Ok(v.clone())))
        .collect()
}
fn bytes_vec(value: &Bound<'_, PyAny>) -> PyResult<Vec<Vec<u8>>> {
    value
        .try_iter()?
        .map(|v| Ok(strict_bytes(&v?)?.to_vec()))
        .collect()
}
fn missing() -> PyErr {
    PyReferenceError::new_err("search element was removed")
}

#[pyclass(module = "cs705.clone.search", frozen)]
#[derive(Clone)]
/// slots is a snapshot list of live elements; replace it to edit membership.
pub struct SearchRecord {
    pub(crate) inner: Handle<native::SearchRecord>,
}
#[pymethods]
impl SearchRecord {
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
    fn new(
        wave_in: &Bound<'_, PyAny>,
        wave_out: &Bound<'_, PyAny>,
        slots: Vec<Py<Slot>>,
    ) -> PyResult<Self> {
        let slots = slots
            .into_iter()
            .map(|v| v.get().inner.with(|v| Ok(v.clone())))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            inner: Handle::new(native::SearchRecord {
                wave_in: strict_i32(wave_in)?,
                wave_out: strict_i32(wave_out)?,
                slots,
            }),
        })
    }
    #[getter]
    fn wave_in(&self) -> PyResult<i32> {
        self.inner.with(|v| Ok(v.wave_in))
    }
    #[setter]
    fn set_wave_in(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_i32(v)?;
        self.inner.with(|v| {
            v.wave_in = n;
            Ok(())
        })
    }
    #[getter]
    fn wave_out(&self) -> PyResult<i32> {
        self.inner.with(|v| Ok(v.wave_out))
    }
    #[setter]
    fn set_wave_out(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_i32(v)?;
        self.inner.with(|v| {
            v.wave_out = n;
            Ok(())
        })
    }
    #[getter]
    fn slots(&self) -> PyResult<Vec<Slot>> {
        self.inner.with(|v| {
            Ok((0..v.slots.len())
                .map(|index| Slot {
                    inner: self
                        .inner
                        .project(move |v| v.slots.get_mut(index).ok_or_else(missing)),
                })
                .collect())
        })
    }
    #[setter]
    fn set_slots(&self, values: Vec<Py<Slot>>) -> PyResult<()> {
        let n = values
            .into_iter()
            .map(|v| v.get().inner.with(|v| Ok(v.clone())))
            .collect::<PyResult<Vec<_>>>()?;
        self.inner.edit(|v| {
            v.slots = n;
            Ok(())
        })
    }
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.inner
            .with(|v| Ok(PyBytes::new(py, &v.to_bytes().map_err(errors::clone)?)))
    }
    #[staticmethod]
    fn from_bytes(bytes: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: Handle::new(
                native::SearchRecord::from_bytes(strict_bytes(bytes)?).map_err(errors::clone)?,
            ),
        })
    }
    fn __eq__(&self, other: &Self) -> PyResult<bool> {
        let a = self.inner.with(|v| Ok(v.clone()))?;
        other.inner.with(|v| Ok(a == *v))
    }
}
#[pyclass(module = "cs705.clone.search", frozen)]
#[derive(Clone)]
/// Inputs are copied; ports reads a snapshot list of owner-backed elements.
pub struct Observation {
    pub(crate) inner: Handle<native::Observation>,
}
macro_rules! observation_impl {
    ($($byte:ident,$set:ident),* $(,)?)=>{
        #[pymethods] impl Observation {
            fn __copy__(&self)->PyResult<Self>{self.inner.with(|v|Ok(Self{inner:Handle::new(v.clone())}))}
            fn __deepcopy__(&self,_memo:&Bound<'_,PyAny>)->PyResult<Self>{self.__copy__()}
            #[expect(clippy::too_many_arguments, reason = "The Python constructor exposes all nine native Observation fields.")]
            #[new] fn new($($byte:&Bound<'_,PyAny>,)*parent_path:&Bound<'_,PyAny>,ports:Vec<Py<Port>>,wave_in:&Bound<'_,PyAny>,wave_out:&Bound<'_,PyAny>)->PyResult<Self>{let ports=ports.into_iter().map(|v|v.get().inner.with(|v|Ok(v.clone()))).collect::<PyResult<Vec<_>>>()?;Ok(Self{inner:Handle::new(native::Observation{$($byte:strict_bytes($byte)?.to_vec(),)*parent_path:strict_bool(parent_path)?,ports,wave_in:strict_i32(wave_in)?,wave_out:strict_i32(wave_out)?})})}
            $(#[getter] fn $byte<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyBytes>>{self.inner.with(|v|Ok(PyBytes::new(py,&v.$byte)))}
              #[setter] fn $set(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_bytes(v)?.to_vec();self.inner.with(|v|{v.$byte=n;Ok(())})})*
            #[getter] fn parent_path(&self)->PyResult<bool>{self.inner.with(|v|Ok(v.parent_path))}
            #[setter] fn set_parent_path(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_bool(v)?;self.inner.with(|v|{v.parent_path=n;Ok(())})}
            #[getter] fn wave_in(&self)->PyResult<i32>{self.inner.with(|v|Ok(v.wave_in))}
            #[setter] fn set_wave_in(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_i32(v)?;self.inner.with(|v|{v.wave_in=n;Ok(())})}
            #[getter] fn wave_out(&self)->PyResult<i32>{self.inner.with(|v|Ok(v.wave_out))}
            #[setter] fn set_wave_out(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_i32(v)?;self.inner.with(|v|{v.wave_out=n;Ok(())})}
            #[getter] fn ports(&self)->PyResult<Vec<Port>>{self.inner.with(|v|Ok((0..v.ports.len()).map(|index|Port{inner:self.inner.project(move|v|v.ports.get_mut(index).ok_or_else(missing))}).collect()))}
            #[setter] fn set_ports(&self,values:Vec<Py<Port>>)->PyResult<()>{let n=values.into_iter().map(|v|v.get().inner.with(|v|Ok(v.clone()))).collect::<PyResult<Vec<_>>>()?;self.inner.edit(|v|{v.ports=n;Ok(())})}
        }
    }
}
observation_impl!(
    key, set_key, base, set_base, com, set_com, hub, set_hub, audio, set_audio
);
#[pyclass(module = "cs705.clone.search", frozen)]
/// Result lists are snapshots of live records; rebuilding expires old elements.
pub struct SearchContext {
    inner: Handle<native::SearchContext>,
}
#[pymethods]
impl SearchContext {
    #[new]
    fn new() -> Self {
        Self {
            inner: Handle::new(native::SearchContext::default()),
        }
    }
    #[getter]
    fn results(&self) -> PyResult<Vec<SearchRecord>> {
        self.inner.with(|v| {
            Ok((0..v.results.len())
                .map(|index| SearchRecord {
                    inner: self
                        .inner
                        .project(move |v| v.results.get_mut(index).ok_or_else(missing)),
                })
                .collect())
        })
    }
    #[setter]
    fn set_results(&self, values: Vec<Py<SearchRecord>>) -> PyResult<()> {
        let n = values
            .into_iter()
            .map(|v| v.get().inner.with(|v| Ok(v.clone())))
            .collect::<PyResult<Vec<_>>>()?;
        self.inner.edit(|v| {
            v.results = n;
            Ok(())
        })
    }
    fn search(
        &self,
        py: Python<'_>,
        filters_value: Vec<Py<Filter>>,
        observations_value: Vec<Py<Observation>>,
    ) -> PyResult<usize> {
        let (f, o) = (
            filters(py, filters_value)?,
            observations(observations_value)?,
        );
        self.inner
            .edit(|v| py.detach(|| v.search(&f, &o)).map_err(errors::clone))
    }
    /// Write into RECORD_BYTES-sized MutableBytes; false leaves output unchanged.
    fn get(&self, index: &Bound<'_, PyAny>, out: &MutableBytes) -> PyResult<bool> {
        let index = strict_usize(index)?;
        out.with_mut(|out| {
            let out: &mut [u8; native::RECORD_BYTES] = out.try_into().map_err(|_| {
                pyo3::exceptions::PyValueError::new_err("expected RECORD_BYTES mutable output")
            })?;
            self.inner
                .with(|v| v.get(index, out).map_err(errors::clone))
        })
    }
}
#[pyfunction]
fn manual_number(text: &Bound<'_, PyAny>) -> PyResult<i32> {
    native::manual_number(strict_bytes(text)?).map_err(errors::clone)
}
#[pyfunction]
fn manual_commit(text: &Bound<'_, PyAny>) -> PyResult<u16> {
    native::manual_commit(strict_bytes(text)?).map_err(errors::clone)
}
#[pyfunction]
fn label_number(label: &Bound<'_, PyAny>) -> PyResult<i32> {
    native::label_number(strict_bytes(label)?).map_err(errors::clone)
}
#[pyfunction]
fn mi_index(s: &Bound<'_, PyAny>) -> PyResult<i32> {
    Ok(native::mi_index(strict_bytes(s)?))
}
#[pyfunction]
fn com_matches(
    py: Python<'_>,
    values: Vec<Py<Filter>>,
    p: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    Ok(native::com_matches(
        &filters(py, values)?,
        strict_bytes(p)?,
        strict_i32(index)?,
    ))
}
#[pyfunction]
fn com_only(py: Python<'_>, values: Vec<Py<Filter>>, p: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(native::com_only(&filters(py, values)?, strict_bytes(p)?))
}
#[pyfunction]
fn device_matches(
    py: Python<'_>,
    values: Vec<Py<Filter>>,
    com: &Bound<'_, PyAny>,
    hub: &Bound<'_, PyAny>,
    audio: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    Ok(native::device_matches(
        &filters(py, values)?,
        strict_bytes(com)?,
        strict_bytes(hub)?,
        strict_bytes(audio)?,
    ))
}
#[pyfunction]
fn marker(py: Python<'_>, values: Vec<Py<Filter>>, com: &Bound<'_, PyAny>) -> PyResult<Option<u8>> {
    Ok(native::marker(&filters(py, values)?, strict_bytes(com)?))
}
#[pyfunction]
fn instance_prefix<'py>(
    py: Python<'py>,
    id: &Bound<'_, PyAny>,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    Ok(native::instance_prefix(strict_bytes(id)?).map(|v| PyBytes::new(py, &v)))
}
#[pyfunction]
#[pyo3(signature=(suffix,full_id,parent))]
fn node_key<'py>(
    py: Python<'py>,
    suffix: &Bound<'_, PyAny>,
    full_id: &Bound<'_, PyAny>,
    parent: Option<&Bound<'_, PyAny>>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &native::node_key(
            strict_bytes(suffix)?,
            strict_bytes(full_id)?,
            parent.map(strict_bytes).transpose()?,
        ),
    ))
}
#[pyfunction]
fn join_ports(
    py: Python<'_>,
    id: &Bound<'_, PyAny>,
    ports: Vec<Py<Port>>,
) -> PyResult<(bool, Vec<Py<Port>>)> {
    let values = ports
        .iter()
        .map(|v| v.get().inner.with(|v| Ok(v.clone())))
        .collect::<PyResult<Vec<_>>>()?;
    let (parent, matched) = native::join_ports(strict_bytes(id)?, &values);
    Ok((
        parent,
        matched
            .into_iter()
            .map(|port| {
                ports[values
                    .iter()
                    .position(|v| std::ptr::eq(v, port))
                    .expect("native member")]
                .clone_ref(py)
            })
            .collect(),
    ))
}
#[pyfunction]
fn header_query(
    record: &SearchRecord,
    prefix: &Bound<'_, PyAny>,
    parent_prefix: &Bound<'_, PyAny>,
    wave_in: &Bound<'_, PyAny>,
    wave_out: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let (p, pp, wi, wo) = (
        strict_bytes(prefix)?.to_vec(),
        strict_bytes(parent_prefix)?.to_vec(),
        bytes_vec(wave_in)?,
        bytes_vec(wave_out)?,
    );
    record
        .inner
        .with(|v| Ok(native::header_query(v, &p, &pp, &wi, &wo)))
}
#[pyfunction]
fn candidates(
    records: Vec<Py<SearchRecord>>,
    prefix: &Bound<'_, PyAny>,
    suffixes: &Bound<'_, PyAny>,
    previous: &Bound<'_, PyAny>,
) -> PyResult<(Vec<Candidate>, Option<usize>)> {
    let records = records
        .into_iter()
        .map(|v| v.get().inner.with(|v| Ok(v.clone())))
        .collect::<PyResult<Vec<_>>>()?;
    let (values, selected) = native::candidates(
        &records,
        strict_bytes(prefix)?,
        &bytes_vec(suffixes)?,
        strict_u32(previous)?,
    )
    .map_err(errors::clone)?;
    Ok((
        values
            .into_iter()
            .map(|v| Candidate {
                inner: Handle::new(v),
            })
            .collect(),
        selected,
    ))
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "search")?;
    m.add("EXPORTS", native::EXPORTS.to_vec())?;
    m.add("RECORD_BYTES", native::RECORD_BYTES)?;
    m.add("PORTS_GUID", native::PORTS_GUID)?;
    m.add("USB_GUID", native::USB_GUID)?;
    m.add("PORTS_FLAGS", native::PORTS_FLAGS)?;
    m.add("USB_FLAGS", native::USB_FLAGS)?;
    m.add_class::<Filter>()?;
    m.add_class::<Port>()?;
    m.add_class::<Slot>()?;
    m.add_class::<SearchRecord>()?;
    m.add_class::<Observation>()?;
    m.add_class::<SearchContext>()?;
    m.add_class::<Candidate>()?;
    m.add_function(wrap_pyfunction!(manual_number, &m)?)?;
    m.add_function(wrap_pyfunction!(manual_commit, &m)?)?;
    m.add_function(wrap_pyfunction!(label_number, &m)?)?;
    m.add_function(wrap_pyfunction!(mi_index, &m)?)?;
    m.add_function(wrap_pyfunction!(com_matches, &m)?)?;
    m.add_function(wrap_pyfunction!(com_only, &m)?)?;
    m.add_function(wrap_pyfunction!(device_matches, &m)?)?;
    m.add_function(wrap_pyfunction!(marker, &m)?)?;
    m.add_function(wrap_pyfunction!(instance_prefix, &m)?)?;
    m.add_function(wrap_pyfunction!(node_key, &m)?)?;
    m.add_function(wrap_pyfunction!(join_ports, &m)?)?;
    m.add_function(wrap_pyfunction!(header_query, &m)?)?;
    m.add_function(wrap_pyfunction!(candidates, &m)?)?;
    root.add_submodule(&m)
}
