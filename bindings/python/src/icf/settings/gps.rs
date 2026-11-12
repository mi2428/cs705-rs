//! Direct GPS setting editors and explicit caller-owned native resources.
use super::extended::{
    ChoiceEffect, ChoiceInput, FixedTextInput, NumberEffect, NumberInput, TextEffect,
};
use crate::icf::values::{Input, Output};
use crate::{
    callbacks::{CallbackError, CallbackFailed},
    conversion::*,
    icf::Icf,
    owner::Owner,
};
use cs705_core::icf::settings::gps as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::cell::RefCell;

const MODULE: &str = "cs705.icf.settings.gps";

fn invoke<R>(
    interpreter: i64,
    errors: &RefCell<CallbackError>,
    f: impl FnOnce(Python<'_>) -> PyResult<R>,
) -> Result<R, CallbackFailed> {
    errors.borrow_mut().invoke(|| {
        Python::attach(|py| {
            if crate::scoped::interpreter(py)? != interpreter {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "GPS callback belongs to another interpreter",
                ));
            }
            f(py)
        })
    })
}
fn callable(value: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if !value.is_callable() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "expected a callable",
        ));
    }
    Ok(value.clone().unbind())
}
fn caption(
    interpreter: i64,
    errors: &RefCell<CallbackError>,
    f: &Py<PyAny>,
    resource: &[u8],
    index: usize,
) -> Result<Vec<u8>, cs705_core::icf::Error> {
    invoke(interpreter, errors, |py| {
        Ok(strict_bytes(&f.bind(py).call1((PyBytes::new(py, resource), index))?)?.to_vec())
    })
    .map_err(|_| cs705_core::icf::Error::Operation("Python GPS caption callback failed".into()))
}

