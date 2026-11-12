//! Document roots with real file publication and separately supplied UI input.
//! Native OS acquisition/delivery is not simulated by a successful event log.

use super::{
    DefaultSource, Document, Error, FileError, OpenStatus, SaveOutcome, SaveReport, SaveRequest,
    SaveStatus, Session, ValidationSource, invalid, resolve_prompt,
};
use crate::icf::normalizer::ResetState;
use std::borrow::BorrowMut;
use std::io::{self, Write};
use std::{fmt, sync::Arc};

#[path = "document_lifecycle/native.rs"]
mod native;
pub use native::{NativeState, NativeView};

pub const RECENT_LIMIT: usize = 10;
pub const RECENT_VISIBLE: usize = 4;

/// CompareStringA inputs are raw ANSI paths, locale 0x400, flags 1. Result 0 is
/// API failure, NOT equality. Do not substitute host case folding or Unicode.
pub trait PathComparison {
    fn compare(&mut self, left: &[u8], right: &[u8]) -> Result<i32, Error>;
}

pub trait RecentStore {
    fn read(&mut self, index: usize) -> io::Result<Vec<u8>>;
    fn write(&mut self, index: usize, path: &[u8]) -> io::Result<()>;
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RecentFiles {
    paths: Vec<Vec<u8>>,
}
impl RecentFiles {
    pub fn paths(&self) -> &[Vec<u8>] {
        &self.paths
    }
    pub fn visible(&self) -> &[Vec<u8>] {
        &self.paths[..self.paths.len().min(RECENT_VISIBLE)]
    }
    pub fn selected(&self, index: usize) -> Result<&[u8], Error> {
        self.visible()
            .get(index)
            .map(Vec::as_slice)
            .ok_or(invalid("recent menu index outside visible entries"))
    }
    fn deduplicate(&mut self, path: &[u8], compare: &mut impl PathComparison) -> Result<(), Error> {
        for index in (0..self.paths.len()).rev() {
            let result = compare.compare(&self.paths[index], path)?;
            if !(0..=3).contains(&result) {
                return Err(invalid("CompareStringA result outside0..3"));
            }
            if result == 2 {
                self.paths.remove(index);
            }
        }
        Ok(())
    }
    pub fn insert(
        &mut self,
        path: Vec<u8>,
        compare: &mut impl PathComparison,
    ) -> Result<(), Error> {
        self.deduplicate(&path, compare)?;
        self.paths.insert(0, path);
        self.paths.truncate(RECENT_LIMIT);
        Ok(())
    }
    /// The native load entry appends after deduplication; it does not reverse MRU.
    pub fn load(
        store: &mut impl RecentStore,
        compare: &mut impl PathComparison,
    ) -> Result<Self, FileError> {
        let mut recent = Self::default();
        for index in 0..RECENT_LIMIT {
            let path = store.read(index).map_err(FileError::Output)?;
            if !path.is_empty() {
                recent.deduplicate(&path, compare)?;
                recent.paths.push(path);
                recent.paths.truncate(RECENT_LIMIT);
            }
        }
        Ok(recent)
    }
    /// Write all ten slots, including empty trailing paths. Failure keeps prefix.
    pub fn persist(&self, store: &mut impl RecentStore) -> io::Result<()> {
        for index in 0..RECENT_LIMIT {
            store.write(index, self.paths.get(index).map_or(&[], Vec::as_slice))?;
        }
        Ok(())
    }
}

/// Native one-based dialog filters select descending targets 6 through 1.
pub fn filter_target(index: u32) -> u32 {
    if (1..=6).contains(&index) {
        7 - index
    } else {
        6
    }
}
pub fn target_filter(target: u32) -> u32 {
    if (1..=6).contains(&target) {
        7 - target
    } else {
        1
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveDialog {
    pub path: Vec<u8>,
    pub directory: Vec<u8>,
    pub filter_index: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveSelection {
    pub path: Vec<u8>,
    pub filter_index: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    Dirty,
    Legacy { target: u32 },
    Warning { count: u32 },
}
/// Only user gestures are supplied here, never a prepared SaveReport/outcome.
pub trait Dialogs {
    fn save_as(&mut self, initial: SaveDialog) -> Result<Option<SaveSelection>, Error>;
    fn choice(&mut self, prompt: Prompt) -> Result<i32, Error>;
}
/// Real byte I/O. publish must finish all publication work before returning Ok.
/// A Vec staging writer alone is NOT a publisher and must not be used as one.
pub trait Files {
    fn read(&mut self, path: &[u8]) -> io::Result<Vec<u8>>;
    fn publish(&mut self, path: &[u8], bytes: &[u8]) -> io::Result<()>;
}

pub struct SaveOptions<'a> {
    pub validation: ValidationSource<'a>,
    pub reset: Option<&'a mut ResetState>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    Save,
    SaveAs,
}
#[derive(Debug)]
pub struct SaveAttempt {
    pub selection: SaveSelection,
    pub legacy_choice: i32,
    pub warning_choice: Option<i32>,
    pub report: Option<SaveReport>,
    pub publication_error: Option<io::Error>,
}
#[derive(Debug)]
pub struct SaveFlow {
    pub outcome: SaveOutcome,
    pub attempts: Vec<SaveAttempt>,
    pub dialog_cancelled: bool,
}
#[derive(Debug)]
pub struct DirtyPrompt {
    pub choice: Option<i32>,
    pub result: i32,
    pub save: Option<SaveFlow>,
}

pub struct OpenFile {
    pub path: Vec<u8>,
    pub bytes: Vec<u8>,
}

/// The shared writer calls write_all once with its generated complete file.
/// Publication happens INSIDE that write, before legacy reload or baseline commit.
struct Publisher<'a, F> {
    files: &'a mut F,
    path: &'a [u8],
    written: bool,
}
impl<F: Files> Write for Publisher<'_, F> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.written {
            return Err(io::Error::other(
                "document writer requires one complete publication",
            ));
        }
        self.files.publish(self.path, bytes)?;
        self.written = true;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub flags: u32,
    pub show: u32,
    pub minimum: [i32; 2],
    pub maximum: [i32; 2],
    pub normal: [i32; 4],
}
/// GetWindowPlacement is a genuine external source. None means BOOL false.
pub trait PlacementSource {
    fn placement(&mut self) -> Result<Option<Placement>, Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormState {
    pub visible57: u8,
    pub enabled58: u8,
    pub focused: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewState {
    pub key: Vec<u8>,
    pub views: Vec<super::ValidationView>,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TreeState {
    pub keys: Vec<Vec<u8>>,
    pub selected: Option<usize>,
    pub groups: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuDescriptor {
    pub group: u32,
    pub key: u32,
    pub index: i32,
    pub name: Vec<u8>,
    pub caption: Vec<u8>,
    pub flags1d: u8,
    pub flags1e: u8,
    pub shortcut18: u32,
}
pub struct MenuContext<'a> {
    pub document: &'a mut Document,
    pub session: &'a mut Session,
    pub settings: &'a crate::display::Settings,
    pub active: Option<&'a NativeView>,
    pub recent: &'a RecentFiles,
}
type MenuQueryFn =
    dyn for<'a> Fn(&mut MenuContext<'a>, &mut MenuDescriptor) -> Result<(), Error> + Send + Sync;

/// Registered software code receives the SAME live objects, not snapshots or
/// caller-completed menu flags or caption packets.
#[derive(Clone)]
pub struct MenuQuery(Arc<MenuQueryFn>);
impl MenuQuery {
    pub fn new(
        query: impl for<'a> Fn(&mut MenuContext<'a>, &mut MenuDescriptor) -> Result<(), Error>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(query))
    }

