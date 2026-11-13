//! Fixed type-3 call pairs, distinct from the normal-memory selection model.
use super::*;
use cs705_core::icf::rows::native::call as native;

#[pyclass(module = "cs705.icf.rows.native.call")]
pub struct Selection {
    inner: native::Selection,
}
macro_rules! selection {
    ($($field:ident => $setter:ident, $ty:ty, $strict:ident);* ) => {
        #[pymethods] impl Selection {
            #[new] fn new($($field:&Bound<'_,PyAny>),*) -> PyResult<Self> {
                Ok(Self { inner: native::Selection { $($field:$strict($field)?),* } })
            }
            $(#[getter] fn $field(&self)->$ty { self.inner.$field }
            #[setter] fn $setter(&mut self,value:&Bound<'_,PyAny>)->PyResult<()> { self.inner.$field=$strict(value)?;Ok(()) })*
        }
    }
}
selection!(active=>set_active,usize,strict_usize;first=>set_first,usize,strict_usize;last=>set_last,usize,strict_usize;merged=>set_merged,bool,strict_bool;show_both=>set_show_both,bool,strict_bool);

#[pyfunction]
fn copy(
    py: Python<'_>,
    image: Py<MutableBytes>,
    selection: Py<Selection>,
    clipboard: Py<Clipboard>,
    effects: Py<Effects>,
) -> PyResult<()> {
    let selection = selection.try_borrow(py)?.inner;
    let image = image.get().take()?;
    let mut clipboard = clipboard.get().state.take()?;
    let mut effects = effects.get().state.take()?;
    py.detach(|| native::copy(&image, selection, &mut clipboard, &mut effects))
        .map_err(errors::icf)
}
#[pyfunction]
#[expect(
    clippy::too_many_arguments,
    reason = "Call paste retains explicit selection, clipboard, ranges, effects, confirmation and pre-store producer"
)]
fn paste<'py>(
    py: Python<'py>,
    image: Py<MutableBytes>,
    selection: Py<Selection>,
    clipboard: Py<Clipboard>,
    ranges: Py<Ranges>,
    effects: Py<Effects>,
    confirm: Py<PyAny>,
    before_store: Py<PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let selection = selection.try_borrow(py)?.inner;
    let confirm_holder = CallbackHolder::new(py, confirm.bind(py))?;
    let confirm = CallbackHolder::link(confirm_holder.bind(py));
    let store_holder = CallbackHolder::new(py, before_store.bind(py))?;
    let writer = CallbackHolder::link(store_holder.bind(py));
    super::execute(py, || {
        let mut image = image.get().take()?;
        let mut clipboard = clipboard.get().state.take()?;
        let mut effects = effects.get().state.take()?;
        let ranges = ranges.get().take()?;
        Ok(py.detach(|| {
            native::paste(
                &mut image,
                selection,
                &mut clipboard,
                &ranges,
                &mut effects,
                |reason| {
                    super::call(&confirm, |py, callback| {
                        strict_i32(&callback.call1((super::confirmation(py, reason)?,))?)
                    })
                },
                |index, event| super::store(&writer, index, event),
            )
        }))
    })
}
#[pyfunction]
fn carry<'py>(
    py: Python<'py>,
    image: Py<MutableBytes>,
    selection: Py<Selection>,
    direction: &Bound<'_, PyAny>,
    ranges: Py<Ranges>,
    effects: Py<Effects>,
    before_store: Py<PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let selection = selection.try_borrow(py)?.inner;
    let direction = strict_i32(direction)?;
    let store_holder = CallbackHolder::new(py, before_store.bind(py))?;
    let writer = CallbackHolder::link(store_holder.bind(py));
    super::execute(py, || {
        let mut image = image.get().take()?;
        let mut effects = effects.get().state.take()?;
        let ranges = ranges.get().take()?;
        Ok(py.detach(|| {
            native::carry(
                &mut image,
                selection,
                direction,
                &ranges,
                &mut effects,
                |index, event| super::store(&writer, index, event),
            )
        }))
    })
}
#[pyfunction]
fn reset_pair<'py>(
    py: Python<'py>,
    image: Py<MutableBytes>,
    selection: Py<Selection>,
    generation: Py<ReferenceImage>,
    confirmation: &Bound<'_, PyAny>,
    effects: Py<Effects>,
    before_store: Py<PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let selection = selection.try_borrow(py)?.inner;
    let confirmation = strict_i32(confirmation)?;
    let store_holder = CallbackHolder::new(py, before_store.bind(py))?;
    let writer = CallbackHolder::link(store_holder.bind(py));
    super::execute(py, || {
        let mut image = image.get().take()?;
        let mut effects = effects.get().state.take()?;
        generation.get().with_core(py, |generation| {
            Ok(py.detach(|| {
                native::reset_pair(
                    &mut image,
                    selection,
                    generation,
                    confirmation,
                    &mut effects,
                    |index, event| super::store(&writer, index, event),
                )
            }))
        })
    })
}
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "call")?;
    m.add_class::<Selection>()?;
    m.add_function(wrap_pyfunction!(copy, &m)?)?;
    m.add_function(wrap_pyfunction!(paste, &m)?)?;
    m.add_function(wrap_pyfunction!(carry, &m)?)?;
    m.add_function(wrap_pyfunction!(reset_pair, &m)?)?;
    parent.add_submodule(&m)
}
