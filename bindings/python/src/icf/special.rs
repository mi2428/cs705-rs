//! Typed special-sheet holders and checked live callback boundaries.
use super::values::{Input, Output};
use crate::clone::Handle;
use crate::{conversion::*, errors, owner::Owner};
use cs705_core::icf::special as core;
use pyo3::prelude::*;

enum State<T: 'static> {
    Owned(Owner<T>),
    Scoped(crate::scoped::Loan<T>),
    Projected(Handle<T>),
}

fn predicate(
    py: Python<'_>,
    callable: &Py<PyAny>,
    frequency: u32,
    failures: &mut crate::callbacks::CallbackError,
) -> Result<bool, cs705_core::icf::Error> {
    failures
        .capture(
            callable
                .bind(py)
                .call1((frequency,))
                .and_then(|value| strict_bool(&value)),
        )
        .map_err(|_| cs705_core::icf::Error::Operation("Python special predicate failed".into()))
}
fn call_classifier(
    py: Python<'_>,
    callable: &Py<PyAny>,
    frequency: u32,
    failures: &mut crate::callbacks::CallbackError,
) -> Result<u32, cs705_core::icf::Error> {
    failures
        .capture(
            callable
                .bind(py)
                .call1((frequency,))
                .and_then(|value| strict_u32(&value)),
        )
        .map_err(|_| cs705_core::icf::Error::Operation("Python special classifier failed".into()))
}
fn edge_side(value: &Bound<'_, PyAny>) -> PyResult<core::EdgeSide> {
    Ok(
        match super::enum_value(value, "cs705.icf.special", "EdgeSide")? {
            0 => core::EdgeSide::Low,
            1 => core::EdgeSide::High,
            _ => return Err(pyo3::exceptions::PyValueError::new_err("invalid EdgeSide")),
        },
    )
}
fn compact_kind(value: &Bound<'_, PyAny>) -> PyResult<core::CompactKind> {
    Ok(
        match super::enum_value(value, "cs705.icf.special", "CompactKind")? {
            0 => core::CompactKind::BandStack,
            1 => core::CompactKind::Memopad,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "invalid CompactKind",
                ));
            }
        },
    )
}
impl Input<'_> for core::CompactKind {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        compact_kind(value)
    }
}
impl Input<'_> for core::ScanField {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(
            match super::enum_value(value, "cs705.icf.special", "ScanField")? {
                0 => Self::Name,
                1 => Self::Mode,
                2 => Self::Data,
                3 => Self::Filter,
                _ => return Err(pyo3::exceptions::PyValueError::new_err("invalid ScanField")),
            },
        )
    }
}
impl Input<'_> for core::ScanRowAction {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(
            match super::enum_value(value, "cs705.icf.special", "ScanRowAction")? {
                0 => Self::Clear,
                1 => Self::Delete,
                2 => Self::Insert,
                3 => Self::Up,
                4 => Self::Down,
                _ => {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "invalid ScanRowAction",
                    ));
                }
            },
        )
    }
}
fn compact_field(value: &Bound<'_, PyAny>) -> PyResult<core::CompactField> {
    let code = super::enum_value(value, "cs705.icf.special", "CompactField")?;
    use cs705_core::icf::special::CompactField::*;
    [
        Frequency,
        Duplex,
        Offset,
        Mode,
        Data,
        Filter,
        Tone,
        TxTone,
        RxTone,
        Dtcs,
        Polarity,
        DigitalSquelch,
        DigitalCode,
        Your,
        Repeater1,
        Repeater2,
    ]
    .get(
        usize::try_from(code)
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("invalid CompactField"))?,
    )
    .copied()
    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid CompactField"))
}

#[pyclass(module = "cs705.icf.special", frozen)]
pub struct DisplayValue {
    pub inner: core::DisplayValue,
}
impl DisplayValue {
    pub fn from_core(inner: core::DisplayValue) -> Self {
        Self { inner }
    }
}
#[pymethods]
#[allow(non_snake_case)]
impl DisplayValue {
    #[classattr]
    fn Blank() -> Self {
        Self::from_core(core::DisplayValue::Blank)
    }
    #[classattr]
    fn Placeholder() -> Self {
        Self::from_core(core::DisplayValue::Placeholder)
    }
    #[staticmethod]
    fn Number(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(core::DisplayValue::Number(strict_u32(
            value,
        )?)))
    }
    #[staticmethod]
    fn Callsign(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_core(core::DisplayValue::Callsign(fixed_bytes(
            value,
        )?)))
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self.inner {
            core::DisplayValue::Blank => "Blank",
            core::DisplayValue::Placeholder => "Placeholder",
            core::DisplayValue::Number(_) => "Number",
            core::DisplayValue::Callsign(_) => "Callsign",
        }
    }
    #[getter]
    fn number(&self) -> Option<u32> {
        match self.inner {
            core::DisplayValue::Number(value) => Some(value),
            _ => None,
        }
    }
    #[getter]
    fn callsign<'py>(&self, py: Python<'py>) -> Option<Bound<'py, pyo3::types::PyBytes>> {
        match &self.inner {
            core::DisplayValue::Callsign(value) => Some(pyo3::types::PyBytes::new(py, value)),
            _ => None,
        }
    }
}

