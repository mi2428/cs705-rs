//! Cross-module rejection corpus, independent of the format-specific fixtures.
//!
//! Each accepted document must remain parseable after serialization. Rejected
//! bytes need not satisfy another parser's grammar, but must never unwind.

use cs705::{clone, gps, icf, repeater};

#[test]
fn home_dr_l30_membership_does_not_inherit_the_common_context_bypass() {
    let mut raw = [0; 24];
    raw[..4].copy_from_slice(&16u32.to_le_bytes());
    let q = icf::context::Context::from_bytes(raw);
    let ranges = icf::frequency::Ranges::generate(&q, Some(0x411)).unwrap();
    for (frequency, expected) in [
        (29_999, false),
        (30_000, true),
        (199_999_999, true),
        (200_000_000, false),
        (399_999_999, false),
        (400_000_000, true),
        (470_000_000, true),
        (470_000_001, false),
    ] {
        assert_eq!(ranges.eb98(frequency), expected);
        assert!(!ranges.eb64(&q, frequency));
    }
}

#[test]
fn scan_selected_raw_projection_and_late_source_failure_keep_distinct_boundaries() {
    let mut raw = vec![0xa5; 0x3c9c0];
    raw[0x355a1] = 6;
    raw[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    let base = icf::Icf::from_memory(raw.clone()).unwrap();
    for candidate in [-1, 0, 24, 25, 31, 32, 255, 256, i32::MIN, i32::MAX] {
        let mut image = base.clone();
        let mut expected = raw.clone();
        expected[0x37802] = (0xa5 & !0x7c) | ((candidate as u8 & 31) << 2);
        icf::special::select_scan_raw(&mut image, &[], candidate, false).unwrap();
        assert_eq!(image.memory(), expected);
        assert_eq!(
            icf::special::selected_scan_raw(image.memory()).unwrap(),
            candidate as u8 & 31
        );
    }
    assert!(icf::special::selected_scan_raw(&[]).is_err());
    let mut image = base.clone();
    assert!(icf::special::select_scan(&mut image, &[], 25, false).is_err());
    assert_eq!(image.memory(), raw);
    let mut image = base.clone();
    let mut expected = raw.clone();
    expected[0x37802] = (0xa5 & !0x7c) | (31 << 2);
    expected[0x37807] = 0xa4;
    assert!(icf::special::select_scan_raw(&mut image, &[], -1, true).is_err());
    assert_eq!(image.memory(), expected);
    let mut generation = vec![0; 0x37814];
    generation[0x37810..].copy_from_slice(&[1, 2, 3, 4]);
    icf::special::select_scan_raw(&mut image, &generation, -1, true).unwrap();
    expected[0x37810..0x37814].copy_from_slice(&[1, 2, 3, 4]);
    assert_eq!(image.memory(), expected);
}

#[test]
fn auto_repeater_predicates_keep_specific_numeric_and_source_gates_distinct() {
    use icf::{context::Context, frequency, record};
    let context = |kind: u32, flag: u8| {
        let mut raw = [0; 24];
        raw[..4].copy_from_slice(&kind.to_le_bytes());
        raw[0x13] = flag;
        Context::from_bytes(raw)
    };
    let q = context(1, 1);
    let ranges = frequency::Ranges::generate(&q, None).unwrap();
    let holder = |low: u32, high: u32| {
        let mut raw = [0; 10];
        raw[0] = 1;
        raw[1] = 0xa7;
        raw[2..6].copy_from_slice(&low.to_le_bytes());
        raw[6..].copy_from_slice(&high.to_le_bytes());
        let mut h = record::reset_holder(0x18, 0, 0);
        record::decode_mode_pair_into(&mut h, &raw);
        h
    };
    for (low, high, specific, numeric) in [
        (1_000_000, 2_000_000, false, false),
        (2_000_000, 1_000_000, true, false),
        (200_000_000, 200_000_001, true, false),
        (29_999, 30_000, true, true),
        (470_000_000, 470_000_001, true, true),
    ] {
        let h = holder(low, high);
        let saved = h.clone();
        assert_eq!(
            frequency::auto_repeater_bad(&h, &ranges, &q, false, None).unwrap(),
            specific
        );
        assert_eq!(
            frequency::auto_repeater_bad(&h, &ranges, &q, true, None).unwrap(),
            numeric
        );
        assert_eq!(h.as_bytes(), saved.as_bytes());
    }
    let h = holder(200_000_000, 1_000_000);
    assert!(frequency::auto_repeater_edge_bad(&h, &ranges, &q, true, false, None).unwrap());
    // A bad low edge suppresses only the ordering warning on the valid high.
    assert!(!frequency::auto_repeater_edge_bad(&h, &ranges, &q, false, false, None).unwrap());
    for q in [context(0, 1), context(1, 0), context(1, 2), context(16, 0)] {
        let h = holder(2_000_000, 1_000_000);
        assert!(!frequency::auto_repeater_bad(&h, &ranges, &q, false, Some(0x411)).unwrap());
    }
    let mut h = holder(0, 0);
    h.set_valid(false);
    let q = context(16, 0);
    assert!(frequency::auto_repeater_bad(&h, &ranges, &q, false, None).is_err());
    assert!(!frequency::auto_repeater_bad(&h, &ranges, &q, true, None).unwrap());
    h.set_valid(true);
    assert!(frequency::auto_repeater_bad(&h, &ranges, &q, true, None).unwrap());
    let q = context(16, 1);
    let h = holder(2_000_000, 1_000_000);
    assert!(frequency::auto_repeater_bad(&h, &ranges, &q, false, Some(0x409)).unwrap());
    assert!(!frequency::auto_repeater_bad(&h, &ranges, &q, false, Some(0x411)).unwrap());
}

#[test]
fn repeater_offset_schema_selects_live_q_and_never_guesses_missing_host_or_class() {
    let context = |kind: u32| {
        let mut raw = [0; 24];
        raw[..4].copy_from_slice(&kind.to_le_bytes());
        icf::context::Context::from_bytes(raw)
    };
    for (kind, expected) in [
        (0, [100_000, 0, 5_000_000, 20_000_000]),
        (1, [500_000, 600_000, 5_000_000, 12_000_000]),
        (2, [500_000, 600_000, 7_600_000, 28_000_000]),
        (4, [500_000, 600_000, 7_600_000, 28_000_000]),
        (5, [500_000, 600_000, 5_000_000, 0]),
    ] {
        let q = context(kind);
        for (class, expected) in expected.into_iter().enumerate() {
            assert_eq!(
                icf::frequency::f20c(&q, None, class as i32).unwrap(),
                expected
            );
        }
    }
    let q = context(16);
    assert!(icf::frequency::f20c(&q, None, 0).is_err());
    assert_eq!(
        icf::frequency::f20c(&q, Some(0x411), 3).unwrap(),
        20_000_000
    );
    assert_eq!(
        icf::frequency::f20c(&q, Some(0x409), 3).unwrap(),
        12_000_000
    );
    for class in [-1, 4, i32::MIN, i32::MAX] {
        assert!(icf::frequency::f20c(&q, Some(0x411), class).is_err());
    }
    assert!(icf::frequency::offset_for_source(5, 0).is_err());
    assert!(icf::frequency::offset_for_source(255, 0).is_err());
}

#[test]
fn raw_record_boundaries_preserve_headers_and_reject_invalid_sides_before_stores() {
    use icf::record::{self, ChannelWrite, DecodedRecord, RepeaterWrite};

    // Arbitrary holder DWORDs are intentional. Lookup and raw storage are not
    // canonical typed-enum validation, and must not panic on unknown values.
    let mut state = 0x0705_0616_u32;
    for case in 0..256 {
        let mut bytes = [0; 616];
        for byte in &mut bytes {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *byte = (state >> 24) as u8;
        }
        let mut holder = DecodedRecord::from_bytes(bytes);
        holder.set_valid(case % 2 == 0);
        let header: [u8; 12] = holder.as_bytes()[..12].try_into().unwrap();
        let mut raw_channel = [case as u8; 83];
        let mut flag = 0xa5;
        let before = holder.clone();
        assert!(
            record::encode_channel_into(
                &mut holder,
                &mut raw_channel,
                usize::MAX,
                Some(&mut flag),
                |_| panic!("invalid destination side must not call a writer"),
            )
            .is_err()
        );
        assert!(holder == before);
        assert_eq!(raw_channel, [case as u8; 83]);
        assert_eq!(flag, 0xa5);
        assert!(record::decode_channel_into(&mut holder, &raw_channel, 2, None).is_err());
        assert!(holder == before);

        for side in 0..2 {
            let mut stores = 0;
            record::encode_channel_into(
                &mut holder,
                &mut raw_channel,
                side,
                Some(&mut flag),
                |event| {
                    if let ChannelWrite::Body { offset, bytes, .. } = event {
                        assert!(offset.checked_add(bytes.len()).is_some_and(|end| end <= 83));
                    }
                    stores += 1;
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(stores == 1, !holder.valid());
            record::decode_channel_into(&mut holder, &raw_channel, side, Some(flag)).unwrap();
            assert_eq!(&holder.as_bytes()[..12], &header);
        }

        let mut raw_repeater = [case as u8; 52];
        let mut stores = 0;
        record::encode_repeater_into(&holder, &mut raw_repeater, |event| {
            if let RepeaterWrite::Body { offset, bytes, .. } = event {
                assert!(offset.checked_add(bytes.len()).is_some_and(|end| end <= 52));
            }
            stores += 1;
            Ok(())
        })
        .unwrap();
        assert!(
            stores > 0,
            "repeater saver does not gate on holder validity"
        );
        record::decode_repeater_into(&mut holder, &raw_repeater, &[0xff; 7], u32::MAX, u32::MAX);
        assert_eq!(&holder.as_bytes()[..12], &header);

        let mut raw_gps = [case as u8; 34];
        let mut stores = 0;
        record::encode_gps_into(&holder, &mut raw_gps, |offset, bytes| {
            assert!(offset.checked_add(bytes.len()).is_some_and(|end| end <= 34));
            stores += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(stores == 0, !holder.valid());
        record::decode_gps_into(&mut holder, &raw_gps, u32::MAX);
        assert_eq!(&holder.as_bytes()[..12], &header);
    }
}

#[test]
fn working_group_length_is_the_last_nonempty_ordinal_not_descriptor_count() {
    // Synthetic blank topology, independent of factory images and parser fixtures.
    let mut memory = vec![0; 0x3c9c0];
    memory[0x355a1] = 6;
    memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
    for group in 0..100 {
        let at = 0x355c0 + 19 * group;
        memory[at] = if group == 99 { 0xff } else { group as u8 + 1 };
        memory[at + 1..at + 3].copy_from_slice(&u16::MAX.to_le_bytes());
        memory[at + 3..at + 19].fill(b' ');
    }
    memory[0x36114..0x36114 + 1300].fill(0xff);
    memory[0x36629..0x3662b].copy_from_slice(&0_u16.to_le_bytes());
    memory[0x3662b..0x3662d].copy_from_slice(&499_u16.to_le_bytes());
    memory[0x3662d..0x3662d + 500].fill(1);
    for slot in 0..500 {
        let next = if slot == 499 {
            u16::MAX
        } else {
            slot as u16 + 1
        };
        let at = 0x35d2c + 2 * slot;
        memory[at..at + 2].copy_from_slice(&next.to_le_bytes());
    }
    let mut image = icf::Icf::from_memory(memory).unwrap();
    assert_eq!(
        icf::rows::NormalMemory::from_icf(&image)
            .unwrap()
            .group_count(),
        0
    );

    // Descriptor order 2 -> 0 -> 1 -> 3...; only ordinal 2 has a nonblank name.
    image
        .patch_memory(&[
            (0x36628, &[2]),
            (0x355c0 + 19 * 2, &[0]),
            (0x355c0 + 19, &[3]),
            (0x355c0 + 19 + 3, b"X"),
        ])
        .unwrap();
    let model = icf::rows::NormalMemory::from_icf(&image).unwrap();
    assert_eq!(model.group_count(), 3);
    assert_eq!(model.used_slots(), 0);
    assert_eq!(
        &model.working()[0x355c3 + 19 * 2..0x355c3 + 19 * 2 + 1],
        b"X"
    );
}

#[test]
fn arbitrary_input_and_valid_prefix_mutations_do_not_panic() {
    let seeds: &[&[u8]] = &[
        b"40750001\r\n#Comment=\r\n#MapRev=6\r\n#EtcData=000000\r\n",
        b"Group,Group Name,Name,Date,Time,Latitude,Longitude,Altitude,Alarm\r\n",
        b"Group No,Group Name,Name,Sub Name,Repeater Call Sign,Gateway Call Sign,Frequency,Dup,Offset,Mode,TONE,Repeater Tone,RPT1USE,Position,Latitude,Longitude,UTC Offset\r\n",
        &[0xfe, 0xfe, 0xee, 0xef, 0xe0, 0x40, 0x75, 0, 0, 0xfd],
        &[0xfe, 0xfe, 0xef, 0xee, 0xe6, 0, 0xfd],
    ];
    let mut state = 0x7050_cafe_u32;
    for case in 0..4096 {
        let mut input = if case < 2048 {
            seeds[case % seeds.len()].to_vec()
        } else {
            vec![0; case % 257]
        };
        if case % 3 == 0 {
            input.truncate(case % (input.len() + 1));
        }
        for _ in 0..case % 7 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            if !input.is_empty() {
                let index = state as usize % input.len();
                input[index] ^= (state >> 24) as u8;
            }
        }
        if let Ok(document) = icf::Icf::parse(&input) {
            let serialized = document.to_bytes();
            assert_eq!(
                icf::Icf::parse(&serialized).unwrap().memory(),
                document.memory(),
                "ICF case {case}"
            );
        }
        if let Ok(document) = gps::Csv::parse(&input) {
            let serialized = document.to_bytes();
            assert!(gps::Csv::parse(&serialized).is_ok(), "GPS case {case}");
        }
        if let Ok(document) = repeater::Csv::parse(&input) {
            let serialized = document.to_bytes();
            assert!(
                repeater::Csv::parse(&serialized).is_ok(),
                "repeater case {case}"
            );
        }
        for direction in [clone::Direction::ToRadio, clone::Direction::FromRadio] {
            if let Ok(frame) = clone::Frame::decode(&input, direction) {
                let encoded = frame.encode(direction).unwrap();
                assert_eq!(
                    clone::Frame::decode(&encoded, direction).unwrap(),
                    frame,
                    "clone case {case}"
                );
            }
        }
        let mut framer = clone::Framer::default();
        for byte in input {
            let _ = framer.push(byte);
        }
        let _ = framer.finish();
    }
}
