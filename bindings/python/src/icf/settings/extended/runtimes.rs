//! Explicit runtime sources are retained as canonical Python owners.
use super::*;
use crate::{
    clone::Handle,
    icf::{context::Context, frequency::Ranges},
};

effects!(#[expect(clippy::too_many_arguments, reason = "The constructor keeps fixed-edge candidate, pair, validation flags and helper effects independent.")] FixedEdgeEffect {stored:bool=>set_stored;helper_accepted:bool=>set_helper_accepted;candidate_hz:u32=>set_candidate_hz;raw_candidate_hz:u32=>set_raw_candidate_hz;pair_hz:[u32;2]=>set_pair_hz;range_bad:[bool;2]=>set_range_bad;absolute_bad:[bool;2]=>set_absolute_bad;state_out:u32=>set_state_out;notify_flags:u32=>set_notify_flags;});
effects!(AutoRepeaterInvocation {code:u32=>set_code;argument:u32=>set_argument;warning_bits:u8=>set_warning_bits;});
effects!(#[expect(clippy::too_many_arguments, reason = "The constructor exposes the full native auto-repeater effect, including ordered warnings and redispatch state.")] AutoRepeaterEffect {stored:bool=>set_stored;helper_accepted:bool=>set_helper_accepted;candidate:i32=>set_candidate;pair_hz:[u32;2]=>set_pair_hz;dup:u32=>set_dup;valid:bool=>set_valid;state_out:u32=>set_state_out;notify_flags:u32=>set_notify_flags;warning_bits:u8=>set_warning_bits;gray:bool=>set_gray;popup_requested:bool=>set_popup_requested;blocked:bool=>set_blocked;move_to_low:bool=>set_move_to_low;redispatch_code:Option<u32> =>set_redispatch_code;});
effects!(DrEffect {stored:bool=>set_stored;synchronized:bool=>set_synchronized;candidate:i32=>set_candidate;state_out:u32=>set_state_out;gray:bool=>set_gray;notify_flags:u32=>set_notify_flags;display:Vec<u8> =>set_display;});
effects!(HomeGroupEffect {stored:bool=>set_stored;candidate:i32=>set_candidate;group_raw:u8=>set_group_raw;channel_raw:u16=>set_channel_raw;state_out:u32=>set_state_out;notify_flags:u32=>set_notify_flags;});
effects!(HomeChannelEffect {stored:bool=>set_stored;candidate:u32=>set_candidate;state_out:u32=>set_state_out;popup_requested:bool=>set_popup_requested;notify_flags:u32=>set_notify_flags;warning_bits:u32=>set_warning_bits;});
effects!(#[expect(clippy::too_many_arguments, reason = "The constructor keeps home-DR helper acceptance, store, reference, warnings and popup effects separate.")] HomeDrEffect {accepted:bool=>set_accepted;stored:bool=>set_stored;candidate:i32=>set_candidate;reference:u16=>set_reference;notify_flags:u32=>set_notify_flags;warning_bits:u32=>set_warning_bits;state_out:u32=>set_state_out;popup_requested:bool=>set_popup_requested;});
effects!(ProgramScanEffect {accepted:bool=>set_accepted;stored:bool=>set_stored;candidate:i32=>set_candidate;selected:u8=>set_selected;notify_flags:u32=>set_notify_flags;popup_requested:bool=>set_popup_requested;});

#[pyclass(module = "cs705.icf.settings.extended", frozen)]
pub struct HomeVfoEffect {
    root: Handle<core::HomeVfoEffect>,
}
impl HomeVfoEffect {
    pub fn from_core(value: core::HomeVfoEffect) -> Self {
        Self {
            root: Handle::new(value),
        }
    }
}
#[pymethods]
impl HomeVfoEffect {
    #[new]
    fn new(
        number: &NumberEffect,
        warning_bits: &Bound<'_, PyAny>,
        empty_marker: &Bound<'_, PyAny>,
        outside_marker: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self::from_core(core::HomeVfoEffect {
            number: number.with_ref(|v| Ok(v.clone()))?,
            warning_bits: strict_u32(warning_bits)?,
            empty_marker: strict_bool(empty_marker)?,
            outside_marker: strict_bool(outside_marker)?,
        }))
    }
    #[getter]
    fn number(&self) -> PyResult<NumberEffect> {
        self.root.with(|_| Ok(()))?;
        Ok(NumberEffect::from_projection(
            self.root.project(|v| Ok(&mut v.number)),
        ))
    }
    #[setter]
    fn set_number(&self, v: &NumberEffect) -> PyResult<()> {
        let value = v.with_ref(|v| Ok(v.clone()))?;
        self.root.with(|v| {
            v.number = value;
            Ok(())
        })
    }
    #[getter]
    fn warning_bits(&self) -> PyResult<u32> {
        self.root.with(|v| Ok(v.warning_bits))
    }
    #[setter]
    fn set_warning_bits(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u32(value)?;
        self.root.with(|v| {
            v.warning_bits = value;
            Ok(())
        })
    }
    #[getter]
    fn empty_marker(&self) -> PyResult<bool> {
        self.root.with(|v| Ok(v.empty_marker))
    }
    #[setter]
    fn set_empty_marker(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.root.with(|v| {
            v.empty_marker = value;
            Ok(())
        })
    }
    #[getter]
    fn outside_marker(&self) -> PyResult<bool> {
        self.root.with(|v| Ok(v.outside_marker))
    }
    #[setter]
    fn set_outside_marker(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.root.with(|v| {
            v.outside_marker = value;
            Ok(())
        })
    }
}
#[pyclass(module = "cs705.icf.settings.extended")]
pub struct HomeGroupCatalog {
    pub inner: core::HomeGroupCatalog,
}
#[pymethods]
impl HomeGroupCatalog {
    #[new]
    fn new(empty: &Bound<'_, PyAny>, call: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: core::HomeGroupCatalog {
                empty: strict_i32(empty)?,
                call: strict_i32(call)?,
            },
        })
    }
    #[staticmethod]
    fn initial() -> Self {
        Self {
            inner: core::HomeGroupCatalog::initial(),
        }
    }
    #[getter]
    fn empty(&self) -> i32 {
        self.inner.empty
    }
    #[setter]
    fn set_empty(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.empty = strict_i32(v)?;
        Ok(())
    }
    #[getter]
    fn call(&self) -> i32 {
        self.inner.call
    }
    #[setter]
    fn set_call(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.call = strict_i32(v)?;
        Ok(())
    }
}

