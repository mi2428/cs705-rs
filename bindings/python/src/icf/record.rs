//! Owned decoded holder shared by codecs and higher-level domains.
mod buffer;
mod history;
mod storage;
use crate::{conversion::*, owner::Owner};
use cs705_core::icf::record::{ChannelField, RepeaterField, RepeaterText};
use cs705_core::icf::{MemoryCallsign, record as core};
use pyo3::prelude::*;
use pyo3::{PyTraverseError, PyVisit};

fn cleared_write() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err("write payload was cleared")
}

struct OwnedCallsignWrite {
    name: bool,
    bytes: Py<PyBytes>,
}
enum CallsignState {
    Owned(OwnedCallsignWrite),
    Cleared,
    Scoped {
        name: bool,
        bytes: crate::scoped::Loan<[u8]>,
    },
}
#[pyclass(module = "cs705.icf.record")]
pub struct CallsignWrite {
    state: CallsignState,
}
impl CallsignWrite {
    fn owned(name: bool, bytes: &Bound<'_, PyAny>) -> PyResult<Self> {
        let bytes = bytes.cast::<PyBytes>()?;
        if bytes.as_bytes().len() != if name { 16 } else { 8 } {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "CallsignWrite requires Name16 or Call8 bytes",
            ));
        }
        Ok(Self {
            state: CallsignState::Owned(OwnedCallsignWrite {
                name,
                bytes: bytes.clone().unbind(),
            }),
        })
    }
    pub fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::CallsignWrite<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        match &self.state {
            CallsignState::Owned(v) => {
                let bytes = v.bytes.bind(py).as_bytes();
                if v.name {
                    f(core::CallsignWrite::Name(bytes.try_into().map_err(
                        |_| pyo3::exceptions::PyValueError::new_err("expected 16 bytes"),
                    )?))
                } else {
                    f(core::CallsignWrite::Call(bytes.try_into().map_err(
                        |_| pyo3::exceptions::PyValueError::new_err("expected 8 bytes"),
                    )?))
                }
            }
            CallsignState::Cleared => Err(cleared_write()),
            CallsignState::Scoped { bytes, .. } => {
                bytes.with_ref(py, |_| Ok(()))?;
                Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "native payload borrowing requires an ordinary CallsignWrite",
                ))
            }
        }
    }
}
#[pymethods]
#[allow(non_snake_case)]
impl CallsignWrite {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let CallsignState::Owned(v) = &self.state {
            visit.call(&v.bytes)?;
        }
        Ok(())
    }
    fn __clear__(&mut self) {
        self.state = CallsignState::Cleared;
    }
    #[staticmethod]
    fn Name(bytes: &Bound<'_, PyAny>) -> PyResult<Self> {
        Self::owned(true, bytes)
    }
    #[staticmethod]
    fn Call(bytes: &Bound<'_, PyAny>) -> PyResult<Self> {
        Self::owned(false, bytes)
    }
    #[getter]
    fn variant(&self, py: Python<'_>) -> PyResult<&'static str> {
        let name = match &self.state {
            CallsignState::Owned(v) => v.name,
            CallsignState::Cleared => return Err(cleared_write()),
            CallsignState::Scoped { name, bytes } => bytes.with_ref(py, |_| Ok(*name))?,
        };
        Ok(if name { "Name" } else { "Call" })
    }
    #[getter]
    fn bytes(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match &self.state {
            CallsignState::Owned(v) => Ok(v.bytes.clone_ref(py).into_any()),
            CallsignState::Cleared => Err(cleared_write()),
            CallsignState::Scoped { bytes, .. } => {
                bytes.with_ref(py, |_| Ok(()))?;
                Ok(Py::new(py, crate::csv::nls::Bytes::from_loan(bytes.clone()))?.into_any())
            }
        }
    }
    fn replace(&mut self, py: Python<'_>, other: &CallsignWrite) -> PyResult<()> {
        let CallsignState::Owned(_) = &self.state else {
            self.variant(py)?;
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "scoped CallsignWrite is read-only",
            ));
        };
        let CallsignState::Owned(source) = &other.state else {
            other.variant(py)?;
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "replacement requires an ordinary CallsignWrite",
            ));
        };
        self.state = CallsignState::Owned(OwnedCallsignWrite {
            name: source.name,
            bytes: source.bytes.clone_ref(py),
        });
        Ok(())
    }
}
pub fn with_callsign_write<R>(
    py: Python<'_>,
    value: &core::CallsignWrite<'_>,
    f: impl FnOnce(Py<CallsignWrite>) -> PyResult<R>,
) -> PyResult<R> {
    let (name, bytes): (bool, &[u8]) = match value {
        core::CallsignWrite::Name(v) => (true, v.as_slice()),
        core::CallsignWrite::Call(v) => (false, v.as_slice()),
    };
    crate::scoped::Frame::with_ref(py, bytes, |bytes| {
        f(Py::new(
            py,
            CallsignWrite {
                state: CallsignState::Scoped { name, bytes },
            },
        )?)
    })
}

