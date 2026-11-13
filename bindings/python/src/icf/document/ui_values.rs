//! UI metadata uses immutable snapshots and explicit whole-field replacement.
use crate::conversion::*;
use cs705_core::icf::document::{self as document, lifecycle as core};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};
use pyo3::{PyTraverseError, PyVisit};

fn paths(value: &Bound<'_, PyAny>) -> PyResult<Vec<Vec<u8>>> {
    value
        .try_iter()?
        .map(|v| Ok(strict_bytes(&v?)?.to_vec()))
        .collect()
}
fn bytes_tuple<'py>(py: Python<'py>, values: &[Vec<u8>]) -> PyResult<Bound<'py, PyTuple>> {
    PyTuple::new(py, values.iter().map(|v| PyBytes::new(py, v)))
}
fn view_values(
    py: Python<'_>,
    values: Vec<Py<super::values::ValidationView>>,
) -> PyResult<Vec<document::ValidationView>> {
    values.iter().map(|v| Ok(v.try_borrow(py)?.inner)).collect()
}
fn views_tuple<'py>(
    py: Python<'py>,
    values: &[document::ValidationView],
) -> PyResult<Bound<'py, PyTuple>> {
    let values = values
        .iter()
        .map(|v| Py::new(py, ValidationViewSnapshot { inner: *v }))
        .collect::<PyResult<Vec<_>>>()?;
    PyTuple::new(py, values)
}

macro_rules! snapshot {
    ($name:ident, $native:ty, {$($field:ident:$ty:ty),* $(,)?} {$($extra:tt)*}) => {
        #[pyclass(frozen, module = "cs705.icf.document.lifecycle")]
        pub struct $name { pub inner: $native }
        #[pymethods]
        impl $name {
            $(#[getter] fn $field(&self) -> $ty { self.inner.$field })*
            $($extra)*
        }
    };
}
snapshot!(FormStateSnapshot, core::FormState, {visible57:u8, enabled58:u8, focused:bool} {});
snapshot!(ValidationViewSnapshot, document::ValidationView, {kind:u32, group:u32, first:i32, last:i32, count:i32, include_side1:bool, show_both:u32} {});
snapshot!(ValidationReportSnapshot, document::ValidationReport, {errors:u32, warnings:u32} {
    #[getter] fn flags<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> { PyBytes::new(py, &self.inner.flags) }
});
snapshot!(PlacementSnapshot, core::Placement, {flags:u32, show:u32} {
    #[getter] fn minimum(&self) -> (i32,i32) { self.inner.minimum.into() }
    #[getter] fn maximum(&self) -> (i32,i32) { self.inner.maximum.into() }
    #[getter] fn normal(&self) -> (i32,i32,i32,i32) { self.inner.normal.into() }
});
snapshot!(ContextFieldsSnapshot, cs705_core::clone::ContextFields, {} {
    #[getter] fn values<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> { PyTuple::new(py, self.inner.0) }
});
snapshot!(TreeStateSnapshot, core::TreeState, {selected:Option<usize>} {
    #[getter] fn keys<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> { bytes_tuple(py, &self.inner.keys) }
    #[getter] fn groups<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> { bytes_tuple(py, &self.inner.groups) }
});
snapshot!(ViewStateSnapshot, core::ViewState, {} {
    #[getter] fn key<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> { PyBytes::new(py, &self.inner.key) }
    #[getter] fn views<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> { views_tuple(py, &self.inner.views) }
});
snapshot!(TitleStateSnapshot, core::TitleState, {dirty:u32, target:u32} {
    #[getter] fn path<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> { PyBytes::new(py, &self.inner.path) }
});
snapshot!(MenuDescriptorSnapshot, core::MenuDescriptor, {group:u32,key:u32,index:i32,flags1d:u8,flags1e:u8,shortcut18:u32} {
    #[getter] fn name<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> { PyBytes::new(py, &self.inner.name) }
    #[getter] fn caption<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> { PyBytes::new(py, &self.inner.caption) }
});
#[pyclass(frozen, module = "cs705.icf.document.lifecycle")]
pub struct MenuItemSnapshot {
    pub inner: core::MenuItem,
    #[pyo3(get)]
    pub query: Option<Py<super::menu::MenuQuery>>,
}
#[pymethods]
impl MenuItemSnapshot {
    #[getter]
    fn descriptor(&self) -> Option<MenuDescriptorSnapshot> {
        self.inner
            .descriptor
            .clone()
            .map(|inner| MenuDescriptorSnapshot { inner })
    }
    #[getter]
    fn property19a(&self) -> bool {
        self.inner.property19a
    }
    #[getter]
    fn property1ae(&self) -> bool {
        self.inner.property1ae
    }
    #[getter]
    fn property1a1(&self) -> bool {
        self.inner.property1a1
    }
    #[getter]
    fn enabled(&self) -> bool {
        self.inner.enabled
    }
    #[getter]
    fn shortcut(&self) -> u32 {
        self.inner.shortcut
    }
    #[getter]
    fn caption<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.caption)
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.query)
    }
}