macro_rules! value {
    ($name:ident { $($field:ident:$ty:ty=>$set:ident;)* } $($extra:tt)*) => {
        #[pyclass(module="cs705.icf.settings.gps")]
        #[derive(Clone)]
        pub struct $name { inner: core::$name }
        impl $name { pub fn from_core(inner: core::$name) -> Self { Self { inner } } }
        #[pymethods]
        impl $name {
            #[new]
            fn new($($field:&Bound<'_,PyAny>),*) -> PyResult<Self> {
                Ok(Self::from_core(core::$name { $($field: <$ty as Input>::input($field)?),* }))
            }
            $(#[getter] fn $field<'py>(&self, py:Python<'py>) -> PyResult<Bound<'py,PyAny>> { self.inner.$field.clone().output(py) }
            #[setter] fn $set(&mut self, value:&Bound<'_,PyAny>) -> PyResult<()> { self.inner.$field=<$ty as Input>::input(value)?; Ok(()) })*
            fn __copy__(&self)->Self { self.clone() }
            fn __deepcopy__(&self,_memo:&Bound<'_,PyAny>)->Self { self.clone() }
            $($extra)*
        }
    }
}
value!(Resources { system_langid:Option<u16> =>set_system_langid; language_descriptor:Option<u32> =>set_language_descriptor; });
value!(CoordinateEffect { stored:bool=>set_stored; raw:[u8;4]=>set_raw; gray:bool=>set_gray; }
    fn __eq__(&self,other:&Self)->bool { self.inner==other.inner }
);
value!(SentenceEffect { stored:bool=>set_stored; helper_changed:bool=>set_helper_changed; valid:bool=>set_valid; selected:u8=>set_selected; }
    fn __eq__(&self,other:&Self)->bool { self.inner==other.inner }
);

fn alarm(value: &Bound<'_, PyAny>) -> PyResult<core::AlarmField> {
    match crate::icf::enum_value(value, MODULE, "AlarmField")? {
        0 => Ok(core::AlarmField::Type),
        1 => Ok(core::AlarmField::Group),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "invalid AlarmField",
        )),
    }
}
fn sentence(value: &Bound<'_, PyAny>) -> PyResult<core::SentenceField> {
    match crate::icf::enum_value(value, MODULE, "SentenceField")? {
        0 => Ok(core::SentenceField::Nmea),
        1 => Ok(core::SentenceField::Logger),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "invalid SentenceField",
        )),
    }
}
macro_rules! metadata {
    ($function:ident,$convert:ident,$method:ident,$ty:ty) => {
        #[pyfunction]
        fn $function(field: &Bound<'_, PyAny>) -> PyResult<$ty> {
            Ok($convert(field)?.$method())
        }
    };
}
metadata!(alarm_native_name, alarm, name, &'static str);
metadata!(alarm_feature_id, alarm, feature_id, &'static str);
metadata!(alarm_key, alarm, key, u16);
metadata!(sentence_native_name, sentence, name, &'static str);
metadata!(sentence_feature_id, sentence, feature_id, &'static str);
metadata!(sentence_key, sentence, key, u16);
metadata!(sentence_count, sentence, count, usize);
#[pyfunction]
fn alarm_read(field: &Bound<'_, PyAny>, image: &Icf) -> PyResult<u8> {
    let field = alarm(field)?;
    image.with_ref(|image| Ok(field.read(image)))
}
#[pyfunction]
fn alarm_choices(field: &Bound<'_, PyAny>, image: &Icf) -> PyResult<Vec<i64>> {
    let field = alarm(field)?;
    image.with_ref(|image| Ok(field.choices(image)))
}
#[pyfunction]
fn alarm_edit(
    py: Python<'_>,
    field: &Bound<'_, PyAny>,
    image: &Icf,
    input: &ChoiceInput,
) -> PyResult<ChoiceEffect> {
    let (field, input) = (alarm(field)?, input.to_core(py)?);
    image.with_mut(|image| {
        field
            .edit(image, input)
            .map(ChoiceEffect::from_core)
            .map_err(crate::errors::icf)
    })
}
#[pyfunction]
fn find_sentence<'py>(py: Python<'py>, name_or_id: &str) -> PyResult<Bound<'py, PyAny>> {
    let value = core::find_sentence(name_or_id).map_err(crate::errors::icf)?;
    crate::icf::enum_result(py, MODULE, "SentenceField", value as i64)
}
#[pyfunction]
fn paint_alarm_group<'py>(
    py: Python<'py>,
    image: &Icf,
    provider: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let provider = callable(provider)?;
    let interpreter = crate::scoped::interpreter(py)?;
    let image = image.take()?;
    let (errors, result) = py.detach(move || {
        let errors = RefCell::new(CallbackError::default());
        let result = core::paint_alarm_group(&image, |resource, index| {
            caption(interpreter, &errors, &provider, resource, index)
        });
        (errors.into_inner(), result)
    });
    let result = errors.finish(py, result.map_err(crate::errors::icf))?;
    Ok(PyBytes::new(py, &result))
}
#[pyfunction]
fn sentence_paint<'py>(
    py: Python<'py>,
    field: &Bound<'_, PyAny>,
    image: &Icf,
    provider: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let field = sentence(field)?;
    let provider = callable(provider)?;
    let interpreter = crate::scoped::interpreter(py)?;
    let image = image.take()?;
    let (errors, result) = py.detach(move || {
        let errors = RefCell::new(CallbackError::default());
        let result = field.paint(&image, |resource, index| {
            caption(interpreter, &errors, &provider, resource, index)
        });
        (errors.into_inner(), result)
    });
    let result = errors.finish(py, result.map_err(crate::errors::icf))?;
    Ok(PyBytes::new(py, &result))
}
#[pyfunction]
fn gps_group_count_source(source: &Bound<'_, PyAny>, group: &Bound<'_, PyAny>) -> PyResult<u16> {
    core::gps_group_count_source(strict_bytes(source)?, strict_u8(group)?)
        .map_err(crate::errors::icf)
}
#[pyfunction]
fn paint_altitude_raw<'py>(
    py: Python<'py>,
    old: &Bound<'_, PyAny>,
    meter: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &core::paint_altitude_raw(strict_i32(old)?, strict_bool(meter)?),
    ))
}
#[pyfunction]
fn edit_altitude_live(
    py: Python<'_>,
    old: &Bound<'_, PyAny>,
    image: &Icf,
    input: &NumberInput,
    point: &Bound<'_, PyAny>,
) -> PyResult<NumberEffect> {
    let (old, point) = (strict_i32(old)?, strict_u8(point)?);
    input.with_core(py, |input| {
        image.with_ref(|image| {
            core::edit_altitude_live(old, image, input, point)
                .map(NumberEffect::from_core)
                .map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
#[pyo3(signature=(old,meter,input,point,unit_output))]
fn edit_altitude_raw(
    py: Python<'_>,
    old: &Bound<'_, PyAny>,
    meter: &Bound<'_, PyAny>,
    input: &NumberInput,
    point: &Bound<'_, PyAny>,
    unit_output: Option<&Bound<'_, PyAny>>,
) -> PyResult<NumberEffect> {
    let (old, meter, point) = (strict_i32(old)?, strict_bool(meter)?, strict_u8(point)?);
    let output = unit_output.map(strict_bytes).transpose()?;
    input.with_core(py, |input| {
        core::edit_altitude_raw(old, meter, input, point, output)
            .map(NumberEffect::from_core)
            .map_err(crate::errors::icf)
    })
}

#[pyclass(module = "cs705.icf.settings.gps", frozen)]
#[derive(Clone, Copy)]
pub struct SentenceEvent {
    inner: core::SentenceEvent,
}
#[pymethods]
#[allow(non_snake_case)]
impl SentenceEvent {
    #[staticmethod]
    fn Choose(index: &Bound<'_, PyAny>, selected: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::SentenceEvent::Choose {
                index: strict_usize(index)?,
                selected: strict_bool(selected)?,
            },
        })
    }
    #[classattr]
    fn Ok() -> Self {
        Self {
            inner: core::SentenceEvent::Ok,
        }
    }
    #[classattr]
    fn Cancel() -> Self {
        Self {
            inner: core::SentenceEvent::Cancel,
        }
    }
    #[classattr]
    fn Close() -> Self {
        Self {
            inner: core::SentenceEvent::Close,
        }
    }
    #[classattr]
    fn Deactivate() -> Self {
        Self {
            inner: core::SentenceEvent::Deactivate,
        }
    }
    #[classattr]
    fn OwnerDestroyed() -> Self {
        Self {
            inner: core::SentenceEvent::OwnerDestroyed,
        }
    }
    #[getter]
    fn kind(&self) -> &'static str {
        match self.inner {
            core::SentenceEvent::Choose { .. } => "Choose",
            core::SentenceEvent::Ok => "Ok",
            core::SentenceEvent::Cancel => "Cancel",
            core::SentenceEvent::Close => "Close",
            core::SentenceEvent::Deactivate => "Deactivate",
            core::SentenceEvent::OwnerDestroyed => "OwnerDestroyed",
        }
    }
    #[getter]
    fn index(&self) -> Option<usize> {
        if let core::SentenceEvent::Choose { index, .. } = self.inner {
            Some(index)
        } else {
            None
        }
    }
    #[getter]
    fn selected(&self) -> Option<bool> {
        if let core::SentenceEvent::Choose { selected, .. } = self.inner {
            Some(selected)
        } else {
            None
        }
    }
    /// Return a replacement Choose value; the original remains unchanged.
    fn with_choice(&self, index: &Bound<'_, PyAny>, selected: &Bound<'_, PyAny>) -> PyResult<Self> {
        if !matches!(self.inner, core::SentenceEvent::Choose { .. }) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "with_choice requires SentenceEvent.Choose",
            ));
        }
        Self::Choose(index, selected)
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
    fn __copy__(&self) -> Self {
        *self
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> Self {
        *self
    }
}
#[pyclass(module = "cs705.icf.settings.gps", frozen)]
pub struct SentenceDialog {
    inner: Owner<core::SentenceDialog>,
}
#[pymethods]
impl SentenceDialog {
    #[new]
    fn new(image: &Icf, field: &Bound<'_, PyAny>) -> PyResult<Self> {
        let field = sentence(field)?;
        image.with_ref(|image| {
            Ok(Self {
                inner: Owner::new(core::SentenceDialog::new(image, field)),
            })
        })
    }
    fn selected(&self) -> PyResult<u8> {
        self.inner.with_ref(|v| Ok(v.selected()))
    }
    fn valid(&self) -> PyResult<bool> {
        self.inner.with_ref(|v| Ok(v.valid()))
    }
    #[getter]
    fn cancelled(&self) -> PyResult<bool> {
        self.inner.with_ref(|v| Ok(v.cancelled))
    }
    #[setter]
    fn set_cancelled(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.inner.with_mut(|v| {
            v.cancelled = value;
            Ok(())
        })
    }
    #[getter]
    fn done(&self) -> PyResult<bool> {
        self.inner.with_ref(|v| Ok(v.done))
    }
    #[setter]
    fn set_done(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.inner.with_mut(|v| {
            v.done = value;
            Ok(())
        })
    }
    #[getter]
    fn release_requested(&self) -> PyResult<bool> {
        self.inner.with_ref(|v| Ok(v.release_requested))
    }
    #[setter]
    fn set_release_requested(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.inner.with_mut(|v| {
            v.release_requested = value;
            Ok(())
        })
    }
    fn apply(&self, image: &Icf, event: &SentenceEvent) -> PyResult<SentenceEffect> {
        self.inner.with_mut(|dialog| {
            image.with_mut(|image| {
                dialog
                    .apply(image, event.inner)
                    .map(SentenceEffect::from_core)
                    .map_err(crate::errors::icf)
            })
        })
    }
}

