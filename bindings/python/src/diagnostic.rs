//! Diagnostics are derived from retained native errors, never parsed messages.
use cs705_core::diagnostic as core;
use pyo3::prelude::*;

enum Source {
    Icf(cs705_core::icf::Error),
    Csv(cs705_core::csv::Error),
    Gps(cs705_core::gps::Error),
    Repeater(cs705_core::repeater::Error),
    Clone(cs705_core::clone::Error),
    Presentation(cs705_core::presentation::Error),
}
#[pyclass(module = "cs705.diagnostic")]
/// Produced by native errors, not by a public constructor. Summary fields are
/// writable; technical_detail retains the original error text.
pub struct Diagnostic {
    source: Source,
    domain: core::Domain,
    kind: core::Kind,
    line: Option<usize>,
    index: Option<usize>,
    value: Option<u32>,
}
impl Diagnostic {
    pub fn csv_error(&self) -> Option<cs705_core::csv::Error> {
        match &self.source {
            Source::Csv(error) => Some(error.clone()),
            _ => None,
        }
    }
    pub fn icf_error(&self) -> Option<cs705_core::icf::Error> {
        match &self.source {
            Source::Icf(error) => Some(error.clone()),
            _ => None,
        }
    }
    pub fn presentation_error(&self) -> Option<cs705_core::presentation::Error> {
        match &self.source {
            Source::Presentation(error) => Some(*error),
            _ => None,
        }
    }
}
macro_rules! sources {
    ($($method:ident, $variant:ident, $ty:path);* $(;)?) => {
        impl Diagnostic {
            $(pub fn $method(error:$ty) -> Self {
                let native=core::Diagnostic::from(&error);
                Self {domain:native.domain,kind:native.kind,line:native.line,index:native.index,value:native.value,source:Source::$variant(error)}
            })*
            pub fn native(&self) -> core::Diagnostic<'_> {
                let mut native=match &self.source { $(Source::$variant(error)=>core::Diagnostic::from(error),)* };
                native.domain=self.domain;native.kind=self.kind;native.line=self.line;native.index=self.index;native.value=self.value;
                native
            }
    pub fn summary_for(&self, language:cs705_core::presentation::Language) -> String { self.native().summary(language) }
        }
    }
}
sources!(from_icf,Icf,cs705_core::icf::Error; from_csv,Csv,cs705_core::csv::Error; from_gps,Gps,cs705_core::gps::Error; from_repeater,Repeater,cs705_core::repeater::Error; from_clone,Clone,cs705_core::clone::Error; from_presentation,Presentation,cs705_core::presentation::Error;);
#[pymethods]
impl Diagnostic {
    fn summary(&self, language: &Bound<'_, PyAny>) -> PyResult<String> {
        Ok(self.summary_for(crate::presentation::language(language)?))
    }
    fn technical_detail(&self) -> Option<String> {
        self.native().technical_detail().map(str::to_owned)
    }
    #[getter]
    fn domain<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        crate::icf::enum_result(
            py,
            "cs705.diagnostic",
            "Domain",
            self.native().domain as i64,
        )
    }
    #[setter]
    fn set_domain(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        use core::Domain::*;
        let variants = [Icf, Csv, GpsCsv, RepeaterCsv, Clone, Presentation, Io];
        let code = crate::icf::enum_value(value, "cs705.diagnostic", "Domain")?;
        self.domain = *variants
            .get(usize::try_from(code).unwrap_or(usize::MAX))
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid Domain"))?;
        Ok(())
    }
    #[getter]
    fn kind<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        crate::icf::enum_result(py, "cs705.diagnostic", "Kind", self.native().kind as i64)
    }
    #[setter]
    fn set_kind(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        use core::Kind::*;
        let variants = [
            Invalid,
            Unsupported,
            Limit,
            Conversion,
            SourceUnavailable,
            Operation,
            ChecksumMismatch,
            RowIndex,
            UnsupportedCommand,
            AddressOverflow,
            SourceBounds,
            Rejected,
            UnsupportedLanguage,
            UnregisteredField,
            InvalidIndex,
            InvalidSymbolWidth,
            AmbiguousField,
        ];
        let code = crate::icf::enum_value(value, "cs705.diagnostic", "Kind")?;
        self.kind = *variants
            .get(usize::try_from(code).unwrap_or(usize::MAX))
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid Kind"))?;
        Ok(())
    }
    #[getter]
    fn line(&self) -> Option<usize> {
        self.native().line
    }
    #[setter]
    fn set_line(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.line = if value.is_none() {
            None
        } else {
            Some(crate::conversion::strict_usize(value)?)
        };
        Ok(())
    }
    #[getter]
    fn index(&self) -> Option<usize> {
        self.native().index
    }
    #[setter]
    fn set_index(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.index = if value.is_none() {
            None
        } else {
            Some(crate::conversion::strict_usize(value)?)
        };
        Ok(())
    }
    #[getter]
    fn value(&self) -> Option<u32> {
        self.native().value
    }
    #[setter]
    fn set_value(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.value = if value.is_none() {
            None
        } else {
            Some(crate::conversion::strict_u32(value)?)
        };
        Ok(())
    }
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "diagnostic")?;
    crate::presentation::add_enum(
        &m,
        "cs705.diagnostic",
        "Domain",
        &[
            "Icf",
            "Csv",
            "GpsCsv",
            "RepeaterCsv",
            "Clone",
            "Presentation",
            "Io",
        ],
    )?;
    crate::presentation::add_enum(
        &m,
        "cs705.diagnostic",
        "Kind",
        &[
            "Invalid",
            "Unsupported",
            "Limit",
            "Conversion",
            "SourceUnavailable",
            "Operation",
            "ChecksumMismatch",
            "RowIndex",
            "UnsupportedCommand",
            "AddressOverflow",
            "SourceBounds",
            "Rejected",
            "UnsupportedLanguage",
            "UnregisteredField",
            "InvalidIndex",
            "InvalidSymbolWidth",
            "AmbiguousField",
        ],
    )?;
    m.add_class::<Diagnostic>()?;
    root.add_submodule(&m)
}
