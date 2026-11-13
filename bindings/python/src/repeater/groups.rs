//! Working group metadata with explicit whole-field assignment.
use crate::{conversion::*, errors};
use cs705_core::repeater::groups as core;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

pub(crate) fn words<const N: usize>(v: &Bound<'_, PyAny>) -> PyResult<[u16; N]> {
    let values: Vec<_> = v
        .try_iter()?
        .map(|v| strict_u16(&v?))
        .collect::<PyResult<_>>()?;
    values
        .try_into()
        .map_err(|_| pyo3::exceptions::PyValueError::new_err(format!("expected {N} integers")))
}
/// Mutable array values are explicit snapshots with replacement setters.
#[pyclass(module = "cs705.repeater.groups")]
#[derive(Clone)]
pub struct Groups {
    pub(crate) inner: core::Groups,
}
#[pymethods]
impl Groups {
    #[new]
    fn new(
        counts: &Bound<'_, PyAny>,
        names: &Bound<'_, PyAny>,
        references: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::Groups {
                counts: words(counts)?,
                names: names_array(names)?,
                references: words(references)?,
            },
        })
    }
    #[staticmethod]
    fn from_working(memory: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Groups::from_working(strict_bytes(memory)?).map_err(errors::icf)?,
        })
    }
    #[staticmethod]
    fn from_icf(image: &crate::icf::Icf) -> PyResult<Self> {
        image.with_ref(|image| {
            Ok(Self {
                inner: core::Groups::from_icf(image).map_err(errors::icf)?,
            })
        })
    }
    fn counts_snapshot(&self) -> Vec<u16> {
        self.inner.counts.to_vec()
    }
    fn set_counts(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.counts = words(v)?;
        Ok(())
    }
    #[getter]
    fn names<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        PyTuple::new(py, self.inner.names.iter().map(|v| PyBytes::new(py, v)))
    }
    #[setter]
    fn set_names(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.names = names_array(v)?;
        Ok(())
    }
    fn references_snapshot(&self) -> Vec<u16> {
        self.inner.references.to_vec()
    }
    fn set_references(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.references = words(v)?;
        Ok(())
    }
    fn total(&self) -> u32 {
        self.inner.total()
    }
    fn start(&self, group: &Bound<'_, PyAny>) -> PyResult<u32> {
        self.inner.start(strict_usize(group)?).map_err(errors::icf)
    }
    fn end(&self, group: &Bound<'_, PyAny>) -> PyResult<i32> {
        self.inner.end(strict_usize(group)?).map_err(errors::icf)
    }
    fn group_of(&self, absolute: &Bound<'_, PyAny>) -> PyResult<usize> {
        Ok(self.inner.group_of(strict_i32(absolute)?))
    }
    fn after_delete(
        &mut self,
        group: &Bound<'_, PyAny>,
        relative: &Bound<'_, PyAny>,
        reset: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.inner
            .after_delete(strict_usize(group)?, strict_usize(relative)?, words(reset)?)
            .map_err(errors::icf)
    }
    fn after_insert(
        &mut self,
        group: &Bound<'_, PyAny>,
        relative: &Bound<'_, PyAny>,
        reset: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.inner
            .after_insert(strict_usize(group)?, strict_usize(relative)?, words(reset)?)
            .map_err(errors::icf)
    }
    fn after_paste(
        &mut self,
        group: &Bound<'_, PyAny>,
        first: &Bound<'_, PyAny>,
        last: &Bound<'_, PyAny>,
        reset: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.inner
            .after_paste(
                strict_usize(group)?,
                strict_usize(first)?,
                strict_usize(last)?,
                words(reset)?,
            )
            .map_err(errors::icf)
    }
    fn after_clear(
        &mut self,
        group: &Bound<'_, PyAny>,
        first: &Bound<'_, PyAny>,
        last: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.inner
            .after_clear(
                strict_usize(group)?,
                strict_usize(first)?,
                strict_usize(last)?,
            )
            .map_err(errors::icf)
    }
    fn after_carry(
        &mut self,
        group: &Bound<'_, PyAny>,
        first: &Bound<'_, PyAny>,
        last: &Bound<'_, PyAny>,
        direction: &Bound<'_, PyAny>,
        reset: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.inner
            .after_carry(
                strict_usize(group)?,
                strict_usize(first)?,
                strict_usize(last)?,
                strict_i32(direction)?,
                words(reset)?,
            )
            .map_err(errors::icf)
    }
    fn storage_tables<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<(Bound<'py, PyBytes>, Bound<'py, PyBytes>)> {
        let (order, starts) = self.inner.storage_tables().map_err(errors::icf)?;
        Ok((PyBytes::new(py, &order), PyBytes::new(py, &starts)))
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
    fn write_storage_tables_atomic(&self, image: &crate::icf::Icf) -> PyResult<()> {
        image.with_mut(|i| {
            self.inner
                .write_storage_tables_atomic(i)
                .map_err(errors::icf)
        })
    }
    fn write_references(&self, image: &crate::icf::Icf) -> PyResult<()> {
        image.with_mut(|i| self.inner.write_references(i).map_err(errors::icf))
    }
}
#[pyfunction]
fn rename_group_raw(
    image: &crate::icf::Icf,
    group: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let (group, name) = (strict_usize(group)?, fixed_bytes(name)?);
    image.with_mut(|i| core::rename_group_raw(i, group, name).map_err(errors::icf))
}
pub(crate) fn names_array(v: &Bound<'_, PyAny>) -> PyResult<[[u8; 16]; 50]> {
    let values: Vec<_> = v
        .try_iter()?
        .map(|v| fixed_bytes::<16>(&v?))
        .collect::<PyResult<_>>()?;
    values
        .try_into()
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("expected exactly 50 names"))
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "groups")?;
    m.add_class::<Groups>()?;
    m.add_function(wrap_pyfunction!(rename_group_raw, &m)?)?;
    root.add_submodule(&m)
}
