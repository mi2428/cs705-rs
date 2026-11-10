//! Pure-file print process checks. Fixtures are independently constructed raw
//! storage; no native application, printer, device or serial port is opened.

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
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/private/cli-r3/fixtures");
        fs::create_dir_all(&root).unwrap();
        let dir = root.join(format!(
            "printing-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, bytes).unwrap();
        path
    }
    fn image(&self, memory: bool) -> PathBuf {
        let bytes = Icf::from_memory(image(memory)).unwrap().to_bytes();
        // Metadata is deliberately not the empty from_memory header.
        let text = String::from_utf8(bytes)
            .unwrap()
            .replace("#Comment=", "#Comment=PRINT-METADATA");
        self.write("input.icf", text.as_bytes())
    }
    fn metric(&self) -> PathBuf {
        self.write("metric.json", &serde_json::to_vec(&metric()).unwrap())
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
fn ok(out: &Output) {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn reports(out: &Output) -> Vec<Value> {
    // The final process error is plain text; successful operation reports are
    // complete consecutive JSON values before that error.
    serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .take_while(Result::is_ok)
        .map(Result::unwrap)
        .collect()
}
fn report(out: &Output) -> Value {
    reports(out)
        .into_iter()
        .find(|v| v["operation"] == "portable-print")
        .unwrap()
}
fn metric() -> Value {
    json!({"version":1,"schema":"cs705.portable-monospace","font_height":-12,"em":12,"advance_em":6,"line_em":12,"padding":1})
}
fn image(memory: bool) -> Vec<u8> {
    let mut m = vec![0; 0x3c9c0];
    m[0x355a1] = 6;
    m[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    m[0x36114..0x36628].fill(255);
    m[0x36628] = 255;
    for group in 0..100 {
        let at = 0x355c0 + 19 * group;
        m[at] = 255;
        m[at + 1..at + 3].fill(255);
        m[at + 3..at + 19].fill(b' ');
    }
    m[0x355ae..0x355b0].fill(255);
    m[0x35d2c..0x36114].fill(255);
    m[0x3c0a7..0x3c2ff].fill(255);
    m[0x3c385..0x3c613].fill(255);
    m[0x3c5dd..0x3c613].fill(255);
    m[0x3bf0a..0x3bf6e].fill(255);
    m[0x3ab82..0x3bf0a].fill(255);
    m[0xa60b..0xa60f].fill(255);
    // Ten invalid memopad slots; the reserved slot 0 remains distinct.
    for row in 1..11 {
        let at = cs705::icf::special::CompactKind::Memopad
            .offset(row)
            .unwrap();
        m[at..at + 4].fill(255);
    }
    for row in 0..4 {
        let at = 0xa21c + 83 * row;
        m[at + 67..at + 83].fill(b' ');
    }
    m[0xa21c + 67..0xa21c + 75].copy_from_slice(b"<PRINT&>");
    if memory {
        m[0x36628] = 0;
        m[0x355c1..0x355c3].copy_from_slice(&7_u16.to_le_bytes());
        m[0x355c3..0x355c7].copy_from_slice(b"BANK");
        m[0x36114] &= !1;
        let at = 83 * 7;
        m[at + 67..at + 83].fill(b' ');
        m[at + 67..at + 72].copy_from_slice(b"SLOT7");
    }
    m
}
fn args<'a>(input: &'a str, metric: &'a str, sheet: &'a str) -> Vec<&'a str> {
    vec![
        "print",
        input,
        "--yes",
        "--measurement",
        metric,
        "--sheet",
        sheet,
        "--paper-width",
        "1200",
        "--paper-height",
        "800",
    ]
}

#[test]
#[ignore = "requires the original private PE asset; bounded resource reads only"]
fn native_resource_mycall_message_and_preprint_inline_use_real_sources() {
    let asset = std::env::var("CS705_PRINT_SOURCE").expect("CS705_PRINT_SOURCE");
    let f = Files::new();
    let mut bytes = image(false);
    bytes[0x2ed5b..0x2ed5b + 12].copy_from_slice(b"MY-NATIVTEST");
    bytes[0x32bb5..0x32bb5 + 20].copy_from_slice(b"TX-NATIVE-MESSAGE   ");
    let input = f.write("native.icf", &Icf::from_memory(bytes).unwrap().to_bytes());
    let before = fs::read(&input).unwrap();
    let metric = f.metric();
    for (sheet, expected) in [
        ("My_Callsign", "MY-NATIV"),
        ("Message", "TX-NATIVE-MESSAGE"),
    ] {
        let out = cli(
            &[
                "print",
                p(&input),
                "--yes",
                "--native-resource",
                &asset,
                "--native-sheet",
                sheet,
                "--measurement",
                p(&metric),
                "--paper-width",
                "1200",
                "--paper-height",
                "800",
                "-o",
                "-",
            ],
            b"",
        );
        ok(&out);
        assert!(String::from_utf8_lossy(&out.stdout).contains(expected));
        let r = reports(&out)
            .into_iter()
            .find(|r| r["operation"] == "native-resource-print")
            .unwrap();
        assert_eq!(r["layouts"][0]["native_fixed_columns"], 3);
        assert_eq!(r["count_pages"], r["drawn_pages"]);
        assert_eq!(r["os_printed"], false);
    }
    let config = f.write(
        "config.json",
        &serde_json::to_vec(&json!({"native-display":{
            "c00":1,"c04":1,"c10":100,"c14":1,"c18":0,"c34":1,"c38":1,"c90":0,"c94":0,"c98":0
        }}))
        .unwrap(),
    );
    let q = f.write("q.bin", &[0; 24]);
    let all_target = f.path("native-all-cancel.html");
    let all = cli(
        &[
            "--config",
            p(&config),
            "print",
            p(&input),
            "--yes",
            "--native-resource",
            &asset,
            "--all",
            "--record-context",
            p(&q),
            "--measurement",
            p(&metric),
            "--paper-width",
            "1200",
            "--paper-height",
            "800",
            "--cancel-after-sheets",
            "0",
            "-o",
            p(&all_target),
        ],
        b"",
    );
    ok(&all);
    let r = reports(&all)
        .into_iter()
        .find(|r| r["operation"] == "native-resource-print")
        .unwrap();
    assert_eq!(r["drawn_pages"], 0);
    assert_eq!(r["ended"], true);
    assert_eq!(r["remaining_names"], json!([]));
    let events = r["events"].as_array().unwrap();
    assert!(
        events
            .iter()
            .filter(|e| e.as_str().is_some_and(|s| s.starts_with("Tick")))
            .count()
            > 2
    );
    assert_eq!(events[events.len() - 2], "EndDoc");
    assert!(!all_target.exists());
    let target = f.path("native-cancel.html");
    let out = cli(
        &[
            "--config",
            p(&config),
            "print",
            p(&input),
            "--native-resource",
            &asset,
            "--sheet",
            "call",
            "--pending-cell",
            "3/5=NATIVE",
            "--record-context",
            p(&q),
            "--measurement",
            p(&metric),
            "--paper-width",
            "1200",
            "--paper-height",
            "800",
            "--preview",
            "cancel",
            "-o",
            p(&target),
        ],
        b"",
    );
    ok(&out);
    let r = reports(&out)
        .into_iter()
        .find(|r| r["operation"] == "native-resource-print")
        .unwrap();
    assert_eq!(r["syncs"][0]["visible_before"], true);
    assert_eq!(r["syncs"][1]["visible_before"], false);
    assert_eq!(r["editor_visible"], false);
    assert_eq!(r["begin_doc"], false);
    assert!(r["changed_bytes"].as_u64().unwrap() > 0);
    assert_eq!(r["timer_ms"], 250);
    assert!(!target.exists());
    assert_eq!(fs::read(&input).unwrap(), before);
}

#[test]
fn current_typed_call_draws_cells_and_leaves_input_and_metadata_untouched() {
    let f = Files::new();
    let input = f.image(false);
    let m = f.metric();
    let before = fs::read(&input).unwrap();
    let mut a = args(p(&input), p(&m), "call");
    a.extend(["-o", "-"]);
    let out = cli(&a, b"");
    ok(&out);
    let html = String::from_utf8(out.stdout.clone()).unwrap();
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("<svg ") && html.contains("<rect ") && html.contains("<text "));
    assert!(html.contains("&lt;PRINT&amp;&gt;"));
    assert!(!html.contains("OS printer"));
    let r = report(&out);
    assert_eq!(r["os_printed"], false);
    assert_eq!(r["changed_bytes"], 0);
    assert_eq!(r["layouts"][0]["native_fixed_columns"], 3);
    assert_eq!(r["layouts"][0]["native_fixed_rows"], 5);
    assert_eq!(r["layouts"][0]["portable_fixed_columns"], 1);
    assert_eq!(r["layouts"][0]["portable_fixed_rows"], 1);
    assert_eq!(r["count_pages"], r["drawn_pages"]);
    assert_eq!(r["syncs"].as_array().unwrap().len(), 2);
    assert!(
        r["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["event"] != "Tick")
    );
    assert_eq!(fs::read(&input).unwrap(), before);
}

#[test]
fn real_expanded_memory_map_selects_nonidentity_slot_and_empty_never_publishes() {
    let f = Files::new();
    let input = f.image(true);
    let m = f.metric();
    let mut a = args(p(&input), p(&m), "memory");
    a.extend(["-o", "-"]);
    let out = cli(&a, b"");
    ok(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("SLOT7"));
    let empty = f.write(
        "empty.icf",
        &Icf::from_memory(image(false)).unwrap().to_bytes(),
    );
    let target = f.path("empty.html");
    let mut a = args(p(&empty), p(&m), "memory");
    a.extend(["-o", p(&target)]);
    let out = cli(&a, b"");
    ok(&out);
    assert!(!target.exists() && out.stdout.is_empty());
    let r = report(&out);
    assert_eq!(r["count_pages"], 0);
    assert_eq!(r["begin_doc"], false);
}

#[test]
fn cached_ur_and_repeater_counts_use_real_stage8_ordered_typed_rows() {
    let f = Files::new();
    let metric = f.metric();
    let mut m = image(false);
    m[0x3c0a7..0x3c0a9].copy_from_slice(&7_u16.to_le_bytes());
    m[0x2f703..0x2f713].fill(b' ');
    m[0x2f703..0x2f708].copy_from_slice(b"WRONG");
    let at = 0x2f703 + 16 * 7;
    m[at..at + 16].fill(b' ');
    m[at..at + 9].copy_from_slice(b"UR-ORDER7");
    let input = f.write("ur.icf", &Icf::from_memory(m).unwrap().to_bytes());
    let mut a = args(p(&input), p(&metric), "ur");
    a.extend(["-o", "-"]);
    let out = cli(&a, b"");
    ok(&out);
    let html = String::from_utf8_lossy(&out.stdout);
    assert!(html.contains("UR-ORDER7") && !html.contains("WRONG"));
    assert!(report(&out)["count_pages"].as_u64().unwrap() > 0);

    let mut m = image(false);
    m[0x3ab82..0x3ab84].copy_from_slice(&7_u16.to_le_bytes());
    m[0x3bf0a + 6..0x3bf0a + 8].copy_from_slice(&0_u16.to_le_bytes());
    let at = 0xaa0f + 52 * 7 + 18;
    m[at..at + 16].fill(b' ');
    m[at..at + 10].copy_from_slice(b"RPT-ORDER7");
    let input = f.write("rpt.icf", &Icf::from_memory(m).unwrap().to_bytes());
    let q = f.write("q.bin", &[0; 24]);
    let mut a = args(p(&input), p(&metric), "repeater");
    a.extend(["--group", "3", "--record-context", p(&q), "-o", "-"]);
    let out = cli(&a, b"");
    ok(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("RPT-ORDER7"));
    // A zero cached group still follows stage8 with the actual live Q source.
    let mut a = args(p(&input), p(&metric), "repeater");
    a.extend(["--record-context", p(&q), "--dry-run"]);
    let out = cli(&a, b"");
    ok(&out);
    assert_eq!(report(&out)["count_pages"], 0);
}

#[test]
fn preview_cancel_keeps_real_preprint_commit_but_no_doc_or_publication() {
    let f = Files::new();
    let input = f.image(false);
    let m = f.metric();
    let before = fs::read(&input).unwrap();
    let target = f.path("cancel.html");
    let mut a = args(p(&input), p(&m), "call");
    a.extend([
        "--preview",
        "cancel",
        "--pending-edit",
        "clone-comment=\"PRINT\"",
        "-o",
        p(&target),
    ]);
    let out = cli(&a, b"");
    ok(&out);
    assert!(out.stdout.is_empty() && !target.exists());
    let r = report(&out);
    assert_eq!(r["begin_doc"], false);
    assert_eq!(r["editor_visible"], false);
    assert!(r["changed_bytes"].as_u64().unwrap() > 0);
    assert!(
        r["editor_adapter"]
            .as_str()
            .unwrap()
            .contains("native inline reachability not asserted")
    );
    assert_eq!(r["syncs"][0]["visible_before"], true);
    assert_eq!(r["syncs"][1]["visible_before"], false);
    assert_eq!(fs::read(&input).unwrap(), before);
}

#[test]
fn preview_print_and_multipage_newpage_have_distinct_count_layout_render_phases() {
    let f = Files::new();
    let input = f.image(false);
    let m = f.metric();
    let out = cli(
        &[
            "print",
            p(&input),
            "--sheet",
            "call",
            "--measurement",
            p(&m),
            "--paper-width",
            "300",
            "--paper-height",
            "70",
            "--preview",
            "print",
            "-o",
            "-",
        ],
        b"",
    );
    ok(&out);
    let r = report(&out);
    let pages = r["drawn_pages"].as_u64().unwrap();
    assert_eq!(r["preview_approved"], true);
    assert!(pages > 1);
    assert_eq!(pages, r["count_pages"].as_u64().unwrap());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout)
            .matches("<section class=\"page\"")
            .count(),
        pages as usize
    );
    let events = r["events"].as_array().unwrap();
    assert_eq!(
        events.iter().filter(|e| e["event"] == "NewPage").count(),
        pages as usize - 1
    );
    assert_eq!(events.first().unwrap()["event"], "BeginDoc");
    assert_eq!(events[events.len() - 2]["event"], "EndDoc");
    assert_eq!(events.last().unwrap()["event"], "ClearNames");
}