    pub fn call(
        &self,
        context: &mut MenuContext<'_>,
        descriptor: &mut MenuDescriptor,
    ) -> Result<(), Error> {
        (self.0)(context, descriptor)
    }
}
impl fmt::Debug for MenuQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MenuQuery(..)")
    }
}
#[derive(Debug, Clone)]
pub struct MenuItem {
    pub descriptor: Option<MenuDescriptor>,
    pub query: Option<MenuQuery>,
    pub property19a: bool,
    pub property1ae: bool,
    pub property1a1: bool,
    pub enabled: bool,
    pub shortcut: u32,
    pub caption: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleState {
    pub path: Vec<u8>,
    pub dirty: u32,
    pub target: u32,
}
#[derive(Debug, Clone)]
pub struct UiState {
    pub native: Option<NativeState>,
    pub form: Option<FormState>,
    pub view: Option<ViewState>,
    pub tree: Option<TreeState>,
    pub task_period: i32,
    pub task_enabled: bool,
    /// Main-form-owned resource, not freed by document Close.
    pub image_list: Vec<u32>,
    pub application5b: u8,
    /// VA 8646AC, distinct from main-form visibility and query readiness.
    pub profile_ready: u8,
    pub operation40: u32,
    pub fc: u8,
    pub fd: u8,
    pub saved_placement: Option<Placement>,
    /// Resource-derived software sources. None is an actual headless no-tree,
    /// not a request to fabricate the missing native resource/factory classes.
    pub tree_source: Option<TreeState>,
    pub view_sources: Vec<ViewState>,
    pub menu: Vec<MenuItem>,
    /// Owned LIVE C state. Move the source here; query borrows it without taking
    /// a configuration snapshot. No implicit Settings defaults are invented.
    pub display_settings: Option<crate::display::Settings>,
    pub configuration: Option<crate::clone::ContextFields>,
    pub title: Option<TitleState>,
    pub validation: super::ValidationReport,
}
impl UiState {
    /// Explicit headless form with no native view/tree. Not a fabricated GUI tree.
    pub fn headless() -> Self {
        Self {
            native: None,
            form: Some(FormState {
                visible57: 1,
                enabled58: 1,
                focused: false,
            }),
            view: None,
            tree: None,
            task_period: 10,
            task_enabled: true,
            image_list: Vec::new(),
            application5b: 0,
            profile_ready: 0,
            operation40: 0,
            fc: 0,
            fd: 0,
            saved_placement: None,
            tree_source: None,
            view_sources: Vec::new(),
            menu: Vec::new(),
            display_settings: None,
            configuration: None,
            title: None,
            validation: super::ValidationReport::default(),
        }
    }
    pub fn owner_enabled(&mut self, enabled: u8) -> Result<(), Error> {
        self.form
            .as_mut()
            .ok_or(invalid("main form is absent"))?
            .enabled58 = enabled;
        Ok(())
    }
    /// Normal-return owner destruction tail; it does not restore a snapshot.
    pub fn owner_destroyed(&mut self) -> Result<(), Error> {
        self.owner_enabled(1)?;
        self.form.as_mut().unwrap().focused = true;
        Ok(())
    }
    pub fn sync28(&mut self, _document: &mut Document, busy: bool) -> bool {
        if busy {
            return false;
        }
        self.operation40 = 0;
        self.fc = 1;
        self.fd = 0;
        true
    }
    pub fn sync29(&mut self, _document: &mut Document) -> Result<(), Error> {
        self.application5b = 1;
        let form = self.form.as_mut().ok_or(invalid("main form is absent"))?;
        form.visible57 = 1;
        form.enabled58 = 1;
        form.focused = true;
        Ok(())
    }
    pub fn sync2f(&mut self, _document: &mut Document) -> Result<(), Error> {
        let form = self.form.as_mut().ok_or(invalid("main form is absent"))?;
        if form.visible57 == 1 {
            form.enabled58 = 0;
        }
        Ok(())
    }
    /// 23510->3EF44: query each registered descriptor then apply its LIVE bits.
    /// A missing query implementation is a software error, never OS waiting or a
    /// successful callback. A handler failure retains the already updated items.
    pub fn sync02(
        &mut self,
        document: &mut Document,
        session: &mut Session,
        recent: &RecentFiles,
    ) -> Result<(), Error> {
        if self.native.is_some() && self.menu.is_empty() {
            return Err(invalid(
                "native menu resource/query registration is unavailable",
            ));
        }
        for item in &mut self.menu {
            let Some(descriptor) = &mut item.descriptor else {
                item.property19a = false;
                item.property1a1 = false;
                continue;
            };
            let query = item.query.as_ref().ok_or(invalid(
                "native menu descriptor has no registered software query",
            ))?;
            let settings = self.display_settings.as_ref().ok_or(invalid(
                "menu query requires the live display Settings source",
            ))?;
            let active = self
                .native
                .as_ref()
                .and_then(|native| native.active.as_ref());
            query.call(
                &mut MenuContext {
                    document,
                    session,
                    settings,
                    active,
                    recent,
                },
                descriptor,
            )?;
            item.property1ae = descriptor.flags1d & 0x80 != 0;
            item.property19a = item.property1ae;
            item.enabled = descriptor.flags1e & 2 == 0;
            item.shortcut = descriptor.shortcut18;
            item.caption.clone_from(&descriptor.caption);
        }
        Ok(())
    }
    /// Document 25 ownership/data pass. Typed view coordinates come from actual
    /// registered class sources; current/baseline/pending are never cloned/reset.
    pub fn sync25(&mut self, document: &mut Document, session: &mut Session) -> Result<(), Error> {
        self.instantiate_native(document)?;
        let Some(tree) = &mut self.tree else {
            return Ok(());
        };
        if self.view.is_none()
            && self.form.is_some()
            && let Some(index) = tree.selected
        {
            let key = tree
                .keys
                .get(index)
                .ok_or(invalid("tree selection outside owned keys"))?;
            let source = self
                .view_sources
                .iter()
                .find(|v| &v.key == key)
                .ok_or(invalid(
                    "selected native view has no registered class source",
                ))?;
            self.view = Some(source.clone());
        }
        let count = u32::from_le_bytes(
            document
                .current
                .get(0x3fb40..0x3fb44)
                .ok_or(invalid("normal group count source unavailable"))?
                .try_into()
                .unwrap(),
        ) as usize;
        if count > 100 {
            return Err(invalid("normal group count outside source capacity"));
        }
        tree.groups.truncate(count);
        for index in 0..count {
            let at = 0x355c3 + 19 * index;
            let name = document
                .current
                .get(at..at + 16)
                .ok_or(invalid("normal group name source unavailable"))?;
            if index < tree.groups.len() {
                tree.groups[index] = name.to_vec();
            } else {
                tree.groups.push(name.to_vec());
            }
        }
        let _ = session;
        Ok(())
    }
    pub fn initial_boundary(
        &mut self,
        boundary: super::InitialBoundary,
        document: &mut Document,
        session: &mut Session,
    ) -> Result<(), Error> {
        use super::InitialBoundary;
        match boundary {
            InitialBoundary::Reconfigure => self.configuration = Some(session.context.fields()),
            InitialBoundary::Rebuild => {
                drop(self.tree.take());
                if self.native.is_some() {
                    self.rebuild_native(document)?;
                } else {
                    self.tree = self.tree_source.clone();
                }
            }
            InitialBoundary::ReleaseView => self.destroy_view(document),
            InitialBoundary::Title => {
                self.title = Some(TitleState {
                    path: document.path.clone(),
                    dirty: document.dirty,
                    target: document.target_revision,
                })
            }
            InitialBoundary::Sync(2) => {
                return Err(invalid(
                    "menu receiver requires the live Lifecycle MRU source",
                ));
            }
            InitialBoundary::Sync(0x25) => self.sync25(document, session)?,
            InitialBoundary::Sync(0x11) => {
                if self.native.is_some() {
                    return Err(invalid(
                        "native tree validation coordinate producer is unavailable",
                    ));
                }
                self.validation = document.validate_views(
                    session,
                    self.view.as_ref().map_or(ValidationSource::NoTree, |view| {
                        ValidationSource::Views(&view.views)
                    }),
                )?
            }
            // Document VMT slot 24 is RVA 4FB84: push ebp; mov ebp,esp;
            // pop ebp; ret. This concrete leaf has no effects or external input.
            InitialBoundary::Additional => {}
            // Registered document listeners cover 00..09 and 25..38. This says
            // nothing about other bus owners; do not acknowledge their work.
            InitialBoundary::Sync(0x49) => {
                return Err(invalid(
                    "event0x49 is outside registered lifecycle receiver ownership",
                ));
            }
            InitialBoundary::Sync(_) => {
                return Err(invalid("unregistered synchronous document receiver"));
            }
        }
        Ok(())
    }
    /// Local ownership release; current/path/pending stay on the SAME document.
    pub fn destroy_view(&mut self, _document: &mut Document) {
        drop(self.view.take());
        if let Some(native) = &mut self.native {
            native.active = None;
        }
    }
    /// Close callback order. No image-list free and no document reset are added.
    pub fn finish_close(&mut self, document: &mut Document) {
        self.task_period = -1;
        self.task_enabled = false;
        self.destroy_view(document);
        drop(self.tree.take());
        self.form = None;
    }
}

#[cfg(test)]
mod capturing_query_tests {
    use super::*;

