//! CSV channel operations use the shared live ICF owner.
use super::*;
use crate::icf::Icf;
use crate::icf::{context::Context as IcfContext, frequency::Ranges, record::DecodedRecord};
use cs705_core::csv::channel as c;

pub(crate) fn kind(value: &Bound<'_, PyAny>) -> PyResult<c::Kind> {
    Ok(match enum_index(value, "cs705.csv.channel", "Kind")? {
        0 => c::Kind::Memory,
        1 => c::Kind::FixedPair,
        2 => c::Kind::AlternatingPair,
        3 => c::Kind::Scan,
        4 => c::Kind::Callsign,
        5 => c::Kind::UserEdge,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "invalid channel Kind",
            ));
        }
    })
}
fn kind_result(py: Python<'_>, value: c::Kind) -> PyResult<Bound<'_, PyAny>> {
    enum_result(py, "cs705.csv.channel", "Kind", value as usize)
}
#[pyfunction]
fn from_context<'py>(py: Python<'py>, context: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    kind_result(
        py,
        c::Kind::from_context(strict_u32(context)?).map_err(crate::errors::csv)?,
    )
}

#[pyclass(module = "cs705.csv.channel")]
#[derive(Clone, Copy)]
pub struct Selection {
    pub(crate) inner: c::Selection,
}
#[pymethods]
impl Selection {
    #[setter]
    fn set_first(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.first = strict_u32(value)?;
        Ok(())
    }
    #[setter]
    fn set_last(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.last = strict_u32(value)?;
        Ok(())
    }
    #[setter]
    fn set_number_base(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.number_base = strict_u32(value)?;
        Ok(())
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
    #[new]
    fn new(
        first: &Bound<'_, PyAny>,
        last: &Bound<'_, PyAny>,
        number_base: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: c::Selection {
                first: strict_u32(first)?,
                last: strict_u32(last)?,
                number_base: strict_u32(number_base)?,
            },
        })
    }
    #[getter]
    fn first(&self) -> u32 {
        self.inner.first
    }
    #[getter]
    fn last(&self) -> u32 {
        self.inner.last
    }
    #[getter]
    fn number_base(&self) -> u32 {
        self.inner.number_base
    }
}
macro_rules! native_error {
    ($name:ident, $ty:ty, $accessor:ident, $raise:path) => {
        #[derive(Clone)]
        pub struct $name(pub $ty);
        impl<'a, 'py> FromPyObject<'a, 'py> for $name {
            type Error = PyErr;
            fn extract(value: pyo3::Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
                let diagnostic = crate::error_values::diagnostic_from_value(&value)?;
                let diagnostic = diagnostic.try_borrow()?;
                diagnostic.$accessor().map(Self).ok_or_else(|| {
                    pyo3::exceptions::PyTypeError::new_err(
                        "native error belongs to a different domain",
                    )
                })
            }
        }
        impl<'py> IntoPyObject<'py> for &$name {
            type Target = PyAny;
            type Output = Bound<'py, PyAny>;
            type Error = PyErr;
            fn into_pyobject(self, py: Python<'py>) -> PyResult<Self::Output> {
                Ok($raise(self.0.clone())
                    .into_value(py)
                    .into_bound(py)
                    .into_any())
            }
        }
        impl<'py> IntoPyObject<'py> for $name {
            type Target = PyAny;
            type Output = Bound<'py, PyAny>;
            type Error = PyErr;
            fn into_pyobject(self, py: Python<'py>) -> PyResult<Self::Output> {
                (&self).into_pyobject(py)
            }
        }
    };
}
native_error!(CsvError, core::Error, csv_error, crate::errors::csv);
native_error!(
    ImageError,
    cs705_core::icf::Error,
    icf_error,
    crate::errors::icf
);
#[pyclass(module = "cs705.csv.channel", frozen)]
pub enum Failure {
    Text { error: CsvError },
    Image { error: ImageError },
}
impl Failure {
    fn from_core(value: c::Failure) -> Self {
        match value {
            c::Failure::Text(error) => Self::Text {
                error: CsvError(error),
            },
            c::Failure::Image(error) => Self::Image {
                error: ImageError(error),
            },
        }
    }
    fn native(&self) -> c::Failure {
        match self {
            Self::Text { error } => c::Failure::Text(error.0.clone()),
            Self::Image { error } => c::Failure::Image(error.0.clone()),
        }
    }
}
#[pymethods]
impl Failure {
    fn __eq__(&self, other: &Self) -> bool {
        self.native() == other.native()
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self {
            Self::Text { .. } => "Text",
            Self::Image { .. } => "Image",
        }
    }
    #[getter]
    fn error(&self, py: Python<'_>) -> Py<PyAny> {
        let err = match self {
            Self::Text { error } => crate::errors::csv(error.0.clone()),
            Self::Image { error } => crate::errors::icf(error.0.clone()),
        };
        err.into_value(py).into_any()
    }
}
#[pyclass(module = "cs705.csv.channel")]
pub struct Import {
    counts: Py<super::control::Counts>,
    rows_read: usize,
    applied: usize,
    failure: Option<c::Failure>,
}
impl Import {
    fn from_core(py: Python<'_>, value: c::Import) -> PyResult<Self> {
        Ok(Self {
            counts: Py::new(py, super::control::Counts::from_core(value.counts))?,
            rows_read: value.rows_read,
            applied: value.applied,
            failure: value.failure,
        })
    }
}
#[pymethods]
impl Import {
    #[new]
    fn new(
        counts: Py<super::control::Counts>,
        rows_read: &Bound<'_, PyAny>,
        applied: &Bound<'_, PyAny>,
        failure: Option<&Failure>,
    ) -> PyResult<Self> {
        Ok(Self {
            counts,
            rows_read: strict_usize(rows_read)?,
            applied: strict_usize(applied)?,
            failure: failure.map(Failure::native),
        })
    }
    #[setter]
    fn set_counts(&mut self, value: Py<super::control::Counts>) {
        self.counts = value;
    }
    #[setter]
    fn set_rows_read(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.rows_read = strict_usize(value)?;
        Ok(())
    }
    #[setter]
    fn set_applied(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.applied = strict_usize(value)?;
        Ok(())
    }
    #[setter]
    fn set_failure(&mut self, value: Option<&Failure>) {
        self.failure = value.map(Failure::native);
    }
    #[getter]
    fn counts(&self, py: Python<'_>) -> Py<super::control::Counts> {
        self.counts.clone_ref(py)
    }
    #[getter]
    fn rows_read(&self) -> usize {
        self.rows_read
    }
    #[getter]
    fn applied(&self) -> usize {
        self.applied
    }
    #[getter]
    fn failure(&self) -> Option<Failure> {
        self.failure.clone().map(Failure::from_core)
    }
}
#[pyclass(module = "cs705.csv.channel")]
pub struct Export {
    inner: c::Export,
}
#[pymethods]
impl Export {
    #[new]
    fn new(
        bytes: &Bound<'_, PyAny>,
        rows: &Bound<'_, PyAny>,
        failure: Option<&Failure>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: c::Export {
                bytes: strict_bytes(bytes)?.to_vec(),
                rows: strict_usize(rows)?,
                failure: failure.map(Failure::native),
            },
        })
    }
    #[setter]
    fn set_bytes(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.bytes = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[setter]
    fn set_rows(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.rows = strict_usize(value)?;
        Ok(())
    }
    #[setter]
    fn set_failure(&mut self, value: Option<&Failure>) {
        self.inner.failure = value.map(Failure::native);
    }
    #[getter]
    fn bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.bytes)
    }
    #[getter]
    fn rows(&self) -> usize {
        self.inner.rows
    }
    #[getter]
    fn failure(&self) -> Option<Failure> {
        self.inner.failure.clone().map(Failure::from_core)
    }
}
#[pyclass(module = "cs705.csv.channel")]
pub struct MemoryRows {
    inner: c::MemoryRows<'static>,
}
#[pymethods]
impl MemoryRows {
    #[new]
    fn new(
        source: &Bound<'_, PyAny>,
        kind: &Bound<'_, PyAny>,
        locale: &Locale,
        whole: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: c::MemoryRows::new(
                strict_bytes(source)?,
                self::kind(kind)?,
                locale.native()?,
                strict_bool(whole)?,
            )
            .map_err(crate::errors::csv)?
            .into_owned(),
        })
    }
    fn binding<'py>(
        &self,
        py: Python<'py>,
        call: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        crate::scoped::Frame::with_ref(py, self.inner.binding(), |loan| {
            call.call1((super::dispatch::Binding::from_loan(loan),))
        })
    }
    fn kind<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        kind_result(py, self.inner.kind())
    }
    fn sequential(&self) -> bool {
        self.inner.sequential()
    }
    fn next_source<'py>(&mut self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.inner.next_source().map(|v| PyBytes::new(py, v))
    }
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__<'py>(&mut self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.next_source(py)
    }
    fn into_owned(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
}
struct MemoryOptionsData {
    locale: core::Locale,
    crt_decimal: u8,
    generation: Vec<u8>,
    ranges: Option<Py<Ranges>>,
    live_q: Py<IcfContext>,
    system_langid: Option<u16>,
}
impl MemoryOptionsData {
    fn with_options<T>(
        &self,
        py: Python<'_>,
        call: impl FnOnce(&c::MemoryOptions<'_>) -> PyResult<T>,
    ) -> PyResult<T> {
        let q = self.live_q.try_borrow(py)?.with_ref(|q| Ok(*q))?;
        let run = |ranges: Option<&cs705_core::icf::frequency::Ranges>| {
            call(&c::MemoryOptions {
                locale: self.locale,
                crt_decimal: self.crt_decimal,
                generation: &self.generation,
                ranges,
                live_q: &q,
                system_langid: self.system_langid,
            })
        };
        match &self.ranges {
            Some(r) => r.borrow(py).with_ref(|r| run(Some(r))),
            None => run(None),
        }
    }
}
#[pyclass(module = "cs705.csv.channel", frozen)]
pub struct MemoryOptions {
    inner: crate::clone::Handle<MemoryOptionsData>,
}
impl MemoryOptions {
    fn with_options<T>(
        &self,
        py: Python<'_>,
        call: impl FnOnce(&c::MemoryOptions<'_>) -> PyResult<T>,
    ) -> PyResult<T> {
        self.inner.with(|v| v.with_options(py, call))
    }
}
#[pymethods]
impl MemoryOptions {
    #[setter]
    fn set_locale(&self, value: &Locale) -> PyResult<()> {
        let value = value.native()?;
        self.inner.edit(|v| {
            v.locale = value;
            Ok(())
        })
    }
    #[setter]
    fn set_crt_decimal(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u8(value)?;
        self.inner.with(|v| {
            v.crt_decimal = value;
            Ok(())
        })
    }
    #[setter]
    fn set_generation(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bytes(value)?.to_vec();
        self.inner.with(|v| {
            v.generation = value;
            Ok(())
        })
    }
    #[setter]
    fn set_ranges(&self, value: Option<Py<Ranges>>) -> PyResult<()> {
        self.inner.with(|v| {
            v.ranges = value;
            Ok(())
        })
    }
    #[setter]
    fn set_live_q(&self, value: Py<IcfContext>) -> PyResult<()> {
        self.inner.with(|v| {
            v.live_q = value;
            Ok(())
        })
    }
    #[setter]
    fn set_system_langid(&self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let value = value.map(strict_u16).transpose()?;
        self.inner.with(|v| {
            v.system_langid = value;
            Ok(())
        })
    }
    #[new]
    #[pyo3(signature = (locale, crt_decimal, generation, ranges, live_q, system_langid))]
    fn new(
        locale: &Locale,
        crt_decimal: &Bound<'_, PyAny>,
        generation: &Bound<'_, PyAny>,
        ranges: Option<Py<Ranges>>,
        live_q: Py<IcfContext>,
        system_langid: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: crate::clone::Handle::new(MemoryOptionsData {
                locale: locale.native()?,
                crt_decimal: strict_u8(crt_decimal)?,
                generation: strict_bytes(generation)?.to_vec(),
                ranges,
                live_q,
                system_langid: system_langid.map(strict_u16).transpose()?,
            }),
        })
    }
    #[getter]
    fn locale(&self) -> Locale {
        Locale::from_handle(self.inner.project(|v| Ok(&mut v.locale)))
    }
    #[getter]
    fn crt_decimal(&self) -> PyResult<u8> {
        self.inner.with(|v| Ok(v.crt_decimal))
    }
    #[getter]
    fn generation<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.inner.with(|v| Ok(PyBytes::new(py, &v.generation)))
    }
    #[getter]
    fn ranges(&self, py: Python<'_>) -> PyResult<Option<Py<Ranges>>> {
        self.inner
            .with(|v| Ok(v.ranges.as_ref().map(|r| r.clone_ref(py))))
    }
    #[getter]
    fn live_q(&self, py: Python<'_>) -> PyResult<Py<IcfContext>> {
        self.inner.with(|v| Ok(v.live_q.clone_ref(py)))
    }
    #[getter]
    fn system_langid(&self) -> PyResult<Option<u16>> {
        self.inner.with(|v| Ok(v.system_langid))
    }
}
#[pyclass(module = "cs705.csv.channel")]
pub struct MemoryImport {
    report: Py<Import>,
    home_invalidated: bool,
    call_invalidated: bool,
}
#[pymethods]
impl MemoryImport {
    #[new]
    fn new(
        report: Py<Import>,
        home_invalidated: &Bound<'_, PyAny>,
        call_invalidated: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            report,
            home_invalidated: strict_bool(home_invalidated)?,
            call_invalidated: strict_bool(call_invalidated)?,
        })
    }
    #[setter]
    fn set_report(&mut self, value: Py<Import>) {
        self.report = value;
    }
    #[setter]
    fn set_home_invalidated(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.home_invalidated = strict_bool(value)?;
        Ok(())
    }
    #[setter]
    fn set_call_invalidated(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.call_invalidated = strict_bool(value)?;
        Ok(())
    }
    #[getter]
    fn report(&self, py: Python<'_>) -> Py<Import> {
        self.report.clone_ref(py)
    }
    #[getter]
    fn home_invalidated(&self) -> bool {
        self.home_invalidated
    }
    #[getter]
    fn call_invalidated(&self) -> bool {
        self.call_invalidated
    }
}
#[pyclass(module = "cs705.csv.channel")]
pub struct EdgeRow {
    holder: Py<DecodedRecord>,
    position: u32,
    discard: u32,
    corrections: u32,
}
impl EdgeRow {
    fn from_core(py: Python<'_>, value: c::EdgeRow) -> PyResult<Self> {
        Ok(Self {
            holder: Py::new(py, DecodedRecord::from_core(value.holder))?,
            position: value.position,
            discard: value.discard,
            corrections: value.corrections,
        })
    }
}
#[pymethods]
impl EdgeRow {
    #[new]
    fn new(
        holder: Py<DecodedRecord>,
        position: &Bound<'_, PyAny>,
        discard: &Bound<'_, PyAny>,
        corrections: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            holder,
            position: strict_u32(position)?,
            discard: strict_u32(discard)?,
            corrections: strict_u32(corrections)?,
        })
    }
    #[setter]
    fn set_holder(&mut self, value: Py<DecodedRecord>) {
        self.holder = value;
    }
    #[setter]
    fn set_position(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.position = strict_u32(value)?;
        Ok(())
    }
    #[setter]
    fn set_discard(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.discard = strict_u32(value)?;
        Ok(())
    }
    #[setter]
    fn set_corrections(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.corrections = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn holder(&self, py: Python<'_>) -> Py<DecodedRecord> {
        self.holder.clone_ref(py)
    }
    #[getter]
    fn position(&self) -> u32 {
        self.position
    }
    #[getter]
    fn discard(&self) -> u32 {
        self.discard
    }
    #[getter]
    fn corrections(&self) -> u32 {
        self.corrections
    }
}
#[pyclass(module = "cs705.csv.channel")]
pub struct UserEdgeRows {
    inner: c::UserEdgeRows<'static>,
}
#[pymethods]
impl UserEdgeRows {
    #[new]
    fn new(
        source: &Bound<'_, PyAny>,
        locale: &Locale,
        selection: &Selection,
        whole: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: c::UserEdgeRows::new(
                strict_bytes(source)?,
                locale.native()?,
                selection.inner,
                strict_bool(whole)?,
            )
            .map_err(crate::errors::csv)?
            .into_owned(),
        })
    }
    fn next_row(&mut self, py: Python<'_>) -> PyResult<Option<EdgeRow>> {
        self.inner
            .next_row()
            .transpose()
            .map_err(crate::errors::csv)?
            .map(|r| EdgeRow::from_core(py, r))
            .transpose()
    }
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<EdgeRow>> {
        self.next_row(py)
    }
    fn into_owned(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
}
#[pyclass(module = "cs705.csv.channel")]
pub struct ScanRows {
    inner: c::ScanRows<'static>,
}
#[pymethods]
impl ScanRows {
    #[new]
    fn new(
        source: &Bound<'_, PyAny>,
        locale: &Locale,
        selection: &Selection,
        whole: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: c::ScanRows::new(
                strict_bytes(source)?,
                locale.native()?,
                selection.inner,
                strict_bool(whole)?,
            )
            .map_err(crate::errors::csv)?
            .into_owned(),
        })
    }
    fn next_with(
        &mut self,
        py: Python<'_>,
        default: &DecodedRecord,
        crt_decimal: &Bound<'_, PyAny>,
    ) -> PyResult<Option<EdgeRow>> {
        let point = strict_u8(crt_decimal)?;
        default
            .with_ref(|d| {
                self.inner
                    .next_with(d, point)
                    .transpose()
                    .map_err(crate::errors::csv)
            })?
            .map(|r| EdgeRow::from_core(py, r))
            .transpose()
    }
    fn applied(&mut self) {
        self.inner.applied();
    }
    fn into_owned(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
}
#[pyclass(module = "cs705.csv.channel")]
pub struct CallsignRow {
    holder: Py<DecodedRecord>,
    accepted: bool,
    corrections: Option<u32>,
}
#[pymethods]
impl CallsignRow {
    #[new]
    fn new(
        holder: Py<DecodedRecord>,
        accepted: &Bound<'_, PyAny>,
        corrections: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            holder,
            accepted: strict_bool(accepted)?,
            corrections: corrections.map(strict_u32).transpose()?,
        })
    }
    #[setter]
    fn set_holder(&mut self, value: Py<DecodedRecord>) {
        self.holder = value;
    }
    #[setter]
    fn set_accepted(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.accepted = strict_bool(value)?;
        Ok(())
    }
    #[setter]
    fn set_corrections(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.corrections = value.map(strict_u32).transpose()?;
        Ok(())
    }
    #[getter]
    fn holder(&self, py: Python<'_>) -> Py<DecodedRecord> {
        self.holder.clone_ref(py)
    }
    #[getter]
    fn accepted(&self) -> bool {
        self.accepted
    }
    #[getter]
    fn corrections(&self) -> Option<u32> {
        self.corrections
    }
}
#[pyclass(module = "cs705.csv.channel")]
pub struct CallsignRows {
    inner: c::CallsignRows<'static>,
}
#[pymethods]
impl CallsignRows {
    #[new]
    fn new(source: &Bound<'_, PyAny>, locale: &Locale) -> PyResult<Self> {
        Ok(Self {
            inner: c::CallsignRows::new(strict_bytes(source)?, locale.native()?)
                .map_err(crate::errors::csv)?
                .into_owned(),
        })
    }
    fn binding<'py>(
        &self,
        py: Python<'py>,
        call: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        crate::scoped::Frame::with_ref(py, self.inner.binding(), |loan| {
            call.call1((super::dispatch::Binding::from_loan(loan),))
        })
    }
    fn next_with(
        &mut self,
        py: Python<'_>,
        default: &DecodedRecord,
        native: &Bound<'_, PyAny>,
    ) -> PyResult<Option<CallsignRow>> {
        let mut errors = crate::callbacks::CallbackError::default();
        let result = default.with_ref(|d| {
            self.inner
                .next_with(d, |s, d| {
                    callback(&mut errors, super::nls::map_call(py, native, s, d))
                })
                .transpose()
                .map_err(crate::errors::csv)
        });
        errors
            .finish(py, result)?
            .map(|r| {
                Ok(CallsignRow {
                    holder: Py::new(py, DecodedRecord::from_core(r.holder))?,
                    accepted: r.accepted,
                    corrections: r.corrections,
                })
            })
            .transpose()
    }
    fn into_owned(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
}
#[pyfunction]
fn memory_default(
    kind: &Bound<'_, PyAny>,
    position: Option<&Bound<'_, PyAny>>,
    frequency: &Bound<'_, PyAny>,
    generation: &Bound<'_, PyAny>,
    ranges: &Ranges,
    system_langid: Option<&Bound<'_, PyAny>>,
) -> PyResult<DecodedRecord> {
    let kind = self::kind(kind)?;
    let position = position.map(strict_u32).transpose()?;
    let frequency = strict_u32(frequency)?;
    let generation = strict_bytes(generation)?;
    let langid = system_langid.map(strict_u16).transpose()?;
    ranges.with_ref(|r| {
        c::memory_default(kind, position, frequency, generation, r, langid)
            .map(DecodedRecord::from_core)
            .map_err(|e| match e {
                c::Failure::Text(e) => crate::errors::csv(e),
                c::Failure::Image(e) => crate::errors::icf(e),
            })
    })
}
#[pyfunction]
fn import_memory(
    py: Python<'_>,
    image: &Icf,
    source: &Bound<'_, PyAny>,
    kind: &Bound<'_, PyAny>,
    group: Option<&Bound<'_, PyAny>>,
    selection: Option<&Selection>,
    options: &MemoryOptions,
) -> PyResult<MemoryImport> {
    let source = strict_bytes(source)?;
    let kind = self::kind(kind)?;
    let group = group.map(strict_usize).transpose()?;
    let value = image.with_mut(|i| {
        options.with_options(py, |o| {
            Ok(c::import_memory(
                i,
                source,
                kind,
                group,
                selection.map(|s| s.inner),
                o,
            ))
        })
    })?;
    Ok(MemoryImport {
        report: Py::new(py, Import::from_core(py, value.report)?)?,
        home_invalidated: value.home_invalidated,
        call_invalidated: value.call_invalidated,
    })
}

