//! Real registered command tests; all files remain under target/private.
use cs705::{
    icf::{
        Icf,
        context::Context,
        record::{self, ChannelField as C, RepeaterField as R, RepeaterText as T},
    },
    repeater::working::{DefaultContext, Working},
};
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
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/private");
        fs::create_dir_all(&root).unwrap();
        let dir = root.join(format!(
            "native-repeater-process-{}-{}",
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
fn cli(args: &[&str], bytes: &[u8]) -> Output {
    let args: Vec<_> = std::iter::once("repeater")
        .chain(args.iter().copied())
        .collect();
    root_cli(&args, bytes)
}
fn root_cli(args: &[&str], bytes: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cs705"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Err(e) = child.stdin.take().unwrap().write_all(bytes) {
        assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe);
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
fn reports(out: &Output) -> Vec<Value> {
    serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter()
        .map(|v| v.unwrap())
        .collect()
}
fn context() -> Vec<u8> {
    let hex = "00".repeat(24);
    serde_json::to_vec(&json!({"generation_q_hex":hex,"live_q_hex":hex})).unwrap()
}
fn fixture() -> Vec<u8> {
    let mut m = vec![0; 0x4997a];
    m[0x355a1] = 6;
    m[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
    m[0x4991a..0x4991c].copy_from_slice(&1_u16.to_le_bytes());
    m[0x49926..0x49928].copy_from_slice(&1_u16.to_le_bytes());
    for at in [0x392fd, 0x3c8b9, 0x3c903] {
        m[at..at + 2].fill(255);
    }
    m[0x100..0x106].copy_from_slice(b"OPAQUE");
    m[0xaa0f + 51] = 0x25;
    m[0xaa0f + 52 + 51] = 0x16;
    let mut w = Working::from_working(m).unwrap();
    for (index, name, call, gateway) in [
        (0, b"FIRST           ", b"CALLSIGN", b"JA1ABC G"),
        (1, b"SECOND          ", b"CALLSIGN", b"JA2DEF G"),
    ] {
        let mut h = record::reset_holder(0x0c, index, 0);
        h.set_valid(true);
        h.set_repeater_field(R::Mode, 2);
        h.set_name(name);
        h.set_repeater_text(T::Call, call);
        h.set_repeater_text(T::Gateway, gateway);
        h.set_channel_field(C::FrequencyHz, 0);
        h.set_repeater_field(R::UtcOffset, 540);
        w.save(index as usize, &h).unwrap();
    }
    let mut image = w.to_icf().unwrap();
    // Independent nonidentity saved order; group 3 owns logical 0, group 9 logical 1.
    image.patch_memory(&[(0x3ab82, &[1, 0, 0, 0])]).unwrap();
    String::from_utf8(image.to_bytes())
        .unwrap()
        .replace("#Comment=\r\n", "#Comment=REPEATER META\r\n")
        .replace("#EtcData=00000000", "#EtcData=00000003")
        .into_bytes()
}
fn loaded(bytes: &[u8], index: usize) -> record::DecodedRecord {
    let image = Icf::parse(bytes).unwrap();
    let q = Context::from_bytes([0; 24]);
    let w = Working::from_icf(&image, DefaultContext::from_context(&q, None).unwrap()).unwrap();
    let mut h = record::reset_holder(0x0c, index as u32, 0);
    w.load_into(index, &mut h).unwrap();
    h
}

#[test]
fn frequency_offset_uses_generation_b_not_live_q_or_later_langid() {
    let f = Files::new();
    let input = f.write("in.icf", &fixture());
    for (source, live, langid, offset) in [
        (0_u8, 1_u8, None, 0),
        (1, 0, None, 600_000),
        (1, 16, Some(0x411_u16), 600_000),
        (0, 16, Some(0x409), 0),
    ] {
        let mut generation = [0; 24];
        generation[0] = source;
        let mut current = [0; 24];
        current[0] = live;
        let hex = |b: &[u8]| b.iter().map(|b| format!("{b:02X}")).collect::<String>();
        let mut context = json!({"generation_q_hex":hex(&generation),"live_q_hex":hex(&current)});
        if let Some(id) = langid {
            context["system_langid"] = json!(id);
        }
        let q = f.write(
            &format!("b{source}-q{live}-{langid:?}"),
            &serde_json::to_vec(&context).unwrap(),
        );
        let out = cli(
            &[
                "edit",
                p(&input),
                "0",
                "--range-context",
                p(&q),
                "--set",
                "frequency=145",
                "-o",
                "-",
            ],
            b"",
        );
        success(&out);
        let h = loaded(&out.stdout, 0);
        assert_eq!(h.channel_field(C::FrequencyHz), 145_000_000);
        assert_eq!(h.offset_hz(), offset);
        assert_eq!(reports(&out)[0]["native"]["selected"], u32::from(source));
    }
}

#[test]
fn catalog_read_and_all_sixteen_editable_grid_fields_reach_typed_storage() {
    let f = Files::new();
    let q = f.write("q", &context());
    let input = f.write("in.icf", &fixture());
    let output = f.path("out.icf");
    let out = cli(&["fields"], b"");
    success(&out);
    let fields: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(fields.as_array().unwrap().len(), 17);
    assert_eq!(fields[0]["read_only"], true);
    let out = cli(&["read", p(&input), "0", "--range-context", p(&q)], b"");
    success(&out);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["group"], 3);
    assert_eq!(v["name"]["utf8"], "SECOND          ");
    assert_eq!(v["gateway-call"]["utf8"], "JA2DEF G");
    let out = cli(
        &[
            "edit",
            p(&input),
            "0",
            "--range-context",
            p(&q),
            "--set",
            "type=0",
            "--set",
            "name=New name",
            "--set",
            "sub-name=Sub",
            "--set",
            "repeater-call=ja3ghi b",
            "--set",
            "gateway-call=ja3ghi g",
            "--set",
            "frequency=145.000025",
            "--set",
            "duplex=2",
            "--set",
            "offset=0.600049",
            "--set",
            r#"mode={"action":"selected","key":10,"phase":2,"candidate":0,"accepted":true}"#,
            "--set",
            "tone=2",
            "--set",
            "tone-frequency=88.5",
            "--set",
            "use=1",
            "--set",
            "position=2",
            "--set",
            "latitude=35 12.34 S",
            "--set",
            "longitude=139 56.78 W",
            "--set",
            "utc-offset=+9:00",
            "-o",
            p(&output),
        ],
        b"",
    );
    success(&out);
    let report = reports(&out);
    assert_eq!(report[0]["operations"].as_array().unwrap().len(), 16);
    for operation in report[0]["operations"].as_array().unwrap() {
        assert_eq!(operation["effect"]["save_returned"], true, "{operation}");
    }
    let bytes = fs::read(&output).unwrap();
    let image = Icf::parse(&bytes).unwrap();
    assert_eq!(image.comment_bytes(), b"REPEATER META");
    assert_eq!(image.extra_data(), 3);
    assert_eq!(&image.memory()[0x100..0x106], b"OPAQUE");
    let h = loaded(&bytes, 0);
    assert_eq!(h.name(), b"New name        ");
    assert_eq!(h.repeater_text(T::SubName), b"Sub     ");
    assert_eq!(h.repeater_text(T::Call), b"JA3GHI B");
    assert_eq!(h.repeater_text(T::Gateway), b"JA3GHI G");
    assert_eq!(
        (
            h.channel_field(C::FrequencyHz),
            h.offset_hz(),
            h.channel_field(C::Duplex)
        ),
        (145_000_025, 600_000, 2)
    );
    assert_eq!(
        (
            h.repeater_field(R::Mode),
            h.repeater_field(R::Tone),
            h.repeater_field(R::ToneIndex)
        ),
        (0, 2, 8)
    );
    assert_eq!(*h.coordinate_bytes(false), [35, 0x30, 0x34, b'S']);
    assert_eq!(*h.coordinate_bytes(true), [139, 0xdd, 0xcc, b'W']);
    // Literal byte windows do not obtain their expectations from the decoder.
    assert_eq!(&image.memory()[0xaa0f..0xaa13], &[0x59, 0x86, 0xa4, 0x08]);
    assert_eq!(
        &image.memory()[0xaa0f + 18..0xaa0f + 34],
        b"New name        "
    );
    assert_eq!(image.memory()[0xaa0f + 51] & 0x3f, 0x25);
    assert_eq!(loaded(&bytes, 1).name(), b"FIRST           ");
}

#[test]
fn numeric_shared_grammar_unknown_old_full_candidate_and_cancel_are_not_defaults() {
    let f = Files::new();
    let q = f.write("q", &context());
    let input = f.write("in.icf", &fixture());
    for (text, expected) in [
        ("1e-3", 1_000_000),
        (r#" {"action":"text","text":"4.0000005"}"#, 4_000_000),
    ] {
        let out = cli(
            &[
                "edit",
                p(&input),
                "0",
                "--range-context",
                p(&q),
                "--set",
                &format!("frequency={text}"),
                "-o",
                "-",
            ],
            b"",
        );
        success(&out);
        assert_eq!(
            loaded(&out.stdout, 0).channel_field(C::FrequencyHz),
            expected
        );
    }
    for action in ["display", "cancel", "begin"] {
        let output = f.path(action);
        let out = cli(
            &[
                "edit",
                p(&input),
                "0",
                "--range-context",
                p(&q),
                "--set",
                &format!("frequency={{\"action\":\"{action}\"}}"),
                "-o",
                p(&output),
            ],
            b"",
        );
        success(&out);
        assert!(!output.exists());
        assert!(out.stdout.is_empty());
    }
    let out = cli(
        &[
            "edit",
            p(&input),
            "0",
            "--range-context",
            p(&q),
            "--set",
            r#"mode={"action":"selected","key":10,"phase":2,"candidate":257,"accepted":true}"#,
            "--set",
            r#"mode={"action":"selected","key":10,"phase":2,"candidate":0,"accepted":true}"#,
            "--set",
            "use=257",
            "--set",
            "position=258",
            "-o",
            "-",
        ],
        b"",
    );
    success(&out);
    let r = reports(&out);
    assert_eq!(r[0]["operations"][0]["effect"]["candidate"], 257);
    let h = loaded(&out.stdout, 0);
    assert_eq!(h.repeater_field(R::Mode), 0);
    assert_eq!(h.repeater_field(R::Use), 1);
    assert_eq!(h.repeater_field(R::Position), 2);
    let out = cli(
        &[
            "edit",
            p(&input),
            "0",
            "--range-context",
            p(&q),
            "--set",
            "index=1",
            "-o",
            "-",
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

#[test]
fn nls_real_order_capacity_zero_return_partial_output_and_late_source_failure() {
    let f = Files::new();
    let q = f.write("q", &context());
    let input = f.write("in.icf", &fixture());
    let events = json!([{"api":"lcmap","input_hex":"612062202020","written_hex":"7879","return":0},{"api":"upper","input_hex":"787920","written_hex":"5a","return":0}]);
    let nls = f.write("nls", &serde_json::to_vec(&events).unwrap());
    let out = cli(
        &[
            "edit",
            p(&input),
            "0",
            "--range-context",
            p(&q),
            "--nls-source",
            p(&nls),
            "--set",
            r#"gateway-call={"action":"text","hex":"610062"}"#,
            "-o",
            "-",
        ],
        b"",
    );
    success(&out);
    assert_eq!(
        loaded(&out.stdout, 0).repeater_text(T::Gateway),
        b"Zy      "
    );
    let report = reports(&out);
    assert_eq!(
        report[0]["operations"][0]["effect"]["shared_input"]["hex"],
        "787920"
    );
    let nls = f.write(
        "late",
        br#"[{"api":"lcmap","input_hex":"61622020","written_hex":"7879","return":0}]"#,
    );
    for partial in [false, true] {
        let output = f.path(if partial { "prefix" } else { "no-prefix" });
        let mut args = vec![
            "edit",
            p(&input),
            "0",
            "--range-context",
            p(&q),
            "--nls-source",
            p(&nls),
            "--set",
            "name=Saved prefix",
            "--set",
            "gateway-call=ab",
            "-o",
            p(&output),
        ];
        if partial {
            args.extend(["--accept-partial", "--yes"]);
        }
        let out = cli(&args, b"");
        assert_eq!(out.status.code(), Some(if partial { 5 } else { 3 }));
        assert_eq!(output.exists(), partial);
        assert!(out.stdout.is_empty());
        if partial {
            assert_eq!(
                loaded(&fs::read(output).unwrap(), 0).name(),
                b"Saved prefix    "
            );
        }
    }
    let out = cli(
        &[
            "edit",
            p(&input),
            "0",
            "--range-context",
            p(&q),
            "--set",
            r#"gateway-call={"action":"text","hex":"82a0"}"#,
            "-o",
            "-",
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

#[test]
fn type_enter_insert_cancel_and_late_g_publish_only_authorized_prefix() {
    let f = Files::new();
    let q = f.write("q", &context());
    let input = f.write("in.icf", &fixture());
    let mut g = vec![0; 0x3c905];
    g[0x3c8b9..0x3c8bb].copy_from_slice(&1234_u16.to_le_bytes());
    g[0x3c903..0x3c905].copy_from_slice(&4321_u16.to_le_bytes());
    let full = f.write("g", &g);
    let short = f.write("short-g", &g[..0x3c8bb]);
    let mut args = vec![
        "edit",
        p(&input),
        "2",
        "--group",
        "9",
        "--range-context",
        p(&q),
        "--generation",
        p(&full),
        "--set",
        r#"type={"action":"begin"}"#,
        "--set",
        r#"type={"action":"cancel"}"#,
        "-o",
        "-",
    ];
    let out = cli(&args, b"");
    success(&out);
    assert!(out.stdout.is_empty());
    let r = reports(&out);
    assert_eq!(r[0]["publication_authorized"], false);
    assert_eq!(r[0]["operations"][0]["effect"]["inserted"], true);
    assert_eq!(r[0]["operations"][1]["row"]["valid"], true);
    assert_eq!(r[0]["operations"][1]["row"]["group"], 9);
    assert_eq!(
        r[0]["operations"][0]["effect"]["notifications"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let keep = f.write("keep.icf", b"KEEP");
    *args.last_mut().unwrap() = p(&keep);
    args.push("--overwrite");
    for partial in [false, true] {
        let mut cancel = args.clone();
        if partial {
            cancel.extend(["--accept-partial", "--yes"]);
        }
        let out = cli(&cancel, b"");
        success(&out);
        assert!(out.stdout.is_empty());
        assert_eq!(reports(&out)[0]["publication_authorized"], false);
        assert_eq!(fs::read(&keep).unwrap(), b"KEEP");
    }
    args.extend(["--set", "type=0"]);
    let out = cli(&args, b"");
    success(&out);
    assert_eq!(reports(&out)[0]["publication_authorized"], true);
    assert_eq!(
        reports(&out)[0]["operations"][2]["effect"]["save_returned"],
        true
    );
    let saved = fs::read(&keep).unwrap();
    assert!(loaded(&saved, 2).valid());
    assert_eq!(loaded(&saved, 2).group(), 9);
    let image = Icf::parse(&saved).unwrap();
    assert_eq!(&image.memory()[0x3c8b9..0x3c8bb], &1234_u16.to_le_bytes());
    for partial in [false, true] {
        let output = f.path(if partial { "partial" } else { "nothing" });
        let mut args = vec![
            "edit",
            p(&input),
            "2",
            "--group",
            "9",
            "--range-context",
            p(&q),
            "--generation",
            p(&short),
            "--set",
            r#"type={"action":"begin"}"#,
            "-o",
            p(&output),
        ];
        if partial {
            args.extend(["--accept-partial", "--yes"]);
        }
        let out = cli(&args, b"");
        assert_eq!(out.status.code(), Some(if partial { 5 } else { 3 }));
        assert_eq!(output.exists(), partial);
        let r = reports(&out);
        assert_eq!(r[0]["operations"][0]["effect"]["inserted"], true);
        if partial {
            assert!(loaded(&fs::read(output).unwrap(), 2).valid());
        }
    }
}

#[test]
fn false_optional_utc_and_coordinate_holder_only_fault_do_not_publish() {
    let f = Files::new();
    let q = f.write("q", &context());
    let bytes = fixture();
    let mut image = Icf::parse(&bytes).unwrap();
    image.patch_memory(&[(0x396b4, &[1])]).unwrap();
    let input = f.write("in.icf", &image.to_bytes());
    let output = f.path("utc");
    let out = cli(
        &[
            "edit",
            p(&input),
            "0",
            "--range-context",
            p(&q),
            "--set",
            "utc-offset=--:--99:99",
            "-o",
            p(&output),
        ],
        b"",
    );
    success(&out);
    assert!(!output.exists());
    let r = reports(&out);
    assert_eq!(r[0]["operations"][0]["effect"]["candidate"], 32767);
    assert_eq!(r[0]["operations"][0]["effect"]["helper_accepted"], false);
    assert_eq!(r[0]["operations"][0]["row"]["utc-offset"], 540);
    let output = f.path("coordinate");
    let out = cli(
        &[
            "edit",
            p(&input),
            "0",
            "--range-context",
            p(&q),
            "--set",
            r#"latitude={"action":"phase2","text":"123x45"}"#,
            "--accept-partial",
            "--yes",
            "-o",
            p(&output),
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(!output.exists());
    let r = reports(&out);
    assert_eq!(r[0]["publication_authorized"], false);
    assert!(
        r[0]["operations"][0]["row"]["latitude"]["hex"]
            .as_str()
            .unwrap()
            .starts_with("7B")
    );
}

#[test]
fn normal_coordinate_masks_modal_callbacks_and_explicit_phase2_stay_distinct() {
    let f = Files::new();
    let q = f.write("q", &context());
    let original = fixture();
    for (unit, input, expected) in [
        (0, "35 12.34 s", [35, 0x30, 0x34, b'S']),
        (1, "35 12 34 s", [35, 0x31, 0x16, b'S']),
        (2, "35.1234s", [35, 0x1c, 0xec, b'S']),
        (255, "35 12.34 s", [35, 0, 0, b'N']),
    ] {
        let mut image = Icf::parse(&original).unwrap();
        image.patch_memory(&[(0x396b4, &[unit])]).unwrap();
        let path = f.write(&format!("unit-{unit}"), &image.to_bytes());
        let out = cli(
            &[
                "edit",
                p(&path),
                "0",
                "--range-context",
                p(&q),
                "--set",
                &format!("latitude={input}"),
                "-o",
                "-",
            ],
            b"",
        );
        success(&out);
        assert_eq!(*loaded(&out.stdout, 0).coordinate_bytes(false), expected);
        let r = reports(&out);
        assert_eq!(
            r[0]["operations"][0]["effect"]["coordinate_dialog"]["done"],
            true
        );
    }
    let mut image = Icf::parse(&original).unwrap();
    image.patch_memory(&[(0x396b4, &[1])]).unwrap();
    let input = f.write("dms", &image.to_bytes());
    for (name, value, stores) in [
        (
            "invalid",
            r#"latitude={"action":"text","text":"35 12 60 N"}"#,
            false,
        ),
        (
            "wrong-direction",
            r#"latitude={"action":"text","text":"35 12 34 W"}"#,
            false,
        ),
        ("short", r#"latitude={"action":"text","text":"35N"}"#, false),
        (
            "cancel",
            r#"latitude={"action":"dialog","text":"35 12 34 S","events":["cancel","deactivate"]}"#,
            false,
        ),
        (
            "deactivate-seed",
            r#"latitude={"action":"dialog","text":"35 12 60 N","events":["deactivate"]}"#,
            true,
        ),
        (
            "direct-escape",
            r#"latitude={"action":"dialog","text":"35 12 34 S","events":["escape-keypress","deactivate"]}"#,
            true,
        ),
        (
            "same-value",
            r#"latitude={"action":"text","text":"00 00 00 N"}"#,
            true,
        ),
    ] {
        let output = f.path(name);
        let out = cli(
            &[
                "edit",
                p(&input),
                "0",
                "--range-context",
                p(&q),
                "--set",
                value,
                "-o",
                p(&output),
            ],
            b"",
        );
        success(&out);
        assert_eq!(output.exists(), stores, "{name}");
        let r = reports(&out);
        assert_eq!(
            r[0]["operations"][0]["effect"]["save_returned"], stores,
            "{name}"
        );
        if name == "deactivate-seed" {
            assert_eq!(
                *loaded(&fs::read(&output).unwrap(), 0).coordinate_bytes(false),
                [0, 0, 0, b'N']
            );
            assert_eq!(
                r[0]["operations"][0]["effect"]["coordinate_dialog"]["valid"],
                false
            );
        }
    }
    let out = cli(
        &[
            "edit",
            p(&input),
            "0",
            "--range-context",
            p(&q),
            "--set",
            r#"latitude={"action":"phase2","text":"035x12y60zS"}"#,
            "-o",
            "-",
        ],
        b"",
    );
    success(&out);
    assert_eq!(
        *loaded(&out.stdout, 0).coordinate_bytes(false),
        [35, 0x2e, 0xe0, b'S']
    );
}

#[test]
fn existing_group_root_uses_native_mode1_without_row_rebuild_or_second_field() {
    let f = Files::new();
    let bytes = fixture();
    let input = f.write("in.icf", &bytes);
    let output = f.path("out.icf");
    let out = root_cli(
        &[
            "group",
            p(&input),
            "--sheet",
            "repeater",
            "rename",
            "49",
            r#"{"action":"text","hex":"204100422082a0"}"#,
            "-o",
            p(&output),
        ],
        b"",
    );
    success(&out);
    let saved = fs::read(&output).unwrap();
    let before = Icf::parse(&bytes).unwrap();
    let after = Icf::parse(&saved).unwrap();
    let at = 0x2ea3b + 49 * 16;
    assert_eq!(&after.memory()[at..at + 16], b" A B \x82\xa0         ");
    assert_eq!(&after.memory()[..at], &before.memory()[..at]);
    assert_eq!(&after.memory()[at + 16..], &before.memory()[at + 16..]);
    assert_eq!(after.comment_bytes(), b"REPEATER META");
    let r = reports(&out);
    assert_eq!(r[0]["effect"]["group_label"]["node"], 172);
    assert_eq!(r[0]["effect"]["refresh_if_binding_matches"], true);
    let out = root_cli(
        &[
            "group",
            p(&input),
            "--sheet",
            "repeater",
            "rename",
            "49",
            "No clobber",
            "-o",
            p(&output),
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(fs::read(&output).unwrap(), saved);
    let out = root_cli(
        &[
            "group",
            p(&input),
            "--sheet",
            "repeater",
            "rename",
            "49",
            "Dry run",
            "--dry-run",
            "-o",
            p(&output),
        ],
        b"",
    );
    success(&out);
    assert_eq!(fs::read(&output).unwrap(), saved);
    for action in ["display", "cancel", "begin"] {
        let path = f.path(action);
        let out = root_cli(
            &[
                "group",
                p(&input),
                "--sheet",
                "repeater",
                "rename",
                "49",
                &format!("{{\"action\":\"{action}\"}}"),
                "-o",
                p(&path),
            ],
            b"",
        );
        success(&out);
        assert!(!path.exists());
    }
    let out = root_cli(&["group", p(&input), "--sheet", "repeater", "list"], b"");
    success(&out);
    assert!(
        serde_json::from_slice::<Value>(&out.stdout)
            .unwrap()
            .is_array()
    );
    let out = root_cli(
        &[
            "group",
            p(&input),
            "--sheet",
            "repeater",
            "rename",
            "50",
            "X",
            "-o",
            "-",
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

#[test]
fn native_tone_crt_exponent_hex_prefix_and_captured_upper_reach_process() {
    let f = Files::new();
    let q = f.write("q", &context());
    let input = f.write("in.icf", &fixture());
    for (text, index) in [
        ("8.85e1", 8),
        ("0x58.8p1", 0),
        ("8.85e+", 0),
        ("1e100", 0),
        ("+inf", 0),
    ] {
        let out = cli(
            &[
                "edit",
                p(&input),
                "0",
                "--range-context",
                p(&q),
                "--set",
                "type=2",
                "--set",
                &format!("tone-frequency={text}"),
                "-o",
                "-",
            ],
            b"",
        );
        success(&out);
        assert_eq!(loaded(&out.stdout, 0).repeater_field(R::ToneIndex), index);
    }
    let source = f.write(
        "upper",
        br#"[{"api":"upper","input_hex":"382e38356531","written_hex":"382e38354531","return":0}]"#,
    );
    let out = cli(
        &[
            "edit",
            p(&input),
            "0",
            "--range-context",
            p(&q),
            "--nls-source",
            p(&source),
            "--set",
            "type=2",
            "--set",
            "tone-frequency=8.85e1",
            "-o",
            "-",
        ],
        b"",
    );
    success(&out);
    assert_eq!(loaded(&out.stdout, 0).repeater_field(R::ToneIndex), 8);
    assert_eq!(reports(&out)[0]["NLS_requests"], 1);
}

#[test]
fn complete_stdin_set_checked_before_read_and_publisher_preserves_no_clobber_dry_run() {
    let f = Files::new();
    let q = f.write("q", &context());
    let input = f.write("in.icf", &fixture());
    let output = f.write("out", b"KEEP");
    for pair in [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)] {
        let mut paths = ["missing-input", "missing-q", "missing-g", "missing-nls"];
        paths[pair.0] = "-";
        paths[pair.1] = "-";
        let out = cli(
            &[
                "edit",
                paths[0],
                "0",
                "--range-context",
                paths[1],
                "--generation",
                paths[2],
                "--nls-source",
                paths[3],
                "--set",
                "name=X",
                "-o",
                "-",
            ],
            b"",
        );
        assert_eq!(out.status.code(), Some(3));
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains("stdin may be consumed by only one input")
        );
        assert!(out.stdout.is_empty());
    }
    let args = [
        "edit",
        p(&input),
        "0",
        "--range-context",
        p(&q),
        "--set",
        "name=Same store",
        "-o",
        p(&output),
    ];
    let out = cli(&args, b"");
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(fs::read(&output).unwrap(), b"KEEP");
    let mut dry = args.to_vec();
    dry.push("--dry-run");
    let out = cli(&dry, b"");
    success(&out);
    assert_eq!(fs::read(&output).unwrap(), b"KEEP");
    let mut overwrite = args.to_vec();
    overwrite.push("--overwrite");
    success(&cli(&overwrite, b""));
    let bytes = fs::read(&output).unwrap();
    let out = cli(
        &[
            "edit",
            p(&output),
            "0",
            "--range-context",
            p(&q),
            "--set",
            "name=Same store",
            "-o",
            "-",
        ],
        b"",
    );
    success(&out);
    assert_eq!(
        reports(&out)[0]["operations"][0]["effect"]["save_returned"],
        true
    );
    assert_eq!(out.stdout, bytes);
    let out = cli(
        &[
            "edit",
            p(&input),
            "0",
            "--range-context",
            p(&q),
            "--accept-partial",
            "--set",
            "name=X",
            "-o",
            "-",
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
}

#[test]
fn repeater_r3_bulk_use_runs_live_getters_typed_saves_and_ordered_notifications() {
    let f = Files::new();
    let bytes = fixture();
    let image = Icf::parse(&bytes).unwrap();
    let q = Context::from_bytes([0; 24]);
    let mut working =
        Working::from_icf(&image, DefaultContext::from_context(&q, None).unwrap()).unwrap();
    for index in 0..2 {
        let mut holder = record::reset_holder(12, index, 0);
        working.load_into(index as usize, &mut holder).unwrap();
        holder.set_channel_field(C::FrequencyHz, 145_000_000);
        holder.set_channel_field(C::Duplex, 1);
        holder.set_repeater_field(R::Use, 0);
        working.save(index as usize, &holder).unwrap();
    }
    let original = working.to_icf().unwrap().to_bytes();
    let input = f.write("bulk.icf", &original);
    let context = f.write("q.json", &context());
    let output = f.path("bulk-out.icf");
    let args = [
        "bulk-use",
        p(&input),
        "yes",
        "--range-context",
        p(&context),
        "--group",
        "3",
        "--yes",
        "-o",
        p(&output),
    ];
    let out = cli(&args, b"");
    success(&out);
    let report = &reports(&out)[0];
    assert_eq!(report["live_getter_calls"], 2);
    assert_eq!(report["publication_authorized"], true);
    assert_eq!(report["notifications"][0]["id"], 0x11);
    assert_eq!(report["notifications"][1]["id"], 0x13);
    assert_eq!(report["notifications"][1]["second"], 0x14f);
    let saved = fs::read(&output).unwrap();
    assert_eq!(loaded(&saved, 0).repeater_field(R::Use), 1);
    assert_eq!(loaded(&saved, 1).repeater_field(R::Use), 0);
    let image = Icf::parse(&saved).unwrap();
    assert_eq!(image.comment_bytes(), b"REPEATER META");
    assert_eq!(&image.memory()[0x100..0x106], b"OPAQUE");
    assert_eq!(image.memory()[0xaa0f + 51] & 0x3f, 0x25);
    let same = cli(
        &[
            "bulk-use",
            p(&output),
            "yes",
            "--range-context",
            p(&context),
            "--group",
            "3",
            "--yes",
            "-o",
            "-",
        ],
        b"",
    );
    success(&same);
    assert_eq!(reports(&same)[0]["publication_authorized"], true);
    assert_eq!(same.stdout, saved);
    let no = cli(
        &[
            "bulk-use",
            p(&output),
            "no",
            "--range-context",
            p(&context),
            "--group",
            "3",
            "--yes",
            "-o",
            "-",
        ],
        b"",
    );
    success(&no);
    assert_eq!(reports(&no)[0]["live_getter_calls"], 0);
    assert_eq!(loaded(&no.stdout, 0).repeater_field(R::Use), 0);
    let leading = cli(
        &[
            "bulk-use",
            p(&input),
            "yes",
            "--range-context",
            p(&context),
            "--group",
            "0",
            "--yes",
            "-o",
            "-",
        ],
        b"",
    );
    success(&leading);
    assert_eq!(reports(&leading)[0]["range"], json!({"first":0,"last":0}));
    assert_eq!(loaded(&leading.stdout, 0).repeater_field(R::Use), 1);
    assert_eq!(fs::read(&input).unwrap(), original);
}

#[test]
fn repeater_r3_bulk_use_cancel_denial_failure_and_dry_run_keep_existing_output() {
    let f = Files::new();
    let input = f.write("in.icf", &fixture());
    let q = f.write("q.json", &context());
    let output = f.write("keep.icf", b"KEEP");
    let out = cli(
        &[
            "bulk-use",
            "missing",
            "yes",
            "--range-context",
            "missing",
            "--cancel",
            "--overwrite",
            "-o",
            p(&output),
        ],
        b"",
    );
    success(&out);
    assert_eq!(reports(&out)[0]["inputs_read"], false);
    let args = [
        "bulk-use",
        p(&input),
        "no",
        "--range-context",
        p(&q),
        "--group",
        "3",
        "--overwrite",
        "-o",
        p(&output),
    ];
    let out = cli(&args, b"");
    assert_eq!(out.status.code(), Some(6));
    assert_eq!(fs::read(&output).unwrap(), b"KEEP");
    let mut approved = args.to_vec();
    approved.extend(["--yes", "--dry-run"]);
    success(&cli(&approved, b""));
    assert_eq!(fs::read(&output).unwrap(), b"KEEP");
    fs::write(&q, b"{}").unwrap();
    let out = cli(&approved, b"");
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(fs::read(&output).unwrap(), b"KEEP");
}

#[test]
fn repeater_r3_equal_saved_prefix_is_approval_even_when_later_action_fails() {
    let f = Files::new();
    let original = fixture();
    let input = f.write("in.icf", &original);
    let q = f.write("q.json", &context());
    let output = f.path("prefix.icf");
    let out = cli(
        &[
            "edit",
            p(&input),
            "0",
            "--range-context",
            p(&q),
            "--set",
            "name=SECOND",
            "--set",
            "index=1",
            "--accept-partial",
            "--yes",
            "-o",
            p(&output),
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(5));
    assert_eq!(
        reports(&out)[0]["operations"][0]["effect"]["save_returned"],
        true
    );
    assert_eq!(reports(&out)[0]["publication_authorized"], true);
    assert_eq!(
        loaded(&fs::read(&output).unwrap(), 0).name(),
        b"SECOND          "
    );
}