#[pyclass(module = "cs705.icf.settings.gps", frozen)]
pub struct NativeSymbolPairs {
    bytes: Vec<u8>,
}
#[pymethods]
impl NativeSymbolPairs {
    #[staticmethod]
    fn read(bytes: &Bound<'_, PyAny>) -> PyResult<Self> {
        let bytes = strict_bytes(bytes)?;
        core::NativeSymbolPairs::read(bytes).map_err(crate::errors::icf)?;
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }
}

macro_rules! descriptor {
    ($name:ident) => {
        #[pyclass(module = "cs705.icf.settings.gps", frozen)]
        pub struct $name {
            inner: Owner<core::$name>,
        }
        impl $name {
            fn from_core(value: core::$name) -> Self {
                Self {
                    inner: Owner::new(value),
                }
            }
        }
    };
}
#[pyclass(module = "cs705.icf.settings.gps", frozen)]
pub struct ChoiceField {
    inner: crate::clone::Handle<core::ChoiceField>,
}
impl ChoiceField {
    fn from_core(value: core::ChoiceField) -> Self {
        Self {
            inner: crate::clone::Handle::new(value),
        }
    }
}
#[pyclass(module = "cs705.icf.settings.gps", frozen)]
pub struct NumberField {
    inner: crate::clone::Handle<core::NumberField>,
}
impl NumberField {
    fn from_core(value: core::NumberField) -> Self {
        Self {
            inner: crate::clone::Handle::new(value),
        }
    }
}
descriptor!(SymbolField);
descriptor!(CoordinateField);
descriptor!(TextField);