#[test]
fn all_render_and_cancel_drain_virtual_50ms_callbacks_with_normal_end() {
    let f = Files::new();
    let input = f.image(false);
    let m = f.metric();
    let q = f.write("q.bin", &[0; 24]);
    let target = f.path("cancel.html");
    let out = cli(
        &[
            "print",
            p(&input),
            "--yes",
            "--all",
            "--record-context",
            p(&q),
            "--measurement",
            p(&m),
            "--paper-width",
            "1200",
            "--paper-height",
            "800",
            "--cancel-after-sheets",
            "1",
            "-o",
            p(&target),
        ],
        b"",
    );
    ok(&out);
    assert!(!target.exists() && out.stdout.is_empty());
    let r = report(&out);
    assert_eq!(r["cancelled"], true);
    assert_eq!(r["ended"], true);
    assert_eq!(r["remaining_names"], json!([]));
    let events = r["events"].as_array().unwrap();
    let ticks: Vec<_> = events.iter().filter(|e| e["event"] == "Tick").collect();
    assert!(ticks.len() > 2);
    for (i, tick) in ticks.iter().enumerate() {
        assert_eq!(tick["virtual_due_ms"], (i as u64 + 1) * 50);
    }
    assert!(ticks.len() < r["candidates"].as_u64().unwrap() as usize);
    assert_eq!(events[events.len() - 2]["event"], "EndDoc");
    assert!(r["drawn_pages"].as_u64().unwrap() < r["count_pages"].as_u64().unwrap());
    let out = cli(
        &[
            "print",
            p(&input),
            "--yes",
            "--all",
            "--record-context",
            p(&q),
            "--measurement",
            p(&m),
            "--paper-width",
            "1200",
            "--paper-height",
            "800",
            "-o",
            "-",
        ],
        b"",
    );
    ok(&out);
    let r = report(&out);
    assert_eq!(r["cancelled"], false);
    assert_eq!(r["count_pages"], r["drawn_pages"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout)
            .matches("<section class=\"page\"")
            .count(),
        r["drawn_pages"].as_u64().unwrap() as usize
    );
}

