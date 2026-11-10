//! Process-level special commands, using synthetic storage and raw G only.
//! Full-image literal comparisons do not call the operation being tested.

use cs705::icf::Icf;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "special-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.path(name);
        fs::write(&p, bytes).unwrap();
        p
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}
fn cli(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cs705"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Err(e) = child.stdin.take().unwrap().write_all(input) {
        assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe);
    }
    child.wait_with_output().unwrap()
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn value(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}
fn image() -> Vec<u8> {
    let mut m = vec![0xa5; 0x3c9c0];
    m[0x355a1] = 6;
    m[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    m[0xa60b..0xa60f].fill(255);
    m[0xa78c] = 255;
    // C5DE0's first guard is false; no invented G is needed for these tests.
    m[0x3a8ca] = 1;
    for r in [23usize, 24] {
        let a = 0xa368 + 27 * r;
        m[a..a + 4].copy_from_slice(&(60_000 + r as u32).to_le_bytes());
        m[a + 4..a + 8].copy_from_slice(&(30_000 + r as u32).to_le_bytes());
        m[a + 8..a + 11].copy_from_slice(&[7, 0, 2]);
        m[a + 11..a + 27].fill(b' ');
        m[a + 11] = b'A' + (r - 23) as u8;
        m[0xa60b + r / 8] &= !(1 << (r % 8));
    }
    m[0x37802] = 23 << 2;
    for r in 0..5 {
        let a = 0xa60f + 41 * r;
        m[a..a + 41].fill(0);
        m[a] = 1;
        m[a + 3] = 10;
        m[a + 4..a + 6].fill(255);
        m[a + 25..a + 41].fill(b' ');
        m[0xa6dc + 22 * r..0xa6dc + 22 * (r + 1)].fill(0);
    }
    m
}
fn icf(m: &[u8]) -> Vec<u8> {
    Icf::from_memory(m.to_vec()).unwrap().to_bytes()
}
fn generation(m: &[u8]) -> Vec<u8> {
    let mut g = m.to_vec();
    g.resize(0x4997a, 0);
    g
}
fn result_image(output: &Output) -> Vec<u8> {
    success(output);
    Icf::parse(&output.stdout).unwrap().memory().to_vec()
}
fn range_context(generation_kind: i32, live_kind: i32, f: u8, langid: Option<u16>) -> Vec<u8> {
    let mut generation = [0u8; 24];
    generation[..4].copy_from_slice(&generation_kind.to_le_bytes());
    generation[4] = 1;
    generation[5] = 1;
    generation[8..12].copy_from_slice(&3u32.to_le_bytes());
    generation[12] = 1;
    generation[13] = 1;
    generation[14] = 1;
    generation[15] = f;
    let mut live = generation;
    live[..4].copy_from_slice(&live_kind.to_le_bytes());
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
    serde_json::to_vec(&json!({"generation_q_hex":hex(&generation),"live_q_hex":hex(&live),"system_langid":langid})).unwrap()
}

#[test]
fn r3_settings_preset_numeric_input_uses_library_grammar_and_approval_or() {
    let files = Files::new();
    for (field, text, offset, expected) in [
        ("usb-af-level", ".", 16, 0),
        ("usb-if-level", "127.5", 18, 128),
        ("usb-mod-level", "1e3", 15, 13),
        ("usb-af-level", "256", 16, 255),
        ("civ-address", "foo", 22, 15),
        ("civ-address", "", 22, 2),
        ("civ-address", "0x7c", 22, 7),
        ("bandwidth-hz", "501", 3, 9),
        ("bandwidth-hz", "550", 3, 10),
    ] {
        let mut raw = image();
        raw[0xa60f + offset] = 127;
        let g = files.write("numeric-g", &generation(&raw));
        let assignment = format!("{field}={text}");
        let out = cli(
            &[
                "special",
                "preset",
                "-",
                "edit",
                "0",
                "--set",
                &assignment,
                "--seed",
                p(&g),
                "-o",
                "-",
            ],
            &icf(&raw),
        );
        let actual = result_image(&out);
        raw[0xa60f + offset] = expected;
        if field == "bandwidth-hz" && expected == 10 {
            raw[0xa613..0xa615].fill(0xa5);
        }
        assert_eq!(actual, raw, "{field}={text}");
    }
    let raw = image();
    for field in ["usb-af-level", "civ-address", "bandwidth-hz"] {
        for action in ["begin", "display", "cancel"] {
            let output = files.write("not-approved", b"keep");
            let assignment = format!("{field}={}", json!({"action":action}));
            let out = cli(
                &[
                    "special",
                    "preset",
                    "-",
                    "edit",
                    "0",
                    "--set",
                    &assignment,
                    "-o",
                    p(&output),
                    "--overwrite",
                ],
                &icf(&raw),
            );
            success(&out);
            assert_eq!(fs::read(&output).unwrap(), b"keep");
        }
    }
    let out = cli(
        &[
            "special",
            "preset",
            "-",
            "edit",
            "0",
            "--set",
            "usb-af-level=200",
            "--set",
            "usb-if-level={\"action\":\"cancel\"}",
            "--set",
            "usb-mod-level={\"action\":\"begin\"}",
            "-o",
            "-",
        ],
        &icf(&raw),
    );
    let mut expected = raw.clone();
    expected[0xa61f] = 200;
    assert_eq!(result_image(&out), expected);
    let out = cli(
        &[
            "special",
            "preset",
            "-",
            "edit",
            "0",
            "--set",
            "usb-af-level=0",
            "--set",
            "usb-if-level={\"action\":\"cancel\"}",
            "-o",
            "-",
        ],
        &icf(&raw),
    );
    assert_eq!(result_image(&out), raw);
}

#[test]
fn special_help_domain_and_stdin_ownership_precede_input_reads() {
    success(&cli(&["special", "--help"], b""));
    for sheet in ["bandstack", "memopad", "scan", "user-edge", "preset"] {
        success(&cli(&["special", sheet, "--help"], b""));
    }
    for args in [
        vec![
            "special",
            "bandstack",
            "-",
            "edit",
            "0",
            "--set",
            "mode=0",
            "--dry-run",
        ],
        vec!["special", "preset", "-", "recall", "0"],
        vec!["special", "scan", "-", "edit", "0", "--set", "mode=0"],
    ] {
        let out = cli(&args, b"");
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
    }
    for args in [
        vec![
            "special",
            "scan",
            "-",
            "paste",
            "0",
            "--clipboard",
            "-",
            "--yes",
            "--dry-run",
        ],
        vec![
            "special",
            "preset",
            "-",
            "edit",
            "0",
            "--set",
            "mode=0",
            "--seed",
            "-",
            "--dry-run",
        ],
        vec![
            "special",
            "memopad",
            "-",
            "reset",
            "--seed",
            "-",
            "--yes",
            "--dry-run",
        ],
    ] {
        let out = cli(&args, b"");
        assert_eq!(out.status.code(), Some(3));
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains("stdin may be consumed by only one input")
        );
    }
    let out = cli(&["special", "fields"], b"");
    success(&out);
    assert_eq!(value(&out).as_array().unwrap().len(), 24);
}