#[pymethods]
impl ChoiceField {
    fn __copy__(&self) -> PyResult<Self> {
        self.inner.with(|v| Ok(Self::from_core(v.clone())))
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.__copy__()
    }
    #[getter]
    fn setting(&self) -> super::Setting {
        super::Setting::from_projection(self.inner.project(|v| Ok(&mut v.setting)))
    }
    #[setter]
    fn set_setting(&self, setting: &super::Setting) -> PyResult<()> {
        let value = setting.with_ref(|v| Ok(v.clone()))?;
        self.inner.with(|v| {
            v.setting = value;
            Ok(())
        })
    }
    fn selector(&self, image: &Icf, resources: &Resources) -> PyResult<String> {
        self.inner.with(|field| {
            image.with_ref(|image| {
                field
                    .selector(image, resources.inner)
                    .map(str::to_owned)
                    .map_err(crate::errors::icf)
            })
        })
    }
    fn edit(
        &self,
        py: Python<'_>,
        image: &Icf,
        input: &ChoiceInput,
        resources: &Resources,
    ) -> PyResult<ChoiceEffect> {
        let input = input.to_core(py)?;
        self.inner.with(|field| {
            image.with_mut(|image| {
                field
                    .edit(image, input, resources.inner)
                    .map(ChoiceEffect::from_core)
                    .map_err(crate::errors::icf)
            })
        })
    }
}
#[pymethods]
impl NumberField {
    fn __copy__(&self) -> PyResult<Self> {
        self.inner.with(|v| Ok(Self::from_core(v.clone())))
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.__copy__()
    }
    #[getter]
    fn setting(&self) -> super::Setting {
        super::Setting::from_projection(self.inner.project(|v| Ok(&mut v.setting)))
    }
    #[setter]
    fn set_setting(&self, setting: &super::Setting) -> PyResult<()> {
        let value = setting.with_ref(|v| Ok(v.clone()))?;
        self.inner.with(|v| {
            v.setting = value;
            Ok(())
        })
    }
    fn bounds(&self, image: &Icf) -> PyResult<(i64, i64)> {
        self.inner
            .with(|field| image.with_ref(|image| Ok(field.bounds(image))))
    }
    fn paint<'py>(
        &self,
        py: Python<'py>,
        image: &Icf,
        resources: &Resources,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = self.inner.with(|field| {
            image.with_ref(|image| {
                field
                    .paint(image, resources.inner)
                    .map_err(crate::errors::icf)
            })
        })?;
        Ok(PyBytes::new(py, &bytes))
    }
    #[pyo3(signature=(image,input,point,unit_output))]
    fn edit(
        &self,
        py: Python<'_>,
        image: &Icf,
        input: &NumberInput,
        point: &Bound<'_, PyAny>,
        unit_output: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<NumberEffect> {
        let point = strict_u8(point)?;
        let output = unit_output.map(strict_bytes).transpose()?;
        input.with_core(py, |input| {
            self.inner.with(|field| {
                image.with_mut(|image| {
                    field
                        .edit(image, input, point, output)
                        .map(NumberEffect::from_core)
                        .map_err(crate::errors::icf)
                })
            })
        })
    }
    fn edit_live(
        &self,
        py: Python<'_>,
        image: &Icf,
        input: &NumberInput,
        point: &Bound<'_, PyAny>,
        resources: &Resources,
    ) -> PyResult<NumberEffect> {
        let point = strict_u8(point)?;
        input.with_core(py, |input| {
            self.inner.with(|field| {
                image.with_mut(|image| {
                    field
                        .edit_live(image, input, point, resources.inner)
                        .map(NumberEffect::from_core)
                        .map_err(crate::errors::icf)
                })
            })
        })
    }
}
#[pyfunction]
fn choice_fields() -> Vec<ChoiceField> {
    core::choice_fields()
        .iter()
        .cloned()
        .map(ChoiceField::from_core)
        .collect()
}
#[pyfunction]
fn find_choice(name_or_id: &str) -> PyResult<ChoiceField> {
    core::find_choice(name_or_id)
        .cloned()
        .map(ChoiceField::from_core)
        .map_err(crate::errors::icf)
}
#[pyfunction]
fn number_fields() -> Vec<NumberField> {
    core::number_fields()
        .iter()
        .cloned()
        .map(NumberField::from_core)
        .collect()
}

