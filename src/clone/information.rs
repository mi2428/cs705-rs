//! O025..028 controllers with borrowed document receivers and the shared Session.
//! Native UI acquisition is external; events do not prove native delivery.

use super::{
    ContextFields, Error, Identity, IdentityGate, Outcome, Session, SessionConfig, Transport,
    search,
};
use crate::icf::document::{
    self,
    lifecycle::{RecentFiles, UiState},
};

/// Receivers borrow the real document and shared session, never a copied image.
pub struct DocumentReceiver<'a> {
    pub document: &'a mut document::Document,
    pub session: &'a mut document::Session,
    pub initial: &'a mut document::InitialState,
    pub ui: &'a mut UiState,
    pub recent: &'a RecentFiles,
}

/// Only actual resource assets and the reached native confirmation are inputs.
pub struct ReturnSource<'a> {
    pub defaults: Option<document::DefaultSource<'a>>,
    pub profile_return: Option<u32>,
    pub captions: Option<[Vec<u8>; 2]>,
    pub close_returns: &'a [NativeReturn],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Life {
    Absent,
    Open,
    ReleaseRequested,
    Destroyed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub name: &'static str,
    pub argument: i64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub kind: u8,
    pub key: u16,
    pub span: bool,
    pub selectable: bool,
    pub text: Vec<u8>,
}

