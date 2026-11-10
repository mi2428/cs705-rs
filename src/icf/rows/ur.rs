//! BA04 UR Name/Call input over the existing logical WorkingUr generation.
//! The view descriptor and typed validity are independent. A Call editor can
//! insert before cancellation or failure; neither the image nor its 40/39
//! requests are a transaction. OS delivery and document publication stay outside.

use super::{Error, ReferenceImage, WorkingUr, invalid};
use crate::{
    csv::nls,
    icf::{MemoryCallsign, record, record_image, settings::extended::FixedTextInput},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Call,
}
impl Field {
    pub fn key(self) -> u32 {
        match self {
            Self::Name => 0x13e,
            Self::Call => 0x13f,
        }
    }
}

/// Live caller/node scalars, not a prepared holder or an accepted helper result.
/// view_last/view_enabled are descriptor+8/+C consumed by 635F4 -> 614A8;
/// they are not recomputed from the current UR count or holder validity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub code: i32,
    pub command: u32,
    pub blocked: bool,
    pub view_last: i32,
    pub view_enabled: u32,
    pub node_flags: u8,
    pub warning_bits: u8,
    pub notify_flags: u32,
    pub ur_changed: bool,
}

pub struct Request<'a> {
    pub index: u32,
    pub field: Field,
    pub input: FixedTextInput<'a>,
    pub alignment: u32,
    pub generation: ReferenceImage<'a>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Effect {
    pub stored: bool,
    pub inserted: bool,
    pub full_warning: bool,
    pub blocked: bool,
    pub view_bad: bool,
    pub redirect: Option<u32>,
    pub state_out: u32,
    /// None is unreached; Some(empty) is an explicit display clear.
    pub display: Option<Vec<u8>>,
    /// Ordered native requests, not evidence of GUI delivery.
    pub notifications: Vec<u32>,
}

fn text_error(error: crate::csv::Error) -> Error {
    use crate::csv::Error::*;
    invalid(match error {
        Limit(reason)
        | Invalid(reason)
        | Conversion(reason)
        | SourceUnavailable(reason)
        | Operation(reason) => reason,
    })
}

fn prefix(bytes: &[u8]) -> Vec<u8> {
    bytes.split(|&b| b == 0).next().unwrap().to_vec()
}

impl WorkingUr {
    /// 6494C -> 6C3B7/6C464 using the shared platform NLS producer. On other
    /// platforms its explicit ASCII boundary still refuses unavailable NLS data.
    pub fn native_text(
        &mut self,
        request: Request<'_>,
        frame: &mut Frame,
        effect: &mut Effect,
    ) -> Result<(), Error> {
        self.native_text_with_nls(
            request,
            frame,
            effect,
            nls::platform_map,
            nls::platform_uppercase,
        )
    }

