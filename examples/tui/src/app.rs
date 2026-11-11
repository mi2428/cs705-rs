//! Session event loop. The single I/O thread is not a CLI subprocess.
//! Rendering and navigation never open a serial endpoint.
//!
//! Acknowledgement and readback are different state boundaries:
//! ```text
//! loaded -> local draft -> confirmed batch -> E6 ACK -> pending restart
//!                      \-> started-write error -> uncertain (no retry)
//! pending restart -> operator confirmation -> readback -> new loaded base
//! ```
//! The draft and its original survive every failed job. `write_started` is a
//! conservative at-most-once guard, not evidence that a wire byte was accepted.

use crate::{
    Result,
    activity::{Entry as LogEntry, Level},
    catalog::{self, Category, Entry, Field, Target},
    encoding::{self, TextEncoding},
    model::{self, Editor, Snapshot},
    radio::Radio,
    storage, theme,
};
use cs705::{
    icf::record::ChannelField,
    presentation::{Language, Message, Presenter},
};
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{
        self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind,
    },
    layout::{Position, Rect},
    widgets::{ListState, TableState},
};
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

mod input;
mod navigation;
use navigation::{NodeId, Section};
#[cfg(test)]
mod tests;
mod view;

#[derive(Clone)]
pub enum Source {
    File(PathBuf),
    Radio(Radio),
}
enum Finished {
    Loaded(Snapshot),
    Saved(Snapshot, PathBuf, PathBuf),
    Written(Snapshot, PathBuf),
    Verified(Snapshot),
}
struct Job {
    receiver: Receiver<Result<Finished>>,
    logs: Receiver<LogEntry>,
    thread: JoinHandle<()>,
    cancelled: Arc<AtomicBool>,
    write_started: Arc<AtomicBool>,
}

enum FormKind {
    Edit,
    New,
    Copy,
    Group,
    Setting(Target),
    Encoding,
}
struct Form {
    kind: FormKind,
    title: String,
    hint: String,
    error: Option<String>,
    fields: Vec<Field>,
    selected: usize,
    replace: bool,
}
enum Dialog {
    Form(Form),
    Diff(Vec<String>, u16),
    Apply(Vec<String>),
    Verify,
    Discard,
    Reload,
    Quit,
    Help(u16),
    Details(u16),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    Categories,
    Items,
    Logs,
}

pub struct App {
    encoding: TextEncoding,
    source: Source,
    backup_dir: PathBuf,
    editor: Option<Editor>,
    group: usize,
    row: usize,
    category: Category,
    pane: Pane,
    expanded: HashSet<NodeId>,
    section: Option<Section>,
    row_group: Option<usize>,
    navigation: usize,
    entries: Vec<Entry>,
    load_error: Option<String>,
    source_error: Option<String>,
    filter: String,
    filtering: bool,
    groups: ListState,
    rows: TableState,
    pane_areas: [Rect; 3],
    tree_rows: Rect,
    item_rows: Rect,
    log_rows: Rect,
    logs: VecDeque<LogEntry>,
    log_scroll: usize,
    log_horizontal: u16,
    dialog: Option<Dialog>,
    job: Option<Job>,
    pending: Option<Snapshot>,
    uncertain: bool,
    status: String,
    quit: bool,
    size_ok: bool,
}

