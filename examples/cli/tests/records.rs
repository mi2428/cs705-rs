//! Pure-file general-record integration. No device discovery or port command.
use cs705::icf::{
    Icf,
    record::{self, ChannelField},
    record_image::{self, ChannelKind},
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
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/private/logic-collector")
            .join(format!(
                "records-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap();
        path
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
fn cli(args: &[&str], stdin: &[u8]) -> Output {
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
    let mut memory = vec![0xa6; 0x3c9c0];
    memory[0x355a1] = 6;
    memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    memory[0x392fa] = 100;
    memory[0x392fb..0x392fd].copy_from_slice(&1u16.to_le_bytes());
    let mut h = record::reset_holder(3, 0, 0);
    h.set_name(b"SOURCE          ");
    h.set_mode(255);
    h.set_filter(10);
    h.set_channel_field(ChannelField::FrequencyHz, 145_123_456);
    h.set_channel_field(ChannelField::Duplex, 1);
    record_image::store_channel_into(&mut memory, ChannelKind::Call, 0, &mut h, |_| Ok(()))
        .unwrap();
    Icf::from_memory(memory).unwrap().to_bytes()
}
fn holder(bytes: &[u8], index: u32) -> record::DecodedRecord {
    let image = Icf::parse(bytes).unwrap();
    let mut h = record::reset_holder(3, index, 0);
    record::load_channel_image_into(&image, ChannelKind::Call, index, &mut h).unwrap();
    h
}
fn native_call_fixture(mode: u32, dr_frequency: u32) -> Vec<u8> {
    let source = fixture();
    let mut w = Icf::parse(&source).unwrap().memory().to_vec();
    w[0x3c84a] = 1;
    w[0x3c84b] = 100;
    w[0x3c84c..0x3c84e].copy_from_slice(&0u16.to_le_bytes());
    w[0x3a780] = 0;
    w[0x3a781] = 1;
    for index in 0..8 {
        let mut h = record::reset_holder(3, index, 0);
        h.set_mode(mode);
        h.set_filter(1);
        h.set_name(b"NATIVE CALL     ");
        h.set_offset_hz(600_000);
        h.set_channel_field(
            ChannelField::FrequencyHz,
            if index < 4 { 145_000_000 } else { 433_000_000 },
        );
        record_image::store_channel_into(&mut w, ChannelKind::Call, index, &mut h, |_| Ok(()))
            .unwrap();
    }
    let mut dr = record::reset_holder(0x1a, 0, 0);
    dr.set_valid(true);
    dr.set_mode(13);
    dr.set_channel_field(ChannelField::FrequencyHz, dr_frequency);
    record_image::store_paired_into(&mut w, 0x1a, 0, &mut dr, |_, _| Ok(())).unwrap();
    String::from_utf8(Icf::from_memory(w).unwrap().to_bytes())
        .unwrap()
        .replace("#Comment=\r\n", "#Comment=NATIVE CALL META\r\n")
        .replace("#EtcData=00000000", "#EtcData=00000003")
        .into_bytes()
}
fn native_call_report(out: &Output) -> Value {
    serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .filter_map(std::result::Result::ok)
        .find(|v| v.get("kind") == Some(&json!(3)))
        .unwrap_or_else(|| {
            panic!(
                "missing native call report: {}",
                String::from_utf8_lossy(&out.stderr)
            )
        })
}

#[test]
fn native_call_inputs_use_fixed_image_shared_consumers_and_keep_other_side_metadata() {
    let files = Files::new();
    let q = files.write("call-Q", &context(0));
    for (name, input, mode, side, expected) in [
        ("duplex", json!({"action":"next"}), 13, 0, 1),
        ("polarity", json!({"action":"next"}), 13, 0, 3),
        ("digital-squelch", json!({"action":"next"}), 16, 0, 2),
        ("data", json!({"action":"next"}), 13, 0, 1),
        ("filter", json!({"action":"next"}), 13, 0, 2),
        ("offset-hz", json!("0.00015"), 13, 0, 200),
        (
            "mode",
            json!({"input":{"action":"next"},"warning_bits":129}),
            13,
            0,
            16,
        ),
        (
            "name",
            json!({"input":{"action":"text","hex":"410043414C4C2020"},"layout":0,"node_flags":165}),
            13,
            0,
            0,
        ),
        (
            "frequency-hz",
            json!({"input":"150","warning_bits":129,"focus":0}),
            13,
            0,
            148_000_000,
        ),
        (
            "frequency-hz",
            json!({"input":"400","warning_bits":129,"focus":0}),
            13,
            6,
            420_000_000,
        ),
        (
            "digital-code",
            json!({"input":"09","node_flags":165}),
            16,
            0,
            9,
        ),
        ("tone", json!({"action":"next"}), 13, 0, 1),
        ("tx-tone", json!("88.5"), 13, 0, 8),
        ("rx-tone", json!("88.5"), 13, 0, 8),
        ("dtcs", json!("023"), 13, 0, 0),
    ] {
        let source = native_call_fixture(mode, 145_000_000);
        let state = files.write(
            "call-state",
            &serde_json::to_vec(&initialized_normal_state(&source, 0, None)).unwrap(),
        );
        let assignment = format!("record/native-call/{side}/{name}={input}");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--record-context",
                p(&q),
                "--set",
                &assignment,
                "--output",
                "-",
            ],
            &source,
        );
        assert!(
            out.status.success(),
            "{assignment}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let report = native_call_report(&out);
        assert_eq!(report["stored"], true);
        assert_eq!(report["group"], 100);
        assert_eq!(report["index"], side);
        assert_eq!(report["range_index"], 12 + side / 4);
        let h = holder(&out.stdout, side);
        let value = match name {
            "name" => {
                assert_eq!(h.name(), b"A CALL          ");
                assert_eq!(report["name_frame"]["node_flags"], 164);
                0
            }
            "data" => h.data(),
            "filter" => h.filter(),
            "mode" => h.mode(),
            "offset-hz" => h.offset_hz(),
            "tx-tone" => h.tx_tone(),
            "rx-tone" => h.rx_tone(),
            _ => h.channel_field(match name {
                "duplex" => ChannelField::Duplex,
                "polarity" => ChannelField::Polarity,
                "digital-squelch" => ChannelField::DigitalSquelch,
                "frequency-hz" => ChannelField::FrequencyHz,
                "digital-code" => ChannelField::DigitalCode,
                "tone" => ChannelField::Tone,
                _ => ChannelField::Dtcs,
            }),
        };
        assert_eq!(value, expected, "{name}");
        if name == "frequency-hz" {
            assert_eq!(
                report["first"]["candidate"],
                if side == 0 { 150_000_000 } else { 400_000_000 }
            );
            assert_eq!(report["second"]["candidate"], expected);
            assert_eq!(report["notify_flags"], 24);
        }
        let before = Icf::parse(&source).unwrap();
        let after = Icf::parse(&out.stdout).unwrap();
        assert_eq!(after.memory().len(), 0x3c9c0);
        assert_eq!(after.comment_bytes(), b"NATIVE CALL META");
        assert_eq!(after.extra_data(), 3);
        let address = record_image::channel_address(before.memory(), ChannelKind::Call, side)
            .unwrap()
            .unwrap();
        let at = address.record_offset;
        let other = at + 1 + 33 * (1 - address.side);
        assert_eq!(
            &after.memory()[other..other + 33],
            &before.memory()[other..other + 33]
        );
        let mut expected = before.memory().to_vec();
        expected[at..at + 83].copy_from_slice(&after.memory()[at..at + 83]);
        assert_eq!(
            after.memory(),
            expected,
            "only the addressed call raw83 may change"
        );
    }
}

#[test]
fn native_call_cancel_same_value_and_or_order_use_typed_authorization() {
    let files = Files::new();
    let source = native_call_fixture(16, 145_000_000);
    let state = files.write(
        "call-state",
        &serde_json::to_vec(&initialized_normal_state(&source, 0, None)).unwrap(),
    );
    let q = files.write("call-Q", &context(0));
    for name in [
        "duplex",
        "polarity",
        "digital-squelch",
        "data",
        "filter",
        "offset-hz",
        "mode",
        "name",
        "frequency-hz",
        "digital-code",
        "tone",
        "tx-tone",
        "rx-tone",
        "dtcs",
    ] {
        let cancel = json!({"action":"cancel"});
        let input = match name {
            "mode" => json!({"input":cancel,"warning_bits":129}),
            "name" => json!({"input":cancel,"layout":0,"node_flags":165}),
            "frequency-hz" => json!({"input":cancel,"warning_bits":129,"focus":0}),
            "digital-code" => json!({"input":cancel,"node_flags":165}),
            _ => cancel,
        };
        let assignment = format!("record/native-call/0/{name}={input}");
        let target = files.path("declined-call.icf");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--record-context",
                p(&q),
                "--set",
                &assignment,
                "--output",
                p(&target),
            ],
            &source,
        );
        assert!(
            out.status.success(),
            "{assignment}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
        assert!(!target.exists(), "{name}");
        let report = native_call_report(&out);
        assert_eq!(report["stored"], false);
        assert_eq!(report["effects"]["home_invalidated"], 0);
        assert_eq!(report["effects"]["dr_invalidated"], 0);
    }
    let cancelled = "record/native-call/0/duplex={\"action\":\"cancel\"}";
    let same = "record/native-call/0/duplex=0";
    for pair in [[cancelled, same], [same, cancelled]] {
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--set",
                pair[0],
                "--set",
                pair[1],
                "--output",
                "-",
            ],
            &source,
        );
        assert!(out.status.success());
        assert_eq!(
            Icf::parse(&out.stdout).unwrap().memory(),
            Icf::parse(&source).unwrap().memory()
        );
    }
    let raw = Icf::parse(&source).unwrap().memory().to_vec();
    for (input, accepted) in [(cancelled, false), (same, true)] {
        let out = cli(
            &[
                "edit",
                "-",
                "--kind",
                "memory",
                "--normal-state",
                p(&state),
                "--set",
                input,
                "--output",
                "-",
            ],
            &raw,
        );
        assert!(out.status.success());
        assert_eq!(out.stdout, if accepted { raw.as_slice() } else { &[] });
    }
}

#[test]
fn native_call_post_save_reads_actual_dr_and_missing_sources_never_publish() {
    let files = Files::new();
    for (dr, side, dr_effect, home) in [
        (145_000_000, 0, 1, 1),
        (433_000_000, 0, 0, 1),
        (145_000_000, 6, 0, 0),
        (433_000_000, 6, 1, 0),
    ] {
        let source = native_call_fixture(13, dr);
        let state = files.write(
            "call-state",
            &serde_json::to_vec(&initialized_normal_state(&source, 0, None)).unwrap(),
        );
        let assignment = format!("record/native-call/{side}/duplex=0");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--set",
                &assignment,
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
        let report = native_call_report(&out);
        assert_eq!(report["effects"]["dr_invalidated"], dr_effect);
        assert_eq!(report["effects"]["home_invalidated"], home);
    }
    let source = native_call_fixture(13, 145_000_000);
    let state = files.write(
        "call-state",
        &serde_json::to_vec(&initialized_normal_state(&source, 0, None)).unwrap(),
    );
    for assignment in [
        "record/native-call/0/selection=1",
        "record/native-call/8/duplex=0",
        "record/native-call/0/split=1",
        "record/native-call/0/mode=16",
        "record/native-call/0/name={\"input\":\"X\"}",
        "record/native-call/0/tx-tone=88.5",
        "record/native-call/0/frequency-hz={\"input\":\"146.52\",\"warning_bits\":0,\"focus\":0}",
    ] {
        let target = files.path("failed-call.icf");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--set",
                assignment,
                "--output",
                p(&target),
            ],
            &source,
        );
        assert!(!out.status.success(), "{assignment}");
        assert!(out.stdout.is_empty());
        assert!(!target.exists());
    }
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "record/native-call/0/duplex=0",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            "-",
            "--set",
            "record/native-call/0/duplex=0",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("stdin"));
}

#[test]
fn native_call_split_uses_shared_frame_full_candidate_and_false_tail() {
    let files = Files::new();
    let source = native_call_fixture(13, 145_000_000);
    let state = files.write(
        "call-state",
        &serde_json::to_vec(&initialized_normal_state(&source, 0, None)).unwrap(),
    );
    for (input, raw, notices, active) in [
        (json!(1), 1, json!([0x45, 0x3c]), 47),
        (json!(256), 0, json!([0x45, 0x3c]), 47),
        (json!(0), 0, json!([0x3c]), 62),
        (json!({"action":"cancel"}), 0, json!([]), 62),
    ] {
        let assignment = split_assignment(input, 0, 1).replacen(
            "record/native-memory/0/2/",
            "record/native-call/0/",
            1,
        );
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--set",
                &assignment,
                "--output",
                "-",
            ],
            &source,
        );
        assert!(
            out.status.success(),
            "{assignment}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let report = native_call_report(&out);
        assert_eq!(report["split_frame"]["node_flags"], 164);
        assert_eq!(report["split_frame"]["active"], active);
        let actual: Vec<_> = report["effects"]["requested_notifications"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].clone())
            .collect();
        assert_eq!(json!(actual), notices);
        if notices.as_array().unwrap().is_empty() {
            assert_eq!(report["stored"], false);
            assert!(out.stdout.is_empty());
            assert_eq!(report["split_frame"]["notify_flags"], 1);
        } else {
            assert_eq!(holder(&out.stdout, 0).split(), raw);
            assert_eq!(report["stored"], true);
            assert_eq!(report["split_frame"]["notify_flags"], 33);
            assert_eq!(report["effects"]["home_invalidated"], 1);
            assert_eq!(report["effects"]["dr_invalidated"], 1);
            let before = Icf::parse(&source).unwrap();
            let after = Icf::parse(&out.stdout).unwrap();
            assert_eq!(
                &before.memory()[0xa21c + 34..0xa21c + 67],
                &after.memory()[0xa21c + 34..0xa21c + 67]
            );
            assert_eq!(after.comment_bytes(), before.comment_bytes());
            assert_eq!(after.extra_data(), before.extra_data());
        }
    }
}

