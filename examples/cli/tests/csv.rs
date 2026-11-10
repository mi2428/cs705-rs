//! Pure-file process CSV checks. Fixtures and build outputs stay in target.

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
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"));
        fs::create_dir_all(root).unwrap();
        let path = root.join(format!(
            "csv-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn write(&self, name: &str, data: &[u8]) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, data).unwrap();
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
    assert!(!args.contains(&"--port"));
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
fn exit(out: &Output, expected: i32) {
    assert_eq!(
        out.status.code(),
        Some(expected),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn report(out: &Output) -> Value {
    serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .next()
        .unwrap()
        .unwrap()
}
fn context(files: &Files) -> Value {
    with_document(
        files,
        json!({"profile":"portable-ascii","decimal":".","rtl_decimal":".","separator":",",
        "short_date":"yyyy/mm/dd","date_separator":"/","time_separator":":",
        "current_year":2026,"two_digit_window":50,"clock":"12:34:56",
        "gps_hemispheres":"NE","gps_group_name":"                "}),
    )
}
fn with_document(files: &Files, mut value: Value) -> Value {
    use cs705::icf::{
        context::Context,
        document::{DefaultSource, Session},
    };
    let bytes = fixture();
    let image = Icf::parse(&bytes).unwrap();
    let mut q = [0; 24];
    q[0] = 1;
    let mut session = Session::new(Context::from_bytes(q), None, 1, [1, 0]);
    session
        .regenerate_defaults(
            DefaultSource {
                selector: 1,
                prefix: image.memory(),
                japanese_mode_pairs: None,
                restricted_band_stack: None,
            },
            &vec![0; cs705::clone::IMAGE_CAPACITY],
        )
        .unwrap();
    let generation = files.write("document-generation.raw", session.defaults().unwrap());
    value["document_generation"] = json!(p(&generation));
    value["document_generation_selector"] = json!(1);
    value["previous_reload_guard"] = json!(1);
    value["document_flags"] = json!("0100");
    for key in ["generation_q_hex", "live_q_hex"] {
        if value.get(key).is_none() {
            value[key] = json!("00".repeat(24));
        }
    }
    value
}
fn fixture() -> Vec<u8> {
    let mut m = vec![0; 0x3c9c0];
    m[0x355a1] = 6;
    m[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    m[0x3c385..0x3c613].fill(0xff);
    m[0x3ab82..0x3bf6e].fill(0xff);
    m[0x3c0a7..0x3c2ff].fill(0xff);
    m[0x35d2c..0x36628].fill(0xff);
    m[0x3662d..0x36821].fill(1);
    for group in 0..100 {
        let at = 0x355c0 + 19 * group;
        m[at] = if group == 99 { 255 } else { (group + 1) as u8 };
        m[at + 1..at + 3].fill(255);
        m[at + 3..at + 19].fill(b' ');
    }
    for row in 0..45 {
        let at = cs705::icf::special::CompactKind::BandStack
            .offset(row)
            .unwrap();
        m[at..at + 4].fill(255);
    }
    for row in 0..=10 {
        m[0x36f54 + 36 * row..0x36f58 + 36 * row].fill(255);
    }
    m[0xa60b..0xa60f].fill(255);
    m[0x3c84b] = 255;
    m[0x3c8b9..0x3c8bb].fill(255);
    m[0x3c903..0x3c905].fill(255);
    Icf::from_memory(m).unwrap().to_bytes()
}
fn memory_import_fixture() -> Vec<u8> {
    use cs705::icf::{
        record::{self, ChannelField},
        rows::NormalMemory,
    };
    let mut raw = vec![0; 0x4997a];
    raw[0x355a1] = 6;
    raw[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
    raw[0x3fb40..0x3fb44].copy_from_slice(&3_u32.to_le_bytes());
    raw[0x3662d..0x36821].fill(1);
    for g in 0..100 {
        raw[0x355c3 + 19 * g..0x355d3 + 19 * g].fill(b' ');
    }
    raw[0x355c3..0x355d3].copy_from_slice(b"OLD GROUP       ");
    raw[0x355c3 + 38..0x355d3 + 38].copy_from_slice(b"REMOTE          ");
    for cell in raw[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
        cell.copy_from_slice(&0xffff_u32.to_le_bytes());
    }
    raw[0x392fa] = 0;
    raw[0x392fb..0x392fd].copy_from_slice(&99_u16.to_le_bytes());
    let mut model = NormalMemory::from_working(raw).unwrap();
    for (g, row) in [(0, 5), (2, 3)] {
        model.allocate(g, row..row + 1).unwrap();
        for side in 0..2 {
            let index = (200 * g + 2 * row + side) as u32;
            let mut h = record::reset_holder(2, index, 0);
            h.set_name(b"OLD ROW         ");
            h.set_mode(side as u32);
            h.set_filter(0);
            h.set_channel_field(
                ChannelField::FrequencyHz,
                145_000_000 + 500_000 * side as u32,
            );
            model.store_channel(index, &mut h).unwrap();
        }
    }
    model.to_icf().unwrap().to_bytes()
}
fn memory_context(files: &Files, seed: &Path) -> Value {
    with_document(
        files,
        json!({"profile":"portable-ascii","decimal":".","separator":",","crt_decimal":".",
        "live_q_hex":"000000000000000000000000000000000000000000000000",
        "generation_q_hex":"000000000000000000000000000000000000000000000000","seed_icf":p(seed)}),
    )
}
const GPS: &[u8] = b"Group,Group Name,Name,Date,Time,Latitude,Longitude,Altitude,Alarm\r\nA,HOME,FIRST,2026/11/10,12:34:56,0,0,,ON\r\n";

#[test]
fn gps_native_date_source_uses_shared_tokens_shifted_date_and_dummy_before_clear() {
    let f = Files::new();
    let input = f.write("input.icf", &fixture());
    let csv = f.write("source.csv", GPS);
    let ctx = f.write("context.json", &serde_json::to_vec(&context(&f)).unwrap());
    let stored = f.path("stored.icf");
    exit(
        &cli(
            &[
                "import",
                p(&input),
                "--csv",
                p(&csv),
                "--sheet",
                "gps",
                "--csv-context",
                p(&ctx),
                "--yes",
                "-o",
                p(&stored),
            ],
            b"",
        ),
        0,
    );
    let source = fs::read(&stored).unwrap();
    let mut image = Icf::parse(&source).unwrap();
    let slot = image.gps_groups().unwrap()[1].physical_slots()[0];
    let at = 0x32c19 + 34 * slot;
    image
        .patch_memory(&[
            (at + 8, &[0x31, 0x12, 0x24, 0x23, 0x30, 0x45]),
            (0x396ff, &60_i16.to_le_bytes()),
        ])
        .unwrap();
    let original = image.to_bytes();
    fs::write(&stored, &original).unwrap();
    let mut context = context(&f);
    context["short_date"] = json!("gg e mmmm ddd yyyy/mm/dd");
    context["native_date_tokens"] = json!([
        {"date":[2025,1,1],"letter":"g","count":2,"output":"455241"},
        {"date":[2025,1,1],"letter":"e","count":1,"output":[55]},
        {"date":[2025,1,1],"letter":"m","count":4,"output":"4d4f4e5448"},
        {"date":[2025,1,1],"letter":"d","count":3,"output":[68,65,89]}
    ]);
    fs::write(&ctx, serde_json::to_vec(&context).unwrap()).unwrap();
    let args = [
        "export",
        p(&stored),
        "--sheet",
        "gps",
        "--csv-context",
        p(&ctx),
        "-o",
        "-",
    ];
    let out = cli(&args, b"");
    exit(&out, 0);
    assert!(
        out.stdout
            .windows(b"ERA 7 MONTH DAY 2025/01/01,00:30:45".len())
            .any(|w| w == b"ERA 7 MONTH DAY 2025/01/01,00:30:45")
    );
    let keep = f.write("keep.csv", b"KEEP");
    for tokens in [
        json!([]),
        json!([
            {"date":[2024,12,31],"letter":"g","count":2,"output":"455241"}
        ]),
    ] {
        let mut bad = context.clone();
        bad["native_date_tokens"] = tokens;
        fs::write(&ctx, serde_json::to_vec(&bad).unwrap()).unwrap();
        let out = cli(
            &[
                "export",
                p(&stored),
                "--sheet",
                "gps",
                "--csv-context",
                p(&ctx),
                "--overwrite",
                "--dry-run",
                "-o",
                p(&keep),
            ],
            b"",
        );
        exit(&out, 3);
        assert!(out.stdout.is_empty());
        assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    }
    image.patch_memory(&[(at + 8, &[0xff; 3])]).unwrap();
    fs::write(&stored, image.to_bytes()).unwrap();
    for token in context["native_date_tokens"].as_array_mut().unwrap() {
        token["date"] = json!([2000, 10, 10]);
    }
    fs::write(&ctx, serde_json::to_vec(&context).unwrap()).unwrap();
    let out = cli(&args, b"");
    exit(&out, 0);
    assert!(
        out.stdout
            .windows(b"FIRST,,00:30:45".len())
            .any(|w| w == b"FIRST,,00:30:45")
    );
    context["native_date_tokens"] = json!([]);
    fs::write(&ctx, serde_json::to_vec(&context).unwrap()).unwrap();
    let out = cli(&args, b"");
    exit(&out, 3);
    assert!(out.stdout.is_empty());
    assert_eq!(fs::read(input).unwrap(), fixture());
}

#[test]
fn gps_bytes_reach_parser_finalizer_image_and_literal_export() {
    let f = Files::new();
    let input = f.write("input.icf", &fixture());
    let csv = f.write("source.csv", GPS);
    let ctx = f.write("context.json", &serde_json::to_vec(&context(&f)).unwrap());
    let output = f.path("imported.icf");
    let out = cli(
        &[
            "import",
            p(&input),
            "--csv",
            p(&csv),
            "--sheet",
            "gps",
            "--csv-context",
            p(&ctx),
            "--yes",
            "-o",
            p(&output),
        ],
        b"",
    );
    exit(&out, 0);
    assert!(out.stdout.is_empty());
    let stored = fs::read(&output).unwrap();
    let image = Icf::parse(&stored).unwrap();
    assert_eq!(image.gps_groups().unwrap()[1].physical_slots().len(), 1);
    let raw = image.physical_gps_record_raw(0).unwrap().unwrap();
    assert_eq!(
        &raw[..14],
        &[
            0, 0, 0, b'N', 0, 0, 0, b'E', 0x10, 0x11, 0x26, 0x12, 0x34, 0x56
        ]
    );
    assert_eq!(&raw[18..], b"FIRST           ");
    let exported = cli(
        &[
            "export",
            p(&output),
            "--sheet",
            "gps",
            "--csv-context",
            p(&ctx),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&exported, 0);
    assert_eq!(exported.stdout, b"Group,Group Name,Name,Date,Time,Latitude,Longitude,Altitude,Alarm\r\nA,HOME,FIRST,2026/11/10,12:34:56,0.000000,0.000000,,ON\r\n");
    let other = cli(
        &[
            "export",
            p(&output),
            "--sheet",
            "gps",
            "--group",
            "2",
            "--csv-context",
            p(&ctx),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&other, 0);
    assert_eq!(
        other.stdout,
        &GPS[..GPS.iter().position(|b| *b == b'\n').unwrap() + 1]
    );
    let selected = cli(
        &[
            "export",
            p(&output),
            "--sheet",
            "gps",
            "--group",
            "1",
            "--csv-context",
            p(&ctx),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&selected, 0);
    assert_eq!(selected.stdout, exported.stdout);
}

#[test]
fn gps_prefix_is_not_a_rollback_or_an_implicit_publication() {
    let f = Files::new();
    let input = f.write("input.icf", &fixture());
    let mut source = GPS.to_vec();
    source.extend_from_slice(b"A,HOME,SECOND,,,0,0,bad,OFF\r\n");
    let csv = f.write("partial.csv", &source);
    let ctx = f.write("context.json", &serde_json::to_vec(&context(&f)).unwrap());
    let output = f.write("output.icf", b"KEEP");
    let args = [
        "import",
        p(&input),
        "--csv",
        p(&csv),
        "--sheet",
        "gps",
        "--csv-context",
        p(&ctx),
        "--yes",
        "--overwrite",
        "-o",
        p(&output),
    ];
    let rejected = cli(&args, b"");
    exit(&rejected, 3);
    assert_eq!(fs::read(&output).unwrap(), b"KEEP");
    let report = report(&rejected);
    assert_eq!(report["applied"], 1);
    assert_eq!(report["changed"], true);
    assert!(!report["failure"].is_null());
    let mut accepted = args.to_vec();
    accepted.push("--accept-partial");
    exit(&cli(&accepted, b""), 5);
    let bytes = fs::read(&output).unwrap();
    assert_eq!(
        Icf::parse(&bytes).unwrap().gps_groups().unwrap()[1]
            .physical_slots()
            .len(),
        1
    );
    let dry = f.write("dry.icf", b"KEEP");
    let output_index = accepted.iter().position(|s| *s == p(&output)).unwrap();
    accepted[output_index] = p(&dry);
    accepted.push("--dry-run");
    exit(&cli(&accepted, b""), 5);
    assert_eq!(fs::read(dry).unwrap(), b"KEEP");
}

#[test]
fn csv_cancel_context_stdin_and_file_boundaries_do_not_publish() {
    let f = Files::new();
    let input = f.write("input.icf", &fixture());
    let ctx = f.write("context.json", &serde_json::to_vec(&context(&f)).unwrap());
    let bad = f.write("bad.csv", b"\xef\xbb\xbfLatitude,Longitude\n0,0\n");
    let output = f.write("output.icf", b"KEEP");
    for extra in [None, Some("--accept-partial")] {
        let mut args = vec![
            "import",
            p(&input),
            "--csv",
            p(&bad),
            "--sheet",
            "gps",
            "--csv-context",
            p(&ctx),
            "--yes",
            "--overwrite",
            "-o",
            p(&output),
        ];
        args.extend(extra);
        exit(&cli(&args, b""), 3);
        assert_eq!(fs::read(&output).unwrap(), b"KEEP");
    }
    exit(
        &cli(
            &[
                "import",
                "missing",
                "--csv",
                "missing",
                "--sheet",
                "all",
                "--mode",
                "cancel",
                "--csv-context",
                "missing",
                "-o",
                p(&output),
            ],
            b"",
        ),
        0,
    );
    assert_eq!(fs::read(&output).unwrap(), b"KEEP");
    for args in [
        vec![
            "import",
            "-",
            "--csv",
            "-",
            "--sheet",
            "gps",
            "--csv-context",
            p(&ctx),
            "--yes",
            "--dry-run",
        ],
        vec![
            "import",
            "-",
            "--csv",
            p(&bad),
            "--sheet",
            "gps",
            "--csv-context",
            "-",
            "--yes",
            "--dry-run",
        ],
        vec![
            "export",
            "-",
            "--sheet",
            "gps",
            "--csv-context",
            "-",
            "--dry-run",
        ],
    ] {
        exit(&cli(&args, b""), 3);
    }
    let mut nested = context(&f);
    nested["seed_icf"] = json!("-");
    let nested = f.write("nested.json", &serde_json::to_vec(&nested).unwrap());
    exit(
        &cli(
            &[
                "import",
                "-",
                "--csv",
                p(&bad),
                "--sheet",
                "gps",
                "--csv-context",
                p(&nested),
                "--yes",
                "--dry-run",
            ],
            b"",
        ),
        3,
    );
}

#[test]
fn gps_short_row_discard_locale_and_formatter_failure_keep_exact_boundaries() {
    let f = Files::new();
    let input = f.write("input.icf", &fixture());
    let mut ctx = context(&f);
    ctx["decimal"] = json!(",");
    ctx["separator"] = json!(",");
    let context_path = f.write("context.json", &serde_json::to_vec(&ctx).unwrap());
    let csv = f.write("short.csv", b"Latitude;Longitude;Name\n0;0;OK\n0\n");
    let output = f.path("output.icf");
    let out = cli(
        &[
            "import",
            p(&input),
            "--csv",
            p(&csv),
            "--sheet",
            "gps",
            "--csv-context",
            p(&context_path),
            "--yes",
            "--accept-partial",
            "-o",
            p(&output),
        ],
        b"",
    );
    exit(&out, 5);
    let report = report(&out);
    assert_eq!(report["counts"]["c2"], 1);
    assert_eq!(report["applied"], 1);
    let out = cli(
        &[
            "export",
            p(&output),
            "--sheet",
            "gps",
            "--csv-context",
            p(&context_path),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&out, 0);
    assert!(
        out.stdout
            .windows(b"OFF;;OK;;;0,000000;0,000000;;OFF".len())
            .any(|w| w == b"OFF;;OK;;;0,000000;0,000000;;OFF")
    );
    ctx["short_date"] = json!("gg yyyy/mm/dd");
    let ctx = f.write("native.json", &serde_json::to_vec(&ctx).unwrap());
    let keep = f.write("export.csv", b"KEEP");
    exit(
        &cli(
            &[
                "export",
                p(&output),
                "--sheet",
                "gps",
                "--csv-context",
                p(&ctx),
                "--overwrite",
                "-o",
                p(&keep),
            ],
            b"",
        ),
        3,
    );
    assert_eq!(fs::read(keep).unwrap(), b"KEEP");
}

#[test]
fn selected_gps_import_keeps_other_groups_and_empty_success_is_not_noop() {
    let f = Files::new();
    let input = f.write("input.icf", &fixture());
    let ctx = f.write("context.json", &serde_json::to_vec(&context(&f)).unwrap());
    let mut source = GPS.to_vec();
    source.extend_from_slice(b"B,OTHER,SECOND,,,1,2,,OFF\r\n");
    let csv = f.write("whole.csv", &source);
    let whole = f.path("whole.icf");
    exit(
        &cli(
            &[
                "import",
                p(&input),
                "--csv",
                p(&csv),
                "--sheet",
                "gps",
                "--csv-context",
                p(&ctx),
                "--yes",
                "-o",
                p(&whole),
            ],
            b"",
        ),
        0,
    );
    let old_bytes = fs::read(&whole).unwrap();
    let old = Icf::parse(&old_bytes).unwrap();
    let other_slot = old.gps_groups().unwrap()[2].physical_slots()[0];
    let other = old
        .physical_gps_record_raw(other_slot)
        .unwrap()
        .unwrap()
        .to_vec();
    let csv = f.write(
        "selected.csv",
        b"Latitude,Longitude,Name\r-0,0,NEW\r\0bad data ignored",
    );
    let selected = f.path("selected.icf");
    exit(
        &cli(
            &[
                "import",
                p(&whole),
                "--csv",
                p(&csv),
                "--sheet",
                "gps",
                "--group",
                "1",
                "--csv-context",
                p(&ctx),
                "--yes",
                "-o",
                p(&selected),
            ],
            b"",
        ),
        0,
    );
    let selected_bytes = fs::read(&selected).unwrap();
    let image = Icf::parse(&selected_bytes).unwrap();
    let groups = image.gps_groups().unwrap();
    assert_eq!(groups[1].physical_slots().len(), 1);
    assert_eq!(groups[2].name_bytes(), Some(b"OTHER           ".as_slice()));
    assert_eq!(
        image
            .physical_gps_record_raw(groups[2].physical_slots()[0])
            .unwrap()
            .unwrap(),
        other
    );
    assert_eq!(
        &image
            .physical_gps_record_raw(groups[1].physical_slots()[0])
            .unwrap()
            .unwrap()[..4],
        &[0, 0, 0, b'S']
    );
    let empty = f.write("empty.csv", b"Latitude,Longitude\n");
    let cleared = f.path("cleared.icf");
    let out = cli(
        &[
            "import",
            p(&selected),
            "--csv",
            p(&empty),
            "--sheet",
            "gps",
            "--group",
            "1",
            "--csv-context",
            p(&ctx),
            "--yes",
            "-o",
            p(&cleared),
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(report(&out)["changed"], true);
    let cleared_bytes = fs::read(cleared).unwrap();
    let image = Icf::parse(&cleared_bytes).unwrap();
    assert!(image.gps_groups().unwrap()[1].physical_slots().is_empty());
    assert_eq!(image.gps_groups().unwrap()[2].physical_slots().len(), 1);
}

#[test]
fn all_routes_help_and_native_unsupported_history_import_are_explicit() {
    for command in ["import", "export"] {
        let out = cli(&[command, "--help"], b"");
        exit(&out, 0);
        assert!(String::from_utf8_lossy(&out.stdout).contains("--csv-context"));
    }
    for sheet in [
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
        "all",
    ] {
        exit(
            &cli(
                &[
                    "import",
                    "missing",
                    "--csv",
                    "missing",
                    "--sheet",
                    sheet,
                    "--mode",
                    "cancel",
                    "--dry-run",
                ],
                b"",
            ),
            0,
        );
    }
    for sheet in ["from-dv", "from-fm", "to", "rx"] {
        let out = cli(
            &[
                "import",
                "missing",
                "--csv",
                "missing",
                "--sheet",
                sheet,
                "--yes",
                "--dry-run",
            ],
            b"",
        );
        exit(&out, 3);
        assert!(
            report(&out)["error"]
                .as_str()
                .unwrap()
                .contains("NULL import callbacks")
        );
    }
    let f = Files::new();
    let input = f.write("input.icf", &fixture());
    let ctx = f.write("context.json", &serde_json::to_vec(&context(&f)).unwrap());
    let csv = f.write("source.csv", GPS);
    let keep = f.write("keep.icf", b"KEEP");
    let mut args = vec![
        "import",
        p(&input),
        "--csv",
        p(&csv),
        "--sheet",
        "gps",
        "--csv-context",
        p(&ctx),
        "--yes",
        "-o",
        p(&keep),
    ];
    exit(&cli(&args, b""), 4);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    args.push("--dry-run");
    let dry = cli(&args, b"");
    exit(&dry, 0);
    assert!(dry.stdout.is_empty());
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    let bytes = serde_json::to_vec(&context(&f)).unwrap();
    exit(
        &cli(
            &[
                "export",
                p(&input),
                "--sheet",
                "gps",
                "--csv-context",
                "-",
                "--dry-run",
            ],
            &bytes,
        ),
        0,
    );
}

#[test]
fn copied_csv_and_live_rtl_points_are_independent_and_context_is_not_a_packet_provider() {
    let f = Files::new();
    let input = f.write("input.icf", &fixture());
    let csv = f.write(
        "source.csv",
        b"Latitude;Longitude;Date;Time\n0.5;1.5;2026/11/10;01:02:03,4\n",
    );
    let mut value = context(&f);
    value["separator"] = json!(";");
    value["rtl_decimal"] = json!(",");
    let ctx = f.write("context.json", &serde_json::to_vec(&value).unwrap());
    let output = f.path("output.icf");
    exit(
        &cli(
            &[
                "import",
                p(&input),
                "--csv",
                p(&csv),
                "--sheet",
                "gps",
                "--csv-context",
                p(&ctx),
                "--yes",
                "-o",
                p(&output),
            ],
            b"",
        ),
        0,
    );
    let bytes = fs::read(output).unwrap();
    let image = Icf::parse(&bytes).unwrap();
    let raw = image.physical_gps_record_raw(0).unwrap().unwrap();
    assert_eq!(&raw[..8], &[0, 0x75, 0x30, b'N', 1, 0x75, 0x30, b'E']);
    assert_eq!(&raw[11..14], &[1, 2, 3]);
    for key in ["csv_rows", "packets", "formatter_output"] {
        let mut bad = value.clone();
        bad[key] = json!(["invented"]);
        let ctx = f.write("invalid.json", &serde_json::to_vec(&bad).unwrap());
        let out = cli(
            &[
                "import",
                p(&input),
                "--csv",
                p(&csv),
                "--sheet",
                "gps",
                "--csv-context",
                p(&ctx),
                "--yes",
                "--dry-run",
            ],
            b"",
        );
        exit(&out, 3);
        assert!(out.stdout.is_empty());
        assert!(
            report(&out)["error"]
                .as_str()
                .unwrap()
                .contains("unknown CSV context member")
        );
    }
}

#[test]
fn user_edge_process_uses_real_root_and_literal_schema_without_locale_or_row_provider() {
    let f = Files::new();
    let input = f.write("input.icf", &fixture());
    let ctx = f.write(
        "edge.json",
        &serde_json::to_vec(&with_document(
            &f,
            json!({"profile":"portable-ascii","decimal":".","separator":",","range":[0,1]}),
        ))
        .unwrap(),
    );
    let csv = f.write(
        "source.csv",
        b"No,Frequency Low,Frequency High\r1,2,1\r2,3,4\0ignored",
    );
    let output = f.path("edge.icf");
    let out = cli(
        &[
            "import",
            p(&input),
            "--csv",
            p(&csv),
            "--sheet",
            "user-edge",
            "--csv-context",
            p(&ctx),
            "--yes",
            "-o",
            p(&output),
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(report(&out)["applied"], 2);
    let out = cli(
        &[
            "export",
            p(&output),
            "--sheet",
            "user-edge",
            "--csv-context",
            p(&ctx),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(
        out.stdout,
        b"No,Frequency Low,Frequency High\r\n01,2.000000,1.000000\r\n02,3.000000,4.000000\r\n"
    );
    let bad = f.write(
        "bad.csv",
        b"No,Frequency Low,Frequency High\n1,7,8\n2,bad,4\n",
    );
    let keep = f.write("keep.icf", b"KEEP");
    let mut args = vec![
        "import",
        p(&output),
        "--csv",
        p(&bad),
        "--sheet",
        "user-edge",
        "--csv-context",
        p(&ctx),
        "--yes",
        "--overwrite",
        "-o",
        p(&keep),
    ];
    let out = cli(&args, b"");
    exit(&out, 3);
    assert_eq!(report(&out)["counts"]["c2"], 1);
    assert_eq!(report(&out)["applied"], 1);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    args.push("--accept-partial");
    exit(&cli(&args, b""), 5);
    let out = cli(
        &[
            "export",
            p(&keep),
            "--sheet",
            "user-edge",
            "--csv-context",
            p(&ctx),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(
        out.stdout,
        b"No,Frequency Low,Frequency High\r\n01,7.000000,8.000000\r\n02,,\r\n"
    );
    for sheet in ["bandstack", "memopad", "preset"] {
        for command in ["import", "export"] {
            let mut args = vec![command, "missing", "--sheet", sheet, "--dry-run", "--yes"];
            if command == "import" {
                args.extend(["--csv", "missing"]);
            } else {
                args.pop();
            }
            let out = cli(&args, b"");
            exit(&out, 3);
            assert!(
                report(&out)["error"]
                    .as_str()
                    .unwrap()
                    .contains("not a registered parent")
            );
        }
    }
}

#[test]
fn ur_process_uses_shared_working_root_and_keeps_partial_prefix_explicit() {
    let f = Files::new();
    let source = fixture();
    let mut image = Icf::parse(&source).unwrap();
    image.patch_memory(&[(0x3c0a7, &[0xff; 600])]).unwrap();
    let input = f.write("input.icf", &image.to_bytes());
    let seed = f.write("generation.icf", &image.to_bytes());
    let ctx = f.write(
        "ur.json",
        &serde_json::to_vec(&with_document(
            &f,
            json!({"profile":"portable-ascii","decimal":".","separator":",","seed_icf":p(&seed)}),
        ))
        .unwrap(),
    );
    let csv = f.write(
        "source.csv",
        b"Call Sign,Name\nab1,12345678901234567\n\xa7,FAIL\n",
    );
    let keep = f.write("output.icf", b"KEEP");
    let mut args = vec![
        "import",
        p(&input),
        "--csv",
        p(&csv),
        "--sheet",
        "ur",
        "--csv-context",
        p(&ctx),
        "--yes",
        "--overwrite",
        "-o",
        p(&keep),
    ];
    let out = cli(&args, b"");
    exit(&out, 3);
    assert_eq!(report(&out)["applied"], 1);
    assert_eq!(report(&out)["counts"]["c3"], 3);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    args.push("--accept-partial");
    exit(&cli(&args, b""), 5);
    let out = cli(
        &[
            "export",
            p(&keep),
            "--sheet",
            "ur",
            "--csv-context",
            p(&ctx),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(
        out.stdout,
        b"No,Name,Call Sign\r\n01,1234567890123456,AB1\r\n"
    );
    let malformed = f.write("header.csv", b"Name,Repeater Call Sign\nX,A1\n");
    let fresh = f.path("not-published.icf");
    exit(
        &cli(
            &[
                "import",
                p(&input),
                "--csv",
                p(&malformed),
                "--sheet",
                "ur",
                "--csv-context",
                p(&ctx),
                "--yes",
                "--accept-partial",
                "-o",
                p(&fresh),
            ],
            b"",
        ),
        3,
    );
    assert!(!fresh.exists());
    let nested = f.write(
        "stdin.json",
        br#"{"profile":"portable-ascii","decimal":".","separator":",","seed_icf":"-"}"#,
    );
    exit(
        &cli(
            &[
                "import",
                "-",
                "--csv",
                p(&csv),
                "--sheet",
                "ur",
                "--csv-context",
                p(&nested),
                "--yes",
                "--dry-run",
            ],
            b"",
        ),
        3,
    );
}

#[test]
fn scan_export_process_keeps_native_zero_base_empty_rows_and_range_boundary() {
    let f = Files::new();
    let bytes = fixture();
    let mut image = Icf::parse(&bytes).unwrap();
    image
        .patch_memory(&[
            (0xa60b, &[0xff, 0xff, 0x7f, 0xff]),
            (
                0xa5d5,
                &[0x80, 0xa1, 0x03, 0x1c, 0x80, 0xa1, 0x03, 0x1c, 0xff, 1, 2],
            ),
            (0xa5e0, b"LAST            "),
        ])
        .unwrap();
    let original = image.to_bytes();
    let input = f.write("scan.icf", &original);
    let mut value =
        json!({"profile":"portable-ascii","decimal":".","separator":",","range":[23,24]});
    let ctx = f.write("scan.json", &serde_json::to_vec(&value).unwrap());
    let args = [
        "export",
        p(&input),
        "--sheet",
        "scan",
        "--csv-context",
        p(&ctx),
        "-o",
        "-",
    ];
    let out = cli(&args, b"");
    exit(&out, 0);
    assert_eq!(report(&out)["sheet"], "scan");
    assert_eq!(report(&out)["rows"], 2);
    assert_eq!(out.stdout, b"No,Name,Frequency Low,Frequency High,Mode,DATA,Filter\r\n23,LAST,470.000000,470.000000,-,ON,3\r\n24,,,,,,\r\n");
    value.as_object_mut().unwrap().remove("range");
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    let out = cli(&args, b"");
    exit(&out, 0);
    assert_eq!(report(&out)["rows"], 25);
    assert_eq!(out.stdout.iter().filter(|b| **b == b'\n').count(), 26);
    assert!(
        out.stdout
            .ends_with(b"23,LAST,470.000000,470.000000,-,ON,3\r\n24,,,,,,\r\n")
    );
    value["range"] = json!([24, 25]);
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    let keep = f.write("keep.csv", b"KEEP");
    let mut rejected = args.to_vec();
    *rejected.last_mut().unwrap() = p(&keep);
    rejected.push("--overwrite");
    exit(&cli(&rejected, b""), 3);
    assert_eq!(fs::read(keep).unwrap(), b"KEEP");
    assert_eq!(fs::read(input).unwrap(), original);
}

#[test]
fn scan_import_process_reaches_finalizer_typed_store_and_explicit_partial_publication() {
    let f = Files::new();
    let input = f.write("scan.icf", &fixture());
    let ctx = f.write(
        "scan.json",
        &serde_json::to_vec(&with_document(
            &f,
            json!({"profile":"portable-ascii","decimal":".","separator":",","range":[0,1]}),
        ))
        .unwrap(),
    );
    let csv=f.write("scan.csv",b"No,Name,Frequency Low,Frequency High,Mode,DATA,Filter\n0,FIRST,2,1,WFM,ON,1\n1,SECOND,3,4,FM,ON,2\n");
    let output = f.path("imported.icf");
    let out = cli(
        &[
            "import",
            p(&input),
            "--csv",
            p(&csv),
            "--sheet",
            "scan",
            "--csv-context",
            p(&ctx),
            "--yes",
            "-o",
            p(&output),
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(report(&out)["sheet"], "scan");
    assert_eq!(report(&out)["applied"], 2);
    assert_eq!(report(&out)["counts"]["c3"], 1);
    let out = cli(
        &[
            "export",
            p(&output),
            "--sheet",
            "scan",
            "--csv-context",
            p(&ctx),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(out.stdout,b"No,Name,Frequency Low,Frequency High,Mode,DATA,Filter\r\n00,FIRST,2.000000,1.000000,WFM,OFF,-\r\n01,SECOND,3.000000,4.000000,FM,ON,2\r\n");
    let partial = f.write(
        "partial.csv",
        b"No,Name,Frequency Low,Frequency High\n0,NEW,5,6\n1,BAD,bad,4\n",
    );
    let keep = f.write("keep.icf", b"KEEP");
    let mut args = vec![
        "import",
        p(&output),
        "--csv",
        p(&partial),
        "--sheet",
        "scan",
        "--csv-context",
        p(&ctx),
        "--yes",
        "--overwrite",
        "-o",
        p(&keep),
    ];
    let out = cli(&args, b"");
    exit(&out, 3);
    assert_eq!(report(&out)["counts"]["c2"], 1);
    assert_eq!(report(&out)["applied"], 1);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    args.push("--accept-partial");
    exit(&cli(&args, b""), 5);
    let out = cli(
        &[
            "export",
            p(&keep),
            "--sheet",
            "scan",
            "--csv-context",
            p(&ctx),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(out.stdout,b"No,Name,Frequency Low,Frequency High,Mode,DATA,Filter\r\n00,NEW,5.000000,6.000000,-,OFF,-\r\n01,,,,,,\r\n");
}

#[test]
fn scan_import_conditional_g_failure_keeps_native_prefix_without_implicit_seed() {
    let f = Files::new();
    let bytes = fixture();
    let mut image = Icf::parse(&bytes).unwrap();
    image
        .patch_memory(&[(0x37802, &[0x97]), (0x37807, &[0x81]), (0x37806, &[8])])
        .unwrap();
    let input = f.write("scan.icf", &image.to_bytes());
    let mut value = with_document(
        &f,
        json!({"profile":"portable-ascii","decimal":".","separator":",","range":[5,5]}),
    );
    let ctx = f.write("scan.json", &serde_json::to_vec(&value).unwrap());
    let csv = f.write("empty.csv", b"Frequency Low,Frequency High\n");
    let keep = f.write("keep.icf", b"KEEP");
    let mut args = vec![
        "import",
        p(&input),
        "--csv",
        p(&csv),
        "--sheet",
        "scan",
        "--csv-context",
        p(&ctx),
        "--yes",
        "--overwrite",
        "-o",
        p(&keep),
    ];
    let out = cli(&args, b"");
    exit(&out, 3);
    assert_eq!(report(&out)["applied"], 0);
    assert_eq!(report(&out)["changed"], true);
    assert!(!report(&out)["failure"].is_null());
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    args.push("--accept-partial");
    exit(&cli(&args, b""), 5);
    let bytes = fs::read(&keep).unwrap();
    let prefix = Icf::parse(&bytes).unwrap();
    assert_eq!(
        (prefix.memory()[0x37802], prefix.memory()[0x37807]),
        (0x83, 0x80)
    );
    assert_eq!(&prefix.memory()[0x37810..0x37814], &[0; 4]);
    let generation_bytes = fixture();
    let mut generation = Icf::parse(&generation_bytes).unwrap();
    generation
        .patch_memory(&[(0x37810, &[0x78, 0x56, 0x34, 0x12])])
        .unwrap();
    let seed = f.write("generation.icf", &generation.to_bytes());
    value["seed_icf"] = json!(p(&seed));
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    args.pop();
    let out = cli(&args, b"");
    exit(&out, 0);
    assert!(report(&out)["failure"].is_null());
    let bytes = fs::read(&keep).unwrap();
    let result = Icf::parse(&bytes).unwrap();
    // Stage9 suffix copies its completed document G, not the CSV reader seed.
    let document_g = fs::read(value["document_generation"].as_str().unwrap()).unwrap();
    assert_eq!(&document_g[0x37810..0x37814], &[0; 4]);
    assert_eq!(
        &result.memory()[0x37810..0x37814],
        &document_g[0x37810..0x37814]
    );
}

fn repeater_fixture() -> Vec<u8> {
    use cs705::{
        icf::record::{self, ChannelField as C, RepeaterField as F, RepeaterText as T},
        repeater::working::Working,
    };
    let bytes = fixture();
    let image = Icf::parse(&bytes).unwrap();
    let q = cs705::icf::context::Context::from_bytes([0; 24]);
    let context = record::repeater_default_context(&q, None).unwrap();
    let mut working = Working::from_icf(&image, context).unwrap();
    let seed = record::reset_holder(12, 0, 0);
    for (group, hz, name, call, used) in [
        (0, 145_125_000, *b"FIRST           ", *b"CALLA  B", 0),
        (3, 433_500_000, *b"SECOND          ", *b"CALLB  B", 1),
    ] {
        assert!(
            working
                .insert_one(
                    group,
                    0,
                    &seed,
                    |_| Ok(context.holder(group as u32, 0)),
                    |_, _| Ok(65535)
                )
                .unwrap()
        );
        let mut row = context.holder(group as u32, 0);
        row.set_valid(true);
        row.set_name(&name);
        row.set_channel_field(C::FrequencyHz, hz);
        row.set_channel_field(C::Duplex, 1);
        row.set_offset_hz(600_000);
        row.set_repeater_field(F::Mode, 2);
        row.set_repeater_field(F::Use, used);
        row.set_repeater_text(T::Call, &call);
        working.save(if group == 0 { 0 } else { 1 }, &row).unwrap();
        working
            .rename_group_raw(
                group,
                if group == 0 {
                    *b"HOME            "
                } else {
                    *b"REMOTE          "
                },
            )
            .unwrap();
    }
    working.to_icf().unwrap().to_bytes()
}

fn repeater_context(files: &Files, seed: &Path) -> Value {
    let mut value = context(files);
    value["generation_q_hex"] = json!("000000000000000000000000000000000000000000000000");
    value["live_q_hex"] = json!("000000000000000000000000000000000000000000000000");
    value["crt_decimal"] = json!(".");
    value["repeater_group_name"] = json!("                ");
    value["seed_icf"] = json!(p(seed));
    value
}

#[test]
fn memory_and_call_export_process_use_actual_typed_pairs_not_json_rows() {
    use cs705::icf::{
        record::{self, ChannelField},
        record_image::{self, ChannelKind},
        rows::NormalMemory,
    };
    let f = Files::new();
    let mut raw = vec![0; 0x4997a];
    raw[0x355a1] = 6;
    raw[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
    raw[0x3fb40..0x3fb44].copy_from_slice(&1_u32.to_le_bytes());
    for cell in raw[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
        cell.copy_from_slice(&0xffff_u32.to_le_bytes());
    }
    raw[0x3662d..0x36821].fill(1);
    raw[0x355c3..0x355d3].copy_from_slice(b"LIVE GROUP      ");
    for side in 6..8 {
        let mut holder = record::reset_holder(3, side, 0);
        holder.set_name(b"CALL ROW        ");
        holder.set_mode(side % 2);
        holder.set_filter(0);
        holder.set_channel_field(ChannelField::FrequencyHz, 145_000_000 + side % 2 * 500_000);
        record_image::store_channel_into(
            &mut raw,
            ChannelKind::Call,
            side,
            &mut holder,
            |_| Ok(()),
        )
        .unwrap();
    }
    let mut normal = NormalMemory::from_working(raw).unwrap();
    normal.allocate(0, 4..5).unwrap();
    for side in 8..10 {
        let mut holder = record::reset_holder(2, side, 0);
        holder.set_name(b"MEMORY ROW      ");
        holder.set_mode(side % 2);
        holder.set_filter(0);
        holder.set_channel_field(ChannelField::FrequencyHz, 145_000_000 + side % 2 * 500_000);
        normal.store_channel(side, &mut holder).unwrap();
    }
    let bytes = normal.to_icf().unwrap().to_bytes();
    let input = f.write("pairs.icf", &bytes);
    let mut value = json!({"profile":"portable-ascii","decimal":".","separator":","});
    let ctx = f.write("context.json", &serde_json::to_vec(&value).unwrap());
    let out = cli(
        &[
            "export",
            p(&input),
            "--sheet",
            "memory",
            "--csv-context",
            p(&ctx),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(report(&out)["rows"], 1);
    assert!(
        out.stdout
            .starts_with(b"Group No,Group Name,CH No,Name,SEL,Frequency,Dup,Offset,")
    );
    assert!(out.stdout.ends_with(b"00,LIVE GROUP,04,MEMORY ROW,OFF,145.000000,OFF,0.000000,LSB,OFF,1,,,,,,,,,,,OFF,145.500000,OFF,0.000000,USB,OFF,1,,,,,,,,,,\r\n"));
    value["range"] = json!([6, 7]);
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    let out = cli(
        &[
            "export",
            p(&input),
            "--sheet",
            "call",
            "--csv-context",
            p(&ctx),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(report(&out)["rows"], 1);
    assert!(out.stdout.starts_with(b"CH No,Name,Frequency,Dup,Offset,"));
    assert!(out.stdout.ends_with(b"04,CALL ROW,145.000000,OFF,0.000000,LSB,OFF,1,,,,,,,,,,,OFF,145.500000,OFF,0.000000,USB,OFF,1,,,,,,,,,,\r\n"));
    let keep = f.write("keep.csv", b"KEEP");
    exit(
        &cli(
            &[
                "export",
                p(&input),
                "--sheet",
                "memory",
                "--csv-context",
                p(&ctx),
                "--overwrite",
                "-o",
                p(&keep),
            ],
            b"",
        ),
        3,
    );
    assert_eq!(fs::read(keep).unwrap(), b"KEEP");
    assert_eq!(fs::read(input).unwrap(), bytes);
}

#[test]
fn memory_import_process_reaches_map_pair_stores_without_row_packets_and_preserves_file_boundaries()
{
    use cs705::icf::{record, record_image::ChannelKind, rows::NormalMemory};
    let f = Files::new();
    let original = memory_import_fixture();
    let input = f.write("memory.icf", &original);
    let seed = f.write("g.icf", &original);
    let mut value = memory_context(&f, &seed);
    value["range"] = json!([5, 5]);
    let ctx = f.write("context.json", &serde_json::to_vec(&value).unwrap());
    let csv=b"Group No,Group Name,CH No,Name,SEL,Frequency,Offset,Mode,Split,Frequency(2),Offset(2),Mode(2)\n99,NEW,2,PAIR;NAME,SEL2,145.125,0,LSB,ON,145.5,0,USB\n";
    let output = f.path("imported.icf");
    let mut args = vec![
        "import",
        p(&input),
        "--csv",
        "-",
        "--sheet",
        "memory",
        "--group",
        "0",
        "--csv-context",
        p(&ctx),
        "--yes",
        "-o",
        p(&output),
    ];
    let out = cli(&args, csv);
    exit(&out, 0);
    let summary = report(&out);
    assert_eq!(summary["applied"], 3);
    assert_eq!(summary["counts"]["c4"], 1);
    assert_eq!(summary["memory_effects"]["home_invalidated"], true);
    let saved = fs::read(&output).unwrap();
    let image = Icf::parse(&saved).unwrap();
    let model = NormalMemory::from_icf(&image).unwrap();
    assert_eq!(model.slot(0, 5).unwrap(), None);
    assert!(model.slot(0, 2).unwrap().is_some());
    assert!(model.slot(2, 3).unwrap().is_some());
    assert_eq!(image.memory()[0x392fa], 255);
    value.as_object_mut().unwrap().remove("range");
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    let out = cli(
        &[
            "export",
            p(&output),
            "--sheet",
            "memory",
            "--group",
            "0",
            "--csv-context",
            p(&ctx),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&out, 0);
    assert!(out.stdout.ends_with(b"00,NEW,02,PAIR NAME,SEL2,145.125000,OFF,0.000000,LSB,OFF,1,,,,,,,,,,,ON,145.500000,OFF,0.000000,USB,OFF,1,,,,,,,,,,\r\n"));
    let keep = f.write("keep.icf", b"KEEP");
    let last = args.len() - 1;
    args[last] = p(&keep);
    exit(&cli(&args, csv), 4);
    args.push("--dry-run");
    exit(&cli(&args, csv), 0);
    // CSV removes tone50 when Q12!=0; stage9 removes it when Q12==0.
    // Both paths finish at24; captured B still owns the default offset.
    for (captured, live, gate, offset) in [
        (0, 1, 0, 0),
        (1, 0, 0, 600_000),
        (0, 1, 1, 0),
        (1, 0, 1, 600_000),
    ] {
        value["generation_q_hex"] = json!(format!("{captured:02x}{}", "00".repeat(23)));
        value["live_q_hex"] = json!(format!(
            "{live:02x}{}{gate:02x}{}",
            "00".repeat(17),
            "00".repeat(5)
        ));
        fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
        let output = f.path(&format!("source-{captured}-{gate}.icf"));
        let out = cli(&["import", p(&input), "--csv", "-", "--sheet", "memory", "--group", "0", "--csv-context", p(&ctx), "--yes", "-o", p(&output)],
            b"CH No,Name,Frequency,Mode,Offset,Repeater Tone,TSQL Frequency,Frequency(2)\n2,GENERATION,145,FM,100,150.0Hz,150.0Hz,\n");
        exit(&out, 0);
        assert_eq!(report(&out)["applied"], 2);
        let bytes = fs::read(output).unwrap();
        let image = Icf::parse(&bytes).unwrap();
        let mut holder = record::reset_holder(2, 4, 0);
        record::load_channel_image_into(&image, ChannelKind::Normal, 4, &mut holder).unwrap();
        assert_eq!(
            (holder.offset_hz(), holder.tx_tone(), holder.rx_tone()),
            (offset, 24, 24)
        );
    }
    assert_eq!(fs::read(keep).unwrap(), b"KEEP");
    assert_eq!(fs::read(input).unwrap(), original);
}

#[test]
fn memory_whole_mixed_groups_use_per_group_sequential_no_and_real_staged_names() {
    use cs705::icf::rows::NormalMemory;
    let f = Files::new();
    let original = memory_import_fixture();
    let input = f.write("memory.icf", &original);
    let seed = f.write("g.icf", &original);
    let ctx = f.write(
        "context.json",
        &serde_json::to_vec(&memory_context(&f, &seed)).unwrap(),
    );
    let csv=f.write("whole.csv",b"Group No,Group Name,CH No,Name,Frequency,Offset,Mode,Frequency(2),Offset(2),Mode(2)\n2,THIRD,87,C,433,0,LSB,433.5,0,USB\n0,FIRST,53,A,145,0,LSB,145.5,0,USB\n");
    let output = f.path("whole.icf");
    let out = cli(
        &[
            "import",
            p(&input),
            "--csv",
            p(&csv),
            "--sheet",
            "memory",
            "--csv-context",
            p(&ctx),
            "--yes",
            "-o",
            p(&output),
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(report(&out)["applied"], 6);
    assert_eq!(report(&out)["counts"]["c4"], 2);
    let saved = fs::read(output).unwrap();
    let image = Icf::parse(&saved).unwrap();
    let model = NormalMemory::from_icf(&image).unwrap();
    assert_eq!((model.group_count(), model.used_slots()), (3, 2));
    assert!(model.slot(0, 0).unwrap().is_some());
    assert!(model.slot(2, 0).unwrap().is_some());
    assert_eq!(
        (
            model.slot(0, 53).unwrap(),
            model.slot(2, 87).unwrap(),
            model.slot(0, 5).unwrap(),
            model.slot(2, 3).unwrap()
        ),
        (None, None, None, None)
    );
    let groups = image.memory_groups().unwrap();
    assert_eq!(groups[0].name_bytes(), b"FIRST           ");
    assert_eq!(groups[2].name_bytes(), b"THIRD           ");
    assert_eq!(image.memory()[0x3c84b], 255);
}

#[test]
fn memory_late_error_and_missing_g_never_publish_prefix_without_explicit_acceptance() {
    use cs705::icf::{
        record::{self, ChannelField},
        record_image::ChannelKind,
        rows::NormalMemory,
    };
    let f = Files::new();
    let original = memory_import_fixture();
    let input = f.write("memory.icf", &original);
    let seed = f.write("g.icf", &original);
    let mut value = memory_context(&f, &seed);
    let ctx = f.write("context.json", &serde_json::to_vec(&value).unwrap());
    let mut source=b"CH No,Name,Frequency,Offset,Mode,Split,Frequency(2),Mode(2),Offset(2)\n0,PREFIX,145,0,LSB,ON,433,LSB,".to_vec();
    source.extend_from_slice(&vec![b'9'; 4097]);
    source.push(b'\n');
    let csv = f.write("late.csv", &source);
    let keep = f.write("keep.icf", b"KEEP");
    let mut args = vec![
        "import",
        p(&input),
        "--csv",
        p(&csv),
        "--sheet",
        "memory",
        "--group",
        "0",
        "--csv-context",
        p(&ctx),
        "--yes",
        "--overwrite",
        "-o",
        p(&keep),
    ];
    let out = cli(&args, b"");
    exit(&out, 3);
    assert_eq!(report(&out)["applied"], 2);
    assert_eq!(report(&out)["counts"]["c4"], 0);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    args.push("--accept-partial");
    exit(&cli(&args, b""), 5);
    let saved = fs::read(&keep).unwrap();
    let image = Icf::parse(&saved).unwrap();
    for side in 0..2 {
        let mut h = record::reset_holder(2, side, 0);
        record::load_channel_image_into(&image, ChannelKind::Normal, side, &mut h).unwrap();
        assert_eq!(
            (h.channel_field(ChannelField::FrequencyHz), h.split()),
            (145_000_000, 0)
        );
    }
    assert_eq!(image.memory()[0x392fa], 0);
    args.pop();
    value.as_object_mut().unwrap().remove("seed_icf");
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    fs::write(&csv, b"Frequency\n").unwrap();
    fs::write(&keep, b"KEEP").unwrap();
    let out = cli(&args, b"");
    exit(&out, 3);
    assert_eq!(report(&out)["rows_read"], 0);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    args.push("--accept-partial");
    exit(&cli(&args, b""), 5);
    let saved = fs::read(&keep).unwrap();
    let image = Icf::parse(&saved).unwrap();
    assert_eq!(
        NormalMemory::from_icf(&image).unwrap().slot(0, 5).unwrap(),
        None
    );
    assert_eq!(&image.memory()[0x355c3..0x355d3], b"OLD GROUP       ");
}

#[test]
fn call_import_process_distinguishes_real_lower_blank_from_unwritten_default_source() {
    use cs705::icf::{
        record::{self, ChannelField},
        record_image,
    };
    let f = Files::new();
    let original = memory_import_fixture();
    let input = f.write("call.icf", &original);
    let seed = f.write("g.icf", &original);
    let ctx = f.write(
        "context.json",
        &serde_json::to_vec(&memory_context(&f, &seed)).unwrap(),
    );
    let csv = f.write(
        "call.csv",
        b"CH No,Name,Frequency,Offset,Mode,Split,Frequency(2)\n1,CALL ROW,145.25,0,LSB,ON,\n",
    );
    let output = f.path("call-imported.icf");
    let mut args = vec![
        "import",
        p(&input),
        "--csv",
        p(&csv),
        "--sheet",
        "call",
        "--csv-context",
        p(&ctx),
        "--yes",
        "-o",
        p(&output),
    ];
    let out = cli(&args, b"");
    exit(&out, 0);
    assert_eq!(report(&out)["applied"], 2);
    assert_eq!(report(&out)["counts"]["c4"], 1);
    assert_eq!(report(&out)["counts"]["c3"], 1);
    assert_eq!(report(&out)["memory_effects"]["call_invalidated"], true);
    fs::write(
        &csv,
        b"CH No,Name,Frequency,Offset,Mode,Split,Frequency(2)\n1,CALL ROW,145.25,0,LSB,ON,433\n",
    )
    .unwrap();
    let keep = f.write("keep.icf", b"KEEP");
    let last = args.len() - 1;
    args[last] = p(&keep);
    args.push("--overwrite");
    let out = cli(&args, b"");
    exit(&out, 3);
    assert_eq!(report(&out)["applied"], 2);
    assert_eq!(report(&out)["counts"]["c4"], 0);
    assert!(
        report(&out)["failure"]
            .as_str()
            .unwrap()
            .contains("parser-local No is unwritten")
    );
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    args.push("--accept-partial");
    exit(&cli(&args, b""), 5);
    let saved = fs::read(keep).unwrap();
    let image = Icf::parse(&saved).unwrap();
    for side in 0..2 {
        let mut h = record::reset_holder(3, side, 0);
        record_image::load_into(image.memory(), 3, side, &mut h).unwrap();
        assert_eq!(
            (h.channel_field(ChannelField::FrequencyHz), h.split()),
            (145_250_000, 0)
        );
    }
}

#[test]
fn repeater_whole_modes_keep_real_save_count_and_explicit_scope() {
    let f = Files::new();
    let bytes = repeater_fixture();
    let input = f.write("repeaters.icf", &bytes);
    let seed = f.write("seed.icf", &bytes);
    let ctx = f.write(
        "context.json",
        &serde_json::to_vec(&repeater_context(&f, &seed)).unwrap(),
    );
    let csv=f.write("whole.csv",b"Group No,Group Name,Name,Repeater Call Sign,Frequency,Mode,RPT1USE\n1,HOME,UPDATED,CALLA  B,145.125,DV,YES\n");
    for (mode, applied, other, used) in [
        ("preserve-order-use", 2, 1, false),
        ("preserve-use", 1, 0, false),
        ("replace", 1, 0, true),
    ] {
        let output = f.path(mode);
        let out = cli(
            &[
                "import",
                p(&input),
                "--csv",
                p(&csv),
                "--sheet",
                "repeater",
                "--mode",
                mode,
                "--csv-context",
                p(&ctx),
                "--yes",
                "-o",
                p(&output),
            ],
            b"",
        );
        exit(&out, 0);
        assert_eq!(report(&out)["applied"], applied);
        assert_eq!(report(&out)["counts"]["c4"], 1);
        let bytes = fs::read(output).unwrap();
        let image = Icf::parse(&bytes).unwrap();
        assert_eq!(
            image.physical_repeater(0).unwrap().unwrap().use_repeater,
            used
        );
        assert_eq!(
            image.repeater_groups().unwrap()[3].physical_slots().len(),
            other
        );
    }
}

#[test]
fn repeater_all_three_modes_use_captured_b_offset_live_q_defaults_and_shared_applied_prefix() {
    let f = Files::new();
    let bytes = repeater_fixture();
    let input = f.write("repeaters.icf", &bytes);
    let mut generation = Icf::parse(&bytes).unwrap();
    generation
        .patch_memory(&[
            (0x3c8b9, &0x1234_u16.to_le_bytes()),
            (0x3c903, &0x5678_u16.to_le_bytes()),
        ])
        .unwrap();
    let seed = f.write("generation.icf", &generation.to_bytes());
    let mut value = repeater_context(&f, &seed);
    let csv=f.write("repeater.csv",b"Group No,Group Name,Name,Repeater Call Sign,Frequency,Dup,Offset,Mode,RPT1USE\n1,NEW,UPDATED,CALLA  B,145.125,DUP+,100,DV,YES\n");
    // F20C/56CC8 reads captured B, unlike type22's live-Q hemisphere source.
    for (captured, live, offset, duplex, longitude) in [
        (0, 1, 0, "OFF", "-0.000000"),
        (1, 0, 600_000, "DUP-", "0.000000"),
    ] {
        value["generation_q_hex"] = json!(format!("{captured:02x}{}", "00".repeat(23)));
        value["live_q_hex"] = json!(format!("{live:02x}{}", "00".repeat(23)));
        let ctx = f.write("repeater.json", &serde_json::to_vec(&value).unwrap());
        for (mode, used, name) in [
            ("preserve-order-use", false, b"HOME            ".as_slice()),
            ("preserve-use", false, b"NEW             "),
            ("replace", true, b"NEW             "),
        ] {
            let output = f.path(&format!("{captured}-{mode}"));
            let out = cli(
                &[
                    "import",
                    p(&input),
                    "--csv",
                    p(&csv),
                    "--sheet",
                    "repeater",
                    "--group",
                    "0",
                    "--mode",
                    mode,
                    "--csv-context",
                    p(&ctx),
                    "--yes",
                    "-o",
                    p(&output),
                ],
                b"",
            );
            exit(&out, 0);
            assert_eq!(report(&out)["applied"], 1);
            assert_eq!(report(&out)["counts"]["c4"], 1);
            let saved = fs::read(&output).unwrap();
            let image = Icf::parse(&saved).unwrap();
            let row = image.physical_repeater(0).unwrap().unwrap();
            assert_eq!((row.offset_hz, row.use_repeater), (offset, used));
            assert_eq!(image.repeater_groups().unwrap()[0].name_bytes(), name);
            assert_eq!(
                image.repeater_groups().unwrap()[3].physical_slots().len(),
                1
            );
            assert_eq!(&image.memory()[0x3c8b9..0x3c8bb], &0x1234_u16.to_le_bytes());
            // Stage9 suffix restores this secondary reference from document G
            // because the primary0x1234 reference lies outside2500 rows.
            let document_g = fs::read(value["document_generation"].as_str().unwrap()).unwrap();
            assert_eq!(&document_g[0x3c903..0x3c905], &u16::MAX.to_le_bytes());
            assert_eq!(
                &image.memory()[0x3c903..0x3c905],
                &document_g[0x3c903..0x3c905]
            );
            let exported = cli(
                &[
                    "export",
                    p(&output),
                    "--sheet",
                    "repeater",
                    "--group",
                    "0",
                    "--csv-context",
                    p(&ctx),
                    "-o",
                    "-",
                ],
                b"",
            );
            exit(&exported, 0);
            let group = if mode == "preserve-order-use" {
                "HOME"
            } else {
                "NEW"
            };
            let line = format!(
                "1,{group},UPDATED,,CALLA  B,,145.125000,{duplex},{:.6},DV,,,{},{},0.000000,{longitude},--:--\r\n",
                f64::from(offset) / 1_000_000.0,
                if used { "YES" } else { "NO" },
                "None"
            );
            assert!(
                exported.stdout.ends_with(line.as_bytes()),
                "{}",
                String::from_utf8_lossy(&exported.stdout)
            );
        }
    }
    assert_eq!(fs::read(input).unwrap(), bytes);
}

#[test]
fn repeater_staged_counts_are_not_applied_and_unwritten_heap_is_not_invented() {
    let f = Files::new();
    let bytes = repeater_fixture();
    let input = f.write("repeaters.icf", &bytes);
    let seed = f.write("generation.icf", &bytes);
    let value = repeater_context(&f, &seed);
    let ctx = f.write("repeater.json", &serde_json::to_vec(&value).unwrap());
    let bad = f.write(
        "staged.csv",
        b"Repeater Call Sign,Frequency,Mode\nCALLA  B,145.125,DV\n\xa7,145,DV\n",
    );
    let keep = f.write("keep.icf", b"KEEP");
    let out = cli(
        &[
            "import",
            p(&input),
            "--csv",
            p(&bad),
            "--sheet",
            "repeater",
            "--group",
            "0",
            "--mode",
            "preserve-order-use",
            "--csv-context",
            p(&ctx),
            "--yes",
            "--overwrite",
            "-o",
            p(&keep),
        ],
        b"",
    );
    exit(&out, 3);
    assert_eq!(report(&out)["counts"]["c4"], 1);
    assert_eq!(report(&out)["applied"], 0);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    let empty = f.write("empty.icf", &fixture());
    let csv=f.write("replace.csv",b"Group No,Group Name,Repeater Call Sign,Frequency,Mode,RPT1USE\n1,NEW,NEW    B,145,DV,YES\n");
    let mut args = vec![
        "import",
        p(&empty),
        "--csv",
        p(&csv),
        "--sheet",
        "repeater",
        "--group",
        "0",
        "--mode",
        "preserve-use",
        "--csv-context",
        p(&ctx),
        "--yes",
        "--overwrite",
        "-o",
        p(&keep),
    ];
    let out = cli(&args, b"");
    exit(&out, 3);
    assert_eq!(report(&out)["applied"], 1);
    assert_eq!(report(&out)["counts"]["c4"], 1);
    assert!(
        report(&out)["failure"]
            .as_str()
            .unwrap()
            .contains("unwritten three-word source")
    );
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    args.push("--accept-partial");
    exit(&cli(&args, b""), 5);
    let bytes = fs::read(keep).unwrap();
    let image = Icf::parse(&bytes).unwrap();
    assert_eq!(
        image.repeater_groups().unwrap()[0].physical_slots().len(),
        1
    );
}

#[test]
fn repeater_capacity_marker_does_not_replace_actual_applied_count() {
    use cs705::repeater::working::Working;
    let f = Files::new();
    let source = fixture();
    let mut memory = Icf::parse(&source).unwrap().memory().to_vec();
    memory.resize(0x4997a, 0);
    memory[0x2ea3b..0x2ea3b + 800].fill(b' ');
    for at in [0x392fd, 0x3c8b9, 0x3c903] {
        memory[at..at + 2].fill(255);
    }
    memory[0x49916..0x49918].copy_from_slice(&2498_u16.to_le_bytes());
    let working = Working::from_working(memory).unwrap();
    let bytes = working.to_icf().unwrap().to_bytes();
    let input = f.write("capacity.icf", &bytes);
    let seed = f.write("generation.icf", &bytes);
    let ctx = f.write(
        "repeater.json",
        &serde_json::to_vec(&repeater_context(&f, &seed)).unwrap(),
    );
    let csv = f.write(
        "three.csv",
        b"Repeater Call Sign,Frequency,Mode\nONE    B,145,DV\nTWO    B,145,DV\nTHREE  B,145,DV\n",
    );
    let output = f.path("partial.icf");
    let out = cli(
        &[
            "import",
            p(&input),
            "--csv",
            p(&csv),
            "--sheet",
            "repeater",
            "--group",
            "0",
            "--mode",
            "replace",
            "--csv-context",
            p(&ctx),
            "--yes",
            "--accept-partial",
            "-o",
            p(&output),
        ],
        b"",
    );
    exit(&out, 5);
    let summary = report(&out);
    assert_eq!(summary["applied"], 2);
    assert_eq!(summary["counts"]["c4"], 1);
    assert_eq!(summary["counts"]["s"], 1);
    let bytes = fs::read(output).unwrap();
    let image = Icf::parse(&bytes).unwrap();
    let groups = image.repeater_groups().unwrap();
    assert_eq!(
        (
            groups[0].physical_slots().len(),
            groups[1].physical_slots().len()
        ),
        (2, 2498)
    );
}

#[test]
fn csv_fix1_document_storage_rejects_inconsistent_normal_free_flags() {
    let f = Files::new();
    let bytes = fixture();
    let mut image = Icf::parse(&bytes).unwrap();
    // Empty normal maps require500 free slots; zero flags claim none are free.
    image.patch_memory(&[(0x3662d, &[0; 500])]).unwrap();
    let original = image.to_bytes();
    let input = f.write("inconsistent.icf", &original);
    let value = with_document(
        &f,
        json!({"profile":"portable-ascii","decimal":".","separator":",","range":[0,0]}),
    );
    let ctx = f.write("context.json", &serde_json::to_vec(&value).unwrap());
    let csv = f.write(
        "source.csv",
        b"No,Frequency Low,Frequency High\n1,145,146\n",
    );
    let output = f.write("keep.icf", b"KEEP");
    let mut args = vec![
        "import",
        p(&input),
        "--csv",
        p(&csv),
        "--sheet",
        "user-edge",
        "--csv-context",
        p(&ctx),
        "--yes",
        "--overwrite",
        "-o",
        p(&output),
    ];
    for partial in [false, true] {
        if partial {
            args.push("--accept-partial");
        }
        let out = cli(&args, b"");
        exit(&out, 3);
        assert_eq!(report(&out)["stage9_completed"], true);
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains("normal free chain reads empty native vector")
        );
        assert!(out.stdout.is_empty());
        assert_eq!(fs::read(&output).unwrap(), b"KEEP");
    }
    assert_eq!(fs::read(&input).unwrap(), original);
}

#[test]
fn repeater_export_process_consumes_storage_root_and_explicit_q_without_row_provider() {
    let f = Files::new();
    let original = repeater_fixture();
    let input = f.write("repeaters.icf", &original);
    let mut value = json!({"profile":"portable-ascii","decimal":".","separator":",","live_q_hex":"000000000000000000000000000000000000000000000000"});
    let ctx = f.write("repeater.json", &serde_json::to_vec(&value).unwrap());
    let args = [
        "export",
        p(&input),
        "--sheet",
        "repeater",
        "--csv-context",
        p(&ctx),
        "-o",
        "-",
    ];
    let out = cli(&args, b"");
    exit(&out, 0);
    let header=b"Group No,Group Name,Name,Sub Name,Repeater Call Sign,Gateway Call Sign,Frequency,Dup,Offset,Mode,TONE,Repeater Tone,RPT1USE,Position,Latitude,Longitude,UTC Offset\r\n";
    let first=b"1,HOME,FIRST,,CALLA  B,,145.125000,DUP-,0.600000,DV,,,NO,None,0.000000,0.000000,--:--\r\n";
    let second=b"4,REMOTE,SECOND,,CALLB  B,,433.500000,DUP-,0.600000,DV,,,YES,None,0.000000,0.000000,--:--\r\n";
    assert_eq!(report(&out)["sheet"], "repeater");
    assert_eq!(report(&out)["rows"], 2);
    assert_eq!(out.stdout, [header.as_slice(), first, second].concat());
    let mut selected = args.to_vec();
    selected.extend(["--group", "3"]);
    let out = cli(&selected, b"");
    exit(&out, 0);
    assert_eq!(out.stdout, [header.as_slice(), second].concat());
    value["live_q_hex"] = json!("100000000000000000000000000000000000000000000000");
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    let out = cli(&args, b"");
    exit(&out, 3);
    assert!(out.stdout.is_empty());
    value["system_langid"] = json!(1041);
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    exit(&cli(&args, b""), 0);
    value["range"] = json!([0, 1]);
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    let out = cli(&args, b"");
    exit(&out, 3);
    assert!(out.stdout.is_empty());
    assert_eq!(fs::read(input).unwrap(), original);
}

#[test]
fn history_csv_process_uses_typed_supplements_and_preserves_fm_csv_gate() {
    use cs705::{
        icf::record::{self, ChannelField as C, RepeaterField as F},
        repeater::working::Working,
    };
    let f = Files::new();
    let bytes = repeater_fixture();
    let image = Icf::parse(&bytes).unwrap();
    let defaults =
        record::repeater_default_context(&cs705::icf::context::Context::from_bytes([0; 24]), None)
            .unwrap();
    let mut working = Working::from_icf(&image, defaults).unwrap();
    let mut row = record::reset_holder(12, 1, 0);
    working.load_into(1, &mut row).unwrap();
    row.set_repeater_field(F::Mode, 0);
    row.set_channel_field(C::Duplex, 2);
    row.set_offset_hz(5_000_000);
    working.save(1, &row).unwrap();
    let mut image = working.to_icf().unwrap();
    image
        .patch_memory(&[
            (0x3c331, &[0xff; 10]),
            (0x3c33b, &[0xff; 10]),
            (0x3c34f, &[0xff; 20]),
            (0x3c0a7, &[0xff; 600]),
            (0x3c331, &[7, 2]),
            (0x32935 + 7 * 8, b"CALLA  B"),
            (0x32935 + 2 * 8, b"NOMATCH "),
            (0x3c33b, &[4, 5]),
            (
                0x32985 + 4 * 12,
                &[b"CALLB  B".as_slice(), &433_500_000_u32.to_le_bytes()].concat(),
            ),
            (
                0x32985 + 5 * 12,
                &[b"CALLB  B".as_slice(), &145_000_000_u32.to_le_bytes()].concat(),
            ),
            (0x3c34f, &[3, 4]),
            (0x32a4d + 3 * 8, b"/CALLA B"),
            (0x32a4d + 4 * 8, b"URCALL A"),
        ])
        .unwrap();
    let mut ur = cs705::icf::rows::ur_default(0, 0);
    ur.set_name(b"UR NAME         ");
    ur.set_callsign(cs705::icf::MemoryCallsign::Your, b"URCALL  ");
    record::store_callsign_storage_into(&mut image, 0, &ur).unwrap();
    image.patch_memory(&[(0x3c0a7, &[0, 0])]).unwrap();
    let original = image.to_bytes();
    let input = f.write("history.icf", &original);
    let mut value = json!({"profile":"portable-ascii","decimal":".","separator":",","live_q_hex":"000000000000000000000000000000000000000000000000"});
    let ctx = f.write("history.json", &serde_json::to_vec(&value).unwrap());
    for (sheet,expected) in [
        ("from-dv",b"Call Sign,Name,Frequency,Dup,Offset\r\nCALLA  B,FIRST,145.125000,DUP-,0.600000\r\nNOMATCH,,,,\r\n".as_slice()),
        ("from-fm",b"Call Sign,Name,Frequency,Dup,Offset\r\nCALLB  B,SECOND,433.500000,DUP+,5.000000\r\nCALLB  B,,,,\r\n"),
        ("to",b"Call Sign,Name\r\n/CALLA B,FIRST\r\nURCALL A,UR NAME\r\n"),
    ] {
        let out=cli(&["export",p(&input),"--sheet",sheet,"--csv-context",p(&ctx),"-o","-"],b"");exit(&out,0);
        assert_eq!(report(&out)["sheet"],sheet);assert_eq!(report(&out)["rows"],2);assert_eq!(out.stdout,expected);
    }
    value["range"] = json!([1, 1]);
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    let out = cli(
        &[
            "export",
            p(&input),
            "--sheet",
            "from-fm",
            "--csv-context",
            p(&ctx),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(
        out.stdout,
        b"Call Sign,Name,Frequency,Dup,Offset\r\nCALLB  B,,,,\r\n"
    );
    value.as_object_mut().unwrap().remove("live_q_hex");
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    let keep = f.write("keep.csv", b"KEEP");
    let out = cli(
        &[
            "export",
            p(&input),
            "--sheet",
            "to",
            "--csv-context",
            p(&ctx),
            "--overwrite",
            "-o",
            p(&keep),
        ],
        b"",
    );
    exit(&out, 3);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    let out = cli(
        &[
            "export",
            p(&input),
            "--sheet",
            "all",
            "--csv-context",
            p(&ctx),
            "--overwrite",
            "-o",
            p(&keep),
        ],
        b"",
    );
    exit(&out, 3);
    assert!(
        report(&out)["error"]
            .as_str()
            .unwrap()
            .contains("no single native schema")
    );
    assert_eq!(fs::read(keep).unwrap(), b"KEEP");
    assert_eq!(fs::read(input).unwrap(), original);
}

#[test]
fn rx_csv_process_consumes_reverse_rank_raw_and_captured_offset_without_packet_input() {
    let f = Files::new();
    let bytes = fixture();
    let mut image = Icf::parse(&bytes).unwrap();
    image
        .patch_memory(&[(0x3c2ff, &[255; 50]), (0x396ff, &60_i16.to_le_bytes())])
        .unwrap();
    for (physical, rank, call) in [(8, 0, b"FIRST   "), (20, 2, b"SECOND  ")] {
        let mut basic = [b' '; 48];
        basic[..8].copy_from_slice(call);
        basic[36..40].copy_from_slice(&433_500_000_u32.to_le_bytes());
        basic[40] = 9;
        basic[41..47].copy_from_slice(&[0x31, 0x12, 0x24, 0x23, 0x30, 0x45]);
        basic[47] = 0;
        let mut message = [b' '; 20];
        message[..9].copy_from_slice(b"RECE;IVED");
        let mut gps = [255; 92];
        gps[8..12].copy_from_slice(&i32::MAX.to_le_bytes());
        gps[12..14].copy_from_slice(b"/>");
        gps[26..29].copy_from_slice(&[0x23, 0x30, 0x45]);
        gps[29] = 0;
        gps[49..].fill(b' ');
        image
            .patch_memory(&[
                (0x3c2ff + physical, &[rank]),
                (0x309c3 + physical, &[0x40]),
                (0x309c3 + 0x32 + 48 * physical, &basic),
                (0x309c3 + 0x992 + 20 * physical, &message),
                (0x309c3 + 0xd7a + 92 * physical, &gps),
            ])
            .unwrap();
    }
    let original = image.to_bytes();
    let input = f.write("rx.icf", &original);
    let mut value = context(&f);
    let ctx = f.write("context.json", &serde_json::to_vec(&value).unwrap());
    let out = cli(
        &[
            "export",
            p(&input),
            "--sheet",
            "rx",
            "--csv-context",
            p(&ctx),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(report(&out)["sheet"], "rx");
    assert_eq!(report(&out)["rows"], 1);
    let lines = out.stdout.split(|b| *b == b'\n').collect::<Vec<_>>();
    assert_eq!(lines[0].split(|b| *b == b',').count(), 37);
    let expected = [
        "RF",
        "433.500000",
        "DV",
        "FIRST",
        "",
        "",
        "",
        "",
        "RECE IVED",
        "RPT UP",
        "2025/01/01 00:30:45",
        "",
        "",
        "",
        "",
        "",
        "",
        "APRS 2F3E",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "00:30:45",
        "\r",
    ]
    .join(",");
    assert_eq!(lines[1], expected.as_bytes());
    let symbol_at = 0x309c3 + 0xd7a + 92 * 8 + 12;
    for (raw, expected) in [
        ([0x81, 0xff], b"APRS 81FF".as_slice()),
        ([0, 0], b"APRS 0000"),
        (*b",;", b"APRS 2C3B"),
        ([255, b'>'], b""),
    ] {
        image.patch_memory(&[(symbol_at, &raw)]).unwrap();
        let bytes = image.to_bytes();
        fs::write(&input, &bytes).unwrap();
        let out = cli(
            &[
                "export",
                p(&input),
                "--sheet",
                "rx",
                "--csv-context",
                p(&ctx),
                "-o",
                "-",
            ],
            b"",
        );
        exit(&out, 0);
        let row = out.stdout.split(|b| *b == b'\n').nth(1).unwrap();
        let cells: Vec<_> = row.split(|b| *b == b',').collect();
        assert_eq!(cells.len(), 37);
        assert_eq!(cells[17], expected);
        assert_eq!(fs::read(&input).unwrap(), bytes);
        assert_eq!(
            &Icf::parse(&bytes).unwrap().memory()[symbol_at..symbol_at + 2],
            &raw
        );
    }
    fs::write(&input, &original).unwrap();
    value["range"] = json!([2, 2]);
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    let out = cli(
        &[
            "export",
            "-",
            "--sheet",
            "rx",
            "--csv-context",
            p(&ctx),
            "-o",
            "-",
        ],
        &original,
    );
    exit(&out, 0);
    assert!(
        out.stdout
            .windows(b"RF,433.500000,DV,SECOND,".len())
            .any(|w| w == b"RF,433.500000,DV,SECOND,")
    );
    let keep = f.write("keep.csv", b"KEEP");
    let mut args = vec![
        "export",
        p(&input),
        "--sheet",
        "rx",
        "--csv-context",
        p(&ctx),
        "-o",
        p(&keep),
    ];
    exit(&cli(&args, b""), 4);
    args.push("--dry-run");
    exit(&cli(&args, b""), 0);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    image
        .patch_memory(&[(0x396ff, &1440_i16.to_le_bytes())])
        .unwrap();
    fs::write(&input, image.to_bytes()).unwrap();
    args.pop();
    args.push("--overwrite");
    let out = cli(&args, b"");
    exit(&out, 3);
    assert_eq!(report(&out)["rows"], 0);
    assert!(report(&out)["prefix_bytes"].as_u64().unwrap() > 0);
    assert_eq!(fs::read(keep).unwrap(), b"KEEP");
}

#[test]
fn retired_symbol_asset_is_rejected_before_any_image_or_asset_read() {
    let f = Files::new();
    let ctx = f.write(
        "legacy.json",
        br#"{"profile":"portable-ascii","symbol_asset":"-"}"#,
    );
    let input = f.path("unavailable.icf");
    let keep = f.write("keep.csv", b"KEEP");
    let out = cli(
        &[
            "export",
            p(&input),
            "--sheet",
            "rx",
            "--csv-context",
            p(&ctx),
            "--overwrite",
            "-o",
            p(&keep),
        ],
        b"",
    );
    exit(&out, 3);
    assert!(
        report(&out)["error"]
            .as_str()
            .unwrap()
            .contains("unknown CSV context member")
    );
    assert!(out.stdout.is_empty());
    assert_eq!(fs::read(keep).unwrap(), b"KEEP");
}

fn native_date_fixture() -> Vec<u8> {
    let bytes = fixture();
    let mut image = Icf::parse(&bytes).unwrap();
    image
        .patch_memory(&[(0x3c2ff, &[255; 50]), (0x396ff, &60_i16.to_le_bytes())])
        .unwrap();
    for (physical, day, month, year, call) in [
        (0, 0x31, 0x12, 0x24, b"FIRST   "),
        (1, 1, 1, 0x25, b"SECOND  "),
    ] {
        let mut basic = [b' '; 48];
        basic[..8].copy_from_slice(call);
        basic[36..40].copy_from_slice(&433_500_000_u32.to_le_bytes());
        basic[40] = 9;
        basic[41..47].copy_from_slice(&[day, month, year, 0x23, 0x30, 0x45]);
        basic[47] = 0;
        let mut gps = [255; 92];
        gps[8..12].copy_from_slice(&i32::MAX.to_le_bytes());
        gps[26..29].copy_from_slice(&[0x23, 0x30, 0x45]);
        gps[29] = 0;
        gps[49..].fill(b' ');
        image
            .patch_memory(&[
                (0x3c2ff + physical, &[physical as u8]),
                (0x309c3 + physical, &[0x40]),
                (0x309c3 + 0x32 + 48 * physical, &basic),
                (0x309c3 + 0x992 + 20 * physical, &[b' '; 20]),
                (0x309c3 + 0xd7a + 92 * physical, &gps),
            ])
            .unwrap();
    }
    String::from_utf8(image.to_bytes())
        .unwrap()
        .replacen("#Comment=", "#Comment=TOKEN-METADATA ", 1)
        .into_bytes()
}
fn native_date_context(files: &Files) -> Value {
    let mut value = context(files);
    value["short_date"] = json!("ddd mmmm d ggg e mmm mmm");
    value["range"] = json!([0, 0]);
    value["native_date_tokens"] = json!([
        {"date":[2025,1,1],"letter":"d","count":3,"output":[87,34,101,100]},
        {"date":[2025,1,1],"letter":"m","count":4,"output":"4a616e75617279"},
        {"date":[2025,1,1],"letter":"g","count":3,"output":[69,82,65]},
        {"date":[2025,1,1],"letter":"e","count":1,"output":"37"},
        {"date":[2025,1,1],"letter":"m","count":3,"output":"4a31"},
        {"date":[2025,1,1],"letter":"m","count":3,"output":"4a32"}
    ]);
    value
}

#[test]
fn rx_native_date_capture_reaches_shifted_text_era_quotes_and_file_publication_boundaries() {
    let f = Files::new();
    let original = native_date_fixture();
    let input = f.write("native.icf", &original);
    let value = native_date_context(&f);
    let ctx = f.write("native.json", &serde_json::to_vec(&value).unwrap());
    let out = cli(
        &[
            "export",
            p(&input),
            "--sheet",
            "rx",
            "--csv-context",
            "-",
            "-o",
            "-",
        ],
        &serde_json::to_vec(&value).unwrap(),
    );
    exit(&out, 0);
    assert_eq!(report(&out)["rows"], 1);
    let line = out.stdout.split(|b| *b == b'\n').nth(1).unwrap();
    let cells = line.split(|b| *b == b',').collect::<Vec<_>>();
    assert_eq!(cells.len(), 37);
    assert_eq!(cells[10], b"W\"ed January 1 ERA 7 J1 J2 00:30:45");
    assert_eq!(cells[35], b"00:30:45");
    let keep = f.write("keep.csv", b"KEEP");
    let mut args = vec![
        "export",
        p(&input),
        "--sheet",
        "rx",
        "--csv-context",
        p(&ctx),
        "-o",
        p(&keep),
    ];
    exit(&cli(&args, b""), 4);
    args.push("--dry-run");
    exit(&cli(&args, b""), 0);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    args.pop();
    args.push("--overwrite");
    exit(&cli(&args, b""), 0);
    assert_eq!(fs::read(keep).unwrap(), out.stdout);
    assert_eq!(fs::read(input).unwrap(), original);
    assert!(
        Icf::parse(&original)
            .unwrap()
            .comment_bytes()
            .starts_with(b"TOKEN-METADATA ")
    );
}

#[test]
fn rx_native_date_missing_mismatched_and_late_captures_never_publish_even_in_dry_run() {
    let f = Files::new();
    let original = native_date_fixture();
    let input = f.write("native.icf", &original);
    let ctx = f.path("native.json");
    let keep = f.write("keep.csv", b"KEEP");
    let args = [
        "export",
        p(&input),
        "--sheet",
        "rx",
        "--csv-context",
        p(&ctx),
        "--overwrite",
        "-o",
        p(&keep),
    ];
    let base = native_date_context(&f);
    for case in 0..11 {
        let mut value = base.clone();
        match case {
            0 => {
                value.as_object_mut().unwrap().remove("native_date_tokens");
            }
            1 => value["native_date_tokens"] = json!([]),
            2 => value["native_date_tokens"][0]["date"] = json!([2024, 12, 31]),
            3 => value["native_date_tokens"][0]["letter"] = json!("m"),
            4 => value["native_date_tokens"][0]["count"] = json!(4),
            5 => value["native_date_tokens"][0]["output"] = json!("GG"),
            6 => value["native_date_tokens"][0]["output"] = json!([256]),
            7 => value["native_date_tokens"][0]["row"] = json!("not a token"),
            8 => {
                value.as_object_mut().unwrap().remove("range");
            }
            9 => {
                value["native_date_tokens"]
                    .as_array_mut()
                    .unwrap()
                    .swap(0, 1);
            }
            _ => {
                value["native_date_tokens"]
                    .as_array_mut()
                    .unwrap()
                    .truncate(5);
            }
        }
        fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
        let out = cli(&args, b"");
        exit(&out, 3);
        assert_eq!(report(&out)["rows"], usize::from(case == 8));
        assert!(report(&out)["prefix_bytes"].as_u64().unwrap() > 0);
        let expected = match case {
            0 => "source unavailable",
            1 | 8 | 10 => "capture exhausted",
            2..=4 | 9 => "request mismatch",
            5 => "output hex",
            6 => "output byte",
            _ => "date/letter/count/output only",
        };
        assert!(
            report(&out)["failure"].as_str().unwrap().contains(expected),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
        assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
        if case == 8 {
            let mut dry = args.to_vec();
            dry.push("--dry-run");
            exit(&cli(&dry, b""), 3);
        }
    }
    assert_eq!(fs::read(input).unwrap(), original);
}

#[test]
fn rx_native_date_empty_output_and_unused_sources_are_not_missing_captures() {
    let f = Files::new();
    let original = native_date_fixture();
    let input = f.write("native.icf", &original);
    let ctx = f.path("native.json");
    let args = [
        "export",
        p(&input),
        "--sheet",
        "rx",
        "--csv-context",
        p(&ctx),
        "-o",
        "-",
    ];
    let mut value = native_date_context(&f);
    value["native_date_tokens"][5]["output"] = json!([]);
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    let out = cli(&args, b"");
    exit(&out, 0);
    assert!(
        out.stdout
            .windows(b"W\"ed January 1 ERA 7 J1  00:30:45".len())
            .any(|w| w == b"W\"ed January 1 ERA 7 J1  00:30:45")
    );
    value["native_date_tokens"][5]["output"] = json!("");
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    let empty_hex = cli(&args, b"");
    exit(&empty_hex, 0);
    assert_eq!(empty_hex.stdout, out.stdout);
    value["native_date_tokens"][2]["output"] = json!([0x81, 0x41]);
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    let ansi = cli(&args, b"");
    exit(&ansi, 0);
    assert!(
        ansi.stdout
            .windows(b"January 1 \x81\x41 7".len())
            .any(|bytes| bytes == b"January 1 \x81\x41 7")
    );
    value["short_date"] = json!("yyyy/mm/dd");
    value["native_date_tokens"] = json!("unused malformed source");
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    exit(&cli(&args, b""), 0);
    value["short_date"] = json!("ddd mmmm ggg e");
    let mut image = Icf::parse(&original).unwrap();
    image
        .patch_memory(&[(0x309c3 + 0x32 + 41, &[255])])
        .unwrap();
    fs::write(&input, image.to_bytes()).unwrap();
    fs::write(&ctx, serde_json::to_vec(&value).unwrap()).unwrap();
    exit(&cli(&args, b""), 0);
    image.patch_memory(&[(0x3c2ff, &[255; 50])]).unwrap();
    fs::write(&input, image.to_bytes()).unwrap();
    let out = cli(&args, b"");
    exit(&out, 0);
    assert_eq!(report(&out)["rows"], 0);
}

#[test]
fn csv_r3_document_tail_cancel_header_and_approval_boundaries_use_real_sources() {
    let f = Files::new();
    let bytes = fixture();
    let mut image = Icf::parse(&bytes).unwrap();
    image
        .patch_memory(&[(0x37158, b"a"), (0x100, b"OPAQUE")])
        .unwrap();
    let original = image.to_bytes();
    let input = f.write("tail.icf", &original);
    let value = with_document(
        &f,
        json!({"profile":"portable-ascii","decimal":".","separator":",","range":[0,1]}),
    );
    let ctx = f.write("tail.json", &serde_json::to_vec(&value).unwrap());
    let csv = f.write(
        "tail.csv",
        b"No,Frequency Low,Frequency High\n1,145,146\n2,bad,146\n",
    );
    let keep = f.write("keep.icf", b"KEEP");
    let args = [
        "import",
        p(&input),
        "--csv",
        p(&csv),
        "--sheet",
        "user-edge",
        "--csv-context",
        p(&ctx),
        "--overwrite",
        "-o",
        p(&keep),
    ];
    exit(&cli(&args, b""), 6);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    let mut read = args.to_vec();
    read.push("--yes");
    let out = cli(&read, b"");
    exit(&out, 3);
    assert_eq!(report(&out)["stage9_completed"], true);
    assert_eq!(report(&out)["applied"], 1);
    assert_eq!(report(&out)["publication_authorized"], false);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    read.push("--accept-partial");
    exit(&cli(&read, b""), 5);
    let saved = fs::read(&keep).unwrap();
    let result = Icf::parse(&saved).unwrap();
    assert_eq!(result.memory()[0x37158], b'A');
    assert_eq!(&result.memory()[0x100..0x106], b"OPAQUE");
    fs::write(&keep, b"KEEP").unwrap();
    let mut cancel = args.to_vec();
    let csv_index = cancel.iter().position(|arg| *arg == p(&csv)).unwrap();
    cancel[csv_index] = "missing-cancelled-csv";
    cancel.extend(["--csv-gate", "inner-cancel"]);
    let out = cli(&cancel, b"");
    exit(&out, 0);
    assert_eq!(report(&out)["stage9_completed"], true);
    assert_eq!(report(&out)["reader_returned"], false);
    assert_eq!(report(&out)["changed"], true);
    assert_eq!(report(&out)["publication_authorized"], false);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    fs::write(&csv, b"Unknown\n1\n").unwrap();
    let out = cli(&read, b"");
    exit(&out, 3);
    assert_eq!(report(&out)["stage9_completed"], true);
    assert_eq!(report(&out)["reader_returned"], false);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    let mut missing = value;
    missing
        .as_object_mut()
        .unwrap()
        .remove("document_generation");
    fs::write(&ctx, serde_json::to_vec(&missing).unwrap()).unwrap();
    exit(&cli(&read, b""), 3);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    assert_eq!(fs::read(&input).unwrap(), original);
}

#[test]
fn csv_r3_stage9_failure_preserves_output_until_explicit_partial_publication() {
    let f = Files::new();
    let bytes = fixture();
    let mut image = Icf::parse(&bytes).unwrap();
    image
        .patch_memory(&[(0x37158, b"a"), (0x3a8ca, &[1]), (0x3c8b3, &[255])])
        .unwrap();
    let input = f.write("failure.icf", &image.to_bytes());
    let value = with_document(
        &f,
        json!({"profile":"portable-ascii","decimal":".","separator":",","range":[0,0]}),
    );
    let generation = value["document_generation"].as_str().unwrap();
    let mut raw = fs::read(generation).unwrap();
    raw[0x3a8ca] = 1;
    raw[0x3c8b3] = 255;
    fs::write(generation, raw).unwrap();
    let ctx = f.write("failure.json", &serde_json::to_vec(&value).unwrap());
    let csv = f.write(
        "failure.csv",
        b"No,Frequency Low,Frequency High\n1,145,146\n",
    );
    let keep = f.write("keep.icf", b"KEEP");
    let mut args = vec![
        "import",
        p(&input),
        "--csv",
        p(&csv),
        "--sheet",
        "user-edge",
        "--csv-context",
        p(&ctx),
        "--yes",
        "--overwrite",
        "-o",
        p(&keep),
    ];
    let out = cli(&args, b"");
    exit(&out, 3);
    assert_eq!(report(&out)["reader_returned"], true);
    assert_eq!(report(&out)["stage9_completed"], false);
    assert!(!report(&out)["failure"].is_null());
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    args.push("--accept-partial");
    exit(&cli(&args, b""), 5);
    let saved = fs::read(&keep).unwrap();
    assert_eq!(Icf::parse(&saved).unwrap().memory()[0x37158], b'A');
}

#[test]
fn csv_r3_unwritten_repeater_source_reads_actual_bytes_and_reports_short_windows() {
    use cs705::{icf::record, repeater::working::Working};
    let f = Files::new();
    let empty = f.write("empty.icf", &fixture());
    let old_bytes = repeater_fixture();
    let old = Icf::parse(&old_bytes).unwrap();
    let seed = f.write("seed.icf", &old_bytes);
    let q = cs705::icf::context::Context::from_bytes([0; 24]);
    let defaults = record::repeater_default_context(&q, None).unwrap();
    let working = Working::from_icf(&old, defaults).unwrap();
    let mut holder = record::reset_holder(12, 0, 0);
    working.load_into(0, &mut holder).unwrap();
    let mut memory = vec![0x5a; 4 * 616];
    memory[3 * 616..].copy_from_slice(holder.as_bytes());
    let source = f.write("heap.raw", &memory);
    let mut value = repeater_context(&f, &seed);
    value["repeater_unwritten"] =
        json!({"raw":4096,"first":3,"last":3,"base":4096,"read_budget":1,"memory":p(&source)});
    let ctx = f.write("heap.json", &serde_json::to_vec(&value).unwrap());
    let csv = f.write(
        "new.csv",
        b"Repeater Call Sign,Frequency,Mode,RPT1USE\nCALLA  B,145.125,DV,YES\n",
    );
    let output = f.path("restored.icf");
    let args = [
        "import",
        p(&empty),
        "--csv",
        p(&csv),
        "--sheet",
        "repeater",
        "--group",
        "0",
        "--mode",
        "preserve-use",
        "--csv-context",
        p(&ctx),
        "--yes",
        "-o",
        p(&output),
    ];
    let out = cli(&args, b"");
    exit(&out, 0);
    assert_eq!(report(&out)["stage9_completed"], true);
    let saved = fs::read(&output).unwrap();
    assert!(
        !Icf::parse(&saved)
            .unwrap()
            .physical_repeater(0)
            .unwrap()
            .unwrap()
            .use_repeater
    );
    fs::write(&source, &memory[..memory.len() - 1]).unwrap();
    fs::write(&output, b"KEEP").unwrap();
    let mut short = args.to_vec();
    short.push("--overwrite");
    let out = cli(&short, b"");
    exit(&out, 3);
    assert_eq!(report(&out)["applied"], 1);
    assert!(
        report(&out)["failure"]
            .as_str()
            .unwrap()
            .contains("byte window unavailable")
    );
    assert_eq!(fs::read(&output).unwrap(), b"KEEP");
    short.push("--accept-partial");
    exit(&cli(&short, b""), 5);
}