#[pyclass(module = "cs705.icf.settings.extended")]
pub struct FrequencyRuntime {
    #[pyo3(get, set)]
    pub ranges: Py<Ranges>,
    #[pyo3(get, set)]
    pub live_q: Py<Context>,
    pub binding: u32,
    pub crt_point: u8,
}
pub struct FrequencyArgs {
    pub ranges: Py<Ranges>,
    pub live_q: Py<Context>,
    pub binding: u32,
    pub crt_point: u8,
}
impl FrequencyArgs {
    pub fn capture(py: Python<'_>, runtime: &Py<FrequencyRuntime>) -> PyResult<Self> {
        let value = runtime.try_borrow(py)?;
        Ok(Self {
            ranges: value.ranges.clone_ref(py),
            live_q: value.live_q.clone_ref(py),
            binding: value.binding,
            crt_point: value.crt_point,
        })
    }
    pub fn with_core<R>(
        &self,
        f: impl FnOnce(core::FrequencyRuntime<'_>) -> PyResult<R>,
    ) -> PyResult<R> {
        let q = self.live_q.get().take()?;
        self.ranges.get().with_ref(|ranges| {
            f(core::FrequencyRuntime {
                ranges,
                live_q: &q,
                binding: self.binding,
                crt_point: self.crt_point,
            })
        })
    }
}
#[pymethods]
impl FrequencyRuntime {
    #[new]
    fn new(
        ranges: Py<Ranges>,
        live_q: Py<Context>,
        binding: &Bound<'_, PyAny>,
        crt_point: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            ranges,
            live_q,
            binding: strict_u32(binding)?,
            crt_point: strict_u8(crt_point)?,
        })
    }
    #[getter]
    fn binding(&self) -> u32 {
        self.binding
    }
    #[setter]
    fn set_binding(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.binding = strict_u32(v)?;
        Ok(())
    }
    #[getter]
    fn crt_point(&self) -> u8 {
        self.crt_point
    }
    #[setter]
    fn set_crt_point(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.crt_point = strict_u8(v)?;
        Ok(())
    }
}
#[pyclass(module = "cs705.icf.settings.extended")]
pub struct AutoRepeaterRuntime {
    #[pyo3(get, set)]
    pub frequency: Py<FrequencyRuntime>,
    pub system_langid: Option<u16>,
}
#[pymethods]
impl AutoRepeaterRuntime {
    #[new]
    #[pyo3(signature=(frequency,system_langid))]
    fn new(
        frequency: Py<FrequencyRuntime>,
        system_langid: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            frequency,
            system_langid: system_langid.map(strict_u16).transpose()?,
        })
    }
    #[getter]
    fn system_langid(&self) -> Option<u16> {
        self.system_langid
    }
    #[setter]
    fn set_system_langid(&mut self, v: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.system_langid = v.map(strict_u16).transpose()?;
        Ok(())
    }
}
#[pyclass(module = "cs705.icf.settings.extended")]
pub struct HomeChannelRuntime {
    #[pyo3(get, set)]
    pub frequency: Py<FrequencyRuntime>,
    #[pyo3(get, set)]
    pub current: Py<PyBytes>,
    pub show_both_864614: u32,
}
#[pymethods]
impl HomeChannelRuntime {
    #[new]
    fn new(
        frequency: Py<FrequencyRuntime>,
        current: Py<PyBytes>,
        show_both_864614: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            frequency,
            current,
            show_both_864614: strict_u32(show_both_864614)?,
        })
    }
    #[getter]
    fn show_both_864614(&self) -> u32 {
        self.show_both_864614
    }
    #[setter]
    fn set_show_both_864614(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        self.show_both_864614 = strict_u32(v)?;
        Ok(())
    }
}
#[pyclass(module = "cs705.icf.settings.extended")]
pub struct HomeDrRuntime {
    #[pyo3(get, set)]
    pub frequency: Py<FrequencyRuntime>,
    #[pyo3(get, set)]
    pub source: Py<PyBytes>,
    #[pyo3(get, set)]
    pub current: Py<PyBytes>,
}
#[pymethods]
impl HomeDrRuntime {
    #[new]
    fn new(frequency: Py<FrequencyRuntime>, source: Py<PyBytes>, current: Py<PyBytes>) -> Self {
        Self {
            frequency,
            source,
            current,
        }
    }
}

