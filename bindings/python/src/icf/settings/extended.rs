//! Shared typed editor inputs and observable effects for domain editors.
use super::super::values::{Input, Output};
use super::Selection;
use crate::{conversion::*, owner::Owner};
use cs705_core::icf::settings::extended as core;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
macro_rules! pair {
    ($($ty:ty => $strict:ident),*) => {$(
        impl Input<'_> for [$ty;2] {fn input(v:&Bound<'_,PyAny>)->PyResult<Self>{let(a,b):(Bound<'_,PyAny>,Bound<'_,PyAny>)=v.extract()?;Ok([$strict(&a)?,$strict(&b)?])}}
        impl Output for [$ty;2] {fn output<'py>(self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{Ok(pyo3::types::PyTuple::new(py,self)?.into_any())}}
    )*}
}
pair!(i32=>strict_i32,u32=>strict_u32,bool=>strict_bool);

#[pyclass(module = "cs705.icf.settings.extended", frozen)]
pub struct ChoiceInput {
    inner: core::ChoiceInput,
    selection: Option<Py<Selection>>,
}
impl ChoiceInput {
    pub fn from_core(py: Python<'_>, inner: core::ChoiceInput) -> PyResult<Self> {
        let selection = match inner {
            core::ChoiceInput::Selected(value) => Some(Py::new(py, Selection::from_core(value))?),
            _ => None,
        };
        Ok(Self { inner, selection })
    }
    pub fn to_core(&self, py: Python<'_>) -> PyResult<core::ChoiceInput> {
        Ok(match &self.selection {
            Some(value) => core::ChoiceInput::Selected(value.try_borrow(py)?.inner),
            None => self.inner,
        })
    }
    pub fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::ChoiceInput) -> PyResult<R>,
    ) -> PyResult<R> {
        f(self.to_core(py)?)
    }
}
macro_rules! choices {
    ($($name:ident),*) => { #[pymethods] #[allow(non_snake_case)] impl ChoiceInput {
        $(#[classattr] fn $name() -> Self { Self { inner: core::ChoiceInput::$name, selection: None } })*
        #[staticmethod] fn Selected(selection: Py<Selection>) -> Self { Self { inner: core::ChoiceInput::Display, selection: Some(selection) } }
        #[getter] fn selection(&self, py: Python<'_>) -> Option<Py<Selection>> { self.selection.as_ref().map(|value| value.clone_ref(py)) }
        #[getter] fn kind(&self) -> &'static str { if self.selection.is_some() { "Selected" } else { match self.inner { $(core::ChoiceInput::$name => stringify!($name),)* core::ChoiceInput::Selected(_) => unreachable!() } } }
    } };
}
choices!(Display, Cancel, Next, Previous, Begin);

