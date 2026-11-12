//! Explicit callsign lookup sources, selection gestures and native list models.
use super::{
    frequency::Ranges,
    record::DecodedRecord,
    values::{Input, Output},
};
use crate::owner::Owner;
use crate::{conversion::*, errors};
use cs705_core::icf::callsign_dialog as core;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};
use pyo3::{PyTraverseError, PyVisit};
use std::sync::Arc;

#[pyclass(module = "cs705.icf.callsign_dialog")]
pub struct Sources {
    #[pyo3(get, set)]
    pub current: Py<PyBytes>,
    #[pyo3(get, set)]
    pub defaults: Py<PyBytes>,
    #[pyo3(get, set)]
    pub ranges: Py<Ranges>,
    system_langid: Option<u16>,
}
impl Sources {
    pub fn capture(&self, py: Python<'_>) -> Self {
        Self {
            current: self.current.clone_ref(py),
            defaults: self.defaults.clone_ref(py),
            ranges: self.ranges.clone_ref(py),
            system_langid: self.system_langid,
        }
    }
    pub fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::Sources<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let ranges = self.ranges.get().take()?;
        f(core::Sources {
            current: self.current.bind(py).as_bytes(),
            defaults: self.defaults.bind(py).as_bytes(),
            ranges: &ranges,
            system_langid: self.system_langid,
        })
    }
}
#[pymethods]
impl Sources {
    #[new]
    #[pyo3(signature = (current, defaults, ranges, system_langid))]
    fn new(
        current: Py<PyBytes>,
        defaults: Py<PyBytes>,
        ranges: Py<Ranges>,
        system_langid: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            current,
            defaults,
            ranges,
            system_langid: system_langid.map(strict_u16).transpose()?,
        })
    }
    #[getter]
    fn system_langid(&self) -> Option<u16> {
        self.system_langid
    }
    #[setter]
    fn set_system_langid(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.system_langid = value.map(strict_u16).transpose()?;
        Ok(())
    }
}

#[pyclass(module = "cs705.icf.callsign_dialog")]
pub struct ReadInputs {
    ur_name_tail: Option<[u8; 8]>,
    blank_source_index: Option<u32>,
    initial_first_marker: Option<i32>,
    #[pyo3(get, set)]
    pub string_tail: Py<PyBytes>,
}
impl ReadInputs {
    pub fn capture(&self, py: Python<'_>) -> Self {
        Self {
            ur_name_tail: self.ur_name_tail,
            blank_source_index: self.blank_source_index,
            initial_first_marker: self.initial_first_marker,
            string_tail: self.string_tail.clone_ref(py),
        }
    }
    pub fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::ReadInputs<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        f(core::ReadInputs {
            ur_name_tail: self.ur_name_tail,
            blank_source_index: self.blank_source_index,
            initial_first_marker: self.initial_first_marker,
            string_tail: self.string_tail.bind(py).as_bytes(),
        })
    }
}
#[pymethods]
impl ReadInputs {
    #[new]
    #[pyo3(signature = (ur_name_tail, blank_source_index, initial_first_marker, string_tail))]
    fn new(
        ur_name_tail: Option<&Bound<'_, PyAny>>,
        blank_source_index: Option<&Bound<'_, PyAny>>,
        initial_first_marker: Option<&Bound<'_, PyAny>>,
        string_tail: Py<PyBytes>,
    ) -> PyResult<Self> {
        Ok(Self {
            ur_name_tail: ur_name_tail.map(fixed_bytes).transpose()?,
            blank_source_index: blank_source_index.map(strict_u32).transpose()?,
            initial_first_marker: initial_first_marker.map(strict_i32).transpose()?,
            string_tail,
        })
    }
    #[staticmethod]
    fn default(py: Python<'_>) -> Self {
        Self {
            ur_name_tail: None,
            blank_source_index: None,
            initial_first_marker: None,
            string_tail: PyBytes::new(py, b"").unbind(),
        }
    }
    #[getter]
    fn ur_name_tail<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.ur_name_tail
            .as_ref()
            .map(|value| PyBytes::new(py, value))
    }
    #[setter]
    fn set_ur_name_tail(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.ur_name_tail = value.map(fixed_bytes).transpose()?;
        Ok(())
    }
    #[getter]
    fn blank_source_index(&self) -> Option<u32> {
        self.blank_source_index
    }
    #[setter]
    fn set_blank_source_index(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.blank_source_index = value.map(strict_u32).transpose()?;
        Ok(())
    }
    #[getter]
    fn initial_first_marker(&self) -> Option<i32> {
        self.initial_first_marker
    }
    #[setter]
    fn set_initial_first_marker(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.initial_first_marker = value.map(strict_i32).transpose()?;
        Ok(())
    }
}
macro_rules! value {
    ($name:ident { $($field:ident: $ty:ty => $set:ident;)* }) => {
        #[pyclass(module="cs705.icf.callsign_dialog")]
        pub struct $name { pub inner:core::$name }
        impl $name { pub fn from_core(inner:core::$name)->Self{Self{inner}} }
        #[pymethods] impl $name {
            #[new] fn new($($field:&Bound<'_,PyAny>),*)->PyResult<Self>{Ok(Self::from_core(core::$name{$($field:<$ty as Input>::input($field)?),*}))}
            $(#[getter] fn $field<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{self.inner.$field.clone().output(py)}
            #[setter] fn $set(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{self.inner.$field=<$ty as Input>::input(value)?;Ok(())})*
        }
    }
}
value!(ListSelection {category:i32=>set_category;name:i32=>set_name;call:i32=>set_call;modal_result:u32=>set_modal_result;checkbox:bool=>set_checkbox;});
value!(ListOutcome {modal_result:u32=>set_modal_result;save:bool=>set_save;copied:bool=>set_copied;checkbox:bool=>set_checkbox;mode:u32=>set_mode;});
value!(ListItem {row:u32=>set_row;text:Vec<u8> => set_text;});
value!(Category {tag:u32=>set_tag;text:Vec<u8> => set_text;});
value!(DialogOutcome {accepted:bool=>set_accepted;save:bool=>set_save;checkbox:bool=>set_checkbox;});