#[test]
fn independent_fields_use_defaults_and_require_real_print_approval() {
    let f = Files::new();
    let input = f.image(false);
    let before = fs::read(&input).unwrap();
    let target = f.path("fields.html");
    let mut a = vec![
        "print",
        p(&input),
        "--field",
        "network/prefix",
        "-o",
        p(&target),
    ];
    let out = cli(&a, b"");
    ok(&out);
    assert!(!target.exists() && out.stdout.is_empty());
    assert_eq!(report(&out)["begin_doc"], false);
    a.push("--yes");
    let out = cli(&a, b"");
    ok(&out);
    let html = String::from_utf8(fs::read(&target).unwrap()).unwrap();
    assert!(html.contains("network prefix") && html.contains("<svg "));
    assert_eq!(report(&out)["count_pages"], report(&out)["drawn_pages"]);
    fs::remove_file(&target).unwrap();
    a.extend(["--preview", "cancel"]);
    ok(&cli(&a, b""));
    assert!(!target.exists());
    assert_eq!(fs::read(&input).unwrap(), before);
}

#[test]
fn publication_no_clobber_overwrite_dryrun_and_cancel_zero_are_safe() {
    let f = Files::new();
    let input = f.image(false);
    let m = f.metric();
    let target = f.write("output.html", b"KEEP");
    let mut a = args(p(&input), p(&m), "call");
    a.extend(["-o", p(&target)]);
    let out = cli(&a, b"");
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(fs::read(&target).unwrap(), b"KEEP");
    a.push("--overwrite");
    ok(&cli(&a, b""));
    assert!(fs::read(&target).unwrap().starts_with(b"<!doctype html>"));
    let absent = f.path("dry.html");
    let mut a = args(p(&input), p(&m), "call");
    a.extend(["--dry-run", "-o", p(&absent)]);
    let out = cli(&a, b"");
    ok(&out);
    assert!(!absent.exists() && out.stdout.is_empty());
    let mut a = args(p(&input), p(&m), "call");
    a.extend(["--cancel-after-sheets", "0", "-o", p(&absent)]);
    let out = cli(&a, b"");
    ok(&out);
    assert_eq!(report(&out)["drawn_pages"], 0);
    assert!(!absent.exists());
}