enum NumberValue {
    Display,
    Cancel,
    Next,
    Previous,
    Begin,
    Text(Py<PyBytes>),
    Selected(Py<Selection>),
}
#[pyclass(module = "cs705.icf.settings.extended", frozen)]
pub struct NumberInput {
    value: NumberValue,
}
impl NumberInput {
    pub fn from_core(py: Python<'_>, value: core::NumberInput<'_>) -> PyResult<Self> {
        Ok(Self {
            value: match value {
                core::NumberInput::Display => NumberValue::Display,
                core::NumberInput::Cancel => NumberValue::Cancel,
                core::NumberInput::Next => NumberValue::Next,
                core::NumberInput::Previous => NumberValue::Previous,
                core::NumberInput::Begin => NumberValue::Begin,
                core::NumberInput::Text(value) => {
                    NumberValue::Text(PyBytes::new(py, value).unbind())
                }
                core::NumberInput::Selected(value) => {
                    NumberValue::Selected(Py::new(py, Selection::from_core(value))?)
                }
            },
        })
    }
    pub fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::NumberInput<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let value = match &self.value {
            NumberValue::Display => core::NumberInput::Display,
            NumberValue::Cancel => core::NumberInput::Cancel,
            NumberValue::Next => core::NumberInput::Next,
            NumberValue::Previous => core::NumberInput::Previous,
            NumberValue::Begin => core::NumberInput::Begin,
            NumberValue::Text(value) => core::NumberInput::Text(value.bind(py).as_bytes()),
            NumberValue::Selected(value) => {
                core::NumberInput::Selected(value.try_borrow(py)?.inner)
            }
        };
        f(value)
    }
}
macro_rules! numbers {
    ($($name:ident),*) => { #[pymethods] #[allow(non_snake_case)] impl NumberInput {
        $(#[classattr] fn $name() -> Self { Self { value: NumberValue::$name } })*
        #[staticmethod] fn Text(value: &Bound<'_, PyAny>) -> PyResult<Self> { Ok(Self { value: NumberValue::Text(value.cast::<PyBytes>()?.clone().unbind()) }) }
        #[staticmethod] fn Selected(value: Py<Selection>) -> Self { Self { value: NumberValue::Selected(value) } }
        #[getter] fn source(&self, py: Python<'_>) -> Option<Py<PyBytes>> { match &self.value { NumberValue::Text(value) => Some(value.clone_ref(py)), _ => None } }
        #[getter] fn selection(&self, py: Python<'_>) -> Option<Py<Selection>> { match &self.value { NumberValue::Selected(value) => Some(value.clone_ref(py)), _ => None } }
        #[getter] fn kind(&self) -> &'static str { match self.value { $(NumberValue::$name => stringify!($name),)* NumberValue::Text(_) => "Text", NumberValue::Selected(_) => "Selected" } }
    } };
}
numbers!(Display, Cancel, Next, Previous, Begin);

enum TextValue {
    Display,
    Cancel,
    Begin(Option<u8>),
    Text(Py<PyBytes>),
}
#[pyclass(module = "cs705.icf.settings.extended", frozen)]
pub struct TextInput {
    value: TextValue,
}
impl TextInput {
    pub fn from_core(py: Python<'_>, value: core::TextInput<'_>) -> Self {
        Self {
            value: match value {
                core::TextInput::Display => TextValue::Display,
                core::TextInput::Cancel => TextValue::Cancel,
                core::TextInput::Begin(value) => TextValue::Begin(value),
                core::TextInput::Text(value) => TextValue::Text(PyBytes::new(py, value).unbind()),
            },
        }
    }
    pub fn with_core<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(core::TextInput<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        f(match &self.value {
            TextValue::Display => core::TextInput::Display,
            TextValue::Cancel => core::TextInput::Cancel,
            TextValue::Begin(value) => core::TextInput::Begin(*value),
            TextValue::Text(value) => core::TextInput::Text(value.bind(py).as_bytes()),
        })
    }
}
#[pymethods]
#[allow(non_snake_case)]
impl TextInput {
    #[classattr]
    fn Display() -> Self {
        Self {
            value: TextValue::Display,
        }
    }
    #[classattr]
    fn Cancel() -> Self {
        Self {
            value: TextValue::Cancel,
        }
    }
    #[staticmethod]
    #[pyo3(signature = (character))]
    fn Begin(character: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self {
            value: TextValue::Begin(character.map(strict_u8).transpose()?),
        })
    }
    #[staticmethod]
    fn Text(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            value: TextValue::Text(value.cast::<PyBytes>()?.clone().unbind()),
        })
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self.value {
            TextValue::Display => "Display",
            TextValue::Cancel => "Cancel",
            TextValue::Begin(_) => "Begin",
            TextValue::Text(_) => "Text",
        }
    }
    #[getter]
    fn character(&self) -> Option<u8> {
        match self.value {
            TextValue::Begin(value) => value,
            _ => None,
        }
    }
    #[getter]
    fn source(&self, py: Python<'_>) -> Option<Py<PyBytes>> {
        match &self.value {
            TextValue::Text(value) => Some(value.clone_ref(py)),
            _ => None,
        }
    }
}

enum State<T: 'static> {
    Owned(Owner<T>),
    Scoped(crate::scoped::Loan<T>),
    Projection(crate::clone::Handle<T>),
}

