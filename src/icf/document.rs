//! Document state shared by file, field-edit and menu integrations.
//!
//! Current/default/baseline are distinct content generations. Dirty comparison
//! covers the complete current image, including expanded working-tail bytes,
//! and is sticky: reverting bytes alone never clears an already nonzero value.
//! Source: A2 ownership-and-lifecycle-ba01 and accepted BA26 common edit tail.
//!
//! File entry points consume the actual shared stage7/8/9 driver and MapRev
//! conversion. Native GUI callbacks and file publication remain separate from
//! image normalization; neither is replaced with a successful no-op callback.

use super::{Error, image::apply_patches};

#[path = "document_io.rs"]
mod io;
pub use io::{
    DefaultSource, FileError, OpenStatus, ResetEffects, SaveReport, SaveRequest, SaveStatus,
    Session, ValidationReport, ValidationSource, ValidationView,
};

#[path = "document_clone.rs"]
mod clone_write;
pub use clone_write::{
    CloneControls, CloneError, ClonePreparation, ClonePreparationStage, CloneReply, CloneStep,
    CloneStop, CloneWrite,
};

#[path = "document_initial.rs"]
pub mod initial;
pub use initial::{InitialBoundary, InitialRequest, InitialState};

#[path = "document_read.rs"]
pub mod native_read;
pub use native_read::{CloneRead, CloneReadReply, CloneReadStep, CloneReadStop};

#[path = "document_options.rs"]
pub mod options;
pub use options::{CloneOptionButton, CloneOptions};

#[path = "document_lifecycle.rs"]
pub mod lifecycle;

#[path = "document_ui_resources.rs"]
pub mod ui_resources;

/// Original Yes/No/Cancel prompt's save result, separate from the user's choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveOutcome {
    /// Save returned zero.
    Saved,
    /// Save failed or was cancelled, returning one.
    NotSaved,
}

/// Resolve 0x2480C without collapsing native MessageBox return values to bool.
/// Dirty zero returns 6 without showing/saving. For dirty nonzero, choice 6
/// invokes Save and maps failure to 2; every other native result is preserved.
pub fn resolve_prompt(dirty: u32, choice: i32, save: Option<SaveOutcome>) -> Result<i32, Error> {
    if dirty == 0 {
        return Ok(6);
    }
    if choice != 6 {
        return Ok(choice);
    }
    match save {
        Some(SaveOutcome::Saved) => Ok(6),
        Some(SaveOutcome::NotSaved) => Ok(2),
        None => Err(invalid("Yes prompt requires the actual Save outcome")),
    }
}

/// Common grid context words read after the field handler and again after callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActionContext {
    /// context+40: action/command.
    pub action: u32,
    /// context+64: nonzero also requests the document tail.
    pub state: u32,
    /// context+44: nonzero also requests the document tail.
    pub phase: u32,
    /// Raw callback-address word. The original tests its high word, not pointer!=0.
    pub callback_address: u32,
}

/// Pending normal-memory name state is outside current/baseline image contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingName {
    /// document+190 byte, tested for nonzero rather than canonicalized to bool.
    pub active: u8,
    /// document+194 typed index, not a group or physical-slot ordinal.
    pub index: i32,
    /// Logical byte string, including any NULs supplied by its text producer.
    pub name: Vec<u8>,
}

/// Normal-return effects; an exceptional callback has no successful tail report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionEffects {
    /// The native callback guard was satisfied and the supplied callback ran.
    pub callback_invoked: bool,
    /// Timer period requested before dirty comparison, if the tail was reached.
    pub timer_ms: Option<u32>,
    /// Current sticky dirty DWORD, not a canonicalized boolean.
    pub dirty: u32,
}

/// Owned document content. Native path/title/view rendering is not executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    current: Vec<u8>,
    baseline: Vec<u8>,
    path: Vec<u8>,
    target_revision: u32,
    dirty: u32,
    timer_ms: Option<u32>,
    pending_name: Option<PendingName>,
    directory: Vec<u8>,
    original: Vec<u8>,
    comment: Vec<u8>,
    extra_data: u32,
}

impl Document {
    /// Adopt an explicitly supplied current image as the clean baseline.
    /// No factory bytes, file validity or profile compatibility are inferred.
    pub fn from_current(current: Vec<u8>, path: Vec<u8>, target_revision: u32) -> Self {
        Self {
            baseline: current.clone(),
            current,
            path,
            target_revision,
            dirty: 0,
            timer_ms: None,
            pending_name: None,
            directory: Vec::new(),
            original: Vec::new(),
            comment: Vec::new(),
            extra_data: 0,
        }
    }