#[test]
fn native_ur_rpt_direct_text_collects_logical_bytes_and_keeps_modal_unconnected() {
    let files = Files::new();
    for is_call in [false, true] {
        let source = if is_call {
            native_call_fixture(16, 145_000_000)
        } else {
            let source = normal_fixture();
            let mut image = Icf::parse(&source).unwrap();
            let mut h = normal_holder(&source, 2);
            h.set_mode(16);
            record::store_channel_image_into(&mut image, ChannelKind::Normal, 2, &mut h).unwrap();
            image.to_bytes()
        };
        let state = files.write(
            "ur-rpt-state",
            &serde_json::to_vec(&initialized_normal_state(&source, 0, None)).unwrap(),
        );
        let prefix = if is_call {
            "record/native-call/accepted/0"
        } else {
            "record/native-memory/accepted/0/2"
        };
        for name in ["your-call", "repeater1", "repeater2"] {
            for (input, right_word, upper, expected) in [
                (json!(""), false, None, b"        "),
                (json!("ab cd"), false, None, b"AB CD   "),
                (json!("ab cd"), true, None, b"AB    CD"),
                (json!(" ab"), true, None, b"      AB"),
                (
                    json!({"action":"text","hex":"8000FF6162"}),
                    false,
                    Some(json!([[-128, 65], [0, 0], [-1, -1], [97, 97], [98, 98]])),
                    b"A  AB   ",
                ),
                (
                    json!({"action":"text","hex":"414243444546474880"}),
                    false,
                    None,
                    b"ABCDEFGH",
                ),
            ] {
                let mut envelope = json!({"input":input,"right_word":right_word});
                if let Some(upper) = upper {
                    envelope["upper"] = upper;
                }
                let assignment = format!("{prefix}/{name}={envelope}");
                let out = cli(
                    &[
                        "edit",
                        "-",
                        "--normal-state",
                        p(&state),
                        "--set",
                        &assignment,
                        "--output",
                        "-",
                    ],
                    &source,
                );
                assert!(
                    out.status.success(),
                    "{assignment}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                let h = if is_call {
                    holder(&out.stdout, 0)
                } else {
                    normal_holder(&out.stdout, 2)
                };
                let which = match name {
                    "your-call" => cs705::icf::MemoryCallsign::Your,
                    "repeater1" => cs705::icf::MemoryCallsign::Repeater1,
                    _ => cs705::icf::MemoryCallsign::Repeater2,
                };
                assert_eq!(h.callsign(which), expected, "{assignment}");
                let before = Icf::parse(&source).unwrap();
                let after = Icf::parse(&out.stdout).unwrap();
                assert_eq!(after.comment_bytes(), before.comment_bytes());
                assert_eq!(after.extra_data(), before.extra_data());
            }
            let cancelled = format!(
                "{prefix}/{name}={}",
                json!({"input":{"action":"cancel"},"right_word":false})
            );
            let out = cli(
                &[
                    "edit",
                    "-",
                    "--normal-state",
                    p(&state),
                    "--set",
                    &cancelled,
                    "--output",
                    "-",
                ],
                &source,
            );
            assert!(out.status.success());
            assert!(out.stdout.is_empty());
            for input in [
                json!({"action":"begin"}),
                json!({"action":"text","hex":"80"}),
            ] {
                let assignment = format!(
                    "{prefix}/{name}={}",
                    json!({"input":input,"right_word":false})
                );
                let target = files.path("ur-rpt-unavailable.icf");
                let out = cli(
                    &[
                        "edit",
                        "-",
                        "--normal-state",
                        p(&state),
                        "--set",
                        &assignment,
                        "--output",
                        p(&target),
                    ],
                    &source,
                );
                assert!(!out.status.success());
                assert!(out.stdout.is_empty());
                assert!(!target.exists());
            }
        }
    }
}
fn context(kind: i32) -> [u8; 24] {
    let mut raw = [0; 24];
    raw[..4].copy_from_slice(&kind.to_le_bytes());
    raw
}
fn native_ranges(files: &Files) -> PathBuf {
    let q = "00".repeat(24);
    let live = format!("10{}", "00".repeat(23));
    files.write(
        "native-ranges.json",
        &serde_json::to_vec(&json!({"generation_q_hex":q,"live_q_hex":live})).unwrap(),
    )
}
fn repeater_fixture() -> Vec<u8> {
    use cs705::repeater::working::{DefaultContext, Working};
    let defaults =
        DefaultContext::from_context(&cs705::icf::context::Context::from_bytes(context(0)), None)
            .unwrap();
    let mut memory = vec![0xa5; 0x4997a];
    memory[0x355a1] = 6;
    memory[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
    memory[0x49914..0x49978].fill(0);
    memory[0x49918..0x4991a].copy_from_slice(&3u16.to_le_bytes());
    for at in [0x392fd, 0x3c8b9, 0x3c903] {
        memory[at..at + 2].copy_from_slice(&u16::MAX.to_le_bytes());
    }
    let mut working = Working::from_working(memory).unwrap();
    for index in 0..3 {
        let mut h = defaults.holder(2, 0);
        h.set_valid(true);
        h.set_name(b"REPEATER NAME   ");
        h.set_channel_field(ChannelField::FrequencyHz, 145_000_000 + index as u32);
        h.set_repeater_text(record::RepeaterText::Call, b"REPEAT A");
        h.set_repeater_text(
            record::RepeaterText::Gateway,
            if index < 2 { b"GATEWAYG" } else { b"OTHER  G" },
        );
        working.save(index, &h).unwrap();
    }
    let mut image = working.to_icf().unwrap();
    image
        .patch_memory(&[(0x3ab82, &[2, 0, 0, 0, 1, 0])])
        .unwrap();
    let bytes = image.to_bytes();
    let source = String::from_utf8(bytes).unwrap();
    let source = source
        .replace("#Comment=\r\n", "#Comment=RECORD META\r\n")
        .replace("#EtcData=00000000", "#EtcData=00000003");
    assert_eq!(
        Icf::parse(source.as_bytes()).unwrap().comment_bytes(),
        b"RECORD META"
    );
    assert_eq!(Icf::parse(source.as_bytes()).unwrap().extra_data(), 3);
    source.into_bytes()
}
fn repeater_holder(bytes: &[u8], index: usize) -> record::DecodedRecord {
    let image = Icf::parse(bytes).unwrap();
    let defaults = cs705::repeater::working::DefaultContext::from_context(
        &cs705::icf::context::Context::from_bytes(context(0)),
        None,
    )
    .unwrap();
    let working = cs705::repeater::working::Working::from_icf(&image, defaults).unwrap();
    let mut h = record::reset_holder(0x0c, index as u32, 0);
    working.load_into(index, &mut h).unwrap();
    h
}
fn ur_fixture() -> Vec<u8> {
    let source = repeater_fixture();
    let mut image = Icf::parse(&source).unwrap();
    let mut order = [255; 600];
    order[..6].copy_from_slice(&[2, 0, 0, 0, 1, 0]);
    image
        .patch_memory(&[(0x3c0a7, &order), (0x3c901, &0x4567u16.to_le_bytes())])
        .unwrap();
    for (index, name, call) in [
        (0, b"WILDCARD NAME   ", b"UR CALL "),
        (1, b"EXACT NAME      ", b"UR CALLA"),
        (2, b"SLASH UR NAME   ", b"/ABCDEFA"),
    ] {
        let mut holder = record::reset_holder(0x0d, index, 0);
        holder.set_name(name);
        holder.set_callsign(cs705::icf::MemoryCallsign::Your, call);
        record::store_callsign_storage_into(&mut image, index as usize, &holder).unwrap();
    }
    image.to_bytes()
}
fn normal_fixture() -> Vec<u8> {
    let mut memory = vec![0xa6; 0x4997a];
    memory[0x355a1] = 6;
    memory[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
    memory[0x3fb40..0x3fb44].copy_from_slice(&2u32.to_le_bytes());
    memory[0x3fb44..0x3fcd4].fill(0);
    for cell in memory[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
        cell.copy_from_slice(&65535u32.to_le_bytes());
    }
    memory[0x3662d..0x36821].fill(1);
    memory[0x392fa] = 0;
    memory[0x392fb..0x392fd].copy_from_slice(&2u16.to_le_bytes());
    memory[0x3c84b] = 0;
    let mut model = cs705::icf::rows::NormalMemory::from_working(memory).unwrap();
    model.allocate(0, 0..3).unwrap();
    for index in 0..6u32 {
        let mut h = record::reset_holder(2, index, 0);
        h.set_name(b"NORMAL SOURCE   ");
        h.set_split(u8::from(index < 2));
        h.set_mode(13);
        h.set_filter(1);
        h.set_offset_hz(149);
        h.set_channel_field(ChannelField::FrequencyHz, 145_000_000 + index);
        model.store_channel(index, &mut h).unwrap();
    }
    String::from_utf8(model.to_icf().unwrap().to_bytes())
        .unwrap()
        .replace("#Comment=\r\n", "#Comment=NATIVE NORMAL\r\n")
        .replace("#EtcData=00000000", "#EtcData=00000003")
        .into_bytes()
}
fn normal_holder(source: &[u8], side: u32) -> record::DecodedRecord {
    let image = Icf::parse(source).unwrap();
    let mut h = record::reset_holder(2, side, 0);
    record::load_channel_image_into(&image, ChannelKind::Normal, side, &mut h).unwrap();
    h
}

fn modal_fixture() -> Vec<u8> {
    let source = normal_fixture();
    let lists = ur_fixture();
    let lists = Icf::parse(&lists).unwrap();
    let calls = native_call_fixture(16, 145_000_000);
    let calls = Icf::parse(&calls).unwrap();
    let mut image = Icf::parse(&source).unwrap();
    image
        .patch_memory(&[
            (0xaa0f, &lists.memory()[0xaa0f..0x355a0]),
            (0x3ab82, &lists.memory()[0x3ab82..0x3c2ff]),
            (0xa21c, &calls.memory()[0xa21c..0xaa0f]),
        ])
        .unwrap();
    let mut h = normal_holder(&source, 2);
    h.set_mode(16);
    h.set_channel_field(ChannelField::Duplex, 0);
    for field in [
        cs705::icf::MemoryCallsign::Your,
        cs705::icf::MemoryCallsign::Repeater1,
        cs705::icf::MemoryCallsign::Repeater2,
    ] {
        h.set_callsign(
            field,
            if field == cs705::icf::MemoryCallsign::Your {
                b"CQCQCQ  "
            } else {
                b"        "
            },
        );
    }
    record::store_channel_image_into(&mut image, ChannelKind::Normal, 2, &mut h).unwrap();
    image.to_bytes()
}

#[test]
fn native_modal_sources_actions_cancel_outer_save_and_options_use_real_collector() {
    use cs705::icf::MemoryCallsign;
    let files = Files::new();
    let source = modal_fixture();
    let config = files.write(
        "options.json",
        br#"{"callsign-dialog":true,"auto-correct":true}"#,
    );
    let list_config = files.write(
        "list-options.json",
        br#"{"callsign-dialog":false,"auto-correct":false}"#,
    );
    let q = files.write("live-q", &context(0));
    let state = initialized_normal_state(&source, 0, None);
    let state = files.write("modal-state", &serde_json::to_vec(&state).unwrap());
    let reads = json!({"ur_name_tail":[32,32,32,32,32,32,32,32],"blank_source_index":2500,"string_tail":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]});
    let caller =
        json!({"code":13,"command":0,"state":0,"blocked":false,"node_index":2,"node_back":0});
    for (path, kind, index) in [
        ("record/native-memory/0/2/repeater1", ChannelKind::Normal, 2),
        ("record/native-call/0/repeater1", ChannelKind::Call, 0),
    ] {
        for result in [1, 2] {
            let edit = json!({"input":{"action":"begin"},"modal":{"modal_result":result,"checkbox":true,
                "actions":[{"action":"ur-text","input":"NEWCALL"}]},"reads":reads});
            let assignment = format!("{path}={}", json!({"edit":edit,"caller":caller}));
            let out = cli(
                &[
                    "--config",
                    p(&config),
                    "edit",
                    "-",
                    "--record-context",
                    p(&q),
                    "--normal-state",
                    p(&state),
                    "--set",
                    &assignment,
                    "--output",
                    "-",
                ],
                &source,
            );
            assert!(
                out.status.success(),
                "{path}/{result}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(!out.stdout.is_empty(), "outer modal Cancel still saves");
            let mut h =
                record::reset_holder(if kind == ChannelKind::Normal { 2 } else { 3 }, index, 0);
            record::load_channel_image_into(&Icf::parse(&out.stdout).unwrap(), kind, index, &mut h)
                .unwrap();
            let mut old = record::reset_holder(h.kind(), index, 0);
            record::load_channel_image_into(&Icf::parse(&source).unwrap(), kind, index, &mut old)
                .unwrap();
            assert_eq!(
                h.callsign(MemoryCallsign::Your),
                if result == 1 {
                    b"NEWCALL "
                } else {
                    old.callsign(MemoryCallsign::Your)
                }
            );
            let reports: Vec<Value> = serde_json::Deserializer::from_slice(&out.stderr)
                .into_iter::<Value>()
                .map(|v| v.unwrap())
                .collect();
            let modal = reports.iter().find(|v| v.get("preview").is_some()).unwrap();
            assert_eq!(modal["stored"], true);
            assert_eq!(modal["accepted"], result == 1);
            assert_eq!(modal["al"], false);
            assert_eq!(modal["preview"]["callsigns"][0]["hex"], "4E455743414C4C20");
        }
    }
    let mut list_caller = caller.clone();
    list_caller["command"] = json!(0xd1);
    for result in [0, 0x100] {
        let assignment = format!(
            "record/native-memory/0/2/your-call={}",
            json!({"edit":{"input":{"action":"begin"},
            "list":{"category":0,"name":0,"call":0,"modal_result":result,"checkbox":false},"reads":reads},"caller":list_caller})
        );
        let out = cli(
            &[
                "--config",
                p(&list_config),
                "edit",
                "-",
                "--record-context",
                p(&q),
                "--normal-state",
                p(&state),
                "--set",
                &assignment,
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
        assert_eq!(out.stdout.is_empty(), result == 0);
        let reports: Vec<Value> = serde_json::Deserializer::from_slice(&out.stderr)
            .into_iter::<Value>()
            .map(|v| v.unwrap())
            .collect();
        let modal = reports.iter().find(|v| v.get("preview").is_some()).unwrap();
        assert_eq!(modal["stored"], result != 0);
        let ids: Vec<_> = modal["effects"]["requested_notifications"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_u64().unwrap())
            .collect();
        assert_eq!(ids, [0x11, 0x39]);
        let tail = reports.iter().find_map(|v| v.get("document_tail")).unwrap();
        assert_eq!(tail["timer_ms"], 250);
        assert_eq!(tail["publication_authorized"], result != 0);
        if result == 0 {
            assert_eq!(tail["dirty"], 0);
        }
    }
    let inline = format!(
        "record/native-memory/0/2/your-call={}",
        json!({"edit":{"input":{"action":"begin"}},"caller":caller})
    );
    let out = cli(
        &[
            "--config",
            p(&list_config),
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            &inline,
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
    assert!(out.stdout.is_empty());
    let reports: Vec<Value> = serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .map(|v| v.unwrap())
        .collect();
    let inline = reports
        .iter()
        .find(|v| v.get("inline_text").is_some())
        .unwrap();
    assert_eq!(inline["state_out"], 2);
    assert_eq!(inline["width"], 8);
    assert_eq!(inline["stored"], false);
    for (config, expected) in [(&config, b"      AB"), (&list_config, b" AB     ")] {
        let assignment = format!(
            "record/native-memory/0/2/your-call={}",
            json!({"input":" ab"})
        );
        let out = cli(
            &[
                "--config",
                p(config),
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--set",
                &assignment,
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
        assert_eq!(
            normal_holder(&out.stdout, 2).callsign(MemoryCallsign::Your),
            expected
        );
    }
    let assignment = format!(
        "record/native-memory/0/2/data={}",
        json!({"edit":{"action":"display"},
        "caller":{"code":0,"command":0,"state":0,"blocked":false,"node_index":2,"node_back":0}})
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            &assignment,
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
    assert!(out.stdout.is_empty());
    let tail = serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .filter_map(std::result::Result::ok)
        .find_map(|v| v.get("document_tail").cloned())
        .unwrap();
    assert_eq!(
        tail,
        json!({"dirty":0,"timer_ms":null,"callback_invoked":false,"publication_authorized":false})
    );
    let hex = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>()
    };
    let ranges = files.write(
        "range-sources.json",
        &serde_json::to_vec(
            &json!({"generation_q_hex":hex(&context(1)),"live_q_hex":hex(&context(4))}),
        )
        .unwrap(),
    );
    let format = files.write(
        "format-source.json",
        br#"{"profile":"portable-ascii","crt_decimal":","}"#,
    );
    let assignment = format!(
        "record/native-memory/0/10/name={}",
        json!({"input":"PENDING","layout":0,"node_flags":164,"node_index":701})
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--native-range-context",
            p(&ranges),
            "--native-format-context",
            p(&format),
            "--set",
            &assignment,
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
    assert_eq!(out.stdout, source);
    let generated = reported_normal_state(&out, "range source creates native editor caches");
    assert_eq!(generated["range_context"], json!(context(1)));
    assert_eq!(generated["mode_cache"].as_array().unwrap().len(), 616);
    assert_eq!(generated["pending"]["bytes"], json!(b"PENDING"));
    let out = cli(
        &[
            "edit",
            "-",
            "--native-range-context",
            p(&ranges),
            "--normal-state",
            p(&state),
            "--set",
            &assignment,
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let out = cli(
        &[
            "edit",
            "-",
            "--native-range-context",
            "-",
            "--native-format-context",
            "-",
            "--set",
            &assignment,
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}
fn generation(files: &Files) -> PathBuf {
    let mut g = vec![0; 0x4997a];
    g[0x36821..0x36823].copy_from_slice(&0x1234u16.to_le_bytes());
    files.write("normalG", &g)
}
fn initialized_normal_state(source: &[u8], selected: u32, langid: Option<u16>) -> Value {
    let input = serde_json::json!({"range_context":context(0),"range_selected":selected,"range_langid":langid,
        "notification":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24]});
    let assignment = format!("record/native-memory/state={input}");
    let out = cli(
        &["edit", "-", "--set", &assignment, "--output", "-"],
        source,
    );
    assert!(out.status.success(), "normal state initialization");
    reported_normal_state(&out, "initialization")
}
fn reported_normal_state(out: &Output, case: &str) -> Value {
    serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .filter_map(std::result::Result::ok)
        .filter_map(|v| v.get("normal_state").cloned())
        .last()
        .unwrap_or_else(|| panic!("native state report: {case}"))
}
fn split_assignment(input: Value, layout: u32, step: i32) -> String {
    format!(
        "record/native-memory/0/2/split={}",
        json!({"input":input,"frame":{
        "layout":layout,"pre_step":step,"selection_first":12,"node_index":47,"node_flags":0xa5,
        "notify_flags":1,"active":62,"display_mode":7}})
    )
}
fn split_report(out: &Output, case: &str) -> Value {
    serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .filter_map(std::result::Result::ok)
        .filter(|v| v.get("split_frame").is_some())
        .last()
        .unwrap_or_else(|| panic!("split report: {case}"))
}
fn split_nonidentity_fixture() -> Vec<u8> {
    let source = normal_fixture();
    let image = Icf::parse(&source).unwrap();
    let mut model = cs705::icf::rows::NormalMemory::from_icf(&image).unwrap();
    assert!(model.carry(0, 1..2, 1).unwrap());
    model.to_icf().unwrap().to_bytes()
}

#[test]
fn native_split_editor_consumes_real_state_and_preserves_other_side_metadata() {
    let files = Files::new();
    let source = split_nonidentity_fixture();
    let before_image = Icf::parse(&source).unwrap();
    let before = cs705::icf::rows::NormalMemory::from_icf(&before_image).unwrap();
    assert_eq!(before.slot(0, 1).unwrap(), Some(2));
    let other = before.record(0, 1).unwrap().unwrap()[34..67].to_vec();
    let state = files.write(
        "split-initialized",
        &serde_json::to_vec(&initialized_normal_state(&source, 0, None)).unwrap(),
    );
    let assignment = split_assignment(json!(1), 0, 1);
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            &assignment,
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success(), "split on");
    let report = split_report(&out, "split on");
    assert_eq!(report["stored"], true);
    assert_eq!(normal_holder(&out.stdout, 2).split(), 1);
    assert_eq!(report["split_frame"]["active"], 47);
    assert_eq!(report["split_frame"]["display_mode"], 2);
    assert_eq!(report["split_frame"]["notify_flags"], 0x21);
    assert_eq!(report["split_frame"]["node_flags"], 0xa4);
    assert_eq!(report["effects"]["requested_notifications"][0]["id"], 0x45);
    assert_eq!(
        report["effects"]["requested_notifications"][0]["synchronous"],
        true
    );
    assert_eq!(report["effects"]["requested_notifications"][1]["id"], 0x3c);
    assert_eq!(
        report["effects"]["requested_notifications"][1]["synchronous"],
        false
    );
    let image = Icf::parse(&out.stdout).unwrap();
    let after = cs705::icf::rows::NormalMemory::from_icf(&image).unwrap();
    assert_eq!(after.slot(0, 1).unwrap(), Some(2));
    assert_eq!(&after.record(0, 1).unwrap().unwrap()[34..67], other);
    assert_eq!(after.used_slots(), 3);
    assert_eq!(image.comment_bytes(), b"NATIVE NORMAL");
    assert_eq!(image.extra_data(), 3);
    let state = files.write(
        "split-after-on",
        &serde_json::to_vec(&report["normal_state"]).unwrap(),
    );
    let assignment = split_assignment(json!(1), 0, 2);
    let same = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            &assignment,
            "--output",
            "-",
        ],
        &out.stdout,
    );
    assert!(same.status.success(), "same split accepted");
    let report = split_report(&same, "same split");
    assert_eq!(report["stored"], true);
    assert_eq!(
        report["effects"]["requested_notifications"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(report["effects"]["requested_notifications"][0]["id"], 0x3c);
    assert_eq!(report["split_frame"]["active"], 62);
    let assignment = split_assignment(json!({"action":"next"}), 0, 1);
    let off = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            &assignment,
            "--output",
            "-",
        ],
        &out.stdout,
    );
    assert!(off.status.success(), "split keyboard next");
    let report = split_report(&off, "split off");
    assert_eq!(normal_holder(&off.stdout, 2).split(), 0);
    assert_eq!(report["candidate"], 0);
    assert_eq!(report["effects"]["requested_notifications"][0]["id"], 0x46);
    assert_eq!(report["split_frame"]["active"], 12);
}

#[test]
fn native_split_full_candidate_layout_and_false_editor_paths_are_distinct() {
    let files = Files::new();
    let source = split_nonidentity_fixture();
    let state = initialized_normal_state(&source, 0, None);
    let state_file = files.write("split-state", &serde_json::to_vec(&state).unwrap());
    for (input, layout, raw, notice, active) in
        [(json!(256), 0, 0, 0x45, 47), (json!(1), 2, 1, 0x3c, 62)]
    {
        let assignment = split_assignment(input, layout, 1);
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state_file),
                "--set",
                &assignment,
                "--output",
                "-",
            ],
            &source,
        );
        assert!(out.status.success(), "full split candidate/layout");
        let report = split_report(&out, "full split candidate/layout");
        assert_eq!(normal_holder(&out.stdout, 2).split(), raw);
        assert_eq!(
            report["effects"]["requested_notifications"][0]["id"],
            notice
        );
        assert_eq!(report["split_frame"]["active"], active);
    }
    for input in [
        json!({"action":"cancel"}),
        json!({"action":"display"}),
        json!({"action":"begin"}),
        json!({"action":"selected","key":2,"phase":2,"candidate":1,"accepted":true}),
        json!({"action":"selected","key":1,"phase":1,"candidate":1,"accepted":true}),
        json!({"action":"selected","key":1,"phase":2,"candidate":1,"accepted":false}),
    ] {
        let assignment = split_assignment(input.clone(), 0, 1);
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state_file),
                "--set",
                &assignment,
                "--output",
                "-",
            ],
            &source,
        );
        assert!(out.status.success(), "false split editor");
        let report = split_report(&out, "false split editor");
        assert_eq!(report["stored"], false);
        assert!(out.stdout.is_empty());
        assert_eq!(report["normal_state"], state);
        assert_eq!(report["split_frame"]["node_flags"], 0xa4);
        assert_eq!(report["split_frame"]["notify_flags"], 1);
        assert!(
            report["effects"]["requested_notifications"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(report["popup_requested"], input["action"] == "begin");
    }
}

#[test]
fn native_split_requires_actual_frame_and_supports_same_process_initialization() {
    let files = Files::new();
    let source = split_nonidentity_fixture();
    let init = format!(
        "record/native-memory/state={}",
        json!({"range_context":context(0),"range_selected":0,"range_langid":null,"notification":null})
    );
    let split = split_assignment(json!({"action":"previous"}), 0, 1);
    let out = cli(
        &[
            "edit", "-", "--set", &init, "--set", &split, "--output", "-",
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "initialize then split in same process"
    );
    assert_eq!(normal_holder(&out.stdout, 2).split(), 1);
    let state = files.write(
        "split-state",
        &serde_json::to_vec(&reported_normal_state(&out, "live split state")).unwrap(),
    );
    let bad = format!(
        "record/native-memory/0/2/split={}",
        json!({"input":1,"frame":{"layout":0}})
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            &bad,
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            "record/native-memory/0/2/split=1",
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    let out = cli(&["edit", "-", "--set", &split, "--output", "-"], &source);
    assert_eq!(out.status.code(), Some(6));
    assert!(out.stdout.is_empty());
}

#[test]
fn native_state_restore_and_constructor_or_duplicate_init_fail_before_publication() {
    let files = Files::new();
    let source = normal_fixture();
    let g = generation(&files);
    let mut notification: [u8; 24] = std::array::from_fn(|i| i as u8);
    notification[8..12].copy_from_slice(&0x105u32.to_le_bytes());
    notification[20..24].copy_from_slice(&2u32.to_le_bytes());
    let init = format!(
        "record/native-memory/state={}",
        json!({"range_context":context(0),"range_selected":0,"range_langid":null,"notification":notification})
    );
    let pending = cli(
        &[
            "edit",
            "-",
            "--set",
            &init,
            "--set",
            "record/native-memory/accepted/0/10/name=KEEP",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(
        pending.status.success(),
        "actual initialization then pending KEEP"
    );
    let state = reported_normal_state(&pending, "pending KEEP");
    assert_eq!(state["pending"]["active"], true);
    assert_eq!(state["pending"]["index"], 10);
    assert_eq!(state["pending"]["bytes"], json!(b"KEEP"));
    let bytes = serde_json::to_vec(&state).unwrap();
    let state_file = files.write("pending-keep", &bytes);
    let target = files.path("exclusive-output.icf");
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state_file),
            "--generation",
            p(&g),
            "--set",
            &init,
            "--set",
            "record/native-memory/accepted/0/10/frequency-hz=146000000",
            "--output",
            p(&target),
        ],
        &pending.stdout,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    assert!(!target.exists());
    assert_eq!(fs::read(&state_file).unwrap(), bytes);
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            &init,
            "--set",
            &init,
            "--output",
            p(&target),
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    assert!(!target.exists());
    let reports = serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .map(|v| v.expect("native state/error JSON"))
        .filter(|v| v.get("normal_state").is_some())
        .count();
    assert!(
        reports <= 1,
        "duplicate constructor must not emit a replacement state"
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state_file),
            "--generation",
            p(&g),
            "--set",
            "record/native-memory/accepted/0/10/frequency-hz=146000000",
            "--set",
            "record/native-memory/accepted/0/10/mode=1",
            "--set",
            "record/native-memory/0/10/data=1",
            "--set",
            "record/native-memory/0/10/filter=2",
            "--output",
            p(&target),
        ],
        &pending.stdout,
    );
    assert!(
        out.status.success(),
        "restore alone and same live state across edits"
    );
    let output = fs::read(&target).unwrap();
    let h = normal_holder(&output, 10);
    assert_eq!(h.name(), b"KEEP            ");
    assert_eq!(h.channel_field(ChannelField::FrequencyHz), 146_000_000);
    assert_eq!((h.mode(), h.data(), h.filter()), (1, 1, 2));
    assert_eq!(
        reported_normal_state(&out, "restored KEEP registration")["pending"]["active"],
        false
    );
}

#[test]
fn native_empty_and_upper_frequency_redirect_skip_sibling_editor_and_publication() {
    let files = Files::new();
    let source = normal_fixture();
    let caller =
        json!({"code":0x7100,"command":0,"state":0,"blocked":false,"node_index":701,"node_back":2});
    let assignment = format!(
        "record/native-memory/0/10/mode={}",
        json!({"edit":{"input":{"action":"begin"},"warning_bits":0},"caller":caller})
    );
    let out = cli(
        &["edit", "-", "--set", &assignment, "--output", "-"],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty());
    let report = serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .filter_map(std::result::Result::ok)
        .find(|v| v.get("redirect").is_some())
        .unwrap();
    assert_eq!(
        report["redirect"],
        json!({"key":0x107,"index":699,"code":0x7100})
    );
    let tail = serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .filter_map(std::result::Result::ok)
        .find_map(|v| v.get("document_tail").cloned())
        .unwrap();
    assert_eq!(
        tail,
        json!({"dirty":0,"timer_ms":250,"callback_invoked":false,"publication_authorized":false})
    );
    let state = initialized_normal_state(&source, 0, None);
    let state = files.write("cancel-tail-state", &serde_json::to_vec(&state).unwrap());
    let cancel = format!(
        "record/native-memory/0/2/data={}",
        json!({"edit":{"action":"cancel"},
        "caller":{"code":27,"command":0,"state":0,"blocked":false,"node_index":2,"node_back":0}})
    );
    let cancelled = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            &cancel,
            "--output",
            "-",
        ],
        &source,
    );
    assert!(
        cancelled.status.success(),
        "{}",
        String::from_utf8_lossy(&cancelled.stderr)
    );
    assert!(cancelled.stdout.is_empty());
    let tail = serde_json::Deserializer::from_slice(&cancelled.stderr)
        .into_iter::<Value>()
        .filter_map(std::result::Result::ok)
        .find_map(|v| v.get("document_tail").cloned())
        .unwrap();
    assert_eq!(
        tail,
        json!({"dirty":0,"timer_ms":250,"callback_invoked":false,"publication_authorized":false})
    );
    let source = native_call_fixture(16, 145_000_000);
    let mut image = Icf::parse(&source).unwrap();
    let mut h = holder(&source, 0);
    h.set_channel_field(ChannelField::FrequencyHz, 470_000_001);
    record::store_channel_image_into(&mut image, ChannelKind::Call, 0, &mut h).unwrap();
    let source = image.to_bytes();
    let assignment = format!(
        "record/native-call/0/data={}",
        json!({"edit":1,"caller":caller})
    );
    let out = cli(
        &["edit", "-", "--set", &assignment, "--output", "-"],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty());
}

#[test]
fn native_name_notification_is_produced_from_live_node_without_a_client_packet() {
    let source = normal_fixture();
    let init = format!(
        "record/native-memory/state={}",
        json!({"range_context":context(0),"range_selected":0,"range_langid":null})
    );
    let name = format!(
        "record/native-memory/0/10/name={}",
        json!({"input":"PENDING","layout":0,"node_flags":164,"node_index":701})
    );
    let out = cli(
        &["edit", "-", "--set", &init, "--set", &name, "--output", "-"],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, source);
    let reports: Vec<Value> = serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .map(|v| v.unwrap())
        .collect();
    let report = reports
        .iter()
        .find(|v| v.get("name_frame").is_some())
        .unwrap();
    let expected: Vec<u8> = [3u32, 701, 0x105, 0, 0, 2]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    assert_eq!(report["requested_pending"]["payload"], json!(expected));
    assert_eq!(report["normal_state"]["pending"]["index"], 10);
    assert_eq!(
        report["normal_state"]["pending"]["bytes"],
        json!(b"PENDING")
    );
    assert!(!normal_holder(&out.stdout, 10).valid());
}

#[test]
fn native_name_collector_uses_shared_text_hex_nls_and_false_tail() {
    let files = Files::new();
    let source = normal_fixture();
    let state = initialized_normal_state(&source, 0, None);
    let state = files.write("name-state", &serde_json::to_vec(&state).unwrap());
    for (input, expected) in [
        (
            json!({"action":"text","hex":"41005441494C2020"}),
            b"A TAIL          ".as_slice(),
        ),
        (
            json!({"action":"text","hex":"2082A058814020"}),
            b" \x82\xa0X            ".as_slice(),
        ),
        (
            json!({"action":"text","hex":"31323334353637383930313233343582A058"}),
            b"123456789012345 ".as_slice(),
        ),
        (json!(""), b"                ".as_slice()),
    ] {
        let assignment = format!(
            "record/native-memory/0/2/name={}",
            json!({"input":input,"layout":0,"node_flags":165})
        );
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--set",
                &assignment,
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
        assert_eq!(normal_holder(&out.stdout, 2).name(), expected);
        let report = serde_json::Deserializer::from_slice(&out.stderr)
            .into_iter::<Value>()
            .filter_map(std::result::Result::ok)
            .find(|v| v.get("name_frame").is_some())
            .unwrap();
        assert_eq!(report["stored"], true);
        assert_eq!(report["name_frame"]["node_flags"], 164);
    }
    for (input, state_out) in [
        (json!({"action":"cancel"}), 0),
        (json!({"action":"begin","first":b'X'}), 2),
    ] {
        let assignment = format!(
            "record/native-memory/0/2/name={}",
            json!({"input":input,"layout":1,"node_flags":164})
        );
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--set",
                &assignment,
                "--output",
                "-",
            ],
            &source,
        );
        assert!(out.status.success());
        assert!(out.stdout.is_empty());
        let report = serde_json::Deserializer::from_slice(&out.stderr)
            .into_iter::<Value>()
            .filter_map(std::result::Result::ok)
            .find(|v| v.get("name_frame").is_some())
            .unwrap();
        assert_eq!(report["stored"], false);
        assert_eq!(report["state_out"], state_out);
        assert_eq!(report["name_frame"]["node_flags"], 165);
        assert_eq!(
            report["normal_state"],
            serde_json::from_slice::<Value>(&fs::read(&state).unwrap()).unwrap()
        );
    }
}

#[test]
fn native_frequency_collector_consumes_real_pending_text_two_pass_and_live_sources() {
    let files = Files::new();
    let source = normal_fixture();
    let state = initialized_normal_state(&source, 1, None);
    let state = files.write("frequency-state", &serde_json::to_vec(&state).unwrap());
    let name = format!(
        "record/native-memory/0/10/name={}",
        json!({"input":{"action":"text","hex":"50454E0044494E472020"},"layout":0,"node_flags":164,"node_index":10})
    );
    let pending = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            &name,
            "--output",
            "-",
        ],
        &source,
    );
    assert!(pending.status.success());
    let state = reported_normal_state(&pending, "native pending producer");
    assert_eq!(state["pending"]["bytes"], json!(b"PEN DING"));
    let state = files.write("frequency-pending", &serde_json::to_vec(&state).unwrap());
    let q = files.write("live-Q", &context(4));
    let g = generation(&files);
    let assignment = format!(
        "record/native-memory/0/10/frequency-hz={}",
        json!({"input":{"action":"text","text":"146.520000"},"warning_bits":129,"focus":0})
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--record-context",
            p(&q),
            "--generation",
            p(&g),
            "--set",
            &assignment,
            "--output",
            "-",
        ],
        &pending.stdout,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for side in [10, 11] {
        let h = normal_holder(&out.stdout, side);
        assert_eq!(h.channel_field(ChannelField::FrequencyHz), 146_520_000);
        assert_eq!(h.offset_hz(), 600_000);
        assert_eq!(h.name(), b"PEN DING        ");
    }
    let report = serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .filter_map(std::result::Result::ok)
        .find(|v| v.get("first").is_some())
        .unwrap();
    assert_eq!(report["first"]["accepted"], true);
    assert_eq!(report["second"]["accepted"], true);
    assert_eq!(report["notify_flags"], 24);
    assert_eq!(report["warning_bits"].as_u64().unwrap() & 129, 129);
    assert_eq!(report["normal_state"]["pending"]["active"], false);
    let begin = format!(
        "record/native-memory/0/10/frequency-hz={}",
        json!({"input":{"action":"begin"},"warning_bits":129,"focus":0})
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--record-context",
            p(&q),
            "--set",
            &begin,
            "--output",
            "-",
        ],
        &pending.stdout,
    );
    assert!(out.status.success());
    let report = serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .filter_map(std::result::Result::ok)
        .find(|v| v.get("first").is_some())
        .unwrap();
    assert_eq!(report["stored"], false);
    assert_eq!(report["focus"], 1);
    assert_eq!(report["first"]["state_out"], 2);
    assert_eq!(report["normal_state"]["pending"]["active"], true);
}