/// Fresh construction and restoration of a captured state are separate callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Controller {
    pub com: i32,
    pub text: Option<Vec<u8>>,
    pub com_mode: Option<u8>,
    pub form: Life,
    pub popup: Life,
    pub retired_popups: Vec<Life>,
    pub information: Life,
    pub pair_registered: bool,
    pub selector: u8,
    pub fc: u8,
    pub fd: u8,
    pub identity: IdentityGate,
    pub identity_result: Option<u8>,
    pub captions: Option<[Vec<u8>; 2]>,
    pub dropdown: Vec<i32>,
    pub candidates: Vec<search::Candidate>,
    pub generation: u64,
    pub previous: u32,
    /// Current selection after programmatic preselection/user gestures/reads.
    pub selected: Option<usize>,
    pub last_item_index: Option<i32>,
    pub search_result: i32,
    pub reidentify_requested: bool,
    pub events: Vec<Event>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn captured(values: &[i32]) -> ReadMode {
        ReadMode::Captured(values.iter().copied().map(Ok).collect())
    }
    fn registry(values: &[Result<Vec<u8>, Error>]) -> RegistrySource<'_> {
        RegistrySource {
            values,
            crt_point: Some(b'.'),
        }
    }
    fn controller() -> Controller {
        let mut c = Controller::new(7, b"SEED".to_vec(), [6, 0, 1]);
        c.construct_com(&registry(&[]), &captured(&[7, 7])).unwrap();
        c.events.clear();
        c
    }
    fn filters() -> Vec<search::Filter> {
        vec![search::Filter::from_ini(b"*", b"*", b"*", b"A", b"A").unwrap()]
    }
    fn observed(key: u8, number: u32) -> search::Observation {
        search::Observation {
            key: vec![key],
            base: b"RAD".to_vec(),
            com: b"USB".to_vec(),
            hub: Vec::new(),
            audio: Vec::new(),
            parent_path: false,
            ports: vec![search::Port {
                instance_id: b"D".to_vec(),
                parent_id: b"P".to_vec(),
                prefix: b"USB".to_vec(),
                mi: 0,
                number,
                description: b"RAD_A".to_vec(),
            }],
            wave_in: -1,
            wave_out: 3,
        }
    }
    fn source<'a>(
        filters: &'a [search::Filter],
        observations: &'a [search::Observation],
        suffixes: &'a [Vec<u8>],
    ) -> SearchSource<'a> {
        SearchSource {
            ready: 1,
            opened: Some(true),
            filters,
            observations,
            prefix: b"RAD",
            suffixes,
        }
    }
    #[test]
    fn manual_commit_keyboard_and_lower_long_prefix_are_distinct_and_busy_keeps_store() {
        for (text, accepted, expected) in [
            (b"0".as_slice(), false, 7),
            (b"1", true, 1),
            (b"256", true, 256),
            (b"257", false, 7),
        ] {
            let mut c = controller();
            let result = c.manual_commit(text, &[false, false]);
            assert_eq!(result.is_ok(), accepted);
            assert_eq!(c.com, expected);
            assert_eq!(c.reidentify_requested, accepted);
            if accepted {
                assert_eq!((c.selector, c.fc, c.fd), (0, 1, 0));
                assert_eq!(
                    c.events
                        .iter()
                        .filter(|e| e.name == "sync")
                        .map(|e| e.argument)
                        .collect::<Vec<_>>(),
                    [0x28, 0x2f]
                );
            }
        }
        let mut c = controller();
        c.input_text(b"256.").unwrap();
        assert!(c.input_text(b"4294967297").is_err());
        assert_eq!(c.text.as_deref(), Some(b"256.".as_slice()));
        c.manual_commit(b"4294967297", &[true]).unwrap();
        assert_eq!(c.com, 1);
        assert_eq!((c.selector, c.fc, c.fd), (6, 0, 1));
        assert!(!c.reidentify_requested);
        c.manual_commit(b"256.9", &[false, true]).unwrap();
        assert_eq!(c.com, 256);
        assert_eq!((c.selector, c.fc, c.fd), (0, 1, 0));
        assert!(!c.reidentify_requested);
        assert!(c.manual_commit(b"12tail", &[]).is_err());
        assert_eq!(c.com, 12);
    }
    #[test]
    fn constructor_keeps_startup300_text_and_reads_global_twice_without_ui_clamping() {
        let mut c = Controller::new(300, b"UNCHANGED".to_vec(), [6, 0, 0]);
        c.construct_com(&registry(&[]), &captured(&[300])).unwrap();
        assert_eq!(c.com, 300);
        assert_eq!(c.text.as_deref(), Some(b"UNCHANGED".as_slice()));
        assert!(!c.events.iter().any(|e| e.name == "read_com_format"));
        assert_eq!(c.com_mode, Some(0));
        let mut c = Controller::new(1, b"RESOURCE".to_vec(), [6, 0, 0]);
        c.construct_com(&registry(&[]), &captured(&[1, 300]))
            .unwrap();
        assert_eq!(c.com, 300);
        assert_eq!(c.text.as_deref(), Some(b"300".as_slice()));
        assert_eq!(
            c.events
                .iter()
                .filter(|e| e.name.starts_with("read_com_"))
                .map(|e| e.argument)
                .collect::<Vec<_>>(),
            [1, 300]
        );
    }
    #[test]
    fn registry_uses_shared_b0cf8_before_sort_and_preserves_unsorted_failure_prefix() {
        let values = [
            Ok(b"COM12".to_vec()),
            Ok(b"1e2".to_vec()),
            Ok(b"COM-1.5".to_vec()),
            Ok(b"1234567890".to_vec()),
            Ok(Vec::new()),
        ];
        let mut c = controller();
        c.refresh(&registry(&values)).unwrap();
        assert_eq!(c.dropdown, [-2, 0, 12, 12, 123456789]);
        let values = [Ok(b"COM1.5".to_vec()), Ok(b"COM1,5".to_vec())];
        c.refresh(&RegistrySource {
            values: &values,
            crt_point: Some(b','),
        })
        .unwrap();
        assert_eq!(c.dropdown, [1, 15]);
        let values = [
            Ok(b"COM300".to_vec()),
            Ok(b"COM2".to_vec()),
            Err(Error::Invalid("captured native exception")),
        ];
        c.events.clear();
        assert!(c.refresh(&registry(&values)).is_err());
        assert_eq!(c.dropdown, [300, 2]);
        assert_eq!(c.text, None);
        assert!(!c.events.iter().any(|e| e.name == "dropdown_sort"));
    }
    #[test]
    fn actual_search_startup_and_rescan_use_new_observations_and_saved_previous_com() {
        let filters = filters();
        let suffixes = vec![b"A".to_vec(), b"#".to_vec()];
        let observations = [observed(b'a', 300), observed(b'b', 12)];
        let mut c = Controller::new(0, b"RESOURCE".to_vec(), [6, 0, 0]);
        c.startup(Some(&source(&filters, &observations, &suffixes)))
            .unwrap();
        assert_eq!(c.com, 300);
        assert_eq!(c.generation, 1);
        assert_eq!(c.candidates[0].label, b"RAD A  COM=300");
        c.construct_com(&registry(&[]), &captured(&[300])).unwrap();
        assert_eq!(c.text.as_deref(), Some(b"RESOURCE".as_slice()));
        c.open_search(&source(&filters, &observations, &suffixes))
            .unwrap();
        c.com = 999;
        let next = [observed(b'a', 17), observed(b'b', 300)];
        c.rescan(&source(&filters, &next, &suffixes)).unwrap();
        assert_eq!((c.generation, c.previous, c.selected), (3, 300, Some(1)));
        assert_eq!(
            c.candidates.iter().map(|v| v.number).collect::<Vec<_>>(),
            [17, 300]
        );
        let stopped = SearchSource {
            ready: 0,
            opened: None,
            filters: &[],
            observations: &[],
            prefix: b"",
            suffixes: &[],
        };
        c.rescan(&stopped).unwrap();
        assert!(c.candidates.is_empty());
        assert_eq!(c.selected, None);
        assert_eq!(c.com, 999);
        c.startup(None).unwrap();
        assert_eq!(c.com, 999);
    }
    #[test]
    fn two_live_indices_and_interactive_positive_text_are_not_com_commit() {
        let f = filters();
        let suffixes = vec![b"A".to_vec()];
        let observations = [observed(b'a', 300), observed(b'b', 12)];
        let mut c = controller();
        c.open_search(&source(&f, &observations, &suffixes))
            .unwrap();
        c.select(&captured(&[0, 1])).unwrap();
        assert_eq!(c.search_result, 12);
        assert_eq!(c.popup, Life::ReleaseRequested);
        c.return_search(&registry(&[])).unwrap();
        assert_eq!(c.text.as_deref(), Some(b"12".as_slice()));
        assert_eq!(c.com, 7);
        c.open_search(&source(&f, &observations, &suffixes))
            .unwrap();
        assert_eq!(c.retired_popups, [Life::ReleaseRequested]);
        assert_eq!(c.popup, Life::Open);
        c.select(&captured(&[-1])).unwrap();
        c.return_search(&registry(&[])).unwrap();
        assert_eq!(c.search_result, -1);
        assert_eq!(c.com, 7);
        c.open_search(&source(&f, &observations, &suffixes))
            .unwrap();
        assert!(c.select(&captured(&[0, 9])).is_err());
        assert_eq!(c.search_result, -1);
        assert_eq!(c.popup, Life::Open);
        c.escape().unwrap();
        assert_eq!(c.popup, Life::ReleaseRequested);
        assert_eq!(
            c.retired_popups,
            [Life::ReleaseRequested, Life::ReleaseRequested]
        );
        c.destroy_retired_popup(0).unwrap();
        assert_eq!(c.retired_popups[0], Life::Destroyed);
        assert_eq!(c.popup, Life::ReleaseRequested);
        assert_eq!(c.com, 7);
    }
    #[test]
    fn cancel_close_false_returns_order_pair_release_and_late_exception_are_distinct() {
        let mut c = controller();
        c.close_com(true, &[NativeReturn::Returned(0); 5]).unwrap();
        assert_eq!(c.com, 7);
        assert!(!c.pair_registered);
        assert_eq!(c.form, Life::ReleaseRequested);
        assert_eq!(
            c.events
                .iter()
                .filter(|e| e.name != "native_return_ignored")
                .map(|e| (e.name, e.argument))
                .collect::<Vec<_>>(),
            [
                ("cancel_close_request", 0),
                ("screen_484c8", 0),
                ("screen_486c0", 0),
                ("sync", 0x29),
                ("sync", 2),
                ("detach_form_pair", 0x4a5ec),
                ("com_release_request", 0)
            ]
        );
        c.destroy("com").unwrap();
        assert_eq!(c.form, Life::Destroyed);
        let mut c = controller();
        assert!(
            c.close_com(
                true,
                &[
                    NativeReturn::Returned(0),
                    NativeReturn::Returned(0),
                    NativeReturn::Returned(0),
                    NativeReturn::Exception
                ]
            )
            .is_err()
        );
        assert!(c.pair_registered);
        assert_eq!(c.form, Life::Open);
        assert!(!c.events.iter().any(|e| e.name == "com_release_request"));
    }
    fn identity(comment: &[u8], selector: u8) -> Identity {
        let mut tail = vec![0; 33];
        tail[0] = 33;
        tail[1] = selector;
        tail[2..18].fill(b' ');
        tail[2..2 + comment.len()].copy_from_slice(comment);
        Identity {
            model: 0x4075,
            revision: 1,
            tail,
        }
    }
    fn info_source() -> InformationSource {
        InformationSource {
            gate_context: Some(ContextFields([0; 12])),
            gate_flag: 0,
            display_context: Some(ContextFields([0; 12])),
            display_flag: 1,
            operation: 1,
            captions: Some([b"HEADER".to_vec(), b"LABEL".to_vec()]),
        }
    }
    #[test]
    fn information_uses_live_tail_two_by_two_gate_prefix_and_release_not_destroy() {
        let mut c = Controller::new(0, Vec::new(), [6, 0, 0]);
        c.identify(&identity(b"FIRST GENERATION", 6), info_source())
            .unwrap();
        let rows = c.information_rows().unwrap().unwrap();
        assert_eq!(rows[1][1].text, b"FIRST GENERATION");
        assert_eq!((rows[1][1].kind, rows[1][1].key), (0x19, 0xb0));
        assert!(rows[0][1].span);
        assert!(!rows[1][1].selectable);
        assert!(!c.information_selection(0, 0, true));
        assert!(c.information_selection(1, 0, true));
        assert!(c.information_selection(0, 1, true));
        let before = c.identity.cached_tail;
        let mut s = info_source();
        s.gate_flag = 1;
        c.identify(&identity(b"SECOND", 0), s).unwrap();
        assert_eq!(c.identity_result, Some(13));
        assert_ne!(c.identity.cached_tail, before);
        assert_eq!(
            c.information_rows().unwrap().unwrap()[1][1].text,
            b"SECOND          "
        );
        assert_eq!(
            c.events
                .iter()
                .filter(|e| e.name == "create_information_2_by_2")
                .count(),
            1
        );
        assert_eq!(
            c.events
                .iter()
                .filter(|e| e.name == "compare_information_context")
                .count(),
            2
        );
        c.information_close(true).unwrap();
        assert_eq!(c.information, Life::ReleaseRequested);
        assert!(
            !c.events
                .iter()
                .any(|e| e.name == "destroy_information_unit")
        );
        c.destroy("information").unwrap();
        assert_eq!(c.information, Life::Destroyed);
        assert!(c.information_rows().unwrap().is_none());
    }
    #[test]
    fn missing_identity_source_keeps_early_tail_and_never_invents_context_or_captions() {
        let mut c = Controller::new(0, Vec::new(), [6, 1, 0]);
        let mut s = info_source();
        s.display_context = None;
        assert!(c.identify(&identity(b"AB\0TRAIL", 6), s).is_err());
        assert_eq!(c.identity.cached_tail.unwrap()[2..10], *b"AB\0TRAIL");
        assert_eq!(c.information, Life::Absent);
        c.identify(&identity(b"AB\0TRAIL", 6), info_source())
            .unwrap();
        assert_eq!(c.information_rows().unwrap().unwrap()[1][1].text, b"AB");
        let mut id = identity(b"NOT COPIED", 6);
        id.revision = 2;
        c.identify(&id, info_source()).unwrap();
        assert_eq!(c.identity_result, Some(14));
        assert_eq!(c.information_rows().unwrap().unwrap()[1][1].text, b"AB");
    }
    #[test]
    fn producer_current_reads_owned_state_each_time() {
        let f = filters();
        let suffixes = vec![b"A".to_vec()];
        let observations = [observed(b'a', 300), observed(b'b', 12)];
        let mut c = Controller::new(0, b"RESOURCE".to_vec(), [6, 0, 1]);
        c.startup(Some(&source(&f, &observations, &suffixes)))
            .unwrap();
        c.construct_com(
            &RegistrySource {
                values: &[],
                crt_point: None,
            },
            &ReadMode::Current,
        )
        .unwrap();
        assert_eq!(c.com, 300);
        assert_eq!(c.text.as_deref(), Some(b"RESOURCE".as_slice()));
        let mut c = Controller::new(1, Vec::new(), [6, 0, 1]);
        c.construct_com(&registry(&[]), &ReadMode::Current).unwrap();
        assert_eq!(
            c.events
                .iter()
                .filter(|e| e.name.starts_with("read_com_"))
                .map(|e| e.argument)
                .collect::<Vec<_>>(),
            [1, 1]
        );
        assert_eq!(c.com_value(&ReadMode::Current, 0).unwrap(), 1);
        c.com = 300;
        assert_eq!(c.com_value(&ReadMode::Current, 1).unwrap(), 300);
        c.open_search(&source(&f, &observations, &suffixes))
            .unwrap();
        c.choose(0).unwrap();
        assert_eq!(c.item_index(&ReadMode::Current, 0).unwrap(), 0);
        c.choose(1).unwrap();
        assert_eq!(c.item_index(&ReadMode::Current, 1).unwrap(), 1);
        c.select(&ReadMode::Current).unwrap();
        assert_eq!(c.search_result, 12);
        c.input_text(b"256.").unwrap();
        c.manual_commit_current(&[true]).unwrap();
        assert_eq!(c.com, 256);
    }
    #[test]
    fn producer_captured_errors_never_fall_back_to_owned_state() {
        let f = filters();
        let suffixes = vec![b"A".to_vec()];
        let observations = [observed(b'a', 300), observed(b'b', 12)];
        let mut c = Controller::new(1, Vec::new(), [6, 0, 1]);
        assert!(c.construct_com(&registry(&[]), &captured(&[])).is_err());
        assert!(!c.pair_registered);
        let mut c = controller();
        c.open_search(&source(&f, &observations, &suffixes))
            .unwrap();
        assert!(
            c.select(&ReadMode::Captured(vec![
                Ok(0),
                Err(Error::Invalid("captured getter error"))
            ]))
            .is_err()
        );
        assert_eq!(c.search_result, -1);
        assert_eq!(c.popup, Life::Open);
    }
    #[test]
    fn document_close_runs_receivers_and_keeps_same_pending_image_and_failure_prefix() {
        use crate::icf::{
            context::Context,
            document::{
                Document, InitialState, Session,
                lifecycle::{MenuDescriptor, MenuItem, UiState},
            },
        };
        let mut c = controller();
        let mut document = Document::from_current(vec![7; 16], b"KEEP".to_vec(), 6);
        document.begin_pending_name(4, b"PENDING".to_vec());
        let mut session = Session::new(Context::before_registry(), None, 1, [1, 0]);
        let mut initial = InitialState::default();
        let mut ui = UiState::headless();
        let recent = RecentFiles::default();
        ui.form.as_mut().unwrap().visible57 = 0;
        ui.owner_enabled(0).unwrap();
        ui.menu.push(MenuItem {
            descriptor: None,
            query: None,
            property19a: true,
            property1ae: true,
            property1a1: true,
            enabled: true,
            shortcut: 7,
            caption: Vec::new(),
        });
        let mut receiver = DocumentReceiver {
            document: &mut document,
            session: &mut session,
            initial: &mut initial,
            ui: &mut ui,
            recent: &recent,
        };
        c.close_com_document(true, &[NativeReturn::Returned(0); 3], &mut receiver)
            .unwrap();
        assert_eq!(receiver.ui.application5b, 1);
        assert_eq!(
            (
                receiver.ui.form.as_ref().unwrap().visible57,
                receiver.ui.form.as_ref().unwrap().enabled58
            ),
            (1, 1)
        );
        assert!(receiver.ui.form.as_ref().unwrap().focused);
        assert!(!receiver.ui.menu[0].property19a);
        assert!(!receiver.ui.menu[0].property1a1);
        assert!(receiver.ui.menu[0].enabled);
        assert_eq!(receiver.document.current(), &[7; 16]);
        assert_eq!(receiver.document.path(), b"KEEP");
        assert_eq!(receiver.document.pending_name().unwrap().name, b"PENDING");
        assert!(!c.pair_registered);
        assert_eq!(c.form, Life::ReleaseRequested);
        let mut c = controller();
        receiver.ui.menu[0].descriptor = Some(MenuDescriptor {
            group: 0x19,
            key: 0,
            index: 0,
            name: b"CL_Null".to_vec(),
            caption: Vec::new(),
            flags1d: 0,
            flags1e: 0,
            shortcut18: 0,
        });
        assert!(
            c.close_com_document(false, &[NativeReturn::Returned(0); 3], &mut receiver)
                .is_err()
        );
        assert!(c.pair_registered);
        assert_eq!(c.form, Life::Open);
        assert!(!c.events.iter().any(|e| e.name == "com_release_request"));
    }
}

