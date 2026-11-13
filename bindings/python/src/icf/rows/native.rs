//! Native row operations; mutable nested values use explicit snapshots/setters.
pub mod call;
use super::NormalMemory;
use crate::icf::{
    context::Context,
    frequency::Ranges,
    normalizer::ReferenceImage,
    record::DecodedRecord,
    settings::extended::{ChoiceInput, NumberInput, TextInput},
    values::{Input, Output},
};
use crate::{
    callbacks::{CallbackError, CallbackHolder, CallbackLink},
    conversion::*,
    errors,
    owner::{Owner, Taken},
    scoped::{Frame, Loan},
};
use cs705_core::icf::{record::ChannelWrite, rows::native as core};
use pyo3::{
    PyTraverseError, PyVisit,
    prelude::*,
    types::{PyBytes, PyList},
};

pub(super) enum Value<T: 'static> {
    Owned(Owner<T>),
    Scoped(Loan<T>),
}
impl<T> Value<T> {
    pub(super) fn new(value: T) -> Self {
        Self::Owned(Owner::new(value))
    }
    pub(super) fn read<R>(&self, py: Python<'_>, f: impl FnOnce(&T) -> PyResult<R>) -> PyResult<R> {
        match self {
            Self::Owned(v) => v.with_ref(f),
            Self::Scoped(v) => v.with_ref(py, f),
        }
    }
    pub(super) fn edit<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut T) -> PyResult<R>,
    ) -> PyResult<R> {
        match self {
            Self::Owned(v) => v.with_mut(f),
            Self::Scoped(v) => v.with_mut(py, f),
        }
    }
    pub(super) fn take(&self) -> PyResult<Taken<'_, T>> {
        match self {
            Self::Owned(v) => v.take(),
            Self::Scoped(_) => Err(pyo3::exceptions::PyRuntimeError::new_err(
                "operation requires an ordinary native owner",
            )),
        }
    }
}

