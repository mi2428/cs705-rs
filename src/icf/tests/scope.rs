use super::*;

// Literal storage contract; captions are not physical units or raw values.
const INITIAL_SCOPE: [(usize, u8); 16] = [
    (0x377c6, 0),
    (0x3808a, 0),
    (0x377c7, 0),
    (0x377d9, 0),
    (0x39725, 0),
    (0x39726, 0),
    (0x3972b, 0),
    (0x3972c, 0),
    (0x39736, 0),
    (0x39737, 0),
    (0x39727, 0),
    (0x39728, 0),
    (0x3972a, 0),
    (0x39738, 0),
    (0x39729, 0),
    (0x39739, 0),
];

// Independent Python hashlib/struct, not a product serializer:
// m = bytearray(i % 256 for i in range(0x3c9c0))
// m[0x355a1] = 6; m[-16:] = b"IcomCloneFormat3"
// for offset, value in INITIAL_SCOPE: m[offset] = value
// h = hashlib.md5(bytes.fromhex("440077554075"))
// for a in range(0, len(m), 32):
//     h.update(struct.pack(">IB", a, 32) + m[a:a+32])
// h.hexdigest().upper()
const SCOPE_CD: &str = "A9DC09BF7E9E76D421624D87A748DFC8";

fn scope_memory() -> Vec<u8> {
    let mut memory = synthetic_memory();
    for (at, raw) in INITIAL_SCOPE {
        memory[at] = raw;
    }
    memory
}

#[test]
fn scope_selectors_literal_domains_and_unknown_current_are_state_preserving() {
    let source = fixture_with_bytes(&INITIAL_SCOPE, SCOPE_CD);
    let original = source.clone();
    let mut expected = scope_memory();
    let mut image = Icf::parse(&source).unwrap();
    assert_setting_state(&image, &source, &expected);
    macro_rules! selector {
        ($get:ident, $set:ident, $at:literal, $cases:expr) => {
            assert_eq!(image.$get(), Ok($cases[0].1));
            for (raw, value) in $cases {
                image.$set(value).unwrap();
                expected[$at] = raw;
                assert_eq!(image.$get(), Ok(value));
                assert_setting_state(&image, &source, &expected);
            }
            for raw in [$cases.len() as u8, 0x80, 0xfe, 0xff] {
                // Inject raw current state independently of any setter.
                image.memory[$at] = raw;
                expected[$at] = raw;
                assert!(matches!(image.$get(), Err(Error::Unsupported { line, .. }) if line == 5 + $at / 32));
                assert_setting_state(&image, &source, &expected);
                for (_, value) in $cases {
                    assert!(matches!(image.$set(value), Err(Error::Unsupported { line, .. }) if line == 5 + $at / 32));
                    assert_setting_state(&image, &source, &expected);
                }
            }
            image.memory[$at] = 0;
            expected[$at] = 0;
        };
    }
    selector!(
        scope_span,
        set_scope_span,
        0x377c7,
        [
            (0, ScopeSpan::K2p5),
            (1, ScopeSpan::K5),
            (2, ScopeSpan::K10),
            (3, ScopeSpan::K25),
            (4, ScopeSpan::K50),
            (5, ScopeSpan::K100),
            (6, ScopeSpan::K250),
            (7, ScopeSpan::K500)
        ]
    );
    selector!(
        scope_sweep_speed,
        set_scope_sweep_speed,
        0x377d9,
        [
            (0, ScopeSweepSpeed::Fast),
            (1, ScopeSweepSpeed::Mid),
            (2, ScopeSweepSpeed::Slow)
        ]
    );
    selector!(
        scope_during_tx_center,
        set_scope_during_tx_center,
        0x39725,
        [(0, false), (1, true)]
    );
    selector!(
        scope_max_hold,
        set_scope_max_hold,
        0x39726,
        [
            (0, ScopeMaxHold::Off),
            (1, ScopeMaxHold::Hold10Seconds),
            (2, ScopeMaxHold::On)
        ]
    );
    selector!(
        scope_center_display,
        set_scope_center_display,
        0x3972b,
        [
            (0, ScopeCenterDisplay::FilterCenter),
            (1, ScopeCenterDisplay::CarrierPoint),
            (2, ScopeCenterDisplay::CarrierPointAbsolute)
        ]
    );
    selector!(
        scope_marker_position,
        set_scope_marker_position,
        0x3972c,
        [
            (0, ScopeMarkerPosition::FilterCenter),
            (1, ScopeMarkerPosition::CarrierPoint)
        ]
    );
    selector!(
        scope_vbw,
        set_scope_vbw,
        0x39736,
        [(0, ScopeVbw::Narrow), (1, ScopeVbw::Wide)]
    );
    selector!(
        scope_averaging,
        set_scope_averaging,
        0x39737,
        [
            (0, ScopeAveraging::Off),
            (1, ScopeAveraging::Two),
            (2, ScopeAveraging::Three),
            (3, ScopeAveraging::Four)
        ]
    );
    selector!(
        scope_waveform,
        set_scope_waveform,
        0x39727,
        [(0, ScopeWaveform::Fill), (1, ScopeWaveform::FillAndLine)]
    );
    selector!(
        scope_waterfall_display,
        set_scope_waterfall_display,
        0x39728,
        [(0, false), (1, true)]
    );
    selector!(
        scope_waterfall_speed,
        set_scope_waterfall_speed,
        0x3972a,
        [
            (0, ScopeWaterfallSpeed::Slow),
            (1, ScopeWaterfallSpeed::Mid),
            (2, ScopeWaterfallSpeed::Fast)
        ]
    );
    selector!(
        scope_waterfall_size_expand,
        set_scope_waterfall_size_expand,
        0x39738,
        [
            (0, ScopeWaterfallSize::Small),
            (1, ScopeWaterfallSize::Mid),
            (2, ScopeWaterfallSize::Large)
        ]
    );
    selector!(
        scope_waterfall_peak_color_level,
        set_scope_waterfall_peak_color_level,
        0x39729,
        [
            (0, ScopeWaterfallPeak::Grid1),
            (1, ScopeWaterfallPeak::Grid2),
            (2, ScopeWaterfallPeak::Grid3),
            (3, ScopeWaterfallPeak::Grid4),
            (4, ScopeWaterfallPeak::Grid5),
            (5, ScopeWaterfallPeak::Grid6),
            (6, ScopeWaterfallPeak::Grid7),
            (7, ScopeWaterfallPeak::Grid8)
        ]
    );
    selector!(
        scope_waterfall_marker_autohide,
        set_scope_waterfall_marker_autohide,
        0x39739,
        [(0, false), (1, true)]
    );
    assert_setting_state(&image, &source, &expected);
    assert_eq!(source, original);
}