enum FixedInputValue {
    Display,
    Cancel,
    Begin,
    Text {
        source: Py<MutableBytes>,
        phase: u32,
    },
}
#[pyclass(module = "cs705.icf.settings.extended", frozen)]
pub struct FixedTextInput {
    value: FixedInputValue,
}
impl FixedTextInput {
    pub fn with_core<R>(
        &self,
        _py: Python<'_>,
        f: impl FnOnce(core::FixedTextInput<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        match &self.value {
            FixedInputValue::Display => f(core::FixedTextInput::Display),
            FixedInputValue::Cancel => f(core::FixedTextInput::Cancel),
            FixedInputValue::Begin => f(core::FixedTextInput::Begin),
            FixedInputValue::Text { source, phase } => {
                let mut bytes = source.get().take()?;
                f(core::FixedTextInput::Text {
                    source: bytes.as_mut_slice(),
                    phase: *phase,
                })
            }
        }
    }
}
#[pymethods]
#[allow(non_snake_case)]
impl FixedTextInput {
    #[classattr]
    fn Display() -> Self {
        Self {
            value: FixedInputValue::Display,
        }
    }
    #[classattr]
    fn Cancel() -> Self {
        Self {
            value: FixedInputValue::Cancel,
        }
    }
    #[classattr]
    fn Begin() -> Self {
        Self {
            value: FixedInputValue::Begin,
        }
    }
    #[staticmethod]
    fn Text(source: Py<MutableBytes>, phase: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            value: FixedInputValue::Text {
                source,
                phase: strict_u32(phase)?,
            },
        })
    }
    #[getter]
    fn source(&self, py: Python<'_>) -> Option<Py<MutableBytes>> {
        match &self.value {
            FixedInputValue::Text { source, .. } => Some(source.clone_ref(py)),
            _ => None,
        }
    }
    #[getter]
    fn phase(&self) -> Option<u32> {
        match self.value {
            FixedInputValue::Text { phase, .. } => Some(phase),
            _ => None,
        }
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self.value {
            FixedInputValue::Display => "Display",
            FixedInputValue::Cancel => "Cancel",
            FixedInputValue::Begin => "Begin",
            FixedInputValue::Text { .. } => "Text",
        }
    }
}
macro_rules! effects {
    ($name:ident { $($field:ident: $ty:ty => $set:ident;)* }) => {
        #[pyclass(module = "cs705.icf.settings.extended", frozen)]
        pub struct $name { state: State<core::$name> }
        impl $name {
            pub fn from_core(value: core::$name) -> Self { Self { state: State::Owned(Owner::new(value)) } }
            pub fn from_loan(value: crate::scoped::Loan<core::$name>) -> Self { Self { state: State::Scoped(value) } }
            pub fn with_ref<R>(&self, f: impl FnOnce(&core::$name)->PyResult<R>)->PyResult<R> { match &self.state { State::Owned(owner)=>owner.with_ref(f),State::Scoped(loan)=>Python::attach(|py|loan.with_ref(py,f)),State::Projection(root)=>root.with(|v|f(v)) } }
            pub fn with_mut<R>(&self, f: impl FnOnce(&mut core::$name)->PyResult<R>)->PyResult<R> { match &self.state { State::Owned(owner)=>owner.with_mut(f),State::Scoped(loan)=>Python::attach(|py|loan.with_mut(py,f)),State::Projection(root)=>root.with(f) } }
        }
        #[pymethods]
        impl $name {
            #[new] fn new($($field: &Bound<'_, PyAny>),*) -> PyResult<Self> { Ok(Self::from_core(core::$name { $($field: <$ty as Input>::input($field)?),* })) }
            $(#[getter] fn $field<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py,PyAny>> { let value=self.with_ref(|v|Ok(v.$field.clone()))?; value.output(py) }
            #[setter] fn $set(&self,value:&Bound<'_,PyAny>)->PyResult<()> { let value=<$ty as Input>::input(value)?; self.with_mut(|v| {v.$field=value;Ok(())}) })*
        }
    }
}
effects!(NumberEffect { stored: bool => set_stored; accepted: bool => set_accepted; candidate: i64 => set_candidate; raw_candidate: i32 => set_raw_candidate; state_out: u32 => set_state_out; notify_flags: u32 => set_notify_flags; suffix: Vec<u8> => set_suffix; gray: bool => set_gray; });
effects!(TextEffect { stored: bool => set_stored; state_out: u32 => set_state_out; notify_flags: u32 => set_notify_flags; display: Vec<u8> => set_display; });
effects!(ChoiceEffect { stored: bool => set_stored; value: i64 => set_value; notify_flags: u32 => set_notify_flags; gray: bool => set_gray; popup_requested: bool => set_popup_requested; fixed_value: Option<core::FixedValue> => set_fixed_value; });
impl NumberEffect {
    pub(crate) fn from_projection(value: crate::clone::Handle<core::NumberEffect>) -> Self {
        Self {
            state: State::Projection(value),
        }
    }
}
impl Input<'_> for core::FixedValue {
    fn input(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(
            match super::super::enum_value(value, "cs705.icf.settings.extended", "FixedValue")? {
                0 => Self::AgcFast,
                1 => Self::AgcDelay100Millis,
                2 => Self::NotApplicable,
                _ => {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "invalid FixedValue",
                    ));
                }
            },
        )
    }
}
impl Output for core::FixedValue {
    fn output<'py>(self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        super::super::enum_result(py, "cs705.icf.settings.extended", "FixedValue", self as i64)
    }
}