#[test]
fn special_views_read_library_endpoints_and_escape_opaque_names() {
    let mut m = image();
    let a = 0x36f54;
    m[a..a + 4].copy_from_slice(&123u32.to_le_bytes());
    for row in 1..11 {
        let a = 0x36f54 + 36 * row;
        m[a..a + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    }
    let out = cli(&["special", "memopad", "-", "show"], &icf(&m));
    success(&out);
    let rows = value(&out);
    assert_eq!(rows.as_array().unwrap().len(), 10);
    assert_eq!(rows[0]["index"], 1);
    assert_eq!(rows[9]["index"], 10);
    assert_eq!(rows[0]["valid"], false);
    let out = cli(
        &[
            "special", "scan", "-", "show", "--start", "24", "--count", "1",
        ],
        &icf(&m),
    );
    success(&out);
    assert_eq!(value(&out)[0]["high_hz"], 60_024);
    assert_eq!(value(&out)[0]["mode"], 13);
    let a = 0x378c0 + 8 * 29;
    m[a..a + 4].copy_from_slice(&144_000_000u32.to_le_bytes());
    m[a + 4..a + 8].copy_from_slice(&146_000_000u32.to_le_bytes());
    let out = cli(
        &[
            "special",
            "user-edge",
            "-",
            "show",
            "--start",
            "29",
            "--count",
            "1",
        ],
        &icf(&m),
    );
    success(&out);
    assert_eq!(value(&out)[0]["low_hz"], 144_000_000);
    m[0xa628 + 41 * 4] = 0x1b;
    let out = cli(
        &[
            "special", "preset", "-", "show", "--start", "4", "--count", "1",
        ],
        &icf(&m),
    );
    success(&out);
    assert!(
        value(&out)[0]["name"]["hex"]
            .as_str()
            .unwrap()
            .starts_with("1B")
    );
    assert!(!out.stdout.contains(&0x1b));
}

#[test]
fn scan_edit_known_literal_changes_only_its_full_record() {
    let m = image();
    let mut expected = m.clone();
    let a = 0xa368 + 27 * 24;
    expected[a + 8..a + 11].copy_from_slice(&[6, 1, 255]);
    expected[a + 11..a + 27].copy_from_slice(b"CLI             ");
    let out = cli(
        &[
            "special",
            "scan",
            "-",
            "edit",
            "24",
            "--set",
            "mode=8",
            "--set",
            "data=1",
            "--set",
            "filter=255",
            "--set",
            "name=CLI",
            "-o",
            "-",
        ],
        &icf(&m),
    );
    assert_eq!(result_image(&out), expected);
    let out = cli(
        &[
            "special",
            "scan",
            "-",
            "edit",
            "24",
            "--set",
            "name=\u{00e9}",
            "-o",
            "-",
        ],
        &icf(&m),
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    let out = cli(
        &[
            "special", "scan", "-", "edit", "24", "--set", "mode=15", "--set", "filter=1", "-o",
            "-",
        ],
        &icf(&m),
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

#[test]
fn preset_all_catalog_keys_and_both_endpoint_pairs_match_literal_full_image() {
    let files = Files::new();
    let mut m = image();
    m[0xa78c] = 4;
    let mut g = generation(&m);
    g[0xa78c] = 255;
    g[0x37825] = 1;
    g[0x37857] = 10;
    g[0x37862] = 1;
    g[0x37866] = 0;
    g[0x392ee..0x392f0].copy_from_slice(&[3, 2]);
    let seed = files.write("g.raw", &g);
    let edits = [
        "mode=5",
        "filter=1",
        "bandwidth-hz=600",
        "filter-type-hf=1",
        "filter-type50m=0",
        "comp=1",
        "data-off-mod=3",
        "data-on-mod=2",
        "tbw=2",
        "ssb-low=0",
        "ssb-high=1",
        "ssb-data-low=2",
        "ssb-data-high=3",
        "usb-output=1",
        "usb-mod-level=200",
        "usb-af-level=201",
        "usb-af-squelch=1",
        "usb-if-level=202",
        "usb-send=2",
        "usb-cw=3",
        "usb-rtty=4",
        "civ-address=7c",
        "civ-transceive=1",
        "usb-echo=1",
    ];
    let mut args = vec![
        "special",
        "preset",
        "-",
        "edit",
        "4",
        "--seed",
        p(&seed),
        "-o",
        "-",
    ];
    for edit in edits {
        args.extend(["--set", edit]);
    }
    let out = cli(&args, &icf(&m));
    let mut expected = m;
    expected[0xa60f + 41 * 4..0xa60f + 41 * 4 + 25].copy_from_slice(&[
        3, 0, 1, 10, 1, 0, 1, 3, 2, 2, 0, 1, 2, 3, 1, 200, 201, 1, 202, 2, 3, 4, 0x7c, 1, 1,
    ]);
    expected[0xa78c] = 255;
    assert_eq!(result_image(&out), expected);
}

#[test]
fn preset_mask_ff_and_01_toggle_have_different_usb_conflict_effects() {
    for old in [255u8, 1] {
        let mut m = image();
        let a = 0xa60f;
        let mask = 0xa6dc + 16;
        m[a + 19..a + 22].copy_from_slice(&[2, 2, 2]);
        m[mask] = old;
        let out = cli(
            &[
                "special", "preset", "-", "toggle", "0", "usb-send", "-o", "-",
            ],
            &icf(&m),
        );
        let mut expected = m;
        expected[mask] = 0;
        if old == 255 {
            expected[a + 19] = 0;
        }
        assert_eq!(result_image(&out), expected);
    }
}

#[test]
fn cancel_late_failure_and_missing_g_never_publish_an_offline_prefix() {
    let files = Files::new();
    let mut m = image();
    let existing = files.write("existing.icf", b"must survive");
    for tail in [
        vec!["name", "0", "NEW", "--cancel"],
        vec![
            "edit",
            "0",
            "--set",
            "usb-af-level=200",
            "--set",
            "no-such-field=1",
        ],
    ] {
        let mut args = vec!["special", "preset", "-"];
        args.extend(tail);
        args.extend(["--output", p(&existing), "--overwrite"]);
        let out = cli(&args, &icf(&m));
        assert!(out.stdout.is_empty());
        assert_eq!(fs::read(&existing).unwrap(), b"must survive");
    }
    m[0xa78c] = 0;
    let out = cli(
        &[
            "special",
            "preset",
            "-",
            "edit",
            "0",
            "--set",
            "usb-af-level=200",
            "--output",
            p(&existing),
            "--overwrite",
        ],
        &icf(&m),
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("\"partial_in_memory\": true"));
    assert_eq!(fs::read(&existing).unwrap(), b"must survive");
    let out = cli(
        &[
            "special",
            "preset",
            "-",
            "reset",
            "0",
            "--seed",
            "absent",
            "--output",
            p(&existing),
            "--overwrite",
        ],
        &icf(&m),
    );
    assert_eq!(out.status.code(), Some(6));
    assert_eq!(fs::read(&existing).unwrap(), b"must survive");
}

#[test]
fn special_publication_no_clobber_dry_run_and_explicit_overwrite() {
    let files = Files::new();
    let m = image();
    let outpath = files.write("out.icf", b"old");
    let args = [
        "special",
        "preset",
        "-",
        "edit",
        "4",
        "--set",
        "usb-echo=1",
        "--output",
        p(&outpath),
    ];
    let out = cli(&args, &icf(&m));
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(fs::read(&outpath).unwrap(), b"old");
    let mut dry = args.to_vec();
    dry.push("--dry-run");
    let out = cli(&dry, &icf(&m));
    success(&out);
    assert!(out.stdout.is_empty());
    assert_eq!(fs::read(&outpath).unwrap(), b"old");
    let mut yes = args.to_vec();
    yes.push("--overwrite");
    let out = cli(&yes, &icf(&m));
    success(&out);
    assert_eq!(
        Icf::parse(&fs::read(&outpath).unwrap()).unwrap().memory()[0xa60f + 41 * 4 + 24],
        1
    );
    assert_eq!(fs::read_dir(&files.0).unwrap().count(), 1);
}

#[test]
fn scan_copy_paste_and_clear_preserve_literal_tail_and_notification_order() {
    let files = Files::new();
    let m = image();
    let copy = cli(
        &[
            "special", "scan", "-", "copy", "23", "--count", "2", "-o", "-",
        ],
        &icf(&m),
    );
    success(&copy);
    let clip = files.write("scan.json", &copy.stdout);
    let out = cli(
        &[
            "special",
            "scan",
            "-",
            "paste",
            "24",
            "--clipboard",
            p(&clip),
            "--yes",
            "-o",
            "-",
        ],
        &icf(&m),
    );
    let mut expected = m.clone();
    let from = 0xa368 + 27 * 23;
    let to = 0xa368 + 27 * 24;
    expected[to..to + 27].copy_from_slice(&m[from..from + 27]);
    assert_eq!(result_image(&out), expected);
    let mut g = generation(&m);
    g[0xa60b..0xa60f].fill(255);
    let seed = files.write("g.raw", &g);
    let out = cli(
        &[
            "special",
            "scan",
            "-",
            "clear",
            "24",
            "--seed",
            p(&seed),
            "--yes",
            "-o",
            "-",
        ],
        &icf(&m),
    );
    let mut expected = m;
    expected[0xa60e] |= 1;
    expected[to..to + 11].fill(0);
    expected[to + 11..to + 27].fill(b' ');
    assert_eq!(result_image(&out), expected);
    let report: Value = serde_json::from_slice(&out.stderr).unwrap();
    let events = report["effects"]["notifications"].as_array().unwrap();
    assert_eq!(
        events
            .iter()
            .map(|e| e["event"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [0x14, 0x11, 0x3c]
    );
    assert_eq!(events[0]["second"], 24);
}

#[test]
fn malformed_clipboard_and_out_of_domain_candidates_leave_output_untouched() {
    let files = Files::new();
    let m = image();
    let output = files.write("out", b"unrelated");
    let clip = files.write(
        "bad.json",
        &serde_json::to_vec(&json!({"version":1,"sheet":"scan","records":[],"transfer_count":1}))
            .unwrap(),
    );
    let out = cli(
        &[
            "special",
            "scan",
            "-",
            "paste",
            "24",
            "--clipboard",
            p(&clip),
            "--yes",
            "--output",
            p(&output),
            "--overwrite",
        ],
        &icf(&m),
    );
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(fs::read(&output).unwrap(), b"unrelated");
    for edit in ["mode=15", "filter=3"] {
        let out = cli(
            &[
                "special",
                "preset",
                "-",
                "edit",
                "0",
                "--set",
                edit,
                "--output",
                p(&output),
                "--overwrite",
            ],
            &icf(&m),
        );
        assert_eq!(out.status.code(), Some(3), "{edit}");
        assert_eq!(fs::read(&output).unwrap(), b"unrelated");
    }
    let out = cli(
        &[
            "special",
            "scan",
            "-",
            "copy",
            "24",
            "--count",
            "2",
            "--output",
            p(&output),
            "--overwrite",
        ],
        &icf(&m),
    );
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(fs::read(&output).unwrap(), b"unrelated");
}

#[test]
fn compact_resets_use_explicit_g_and_preserve_memopad_residual_bytes() {
    let files = Files::new();
    let m = image();
    let mut g = generation(&m);
    for row in 1..11 {
        let a = 0x36f54 + 36 * row;
        g[a..a + 36].fill(0x5a);
        g[a..a + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    }
    let seed = files.write("memopad-g.raw", &g);
    let out = cli(
        &[
            "special",
            "memopad",
            "-",
            "reset",
            "--seed",
            p(&seed),
            "--yes",
            "-o",
            "-",
        ],
        &icf(&m),
    );
    let mut expected = m.clone();
    for row in 1..11 {
        let a = 0x36f54 + 36 * row;
        expected[a..a + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    }
    assert_eq!(result_image(&out), expected);
    let copied = cli(
        &[
            "special", "memopad", "-", "copy", "1", "--count", "10", "-o", "-",
        ],
        &out.stdout,
    );
    success(&copied);
    assert_eq!(value(&copied)["records"].as_array().unwrap().len(), 10);
    let bands = [0usize, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 10, 11, 14];
    let at = |row: usize| 0x368eb + 108 * bands[row / 3] + 36 * (row % 3);
    for row in 0..45 {
        let a = at(row);
        g[a..a + 36].fill(0);
        g[a..a + 4].copy_from_slice(&(30_000 + row as u32).to_le_bytes());
        g[a + 4] = 0x11;
        g[a + 5] = 1;
        g[a + 6] = 0xa4;
    }
    let seed = files.write("band-g.raw", &g);
    let out = cli(
        &[
            "special",
            "bandstack",
            "-",
            "reset",
            "--seed",
            p(&seed),
            "--yes",
            "-o",
            "-",
        ],
        &icf(&m),
    );
    let mut expected = m;
    for (dest, source) in (0..27).chain(30..45).enumerate() {
        expected[at(dest)..at(dest) + 36].copy_from_slice(&g[at(source)..at(source) + 36]);
    }
    assert_eq!(result_image(&out), expected);
    let report: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(report["writes"], 42);
    assert_eq!(report["notifications"].as_array().unwrap().len(), 47);
    let copy = cli(
        &[
            "special",
            "bandstack",
            "-",
            "copy",
            "0",
            "--count",
            "45",
            "-o",
            "-",
        ],
        &out.stdout,
    );
    success(&copy);
    assert_eq!(value(&copy)["records"].as_array().unwrap().len(), 42);
}

#[test]
fn preset_name_and_reset_preserve_machine_notification_order_and_other_presets() {
    let files = Files::new();
    let m = image();
    let out = cli(
        &["special", "preset", "-", "name", "4", " CLI  ", "-o", "-"],
        &icf(&m),
    );
    let mut expected = m.clone();
    expected[0xa628 + 41 * 4..0xa638 + 41 * 4].copy_from_slice(b" CLI            ");
    assert_eq!(result_image(&out), expected);
    let report: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(
        report["notifications"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["event"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [0x3e, 0x39, 0x11]
    );
    assert_eq!(report["notifications"][1]["synchronous"], true);
    let mut g = generation(&m);
    g[0xa60f + 41 * 4..0xa638 + 41 * 4].fill(0x6d);
    g[0xa6dc + 22 * 4..0xa6f2 + 22 * 4].fill(1);
    let seed = files.write("g.raw", &g);
    let out = cli(
        &[
            "special",
            "preset",
            "-",
            "reset",
            "4",
            "--seed",
            p(&seed),
            "--yes",
            "-o",
            "-",
        ],
        &icf(&m),
    );
    let mut expected = m;
    expected[0xa60f + 41 * 4..0xa638 + 41 * 4].fill(0x6d);
    expected[0xa6dc + 22 * 4..0xa6f2 + 22 * 4].fill(1);
    assert_eq!(result_image(&out), expected);
}

#[test]
fn scan_move_selection_delete_and_insert_reach_the_library_operations() {
    let files = Files::new();
    let m = image();
    let a = 0xa368 + 27 * 23;
    let b = a + 27;
    let out = cli(
        &["special", "scan", "-", "move", "23", "down", "-o", "-"],
        &icf(&m),
    );
    let mut expected = m.clone();
    expected[a..a + 27].copy_from_slice(&m[b..b + 27]);
    expected[b..b + 27].copy_from_slice(&m[a..a + 27]);
    expected[0x37802] = 24 << 2;
    assert_eq!(result_image(&out), expected);
    let up = cli(
        &["special", "scan", "-", "move", "24", "up", "-o", "-"],
        &out.stdout,
    );
    assert_eq!(result_image(&up), m);
    let out = cli(
        &["special", "scan", "-", "select", "24", "-o", "-"],
        &icf(&m),
    );
    let mut expected = m.clone();
    expected[0x37802] = 24 << 2;
    assert_eq!(result_image(&out), expected);
    let mut g = generation(&m);
    g[0xa60b..0xa60f].fill(255);
    let seed = files.write("g.raw", &g);
    let deleted = cli(
        &[
            "special",
            "scan",
            "-",
            "delete",
            "23",
            "--seed",
            p(&seed),
            "--yes",
            "-o",
            "-",
        ],
        &icf(&m),
    );
    let mut expected = m.clone();
    expected[a..a + 27].copy_from_slice(&m[b..b + 27]);
    expected[b..b + 11].fill(0);
    expected[b + 11..b + 27].fill(b' ');
    expected[0xa60e] |= 1;
    assert_eq!(result_image(&deleted), expected);
    let inserted = cli(
        &[
            "special",
            "scan",
            "-",
            "insert",
            "23",
            "--seed",
            p(&seed),
            "--yes",
            "-o",
            "-",
        ],
        &icf(&m),
    );
    let mut expected = m;
    expected.copy_within(a..a + 27, b);
    expected[a..a + 11].fill(0);
    expected[a + 11..a + 27].fill(b' ');
    expected[0xa60d] |= 0x80;
    expected[0x37802] = 24 << 2;
    assert_eq!(result_image(&inserted), expected);
}

#[test]
fn missing_native_range_context_fails_closed_instead_of_allowing_every_band() {
    let files = Files::new();
    let output = files.write("out", b"must survive");
    let m = image();
    for sheet in ["scan", "user-edge"] {
        for field in ["low-hz=144000000", "high-hz=146000000"] {
            let out = cli(
                &[
                    "special",
                    sheet,
                    "-",
                    "edit",
                    "24",
                    "--set",
                    field,
                    "--output",
                    p(&output),
                    "--overwrite",
                ],
                &icf(&m),
            );
            assert_eq!(out.status.code(), Some(if sheet == "scan" { 3 } else { 2 }));
            assert!(String::from_utf8_lossy(&out.stderr).contains("--range-context"));
            assert!(out.stdout.is_empty());
            assert_eq!(fs::read(&output).unwrap(), b"must survive");
        }
    }
}

#[test]
fn scan_native_frequency_uses_two_pass_ranges_and_original_warning_not_hard_rejection() {
    let files = Files::new();
    let m = image();
    let context = files.write("range.json", &range_context(0, 0, 1, None));
    let out = cli(
        &[
            "special",
            "scan",
            "-",
            "edit",
            "0",
            "--set",
            "low-hz=0",
            "--set",
            "high-hz=200000000",
            "--range-context",
            p(&context),
            "-o",
            "-",
        ],
        &icf(&m),
    );
    let mut expected = m.clone();
    let a = 0xa368;
    expected[a..a + 4].copy_from_slice(&199_999_999u32.to_le_bytes());
    expected[a + 4..a + 8].copy_from_slice(&30_000u32.to_le_bytes());
    expected[a + 8..a + 11].copy_from_slice(&[255, 0, 255]);
    expected[a + 11..a + 27].fill(b' ');
    expected[0xa60b] &= !1;
    assert_eq!(result_image(&out), expected);
    let report: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(report["effects"][1]["warning_bits"], 4);
    assert_eq!(report["frequency_context"]["selected"], 0);
    // B was built from Q.kind0. A later live kind16 bypasses EB64 only; it
    // does not rebuild source0 into a different locale's range generation.
    for (live, warning) in [(0, 4), (16, 0)] {
        let context = files.write(
            &format!("live{live}.json"),
            &range_context(0, live, 1, None),
        );
        let out = cli(
            &[
                "special",
                "scan",
                "-",
                "edit",
                "24",
                "--set",
                "high-hz=147000000",
                "--range-context",
                p(&context),
                "-o",
                "-",
            ],
            &icf(&m),
        );
        let mut expected = m.clone();
        let a = 0xa368 + 27 * 24;
        expected[a..a + 4].copy_from_slice(&147_000_000u32.to_le_bytes());
        assert_eq!(result_image(&out), expected);
        let report: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(report["effects"][0]["warning_bits"], warning);
        assert_eq!(report["frequency_context"]["selected"], 0);
        assert_eq!(report["frequency_context"]["automatic_refresh"], false);
    }
}

#[test]
fn user_native_frequency_writes_last_pair_and_reports_equal_gray_without_a_seed() {
    let files = Files::new();
    let mut m = image();
    m[0x378c0..0x379b0].fill(0);
    let context = files.write("range.json", &range_context(0, 0, 1, None));
    let out = cli(
        &[
            "special",
            "user-edge",
            "-",
            "edit",
            "29",
            "--set",
            "high-hz=145000000",
            "--set",
            "low-hz=144000000",
            "--range-context",
            p(&context),
            "-o",
            "-",
        ],
        &icf(&m),
    );
    let mut expected = m;
    let a = 0x378c0 + 8 * 29;
    expected[a..a + 4].copy_from_slice(&144_000_000u32.to_le_bytes());
    expected[a + 4..a + 8].copy_from_slice(&145_000_000u32.to_le_bytes());
    assert_eq!(result_image(&out), expected);
    let report: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(report["effects"][0]["gray"], true);
    assert_eq!(report["effects"][1]["warning_bits"], 0);
    assert_eq!(report["effects"][1]["callback_flags"], 40);
}

#[test]
fn native_context_stdin_langid_cancel_and_late_g_failure_preserve_outputs() {
    let files = Files::new();
    let output = files.write("out", b"must survive");
    let mut m = image();
    let context = files.write("range.json", &range_context(0, 0, 1, None));
    let duplicate = cli(
        &[
            "special",
            "scan",
            "-",
            "edit",
            "0",
            "--set",
            "low-hz=144000000",
            "--range-context",
            "-",
            "--dry-run",
        ],
        b"",
    );
    assert_eq!(duplicate.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&duplicate.stderr)
            .contains("stdin may be consumed by only one input")
    );
    let cancelled = cli(
        &[
            "special",
            "scan",
            "-",
            "edit",
            "24",
            "--set",
            "high-hz=147000000",
            "--range-context",
            p(&context),
            "--cancel",
            "--output",
            p(&output),
            "--overwrite",
        ],
        &icf(&m),
    );
    success(&cancelled);
    assert!(cancelled.stdout.is_empty());
    assert_eq!(fs::read(&output).unwrap(), b"must survive");
    let missing_langid = files.write("no-lang.json", &range_context(16, 16, 1, None));
    let out = cli(
        &[
            "special",
            "scan",
            "-",
            "edit",
            "0",
            "--set",
            "low-hz=144000000",
            "--range-context",
            p(&missing_langid),
            "--output",
            p(&output),
            "--overwrite",
        ],
        &icf(&m),
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("LANGID source unavailable"));
    assert_eq!(fs::read(&output).unwrap(), b"must survive");
    // One suitable row activates the ORIGINAL conditional G reset after stores.
    m[0xa60b..0xa60f].fill(255);
    m[0x37802] = 0xdf;
    m[0x37807] = 1;
    m[0x37806] = 8;
    m[0x3a8ca] = 0;
    m[0x3a8c9] = 0;
    m[0x3c84a] = 0;
    let out = cli(
        &[
            "special",
            "scan",
            "-",
            "edit",
            "0",
            "--set",
            "low-hz=144000000",
            "--set",
            "high-hz=145000000",
            "--range-context",
            p(&context),
            "--output",
            p(&output),
            "--overwrite",
        ],
        &icf(&m),
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("\"partial_in_memory\": true"));
    assert_eq!(fs::read(&output).unwrap(), b"must survive");
    let mut g = generation(&m);
    g[0x37810..0x37814].copy_from_slice(&[0x11, 0x22, 0x33, 0x44]);
    let seed = files.write("g.raw", &g);
    let out = cli(
        &[
            "special",
            "scan",
            "-",
            "edit",
            "0",
            "--set",
            "low-hz=144000000",
            "--set",
            "high-hz=145000000",
            "--range-context",
            p(&context),
            "--seed",
            p(&seed),
            "-o",
            "-",
        ],
        &icf(&m),
    );
    let mut expected = m;
    expected[0xa368..0xa36c].copy_from_slice(&145_000_000u32.to_le_bytes());
    expected[0xa36c..0xa370].copy_from_slice(&144_000_000u32.to_le_bytes());
    expected[0xa370..0xa373].copy_from_slice(&[255, 0, 255]);
    expected[0xa373..0xa383].fill(b' ');
    expected[0xa60b] &= !1;
    expected[0x37802] = 0x83;
    expected[0x37807] = 0;
    expected[0x37810..0x37814].copy_from_slice(&[0x11, 0x22, 0x33, 0x44]);
    assert_eq!(result_image(&out), expected);
}

#[test]
fn native_context_validation_stdin_and_user_class_warnings_preserve_publication_rules() {
    let files = Files::new();
    let mut m = image();
    m[0x378c0..0x379b0].fill(0);
    let source = files.write("in.icf", &icf(&m));
    let context = range_context(0, 0, 1, None);
    let out = cli(
        &[
            "special",
            "user-edge",
            p(&source),
            "edit",
            "0",
            "--set",
            "low-hz=1800000",
            "--set",
            "high-hz=3500000",
            "--range-context",
            "-",
            "-o",
            "-",
        ],
        &context,
    );
    let mut expected = m.clone();
    expected[0x378c0..0x378c4].copy_from_slice(&1_800_000u32.to_le_bytes());
    expected[0x378c4..0x378c8].copy_from_slice(&3_500_000u32.to_le_bytes());
    assert_eq!(result_image(&out), expected);
    let report: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(report["effects"][1]["warning_bits"], 4); // Both allowed, but different ED70 classes.
    let output = files.write("out", b"must survive");
    let path = files.write("range.json", &context);
    for flag in ["--cancel", "--dry-run"] {
        let out = cli(
            &[
                "special",
                "user-edge",
                p(&source),
                "edit",
                "0",
                "--set",
                "low-hz=2500000",
                "--range-context",
                p(&path),
                flag,
                "--output",
                p(&output),
                "--overwrite",
            ],
            b"",
        );
        success(&out);
        assert!(out.stdout.is_empty());
        assert_eq!(fs::read(&output).unwrap(), b"must survive");
    }
    let out = cli(
        &[
            "special",
            "user-edge",
            p(&source),
            "edit",
            "0",
            "--set",
            "low-hz=2500000",
            "--range-context",
            p(&path),
            "--output",
            p(&output),
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(4));
    assert!(out.stdout.is_empty());
    assert_eq!(fs::read(&output).unwrap(), b"must survive");
    let valid: Value = serde_json::from_slice(&context).unwrap();
    let mut short = valid.clone();
    short["generation_q_hex"] = json!("00");
    let mut unknown = valid.clone();
    unknown["profile"] = json!(0);
    let mut negative_langid = valid.clone();
    negative_langid["system_langid"] = json!(-1);
    let mut negative_kind = valid.clone();
    let mut raw = [0u8; 24];
    raw[..4].fill(255);
    negative_kind["generation_q_hex"] =
        json!(raw.iter().map(|b| format!("{b:02X}")).collect::<String>());
    for (i, bad) in [
        json!([]),
        json!({}),
        short,
        unknown,
        negative_langid,
        negative_kind,
    ]
    .iter()
    .enumerate()
    {
        let path = files.write(&format!("bad{i}.json"), &serde_json::to_vec(bad).unwrap());
        let out = cli(
            &[
                "special",
                "user-edge",
                p(&source),
                "edit",
                "0",
                "--set",
                "low-hz=144000000",
                "--range-context",
                p(&path),
                "--output",
                p(&output),
                "--overwrite",
            ],
            b"",
        );
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
        assert_eq!(fs::read(&output).unwrap(), b"must survive");
    }
}