#[test]
fn native_numeric_collectors_preserve_bare_lexical_input_and_refuse_missing_sources() {
    let files = Files::new();
    let source = normal_fixture();
    let state = initialized_normal_state(&source, 0, None);
    let state = files.write("number-state", &serde_json::to_vec(&state).unwrap());
    let q = files.write("live-Q", &context(0));
    let g = generation(&files);
    for (input, expected) in [
        ("1e-3", 1_000_000),
        ("0.00015", 200),
        ("4.0000005", 4_000_000),
    ] {
        let assignment = format!("record/native-memory/0/2/offset-hz={input}");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--set",
                &assignment,
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
        assert_eq!(normal_holder(&out.stdout, 2).offset_hz(), expected);
    }
    let assignment = format!(
        "record/native-memory/0/2/frequency-hz={}",
        json!({"input":"1e-3","warning_bits":0,"focus":0})
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--record-context",
            p(&q),
            "--generation",
            p(&g),
            "--set",
            &assignment,
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success());
    assert_eq!(
        normal_holder(&out.stdout, 2).channel_field(ChannelField::FrequencyHz),
        1_000_000
    );
    let target = files.path("no-frequency-source.icf");
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--generation",
            p(&g),
            "--set",
            &assignment,
            "--output",
            p(&target),
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(6));
    assert!(out.stdout.is_empty());
    assert!(!target.exists());
    let assignment = format!(
        "record/native-memory/0/10/frequency-hz={}",
        json!({"input":"146.52","warning_bits":0,"focus":0})
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--record-context",
            p(&q),
            "--set",
            &assignment,
            "--output",
            p(&target),
        ],
        &source,
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(!target.exists());
    for assignment in [
        "record/native-memory/0/2/name=RAW",
        "record/native-memory/0/2/frequency-hz=146520000",
    ] {
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--set",
                assignment,
                "--output",
                p(&target),
            ],
            &source,
        );
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
        assert!(!target.exists());
    }
}

#[test]
fn native_tone_collectors_use_whole_text_resource_order_and_actual_q_profile() {
    let files = Files::new();
    let source = normal_fixture();
    let state = initialized_normal_state(&source, 0, None);
    let state = files.write("tone-state", &serde_json::to_vec(&state).unwrap());
    let q0 = files.write("tone-Q0", &context(0));
    let mut q = context(0);
    q[0x12] = 1;
    let q1 = files.write("tone-Q1", &q);
    for (name, input, q, expected) in [
        ("tx-tone", "1e2", &q0, 12),
        ("rx-tone", "0000000000088.5", &q0, 8),
        ("rx-tone", "150", &q0, 50),
        ("rx-tone", "150", &q1, 24),
        ("dtcs", "123.5", &q0, 20),
        ("dtcs", "24", &q0, 0),
        (
            "dtcs",
            r#"{"action":"selected","key":1,"phase":2,"candidate":355,"accepted":true}"#,
            &q0,
            99,
        ),
        (
            "tone",
            r#"{"action":"selected","key":1,"phase":2,"candidate":260,"accepted":true}"#,
            &q0,
            4,
        ),
    ] {
        let assignment = format!("record/native-memory/0/2/{name}={input}");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--record-context",
                p(q),
                "--set",
                &assignment,
                "--output",
                "-",
            ],
            &source,
        );
        assert!(
            out.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let h = normal_holder(&out.stdout, 2);
        let actual = match name {
            "tx-tone" => h.tx_tone(),
            "rx-tone" => h.rx_tone(),
            "tone" => h.channel_field(ChannelField::Tone),
            _ => h.channel_field(ChannelField::Dtcs),
        };
        assert_eq!(actual, expected);
        let image = Icf::parse(&out.stdout).unwrap();
        assert_eq!(image.comment_bytes(), b"NATIVE NORMAL");
        assert_eq!(image.extra_data(), 3);
    }
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            "record/native-memory/0/2/dtcs=123.5",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success());
    assert_eq!(
        normal_holder(&out.stdout, 2).channel_field(ChannelField::Dtcs),
        20
    );
}