macro_rules! value {
    ($(#[$constructor:meta])* $name:ident { $($field:ident:$ty:ty => $set:ident,)* } snapshots { $($nested:ident:$nty:ty => $get:ident,$nset:ident,)* } methods { $($methods:tt)* })=>{
        #[pyclass(module="cs705.icf.rows.native",frozen)]
        pub struct $name {pub(super) state:Value<core::$name>}
        impl $name {pub(super) fn from_core(v:core::$name)->Self{Self{state:Value::new(v)}}}
        impl Input<'_> for core::$name {fn input(v:&Bound<'_,PyAny>)->PyResult<Self>{v.extract::<PyRef<'_,$name>>()?.state.read(v.py(),|v|Ok(v.clone()))}}
        impl Output for core::$name {fn output<'py>(self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{Ok(Py::new(py,$name::from_core(self))?.into_bound(py).into_any())}}
        #[pymethods] impl $name {
            $(#[$constructor])* #[new] fn new($($field:&Bound<'_,PyAny>,)*$($nested:&Bound<'_,PyAny>,)*)->PyResult<Self>{Ok(Self::from_core(core::$name{$($field:<$ty as Input>::input($field)?,)*$($nested:<$nty as Input>::input($nested)?,)*}))}
            $(#[getter]fn $field<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{let v=self.state.read(py,|v|Ok(v.$field.clone()))?;v.output(py)}
            #[setter]fn $set(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()>{let v=<$ty as Input>::input(value)?;self.state.edit(py,|o|{o.$field=v;Ok(())})})*
            $(fn $get<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{let v=self.state.read(py,|v|Ok(v.$nested.clone()))?;v.output(py)}
            fn $nset(&self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()>{let v=<$nty as Input>::input(value)?;self.state.edit(py,|o|{o.$nested=v;Ok(())})})*
            $($methods)*
        }
    }
}
impl Output for (usize, usize) {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(self.into_pyobject(py)?.into_any())
    }
}
impl Input<'_> for cs705_core::icf::record::DecodedRecord {
    fn input(v: &Bound<'_, PyAny>) -> PyResult<Self> {
        v.extract::<PyRef<'_, DecodedRecord>>()?
            .with_ref(|h| Ok(h.clone()))
    }
}
impl Output for cs705_core::icf::record::DecodedRecord {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(Py::new(py, DecodedRecord::from_core(self))?
            .into_bound(py)
            .into_any())
    }
}
macro_rules! lists {($($ty:ty),*)=>{$(impl Input<'_> for Vec<$ty>{fn input(v:&Bound<'_,PyAny>)->PyResult<Self>{v.try_iter()?.map(|v|<$ty as Input>::input(&v?)).collect()}}
impl Output for Vec<$ty>{fn output<'py>(self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{let v=self.into_iter().map(|v|v.output(py)).collect::<PyResult<Vec<_>>>()?;Ok(PyList::new(py,v)?.into_any())}})*}}
lists!(cs705_core::icf::record::DecodedRecord, core::Notification);

value!(Selection {group:usize=>set_group,active:usize=>set_active,first:usize=>set_first,last:usize=>set_last,merged:bool=>set_merged,show_both:bool=>set_show_both,} snapshots{} methods{});
value!(CopySource {kind:u32=>set_kind,first:u32=>set_first,count:usize=>set_count,request:u32=>set_request,expanded:bool=>set_expanded,binding:u32=>set_binding,band_enabled:Option<bool> =>set_band_enabled,} snapshots{} methods{});
value!(Clipboard {source_kind:u32=>set_source_kind,mode:u32=>set_mode,declared_count:usize=>set_declared_count,logical_count:usize=>set_logical_count,} snapshots{records:Vec<cs705_core::icf::record::DecodedRecord> =>records_snapshot,set_records,} methods{
    #[staticmethod]fn default()->Self{Self::from_core(core::Clipboard::default())}
    fn count(&self,py:Python<'_>,paired:&Bound<'_,PyAny>)->PyResult<usize>{let paired=strict_bool(paired)?;self.state.read(py,|v|Ok(v.count(paired)))}
    fn validate(&self,py:Python<'_>)->PyResult<()>{self.state.read(py,|v|v.validate().map_err(errors::icf))}
    fn copy_from(&self,py:Python<'_>,image:&Bound<'_,PyAny>,source:&Bound<'_,PyAny>)->PyResult<()>{let source=core::CopySource::input(source)?;let image=strict_bytes(image)?;let mut v=self.state.take()?;py.detach(||v.copy_from(image,source)).map_err(errors::icf)}
});
value!(Notification {id:u32=>set_id,synchronous:bool=>set_synchronous,arg1:u32=>set_arg1,arg2:u32=>set_arg2,} snapshots{} methods{});
value!(#[expect(clippy::too_many_arguments, reason = "The constructor exposes all native row effect fields and its notification list")] Effects {home_invalidated:u8=>set_home_invalidated,dr_invalidated:u8=>set_dr_invalidated,context_flags:u8=>set_context_flags,selection:Option<(usize,usize)> =>set_selection,active:Option<usize> =>set_active,display_mode:Option<u32> =>set_display_mode,cache_invalidated:bool=>set_cache_invalidated,} snapshots{notifications:Vec<core::Notification> =>notifications_snapshot,set_notifications,} methods{
    #[staticmethod]fn default()->Self{Self::from_core(core::Effects::default())}
});
value!(Outcome {processed:bool=>set_processed,written:usize=>set_written,native_count:usize=>set_native_count,exhausted:bool=>set_exhausted,} snapshots{} methods{#[staticmethod]fn default()->Self{Self::from_core(core::Outcome::default())}});
value!(#[expect(clippy::too_many_arguments, reason = "The split frame retains independent native layout, selection, node and notification controls")] SplitFrame {layout:u32=>set_layout,pre_step:i32=>set_pre_step,selection_first:i32=>set_selection_first,node_index:i32=>set_node_index,node_flags:u8=>set_node_flags,notify_flags:u32=>set_notify_flags,active:i32=>set_active,display_mode:i32=>set_display_mode,} snapshots{} methods{});
value!(SplitOutcome {stored:bool=>set_stored,candidate:Option<i32> =>set_candidate,popup_requested:bool=>set_popup_requested,} snapshots{} methods{});
value!(ChoiceOutcome {stored:bool=>set_stored,candidate:Option<i32> =>set_candidate,mode_blocked:bool=>set_mode_blocked,popup_requested:bool=>set_popup_requested,} snapshots{} methods{});
value!(ChoiceFrame {layout:u32=>set_layout,node_flags:u8=>set_node_flags,} snapshots{} methods{});
value!(ChannelCaller {code:i32=>set_code,command:u32=>set_command,blocked:bool=>set_blocked,node_index:i32=>set_node_index,node_back:i32=>set_node_back,} snapshots{} methods{});
value!(CallsignInline {state_out:u32=>set_state_out,display:Vec<u8> =>set_display,cursor:usize=>set_cursor,selection:usize=>set_selection,width:usize=>set_width,} snapshots{} methods{});
value!(NameEffect {stored:bool=>set_stored,pending_requested:Option<[u8;24]> =>set_pending_requested,state_out:u32=>set_state_out,display:Vec<u8> =>set_display,shared_input:Vec<u8> =>set_shared_input,} snapshots{} methods{#[staticmethod]fn default()->Self{Self::from_core(core::NameEffect::default())}});
value!(NumericOutcome {candidate:u32=>set_candidate,accepted:bool=>set_accepted,state_out:u32=>set_state_out,} snapshots{} methods{});
value!(FrequencyOutcome {stored:bool=>set_stored,notify_flags:u32=>set_notify_flags,warning_bits:u8=>set_warning_bits,focus:u8=>set_focus,} snapshots{first:core::NumericOutcome=>first_snapshot,set_first,second:Option<core::NumericOutcome> =>second_snapshot,set_second,} methods{});
value!(EnumOutcome {context_state:Option<u32> =>set_context_state,warning_bits:Option<u8> =>set_warning_bits,} snapshots{choice:core::ChoiceOutcome=>choice_snapshot,set_choice,} methods{});
value!(ToneEffect {stored:bool=>set_stored,candidate:Option<i32> =>set_candidate,mode_blocked:bool=>set_mode_blocked,popup_requested:bool=>set_popup_requested,state_out:u32=>set_state_out,shared_input:Vec<u8> =>set_shared_input,inline_text:Option<Vec<u8>> =>set_inline_text,} snapshots{} methods{#[staticmethod]fn default()->Self{Self::from_core(core::ToneEffect::default())}});
value!(CodeOutcome {stored:bool=>set_stored,candidate:Option<u32> =>set_candidate,mode_blocked:bool=>set_mode_blocked,state_out:u32=>set_state_out,display:Vec<u8> =>set_display,selection:(usize,usize)=>set_selection,} snapshots{} methods{});
value!(PendingName {active:bool=>set_active,index:u32=>set_index,bytes:Vec<u8> =>set_bytes,} snapshots{} methods{fn reset(&self,py:Python<'_>)->PyResult<()>{self.state.edit(py,|v|{v.reset();Ok(())})}});
value!(EditorState {filter_enabled:u8=>set_filter_enabled,} snapshots{mode_cache:cs705_core::icf::record::DecodedRecord=>mode_cache_snapshot,set_mode_cache,filter_cache:cs705_core::icf::record::DecodedRecord=>filter_cache_snapshot,set_filter_cache,pending:core::PendingName=>pending_snapshot,set_pending,} methods{#[staticmethod]fn initialized()->Self{Self::from_core(core::EditorState::initialized())}});

macro_rules! enums {($($name:ident {$($v:ident),*});*)=>{
    $(impl Input<'_> for core::$name {
        fn input(v:&Bound<'_,PyAny>)->PyResult<Self>{
            let code=crate::icf::enum_value(v,"cs705.icf.rows.native",stringify!($name))?;
            $(if code==Self::$v as i64{return Ok(Self::$v);})*
            Err(pyo3::exceptions::PyValueError::new_err("invalid row enum"))
        }
    }
    impl Output for core::$name {
        fn output<'py>(self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{crate::icf::enum_result(py,"cs705.icf.rows.native",stringify!($name),self as i64)}
    })*
    fn register_enums(m:&Bound<'_,PyModule>)->PyResult<()>{
        $(crate::icf::add_enum(m,stringify!($name),&[$((stringify!($v),core::$name::$v as i64)),*])?;
        m.getattr(stringify!($name))?.setattr("__module__","cs705.icf.rows.native")?;)*
        Ok(())
    }
}}
enums!(Confirmation{Overwrite,Truncate,Clear,Delete,DiscardTail};ChannelChoice{Selection,Duplex,Polarity,DigitalSquelch};ChannelEnum{Mode,Data,Filter};ToneField{Tone,Tx,Rx,Dtcs});

#[pyfunction]
fn _channel_choice_key(field: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(core::ChannelChoice::input(field)?.key())
}
#[pyfunction]
fn _channel_choice_selector(field: &Bound<'_, PyAny>) -> PyResult<&'static str> {
    Ok(core::ChannelChoice::input(field)?.selector())
}
#[pyfunction]
fn _channel_choice_choices(field: &Bound<'_, PyAny>, holder: &DecodedRecord) -> PyResult<Vec<i64>> {
    let field = core::ChannelChoice::input(field)?;
    holder.with_ref(|h| Ok(field.choices(h)))
}
#[pyfunction]
fn _channel_enum_key(field: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(core::ChannelEnum::input(field)?.key())
}
#[pyfunction]
fn _channel_enum_choices(field: &Bound<'_, PyAny>, holder: &DecodedRecord) -> PyResult<Vec<i64>> {
    let field = core::ChannelEnum::input(field)?;
    holder.with_ref(|h| Ok(field.choices(h)))
}
#[pyfunction]
fn _tone_field_key(field: &Bound<'_, PyAny>) -> PyResult<u32> {
    Ok(core::ToneField::input(field)?.key())
}

#[pyclass(module = "cs705.icf.rows.native")]
pub struct ChannelEntry {
    value: core::ChannelEntry,
}
impl From<core::ChannelEntry> for ChannelEntry {
    fn from(value: core::ChannelEntry) -> Self {
        Self { value }
    }
}
#[pymethods]
#[allow(non_snake_case)]
impl ChannelEntry {
    #[staticmethod]
    fn Edit() -> Self {
        core::ChannelEntry::Edit.into()
    }
    #[staticmethod]
    fn Blocked() -> Self {
        core::ChannelEntry::Blocked.into()
    }
    #[staticmethod]
    fn Redirect(
        key: &Bound<'_, PyAny>,
        index: &Bound<'_, PyAny>,
        code: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(core::ChannelEntry::Redirect {
            key: strict_u32(key)?,
            index: strict_i32(index)?,
            code: strict_i32(code)?,
        }
        .into())
    }
    #[getter]
    fn variant(&self) -> &'static str {
        match self.value {
            core::ChannelEntry::Edit => "Edit",
            core::ChannelEntry::Blocked => "Blocked",
            _ => "Redirect",
        }
    }
    #[getter]
    fn key(&self) -> Option<u32> {
        match self.value {
            core::ChannelEntry::Redirect { key, .. } => Some(key),
            _ => None,
        }
    }
    #[getter]
    fn index(&self) -> Option<i32> {
        match self.value {
            core::ChannelEntry::Redirect { index, .. } => Some(index),
            _ => None,
        }
    }
    #[getter]
    fn code(&self) -> Option<i32> {
        match self.value {
            core::ChannelEntry::Redirect { code, .. } => Some(code),
            _ => None,
        }
    }
    #[setter]
    fn set_key(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        match &mut self.value {
            core::ChannelEntry::Redirect { key, .. } => {
                *key = strict_u32(value)?;
                Ok(())
            }
            _ => Err(pyo3::exceptions::PyAttributeError::new_err(
                "only Redirect has key",
            )),
        }
    }
    #[setter]
    fn set_index(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        match &mut self.value {
            core::ChannelEntry::Redirect { index, .. } => {
                *index = strict_i32(value)?;
                Ok(())
            }
            _ => Err(pyo3::exceptions::PyAttributeError::new_err(
                "only Redirect has index",
            )),
        }
    }
    #[setter]
    fn set_code(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        match &mut self.value {
            core::ChannelEntry::Redirect { code, .. } => {
                *code = strict_i32(value)?;
                Ok(())
            }
            _ => Err(pyo3::exceptions::PyAttributeError::new_err(
                "only Redirect has code",
            )),
        }
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

#[pyfunction]
fn compatible(source: &Bound<'_, PyAny>, destination: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(core::compatible(
        strict_u32(source)?,
        strict_u32(destination)?,
    ))
}
#[pyfunction]
fn channel_choice_input(
    py: Python<'_>,
    holder: &DecodedRecord,
    field: &Bound<'_, PyAny>,
    input: &ChoiceInput,
) -> PyResult<ChoiceOutcome> {
    let field = core::ChannelChoice::input(field)?;
    let input = input.to_core(py)?;
    holder.with_mut(|h| {
        core::channel_choice_input(h, field, input)
            .map(ChoiceOutcome::from_core)
            .map_err(errors::icf)
    })
}
#[pyfunction]
fn channel_enum_input(
    py: Python<'_>,
    holder: &DecodedRecord,
    field: &Bound<'_, PyAny>,
    input: &ChoiceInput,
) -> PyResult<ChoiceOutcome> {
    let field = core::ChannelEnum::input(field)?;
    let input = input.to_core(py)?;
    holder.with_mut(|h| {
        core::channel_enum_input(h, field, input)
            .map(ChoiceOutcome::from_core)
            .map_err(errors::icf)
    })
}
#[pyfunction]
fn channel_offset_input(
    py: Python<'_>,
    holder: &DecodedRecord,
    input: &NumberInput,
    point: &Bound<'_, PyAny>,
) -> PyResult<NumericOutcome> {
    let point = strict_u8(point)?;
    input.with_core(py, |input| {
        holder.with_mut(|h| {
            core::channel_offset_input(h, input, point)
                .map(NumericOutcome::from_core)
                .map_err(errors::icf)
        })
    })
}
#[pyfunction]
fn channel_entry(
    py: Python<'_>,
    holder: &DecodedRecord,
    key: &Bound<'_, PyAny>,
    caller: &Bound<'_, PyAny>,
    callsign_dialog: &Bound<'_, PyAny>,
) -> PyResult<ChannelEntry> {
    let (key, caller, dialog) = (
        strict_u32(key)?,
        core::ChannelCaller::input(caller)?,
        strict_bool(callsign_dialog)?,
    );
    let _ = py;
    holder.with_ref(|h| {
        core::channel_entry(h, key, caller, dialog)
            .map(Into::into)
            .map_err(errors::icf)
    })
}
#[pyfunction]
fn name_notification<'py>(
    py: Python<'py>,
    holder: &DecodedRecord,
    node_index: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let index = strict_u32(node_index)?;
    let v = holder.with_ref(|h| core::name_notification(h, index).map_err(errors::icf))?;
    Ok(PyBytes::new(py, &v))
}
#[pyfunction]
fn callsign_inline(
    holder: &DecodedRecord,
    field: &Bound<'_, PyAny>,
    code: &Bound<'_, PyAny>,
) -> PyResult<CallsignInline> {
    let (field, code) = (
        cs705_core::icf::MemoryCallsign::input(field)?,
        strict_i32(code)?,
    );
    holder.with_ref(|h| {
        core::callsign_inline(h, field, code)
            .map(CallsignInline::from_core)
            .map_err(errors::icf)
    })
}
#[pyfunction]
fn digital_code_label<'py>(
    py: Python<'py>,
    value: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &core::digital_code_label(strict_u32(value)?),
    ))
}
#[pyfunction]
fn tone_choices(field: &Bound<'_, PyAny>, live_q: Option<Py<Context>>) -> PyResult<Vec<i64>> {
    let field = core::ToneField::input(field)?;
    let q = live_q.map(|q| q.get().with_ref(|q| Ok(*q))).transpose()?;
    core::tone_choices(field, q.as_ref()).map_err(errors::icf)
}

trait ClonePy {
    fn clone_py(&self, py: Python<'_>) -> Self;
}
impl<T> ClonePy for Py<T> {
    fn clone_py(&self, py: Python<'_>) -> Self {
        self.clone_ref(py)
    }
}
impl<T> ClonePy for Option<Py<T>> {
    fn clone_py(&self, py: Python<'_>) -> Self {
        self.as_ref().map(|v| v.clone_ref(py))
    }
}
macro_rules! request {
    ($(#[$constructor:meta])* $name:ident refs{$($r:ident:$rt:ty,)*} scalars{$($s:ident:$st:ty=>$set:ident,)*} callbacks{$($cb:ident=>$cbset:ident,)*})=>{
        #[pyclass(module="cs705.icf.rows.native")]
        pub struct $name {$(#[pyo3(get,set)]$r:$rt,)*$($s:$st,)*$($cb:Py<CallbackHolder>,)*}
        impl $name {fn inputs(&self,py:Python<'_>)->Self{Self{$($r:self.$r.clone_py(py),)*$($s:self.$s.clone(),)*$($cb:self.$cb.clone_ref(py),)*}}}
        #[pymethods]impl $name {
            $(#[$constructor])* #[new]fn new(py:Python<'_>,$($r:$rt,)*$($s:&Bound<'_,PyAny>,)*$($cb:&Bound<'_,PyAny>,)*)->PyResult<Self>{let _=py;Ok(Self{$($r,)*$($s:<$st as Input>::input($s)?,)*$($cb:CallbackHolder::new(py,$cb)?,)*})}
            $(#[getter]fn $s<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{self.$s.clone().output(py)}#[setter]fn $set(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{self.$s=<$st as Input>::input(value)?;Ok(())})*
            $(#[getter]fn $cb(&self,py:Python<'_>)->PyResult<Py<PyAny>>{CallbackHolder::link(self.$cb.bind(py)).with_callable(py,|c|Ok(c.clone().unbind()))}#[setter]fn $cbset(&mut self,py:Python<'_>,value:&Bound<'_,PyAny>)->PyResult<()>{self.$cb=CallbackHolder::new(py,value)?;Ok(())})*
            fn __traverse__(&self,visit:PyVisit<'_>)->Result<(),PyTraverseError>{$(visit.call(&self.$r)?;)*$(visit.call(&self.$cb)?;)*Ok(())}
        }
    }
}
request!(EditorContext refs{ranges:Py<Ranges>,generation:Py<ReferenceImage>,}scalars{}callbacks{});
request!(CacheWrite refs{previous:Option<Py<DecodedRecord>>,}scalars{key:u32=>set_key,}callbacks{});
request!(ChoiceRequest refs{input:Py<ChoiceInput>,context:Py<EditorContext>,selection:Option<Py<ChoiceFrame>>,}scalars{field:core::ChannelChoice=>set_field,}callbacks{});
request!(NameRequest refs{input:Py<TextInput>,context:Py<EditorContext>,frame:Py<ChoiceFrame>,}scalars{notification:Option<[u8;24]> =>set_notification,}callbacks{});
request!(SplitRequest refs{input:Py<ChoiceInput>,context:Py<EditorContext>,frame:Py<SplitFrame>,}scalars{}callbacks{});
request!(CallsignTextRequest refs{input:Py<TextInput>,context:Py<EditorContext>,}scalars{field:cs705_core::icf::MemoryCallsign=>set_field,right_word:bool=>set_right_word,}callbacks{upper=>set_upper,});
request!(FrequencyRequest refs{input:Py<NumberInput>,context:Py<EditorContext>,live_q:Py<Context>,}scalars{point:u8=>set_point,warning_bits:u8=>set_warning_bits,focus:u8=>set_focus,}callbacks{});
request!(OffsetRequest refs{input:Py<NumberInput>,context:Py<EditorContext>,}scalars{point:u8=>set_point,}callbacks{});
request!(EnumRequest refs{input:Py<ChoiceInput>,context:Py<EditorContext>,}scalars{field:core::ChannelEnum=>set_field,warning_bits:Option<u8> =>set_warning_bits,}callbacks{});
request!(#[expect(clippy::too_many_arguments, reason = "Tone input keeps the native numeric source, live Q, field, inline controls and uppercase producer explicit")] ToneInput refs{input:Py<NumberInput>,live_q:Option<Py<Context>>,}scalars{field:core::ToneField=>set_field,point:u8=>set_point,inline:bool=>set_inline,first:Option<u8> =>set_first,}callbacks{upper=>set_upper,});
request!(ToneRequest refs{input:Py<ToneInput>,context:Py<EditorContext>,}scalars{}callbacks{});
request!(CodeRequest refs{input:Py<NumberInput>,node_flags:Py<MutableBytes>,context:Py<EditorContext>,}scalars{first:Option<u8> =>set_first,}callbacks{});

#[pyclass(module = "cs705.icf.rows.native")]
pub struct CallsignModal {
    selection: Option<Py<crate::icf::callsign_dialog::ListSelection>>,
    modal_result: Option<u32>,
}
#[pymethods]
#[allow(non_snake_case)]
impl CallsignModal {
    #[staticmethod]
    fn List(selection: Py<crate::icf::callsign_dialog::ListSelection>) -> Self {
        Self {
            selection: Some(selection),
            modal_result: None,
        }
    }
    #[staticmethod]
    fn Dr(modal_result: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            selection: None,
            modal_result: Some(strict_u32(modal_result)?),
        })
    }
    #[getter]
    fn variant(&self) -> &'static str {
        if self.modal_result.is_some() {
            "Dr"
        } else {
            "List"
        }
    }
    #[getter]
    fn selection(&self, py: Python<'_>) -> Option<Py<crate::icf::callsign_dialog::ListSelection>> {
        self.selection.clone_py(py)
    }
    #[setter]
    fn set_selection(
        &mut self,
        value: Py<crate::icf::callsign_dialog::ListSelection>,
    ) -> PyResult<()> {
        if self.modal_result.is_some() {
            return Err(pyo3::exceptions::PyAttributeError::new_err(
                "Dr has no selection",
            ));
        }
        self.selection = Some(value);
        Ok(())
    }
    #[getter]
    fn modal_result(&self) -> Option<u32> {
        self.modal_result
    }
    #[setter]
    fn set_modal_result(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        if self.modal_result.is_none() {
            return Err(pyo3::exceptions::PyAttributeError::new_err(
                "List has no modal_result",
            ));
        }
        self.modal_result = Some(strict_u32(value)?);
        Ok(())
    }
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.selection)
    }
}
impl CallsignModal {
    fn native(&self, py: Python<'_>) -> PyResult<core::CallsignModal> {
        match self.modal_result {
            Some(modal_result) => Ok(core::CallsignModal::Dr { modal_result }),
            None => Ok(core::CallsignModal::List(
                self.selection
                    .as_ref()
                    .ok_or_else(|| {
                        pyo3::exceptions::PyRuntimeError::new_err("modal selection was cleared")
                    })?
                    .try_borrow(py)?
                    .inner,
            )),
        }
    }
}
request!(CallsignModalRequest refs{modal:Py<CallsignModal>,sources:Py<crate::icf::callsign_dialog::Sources>,reads:Py<crate::icf::callsign_dialog::ReadInputs>,context:Py<EditorContext>,}scalars{field:cs705_core::icf::MemoryCallsign=>set_field,}callbacks{});
impl Input<'_> for cs705_core::icf::callsign_dialog::ListSelection {
    fn input(v: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(
            v.extract::<PyRef<'_, crate::icf::callsign_dialog::ListSelection>>()?
                .inner,
        )
    }
}
impl Output for cs705_core::icf::callsign_dialog::ListSelection {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(Py::new(
            py,
            crate::icf::callsign_dialog::ListSelection::from_core(self),
        )?
        .into_bound(py)
        .into_any())
    }
}
value!(CallsignModalOutcome{stored:bool=>set_stored,accepted:bool=>set_accepted,al:bool=>set_al,checkbox:bool=>set_checkbox,}snapshots{initial_selection:Option<cs705_core::icf::callsign_dialog::ListSelection> =>initial_selection_snapshot,set_initial_selection,}methods{});

impl EditorContext {
    fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::EditorContext<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        self.ranges.get().with_ref(|ranges| {
            self.generation.get().with_core(py, |generation| {
                f(core::EditorContext { ranges, generation })
            })
        })
    }
}
fn context<R>(
    py: Python<'_>,
    value: &Py<EditorContext>,
    f: impl FnOnce(core::EditorContext<'_>) -> PyResult<R>,
) -> PyResult<R> {
    let value = value.try_borrow(py)?.inputs(py);
    value.with_core(py, f)
}
impl CacheWrite {
    fn with_core<R>(&self, f: impl FnOnce(core::CacheWrite<'_>) -> PyResult<R>) -> PyResult<R> {
        let previous = self.previous.as_ref().map(|v| v.get().take()).transpose()?;
        f(core::CacheWrite {
            key: self.key,
            previous: previous.as_deref(),
        })
    }
}
impl ChoiceRequest {
    fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::ChoiceRequest<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let input = self.input.get().to_core(py)?;
        let mut selection = self
            .selection
            .as_ref()
            .map(|v| v.get().state.take())
            .transpose()?;
        context(py, &self.context, |context| {
            f(core::ChoiceRequest {
                field: self.field,
                input,
                context,
                selection: selection.as_deref_mut(),
            })
        })
    }
}
impl NameRequest {
    fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::NameRequest<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let mut frame = self.frame.get().state.take()?;
        self.input.get().with_core(py, |input| {
            context(py, &self.context, |context| {
                f(core::NameRequest {
                    input,
                    context,
                    frame: &mut frame,
                    notification: self.notification,
                })
            })
        })
    }
}
impl SplitRequest {
    fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::SplitRequest<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let input = self.input.get().to_core(py)?;
        let mut frame = self.frame.get().state.take()?;
        context(py, &self.context, |context| {
            f(core::SplitRequest {
                input,
                context,
                frame: &mut frame,
            })
        })
    }
}
impl OffsetRequest {
    fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::OffsetRequest<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        self.input.get().with_core(py, |input| {
            context(py, &self.context, |context| {
                f(core::OffsetRequest {
                    input,
                    point: self.point,
                    context,
                })
            })
        })
    }
}
impl EnumRequest {
    fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::EnumRequest<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let input = self.input.get().to_core(py)?;
        context(py, &self.context, |context| {
            f(core::EnumRequest {
                field: self.field,
                input,
                context,
                warning_bits: self.warning_bits,
            })
        })
    }
}
impl FrequencyRequest {
    fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::FrequencyRequest<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let q = self.live_q.get().with_ref(|q| Ok(*q))?;
        self.input.get().with_core(py, |input| {
            context(py, &self.context, |context| {
                f(core::FrequencyRequest {
                    input,
                    point: self.point,
                    context,
                    live_q: &q,
                    warning_bits: self.warning_bits,
                    focus: self.focus,
                })
            })
        })
    }
}
impl CodeRequest {
    fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::CodeRequest<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let mut flags = self.node_flags.get().take()?;
        let [flags] = flags.as_mut_slice() else {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "node_flags requires exactly one byte",
            ));
        };
        self.input.get().with_core(py, |input| {
            context(py, &self.context, |context| {
                f(core::CodeRequest {
                    input,
                    first: self.first,
                    node_flags: flags,
                    context,
                })
            })
        })
    }
}

fn call<T>(
    link: &CallbackLink,
    f: impl FnOnce(Python<'_>, &Bound<'_, PyAny>) -> PyResult<T>,
) -> Result<T, cs705_core::icf::Error> {
    Python::attach(|py| link.invoke(py, |c| f(py, c)))
        .map_err(|_| cs705_core::icf::Error::Operation("native row callback failed".into()))
}
impl CallsignTextRequest {
    fn detached<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::CallsignTextRequest<'_>) -> R + Send,
    ) -> PyResult<R> {
        let link = CallbackHolder::link(self.upper.bind(py));
        self.input.get().with_core(py, |input| {
            context(py, &self.context, |context| {
                Ok(py.detach(|| {
                    let mut upper = |b| call(&link, |_, c| strict_i32(&c.call1((b,))?));
                    f(core::CallsignTextRequest {
                        field: self.field,
                        input,
                        right_word: self.right_word,
                        upper: &mut upper,
                        context,
                    })
                }))
            })
        })
    }
}
impl ToneInput {
    fn detached<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::ToneInput<'_>) -> R + Send,
    ) -> PyResult<R> {
        let link = CallbackHolder::link(self.upper.bind(py));
        let q = self
            .live_q
            .as_ref()
            .map(|q| q.get().with_ref(|q| Ok(*q)))
            .transpose()?;
        self.input.get().with_core(py, |input| {
            Ok(py.detach(|| {
                let mut upper = |bytes: &mut [u8]| {
                    Python::attach(|py| {
                        link.invoke(py, |c| {
                            crate::csv::nls::upper_call(py, c, bytes).map(|_| ())
                        })
                    })
                    .map_err(|_| {
                        cs705_core::csv::Error::Operation(
                            "native tone uppercase callback failed".into(),
                        )
                    })
                };
                f(core::ToneInput {
                    field: self.field,
                    input,
                    point: self.point,
                    live_q: q.as_ref(),
                    inline: self.inline,
                    first: self.first,
                    upper: &mut upper,
                })
            }))
        })
    }
}
impl ToneRequest {
    fn detached<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::ToneRequest<'_>) -> R + Send,
    ) -> PyResult<R> {
        let input = self.input.try_borrow(py)?.inputs(py);
        let link = CallbackHolder::link(input.upper.bind(py));
        let q = input
            .live_q
            .as_ref()
            .map(|q| q.get().with_ref(|q| Ok(*q)))
            .transpose()?;
        input.input.get().with_core(py, |number| {
            context(py, &self.context, |context| {
                Ok(py.detach(|| {
                    let mut upper = |bytes: &mut [u8]| {
                        Python::attach(|py| {
                            link.invoke(py, |c| {
                                crate::csv::nls::upper_call(py, c, bytes).map(|_| ())
                            })
                        })
                        .map_err(|_| {
                            cs705_core::csv::Error::Operation(
                                "native tone uppercase callback failed".into(),
                            )
                        })
                    };
                    f(core::ToneRequest {
                        input: core::ToneInput {
                            field: input.field,
                            input: number,
                            point: input.point,
                            live_q: q.as_ref(),
                            inline: input.inline,
                            first: input.first,
                            upper: &mut upper,
                        },
                        context,
                    })
                }))
            })
        })
    }
}
macro_rules! detach_request {($($name:ident),*)=>{$(impl $name{fn detached<R:Send>(&self,py:Python<'_>,f:impl FnOnce(core::$name<'_>)->R+Send)->PyResult<R>{self.with_core(py,|v|Ok(py.detach(||f(v))))}})*}}
detach_request!(
    ChoiceRequest,
    NameRequest,
    SplitRequest,
    FrequencyRequest,
    OffsetRequest,
    EnumRequest,
    CodeRequest
);

fn execute<'py, T: Output>(
    py: Python<'py>,
    f: impl FnOnce() -> PyResult<Result<T, cs705_core::icf::Error>>,
) -> PyResult<Bound<'py, PyAny>> {
    let (errors, native) = CallbackError::scoped(py, f)?;
    let native = native
        .and_then(|v| v.map_err(errors::icf))
        .and_then(|v| v.output(py));
    errors.finish(py, native)
}

