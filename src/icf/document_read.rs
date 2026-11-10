//! Native read controller and borrowed-result document callback.
//!
//! The probe and acquisition are separate port operations. The last successful
//! probe supplies identity, profile and echo mode; acquisition starts with E2,
//! not another E0. Failed acquisition bytes remain on the outcome and never
//! replace document current, baseline or path.

use super::{
    CloneControls, CloneError, DefaultSource, Document, Error, InitialBoundary, InitialRequest,
    InitialState, Session, ValidationReport, ValidationSource, invalid, io,
};
use crate::clone::{self, Operation, Outcome, SessionConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloneReadStop {
    Cancelled,
    Busy,
    ProbeFailed,
    ContextMismatch,
    ReadFailed,
    Completed,
}
pub enum CloneReadStep {
    Query(clone::Session),
    Sync(u32),
    Read(clone::Session),
    Apply,
    Stopped(CloneReadStop),
}
pub enum CloneReadReply {
    Continue,
    Probe(Outcome),
    Read(Outcome),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Start,
    Probe,
    Compare,
    Reprobe,
    Read,
    Apply,
    End,
    Failed,
}

pub struct CloneRead {
    config: SessionConfig,
    prompt_result: i32,
    phase: Phase,
    pub initial: InitialState,
    pub outcomes: Vec<Outcome>,
    pub validation: Option<ValidationReport>,
    pub selector: u8,
    pub fc: u8,
    pub fd: u8,
    matched_before_sync: bool,
}
impl CloneRead {
    pub fn new(config: SessionConfig, prompt_result: i32) -> Result<Self, CloneError> {
        clone::Session::query(config.clone())?;
        Ok(Self {
            config,
            prompt_result,
            phase: Phase::Start,
            initial: InitialState::default(),
            outcomes: Vec::new(),
            validation: None,
            selector: 0,
            fc: 1,
            fd: 0,
            matched_before_sync: false,
        })
    }
    pub fn advance(
        &mut self,
        session: &Session,
        controls: CloneControls,
        reply: CloneReadReply,
    ) -> Result<CloneReadStep, CloneError> {
        let result = self.advance_inner(session, controls, reply);
        if result.is_err() {
            self.phase = Phase::Failed;
        }
        result
    }
    fn stop(&mut self, reason: CloneReadStop) -> CloneReadStep {
        self.phase = Phase::End;
        CloneReadStep::Stopped(reason)
    }
    fn advance_inner(
        &mut self,
        session: &Session,
        controls: CloneControls,
        reply: CloneReadReply,
    ) -> Result<CloneReadStep, CloneError> {
        match (self.phase, reply) {
            (Phase::Start, CloneReadReply::Continue) => {
                if self.prompt_result == 2 {
                    return Ok(self.stop(CloneReadStop::Cancelled));
                }
                if controls.controller_busy != 0 {
                    return Ok(self.stop(CloneReadStop::Busy));
                }
                self.initial.saved_context = Some(session.context.fields());
                self.initial.saved_selector = 0;
                self.fc = 1;
                self.fd = 0;
                let mut config = self.config.clone();
                config.expected_context = None;
                config.saved_selector = 0;
                config.flag57 = controls.form_flag57;
                self.phase = Phase::Probe;
                Ok(CloneReadStep::Query(clone::Session::query(config)?))
            }
            (Phase::Probe, CloneReadReply::Probe(out)) => {
                if out.operation != Operation::Query {
                    return Err(invalid("native read requires actual query outcome").into());
                }
                let failed = out.result_code() != 0;
                if let Some(tail) = out.identity_gate.cached_tail {
                    self.selector = tail[1];
                }
                if !failed {
                    self.matched_before_sync = out
                        .identity
                        .as_ref()
                        .ok_or(invalid("successful read probe has no identity"))?
                        .context()?
                        == session.context.fields();
                }
                self.outcomes.push(out);
                if failed {
                    return Ok(self.stop(CloneReadStop::ProbeFailed));
                }
                if controls.controller_busy != 0 {
                    return Ok(self.stop(CloneReadStop::Busy));
                }
                self.phase = Phase::Compare;
                Ok(CloneReadStep::Sync(0x2a))
            }
            (Phase::Compare, CloneReadReply::Continue) => {
                if !self.matched_before_sync {
                    let fields = self
                        .outcomes
                        .last()
                        .and_then(|o| o.identity.as_ref())
                        .ok_or(invalid("read probe identity unavailable"))?
                        .context()?;
                    if fields
                        != self
                            .initial
                            .saved_context
                            .ok_or(invalid("saved read context unavailable"))?
                    {
                        return Ok(self.stop(CloneReadStop::ContextMismatch));
                    }
                    self.fc = 0;
                    self.fd = 1;
                    if controls.controller_busy != 0 {
                        return Ok(self.stop(CloneReadStop::Busy));
                    }
                    let mut config = self.config.clone();
                    config.expected_context = self.initial.saved_context;
                    config.saved_selector = self.initial.saved_selector;
                    config.flag57 = controls.form_flag57;
                    self.phase = Phase::Reprobe;
                    return Ok(CloneReadStep::Query(clone::Session::query(config)?));
                }
                self.start_read(controls)
            }
            (Phase::Reprobe, CloneReadReply::Probe(out)) => {
                if out.operation != Operation::Query {
                    return Err(invalid("native read requires query outcome").into());
                }
                let failed = out.result_code() != 0;
                if let Some(tail) = out.identity_gate.cached_tail {
                    self.selector = tail[1];
                }
                self.outcomes.push(out);
                if failed {
                    return Ok(self.stop(CloneReadStop::ProbeFailed));
                }
                if controls.controller_busy != 0 {
                    return Ok(self.stop(CloneReadStop::Busy));
                }
                self.start_read(controls)
            }
            (Phase::Read, CloneReadReply::Read(out)) => {
                if out.operation != Operation::Read {
                    return Err(invalid("native read requires actual read outcome").into());
                }
                self.outcomes.push(out);
                let out = self
                    .outcomes
                    .last_mut()
                    .ok_or(invalid("native read outcome unavailable"))?;
                if out.result_code() == 0
                    && !crate::icf::format::validate_image(
                        &out.image,
                        crate::icf::format::Layout::from_map_revision(u32::from(self.selector)),
                    )?
                {
                    out.failure = Some(clone::Failure::ImageValidation);
                }
                self.phase = Phase::Apply;
                Ok(CloneReadStep::Apply)
            }
            _ => Err(invalid("unexpected native read state or reply").into()),
        }
    }

    fn start_read(&mut self, controls: CloneControls) -> Result<CloneReadStep, CloneError> {
        if controls.controller_busy != 0 {
            return Ok(self.stop(CloneReadStop::Busy));
        }
        let mut config = self.config.clone();
        config.expected_context = if self.fc == 0 {
            Some(
                self.initial
                    .saved_context
                    .ok_or(invalid("saved read context unavailable"))?,
            )
        } else {
            None
        };
        config.saved_selector = self.initial.saved_selector;
        config.flag57 = controls.form_flag57;
        self.phase = Phase::Read;
        Ok(CloneReadStep::Read(clone::Session::read_after_query(
            config,
            self.selector.try_into()?,
            self.outcomes
                .last()
                .ok_or(invalid("successful read probe unavailable"))?,
        )?))
    }

    pub fn finish_document(
        &mut self,
        doc: &mut Document,
        session: &mut Session,
        source: Option<DefaultSource<'_>>,
        validation: ValidationSource<'_>,
        boundary: impl FnMut(InitialBoundary, &mut Document, &mut Session) -> Result<(), Error>,
    ) -> Result<CloneReadStop, CloneError> {
        if self.phase != Phase::Apply {
            return Err(invalid("native read callback is not pending").into());
        }
        self.phase = Phase::Failed;
        let outcome = self
            .outcomes
            .last()
            .ok_or(invalid("native read outcome unavailable"))?;
        self.validation = doc.receive_clone_read(
            session,
            &self.initial,
            self.selector,
            outcome,
            source,
            validation,
            boundary,
        )?;
        let stop = if outcome.result_code() == 0 {
            CloneReadStop::Completed
        } else {
            CloneReadStop::ReadFailed
        };
        self.phase = Phase::End;
        Ok(stop)
    }
}

impl Document {
    /// 4D2BC: borrow R24 through conversion and copy; never consume a synthetic
    /// success flag or replace a failed outcome with a completed image.
    #[allow(
        clippy::too_many_arguments,
        reason = "The callback borrows independent saved state, result, assets and UI sources."
    )]
    pub fn receive_clone_read(
        &mut self,
        session: &mut Session,
        initial: &InitialState,
        selector: u8,
        outcome: &Outcome,
        source: Option<DefaultSource<'_>>,
        validation: ValidationSource<'_>,
        mut boundary: impl FnMut(InitialBoundary, &mut Document, &mut Session) -> Result<(), Error>,
    ) -> Result<Option<ValidationReport>, Error> {
        if outcome.operation != Operation::Read {
            return Err(invalid("read callback requires actual read outcome"));
        }
        if outcome.result_code() != 0 {
            boundary(InitialBoundary::Sync(0x11), self, session)?;
            return self.validate_views(session, validation).map(Some);
        }
        let fields = initial
            .saved_context
            .ok_or(invalid("saved read context unavailable"))?;
        let mode = if fields == session.context.fields() {
            0
        } else {
            2
        };
        self.adopt_initial_context(
            session,
            fields,
            InitialRequest {
                mode,
                notification: 0,
                additional: 0,
            },
            Some(super::initial::InitialImage::Current),
            source,
            &mut boundary,
        )?;
        self.current = io::convert(&outcome.image, selector, 6, session.defaults()?)?;
        self.reload_after_conversion(session)?;
        boundary(InitialBoundary::Sync(0x25), self, session)?;
        self.target_revision = u32::from(selector);
        self.dirty = 1;
        boundary(InitialBoundary::Title, self, session)?;
        self.path.clear();
        boundary(InitialBoundary::Title, self, session)?;
        boundary(InitialBoundary::Rebuild, self, session)?;
        boundary(InitialBoundary::ReleaseView, self, session)?;
        boundary(InitialBoundary::Sync(0x25), self, session)?;
        Ok(None)
    }
}