#[pyclass(module = "cs705.icf.settings.extended")]
pub struct FixedTextEffect {
    #[pyo3(get, set)]
    pub text: Py<TextEffect>,
    context60: Option<u32>,
    warning_bits: u32,
    marked: bool,
}
impl FixedTextEffect {
    pub fn from_core(py: Python<'_>, value: core::FixedTextEffect) -> PyResult<Self> {
        Ok(Self {
            text: Py::new(py, TextEffect::from_core(value.text))?,
            context60: value.context60,
            warning_bits: value.warning_bits,
            marked: value.marked,
        })
    }
    pub fn to_core(&self, py: Python<'_>) -> PyResult<core::FixedTextEffect> {
        Ok(core::FixedTextEffect {
            text: self
                .text
                .try_borrow(py)?
                .with_ref(|value| Ok(value.clone()))?,
            context60: self.context60,
            warning_bits: self.warning_bits,
            marked: self.marked,
        })
    }
}
#[pymethods]
impl FixedTextEffect {
    #[new]
    #[pyo3(signature = (text, context60, warning_bits, marked))]
    fn new(
        text: Py<TextEffect>,
        context60: Option<&Bound<'_, PyAny>>,
        warning_bits: &Bound<'_, PyAny>,
        marked: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            text,
            context60: context60.map(strict_u32).transpose()?,
            warning_bits: strict_u32(warning_bits)?,
            marked: strict_bool(marked)?,
        })
    }
    #[getter]
    fn context60(&self) -> Option<u32> {
        self.context60
    }
    #[setter]
    fn set_context60(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.context60 = value.map(strict_u32).transpose()?;
        Ok(())
    }
    #[getter]
    fn warning_bits(&self) -> u32 {
        self.warning_bits
    }
    #[setter]
    fn set_warning_bits(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.warning_bits = strict_u32(value)?;
        Ok(())
    }
    #[getter]
    fn marked(&self) -> bool {
        self.marked
    }
    #[setter]
    fn set_marked(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.marked = strict_bool(value)?;
        Ok(())
    }
}
#[pyfunction]
fn choice_candidate(
    py: Python<'_>,
    old: &Bound<'_, PyAny>,
    choices: Vec<Bound<'_, PyAny>>,
    input: &ChoiceInput,
) -> PyResult<Option<i32>> {
    let old = strict_i64(old)?;
    let choices = choices
        .iter()
        .map(strict_i64)
        .collect::<PyResult<Vec<_>>>()?;
    input.with_core(py, |input| {
        if choices.is_empty()
            && matches!(input, core::ChoiceInput::Next | core::ChoiceInput::Previous)
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "stepping requires at least one choice",
            ));
        }
        Ok(core::choice_candidate(old, &choices, input))
    })
}
#[pyclass(module = "cs705.icf.settings.extended")]
pub struct BandwidthEffect {
    #[pyo3(get, set)]
    pub number: Py<NumberEffect>,
    popup_requested: bool,
    rewritten_text: Option<Vec<u8>>,
}
impl BandwidthEffect {
    pub fn from_core(py: Python<'_>, value: core::BandwidthEffect) -> PyResult<Self> {
        Ok(Self {
            number: Py::new(py, NumberEffect::from_core(value.number))?,
            popup_requested: value.popup_requested,
            rewritten_text: value.rewritten_text,
        })
    }
    pub fn to_core(&self, _py: Python<'_>) -> PyResult<core::BandwidthEffect> {
        Ok(core::BandwidthEffect {
            number: self.number.get().with_ref(|value| Ok(value.clone()))?,
            popup_requested: self.popup_requested,
            rewritten_text: self.rewritten_text.clone(),
        })
    }
}
#[pymethods]
impl BandwidthEffect {
    #[new]
    #[pyo3(signature=(number,popup_requested,rewritten_text))]
    fn new(
        number: Py<NumberEffect>,
        popup_requested: &Bound<'_, PyAny>,
        rewritten_text: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            number,
            popup_requested: strict_bool(popup_requested)?,
            rewritten_text: rewritten_text
                .map(|value| Ok::<_, PyErr>(strict_bytes(value)?.to_vec()))
                .transpose()?,
        })
    }
    #[getter]
    fn popup_requested(&self) -> bool {
        self.popup_requested
    }
    #[setter]
    fn set_popup_requested(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.popup_requested = strict_bool(value)?;
        Ok(())
    }
    #[getter]
    fn rewritten_text<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.rewritten_text
            .as_ref()
            .map(|value| PyBytes::new(py, value))
    }
    #[setter]
    fn set_rewritten_text(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.rewritten_text = value
            .map(|value| Ok::<_, PyErr>(strict_bytes(value)?.to_vec()))
            .transpose()?;
        Ok(())
    }
}
#[pyfunction]
fn edit_bandwidth(
    py: Python<'_>,
    image: &super::super::Icf,
    index: &Bound<'_, PyAny>,
    input: &NumberInput,
    crt_point: &Bound<'_, PyAny>,
) -> PyResult<BandwidthEffect> {
    let index = strict_usize(index)?;
    let point = strict_u8(crt_point)?;
    let value = input.with_core(py, |input| {
        image.with_mut(|image| {
            core::edit_bandwidth(image, index, input, point).map_err(crate::errors::icf)
        })
    })?;
    BandwidthEffect::from_core(py, value)
}