macro_rules! holder {
    ($(#[$constructor:meta])* $name:ident { $($field:ident: $ty:ty => $set:ident;)* } methods { $($methods:tt)* }) => {
        #[pyclass(module = "cs705.icf.special", frozen)]
        pub struct $name { state: State<core::$name> }
        impl $name {
            pub fn from_core(value: core::$name) -> Self { Self { state: State::Owned(Owner::new(value)) } }
            pub fn from_loan(loan: crate::scoped::Loan<core::$name>) -> Self { Self { state: State::Scoped(loan) } }
            pub fn with_ref<R>(&self, f: impl FnOnce(&core::$name) -> PyResult<R>) -> PyResult<R> { match &self.state { State::Owned(owner) => owner.with_ref(f), State::Scoped(loan) => Python::attach(|py| loan.with_ref(py, f)),State::Projected(handle)=>handle.with(|value|f(value)) } }
            pub fn with_mut<R>(&self, f: impl FnOnce(&mut core::$name) -> PyResult<R>) -> PyResult<R> { match &self.state { State::Owned(owner) => owner.with_mut(f), State::Scoped(loan) => Python::attach(|py| loan.with_mut(py, f)),State::Projected(handle)=>handle.with(f) } }
        }
        #[pymethods]
        impl $name {
            #[new]
            $(#[$constructor])*
            fn new($($field: &Bound<'_, PyAny>),*) -> PyResult<Self> { Ok(Self::from_core(core::$name { $($field: <$ty as Input>::input($field)?),* })) }
            $(#[getter] fn $field<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> { let value = self.with_ref(|h| Ok(h.$field.clone()))?; value.output(py) }
            #[setter] fn $set(&self, value: &Bound<'_, PyAny>) -> PyResult<()> { let value = <$ty as Input>::input(value)?; self.with_mut(|h| { h.$field = value; Ok(()) }) })*
            $($methods)*
        }
        impl Output for core::$name {
            fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> { Ok(Py::new(py, $name::from_core(self))?.into_bound(py).into_any()) }
        }
        impl Input<'_> for core::$name {
            fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> { value.extract::<PyRef<'_, $name>>()?.with_ref(|h| Ok(h.clone())) }
        }
    }
}
holder!(Notification {
    event: u32 => set_event;
    synchronous: bool => set_synchronous;
    first: u32 => set_first;
    second: u32 => set_second;
    payload: Vec<u8> => set_payload;
} methods {});
holder!(PresetField {
    key: u16 => set_key;
    offset: usize => set_offset;
    mask: usize => set_mask;
    field: u32 => set_field;
    choices: u32 => set_choices;
} methods {});
holder!(FrequencyInput {
    first_accepted: bool => set_first_accepted;
    second_value: u32 => set_second_value;
    second_accepted: bool => set_second_accepted;
} methods {});
holder!(#[expect(clippy::too_many_arguments, reason = "The constructor exposes all native compact-channel scalars and the three explicit callsigns")] CompactChannel {
    valid: bool => set_valid;
    frequency_hz: u32 => set_frequency_hz;
    duplex: u32 => set_duplex;
    offset_hz: u32 => set_offset_hz;
    mode: u32 => set_mode;
    data: u32 => set_data;
    filter: u32 => set_filter;
    tone: u32 => set_tone;
    tx_tone: u32 => set_tx_tone;
    rx_tone: u32 => set_rx_tone;
    dtcs: u32 => set_dtcs;
    polarity: u32 => set_polarity;
    digital_squelch: u32 => set_digital_squelch;
    digital_code: u32 => set_digital_code;
    callsigns: [[u8;8];3] => set_callsigns;
} methods {
    #[staticmethod] fn read(source:&Bound<'_,PyAny>,kind:&Bound<'_,PyAny>,row:&Bound<'_,PyAny>)->PyResult<Self>{core::CompactChannel::read(strict_bytes(source)?,compact_kind(kind)?,strict_usize(row)?).map(Self::from_core).map_err(errors::icf)}
    #[staticmethod] fn decode(raw:&Bound<'_,PyAny>)->PyResult<Self>{core::CompactChannel::decode(&fixed_bytes(raw)?).map(Self::from_core).map_err(errors::icf)}
    #[staticmethod] fn decode_valid(raw:&Bound<'_,PyAny>,valid:&Bound<'_,PyAny>)->PyResult<Self>{core::CompactChannel::decode_valid(&fixed_bytes(raw)?,strict_bool(valid)?).map(Self::from_core).map_err(errors::icf)}
    #[staticmethod] #[pyo3(signature=(source,valid))] fn decode_bound(source:Option<&Bound<'_,PyAny>>,valid:&Bound<'_,PyAny>)->PyResult<Self>{let source=source.map(fixed_bytes).transpose()?;core::CompactChannel::decode_bound(source.as_ref(),strict_bool(valid)?).map(Self::from_core).map_err(errors::icf)}
    fn display(&self,field:&Bound<'_,PyAny>)->PyResult<DisplayValue>{let field=compact_field(field)?;self.with_ref(|h|Ok(DisplayValue::from_core(h.display(field))))}
    fn input_blocked(&self,code:&Bound<'_,PyAny>,command:&Bound<'_,PyAny>)->PyResult<bool>{let code=strict_u32(code)?;let command=strict_u32(command)?;self.with_ref(|h|Ok(h.input_blocked(code,command)))}
    fn write_to(&self,py:Python<'_>,old:&Bound<'_,PyAny>,write:Py<PyAny>)->PyResult<()>{
        let old=fixed_bytes(old)?;let mut failures=crate::callbacks::CallbackError::default();
        let result=self.with_mut(|h|h.write_to(&old,|at,bytes|failures.capture(write.bind(py).call1((at,pyo3::types::PyBytes::new(py,bytes))).and_then(super::document::adapters::require_none)).map_err(|_|cs705_core::icf::Error::Operation("Python compact writer failed".into()))).map_err(errors::icf));failures.finish(py,result)
    }
});
holder!(ScanEdge {
    valid: bool => set_valid;
    low_hz: u32 => set_low_hz;
    high_hz: u32 => set_high_hz;
    mode: u32 => set_mode;
    data: u32 => set_data;
    filter: u32 => set_filter;
    name: [u8;16] => set_name;
} methods {
    #[staticmethod]
    fn read(source: &Bound<'_, PyAny>, row: &Bound<'_, PyAny>) -> PyResult<Self> { core::ScanEdge::read(strict_bytes(source)?, strict_usize(row)?).map(Self::from_core).map_err(errors::icf) }
    fn mode_candidate_allowed(&self, candidate: &Bound<'_, PyAny>) -> PyResult<bool> { let candidate = strict_u32(candidate)?; self.with_ref(|h| Ok(h.mode_candidate_allowed(candidate))) }
    fn filter_candidate_allowed(&self, candidate: &Bound<'_, PyAny>) -> PyResult<bool> { let candidate = strict_u32(candidate)?; self.with_ref(|h| Ok(h.filter_candidate_allowed(candidate))) }
    fn warnings(&self, py: Python<'_>, side: &Bound<'_, PyAny>, bad: Py<PyAny>) -> PyResult<u32> {
        let side = edge_side(side)?; let mut failures=crate::callbacks::CallbackError::default();
        let result=self.with_ref(|h| h.warnings(side,|f|predicate(py,&bad,f,&mut failures)).map_err(errors::icf));
        failures.finish(py,result)
    }
    fn suitable(&self,py:Python<'_>,allow_empty:&Bound<'_,PyAny>,allow_equal:&Bound<'_,PyAny>,check_specific:&Bound<'_,PyAny>,bad:Py<PyAny>)->PyResult<bool> {
        let allow_empty=strict_bool(allow_empty)?;let allow_equal=strict_bool(allow_equal)?;let check_specific=strict_bool(check_specific)?;let mut failures=crate::callbacks::CallbackError::default();
        let result=self.with_ref(|h|h.suitable(allow_empty,allow_equal,check_specific,|f|predicate(py,&bad,f,&mut failures)).map_err(errors::icf));failures.finish(py,result)
    }
    fn save(&self, image: &super::Icf, row: &Bound<'_, PyAny>) -> PyResult<()> { let row = strict_usize(row)?; self.with_mut(|h| image.with_mut(|image| h.save(image, row).map_err(errors::icf))) }
    fn save_with(&self, py: Python<'_>, row: &Bound<'_, PyAny>, flag: &Bound<'_, PyAny>, write: Py<PyAny>) -> PyResult<()> {
        let row = strict_usize(row)?; let flag = strict_u8(flag)?;
        let mut failures = crate::callbacks::CallbackError::default();
        let result = self.with_mut(|h| h.save_with(row, flag, |at, bytes| {
            failures.capture(write.bind(py).call1((at, pyo3::types::PyBytes::new(py, bytes))).and_then(super::document::adapters::require_none)).map_err(|_| cs705_core::icf::Error::Operation("Python scan writer failed".into()))
        }).map_err(errors::icf));
        failures.finish(py, result)
    }
});
holder!(UserEdge {
    valid: bool => set_valid;
    low_hz: u32 => set_low_hz;
    high_hz: u32 => set_high_hz;
} methods {
    #[staticmethod]
    fn read(source: &Bound<'_, PyAny>, row: &Bound<'_, PyAny>) -> PyResult<Self> { core::UserEdge::read(strict_bytes(source)?, strict_usize(row)?).map(Self::from_core).map_err(errors::icf) }
    #[staticmethod]
    fn warnings(py:Python<'_>,source:&Bound<'_,PyAny>,row:&Bound<'_,PyAny>,side:&Bound<'_,PyAny>,bad:Py<PyAny>,classify:Py<PyAny>)->PyResult<(u32,bool)> {
        let source=strict_bytes(source)?;let row=strict_usize(row)?;let side=edge_side(side)?;let failures=std::cell::RefCell::new(crate::callbacks::CallbackError::default());
        let result=core::UserEdge::warnings(source,row,side,|f|predicate(py,&bad,f,&mut failures.borrow_mut()),|f|call_classifier(py,&classify,f,&mut failures.borrow_mut())).map_err(errors::icf);
        failures.into_inner().finish(py,result)
    }
});
holder!(Preset {
    body: [u8;41] => set_body;
    masks: [u8;22] => set_masks;
} methods {
    #[staticmethod]
    fn read(source: &Bound<'_, PyAny>, p: &Bound<'_, PyAny>) -> PyResult<Self> { core::Preset::read(strict_bytes(source)?, strict_usize(p)?).map(Self::from_core).map_err(errors::icf) }
    fn bandwidth_class(&self) -> PyResult<u32> { self.with_ref(|h| Ok(h.bandwidth_class())) }
    fn combined_mode(&self) -> PyResult<u32> { self.with_ref(|h| Ok(h.combined_mode())) }
    fn bandwidth_hz(&self) -> PyResult<i32> { self.with_ref(|h| h.bandwidth_hz().map_err(errors::icf)) }
    fn checkbox_paint<'py>(&self, py: Python<'py>, field: &Bound<'_, PyAny>) -> PyResult<(Bound<'py, pyo3::types::PyBytes>, Option<bool>)> { let field = strict_u32(field)?; self.with_ref(|h| { let (text, check) = h.checkbox_paint(field).map_err(errors::icf)?; Ok((pyo3::types::PyBytes::new(py, &text), check)) }) }
    fn bandwidth_paint<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> { self.with_ref(|h| h.bandwidth_paint().map_err(errors::icf)?.output(py)) }
    fn paint_byte(&self, key: &Bound<'_, PyAny>) -> PyResult<u8> { let key = strict_u32(key)?; self.with_ref(|h| h.paint_byte(key).map_err(errors::icf)) }
    fn incompatible(&self, field: &Bound<'_, PyAny>) -> PyResult<bool> { let field = strict_u32(field)?; self.with_ref(|h| h.incompatible(field).map_err(errors::icf)) }
    fn has_enabled_fields(&self) -> PyResult<bool> { self.with_ref(|h| h.has_enabled_fields().map_err(errors::icf)) }
    fn keying_candidate_allowed(&self, which: &Bound<'_, PyAny>, candidate: &Bound<'_, PyAny>) -> PyResult<bool> { let which = strict_usize(which)?; let candidate = strict_u32(candidate)?; self.with_ref(|h| h.keying_candidate_allowed(which, candidate).map_err(errors::icf)) }
});

#[pyfunction]
fn selected_scan_raw(image: &Bound<'_, PyAny>) -> PyResult<u8> {
    core::selected_scan_raw(strict_bytes(image)?).map_err(errors::icf)
}
#[pyfunction]
fn select_scan(
    image: &super::Icf,
    g: &Bound<'_, PyAny>,
    row: &Bound<'_, PyAny>,
    reset: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let g = strict_bytes(g)?;
    let row = strict_usize(row)?;
    let reset = strict_bool(reset)?;
    image.with_mut(|image| core::select_scan(image, g, row, reset).map_err(errors::icf))
}
#[pyfunction]
fn scan_reset_enabled(source: &Bound<'_, PyAny>, index: &Bound<'_, PyAny>) -> PyResult<bool> {
    core::scan_reset_enabled(strict_bytes(source)?, strict_u32(index)?).map_err(errors::icf)
}
#[pyfunction]
fn decode_bandwidth(class_: &Bound<'_, PyAny>, raw: &Bound<'_, PyAny>) -> PyResult<i32> {
    core::decode_bandwidth(strict_u32(class_)?, strict_u8(raw)?).map_err(errors::icf)
}
#[pyfunction]
fn encode_bandwidth(class_: &Bound<'_, PyAny>, width: &Bound<'_, PyAny>) -> PyResult<u8> {
    core::encode_bandwidth(strict_u32(class_)?, strict_i32(width)?).map_err(errors::icf)
}
#[pyfunction]
fn scan_suitable_rows(
    py: Python<'_>,
    source: &Bound<'_, PyAny>,
    live_current: &Bound<'_, PyAny>,
    allow_empty: &Bound<'_, PyAny>,
    allow_equal: &Bound<'_, PyAny>,
    check_specific: &Bound<'_, PyAny>,
    bad: Py<PyAny>,
) -> PyResult<Vec<usize>> {
    let mut failures = crate::callbacks::CallbackError::default();
    let result = core::scan_suitable_rows(
        strict_bytes(source)?,
        strict_bytes(live_current)?,
        strict_bool(allow_empty)?,
        strict_bool(allow_equal)?,
        strict_bool(check_specific)?,
        |f| predicate(py, &bad, f, &mut failures),
    )
    .map_err(errors::icf);
    failures.finish(py, result)
}
#[pyfunction]
fn _compact_kind_offset(kind: &Bound<'_, PyAny>, row: &Bound<'_, PyAny>) -> PyResult<usize> {
    compact_kind(kind)?
        .offset(strict_usize(row)?)
        .map_err(errors::icf)
}
#[pyfunction]
fn _compact_kind_displayed_rows<'py>(
    py: Python<'py>,
    kind: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let range = compact_kind(kind)?.displayed_rows();
    py.import("builtins")?
        .getattr("range")?
        .call1((range.start, range.end))
}
#[pyfunction]
fn _compact_field_key(field: &Bound<'_, PyAny>) -> PyResult<u16> {
    Ok(compact_field(field)?.key())
}
impl Notification {
    pub(crate) fn from_projection(handle: Handle<core::Notification>) -> Self {
        Self {
            state: State::Projected(handle),
        }
    }
}
fn notification_values(value: &Bound<'_, PyAny>) -> PyResult<Vec<core::Notification>> {
    value
        .try_iter()?
        .map(|value| {
            value?
                .extract::<PyRef<'_, Notification>>()?
                .with_ref(|value| Ok(value.clone()))
        })
        .collect()
}
fn notification_index(index: isize, count: usize) -> PyResult<usize> {
    let index = if index < 0 {
        count.checked_add_signed(index)
    } else {
        Some(index as usize)
    };
    index
        .filter(|index| *index < count)
        .ok_or_else(|| pyo3::exceptions::PyIndexError::new_err("notification index out of range"))
}
#[pyclass(module = "cs705.icf.special", name = "_Notifications", frozen)]
pub struct Notifications {
    root: Handle<core::Effects>,
}
#[pymethods]
impl Notifications {
    fn __len__(&self) -> PyResult<usize> {
        self.root.with(|effects| Ok(effects.notifications.len()))
    }
    fn __getitem__(&self, index: &Bound<'_, PyAny>) -> PyResult<Notification> {
        let index = strict_isize(index)?;
        let handle = self.root.with(|effects| {
            let index = notification_index(index, effects.notifications.len())?;
            Ok(self.root.project(move |effects| {
                effects.notifications.get_mut(index).ok_or_else(|| {
                    pyo3::exceptions::PyReferenceError::new_err("notification was removed")
                })
            }))
        })?;
        Ok(Notification::from_projection(handle))
    }
    fn __setitem__(&self, index: &Bound<'_, PyAny>, value: &Notification) -> PyResult<()> {
        let index = strict_isize(index)?;
        let value = value.with_ref(|value| Ok(value.clone()))?;
        self.root.edit(|effects| {
            let index = notification_index(index, effects.notifications.len())?;
            effects.notifications[index] = value;
            Ok(())
        })
    }
    fn __delitem__(&self, index: &Bound<'_, PyAny>) -> PyResult<()> {
        let index = strict_isize(index)?;
        self.root.edit(|effects| {
            let index = notification_index(index, effects.notifications.len())?;
            effects.notifications.remove(index);
            Ok(())
        })
    }
    fn append(&self, value: &Notification) -> PyResult<()> {
        let value = value.with_ref(|value| Ok(value.clone()))?;
        self.root.edit(|effects| {
            effects.notifications.push(value);
            Ok(())
        })
    }
    fn clear(&self) -> PyResult<()> {
        self.root.edit(|effects| {
            effects.notifications.clear();
            Ok(())
        })
    }
}
#[pyclass(module = "cs705.icf.special", frozen)]
pub struct Effects {
    root: Handle<core::Effects>,
}
impl Effects {
    pub fn from_core(value: core::Effects) -> Self {
        Self {
            root: Handle::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::Effects) -> PyResult<R>) -> PyResult<R> {
        self.root.with(|value| f(value))
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut core::Effects) -> PyResult<R>) -> PyResult<R> {
        self.root.edit(f)
    }
}
macro_rules! effect_fields {
    ($($field:ident:$ty:ty=>$set:ident;)*)=>{#[pymethods]impl Effects {
        #[new]
        #[expect(clippy::too_many_arguments, reason = "The constructor preserves every native special-sheet effect field and its notification list")]
        fn new($($field:&Bound<'_,PyAny>,)*notifications:&Bound<'_,PyAny>)->PyResult<Self>{let notifications=notification_values(notifications)?;Ok(Self::from_core(core::Effects{$($field:<$ty as Input>::input($field)?,)*notifications}))}
        #[staticmethod]fn default()->Self{Self::from_core(core::Effects::default())}
        $(#[getter]fn $field<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{let value=self.with_ref(|value|Ok(value.$field))?;value.output(py)}
        #[setter]fn $set(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let value=<$ty as Input>::input(value)?;self.with_mut(|effects|{effects.$field=value;Ok(())})})*
        #[getter]fn notifications(&self)->PyResult<Notifications>{self.with_ref(|_|Ok(()))?;Ok(Notifications{root:self.root.clone()})}
        #[setter]fn set_notifications(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let value=notification_values(value)?;self.with_mut(|effects|{effects.notifications=value;Ok(())})}
        fn dispatch(&self,py:Python<'_>,send:Py<PyAny>)->PyResult<()>{
            let mut failures=crate::callbacks::CallbackError::default();
            let result=self.with_ref(|effects|effects.dispatch(|event|{
                let result=crate::scoped::Frame::with_ref(py,event,|loan|{let event=Py::new(py,Notification::from_loan(loan))?;super::document::adapters::require_none(send.bind(py).call1((event,))?)?;Ok(())});
                failures.capture(result).map_err(|_|cs705_core::icf::Error::Operation("Python notification receiver failed".into()))
            }).map_err(errors::icf));
            failures.finish(py,result)
        }
    }};
}
effect_fields! {writes:usize=>set_writes;callback_flags:u32=>set_callback_flags;phase:Option<u32> => set_phase;warning_bits:u32=>set_warning_bits;gray:bool=>set_gray;blocked:bool=>set_blocked;node_style_clear:u32=>set_node_style_clear;}

#[pyclass(module = "cs705.icf.special")]
pub struct RowState {
    pub inner: core::RowState,
}
impl RowState {
    pub fn from_core(inner: core::RowState) -> Self {
        Self { inner }
    }
}
macro_rules! row_fields {
    ($($field:ident:$ty:ty=>$set:ident,$strict:ident;)*)=>{#[pymethods]impl RowState {
        #[new]fn new($($field:&Bound<'_,PyAny>),*)->PyResult<Self>{Ok(Self::from_core(core::RowState{$($field:$strict($field)?),*}))}
        #[staticmethod]fn default()->Self{Self::from_core(core::RowState::default())}
        $(#[getter]fn $field(&self)->$ty{self.inner.$field}
        #[setter]fn $set(&mut self,value:&Bound<'_,PyAny>)->PyResult<()>{self.inner.$field=$strict(value)?;Ok(())})*
    }};
}
row_fields! {bits:u32=>set_bits,strict_u32;start:usize=>set_start,strict_usize;end:usize=>set_end,strict_usize;active:usize=>set_active,strict_usize;state:u32=>set_state,strict_u32;}

macro_rules! image_effects {
    ($($name:ident($($arg:ident:$ty:ty),*);)*)=>{$(
        #[pyfunction]fn $name(image:&super::Icf,$($arg:&Bound<'_,PyAny>),*)->PyResult<Effects>{
            $(let $arg=<$ty as Input>::input($arg)?;)*
            image.with_mut(|image|core::$name(image,$($arg),*).map(Effects::from_core).map_err(errors::icf))
        }
    )*};
}
image_effects! {
    reset_compact(g:&[u8],kind:core::CompactKind,confirmation:i32,only:bool);
    reset_preset(g:&[u8],p:usize,confirmation:i32);
    toggle_preset(g:&[u8],p:usize,field:u32,grid_present:bool,code:u32,live_preset:usize);
    commit_preset(g:&[u8],p:usize,key:u16,side:usize,accepted:Option<i32>,live_preset:usize);
    edit_scan_field(row:usize,field:core::ScanField,accepted:Option<u32>,name:Option<&[u8]>);
}
#[pyfunction]
fn select_scan_raw(
    image: &super::Icf,
    g: &Bound<'_, PyAny>,
    candidate: &Bound<'_, PyAny>,
    reset: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let g = strict_bytes(g)?;
    let candidate = strict_i32(candidate)?;
    let reset = strict_bool(reset)?;
    image.with_mut(|image| core::select_scan_raw(image, g, candidate, reset).map_err(errors::icf))
}
#[pyfunction]
fn scan_rows(
    image: &super::Icf,
    g: &Bound<'_, PyAny>,
    range: &Bound<'_, PyAny>,
    action: &Bound<'_, PyAny>,
    confirmed: &Bound<'_, PyAny>,
    mut state: PyRefMut<'_, RowState>,
    reset: &Bound<'_, PyAny>,
) -> PyResult<Effects> {
    let g = strict_bytes(g)?;
    let range = strict_range(range)?;
    let action = core::ScanRowAction::input(action)?;
    let confirmed = strict_bool(confirmed)?;
    let reset = strict_bool(reset)?;
    image.with_mut(|image| {
        core::scan_rows(image, g, range, action, confirmed, &mut state.inner, reset)
            .map(Effects::from_core)
            .map_err(errors::icf)
    })
}
#[pyfunction]
#[expect(
    clippy::too_many_arguments,
    reason = "Scan frequency editing keeps generation, edge side, parser result, reset gate and predicate explicit"
)]
fn edit_scan_frequency(
    py: Python<'_>,
    image: &super::Icf,
    g: &Bound<'_, PyAny>,
    row: &Bound<'_, PyAny>,
    side: &Bound<'_, PyAny>,
    input: Py<FrequencyInput>,
    reset: &Bound<'_, PyAny>,
    bad: Py<PyAny>,
) -> PyResult<Effects> {
    let g = strict_bytes(g)?;
    let row = strict_usize(row)?;
    let side = edge_side(side)?;
    let input = input.get().with_ref(|value| Ok(*value))?;
    let reset = strict_bool(reset)?;
    let mut failures = crate::callbacks::CallbackError::default();
    let result = image.with_mut(|image| {
        core::edit_scan_frequency(image, g, row, side, input, reset, |f| {
            predicate(py, &bad, f, &mut failures)
        })
        .map(Effects::from_core)
        .map_err(errors::icf)
    });
    failures.finish(py, result)
}
#[pyfunction]
fn edit_user_frequency(
    py: Python<'_>,
    image: &super::Icf,
    row: &Bound<'_, PyAny>,
    side: &Bound<'_, PyAny>,
    input: Py<FrequencyInput>,
    bad: Py<PyAny>,
    classify: Py<PyAny>,
) -> PyResult<Effects> {
    let row = strict_usize(row)?;
    let side = edge_side(side)?;
    let input = input.get().with_ref(|value| Ok(*value))?;
    let failures = std::cell::RefCell::new(crate::callbacks::CallbackError::default());
    let result = image.with_mut(|image| {
        core::edit_user_frequency(
            image,
            row,
            side,
            input,
            |f| predicate(py, &bad, f, &mut failures.borrow_mut()),
            |f| call_classifier(py, &classify, f, &mut failures.borrow_mut()),
        )
        .map(Effects::from_core)
        .map_err(errors::icf)
    });
    failures.into_inner().finish(py, result)
}
#[pyfunction]
#[pyo3(signature=(image,g,q,live_preset,name,label,notify))]
#[expect(
    clippy::too_many_arguments,
    reason = "Preset rename distinguishes generation, selected and live presets, text, label producer and notifier"
)]
fn rename_preset(
    py: Python<'_>,
    image: &super::Icf,
    g: &Bound<'_, PyAny>,
    q: &Bound<'_, PyAny>,
    live_preset: &Bound<'_, PyAny>,
    name: Option<&Bound<'_, PyAny>>,
    label: Py<PyAny>,
    notify: Py<PyAny>,
) -> PyResult<Effects> {
    let g = strict_bytes(g)?;
    let q = strict_usize(q)?;
    let live_preset = strict_usize(live_preset)?;
    let name = name.map(strict_bytes).transpose()?;
    let failures = std::cell::RefCell::new(crate::callbacks::CallbackError::default());
    let result = image.with_mut(|image| {
        core::rename_preset(
            image,
            g,
            q,
            live_preset,
            name,
            |name| {
                failures
                    .borrow_mut()
                    .capture(
                        label
                            .bind(py)
                            .call1((pyo3::types::PyBytes::new(py, name),))
                            .and_then(|value| Ok(strict_bytes(&value)?.to_vec())),
                    )
                    .map_err(|_| {
                        cs705_core::icf::Error::Operation("Python preset label failed".into())
                    })
            },
            |event| {
                let result = crate::scoped::Frame::with_ref(py, event, |loan| {
                    let event = Py::new(py, Notification::from_loan(loan))?;
                    super::document::adapters::require_none(notify.bind(py).call1((event,))?)
                });
                failures.borrow_mut().capture(result).map_err(|_| {
                    cs705_core::icf::Error::Operation("Python preset notification failed".into())
                })
            },
        )
        .map(Effects::from_core)
        .map_err(errors::icf)
    });
    failures.into_inner().finish(py, result)
}

#[pyclass(module = "cs705.icf.special")]
pub struct PresetNumberEffect {
    #[pyo3(get, set)]
    pub editor: Py<super::settings::extended::NumberEffect>,
    #[pyo3(get, set)]
    pub effects: Py<Effects>,
}
impl PresetNumberEffect {
    pub fn from_core(py: Python<'_>, value: core::PresetNumberEffect) -> PyResult<Self> {
        Ok(Self {
            editor: Py::new(
                py,
                super::settings::extended::NumberEffect::from_core(value.editor),
            )?,
            effects: Py::new(py, Effects::from_core(value.effects))?,
        })
    }
}
#[pymethods]
impl PresetNumberEffect {
    #[new]
    fn new(editor: Py<super::settings::extended::NumberEffect>, effects: Py<Effects>) -> Self {
        Self { editor, effects }
    }
}

#[pyclass(module = "cs705.icf.special")]
pub struct PresetBandwidthEffect {
    #[pyo3(get, set)]
    pub editor: Option<Py<super::settings::extended::BandwidthEffect>>,
    #[pyo3(get, set)]
    pub effects: Py<Effects>,
}
impl PresetBandwidthEffect {
    pub fn from_core(py: Python<'_>, value: core::PresetBandwidthEffect) -> PyResult<Self> {
        Ok(Self {
            editor: value
                .editor
                .map(|value| {
                    Py::new(
                        py,
                        super::settings::extended::BandwidthEffect::from_core(py, value)?,
                    )
                })
                .transpose()?,
            effects: Py::new(py, Effects::from_core(value.effects))?,
        })
    }
}
#[pymethods]
impl PresetBandwidthEffect {
    #[new]
    #[pyo3(signature = (editor, effects))]
    fn new(
        editor: Option<Py<super::settings::extended::BandwidthEffect>>,
        effects: Py<Effects>,
    ) -> Self {
        Self { editor, effects }
    }
}

#[pyfunction]
fn bandwidth_input(
    py: Python<'_>,
    class_: &Bound<'_, PyAny>,
    old_raw: &Bound<'_, PyAny>,
    input: &super::settings::extended::NumberInput,
    crt_point: &Bound<'_, PyAny>,
) -> PyResult<super::settings::extended::BandwidthEffect> {
    let class = strict_u32(class_)?;
    let old = strict_u8(old_raw)?;
    let point = strict_u8(crt_point)?;
    let value = input.with_core(py, |input| {
        core::bandwidth_input(class, old, input, point).map_err(errors::icf)
    })?;
    super::settings::extended::BandwidthEffect::from_core(py, value)
}

#[pyfunction]
fn edit_preset_bandwidth(
    py: Python<'_>,
    image: &super::Icf,
    g: &Bound<'_, PyAny>,
    p: &Bound<'_, PyAny>,
    input: &super::settings::extended::NumberInput,
    crt_point: &Bound<'_, PyAny>,
    live_preset: &Bound<'_, PyAny>,
) -> PyResult<PresetBandwidthEffect> {
    let g = strict_bytes(g)?;
    let p = strict_usize(p)?;
    let point = strict_u8(crt_point)?;
    let live = strict_usize(live_preset)?;
    let value = input.with_core(py, |input| {
        image.with_mut(|image| {
            core::edit_preset_bandwidth(image, g, p, input, point, live).map_err(errors::icf)
        })
    })?;
    PresetBandwidthEffect::from_core(py, value)
}

#[pyfunction]
#[expect(
    clippy::too_many_arguments,
    reason = "Preset number editing requires generation, field key, numeric source, decimal point and live preset separately"
)]
fn edit_preset_number(
    py: Python<'_>,
    image: &super::Icf,
    g: &Bound<'_, PyAny>,
    p: &Bound<'_, PyAny>,
    key: &Bound<'_, PyAny>,
    input: &super::settings::extended::NumberInput,
    crt_point: &Bound<'_, PyAny>,
    live_preset: &Bound<'_, PyAny>,
) -> PyResult<PresetNumberEffect> {
    let g = strict_bytes(g)?;
    let p = strict_usize(p)?;
    let key = strict_u16(key)?;
    let point = strict_u8(crt_point)?;
    let live = strict_usize(live_preset)?;
    let value = input.with_core(py, |input| {
        image.with_mut(|image| {
            core::edit_preset_number(image, g, p, key, input, point, live).map_err(errors::icf)
        })
    })?;
    PresetNumberEffect::from_core(py, value)
}

#[pyclass(module = "cs705.icf.special")]
#[derive(Clone, Copy)]
pub struct PasteApproval {
    inner: core::PasteApproval,
}
#[pymethods]
impl PasteApproval {
    #[new]
    fn new(tail: &Bound<'_, PyAny>, overwrite: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::PasteApproval {
                tail: strict_bool(tail)?,
                overwrite: strict_bool(overwrite)?,
            },
        })
    }
    #[getter]
    fn tail(&self) -> bool {
        self.inner.tail
    }
    #[setter]
    fn set_tail(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.tail = strict_bool(value)?;
        Ok(())
    }
    #[getter]
    fn overwrite(&self) -> bool {
        self.inner.overwrite
    }
    #[setter]
    fn set_overwrite(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.overwrite = strict_bool(value)?;
        Ok(())
    }
}