#[pymethods]
impl SymbolField {
    fn __copy__(&self) -> PyResult<Self> {
        self.inner.with_ref(|v| Ok(Self::from_core(v.clone())))
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.__copy__()
    }
    #[getter]
    fn feature_id(&self) -> PyResult<String> {
        self.inner.with_ref(|v| Ok(v.feature_id.to_string()))
    }
    #[setter]
    fn set_feature_id(&self, value: &str) -> PyResult<()> {
        self.inner.with_mut(|v| {
            v.feature_id = value.to_owned().into();
            Ok(())
        })
    }
    #[getter]
    fn name(&self) -> PyResult<String> {
        self.inner.with_ref(|v| Ok(v.name.to_string()))
    }
    #[setter]
    fn set_name(&self, value: &str) -> PyResult<()> {
        self.inner.with_mut(|v| {
            v.name = value.to_owned().into();
            Ok(())
        })
    }
    #[getter]
    fn count(&self) -> PyResult<usize> {
        self.inner.with_ref(|v| Ok(v.count))
    }
    #[setter]
    fn set_count(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_usize(value)?;
        self.inner.with_mut(|v| {
            v.count = value;
            Ok(())
        })
    }
    #[getter]
    fn key(&self) -> PyResult<u16> {
        self.inner.with_ref(|v| Ok(v.key))
    }
    #[setter]
    fn set_key(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u16(value)?;
        self.inner.with_mut(|v| {
            v.key = value;
            Ok(())
        })
    }
    fn read<'py>(
        &self,
        py: Python<'py>,
        image: &Icf,
        index: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let index = strict_usize(index)?;
        let bytes = self.inner.with_ref(|field| {
            image.with_ref(|image| field.read(image, index).map_err(crate::errors::icf))
        })?;
        Ok(PyBytes::new(py, &bytes))
    }
    fn select(
        &self,
        py: Python<'_>,
        image: &Icf,
        index: &Bound<'_, PyAny>,
        input: &ChoiceInput,
        resource: &NativeSymbolPairs,
    ) -> PyResult<ChoiceEffect> {
        let (index, input) = (strict_usize(index)?, input.to_core(py)?);
        let resource =
            core::NativeSymbolPairs::read(&resource.bytes).map_err(crate::errors::icf)?;
        self.inner.with_ref(|field| {
            image.with_mut(|image| {
                field
                    .select(image, index, input, &resource)
                    .map(ChoiceEffect::from_core)
                    .map_err(crate::errors::icf)
            })
        })
    }
    fn edit(
        &self,
        py: Python<'_>,
        image: &Icf,
        index: &Bound<'_, PyAny>,
        input: &FixedTextInput,
    ) -> PyResult<TextEffect> {
        let index = strict_usize(index)?;
        let field = self.inner.take()?;
        let mut image = image.take()?;
        input.with_core(py, |input| {
            py.detach(|| {
                field
                    .edit(&mut image, index, input)
                    .map(TextEffect::from_core)
                    .map_err(crate::errors::csv)
            })
        })
    }
    fn edit_with_nls(
        &self,
        py: Python<'_>,
        image: &Icf,
        index: &Bound<'_, PyAny>,
        input: &FixedTextInput,
        map: &Bound<'_, PyAny>,
    ) -> PyResult<TextEffect> {
        let index = strict_usize(index)?;
        let map = callable(map)?;
        let interpreter = crate::scoped::interpreter(py)?;
        let field = self.inner.take()?;
        let mut image = image.take()?;
        input.with_core(py, |input| {
            let (errors, result) = py.detach(move || {
                let errors = RefCell::new(CallbackError::default());
                let result =
                    field.edit_with_nls(&mut image, index, input, |source, destination| {
                        invoke(interpreter, &errors, |py| {
                            crate::csv::nls::map_call(py, map.bind(py), source, destination)
                        })
                        .map_err(|_| {
                            cs705_core::csv::Error::Operation(
                                "Python GPS NLS callback failed".into(),
                            )
                        })
                    });
                (errors.into_inner(), result)
            });
            errors.finish(
                py,
                result
                    .map(TextEffect::from_core)
                    .map_err(crate::errors::csv),
            )
        })
    }
}
#[pyfunction]
fn symbol_fields() -> Vec<SymbolField> {
    core::symbol_fields()
        .iter()
        .cloned()
        .map(SymbolField::from_core)
        .collect()
}
#[pyfunction]
fn find_symbol(name_or_id: &str) -> PyResult<SymbolField> {
    core::find_symbol(name_or_id)
        .cloned()
        .map(SymbolField::from_core)
        .map_err(crate::errors::icf)
}

