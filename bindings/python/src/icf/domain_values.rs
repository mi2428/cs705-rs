//! Typed stored coordinate, calendar and altitude values.
use super::values::{Capture, Input, Output};
use crate::conversion::*;
use cs705_core::icf as core;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};

#[pyclass(module = "cs705.icf", frozen)]
pub struct MemoryGroup {
    descriptor: usize,
    name: Vec<u8>,
    slots: Vec<Option<usize>>,
}
#[pymethods]
impl MemoryGroup {
    fn raw_descriptor(&self) -> usize {
        self.descriptor
    }
    fn name_bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.name)
    }
    fn slots(&self) -> Vec<Option<usize>> {
        self.slots.clone()
    }
}
#[pyclass(module = "cs705.icf", frozen)]
pub struct GpsGroup {
    name: Option<Vec<u8>>,
    slots: Vec<usize>,
}
#[pymethods]
impl GpsGroup {
    fn name_bytes<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.name.as_ref().map(|name| PyBytes::new(py, name))
    }
    fn physical_slots(&self) -> Vec<usize> {
        self.slots.clone()
    }
}
#[pyclass(module = "cs705.icf", frozen)]
pub struct RepeaterGroup {
    name: Vec<u8>,
    slots: Vec<usize>,
}
#[pymethods]
impl RepeaterGroup {
    fn name_bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.name)
    }
    fn physical_slots(&self) -> Vec<usize> {
        self.slots.clone()
    }
}
impl Capture for Vec<core::MemoryGroup<'_>> {
    type Owned = Vec<MemoryGroup>;
    fn capture(self) -> Self::Owned {
        self.into_iter()
            .map(|g| MemoryGroup {
                descriptor: g.raw_descriptor(),
                name: g.name_bytes().to_vec(),
                slots: g.slots().to_vec(),
            })
            .collect()
    }
}
impl Capture for Vec<core::GpsGroup<'_>> {
    type Owned = Vec<GpsGroup>;
    fn capture(self) -> Self::Owned {
        self.into_iter()
            .map(|g| GpsGroup {
                name: g.name_bytes().map(<[u8]>::to_vec),
                slots: g.physical_slots().to_vec(),
            })
            .collect()
    }
}
impl Capture for Vec<core::RepeaterGroup<'_>> {
    type Owned = Vec<RepeaterGroup>;
    fn capture(self) -> Self::Owned {
        self.into_iter()
            .map(|g| RepeaterGroup {
                name: g.name_bytes().to_vec(),
                slots: g.physical_slots().to_vec(),
            })
            .collect()
    }
}
macro_rules! group_output {
    ($($name:ty),*) => { $(impl Output for Vec<$name> {
        fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
            let values = self.into_iter().map(|g| Py::new(py, g)).collect::<PyResult<Vec<_>>>()?;
            Ok(PyList::new(py, values)?.into_any())
        }
    })* };
}
group_output!(MemoryGroup, GpsGroup, RepeaterGroup);

