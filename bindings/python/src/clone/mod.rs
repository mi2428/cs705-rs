//! Native clone codecs and explicit, caller-owned transport operations.
pub mod information;
pub mod receiver;
pub mod search;
pub mod session;
mod values;
use crate::{conversion::*, errors};
use cs705_core::clone as core;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
pub(crate) use values::Handle;

pub(crate) fn add_enum(m: &Bound<'_, PyModule>, name: &str, variants: &[&str]) -> PyResult<()> {
    let entries = PyDict::new(m.py());
    for (i, name) in variants.iter().enumerate() {
        entries.set_item(*name, i)?;
    }
    let kwargs = PyDict::new(m.py());
    let module = m.name()?;
    let module = module.to_str()?;
    kwargs.set_item(
        "module",
        if module == "clone" {
            "cs705.clone".to_string()
        } else {
            format!("cs705.clone.{module}")
        },
    )?;
    m.add(
        name,
        m.py()
            .import("enum")?
            .getattr("Enum")?
            .call((name, entries), Some(&kwargs))?,
    )
}
pub(crate) fn enum_in(value: &Bound<'_, PyAny>, module: &str, name: &str) -> PyResult<usize> {
    if !value.is_instance(&value.py().import(module)?.getattr(name)?)? {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "expected {module}.{name}"
        )));
    }
    strict_usize(&value.getattr("value")?)
}
pub(crate) fn enum_out<'py>(
    py: Python<'py>,
    module: &str,
    name: &str,
    value: usize,
) -> PyResult<Bound<'py, PyAny>> {
    py.import(module)?.getattr(name)?.call1((value,))
}
fn direction(value: &Bound<'_, PyAny>) -> PyResult<core::Direction> {
    Ok(match enum_in(value, "cs705.clone", "Direction")? {
        0 => core::Direction::ToRadio,
        1 => core::Direction::FromRadio,
        _ => return Err(pyo3::exceptions::PyValueError::new_err("invalid Direction")),
    })
}
fn range_in(value: &Bound<'_, PyAny>) -> PyResult<std::ops::Range<u32>> {
    let range = strict_range(value)?;
    Ok(u32::try_from(range.start)
        .map_err(|_| pyo3::exceptions::PyOverflowError::new_err("range start exceeds u32"))?
        ..u32::try_from(range.end)
            .map_err(|_| pyo3::exceptions::PyOverflowError::new_err("range stop exceeds u32"))?)
}
fn range_out<'py>(py: Python<'py>, range: std::ops::Range<u32>) -> PyResult<Bound<'py, PyAny>> {
    py.import("builtins")?
        .getattr("range")?
        .call1((range.start, range.end))
}