#[pyclass(module = "cs705.icf.document.lifecycle")]
pub struct TreeState {
    pub inner: core::TreeState,
}
#[pymethods]
impl TreeState {
    #[new]
    #[pyo3(signature=(keys, selected, groups))]
    fn new(
        keys: &Bound<'_, PyAny>,
        selected: Option<&Bound<'_, PyAny>>,
        groups: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::TreeState {
                keys: paths(keys)?,
                selected: selected.map(strict_usize).transpose()?,
                groups: paths(groups)?,
            },
        })
    }
    #[staticmethod]
    fn default() -> Self {
        Self {
            inner: core::TreeState::default(),
        }
    }
    #[getter]
    fn selected(&self) -> Option<usize> {
        self.inner.selected
    }
    #[setter]
    fn set_selected(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.inner.selected = value.map(strict_usize).transpose()?;
        Ok(())
    }
    #[getter]
    fn keys<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        let keys = slf.try_borrow()?.inner.keys.clone();
        bytes_tuple(slf.py(), &keys)
    }
    #[setter(keys)]
    fn assign_keys(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = paths(value)?;
        slf.try_borrow_mut()?.inner.keys = value;
        Ok(())
    }
    #[getter]
    fn groups<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        let groups = slf.try_borrow()?.inner.groups.clone();
        bytes_tuple(slf.py(), &groups)
    }
    #[setter(groups)]
    fn assign_groups(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = paths(value)?;
        slf.try_borrow_mut()?.inner.groups = value;
        Ok(())
    }
}
#[pyclass(module = "cs705.icf.document.lifecycle")]
pub struct ViewState {
    pub inner: core::ViewState,
}
#[pymethods]
impl ViewState {
    #[new]
    fn new(
        py: Python<'_>,
        key: &Bound<'_, PyAny>,
        views: Vec<Py<super::values::ValidationView>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::ViewState {
                key: strict_bytes(key)?.to_vec(),
                views: view_values(py, views)?,
            },
        })
    }
    #[getter]
    fn key<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.key)
    }
    #[setter]
    fn set_key(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.key = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[getter]
    fn views<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        let values = slf.try_borrow()?.inner.views.clone();
        views_tuple(slf.py(), &values)
    }
    #[setter(views)]
    fn assign_views(
        slf: &Bound<'_, Self>,
        value: Vec<Py<super::values::ValidationView>>,
    ) -> PyResult<()> {
        let value = view_values(slf.py(), value)?;
        slf.try_borrow_mut()?.inner.views = value;
        Ok(())
    }
}
#[pyclass(module = "cs705.icf.document.lifecycle")]
pub struct TitleState {
    pub inner: core::TitleState,
}
#[pymethods]
impl TitleState {
    #[new]
    fn new(
        path: &Bound<'_, PyAny>,
        dirty: &Bound<'_, PyAny>,
        target: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: core::TitleState {
                path: strict_bytes(path)?.to_vec(),
                dirty: strict_u32(dirty)?,
                target: strict_u32(target)?,
            },
        })
    }
    #[getter]
    fn path<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.path)
    }
    #[setter]
    fn set_path(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.path = strict_bytes(value)?.to_vec();
        Ok(())
    }
    #[getter]
    fn dirty(&self) -> u32 {
        self.inner.dirty
    }
    #[setter]
    fn set_dirty(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.dirty = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn target(&self) -> u32 {
        self.inner.target
    }
    #[setter]
    fn set_target(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.target = strict_u32(value)?;
        Ok(())
    }
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<TreeState>()?;
    m.add_class::<ViewState>()?;
    m.add_class::<TitleState>()?;
    m.add_class::<FormStateSnapshot>()?;
    m.add_class::<ValidationViewSnapshot>()?;
    m.add_class::<ValidationReportSnapshot>()?;
    m.add_class::<PlacementSnapshot>()?;
    m.add_class::<ContextFieldsSnapshot>()?;
    m.add_class::<TreeStateSnapshot>()?;
    m.add_class::<ViewStateSnapshot>()?;
    m.add_class::<TitleStateSnapshot>()?;
    m.add_class::<MenuDescriptorSnapshot>()?;
    m.add_class::<MenuItemSnapshot>()?;
    Ok(())
}
