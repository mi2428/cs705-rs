//! Offline process consumers using independent physical-text input fixtures.
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
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "documents-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        fs::write(&p, bytes).unwrap();
        p
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
fn cli(args: &[String], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cs705"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Err(e) = child.stdin.take().unwrap().write_all(stdin) {
        assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe);
    }
    child.wait_with_output().unwrap()
}
fn fixture() -> Vec<u8> {
    let mut m = vec![0; 0x3c9c0];
    m[0x355a1] = 6;
    for g in 0..100 {
        let a = 0x355c0 + 19 * g;
        m[a] = if g == 99 { 255 } else { g as u8 + 1 };
        m[a + 1..a + 3].fill(255);
        m[a + 3..a + 19].fill(b' ');
    }
    m[0x35d2c..0x36628].fill(255);
    m[0x3662d..0x36821].fill(1);
    m[0x3ab82..0x3bf6e].fill(255);
    m[0x3c0a7..0x3c2ff].fill(255);
    m[0x3c385..0x3c613].fill(255);
    // Native band-stack physical order is immaterial when every row is empty.
    for i in 0..45 {
        m[0x368eb + 36 * i..0x368ef + 36 * i].fill(255);
    }
    for i in 0..11 {
        m[0x36f54 + 36 * i..0x36f58 + 36 * i].fill(255);
    }
    m[0xa60b..0xa60f].fill(255);
    m[0x3c84b] = 255;
    m[0x3c8b9..0x3c8bb].fill(255);
    m[0x399c1..0x399dd].copy_from_slice(b"0123456789ABCDEFGHIJKLMNOPQR");
    m[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    m
}
fn literal(m: &[u8], extra: u32) -> Vec<u8> {
    let mut out = format!("40750001\r\n#Comment=DOCTEST\r\n#MapRev=6\r\n#EtcData={extra:08X}\r\n")
        .into_bytes();
    for (i, chunk) in m.chunks(32).enumerate() {
        out.extend_from_slice(format!("{:08X}20", i * 32).as_bytes());
        for b in chunk {
            out.extend_from_slice(format!("{b:02X}").as_bytes());
        }
        out.extend_from_slice(b"\r\n");
    }
    out
}
fn context(kind: u8) -> Vec<u8> {
    let mut raw = [0u8; 24];
    raw[0] = kind;
    let hex: String = raw.iter().map(|b| format!("{b:02X}")).collect();
    serde_json::to_vec(&json!({"generation_q_hex":hex,"live_q_hex":hex})).unwrap()
}
fn options(ctx: &Path, source: &Path) -> Vec<String> {
    vec![
        "--range-context".into(),
        p(ctx).into(),
        "--constructor-current".into(),
        "--regenerate-defaults".into(),
        "--source-asset".into(),
        format!("1={}", p(source)),
        "--previous-reload-guard".into(),
        "1".into(),
        "--document-flags".into(),
        "0100".into(),
    ]
}
fn args(action: &str, input: Option<&Path>, opts: &[String], out: &Path) -> Vec<String> {
    let mut a = vec!["document".into(), action.into()];
    if let Some(i) = input {
        a.push(p(i).into());
    }
    a.extend_from_slice(opts);
    a.extend(["-o".into(), p(out).into()]);
    a
}
fn success(o: &Output) {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn logic_phase_initial_icf_adopts_generated_g_not_file_payload() {
    let f = Files::new();
    let mut fields = fixture();
    fields[0x399a2] = 0x15;
    let source = f.write("initial-asset.raw", &fields);
    let mut input = fixture();
    input[0x399a2] = 0xa7;
    let input = f.write("initial.icf", &literal(&input, 0x40001));
    let ctx = f.write("initial-context.json", &context(1));
    let opts = options(&ctx, &source);
    let output = f.path("initial-output.icf");
    let result = cli(&args("initial", Some(&input), &opts, &output), &[]);
    success(&result);
    let report: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(report["status"], "initial-adopted");
    assert_eq!(report["dirty"], 0);
    assert_eq!(report["effects"]["notifications_delivered"], false);
    let bytes = fs::read(output).unwrap();
    assert_eq!(Icf::parse(&bytes).unwrap().memory()[0x399a2], 0x15);
}

#[test]
fn literal_open_edit_legacy_save_and_reopen_use_actual_document_driver() {
    let f = Files::new();
    let raw = fixture();
    let source = f.write("pre24A20.raw", &raw);
    let input = f.write("literal.icf", &literal(&raw, 0x40001));
    let ctx = f.write("context.json", &context(1));
    let opts = options(&ctx, &source);
    let opened = f.path("opened.icf");
    success(&cli(&args("open", Some(&input), &opts, &opened), &[]));
    let opened_bytes = fs::read(&opened).unwrap();
    let image = Icf::parse(&opened_bytes).unwrap();
    assert_eq!(image.comment_bytes(), b"DOCTEST");
    assert_eq!(image.extra_data(), 0x40001);
    let saved = f.path("legacy.icf");
    let mut a = args("save", Some(&opened), &opts, &saved);
    a.extend([
        "--set".into(),
        "noise_blanker_depth=7".into(),
        "--target-revision".into(),
        "5".into(),
        "--legacy-choice".into(),
        "yes".into(),
    ]);
    assert_eq!(cli(&a[..a.len() - 2], &[]).status.code(), Some(3));
    assert!(!saved.exists()); // Choosing a legacy target is not an implicit Yes.
    let result = cli(&a, &[]);
    success(&result);
    let report: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(report["status"], "saved");
    assert_eq!(report["published"], true);
    assert_eq!(report["dirty"], 0);
    assert_eq!(report["timer_ms"], 250);
    let bytes = fs::read(&saved).unwrap();
    let file = cs705::icf::format::read(&bytes).unwrap();
    assert_eq!(file.reader.map_revision, 5);
    assert_eq!(file.image[0x399a2], 6);
    let reopened = f.path("reopened.icf");
    success(&cli(&args("open", Some(&saved), &opts, &reopened), &[]));
    let bytes = fs::read(&reopened).unwrap();
    let image = Icf::parse(&bytes).unwrap();
    assert_eq!(image.memory()[0x399a2], 6);
    assert_eq!(
        &image.memory()[0x399c1..0x399dd],
        b"0123456789ABCDEFGHIJKLMNOPQR"
    );
    assert_eq!(&image.memory()[0x3c9b0..], b"IcomCloneFormat3");
    assert_eq!(image.comment_bytes(), b"DOCTEST");
}

#[test]
fn new_generation_cancellation_and_missing_sources_do_not_publish_success_defaults() {
    let f = Files::new();
    let source = f.write("asset.raw", &fixture());
    let ctx = f.write("context.json", &context(1));
    let opts = options(&ctx, &source);
    let out = f.path("new.icf");
    success(&cli(&args("new", None, &opts, &out), &[]));
    assert_eq!(
        Icf::parse(&fs::read(&out).unwrap()).unwrap().extra_data(),
        0x40001
    );
    let short = f.write("short.raw", b"SHORT");
    let out = f.path("missing.icf");
    let result = cli(&args("new", None, &options(&ctx, &short), &out), &[]);
    assert_eq!(result.status.code(), Some(3));
    assert!(!out.exists());
    let mut a = args(
        "new",
        None,
        &options(&f.path("no-context"), &f.path("no-source")),
        &out,
    );
    a.push("--cancel".into());
    let result = cli(&a, &[]);
    success(&result);
    assert!(!out.exists());
    assert!(String::from_utf8_lossy(&result.stderr).contains("\"inputs_read\": false"));
    for command in [
        vec!["new"],
        vec!["document", "new"],
        vec!["document", "open"],
        vec!["document", "normalize"],
        vec!["document", "save"],
        vec!["document", "reset"],
    ] {
        let mut args: Vec<String> = command.into_iter().map(str::to_owned).collect();
        args.push("--cancel".into());
        let result = cli(&args, &[]);
        success(&result);
        assert!(result.stdout.is_empty());
        let report: Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(report["status"], "cancelled");
        assert_eq!(report["inputs_read"], false);
    }
}

#[test]
fn profile_foreign_cancel_and_yes_require_the_actual_adopted_source_branch() {
    let f = Files::new();
    let source = f.write("asset.raw", &fixture());
    let input = f.write("profile0.icf", &literal(&fixture(), 0));
    let ctx = f.write("context.json", &context(1));
    let opts = options(&ctx, &source);
    let out = f.path("foreign.icf");
    let mut a = args("open", Some(&input), &opts, &out);
    a.extend(["--profile-choice".into(), "no".into()]);
    let result = cli(&a, &[]);
    success(&result);
    let report: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(report["dirty"], 1);
    assert_eq!(report["path"]["hex"], "");
    assert_eq!(
        Icf::parse(&fs::read(&out).unwrap()).unwrap().extra_data(),
        0x40001
    );
    let out = f.path("cancel.icf");
    let mut a = args("open", Some(&input), &opts, &out);
    a.extend(["--profile-choice".into(), "cancel".into()]);
    success(&cli(&a, &[]));
    assert!(!out.exists());
    let out = f.path("adopt.icf");
    let mut a = args("open", Some(&input), &opts, &out);
    a.extend(["--profile-choice".into(), "yes".into()]);
    assert_eq!(cli(&a, &[]).status.code(), Some(3));
    assert!(!out.exists());
    let patch = f.write("mode-pairs.raw", &[0; 120]);
    a.extend([
        "--source-asset".into(),
        format!("0={}", p(&source)),
        "--japanese-mode-pairs".into(),
        p(&patch).into(),
    ]);
    let result = cli(&a, &[]);
    success(&result);
    let report: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(report["default_selector"], 0);
    assert_eq!(report["dirty"], 0);
}

#[test]
fn no_clobber_dry_run_stdin_competition_and_broken_output_never_report_saved() {
    let f = Files::new();
    let source = f.write("asset.raw", &fixture());
    let input = f.write("input.icf", &literal(&fixture(), 0x40001));
    let ctx = f.write("context.json", &context(1));
    let opts = options(&ctx, &source);
    let out = f.write("existing.icf", b"KEEP");
    let a = args("save", Some(&input), &opts, &out);
    assert_eq!(cli(&a, &[]).status.code(), Some(4));
    assert_eq!(fs::read(&out).unwrap(), b"KEEP");
    let mut a = a.clone();
    a.push("--dry-run".into());
    let result = cli(&a, &[]);
    success(&result);
    assert_eq!(fs::read(&out).unwrap(), b"KEEP");
    assert!(!String::from_utf8_lossy(&result.stderr).contains("\"status\": \"saved\""));
    let mut opts = opts.clone();
    let pos = opts.iter().position(|a| a == "--range-context").unwrap() + 1;
    opts[pos] = "-".into();
    let result = cli(
        &args("save", Some(Path::new("-")), &opts, &f.path("never")),
        &[],
    );
    assert_eq!(result.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&result.stderr).contains("stdin may be consumed"));
    let a = args(
        "save",
        Some(&input),
        &options(&ctx, &source),
        Path::new("-"),
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_cs705"))
        .args(a)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let result = child.wait_with_output().unwrap();
    assert_eq!(result.status.code(), Some(4));
    assert!(!String::from_utf8_lossy(&result.stderr).contains("\"status\": \"saved\""));
}

#[test]
fn publication_accepts_full_length_destination_components() {
    let f = Files::new();
    let out = f.write(&"x".repeat(255), b"KEEP");
    let mut args = vec![
        "display".into(),
        "--list-fields".into(),
        "-o".into(),
        p(&out).into(),
    ];
    assert_eq!(cli(&args, &[]).status.code(), Some(4));
    assert_eq!(fs::read(&out).unwrap(), b"KEEP");
    args.push("--overwrite".into());
    success(&cli(&args, &[]));
    assert!(
        serde_json::from_slice::<Value>(&fs::read(&out).unwrap())
            .unwrap()
            .is_array()
    );
    fs::remove_file(&out).unwrap();
    args.pop();
    success(&cli(&args, &[]));
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn overwrite_refuses_symlinks_without_touching_the_referent() {
    let f = Files::new();
    let target = f.write("target", b"KEEP");
    let link = f.path("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let result = cli(
        &[
            "display".into(),
            "--list-fields".into(),
            "--overwrite".into(),
            "-o".into(),
            p(&link).into(),
        ],
        &[],
    );
    assert_eq!(result.status.code(), Some(4));
    assert_eq!(fs::read(&target).unwrap(), b"KEEP");
    assert_eq!(fs::read_link(&link).unwrap(), target);
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 2);
}

#[test]
fn captured_g_is_not_regenerated_from_live_q_and_stdin_asset_is_owned_once() {
    use cs705::{
        clone::IMAGE_CAPACITY,
        icf::{
            context::Context,
            document::{DefaultSource, Session},
        },
    };
    let f = Files::new();
    let raw = fixture();
    let mut q = Context::from_bytes([0; 24]);
    assert!(q.decode_packed(0x40001));
    let mut generated = Session::new(q, None, 1, [1, 0]);
    generated
        .regenerate_defaults(
            DefaultSource {
                selector: 1,
                prefix: &raw,
                japanese_mode_pairs: None,
                restricted_band_stack: None,
            },
            &vec![0; IMAGE_CAPACITY],
        )
        .unwrap();
    let generation = f.write("captured-G.raw", generated.defaults().unwrap());
    let mut live = [0; 24];
    live[0] = 16;
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
    let ctx = f.write("different-live.json",&serde_json::to_vec(&json!({"generation_q_hex":hex(q.as_bytes()),"live_q_hex":hex(&live),"system_langid":1033})).unwrap());
    let out = f.path("captured-new.icf");
    let new_args: Vec<String> = [
        "new",
        "--range-context",
        p(&ctx),
        "--constructor-current",
        "--generation",
        p(&generation),
        "--generation-selector",
        "1",
        "--previous-reload-guard",
        "1",
        "--document-flags",
        "0100",
        "-o",
        p(&out),
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let result = cli(&new_args, &[]);
    success(&result);
    let report: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(report["default_selector"], 1);
    assert_eq!(report["range_source_selector"], 1);
    assert_eq!(report["live_kind"], 16);
    assert_eq!(report["automatic_refresh"], false);
    assert!(Icf::parse(&fs::read(&out).unwrap()).is_ok());

    let ctx = f.write("context.json", &context(1));
    let input = f.write("same-selector-new-q.icf", &literal(&raw, 0x40808));
    let out = f.path("stdin-asset.icf");
    let mut call = args("open", Some(&input), &options(&ctx, Path::new("-")), &out);
    call.extend(["--profile-choice".into(), "yes".into()]);
    let result = cli(&call, &raw);
    success(&result);
    let report: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(report["default_selector"], 1);
    assert_eq!(report["dirty"], 0);
    assert_eq!(
        Icf::parse(&fs::read(&out).unwrap()).unwrap().extra_data(),
        0x40808
    );
}

#[test]
fn validation_uses_real_typed_views_and_warning_choices_drive_cd_and_slot10() {
    let f = Files::new();
    let asset = f.write("asset.raw", &fixture());
    let mut raw = fixture();
    raw[0xa21d..0xa221].copy_from_slice(&123_000_000_u32.to_le_bytes());
    let input = f.write("warning.icf", &literal(&raw, 0x40001));
    let ctx = f.write("context.json", &context(1));
    let opts = options(&ctx, &asset);
    let view=f.write("views.json",&serde_json::to_vec(&json!([{"kind":3,"group":100,"first":0,"last":0,"count":1,"include_side1":true,"show_both":1}])).unwrap());
    for choice in ["no", "cancel", "yes"] {
        let out = f.path(&format!("{choice}.icf"));
        let mut a = args("save", Some(&input), &opts, &out);
        a.extend([
            "--validation-views".into(),
            p(&view).into(),
            "--warning-choice".into(),
            choice.into(),
        ]);
        if choice == "yes" {
            a.extend([
                "--show-both".into(),
                "1".into(),
                "--home-cache".into(),
                "0".into(),
            ]);
        }
        let result = cli(&a, &[]);
        success(&result);
        let report: Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(report["effects"]["validation"]["warnings"], 1);
        assert_eq!(report["effects"]["validation"]["errors"], 0);
        assert_eq!(report["effects"]["checksum_written"], choice == "yes");
        let bytes = fs::read(&out).unwrap();
        let image = cs705::icf::format::read(&bytes).unwrap();
        assert_eq!(image.status, cs705::icf::format::Status::Success);
        assert_eq!(bytes.windows(4).any(|w| w == b"#CD="), choice == "yes");
        assert_eq!(
            u32::from_le_bytes(image.image[0xa21d..0xa221].try_into().unwrap()),
            if choice == "yes" { 0 } else { 123_000_000 }
        );
        if choice == "yes" {
            assert_eq!(
                report["effects"]["correction"]["notification_requests"]
                    .as_array()
                    .unwrap()
                    .len(),
                104
            );
            assert_eq!(
                report["effects"]["correction"]["notifications_delivered"],
                false
            );
        }
    }
    let out = f.path("missing-state.icf");
    let mut a = args("save", Some(&input), &opts, &out);
    a.extend([
        "--validation-views".into(),
        p(&view).into(),
        "--warning-choice".into(),
        "yes".into(),
    ]);
    assert_eq!(cli(&a, &[]).status.code(), Some(3));
    assert!(!out.exists());
    let invalid = f.write("fake-flags.json", b"[{\"errors\":0,\"warnings\":0}]");
    let mut a = args("save", Some(&input), &opts, &out);
    a.extend(["--validation-views".into(), p(&invalid).into()]);
    assert_eq!(cli(&a, &[]).status.code(), Some(3));
    assert!(!out.exists());
    let mut a = args("save", Some(&input), &opts, &out);
    a.extend([
        "--validation-views".into(),
        p(&f.path("unread-view-source")).into(),
        "--target-revision".into(),
        "5".into(),
        "--legacy-choice".into(),
        "cancel".into(),
    ]);
    success(&cli(&a, &[]));
    assert!(!out.exists());
}

#[test]
fn standalone_reset_reaches_the_real_slot10_and_reports_requests_not_delivery() {
    let f = Files::new();
    let asset = f.write("asset.raw", &fixture());
    let mut raw = fixture();
    raw[0xa21d..0xa221].copy_from_slice(&123_000_000_u32.to_le_bytes());
    let input = f.write("input.icf", &literal(&raw, 0x40001));
    let ctx = f.write("context.json", &context(1));
    let out = f.path("reset.icf");
    let mut a = args("reset", Some(&input), &options(&ctx, &asset), &out);
    a.extend([
        "--show-both".into(),
        "1".into(),
        "--home-cache".into(),
        "167".into(),
    ]);
    let result = cli(&a, &[]);
    success(&result);
    let report: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(report["status"], "reset");
    assert_eq!(report["dirty"], 1);
    assert_eq!(report["effects"]["home_cache"], 167);
    assert_eq!(report["effects"]["notifications_delivered"], false);
    let bytes = fs::read(&out).unwrap();
    let image = Icf::parse(&bytes).unwrap();
    assert_eq!(&image.memory()[0xa21d..0xa221], &[0; 4]);
    assert_eq!(&image.memory()[0x3c9b0..], b"IcomCloneFormat3");
    let notices = report["effects"]["notification_requests"]
        .as_array()
        .unwrap();
    assert_eq!(notices.len(), 104);
    assert_eq!(notices[0]["event"], 60);
    assert_eq!(notices[1]["event"], 57);
    assert_eq!(notices[1]["synchronous"], true);
    assert_eq!(notices[103]["first"], 101);
}

#[test]
fn lifecycle_publication_requires_consent_and_close_preserves_cancellation() {
    use cs705::{
        clone::IMAGE_CAPACITY,
        icf::{
            context::Context,
            document::{DefaultSource, Session},
        },
    };
    let f = Files::new();
    let fields = fixture();
    let mut q = [0; 24];
    q[0] = 1;
    let mut session = Session::new(Context::from_bytes(q), None, 1, [1, 0]);
    session
        .regenerate_defaults(
            DefaultSource {
                selector: 1,
                prefix: &fields,
                japanese_mode_pairs: None,
                restricted_band_stack: None,
            },
            &vec![0; IMAGE_CAPACITY],
        )
        .unwrap();
    let current = f.write("current.raw", session.defaults().unwrap());
    let generation = f.write("generation.raw", session.defaults().unwrap());
    let ctx = f.write("context.json", &context(1));
    let output = f.path("saved.icf");
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
    let gestures = f.write(
        "save.json",
        &serde_json::to_vec(&json!([
            {"kind":"save-as","path_hex":hex(p(&output).as_bytes()),"filter_index":1}
        ]))
        .unwrap(),
    );
    let mut base: Vec<String> = [
        "lifecycle",
        "--current",
        p(&current),
        "--range-context",
        p(&ctx),
        "--generation",
        p(&generation),
        "--generation-selector",
        "1",
        "--previous-reload-guard",
        "1",
        "--document-flags",
        "0100",
        "--set",
        "noise_blanker_depth=7",
    ]
    .map(str::to_owned)
    .to_vec();
    let mut args = base.clone();
    args.extend(["--gestures".into(), p(&gestures).into(), "save-as".into()]);
    let denied = cli(&args, &[]);
    assert_eq!(
        denied.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&denied.stderr)
    );
    let report: Value = serde_json::from_slice(&denied.stdout).unwrap();
    assert_eq!(report["publications"], 0);
    assert_eq!(report["dirty"], 1);
    assert_eq!(report["effect"]["native_return"], 1);
    assert!(!output.exists());
    args.insert(1, "--allow-file-write".into());
    let approved = cli(&args, &[]);
    success(&approved);
    let report: Value = serde_json::from_slice(&approved.stdout).unwrap();
    assert_eq!(report["publications"], 1);
    assert_eq!(report["dirty"], 0);
    assert_eq!(report["baseline_matches_current"], true);
    assert_eq!(report["effect"]["native_return"], 0);
    assert_eq!(
        Icf::parse(&fs::read(&output).unwrap()).unwrap().memory()[0x399a2],
        6
    );

    let cancel = f.write("cancel.json", br#"[{"kind":"dirty","choice":2}]"#);
    base.extend(["--gestures".into(), p(&cancel).into(), "close".into()]);
    let out = cli(&base, &[]);
    success(&out);
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["effect"]["incoming"], false);
    assert_eq!(report["form_present"], true);
    assert_eq!(report["publications"], 0);
    assert_eq!(report["dirty"], 1);
    assert_eq!(fs::read(&current).unwrap(), session.defaults().unwrap());
}
