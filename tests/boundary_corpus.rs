//! Cross-module rejection corpus, independent of the format-specific fixtures.
//!
//! Each accepted document must remain parseable after serialization. Rejected
//! bytes need not satisfy another parser's grammar, but must never unwind.

use cs705::{clone, gps, icf, repeater};

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
