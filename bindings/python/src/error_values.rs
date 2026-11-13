//! Constructible native error values; exceptions remain a separate surface.
use crate::{conversion::*, diagnostic::Diagnostic, errors};
use cs705_core::{csv, gps, icf, presentation, repeater};
use pyo3::exceptions::{PyAttributeError, PyTypeError, PyValueError};
use pyo3::exceptions::{PyBaseException, PyOSError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::PyString;
use pyo3::{PyTraverseError, PyVisit};
use std::borrow::Cow;
use std::{io, sync::Arc};

fn owned_reason(value: &Bound<'_, PyAny>) -> PyResult<Cow<'static, str>> {
    Ok(Cow::Owned(value.cast::<PyString>()?.extract::<String>()?))
}
fn wrong_variant(operation: &str) -> PyErr {
    PyValueError::new_err(format!("{operation} does not apply to this error variant"))
}

/// Accept only canonical error values or captured library exceptions, never duck-typed payloads.
pub(crate) fn diagnostic_from_value<'py>(
    value: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, Diagnostic>> {
    if !(value.is_instance_of::<IcfErrorData>()
        || value.is_instance_of::<CsvErrorData>()
        || value.is_instance_of::<GpsErrorData>()
        || value.is_instance_of::<RepeaterErrorData>()
        || value.is_instance_of::<PresentationErrorData>()
        || value.is_instance_of::<errors::Error>())
    {
        return Err(PyTypeError::new_err(
            "expected canonical ErrorData or a captured cs705.Error",
        ));
    }
    let diagnostic = value.getattr("diagnostic").map_err(|error| {
        if error.is_instance_of::<PyAttributeError>(value.py()) {
            PyTypeError::new_err("expected a retained native diagnostic payload")
        } else {
            error
        }
    })?;
    diagnostic.cast_into::<Diagnostic>().map_err(Into::into)
}

macro_rules! error_value {
    ($name:ident,$native:path,$module:literal,$domain:literal,$raise:path,$diagnostic:path; $($methods:tt)*) => {
        #[pyclass(name="ErrorData",module=$module,frozen)]
        #[derive(Clone)]
        pub struct $name { pub(crate) inner:$native }
        impl $name { pub fn from_core(inner:$native)->Self {Self {inner}} }
        #[pymethods]
        #[allow(non_snake_case)]
        impl $name {
            #[getter] fn domain(&self)->&'static str {$domain}
            #[getter] fn diagnostic(&self)->Diagnostic {$diagnostic(self.inner.clone())}
            fn exception(&self,py:Python<'_>)->Py<PyAny> {$raise(self.inner.clone()).into_value(py).into_any()}
            fn __str__(&self)->String {self.inner.to_string()}
            fn __eq__(&self,other:&Self)->bool {self.inner==other.inner}
            fn __copy__(&self)->Self {self.clone()}
            fn __deepcopy__(&self,_memo:&Bound<'_,PyAny>)->Self {self.clone()}
            $($methods)*
        }
    }
}
error_value!(IcfErrorData,icf::Error,"cs705.icf","icf",errors::icf,Diagnostic::from_icf;
    #[staticmethod] fn Invalid(line:&Bound<'_,PyAny>,reason:&Bound<'_,PyAny>)->PyResult<Self> {Ok(Self::from_core(icf::Error::Invalid {line:strict_usize(line)?,reason:owned_reason(reason)?}))}
    #[staticmethod] fn Unsupported(line:&Bound<'_,PyAny>,reason:&Bound<'_,PyAny>)->PyResult<Self> {Ok(Self::from_core(icf::Error::Unsupported {line:strict_usize(line)?,reason:owned_reason(reason)?}))}
    #[staticmethod] fn ChecksumMismatch(line:&Bound<'_,PyAny>)->PyResult<Self> {Ok(Self::from_core(icf::Error::ChecksumMismatch {line:strict_usize(line)?}))}
    #[staticmethod] fn Operation(reason:&Bound<'_,PyAny>)->PyResult<Self> {Ok(Self::from_core(icf::Error::Operation(owned_reason(reason)?)))}
    #[getter] fn kind(&self)->&'static str {match self.inner {icf::Error::Invalid {..}=>"Invalid",icf::Error::Unsupported {..}=>"Unsupported",icf::Error::ChecksumMismatch {..}=>"ChecksumMismatch",icf::Error::Operation(_)=>"Operation"}}
    #[getter] fn reason(&self)->Option<&str> {match &self.inner {icf::Error::Invalid {reason,..}|icf::Error::Unsupported {reason,..}|icf::Error::Operation(reason)=>Some(reason.as_ref()),_=>None}}
    #[getter] fn line(&self)->Option<usize> {match self.inner {icf::Error::Invalid {line,..}|icf::Error::Unsupported {line,..}|icf::Error::ChecksumMismatch {line}=>Some(line),_=>None}}
    /// Return a replacement value, leaving this error unchanged.
    fn with_reason(&self,value:&Bound<'_,PyAny>)->PyResult<Self> {
        let mut error=self.inner.clone();
        match &mut error {icf::Error::Invalid {reason:target,..}|icf::Error::Unsupported {reason:target,..}|icf::Error::Operation(target)=>*target=owned_reason(value)?,_=>return Err(wrong_variant("with_reason"))}
        Ok(Self::from_core(error))
    }
    /// Return a replacement value, leaving this error unchanged.
    fn with_line(&self,value:&Bound<'_,PyAny>)->PyResult<Self> {
        let mut error=self.inner.clone();
        match &mut error {icf::Error::Invalid {line,..}|icf::Error::Unsupported {line,..}|icf::Error::ChecksumMismatch {line}=>*line=strict_usize(value)?,_=>return Err(wrong_variant("with_line"))}
        Ok(Self::from_core(error))
    }
);
error_value!(CsvErrorData,csv::Error,"cs705.csv","csv",errors::csv,Diagnostic::from_csv;
    #[staticmethod] fn Limit(reason:&Bound<'_,PyAny>)->PyResult<Self> {Ok(Self::from_core(csv::Error::Limit(owned_reason(reason)?)))}
    #[staticmethod] fn Invalid(reason:&Bound<'_,PyAny>)->PyResult<Self> {Ok(Self::from_core(csv::Error::Invalid(owned_reason(reason)?)))}
    #[staticmethod] fn Conversion(reason:&Bound<'_,PyAny>)->PyResult<Self> {Ok(Self::from_core(csv::Error::Conversion(owned_reason(reason)?)))}
    #[staticmethod] fn SourceUnavailable(reason:&Bound<'_,PyAny>)->PyResult<Self> {Ok(Self::from_core(csv::Error::SourceUnavailable(owned_reason(reason)?)))}
    #[staticmethod] fn Operation(reason:&Bound<'_,PyAny>)->PyResult<Self> {Ok(Self::from_core(csv::Error::Operation(owned_reason(reason)?)))}
    #[getter] fn kind(&self)->&'static str {match self.inner {csv::Error::Limit(_)=>"Limit",csv::Error::Invalid(_)=>"Invalid",csv::Error::Conversion(_)=>"Conversion",csv::Error::SourceUnavailable(_)=>"SourceUnavailable",csv::Error::Operation(_)=>"Operation"}}
    #[getter] fn reason(&self)->&str {match &self.inner {csv::Error::Limit(s)|csv::Error::Invalid(s)|csv::Error::Conversion(s)|csv::Error::SourceUnavailable(s)|csv::Error::Operation(s)=>s.as_ref()}}
    /// Return a replacement value, preserving the variant and leaving this error unchanged.
    fn with_reason(&self,value:&Bound<'_,PyAny>)->PyResult<Self> {
        let reason=owned_reason(value)?;
        Ok(Self::from_core(match self.inner {csv::Error::Limit(_)=>csv::Error::Limit(reason),csv::Error::Invalid(_)=>csv::Error::Invalid(reason),csv::Error::Conversion(_)=>csv::Error::Conversion(reason),csv::Error::SourceUnavailable(_)=>csv::Error::SourceUnavailable(reason),csv::Error::Operation(_)=>csv::Error::Operation(reason)}))
    }
);
macro_rules! row_error {
    ($name:ident,$native:ident,$module:literal,$domain:literal,$raise:path,$diagnostic:path) => {
        error_value!($name,$native::Error,$module,$domain,$raise,$diagnostic;
            #[staticmethod] fn Invalid(line:&Bound<'_,PyAny>,reason:&Bound<'_,PyAny>)->PyResult<Self> {Ok(Self::from_core($native::Error::Invalid {line:strict_usize(line)?,reason:owned_reason(reason)?}))}
            #[staticmethod] fn Unsupported(line:&Bound<'_,PyAny>,reason:&Bound<'_,PyAny>)->PyResult<Self> {Ok(Self::from_core($native::Error::Unsupported {line:strict_usize(line)?,reason:owned_reason(reason)?}))}
            #[staticmethod] fn RowIndexOutOfBounds(index:&Bound<'_,PyAny>)->PyResult<Self> {Ok(Self::from_core($native::Error::RowIndexOutOfBounds {index:strict_usize(index)?}))}
            #[getter] fn kind(&self)->&'static str {match self.inner {$native::Error::Invalid {..}=>"Invalid",$native::Error::Unsupported {..}=>"Unsupported",$native::Error::RowIndexOutOfBounds {..}=>"RowIndexOutOfBounds"}}
            #[getter] fn reason(&self)->Option<&str> {match &self.inner {$native::Error::Invalid {reason,..}|$native::Error::Unsupported {reason,..}=>Some(reason.as_ref()),_=>None}}
            #[getter] fn line(&self)->Option<usize> {match self.inner {$native::Error::Invalid {line,..}|$native::Error::Unsupported {line,..}=>Some(line),_=>None}}
            #[getter] fn index(&self)->Option<usize> {match self.inner {$native::Error::RowIndexOutOfBounds {index}=>Some(index),_=>None}}
            /// Return a replacement value, leaving this error unchanged.
            fn with_reason(&self,value:&Bound<'_,PyAny>)->PyResult<Self> {
                let mut error=self.inner.clone();
                match &mut error {$native::Error::Invalid {reason:target,..}|$native::Error::Unsupported {reason:target,..}=>*target=owned_reason(value)?,_=>return Err(wrong_variant("with_reason"))}
                Ok(Self::from_core(error))
            }
            /// Return a replacement value, leaving this error unchanged.
            fn with_line(&self,value:&Bound<'_,PyAny>)->PyResult<Self> {
                let mut error=self.inner.clone();
                match &mut error {$native::Error::Invalid {line,..}|$native::Error::Unsupported {line,..}=>*line=strict_usize(value)?,_=>return Err(wrong_variant("with_line"))}
                Ok(Self::from_core(error))
            }
            /// Return a replacement value, leaving this error unchanged.
            fn with_index(&self,value:&Bound<'_,PyAny>)->PyResult<Self> {
                if !matches!(self.inner,$native::Error::RowIndexOutOfBounds {..}) {return Err(wrong_variant("with_index"));}
                Self::RowIndexOutOfBounds(value)
            }
        );
    }
}
row_error!(
    GpsErrorData,
    gps,
    "cs705.gps",
    "gps",
    errors::gps,
    Diagnostic::from_gps
);
row_error!(
    RepeaterErrorData,
    repeater,
    "cs705.repeater",
    "repeater",
    errors::repeater,
    Diagnostic::from_repeater
);
error_value!(PresentationErrorData,presentation::Error,"cs705.presentation","presentation",errors::presentation,Diagnostic::from_presentation;
    #[staticmethod] fn UnsupportedLanguage()->Self {Self::from_core(presentation::Error::UnsupportedLanguage)}
    #[staticmethod] fn UnregisteredField(key:&Bound<'_,PyAny>)->PyResult<Self> {Ok(Self::from_core(presentation::Error::UnregisteredField {key:strict_u16(key)?}))}
    #[staticmethod] fn InvalidIndex()->Self {Self::from_core(presentation::Error::InvalidIndex)}
    #[staticmethod] fn InvalidSymbolWidth()->Self {Self::from_core(presentation::Error::InvalidSymbolWidth)}
    #[staticmethod] fn AmbiguousField(key:&Bound<'_,PyAny>)->PyResult<Self> {Ok(Self::from_core(presentation::Error::AmbiguousField {key:strict_u16(key)?}))}
    #[getter] fn kind(&self)->&'static str {match self.inner {presentation::Error::UnsupportedLanguage=>"UnsupportedLanguage",presentation::Error::UnregisteredField {..}=>"UnregisteredField",presentation::Error::InvalidIndex=>"InvalidIndex",presentation::Error::InvalidSymbolWidth=>"InvalidSymbolWidth",presentation::Error::AmbiguousField {..}=>"AmbiguousField"}}
    #[getter] fn key(&self)->Option<u16> {match self.inner {presentation::Error::UnregisteredField {key}|presentation::Error::AmbiguousField {key}=>Some(key),_=>None}}
    /// Return a replacement value, leaving this error unchanged.
    fn with_key(&self,value:&Bound<'_,PyAny>)->PyResult<Self> {
        Ok(Self::from_core(match self.inner {presentation::Error::UnregisteredField {..}=>presentation::Error::UnregisteredField {key:strict_u16(value)?},presentation::Error::AmbiguousField {..}=>presentation::Error::AmbiguousField {key:strict_u16(value)?},_=>return Err(wrong_variant("with_key"))}))
    }
    fn localized(&self,language:&Bound<'_,PyAny>)->PyResult<String> {Ok(self.inner.localized(crate::presentation::language(language)?))}
);

enum IoPayload {
    Native(Arc<io::Error>),
    Python {
        kind: io::ErrorKind,
        error: Py<PyBaseException>,
    },
}
#[pyclass(module = "cs705")]
/// Shared I/O error payload, created by from_os_error() or a native error report.
/// share() retains the actual source rather than reconstructing it from text.
/// Python sources are GC-visible and keep their exception identity and traceback.
pub struct IoErrorData {
    payload: Option<IoPayload>,
}
impl IoErrorData {
    fn from_core(py: Python<'_>, error: io::Error) -> Self {
        let payload = if error.get_ref().is_some_and(|source| source.is::<PyErr>()) {
            let kind = error.kind();
            let error = *error
                .into_inner()
                .expect("Python error source")
                .downcast::<PyErr>()
                .expect("checked Python error source");
            // Move the real Python source into a GC-visible edge; never replace it with text.
            IoPayload::Python {
                kind,
                error: error.into_value(py),
            }
        } else {
            IoPayload::Native(Arc::new(error))
        };
        Self {
            payload: Some(payload),
        }
    }
    fn snapshot(&self, py: Python<'_>) -> PyResult<IoPayload> {
        match self
            .payload
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("I/O carrier was cleared"))?
        {
            IoPayload::Native(error) => Ok(IoPayload::Native(error.clone())),
            IoPayload::Python { kind, error } => Ok(IoPayload::Python {
                kind: *kind,
                error: error.clone_ref(py),
            }),
        }
    }
    fn exception_for(slf: Py<Self>, py: Python<'_>) -> PyResult<PyErr> {
        let source = slf.try_borrow(py)?.snapshot(py)?;
        let error = match source {
            IoPayload::Native(error) => errors::io_view(&error),
            IoPayload::Python { error, .. } => PyErr::from_value(error.into_bound(py).into_any()),
        };
        errors::attach_native(&error, slf.bind(py).as_any())?;
        Ok(error)
    }
}
#[pymethods]
impl IoErrorData {
    #[staticmethod]
    /// Retain the supplied OSError object, including its traceback; do not raise it.
    fn from_os_error(py: Python<'_>, error: &Bound<'_, PyAny>) -> PyResult<Self> {
        error.cast::<PyOSError>()?;
        Ok(Self::from_core(
            py,
            io::Error::from(PyErr::from_value(error.clone())),
        ))
    }
    /// Share the actual source; std::io::Error is not cloned or reconstructed.
    fn share(&self, py: Python<'_>) -> PyResult<Self> {
        Ok(Self {
            payload: Some(self.snapshot(py)?),
        })
    }
    #[getter]
    fn error_kind(&self) -> PyResult<String> {
        let kind = match self
            .payload
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("I/O carrier was cleared"))?
        {
            IoPayload::Native(error) => error.kind(),
            IoPayload::Python { kind, .. } => *kind,
        };
        Ok(format!("{kind:?}"))
    }
    #[getter]
    fn raw_os_error(slf: Py<Self>, py: Python<'_>) -> PyResult<Option<i32>> {
        let source = slf.try_borrow(py)?.snapshot(py)?;
        match source {
            IoPayload::Native(error) => Ok(error.raw_os_error()),
            IoPayload::Python { error, .. } => {
                if !error.bind(py).is_instance_of::<PyOSError>() {
                    return Ok(None);
                }
                let code = error.bind(py).getattr("errno")?;
                if code.is_none() {
                    Ok(None)
                } else {
                    strict_i32(&code).map(Some)
                }
            }
        }
    }
    /// Return without raising: map a native payload or return the same Python
    /// exception object and traceback. Updates its native carrier metadata.
    fn exception(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(Self::exception_for(slf, py)?.into_value(py).into_any())
    }
    fn __str__(slf: Py<Self>, py: Python<'_>) -> PyResult<String> {
        let source = slf.try_borrow(py)?.snapshot(py)?;
        match source {
            IoPayload::Native(error) => Ok(error.to_string()),
            IoPayload::Python { error, .. } => {
                Ok(PyErr::from_value(error.into_bound(py).into_any()).to_string())
            }
        }
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Some(IoPayload::Python { error, .. }) = &self.payload {
            visit.call(error)?;
        }
        Ok(())
    }
    fn __clear__(&mut self) {
        self.payload = None;
    }
}

enum FilePayload {
    Image(icf::Error),
    Output(Py<IoErrorData>),
}
#[pyclass(name = "FileError", module = "cs705.icf.document")]
pub struct FileErrorData {
    payload: Option<FilePayload>,
}
impl FileErrorData {
    pub fn from_core(py: Python<'_>, error: icf::document::FileError) -> PyResult<Self> {
        Ok(Self {
            payload: Some(match error {
                icf::document::FileError::Image(error) => FilePayload::Image(error),
                icf::document::FileError::Output(error) => {
                    FilePayload::Output(Py::new(py, IoErrorData::from_core(py, error))?)
                }
            }),
        })
    }
    fn snapshot(&self, py: Python<'_>) -> PyResult<FilePayload> {
        match self
            .payload
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("file error carrier was cleared"))?
        {
            FilePayload::Image(error) => Ok(FilePayload::Image(error.clone())),
            FilePayload::Output(error) => Ok(FilePayload::Output(error.clone_ref(py))),
        }
    }
    fn output_value(py: Python<'_>, error: &Bound<'_, PyAny>) -> PyResult<Py<IoErrorData>> {
        if let Ok(error) = error.cast::<IoErrorData>() {
            return Ok(error.clone().unbind());
        }
        Py::new(py, IoErrorData::from_os_error(py, error)?)
    }
    pub(crate) fn exception_for(slf: Py<Self>, py: Python<'_>) -> PyResult<PyErr> {
        let source = slf.try_borrow(py)?.snapshot(py)?;
        let error = match source {
            FilePayload::Image(error) => errors::icf(error),
            FilePayload::Output(error) => IoErrorData::exception_for(error, py)?,
        };
        errors::attach_native(&error, slf.bind(py).as_any())?;
        Ok(error)
    }
}
#[pymethods]
#[allow(non_snake_case)]
impl FileErrorData {
    #[staticmethod]
    fn Image(error: &IcfErrorData) -> Self {
        Self {
            payload: Some(FilePayload::Image(error.inner.clone())),
        }
    }
    #[staticmethod]
    fn Output(py: Python<'_>, error: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            payload: Some(FilePayload::Output(Self::output_value(py, error)?)),
        })
    }
    #[getter]
    fn kind(&self) -> PyResult<&'static str> {
        match self.payload.as_ref() {
            Some(FilePayload::Image(_)) => Ok("Image"),
            Some(FilePayload::Output(_)) => Ok("Output"),
            None => Err(PyRuntimeError::new_err("file error carrier was cleared")),
        }
    }
    #[getter]
    fn image(&self) -> PyResult<Option<IcfErrorData>> {
        match self.payload.as_ref() {
            Some(FilePayload::Image(error)) => Ok(Some(IcfErrorData::from_core(error.clone()))),
            Some(_) => Ok(None),
            None => Err(PyRuntimeError::new_err("file error carrier was cleared")),
        }
    }
    #[getter]
    fn output(&self, py: Python<'_>) -> PyResult<Option<Py<IoErrorData>>> {
        match self.payload.as_ref() {
            Some(FilePayload::Output(error)) => Ok(Some(error.clone_ref(py))),
            Some(_) => Ok(None),
            None => Err(PyRuntimeError::new_err("file error carrier was cleared")),
        }
    }
    #[getter]
    fn diagnostic(&self) -> PyResult<Option<Diagnostic>> {
        match self.payload.as_ref() {
            Some(FilePayload::Image(error)) => Ok(Some(Diagnostic::from_icf(error.clone()))),
            Some(_) => Ok(None),
            None => Err(PyRuntimeError::new_err("file error carrier was cleared")),
        }
    }
    /// Return a replacement Image value; the original is unchanged.
    fn with_image(&self, error: &IcfErrorData) -> PyResult<Self> {
        if !matches!(self.payload, Some(FilePayload::Image(_))) {
            return Err(wrong_variant("with_image"));
        }
        Ok(Self::Image(error))
    }
    /// Return a replacement Output value; the original is unchanged.
    fn with_output(&self, py: Python<'_>, error: &Bound<'_, PyAny>) -> PyResult<Self> {
        if !matches!(self.payload, Some(FilePayload::Output(_))) {
            return Err(wrong_variant("with_output"));
        }
        Self::Output(py, error)
    }
    fn exception(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(Self::exception_for(slf, py)?.into_value(py).into_any())
    }
    fn __str__(slf: Py<Self>, py: Python<'_>) -> PyResult<String> {
        let source = slf.try_borrow(py)?.snapshot(py)?;
        match source {
            FilePayload::Image(error) => Ok(error.to_string()),
            FilePayload::Output(error) => IoErrorData::__str__(error, py),
        }
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Some(FilePayload::Output(error)) = &self.payload {
            visit.call(error)?;
        }
        Ok(())
    }
    fn __clear__(&mut self) {
        self.payload = None;
    }
}

