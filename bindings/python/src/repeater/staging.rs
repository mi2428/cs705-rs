//! Decoded staging records and native identity operations.
use crate::{
    callbacks::CallbackError,
    conversion::*,
    csv::control::Counts,
    errors,
    owner::Owner,
    scoped::{Frame, Loan},
};
use cs705_core::repeater::staging as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

#[pyclass(module = "cs705.repeater.staging", frozen)]
pub struct Record {
    pub(crate) inner: RecordState,
}
pub(crate) enum RecordState {
    Owned(Owner<core::Record>),
    Scoped(Loan<core::Record>),
}
impl RecordState {
    pub(crate) fn with_ref<R>(&self, f: impl FnOnce(&core::Record) -> PyResult<R>) -> PyResult<R> {
        match self {
            Self::Owned(o) => o.with_ref(f),
            Self::Scoped(l) => Python::attach(|py| l.with_ref(py, f)),
        }
    }
    pub(crate) fn with_mut<R>(
        &self,
        f: impl FnOnce(&mut core::Record) -> PyResult<R>,
    ) -> PyResult<R> {
        match self {
            Self::Owned(o) => o.with_mut(f),
            Self::Scoped(l) => Python::attach(|py| l.with_mut(py, f)),
        }
    }
}
impl Record {
    pub(crate) fn from_core(value: core::Record) -> Self {
        Self {
            inner: RecordState::Owned(Owner::new(value)),
        }
    }
    pub(crate) fn from_loan(value: Loan<core::Record>) -> Self {
        Self {
            inner: RecordState::Scoped(value),
        }
    }
}
#[pymethods]
impl Record {
    #[new]
    fn new(bytes: &Bound<'_, PyAny>, frequency_class: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(core::Record {
            bytes: Box::new(fixed_bytes(bytes)?),
            frequency_class: strict_u32(frequency_class)?,
        }))
    }
    #[getter]
    fn bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.inner
            .with_ref(|r| Ok(PyBytes::new(py, r.bytes.as_slice())))
    }
    #[setter]
    fn set_bytes(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = fixed_bytes(value)?;
        self.inner.with_mut(|r| {
            *r.bytes = value;
            Ok(())
        })
    }
    #[getter]
    fn frequency_class(&self) -> PyResult<u32> {
        self.inner.with_ref(|r| Ok(r.frequency_class))
    }
    #[setter]
    fn set_frequency_class(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u32(value)?;
        self.inner.with_mut(|r| {
            r.frequency_class = value;
            Ok(())
        })
    }
    fn group(&self) -> PyResult<u32> {
        self.inner.with_ref(|r| Ok(r.group()))
    }
    fn __eq__(&self, other: &Self) -> PyResult<bool> {
        if std::ptr::eq(self, other) {
            return Ok(true);
        }
        self.inner
            .with_ref(|a| other.inner.with_ref(|b| Ok(a == b)))
    }
}
#[pyfunction]
fn same(a: &Record, b: &Record) -> PyResult<bool> {
    if std::ptr::eq(a, b) {
        return a.inner.with_ref(|r| Ok(core::same(r, r)));
    }
    a.inner
        .with_ref(|a| b.inner.with_ref(|b| Ok(core::same(a, b))))
}
#[pyfunction]
fn classify(row: &Record) -> PyResult<u32> {
    row.inner.with_ref(|r| Ok(core::classify(r)))
}
#[pyfunction]
fn classify_bytes(bytes: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(core::classify_bytes(&fixed_bytes(bytes)?))
}