enum SharedInput {
    Display,
    Cancel,
    Popup,
    Next,
    Previous,
    Text(Py<MutableBytes>),
    Selected(Py<Selection>),
}
macro_rules! shared_input {
    ($name:ident) => {
        #[pyclass(module = "cs705.icf.settings.extended", frozen)]
        pub struct $name {
            value: SharedInput,
        }
        impl $name {
            pub fn with_core<R>(
                &self,
                py: Python<'_>,
                f: impl FnOnce(core::$name<'_>) -> PyResult<R>,
            ) -> PyResult<R> {
                match &self.value {
                    SharedInput::Display => f(core::$name::Display),
                    SharedInput::Cancel => f(core::$name::Cancel),
                    SharedInput::Popup => f(core::$name::Popup),
                    SharedInput::Next => f(core::$name::Next),
                    SharedInput::Previous => f(core::$name::Previous),
                    SharedInput::Text(source) => f(core::$name::Text(&mut *source.get().take()?)),
                    SharedInput::Selected(selection) => {
                        let value = selection.try_borrow(py)?.inner;
                        f(core::$name::Selected(value))
                    }
                }
            }
        }
        #[pymethods]
        #[allow(non_snake_case)]
        impl $name {
            #[classattr]
            fn Display() -> Self {
                Self {
                    value: SharedInput::Display,
                }
            }
            #[classattr]
            fn Cancel() -> Self {
                Self {
                    value: SharedInput::Cancel,
                }
            }
            #[classattr]
            fn Popup() -> Self {
                Self {
                    value: SharedInput::Popup,
                }
            }
            #[classattr]
            fn Next() -> Self {
                Self {
                    value: SharedInput::Next,
                }
            }
            #[classattr]
            fn Previous() -> Self {
                Self {
                    value: SharedInput::Previous,
                }
            }
            #[staticmethod]
            fn Text(source: Py<MutableBytes>) -> Self {
                Self {
                    value: SharedInput::Text(source),
                }
            }
            #[staticmethod]
            fn Selected(selection: Py<Selection>) -> Self {
                Self {
                    value: SharedInput::Selected(selection),
                }
            }
            #[getter]
            fn source(&self, py: Python<'_>) -> Option<Py<MutableBytes>> {
                match &self.value {
                    SharedInput::Text(value) => Some(value.clone_ref(py)),
                    _ => None,
                }
            }
            #[getter]
            fn selection(&self, py: Python<'_>) -> Option<Py<Selection>> {
                match &self.value {
                    SharedInput::Selected(value) => Some(value.clone_ref(py)),
                    _ => None,
                }
            }
            #[getter]
            fn kind(&self) -> &'static str {
                match self.value {
                    SharedInput::Display => "Display",
                    SharedInput::Cancel => "Cancel",
                    SharedInput::Popup => "Popup",
                    SharedInput::Next => "Next",
                    SharedInput::Previous => "Previous",
                    SharedInput::Text(_) => "Text",
                    SharedInput::Selected(_) => "Selected",
                }
            }
        }
    };
}
shared_input!(HomeDrMemoryInput);
shared_input!(ProgramScanInput);

