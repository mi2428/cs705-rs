//! Process-level checks use synthetic bytes and mock transcripts only.
//! No test enumerates or opens a serial port, even when serialport is installed.

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
            "cli-test-{}-{}",
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
fn cli(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cs705"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Err(error) = child.stdin.take().unwrap().write_all(stdin) {
        // Early argument rejection closes stdin before a large producer finishes.
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }
    child.wait_with_output().unwrap()
}
fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}
fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "stdout is not JSON; stderr={}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}
fn fixture() -> Vec<u8> {
    let mut memory = vec![0; 0x3c9c0];
    memory[0x355a1] = 6;
    memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    memory[1..5].copy_from_slice(&145_000_000u32.to_le_bytes());
    memory[5] = 0x70;
    memory[0x43..0x53].fill(b' ');
    memory[0x43..0x47].copy_from_slice(b"TEST");
    // Empty GPS/repeater order tables. No real factory data is embedded.
    memory[0x3c385..0x3c5dd].fill(0xff);
    memory[0x3c5dd..0x3c613].fill(0xff);
    memory[0x3ab82..0x3bf0a].fill(0xff);
    memory[0x3bf0a..0x3bf6e].fill(0xff);
    Icf::from_memory(memory).unwrap().to_bytes()
}

#[test]
fn help_all_commands_and_invalid_arguments_never_need_input() {
    for command in [
        "recent", "parse", "check", "show", "edit", "new", "convert", "import", "export", "group",
        "row", "clone", "config",
    ] {
        let out = cli(&[command, "--help"], b"");
        assert!(out.status.success(), "{command}");
        assert!(out.stderr.is_empty());
    }
    for command in [
        "decode",
        "encode",
        "query",
        "search",
        "raw-read",
        "write",
        "raw-write",
        "plan",
    ] {
        assert!(
            cli(&["clone", command, "--help"], b"").status.success(),
            "{command}"
        );
    }
    for args in [
        &["unknown"][..],
        &["edit", "-"][..],
        &["clone", "raw-read", "--profile", "6"][..],
        &["clone", "plan", "--profile", "6", "--mask", "32"][..],
    ] {
        let out = cli(args, b"");
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn stdin_roundtrip_semantic_edit_and_late_failure_preserve_outputs() {
    let files = Files::new();
    let source = fixture();
    let parsed = cli(&["parse", "-"], &source);
    assert!(parsed.status.success());
    assert_eq!(json(&parsed)["memory_bytes"], 0x3c9c0);
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "setting/beep_level_raw/0=203",
            "--set",
            "scope-waveform-color-current=[1,2,3]",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let image = Icf::parse(&out.stdout).unwrap();
    let before = Icf::parse(&source).unwrap();
    assert_eq!(image.memory()[0x392f2], 203);
    assert_eq!(&image.memory()[0x3972d..0x39730], &[1, 2, 3]);
    for i in 0..image.memory().len() {
        if i != 0x392f2 && !(0x3972d..0x39730).contains(&i) {
            assert_eq!(image.memory()[i], before.memory()[i], "{i:X}");
        }
    }
    let existing = files.write("existing.icf", b"must survive");
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "setting/beep_level_raw/0=99",
            "--set",
            "no-such-field=0",
            "--output",
            p(&existing),
            "--overwrite",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    assert_eq!(fs::read(existing).unwrap(), b"must survive");
    let out = cli(
        &[
            "convert",
            "-",
            "--conversion",
            "canonicalize",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success());
    assert_eq!(out.stdout, source);
}

#[test]
fn no_clobber_dry_run_and_atomic_replacement() {
    let files = Files::new();
    let source = fixture();
    let existing = files.write("settings.icf", b"old");
    let args = [
        "edit",
        "-",
        "--set",
        "setting/beep_level_raw/0=42",
        "--output",
        p(&existing),
    ];
    let out = cli(&args, &source);
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(fs::read(&existing).unwrap(), b"old");
    let mut args = args.to_vec();
    args.push("--overwrite");
    let out = cli(&args, &source);
    assert!(out.status.success());
    assert_eq!(
        Icf::parse(&fs::read(&existing).unwrap()).unwrap().memory()[0x392f2],
        42
    );
    let absent = files.path("not-created");
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "setting/beep_level_raw/0=43",
            "--dry-run",
            "--output",
            p(&absent),
        ],
        &source,
    );
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(!absent.exists());
    assert_eq!(fs::read_dir(&files.0).unwrap().count(), 1);
}

#[test]
fn batch_check_reports_partial_failure_and_escaped_opaque_text() {
    let files = Files::new();
    let valid = files.write("valid.icf", &fixture());
    let bad = files.write("bad.icf", b"bad");
    let out = cli(&["check", p(&valid), p(&bad)], b"");
    assert_eq!(out.status.code(), Some(5));
    assert_eq!(json(&out)["passed"], 1);
    assert_eq!(json(&out)["failed"], 1);
    let out = cli(
        &[
            "show",
            "-",
            "--key",
            "setting/beep_level_raw/0",
            "--key",
            "no-such",
        ],
        &fixture(),
    );
    assert_eq!(out.status.code(), Some(5));
    assert_eq!(json(&out)[0]["value"], 0);
    assert!(json(&out)[1].get("error").is_some());
}

#[test]
fn csv_typed_edit_roundtrip_and_projection() {
    let source=b"Group,Group Name,Name,Date,Time,Latitude,Longitude,Altitude,Alarm\r\nA,G,OLD,opaque-date,opaque-time,35,139,,OFF\r\n";
    let out = cli(
        &[
            "edit",
            "-",
            "--kind",
            "gps-csv",
            "--set",
            "0/name=NEW",
            "--set",
            "0/alarm=true",
            "--output",
            "-",
        ],
        source,
    );
    assert!(out.status.success());
    assert_eq!(out.stdout,b"Group,Group Name,Name,Date,Time,Latitude,Longitude,Altitude,Alarm\r\nA,G,NEW,opaque-date,opaque-time,35,139,,ON\r\n");
    let out = cli(
        &["show", "-", "--kind", "gps-csv", "--columns", "Name,Alarm"],
        &out.stdout,
    );
    assert!(out.status.success());
    assert_eq!(json(&out)[0][0]["column"], "Name");
    assert_eq!(json(&out)[0][0]["value"]["utf8"], "NEW");
}

#[test]
fn clone_frames_mock_failure_and_safety_gate_never_open_devices() {
    let out = cli(
        &[
            "clone",
            "encode",
            "--frame",
            "identify",
            "--direction",
            "to-radio",
            "--output",
            "-",
        ],
        b"",
    );
    assert!(out.status.success());
    assert_eq!(
        out.stdout,
        [0xfe, 0xfe, 0xee, 0xef, 0xe0, 0x40, 0x75, 0, 0, 0xfd]
    );
    let decoded = cli(
        &["clone", "decode", "-", "--direction", "to-radio"],
        &out.stdout,
    );
    assert!(decoded.status.success());
    assert_eq!(json(&decoded)[0]["frame"], "Identify");
    let files = Files::new();
    let mock = files.write("fail.json", br#"{"events":[{"write_error":true}]}"#);
    let out = cli(&["clone", "query", "--mock", p(&mock)], b"");
    assert_eq!(out.status.code(), Some(5));
    assert_eq!(json(&out)["transmitted_wire_bytes"], 0);
    assert_eq!(json(&out)["automatic_retry"], false);
    // Missing consent is rejected before Serial::open reaches the OS. The name
    // is deliberately not a real device path; no fake OS port is opened either.
    let out = cli(&["clone", "query", "--port", "NEVER_OPENED"], b"");
    assert_eq!(out.status.code(), Some(6));
    assert!(out.stdout.is_empty());
    let out = cli(
        &[
            "clone",
            "raw-read",
            "--port",
            "NEVER_OPENED",
            "--profile",
            "6",
            "--dry-run",
        ],
        b"",
    );
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn dangerous_commands_require_confirmation_and_cancel_publishes_nothing() {
    let files = Files::new();
    let output = files.path("absent.icf");
    let out = cli(
        &[
            "import",
            "unused.icf",
            "--csv",
            "unused.csv",
            "--sheet",
            "repeater",
            "--output",
            p(&output),
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(6));
    assert!(!output.exists());
    let out = cli(
        &[
            "import",
            "unused.icf",
            "--csv",
            "unused.csv",
            "--sheet",
            "repeater",
            "--mode",
            "cancel",
            "--output",
            p(&output),
        ],
        b"",
    );
    assert!(out.status.success());
    assert!(!output.exists());
    let config = files.path("config.json");
    let out = cli(
        &[
            "config",
            p(&config),
            "set",
            "show-split=false",
            "columns=[\"name\"]",
            "--output",
            p(&config),
        ],
        b"",
    );
    assert!(out.status.success());
    let out = cli(&["config", p(&config), "show"], b"");
    assert!(out.status.success());
    assert_eq!(json(&out), json!({"show-split":false,"columns":["name"]}));
}

#[test]
fn semantic_catalog_and_original_selection_events_use_the_core_contract() {
    use cs705::icf::settings::{self, Value as SettingValue};
    // These source case groups need separate contexts, not a guessed common
    // maximum. The literal guard/filter and post-store expectations are below.
    const CONTEXTUAL: [u16; 4] = [0x2e0, 0x2fb, 0x2fc, 0x2fd];
    let source = fixture();
    let listed = cli(&["show", "-", "--list-fields"], b"");
    assert!(listed.status.success());
    let list = json(&listed);
    let list = list.as_array().unwrap();
    let mut args = vec![
        "edit".to_owned(),
        "-".to_owned(),
        "--output".to_owned(),
        "-".to_owned(),
    ];
    for setting in settings::catalog() {
        assert!(
            list.iter()
                .any(|f| f["key"] == format!("setting/{}/0", setting.name))
        );
        if CONTEXTUAL.contains(&setting.key) {
            continue;
        }
        for index in 0..setting.count {
            args.push("--set".into());
            args.push(format!(
                "setting/{}/{index}={}",
                setting.name, setting.domain.max
            ));
        }
    }
    let refs: Vec<_> = args.iter().map(String::as_str).collect();
    let output = cli(&refs, &source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let image = Icf::parse(&output.stdout).unwrap();
    for setting in settings::catalog() {
        if CONTEXTUAL.contains(&setting.key) {
            continue;
        }
        for index in 0..setting.count {
            assert_eq!(
                setting.read(&image, index).unwrap(),
                SettingValue::Number(setting.domain.max)
            );
        }
    }
    let setting = settings::find("A2-H54848-31").unwrap();
    for (key, phase, accepted, expected) in [
        (0x215, 2, true, 0),
        (1, 1, true, 0),
        (1, 2, false, 0),
        (1, 2, true, 255),
    ] {
        let assignment = format!(
            "setting/{}/0={}",
            setting.name,
            json!({"key":key,"phase":phase,"candidate":-1,"accepted":accepted})
        );
        let output = cli(
            &["edit", "-", "--event", &assignment, "--output", "-"],
            &source,
        );
        assert!(output.status.success());
        if expected == 255 {
            let image = Icf::parse(&output.stdout).unwrap();
            assert_eq!(
                setting.read(&image, 0).unwrap(),
                SettingValue::Number(expected)
            );
        } else {
            assert_not_published(&output);
        }
        let report: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(report["stored"], expected == 255);
        assert_eq!(report["notify_flags"], 0); // Plain meter case has no extra 0x20.
    }
}

#[test]
fn edit_publication_declined_handlers_never_touch_outputs() {
    let files = Files::new();
    let source = fixture();
    let image = Icf::parse(&source).unwrap();
    for (kind, input) in [("icf", source.as_slice()), ("memory", image.memory())] {
        for (flag, assignment) in [
            (
                "--set",
                r#"number/noise_blanker_width_raw={"action":"display"}"#,
            ),
            ("--set", r#"number/beep_level_raw={"action":"cancel"}"#),
            ("--set", r#"choice/notch_width_am/3={"action":"begin"}"#),
            ("--set", r#"text/dtmf/0={"action":"display"}"#),
            ("--set", "color/A2-H56095-11=null"),
            (
                "--event",
                r#"number/noise_blanker_width_raw={"action":"display"}"#,
            ),
            (
                "--event",
                r#"number/beep_level_raw={"action":"selected","key":0,"phase":2,"candidate":42,"accepted":true}"#,
            ),
            (
                "--event",
                r#"choice/notch_width_am/3={"action":"selected","key":1,"phase":2,"candidate":2,"accepted":false}"#,
            ),
            ("--event", r#"text/rtty/0={"action":"cancel"}"#),
            (
                "--event",
                r#"setting/A2-H54848-31/0={"key":1,"phase":1,"candidate":2,"accepted":true}"#,
            ),
            (
                "--event",
                r#"network/prefix={"key":2,"phase":2,"candidate":24,"accepted":false}"#,
            ),
        ] {
            for destination in ["-", "absent", "existing"] {
                let absent = files.path("absent");
                let existing = files.write("existing", b"preserved");
                let output = match destination {
                    "absent" => p(&absent),
                    "existing" => p(&existing),
                    _ => "-",
                };
                let out = cli(
                    &[
                        "edit",
                        "-",
                        "--kind",
                        kind,
                        flag,
                        assignment,
                        "--overwrite",
                        "-o",
                        output,
                    ],
                    input,
                );
                assert_not_published(&out);
                assert!(!absent.exists(), "{kind}: {assignment}");
                assert_eq!(fs::read(&existing).unwrap(), b"preserved");
                if assignment.contains("noise_blanker_width_raw") {
                    let report: Value = serde_json::from_slice(&out.stderr).unwrap();
                    // This native case stores even on display: stored is not approval.
                    assert_eq!(report["stored"], true);
                    assert_eq!(report["accepted"], false);
                }
            }
        }
    }
}

#[test]
fn edit_publication_same_value_approval_and_mixed_or_are_independent_of_byte_changes() {
    let original = fixture();
    let mut image = Icf::parse(&original).unwrap();
    image.patch_memory(&[(0x399a4, &[0xfe])]).unwrap();
    for (flag, assignment) in [
        ("--set", "setting/beep_level_raw/0=0"),
        ("--set", "number/beep_level_raw=0"),
        ("--set", "choice/notch_width_am/3=0"),
        ("--set", "color/A2-H56095-11=[0,0,0]"),
        ("--set", r#"text/dtmf/0="123""#),
        ("--set", "text/rtty/0=TEST"),
        (
            "--event",
            r#"setting/A2-H54848-31/0={"key":1,"phase":2,"candidate":0,"accepted":true}"#,
        ),
        (
            "--event",
            r#"number/speech_compression_level={"action":"selected","key":1,"phase":2,"candidate":0,"accepted":true}"#,
        ),
        ("--event", "choice/notch_width_am/3=0"),
        ("--event", r#"text/dtmf/0="123""#),
    ] {
        for kind in ["icf", "memory"] {
            let source = if kind == "icf" {
                image.to_bytes()
            } else {
                image.memory().to_vec()
            };
            let args = ["edit", "-", "--kind", kind, flag, assignment, "-o", "-"];
            let first = cli(&args, &source);
            assert!(
                first.status.success(),
                "{assignment}: {}",
                String::from_utf8_lossy(&first.stderr)
            );
            assert!(!first.stdout.is_empty());
            let second = cli(&args, &first.stdout);
            assert!(second.status.success());
            assert_eq!(second.stdout, first.stdout, "{kind}: {assignment}");
        }
    }
    let accepted = "number/beep_level_raw=42";
    let declined = r#"number/beep_level_raw={"action":"cancel"}"#;
    for edits in [
        vec!["--set", accepted, "--set", declined],
        vec!["--set", declined, "--set", accepted],
        vec!["--event", accepted, "--event", declined],
        vec!["--event", declined, "--event", accepted],
        vec!["--set", accepted, "--event", declined],
        vec!["--set", declined, "--event", accepted],
    ] {
        let source = image.to_bytes();
        for kind in ["icf", "memory"] {
            let mut args = vec!["edit", "-", "--kind", kind, "-o", "-"];
            args.extend(edits.iter().copied());
            let input = if kind == "icf" {
                source.as_slice()
            } else {
                image.memory()
            };
            let out = cli(&args, input);
            if kind == "icf" {
                assert_memory_delta(&out, &source, &[(0x392f2, &[42])]);
            } else {
                assert!(out.status.success());
                let mut expected = image.memory().to_vec();
                expected[0x392f2] = 42;
                assert_eq!(out.stdout, expected);
            }
        }
    }
}

#[test]
fn edit_publication_failure_after_approval_preserves_prefix_only_in_memory() {
    let files = Files::new();
    let source = fixture();
    let image = Icf::parse(&source).unwrap();
    let target = files.write("preserved", b"preserved");
    for kind in ["icf", "memory"] {
        for failed in [
            "number/beep_level_raw={broken",
            "number/beep_level_raw=not-a-number",
            "no-such-field=1",
        ] {
            for output in ["-", p(&target)] {
                let out = cli(
                    &[
                        "edit",
                        "-",
                        "--kind",
                        kind,
                        "--set",
                        "number/beep_level_raw=42",
                        "--event",
                        failed,
                        "--overwrite",
                        "-o",
                        output,
                    ],
                    if kind == "icf" {
                        source.as_slice()
                    } else {
                        image.memory()
                    },
                );
                assert_eq!(out.status.code(), Some(3), "{kind}: {failed}");
                assert!(out.stdout.is_empty());
                assert_eq!(fs::read(&target).unwrap(), b"preserved");
            }
        }
    }
}

#[test]
fn clone_stdin_ownership_is_checked_before_reading_or_starting_a_transport() {
    let files = Files::new();
    let bytes = vec![0; cs705::clone::IMAGE_CAPACITY];
    let image = files.write("image.bin", &bytes);
    let script = files.write("startup.txt", b"");
    let mock = files.write("mock.json", br#"{"events":[]}"#);
    for (input, startup, events) in [
        ("-", "-", p(&mock)),
        ("-", p(&script), "-"),
        (p(&image), "-", "-"),
        ("-", "-", "-"),
    ] {
        let out = cli(
            &[
                "clone",
                "raw-write",
                input,
                "--startup-script",
                startup,
                "--mock",
                events,
                "--profile",
                "6",
                "--echo",
                "none",
                "--yes",
                "-o",
                "-",
            ],
            &bytes,
        );
        assert_eq!(
            out.status.code(),
            Some(3),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains("stdin"));
    }
    for (input, startup, events, stdin) in [
        ("-", p(&script), p(&mock), bytes.as_slice()),
        (p(&image), "-", p(&mock), b"".as_slice()),
        (p(&image), p(&script), "-", br#"{"events":[]}"#.as_slice()),
    ] {
        let out = cli(
            &[
                "clone",
                "raw-write",
                input,
                "--startup-script",
                startup,
                "--mock",
                events,
                "--profile",
                "6",
                "--yes",
                "--dry-run",
            ],
            stdin,
        );
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn clone_dry_run_validates_mock_inputs_without_running_or_publishing() {
    let files = Files::new();
    let image = files.write("image.bin", &vec![0; cs705::clone::IMAGE_CAPACITY]);
    for (name, source, code) in [
        ("syntax.json", Some(b"{".as_slice()), 3),
        ("type.json", Some(br#"{"events":0}"#.as_slice()), 3),
        (
            "unknown.json",
            Some(br#"{"events":[{"unknown":true}]}"#.as_slice()),
            3,
        ),
        ("missing.json", None, 4),
    ] {
        let mock = match source {
            Some(bytes) => files.write(name, bytes),
            None => files.path(name),
        };
        for operation in ["raw-read", "raw-write"] {
            let mut args = vec!["clone", operation];
            if operation == "raw-write" {
                args.extend([p(&image), "--yes"]);
            }
            args.extend(["--mock", p(&mock), "--profile", "6", "--dry-run"]);
            let out = cli(&args, b"");
            assert_eq!(out.status.code(), Some(code));
            assert!(out.stdout.is_empty());
        }
    }
}

#[test]
fn group_names_use_an_explicit_ascii_boundary_not_host_utf8() {
    let files = Files::new();
    let original = topology_fixture();
    let input = files.write("groups.icf", &original);
    for name in ["\u{6771}\u{4eac}"] {
        for action in ["add", "rename"] {
            let output = files.path("rejected.icf");
            let mut args = vec!["group", p(&input), "--sheet", "memory", action];
            if action == "add" {
                args.extend(["--name", name]);
            } else {
                args.extend(["0", name]);
            }
            args.extend(["-o", p(&output)]);
            let out = cli(&args, b"");
            assert_eq!(out.status.code(), Some(3));
            assert!(!output.exists());
            assert_eq!(fs::read(&input).unwrap(), original);
        }
    }
    for action in ["add", "rename"] {
        let mut args = vec!["group", p(&input), "--sheet", "memory", action];
        if action == "add" {
            args.extend(["--name", "0123456789ABCDEF"]);
        } else {
            args.extend(["0", "0123456789ABCDEF"]);
        }
        args.extend(["-o", "-"]);
        let out = cli(&args, b"");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(Icf::parse(&out.stdout).unwrap().comment_bytes(), b"KEEP");
    }
}

#[test]
fn native_memory_group_names_use_shared_editor_and_declined_helpers_do_not_publish() {
    let files = Files::new();
    let original = topology_fixture();
    let input = files.write("native-groups.icf", &original);
    for (value, name) in [
        ("A\tB", b"A B             ".as_slice()),
        ("\x7f", b"                ".as_slice()),
        ("0123456789ABCDEFG", b"0123456789ABCDEF".as_slice()),
        (
            r#"{"action":"text","hex":"418140424300"}"#,
            b"A\x81\x40BC           ".as_slice(),
        ),
    ] {
        for action in ["add", "rename"] {
            let mut args = vec!["group", p(&input), "--sheet", "memory", action];
            if action == "add" {
                args.extend(["--name", value]);
            } else {
                args.extend(["0", value]);
            }
            args.extend(["-o", "-"]);
            let out = cli(&args, b"");
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let image = Icf::parse(&out.stdout).unwrap();
            let model = cs705::icf::rows::NormalMemory::from_icf(&image).unwrap();
            let index = if action == "add" { 1 } else { 0 };
            // A named empty group contributes to the next displayed count;
            // an empty all-space placeholder does not. Both retain raw names.
            assert_eq!(
                &image.memory()[0x355c3 + 19 * index..0x355d3 + 19 * index],
                name
            );
            assert_eq!(model.group_used_slots(0).unwrap(), 2);
            assert_eq!(image.comment_bytes(), b"KEEP");
        }
    }
    for action in [
        r#"{"action":"cancel"}"#,
        r#"{"action":"display"}"#,
        r#"{"action":"begin","first":65}"#,
    ] {
        let output = files.path("declined.icf");
        let out = cli(
            &[
                "group",
                p(&input),
                "--sheet",
                "memory",
                "add",
                "--name",
                action,
                "-o",
                p(&output),
            ],
            b"",
        );
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!output.exists());
        assert!(out.stdout.is_empty());
        let report: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(report["helper_accepted"], false);
        assert_eq!(report["group_count_before"], 1);
        assert_eq!(report["group_count_after"], 2);
    }
    let out = cli(&["group", p(&input), "--sheet", "memory", "list"], b"");
    assert_eq!(json(&out)[0]["used"], 2);
    assert_eq!(fs::read(&input).unwrap(), original);
}

#[test]
fn batch_exit_codes_preserve_uniform_failures_and_distinguish_mixed_classes() {
    let files = Files::new();
    let missing = files.path("absent.icf");
    let bad = files.write("bad.icf", b"not ICF");
    let good = files.write("good.icf", &fixture());
    for (inputs, code) in [
        (vec![p(&missing)], 4),
        (vec![p(&bad)], 3),
        (vec![p(&missing), p(&missing)], 4),
        (vec![p(&bad), p(&bad)], 3),
        (vec![p(&good), p(&missing)], 5),
        (vec![p(&missing), p(&bad)], 5),
    ] {
        let mut args = vec!["check"];
        args.extend(inputs);
        let out = cli(&args, b"");
        assert_eq!(out.status.code(), Some(code));
        let result = json(&out);
        for row in result["results"].as_array().unwrap() {
            if !row["ok"].as_bool().unwrap() {
                assert!([3, 4].contains(&row["exit_code"].as_u64().unwrap()));
            }
        }
    }
}

#[test]
fn network_editor_paths_keep_linked_corrections_and_prefix_event_domains() {
    let fixture = fixture();
    let mut image = Icf::parse(&fixture).unwrap();
    image
        .patch_memory(&[
            (0x3935a, &[1]),
            (0x3935b, &[192, 0, 2, 10, 24]),
            (0x39360, &[192, 0, 2, 1]),
        ])
        .unwrap();
    let source = image.to_bytes();
    let out = cli(
        &["edit", "-", "--set", "network/address=192.0.2.0", "-o", "-"],
        &source,
    );
    assert_memory_delta(
        &out,
        &source,
        &[(0x3935b, &[192, 0, 2, 1]), (0x39360, &[192, 0, 2, 2])],
    );
    let report: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(report["gray"], true);
    assert_eq!(report["notify_flags"], 0x20);
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "network/primary-dns=255.255.255.255",
            "-o",
            "-",
        ],
        &source,
    );
    assert_memory_delta(&out, &source, &[(0x39364, &[255, 255, 255, 254])]);
    for (key, phase, accepted) in [(0x31a, 2, true), (2, 1, true), (2, 2, false)] {
        let event = format!(
            "network/prefix={{\"key\":{key},\"phase\":{phase},\"candidate\":33,\"accepted\":{accepted}}}"
        );
        let out = cli(&["edit", "-", "--event", &event, "-o", "-"], &source);
        assert_not_published(&out);
    }
    let event = "network/prefix={\"key\":2,\"phase\":2,\"candidate\":289,\"accepted\":true}";
    let out = cli(&["edit", "-", "--event", event, "-o", "-"], &source);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        Icf::parse(&out.stdout)
            .unwrap()
            .network_configuration()
            .prefix,
        33
    );
    let typed = cli(
        &["edit", "-", "--set", "network/prefix=33", "-o", "-"],
        &source,
    );
    assert!(typed.status.success());
    assert_eq!(typed.stdout, out.stdout);
    let files = Files::new();
    let target = files.write("keep.icf", b"unchanged");
    for assignment in [
        "network/prefix=256",
        "network/address=\"1234567890123456\"",
        "network/gateway=\"\u{6771}\u{4eac}\"",
    ] {
        let out = cli(
            &[
                "edit",
                "-",
                "--set",
                "setting/beep_level_raw/0=91",
                "--set",
                assignment,
                "--overwrite",
                "-o",
                p(&target),
            ],
            &source,
        );
        assert_eq!(out.status.code(), Some(3));
        assert!(String::from_utf8_lossy(&out.stderr).contains("network"));
        assert!(out.stdout.is_empty());
        assert_eq!(fs::read(&target).unwrap(), b"unchanged");
    }
    let out = cli(
        &[
            "show",
            "-",
            "--key",
            "network/prefix",
            "--key",
            "network/address",
        ],
        &source,
    );
    assert!(out.status.success());
    let shown = json(&out);
    assert_eq!(shown[0]["value"], 24);
    assert_eq!(shown[1]["value"], "192.0.2.10");
}

#[test]
fn numeric_editor_paths_preserve_native_text_actions_and_selected_raw_bypass() {
    let bytes = fixture();
    let mut image = Icf::parse(&bytes).unwrap();
    image.patch_memory(&[(0x392f2, &[255])]).unwrap();
    let source = image.to_bytes();
    for (input, expected) in [
        ("1e-3", 1),
        ("1.8", 2),
        (r#"{"action":"text","text":"1.8","decimal":","}"#, 1),
        (r#"{"action":"next"}"#, 0),
    ] {
        let assignment = format!("number/beep_level_raw={input}");
        let out = cli(&["edit", "-", "--set", &assignment, "-o", "-"], &source);
        assert_memory_delta(&out, &source, &[(0x392f2, &[expected])]);
        let report: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(report["accepted"], true);
        assert_eq!(report["candidate"], expected);
    }
    for (action, state) in [("begin", 2), ("cancel", 0), ("display", 0)] {
        let event = format!("number/beep_level_raw={{\"action\":\"{action}\"}}");
        let out = cli(&["edit", "-", "--event", &event, "-o", "-"], &source);
        assert_not_published(&out);
        let report: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(report["state_out"], state);
        assert_eq!(report["accepted"], false);
    }
    let selected = r#"number/speech_compression_level={"action":"selected","key":1,"phase":2,"candidate":4660,"accepted":true}"#;
    let out = cli(&["edit", "-", "--event", selected, "-o", "-"], &source);
    assert_memory_delta(&out, &source, &[(0x3999a, &[0x34])]);
    let report: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(report["raw_candidate"], 4660);
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "number/utc_offset_minutes=+9:13",
            "-o",
            "-",
        ],
        &source,
    );
    assert_memory_delta(&out, &source, &[(0x396ff, &[0x2b, 0x02])]);
    let out = cli(&["show", "-", "--key", "number/beep_level_raw"], &source);
    assert!(out.status.success());
    assert_eq!(json(&out)[0]["value"]["value"], 255);
    for text in [
        r#"{"action":"next","extra":0}"#,
        r#"{"action":"text","text":"1","decimal":"xx"}"#,
        r#"{"action":"selected","key":1}"#,
        r#"{"action":"text","text":"\u6771"}"#,
    ] {
        let assignment = format!("number/beep_level_raw={text}");
        let out = cli(&["edit", "-", "--set", &assignment, "-o", "-"], &source);
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn indexed_number_choice_and_color_editors_keep_native_axes_and_effects() {
    let source = fixture();
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "number/scope_reference_tenths_db/16=-1.5",
            "-o",
            "-",
        ],
        &source,
    );
    assert_memory_delta(&out, &source, &[(0x377fb, &[185, 0])]);
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "choice/mode_select_cw/1=257",
            "-o",
            "-",
        ],
        &source,
    );
    assert_memory_delta(&out, &source, &[(0x3971b, &[1])]);
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "choice/picture_quality/0=2",
            "-o",
            "-",
        ],
        &source,
    );
    assert_memory_delta(&out, &source, &[(0x399f8, &[2]), (0x37d87, &[1])]);
    let out = cli(
        &[
            "edit",
            "-",
            "--event",
            r#"choice/notch_width_am/3={"action":"selected","key":0,"phase":2,"candidate":2,"accepted":true}"#,
            "-o",
            "-",
        ],
        &source,
    );
    assert_not_published(&out);
    let report: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(report["stored"], false);
    let out = cli(
        &[
            "edit",
            "-",
            "--event",
            r#"choice/agc/4={"action":"begin"}"#,
            "-o",
            "-",
        ],
        &source,
    );
    assert_not_published(&out);
    let report: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(report["fixed_label"], "FAST");
    assert_eq!(report["popup_requested"], false);
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "color/A2-H56095-11=[17,34,51]",
            "-o",
            "-",
        ],
        &source,
    );
    assert_memory_delta(&out, &source, &[(0x3972d, &[17, 34, 51])]);
    let out = cli(
        &["edit", "-", "--set", "color/A2-H56095-11=null", "-o", "-"],
        &source,
    );
    assert_not_published(&out);
    for assignment in [
        "number/scope_reference_tenths_db/17=0",
        "number/beep_level_raw/0/1=2",
        "choice/notch_width_am/0=1",
        "choice/filter_shape_hf_wide/2=1",
        "choice/A2-H56186-02/0=1",
        r#"choice/agc/0={"action":"next","extra":0}"#,
        "color/A2-H56095-11=[1,2,256]",
    ] {
        let out = cli(&["edit", "-", "--set", assignment, "-o", "-"], &source);
        assert_eq!(
            out.status.code(),
            Some(3),
            "{assignment}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
    }
    let out = cli(&["show", "-", "--key", "filter-view/0"], &source);
    assert!(out.status.success());
    assert!(json(&out)[0]["value"]["passband_hz"].is_i64());
}

#[test]
fn native_text_editors_filter_dtmf_aliases_and_keep_rtty_fixed_rows() {
    let source = fixture();
    let out = cli(
        &["edit", "-", "--set", r#"text/dtmf/15="ef-09""#, "-o", "-"],
        &source,
    );
    let mut expected = [0xff; 24];
    expected[..4].copy_from_slice(&[14, 15, 0, 9]);
    assert_memory_delta(&out, &source, &[(0x37720, &expected)]);
    let out = cli(
        &["edit", "-", "--set", r#"text/rtty/7="ab\\c_""#, "-o", "-"],
        &source,
    );
    let mut expected = [b' '; 70];
    expected[..4].copy_from_slice(b"AB\rC");
    assert_memory_delta(&out, &source, &[(0x37572, &expected)]);
    for assignment in [
        r#"text/dtmf/15={"action":"begin","first":49}"#,
        r#"text/rtty/7={"action":"cancel"}"#,
    ] {
        let out = cli(&["edit", "-", "--event", assignment, "-o", "-"], &source);
        assert_not_published(&out);
    }
    for assignment in [
        "text/dtmf/16=A",
        "text/rtty/8=A",
        r#"text/dtmf/0={"action":"begin","first":255}"#,
        r#"text/rtty/0={"action":"cancel","extra":true}"#,
        r#"text/dtmf/0="\u6771""#,
    ] {
        let out = cli(&["edit", "-", "--set", assignment, "-o", "-"], &source);
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn editor_json_type_detection_ignores_only_json_whitespace_without_rewriting_native_text() {
    let source = fixture();
    let files = Files::new();
    let output = files.write("preserved.icf", b"unchanged");
    let event = r#"{"action":"selected","key":1,"phase":2,"candidate":4660,"accepted":true}"#;
    for prefix in [" ", "\t", "\r", "\n", " \t\r\n"] {
        for flag in ["--set", "--event"] {
            let assignment = format!("number/speech_compression_level={prefix}{event}");
            let out = cli(&["edit", "-", flag, &assignment, "-o", "-"], &source);
            assert_memory_delta(&out, &source, &[(0x3999a, &[0x34])]);
        }
        let assignment = format!("number/beep_level_raw={prefix}\"1e-3\"");
        let out = cli(&["edit", "-", "--set", &assignment, "-o", "-"], &source);
        assert_memory_delta(&out, &source, &[(0x392f2, &[1])]);
        let assignment = format!("number/beep_level_raw={prefix}{{\"action\":\"cancel\"}}");
        let out = cli(&["edit", "-", "--event", &assignment, "-o", "-"], &source);
        assert_not_published(&out);
        for malformed in [
            r#"{"x":2}"#,
            r#"{"action":"next","extra":0}"#,
            r#"{"action":"selected"}"#,
            r#"{"action":"next""#,
        ] {
            let assignment = format!("number/beep_level_raw={prefix}{malformed}");
            let out = cli(
                &[
                    "edit",
                    "-",
                    "--set",
                    &assignment,
                    "--overwrite",
                    "-o",
                    p(&output),
                ],
                &source,
            );
            assert_eq!(out.status.code(), Some(3));
            assert!(out.stdout.is_empty());
            assert_eq!(fs::read(&output).unwrap(), b"unchanged");
        }
        for assignment in [
            format!("choice/picture_quality/0={prefix}{{\"action\":\"cancel\"}}"),
            format!("text/rtty/0={prefix}{{\"action\":\"cancel\"}}"),
        ] {
            let out = cli(&["edit", "-", "--event", &assignment, "-o", "-"], &source);
            assert_not_published(&out);
        }
        for malformed in ["{broken", "[broken", "\"broken"] {
            let assignment = format!("ntp-server={prefix}{malformed}");
            let out = cli(&["edit", "-", "--set", &assignment, "-o", "-"], &source);
            assert_eq!(out.status.code(), Some(3));
            assert!(out.stdout.is_empty());
        }
    }
}

#[test]
fn all_sheet_view_aggregates_every_registered_view_without_a_partial_fallback() {
    let files = Files::new();
    let q = files.write("q.raw", &[0; 24]);
    let original = fixture();
    let mut image = Icf::parse(&original).unwrap();
    image
        .patch_memory(&[
            (0x3ab82, &[0xff; 5100]),
            (0x3c385, &[0xff; 654]),
            (0x3c0a7, &[0xff; 600]),
        ])
        .unwrap();
    let source = image.to_bytes();
    let out = cli(
        &[
            "show",
            "-",
            "--sheet",
            "all",
            "--count",
            "1",
            "--record-context",
            p(&q),
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let aggregate = json(&out);
    let names = [
        "memory",
        "call",
        "bandstack",
        "memopad",
        "scan",
        "user-edge",
        "gps",
        "repeater",
        "from-dv",
        "from-fm",
        "to",
        "rx",
        "ur",
        "preset",
    ];
    assert_eq!(aggregate.as_object().unwrap().len(), names.len());
    for name in names {
        let single = cli(
            &[
                "show",
                "-",
                "--sheet",
                name,
                "--count",
                "1",
                "--record-context",
                p(&q),
            ],
            &source,
        );
        assert!(
            single.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&single.stderr)
        );
        assert_eq!(aggregate[name], json(&single), "{name}");
    }
    let out = cli(
        &[
            "export",
            "-",
            "--sheet",
            "all",
            "--format",
            "json",
            "--record-context",
            p(&q),
            "-o",
            "-",
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(json(&out).as_object().unwrap().len(), 14);
    let out = cli(
        &[
            "export", "-", "--sheet", "gps", "--group", "0", "--format", "json", "-o", "-",
        ],
        &source,
    );
    assert!(out.status.success());
    assert_eq!(json(&out), serde_json::json!([]));
    let target = files.write("all-preserved.json", b"unchanged");
    let out = cli(
        &[
            "export",
            "-",
            "--sheet",
            "all",
            "--format",
            "json",
            "--overwrite",
            "-o",
            p(&target),
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(6));
    assert!(out.stdout.is_empty());
    assert_eq!(fs::read(&target).unwrap(), b"unchanged");
    let out = cli(
        &[
            "export",
            "-",
            "--sheet",
            "gps",
            "--format",
            "csv",
            "--record-context",
            p(&q),
            "-o",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    let out = cli(&["show", "-", "--sheet", "all", "--count", "1"], &source);
    assert_eq!(out.status.code(), Some(6));
    assert!(out.stdout.is_empty());
    let out = cli(
        &[
            "show",
            "-",
            "--sheet",
            "all",
            "--start",
            &usize::MAX.to_string(),
            "--count",
            "2",
            "--record-context",
            p(&q),
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

fn with_context(patches: &[(usize, &[u8])]) -> Vec<u8> {
    let mut memory = Icf::parse(&fixture()).unwrap().memory().to_vec();
    for &(at, bytes) in patches {
        memory[at..at + bytes.len()].copy_from_slice(bytes);
    }
    Icf::from_memory(memory).unwrap().to_bytes()
}
fn assert_not_published(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
}
fn assert_memory_delta(output: &Output, source: &[u8], patches: &[(usize, &[u8])]) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let before = Icf::parse(source).unwrap();
    let after = Icf::parse(&output.stdout).unwrap();
    let mut expected = before.memory().to_vec();
    for &(at, bytes) in patches {
        expected[at..at + bytes.len()].copy_from_slice(bytes);
    }
    assert_eq!(after.memory(), expected);
    assert_eq!(after.comment_bytes(), before.comment_bytes());
    assert_eq!(after.extra_data(), before.extra_data());
}

#[test]
fn catalog_context_guards_and_selection_keys_have_literal_expectations() {
    use cs705::icf::settings;
    let files = Files::new();
    let listed = cli(&["show", "-", "--list-fields"], b"");
    let list = json(&listed);
    // Outer command keys are not popup-node keys. WLAN uses node 0x14; the
    // common selector and TwinPeak use node 1. These are source literals.
    for (id, outer, popup) in [
        ("A2-H54848-31", 0x215, 1),
        ("A2-H56202-01", 0x316, 0x14),
        ("A2-H56207-14", 0x2e0, 1),
    ] {
        let field = list
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["feature_id"] == id)
            .unwrap();
        assert_eq!(field["source_key"], outer);
        assert_eq!(field["selection_key"], popup);
    }
    assert!(
        !list
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["key"] == "rtty-twin-peak")
    );
    for (key, expected) in [(1, 0), (0x316, 0), (0x14, 1)] {
        let source = with_context(&[(0x39a05, &[1]), (0x39a27, b"1234567")]);
        let event = format!(
            "setting/A2-H56202-01/0={}",
            json!({"key":key,"phase":2,"candidate":1,"accepted":true})
        );
        let output = cli(&["edit", "-", "--event", &event, "--output", "-"], &source);
        if key == 0x14 {
            assert_memory_delta(&output, &source, &[(0x39359, &[expected])]);
        } else {
            assert_not_published(&output);
        }
    }
    // TwinPeak's helper is reached only for Mark=2 AND Shift=0. A selected
    // event on a disabled helper is a no-op; checked write must fail visibly.
    let twin = settings::find("A2-H56207-14").unwrap();
    assert_eq!(twin.domain.max, 1);
    for (mark, shift, enabled) in [(0, 0, false), (2, 1, false), (2, 0, true)] {
        let source = with_context(&[(0x39313, &[mark, shift])]);
        let event = format!(
            "setting/A2-H56207-14/0={}",
            json!({"key":1,"phase":2,"candidate":1,"accepted":true})
        );
        let output = cli(&["edit", "-", "--event", &event, "--output", "-"], &source);
        if enabled {
            assert_memory_delta(&output, &source, &[(0x399a0, &[1])]);
        } else {
            assert_not_published(&output);
        }
        let report: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(report["stored"], enabled);
        assert_eq!(report["notify_flags"], 0);
        if enabled {
            let output = cli(
                &[
                    "edit",
                    "-",
                    "--set",
                    "setting/A2-H56207-14/0=1",
                    "--output",
                    "-",
                ],
                &source,
            );
            assert_memory_delta(&output, &source, &[(0x399a0, &[1])]);
        } else {
            let existing = files.write(&format!("guard-{mark}-{shift}.icf"), b"unchanged");
            let output = cli(
                &[
                    "edit",
                    "-",
                    "--set",
                    "setting/beep_level_raw/0=91",
                    "--set",
                    "setting/A2-H56207-14/0=1",
                    "--output",
                    p(&existing),
                    "--overwrite",
                ],
                &source,
            );
            assert_eq!(output.status.code(), Some(3));
            assert!(output.stdout.is_empty());
            assert!(String::from_utf8_lossy(&output.stderr).contains("actual case guard"));
            assert_eq!(fs::read(existing).unwrap(), b"unchanged");
        }
    }
    // USB SEND clears only keying assignments equal to the new nonzero code.
    // Zero and nonmatching assignments survive. CW and RTTY may equal each
    // other: their choice filter excludes SEND, not their sibling's code.
    for (candidate, expected) in [(0, [0, 3, 4]), (3, [3, 0, 4]), (4, [4, 3, 0])] {
        let source = with_context(&[(0x39346, &[0, 3, 4])]);
        let assignment = format!("setting/A2-H56399-15/0={candidate}");
        let output = cli(
            &["edit", "-", "--set", &assignment, "--output", "-"],
            &source,
        );
        assert_memory_delta(&output, &source, &[(0x39346, &expected)]);
    }
    for (id, target) in [("A2-H56399-16", 0x39347), ("A2-H56399-17", 0x39348)] {
        for (current, candidate, allowed) in
            [(0, 3, false), (0, 0, true), (0, 4, true), (3, 3, true)]
        {
            let source = with_context(&[(0x39346, &[3, 0, 0]), (target, &[current])]);
            let assignment = format!("setting/{id}/0={candidate}");
            let existing = files.write(
                &format!("choice-{target}-{current}-{candidate}.icf"),
                b"unchanged",
            );
            let output = cli(
                &[
                    "edit",
                    "-",
                    "--set",
                    &assignment,
                    "--output",
                    p(&existing),
                    "--overwrite",
                ],
                &source,
            );
            if allowed {
                assert!(output.status.success());
                let bytes = fs::read(existing).unwrap();
                let mut expected = Icf::parse(&source).unwrap().memory().to_vec();
                expected[target] = candidate;
                assert_eq!(Icf::parse(&bytes).unwrap().memory(), expected);
            } else {
                assert_eq!(output.status.code(), Some(3));
                assert!(output.stdout.is_empty());
                assert!(
                    String::from_utf8_lossy(&output.stderr)
                        .contains("filtered in the current context")
                );
                assert_eq!(fs::read(existing).unwrap(), b"unchanged");
            }
        }
    }
    let source = with_context(&[(0x39346, &[0, 3, 4])]);
    let event = format!(
        "setting/A2-H56399-15/0={}",
        json!({"key":1,"phase":2,"candidate":3,"accepted":true})
    );
    assert_memory_delta(
        &cli(&["edit", "-", "--event", &event, "--output", "-"], &source),
        &source,
        &[(0x39346, &[3, 0, 4])],
    );
    // Gray GPS output is presentation only, unlike the disabled TwinPeak helper.
    let source = with_context(&[(0x39353, &[1])]);
    assert_memory_delta(
        &cli(
            &[
                "edit",
                "-",
                "--set",
                "setting/A2-H56399-25/0=1",
                "--output",
                "-",
            ],
            &source,
        ),
        &source,
        &[(0x39355, &[1])],
    );
}

#[test]
fn mock_query_and_uncertain_write_failure_are_not_retried() {
    let files = Files::new();
    let mut response = vec![0xfe, 0xfe, 0xef, 0xee, 0xe1, 0x40, 0x75, 0, 1];
    let mut tail = [0u8; 33];
    tail[0] = 33;
    tail[1] = 6;
    response.extend(tail);
    response.push(0xfd);
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
    let events =
        json!({"events":[{"write_hex":"FEFEEEEFE040750000FD"},{"read_hex":hex(&response)}]});
    let mock = files.write("query.json", &serde_json::to_vec(&events).unwrap());
    let out = cli(
        &["clone", "query", "--mock", p(&mock), "--echo", "none"],
        b"",
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(json(&out)["identity"]["profile"], 6);
    let events = json!({"events":[{"write_hex":"FEFEEEEFE040750000FD"},{"read_hex":hex(&response)},{"write_error":true}]});
    let mock = files.write("write-error.json", &serde_json::to_vec(&events).unwrap());
    let image = files.write("image.bin", &vec![0; cs705::clone::IMAGE_CAPACITY]);
    let out = cli(
        &[
            "clone",
            "raw-write",
            p(&image),
            "--mock",
            p(&mock),
            "--profile",
            "6",
            "--echo",
            "none",
            "--yes",
            "--output",
            "-",
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(5));
    assert_eq!(json(&out)["write_disposition"], "Unconfirmed");
    assert_eq!(json(&out)["failure"], "Io(BrokenPipe)");
    assert_eq!(json(&out)["transmitted_wire_bytes"], 10);
}

fn topology_fixture() -> Vec<u8> {
    let mut memory = Icf::parse(&fixture()).unwrap().memory().to_vec();
    memory[0x3662d..0x36821].fill(0xff);
    memory[0x3662d] = 0xfe;
    memory[0x3662e] = 0xfe;
    memory[0x36114..0x36628].fill(0xff);
    memory[0x36114] = 0xfc;
    memory[0x35d2c..0x36114].fill(0xff);
    memory[0x35d2c..0x35d2e].copy_from_slice(&1u16.to_le_bytes());
    for g in 0..100 {
        let at = 0x355c0 + 19 * g;
        memory[at] = if g == 99 { 0xff } else { g as u8 + 1 };
        memory[at + 1..at + 3].fill(0xff);
        memory[at + 3..at + 19].fill(b' ');
    }
    memory[0x36628] = 0;
    memory[0x355c1..0x355c3].fill(0);
    memory[0x355c3] = b'G';
    memory[83 + 0x43..83 + 0x53].fill(b' ');
    memory[83 + 0x43] = b'B';
    String::from_utf8(Icf::from_memory(memory).unwrap().to_bytes())
        .unwrap()
        .replace("#Comment=\r\n", "#Comment=KEEP\r\n")
        .into_bytes()
}

#[test]
fn normal_memory_topology_commands() {
    let files = Files::new();
    let source = topology_fixture();
    let before = Icf::parse(&source).unwrap();
    let mut generation = before.memory().to_vec();
    generation.resize(cs705::clone::IMAGE_CAPACITY, 0xa7);
    generation[0x36821..0x36823].copy_from_slice(&0xbeefu16.to_le_bytes());
    let generation = files.write("generation.raw", &generation);
    assert_eq!(
        before.memory_groups().unwrap()[0].slots()[..2],
        [Some(0), Some(1)]
    );
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--generation",
            p(&generation),
            "--show-both",
            "false",
            "move",
            "0",
            "0",
            "--destination-group",
            "0",
            "--destination-index",
            "1",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = Icf::parse(&out.stdout).unwrap();
    assert_eq!(
        after.memory_groups().unwrap()[0].slots()[..2],
        [Some(1), Some(0)]
    );
    assert_eq!(after.comment_bytes(), b"KEEP");
    assert_eq!(&after.memory()[0x36821..0x36823], &0xbeefu16.to_le_bytes());
    assert_eq!(&after.memory()[..166], &before.memory()[..166]);
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--generation",
            p(&generation),
            "delete",
            "0",
            "0",
            "--yes",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success());
    assert_eq!(
        Icf::parse(&out.stdout).unwrap().memory_groups().unwrap()[0].slots()[..2],
        [Some(1), None]
    );
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--generation",
            p(&generation),
            "insert",
            "0",
            "0",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success());
    assert_eq!(
        Icf::parse(&out.stdout).unwrap().memory_groups().unwrap()[0].slots()[..3],
        [None, Some(0), Some(1)]
    );
    let out = cli(
        &[
            "group",
            "-",
            "--sheet",
            "memory",
            "rename",
            "0",
            "NEW GROUP",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success());
    assert_eq!(
        Icf::parse(&out.stdout).unwrap().memory_groups().unwrap()[0].name_bytes(),
        b"NEW GROUP       "
    );
    let out = cli(
        &[
            "group", "-", "--sheet", "memory", "add", "--name", "NEXT", "--output", "-",
        ],
        &source,
    );
    assert!(out.status.success());
    assert_eq!(
        Icf::parse(&out.stdout)
            .unwrap()
            .memory_groups()
            .unwrap()
            .len(),
        2
    );
    let out = cli(&["group", "-", "--sheet", "memory", "list"], &source);
    assert!(out.status.success());
    assert_eq!(json(&out)[0]["physical_slots"][0], 0);
}

#[test]
fn source_reader_keeps_its_separate_contract() {
    let files = Files::new();
    let seed = files.write("seed.bin", &vec![0x55; cs705::clone::IMAGE_CAPACITY]);
    let source=b"40750001\r\n#MapRev=6\r\n#EtcData=00000000\r\n0000000001AA\r\n#CD=not-checked-by-source\r\n";
    let out = cli(
        &["parse", "-", "--source-reader", "--seed", p(&seed)],
        source,
    );
    assert!(out.status.success());
    assert_eq!(json(&out)["bytes_written"], 1);
    assert_eq!(json(&out)["checksum_validated"], false);
    assert_eq!(json(&out)["source_status"], 0);
    assert_eq!(cli(&["parse", "-"], source).status.code(), Some(3));
    let out = cli(
        &["parse", "-", "--source-reader", "--seed", p(&seed)],
        b"broken",
    );
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(json(&out)["source_status"], 6);
}

#[test]
fn all_representational_conversions_are_distinct_from_native_document_operations() {
    let source = fixture();
    let out = cli(
        &[
            "convert",
            "-",
            "--conversion",
            "icf-to-memory",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success());
    let memory = out.stdout;
    let out = cli(
        &[
            "convert",
            "-",
            "--conversion",
            "memory-to-icf",
            "--output",
            "-",
        ],
        &memory,
    );
    assert!(out.status.success());
    assert_eq!(out.stdout, source);
    let mut working = memory;
    working.resize(cs705::clone::IMAGE_CAPACITY, 0x77);
    working[0x399bb] = 0xfe;
    working[0x399c0] = 0xfe;
    let out = cli(
        &[
            "convert",
            "-",
            "--conversion",
            "profile",
            "--source-profile",
            "6",
            "--destination-profile",
            "5",
            "--output",
            "-",
        ],
        &working,
    );
    assert!(out.status.success());
    assert_eq!(out.stdout[0x355a1], 5);
    assert_eq!(out.stdout[0x399bb], 1);
    assert_eq!(out.stdout[0x399c0], 2);
    assert_eq!(working[0x355a1], 6);
}

#[test]
fn gps_row_clipboard_and_template_operations() {
    let files = Files::new();
    let mut record = [0u8; 34];
    record[3] = b'N';
    record[7] = b'E';
    record[8..14].copy_from_slice(&[1, 1, 0x26, 0x12, 0x34, 0x56]);
    record[14..18].copy_from_slice(&i32::MAX.to_le_bytes());
    record[18..].fill(b' ');
    record[18] = b'A';
    let seed=files.write("gps-seed.json",&serde_json::to_vec(&json!({"version":1,"sheet":"gps","records":[{"valid":true,"bytes":record.as_slice()}]})).unwrap());
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "gps",
            "insert",
            "1",
            "0",
            "--seed",
            p(&seed),
            "--output",
            "-",
        ],
        &fixture(),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let inserted = out.stdout;
    assert_eq!(
        Icf::parse(&inserted).unwrap().gps_groups().unwrap()[1].physical_slots(),
        [0]
    );
    let out = cli(
        &[
            "row", "-", "--sheet", "gps", "copy", "1", "0", "--output", "-",
        ],
        &inserted,
    );
    assert!(out.status.success());
    assert_eq!(json(&out)["records"][0]["bytes"][18], b'A');
    let clipboard = files.write("clipboard.json", &out.stdout);
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "gps",
            "paste",
            "1",
            "0",
            "--clipboard",
            p(&clipboard),
            "--yes",
            "--output",
            "-",
        ],
        &inserted,
    );
    assert!(out.status.success());
    assert_eq!(out.stdout, inserted);
    let out = cli(
        &[
            "row", "-", "--sheet", "gps", "delete", "1", "0", "--yes", "--output", "-",
        ],
        &inserted,
    );
    assert!(out.status.success());
    assert!(
        Icf::parse(&out.stdout).unwrap().gps_groups().unwrap()[1]
            .physical_slots()
            .is_empty()
    );
    assert_eq!(
        &Icf::parse(&out.stdout).unwrap().memory()[0x32c19..0x32c19 + 34],
        &record
    );
}

#[test]
fn text_memory_checked_roundtrip() {
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "keyer-memory/0=CQ TEST",
            "--set",
            "rtty-memory/0=HELLO",
            "--set",
            "dtmf-memory/0=123#",
            "--output",
            "-",
        ],
        &fixture(),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let shown = cli(
        &[
            "show",
            "-",
            "--key",
            "keyer-memory/0",
            "--key",
            "rtty-memory/0",
            "--key",
            "dtmf-memory/0",
        ],
        &out.stdout,
    );
    assert!(shown.status.success());
    assert!(
        json(&shown)[0]["value"]["utf8"]
            .as_str()
            .unwrap()
            .starts_with("CQ TEST")
    );
}

#[test]
fn rf_fields_preserve_opaque_neighbors() {
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "filter-bandwidth-hz/Ssb/One=1200",
            "--set",
            "filter-pbt-hz/Ssb/One/One=100",
            "--set",
            "preamp/0=Amp1",
            "--set",
            "attenuator/0=true",
            "--set",
            "filter-shape/Hf/Ssb=Soft",
            "--set",
            "tuning-step-code/Fm=5",
            "--set",
            "air-tuning-step-code=4",
            "--set",
            "agc/Ssb=Mid",
            "--set",
            "agc-delay-code/Ssb/Mid=13",
            "--set",
            "receive-high-pass-code/Ssb=3",
            "--set",
            "receive-low-pass-code/Ssb=10",
            "--set",
            "receive-bass/Ssb=-3",
            "--set",
            "transmit-bandwidth-low/Wide=Hz100",
            "--set",
            "transmit-bandwidth-high/Wide=Hz2900",
            "--set",
            "swr-graph-step-index/0=1",
            "--set",
            "swr-graph-bar-index/0=2",
            "--set",
            "scope-reference-level-tenths-db/0=-15",
            "--output",
            "-",
        ],
        &fixture(),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let image = Icf::parse(&out.stdout).unwrap();
    assert_eq!(image.memory()[0x37867], 2);
    assert_eq!(image.memory()[0x3785f], 1);
    assert_eq!(image.memory()[0x392da], 3);
    assert_eq!(image.memory()[0x392e2], 10);
    let out = cli(
        &[
            "show",
            "-",
            "--key",
            "filter-passband-hz/Ssb/One",
            "--key",
            "filter-shift-hz/Ssb/One",
        ],
        &out.stdout,
    );
    assert!(out.status.success());
}

#[test]
fn clone_mask_and_search_mock() {
    let base = cli(
        &["clone", "plan", "--profile", "6", "--write", "--mask", "0"],
        b"",
    );
    let all = cli(
        &["clone", "plan", "--profile", "6", "--write", "--mask", "31"],
        b"",
    );
    assert!(base.status.success() && all.status.success());
    assert_ne!(
        json(&base)["blocks"].as_array().unwrap().len(),
        json(&all)["blocks"].as_array().unwrap().len()
    );
    let files = Files::new();
    let mut response = vec![0xfe, 0xfe, 0xef, 0xee, 0xe1, 0x40, 0x75, 0, 1];
    let mut tail = [0u8; 33];
    tail[0] = 33;
    tail[1] = 6;
    response.extend(tail);
    response.push(0xfd);
    let hex: String = response.iter().map(|b| format!("{b:02X}")).collect();
    let mock = files.write(
        "search.json",
        &serde_json::to_vec(
            &json!({"events":[{"write_hex":"FEFEEEEFE040750000FD"},{"read_hex":hex}]}),
        )
        .unwrap(),
    );
    let out = cli(&["clone", "search", "--mock", p(&mock), "--yes"], b"");
    assert!(out.status.success());
    assert_eq!(json(&out)["failed"], 0);
}

#[test]
fn mock_complete_read_reports_exact_acquired_coverage() {
    use cs705::clone::{Direction, Frame, Identity};
    let files = Files::new();
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
    let mut tail = vec![0; 33];
    tail[0] = 33;
    tail[1] = 1;
    let response = Frame::Response(Identity {
        model: 0x4075,
        revision: 1,
        tail,
    })
    .encode(Direction::FromRadio)
    .unwrap();
    let prefix = vec![
        json!({"write_hex":"FEFEEEEFE040750000FD"}),
        json!({"read_hex":hex(&response)}),
        json!({"write_hex":"FEFEEEEFE240750000FD"}),
    ];
    let mut events = prefix.clone();
    let end = 0x3ad00u32;
    for address in (0..end).step_by(64) {
        let frame = Frame::Data {
            address,
            payload: (0..64).map(|n| (address as usize / 64 + n) as u8).collect(),
        };
        events.push(json!({"read_hex":hex(&frame.encode(Direction::FromRadio).unwrap())}));
    }
    let blocks = end / 64;
    events.push(json!({"read_hex":hex(&Frame::End{block_count:blocks as u8}.encode(Direction::FromRadio).unwrap())}));
    let mock = files.write(
        "read.json",
        &serde_json::to_vec(&json!({"events":events})).unwrap(),
    );
    let output = cli(
        &[
            "clone",
            "raw-read",
            "--mock",
            p(&mock),
            "--profile",
            "1",
            "--echo",
            "none",
            "--output",
            "-",
        ],
        b"",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value = json(&output);
    assert_eq!(value["blocks"], blocks);
    let coverage = value["coverage"]["hex"].as_str().unwrap();
    let active = (end as usize).div_ceil(8) * 2;
    assert!(coverage[..active].bytes().all(|b| b == b'F'));
    assert!(coverage[active..].bytes().all(|b| b == b'0'));
    let image = value["image"]["hex"].as_str().unwrap();
    assert!(image.starts_with("000102030405060708090A0B0C0D0E0F"));
    assert_eq!(image.len(), cs705::clone::IMAGE_CAPACITY * 2);
    let mut events = prefix;
    events.push(json!({"read_hex":hex(&Frame::Data{address:0,payload:vec![0x42;64]}.encode(Direction::FromRadio).unwrap())}));
    events.push(
        json!({"read_hex":hex(&Frame::End{block_count:1}.encode(Direction::FromRadio).unwrap())}),
    );
    let mock = files.write(
        "partial.json",
        &serde_json::to_vec(&json!({"events":events})).unwrap(),
    );
    let existing = files.write("existing.json", b"preserved");
    let out = cli(
        &[
            "clone",
            "raw-read",
            "--mock",
            p(&mock),
            "--profile",
            "1",
            "--echo",
            "none",
            "--output",
            p(&existing),
            "--overwrite",
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(5));
    assert_eq!(fs::read(existing).unwrap(), b"preserved");
}

#[test]
fn repeater_record_checked_edit_is_atomic() {
    let mut source = Icf::parse(&fixture()).unwrap().memory().to_vec();
    source[0x3ab82..0x3ab84].fill(0);
    source[0x3bf0a..0x3bf0c].fill(0);
    source[0xaa0f + 8..0xaa0f + 10].fill(0xff);
    let source = Icf::from_memory(source).unwrap().to_bytes();
    let name = [b'A'; 16];
    let blank = [b' '; 8];
    let record = json!({"name":name,"sub_name":blank,"repeater_call":blank,"gateway_call":blank,"frequency_hz":145_000_000,"offset_hz":0,"mode":"Fm","duplex":"Off","tone":"Off","tone_index":0,"use_repeater":true,"position":"Exact","latitude":{"degrees":35,"thousandths_of_minute":30000,"negative":false},"longitude":{"degrees":139,"thousandths_of_minute":45000,"negative":false},"utc_offset_minutes":540});
    let assignment = format!("repeater/0={record}");
    let out = cli(
        &["edit", "-", "--set", &assignment, "--output", "-"],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let image = Icf::parse(&out.stdout).unwrap();
    assert_eq!(
        &image.memory()[0xaa0f..0xaa13],
        &145_000_000u32.to_le_bytes()
    );
    assert_eq!(&image.memory()[0xaa0f + 18..0xaa0f + 34], &[b'A'; 16]);
    let shown = cli(&["show", "-", "--key", "repeater/0"], &out.stdout);
    assert!(shown.status.success());
    assert_eq!(json(&shown)[0]["value"]["frequency_hz"], 145_000_000);
    let mut invalid = record;
    invalid["latitude"]["degrees"] = 91.into();
    let assignment = format!("repeater/0={invalid}");
    assert_eq!(
        cli(
            &["edit", "-", "--set", &assignment, "--output", "-"],
            &source
        )
        .status
        .code(),
        Some(3)
    );
}

#[test]
fn sheet_export_reports_are_machine_and_terminal_safe() {
    let files = Files::new();
    let q = files.write("q.raw", &[0; 24]);
    let out = cli(
        &[
            "export", "-", "--sheet", "memory", "--format", "json", "--output", "-",
        ],
        &fixture(),
    );
    assert!(out.status.success());
    assert_eq!(json(&out)[0]["name"]["utf8"], "TEST            ");
    let out = cli(
        &[
            "export",
            "-",
            "--sheet",
            "all",
            "--format",
            "text",
            "--record-context",
            p(&q),
            "--output",
            "-",
        ],
        &fixture(),
    );
    assert!(out.status.success());
    assert!(!out.stdout.contains(&0x1b));
}

#[test]
fn raw_memory_queries_share_the_checked_image_boundary_and_semantic_views() {
    let text = fixture();
    let image = Icf::parse(&text).unwrap();
    let raw = image.memory();
    for args in [
        vec!["--key", "filter-view/0"],
        vec!["--sheet", "memory", "--count", "1"],
    ] {
        let mut command = vec!["show", "-"];
        command.extend(args.iter().copied());
        let expected = cli(&command, &text);
        assert!(expected.status.success());
        command.extend(["--kind", "memory"]);
        let actual = cli(&command, raw);
        assert!(actual.status.success());
        assert_eq!(json(&actual), json(&expected));
    }
    for raw in [&raw[..raw.len() - 1], b"not an image".as_slice()] {
        let out = cli(
            &["show", "-", "--kind", "memory", "--key", "filter-view/0"],
            raw,
        );
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn unused_normal_state_is_rejected_before_reading_or_publishing() {
    for args in [
        vec![
            "show",
            "-",
            "--normal-state",
            "absent-state",
            "--list-fields",
        ],
        vec![
            "show",
            "-",
            "--normal-state",
            "absent-state",
            "--sheet",
            "memory",
        ],
        vec![
            "edit",
            "-",
            "--normal-state",
            "absent-state",
            "--set",
            "comment=NEW",
            "--output",
            "-",
        ],
        vec![
            "export",
            "-",
            "--normal-state",
            "absent-state",
            "--sheet",
            "memory",
            "--format",
            "json",
            "--output",
            "-",
        ],
        vec![
            "row",
            "-",
            "--sheet",
            "memory",
            "--normal-state",
            "absent-state",
            "--show-both",
            "false",
            "copy",
            "0",
            "0",
            "--output",
            "-",
        ],
        vec![
            "row",
            "-",
            "--sheet",
            "ur",
            "--normal-state",
            "absent-state",
            "copy",
            "0",
            "0",
            "--output",
            "-",
        ],
    ] {
        let out = cli(&args, b"not an image");
        assert_eq!(
            out.status.code(),
            Some(3),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains("normal-state requires a native channel field edit")
        );
    }
}

#[test]
fn explicit_preferences_apply_to_terminal_projection_and_recent_requires_native_sources() {
    let files = Files::new();
    let source = fixture();
    let recent = files.write("recent.icf", &source);
    let preferences=files.write("prefs.json",&serde_json::to_vec(&json!({"fixed-name":true,"show-split":false,"show-sql":false,"text-scale":2,"toolbar":true,"recent-files":[p(&recent)]})).unwrap());
    let out = cli(
        &[
            "--config",
            p(&preferences),
            "show",
            "-",
            "--sheet",
            "memory",
            "--count",
            "1",
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value = json(&out);
    assert_eq!(value[0][0]["column"], "name");
    assert!(!value[0].as_array().unwrap().iter().any(|v| {
        v["column"]
            .as_str()
            .is_some_and(|s| s.contains("tone") || s.contains("dtcs") || s.contains("split"))
    }));
    assert!(String::from_utf8_lossy(&out.stderr).contains("commands"));
    let out = cli(
        &[
            "--config",
            p(&preferences),
            "show",
            "-",
            "--sheet",
            "memory",
            "--count",
            "1",
            "--format",
            "text",
        ],
        &source,
    );
    assert!(out.status.success());
    assert!(out.stdout.ends_with(b"\n\n"));
    assert!(!out.stdout.contains(&0x1b));
    // Recent now runs native Open, so a path list alone is not a document source.
    // The complete MRU/Open flow is checked with the lifecycle source fixtures.
    for index in ["0", "1"] {
        let out = cli(&["--config", p(&preferences), "recent", index], b"");
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn clone_cancellation_and_output_preflight_happen_before_any_send() {
    let files = Files::new();
    let mock = files.write("empty.json", br#"{"events":[]}"#);
    let out = cli(
        &[
            "clone",
            "query",
            "--mock",
            p(&mock),
            "--cancel-after-ms",
            "0",
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(5));
    assert_eq!(json(&out)["failure"], "Cancelled");
    assert_eq!(json(&out)["transmitted_wire_bytes"], 0);
    let existing = files.write("report.json", b"original");
    let out = cli(
        &[
            "clone",
            "raw-write",
            "-",
            "--mock",
            p(&files.path("must-not-be-read.json")),
            "--profile",
            "6",
            "--yes",
            "--output",
            p(&existing),
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&out.stderr).contains("output exists"));
    assert_eq!(fs::read(existing).unwrap(), b"original");
}
