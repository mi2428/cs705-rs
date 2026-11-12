//! Native A02AC modes 0..3. Mask input is NOT CSV import_raw grammar.
//! F61C8/F6230 token primitives are shared only under the native forced locale.

use super::{
    Error,
    datetime::{self, Context, Date, Time},
    number::{self, CoordinateDialog, CoordinateEvent},
};

/// Actual GetLocalTime WORD fields, never an implicit host-clock/default date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub millisecond: u16,
}
impl Clock {
    pub fn datetime(self) -> Result<(Date, Time), Error> {
        let narrow = |n| {
            u8::try_from(n)
                .map_err(|_| Error::Conversion("local clock WORD outside byte field".into()))
        };
        let date = Date {
            year: self.year,
            month: narrow(self.month)?,
            day: narrow(self.day)?,
        };
        let time = Time {
            hour: narrow(self.hour)?,
            minute: narrow(self.minute)?,
            second: narrow(self.second)?,
            millisecond: self.millisecond,
        };
        if !datetime::valid_date(date) || !datetime::valid_time(time) {
            return Err(Error::Conversion("local clock calendar/time fields".into()));
        }
        Ok((date, time))
    }
}

pub struct Runtime<'a> {
    pub locale: Context<'a>,
    pub time_format: &'a [u8],
    /// Live VA 8648E0, separate from the dialog's captured mode.
    pub date_order: u32,
    /// Required only by an invalid raw edit seed, not by valid display/parse.
    pub clock: Option<Clock>,
}

fn clock(runtime: &Runtime<'_>) -> Result<(Date, Time), Error> {
    runtime
        .clock
        .ok_or(Error::SourceUnavailable(
            "native datetime edit requires GetLocalTime fields".into(),
        ))?
        .datetime()
}
fn mode(mode: u8) -> Result<(), Error> {
    if mode > 3 {
        Err(Error::Invalid("native datetime mode outside0..3".into()))
    } else {
        Ok(())
    }
}
fn date_text(date: Date, order: u32) -> Vec<u8> {
    match order {
        0 => format!("{:04}/{:02}/{:02}", date.year, date.month, date.day),
        1 => format!("{:02}/{:02}/{:04}", date.month, date.day, date.year),
        _ => format!("{:02}/{:02}/{:04}", date.day, date.month, date.year),
    }
    .into_bytes()
}
fn time_text(time: Time) -> Vec<u8> {
    datetime::fixed_time(time, b':')
}

/// 9EA34/9EDC8 add the offset for display; invalid edit seeds use local clock
/// WITHOUT adding it again. Numeric formats are code literals, not NLS packets.
pub fn format(
    raw: [u8; 6],
    mode_value: u8,
    offset: i16,
    edit: bool,
    runtime: &mut Runtime<'_>,
) -> Result<Vec<u8>, Error> {
    mode(mode_value)?;
    let offset = datetime::effective_offset(offset)?;
    let (date, time) = datetime::decode_raw(raw);
    if mode_value < 3 {
        let date = if datetime::valid_date(date) && datetime::valid_time(time) {
            datetime::shift_minutes(date, time, offset)?.0
        } else if edit {
            clock(runtime)?.0
        } else {
            return Ok(if runtime.date_order == 0 {
                b"----/--/--".to_vec()
            } else {
                b"--/--/----".to_vec()
            });
        };
        let saved = runtime.locale.date_separator;
        runtime.locale.date_separator = b'/';
        let out = date_text(date, if edit { 0 } else { runtime.date_order });
        runtime.locale.date_separator = saved;
        Ok(out)
    } else {
        let time = if datetime::valid_time(time) {
            datetime::shift_minutes(
                if datetime::valid_date(date) {
                    date
                } else {
                    Date {
                        year: 2006,
                        month: 4,
                        day: 10,
                    }
                },
                time,
                offset,
            )?
            .1
        } else if edit {
            clock(runtime)?.1
        } else {
            return Ok(b"--:--:--".to_vec());
        };
        let saved = runtime.locale.time_separator;
        runtime.locale.time_separator = b':';
        let out = time_text(time);
        runtime.locale.time_separator = saved;
        Ok(out)
    }
}

