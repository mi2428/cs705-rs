//! GPS operations over the package's one owning ICF type.
use super::{Event, Outcome, Record};
use crate::icf::document::adapters::attach_checked;
use crate::{callbacks::CallbackError, scoped::Frame};
use crate::{conversion::*, errors, icf::Icf};
use cs705_core::gps::operations as core;
use pyo3::prelude::*;
use std::cell::RefCell;

fn run<R: Send>(
    py: Python<'_>,
    image: &Icf,
    f: impl for<'data> FnOnce(
        &mut cs705_core::icf::Icf<'data>,
        &RefCell<CallbackError>,
        i64,
    ) -> Result<R, cs705_core::icf::Error>
    + Send,
) -> PyResult<R> {
    let interpreter = crate::scoped::interpreter(py)?;
    let (value, error) = image.with_mut(|image| {
        Ok(py.detach(|| {
            let error = RefCell::new(CallbackError::default());
            let value = f(image, &error, interpreter);
            (value, error.into_inner())
        }))
    })?;
    error.finish(py, value.map_err(errors::icf))
}

fn callback<T>(
    error: &RefCell<CallbackError>,
    result: PyResult<T>,
) -> Result<T, cs705_core::icf::Error> {
    error
        .borrow_mut()
        .capture(result)
        .map_err(|_| cs705_core::icf::Error::Operation("GPS operation callback failed".into()))
}
fn call_notify(
    py: Python<'_>,
    image: &mut cs705_core::icf::Icf<'_>,
    event: core::Event,
    call: &Py<PyAny>,
) -> PyResult<()> {
    Frame::with_icf_mut(py, image, |loan| {
        call.bind(py)
            .call1((Icf::from_loan(loan), Event { inner: event }))?;
        Ok(())
    })
}
#[pyfunction]
#[expect(
    clippy::too_many_arguments,
    reason = "Preserves native GPS insert arguments and explicit seed and notification callbacks."
)]
fn insert(
    py: Python<'_>,
    image: &Icf,
    group: &Bound<'_, PyAny>,
    row: &Bound<'_, PyAny>,
    requested: &Bound<'_, PyAny>,
    selection: (Bound<'_, PyAny>, Bound<'_, PyAny>),
    seed: Py<PyAny>,
    notify: Py<PyAny>,
) -> PyResult<Outcome> {
    let (g, r, n, s) = (
        strict_usize(group)?,
        strict_usize(row)?,
        strict_usize(requested)?,
        (strict_usize(&selection.0)?, strict_usize(&selection.1)?),
    );
    run(py, image, |image, errors, interpreter| {
        core::insert(
            image,
            g,
            r,
            n,
            s,
            |image| {
                callback(
                    errors,
                    attach_checked(interpreter, |py| {
                        Frame::with_icf_ref(py, image, |loan| {
                            Ok(seed
                                .bind(py)
                                .call1((Icf::from_loan(loan),))?
                                .extract::<PyRef<'_, Record>>()?
                                .inner)
                        })
                    }),
                )
            },
            |image, event| {
                callback(
                    errors,
                    attach_checked(interpreter, |py| call_notify(py, image, event, &notify)),
                )
            },
        )
    })
    .map(|inner| Outcome { inner })
}
fn clock_value(value: &Bound<'_, PyAny>) -> PyResult<cs705_core::csv::date_input::Clock> {
    let _: Py<crate::csv::date_input::Clock> = value.extract()?;
    Ok(cs705_core::csv::date_input::Clock {
        year: strict_u16(&value.getattr("year")?)?,
        month: strict_u16(&value.getattr("month")?)?,
        day: strict_u16(&value.getattr("day")?)?,
        hour: strict_u16(&value.getattr("hour")?)?,
        minute: strict_u16(&value.getattr("minute")?)?,
        second: strict_u16(&value.getattr("second")?)?,
        millisecond: strict_u16(&value.getattr("millisecond")?)?,
    })
}
#[pyfunction]
#[pyo3(signature=(image,group,row,requested,selection,q,system_langid,clock,notify))]
#[expect(
    clippy::too_many_arguments,
    reason = "Preserves native initialized insertion with explicit Q, language, clock and notification inputs."
)]
fn insert_initialized(
    py: Python<'_>,
    image: &Icf,
    group: &Bound<'_, PyAny>,
    row: &Bound<'_, PyAny>,
    requested: &Bound<'_, PyAny>,
    selection: (Bound<'_, PyAny>, Bound<'_, PyAny>),
    q: Py<crate::icf::context::Context>,
    system_langid: Option<&Bound<'_, PyAny>>,
    clock: Py<PyAny>,
    notify: Py<PyAny>,
) -> PyResult<Outcome> {
    let (g, r, n, s, id) = (
        strict_usize(group)?,
        strict_usize(row)?,
        strict_usize(requested)?,
        (strict_usize(&selection.0)?, strict_usize(&selection.1)?),
        system_langid.map(strict_u16).transpose()?,
    );
    let q = q.get().with_ref(|q| Ok(*q))?;
    run(py, image, |image, errors, interpreter| {
        core::insert_initialized(
            image,
            g,
            r,
            n,
            s,
            &q,
            id,
            || {
                callback(
                    errors,
                    attach_checked(interpreter, |py| {
                        clock.bind(py).call0().and_then(|v| clock_value(&v))
                    }),
                )
            },
            |image, event| {
                callback(
                    errors,
                    attach_checked(interpreter, |py| call_notify(py, image, event, &notify)),
                )
            },
        )
    })
    .map(|inner| Outcome { inner })
}
#[pyfunction]
fn delete(
    py: Python<'_>,
    image: &Icf,
    group: &Bound<'_, PyAny>,
    row: &Bound<'_, PyAny>,
    requested: &Bound<'_, PyAny>,
    notify: Py<PyAny>,
) -> PyResult<usize> {
    let (g, r, n) = (
        strict_usize(group)?,
        strict_usize(row)?,
        strict_usize(requested)?,
    );
    run(py, image, |image, errors, interpreter| {
        core::delete(image, g, r, n, |image, event| {
            callback(
                errors,
                attach_checked(interpreter, |py| call_notify(py, image, event, &notify)),
            )
        })
    })
}
#[pyfunction]
fn carry(
    py: Python<'_>,
    image: &Icf,
    group: &Bound<'_, PyAny>,
    first: &Bound<'_, PyAny>,
    last: &Bound<'_, PyAny>,
    direction: &Bound<'_, PyAny>,
    notify: Py<PyAny>,
) -> PyResult<bool> {
    let (g, f, l, d) = (
        strict_usize(group)?,
        strict_usize(first)?,
        strict_usize(last)?,
        strict_i32(direction)?,
    );
    run(py, image, |image, errors, interpreter| {
        core::carry(image, g, f, l, d, |image, event| {
            callback(
                errors,
                attach_checked(interpreter, |py| call_notify(py, image, event, &notify)),
            )
        })
    })
}