enum WriteFields {
    Body {
        offset: crate::scoped::Loan<usize>,
        bytes: crate::scoped::Loan<[u8]>,
    },
    Flag {
        byte: crate::scoped::Loan<u8>,
    },
}

enum OwnedRepeaterWrite {
    Body {
        stage: core::RepeaterStage,
        offset: usize,
        bytes: Py<PyBytes>,
    },
    Gateway(Py<PyBytes>),
    Use(u32),
}
enum RepeaterWriteState {
    Owned(OwnedRepeaterWrite),
    Cleared,
    Body {
        stage: crate::scoped::Loan<core::RepeaterStage>,
        offset: crate::scoped::Loan<usize>,
        bytes: crate::scoped::Loan<[u8]>,
    },
}
#[pyclass(module = "cs705.icf.record")]
pub struct RepeaterWrite {
    state: RepeaterWriteState,
}
impl RepeaterWrite {
    fn owned(value: OwnedRepeaterWrite) -> Self {
        Self {
            state: RepeaterWriteState::Owned(value),
        }
    }
    fn native_stage(&self, py: Python<'_>) -> PyResult<core::RepeaterStage> {
        match &self.state {
            RepeaterWriteState::Owned(v) => Ok(match v {
                OwnedRepeaterWrite::Body { stage, .. } => *stage,
                OwnedRepeaterWrite::Gateway(_) => core::RepeaterStage::GatewayCall,
                OwnedRepeaterWrite::Use(_) => core::RepeaterStage::Use,
            }),
            RepeaterWriteState::Cleared => Err(cleared_write()),
            RepeaterWriteState::Body { stage, .. } => stage.with_ref(py, |v| Ok(*v)),
        }
    }
    pub fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::RepeaterWrite<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        match &self.state {
            RepeaterWriteState::Owned(v) => f(match v {
                OwnedRepeaterWrite::Body {
                    stage,
                    offset,
                    bytes,
                } => core::RepeaterWrite::Body {
                    stage: *stage,
                    offset: *offset,
                    bytes: bytes.bind(py).as_bytes(),
                },
                OwnedRepeaterWrite::Gateway(bytes) => {
                    core::RepeaterWrite::Gateway(fixed_bytes(bytes.bind(py).as_any())?)
                }
                OwnedRepeaterWrite::Use(value) => core::RepeaterWrite::Use(*value),
            }),
            RepeaterWriteState::Cleared => Err(cleared_write()),
            RepeaterWriteState::Body { .. } => {
                self.native_stage(py)?;
                Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "native payload borrowing requires an ordinary RepeaterWrite",
                ))
            }
        }
    }
}
#[pymethods]
#[allow(non_snake_case)]
impl RepeaterWrite {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        match &self.state {
            RepeaterWriteState::Owned(
                OwnedRepeaterWrite::Body { bytes, .. } | OwnedRepeaterWrite::Gateway(bytes),
            ) => visit.call(bytes)?,
            _ => {}
        }
        Ok(())
    }
    fn __clear__(&mut self) {
        self.state = RepeaterWriteState::Cleared;
    }
    #[staticmethod]
    fn Body(
        stage: &Bound<'_, PyAny>,
        offset: &Bound<'_, PyAny>,
        bytes: Py<PyBytes>,
    ) -> PyResult<Self> {
        use super::values::Input;
        Ok(Self::owned(OwnedRepeaterWrite::Body {
            stage: core::RepeaterStage::input(stage)?,
            offset: strict_usize(offset)?,
            bytes,
        }))
    }
    #[staticmethod]
    fn Gateway(bytes: &Bound<'_, PyAny>) -> PyResult<Self> {
        let _ = fixed_bytes::<7>(bytes)?;
        Ok(Self::owned(OwnedRepeaterWrite::Gateway(
            bytes.cast::<PyBytes>()?.clone().unbind(),
        )))
    }
    #[staticmethod]
    fn Use(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::owned(OwnedRepeaterWrite::Use(strict_u32(value)?)))
    }
    #[getter]
    fn variant(&self, py: Python<'_>) -> PyResult<&'static str> {
        self.native_stage(py)?;
        match &self.state {
            RepeaterWriteState::Owned(v) => Ok(match v {
                OwnedRepeaterWrite::Body { .. } => "Body",
                OwnedRepeaterWrite::Gateway(_) => "Gateway",
                OwnedRepeaterWrite::Use(_) => "Use",
            }),
            RepeaterWriteState::Cleared => Err(cleared_write()),
            RepeaterWriteState::Body { .. } => Ok("Body"),
        }
    }
    fn stage<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        super::enum_result(
            py,
            "cs705.icf.record",
            "RepeaterStage",
            self.native_stage(py)? as i64,
        )
    }
    #[getter]
    fn offset(&self, py: Python<'_>) -> PyResult<Option<usize>> {
        self.native_stage(py)?;
        match &self.state {
            RepeaterWriteState::Owned(v) => Ok(match v {
                OwnedRepeaterWrite::Body { offset, .. } => Some(*offset),
                _ => None,
            }),
            RepeaterWriteState::Cleared => Err(cleared_write()),
            RepeaterWriteState::Body { offset, .. } => offset.with_ref(py, |v| Ok(Some(*v))),
        }
    }
    #[getter]
    fn bytes(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self.native_stage(py)?;
        match &self.state {
            RepeaterWriteState::Owned(v) => Ok(match v {
                OwnedRepeaterWrite::Body { bytes, .. } => Some(bytes.clone_ref(py).into_any()),
                _ => None,
            }),
            RepeaterWriteState::Cleared => Err(cleared_write()),
            RepeaterWriteState::Body { bytes, .. } => Ok(Some(
                Py::new(py, crate::csv::nls::Bytes::from_loan(bytes.clone()))?.into_any(),
            )),
        }
    }
    #[getter]
    fn gateway(&self, py: Python<'_>) -> PyResult<Option<Py<PyBytes>>> {
        self.native_stage(py)?;
        match &self.state {
            RepeaterWriteState::Owned(v) => Ok(match v {
                OwnedRepeaterWrite::Gateway(bytes) => Some(bytes.clone_ref(py)),
                _ => None,
            }),
            _ => Ok(None),
        }
    }
    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<Option<u32>> {
        self.native_stage(py)?;
        match &self.state {
            RepeaterWriteState::Owned(v) => Ok(match v {
                OwnedRepeaterWrite::Use(value) => Some(*value),
                _ => None,
            }),
            _ => Ok(None),
        }
    }
    fn replace(&mut self, py: Python<'_>, other: &RepeaterWrite) -> PyResult<()> {
        let RepeaterWriteState::Owned(_) = &self.state else {
            self.native_stage(py)?;
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "scoped RepeaterWrite is read-only",
            ));
        };
        let RepeaterWriteState::Owned(source) = &other.state else {
            other.native_stage(py)?;
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "replacement requires an ordinary RepeaterWrite",
            ));
        };
        let value = match source {
            OwnedRepeaterWrite::Body {
                stage,
                offset,
                bytes,
            } => OwnedRepeaterWrite::Body {
                stage: *stage,
                offset: *offset,
                bytes: bytes.clone_ref(py),
            },
            OwnedRepeaterWrite::Gateway(bytes) => OwnedRepeaterWrite::Gateway(bytes.clone_ref(py)),
            OwnedRepeaterWrite::Use(value) => OwnedRepeaterWrite::Use(*value),
        };
        self.state = RepeaterWriteState::Owned(value);
        Ok(())
    }
}
pub fn with_repeater_write<R>(
    py: Python<'_>,
    value: core::RepeaterWrite<'_>,
    f: impl FnOnce(Py<RepeaterWrite>) -> PyResult<R>,
) -> PyResult<R> {
    match value {
        core::RepeaterWrite::Gateway(bytes) => f(Py::new(
            py,
            RepeaterWrite::owned(OwnedRepeaterWrite::Gateway(
                PyBytes::new(py, &bytes).unbind(),
            )),
        )?),
        core::RepeaterWrite::Use(value) => f(Py::new(
            py,
            RepeaterWrite::owned(OwnedRepeaterWrite::Use(value)),
        )?),
        core::RepeaterWrite::Body {
            stage,
            offset,
            bytes,
        } => crate::scoped::Frame::with_ref(py, &stage, |stage| {
            crate::scoped::Frame::with_ref(py, &offset, |offset| {
                crate::scoped::Frame::with_ref(py, bytes, |bytes| {
                    f(Py::new(
                        py,
                        RepeaterWrite {
                            state: RepeaterWriteState::Body {
                                stage,
                                offset,
                                bytes,
                            },
                        },
                    )?)
                })
            })
        }),
    }
}
#[pyclass(module = "cs705.icf.record")]
pub struct ChannelWrite {
    state: WriteState,
}
enum WriteState {
    Owned(OwnedWrite),
    Cleared,
    Scoped {
        stage: crate::scoped::Loan<core::ChannelStage>,
        fields: WriteFields,
    },
}
enum OwnedWrite {
    Body {
        stage: core::ChannelStage,
        offset: usize,
        bytes: Py<PyBytes>,
    },
    Flag {
        stage: core::ChannelStage,
        byte: u8,
    },
}
impl ChannelWrite {
    pub fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::ChannelWrite<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        match &self.state {
            WriteState::Owned(v) => f(match v {
                OwnedWrite::Body {
                    stage,
                    offset,
                    bytes,
                } => core::ChannelWrite::Body {
                    stage: *stage,
                    offset: *offset,
                    bytes: bytes.bind(py).as_bytes(),
                },
                OwnedWrite::Flag { stage, byte } => core::ChannelWrite::Flag {
                    stage: *stage,
                    byte: *byte,
                },
            }),
            WriteState::Cleared => Err(cleared_write()),
            WriteState::Scoped { stage, .. } => {
                stage.with_ref(py, |_| Ok(()))?;
                Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "native payload borrowing requires an ordinary ChannelWrite",
                ))
            }
        }
    }
    fn native_stage(&self, py: Python<'_>) -> PyResult<core::ChannelStage> {
        match &self.state {
            WriteState::Owned(v) => Ok(match v {
                OwnedWrite::Body { stage, .. } | OwnedWrite::Flag { stage, .. } => *stage,
            }),
            WriteState::Cleared => Err(cleared_write()),
            WriteState::Scoped { stage, .. } => stage.with_ref(py, |v| Ok(*v)),
        }
    }
}
#[pymethods]
#[allow(non_snake_case)]
impl ChannelWrite {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let WriteState::Owned(OwnedWrite::Body { bytes, .. }) = &self.state {
            visit.call(bytes)?;
        }
        Ok(())
    }
    fn __clear__(&mut self) {
        self.state = WriteState::Cleared;
    }
    #[staticmethod]
    fn Body(
        stage: &Bound<'_, PyAny>,
        offset: &Bound<'_, PyAny>,
        bytes: Py<PyBytes>,
    ) -> PyResult<Self> {
        use super::values::Input;
        Ok(Self {
            state: WriteState::Owned(OwnedWrite::Body {
                stage: core::ChannelStage::input(stage)?,
                offset: strict_usize(offset)?,
                bytes,
            }),
        })
    }
    #[staticmethod]
    fn Flag(stage: &Bound<'_, PyAny>, byte: &Bound<'_, PyAny>) -> PyResult<Self> {
        use super::values::Input;
        Ok(Self {
            state: WriteState::Owned(OwnedWrite::Flag {
                stage: core::ChannelStage::input(stage)?,
                byte: strict_u8(byte)?,
            }),
        })
    }
    fn replace(&mut self, py: Python<'_>, other: &ChannelWrite) -> PyResult<()> {
        let WriteState::Owned(_) = &self.state else {
            self.native_stage(py)?;
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "scoped ChannelWrite is read-only",
            ));
        };
        let WriteState::Owned(source) = &other.state else {
            other.native_stage(py)?;
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "replacement requires an ordinary ChannelWrite",
            ));
        };
        let value = match source {
            OwnedWrite::Body {
                stage,
                offset,
                bytes,
            } => OwnedWrite::Body {
                stage: *stage,
                offset: *offset,
                bytes: bytes.clone_ref(py),
            },
            OwnedWrite::Flag { stage, byte } => OwnedWrite::Flag {
                stage: *stage,
                byte: *byte,
            },
        };
        self.state = WriteState::Owned(value);
        Ok(())
    }
    #[getter]
    fn variant(&self, py: Python<'_>) -> PyResult<&'static str> {
        self.native_stage(py)?;
        match &self.state {
            WriteState::Owned(v) => Ok(match v {
                OwnedWrite::Body { .. } => "Body",
                OwnedWrite::Flag { .. } => "Flag",
            }),
            WriteState::Cleared => Err(cleared_write()),
            WriteState::Scoped { fields, .. } => Ok(match fields {
                WriteFields::Body { .. } => "Body",
                WriteFields::Flag { .. } => "Flag",
            }),
        }
    }
    fn stage<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        super::enum_result(
            py,
            "cs705.icf.record",
            "ChannelStage",
            self.native_stage(py)? as i64,
        )
    }
    #[getter]
    fn offset(&self, py: Python<'_>) -> PyResult<Option<usize>> {
        self.native_stage(py)?;
        match &self.state {
            WriteState::Owned(v) => Ok(match v {
                OwnedWrite::Body { offset, .. } => Some(*offset),
                _ => None,
            }),
            WriteState::Scoped {
                fields: WriteFields::Body { offset, .. },
                ..
            } => offset.with_ref(py, |v| Ok(Some(*v))),
            _ => Ok(None),
        }
    }
    #[getter]
    fn bytes(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self.native_stage(py)?;
        match &self.state {
            WriteState::Owned(v) => Ok(match v {
                OwnedWrite::Body { bytes, .. } => Some(bytes.clone_ref(py).into_any()),
                _ => None,
            }),
            WriteState::Scoped {
                fields: WriteFields::Body { bytes, .. },
                ..
            } => {
                bytes.with_ref(py, |_| Ok(()))?;
                Ok(Some(
                    Py::new(py, crate::csv::nls::Bytes::from_loan(bytes.clone()))?.into_any(),
                ))
            }
            _ => Ok(None),
        }
    }
    #[getter]
    fn byte(&self, py: Python<'_>) -> PyResult<Option<u8>> {
        self.native_stage(py)?;
        match &self.state {
            WriteState::Owned(v) => Ok(match v {
                OwnedWrite::Flag { byte, .. } => Some(*byte),
                _ => None,
            }),
            WriteState::Scoped {
                fields: WriteFields::Flag { byte },
                ..
            } => byte.with_ref(py, |v| Ok(Some(*v))),
            _ => Ok(None),
        }
    }
}
pub fn with_channel_write<R>(
    py: Python<'_>,
    value: &core::ChannelWrite<'_>,
    f: impl FnOnce(Py<ChannelWrite>) -> PyResult<R>,
) -> PyResult<R> {
    use crate::scoped::Frame;
    match value {
        core::ChannelWrite::Body {
            stage,
            offset,
            bytes,
        } => Frame::with_ref(py, stage, |stage| {
            Frame::with_ref(py, offset, |offset| {
                Frame::with_ref(py, *bytes, |bytes| {
                    f(Py::new(
                        py,
                        ChannelWrite {
                            state: WriteState::Scoped {
                                stage,
                                fields: WriteFields::Body { offset, bytes },
                            },
                        },
                    )?)
                })
            })
        }),
        core::ChannelWrite::Flag { stage, byte } => Frame::with_ref(py, stage, |stage| {
            Frame::with_ref(py, byte, |byte| {
                f(Py::new(
                    py,
                    ChannelWrite {
                        state: WriteState::Scoped {
                            stage,
                            fields: WriteFields::Flag { byte },
                        },
                    },
                )?)
            })
        }),
    }
}
use pyo3::types::PyBytes;