#[test]
fn native_mode_data_filter_collectors_use_actual_key_catalog_and_cache_tail() {
    let files = Files::new();
    let source = normal_fixture();
    let state = initialized_normal_state(&source, 0, None);
    let state = files.write("enum-state", &serde_json::to_vec(&state).unwrap());
    for (name, input, expected) in [
        (
            "mode",
            json!({"input":{"action":"next"},"warning_bits":129}),
            16,
        ),
        (
            "mode",
            json!({"input":{"action":"selected","key":7,"phase":2,"candidate":262,"accepted":true},"warning_bits":129}),
            6,
        ),
        ("data", json!({"action":"next"}), 1),
        ("filter", json!({"action":"next"}), 2),
        ("filter", json!(1), 1),
    ] {
        let assignment = format!("record/native-memory/0/2/{name}={input}");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--set",
                &assignment,
                "--output",
                "-",
            ],
            &source,
        );
        assert!(
            out.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let h = normal_holder(&out.stdout, 2);
        assert_eq!(
            match name {
                "mode" => h.mode(),
                "data" => h.data(),
                _ => h.filter(),
            },
            expected
        );
        let report = serde_json::Deserializer::from_slice(&out.stderr)
            .into_iter::<Value>()
            .filter_map(std::result::Result::ok)
            .find(|v| v.get("enum_key").is_some())
            .unwrap();
        assert_eq!(report["stored"], true);
        if name == "mode" {
            assert_eq!(report["warning_bits"], 129);
            assert_eq!(report["effects"]["requested_notifications"][0]["id"], 60);
        } else {
            assert!(
                report["effects"]["requested_notifications"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }
        if name == "filter" {
            assert_eq!(report["choice_values"], json!([0, 1, 2]));
        } else {
            assert_eq!(report["context_state"], 2);
        }
    }
}

#[test]
fn native_enum_false_mode_key_and_ignored_filter_selector_are_not_raw_setters() {
    let files = Files::new();
    let source = normal_fixture();
    let state = initialized_normal_state(&source, 0, None);
    let state = files.write("enum-state", &serde_json::to_vec(&state).unwrap());
    let assignment = format!(
        "record/native-memory/0/2/mode={}",
        json!({"input":{"action":"selected","key":1,"phase":2,"candidate":6,"accepted":true},"warning_bits":133})
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            &assignment,
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert_eq!(normal_holder(&source, 2).mode(), 13);
    let report = serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .filter_map(std::result::Result::ok)
        .find(|v| v.get("enum_key").is_some())
        .unwrap();
    assert_eq!(report["stored"], false);
    assert_eq!(report["context_state"], 2);
    assert_eq!(report["warning_bits"], 133);
    for (name, mode, input, popup) in [
        ("data", 16, json!({"action":"begin"}), false),
        ("filter", 15, json!({"action":"begin"}), true),
    ] {
        let mut image = Icf::parse(&source).unwrap();
        let mut h = normal_holder(&source, 2);
        h.set_mode(mode);
        record::store_channel_image_into(&mut image, ChannelKind::Normal, 2, &mut h).unwrap();
        let source = image.to_bytes();
        let assignment = format!("record/native-memory/0/2/{name}={input}");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--set",
                &assignment,
                "--output",
                "-",
            ],
            &source,
        );
        assert!(out.status.success());
        assert!(out.stdout.is_empty());
        assert_eq!(normal_holder(&source, 2).mode(), mode);
        let report = serde_json::Deserializer::from_slice(&out.stderr)
            .into_iter::<Value>()
            .filter_map(std::result::Result::ok)
            .find(|v| v.get("enum_key").is_some())
            .unwrap();
        assert_eq!(report["stored"], false);
        assert_eq!(report["mode_blocked"], true);
        assert_eq!(report["popup_requested"], popup);
    }
}

#[test]
fn native_tone_false_popup_inline_and_missing_source_paths_are_not_accepted_cutpoints() {
    let files = Files::new();
    let source = normal_fixture();
    let state = initialized_normal_state(&source, 0, None);
    let state = files.write("tone-state", &serde_json::to_vec(&state).unwrap());
    let q = files.write("tone-Q", &context(0));
    for (name, input, popup, state_out, inline) in [
        ("tone", json!("4"), false, 0, None),
        ("tone", json!({"action":"cancel"}), false, 0, None),
        ("tx-tone", json!({"action":"begin"}), true, 0, None),
        (
            "tx-tone",
            json!({"input":{"action":"begin"},"inline":true}),
            false,
            2,
            Some("38382E35"),
        ),
        (
            "tx-tone",
            json!({"input":{"action":"begin"},"inline":true,"first":b'7'}),
            false,
            2,
            Some("37"),
        ),
    ] {
        let assignment = format!("record/native-memory/0/2/{name}={input}");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--record-context",
                p(&q),
                "--set",
                &assignment,
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
        assert!(out.stdout.is_empty());
        let report = serde_json::Deserializer::from_slice(&out.stderr)
            .into_iter::<Value>()
            .filter_map(std::result::Result::ok)
            .find(|v| v.get("tone_key").is_some())
            .unwrap();
        assert_eq!(report["stored"], false);
        assert_eq!(report["popup_requested"], popup);
        assert_eq!(report["state_out"], state_out);
        assert_eq!(
            report["normal_state"],
            serde_json::from_slice::<Value>(&fs::read(&state).unwrap()).unwrap()
        );
        assert_eq!(
            report["inline_text"].get("hex").and_then(Value::as_str),
            inline
        );
    }
    let target = files.path("missing-tone-source.icf");
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            "record/native-memory/0/2/tx-tone=88.5",
            "--output",
            p(&target),
        ],
        &source,
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(!target.exists());
    for assignment in [
        r#"record/native-memory/0/2/tx-tone={"input":1e-3}"#,
        r#"record/native-memory/0/2/frequency-hz={"input":1e-3,"warning_bits":0,"focus":0}"#,
    ] {
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--record-context",
                p(&q),
                "--set",
                assignment,
                "--output",
                p(&target),
            ],
            &source,
        );
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
        assert!(!target.exists());
    }
}

fn choice_report(out: &Output) -> Value {
    serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .filter_map(std::result::Result::ok)
        .find(|v| v.get("choice_values").is_some() && v.get("mode_blocked").is_some())
        .expect("native choice report")
}

#[test]
fn canonical_publication_requires_authorization_but_keeps_same_value_and_or_order() {
    let files = Files::new();
    let source = normal_fixture();
    let state = initialized_normal_state(&source, 0, None);
    let state = files.write("publication-state", &serde_json::to_vec(&state).unwrap());
    let q = files.write("publication-Q", &context(0));
    let cancelled = "record/native-memory/0/2/duplex={\"action\":\"cancel\"}";
    for (field, input) in [
        ("duplex", json!({"action":"cancel"})),
        (
            "selection",
            json!({"input":{"action":"cancel"},"layout":1,"node_flags":164}),
        ),
        (
            "mode",
            json!({"input":{"action":"selected","key":1,"phase":2,"candidate":6,"accepted":true},"warning_bits":129}),
        ),
        ("data", json!({"action":"cancel"})),
        ("filter", json!({"action":"cancel"})),
        ("offset-hz", json!({"action":"cancel"})),
        ("tone", json!({"action":"cancel"})),
        ("tx-tone", json!({"action":"begin"})),
        ("rx-tone", json!({"action":"cancel"})),
        ("dtcs", json!({"action":"cancel"})),
        ("polarity", json!({"action":"cancel"})),
        ("digital-squelch", json!({"action":"next"})),
        (
            "digital-code",
            json!({"input":{"action":"cancel"},"node_flags":165}),
        ),
        (
            "name",
            json!({"input":{"action":"cancel"},"layout":1,"node_flags":164}),
        ),
        (
            "frequency-hz",
            json!({"input":{"action":"begin"},"warning_bits":129,"focus":0}),
        ),
    ] {
        let assignment = format!("record/native-memory/0/2/{field}={input}");
        let target = files.path(&format!("declined-{field}.icf"));
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--record-context",
                p(&q),
                "--set",
                &assignment,
                "--output",
                p(&target),
            ],
            &source,
        );
        assert!(out.status.success(), "{field}");
        assert!(out.stdout.is_empty());
        assert!(!target.exists(), "{field} cannot authorize publication");
    }
    let split = split_assignment(json!({"action":"cancel"}), 0, 1);
    let target = files.path("declined-split.icf");
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            &split,
            "--output",
            p(&target),
        ],
        &source,
    );
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(!target.exists());
    assert_eq!(
        split_report(&out, "split false prefix")["split_frame"]["node_flags"],
        164
    );
    for accepted in [
        "record/native-memory/0/2/duplex=0",
        "record/memory/0/2/duplex=0",
    ] {
        for reverse in [false, true] {
            let pair = if reverse {
                [accepted, cancelled]
            } else {
                [cancelled, accepted]
            };
            let out = cli(
                &[
                    "edit",
                    "-",
                    "--normal-state",
                    p(&state),
                    "--set",
                    pair[0],
                    "--set",
                    pair[1],
                    "--output",
                    "-",
                ],
                &source,
            );
            assert!(out.status.success());
            assert!(!out.stdout.is_empty());
            assert_eq!(
                Icf::parse(&out.stdout).unwrap().memory(),
                Icf::parse(&source).unwrap().memory()
            );
        }
    }
    let raw = Icf::parse(&source).unwrap().memory().to_vec();
    let out = cli(
        &[
            "edit",
            "-",
            "--kind",
            "memory",
            "--normal-state",
            p(&state),
            "--set",
            cancelled,
            "--output",
            "-",
        ],
        &raw,
    );
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let out = cli(
        &[
            "edit",
            "-",
            "--kind",
            "memory",
            "--normal-state",
            p(&state),
            "--set",
            "record/native-memory/0/2/duplex=0",
            "--output",
            "-",
        ],
        &raw,
    );
    assert!(out.status.success());
    assert_eq!(out.stdout, raw);
    let pending = format!(
        "record/native-memory/0/10/name={}",
        json!({"input":"PENDING","layout":0,"node_flags":164,"node_index":701})
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            &pending,
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success());
    assert!(!out.stdout.is_empty());
    assert_eq!(
        reported_normal_state(&out, "pending authorization")["pending"]["active"],
        true
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            "record/native-memory/accepted/0/2/mode=24",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success());
    assert!(!out.stdout.is_empty());
}

#[test]
fn canonical_full_capacity_false_pending_prefix_cannot_authorize_publication() {
    let files = Files::new();
    let source = normal_fixture();
    let mut image = Icf::parse(&source).unwrap();
    let mut w = cs705::icf::rows::NormalMemory::from_icf(&image)
        .unwrap()
        .working()
        .to_vec();
    w[0x3fb40..0x3fb44].copy_from_slice(&6u32.to_le_bytes());
    let mut full = cs705::icf::rows::NormalMemory::from_working(w).unwrap();
    for group in 0..5 {
        full.allocate(group, 0..100).unwrap();
    }
    image
        .patch_memory(&[(0, full.to_icf().unwrap().memory())])
        .unwrap();
    let source = image.to_bytes();
    let mut state = initialized_normal_state(&source, 0, None);
    state["pending"] = json!({"active":true,"index":1000,"bytes":b"KEEP"});
    let state = files.write("full-pending-state", &serde_json::to_vec(&state).unwrap());
    let q = files.write("full-Q", &context(0));
    for (field, input) in [
        (
            "name",
            json!({"input":"NO SPACE","layout":0,"node_flags":164}),
        ),
        (
            "frequency-hz",
            json!({"input":"146.52","warning_bits":0,"focus":0}),
        ),
    ] {
        let assignment = format!("record/native-memory/5/0/{field}={input}");
        let target = files.path(&format!("full-{field}.icf"));
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--record-context",
                p(&q),
                "--set",
                &assignment,
                "--output",
                p(&target),
            ],
            &source,
        );
        assert!(
            out.status.success(),
            "{field}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
        assert!(!target.exists());
        assert_eq!(
            reported_normal_state(&out, "capacity false prefix")["pending"]["active"],
            false
        );
    }
}

#[test]
fn native_choice_collectors_use_shared_grammar_full_candidates_and_two_savers() {
    let files = Files::new();
    for (name, mode, input, field, expected) in [
        (
            "selection",
            13,
            json!({"input":258,"layout":0,"node_flags":165}),
            ChannelField::Selection,
            2,
        ),
        (
            "duplex",
            13,
            json!({"action":"next"}),
            ChannelField::Duplex,
            1,
        ),
        ("duplex", 17, json!(260), ChannelField::Duplex, 0),
        ("polarity", 13, json!(259), ChannelField::Polarity, 2),
        (
            "polarity",
            13,
            json!({"action":"previous"}),
            ChannelField::Polarity,
            5,
        ),
        (
            "digital-squelch",
            16,
            json!({"action":"next"}),
            ChannelField::DigitalSquelch,
            2,
        ),
        (
            "digital-squelch",
            16,
            json!(258),
            ChannelField::DigitalSquelch,
            2,
        ),
    ] {
        let source = normal_fixture();
        let mut image = Icf::parse(&source).unwrap();
        let mut h = normal_holder(&source, 2);
        h.set_mode(mode);
        h.set_channel_field(
            field,
            if field == ChannelField::Polarity {
                2
            } else {
                0
            },
        );
        record::store_channel_image_into(&mut image, ChannelKind::Normal, 2, &mut h).unwrap();
        let source = image.to_bytes();
        let state = initialized_normal_state(&source, 0, None);
        let state = files.write("choice-state", &serde_json::to_vec(&state).unwrap());
        let assignment = format!("record/native-memory/0/2/{name}=  {input}");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--set",
                &assignment,
                "--output",
                "-",
            ],
            &source,
        );
        assert!(
            out.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let report = choice_report(&out);
        assert_eq!(report["stored"], true);
        assert_eq!(report["mode_blocked"], false);
        assert_eq!(normal_holder(&out.stdout, 2).channel_field(field), expected);
        let after = Icf::parse(&out.stdout).unwrap();
        assert_eq!(after.comment_bytes(), b"NATIVE NORMAL");
        assert_eq!(after.extra_data(), 3);
        let before = cs705::icf::rows::NormalMemory::from_icf(&image).unwrap();
        let after = cs705::icf::rows::NormalMemory::from_icf(&after).unwrap();
        assert_eq!(
            &before.record(0, 1).unwrap().unwrap()[34..67],
            &after.record(0, 1).unwrap().unwrap()[34..67]
        );
        if name == "selection" {
            assert_eq!(report["selection_frame"]["node_flags"], 164);
        }
    }
}

#[test]
fn native_choice_false_mode_guard_and_sel_external_inputs_are_explicit() {
    let files = Files::new();
    let source = normal_fixture();
    let state = initialized_normal_state(&source, 0, None);
    let state_file = files.write("choice-state", &serde_json::to_vec(&state).unwrap());
    for (name, input, blocked) in [
        (
            "selection",
            json!({"input":{"action":"cancel"},"layout":1,"node_flags":164}),
            false,
        ),
        (
            "duplex",
            json!({"action":"selected","key":2,"phase":2,"candidate":4,"accepted":true}),
            false,
        ),
        (
            "polarity",
            json!({"action":"selected","key":1,"phase":1,"candidate":5,"accepted":true}),
            false,
        ),
        ("digital-squelch", json!({"action":"next"}), true),
    ] {
        let assignment = format!("record/native-memory/0/2/{name}={input}");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state_file),
                "--set",
                &assignment,
                "--output",
                "-",
            ],
            &source,
        );
        assert!(out.status.success(), "{name}");
        let report = choice_report(&out);
        assert_eq!(report["stored"], false);
        assert_eq!(report["mode_blocked"], blocked);
        assert_eq!(report["normal_state"], state);
        assert!(out.stdout.is_empty());
        if name == "selection" {
            assert_eq!(report["selection_frame"]["node_flags"], 165);
        }
    }
    for text in [
        "1",
        r#"{"input":1,"layout":0}"#,
        r#"{"input":1,"layout":0,"node_flags":256}"#,
    ] {
        let assignment = format!("record/native-memory/0/2/selection={text}");
        let target = files.path("rejected.icf");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state_file),
                "--set",
                &assignment,
                "--output",
                p(&target),
            ],
            &source,
        );
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
        assert!(!target.exists());
    }
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            "-",
            "--set",
            "record/native-memory/0/2/duplex=1",
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

