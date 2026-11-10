//! Native clone-write document gates, distinct from file Save and raw protocol.
//! Each yielded query must run once on the same prepared transport sequence.
//! Sync requests expose a live boundary; this module does not deliver OS events.

use super::{Document, Error, Session, ValidationReport, ValidationSource, invalid, io};
use crate::{
    clone::{self, IMAGE_CAPACITY, Operation, Outcome, Profile, SessionConfig},
    icf::{normalizer, stage9},
};
use std::fmt;

fn signed_positive(count: u32) -> bool {
    count as i32 > 0
}

#[derive(Debug)]
pub enum CloneError {
    Image(Error),
    Protocol(clone::Error),
}
impl fmt::Display for CloneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Image(e) => e.fmt(f),
            Self::Protocol(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for CloneError {}
impl From<Error> for CloneError {
    fn from(e: Error) -> Self {
        Self::Image(e)
    }
}
impl From<clone::Error> for CloneError {
    fn from(e: clone::Error) -> Self {
        Self::Protocol(e)
    }
}

/// Read anew after every synchronous request and completed protocol operation.
#[derive(Debug, Clone, Copy)]
pub struct CloneControls {
    pub controller_busy: u32,
    pub wrapper_state: u32,
    pub form_flag57: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloneStop {
    Busy,
    ProbeFailed,
    ContextMismatch,
    InvalidItems,
    WrapperState,
    Declined,
    Completed,
    WriteFailed,
}

pub enum CloneStep {
    /// A synchronous native request, not proof of listener/OS delivery.
    Sync(u32),
    Query(clone::Session),
    /// Only native integer response1 approves; file Yes6 does not.
    Confirm {
        warning: bool,
        selector: u8,
    },
    Write(clone::Session),
    /// Detached-result callback must run before the operation is complete.
    Finalize,
    Stopped(CloneStop),
}
pub enum CloneReply {
    Continue,
    Probe(Outcome),
    Confirmation(i32),
    Written(Outcome),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Start,
    Entry,
    First,
    Compare,
    Notify11,
    Validate,
    Confirm,
    Second,
    Third,
    Write,
    Final,
    End,
    Failed,
}

/// Prefixes remain available on errors. No Save baseline/path commit is made.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClonePreparationStage {
    #[default]
    NotStarted,
    CurrentStage9,
    PrivateCopy,
    PrivateSlot10,
    PrivateMetadata,
    Ready,
}

#[derive(Default)]
pub struct ClonePreparation {
    pub stage: ClonePreparationStage,
    pub image: Vec<u8>,
    pub stage9: Option<stage9::Report>,
    pub correction: Option<normalizer::ResetReport>,
    pub metadata_complete: bool,
}
impl ClonePreparation {
    /// Same local preparation for live and dry-run; target conversion requires
    /// an actual successful probe selector and is intentionally separate.
    pub fn prepare(
        &mut self,
        doc: &mut Document,
        session: &Session,
        reset: &mut normalizer::ResetState,
    ) -> Result<(), Error> {
        self.image = vec![0; IMAGE_CAPACITY];
        self.stage9 = None;
        self.correction = None;
        self.metadata_complete = false;
        self.stage = ClonePreparationStage::CurrentStage9;
        self.stage9 = Some(doc.normalize(session)?);
        self.stage = ClonePreparationStage::PrivateCopy;
        self.image.clone_from(&doc.current);
        // The native AL is not a success gate. Do not call the current-image
        // reset wrapper or emit its queued/synchronous notification sequence.
        self.stage = ClonePreparationStage::PrivateSlot10;
        self.correction = Some(normalizer::reset_operating_state(
            &mut self.image,
            normalizer::ReferenceImage::Separate(session.defaults()?),
            &session.context,
            &session.ranges,
            session.system_langid,
            reset,
        )?);
        self.stage = ClonePreparationStage::PrivateMetadata;
        io::storage_into(&mut self.image)?;
        self.metadata_complete = true;
        self.stage = ClonePreparationStage::Ready;
        Ok(())
    }
}

/// 2753C -> 275C0 -> 27908 -> 279F8, with actual Session outcomes.
pub struct CloneWrite {
    config: SessionConfig,
    mask: u8,
    phase: Phase,
    pub saved_selector: u8,
    pub fc: u8,
    pub fd: u8,
    pub copied_tail: Vec<u8>,
    pub validation: Option<ValidationReport>,
    pub preparation: ClonePreparation,
    pub wire_image: Vec<u8>,
    pub outcomes: Vec<Outcome>,
    pub reset: normalizer::ResetState,
    pub saved_context: Option<clone::ContextFields>,
    pub options: Option<super::CloneOptions>,
    pub active_mask: Option<Vec<std::ops::Range<u32>>>,
    pub information_requested: bool,
}
impl CloneWrite {
    pub fn new(
        config: SessionConfig,
        mask: u8,
        reset: normalizer::ResetState,
    ) -> Result<Self, CloneError> {
        // Validate the real protocol configuration before transport preparation.
        clone::Session::query(config.clone())?;
        Ok(Self {
            config,
            mask,
            phase: Phase::Start,
            saved_selector: 0,
            fc: 0,
            fd: 0,
            copied_tail: Vec::new(),
            validation: None,
            preparation: ClonePreparation::default(),
            wire_image: Vec::new(),
            outcomes: Vec::new(),
            reset,
            saved_context: None,
            options: None,
            active_mask: None,
            information_requested: false,
        })
    }
    fn config(
        &self,
        session: &Session,
        controls: CloneControls,
    ) -> Result<SessionConfig, CloneError> {
        let mut config = self.config.clone();
        config.expected_context = if self.fc != 0 {
            None
        } else if self.fd == 1 {
            Some(
                self.saved_context
                    .ok_or(invalid("saved clone comparison source unavailable"))?,
            )
        } else {
            Some(session.context.fields())
        };
        config.saved_selector = self.saved_selector;
        config.flag57 = controls.form_flag57;
        Ok(config)
    }
    fn stop(&mut self, reason: CloneStop) -> CloneStep {
        self.phase = Phase::End;
        CloneStep::Stopped(reason)
    }
    fn probe(
        &mut self,
        out: Outcome,
        controls: CloneControls,
    ) -> Result<Option<CloneStep>, CloneError> {
        if out.operation != Operation::Query {
            return Err(invalid("clone expected a query outcome").into());
        }
        if let Some(tail) = out.identity_gate.cached_tail {
            self.copied_tail = tail.to_vec();
        }
        let failed = out.failure.is_some();
        let missing_identity = out.identity.is_none();
        self.outcomes.push(out);
        if failed {
            return Ok(Some(self.stop(CloneStop::ProbeFailed)));
        }
        if missing_identity {
            return Err(invalid("successful clone probe has no identity").into());
        }
        if controls.controller_busy != 0 {
            return Ok(Some(self.stop(CloneStop::Busy)));
        }
        Ok(None)
    }
    /// Advance exactly one native boundary. Callers may update live document/Q
    /// state at Sync boundaries; an old snapshot is not substituted on resume.
    /// On error this operation cannot be resumed, and all prior prefixes remain.
    pub fn advance(
        &mut self,
        doc: &mut Document,
        session: &Session,
        controls: CloneControls,
        validation: ValidationSource<'_>,
        reply: CloneReply,
    ) -> Result<CloneStep, CloneError> {
        let result = self.advance_inner(doc, session, controls, validation, reply);
        if result.is_err() {
            self.phase = Phase::Failed;
        }
        result
    }
    fn advance_inner(
        &mut self,
        doc: &mut Document,
        session: &Session,
        controls: CloneControls,
        validation: ValidationSource<'_>,
        reply: CloneReply,
    ) -> Result<CloneStep, CloneError> {
        match (self.phase, reply) {
            (Phase::Start, CloneReply::Continue) => {
                self.phase = Phase::Entry;
                Ok(CloneStep::Sync(0x1b))
            }
            (Phase::Entry, CloneReply::Continue) => {
                if controls.controller_busy != 0 {
                    return Ok(self.stop(CloneStop::Busy));
                }
                self.saved_selector = 0;
                self.fc = 1;
                self.fd = 0;
                self.phase = Phase::First;
                Ok(CloneStep::Query(clone::Session::query(
                    self.config(session, controls)?,
                )?))
            }
            (Phase::First, CloneReply::Probe(out)) => {
                if let Some(stop) = self.probe(out, controls)? {
                    return Ok(stop);
                }
                self.saved_selector = *self
                    .copied_tail
                    .get(1)
                    .ok_or(invalid("clone selector unavailable"))?;
                self.phase = Phase::Compare;
                Ok(CloneStep::Sync(0x2a))
            }
            (Phase::Compare, CloneReply::Continue) => {
                let identity = self
                    .outcomes
                    .last()
                    .and_then(|o| o.identity.as_ref())
                    .ok_or(invalid("clone identity unavailable"))?;
                if identity.context()? != session.context.fields() {
                    return Ok(self.stop(CloneStop::ContextMismatch));
                }
                self.phase = Phase::Notify11;
                Ok(CloneStep::Sync(0x38))
            }
            (Phase::Notify11, CloneReply::Continue) => {
                self.phase = Phase::Validate;
                Ok(CloneStep::Sync(0x11))
            }
            (Phase::Validate, CloneReply::Continue) => {
                let report = doc.validate_views(session, validation)?;
                let errors = signed_positive(report.errors);
                let warning = signed_positive(report.warnings);
                self.validation = Some(report);
                if errors {
                    return Ok(self.stop(CloneStop::InvalidItems));
                }
                if controls.wrapper_state != 0 {
                    return Ok(self.stop(CloneStop::WrapperState));
                }
                self.phase = Phase::Confirm;
                Ok(CloneStep::Confirm {
                    warning,
                    selector: self.saved_selector,
                })
            }
            (Phase::Confirm, CloneReply::Confirmation(response)) => {
                if response != 1 {
                    return Ok(self.stop(CloneStop::Declined));
                }
                self.fc = 0;
                self.phase = Phase::Second;
                Ok(CloneStep::Query(clone::Session::query(
                    self.config(session, controls)?,
                )?))
            }
            (Phase::Second, CloneReply::Probe(out)) => {
                if let Some(stop) = self.probe(out, controls)? {
                    return Ok(stop);
                }
                let selector = Profile::try_from(
                    *self
                        .copied_tail
                        .get(1)
                        .ok_or(invalid("clone selector unavailable"))?,
                )?;
                self.preparation.prepare(doc, session, &mut self.reset)?;
                self.wire_image = io::convert(
                    &self.preparation.image,
                    6,
                    selector.selector(),
                    session.defaults()?,
                )?;
                self.phase = Phase::Third;
                Ok(CloneStep::Query(clone::Session::query(
                    self.config(session, controls)?,
                )?))
            }
            (Phase::Third, CloneReply::Probe(out)) => {
                if let Some(stop) = self.probe(out, controls)? {
                    return Ok(stop);
                }
                let selector = Profile::try_from(
                    *self
                        .copied_tail
                        .get(1)
                        .ok_or(invalid("clone selector unavailable"))?,
                )?;
                let write = clone::Session::write(
                    self.config(session, controls)?,
                    selector,
                    self.options
                        .as_ref()
                        .map_or(self.mask, super::CloneOptions::mask),
                    &self.wire_image,
                )?;
                self.active_mask = Some(clone::write_blocks_with_cap(
                    selector.selector(),
                    self.options
                        .as_ref()
                        .map_or(self.mask, super::CloneOptions::mask),
                    self.config.block_cap as u32,
                )?);
                self.phase = Phase::Write;
                Ok(CloneStep::Write(write))
            }
            (Phase::Write, CloneReply::Written(out)) => {
                if out.operation != Operation::Write {
                    return Err(invalid("clone expected a write outcome").into());
                }
                self.outcomes.push(out);
                self.phase = Phase::Final;
                Ok(CloneStep::Finalize)
            }
            _ => Err(invalid("unexpected clone state or reply").into()),
        }
    }

    /// 27ACC: display is gated by actual result/live W3C, but mask release and
    /// live wrapper reset follow every NORMAL callback return. A display error
    /// keeps the mask and skips the later clear; no Save commit is performed.
    pub fn finalize(
        &mut self,
        controls: &mut CloneControls,
        request_information: impl FnOnce(&mut Self, &mut CloneControls) -> Result<(), Error>,
    ) -> Result<CloneStop, CloneError> {
        if self.phase != Phase::Final {
            return Err(invalid("clone final callback is not pending").into());
        }
        let failed = self
            .outcomes
            .last()
            .ok_or(invalid("clone write outcome unavailable"))?
            .result_code()
            != 0;
        if !failed && controls.wrapper_state == 0 {
            self.information_requested = true;
            if let Err(error) = request_information(self, controls) {
                self.phase = Phase::Failed;
                return Err(error.into());
            }
        }
        drop(self.active_mask.take());
        controls.wrapper_state = 0;
        self.phase = Phase::End;
        Ok(if failed {
            CloneStop::WriteFailed
        } else {
            CloneStop::Completed
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_count_gate_is_signed_positive_not_nonzero() {
        for (count, expected) in [
            (0, false),
            (1, true),
            (0x7fff_ffff, true),
            (0x8000_0000, false),
            (u32::MAX, false),
        ] {
            assert_eq!(super::signed_positive(count), expected);
        }
    }
}