macro_rules! enums {
    ($($name:ident { $($variant:ident),* };)*) => {
        $(impl super::values::Input<'_> for core::$name {
            fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
                let code = super::enum_value(value,"cs705.icf.record",stringify!($name))?;
                $(if code == Self::$variant as i64 {return Ok(Self::$variant);})*
                Err(pyo3::exceptions::PyValueError::new_err("invalid record enum"))
            }
        }
        impl super::values::Output for core::$name {
            fn output<'py>(self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> {super::enum_result(py,"cs705.icf.record",stringify!($name),self as i64)}
        }
        impl super::values::Capture for core::$name {
            type Owned=Self;fn capture(self)->Self{self}
        })*
        fn register_enums(m:&Bound<'_,PyModule>)->PyResult<()> {
            $(super::add_enum(m,stringify!($name),&[$((stringify!($variant),core::$name::$variant as i64)),*])?;)*
            Ok(())
        }
    }
}
enums! {
    ChannelField { FrequencyHz, Duplex, Selection, Tone, Dtcs, Polarity, DigitalSquelch, DigitalCode };
    RepeaterField { Mode, Tone, ToneIndex, Use, Position, UtcOffset, GatewayIndex };
    RepeaterText { SubName, Call, Gateway };
    ChannelStage { Empty, Name, Select, Split, Frequency, Mode, Data, Filter, Duplex, Offset, Tone, TxTone, RxTone, Dtcs, TxPolarity, RxPolarity, DigitalSquelch, DigitalCode, YourCall, Repeater1, Repeater2, CorrectRepeater1 };
    RepeaterStage { Name, SubName, Type, RepeaterCall, GatewayCall, ToneIndex, Frequency, Offset, Duplex, Tone, Use, Position, Latitude, South, Longitude, West, UtcOffset, GatewayIndex };
}

