//! Consuming sessions with synchronous, detached Python transport adapters.
use super::*;
use crate::{callbacks::CallbackError, owner::Owner};
use pyo3::exceptions::{PyTypeError, PyValueError};
use std::{
    cell::RefCell,
    io::{self, Read, Write},
};

macro_rules! io_kinds {
    ($($name:ident),* $(,)?) => {
        const IO_KINDS: &[io::ErrorKind] = &[$(io::ErrorKind::$name),*];
        const IO_NAMES: &[&str] = &[$(stringify!($name)),*];
    }
}
io_kinds!(
    NotFound,
    PermissionDenied,
    ConnectionRefused,
    ConnectionReset,
    HostUnreachable,
    NetworkUnreachable,
    ConnectionAborted,
    NotConnected,
    AddrInUse,
    AddrNotAvailable,
    NetworkDown,
    BrokenPipe,
    AlreadyExists,
    WouldBlock,
    NotADirectory,
    IsADirectory,
    DirectoryNotEmpty,
    ReadOnlyFilesystem,
    StaleNetworkFileHandle,
    InvalidInput,
    InvalidData,
    TimedOut,
    WriteZero,
    StorageFull,
    NotSeekable,
    QuotaExceeded,
    FileTooLarge,
    ResourceBusy,
    ExecutableFileBusy,
    Deadlock,
    CrossesDevices,
    TooManyLinks,
    InvalidFilename,
    ArgumentListTooLong,
    Interrupted,
    Unsupported,
    UnexpectedEof,
    OutOfMemory,
    Other
);
fn io_in(value: &Bound<'_, PyAny>) -> PyResult<io::ErrorKind> {
    IO_KINDS
        .get(enum_in(value, "cs705.clone", "ErrorKind")?)
        .copied()
        .ok_or_else(|| PyValueError::new_err("invalid ErrorKind"))
}
fn io_out<'py>(py: Python<'py>, kind: io::ErrorKind) -> PyResult<Bound<'py, PyAny>> {
    let index = IO_KINDS
        .iter()
        .position(|k| *k == kind)
        .ok_or_else(|| PyValueError::new_err(format!("unsupported native ErrorKind: {kind:?}")))?;
    enum_out(py, "cs705.clone", "ErrorKind", index)
}
pub(crate) fn echo_in(value: &Bound<'_, PyAny>) -> PyResult<core::Echo> {
    Ok(match enum_in(value, "cs705.clone", "Echo")? {
        0 => core::Echo::None,
        1 => core::Echo::Known,
        2 => core::Echo::Auto,
        _ => return Err(PyValueError::new_err("invalid Echo")),
    })
}

