//! Native resource operations over the one canonical display Resources class.
use super::*;
use crate::{
    callbacks::CallbackError, display::Resources, icf::document::adapters::attach_checked,
    scoped::Frame,
};
use std::cell::RefCell;
pub(crate) fn condition(
    interpreter: i64,
    f: &Py<PyAny>,
    errors: &RefCell<CallbackError>,
    name: &[u8],
) -> std::io::Result<bool> {
    let result = attach_checked(interpreter, |py| {
        strict_bool(&f.bind(py).call1((PyBytes::new(py, name),))?)
    });
    errors
        .borrow_mut()
        .capture(result)
        .map_err(|_| std::io::Error::other("Python resource condition failed"))
}
fn display(
    interpreter: i64,
    f: &Py<PyAny>,
    errors: &RefCell<CallbackError>,
    node: &core::native::CellNode,
) -> std::io::Result<Vec<u8>> {
    let result = attach_checked(interpreter, |py| {
        Frame::with_ref(py, node, |loan| {
            let result = f
                .bind(py)
                .call1((Py::new(py, native::CellNode::from_loan(loan))?,))?;
            bytes(&result)
        })
    });
    errors
        .borrow_mut()
        .capture(result)
        .map_err(|_| std::io::Error::other("Python native display failed"))
}
pub fn schema(
    py: Python<'_>,
    resources: &Resources,
    factory: &native::Factory,
    indices: &Bound<'_, PyAny>,
    predicate: Py<PyAny>,
) -> PyResult<Grid> {
    let indices = i32s(indices)?;
    let factory = factory.target.take()?;
    let interpreter = crate::scoped::interpreter(py)?;
    let (errors, result) = py.detach(move || {
        let errors = RefCell::new(CallbackError::default());
        let result = resources.with_core(|r| {
            r.schema(&factory, &indices, |name| {
                condition(interpreter, &predicate, &errors, name)
            })
            .map_err(crate::errors::io)
        });
        (errors.into_inner(), result)
    });
    Ok(Grid::from_core(errors.finish(py, result)?))
}
#[expect(
    clippy::too_many_arguments,
    reason = "Production keeps the image, source snapshot, factory, split and two callbacks separate."
)]
pub fn instantiate(
    py: Python<'_>,
    resources: &Resources,
    image: Py<crate::icf::Icf>,
    current: &Bound<'_, PyAny>,
    factory: &native::Factory,
    split: &Bound<'_, PyAny>,
    predicate: Py<PyAny>,
    paint: Option<Py<PyAny>>,
) -> PyResult<Grid> {
    let current = bytes(current)?;
    let split = strict_u32(split)?;
    let factory = factory.target.take()?;
    let image = image.get().take()?;
    let interpreter = crate::scoped::interpreter(py)?;
    let (errors, result) = py.detach(move || {
        let errors = RefCell::new(CallbackError::default());
        let result = resources.with_core(|r| {
            let condition = |name: &[u8]| condition(interpreter, &predicate, &errors, name);
            match paint {
                None => r.instantiate(&image, &current, &factory, split, condition),
                Some(ref paint) => {
                    r.produce(&image, &current, &factory, split, condition, |node| {
                        display(interpreter, paint, &errors, node)
                    })
                }
            }
            .map_err(crate::errors::io)
        });
        (errors.into_inner(), result)
    });
    Ok(Grid::from_core(errors.finish(py, result)?))
}
owner!(HtmlBuffer, String);
#[pymethods]
impl HtmlBuffer {
    #[new]
    fn new(text: String) -> Self {
        Self::from_core(text)
    }
    #[getter]
    fn text(&self) -> PyResult<String> {
        self.with_ref(|v| Ok(v.clone()))
    }
    #[setter]
    fn set_text(&self, text: String) -> PyResult<()> {
        self.with_mut(|v| {
            *v = text;
            Ok(())
        })
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Rendering borrows distinct resource, grid, layout, view, job and output owners with explicit measurements."
)]
pub fn render(
    py: Python<'_>,
    resources: &Resources,
    current: &Bound<'_, PyAny>,
    grid: &Grid,
    layout: &Layout,
    views: &crate::display::Views,
    job: &super::Job,
    paper: &Paper,
    metrics: &Metrics,
    html: &HtmlBuffer,
    paint: Py<PyAny>,
) -> PyResult<()> {
    let current = bytes(current)?;
    let grid = grid.target.take()?;
    let layout = layout.target.take()?;
    let mut html = html.target.take()?;
    let paper = paper.with_ref(|v| Ok(*v))?;
    let metrics = metrics.with_ref(|v| Ok(*v))?;
    let interpreter = crate::scoped::interpreter(py)?;
    // A scoped Job is an explicit native reborrow from Job.tick; nested display
    // callbacks get their own node frame while the outer job stays exclusive.
    let (errors, result) = job.with_mut(|job| {
        views.with_execution(|views| {
            Ok(py.detach(move || {
                let errors = RefCell::new(CallbackError::default());
                let result = resources.with_core(|r| {
                    r.render(
                        &current,
                        &grid,
                        &layout,
                        views,
                        job,
                        paper,
                        metrics,
                        &mut html,
                        |node| display(interpreter, &paint, &errors, node),
                    )
                    .map_err(crate::errors::io)
                });
                drop(html);
                drop(grid);
                drop(layout);
                (errors.into_inner(), result)
            }))
        })
    })?;
    errors.finish(py, result)
}