#[test]
fn scope_mode_uses_two_nonadjacent_bytes_and_rejects_each_unknown_window() {
    let source = fixture_with_bytes(&INITIAL_SCOPE, SCOPE_CD);
    let original = source.clone();
    let mut expected = scope_memory();
    let mut image = Icf::parse(&source).unwrap();
    image.set_scope_span(ScopeSpan::K250).unwrap();
    expected[0x377c7] = 6;
    let cases = [
        ([0, 0], ScopeMode::Center),
        ([1, 0], ScopeMode::Fixed),
        ([0, 1], ScopeMode::ScrollCenter),
        ([1, 1], ScopeMode::ScrollFixed),
    ];
    for (pair, current) in cases {
        image.memory[0x377c6] = pair[0];
        image.memory[0x3808a] = pair[1];
        expected[0x377c6] = pair[0];
        expected[0x3808a] = pair[1];
        assert_eq!(image.scope_mode(), Ok(current));
        assert_setting_state(&image, &source, &expected);
        for (pair, next) in cases {
            image.set_scope_mode(next).unwrap();
            expected[0x377c6] = pair[0];
            expected[0x3808a] = pair[1];
            assert_eq!(image.scope_mode(), Ok(next));
            assert_eq!(image.scope_span(), Ok(ScopeSpan::K250));
            assert_setting_state(&image, &source, &expected);
        }
    }
    for (pair, error_at) in [
        ([2, 0], 0x377c6),
        ([0, 2], 0x3808a),
        ([0xfe, 1], 0x377c6),
        ([1, 0x83], 0x3808a),
        ([0xff, 0xff], 0x377c6),
    ] {
        image.memory[0x377c6] = pair[0];
        image.memory[0x3808a] = pair[1];
        expected[0x377c6] = pair[0];
        expected[0x3808a] = pair[1];
        let before = image.to_bytes();
        assert!(
            matches!(image.scope_mode(), Err(Error::Unsupported { line, .. }) if line == 5 + error_at / 32)
        );
        assert_setting_state(&image, &source, &expected);
        for (_, value) in cases {
            assert!(
                matches!(image.set_scope_mode(value), Err(Error::Unsupported { line, .. }) if line == 5 + error_at / 32)
            );
            assert_eq!(image.scope_span(), Ok(ScopeSpan::K250));
            assert_setting_state(&image, &source, &expected);
        }
        assert_eq!(image.to_bytes(), before);
        let reparsed = Icf::parse(&before).unwrap();
        assert_setting_state(&reparsed, &before, &expected);
        assert!(
            matches!(reparsed.scope_mode(), Err(Error::Unsupported { line, .. }) if line == 5 + error_at / 32)
        );
    }
    assert_eq!(source, original);
}