#[pyclass(module = "cs705.clone", frozen)]
#[derive(Clone, Copy)]
pub struct Profile {
    pub inner: core::Profile,
}
#[pymethods]
impl Profile {
    fn __copy__(&self) -> Self {
        *self
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> Self {
        *self
    }
    #[new]
    fn new(selector: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Profile::try_from(strict_u8(selector)?).map_err(errors::clone)?,
        })
    }
    fn selector(&self) -> u8 {
        self.inner.selector()
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
#[pyclass(module = "cs705.clone", frozen)]
#[derive(Clone)]
pub struct ContextFields {
    pub(crate) inner: Handle<core::ContextFields>,
}
impl ContextFields {
    pub fn from_core(value: core::ContextFields) -> Self {
        Self {
            inner: Handle::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::ContextFields) -> PyResult<R>) -> PyResult<R> {
        self.inner.with(|v| f(v))
    }
    pub fn with_mut<R>(
        &self,
        f: impl FnOnce(&mut core::ContextFields) -> PyResult<R>,
    ) -> PyResult<R> {
        self.inner.with(f)
    }
}
#[pymethods]
impl ContextFields {
    fn __copy__(&self) -> PyResult<Self> {
        self.inner.with(|v| Ok(Self::from_core(*v)))
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.__copy__()
    }
    #[new]
    fn new(values: &Bound<'_, PyAny>) -> PyResult<Self> {
        let fields = values
            .try_iter()?
            .map(|v| strict_u32(&v?))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            inner: Handle::new(core::ContextFields(fields.try_into().map_err(|_| {
                pyo3::exceptions::PyValueError::new_err("expected twelve context fields")
            })?)),
        })
    }
    #[staticmethod]
    fn from_tail(tail: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: Handle::new(
                core::ContextFields::from_tail(strict_bytes(tail)?).map_err(errors::clone)?,
            ),
        })
    }
    #[getter]
    fn values(&self) -> PyResult<Vec<u32>> {
        self.inner.with(|v| Ok(v.0.to_vec()))
    }
    #[setter]
    fn set_values(&self, values: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = Self::new(values)?.inner.with(|v| Ok(*v))?;
        self.inner.with(|v| {
            *v = value;
            Ok(())
        })
    }
    fn __eq__(&self, other: &Self) -> PyResult<bool> {
        let a = self.inner.with(|v| Ok(*v))?;
        other.inner.with(|b| Ok(a == *b))
    }
}
#[pyclass(module = "cs705.clone", frozen)]
#[derive(Clone)]
pub struct Identity {
    pub(crate) inner: Handle<core::Identity>,
}
#[pymethods]
impl Identity {
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
        model: &Bound<'_, PyAny>,
        revision: &Bound<'_, PyAny>,
        tail: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Handle::new(core::Identity {
                model: strict_u16(model)?,
                revision: strict_u16(revision)?,
                tail: strict_bytes(tail)?.to_vec(),
            }),
        })
    }
    #[getter]
    fn model(&self) -> PyResult<u16> {
        self.inner.with(|v| Ok(v.model))
    }
    #[setter]
    fn set_model(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_u16(v)?;
        self.inner.with(|v| {
            v.model = n;
            Ok(())
        })
    }
    #[getter]
    fn revision(&self) -> PyResult<u16> {
        self.inner.with(|v| Ok(v.revision))
    }
    #[setter]
    fn set_revision(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_u16(v)?;
        self.inner.with(|v| {
            v.revision = n;
            Ok(())
        })
    }
    #[getter]
    fn tail<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.inner.with(|v| Ok(PyBytes::new(py, &v.tail)))
    }
    #[setter]
    fn set_tail(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_bytes(v)?.to_vec();
        self.inner.with(|i| {
            i.tail = v;
            Ok(())
        })
    }
    fn validate(&self) -> PyResult<Profile> {
        self.inner.with(|v| {
            Ok(Profile {
                inner: v.validate().map_err(errors::clone)?,
            })
        })
    }
    fn context(&self) -> PyResult<ContextFields> {
        self.inner.with(|v| {
            Ok(ContextFields {
                inner: Handle::new(v.context().map_err(errors::clone)?),
            })
        })
    }
    fn __eq__(&self, other: &Self) -> PyResult<bool> {
        let a = self.inner.with(|v| Ok(v.clone()))?;
        other.inner.with(|b| Ok(a == *b))
    }
}
#[pyclass(module = "cs705.clone", frozen)]
#[derive(Clone)]
pub struct IdentityGate {
    pub(crate) inner: Handle<core::IdentityGate>,
}
#[pymethods]
impl IdentityGate {
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
    fn new() -> Self {
        Self {
            inner: Handle::new(core::IdentityGate::default()),
        }
    }
    fn __eq__(&self, other: &Self) -> PyResult<bool> {
        let value = self.inner.with(|value| Ok(value.clone()))?;
        other.inner.with(|other| Ok(value == *other))
    }
    #[getter]
    fn cached_model(&self) -> PyResult<Option<u16>> {
        self.inner.with(|v| Ok(v.cached_model))
    }
    #[setter]
    fn set_cached_model(&self, v: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let v = v.map(strict_u16).transpose()?;
        self.inner.with(|g| {
            g.cached_model = v;
            Ok(())
        })
    }
    #[getter]
    fn cached_tail<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyBytes>>> {
        self.inner
            .with(|v| Ok(v.cached_tail.as_ref().map(|v| PyBytes::new(py, v))))
    }
    #[setter]
    fn set_cached_tail(&self, v: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let v = v.map(fixed_bytes).transpose()?;
        self.inner.with(|g| {
            g.cached_tail = v;
            Ok(())
        })
    }
    #[pyo3(signature=(identity, context, saved_selector, flag57))]
    fn apply(
        &self,
        identity: &Identity,
        context: Option<&ContextFields>,
        saved_selector: &Bound<'_, PyAny>,
        flag57: &Bound<'_, PyAny>,
    ) -> PyResult<u8> {
        let identity = identity.inner.with(|v| Ok(v.clone()))?;
        let context = context.map(|v| v.inner.with(|v| Ok(*v))).transpose()?;
        let (s, f) = (strict_u8(saved_selector)?, strict_u8(flag57)?);
        self.inner
            .with(|g| g.apply(&identity, context, s, f).map_err(errors::clone))
    }
}