pub(crate) fn records(value: &Bound<'_, PyAny>) -> PyResult<Vec<core::Record>> {
    value
        .try_iter()?
        .map(|v| {
            v?.extract::<PyRef<'_, Record>>()?
                .inner
                .with_ref(|r| Ok(r.clone()))
        })
        .collect()
}
/// Counts is read as a snapshot and replaced with set_counts.
#[pyclass(module = "cs705.repeater.staging", frozen)]
pub struct Staging {
    inner: Owner<core::Staging>,
}
#[pymethods]
impl Staging {
    #[staticmethod]
    fn preload(
        py: Python<'_>,
        rows: &Bound<'_, PyAny>,
        names: &Bound<'_, PyAny>,
        old_reference: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let (names, old_reference) = (
            super::groups::names_array(names)?,
            strict_u16(old_reference)?,
        );
        let mut iterator = rows.try_iter()?;
        let mut error = CallbackError::default();
        let rows = std::iter::from_fn(|| {
            iterator.next().map(|item| {
                let result = item.and_then(|v| {
                    v.extract::<PyRef<'_, Record>>()?
                        .inner
                        .with_ref(|r| Ok(r.clone()))
                });
                error.capture(result).map_err(|_| {
                    cs705_core::csv::Error::Operation("repeater preload iterator failed".into())
                })
            })
        });
        let native = core::Staging::preload_fallible(rows, names, old_reference)
            .map(|s| Self {
                inner: Owner::new(s),
            })
            .map_err(errors::csv);
        error.finish(py, native)
    }
    #[staticmethod]
    fn preload_fallible(
        py: Python<'_>,
        rows: &Bound<'_, PyAny>,
        names: &Bound<'_, PyAny>,
        old_reference: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Self::preload(py, rows, names, old_reference)
    }
    fn rows(&self) -> PyResult<Vec<Record>> {
        self.inner
            .with_ref(|s| Ok(s.rows().cloned().map(Record::from_core).collect()))
    }
    fn pending_count(&self) -> PyResult<usize> {
        self.inner.with_ref(|s| Ok(s.pending_count()))
    }
    fn first_reference_ordinal(&self) -> PyResult<u16> {
        self.inner.with_ref(|s| Ok(s.first_reference_ordinal()))
    }
    #[getter]
    fn names<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyTuple>> {
        self.inner.with_ref(|s| {
            pyo3::types::PyTuple::new(py, s.names.iter().map(|n| PyBytes::new(py, n)))
        })
    }
    #[setter]
    fn set_names(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let names = super::groups::names_array(value)?;
        self.inner.with_mut(|s| {
            s.names = names;
            Ok(())
        })
    }
    fn counts_snapshot(&self) -> PyResult<Counts> {
        self.inner.with_ref(|s| Ok(Counts::from_core(s.counts)))
    }
    fn set_counts(&self, value: &Counts) -> PyResult<()> {
        self.inner.with_mut(|s| {
            s.counts = value.inner;
            Ok(())
        })
    }
    #[getter]
    fn old_reference_existed(&self) -> PyResult<bool> {
        self.inner.with_ref(|s| Ok(s.old_reference_existed))
    }
    #[setter]
    fn set_old_reference_existed(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_bool(value)?;
        self.inner.with_mut(|s| {
            s.old_reference_existed = v;
            Ok(())
        })
    }
    fn selected_row(
        &self,
        input: &Record,
        name: &Bound<'_, PyAny>,
        group: &Bound<'_, PyAny>,
        clone_header: &Bound<'_, PyAny>,
        delta: &Bound<'_, PyAny>,
    ) -> PyResult<u32> {
        let (name, group, header, delta) = (
            fixed_bytes(name)?,
            strict_usize(group)?,
            fixed_bytes(clone_header)?,
            strict_u32(delta)?,
        );
        self.inner.with_mut(|s| {
            input.inner.with_mut(|r| {
                s.selected_row(r, name, group, header, delta)
                    .map_err(errors::csv)
            })
        })
    }
    fn whole_row(
        &self,
        input: &Record,
        name: &Bound<'_, PyAny>,
        clone_header: &Bound<'_, PyAny>,
        delta: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let (name, header, delta) = (
            fixed_bytes(name)?,
            fixed_bytes(clone_header)?,
            strict_u32(delta)?,
        );
        self.inner.with_mut(|s| {
            input
                .inner
                .with_mut(|r| s.whole_row(r, name, header, delta).map_err(errors::csv))
        })
    }
    fn finish_whole(&self) -> PyResult<()> {
        self.inner.with_mut(|s| {
            s.finish_whole();
            Ok(())
        })
    }
    fn flush_selected(
        &self,
        py: Python<'_>,
        group: &Bound<'_, PyAny>,
        old_start: &Bound<'_, PyAny>,
        old_count: &Bound<'_, PyAny>,
        apply: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let (group, start, count) = (
            strict_usize(group)?,
            strict_usize(old_start)?,
            strict_usize(old_count)?,
        );
        let mut error = CallbackError::default();
        let native = self.inner.with_ref(|s| {
            s.flush_selected(group, start, count, |c| {
                let result = call_commit(py, c, apply);
                error.capture(result).map_err(|_| {
                    cs705_core::csv::Error::Operation(
                        "repeater selected commit callback failed".into(),
                    )
                })
            })
            .map_err(errors::csv)
        });
        error.finish(py, native)
    }
    fn flush_whole(
        &self,
        py: Python<'_>,
        old_total: &Bound<'_, PyAny>,
        apply: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let total = strict_usize(old_total)?;
        let mut error = CallbackError::default();
        let native = self.inner.with_ref(|s| {
            s.flush_whole(total, |c| {
                let result = call_commit(py, c, apply);
                error.capture(result).map_err(|_| {
                    cs705_core::csv::Error::Operation(
                        "repeater whole commit callback failed".into(),
                    )
                })
            })
            .map_err(errors::csv)
        });
        error.finish(py, native)
    }
}
#[pyclass(module = "cs705.repeater.staging")]
pub struct Commit {
    #[pyo3(get)]
    variant: &'static str,
    #[pyo3(get)]
    absolute: Option<usize>,
    #[pyo3(get)]
    group: Option<usize>,
    #[pyo3(get)]
    byte: Option<usize>,
    #[pyo3(get)]
    value: Option<u16>,
    #[pyo3(get)]
    record: Option<Py<Record>>,
}
impl Commit {
    fn blank(variant: &'static str) -> Self {
        Self {
            variant,
            absolute: None,
            group: None,
            byte: None,
            value: None,
            record: None,
        }
    }
    fn field(&self, variants: &[&str], name: &str) -> PyResult<()> {
        if variants.contains(&self.variant) {
            Ok(())
        } else {
            Err(pyo3::exceptions::PyAttributeError::new_err(format!(
                "{} has no {name} field",
                self.variant
            )))
        }
    }
}
#[pymethods]
#[allow(non_snake_case)]
impl Commit {
    #[staticmethod]
    fn Delete(absolute: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut v = Self::blank("Delete");
        v.absolute = Some(strict_usize(absolute)?);
        Ok(v)
    }
    #[staticmethod]
    #[pyo3(signature=(absolute,group,record))]
    fn Insert(
        absolute: Option<&Bound<'_, PyAny>>,
        group: &Bound<'_, PyAny>,
        record: Py<Record>,
    ) -> PyResult<Self> {
        let mut v = Self::blank("Insert");
        v.absolute = absolute.map(strict_usize).transpose()?;
        v.group = Some(strict_usize(group)?);
        v.record = Some(record);
        Ok(v)
    }
    #[staticmethod]
    fn NameByte(
        group: &Bound<'_, PyAny>,
        byte: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let mut v = Self::blank("NameByte");
        v.group = Some(strict_usize(group)?);
        v.byte = Some(strict_usize(byte)?);
        v.value = Some(u16::from(strict_u8(value)?));
        Ok(v)
    }
    #[staticmethod]
    fn Reference(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut v = Self::blank("Reference");
        v.value = Some(strict_u16(value)?);
        Ok(v)
    }
    #[staticmethod]
    fn GroupNotification(group: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut v = Self::blank("GroupNotification");
        v.group = Some(strict_usize(group)?);
        Ok(v)
    }
    #[setter]
    fn set_absolute(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.field(&["Delete", "Insert"], "absolute")?;
        let value = value.map(strict_usize).transpose()?;
        if self.variant == "Delete" && value.is_none() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "Delete.absolute requires an integer",
            ));
        }
        self.absolute = value;
        Ok(())
    }
    #[setter]
    fn set_group(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.field(&["Insert", "NameByte", "GroupNotification"], "group")?;
        self.group = Some(strict_usize(value)?);
        Ok(())
    }
    #[setter]
    fn set_byte(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.field(&["NameByte"], "byte")?;
        self.byte = Some(strict_usize(value)?);
        Ok(())
    }
    #[setter]
    fn set_value(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.field(&["NameByte", "Reference"], "value")?;
        self.value = Some(if self.variant == "NameByte" {
            u16::from(strict_u8(value)?)
        } else {
            strict_u16(value)?
        });
        Ok(())
    }
    #[setter]
    fn set_record(&mut self, value: Py<Record>) -> PyResult<()> {
        self.field(&["Insert"], "record")?;
        self.record = Some(value);
        Ok(())
    }
}
fn call_commit(py: Python<'_>, c: core::Commit<'_>, apply: &Bound<'_, PyAny>) -> PyResult<()> {
    let mut out = Commit::blank("");
    match c {
        core::Commit::Delete { absolute } => {
            out.variant = "Delete";
            out.absolute = Some(absolute);
        }
        core::Commit::Insert {
            absolute,
            group,
            record,
        } => {
            return Frame::with_ref(py, record, |loan| {
                out.variant = "Insert";
                out.absolute = absolute;
                out.group = Some(group);
                out.record = Some(Py::new(py, Record::from_loan(loan))?);
                apply.call1((out,))?;
                Ok(())
            });
        }
        core::Commit::NameByte { group, byte, value } => {
            out.variant = "NameByte";
            out.group = Some(group);
            out.byte = Some(byte);
            out.value = Some(u16::from(value));
        }
        core::Commit::Reference(value) => {
            out.variant = "Reference";
            out.value = Some(value);
        }
        core::Commit::GroupNotification { group } => {
            out.variant = "GroupNotification";
            out.group = Some(group);
        }
    }
    apply.call1((out,))?;
    Ok(())
}

