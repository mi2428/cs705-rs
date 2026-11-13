//! Native linked IPv4 edits, keeping their nonstandard typed-text parser.
use super::super::values::{Input, Output};
use crate::conversion::*;
use cs705_core::icf::settings::network as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::net::Ipv4Addr;

fn address(value: &Bound<'_, PyAny>) -> PyResult<Ipv4Addr> {
    let ty = value.py().import("ipaddress")?.getattr("IPv4Address")?;
    if !value.is_instance(&ty)? {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "expected ipaddress.IPv4Address",
        ));
    }
    Ok(Ipv4Addr::from(fixed_bytes::<4>(&value.getattr("packed")?)?))
}
fn address_out<'py>(py: Python<'py>, value: Ipv4Addr) -> PyResult<Bound<'py, PyAny>> {
    py.import("ipaddress")?
        .getattr("IPv4Address")?
        .call1((PyBytes::new(py, &value.octets()),))
}

#[pyclass(module = "cs705.icf.settings.network")]
#[derive(Clone, Copy)]
pub struct Configuration {
    pub inner: core::Configuration,
}
macro_rules! configuration {
    ($($field:ident => $set:ident;)*) => {
        #[pymethods]
        impl Configuration {
            #[new]
            fn new($($field: &Bound<'_, PyAny>,)* prefix: &Bound<'_, PyAny>) -> PyResult<Self> { Ok(Self { inner: core::Configuration { $($field: address($field)?,)* prefix: strict_u8(prefix)? } }) }
            $(#[getter] fn $field<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> { address_out(py, self.inner.$field) }
            #[setter] fn $set(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> { self.inner.$field = address(value)?; Ok(()) })*
            #[getter] fn prefix(&self) -> u8 { self.inner.prefix }
            #[setter] fn set_prefix(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> { self.inner.prefix = strict_u8(value)?; Ok(()) }
        }
    }
}
configuration!(address => set_address; gateway => set_gateway; primary_dns => set_primary_dns; secondary_dns => set_secondary_dns;);
impl Output for core::Configuration {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(Py::new(py, Configuration { inner: self })?
            .into_bound(py)
            .into_any())
    }
}
impl super::super::values::Capture for core::Configuration {
    type Owned = Self;
    fn capture(self) -> Self {
        self
    }
}

#[pyclass(module = "cs705.icf.settings.network")]
#[derive(Clone, Copy)]
pub struct Effect {
    pub inner: core::Effect,
}
#[pymethods]
impl Effect {
    #[new]
    fn new(
        stored: &Bound<'_, PyAny>,
        notify_flags: &Bound<'_, PyAny>,
        gray: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::Effect {
                stored: strict_bool(stored)?,
                notify_flags: strict_u32(notify_flags)?,
                gray: strict_bool(gray)?,
            },
        })
    }
    #[getter]
    fn stored(&self) -> bool {
        self.inner.stored
    }
    #[setter]
    fn set_stored(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.stored = strict_bool(value)?;
        Ok(())
    }
    #[getter]
    fn notify_flags(&self) -> u32 {
        self.inner.notify_flags
    }
    #[setter]
    fn set_notify_flags(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.notify_flags = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn gray(&self) -> bool {
        self.inner.gray
    }
    #[setter]
    fn set_gray(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.gray = strict_bool(value)?;
        Ok(())
    }
}
impl Output for core::Effect {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(Py::new(py, Effect { inner: self })?
            .into_bound(py)
            .into_any())
    }
}
impl super::super::values::Capture for core::Effect {
    type Owned = Self;
    fn capture(self) -> Self {
        self
    }
}
impl Input<'_> for core::Field {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(
            match super::super::enum_value(value, "cs705.icf.settings.network", "Field")? {
                0 => Self::Address,
                1 => Self::Gateway,
                2 => Self::PrimaryDns,
                3 => Self::SecondaryDns,
                _ => return Err(pyo3::exceptions::PyValueError::new_err("invalid Field")),
            },
        )
    }
}
#[pyfunction]
fn parse_typed<'py>(
    py: Python<'py>,
    source: &Bound<'_, PyAny>,
    allow_zero: &Bound<'_, PyAny>,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    core::parse_typed(strict_bytes(source)?, strict_bool(allow_zero)?)
        .map(|value| address_out(py, value))
        .transpose()
}
#[pyfunction]
fn prefix_mask(prefix: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(core::prefix_mask(strict_u8(prefix)?))
}
#[pyfunction]
fn correct_address<'py>(
    py: Python<'py>,
    address: &Bound<'_, PyAny>,
    prefix: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    address_out(
        py,
        core::correct_address(self::address(address)?, strict_u8(prefix)?),
    )
}
#[pyfunction]
fn correct_gateway<'py>(
    py: Python<'py>,
    address: &Bound<'_, PyAny>,
    gateway: &Bound<'_, PyAny>,
    prefix: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    address_out(
        py,
        core::correct_gateway(
            self::address(address)?,
            self::address(gateway)?,
            strict_u8(prefix)?,
        ),
    )
}
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "network")?;
    // This nested module has its own public Python import path.
    let entries = pyo3::types::PyDict::new(m.py());
    for (name, code) in [
        ("Address", 0),
        ("Gateway", 1),
        ("PrimaryDns", 2),
        ("SecondaryDns", 3),
    ] {
        entries.set_item(name, code)?;
    }
    let kwargs = pyo3::types::PyDict::new(m.py());
    kwargs.set_item("module", "cs705.icf.settings.network")?;
    m.add(
        "Field",
        m.py()
            .import("enum")?
            .getattr("Enum")?
            .call(("Field", entries), Some(&kwargs))?,
    )?;
    m.add_class::<Configuration>()?;
    m.add_class::<Effect>()?;
    m.add_function(wrap_pyfunction!(parse_typed, &m)?)?;
    m.add_function(wrap_pyfunction!(prefix_mask, &m)?)?;
    m.add_function(wrap_pyfunction!(correct_address, &m)?)?;
    m.add_function(wrap_pyfunction!(correct_gateway, &m)?)?;
    parent.add_submodule(&m)
}