#[pyfunction]
fn first_free_physical(image: &Icf) -> PyResult<Option<usize>> {
    image.with_ref(|i| Ok(core::first_free_physical(i)))
}
#[pyfunction]
fn copy(
    image: &Icf,
    group: &Bound<'_, PyAny>,
    first: &Bound<'_, PyAny>,
    last: &Bound<'_, PyAny>,
) -> PyResult<Vec<Record>> {
    let (group, first, last) = (
        strict_usize(group)?,
        strict_usize(first)?,
        strict_usize(last)?,
    );
    image.with_ref(|i| {
        Ok(core::copy(i, group, first, last)
            .map_err(errors::icf)?
            .into_iter()
            .map(|inner| Record { inner })
            .collect())
    })
}
#[pyfunction]
fn paste(
    image: &Icf,
    group: &Bound<'_, PyAny>,
    first: &Bound<'_, PyAny>,
    source: &Bound<'_, PyAny>,
    limit: &Bound<'_, PyAny>,
) -> PyResult<usize> {
    let (group, first, limit) = (
        strict_usize(group)?,
        strict_usize(first)?,
        strict_usize(limit)?,
    );
    let rows: Vec<_> = source
        .try_iter()?
        .map(|v| Ok(v?.extract::<PyRef<'_, Record>>()?.inner))
        .collect::<PyResult<_>>()?;
    image.with_mut(|i| core::paste(i, group, first, &rows, limit).map_err(errors::icf))
}
#[pyfunction]
fn clear(
    image: &Icf,
    group: &Bound<'_, PyAny>,
    first: &Bound<'_, PyAny>,
    last: &Bound<'_, PyAny>,
    seed: &Record,
) -> PyResult<usize> {
    let (group, first, last) = (
        strict_usize(group)?,
        strict_usize(first)?,
        strict_usize(last)?,
    );
    image.with_mut(|i| core::clear(i, group, first, last, seed.inner).map_err(errors::icf))
}
#[pyfunction]
fn rename_group_raw(
    image: &Icf,
    group: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let (group, name) = (strict_usize(group)?, fixed_bytes(name)?);
    image.with_mut(|i| core::rename_group_raw(i, group, name).map_err(errors::icf))
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(first_free_physical, m)?)?;
    m.add_function(wrap_pyfunction!(copy, m)?)?;
    m.add_function(wrap_pyfunction!(paste, m)?)?;
    m.add_function(wrap_pyfunction!(clear, m)?)?;
    m.add_function(wrap_pyfunction!(rename_group_raw, m)?)?;
    m.add_function(wrap_pyfunction!(insert, m)?)?;
    m.add_function(wrap_pyfunction!(insert_initialized, m)?)?;
    m.add_function(wrap_pyfunction!(delete, m)?)?;
    m.add_function(wrap_pyfunction!(carry, m)?)?;
    Ok(())
}
