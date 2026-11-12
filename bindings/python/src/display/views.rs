//! Shared view inputs required by display; no printing/layout implementation.
use super::*;
use cs705_core::printing as native;

pub fn kind(value: &Bound<'_, PyAny>) -> PyResult<native::ViewKind> {
    use native::ViewKind::*;
    let values = [
        Memory,
        Call,
        Bandstack,
        Memopad,
        Scan,
        Gps,
        Repeater,
        Ur,
        Allowed,
        Rx,
        FromDv,
        FromFm,
        To,
        UserEdge,
        Preset,
        RepeaterTop,
    ];
    let code = crate::icf::enum_value(value, "cs705.display", "ViewKind")?;
    values
        .get(usize::try_from(code).unwrap_or(usize::MAX))
        .copied()
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid ViewKind"))
}
owner!(ViewState, native::ViewState);
impl ViewState {
    pub fn with_core<R>(
        &self,
        f: impl FnOnce(&mut native::ViewState) -> PyResult<R>,
    ) -> PyResult<R> {
        self.with_mut(f)
    }
}
macro_rules! view {
    ($($field:ident:$ty:ty=>$set:ident,$parse:ident);* $(;)?)=>{
        #[pymethods] impl ViewState {
            #[new] fn new(kind:&Bound<'_,PyAny>,group:&Bound<'_,PyAny>)->PyResult<Self>{Ok(Self::from_core(native::ViewState::new(self::kind(kind)?,strict_i32(group)?).map_err(crate::errors::io)?))}
            #[staticmethod] fn from_image(current:&Bound<'_,PyAny>,kind:&Bound<'_,PyAny>,group:&Bound<'_,PyAny>)->PyResult<Self>{Ok(Self::from_core(native::ViewState::from_image(strict_bytes(current)?,self::kind(kind)?,strict_i32(group)?).map_err(crate::errors::io)?))}
            #[staticmethod] fn from_image_with_split(current:&Bound<'_,PyAny>,kind:&Bound<'_,PyAny>,group:&Bound<'_,PyAny>,show_split:&Bound<'_,PyAny>)->PyResult<Self>{Ok(Self::from_core(native::ViewState::from_image_with_split(strict_bytes(current)?,self::kind(kind)?,strict_i32(group)?,strict_u32(show_split)?).map_err(crate::errors::io)?))}
            fn record_type(&self)->PyResult<Option<u32>>{self.with_ref(|v|Ok(v.record_type()))}
            #[getter] fn kind<'py>(&self,py:Python<'py>)->PyResult<Bound<'py,PyAny>>{let kind=self.with_ref(|v|Ok(v.kind as i64))?;crate::icf::enum_result(py,"cs705.display","ViewKind",kind)}
            #[setter] fn set_kind(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let kind=self::kind(value)?;self.with_mut(|v|{v.kind=kind;Ok(())})}
            $(#[getter] fn $field(&self)->PyResult<$ty>{self.with_ref(|v|Ok(v.$field))}
            #[setter] fn $set(&self,value:&Bound<'_,PyAny>)->PyResult<()>{let value=$parse(value)?;self.with_mut(|v|{v.$field=value;Ok(())})})*
        }
    }
}
view!(node:Option<u32> =>set_node,optional_u32;group:i32=>set_group,strict_i32;bc:u8=>set_bc,strict_u8;fixed_columns:usize=>set_fixed_columns,strict_usize;fixed_rows:usize=>set_fixed_rows,strict_usize;current_row:i32=>set_current_row,strict_i32;show_split:u32=>set_show_split,strict_u32;active:bool=>set_active,strict_bool);

