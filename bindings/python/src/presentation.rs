//! Explicit per-client language and native presentation resources.
use crate::conversion::*;
use cs705_core::presentation as core;
use pyo3::prelude::*;
use pyo3::types::PyDict;

pub(crate) fn add_enum(
    m: &Bound<'_, PyModule>,
    module: &str,
    name: &str,
    names: &[&str],
) -> PyResult<()> {
    let entries = PyDict::new(m.py());
    for (index, name) in names.iter().enumerate() {
        entries.set_item(name, index)?;
    }
    let kwargs = PyDict::new(m.py());
    kwargs.set_item("module", module)?;
    m.add(
        name,
        m.py()
            .import("enum")?
            .getattr("Enum")?
            .call((name, entries), Some(&kwargs))?,
    )
}
pub(crate) fn language(value: &Bound<'_, PyAny>) -> PyResult<core::Language> {
    let code = crate::icf::enum_value(value, "cs705.presentation", "Language")?;
    core::Language::ALL
        .get(usize::try_from(code).unwrap_or(usize::MAX))
        .copied()
        .ok_or_else(|| crate::errors::presentation(core::Error::UnsupportedLanguage))
}
#[pyfunction]
fn language_code(value: &Bound<'_, PyAny>) -> PyResult<&'static str> {
    Ok(language(value)?.code())
}
#[pyfunction]
fn localized_error(
    error: &crate::diagnostic::Diagnostic,
    value: &Bound<'_, PyAny>,
) -> PyResult<String> {
    let error = error.presentation_error().ok_or_else(|| {
        pyo3::exceptions::PyTypeError::new_err("expected a presentation diagnostic")
    })?;
    Ok(error.localized(language(value)?))
}
#[pyfunction]
fn fixed_value_source_label(value: &Bound<'_, PyAny>) -> PyResult<&'static str> {
    use cs705_core::icf::settings::extended::FixedValue::*;
    match crate::icf::enum_value(value, "cs705.presentation", "FixedValue")? {
        0 => Ok(AgcFast.source_label()),
        1 => Ok(AgcDelay100Millis.source_label()),
        2 => Ok(NotApplicable.source_label()),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "invalid FixedValue",
        )),
    }
}
#[pyfunction]
fn parse_language<'py>(py: Python<'py>, code: &str) -> PyResult<Bound<'py, PyAny>> {
    let language: core::Language = code.parse().map_err(crate::errors::presentation)?;
    crate::icf::enum_result(py, "cs705.presentation", "Language", language as i64)
}

#[pyclass(module = "cs705.presentation", frozen)]
pub struct Message {
    pub inner: core::Message,
}
macro_rules! messages {
    ($($unit:ident),*; $($signed:ident),*) => {
        #[pymethods] impl Message {
            #[getter] fn variant(&self)->&'static str { match self.inner { $(core::Message::$unit=>stringify!($unit),)* $(core::Message::$signed(_)=>stringify!($signed),)* core::Message::RowsProcessed(_)=>"RowsProcessed" } }
            #[getter] fn value(&self)->Option<i64>{match self.inner {$(core::Message::$signed(value)=>Some(value),)* _=>None}}
            #[getter] fn count(&self)->Option<u64>{match self.inner {core::Message::RowsProcessed(count)=>Some(count),_=>None}}
            $(#[staticmethod] #[allow(non_snake_case)] fn $unit() -> Self { Self { inner: core::Message::$unit } })*
            $(#[staticmethod] #[allow(non_snake_case)] fn $signed(value: &Bound<'_,PyAny>) -> PyResult<Self> { Ok(Self { inner: core::Message::$signed(strict_i64(value)?) }) })*
            #[staticmethod] #[allow(non_snake_case)] fn RowsProcessed(count: &Bound<'_,PyAny>) -> PyResult<Self> { Ok(Self { inner: core::Message::RowsProcessed(strict_u64(count)?) }) }
        }
    }
}
messages!(Enabled, Disabled, NoSymbol, NotApplicable, AgcFast, AgcDelay100Millis, ReadOnly, Cancelled, NotPublished, PartialNotPublished, PartialPublished, WriteAcknowledged, ReadRadio, WriteRadio, SaveFile, ReadFile; Code, UnknownCode, Number, OutOfRange);

#[pyclass(module = "cs705.presentation", frozen)]
pub struct Value {
    inner: core::Value,
}
macro_rules! values {
    ($($name:ident { $($variant:ident),* });*) => {
        #[pymethods] impl Value {
            #[getter] fn variant(&self)->&'static str {match self.inner {core::Value::Boolean(_)=>"Boolean",$(core::Value::$name(_)=>stringify!($name),)*}}
            #[getter] fn payload<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>> {match self.inner {core::Value::Boolean(value)=>Ok(value.into_pyobject(py)?.to_owned().into_any()),$(core::Value::$name(value)=>crate::icf::enum_result(py,"cs705.icf",stringify!($name),value as i64),)*}}
            #[staticmethod] #[allow(non_snake_case)] fn Boolean(value: &Bound<'_,PyAny>) -> PyResult<Self> { Ok(Self { inner: core::Value::Boolean(strict_bool(value)?) }) }
            $(#[staticmethod] #[allow(non_snake_case)] fn $name(value: &Bound<'_,PyAny>) -> PyResult<Self> {
                let code=crate::icf::enum_value(value,"cs705.icf",stringify!($name))?;
                $(if code==cs705_core::icf::$name::$variant as i64 {return Ok(Self {inner:core::Value::$name(cs705_core::icf::$name::$variant)});})*
                Err(pyo3::exceptions::PyValueError::new_err("invalid enum value"))
            })*
        }
    }
}
values!(Agc {Fast,Mid,Slow}; FilterShape {Sharp,Soft}; RepeaterMode {Dv,Fm,FmNarrow,Dd}; RepeaterDuplex {Off,Minus,Plus,Rps}; RepeaterTone {Off,Tone,ToneSquelch}; RepeaterPosition {None,Approximate,Exact});

