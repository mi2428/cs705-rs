//! Native repeater field invocations over the one decoded/working codec.
//! Effects are GUI requests, never a claim of native UI or device delivery.

use crate::{
    csv::{self, Error},
    icf::{
        self,
        context::Context,
        frequency::Ranges,
        record::{self, ChannelField as C, DecodedRecord},
        settings::extended::{ChoiceInput, NumberInput, TextInput},
    },
    repeater::{
        fields::{self, Effects, Selection, TypeSource},
        predicate,
        staging::{self, Record},
        working::{DefaultContext, Working},
    },
};

/// C7340 and 6CB13/6CB57 short literals, independently matched to their hashes.
pub const TYPE_SOURCE: TypeSource = TypeSource {
    placeholder: *b"CALLSIGN",
    gateway_marker: *b"       G",
};

#[derive(Debug, Clone, Copy)]
pub struct Runtime<'a> {
    /// Captured B owns F20C's source selector, independent of later live Q.
    pub ranges: &'a Ranges,
    /// Type 22 defaults and live-Q warnings still consume this actual Q.
    pub live_q: &'a Context,
    pub system_langid: Option<u16>,
    /// Only reached row insertion/reference callbacks read this actual G.
    pub generation: Option<&'a [u8]>,
    /// Actual 864634 option, not a blanket right-justify policy.
    pub align_callsign: bool,
    pub crt_decimal: u8,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Effect {
    pub native: Effects,
    pub state_out: u32,
    pub popup_requested: bool,
    pub blocked: bool,
    pub gray: bool,
    pub display: Vec<u8>,
    pub source_error: Option<String>,
    pub notifications: Vec<(u32, bool)>,
    pub inserted: bool,
    pub helper_accepted: bool,
    pub candidate: Option<i64>,
    pub shared_input: Vec<u8>,
    pub group_label: Option<(u32, Vec<u8>)>,
    pub refresh_if_binding_matches: bool,
    pub coordinate_dialog: Option<csv::number::CoordinateDialog>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChoiceField {
    Type,
    Mode,
    Duplex,
    Tone,
    Use,
    Position,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextField {
    Name,
    SubName,
    Call,
    Gateway,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberField {
    Frequency,
    Offset,
    ToneFrequency,
    UtcOffset,
}

/// Real LCMap 2n and CharUpper n inputs/outputs, not completed record packets.
pub type Map<'a> = dyn FnMut(&[u8], &mut [u8]) -> Result<i32, Error> + 'a;
pub struct Nls<'a> {
    pub map: &'a mut Map<'a>,
    pub upper: &'a mut dyn FnMut(&mut [u8]) -> Result<u32, Error>,
}

/// H55727-01/6C700 is a group-name operation, not a repeater row cell.
/// No row load/save, count/order/reference edit or implicit group insertion runs.
pub fn group_name(
    image: &mut icf::Icf<'_>,
    group: usize,
    input: TextInput<'_>,
    effect: &mut Effect,
) -> Result<(), Error> {
    if group >= 50 {
        return Err(Error::Invalid("repeater named group outside 0..50".into()));
    }
    let names = crate::repeater::groups::NAMES;
    let old = &image.memory()[names + 16 * group..names + 16 * (group + 1)];
    effect.display = old.split(|&b| b == 0).next().unwrap().to_vec();
    match input {
        TextInput::Begin(first) => {
            effect.state_out = 2;
            if let Some(first) = first {
                effect.display = vec![first];
            }
        }
        TextInput::Text(source) => {
            effect.shared_input = source.to_vec();
            let mut candidate = csv::nls::name_candidate(&mut effect.shared_input, 16)?;
            effect.helper_accepted = true;
            candidate.resize(16, b' ');
            effect.native.save_requested = true;
            crate::repeater::groups::rename_group_raw(
                image,
                group,
                candidate.as_slice().try_into().expect("group name width"),
            )
            .map_err(|_| Error::Operation("repeater group name store failed".into()))?;
            effect.native.save_returned = true;
            effect.native.changed = true;
            let mut label = format!("{:02}: ", group + 1).into_bytes();
            label.extend_from_slice(&candidate);
            effect.group_label = Some((group as u32 + 0x7b, label));
            effect.notifications.push((0x3e, false));
            effect.refresh_if_binding_matches = true;
            effect.display = candidate;
        }
        TextInput::Display | TextInput::Cancel => {}
    }
    Ok(())
}

/// One normal-CW A522C calculation for frequency and offset callers. This is not
/// CSV's strict decimal parser. Range clamp follows binary64 signed rounding.
fn scaled_input(
    input: NumberInput<'_>,
    old: u32,
    min: u32,
    max: u32,
    point: u8,
    empty_frequency: bool,
    effect: &mut Effect,
) -> Result<(u32, bool), Error> {
    match input {
        NumberInput::Begin => {
            effect.state_out = 2;
            Ok((old, false))
        }
        NumberInput::Text(text) => {
            if empty_frequency && text.is_empty() {
                return Ok((0, true));
            }
            let (value, accepted) = icf::settings::extended::numeric_text(
                text,
                point,
                6,
                i64::from(min),
                i64::from(max),
                1,
            )
            .map_err(|_| Error::Conversion("native scaled numeric text".into()))?;
            Ok((value as u32, accepted))
        }
        _ => Ok((old, false)),
    }
}

fn utc_input(input: NumberInput<'_>, old: i16, effect: &mut Effect) -> Result<(i16, bool), Error> {
    Ok(match input {
        NumberInput::Begin => {
            effect.state_out = 2;
            (old, false)
        }
        NumberInput::Next | NumberInput::Previous => {
            if old == 0x7fff {
                return Ok((-840, true));
            }
            let n = i32::from(old) + if input == NumberInput::Next { 5 } else { -5 };
            (
                if n < -840 {
                    840
                } else if n > 840 {
                    -840
                } else {
                    n as i16
                },
                true,
            )
        }
        NumberInput::Text(text) => {
            let prefix = text.split(|&b| b == 0).next().unwrap_or_default();
            let optional = prefix.is_empty() || prefix.starts_with(b"--:--");
            let first: Vec<_> = prefix
                .iter()
                .copied()
                .filter(|b| b.is_ascii_digit() || b"+-:".contains(b))
                .collect();
            let minus = first.first() == Some(&b'-');
            let digits: Vec<_> = first
                .into_iter()
                .filter(|b| b.is_ascii_digit() || *b == b':')
                .collect();
            let mut parts = digits.split(|&b| b == b':');
            // Decimal-prefix byte projection belongs to 9D968, not strict HH:MM.
            let decimal = |s: &[u8]| {
                s.iter()
                    .take_while(|b| b.is_ascii_digit())
                    .fold(0_u32, |n, b| {
                        n.wrapping_mul(10).wrapping_add(u32::from(*b - b'0'))
                    }) as u8
            };
            let hour = decimal(parts.next().unwrap_or_default());
            let minute = decimal(parts.next().unwrap_or_default());
            let minute = (5 * ((u16::from(minute) + 2) / 5)) as u8;
            let total = i32::from(hour) * 60 + i32::from(minute);
            let accepted = digits.len() <= 5 && minute <= 59 && total <= 840;
            (
                if optional {
                    0x7fff
                } else if accepted {
                    if minus { -(total as i16) } else { total as i16 }
                } else {
                    old
                },
                accepted,
            )
        }
        _ => (old, false),
    })
}

/// Native 181860 adds decimal exponent to the existing finite mantissa helper.
/// x/X is a foreign suffix (0x... is decimal-prefix 0), NOT C99 hex floating.
/// The scanner consumes an incomplete exponent as exponent 0, not parse failure.
pub(crate) fn tone_crt(source: &[u8], point: u8) -> Result<f64, Error> {
    let prefix = source.split(|&b| b == 0).next().unwrap_or_default();
    let text = &prefix[prefix
        .iter()
        .position(|b| !matches!(*b, 9..=13 | 32))
        .unwrap_or(prefix.len())..];
    if text.starts_with(b"+NAN") || text.starts_with(b"-NAN") {
        return Err(Error::Operation(
            "native tone NaN comparison raises invalid under normal unmasked x87 CW".into(),
        ));
    }
    if text.starts_with(b"+INF") {
        return Ok(f64::INFINITY);
    }
    if text.starts_with(b"-INF") {
        return Ok(f64::NEG_INFINITY);
    }
    match csv::number::crt_decimal_prefix(source, point) {
        Ok(n) => Ok(n),
        Err(Error::SourceUnavailable(_)) => {
            let at = prefix
                .iter()
                .position(|b| b"eExX".contains(b))
                .expect("finite helper exponent/hex stop");
            let mantissa = csv::number::crt_decimal_prefix(&prefix[..at], point)?;
            if !prefix[..at].iter().any(u8::is_ascii_digit) {
                return Ok(mantissa);
            }
            if matches!(prefix[at], b'x' | b'X') {
                return Ok(mantissa);
            }
            let tail = &prefix[at + 1..];
            let sign = usize::from(tail.first().is_some_and(|b| matches!(b, b'+' | b'-')));
            let count = tail[sign..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            if count == 0 {
                return Ok(mantissa);
            }
            let mut normalized: Vec<_> = prefix[..at]
                .iter()
                .map(|&b| if b == point { b'.' } else { b })
                .collect();
            normalized.push(b'e');
            normalized.extend_from_slice(&tail[..sign + count]);
            let start = normalized
                .iter()
                .position(|b| !matches!(*b, 9..=13 | 32))
                .unwrap_or(normalized.len());
            std::str::from_utf8(&normalized[start..])
                .map_err(|_| Error::Conversion("native CRT exponent bytes".into()))?
                .parse::<f64>()
                .map_err(|_| Error::Conversion("native CRT exponent".into()))
        }
        Err(error) => Err(error),
    }
}

/// 6D342/6D7AF/6D9FA/6DC62 consume actual input, not a supplied helper result.
pub fn number(
    w: &mut Working,
    holder: &mut DecodedRecord,
    field: NumberField,
    input: NumberInput<'_>,
    r: &Runtime<'_>,
    effect: &mut Effect,
    nls: &mut Nls<'_>,
) -> Result<(), Error> {
    let point = r.crt_decimal;
    let mut row = snapshot(holder, r);
    let k = kind(&row);
    let result = (|| {
        match field {
            NumberField::Frequency => {
                let old = value(&row, 0x3c);
                let offset = value(&row, 0x48);
                let bounds = r.ranges.f760(0x0c, -1, 0);
                let (candidate, accepted) =
                    scaled_input(input, old, bounds.low, bounds.high, point, true, effect)?;
                effect.helper_accepted = accepted;
                effect.candidate = Some(i64::from(candidate));
                if accepted {
                    effect.native.changed = true;
                    effect.native.flags |= 0x20;
                    if (candidate as i32) >= 0 {
                        put(&mut row, 0x3c, candidate);
                        let new_class = r.ranges.db98(candidate).f094(candidate) as i32;
                        let old_class = r.ranges.db98(old).f094(old) as i32;
                        let new_offset = if (old == 0) != (candidate == 0) || old_class != new_class
                        {
                            icf::frequency::f20c(r.ranges, r.system_langid, new_class).map_err(
                                |e| {
                                    effect.source_error = Some(e.to_string());
                                    Error::SourceUnavailable(
                                        "actual F20C source unavailable".into(),
                                    )
                                },
                            )?
                        } else {
                            offset
                        };
                        if new_offset != offset {
                            put(&mut row, 0x48, new_offset);
                            let d = match k {
                                1 | 3 | 5 => 0,
                                2 => 1,
                                4 => 3,
                                _ => u32::from(new_offset != 0),
                            };
                            put(&mut row, 0x44, d);
                        }
                        if old == 0 && candidate != 0 {
                            put(&mut row, 0x17c, 1);
                        }
                        effect.native.save_requested = true;
                        save(w, &mut row)?;
                        effect.native.save_returned = true;
                    }
                }
                let h = DecodedRecord::from_bytes(*row.bytes);
                if predicate::frequency_bad(&h, r.ranges, r.live_q) {
                    effect.native.warning_flags |= 4;
                }
                if predicate::numeric_frequency_bad(&h) {
                    effect.native.warning_flags |= 8;
                }
                effect.gray = fields::paint_meta_bytes(&row.bytes, 0x14a)?.gray;
            }
            NumberField::Offset => {
                let meta = fields::paint_meta_bytes(&row.bytes, 0x14c)?;
                let blocked = meta.gate != fields::Gate::Open;
                effect.blocked = blocked;
                if let Some(placeholder) = meta.placeholder() {
                    effect.display = placeholder.to_vec();
                }
                if !blocked {
                    effect.gray = meta.gray;
                    let class = r
                        .ranges
                        .db98(value(&row, 0x3c))
                        .edf0(0x0c, value(&row, 0x3c));
                    let (candidate, accepted) = scaled_input(
                        input,
                        value(&row, 0x48),
                        0,
                        if class == 3 { 99_999_900 } else { 9_999_900 },
                        point,
                        false,
                        effect,
                    )?;
                    effect.helper_accepted = accepted;
                    effect.candidate = Some(i64::from(candidate));
                    fields::select_offset(
                        &mut row,
                        false,
                        u32::from(accepted),
                        candidate,
                        &mut effect.native,
                        |_| Ok(()),
                        |row| save(w, row),
                    )?;
                }
            }
            NumberField::ToneFrequency => {
                effect.blocked =
                    fields::paint_meta_bytes(&row.bytes, 0x14e)?.gate != fields::Gate::Open;
                if !effect.blocked {
                    let old = value(&row, 0x178);
                    let order: Vec<_> = (0..10)
                        .flat_map(|r| (0..5).map(move |c| r + 10 * c))
                        .collect();
                    let selection = match input {
                        NumberInput::Text(text) => {
                            effect.shared_input = csv::trim(text).to_vec();
                            (nls.upper)(&mut effect.shared_input)?;
                            let input = tone_crt(&effect.shared_input, point)?;
                            let mut best = order[0];
                            let mut difference = f64::INFINITY;
                            for &index in &order {
                                let tone = crate::repeater::REPEATER_TONES[index as usize];
                                let label = format!("{}.{}", tone / 10, tone % 10);
                                let distance = (input
                                    - csv::number::crt_decimal_prefix(label.as_bytes(), point)?)
                                .abs();
                                if distance < difference {
                                    best = index;
                                    difference = distance;
                                }
                            }
                            Selection {
                                accepted: true,
                                candidate: best,
                            }
                        }
                        NumberInput::Selected(e) => Selection {
                            accepted: e.key == 1 && e.phase == 2 && e.accepted,
                            candidate: e.candidate as u32,
                        },
                        NumberInput::Next | NumberInput::Previous => selected(
                            old,
                            &order,
                            1,
                            if input == NumberInput::Next {
                                ChoiceInput::Next
                            } else {
                                ChoiceInput::Previous
                            },
                        ),
                        NumberInput::Begin => {
                            effect.popup_requested = true;
                            Selection {
                                accepted: false,
                                candidate: old,
                            }
                        }
                        _ => Selection {
                            accepted: false,
                            candidate: old,
                        },
                    };
                    effect.helper_accepted = selection.accepted;
                    effect.candidate = Some(i64::from(selection.candidate));
                    fields::select(
                        &mut row,
                        fields::Field::ToneFrequency,
                        selection,
                        &mut effect.native,
                        |row| save(w, row),
                        |_| Ok(()),
                    )?;
                    effect.gray = fields::paint_meta_bytes(&row.bytes, 0x14e)?.gray;
                }
            }
            NumberField::UtcOffset => {
                let old = value(&row, 0x184) as i16;
                let (candidate, accepted) = utc_input(input, old, effect)?;
                effect.helper_accepted = accepted;
                effect.candidate = Some(i64::from(candidate));
                let display = if matches!(input, NumberInput::Text(_)) {
                    candidate
                } else {
                    old
                };
                effect.display = csv::payload::utc_text(i32::from(display))?;
                fields::select_utc_offset(
                    &mut row,
                    u32::from(accepted),
                    candidate,
                    &mut effect.native,
                    |row| save(w, row),
                )?;
            }
        }
        Ok(())
    })();
    *holder = DecodedRecord::from_bytes(*row.bytes);
    result
}

/// Actual 9E714 fixed-position phase 2 parse. DMSType is the live current 396B4
/// byte. It is deliberately NOT CSV decimal-coordinate parsing or a raw4 input.
pub fn coordinate(
    w: &mut Working,
    holder: &mut DecodedRecord,
    longitude: bool,
    input: TextInput<'_>,
    r: &Runtime<'_>,
    effect: &mut Effect,
) -> Result<(), Error> {
    let mut row = snapshot(holder, r);
    let at = if longitude { 0x25e } else { 0x25a };
    let result = (|| {
        match input {
            TextInput::Begin(first) => {
                effect.state_out = 1;
                if let Some(first) = first {
                    effect.display = vec![first];
                }
            }
            TextInput::Text(text) => {
                effect.shared_input = text.to_vec();
                csv::number::coordinate_native_phase2(
                    (&mut row.bytes[at..at + 4])
                        .try_into()
                        .expect("decoded coordinate width"),
                    text,
                    w.working(),
                )?;
                effect.helper_accepted = true;
                effect.native.save_requested = true;
                save(w, &mut row)?;
                effect.native.save_returned = true;
                effect.native.changed = true;
            }
            TextInput::Display | TextInput::Cancel => {}
        }
        effect.gray =
            fields::paint_meta_bytes(&row.bytes, if longitude { 0x152 } else { 0x151 })?.gray;
        Ok(())
    })();
    *holder = DecodedRecord::from_bytes(*row.bytes);
    result
}

/// Normal ASCII key/control input through the real mask, validator and modal
/// callbacks. The lower-level coordinate() remains an explicit phase 2 cutpoint.
pub fn coordinate_input(
    w: &mut Working,
    holder: &mut DecodedRecord,
    longitude: bool,
    input: TextInput<'_>,
    events: &[csv::number::CoordinateEvent],
    r: &Runtime<'_>,
    effect: &mut Effect,
) -> Result<(), Error> {
    use csv::number::{CoordinateDialog, CoordinateEvent};
    let mut dialog = CoordinateDialog::new(
        *holder.coordinate_bytes(longitude),
        longitude,
        w.working(),
        match input {
            TextInput::Begin(first) => first,
            _ => None,
        },
    )?;
    match input {
        TextInput::Text(text) => dialog.replace(text),
        TextInput::Begin(_) => {
            effect.state_out = 1;
            effect.popup_requested = true;
        }
        TextInput::Cancel => {
            dialog.event(CoordinateEvent::Cancel);
        }
        TextInput::Display => {}
    }
    let result = (|| {
        if matches!(input, TextInput::Text(_)) {
            for &event in events {
                if let Some(text) = dialog.event(event) {
                    coordinate(w, holder, longitude, TextInput::Text(&text), r, effect)?;
                    dialog.dispatched();
                }
            }
        }
        Ok(())
    })();
    effect.display = dialog.control.clone();
    effect.gray =
        fields::paint_meta_bytes(holder.as_bytes(), if longitude { 0x152 } else { 0x151 })?.gray;
    effect.coordinate_dialog = Some(dialog);
    result
}

fn value(row: &Record, at: usize) -> u32 {
    u32::from_le_bytes(row.bytes[at..at + 4].try_into().expect("decoded DWORD"))
}
fn put(row: &mut Record, at: usize, n: u32) {
    row.bytes[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn snapshot(holder: &DecodedRecord, r: &Runtime<'_>) -> Record {
    Record {
        bytes: Box::new(*holder.as_bytes()),
        frequency_class: r.ranges.f094(holder.channel_field(C::FrequencyHz)),
    }
}
fn save(w: &mut Working, row: &mut Record) -> Result<(), Error> {
    let mut h = DecodedRecord::from_bytes(*row.bytes);
    w.save_reload(&mut h).map_err(|error| match error {
        icf::Error::Invalid { reason, .. }
        | icf::Error::Unsupported { reason, .. }
        | icf::Error::Operation(reason) => Error::Operation(reason),
        icf::Error::ChecksumMismatch { .. } => {
            Error::Operation("repeater typed save/reload checksum mismatch".into())
        }
    })?;
    row.bytes.copy_from_slice(h.as_bytes());
    Ok(())
}
fn kind(row: &Record) -> u32 {
    staging::classify(row)
}
fn selected(old: u32, choices: &[u32], key: u16, input: ChoiceInput) -> Selection {
    let candidate = match input {
        ChoiceInput::Selected(event) if event.key == key && event.phase == 2 && event.accepted => {
            Some(event.candidate as u32)
        }
        ChoiceInput::Next | ChoiceInput::Previous => {
            Some(match choices.iter().position(|&n| n == old) {
                None => choices[0],
                Some(at) => {
                    choices[if input == ChoiceInput::Next {
                        (at + 1) % choices.len()
                    } else {
                        (at + choices.len() - 1) % choices.len()
                    }]
                }
            })
        }
        _ => None,
    };
    Selection {
        accepted: candidate.is_some(),
        candidate: candidate.unwrap_or(old),
    }
}
fn style(row: &Record, field: ChoiceField) -> Result<bool, Error> {
    let key = match field {
        ChoiceField::Duplex => 0x14b,
        ChoiceField::Use => 0x14f,
        _ => return Ok(false),
    };
    Ok(fields::paint_meta_bytes(&row.bytes, key)?.gray)
}

/// CT_Rptlist's invalid-view Enter path: insertion precedes the selector. G is
/// read at its native reset callback, never replaced with an empty generation.
pub fn begin_type(
    w: &mut Working,
    holder: &mut DecodedRecord,
    group: usize,
    relative: usize,
    r: &Runtime<'_>,
    effect: &mut Effect,
) -> Result<(), Error> {
    let before = w
        .groups()
        .map_err(|_| Error::Operation("repeater group source unavailable".into()))?;
    if before.total() >= 2500 {
        effect.blocked = true;
        return Ok(());
    }
    let absolute = before
        .start(group)
        .map_err(|_| Error::Invalid("repeater group outside native axis".into()))?
        as usize
        + relative;
    let seed = record::reset_holder(0x0c, absolute as u32, holder.binding());
    let result = w.insert_one(
        group,
        relative,
        &seed,
        |w| {
            Ok(
                DefaultContext::from_context(r.live_q, r.system_langid)?.holder(
                    w.groups()?.group_of(absolute as i32) as u32,
                    holder.binding(),
                ),
            )
        },
        |_, which| {
            record::repeater_reset_word(
                r.generation.ok_or(icf::Error::Invalid {
                    line: 0,
                    reason: "actual G source unavailable".into(),
                })?,
                which,
            )
        },
    );
    effect.inserted = w
        .groups()
        .map_err(|_| Error::Operation("repeater post-insert group source unavailable".into()))?
        .total()
        != before.total();
    result.map_err(|e| {
        effect.source_error = Some(e.to_string());
        Error::SourceUnavailable("native repeater insertion source failed".into())
    })?;
    effect.notifications.extend([(0x40, true), (0x39, true)]);
    w.load_into(absolute, holder)
        .map_err(|_| Error::Operation("repeater inserted row reload failed".into()))?;
    holder.set_valid(false);
    Ok(())
}

/// Actual selector invocation. Menus and raw selected candidates are distinct:
/// Type advertises 0..3, but the helper does not invent a candidate range guard.
pub fn choice(
    w: &mut Working,
    holder: &mut DecodedRecord,
    field: ChoiceField,
    input: ChoiceInput,
    r: &Runtime<'_>,
    effect: &mut Effect,
) -> Result<(), Error> {
    let mut row = snapshot(holder, r);
    effect.popup_requested = input == ChoiceInput::Begin;
    let result = (|| {
        let (at, key, menu): (usize, u16, &[u32]) = match field {
            ChoiceField::Type => (0, 1, &[0, 1, 2, 3]),
            ChoiceField::Mode => (0x15c, 10, &[2, 0, 1, 3]),
            ChoiceField::Duplex => (0x44, 11, &[0, 1, 2, 3]),
            ChoiceField::Tone => (0x174, 1, &[0, 1, 2]),
            ChoiceField::Use => (0x17c, 1, &[0, 1]),
            ChoiceField::Position => (0x180, 1, &[0, 1, 2]),
        };
        let old = if field == ChoiceField::Type {
            kind(&row)
        } else {
            value(&row, at)
        };
        let filtered = if field == ChoiceField::Duplex {
            menu.iter()
                .copied()
                .filter_map(|n| {
                    match fields::candidate_allowed(&row, fields::Field::Duplex, n, old) {
                        Ok(true) => Some(Ok(n)),
                        Ok(false) => None,
                        Err(e) => Some(Err(e)),
                    }
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            menu.to_vec()
        };
        // A blocked DUP row never reaches cycling, including an empty menu.
        let blocked_dup = field == ChoiceField::Duplex
            && fields::paint_meta_bytes(&row.bytes, 0x14b)?.gate != fields::Gate::Open;
        let mut selection = if blocked_dup {
            Selection {
                accepted: false,
                candidate: old,
            }
        } else {
            selected(old, &filtered, key, input)
        };
        if field == ChoiceField::Mode {
            effect.blocked =
                fields::paint_meta_bytes(&row.bytes, 0x146)?.gate != fields::Gate::Open;
            if !matches!(input, ChoiceInput::Selected(_)) {
                selection.accepted = false;
                effect.popup_requested = false;
            }
        }
        effect.helper_accepted = selection.accepted;
        effect.candidate = Some(i64::from(selection.candidate));
        if field == ChoiceField::Type {
            fields::select_type(
                &mut row,
                selection,
                TYPE_SOURCE,
                &mut effect.native,
                || {
                    DefaultContext::from_context(r.live_q, r.system_langid)
                        .map(|d| Record {
                            bytes: Box::new(*d.holder(holder.group(), holder.binding()).as_bytes()),
                            frequency_class: 0,
                        })
                        .map_err(|e| {
                            effect.source_error = Some(e.to_string());
                            Error::SourceUnavailable(
                                "actual type22 Q/LANGID source unavailable".into(),
                            )
                        })
                },
                |row| save(w, row),
            )?;
        } else if matches!(
            field,
            ChoiceField::Mode | ChoiceField::Duplex | ChoiceField::Tone
        ) {
            let f = match field {
                ChoiceField::Mode => fields::Field::Mode,
                ChoiceField::Duplex => fields::Field::Duplex,
                _ => fields::Field::Tone,
            };
            let gray = std::cell::Cell::new(false);
            let out = fields::select(
                &mut row,
                f,
                selection,
                &mut effect.native,
                |row| save(w, row),
                |row| {
                    gray.set(style(row, field)?);
                    Ok(())
                },
            )?;
            effect.blocked |=
                matches!(out, fields::Outcome::Blocked | fields::Outcome::DisplayOnly);
            if matches!(out, fields::Outcome::Blocked | fields::Outcome::DisplayOnly) {
                effect.popup_requested = false;
                effect.helper_accepted = false;
            }
            effect.gray = gray.get();
        } else {
            if selection.accepted {
                put(&mut row, at, selection.candidate);
                effect.native.save_requested = true;
                save(w, &mut row)?;
                effect.native.save_returned = true;
                effect.native.changed = true;
                effect.native.flags |= 8;
            }
            if field == ChoiceField::Use {
                effect.gray = style(&row, field)?;
            }
        }
        Ok(())
    })();
    *holder = DecodedRecord::from_bytes(*row.bytes);
    result
}

/// Repeater Name/SubName use mode 1; callsigns use the existing mode 0 NLS pipeline.
/// The native return outputs are real byte transforms supplied by the caller.
pub fn text(
    w: &mut Working,
    holder: &mut DecodedRecord,
    field: TextField,
    input: TextInput<'_>,
    r: &Runtime<'_>,
    effect: &mut Effect,
    nls: &mut Nls<'_>,
) -> Result<(), Error> {
    let mut row = snapshot(holder, r);
    let k = kind(&row);
    let (at, key) = match field {
        TextField::Name => (0x26, 0x144),
        TextField::SubName => (0x154, 0x145),
        TextField::Call => (0x160, 0x147),
        TextField::Gateway => (0x168, 0x148),
    };
    let width = if field == TextField::Name { 16 } else { 8 };
    let meta = fields::paint_meta_bytes(&row.bytes, key)?;
    effect.blocked = meta.gate != fields::Gate::Open;
    if let Some(placeholder) = meta.placeholder() {
        effect.display = placeholder.to_vec();
    }
    let result = (|| {
        if !effect.blocked {
            match input {
                TextInput::Begin(first) => {
                    effect.state_out = 2;
                    effect.display = first.map_or_else(
                        || {
                            row.bytes[at..at + width]
                                .split(|&b| b == 0)
                                .next()
                                .unwrap()
                                .to_vec()
                        },
                        |b| vec![b],
                    );
                }
                TextInput::Text(text) => {
                    let mut shared = text.to_vec();
                    let candidate = if matches!(field, TextField::Name | TextField::SubName) {
                        csv::nls::name_candidate(&mut shared, width)
                    } else {
                        csv::nls::whole_text(
                            &mut shared,
                            csv::nls::Profile {
                                uppercase: true,
                                keep_kana: false,
                                keep_spaces: true,
                                punctuation_exceptions: b"/",
                                maximum: 8,
                            },
                            &mut nls.map,
                            &mut nls.upper,
                        )
                    };
                    effect.shared_input = shared;
                    let mut candidate = candidate?;
                    candidate.resize(width, b' ');
                    if matches!(field, TextField::Call | TextField::Gateway) && r.align_callsign {
                        let end = candidate
                            .iter()
                            .rposition(|&b| b != b' ')
                            .map_or(0, |n| n + 1);
                        if end > 0 && (field == TextField::Call || candidate[end - 1] == b'G') {
                            let start = candidate[..end]
                                .iter()
                                .rposition(|&b| b == b' ')
                                .map_or(0, |n| n + 1);
                            if start != 0 {
                                let word = candidate[start..end].to_vec();
                                candidate[start..].fill(b' ');
                                candidate[8 - word.len()..].copy_from_slice(&word);
                            }
                        }
                    }
                    let old: [u8; 8] = row.bytes[0x160..0x168].try_into().expect("call width");
                    row.bytes[at..at + width].copy_from_slice(&candidate);
                    effect.helper_accepted = true;
                    effect.native.changed = true;
                    let veto =
                        field == TextField::Call && matches!(k, 0 | 4) && candidate == [b' '; 8];
                    if field == TextField::Call
                        && !veto
                        && (old == TYPE_SOURCE.placeholder || old == [b' '; 8])
                        && candidate != TYPE_SOURCE.placeholder
                        && candidate != [b' '; 8]
                        && row.bytes[0x168..0x170]
                            == if k == 4 {
                                TYPE_SOURCE.gateway_marker
                            } else {
                                [b' '; 8]
                            }
                    {
                        row.bytes[0x168..0x170].copy_from_slice(&candidate);
                        row.bytes[0x16f] = b'G';
                    }
                    if field == TextField::Gateway && row.bytes[0x16f] != b'G' {
                        row.bytes[0x16f] = if kind(&row) == 4 { b'G' } else { b' ' };
                    }
                    if !veto {
                        effect.native.save_requested = true;
                        save(w, &mut row)?;
                        effect.native.save_returned = true;
                        if field == TextField::Call {
                            effect.native.flags |= 0x20;
                        }
                    }
                    effect.display = candidate;
                }
                TextInput::Display | TextInput::Cancel => {
                    effect.display = row.bytes[at..at + width]
                        .split(|&b| b == 0)
                        .next()
                        .unwrap()
                        .to_vec()
                }
            }
        }
        if field == TextField::Call
            && predicate::callsign_bad(
                w.working(),
                &DecodedRecord::from_bytes(*row.bytes),
                r.ranges,
            )
            .map_err(|_| {
                Error::Operation("repeater callsign predicate source read failed".into())
            })?
        {
            effect.native.warning_flags |= 4;
        }
        Ok(())
    })();
    *holder = DecodedRecord::from_bytes(*row.bytes);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icf::record::{RepeaterField as R, RepeaterText as T};

    #[test]
    fn native_tone_crt_exponent_hex_stop_upper_output_and_high_byte_filter() {
        for (source, expected) in [
            (b"8.85E1".as_slice(), 88.5),
            (b"0X58.8P1", 0.0),
            (b"8.85E+", 8.85),
            (b"\x0b+8.85E+1", 88.5),
            (b"E1", 0.0),
            (b".E2", 0.0),
            (b"88Z E2", 88.0),
            (b"-1E-2", -0.01),
            (b"1E100", 1e100),
            (b"INF", 0.0),
        ] {
            assert_eq!(tone_crt(source, b'.').unwrap(), expected);
        }
        assert!(tone_crt(b"+INF", b'.').unwrap().is_infinite());
        assert!(tone_crt(b"-NAN", b'.').is_err());
        assert_eq!(tone_crt(b"8,85E1", b',').unwrap(), 88.5);
        let q = Context::from_bytes([0; 24]);
        let ranges = Ranges::generate(&q, None).unwrap();
        let r = runtime(&q, &ranges);
        let (mut w, mut h) = fixture();
        number(
            &mut w,
            &mut h,
            NumberField::Frequency,
            NumberInput::Text(b"1\x80e2"),
            b'.',
            &r,
            &mut Effect::default(),
        )
        .unwrap();
        assert_eq!(h.channel_field(C::FrequencyHz), 12_000_000);
        choice(
            &mut w,
            &mut h,
            ChoiceField::Type,
            event(1, 2),
            &r,
            &mut Effect::default(),
        )
        .unwrap();
        for (text, index) in [
            (b"8.85e1".as_slice(), 8),
            (b"0x58", 0),
            (b"1e100", 0),
            (b"+inf", 0),
            (b"97.4", 11),
        ] {
            let mut e = Effect::default();
            number(
                &mut w,
                &mut h,
                NumberField::ToneFrequency,
                NumberInput::Text(text),
                b'.',
                &r,
                &mut e,
            )
            .unwrap();
            assert_eq!(h.repeater_field(R::ToneIndex), index);
            assert!(e.native.save_returned);
        }
        let mut e = Effect::default();
        super::number(
            &mut w,
            &mut h,
            NumberField::ToneFrequency,
            NumberInput::Text(b"8.85e1\x82\xa0"),
            &r,
            &mut e,
            &mut Nls {
                map: &mut |_, _| panic!("tone never reaches LCMap"),
                upper: &mut |bytes| {
                    assert_eq!(bytes, b"8.85e1\x82\xa0");
                    bytes.copy_from_slice(b"8.85E1\x82\xa0");
                    Ok(0)
                },
            },
        )
        .unwrap();
        assert_eq!(h.repeater_field(R::ToneIndex), 8);
        assert_eq!(e.shared_input, b"8.85E1\x82\xa0");
        let before = w.working().to_vec();
        let mut e = Effect::default();
        assert!(
            super::number(
                &mut w,
                &mut h,
                NumberField::ToneFrequency,
                NumberInput::Text(b"lower"),
                &r,
                &mut e,
                &mut Nls {
                    map: &mut |_, _| panic!("tone never reaches LCMap"),
                    upper: &mut |bytes| {
                        bytes[0] = b'X';
                        Err(Error::SourceUnavailable(
                            "captured Upper late failure".into(),
                        ))
                    }
                }
            )
            .is_err()
        );
        assert_eq!(e.shared_input, b"Xower");
        assert_eq!(w.working(), before);
        assert!(!e.native.save_requested);
    }

    fn number(
        w: &mut Working,
        h: &mut DecodedRecord,
        field: NumberField,
        input: NumberInput<'_>,
        point: u8,
        r: &Runtime<'_>,
        e: &mut Effect,
    ) -> Result<(), Error> {
        let r = Runtime {
            crt_decimal: point,
            ..*r
        };
        super::number(
            w,
            h,
            field,
            input,
            &r,
            e,
            &mut Nls {
                map: &mut csv::nls::ascii_map,
                upper: &mut |bytes| {
                    if !bytes.is_ascii() {
                        return Err(Error::SourceUnavailable(
                            "test requires captured upper output".into(),
                        ));
                    }
                    bytes.make_ascii_uppercase();
                    Ok(bytes.len() as u32)
                },
            },
        )
    }

    #[test]
    fn group_name_native_mode1_has_no_row_or_topology_side_effects() {
        let mut m = vec![0xa5; 0x3c9c0];
        m[0x355a1] = 6;
        m[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        let mut image = icf::Icf::from_memory(m).unwrap();
        let before = image.memory().to_vec();
        let mut e = Effect::default();
        group_name(&mut image, 49, TextInput::Text(b" A\0B \x82\xa0"), &mut e).unwrap();
        let at = 0x2ea3b + 49 * 16;
        assert_eq!(&image.memory()[at..at + 16], b" A B \x82\xa0         ");
        assert_eq!(&image.memory()[..at], &before[..at]);
        assert_eq!(&image.memory()[at + 16..], &before[at + 16..]);
        assert_eq!(
            e.group_label,
            Some((172, b"50:  A B \x82\xa0         ".to_vec()))
        );
        assert_eq!(e.notifications, [(0x3e, false)]);
        assert!(e.refresh_if_binding_matches);
        let before = image.memory().to_vec();
        let mut e = Effect::default();
        group_name(&mut image, 49, TextInput::Text(b" A\0B \x82\xa0"), &mut e).unwrap();
        assert!(e.native.save_returned);
        assert_eq!(image.memory(), before);
        for input in [
            TextInput::Cancel,
            TextInput::Display,
            TextInput::Begin(None),
        ] {
            let mut e = Effect::default();
            group_name(&mut image, 49, input, &mut e).unwrap();
            assert!(!e.native.save_requested);
            assert!(e.notifications.is_empty());
            assert_eq!(image.memory(), before);
        }
        group_name(&mut image, 0, TextInput::Text(b""), &mut Effect::default()).unwrap();
        assert_eq!(&image.memory()[0x2ea3b..0x2ea4b], &[b' '; 16]);
        assert!(group_name(&mut image, 50, TextInput::Display, &mut Effect::default()).is_err());
    }

    fn fixture() -> (Working, DecodedRecord) {
        let mut m = vec![0; 0x4997a];
        m[0x49914..0x49916].copy_from_slice(&1_u16.to_le_bytes());
        for at in [0x392fd, 0x3c8b9, 0x3c903] {
            m[at..at + 2].fill(255);
        }
        let mut w = Working::from_working(m).unwrap();
        let mut h = record::reset_holder(0x0c, 0, 0);
        h.set_valid(true);
        h.set_repeater_field(R::Mode, 2);
        h.set_repeater_text(T::Call, b"CALLSIGN");
        h.set_repeater_text(T::Gateway, b"        ");
        w.save_reload(&mut h).unwrap();
        (w, h)
    }
    fn runtime<'a>(q: &'a Context, ranges: &'a Ranges) -> Runtime<'a> {
        Runtime {
            ranges,
            live_q: q,
            system_langid: None,
            generation: None,
            align_callsign: false,
            crt_decimal: b'.',
        }
    }
    fn event(key: u16, candidate: i32) -> ChoiceInput {
        ChoiceInput::Selected(icf::settings::Selection {
            key,
            phase: 2,
            candidate,
            accepted: true,
        })
    }
    fn ascii(
        w: &mut Working,
        h: &mut DecodedRecord,
        field: TextField,
        input: TextInput<'_>,
        r: &Runtime<'_>,
        e: &mut Effect,
    ) -> Result<(), Error> {
        text(
            w,
            h,
            field,
            input,
            r,
            e,
            &mut Nls {
                map: &mut csv::nls::ascii_map,
                upper: &mut |b| {
                    assert!(b.is_ascii());
                    b.make_ascii_uppercase();
                    Ok(b.len() as u32)
                },
            },
        )
    }

    #[test]
    fn type_same_source_failure_and_conditional_six_cases() {
        let q = Context::from_bytes([0; 24]);
        let ranges = Ranges::generate(&q, None).unwrap();
        let missing = Context::before_registry();
        let r = runtime(&missing, &ranges);
        let (mut w, mut h) = fixture();
        let mut e = Effect::default();
        choice(&mut w, &mut h, ChoiceField::Type, event(1, 0), &r, &mut e).unwrap();
        assert!(e.native.save_returned);
        assert!(!e.native.changed);
        let before = w.working().to_vec();
        let mut e = Effect::default();
        assert!(choice(&mut w, &mut h, ChoiceField::Type, event(1, 3), &r, &mut e).is_err());
        assert_eq!(h.repeater_field(R::Mode), 0);
        assert_eq!(h.repeater_text(T::Call), b"        ");
        assert!(e.native.changed);
        assert!(!e.native.save_requested);
        assert_eq!(w.working(), before);
        for (candidate, mode, dup) in [
            (0, 2, 0),
            (1, 2, 0),
            (2, 0, 1),
            (3, 0, 0),
            (4, 3, 3),
            (5, 3, 0),
        ] {
            let (mut w, mut h) = fixture();
            choice(
                &mut w,
                &mut h,
                ChoiceField::Type,
                event(1, candidate),
                &runtime(&q, &ranges),
                &mut Effect::default(),
            )
            .unwrap();
            assert_eq!(
                (h.repeater_field(R::Mode), h.channel_field(C::Duplex)),
                (mode, dup)
            );
        }
    }

    #[test]
    fn selectors_use_full_candidate_typed_projection_and_non_gate_gray() {
        let q = Context::from_bytes([0; 24]);
        let ranges = Ranges::generate(&q, None).unwrap();
        let r = runtime(&q, &ranges);
        for mode in [0, 1, 2, 3, 257] {
            let (mut w, mut h) = fixture();
            let mut e = Effect::default();
            choice(
                &mut w,
                &mut h,
                ChoiceField::Mode,
                event(10, mode),
                &r,
                &mut e,
            )
            .unwrap();
            assert!(e.blocked && e.helper_accepted && e.native.save_returned);
            assert_eq!(e.candidate, Some(i64::from(mode)));
            let before = w.working().to_vec();
            for input in [
                ChoiceInput::Display,
                ChoiceInput::Cancel,
                ChoiceInput::Next,
                ChoiceInput::Previous,
                ChoiceInput::Begin,
                event(1, 0),
            ] {
                let mut e = Effect::default();
                choice(&mut w, &mut h, ChoiceField::Mode, input, &r, &mut e).unwrap();
                assert!(!e.native.save_requested);
                assert_eq!(w.working(), before);
            }
        }
        let (mut w, mut h) = fixture();
        for dup in [0, 1, 2, 3] {
            let mut e = Effect::default();
            choice(
                &mut w,
                &mut h,
                ChoiceField::Duplex,
                event(11, dup),
                &r,
                &mut e,
            )
            .unwrap();
            assert!(e.native.save_returned);
        }
        choice(
            &mut w,
            &mut h,
            ChoiceField::Duplex,
            ChoiceInput::Next,
            &r,
            &mut Effect::default(),
        )
        .unwrap();
        assert_eq!(h.channel_field(C::Duplex), 0);
        for (field, candidate) in [(ChoiceField::Use, 257), (ChoiceField::Position, 258)] {
            let mut e = Effect::default();
            choice(&mut w, &mut h, field, event(1, candidate), &r, &mut e).unwrap();
            assert_eq!(e.candidate, Some(i64::from(candidate)));
            assert!(e.native.save_returned);
        }
        assert_eq!(h.repeater_field(R::Use), 1);
        assert_eq!(h.repeater_field(R::Position), 2);
        choice(
            &mut w,
            &mut h,
            ChoiceField::Type,
            event(1, 4),
            &r,
            &mut Effect::default(),
        )
        .unwrap();
        let mut e = Effect::default();
        choice(
            &mut w,
            &mut h,
            ChoiceField::Duplex,
            ChoiceInput::Begin,
            &r,
            &mut e,
        )
        .unwrap();
        assert!(e.blocked);
        assert!(!e.popup_requested);
        assert!(!e.helper_accepted);
        let mut e = Effect::default();
        choice(&mut w, &mut h, ChoiceField::Tone, event(1, 1), &r, &mut e).unwrap();
        assert!(!e.helper_accepted);
    }

    #[test]
    fn numeric_native_grammar_rounding_gray_and_optional_utc_local() {
        let q = Context::from_bytes([0; 24]);
        let ranges = Ranges::generate(&q, None).unwrap();
        let r = runtime(&q, &ranges);
        let (mut w, mut h) = fixture();
        for (text, expected, accepted) in [
            (b"4.0000005".as_slice(), 4_000_000, true),
            (b"1e2", 12_000_000, true),
            (b"", 0, true),
            (b"\0", 30_000, false),
        ] {
            let mut e = Effect::default();
            number(
                &mut w,
                &mut h,
                NumberField::Frequency,
                NumberInput::Text(text),
                b'.',
                &r,
                &mut e,
            )
            .unwrap();
            assert_eq!(e.candidate, Some(expected));
            assert_eq!(e.helper_accepted, accepted);
            if accepted {
                assert_eq!(h.channel_field(C::FrequencyHz), expected as u32);
            }
        }
        h.set_channel_field(C::FrequencyHz, 0);
        h.set_channel_field(C::Duplex, 0);
        h.set_repeater_field(R::Use, 0);
        w.save_reload(&mut h).unwrap();
        let mut e = Effect::default();
        number(
            &mut w,
            &mut h,
            NumberField::Offset,
            NumberInput::Text(b"0.600049"),
            b'.',
            &r,
            &mut e,
        )
        .unwrap();
        assert!(e.gray && e.native.save_returned);
        assert_eq!(h.offset_hz(), 600_000);
        assert_eq!(h.channel_field(C::Duplex), 0);
        for (text, candidate, accepted) in [
            (b"junk".as_slice(), 0, true),
            (b"+9:58", 540, false),
            (b"--:--99:99", 32767, false),
            (b"", 32767, true),
        ] {
            h.set_repeater_field(R::UtcOffset, 540);
            w.save_reload(&mut h).unwrap();
            let mut e = Effect::default();
            number(
                &mut w,
                &mut h,
                NumberField::UtcOffset,
                NumberInput::Text(text),
                b'.',
                &r,
                &mut e,
            )
            .unwrap();
            assert_eq!(
                (e.candidate, e.helper_accepted),
                (Some(candidate), accepted)
            );
            assert_eq!(
                h.repeater_field(R::UtcOffset),
                if accepted { candidate as u32 } else { 540 }
            );
        }
        let mut e = Effect::default();
        number(
            &mut w,
            &mut h,
            NumberField::UtcOffset,
            NumberInput::Next,
            b'.',
            &r,
            &mut e,
        )
        .unwrap();
        assert_eq!(h.repeater_field(R::UtcOffset) as i32, -840);
        assert_eq!(e.display, b"--:--");
        choice(
            &mut w,
            &mut h,
            ChoiceField::Type,
            event(1, 2),
            &r,
            &mut Effect::default(),
        )
        .unwrap();
        choice(
            &mut w,
            &mut h,
            ChoiceField::Tone,
            event(1, 2),
            &r,
            &mut Effect::default(),
        )
        .unwrap();
        number(
            &mut w,
            &mut h,
            NumberField::ToneFrequency,
            NumberInput::Text(b"88.5"),
            b'.',
            &r,
            &mut Effect::default(),
        )
        .unwrap();
        assert_eq!(h.repeater_field(R::ToneIndex), 8);
    }

    #[test]
    fn unresolved_b_fault_keeps_frequency_before_default_offset_and_zero_transition_sets_use() {
        let q = Context::from_bytes([0; 24]);
        let mut unresolved = Ranges::unpopulated();
        assert!(unresolved.regenerate(5, &q, None).is_err());
        assert_eq!(unresolved.selected(), 5);
        assert!(unresolved.source_q().is_some());
        let (mut w, mut h) = fixture();
        let before = w.working().to_vec();
        let mut e = Effect::default();
        assert!(
            number(
                &mut w,
                &mut h,
                NumberField::Frequency,
                NumberInput::Text(b"145"),
                b'.',
                &runtime(&q, &unresolved),
                &mut e
            )
            .is_err()
        );
        assert_eq!(h.channel_field(C::FrequencyHz), 145_000_000);
        assert_eq!(w.working(), before);
        assert!(e.native.changed);
        assert!(!e.native.save_requested);
        assert!(
            e.source_error
                .as_deref()
                .unwrap()
                .contains("system LANGID source unavailable")
        );
        let ranges = Ranges::generate(&q, None).unwrap();
        h.set_channel_field(C::FrequencyHz, 0);
        w.save_reload(&mut h).unwrap();
        number(
            &mut w,
            &mut h,
            NumberField::Frequency,
            NumberInput::Text(b"145"),
            b'.',
            &runtime(&q, &ranges),
            &mut Effect::default(),
        )
        .unwrap();
        assert_eq!(h.repeater_field(R::Use), 1);
        number(
            &mut w,
            &mut h,
            NumberField::Frequency,
            NumberInput::Text(b""),
            b'.',
            &runtime(&q, &ranges),
            &mut Effect::default(),
        )
        .unwrap();
        assert_eq!(h.repeater_field(R::Use), 1);
    }

    #[test]
    fn completed_b_offset_ignores_later_live_q_and_langid() {
        for (source, live, langid, offset145, offset1200) in [
            (0_u8, 1_u8, None, 0, 20_000_000),
            (1, 0, None, 600_000, 12_000_000),
            (0, 16, None, 0, 20_000_000),
            (1, 16, Some(0x411), 600_000, 12_000_000),
            (0, 16, Some(0x409), 0, 20_000_000),
        ] {
            let mut bytes = [0; 24];
            bytes[0] = source;
            let generation_q = Context::from_bytes(bytes);
            let ranges = Ranges::generate(&generation_q, None).unwrap();
            assert_eq!(ranges.selected(), u32::from(source));
            bytes[0] = live;
            let live_q = Context::from_bytes(bytes);
            let r = Runtime {
                system_langid: langid,
                ..runtime(&live_q, &ranges)
            };
            let (mut w, mut h) = fixture();
            for (text, offset, class) in
                [(b"145".as_slice(), offset145, 1), (b"1200", offset1200, 3)]
            {
                let mut e = Effect::default();
                number(
                    &mut w,
                    &mut h,
                    NumberField::Frequency,
                    NumberInput::Text(text),
                    b'.',
                    &r,
                    &mut e,
                )
                .unwrap();
                assert_eq!(ranges.f094(h.channel_field(C::FrequencyHz)), class);
                assert_eq!(h.offset_hz(), offset);
                assert!(e.native.save_returned);
                assert!(e.source_error.is_none());
            }
            let mut e = Effect::default();
            number(
                &mut w,
                &mut h,
                NumberField::Frequency,
                NumberInput::Text(b"300"),
                b'.',
                &r,
                &mut e,
            )
            .unwrap();
            assert_eq!(h.offset_hz(), 5_000_000);
            assert_eq!(e.native.warning_flags & 4, if live < 16 { 4 } else { 0 });
            assert_eq!(ranges.selected(), u32::from(source));
        }
    }

    #[test]
    fn names_callsigns_nls_zero_partial_failure_and_coordinate_phase_two_prefix() {
        let q = Context::from_bytes([0; 24]);
        let ranges = Ranges::generate(&q, None).unwrap();
        let r = runtime(&q, &ranges);
        let (mut w, mut h) = fixture();
        ascii(
            &mut w,
            &mut h,
            TextField::SubName,
            TextInput::Text(b"ABCDEF\x82\xa0X"),
            &r,
            &mut Effect::default(),
        )
        .unwrap();
        assert_eq!(h.repeater_text(T::SubName), b"ABCDEF\x82\xa0");
        ascii(
            &mut w,
            &mut h,
            TextField::Call,
            TextInput::Text(b"ja1abc b"),
            &r,
            &mut Effect::default(),
        )
        .unwrap();
        assert_eq!(h.repeater_text(T::Call), b"JA1ABC B");
        assert_eq!(h.repeater_text(T::Gateway), b"JA1ABC G");
        let mut e = Effect::default();
        text(
            &mut w,
            &mut h,
            TextField::Gateway,
            TextInput::Text(b"abc"),
            &r,
            &mut e,
            &mut Nls {
                map: &mut |src, dst| {
                    assert_eq!(src, b"abc   ");
                    dst[..2].copy_from_slice(b"xy");
                    Ok(0)
                },
                upper: &mut |dst| {
                    assert_eq!(dst, b"xy ");
                    dst[0] = b'Z';
                    Ok(0)
                },
            },
        )
        .unwrap();
        assert_eq!(h.repeater_text(T::Gateway), b"Zy      ");
        assert!(e.helper_accepted);
        let before = w.working().to_vec();
        let mut e = Effect::default();
        assert!(
            text(
                &mut w,
                &mut h,
                TextField::Call,
                TextInput::Text(b"a\0b"),
                &r,
                &mut e,
                &mut Nls {
                    map: &mut |_, _| Err(Error::SourceUnavailable(
                        "test LCMap source failure".into()
                    )),
                    upper: &mut |_| panic!()
                }
            )
            .is_err()
        );
        assert_eq!(e.shared_input, b"a b");
        assert_eq!(w.working(), before);
        for (unit, text, expected) in [
            (0, b"035x12y34zS".as_slice(), [35, 0x30, 0x34, b'S']),
            (1, b"035x12y34zS", [35, 0x31, 0x16, b'S']),
            (2, b"139x1234zW", [139, 0x1c, 0xec, b'W']),
        ] {
            let mut m = w.into_working();
            m[0x396b4] = unit;
            w = Working::from_working(m).unwrap();
            coordinate(
                &mut w,
                &mut h,
                unit == 2,
                TextInput::Text(text),
                &r,
                &mut Effect::default(),
            )
            .unwrap();
            assert_eq!(*h.coordinate_bytes(unit == 2), expected);
        }
        let mut m = w.into_working();
        m[0x396b4] = 1;
        w = Working::from_working(m).unwrap();
        let before = w.working().to_vec();
        assert!(
            coordinate(
                &mut w,
                &mut h,
                true,
                TextInput::Text(b"123x45"),
                &r,
                &mut Effect::default()
            )
            .is_err()
        );
        assert_eq!(h.coordinate_bytes(true)[0], 123);
        assert_eq!(w.working(), before);
    }

    #[test]
    fn type_insert_cancel_keeps_count_notifications_and_late_g_keeps_real_prefix() {
        let q = Context::from_bytes([0; 24]);
        let ranges = Ranges::generate(&q, None).unwrap();
        let mut r = runtime(&q, &ranges);
        let mut g = vec![0; 0x3c905];
        g[0x3c8b9..0x3c8bb].copy_from_slice(&1234_u16.to_le_bytes());
        g[0x3c903..0x3c905].copy_from_slice(&4321_u16.to_le_bytes());
        r.generation = Some(&g);
        let (mut w, mut h) = fixture();
        let mut e = Effect::default();
        begin_type(&mut w, &mut h, 0, 1, &r, &mut e).unwrap();
        assert!(e.inserted);
        assert_eq!(e.notifications, [(0x40, true), (0x39, true)]);
        assert_eq!(w.groups().unwrap().total(), 2);
        assert_eq!(w.groups().unwrap().references[1..], [1234, 4321]);
        choice(
            &mut w,
            &mut h,
            ChoiceField::Type,
            ChoiceInput::Cancel,
            &r,
            &mut e,
        )
        .unwrap();
        assert_eq!(w.groups().unwrap().total(), 2);
        let (w, mut h) = fixture();
        let mut m = w.into_working();
        m[0x3c8b9..0x3c8bb].fill(0);
        let mut w = Working::from_working(m).unwrap();
        let before = w.working().to_vec();
        let mut e = Effect::default();
        r.generation = Some(&g[..0x3c8bb]);
        assert!(begin_type(&mut w, &mut h, 0, 1, &r, &mut e).is_err());
        assert_eq!(w.groups().unwrap().references[1], 1234);
        assert!(e.inserted);
        assert!(e.source_error.is_some());
        assert_ne!(w.working(), before);
        assert_eq!(w.groups().unwrap().total(), 2);
        assert!(e.notifications.is_empty());
    }
    #[test]
    fn type_literals_menu_gate_and_name_are_real_typed_operations() {
        let mut m = vec![0; 0x4997a];
        m[0x49914..0x49916].copy_from_slice(&1_u16.to_le_bytes());
        let mut w = Working::from_working(m).unwrap();
        let q = Context::from_bytes([0; 24]);
        let ranges = Ranges::generate(&q, None).unwrap();
        let r = Runtime {
            ranges: &ranges,
            live_q: &q,
            system_langid: None,
            generation: None,
            align_callsign: false,
            crt_decimal: b'.',
        };
        let mut h = record::reset_holder(0x0c, 0, 0);
        h.set_valid(true);
        h.set_repeater_field(R::Mode, 2);
        h.set_repeater_text(T::Call, b"CALLSIGN");
        w.save(0, &h).unwrap();
        let mut effect = Effect::default();
        choice(
            &mut w,
            &mut h,
            ChoiceField::Type,
            ChoiceInput::Selected(icf::settings::Selection {
                key: 1,
                phase: 2,
                candidate: 2,
                accepted: true,
            }),
            &r,
            &mut effect,
        )
        .unwrap();
        assert_eq!(h.repeater_field(R::Mode), 0);
        assert_eq!(h.repeater_text(T::Call), b"        ");
        assert_eq!(effect.native.flags, 0x20);
        text(
            &mut w,
            &mut h,
            TextField::Name,
            TextInput::Text(b" A\0B "),
            &r,
            &mut Effect::default(),
            &mut Nls {
                map: &mut csv::nls::ascii_map,
                upper: &mut |b| {
                    b.make_ascii_uppercase();
                    Ok(b.len() as u32)
                },
            },
        )
        .unwrap();
        assert_eq!(h.name(), b" A B            ");
    }
}