enum ViewsTarget {
    Owned(Target<native::Views>),
    Display(Target<core::State>),
}
#[pyclass(module = "cs705.display", frozen)]
pub struct Views {
    target: ViewsTarget,
}
impl Views {
    pub fn with_execution<R>(
        &self,
        f: impl FnOnce(&mut native::Views) -> PyResult<R>,
    ) -> PyResult<R> {
        match &self.target {
            ViewsTarget::Owned(target) => f(&mut *target.take()?),
            ViewsTarget::Display(target) => {
                let mut state = target.take()?;
                f(&mut state.views)
            }
        }
    }
    pub fn from_loan(loan: crate::scoped::Loan<native::Views>) -> Self {
        Self {
            target: ViewsTarget::Owned(Target::Scoped(loan)),
        }
    }
    pub fn with_core<R>(&self, f: impl FnOnce(&mut native::Views) -> PyResult<R>) -> PyResult<R> {
        self.with_mut(f)
    }
    pub fn from_core(value: native::Views) -> Self {
        Self {
            target: ViewsTarget::Owned(Target::new(value)),
        }
    }
    pub fn from_state(target: Target<core::State>) -> Self {
        Self {
            target: ViewsTarget::Display(target),
        }
    }
    pub fn with_ref<R>(&self, f: impl FnOnce(&native::Views) -> PyResult<R>) -> PyResult<R> {
        match &self.target {
            ViewsTarget::Owned(v) => v.with_ref(f),
            ViewsTarget::Display(v) => v.with_ref(|v| f(&v.views)),
        }
    }
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut native::Views) -> PyResult<R>) -> PyResult<R> {
        match &self.target {
            ViewsTarget::Owned(v) => v.with_mut(f),
            ViewsTarget::Display(v) => v.with_mut(|v| f(&mut v.views)),
        }
    }
    pub fn take(&self) -> PyResult<crate::owner::Taken<'_, native::Views>> {
        match &self.target {
            ViewsTarget::Owned(v) => v.take(),
            ViewsTarget::Display(_) => Err(pyo3::exceptions::PyRuntimeError::new_err(
                "cannot consume a display's live view field",
            )),
        }
    }
}
#[pymethods]
impl Views {
    #[new]
    fn new() -> Self {
        Self::from_core(native::Views::default())
    }
    fn states_snapshot(&self) -> PyResult<Vec<ViewState>> {
        Ok(self
            .with_ref(|v| Ok(v.states.clone()))?
            .into_iter()
            .map(ViewState::from_core)
            .collect())
    }
    fn set_states(&self, values: Vec<Py<ViewState>>) -> PyResult<()> {
        let values = values
            .iter()
            .map(|v| v.get().with_ref(|v| Ok(v.clone())))
            .collect::<PyResult<Vec<_>>>()?;
        match &self.target {
            ViewsTarget::Owned(v) => v.with_mut(|v| {
                v.states = values;
                Ok(())
            }),
            ViewsTarget::Display(v) => v.with_mut(|v| {
                if v.current
                    .as_ref()
                    .is_some_and(|grid| grid.view_id >= values.len())
                {
                    return Err(pyo3::exceptions::PyIndexError::new_err(
                        "new states would invalidate current grid",
                    ));
                }
                v.views.states = values;
                Ok(())
            }),
        }
    }
    fn register(&self, view: &ViewState) -> PyResult<usize> {
        let view = view.with_ref(|v| Ok(v.clone()))?;
        self.with_mut(|v| Ok(v.register(view)))
    }
    fn deactivate(&self, id: &Bound<'_, PyAny>) -> PyResult<()> {
        let id = strict_usize(id)?;
        self.with_mut(|v| {
            if id >= v.states.len() {
                return Err(pyo3::exceptions::PyIndexError::new_err(
                    "view index out of bounds",
                ));
            }
            v.deactivate(id);
            Ok(())
        })
    }
    fn query(
        &self,
        current: &Bound<'_, PyAny>,
        event: &Bound<'_, PyAny>,
        argument: &Bound<'_, PyAny>,
    ) -> PyResult<Vec<(u32, u32)>> {
        let current = strict_bytes(current)?;
        let event = strict_u32(event)?;
        let argument = strict_u32(argument)?;
        self.with_mut(|v| {
            Ok(v.query(current, event, argument)
                .map_err(crate::errors::io)?
                .iter()
                .map(|[a, b]| (*a, *b))
                .collect())
        })
    }
    fn nonempty(&self, current: &Bound<'_, PyAny>, id: &Bound<'_, PyAny>) -> PyResult<bool> {
        let current = strict_bytes(current)?;
        let id = strict_usize(id)?;
        self.with_mut(|v| v.nonempty(current, id).map_err(crate::errors::io))
    }
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    crate::presentation::add_enum(
        m,
        "cs705.display",
        "ViewKind",
        &[
            "Memory",
            "Call",
            "Bandstack",
            "Memopad",
            "Scan",
            "Gps",
            "Repeater",
            "Ur",
            "Allowed",
            "Rx",
            "FromDv",
            "FromFm",
            "To",
            "UserEdge",
            "Preset",
            "RepeaterTop",
        ],
    )?;
    m.add_class::<ViewState>()?;
    m.add_class::<Views>()
}
