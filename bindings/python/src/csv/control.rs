//! Exact native counter values.
use super::*;
use cs705_core::csv::control as c;

#[pyclass(module = "cs705.csv.control")]
pub struct Counts {
    pub(crate) inner: c::Counts,
}
impl Counts {
    pub fn from_core(inner: c::Counts) -> Self {
        Self { inner }
    }
}
#[pymethods]
impl Counts {
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
    #[new]
    fn new(
        s: &Bound<'_, PyAny>,
        c2: &Bound<'_, PyAny>,
        c3: &Bound<'_, PyAny>,
        c4: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: c::Counts {
                s: strict_u32(s)?,
                c2: strict_u32(c2)?,
                c3: strict_u32(c3)?,
                c4: strict_u32(c4)?,
            },
        })
    }
    #[getter]
    fn s(&self) -> u32 {
        self.inner.s
    }
    #[setter]
    fn set_s(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.s = strict_u32(v)?;
        Ok(())
    }
    #[getter]
    fn c2(&self) -> u32 {
        self.inner.c2
    }
    #[setter]
    fn set_c2(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.c2 = strict_u32(v)?;
        Ok(())
    }
    #[getter]
    fn c3(&self) -> u32 {
        self.inner.c3
    }
    #[setter]
    fn set_c3(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.c3 = strict_u32(v)?;
        Ok(())
    }
    #[getter]
    fn c4(&self) -> u32 {
        self.inner.c4
    }
    #[setter]
    fn set_c4(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.c4 = strict_u32(v)?;
        Ok(())
    }
    fn selected_event(
        &mut self,
        code: &Bound<'_, PyAny>,
        delta: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.inner
            .selected_event(strict_u32(code)?, strict_u32(delta)?);
        Ok(())
    }
    fn whole_event(&mut self, code: &Bound<'_, PyAny>, delta: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner
            .whole_event(strict_u32(code)?, strict_u32(delta)?);
        Ok(())
    }
    fn capacity_marker(&mut self, status: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.capacity_marker(strict_u32(status)?);
        Ok(())
    }
    fn finish_selected(&mut self, group_count: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.finish_selected(strict_u32(group_count)?);
        Ok(())
    }
    fn messages(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .messages()
            .into_iter()
            .map(|v| Ok(enum_result(py, "cs705.csv.control", "Message", v as usize)?.unbind()))
            .collect()
    }
}
#[pyclass(module = "cs705.csv.control")]
pub struct WriteResult {
    pub(crate) inner: c::WriteResult,
}
#[pymethods]
impl WriteResult {
    #[setter]
    fn set_bool_value(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.bool_value = strict_u32(value)?;
        Ok(())
    }
    #[setter]
    fn set_bytes_written(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.bytes_written = strict_usize(value)?;
        Ok(())
    }
    #[new]
    fn new(bool_value: &Bound<'_, PyAny>, bytes_written: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: c::WriteResult {
                bool_value: strict_u32(bool_value)?,
                bytes_written: strict_usize(bytes_written)?,
            },
        })
    }
    #[getter]
    fn bool_value(&self) -> u32 {
        self.inner.bool_value
    }
    #[getter]
    fn bytes_written(&self) -> usize {
        self.inner.bytes_written
    }
}
#[pyfunction]
fn pair_correction(
    upper: &Bound<'_, PyAny>,
    lower_valid: &Bound<'_, PyAny>,
    lower_c2: &Bound<'_, PyAny>,
    lower_c3: &Bound<'_, PyAny>,
    old_split: &Bound<'_, PyAny>,
) -> PyResult<u32> {
    Ok(c::pair_correction(
        strict_u32(upper)?,
        strict_bool(lower_valid)?,
        strict_u32(lower_c2)?,
        strict_u32(lower_c3)?,
        strict_bool(old_split)?,
    ))
}
#[pyclass(module = "cs705.csv.control")]
pub struct ScanRow {
    inner: c::ScanRow,
}
#[pymethods]
impl ScanRow {
    #[setter]
    fn set_position(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.position = strict_u32(value)?;
        Ok(())
    }
    #[setter]
    fn set_valid(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.valid = strict_bool(value)?;
        Ok(())
    }
    #[setter]
    fn set_c2_delta(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.c2_delta = strict_u32(value)?;
        Ok(())
    }
    #[setter]
    fn set_c3_delta(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.c3_delta = strict_u32(value)?;
        Ok(())
    }
    #[new]
    fn new(
        position: &Bound<'_, PyAny>,
        valid: &Bound<'_, PyAny>,
        c2_delta: &Bound<'_, PyAny>,
        c3_delta: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: c::ScanRow {
                position: strict_u32(position)?,
                valid: strict_bool(valid)?,
                c2_delta: strict_u32(c2_delta)?,
                c3_delta: strict_u32(c3_delta)?,
            },
        })
    }
    #[getter]
    fn position(&self) -> u32 {
        self.inner.position
    }
    #[getter]
    fn valid(&self) -> bool {
        self.inner.valid
    }
    #[getter]
    fn c2_delta(&self) -> u32 {
        self.inner.c2_delta
    }
    #[getter]
    fn c3_delta(&self) -> u32 {
        self.inner.c3_delta
    }
}
#[pyclass(module = "cs705.csv.control")]
pub struct ScanRange {
    inner: c::ScanRange,
}
#[pymethods]
impl ScanRange {
    #[setter]
    fn set_base(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.base = strict_u32(value)?;
        Ok(())
    }
    #[setter]
    fn set_last(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.last = strict_i32(value)?;
        Ok(())
    }
    #[setter]
    fn set_stop_on_excess(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.stop_on_excess = strict_bool(value)?;
        Ok(())
    }
    #[new]
    fn new(
        base: &Bound<'_, PyAny>,
        last: &Bound<'_, PyAny>,
        stop_on_excess: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: c::ScanRange {
                base: strict_u32(base)?,
                last: strict_i32(last)?,
                stop_on_excess: strict_bool(stop_on_excess)?,
            },
        })
    }
    #[getter]
    fn base(&self) -> u32 {
        self.inner.base
    }
    #[getter]
    fn last(&self) -> i32 {
        self.inner.last
    }
    #[getter]
    fn stop_on_excess(&self) -> bool {
        self.inner.stop_on_excess
    }
}
#[pyclass(module = "cs705.csv.control", frozen)]
pub enum ImportGate {
    FileCancelled {},
    InnerCancelled {},
    Header { status: CheckedByte },
}
impl ImportGate {
    fn native(&self) -> c::ImportGate {
        match self {
            Self::FileCancelled {} => c::ImportGate::FileCancelled,
            Self::InnerCancelled {} => c::ImportGate::InnerCancelled,
            Self::Header { status } => c::ImportGate::Header(status.0),
        }
    }
}
#[pymethods]
impl ImportGate {
    fn __eq__(&self, other: &Self) -> bool {
        self.native() == other.native()
    }
    fn reaches_rows(&self) -> bool {
        self.native().reaches_rows()
    }
    fn reaches_stage9(&self) -> bool {
        self.native().reaches_stage9()
    }
}
#[pyfunction]
fn scan_row(
    py: Python<'_>,
    mut counts: PyRefMut<'_, Counts>,
    row: &ScanRow,
    range: &ScanRange,
    apply: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let mut errors = crate::callbacks::CallbackError::default();
    let result = c::scan_row(&mut counts.inner, row.inner, range.inner, |position| {
        callback(&mut errors, apply.call1((position,)).map(|_| ()))
    });
    errors.finish(py, result.map_err(crate::errors::csv))
}
fn write_result(
    py: Python<'_>,
    write: &Bound<'_, PyAny>,
    bytes: &[u8],
) -> PyResult<c::WriteResult> {
    Ok(write
        .call1((PyBytes::new(py, bytes),))?
        .extract::<PyRef<'_, WriteResult>>()?
        .inner)
}
#[pyfunction]
fn export_ranges(
    py: Python<'_>,
    header: &Bound<'_, PyAny>,
    ranges: &Bound<'_, PyAny>,
    memory_ranges: &Bound<'_, PyAny>,
    write: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let ranges = ranges
        .try_iter()?
        .map(|r| byte_vectors(&r?))
        .collect::<PyResult<Vec<_>>>()?;
    let mut errors = crate::callbacks::CallbackError::default();
    let result = c::export_ranges(
        strict_bytes(header)?,
        &ranges,
        strict_bool(memory_ranges)?,
        |bytes| callback(&mut errors, write_result(py, write, bytes)),
    );
    errors.finish(py, result.map_err(crate::errors::csv))
}
#[pyfunction]
fn export_memory(
    py: Python<'_>,
    header: &Bound<'_, PyAny>,
    ranges: &Bound<'_, PyAny>,
    pair: &Bound<'_, PyAny>,
    build: &Bound<'_, PyAny>,
    write: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let ranges = ranges
        .try_iter()?
        .map(|r| {
            r?.try_iter()?
                .map(|v| v.map(Bound::unbind))
                .collect::<PyResult<Vec<_>>>()
        })
        .collect::<PyResult<Vec<_>>>()?;
    let errors = std::cell::RefCell::new(crate::callbacks::CallbackError::default());
    let result = c::export_memory(
        strict_bytes(header)?,
        &ranges,
        strict_bool(pair)?,
        |source| {
            let result = (|| {
                let value = build.call1((source.bind(py),))?;
                if value.is_none() {
                    return Ok(None);
                }
                let pair = value.cast::<pyo3::types::PyTuple>()?;
                if pair.len() != 2 {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "expected (side, bytes)",
                    ));
                }
                Ok(Some((
                    strict_u8(&pair.get_item(0)?)?,
                    strict_bytes(&pair.get_item(1)?)?.to_vec(),
                )))
            })();
            callback(&mut errors.borrow_mut(), result)
        },
        |bytes| callback(&mut errors.borrow_mut(), write_result(py, write, bytes)),
    );
    errors
        .into_inner()
        .finish(py, result.map_err(crate::errors::csv))
}
#[pyfunction]
fn import_with_gate<'py>(
    state: &Bound<'py, PyAny>,
    gate: &ImportGate,
    rows: &Bound<'py, PyAny>,
    stage9: &Bound<'py, PyAny>,
) -> PyResult<(Option<Bound<'py, PyAny>>, Option<Bound<'py, PyAny>>)> {
    let mut state = state.clone();
    c::import_with_gate(
        &mut state,
        gate.native(),
        |s| rows.call1((&*s,)),
        |s| stage9.call1((&*s,)),
    )
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "control")?;
    add_enum(
        &m,
        "Message",
        &[
            "NoData",
            "NoChannelSpace",
            "ChannelOver",
            "GroupChannelOver",
            "Discard",
            "Correct",
        ],
    )?;
    m.add_class::<Counts>()?;
    m.add_class::<WriteResult>()?;
    m.add_class::<ScanRow>()?;
    m.add_class::<ScanRange>()?;
    m.add_class::<ImportGate>()?;
    m.add_function(wrap_pyfunction!(pair_correction, &m)?)?;
    m.add_function(wrap_pyfunction!(scan_row, &m)?)?;
    m.add_function(wrap_pyfunction!(export_ranges, &m)?)?;
    m.add_function(wrap_pyfunction!(export_memory, &m)?)?;
    m.add_function(wrap_pyfunction!(import_with_gate, &m)?)?;
    root.add_submodule(&m)
}