macro_rules! value {
    ($name:ident { $($field:ident: $ty:ty => $set:ident, $strict:ident);* $(;)? }) => {
        #[pyclass(module = "cs705.icf")]
        #[derive(Clone, Copy)]
        pub struct $name { pub inner: core::$name }
        impl $name { pub fn from_core(inner: core::$name) -> Self { Self { inner } } }
        #[pymethods]
        impl $name {
            #[new]
            fn new($($field: &Bound<'_, PyAny>),*) -> PyResult<Self> { Ok(Self { inner: core::$name { $($field: $strict($field)?),* } }) }
            $(#[getter] fn $field(&self) -> $ty { self.inner.$field }
            #[setter] fn $set(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> { self.inner.$field = $strict(value)?; Ok(()) })*
        }
        impl Input<'_> for core::$name {
            fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> { Ok(value.extract::<PyRef<'_, $name>>()?.inner) }
        }
        impl Capture for core::$name {
            type Owned = Self;
            fn capture(self) -> Self { self }
        }
        impl Output for core::$name {
            fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> { Ok(Py::new(py, $name::from_core(self))?.into_bound(py).into_any()) }
        }
    }
}
value!(GpsCoordinate { degrees: u8 => set_degrees, strict_u8; thousandths_of_minute: u16 => set_thousandths_of_minute, strict_u16; negative: bool => set_negative, strict_bool });
value!(GpsDate { year: u16 => set_year, strict_u16; month: u8 => set_month, strict_u8; day: u8 => set_day, strict_u8 });
value!(GpsTime { hour: u8 => set_hour, strict_u8; minute: u8 => set_minute, strict_u8; second: u8 => set_second, strict_u8 });
value!(RepeaterCoordinate { degrees: u8 => set_degrees, strict_u8; thousandths_of_minute: u16 => set_thousandths_of_minute, strict_u16; negative: bool => set_negative, strict_bool });

#[pyclass(module = "cs705.icf", frozen)]
#[derive(Clone, Copy)]
pub struct GpsAlarm {
    pub inner: core::GpsAlarm,
}
impl GpsAlarm {
    pub fn from_core(inner: core::GpsAlarm) -> Self {
        Self { inner }
    }
}
#[pymethods]
impl GpsAlarm {
    #[classattr]
    #[pyo3(name = "Off")]
    fn off() -> Self {
        Self {
            inner: core::GpsAlarm::Off,
        }
    }
    #[classattr]
    #[pyo3(name = "Receive")]
    fn receive() -> Self {
        Self {
            inner: core::GpsAlarm::Receive,
        }
    }
    #[staticmethod]
    #[pyo3(name = "Group", signature = (group))]
    fn group(group: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self {
            inner: core::GpsAlarm::Group(group.map(strict_usize).transpose()?),
        })
    }
    #[staticmethod]
    #[pyo3(name = "Memory")]
    fn memory(group: &Bound<'_, PyAny>, index: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::GpsAlarm::Memory {
                group: strict_usize(group)?,
                index: strict_usize(index)?,
            },
        })
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self.inner {
            core::GpsAlarm::Off => "Off",
            core::GpsAlarm::Receive => "Receive",
            core::GpsAlarm::Group(_) => "Group",
            core::GpsAlarm::Memory { .. } => "Memory",
        }
    }
    #[getter]
    fn group_index(&self) -> Option<usize> {
        match self.inner {
            core::GpsAlarm::Group(group) => group,
            core::GpsAlarm::Memory { group, .. } => Some(group),
            _ => None,
        }
    }
    #[getter]
    fn row_index(&self) -> Option<usize> {
        match self.inner {
            core::GpsAlarm::Memory { index, .. } => Some(index),
            _ => None,
        }
    }
}
impl Input<'_> for core::GpsAlarm {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(value.extract::<PyRef<'_, GpsAlarm>>()?.inner)
    }
}
impl Output for core::GpsAlarm {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(Py::new(py, GpsAlarm::from_core(self))?
            .into_bound(py)
            .into_any())
    }
}
impl Capture for core::GpsAlarm {
    type Owned = Self;
    fn capture(self) -> Self {
        self
    }
}

#[pyclass(module = "cs705.icf", frozen)]
#[derive(Clone, Copy)]
pub struct GpsAltitude {
    pub inner: core::GpsAltitude,
}
impl GpsAltitude {
    pub fn from_core(inner: core::GpsAltitude) -> Self {
        Self { inner }
    }
}
#[pymethods]
impl GpsAltitude {
    #[classattr]
    #[pyo3(name = "Unknown")]
    fn unknown() -> Self {
        Self {
            inner: core::GpsAltitude::Unknown,
        }
    }
    #[staticmethod]
    #[pyo3(name = "TenthsOfFoot")]
    fn tenths_of_foot(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::GpsAltitude::TenthsOfFoot(strict_i32(value)?),
        })
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self.inner {
            core::GpsAltitude::Unknown => "Unknown",
            core::GpsAltitude::TenthsOfFoot(_) => "TenthsOfFoot",
        }
    }
    #[getter]
    fn value(&self) -> Option<i32> {
        match self.inner {
            core::GpsAltitude::Unknown => None,
            core::GpsAltitude::TenthsOfFoot(value) => Some(value),
        }
    }
}
impl Input<'_> for core::GpsAltitude {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(value.extract::<PyRef<'_, GpsAltitude>>()?.inner)
    }
}
impl Output for core::GpsAltitude {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(Py::new(py, GpsAltitude::from_core(self))?
            .into_bound(py)
            .into_any())
    }
}
impl Capture for core::GpsAltitude {
    type Owned = Self;
    fn capture(self) -> Self {
        self
    }
}

