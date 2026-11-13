//! Decoded Type conversion and its ordered field-handler effects.
//! C7340's 0..5 helper domain is distinct from the registered UI's 0..3 choices.
//! Placeholder/marker/default type 22 data are explicit source inputs, never
//! copied factory constants or invented blank/default values.

use super::staging::{Record, classify};
use crate::csv::Error;

#[derive(Debug, Clone, Copy)]
pub struct TypeSource {
    pub placeholder: [u8; 8],
    pub gateway_marker: [u8; 8],
}
#[derive(Debug, Clone, Copy)]
pub struct Selection {
    pub accepted: bool,
    pub candidate: u32,
}
/// Observable caller context retained even when later seed/save callbacks fail.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Effects {
    pub changed: bool,
    pub flags: u32,
    pub warning_flags: u32,
    pub save_requested: bool,
    pub save_returned: bool,
}

fn dword(row: &Record, at: usize) -> u32 {
    u32::from_le_bytes([
        row.bytes[at],
        row.bytes[at + 1],
        row.bytes[at + 2],
        row.bytes[at + 3],
    ])
}
fn put(row: &mut Record, at: usize, n: u32) {
    row.bytes[at..at + 4].copy_from_slice(&n.to_le_bytes());
}

/// C7340. Unknown candidate performs no writes, as in the source helper. This
/// does not expose cases 4/5 as registered GUI candidates or clear valid 0.
pub fn transform_type(row: &mut Record, candidate: u32, source: TypeSource) -> bool {
    match candidate {
        0 => {
            if row.bytes[0x160..0x168] == [b' '; 8] {
                row.bytes[0x160..0x168].copy_from_slice(&source.placeholder);
            }
            if dword(row, 0x44) > 2 {
                put(row, 0x44, 0);
            }
            put(row, 0x15c, 2);
        }
        1 => {
            row.bytes[0x160..0x170].fill(b' ');
            put(row, 0x44, 0);
            put(row, 0x15c, 2);
        }
        2 => {
            if !matches!(dword(row, 0x44), 1 | 2) {
                put(row, 0x44, 1);
            }
            if dword(row, 0x15c) > 1 {
                put(row, 0x15c, 0);
            }
        }
        3 => {
            row.bytes[0x160..0x170].fill(b' ');
            put(row, 0x44, 0);
            if dword(row, 0x15c) > 1 {
                put(row, 0x15c, 0);
            }
        }
        4 => {
            if row.bytes[0x160..0x168] == [b' '; 8] {
                row.bytes[0x160..0x168].copy_from_slice(&source.placeholder);
            }
            row.bytes[0x16f] = b'G';
            put(row, 0x44, 3);
            put(row, 0x15c, 3);
        }
        5 => {
            row.bytes[0x160..0x170].fill(b' ');
            put(row, 0x44, 0);
            put(row, 0x15c, 3);
        }
        _ => return false,
    }
    true
}

/// 6C8E2's post-selector portion: transformation precedes changed/default load;
/// even same-K acceptance performs correction and typed save/reload. The source
/// callback is reached only on changed-K. Caller owns any pre-dialog row add,
/// which therefore cannot be rolled back merely by Selection.accepted=false.
pub fn select_type(
    row: &mut Record,
    selection: Selection,
    source: TypeSource,
    effects: &mut Effects,
    mut defaults: impl FnMut() -> Result<Record, Error>,
    mut save_reload: impl FnMut(&mut Record) -> Result<(), Error>,
) -> Result<(), Error> {
    if !selection.accepted {
        return Ok(());
    }
    let old = classify(row);
    let candidate = selection.candidate;
    transform_type(row, candidate, source);
    if candidate != old {
        effects.changed = true;
        let default = defaults()?;
        if candidate == 2 && row.bytes[0x160..0x168] == source.placeholder {
            row.bytes[0x160..0x168].fill(b' ');
        }
        if candidate == 0 && row.bytes[0x168..0x170] == source.gateway_marker {
            row.bytes[0x168..0x170].fill(b' ');
        }
        if candidate == 4 {
            row.bytes[0x170..0x174].copy_from_slice(&default.bytes[0x170..0x174]);
        }
        if matches!(candidate, 2 | 3) && !matches!(old, 2 | 3) {
            row.bytes[0x178..0x17c].copy_from_slice(&default.bytes[0x178..0x17c]);
        }
    }
    effects.save_requested = true;
    save_reload(row)?;
    effects.save_returned = true;
    effects.flags |= 0x20;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Mode,
    Duplex,
    Tone,
    ToneFrequency,
}
impl Field {
    fn key(self) -> u16 {
        match self {
            Self::Mode => 0x146,
            Self::Duplex => 0x14b,
            Self::Tone => 0x14d,
            Self::ToneFrequency => 0x14e,
        }
    }
}

