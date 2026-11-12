//! Native print resource bindings, without native UI or printer delivery.
use super::*;
use cs705_core::printing::native as n;
use pyo3::IntoPyObjectExt;
fn out<'py, T: IntoPyObject<'py>>(py: Python<'py>, v: T) -> PyResult<Bound<'py, PyAny>> {
    v.into_bound_py_any(py)
}
fn bytes_out<'py>(py: Python<'py>, v: Vec<u8>) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyBytes::new(py, &v).into_any())
}
fn optional_bytes(v: &Bound<'_, PyAny>) -> PyResult<Option<Vec<u8>>> {
    if v.is_none() {
        Ok(None)
    } else {
        Ok(Some(bytes(v)?))
    }
}
fn optional_bytes_out<'py>(py: Python<'py>, v: Option<Vec<u8>>) -> PyResult<Bound<'py, PyAny>> {
    match v {
        Some(v) => bytes_out(py, v),
        None => Ok(py.None().into_bound(py)),
    }
}
fn optional_bool(v: &Bound<'_, PyAny>) -> PyResult<Option<bool>> {
    if v.is_none() {
        Ok(None)
    } else {
        Ok(Some(strict_bool(v)?))
    }
}
fn optional_u32(v: &Bound<'_, PyAny>) -> PyResult<Option<u32>> {
    if v.is_none() {
        Ok(None)
    } else {
        Ok(Some(strict_u32(v)?))
    }
}
fn class(v: &Bound<'_, PyAny>) -> PyResult<n::Class> {
    use n::Class::*;
    let values = [
        Base, Gps, Common, Preset, ColCut, Simplex, Split, Satellite, RxHistory, Groups, Repeater,
        Ur, Allowed,
    ];
    let code = crate::icf::enum_value(v, "cs705.printing.native", "Class")?;
    values
        .get(usize::try_from(code).unwrap_or(usize::MAX))
        .copied()
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid Class"))
}
#[pyclass(module = "cs705.printing.native", frozen)]
pub struct Factory {
    pub(crate) target: Target<n::Factory>,
}
impl Factory {
    pub fn from_core(v: n::Factory) -> Self {
        Self {
            target: Target::new(v),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&n::Factory) -> PyResult<R>) -> PyResult<R> {
        self.target.with_ref(f)
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut n::Factory) -> PyResult<R>) -> PyResult<R> {
        self.target.with_mut(f)
    }
}
#[pymethods]
impl Factory {
    #[new]
    fn new(
        class_: &Bound<'_, PyAny>,
        resource: String,
        node: &Bound<'_, PyAny>,
        group: &Bound<'_, PyAny>,
        parameter: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self::from_core(n::Factory {
            class: class(class_)?,
            resource,
            node: strict_i32(node)?,
            group: strict_i32(group)?,
            parameter: bytes(parameter)?,
        }))
    }
    #[staticmethod]
    fn parse(target: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(
            n::Factory::parse(strict_bytes(target)?).map_err(crate::errors::io)?,
        ))
    }
    #[getter]
    fn class_<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let v = self.with_ref(|v| Ok(v.class as i64))?;
        crate::icf::enum_result(py, "cs705.printing.native", "Class", v)
    }
    #[setter]
    fn set_class_(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = class(v)?;
        self.with_mut(|o| {
            o.class = v;
            Ok(())
        })
    }
    #[getter]
    fn resource(&self) -> PyResult<String> {
        self.with_ref(|v| Ok(v.resource.clone()))
    }
    #[setter]
    fn set_resource(&self, v: String) -> PyResult<()> {
        self.with_mut(|o| {
            o.resource = v;
            Ok(())
        })
    }
    #[getter]
    fn node(&self) -> PyResult<i32> {
        self.with_ref(|v| Ok(v.node))
    }
    #[setter]
    fn set_node(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_i32(v)?;
        self.with_mut(|o| {
            o.node = v;
            Ok(())
        })
    }
    #[getter]
    fn group(&self) -> PyResult<i32> {
        self.with_ref(|v| Ok(v.group))
    }
    #[setter]
    fn set_group(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = strict_i32(v)?;
        self.with_mut(|o| {
            o.group = v;
            Ok(())
        })
    }
    #[getter]
    fn parameter<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let v = self.with_ref(|v| Ok(v.parameter.clone()))?;
        Ok(PyBytes::new(py, &v))
    }
    #[setter]
    fn set_parameter(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = bytes(v)?;
        self.with_mut(|o| {
            o.parameter = v;
            Ok(())
        })
    }
    fn context_type(&self) -> PyResult<u32> {
        self.with_ref(|v| Ok(v.context_type()))
    }
    fn view(
        &self,
        current: &Bound<'_, PyAny>,
        show_split: &Bound<'_, PyAny>,
    ) -> PyResult<Option<crate::display::ViewState>> {
        let current = strict_bytes(current)?;
        let split = strict_u32(show_split)?;
        self.with_ref(|v| {
            Ok(v.view(current, split)
                .map_err(crate::errors::io)?
                .map(crate::display::ViewState::from_core))
        })
    }
}
macro_rules! record{
 ($name:ident,$ty:path,$module:literal; $([$($getattr:meta),*] [$($setattr:meta),*] $field:ident:$ft:ty=>$get:ident,$set:ident,$parse:ident,$output:ident);*;@extra{$($extra:tt)*})=>{
  #[pyclass(module=$module,frozen)]pub struct $name{pub(crate) target:Target<$ty>}
  impl $name{pub fn from_core(v:$ty)->Self{Self{target:Target::new(v)}}pub fn from_loan(loan:crate::scoped::Loan<$ty>)->Self{Self{target:Target::Scoped(loan)}}pub fn with_ref<R>(&self,f:impl FnOnce(&$ty)->PyResult<R>)->PyResult<R>{self.target.with_ref(f)}pub fn with_mut<R>(&self,f:impl FnOnce(&mut $ty)->PyResult<R>)->PyResult<R>{self.target.with_mut(f)}}
  #[pymethods]impl $name{
   #[new]fn new($($field:&Bound<'_,PyAny>),*)->PyResult<Self>{use $ty as Core;Ok(Self::from_core(Core{$($field:self::$parse($field)?),*}))}
   $($(#[$getattr])*fn $get<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{let v=self.with_ref(|v|Ok(v.$field.clone()))?;$output(py,v)}
   $(#[$setattr])*fn $set(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let v=$parse(v)?;self.with_mut(|o|{o.$field=v;Ok(())})})*
   $($extra)*
  }
 }
}
record!(CellNode,n::CellNode,"cs705.printing.native";
 [getter][setter]group:u32=>group,set_group,strict_u32,out;[getter][setter]key:u32=>key,set_key,strict_u32,out;[getter][setter]index:i32=>index,set_index,strict_i32,out;
 [getter][setter]name:Vec<u8> =>name,set_name,bytes,bytes_out;[getter][setter]flags1c:u8=>flags1c,set_flags1c,strict_u8,out;[getter][setter]flags1d:u8=>flags1d,set_flags1d,strict_u8,out;
 [getter][setter]comb:i32=>comb,set_comb,strict_i32,out;[getter][setter]above:usize=>above,set_above,strict_usize,out;[getter][setter]below:usize=>below,set_below,strict_usize,out;
 [getter][setter]merge:bool=>merge,set_merge,strict_bool,out;[getter][setter]typed_index:Option<u32> =>typed_index,set_typed_index,optional_u32,out;
 [getter][setter]record_visible:bool=>record_visible,set_record_visible,strict_bool,out;[getter][setter]literal:Vec<u8> =>literal,set_literal,bytes,bytes_out;
 [getter][setter]alias:Option<Vec<u8>> =>alias,set_alias,optional_bytes,optional_bytes_out;[getter][setter]checked:Option<bool> =>checked,set_checked,optional_bool,out;
 @extra{#[staticmethod]fn default()->Self{Self::from_core(n::CellNode::default())}});
pub(crate) fn factory(v: &Bound<'_, PyAny>) -> PyResult<n::Factory> {
    v.cast::<Factory>()?.get().with_ref(|v| Ok(v.clone()))
}
pub(crate) fn factory_out<'py>(py: Python<'py>, v: n::Factory) -> PyResult<Bound<'py, PyAny>> {
    Ok(Py::new(py, Factory::from_core(v))?
        .into_bound(py)
        .into_any())
}
pub(crate) fn node(v: &Bound<'_, PyAny>) -> PyResult<n::CellNode> {
    v.cast::<CellNode>()?.get().with_ref(|v| Ok(v.clone()))
}
fn node_out<'py>(py: Python<'py>, v: n::CellNode) -> PyResult<Bound<'py, PyAny>> {
    Ok(Py::new(py, CellNode::from_core(v))?
        .into_bound(py)
        .into_any())
}
pub(crate) fn nodes(v: &Bound<'_, PyAny>) -> PyResult<Option<Vec<Vec<n::CellNode>>>> {
    if v.is_none() {
        return Ok(None);
    }
    Ok(Some(
        v.try_iter()?
            .map(|row| row?.try_iter()?.map(|v| node(&v?)).collect())
            .collect::<PyResult<_>>()?,
    ))
}
record!(InlineFrame,n::InlineFrame,"cs705.printing.native";
 [getter][setter]column:usize=>column,set_column,strict_usize,out;[getter][setter]row:usize=>row,set_row,strict_usize,out;
 [][]node:n::CellNode=>node_snapshot,set_node,node,node_out;[][]factory:n::Factory=>factory_snapshot,set_factory,factory,factory_out;
 [getter][setter]input_code:u32=>input_code,set_input_code,strict_u32,out;[getter][setter]layout:u32=>layout,set_layout,strict_u32,out;
 [getter][setter]warning_bits:u8=>warning_bits,set_warning_bits,strict_u8,out;[getter][setter]focus:u8=>focus,set_focus,strict_u8,out;
 [getter][setter]action:u32=>action,set_action,strict_u32,out;[getter][setter]command:u32=>command,set_command,strict_u32,out;[getter][setter]phase:u32=>phase,set_phase,strict_u32,out;
 [getter][setter]notify:u32=>notify,set_notify,strict_u32,out;[getter][setter]initial_flags:u32=>initial_flags,set_initial_flags,strict_u32,out;
 [getter][setter]grid4:bool=>grid4,set_grid4,strict_bool,out;[getter][setter]grid8:bool=>grid8,set_grid8,strict_bool,out;
 [getter][setter]callback:u32=>callback,set_callback,strict_u32,out;[getter][setter]text:Vec<u8> =>text,set_text,bytes,bytes_out;
 @extra{#[staticmethod]fn commit(grid:&super::Grid,column:&Bound<'_,PyAny>,row:&Bound<'_,PyAny>,text:&Bound<'_,PyAny>)->PyResult<Self>{let column=strict_usize(column)?;let row=strict_usize(row)?;let text=strict_bytes(text)?;grid.with_ref(|v|Ok(Self::from_core(n::InlineFrame::commit(v,column,row,text).map_err(crate::errors::io)?)))}});
fn optional_i32(v: &Bound<'_, PyAny>) -> PyResult<Option<i32>> {
    if v.is_none() {
        Ok(None)
    } else {
        Ok(Some(strict_i32(v)?))
    }
}
record!(Command,n::menu::Command,"cs705.printing.native.menu";
 [][]node:n::CellNode=>node_snapshot,set_node,node,node_out;[getter][setter]depth:usize=>depth,set_depth,strict_usize,out;
 [getter][setter]separator:bool=>separator,set_separator,strict_bool,out;[getter][setter]flags1e:u8=>flags1e,set_flags1e,strict_u8,out;
 [getter][setter]shortcut18:u32=>shortcut18,set_shortcut18,strict_u32,out;[getter][setter]image_index:Option<i32> =>image_index,set_image_index,optional_i32,out;@extra{});
#[pyfunction]
fn byte_text(bytes: &Bound<'_, PyAny>) -> PyResult<String> {
    Ok(n::byte_text(strict_bytes(bytes)?))
}
#[pyfunction]
fn sync_editor(
    py: Python<'_>,
    editor: Py<super::Editor>,
    grid: Py<super::Grid>,
    column: &Bound<'_, PyAny>,
    row: &Bound<'_, PyAny>,
    dispatch: Py<PyAny>,
) -> PyResult<()> {
    let column = strict_usize(column)?;
    let row = strict_usize(row)?;
    let mut e = editor.get().target.take()?;
    let g = grid.get().target.take()?;
    let interpreter = crate::scoped::interpreter(py)?;
    let (errors, result) = py.detach(move || {
        let mut errors = crate::callbacks::CallbackError::default();
        let result = n::sync_editor(&mut e, &g, column, row, |frame| {
            let result = crate::icf::document::adapters::attach_checked(interpreter, |py| {
                crate::scoped::Frame::with_ref(py, frame, |loan| {
                    crate::icf::document::adapters::require_none(
                        dispatch
                            .bind(py)
                            .call1((Py::new(py, InlineFrame::from_loan(loan))?,))?,
                    )
                })
            });
            errors
                .capture(result)
                .map_err(|_| std::io::Error::other("Python native editor dispatch failed"))
        });
        drop(e);
        drop(g);
        (errors, result)
    });
    errors.finish(py, result.map_err(crate::errors::io))
}
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "native")?;
    crate::presentation::add_enum(
        &m,
        "cs705.printing.native",
        "Class",
        &[
            "Base",
            "Gps",
            "Common",
            "Preset",
            "ColCut",
            "Simplex",
            "Split",
            "Satellite",
            "RxHistory",
            "Groups",
            "Repeater",
            "Ur",
            "Allowed",
        ],
    )?;
    m.add_class::<Factory>()?;
    m.add_class::<CellNode>()?;
    m.add_class::<InlineFrame>()?;
    m.add("Resources", parent.getattr("Resources")?)?;
    m.add_function(wrap_pyfunction!(byte_text, &m)?)?;
    m.add_function(wrap_pyfunction!(sync_editor, &m)?)?;
    m.add_function(wrap_pyfunction!(super::io::read_source, &m)?)?;
    let menu = PyModule::new(parent.py(), "menu")?;
    menu.add_class::<Command>()?;
    m.add_submodule(&menu)?;
    parent.add_submodule(&m)
}