#[pyclass(module = "cs705.clone", frozen)]
pub struct Frame {
    inner: Handle<core::Frame>,
}
#[pymethods]
#[allow(non_snake_case)]
impl Frame {
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
    fn Identify() -> Self {
        Self {
            inner: Handle::new(core::Frame::Identify),
        }
    }
    #[staticmethod]
    fn Read() -> Self {
        Self {
            inner: Handle::new(core::Frame::Read),
        }
    }
    #[staticmethod]
    fn Write() -> Self {
        Self {
            inner: Handle::new(core::Frame::Write),
        }
    }
    #[staticmethod]
    fn EndUncounted() -> Self {
        Self {
            inner: Handle::new(core::Frame::EndUncounted),
        }
    }
    #[staticmethod]
    fn Acknowledgement() -> Self {
        Self {
            inner: Handle::new(core::Frame::Acknowledgement),
        }
    }
    #[staticmethod]
    fn Response(identity: &Identity) -> PyResult<Self> {
        Ok(Self {
            inner: Handle::new(core::Frame::Response(
                identity.inner.with(|v| Ok(v.clone()))?,
            )),
        })
    }
    #[staticmethod]
    fn Data(address: &Bound<'_, PyAny>, payload: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: Handle::new(core::Frame::Data {
                address: strict_u32(address)?,
                payload: strict_bytes(payload)?.to_vec(),
            }),
        })
    }
    #[staticmethod]
    fn End(block_count: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: Handle::new(core::Frame::End {
                block_count: strict_u8(block_count)?,
            }),
        })
    }
    #[getter]
    fn kind(&self) -> PyResult<&'static str> {
        self.inner.with(|v| {
            Ok(match v {
                core::Frame::Identify => "Identify",
                core::Frame::Read => "Read",
                core::Frame::Write => "Write",
                core::Frame::Response(_) => "Response",
                core::Frame::Data { .. } => "Data",
                core::Frame::End { .. } => "End",
                core::Frame::EndUncounted => "EndUncounted",
                core::Frame::Acknowledgement => "Acknowledgement",
            })
        })
    }
    #[getter]
    fn identity(&self) -> PyResult<Identity> {
        self.inner.with(|v| {
            if matches!(v, core::Frame::Response(_)) {
                Ok(Identity {
                    inner: self.inner.project(|v| match v {
                        core::Frame::Response(i) => Ok(i),
                        _ => Err(pyo3::exceptions::PyReferenceError::new_err("not Response")),
                    }),
                })
            } else {
                Err(pyo3::exceptions::PyAttributeError::new_err("not Response"))
            }
        })
    }
    #[getter]
    fn address(&self) -> PyResult<u32> {
        self.inner.with(|v| match v {
            core::Frame::Data { address, .. } => Ok(*address),
            _ => Err(pyo3::exceptions::PyAttributeError::new_err("not Data")),
        })
    }
    #[setter]
    fn set_identity(&self, value: &Identity) -> PyResult<()> {
        let value = value.inner.with(|value| Ok(value.clone()))?;
        self.inner.edit(|frame| match frame {
            core::Frame::Response(identity) => {
                *identity = value;
                Ok(())
            }
            _ => Err(pyo3::exceptions::PyAttributeError::new_err("not Response")),
        })
    }
    #[setter]
    fn set_address(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_u32(value)?;
        self.inner.with(|v| match v {
            core::Frame::Data { address, .. } => {
                *address = n;
                Ok(())
            }
            _ => Err(pyo3::exceptions::PyAttributeError::new_err("not Data")),
        })
    }
    #[getter]
    fn payload<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.inner.with(|v| match v {
            core::Frame::Data { payload, .. } => Ok(PyBytes::new(py, payload)),
            _ => Err(pyo3::exceptions::PyAttributeError::new_err("not Data")),
        })
    }
    #[setter]
    fn set_payload(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_bytes(value)?.to_vec();
        self.inner.with(|v| match v {
            core::Frame::Data { payload, .. } => {
                *payload = n;
                Ok(())
            }
            _ => Err(pyo3::exceptions::PyAttributeError::new_err("not Data")),
        })
    }
    #[getter]
    fn block_count(&self) -> PyResult<u8> {
        self.inner.with(|v| match v {
            core::Frame::End { block_count } => Ok(*block_count),
            _ => Err(pyo3::exceptions::PyAttributeError::new_err("not End")),
        })
    }
    #[setter]
    fn set_block_count(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_u8(value)?;
        self.inner.with(|v| match v {
            core::Frame::End { block_count } => {
                *block_count = n;
                Ok(())
            }
            _ => Err(pyo3::exceptions::PyAttributeError::new_err("not End")),
        })
    }
    fn encode<'py>(
        &self,
        py: Python<'py>,
        direction_value: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let d = direction(direction_value)?;
        self.inner
            .with(|v| Ok(PyBytes::new(py, &v.encode(d).map_err(errors::clone)?)))
    }
    #[staticmethod]
    fn decode(wire: &Bound<'_, PyAny>, direction_value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: Handle::new(
                core::Frame::decode(strict_bytes(wire)?, direction(direction_value)?)
                    .map_err(errors::clone)?,
            ),
        })
    }
    fn check_data_range(&self, allowed: &Bound<'_, PyAny>) -> PyResult<()> {
        let r = range_in(allowed)?;
        self.inner
            .with(|v| v.check_data_range(r).map_err(errors::clone))
    }
    fn __eq__(&self, other: &Self) -> PyResult<bool> {
        let a = self.inner.with(|v| Ok(v.clone()))?;
        other.inner.with(|v| Ok(a == *v))
    }
}
#[pyclass(module = "cs705.clone")]
pub struct Framer {
    inner: core::Framer,
}
#[pymethods]
impl Framer {
    #[new]
    fn new() -> Self {
        Self {
            inner: core::Framer::default(),
        }
    }
    fn push<'py>(
        &mut self,
        py: Python<'py>,
        byte: &Bound<'_, PyAny>,
    ) -> PyResult<Option<Bound<'py, PyBytes>>> {
        Ok(self
            .inner
            .push(strict_u8(byte)?)
            .map_err(errors::clone)?
            .map(|v| PyBytes::new(py, &v)))
    }
    fn finish(&mut self) -> PyResult<()> {
        self.inner.finish().map_err(errors::clone)
    }
}
#[pyclass(module = "cs705.clone")]
pub struct Extractor {
    inner: core::Extractor,
}
#[pymethods]
impl Extractor {
    #[new]
    fn new() -> Self {
        Self {
            inner: core::Extractor::default(),
        }
    }
    fn push(&mut self, bytes: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.push(strict_bytes(bytes)?);
        Ok(())
    }
    fn partial<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, self.inner.partial())
    }
    fn queued(&self) -> usize {
        self.inner.queued()
    }
    fn extract<'py>(&mut self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.extract())
    }
}
#[pyfunction]
fn decode_hex_script<'py>(
    py: Python<'py>,
    text: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &core::decode_hex_script(strict_bytes(text)?),
    ))
}
#[pyfunction]
fn script_token<'py>(
    py: Python<'py>,
    script: &Bound<'_, PyAny>,
) -> PyResult<(Bound<'py, PyBytes>, usize, bool)> {
    let (v, n, b) = core::script_token(strict_bytes(script)?);
    Ok((PyBytes::new(py, v), n, b))
}
#[pyfunction]
fn read_range<'py>(py: Python<'py>, selector: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    range_out(
        py,
        core::read_range(strict_u8(selector)?).map_err(errors::clone)?,
    )
}
#[pyfunction]
fn write_blocks<'py>(
    py: Python<'py>,
    selector: &Bound<'_, PyAny>,
    mask: &Bound<'_, PyAny>,
) -> PyResult<Vec<Bound<'py, PyAny>>> {
    core::write_blocks(strict_u8(selector)?, strict_u8(mask)?)
        .map_err(errors::clone)?
        .into_iter()
        .map(|r| range_out(py, r))
        .collect()
}
#[pyfunction]
fn write_blocks_with_cap<'py>(
    py: Python<'py>,
    selector: &Bound<'_, PyAny>,
    mask: &Bound<'_, PyAny>,
    cap: &Bound<'_, PyAny>,
) -> PyResult<Vec<Bound<'py, PyAny>>> {
    core::write_blocks_with_cap(strict_u8(selector)?, strict_u8(mask)?, strict_u32(cap)?)
        .map_err(errors::clone)?
        .into_iter()
        .map(|r| range_out(py, r))
        .collect()
}
#[pyfunction]
#[pyo3(signature=(image, source, destination, seed))]
fn convert_image<'py>(
    py: Python<'py>,
    image: &Bound<'_, PyAny>,
    source: &Profile,
    destination: &Profile,
    seed: Option<&Bound<'_, PyAny>>,
) -> PyResult<Bound<'py, PyBytes>> {
    let (image, seed) = (
        strict_bytes(image)?.to_vec(),
        seed.map(strict_bytes).transpose()?.map(<[u8]>::to_vec),
    );
    let (s, d) = (source.inner, destination.inner);
    let out = py
        .detach(|| core::convert_image(&image, s, d, seed.as_deref()))
        .map_err(errors::clone)?;
    Ok(PyBytes::new(py, &out))
}

pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "clone")?;
    m.add("Error", root.getattr("Error")?)?;
    add_enum(&m, "Direction", &["ToRadio", "FromRadio"])?;
    m.add("IMAGE_CAPACITY", core::IMAGE_CAPACITY)?;
    m.add_class::<Profile>()?;
    m.add_class::<ContextFields>()?;
    m.add_class::<Identity>()?;
    m.add_class::<IdentityGate>()?;
    m.add_class::<Frame>()?;
    m.add_class::<Framer>()?;
    m.add_class::<Extractor>()?;
    m.add_function(wrap_pyfunction!(decode_hex_script, &m)?)?;
    m.add_function(wrap_pyfunction!(script_token, &m)?)?;
    m.add_function(wrap_pyfunction!(read_range, &m)?)?;
    m.add_function(wrap_pyfunction!(write_blocks, &m)?)?;
    m.add_function(wrap_pyfunction!(write_blocks_with_cap, &m)?)?;
    m.add_function(wrap_pyfunction!(convert_image, &m)?)?;
    session::register(&m)?;
    search::register(&m)?;
    information::register(&m)?;
    root.add_submodule(&m)
}