    /// Name uses mode 1/name_candidate; Call uses mode 0/whole_text with the
    /// actual LCMapStringA 2n and additional CharUpperBuffA n output boundaries.
    /// Errors preserve earlier insertion, source sanitation and typed writes.
    pub fn native_text_with_nls(
        &mut self,
        request: Request<'_>,
        frame: &mut Frame,
        effect: &mut Effect,
        map: impl FnMut(&[u8], &mut [u8]) -> Result<i32, crate::csv::Error>,
        upper: impl FnMut(&mut [u8]) -> Result<u32, crate::csv::Error>,
    ) -> Result<(), Error> {
        *effect = Effect::default();
        let Request {
            index,
            field,
            input,
            alignment,
            generation,
        } = request;
        if index >= 300 {
            return Err(invalid("native UR index outside0..300"));
        }
        effect.view_bad = index as i32 > frame.view_last || frame.view_enabled == 0;
        let mut holder = record::reset_holder(0x0d, index, 1);
        self.load_into(index, &mut holder)?;
        if frame.blocked || frame.command == 0xdc {
            effect.blocked = true;
            return Ok(());
        }
        if !holder.valid() && (frame.code <= 127 || frame.code == 0x7100) || effect.view_bad {
            effect.display = Some(Vec::new());
            if field != Field::Call {
                effect.redirect = Some(Field::Call.key());
                return Ok(());
            }
        }
        frame.node_flags &= !3;
        if field == Field::Call
            && effect.view_bad
            && (matches!(frame.code, 8 | 13 | 0x7100) || (0x20..=0x7e).contains(&frame.code))
        {
            if self.count() >= 300 {
                effect.full_warning = true;
                return Ok(());
            }
            self.insert_one(index, generation)?;
            effect.inserted = true;
            effect.notifications.extend([0x40, 0x39]);
            self.load_into(index, &mut holder)?;
            holder.set_valid(false);
        }
        effect.display = Some(match field {
            Field::Name => prefix(holder.name()),
            Field::Call => prefix(holder.callsign(MemoryCallsign::Your)),
        });
        effect.state_out = 2 * u32::from(matches!(input, FixedTextInput::Begin));
        if let FixedTextInput::Text { source, phase } = input {
            if !matches!(phase, 1 | 2) {
                return Err(invalid("native UR text requires phase1 or2"));
            }
            let mut candidate = match field {
                Field::Name => nls::name_candidate(source, 16).map_err(text_error)?,
                Field::Call => nls::whole_text(
                    source,
                    nls::Profile {
                        uppercase: true,
                        keep_kana: false,
                        keep_spaces: true,
                        punctuation_exceptions: b"/",
                        maximum: 8,
                    },
                    map,
                    upper,
                )
                .map_err(text_error)?,
            };
            effect.display = Some(candidate.clone());
            // Call's descriptor forbids empty input; Name explicitly allows it.
            if field == Field::Name || !candidate.is_empty() {
                match field {
                    Field::Name => {
                        candidate.resize(16, b' ');
                        holder.set_name(candidate.as_slice().try_into().unwrap());
                    }
                    Field::Call => {
                        candidate.resize(8, b' ');
                        let call: &mut [u8; 8] = candidate.as_mut_slice().try_into().unwrap();
                        if alignment != 0 && phase == 1 {
                            crate::icf::settings::extended::align_callsign(call);
                        }
                        holder.set_callsign(MemoryCallsign::Your, call);
                        effect.display = Some(candidate.clone());
                    }
                }
                // The real type 0D saver commits Name16 then Call8, even valid 0.
                self.save(index, &holder)?;
                self.load_into(index, &mut holder)?;
                frame.ur_changed = true;
                if field == Field::Call {
                    frame.notify_flags |= 4;
                }
                effect.stored = true;
            }
        }
        if field == Field::Call {
            if effect.view_bad {
                effect.display = Some(Vec::new());
            } else if record_image::callsign_bad(self.working(), &holder)? {
                frame.warning_bits |= 4;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(count: u16) -> WorkingUr {
        let mut model = WorkingUr {
            memory: vec![0xa6; 0x4997a],
            original: Vec::new(),
            comment: Vec::new(),
            extra_data: 0x12345639,
        };
        model.memory[0x49978..0x4997a].copy_from_slice(&count.to_le_bytes());
        for index in 0..300 {
            model
                .save(index, &super::super::ur_default(index, 2))
                .unwrap();
        }
        for (index, call) in [(0, b"FIRST  A"), (1, b"SECOND B")] {
            let mut h = record::reset_holder(0x0d, index, 1);
            model.load_into(index, &mut h).unwrap();
            h.set_name(b"ORIGINAL NAME   ");
            h.set_callsign(MemoryCallsign::Your, call);
            model.save(index, &h).unwrap();
        }
        model
    }

    fn frame(last: i32) -> Frame {
        Frame {
            code: 13,
            command: 0,
            blocked: false,
            view_last: last,
            view_enabled: 1,
            node_flags: 0xa7,
            warning_bits: 0x80,
            notify_flags: 0x20,
            ur_changed: false,
        }
    }

    fn request(index: u32, field: Field, input: FixedTextInput<'_>) -> Request<'_> {
        Request {
            index,
            field,
            input,
            alignment: 0,
            generation: ReferenceImage::Working,
        }
    }

    fn call(model: &WorkingUr, index: u32) -> [u8; 8] {
        let mut holder = record::reset_holder(0x0d, index, 1);
        model.load_into(index, &mut holder).unwrap();
        *holder.callsign(MemoryCallsign::Your)
    }

    #[test]
    fn native_ur_name_shares_byte_grammar_and_keeps_parent_redirect_and_blocked_distinct() {
        let mut model = model(2);
        let before = model.working().to_vec();
        let mut source = b" a\0b  ".to_vec();
        let mut frame = frame(1);
        let mut effect = Effect::default();
        model
            .native_text_with_nls(
                request(
                    0,
                    Field::Name,
                    FixedTextInput::Text {
                        source: &mut source,
                        phase: 2,
                    },
                ),
                &mut frame,
                &mut effect,
                |_, _| panic!("Name never maps"),
                |_| panic!("Name never uppercases"),
            )
            .unwrap();
        let mut expected = before;
        expected[0x2f703..0x2f713].copy_from_slice(b" a b            ");
        assert_eq!(model.working(), expected);
        assert_eq!(source, b" a b  ");
        assert!(effect.stored && frame.ur_changed);
        assert_eq!(effect.display, Some(b" a b".to_vec()));
        assert_eq!(
            (frame.node_flags, frame.warning_bits, frame.notify_flags),
            (0xa4, 0x80, 0x20)
        );
        let mut source = [vec![b'A'; 15], vec![0x82, 0xa0, b'X']].concat();
        model
            .native_text(
                request(
                    0,
                    Field::Name,
                    FixedTextInput::Text {
                        source: &mut source,
                        phase: 1,
                    },
                ),
                &mut frame,
                &mut effect,
            )
            .unwrap();
        assert_eq!(&model.working()[0x2f703..0x2f713], b"AAAAAAAAAAAAAAA ");
        let mut empty = [];
        model
            .native_text(
                request(
                    0,
                    Field::Name,
                    FixedTextInput::Text {
                        source: &mut empty,
                        phase: 2,
                    },
                ),
                &mut frame,
                &mut effect,
            )
            .unwrap();
        assert!(effect.stored);
        assert_eq!(&model.working()[0x2f703..0x2f713], &[b' '; 16]);
        assert_eq!(call(&model, 0), *b"FIRST  A");
        model
            .native_text(
                request(0, Field::Name, FixedTextInput::Begin),
                &mut frame,
                &mut effect,
            )
            .unwrap();
        assert_eq!(effect.state_out, 2);
        assert!(!effect.stored);

        let before = model.working().to_vec();
        let mut frame = super::tests::frame(299); // Valid view, but typed holder is invalid.
        model
            .native_text(
                request(2, Field::Name, FixedTextInput::Begin),
                &mut frame,
                &mut effect,
            )
            .unwrap();
        assert_eq!(effect.redirect, Some(0x13f));
        assert_eq!(effect.display, Some(Vec::new()));
        assert_eq!(frame.node_flags, 0xa7); // The Name handler was never reached.
        assert_eq!(model.working(), before);
        for blocked in [false, true] {
            frame.blocked = blocked;
            frame.command = if blocked { 0 } else { 0xdc };
            model
                .native_text(
                    request(0, Field::Call, FixedTextInput::Begin),
                    &mut frame,
                    &mut effect,
                )
                .unwrap();
            assert!(effect.blocked);
            assert_eq!(effect.display, None);
            assert_eq!(model.working(), before);
        }
    }

    #[test]
    fn native_ur_insert_before_cancel_keeps_full_tail_seed_count_g_word_and_notices() {
        for accept in [false, true] {
            let mut model = model(1);
            let mut tail = record::reset_holder(0x0d, 298, 1);
            model.load_into(298, &mut tail).unwrap();
            tail.set_callsign(MemoryCallsign::Your, b"TAIL  ZZ");
            model.save(298, &tail).unwrap();
            let mut generation = vec![0; 0x3c903];
            generation[0x3c901..0x3c903].copy_from_slice(&0x1234u16.to_le_bytes());
            let mut frame = frame(0);
            let mut effect = Effect::default();
            let mut same = *b"CALLSIGN";
            let input = if accept {
                FixedTextInput::Text {
                    source: &mut same,
                    phase: 2,
                }
            } else {
                FixedTextInput::Cancel
            };
            model
                .native_text(
                    Request {
                        generation: ReferenceImage::Separate(&generation),
                        ..request(1, Field::Call, input)
                    },
                    &mut frame,
                    &mut effect,
                )
                .unwrap();
            assert_eq!(model.count(), 2);
            assert_eq!(call(&model, 0), *b"FIRST  A");
            assert_eq!(call(&model, 1), *b"CALLSIGN");
            assert_eq!(call(&model, 299), *b"TAIL  ZZ");
            assert_eq!(&model.working()[0x3c901..0x3c903], &0x1234u16.to_le_bytes());
            assert_eq!(effect.notifications, [0x40, 0x39]);
            assert!(effect.inserted && effect.view_bad);
            assert_eq!(effect.stored, accept); // The reloaded holder was cleared valid 0.
            assert_eq!(effect.display, Some(Vec::new()));
            assert_eq!(frame.node_flags, 0xa4);
            assert_eq!(frame.warning_bits, 0x80); // Old view-bad skips C9678.
            assert_eq!(frame.notify_flags, if accept { 0x24 } else { 0x20 });
            assert_eq!(frame.ur_changed, accept);
        }
    }

    #[test]
    fn native_ur_call_phase_alignment_empty_rejection_and_false_warning_are_separate() {
        for (phase, expected, warning) in [(1, *b"FIRST  A", 0x84), (2, *b"FIRST A ", 0x80)] {
            let mut model = model(2);
            let mut source = *b"first a";
            let mut frame = frame(1);
            let mut effect = Effect::default();
            model
                .native_text(
                    Request {
                        alignment: 1,
                        ..request(
                            1,
                            Field::Call,
                            FixedTextInput::Text {
                                source: &mut source,
                                phase,
                            },
                        )
                    },
                    &mut frame,
                    &mut effect,
                )
                .unwrap();
            assert!(effect.stored && !effect.inserted && frame.ur_changed);
            assert_eq!(call(&model, 1), expected);
            assert_eq!(&model.working()[0x2f713..0x2f723], b"ORIGINAL NAME   ");
            assert_eq!(frame.warning_bits, warning); // Duplicates are stored before warning.
            assert_eq!(frame.notify_flags, 0x24);
            assert!(effect.notifications.is_empty());
        }
        let mut model = model(2);
        let before = model.working().to_vec();
        let mut empty = *b" !!! ";
        let mut frame = frame(1);
        let mut effect = Effect::default();
        model
            .native_text(
                request(
                    1,
                    Field::Call,
                    FixedTextInput::Text {
                        source: &mut empty,
                        phase: 2,
                    },
                ),
                &mut frame,
                &mut effect,
            )
            .unwrap();
        assert!(!effect.stored && !frame.ur_changed);
        assert_eq!(model.working(), before);
        assert_eq!(frame.notify_flags, 0x20);
        let mut duplicate = record::reset_holder(0x0d, 1, 1);
        model.load_into(1, &mut duplicate).unwrap();
        duplicate.set_callsign(MemoryCallsign::Your, b"FIRST  A");
        model.save(1, &duplicate).unwrap();
        model
            .native_text(
                request(1, Field::Call, FixedTextInput::Cancel),
                &mut frame,
                &mut effect,
            )
            .unwrap();
        assert!(!effect.stored);
        assert_eq!(frame.warning_bits, 0x84); // A false editor still runs C9678.
    }

    #[test]
    fn native_ur_failure_prefixes_keep_insertion_and_shared_nls_mutations() {
        let mut model = model(1);
        let mut frame = frame(0);
        let mut effect = Effect::default();
        let mut source = *b"x\0y";
        assert!(
            model
                .native_text_with_nls(
                    Request {
                        generation: ReferenceImage::Separate(&[]),
                        ..request(
                            1,
                            Field::Call,
                            FixedTextInput::Text {
                                source: &mut source,
                                phase: 2
                            }
                        )
                    },
                    &mut frame,
                    &mut effect,
                    |_, _| panic!("G read must fail first"),
                    |_| panic!("unreached upper"),
                )
                .is_err()
        );
        assert_eq!(model.count(), 2); // Count precedes the missing G word.
        assert_eq!(call(&model, 1), *b"CALLSIGN");
        assert_eq!(source, *b"x\0y");
        assert!(effect.notifications.is_empty());
        assert!(!effect.inserted && !frame.ur_changed);
        assert_eq!(frame.node_flags, 0xa4);

        for fail_upper in [false, true] {
            let mut model = super::tests::model(1);
            let mut frame = super::tests::frame(0);
            let mut source = *b"x\0y";
            assert!(
                model
                    .native_text_with_nls(
                        request(
                            1,
                            Field::Call,
                            FixedTextInput::Text {
                                source: &mut source,
                                phase: 1
                            }
                        ),
                        &mut frame,
                        &mut effect,
                        |src, dst| {
                            assert_eq!(src, b"x y   ");
                            assert_eq!(src.len(), dst.len());
                            if !fail_upper {
                                return Err(crate::csv::Error::Operation("injected map failure"));
                            }
                            dst.copy_from_slice(src);
                            Ok(0)
                        },
                        |bytes| {
                            bytes[0] = b'Z';
                            Err(crate::csv::Error::Operation("injected upper failure"))
                        },
                    )
                    .is_err()
            );
            assert_eq!(model.count(), 2);
            assert_eq!(call(&model, 1), *b"CALLSIGN");
            assert_eq!(source, *b"x y"); // Upper's private copy never aliases this source.
            assert!(effect.inserted && !effect.stored && !frame.ur_changed);
            assert_eq!(effect.notifications, [0x40, 0x39]);
            assert_eq!(frame.notify_flags, 0x20);
        }
    }

    #[test]
    fn native_ur_view_capacity_boundary_and_native_zero_are_not_acceptance_bools() {
        let mut model = model(300);
        let before = model.working().to_vec();
        let mut frame = frame(299);
        frame.view_enabled = 0; // View-bad is independent of the valid typed row.
        let mut effect = Effect::default();
        let mut source = *b"NEVER";
        model
            .native_text_with_nls(
                request(
                    299,
                    Field::Call,
                    FixedTextInput::Text {
                        source: &mut source,
                        phase: 2,
                    },
                ),
                &mut frame,
                &mut effect,
                |_, _| panic!("full skips map"),
                |_| panic!("full skips upper"),
            )
            .unwrap();
        assert!(effect.full_warning && effect.view_bad);
        assert_eq!(effect.display, Some(Vec::new()));
        assert_eq!(model.working(), before);
        assert_eq!(frame.node_flags, 0xa4);
        assert!(
            model
                .native_text(
                    request(300, Field::Call, FixedTextInput::Display),
                    &mut frame,
                    &mut effect
                )
                .is_err()
        );

        let mut model = super::tests::model(1);
        let mut frame = super::tests::frame(299);
        frame.view_enabled = 0;
        model
            .native_text(
                request(0, Field::Call, FixedTextInput::Cancel),
                &mut frame,
                &mut effect,
            )
            .unwrap();
        assert!(effect.inserted && effect.view_bad); // Valid row still inserts for this view.
        assert_eq!(model.count(), 2);
        assert_eq!(call(&model, 1), *b"FIRST  A");

        let mut model = super::tests::model(299);
        let mut frame = super::tests::frame(298);
        let mut source = *b"last z";
        model
            .native_text(
                Request {
                    alignment: 1,
                    ..request(
                        299,
                        Field::Call,
                        FixedTextInput::Text {
                            source: &mut source,
                            phase: 1,
                        },
                    )
                },
                &mut frame,
                &mut effect,
            )
            .unwrap();
        assert_eq!(model.count(), 300);
        assert_eq!(call(&model, 299), *b"LAST   Z");
        assert!(effect.inserted && effect.stored && effect.view_bad);
        assert_eq!(effect.notifications, [0x40, 0x39]);
        assert_eq!(effect.display, Some(Vec::new()));

        let mut model = super::tests::model(2);
        let mut frame = super::tests::frame(1);
        let mut source = *b"abc";
        model
            .native_text_with_nls(
                request(
                    1,
                    Field::Call,
                    FixedTextInput::Text {
                        source: &mut source,
                        phase: 2,
                    },
                ),
                &mut frame,
                &mut effect,
                |src, dst| {
                    assert_eq!(src.len(), 6);
                    dst[..2].copy_from_slice(b"xy");
                    Ok(0)
                },
                |bytes| {
                    bytes.make_ascii_uppercase();
                    Ok(0)
                },
            )
            .unwrap();
        assert!(effect.stored);
        assert_eq!(call(&model, 1), *b"XY      ");
        assert_eq!(source, *b"xy ");
    }
}
