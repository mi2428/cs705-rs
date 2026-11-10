//! Concrete offline document file driver. Factory data are explicit source
//! fields, never an embedded compiled image or a caller-supplied finalized G.
//!
//! Current and baseline contain M working bytes. The file contains only the
//! selected layout's storage prefix; rebuilding links/counts belongs to a copy,
//! not to the live logical row maps. Save has two distinct mutation boundaries:
//!
//! ```text
//! current -> stage9 -> copy + storage metadata -> MapRev -> writer
//!                                                        |
//!        legacy current round-trip + slot08 <--- success -+
//!        path/target/baseline commit follows the required reload
//! ```
//!
//! A writer error retains prior current normalization and any output prefix.
//! A post-writer reload error can leave a completed file beside a changed current
//! and the old baseline. Neither condition authorizes an implicit clean commit.

use super::{Document, Error, invalid};
use crate::{
    clone::{self, IMAGE_CAPACITY, Profile},
    icf::{
        self,
        context::Context,
        format::{self, Layout},
        frequency::{Container, Ranges},
        image::apply_patches,
        normalizer::{self, ReferenceImage},
        rows, stage9,
    },
};
use std::{fmt, io::Write};

/// The selected external source data consumed by24A20. prefix is the raw source
/// asset BEFORE range/map/typed normalization, not a completed/default image G.
/// Only branch-reached patch fields are required. No missing byte becomes zero.
pub struct DefaultSource<'a> {
    pub selector: u8,
    pub prefix: &'a [u8],
    pub japanese_mode_pairs: Option<&'a [u8; 120]>,
    pub restricted_band_stack: Option<&'a [u8; 108]>,
}

/// Global Q/B/G state, separate from any document's current/baseline generation.
/// Q can change without silently refreshing B/G. regenerate_defaults is explicit.
pub struct Session {
    pub context: Context,
    pub ranges: Ranges,
    defaults: Vec<u8>,
    default_selector: Option<u8>,
    pub system_langid: Option<u16>,
    pub previous_reload_guard: u32,
    pub document_flags: [u8; 2],
}
impl Session {
    /// Actual constructor state: an unpopulated B and allocated zeroed G object.
    /// It is NOT a generated factory image; operations requiring G fail until
    /// regenerate_defaults has normally completed the actual recipe below.
    pub fn new(
        context: Context,
        system_langid: Option<u16>,
        previous_reload_guard: u32,
        document_flags: [u8; 2],
    ) -> Self {
        Self {
            context,
            ranges: Ranges::unpopulated(),
            defaults: vec![0; IMAGE_CAPACITY],
            default_selector: None,
            system_langid,
            previous_reload_guard,
            document_flags,
        }
    }
    pub fn defaults(&self) -> Result<&[u8], Error> {
        self.default_selector
            .ok_or(invalid("no completed default generation"))?;
        Ok(&self.defaults)
    }
    pub fn default_selector(&self) -> Option<u8> {
        self.default_selector
    }
    /// Restore an explicitly captured, completed G generation. This is NOT the
    /// source-asset recipe and never regenerates or substitutes Q/B. The caller
    /// supplies its captured source selector, independently of today's live Q.
    pub fn adopt_captured_defaults(&mut self, image: Vec<u8>, selector: u8) -> Result<(), Error> {
        if selector >= 5 || image.len() != IMAGE_CAPACITY {
            return Err(invalid(
                "captured G requires source selector0..4 and0x4997a bytes",
            ));
        }
        icf::Icf::from_memory(image[..0x3c9c0].to_vec())?;
        self.defaults = image;
        self.default_selector = Some(selector);
        Ok(())
    }
    pub(super) fn runtime(&self) -> stage9::Runtime<'_> {
        stage9::Runtime {
            context: &self.context,
            ranges: &self.ranges,
            system_langid: self.system_langid,
            previous_reload_guard: self.previous_reload_guard,
            document_flags: self.document_flags,
        }
    }

    /// Q -> selected source -> DC78 -> original byte-source copy -> early patches
    /// -> F360 -> actual stage8 -> complete stage9 (W=F, C/T=old current).
    /// Errors expose updated Q/B/G prefixes; readiness is published only at end.
    pub fn regenerate_defaults(
        &mut self,
        source: DefaultSource<'_>,
        current: &[u8],
    ) -> Result<stage9::Report, Error> {
        let selector = self.context.source_selector(self.system_langid)?;
        self.default_selector = None;
        self.ranges
            .regenerate(u32::from(selector), &self.context, self.system_langid)?;
        if source.selector != selector {
            return Err(invalid(
                "default source selector does not match captured Q source",
            ));
        }
        for at in 0..0x3c9c0 {
            self.defaults[at] = *source
                .prefix
                .get(at)
                .ok_or(invalid("default source byte unavailable"))?;
        }
        let japanese = self.context.source_selector(self.system_langid)? == 0;
        if japanese {
            apply_patches(
                &mut self.defaults,
                &[(
                    0x370e0,
                    source
                        .japanese_mode_pairs
                        .ok_or(invalid("Japanese mode-pair source unavailable"))?,
                )],
            )?;
        }
        if matches!(selector, 2 | 3) && self.context.effective_f()? == 0 {
            apply_patches(
                &mut self.defaults,
                &[(
                    0x36ed3,
                    source
                        .restricted_band_stack
                        .ok_or(invalid("restricted band-stack source unavailable"))?,
                )],
            )?;
        }
        if self.context.as_bytes()[0x13] == 1 {
            self.defaults[0x39312] = if japanese { 2 } else { 1 };
        }
        normalizer::stage7(
            &mut self.defaults,
            self.ranges.list(if selector == 0 {
                Container::LF8
            } else {
                Container::LD0
            }),
        )?;
        stage8(&mut self.defaults, &self.context, self.system_langid)?;
        let runtime = stage9::Runtime {
            context: &self.context,
            ranges: &self.ranges,
            system_langid: self.system_langid,
            previous_reload_guard: self.previous_reload_guard,
            document_flags: self.document_flags,
        };
        let result = stage9::normalize(
            &mut self.defaults,
            ReferenceImage::Working,
            ReferenceImage::Separate(current),
            ReferenceImage::Separate(current),
            runtime,
        )?;
        self.default_selector = Some(selector);
        Ok(result)
    }
}