/// Try-parse false is a NORMAL return and restores the temporary locale.
/// A genuine exceptional Error leaves the forced globals, as the original frame.
fn parse_date(source: &[u8], runtime: &mut Runtime<'_>) -> Result<Option<Date>, Error> {
    let saved = (runtime.locale.short_date, runtime.locale.date_separator);
    runtime.locale.short_date = b"yyyy/mm/dd";
    runtime.locale.date_separator = b'/';
    let result = match datetime::parse_date(source, &runtime.locale) {
        Ok(date) => Some(date),
        Err(Error::Conversion(_)) => None,
        Err(error) => return Err(error),
    };
    (runtime.locale.short_date, runtime.locale.date_separator) = saved;
    Ok(result)
}
fn parse_time(source: &[u8], runtime: &mut Runtime<'_>) -> Result<Option<Time>, Error> {
    let saved = (runtime.time_format, runtime.locale.time_separator);
    runtime.time_format = b"hh:nn:ss";
    runtime.locale.time_separator = b':';
    let result = match datetime::parse_time(source, &runtime.locale) {
        Ok(time) => Some(time),
        Err(Error::Conversion(_)) => None,
        Err(error) => return Err(error),
    };
    (runtime.time_format, runtime.locale.time_separator) = saved;
    Ok(result)
}
fn store_date(raw: &mut [u8; 6], source: &[u8], runtime: &mut Runtime<'_>) -> Result<(), Error> {
    if parse_date(source, runtime)?.is_some() {
        for (target, at) in [(2, 2), (1, 5), (0, 8)] {
            raw[target] = datetime::encode_bcd(number::native_integer(number::native_substring(
                source, at, 2,
            ))? as u8);
        }
    }
    Ok(())
}
fn store_time(raw: &mut [u8; 6], source: &[u8], runtime: &mut Runtime<'_>) -> Result<(), Error> {
    if parse_time(source, runtime)?.is_some() {
        for (target, at) in [(3, 0), (4, 3), (5, 6)] {
            raw[target] = datetime::encode_bcd(number::native_integer(number::native_substring(
                source, at, 2,
            ))? as u8);
        }
    }
    Ok(())
}
fn store_fields(raw: &mut [u8; 6], date: Option<Date>, time: Time) {
    if let Some(date) = date {
        raw[2] = datetime::encode_bcd((date.year % 100) as u8);
        raw[1] = datetime::encode_bcd(date.month);
        raw[0] = datetime::encode_bcd(date.day);
    }
    raw[3] = datetime::encode_bcd(time.hour);
    raw[4] = datetime::encode_bcd(time.minute);
    raw[5] = datetime::encode_bcd(time.second);
}

/// Native 9E88C computes offset before validity gates. Invalid Time skips all
/// writes; invalid Date with VALID Time uses 2006/04/10 and writes only Time.
fn subtract_offset(raw: &mut [u8; 6], offset: i16) -> Result<(), Error> {
    let (date, time) = datetime::decode_raw(*raw);
    let offset = datetime::effective_offset(offset)?;
    if !datetime::valid_time(time) {
        return Ok(());
    }
    let has_date = datetime::valid_date(date);
    let (date, time) = datetime::shift_minutes(
        if has_date {
            date
        } else {
            Date {
                year: 2006,
                month: 4,
                day: 10,
            }
        },
        time,
        -offset,
    )?;
    store_fields(raw, has_date.then_some(date), time);
    Ok(())
}