/// C78C0's input gate is distinct from the caller's placeholder/helper branch.
/// Mode still consumes explicit selected events; K4 DUP discards their result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Open,
    DisplayOnly,
    Placeholder,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaintMeta {
    pub gate: Gate,
    /// C7800 style only, not the separate blocked-input color request.
    pub gray: bool,
}
impl PaintMeta {
    pub fn placeholder(self) -> Option<&'static [u8]> {
        (self.gate == Gate::Placeholder).then_some(b"---".as_slice())
    }
}

/// Read native row paint gates without an edit snapshot, B, Q or frequency class.
/// The caller retains typed values and native labels whenever no placeholder is
/// returned. Invalid-row blanking belongs to its existing row/view loader.
pub fn paint_meta(
    holder: &crate::icf::record::DecodedRecord,
    key: u16,
) -> Result<PaintMeta, Error> {
    if holder.kind() != 0x0c {
        return Err(Error::Invalid(
            "repeater paint requires a type0C holder".into(),
        ));
    }
    paint_meta_bytes(holder.as_bytes(), key)
}

pub(crate) fn paint_meta_bytes(bytes: &[u8; 616], key: u16) -> Result<PaintMeta, Error> {
    if !(0x144..=0x153).contains(&key) {
        return Err(Error::Invalid(
            "repeater paint key outside native row fields".into(),
        ));
    }
    let k = super::staging::classify_bytes(bytes);
    let value = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("holder DWORD"));
    let gate = match key {
        0x146 => Gate::DisplayOnly,
        0x14b => match k {
            0 | 2 => Gate::Open,
            4 => Gate::DisplayOnly,
            _ => Gate::Placeholder,
        },
        0x147 | 0x14c if !matches!(k, 0 | 2 | 4) => Gate::Placeholder,
        0x148 if !matches!(k, 0 | 4) => Gate::Placeholder,
        0x14d | 0x14e if !matches!(k, 2 | 3) => Gate::Placeholder,
        _ => Gate::Open,
    };
    let frequency_gray =
        matches!(k, 0 | 4) && (value(0x3c) == 0 || value(0x44) == 0 || value(0x17c) == 0);
    let gray = match key {
        0x14a => frequency_gray,
        0x14b if gate == Gate::Open => frequency_gray,
        0x14c if gate == Gate::Open => frequency_gray || value(0x44) == 0,
        0x14e if gate == Gate::Open => !matches!(value(0x174), 1 | 2),
        0x14f => matches!(k, 0 | 4) && (value(0x3c) == 0 || value(0x44) == 0),
        0x151 | 0x152 => value(0x180) == 0,
        _ => false,
    };
    Ok(PaintMeta { gate, gray })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Blocked,
    DisplayOnly,
    Skipped,
    Saved,
}

/// B0950 consumes only ordinary 1..126 codes. It does not delete a selected
/// pointer, reset phase or make Mode's explicit selected event unreachable.
pub fn suppress_mode_code(code: &mut i32) {
    if (1..=126).contains(code) {
        *code = 0;
    }
}

