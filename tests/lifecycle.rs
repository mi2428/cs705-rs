use cs705::{
    clone::IMAGE_CAPACITY,
    icf::{
        self,
        context::Context,
        document::{
            DefaultSource, Document, InitialBoundary, OpenStatus, SaveOutcome, Session,
            ValidationSource, lifecycle::*,
        },
    },
};
use std::{collections::VecDeque, io};

struct Compare;
impl PathComparison for Compare {
    fn compare(&mut self, left: &[u8], right: &[u8]) -> Result<i32, icf::Error> {
        // Explicit mock native observations, not host case folding.
        Ok(if left == b"API-FAIL" {
            0
        } else if left == right || (left == b"A" && right == b"a") {
            2
        } else {
            1
        })
    }
}
struct Store([Vec<u8>; RECENT_LIMIT]);
impl RecentStore for Store {
    fn read(&mut self, index: usize) -> io::Result<Vec<u8>> {
        Ok(self.0[index].clone())
    }
    fn write(&mut self, index: usize, path: &[u8]) -> io::Result<()> {
        self.0[index] = path.to_vec();
        Ok(())
    }
}
struct Input {
    selections: VecDeque<Option<SaveSelection>>,
    choices: VecDeque<(Prompt, i32)>,
    initial_filters: Vec<u32>,
}
impl Input {
    fn empty() -> Self {
        Self {
            selections: VecDeque::new(),
            choices: VecDeque::new(),
            initial_filters: Vec::new(),
        }
    }
}
impl Dialogs for Input {
    fn save_as(&mut self, initial: SaveDialog) -> Result<Option<SaveSelection>, icf::Error> {
        self.initial_filters.push(initial.filter_index);
        Ok(self.selections.pop_front().expect("unexpected dialog"))
    }
    fn choice(&mut self, prompt: Prompt) -> Result<i32, icf::Error> {
        let (expected, choice) = self.choices.pop_front().expect("unexpected prompt");
        assert_eq!(prompt, expected);
        Ok(choice)
    }
}
#[derive(Default)]
struct FileSystem {
    bytes: Vec<u8>,
    reads: usize,
    publications: usize,
    fail: bool,
}
impl Files for FileSystem {
    fn read(&mut self, _: &[u8]) -> io::Result<Vec<u8>> {
        self.reads += 1;
        Ok(self.bytes.clone())
    }
    fn publish(&mut self, _: &[u8], bytes: &[u8]) -> io::Result<()> {
        if self.fail {
            return Err(io::Error::other("mock publication failure"));
        }
        self.bytes = bytes.to_vec();
        self.publications += 1;
        Ok(())
    }
}
struct Window {
    result: Option<Placement>,
    calls: usize,
}
impl PlacementSource for Window {
    fn placement(&mut self) -> Result<Option<Placement>, icf::Error> {
        self.calls += 1;
        Ok(self.result)
    }
}

