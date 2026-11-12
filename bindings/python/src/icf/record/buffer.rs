//! Fixed-size live bytes, borrowing the canonical record for every operation.
use super::DecodedRecord;
use crate::conversion::{strict_bytes, strict_isize, strict_u8, strict_usize};
use pyo3::{
    PyTraverseError, PyVisit,
    prelude::*,
    types::{PyBytes, PySlice, PySliceMethods, slice::PySliceIndices},
};

enum Index {
    One(usize),
    Slice(PySliceIndices),
}
fn index(value: &Bound<'_, PyAny>, length: usize) -> PyResult<Index> {
    if let Ok(slice) = value.cast::<PySlice>() {
        for field in ["start", "stop", "step"] {
            let part = slice.getattr(field)?;
            if !part.is_none() {
                strict_isize(&part)?;
            }
        }
        return Ok(Index::Slice(slice.indices(length as isize)?));
    }
    let at = strict_isize(value)?;
    let at = if at < 0 { length as isize + at } else { at };
    if at < 0 || at as usize >= length {
        return Err(pyo3::exceptions::PyIndexError::new_err(
            "byte index outside decoded record",
        ));
    }
    Ok(Index::One(at as usize))
}

#[pyclass(module = "cs705.icf.record", frozen)]
pub struct RecordBytes {
    parent: Py<DecodedRecord>,
}
impl RecordBytes {
    pub(crate) fn from_parent(parent: Py<DecodedRecord>) -> PyResult<Self> {
        parent.get().with_mut(|h| {
            let _ = h.as_mut_bytes();
            Ok(())
        })?;
        Ok(Self { parent })
    }
}
#[pymethods]
impl RecordBytes {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.parent)
    }
    fn __len__(&self) -> PyResult<usize> {
        self.parent.get().with_ref(|h| Ok(h.as_bytes().len()))
    }
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.parent
            .get()
            .with_ref(|h| Ok(PyBytes::new(py, h.as_bytes())))
    }
    fn __getitem__<'py>(
        &self,
        py: Python<'py>,
        key: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let key = index(key, self.__len__()?)?;
        self.parent.get().with_ref(|h| match key {
            Index::One(at) => Ok(h.as_bytes()[at].into_pyobject(py)?.into_any()),
            Index::Slice(s) => {
                let bytes: Vec<_> = (0..s.slicelength)
                    .map(|i| h.as_bytes()[(s.start + s.step * i as isize) as usize])
                    .collect();
                Ok(PyBytes::new(py, &bytes).into_any())
            }
        })
    }
    fn __setitem__(&self, key: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        match index(key, self.__len__()?)? {
            Index::One(at) => {
                let value = strict_u8(value)?;
                self.parent.get().with_mut(|h| {
                    h.as_mut_bytes()[at] = value;
                    Ok(())
                })
            }
            Index::Slice(s) => {
                let value = strict_bytes(value)?;
                if value.len() != s.slicelength {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "decoded record slice assignment cannot change length",
                    ));
                }
                self.parent.get().with_mut(|h| {
                    for (i, byte) in value.iter().enumerate() {
                        h.as_mut_bytes()[(s.start + s.step * i as isize) as usize] = *byte;
                    }
                    Ok(())
                })
            }
        }
    }
    fn write(&self, start: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let start = strict_usize(start)?;
        let value = strict_bytes(value)?;
        self.parent.get().with_mut(|h| {
            let end = start.checked_add(value.len()).ok_or_else(|| {
                pyo3::exceptions::PyIndexError::new_err("decoded record write overflow")
            })?;
            h.as_mut_bytes()
                .get_mut(start..end)
                .ok_or_else(|| {
                    pyo3::exceptions::PyIndexError::new_err("write outside decoded record")
                })?
                .copy_from_slice(value);
            Ok(())
        })
    }
}
pub(crate) fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<RecordBytes>()
}

#[cfg(test)]
mod checks {
    use super::*;
    use crate::scoped::Frame;
    use pyo3::types::PyDict;
    #[test]
    fn record_bytes_live_owner_and_checked_scopes() {
        Python::initialize();
        Python::attach(|py| {
            let parent = Py::new(
                py,
                DecodedRecord::from_core(cs705_core::icf::record::DecodedRecord::from_bytes(
                    [0; 616],
                )),
            )
            .unwrap();
            let view =
                Py::new(py, RecordBytes::from_parent(parent.clone_ref(py)).unwrap()).unwrap();
            let locals = PyDict::new(py);
            locals.set_item("v", &view).unwrap();
            py.run(c"assert len(v)==616\nv[0]=2\nv[-1]=255\nv[1:5]=b'abcd'\nv[5:1:-1]=b'WXYZ'\nassert v[1:6]==b'aZYXW'\nassert v[::-1][0]==255\nfor key,value in [(True,1),(0,True),(0,256),(slice(0,2),b'x'),(slice(True,2),b'x')]:\n before=v.to_bytes()\n try: v[key]=value\n except (TypeError,ValueError,OverflowError): pass\n else: raise AssertionError('invalid write accepted')\n assert v.to_bytes()==before", None, Some(&locals)).unwrap();
            assert_eq!(
                parent
                    .get()
                    .with_ref(|h| Ok((h.as_bytes()[0], h.as_bytes()[615])))
                    .unwrap(),
                (2, 255)
            );
            let taken = parent.get().take().unwrap();
            assert!(view.get().__len__().is_err());
            drop(taken);
            let _consumed = parent.get().take().unwrap().consume();
            assert!(view.get().__len__().is_err());
            let mut native = cs705_core::icf::record::DecodedRecord::from_bytes([0; 616]);
            Frame::with_ref(py, &native, |loan| {
                let parent = Py::new(py, DecodedRecord::from_loan(loan))?;
                assert!(RecordBytes::from_parent(parent).is_err());
                Ok(())
            })
            .unwrap();
            let saved = Frame::with_mut(py, &mut native, |loan| {
                let parent = Py::new(py, DecodedRecord::from_loan(loan))?;
                let view = RecordBytes::from_parent(parent)?;
                view.write(
                    &0usize.into_pyobject(py)?.into_any(),
                    PyBytes::new(py, b"X").as_any(),
                )?;
                Ok(view)
            })
            .unwrap();
            assert_eq!(native.as_bytes()[0], b'X');
            assert!(
                saved
                    .__len__()
                    .unwrap_err()
                    .is_instance_of::<pyo3::exceptions::PyReferenceError>(py)
            );
        });
    }
}