impl App {
    pub fn new(source: Source, backup_dir: PathBuf) -> Self {
        Self {
            encoding: TextEncoding::default(),
            source,
            backup_dir,
            editor: None,
            group: 0,
            row: 0,
            category: Category::Memory,
            pane: Pane::Categories,
            expanded: Section::ALL
                .into_iter()
                .map(NodeId::Section)
                .chain([NodeId::Category(Category::Memory)])
                .collect(),
            section: None,
            row_group: None,
            navigation: 0,
            entries: Vec::new(),
            load_error: None,
            source_error: None,
            filter: String::new(),
            filtering: false,
            groups: ListState::default(),
            rows: TableState::default(),
            pane_areas: [Rect::default(); 3],
            tree_rows: Rect::default(),
            item_rows: Rect::default(),
            log_rows: Rect::default(),
            logs: VecDeque::from([LogEntry::new(
                Level::Info,
                "Session created. No device write has started.",
            )]),
            log_scroll: 0,
            log_horizontal: 0,
            dialog: None,
            job: None,
            pending: None,
            uncertain: false,
            status: "Loading; no changes have been sent.".into(),
            quit: false,
            size_ok: true,
        }
    }
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        self.reload();
        while !self.quit {
            self.poll();
            terminal.draw(|frame| self.draw(frame))?;
            if event::poll(Duration::from_millis(80))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => self.key(key),
                    Event::Paste(text) => self.paste(&text),
                    Event::Mouse(mouse) => self.mouse(mouse),
                    _ => {}
                }
            }
        }
        Ok(())
    }
    pub fn shutdown(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancelled.store(true, Ordering::Relaxed);
            let _ = job.thread.join();
            self.uncertain |= job.write_started.load(Ordering::Relaxed);
        }
    }
    pub fn needs_verification(&self) -> bool {
        self.pending.is_some() || self.uncertain
    }
    pub fn set_encoding(&mut self, encoding: TextEncoding) {
        self.encoding = encoding;
        if let Some(editor) = &mut self.editor {
            editor.encoding = encoding;
        }
        self.refresh();
    }
    fn text(&self, bytes: &[u8]) -> String {
        self.encoding.display(bytes)
    }
    fn push_log(&mut self, entry: LogEntry) {
        if self.log_scroll > 0 {
            self.log_scroll += 1;
        }
        self.logs.push_back(entry);
        if self.logs.len() > 500 {
            self.logs.pop_front();
        }
        self.log_scroll = self.log_scroll.min(
            self.logs
                .len()
                .saturating_sub(self.log_rows.height as usize),
        );
    }
    fn set_status(&mut self, level: Level, message: impl Into<String>) {
        self.status = message.into();
        self.push_log(LogEntry::new(level, self.status.clone()));
    }
    fn start(
        &mut self,
        task: impl FnOnce(Arc<AtomicBool>, Arc<AtomicBool>, Sender<LogEntry>) -> Result<Finished>
        + Send
        + 'static,
    ) {
        let (sender, receiver) = mpsc::channel();
        let (log, logs) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let write_started = Arc::new(AtomicBool::new(false));
        let cancel = cancelled.clone();
        let started = write_started.clone();
        let thread = thread::spawn(move || {
            let _ = sender.send(task(cancel, started, log));
        });
        self.job = Some(Job {
            receiver,
            logs,
            thread,
            cancelled,
            write_started,
        });
    }
    fn reload(&mut self) {
        let source = self.source.clone();
        self.set_status(
            Level::Info,
            "Reading source; Escape requests cancellation. No write is performed.",
        );
        self.start(move |cancel, _, log| {
            Ok(Finished::Loaded(match source {
                Source::File(path) => {
                    crate::activity::report(
                        &log,
                        Level::Info,
                        format!("Reading ICF: {}", path.display()),
                    );
                    storage::load(&path)?
                }
                Source::Radio(radio) => radio.read(&cancel, &log)?,
            }))
        });
    }
    fn poll(&mut self) {
        if let Some(job) = &self.job {
            let entries = job.logs.try_iter().take(128).collect::<Vec<_>>();
            for entry in entries {
                self.push_log(entry);
            }
        }
        let result = match self.job.as_ref().map(|job| job.receiver.try_recv()) {
            None | Some(Err(TryRecvError::Empty)) => return,
            Some(Err(TryRecvError::Disconnected)) => Err("I/O worker stopped unexpectedly".into()),
            Some(Ok(result)) => result,
        };
        let job = self.job.take().expect("polled job exists");
        let result = if job.thread.join().is_err() {
            Err("I/O worker panicked; inspect the device before continuing".into())
        } else {
            result
        };
        for entry in job.logs.try_iter() {
            self.push_log(entry);
        }
        if let Err(error) = self.finish(result) {
            self.uncertain |= job.write_started.load(Ordering::Relaxed);
            self.set_status(
                Level::Error,
                format!(
                    "{}{} Backup directory: {}",
                    if self.uncertain {
                        "WRITE STATE UNCONFIRMED. No automatic retry. "
                    } else {
                        "Error: "
                    },
                    encoding::safe_text(&error.to_string()),
                    self.backup_dir.display()
                ),
            );
            if self.editor.is_none() {
                self.source_error = Some(self.status.clone());
            }
        }
    }
    fn finish(&mut self, result: Result<Finished>) -> Result<()> {
        match result? {
            Finished::Loaded(snapshot) => {
                self.editor = Some(Editor::new(snapshot)?);
                self.pending = None;
                self.uncertain = false;
                self.source_error = None;
                self.set_status(Level::Info, "Loaded. Edits remain local until Apply.");
            }
            Finished::Saved(snapshot, path, backup) => {
                self.editor = Some(Editor::new(snapshot)?);
                self.source = Source::File(path.clone());
                self.source_error = None;
                self.set_status(
                    Level::Info,
                    format!(
                        "Saved new ICF: {}. Original backup: {}",
                        path.display(),
                        backup.display()
                    ),
                );
            }
            Finished::Written(expected, backup) => {
                self.pending = Some(expected);
                self.set_status(
                    Level::Warning,
                    format!(
                        "{} Power-cycle the radio, then press v. Backup: {}",
                        Presenter::new(Language::English).message(Message::WriteAcknowledged),
                        backup.display()
                    ),
                );
            }
            Finished::Verified(snapshot) => {
                let expected = self.pending.as_ref().ok_or("no pending write")?;
                let other = snapshot.native[..model::STORAGE]
                    .iter()
                    .zip(&expected.native[..model::STORAGE])
                    .filter(|(a, b)| a != b)
                    .count();
                self.editor = Some(Editor::new(snapshot)?);
                self.pending = None;
                self.uncertain = false;
                self.set_status(Level::Info, format!(
                    "Edited data verified after operator-confirmed restart. Other storage differences: {other}."
                ));
            }
        }
        if let Some(editor) = &mut self.editor {
            editor.encoding = self.encoding;
        }
        self.sync_navigation();
        self.refresh();
        Ok(())
    }
    fn apply(&mut self) -> Result<()> {
        let editor = self.editor.as_ref().ok_or("no image loaded")?;
        let original = editor.original.clone();
        let candidate = editor.candidate()?;
        let directory = self.backup_dir.clone();
        let source = self.source.clone();
        self.set_status(
            Level::Info,
            "Checking for conflicts, saving backup, then applying. Do not disconnect or power off.",
        );
        self.start(move |cancel, started, log| match source {
            Source::File(path) => {
                crate::activity::report(
                    &log,
                    Level::Info,
                    "Checking input file and saving backup before batch save.",
                );
                let (snapshot, output, backup) =
                    storage::save(&path, &original, &candidate, &directory)?;
                Ok(Finished::Saved(snapshot, output, backup))
            }
            Source::Radio(radio) => {
                let (expected, backup) =
                    radio.apply(&original, &candidate, &directory, &cancel, &started, &log)?;
                Ok(Finished::Written(expected, backup))
            }
        });
        Ok(())
    }
    fn verify(&mut self) -> Result<()> {
        let expected = self
            .pending
            .clone()
            .ok_or("no acknowledged write to verify")?;
        let Source::Radio(radio) = self.source.clone() else {
            return Err("verification requires radio mode".into());
        };
        let original = self
            .editor
            .as_ref()
            .ok_or("no original snapshot")?
            .original
            .clone();
        let candidate = self.editor.as_ref().ok_or("no draft")?.candidate()?;
        self.set_status(
            Level::Info,
            "Reading the restarted radio and verifying edited data.",
        );
        self.start(move |cancel, _, log| {
            Ok(Finished::Verified(
                radio.verify(&original, &candidate, &expected, &cancel, &log)?,
            ))
        });
        Ok(())
    }
    fn key(&mut self, key: KeyEvent) {
        if let Some(job) = &self.job {
            if key.code == KeyCode::Esc
                || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
            {
                if !job.cancelled.swap(true, Ordering::Relaxed) {
                    self.set_status(
                        Level::Warning,
                        "Cancellation requested; waiting for protocol close. No automatic retry.",
                    );
                }
            } else if matches!(
                key.code,
                KeyCode::Tab
                    | KeyCode::BackTab
                    | KeyCode::Char('1' | '2' | '3')
                    | KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Home
                    | KeyCode::End
            ) {
                // Browsing/log scrolling is safe while the captured I/O job runs.
                if let Err(error) = self.handle_key(key) {
                    self.set_status(Level::Error, error.to_string());
                }
            }
            return;
        }
        if !self.size_ok && !matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
            return;
        }
        if let Err(error) = self.handle_key(key) {
            self.set_status(
                Level::Error,
                format!("Error: {}", encoding::safe_text(&error.to_string())),
            );
        }
    }
    fn refresh(&mut self) {
        self.entries.clear();
        self.load_error = None;
        if self.section.is_none()
            && self.category != Category::Memory
            && let Some(editor) = &self.editor
        {
            match catalog::grouped_entries(editor, self.category, self.row_group) {
                Ok(entries) => self.entries = entries,
                Err(error) => self.load_error = Some(encoding::safe_text(&error.to_string())),
            }
        }
        self.row = self.row.min(self.visible().len().saturating_sub(1));
    }
    fn visible(&self) -> Vec<usize> {
        if let Some(section) = self.section {
            return (0..section.categories().len()).collect();
        }
        if self.editor.is_none() {
            return Vec::new();
        }
        let query = self.filter.to_lowercase();
        if self.category == Category::Memory {
            (0..100)
                .filter(|&r| {
                    if query.is_empty() {
                        return true;
                    }
                    self.editor
                        .as_ref()
                        .is_some_and(|e| match e.row(self.group, r) {
                            Ok(Some(h)) => format!(
                                "{r:02} {} {}",
                                self.text(h.name()),
                                model::mhz_text(h.channel_field(ChannelField::FrequencyHz))
                            )
                            .to_lowercase()
                            .contains(&query),
                            Ok(None) => format!("{r:02} empty").contains(&query),
                            Err(_) => false,
                        })
                })
                .collect()
        } else {
            self.entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| {
                    format!("{} {}", e.label, e.value)
                        .to_lowercase()
                        .contains(&query)
                        .then_some(i)
                })
                .collect()
        }
    }
    fn selected(&self) -> Option<usize> {
        self.visible().get(self.row).copied()
    }
}
impl Drop for App {
    fn drop(&mut self) {
        self.shutdown();
    }
}
