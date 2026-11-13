//! Ordered native runners; Python failures preserve already committed prefixes.
use super::runtimes::*;
use super::*;
use crate::{
    callbacks::CallbackError,
    icf::document::adapters::{attach_checked, require_none},
    icf::{Icf, context::Context, frequency::Ranges, record::DecodedRecord, rows::NormalMemory},
    scoped::Frame,
};
use std::cell::RefCell;

fn store(
    interpreter: i64,
    callback: &Py<PyAny>,
    errors: &RefCell<CallbackError>,
    at: usize,
    bytes: &[u8],
) -> Result<(), cs705_core::icf::Error> {
    let result = attach_checked(interpreter, |py| {
        Frame::with_ref(py, bytes, |loan| {
            require_none(
                callback
                    .bind(py)
                    .call1((at, crate::csv::nls::Bytes::from_loan(loan)))?,
            )
        })
    });
    errors.borrow_mut().capture(result).map_err(|_| {
        cs705_core::icf::Error::Operation("Python extended before_store failed".into())
    })
}
#[pyfunction]
fn rf_row_visible(
    q: &Context,
    layout: &Bound<'_, PyAny>,
    index: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let layout = core::RfLayout::input(layout)?;
    let index = strict_usize(index)?;
    q.with_ref(|q| core::rf_row_visible(q, layout, index).map_err(crate::errors::icf))
}
#[pyfunction]
fn auto_repeater_row_visible(q: &Context, index: &Bound<'_, PyAny>) -> PyResult<bool> {
    let index = strict_usize(index)?;
    q.with_ref(|q| core::auto_repeater_row_visible(q, index).map_err(crate::errors::icf))
}
#[pyfunction]
fn fixed_edge_bad(
    holder: &DecodedRecord,
    ranges: &Ranges,
    q: &Context,
    numeric_only: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let numeric = strict_bool(numeric_only)?;
    holder.with_ref(|holder| {
        ranges.with_ref(|ranges| {
            q.with_ref(|q| {
                core::fixed_edge_bad(holder, ranges, q, numeric).map_err(crate::errors::icf)
            })
        })
    })
}

