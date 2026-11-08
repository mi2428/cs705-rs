use super::*;

// Independent P01 fixture/checksum recipe, using Python hashlib and struct:
// m = bytearray(i % 256 for i in range(0x3c9c0))
// m[0x355a1] = 6; m[-16:] = b"IcomCloneFormat3"
// Apply the literal window bytes below, then:
// h = hashlib.md5(bytes.fromhex("440077554075"))
// for a in range(0, len(m), 32):
//     h.update(struct.pack(">IB", a, 32) + m[a:a+32])
// h.hexdigest().upper()

#[test]
fn scope_reference_all_slots_canonical_steps_and_seven_literal_words() {
    let mut expected = synthetic_memory();
    expected[0x377db..0x377fd].fill(0xff);
    let source = fixture_with_memory(&expected, "D303A1E4B9BB8B140C8A0B038E0E24A9");
    let original = source.clone();
    let mut image = Icf::parse(&source).unwrap();
    for slot in 0..17 {
        assert_eq!(image.scope_reference_level_tenths_db(slot), Ok(65335));
    }
    assert_setting_state(&image, &source, &expected);
    for slot in 0..17 {
        let at = 0x377db + 2 * slot;
        // Enumerate the contract's 81 raw words independently of the setter.
        // A displayed half dB is five tenths, never a raw signed byte or float.
        for (step, raw) in (0_u16..=400).step_by(5).enumerate() {
            let value = -200 + 5 * step as i32;
            image
                .set_scope_reference_level_tenths_db(slot, value)
                .unwrap();
            expected[at..at + 2].copy_from_slice(&raw.to_le_bytes());
            assert_eq!(image.scope_reference_level_tenths_db(slot), Ok(value));
            assert_setting_state(&image, &source, &expected);
        }
    }
    // Explicit independent pairs catch bias, units and endian mistakes at
    // first, interior, adjacent and last physical words.
    for (slot, value, pair) in [
        (16, -200, [0x00, 0x00]),
        (0, -195, [0x05, 0x00]),
        (8, -5, [0xc3, 0x00]),
        (15, 0, [0xc8, 0x00]),
        (1, 5, [0xcd, 0x00]),
        (9, 195, [0x8b, 0x01]),
        (7, 200, [0x90, 0x01]),
    ] {
        let at = 0x377db + 2 * slot;
        image
            .set_scope_reference_level_tenths_db(slot, value)
            .unwrap();
        expected[at..at + 2].copy_from_slice(&pair);
        assert_eq!(image.scope_reference_level_tenths_db(slot), Ok(value));
        assert_setting_state(&image, &source, &expected);
    }
    assert_eq!(&image.memory()[0x377fb..0x377fd], [0x00, 0x00]);
    assert_eq!(image.memory()[0x377da], 0xda);
    assert_eq!(image.memory()[0x377fd], 0xfd);
    assert_eq!(image.memory()[0x377c7], 0xc7);
    assert_eq!(&image.memory()[0x379b9..0x379bb], [0xb9, 0xba]);
    let serialized = image.to_bytes();
    // Python P01 checksum after every slot ends at 9001, then the seven
    // literal slot/pair edits above. No product output generated this CD.
    assert!(serialized.ends_with(b"#CD=AB548BB9ED1E71E08E629E3B188ACBDD\r\n"));
    let parsed = Icf::parse(&serialized).unwrap();
    assert_setting_state(&parsed, &serialized, &expected);
    for (slot, value) in [
        (0, -195),
        (1, 5),
        (2, 200),
        (3, 200),
        (4, 200),
        (5, 200),
        (6, 200),
        (7, 200),
        (8, -5),
        (9, 195),
        (10, 200),
        (11, 200),
        (12, 200),
        (13, 200),
        (14, 200),
        (15, 0),
        (16, -200),
    ] {
        assert_eq!(parsed.scope_reference_level_tenths_db(slot), Ok(value));
    }
    assert_eq!(source, original);
}

