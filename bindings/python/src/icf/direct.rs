//! Compile-time expansion into ordinary, individually named native calls.
macro_rules! native_result {
    (fallible, $value:expr) => {
        $value.map_err(crate::errors::icf)?
    };
    (plain, $value:expr) => {
        $value
    };
}
pub(crate) use native_result;

macro_rules! direct_methods {
    ($class:ident existing { $($existing:tt)* } methods { $($name:ident($($arg:ident: $ty:ty),*) [$access:ident, $result:ident];)* }) => {
        #[pyo3::pymethods]
        impl $class {
            $($existing)*
            $(fn $name<'py>(&self, py: pyo3::Python<'py>, $($arg: &pyo3::Bound<'_, pyo3::PyAny>),*) -> pyo3::PyResult<pyo3::Bound<'py, pyo3::PyAny>> {
                $(let $arg = <$ty as crate::icf::values::Input>::input($arg)?;)*
                let output = self.$access(|core| {
                    let output = crate::icf::direct::native_result!($result, core.$name($($arg),*));
                    Ok(crate::icf::values::Capture::capture(output))
                })?;
                crate::icf::values::Output::output(output, py)
            })*
        }
    };
}
pub(crate) use direct_methods;