#[pyclass(name = "CloneError", module = "cs705.icf.document", frozen)]
pub struct CloneErrorData {
    inner: icf::document::CloneError,
}
impl CloneErrorData {
    pub fn from_core(inner: icf::document::CloneError) -> Self {
        Self { inner }
    }
    fn exception_for(slf: Py<Self>, py: Python<'_>) -> PyResult<PyErr> {
        let error = match &slf.get().inner {
            icf::document::CloneError::Image(error) => errors::icf(error.clone()),
            icf::document::CloneError::Protocol(error) => errors::clone(error.clone()),
        };
        errors::attach_native(&error, slf.bind(py).as_any())?;
        Ok(error)
    }
}
#[pymethods]
#[allow(non_snake_case)]
impl CloneErrorData {
    #[staticmethod]
    fn Image(error: &IcfErrorData) -> Self {
        Self::from_core(icf::document::CloneError::Image(error.inner.clone()))
    }
    #[staticmethod]
    fn Protocol(error: &crate::clone::session::ErrorData) -> Self {
        Self::from_core(icf::document::CloneError::Protocol(error.inner.clone()))
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self.inner {
            icf::document::CloneError::Image(_) => "Image",
            icf::document::CloneError::Protocol(_) => "Protocol",
        }
    }
    #[getter]
    fn image(&self) -> Option<IcfErrorData> {
        match &self.inner {
            icf::document::CloneError::Image(error) => Some(IcfErrorData::from_core(error.clone())),
            _ => None,
        }
    }
    #[getter]
    fn protocol(&self) -> Option<crate::clone::session::ErrorData> {
        match &self.inner {
            icf::document::CloneError::Protocol(error) => Some(crate::clone::session::ErrorData {
                inner: error.clone(),
            }),
            _ => None,
        }
    }
    #[getter]
    fn diagnostic(&self) -> Diagnostic {
        match &self.inner {
            icf::document::CloneError::Image(error) => Diagnostic::from_icf(error.clone()),
            icf::document::CloneError::Protocol(error) => Diagnostic::from_clone(error.clone()),
        }
    }
    /// Return a replacement Image value; the original is unchanged.
    fn with_image(&self, error: &IcfErrorData) -> PyResult<Self> {
        if !matches!(self.inner, icf::document::CloneError::Image(_)) {
            return Err(wrong_variant("with_image"));
        }
        Ok(Self::Image(error))
    }
    /// Return a replacement Protocol value; the original is unchanged.
    fn with_protocol(&self, error: &crate::clone::session::ErrorData) -> PyResult<Self> {
        if !matches!(self.inner, icf::document::CloneError::Protocol(_)) {
            return Err(wrong_variant("with_protocol"));
        }
        Ok(Self::Protocol(error))
    }
    fn __str__(&self) -> String {
        self.inner.to_string()
    }
    fn exception(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(Self::exception_for(slf, py)?.into_value(py).into_any())
    }
}