#[test]
fn scope_reference_raw_words_bounds_priority_and_late_errors_preserve_state() {
    // All raw words are readable, even outside the new-value grid/domain.
    let words = [
        ([0x00, 0x00], -200),
        ([0x01, 0x00], -199),
        ([0x04, 0x00], -196),
        ([0x05, 0x00], -195),
        ([0xc7, 0x00], -1),
        ([0xc8, 0x00], 0),
        ([0xc9, 0x00], 1),
        ([0x90, 0x01], 200),
        ([0x91, 0x01], 201),
        ([0xff, 0x7f], 32567),
        ([0x00, 0x80], 32568),
        ([0x00, 0xff], 65080),
        ([0xfe, 0xff], 65334),
        ([0xff, 0xff], 65335),
        ([0x34, 0x12], 4460),
        ([0xcd, 0xab], 43781),
        ([0x00, 0x01], 56),
    ];
    let mut expected = synthetic_memory();
    for (slot, (pair, _)) in words.iter().enumerate() {
        let at = 0x377db + 2 * slot;
        expected[at..at + 2].copy_from_slice(pair);
    }
    let source = fixture_with_memory(&expected, "85210454A85ABD12B42614F9EEAD30D1");
    let original = source.clone();
    let mut image = Icf::parse(&source).unwrap();
    for (slot, (_, value)) in words.iter().enumerate() {
        assert_eq!(image.scope_reference_level_tenths_db(slot), Ok(*value));
        assert_setting_state(&image, &source, &expected);
    }
    let invalid_values = [
        i32::MIN,
        -205,
        -201,
        -199,
        -196,
        -1,
        1,
        4,
        6,
        199,
        201,
        205,
        i32::MAX,
    ];
    for slot in 0..17 {
        let at = 0x377db + 2 * slot;
        for value in invalid_values {
            assert!(matches!(
                image.set_scope_reference_level_tenths_db(slot, value),
                Err(Error::Invalid { line, .. }) if line == 5 + at / 32
            ));
            assert_setting_state(&image, &source, &expected);
        }
    }
    for slot in [17, 18, usize::MAX / 2, usize::MAX] {
        assert!(matches!(
            image.scope_reference_level_tenths_db(slot),
            Err(Error::Unsupported { line: 5, .. })
        ));
        assert_setting_state(&image, &source, &expected);
        // Slot validation wins over every invalid-value class, before any
        // slot multiplication or potentially overflowing value arithmetic.
        for value in invalid_values.into_iter().chain([-200, 0, 200]) {
            assert!(matches!(
                image.set_scope_reference_level_tenths_db(slot, value),
                Err(Error::Unsupported { line: 5, .. })
            ));
            assert_setting_state(&image, &source, &expected);
        }
    }
    // Explicit replacement of unknown current FFFF must succeed. No typed
    // current-domain guard may leak in from enum/bool storage helpers.
    for (slot, value, pair) in [
        (13, 200, [0x90, 0x01]),
        (16, -195, [0x05, 0x00]),
        (0, 5, [0xcd, 0x00]),
    ] {
        image
            .set_scope_reference_level_tenths_db(slot, value)
            .unwrap();
        let at = 0x377db + 2 * slot;
        expected[at..at + 2].copy_from_slice(&pair);
        assert_eq!(image.scope_reference_level_tenths_db(slot), Ok(value));
        assert_setting_state(&image, &source, &expected);
    }
    let before_error = image.to_bytes();
    for (slot, value, line_number) in [
        (16, i32::MIN, 5 + 0x377fb / 32),
        (0, i32::MAX, 5 + 0x377db / 32),
        (13, -1, 5 + 0x377f5 / 32),
    ] {
        assert!(matches!(
            image.set_scope_reference_level_tenths_db(slot, value),
            Err(Error::Invalid { line, .. }) if line == line_number
        ));
        assert_setting_state(&image, &source, &expected);
        assert_eq!(image.to_bytes(), before_error);
    }
    let reparsed = Icf::parse(&before_error).unwrap();
    assert_setting_state(&reparsed, &before_error, &expected);
    assert_eq!(reparsed.scope_reference_level_tenths_db(0), Ok(5));
    assert_eq!(reparsed.scope_reference_level_tenths_db(10), Ok(32568));
    assert_eq!(reparsed.scope_reference_level_tenths_db(13), Ok(200));
    assert_eq!(reparsed.scope_reference_level_tenths_db(16), Ok(-195));
    assert_eq!(source, original);
}