fn session_document() -> (Session, Document) {
    // Synthetic source fields, not a captured G or compiled application payload.
    let mut fields = vec![0; 0x3c9c0];
    fields[0x355a1] = 6;
    for group in 0..100 {
        let at = 0x355c0 + 19 * group;
        fields[at] = if group == 99 { 255 } else { (group + 1) as u8 };
        fields[at + 1..at + 3].fill(255);
        fields[at + 3..at + 19].fill(b' ');
    }
    for range in [
        0x35d2c..0x36628,
        0x3ab82..0x3bf6e,
        0x3c0a7..0x3c2ff,
        0x3c385..0x3c613,
    ] {
        fields[range].fill(255);
    }
    fields[0x3662d..0x36821].fill(1);
    for row in 0..45 {
        let at = icf::special::CompactKind::BandStack.offset(row).unwrap();
        fields[at..at + 4].fill(255);
    }
    for row in 0..=10 {
        fields[0x36f54 + 36 * row..0x36f58 + 36 * row].fill(255);
    }
    fields[0xa60b..0xa60f].fill(255);
    fields[0x3c84b] = 255;
    fields[0x3c8b9..0x3c8bb].fill(255);
    fields[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
    let mut context = Context::before_registry();
    assert!(context.decode_packed(0x40001));
    let mut session = Session::new(context, None, 1, [1, 0]);
    let mut doc = Document::from_current(vec![0; IMAGE_CAPACITY], b"OLD".to_vec(), 6);
    session
        .regenerate_defaults(
            DefaultSource {
                selector: 1,
                prefix: &fields,
                japanese_mode_pairs: None,
                restricted_band_stack: None,
            },
            doc.current(),
        )
        .unwrap();
    doc.new_file(&session, 7).unwrap();
    doc.accept_saved(b"OLD".to_vec(), 6);
    (session, doc)
}

#[test]
#[ignore = "requires an explicitly built offline CLI binary"]
fn recent_process_open_and_visible_boundary() {
    use std::{
        fs,
        io::Write,
        path::PathBuf,
        process::{Command, Stdio},
    };
    let binary = std::env::var_os("CS705_LIFECYCLE_CLI").expect("set CS705_LIFECYCLE_CLI to the current CLI binary");
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/private/logic-lifecycle/recent-process-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let fixture = Fixture(root);
    let path = |name: &str| fixture.0.join(name);
    let text = |p: &std::path::Path| p.to_str().unwrap().to_owned();
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
    let (session, document) = session_document();
    let input = path("selected.icf");
    let current = path("current.bin");
    let generation = path("generation.bin");
    let context = path("context.json");
    let config = path("preferences.json");
    let comparisons = path("comparisons.json");
    fs::write(&input, document.current_bytes(&session).unwrap()).unwrap();
    fs::write(&current, document.current()).unwrap();
    fs::write(&generation, session.defaults().unwrap()).unwrap();
    let q = hex(session.context.as_bytes());
    fs::write(
        &context,
        format!(r#"{{"generation_q_hex":"{q}","live_q_hex":"{q}"}}"#),
    )
    .unwrap();
    let paths: Vec<_> = (0..5)
        .map(|index| {
            if index == 0 {
                text(&input)
            } else {
                text(&path(&format!("unselected-{index}.icf")))
            }
        })
        .collect();
    assert!(
        paths
            .iter()
            .all(|p| p.is_ascii() && !p.contains(['"', '\\']))
    );
    fs::write(
        &config,
        format!(
            "{{\"recent-files\":[{}]}}",
            paths
                .iter()
                .map(|p| format!("\"{p}\""))
                .collect::<Vec<_>>()
                .join(",")
        ),
    )
    .unwrap();
    let observation = |left: &str, right: &str, result: i32| {
        format!(
            r#"{{"left_hex":"{}","right_hex":"{}","locale":1024,"flags":1,"result":{result}}}"#,
            hex(left.as_bytes()),
            hex(right.as_bytes())
        )
    };
    let mut rows = Vec::new();
    // Mock native comparisons requested during append-load, then Open's insert.
    for right in 0..paths.len() {
        for left in (0..right).rev() {
            rows.push(observation(&paths[left], &paths[right], 1));
        }
    }
    for left in paths.iter().rev() {
        rows.push(observation(
            left,
            &paths[0],
            if left == &paths[0] { 2 } else { 1 },
        ));
    }
    fs::write(&comparisons, format!("[{}]", rows.join(","))).unwrap();
    let invoke = |index: &str| {
        Command::new(&binary)
            .args([
                "--config",
                &text(&config),
                "recent",
                index,
                "--range-context",
                &text(&context),
                "--current",
                &text(&current),
                "--generation",
                &text(&generation),
                "--generation-selector",
                "1",
                "--previous-reload-guard",
                "1",
                "--document-flags",
                "0000",
                "--document-path-hex",
                "4F4C44",
                "--path-comparisons",
                &text(&comparisons),
                "--allow-file-write",
                "--overwrite",
            ])
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    let opened = invoke("0");
    assert!(
        opened.status.success(),
        "{}",
        String::from_utf8_lossy(&opened.stderr)
    );
    let mut check = Command::new("python3")
        .args([
            "-c",
            r#"
import json, sys
r = json.load(sys.stdin)
p = json.load(open(sys.argv[1], encoding='utf-8'))['recent-files']
assert r['effect']['open_status'] == 'Opened { adopted_context: false, foreign_context: false }'
assert r['path']['utf8'] == sys.argv[2] and r['dirty'] == 0 and r['baseline_matches_current']
assert r['effect']['prompt']['choice'] is None and r['effect']['prompt']['result'] == 6
assert r['unused_comparisons'] == 0 and r['publications'] == 1 and r['visible_recent'] == 4
assert len(p) == 10 and p[0] == sys.argv[2] and len(set(p[:5])) == 5 and p[5:] == [''] * 5
"#,
        ])
        .arg(&config)
        .arg(&input)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    check
        .stdin
        .take()
        .unwrap()
        .write_all(&opened.stdout)
        .unwrap();
    let checked = check.wait_with_output().unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let saved = fs::read(&config).unwrap();
    let outside = invoke("4");
    assert_eq!(outside.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&outside.stderr)
            .contains("recent menu index outside visible entries")
    );
    assert!(outside.stdout.is_empty());
    assert_eq!(fs::read(&config).unwrap(), saved);
}

#[test]
fn lifecycle_drives_real_document_publication_mru_and_owned_close_state() {
    let mut compare = Compare;
    let mut store = Store(std::array::from_fn(|_| Vec::new()));
    store.0[0] = b"A".to_vec();
    store.0[1] = b"a".to_vec();
    let mut recent = RecentFiles::load(&mut store, &mut compare).unwrap();
    assert_eq!(recent.paths(), &[b"a".to_vec()]);
    recent.insert(b"API-FAIL".to_vec(), &mut compare).unwrap();
    recent.insert(b"API-FAIL".to_vec(), &mut compare).unwrap();
    assert_eq!(
        recent.paths()[..2],
        [b"API-FAIL".to_vec(), b"API-FAIL".to_vec()]
    );
    for n in 0..12 {
        recent.insert(vec![b'0' + n], &mut compare).unwrap();
    }
    assert_eq!((recent.paths().len(), recent.visible().len()), (10, 4));
    recent.persist(&mut store).unwrap();
    assert_eq!(&store.0, recent.paths());
    assert!(recent.selected(4).is_err());
    for index in 1..=6 {
        assert_eq!(target_filter(filter_target(index)), index);
    }
    assert_eq!(
        (filter_target(0), filter_target(7), target_filter(255)),
        (6, 6, 1)
    );

    let (mut session, mut doc) = session_document();
    doc.begin_pending_name(7, b"LIVE".to_vec());
    doc.patch_current(&[(0x37158, b"a")]).unwrap();
    doc.compare_dirty();
    let baseline = doc.baseline().to_vec();
    let mut life = Lifecycle {
        recent: RecentFiles::default(),
        ui: UiState::headless(),
    };
    let mut input = Input::empty();
    let mut files = FileSystem {
        fail: true,
        ..Default::default()
    };
    let mut options = SaveOptions {
        validation: ValidationSource::NoTree,
        reset: None,
    };
    let failure = life
        .save(
            &mut doc,
            &mut session,
            SaveMode::Save,
            &mut options,
            &mut input,
            &mut files,
            &mut compare,
        )
        .unwrap();
    assert_eq!(failure.outcome, SaveOutcome::NotSaved);
    assert!(failure.attempts[0].publication_error.is_some());
    assert_eq!(doc.path(), b"OLD");
    assert_eq!(doc.baseline(), baseline);
    assert_ne!(doc.dirty(), 0);
    assert_eq!(doc.current()[0x37158], b'A');
    assert!(life.recent.paths().is_empty());
    input.choices.push_back((Prompt::Dirty, 6));
    let prompt = life
        .new_file(
            &mut doc,
            &mut session,
            &mut options,
            &mut input,
            &mut files,
            &mut compare,
        )
        .unwrap();
    assert_eq!((prompt.choice, prompt.result), (Some(6), 2));
    assert_eq!(doc.path(), b"OLD");
    assert_eq!(doc.baseline(), baseline);
    input.selections.push_back(None);
    let cancelled = life
        .save(
            &mut doc,
            &mut session,
            SaveMode::SaveAs,
            &mut options,
            &mut input,
            &mut files,
            &mut compare,
        )
        .unwrap();
    assert!(cancelled.dialog_cancelled && cancelled.attempts.is_empty());
    input.initial_filters.clear();
    files.fail = false;
    input.selections.extend([
        Some(SaveSelection {
            path: b"LEGACY".to_vec(),
            filter_index: 2,
        }),
        Some(SaveSelection {
            path: b"SAVED".to_vec(),
            filter_index: 1,
        }),
    ]);
    input.choices.push_back((Prompt::Legacy { target: 5 }, 7));
    let saved = life
        .save(
            &mut doc,
            &mut session,
            SaveMode::SaveAs,
            &mut options,
            &mut input,
            &mut files,
            &mut compare,
        )
        .unwrap();
    assert_eq!(saved.outcome, SaveOutcome::Saved);
    assert_eq!(saved.attempts.len(), 2);
    assert_eq!(
        saved.attempts[0].report.as_ref().unwrap().status,
        cs705::icf::document::SaveStatus::RetrySaveAs
    );
    assert_eq!(input.initial_filters, [1, 1]);
    assert_eq!(files.publications, 1);
    assert_eq!(
        icf::format::read(&files.bytes).unwrap().status,
        icf::format::Status::Success
    );
    assert_eq!(doc.path(), b"SAVED");
    assert_eq!(doc.baseline(), doc.current());
    assert_eq!(doc.dirty(), 0);
    assert_eq!(session.document_flags, [0, 0]);
    assert_eq!(life.recent.paths(), &[b"SAVED".to_vec()]);

    doc.patch_current(&[(0x37158, b"b")]).unwrap();
    doc.compare_dirty();
    input.choices.push_back((Prompt::Dirty, 2));
    let (_, status) = life
        .open_recent(
            &mut doc,
            &mut session,
            0,
            None,
            None,
            &mut options,
            &mut input,
            &mut files,
            &mut compare,
        )
        .unwrap();
    assert_eq!(status, OpenStatus::Cancelled);
    assert_eq!(files.reads, 0);
    input.choices.push_back((Prompt::Dirty, 7));
    let (_, status) = life
        .open_recent(
            &mut doc,
            &mut session,
            0,
            None,
            None,
            &mut options,
            &mut input,
            &mut files,
            &mut compare,
        )
        .unwrap();
    assert!(matches!(status, OpenStatus::Opened { .. }));
    assert_eq!(files.reads, 1);
    assert_eq!(doc.dirty(), 0);
    assert_eq!(doc.path(), b"SAVED");
    assert!(doc.pending_matches(7));

    fn query(
        context: &mut MenuContext<'_>,
        descriptor: &mut MenuDescriptor,
    ) -> Result<(), icf::Error> {
        assert_eq!(
            (descriptor.group, descriptor.key, descriptor.index),
            (0x19, 0x28, 3)
        );
        assert_eq!(descriptor.name, b"RAW_MENU_NAME");
        assert!(context.active.is_none());
        context
            .document
            .begin_pending_name(7, b"SAME LIVE DOCUMENT".to_vec());
        descriptor.flags1d = if context.settings.toolbar != 0 {
            0x80
        } else {
            0
        };
        descriptor.flags1e = 2;
        descriptor.shortcut18 = 99;
        Ok(())
    }
    // Captions are unused in this query-state fixture.
    life.ui.menu.push(MenuItem {
        descriptor: Some(MenuDescriptor {
            group: 0x19,
            key: 0x28,
            index: 3,
            name: b"RAW_MENU_NAME".to_vec(),
            caption: Vec::new(),
            flags1d: 0,
            flags1e: 0,
            shortcut18: 0,
        }),
        query: Some(query),
        property19a: false,
        property1ae: false,
        property1a1: true,
        enabled: true,
        shortcut: 0,
        caption: Vec::new(),
    });
    assert!(life.sync02(&mut doc, &mut session).is_err());
    life.ui.display_settings = Some(cs705::display::Settings {
        language: 0,
        system_language: 0,
        font_percent: 100,
        toolbar: 1,
        split: 1,
        callsign_dialog: 1,
        auto_correct: 1,
        fixed_name: 0,
        column_bits: 0,
        column_bits_high: 0,
    });
    life.sync02(&mut doc, &mut session).unwrap();
    assert_eq!(doc.pending_name().unwrap().name, b"SAME LIVE DOCUMENT");
    assert!(life.ui.menu[0].property19a && life.ui.menu[0].property1ae);
    assert!(!life.ui.menu[0].enabled);
    assert_eq!(life.ui.menu[0].shortcut, 99);
    life.ui.display_settings.as_mut().unwrap().toolbar = 0;
    life.sync02(&mut doc, &mut session).unwrap();
    assert!(!life.ui.menu[0].property19a);
    life.ui.menu[0].descriptor = None;
    life.ui.menu[0].enabled = true;
    life.sync02(&mut doc, &mut session).unwrap();
    assert!(!life.ui.menu[0].property19a && !life.ui.menu[0].property1a1);
    assert!(life.ui.menu[0].enabled);

    let view = ViewState {
        key: b"K".to_vec(),
        views: Vec::new(),
    };
    life.ui.view = Some(view.clone());
    life.ui.tree = Some(TreeState {
        keys: vec![b"K".to_vec()],
        selected: Some(0),
        groups: Vec::new(),
    });
    life.ui.view = None;
    life.ui.view_sources.push(view);
    doc.patch_current(&[
        (0x3fb40, &2_u32.to_le_bytes()),
        (0x355c3, b"FIRST           "),
    ])
    .unwrap();
    life.ui.sync25(&mut doc, &mut session).unwrap();
    assert!(life.ui.view.is_some());
    assert_eq!(life.ui.tree.as_ref().unwrap().groups.len(), 2);
    assert_eq!(
        life.ui.tree.as_ref().unwrap().groups[0],
        b"FIRST           "
    );
    life.ui
        .initial_boundary(InitialBoundary::Additional, &mut doc, &mut session)
        .unwrap();
    assert!(
        life.ui
            .initial_boundary(InitialBoundary::Sync(0x49), &mut doc, &mut session)
            .is_err()
    );
    if let Some(path) = std::env::var_os("CS705_LIFECYCLE_RESOURCE_SOURCE") {
        let asset =
            cs705::printing::native::read_source(std::fs::File::open(path).unwrap()).unwrap();
        let mut native =
            UiState::from_native_source(std::sync::Arc::from(asset), &mut doc, &mut session, 1)
                .unwrap();
        assert!(!native.native.as_ref().unwrap().factories.is_empty());
        assert_eq!(
            native.tree.as_ref().unwrap().keys.len(),
            native.native.as_ref().unwrap().factories.len()
        );
        let selected = native
            .native
            .as_ref()
            .unwrap()
            .factories
            .iter()
            .position(|f| f.node == 2)
            .unwrap();
        native
            .select_native_index(selected, &mut doc, &mut session)
            .unwrap();
        assert!(
            native
                .native
                .as_ref()
                .unwrap()
                .active
                .as_ref()
                .unwrap()
                .dataset
                .is_some()
        );
        assert!(native.sync02(&mut doc, &mut session, &life.recent).is_err());
    }
    life.ui.image_list.push(42);
    let mut window = Window {
        result: None,
        calls: 0,
    };
    let mut incoming = true;
    life.ui.owner_enabled(0).unwrap();
    life.close(
        &mut doc,
        &mut session,
        &mut incoming,
        &mut options,
        &mut input,
        &mut files,
        &mut compare,
        &mut window,
    )
    .unwrap();
    assert!(!incoming);
    assert_eq!(window.calls, 0);
    assert!(life.ui.view.is_some());
    life.ui.owner_destroyed().unwrap();
    incoming = false;
    life.close_query(
        &mut doc,
        &mut session,
        &mut incoming,
        &mut options,
        &mut input,
        &mut files,
        &mut compare,
        &mut window,
    )
    .unwrap();
    assert!(!incoming);
    assert_eq!(window.calls, 1);
    assert!(life.ui.view.is_some());
    life.ui.sync29(&mut doc).unwrap();
    assert_eq!(life.ui.application5b, 1);
    life.ui.sync2f(&mut doc).unwrap();
    assert_eq!(life.ui.form.as_ref().unwrap().enabled58, 0);
    life.ui.owner_destroyed().unwrap();
    assert!(!life.ui.sync28(&mut doc, true));
    assert!(life.ui.sync28(&mut doc, false));
    assert_eq!((life.ui.operation40, life.ui.fc, life.ui.fd), (0, 1, 0));
    life.ui
        .initial_boundary(InitialBoundary::Title, &mut doc, &mut session)
        .unwrap();
    assert_eq!(life.ui.title.as_ref().unwrap().path, b"SAVED");
    let placement = Placement {
        flags: 1,
        show: 2,
        minimum: [3, 4],
        maximum: [5, 6],
        normal: [7, 8, 9, 10],
    };
    window.result = Some(placement);
    incoming = true;
    life.close(
        &mut doc,
        &mut session,
        &mut incoming,
        &mut options,
        &mut input,
        &mut files,
        &mut compare,
        &mut window,
    )
    .unwrap();
    assert!(incoming);
    assert!(!life.ui.task_enabled);
    assert_eq!(life.ui.task_period, -1);
    assert!(life.ui.view.is_none() && life.ui.tree.is_none() && life.ui.form.is_none());
    assert_eq!(life.ui.image_list, [42]);
    assert_eq!(life.ui.saved_placement, Some(placement));
    assert_eq!(doc.path(), b"SAVED");
    assert!(doc.pending_matches(7));
}