#[pyclass(module = "cs705.icf.record", frozen)]
pub struct DecodedRecord {
    state: State,
}
enum State {
    Owned(Owner<core::DecodedRecord>),
    Scoped(crate::scoped::Loan<core::DecodedRecord>),
    DialogField {
        parent: super::callsign_dialog::DialogOwner,
        other: bool,
    },
}
impl DecodedRecord {
    pub fn take(&self) -> PyResult<crate::owner::Taken<'_, core::DecodedRecord>> {
        match &self.state {
            State::Owned(owner) => owner.take(),
            State::Scoped(_) => Err(pyo3::exceptions::PyRuntimeError::new_err(
                "this operation requires an ordinary record owner, not a scoped view",
            )),
            State::DialogField { .. } => Err(pyo3::exceptions::PyRuntimeError::new_err(
                "this operation requires an ordinary record owner, not a dialog field view",
            )),
        }
    }
    pub(crate) fn from_dialog(parent: super::callsign_dialog::DialogOwner, other: bool) -> Self {
        Self {
            state: State::DialogField { parent, other },
        }
    }
    pub fn from_core(value: core::DecodedRecord) -> Self {
        Self {
            state: State::Owned(Owner::new(value)),
        }
    }
    pub fn from_loan(loan: crate::scoped::Loan<core::DecodedRecord>) -> Self {
        Self {
            state: State::Scoped(loan),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::DecodedRecord) -> PyResult<R>) -> PyResult<R> {
        match &self.state {
            State::Owned(owner) => owner.with_ref(f),
            State::Scoped(loan) => Python::attach(|py| loan.with_ref(py, f)),
            State::DialogField { parent, other } => parent.with_ref(|dialog| {
                f(if *other {
                    &dialog.other
                } else {
                    &dialog.holder
                })
            }),
        }
    }
    pub fn with_mut<R>(
        &self,
        f: impl FnOnce(&mut core::DecodedRecord) -> PyResult<R>,
    ) -> PyResult<R> {
        match &self.state {
            State::Owned(owner) => owner.with_mut(f),
            State::Scoped(loan) => Python::attach(|py| loan.with_mut(py, f)),
            State::DialogField { parent, other } => parent.with_mut(|dialog| {
                f(if *other {
                    &mut dialog.other
                } else {
                    &mut dialog.holder
                })
            }),
        }
    }
}
super::direct::direct_methods! {
DecodedRecord
existing {
    fn as_mut_bytes(slf: Py<Self>) -> PyResult<buffer::RecordBytes> { buffer::RecordBytes::from_parent(slf) }
    #[staticmethod]
    fn from_bytes(bytes: &Bound<'_, PyAny>) -> PyResult<Self> { Ok(Self::from_core(core::DecodedRecord::from_bytes(fixed_bytes(bytes)?))) }
    fn as_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> { self.with_ref(|h| Ok(PyBytes::new(py, h.as_bytes()))) }
    fn copy_body_from(&self, source: &DecodedRecord) -> PyResult<()> { self.with_mut(|h| source.with_ref(|s| { h.copy_body_from(s); Ok(()) })) }
}
methods {
    history_call() [with_ref, plain];
    history_supplemented() [with_ref, plain];
    kind() [with_ref, plain];
    index() [with_ref, plain];
    binding() [with_ref, plain];
    set_header(kind: u32, index: u32, binding: u32) [with_mut, plain];
    valid() [with_ref, plain];
    set_valid(valid: bool) [with_mut, plain];
    group() [with_ref, plain];
    set_group(group: u32) [with_mut, plain];
    range_index() [with_ref, plain];
    mode() [with_ref, plain];
    set_mode(mode: u32) [with_mut, plain];
    data() [with_ref, plain];
    set_data(data: u32) [with_mut, plain];
    filter() [with_ref, plain];
    set_filter(filter: u32) [with_mut, plain];
    offset_hz() [with_ref, plain];
    set_offset_hz(hz: u32) [with_mut, plain];
    tx_tone() [with_ref, plain];
    set_tx_tone(tone: u32) [with_mut, plain];
    rx_tone() [with_ref, plain];
    set_rx_tone(tone: u32) [with_mut, plain];
    name() [with_ref, plain];
    set_name(name: &[u8;16]) [with_mut, plain];
    side() [with_ref, plain];
    split() [with_ref, plain];
    set_split(split: u8) [with_mut, plain];
    coordinate_bytes(longitude: bool) [with_ref, plain];
    set_coordinate_bytes(longitude: bool, value: &[u8;4]) [with_mut, plain];
    callsign(which: MemoryCallsign) [with_ref, plain];
    set_callsign(which: MemoryCallsign, call: &[u8;8]) [with_mut, plain];
    channel_field(field: ChannelField) [with_ref, plain];
    set_channel_field(field: ChannelField, value: u32) [with_mut, plain];
    repeater_field(field: RepeaterField) [with_ref, plain];
    set_repeater_field(field: RepeaterField, value: u32) [with_mut, plain];
    repeater_text(field: RepeaterText) [with_ref, plain];
    set_repeater_text(field: RepeaterText, value: &[u8;8]) [with_mut, plain];
}
}
#[pyfunction]
fn reset_holder(
    kind: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    binding: &Bound<'_, PyAny>,
) -> PyResult<DecodedRecord> {
    Ok(DecodedRecord::from_core(core::reset_holder(
        strict_u32(kind)?,
        strict_u32(index)?,
        strict_u32(binding)?,
    )))
}
#[pyfunction]
fn decode_mode(raw: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(core::decode_mode(strict_u8(raw)?))
}
#[pyfunction]
fn encode_mode(decoded: &Bound<'_, PyAny>) -> PyResult<u8> {
    Ok(core::encode_mode(strict_u32(decoded)?))
}
#[pyfunction]
fn decode_tone(raw: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(core::decode_tone(strict_u8(raw)?))
}
#[pyfunction]
fn encode_tone(decoded: &Bound<'_, PyAny>) -> PyResult<u8> {
    Ok(core::encode_tone(strict_u32(decoded)?))
}
#[pyfunction]
fn decode_digital_squelch(raw: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(core::decode_digital_squelch(strict_u8(raw)?))
}
#[pyfunction]
fn encode_digital_squelch(decoded: &Bound<'_, PyAny>) -> PyResult<u8> {
    Ok(core::encode_digital_squelch(strict_u32(decoded)?))
}
#[pyfunction]
fn compact_values(holder: &DecodedRecord) -> PyResult<super::special::CompactChannel> {
    holder.with_ref(|h| {
        Ok(super::special::CompactChannel::from_core(
            core::compact_values(h),
        ))
    })
}
#[pyfunction]
fn scan_values(holder: &DecodedRecord) -> PyResult<super::special::ScanEdge> {
    holder.with_ref(|h| Ok(super::special::ScanEdge::from_core(core::scan_values(h))))
}
#[pyfunction]
fn decode_compact_into(
    holder: &DecodedRecord,
    source: &super::special::CompactChannel,
    split: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let split = strict_u8(split)?;
    holder.with_mut(|h| {
        source.with_ref(|s| {
            core::decode_compact_into(h, s, split);
            Ok(())
        })
    })
}
#[pyfunction]
fn decode_scan_into(holder: &DecodedRecord, source: &super::special::ScanEdge) -> PyResult<()> {
    holder.with_mut(|h| {
        source.with_ref(|s| {
            core::decode_scan_into(h, s);
            Ok(())
        })
    })
}
macro_rules! decoders {
    ($($name:ident($($arg:ident:$ty:ty),*) [$result:ident];)*)=>{$(
        #[pyfunction]
        fn $name(holder:&DecodedRecord,$($arg:&Bound<'_,PyAny>),*)->PyResult<()>{
            $(let $arg=<$ty as super::values::Input>::input($arg)?;)*
            holder.with_mut(|h|{super::direct::native_result!($result,core::$name(h,$($arg),*));Ok(())})
        }
    )*};
}
decoders! {
    decode_channel_into(raw:&[u8;83],side:usize,normal_flag:Option<u8>) [fallible];
    decode_gps_into(raw:&[u8;34],group:u32) [plain];
    decode_callsign_into(name:&[u8;16],call:&[u8;8]) [plain];
    decode_pair_into(raw:&[u8;8],kind:u32) [fallible];
    decode_mode_pair_into(raw:&[u8;10]) [plain];
    decode_repeater_into(raw:&[u8;52],gateway:&[u8;7],use_value:u32,group:u32) [plain];
    decode_history_base_into(kind:u32,raw:Option<&[u8]>) [fallible];
    decode_receive_history_into(basic:&[u8;48],message:&[u8;20],state:u8,gps:&[u8;92]) [plain];
}
macro_rules! encoders {
    ($($name:ident [$width:literal,$access:ident] ($($arg:ident:$ty:ty),*);)*)=>{$(
        #[pyfunction]
        fn $name(py:Python<'_>,holder:&DecodedRecord,raw:Py<MutableBytes>,$($arg:&Bound<'_,PyAny>,)*write:Py<PyAny>)->PyResult<()>{
            $(let $arg=<$ty as super::values::Input>::input($arg)?;)*
            let mut bytes=raw.get().take()?;
            let raw:&mut[u8;$width]=bytes.as_mut_slice().try_into().map_err(|_|pyo3::exceptions::PyValueError::new_err(concat!("expected exactly ",stringify!($width)," bytes")))?;
            let mut failures=crate::callbacks::CallbackError::default();
            let result=holder.$access(|h|core::$name(h,raw,$($arg,)*|at,value|failures.capture(write.bind(py).call1((at,PyBytes::new(py,value))).and_then(super::document::adapters::require_none)).map_err(|_|cs705_core::icf::Error::Operation("Python record writer failed".into()))).map_err(crate::errors::icf));
            failures.finish(py,result)
        }
    )*};
}
encoders! {
    encode_gps_into [34,with_ref] ();
    encode_pair_into [8,with_mut] (kind:u32);
    encode_mode_pair_into [10,with_mut] ();
}
#[pyfunction]
fn mode_filter_class(mode: &Bound<'_, PyAny>, data: &Bound<'_, PyAny>) -> PyResult<u8> {
    Ok(core::mode_filter_class(
        strict_u32(mode)?,
        strict_u32(data)?,
    ))
}
#[pyfunction]
fn channel_mode_bad(holder: &DecodedRecord) -> PyResult<bool> {
    holder.with_ref(|h| Ok(core::channel_mode_bad(h)))
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "record")?;
    m.add_class::<ChannelWrite>()?;
    m.add_class::<CallsignWrite>()?;
    m.add_class::<RepeaterWrite>()?;
    m.add_function(wrap_pyfunction!(encode_callsign_into, &m)?)?;
    m.add_function(wrap_pyfunction!(encode_channel_into, &m)?)?;
    m.add_function(wrap_pyfunction!(encode_repeater_into, &m)?)?;
    m.add_function(wrap_pyfunction!(load_callsign_storage_into, &m)?)?;
    m.add_function(wrap_pyfunction!(store_callsign_storage_into, &m)?)?;
    m.add_class::<DecodedRecord>()?;
    buffer::register(&m)?;
    history::register(&m)?;
    storage::register(&m)?;
    register_enums(&m)?;
    m.add("DECODED_BYTES", core::DECODED_BYTES)?;
    m.add("BODY_BYTES", core::BODY_BYTES)?;
    m.add_function(wrap_pyfunction!(reset_holder, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_mode, &m)?)?;
    m.add_function(wrap_pyfunction!(encode_mode, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_tone, &m)?)?;
    m.add_function(wrap_pyfunction!(encode_tone, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_digital_squelch, &m)?)?;
    m.add_function(wrap_pyfunction!(encode_digital_squelch, &m)?)?;
    m.add_function(wrap_pyfunction!(compact_values, &m)?)?;
    m.add_function(wrap_pyfunction!(scan_values, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_compact_into, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_scan_into, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_channel_into, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_gps_into, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_callsign_into, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_pair_into, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_mode_pair_into, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_repeater_into, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_history_base_into, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_receive_history_into, &m)?)?;
    m.add_function(wrap_pyfunction!(encode_gps_into, &m)?)?;
    m.add_function(wrap_pyfunction!(encode_pair_into, &m)?)?;
    m.add_function(wrap_pyfunction!(encode_mode_pair_into, &m)?)?;
    m.add_function(wrap_pyfunction!(mode_filter_class, &m)?)?;
    m.add_function(wrap_pyfunction!(channel_mode_bad, &m)?)?;
    parent.add_submodule(&m)
}

