//! Native variants become structured exceptions, never message-parsed categories.
use pyo3::create_exception;
use pyo3::exceptions::{PyBaseException, PyException, PyOSError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

create_exception!(
    cs705,
    Error,
    PyException,
    "Base for native domain failures.\n\nNative-raised instances carry structured domain, kind, diagnostic, and native data.\nCommitted effects are not rolled back. Standard boundary exceptions, OSError,\nand retained callback exceptions are not replaced by this class."
);

pub(crate) fn attach_native(error: &PyErr, native: &Bound<'_, PyAny>) -> PyResult<()> {
    let py = native.py();
    // Error carriers may retain a caller-supplied exception. Its attribute hooks
    // must not replace that primary failure or reenter while metadata is added.
    py.get_type::<PyBaseException>()
        .getattr("__dict__")?
        .get_item("__dict__")?
        .call_method1("__get__", (error.value(py),))?
        .cast::<PyDict>()?
        .set_item("native", native)
}

fn with_native<T: pyo3::PyClass<BaseType = PyAny>>(error: PyErr, native: T) -> PyErr {
    Python::attach(|py| {
        match Py::new(py, native).and_then(|native| attach_native(&error, native.bind(py).as_any()))
        {
            Ok(()) => error,
            Err(error) => error,
        }
    })
}

fn domain_error(
    domain: &str,
    kind: &str,
    message: String,
    reason: Option<&str>,
    fields: &[(&str, u64)],
    diagnostic: crate::diagnostic::Diagnostic,
) -> PyErr {
    Python::attach(|py| {
        let error = Error::new_err(message);
        let value = error.value(py);
        let result = (|| -> PyResult<()> {
            value.setattr("domain", domain)?;
            value.setattr("kind", kind)?;
            if let Some(reason) = reason {
                value.setattr("reason", reason)?;
            }
            for (name, number) in fields {
                value.setattr(*name, *number)?;
            }
            value.setattr("diagnostic", Py::new(py, diagnostic)?)?;
            Ok(())
        })();
        match result {
            Ok(()) => error,
            Err(error) => error,
        }
    })
}

pub fn icf(error: cs705_core::icf::Error) -> PyErr {
    use cs705_core::icf::Error::*;
    let native = crate::error_values::IcfErrorData::from_core(error.clone());
    let (kind, reason, line) = match &error {
        Invalid { line, reason } => ("Invalid", Some(reason.to_string()), Some(*line)),
        Unsupported { line, reason } => ("Unsupported", Some(reason.to_string()), Some(*line)),
        ChecksumMismatch { line } => ("ChecksumMismatch", None, Some(*line)),
        Operation(reason) => ("Operation", Some(reason.to_string()), None),
    };
    let fields: Vec<_> = line.into_iter().map(|line| ("line", line as u64)).collect();
    with_native(
        domain_error(
            "icf",
            kind,
            error.to_string(),
            reason.as_deref(),
            &fields,
            crate::diagnostic::Diagnostic::from_icf(error),
        ),
        native,
    )
}
pub fn file(error: cs705_core::icf::document::FileError) -> PyErr {
    Python::attach(|py| {
        crate::error_values::FileErrorData::from_core(py, error)
            .and_then(|value| Py::new(py, value))
            .and_then(|value| crate::error_values::FileErrorData::exception_for(value, py))
            .unwrap_or_else(|error| error)
    })
}
pub fn io(error: std::io::Error) -> PyErr {
    match error.raw_os_error() {
        Some(code) => {
            #[cfg(windows)]
            return PyOSError::new_err((0, error.to_string(), None::<String>, code));
            #[cfg(not(windows))]
            return PyOSError::new_err((code, error.to_string()));
        }
        // PyO3 preserves a wrapped Python exception, including its traceback.
        None => error.into(),
    }
}

/// Convert a borrowed native I/O value to a Python exception without cloning its payload.
pub(crate) fn io_view(error: &std::io::Error) -> PyErr {
    if let Some(code) = error.raw_os_error() {
        #[cfg(windows)]
        return PyOSError::new_err((0, error.to_string(), None::<String>, code));
        #[cfg(not(windows))]
        return PyOSError::new_err((code, error.to_string()));
    }
    use pyo3::exceptions::*;
    use std::io::ErrorKind;
    let message = error.to_string();
    match error.kind() {
        ErrorKind::BrokenPipe => PyBrokenPipeError::new_err(message),
        ErrorKind::ConnectionRefused => PyConnectionRefusedError::new_err(message),
        ErrorKind::ConnectionAborted => PyConnectionAbortedError::new_err(message),
        ErrorKind::ConnectionReset => PyConnectionResetError::new_err(message),
        ErrorKind::Interrupted => PyInterruptedError::new_err(message),
        ErrorKind::NotFound => PyFileNotFoundError::new_err(message),
        ErrorKind::PermissionDenied => PyPermissionError::new_err(message),
        ErrorKind::AlreadyExists => PyFileExistsError::new_err(message),
        ErrorKind::WouldBlock => PyBlockingIOError::new_err(message),
        ErrorKind::TimedOut => PyTimeoutError::new_err(message),
        ErrorKind::OutOfMemory => PyMemoryError::new_err(message),
        ErrorKind::IsADirectory => PyIsADirectoryError::new_err(message),
        ErrorKind::NotADirectory => PyNotADirectoryError::new_err(message),
        _ => PyOSError::new_err(message),
    }
}
pub fn csv(error: cs705_core::csv::Error) -> PyErr {
    use cs705_core::csv::Error::*;
    let native = crate::error_values::CsvErrorData::from_core(error.clone());
    let (kind, reason) = match &error {
        Limit(s) => ("Limit", s.to_string()),
        Invalid(s) => ("Invalid", s.to_string()),
        Conversion(s) => ("Conversion", s.to_string()),
        SourceUnavailable(s) => ("SourceUnavailable", s.to_string()),
        Operation(s) => ("Operation", s.to_string()),
    };
    with_native(
        domain_error(
            "csv",
            kind,
            error.to_string(),
            Some(reason.as_str()),
            &[],
            crate::diagnostic::Diagnostic::from_csv(error),
        ),
        native,
    )
}
macro_rules! row_error {
    ($name:ident, $from:ident, $data:ident) => {
        pub fn $name(error: cs705_core::$name::Error) -> PyErr {
            use cs705_core::$name::Error::*;
            let native = crate::error_values::$data::from_core(error.clone());
            let (kind, reason, field, number) = match &error {
                Invalid { line, reason } => ("Invalid", Some(reason.to_string()), "line", *line),
                Unsupported { line, reason } => {
                    ("Unsupported", Some(reason.to_string()), "line", *line)
                }
                RowIndexOutOfBounds { index } => ("RowIndexOutOfBounds", None, "index", *index),
            };
            with_native(
                domain_error(
                    stringify!($name),
                    kind,
                    error.to_string(),
                    reason.as_deref(),
                    &[(field, number as u64)],
                    crate::diagnostic::Diagnostic::$from(error),
                ),
                native,
            )
        }
    };
}
row_error!(gps, from_gps, GpsErrorData);
row_error!(repeater, from_repeater, RepeaterErrorData);
pub fn clone(error: cs705_core::clone::Error) -> PyErr {
    use cs705_core::clone::Error::*;
    let (kind, reason, fields): (_, _, Vec<(&str, u64)>) = match &error {
        Invalid(s) => ("Invalid", Some(s.to_string()), vec![]),
        UnsupportedCommand(n) => ("UnsupportedCommand", None, vec![("command", *n as u64)]),
        ChecksumMismatch => ("ChecksumMismatch", None, vec![]),
        AddressOverflow => ("AddressOverflow", None, vec![]),
        SourceBounds => ("SourceBounds", None, vec![]),
        Rejected(n) => ("Rejected", None, vec![("status", *n as u64)]),
    };
    let native = crate::clone::session::ErrorData {
        inner: error.clone(),
    };
    with_native(
        domain_error(
            "clone",
            kind,
            error.to_string(),
            reason.as_deref(),
            &fields,
            crate::diagnostic::Diagnostic::from_clone(error),
        ),
        native,
    )
}
pub fn presentation(error: cs705_core::presentation::Error) -> PyErr {
    use cs705_core::presentation::Error::*;
    let native = crate::error_values::PresentationErrorData::from_core(error);
    let (kind, fields): (_, Vec<(&str, u64)>) = match &error {
        UnsupportedLanguage => ("UnsupportedLanguage", vec![]),
        UnregisteredField { key } => ("UnregisteredField", vec![("key", *key as u64)]),
        InvalidIndex => ("InvalidIndex", vec![]),
        InvalidSymbolWidth => ("InvalidSymbolWidth", vec![]),
        AmbiguousField { key } => ("AmbiguousField", vec![("key", *key as u64)]),
    };
    with_native(
        domain_error(
            "presentation",
            kind,
            error.to_string(),
            None,
            &fields,
            crate::diagnostic::Diagnostic::from_presentation(error),
        ),
        native,
    )
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("Error", m.py().get_type::<Error>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::exceptions::{PyFileNotFoundError, PyValueError};

    #[test]
    fn native_errors_keep_variant_fields_and_io_semantics() {
        Python::initialize();
        Python::attach(|py| {
            let error = icf(cs705_core::icf::Error::Operation("producer failed".into()));
            assert!(error.is_instance_of::<Error>(py));
            let value = error.value(py);
            assert_eq!(
                value
                    .getattr("domain")
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "icf"
            );
            assert_eq!(
                value.getattr("kind").unwrap().extract::<String>().unwrap(),
                "Operation"
            );
            assert_eq!(
                value
                    .getattr("reason")
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "producer failed"
            );
            assert!(!value.hasattr("line").unwrap());
            let diagnostic = value.getattr("diagnostic").unwrap();
            let diagnostic = diagnostic.cast::<crate::diagnostic::Diagnostic>().unwrap();
            let diagnostic = diagnostic.try_borrow().unwrap();
            assert_eq!(
                diagnostic.native().kind,
                cs705_core::diagnostic::Kind::Operation
            );
            assert_eq!(
                diagnostic.native().technical_detail(),
                Some("producer failed")
            );

            let error = gps(cs705_core::gps::Error::RowIndexOutOfBounds { index: 7 });
            assert_eq!(
                error
                    .value(py)
                    .getattr("index")
                    .unwrap()
                    .extract::<usize>()
                    .unwrap(),
                7
            );

            let error = file(cs705_core::icf::document::FileError::Output(
                std::io::Error::from_raw_os_error(2),
            ));
            assert!(error.is_instance_of::<PyOSError>(py));
            assert!(error.is_instance_of::<PyFileNotFoundError>(py));
            assert_eq!(
                error
                    .value(py)
                    .getattr("errno")
                    .unwrap()
                    .extract::<i32>()
                    .unwrap(),
                2
            );
            let original = PyValueError::new_err("callback failed");
            let error = io(std::io::Error::other(original.clone_ref(py)));
            assert!(error.value(py).is(original.value(py)));
        });
    }
}