#[test]
fn scope_rgb_windows_and_eighteen_mixed_edits_roundtrip_without_late_error_mutation() {
    let mut expected = scope_memory();
    let source = fixture_with_memory(&expected, SCOPE_CD);
    let original = source.clone();
    let mut image = Icf::parse(&source).unwrap();
    assert_eq!(image.scope_waveform_color_current(), Ok([0x2d, 0x2e, 0x2f]));
    assert_eq!(image.scope_waveform_color_line(), Ok([0x30, 0x31, 0x32]));
    assert_eq!(
        image.scope_waveform_color_max_hold(),
        Ok([0x33, 0x34, 0x35])
    );
    image.set_scope_vbw(ScopeVbw::Wide).unwrap();
    expected[0x39736] = 1;
    macro_rules! color {
        ($get:ident, $set:ident, $start:literal, $end:literal) => {
            for rgb in [[0, 255, 1], [255, 0, 254], [1, 128, 255], [254, 127, 0]] {
                image.$set(rgb).unwrap();
                expected[$start..$end].copy_from_slice(&rgb);
                assert_eq!(image.$get(), Ok(rgb));
                assert_eq!(image.scope_vbw(), Ok(ScopeVbw::Wide));
                assert_setting_state(&image, &source, &expected);
            }
        };
    }
    color!(
        scope_waveform_color_current,
        set_scope_waveform_color_current,
        0x3972d,
        0x39730
    );
    color!(
        scope_waveform_color_line,
        set_scope_waveform_color_line,
        0x39730,
        0x39733
    );
    color!(
        scope_waveform_color_max_hold,
        set_scope_waveform_color_max_hold,
        0x39733,
        0x39736
    );

    image.set_scope_mode(ScopeMode::ScrollFixed).unwrap();
    image.set_scope_span(ScopeSpan::K250).unwrap();
    image.set_scope_sweep_speed(ScopeSweepSpeed::Mid).unwrap();
    image.set_scope_during_tx_center(true).unwrap();
    image
        .set_scope_max_hold(ScopeMaxHold::Hold10Seconds)
        .unwrap();
    image
        .set_scope_center_display(ScopeCenterDisplay::CarrierPointAbsolute)
        .unwrap();
    image
        .set_scope_marker_position(ScopeMarkerPosition::CarrierPoint)
        .unwrap();
    image.set_scope_vbw(ScopeVbw::Wide).unwrap();
    image.set_scope_averaging(ScopeAveraging::Four).unwrap();
    image
        .set_scope_waveform(ScopeWaveform::FillAndLine)
        .unwrap();
    image
        .set_scope_waveform_color_current([0, 255, 129])
        .unwrap();
    image.set_scope_waveform_color_line([254, 1, 128]).unwrap();
    image
        .set_scope_waveform_color_max_hold([127, 0, 255])
        .unwrap();
    image.set_scope_waterfall_display(true).unwrap();
    image
        .set_scope_waterfall_speed(ScopeWaterfallSpeed::Fast)
        .unwrap();
    image
        .set_scope_waterfall_size_expand(ScopeWaterfallSize::Large)
        .unwrap();
    image
        .set_scope_waterfall_peak_color_level(ScopeWaterfallPeak::Grid8)
        .unwrap();
    image.set_scope_waterfall_marker_autohide(true).unwrap();
    // These 25 bytes describe 18 fields, not a contiguous mode or RGBA store.
    for (at, raw) in [
        (0x377c6, 1),
        (0x3808a, 1),
        (0x377c7, 6),
        (0x377d9, 1),
        (0x39725, 1),
        (0x39726, 1),
        (0x3972b, 2),
        (0x3972c, 1),
        (0x39736, 1),
        (0x39737, 3),
        (0x39727, 1),
        (0x39728, 1),
        (0x3972a, 2),
        (0x39738, 2),
        (0x39729, 7),
        (0x39739, 1),
        (0x3972d, 0),
        (0x3972e, 255),
        (0x3972f, 129),
        (0x39730, 254),
        (0x39731, 1),
        (0x39732, 128),
        (0x39733, 127),
        (0x39734, 0),
        (0x39735, 255),
    ] {
        expected[at] = raw;
    }
    assert_setting_state(&image, &source, &expected);
    let serialized = image.to_bytes();
    // Independent P01 checksum of the literal 25 edits above, using the same
    // hashlib/struct recipe as SCOPE_CD, never product-generated expected bytes.
    assert!(serialized.ends_with(b"#CD=76BD93B8328B7632899219F507442163\r\n"));
    let parsed = Icf::parse(&serialized).unwrap();
    assert_setting_state(&parsed, &serialized, &expected);
    assert_eq!(parsed.scope_mode(), Ok(ScopeMode::ScrollFixed));
    assert_eq!(parsed.scope_span(), Ok(ScopeSpan::K250));
    assert_eq!(parsed.scope_sweep_speed(), Ok(ScopeSweepSpeed::Mid));
    assert_eq!(parsed.scope_during_tx_center(), Ok(true));
    assert_eq!(parsed.scope_max_hold(), Ok(ScopeMaxHold::Hold10Seconds));
    assert_eq!(
        parsed.scope_center_display(),
        Ok(ScopeCenterDisplay::CarrierPointAbsolute)
    );
    assert_eq!(
        parsed.scope_marker_position(),
        Ok(ScopeMarkerPosition::CarrierPoint)
    );
    assert_eq!(parsed.scope_vbw(), Ok(ScopeVbw::Wide));
    assert_eq!(parsed.scope_averaging(), Ok(ScopeAveraging::Four));
    assert_eq!(parsed.scope_waveform(), Ok(ScopeWaveform::FillAndLine));
    assert_eq!(parsed.scope_waveform_color_current(), Ok([0, 255, 129]));
    assert_eq!(parsed.scope_waveform_color_line(), Ok([254, 1, 128]));
    assert_eq!(parsed.scope_waveform_color_max_hold(), Ok([127, 0, 255]));
    assert_eq!(parsed.scope_waterfall_display(), Ok(true));
    assert_eq!(
        parsed.scope_waterfall_speed(),
        Ok(ScopeWaterfallSpeed::Fast)
    );
    assert_eq!(
        parsed.scope_waterfall_size_expand(),
        Ok(ScopeWaterfallSize::Large)
    );
    assert_eq!(
        parsed.scope_waterfall_peak_color_level(),
        Ok(ScopeWaterfallPeak::Grid8)
    );
    assert_eq!(parsed.scope_waterfall_marker_autohide(), Ok(true));

    // A later rejected edit retains every earlier successful field change.
    image.memory[0x39736] = 0xfe;
    expected[0x39736] = 0xfe;
    let before_error = image.to_bytes();
    for value in [ScopeVbw::Narrow, ScopeVbw::Wide] {
        assert!(
            matches!(image.set_scope_vbw(value), Err(Error::Unsupported { line, .. }) if line == 5 + 0x39736 / 32)
        );
        assert_setting_state(&image, &source, &expected);
        assert_eq!(image.to_bytes(), before_error);
    }
    let reparsed = Icf::parse(&before_error).unwrap();
    assert_setting_state(&reparsed, &before_error, &expected);
    assert!(
        matches!(reparsed.scope_vbw(), Err(Error::Unsupported { line, .. }) if line == 5 + 0x39736 / 32)
    );
    assert_eq!(source, original);
}