/// 4FB00 then4FB20: counts/maps are built before their own later field snapshots.
pub(super) fn stage8(
    image: &mut [u8],
    context: &Context,
    system_langid: Option<u16>,
) -> Result<(), Error> {
    rows::expand_map(image)?;
    crate::repeater::working::expand_counts(image)?;
    rows::expand_ur_count(image)?;
    rows::expand_names(image)?;
    crate::repeater::working::expand_rows(
        image,
        crate::repeater::working::DefaultContext::from_context(context, system_langid)?,
    )?;
    rows::expand_ur_data(image)
}
fn storage(image: &[u8]) -> Result<Vec<u8>, Error> {
    let mut image = image.to_vec();
    storage_into(&mut image)?;
    Ok(image)
}
pub(super) fn storage_into(image: &mut [u8]) -> Result<(), Error> {
    rows::rebuild_metadata(image)?;
    crate::repeater::working::rebuild_storage_into(image)?;
    rows::rebuild_ur_metadata(image)
}
pub(super) fn convert(
    image: &[u8],
    source: u8,
    destination: u8,
    defaults: &[u8],
) -> Result<Vec<u8>, Error> {
    clone::convert_image(
        image,
        Profile::try_from(source).map_err(|_| invalid("invalid source layout"))?,
        Profile::try_from(destination).map_err(|_| invalid("invalid destination layout"))?,
        Some(defaults),
    )
    .map_err(|_| invalid("MapRev conversion source/window unavailable"))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenStatus {
    Cancelled,
    Rejected(format::Status),
    Opened {
        adopted_context: bool,
        foreign_context: bool,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveStatus {
    Saved,
    Cancelled,
    RetrySaveAs,
}

/// One actual typed view descriptor, in50344 traversal order. first/last are
/// inclusive typed indices (normal/call are local sides); count is only the
/// native entry gate. These are source coordinates, never supplied error flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationView {
    pub kind: u32,
    pub group: u32,
    pub first: i32,
    pub last: i32,
    pub count: i32,
    pub include_side1: bool,
    pub show_both: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ValidationReport {
    pub errors: u32,
    pub warnings: u32,
    /// Per-descriptor context5C bits. Bit8 has count precedence over bit4.
    pub flags: Vec<u8>,
}
#[derive(Debug, Clone, Copy)]
pub enum ValidationSource<'a> {
    /// Actual missing native tree, not a substitute for an unknown view source.
    NoTree,
    /// Explicit ordered descriptors from the caller's real view/group source.
    Views(&'a [ValidationView]),
}
pub struct SaveRequest<'a> {
    pub path: Vec<u8>,
    pub target_revision: u32,
    pub legacy_choice: i32,
    pub warning_choice: Option<i32>,
    pub validation: ValidationSource<'a>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveReport {
    pub status: SaveStatus,
    pub validation: ValidationReport,
    pub correction: Option<ResetEffects>,
    pub checksum_written: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetEffects {
    pub correction: normalizer::ResetReport,
    /// Native238C0 requests in order. They are NOT delivered GUI callbacks.
    pub notifications: Vec<icf::special::Notification>,
}

fn view_bad(
    image: &[u8],
    view: ValidationView,
    numeric: bool,
    session: &Session,
) -> Result<bool, Error> {
    use icf::record_image::{self, ChannelFilter, ChannelView};
    if view.count <= 0 {
        return Ok(false);
    }
    if matches!(view.kind, 2 | 3) {
        if (view.kind == 3) != (view.group == 100) {
            return Err(invalid("validation kind and group disagree"));
        }
        return Ok(record_image::channel_bad_count(
            image,
            ChannelView {
                group: view.group,
                first: view.first,
                last: view.last,
                count: view.count,
            },
            ChannelFilter {
                broad: numeric,
                include_side1: view.include_side1,
                count_all: false,
                show_both: view.show_both,
            },
            &session.ranges,
            &session.context,
            1,
        )? != 0);
    }
    for index in view.first..=view.last {
        let mut holder = icf::record::reset_holder(view.kind, index as u32, 1);
        record_image::load_into(image, view.kind, index as u32, &mut holder)?;
        if normalizer::typed_bad(
            image,
            &holder,
            numeric,
            &session.context,
            &session.ranges,
            session.system_langid,
        )? {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Debug)]
pub enum FileError {
    Image(Error),
    Output(std::io::Error),
}
impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Image(e) => e.fmt(f),
            Self::Output(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for FileError {}
impl From<Error> for FileError {
    fn from(value: Error) -> Self {
        Self::Image(value)
    }
}

impl Document {
    /// Slot08 clears document flags before stage8 and stage9 read them. Open and
    /// post-writer legacy reload share this order; failures retain the cleared
    /// flags and any image prefix. Native node/Sync25 delivery is a separate edge.
    pub(crate) fn reload_after_conversion(
        &mut self,
        session: &mut Session,
    ) -> Result<stage9::Report, Error> {
        session.document_flags = [0, 0];
        stage8(&mut self.current, &session.context, session.system_langid)?;
        self.normalize(session)
    }
    pub fn directory(&self) -> &[u8] {
        &self.directory
    }
    pub fn comment_bytes(&self) -> &[u8] {
        &self.comment
    }
    pub fn extra_data(&self) -> u32 {
        self.extra_data
    }
    /// Headless equivalent of50344's ordered typed descriptor pass. Actual data
    /// and shared predicates compute context5C; no error/warning bool is accepted.
    /// GUI resource parsing, painting and native tree-list delivery are not faked.
    pub fn validate_views(
        &self,
        session: &Session,
        source: ValidationSource<'_>,
    ) -> Result<ValidationReport, Error> {
        let mut report = ValidationReport::default();
        if let ValidationSource::Views(views) = source {
            for &view in views {
                let specific = view_bad(&self.current, view, false, session)?;
                let numeric = view_bad(&self.current, view, true, session)?;
                let flags = (u8::from(specific) * 4) | (u8::from(numeric) * 8);
                report.flags.push(flags);
                if flags & 8 != 0 {
                    report.errors = report.errors.wrapping_add(1);
                } else if flags & 4 != 0 {
                    report.warnings = report.warnings.wrapping_add(1);
                }
            }
        }
        Ok(report)
    }
    /// Actual operating-state correction root (slot10). W is this current, not
    /// a private clone image. Requested GUI notifications are not delivered here.
    pub fn reset_operating_state(
        &mut self,
        session: &Session,
        state: &mut normalizer::ResetState,
    ) -> Result<ResetEffects, Error> {
        let correction = normalizer::reset_operating_state(
            &mut self.current,
            ReferenceImage::Separate(session.defaults()?),
            &session.context,
            &session.ranges,
            session.system_langid,
            state,
        )?;
        let mut notifications = Vec::new();
        for (event, synchronous, first) in [
            (0x3c, false, 1),
            (0x39, true, 0),
            (0x27, false, 0),
            (0x38, false, 0),
        ] {
            notifications.push(icf::special::Notification {
                event,
                synchronous,
                first,
                second: 0,
                payload: Vec::new(),
            });
        }
        notifications.extend((2..=101).map(|first| icf::special::Notification {
            event: 0x36,
            synchronous: true,
            first,
            second: 0,
            payload: Vec::new(),
        }));
        Ok(ResetEffects {
            correction,
            notifications,
        })
    }
    /// Serialize a COPY of current with actual storage metadata. No normalize,
    /// Save status, baseline/path commit or filesystem publication is implied.
    pub fn current_bytes(&self, session: &Session) -> Result<Vec<u8>, Error> {
        let saved = storage(&self.current)?;
        format::write_storage(
            &saved[..0x3c9c0],
            Layout::from_map_revision(6),
            &self.comment,
            session.context.encode_packed(),
        )
    }
    /// Obtain the semantic Icf editor without discarding working-tail contents or
    /// original/comment/EtcData. Publication and normalization remain explicit.
    pub fn current_icf(&self) -> Result<icf::Icf<'_>, Error> {
        let mut image = icf::Icf::from_memory(
            self.current
                .get(..0x3c9c0)
                .ok_or(invalid("current storage prefix unavailable"))?
                .to_vec(),
        )?;
        image.source = &self.original;
        image.comment = &self.comment;
        image.extra_data = self.extra_data;
        Ok(image)
    }
    /// Actual current stage9 entry, as used after CSV/field operations. It does
    /// not reset baseline/path/pending or invent a successful GUI callback.
    pub fn normalize(&mut self, session: &Session) -> Result<stage9::Report, Error> {
        stage9::normalize(
            &mut self.current,
            ReferenceImage::Separate(session.defaults()?),
            ReferenceImage::Working,
            ReferenceImage::Working,
            session.runtime(),
        )
    }
    /// 94C20/9514F after the actual file/header/confirmation boundary. Only
    /// Header(1) runs the reader operation, including its ordered post-row work.
    /// Inner cancellation and header rejection still normalize the LIVE current;
    /// file cancellation does not read G or run stage9. An exceptional reader
    /// skips stage9 and retains its prefix. A normal partial/zero-row report does
    /// not reject the reader. Path/baseline/dirty and the common command tail are
    /// not committed here; import directory caches belong to the reader caller.
    pub fn import_csv<R>(
        &mut self,
        session: &Session,
        gate: crate::csv::control::ImportGate,
        reader: impl FnOnce(&mut Self) -> Result<R, Error>,
    ) -> Result<(Option<R>, Option<stage9::Report>), Error> {
        crate::csv::control::import_with_gate(self, gate, reader, |doc| doc.normalize(session))
    }
    /// Headless accepted semantic setting edit plus the common timer/dirty tail.
    /// Native popup parsing/GUI notification is not simulated by this method.
    pub fn edit_setting(
        &mut self,
        name: &str,
        index: usize,
        value: &icf::settings::Value,
    ) -> Result<(), Error> {
        let setting = icf::settings::find(name)?;
        let mut image = self.current_icf()?;
        let result = setting.write(&mut image, index, value);
        let updated = image.memory().to_vec();
        drop(image);
        self.current[..updated.len()].copy_from_slice(&updated);
        result?;
        self.timer_ms = Some(250);
        self.compare_dirty();
        Ok(())
    }
    /// Actual4CA88 headless file operation. Only native prompt result2 cancels;
    /// None is file-dialog cancellation. On profile mismatch, choice6 adopts Q/G,
    /// choice7 imports foreign content with empty path/dirty1,2 cancels. Other
    /// raw MessageBox results continue without adoption, as in the native switch.
    pub fn read(
        &mut self,
        source: Option<&[u8]>,
        path: Vec<u8>,
        prompt_result: i32,
        profile_choice: Option<i32>,
        session: &mut Session,
        default_source: Option<DefaultSource<'_>>,
    ) -> Result<OpenStatus, Error> {
        if prompt_result == 2 {
            return Ok(OpenStatus::Cancelled);
        }
        let Some(source) = source else {
            return Ok(OpenStatus::Cancelled);
        };
        let file = format::read(source)?;
        if file.status != format::Status::Success {
            return Ok(OpenStatus::Rejected(file.status));
        }
        let mut incoming = Context::before_registry();
        incoming.decode_packed(file.reader.extra_data);
        let mismatch = incoming.fields() != session.context.fields();
        let mut adopted = false;
        let mut foreign = false;
        if mismatch {
            match profile_choice.ok_or(invalid("profile mismatch requires the native choice"))? {
                2 => return Ok(OpenStatus::Cancelled),
                6 => {
                    session.context.adopt(incoming.fields());
                    session.regenerate_defaults(
                        default_source.ok_or(invalid(
                            "adopted profile requires actual default source fields",
                        ))?,
                        &self.current,
                    )?;
                    adopted = true;
                }
                7 => foreign = true,
                _ => {}
            }
        }
        let converted = file
            .convert_to(Layout::from_map_revision(6), Some(session.defaults()?))
            .map_err(|_| invalid("open MapRev conversion source/window unavailable"))?;
        self.directory = path
            .iter()
            .rposition(|b| matches!(b, b'/' | b'\\'))
            .map_or_else(Vec::new, |at| path[..=at].to_vec());
        self.path = if foreign { Vec::new() } else { path };
        self.target_revision = file.reader.map_revision;
        self.current = converted;
        self.original = source.to_vec();
        self.comment = file.report.source_comment;
        self.extra_data = file.reader.extra_data;
        self.reload_after_conversion(session)?;
        if foreign {
            self.dirty = 1;
        } else {
            self.baseline.clone_from(&self.current);
            self.dirty = 0;
        }
        Ok(OpenStatus::Opened {
            adopted_context: adopted,
            foreign_context: foreign,
        })
    }
    /// O003 consumes a completed generated G, rather than copying a source asset
    /// directly. Pending name/timer state is not silently reset by New.
    pub fn new_file(&mut self, session: &Session, prompt_result: i32) -> Result<bool, Error> {
        if prompt_result == 2 {
            return Ok(false);
        }
        Ok(self.new_from_defaults(session.defaults()?, prompt_result))
    }
    /// O005/006's actual headless Save body. With no native view tree,50344's
    /// counts are genuinely0/0, so flags=(0,1); no unimplemented validation is
    /// disguised as a successful callback. GUI-tree validation/optional slot10
    /// is a different entry and is not accepted by this headless API.
    /// Write errors retain normalized current and any output prefix; path/target/
    /// baseline commit only after successful write and legacy roundtrip/slot08.
    pub fn save(
        &mut self,
        session: &mut Session,
        path: Vec<u8>,
        target_revision: u32,
        legacy_choice: i32,
        output: &mut impl Write,
    ) -> Result<SaveStatus, FileError> {
        Ok(self
            .save_with_validation(
                session,
                SaveRequest {
                    path,
                    target_revision,
                    legacy_choice,
                    warning_choice: None,
                    validation: ValidationSource::NoTree,
                },
                None,
                output,
            )?
            .status)
    }

    /// Native Save count/choice branches over explicit actual view sources.
    /// Validation errors STILL save without CD; warning Yes alone runs slot10.
    /// A missing reset state is an error only when that branch is reached.
    pub fn save_with_validation(
        &mut self,
        session: &mut Session,
        request: SaveRequest<'_>,
        state: Option<&mut normalizer::ResetState>,
        output: &mut impl Write,
    ) -> Result<SaveReport, FileError> {
        let path = request.path.clone();
        let target = request.target_revision;
        let report = self.save_with_validation_uncommitted(session, request, state, output)?;
        if report.status == SaveStatus::Saved {
            self.accept_saved(path, target);
        }
        Ok(report)
    }

    pub(super) fn save_with_validation_uncommitted(
        &mut self,
        session: &mut Session,
        request: SaveRequest<'_>,
        state: Option<&mut normalizer::ResetState>,
        output: &mut impl Write,
    ) -> Result<SaveReport, FileError> {
        let layout = Layout::from_map_revision(request.target_revision);
        if request.target_revision != 6
            && let choice @ (2 | 7) = request.legacy_choice
        {
            return Ok(SaveReport {
                status: if choice == 2 {
                    SaveStatus::Cancelled
                } else {
                    SaveStatus::RetrySaveAs
                },
                validation: ValidationReport::default(),
                correction: None,
                checksum_written: false,
            });
        }
        let validation = self.validate_views(session, request.validation)?;
        let choice = if validation.errors == 0 && validation.warnings != 0 {
            request.warning_choice.ok_or(invalid(
                "validation warning requires the actual native choice",
            ))?
        } else {
            0
        };
        let correct = validation.errors == 0 && validation.warnings != 0 && choice == 6;
        let checksum = validation.errors == 0 && (validation.warnings == 0 || choice == 6);
        self.normalize(session)?;
        let correction = if correct {
            Some(self.reset_operating_state(
                session,
                state.ok_or(invalid("warning Yes requires actual reset state"))?,
            )?)
        } else {
            None
        };
        let saved = storage(&self.current)?;
        let converted = convert(&saved, 6, layout.selector(), session.defaults()?)?;
        let bytes = format::write_storage_with_checksum(
            &converted[..layout.storage_len()],
            layout,
            &self.comment,
            session.context.encode_packed(),
            checksum,
        )?;
        output.write_all(&bytes).map_err(FileError::Output)?;
        if request.target_revision != 6 {
            let saved = storage(&self.current)?; // Native post-writer current re-copy.
            let downgraded = convert(&saved, 6, layout.selector(), session.defaults()?)?;
            self.current = convert(&downgraded, layout.selector(), 6, session.defaults()?)?;
            self.reload_after_conversion(session)?;
        }
        Ok(SaveReport {
            status: SaveStatus::Saved,
            validation,
            correction,
            checksum_written: checksum,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_fields() -> Vec<u8> {
        let mut b = vec![0; 0x3c9c0];
        b[0x355a1] = 6;
        for g in 0..100 {
            let at = 0x355c0 + 19 * g;
            b[at] = if g == 99 { 255 } else { (g + 1) as u8 };
            b[at + 1..at + 3].fill(255);
            b[at + 3..at + 19].fill(b' ');
        }
        b[0x35d2c..0x36628].fill(255);
        b[0x3662d..0x36821].fill(1);
        b[0x3ab82..0x3bf6e].fill(255);
        b[0x3c0a7..0x3c2ff].fill(255);
        b[0x3c385..0x3c613].fill(255);
        for row in 0..45 {
            let at = icf::special::CompactKind::BandStack.offset(row).unwrap();
            b[at..at + 4].fill(255);
        }
        for row in 0..=10 {
            b[0x36f54 + 36 * row..0x36f58 + 36 * row].fill(255);
        }
        b[0xa60b..0xa60f].fill(255);
        b[0x3c84b] = 255;
        b[0x3c8b9..0x3c8bb].fill(255);
        b[0x399c1..0x399dd].copy_from_slice(b"OPAQUE-SOURCE-FIELD-28-BYTES");
        b[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
        b
    }
    // Independent physical-text fixture, not the production format writer.
    fn literal_file(memory: &[u8], revision: u8, extra: u32) -> Vec<u8> {
        let mut out = format!(
            "40750001\r\n#Comment=LITERAL\r\n#MapRev={revision}\r\n#EtcData={extra:08X}\r\n"
        )
        .into_bytes();
        for (i, block) in memory.chunks(32).enumerate() {
            out.extend_from_slice(format!("{:08X}20", i * 32).as_bytes());
            for byte in block {
                out.extend_from_slice(format!("{byte:02X}").as_bytes());
            }
            out.extend_from_slice(b"\r\n");
        }
        out
    }
    fn session_and_document(fields: &[u8]) -> (Session, Document) {
        let mut q = Context::before_registry();
        assert!(q.decode_packed(0x40001));
        let mut session = Session::new(q, None, 1, [1, 0]);
        let doc = Document::from_current(vec![0; IMAGE_CAPACITY], b"OLD".to_vec(), 6);
        session
            .regenerate_defaults(
                DefaultSource {
                    selector: 1,
                    prefix: fields,
                    japanese_mode_pairs: None,
                    restricted_band_stack: None,
                },
                doc.current(),
            )
            .unwrap();
        (session, doc)
    }

    #[test]
    fn library_r2_csv_cancel_boundaries_keep_baseline_and_use_real_stage9() {
        use crate::csv::control::ImportGate;
        let unready = Session::new(Context::before_registry(), None, 1, [1, 0]);
        let mut short = Document::from_current(vec![7], b"KEEP".to_vec(), 5);
        let before = short.clone();
        let report = short
            .import_csv::<()>(&unready, ImportGate::FileCancelled, |_| panic!("reader"))
            .unwrap();
        assert_eq!(report, (None, None));
        assert_eq!(short, before);

        let fields = source_fields();
        let (session, mut doc) = session_and_document(&fields);
        doc.new_file(&session, 7).unwrap();
        doc.accept_saved(b"KEEP".to_vec(), 5);
        doc.begin_pending_name(17, b"PENDING".to_vec());
        let baseline = doc.baseline().to_vec();
        for gate in [
            ImportGate::InnerCancelled,
            ImportGate::Header(0),
            ImportGate::Header(2),
            ImportGate::Header(255),
        ] {
            doc.patch_current(&[(0x37158, b"a"), (0x3c9b0, &[0xcc; 16])])
                .unwrap();
            let (rows, stage9) = doc
                .import_csv::<()>(&session, gate, |_| panic!("reader must not run"))
                .unwrap();
            assert!(rows.is_none());
            assert!(stage9.is_some());
            assert_eq!(doc.current()[0x37158], b'A');
            assert_eq!(&doc.current()[0x3c9b0..0x3c9c0], b"IcomCloneFormat3");
            assert_eq!(doc.baseline(), baseline);
            assert_eq!(doc.path(), b"KEEP");
            assert_eq!(doc.target_revision(), 5);
            assert!(doc.pending_matches(17));
            assert_eq!(doc.timer_ms(), None);
        }
        assert_eq!(doc.dirty(), 0); // Common command tail has not run yet.
        doc.patch_current(&[(0x399a2, &[6])]).unwrap();
        assert_eq!(doc.compare_dirty(), 1);
        doc.import_csv::<()>(&session, ImportGate::InnerCancelled, |_| panic!("reader"))
            .unwrap();
        assert_eq!(doc.dirty(), 1);
    }

    #[test]
    fn library_r2_csv_accepted_reader_applies_bytes_then_normalizes_post_row_live_current() {
        use crate::csv::{self, channel, control::ImportGate};
        let fields = source_fields();
        let (session, mut doc) = session_and_document(&fields);
        doc.new_file(&session, 7).unwrap();
        doc.accept_saved(b"KEEP".to_vec(), 6);
        let baseline = doc.baseline().to_vec();
        let (report, stage9) = doc
            .import_csv(&session, ImportGate::Header(1), |doc| {
                let mut image = doc.current_icf()?;
                let report = channel::import_user_edge(
                    &mut image,
                    b"No,Frequency Low,Frequency High\n1,145,146\n",
                    channel::Selection {
                        first: 0,
                        last: 0,
                        number_base: 1,
                    },
                    false,
                    csv::Locale::copied(b'.', b','),
                );
                let updated = image.memory().to_vec();
                drop(image);
                doc.patch_current(&[(0, &updated)])?;
                // An actual post-row recipient may replace current before9514F.
                let mut live = doc.current().to_vec();
                live[0x37158] = b'z';
                live[0x3c9b0..0x3c9c0].fill(0xcc);
                doc.replace_current(live);
                Ok(report)
            })
            .unwrap();
        let report = report.unwrap();
        assert!(report.failure.is_none(), "{:?}", report.failure);
        assert_eq!(
            (report.rows_read, report.applied, report.counts.c4),
            (1, 1, 1)
        );
        assert!(stage9.is_some());
        let mut holder = icf::record::reset_holder(0x15, 0, 0);
        icf::record_image::load_range_into(doc.current(), 0x15, 0, &mut holder).unwrap();
        assert!(holder.valid());
        assert_eq!(
            &doc.current()[0x378c0..0x378c4],
            &145_000_000_u32.to_le_bytes()
        );
        assert_eq!(doc.current()[0x37158], b'Z');
        assert_eq!(doc.baseline(), baseline);
        assert_eq!(doc.path(), b"KEEP");
        assert_eq!(doc.dirty(), 0);
    }

    #[test]
    fn library_r2_csv_reader_and_stage9_failures_keep_their_real_prefix() {
        use crate::csv::control::ImportGate;
        let fields = source_fields();
        let (mut session, mut doc) = session_and_document(&fields);
        doc.new_file(&session, 7).unwrap();
        doc.accept_saved(b"KEEP".to_vec(), 6);
        let baseline = doc.baseline().to_vec();
        doc.patch_current(&[(0x3c9b0, &[0xcc; 16])]).unwrap();
        let error = doc.import_csv::<()>(&session, ImportGate::Header(1), |doc| {
            doc.patch_current(&[(0x37158, b"a")])?;
            Err(invalid("injected reader exception"))
        });
        assert_eq!(error, Err(invalid("injected reader exception")));
        assert_eq!(doc.current()[0x37158], b'a');
        assert_eq!(&doc.current()[0x3c9b0..0x3c9c0], &[0xcc; 16]);
        // Keep the invalid weather source through R's intervening G restore.
        session.defaults[0x3a8ca] = 1;
        session.defaults[0x3c8b3] = 255;
        assert!(
            doc.import_csv(&session, ImportGate::Header(1), |doc| {
                doc.patch_current(&[(0x3a8ca, &[1]), (0x3c8b3, &[255]), (0x3c800, &[255])])
            })
            .is_err()
        );
        assert_eq!(doc.current()[0x37158], b'A'); // Earlier stage9 work remains.
        assert_eq!(doc.current()[0x3c800] & 3, 0); // Native weather prefix precedes failure.
        assert_eq!(&doc.current()[0x3c9b0..0x3c9c0], &[0xcc; 16]);
        assert_eq!(doc.baseline(), baseline);
        assert_eq!(doc.path(), b"KEEP");
        assert_eq!(doc.dirty(), 0);
    }

    fn wire_for_session() -> crate::clone::session_test_support::Mock {
        use crate::clone::session_test_support::{Mock, response};
        let mut transport = Mock::new(6);
        let mut identity = response(6);
        identity[9 + 0x13] = 8; // Independent E1 bits: Q.kind1, all other fields0.
        transport.query_response = Some(identity);
        transport
    }

    #[test]
    fn library_r2_clone_write_runs_three_real_probes_and_detached_final_recipient() {
        use crate::{
            clone::session_test_support::config,
            icf::document::{CloneControls, CloneReply, CloneStep, CloneStop, CloneWrite},
        };
        let fields = source_fields();
        let (session, mut doc) = session_and_document(&fields);
        doc.new_file(&session, 7).unwrap();
        doc.accept_saved(b"KEEP".to_vec(), 6);
        doc.patch_current(&[(0x37158, b"a")]).unwrap();
        let baseline = doc.baseline().to_vec();
        let mut controls = CloneControls {
            controller_busy: 0,
            wrapper_state: 0,
            form_flag57: 1,
        };
        let mut operation = CloneWrite::new(
            config(),
            0,
            normalizer::ResetState {
                show_both: 1,
                home_invalidated: 0,
            },
        )
        .unwrap();
        let mut reply = CloneReply::Continue;
        let mut probes = 0;
        let mut final_calls = 0;
        loop {
            match operation
                .advance(
                    &mut doc,
                    &session,
                    controls,
                    ValidationSource::NoTree,
                    reply,
                )
                .unwrap()
            {
                CloneStep::Sync(_) => reply = CloneReply::Continue,
                CloneStep::Query(query) => {
                    let mut transport = wire_for_session();
                    let outcome = query.run(&mut transport, || false);
                    assert_eq!(outcome.failure, None);
                    assert_eq!(transport.commands, [0xe0]);
                    assert_eq!(transport.closes, 1);
                    probes += 1;
                    reply = CloneReply::Probe(outcome);
                }
                CloneStep::Confirm { warning, selector } => {
                    assert!(!warning);
                    assert_eq!(selector, 6);
                    reply = CloneReply::Confirmation(1);
                }
                CloneStep::Write(write) => {
                    let mut transport = wire_for_session();
                    let outcome = write.run(&mut transport, || false);
                    assert_eq!(outcome.failure, None);
                    assert_eq!(
                        outcome.write_disposition,
                        clone::WriteDisposition::Acknowledged
                    );
                    assert_eq!(transport.commands.first(), Some(&0xe3));
                    assert!(!transport.commands.contains(&0xe0));
                    assert_eq!(transport.commands.last(), Some(&0xe5));
                    let probe = operation.outcomes.last().unwrap();
                    assert_eq!(outcome.identity, probe.identity);
                    assert_eq!(outcome.echo, probe.echo);
                    reply = CloneReply::Written(outcome);
                }
                CloneStep::Finalize => {
                    assert!(operation.active_mask.is_some());
                    assert_eq!(
                        operation
                            .finalize(&mut controls, |operation, _| {
                                assert!(operation.active_mask.is_some());
                                final_calls += 1;
                                Ok(())
                            })
                            .unwrap(),
                        CloneStop::Completed
                    );
                    break;
                }
                CloneStep::Stopped(reason) => panic!("unexpected stop: {reason:?}"),
            }
        }
        assert_eq!((probes, final_calls, operation.outcomes.len()), (3, 1, 4));
        assert!(operation.active_mask.is_none());
        assert_eq!(doc.current()[0x37158], b'A');
        assert_eq!(doc.baseline(), baseline);
        assert_eq!(doc.path(), b"KEEP");
        assert_eq!(doc.dirty(), 0);
    }

    #[test]
    fn library_r2_clone_read_uses_actual_wire_image_and_keeps_failed_acquisition_private() {
        use crate::{
            clone::session_test_support::{config, end, packet},
            icf::document::{
                CloneControls, CloneRead, CloneReadReply, CloneReadStep, CloneReadStop,
            },
        };
        let fields = source_fields();
        for (fail, reprobe) in [(false, false), (false, true), (true, false)] {
            let (mut session, mut doc) = session_and_document(&fields);
            doc.new_file(&session, 7).unwrap();
            doc.accept_saved(b"KEEP".to_vec(), 6);
            let before = doc.clone();
            let mut operation = CloneRead::new(config(), 7).unwrap();
            let controls = CloneControls {
                controller_busy: 0,
                wrapper_state: 0,
                form_flag57: 1,
            };
            let mut reply = CloneReadReply::Continue;
            let mut boundaries = Vec::new();
            let mut probes = 0;
            loop {
                match operation.advance(&session, controls, reply).unwrap() {
                    CloneReadStep::Query(query) => {
                        let mut transport = wire_for_session();
                        if reprobe && probes != 0 {
                            transport.query_response.as_mut().unwrap()[10] = 5;
                        }
                        let outcome = query.run(&mut transport, || false);
                        assert_eq!(outcome.failure, None);
                        assert_eq!(transport.commands, [0xe0]);
                        if reprobe && probes == 0 {
                            // The sync boundary sees live Q, not the entry snapshot.
                            let mut fields = session.context.fields();
                            fields.0[0] = 0;
                            session.context.adopt(fields);
                        }
                        probes += 1;
                        reply = CloneReadReply::Probe(outcome);
                    }
                    CloneReadStep::Sync(_) => reply = CloneReadReply::Continue,
                    CloneReadStep::Read(read) => {
                        let mut transport = wire_for_session();
                        let mut incoming = fields.clone();
                        incoming[0x399a2] = 6;
                        let length = if fail { 64 } else { 0x3c9c0 };
                        for (block, bytes) in incoming[..length].chunks(64).enumerate() {
                            transport
                                .read_data
                                .extend(packet((block * 64) as u32, bytes));
                        }
                        transport.read_data.extend(end(length.div_ceil(64)));
                        let outcome = read.run(&mut transport, || false);
                        assert_eq!(
                            outcome.failure,
                            fail.then_some(clone::Failure::MissingCoverage)
                        );
                        assert_eq!(transport.commands, [0xe2]);
                        let probe = operation.outcomes.last().unwrap();
                        assert_eq!(outcome.identity, probe.identity);
                        assert_eq!(outcome.echo, probe.echo);
                        assert!(outcome.acquired(0));
                        reply = CloneReadReply::Read(outcome);
                    }
                    CloneReadStep::Apply => {
                        let stop = operation
                            .finish_document(
                                &mut doc,
                                &mut session,
                                Some(DefaultSource {
                                    selector: 1,
                                    prefix: &fields,
                                    japanese_mode_pairs: None,
                                    restricted_band_stack: None,
                                }),
                                ValidationSource::NoTree,
                                |boundary, _, _| {
                                    boundaries.push(boundary);
                                    Ok(())
                                },
                            )
                            .unwrap();
                        assert_eq!(
                            stop,
                            if fail {
                                CloneReadStop::ReadFailed
                            } else {
                                CloneReadStop::Completed
                            }
                        );
                        break;
                    }
                    CloneReadStep::Stopped(reason) => panic!("unexpected stop: {reason:?}"),
                }
            }
            assert_eq!(probes, if reprobe { 2 } else { 1 });
            if fail {
                assert_eq!(doc, before);
                assert_eq!(boundaries, [super::super::InitialBoundary::Sync(0x11)]);
            } else {
                assert_eq!(doc.current()[0x399a2], 6);
                assert_eq!(doc.target_revision(), if reprobe { 5 } else { 6 });
                assert_eq!(doc.baseline(), before.baseline());
                assert!(doc.path().is_empty());
                assert_eq!(doc.dirty(), 1);
                assert_eq!(
                    boundaries.last(),
                    Some(&super::super::InitialBoundary::Sync(0x25))
                );
            }
        }
    }

    #[test]
    fn library_r2_information_queries_real_transport_and_uses_same_document_recipients() {
        use crate::clone::{information::*, session_test_support::config};
        use crate::icf::document::{
            InitialState,
            lifecycle::{RecentFiles, UiState},
        };
        let fields = source_fields();
        let (mut session, mut doc) = session_and_document(&fields);
        doc.new_file(&session, 7).unwrap();
        doc.accept_saved(b"KEEP".to_vec(), 6);
        let before = doc.clone();
        let mut controller = Controller::new(7, b"7".to_vec(), [6, 0, 0]);
        controller
            .construct_com(
                &RegistrySource {
                    values: &[],
                    crt_point: Some(b'.'),
                },
                &ReadMode::Current,
            )
            .unwrap();
        let mut initial = InitialState::default();
        let mut ui = UiState::headless();
        let recent = RecentFiles::default();
        let mut receiver = DocumentReceiver {
            document: &mut doc,
            session: &mut session,
            initial: &mut initial,
            ui: &mut ui,
            recent: &recent,
        };
        controller
            .request_reidentify_document(&[false, false], &mut receiver)
            .unwrap();
        assert_eq!(receiver.ui.form.as_ref().unwrap().enabled58, 0);
        let mut transport = wire_for_session();
        let outcome = controller
            .query_document(config(), &mut transport, || false, &mut receiver)
            .unwrap();
        assert_eq!(outcome.failure, None);
        assert_eq!(transport.commands, [0xe0]);
        controller
            .return_query(
                &outcome,
                &mut receiver,
                ReturnSource {
                    defaults: None,
                    profile_return: None,
                    captions: Some([b"HEADER".to_vec(), b"LABEL".to_vec()]),
                    close_returns: &[],
                },
            )
            .unwrap();
        let rows = controller.information_rows().unwrap().unwrap();
        assert_eq!(rows[0][0].text, b"HEADER");
        assert_eq!(rows[1][0].text, b"LABEL");
        assert_eq!(rows[1][1].text, b"");
        assert_eq!(*receiver.document, before);
        assert_eq!(receiver.ui.form.as_ref().unwrap().enabled58, 0);
        controller.information_close(true).unwrap();
        assert_eq!(controller.information, Life::ReleaseRequested);
        assert_eq!(receiver.ui.form.as_ref().unwrap().enabled58, 0);
        controller
            .destroy_document("information", &mut receiver)
            .unwrap();
        assert_eq!(controller.information, Life::Destroyed);
        assert_eq!(receiver.ui.form.as_ref().unwrap().enabled58, 1);
        assert!(receiver.ui.form.as_ref().unwrap().focused);
    }

    #[test]
    fn literal_open_edit_save_reopen_consumer_preserves_metadata_opaque_fields_and_dirty_timer() {
        let fields = source_fields();
        let (mut session, mut doc) = session_and_document(&fields);
        let input = literal_file(&fields, 6, 0x40001);
        assert_eq!(
            doc.read(
                Some(&input),
                b"/x/input.icf".to_vec(),
                7,
                None,
                &mut session,
                None
            )
            .unwrap(),
            OpenStatus::Opened {
                adopted_context: false,
                foreign_context: false
            }
        );
        assert_eq!(doc.path(), b"/x/input.icf");
        assert_eq!(doc.directory(), b"/x/");
        assert_eq!(doc.baseline(), doc.current());
        assert_eq!(doc.dirty(), 0);
        assert_eq!(doc.comment_bytes(), b"LITERAL");
        assert_eq!(doc.extra_data(), 0x40001);
        doc.edit_setting("noise_blanker_depth", 0, &icf::settings::Value::Number(7))
            .unwrap();
        assert_eq!(doc.current()[0x399a2], 6); // Native displayed depth7 is stored6.
        assert_eq!(doc.dirty(), 1);
        assert_eq!(doc.timer_ms(), Some(250));
        let mut output = Vec::new();
        assert_eq!(
            doc.save(&mut session, b"/x/output.icf".to_vec(), 6, 6, &mut output)
                .unwrap(),
            SaveStatus::Saved
        );
        assert_eq!(doc.dirty(), 0);
        assert_eq!(doc.baseline(), doc.current());
        assert_eq!(doc.path(), b"/x/output.icf");
        assert_eq!(doc.timer_ms(), Some(250));
        assert!(
            output
                .starts_with(b"40750001\r\n#Comment=LITERAL\r\n#MapRev=6\r\n#EtcData=00040001\r\n")
        );
        let mut reread = Document::from_current(vec![0; IMAGE_CAPACITY], Vec::new(), 6);
        assert!(matches!(
            reread
                .read(Some(&output), b"OUT".to_vec(), 7, None, &mut session, None)
                .unwrap(),
            OpenStatus::Opened { .. }
        ));
        assert_eq!(reread.current()[0x399a2], 6);
        assert_eq!(
            &reread.current()[0x399c1..0x399dd],
            b"OPAQUE-SOURCE-FIELD-28-BYTES"
        );
        assert_eq!(reread.comment_bytes(), b"LITERAL");
        assert_eq!(reread.extra_data(), 0x40001);
        assert_eq!(&reread.current()[0x3c9b0..0x3c9c0], b"IcomCloneFormat3");
    }

    #[test]
    fn open_resets_document_flags_only_after_outer_gates_and_before_stage8() {
        let fields = source_fields();
        let (mut session, mut doc) = session_and_document(&fields);
        session.document_flags = [3, 5];
        let before = doc.clone();
        assert!(matches!(
            doc.read(Some(b""), b"REJECTED".to_vec(), 7, None, &mut session, None)
                .unwrap(),
            OpenStatus::Rejected(_)
        ));
        assert_eq!(doc, before);
        assert_eq!(session.document_flags, [3, 5]);

        let mut broken = fields.clone();
        // The outer gates accept storage with a next-slot address outside M.
        broken[0x355c1..0x355c3].copy_from_slice(&0xfffe_u16.to_le_bytes());
        broken[0x36114] = 0;
        assert!(
            doc.read(
                Some(&literal_file(&broken, 6, 0x40001)),
                b"FAILED-STAGE8".to_vec(),
                7,
                None,
                &mut session,
                None,
            )
            .is_err()
        );
        assert_eq!(session.document_flags, [0, 0]);
        assert_eq!(doc.path(), b"FAILED-STAGE8");
        assert_eq!(&doc.current()[0x355c1..0x355c3], &[0xfe, 0xff]);
        assert_eq!(doc.baseline(), before.baseline());

        session.document_flags = [3, 5];
        assert!(matches!(
            doc.read(
                Some(&literal_file(&fields, 6, 0x40001)),
                b"OPENED".to_vec(),
                7,
                None,
                &mut session,
                None,
            )
            .unwrap(),
            OpenStatus::Opened { .. }
        ));
        assert_eq!(session.document_flags, [0, 0]);
        assert_eq!(doc.baseline(), doc.current());
    }

    #[test]
    fn legacy_save_roundtrip_changes_current_only_after_writer_success_and_new_keeps_pending() {
        struct FailAfterPrefix(Vec<u8>);
        impl Write for FailAfterPrefix {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                if !self.0.is_empty() {
                    return Err(std::io::Error::other("synthetic writer failure"));
                }
                let n = b.len().min(17);
                self.0.extend_from_slice(&b[..n]);
                Ok(n)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let fields = source_fields();
        let (mut session, mut doc) = session_and_document(&fields);
        doc.begin_pending_name(17, b"PENDING".to_vec());
        assert!(!doc.new_file(&session, 2).unwrap());
        assert_eq!(doc.path(), b"OLD");
        assert!(doc.new_file(&session, 7).unwrap());
        assert!(doc.path().is_empty());
        assert!(doc.pending_matches(17));
        doc.patch_current(&[(0x399bb, &[5]), (0x37158, b"a")])
            .unwrap();
        doc.compare_dirty();
        let baseline = doc.baseline().to_vec();
        let mut failed = FailAfterPrefix(Vec::new());
        session.document_flags = [3, 5];
        assert!(matches!(
            doc.save(&mut session, b"FAILED".to_vec(), 5, 6, &mut failed),
            Err(FileError::Output(_))
        ));
        assert_eq!(failed.0.len(), 17);
        assert_eq!(session.document_flags, [3, 5]);
        assert_eq!(doc.current()[0x37158], b'A');
        assert_eq!(doc.current()[0x399bb], 5);
        assert_eq!(doc.baseline(), baseline);
        assert_eq!(doc.dirty(), 1);
        assert!(doc.path().is_empty());
        let mut out = Vec::new();
        assert_eq!(
            doc.save(&mut session, b"OLD-FORMAT".to_vec(), 5, 6, &mut out)
                .unwrap(),
            SaveStatus::Saved
        );
        assert_eq!(session.document_flags, [0, 0]);
        assert_eq!(doc.current()[0x399bb], 1);
        assert_eq!(doc.target_revision(), 5);
        assert_eq!(doc.dirty(), 0);
        assert_eq!(&doc.current()[0x3c9b0..0x3c9c0], b"IcomCloneFormat3");
        assert!(doc.pending_matches(17));
        let read = format::read(&out).unwrap();
        assert_eq!(read.status, format::Status::Success);
        assert_eq!(read.layout.unwrap().selector(), 5);
        let before = doc.clone();
        assert_eq!(
            doc.save(&mut session, b"NO".to_vec(), 4, 7, &mut Vec::new())
                .unwrap(),
            SaveStatus::RetrySaveAs
        );
        assert_eq!(doc, before);
    }

    #[test]
    fn save_keeps_raw_target_gates_separate_from_converter_local_layout_six() {
        let fields = source_fields();
        let (mut session, mut doc) = session_and_document(&fields);
        doc.new_file(&session, 7).unwrap();
        session.document_flags = [3, 5];
        let inactive = 0x2eda3 + 8 * 299;
        doc.patch_current(&[(inactive, b"INACTIVE")]).unwrap();
        let before = doc.clone();
        for target in [0, 7, 255, u32::MAX] {
            let mut output = Vec::new();
            assert_eq!(
                doc.save(&mut session, b"NO".to_vec(), target, 7, &mut output)
                    .unwrap(),
                SaveStatus::RetrySaveAs
            );
            assert_eq!(
                doc.save(&mut session, b"CANCEL".to_vec(), target, 2, &mut output)
                    .unwrap(),
                SaveStatus::Cancelled
            );
            assert!(output.is_empty());
            assert_eq!(doc, before);
            assert_eq!(session.document_flags, [3, 5]);
        }
        let mut output = Vec::new();
        assert_eq!(
            doc.save(&mut session, b"RAW255".to_vec(), 255, 6, &mut output)
                .unwrap(),
            SaveStatus::Saved
        );
        let file = format::read(&output).unwrap();
        assert_eq!(file.status, format::Status::Success);
        assert_eq!(file.reader.map_revision, 6); // Converter-local layout only.
        assert_eq!(&file.image[inactive..inactive + 8], b"INACTIVE");
        assert_eq!(&doc.current()[inactive..inactive + 8], b"CALLSIGN"); // Post-write slot08 reached.
        assert_eq!(doc.target_revision(), 255); // Raw target is not rewritten to6.
        assert_eq!(session.document_flags, [0, 0]);
        assert_eq!(doc.path(), b"RAW255");
        assert_eq!(doc.baseline(), doc.current());
    }

    #[test]
    fn profile_cancel_and_late_generation_failure_keep_their_actual_commit_prefix() {
        let fields = source_fields();
        let (mut session, mut doc) = session_and_document(&fields);
        let input = literal_file(&fields, 6, 0);
        let before = doc.clone();
        assert_eq!(
            doc.read(
                Some(&input),
                b"CANCEL".to_vec(),
                7,
                Some(2),
                &mut session,
                None
            )
            .unwrap(),
            OpenStatus::Cancelled
        );
        assert_eq!(doc, before);
        assert!(
            doc.read(
                Some(&input),
                b"ADOPT".to_vec(),
                7,
                Some(6),
                &mut session,
                None
            )
            .is_err()
        );
        assert_eq!(session.context.fields().0[0], 0); // Q adoption precedes missing source error.
        assert_eq!(doc, before);
        let (mut session, mut doc) = session_and_document(&fields);
        let baseline = doc.baseline().to_vec();
        assert_eq!(
            doc.read(
                Some(&input),
                b"FOREIGN".to_vec(),
                7,
                Some(7),
                &mut session,
                None
            )
            .unwrap(),
            OpenStatus::Opened {
                adopted_context: false,
                foreign_context: true
            }
        );
        assert!(doc.path().is_empty());
        assert_eq!(doc.dirty(), 1);
        assert_eq!(doc.baseline(), baseline);
    }

    #[test]
    fn profile_yes_runs_new_q_range_stage7_raw_phases_and_full_g_before_open_adoption() {
        let fields = source_fields();
        let (mut session, mut doc) = session_and_document(&fields);
        let input = literal_file(&fields, 6, 0);
        let patch = [0; 120];
        assert_eq!(
            doc.read(
                Some(&input),
                b"ADOPTED".to_vec(),
                7,
                Some(6),
                &mut session,
                Some(DefaultSource {
                    selector: 0,
                    prefix: &fields,
                    japanese_mode_pairs: Some(&patch),
                    restricted_band_stack: None
                })
            )
            .unwrap(),
            OpenStatus::Opened {
                adopted_context: true,
                foreign_context: false
            }
        );
        assert_eq!(session.context.fields().0[0], 0);
        assert_eq!(session.default_selector(), Some(0));
        assert!(session.ranges.is_complete());
        assert_eq!(
            &session.defaults().unwrap()[0x3c9b0..0x3c9c0],
            b"IcomCloneFormat3"
        );
        assert_eq!(doc.path(), b"ADOPTED");
        assert_eq!(doc.dirty(), 0);
        assert_eq!(doc.baseline(), doc.current());
        let old = doc.clone();
        assert!(
            session
                .regenerate_defaults(
                    DefaultSource {
                        selector: 0,
                        prefix: &fields[..17],
                        japanese_mode_pairs: Some(&patch),
                        restricted_band_stack: None
                    },
                    doc.current()
                )
                .is_err()
        );
        assert_eq!(session.default_selector(), None);
        assert!(session.defaults().is_err());
        assert!(doc.new_file(&session, 7).is_err());
        assert_eq!(doc, old);
    }

    #[test]
    fn operating_reset_list_phase_descends_live_lists_and_keeps_late_f_failure_prefix() {
        use icf::{
            record::{self, ChannelField, RepeaterText},
            record_image,
        };
        let fields = source_fields();
        let (mut session, mut doc) = session_and_document(&fields);
        doc.new_file(&session, 7).unwrap();
        session.defaults[0x3c901..0x3c903].copy_from_slice(&0x3412_u16.to_le_bytes());
        doc.current[0x49922..0x49924].copy_from_slice(&2_u16.to_le_bytes());
        let context =
            crate::repeater::working::DefaultContext::from_context(&session.context, None).unwrap();
        for index in 0..2 {
            let mut h = context.holder(7, 1);
            h.set_header(0x0c, index, 1);
            h.set_valid(true);
            h.set_channel_field(ChannelField::FrequencyHz, 145_000_000);
            h.set_repeater_text(RepeaterText::Call, b"REPEAT A");
            crate::repeater::working::save_into(&mut doc.current, index as usize, &h).unwrap();
        }
        doc.current[0x49978..0x4997a].copy_from_slice(&3_u16.to_le_bytes());
        doc.current[0x2eda3..0x2edbb].copy_from_slice(b"FIRST  AFIRST  ASECOND B");
        doc.current[0x395ad..0x3969d].fill(b' ');
        doc.current[0x395ad..0x395c5].copy_from_slice(b"ALLOW  AALLOW  AOTHER  B");
        let report = normalizer::reset_lists(
            &mut doc.current,
            ReferenceImage::Separate(session.defaults().unwrap()),
            &session.context,
            &session.ranges,
            None,
        )
        .unwrap();
        assert_eq!(
            report,
            normalizer::ResetLists {
                saved_repeaters: 1,
                deleted_repeaters: 1,
                deleted_ur: 1,
                deleted_allowed: 1
            }
        );
        assert_eq!(&doc.current[0x49922..0x49924], &[1, 0]);
        assert_eq!(&doc.current[0x49978..0x4997a], &[2, 0]);
        assert_eq!(&doc.current[0x2eda3..0x2edb3], b"FIRST  ASECOND B");
        assert_eq!(
            &doc.current[0x2eda3 + 8 * 299..0x2edab + 8 * 299],
            b"CALLSIGN"
        );
        assert_eq!(&doc.current[0x3c901..0x3c903], &[0x12, 0x34]);
        assert_eq!(record_image::callsign_count(&doc.current, 0x17).unwrap(), 2);
        let mut h = record::reset_holder(0x0c, 0, 1);
        crate::repeater::working::load_into(&doc.current, 0, &mut h).unwrap();
        assert_eq!(h.repeater_text(RepeaterText::Call), b"REPEAT A");
        assert_eq!(&doc.current[0x3c9b0..0x3c9c0], b"IcomCloneFormat3");

        doc.current[0x49978..0x4997a].copy_from_slice(&3_u16.to_le_bytes());
        doc.current[0x2eda3..0x2edbb].copy_from_slice(b"FIRST  AFIRST  ASECOND B");
        doc.current[0x395ad..0x395bd].copy_from_slice(b"ALLOW  AALLOW  A");
        assert!(
            normalizer::reset_lists(
                &mut doc.current,
                ReferenceImage::Separate(&[]),
                &session.context,
                &session.ranges,
                None
            )
            .is_err()
        );
        assert_eq!(&doc.current[0x49978..0x4997a], &[2, 0]);
        assert_eq!(&doc.current[0x2eda3..0x2edb3], b"FIRST  ASECOND B");
        assert_eq!(&doc.current[0x395ad..0x395bd], b"ALLOW  AALLOW  A"); // Allowed phase is later.
    }

    #[test]
    fn native_publication_packs_live_q_after_new_foreign_no_and_legacy_save() {
        let fields = source_fields();
        let (mut session, mut doc) = session_and_document(&fields);
        doc.new_file(&session, 7).unwrap();
        assert_eq!(doc.extra_data(), 0);
        assert_eq!(
            format::read(&doc.current_bytes(&session).unwrap())
                .unwrap()
                .reader
                .extra_data,
            0x40001
        );
        for target in [6, 5] {
            let mut output = Vec::new();
            doc.save(&mut session, b"NEW".to_vec(), target, 6, &mut output)
                .unwrap();
            assert_eq!(format::read(&output).unwrap().reader.extra_data, 0x40001);
        }
        let foreign = literal_file(&fields, 6, 0);
        doc.read(
            Some(&foreign),
            b"FOREIGN".to_vec(),
            7,
            Some(7),
            &mut session,
            None,
        )
        .unwrap();
        assert_eq!(doc.extra_data(), 0); // Reader observation is NOT rewritten.
        let mut output = Vec::new();
        doc.save(&mut session, b"SAVED".to_vec(), 6, 6, &mut output)
            .unwrap();
        assert_eq!(format::read(&output).unwrap().reader.extra_data, 0x40001);
        assert_eq!(doc.extra_data(), 0);
        session.context.decode_packed(0x40808);
        assert_eq!(
            format::read(&doc.current_bytes(&session).unwrap())
                .unwrap()
                .reader
                .extra_data,
            0x40808
        );
        assert_eq!(session.ranges.selected(), 1); // Packing does not regenerate B/G.
        assert_eq!(
            format::read(
                &format::write_storage(
                    &fields,
                    Layout::from_map_revision(6),
                    b"OBSERVED",
                    0x1234_5639
                )
                .unwrap()
            )
            .unwrap()
            .reader
            .extra_data,
            0x1234_5639
        );
    }

    #[test]
    fn typed_view_validation_counts_bit8_once_and_save_no_yes_take_real_different_paths() {
        use icf::{
            record::{self, ChannelField},
            record_image,
        };
        let fields = source_fields();
        let (mut session, mut doc) = session_and_document(&fields);
        doc.new_file(&session, 7).unwrap();
        let view = ValidationView {
            kind: 3,
            group: 100,
            first: 0,
            last: 0,
            count: 1,
            include_side1: true,
            show_both: 1,
        };
        let set_call = |doc: &mut Document, hz| {
            let mut h = record::reset_holder(3, 0, 1);
            h.set_channel_field(ChannelField::FrequencyHz, hz);
            h.set_mode(13);
            record_image::store_into(&mut doc.current, 3, 0, &mut h).unwrap();
        };
        set_call(&mut doc, 1);
        let report = doc
            .validate_views(&session, ValidationSource::Views(&[view]))
            .unwrap();
        assert_eq!(report.flags, [12]);
        assert_eq!((report.errors, report.warnings), (1, 0));
        let mut output = Vec::new();
        let request = SaveRequest {
            path: b"ERROR-CONTINUES".to_vec(),
            target_revision: 6,
            legacy_choice: 6,
            warning_choice: Some(6),
            validation: ValidationSource::Views(&[view]),
        };
        let report = doc
            .save_with_validation(&mut session, request, None, &mut output)
            .unwrap();
        assert_eq!(report.status, SaveStatus::Saved);
        assert!(report.correction.is_none());
        assert!(!report.checksum_written);
        assert!(!output.windows(4).any(|w| w == b"#CD="));
        assert_eq!(
            format::read(&output).unwrap().status,
            format::Status::Success
        );
        set_call(&mut doc, 123_000_000);
        let report = doc
            .validate_views(&session, ValidationSource::Views(&[view]))
            .unwrap();
        assert_eq!(report.flags, [4]);
        for choice in [7, 2] {
            let mut no = doc.clone();
            let mut output = Vec::new();
            let report = no
                .save_with_validation(
                    &mut session,
                    SaveRequest {
                        path: b"NO-CONTINUES".to_vec(),
                        target_revision: 6,
                        legacy_choice: 6,
                        warning_choice: Some(choice),
                        validation: ValidationSource::Views(&[view]),
                    },
                    None,
                    &mut output,
                )
                .unwrap();
            assert_eq!(report.status, SaveStatus::Saved);
            assert!(report.correction.is_none());
            assert!(!report.checksum_written);
            let mut h = record::reset_holder(3, 0, 1);
            record_image::load_into(&no.current, 3, 0, &mut h).unwrap();
            assert_eq!(h.channel_field(ChannelField::FrequencyHz), 123_000_000);
        }
        let mut state = normalizer::ResetState {
            show_both: 1,
            home_invalidated: 0xa7,
        };
        let mut output = Vec::new();
        let report = doc
            .save_with_validation(
                &mut session,
                SaveRequest {
                    path: b"YES-CORRECTS".to_vec(),
                    target_revision: 6,
                    legacy_choice: 6,
                    warning_choice: Some(6),
                    validation: ValidationSource::Views(&[view]),
                },
                Some(&mut state),
                &mut output,
            )
            .unwrap();
        assert!(report.checksum_written);
        let effects = report.correction.unwrap();
        assert!(effects.correction.typed_saved > 0);
        assert_eq!(effects.notifications.len(), 104);
        assert_eq!(
            (
                effects.notifications[0].event,
                effects.notifications[0].first
            ),
            (0x3c, 1)
        );
        assert!(effects.notifications[1].synchronous);
        assert_eq!(
            (
                effects.notifications[103].event,
                effects.notifications[103].first
            ),
            (0x36, 101)
        );
        let mut h = record::reset_holder(3, 0, 1);
        record_image::load_into(&doc.current, 3, 0, &mut h).unwrap();
        assert_eq!(h.channel_field(ChannelField::FrequencyHz), 0);
        assert_eq!(format::read(&output).unwrap().reader.extra_data, 0x40001);
    }

    #[test]
    fn reset_root_preserves_map_release_order_and_autorpt_reserved_byte_with_shared_predicates() {
        use icf::{
            record::{self, ChannelField},
            record_image,
        };
        let fields = source_fields();
        let (mut session, mut doc) = session_and_document(&fields);
        doc.new_file(&session, 7).unwrap();
        session.context.decode_packed(0x40808); // LiveQ changes; B/G deliberately stay the old generation.
        doc.current[0x3fb40..0x3fb48].copy_from_slice(&[1, 0, 0, 0, 1, 0, 0, 0]);
        doc.current[0x3fcd4..0x3fcd8].fill(0);
        doc.current[0x3662d] &= !1;
        doc.current[0x392fb..0x392ff].fill(255);
        doc.current[0x3c84b] = 0;
        let mut h = record::reset_holder(2, 0, 1);
        h.set_channel_field(ChannelField::FrequencyHz, 1);
        h.set_mode(13);
        record_image::store_into(&mut doc.current, 2, 0, &mut h).unwrap();
        let record = doc.current[..83].to_vec();
        doc.current[0x370e0] = 1; // A valid bad AutoRpt pair has nonzero DUP/low/high.
        doc.current[0x370e1] = 0xa7;
        doc.current[0x370e2..0x370ea].copy_from_slice(&[1, 0, 0, 0, 2, 0, 0, 0]);
        let mut state = normalizer::ResetState {
            show_both: 1,
            home_invalidated: 0,
        };
        doc.reset_operating_state(&session, &mut state).unwrap();
        assert_eq!(&doc.current[0x3fcd4..0x3fcd8], &0xffff_u32.to_le_bytes());
        assert_eq!(&doc.current[0x3fb44..0x3fb48], &[0; 4]);
        assert_eq!(doc.current[0x3662d] & 1, 1);
        assert_eq!(&doc.current[..83], record);
        assert_eq!(state.home_invalidated, 1);
        assert_eq!(doc.current[0x370e1], 0xa7);
        assert_eq!(&doc.current[0x370e2..0x370ea], &[0; 8]);
        let mut empty = Document::from_current(vec![], Vec::new(), 6);
        assert_eq!(
            empty
                .validate_views(
                    &session,
                    ValidationSource::Views(&[ValidationView {
                        kind: 2,
                        group: 0,
                        first: 0,
                        last: 0,
                        count: 0,
                        include_side1: true,
                        show_both: 0
                    }])
                )
                .unwrap()
                .flags,
            [0]
        );
        empty.current = doc.current.clone();
        empty.current[0x392fb..0x392ff].fill(255);
        let original = empty.current.clone();
        assert!(
            normalizer::reset_operating_state(
                &mut empty.current,
                ReferenceImage::Separate(&[]),
                &session.context,
                &session.ranges,
                None,
                &mut state
            )
            .is_err()
        );
        assert_eq!(empty.current, original); // F is loaded even for invalid first normal row.
    }
}