/// The one mutable DWORD consumed by the DR runner; no copied in/out scalar.
#[pyclass(module = "cs705.icf.settings.extended", frozen)]
pub struct DrNodeFlags {
    pub owner: Owner<u32>,
}
#[pymethods]
impl DrNodeFlags {
    #[new]
    fn new(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            owner: Owner::new(strict_u32(value)?),
        })
    }
    #[getter]
    fn value(&self) -> PyResult<u32> {
        self.owner.with_ref(|v| Ok(*v))
    }
    #[setter]
    fn set_value(&self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_u32(v)?;
        self.owner.with_mut(|v| {
            *v = value;
            Ok(())
        })
    }
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<FixedEdgeEffect>()?;
    m.add_class::<AutoRepeaterInvocation>()?;
    m.add_class::<AutoRepeaterEffect>()?;
    m.add_class::<DrEffect>()?;
    m.add_class::<HomeGroupEffect>()?;
    m.add_class::<HomeChannelEffect>()?;
    m.add_class::<HomeDrEffect>()?;
    m.add_class::<ProgramScanEffect>()?;
    m.add_class::<HomeVfoEffect>()?;
    m.add_class::<HomeGroupCatalog>()?;
    m.add_class::<FrequencyRuntime>()?;
    m.add_class::<AutoRepeaterRuntime>()?;
    m.add_class::<HomeChannelRuntime>()?;
    m.add_class::<HomeDrRuntime>()?;
    m.add_class::<HomeDrMemoryInput>()?;
    m.add_class::<ProgramScanInput>()?;
    m.add_class::<DrNodeFlags>()?;
    Ok(())
}
