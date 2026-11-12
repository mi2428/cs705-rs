//! Native GPS field operations. Selection effects request notifications, not
//! native delivery. The parent supplies its actual view and post-gate input.

use crate::{
    csv,
    icf::{
        Error, Icf,
        record::{self, DecodedRecord},
        settings::{
            extended::{ChoiceEffect, ChoiceInput, NumberEffect, NumberInput, TextInput},
            gps,
        },
    },
};

const ALARM: usize = 0x3c622;
const NAMES: usize = 0x353f1;
const ORDER: usize = 0x3c385;
const RECORDS: usize = 0x32c19;

fn invalid(reason: &'static str) -> Error {
    Error::Invalid {
        line: 0,
        reason: reason.into(),
    }
}

/// A real Flex0 selector entry. Group uses the alarm wire ordinal 1..27, not
/// the row-group ordinal 0..26. Captions come from GI_Sheet_Sel_GPS_Alarm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAlarmChoice {
    Off,
    Memory,
    Receive,
    All,
    Group(u8),
}

impl RowAlarmChoice {
    pub const ALL: [Self; 31] = {
        let mut choices = [Self::Off; 31];
        choices[1] = Self::Memory;
        choices[2] = Self::Receive;
        choices[3] = Self::All;
        let mut index = 4;
        while index < choices.len() {
            choices[index] = Self::Group(index as u8 - 3);
            index += 1;
        }
        choices
    };

    pub fn index(self) -> Result<u8, Error> {
        match self {
            Self::Off => Ok(0),
            Self::Memory => Ok(1),
            Self::Receive => Ok(2),
            Self::All => Ok(3),
            Self::Group(group @ 1..=27) => Ok(group + 3),
            _ => Err(invalid("GPS row alarm choice group outside1..27")),
        }
    }
}

/// The parent view's captured group/start and single active absolute row.
/// C855C uses low16(active-start), including a negative/wrapped difference;
/// the selected range and holder.valid are deliberately not additional gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlarmView {
    pub group: u8,
    pub start: u32,
    pub active: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowAlarmEffect {
    pub choice: ChoiceEffect,
    pub marked: bool,
    /// Original synchronous 39 request; delivery remains the caller's boundary.
    pub notify_39: bool,
}

fn alarm_index(raw: &[u8]) -> u8 {
    match raw[0] {
        1 => 2,
        3 => 1,
        2 if raw[1] <= 27 => raw[1] + 3,
        _ => 0,
    }
}

/// 6BC5B/C8508/C855C: actual 31-entry producer and original accepted-event
/// consumer. The lower writer projects arbitrary accepted candidates to low 8,
/// as the original does; it does not clamp them to the normal producer domain.
/// Off/Receive/Group save only the global tuple, never the row's raw 34 bytes.
pub fn row_alarm(
    image: &mut Icf<'_>,
    view: AlarmView,
    input: ChoiceInput,
) -> Result<RowAlarmEffect, Error> {
    if view.group > 26 {
        return Err(invalid("GPS alarm view group outside0..26"));
    }
    let old = alarm_index(&image.memory()[ALARM..ALARM + 4]);
    let choices = RowAlarmChoice::ALL.map(|choice| i64::from(choice.index().unwrap()));
    let candidate =
        crate::icf::settings::extended::choice_candidate(i64::from(old), &choices, input);
    let row = view.active.wrapping_sub(view.start) as u16;
    if let Some(candidate) = candidate {
        let selector = candidate as u8;
        let tuple = match selector {
            0 => [0, 0, 0, 0],
            1 => {
                let [low, high] = row.to_le_bytes();
                [3, view.group, low, high]
            }
            2 => [1, 0, 0, 0],
            3 => [2, 0, 0, 0],
            _ => [2, selector - 3, 0, 0],
        };
        image.patch_memory(&[(ALARM, &tuple)])?;
    }
    let raw = &image.memory()[ALARM..ALARM + 4];
    let selector = alarm_index(raw);
    let marked = match selector {
        1 => raw[1] == view.group && u16::from_le_bytes([raw[2], raw[3]]) == row,
        3 => true,
        4..=30 => selector - 4 == view.group,
        _ => false,
    };
    Ok(RowAlarmEffect {
        choice: ChoiceEffect {
            stored: candidate.is_some(),
            value: i64::from(selector),
            notify_flags: if candidate.is_some() { 0x20 } else { 0 },
            gray: false,
            popup_requested: input == ChoiceInput::Begin,
            fixed_value: None,
        },
        marked,
        notify_39: candidate.is_some(),
    })
}

