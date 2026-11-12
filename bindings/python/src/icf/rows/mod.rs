//! Owned native working maps and explicit UR storage reconstruction.
pub mod native;
pub mod ur;
use super::{Icf, normalizer::ReferenceImage, record::DecodedRecord, values::Output};
use crate::{conversion::*, errors, owner::Owner};
use cs705_core::icf::rows as core;
use pyo3::prelude::*;
use std::ops::Range;

#[pyclass(module = "cs705.icf.rows")]
pub struct Allocation {
    pub inner: core::Allocation,
}
#[pymethods]
impl Allocation {
    #[new]
    fn new(
        requested: &Bound<'_, PyAny>,
        allocated: &Bound<'_, PyAny>,
        exhausted: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::Allocation {
                requested: strict_usize(requested)?,
                allocated: strict_usize(allocated)?,
                exhausted: strict_bool(exhausted)?,
            },
        })
    }
    #[getter]
    fn requested(&self) -> usize {
        self.inner.requested
    }
    #[setter]
    fn set_requested(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.requested = strict_usize(value)?;
        Ok(())
    }
    #[getter]
    fn allocated(&self) -> usize {
        self.inner.allocated
    }
    #[setter]
    fn set_allocated(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.allocated = strict_usize(value)?;
        Ok(())
    }
    #[getter]
    fn exhausted(&self) -> bool {
        self.inner.exhausted
    }
    #[setter]
    fn set_exhausted(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.exhausted = strict_bool(value)?;
        Ok(())
    }
}
impl Output for core::Allocation {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(Py::new(py, Allocation { inner: self })?
            .into_bound(py)
            .into_any())
    }
}
impl super::values::Capture for core::Allocation {
    type Owned = Self;
    fn capture(self) -> Self {
        self
    }
}

#[pyclass(module = "cs705.icf.rows", frozen)]
pub struct NormalMemory {
    owner: Owner<core::NormalMemory>,
}
impl NormalMemory {
    pub fn take(&self) -> PyResult<crate::owner::Taken<'_, core::NormalMemory>> {
        self.owner.take()
    }
    pub fn from_core(value: core::NormalMemory) -> Self {
        Self {
            owner: Owner::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::NormalMemory) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_ref(f)
    }
    pub fn with_mut<R>(
        &self,
        f: impl FnOnce(&mut core::NormalMemory) -> PyResult<R>,
    ) -> PyResult<R> {
        self.owner.with_mut(f)
    }
}

