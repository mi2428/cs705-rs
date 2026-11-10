//! Native GPS settings process boundaries with independent literal windows.
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
            "gps-settings-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
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
    if let Err(error) = child.stdin.take().unwrap().write_all(input) {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
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
fn image() -> Vec<u8> {
    let mut image = vec![0xa5; 0x3c9c0];
    image[0x355a1] = 6;
    image[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    image
}
fn reports(output: &Output) -> Vec<Value> {
    serde_json::Deserializer::from_slice(&output.stderr)
        .into_iter()
        .map(|value| value.unwrap())
        .collect()
}

#[test]
fn gps_catalog_and_native_choices_execute_literal_stores_cancel_unknown_and_resource_axes() {
    let files = Files::new();
    let raw = image();
    let catalog = cli(&["editor", "gps", "fields"], &[]);
    success(&catalog);
    let fields: Value = serde_json::from_slice(&catalog.stdout).unwrap();
    assert_eq!(fields.as_array().unwrap().len(), 70);
    for (name, at, input, new, flags) in [
        ("gps-select", 0x399e3, "257", 1, 32),
        ("position-ssid", 0x3c675, "42", 42, 0),
        ("object-timestamp", 0x3c780, "{\"action\":\"next\"}", 1, 0),
        ("position-height", 0x3c72e, "8", 8, 0),
        ("position-extension", 0x3c72c, "2", 2, 32),
        ("logger", 0x399f1, "1", 1, 0),
    ] {
        let output = files.path(name);
        let result = cli(
            &[
                "editor",
                "gps",
                "edit",
                "-",
                name,
                "--kind",
                "memory",
                "--value",
                input,
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&result);
        let mut expected = raw.clone();
        expected[at] = new;
        assert_eq!(fs::read(&output).unwrap(), expected);
        assert!(
            reports(&result)
                .iter()
                .any(|value| value["editor"]["notify_flags"] == flags)
        );
        let cancel = files.path(&format!("cancel-{name}"));
        success(&cli(
            &[
                "editor",
                "gps",
                "edit",
                "-",
                name,
                "--kind",
                "memory",
                "--value",
                "{\"action\":\"cancel\"}",
                "-o",
                p(&cancel),
            ],
            &raw,
        ));
        assert!(!cancel.exists());
    }
    let query = cli(
        &[
            "editor",
            "gps",
            "read",
            "-",
            "object-directivity",
            "--kind",
            "memory",
        ],
        &raw,
    );
    success(&query);
    assert_eq!(
        serde_json::from_slice::<Value>(&query.stdout).unwrap()["value"],
        165
    );
    let output = files.path("locale-required");
    let result = cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "object-directivity",
            "--kind",
            "memory",
            "--value",
            "0",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(result.status.code(), Some(3));
    assert!(!output.exists());
    let result = cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "object-directivity",
            "--kind",
            "memory",
            "--language-descriptor",
            "1",
            "--value",
            "0",
            "--dry-run",
        ],
        &raw,
    );
    success(&result);
    assert!(
        reports(&result)
            .iter()
            .any(|value| value["editor"]["selector"] == "GI_Sheet_Sel_Common_PhgdDir_JPN")
    );
}

#[test]
fn gps_choices_keep_no_clobber_dryrun_overwrite_and_reject_bad_event_members() {
    let files = Files::new();
    let raw = image();
    let output = files.path("out");
    fs::write(&output, b"keep").unwrap();
    let mut args = vec![
        "editor",
        "gps",
        "edit",
        "-",
        "tx-format",
        "--kind",
        "memory",
        "--value",
        "3",
        "-o",
        p(&output),
    ];
    assert_eq!(cli(&args, &raw).status.code(), Some(4));
    assert_eq!(fs::read(&output).unwrap(), b"keep");
    args.push("--dry-run");
    success(&cli(&args, &raw));
    assert_eq!(fs::read(&output).unwrap(), b"keep");
    args.pop();
    args.push("--overwrite");
    success(&cli(&args, &raw));
    let mut expected = raw.clone();
    expected[0x3c63c] = 3;
    assert_eq!(fs::read(output).unwrap(), expected);
    let result = cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "gps-select",
            "--kind",
            "memory",
            "--value",
            "\t{\"action\":\"selected\",\"key\":1,\"phase\":2,\"candidate\":1,\"accepted\":true,\"extra\":true}",
            "--dry-run",
        ],
        &raw,
    );
    assert_eq!(result.status.code(), Some(3));
    let action =
        json!({"action":"selected","key":0,"phase":2,"candidate":255,"accepted":true}).to_string();
    let result = cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "gps-select",
            "--kind",
            "memory",
            "--value",
            &action,
            "--dry-run",
        ],
        &raw,
    );
    success(&result);
    assert!(
        reports(&result)
            .iter()
            .any(|value| value["editor"]["stored"] == false)
    );
}