#[pyclass(module = "cs705.icf.special", frozen)]
pub struct CompactClipboard {
    owner: Owner<core::CompactClipboard>,
}
#[pymethods]
impl CompactClipboard {
    #[new]
    fn new(
        reserve_count: &Bound<'_, PyAny>,
        source_rows: &Bound<'_, PyAny>,
        channels: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let reserve_count = strict_i32(reserve_count)?;
        let source_rows = source_rows
            .try_iter()?
            .map(|v| strict_usize(&v?))
            .collect::<PyResult<Vec<_>>>()?;
        let channels = channels
            .try_iter()?
            .map(|v| core::CompactChannel::input(&v?))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            owner: Owner::new(core::CompactClipboard {
                reserve_count,
                source_rows,
                channels,
            }),
        })
    }
    #[staticmethod]
    fn default() -> Self {
        Self {
            owner: Owner::new(core::CompactClipboard::default()),
        }
    }
    #[getter]
    fn reserve_count(&self) -> PyResult<i32> {
        self.owner.with_ref(|c| Ok(c.reserve_count))
    }
    #[setter]
    fn set_reserve_count(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_i32(value)?;
        self.owner.with_mut(|c| {
            c.reserve_count = value;
            Ok(())
        })
    }
    fn source_rows_snapshot(&self) -> PyResult<Vec<usize>> {
        self.owner.with_ref(|c| Ok(c.source_rows.clone()))
    }
    fn set_source_rows(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = value
            .try_iter()?
            .map(|v| strict_usize(&v?))
            .collect::<PyResult<Vec<_>>>()?;
        self.owner.with_mut(|c| {
            c.source_rows = value;
            Ok(())
        })
    }
    fn channels_snapshot(&self) -> PyResult<Vec<CompactChannel>> {
        let values = self.owner.with_ref(|c| Ok(c.channels.clone()))?;
        Ok(values.into_iter().map(CompactChannel::from_core).collect())
    }
    fn set_channels(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = value
            .try_iter()?
            .map(|v| core::CompactChannel::input(&v?))
            .collect::<PyResult<Vec<_>>>()?;
        self.owner.with_mut(|c| {
            c.channels = value;
            Ok(())
        })
    }
    fn copy(
        &self,
        source: &Bound<'_, PyAny>,
        kind: &Bound<'_, PyAny>,
        range: &Bound<'_, PyAny>,
        only: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let (source, kind, range, only) = (
            strict_bytes(source)?,
            compact_kind(kind)?,
            strict_range(range)?,
            strict_bool(only)?,
        );
        self.owner
            .with_mut(|c| c.copy(source, kind, range, only).map_err(errors::icf))
    }
}