fn with_home_byte<R>(state: &MutableBytes, f: impl FnOnce(&mut u8) -> PyResult<R>) -> PyResult<R> {
    let mut state = state.take()?;
    let [value] = state.as_mut_slice() else {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "home_invalidated requires one byte",
        ));
    };
    f(value)
}
super::direct::direct_methods! {
NormalMemory
existing {
    #[staticmethod]
    fn from_icf(image: &Icf) -> PyResult<Self> { image.with_ref(|image| core::NormalMemory::from_icf(image).map(Self::from_core).map_err(errors::icf)) }
    #[staticmethod]
    fn from_working(working: &Bound<'_, PyAny>) -> PyResult<Self> { core::NormalMemory::from_working(strict_bytes(working)?.to_vec()).map(Self::from_core).map_err(errors::icf) }
    fn to_icf(&self) -> PyResult<Icf> { self.with_ref(|model| model.to_icf().map(|image| Icf::from_core(image.into_owned())).map_err(errors::icf)) }
    fn store_channel(&self, index: &Bound<'_, PyAny>, holder: &DecodedRecord) -> PyResult<()> { let index = strict_u32(index)?; self.with_mut(|model| holder.with_mut(|h| model.store_channel(index, h).map_err(errors::icf))) }
    #[pyo3(signature = (group, input))]
    fn edit_group_name(&self, group: &Bound<'_, PyAny>, input: Option<&MutableBytes>) -> PyResult<bool> { let group = strict_usize(group)?;let mut input=input.map(MutableBytes::take).transpose()?; self.with_mut(|model| model.edit_group_name(group, input.as_mut().map(|v| v.as_mut_slice())).map_err(errors::icf)) }
    fn reset_home(&self, py: Python<'_>, group: &Bound<'_, PyAny>, defaults: &ReferenceImage, home_invalidated: &MutableBytes) -> PyResult<()> {
        let group = strict_usize(group)?;
        with_home_byte(home_invalidated, |value| defaults.with_core(py, |defaults| self.with_mut(|model| model.reset_home(group, defaults, value).map_err(errors::icf))))
    }
    fn release_with_home(&self, py: Python<'_>, group: &Bound<'_, PyAny>, rows: &Bound<'_, PyAny>, defaults: &ReferenceImage, home_invalidated: &MutableBytes) -> PyResult<usize> {
        let group = strict_usize(group)?; let rows = strict_range(rows)?;
        with_home_byte(home_invalidated, |value| defaults.with_core(py, |defaults| self.with_mut(|model| model.release_with_home(group, rows, defaults, value).map_err(errors::icf))))
    }
    fn delete_with_home(&self, py: Python<'_>, group: &Bound<'_, PyAny>, rows: &Bound<'_, PyAny>, defaults: &ReferenceImage, home_invalidated: &MutableBytes) -> PyResult<usize> {
        let group = strict_usize(group)?; let rows = strict_range(rows)?;
        with_home_byte(home_invalidated, |value| defaults.with_core(py, |defaults| self.with_mut(|model| model.delete_with_home(group, rows, defaults, value).map_err(errors::icf))))
    }
    fn insert_with_home(&self, py: Python<'_>, group: &Bound<'_, PyAny>, at: &Bound<'_, PyAny>, count: &Bound<'_, PyAny>, defaults: &ReferenceImage, home_invalidated: &MutableBytes) -> PyResult<usize> {
        let group = strict_usize(group)?; let at = strict_usize(at)?; let count = strict_usize(count)?;
        with_home_byte(home_invalidated, |value| defaults.with_core(py, |defaults| self.with_mut(|model| model.insert_with_home(group, at, count, defaults, value).map_err(errors::icf))))
    }
    fn carry_with_home(&self, py: Python<'_>, group: &Bound<'_, PyAny>, rows: &Bound<'_, PyAny>, direction: &Bound<'_, PyAny>, defaults: &ReferenceImage, home_invalidated: &MutableBytes) -> PyResult<bool> {
        let group = strict_usize(group)?; let rows = strict_range(rows)?; let direction = strict_i32(direction)?;
        with_home_byte(home_invalidated, |value| defaults.with_core(py, |defaults| self.with_mut(|model| model.carry_with_home(group, rows, direction, defaults, value).map_err(errors::icf))))
    }
}
methods {
    working() [with_ref, plain];
    group_count() [with_ref, plain];
    group_used_slots(group: usize) [with_ref, fallible];
    used_slots() [with_ref, plain];
    slot(group: usize, row: usize) [with_ref, fallible];
    record(group: usize, row: usize) [with_ref, fallible];
    allocate(group: usize, rows: Range<usize>) [with_mut, fallible];
    release(group: usize, rows: Range<usize>) [with_mut, fallible];
    delete(group: usize, rows: Range<usize>) [with_mut, fallible];
    insert(group: usize, at: usize, count: usize) [with_mut, fallible];
    carry(group: usize, rows: Range<usize>, direction: i32) [with_mut, fallible];
    add_group() [with_mut, plain];
    rename_group(group: usize, candidate: Option<&[u8]>) [with_mut, fallible];
    set_channel_name_and_frequency(group: usize, row: usize, name: &[u8;16], frequency_hz: Option<u32>) [with_mut, fallible];
    copy_channel(source: (usize,usize), target: (usize,usize)) [with_mut, fallible];
}
}

#[pyclass(module = "cs705.icf.rows", frozen)]
pub struct WorkingUr {
    owner: Owner<core::WorkingUr>,
}
impl WorkingUr {
    pub fn take(&self) -> PyResult<crate::owner::Taken<'_, core::WorkingUr>> {
        self.owner.take()
    }
    pub fn from_core(value: core::WorkingUr) -> Self {
        Self {
            owner: Owner::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::WorkingUr) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_ref(f)
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut core::WorkingUr) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_mut(f)
    }
}
#[pymethods]
impl WorkingUr {
    #[staticmethod]
    fn from_icf(image: &Icf) -> PyResult<Self> {
        image.with_ref(|image| {
            core::WorkingUr::from_icf(image)
                .map(Self::from_core)
                .map_err(errors::icf)
        })
    }
    fn working<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
        self.with_ref(|model| Ok(pyo3::types::PyBytes::new(py, model.working())))
    }
    fn count(&self) -> PyResult<u16> {
        self.with_ref(|model| Ok(model.count()))
    }
    fn to_icf(&self) -> PyResult<Icf> {
        self.with_ref(|model| {
            model
                .to_icf()
                .map(|image| Icf::from_core(image.into_owned()))
                .map_err(errors::icf)
        })
    }
    fn load_into(&self, index: &Bound<'_, PyAny>, holder: &DecodedRecord) -> PyResult<bool> {
        let index = strict_u32(index)?;
        self.with_ref(|model| holder.with_mut(|h| model.load_into(index, h).map_err(errors::icf)))
    }
    fn save(&self, index: &Bound<'_, PyAny>, holder: &DecodedRecord) -> PyResult<()> {
        let index = strict_u32(index)?;
        self.with_mut(|model| holder.with_ref(|h| model.save(index, h).map_err(errors::icf)))
    }
    fn insert_one(
        &self,
        py: Python<'_>,
        index: &Bound<'_, PyAny>,
        defaults: &ReferenceImage,
    ) -> PyResult<bool> {
        let index = strict_u32(index)?;
        defaults.with_core(py, |defaults| {
            self.with_mut(|model| model.insert_one(index, defaults).map_err(errors::icf))
        })
    }
    fn append(&self, py: Python<'_>, defaults: &ReferenceImage) -> PyResult<bool> {
        defaults.with_core(py, |defaults| {
            self.with_mut(|model| model.append(defaults).map_err(errors::icf))
        })
    }
    fn delete_one(
        &self,
        py: Python<'_>,
        index: &Bound<'_, PyAny>,
        defaults: &ReferenceImage,
    ) -> PyResult<()> {
        let index = strict_u32(index)?;
        defaults.with_core(py, |defaults| {
            self.with_mut(|model| model.delete_one(index, defaults).map_err(errors::icf))
        })
    }
}
#[pyfunction]
fn ur_default(index: &Bound<'_, PyAny>, binding: &Bound<'_, PyAny>) -> PyResult<DecodedRecord> {
    Ok(DecodedRecord::from_core(core::ur_default(
        strict_u32(index)?,
        strict_u32(binding)?,
    )))
}
#[pyfunction]
fn allocate_into(
    image: &MutableBytes,
    group: &Bound<'_, PyAny>,
    rows: &Bound<'_, PyAny>,
) -> PyResult<Allocation> {
    let group = strict_usize(group)?;
    let rows = strict_range(rows)?;
    image.with_mut(|image| {
        core::allocate_into(image, group, rows)
            .map(|inner| Allocation { inner })
            .map_err(errors::icf)
    })
}
#[pyfunction]
fn reset_home_into(
    py: Python<'_>,
    image: &MutableBytes,
    group: &Bound<'_, PyAny>,
    defaults: &ReferenceImage,
    home_invalidated: &MutableBytes,
) -> PyResult<()> {
    let group = strict_usize(group)?;
    with_home_byte(home_invalidated, |value| {
        defaults.with_core(py, |defaults| {
            image.with_mut(|image| {
                core::reset_home_into(image, group, defaults, value).map_err(errors::icf)
            })
        })
    })
}
#[pyfunction]
fn release_into(
    py: Python<'_>,
    image: &MutableBytes,
    group: &Bound<'_, PyAny>,
    rows: &Bound<'_, PyAny>,
    defaults: &ReferenceImage,
    home_invalidated: &MutableBytes,
) -> PyResult<usize> {
    let group = strict_usize(group)?;
    let rows = strict_range(rows)?;
    with_home_byte(home_invalidated, |value| {
        defaults.with_core(py, |defaults| {
            image.with_mut(|image| {
                core::release_into(image, group, rows, defaults, value).map_err(errors::icf)
            })
        })
    })
}
#[pyfunction]
fn delete_ur_into(
    py: Python<'_>,
    image: &MutableBytes,
    index: &Bound<'_, PyAny>,
    defaults: &ReferenceImage,
) -> PyResult<()> {
    let index = strict_u32(index)?;
    defaults.with_core(py, |defaults| {
        image.with_mut(|image| core::delete_ur_into(image, index, defaults).map_err(errors::icf))
    })
}
macro_rules! image_functions {
    ($($name:ident),*) => { $(#[pyfunction] fn $name(image: &MutableBytes) -> PyResult<()> { image.with_mut(|image|core::$name(image).map_err(errors::icf)) })* };
}
image_functions!(
    expand_map,
    expand_names,
    rebuild_metadata,
    expand_ur_count,
    expand_ur_data,
    rebuild_ur_metadata
);
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "rows")?;
    m.add_class::<NormalMemory>()?;
    m.add_class::<WorkingUr>()?;
    m.add_class::<Allocation>()?;
    m.add_function(wrap_pyfunction!(ur_default, &m)?)?;
    m.add_function(wrap_pyfunction!(allocate_into, &m)?)?;
    m.add_function(wrap_pyfunction!(reset_home_into, &m)?)?;
    m.add_function(wrap_pyfunction!(release_into, &m)?)?;
    m.add_function(wrap_pyfunction!(delete_ur_into, &m)?)?;
    m.add_function(wrap_pyfunction!(expand_map, &m)?)?;
    m.add_function(wrap_pyfunction!(expand_names, &m)?)?;
    m.add_function(wrap_pyfunction!(rebuild_metadata, &m)?)?;
    m.add_function(wrap_pyfunction!(expand_ur_count, &m)?)?;
    m.add_function(wrap_pyfunction!(expand_ur_data, &m)?)?;
    m.add_function(wrap_pyfunction!(rebuild_ur_metadata, &m)?)?;
    native::register(&m)?;
    ur::register(&m)?;
    parent.add_submodule(&m)
}
