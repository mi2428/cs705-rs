//! Real digital command checks: native fields, source boundaries and publication.
use cs705::icf::{Icf, format};
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
            "digital-settings-{}-{}",
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
fn image() -> Vec<u8> {
    let mut raw = vec![0xa5; 0x3c9c0];
    raw[0x355a1] = 6;
    raw[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    raw
}
fn reports(out: &Output) -> Vec<Value> {
    serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter()
        .map(|v| v.unwrap())
        .collect()
}

#[test]
fn selections_cover_every_row_bases_off_same_unknown_and_rejected_event_publication() {
    let files = Files::new();
    for (name, count, at, base) in [
        ("my-call-selected", 6, 0x3ab81, 0),
        ("tx-message-selected", 5, 0x3ab80, 1),
    ] {
        for index in 0..count {
            let mut raw = image();
            raw[at] = 255;
            let path = format!("{name}/{index}");
            let output = files.path(&format!("{name}-{index}"));
            let out = cli(
                &[
                    "editor",
                    "digital",
                    "select",
                    "-",
                    &path,
                    "--kind",
                    "memory",
                    "--value",
                    "1",
                    "-o",
                    p(&output),
                ],
                &raw,
            );
            success(&out);
            let mut expected = raw;
            expected[at] = index as u8 + base;
            assert_eq!(fs::read(&output).unwrap(), expected);
            let same = files.path(&format!("same-{name}-{index}"));
            success(&cli(
                &[
                    "editor",
                    "digital",
                    "select",
                    p(&output),
                    &path,
                    "--kind",
                    "memory",
                    "--value",
                    "1",
                    "-o",
                    p(&same),
                ],
                &[],
            ));
            assert_eq!(fs::read(same).unwrap(), expected);
            let off = files.path(&format!("off-{name}-{index}"));
            success(&cli(
                &[
                    "editor",
                    "digital",
                    "select",
                    p(&output),
                    &path,
                    "--kind",
                    "memory",
                    "--value",
                    "0",
                    "-o",
                    p(&off),
                ],
                &[],
            ));
            if base == 1 {
                expected[at] = 0;
            }
            assert_eq!(fs::read(off).unwrap(), expected);
        }
    }
    let raw = image();
    for (i, action) in [
        r#"{"action":"cancel"}"#,
        r#"{"action":"display"}"#,
        r#"{"action":"begin"}"#,
    ]
    .into_iter()
    .enumerate()
    {
        let output = files.path(&format!("inspection-{i}"));
        success(&cli(
            &[
                "editor",
                "digital",
                "select",
                "-",
                "my-call-selected/0",
                "--kind",
                "memory",
                "--value",
                action,
                "-o",
                p(&output),
            ],
            &raw,
        ));
        assert!(!output.exists());
    }
    for (i, action) in [
        json!({"action":"selected","key":2,"phase":2,"candidate":1,"accepted":true}),
        json!({"action":"selected","key":1,"phase":1,"candidate":1,"accepted":true}),
        json!({"action":"selected","key":1,"phase":2,"candidate":1,"accepted":false}),
    ]
    .into_iter()
    .enumerate()
    {
        let output = files.path(&format!("rejected-{i}"));
        let out = cli(
            &[
                "editor",
                "digital",
                "select",
                "-",
                "tx-message-selected/4",
                "--kind",
                "memory",
                "--value",
                &action.to_string(),
                "-o",
                p(&output),
            ],
            &raw,
        );
        assert!(!out.status.success());
        assert!(!output.exists());
    }
    for path in [
        "my-call-selected/6",
        "tx-message-selected/5",
        "my-call-selected",
        "tx-message-selected/18446744073709551615",
    ] {
        assert!(
            !cli(
                &[
                    "editor",
                    "digital",
                    "select",
                    "-",
                    path,
                    "--kind",
                    "memory",
                    "--value",
                    "1",
                    "--dry-run"
                ],
                &raw
            )
            .status
            .success()
        );
    }
}

#[test]
fn text_real_commands_preserve_opaque_metadata_and_require_actual_phase1_alignment() {
    let files = Files::new();
    let raw = image();
    for (name, count, base, stride, literal) in [
        ("my-call", 6, 0x2ed5b, 12, &b"AB/C D  "[..]),
        ("my-call-suffix", 6, 0x2ed63, 12, &b"AB/C"[..]),
        ("tx-message", 5, 0x32bb5, 20, &b"ab/c.d              "[..]),
    ] {
        for index in 0..count {
            let output = files.path(&format!("{name}-{index}"));
            let path = format!("{name}/{index}");
            success(&cli(
                &[
                    "editor",
                    "digital",
                    "edit",
                    "-",
                    &path,
                    "--kind",
                    "memory",
                    "--value",
                    "ab/c.d",
                    "--alignment",
                    "0",
                    "-o",
                    p(&output),
                ],
                &raw,
            ));
            let mut expected = raw.clone();
            let at = base + stride * index;
            expected[at..at + literal.len()].copy_from_slice(literal);
            assert_eq!(fs::read(output).unwrap(), expected);
        }
    }
    let output = files.path("aligned");
    let action = r#"{"action":"text","hex":"6162206364","phase":1}"#;
    success(&cli(
        &[
            "editor",
            "digital",
            "edit",
            "-",
            "my-call/5",
            "--kind",
            "memory",
            "--value",
            action,
            "--alignment",
            "1",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    assert_eq!(&fs::read(&output).unwrap()[0x2ed97..0x2ed9f], b"AB    CD");
    let missing = files.path("missing-alignment");
    assert!(
        !cli(
            &[
                "editor",
                "digital",
                "edit",
                "-",
                "my-call/5",
                "--kind",
                "memory",
                "--value",
                action,
                "-o",
                p(&missing)
            ],
            &raw
        )
        .status
        .success()
    );
    assert!(!missing.exists());
    for (i, action) in [
        r#"{"action":"cancel"}"#,
        r#"{"action":"display"}"#,
        r#"{"action":"begin"}"#,
    ]
    .into_iter()
    .enumerate()
    {
        let output = files.path(&format!("text-inspect-{i}"));
        success(&cli(
            &[
                "editor",
                "digital",
                "edit",
                "-",
                "my-call/0",
                "--kind",
                "memory",
                "--value",
                action,
                "-o",
                p(&output),
            ],
            &raw,
        ));
        assert!(!output.exists());
    }
    let comment = &[b'R', 0x82, 0xa0];
    let extra = 0x80000001;
    let icf =
        format::write_storage(&raw, format::Layout::from_map_revision(6), comment, extra).unwrap();
    let output = files.path("metadata.icf");
    success(&cli(
        &[
            "editor",
            "digital",
            "edit",
            "-",
            "tx-message/4",
            "--value",
            "hello /?",
            "-o",
            p(&output),
        ],
        &icf,
    ));
    let bytes = fs::read(output).unwrap();
    let edited = Icf::parse(&bytes).unwrap();
    assert_eq!(edited.comment_bytes(), comment);
    assert_eq!(edited.extra_data(), extra);
    let mut expected = raw;
    expected[0x32c05..0x32c19].copy_from_slice(b"hello /?            ");
    assert_eq!(edited.memory(), expected);
}

#[test]
fn actual_nls_outputs_keep_zero_result_distinct_and_all_stdin_collisions_fail_before_reads() {
    let files = Files::new();
    let raw = image();
    let output = files.path("nls");
    let context=files.write("nls-context",br#"{"lcmap":{"output_hex":"A12FDF6120202020","result":0},"uppercase":{"output_hex":"A12FDF41","result":0}}"#);
    let action = r#"{"action":"text","hex":"A12FDF61","phase":2}"#;
    success(&cli(
        &[
            "editor",
            "digital",
            "edit",
            "-",
            "my-call/0",
            "--kind",
            "memory",
            "--value",
            action,
            "--nls-context",
            p(&context),
            "-o",
            p(&output),
        ],
        &raw,
    ));
    assert_eq!(&fs::read(output).unwrap()[0x2ed5b..0x2ed63], b" / A    ");
    let output = files.path("unavailable");
    let out = cli(
        &[
            "editor",
            "digital",
            "edit",
            "-",
            "my-call/0",
            "--kind",
            "memory",
            "--value",
            action,
            "-o",
            p(&output),
        ],
        &raw,
    );
    #[cfg(not(windows))]
    {
        assert!(!out.status.success());
        assert!(!output.exists());
    }
    #[cfg(windows)]
    {
        success(&out);
        assert!(output.exists());
    }
    for args in [
        vec![
            "editor",
            "digital",
            "edit",
            "-",
            "my-call/0",
            "--kind",
            "memory",
            "--value-file",
            "-",
            "--dry-run",
        ],
        vec![
            "editor",
            "digital",
            "edit",
            "-",
            "my-call/0",
            "--kind",
            "memory",
            "--value",
            action,
            "--nls-context",
            "-",
            "--dry-run",
        ],
        vec![
            "editor",
            "digital",
            "edit",
            "/missing-image",
            "my-call/0",
            "--kind",
            "memory",
            "--value-file",
            "-",
            "--nls-context",
            "-",
            "--dry-run",
        ],
        vec![
            "editor",
            "digital",
            "select",
            "-",
            "my-call-selected/0",
            "--kind",
            "memory",
            "--value-file",
            "-",
            "--dry-run",
        ],
    ] {
        let out = cli(&args, b"not read");
        assert!(!out.status.success());
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains("stdin may be consumed by only one input")
        );
    }
}

#[test]
fn allowed_prefix_cancel_late_failure_partial_policy_duplicates_and_full_row_are_native() {
    let files = Files::new();
    let mut raw = image();
    raw[0x395ad..0x3969d].fill(b' ');
    raw[0x395ad..0x395b5].copy_from_slice(b"OLD     ");
    raw[0x395bd..0x395c5].copy_from_slice(b"LATE    ");
    raw[0x395ac] = 0;
    let output = files.path("cancel");
    let out = cli(
        &[
            "editor",
            "digital",
            "edit",
            "-",
            "allowed-call/1",
            "--kind",
            "memory",
            "--input-code",
            "13",
            "--value",
            r#"{"action":"cancel"}"#,
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
            .any(|v| v["native_effect"]["inserted"] == true
                && v["native_effect"]["notifications"] == json!([64, 57]))
    );
    let output = files.path("saved");
    success(&cli(
        &[
            "editor",
            "digital",
            "edit",
            "-",
            "allowed-call/1",
            "--kind",
            "memory",
            "--input-code",
            "28928",
            "--value",
            "old",
            "--alignment",
            "0",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    let mut expected = raw.clone();
    expected.copy_within(0x395b5..0x39695, 0x395bd);
    expected[0x395b5..0x395bd].copy_from_slice(b"OLD     ");
    assert_eq!(fs::read(&output).unwrap(), expected);
    let out = cli(
        &[
            "editor",
            "digital",
            "read",
            p(&output),
            "allowed-call/1",
            "--kind",
            "memory",
        ],
        &[],
    );
    success(&out);
    let view: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(view["duplicate_warning"], true);
    assert_eq!(view["parent_enable_raw"], 0);
    assert_eq!(view["prefix_count"], 2);
    let action = r#"{"action":"text","hex":"A1","phase":2}"#;
    let output = files.path("error");
    let out = cli(
        &[
            "editor",
            "digital",
            "edit",
            "-",
            "allowed-call/1",
            "--kind",
            "memory",
            "--input-code",
            "8",
            "--value",
            action,
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert!(!out.status.success());
    assert!(!output.exists());
    let output = files.path("partial");
    let out = cli(
        &[
            "editor",
            "digital",
            "edit",
            "-",
            "allowed-call/1",
            "--kind",
            "memory",
            "--input-code",
            "8",
            "--value",
            action,
            "--accept-partial",
            "--yes",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert!(!out.status.success());
    expected[0x395b5..0x395bd].copy_from_slice(b"CALLSIGN");
    assert_eq!(fs::read(output).unwrap(), expected);
    let output = files.path("no-consent");
    assert!(
        !cli(
            &[
                "editor",
                "digital",
                "edit",
                "-",
                "allowed-call/1",
                "--kind",
                "memory",
                "--input-code",
                "8",
                "--value",
                action,
                "--accept-partial",
                "-o",
                p(&output)
            ],
            &raw
        )
        .status
        .success()
    );
    assert!(!output.exists());
    raw[0x395ad..0x3969d].fill(b'X');
    let output = files.path("full");
    let out = cli(
        &[
            "editor",
            "digital",
            "edit",
            "-",
            "allowed-call/30",
            "--kind",
            "memory",
            "--alignment",
            "0",
            "--input-code",
            "13",
            "--value",
            "a",
            "-o",
            p(&output),
        ],
        &raw,
    );
    assert!(!out.status.success());
    assert!(!output.exists());
    assert!(
        reports(&out)
            .iter()
            .any(|v| v["native_effect"]["full_warning"] == true)
    );
}

#[test]
fn reflector_readonly_source_fields_and_shared_no_clobber_dryrun_remain_real_commands() {
    let files = Files::new();
    let mut raw = image();
    for (index, physical) in [0, 1, 4, 17, 254].into_iter().enumerate() {
        raw[0x3c363 + index] = physical;
        let at = 0x32aed + 8 * usize::from(physical);
        raw[at..at + 8].copy_from_slice(&[0x82, 0xa0, b' ', 0, b'X', b' ', b' ', b' ']);
    }
    for index in 0..5 {
        let out = cli(
            &[
                "editor",
                "digital",
                "read",
                "-",
                &format!("reflector/{index}"),
                "--kind",
                "memory",
            ],
            &raw,
        );
        success(&out);
        let view: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(view["valid"], true);
        assert_eq!(view["display"]["hex"], "82A020");
        assert_eq!(view["read_only"], true);
    }
    raw[0x3c363] = 255;
    let out = cli(
        &[
            "editor",
            "digital",
            "read",
            "-",
            "reflector/0",
            "--kind",
            "memory",
        ],
        &raw,
    );
    success(&out);
    let view: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(view["valid"], false);
    assert_eq!(view["raw"], Value::Null);
    assert_eq!(view["display"]["hex"], "");
    assert!(
        !cli(
            &[
                "editor",
                "digital",
                "edit",
                "-",
                "reflector/0",
                "--kind",
                "memory",
                "--value",
                "a",
                "--dry-run"
            ],
            &raw
        )
        .status
        .success()
    );
    assert!(
        !cli(
            &[
                "editor",
                "digital",
                "read",
                "-",
                "reflector/5",
                "--kind",
                "memory"
            ],
            &raw
        )
        .status
        .success()
    );
    let out = cli(&["editor", "digital", "fields"], &[]);
    success(&out);
    let fields: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(fields.len(), 7);
    let mut ids: Vec<_> = fields
        .iter()
        .map(|v| v["feature_id"].as_str().unwrap())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 7);
    let output = files.write("existing", b"keep");
    assert!(
        !cli(
            &[
                "editor",
                "digital",
                "edit",
                "-",
                "tx-message/4",
                "--kind",
                "memory",
                "--value",
                "a",
                "-o",
                p(&output)
            ],
            &raw
        )
        .status
        .success()
    );
    assert_eq!(fs::read(&output).unwrap(), b"keep");
    let output = files.path("dryrun");
    success(&cli(
        &[
            "editor",
            "digital",
            "edit",
            "-",
            "tx-message/4",
            "--kind",
            "memory",
            "--value",
            "a",
            "--dry-run",
            "-o",
            p(&output),
        ],
        &raw,
    ));
    assert!(!output.exists());
}