fn store(
    link: &CallbackLink,
    index: u32,
    event: &ChannelWrite<'_>,
) -> Result<(), cs705_core::icf::Error> {
    call(link, |py, c| {
        crate::icf::record::with_channel_write(py, event, |view| {
            crate::icf::document::adapters::require_none(c.call1((index, view))?)
        })
    })
}
macro_rules! target {
    (normal,$name:ident,$w:expr,$($args:expr),*)=>{$w.$name($($args),*)};
    (raw,$name:ident,$w:expr,$($args:expr),*)=>{core::$name(&mut $w,$($args),*)};
}
macro_rules! runners {
    ($mode:ident,$model:ty;$( $(#[$function:meta])* $name:ident:$request:ident ($($extra:ident:$et:ident),*); )*)=>{$(
        $(#[$function])* #[pyfunction]fn $name<'py>(py:Python<'py>,model:Py<$model>,holder:Py<DecodedRecord>,state:Py<EditorState>,request:Py<$request>,$($extra:Py<$et>,)*effects:Py<Effects>,before_store:Py<PyAny>)->PyResult<Bound<'py,PyAny>>{
            let request=request.try_borrow(py)?.inputs(py);
            let callback=CallbackHolder::new(py,before_store.bind(py))?;
            let link=CallbackHolder::link(callback.bind(py));
            execute(py,||{
                let mut model=model.get().take()?;let mut h=holder.get().take()?;let mut state=state.get().state.take()?;let mut effects=effects.get().state.take()?;
                $(let mut $extra=$extra.get().state.take()?;)*
                request.detached(py,|request|target!($mode,$name,model,&mut h,&mut state,request,$(&mut $extra,)*&mut effects,|i,e|store(&link,i,e)))
            })
        }
    )*}
}
runners!(normal,NormalMemory;
    native_choice_input:ChoiceRequest();native_enum_input:EnumRequest();native_offset_input:OffsetRequest();native_frequency_input:FrequencyRequest();native_code_input:CodeRequest();
    #[expect(clippy::too_many_arguments, reason = "The native tone runner borrows separate model, record, cache, request, tone effect, row effects and writer")]
    native_tone_input:ToneRequest(tone_effect:ToneEffect);
    #[expect(clippy::too_many_arguments, reason = "The native name runner preserves separate model, record, cache, request, text effect, row effects and writer")]
    native_name_input:NameRequest(text_effect:NameEffect);
);
runners!(raw,MutableBytes;
    call_choice_input_into:ChoiceRequest();call_enum_input_into:EnumRequest();call_offset_input_into:OffsetRequest();call_frequency_input_into:FrequencyRequest();call_code_input_into:CodeRequest();
    #[expect(clippy::too_many_arguments, reason = "The call tone runner borrows separate image, record, cache, request, tone effect, row effects and writer")]
    call_tone_input_into:ToneRequest(tone_effect:ToneEffect);
    #[expect(clippy::too_many_arguments, reason = "The call name runner preserves separate image, record, cache, request, text effect, row effects and writer")]
    call_name_input_into:NameRequest(text_effect:NameEffect);
    channel_callsign_text_input_into:CallsignTextRequest();
);

#[pyfunction]
fn channel_tone_input<'py>(
    py: Python<'py>,
    holder: Py<DecodedRecord>,
    input: Py<ToneInput>,
    effect: Py<ToneEffect>,
) -> PyResult<Bound<'py, PyAny>> {
    let input = input.try_borrow(py)?.inputs(py);
    execute(py, || {
        let mut h = holder.get().take()?;
        let mut effect = effect.get().state.take()?;
        input.detached(py, |mut input| {
            core::channel_tone_input(&mut h, &mut input, &mut effect)
        })
    })
}
#[pyfunction]
fn channel_code_input(
    py: Python<'_>,
    holder: &DecodedRecord,
    input: &NumberInput,
    first: Option<&Bound<'_, PyAny>>,
    node_flags: &MutableBytes,
) -> PyResult<CodeOutcome> {
    let first = first.map(strict_u8).transpose()?;
    input.with_core(py, |input| {
        let mut flags = node_flags.take()?;
        let [flags] = flags.as_mut_slice() else {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "node_flags requires exactly one byte",
            ));
        };
        holder.with_mut(|h| {
            core::channel_code_input(h, input, first, flags)
                .map(CodeOutcome::from_core)
                .map_err(errors::icf)
        })
    })
}