#[pyclass(module = "cs705.icf.callsign_dialog")]
pub struct ListModel {
    #[pyo3(get, set)]
    pub categories: Py<PyList>,
    #[pyo3(get, set)]
    pub names: Py<PyList>,
    #[pyo3(get, set)]
    pub calls: Py<PyList>,
    tag: u32,
}
impl ListModel {
    pub fn from_core(py: Python<'_>, value: core::ListModel) -> PyResult<Self> {
        let categories = value
            .categories
            .into_iter()
            .map(|value| Py::new(py, Category::from_core(value)))
            .collect::<PyResult<Vec<_>>>()?;
        let names = value
            .names
            .into_iter()
            .map(|value| Py::new(py, ListItem::from_core(value)))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            categories: PyList::new(py, categories)?.unbind(),
            names: PyList::new(py, names)?.unbind(),
            calls: PyList::new(py, value.calls.iter().map(|value| PyBytes::new(py, value)))?
                .unbind(),
            tag: value.tag,
        })
    }
}
#[pymethods]
impl ListModel {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.categories)?;
        visit.call(&self.names)?;
        visit.call(&self.calls)
    }
    #[new]
    fn new(
        categories: Py<PyList>,
        names: Py<PyList>,
        calls: Py<PyList>,
        tag: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            categories,
            names,
            calls,
            tag: strict_u32(tag)?,
        })
    }
    #[getter]
    fn tag(&self) -> u32 {
        self.tag
    }
    #[setter]
    fn set_tag(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.tag = strict_u32(value)?;
        Ok(())
    }
}
#[pyfunction]
fn lookup_ur_name(py: Python<'_>, s: &Sources, query: &Bound<'_, PyAny>) -> PyResult<u32> {
    let query = fixed_bytes(query)?;
    s.with_core(py, |s| {
        core::lookup_ur_name(&s, &query).map_err(errors::icf)
    })
}
#[pyfunction]
fn lookup_repeater_name(
    py: Python<'_>,
    s: &Sources,
    query: &Bound<'_, PyAny>,
    mode: &Bound<'_, PyAny>,
) -> PyResult<u32> {
    let query = fixed_bytes(query)?;
    let mode = strict_u32(mode)?;
    s.with_core(py, |s| {
        core::lookup_repeater_name(&s, &query, mode).map_err(errors::icf)
    })
}
#[pyfunction]
#[pyo3(signature=(s,query,optional_prefix,selector,mode))]
fn lookup_repeater(
    py: Python<'_>,
    s: &Sources,
    query: &Bound<'_, PyAny>,
    optional_prefix: Option<&Bound<'_, PyAny>>,
    selector: &Bound<'_, PyAny>,
    mode: &Bound<'_, PyAny>,
) -> PyResult<u32> {
    let query = strict_bytes(query)?;
    let prefix = optional_prefix.map(fixed_bytes).transpose()?;
    let selector = strict_u32(selector)?;
    let mode = strict_u32(mode)?;
    s.with_core(py, |s| {
        core::lookup_repeater(&s, query, prefix.as_ref(), selector, mode).map_err(errors::icf)
    })
}
#[pyfunction]
fn lookup_rpt2(
    py: Python<'_>,
    s: &Sources,
    prefix: &Bound<'_, PyAny>,
    query: &Bound<'_, PyAny>,
    flag: &Bound<'_, PyAny>,
    mode: &Bound<'_, PyAny>,
) -> PyResult<u32> {
    let prefix = fixed_bytes(prefix)?;
    let query = strict_bytes(query)?;
    let flag = strict_u32(flag)?;
    let mode = strict_u32(mode)?;
    s.with_core(py, |s| {
        core::lookup_rpt2(&s, &prefix, query, flag, mode).map_err(errors::icf)
    })
}
#[pyfunction]
fn slash_call<'py>(
    py: Python<'py>,
    call: &Bound<'_, PyAny>,
    reads: &ReadInputs,
) -> PyResult<Bound<'py, PyBytes>> {
    let call = fixed_bytes(call)?;
    let value = reads.with_core(py, |reads| {
        core::slash_call(&call, &reads).map_err(errors::icf)
    })?;
    Ok(PyBytes::new(py, &value))
}
#[pyfunction]
fn list_model(
    py: Python<'_>,
    s: &Sources,
    list_mode: &Bound<'_, PyAny>,
    mode: &Bound<'_, PyAny>,
    selection: &ListSelection,
    reads: &ReadInputs,
) -> PyResult<ListModel> {
    let list_mode = strict_u32(list_mode)?;
    let mode = strict_u32(mode)?;
    let value = s.with_core(py, |s| {
        reads.with_core(py, |reads| {
            core::list_model(&s, list_mode, mode, selection.inner, &reads).map_err(errors::icf)
        })
    })?;
    ListModel::from_core(py, value)
}
#[pyfunction]
fn list_into(
    py: Python<'_>,
    sources: &Sources,
    holder: &DecodedRecord,
    field: &Bound<'_, PyAny>,
    selection: &ListSelection,
    reads: &ReadInputs,
) -> PyResult<ListOutcome> {
    let field = <cs705_core::icf::MemoryCallsign as Input>::input(field)?;
    sources.with_core(py, |s| {
        reads.with_core(py, |reads| {
            holder.with_mut(|h| {
                core::list_into(&s, h, field, selection.inner, &reads)
                    .map(ListOutcome::from_core)
                    .map_err(errors::icf)
            })
        })
    })
}
#[pyfunction]
fn initial_selection(
    py: Python<'_>,
    s: &Sources,
    holder: &DecodedRecord,
    list_mode: &Bound<'_, PyAny>,
    query: &Bound<'_, PyAny>,
    other_mode: &Bound<'_, PyAny>,
    reads: &ReadInputs,
) -> PyResult<ListSelection> {
    let list_mode = strict_u32(list_mode)?;
    let query = strict_bytes(query)?;
    let other_mode = strict_u32(other_mode)?;
    s.with_core(py, |s| {
        reads.with_core(py, |reads| {
            holder.with_ref(|h| {
                core::initial_selection(&s, h, list_mode, query, other_mode, &reads)
                    .map(ListSelection::from_core)
                    .map_err(errors::icf)
            })
        })
    })
}
enum ActionValue {
    SourceText(Py<PyBytes>),
    UrText(Py<PyBytes>),
    RepeaterText(Py<PyBytes>),
    SourceList(Py<ListSelection>),
    UrList(Py<ListSelection>),
    Designation { selected: i32, accept: bool },
    Mode(u32),
}
#[pyclass(module = "cs705.icf.callsign_dialog", frozen)]
pub struct Action {
    value: ActionValue,
}
impl Action {
    pub fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::Action<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let value = match &self.value {
            ActionValue::SourceText(value) => core::Action::SourceText(value.bind(py).as_bytes()),
            ActionValue::UrText(value) => core::Action::UrText(value.bind(py).as_bytes()),
            ActionValue::RepeaterText(value) => {
                core::Action::RepeaterText(value.bind(py).as_bytes())
            }
            ActionValue::SourceList(value) => core::Action::SourceList(value.try_borrow(py)?.inner),
            ActionValue::UrList(value) => core::Action::UrList(value.try_borrow(py)?.inner),
            ActionValue::Designation { selected, accept } => core::Action::Designation {
                selected: *selected,
                accept: *accept,
            },
            ActionValue::Mode(value) => core::Action::Mode(*value),
        };
        f(value)
    }
}
#[pymethods]
#[allow(non_snake_case)]
impl Action {
    #[staticmethod]
    fn SourceText(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            value: ActionValue::SourceText(value.cast::<PyBytes>()?.clone().unbind()),
        })
    }
    #[staticmethod]
    fn UrText(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            value: ActionValue::UrText(value.cast::<PyBytes>()?.clone().unbind()),
        })
    }
    #[staticmethod]
    fn RepeaterText(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            value: ActionValue::RepeaterText(value.cast::<PyBytes>()?.clone().unbind()),
        })
    }
    #[staticmethod]
    fn SourceList(value: Py<ListSelection>) -> Self {
        Self {
            value: ActionValue::SourceList(value),
        }
    }
    #[staticmethod]
    fn UrList(value: Py<ListSelection>) -> Self {
        Self {
            value: ActionValue::UrList(value),
        }
    }
    #[staticmethod]
    fn Designation(selected: &Bound<'_, PyAny>, accept: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            value: ActionValue::Designation {
                selected: strict_i32(selected)?,
                accept: strict_bool(accept)?,
            },
        })
    }
    #[staticmethod]
    fn Mode(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            value: ActionValue::Mode(strict_u32(value)?),
        })
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self.value {
            ActionValue::SourceText(_) => "SourceText",
            ActionValue::UrText(_) => "UrText",
            ActionValue::RepeaterText(_) => "RepeaterText",
            ActionValue::SourceList(_) => "SourceList",
            ActionValue::UrList(_) => "UrList",
            ActionValue::Designation { .. } => "Designation",
            ActionValue::Mode(_) => "Mode",
        }
    }
    #[getter]
    fn text(&self, py: Python<'_>) -> Option<Py<PyBytes>> {
        match &self.value {
            ActionValue::SourceText(value)
            | ActionValue::UrText(value)
            | ActionValue::RepeaterText(value) => Some(value.clone_ref(py)),
            _ => None,
        }
    }
    #[getter]
    fn selection(&self, py: Python<'_>) -> Option<Py<ListSelection>> {
        match &self.value {
            ActionValue::SourceList(value) | ActionValue::UrList(value) => {
                Some(value.clone_ref(py))
            }
            _ => None,
        }
    }
    #[getter]
    fn selected(&self) -> Option<i32> {
        match self.value {
            ActionValue::Designation { selected, .. } => Some(selected),
            _ => None,
        }
    }
    #[getter]
    fn accept(&self) -> Option<bool> {
        match self.value {
            ActionValue::Designation { accept, .. } => Some(accept),
            _ => None,
        }
    }
    #[getter]
    fn mode(&self) -> Option<u32> {
        match self.value {
            ActionValue::Mode(value) => Some(value),
            _ => None,
        }
    }
}

