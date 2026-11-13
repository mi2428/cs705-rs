//! Direct native editor calls; descriptors and input buffers stay canonical.
use super::super::super::Icf;
use super::*;

#[pyfunction]
fn paint_level<'py>(py: Python<'py>, raw: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(py, &core::paint_level(strict_u8(raw)?)))
}
#[pyfunction]
fn paint_hex_byte<'py>(py: Python<'py>, raw: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(py, &core::paint_hex_byte(strict_u8(raw)?)))
}
#[pyfunction]
#[pyo3(signature=(image,key,index,seconds_unit,system_langid))]
fn paint_number_at<'py>(
    py: Python<'py>,
    image: &Icf,
    key: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    seconds_unit: Option<&Bound<'_, PyAny>>,
    system_langid: Option<&Bound<'_, PyAny>>,
) -> PyResult<Bound<'py, PyBytes>> {
    let key = strict_u16(key)?;
    let index = strict_usize(index)?;
    let unit = seconds_unit.map(strict_bytes).transpose()?;
    let system = system_langid.map(strict_u16).transpose()?;
    let text = image.with_ref(|image| {
        core::paint_number_at(image, key, index, unit, system).map_err(crate::errors::icf)
    })?;
    Ok(PyBytes::new(py, &text))
}
#[pyfunction]
fn edit_number(
    py: Python<'_>,
    image: &Icf,
    name: &str,
    input: &NumberInput,
    crt_point: &Bound<'_, PyAny>,
) -> PyResult<NumberEffect> {
    let point = strict_u8(crt_point)?;
    input.with_core(py, |input| {
        image.with_mut(|image| {
            core::edit_number(image, name, input, point)
                .map(NumberEffect::from_core)
                .map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
fn edit_number_at(
    py: Python<'_>,
    image: &Icf,
    name: &str,
    index: &Bound<'_, PyAny>,
    input: &NumberInput,
    crt_point: &Bound<'_, PyAny>,
) -> PyResult<NumberEffect> {
    let point = strict_u8(crt_point)?;
    let index = strict_usize(index)?;
    input.with_core(py, |input| {
        image.with_mut(|image| {
            core::edit_number_at(image, name, index, input, point)
                .map(NumberEffect::from_core)
                .map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
fn utc_offset_text<'py>(
    py: Python<'py>,
    minutes: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &core::utc_offset_text(strict_i16(minutes)?),
    ))
}
#[pyfunction]
fn edit_utc_offset(py: Python<'_>, image: &Icf, input: &NumberInput) -> PyResult<NumberEffect> {
    input.with_core(py, |input| {
        image.with_mut(|image| {
            core::edit_utc_offset(image, input)
                .map(NumberEffect::from_core)
                .map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
fn edit_pbt(
    py: Python<'_>,
    image: &Icf,
    index: &Bound<'_, PyAny>,
    side: &Bound<'_, PyAny>,
    input: &NumberInput,
    crt_point: &Bound<'_, PyAny>,
) -> PyResult<NumberEffect> {
    let index = strict_usize(index)?;
    let point = strict_u8(crt_point)?;
    let side = match crate::icf::enum_value(side, "cs705.icf", "PbtSide")? {
        0 => cs705_core::icf::PbtSide::One,
        1 => cs705_core::icf::PbtSide::Two,
        _ => return Err(pyo3::exceptions::PyValueError::new_err("invalid PbtSide")),
    };
    input.with_core(py, |input| {
        image.with_mut(|image| {
            core::edit_pbt(image, index, side, input, point)
                .map(NumberEffect::from_core)
                .map_err(crate::errors::icf)
        })
    })
}
effects!(FilterView { bandwidth_hz:i32=>set_bandwidth_hz;pbt_hz:[i32;2]=>set_pbt_hz;passband_hz:i32=>set_passband_hz;shift_hz:i32=>set_shift_hz; });
effects!(ColorEffect { stored:bool=>set_stored;value:[u8;3]=>set_value;notify_flags:u32=>set_notify_flags; });
effects!(MetadataView { attached:bool=>set_attached;name:Vec<u8> =>set_name;stem:Vec<u8> =>set_stem;read_only:bool=>set_read_only; });
#[pyfunction]
fn filter_view(image: &Icf, index: &Bound<'_, PyAny>) -> PyResult<FilterView> {
    let index = strict_usize(index)?;
    image.with_ref(|image| {
        core::filter_view(image, index)
            .map(FilterView::from_core)
            .map_err(crate::errors::icf)
    })
}
#[pyfunction]
fn edit_dtmf(
    py: Python<'_>,
    image: &Icf,
    channel: &Bound<'_, PyAny>,
    input: &TextInput,
) -> PyResult<TextEffect> {
    let channel = strict_usize(channel)?;
    input.with_core(py, |input| {
        image.with_mut(|image| {
            core::edit_dtmf(image, channel, input)
                .map(TextEffect::from_core)
                .map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
fn edit_rtty(
    py: Python<'_>,
    image: &Icf,
    channel: &Bound<'_, PyAny>,
    input: &TextInput,
) -> PyResult<TextEffect> {
    let channel = strict_usize(channel)?;
    input.with_core(py, |input| {
        image.with_mut(|image| {
            core::edit_rtty(image, channel, input)
                .map(TextEffect::from_core)
                .map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
fn align_callsign(bytes: &MutableBytes) -> PyResult<()> {
    let mut bytes = bytes.take()?;
    let bytes: &mut [u8; 8] = bytes
        .as_mut_slice()
        .try_into()
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("expected exactly 8 bytes"))?;
    core::align_callsign(bytes);
    Ok(())
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(edit_fixed_text, m)?)?;
    m.add_function(wrap_pyfunction!(edit_fixed_text_with_nls, m)?)?;
    m.add_class::<FilterView>()?;
    m.add_class::<ColorEffect>()?;
    m.add_class::<MetadataView>()?;
    m.add_function(wrap_pyfunction!(paint_level, m)?)?;
    m.add_function(wrap_pyfunction!(paint_hex_byte, m)?)?;
    m.add_function(wrap_pyfunction!(paint_number_at, m)?)?;
    m.add_function(wrap_pyfunction!(edit_number, m)?)?;
    m.add_function(wrap_pyfunction!(edit_number_at, m)?)?;
    m.add_function(wrap_pyfunction!(utc_offset_text, m)?)?;
    m.add_function(wrap_pyfunction!(edit_utc_offset, m)?)?;
    m.add_function(wrap_pyfunction!(edit_pbt, m)?)?;
    m.add_function(wrap_pyfunction!(filter_view, m)?)?;
    m.add_function(wrap_pyfunction!(edit_dtmf, m)?)?;
    m.add_function(wrap_pyfunction!(edit_rtty, m)?)?;
    m.add_function(wrap_pyfunction!(align_callsign, m)?)?;
    Ok(())
}

#[pyfunction]
fn edit_fixed_text(
    py: Python<'_>,
    image: Py<Icf>,
    field: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    input: &FixedTextInput,
    alignment_option: &Bound<'_, PyAny>,
) -> PyResult<FixedTextEffect> {
    let field = core::FixedTextField::input(field)?;
    let index = strict_usize(index)?;
    let alignment = strict_u32(alignment_option)?;
    let mut image = image.get().take()?;
    let result = input.with_core(py, |input| {
        py.detach(move || {
            core::edit_fixed_text(&mut image, field, index, input, alignment)
                .map_err(crate::errors::csv)
        })
    })?;
    FixedTextEffect::from_core(py, result)
}
#[pyfunction]
#[expect(
    clippy::too_many_arguments,
    reason = "Fixed-text editing keeps the native field, index, phase input, alignment and both NLS callbacks explicit."
)]
fn edit_fixed_text_with_nls(
    py: Python<'_>,
    image: Py<Icf>,
    field: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
    input: &FixedTextInput,
    alignment_option: &Bound<'_, PyAny>,
    map: Py<PyAny>,
    upper: Py<PyAny>,
) -> PyResult<FixedTextEffect> {
    use crate::{callbacks::CallbackError, icf::document::adapters::attach_checked};
    use std::cell::RefCell;
    let field = core::FixedTextField::input(field)?;
    let index = strict_usize(index)?;
    let alignment = strict_u32(alignment_option)?;
    let mut image = image.get().take()?;
    let interpreter = crate::scoped::interpreter(py)?;
    let (errors, result) = input.with_core(py, |input| {
        Ok(py.detach(move || {
            let errors = RefCell::new(CallbackError::default());
            let result = core::edit_fixed_text_with_nls(
                &mut image,
                field,
                index,
                input,
                alignment,
                |source, destination| {
                    let result = attach_checked(interpreter, |py| {
                        crate::csv::nls::map_call(py, map.bind(py), source, destination)
                    });
                    errors.borrow_mut().capture(result).map_err(|_| {
                        cs705_core::csv::Error::Operation("Python extended text map failed".into())
                    })
                },
                |destination| {
                    let result = attach_checked(interpreter, |py| {
                        crate::csv::nls::upper_call(py, upper.bind(py), destination)
                    });
                    errors.borrow_mut().capture(result).map_err(|_| {
                        cs705_core::csv::Error::Operation(
                            "Python extended text uppercase failed".into(),
                        )
                    })
                },
            );
            drop(image);
            (errors.into_inner(), result)
        }))
    })?;
    let value = errors.finish(py, result.map_err(crate::errors::csv))?;
    FixedTextEffect::from_core(py, value)
}