#[pyfunction]
fn default_filter_into(
    generation: &Bound<'_, PyAny>,
    ranges: &Ranges,
    holder: &DecodedRecord,
) -> PyResult<()> {
    let generation = strict_bytes(generation)?;
    ranges.with_ref(|ranges| {
        holder.with_mut(|h| core::default_filter_into(generation, ranges, h).map_err(errors::icf))
    })
}
#[pyfunction]
fn call_after_into(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    row: &Bound<'_, PyAny>,
    ranges: &Ranges,
    effects: &Effects,
) -> PyResult<()> {
    let (image, row) = (strict_bytes(image)?, strict_i32(row)?);
    ranges.with_ref(|ranges| {
        effects.state.edit(py, |effects| {
            core::call_after_into(image, row, ranges, effects).map_err(errors::icf)
        })
    })
}
#[pyfunction]
fn reference_into(
    py: Python<'_>,
    image: Py<MutableBytes>,
    action: &Bound<'_, PyAny>,
    group: &Bound<'_, PyAny>,
    first: &Bound<'_, PyAny>,
    last: &Bound<'_, PyAny>,
    destination: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let (action, group, first, last, destination) = (
        strict_u32(action)?,
        strict_usize(group)?,
        strict_usize(first)?,
        strict_usize(last)?,
        strict_usize(destination)?,
    );
    let mut image = image.get().take()?;
    py.detach(|| core::reference_into(&mut image, action, group, first, last, destination))
        .map_err(errors::icf)
}