#[pyclass(module = "cs705.presentation", get_all)]
pub struct FieldText {
    key: u16,
    name: String,
    label: String,
}
#[pymethods]
impl FieldText {
    #[new]
    fn new(key: &Bound<'_, PyAny>, name: &str, label: &str) -> PyResult<Self> {
        Ok(Self {
            key: strict_u16(key)?,
            name: name.into(),
            label: label.into(),
        })
    }
    #[setter]
    fn set_key(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.key = strict_u16(value)?;
        Ok(())
    }
    #[setter]
    fn set_name(&mut self, value: &str) {
        self.name = value.into();
    }
    #[setter]
    fn set_label(&mut self, value: &str) {
        self.label = value.into();
    }
}
#[pyclass(module = "cs705.presentation", get_all)]
pub struct ValueText {
    raw: i64,
    text: String,
    in_domain: bool,
}
#[pymethods]
impl ValueText {
    #[new]
    fn new(raw: &Bound<'_, PyAny>, text: &str, in_domain: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            raw: strict_i64(raw)?,
            text: text.into(),
            in_domain: strict_bool(in_domain)?,
        })
    }
    #[setter]
    fn set_raw(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.raw = strict_i64(value)?;
        Ok(())
    }
    #[setter]
    fn set_text(&mut self, value: &str) {
        self.text = value.into();
    }
    #[setter]
    fn set_in_domain(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.in_domain = strict_bool(value)?;
        Ok(())
    }
}
impl From<core::ValueText> for ValueText {
    fn from(v: core::ValueText) -> Self {
        Self {
            raw: v.raw,
            text: v.text,
            in_domain: v.in_domain,
        }
    }
}