pub(crate) fn clone_error(error: icf::document::CloneError) -> PyErr {
    Python::attach(|py| {
        Py::new(py, CloneErrorData::from_core(error))
            .and_then(|value| CloneErrorData::exception_for(value, py))
            .unwrap_or_else(|error| error)
    })
}

pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    root.getattr("icf")?
        .cast::<PyModule>()?
        .add("ErrorData", root.py().get_type::<IcfErrorData>())?;
    root.getattr("csv")?
        .cast::<PyModule>()?
        .add("ErrorData", root.py().get_type::<CsvErrorData>())?;
    root.getattr("gps")?
        .cast::<PyModule>()?
        .add("ErrorData", root.py().get_type::<GpsErrorData>())?;
    root.getattr("repeater")?
        .cast::<PyModule>()?
        .add("ErrorData", root.py().get_type::<RepeaterErrorData>())?;
    root.getattr("presentation")?
        .cast::<PyModule>()?
        .add("ErrorData", root.py().get_type::<PresentationErrorData>())?;
    root.add_class::<IoErrorData>()?;
    root.getattr("icf")?
        .getattr("document")?
        .cast::<PyModule>()?
        .add("FileError", root.py().get_type::<FileErrorData>())?;
    root.getattr("icf")?
        .getattr("document")?
        .cast::<PyModule>()?
        .add("CloneError", root.py().get_type::<CloneErrorData>())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{error::Error, fmt};

    #[derive(Debug)]
    struct PrivatePayload(u32);
    impl fmt::Display for PrivatePayload {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "private payload {}", self.0)
        }
    }
    impl Error for PrivatePayload {}

    #[test]
    fn file_output_keeps_private_native_payload_and_python_identity() {
        Python::initialize();
        Python::attach(|py| {
            let native = io::Error::new(io::ErrorKind::PermissionDenied, PrivatePayload(37));
            let address = native
                .get_ref()
                .unwrap()
                .downcast_ref::<PrivatePayload>()
                .unwrap() as *const _;
            let file =
                FileErrorData::from_core(py, icf::document::FileError::Output(native)).unwrap();
            let Some(FilePayload::Output(output)) = file.payload else {
                panic!("expected Output")
            };
            let source = output.borrow(py);
            let IoPayload::Native(original) = source.snapshot(py).unwrap() else {
                panic!("expected native payload")
            };
            let IoPayload::Native(shared) = source.share(py).unwrap().snapshot(py).unwrap() else {
                panic!("expected shared native payload")
            };
            assert!(Arc::ptr_eq(&original, &shared));
            let retained = original
                .get_ref()
                .unwrap()
                .downcast_ref::<PrivatePayload>()
                .unwrap();
            assert_eq!(retained.0, 37);
            assert!(std::ptr::eq(address, retained));
            drop(source);
            let value = Py::new(
                py,
                FileErrorData {
                    payload: Some(FilePayload::Output(output)),
                },
            )
            .unwrap();
            let exception = FileErrorData::exception_for(value, py).unwrap();
            assert!(exception.is_instance_of::<pyo3::exceptions::PyPermissionError>(py));

            let original =
                pyo3::exceptions::PyFileNotFoundError::new_err((2, "actual Python source"));
            let expected = original.value(py).clone().unbind();
            let value = FileErrorData::from_core(
                py,
                icf::document::FileError::Output(io::Error::from(original)),
            )
            .unwrap();
            let exception = FileErrorData::exception_for(Py::new(py, value).unwrap(), py).unwrap();
            assert!(exception.value(py).is(expected.bind(py)));
            assert_eq!(
                exception
                    .value(py)
                    .getattr("errno")
                    .unwrap()
                    .extract::<i32>()
                    .unwrap(),
                2
            );
        });
    }
}