macro_rules! cached {
    ($name:ident,$model:ty,$mode:ident) => {
        #[pyfunction]
        #[expect(clippy::too_many_arguments, reason = "Cached native writes require separate cache request, editor sources, row effects and pre-store producer")]
        fn $name<'py>(
            py: Python<'py>,
            model: Py<$model>,
            holder: Py<DecodedRecord>,
            state: Py<EditorState>,
            request: Py<CacheWrite>,
            editor_context: Py<EditorContext>,
            effects: Py<Effects>,
            before_store: Py<PyAny>,
        ) -> PyResult<Bound<'py, PyAny>> {
            let r = request.try_borrow(py)?.inputs(py);
            let c = editor_context.try_borrow(py)?.inputs(py);
            let callback = CallbackHolder::new(py, before_store.bind(py))?;
            let link = CallbackHolder::link(callback.bind(py));
            execute(py, || {
                let mut model = model.get().take()?;
                let mut h = holder.get().take()?;
                let mut state = state.get().state.take()?;
                let mut effects = effects.get().state.take()?;
                r.with_core(|r| {
                    c.with_core(py, |c| {
                        Ok(py.detach(|| {
                            target!(
                                $mode,
                                $name,
                                model,
                                &mut h,
                                &mut state,
                                r,
                                c,
                                &mut effects,
                                |i, e| store(&link, i, e)
                            )
                        }))
                    })
                })
            })
        }
    };
}
cached!(native_save_cached, NormalMemory, normal);
cached!(channel_save_cached_into, MutableBytes, raw);