#[test]
fn every_stdin_pair_is_rejected_before_missing_input_or_source_reads() {
    let flags = [
        None,
        Some("--measurement"),
        Some("--record-context"),
        Some("--generation"),
        Some("--reference-defaults"),
        Some("--normal-state"),
    ];
    for left in 0..flags.len() {
        for right in left + 1..flags.len() {
            let mut a = vec![
                "print",
                if left == 0 { "-" } else { "missing.icf" },
                "--sheet",
                "call",
                "--paper-width",
                "100",
                "--paper-height",
                "100",
                "--dry-run",
            ];
            for (i, flag) in flags.iter().enumerate().skip(1) {
                if i == 1 || i == left || i == right {
                    a.extend([
                        flag.unwrap(),
                        if i == left || i == right {
                            "-"
                        } else {
                            "missing.json"
                        },
                    ]);
                }
            }
            let out = cli(&a, b"");
            assert_eq!(
                out.status.code(),
                Some(3),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(out.stdout.is_empty());
            assert!(
                String::from_utf8_lossy(&out.stderr)
                    .contains("stdin may be consumed by only one input")
            );
        }
    }
}

#[test]
fn failed_editor_layout_or_untrusted_packet_never_publishes_or_hides_unknown_factory() {
    let f = Files::new();
    let input = f.image(false);
    let m = f.metric();
    let target = f.path("failed.html");
    let mut a = args(p(&input), p(&m), "call");
    a.extend(["--pending-edit", "not-a-field=1", "-o", p(&target)]);
    let out = cli(&a, b"");
    assert_eq!(out.status.code(), Some(3));
    let r = report(&out);
    assert_eq!(r["editor_visible"], true);
    assert_eq!(r["validation_guard"], 0);
    assert_eq!(r["syncs"].as_array().unwrap().len(), 1);
    assert!(!target.exists());
    for source in [json!({"pages":[],"cells":[]}), {
        let mut v = metric();
        v["font_height"] = 0.into();
        v
    }] {
        let bad = f.write("bad.json", &serde_json::to_vec(&source).unwrap());
        let mut a = args(p(&input), p(&bad), "call");
        a.extend(["-o", p(&target)]);
        let out = cli(&a, b"");
        assert!(!out.status.success());
        assert!(!target.exists() && out.stdout.is_empty());
    }
    for extra in [vec!["--group", "100"], vec!["--group", "-1"]] {
        let mut a = args(p(&input), p(&m), "memory");
        a.extend(extra);
        a.extend(["-o", p(&target)]);
        assert!(!cli(&a, b"").status.success());
        assert!(!target.exists());
    }
    let mut a = args(p(&input), p(&m), "all");
    a.extend(["-o", p(&target)]);
    let out = cli(&a, b"");
    assert_eq!(out.status.code(), Some(3));
    assert!(!target.exists());
}