/// Load an actual occupied logical row through the shared raw 34 decoder. This
/// is a source producer, not an input path for a client-completed holder.
pub fn load(image: &Icf<'_>, absolute: usize, binding: u32) -> Result<DecodedRecord, Error> {
    let groups = image.gps_groups()?;
    let mut start = 0;
    for (group, entries) in groups.iter().enumerate() {
        let end = start + entries.physical_slots().len();
        if absolute < end {
            let physical = entries.physical_slots()[absolute - start];
            let raw = image
                .physical_gps_record_raw(physical)?
                .ok_or_else(|| invalid("GPS row loader physical record unavailable"))?;
            let mut holder = record::reset_holder(0x0a, absolute as u32, binding);
            record::decode_gps_into(&mut holder, raw.try_into().unwrap(), group as u32);
            return Ok(holder);
        }
        start = end;
    }
    Err(invalid("GPS row loader index outside occupied prefix"))
}

/// Re-resolve the LIVE target order. The holder may predate an order change.
/// Valid 0 returns before address lookup. The one codec emits all six fields in
/// original save order, not a field-only patch or a decoded-value normalization.
pub fn save(image: &mut Icf<'_>, absolute: usize, holder: &DecodedRecord) -> Result<bool, Error> {
    if !holder.valid() {
        return Ok(false);
    }
    if absolute >= 300 {
        return Err(invalid("GPS saver logical index outside0..300"));
    }
    let at = ORDER + 2 * absolute;
    let physical = usize::from(u16::from_le_bytes(
        image.memory()[at..at + 2].try_into().unwrap(),
    ));
    if physical >= 300 {
        return Err(invalid("GPS saver target has no physical record"));
    }
    let at = RECORDS + 34 * physical;
    let mut raw = image.memory()[at..at + 34].try_into().unwrap();
    record::encode_gps_into(holder, &mut raw, |offset, bytes| {
        image.patch_memory(&[(at + offset, bytes)])
    })?;
    Ok(true)
}