macro_rules! split {
    ($name:ident,$model:ty,$mode:ident) => {
        #[pyfunction]
        #[expect(clippy::too_many_arguments, reason = "Split editing preserves distinct pre-store and pre-notification callbacks with live frame state")]
        fn $name<'py>(
            py: Python<'py>,
            model: Py<$model>,
            holder: Py<DecodedRecord>,
            state: Py<EditorState>,
            request: Py<SplitRequest>,
            effects: Py<Effects>,
            before_store: Py<PyAny>,
            before_notice: Py<PyAny>,
        ) -> PyResult<Bound<'py, PyAny>> {
            let r = request.try_borrow(py)?.inputs(py);
            let cb = CallbackHolder::new(py, before_store.bind(py))?;
            let link = CallbackHolder::link(cb.bind(py));
            let notice_cb = CallbackHolder::new(py, before_notice.bind(py))?;
            let notice = CallbackHolder::link(notice_cb.bind(py));
            execute(py, || {
                let mut w = model.get().take()?;
                let mut h = holder.get().take()?;
                let mut state = state.get().state.take()?;
                let mut effects = effects.get().state.take()?;
                r.detached(py, |r| {
                    target!(
                        $mode,
                        $name,
                        w,
                        &mut h,
                        &mut state,
                        r,
                        &mut effects,
                        |i, e| store(&link, i, e),
                        |n, frame| call(&notice, |py, c| Frame::with_ref(py, n, |n| {
                            Frame::with_mut(py, frame, |frame| {
                                crate::icf::document::adapters::require_none(c.call1((
                                    Notification {
                                        state: Value::Scoped(n),
                                    },
                                    SplitFrame {
                                        state: Value::Scoped(frame),
                                    },
                                ))?)
                            })
                        }))
                    )
                })
            })
        }
    };
}
split!(native_split_input, NormalMemory, normal);
split!(channel_split_input_into, MutableBytes, raw);

