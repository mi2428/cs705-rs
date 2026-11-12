//! Live header bindings preserve missing columns and partial rebind state.
use super::*;
use crate::{owner::Owner, scoped::Loan};
use cs705_core::csv::dispatch as d;

pub(crate) fn import_kind(value: &Bound<'_, PyAny>) -> PyResult<d::ImportKind> {
    Ok(
        match enum_index(value, "cs705.csv.dispatch", "ImportKind")? {
            0 => d::ImportKind::Memory,
            1 => d::ImportKind::FixedPair,
            2 => d::ImportKind::AlternatingPair,
            3 => d::ImportKind::Gps,
            4 => d::ImportKind::Repeater,
            5 => d::ImportKind::Callsign,
            6 => d::ImportKind::Scan,
            7 => d::ImportKind::UserEdge,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "invalid ImportKind",
                ));
            }
        },
    )
}
enum BindingState {
    Owned(Owner<d::Binding>),
    Scoped(Loan<d::Binding>),
}
#[pyclass(module = "cs705.csv.dispatch", frozen)]
pub struct Binding {
    state: BindingState,
}
impl Binding {
    pub fn from_core(inner: d::Binding) -> Self {
        Self {
            state: BindingState::Owned(Owner::new(inner)),
        }
    }
    pub fn from_loan(loan: Loan<d::Binding>) -> Self {
        Self {
            state: BindingState::Scoped(loan),
        }
    }
    fn with_ref<T>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&d::Binding) -> PyResult<T>,
    ) -> PyResult<T> {
        match &self.state {
            BindingState::Owned(o) => o.with_ref(f),
            BindingState::Scoped(l) => l.with_ref(py, f),
        }
    }
    fn with_mut<T>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut d::Binding) -> PyResult<T>,
    ) -> PyResult<T> {
        match &self.state {
            BindingState::Owned(o) => o.with_mut(f),
            BindingState::Scoped(l) => l.with_mut(py, f),
        }
    }
}
#[pymethods]
impl Binding {
    fn __eq__(&self, py: Python<'_>, other: &Self) -> PyResult<bool> {
        if std::ptr::eq(self, other) {
            return self.with_ref(py, |_| Ok(true));
        }
        self.with_ref(py, |a| other.with_ref(py, |b| Ok(a == b)))
    }
    #[new]
    fn from_fields(
        positions: &Bound<'_, PyAny>,
        accepted: &Bound<'_, PyAny>,
        separator: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let positions = positions
            .try_iter()?
            .map(|v| {
                let v = v?;
                if v.is_none() {
                    Ok(None)
                } else {
                    Ok(Some(strict_usize(&v)?))
                }
            })
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self::from_core(d::Binding {
            positions,
            accepted: strict_bool(accepted)?,
            separator: strict_u8(separator)?,
        }))
    }
    #[staticmethod]
    fn new(
        py: Python<'_>,
        header: &Bound<'_, PyAny>,
        separator: &Bound<'_, PyAny>,
        labels: &Bound<'_, PyAny>,
        required: &Bound<'_, PyAny>,
        lower: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let labels = byte_vectors(labels)?;
        let mut errors = crate::callbacks::CallbackError::default();
        let result = d::Binding::new(
            strict_bytes(header)?,
            strict_u8(separator)?,
            &labels.iter().map(Vec::as_slice).collect::<Vec<_>>(),
            &indices(required)?,
            |v| {
                callback(
                    &mut errors,
                    lower
                        .call1((PyBytes::new(py, v),))
                        .and_then(|v| Ok(strict_bytes(&v)?.to_vec())),
                )
            },
        );
        errors.finish(py, result.map(Self::from_core).map_err(crate::errors::csv))
    }
    fn rebind(
        &self,
        py: Python<'_>,
        header: &Bound<'_, PyAny>,
        labels: &Bound<'_, PyAny>,
        required: &Bound<'_, PyAny>,
        lower: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        let header = strict_bytes(header)?;
        let labels = byte_vectors(labels)?;
        let required = indices(required)?;
        let mut errors = crate::callbacks::CallbackError::default();
        let result = self.with_mut(py, |binding| {
            binding
                .rebind(
                    header,
                    &labels.iter().map(Vec::as_slice).collect::<Vec<_>>(),
                    &required,
                    |v| {
                        callback(
                            &mut errors,
                            lower
                                .call1((PyBytes::new(py, v),))
                                .and_then(|v| Ok(strict_bytes(&v)?.to_vec())),
                        )
                    },
                )
                .map_err(crate::errors::csv)
        });
        errors.finish(py, result)
    }
    #[staticmethod]
    fn ascii(
        header: &Bound<'_, PyAny>,
        separator: &Bound<'_, PyAny>,
        labels: &Bound<'_, PyAny>,
        required: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let labels = byte_vectors(labels)?;
        d::Binding::ascii(
            strict_bytes(header)?,
            strict_u8(separator)?,
            &labels.iter().map(Vec::as_slice).collect::<Vec<_>>(),
            &indices(required)?,
        )
        .map(Self::from_core)
        .map_err(crate::errors::csv)
    }
    #[staticmethod]
    fn builtin(
        header: &Bound<'_, PyAny>,
        separator: &Bound<'_, PyAny>,
        labels: &Bound<'_, PyAny>,
        kind: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let labels = byte_vectors(labels)?;
        d::Binding::builtin(
            strict_bytes(header)?,
            strict_u8(separator)?,
            &labels.iter().map(Vec::as_slice).collect::<Vec<_>>(),
            import_kind(kind)?,
        )
        .map(Self::from_core)
        .map_err(crate::errors::csv)
    }
    #[getter]
    fn positions<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyTuple>> {
        self.with_ref(py, |b| {
            pyo3::types::PyTuple::new(py, b.positions.iter().copied())
        })
    }
    #[setter]
    fn set_positions(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = value
            .try_iter()?
            .map(|v| {
                let v = v?;
                if v.is_none() {
                    Ok(None)
                } else {
                    Ok(Some(strict_usize(&v)?))
                }
            })
            .collect::<PyResult<Vec<_>>>()?;
        self.with_mut(py, |b| {
            b.positions = value;
            Ok(())
        })
    }
    #[getter]
    fn accepted(&self, py: Python<'_>) -> PyResult<bool> {
        self.with_ref(py, |b| Ok(b.accepted))
    }
    #[setter]
    fn set_accepted(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.with_mut(py, |b| {
            b.accepted = value;
            Ok(())
        })
    }
    #[getter]
    fn separator(&self, py: Python<'_>) -> PyResult<u8> {
        self.with_ref(py, |b| Ok(b.separator))
    }
    #[setter]
    fn set_separator(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u8(value)?;
        self.with_mut(py, |b| {
            b.separator = value;
            Ok(())
        })
    }
    fn cell<'py>(
        &self,
        py: Python<'py>,
        line: &Bound<'_, PyAny>,
        field: &Bound<'_, PyAny>,
    ) -> PyResult<Option<Bound<'py, PyBytes>>> {
        let line = strict_bytes(line)?;
        let field = strict_usize(field)?;
        self.with_ref(py, |b| {
            Ok(b.cell(line, field)
                .map_err(crate::errors::csv)?
                .map(|v| PyBytes::new(py, v)))
        })
    }
}
#[pyclass(module = "cs705.csv.dispatch")]
pub struct Dispatch {
    inner: d::Dispatch,
}
#[pymethods]
impl Dispatch {
    #[new]
    fn new(
        slot: &Bound<'_, PyAny>,
        export: &Bound<'_, PyAny>,
        import_kind: Option<&Bound<'_, PyAny>>,
        skip_invalid_export: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: d::Dispatch {
                slot: strict_u8(slot)?,
                export: super::payload::kind(export)?,
                import: import_kind.map(self::import_kind).transpose()?,
                skip_invalid_export: strict_bool(skip_invalid_export)?,
            },
        })
    }
    #[setter]
    fn set_slot(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.slot = strict_u8(value)?;
        Ok(())
    }
    #[setter]
    fn set_export(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.export = super::payload::kind(value)?;
        Ok(())
    }
    #[setter("import")]
    fn set_import_kind(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.inner.import = value.map(import_kind).transpose()?;
        Ok(())
    }
    #[setter]
    fn set_skip_invalid_export(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.skip_invalid_export = strict_bool(value)?;
        Ok(())
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
    #[getter]
    fn slot(&self) -> u8 {
        self.inner.slot
    }
    #[getter]
    fn export<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        enum_result(py, "cs705.csv.payload", "Kind", self.inner.export as usize)
    }
    #[getter("import")]
    fn import_kind<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.inner
            .import
            .map(|v| enum_result(py, "cs705.csv.dispatch", "ImportKind", v as usize))
            .transpose()
    }
    #[getter]
    fn skip_invalid_export(&self) -> bool {
        self.inner.skip_invalid_export
    }
}
#[pyfunction]
fn for_context(context_type: &Bound<'_, PyAny>) -> PyResult<Dispatch> {
    Ok(Dispatch {
        inner: d::for_context(strict_u32(context_type)?),
    })
}
#[pyfunction]
fn required_labels<'py>(py: Python<'py>, kind: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyList>> {
    bytes_list(py, import_kind(kind)?.required_labels())
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "dispatch")?;
    add_enum(
        &m,
        "ImportKind",
        &[
            "Memory",
            "FixedPair",
            "AlternatingPair",
            "Gps",
            "Repeater",
            "Callsign",
            "Scan",
            "UserEdge",
        ],
    )?;
    m.add_class::<Binding>()?;
    m.add_class::<Dispatch>()?;
    m.add_function(wrap_pyfunction!(for_context, &m)?)?;
    m.add_function(wrap_pyfunction!(required_labels, &m)?)?;
    m.getattr("ImportKind")?.setattr(
        "required_labels",
        root.py()
            .import("functools")?
            .getattr("partialmethod")?
            .call1((m.getattr("required_labels")?,))?,
    )?;
    root.add_submodule(&m)
}