    #[test]
    fn cloned_query_captures_state_and_updates_the_live_document() {
        let pending = b"CAPTURED".to_vec();
        let query = MenuQuery::new(move |context, descriptor| {
            context.document.begin_pending_name(7, pending.clone());
            context.session.document_flags[0] = 9;
            descriptor.caption = context.document.path().to_vec();
            Err(Error::Operation("query failed".into()))
        });
        let mut document = Document::from_current(vec![1], b"LIVE".to_vec(), 6);
        let mut session = Session::new(
            crate::icf::context::Context::before_registry(),
            None,
            0,
            [0; 2],
        );
        let settings = crate::display::Settings {
            language: 0,
            system_language: 0,
            font_percent: 100,
            toolbar: 1,
            split: 0,
            callsign_dialog: 1,
            auto_correct: 1,
            fixed_name: 0,
            column_bits: 0,
            column_bits_high: 0,
        };
        let recent = RecentFiles::default();
        let mut descriptor = MenuDescriptor {
            group: 0,
            key: 0,
            index: 0,
            name: vec![],
            caption: vec![],
            flags1d: 0,
            flags1e: 0,
            shortcut18: 0,
        };
        let result = query.clone().call(
            &mut MenuContext {
                document: &mut document,
                session: &mut session,
                settings: &settings,
                active: None,
                recent: &recent,
            },
            &mut descriptor,
        );
        assert!(result.is_err());
        assert_eq!(document.pending_name().unwrap().name, b"CAPTURED");
        assert_eq!(session.document_flags, [9, 0]);
        assert_eq!(descriptor.caption, b"LIVE");
        assert_eq!(format!("{query:?}"), "MenuQuery(..)");
    }
}

#[derive(Debug)]
pub struct Lifecycle<R = RecentFiles, U = UiState> {
    pub recent: R,
    pub ui: U,
}
impl<R: BorrowMut<RecentFiles>, U: BorrowMut<UiState>> Lifecycle<R, U> {
    pub fn sync02(&mut self, document: &mut Document, session: &mut Session) -> Result<(), Error> {
        self.ui
            .borrow_mut()
            .sync02(document, session, self.recent.borrow_mut())
    }