/// Source observations, not a completed candidate/popup packet. ASCII matching
/// and all candidate/label calculations remain in the existing search module.
pub struct SearchSource<'a> {
    pub ready: u8,
    pub opened: Option<bool>,
    pub filters: &'a [search::Filter],
    pub observations: &'a [search::Observation],
    pub prefix: &'a [u8],
    pub suffixes: &'a [Vec<u8>],
}

/// Each value is an actual registry read or a captured exception at that read.
pub struct RegistrySource<'a> {
    pub values: &'a [Result<Vec<u8>, Error>],
    pub crt_point: Option<u8>,
}

pub struct InformationSource {
    pub gate_context: Option<ContextFields>,
    pub gate_flag: u8,
    pub display_context: Option<ContextFields>,
    pub display_flag: u8,
    pub operation: u32,
    pub captions: Option<[Vec<u8>; 2]>,
}

/// A native call's false return is distinct from failure to return normally.
#[derive(Debug, Clone, Copy)]
pub enum NativeReturn {
    Returned(i64),
    Exception,
}

/// Current is the ordinary owned-state producer. Captured is an explicit
/// external getter trace: missing/invalid entries never fall back to Current.
#[derive(Debug, Clone)]
pub enum ReadMode {
    Current,
    Captured(Vec<Result<i32, Error>>),
}

