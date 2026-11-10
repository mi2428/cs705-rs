//! Real O046 consumers: source resolution, API observations and safe reports.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/private/cli-r3/fixtures");
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "item-help-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().to_string()
    }
    fn write(&self, name: &str, bytes: &[u8]) -> String {
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

fn cli(args: &[String], input: &[u8]) -> Output {
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
fn put32(pe: &mut [u8], at: usize, n: u32) {
    pe[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn asset() -> Vec<u8> {
    let mut pe = vec![0; 0x9400];
    pe[..2].copy_from_slice(b"MZ");
    put32(&mut pe, 0x3c, 0x80);
    pe[0x80..0x84].copy_from_slice(b"PE\0\0");
    pe[0x84..0x86].copy_from_slice(&0x14c_u16.to_le_bytes());
    pe[0x86..0x88].copy_from_slice(&4_u16.to_le_bytes());
    pe[0x94..0x96].copy_from_slice(&0xe0_u16.to_le_bytes());
    pe[0x98..0x9a].copy_from_slice(&0x10b_u16.to_le_bytes());
    put32(&mut pe, 0xb4, 0x400000);
    for (i, (rva, size, raw)) in [
        (0x314000, 0x1000, 0x400),
        (0x3db000, 0x5000, 0x1400),
        (0x3f2000, 0x2000, 0x6400),
        (0x410000, 0x1000, 0x8400),
    ]
    .into_iter()
    .enumerate()
    {
        let at = 0x178 + 40 * i;
        put32(&mut pe, at + 8, size);
        put32(&mut pe, at + 12, rva);
        put32(&mut pe, at + 16, size);
        put32(&mut pe, at + 20, raw);
    }
    for key in [0x136, 0x215] {
        put32(&mut pe, 0x6f44 + 4 * key, 0x810000);
    }
    pe[0x8400..0x840d].copy_from_slice(b"CL_Test_Case\0");
    let files = b"GI_Sys_MyHelpFile\0CS705_ENG.chm\0CS705_JPN.chm\0\0#\0";
    pe[0x476..0x476 + files.len()].copy_from_slice(files);
    let rows =
        b"GI_AppItem_CellName_to_ContextID\0CL_Test_Case\t12345.htm\0CL_Test_Case\t99999.htm\0#\0";
    pe[0x1d3f..0x1d3f + rows.len()].copy_from_slice(rows);
    pe
}
fn state() -> Value {
    json!({"module":0,"procedure":0,"cookie":0,"d188":77,"d184":88})
}
fn capture(status: u32) -> Value {
    let api = match status {
        3 => json!({"load_library":0}),
        0 => {
            json!({"load_library":11,"get_proc_address":22,"initialize_cookie":33,"window_before":0,"display_window":44,"window_after":0})
        }
        1 => {
            json!({"load_library":11,"get_proc_address":22,"initialize_cookie":33,"window_before":0,"display_window":0,"file_attributes":u32::MAX})
        }
        _ => {
            json!({"load_library":11,"get_proc_address":22,"initialize_cookie":33,"window_before":0,"display_window":0,"file_attributes":32})
        }
    };
    json!({"state":state(),"api":api})
}
fn args(asset: &str, capture: &str, output: &str, tail: &[&str]) -> Vec<String> {
    let mut a = vec![
        "item-help",
        "--asset",
        asset,
        "--capture",
        capture,
        "--gui-language",
        "0",
        "--application-dir-hex",
        "433a5c82a05c",
        "-o",
        output,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    a.extend(tail.iter().map(|s| s.to_string()));
    a
}
fn topic() -> [&'static str; 7] {
    ["topic", "--key", "310", "--index", "0", "--group", ""]
}
fn reports(output: &Output) -> Vec<Value> {
    serde_json::Deserializer::from_slice(&output.stderr)
        .into_iter()
        .map(|v| v.unwrap())
        .collect()
}
fn ok(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn source_key_catalog_and_contents_are_real_requests_not_supplied_topics() {
    let f = Files::new();
    let pe = f.write("source.pe", &asset());
    let cap = f.write("api.json", &serde_json::to_vec(&capture(0)).unwrap());
    for tail in [
        topic().as_slice(),
        &[
            "topic",
            "--field",
            "A2-H54848-31",
            "--index",
            "0",
            "--group",
            "",
        ],
    ] {
        let v = ok(&cli(&args(&pe, &cap, "-", tail), &[]));
        assert_eq!(v["topic"], 12345);
        assert_eq!(v["name"]["hex"], "434C5F546573745F43617365");
        assert_eq!(v["status"], 0);
        assert_eq!(v["state_after"]["d188"], 44);
        assert_eq!(v["chm"]["hex"], "433A5C82A05C43533730355F454E472E63686D");
        assert_eq!(v["help_window_displayed_by_cli"], false);
    }
    let mut a = args(&pe, &cap, "-", &topic());
    a[6] = "1".to_string();
    assert!(
        ok(&cli(&a, &[]))["chm"]["hex"]
            .as_str()
            .unwrap()
            .ends_with("43533730355F4A504E2E63686D")
    );
    let a = args(&pe, &cap, "-", &["contents"]);
    let v = ok(&cli(&a, &[]));
    assert_eq!(v["topic"], -1);
    assert_eq!(v["state_after"]["d184"], 44);
    assert_eq!(v["state_after"]["d188"], 77);
}

#[test]
fn captured_statuses_d188_and_messages_are_not_native_delivery() {
    let f = Files::new();
    let pe = f.write("source.pe", &asset());
    for status in 0..=3 {
        let cap = f.write("api.json", &serde_json::to_vec(&capture(status)).unwrap());
        let v = ok(&cli(&args(&pe, &cap, "-", &topic()), &[]));
        assert_eq!(v["status"], status);
        assert_eq!(
            v["state_after"]["d188"],
            match status {
                0 => 44,
                3 => 77,
                _ => 0,
            }
        );
        assert_eq!(v["effects"]["os_calls_performed"], false);
        assert_eq!(
            v["effects"]["message_icon"],
            match status {
                1 => json!(16),
                2 => json!(48),
                _ => Value::Null,
            }
        );
    }
}

#[test]
fn missing_api_and_null_info_preserve_real_prefix_and_do_not_publish() {
    let f = Files::new();
    let pe = f.write("source.pe", &asset());
    let output = f.write("keep.json", b"KEEP");
    for api in [
        json!({"load_library":11}),
        json!({"load_library":11,"get_proc_address":22,"initialize_cookie":33,"window_before":0,"display_window":44,"window_after":44,"window_info_pointer":0}),
    ] {
        let cap = f.write(
            "api.json",
            &serde_json::to_vec(&json!({"state":state(),"api":api})).unwrap(),
        );
        let mut a = args(&pe, &cap, &output, &topic());
        a.insert(11, "--overwrite".to_string());
        let out = cli(&a, &[]);
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
        assert_eq!(fs::read(&output).unwrap(), b"KEEP");
        let reports = reports(&out);
        let prefix = reports
            .iter()
            .find_map(|r| r.get("item_help_prefix"))
            .unwrap();
        assert_eq!(prefix["state_after"]["module"], 11);
        assert_eq!(prefix["state_after"]["d188"], 77);
        assert!(prefix["status"].is_null());
    }
}

#[test]
fn source_failures_group_errors_and_completed_packet_members_fail_closed() {
    let f = Files::new();
    let mut bytes = asset();
    bytes[0x1d3f] = b'!';
    let pe = f.write("source.pe", &bytes);
    let cap = f.write("api.json", &serde_json::to_vec(&capture(0)).unwrap());
    let out = cli(&args(&pe, &cap, "-", &topic()), &[]);
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    fs::write(&pe, asset()).unwrap();
    for tail in [
        vec!["topic"],
        vec!["topic", "--key", "310", "--group", ""],
        vec!["topic", "--key", "310", "--index", "0"],
        vec!["topic", "--field", "unknown", "--index", "0", "--group", ""],
        vec![
            "topic",
            "--key",
            "310",
            "--field",
            "A2-H54848-31",
            "--index",
            "0",
            "--group",
            "",
        ],
    ] {
        let out = cli(&args(&pe, &cap, "-", &tail), &[]);
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
    }
    for (asset, capture) in [
        (f.path("missing.pe"), cap.clone()),
        (pe.clone(), f.path("missing.json")),
    ] {
        let out = cli(&args(&asset, &capture, "-", &topic()), &[]);
        assert_eq!(out.status.code(), Some(4));
        assert!(out.stdout.is_empty());
    }
    let out = cli(
        &args(
            &pe,
            &cap,
            "-",
            &[
                "topic", "--key", "310", "--index", "0", "--group", "bad,401",
            ],
        ),
        &[],
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    let out = cli(
        &args(
            &pe,
            &cap,
            "-",
            &["topic", "--key", "-1", "--index", "0", "--group", ""],
        ),
        &[],
    );
    assert_eq!(ok(&out)["topic"], 0);
    let mut packet = capture(0);
    packet["topic"] = json!(999);
    fs::write(&cap, serde_json::to_vec(&packet).unwrap()).unwrap();
    assert_eq!(
        cli(&args(&pe, &cap, "-", &topic()), &[]).status.code(),
        Some(3)
    );
}

#[test]
fn stdin_all_roles_are_owned_before_any_read_and_image_remains_readonly() {
    let f = Files::new();
    let source = asset();
    let api = serde_json::to_vec(&capture(0)).unwrap();
    let node = serde_json::to_vec(&json!({"key":310,"index":0,"context58_hex":""})).unwrap();
    let mut raw = vec![0xd3; 0x3c9c0];
    raw[0x355a1] = 6;
    raw[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    let image = cs705::icf::format::write_storage(
        &raw,
        cs705::icf::format::Layout::from_map_revision(6),
        b"HELP QA",
        0x40001,
    )
    .unwrap();
    let pe = f.write("source.pe", &source);
    let cap = f.write("api.json", &api);
    let context = f.write("node.json", &node);
    let ip = f.write("image.icf", &image);
    for path in [&pe, &cap, &context, &ip] {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions).unwrap();
    }
    for mask in 0_u32..16 {
        let paths = [
            if mask & 1 != 0 { "-" } else { &pe },
            if mask & 2 != 0 { "-" } else { &cap },
            if mask & 4 != 0 { "-" } else { &context },
            if mask & 8 != 0 { "-" } else { &ip },
        ];
        let mut a = args(paths[0], paths[1], "-", &["topic", "--context", paths[2]]);
        a.splice(11..11, ["--image".to_string(), paths[3].to_string()]);
        let input = match mask {
            1 => source.as_slice(),
            2 => api.as_slice(),
            4 => node.as_slice(),
            8 => image.as_slice(),
            _ => &[],
        };
        let out = cli(&a, input);
        if mask.count_ones() > 1 {
            assert_eq!(out.status.code(), Some(3));
            assert!(out.stdout.is_empty());
            assert!(reports(&out).iter().any(|r| {
                r["error"]
                    .as_str()
                    .is_some_and(|s| s.contains("stdin may be consumed by only one input"))
            }));
        } else {
            assert_eq!(ok(&out)["image_modified"], false);
        }
    }
    for (path, expected) in [
        (&pe, source.as_slice()),
        (&cap, api.as_slice()),
        (&context, node.as_slice()),
        (&ip, image.as_slice()),
    ] {
        assert_eq!(fs::read(path).unwrap(), expected);
    }
}

#[test]
fn no_clobber_dryrun_and_overwrite_use_shared_io() {
    let f = Files::new();
    let pe = f.write("source.pe", &asset());
    let cap = f.write("api.json", &serde_json::to_vec(&capture(0)).unwrap());
    let output = f.write("keep.json", b"KEEP");
    assert_eq!(
        cli(&args(&pe, &cap, &output, &topic()), &[]).status.code(),
        Some(4)
    );
    assert_eq!(fs::read(&output).unwrap(), b"KEEP");
    let mut a = args(&pe, &cap, &output, &topic());
    a.insert(11, "--dry-run".to_string());
    assert!(cli(&a, &[]).status.success());
    assert_eq!(fs::read(&output).unwrap(), b"KEEP");
    let mut a = args(&pe, &cap, &output, &topic());
    a.insert(11, "--overwrite".to_string());
    assert!(cli(&a, &[]).status.success());
    let report: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(report["topic"], 12345);
}

#[test]
#[ignore = "requires CS705_ITEM_HELP_ASSET pointing to the approved original PE"]
fn fixed_asset_small_literal_topic_uses_the_real_cli() {
    let f = Files::new();
    let cap = f.write("api.json", &serde_json::to_vec(&capture(3)).unwrap());
    let pe = std::env::var("CS705_ITEM_HELP_ASSET").expect("explicit original asset source");
    let v = ok(&cli(&args(&pe, &cap, "-", &topic()), &[]));
    assert_eq!(v["topic"], 55782);
    assert_eq!(v["status"], 3);
    assert_eq!(v["state_after"]["d188"], 77);
}

#[test]
fn independent_english_help_uses_the_semantic_catalog_without_pe_or_capture() {
    let contents = ok(&cli(&["item-help", "-o", "-"].map(str::to_owned), b""));
    assert_eq!(contents["language"], "English");
    assert!(
        contents["fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["key"] == "network/prefix")
    );
    let topic = ok(&cli(
        &["item-help", "-o", "-", "topic", "--field", "network/prefix"].map(str::to_owned),
        b"",
    ));
    assert_eq!(topic["fields"][0]["label"], "network prefix");
    assert_eq!(topic["image_modified"], false);
    assert_eq!(topic["application_assets_used"], false);
    let f = Files::new();
    let output = f.write("keep.json", b"KEEP");
    let out = cli(
        &[
            "item-help",
            "-o",
            &output,
            "--overwrite",
            "topic",
            "--field",
            "unknown-field",
        ]
        .map(str::to_owned),
        b"",
    );
    assert!(!out.status.success() && out.stdout.is_empty());
    assert_eq!(fs::read(output).unwrap(), b"KEEP");
}