#[pymethods]
impl CoordinateField {
    fn __copy__(&self) -> PyResult<Self> {
        self.inner.with_ref(|v| Ok(Self::from_core(v.clone())))
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.__copy__()
    }
    #[getter]
    fn feature_id(&self) -> PyResult<String> {
        self.inner.with_ref(|v| Ok(v.feature_id.to_string()))
    }
    #[setter]
    fn set_feature_id(&self, value: &str) -> PyResult<()> {
        self.inner.with_mut(|v| {
            v.feature_id = value.to_owned().into();
            Ok(())
        })
    }
    #[getter]
    fn name(&self) -> PyResult<String> {
        self.inner.with_ref(|v| Ok(v.name.to_string()))
    }
    #[setter]
    fn set_name(&self, value: &str) -> PyResult<()> {
        self.inner.with_mut(|v| {
            v.name = value.to_owned().into();
            Ok(())
        })
    }
    #[getter]
    fn key(&self) -> PyResult<u16> {
        self.inner.with_ref(|v| Ok(v.key))
    }
    #[setter]
    fn set_key(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u16(value)?;
        self.inner.with_mut(|v| {
            v.key = value;
            Ok(())
        })
    }
    #[getter]
    fn longitude(&self) -> PyResult<bool> {
        self.inner.with_ref(|v| Ok(v.longitude))
    }
    #[setter]
    fn set_longitude(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.inner.with_mut(|v| {
            v.longitude = value;
            Ok(())
        })
    }
    #[getter]
    fn area(&self) -> PyResult<bool> {
        self.inner.with_ref(|v| Ok(v.area))
    }
    #[setter]
    fn set_area(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.inner.with_mut(|v| {
            v.area = value;
            Ok(())
        })
    }
    fn read<'py>(&self, py: Python<'py>, image: &Icf) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = self
            .inner
            .with_ref(|field| image.with_ref(|image| Ok(field.read(image))))?;
        Ok(PyBytes::new(py, &bytes))
    }
    #[pyo3(signature=(image,current,first))]
    fn dialog(
        &self,
        image: &Icf,
        current: &Bound<'_, PyAny>,
        first: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<crate::csv::number::CoordinateDialog> {
        let current = strict_bytes(current)?;
        let first = first.map(strict_u8).transpose()?;
        self.inner.with_ref(|field| {
            image.with_ref(|image| {
                field
                    .dialog(image, current, first)
                    .map(crate::csv::number::CoordinateDialog::from_core)
                    .map_err(crate::errors::csv)
            })
        })
    }
    fn phase2(
        &self,
        image: &Icf,
        source: &Bound<'_, PyAny>,
        current: &Bound<'_, PyAny>,
    ) -> PyResult<CoordinateEffect> {
        let (source, current) = (strict_bytes(source)?, strict_bytes(current)?);
        self.inner.with_ref(|field| {
            image.with_mut(|image| {
                field
                    .phase2(image, source, current)
                    .map(CoordinateEffect::from_core)
                    .map_err(crate::errors::csv)
            })
        })
    }
    fn apply_dialog(
        &self,
        py: Python<'_>,
        image: &Icf,
        dialog: &crate::csv::number::CoordinateDialog,
        event: &Bound<'_, PyAny>,
        current: &Bound<'_, PyAny>,
    ) -> PyResult<CoordinateEffect> {
        let (event, current) = (
            crate::csv::number::coordinate_event(event)?,
            strict_bytes(current)?,
        );
        self.inner.with_ref(|field| {
            dialog.with_mut(py, |dialog| {
                image.with_mut(|image| {
                    field
                        .apply_dialog(image, dialog, event, current)
                        .map(CoordinateEffect::from_core)
                        .map_err(crate::errors::csv)
                })
            })
        })
    }
}
#[pyfunction]
fn coordinate_fields() -> Vec<CoordinateField> {
    core::coordinate_fields()
        .iter()
        .cloned()
        .map(CoordinateField::from_core)
        .collect()
}
#[pyfunction]
fn find_coordinate(name_or_id: &str) -> PyResult<CoordinateField> {
    core::find_coordinate(name_or_id)
        .cloned()
        .map(CoordinateField::from_core)
        .map_err(crate::errors::icf)
}