#[pyclass(module = "cs705.icf.settings.extended", frozen)]
pub struct NumberField {
    root: crate::clone::Handle<core::NumberField>,
}
impl NumberField {
    pub fn from_core(value: core::NumberField) -> Self {
        Self {
            root: crate::clone::Handle::new(value),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&core::NumberField) -> PyResult<R>) -> PyResult<R> {
        self.root.with(|v| f(v))
    }
    pub fn with_mut<R>(
        &self,
        f: impl FnOnce(&mut core::NumberField) -> PyResult<R>,
    ) -> PyResult<R> {
        self.root.with(f)
    }
}
macro_rules! number_metadata {
    ($($field:ident:$ty:ty=>$setter:ident,$strict:ident;)*) => { #[pymethods] impl NumberField {
        #[new]
        fn new(feature_id: &str, name: &str, unit: &str, $($field: &Bound<'_, PyAny>),*) -> PyResult<Self> {
            Ok(Self::from_core(core::NumberField {
                feature_id: std::borrow::Cow::Owned(feature_id.to_owned()),
                name: std::borrow::Cow::Owned(name.to_owned()), unit: std::borrow::Cow::Owned(unit.to_owned()),
                $($field: $strict($field)?,)*
            }))
        }
        $(#[getter] fn $field(&self) -> PyResult<$ty> { self.with_ref(|v| Ok(v.$field)) }
        #[setter] fn $setter(&self, v: &Bound<'_, PyAny>) -> PyResult<()> { let v = $strict(v)?; self.with_mut(|d| { d.$field = v; Ok(()) }) })*
        #[getter] fn feature_id(&self) -> PyResult<String> { self.with_ref(|v| Ok(v.feature_id.to_string())) }
        #[setter] fn set_feature_id(&self, v: &str) -> PyResult<()> { self.with_mut(|d| { d.feature_id = std::borrow::Cow::Owned(v.to_owned()); Ok(()) }) }
        #[getter] fn name(&self) -> PyResult<String> { self.with_ref(|v| Ok(v.name.to_string())) }
        #[setter] fn set_name(&self, v: &str) -> PyResult<()> { self.with_mut(|d| { d.name = std::borrow::Cow::Owned(v.to_owned()); Ok(()) }) }
        #[getter] fn unit(&self) -> PyResult<String> { self.with_ref(|v| Ok(v.unit.to_string())) }
        #[setter] fn set_unit(&self, v: &str) -> PyResult<()> { self.with_mut(|d| { d.unit = std::borrow::Cow::Owned(v.to_owned()); Ok(()) }) }
        fn accepts_index(&self, index: &Bound<'_, PyAny>) -> PyResult<bool> { let index = strict_usize(index)?; self.with_ref(|v| Ok(v.accepts_index(index))) }
        fn text_grammar(&self) -> PyResult<&'static str> { self.with_ref(|v| Ok(v.text_grammar())) }
    }};
}
number_metadata! {key:u16=>set_key,strict_u16;minimum:i64=>set_minimum,strict_i64;maximum:i64=>set_maximum,strict_i64;step:i64=>set_step,strict_i64;count:usize=>set_count,strict_usize;text_scale:u32=>set_text_scale,strict_u32;selected_raw_bypass:bool=>set_selected_raw_bypass,strict_bool;}
#[pyfunction]
fn number_fields() -> Vec<NumberField> {
    core::number_fields()
        .into_iter()
        .map(NumberField::from_core)
        .collect()
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "extended")?;
    let entries = PyDict::new(m.py());
    for (name, value) in [
        ("AgcFast", 0),
        ("AgcDelay100Millis", 1),
        ("NotApplicable", 2),
    ] {
        entries.set_item(name, value)?;
    }
    let kwargs = PyDict::new(m.py());
    kwargs.set_item("module", "cs705.icf.settings.extended")?;
    m.add(
        "FixedValue",
        m.py()
            .import("enum")?
            .getattr("Enum")?
            .call(("FixedValue", entries), Some(&kwargs))?,
    )?;
    m.add_class::<ChoiceInput>()?;
    m.add_class::<NumberInput>()?;
    m.add_class::<TextInput>()?;
    m.add_class::<NumberEffect>()?;
    m.add_class::<TextEffect>()?;
    m.add_class::<ChoiceEffect>()?;
    m.add_class::<FixedTextEffect>()?;
    m.add_class::<FixedTextInput>()?;
    m.add_class::<BandwidthEffect>()?;
    m.add_class::<NumberField>()?;
    m.add_function(wrap_pyfunction!(number_fields, &m)?)?;
    m.add_function(wrap_pyfunction!(edit_bandwidth, &m)?)?;
    m.add_function(wrap_pyfunction!(choice_candidate, &m)?)?;
    editors::register(&m)?;
    enums::register(&m)?;
    descriptors::register(&m)?;
    runtimes::register(&m)?;
    runners::register(&m)?;
    parent.add_submodule(&m)
}
mod descriptors;
pub use descriptors::ChoiceField;
mod editors;
mod enums;
mod runners;
mod runtimes;