#[test]
fn native_choice_same_value_store_dry_run_cancel_and_no_clobber_keep_root_policy() {
    let files = Files::new();
    let source = normal_fixture();
    let state = initialized_normal_state(&source, 0, None);
    let state = files.write("choice-state", &serde_json::to_vec(&state).unwrap());
    let target = files.write("existing.icf", b"KEEP");
    let assignment = "record/native-memory/0/2/duplex=0";
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            assignment,
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success());
    assert_eq!(choice_report(&out)["stored"], true);
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            assignment,
            "--output",
            p(&target),
        ],
        &source,
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert_eq!(fs::read(&target).unwrap(), b"KEEP");
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            assignment,
            "--dry-run",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            assignment,
            "--cancel",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn native_code_uses_dedicated_text_and_keyboard_not_selected_candidates() {
    let files = Files::new();
    let source = normal_fixture();
    let mut image = Icf::parse(&source).unwrap();
    let mut h = normal_holder(&source, 2);
    h.set_mode(16);
    h.set_channel_field(ChannelField::DigitalCode, 99);
    record::store_channel_image_into(&mut image, ChannelKind::Normal, 2, &mut h).unwrap();
    let source = image.to_bytes();
    let state = initialized_normal_state(&source, 0, None);
    let state = files.write("code-state", &serde_json::to_vec(&state).unwrap());
    for (input, first, expected, stored, display, selection, state_out) in [
        (
            json!("09"),
            None,
            9,
            true,
            b"09".as_slice(),
            json!([0, 2]),
            0,
        ),
        (
            json!("12\u{0000}TAIL"),
            None,
            12,
            true,
            b"12".as_slice(),
            json!([0, 2]),
            0,
        ),
        (
            json!(" 1"),
            None,
            99,
            false,
            b"99".as_slice(),
            json!([0, 2]),
            0,
        ),
        (
            json!("001"),
            None,
            99,
            false,
            b"99".as_slice(),
            json!([0, 2]),
            0,
        ),
        (
            json!({"action":"selected","key":1,"phase":2,"candidate":7,"accepted":true}),
            None,
            99,
            false,
            b"99".as_slice(),
            json!([0, 2]),
            0,
        ),
        (
            json!({"action":"next"}),
            None,
            0,
            true,
            b"99".as_slice(),
            json!([0, 2]),
            0,
        ),
        (
            json!({"action":"begin"}),
            Some(b'3'),
            99,
            false,
            b"3".as_slice(),
            json!([1, 0]),
            2,
        ),
        (
            json!({"action":"cancel"}),
            None,
            99,
            false,
            b"99".as_slice(),
            json!([0, 2]),
            0,
        ),
    ] {
        let mut v = json!({"input":input,"node_flags":165});
        if let Some(first) = first {
            v["first"] = json!(first);
        }
        let assignment = format!("record/native-memory/0/2/digital-code={v}");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--set",
                &assignment,
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
        let report = serde_json::Deserializer::from_slice(&out.stderr)
            .into_iter::<Value>()
            .filter_map(std::result::Result::ok)
            .find(|v| v.get("code_node_flags").is_some())
            .expect("native code report");
        assert_eq!(report["stored"], stored);
        assert_eq!(report["code_node_flags"], 166);
        assert_eq!(report["state_out"], state_out);
        assert_eq!(report["selection"], selection);
        assert_eq!(
            report["display"]["hex"],
            display
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<String>()
        );
        if stored {
            assert_eq!(
                normal_holder(&out.stdout, 2).channel_field(ChannelField::DigitalCode),
                expected
            );
        } else {
            assert!(out.stdout.is_empty());
            assert_eq!(
                normal_holder(&source, 2).channel_field(ChannelField::DigitalCode),
                expected
            );
            assert_eq!(
                report["normal_state"],
                serde_json::from_slice::<Value>(&fs::read(&state).unwrap()).unwrap()
            );
        }
    }
}

