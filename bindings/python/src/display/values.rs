//! Display value holders; copied collections use explicit snapshot/set methods.
use super::*;
use crate::callbacks::CallbackError;
use cs705_core::display as core;
use pyo3::types::{PyBytes, PyTuple};
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Cell>()?;
    m.add_class::<NoticeCell>()?;
    m.add_class::<Row>()?;
    m.add_class::<Toolbar>()?;
    crate::presentation::add_enum(
        m,
        "cs705.display",
        "NoticeParent",
        &["Agc", "Filter", "Ts", "ToneControl"],
    )?;
    m.add_function(wrap_pyfunction!(notice_cell, m)?)?;
    Ok(())
}
owner!(Cell, core::Cell);
owner!(NoticeCell, core::NoticeCell);
owner!(Row, core::Row);
owner!(Toolbar, core::Toolbar);

// PyO3 needs complete impl expansion, rather than method-position macros.
macro_rules! value_impl {
    ($name:ident; $($field:ident:$ty:ty=>$set:ident,$parse:ident);*; @extra {$($extra:tt)*})=>{
        #[pymethods] impl $name {
            $(#[getter] fn $field(&self)->PyResult<$ty>{self.with_ref(|v|Ok(v.$field))}
            #[setter] fn $set(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let value=$parse(value)?;self.with_mut(|v|{v.$field=value;Ok(())})})*
            $($extra)*
        }
    }
}
value_impl!(Cell;key:u32=>set_key,strict_u32;width:i32=>set_width,strict_i32;height:i32=>set_height,strict_i32;@extra {
    #[new] fn new(key:&Bound<'_,PyAny>,text:&Bound<'_,PyAny>,width:&Bound<'_,PyAny>,height:&Bound<'_,PyAny>)->PyResult<Self>{Ok(Self::from_core(core::Cell{key:strict_u32(key)?,text:strict_bytes(text)?.to_vec(),width:strict_i32(width)?,height:strict_i32(height)?}))}
    #[getter] fn text<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyBytes>>{let bytes=self.with_ref(|v|Ok(v.text.clone()))?;Ok(PyBytes::new(py,&bytes))}
    #[setter] fn set_text(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let text=strict_bytes(value)?.to_vec();self.with_mut(|v|{v.text=text;Ok(())})}
});
value_impl!(NoticeCell;topic:u32=>set_topic,strict_u32;key:u32=>set_key,strict_u32;index:i32=>set_index,strict_i32;foreground:Option<u32> =>set_foreground,optional_u32;ascii_event_disabled:bool=>set_ascii_event_disabled,strict_bool;editable:bool=>set_editable,strict_bool;@extra {
    #[new] fn new(topic:&Bound<'_,PyAny>,key:&Bound<'_,PyAny>,index:&Bound<'_,PyAny>,text:&Bound<'_,PyAny>,foreground:&Bound<'_,PyAny>,ascii_event_disabled:&Bound<'_,PyAny>,editable:&Bound<'_,PyAny>)->PyResult<Self>{Ok(Self::from_core(core::NoticeCell{topic:strict_u32(topic)?,key:strict_u32(key)?,index:strict_i32(index)?,text:strict_bytes(text)?.to_vec(),foreground:optional_u32(foreground)?,ascii_event_disabled:strict_bool(ascii_event_disabled)?,editable:strict_bool(editable)?}))}
    #[getter] fn text<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyBytes>>{let bytes=self.with_ref(|v|Ok(v.text.clone()))?;Ok(PyBytes::new(py,&bytes))}
    #[setter] fn set_text(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let text=strict_bytes(value)?.to_vec();self.with_mut(|v|{v.text=text;Ok(())})}
});
pub(crate) fn cells(values: Vec<Py<Cell>>) -> PyResult<Vec<core::Cell>> {
    values
        .iter()
        .map(|v| v.get().with_ref(|v| Ok(v.clone())))
        .collect()
}
value_impl!(Row;display_index:i32=>set_display_index,strict_i32;typed_index:u32=>set_typed_index,strict_u32;side:u8=>set_side,strict_u8;valid:bool=>set_valid,strict_bool;height:i32=>set_height,strict_i32;@extra {
    #[new] fn new(display_index:&Bound<'_,PyAny>,typed_index:&Bound<'_,PyAny>,side:&Bound<'_,PyAny>,valid:&Bound<'_,PyAny>,cells:Vec<Py<Cell>>,height:&Bound<'_,PyAny>)->PyResult<Self>{Ok(Self::from_core(core::Row{display_index:strict_i32(display_index)?,typed_index:strict_u32(typed_index)?,side:strict_u8(side)?,valid:strict_bool(valid)?,cells:self::cells(cells)?,height:strict_i32(height)?}))}
    fn cells_snapshot(&self)->PyResult<Vec<Cell>>{self.with_ref(|v|Ok(v.cells.clone()))?.into_iter().map(|v|Ok(Cell::from_core(v))).collect()}
    fn set_cells(&self,values:Vec<Py<Cell>>)->PyResult<()>{let values=cells(values)?;self.with_mut(|v|{v.cells=values;Ok(())})}
});

