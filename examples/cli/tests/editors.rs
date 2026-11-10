//! Process checks for native editors, literal byte windows and publication policy.
use cs705::icf::context::Context;
use serde_json::json;
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
            "editors-{}-{}",
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
        let path = self.path(name);
        fs::write(&path, bytes).unwrap();
        path
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
fn success(out: &Output) {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn reports(out: &Output) -> Vec<serde_json::Value> {
    serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter()
        .map(|value| value.unwrap())
        .collect()
}
fn image() -> Vec<u8> {
    let mut raw = vec![0xa5; 0x3c9c0];
    raw[0x355a1] = 6;
    raw[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    raw
}
fn normal() -> Vec<u8> {
    let mut raw = vec![0; 0x4997a];
    raw[0x3662d..0x3662d + 500].fill(1);
    for cell in raw[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
        cell.copy_from_slice(&65535_u32.to_le_bytes());
    }
    raw
}
fn context() -> Vec<u8> {
    let q = Context::before_registry();
    let hex: String = q.as_bytes().iter().map(|b| format!("{b:02X}")).collect();
    serde_json::to_vec(&json!({"generation_q_hex":hex,"live_q_hex":hex,"system_langid":1041}))
        .unwrap()
}

#[test]
fn r3_settings_unapproved_actions_never_publish_repairs_or_clobber_outputs() {
    let files = Files::new();
    let mut raw = image();
    raw[0x37850] = 9;
    raw[0x37867] = 127;
    raw[0x37158..0x3715a].copy_from_slice(b"**");
    for prefix in [
        vec!["bw", "-", "0"],
        vec!["pbt", "-", "0", "one"],
        vec!["text", "-", "keyer", "0"],
        vec!["text", "-", "user1-password", "0"],
        vec!["rf", "-", "0", "preamp"],
        vec!["auto-repeater", "-", "0", "dup"],
        vec!["dr", "-", "digital-code"],
        vec!["program-scan", "-", "--current-source"],
    ] {
        for action in ["begin", "display", "cancel"] {
            let output = files.write("unapproved", b"keep");
            let value = json!({"action":action}).to_string();
            let mut args = vec!["editor"];
            args.extend(prefix.iter().copied());
            args.extend([
                "--kind",
                "memory",
                "--value",
                &value,
                "-o",
                p(&output),
                "--overwrite",
                "--accept-partial",
                "--yes",
            ]);
            let out = cli(&args, &raw);
            success(&out);
            assert!(out.stdout.is_empty());
            assert_eq!(fs::read(&output).unwrap(), b"keep", "{prefix:?}/{action}");
            assert!(reports(&out).iter().any(|r| r["approved"] == false));
        }
    }
    for event in [
        json!({"action":"selected","key":1,"phase":2,"candidate":100,"accepted":false}),
        json!({"action":"selected","key":2,"phase":2,"candidate":100,"accepted":true}),
        json!({"action":"selected","key":1,"phase":1,"candidate":100,"accepted":true}),
    ] {
        let output = files.path("rejected");
        let out = cli(
            &[
                "editor",
                "pbt",
                "-",
                "0",
                "one",
                "--kind",
                "memory",
                "--value",
                &event.to_string(),
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        assert!(!output.exists());
        assert!(
            reports(&out)
                .iter()
                .any(|r| r["partial_in_memory"] == true && r["approved"] == false)
        );
    }
}

#[test]
fn r3_settings_wlan_password_rejections_empty_and_same_approval_are_safe() {
    let files = Files::new();
    for (field, at) in [("user1-password", 0x39393), ("user2-password", 0x393b4)] {
        for text in ["short", "aaaaaaaa", "a a a a a a a a"] {
            let output = files.write("password", b"keep");
            let out = cli(
                &[
                    "editor",
                    "text",
                    "-",
                    field,
                    "0",
                    "--kind",
                    "memory",
                    "--value",
                    text,
                    "-o",
                    p(&output),
                    "--overwrite",
                ],
                &image(),
            );
            assert_eq!(out.status.code(), Some(3));
            assert_eq!(fs::read(&output).unwrap(), b"keep");
            assert!(!String::from_utf8_lossy(&out.stderr).contains(text));
        }
        let mut raw = image();
        raw[at..at + 16].fill(b' ');
        let output = files.path(&format!("empty-{field}"));
        let out = cli(
            &[
                "editor",
                "text",
                "-",
                field,
                "0",
                "--kind",
                "memory",
                "--value",
                "",
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        assert_eq!(fs::read(output).unwrap(), raw);
        assert!(reports(&out).iter().any(|r| r["approved"] == true));
    }
}

#[test]
fn r3_settings_conditional_language_sources_follow_short_circuit_branches() {
    let files = Files::new();
    for selector in [1_u8, 16] {
        let mut q = [0_u8; 24];
        q[0] = selector;
        let hex = q.iter().map(|b| format!("{b:02X}")).collect::<String>();
        let context = files.write(
            &format!("no-system-{selector}"),
            &serde_json::to_vec(&json!({"generation_q_hex":hex,"live_q_hex":hex})).unwrap(),
        );
        let key_output = files.path(&format!("key-{selector}"));
        success(&cli(
            &[
                "editor",
                "key",
                "-",
                "0",
                "microphone_keys",
                "--range-context",
                p(&context),
                "--kind",
                "memory",
                "--value",
                "0",
                "-o",
                p(&key_output),
            ],
            &image(),
        ));
        assert!(key_output.exists());
        let rf_output = files.path(&format!("rf-{selector}"));
        success(&cli(
            &[
                "editor",
                "rf",
                "-",
                "0",
                "preamp",
                "--resource-context",
                p(&context),
                "--kind",
                "memory",
                "--value",
                "0",
                "-o",
                p(&rf_output),
            ],
            &image(),
        ));
        assert!(rf_output.exists());
        for (field, descriptor, approved) in [
            ("coordinates", Some("1"), true),
            ("temperature", Some("1"), true),
            ("auto-repeater", None, selector != 16),
            ("coordinates", Some("2"), false),
            ("scope-span", None, false),
        ] {
            let output = files.path(&format!("{selector}-{field}-{descriptor:?}"));
            let mut args = vec![
                "editor",
                "conditional",
                "-",
                field,
                "--range-context",
                p(&context),
                "--kind",
                "memory",
                "--value",
                "0",
                "-o",
                p(&output),
            ];
            if let Some(descriptor) = descriptor {
                args.extend(["--language-descriptor", descriptor]);
            }
            let out = cli(&args, &image());
            if approved {
                success(&out);
                assert!(output.exists());
            } else {
                assert_eq!(out.status.code(), Some(3));
                assert!(!output.exists());
            }
        }
    }
}

#[test]
fn auto_repeater_edges_and_dup_use_literal_rows_shared_codec_and_public_effects() {
    let files = Files::new();
    let mut q = [0; 24];
    q[0] = 1;
    q[0x13] = 1;
    let hex: String = q.iter().map(|b| format!("{b:02X}")).collect();
    let context = files.write(
        "auto-context",
        &serde_json::to_vec(&json!({"generation_q_hex":hex,"live_q_hex":hex})).unwrap(),
    );
    for (row, text, hz) in [
        (0, "1.25", 1_250_000_u32),
        (2, "29.999999", 29_999_999),
        (3, "31", 31_000_000),
        (8, "145", 145_000_000),
        (9, "433", 433_000_000),
        (11, "470", 470_000_000),
    ] {
        let mut raw = image();
        let at = 0x370e0 + 10 * row;
        raw[at] = 0;
        let output = files.path(&format!("auto-{row}"));
        let out = cli(
            &[
                "editor",
                "auto-repeater",
                "-",
                &row.to_string(),
                "high",
                "--value",
                text,
                "--range-context",
                p(&context),
                "--kind",
                "memory",
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        let mut expected = raw.clone();
        expected[at] = 1;
        expected[at + 2..at + 6].copy_from_slice(&hz.to_le_bytes());
        expected[at + 6..at + 10].copy_from_slice(&hz.to_le_bytes());
        assert_eq!(fs::read(output).unwrap(), expected);
        let report = &reports(&out)[0]["editor"];
        assert_eq!(report["feature_id"], "A2-O076");
        assert_eq!(report["effect"]["notify_flags"], 32);
        assert_eq!(report["effect"]["gray"], true);
        if row == 3 {
            assert_eq!(report["resource_visible"], false);
        }
    }
    let mut raw = image();
    raw[0x370e0] = 2;
    raw[0x370e2..0x370e6].copy_from_slice(&2_000_000_u32.to_le_bytes());
    raw[0x370e6..0x370ea].copy_from_slice(&3_000_000_u32.to_le_bytes());
    let output = files.path("inverted");
    let out = cli(
        &[
            "editor",
            "auto-repeater",
            "-",
            "0",
            "high",
            "--value",
            "1",
            "--range-context",
            p(&context),
            "--kind",
            "memory",
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&out);
    let mut expected = raw.clone();
    expected[0x370e6..0x370ea].copy_from_slice(&1_000_000_u32.to_le_bytes());
    assert_eq!(fs::read(output).unwrap(), expected);
    assert_eq!(reports(&out)[0]["editor"]["effect"]["warning_bits"], 4);
    for candidate in [2, 257, -1, 256] {
        let output = files.path(&format!("dup-{candidate}"));
        let out = cli(
            &[
                "editor",
                "auto-repeater",
                "-",
                "0",
                "dup",
                "--value",
                &candidate.to_string(),
                "--kind",
                "memory",
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        let mut expected = raw.clone();
        expected[0x370e0] = candidate as u8;
        assert_eq!(fs::read(output).unwrap(), expected);
        let report = &reports(&out)[0]["editor"];
        assert_eq!(report["feature_id"], "A2-O077");
        assert_eq!(report["effect"]["candidate"], candidate);
        assert_eq!(report["effect"]["notify_flags"], 0);
    }
    for missing in [0, 2, 6] {
        let mut raw = raw.clone();
        let at = 0x370e0 + missing;
        if missing == 0 {
            raw[at] = 0;
        } else {
            raw[at..at + 4].fill(0);
        }
        let out = cli(
            &[
                "editor",
                "auto-repeater",
                "-",
                "0",
                "dup",
                "--value",
                "2",
                "--native-code",
                "28928",
                "--kind",
                "memory",
                "--dry-run",
            ],
            &raw,
        );
        success(&out);
        let effect = &reports(&out)[0]["editor"]["effect"];
        assert_eq!(effect["stored"], false);
        assert_eq!(effect["move_to_low"], true);
        assert_eq!(effect["redispatch_code"], 28928);
        assert_eq!(effect["parent_requests_executed"], false);
    }
}

#[test]
fn auto_repeater_cancel_begin_same_value_and_publication_keep_existing_policy() {
    let files = Files::new();
    let context = files.write("context", &context());
    let mut raw = image();
    raw[0x370e0] = 2;
    raw[0x370e2..0x370e6].copy_from_slice(&1_000_000_u32.to_le_bytes());
    raw[0x370e6..0x370ea].copy_from_slice(&2_000_000_u32.to_le_bytes());
    let output = files.path("cancel");
    let out = cli(
        &[
            "editor",
            "auto-repeater",
            "-",
            "0",
            "low",
            "--value",
            "{\"action\":\"cancel\"}",
            "--range-context",
            p(&context),
            "--kind",
            "memory",
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&out);
    assert!(!output.exists());
    for (field, key) in [("low", "state_out"), ("dup", "popup_requested")] {
        let mut args = vec![
            "editor",
            "auto-repeater",
            "-",
            "0",
            field,
            "--value",
            "{\"action\":\"begin\"}",
            "--kind",
            "memory",
            "--dry-run",
        ];
        if field == "low" {
            args.extend(["--range-context", p(&context)]);
        }
        let out = cli(&args, &raw);
        success(&out);
        assert_eq!(
            reports(&out)[0]["editor"]["effect"][key],
            if field == "low" {
                json!(2)
            } else {
                json!(true)
            }
        );
    }
    let output = files.write("exists", b"keep");
    let args = [
        "editor",
        "auto-repeater",
        "-",
        "0",
        "low",
        "--value",
        "1",
        "--range-context",
        p(&context),
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
    let out = cli(&overwrite, &raw);
    success(&out);
    assert_eq!(fs::read(&output).unwrap(), raw);
    assert_eq!(reports(&out)[0]["editor"]["effect"]["stored"], true);
    for value in ["abc", "{\"action\":\"text\",\"text\":\"1\",\"extra\":0}"] {
        let out = cli(
            &[
                "editor",
                "auto-repeater",
                "-",
                "0",
                "low",
                "--value",
                value,
                "--range-context",
                p(&context),
                "--kind",
                "memory",
                "--dry-run",
            ],
            &raw,
        );
        assert_eq!(out.status.code(), Some(3));
    }
    let out = cli(
        &[
            "editor",
            "auto-repeater",
            "-",
            "0",
            "low",
            "--value-file",
            "-",
            "--range-context",
            p(&context),
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
fn auto_repeater_late_warning_source_failure_has_real_prefix_but_no_default_publication() {
    let files = Files::new();
    let mut generation = [0; 24];
    generation[0] = 1;
    let mut live = [0; 24];
    live[0] = 16;
    let hex = |q: &[u8]| q.iter().map(|b| format!("{b:02X}")).collect::<String>();
    let context = files.write(
        "late-context",
        &serde_json::to_vec(&json!({"generation_q_hex":hex(&generation),"live_q_hex":hex(&live)}))
            .unwrap(),
    );
    let mut raw = image();
    raw[0x370e0] = 0;
    let output = files.path("late-prefix");
    let args = [
        "editor",
        "auto-repeater",
        "-",
        "0",
        "low",
        "--value",
        "1.25",
        "--range-context",
        p(&context),
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
    expected[0x370e0] = 1;
    expected[0x370e2..0x370e6].copy_from_slice(&1_250_000_u32.to_le_bytes());
    expected[0x370e6..0x370ea].copy_from_slice(&1_250_000_u32.to_le_bytes());
    assert_eq!(fs::read(output).unwrap(), expected);
    let short = files.write(
        "short-context",
        br#"{"generation_q_hex":"01","live_q_hex":"01"}"#,
    );
    let out = cli(
        &[
            "editor",
            "auto-repeater",
            "-",
            "0",
            "low",
            "--value",
            "1",
            "--range-context",
            p(&short),
            "--kind",
            "memory",
            "--dry-run",
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn keys_conditional_and_rf_preserve_native_slots_raw_bypass_and_unknown_replacement() {
    let files = Files::new();
    let context = files.write("context", &context());
    for (name, indices) in [
        ("microphone_keys", vec![0, 1, 2, 3]),
        ("bluetooth_keys", vec![5, 6, 7]),
    ] {
        for index in indices {
            let mut raw = image();
            raw[0x3970a + index] = 23;
            let output = files.path(&format!("{name}-{index}"));
            let out = cli(
                &[
                    "editor",
                    "key",
                    "-",
                    &index.to_string(),
                    name,
                    "--range-context",
                    p(&context),
                    "--kind",
                    "memory",
                    "--value",
                    "{\"action\":\"next\"}",
                    "-o",
                    p(&output),
                ],
                &raw,
            );
            success(&out);
            let mut expected = raw.clone();
            expected[0x3970a + index] = 25;
            assert_eq!(fs::read(output).unwrap(), expected);
            let output = files.path(&format!("bypass-{name}-{index}"));
            success(&cli(
                &[
                    "editor",
                    "key",
                    "-",
                    &index.to_string(),
                    name,
                    "--range-context",
                    p(&context),
                    "--kind",
                    "memory",
                    "--value",
                    "24",
                    "-o",
                    p(&output),
                ],
                &raw,
            ));
            expected[0x3970a + index] = 24;
            assert_eq!(fs::read(output).unwrap(), expected);
        }
    }
    for (field, at) in [
        ("coordinates", 0x396b4),
        ("temperature", 0x396b7),
        ("auto-repeater", 0x39312),
        ("scope-span", 0x377c7),
    ] {
        let raw = image();
        let output = files.path(field);
        success(&cli(
            &[
                "editor",
                "conditional",
                "-",
                field,
                "--range-context",
                p(&context),
                "--language-descriptor",
                "2",
                "--kind",
                "memory",
                "--value",
                "254",
                "-o",
                p(&output),
            ],
            &raw,
        ));
        let mut expected = raw;
        expected[at] = 254;
        assert_eq!(fs::read(output).unwrap(), expected);
    }
    let mut raw = image();
    raw[0x39312] = 0;
    let output = files.path("jpn-next");
    let out = cli(
        &[
            "editor",
            "conditional",
            "-",
            "auto-repeater",
            "--range-context",
            p(&context),
            "--kind",
            "memory",
            "--value",
            "{\"action\":\"next\"}",
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&out);
    let mut expected = raw;
    expected[0x39312] = 2;
    assert_eq!(fs::read(output).unwrap(), expected);
    assert!(String::from_utf8_lossy(&out.stderr).contains("AutoRepeater_JPN"));
    for field in ["preamp", "attenuator"] {
        for index in 0..17 {
            let raw = image();
            let output = files.path(&format!("{field}-{index}"));
            success(&cli(
                &[
                    "editor",
                    "rf",
                    "-",
                    &index.to_string(),
                    field,
                    "--kind",
                    "memory",
                    "--value",
                    "-1",
                    "-o",
                    p(&output),
                ],
                &raw,
            ));
            let mut expected = raw;
            if field == "preamp" {
                expected[0x378af + index] |= 3;
            } else if index < 13 {
                expected[0x378af + index] |= 4;
            }
            if field == "attenuator" && index >= 13 {
                assert!(!output.exists());
            } else {
                assert_eq!(fs::read(output).unwrap(), expected);
            }
        }
    }
    let out = cli(
        &[
            "editor",
            "key",
            "-",
            "4",
            "microphone_keys",
            "--range-context",
            p(&context),
            "--kind",
            "memory",
            "--value",
            "0",
            "--dry-run",
        ],
        &image(),
    );
    assert_eq!(out.status.code(), Some(3));
    let out = cli(
        &[
            "editor",
            "rf",
            "-",
            "17",
            "preamp",
            "--kind",
            "memory",
            "--value",
            "0",
            "--dry-run",
        ],
        &image(),
    );
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn all_fixed_rows_both_sides_publish_and_pbt_display_repairs_stay_private() {
    use cs705::icf::frequency::Ranges;
    let files = Files::new();
    let context = files.write("context", &context());
    let q = Context::before_registry();
    let ranges = Ranges::generate(&q, Some(0x411)).unwrap();
    for index in 0..68 {
        let hz = ranges.f760(0x16, (index / 4) as i32, 0).low + 10_000;
        for (side, value) in [("low", hz), ("high", hz + 5_000)] {
            let mut raw = image();
            let at = 0x3973a + 8 * index;
            raw[at..at + 4].copy_from_slice(&hz.to_le_bytes());
            raw[at + 4..at + 8].copy_from_slice(&u32::MAX.to_le_bytes());
            let output = files.path(&format!("fix-{index}-{side}"));
            let text = format!("{}.{:06}", value / 1_000_000, value % 1_000_000);
            success(&cli(
                &[
                    "editor",
                    "fix",
                    "-",
                    &index.to_string(),
                    side,
                    "--kind",
                    "memory",
                    "--range-context",
                    p(&context),
                    "--value",
                    &text,
                    "-o",
                    p(&output),
                ],
                &raw,
            ));
            let mut expected = raw;
            let high = if side == "low" { hz + 1_000_000 } else { value };
            expected[at + 4..at + 8].copy_from_slice(&high.to_le_bytes());
            assert_eq!(fs::read(output).unwrap(), expected, "FIX{index}/{side}");
        }
    }
    for index in 0..15 {
        for (side, delta) in [("one", 0), ("two", 1)] {
            let mut raw = image();
            raw[0x37850 + index] = 9;
            let at = 0x37867 + 2 * index + delta;
            raw[at] = 127;
            let output = files.path(&format!("pbt-{index}-{side}"));
            success(&cli(
                &[
                    "editor",
                    "pbt",
                    "-",
                    &index.to_string(),
                    side,
                    "--kind",
                    "memory",
                    "--value",
                    "{\"action\":\"display\"}",
                    "-o",
                    p(&output),
                ],
                &raw,
            ));
            assert!(!output.exists());
        }
    }
}

#[test]
fn dr_real_sync_uses_kind1b_and_digital_code_cleanup_without_default_sources() {
    use cs705::icf::special::CompactChannel;
    let files = Files::new();
    for (field, setting, mode, value) in [
        ("dv-filter", 0x3c383, 0x10, "2"),
        ("fm-filter", 0x3c384, 0x0d, "2"),
        ("digital-squelch", 0x3c8b5, 0x10, "2"),
        ("digital-code", 0x3c8b6, 0x10, "12"),
    ] {
        let mut raw = image();
        let mut record = [0; 36];
        let mut h = CompactChannel::decode_bound(Some(&record), true).unwrap();
        h.frequency_hz = 14_200_000;
        h.mode = mode;
        h.write_to(&[0; 36], |at, bytes| {
            record[at..at + bytes.len()].copy_from_slice(bytes);
            Ok(())
        })
        .unwrap();
        raw[0x3c801..0x3c825].copy_from_slice(&record);
        raw[0x3a8ca] = 0;
        raw[0x3a8c9] = 1;
        raw[0x3c800] = 0;
        let output = files.path(field);
        let input = if field == "digital-code" {
            value.into()
        } else {
            json!({"action":"selected","key":1,"phase":2,"candidate":2,"accepted":true}).to_string()
        };
        let out = cli(
            &[
                "editor",
                "dr",
                "-",
                field,
                "--kind",
                "memory",
                "--value",
                &input,
                "--node-flags",
                "167",
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        let mut expected = raw;
        expected[setting] = if field == "digital-code" {
            12
        } else if field == "digital-squelch" {
            0xa6
        } else {
            2
        };
        if field.ends_with("filter") {
            expected[0x3c805] = (expected[0x3c805] & 0xf0) | 2;
            expected[0x3c8b4] = 2;
        }
        if field == "digital-squelch" {
            expected[0x3c806] = (expected[0x3c806] & 0xcf) | 0x20;
        }
        if field == "digital-code" {
            expected[0x3c80f] = 12;
        }
        let actual = fs::read(output).unwrap();
        let differences = actual
            .iter()
            .zip(&expected)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(at, (a, b))| (format!("{at:X}"), *a, *b))
            .collect::<Vec<_>>();
        assert!(actual == expected, "{field}: {differences:?}");
        assert!(
            reports(&out)
                .iter()
                .any(|value| value["editor"]["synchronized"] == true)
        );
        if field == "digital-code" {
            assert!(
                reports(&out)
                    .iter()
                    .any(|value| value["node_flags_after"] == 164)
            );
        }
    }
}

#[test]
fn home_mr_and_dr_keep_source_roles_typed_coordinates_cancel_and_warning_bits() {
    use cs705::{
        icf::{
            record::{self, ChannelField as C, RepeaterField as R},
            record_image::{self, ChannelKind},
        },
        repeater::working,
    };
    let files = Files::new();
    let context = files.write("context", &context());
    let mut current = normal();
    current[0x3fcd4..0x3fcd8].copy_from_slice(&0_u32.to_le_bytes());
    let mut holder = record::reset_holder(2, 0, 1);
    holder.set_channel_field(C::FrequencyHz, 14_200_000);
    holder.set_split(0);
    record_image::store_channel_into(
        &mut current,
        ChannelKind::Normal,
        0,
        &mut holder,
        |_| Ok(()),
    )
    .unwrap();
    let current = files.write("current", &current);
    let mut raw = image();
    raw[0x392fa] = 0;
    raw[0x392fb..0x392fd].copy_from_slice(&1_u16.to_le_bytes());
    let output = files.path("mr");
    success(&cli(
        &[
            "editor",
            "home-channel",
            "-",
            "--kind",
            "memory",
            "--current-working",
            p(&current),
            "--range-context",
            p(&context),
            "--show-both",
            "0",
            "--value",
            "0x0",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut expected = raw.clone();
    expected[0x392fb..0x392fd].fill(0);
    assert_eq!(fs::read(output).unwrap(), expected);
    let output = files.path("mr-cancel");
    success(&cli(
        &[
            "editor",
            "home-channel",
            "-",
            "--kind",
            "memory",
            "--current-working",
            p(&current),
            "--range-context",
            p(&context),
            "--show-both",
            "0",
            "--value",
            "{\"action\":\"cancel\"}",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    assert!(!output.exists());
    let mut source = vec![0; 0x4997a];
    source[0x49914..0x49916].copy_from_slice(&2_u16.to_le_bytes());
    source[0x4991a..0x4991c].copy_from_slice(&1_u16.to_le_bytes());
    for (index, hz) in [(0, 10_000), (1, 14_200_000), (2, 14_200_000)] {
        let mut h = record::reset_holder(0x0c, index, 1);
        h.set_channel_field(C::FrequencyHz, hz);
        h.set_channel_field(C::Duplex, 1);
        h.set_repeater_field(R::Mode, 0);
        h.set_repeater_field(R::Use, 1);
        working::save_into(&mut source, index as usize, &h).unwrap();
    }
    let source = files.write("source", &source);
    raw[0x392fd..0x392ff].copy_from_slice(&2_u16.to_le_bytes());
    let output = files.path("dr-group");
    success(&cli(
        &[
            "editor",
            "home-dr-group",
            "-",
            "--kind",
            "memory",
            "--source-working",
            p(&source),
            "--range-context",
            p(&context),
            "--value",
            "1",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut expected = raw.clone();
    expected[0x392fd..0x392ff].copy_from_slice(&1_u16.to_le_bytes());
    assert_eq!(fs::read(output).unwrap(), expected);
    let output = files.path("dr-text-rejected");
    let out = cli(
        &[
            "editor",
            "home-dr-memory",
            "-",
            "--kind",
            "memory",
            "--source-working",
            p(&source),
            "--current-source",
            "--range-context",
            p(&context),
            "--value",
            " x0.0 ",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    assert!(String::from_utf8_lossy(&out.stderr).contains("3030"));
    let output = files.path("dr-same");
    let out = cli(
        &[
            "editor",
            "home-dr-memory",
            "-",
            "--kind",
            "memory",
            "--source-working",
            p(&source),
            "--current-source",
            "--range-context",
            p(&context),
            "--warning-bits",
            "8",
            "--value",
            "{\"action\":\"selected\",\"key\":19,\"phase\":2,\"candidate\":0,\"accepted\":true}",
            "-o",
            p(&output),
        ],
        &raw,
    );
    success(&out);
    assert_eq!(fs::read(output).unwrap(), raw);
    assert!(
        reports(&out)
            .iter()
            .any(|value| value["editor"]["home"]["notify_flags"] == 32)
    );
    let out = cli(
        &[
            "editor",
            "home-dr-memory",
            "-",
            "--kind",
            "memory",
            "--source-working",
            "-",
            "--current-source",
            "--range-context",
            p(&context),
            "--value",
            "0",
            "--dry-run",
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn packed_shared_fields_readonly_view_and_second_metadata_remain_real_commands() {
    let files = Files::new();
    let raw = image();
    let source = cs705::icf::Icf::from_memory(raw.clone())
        .unwrap()
        .to_bytes();
    for (id, value, patches) in [
        ("A2-H54848-124", "-1", vec![(0x37804, 0xaf)]),
        ("A2-H56095-01", "510", vec![(0x377c6, 0), (0x3808a, 255)]),
    ] {
        let assignment = format!("choice/{id}/0={value}");
        let out = cli(&["edit", "-", "--set", &assignment, "-o", "-"], &source);
        success(&out);
        let mut expected = raw.clone();
        for (at, value) in patches {
            expected[at] = value;
        }
        assert_eq!(
            cs705::icf::Icf::parse(&out.stdout).unwrap().memory(),
            expected
        );
    }
    for index in [0, 14] {
        let out = cli(
            &["show", "-", "--key", &format!("filter-view/{index}")],
            &source,
        );
        success(&out);
        let view: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(view[0]["value"]["passband_hz"].is_i64());
        assert!(view[0]["value"]["shift_hz"].is_i64());
    }
    let output = files.path("connections");
    success(&cli(
        &[
            "editor",
            "metadata",
            "-",
            "connection-ip-list",
            "--kind",
            "memory",
            "clear",
            "--yes",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut expected = raw;
    expected[0x399c1] = 0;
    assert_eq!(fs::read(output).unwrap(), expected);
}

#[test]
fn compound_publication_preserves_no_clobber_overwrite_dryrun_and_complete_stdin_set() {
    let files = Files::new();
    let mut raw = image();
    raw[0x37850] = 9;
    let output = files.write("output", b"keep");
    let args = [
        "editor",
        "bw",
        "-",
        "0",
        "--kind",
        "memory",
        "--value",
        "100",
        "-o",
        p(&output),
    ];
    assert_eq!(cli(&args, &raw).status.code(), Some(4));
    assert_eq!(fs::read(&output).unwrap(), b"keep");
    let mut args = args.to_vec();
    args.push("--dry-run");
    success(&cli(&args, &raw));
    assert_eq!(fs::read(&output).unwrap(), b"keep");
    args.pop();
    args.push("--overwrite");
    success(&cli(&args, &raw));
    let mut expected = raw.clone();
    expected[0x37850] = 1;
    expected[0x37867..0x37869].fill(0);
    assert_eq!(fs::read(&output).unwrap(), expected);
    for args in [
        vec![
            "editor",
            "key",
            "-",
            "0",
            "microphone_keys",
            "--kind",
            "memory",
            "--range-context",
            "-",
            "--value",
            "0",
            "--dry-run",
        ],
        vec![
            "editor",
            "conditional",
            "-",
            "auto-repeater",
            "--kind",
            "memory",
            "--range-context",
            "-",
            "--value",
            "0",
            "--dry-run",
        ],
        vec![
            "editor",
            "rf",
            "-",
            "0",
            "preamp",
            "--kind",
            "memory",
            "--resource-context",
            "-",
            "--value",
            "0",
            "--dry-run",
        ],
        vec![
            "editor",
            "home-channel",
            "-",
            "--kind",
            "memory",
            "--current-working",
            "-",
            "--range-context",
            "missing",
            "--show-both",
            "0",
            "--value",
            "0",
            "--dry-run",
        ],
    ] {
        let out = cli(&args, &[]);
        assert_eq!(out.status.code(), Some(3));
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains("stdin may be consumed by only one input")
        );
    }
}

#[test]
fn program_scan_ps01_ps02_use_current_generation_and_do_not_join_nul_tail_digits() {
    let files = Files::new();
    let mut raw = image();
    raw[0xa60b..0xa60f].fill(255);
    raw[0x37802] = 255;
    raw[0x3a8ca] = 1;
    let mut current = raw.clone();
    current.resize(0x4997a, 0xcc);
    current[0xa60b] = 0xde;
    current[0xa60e] = 0xfe;
    for (at, low, high) in [
        (0xa368, 30_000_u32, 470_000_000_u32),
        (0xa3ef, 145_000_000, 146_000_000),
        (0xa5f0, 433_000_000, 430_000_000),
    ] {
        current[at..at + 4].copy_from_slice(&high.to_le_bytes());
        current[at + 4..at + 8].copy_from_slice(&low.to_le_bytes());
    }
    let current = files.write("current", &current);
    for (name, input, normalized, selected) in [
        ("typed", "5", "35", 5_u8),
        (
            "internal-nul",
            "{\"action\":\"text\",\"text\":\"P0\\u0000 5\"}",
            "30",
            0,
        ),
        (
            "trim-nul",
            "{\"action\":\"text\",\"text\":\"\\u0000 P05\\u0000\"}",
            "3035",
            5,
        ),
    ] {
        let output = files.path(name);
        let out = cli(
            &[
                "editor",
                "program-scan",
                "-",
                "--kind",
                "memory",
                "--current-working",
                p(&current),
                "--value",
                input,
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        let mut expected = raw.clone();
        expected[0x37802] = 0x83 | (selected << 2);
        assert_eq!(fs::read(output).unwrap(), expected);
        assert!(
            reports(&out)
                .iter()
                .any(|value| value["shared_source_after"]["hex"] == normalized)
        );
    }
    let out = cli(
        &[
            "editor",
            "program-scan",
            "-",
            "--kind",
            "memory",
            "--current-working",
            p(&current),
            "--value",
            "{\"action\":\"begin\"}",
            "--dry-run",
        ],
        &raw,
    );
    success(&out);
    assert!(
        reports(&out)
            .iter()
            .any(|value| value["editor"]["choices"] == json!([0, 5, 24]))
    );
    let mut empty_current = raw.clone();
    empty_current.resize(0x4997a, 0xcc);
    let empty_current = files.write("empty-current", &empty_current);
    raw[0xa60b] = 0xdf;
    raw[0xa3ef..0xa3f3].copy_from_slice(&146_000_000_u32.to_le_bytes());
    raw[0xa3f3..0xa3f7].copy_from_slice(&145_000_000_u32.to_le_bytes());
    let output = files.path("rejected-current");
    let out = cli(
        &[
            "editor",
            "program-scan",
            "-",
            "--kind",
            "memory",
            "--current-working",
            p(&empty_current),
            "--value",
            "5",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    let out = cli(
        &[
            "editor",
            "program-scan",
            "-",
            "--kind",
            "memory",
            "--current-working",
            p(&empty_current),
            "--value",
            "{\"action\":\"begin\"}",
            "--dry-run",
        ],
        &raw,
    );
    success(&out);
    assert!(
        reports(&out)
            .iter()
            .any(|value| value["editor"]["choices"] == json!([]))
    );
}

#[test]
fn program_scan_native_text_raw_low5_and_reset_failure_have_literal_process_outputs() {
    let files = Files::new();
    let mut raw = image();
    raw[0xa60b..0xa60f].fill(0);
    raw[0x3a8ca] = 1;
    for row in 0..25 {
        let at = 0xa368 + 27 * row;
        raw[at..at + 4].copy_from_slice(&14_300_000_u32.to_le_bytes());
        raw[at + 4..at + 8].copy_from_slice(&14_200_000_u32.to_le_bytes());
    }
    for (text, row) in [
        ("P00", 0),
        ("P05", 5),
        ("1.0", 10),
        ("1e1", 11),
        ("P24", 24),
    ] {
        let output = files.path(text);
        let out = cli(
            &[
                "editor",
                "program-scan",
                "-",
                "--kind",
                "memory",
                "--current-source",
                "--value",
                text,
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        let mut expected = raw.clone();
        expected[0x37802] = (expected[0x37802] & 0x83) | (row << 2);
        assert_eq!(fs::read(output).unwrap(), expected);
    }
    for code in 25..32 {
        let output = files.path(&format!("raw-{code}"));
        let action =
            json!({"action":"selected","key":16,"phase":2,"candidate":code,"accepted":true})
                .to_string();
        success(&cli(
            &[
                "editor",
                "program-scan",
                "-",
                "--kind",
                "memory",
                "--current-source",
                "--value",
                &action,
                "-o",
                p(&output),
            ],
            &raw,
        ));
        let mut expected = raw.clone();
        expected[0x37802] = (expected[0x37802] & 0x83) | ((code as u8) << 2);
        assert_eq!(fs::read(output).unwrap(), expected);
    }
    for text in ["25", "-1", ""] {
        let output = files.path(&format!("bad-{text}"));
        let out = cli(
            &[
                "editor",
                "program-scan",
                "-",
                "--kind",
                "memory",
                "--current-source",
                "--value",
                text,
                "-o",
                p(&output),
            ],
            &raw,
        );
        assert_eq!(out.status.code(), Some(3));
        assert!(!output.exists());
    }
    let output = files.path("cancel");
    success(&cli(
        &[
            "editor",
            "program-scan",
            "-",
            "--kind",
            "memory",
            "--current-source",
            "--value",
            "{\"action\":\"cancel\"}",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    assert!(!output.exists());
    raw[0x3a8ca] = 0;
    raw[0x3a8c9] = 0;
    raw[0x3c84a] = 0;
    raw[0x37806] = 8;
    raw[0x37807] = 1;
    let action =
        "{\"action\":\"selected\",\"key\":16,\"phase\":2,\"candidate\":31,\"accepted\":true}";
    let output = files.path("default");
    let out = cli(
        &[
            "editor",
            "program-scan",
            "-",
            "--kind",
            "memory",
            "--current-source",
            "--value",
            action,
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    let output = files.path("partial");
    let out = cli(
        &[
            "editor",
            "program-scan",
            "-",
            "--kind",
            "memory",
            "--current-source",
            "--value",
            action,
            "--accept-partial",
            "--yes",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(5));
    let mut expected = raw.clone();
    expected[0x37802] = (expected[0x37802] & 0x83) | 0x7c;
    expected[0x37807] = 0;
    assert_eq!(fs::read(output).unwrap(), expected);
    let mut g = vec![0; 0x37814];
    g[0x37810..0x37814].copy_from_slice(&[1, 2, 3, 4]);
    let g = files.write("g", &g);
    let output = files.path("reset");
    success(&cli(
        &[
            "editor",
            "program-scan",
            "-",
            "--kind",
            "memory",
            "--current-source",
            "--defaults",
            p(&g),
            "--value",
            action,
            "-o",
            p(&output),
        ],
        &raw,
    ));
    expected[0x37810..0x37814].copy_from_slice(&[1, 2, 3, 4]);
    assert_eq!(fs::read(output).unwrap(), expected);
    let out = cli(
        &[
            "editor",
            "program-scan",
            "-",
            "--kind",
            "memory",
            "--current-source",
            "--defaults",
            "-",
            "--value",
            action,
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
fn bw_and_pbt_native_effects_cancel_and_index_boundaries_have_literal_outputs() {
    let files = Files::new();
    for (index, value) in [(0, 501), (14, 2001)] {
        let mut raw = image();
        raw[0x37850 + index] = 9;
        raw[0x37867 + 2 * index..0x37869 + 2 * index].copy_from_slice(&[1, 255]);
        let output = files.path(&format!("same-{index}"));
        let action =
            json!({"action":"selected","key":1,"phase":2,"candidate":value,"accepted":true})
                .to_string();
        success(&cli(
            &[
                "editor",
                "bw",
                "-",
                &index.to_string(),
                "--kind",
                "memory",
                "--value",
                &action,
                "-o",
                p(&output),
            ],
            &raw,
        ));
        assert_eq!(fs::read(output).unwrap(), raw);
    }
    let mut raw = image();
    raw[0x37850] = 9;
    raw[0x37867..0x37869].copy_from_slice(&[1, 255]);
    let output = files.path("typed");
    success(&cli(
        &[
            "editor",
            "bw",
            "-",
            "0",
            "--kind",
            "memory",
            "--value",
            "\t{\"action\":\"text\",\"text\":\"1e3\"}",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut expected = raw.clone();
    expected[0x37850] = 0;
    expected[0x37867..0x37869].fill(0);
    assert_eq!(fs::read(output).unwrap(), expected);
    let output = files.path("cancelled");
    success(&cli(
        &[
            "editor",
            "bw",
            "-",
            "0",
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
    let output = files.path("invalid");
    let out = cli(
        &[
            "editor",
            "bw",
            "-",
            "15",
            "--kind",
            "memory",
            "--value",
            "100",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    raw[0x3785e] = 9;
    raw[0x37883] = 127;
    let output = files.path("pbt");
    success(&cli(
        &[
            "editor",
            "pbt",
            "-",
            "14",
            "one",
            "--kind",
            "memory",
            "--value",
            "{\"action\":\"display\"}",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    assert!(!output.exists());
}

#[test]
fn metadata_read_and_separate_clear_preserve_opaque_tails_and_no_clobber() {
    let files = Files::new();
    let mut raw = image();
    raw[0x37d89..0x37d95].copy_from_slice(b"a.ext.ext\0ZZ");
    let shown = cli(
        &[
            "editor", "metadata", "-", "picture", "--kind", "memory", "show",
        ],
        &raw,
    );
    success(&shown);
    let view: serde_json::Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(view["stem"]["utf8"], "a");
    assert_eq!(view["read_only"], true);
    let output = files.path("clear");
    success(&cli(
        &[
            "editor",
            "metadata",
            "-",
            "picture",
            "--kind",
            "memory",
            "clear",
            "--yes",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut expected = raw.clone();
    expected[0x37d89] = 0;
    expected[0x37da5..0x37dab].copy_from_slice(&[0, 255, 0, 0, 0, 0]);
    assert_eq!(fs::read(&output).unwrap(), expected);
    let out = cli(
        &[
            "editor",
            "metadata",
            "-",
            "picture",
            "--kind",
            "memory",
            "clear",
            "--yes",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(fs::read(output).unwrap(), expected);
}

#[test]
fn home_group_real_failure_prefix_is_only_published_with_explicit_partial_approval() {
    let files = Files::new();
    let normal = files.write("normal", &normal());
    let mut raw = image();
    raw[0x392fa] = 2;
    raw[0x392fb..0x392fd].copy_from_slice(&44_u16.to_le_bytes());
    let action =
        "{\"action\":\"selected\",\"key\":1,\"phase\":2,\"candidate\":256,\"accepted\":true}";
    let output = files.path("default");
    let out = cli(
        &[
            "editor",
            "home-group",
            "-",
            "--kind",
            "memory",
            "--normal-working",
            p(&normal),
            "--initial-catalog",
            "--value",
            action,
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    let output = files.path("partial");
    let out = cli(
        &[
            "editor",
            "home-group",
            "-",
            "--kind",
            "memory",
            "--normal-working",
            p(&normal),
            "--initial-catalog",
            "--value",
            action,
            "--accept-partial",
            "--yes",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(5));
    let mut expected = raw.clone();
    expected[0x392fa] = 0;
    assert_eq!(fs::read(output).unwrap(), expected);
    let out = cli(
        &[
            "editor",
            "home-group",
            "-",
            "--kind",
            "memory",
            "--normal-working",
            "-",
            "--initial-catalog",
            "--value",
            "0",
            "--dry-run",
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn home_vfo_shared_numeric_parser_preserves_lexical_exponent_and_explicit_context_roles() {
    let files = Files::new();
    let context = files.write("context", &context());
    let raw = image();
    let output = files.path("home");
    success(&cli(
        &[
            "editor",
            "home-vfo",
            "-",
            "--kind",
            "memory",
            "--range-context",
            p(&context),
            "--value",
            "1e-3",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut expected = raw.clone();
    expected[0x392f6..0x392fa].copy_from_slice(&1_000_000_u32.to_le_bytes());
    assert_eq!(fs::read(output).unwrap(), expected);
    let output = files.path("json-error");
    let out = cli(
        &[
            "editor",
            "home-vfo",
            "-",
            "--kind",
            "memory",
            "--range-context",
            p(&context),
            "--value",
            "\n{\"action\":broken}",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
}

#[test]
fn all_fixed_text_families_use_native_profiles_and_keyer_last_row_without_secret_diagnostics() {
    let files = Files::new();
    for (field, index, text, at, width, expected) in [
        (
            "gateway-address",
            0,
            "a-b.c/d:8",
            0x39562,
            64,
            b"a-b.cd8".as_slice(),
        ),
        ("gateway-callsign", 0, "ja1abc a", 0x395a2, 8, b"JA1ABC A"),
        ("ntp-address", 0, " a-b.c/d:8 ", 0x396be, 64, b"a-b.cd8"),
        ("comment", 0, "a /b ", 0x35591, 16, b"a /b"),
        ("picture-recipient", 0, "AB CD", 0x399f9, 8, b"AB    CD"),
        ("network-name", 0, ". .a/b:c", 0x3936c, 15, b"A B C"),
        ("ssid", 0, "a/b c\\d", 0x39a07, 32, b"a/b c\\d"),
        ("ap-password", 0, "abcdefgh", 0x39a27, 63, b"abcdefgh"),
        ("network-radio-name", 0, "a\\b", 0x39551, 16, b"a b"),
        ("user1-id", 0, "a b\\c", 0x39383, 16, b"abc"),
        ("user1-password", 0, "abcd1234", 0x39393, 16, b"abcd1234"),
        ("user2-id", 0, "a b\\c", 0x393a4, 16, b"abc"),
        ("user2-password", 0, "abcd1234", 0x393b4, 16, b"abcd1234"),
        ("keyer", 0, "ab!c**", 0x37158, 70, b"AB C* "),
        ("keyer", 7, "ab!c**", 0x37342, 70, b"AB C* "),
    ] {
        let raw = image();
        let output = files.path(&format!("{field}-{index}"));
        let alignment = if field == "picture-recipient" {
            "1"
        } else {
            "0"
        };
        let out = cli(
            &[
                "editor",
                "text",
                "-",
                field,
                &index.to_string(),
                "--kind",
                "memory",
                "--value",
                text,
                "--alignment",
                alignment,
                "-o",
                p(&output),
            ],
            &raw,
        );
        success(&out);
        let mut literal = raw;
        literal[at..at + width].fill(b' ');
        literal[at..at + expected.len()].copy_from_slice(expected);
        assert_eq!(fs::read(output).unwrap(), literal, "{field}");
        if field.ends_with("password") {
            assert!(!String::from_utf8_lossy(&out.stderr).contains(text));
        }
    }
    let raw = image();
    let output = files.path("empty-ssid");
    let out = cli(
        &[
            "editor",
            "text",
            "-",
            "ssid",
            "0",
            "--kind",
            "memory",
            "--value",
            "",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    let output = files.path("no-alignment");
    let out = cli(
        &[
            "editor",
            "text",
            "-",
            "picture-recipient",
            "0",
            "--kind",
            "memory",
            "--value",
            "AB CD",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    let output = files.path("phase2");
    success(&cli(
        &[
            "editor",
            "text",
            "-",
            "picture-recipient",
            "0",
            "--kind",
            "memory",
            "--value",
            "{\"action\":\"text\",\"text\":\"AB CD\",\"phase\":2}",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut literal = raw;
    literal[0x399f9..0x39a01].copy_from_slice(b"AB CD   ");
    assert_eq!(fs::read(output).unwrap(), literal);
}

#[test]
fn explicit_nls_outputs_keep_zero_returns_width_checks_and_shared_failure_prefix() {
    let files = Files::new();
    let raw = image();
    let context = files.write(
        "nls",
        &serde_json::to_vec(&json!({"lcmap":{"result":0,"output_hex":"41A142202020"}})).unwrap(),
    );
    let output = files.path("mapped");
    success(&cli(
        &[
            "editor",
            "text",
            "-",
            "comment",
            "0",
            "--kind",
            "memory",
            "--value",
            "{\"action\":\"text\",\"hex\":\"41A142\"}",
            "--nls-context",
            p(&context),
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut literal = raw.clone();
    literal[0x35591..0x355a1].fill(b' ');
    literal[0x35591..0x35594].copy_from_slice(b"A B");
    assert_eq!(fs::read(output).unwrap(), literal);
    let context = files.write(
        "missing-upper",
        &serde_json::to_vec(&json!({"lcmap":{"result":0,"output_hex":"78797A202020"}})).unwrap(),
    );
    let output = files.path("not-published");
    let out = cli(
        &[
            "editor",
            "text",
            "-",
            "picture-recipient",
            "0",
            "--kind",
            "memory",
            "--value",
            "{\"action\":\"text\",\"hex\":\"410042\"}",
            "--alignment",
            "0",
            "--nls-context",
            p(&context),
            "--accept-partial",
            "--yes",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    assert!(String::from_utf8_lossy(&out.stderr).contains("78797A"));
    let context = files.write(
        "wrong-width",
        &serde_json::to_vec(&json!({"lcmap":{"result":0,"output_hex":"41"}})).unwrap(),
    );
    let output = files.path("wrong");
    let out = cli(
        &[
            "editor",
            "text",
            "-",
            "comment",
            "0",
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
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    let out = cli(
        &[
            "editor",
            "text",
            "-",
            "comment",
            "0",
            "--kind",
            "memory",
            "--value-file",
            "-",
            "--dry-run",
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(3));
}