/// 1035C/1038C list filter with 9A6C4's current-value bypass. This is NOT a
/// selected-event storage guard. Out-of-table access is an explicit unavailable
/// native memory input rather than false/default/clamp.
pub fn candidate_allowed(
    row: &Record,
    field: Field,
    candidate: u32,
    current: u32,
) -> Result<bool, Error> {
    if candidate == current {
        return Ok(true);
    }
    if candidate > 3 {
        return Err(Error::SourceUnavailable(
            "repeater candidate table outside finite domain".into(),
        ));
    }
    let k = classify(row);
    Ok(match field {
        Field::Mode => match k {
            0 | 1 => candidate == 2,
            2 | 3 => candidate == 0,
            4 | 5 => candidate == 3,
            _ => false,
        },
        Field::Duplex => match k {
            0 => candidate <= 2,
            1 | 3 => candidate == 0,
            2 => matches!(candidate, 1 | 2),
            4 => candidate == 3,
            _ => false,
        },
        _ => {
            return Err(Error::Invalid(
                "candidate filter requires Mode or Duplex".into(),
            ));
        }
    })
}

/// Actual post-helper field handlers. Gate/list/modal producer remain explicit
/// inputs. Mode always consumes its helper result despite the keyboard gate;
/// blocked K4 DUP consumes a display helper but discards its selected result.
/// `style` is C7800 and is not added to branches that did not call it originally.
pub fn select(
    row: &mut Record,
    field: Field,
    selection: Selection,
    effects: &mut Effects,
    mut save_reload: impl FnMut(&mut Record) -> Result<(), Error>,
    mut style: impl FnMut(&Record) -> Result<(), Error>,
) -> Result<Outcome, Error> {
    let gate = paint_meta_bytes(&row.bytes, field.key())?.gate;
    // Mode ignores C78C0's return and still consumes its selected helper result.
    let blocked = field != Field::Mode && gate != Gate::Open;
    let outcome = if blocked {
        if gate == Gate::DisplayOnly {
            Outcome::DisplayOnly
        } else {
            Outcome::Blocked
        }
    } else {
        let mut outcome = Outcome::Skipped;
        if selection.accepted {
            let at = match field {
                Field::Mode => 0x15c,
                Field::Duplex => 0x44,
                Field::Tone => 0x174,
                Field::ToneFrequency => 0x178,
            };
            put(row, at, selection.candidate);
            effects.save_requested = true;
            save_reload(row)?;
            effects.save_returned = true;
            effects.changed = true;
            if matches!(field, Field::Duplex | Field::Tone) {
                effects.flags |= 8;
            }
            outcome = Outcome::Saved;
        }
        if matches!(field, Field::Duplex | Field::ToneFrequency) {
            style(row)?;
        }
        outcome
    };
    if field == Field::Duplex && classify(row) == 1 && dword(row, 0x44) != 0 {
        effects.warning_flags |= 4;
    }
    Ok(outcome)
}

/// 99F30 rounds a raw DWORD with wrapping ADD before unsigned division. It is
/// not checked addition, saturating arithmetic or floating-point rounding.
pub fn quantize_offset_hz(value: u32) -> u32 {
    value.wrapping_add(50) / 100 * 100
}

/// 6D7AF: actual C78C0 blocked result is supplied by its caller. Nonblocked style
/// runs BEFORE the numeric helper result is consumed, even when result 0 skips
/// saving. This field never adds frequency's DUP/USE coupling or flag 20.
pub fn select_offset(
    row: &mut Record,
    blocked: bool,
    native_result: u32,
    candidate_hz: u32,
    effects: &mut Effects,
    mut style: impl FnMut(&Record) -> Result<(), Error>,
    mut save_reload: impl FnMut(&mut Record) -> Result<(), Error>,
) -> Result<Outcome, Error> {
    if blocked {
        return Ok(Outcome::Blocked);
    }
    style(row)?;
    if native_result == 0 {
        return Ok(Outcome::Skipped);
    }
    put(row, 0x48, quantize_offset_hz(candidate_hz));
    effects.save_requested = true;
    save_reload(row)?;
    effects.save_returned = true;
    effects.changed = true;
    Ok(Outcome::Saved)
}