    pub fn initial_boundary(
        &mut self,
        boundary: super::InitialBoundary,
        document: &mut Document,
        session: &mut Session,
    ) -> Result<(), Error> {
        if boundary == super::InitialBoundary::Sync(2) {
            self.sync02(document, session)
        } else {
            self.ui
                .borrow_mut()
                .initial_boundary(boundary, document, session)
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Dialogs, publication and path comparison have independent failure boundaries."
    )]
    pub fn save(
        &mut self,
        document: &mut Document,
        session: &mut Session,
        mode: SaveMode,
        options: &mut SaveOptions<'_>,
        dialogs: &mut impl Dialogs,
        files: &mut impl Files,
        compare: &mut impl PathComparison,
    ) -> Result<SaveFlow, FileError> {
        if self.ui.borrow_mut().native.is_some()
            && matches!(options.validation, ValidationSource::NoTree)
        {
            return Err(
                invalid("native Save requires generated tree validation descriptors").into(),
            );
        }
        let mut flow = SaveFlow {
            outcome: SaveOutcome::NotSaved,
            attempts: Vec::new(),
            dialog_cancelled: false,
        };
        let mut dialog = mode == SaveMode::SaveAs || document.path.is_empty();
        loop {
            let selection = if dialog {
                let Some(selected) = dialogs.save_as(SaveDialog {
                    path: document.path.clone(),
                    directory: document.directory.clone(),
                    filter_index: target_filter(document.target_revision),
                })?
                else {
                    flow.dialog_cancelled = true;
                    return Ok(flow);
                };
                selected
            } else {
                SaveSelection {
                    path: document.path.clone(),
                    filter_index: target_filter(document.target_revision),
                }
            };
            let target = if dialog {
                filter_target(selection.filter_index)
            } else {
                document.target_revision
            };
            let legacy = if target != 6 {
                dialogs.choice(Prompt::Legacy { target })?
            } else {
                0
            };
            let warning = if target != 6 && matches!(legacy, 2 | 7) {
                None
            } else {
                let counts = document.validate_views(session, options.validation)?;
                if counts.errors == 0 && counts.warnings != 0 {
                    Some(dialogs.choice(Prompt::Warning {
                        count: counts.warnings,
                    })?)
                } else {
                    None
                }
            };
            let mut publisher = Publisher {
                files,
                path: &selection.path,
                written: false,
            };
            let result = document.save_with_validation_uncommitted(
                session,
                SaveRequest {
                    path: selection.path.clone(),
                    target_revision: target,
                    legacy_choice: legacy,
                    warning_choice: warning,
                    validation: options.validation,
                },
                options.reset.as_deref_mut(),
                &mut publisher,
            );
            let (report, publication_error) = match result {
                Ok(report) => (Some(report), None),
                Err(FileError::Output(error)) => (None, Some(error)),
                Err(error) => return Err(error),
            };
            let status = report.as_ref().map(|r| r.status);
            flow.attempts.push(SaveAttempt {
                selection: selection.clone(),
                legacy_choice: legacy,
                warning_choice: warning,
                report,
                publication_error,
            });
            match status {
                Some(SaveStatus::RetrySaveAs) => {
                    dialog = true;
                }
                Some(SaveStatus::Saved) => {
                    if target != 6 {
                        self.ui.borrow_mut().sync25(document, session)?;
                    }
                    document.path = selection.path.clone();
                    self.recent.borrow_mut().insert(selection.path, compare)?;
                    document.accept_saved(document.path.clone(), target);
                    self.ui.borrow_mut().initial_boundary(
                        super::InitialBoundary::Title,
                        document,
                        session,
                    )?;
                    session.document_flags = [0, 0];
                    flow.outcome = SaveOutcome::Saved;
                    return Ok(flow);
                }
                _ => return Ok(flow),
            }
        }
    }
    pub fn dirty_prompt(
        &mut self,
        document: &mut Document,
        session: &mut Session,
        options: &mut SaveOptions<'_>,
        dialogs: &mut impl Dialogs,
        files: &mut impl Files,
        compare: &mut impl PathComparison,
    ) -> Result<DirtyPrompt, FileError> {
        if document.dirty == 0 {
            return Ok(DirtyPrompt {
                choice: None,
                result: resolve_prompt(0, 0, None)?,
                save: None,
            });
        }
        let choice = dialogs.choice(Prompt::Dirty)?;
        let save = if choice == 6 {
            Some(self.save(
                document,
                session,
                SaveMode::Save,
                options,
                dialogs,
                files,
                compare,
            )?)
        } else {
            None
        };
        let result = resolve_prompt(document.dirty, choice, save.as_ref().map(|s| s.outcome))?;
        Ok(DirtyPrompt {
            choice: Some(choice),
            result,
            save,
        })
    }
    pub fn new_file(
        &mut self,
        document: &mut Document,
        session: &mut Session,
        options: &mut SaveOptions<'_>,
        dialogs: &mut impl Dialogs,
        files: &mut impl Files,
        compare: &mut impl PathComparison,
    ) -> Result<DirtyPrompt, FileError> {
        let prompt = self.dirty_prompt(document, session, options, dialogs, files, compare)?;
        if document.new_file(session, prompt.result)? {
            self.ui.borrow_mut().initial_boundary(
                super::InitialBoundary::Title,
                document,
                session,
            )?;
            self.ui.borrow_mut().initial_boundary(
                super::InitialBoundary::Rebuild,
                document,
                session,
            )?;
            self.ui.borrow_mut().destroy_view(document);
            self.ui.borrow_mut().sync25(document, session)?;
        }
        Ok(prompt)
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "The open root keeps prompt, profile, asset and file sources explicit."
    )]
    pub fn open(
        &mut self,
        document: &mut Document,
        session: &mut Session,
        path: Option<Vec<u8>>,
        profile_choice: Option<i32>,
        defaults: Option<DefaultSource<'_>>,
        options: &mut SaveOptions<'_>,
        dialogs: &mut impl Dialogs,
        files: &mut impl Files,
        compare: &mut impl PathComparison,
    ) -> Result<(DirtyPrompt, OpenStatus), FileError> {
        let prompt = self.dirty_prompt(document, session, options, dialogs, files, compare)?;
        if prompt.result == 2 || path.is_none() {
            return Ok((prompt, OpenStatus::Cancelled));
        }
        let path = path.unwrap();
        let bytes = files.read(&path).map_err(FileError::Output)?;
        self.open_after_prompt(
            document,
            session,
            prompt,
            Some(OpenFile { path, bytes }),
            profile_choice,
            defaults,
            compare,
        )
    }
    /// Used by file frontends to acquire adopted-Q assets after the actual dirty
    /// prompt and input read. No repeated prompt/read and no completed Open report.
    #[allow(
        clippy::too_many_arguments,
        reason = "The frontend supplies a completed prompt and independently acquired file and assets."
    )]
    pub fn open_after_prompt(
        &mut self,
        document: &mut Document,
        session: &mut Session,
        prompt: DirtyPrompt,
        file: Option<OpenFile>,
        profile_choice: Option<i32>,
        defaults: Option<DefaultSource<'_>>,
        compare: &mut impl PathComparison,
    ) -> Result<(DirtyPrompt, OpenStatus), FileError> {
        if prompt.result == 2 || file.is_none() {
            return Ok((prompt, OpenStatus::Cancelled));
        }
        let OpenFile { path, bytes } = file.unwrap();
        let status = document.read(
            Some(&bytes),
            path.clone(),
            prompt.result,
            profile_choice,
            session,
            defaults,
        )?;
        if matches!(status, OpenStatus::Opened { .. }) {
            self.recent.borrow_mut().insert(path, compare)?;
            self.ui.borrow_mut().destroy_view(document);
            self.ui.borrow_mut().sync25(document, session)?;
            self.ui.borrow_mut().initial_boundary(
                super::InitialBoundary::Title,
                document,
                session,
            )?;
        }
        Ok((prompt, status))
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "MRU selection forwards the same independent open sources without capturing them."
    )]
    pub fn open_recent(
        &mut self,
        document: &mut Document,
        session: &mut Session,
        index: usize,
        profile_choice: Option<i32>,
        defaults: Option<DefaultSource<'_>>,
        options: &mut SaveOptions<'_>,
        dialogs: &mut impl Dialogs,
        files: &mut impl Files,
        compare: &mut impl PathComparison,
    ) -> Result<(DirtyPrompt, OpenStatus), FileError> {
        let path = self.recent.borrow_mut().selected(index)?.to_vec();
        self.open(
            document,
            session,
            Some(path),
            profile_choice,
            defaults,
            options,
            dialogs,
            files,
            compare,
        )
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "Placement is acquired only after the independently fallible pending-save gate."
    )]
    pub fn close_query(
        &mut self,
        document: &mut Document,
        session: &mut Session,
        incoming: &mut bool,
        options: &mut SaveOptions<'_>,
        dialogs: &mut impl Dialogs,
        files: &mut impl Files,
        compare: &mut impl PathComparison,
        placement: &mut impl PlacementSource,
    ) -> Result<Option<DirtyPrompt>, FileError> {
        let form = self
            .ui
            .borrow_mut()
            .form
            .as_ref()
            .ok_or(invalid("main form is absent"))?;
        if form.enabled58 == 0 {
            *incoming = false;
            return Ok(None);
        }
        let prompt = self.dirty_prompt(document, session, options, dialogs, files, compare)?;
        if prompt.result == 2 {
            *incoming = false;
            return Ok(Some(prompt));
        }
        if let Some(value) = placement.placement()? {
            self.ui.borrow_mut().saved_placement = Some(value);
        }
        Ok(Some(prompt))
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "Close forwards the explicit save and placement sources before releasing ownership."
    )]
    pub fn close(
        &mut self,
        document: &mut Document,
        session: &mut Session,
        incoming: &mut bool,
        options: &mut SaveOptions<'_>,
        dialogs: &mut impl Dialogs,
        files: &mut impl Files,
        compare: &mut impl PathComparison,
        placement: &mut impl PlacementSource,
    ) -> Result<Option<DirtyPrompt>, FileError> {
        let prompt = self.close_query(
            document, session, incoming, options, dialogs, files, compare, placement,
        )?;
        if *incoming {
            self.ui.borrow_mut().finish_close(document);
        }
        Ok(prompt)
    }
}