/// Real phase2 consumes canonical producer text. It is NOT CSV import_raw:
/// parse false skips only that half, then the peer formatter and offset run.
pub fn phase2(
    raw: &mut [u8; 6],
    mode_value: u8,
    offset: i16,
    source: &[u8],
    runtime: &mut Runtime<'_>,
) -> Result<(), Error> {
    mode(mode_value)?;
    if mode_value < 3 {
        store_date(raw, source, runtime)?;
        let time = format(*raw, 3, offset, true, runtime)?;
        store_time(raw, &time, runtime)?;
    } else {
        let date = format(*raw, 0, offset, true, runtime)?;
        store_date(raw, &date, runtime)?;
        store_time(raw, source, runtime)?;
    }
    subtract_offset(raw, offset)
}

const MASKS: [&[u8]; 4] = [
    b"!2\\099/99/99;1;_",
    b"!99/99/2\\099;1;_",
    b"!99/99/2\\099;1;_",
    b"!99:99:99;1;_",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateDialog {
    pub editor: CoordinateDialog,
    pub enabled: bool,
}
impl DateDialog {
    pub fn new(
        raw: [u8; 6],
        mode_value: u8,
        offset: i16,
        runtime: &mut Runtime<'_>,
        first: Option<u8>,
    ) -> Result<Self, Error> {
        mode(mode_value)?;
        let seed = format(raw, mode_value, offset, true, runtime)?;
        let display = if mode_value == 1 || mode_value == 2 {
            let sub = |at, len| number::native_substring(&seed, at, len);
            let (first, second) = if mode_value == 1 {
                (sub(5, 2), sub(8, 2))
            } else {
                (sub(8, 2), sub(5, 2))
            };
            [first, b"/", second, b"/", sub(0, 4)].concat()
        } else {
            seed.clone()
        };
        let mut editor =
            CoordinateDialog::from_mask(mode_value, MASKS[usize::from(mode_value)], display, first);
        editor.seed = seed;
        let mut dialog = Self {
            editor,
            enabled: false,
        };
        dialog.enabled = dialog.valid(runtime)?;
        Ok(dialog)
    }
    /// Normal full TMaskEdit text. Numeric literal 20 stays fixed; this is not a
    /// generic CSV date or a ready canonical phase2 packet supplied by a client.
    pub fn replace_masked(
        &mut self,
        source: &[u8],
        runtime: &mut Runtime<'_>,
    ) -> Result<(), Error> {
        self.editor.replace_masked(source)?;
        self.enabled = self.valid(runtime)?;
        Ok(())
    }
    fn candidate(&self, runtime: &Runtime<'_>) -> Vec<u8> {
        let text = self.editor.candidate();
        let sub = |at, len| number::native_substring(&text, at, len);
        match self.editor.mode {
            0 => [sub(0, 4), b"/", sub(5, 2), b"/", sub(8, 2)].concat(),
            1 => [sub(6, 4), b"/", sub(0, 2), b"/", sub(3, 2)].concat(),
            2 => [sub(6, 4), b"/", sub(3, 2), b"/", sub(0, 2)].concat(),
            _ => text
                .iter()
                .map(|&b| {
                    if b == runtime.locale.time_separator {
                        b':'
                    } else {
                        b
                    }
                })
                .collect(),
        }
    }
    pub fn valid(&self, runtime: &mut Runtime<'_>) -> Result<bool, Error> {
        let text = self.candidate(runtime);
        if self.editor.mode == 3 {
            return Ok(parse_time(&text, runtime)?.is_some());
        }
        Ok(parse_date(&text, runtime)?.is_some_and(|date| {
            ((2000, 1, 2)..=(2099, 12, 30)).contains(&(date.year, date.month, date.day))
        }))
    }
    pub fn event(
        &mut self,
        event: CoordinateEvent,
        runtime: &mut Runtime<'_>,
    ) -> Result<Option<Vec<u8>>, Error> {
        if self.editor.done || (event == CoordinateEvent::Deactivate && self.editor.cancel) {
            return Ok(self.editor.event_with_candidate(event, None));
        }
        if event == CoordinateEvent::Accept && !self.enabled {
            return Ok(None);
        }
        let candidate = if matches!(event, CoordinateEvent::Accept | CoordinateEvent::Deactivate)
            && self.valid(runtime)?
        {
            Some(self.candidate(runtime))
        } else {
            None
        };
        // Enabled is the prior OnChange result. Once a real Click reaches the
        // approval callback, a NEW false validation dispatches the original seed.
        let dispatch = if event == CoordinateEvent::Accept {
            CoordinateEvent::Deactivate
        } else {
            event
        };
        Ok(self.editor.event_with_candidate(dispatch, candidate))
    }
    pub fn apply_event(
        &mut self,
        raw: &mut [u8; 6],
        offset: i16,
        event: CoordinateEvent,
        runtime: &mut Runtime<'_>,
        effect: &mut Effect,
    ) -> Result<bool, Error> {
        let Some(text) = self.event(event, runtime)? else {
            return Ok(false);
        };
        input(
            raw,
            self.editor.mode,
            offset,
            Input::Text(&text),
            runtime,
            effect,
        )?;
        self.editor.dispatched();
        Ok(true)
    }
}

#[derive(Clone, Copy)]
pub enum Input<'a> {
    Display,
    Inline,
    Begin(Option<u8>),
    Text(&'a [u8]),
    Cancel,
}
#[derive(Debug, Default)]
pub struct Effect {
    pub changed: bool,
    /// None means the original state-out word was not written.
    pub state_out: Option<u32>,
    pub display: Vec<u8>,
    pub selection: (usize, usize),
    pub dialog: Option<DateDialog>,
}
pub fn input(
    raw: &mut [u8; 6],
    mode_value: u8,
    offset: i16,
    input: Input<'_>,
    runtime: &mut Runtime<'_>,
    effect: &mut Effect,
) -> Result<(), Error> {
    mode(mode_value)?;
    effect.changed = false;
    match input {
        Input::Text(text) => {
            phase2(raw, mode_value, offset, text, runtime)?;
            effect.changed = true;
        }
        Input::Inline | Input::Begin(_) => {
            let first = if let Input::Begin(first) = input {
                first
            } else {
                None
            };
            effect.dialog = Some(DateDialog::new(*raw, mode_value, offset, runtime, first)?);
            effect.state_out = Some(1);
        }
        Input::Cancel => {
            if let Some(dialog) = &mut effect.dialog {
                dialog
                    .editor
                    .event_with_candidate(CoordinateEvent::Cancel, None);
            }
        }
        Input::Display => {}
    }
    effect.display = format(*raw, mode_value, offset, false, runtime)?;
    effect.selection = if matches!(input, Input::Begin(Some(byte)) if byte.is_ascii_digit()) {
        (effect.display.len(), 0)
    } else {
        (0, effect.display.len())
    };
    Ok(())
}

/// A raw inline text gesture traverses the actual mask/validator/Accept producer
/// before phase2. The original raw 6 stays borrowed through all partial writes.
pub fn edit_masked(
    raw: &mut [u8; 6],
    mode_value: u8,
    offset: i16,
    text: &[u8],
    runtime: &mut Runtime<'_>,
    effect: &mut Effect,
) -> Result<(), Error> {
    input(raw, mode_value, offset, Input::Inline, runtime, effect)?;
    let mut dialog = effect.dialog.take().unwrap();
    let result = (|| {
        dialog.replace_masked(text, runtime)?;
        dialog.apply_event(raw, offset, CoordinateEvent::Accept, runtime, effect)?;
        Ok(())
    })();
    effect.dialog = Some(dialog);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_masks_phase_prefix_and_type20_clock_seed() {
        let mut current = vec![0; 0x396b5];
        current[0x396b4] = 1;
        let mut coordinate =
            CoordinateDialog::new([35, 0x31, 0x17, b'N'], false, &current, None).unwrap();
        assert_eq!(coordinate.control, b" 35\xb012'34\"N");
        assert_eq!(
            coordinate.event(CoordinateEvent::Accept).unwrap(),
            b"035\xb012'34\"N"
        );
        coordinate.dispatched();
        assert!(coordinate.done && coordinate.release_requested);
        let mut invalid_coordinate =
            CoordinateDialog::new([35, 0x31, 0x17, b'N'], false, &current, None).unwrap();
        invalid_coordinate.replace(b"990000N");
        assert!(invalid_coordinate.event(CoordinateEvent::Accept).is_none());
        assert_eq!(
            invalid_coordinate
                .event(CoordinateEvent::Deactivate)
                .unwrap(),
            b" 35\xb012'34\"N"
        );
        let mut runtime = Runtime {
            locale: Context {
                short_date: b"dd.mm.yyyy",
                date_separator: b'.',
                time_separator: b';',
                decimal_separator: b'.',
                am: b"AM",
                pm: b"PM",
                current_year: 2026,
                two_digit_window: 50,
                primary_language: 0,
                eras: &[],
            },
            time_format: b"hh;nn;ss",
            date_order: 2,
            clock: None,
        };
        let mut raw = [0x29, 0x02, 0x24, 0x23, 0x30, 0x40];
        let mut dialog = DateDialog::new(raw, 1, 60, &mut runtime, None).unwrap();
        assert_eq!(dialog.editor.seed, b"2024/03/01");
        assert_eq!(dialog.editor.control, b"03/01/2024");
        dialog.replace_masked(b"03/03/2024", &mut runtime).unwrap();
        let mut effect = Effect::default();
        assert!(
            dialog
                .apply_event(
                    &mut raw,
                    60,
                    CoordinateEvent::Accept,
                    &mut runtime,
                    &mut effect
                )
                .unwrap()
        );
        assert_eq!(raw, [0x02, 0x03, 0x24, 0x23, 0x30, 0x40]);
        assert!(effect.changed && dialog.editor.done && dialog.editor.release_requested);
        assert_eq!(effect.display, b"03/03/2024");
        let mut time_raw = [0x29, 0x02, 0x24, 0x23, 0x30, 0x40];
        let mut time_effect = Effect::default();
        edit_masked(
            &mut time_raw,
            3,
            60,
            b"01:45:10",
            &mut runtime,
            &mut time_effect,
        )
        .unwrap();
        assert_eq!(time_raw, [1, 3, 0x24, 0, 0x45, 0x10]);
        assert!(time_effect.changed);
        assert_eq!(time_effect.display, b"01:45:10");
        let mut invalid = DateDialog::new(raw, 2, 60, &mut runtime, None).unwrap();
        invalid.replace_masked(b"29/02/2023", &mut runtime).unwrap();
        assert!(!invalid.enabled);
        assert!(!invalid.valid(&mut runtime).unwrap());
        assert!(
            invalid
                .event(CoordinateEvent::Accept, &mut runtime)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            invalid
                .event(CoordinateEvent::Deactivate, &mut runtime)
                .unwrap()
                .unwrap(),
            b"2024/03/03"
        );
        let mut stale_enabled = DateDialog::new(raw, 0, 0, &mut runtime, None).unwrap();
        stale_enabled.editor.control.copy_from_slice(b"2023/02/29");
        assert!(stale_enabled.enabled);
        assert_eq!(
            stale_enabled
                .event(CoordinateEvent::Accept, &mut runtime)
                .unwrap()
                .unwrap(),
            b"2024/03/02"
        );
        let mut cancelled = DateDialog::new(raw, 0, 0, &mut runtime, None).unwrap();
        cancelled
            .event(CoordinateEvent::Cancel, &mut runtime)
            .unwrap();
        assert!(
            cancelled
                .event(CoordinateEvent::Deactivate, &mut runtime)
                .unwrap()
                .is_none()
        );
        let mut direct_escape = DateDialog::new(raw, 0, 0, &mut runtime, None).unwrap();
        direct_escape
            .event(CoordinateEvent::EscapeKeyPress, &mut runtime)
            .unwrap();
        assert!(
            direct_escape
                .event(CoordinateEvent::Deactivate, &mut runtime)
                .unwrap()
                .is_some()
        );
        let mut blank_time = DateDialog::new(raw, 3, 0, &mut runtime, None).unwrap();
        blank_time
            .replace_masked(b"__:__:__", &mut runtime)
            .unwrap();
        assert_eq!(
            blank_time
                .event(CoordinateEvent::Accept, &mut runtime)
                .unwrap()
                .unwrap(),
            b"00:00:00"
        );
        let mut begin = Effect {
            state_out: Some(37),
            ..Effect::default()
        };
        let before = raw;
        input(
            &mut raw,
            0,
            0,
            Input::Begin(Some(b'7')),
            &mut runtime,
            &mut begin,
        )
        .unwrap();
        assert_eq!(raw, before);
        assert_eq!(begin.state_out, Some(1));
        assert!(!begin.changed);
        assert_eq!(begin.dialog.as_ref().unwrap().editor.control[2], b'7');
        for text in [b"2000/01/01".as_slice(), b"2099/12/31"] {
            let mut d = DateDialog::new(raw, 0, 0, &mut runtime, None).unwrap();
            d.replace_masked(text, &mut runtime).unwrap();
            assert!(!d.valid(&mut runtime).unwrap());
        }
        let mut partial = [0x01, 0x01, 0x20, 0xff, 0xff, 0xff];
        assert!(phase2(&mut partial, 0, 0, b"2024/02/29", &mut runtime).is_err());
        assert_eq!(partial, [0x29, 0x02, 0x24, 0xff, 0xff, 0xff]);
        assert_eq!(runtime.locale.short_date, b"dd.mm.yyyy");
        let huge = vec![b'1'; 4097];
        assert!(phase2(&mut raw, 0, 0, &huge, &mut runtime).is_err());
        assert_eq!(
            (runtime.locale.short_date, runtime.locale.date_separator),
            (b"yyyy/mm/dd".as_slice(), b'/')
        );

        use crate::icf::{Icf, context::Context as Q, record::gps_initial};
        let mut memory = vec![0; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
        memory[0x3c385..0x3c5dd].fill(255);
        memory[0x3c5dd..0x3c613].fill(255);
        memory[0x3c385..0x3c387].copy_from_slice(&0_u16.to_le_bytes());
        memory[0x3c5e3..0x3c5e5].copy_from_slice(&0_u16.to_le_bytes());
        memory[0x396ff..0x39701].copy_from_slice(&60_i16.to_le_bytes());
        let image = Icf::from_memory(memory).unwrap();
        let mut q = Q::before_registry();
        q.decode_packed(0x40001);
        let clock = Clock {
            year: 2024,
            month: 3,
            day: 1,
            hour: 0,
            minute: 30,
            second: 40,
            millisecond: 17,
        };
        let holder = gps_initial::holder(&image, &q, None, 0, 7, clock).unwrap();
        assert_eq!(
            (
                holder.kind(),
                holder.index(),
                holder.binding(),
                holder.group()
            ),
            (0x20, 0, 7, 3)
        );
        assert!(holder.valid());
        assert_eq!(&holder.as_bytes()[0x244..0x254], &[b' '; 16]);
        assert_eq!(
            &holder.as_bytes()[0x254..0x25a],
            &[0x29, 2, 0x24, 0x23, 0x30, 0x40]
        );
        assert_eq!(
            (
                *holder.coordinate_bytes(false),
                *holder.coordinate_bytes(true)
            ),
            ([0, 0, 0, b'N'], [0, 0, 0, b'W'])
        );
        let seed = gps_initial::seed(&image, &q, None, clock).unwrap();
        assert_eq!(&seed.bytes[8..14], &[0x29, 2, 0x24, 0x23, 0x30, 0x40]);
        assert_eq!(&seed.bytes[14..18], &i32::MAX.to_le_bytes());
        assert!(gps_initial::seed(&image, &q, None, Clock { year: 0, ..clock }).is_err());
    }
}
