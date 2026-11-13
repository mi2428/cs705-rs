//! Typed decoded-holder and per-field native formatting requests.
use super::*;
use cs705_core::csv::payload as p;

pub(crate) const KINDS: [p::Kind; 11] = [
    p::Kind::Memory,
    p::Kind::Scan,
    p::Kind::Gps,
    p::Kind::ReceiveHistory,
    p::Kind::FromHistory,
    p::Kind::FromDataHistory,
    p::Kind::ToHistory,
    p::Kind::Reflector,
    p::Kind::UserEdge,
    p::Kind::Repeater,
    p::Kind::Callsign,
];
pub(crate) const ADAPTERS: [p::Adapter; 29] = [
    p::Adapter::Selection,
    p::Adapter::Duplex,
    p::Adapter::Mode,
    p::Adapter::Data,
    p::Adapter::Filter,
    p::Adapter::Tone,
    p::Adapter::ToneFrequency,
    p::Adapter::Dtcs,
    p::Adapter::DtcsPolarity,
    p::Adapter::DigitalSquelch,
    p::Adapter::Split,
    p::Adapter::Group,
    p::Adapter::Alarm,
    p::Adapter::RepeaterDuplex,
    p::Adapter::RepeaterMode,
    p::Adapter::RepeaterUse,
    p::Adapter::Position,
    p::Adapter::UtcOffsetSentinel,
    p::Adapter::RxType,
    p::Adapter::RxStatus,
    p::Adapter::Symbol,
    p::Adapter::Directivity,
    p::Adapter::DataType,
    p::Adapter::GpsDate,
    p::Adapter::GpsTime,
    p::Adapter::HistoryDate,
    p::Adapter::Timestamp,
    p::Adapter::Message,
    p::Adapter::GpsMessage,
];
pub(crate) fn kind(value: &Bound<'_, PyAny>) -> PyResult<p::Kind> {
    KINDS
        .get(enum_index(value, "cs705.csv.payload", "Kind")?)
        .copied()
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid payload Kind"))
}
pub(crate) fn adapter(value: &Bound<'_, PyAny>) -> PyResult<p::Adapter> {
    ADAPTERS
        .get(enum_index(value, "cs705.csv.payload", "Adapter")?)
        .copied()
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid Adapter"))
}
#[pyclass(module = "cs705.csv.payload", frozen)]
pub struct Holder {
    bytes: Vec<u8>,
}
impl Holder {
    pub(crate) fn native(&self) -> Result<p::Holder<'_>, core::Error> {
        p::Holder::new(&self.bytes)
    }
}
#[pymethods]
impl Holder {
    #[new]
    fn new(bytes: &Bound<'_, PyAny>) -> PyResult<Self> {
        let bytes = strict_bytes(bytes)?;
        p::Holder::new(bytes).map_err(crate::errors::csv)?;
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }
}
#[pyclass(module = "cs705.csv.payload")]
pub struct Context {
    separator: u8,
    decimal: u8,
    row: u32,
    number_base: u32,
    group: u32,
    group_name: Vec<u8>,
    alarm: bool,
    utc_offset_minutes: i16,
    memory_group_columns: bool,
    memory_selection_column: bool,
    memory_pair: bool,
}
impl Context {
    pub(crate) fn native(&self) -> p::Context<'_> {
        p::Context {
            separator: self.separator,
            decimal: self.decimal,
            row: self.row,
            number_base: self.number_base,
            group: self.group,
            group_name: &self.group_name,
            alarm: self.alarm,
            utc_offset_minutes: self.utc_offset_minutes,
            memory_group_columns: self.memory_group_columns,
            memory_selection_column: self.memory_selection_column,
            memory_pair: self.memory_pair,
        }
    }
}
#[pymethods]
impl Context {
    #[setter]
    fn set_separator(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.separator = strict_u8(value)?;
        Ok(())
    }
    #[setter]
    fn set_decimal(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.decimal = strict_u8(value)?;
        Ok(())
    }
    #[setter]
    fn set_row(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.row = strict_u32(value)?;
        Ok(())
    }
    #[setter]
    fn set_number_base(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.number_base = strict_u32(value)?;
        Ok(())
    }
    #[setter]
    fn set_group(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.group = strict_u32(value)?;
        Ok(())
    }
    #[setter]
    fn set_group_name(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.group_name = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[setter]
    fn set_alarm(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.alarm = strict_bool(value)?;
        Ok(())
    }
    #[setter]
    fn set_utc_offset_minutes(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.utc_offset_minutes = strict_i16(value)?;
        Ok(())
    }
    #[setter]
    fn set_memory_group_columns(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.memory_group_columns = strict_bool(value)?;
        Ok(())
    }
    #[setter]
    fn set_memory_selection_column(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.memory_selection_column = strict_bool(value)?;
        Ok(())
    }
    #[setter]
    fn set_memory_pair(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.memory_pair = strict_bool(value)?;
        Ok(())
    }
    #[new]
    #[expect(
        clippy::too_many_arguments,
        reason = "Exposes every native payload Context field without an opaque options argument."
    )]
    fn new(
        separator: &Bound<'_, PyAny>,
        decimal: &Bound<'_, PyAny>,
        row: &Bound<'_, PyAny>,
        number_base: &Bound<'_, PyAny>,
        group: &Bound<'_, PyAny>,
        group_name: &Bound<'_, PyAny>,
        alarm: &Bound<'_, PyAny>,
        utc_offset_minutes: &Bound<'_, PyAny>,
        memory_group_columns: &Bound<'_, PyAny>,
        memory_selection_column: &Bound<'_, PyAny>,
        memory_pair: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            separator: strict_u8(separator)?,
            decimal: strict_u8(decimal)?,
            row: strict_u32(row)?,
            number_base: strict_u32(number_base)?,
            group: strict_u32(group)?,
            group_name: strict_bytes(group_name)?.to_vec(),
            alarm: strict_bool(alarm)?,
            utc_offset_minutes: strict_i16(utc_offset_minutes)?,
            memory_group_columns: strict_bool(memory_group_columns)?,
            memory_selection_column: strict_bool(memory_selection_column)?,
            memory_pair: strict_bool(memory_pair)?,
        })
    }
    #[getter]
    fn separator(&self) -> u8 {
        self.separator
    }
    #[getter]
    fn decimal(&self) -> u8 {
        self.decimal
    }
    #[getter]
    fn row(&self) -> u32 {
        self.row
    }
    #[getter]
    fn number_base(&self) -> u32 {
        self.number_base
    }
    #[getter]
    fn group(&self) -> u32 {
        self.group
    }
    #[getter]
    fn group_name<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.group_name)
    }
    #[getter]
    fn alarm(&self) -> bool {
        self.alarm
    }
    #[getter]
    fn utc_offset_minutes(&self) -> i16 {
        self.utc_offset_minutes
    }
    #[getter]
    fn memory_group_columns(&self) -> bool {
        self.memory_group_columns
    }
    #[getter]
    fn memory_selection_column(&self) -> bool {
        self.memory_selection_column
    }
    #[getter]
    fn memory_pair(&self) -> bool {
        self.memory_pair
    }
}
#[pyclass(module = "cs705.csv.payload")]
pub struct Request {
    adapter: p::Adapter,
    source: Vec<u8>,
    utc_offset_minutes: i16,
}
impl Request {
    pub(crate) fn from_core(value: p::Request<'_>) -> Self {
        Self {
            adapter: value.adapter,
            source: value.source.to_vec(),
            utc_offset_minutes: value.utc_offset_minutes,
        }
    }
    pub(crate) fn native(&self) -> p::Request<'_> {
        p::Request {
            adapter: self.adapter,
            source: &self.source,
            utc_offset_minutes: self.utc_offset_minutes,
        }
    }
}
#[pymethods]
impl Request {
    #[setter]
    fn set_adapter(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.adapter = adapter(value)?;
        Ok(())
    }
    #[setter]
    fn set_source(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.source = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[setter]
    fn set_utc_offset_minutes(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.utc_offset_minutes = strict_i16(value)?;
        Ok(())
    }
    #[new]
    fn new(
        adapter: &Bound<'_, PyAny>,
        source: &Bound<'_, PyAny>,
        utc_offset_minutes: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            adapter: self::adapter(adapter)?,
            source: strict_bytes(source)?.to_vec(),
            utc_offset_minutes: strict_i16(utc_offset_minutes)?,
        })
    }
    #[getter]
    fn adapter<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        enum_result(py, "cs705.csv.payload", "Adapter", self.adapter as usize)
    }
    #[getter]
    fn source<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.source)
    }
    #[getter]
    fn utc_offset_minutes(&self) -> i16 {
        self.utc_offset_minutes
    }
}
#[pyfunction]
fn build<'py>(
    py: Python<'py>,
    kind: &Bound<'_, PyAny>,
    h: &Holder,
    c: &Context,
    native: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let mut errors = crate::callbacks::CallbackError::default();
    let result = p::build(
        self::kind(kind)?,
        h.native().map_err(crate::errors::csv)?,
        c.native(),
        |request| {
            callback(
                &mut errors,
                native
                    .call1((Request::from_core(request),))
                    .and_then(|v| Ok(strict_bytes(&v)?.to_vec())),
            )
        },
    );
    errors
        .finish(py, result.map_err(crate::errors::csv))
        .map(|v| PyBytes::new(py, &v))
}
#[pyfunction]
fn build_portable<'py>(
    py: Python<'py>,
    kind: &Bound<'_, PyAny>,
    h: &Holder,
    c: &Context,
    runtime: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let mut errors = crate::callbacks::CallbackError::default();
    let result = p::build_portable(
        self::kind(kind)?,
        h.native().map_err(crate::errors::csv)?,
        c.native(),
        |request| {
            callback(
                &mut errors,
                runtime
                    .call1((Request::from_core(request),))
                    .and_then(|v| Ok(strict_bytes(&v)?.to_vec())),
            )
        },
    );
    errors
        .finish(py, result.map_err(crate::errors::csv))
        .map(|v| PyBytes::new(py, &v))
}
#[pyfunction]
fn callsign_row<'py>(
    py: Python<'py>,
    image: &crate::icf::Icf,
    index: &Bound<'_, PyAny>,
    c: &Context,
) -> PyResult<Bound<'py, PyBytes>> {
    let index = strict_usize(index)?;
    image.with_ref(|i| {
        Ok(PyBytes::new(
            py,
            &p::callsign_row(i, index, c.native()).map_err(crate::errors::csv)?,
        ))
    })
}
#[pyfunction]
fn utc_text<'py>(py: Python<'py>, minutes: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &p::utc_text(strict_i32(minutes)?).map_err(crate::errors::csv)?,
    ))
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "payload")?;
    add_enum(
        &m,
        "Kind",
        &[
            "Memory",
            "Scan",
            "Gps",
            "ReceiveHistory",
            "FromHistory",
            "FromDataHistory",
            "ToHistory",
            "Reflector",
            "UserEdge",
            "Repeater",
            "Callsign",
        ],
    )?;
    add_enum(
        &m,
        "Adapter",
        &[
            "Selection",
            "Duplex",
            "Mode",
            "Data",
            "Filter",
            "Tone",
            "ToneFrequency",
            "Dtcs",
            "DtcsPolarity",
            "DigitalSquelch",
            "Split",
            "Group",
            "Alarm",
            "RepeaterDuplex",
            "RepeaterMode",
            "RepeaterUse",
            "Position",
            "UtcOffsetSentinel",
            "RxType",
            "RxStatus",
            "Symbol",
            "Directivity",
            "DataType",
            "GpsDate",
            "GpsTime",
            "HistoryDate",
            "Timestamp",
            "Message",
            "GpsMessage",
        ],
    )?;
    m.add_class::<Holder>()?;
    m.add_class::<Context>()?;
    m.add_class::<Request>()?;
    m.add_function(wrap_pyfunction!(build, &m)?)?;
    m.add_function(wrap_pyfunction!(build_portable, &m)?)?;
    m.add_function(wrap_pyfunction!(callsign_row, &m)?)?;
    m.add_function(wrap_pyfunction!(utc_text, &m)?)?;
    root.add_submodule(&m)
}