fn byte_rows(value: &Bound<'_, PyAny>) -> PyResult<Vec<Vec<u8>>> {
    value
        .try_iter()?
        .map(|value| Ok(strict_bytes(&value?)?.to_vec()))
        .collect()
}
fn row_index(index: isize, count: usize) -> PyResult<usize> {
    let index = if index < 0 {
        count.checked_add_signed(index)
    } else {
        Some(index as usize)
    };
    index
        .filter(|index| *index < count)
        .ok_or_else(|| pyo3::exceptions::PyIndexError::new_err("dialog row index out of range"))
}
#[pyclass(module = "cs705.icf.callsign_dialog", name = "_ByteRows", frozen)]
pub struct ByteRows {
    parent: DialogOwner,
    preview: bool,
}
#[pymethods]
impl ByteRows {
    fn __len__(&self) -> PyResult<usize> {
        self.parent
            .with_ref(|d| Ok(if self.preview { 3 } else { d.candidates.len() }))
    }
    fn __getitem__<'py>(
        &self,
        py: Python<'py>,
        index: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let index = strict_isize(index)?;
        let value = self.parent.with_ref(|d| {
            let rows: &[Vec<u8>] = if self.preview {
                &d.preview
            } else {
                &d.candidates
            };
            Ok(rows[row_index(index, rows.len())?].clone())
        })?;
        Ok(PyBytes::new(py, &value))
    }
    fn __setitem__(&self, index: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let index = strict_isize(index)?;
        let value = strict_bytes(value)?.to_vec();
        self.parent.with_mut(|d| {
            let rows: &mut [Vec<u8>] = if self.preview {
                &mut d.preview
            } else {
                &mut d.candidates
            };
            let index = row_index(index, rows.len())?;
            rows[index] = value;
            Ok(())
        })
    }
    fn __delitem__(&self, index: &Bound<'_, PyAny>) -> PyResult<()> {
        let index = strict_isize(index)?;
        self.parent.with_mut(|d| {
            if self.preview {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "preview has fixed length",
                ));
            }
            let index = row_index(index, d.candidates.len())?;
            d.candidates.remove(index);
            Ok(())
        })
    }
    fn append(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bytes(value)?.to_vec();
        self.parent.with_mut(|d| {
            if self.preview {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "preview has fixed length",
                ));
            }
            d.candidates.push(value);
            Ok(())
        })
    }
}