#[pyfunction]
fn native_pending_name(
    py: Python<'_>,
    model: Py<NormalMemory>,
    holder: Py<DecodedRecord>,
    name: &Bound<'_, PyAny>,
    pending: Py<PendingName>,
) -> PyResult<bool> {
    let name = strict_bytes(name)?;
    let model = model.get().take()?;
    let h = holder.get().take()?;
    let mut pending = pending.get().state.take()?;
    py.detach(|| model.native_pending_name(&h, name, &mut pending))
        .map_err(errors::icf)
}
#[pyfunction]
fn native_prepare_registration(
    py: Python<'_>,
    model: Py<NormalMemory>,
    holder: Py<DecodedRecord>,
    candidate: &Bound<'_, PyAny>,
    initial: (Bound<'_, PyAny>, Bound<'_, PyAny>),
    editor_context: Py<EditorContext>,
    pending: Py<PendingName>,
) -> PyResult<bool> {
    let candidate = strict_u32(candidate)?;
    let initial = (strict_u32(&initial.0)?, strict_usize(&initial.1)?);
    let c = editor_context.try_borrow(py)?.inputs(py);
    let mut w = model.get().take()?;
    let mut h = holder.get().take()?;
    let mut pending = pending.get().state.take()?;
    c.with_core(py, |c| {
        py.detach(|| w.native_prepare_registration(&mut h, candidate, initial, c, &mut pending))
            .map_err(errors::icf)
    })
}
#[pyfunction]
fn native_carry_target(
    py: Python<'_>,
    model: Py<NormalMemory>,
    selection: &Bound<'_, PyAny>,
    direction: &Bound<'_, PyAny>,
) -> PyResult<Option<(usize, usize)>> {
    let (s, d) = (core::Selection::input(selection)?, strict_i32(direction)?);
    let w = model.get().take()?;
    py.detach(|| w.native_carry_target(s, d))
        .map_err(errors::icf)
}
#[pyfunction]
fn native_copy(
    py: Python<'_>,
    model: Py<NormalMemory>,
    selection: &Bound<'_, PyAny>,
    clipboard: Py<Clipboard>,
    effects: Py<Effects>,
) -> PyResult<()> {
    let s = core::Selection::input(selection)?;
    let w = model.get().take()?;
    let mut clip = clipboard.get().state.take()?;
    let mut effects = effects.get().state.take()?;
    py.detach(|| w.native_copy(s, &mut clip, &mut effects))
        .map_err(errors::icf)
}
fn confirmation(py: Python<'_>, v: core::Confirmation) -> PyResult<Bound<'_, PyAny>> {
    v.output(py)
}
macro_rules! confirmed {($($name:ident),*)=>{$(
    #[pyfunction]fn $name<'py>(py:Python<'py>,model:Py<NormalMemory>,selection:&Bound<'_,PyAny>,generation:Py<ReferenceImage>,effects:Py<Effects>,confirm:Py<PyAny>)->PyResult<Bound<'py,PyAny>>{
        let s=core::Selection::input(selection)?;let cb=CallbackHolder::new(py,confirm.bind(py))?;let link=CallbackHolder::link(cb.bind(py));
        execute(py,||{let mut w=model.get().take()?;let mut effects=effects.get().state.take()?;generation.get().with_core(py,|g|Ok(py.detach(||w.$name(s,g,&mut effects,|kind|call(&link,|py,c|strict_i32(&c.call1((confirmation(py,kind)?,))?))))))})
    }
)*}}
confirmed!(native_clear, native_delete, native_insert);
#[pyfunction]
fn native_carry_step2<'py>(
    py: Python<'py>,
    model: Py<NormalMemory>,
    selection: &Bound<'_, PyAny>,
    direction: &Bound<'_, PyAny>,
    generation: Py<ReferenceImage>,
    effects: Py<Effects>,
) -> PyResult<Bound<'py, PyAny>> {
    let (s, d) = (core::Selection::input(selection)?, strict_i32(direction)?);
    execute(py, || {
        let mut w = model.get().take()?;
        let mut effects = effects.get().state.take()?;
        generation.get().with_core(py, |g| {
            Ok(py.detach(|| w.native_carry_step2(s, d, g, &mut effects)))
        })
    })
}
#[pyfunction]
fn native_carry<'py>(
    py: Python<'py>,
    model: Py<NormalMemory>,
    selection: &Bound<'_, PyAny>,
    direction: &Bound<'_, PyAny>,
    generation: Py<ReferenceImage>,
    effects: Py<Effects>,
    before_store: Py<PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let (s, d) = (core::Selection::input(selection)?, strict_i32(direction)?);
    let cb = CallbackHolder::new(py, before_store.bind(py))?;
    let link = CallbackHolder::link(cb.bind(py));
    execute(py, || {
        let mut w = model.get().take()?;
        let mut effects = effects.get().state.take()?;
        generation.get().with_core(py, |g| {
            Ok(py.detach(|| w.native_carry(s, d, g, &mut effects, |i, e| store(&link, i, e))))
        })
    })
}
#[pyfunction]
#[expect(
    clippy::too_many_arguments,
    reason = "Native paste preserves explicit selection, clipboard, generation, effects, confirmation and writer roles"
)]
fn native_paste<'py>(
    py: Python<'py>,
    model: Py<NormalMemory>,
    selection: &Bound<'_, PyAny>,
    clipboard: Py<Clipboard>,
    generation: Py<ReferenceImage>,
    effects: Py<Effects>,
    confirm: Py<PyAny>,
    before_store: Py<PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let s = core::Selection::input(selection)?;
    let cb = CallbackHolder::new(py, before_store.bind(py))?;
    let link = CallbackHolder::link(cb.bind(py));
    let confirm_cb = CallbackHolder::new(py, confirm.bind(py))?;
    let confirm = CallbackHolder::link(confirm_cb.bind(py));
    execute(py, || {
        let mut w = model.get().take()?;
        let mut clipboard = clipboard.get().state.take()?;
        let mut effects = effects.get().state.take()?;
        generation.get().with_core(py, |g| {
            Ok(py.detach(|| {
                w.native_paste(
                    s,
                    &mut clipboard,
                    g,
                    &mut effects,
                    |kind| {
                        call(&confirm, |py, c| {
                            strict_i32(&c.call1((confirmation(py, kind)?,))?)
                        })
                    },
                    |i, e| store(&link, i, e),
                )
            }))
        })
    })
}