    /// Entire current object contents, not merely the serialized ICF prefix.
    pub fn current(&self) -> &[u8] {
        &self.current
    }
    /// Last explicitly accepted clean baseline.
    pub fn baseline(&self) -> &[u8] {
        &self.baseline
    }
    /// Raw path bytes without lossy Unicode decoding or normalization.
    pub fn path(&self) -> &[u8] {
        &self.path
    }
    /// Raw document save target, distinct from converter-local normalization.
    pub fn target_revision(&self) -> u32 {
        self.target_revision
    }
    /// Sticky dirty DWORD.
    pub fn dirty(&self) -> u32 {
        self.dirty
    }
    /// Most recently requested timer period.
    pub fn timer_ms(&self) -> Option<u32> {
        self.timer_ms
    }
    /// Pending name can survive New when no listener explicitly resets it.
    pub fn pending_name(&self) -> Option<&PendingName> {
        self.pending_name.as_ref()
    }
    /// Publish/reset pending state explicitly; image replacement does not guess it.
    pub fn set_pending_name(&mut self, pending: Option<PendingName>) {
        self.pending_name = pending;
    }

    /// 53218: publish active flag, typed index and the live logical string.
    pub fn begin_pending_name(&mut self, index: i32, name: Vec<u8>) {
        self.pending_name = Some(PendingName {
            active: 1,
            index,
            name,
        });
    }

    /// 53240's inactive/index-FFFFFFFF/empty-string state.
    pub fn reset_pending_name(&mut self) {
        self.pending_name = None;
    }

    /// Test flag/index without inventing a content epoch or image identity check.
    pub fn pending_matches(&self, index: i32) -> bool {
        self.pending_name
            .as_ref()
            .is_some_and(|p| p.active != 0 && p.index == index)
    }

    /// B5A78's nonempty handoff resets pending before a later image writer.
    /// Empty names do not take this branch. A failed later writer must not restore
    /// the consumed pending state. Queue receivers read the live name, not a name
    /// supposedly captured by their copied 24-byte notification.
    pub fn take_pending_nonempty(&mut self) -> Option<Vec<u8>> {
        if self
            .pending_name
            .as_ref()
            .is_some_and(|p| p.active != 0 && !p.name.is_empty())
        {
            self.pending_name.take().map(|p| p.name)
        } else {
            None
        }
    }

    /// Checked atomic raw integration edit; this does not request a GUI callback.
    pub fn patch_current(&mut self, patches: &[(usize, &[u8])]) -> Result<(), Error> {
        apply_patches(&mut self.current, patches)
    }

    /// Replace current only, retaining path/baseline/dirty and pending state.
    /// For example, old-format conversion can commit before later native failure.
    pub fn replace_current(&mut self, current: Vec<u8>) {
        self.current = current;
    }

    /// 0x24B68: compare complete current/baseline only while dirty is zero.
    /// A nonzero dirty value is preserved exactly, even after bytes are reverted.
    pub fn compare_dirty(&mut self) -> u32 {
        if self.dirty == 0 && self.current != self.baseline {
            self.dirty = 1;
        }
        self.dirty
    }

    /// 0x7FEEB..7FF3E common edit tail, with an explicit native callback boundary.
    ///
    /// `notify && flags!=0 && highword(callback)!=0` gates only the callback.
    /// After normal return, live context action/state/phase independently requests
    /// timer 250 then sticky dirty comparison. A callback error retains its prior
    /// mutations and skips this later tail; it is never caught and continued.
    pub fn finish_action(
        &mut self,
        context: &mut ActionContext,
        notify: bool,
        flags: u32,
        callback: impl FnOnce(&mut Self, &mut ActionContext, u32) -> Result<(), Error>,
    ) -> Result<ActionEffects, Error> {
        let callback_invoked = notify && flags != 0 && context.callback_address >> 16 != 0;
        if callback_invoked {
            callback(self, context, flags)?;
        }
        let tail = context.action != 0 || context.state != 0 || context.phase != 0;
        if tail {
            self.timer_ms = Some(250);
            self.compare_dirty();
        }
        Ok(ActionEffects {
            callback_invoked,
            timer_ms: tail.then_some(250),
            dirty: self.dirty,
        })
    }

    /// New's current/default/baseline commit after the actual prompt has returned.
    /// Only native result 2 cancels. The defaults must come from an explicitly
    /// completed source recipe, not an unknown/zero/fallback factory image.
    /// No indirect listener effects or pending reset are invented here.
    pub fn new_from_defaults(&mut self, defaults: &[u8], prompt_result: i32) -> bool {
        if prompt_result == 2 {
            return false;
        }
        self.target_revision = 6;
        self.current = defaults.to_vec();
        self.baseline.clone_from(&self.current);
        self.dirty = 0;
        self.path.clear();
        true
    }

    /// Accept the final save boundary only after writer and required round-trip /
    /// normalization callbacks have all completed. Failed writers must not call
    /// this method: prior current normalization then remains visible and dirty.
    pub fn accept_saved(&mut self, path: Vec<u8>, target_revision: u32) {
        self.path = path;
        self.target_revision = target_revision;
        self.baseline.clone_from(&self.current);
        self.dirty = 0;
    }
}

fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 0, reason }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticky_dirty_compares_tail_and_length_and_preserves_native_prompt_results() {
        let mut doc = Document::from_current(vec![7; 0x4997a], b"OLD".to_vec(), 6);
        doc.patch_current(&[(0x49979, &[8])]).unwrap();
        assert_eq!(doc.compare_dirty(), 1);
        doc.patch_current(&[(0x49979, &[7])]).unwrap();
        assert_eq!(doc.compare_dirty(), 1);
        doc.dirty = 0x100;
        assert_eq!(doc.compare_dirty(), 0x100);
        assert_eq!(resolve_prompt(0, 2, None).unwrap(), 6);
        assert_eq!(
            resolve_prompt(1, 6, Some(SaveOutcome::NotSaved)).unwrap(),
            2
        );
        assert_eq!(resolve_prompt(1, 6, Some(SaveOutcome::Saved)).unwrap(), 6);
        assert!(resolve_prompt(1, 6, None).is_err());
        assert_eq!(resolve_prompt(1, 0x100, None).unwrap(), 0x100);
        doc.accept_saved(b"NEW".to_vec(), 5);
        doc.replace_current(vec![7; 0x49979]);
        assert_eq!(doc.compare_dirty(), 1);
    }

    #[test]
    fn no_notify_still_requests_timer_and_callback_highword_is_not_nonnull() {
        let mut doc = Document::from_current(vec![1, 2], vec![], 6);
        let mut context = ActionContext {
            action: 13,
            callback_address: 0x1234,
            ..Default::default()
        };
        doc.patch_current(&[(1, &[9])]).unwrap();
        let result = doc
            .finish_action(&mut context, true, 0x20, |_, _, _| {
                panic!("low-word-only callback must not run")
            })
            .unwrap();
        assert_eq!(
            result,
            ActionEffects {
                callback_invoked: false,
                timer_ms: Some(250),
                dirty: 1
            }
        );
        doc.accept_saved(vec![], 6);
        context.callback_address = 0x12340000;
        assert!(
            !doc.finish_action(&mut context, false, 0x20, |_, _, _| panic!("notify false"))
                .unwrap()
                .callback_invoked
        );
        assert_eq!(doc.dirty(), 0);
    }

    #[test]
    fn callback_error_retains_prefix_and_normal_return_rereads_context() {
        let mut doc = Document::from_current(vec![1, 2], vec![], 6);
        let mut context = ActionContext {
            action: 13,
            callback_address: 0x10000,
            ..Default::default()
        };
        let failure = invalid("synthetic callback failure");
        assert_eq!(
            doc.finish_action(&mut context, true, 0x20, |doc, _, _| {
                doc.patch_current(&[(0, &[9])])?;
                Err(invalid("synthetic callback failure"))
            }),
            Err(failure)
        );
        assert_eq!(doc.current(), [9, 2]);
        assert_eq!(doc.timer_ms(), None);
        assert_eq!(doc.dirty(), 0);
        let result = doc
            .finish_action(&mut context, true, 0x20, |_, context, _| {
                context.action = 0;
                Ok(())
            })
            .unwrap();
        assert_eq!(result.timer_ms, None);
        assert_eq!(doc.compare_dirty(), 1);
    }

    #[test]
    fn new_cancel_and_pending_generations_are_not_implicit_rollback() {
        let mut doc = Document::from_current(vec![1, 2], b"OLD".to_vec(), 5);
        doc.set_pending_name(Some(PendingName {
            active: 1,
            index: 4,
            name: b"PENDING".to_vec(),
        }));
        let before = doc.clone();
        assert!(!doc.new_from_defaults(&[8, 9, 10], 2));
        assert_eq!(doc, before);
        assert!(doc.new_from_defaults(&[8, 9, 10], 7));
        assert_eq!(doc.current(), [8, 9, 10]);
        assert_eq!(doc.baseline(), doc.current());
        assert_eq!(doc.target_revision(), 6);
        assert!(doc.path().is_empty());
        assert_eq!(doc.pending_name(), before.pending_name());
    }

    #[test]
    fn pending_keeps_live_content_and_resets_before_a_later_writer_failure() {
        let mut doc = Document::from_current(vec![1, 2], Vec::new(), 6);
        doc.begin_pending_name(7, b"FIRST".to_vec());
        let copied_payload_index = 7;
        doc.begin_pending_name(9, b"LATER".to_vec());
        assert!(!doc.pending_matches(copied_payload_index));
        assert!(doc.pending_matches(9));
        assert_eq!(doc.take_pending_nonempty().unwrap(), b"LATER");
        assert_eq!(doc.pending_name(), None);
        assert_eq!(doc.current(), [1, 2]);
        assert!(doc.patch_current(&[(usize::MAX, &[0])]).is_err());
        assert_eq!(doc.pending_name(), None);
        doc.begin_pending_name(9, Vec::new());
        assert_eq!(doc.take_pending_nonempty(), None);
        assert!(doc.pending_matches(9));
    }
}