#[pyclass(module = "cs705.presentation", frozen)]
pub struct Presenter {
    pub inner: core::Presenter,
}
#[pymethods]
impl Presenter {
    #[new]
    fn new(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::Presenter::new(language(value)?),
        })
    }
    fn language<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        crate::icf::enum_result(
            py,
            "cs705.presentation",
            "Language",
            self.inner.language() as i64,
        )
    }
    fn field_label(&self, key: &Bound<'_, PyAny>) -> PyResult<&'static str> {
        self.inner
            .field_label(strict_u16(key)?)
            .map_err(crate::errors::presentation)
    }
    fn setting(&self, value: &crate::icf::settings::Setting) -> PyResult<FieldText> {
        value.with_ref(|value| {
            let v = self
                .inner
                .setting(value)
                .map_err(crate::errors::presentation)?;
            Ok(FieldText {
                key: v.key,
                name: v.name.into(),
                label: v.label.into(),
            })
        })
    }
    fn setting_value(
        &self,
        value: &crate::icf::settings::Setting,
        index: &Bound<'_, PyAny>,
        raw: &Bound<'_, PyAny>,
    ) -> PyResult<ValueText> {
        let index = strict_usize(index)?;
        let raw = strict_i64(raw)?;
        value.with_ref(|value| {
            Ok(self
                .inner
                .setting_value(value, index, raw)
                .map_err(crate::errors::presentation)?
                .into())
        })
    }
    fn choice(
        &self,
        value: &crate::icf::settings::extended::ChoiceField,
        index: &Bound<'_, PyAny>,
        raw: &Bound<'_, PyAny>,
    ) -> PyResult<ValueText> {
        let index = strict_usize(index)?;
        let raw = strict_i64(raw)?;
        value.with_ref(|value| {
            Ok(self
                .inner
                .choice(value, index, raw)
                .map_err(crate::errors::presentation)?
                .into())
        })
    }
    fn choice_label(
        &self,
        value: &crate::icf::settings::extended::ChoiceField,
        index: &Bound<'_, PyAny>,
    ) -> PyResult<String> {
        let index = strict_usize(index)?;
        value.with_ref(|value| {
            self.inner
                .choice_label(value, index)
                .map_err(crate::errors::presentation)
        })
    }
    fn fixed_value(&self, value: &Bound<'_, PyAny>) -> PyResult<String> {
        use cs705_core::icf::settings::extended::FixedValue::*;
        let code = crate::icf::enum_value(value, "cs705.presentation", "FixedValue")?;
        let v = match code {
            0 => AgcFast,
            1 => AgcDelay100Millis,
            2 => NotApplicable,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "invalid FixedValue",
                ));
            }
        };
        Ok(self.inner.fixed_value(v))
    }
    fn callsign_label(&self, value: &Bound<'_, PyAny>) -> PyResult<&'static str> {
        let code = crate::icf::enum_value(value, "cs705.icf", "MemoryCallsign")?;
        use cs705_core::icf::MemoryCallsign::*;
        let field = match code {
            0 => Your,
            1 => Repeater1,
            2 => Repeater2,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "invalid MemoryCallsign",
                ));
            }
        };
        self.inner
            .callsign_label(field)
            .map_err(crate::errors::presentation)
    }
    fn channel_label(&self, value: &Bound<'_, PyAny>) -> PyResult<&'static str> {
        use cs705_core::icf::record::ChannelField::*;
        let values = [
            FrequencyHz,
            Duplex,
            Selection,
            Tone,
            Dtcs,
            Polarity,
            DigitalSquelch,
            DigitalCode,
        ];
        let code = crate::icf::enum_value(value, "cs705.presentation", "ChannelField")?;
        let field = values
            .get(usize::try_from(code).unwrap_or(usize::MAX))
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid ChannelField"))?;
        self.inner
            .channel_label(*field)
            .map_err(crate::errors::presentation)
    }
    fn repeater_label(&self, value: &Bound<'_, PyAny>) -> PyResult<&'static str> {
        use cs705_core::icf::record::RepeaterField::*;
        let values = [
            Mode,
            Tone,
            ToneIndex,
            Use,
            Position,
            UtcOffset,
            GatewayIndex,
        ];
        let code = crate::icf::enum_value(value, "cs705.presentation", "RepeaterField")?;
        let field = values
            .get(usize::try_from(code).unwrap_or(usize::MAX))
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid RepeaterField"))?;
        self.inner
            .repeater_label(*field)
            .map_err(crate::errors::presentation)
    }
    fn network_label(&self, value: &Bound<'_, PyAny>) -> PyResult<&'static str> {
        use cs705_core::icf::settings::network::Field::*;
        let values = [Address, Gateway, PrimaryDns, SecondaryDns];
        let code = crate::icf::enum_value(value, "cs705.icf.settings.network", "Field")?;
        let field = values
            .get(usize::try_from(code).unwrap_or(usize::MAX))
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid network Field"))?;
        self.inner
            .network_label(*field)
            .map_err(crate::errors::presentation)
    }
    fn symbol(&self, raw: &Bound<'_, PyAny>) -> PyResult<String> {
        self.inner
            .symbol(strict_bytes(raw)?)
            .map_err(crate::errors::presentation)
    }
    fn message(&self, value: &Message) -> String {
        self.inner.message(value.inner)
    }
    fn value(&self, value: &Value) -> String {
        self.inner.value(value.inner)
    }
    fn diagnostic(&self, value: &crate::diagnostic::Diagnostic) -> String {
        self.inner.diagnostic(value.native())
    }
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "presentation")?;
    add_enum(
        &m,
        "cs705.presentation",
        "Language",
        &[
            "English", "Japanese", "German", "French", "Spanish", "Italian",
        ],
    )?;
    for name in ["ChannelField", "RepeaterField"] {
        m.add(name, root.getattr("icf")?.getattr("record")?.getattr(name)?)?;
    }
    m.add(
        "FixedValue",
        root.getattr("icf")?
            .getattr("settings")?
            .getattr("extended")?
            .getattr("FixedValue")?,
    )?;
    m.add_class::<Presenter>()?;
    m.add_class::<Message>()?;
    m.add_class::<Value>()?;
    m.add_class::<FieldText>()?;
    m.add_class::<ValueText>()?;
    m.add_function(wrap_pyfunction!(language_code, &m)?)?;
    m.add_function(wrap_pyfunction!(localized_error, &m)?)?;
    m.add_function(wrap_pyfunction!(fixed_value_source_label, &m)?)?;
    m.add_function(wrap_pyfunction!(parse_language, &m)?)?;
    root.add_submodule(&m)
}