#[pymethods]
impl TextField {
    fn __copy__(&self) -> PyResult<Self> {
        self.inner.with_ref(|v| Ok(Self::from_core(v.clone())))
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.__copy__()
    }
    #[getter]
    fn feature_id(&self) -> PyResult<String> {
        self.inner.with_ref(|v| Ok(v.feature_id.to_string()))
    }
    #[setter]
    fn set_feature_id(&self, value: &str) -> PyResult<()> {
        self.inner.with_mut(|v| {
            v.feature_id = value.to_owned().into();
            Ok(())
        })
    }
    #[getter]
    fn name(&self) -> PyResult<String> {
        self.inner.with_ref(|v| Ok(v.name.to_string()))
    }
    #[setter]
    fn set_name(&self, value: &str) -> PyResult<()> {
        self.inner.with_mut(|v| {
            v.name = value.to_owned().into();
            Ok(())
        })
    }
    #[getter]
    fn key(&self) -> PyResult<u16> {
        self.inner.with_ref(|v| Ok(v.key))
    }
    #[setter]
    fn set_key(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u16(value)?;
        self.inner.with_mut(|v| {
            v.key = value;
            Ok(())
        })
    }
    #[getter]
    fn count(&self) -> PyResult<usize> {
        self.inner.with_ref(|v| Ok(v.count))
    }
    #[setter]
    fn set_count(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_usize(value)?;
        self.inner.with_mut(|v| {
            v.count = value;
            Ok(())
        })
    }
    #[getter]
    fn width(&self) -> PyResult<usize> {
        self.inner.with_ref(|v| Ok(v.width))
    }
    #[setter]
    fn set_width(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_usize(value)?;
        self.inner.with_mut(|v| {
            v.width = value;
            Ok(())
        })
    }
    fn read<'py>(
        &self,
        py: Python<'py>,
        image: &Icf,
        index: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let index = strict_usize(index)?;
        let bytes = self.inner.with_ref(|field| {
            image.with_ref(|image| field.read(image, index).map_err(crate::errors::icf))
        })?;
        Ok(PyBytes::new(py, &bytes))
    }
    fn budget(&self, image: &Icf) -> PyResult<usize> {
        self.inner
            .with_ref(|field| image.with_ref(|image| Ok(field.budget(image))))
    }
    fn edit(
        &self,
        py: Python<'_>,
        image: &Icf,
        index: &Bound<'_, PyAny>,
        input: &FixedTextInput,
    ) -> PyResult<TextEffect> {
        let index = strict_usize(index)?;
        let field = self.inner.take()?;
        let mut image = image.take()?;
        input.with_core(py, |input| {
            py.detach(|| {
                field
                    .edit(&mut image, index, input)
                    .map(TextEffect::from_core)
                    .map_err(crate::errors::csv)
            })
        })
    }
    fn edit_with_nls(
        &self,
        py: Python<'_>,
        image: &Icf,
        index: &Bound<'_, PyAny>,
        input: &FixedTextInput,
        map: &Bound<'_, PyAny>,
    ) -> PyResult<TextEffect> {
        let index = strict_usize(index)?;
        let map = callable(map)?;
        let interpreter = crate::scoped::interpreter(py)?;
        let field = self.inner.take()?;
        let mut image = image.take()?;
        input.with_core(py, |input| {
            let (errors, result) = py.detach(move || {
                let errors = RefCell::new(CallbackError::default());
                let result =
                    field.edit_with_nls(&mut image, index, input, |source, destination| {
                        invoke(interpreter, &errors, |py| {
                            crate::csv::nls::map_call(py, map.bind(py), source, destination)
                        })
                        .map_err(|_| {
                            cs705_core::csv::Error::Operation(
                                "Python GPS NLS callback failed".into(),
                            )
                        })
                    });
                (errors.into_inner(), result)
            });
            errors.finish(
                py,
                result
                    .map(TextEffect::from_core)
                    .map_err(crate::errors::csv),
            )
        })
    }
}
#[pyfunction]
fn text_fields() -> Vec<TextField> {
    core::text_fields()
        .iter()
        .cloned()
        .map(TextField::from_core)
        .collect()
}
#[pyfunction]
fn find_text(name_or_id: &str) -> PyResult<TextField> {
    core::find_text(name_or_id)
        .cloned()
        .map(TextField::from_core)
        .map_err(crate::errors::icf)
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "gps")?;
    crate::presentation::add_enum(&m, MODULE, "AlarmField", &["Type", "Group"])?;
    crate::presentation::add_enum(&m, MODULE, "SentenceField", &["Nmea", "Logger"])?;
    m.add_class::<Resources>()?;
    m.add_class::<CoordinateEffect>()?;
    m.add_class::<SentenceEffect>()?;
    m.add_class::<SentenceEvent>()?;
    m.add_class::<SentenceDialog>()?;
    m.add_class::<NativeSymbolPairs>()?;
    m.add_class::<ChoiceField>()?;
    m.add_class::<NumberField>()?;
    m.add_class::<SymbolField>()?;
    m.add_class::<CoordinateField>()?;
    m.add_class::<TextField>()?;
    m.add(
        "ALARM_AREA",
        CoordinateField::from_core(core::ALARM_AREA.clone()),
    )?;
    m.add_function(wrap_pyfunction!(choice_fields, &m)?)?;
    m.add_function(wrap_pyfunction!(find_choice, &m)?)?;
    m.add_function(wrap_pyfunction!(number_fields, &m)?)?;
    m.add_function(wrap_pyfunction!(symbol_fields, &m)?)?;
    m.add_function(wrap_pyfunction!(find_symbol, &m)?)?;
    m.add_function(wrap_pyfunction!(coordinate_fields, &m)?)?;
    m.add_function(wrap_pyfunction!(find_coordinate, &m)?)?;
    m.add_function(wrap_pyfunction!(text_fields, &m)?)?;
    m.add_function(wrap_pyfunction!(find_text, &m)?)?;
    m.add_function(wrap_pyfunction!(alarm_native_name, &m)?)?;
    m.add_function(wrap_pyfunction!(alarm_feature_id, &m)?)?;
    m.add_function(wrap_pyfunction!(alarm_key, &m)?)?;
    m.add_function(wrap_pyfunction!(alarm_read, &m)?)?;
    m.add_function(wrap_pyfunction!(alarm_choices, &m)?)?;
    m.add_function(wrap_pyfunction!(alarm_edit, &m)?)?;
    m.add_function(wrap_pyfunction!(sentence_native_name, &m)?)?;
    m.add_function(wrap_pyfunction!(sentence_feature_id, &m)?)?;
    m.add_function(wrap_pyfunction!(sentence_key, &m)?)?;
    m.add_function(wrap_pyfunction!(sentence_count, &m)?)?;
    m.add_function(wrap_pyfunction!(sentence_paint, &m)?)?;
    m.add_function(wrap_pyfunction!(find_sentence, &m)?)?;
    m.add_function(wrap_pyfunction!(paint_alarm_group, &m)?)?;
    m.add_function(wrap_pyfunction!(gps_group_count_source, &m)?)?;
    m.add_function(wrap_pyfunction!(paint_altitude_raw, &m)?)?;
    m.add_function(wrap_pyfunction!(edit_altitude_live, &m)?)?;
    m.add_function(wrap_pyfunction!(edit_altitude_raw, &m)?)?;
    parent.add_submodule(&m)
}
