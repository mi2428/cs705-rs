//! Cross-module rejection corpus, independent of the format-specific fixtures.
//!
//! Each accepted document must remain parseable after serialization. Rejected
//! bytes need not satisfy another parser's grammar, but must never unwind.

use cs705::{clone, gps, icf, repeater};

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
