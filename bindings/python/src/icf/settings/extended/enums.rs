//! Native unit enums remain enum.Enum; methods call typed native operations.
use super::*;
use crate::icf::Icf;
use pyo3::types::PyTuple;

pub(super) fn method(
    m: &Bound<'_, PyModule>,
    class: &str,
    name: &str,
    call: Bound<'_, PyAny>,
) -> PyResult<()> {
    let globals = PyDict::new(m.py());
    globals.set_item("_call", &call)?;
    m.py().run(
        &std::ffi::CString::new(format!(
            "def {name}(self, *args, **kwargs):\n    return _call(self, *args, **kwargs)\n"
        ))
        .unwrap(),
        Some(&globals),
        Some(&globals),
    )?;
    let method = globals.get_item(name)?.unwrap();
    m.py()
        .import("functools")?
        .getattr("update_wrapper")?
        .call1((&method, &call))?;
    m.getattr(class)?.setattr(name, method)
}
macro_rules! unit {
    ($name:ident; $($variant:ident),*)=>{
        impl Input<'_> for core::$name{fn input(v:&Bound<'_,PyAny>)->PyResult<Self>{let code=crate::icf::enum_value(v,"cs705.icf.settings.extended",stringify!($name))?;let values=[$(Self::$variant),*];values.get(usize::try_from(code).unwrap_or(usize::MAX)).copied().ok_or_else(||pyo3::exceptions::PyValueError::new_err(concat!("invalid ",stringify!($name))))}}
        impl Output for core::$name{fn output<'py>(self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{crate::icf::enum_result(py,"cs705.icf.settings.extended",stringify!($name),self as i64)}}
        fn register_enum(m:&Bound<'_,PyModule>)->PyResult<()>{crate::presentation::add_enum(m,"cs705.icf.settings.extended",stringify!($name),&[$(stringify!($variant)),*])}
    }
}
macro_rules! metadata {
    ($name:ident) => {
        #[pyfunction]
        fn feature_id(field: &Bound<'_, PyAny>) -> PyResult<String> {
            Ok(core::$name::input(field)?.feature_id().to_string())
        }
        #[pyfunction]
        fn key(field: &Bound<'_, PyAny>) -> PyResult<u16> {
            Ok(core::$name::input(field)?.key())
        }
        fn register_metadata(m: &Bound<'_, PyModule>) -> PyResult<()> {
            method(
                m,
                stringify!($name),
                "feature_id",
                wrap_pyfunction!(feature_id, m)?.into_any(),
            )?;
            method(
                m,
                stringify!($name),
                "key",
                wrap_pyfunction!(key, m)?.into_any(),
            )?;
            let values = core::$name::ALL
                .into_iter()
                .map(|v| m.getattr(stringify!($name))?.call1((v as i64,)))
                .collect::<PyResult<Vec<_>>>()?;
            m.getattr(stringify!($name))?
                .setattr("ALL", PyTuple::new(m.py(), values)?)?;
            Ok(())
        }
    };
}
mod color {
    use super::*;
    unit!(ColorField;ScopeCurrent,ScopeOutline,ScopeMaxHold,AudioFft,AudioOsc,RttyFft,RttyReceive,RttyTransmit);
    metadata!(ColorField);
    #[pyfunction]
    #[pyo3(signature=(image,field,choice))]
    fn edit_color(
        image: &Icf,
        field: &Bound<'_, PyAny>,
        choice: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<super::super::editors::ColorEffect> {
        let field = core::ColorField::input(field)?;
        let choice = choice.map(<[u8; 3] as Input>::input).transpose()?;
        image.with_mut(|image| {
            core::edit_color(image, field, choice)
                .map(super::super::editors::ColorEffect::from_core)
                .map_err(crate::errors::icf)
        })
    }
    pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
        register_enum(m)?;
        register_metadata(m)?;
        m.add_function(wrap_pyfunction!(edit_color, m)?)
    }
}
mod packed {
    use super::*;
    unit!(PackedField;Vox,OneHz,SelectMemoryScan,DeltaFSpan,ScopeMode);
    metadata!(PackedField);
    #[pyfunction]
    fn read(field: &Bound<'_, PyAny>, image: &Icf) -> PyResult<i64> {
        let field = core::PackedField::input(field)?;
        image.with_ref(|image| field.read(image).map_err(crate::errors::icf))
    }
    #[pyfunction]
    fn selector(
        field: &Bound<'_, PyAny>,
        system_langid: &Bound<'_, PyAny>,
    ) -> PyResult<Option<&'static str>> {
        Ok(core::PackedField::input(field)?.selector(strict_u16(system_langid)?))
    }
    #[pyfunction]
    fn edit(
        py: Python<'_>,
        field: &Bound<'_, PyAny>,
        image: &Icf,
        input: &ChoiceInput,
    ) -> PyResult<ChoiceEffect> {
        let field = core::PackedField::input(field)?;
        input.with_core(py, |input| {
            image.with_mut(|image| {
                field
                    .edit(image, input)
                    .map(ChoiceEffect::from_core)
                    .map_err(crate::errors::icf)
            })
        })
    }
    pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
        register_enum(m)?;
        register_metadata(m)?;
        method(
            m,
            "PackedField",
            "read",
            wrap_pyfunction!(read, m)?.into_any(),
        )?;
        method(
            m,
            "PackedField",
            "selector",
            wrap_pyfunction!(selector, m)?.into_any(),
        )?;
        method(
            m,
            "PackedField",
            "edit",
            wrap_pyfunction!(edit, m)?.into_any(),
        )
    }
}
mod amplifier {
    use super::*;
    unit!(AmplifierField;Preamp,Attenuator);
    metadata!(AmplifierField);
    #[pyfunction]
    fn edit(
        py: Python<'_>,
        field: &Bound<'_, PyAny>,
        image: &Icf,
        index: &Bound<'_, PyAny>,
        input: &ChoiceInput,
    ) -> PyResult<ChoiceEffect> {
        let field = core::AmplifierField::input(field)?;
        let index = strict_usize(index)?;
        input.with_core(py, |input| {
            image.with_mut(|image| {
                field
                    .edit(image, index, input)
                    .map(ChoiceEffect::from_core)
                    .map_err(crate::errors::icf)
            })
        })
    }
    pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
        register_enum(m)?;
        register_metadata(m)?;
        method(
            m,
            "AmplifierField",
            "edit",
            wrap_pyfunction!(edit, m)?.into_any(),
        )
    }
}
mod fixed_text {
    use super::*;
    unit!(FixedTextField;GatewayAddress,GatewayCallsign,NtpAddress,Comment,PictureRecipient,NetworkName,Ssid,ApPassword,NetworkRadioName,Keyer,User1Id,User1Password,User2Id,User2Password);
    metadata!(FixedTextField);
    #[pyfunction]
    fn width(field: &Bound<'_, PyAny>) -> PyResult<usize> {
        Ok(core::FixedTextField::input(field)?.width())
    }
    #[pyfunction]
    fn count(field: &Bound<'_, PyAny>) -> PyResult<usize> {
        Ok(core::FixedTextField::input(field)?.count())
    }
    #[pyfunction]
    fn raw<'py>(
        py: Python<'py>,
        field: &Bound<'_, PyAny>,
        image: &Icf,
        index: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let field = core::FixedTextField::input(field)?;
        let index = strict_usize(index)?;
        image.with_ref(|image| {
            Ok(PyBytes::new(
                py,
                field.raw(image, index).map_err(crate::errors::csv)?,
            ))
        })
    }
    #[pyfunction]
    fn store_encoded_text(
        field: &Bound<'_, PyAny>,
        image: &Icf,
        index: &Bound<'_, PyAny>,
        bytes: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let field = core::FixedTextField::input(field)?;
        let index = strict_usize(index)?;
        let bytes = strict_bytes(bytes)?;
        image.with_mut(|image| {
            field
                .store_encoded_text(image, index, bytes)
                .map_err(crate::errors::csv)
        })
    }
    pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
        register_enum(m)?;
        register_metadata(m)?;
        method(
            m,
            "FixedTextField",
            "width",
            wrap_pyfunction!(width, m)?.into_any(),
        )?;
        method(
            m,
            "FixedTextField",
            "count",
            wrap_pyfunction!(count, m)?.into_any(),
        )?;
        method(
            m,
            "FixedTextField",
            "raw",
            wrap_pyfunction!(raw, m)?.into_any(),
        )?;
        method(
            m,
            "FixedTextField",
            "store_encoded_text",
            wrap_pyfunction!(store_encoded_text, m)?.into_any(),
        )
    }
}
mod metadata_field {
    use super::*;
    unit!(MetadataField;TransmitPicture,ConnectionIpList);
    metadata!(MetadataField);
    #[pyfunction]
    fn clear_command(field: &Bound<'_, PyAny>) -> PyResult<u32> {
        Ok(core::MetadataField::input(field)?.clear_command())
    }
    #[pyfunction]
    fn read(
        field: &Bound<'_, PyAny>,
        image: &Icf,
    ) -> PyResult<super::super::editors::MetadataView> {
        let field = core::MetadataField::input(field)?;
        image.with_ref(|image| {
            Ok(super::super::editors::MetadataView::from_core(
                field.read(image),
            ))
        })
    }
    #[pyfunction]
    fn clear(
        field: &Bound<'_, PyAny>,
        image: &Icf,
        command: &Bound<'_, PyAny>,
    ) -> PyResult<crate::icf::settings::Effect> {
        let field = core::MetadataField::input(field)?;
        let command = strict_u32(command)?;
        image.with_mut(|image| {
            field
                .clear(image, command)
                .map(crate::icf::settings::Effect::from_core)
                .map_err(crate::errors::icf)
        })
    }
    pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
        register_enum(m)?;
        register_metadata(m)?;
        method(
            m,
            "MetadataField",
            "clear_command",
            wrap_pyfunction!(clear_command, m)?.into_any(),
        )?;
        method(
            m,
            "MetadataField",
            "read",
            wrap_pyfunction!(read, m)?.into_any(),
        )?;
        method(
            m,
            "MetadataField",
            "clear",
            wrap_pyfunction!(clear, m)?.into_any(),
        )
    }
}
mod conditional {
    use super::*;
    unit!(ConditionalField;Coordinates,Temperature,AutoRepeater,ScopeSpan);
    metadata!(ConditionalField);
    pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
        register_enum(m)?;
        register_metadata(m)
    }
}
mod dr {
    use super::*;
    unit!(DrField;DvFilter,FmFilter,DigitalSquelch,DigitalCode);
    metadata!(DrField);
    #[pyfunction]
    fn read(field: &Bound<'_, PyAny>, image: &Icf) -> PyResult<u8> {
        let field = core::DrField::input(field)?;
        image.with_ref(|image| Ok(field.read(image)))
    }
    pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
        register_enum(m)?;
        register_metadata(m)?;
        method(m, "DrField", "read", wrap_pyfunction!(read, m)?.into_any())
    }
}
mod rf {
    use super::*;
    unit!(RfLayout;Reference,FixedEdges,PreampAttenuator,AirTuningStep);
    pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
        register_enum(m)
    }
}
mod edge {
    use super::*;
    unit!(EdgeSide;Low,High);
    pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
        register_enum(m)
    }
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    method(
        m,
        "FixedValue",
        "source_label",
        wrap_pyfunction!(source_label, m)?.into_any(),
    )?;
    color::register(m)?;
    packed::register(m)?;
    amplifier::register(m)?;
    fixed_text::register(m)?;
    metadata_field::register(m)?;
    conditional::register(m)?;
    dr::register(m)?;
    rf::register(m)?;
    edge::register(m)?;
    Ok(())
}
#[pyfunction]
fn source_label(value: &Bound<'_, PyAny>) -> PyResult<&'static str> {
    Ok(core::FixedValue::input(value)?.source_label())
}
