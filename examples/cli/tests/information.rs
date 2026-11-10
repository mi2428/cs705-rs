//! Pure-file information/COM controller checks. Never enumerate or open a device.
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
        let p = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "information-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn write(&self, name: &str, data: &[u8]) -> PathBuf {
        let p = self.path(name);
        fs::write(&p, data).unwrap();
        p
    }
    fn json(&self, name: &str, data: &Value) -> PathBuf {
        self.write(name, &serde_json::to_vec(data).unwrap())
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
fn exit(o: &Output, n: i32) {
    assert_eq!(
        o.status.code(),
        Some(n),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
}
fn value(o: &Output) -> Value {
    serde_json::from_slice(&o.stdout).unwrap()
}
fn report(o: &Output) -> Value {
    serde_json::Deserializer::from_slice(&o.stderr)
        .into_iter::<Value>()
        .next()
        .unwrap()
        .unwrap()
}
fn b(text: &[u8]) -> Value {
    json!(text)
}
fn initial(com: i32) -> Value {
    json!({"com":com,"text":b(b"INITIAL"),"document":[6,0,1]})
}
fn registry() -> Value {
    json!({"opened":false})
}
fn construct(reads: &[i32]) -> Value {
    json!({"action":"construct-com","registry":registry(),"com_reads":reads})
}
fn normal_construct() -> Value {
    json!({"action":"construct-com","registry":registry()})
}
fn search(numbers: &[u32]) -> Value {
    json!({"ready":1,"opened":true,"prefix":b(b"RAD"),"suffixes":[b(b"A"),b(b"#")],
        "filters":[{"hub":b(b"*"),"audio":b(b"*"),"com":b(b"*"),"id":b(b"A"),"cid":b(b"A")}],
        "observations":numbers.iter().enumerate().map(|(i,number)|json!({"key":[b'a'+i as u8],"base":b(b"RAD"),"com":b(b"USB"),"hub":[],"audio":[],"parent_path":false,"wave_in":-1,"wave_out":3,
            "ports":[{"instance_id":b(b"DEVICE"),"parent_id":b(b"PARENT"),"prefix":b(b"USB"),"mi":0,"number":number,"description":b(b"RAD_A")}]})).collect::<Vec<_>>()})
}
fn execute(f: &Files, script: &Value) -> Output {
    let path = f.json("script.json", script);
    cli(&["information", "--script", p(&path), "-o", "-"], b"")
}
fn names(v: &Value) -> Vec<&str> {
    v["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["request"].as_str().unwrap())
        .collect()
}
fn bytes(v: &Value) -> Vec<u8> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect()
}

#[test]
fn manual_literal_boundaries_keyboard_four_bytes_and_full_getter_wrap_reach_real_cli() {
    let f = Files::new();
    for (text, ok) in [
        (b"0".as_slice(), false),
        (b"1", true),
        (b"256", true),
        (b"257", false),
    ] {
        let script = json!({"initial":initial(7),"events":[construct(&[7,7]),{"action":"manual-commit","text":b(text),"busy_reads":[false,false]}]});
        let out = execute(&f, &script);
        exit(&out, if ok { 0 } else { 3 });
        let v = if ok {
            value(&out)
        } else {
            assert!(out.stdout.is_empty());
            report(&out)["prefix"].clone()
        };
        assert_eq!(
            v["state"]["com"],
            if ok {
                String::from_utf8_lossy(text).parse::<i32>().unwrap()
            } else {
                7
            }
        );
        assert_eq!(v["state"]["reidentify_requested"], ok);
        if ok {
            assert_eq!(v["state"]["fc"], 1);
            assert_eq!(v["state"]["fd"], 0);
        }
    }
    let out = execute(
        &f,
        &json!({"initial":initial(7),"events":[construct(&[7,7]),{"action":"input","text":b(b"256.")},{"action":"manual-commit","text":b(b"256."),"busy_reads":[true]}]}),
    );
    exit(&out, 0);
    assert_eq!(value(&out)["state"]["com"], 256);
    assert_eq!(value(&out)["state"]["selector"], 6);
    let out = execute(
        &f,
        &json!({"initial":initial(7),"events":[construct(&[7,7]),{"action":"input","text":b(b"4294967297")}]}),
    );
    exit(&out, 3);
    assert_eq!(report(&out)["prefix"]["state"]["com"], 7);
    let out = execute(
        &f,
        &json!({"initial":initial(7),"events":[construct(&[7,7]),{"action":"manual-commit","text":b(b"4294967297"),"busy_reads":[true]}]}),
    );
    exit(&out, 0);
    assert_eq!(value(&out)["state"]["com"], 1);
    assert_eq!(bytes(&value(&out)["state"]["text"]), b"4294967297");
}

#[test]
fn startup300_raw_search_observations_and_constructor_guard_format_reads_are_distinct() {
    let f = Files::new();
    let out = execute(
        &f,
        &json!({"initial":initial(0),"events":[{"action":"startup","source":search(&[300,12])},construct(&[300])]}),
    );
    exit(&out, 0);
    let v = value(&out);
    assert_eq!(v["state"]["com"], 300);
    assert_eq!(v["state"]["generation"], 1);
    assert_eq!(bytes(&v["state"]["text"]), b"INITIAL");
    assert_eq!(
        bytes(&v["state"]["candidates"][0]["label"]),
        b"RAD A  COM=300"
    );
    assert!(!names(&v).contains(&"read_com_format"));
    assert_eq!(v["state"]["com_mode"], 0);
    let out = execute(
        &f,
        &json!({"initial":initial(1),"events":[construct(&[1,300])]}),
    );
    exit(&out, 0);
    assert_eq!(bytes(&value(&out)["state"]["text"]), b"300");
    assert_eq!(value(&out)["state"]["com"], 300);
    let out = execute(
        &f,
        &json!({"initial":initial(300),"events":[{"action":"startup"}]}),
    );
    exit(&out, 0);
    assert!(names(&value(&out)).is_empty());
}

#[test]
fn rescan_two_live_indices_interactive_text_and_new_form_preserve_old_release_lifetime() {
    let f = Files::new();
    let out = execute(
        &f,
        &json!({"initial":initial(7),"events":[construct(&[7,7]),
        {"action":"open-search","source":search(&[300,7])}, {"action":"rescan","source":search(&[12,300])},
        {"action":"select","index_reads":[0,1]}, {"action":"return-search","registry":registry()},
        {"action":"open-search","source":search(&[17])}, {"action":"select","index_reads":[-1]},
        {"action":"return-search","registry":registry()}]}),
    );
    exit(&out, 0);
    let v = value(&out);
    assert_eq!(v["state"]["com"], 7);
    assert_eq!(bytes(&v["state"]["text"]), b"300");
    assert_eq!(v["state"]["generation"], 3);
    assert_eq!(v["state"]["retired_popups"], json!(["release_requested"]));
    assert_eq!(v["state"]["popup"], "release_requested");
    assert_eq!(v["state"]["candidates"][0]["number"], 17);
    assert!(!names(&v).contains(&"destroy_delivered"));
    assert!(!names(&v).contains(&"reidentify_request"));
    let snapshot = f.write("snapshot.json", &out.stdout);
    let script = f.json(
        "next.json",
        &json!({"events":[{"action":"destroy-retired-search","index":0}]}),
    );
    let out = cli(
        &[
            "information",
            "--script",
            p(&script),
            "--state",
            p(&snapshot),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&out, 0);
    let v = value(&out);
    assert_eq!(v["state"]["retired_popups"], json!(["destroyed"]));
    assert_eq!(v["state"]["popup"], "release_requested");
}

#[test]
fn cancel_close_false_returns_and_late_registry_dispatch_failure_never_publish_prefix() {
    let f = Files::new();
    let out = execute(
        &f,
        &json!({"initial":initial(7),"events":[construct(&[7,7]),{"action":"manual-commit","text":b(b"1"),"busy_reads":[true]}, {"action":"cancel","returns":[0,0,0,0,0]}]}),
    );
    exit(&out, 0);
    let v = value(&out);
    assert_eq!(v["state"]["com"], 1);
    assert_eq!(v["state"]["form"], "release_requested");
    assert_eq!(v["state"]["pair_registered"], false);
    assert_eq!(
        v["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["request"] == "sync")
            .map(|e| e["argument"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        [0x28, 0x29, 2]
    );
    assert!(!names(&v).contains(&"destroy_delivered"));
    let keep = f.write("keep.json", b"KEEP");
    for event in [
        json!({"action":"cancel","returns":[0,0,0,"exception"]}),
        json!({"action":"dropdown","registry":{"profile":"ascii-c","crt_point":".","opened":true,"values":[
            {"name":[65],"opened":true,"exists":true,"text":b(b"COM300")}, {"name":[66],"opened":true,"exists":true,"text":b(b"COM2")}, {"exception":true}]}}),
    ] {
        let script = f.json(
            "failure.json",
            &json!({"initial":initial(7),"events":[construct(&[7,7]),event]}),
        );
        let mut args = vec![
            "information",
            "--script",
            p(&script),
            "--overwrite",
            "-o",
            p(&keep),
        ];
        let out = cli(&args, b"");
        exit(&out, 3);
        assert!(out.stdout.is_empty());
        assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
        let v = report(&out)["prefix"].clone();
        assert_eq!(v["state"]["pair_registered"], true);
        assert!(!names(&v).contains(&"com_release_request"));
        if event["action"] == "dropdown" {
            assert_eq!(v["state"]["dropdown"], json!([300, 2]));
            assert_eq!(v["dropdown_text"], json!(["300", "2"]));
            assert!(v["state"]["text"].is_null());
        }
        args.push("--dry-run");
        exit(&cli(&args, b""), 3);
        assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    }
}

fn wire(comment: &[u8], selector: u8) -> Value {
    let mut frame = vec![0xfe, 0xfe, 0xef, 0xee, 0xe1, 0x40, 0x75, 0, 1];
    let mut tail = vec![0; 33];
    tail[0] = 33;
    tail[1] = selector;
    tail[2..18].fill(b' ');
    tail[2..2 + comment.len()].copy_from_slice(comment);
    frame.extend(tail);
    frame.push(0xfd);
    json!(frame)
}
fn identify(comment: &[u8], selector: u8) -> Value {
    json!({"action":"identify","wire":wire(comment,selector),"gate_q":vec![0;24],"gate_flag":1,"display_q":vec![0;24],"display_flag":1,"operation":1})
}

#[test]
fn identity_frame_live_tail_generation_two_by_two_and_release_destroy_are_real_consumers() {
    let f = Files::new();
    let out = execute(
        &f,
        &json!({"initial":{"com":0,"text":[],"document":[6,0,0]},"events":[identify(b"FIRST GENERATION",6),identify(b"SECOND",0)]}),
    );
    exit(&out, 0);
    let v = value(&out);
    assert_eq!(v["state"]["identity_result"], 13);
    assert_eq!(v["information_rows"].as_array().unwrap().len(), 2);
    assert_eq!(v["information_rows"][1][1]["type"], 25);
    assert_eq!(v["information_rows"][1][1]["key"], 176);
    assert_eq!(v["information_rows"][0][1]["span"], true);
    assert_eq!(
        bytes(&v["information_rows"][1][1]["text"]),
        b"SECOND          "
    );
    assert_eq!(v["information_rows"][1][1]["selectable"], false);
    assert_eq!(
        names(&v)
            .iter()
            .filter(|&&name| name == "create_information_2_by_2")
            .count(),
        1
    );
    assert_eq!(
        names(&v)
            .iter()
            .filter(|&&name| name == "compare_information_context")
            .count(),
        2
    );
    let out = execute(
        &f,
        &json!({"initial":{"com":0,"text":[],"document":[6,1,0]},"events":[identify(b"AB\0TRAIL",6),{"action":"information-click"}]}),
    );
    exit(&out, 0);
    let v = value(&out);
    assert_eq!(bytes(&v["information_rows"][1][1]["text"]), b"AB");
    assert_eq!(v["state"]["information"], "release_requested");
    assert!(!names(&v).contains(&"destroy_delivered"));
    let snapshot = f.write("info-state.json", &out.stdout);
    let script = f.json(
        "destroy.json",
        &json!({"events":[{"action":"destroy","target":"information"}]}),
    );
    let out = cli(
        &[
            "information",
            "--script",
            p(&script),
            "--state",
            p(&snapshot),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(value(&out)["state"]["information"], "destroyed");
    assert!(value(&out)["information_rows"].is_null());
}

fn image() -> Vec<u8> {
    let mut m = vec![0; 0x3c9c0];
    m[0x355a1] = 6;
    m[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    String::from_utf8(cs705::icf::Icf::from_memory(m).unwrap().to_bytes())
        .unwrap()
        .replacen("#Comment=", "#Comment=READ-ONLY-METADATA ", 1)
        .into_bytes()
}
#[test]
fn snapshots_all_stdin_ownership_no_clobber_dryrun_readonly_image_and_enumeration_denial() {
    let f = Files::new();
    let original = image();
    let image = f.write("read-only.icf", &original);
    let script = json!({"initial":initial(7),"events":[construct(&[7,7])]});
    let path = f.json("script.json", &script);
    let out = cli(
        &[
            "information",
            "--script",
            "-",
            "--image",
            p(&image),
            "-o",
            "-",
        ],
        &serde_json::to_vec(&script).unwrap(),
    );
    exit(&out, 0);
    let saved = value(&out);
    assert_eq!(saved["native_execution"], false);
    assert_eq!(saved["read_only_image"]["bytes"], 0x3c9c0);
    assert_eq!(
        bytes(&saved["read_only_image"]["comment"]),
        b"READ-ONLY-METADATA "
    );
    let snapshot = f.write("snapshot.json", &out.stdout);
    let next = f.json("next.json", &json!({"events":[]}));
    let out = cli(
        &[
            "information",
            "--script",
            p(&next),
            "--state",
            "-",
            "-o",
            "-",
        ],
        &fs::read(&snapshot).unwrap(),
    );
    exit(&out, 0);
    assert_eq!(value(&out)["state"], saved["state"]);
    assert!(value(&out)["events"].as_array().unwrap().is_empty());
    let out = cli(
        &[
            "information",
            "--script",
            p(&path),
            "--image",
            "-",
            "-o",
            "-",
        ],
        &original,
    );
    exit(&out, 0);
    assert_eq!(value(&out)["state"], saved["state"]);
    for args in [
        vec!["information", "--script", "-", "--state", "-", "-o", "-"],
        vec!["information", "--script", "-", "--image", "-", "-o", "-"],
    ] {
        exit(&cli(&args, b"NOT READ"), 3);
    }
    let keep = f.write("keep.json", b"KEEP");
    let mut args = vec!["information", "--script", p(&path), "-o", p(&keep)];
    exit(&cli(&args, b""), 4);
    args.push("--dry-run");
    exit(&cli(&args, b""), 0);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    args.pop();
    args.push("--overwrite");
    exit(&cli(&args, b""), 0);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(keep).unwrap()).unwrap()["state"],
        saved["state"]
    );
    assert_eq!(fs::read(image).unwrap(), original);
    exit(
        &cli(
            &["information", "--script", p(&path), "--list", "-o", "-"],
            b"",
        ),
        2,
    );
    let out = execute(
        &f,
        &json!({"initial":initial(7),"events":[{"action":"enumerate"}]}),
    );
    exit(&out, 3);
    assert!(out.stdout.is_empty());
}

#[test]
fn captured_restore_and_unavailable_native_profiles_never_fall_back_to_constructor_or_zero() {
    let f = Files::new();
    let out = execute(
        &f,
        &json!({"initial":initial(7),"events":[construct(&[7,7])]}),
    );
    exit(&out, 0);
    let mut captured = value(&out);
    captured["state"]["selected"] = json!(99);
    let snapshot = f.json("bad-state.json", &captured);
    let script = f.json("resume.json", &json!({"events":[]}));
    let keep = f.write("keep.json", b"KEEP");
    let out = cli(
        &[
            "information",
            "--script",
            p(&script),
            "--state",
            p(&snapshot),
            "--overwrite",
            "-o",
            p(&keep),
        ],
        b"",
    );
    exit(&out, 3);
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    assert!(out.stdout.is_empty());
    let mut native = registry();
    native["profile"] = json!("unprovided-native-ctype");
    native["opened"] = json!(true);
    native["values"] = json!([{"name":[65],"opened":true,"exists":true,"text":b(b"COM1")}]);
    let out = execute(
        &f,
        &json!({"initial":initial(7),"events":[construct(&[7,7]),{"action":"dropdown","registry":native}]}),
    );
    exit(&out, 3);
    assert_eq!(report(&out)["prefix"]["state"]["com"], 7);
    let out = execute(
        &f,
        &json!({"initial":initial(0),"events":[{"action":"startup","source":{"ready":1,"opened":true,"candidates":[300]}}]}),
    );
    exit(&out, 3);
    assert!(out.stdout.is_empty());
    assert_eq!(report(&out)["prefix"]["state"]["com"], 0);
}

#[test]
fn producer_current_cli_uses_owned_com_selection_and_keyboard_text() {
    let f = Files::new();
    let out = execute(
        &f,
        &json!({"initial":initial(0),"events":[{"action":"startup","source":search(&[300])},normal_construct()]}),
    );
    exit(&out, 0);
    let v = value(&out);
    assert_eq!(v["state"]["com"], 300);
    assert_eq!(bytes(&v["state"]["text"]), b"INITIAL");
    assert!(!names(&v).contains(&"read_com_format"));
    let out = execute(
        &f,
        &json!({"initial":initial(1),"events":[normal_construct(),{"action":"input","text":b(b"256.")},{"action":"manual-commit","busy_reads":[true]}]}),
    );
    exit(&out, 0);
    let v = value(&out);
    assert_eq!(v["state"]["com"], 256);
    assert_eq!(
        v["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["request"] == "read_com_guard" || e["request"] == "read_com_format")
            .map(|e| e["argument"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        [1, 1]
    );
    let out = execute(
        &f,
        &json!({"initial":initial(7),"events":[normal_construct(),{"action":"open-search","source":search(&[300,12])},{"action":"select","index":1},{"action":"return-search","registry":registry()}]}),
    );
    exit(&out, 0);
    let v = value(&out);
    assert_eq!(v["state"]["com"], 7);
    assert_eq!(v["state"]["search_result"], 12);
    assert_eq!(bytes(&v["state"]["text"]), b"12");
    assert_eq!(
        v["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["request"] == "read_index_guard" || e["request"] == "read_index_label")
            .map(|e| e["argument"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        [1, 1]
    );
    let out = execute(
        &f,
        &json!({"initial":initial(1),"events":[normal_construct(),{"action":"input","text":b(b"1")},{"action":"manual-commit"}]}),
    );
    exit(&out, 3);
    let v = report(&out)["prefix"].clone();
    assert_eq!(v["state"]["com"], 1);
    assert_eq!(v["state"]["reidentify_requested"], false);
    assert!(
        report(&out)["failure"]
            .as_str()
            .unwrap()
            .contains("busy read unavailable")
    );
}

#[test]
fn producer_captured_reads_missing_invalid_and_empty_never_switch_to_current() {
    let f = Files::new();
    for reads in [
        json!([]),
        json!([1]),
        json!([1, "invalid"]),
        json!("invalid"),
    ] {
        let out = execute(
            &f,
            &json!({"initial":initial(1),"events":[{"action":"construct-com","registry":registry(),"com_reads":reads}]}),
        );
        exit(&out, 3);
        let v = report(&out)["prefix"].clone();
        assert_eq!(v["state"]["com"], 1);
        assert!(v["state"]["com_mode"].is_null());
        assert_eq!(v["state"]["pair_registered"], false);
    }
    let out = execute(
        &f,
        &json!({"initial":initial(1),"events":[{"action":"construct-com","registry":registry(),"com_reads":[1,300]}]}),
    );
    exit(&out, 0);
    assert_eq!(value(&out)["state"]["com"], 300);
    assert_eq!(bytes(&value(&out)["state"]["text"]), b"300");
    let out = execute(
        &f,
        &json!({"initial":initial(300),"events":[{"action":"construct-com","registry":registry(),"com_reads":[300,"unread"]}]}),
    );
    exit(&out, 0);
    assert_eq!(bytes(&value(&out)["state"]["text"]), b"INITIAL");
    for reads in [
        json!([]),
        json!([0]),
        json!([0, "invalid"]),
        json!("invalid"),
    ] {
        let out = execute(
            &f,
            &json!({"initial":initial(7),"events":[normal_construct(),{"action":"open-search","source":search(&[300,12])},{"action":"select","index_reads":reads}]}),
        );
        exit(&out, 3);
        let v = report(&out)["prefix"].clone();
        assert_eq!(v["state"]["search_result"], -1);
        assert_eq!(v["state"]["popup"], "open");
        assert!(!names(&v).contains(&"popup_release_request"));
    }
    let out = execute(
        &f,
        &json!({"initial":initial(1),"events":[normal_construct(),{"action":"manual-commit","text":[],"busy_reads":[true]}]}),
    );
    exit(&out, 3);
    assert_eq!(report(&out)["prefix"]["state"]["com"], 1);
    assert_eq!(report(&out)["prefix"]["state"]["text"], json!([]));
}

#[test]
fn missing_search_observation_and_invalid_live_index_fail_closed_after_clear_without_old_candidates()
 {
    let f = Files::new();
    let mut bad = search(&[12]);
    bad["observations"][0]["com"] = json!([0x80]);
    for source in [json!({"ready":1}), bad] {
        let out = execute(
            &f,
            &json!({"initial":initial(7),"events":[{"action":"open-search","source":search(&[300])},{"action":"rescan","source":source}]}),
        );
        exit(&out, 3);
        let v = report(&out)["prefix"].clone();
        assert!(v["state"]["candidates"].as_array().unwrap().is_empty());
        assert_eq!(v["state"]["generation"], 2);
        assert!(out.stdout.is_empty());
    }
    let out = execute(
        &f,
        &json!({"initial":initial(7),"events":[{"action":"open-search","source":search(&[300])},{"action":"select","index_reads":[0,9]}]}),
    );
    exit(&out, 3);
    let v = report(&out)["prefix"].clone();
    assert_eq!(v["state"]["search_result"], -1);
    assert_eq!(v["state"]["popup"], "open");
}

fn initial_asset() -> Vec<u8> {
    let mut raw = vec![0; 0x3c9c0];
    raw[0x355a1] = 6;
    raw[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    for group in 0..100 {
        let at = 0x355c0 + 19 * group;
        raw[at] = if group == 99 { 255 } else { (group + 1) as u8 };
        raw[at + 1..at + 3].fill(255);
        raw[at + 3..at + 19].fill(b' ');
    }
    for (first, last) in [
        (0x35d2c, 0x36628),
        (0x3ab82, 0x3bf6e),
        (0x3c0a7, 0x3c2ff),
        (0x3c385, 0x3c613),
    ] {
        raw[first..last].fill(255);
    }
    raw[0x3662d..0x36821].fill(1);
    for row in 0..45 {
        let at = cs705::icf::special::CompactKind::BandStack
            .offset(row)
            .unwrap();
        raw[at..at + 4].fill(255);
    }
    for row in 0..=10 {
        raw[0x36f54 + 36 * row..0x36f58 + 36 * row].fill(255);
    }
    raw[0xa60b..0xa60f].fill(255);
    raw[0x3c84b] = 255;
    raw[0x3c8b9..0x3c8bb].fill(255);
    raw
}

#[test]
fn software_driver_runs_one_query_initial_adoption_information_and_real_close_receivers() {
    let f = Files::new();
    let asset = f.write("source.raw", &initial_asset());
    let mut response = bytes(&wire(b"ACTUAL QUERY", 6));
    response[9 + 0x13] = 8; // The existing context decoder yields kind1.
    let response: String = response.iter().map(|b| format!("{b:02X}")).collect();
    let mock = f.json(
        "query.json",
        &json!({"events":[
            {"write_hex":"FEFEEEEFE040750000FD"},{"read_hex":response}
        ]}),
    );
    let mut start = initial(7);
    start["software"] = json!({"previous_reload_guard":1,"document_flags":[1,0]});
    let query = json!({"mock":p(&mock),"profile_return":257,"defaults":{"assets":{"1":p(&asset)}}});
    let mut script = json!({"initial":start,"events":[normal_construct(),{"action":"input","text":b(b"12")},
        {"action":"manual-commit","busy_reads":[false,false],"query":query}]});
    let out = execute(&f, &script);
    exit(&out, 0);
    let v = value(&out);
    assert_eq!(v["state"]["com"], 12);
    assert_eq!(v["state"]["reidentify_requested"], false);
    assert_eq!(v["state"]["identity_result"], 0);
    assert_eq!(v["state"]["information"], "open");
    assert_eq!(
        bytes(&v["information_rows"][0][0]["text"]),
        b"Device information"
    );
    assert_eq!(bytes(&v["information_rows"][1][0]["text"]), b"Stored label");
    assert_eq!(
        bytes(&v["information_rows"][1][1]["text"]),
        b"ACTUAL QUERY    "
    );
    assert_eq!(v["software"]["q"][0], 1);
    assert_eq!(v["software"]["default_selector"], 1);
    assert_eq!(v["software"]["document"]["current_matches_defaults"], true);
    assert_eq!(v["software"]["document"]["baseline_equal"], true);
    assert_eq!(v["software"]["ui"]["form"]["enabled58"], 0);
    assert_eq!(v["software"]["ui"]["fc"], 1);
    assert_eq!(
        v["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["request"] == "identity_result")
            .count(),
        1
    );
    script["events"].as_array_mut().unwrap().extend([
        json!({"action":"information-click"}),
        json!({"action":"destroy","target":"information"}),
        json!({"action":"cancel","returns":[0,0,0]}),
    ]);
    let out = execute(&f, &script);
    exit(&out, 0);
    let v = value(&out);
    assert_eq!(v["state"]["information"], "destroyed");
    assert_eq!(v["state"]["form"], "release_requested");
    assert_eq!(v["state"]["pair_registered"], false);
    assert_eq!(v["software"]["ui"]["application5b"], 1);
    assert_eq!(v["software"]["ui"]["form"]["enabled58"], 1);
    assert_eq!(v["software"]["ui"]["form"]["focused"], true);
    let mut direct = script.clone();
    direct["events"].as_array_mut().unwrap().truncate(3);
    direct["events"][2]["action"] = json!("reidentify");
    let out = execute(&f, &direct);
    exit(&out, 0);
    assert_eq!(value(&out)["state"]["com"], 7);
    let mut equal = script.clone();
    equal["events"].as_array_mut().unwrap().truncate(3);
    let mut q = [0; 24];
    q[0] = 1;
    equal["initial"]["software"]["q"] = json!(q);
    equal["events"][2]["query"]
        .as_object_mut()
        .unwrap()
        .remove("defaults");
    equal["events"][2]["query"]
        .as_object_mut()
        .unwrap()
        .remove("profile_return");
    let out = execute(&f, &equal);
    exit(&out, 0);
    assert!(value(&out)["software"]["default_selector"].is_null());
    assert_eq!(value(&out)["state"]["information"], "open");
    for (visible, native_return, adopted) in
        [(2, Some(257), true), (2, Some(256), false), (0, None, true)]
    {
        let mut case = script.clone();
        case["initial"]["software"]["visible57"] = json!(visible);
        case["events"].as_array_mut().unwrap().truncate(3);
        case["events"][2]["query"]["close_returns"] = json!([0, 0, 0]);
        if let Some(native_return) = native_return {
            case["events"][2]["query"]["profile_return"] = json!(native_return);
        } else {
            case["events"][2]["query"]
                .as_object_mut()
                .unwrap()
                .remove("profile_return");
        }
        let out = execute(&f, &case);
        exit(&out, 0);
        assert_eq!(
            value(&out)["software"]["default_selector"],
            if adopted { json!(1) } else { Value::Null }
        );
        assert_eq!(value(&out)["state"]["form"], "release_requested");
        assert_eq!(value(&out)["state"]["pair_registered"], false);
        assert_eq!(value(&out)["software"]["ui"]["application5b"], 1);
        for returns in [
            json!([]),
            json!([0]),
            json!([0, 0]),
            json!([0, "exception"]),
        ] {
            case["events"][2]["query"]["close_returns"] = returns;
            let out = execute(&f, &case);
            exit(&out, 3);
            assert!(out.stdout.is_empty());
            let failure = report(&out);
            assert_eq!(failure["published"], false);
            assert_eq!(failure["prefix"]["state"]["identity_result"], 0);
            assert_eq!(
                failure["prefix"]["software"]["default_selector"],
                if adopted { json!(1) } else { Value::Null }
            );
            assert!(!names(&failure["prefix"]).contains(&"create_information_2_by_2"));
        }
    }
    let keep = f.write("keep.json", b"KEEP");
    let mut missing = script.clone();
    missing["events"].as_array_mut().unwrap().truncate(3);
    missing["events"][2]["query"]
        .as_object_mut()
        .unwrap()
        .remove("defaults");
    let path = f.json("missing.json", &missing);
    let out = cli(
        &[
            "information",
            "--script",
            p(&path),
            "--overwrite",
            "-o",
            p(&keep),
        ],
        b"",
    );
    exit(&out, 3);
    assert!(out.stdout.is_empty());
    assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    let v = report(&out)["prefix"].clone();
    assert_eq!(v["state"]["identity_result"], 0);
    assert_eq!(v["software"]["q"][0], 1); // Early Q adoption is not rolled back.
    assert_eq!(v["software"]["ui"]["form"]["enabled58"], 1);
    let silent = f.json(
        "silent.json",
        &json!({"events":[{"write_hex":"FEFEEEEFE040750000FD"}]}),
    );
    missing["events"][2]["query"] = json!({"mock":p(&silent)});
    let out = execute(&f, &missing);
    exit(&out, 3);
    let v = report(&out)["prefix"].clone();
    assert_eq!(v["state"]["identity_result"], 5);
    assert_eq!(v["state"]["reidentify_requested"], false);
    assert_eq!(v["software"]["ui"]["form"]["enabled58"], 1);
    assert_eq!(v["state"]["information"], "absent");
    assert_eq!(fs::read(asset).unwrap(), initial_asset());
}

#[test]
fn information_uses_owned_english_captions_and_never_reads_ui_assets() {
    let f = Files::new();
    let mut start = initial(0);
    start.as_object_mut().unwrap().remove("text");
    start["document"] = json!([6, 1, 0]);
    let out = execute(
        &f,
        &json!({"initial":start,"events":[identify(b"USER LABEL",6)]}),
    );
    exit(&out, 0);
    let mut state = value(&out);
    assert_eq!(
        bytes(&state["information_rows"][0][0]["text"]),
        b"Device information"
    );
    assert_eq!(
        bytes(&state["information_rows"][1][0]["text"]),
        b"Stored label"
    );
    state["state"]["captions"] = json!([[0x82, 0xa0], [0xff]]);
    let snapshot = f.json("captured.json", &state);
    let script = f.json("resume.json", &json!({"events":[]}));
    let out = cli(
        &[
            "information",
            "--script",
            p(&script),
            "--state",
            p(&snapshot),
            "-o",
            "-",
        ],
        b"",
    );
    exit(&out, 0);
    assert_eq!(
        bytes(&value(&out)["information_rows"][0][0]["text"]),
        b"Device information"
    );
    assert_eq!(
        bytes(&value(&out)["information_rows"][1][1]["text"]),
        b"USER LABEL      "
    );
    let out = execute(
        &f,
        &json!({"initial":{"com":7,"document":[6,0,1],"software":{
        "previous_reload_guard":1,"document_flags":[1,0],"native":{"asset":"MUST_NOT_BE_READ"}
    }},"events":[]}),
    );
    exit(&out, 3);
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown information source member"));
}