#[pyclass(module = "cs705.icf.special", frozen)]
pub struct ScanClipboard {
    owner: Owner<core::ScanClipboard>,
}
#[pymethods]
impl ScanClipboard {
    #[new]
    fn new(rows: &Bound<'_, PyAny>, transfer_count: &Bound<'_, PyAny>) -> PyResult<Self> {
        let rows = rows
            .try_iter()?
            .map(|v| core::ScanEdge::input(&v?))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            owner: Owner::new(core::ScanClipboard {
                rows,
                transfer_count: strict_usize(transfer_count)?,
            }),
        })
    }
    #[staticmethod]
    fn default() -> Self {
        Self {
            owner: Owner::new(core::ScanClipboard::default()),
        }
    }
    #[getter]
    fn transfer_count(&self) -> PyResult<usize> {
        self.owner.with_ref(|c| Ok(c.transfer_count))
    }
    #[setter]
    fn set_transfer_count(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_usize(value)?;
        self.owner.with_mut(|c| {
            c.transfer_count = value;
            Ok(())
        })
    }
    fn rows_snapshot(&self) -> PyResult<Vec<ScanEdge>> {
        let values = self.owner.with_ref(|c| Ok(c.rows.clone()))?;
        Ok(values.into_iter().map(ScanEdge::from_core).collect())
    }
    fn set_rows(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = value
            .try_iter()?
            .map(|v| core::ScanEdge::input(&v?))
            .collect::<PyResult<Vec<_>>>()?;
        self.owner.with_mut(|c| {
            c.rows = value;
            Ok(())
        })
    }
    fn copy(
        &self,
        source: &Bound<'_, PyAny>,
        range: &Bound<'_, PyAny>,
        state: &Bound<'_, RowState>,
    ) -> PyResult<()> {
        let (source, range) = (strict_bytes(source)?, strict_range(range)?);
        let result = {
            let mut state = state.try_borrow_mut()?;
            self.owner
                .with_mut(|c| Ok(c.copy(source, range, &mut state.inner)))?
        };
        result.map_err(errors::icf)
    }
    fn paste(
        &self,
        image: &super::Icf,
        g: &Bound<'_, PyAny>,
        start: &Bound<'_, PyAny>,
        approval: &PasteApproval,
        state: &Bound<'_, RowState>,
        reset: &Bound<'_, PyAny>,
    ) -> PyResult<Effects> {
        let (g, start, approval, reset) = (
            strict_bytes(g)?,
            strict_usize(start)?,
            approval.inner,
            strict_bool(reset)?,
        );
        let result = {
            let mut state = state.try_borrow_mut()?;
            self.owner.with_mut(|c| {
                image.with_mut(|image| {
                    Ok(c.paste(image, g, start, approval, &mut state.inner, reset))
                })
            })?
        };
        result.map(Effects::from_core).map_err(errors::icf)
    }
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "special")?;
    m.add_class::<FrequencyInput>()?;
    m.add_class::<Notification>()?;
    m.add_class::<Effects>()?;
    m.add_class::<Notifications>()?;
    m.add("Notifications", m.getattr("_Notifications")?)?;
    m.add_class::<RowState>()?;
    m.add_class::<CompactClipboard>()?;
    m.add_class::<ScanClipboard>()?;
    m.add_class::<PasteApproval>()?;
    m.add_class::<PresetNumberEffect>()?;
    m.add_class::<PresetBandwidthEffect>()?;
    super::add_enum(
        &m,
        "ScanField",
        &[("Name", 0), ("Mode", 1), ("Data", 2), ("Filter", 3)],
    )?;
    super::add_enum(
        &m,
        "ScanRowAction",
        &[
            ("Clear", 0),
            ("Delete", 1),
            ("Insert", 2),
            ("Up", 3),
            ("Down", 4),
        ],
    )?;
    m.add_class::<PresetField>()?;
    let catalog = core::PRESET_CATALOG
        .iter()
        .map(|value| Py::new(m.py(), PresetField::from_core(*value)))
        .collect::<PyResult<Vec<_>>>()?;
    m.add(
        "PRESET_CATALOG",
        pyo3::types::PyTuple::new(m.py(), catalog)?,
    )?;
    m.add_class::<ScanEdge>()?;
    m.add_class::<UserEdge>()?;
    m.add_class::<Preset>()?;
    m.add_class::<CompactChannel>()?;
    m.add_class::<DisplayValue>()?;
    super::add_enum(&m, "CompactKind", &[("BandStack", 0), ("Memopad", 1)])?;
    super::add_enum(
        &m,
        "CompactField",
        &[
            ("Frequency", 0),
            ("Duplex", 1),
            ("Offset", 2),
            ("Mode", 3),
            ("Data", 4),
            ("Filter", 5),
            ("Tone", 6),
            ("TxTone", 7),
            ("RxTone", 8),
            ("Dtcs", 9),
            ("Polarity", 10),
            ("DigitalSquelch", 11),
            ("DigitalCode", 12),
            ("Your", 13),
            ("Repeater1", 14),
            ("Repeater2", 15),
        ],
    )?;
    super::add_enum(&m, "EdgeSide", &[("Low", 0), ("High", 1)])?;
    m.add_function(wrap_pyfunction!(selected_scan_raw, &m)?)?;
    m.add_function(wrap_pyfunction!(select_scan, &m)?)?;
    m.add_function(wrap_pyfunction!(scan_reset_enabled, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_bandwidth, &m)?)?;
    m.add_function(wrap_pyfunction!(encode_bandwidth, &m)?)?;
    m.add_function(wrap_pyfunction!(scan_suitable_rows, &m)?)?;
    m.add_function(wrap_pyfunction!(_compact_kind_offset, &m)?)?;
    m.add_function(wrap_pyfunction!(_compact_kind_displayed_rows, &m)?)?;
    m.add_function(wrap_pyfunction!(_compact_field_key, &m)?)?;
    m.add_function(wrap_pyfunction!(reset_compact, &m)?)?;
    m.add_function(wrap_pyfunction!(reset_preset, &m)?)?;
    m.add_function(wrap_pyfunction!(toggle_preset, &m)?)?;
    m.add_function(wrap_pyfunction!(commit_preset, &m)?)?;
    m.add_function(wrap_pyfunction!(edit_scan_field, &m)?)?;
    m.add_function(wrap_pyfunction!(select_scan_raw, &m)?)?;
    m.add_function(wrap_pyfunction!(scan_rows, &m)?)?;
    m.add_function(wrap_pyfunction!(edit_scan_frequency, &m)?)?;
    m.add_function(wrap_pyfunction!(edit_user_frequency, &m)?)?;
    m.add_function(wrap_pyfunction!(rename_preset, &m)?)?;
    m.add_function(wrap_pyfunction!(bandwidth_input, &m)?)?;
    m.add_function(wrap_pyfunction!(edit_preset_bandwidth, &m)?)?;
    m.add_function(wrap_pyfunction!(edit_preset_number, &m)?)?;
    parent.add_submodule(&m)
}