#[pyclass(module = "cs705.icf")]
pub struct RepeaterRecord {
    value: core::RepeaterRecord,
    #[pyo3(get, set)]
    pub latitude: Py<RepeaterCoordinate>,
    #[pyo3(get, set)]
    pub longitude: Py<RepeaterCoordinate>,
}
impl RepeaterRecord {
    pub fn from_core(py: Python<'_>, value: core::RepeaterRecord) -> PyResult<Self> {
        Ok(Self {
            latitude: Py::new(py, RepeaterCoordinate::from_core(value.latitude))?,
            longitude: Py::new(py, RepeaterCoordinate::from_core(value.longitude))?,
            value,
        })
    }
    pub fn to_core(&self, py: Python<'_>) -> PyResult<core::RepeaterRecord> {
        let mut value = self.value.clone();
        value.latitude = self.latitude.try_borrow(py)?.inner;
        value.longitude = self.longitude.try_borrow(py)?.inner;
        Ok(value)
    }
}
macro_rules! repeater_fields {
    ($($name:ident: $ty:ty => $set:ident;)*) => {
        #[pymethods]
        impl RepeaterRecord {
            #[new]
            fn new(py: Python<'_>, $($name: &Bound<'_, PyAny>,)* latitude: Py<RepeaterCoordinate>, longitude: Py<RepeaterCoordinate>) -> PyResult<Self> {
                let value = core::RepeaterRecord { $($name: <$ty as Input>::input($name)?,)* latitude: latitude.try_borrow(py)?.inner, longitude: longitude.try_borrow(py)?.inner };
                Ok(Self { value, latitude, longitude })
            }
            $(#[getter] fn $name<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> { self.value.$name.output(py) }
            #[setter] fn $set(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> { self.value.$name = <$ty as Input>::input(value)?; Ok(()) })*
        }
    }
}
repeater_fields! {
    name: [u8;16] => set_name;
    sub_name: [u8;8] => set_sub_name;
    repeater_call: [u8;8] => set_repeater_call;
    gateway_call: [u8;8] => set_gateway_call;
    frequency_hz: u32 => set_frequency_hz;
    offset_hz: u32 => set_offset_hz;
    mode: core::RepeaterMode => set_mode;
    duplex: core::RepeaterDuplex => set_duplex;
    tone: core::RepeaterTone => set_tone;
    tone_index: Option<u8> => set_tone_index;
    use_repeater: bool => set_use_repeater;
    position: core::RepeaterPosition => set_position;
    utc_offset_minutes: i16 => set_utc_offset_minutes;
}
impl Input<'_> for core::RepeaterRecord {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        value
            .extract::<PyRef<'_, RepeaterRecord>>()?
            .to_core(value.py())
    }
}
impl Output for core::RepeaterRecord {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(Py::new(py, RepeaterRecord::from_core(py, self)?)?
            .into_bound(py)
            .into_any())
    }
}
impl Capture for core::RepeaterRecord {
    type Owned = Self;
    fn capture(self) -> Self {
        self
    }
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<GpsCoordinate>()?;
    m.add_class::<GpsDate>()?;
    m.add_class::<GpsTime>()?;
    m.add_class::<GpsAltitude>()?;
    m.add_class::<RepeaterCoordinate>()?;
    m.add_class::<GpsAlarm>()?;
    m.add_class::<RepeaterRecord>()?;
    m.add_class::<MemoryGroup>()?;
    m.add_class::<GpsGroup>()?;
    m.add_class::<RepeaterGroup>()?;
    Ok(())
}