macro_rules! settings_impl {
    ($($field:ident,$set:ident:$ty:ty=>$conv:ident);* $(;)?) => {
        #[pymethods] impl TransportSettings {
            fn __copy__(&self)->PyResult<Self>{self.inner.with(|v|Ok(Self{inner:Handle::new(v.clone())}))}
            fn __deepcopy__(&self,_memo:&Bound<'_,PyAny>)->PyResult<Self>{self.__copy__()}
            #[new] fn new()->Self{Self{inner:Handle::new(core::TransportSettings::default())}}
            $(#[getter] fn $field(&self)->PyResult<$ty>{self.inner.with(|v|Ok(v.$field))}
            #[setter($field)] fn $set(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=$conv(v)?;self.inner.with(|v|{v.$field=n;Ok(())})})*
            #[getter] fn echo<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{self.inner.with(|v|enum_out(py,"cs705.clone","Echo",v.echo as usize))}
            #[setter] fn set_echo(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=echo_in(v)?;self.inner.with(|v|{v.echo=n;Ok(())})}
            fn __richcmp__(&self,other:&Self,op:pyo3::basic::CompareOp,py:Python<'_>)->PyResult<Py<PyAny>>{
                let value=self.inner.with(|v|Ok(v.clone()))?;
                let equal=other.inner.with(|v|Ok(value==*v))?;
                Ok(match op{pyo3::basic::CompareOp::Eq=>pyo3::types::PyBool::new(py,equal).to_owned().into_any().unbind(),pyo3::basic::CompareOp::Ne=>pyo3::types::PyBool::new(py,!equal).to_owned().into_any().unbind(),_=>py.NotImplemented()})
            }
        }
    }
}
#[pyclass(module = "cs705.clone", frozen)]
#[derive(Clone)]
pub struct TransportSettings {
    pub(crate) inner: Handle<core::TransportSettings>,
}
settings_impl! {
    baud,set_baud:u32=>strict_u32;data_bits,set_data_bits:u8=>strict_u8;
    parity,set_parity:u8=>strict_u8;stop_bits,set_stop_bits:u8=>strict_u8;
    dtr_control,set_dtr_control:u8=>strict_u8;rts_control,set_rts_control:u8=>strict_u8;
    extra_preamble,set_extra_preamble:usize=>strict_usize;data_preamble,set_data_preamble:usize=>strict_usize;
}
macro_rules! config_impl {
    ($($field:ident,$set:ident:$ty:ty=>$conv:ident);* $(;)?)=>{
        #[pymethods] impl SessionConfig {
            fn __copy__(&self)->PyResult<Self>{self.inner.with(|v|Ok(Self{inner:Handle::new(v.clone())}))}
            fn __deepcopy__(&self,_memo:&Bound<'_,PyAny>)->PyResult<Self>{self.__copy__()}
            #[new] fn new()->Self{Self{inner:Handle::new(core::SessionConfig::default())}}
            $(#[getter] fn $field(&self)->PyResult<$ty>{self.inner.with(|v|Ok(v.$field))}
              #[setter($field)] fn $set(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=$conv(v)?;self.inner.with(|v|{v.$field=n;Ok(())})})*
            #[getter] fn settings(&self)->TransportSettings{TransportSettings{inner:self.inner.project(|v|Ok(&mut v.settings))}}
            #[setter] fn set_settings(&self,value:&TransportSettings)->PyResult<()>{let n=value.inner.with(|v|Ok(v.clone()))?;self.inner.edit(|v|{v.settings=n;Ok(())})}
            #[getter] fn startup_script<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyBytes>>{self.inner.with(|v|Ok(PyBytes::new(py,&v.startup_script)))}
            #[setter] fn set_startup_script(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_bytes(value)?.to_vec();self.inner.with(|v|{v.startup_script=n;Ok(())})}
            #[getter] fn expected_context(&self)->PyResult<Option<ContextFields>>{self.inner.with(|v|Ok(v.expected_context.map(|_|ContextFields{inner:self.inner.project(|v|v.expected_context.as_mut().ok_or_else(||pyo3::exceptions::PyReferenceError::new_err("context was removed")))})))}
            #[setter] fn set_expected_context(&self,value:Option<&ContextFields>)->PyResult<()>{let n=value.map(|v|v.inner.with(|v|Ok(*v))).transpose()?;self.inner.edit(|v|{v.expected_context=n;Ok(())})}
            fn set_block_cap(&self,cap:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_i32(cap)?;self.inner.with(|v|{v.set_block_cap(n);Ok(())})}
        }
    }
}
#[pyclass(module = "cs705.clone", frozen)]
#[derive(Clone)]
/// Own configuration; settings and expected_context are live nested views.
pub struct SessionConfig {
    pub(crate) inner: Handle<core::SessionConfig>,
}
config_impl! {
    block_cap,set_block_cap_field:i32=>strict_i32;counted_end,set_counted_end:bool=>strict_bool;
    timeout_ms,set_timeout_ms:u64=>strict_u64;ack_timeout_ms,set_ack_timeout_ms:u64=>strict_u64;
    drain_timeout_ms,set_drain_timeout_ms:u64=>strict_u64;prepare_delay_ms,set_prepare_delay_ms:u64=>strict_u64;
    range_gap_ms,set_range_gap_ms:u64=>strict_u64;poll_ms,set_poll_ms:u64=>strict_u64;
    saved_selector,set_saved_selector:u8=>strict_u8;flag57,set_flag57:u8=>strict_u8;
}

#[pyclass(module = "cs705.clone", frozen)]
#[derive(Clone)]
/// Immutable factory-created error value; exception() returns without raising.
pub struct ErrorData {
    pub(crate) inner: core::Error,
}
#[pymethods]
#[allow(non_snake_case)]
impl ErrorData {
    #[staticmethod]
    fn Invalid(reason: &Bound<'_, PyAny>) -> PyResult<Self> {
        let reason = reason
            .cast::<pyo3::types::PyString>()?
            .extract::<String>()?;
        Ok(Self {
            inner: core::Error::Invalid(std::borrow::Cow::Owned(reason)),
        })
    }
    #[staticmethod]
    fn UnsupportedCommand(command: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Error::UnsupportedCommand(strict_u8(command)?),
        })
    }
    #[staticmethod]
    fn ChecksumMismatch() -> Self {
        Self {
            inner: core::Error::ChecksumMismatch,
        }
    }
    #[staticmethod]
    fn AddressOverflow() -> Self {
        Self {
            inner: core::Error::AddressOverflow,
        }
    }
    #[staticmethod]
    fn SourceBounds() -> Self {
        Self {
            inner: core::Error::SourceBounds,
        }
    }
    #[staticmethod]
    fn Rejected(status: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Error::Rejected(strict_u8(status)?),
        })
    }
    fn __copy__(&self) -> Self {
        self.clone()
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> Self {
        self.clone()
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self.inner {
            core::Error::Invalid(_) => "Invalid",
            core::Error::UnsupportedCommand(_) => "UnsupportedCommand",
            core::Error::ChecksumMismatch => "ChecksumMismatch",
            core::Error::AddressOverflow => "AddressOverflow",
            core::Error::SourceBounds => "SourceBounds",
            core::Error::Rejected(_) => "Rejected",
        }
    }
    #[getter]
    fn reason(&self) -> Option<&str> {
        match &self.inner {
            core::Error::Invalid(v) => Some(v.as_ref()),
            _ => None,
        }
    }
    #[getter]
    fn command(&self) -> Option<u8> {
        match self.inner {
            core::Error::UnsupportedCommand(v) => Some(v),
            _ => None,
        }
    }
    #[getter]
    fn status(&self) -> Option<u8> {
        match self.inner {
            core::Error::Rejected(v) => Some(v),
            _ => None,
        }
    }
    fn exception(&self, py: Python<'_>) -> Py<PyAny> {
        errors::clone(self.inner.clone()).into_value(py).into_any()
    }
    fn __str__(&self) -> String {
        self.inner.to_string()
    }
}
#[pyclass(module = "cs705.clone", frozen)]
#[derive(Clone)]
/// Immutable failure value; with_* methods return replacements, not mutations.
pub struct Failure {
    pub(crate) inner: core::Failure,
}
macro_rules! failure_impl {
    ($($name:ident),* $(,)?)=>{
        #[pymethods] #[allow(non_snake_case)] impl Failure {
            fn __copy__(&self)->Self{self.clone()}
            fn __deepcopy__(&self,_memo:&Bound<'_,PyAny>)->Self{self.clone()}
            $(#[staticmethod] fn $name()->Self{Self{inner:core::Failure::$name}})*
            #[staticmethod] fn Protocol(error:&ErrorData)->Self{Self{inner:core::Failure::Protocol(error.inner.clone())}}
            #[staticmethod] fn Identity(code:&Bound<'_,PyAny>)->PyResult<Self>{Ok(Self{inner:core::Failure::Identity(strict_u8(code)?)})}
            #[staticmethod] fn Io(kind:&Bound<'_,PyAny>)->PyResult<Self>{Ok(Self{inner:core::Failure::Io(io_in(kind)?)})}
            #[staticmethod] fn Reconfigure(kind:&Bound<'_,PyAny>)->PyResult<Self>{Ok(Self{inner:core::Failure::Reconfigure(io_in(kind)?)})}
            /// Return a replacement value; this value is not mutated.
            fn with_error(&self,error:&ErrorData)->PyResult<Self>{
                if !matches!(self.inner,core::Failure::Protocol(_)){return Err(PyValueError::new_err("with_error requires Failure.Protocol"));}
                Ok(Self{inner:core::Failure::Protocol(error.inner.clone())})
            }
            /// Return a replacement value; this value is not mutated.
            fn with_code(&self,code:&Bound<'_,PyAny>)->PyResult<Self>{
                if !matches!(self.inner,core::Failure::Identity(_)){return Err(PyValueError::new_err("with_code requires Failure.Identity"));}
                Self::Identity(code)
            }
            /// Return a replacement value preserving Io or Reconfigure.
            fn with_error_kind(&self,kind:&Bound<'_,PyAny>)->PyResult<Self>{
                match self.inner{
                    core::Failure::Io(_)=>Self::Io(kind),
                    core::Failure::Reconfigure(_)=>Self::Reconfigure(kind),
                    _=>Err(PyValueError::new_err("with_error_kind requires Failure.Io or Failure.Reconfigure")),
                }
            }
            #[getter] fn kind(&self)->&'static str{match &self.inner{$(core::Failure::$name=>stringify!($name),)*core::Failure::Protocol(_)=>"Protocol",core::Failure::Identity(_)=>"Identity",core::Failure::Io(_)=>"Io",core::Failure::Reconfigure(_)=>"Reconfigure"}}
            #[getter] fn error(&self)->Option<ErrorData>{match &self.inner{core::Failure::Protocol(v)=>Some(ErrorData{inner:v.clone()}),_=>None}}
            #[getter] fn code(&self)->Option<u8>{match self.inner{core::Failure::Identity(v)=>Some(v),_=>None}}
            #[getter] fn error_kind<'py>(&self,py:Python<'py>)->PyResult<Option<Bound<'py,PyAny>>>{match self.inner{core::Failure::Io(v)|core::Failure::Reconfigure(v)=>Ok(Some(io_out(py,v)?)),_=>Ok(None)}}
            fn result_code(&self)->u8{self.inner.result_code()}
            fn __richcmp__(&self,other:&Self,op:pyo3::basic::CompareOp,py:Python<'_>)->Py<PyAny>{
                match op {
                    pyo3::basic::CompareOp::Eq=>pyo3::types::PyBool::new(py,self.inner==other.inner).to_owned().into_any().unbind(),
                    pyo3::basic::CompareOp::Ne=>pyo3::types::PyBool::new(py,self.inner!=other.inner).to_owned().into_any().unbind(),
                    _=>py.NotImplemented(),
                }
            }
        }
    }
}
failure_impl!(
    MalformedAck,
    Cancelled,
    Timeout,
    EchoTimeout,
    EchoMismatch,
    Truncated,
    EarlyResponse,
    EndCount,
    MissingCoverage,
    ImageValidation,
    OutsideImage,
    OutsideCoverage,
    DrainTimeout,
    ClockRegressed,
    SourceBounds
);

#[pyclass(module = "cs705.clone", frozen)]
#[derive(Clone)]
/// Native-produced failure-prefix report; there is no public constructor.
/// Editing public data cannot forge query proof. Nested identity fields are
/// live views; image and coverage are immutable byte snapshots.
pub struct Outcome {
    pub(crate) inner: Handle<core::Outcome>,
}
impl Outcome {
    pub fn from_core(value: core::Outcome) -> Self {
        Self {
            inner: Handle::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::Outcome) -> PyResult<R>) -> PyResult<R> {
        self.inner.with(|v| f(v))
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut core::Outcome) -> PyResult<R>) -> PyResult<R> {
        self.inner.with(f)
    }
}
macro_rules! outcome_impl {
    ($($field:ident,$set:ident:$ty:ty=>$conv:ident);* $(;)?)=>{
        #[pymethods] impl Outcome {
            fn __copy__(&self)->PyResult<Self>{self.inner.with(|v|Ok(Self::from_core(v.clone())))}
            fn __deepcopy__(&self,_memo:&Bound<'_,PyAny>)->PyResult<Self>{self.__copy__()}
            $(#[getter] fn $field(&self)->PyResult<$ty>{self.inner.with(|v|Ok(v.$field))}
              #[setter] fn $set(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=$conv(v)?;self.inner.with(|v|{v.$field=n;Ok(())})})*
            #[getter] fn operation<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{self.inner.with(|v|enum_out(py,"cs705.clone","Operation",v.operation as usize))}
            #[setter] fn set_operation(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=match enum_in(v,"cs705.clone","Operation")?{0=>core::Operation::Query,1=>core::Operation::Read,2=>core::Operation::Write,_=>return Err(PyValueError::new_err("invalid Operation"))};self.inner.with(|v|{v.operation=n;Ok(())})}
            #[getter] fn echo<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{self.inner.with(|v|enum_out(py,"cs705.clone","Echo",v.echo as usize))}
            #[setter] fn set_echo(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=echo_in(v)?;self.inner.with(|v|{v.echo=n;Ok(())})}
            #[getter] fn write_disposition<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{self.inner.with(|v|enum_out(py,"cs705.clone","WriteDisposition",v.write_disposition as usize))}
            #[setter] fn set_write_disposition(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=match enum_in(v,"cs705.clone","WriteDisposition")?{0=>core::WriteDisposition::NotStarted,1=>core::WriteDisposition::Unconfirmed,2=>core::WriteDisposition::Acknowledged,_=>return Err(PyValueError::new_err("invalid WriteDisposition"))};self.inner.with(|v|{v.write_disposition=n;Ok(())})}
            #[getter] fn terminal_phase<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{self.inner.with(|v|enum_out(py,"cs705.clone","State",v.terminal_phase as usize))}
            #[setter] fn set_terminal_phase(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=enum_in(v,"cs705.clone","State")?;let n=*STATES.get(n).ok_or_else(||PyValueError::new_err("invalid State"))?;self.inner.with(|v|{v.terminal_phase=n;Ok(())})}
            #[getter] fn failure(&self)->PyResult<Option<Failure>>{self.inner.with(|v|Ok(v.failure.clone().map(|inner|Failure{inner})))}
            #[setter] fn set_failure(&self,v:Option<&Failure>)->PyResult<()>{let n=v.map(|v|v.inner.clone());self.inner.with(|v|{v.failure=n;Ok(())})}
            #[getter] fn close_error<'py>(&self,py:Python<'py>)->PyResult<Option<Bound<'py,PyAny>>>{self.inner.with(|v|v.close_error.map(|v|io_out(py,v)).transpose())}
            #[setter] fn set_close_error(&self,v:Option<&Bound<'_,PyAny>>)->PyResult<()>{let n=v.map(io_in).transpose()?;self.inner.with(|v|{v.close_error=n;Ok(())})}
            #[getter] fn identity(&self)->PyResult<Option<Identity>>{self.inner.with(|v|Ok(v.identity.as_ref().map(|_|Identity{inner:self.inner.project(|v|v.identity.as_mut().ok_or_else(||pyo3::exceptions::PyReferenceError::new_err("identity was removed")))})))}
            #[setter] fn set_identity(&self,v:Option<&Identity>)->PyResult<()>{let n=v.map(|v|v.inner.with(|v|Ok(v.clone()))).transpose()?;self.inner.edit(|v|{v.identity=n;Ok(())})}
            #[getter] fn identity_gate(&self)->IdentityGate{IdentityGate{inner:self.inner.project(|v|Ok(&mut v.identity_gate))}}
            #[setter] fn set_identity_gate(&self,v:&IdentityGate)->PyResult<()>{let n=v.inner.with(|v|Ok(v.clone()))?;self.inner.edit(|v|{v.identity_gate=n;Ok(())})}
            #[getter] fn image<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyBytes>>{self.inner.with(|v|Ok(PyBytes::new(py,&v.image)))}
            #[setter] fn set_image(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_bytes(v)?.to_vec();self.inner.with(|v|{v.image=n;Ok(())})}
            #[getter] fn coverage<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyBytes>>{self.inner.with(|v|Ok(PyBytes::new(py,&v.coverage)))}
            #[setter] fn set_coverage(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_bytes(v)?.to_vec();self.inner.with(|v|{v.coverage=n;Ok(())})}
            fn result_code(&self)->PyResult<u8>{self.inner.with(|v|Ok(v.result_code()))}
            fn acquired(&self,address:&Bound<'_,PyAny>)->PyResult<bool>{let n=strict_usize(address)?;self.inner.with(|v|Ok(v.acquired(n)))}
        }
    }
}
const STATES: &[core::State] = &[
    core::State::Startup,
    core::State::QuerySend,
    core::State::QueryWait,
    core::State::ReadSend,
    core::State::ReadWait,
    core::State::WriteStart,
    core::State::WriteData,
    core::State::WriteEnd,
    core::State::AckWait,
    core::State::Drain,
    core::State::Closed,
];
outcome_impl! {blocks,set_blocks:u32=>strict_u32;payload_bytes,set_payload_bytes:u64=>strict_u64;requested_wire_bytes,set_requested_wire_bytes:u64=>strict_u64;transmitted_wire_bytes,set_transmitted_wire_bytes:u64=>strict_u64;received_wire_bytes,set_received_wire_bytes:u64=>strict_u64;range_index,set_range_index:usize=>strict_usize;next_address,set_next_address:u32=>strict_u32;}

pub(crate) struct Adapter<'a> {
    pub object: &'a Py<PyAny>,
    pub failures: &'a RefCell<CallbackError>,
    pub interpreter: i64,
}
impl Adapter<'_> {
    fn call<R>(
        &self,
        cleanup: bool,
        f: impl FnOnce(Python<'_>, &Bound<'_, PyAny>) -> PyResult<R>,
    ) -> io::Result<R> {
        Python::attach(|py| {
            if !cleanup && self.failures.borrow().failed() {
                return Err(io::Error::other("Python transport invocation failed"));
            }
            let result = (|| {
                if crate::scoped::interpreter(py)? != self.interpreter {
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(
                        "transport belongs to another interpreter",
                    ));
                }
                f(py, self.object.bind(py))
            })();
            let kind = result
                .as_ref()
                .err()
                .map(|error| io::Error::from(error.clone_ref(py)).kind());
            let result = if cleanup {
                self.failures.borrow_mut().capture_cleanup(result)
            } else {
                self.failures.borrow_mut().capture(result)
            };
            result.map_err(|_| {
                io::Error::new(
                    kind.expect("captured transport error"),
                    "Python transport callback failed",
                )
            })
        })
    }
    fn unit(
        &self,
        name: &str,
        args: impl for<'py> FnOnce(Python<'py>) -> PyResult<Py<PyAny>>,
        cleanup: bool,
    ) -> io::Result<()> {
        self.call(cleanup, |py, v| {
            let args = args(py)?;
            let args = args.bind(py).cast::<pyo3::types::PyTuple>()?;
            let value = v.call_method(name, args, None)?;
            if !value.is_none() {
                return Err(PyTypeError::new_err(format!("{name} must return None")));
            }
            Ok(())
        })
    }
}
impl Read for Adapter<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.call(false, |_, v| {
            let value = v.call_method1("read", (out.len(),))?;
            let bytes = strict_bytes(&value)?;
            if bytes.len() > out.len() {
                return Err(PyValueError::new_err("read returned more than maximum"));
            }
            out[..bytes.len()].copy_from_slice(bytes);
            Ok(bytes.len())
        })
    }
}
impl Write for Adapter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.call(false, |py, v| {
            let value = v.call_method1("write", (PyBytes::new(py, bytes),))?;
            let count = strict_usize(&value)?;
            if count > bytes.len() {
                return Err(PyValueError::new_err(
                    "write returned an invalid prefix count",
                ));
            }
            Ok(count)
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        self.unit(
            "flush",
            |py| Ok(pyo3::types::PyTuple::empty(py).into_any().unbind()),
            false,
        )
    }
}
impl core::Transport for Adapter<'_> {
    fn now_ms(&self) -> io::Result<u64> {
        self.call(false, |_, v| strict_u64(&v.call_method0("now_ms")?))
    }
    fn wait_ms(&mut self, ms: u64) -> io::Result<()> {
        self.unit(
            "wait_ms",
            |py| Ok(pyo3::types::PyTuple::new(py, [ms])?.into_any().unbind()),
            false,
        )
    }
    fn pending_output(&mut self) -> io::Result<usize> {
        self.call(false, |_, v| {
            strict_usize(&v.call_method0("pending_output")?)
        })
    }
    fn close(&mut self) -> io::Result<()> {
        self.unit(
            "close",
            |py| Ok(pyo3::types::PyTuple::empty(py).into_any().unbind()),
            true,
        )
    }
    fn reconfigure(&mut self, token: &[u8]) -> io::Result<()> {
        self.unit(
            "reconfigure",
            |py| {
                Ok(pyo3::types::PyTuple::new(py, [PyBytes::new(py, token)])?
                    .into_any()
                    .unbind())
            },
            false,
        )
    }
}
pub(crate) fn cancelled(
    callback: &Py<PyAny>,
    failures: &RefCell<CallbackError>,
    interpreter: i64,
) -> io::Result<bool> {
    Python::attach(|py| {
        let result = (|| {
            if crate::scoped::interpreter(py)? != interpreter {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "cancellation belongs to another interpreter",
                ));
            }
            strict_bool(&callback.bind(py).call0()?)
        })();
        let kind = result
            .as_ref()
            .err()
            .map(|error| io::Error::from(error.clone_ref(py)).kind());
        failures.borrow_mut().capture(result).map_err(|_| {
            io::Error::new(
                kind.expect("captured cancellation error"),
                "Python cancellation callback failed",
            )
        })
    })
}
#[pyclass(module = "cs705.clone", frozen)]
/// Factory-created one-shot engine; run consumes it even after provider failure.
pub struct Session {
    inner: Owner<core::Session>,
}
impl Session {
    pub fn from_core(value: core::Session) -> Self {
        Self {
            inner: Owner::new(value),
        }
    }
}
#[pymethods]
impl Session {
    #[staticmethod]
    fn query(config: &SessionConfig) -> PyResult<Self> {
        config.inner.with(|v| {
            Ok(Self {
                inner: Owner::new(core::Session::query(v.clone()).map_err(errors::clone)?),
            })
        })
    }
    #[staticmethod]
    fn read(config: &SessionConfig, profile: &Profile) -> PyResult<Self> {
        config.inner.with(|v| {
            Ok(Self {
                inner: Owner::new(
                    core::Session::read(v.clone(), profile.inner).map_err(errors::clone)?,
                ),
            })
        })
    }
    #[staticmethod]
    fn read_after_query(
        config: &SessionConfig,
        profile: &Profile,
        query: &Outcome,
    ) -> PyResult<Self> {
        let config = config.inner.with(|v| Ok(v.clone()))?;
        query.inner.with(|q| {
            Ok(Self {
                inner: Owner::new(
                    core::Session::read_after_query(config, profile.inner, q)
                        .map_err(errors::clone)?,
                ),
            })
        })
    }
    #[staticmethod]
    fn write(
        config: &SessionConfig,
        profile: &Profile,
        mask: &Bound<'_, PyAny>,
        image: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let (mask, image) = (strict_u8(mask)?, strict_bytes(image)?.to_vec());
        config.inner.with(|v| {
            Ok(Self {
                inner: Owner::new(
                    core::Session::write(v.clone(), profile.inner, mask, &image)
                        .map_err(errors::clone)?,
                ),
            })
        })
    }
    #[staticmethod]
    fn write_after_query(
        config: &SessionConfig,
        profile: &Profile,
        mask: &Bound<'_, PyAny>,
        image: &Bound<'_, PyAny>,
        query: &Outcome,
    ) -> PyResult<Self> {
        let (mask, image) = (strict_u8(mask)?, strict_bytes(image)?.to_vec());
        let config = config.inner.with(|v| Ok(v.clone()))?;
        query.inner.with(|q| {
            Ok(Self {
                inner: Owner::new(
                    core::Session::write_after_query(config, profile.inner, mask, &image, q)
                        .map_err(errors::clone)?,
                ),
            })
        })
    }
    /// Consume this engine after validating that cancelled_callback is callable.
    ///
    /// Transport calls are synchronous on this thread, with the interpreter
    /// detached between calls. Native exit attempts close once. Protocol failure,
    /// timeout, and true cancellation return an Outcome. Provider exceptions keep
    /// their identity and traceback, with Outcome in __context__.exceptions.
    fn run(
        &self,
        py: Python<'_>,
        transport: Py<PyAny>,
        cancelled_callback: Py<PyAny>,
    ) -> PyResult<Py<Outcome>> {
        if !cancelled_callback.bind(py).is_callable() {
            return Err(PyTypeError::new_err("cancelled must be callable"));
        }
        let interpreter = crate::scoped::interpreter(py)?;
        let engine = self.inner.take()?.consume();
        let (native, failures) = py.detach(move || {
            let failures = RefCell::new(CallbackError::default());
            let mut adapter = Adapter {
                object: &transport,
                failures: &failures,
                interpreter,
            };
            let outcome = engine.run(&mut adapter, || {
                cancelled(&cancelled_callback, &failures, interpreter)
            });
            (outcome, failures.into_inner())
        });
        let outcome = Py::new(
            py,
            Outcome {
                inner: Handle::new(native),
            },
        )?;
        failures.finish_with_outcome(py, Ok(outcome.clone_ref(py)), outcome.into_any())
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    add_enum(m, "Echo", &["None", "Known", "Auto"])?;
    add_enum(m, "Operation", &["Query", "Read", "Write"])?;
    add_enum(
        m,
        "State",
        &[
            "Startup",
            "QuerySend",
            "QueryWait",
            "ReadSend",
            "ReadWait",
            "WriteStart",
            "WriteData",
            "WriteEnd",
            "AckWait",
            "Drain",
            "Closed",
        ],
    )?;
    add_enum(
        m,
        "WriteDisposition",
        &["NotStarted", "Unconfirmed", "Acknowledged"],
    )?;
    add_enum(m, "ErrorKind", IO_NAMES)?;
    m.add_class::<TransportSettings>()?;
    m.add_class::<SessionConfig>()?;
    m.add_class::<ErrorData>()?;
    m.add_class::<Failure>()?;
    m.add_class::<Outcome>()?;
    m.add_class::<Session>()?;
    Ok(())
}