/// 6DC62 consumes the FULL native EAX, not AL. A false optional parser may have
/// modified its local candidate to 7FFF, but must not modify holder/typed image.
pub fn select_utc_offset(
    row: &mut Record,
    native_result: u32,
    candidate_minutes: i16,
    effects: &mut Effects,
    mut save_reload: impl FnMut(&mut Record) -> Result<(), Error>,
) -> Result<Outcome, Error> {
    if native_result == 0 {
        return Ok(Outcome::Skipped);
    }
    put(row, 0x184, (i32::from(candidate_minutes)) as u32);
    effects.save_requested = true;
    save_reload(row)?;
    effects.save_returned = true;
    effects.changed = true;
    Ok(Outcome::Saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readonly_meta_uses_native_field_gates_without_edit_sources() {
        use crate::icf::record::{self, ChannelField as C, RepeaterField as R, RepeaterText as T};
        use Gate::{DisplayOnly as D, Open as O, Placeholder as P};
        let mut h = record::reset_holder(0x0c, 0, 0);
        let keys = [0x147, 0x148, 0x14c, 0x14d, 0x14e, 0x14b, 0x146];
        for (k, mode, dup, blank, expected) in [
            (0, 2, 0, false, [O, O, O, P, P, O, D]),
            (1, 2, 1, true, [P, P, P, P, P, P, D]),
            (2, 0, 1, false, [O, P, O, O, O, O, D]),
            (3, 0, 0, false, [P, P, P, O, O, P, D]),
            (4, 3, 3, false, [O, O, O, P, P, D, D]),
            (5, 3, 0, false, [P, P, P, P, P, P, D]),
        ] {
            h.set_repeater_field(R::Mode, mode);
            h.set_channel_field(C::Duplex, dup);
            h.set_repeater_text(T::Call, if blank { b"        " } else { b"CALL    " });
            assert_eq!(super::super::staging::classify_bytes(h.as_bytes()), k);
            for (key, gate) in keys.into_iter().zip(expected) {
                let meta = paint_meta(&h, key).unwrap();
                assert_eq!(meta.gate, gate, "K{k}, key{key:04X}");
                assert_eq!(meta.placeholder(), (gate == P).then_some(b"---".as_slice()));
            }
        }
        for (mode, hz, duplex, used, gray) in [
            (2, 0, 1, 1, true),
            (2, 145_000_000, 0, 1, true),
            (2, 145_000_000, 1, 0, true),
            (2, 145_000_000, 1, 1, false),
            (3, 0, 3, 1, true),
            (3, 145_000_000, 3, 0, true),
            (3, 145_000_000, 3, 1, false),
            (0, 0, 1, 1, false),
            (3, 0, 0, 0, false),
        ] {
            h.set_repeater_field(R::Mode, mode);
            h.set_channel_field(C::FrequencyHz, hz);
            h.set_channel_field(C::Duplex, duplex);
            h.set_repeater_field(R::Use, used);
            let meta = paint_meta(&h, 0x14a).unwrap();
            assert_eq!(meta.gray, gray);
            assert_eq!(meta.placeholder(), None);
        }
        h.set_repeater_field(R::Mode, 2);
        h.set_channel_field(C::FrequencyHz, 145_000_000);
        h.set_channel_field(C::Duplex, 0);
        h.set_repeater_field(R::Use, 1);
        assert!(paint_meta(&h, 0x14b).unwrap().gray);
        assert!(paint_meta(&h, 0x14c).unwrap().gray);
        assert!(paint_meta(&h, 0x14f).unwrap().gray);
        h.set_channel_field(C::Duplex, 1);
        assert!(!paint_meta(&h, 0x14b).unwrap().gray);
        assert!(!paint_meta(&h, 0x14c).unwrap().gray);
        assert!(!paint_meta(&h, 0x14f).unwrap().gray);
        h.set_repeater_field(R::Mode, 0);
        for (tone, gray) in [(0, true), (1, false), (2, false), (3, true)] {
            h.set_repeater_field(R::Tone, tone);
            assert_eq!(paint_meta(&h, 0x14e).unwrap().gray, gray);
        }
        assert!(paint_meta(&h, 0x151).unwrap().gray);
        h.set_repeater_field(R::Position, 1);
        assert!(!paint_meta(&h, 0x152).unwrap().gray);
        assert!(paint_meta(&h, 0x154).is_err());
        assert!(paint_meta(&record::reset_holder(0x0a, 0, 0), 0x147).is_err());
    }

    fn source() -> TypeSource {
        TypeSource {
            placeholder: *b"PLACEHLD",
            gateway_marker: *b"MARKER G",
        }
    }
    fn row() -> Record {
        let mut r = Record {
            bytes: Box::new([0; 616]),
            frequency_class: 0,
        };
        r.bytes[12] = 1;
        r.bytes[0x160..0x168].copy_from_slice(b"CALL    ");
        r.bytes[0x168..0x170].fill(b' ');
        put(&mut r, 0x15c, 2);
        r
    }
    #[test]
    fn same_kind_accept_still_corrects_saves_and_seed_failure_keeps_prior_holder() {
        let mut r = row();
        put(&mut r, 0x44, 3);
        let mut e = Effects::default();
        select_type(
            &mut r,
            Selection {
                accepted: true,
                candidate: 0,
            },
            source(),
            &mut e,
            || panic!("same-K skips seed"),
            |r| {
                assert_eq!(dword(r, 0x44), 0);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            e,
            Effects {
                changed: false,
                flags: 0x20,
                warning_flags: 0,
                save_requested: true,
                save_returned: true
            }
        );
        let mut r = row();
        let mut e = Effects::default();
        assert!(
            select_type(
                &mut r,
                Selection {
                    accepted: true,
                    candidate: 3
                },
                source(),
                &mut e,
                || Err(Error::SourceUnavailable("injected seed failure".into())),
                |_| panic!()
            )
            .is_err()
        );
        assert!(e.changed);
        assert!(!e.save_requested);
        assert_eq!(dword(&r, 0x15c), 0);
        assert_eq!(&r.bytes[0x160..0x170], &[b' '; 16]);
    }
    #[test]
    fn helper_six_cases_and_changed_only_tone_reset_are_not_ui_six_choices() {
        for (candidate, mode, dup, k) in [
            (0, 2, 0, 0),
            (1, 2, 0, 1),
            (2, 0, 1, 2),
            (3, 0, 0, 3),
            (4, 3, 3, 4),
            (5, 3, 0, 5),
        ] {
            let mut r = row();
            assert!(transform_type(&mut r, candidate, source()));
            assert_eq!(
                (dword(&r, 0x15c), dword(&r, 0x44), classify(&r)),
                (mode, dup, k)
            );
        }
        let mut r = row();
        r.bytes[0x160..0x168].copy_from_slice(&source().placeholder);
        let mut e = Effects::default();
        select_type(
            &mut r,
            Selection {
                accepted: true,
                candidate: 2,
            },
            source(),
            &mut e,
            || {
                let mut d = row();
                put(&mut d, 0x178, 24);
                Ok(d)
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(&r.bytes[0x160..0x168], &[b' '; 8]);
        assert_eq!(dword(&r, 0x178), 24);
        let before = r.clone();
        select_type(
            &mut r,
            Selection {
                accepted: false,
                candidate: 0,
            },
            source(),
            &mut e,
            || panic!(),
            |_| panic!(),
        )
        .unwrap();
        assert_eq!(r, before);
    }

    #[test]
    fn selected_mode_is_not_a_readonly_property_and_other_gates_differ() {
        let mut r = row();
        put(&mut r, 0x15c, 0);
        put(&mut r, 0x44, 1);
        let mut e = Effects::default();
        assert!(!candidate_allowed(&r, Field::Mode, 2, 0).unwrap());
        assert!(candidate_allowed(&r, Field::Mode, 1, 1).unwrap());
        let mut code = 0x0d;
        suppress_mode_code(&mut code);
        assert_eq!(code, 0);
        let mut code = 0x7100;
        suppress_mode_code(&mut code);
        assert_eq!(code, 0x7100);
        let outcome = select(
            &mut r,
            Field::Mode,
            Selection {
                accepted: true,
                candidate: 2,
            },
            &mut e,
            |row| {
                assert_eq!(dword(row, 0x15c), 2);
                Ok(())
            },
            |_| panic!(),
        )
        .unwrap();
        assert_eq!(outcome, Outcome::Saved);
        assert_eq!(
            select(
                &mut r,
                Field::Tone,
                Selection {
                    accepted: true,
                    candidate: 1
                },
                &mut e,
                |_| panic!(),
                |_| panic!()
            )
            .unwrap(),
            Outcome::Blocked
        );
        put(&mut r, 0x15c, 0);
        let mut styles = 0;
        assert_eq!(
            select(
                &mut r,
                Field::ToneFrequency,
                Selection {
                    accepted: false,
                    candidate: 9
                },
                &mut e,
                |_| panic!(),
                |_| {
                    styles += 1;
                    Ok(())
                }
            )
            .unwrap(),
            Outcome::Skipped
        );
        assert_eq!(styles, 1);
        let mut e = Effects::default();
        assert!(
            select(
                &mut r,
                Field::Tone,
                Selection {
                    accepted: true,
                    candidate: 2
                },
                &mut e,
                |_| Err(Error::Operation("injected typed save failure".into())),
                |_| panic!()
            )
            .is_err()
        );
        assert_eq!(dword(&r, 0x174), 2);
        assert!(!e.changed);
        assert!(e.save_requested);
        assert!(!e.save_returned);
    }
    #[test]
    fn duplex_k4_discards_helper_result_and_k1_warning_is_sticky() {
        let mut r = row();
        put(&mut r, 0x15c, 3);
        put(&mut r, 0x44, 3);
        let mut e = Effects::default();
        assert_eq!(
            select(
                &mut r,
                Field::Duplex,
                Selection {
                    accepted: true,
                    candidate: 1
                },
                &mut e,
                |_| panic!(),
                |_| panic!()
            )
            .unwrap(),
            Outcome::DisplayOnly
        );
        assert_eq!(dword(&r, 0x44), 3);
        put(&mut r, 0x15c, 2);
        put(&mut r, 0x44, 1);
        r.bytes[0x160..0x168].fill(b' ');
        assert_eq!(
            select(
                &mut r,
                Field::Duplex,
                Selection {
                    accepted: false,
                    candidate: 0
                },
                &mut e,
                |_| panic!(),
                |_| panic!()
            )
            .unwrap(),
            Outcome::Blocked
        );
        assert_eq!(e.warning_flags, 4);
        put(&mut r, 0x44, 0);
        select(
            &mut r,
            Field::Duplex,
            Selection {
                accepted: false,
                candidate: 0,
            },
            &mut e,
            |_| panic!(),
            |_| panic!(),
        )
        .unwrap();
        assert_eq!(e.warning_flags, 4);
    }

    #[test]
    fn numeric_fields_keep_wrap_full_eax_false_local_and_pre_style_order() {
        assert_eq!(quantize_offset_hz(600051), 600100);
        assert_eq!(quantize_offset_hz(u32::MAX), 0);
        let mut r = row();
        put(&mut r, 0x184, 15);
        let mut e = Effects::default();
        assert_eq!(
            select_utc_offset(&mut r, 0, 0x7fff, &mut e, |_| panic!()).unwrap(),
            Outcome::Skipped
        );
        assert_eq!(dword(&r, 0x184), 15);
        select_utc_offset(&mut r, 0x100, -5, &mut e, |row| {
            assert_eq!(dword(row, 0x184), 0xffff_fffb);
            Ok(())
        })
        .unwrap();
        assert!(e.changed);
        let mut calls = Vec::new();
        let old = dword(&r, 0x48);
        assert_eq!(
            select_offset(
                &mut r,
                false,
                0,
                600051,
                &mut e,
                |row| {
                    assert_eq!(dword(row, 0x48), old);
                    calls.push("style");
                    Ok(())
                },
                |_| panic!()
            )
            .unwrap(),
            Outcome::Skipped
        );
        assert_eq!(calls, ["style"]);
        let mut e = Effects::default();
        assert!(
            select_offset(
                &mut r,
                false,
                1,
                600051,
                &mut e,
                |_| Ok(()),
                |_| Err(Error::Operation("injected offset save failure".into()))
            )
            .is_err()
        );
        assert_eq!(dword(&r, 0x48), 600100);
        assert!(!e.changed);
        assert!(e.save_requested);
        assert_eq!(e.flags, 0);
    }
}