#[pyclass(module = "cs705.repeater.staging")]
pub struct PreserveSource {
    pub(crate) inner: core::PreserveSource,
}
macro_rules! preserve {
    ($($name:ident,$setter:ident => $convert:ident: $ty:ty),*) => {
        #[pymethods] impl PreserveSource {
            fn __richcmp__(&self,other:&Self,op:pyo3::basic::CompareOp)->PyResult<bool>{super::equality(self.inner==other.inner,op)}
            #[new] fn new(raw: &Bound<'_,PyAny>, first: &Bound<'_,PyAny>, last: &Bound<'_,PyAny>, unwritten: &Bound<'_,PyAny>) -> PyResult<Self> { Ok(Self { inner: core::PreserveSource { raw: strict_u32(raw)?, first: strict_u32(first)?, last: strict_u32(last)?, unwritten: strict_bool(unwritten)? } }) }
            $(#[getter] fn $name(&self) -> $ty { self.inner.$name }
            #[setter] fn $setter(&mut self, v: &Bound<'_,PyAny>) -> PyResult<()> { self.inner.$name = $convert(v)?; Ok(()) })*
        }
    }
}
preserve!(raw,set_raw => strict_u32: u32, first,set_first => strict_u32: u32, last,set_last => strict_u32: u32, unwritten,set_unwritten => strict_bool: bool);
#[pyfunction]
fn restored_reference(
    rows: &Bound<'_, PyAny>,
    new_absolute_start: &Bound<'_, PyAny>,
    old_reference: &Record,
) -> PyResult<u16> {
    let (rows, start) = (records(rows)?, strict_u32(new_absolute_start)?);
    old_reference
        .inner
        .with_ref(|old| Ok(core::restored_reference(&rows, start, old)))
}
/// Contiguous owned backing for native mutable record slices.
#[pyclass(module = "cs705.repeater.staging", frozen)]
pub struct Records {
    inner: Owner<Vec<core::Record>>,
}
#[pymethods]
impl Records {
    #[new]
    fn new(rows: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: Owner::new(records(rows)?),
        })
    }
    fn __len__(&self) -> PyResult<usize> {
        self.inner.with_ref(|r| Ok(r.len()))
    }
    fn __getitem__(&self, index: &Bound<'_, PyAny>) -> PyResult<Record> {
        let n = strict_usize(index)?;
        self.inner.with_ref(|r| {
            r.get(n).cloned().map(Record::from_core).ok_or_else(|| {
                pyo3::exceptions::PyIndexError::new_err("record index outside owned slice")
            })
        })
    }
    fn __setitem__(&self, index: &Bound<'_, PyAny>, value: &Record) -> PyResult<()> {
        let n = strict_usize(index)?;
        let value = value.inner.with_ref(|r| Ok(r.clone()))?;
        self.inner.with_mut(|r| {
            *r.get_mut(n).ok_or_else(|| {
                pyo3::exceptions::PyIndexError::new_err("record index outside owned slice")
            })? = value;
            Ok(())
        })
    }
    fn to_list(&self) -> PyResult<Vec<Record>> {
        self.inner
            .with_ref(|r| Ok(r.iter().cloned().map(Record::from_core).collect()))
    }
}
#[pyfunction]
fn restore_use(
    py: Python<'_>,
    rows: &Records,
    source: Py<PreserveSource>,
    read_budget: &Bound<'_, PyAny>,
    read: Py<PyAny>,
    save: Py<PyAny>,
) -> PyResult<()> {
    let (source, budget) = (source.try_borrow(py)?.inner, strict_usize(read_budget)?);
    let errors = std::cell::RefCell::new(CallbackError::default());
    let native = rows.inner.with_mut(|rows| {
        core::restore_use(
            rows,
            source,
            budget,
            |raw, at| {
                let result = read.bind(py).call1((raw, at)).and_then(|v| {
                    v.extract::<PyRef<'_, Record>>()?
                        .inner
                        .with_ref(|r| Ok(r.clone()))
                });
                errors.borrow_mut().capture(result).map_err(|_| {
                    cs705_core::csv::Error::Operation(
                        "repeater preserve reader callback failed".into(),
                    )
                })
            },
            |index, row| {
                let result = Frame::with_ref(py, row, |loan| {
                    save.bind(py).call1((index, Record::from_loan(loan)))?;
                    Ok(())
                });
                errors.borrow_mut().capture(result).map_err(|_| {
                    cs705_core::csv::Error::Operation(
                        "repeater preserve saver callback failed".into(),
                    )
                })
            },
        )
        .map_err(errors::csv)
    });
    errors.into_inner().finish(py, native)
}

pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "staging")?;
    m.add_class::<Record>()?;
    m.add_class::<Staging>()?;
    m.add_class::<Commit>()?;
    m.add_class::<PreserveSource>()?;
    m.add_class::<Records>()?;
    m.add_function(wrap_pyfunction!(same, &m)?)?;
    m.add_function(wrap_pyfunction!(classify, &m)?)?;
    m.add_function(wrap_pyfunction!(classify_bytes, &m)?)?;
    m.add_function(wrap_pyfunction!(restored_reference, &m)?)?;
    m.add_function(wrap_pyfunction!(restore_use, &m)?)?;
    root.add_submodule(&m)
}