#[pyclass(module = "cs705.icf.callsign_dialog", frozen)]
pub struct Dialog {
    owner: DialogOwner,
}
#[derive(Clone)]
pub(crate) enum DialogOwner {
    Owned(Arc<Owner<core::Dialog>>),
    Scoped(crate::scoped::Loan<core::Dialog>),
}
impl DialogOwner {
    pub(crate) fn with_ref<R>(&self, f: impl FnOnce(&core::Dialog) -> PyResult<R>) -> PyResult<R> {
        match self {
            Self::Owned(v) => v.with_ref(f),
            Self::Scoped(v) => Python::attach(|py| v.with_ref(py, f)),
        }
    }
    pub(crate) fn with_mut<R>(
        &self,
        f: impl FnOnce(&mut core::Dialog) -> PyResult<R>,
    ) -> PyResult<R> {
        match self {
            Self::Owned(v) => v.with_mut(f),
            Self::Scoped(v) => Python::attach(|py| v.with_mut(py, f)),
        }
    }
    fn take(&self) -> PyResult<crate::owner::Taken<'_, core::Dialog>> {
        match self {
            Self::Owned(v) => v.take(),
            Self::Scoped(_) => Err(pyo3::exceptions::PyRuntimeError::new_err(
                "consuming a scoped dialog is not allowed",
            )),
        }
    }
}
impl Dialog {
    pub fn from_loan(loan: crate::scoped::Loan<core::Dialog>) -> Self {
        Self {
            owner: DialogOwner::Scoped(loan),
        }
    }
    pub fn from_core(value: core::Dialog) -> Self {
        Self {
            owner: DialogOwner::Owned(Arc::new(Owner::new(value))),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::Dialog) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_ref(f)
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut core::Dialog) -> PyResult<R>) -> PyResult<R> {
        self.owner.with_mut(f)
    }
}
macro_rules! dialog_fields {
    ($($field:ident:$ty:ty=>$set:ident;)*)=>{
        #[pymethods] impl Dialog {
            #[new]
            #[expect(clippy::too_many_arguments, reason = "The constructor exposes every public native Dialog field, including both records and preview sources")]
            fn new(holder:&DecodedRecord,other:&DecodedRecord,$($field:&Bound<'_,PyAny>,)*candidates:&Bound<'_,PyAny>,preview:&Bound<'_,PyAny>)->PyResult<Self>{
                let holder=holder.with_ref(|h|Ok(h.clone()))?;let other=other.with_ref(|h|Ok(h.clone()))?;
                let candidates=byte_rows(candidates)?;let preview=byte_rows(preview)?.try_into().map_err(|_|pyo3::exceptions::PyValueError::new_err("preview requires exactly three rows"))?;
                Ok(Self::from_core(core::Dialog{holder,other,$($field:<$ty as Input>::input($field)?,)*candidates,preview}))
            }
            $(#[getter]fn $field<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{let value=self.with_ref(|d|Ok(d.$field.clone()))?;value.output(py)}
            #[setter]fn $set(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let value=<$ty as Input>::input(value)?;self.with_mut(|d|{d.$field=value;Ok(())})})*
            #[getter]fn holder(&self)->PyResult<DecodedRecord>{self.with_ref(|_|Ok(()))?;Ok(DecodedRecord::from_dialog(self.owner.clone(),false))}
            #[setter]fn set_holder(&self,value:&DecodedRecord)->PyResult<()>{let value=value.with_ref(|h|Ok(h.clone()))?;self.with_mut(|d|{d.holder=value;Ok(())})}
            #[getter]fn other(&self)->PyResult<DecodedRecord>{self.with_ref(|_|Ok(()))?;Ok(DecodedRecord::from_dialog(self.owner.clone(),true))}
            #[setter]fn set_other(&self,value:&DecodedRecord)->PyResult<()>{let value=value.with_ref(|h|Ok(h.clone()))?;self.with_mut(|d|{d.other=value;Ok(())})}
            #[getter]fn candidates(&self)->PyResult<ByteRows>{self.with_ref(|_|Ok(()))?;Ok(ByteRows{parent:self.owner.clone(),preview:false})}
            #[setter]fn set_candidates(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let value=byte_rows(value)?;self.with_mut(|d|{d.candidates=value;Ok(())})}
            #[getter]fn preview(&self)->PyResult<ByteRows>{self.with_ref(|_|Ok(()))?;Ok(ByteRows{parent:self.owner.clone(),preview:true})}
            #[setter]fn set_preview(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let value=byte_rows(value)?.try_into().map_err(|_|pyo3::exceptions::PyValueError::new_err("preview requires exactly three rows"))?;self.with_mut(|d|{d.preview=value;Ok(())})}
            #[staticmethod]
            fn open(py:Python<'_>,s:&Sources,original:&DecodedRecord,reads:&ReadInputs)->PyResult<Self>{s.with_core(py,|s|reads.with_core(py,|reads|original.with_mut(|h|core::Dialog::open(&s,h,&reads).map(Self::from_core).map_err(errors::icf))))}
            fn action(&self,py:Python<'_>,s:&Sources,action:&Action,reads:&ReadInputs)->PyResult<()>{s.with_core(py,|s|reads.with_core(py,|reads|action.with_core(py,|action|self.with_mut(|d|d.action(&s,action,&reads).map_err(errors::icf)))))}
            fn finish(&self,original:&DecodedRecord,modal_result:&Bound<'_,PyAny>)->PyResult<DialogOutcome>{let modal_result=strict_u32(modal_result)?;original.with_mut(|h|{let dialog=self.owner.take()?.consume();Ok(DialogOutcome::from_core(dialog.finish(h,modal_result)))})}
            fn list_model(&self,py:Python<'_>,s:&Sources,source:&Bound<'_,PyAny>,selection:&ListSelection,reads:&ReadInputs)->PyResult<ListModel>{let source=strict_bool(source)?;let value=s.with_core(py,|s|reads.with_core(py,|reads|self.with_ref(|d|d.list_model(&s,source,selection.inner,&reads).map_err(errors::icf))))?;ListModel::from_core(py,value)}
            fn initial_selection(&self,py:Python<'_>,s:&Sources,source:&Bound<'_,PyAny>,reads:&ReadInputs)->PyResult<ListSelection>{let source=strict_bool(source)?;s.with_core(py,|s|reads.with_core(py,|reads|self.with_ref(|d|d.initial_selection(&s,source,&reads).map(ListSelection::from_core).map_err(errors::icf))))}
        }
    }
}
dialog_fields! {
    mode:u32=>set_mode;source:[u8;16]=>set_source;your:[u8;16]=>set_your;
    repeater_text:Vec<u8> => set_repeater_text;repeater_caption:Vec<u8> => set_repeater_caption;
    source_class:u32=>set_source_class;your_class:u32=>set_your_class;selected:i32=>set_selected;
    first_marker:Option<i32> => set_first_marker;end_marker:i32=>set_end_marker;repeater_flag:u32=>set_repeater_flag;
    repeater_enabled:bool=>set_repeater_enabled;checkbox:bool=>set_checkbox;last_list_mode:Option<u32> => set_last_list_mode;
}
#[pyfunction]
fn prepare_metadata(py: Python<'_>, s: &Sources, h: &DecodedRecord) -> PyResult<()> {
    s.with_core(py, |s| {
        h.with_mut(|h| core::prepare_metadata(&s, h).map_err(errors::icf))
    })
}
#[pyfunction]
#[expect(
    clippy::too_many_arguments,
    reason = "The native request operation distinguishes both holders, three selections, raw bytes and explicit read sources"
)]
fn apply_requests(
    py: Python<'_>,
    s: &Sources,
    target: &DecodedRecord,
    other: &DecodedRecord,
    source: &Bound<'_, PyAny>,
    your: &Bound<'_, PyAny>,
    rpt: &Bound<'_, PyAny>,
    raw: &Bound<'_, PyAny>,
    reads: &ReadInputs,
) -> PyResult<()> {
    let source = strict_u32(source)?;
    let your = strict_u32(your)?;
    let rpt = strict_u32(rpt)?;
    let raw = fixed_bytes(raw)?;
    s.with_core(py, |s| {
        reads.with_core(py, |reads| {
            target.with_mut(|target| {
                other.with_ref(|other| {
                    core::apply_requests(&s, target, other, source, your, rpt, &raw, &reads)
                        .map_err(errors::icf)
                })
            })
        })
    })
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "callsign_dialog")?;
    m.add_class::<Sources>()?;
    m.add_class::<ReadInputs>()?;
    m.add_class::<ListSelection>()?;
    m.add_class::<ListOutcome>()?;
    m.add_class::<ListItem>()?;
    m.add_class::<Category>()?;
    m.add_class::<ListModel>()?;
    m.add_class::<Action>()?;
    m.add_class::<Dialog>()?;
    m.add_class::<DialogOutcome>()?;
    m.add_class::<ByteRows>()?;
    m.add("ByteRows", m.getattr("_ByteRows")?)?;
    m.add_function(wrap_pyfunction!(lookup_ur_name, &m)?)?;
    m.add_function(wrap_pyfunction!(lookup_repeater_name, &m)?)?;
    m.add_function(wrap_pyfunction!(lookup_repeater, &m)?)?;
    m.add_function(wrap_pyfunction!(lookup_rpt2, &m)?)?;
    m.add_function(wrap_pyfunction!(slash_call, &m)?)?;
    m.add_function(wrap_pyfunction!(list_model, &m)?)?;
    m.add_function(wrap_pyfunction!(list_into, &m)?)?;
    m.add_function(wrap_pyfunction!(initial_selection, &m)?)?;
    m.add_function(wrap_pyfunction!(prepare_metadata, &m)?)?;
    m.add_function(wrap_pyfunction!(apply_requests, &m)?)?;
    parent.add_submodule(&m)
}
