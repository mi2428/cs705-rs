//! Headless print layout and explicitly supplied native resources.
use crate::{conversion::*, icf::document::Target};
use cs705_core::printing as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
pub mod native;
macro_rules! owner {
    ($name:ident,$ty:ty) => {
        #[pyclass(module = "cs705.printing", frozen)]
        pub struct $name {
            pub(crate) target: Target<$ty>,
        }
        impl $name {
            pub fn from_core(value: $ty) -> Self {
                Self {
                    target: Target::new(value),
                }
            }
            pub fn from_loan(loan: crate::scoped::Loan<$ty>) -> Self {
                Self {
                    target: Target::Scoped(loan),
                }
            }
            pub fn with_ref<R>(&self, f: impl FnOnce(&$ty) -> PyResult<R>) -> PyResult<R> {
                self.target.with_ref(f)
            }
            pub fn with_mut<R>(&self, f: impl FnOnce(&mut $ty) -> PyResult<R>) -> PyResult<R> {
                self.target.with_mut(f)
            }
        }
    };
}
pub(crate) fn i32s(v: &Bound<'_, PyAny>) -> PyResult<Vec<i32>> {
    v.try_iter()?.map(|v| strict_i32(&v?)).collect()
}
pub(crate) fn bools(v: &Bound<'_, PyAny>) -> PyResult<Vec<bool>> {
    v.try_iter()?.map(|v| strict_bool(&v?)).collect()
}
pub(crate) fn strings(v: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    v.try_iter()?.map(|v| v?.extract()).collect()
}
pub(crate) fn texts(v: &Bound<'_, PyAny>) -> PyResult<Vec<Vec<String>>> {
    v.try_iter()?.map(|v| strings(&v?)).collect()
}
pub(crate) fn bytes(v: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    Ok(strict_bytes(v)?.to_vec())
}
macro_rules! scalar {
 ($name:ident,$ty:path; $($field:ident:$ft:ty=>$set:ident,$parse:ident);*; @extra {$($extra:tt)*})=>{
  owner!($name,$ty);
  #[pymethods] impl $name {
   #[new] fn new($($field:&Bound<'_,PyAny>),*)->PyResult<Self>{use $ty as Core;Ok(Self::from_core(Core{$($field:$parse($field)?),*}))}
   $(#[getter] fn $field(&self)->PyResult<$ft>{self.with_ref(|v|Ok(v.$field.clone()))}
   #[setter] fn $set(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let value=$parse(value)?;self.with_mut(|v|{v.$field=value;Ok(())})})*
   $($extra)*
  }
 }
}
// Shared ownership is reused, while these native types remain printing types.
scalar!(Metrics,core::Metrics;font_height:i32=>set_font_height,strict_i32;em:i32=>set_em,strict_i32;advance_em:i32=>set_advance_em,strict_i32;line_em:i32=>set_line_em,strict_i32;padding:i32=>set_padding,strict_i32;@extra{fn validate(&self)->PyResult<()>{self.with_ref(|v|v.validate().map_err(crate::errors::io))}});
scalar!(Paper,core::Paper;width:i32=>set_width,strict_i32;height:i32=>set_height,strict_i32;@extra{});
owner!(Page, core::Page);
macro_rules! page {($($field:ident:$ty:ty=>$set:ident,$parse:ident);*)=>{#[pymethods]impl Page {
 #[new]fn new(columns:&Bound<'_,PyAny>,rows:&Bound<'_,PyAny>,$($field:&Bound<'_,PyAny>),*)->PyResult<Self>{Ok(Self::from_core(core::Page{columns:strict_range(columns)?,rows:strict_range(rows)?,$($field:$parse($field)?),*}))}
 #[getter]fn columns<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{let v=self.with_ref(|v|Ok(v.columns.clone()))?;py.import("builtins")?.getattr("range")?.call1((v.start,v.end))}
 #[setter]fn set_columns(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_range(v)?;self.with_mut(|v|{v.columns=n;Ok(())})}
 #[getter]fn rows<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{let v=self.with_ref(|v|Ok(v.rows.clone()))?;py.import("builtins")?.getattr("range")?.call1((v.start,v.end))}
 #[setter]fn set_rows(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=strict_range(v)?;self.with_mut(|v|{v.rows=n;Ok(())})}
 $(#[getter]fn $field(&self)->PyResult<$ty>{self.with_ref(|v|Ok(v.$field))}#[setter]fn $set(&self,v:&Bound<'_,PyAny>)->PyResult<()>{let n=$parse(v)?;self.with_mut(|v|{v.$field=n;Ok(())})})*
}}}
page!(font_height:i32=>set_font_height,strict_i32;attempts:u8=>set_attempts,strict_u8;content_width:i32=>set_content_width,strict_i32;content_height:i32=>set_content_height,strict_i32;fits:bool=>set_fits,strict_bool);
owner!(Grid, core::Grid);
#[pymethods]
impl Grid {
    #[new]
    #[expect(
        clippy::too_many_arguments,
        reason = "The print grid constructor keeps text, fixed axes, boundary arrays, native nodes and factory separate."
    )]
    fn new(
        name: String,
        cells: &Bound<'_, PyAny>,
        fixed_rows: &Bound<'_, PyAny>,
        fixed_columns: &Bound<'_, PyAny>,
        column_boundaries: &Bound<'_, PyAny>,
        row_boundaries: &Bound<'_, PyAny>,
        nodes: &Bound<'_, PyAny>,
        factory: &Bound<'_, PyAny>,
        show_split: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self::from_core(core::Grid {
            name,
            cells: texts(cells)?,
            fixed_rows: strict_usize(fixed_rows)?,
            fixed_columns: strict_usize(fixed_columns)?,
            column_boundaries: bools(column_boundaries)?,
            row_boundaries: bools(row_boundaries)?,
            nodes: native::nodes(nodes)?,
            factory: if factory.is_none() {
                None
            } else {
                Some(native::factory(factory)?)
            },
            show_split: strict_u32(show_split)?,
        }))
    }
    fn nodes_snapshot(&self) -> PyResult<Option<Vec<Vec<native::CellNode>>>> {
        Ok(self.with_ref(|v| Ok(v.nodes.clone()))?.map(|rows| {
            rows.into_iter()
                .map(|row| row.into_iter().map(native::CellNode::from_core).collect())
                .collect()
        }))
    }
    fn set_nodes(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let nodes = native::nodes(value)?;
        self.with_mut(|v| {
            v.nodes = nodes;
            Ok(())
        })
    }
    fn factory_snapshot(&self) -> PyResult<Option<native::Factory>> {
        Ok(self
            .with_ref(|v| Ok(v.factory.clone()))?
            .map(native::Factory::from_core))
    }
    fn set_factory(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let f = if value.is_none() {
            None
        } else {
            Some(native::factory(value)?)
        };
        self.with_mut(|v| {
            v.factory = f;
            Ok(())
        })
    }
    #[getter]
    fn name(&self) -> PyResult<String> {
        self.with_ref(|v| Ok(v.name.clone()))
    }
    #[setter]
    fn set_name(&self, value: String) -> PyResult<()> {
        self.with_mut(|v| {
            v.name = value;
            Ok(())
        })
    }
    fn cells_snapshot(&self) -> PyResult<Vec<Vec<String>>> {
        self.with_ref(|v| Ok(v.cells.clone()))
    }
    fn set_cells(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = texts(value)?;
        self.with_mut(|v| {
            v.cells = value;
            Ok(())
        })
    }
    #[getter]
    fn fixed_rows(&self) -> PyResult<usize> {
        self.with_ref(|v| Ok(v.fixed_rows))
    }
    #[setter]
    fn set_fixed_rows(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_usize(v)?;
        self.with_mut(|v| {
            v.fixed_rows = n;
            Ok(())
        })
    }
    #[getter]
    fn fixed_columns(&self) -> PyResult<usize> {
        self.with_ref(|v| Ok(v.fixed_columns))
    }
    #[setter]
    fn set_fixed_columns(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_usize(v)?;
        self.with_mut(|v| {
            v.fixed_columns = n;
            Ok(())
        })
    }
    fn column_boundaries_snapshot(&self) -> PyResult<Vec<bool>> {
        self.with_ref(|v| Ok(v.column_boundaries.clone()))
    }
    fn set_column_boundaries(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = bools(v)?;
        self.with_mut(|v| {
            v.column_boundaries = n;
            Ok(())
        })
    }
    fn row_boundaries_snapshot(&self) -> PyResult<Vec<bool>> {
        self.with_ref(|v| Ok(v.row_boundaries.clone()))
    }
    fn set_row_boundaries(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = bools(v)?;
        self.with_mut(|v| {
            v.row_boundaries = n;
            Ok(())
        })
    }
    #[getter]
    fn show_split(&self) -> PyResult<u32> {
        self.with_ref(|v| Ok(v.show_split))
    }
    #[setter]
    fn set_show_split(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = strict_u32(v)?;
        self.with_mut(|v| {
            v.show_split = n;
            Ok(())
        })
    }
}
owner!(Layout, core::Layout);
#[pymethods]
impl Layout {
    #[new]
    fn new(
        horizontal: &Bound<'_, PyAny>,
        vertical: &Bound<'_, PyAny>,
        pages: Vec<Py<Page>>,
    ) -> PyResult<Self> {
        let pages = pages
            .iter()
            .map(|v| v.get().with_ref(|v| Ok(v.clone())))
            .collect::<PyResult<_>>()?;
        Ok(Self::from_core(core::Layout {
            horizontal: bytes(horizontal)?,
            vertical: bytes(vertical)?,
            pages,
        }))
    }
    #[getter]
    fn horizontal<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let v = self.with_ref(|v| Ok(v.horizontal.clone()))?;
        Ok(PyBytes::new(py, &v))
    }
    #[setter]
    fn set_horizontal(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = bytes(v)?;
        self.with_mut(|v| {
            v.horizontal = n;
            Ok(())
        })
    }
    #[getter]
    fn vertical<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let v = self.with_ref(|v| Ok(v.vertical.clone()))?;
        Ok(PyBytes::new(py, &v))
    }
    #[setter]
    fn set_vertical(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let n = bytes(v)?;
        self.with_mut(|v| {
            v.vertical = n;
            Ok(())
        })
    }
    fn pages_snapshot(&self) -> PyResult<Vec<Page>> {
        Ok(self
            .with_ref(|v| Ok(v.pages.clone()))?
            .into_iter()
            .map(Page::from_core)
            .collect())
    }
    fn set_pages(&self, pages: Vec<Py<Page>>) -> PyResult<()> {
        let pages = pages
            .iter()
            .map(|v| v.get().with_ref(|v| Ok(v.clone())))
            .collect::<PyResult<_>>()?;
        self.with_mut(|v| {
            v.pages = pages;
            Ok(())
        })
    }
}
#[pyfunction]
fn axis_spans<'py>(
    py: Python<'py>,
    sizes: &Bound<'_, PyAny>,
    fixed: &Bound<'_, PyAny>,
    blocked: &Bound<'_, PyAny>,
    available: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let result = core::axis_spans(
        &i32s(sizes)?,
        strict_usize(fixed)?,
        &bools(blocked)?,
        strict_i32(available)?,
    )
    .map_err(crate::errors::io)?;
    Ok(PyBytes::new(py, &result))
}
#[pyfunction]
fn layout(py: Python<'_>, grid: &Grid, paper: &Paper, metrics: &Metrics) -> PyResult<Layout> {
    let grid = grid.target.take()?;
    let paper = paper.with_ref(|v| Ok(*v))?;
    let metrics = metrics.with_ref(|v| Ok(*v))?;
    Ok(Layout::from_core(
        py.detach(move || core::layout(&grid, paper, metrics))
            .map_err(crate::errors::io)?,
    ))
}
#[pyfunction]
fn html_page(
    py: Python<'_>,
    grid: &Grid,
    page: &Page,
    paper: &Paper,
    metrics: &Metrics,
    number: &Bound<'_, PyAny>,
) -> PyResult<String> {
    let grid = grid.target.take()?;
    let page = page.target.take()?;
    let paper = paper.with_ref(|v| Ok(*v))?;
    let metrics = metrics.with_ref(|v| Ok(*v))?;
    let number = strict_usize(number)?;
    py.detach(move || core::html_page(&grid, &page, paper, metrics, number))
        .map_err(crate::errors::io)
}
mod io;
pub(crate) mod resources;
mod tasks;
pub use tasks::{Editor, Job};
pub fn register(root: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(root.py(), "printing")?;
    m.add_class::<Metrics>()?;
    m.add_class::<Paper>()?;
    m.add_class::<Page>()?;
    m.add_class::<Grid>()?;
    m.add_class::<Layout>()?;
    m.add_function(wrap_pyfunction!(axis_spans, &m)?)?;
    m.add_function(wrap_pyfunction!(layout, &m)?)?;
    m.add_function(wrap_pyfunction!(html_page, &m)?)?;
    tasks::register(&m)?;
    for name in ["ViewState", "Views", "ViewKind", "Resources"] {
        m.add(name, root.getattr("display")?.getattr(name)?)?;
    }
    m.add_class::<resources::HtmlBuffer>()?;
    native::register(&m)?;
    root.add_submodule(&m)
}
