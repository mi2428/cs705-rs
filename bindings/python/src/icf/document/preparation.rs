//! The actual dry-run/live preparation driver, with observable failed prefixes.
use super::{Document, Session, Target, values};
use crate::{conversion::*, errors};
use cs705_core::icf::document as core;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

const STAGES: [core::ClonePreparationStage; 6] = [
    core::ClonePreparationStage::NotStarted,
    core::ClonePreparationStage::CurrentStage9,
    core::ClonePreparationStage::PrivateCopy,
    core::ClonePreparationStage::PrivateSlot10,
    core::ClonePreparationStage::PrivateMetadata,
    core::ClonePreparationStage::Ready,
];
#[pyclass(frozen, module = "cs705.icf.document")]
pub struct ClonePreparation {
    pub target: Target<core::ClonePreparation>,
}
impl ClonePreparation {
    pub fn from_core(value: core::ClonePreparation) -> Self {
        Self {
            target: Target::new(value),
        }
    }
    pub fn snapshot(value: &core::ClonePreparation) -> core::ClonePreparation {
        core::ClonePreparation {
            stage: value.stage,
            image: value.image.clone(),
            stage9: value.stage9,
            correction: value.correction,
            metadata_complete: value.metadata_complete,
        }
    }
}
#[pymethods]
impl ClonePreparation {
    #[new]
    fn new() -> Self {
        Self::from_core(core::ClonePreparation::default())
    }
    fn prepare(
        &self,
        py: Python<'_>,
        document: &Document,
        session: &Session,
        reset: Py<crate::icf::normalizer::ResetState>,
    ) -> PyResult<()> {
        let mut reset = reset.get().take()?;
        let mut preparation = self.target.take()?;
        let mut document = document.take()?;
        let session = session.take()?;
        let result = py.detach(move || preparation.prepare(&mut document, &session, &mut reset));
        result.map_err(errors::icf)
    }
    #[getter]
    fn stage<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stage = self.target.with_ref(|p| Ok(p.stage))?;
        values::enum_out(
            py,
            "cs705.icf.document",
            "ClonePreparationStage",
            STAGES
                .iter()
                .position(|s| *s == stage)
                .expect("native stage variant"),
        )
    }
    #[setter]
    fn set_stage(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let stage = *STAGES
            .get(values::enum_in(
                value,
                "cs705.icf.document",
                "ClonePreparationStage",
            )?)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err("invalid ClonePreparationStage")
            })?;
        self.target.with_mut(|p| {
            p.stage = stage;
            Ok(())
        })
    }
    #[getter]
    fn image<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.target.with_ref(|p| Ok(PyBytes::new(py, &p.image)))
    }
    #[setter]
    fn set_image(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bytes(value)?.to_vec();
        self.target.with_mut(|p| {
            p.image = value;
            Ok(())
        })
    }
    #[getter]
    fn metadata_complete(&self) -> PyResult<bool> {
        self.target.with_ref(|p| Ok(p.metadata_complete))
    }
    #[setter]
    fn set_metadata_complete(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = strict_bool(value)?;
        self.target.with_mut(|p| {
            p.metadata_complete = value;
            Ok(())
        })
    }
    fn stage9_snapshot(&self, py: Python<'_>) -> PyResult<Option<crate::icf::stage9::Report>> {
        let report = self.target.with_ref(|p| Ok(p.stage9))?;
        report
            .map(|r| crate::icf::stage9::Report::from_core(py, r))
            .transpose()
    }
    fn set_stage9(
        &self,
        py: Python<'_>,
        report: Option<Py<crate::icf::stage9::Report>>,
    ) -> PyResult<()> {
        let report = report.map(|r| r.try_borrow(py)?.to_core(py)).transpose()?;
        self.target.with_mut(|p| {
            p.stage9 = report;
            Ok(())
        })
    }
    fn correction_snapshot(
        &self,
        py: Python<'_>,
    ) -> PyResult<Option<crate::icf::normalizer::ResetReport>> {
        let report = self.target.with_ref(|p| Ok(p.correction))?;
        report
            .map(|r| crate::icf::normalizer::ResetReport::from_core(py, r))
            .transpose()
    }
    fn set_correction(
        &self,
        py: Python<'_>,
        report: Option<Py<crate::icf::normalizer::ResetReport>>,
    ) -> PyResult<()> {
        let report = report.map(|r| r.try_borrow(py)?.to_core(py)).transpose()?;
        self.target.with_mut(|p| {
            p.correction = report;
            Ok(())
        })
    }
}
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    values::add_enum(
        m,
        "cs705.icf.document",
        "ClonePreparationStage",
        &[
            "NotStarted",
            "CurrentStage9",
            "PrivateCopy",
            "PrivateSlot10",
            "PrivateMetadata",
            "Ready",
        ],
    )?;
    m.add_class::<ClonePreparation>()
}