impl Controller {
    pub fn new(com: i32, resource_text: Vec<u8>, document: [u8; 3]) -> Self {
        Self {
            com,
            text: Some(resource_text),
            com_mode: None,
            form: Life::Absent,
            popup: Life::Absent,
            retired_popups: Vec::new(),
            information: Life::Absent,
            pair_registered: false,
            selector: document[0],
            fc: document[1],
            fd: document[2],
            identity: IdentityGate::default(),
            identity_result: None,
            captions: None,
            dropdown: Vec::new(),
            candidates: Vec::new(),
            generation: 0,
            previous: 0,
            selected: None,
            last_item_index: None,
            search_result: -1,
            reidentify_requested: false,
            events: Vec::new(),
        }
    }
    fn event(&mut self, name: &'static str, argument: i64) {
        self.events.push(Event {
            name,
            argument,
            bytes: Vec::new(),
        });
    }
    fn set_text(&mut self, text: Vec<u8>) {
        self.events.push(Event {
            name: "set_text",
            argument: 0,
            bytes: text.clone(),
        });
        self.text = Some(text);
    }
    fn com_value(&self, mode: &ReadMode, position: usize) -> Result<i32, Error> {
        match mode {
            ReadMode::Current => Ok(self.com),
            ReadMode::Captured(values) => values
                .get(position)
                .ok_or(Error::Invalid("captured COM read unavailable"))?
                .clone(),
        }
    }
    fn item_index(&mut self, mode: &ReadMode, position: usize) -> Result<i32, Error> {
        let value = match mode {
            ReadMode::Current => self.selected.map_or(-1, |index| index as i32),
            ReadMode::Captured(values) => values
                .get(position)
                .ok_or(Error::Invalid("captured ItemIndex read unavailable"))?
                .clone()?,
        };
        self.last_item_index = Some(value);
        self.selected = if value < 0 {
            None
        } else {
            Some(value as usize)
        };
        Ok(value)
    }
    pub fn choose(&mut self, index: i32) -> Result<(), Error> {
        if self.popup != Life::Open
            || index < -1
            || (index >= 0 && index as usize >= self.candidates.len())
        {
            return Err(Error::Invalid(
                "selection gesture outside current popup generation",
            ));
        }
        self.selected = if index < 0 {
            None
        } else {
            Some(index as usize)
        };
        self.event("selection_gesture", i64::from(index));
        Ok(())
    }
    /// MaxLength applies to normal keyboard input, not programmatic text setters
    /// or the lower native getter/parser, which may contain a longer prefix.
    pub fn input_text(&mut self, text: &[u8]) -> Result<(), Error> {
        if self.form != Life::Open || text.len() > 4 {
            return Err(Error::Invalid(
                "COM input requires open form and at most four bytes",
            ));
        }
        self.set_text(text.to_vec());
        Ok(())
    }
    pub fn refresh(&mut self, source: &RegistrySource<'_>) -> Result<(), Error> {
        let saved = self
            .text
            .clone()
            .ok_or(Error::Invalid("actual COM text source unavailable"))?;
        self.event("save_text", 0);
        self.dropdown.clear();
        self.text = None;
        self.event("dropdown_clear", 0);
        for value in source.values {
            self.event("registry_value_read", self.dropdown.len() as i64);
            let value = value.as_ref().map_err(Clone::clone)?;
            // B0CF8's normal C/CW domain: filter then nine bytes implies abs<1e9.
            // The existing scale 0/step 1 primitive's i32 bounds never clamp it.
            let number = crate::icf::settings::extended::numeric_text(
                value,
                source
                    .crt_point
                    .ok_or(Error::Invalid("COM registry numeric source unavailable"))?,
                0,
                i64::from(i32::MIN),
                i64::from(i32::MAX),
                1,
            )
            .map_err(|_| Error::Invalid("COM registry numeric source unavailable"))?
            .0 as i32;
            self.dropdown.push(number);
            self.events.push(Event {
                name: "dropdown_add",
                argument: i64::from(number),
                bytes: number.to_string().into_bytes(),
            });
        }
        self.dropdown.sort();
        self.event("dropdown_sort", self.dropdown.len() as i64);
        self.set_text(saved);
        Ok(())
    }
    pub fn construct_com(
        &mut self,
        source: &RegistrySource<'_>,
        reads: &ReadMode,
    ) -> Result<(), Error> {
        if self.form == Life::Open {
            return Err(Error::Invalid("COM form already open"));
        }
        self.form = Life::Open;
        self.event("create_com_maxlength", 4);
        self.refresh(source)?;
        let first = self.com_value(reads, 0)?;
        self.com = first;
        self.event("read_com_guard", i64::from(first));
        if (1..=256).contains(&first) {
            let second = self.com_value(reads, 1)?;
            self.com = second;
            self.event("read_com_format", i64::from(second));
            self.set_text(second.to_string().into_bytes());
        }
        self.com_mode = Some(0);
        self.event("set_com_mode", 0);
        self.pair_registered = true;
        self.event("register_form_pair", 0x4a5ec);
        Ok(())
    }
    fn generate_candidates(&mut self, source: &SearchSource<'_>) -> Result<(), Error> {
        self.candidates.clear();
        self.selected = None;
        self.last_item_index = None;
        self.event("popup_combo_clear", 0);
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(Error::Invalid("search generation overflow"))?;
        self.event("search_generation", self.generation as i64);
        if source.ready != 1 {
            return Ok(());
        }
        self.event("open_dll", 0);
        if !source
            .opened
            .ok_or(Error::Invalid("OpenDll observation unavailable"))?
        {
            return Ok(());
        }
        let mut context = search::SearchContext::default();
        self.event("search_observations", source.observations.len() as i64);
        context.search(source.filters, source.observations)?;
        let (candidates, selected) = search::candidates(
            &context.results,
            source.prefix,
            source.suffixes,
            self.previous,
        )?;
        self.candidates = candidates;
        self.selected = selected;
        self.event("close_dll", 0);
        self.event("set_popup_candidates", self.candidates.len() as i64);
        if let Some(index) = selected {
            self.event("set_popup_index", index as i64);
        }
        Ok(())
    }
    pub fn rescan(&mut self, source: &SearchSource<'_>) -> Result<(), Error> {
        if self.popup != Life::Open {
            return Err(Error::Invalid("search popup not open"));
        }
        self.generate_candidates(source)
    }
    fn label(&self, index: i32) -> Result<i32, Error> {
        if index < 0 {
            return Ok(-1);
        }
        let value = self
            .candidates
            .get(index as usize)
            .ok_or(Error::Invalid("live candidate index outside generation"))?;
        search::label_number(&value.label)
    }
    pub fn startup(&mut self, source: Option<&SearchSource<'_>>) -> Result<(), Error> {
        if self.com != 0 {
            return Ok(());
        }
        self.previous = self.com as u32;
        self.generate_candidates(
            source.ok_or(Error::Invalid("startup search source unavailable"))?,
        )?;
        let number = self.label(self.selected.map_or(-1, |i| i as i32))?;
        if number != -1 {
            self.com = number;
            self.event("startup_store_com", i64::from(number));
        }
        Ok(())
    }
    pub fn open_search(&mut self, source: &SearchSource<'_>) -> Result<(), Error> {
        // 20438 constructs a NEW form on every call. Retiring a reference is
        // neither delivery of its pending Release nor destruction of that form.
        if self.popup != Life::Absent {
            self.retired_popups.push(self.popup);
            self.event(
                "retain_previous_popup_lifetime",
                (self.retired_popups.len() - 1) as i64,
            );
        }
        self.popup = Life::Open;
        self.search_result = -1;
        self.previous = self.com as u32;
        self.event("create_search_result", -1);
        self.event("document_windows_enable_request", 0x28230);
        self.generate_candidates(source)
    }
    pub fn select(&mut self, reads: &ReadMode) -> Result<(), Error> {
        if self.popup != Life::Open {
            return Err(Error::Invalid("search popup not open"));
        }
        self.search_result = -1;
        self.event("reset_search_result", -1);
        let first = self.item_index(reads, 0)?;
        self.event("read_index_guard", i64::from(first));
        if first >= 0 {
            let second = self.item_index(reads, 1)?;
            self.event("read_index_label", i64::from(second));
            self.search_result = self.label(second)?;
            self.event("store_search_result", i64::from(self.search_result));
        }
        self.popup_close()
    }
    pub fn popup_close(&mut self) -> Result<(), Error> {
        if !matches!(self.popup, Life::Open | Life::ReleaseRequested) {
            return Err(Error::Invalid("search popup not alive"));
        }
        self.event("popup_close", 0);
        self.event("popup_release_request", 0);
        self.popup = Life::ReleaseRequested;
        Ok(())
    }
    pub fn escape(&mut self) -> Result<(), Error> {
        self.event("escape_close_request", 0);
        self.popup_close()
    }
    pub fn return_search(&mut self, registry: &RegistrySource<'_>) -> Result<(), Error> {
        if self.form != Life::Open {
            return Err(Error::Invalid("COM form not open"));
        }
        self.refresh(registry)?;
        if self.search_result > 0 {
            self.set_text(self.search_result.to_string().into_bytes());
        }
        Ok(())
    }
    pub fn manual_commit_current(&mut self, busy_reads: &[bool]) -> Result<(), Error> {
        let text = self
            .text
            .clone()
            .ok_or(Error::Invalid("current COM text source unavailable"))?;
        self.manual_commit(&text, busy_reads)
    }
    /// Explicit captured getter entry. It may contain more than keyboard 4 bytes.
    pub fn manual_commit(&mut self, getter_text: &[u8], busy_reads: &[bool]) -> Result<(), Error> {
        self.commit_body(getter_text, busy_reads, None)
    }
    /// The ordinary synchronous receiver reads the same owned text by default.
    pub fn manual_commit_document(
        &mut self,
        getter_text: Option<&[u8]>,
        busy_reads: &[bool],
        receiver: &mut DocumentReceiver<'_>,
    ) -> Result<(), Error> {
        let text = getter_text
            .map(ToOwned::to_owned)
            .or_else(|| self.text.clone())
            .ok_or(Error::Invalid("current COM text source unavailable"))?;
        self.commit_body(&text, busy_reads, Some(receiver))
    }
    fn commit_body(
        &mut self,
        getter_text: &[u8],
        busy_reads: &[bool],
        receiver: Option<&mut DocumentReceiver<'_>>,
    ) -> Result<(), Error> {
        if self.form != Life::Open || self.com_mode != Some(0) {
            return Err(Error::Invalid("COM-mode form not open"));
        }
        self.events.push(Event {
            name: "get_com_text",
            argument: 0,
            bytes: getter_text.to_vec(),
        });
        self.text = Some(getter_text.to_vec());
        let number = search::manual_commit(getter_text)?;
        self.com = i32::from(number);
        self.event("store_com", i64::from(number));
        self.event("sync", 0x28);
        self.prepare_reidentify(busy_reads, receiver)
    }
    pub fn request_reidentify_document(
        &mut self,
        busy_reads: &[bool],
        receiver: &mut DocumentReceiver<'_>,
    ) -> Result<(), Error> {
        self.prepare_reidentify(busy_reads, Some(receiver))
    }
    fn prepare_reidentify(
        &mut self,
        busy_reads: &[bool],
        mut receiver: Option<&mut DocumentReceiver<'_>>,
    ) -> Result<(), Error> {
        let first = *busy_reads
            .first()
            .ok_or(Error::Invalid("document busy read unavailable"))?;
        self.event("read_document_busy", i64::from(first));
        if let Some(receiver) = receiver.as_mut() {
            receiver.ui.sync28(receiver.document, first);
        }
        if first {
            return Ok(());
        }
        self.selector = 0;
        self.fc = 1;
        self.fd = 0;
        self.event("prepare_reidentify", 0);
        let second = *busy_reads
            .get(1)
            .ok_or(Error::Invalid("wrapper busy read unavailable"))?;
        self.event("read_wrapper_busy", i64::from(second));
        if second {
            return Ok(());
        }
        self.event("sync", 0x2f);
        if let Some(receiver) = receiver {
            receiver
                .ui
                .sync2f(receiver.document)
                .map_err(|_| Error::Invalid("main form is absent"))?;
        }
        self.reidentify_requested = true;
        self.event("reidentify_request", i64::from(self.com));
        Ok(())
    }
    /// Normal returned values, including false, do not approve or cancel Close.
    /// Exceptions/missing captures stop at the exact call, retaining its prefix.
    pub fn close_com(&mut self, cancel: bool, returns: &[NativeReturn]) -> Result<(), Error> {
        self.close_body(cancel, returns, None)
    }
    /// Only the two screen calls and Release are native return captures here.
    /// Synchronous document receivers run as software, not captured approvals.
    pub fn close_com_document(
        &mut self,
        cancel: bool,
        returns: &[NativeReturn],
        receiver: &mut DocumentReceiver<'_>,
    ) -> Result<(), Error> {
        self.close_body(cancel, returns, Some(receiver))
    }
    fn close_body(
        &mut self,
        cancel: bool,
        returns: &[NativeReturn],
        mut receiver: Option<&mut DocumentReceiver<'_>>,
    ) -> Result<(), Error> {
        if self.form != Life::Open {
            return Err(Error::Invalid("COM form not open"));
        }
        if cancel {
            self.event("cancel_close_request", 0);
        }
        for (index, (name, argument)) in [
            ("screen_484c8", 0),
            ("screen_486c0", 0),
            ("sync", 0x29),
            ("sync", 2),
            ("com_release_request", 0),
        ]
        .into_iter()
        .enumerate()
        {
            if index == 4 {
                self.pair_registered = false;
                self.event("detach_form_pair", 0x4a5ec);
                self.form = Life::ReleaseRequested;
            }
            self.event(name, argument);
            if let Some(receiver) = receiver.as_mut() {
                match index {
                    2 => {
                        receiver
                            .ui
                            .sync29(receiver.document)
                            .map_err(|_| Error::Invalid("COM return receiver failed"))?;
                        continue;
                    }
                    3 => {
                        receiver
                            .ui
                            .sync02(receiver.document, receiver.session, receiver.recent)
                            .map_err(|_| Error::Invalid("COM menu receiver failed"))?;
                        continue;
                    }
                    _ => {}
                }
            }
            let native_index = if receiver.is_some() && index == 4 {
                2
            } else {
                index
            };
            match returns.get(native_index) {
                Some(NativeReturn::Returned(value)) => self.event("native_return_ignored", *value),
                Some(NativeReturn::Exception) => {
                    return Err(Error::Invalid("native Close call raised exception"));
                }
                None => return Err(Error::Invalid("native Close return source unavailable")),
            }
        }
        Ok(())
    }
    pub fn identify(
        &mut self,
        identity: &Identity,
        source: InformationSource,
    ) -> Result<(), Error> {
        let gate_context = if self.fc != 0 {
            None
        } else {
            source.gate_context
        };
        let result =
            self.identity
                .apply(identity, gate_context, self.selector, source.gate_flag)?;
        self.identity_result = Some(result);
        self.event("identity_result", i64::from(result));
        if self.fc == 0
            && gate_context.is_none()
            && identity.model == 0x4075
            && identity.revision == 1
            && identity.tail.len() >= 33
            && identity.tail.len() == usize::from(identity.tail[0])
        {
            return Err(Error::Invalid(
                "actual saved/live gate context source unavailable",
            ));
        }
        if result == 0 && source.operation == 1 {
            self.event("sync", 0x2a);
        }
        self.finish_information(result, source, None, &[])
    }
    /// Consume the normal COM request with the existing wire engine. Its gate
    /// already ran; retain its early stores without applying that gate twice.
    pub fn query_requested(
        &mut self,
        mut config: SessionConfig,
        transport: &mut impl Transport,
        cancelled: impl FnMut() -> bool,
    ) -> Result<Outcome, Error> {
        if !self.reidentify_requested {
            return Err(Error::Invalid("no pending reidentify request"));
        }
        config.saved_selector = self.selector;
        if self.fc != 0 {
            config.expected_context = None;
        } else if config.expected_context.is_none() {
            return Err(Error::Invalid("live query gate context unavailable"));
        }
        let query = Session::query(config)?;
        self.reidentify_requested = false;
        let outcome = query.run(transport, cancelled);
        if outcome.identity_gate.cached_model.is_some() {
            self.identity.cached_model = outcome.identity_gate.cached_model;
        }
        if outcome.identity_gate.cached_tail.is_some() {
            self.identity.cached_tail = outcome.identity_gate.cached_tail;
        }
        self.identity_result = Some(outcome.result_code());
        self.event("identity_result", i64::from(outcome.result_code()));
        Ok(outcome)
    }
    pub fn query_document(
        &mut self,
        mut config: SessionConfig,
        transport: &mut impl Transport,
        cancelled: impl FnMut() -> bool,
        receiver: &mut DocumentReceiver<'_>,
    ) -> Result<Outcome, document::CloneError> {
        config.expected_context = Some(receiver.session.context.fields());
        config.flag57 = receiver
            .ui
            .form
            .as_ref()
            .ok_or(Error::Invalid("main form is absent"))?
            .visible57;
        let outcome = self.query_requested(config, transport, cancelled)?;
        self.event("sync", 0x30);
        if receiver
            .ui
            .form
            .as_ref()
            .ok_or(Error::Invalid("main form is absent"))?
            .visible57
            == 1
        {
            receiver.ui.owner_destroyed()?;
        }
        Ok(outcome)
    }
    /// Query completion uses live Q after its synchronous initial-adoption
    /// receiver. No identity gate, Q/G recipe or UI receiver is reimplemented.
    pub fn return_query(
        &mut self,
        outcome: &Outcome,
        receiver: &mut DocumentReceiver<'_>,
        source: ReturnSource<'_>,
    ) -> Result<(), document::CloneError> {
        if outcome.operation != super::Operation::Query {
            return Err(Error::Invalid("information return requires query outcome").into());
        }
        if outcome.failure.is_some() {
            return Ok(());
        }
        let identity = outcome
            .identity
            .as_ref()
            .ok_or(Error::Invalid("successful query has no identity"))?;
        self.event("sync", 0x2a);
        if let Some(request) = document::InitialRequest::information(receiver.session, identity, 0)?
        {
            let visible = receiver
                .ui
                .form
                .as_ref()
                .ok_or(Error::Invalid("main form is absent"))?
                .visible57;
            let accepted = if visible != 0 {
                source
                    .profile_return
                    .ok_or(Error::Invalid("profile confirmation source unavailable"))?
                    as u8
                    == 1
            } else {
                true
            };
            if accepted {
                let ui = &mut receiver.ui;
                receiver.document.receive_initial_identity(
                    receiver.session,
                    receiver.initial,
                    identity,
                    request,
                    source.defaults,
                    |boundary, document, session| ui.initial_boundary(boundary, document, session),
                )?;
            }
        }
        let visible = receiver
            .ui
            .form
            .as_ref()
            .ok_or(Error::Invalid("main form is absent"))?
            .visible57;
        self.finish_information(
            0,
            InformationSource {
                gate_context: None,
                gate_flag: 0,
                display_context: Some(receiver.session.context.fields()),
                display_flag: visible,
                operation: 1,
                captions: source.captions,
            },
            Some(receiver),
            source.close_returns,
        )?;
        Ok(())
    }
    fn finish_information(
        &mut self,
        result: u8,
        source: InformationSource,
        mut receiver: Option<&mut DocumentReceiver<'_>>,
        close_returns: &[NativeReturn],
    ) -> Result<(), Error> {
        if source.display_flag == 1 {
            self.event("compare_information_context", 0);
            let tail = self
                .identity
                .cached_tail
                .as_ref()
                .ok_or(Error::Invalid("live information identity tail unavailable"))?;
            let display = source
                .display_context
                .ok_or(Error::Invalid("actual live display Q source unavailable"))?;
            let comparison = if ContextFields::from_tail(tail)? == display {
                0
            } else {
                2
            };
            self.event("information_comparison", comparison);
            if result == 0 && comparison == 0 {
                self.captions = Some(source.captions.ok_or(Error::Invalid(
                    "actual information resource captions unavailable",
                ))?);
                self.information = Life::Open;
                self.event("create_information_2_by_2", 0x19);
                self.event("information_show_request", 0);
                self.event("owner_enable_request", 0);
                if let Some(receiver) = receiver.as_mut() {
                    receiver
                        .ui
                        .owner_enabled(0)
                        .map_err(|_| Error::Invalid("main form is absent"))?;
                }
                self.event("document_views_request", 0x24528);
                if let Some(receiver) = receiver.as_mut() {
                    receiver.ui.destroy_view(receiver.document);
                }
                self.event("sync", 0x25);
                if let Some(receiver) = receiver {
                    receiver
                        .ui
                        .sync25(receiver.document, receiver.session)
                        .map_err(|_| Error::Invalid("information tree receiver failed"))?;
                }
            }
        } else if result == 0 && source.operation == 1 {
            self.event("sync", 0x49);
            if let Some(receiver) = receiver {
                self.sync49_document(receiver, close_returns)?;
            }
        }
        Ok(())
    }
    /// Dispatch only the actual live Form2 pair. Other bus registrations are
    /// outside this finite receiver domain, not assumed to be absent globally.
    pub fn sync49_document(
        &mut self,
        receiver: &mut DocumentReceiver<'_>,
        close_returns: &[NativeReturn],
    ) -> Result<bool, Error> {
        if !self.pair_registered {
            return Ok(false);
        }
        if self.form != Life::Open {
            return Err(Error::Invalid("registered COM form is not open"));
        }
        let ready = receiver.ui.form2_profile(receiver.session);
        self.event("profile_ready", i64::from(ready));
        if ready {
            self.event("sync", 0x2b);
            receiver
                .ui
                .sync2b(receiver.document, receiver.session)
                .map_err(|_| Error::Invalid("profile-ready document receiver failed"))?;
        }
        self.close_com_document(false, close_returns, receiver)?;
        Ok(true)
    }
    pub fn information_rows(&self) -> Result<Option<[[Cell; 2]; 2]>, Error> {
        if !matches!(self.information, Life::Open | Life::ReleaseRequested) {
            return Ok(None);
        }
        let captions = self
            .captions
            .as_ref()
            .ok_or(Error::Invalid("information captions unavailable"))?;
        let tail = self
            .identity
            .cached_tail
            .as_ref()
            .ok_or(Error::Invalid("information tail unavailable"))?;
        let cell = |key, span, text| Cell {
            kind: 0x19,
            key,
            span,
            // Finite resource descriptors have S cleared; constructor query
            // state is unit+4C=0/unit+170 bit 0=0. Other live queries may hold.
            selectable: self.information_selection(0, 0, false),
            text,
        };
        let comment = tail[2..18]
            .split(|b| *b == 0)
            .next()
            .expect("nonempty split")
            .to_vec();
        Ok(Some([
            [
                cell(0x3ae, false, captions[0].clone()),
                cell(0x3ae, true, Vec::new()),
            ],
            [
                cell(0x3ae, false, captions[1].clone()),
                cell(0xb0, false, comment),
            ],
        ]))
    }
    pub fn information_selection(&self, unit_locked: u32, live_flags: u8, previous: bool) -> bool {
        if unit_locked != 0 || live_flags & 1 != 0 {
            previous
        } else {
            false
        }
    }
    pub fn information_close(&mut self, click: bool) -> Result<(), Error> {
        if !matches!(self.information, Life::Open | Life::ReleaseRequested) {
            return Err(Error::Invalid("information form not alive"));
        }
        self.event(
            if click {
                "information_click"
            } else {
                "information_close"
            },
            0,
        );
        self.information = Life::ReleaseRequested;
        self.event("information_release_request", 0);
        Ok(())
    }
    pub fn destroy_retired_popup(&mut self, index: usize) -> Result<(), Error> {
        let life = self
            .retired_popups
            .get_mut(index)
            .ok_or(Error::Invalid("retired popup identity unavailable"))?;
        if !matches!(life, Life::Open | Life::ReleaseRequested) {
            return Err(Error::Invalid("retired popup not alive"));
        }
        *life = Life::Destroyed;
        self.event("retired_popup_destroy_delivered", index as i64);
        Ok(())
    }
    pub fn destroy(&mut self, target: &str) -> Result<(), Error> {
        self.destroy_body(target, None)
    }
    pub fn destroy_document(
        &mut self,
        target: &str,
        receiver: &mut DocumentReceiver<'_>,
    ) -> Result<(), Error> {
        self.destroy_body(target, Some(receiver.ui))
    }
    fn destroy_body(&mut self, target: &str, receiver: Option<&mut UiState>) -> Result<(), Error> {
        match target {
            "information" => {
                self.event("owner_enable_request", 1);
                self.event("owner_focus_request", 0);
                if let Some(ui) = receiver {
                    ui.owner_destroyed()
                        .map_err(|_| Error::Invalid("main form is absent"))?;
                }
                self.event("destroy_information_unit", 0);
                self.information = Life::Destroyed;
            }
            "com" => self.form = Life::Destroyed,
            "search" => self.popup = Life::Destroyed,
            _ => {
                return Err(Error::Invalid(
                    "unknown information controller destroy target",
                ));
            }
        }
        self.event("destroy_delivered", 0);
        Ok(())
    }
}
