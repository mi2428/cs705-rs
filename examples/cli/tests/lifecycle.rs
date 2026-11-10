//! Offline MRU process checks use synthetic document bytes and path observations.

use cs705::{
    clone::IMAGE_CAPACITY,
    icf::{
        self,
        context::Context,
        document::{DefaultSource, Document, Session},
    },
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
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
fn recent_process_open_and_visible_boundary() {
    let parent = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(&parent).unwrap();
    let root = parent.join(format!("lifecycle-recent-{}", std::process::id()));
    // Cleanup owns only a freshly created leaf, never a preexisting directory.
    fs::create_dir(&root).unwrap();
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
        serde_json::to_vec(&json!({"generation_q_hex": q, "live_q_hex": q})).unwrap(),
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
    fs::write(
        &config,
        serde_json::to_vec(&json!({"recent-files": paths})).unwrap(),
    )
    .unwrap();
    let observation = |left: &str, right: &str, result: i32| {
        json!({"left_hex": hex(left.as_bytes()), "right_hex": hex(right.as_bytes()),
            "locale": 1024, "flags": 1, "result": result})
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
    fs::write(&comparisons, serde_json::to_vec(&rows).unwrap()).unwrap();
    let invoke = |index: &str| {
        Command::new(env!("CARGO_BIN_EXE_cs705"))
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
    let report: Value = serde_json::from_slice(&opened.stdout).unwrap();
    assert_eq!(
        report["effect"]["open_status"],
        "Opened { adopted_context: false, foreign_context: false }"
    );
    assert_eq!(report["path"]["utf8"], text(&input));
    assert_eq!(report["dirty"], 0);
    assert_eq!(report["baseline_matches_current"], true);
    assert_eq!(report["effect"]["prompt"].get("choice"), Some(&Value::Null));
    assert_eq!(report["effect"]["prompt"]["result"], 6);
    assert_eq!(report["unused_comparisons"], 0);
    assert_eq!(report["publications"], 1);
    assert_eq!(report["visible_recent"], 4);

    let saved = fs::read(&config).unwrap();
    let preferences: Value = serde_json::from_slice(&saved).unwrap();
    let recent = preferences["recent-files"].as_array().unwrap();
    assert_eq!(recent.len(), 10);
    assert_eq!(recent[0], text(&input));
    assert_eq!(
        recent[..5]
            .iter()
            .map(|path| path.as_str().unwrap())
            .collect::<HashSet<_>>(),
        paths.iter().map(String::as_str).collect::<HashSet<_>>()
    );
    assert!(recent[5..].iter().all(|path| path.as_str() == Some("")));

    let outside = invoke("4");
    assert_eq!(outside.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&outside.stderr)
            .contains("recent menu index outside visible entries")
    );
    assert!(outside.stdout.is_empty());
    assert_eq!(fs::read(&config).unwrap(), saved);
}
