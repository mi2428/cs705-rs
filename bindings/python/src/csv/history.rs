//! Native receive-history logical output and display profiles.
use super::*;
use cs705_core::csv::history as h;

#[pyfunction(name = "format")]
fn history_format<'py>(
    py: Python<'py>,
    request: &super::payload::Request,
    context: &super::datetime::Context,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &context.with_context(|c| h::format(request.native(), c).map_err(crate::errors::csv))?,
    ))
}
#[pyfunction]
fn format_with_date<'py>(
    py: Python<'py>,
    request: &super::payload::Request,
    context: &super::datetime::Context,
    native: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let mut errors = crate::callbacks::CallbackError::default();
    let result = context.with_context(|c| {
        h::format_with_date(request.native(), c, |inner| {
            callback(
                &mut errors,
                native
                    .call1((super::datetime::DateToken::from_core(inner),))
                    .and_then(|v| Ok(strict_bytes(&v)?.to_vec())),
            )
        })
        .map_err(crate::errors::csv)
    });
    errors.finish(py, result).map(|v| PyBytes::new(py, &v))
}
#[pyfunction]
fn symbol<'py>(
    py: Python<'py>,
    raw: &Bound<'_, PyAny>,
    compact: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &h::symbol(strict_bytes(raw)?, strict_bool(compact)?).map_err(crate::errors::csv)?,
    ))
}
#[pyfunction]
fn symbol_from_source<'py>(
    py: Python<'py>,
    raw: &Bound<'_, PyAny>,
    source: &Bound<'_, PyAny>,
    locale: &Bound<'_, PyAny>,
    compact: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &h::symbol_from_source(
            strict_bytes(raw)?,
            strict_bytes(source)?,
            strict_bool(locale)?,
            strict_bool(compact)?,
        )
        .map_err(crate::errors::csv)?,
    ))
}
#[pyclass(module = "cs705.csv.history.paint")]
pub struct Context {
    inner: h::paint::Context,
}
#[pymethods]
impl Context {
    #[setter]
    fn set_date_order(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.inner.date_order = value.map(strict_u32).transpose()?;
        Ok(())
    }
    #[setter]
    fn set_gui_language(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.inner.gui_language = value.map(strict_u32).transpose()?;
        Ok(())
    }
    #[setter]
    fn set_system_langid(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.inner.system_langid = value.map(strict_u16).transpose()?;
        Ok(())
    }
    #[setter]
    fn set_time_separators(&mut self, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.inner.time_separators = value.map(fixed_bytes).transpose()?;
        Ok(())
    }
    #[new]
    fn new(
        date_order: Option<&Bound<'_, PyAny>>,
        gui_language: Option<&Bound<'_, PyAny>>,
        system_langid: Option<&Bound<'_, PyAny>>,
        time_separators: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: h::paint::Context {
                date_order: date_order.map(strict_u32).transpose()?,
                gui_language: gui_language.map(strict_u32).transpose()?,
                system_langid: system_langid.map(strict_u16).transpose()?,
                time_separators: time_separators.map(fixed_bytes).transpose()?,
            },
        })
    }
    #[getter]
    fn date_order(&self) -> Option<u32> {
        self.inner.date_order
    }
    #[getter]
    fn gui_language(&self) -> Option<u32> {
        self.inner.gui_language
    }
    #[getter]
    fn system_langid(&self) -> Option<u16> {
        self.inner.system_langid
    }
    #[getter]
    fn time_separators<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.inner.time_separators.map(|v| PyBytes::new(py, &v))
    }
}
#[pyfunction]
fn display_text<'py>(
    py: Python<'py>,
    logical: &Bound<'_, PyAny>,
    context: &Context,
) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(
        py,
        &h::paint::display_text(strict_bytes(logical)?, &context.inner)
            .map_err(crate::errors::csv)?,
    ))
}
#[pyfunction]
fn date_mask<'py>(py: Python<'py>, order: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    Ok(PyBytes::new(py, h::paint::date_mask(strict_u32(order)?)))
}
#[pyfunction(name = "format")]
fn paint_format<'py>(
    py: Python<'py>,
    request: &super::payload::Request,
    context: &Context,
    clock: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let mut errors = crate::callbacks::CallbackError::default();
    let result = h::paint::format(request.native(), &context.inner, || {
        callback(
            &mut errors,
            (|| {
                let value = clock.call0()?;
                let (d, t): (
                    PyRef<'_, super::datetime::Date>,
                    PyRef<'_, super::datetime::Time>,
                ) = value.extract()?;
                Ok((d.native()?, t.inner))
            })(),
        )
    });
    errors
        .finish(py, result.map_err(crate::errors::csv))
        .map(|v| PyBytes::new(py, &v))
}
#[pyfunction]
fn format_field<'py>(
    py: Python<'py>,
    key: &Bound<'_, PyAny>,
    source: &Bound<'_, PyAny>,
    current: &Bound<'_, PyAny>,
    context: &Context,
    source_pe: Option<&Bound<'_, PyAny>>,
    resource: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let mut errors = crate::callbacks::CallbackError::default();
    let result = h::paint::format_field(
        strict_u32(key)?,
        strict_bytes(source)?,
        strict_bytes(current)?,
        &context.inner,
        source_pe.map(strict_bytes).transpose()?,
        |name, index| {
            callback(
                &mut errors,
                resource
                    .call1((PyBytes::new(py, name), index))
                    .and_then(|v| Ok(strict_bytes(&v)?.to_vec())),
            )
        },
    );
    errors
        .finish(py, result.map_err(crate::errors::csv))
        .map(|v| PyBytes::new(py, &v))
}
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "history")?;
    m.add_function(wrap_pyfunction!(history_format, &m)?)?;
    m.add_function(wrap_pyfunction!(format_with_date, &m)?)?;
    m.add_function(wrap_pyfunction!(symbol, &m)?)?;
    m.add_function(wrap_pyfunction!(symbol_from_source, &m)?)?;
    let paint = PyModule::new(root.py(), "paint")?;
    paint.add_class::<Context>()?;
    paint.add_function(wrap_pyfunction!(display_text, &paint)?)?;
    paint.add_function(wrap_pyfunction!(date_mask, &paint)?)?;
    paint.add_function(wrap_pyfunction!(paint_format, &paint)?)?;
    paint.add_function(wrap_pyfunction!(format_field, &paint)?)?;
    m.add_submodule(&paint)?;
    root.add_submodule(&m)
}