#[pyfunction]
fn encode_callsign_into(
    py: Python<'_>,
    holder: Py<DecodedRecord>,
    write: Py<PyAny>,
) -> PyResult<()> {
    let holder = holder.get().take()?;
    let mut failures = crate::callbacks::CallbackError::default();
    let result = core::encode_callsign_into(&holder, |event| {
        let result = with_callsign_write(py, &event, |event| {
            super::document::adapters::require_none(write.bind(py).call1((event,))?)
        });
        failures
            .capture(result)
            .map_err(|_| cs705_core::icf::Error::Operation("Python callsign writer failed".into()))
    })
    .map_err(crate::errors::icf);
    failures.finish(py, result)
}
#[pyfunction]
#[pyo3(signature = (holder, raw, side, normal_flag, write))]
fn encode_channel_into(
    py: Python<'_>,
    holder: Py<DecodedRecord>,
    raw: Py<MutableBytes>,
    side: &Bound<'_, PyAny>,
    normal_flag: Option<&MutableBytes>,
    write: Py<PyAny>,
) -> PyResult<()> {
    let side = strict_usize(side)?;
    let mut raw = raw.get().take()?;
    let raw: &mut [u8; 83] = raw
        .as_mut_slice()
        .try_into()
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("expected exactly 83 bytes"))?;
    let mut flag = normal_flag.map(MutableBytes::take).transpose()?;
    let flag = match flag.as_mut() {
        None => None,
        Some(flag) => {
            let [byte] = flag.as_mut_slice() else {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "normal_flag requires one byte",
                ));
            };
            Some(byte)
        }
    };
    let mut holder = holder.get().take()?;
    let mut failures = crate::callbacks::CallbackError::default();
    let result = core::encode_channel_into(&mut holder, raw, side, flag, |event| {
        let result = with_channel_write(py, &event, |event| {
            super::document::adapters::require_none(write.bind(py).call1((event,))?)
        });
        failures.capture(result).map_err(|_| {
            cs705_core::icf::Error::Operation("Python raw channel writer failed".into())
        })
    })
    .map_err(crate::errors::icf);
    failures.finish(py, result)
}
#[pyfunction]
fn load_callsign_storage_into(
    image: &super::Icf,
    index: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
) -> PyResult<()> {
    let index = strict_usize(index)?;
    image.with_ref(|image| {
        holder.with_mut(|holder| {
            core::load_callsign_storage_into(image, index, holder).map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
fn store_callsign_storage_into(
    image: &super::Icf,
    index: &Bound<'_, PyAny>,
    holder: &DecodedRecord,
) -> PyResult<()> {
    let index = strict_usize(index)?;
    image.with_mut(|image| {
        holder.with_ref(|holder| {
            core::store_callsign_storage_into(image, index, holder).map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
fn encode_repeater_into(
    py: Python<'_>,
    holder: Py<DecodedRecord>,
    raw: Py<MutableBytes>,
    write: Py<PyAny>,
) -> PyResult<()> {
    let holder = holder.get().take()?;
    let mut raw = raw.get().take()?;
    let raw: &mut [u8; 52] = raw
        .as_mut_slice()
        .try_into()
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("expected exactly 52 bytes"))?;
    let mut failures = crate::callbacks::CallbackError::default();
    let result = core::encode_repeater_into(&holder, raw, |event| {
        let result = with_repeater_write(py, event, |event| {
            super::document::adapters::require_none(write.bind(py).call1((event,))?)
        });
        failures.capture(result).map_err(|_| {
            cs705_core::icf::Error::Operation("Python raw repeater writer failed".into())
        })
    })
    .map_err(crate::errors::icf);
    failures.finish(py, result)
}