#[test]
fn native_state_restore_requires_closed_completed_source_but_constructor_resolves_host() {
    let files = Files::new();
    let source = normal_fixture();
    let completed = initialized_normal_state(&source, 16, Some(1033));
    assert_eq!(completed["range_selected"], 1);
    assert_eq!(completed["range_source"], 1);
    for selected in [5u32, 16, u32::MAX] {
        let mut invalid = completed.clone();
        invalid["range_selected"] = json!(selected);
        let state = files.write(
            "closed-completed-source",
            &serde_json::to_vec(&invalid).unwrap(),
        );
        let target = files.path("reinterpreted.icf");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--set",
                "record/native-memory/0/2/filter=3",
                "--output",
                p(&target),
            ],
            &source,
        );
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
        assert!(!target.exists());
    }
    let state = files.write("completed-source", &serde_json::to_vec(&completed).unwrap());
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            "record/native-memory/0/2/filter=3",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success(), "completed restore stays accepted");
    assert_eq!(normal_holder(&out.stdout, 2).filter(), 3);
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn call_view_and_field_read_keep_unknown_decoded_values() {
    let source = fixture();
    let out = cli(
        &[
            "show", "-", "--sheet", "call", "--start", "0", "--count", "1",
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(json(&out)[0]["mode"], 255);
    assert_eq!(json(&out)[0]["frequency-hz"], 145_123_456);
    let out = cli(&["show", "-", "--key", "record/call/0/mode"], &source);
    assert!(out.status.success());
    assert_eq!(json(&out)[0]["value"], 255);
    let out = cli(&["show", "-", "--key", "record/call/8/mode"], &source);
    assert_eq!(out.status.code(), Some(5));
    assert!(json(&out)[0].get("error").is_some());
}

#[test]
fn call_edits_use_shared_saver_ascii_editor_and_late_failure_publication_guard() {
    let files = Files::new();
    let source = fixture();
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "record/call/0/name=\"A\\u0000B\"",
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
    assert_eq!(holder(&out.stdout, 0).name(), b"A B             ");
    assert_eq!(holder(&out.stdout, 0).mode(), 255);
    let output = files.write("keep.icf", b"old output");
    for edits in [
        ["record/call/0/name=FIRST", "record/call/0/not-a-field=0"],
        ["record/call/0/name=FIRST", "record/call/8/name=LAST"],
    ] {
        let out = cli(
            &[
                "edit",
                "-",
                "--set",
                edits[0],
                "--set",
                edits[1],
                "--output",
                p(&output),
                "--overwrite",
            ],
            &source,
        );
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
        assert_eq!(fs::read(&output).unwrap(), b"old output");
    }
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "record/call/0/name=\"\\u65e5\"",
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

#[test]
fn typed_call_clipboard_roundtrip_cancel_and_dry_run() {
    let files = Files::new();
    let source = fixture();
    let input = files.write("source.icf", &source);
    let ranges = native_ranges(&files);
    let copied = cli(
        &[
            "row",
            p(&input),
            "--sheet",
            "call",
            "--show-both",
            "false",
            "copy",
            "100",
            "0",
            "--output",
            "-",
        ],
        b"",
    );
    assert!(
        copied.status.success(),
        "{}",
        String::from_utf8_lossy(&copied.stderr)
    );
    assert_eq!(json(&copied)["schema"], "cs705.native-clipboard");
    assert_eq!(json(&copied)["version"], 2);
    assert_eq!(
        json(&copied)["records"][0]["bytes"]
            .as_array()
            .unwrap()
            .len(),
        616
    );
    let clip = files.write("clip.json", &copied.stdout);
    let pasted = cli(
        &[
            "row",
            p(&input),
            "--sheet",
            "call",
            "--show-both",
            "false",
            "--native-range-context",
            p(&ranges),
            "paste",
            "100",
            "2",
            "--clipboard",
            p(&clip),
            "--yes",
            "--output",
            "-",
        ],
        b"",
    );
    assert!(
        pasted.status.success(),
        "{}",
        String::from_utf8_lossy(&pasted.stderr)
    );
    assert_eq!(holder(&pasted.stdout, 2).name(), b"SOURCE          ");
    assert_eq!(
        &Icf::parse(&pasted.stdout).unwrap().memory()[0x392fa..0x392fd],
        &[100, 1, 0]
    );
    assert_eq!(
        holder(&pasted.stdout, 2).channel_field(ChannelField::FrequencyHz),
        145_123_456
    );
    let cancelled = cli(
        &[
            "row",
            p(&input),
            "--sheet",
            "call",
            "--show-both",
            "false",
            "--native-range-context",
            p(&ranges),
            "--accept-partial",
            "paste",
            "100",
            "3",
            "--clipboard",
            p(&clip),
            "--output",
            "-",
        ],
        b"",
    );
    assert_eq!(cancelled.status.code(), Some(6));
    assert!(cancelled.stdout.is_empty());
    let dry = cli(
        &[
            "row",
            p(&input),
            "--sheet",
            "call",
            "--show-both",
            "false",
            "--native-range-context",
            p(&ranges),
            "paste",
            "100",
            "3",
            "--clipboard",
            p(&clip),
            "--yes",
            "--dry-run",
        ],
        b"",
    );
    assert!(dry.status.success());
    assert!(dry.stdout.is_empty());
    assert!(String::from_utf8_lossy(&dry.stderr).contains("\"published\": false"));
}

#[test]
fn native_call_paste_keeps_invalid_live_mr_reference_failure_prefix_private() {
    let files = Files::new();
    let source = fixture();
    let ranges = native_ranges(&files);
    let copied = cli(
        &[
            "row",
            "-",
            "--sheet",
            "call",
            "--show-both",
            "false",
            "copy",
            "100",
            "0",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(copied.status.success());
    let clipboard = files.write("native-call.json", &copied.stdout);
    let mut image = Icf::parse(&source).unwrap();
    image.patch_memory(&[(0x392fa, &[101])]).unwrap();
    let invalid = image.to_bytes();
    for accept_partial in [false, true] {
        let output = files.path(&format!("invalid-mr-{accept_partial}.icf"));
        let mut args = vec![
            "row",
            "-",
            "--sheet",
            "call",
            "--show-both",
            "false",
            "--native-range-context",
            p(&ranges),
        ];
        if accept_partial {
            args.push("--accept-partial");
        }
        args.extend([
            "paste",
            "100",
            "2",
            "--clipboard",
            p(&clipboard),
            "--yes",
            "--output",
            p(&output),
        ]);
        let out = cli(&args, &invalid);
        assert_eq!(out.status.code(), Some(if accept_partial { 5 } else { 3 }));
        assert!(out.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("native MR group has no typed dataset")
        );
        if accept_partial {
            let bytes = fs::read(&output).unwrap();
            assert_eq!(
                holder(&bytes, 2).channel_field(ChannelField::FrequencyHz),
                145_123_456
            );
            assert_eq!(
                &Icf::parse(&bytes).unwrap().memory()[0x392fa..0x392fd],
                &[101, 1, 0]
            );
        } else {
            assert!(!output.exists());
        }
    }
}

#[test]
fn clipboard_schema_header_width_stdin_set_and_no_clobber_fail_before_publication() {
    let files = Files::new();
    let source = fixture();
    let input = files.write("source.icf", &source);
    let ranges = native_ranges(&files);
    let good = json!({"version":2,"schema":"cs705.native-clipboard","kind":3,"mode":2,"declared_count":1,"logical_count":1,"records":[{"bytes":holder(&source,0).as_bytes().as_slice()}]});
    let mut extra = good.clone();
    extra["extra"] = json!(true);
    let mut short = good.clone();
    short["records"][0]["bytes"].as_array_mut().unwrap().pop();
    let mut header = good.clone();
    header["records"][0]["bytes"][0] = json!(12);
    let mut empty = good.clone();
    empty["records"] = json!([]);
    empty["declared_count"] = json!(0);
    empty["logical_count"] = json!(0);
    let legacy =
        json!({"version":1,"schema":"cs705.decoded616","kind":3,"records":good["records"]});
    let output = files.write("keep.icf", b"old output");
    for bad in [extra, short, header, empty, legacy] {
        let clip = files.write("bad.json", &serde_json::to_vec(&bad).unwrap());
        let out = cli(
            &[
                "row",
                p(&input),
                "--sheet",
                "call",
                "--show-both",
                "false",
                "--native-range-context",
                p(&ranges),
                "paste",
                "100",
                "0",
                "--clipboard",
                p(&clip),
                "--yes",
                "--output",
                p(&output),
                "--overwrite",
            ],
            b"",
        );
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
        assert_eq!(fs::read(&output).unwrap(), b"old output");
    }
    let clip = files.write("good.json", &serde_json::to_vec(&good).unwrap());
    let out = cli(
        &[
            "row",
            p(&input),
            "--sheet",
            "call",
            "--show-both",
            "false",
            "--native-range-context",
            p(&ranges),
            "paste",
            "100",
            "0",
            "--clipboard",
            p(&clip),
            "--yes",
            "--output",
            p(&output),
        ],
        b"",
    );
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(fs::read(&output).unwrap(), b"old output");
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "call",
            "paste",
            "100",
            "0",
            "--clipboard",
            "-",
            "--yes",
            "--output",
            "-",
        ],
        b"not ICF",
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("stdin may be consumed by only one input")
    );
    let unused = files.path("unused");
    assert!(!unused.exists());
}

#[test]
fn ur_raw_slot_read_edit_preserves_call_and_rejects_unicode_or_invalid_slots() {
    let source = fixture();
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "record/ur/299/name=LAST NAME",
            "--set",
            "record/ur/299/call=[193,194,195,196,0,32,32,199]",
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
    let mut h = record::reset_holder(0x0d, 299, 0);
    record::load_callsign_storage_into(&image, 299, &mut h).unwrap();
    assert_eq!(h.name(), b"LAST NAME       ");
    assert_eq!(
        h.callsign(cs705::icf::MemoryCallsign::Your),
        &[193, 194, 195, 196, 0, 32, 32, 199]
    );
    let read = cli(&["show", "-", "--key", "record/ur/299/call"], &out.stdout);
    assert!(read.status.success());
    assert_eq!(json(&read)[0]["value"]["hex"], "C1C2C3C4002020C7");
    for key in [
        "record/ur/300/name=BAD",
        "record/ur/0/frequency-hz=123",
        "record/ur/0/name=\"\\u65e5\"",
    ] {
        let bad = cli(&["edit", "-", "--set", key, "--output", "-"], &source);
        assert_eq!(bad.status.code(), Some(3));
        assert!(bad.stdout.is_empty());
    }
}

#[test]
fn call_move_and_native_row_permissions_are_not_raw_clear_fallbacks() {
    let files = Files::new();
    let ranges = native_ranges(&files);
    let source = fixture();
    let before = holder(&source, 2).channel_field(ChannelField::FrequencyHz);
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "call",
            "--side-view",
            "--show-both",
            "true",
            "--native-range-context",
            p(&ranges),
            "move",
            "100",
            "0",
            "--destination-group",
            "100",
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
    assert_eq!(
        holder(&out.stdout, 1).channel_field(ChannelField::FrequencyHz),
        145_123_456
    );
    assert_eq!(
        holder(&out.stdout, 2).channel_field(ChannelField::FrequencyHz),
        before
    );
    for action in ["clear", "delete", "insert"] {
        let out = cli(
            &[
                "row", "-", "--sheet", "call", action, "100", "0", "--yes", "--output", "-",
            ],
            &source,
        );
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains("forbidden by native metadata"));
    }
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "record/call/0/selection=2",
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

#[test]
fn native_call_rows_accept_real_cross_kind_packets_and_pair_carry() {
    use cs705::icf::rows::native::{Effects, call};
    let files = Files::new();
    let ranges = native_ranges(&files);
    let destination = native_call_fixture(13, 145_000_000);
    for kind in [2, 3, 4, 5] {
        let source = if kind == 3 {
            destination.clone()
        } else {
            normal_fixture()
        };
        let mut image = Icf::parse(&source).unwrap();
        let sheet = match kind {
            2 => "memory",
            3 => "call",
            4 => "bandstack",
            _ => "memopad",
        };
        let index = if kind == 2 {
            "2"
        } else if kind == 3 {
            "0"
        } else {
            "1"
        };
        if kind >= 4 {
            let mut h = record::reset_holder(kind, 1, 0);
            h.set_mode(13);
            h.set_filter(1);
            h.set_channel_field(ChannelField::FrequencyHz, 146_987_654);
            let mut memory = image.memory().to_vec();
            record_image::store_compact_into(&mut memory, kind, 1, &mut h, |_, _| Ok(())).unwrap();
            image.patch_memory(&[(0, &memory)]).unwrap();
        }
        let mut args = vec!["row", "-", "--sheet", sheet];
        if kind < 4 {
            args.extend(["--side-view", "--show-both", "true"]);
        }
        args.extend([
            "copy",
            if kind == 3 { "100" } else { "0" },
            index,
            "--output",
            "-",
        ]);
        let copied = cli(&args, &image.to_bytes());
        assert!(
            copied.status.success(),
            "{}",
            String::from_utf8_lossy(&copied.stderr)
        );
        assert_eq!(json(&copied)["version"], 2);
        assert_eq!(json(&copied)["kind"], kind);
        let clipboard = files.write("cross-kind.json", &copied.stdout);
        let out = cli(
            &[
                "row",
                "-",
                "--sheet",
                "call",
                "--side-view",
                "--show-both",
                "true",
                "--native-range-context",
                p(&ranges),
                "paste",
                "100",
                "6",
                "--clipboard",
                p(&clipboard),
                "--yes",
                "--output",
                "-",
            ],
            &destination,
        );
        assert!(
            out.status.success(),
            "kind{kind}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let h = holder(&out.stdout, 6);
        assert_eq!(
            h.channel_field(ChannelField::FrequencyHz),
            match kind {
                2 => 145_000_002,
                3 => 145_000_000,
                _ => 146_987_654,
            }
        );
        assert_eq!(h.name(), holder(&destination, 6).name());
        assert_eq!(
            holder(&out.stdout, 7).channel_field(ChannelField::FrequencyHz),
            433_000_000
        );
        let image = Icf::parse(&out.stdout).unwrap();
        assert_eq!(image.comment_bytes(), b"NATIVE CALL META");
        assert_eq!(image.extra_data(), 3);
    }
    let mut image = Icf::parse(&destination).unwrap();
    for index in 0..8 {
        let mut h = holder(&destination, index);
        h.set_channel_field(ChannelField::FrequencyHz, 145_000_000 + index * 1000);
        record::store_channel_image_into(&mut image, ChannelKind::Call, index, &mut h).unwrap();
    }
    let source = image.to_bytes();
    let mut expected = image.memory().to_vec();
    let b = cs705::icf::frequency::Ranges::generate(
        &cs705::icf::context::Context::from_bytes(context(0)),
        None,
    )
    .unwrap();
    let mut effects = Effects::default();
    let result = call::carry(
        &mut expected,
        call::Selection {
            active: 2,
            first: 2,
            last: 3,
            merged: true,
            show_both: false,
        },
        1,
        &b,
        &mut effects,
        |_, _| Ok(()),
    )
    .unwrap();
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "call",
            "--end-index",
            "3",
            "--show-both",
            "false",
            "--native-range-context",
            p(&ranges),
            "move",
            "100",
            "2",
            "--destination-group",
            "100",
            "--destination-index",
            "4",
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
    assert_eq!(Icf::parse(&out.stdout).unwrap().memory(), expected);
    assert!(result.processed);
    let target = files.path("boundary-no-op.icf");
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "call",
            "--end-index",
            "7",
            "--show-both",
            "false",
            "--native-range-context",
            p(&ranges),
            "move",
            "100",
            "0",
            "--destination-group",
            "100",
            "--destination-index",
            "2",
            "--output",
            p(&target),
        ],
        &source,
    );
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(!target.exists());
}

#[test]
fn native_call_reset_requires_real_g_and_confirmation_without_clear_fallback() {
    let files = Files::new();
    let source = native_call_fixture(13, 145_000_000);
    let mut generation = Icf::parse(&source).unwrap().memory().to_vec();
    generation.resize(cs705::clone::IMAGE_CAPACITY, 0);
    for index in 0..2 {
        let mut h = holder(&source, index);
        h.set_name(b"RESET SOURCE    ");
        h.set_channel_field(ChannelField::FrequencyHz, 146_000_000 + index);
        record_image::store_channel_into(&mut generation, ChannelKind::Call, index, &mut h, |_| {
            Ok(())
        })
        .unwrap();
    }
    let g = files.write("reset-G", &generation);
    for confirmation in [0, 1, 256] {
        let assignment = format!(
            "record/native-call/reset-pair={}",
            json!({"selection":{
            "active":1,"first":1,"last":1,"merged":false,"show_both":false},"confirmation":confirmation})
        );
        let out = cli(
            &[
                "edit",
                "-",
                "--generation",
                p(&g),
                "--set",
                &assignment,
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
        assert_eq!(out.stdout.is_empty(), confirmation != 1);
        if confirmation == 1 {
            assert_eq!(holder(&out.stdout, 0).name(), b"RESET SOURCE    ");
            assert_eq!(
                holder(&out.stdout, 1).channel_field(ChannelField::FrequencyHz),
                146_000_001
            );
            assert_eq!(
                holder(&out.stdout, 2).channel_field(ChannelField::FrequencyHz),
                145_000_000
            );
            let reports = serde_json::Deserializer::from_slice(&out.stderr)
                .into_iter::<Value>()
                .filter_map(std::result::Result::ok)
                .collect::<Vec<_>>();
            let reset = reports
                .iter()
                .find(|v| v["operation"] == "native-call-reset-pair")
                .unwrap();
            let ids = reset["effects"]["requested_notifications"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n["id"].as_u64().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(ids, [0x14, 0x14, 0x11, 0x3c]);
            assert_eq!(reset["effects"]["dr_invalidated"], 0);
        }
    }
    let assignment = format!(
        "record/native-call/reset-pair={}",
        json!({"selection":{
        "active":0,"first":0,"last":0,"merged":false,"show_both":true},"confirmation":1})
    );
    let out = cli(
        &["edit", "-", "--set", &assignment, "--output", "-"],
        &source,
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let init = format!(
        "record/native-call/state={}",
        json!({"range_context":context(0),"range_selected":0,"range_langid":null})
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            &init,
            "--set",
            "record/native-call/0/duplex=0",
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
    assert!(!out.stdout.is_empty());
}

fn native_ur_assignment(index: u32, field: &str, input: Value, last: i32) -> String {
    format!(
        "record/native-ur/{index}/{field}={}",
        json!({"input":input,"phase":1,"alignment":1,
        "frame":{"code":13,"command":0,"blocked":false,"view_last":last,"view_enabled":1,
            "node_flags":167,"warning_bits":128,"notify_flags":32,"ur_changed":false}})
    )
}

#[test]
fn native_ur_name_call_collectors_preserve_cancel_insert_prefix_and_real_approval() {
    let files = Files::new();
    let source = ur_fixture();
    let t = files.write("ur-T", &vec![0; cs705::clone::IMAGE_CAPACITY]);
    for (field, input, expected) in [
        ("name", json!("NEW NAME"), b"NEW NAME        ".as_slice()),
        ("call", json!("ab cd"), b"AB    CD".as_slice()),
    ] {
        let assignment = native_ur_assignment(1, field, input, 2);
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
        let working = cs705::icf::rows::WorkingUr::from_icf(&image).unwrap();
        let mut h = record::reset_holder(0x0d, 1, 0);
        working.load_into(1, &mut h).unwrap();
        assert_eq!(
            if field == "name" {
                h.name().as_slice()
            } else {
                h.callsign(cs705::icf::MemoryCallsign::Your).as_slice()
            },
            expected
        );
        assert_eq!(image.comment_bytes(), b"RECORD META");
        assert_eq!(image.extra_data(), 3);
    }
    let cancelled = native_ur_assignment(3, "call", json!({"action":"cancel"}), 2);
    let accepted = native_ur_assignment(1, "name", json!("EXACT NAME"), 2);
    for pair in [
        vec![cancelled.as_str()],
        vec![cancelled.as_str(), accepted.as_str()],
        vec![accepted.as_str(), cancelled.as_str()],
    ] {
        let mut args = vec!["edit", "-", "--reference-defaults", p(&t)];
        for assignment in &pair {
            args.extend(["--set", assignment]);
        }
        args.extend(["--output", "-"]);
        let out = cli(&args, &source);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout.is_empty(), pair.len() == 1);
        let reports = serde_json::Deserializer::from_slice(&out.stderr)
            .into_iter::<Value>()
            .filter_map(std::result::Result::ok)
            .collect::<Vec<_>>();
        let cancelled = reports.iter().find(|v| v["inserted"] == true).unwrap();
        assert_eq!(cancelled["stored"], false);
        assert_eq!(cancelled["requested_notifications"], json!([0x40, 0x39]));
        assert_eq!(cancelled["display"]["hex"], "");
        if pair.len() > 1 {
            assert_eq!(
                cs705::icf::rows::WorkingUr::from_icf(&Icf::parse(&out.stdout).unwrap())
                    .unwrap()
                    .count(),
                4
            );
        }
    }
    for assignment in [
        native_ur_assignment(0, "call", json!(""), 2),
        native_ur_assignment(3, "name", json!("IGNORED"), 2),
    ] {
        let out = cli(
            &["edit", "-", "--set", &assignment, "--output", "-"],
            &source,
        );
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
    }
    let out = cli(
        &["edit", "-", "--set", &cancelled, "--output", "-"],
        &source,
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let invalid =
        native_ur_assignment(3, "call", json!("NEW"), 2).replacen("\"phase\":1", "\"phase\":3", 1);
    let out = cli(
        &[
            "edit",
            "-",
            "--reference-defaults",
            p(&t),
            "--set",
            &invalid,
            "--output",
            "-",
        ],
        &source,
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let prefix = serde_json::Deserializer::from_slice(&out.stderr)
        .into_iter::<Value>()
        .filter_map(std::result::Result::ok)
        .find(|v| v["operation"] == "native-ur-input")
        .unwrap();
    assert_eq!(prefix["inserted"], true);
    assert_eq!(prefix["stored"], false);
    assert_eq!(prefix["partial"], true);
}

#[test]
fn rx_namespace_decodes_reverse_map_and_never_saves_read_only_fields() {
    let source = fixture();
    let mut image = Icf::parse(&source).unwrap();
    let mut map = [255; 50];
    map[7] = 0;
    map[11] = 0;
    image
        .patch_memory(&[
            (0x3c2ff, &map),
            (0x309c3 + 0x32 + 48 * 7 + 36, &123_456_789u32.to_le_bytes()),
            (0x309c3 + 0x32 + 48 * 11 + 36, &987_654_321u32.to_le_bytes()),
        ])
        .unwrap();
    let source = image.to_bytes();
    let out = cli(
        &[
            "show",
            "-",
            "--key",
            "record/rx/0/frequency-hz",
            "--key",
            "record/rx/1/frequency-hz",
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(json(&out)[0]["value"], 123_456_789);
    assert_eq!(json(&out)[1]["value"], Value::Null);
    for key in [
        "record/rx/0/frequency-hz=0",
        "record/rx/50/frequency-hz=0",
        "record/rx/0/unknown=0",
    ] {
        let bad = cli(&["edit", "-", "--set", key, "--output", "-"], &source);
        assert_eq!(bad.status.code(), Some(3));
        assert!(bad.stdout.is_empty());
    }
}

#[test]
fn logical_memory_edit_uses_existing_map_and_refuses_implicit_registration() {
    let mut memory = vec![0; 0x4997a];
    memory[0x355a1] = 6;
    memory[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
    memory[0x3fb40..0x3fb44].copy_from_slice(&1u32.to_le_bytes());
    memory[0x3662d..0x36821].fill(1);
    for cell in memory[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
        cell.copy_from_slice(&0xffffu32.to_le_bytes());
    }
    for g in 0..100 {
        memory[0x355c3 + 19 * g..0x355c3 + 19 * g + 16].fill(b' ');
    }
    let mut model = cs705::icf::rows::NormalMemory::from_working(memory).unwrap();
    model.allocate(0, 0..1).unwrap();
    let mut h = record::reset_holder(2, 0, 0);
    h.set_name(b"MAPPED NAME     ");
    h.set_mode(13);
    h.set_filter(1);
    h.set_channel_field(ChannelField::FrequencyHz, 145_000_000);
    model.store_channel(0, &mut h).unwrap();
    model.store_channel(1, &mut h).unwrap();
    let source = model.to_icf().unwrap().to_bytes();
    let out = cli(
        &["show", "-", "--key", "record/memory/0/0/frequency-hz"],
        &source,
    );
    assert!(out.status.success());
    assert_eq!(json(&out)[0]["value"], 145_000_000);
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "record/memory/0/1/name=LOGICAL NAME",
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
    let mut h = record::reset_holder(2, 0, 0);
    record::load_channel_image_into(&image, ChannelKind::Normal, 0, &mut h).unwrap();
    assert_eq!(h.name(), b"LOGICAL NAME    ");
    assert_eq!(h.channel_field(ChannelField::FrequencyHz), 145_000_000);
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "record/memory/0/2/frequency-hz=145000000",
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(6));
    assert!(out.stdout.is_empty());
}

#[test]
fn field_catalog_exports_every_recognized_record_path_once_with_domains() {
    let out = cli(&["show", "-", "--list-fields"], b"input must not be read");
    assert!(out.status.success());
    let rows = json(&out);
    let rows: Vec<_> = rows
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| {
            v["key"]
                .as_str()
                .is_some_and(|key| key.starts_with("record/"))
        })
        .collect();
    assert_eq!(rows.len(), 124);
    let mut keys = std::collections::BTreeSet::new();
    for row in &rows {
        assert!(keys.insert(row["key"].as_str().unwrap()));
        assert!(row["value"].is_object());
        assert!(row["arguments"].is_array());
        assert!(row["read_only"].is_boolean());
    }
    for (key, readonly) in [
        ("record/call/INDEX/selection", true),
        ("record/memory/GROUP/INDEX/selection", false),
        ("record/ur/INDEX/call", false),
        ("record/rx/INDEX/frequency-hz", true),
    ] {
        let row = rows.iter().find(|v| v["key"] == key).unwrap();
        assert_eq!(row["read_only"], readonly);
    }
}

#[test]
fn repeater_runtime_view_edit_uses_saved_order_indirect_gateway_and_keeps_metadata() {
    let files = Files::new();
    let q = files.write("Q", &context(0));
    let source = repeater_fixture();
    let out = cli(
        &[
            "show",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "--start",
            "0",
            "--count",
            "3",
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows = json(&out);
    assert_eq!(rows[0]["frequency-hz"], 145_000_002);
    assert_eq!(rows[1]["frequency-hz"], 145_000_000);
    assert_eq!(rows[2]["frequency-hz"], 145_000_001);
    assert_eq!(rows[0]["gateway-call"]["hex"], "4F54484552202047");
    assert_eq!(rows[1]["gateway-call"], rows[2]["gateway-call"]);
    let out = cli(
        &[
            "edit",
            "-",
            "--record-context",
            p(&q),
            "--set",
            "record/repeater/2/name=EDITED",
            "--set",
            "record/repeater/2/utc-offset-minutes=-60",
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
    assert_eq!(image.comment_bytes(), b"RECORD META");
    let h = repeater_holder(&out.stdout, 2);
    assert_eq!(h.name(), b"EDITED          ");
    assert_eq!(h.repeater_text(record::RepeaterText::Gateway), b"GATEWAYG");
    assert_eq!(
        h.repeater_field(record::RepeaterField::UtcOffset) as i32,
        -60
    );
    assert_eq!(h.channel_field(ChannelField::FrequencyHz), 145_000_001);
    let out = cli(
        &[
            "show",
            "-",
            "--record-context",
            p(&q),
            "--key",
            "record/repeater/2/name",
        ],
        &out.stdout,
    );
    assert!(out.status.success());
    assert_eq!(
        json(&out)[0]["value"]["hex"],
        "45444954454420202020202020202020"
    );
    let missing = cli(
        &["show", "-", "--sheet", "repeater", "--count", "1"],
        &source,
    );
    assert_eq!(missing.status.code(), Some(6));
    assert!(missing.stdout.is_empty());
    for assignment in [
        "record/repeater/0/valid=true",
        "record/repeater/0/gateway-index=0",
        "record/repeater/0/name=\"\\u65e5\"",
    ] {
        let bad = cli(
            &[
                "edit",
                "-",
                "--record-context",
                p(&q),
                "--set",
                assignment,
                "--output",
                "-",
            ],
            &source,
        );
        assert_eq!(bad.status.code(), Some(3));
        assert!(bad.stdout.is_empty());
    }
}

#[test]
fn repeater_rows_require_g_only_at_reset_and_publish_only_accepted_real_prefix() {
    let files = Files::new();
    let q = files.write("Q", &context(0));
    let mut image = Icf::parse(&repeater_fixture()).unwrap().to_bytes();
    let copy = cli(
        &[
            "row",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "copy",
            "2",
            "0",
            "--output",
            "-",
        ],
        &image,
    );
    assert!(
        copy.status.success(),
        "{}",
        String::from_utf8_lossy(&copy.stderr)
    );
    assert_eq!(json(&copy)["kind"], 0x0c);
    let clip = files.write("clipboard", &copy.stdout);
    let cancel = cli(
        &[
            "row",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "paste",
            "2",
            "1",
            "--clipboard",
            p(&clip),
            "--output",
            "-",
        ],
        &image,
    );
    assert_eq!(cancel.status.code(), Some(6));
    assert!(cancel.stdout.is_empty());
    let mut parsed = Icf::parse(&image).unwrap();
    parsed
        .patch_memory(&[(0x3c8b9, &1u16.to_le_bytes())])
        .unwrap();
    image = parsed.to_bytes();
    let args = [
        "row",
        "-",
        "--sheet",
        "repeater",
        "--record-context",
        p(&q),
        "paste",
        "2",
        "1",
        "--clipboard",
        p(&clip),
        "--yes",
        "--output",
        "-",
    ];
    let out = cli(&args, &image);
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("explicit global G reset source unavailable")
    );
    let partial = cli(
        &[
            "row",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "--accept-partial",
            "paste",
            "2",
            "1",
            "--clipboard",
            p(&clip),
            "--yes",
            "--output",
            "-",
        ],
        &image,
    );
    assert_eq!(partial.status.code(), Some(5));
    assert_eq!(
        repeater_holder(&partial.stdout, 1).channel_field(ChannelField::FrequencyHz),
        145_000_002
    );
    let after = Icf::parse(&partial.stdout).unwrap();
    assert_eq!(&after.memory()[0x3c8b9..0x3c8bb], &1u16.to_le_bytes());
    assert_eq!(after.comment_bytes(), b"RECORD META");
    let mut generation = vec![0; 0x4997a];
    generation[0x3c8b9..0x3c8bb].copy_from_slice(&0x1234u16.to_le_bytes());
    generation[0x3c903..0x3c905].copy_from_slice(&0x5678u16.to_le_bytes());
    let g = files.write("G", &generation);
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "--generation",
            p(&g),
            "paste",
            "2",
            "1",
            "--clipboard",
            p(&clip),
            "--yes",
            "--output",
            "-",
        ],
        &image,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed = Icf::parse(&out.stdout).unwrap();
    assert_eq!(&parsed.memory()[0x3c8b9..0x3c8bb], &0x1234u16.to_le_bytes());
    let protected = files.write("protected", b"KEEP");
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "--generation",
            p(&g),
            "delete",
            "2",
            "1",
            "--yes",
            "--output",
            p(&protected),
        ],
        &image,
    );
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(fs::read(&protected).unwrap(), b"KEEP");
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "--generation",
            p(&g),
            "insert",
            "2",
            "1",
            "--output",
            "-",
        ],
        &image,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let inserted = repeater_holder(&out.stdout, 1);
    assert_eq!(
        inserted.repeater_text(record::RepeaterText::Call),
        b"CALLSIGN"
    );
    assert_eq!(inserted.offset_hz(), 5_000_000);
    assert_eq!(
        cs705::repeater::groups::Groups::from_icf(&Icf::parse(&out.stdout).unwrap())
            .unwrap()
            .counts[2],
        4
    );
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "move",
            "2",
            "0",
            "--destination-group",
            "2",
            "--destination-index",
            "1",
            "--dry-run",
        ],
        &image,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

#[test]
fn repeater_q_langid_stdin_set_and_partial_policy_are_explicit() {
    let files = Files::new();
    let q = files.write("Q16", &context(16));
    let source = repeater_fixture();
    let missing = cli(
        &[
            "show",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "--count",
            "1",
        ],
        &source,
    );
    assert_eq!(missing.status.code(), Some(3));
    assert!(missing.stdout.is_empty());
    let out = cli(
        &[
            "show",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "--system-langid",
            "1041",
            "--start",
            "3",
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
    assert_eq!(json(&out)[0]["offset-hz"], 5_000_000);
    assert_eq!(json(&out)[0]["longitude-bytes"]["hex"], "00000045");
    let out = cli(
        &[
            "show",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "--system-langid",
            "1033",
            "--start",
            "3",
            "--count",
            "1",
        ],
        &source,
    );
    assert!(out.status.success());
    assert_eq!(json(&out)[0]["offset-hz"], 0);
    assert_eq!(json(&out)[0]["longitude-bytes"]["hex"], "00000057");
    for args in [
        vec!["show", "-", "--sheet", "repeater", "--record-context", "-"],
        vec![
            "edit",
            "-",
            "--record-context",
            p(&q),
            "--generation",
            "-",
            "--set",
            "record/repeater/0/name=X",
            "--output",
            "-",
        ],
        vec![
            "row",
            "missing",
            "--sheet",
            "repeater",
            "--record-context",
            "-",
            "--generation",
            "-",
            "copy",
            "2",
            "0",
            "--output",
            "-",
        ],
    ] {
        let out = cli(&args, b"not a document");
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains("stdin may be consumed by only one input")
        );
    }
    for args in [
        vec!["show", "missing", "--accept-partial"],
        vec![
            "edit",
            "missing",
            "--accept-partial",
            "--set",
            "record/repeater/0/name=X",
            "--output",
            "-",
        ],
    ] {
        let out = cli(&args, b"");
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn ur_logical_namespace_expands_order_count_and_preserves_raw_slot_boundary() {
    let source = ur_fixture();
    let out = cli(
        &[
            "show",
            "-",
            "--key",
            "record/ur-list/count",
            "--key",
            "record/ur-list/0/name",
            "--key",
            "record/ur-list/3/valid",
            "--key",
            "record/ur/0/name",
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows = json(&out);
    assert_eq!(rows[0]["value"], 3);
    assert_eq!(rows[1]["value"]["utf8"], "SLASH UR NAME   ");
    assert_eq!(rows[2]["value"], false);
    assert_eq!(rows[3]["value"]["utf8"], "WILDCARD NAME   ");
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "record/ur-list/0/name=LOGICAL NAME",
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
    assert_eq!(image.comment_bytes(), b"RECORD META");
    let working = cs705::icf::rows::WorkingUr::from_icf(&image).unwrap();
    assert_eq!(working.count(), 3);
    let mut holder = record::reset_holder(0x0d, 0, 0);
    working.load_into(0, &mut holder).unwrap();
    assert_eq!(holder.name(), b"LOGICAL NAME    ");
    assert_eq!(
        holder.callsign(cs705::icf::MemoryCallsign::Your),
        b"/ABCDEFA"
    );
    for (assignment, code) in [
        ("record/ur-list/3/name=NO IMPLICIT INSERT", 6),
        ("record/ur-list/count=5", 3),
        ("record/ur-list/0/valid=false", 3),
    ] {
        let out = cli(
            &["edit", "-", "--set", assignment, "--output", "-"],
            &source,
        );
        assert_eq!(out.status.code(), Some(code));
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn ur_rows_use_actual_t_and_keep_late_reference_failure_prefix() {
    let files = Files::new();
    let source = ur_fixture();
    let mut t = vec![0; 0x4997a];
    t[0x3c901..0x3c903].copy_from_slice(&0x1234u16.to_le_bytes());
    let t = files.write("T", &t);
    let short = files.write("shortT", b"");
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "ur",
            "--reference-defaults",
            p(&t),
            "insert",
            "0",
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
    let image = Icf::parse(&out.stdout).unwrap();
    assert_eq!(image.comment_bytes(), b"RECORD META");
    assert_eq!(&image.memory()[0x3c901..0x3c903], &0x1234u16.to_le_bytes());
    let working = cs705::icf::rows::WorkingUr::from_icf(&image).unwrap();
    assert_eq!(working.count(), 4);
    let mut h = record::reset_holder(0x0d, 0, 0);
    working.load_into(1, &mut h).unwrap();
    assert_eq!(h.callsign(cs705::icf::MemoryCallsign::Your), b"CALLSIGN");
    working.load_into(2, &mut h).unwrap();
    assert_eq!(h.name(), b"WILDCARD NAME   ");
    let cancel = cli(
        &[
            "row",
            "-",
            "--sheet",
            "ur",
            "--reference-defaults",
            p(&t),
            "delete",
            "0",
            "1",
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(cancel.status.code(), Some(6));
    assert!(cancel.stdout.is_empty());
    let missing = cli(
        &[
            "row", "-", "--sheet", "ur", "delete", "0", "1", "--yes", "--output", "-",
        ],
        &source,
    );
    assert_eq!(missing.status.code(), Some(6));
    assert!(missing.stdout.is_empty());
    let failed = cli(
        &[
            "row",
            "-",
            "--sheet",
            "ur",
            "--reference-defaults",
            p(&short),
            "delete",
            "0",
            "1",
            "--yes",
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(failed.status.code(), Some(3));
    assert!(failed.stdout.is_empty());
    let partial = cli(
        &[
            "row",
            "-",
            "--sheet",
            "ur",
            "--reference-defaults",
            p(&short),
            "--accept-partial",
            "delete",
            "0",
            "1",
            "--yes",
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(partial.status.code(), Some(5));
    let image = Icf::parse(&partial.stdout).unwrap();
    let working = cs705::icf::rows::WorkingUr::from_icf(&image).unwrap();
    assert_eq!(working.count(), 2);
    working.load_into(1, &mut h).unwrap();
    assert_eq!(h.name(), b"EXACT NAME      ");
    assert_eq!(&image.memory()[0x3c901..0x3c903], &0x4567u16.to_le_bytes());
    let duplicate = cli(
        &[
            "row",
            "-",
            "--sheet",
            "ur",
            "--reference-defaults",
            "-",
            "insert",
            "0",
            "0",
            "--output",
            "-",
        ],
        b"not an image",
    );
    assert_eq!(duplicate.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&duplicate.stderr)
            .contains("stdin may be consumed by only one input")
    );
    let mut full = Icf::parse(&source).unwrap();
    let order: Vec<u8> = (0..300u16).flat_map(u16::to_le_bytes).collect();
    full.patch_memory(&[(0x3c0a7, &order)]).unwrap();
    let out = cli(
        &[
            "row", "-", "--sheet", "ur", "insert", "0", "299", "--output", "-",
        ],
        &full.to_bytes(),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        cs705::icf::rows::WorkingUr::from_icf(&Icf::parse(&out.stdout).unwrap())
            .unwrap()
            .count(),
        300
    );
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "ur",
            "--reference-defaults",
            p(&short),
            "--accept-partial",
            "delete",
            "0",
            "1",
            "--yes",
            "--dry-run",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(5));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("\"published\": false"));
    let target = files.write("protected", b"KEEP");
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "ur",
            "--reference-defaults",
            p(&t),
            "delete",
            "0",
            "1",
            "--yes",
            "--output",
            p(&target),
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(fs::read(&target).unwrap(), b"KEEP");
}

#[test]
fn repeater_clipboard_kinds_move_delete_and_dry_run_keep_document_metadata() {
    let files = Files::new();
    let q = files.write("Q", &context(0));
    let source = repeater_fixture();
    let copy = cli(
        &[
            "row",
            "-",
            "--sheet",
            "call",
            "--show-both",
            "false",
            "copy",
            "100",
            "0",
            "--output",
            "-",
        ],
        &fixture(),
    );
    assert!(copy.status.success());
    let call = files.write("call-clipboard", &copy.stdout);
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "paste",
            "2",
            "0",
            "--clipboard",
            p(&call),
            "--yes",
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    let copy = cli(
        &[
            "row",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "copy",
            "2",
            "0",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(copy.status.success());
    let mut wrong = json(&copy);
    wrong["records"][0]["bytes"][0] = json!(3);
    let wrong = files.write("bad-header", &serde_json::to_vec(&wrong).unwrap());
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "paste",
            "2",
            "0",
            "--clipboard",
            p(&wrong),
            "--yes",
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "move",
            "2",
            "0",
            "--destination-group",
            "2",
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
    assert_eq!(
        repeater_holder(&out.stdout, 0).channel_field(ChannelField::FrequencyHz),
        145_000_000
    );
    assert_eq!(
        repeater_holder(&out.stdout, 1).channel_field(ChannelField::FrequencyHz),
        145_000_002
    );
    assert_eq!(Icf::parse(&out.stdout).unwrap().extra_data(), 3);
    let mut generation = vec![0; 0x4997a];
    generation[0x3c8b9..0x3c8bb].copy_from_slice(&0x1234u16.to_le_bytes());
    generation[0x3c903..0x3c905].copy_from_slice(&0x5678u16.to_le_bytes());
    let g = files.write("G", &generation);
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "--generation",
            p(&g),
            "delete",
            "2",
            "0",
            "--yes",
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
    assert_eq!(
        cs705::repeater::groups::Groups::from_icf(&image)
            .unwrap()
            .counts[2],
        2
    );
    assert_eq!(&image.memory()[0x3c8b9..0x3c8bb], &0x1234u16.to_le_bytes());
    assert_eq!(&image.memory()[0x3c903..0x3c905], &0x5678u16.to_le_bytes());
    assert_eq!(image.comment_bytes(), b"RECORD META");
    assert_eq!(image.extra_data(), 3);
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "repeater",
            "--record-context",
            p(&q),
            "--generation",
            p(&g),
            "delete",
            "2",
            "0",
            "--yes",
            "--dry-run",
        ],
        &source,
    );
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

fn history_fixture() -> Vec<u8> {
    let source = ur_fixture();
    let image = Icf::parse(&source).unwrap();
    let defaults = cs705::repeater::working::DefaultContext::from_context(
        &cs705::icf::context::Context::from_bytes(context(0)),
        None,
    )
    .unwrap();
    let mut repeaters = cs705::repeater::working::Working::from_icf(&image, defaults).unwrap();
    for index in 1..3 {
        let mut h = record::reset_holder(0x0c, index, 0);
        repeaters.load_into(index as usize, &mut h).unwrap();
        h.set_repeater_text(record::RepeaterText::Call, b"FM CALL ");
        h.set_repeater_field(record::RepeaterField::Mode, index - 1);
        h.set_channel_field(ChannelField::Duplex, index);
        h.set_channel_field(ChannelField::FrequencyHz, 146_123_456);
        h.set_name(if index == 1 {
            b"FIRST FM        "
        } else {
            b"LATER FM        "
        });
        repeaters.save(index as usize, &h).unwrap();
    }
    let mut image = repeaters.to_icf().unwrap();
    image
        .patch_memory(&[
            (0x3c331, &[0, 255]),
            (0x32935, b"REPEAT  "),
            (0x3c33b, &[5, 6]),
            (0x32985 + 12 * 5, b"FM CALL "),
            (0x32985 + 12 * 5 + 8, &146_123_456u32.to_le_bytes()),
            (0x32985 + 12 * 6, b"UNKNOWN "),
            (0x32985 + 12 * 6 + 8, &123_456_789u32.to_le_bytes()),
            (0x3c34f, &[0, 1, 2, 3]),
            (0x32a4d, b"UR CALLAUR CALLB/REPEATAUNKNOWN "),
        ])
        .unwrap();
    image.to_bytes()
}

#[test]
fn history_resolvers_use_concrete_lists_without_changing_images() {
    let source = history_fixture();
    let image = Icf::parse(&source).unwrap();
    let defaults = cs705::repeater::working::DefaultContext::from_context(
        &cs705::icf::context::Context::from_bytes(context(0)),
        None,
    )
    .unwrap();
    let repeaters = cs705::repeater::working::Working::from_icf(&image, defaults).unwrap();
    let ur = cs705::icf::rows::WorkingUr::from_icf(&image).unwrap();
    use record::HistoryQuery as Q;
    for (query, expected) in [
        (Q::Ur { call: *b"UR CALLA" }, 2),
        (Q::UpperUr { call: *b"UR CALLB" }, 1),
        (Q::Ur { call: *b"        " }, 300),
        (Q::UpperUr { call: *b"/ABCDEFA" }, 300),
        (Q::ToRepeater { call: *b"/REPEATA" }, 0),
        (Q::ToRepeater { call: *b"REPEAT A" }, 65535),
        (
            Q::FromFm {
                call: *b"FM CALL ",
                frequency_hz: 146_123_456,
                limit: 0x10000,
            },
            65535,
        ),
        (
            Q::FromFm {
                call: *b"FM CALL ",
                frequency_hz: 146_123_456,
                limit: 3,
            },
            1,
        ),
        (
            Q::FromFm {
                call: *b"FM CALL ",
                frequency_hz: 146_123_457,
                limit: 3,
            },
            65535,
        ),
    ] {
        assert_eq!(
            record::resolve_history_query(&repeaters, &ur, query).unwrap(),
            expected
        );
    }
    let mut h = record::reset_holder(0x10, 0, 0xdead_beef);
    record::load_history_storage_into(&image, 0x10, 0, &repeaters, &ur, &mut h).unwrap();
    assert_eq!((h.kind(), h.index(), h.binding()), (0x10, 0, 0xdead_beef));
    assert_eq!(h.history_call(), b"FM CALL ");
    assert!(h.history_supplemented());
    assert!(
        record::resolve_history_query(
            &repeaters,
            &ur,
            Q::FromFm {
                call: *b"MISSING ",
                frequency_hz: 0,
                limit: 2501
            }
        )
        .is_err()
    );
    assert_eq!(image.to_bytes(), source);
}

#[test]
fn normal_native_copy_generates_knm_and_paste_allocates_without_losing_metadata() {
    let files = Files::new();
    let source = normal_fixture();
    let g = generation(&files);
    let copy = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--show-both",
            "false",
            "copy",
            "0",
            "1",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(
        copy.status.success(),
        "{}",
        String::from_utf8_lossy(&copy.stderr)
    );
    let value = json(&copy);
    assert_eq!(value["version"], 2);
    assert_eq!(value["mode"], 3);
    assert_eq!(value["declared_count"], 2);
    assert_eq!(value["logical_count"], 1);
    let clipboard = files.write("native-clipboard", &copy.stdout);
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--show-both",
            "false",
            "--generation",
            p(&g),
            "paste",
            "0",
            "5",
            "--clipboard",
            p(&clipboard),
            "--yes",
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
    assert_eq!(image.comment_bytes(), b"NATIVE NORMAL");
    assert_eq!(image.extra_data(), 3);
    assert_eq!(&image.memory()[0x36821..0x36823], &0x1234u16.to_le_bytes());
    assert_eq!(&image.memory()[0x3c84c..0x3c84e], &0x1234u16.to_le_bytes());
    assert_eq!(
        normal_holder(&out.stdout, 10).channel_field(ChannelField::FrequencyHz),
        145_000_002
    );
    assert_eq!(
        normal_holder(&out.stdout, 11).channel_field(ChannelField::FrequencyHz),
        145_000_003
    );
    assert_eq!(normal_holder(&out.stdout, 10).offset_hz(), 149);
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--side-view",
            "--show-both",
            "false",
            "copy",
            "0",
            "1",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success());
    let value = json(&out);
    assert_eq!(
        (
            value["mode"].as_u64(),
            value["declared_count"].as_u64(),
            value["logical_count"].as_u64()
        ),
        (Some(2), Some(1), Some(1))
    );
    assert_eq!(value["records"][0]["bytes"][4], 1);
    assert_eq!(value["records"][0]["bytes"][0x24], 0);
    let out = cli(
        &[
            "row", "-", "--sheet", "memory", "copy", "0", "0", "--output", "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(6));
    assert!(out.stdout.is_empty());
    let target = files.write("existing-native", b"KEEP");
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--show-both",
            "false",
            "copy",
            "0",
            "0",
            "--output",
            p(&target),
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(fs::read(target).unwrap(), b"KEEP");
}

#[test]
fn normal_native_split_delete_step1_and_late_g_prefix_are_distinct() {
    let source = normal_fixture();
    let files = Files::new();
    let g = generation(&files);
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--side-view",
            "delete",
            "0",
            "0",
            "--yes",
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
    assert_eq!(
        normal_holder(&out.stdout, 0).channel_field(ChannelField::FrequencyHz),
        145_000_001
    );
    assert_eq!(normal_holder(&out.stdout, 0).split(), 0);
    assert_eq!(
        cs705::icf::rows::NormalMemory::from_icf(&Icf::parse(&out.stdout).unwrap())
            .unwrap()
            .used_slots(),
        3
    );
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--side-view",
            "--show-both",
            "false",
            "move",
            "0",
            "1",
            "--destination-group",
            "0",
            "--destination-index",
            "0",
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
    assert_eq!(
        normal_holder(&out.stdout, 0).channel_field(ChannelField::FrequencyHz),
        145_000_001
    );
    assert_eq!(
        normal_holder(&out.stdout, 1).channel_field(ChannelField::FrequencyHz),
        145_000_000
    );
    assert_eq!(
        &Icf::parse(&out.stdout).unwrap().memory()[0x392fa..0x392fd],
        &[0, 2, 0]
    );
    let args = [
        "row",
        "-",
        "--sheet",
        "memory",
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
    ];
    let out = cli(&args, &source);
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--show-both",
            "false",
            "--accept-partial",
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
    assert_eq!(out.status.code(), Some(5));
    assert_eq!(
        normal_holder(&out.stdout, 0).channel_field(ChannelField::FrequencyHz),
        145_000_002
    );
    assert_eq!(
        Icf::parse(&out.stdout).unwrap().comment_bytes(),
        b"NATIVE NORMAL"
    );
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--generation",
            p(&g),
            "delete",
            "0",
            "0",
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(6));
    assert!(out.stdout.is_empty());
    let out = cli(
        &[
            "row", "-", "--sheet", "memory", "delete", "0", "99", "--output", "-",
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn normal_native_range_and_special_copy_sources_share_one_destination() {
    let files = Files::new();
    let source = normal_fixture();
    let g = generation(&files);
    let inserted = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--generation",
            p(&g),
            "insert",
            "0",
            "1",
            "--yes",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(inserted.status.success(), "native normal insert");
    assert!(!normal_holder(&inserted.stdout, 2).valid());
    assert_eq!(
        normal_holder(&inserted.stdout, 4).channel_field(ChannelField::FrequencyHz),
        145_000_002
    );
    assert_eq!(
        &Icf::parse(&inserted.stdout).unwrap().memory()[0x392fb..0x392fd],
        &3u16.to_le_bytes()
    );
    let mut image = Icf::parse(&source).unwrap();
    for kind in [4, 5] {
        let mut h = record::reset_holder(kind, 1, 0);
        h.set_mode(13);
        h.set_filter(1);
        h.set_channel_field(ChannelField::FrequencyHz, 146_987_654);
        // The document setter is the shared codec adapter, not a second raw36 recipe.
        let mut memory = image.memory().to_vec();
        record_image::store_compact_into(&mut memory, kind, 1, &mut h, |_, _| Ok(())).unwrap();
        image.patch_memory(&[(0, &memory)]).unwrap();
        let sheet = if kind == 4 { "bandstack" } else { "memopad" };
        let copy = cli(
            &[
                "row", "-", "--sheet", sheet, "copy", "0", "1", "--output", "-",
            ],
            &image.to_bytes(),
        );
        assert!(
            copy.status.success(),
            "{}",
            String::from_utf8_lossy(&copy.stderr)
        );
        assert_eq!(json(&copy)["kind"], kind);
        let clipboard = files.write(&format!("special{kind}"), &copy.stdout);
        let out = cli(
            &[
                "row",
                "-",
                "--sheet",
                "memory",
                "--generation",
                p(&g),
                "--show-both",
                "false",
                "paste",
                "0",
                "8",
                "--clipboard",
                p(&clipboard),
                "--yes",
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
        assert_eq!(
            normal_holder(&out.stdout, 16).channel_field(ChannelField::FrequencyHz),
            146_987_654
        );
        assert_eq!(
            normal_holder(&out.stdout, 17).channel_field(ChannelField::FrequencyHz),
            146_987_654
        );
    }
    let q = files.write("band-restricted-Q", &context(0));
    let copied = cli(
        &[
            "row",
            "-",
            "--sheet",
            "bandstack",
            "--end-index",
            "30",
            "--record-context",
            p(&q),
            "copy",
            "0",
            "26",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(copied.status.success(), "band restricted Copy");
    let packet = json(&copied);
    assert_eq!(packet["declared_count"], 2);
    assert_eq!(packet["records"][0]["bytes"][4], 26);
    assert_eq!(packet["records"][1]["bytes"][4], 30);
    let mut enabled = context(0);
    enabled[15] = 1;
    let q = files.write("band-enabled-Q", &enabled);
    let copied = cli(
        &[
            "row",
            "-",
            "--sheet",
            "bandstack",
            "--end-index",
            "30",
            "--record-context",
            p(&q),
            "copy",
            "0",
            "26",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(copied.status.success(), "band enabled Copy");
    assert_eq!(json(&copied)["declared_count"], 5);
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--end-index",
            "1",
            "--generation",
            p(&g),
            "clear",
            "0",
            "0",
            "--yes",
            "--dry-run",
        ],
        &source,
    );
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--show-both",
            "false",
            "--generation",
            "-",
            "paste",
            "0",
            "0",
            "--clipboard",
            "-",
            "--yes",
            "--output",
            "-",
        ],
        b"not an image",
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("stdin may be consumed by only one input")
    );
}

#[test]
fn native_memory_editor_uses_captured_cache_and_pending_registration_state() {
    let files = Files::new();
    let source = normal_fixture();
    let mut g = vec![0; 0x4997a];
    g[0x37823..0x37823 + 27].fill(9);
    g[0x3c84f + 5] = 1;
    g[0x3c84f + 7] = 29;
    let g = files.write("editorG", &g);
    let state = files.write(
        "mode-state",
        &serde_json::to_vec(&initialized_normal_state(&source, 0, None)).unwrap(),
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--generation",
            p(&g),
            "--set",
            "record/native-memory/accepted/0/2/frequency-hz=145000002",
            "--set",
            "record/native-memory/accepted/0/2/mode=1",
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
    let h = normal_holder(&out.stdout, 2);
    assert_eq!(h.mode(), 1);
    assert_eq!(h.filter(), 9);
    assert_eq!(
        Icf::parse(&out.stdout).unwrap().comment_bytes(),
        b"NATIVE NORMAL"
    );
    let state = reported_normal_state(&out, "accepted mode");
    assert_eq!(state["mode_cache"][0x58], 13);
    assert_eq!(state["mode_cache"][0x0c].as_u64().unwrap() & 1, 1);
    let state = files.write("data-filter-state", &serde_json::to_vec(&state).unwrap());
    let data_filter = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--generation",
            p(&g),
            "--set",
            "record/native-memory/0/2/data=1",
            "--set",
            "record/native-memory/0/2/filter=3",
            "--output",
            "-",
        ],
        &out.stdout,
    );
    assert!(data_filter.status.success(), "accepted DATA and filter");
    let h = normal_holder(&data_filter.stdout, 2);
    assert_eq!(h.data(), 1);
    assert_eq!(h.filter(), 3);
    assert_eq!(
        reported_normal_state(&data_filter, "explicit filter clears cached-valid")["filter_cache"]
            [0x0c]
            .as_u64()
            .unwrap()
            & 1,
        0
    );
    let state = files.write(
        "pending-state",
        &serde_json::to_vec(&initialized_normal_state(&source, 3, None)).unwrap(),
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            "record/native-memory/accepted/0/10/name=PENDING",
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
    let pending = reported_normal_state(&out, "pending name");
    assert_eq!(pending["pending"]["active"], true);
    assert!(!normal_holder(&out.stdout, 10).valid());
    let state = files.write("registration-state", &serde_json::to_vec(&pending).unwrap());
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--generation",
            p(&g),
            "--set",
            "record/native-memory/accepted/0/10/frequency-hz=146000000",
            "--output",
            "-",
        ],
        &out.stdout,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for side in [10, 11] {
        let h = normal_holder(&out.stdout, side);
        assert!(h.valid());
        assert_eq!(h.channel_field(ChannelField::FrequencyHz), 146_000_000);
        assert_eq!(h.name(), b"PENDING         ");
        // F1DC index2 passes class1 to F20C; it never calls EFCC.
        assert_eq!(h.offset_hz(), 600_000);
        assert_eq!(h.tx_tone(), 29);
    }
    assert_eq!(
        reported_normal_state(&out, "registration")["pending"]["active"],
        false
    );
    let out = cli(
        &[
            "edit",
            "-",
            "--set",
            "record/native-memory/accepted/0/2/mode=1",
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(6));
    assert!(out.stdout.is_empty());
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            "-",
            "--set",
            "record/native-memory/accepted/0/2/mode=1",
            "--output",
            "-",
        ],
        b"not input",
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("stdin may be consumed by only one input")
    );
}

#[test]
fn native_memory_initialized_generation_keeps_its_source_and_rejects_mixed_restore() {
    let files = Files::new();
    let source = normal_fixture();
    let mut state = initialized_normal_state(&source, 5, Some(0x411));
    assert_eq!(state["range_selected"], 0);
    assert_eq!(state["range_source"], 0);
    assert_eq!(state["filter_enabled"], 0);
    state["range_langid"] = serde_json::json!(0x409);
    let captured = files.write("host-changed", &serde_json::to_vec(&state).unwrap());
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&captured),
            "--set",
            "record/native-memory/accepted/0/2/mode=1",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "completed generation after host change"
    );
    assert_eq!(normal_holder(&out.stdout, 2).mode(), 1);
    assert_eq!(
        reported_normal_state(&out, "fixed source")["range_source"],
        0
    );
    state["range_source"] = serde_json::json!(1);
    let captured = files.write("mixed-source", &serde_json::to_vec(&state).unwrap());
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&captured),
            "--set",
            "record/native-memory/accepted/0/2/mode=1",
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    state["range_selected"] = serde_json::json!(16);
    state["range_langid"] = Value::Null;
    state["range_source"] = serde_json::json!(0);
    let captured = files.write("missing-host-input", &serde_json::to_vec(&state).unwrap());
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&captured),
            "--set",
            "record/native-memory/accepted/0/2/mode=1",
            "--output",
            "-",
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    let state = initialized_normal_state(&source, 0, None);
    let captured = files.write("reject-candidate", &serde_json::to_vec(&state).unwrap());
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&captured),
            "--set",
            "record/native-memory/accepted/0/2/mode=24",
            "--output",
            "-",
        ],
        &source,
    );
    assert!(out.status.success(), "rejected native candidate");
    assert_eq!(normal_holder(&out.stdout, 2).mode(), 13);
    assert_eq!(
        serde_json::Deserializer::from_slice(&out.stderr)
            .into_iter::<Value>()
            .map(|v| v.unwrap())
            .filter(|v| v.get("normal_state").is_some())
            .count(),
        0
    );
}

#[test]
fn native_initial_registration_f1dc_uses_component_and_captured_source_not_live_q() {
    let files = Files::new();
    let source = normal_fixture();
    let generation = files.write("f1dc-G", &vec![0; 0x4997a]);
    let live_q = files.write("later-live-Q", &context(4));
    // These expectations come from F1DC's direct component argument to F20C,
    // not from EFCC(default frequency) or the later independent live Q.
    for (selected, frequency, offset) in [
        (1, 14_100_000, 500_000),
        (1, 146_520_000, 600_000),
        (1, 446_000_000, 5_000_000),
        (0, 145_000_000, 0),
        (3, 433_500_000, 7_600_000),
        (4, 145_000_000, 600_000),
    ] {
        let mut state = initialized_normal_state(&source, selected, None);
        state["range_langid"] = json!(0x411);
        let state = files.write("f1dc-state", &serde_json::to_vec(&state).unwrap());
        let assignment = format!("record/native-memory/accepted/0/10/frequency-hz={frequency}");
        let out = cli(
            &[
                "edit",
                "-",
                "--normal-state",
                p(&state),
                "--generation",
                p(&generation),
                "--record-context",
                p(&live_q),
                "--system-langid",
                "1033",
                "--set",
                &assignment,
                "--output",
                "-",
            ],
            &source,
        );
        assert!(
            out.status.success(),
            "source{selected}/frequency{frequency}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        for side in [10, 11] {
            let h = normal_holder(&out.stdout, side);
            assert!(h.valid());
            assert_eq!(h.channel_field(ChannelField::FrequencyHz), frequency);
            assert_eq!(h.offset_hz(), offset);
        }
        assert_eq!(
            reported_normal_state(&out, "captured F1DC source")["range_source"],
            selected
        );
        let image = Icf::parse(&out.stdout).unwrap();
        assert_eq!(image.comment_bytes(), b"NATIVE NORMAL");
        assert_eq!(image.extra_data(), 3);
    }
}

#[test]
fn native_initial_registration_g_and_later_failure_never_publish_an_edit_prefix() {
    let files = Files::new();
    let source = normal_fixture();
    let mut state = initialized_normal_state(&source, 1, None);
    state["pending"] = json!({"active":true,"index":10,"bytes":b"KEEP"});
    let state = files.write("f1dc-pending", &serde_json::to_vec(&state).unwrap());
    let target = files.path("f1dc-not-published.icf");
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--set",
            "record/native-memory/accepted/0/10/frequency-hz=146520000",
            "--output",
            p(&target),
        ],
        &source,
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(!target.exists());
    let generation = files.write("f1dc-G", &vec![0; 0x4997a]);
    let out = cli(
        &[
            "edit",
            "-",
            "--normal-state",
            p(&state),
            "--generation",
            p(&generation),
            "--set",
            "record/native-memory/accepted/0/10/frequency-hz=146520000",
            "--set",
            "record/native-memory/0/10/not-a-field=0",
            "--output",
            p(&target),
        ],
        &source,
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(!target.exists());
    let reported = reported_normal_state(&out, "first registration completed before later error");
    assert_eq!(reported["pending"]["active"], false);
}

#[test]
fn from_to_history_searches_current_lists_with_exact_then_prefix_and_slash_rules() {
    let files = Files::new();
    let q = files.write("Q", &context(0));
    let source = history_fixture();
    let out = cli(
        &[
            "show",
            "-",
            "--record-context",
            p(&q),
            "--sheet",
            "from-dv",
            "--count",
            "2",
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(json(&out)[0]["supplemented"], true);
    assert_eq!(json(&out)[0]["frequency-hz"], 145_000_002);
    assert_eq!(json(&out)[1]["valid"], false);
    let out = cli(
        &[
            "show",
            "-",
            "--record-context",
            p(&q),
            "--sheet",
            "from-fm",
            "--count",
            "2",
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(json(&out)[0]["name"]["utf8"], "FIRST FM        ");
    assert_eq!(json(&out)[0]["mode"], 0);
    assert_eq!(json(&out)[1]["frequency-hz"], 123_456_789);
    assert_eq!(json(&out)[1]["name"], Value::Null);
    let out = cli(
        &[
            "show",
            "-",
            "--record-context",
            p(&q),
            "--sheet",
            "to",
            "--count",
            "4",
        ],
        &source,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows = json(&out);
    assert_eq!(rows[0]["name"]["utf8"], "EXACT NAME      ");
    assert_eq!(rows[1]["name"]["utf8"], "WILDCARD NAME   ");
    assert_eq!(rows[2]["name"]["utf8"], "REPEATER NAME   ");
    assert_eq!(rows[3]["supplemented"], false);
    let out = cli(
        &[
            "show",
            "-",
            "--record-context",
            p(&q),
            "--key",
            "record/from-fm/0/name",
            "--key",
            "record/to/1/name",
        ],
        &source,
    );
    assert!(out.status.success());
    assert_eq!(json(&out)[0]["value"]["utf8"], "FIRST FM        ");
    assert_eq!(json(&out)[1]["value"]["utf8"], "WILDCARD NAME   ");
    let out = cli(
        &["edit", "-", "--set", "record/to/0/name=NO", "--output", "-"],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

#[test]
fn routed_ur_rx_views_and_all_history_delete_maps_preserve_physical_payloads() {
    let source = fixture();
    let mut image = Icf::parse(&source).unwrap();
    let mut ur = record::reset_holder(0x0d, 299, 0);
    ur.set_name(b"LAST RAW UR     ");
    ur.set_callsign(cs705::icf::MemoryCallsign::Your, b"UR LAST ");
    record::store_callsign_storage_into(&mut image, 299, &ur).unwrap();
    let mut rank = [255; 50];
    rank[..6].copy_from_slice(&[3, 3, 1, 255, 6, 0]);
    image.patch_memory(&[(0x3c2ff, &rank)]).unwrap();
    let view = cli(
        &[
            "show", "-", "--sheet", "ur", "--start", "299", "--count", "2",
        ],
        &image.to_bytes(),
    );
    assert!(
        view.status.success(),
        "{}",
        String::from_utf8_lossy(&view.stderr)
    );
    assert_eq!(json(&view).as_array().unwrap().len(), 1);
    assert_eq!(json(&view)[0]["call"]["hex"], "5552204C41535420");
    let view = cli(
        &["show", "-", "--sheet", "rx", "--start", "0", "--count", "2"],
        &image.to_bytes(),
    );
    assert!(
        view.status.success(),
        "{}",
        String::from_utf8_lossy(&view.stderr)
    );
    assert_eq!(json(&view)[0]["valid"], true);
    assert_eq!(json(&view)[1]["valid"], true);
    for (sheet, at, capacity) in [
        ("from-dv", 0x3c331, 10),
        ("from-fm", 0x3c33b, 10),
        ("to", 0x3c34f, 20),
        ("rx", 0x3c2ff, 50),
    ] {
        let mut map = vec![255; capacity];
        map[..4].copy_from_slice(&[4, 0, 7, 1]);
        image.patch_memory(&[(at, &map)]).unwrap();
        let bytes = image.to_bytes();
        let cancel = cli(
            &[
                "row", "-", "--sheet", sheet, "delete", "0", "0", "--output", "-",
            ],
            &bytes,
        );
        assert_eq!(cancel.status.code(), Some(6));
        assert!(cancel.stdout.is_empty());
        let out = cli(
            &[
                "row", "-", "--sheet", sheet, "delete", "0", "0", "--yes", "--output", "-",
            ],
            &bytes,
        );
        assert!(
            out.status.success(),
            "{sheet}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let result = Icf::parse(&out.stdout).unwrap();
        let mut expected = image.memory().to_vec();
        if sheet == "rx" {
            expected[at..at + 4].copy_from_slice(&[3, 255, 6, 0]);
        } else {
            expected[at..at + 3].copy_from_slice(&[0, 7, 1]);
            expected[at + 3..at + capacity].fill(255);
        }
        assert_eq!(result.memory(), expected, "{sheet} must edit only its map");
        let mut empty = vec![255; capacity];
        if sheet == "rx" {
            empty[1] = 4;
        }
        image.patch_memory(&[(at, &empty)]).unwrap();
        let empty = image.to_bytes();
        let out = cli(
            &[
                "row", "-", "--sheet", sheet, "delete", "0", "0", "--output", "-",
            ],
            &empty,
        );
        assert!(
            out.status.success(),
            "empty {sheet}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.stdout.is_empty(),
            "empty history delete has no approval"
        );
    }
}

#[test]
fn partial_approval_does_not_publish_a_failure_before_any_mutation() {
    let files = Files::new();
    let source = normal_fixture();
    let output = files.write("unchanged-error", b"KEEP");
    let out = cli(
        &[
            "row",
            "-",
            "--sheet",
            "memory",
            "--accept-partial",
            "delete",
            "100",
            "0",
            "--yes",
            "--overwrite",
            "-o",
            p(&output),
        ],
        &source,
    );
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(fs::read(output).unwrap(), b"KEEP");
}

#[test]
fn history_delete_range_uses_the_selected_width_including_holes() {
    let source = fixture();
    for (sheet, base, capacity) in [
        ("from-dv", 0x3c331, 10),
        ("from-fm", 0x3c33b, 10),
        ("to", 0x3c34f, 20),
        ("rx", 0x3c2ff, 50),
    ] {
        let mut image = Icf::parse(&source).unwrap();
        let mut map = vec![255; capacity];
        map[..5].copy_from_slice(&[0, 255, 2, 3, 4]);
        image.patch_memory(&[(base, &map)]).unwrap();
        let input = image.to_bytes();
        let args = [
            "row",
            "-",
            "--sheet",
            sheet,
            "--end-index",
            "2",
            "delete",
            "0",
            "1",
            "--output",
            "-",
        ];
        let declined = cli(&args, &input);
        assert_eq!(declined.status.code(), Some(6), "{sheet}");
        assert!(declined.stdout.is_empty());
        let mut approved = args.to_vec();
        approved.push("--yes");
        let out = cli(&approved, &input);
        assert!(
            out.status.success(),
            "{sheet}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let result = Icf::parse(&out.stdout).unwrap();
        let expected: &[u8] = if sheet == "rx" {
            &[0, 255, 255, 1, 2]
        } else {
            &[0, 3, 4, 255, 255]
        };
        assert_eq!(&result.memory()[base..base + 5], expected, "{sheet}");
        assert_eq!(&result.memory()[..base], &image.memory()[..base]);
        assert_eq!(
            &result.memory()[base + capacity..],
            &image.memory()[base + capacity..]
        );
    }
}