#[pyfunction]
fn edit_home_vfo(
    py: Python<'_>,
    image: Py<Icf>,
    ranges: Py<Ranges>,
    live_q: Py<Context>,
    input: &NumberInput,
    crt_point: &Bound<'_, PyAny>,
    old_warning_bits: &Bound<'_, PyAny>,
) -> PyResult<HomeVfoEffect> {
    let point = strict_u8(crt_point)?;
    let warning = strict_u32(old_warning_bits)?;
    let mut image = image.get().take()?;
    let q = live_q.get().take()?;
    let result = ranges.get().with_ref(|ranges| {
        input.with_core(py, |input| {
            py.detach(move || {
                core::edit_home_vfo(&mut image, ranges, &q, input, point, warning)
                    .map_err(crate::errors::icf)
            })
        })
    })?;
    Ok(HomeVfoEffect::from_core(result))
}
#[pyfunction]
fn edit_fixed_edge(
    py: Python<'_>,
    image: Py<Icf>,
    runtime: Py<FrequencyRuntime>,
    index: &Bound<'_, PyAny>,
    side: &Bound<'_, PyAny>,
    input: &NumberInput,
) -> PyResult<FixedEdgeEffect> {
    let index = strict_usize(index)?;
    let side = core::EdgeSide::input(side)?;
    let frequency = FrequencyArgs::capture(py, &runtime)?;
    let mut image = image.get().take()?;
    frequency.with_core(|runtime| {
        input.with_core(py, |input| {
            py.detach(move || {
                core::edit_fixed_edge(&mut image, runtime, index, side, input)
                    .map(FixedEdgeEffect::from_core)
                    .map_err(crate::errors::icf)
            })
        })
    })
}
#[pyfunction]
fn edit_fixed_edge_with_store(
    py: Python<'_>,
    image: Py<Icf>,
    runtime: Py<FrequencyRuntime>,
    index: &Bound<'_, PyAny>,
    side: &Bound<'_, PyAny>,
    input: &NumberInput,
    before_store: Py<PyAny>,
) -> PyResult<FixedEdgeEffect> {
    let index = strict_usize(index)?;
    let side = core::EdgeSide::input(side)?;
    let frequency = FrequencyArgs::capture(py, &runtime)?;
    let mut image = image.get().take()?;
    let interpreter = crate::scoped::interpreter(py)?;
    let (errors, result) = frequency.with_core(|runtime| {
        input.with_core(py, |input| {
            Ok(py.detach(move || {
                let errors = RefCell::new(CallbackError::default());
                let result = core::edit_fixed_edge_with_store(
                    &mut image,
                    runtime,
                    index,
                    side,
                    input,
                    |at, bytes| store(interpreter, &before_store, &errors, at, bytes),
                );
                drop(image);
                (errors.into_inner(), result)
            }))
        })
    })?;
    errors.finish(
        py,
        result
            .map(FixedEdgeEffect::from_core)
            .map_err(crate::errors::icf),
    )
}
#[pyfunction]
fn edit_auto_repeater_edge(
    py: Python<'_>,
    image: Py<Icf>,
    runtime: Py<AutoRepeaterRuntime>,
    index: &Bound<'_, PyAny>,
    side: &Bound<'_, PyAny>,
    input: &NumberInput,
    invocation: &AutoRepeaterInvocation,
) -> PyResult<AutoRepeaterEffect> {
    let index = strict_usize(index)?;
    let side = core::EdgeSide::input(side)?;
    let (frequency, system) = {
        let r = runtime.try_borrow(py)?;
        (FrequencyArgs::capture(py, &r.frequency)?, r.system_langid)
    };
    let invocation = invocation.with_ref(|v| Ok(*v))?;
    let mut image = image.get().take()?;
    frequency.with_core(|frequency| {
        input.with_core(py, |input| {
            py.detach(move || {
                core::edit_auto_repeater_edge(
                    &mut image,
                    core::AutoRepeaterRuntime {
                        frequency,
                        system_langid: system,
                    },
                    index,
                    side,
                    input,
                    invocation,
                )
                .map(AutoRepeaterEffect::from_core)
                .map_err(crate::errors::icf)
            })
        })
    })
}
#[pyfunction]
fn edit_auto_repeater_edge_with_store(
    py: Python<'_>,
    image: Py<Icf>,
    runtime: Py<AutoRepeaterRuntime>,
    target: &Bound<'_, PyAny>,
    input: &NumberInput,
    invocation: &AutoRepeaterInvocation,
    before_store: Py<PyAny>,
) -> PyResult<AutoRepeaterEffect> {
    let (index, side): (Bound<'_, PyAny>, Bound<'_, PyAny>) = target.extract()?;
    let index = strict_usize(&index)?;
    let side = core::EdgeSide::input(&side)?;
    let (frequency, system) = {
        let r = runtime.try_borrow(py)?;
        (FrequencyArgs::capture(py, &r.frequency)?, r.system_langid)
    };
    let invocation = invocation.with_ref(|v| Ok(*v))?;
    let mut image = image.get().take()?;
    let interpreter = crate::scoped::interpreter(py)?;
    let (errors, result) = frequency.with_core(|frequency| {
        input.with_core(py, |input| {
            Ok(py.detach(move || {
                let errors = RefCell::new(CallbackError::default());
                let result = core::edit_auto_repeater_edge_with_store(
                    &mut image,
                    core::AutoRepeaterRuntime {
                        frequency,
                        system_langid: system,
                    },
                    (index, side),
                    input,
                    invocation,
                    |at, bytes| store(interpreter, &before_store, &errors, at, bytes),
                );
                drop(image);
                (errors.into_inner(), result)
            }))
        })
    })?;
    errors.finish(
        py,
        result
            .map(AutoRepeaterEffect::from_core)
            .map_err(crate::errors::icf),
    )
}
#[pyfunction]
fn edit_auto_repeater_dup(
    py: Python<'_>,
    image: Py<Icf>,
    index: &Bound<'_, PyAny>,
    input: &ChoiceInput,
    binding: &Bound<'_, PyAny>,
    invocation: &AutoRepeaterInvocation,
) -> PyResult<AutoRepeaterEffect> {
    let index = strict_usize(index)?;
    let binding = strict_u32(binding)?;
    let invocation = invocation.with_ref(|v| Ok(*v))?;
    let input = input.to_core(py)?;
    let mut image = image.get().take()?;
    py.detach(move || {
        core::edit_auto_repeater_dup(&mut image, index, input, binding, invocation)
            .map(AutoRepeaterEffect::from_core)
            .map_err(crate::errors::icf)
    })
}
#[pyfunction]
fn edit_auto_repeater_dup_with_store(
    py: Python<'_>,
    image: Py<Icf>,
    index: &Bound<'_, PyAny>,
    input: &ChoiceInput,
    binding: &Bound<'_, PyAny>,
    invocation: &AutoRepeaterInvocation,
    before_store: Py<PyAny>,
) -> PyResult<AutoRepeaterEffect> {
    let index = strict_usize(index)?;
    let binding = strict_u32(binding)?;
    let invocation = invocation.with_ref(|v| Ok(*v))?;
    let input = input.to_core(py)?;
    let mut image = image.get().take()?;
    let interpreter = crate::scoped::interpreter(py)?;
    let (errors, result) = py.detach(move || {
        let errors = RefCell::new(CallbackError::default());
        let result = core::edit_auto_repeater_dup_with_store(
            &mut image,
            index,
            input,
            binding,
            invocation,
            |at, bytes| store(interpreter, &before_store, &errors, at, bytes),
        );
        drop(image);
        (errors.into_inner(), result)
    });
    errors.finish(
        py,
        result
            .map(AutoRepeaterEffect::from_core)
            .map_err(crate::errors::icf),
    )
}
#[pyfunction]
fn edit_dr(
    py: Python<'_>,
    image: Py<Icf>,
    field: &Bound<'_, PyAny>,
    input: &NumberInput,
    binding: &Bound<'_, PyAny>,
    crt_point: &Bound<'_, PyAny>,
    node_flags: &DrNodeFlags,
) -> PyResult<DrEffect> {
    let field = core::DrField::input(field)?;
    let binding = strict_u32(binding)?;
    let point = strict_u8(crt_point)?;
    let mut flags = node_flags.owner.take()?;
    let mut image = image.get().take()?;
    input.with_core(py, |input| {
        py.detach(move || {
            core::edit_dr(&mut image, field, input, binding, point, &mut flags)
                .map(DrEffect::from_core)
                .map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
fn edit_dr_with_store(
    py: Python<'_>,
    image: Py<Icf>,
    field: &Bound<'_, PyAny>,
    input: &NumberInput,
    runtime: &Bound<'_, PyAny>,
    node_flags: &DrNodeFlags,
    before_store: Py<PyAny>,
) -> PyResult<DrEffect> {
    let (binding, point): (Bound<'_, PyAny>, Bound<'_, PyAny>) = runtime.extract()?;
    let binding = strict_u32(&binding)?;
    let point = strict_u8(&point)?;
    let field = core::DrField::input(field)?;
    let mut flags = node_flags.owner.take()?;
    let mut image = image.get().take()?;
    let interpreter = crate::scoped::interpreter(py)?;
    let (errors, result) = input.with_core(py, |input| {
        Ok(py.detach(move || {
            let errors = RefCell::new(CallbackError::default());
            let result = core::edit_dr_with_store(
                &mut image,
                field,
                input,
                (binding, point),
                &mut flags,
                |at, bytes| store(interpreter, &before_store, &errors, at, bytes),
            );
            drop(image);
            drop(flags);
            (errors.into_inner(), result)
        }))
    })?;
    errors.finish(
        py,
        result.map(DrEffect::from_core).map_err(crate::errors::icf),
    )
}
#[pyfunction]
fn edit_home_group(
    py: Python<'_>,
    image: Py<Icf>,
    normal: Py<NormalMemory>,
    catalog: &HomeGroupCatalog,
    input: &NumberInput,
) -> PyResult<HomeGroupEffect> {
    let catalog = catalog.inner;
    let mut image = image.get().take()?;
    let normal = normal.get().take()?;
    input.with_core(py, |input| {
        py.detach(move || {
            core::edit_home_group(&mut image, &normal, catalog, input)
                .map(HomeGroupEffect::from_core)
                .map_err(crate::errors::icf)
        })
    })
}

fn with_home_channel<R>(
    py: Python<'_>,
    runtime: &Py<HomeChannelRuntime>,
    f: impl FnOnce(core::HomeChannelRuntime<'_>) -> PyResult<R>,
) -> PyResult<R> {
    let (frequency, current, show) = {
        let runtime = runtime.try_borrow(py)?;
        (
            FrequencyArgs::capture(py, &runtime.frequency)?,
            runtime.current.clone_ref(py),
            runtime.show_both_864614,
        )
    };
    frequency.with_core(|frequency| {
        f(core::HomeChannelRuntime {
            frequency,
            current: current.bind(py).as_bytes(),
            show_both_864614: show,
        })
    })
}
fn with_home_dr<R>(
    py: Python<'_>,
    runtime: &Py<HomeDrRuntime>,
    f: impl FnOnce(core::HomeDrRuntime<'_>) -> PyResult<R>,
) -> PyResult<R> {
    let (frequency, source, current) = {
        let runtime = runtime.try_borrow(py)?;
        (
            FrequencyArgs::capture(py, &runtime.frequency)?,
            runtime.source.clone_ref(py),
            runtime.current.clone_ref(py),
        )
    };
    frequency.with_core(|frequency| {
        f(core::HomeDrRuntime {
            frequency,
            source: source.bind(py).as_bytes(),
            current: current.bind(py).as_bytes(),
        })
    })
}
#[pyfunction]
fn home_channel_choices(
    py: Python<'_>,
    image: Py<Icf>,
    runtime: Py<HomeChannelRuntime>,
    popup: &Bound<'_, PyAny>,
) -> PyResult<Vec<i64>> {
    let popup = strict_bool(popup)?;
    let image = image.get().take()?;
    with_home_channel(py, &runtime, |runtime| {
        py.detach(move || {
            core::home_channel_choices(&image, runtime, popup).map_err(crate::errors::icf)
        })
    })
}
#[pyfunction]
fn edit_home_channel(
    py: Python<'_>,
    image: Py<Icf>,
    runtime: Py<HomeChannelRuntime>,
    input: &NumberInput,
    old_warning_bits: &Bound<'_, PyAny>,
) -> PyResult<HomeChannelEffect> {
    let warning = strict_u32(old_warning_bits)?;
    let mut image = image.get().take()?;
    with_home_channel(py, &runtime, |runtime| {
        input.with_core(py, |input| {
            py.detach(move || {
                core::edit_home_channel(&mut image, runtime, input, warning)
                    .map(HomeChannelEffect::from_core)
                    .map_err(crate::errors::icf)
            })
        })
    })
}
#[pyfunction]
fn home_dr_group_choices(
    py: Python<'_>,
    image: Py<Icf>,
    runtime: Py<HomeDrRuntime>,
) -> PyResult<Vec<i64>> {
    let image = image.get().take()?;
    with_home_dr(py, &runtime, |runtime| {
        py.detach(move || core::home_dr_group_choices(&image, runtime).map_err(crate::errors::icf))
    })
}
#[pyfunction]
fn edit_home_dr_group(
    py: Python<'_>,
    image: Py<Icf>,
    runtime: Py<HomeDrRuntime>,
    input: &NumberInput,
) -> PyResult<HomeDrEffect> {
    let mut image = image.get().take()?;
    with_home_dr(py, &runtime, |runtime| {
        input.with_core(py, |input| {
            py.detach(move || {
                core::edit_home_dr_group(&mut image, runtime, input)
                    .map(HomeDrEffect::from_core)
                    .map_err(crate::errors::icf)
            })
        })
    })
}
#[pyfunction]
fn home_dr_memory_choices(
    py: Python<'_>,
    image: Py<Icf>,
    runtime: Py<HomeDrRuntime>,
) -> PyResult<Vec<i64>> {
    let image = image.get().take()?;
    with_home_dr(py, &runtime, |runtime| {
        py.detach(move || core::home_dr_memory_choices(&image, runtime).map_err(crate::errors::icf))
    })
}
#[pyfunction]
fn edit_home_dr_memory(
    py: Python<'_>,
    image: Py<Icf>,
    runtime: Py<HomeDrRuntime>,
    input: &HomeDrMemoryInput,
    old_warning_bits: &Bound<'_, PyAny>,
) -> PyResult<HomeDrEffect> {
    let warning = strict_u32(old_warning_bits)?;
    let mut image = image.get().take()?;
    with_home_dr(py, &runtime, |runtime| {
        input.with_core(py, |input| {
            py.detach(move || {
                core::edit_home_dr_memory(&mut image, runtime, input, warning)
                    .map(HomeDrEffect::from_core)
                    .map_err(crate::errors::icf)
            })
        })
    })
}
#[pyfunction]
fn program_scan_choices(
    py: Python<'_>,
    image: Py<Icf>,
    current: Py<PyBytes>,
) -> PyResult<Vec<i64>> {
    let image = image.get().take()?;
    let current = current.bind(py).as_bytes();
    py.detach(move || core::program_scan_choices(&image, current).map_err(crate::errors::icf))
}
#[pyfunction]
fn edit_program_scan(
    py: Python<'_>,
    image: Py<Icf>,
    current: Py<PyBytes>,
    g: Py<PyBytes>,
    input: &ProgramScanInput,
) -> PyResult<ProgramScanEffect> {
    let mut image = image.get().take()?;
    let current = current.bind(py).as_bytes();
    let g = g.bind(py).as_bytes();
    input.with_core(py, |input| {
        py.detach(move || {
            core::edit_program_scan(&mut image, current, g, input)
                .map(ProgramScanEffect::from_core)
                .map_err(crate::errors::icf)
        })
    })
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(rf_row_visible, m)?)?;
    m.add_function(wrap_pyfunction!(auto_repeater_row_visible, m)?)?;
    m.add_function(wrap_pyfunction!(fixed_edge_bad, m)?)?;
    m.add_function(wrap_pyfunction!(edit_home_vfo, m)?)?;
    m.add_function(wrap_pyfunction!(edit_fixed_edge, m)?)?;
    m.add_function(wrap_pyfunction!(edit_fixed_edge_with_store, m)?)?;
    m.add_function(wrap_pyfunction!(edit_auto_repeater_edge, m)?)?;
    m.add_function(wrap_pyfunction!(edit_auto_repeater_edge_with_store, m)?)?;
    m.add_function(wrap_pyfunction!(edit_auto_repeater_dup, m)?)?;
    m.add_function(wrap_pyfunction!(edit_auto_repeater_dup_with_store, m)?)?;
    m.add_function(wrap_pyfunction!(edit_dr, m)?)?;
    m.add_function(wrap_pyfunction!(edit_dr_with_store, m)?)?;
    m.add_function(wrap_pyfunction!(edit_home_group, m)?)?;
    m.add_function(wrap_pyfunction!(home_channel_choices, m)?)?;
    m.add_function(wrap_pyfunction!(edit_home_channel, m)?)?;
    m.add_function(wrap_pyfunction!(home_dr_group_choices, m)?)?;
    m.add_function(wrap_pyfunction!(edit_home_dr_group, m)?)?;
    m.add_function(wrap_pyfunction!(home_dr_memory_choices, m)?)?;
    m.add_function(wrap_pyfunction!(edit_home_dr_memory, m)?)?;
    m.add_function(wrap_pyfunction!(program_scan_choices, m)?)?;
    m.add_function(wrap_pyfunction!(edit_program_scan, m)?)?;
    Ok(())
}