#[pyfunction]
fn import_user_edge(
    py: Python<'_>,
    image: &Icf,
    source: &Bound<'_, PyAny>,
    selection: &Selection,
    whole: &Bound<'_, PyAny>,
    locale: &Locale,
) -> PyResult<Import> {
    let source = strict_bytes(source)?;
    let whole = strict_bool(whole)?;
    image.with_mut(|i| {
        Import::from_core(
            py,
            c::import_user_edge(i, source, selection.inner, whole, locale.native()?),
        )
    })
}
#[pyfunction]
fn import_scan(
    py: Python<'_>,
    image: &Icf,
    source: &Bound<'_, PyAny>,
    selection: &Selection,
    whole: &Bound<'_, PyAny>,
    locale: &Locale,
    defaults: Option<&Icf>,
) -> PyResult<Import> {
    let source = strict_bytes(source)?;
    let whole = strict_bool(whole)?;
    image.with_mut(|i| match defaults {
        Some(d) => d.with_ref(|d| {
            Import::from_core(
                py,
                c::import_scan(i, source, selection.inner, whole, locale.native()?, Some(d)),
            )
        }),
        None => Import::from_core(
            py,
            c::import_scan(i, source, selection.inner, whole, locale.native()?, None),
        ),
    })
}
#[pyfunction]
fn import_callsign(
    py: Python<'_>,
    image: &Icf,
    source: &Bound<'_, PyAny>,
    selection: Option<&Selection>,
    locale: &Locale,
    defaults: &Icf,
) -> PyResult<Import> {
    let source = strict_bytes(source)?;
    image.with_mut(|i| {
        defaults.with_ref(|d| {
            Import::from_core(
                py,
                c::import_callsign(i, source, selection.map(|s| s.inner), locale.native()?, d),
            )
        })
    })
}
#[pyfunction]
fn export_user_edge(image: &Icf, selection: &Selection, locale: &Locale) -> PyResult<Export> {
    image.with_ref(|i| {
        Ok(Export {
            inner: c::export_user_edge(i, selection.inner, locale.native()?),
        })
    })
}
#[pyfunction]
fn export_scan(image: &Icf, selection: &Selection, locale: &Locale) -> PyResult<Export> {
    image.with_ref(|i| {
        Ok(Export {
            inner: c::export_scan(i, selection.inner, locale.native()?),
        })
    })
}
#[pyfunction]
fn export_callsign(
    image: &Icf,
    selection: Option<&Selection>,
    locale: &Locale,
) -> PyResult<Export> {
    image.with_ref(|i| {
        Ok(Export {
            inner: c::export_callsign(i, selection.map(|s| s.inner), locale.native()?),
        })
    })
}
#[pyfunction]
fn export_memory(
    image: &Icf,
    kind: &Bound<'_, PyAny>,
    group: &Bound<'_, PyAny>,
    selection: Option<&Selection>,
    locale: &Locale,
) -> PyResult<Export> {
    let kind = self::kind(kind)?;
    let group = if group.is_none() {
        None
    } else {
        Some(strict_usize(group)?)
    };
    image.with_ref(|i| {
        Ok(Export {
            inner: c::export_memory(i, kind, group, selection.map(|s| s.inner), locale.native()?),
        })
    })
}
#[pyfunction]
fn export_receive_history(
    image: &Icf,
    selection: Option<&Selection>,
    locale: &Locale,
    datetime: &super::datetime::Context,
) -> PyResult<Export> {
    image.with_ref(|i| {
        datetime.with_context(|d| {
            Ok(Export {
                inner: c::export_receive_history(
                    i,
                    selection.map(|s| s.inner),
                    locale.native()?,
                    d,
                ),
            })
        })
    })
}
#[pyfunction]
fn export_history(
    image: &Icf,
    kind: &Bound<'_, PyAny>,
    selection: Option<&Selection>,
    locale: &Locale,
    defaults: &crate::repeater::working::DefaultContext,
) -> PyResult<Export> {
    let kind = strict_u32(kind)?;
    image.with_ref(|i| {
        Ok(Export {
            inner: c::export_history(
                i,
                kind,
                selection.map(|s| s.inner),
                locale.native()?,
                defaults.inner,
            ),
        })
    })
}
#[pyfunction]
fn export_receive_history_with_date(
    py: Python<'_>,
    image: &Icf,
    selection: Option<&Selection>,
    locale: &Locale,
    datetime: &super::datetime::Context,
    native: &Bound<'_, PyAny>,
) -> PyResult<Py<Export>> {
    let mut errors = crate::callbacks::CallbackError::default();
    let value = image.with_ref(|i| {
        datetime.with_context(|d| {
            Ok(c::export_receive_history_with_date(
                i,
                selection.map(|s| s.inner),
                locale.native()?,
                d,
                |inner| {
                    callback(
                        &mut errors,
                        native
                            .call1((super::datetime::DateToken::from_core(inner),))
                            .and_then(|v| Ok(strict_bytes(&v)?.to_vec())),
                    )
                },
            ))
        })
    })?;
    let value = Py::new(py, Export { inner: value })?;
    errors.finish_with_outcome(py, Ok(value.clone_ref(py)), value.into_any())
}

pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "channel")?;
    add_enum(
        &m,
        "Kind",
        &[
            "Memory",
            "FixedPair",
            "AlternatingPair",
            "Scan",
            "Callsign",
            "UserEdge",
        ],
    )?;
    m.add_function(wrap_pyfunction!(from_context, &m)?)?;
    m.getattr("Kind")?.setattr(
        "from_context",
        root.py()
            .import("builtins")?
            .getattr("staticmethod")?
            .call1((m.getattr("from_context")?,))?,
    )?;
    m.add_class::<Selection>()?;
    m.add_class::<Failure>()?;
    m.add_class::<Import>()?;
    m.add_class::<Export>()?;
    m.add_class::<MemoryRows>()?;
    m.add_class::<MemoryOptions>()?;
    m.add_class::<MemoryImport>()?;
    m.add_class::<EdgeRow>()?;
    m.add_class::<UserEdgeRows>()?;
    m.add_class::<ScanRows>()?;
    m.add_class::<CallsignRow>()?;
    m.add_class::<CallsignRows>()?;
    m.add_function(wrap_pyfunction!(memory_default, &m)?)?;
    m.add_function(wrap_pyfunction!(import_memory, &m)?)?;
    m.add_function(wrap_pyfunction!(import_user_edge, &m)?)?;
    m.add_function(wrap_pyfunction!(import_scan, &m)?)?;
    m.add_function(wrap_pyfunction!(import_callsign, &m)?)?;
    m.add_function(wrap_pyfunction!(export_user_edge, &m)?)?;
    m.add_function(wrap_pyfunction!(export_scan, &m)?)?;
    m.add_function(wrap_pyfunction!(export_callsign, &m)?)?;
    m.add_function(wrap_pyfunction!(export_memory, &m)?)?;
    m.add_function(wrap_pyfunction!(export_receive_history, &m)?)?;
    m.add_function(wrap_pyfunction!(export_history, &m)?)?;
    m.add_function(wrap_pyfunction!(export_receive_history_with_date, &m)?)?;
    root.add_submodule(&m)
}
