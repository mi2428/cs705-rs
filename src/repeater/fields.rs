//! Decoded Type conversion and its ordered field-handler effects.
//! C7340's0..5 helper domain is distinct from the registered UI's0..3 choices.
//! Placeholder/marker/default type22 data are explicit source inputs, never
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
/// does not expose cases4/5 as registered GUI candidates or clear valid0.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Blocked,
    DisplayOnly,
    Skipped,
    Saved,
}

/// B0950 consumes only ordinary1..126 codes. It does not delete a selected
/// pointer, reset phase or make Mode's explicit selected event unreachable.
pub fn suppress_mode_code(code: &mut i32) {
    if (1..=126).contains(code) {
        *code = 0;
    }
}

/// 1035C/1038C list filter with9A6C4's current-value bypass. This is NOT a
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
            "repeater candidate table outside finite domain",
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
        _ => return Err(Error::Invalid("candidate filter requires Mode or Duplex")),
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
    let k = classify(row);
    let blocked = match field {
        Field::Mode => false,
        Field::Duplex => !matches!(k, 0 | 2),
        Field::Tone | Field::ToneFrequency => !matches!(k, 2 | 3),
    };
    let outcome = if blocked {
        if field == Field::Duplex && k == 4 {
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
/// runs BEFORE the numeric helper result is consumed, even when result0 skips
/// saving. This field never adds frequency's DUP/USE coupling or flag20.
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
/// modified its local candidate to7FFF, but must not modify holder/typed image.
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
                || Err(Error::SourceUnavailable("injected seed failure")),
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
                |_| Err(Error::Operation("injected typed save failure")),
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
                |_| Err(Error::Operation("injected offset save failure"))
            )
            .is_err()
        );
        assert_eq!(dword(&r, 0x48), 600100);
        assert!(!e.changed);
        assert!(e.save_requested);
        assert_eq!(e.flags, 0);
    }
}