/// 6BBF2: the shared live-unit altitude editor mutates the holder only on helper
/// success, then requests the existing whole-record saver and reload. `accepted`
/// records helper success; `stored` is false when holder.valid suppressed I/O.
pub fn altitude(
    image: &mut Icf<'_>,
    absolute: usize,
    holder: &mut DecodedRecord,
    input: NumberInput<'_>,
    point: u8,
) -> Result<NumberEffect, Error> {
    let old = i32::from_le_bytes(holder.as_bytes()[0x264..0x268].try_into().unwrap());
    let mut effect = gps::edit_altitude_live(old, image, input, point)?;
    if effect.accepted {
        holder.as_mut_bytes()[0x264..0x268].copy_from_slice(&effect.raw_candidate.to_le_bytes());
        effect.stored = save(image, absolute, holder)?;
        *holder = load(image, absolute, holder.binding())?;
    }
    Ok(effect)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupNameEffect {
    pub stored: bool,
    pub state_out: u32,
    pub display: Vec<u8>,
    /// Copied queued 3E label (node ID, bytes), before conditional synchronous 39.
    pub label: Option<(u32, Vec<u8>)>,
    pub refresh_if_binding_matches: bool,
    /// Sanitized shared input, including bytes after an original embedded NUL.
    pub shared_input: Vec<u8>,
}

/// 6BE5A mode 1 name grammar. Public row group 1..26 maps to native name index 0..25.
/// Empty groups and empty accepted names are legal; no row or metadata is added.
pub fn group_name(
    image: &mut Icf<'_>,
    group: usize,
    input: TextInput<'_>,
) -> Result<GroupNameEffect, csv::Error> {
    if !(1..=26).contains(&group) {
        return Err(csv::Error::Invalid("GPS named group outside1..26".into()));
    }
    let at = NAMES + 16 * (group - 1);
    let mut effect = GroupNameEffect {
        stored: false,
        state_out: 0,
        display: image.memory()[at..at + 16]
            .split(|&b| b == 0)
            .next()
            .unwrap()
            .to_vec(),
        label: None,
        refresh_if_binding_matches: false,
        shared_input: Vec::new(),
    };
    match input {
        TextInput::Begin(first) => {
            effect.state_out = 2;
            if let Some(first) = first {
                effect.display = vec![first];
            }
        }
        TextInput::Text(source) => {
            effect.shared_input = source.to_vec();
            let mut name = csv::nls::name_candidate(&mut effect.shared_input, 16)?;
            name.resize(16, b' ');
            super::operations::rename_group_raw(image, group, name.as_slice().try_into().unwrap())
                .map_err(|_| csv::Error::Operation("GPS group name store failed".into()))?;
            let mut label = vec![b'A' + group as u8 - 1, b':', b' '];
            label.extend_from_slice(&name);
            effect.stored = true;
            effect.display = name;
            effect.label = Some((0xfd + group as u32 - 1, label));
            effect.refresh_if_binding_matches = true;
        }
        TextInput::Display | TextInput::Cancel => {}
    }
    Ok(effect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icf::settings::Selection;

    fn image() -> Icf<'static> {
        let mut bytes = vec![0xa7; 0x3c9c0];
        bytes[0x355a1] = 6;
        bytes[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        bytes[ORDER..ORDER + 600].fill(0xff);
        bytes[0x3c5dd..0x3c613].fill(0xff);
        bytes[ORDER..ORDER + 4].copy_from_slice(&[7, 0, 9, 0]);
        bytes[0x3c5e1..0x3c5e3].copy_from_slice(&0_u16.to_le_bytes());
        bytes[ALARM..ALARM + 4].fill(0);
        bytes[0x396b5] = 0;
        Icf::from_memory(bytes).unwrap()
    }

    fn selected(candidate: i32) -> ChoiceInput {
        ChoiceInput::Selected(Selection {
            key: 1,
            phase: 2,
            candidate,
            accepted: true,
        })
    }

    #[test]
    fn library_r2_row_alarm_31_choices_tuple_approval_marker_and_low_words() {
        let view = AlarmView {
            group: 2,
            start: 40,
            active: 43,
        };
        for (index, choice) in RowAlarmChoice::ALL.into_iter().enumerate() {
            assert_eq!(usize::from(choice.index().unwrap()), index);
            let mut image = image();
            let mut expected = image.memory().to_vec();
            let tuple = match index {
                0 => [0, 0, 0, 0],
                1 => [3, 2, 3, 0],
                2 => [1, 0, 0, 0],
                3 => [2, 0, 0, 0],
                _ => [2, index as u8 - 3, 0, 0],
            };
            expected[ALARM..ALARM + 4].copy_from_slice(&tuple);
            let out = row_alarm(&mut image, view, selected(index as i32)).unwrap();
            assert!(out.choice.stored && out.notify_39);
            assert_eq!(out.choice.notify_flags, 0x20);
            assert_eq!(out.marked, matches!(index, 1 | 3 | 6));
            assert_eq!(image.memory(), expected);
            for input in [
                ChoiceInput::Display,
                ChoiceInput::Cancel,
                ChoiceInput::Begin,
                ChoiceInput::Selected(Selection {
                    key: 1,
                    phase: 2,
                    candidate: 0,
                    accepted: false,
                }),
                ChoiceInput::Selected(Selection {
                    key: 2,
                    phase: 2,
                    candidate: 0,
                    accepted: true,
                }),
                ChoiceInput::Selected(Selection {
                    key: 1,
                    phase: 1,
                    candidate: 0,
                    accepted: true,
                }),
            ] {
                let rejected = row_alarm(&mut image, view, input).unwrap();
                assert!(!rejected.choice.stored && !rejected.notify_39);
                assert_eq!(rejected.choice.notify_flags, 0);
                assert_eq!(rejected.marked, out.marked);
                assert_eq!(image.memory(), expected);
            }
        }
        assert!(RowAlarmChoice::Group(0).index().is_err());
        assert!(RowAlarmChoice::Group(28).index().is_err());
        let mut image = image();
        let wrapped = AlarmView {
            group: 26,
            start: 65539,
            active: 1,
        };
        assert!(row_alarm(&mut image, wrapped, selected(1)).unwrap().marked);
        assert_eq!(&image.memory()[ALARM..ALARM + 4], &[3, 26, 0xfe, 0xff]);
        row_alarm(&mut image, view, selected(31)).unwrap();
        assert_eq!(&image.memory()[ALARM..ALARM + 4], &[2, 28, 0, 0]);
        let before = image.memory().to_vec();
        assert!(
            !row_alarm(&mut image, view, ChoiceInput::Display)
                .unwrap()
                .marked
        );
        assert_eq!(image.memory(), before);
        row_alarm(&mut image, view, selected(-256)).unwrap();
        assert_eq!(&image.memory()[ALARM..ALARM + 4], &[0; 4]);
        assert_eq!(
            row_alarm(&mut image, view, ChoiceInput::Previous)
                .unwrap()
                .choice
                .value,
            30
        );
        assert_eq!(
            row_alarm(&mut image, view, ChoiceInput::Next)
                .unwrap()
                .choice
                .value,
            0
        );
    }

    #[test]
    fn library_r2_altitude_reuses_live_saver_target_and_preserves_valid_gate_and_error_prefix() {
        let mut image = image();
        let mut holder = load(&image, 0, 91).unwrap();
        assert_eq!(
            (holder.kind(), holder.group(), holder.binding()),
            (0x0a, 2, 91)
        );
        let before = image.memory().to_vec();
        let holder_before = holder.clone();
        assert!(
            !altitude(&mut image, 0, &mut holder, NumberInput::Cancel, b'.')
                .unwrap()
                .accepted
        );
        assert!(holder == holder_before);
        assert_eq!(image.memory(), before);
        assert!(altitude(&mut image, 0, &mut holder, NumberInput::Text(b"\xff"), b'.').is_err());
        assert!(holder == holder_before);
        image.patch_memory(&[(ORDER, &[9, 0, 7, 0])]).unwrap();
        holder.as_mut_bytes()[0x244..0x254].fill(b'H');
        let mut expected = image.memory().to_vec();
        let target = RECORDS + 34 * 9;
        expected[target + 18..target + 34].fill(b'H');
        expected[target + 14..target + 18].copy_from_slice(&3281_i32.to_le_bytes());
        let out = altitude(&mut image, 0, &mut holder, NumberInput::Text(b"100"), b'.').unwrap();
        assert!(out.accepted && out.stored);
        assert_eq!(out.suffix, b"m");
        assert_eq!(image.memory(), expected);
        assert_eq!(&holder.as_bytes()[0x244..0x254], &[b'H'; 16]);
        holder.set_valid(false);
        let out = altitude(&mut image, 0, &mut holder, NumberInput::Text(b"0"), b'.').unwrap();
        assert!(out.accepted && !out.stored && holder.valid());
        assert_eq!(image.memory(), expected);
        assert_eq!(&holder.as_bytes()[0x264..0x268], &3281_i32.to_le_bytes());
        assert!(altitude(&mut image, 300, &mut holder, NumberInput::Text(b"0"), b'.').is_err());
        assert_eq!(&holder.as_bytes()[0x264..0x268], &[0; 4]);
        assert_eq!(image.memory(), expected);
        let out = altitude(&mut image, 0, &mut holder, NumberInput::Text(b"0"), b'.').unwrap();
        assert!(out.accepted && out.stored);
        assert_eq!(&image.memory()[target + 14..target + 18], &[0; 4]);
    }

    #[test]
    fn library_r2_group_names_use_mode1_whole_input_and_keep_empty_group_metadata() {
        let mut image = image();
        let before = image.memory().to_vec();
        for input in [TextInput::Cancel, TextInput::Begin(Some(b'Z'))] {
            assert!(!group_name(&mut image, 26, input).unwrap().stored);
            assert_eq!(image.memory(), before);
        }
        let out = group_name(&mut image, 26, TextInput::Text(b" a \0Tail \x82\xa0  ")).unwrap();
        let mut name = b" a  Tail \x82\xa0".to_vec();
        name.resize(16, b' ');
        let mut expected = before;
        expected[NAMES + 25 * 16..NAMES + 26 * 16].copy_from_slice(&name);
        assert_eq!(image.memory(), expected);
        assert_eq!(
            out.label,
            Some((0x116, [b"Z: ".as_slice(), &name].concat()))
        );
        assert!(out.stored && out.refresh_if_binding_matches);
        assert_eq!(out.shared_input, b" a  Tail \x82\xa0  ");
        let clipped = [b"AAAAAAAAAAAAAAA".as_slice(), b"\x82\xa0"].concat();
        group_name(&mut image, 1, TextInput::Text(&clipped)).unwrap();
        assert_eq!(&image.memory()[NAMES..NAMES + 16], b"AAAAAAAAAAAAAAA ");
        group_name(&mut image, 1, TextInput::Text(b"")).unwrap();
        assert_eq!(&image.memory()[NAMES..NAMES + 16], &[b' '; 16]);
        let before = image.memory().to_vec();
        assert!(group_name(&mut image, 0, TextInput::Text(b"bad")).is_err());
        assert_eq!(image.memory(), before);
    }
}