#[test]
fn gps_text_fields_and_all_position_comment_rows_keep_literal_widths_and_budget() {
    let files = Files::new();
    let raw = image();
    for (name, index, at, width) in [
        ("unproto-address", 0, 0x3c63d, 56),
        ("position-comment", 0, 0x3c680, 43),
        ("position-comment", 1, 0x3c6ab, 43),
        ("position-comment", 2, 0x3c6d6, 43),
        ("position-comment", 3, 0x3c701, 43),
        ("object-name", 0, 0x3c733, 9),
        ("object-comment", 0, 0x3c73f, 43),
        ("item-name", 0, 0x3c781, 9),
        ("item-comment", 0, 0x3c78d, 43),
        ("weather-comment", 0, 0x3c7d2, 43),
        ("gps-message", 0, 0x3c628, 20),
    ] {
        let output = files.path(&format!("{name}-{index}"));
        let out = cli(
            &[
                "editor",
                "gps",
                "edit",
                "-",
                name,
                "--index",
                &index.to_string(),
                "--kind",
                "memory",
                "--value",
                "a ! /b ",
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        let mut expected = raw.clone();
        expected[at..at + width].fill(b' ');
        expected[at..at + 6].copy_from_slice(b"a ! /b");
        assert_eq!(fs::read(output).unwrap(), expected);
    }
    let output = files.path("budget43");
    let text = "A".repeat(43);
    let out = cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "position-comment",
            "--index",
            "3",
            "--kind",
            "memory",
            "--value",
            &text,
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&out);
    let mut expected = raw.clone();
    expected[0x3c701..0x3c72c].fill(b'A');
    assert_eq!(fs::read(output).unwrap(), expected);
    assert!(
        reports(&out)
            .iter()
            .any(|value| value["editor"]["budget"] == 28)
    );
    let output = files.path("cancel-text");
    success(&cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "gps-message",
            "--kind",
            "memory",
            "--value",
            "{\"action\":\"cancel\"}",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    assert!(!output.exists());
    let out = cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "position-comment",
            "--index",
            "4",
            "--kind",
            "memory",
            "--value",
            "ABC",
            "--dry-run",
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn gps_explicit_nls_keeps_zero_return_and_distinct_kana_profiles_and_stdin_ownership() {
    let files = Files::new();
    let raw = image();
    let context = files.path("nls");
    fs::write(
        &context,
        serde_json::to_vec(&json!({"lcmap":{"result":0,"output_hex":"41A142202020"}})).unwrap(),
    )
    .unwrap();
    for (name, at, width, prefix) in [
        ("unproto-address", 0x3c63d, 56, b"A\xa1B".as_slice()),
        ("object-name", 0x3c733, 9, b"A B"),
    ] {
        let output = files.path(name);
        success(&cli(
            &[
                "editor",
                "gps",
                "edit",
                "-",
                name,
                "--kind",
                "memory",
                "--value",
                "ABC",
                "--nls-context",
                p(&context),
                "-o",
                p(&output),
            ],
            &raw,
        ));
        let mut expected = raw.clone();
        expected[at..at + width].fill(b' ');
        expected[at..at + 3].copy_from_slice(prefix);
        assert_eq!(fs::read(output).unwrap(), expected);
    }
    let out = cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "gps-message",
            "--kind",
            "memory",
            "--value-file",
            "-",
            "--dry-run",
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("stdin may be consumed by only one input")
    );
    let output = files.path("missing-native");
    let out = cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "unproto-address",
            "--kind",
            "memory",
            "--value",
            "{\"action\":\"text\",\"hex\":\"41A142\"}",
            "--accept-partial",
            "--yes",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
}

#[test]
fn gps_sentence_frames_choose_unchoose_ok_cancel_and_deactivate_have_literal_outputs() {
    let files = Files::new();
    let mut raw = image();
    raw[0x399ea..0x399f0].copy_from_slice(&[0xa5, 0xa4, 0xa4, 0xa4, 0xa4, 0xa5]);
    raw[0x399f3] = 0x81;
    let output = files.path("nmea-same");
    success(&cli(
        &[
            "editor",
            "gps",
            "sentences",
            "-",
            "nmea-sentences",
            "--kind",
            "memory",
            "--step",
            "ok",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    assert_eq!(fs::read(output).unwrap(), raw);
    let output = files.path("logger-same");
    success(&cli(
        &[
            "editor",
            "gps",
            "sentences",
            "-",
            "logger-sentences",
            "--kind",
            "memory",
            "--step",
            "ok",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut expected = raw.clone();
    expected[0x399f3] = 0x80;
    assert_eq!(fs::read(output).unwrap(), expected);
    let output = files.path("nmea-change");
    success(&cli(
        &[
            "editor",
            "gps",
            "sentences",
            "-",
            "nmea-sentences",
            "--kind",
            "memory",
            "--step",
            "unchoose:6",
            "--step",
            "choose:2",
            "--step",
            "ok",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut expected = raw.clone();
    expected[0x399ea..0x399f0].copy_from_slice(&[1, 1, 0, 0, 0, 0]);
    assert_eq!(fs::read(output).unwrap(), expected);
    for step in ["cancel", "close", "owner-destroyed"] {
        let output = files.path(step);
        success(&cli(
            &[
                "editor",
                "gps",
                "sentences",
                "-",
                "nmea-sentences",
                "--kind",
                "memory",
                "--step",
                "choose:2",
                "--step",
                step,
                "-o",
                p(&output),
            ],
            &raw,
        ));
        assert!(!output.exists());
    }
    let output = files.path("deactivate");
    success(&cli(
        &[
            "editor",
            "gps",
            "sentences",
            "-",
            "nmea-sentences",
            "--kind",
            "memory",
            "--step",
            "unchoose:6",
            "--step",
            "close",
            "--step",
            "deactivate",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut expected = raw.clone();
    expected[0x399ea..0x399f0].copy_from_slice(&[1, 0, 0, 0, 0, 0]);
    assert_eq!(fs::read(output).unwrap(), expected);
    let output = files.path("zero");
    let result = cli(
        &[
            "editor",
            "gps",
            "sentences",
            "-",
            "logger-sentences",
            "--kind",
            "memory",
            "--step",
            "unchoose:1",
            "--step",
            "ok",
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&result);
    assert!(!output.exists());
    assert_eq!(reports(&result)[0]["editor"]["done"], true);
    let result = cli(
        &[
            "editor",
            "gps",
            "sentences",
            "-",
            "logger-sentences",
            "--kind",
            "memory",
            "--step",
            "choose:5",
            "--dry-run",
        ],
        &raw,
    );
    assert_eq!(result.status.code(), Some(3));
}

#[test]
fn gs01_sentence_dispatch_done_release_and_rejected_store_are_distinct() {
    let files = Files::new();
    let mut raw = image();
    raw[0x399ea..0x399f0].copy_from_slice(&[0x81, 0x80, 0x80, 0x80, 0x80, 0x80]);
    raw[0x399f3] = 0x81;
    for (name, steps) in [
        ("nmea-sentences", vec!["unchoose:1", "deactivate"]),
        (
            "nmea-sentences",
            vec!["choose:2", "choose:3", "choose:4", "choose:5", "deactivate"],
        ),
        ("logger-sentences", vec!["unchoose:1", "deactivate"]),
    ] {
        let output = files.path(&format!("{name}-{}", steps.len()));
        let mut args = vec![
            "editor",
            "gps",
            "sentences",
            "-",
            name,
            "--kind",
            "memory",
            "-o",
            p(&output),
        ];
        for step in steps {
            args.extend(["--step", step]);
        }
        let out = cli(&args, &raw);
        success(&out);
        assert!(!output.exists());
        let report = &reports(&out)[0]["editor"];
        assert_eq!(report["done"], true);
        assert_eq!(report["release_requested"], true);
        let last = report["steps"].as_array().unwrap().last().unwrap();
        assert_eq!(last["helper_changed"], true);
        assert_eq!(last["stored"], false);
        assert_eq!(last["valid"], false);
    }
    let output = files.path("prefix");
    let args = [
        "editor",
        "gps",
        "sentences",
        "-",
        "nmea-sentences",
        "--kind",
        "memory",
        "--step",
        "choose:2",
        "--step",
        "ok",
        "--step",
        "choose:0",
        "-o",
        p(&output),
    ];
    let out = cli(&args, &raw);
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    assert_eq!(reports(&out)[0]["partial_in_memory"], true);
    let mut partial = args.to_vec();
    partial.extend(["--accept-partial", "--yes"]);
    let out = cli(&partial, &raw);
    assert_eq!(out.status.code(), Some(5));
    let mut expected = raw;
    expected[0x399ea..0x399f0].copy_from_slice(&[1, 1, 0, 0, 0, 0]);
    assert_eq!(fs::read(output).unwrap(), expected);
}

#[test]
fn gps_coordinate_frames_drive_shared_mask_validator_and_all_six_real_targets() {
    let files = Files::new();
    let mut raw = image();
    raw[0x396b4] = 1;
    for (name, at, longitude) in [
        ("manual-latitude", 0x3c613, false),
        ("manual-longitude", 0x3c617, true),
        ("object-latitude", 0x3c76a, false),
        ("object-longitude", 0x3c76e, true),
        ("item-latitude", 0x3c7b8, false),
        ("item-longitude", 0x3c7bc, true),
    ] {
        let output = files.path(name);
        let out = cli(
            &[
                "editor",
                "gps",
                "coordinate",
                "-",
                name,
                "--value",
                if longitude { "1234534W" } else { "351234S" },
                "--current-source",
                "--kind",
                "memory",
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        let mut expected = raw.clone();
        expected[at..at + 4].copy_from_slice(if longitude {
            &[123, 0xb1, 0xfe, b'W']
        } else {
            &[35, 0x31, 0x16, b'S']
        });
        assert_eq!(fs::read(output).unwrap(), expected);
        let report = &reports(&out)[0]["editor"];
        assert_eq!(report["stored"], true);
        assert_eq!(report["dialog"]["done"], true);
        assert_eq!(report["dialog"]["valid"], true);
    }
    let output = files.path("coord-cancel");
    success(&cli(
        &[
            "editor",
            "gps",
            "coordinate",
            "-",
            "manual-latitude",
            "--value",
            "351234S",
            "--event",
            "cancel",
            "--current-source",
            "--kind",
            "memory",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    assert!(!output.exists());
    let output = files.path("coord-escape-deactivate");
    let out = cli(
        &[
            "editor",
            "gps",
            "coordinate",
            "-",
            "manual-latitude",
            "--value",
            "351234S",
            "--event",
            "escape-key-press",
            "--event",
            "deactivate",
            "--current-source",
            "--kind",
            "memory",
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&out);
    let mut expected = raw;
    expected[0x3c613..0x3c617].copy_from_slice(&[35, 0x31, 0x16, b'S']);
    assert_eq!(fs::read(output).unwrap(), expected);
}

#[test]
fn gps_coordinate_direct_phase2_keeps_degree_on_short_text_or_current_and_partial_policy() {
    let files = Files::new();
    let mut raw = image();
    raw[0x396b4] = 1;
    let output = files.path("degree-prefix");
    let args = [
        "editor",
        "gps",
        "coordinate",
        "-",
        "object-latitude",
        "--value",
        r#"{"action":"text","text":"123","phase":2}"#,
        "--current-source",
        "--kind",
        "memory",
        "-o",
        p(&output),
    ];
    let out = cli(&args, &raw);
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    assert_eq!(reports(&out)[0]["partial_in_memory"], true);
    let mut partial = args.to_vec();
    partial.extend(["--accept-partial", "--yes"]);
    let out = cli(&partial, &raw);
    assert_eq!(out.status.code(), Some(5));
    let mut expected = raw.clone();
    expected[0x3c76a] = 123;
    assert_eq!(fs::read(output).unwrap(), expected);
    let current = files.path("short-current");
    fs::write(&current, []).unwrap();
    let output = files.path("current-prefix");
    let out = cli(
        &[
            "editor",
            "gps",
            "coordinate",
            "-",
            "item-longitude",
            "--value",
            r#"{"action":"text","text":"045x12y34zW","phase":2}"#,
            "--current-working",
            p(&current),
            "--kind",
            "memory",
            "--accept-partial",
            "--yes",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(5));
    let mut expected = raw;
    expected[0x3c7bc] = 45;
    assert_eq!(fs::read(output).unwrap(), expected);
    let out = cli(
        &[
            "editor",
            "gps",
            "coordinate",
            "-",
            "manual-latitude",
            "--value-file",
            "-",
            "--current-source",
            "--kind",
            "memory",
            "--dry-run",
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("stdin may be consumed by only one input")
    );
}

#[test]
fn gps_numeric_fields_use_real_wrapper_grammar_units_and_success_only_storage() {
    let files = Files::new();
    let mut raw = image();
    raw[0x396b5] = 0;
    raw[0x396b6] = 2;
    for (name, at, text, value, width) in [
        ("manual-altitude", 0x3c61b, "1", 33_i32, 4),
        ("object-altitude", 0x3c772, "-1", -33, 4),
        ("item-altitude", 0x3c7c0, ".", i32::MAX, 4),
        ("object-course", 0x3c776, "360", 360, 2),
        ("item-course", 0x3c7c4, "1e-3", 1, 2),
        ("object-speed", 0x3c778, "10", 100, 2),
        ("item-speed", 0x3c7c6, "9999", 9990, 2),
    ] {
        let output = files.path(name);
        let out = cli(
            &[
                "editor",
                "gps",
                "edit",
                "-",
                name,
                "--value",
                text,
                "--unit-output-hex",
                "6D",
                "--kind",
                "memory",
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        let mut expected = raw.clone();
        expected[at..at + width].copy_from_slice(&value.to_le_bytes()[..width]);
        assert_eq!(fs::read(output).unwrap(), expected);
        assert_eq!(reports(&out)[0]["editor"]["effect"]["accepted"], true);
    }
    let output = files.path("missing-degree");
    let out = cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "object-course",
            "--value",
            "90",
            "--kind",
            "memory",
            "--accept-partial",
            "--yes",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    assert_eq!(reports(&out)[0]["partial_in_memory"], false);
    let output = files.path("numeric-cancel");
    success(&cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "object-speed",
            "--value",
            r#"{"action":"cancel"}"#,
            "--unit-output-hex",
            "6B74",
            "--kind",
            "memory",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    assert!(!output.exists());
}

#[test]
fn gps_alarm_settings_keep_first_start_empty_flag_same_group_reset_and_row_count_guard() {
    let files = Files::new();
    let mut raw = image();
    raw[0x3c622..0x3c626].copy_from_slice(&[0, 9, 8, 7]);
    raw[0x3c5dd..0x3c613].fill(0xff);
    raw[0x3c385..0x3c5dd].fill(0xff);
    let output = files.path("empty-type3");
    let out = cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "alarm-type",
            "--value",
            "3",
            "--kind",
            "memory",
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&out);
    assert_eq!(fs::read(output).unwrap(), raw);
    assert_eq!(reports(&out)[0]["editor"]["effect"]["notify_flags"], 32);
    raw[0x3c5dd + 10..0x3c5dd + 12].copy_from_slice(&0_u16.to_le_bytes());
    let output = files.path("first-start");
    success(&cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "alarm-type",
            "--value",
            "3",
            "--kind",
            "memory",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut expected = raw.clone();
    expected[0x3c622..0x3c626].copy_from_slice(&[3, 5, 0, 0]);
    assert_eq!(fs::read(output).unwrap(), expected);
    raw[0x3c622..0x3c626].copy_from_slice(&[2, 27, 8, 7]);
    let output = files.path("same-group");
    let out = cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "alarm-group",
            "--value",
            "27",
            "--kind",
            "memory",
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&out);
    let mut expected = raw.clone();
    expected[0x3c624..0x3c626].fill(0);
    assert_eq!(fs::read(output).unwrap(), expected);
    assert_eq!(reports(&out)[0]["editor"]["effect"]["notify_flags"], 0);
    raw[0x3c622..0x3c626].copy_from_slice(&[3, 5, 0, 0]);
    raw[0x3c385..0x3c389].copy_from_slice(&[0, 0, 1, 0]);
    for value in [1_u16, 2] {
        let output = files.path(&format!("alarm-row-{value}"));
        let out = cli(
            &[
                "editor",
                "gps",
                "edit",
                "-",
                "alarm-memory",
                "--value",
                &value.to_string(),
                "--kind",
                "memory",
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        let mut expected = raw.clone();
        if value == 1 {
            expected[0x3c624..0x3c626].copy_from_slice(&value.to_le_bytes());
        }
        assert_eq!(fs::read(output).unwrap(), expected);
        assert_eq!(reports(&out)[0]["editor"]["effect"]["stored"], value == 1);
    }
}

#[test]
fn gps_symbol_menu_and_direct_paths_reach_all_seven_native_pairs_without_raw_setter_packets() {
    let files = Files::new();
    let raw = image();
    let resource = files.path("actual-resource-boundary-fixture");
    let mut pairs = [b'/'; 148];
    for pair in pairs.as_chunks_mut::<2>().0 {
        pair[1] = b'!';
    }
    pairs[6..8].copy_from_slice(b"\\>");
    fs::write(&resource, pairs).unwrap();
    for (name, index, at) in [
        ("position-symbol", 0, 0x3c677),
        ("position-symbol", 1, 0x3c679),
        ("position-symbol", 2, 0x3c67b),
        ("position-symbol", 3, 0x3c67d),
        ("object-symbol", 0, 0x3c73d),
        ("item-symbol", 0, 0x3c78b),
        ("weather-symbol", 0, 0x3c7d0),
    ] {
        let output = files.path(&format!("direct-{name}-{index}"));
        let out = cli(
            &[
                "editor",
                "gps",
                "symbol",
                "-",
                name,
                "--index",
                &index.to_string(),
                "--value",
                "a /tail",
                "--kind",
                "memory",
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        let mut expected = raw.clone();
        expected[at..at + 2].copy_from_slice(b"A/");
        assert_eq!(fs::read(output).unwrap(), expected);
        let output = files.path(&format!("menu-{name}-{index}"));
        success(&cli(
            &[
                "editor",
                "gps",
                "symbol",
                "-",
                name,
                "--index",
                &index.to_string(),
                "--value",
                "3",
                "--menu",
                "--symbol-resource",
                p(&resource),
                "--kind",
                "memory",
                "-o",
                p(&output),
            ],
            &raw,
        ));
        expected[at..at + 2].copy_from_slice(b"\\>");
        assert_eq!(fs::read(output).unwrap(), expected);
    }
    let output = files.path("short-symbol");
    let out = cli(
        &[
            "editor",
            "gps",
            "symbol",
            "-",
            "object-symbol",
            "--value",
            "A",
            "--kind",
            "memory",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    let output = files.path("cancel-symbol");
    success(&cli(
        &[
            "editor",
            "gps",
            "symbol",
            "-",
            "item-symbol",
            "--value",
            r#"{"action":"cancel"}"#,
            "--kind",
            "memory",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    assert!(!output.exists());
    let out = cli(
        &[
            "editor",
            "gps",
            "symbol",
            "-",
            "position-symbol",
            "--index",
            "4",
            "--value",
            "A/",
            "--kind",
            "memory",
            "--dry-run",
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
    let out = cli(
        &[
            "editor",
            "gps",
            "symbol",
            "-",
            "position-symbol",
            "--value",
            "1",
            "--menu",
            "--symbol-resource",
            "-",
            "--kind",
            "memory",
            "--dry-run",
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("stdin may be consumed by only one input")
    );
}

#[test]
fn gps_area_mask_frames_and_explicit_phase2_pack_the_word_without_neighbor_changes() {
    let files = Files::new();
    for (unit, mode, word) in [
        (1, 11, 12566_u16),
        (0, 12, 12340),
        (2, 13, 7404),
        (255, 12, 12340),
    ] {
        let mut raw = image();
        raw[0x396b4] = unit;
        let output = files.path(&format!("area-{unit}"));
        let out = cli(
            &[
                "editor",
                "gps",
                "area",
                "-",
                "alarm-area-group",
                "--value",
                "1234",
                "--current-source",
                "--kind",
                "memory",
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        let mut expected = raw.clone();
        expected[0x3c626..0x3c628].copy_from_slice(&word.to_le_bytes());
        assert_eq!(fs::read(output).unwrap(), expected);
        let report = &reports(&out)[0]["editor"];
        assert_eq!(report["feature_id"], "A2-H55075-10");
        assert_eq!(report["dialog"]["mode"], mode);
        assert_eq!(report["dialog"]["done"], true);
        assert_eq!(report["stored"], true);
    }
    let mut raw = image();
    raw[0x396b4] = 2;
    raw[0x3c626..0x3c628].fill(0);
    let output = files.path("zero-direct");
    success(&cli(
        &[
            "editor",
            "gps",
            "area",
            "-",
            "A2-H55075-10",
            "--value",
            r#"{"action":"text","text":"0.0000","phase":2}"#,
            "--current-source",
            "--kind",
            "memory",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    assert_eq!(fs::read(output).unwrap(), raw);
    let output = files.path("seed-dispatch");
    success(&cli(
        &[
            "editor",
            "gps",
            "area",
            "-",
            "alarm-area-group",
            "--value",
            "0000",
            "--event",
            "deactivate",
            "--current-source",
            "--kind",
            "memory",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut expected = raw.clone();
    expected[0x3c626..0x3c628].copy_from_slice(&84_u16.to_le_bytes());
    assert_eq!(fs::read(output).unwrap(), expected);
    for event in ["cancel", "close"] {
        let output = files.path(event);
        success(&cli(
            &[
                "editor",
                "gps",
                "area",
                "-",
                "alarm-area-group",
                "--value",
                "1234",
                "--event",
                event,
                "--current-source",
                "--kind",
                "memory",
                "-o",
                p(&output),
            ],
            &raw,
        ));
        assert!(!output.exists());
    }
    let output = files.path("escape-deactivate-area");
    success(&cli(
        &[
            "editor",
            "gps",
            "area",
            "-",
            "alarm-area-group",
            "--value",
            "1234",
            "--event",
            "escape-key-press",
            "--event",
            "deactivate",
            "--current-source",
            "--kind",
            "memory",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut expected = raw;
    expected[0x3c626..0x3c628].copy_from_slice(&7404_u16.to_le_bytes());
    assert_eq!(fs::read(output).unwrap(), expected);
}

#[test]
fn gps_area_short_seconds_have_no_publishable_prefix_and_reuse_common_io_policy() {
    let files = Files::new();
    let mut raw = image();
    raw[0x396b4] = 1;
    for partial in [false, true] {
        let output = files.path(&format!("short-{partial}"));
        let mut args = vec![
            "editor",
            "gps",
            "area",
            "-",
            "alarm-area-group",
            "--value",
            r#"{"action":"text","text":"12","phase":2}"#,
            "--current-source",
            "--kind",
            "memory",
            "-o",
            p(&output),
        ];
        if partial {
            args.extend(["--accept-partial", "--yes"]);
        }
        let out = cli(&args, &raw);
        assert_eq!(out.status.code(), Some(3));
        assert!(!output.exists());
        assert_eq!(reports(&out)[0]["partial_in_memory"], false);
    }
    let current = files.path("short-area-current");
    fs::write(&current, []).unwrap();
    let output = files.path("short-current-out");
    let out = cli(
        &[
            "editor",
            "gps",
            "area",
            "-",
            "alarm-area-group",
            "--value",
            r#"{"action":"text","text":"12x34","phase":2}"#,
            "--current-working",
            p(&current),
            "--kind",
            "memory",
            "--accept-partial",
            "--yes",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    assert_eq!(reports(&out)[0]["partial_in_memory"], false);
    let output = files.path("area-existing");
    fs::write(&output, b"keep").unwrap();
    let args = [
        "editor",
        "gps",
        "area",
        "-",
        "alarm-area-group",
        "--value",
        "1234",
        "--current-source",
        "--kind",
        "memory",
        "-o",
        p(&output),
    ];
    assert_eq!(cli(&args, &raw).status.code(), Some(4));
    assert_eq!(fs::read(&output).unwrap(), b"keep");
    let mut dry = args.to_vec();
    dry.push("--dry-run");
    success(&cli(&dry, &raw));
    assert_eq!(fs::read(&output).unwrap(), b"keep");
    let mut overwrite = args.to_vec();
    overwrite.push("--overwrite");
    success(&cli(&overwrite, &raw));
    let mut expected = raw;
    expected[0x3c626..0x3c628].copy_from_slice(&12566_u16.to_le_bytes());
    assert_eq!(fs::read(output).unwrap(), expected);
    let out = cli(
        &[
            "editor",
            "gps",
            "area",
            "-",
            "alarm-area-group",
            "--value-file",
            "-",
            "--current-source",
            "--kind",
            "memory",
            "--dry-run",
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("stdin may be consumed by only one input")
    );
}

#[test]
fn qgps01_rejected_selected_events_do_not_publish_but_approved_no_store_still_does() {
    let files = Files::new();
    let mut raw = image();
    raw[0x3c622..0x3c626].copy_from_slice(&[0, 0, 0, 0]);
    raw[0x3c5dd..0x3c613].fill(0xff);
    raw[0x3c677..0x3c679].copy_from_slice(b"/A");
    let resource = files.path("qgps01-resource");
    fs::write(&resource, b"/A".repeat(74)).unwrap();
    for field in ["position-symbol", "alarm-type"] {
        let invoke = |value: &str, output: &Path| {
            let mut args = vec![
                "editor",
                "gps",
                if field == "position-symbol" {
                    "symbol"
                } else {
                    "edit"
                },
                "-",
                field,
                "--kind",
                "memory",
                "--value",
                value,
                "-o",
                p(output),
            ];
            if field == "position-symbol" {
                args.extend(["--menu", "--symbol-resource", p(&resource)]);
            }
            cli(&args, &raw)
        };
        for (label, value) in [
            (
                "false",
                r#"{"action":"selected","key":1,"phase":2,"candidate":0,"accepted":false}"#,
            ),
            (
                "wrong-key",
                r#"{"action":"selected","key":2,"phase":2,"candidate":0,"accepted":true}"#,
            ),
            (
                "wrong-phase",
                r#"{"action":"selected","key":1,"phase":1,"candidate":0,"accepted":true}"#,
            ),
            ("cancel", r#"{"action":"cancel"}"#),
        ] {
            let output = files.path(&format!("{field}-{label}"));
            let out = invoke(value, &output);
            success(&out);
            assert!(!output.exists());
            let report = &reports(&out)[0];
            assert_eq!(report["approved"], false);
            assert_eq!(report["editor"]["effect"]["stored"], false);
            assert_eq!(report["editor"]["effect"]["notify_flags"], 0);
        }
        for (label, value, flags, stored) in [
            ("display", r#"{"action":"display"}"#.to_owned(), 0, false),
            ("begin", r#"{"action":"begin"}"#.to_owned(), 0, false),
            (
                "same",
                r#"{"action":"selected","key":1,"phase":2,"candidate":0,"accepted":true}"#
                    .to_owned(),
                if field == "position-symbol" { 32 } else { 0 },
                field == "position-symbol",
            ),
            (
                "approved-no-store",
                format!(
                    r#"{{"action":"selected","key":1,"phase":2,"candidate":{},"accepted":true}}"#,
                    if field == "position-symbol" { -1 } else { 3 }
                ),
                32,
                false,
            ),
        ] {
            let output = files.path(&format!("{field}-{label}"));
            let out = invoke(&value, &output);
            success(&out);
            let report = &reports(&out)[0];
            let approved = !matches!(label, "display" | "begin");
            assert_eq!(report["approved"], approved);
            if approved {
                assert_eq!(fs::read(output).unwrap(), raw);
            } else {
                assert!(!output.exists());
            }
            assert_eq!(report["editor"]["effect"]["stored"], stored);
            assert_eq!(report["editor"]["effect"]["notify_flags"], flags);
        }
    }
}

fn gps_rows() -> Vec<u8> {
    let mut raw = image();
    raw[0x3c385..0x3c613].fill(0xff);
    raw[0x3c385..0x3c389].copy_from_slice(&[7, 0, 9, 0]);
    raw[0x3c5e1..0x3c5e3].fill(0);
    raw[0x3c622..0x3c626].fill(0);
    raw[0x396b5] = 0;
    raw[0x396b6] = 2;
    raw[0x396ff..0x39701].copy_from_slice(&60_i16.to_le_bytes());
    raw
}

#[test]
fn r3_gps_live_units_and_degree_use_real_sources_and_no_display_approval() {
    let files = Files::new();
    let raw = gps_rows();
    for (field, text, at, bytes, suffix) in [
        (
            "manual-altitude",
            "100",
            0x3c61b,
            3281_i32.to_le_bytes().to_vec(),
            "6D",
        ),
        (
            "object-speed",
            "10",
            0x3c778,
            100_u16.to_le_bytes().to_vec(),
            "6B74",
        ),
    ] {
        let output = files.path(field);
        let out = cli(
            &[
                "editor",
                "gps",
                "edit",
                "-",
                field,
                "--kind",
                "memory",
                "--value",
                text,
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        let mut expected = raw.clone();
        expected[at..at + bytes.len()].copy_from_slice(&bytes);
        assert_eq!(fs::read(output).unwrap(), expected);
        assert_eq!(
            reports(&out)[0]["editor"]["effect"]["suffix"]["hex"],
            suffix
        );
        for action in ["display", "begin", "cancel"] {
            let output = files.path(&format!("{field}-{action}"));
            let value = json!({"action":action}).to_string();
            let out = cli(
                &[
                    "editor",
                    "gps",
                    "edit",
                    "-",
                    field,
                    "--kind",
                    "memory",
                    "--value",
                    &value,
                    "-o",
                    p(&output),
                ],
                &raw,
            );
            success(&out);
            assert!(!output.exists());
            assert_eq!(reports(&out)[0]["approved"], false);
        }
    }
    for (label, context, suffix) in [
        ("gui-jp", vec!["--language-descriptor", "1"], "DF"),
        (
            "host-jp",
            vec!["--language-descriptor", "0", "--system-langid", "1041"],
            "DF",
        ),
        (
            "host-en",
            vec!["--language-descriptor", "0", "--system-langid", "1033"],
            "B0",
        ),
    ] {
        let output = files.path(label);
        let mut args = vec![
            "editor",
            "gps",
            "edit",
            "-",
            "object-course",
            "--kind",
            "memory",
            "--value",
            "90",
            "-o",
            p(&output),
        ];
        args.extend(context);
        let out = cli(&args, &raw);
        success(&out);
        let mut expected = raw.clone();
        expected[0x3c776..0x3c778].copy_from_slice(&90_u16.to_le_bytes());
        assert_eq!(fs::read(output).unwrap(), expected);
        assert_eq!(
            reports(&out)[0]["editor"]["effect"]["suffix"]["hex"],
            suffix
        );
    }
}

#[test]
fn r3_gps_alarm_all_31_inputs_keep_view_low_words_and_require_actual_approval() {
    let files = Files::new();
    let raw = gps_rows();
    let invoke = |value: &str, output: &Path| {
        cli(
            &[
                "editor",
                "gps",
                "row-alarm",
                "-",
                "2",
                "65539",
                "1",
                "--kind",
                "memory",
                "--value",
                value,
                "-o",
                p(output),
            ],
            &raw,
        )
    };
    for candidate in 0..31 {
        let output = files.path(&format!("alarm-{candidate}"));
        let out = invoke(&candidate.to_string(), &output);
        success(&out);
        let mut expected = raw.clone();
        let tuple = match candidate {
            0 => [0, 0, 0, 0],
            1 => [3, 2, 0xfe, 0xff],
            2 => [1, 0, 0, 0],
            3 => [2, 0, 0, 0],
            _ => [2, candidate as u8 - 3, 0, 0],
        };
        expected[0x3c622..0x3c626].copy_from_slice(&tuple);
        assert_eq!(fs::read(output).unwrap(), expected);
        let report = &reports(&out)[0];
        assert_eq!(report["approved"], true);
        assert_eq!(report["editor"]["choices"].as_array().unwrap().len(), 31);
        assert_eq!(report["editor"]["marked"], matches!(candidate, 1 | 3 | 6));
    }
    for (label, value) in [
        ("display", r#"{"action":"display"}"#),
        ("begin", r#"{"action":"begin"}"#),
        ("cancel", r#"{"action":"cancel"}"#),
        (
            "declined",
            r#"{"action":"selected","key":1,"phase":2,"candidate":1,"accepted":false}"#,
        ),
        (
            "wrong-key",
            r#"{"action":"selected","key":2,"phase":2,"candidate":1,"accepted":true}"#,
        ),
    ] {
        let output = files.path(label);
        let out = invoke(value, &output);
        success(&out);
        assert!(!output.exists());
        assert_eq!(reports(&out)[0]["approved"], false);
    }
}

#[test]
fn r3_gps_native_names_and_altitude_save_opaque_record_bytes_and_empty_group_names() {
    let files = Files::new();
    let raw = gps_rows();
    let target = 0x32c19 + 34 * 7;
    let output = files.path("record-name");
    let out = cli(
        &[
            "editor",
            "gps",
            "record",
            "-",
            "2",
            "0",
            "name",
            "--kind",
            "memory",
            "--value",
            r#"{"action":"text","hex":"41004282A02020"}"#,
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&out);
    let mut expected = raw.clone();
    expected[target + 18..target + 34].fill(b' ');
    expected[target + 18..target + 23].copy_from_slice(b"A B\x82\xa0");
    assert_eq!(fs::read(output).unwrap(), expected);
    let output = files.path("record-altitude");
    let out = cli(
        &[
            "editor",
            "gps",
            "record",
            "-",
            "2",
            "0",
            "altitude",
            "--kind",
            "memory",
            "--value",
            "100",
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&out);
    let mut expected = raw.clone();
    expected[target + 14..target + 18].copy_from_slice(&3281_i32.to_le_bytes());
    assert_eq!(fs::read(output).unwrap(), expected);
    for field in ["name", "altitude"] {
        let output = files.path(&format!("cancel-{field}"));
        success(&cli(
            &[
                "editor",
                "gps",
                "record",
                "-",
                "2",
                "0",
                field,
                "--kind",
                "memory",
                "--value",
                r#"{"action":"cancel"}"#,
                "-o",
                p(&output),
            ],
            &raw,
        ));
        assert!(!output.exists());
    }
    let icf = cs705::icf::Icf::from_memory(raw.clone())
        .unwrap()
        .to_bytes();
    for (label, input, accepted) in [
        ("empty", r#"{"action":"text","text":""}"#, true),
        ("cancel", r#"{"action":"cancel"}"#, false),
        ("begin", r#"{"action":"begin","first":90}"#, false),
    ] {
        let output = files.path(&format!("group-{label}"));
        let out = cli(
            &[
                "group",
                "-",
                "--sheet",
                "gps",
                "rename",
                "26",
                input,
                "-o",
                p(&output),
            ],
            &icf,
        );
        success(&out);
        if accepted {
            let data = fs::read(output).unwrap();
            let image = cs705::icf::Icf::parse(&data).unwrap();
            let mut expected = raw.clone();
            expected[0x353f1 + 25 * 16..0x353f1 + 26 * 16].fill(b' ');
            assert_eq!(image.memory(), expected);
        } else {
            assert!(!output.exists());
        }
    }
}

#[test]
fn r3_gps_clock_insert_clear_delete_and_declined_name_keep_safe_prefix_policy() {
    let files = Files::new();
    let mut raw = gps_rows();
    raw[0x3c385..0x3c613].fill(0xff);
    let icf = cs705::icf::Icf::from_memory(raw.clone())
        .unwrap()
        .to_bytes();
    let q = files.path("q");
    let mut q_bytes = [0_u8; 24];
    q_bytes[..4].copy_from_slice(&1_u32.to_le_bytes());
    fs::write(&q, q_bytes).unwrap();
    let clock = files.path("clock");
    fs::write(&clock,serde_json::to_vec(&json!({"profile":"portable-ascii","native_clock_date":[2024,3,1],"clock":"00:30:40","short_date":"yyyy/MM/dd","date_separator":"/","time_separator":":","rtl_decimal":".","current_year":2024,"two_digit_window":50})).unwrap()).unwrap();
    let output = files.path("insert");
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "gps",
            "--record-context",
            p(&q),
            "--native-format-context",
            p(&clock),
            "--end-index",
            "1",
            "insert",
            "3",
            "0",
            "-o",
            p(&output),
        ],
        &icf,
    );
    success(&out);
    let data = fs::read(output).unwrap();
    let inserted = cs705::icf::Icf::parse(&data).unwrap();
    let mut expected = raw.clone();
    expected[0x3c385..0x3c389].copy_from_slice(&[1, 0, 0, 0]);
    expected[0x3c5e3..0x3c5e5].fill(0);
    let mut seed = [0; 34];
    seed[..8].copy_from_slice(&[0, 0, 0, b'N', 0, 0, 0, b'W']);
    seed[8..14].copy_from_slice(&[0x29, 2, 0x24, 0x23, 0x30, 0x40]);
    seed[14..18].copy_from_slice(&i32::MAX.to_le_bytes());
    seed[18..].fill(b' ');
    for slot in 0..2 {
        expected[0x32c19 + 34 * slot..0x32c19 + 34 * (slot + 1)].copy_from_slice(&seed);
    }
    assert_eq!(inserted.memory(), expected);
    assert_eq!(
        reports(&out)
            .iter()
            .filter(|v| v.get("notification_requested").is_some())
            .count(),
        4
    );
    let output = files.path("clear");
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "gps",
            "--record-context",
            p(&q),
            "--native-format-context",
            p(&clock),
            "clear",
            "3",
            "0",
            "--yes",
            "-o",
            p(&output),
        ],
        &data,
    );
    success(&out);
    let cleared = fs::read(output).unwrap();
    assert_eq!(cs705::icf::Icf::parse(&cleared).unwrap().memory(), expected);
    let output = files.path("delete");
    success(&cli(
        &[
            "row",
            "-",
            "--sheet",
            "gps",
            "--end-index",
            "1",
            "delete",
            "3",
            "0",
            "--yes",
            "-o",
            p(&output),
        ],
        &data,
    ));
    let deleted = fs::read(output).unwrap();
    expected[0x3c385..0x3c613].fill(0xff);
    assert_eq!(cs705::icf::Icf::parse(&deleted).unwrap().memory(), expected);
    for partial in [false, true] {
        let output = files.path(&format!("missing-clock-{partial}"));
        let mut args = vec!["row", "-", "--sheet", "gps", "--record-context", p(&q)];
        if partial {
            args.push("--accept-partial");
        }
        args.extend(["insert", "3", "0", "--yes", "-o", p(&output)]);
        let out = cli(&args, &icf);
        assert_eq!(out.status.code(), Some(if partial { 5 } else { 3 }));
        assert!(reports(&out).iter().any(|v| v["partial_in_memory"] == true));
        if partial {
            let data = fs::read(output).unwrap();
            let mut expected = raw.clone();
            expected[0x3c385..0x3c387].fill(0);
            expected[0x3c5e3..0x3c5e5].fill(0);
            assert_eq!(cs705::icf::Icf::parse(&data).unwrap().memory(), expected);
        } else {
            assert!(!output.exists());
        }
    }
    let output = files.path("cancel-empty-name");
    let out = cli(
        &[
            "editor",
            "gps",
            "record",
            "-",
            "3",
            "0",
            "name",
            "--kind",
            "memory",
            "--record-context",
            p(&q),
            "--native-format-context",
            p(&clock),
            "--value",
            r#"{"action":"cancel"}"#,
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&out);
    assert!(!output.exists());
    assert_eq!(reports(&out).last().unwrap()["published"], false);
    assert!(
        reports(&out)
            .iter()
            .any(|v| v["partial_in_memory"] == true && v["approved"] == false)
    );
}

#[test]
fn r3_gps_modal_approval_is_accumulated_without_publishing_inactive_choices() {
    let files = Files::new();
    let mut raw = gps_rows();
    raw[0x396b4] = 1;
    raw[0x399ea..0x399f0].copy_from_slice(&[0x81, 0x80, 0x80, 0x80, 0x80, 0x80]);
    let output = files.path("coordinate-accepted-then-cancelled");
    let out = cli(
        &[
            "editor",
            "gps",
            "coordinate",
            "-",
            "manual-latitude",
            "--kind",
            "memory",
            "--value",
            "351234S",
            "--current-source",
            "--event",
            "accept",
            "--event",
            "cancel",
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&out);
    let mut expected = raw.clone();
    expected[0x3c613..0x3c617].copy_from_slice(&[35, 0x31, 0x16, b'S']);
    assert_eq!(fs::read(output).unwrap(), expected);
    assert_eq!(reports(&out)[0]["approved"], true);
    let output = files.path("sentence-accepted-then-cancelled");
    let out = cli(
        &[
            "editor",
            "gps",
            "sentences",
            "-",
            "nmea-sentences",
            "--kind",
            "memory",
            "--step",
            "choose:2",
            "--step",
            "ok",
            "--step",
            "cancel",
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&out);
    let mut expected = raw.clone();
    expected[0x399ea..0x399f0].copy_from_slice(&[1, 1, 0, 0, 0, 0]);
    assert_eq!(fs::read(output).unwrap(), expected);
    assert_eq!(reports(&out)[0]["approved"], true);
    let output = files.path("inactive-group");
    let out = cli(
        &[
            "editor",
            "gps",
            "edit",
            "-",
            "alarm-group",
            "--kind",
            "memory",
            "--value",
            r#"{"action":"next"}"#,
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&out);
    assert!(!output.exists());
    assert_eq!(reports(&out)[0]["approved"], false);
}
