//! Initial COM/ICF adoption. Native delivery is an explicit fallible boundary.

use super::{DefaultSource, Document, Error, Session, invalid};
use crate::{
    clone::{ContextFields, Identity},
    icf::{context::Context, format, normalizer::ReferenceImage, stage9},
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InitialState {
    pub saved_context: Option<ContextFields>,
    pub saved_selector: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct InitialRequest {
    pub mode: u32,
    pub notification: u32,
    pub additional: u32,
}
impl InitialRequest {
    /// 51680/event 2A: normal comparison is 0/2; scalar1 is canonicalized before
    /// 23758. The old raw-kind predicate is owned by the receiver, not a packet.
    pub fn information(
        session: &Session,
        identity: &Identity,
        scalar1: u32,
    ) -> Result<Option<Self>, Error> {
        if identity
            .context()
            .map_err(|_| invalid("initial identity context unavailable"))?
            == session.context.fields()
        {
            return Ok(None);
        }
        Ok(Some(Self {
            mode: 2,
            notification: u32::from(scalar1 != 0),
            additional: u32::from(session.context.fields().0[0] as i32 >= 16),
        }))
    }
}

pub(super) enum InitialImage<'a> {
    Current,
    Separate(&'a mut [u8]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialBoundary {
    Reconfigure,
    Additional,
    Rebuild,
    ReleaseView,
    Sync(u32),
    Title,
}

impl Document {
    /// 23758: low-byte notification saves D28/D40 without adopting live Q.
    /// BL is captured before Q changes and is not recomputed after listeners.
    pub fn receive_initial_identity(
        &mut self,
        session: &mut Session,
        state: &mut InitialState,
        identity: &Identity,
        request: InitialRequest,
        source: Option<DefaultSource<'_>>,
        mut boundary: impl FnMut(InitialBoundary, &mut Document, &mut Session) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let fields = identity
            .context()
            .map_err(|_| invalid("initial identity context unavailable"))?;
        if request.notification as u8 != 0 {
            state.saved_context = Some(fields);
            state.saved_selector = *identity
                .tail
                .get(1)
                .ok_or(invalid("initial selector unavailable"))?;
            return Ok(());
        }
        let reset = self.path.is_empty() && self.dirty == 0;
        let request = InitialRequest {
            additional: u32::from(session.context.fields().0[0] as i32 >= 16),
            ..request
        };
        self.adopt_initial_context(
            session,
            fields,
            request,
            Some(InitialImage::Current),
            source,
            &mut boundary,
        )?;
        if reset {
            self.current = session.defaults()?.to_vec();
            self.baseline.clone_from(&self.current);
            self.dirty = 0;
            boundary(InitialBoundary::Title, self, session)?;
        }
        Ok(())
    }

    pub(super) fn adopt_initial_context(
        &mut self,
        session: &mut Session,
        fields: ContextFields,
        request: InitialRequest,
        working: Option<InitialImage<'_>>,
        source: Option<DefaultSource<'_>>,
        boundary: &mut impl FnMut(InitialBoundary, &mut Document, &mut Session) -> Result<(), Error>,
    ) -> Result<(), Error> {
        session.context.adopt(fields);
        session.regenerate_defaults(
            source.ok_or(invalid(
                "initial adoption requires actual default source fields",
            ))?,
            &self.current,
        )?;
        if request.mode == 2 {
            boundary(InitialBoundary::Reconfigure, self, session)?;
            if let Some(working) = working {
                match working {
                    InitialImage::Current => {
                        self.normalize(session)?;
                    }
                    InitialImage::Separate(working) => {
                        stage9::normalize(
                            working,
                            ReferenceImage::Separate(session.defaults()?),
                            ReferenceImage::Separate(&self.current),
                            ReferenceImage::Separate(&self.current),
                            session.runtime(),
                        )?;
                    }
                }
                if request.additional as u8 != 0 {
                    // The actual derived slot 24=4FB84 is an empty body.
                    boundary(InitialBoundary::Additional, self, session)?;
                }
            }
            boundary(InitialBoundary::Rebuild, self, session)?;
            boundary(InitialBoundary::ReleaseView, self, session)?;
            boundary(InitialBoundary::Sync(0x25), self, session)?;
        }
        Ok(())
    }

    /// O002: convert a separate local using OLD G, then decode/adopt Q, generate
    /// NEW B/G, normalize the local, and finally copy the latest G to current.
    pub fn initial_from_icf(
        &mut self,
        bytes: &[u8],
        path: &[u8],
        session: &mut Session,
        source: Option<DefaultSource<'_>>,
        mut boundary: impl FnMut(InitialBoundary, &mut Document, &mut Session) -> Result<(), Error>,
    ) -> Result<format::Status, Error> {
        let file = format::read_bound(bytes, self.current.clone())?;
        if file.status != format::Status::Success {
            return Ok(file.status);
        }
        let mut working = file
            .convert_to(
                format::Layout::from_map_revision(6),
                Some(session.defaults()?),
            )
            .map_err(|_| invalid("initial ICF conversion source unavailable"))?;
        let mut incoming = Context::before_registry();
        incoming.decode_packed(file.reader.extra_data);
        self.adopt_initial_context(
            session,
            incoming.fields(),
            InitialRequest {
                mode: 2,
                notification: 0,
                additional: 0,
            },
            Some(InitialImage::Separate(&mut working)),
            source,
            &mut boundary,
        )?;
        self.current = session.defaults()?.to_vec();
        self.baseline.clone_from(&self.current);
        self.dirty = 0;
        boundary(InitialBoundary::Title, self, session)?;
        self.directory = path
            .iter()
            .rposition(|b| matches!(b, b'/' | b'\\'))
            .map_or_else(Vec::new, |at| path[..=at].to_vec());
        boundary(InitialBoundary::Sync(0x4a), self, session)?;
        Ok(file.status)
    }
}