#[pyfunction]
#[expect(
    clippy::too_many_arguments,
    reason = "Modal callsign editing needs separate editor state, request, effects, dialog actions and writer"
)]
fn channel_callsign_modal_input_into<'py>(
    py: Python<'py>,
    model: Py<MutableBytes>,
    holder: Py<DecodedRecord>,
    state: Py<EditorState>,
    request: Py<CallsignModalRequest>,
    effects: Py<Effects>,
    actions: Py<PyAny>,
    before_store: Py<PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let r = request.try_borrow(py)?.inputs(py);
    let modal = r.modal.try_borrow(py)?.native(py)?;
    let sources = r.sources.try_borrow(py)?.capture(py);
    let reads = r.reads.try_borrow(py)?.capture(py);
    let ec = r.context.try_borrow(py)?.inputs(py);
    let shared_ranges = sources.ranges.as_ptr() == ec.ranges.as_ptr();
    let cb = CallbackHolder::new(py, before_store.bind(py))?;
    let link = CallbackHolder::link(cb.bind(py));
    let action_cb = CallbackHolder::new(py, actions.bind(py))?;
    let actions = CallbackHolder::link(action_cb.bind(py));
    execute(py, || {
        let mut w = model.get().take()?;
        let mut h = holder.get().take()?;
        let mut state = state.get().state.take()?;
        let mut effects = effects.get().state.take()?;
        sources.with_core(py, |sources| {
            reads.with_core(py, |reads| {
                ec.generation.get().with_core(py, |generation| {
                    let invoke = |ranges: &cs705_core::icf::frequency::Ranges| {
                        Ok(py.detach(|| {
                            core::channel_callsign_modal_input_into(
                                &mut w,
                                &mut h,
                                &mut state,
                                core::CallsignModalRequest {
                                    field: r.field,
                                    modal,
                                    sources: &sources,
                                    reads: &reads,
                                    context: core::EditorContext { ranges, generation },
                                },
                                &mut effects,
                                |dialog| {
                                    call(&actions, |py, c| {
                                        Frame::with_mut(py, dialog, |loan| {
                                            crate::icf::document::adapters::require_none(c.call1(
                                                (crate::icf::callsign_dialog::Dialog::from_loan(
                                                    loan,
                                                ),),
                                            )?)
                                        })
                                    })
                                },
                                |i, e| store(&link, i, e),
                            )
                        }))
                    };
                    if shared_ranges {
                        invoke(sources.ranges)
                    } else {
                        ec.ranges.get().with_ref(invoke)
                    }
                })
            })
        })
    })
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "native")?;
    macro_rules! add{($($name:ident),*)=>{$(m.add_class::<$name>()?;)*}}
    add!(
        Selection,
        Clipboard,
        CopySource,
        Notification,
        Effects,
        Outcome,
        SplitFrame,
        SplitOutcome,
        ChoiceOutcome,
        ChoiceFrame,
        ChannelCaller,
        CallsignInline,
        NameEffect,
        NumericOutcome,
        FrequencyOutcome,
        EnumOutcome,
        ToneEffect,
        CodeOutcome,
        PendingName,
        EditorState,
        ChannelEntry,
        EditorContext,
        CacheWrite,
        ChoiceRequest,
        NameRequest,
        SplitRequest,
        CallsignTextRequest,
        FrequencyRequest,
        OffsetRequest,
        EnumRequest,
        ToneInput,
        ToneRequest,
        CodeRequest,
        CallsignModal,
        CallsignModalRequest,
        CallsignModalOutcome
    );
    register_enums(&m)?;
    macro_rules! functions{($($name:ident),*)=>{$(m.add_function(wrap_pyfunction!($name,&m)?)?;)*}}
    functions!(
        _channel_choice_key,
        _channel_choice_selector,
        _channel_choice_choices,
        _channel_enum_key,
        _channel_enum_choices,
        _tone_field_key,
        compatible,
        channel_choice_input,
        channel_enum_input,
        channel_offset_input,
        channel_entry,
        name_notification,
        callsign_inline,
        digital_code_label,
        tone_choices,
        native_choice_input,
        native_enum_input,
        native_offset_input,
        native_frequency_input,
        native_code_input,
        native_tone_input,
        native_name_input,
        call_choice_input_into,
        call_enum_input_into,
        call_offset_input_into,
        call_frequency_input_into,
        call_code_input_into,
        call_tone_input_into,
        call_name_input_into,
        channel_callsign_text_input_into,
        channel_tone_input,
        channel_code_input,
        default_filter_into,
        call_after_into,
        reference_into,
        native_save_cached,
        channel_save_cached_into,
        native_split_input,
        channel_split_input_into,
        native_pending_name,
        native_prepare_registration,
        native_carry_target,
        native_copy,
        native_clear,
        native_delete,
        native_insert,
        native_carry_step2,
        native_carry,
        native_paste,
        channel_callsign_modal_input_into
    );
    call::register(&m)?;
    parent.add_submodule(&m)
}
