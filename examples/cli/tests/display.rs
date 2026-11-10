//! One bounded software check; no native program, font API or device is run.
use cs705::{
    display::*,
    icf::{record, special::CompactKind},
    printing::{ViewKind, Views},
};
use serde_json::{Value, json};
use std::{fs, process::Command};

fn settings() -> Settings {
    Settings {
        language: 0,
        system_language: 0,
        font_percent: 100,
        toolbar: 1,
        split: 0,
        callsign_dialog: 1,
        auto_correct: 1,
        fixed_name: 0,
        column_bits: 255,
        column_bits_high: 0xaabbccdd,
    }
}
fn metrics() -> FontMetrics {
    FontMetrics {
        em: 16,
        advance: 8,
        line: 16,
        dpi: 96,
        padding: 1,
        explicit_height: 0,
    }
}
fn source() -> Vec<u8> {
    let mut m = vec![0; 0x4997a];
    m[0x355a1] = 6;
    m[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
    m[0x3c2ff..0x3c331].fill(255);
    for map in m[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
        map.copy_from_slice(&0xffff_u32.to_le_bytes());
    }
    m[0x3fcd4..0x3fcd8].copy_from_slice(&0_u32.to_le_bytes());
    for base in [0, 0xa21c] {
        m[base + 1..base + 5].copy_from_slice(&145_000_000_u32.to_le_bytes());
        m[base + 5] = record::encode_mode(13) << 4;
        m[base + 38] = record::encode_mode(16) << 4;
        m[base + 67..base + 83].copy_from_slice(b"DISPLAY CHECK   ");
    }
    for kind in [CompactKind::BandStack, CompactKind::Memopad] {
        let at = kind.offset(0).unwrap();
        m[at..at + 4].copy_from_slice(&145_000_000_u32.to_le_bytes());
        m[at + 4] = record::encode_mode(13) << 4;
    }
    m
}
fn menu(state: &mut State, image: &[u8], key: u32, index: u32, confirmation: Option<i32>) -> u8 {
    let mut flags = 0x25;
    state
        .menu(image, key, 13, index, &mut flags, &mut 0xa4, confirmation)
        .unwrap();
    assert_eq!(flags & 127, 0x25);
    flags
}
fn fake_caption_asset() -> Vec<u8> {
    let mut pe = vec![0; 0x10200];
    let w = |p: &mut [u8], at: usize, v: u16| p[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let d = |p: &mut [u8], at: usize, v: u32| p[at..at + 4].copy_from_slice(&v.to_le_bytes());
    pe[..2].copy_from_slice(b"MZ");
    d(&mut pe, 0x3c, 0x80);
    pe[0x80..0x84].copy_from_slice(b"PE\0\0");
    w(&mut pe, 0x84, 0x14c);
    w(&mut pe, 0x86, 1);
    w(&mut pe, 0x94, 96);
    w(&mut pe, 0x98, 0x10b);
    d(&mut pe, 0x98 + 28, 0x400000);
    let section = 0x98 + 96;
    d(&mut pe, section + 12, 0x3f0000);
    d(&mut pe, section + 16, 0x10000);
    d(&mut pe, section + 20, 0x200);
    d(&mut pe, 0x200 + 0x3a2c + 36 * 4, 2400);
    pe[0x200 + 0x90d0..0x200 + 0x90d0 + 6].copy_from_slice(b"%dHz \0");
    pe
}

#[test]
fn native_display_options_queries_geometry_and_cli() {
    let mut image = source();
    let mut views = Views::default();
    let s = settings();
    let pure = Settings {
        callsign_dialog: 2,
        auto_correct: 256,
        split: 2,
        toolbar: 2,
        fixed_name: 2,
        ..settings()
    };
    let mut query_state = State::new(pure.clone(), metrics(), [0, 0, 500, 30]).unwrap();
    for (key, index) in [
        (0x2b, 0),
        (0x2c, 0),
        (0x2d, 0),
        (0x2e, 0),
        (0x2f, 0),
        (0x30, 0),
        (0x31, 0),
        (0x36, 100),
        (0x37, 0),
        (0x38, 0),
        (0x40, 0),
        (0x41, 0),
        (0xe6, 0),
        (0xe9, 0),
    ] {
        let (mut a, mut b, mut c, mut d) = (0x25, 0xa4, 0x25, 0xa4);
        pure.query_menu(key, index, &mut a, &mut b).unwrap();
        query_state
            .menu(&[], key, 0, index, &mut c, &mut d, None)
            .unwrap();
        assert_eq!((a, b), (c, d));
        assert_eq!(a & 127, 0x25);
        assert_eq!(b & !1, 0xa4);
    }
    assert_eq!(query_state.settings, pure);
    assert!(query_state.queue.is_empty() && !query_state.font_changed);
    let mut holder = record::reset_holder(3, 0, 0);
    holder.set_valid(true);
    holder.set_channel_field(record::ChannelField::FrequencyHz, 123);
    holder.set_mode(13);
    assert_eq!(channel_cell(&holder, 0x107, 0).unwrap(), b"123");
    assert_eq!(channel_cell(&holder, 0x108, 0).unwrap(), b"13");
    assert!(channel_cell(&holder, 0x104, 0).is_err());
    for node in [2, 103, 401, 402] {
        assert_eq!(sql_mask(&image, node, &s, &mut views, None).unwrap(), 2);
    }
    assert_eq!(sql_mask(&[], 0, &s, &mut views, None).unwrap(), 0);
    for category in 1..=3 {
        let key = [0, 0x10d, 0x112, 0x114][category];
        for bits in 0..16 {
            let mut configured = settings();
            configured.column_bits = bits;
            assert_eq!(
                !column_hidden(key, false, &configured, 14),
                bits & 1 != 0 && bits & (1 << category) != 0
            );
        }
    }
    let mut state = State::new(s, metrics(), [2, 3, 500, 30]).unwrap();
    state.open(&image, ViewKind::Call, 0, None).unwrap();
    assert_eq!(state.current.as_ref().unwrap().rows.len(), 4);
    assert_eq!(state.current.as_ref().unwrap().sql_mask, 2);
    let id = state.current.as_ref().unwrap().view_id;
    state.views.states[id].current_row = 6;
    assert_ne!(menu(&mut state, &image, 0x2c, 0, None) & 128, 0);
    assert_eq!(
        state.queue[0],
        Queued {
            event: 0x27,
            argument: 1,
            value: 2
        }
    );
    state.views.query(&image, 0x3f, 0).unwrap();
    assert_eq!(state.queue[0].value, 2);
    assert_eq!(state.current.as_ref().unwrap().rows.len(), 4);
    state.drain(&image, None).unwrap();
    let grid = state.current.as_ref().unwrap();
    assert_eq!(grid.rows.len(), 8);
    assert_eq!(grid.sql_mask, 14);
    assert_eq!(state.views.states[grid.view_id].current_row, 7);
    assert_eq!(state.views.query(&image, 0x3f, 0).unwrap()[0], [0, 8]);
    menu(&mut state, &image, 0x2b, 0, None);
    state.drain(&image, None).unwrap();
    let grid = state.current.as_ref().unwrap();
    assert_eq!(grid.frozen_columns, 3);
    let positions = grid.column_positions(50);
    assert_eq!(positions[2], i64::from(grid.columns[2].x));
    assert_eq!(positions[3], i64::from(grid.columns[3].x) - 50);
    let old_height = grid.rows[0].height;
    menu(&mut state, &image, 0x36, 150, None);
    assert!(state.current.is_none() && state.font_changed);
    state.show(&image, None).unwrap();
    assert!(state.current.as_ref().unwrap().rows[0].height > old_height);
    menu(&mut state, &image, 0x38, 0, None);
    assert!(!state.toolbar.visible);
    assert_eq!(state.toolbar.occupied_height(), 0);
    assert_eq!(state.toolbar.placement, [2, 3, 500, 30]);
    assert!(
        state
            .toolbar
            .set_visible(true, |_| Err(std::io::Error::other(
                "native control failure"
            )))
            .is_err()
    );
    assert!(state.toolbar.visible);
    menu(&mut state, &image, 0x37, 1, Some(2));
    assert_eq!(state.settings.language, 0);
    menu(&mut state, &image, 0x37, 1, Some(1));
    assert_eq!(state.resource_language, 0);
    state.drain(&image, None).unwrap();
    assert_eq!(state.current.as_ref().unwrap().language, 1);
    state.settings.callsign_dialog = 2;
    let mut flags = 0x25;
    state
        .menu(&image, 0x40, 0, 0, &mut flags, &mut 0, None)
        .unwrap();
    assert_eq!(flags, 0x25);
    assert!(state.settings.input_options().callsign_dialog_enabled());
    menu(&mut state, &image, 0x40, 0, None);
    assert_eq!(state.settings.callsign_dialog, 0);
    assert_eq!(state.settings.column_bits_high, 0xaabbccdd);
    assert!(InputOptions::default().japanese().is_err());
    let mut noncanonical = State::new(
        Settings {
            split: 2,
            ..settings()
        },
        metrics(),
        [0, 0, 500, 30],
    )
    .unwrap();
    noncanonical.open(&image, ViewKind::Call, 0, None).unwrap();
    assert_eq!(noncanonical.current.as_ref().unwrap().rows.len(), 8);
    assert!(
        noncanonical
            .current
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .filter(|row| row.side == 1)
            .all(|row| !row.valid)
    );
    assert_eq!(
        Settings {
            font_percent: u32::MAX,
            callsign_dialog: 2,
            auto_correct: u32::MAX,
            ..settings()
        }
        .registry_loaded()
        .font_percent,
        25
    );
    // A queue refresh consumes the changed live data, and same-mask skips layout.
    state.settings.split = 0;
    menu(&mut state, &image, 0x2c, 0, None);
    state.drain(&image, None).unwrap();
    image[0xa21c + 5] = record::encode_mode(16) << 4;
    state.queue.push_back(Queued {
        event: 0x3c,
        argument: 1,
        value: 0,
    });
    state.drain(&image, None).unwrap();
    assert_eq!(state.current.as_ref().unwrap().sql_mask, 12);
    // All four Rx groups and shared GPS columns are actual width-zero consumers.
    let mut rx = State::new(settings(), metrics(), [0, 0, 500, 30]).unwrap();
    rx.open(&image, ViewKind::Rx, 0, None).unwrap();
    for key in 0xe6..=0xe9 {
        menu(&mut rx, &image, key, 0, None);
    }
    rx.drain(&image, None).unwrap();
    assert!(
        rx.current
            .as_ref()
            .unwrap()
            .columns
            .iter()
            .all(|c| c.hidden && c.width == 0)
    );
    menu(&mut rx, &image, 0xe8, 0, None);
    rx.drain(&image, None).unwrap();
    let columns = &rx.current.as_ref().unwrap().columns;
    assert!(!columns.iter().find(|c| c.key == 0x163).unwrap().hidden);
    assert!(!columns.iter().find(|c| c.key == 0x16f).unwrap().hidden);
    assert!(columns.iter().find(|c| c.key == 0x168).unwrap().hidden);
    let notice = notice_cell(None, NoticeParent::Ts, 0x342, 4, true)
        .unwrap()
        .unwrap();
    assert!(notice.ascii_event_disabled && !notice.editable);
    assert!(
        notice_cell(None, NoticeParent::Ts, 0x342, 3, true)
            .unwrap()
            .is_none()
    );
    assert!(notice_cell(None, NoticeParent::Filter, 0x12f, 36, true).is_err());
    assert_eq!(
        notice_cell(
            Some(&fake_caption_asset()),
            NoticeParent::Filter,
            0x12f,
            36,
            true
        )
        .unwrap()
        .unwrap()
        .text,
        b"2400Hz "
    );

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/private/cli-r3/fixtures/display-oracle")
        .join(std::process::id().to_string());
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("image.bin"), source()).unwrap();
    fs::write(
        root.join("metrics.json"),
        serde_json::to_vec(
            &json!({"em":16,"advance":8,"line":16,"dpi":96,"padding":1,"explicit_height":0}),
        )
        .unwrap(),
    )
    .unwrap();
    fs::write(root.join("config.json"),serde_json::to_vec(&json!({"native-display":{"c00":0,"c04":0,"c10":100,"c14":1,"c18":0,"c34":2,"c38":256,"c90":0,"c94":255,"c98":99},"fixed-name":true,"font-percent":120})).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cs705"))
        .args([
            "--config",
            root.join("config.json").to_str().unwrap(),
            "display",
            root.join("image.bin").to_str().unwrap(),
            "--oracle",
            "--working-image",
            "--metrics",
            root.join("metrics.json").to_str().unwrap(),
            "--sheet",
            "call",
            "--toolbar-placement",
            "0,0,500,30",
            "--menu",
            r#"{"key":44,"event":13,"index":0,"flags":37,"disabled":0,"confirmation":null}"#,
            "--drain",
            "--count",
            "2",
            "--output",
            "-",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["grid"]["row_count"], 8);
    assert_eq!(report["grid"]["sql_mask"], 14);
    assert_eq!(report["grid"]["frozen_columns"], 3);
    assert_eq!(report["settings"]["c34"], 2);
    assert_eq!(report["settings"]["c98"], 99);
    assert_eq!(report["settings"]["c10"], 120);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn independent_display_catalog_choices_fields_and_svg_need_no_assets() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/private/cli-r3/fixtures/display-english")
        .join(std::process::id().to_string());
    fs::create_dir_all(&root).unwrap();
    let path = root.join("input.icf");
    let mut raw = source()[..0x3c9c0].to_vec();
    raw[0x39346] = 1;
    raw[0x39347] = 255;
    let image = cs705::icf::Icf::from_memory(raw).unwrap();
    let before = image.to_bytes();
    fs::write(&path, &before).unwrap();
    let cli = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_cs705"))
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };
    let catalog = cli(&["display", "--list-fields", "-o", "-"]);
    let catalog: Value = serde_json::from_slice(&catalog.stdout).unwrap();
    assert!(
        catalog
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["key"] == "network/prefix" && f["label"] == "network prefix")
    );
    let choices = cli(&["display", "--choices", "network/prefix", "-o", "-"]);
    let choices: Value = serde_json::from_slice(&choices.stdout).unwrap();
    assert_eq!(choices["candidates"][255]["value"], 255);
    let keying = cs705::icf::settings::catalog()
        .iter()
        .find(|s| s.key == 0x2fc)
        .unwrap();
    let choice_path = format!("setting/{}/0", keying.name);
    let mut usb_choices = Vec::new();
    for key in [choice_path.as_str(), "A2-H56399-16"] {
        let missing_image = Command::new(env!("CARGO_BIN_EXE_cs705"))
            .args(["display", "--choices", key, "-o", "-"])
            .output()
            .unwrap();
        assert_eq!(missing_image.status.code(), Some(3));
        assert!(missing_image.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&missing_image.stderr)
                .contains("USB assignment choices require an input image")
        );
        let shown = cli(&[
            "display",
            path.to_str().unwrap(),
            "--choices",
            key,
            "-o",
            "-",
        ]);
        let shown: Value = serde_json::from_slice(&shown.stdout).unwrap();
        assert_eq!(shown["current"], 255);
        assert!(
            !shown["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["value"] == 1)
        );
        usb_choices.push(shown);
    }
    assert_eq!(usb_choices[0], usb_choices[1]);
    let shown = cli(&[
        "display",
        path.to_str().unwrap(),
        "--sheet",
        "call",
        "--count",
        "2",
        "-o",
        "-",
    ]);
    let rows: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    let field = cli(&[
        "display",
        path.to_str().unwrap(),
        "--field",
        "physical-memory-frequency-hz/0/Main",
        "-o",
        "-",
    ]);
    let field: Value = serde_json::from_slice(&field.stdout).unwrap();
    assert_eq!(field[0]["value"], 145_000_000);
    let svg = cli(&[
        "display",
        path.to_str().unwrap(),
        "--sheet",
        "call",
        "--count",
        "1",
        "--format",
        "svg",
        "-o",
        "-",
    ]);
    let svg = String::from_utf8(svg.stdout).unwrap();
    assert!(svg.starts_with("<svg ") && svg.ends_with("</svg>\n"));
    assert!(svg.contains("DISPLAY CHECK"));
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::remove_dir_all(root).unwrap();
}