pub(crate) fn rect(value: &Bound<'_, PyAny>) -> PyResult<[i32; 4]> {
    let value = value.cast::<PyTuple>()?;
    if value.len() != 4 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "expected four placement integers",
        ));
    }
    Ok([
        strict_i32(&value.get_item(0)?)?,
        strict_i32(&value.get_item(1)?)?,
        strict_i32(&value.get_item(2)?)?,
        strict_i32(&value.get_item(3)?)?,
    ])
}
value_impl!(Toolbar;visible_changes:u64=>set_visible_changes,strict_u64;@extra {
    #[new] fn new(visible:&Bound<'_,PyAny>,placement:&Bound<'_,PyAny>,visible_changes:&Bound<'_,PyAny>)->PyResult<Self>{Ok(Self::from_core(core::Toolbar{visible:strict_bool(visible)?,placement:rect(placement)?,visible_changes:strict_u64(visible_changes)?}))}
    #[getter] fn placement(&self)->PyResult<(i32,i32,i32,i32)>{self.with_ref(|v|{let [a,b,c,d]=v.placement;Ok((a,b,c,d))})}
    #[getter] fn visible(&self)->PyResult<bool>{self.with_ref(|v|Ok(v.visible))}
    #[setter(visible)] fn set_visible_field(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let value=strict_bool(value)?;self.with_mut(|v|{v.visible=value;Ok(())})}
    #[setter] fn set_placement(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let value=rect(value)?;self.with_mut(|v|{v.placement=value;Ok(())})}
    fn occupied_height(&self)->PyResult<i32>{self.with_ref(|v|Ok(v.occupied_height()))}
    fn set_visible(&self,py:Python<'_>,visible:&Bound<'_,PyAny>,notify:Py<PyAny>)->PyResult<()>{
        let visible=strict_bool(visible)?;let mut toolbar=self.target.take()?;
        let mut errors=CallbackError::default();
        let native=toolbar.set_visible(visible,|value|{
            let result=crate::scoped::Frame::with_ref(py,value,|loan|{crate::icf::document::adapters::require_none(notify.bind(py).call1((Py::new(py,Self::from_loan(loan))?,))?)?;Ok(())});
            errors.capture(result).map_err(|_|std::io::Error::other("Python toolbar notification failed"))
        });
        drop(toolbar);errors.finish(py,native.map_err(crate::errors::io))
    }
});
#[pyfunction]
#[pyo3(signature=(pe,parent,key,index,canvas))]
fn notice_cell(
    pe: Option<&Bound<'_, PyAny>>,
    parent: &Bound<'_, PyAny>,
    key: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    canvas: &Bound<'_, PyAny>,
) -> PyResult<Option<NoticeCell>> {
    let code = crate::icf::enum_value(parent, "cs705.display", "NoticeParent")?;
    use core::NoticeParent::*;
    let parent = match code {
        0 => Agc,
        1 => Filter,
        2 => Ts,
        3 => ToneControl,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "invalid NoticeParent",
            ));
        }
    };
    Ok(core::notice_cell(
        pe.map(strict_bytes).transpose()?,
        parent,
        strict_u32(key)?,
        strict_i32(index)?,
        strict_bool(canvas)?,
    )
    .map_err(crate::errors::io)?
    .map(NoticeCell::from_core))
}
