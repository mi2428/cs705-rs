//! GPS 8D288/8D534 byte-to-value phase. Draft is deliberately NOT an accepted
//! Parsed row: 83FDC/BAA10/BAED0/BAF74 finalization must run before typed apply.
//! Missing columns and present empty cells stay distinct. The supplied type 21
//! direction/group-name seeds are explicit caller state, not host/default data.

use super::{operations::Record, transfer::Parsed};
use crate::csv::{self, Error, datetime, dispatch::Binding};

/// Copied CSV bytes, live RTL date/time context and caller scratch sources.
#[derive(Debug, Clone)]
pub struct Options<'a> {
    pub locale: csv::Locale,
    pub datetime: datetime::Context<'a>,
    pub hemispheres: [u8; 2],
    pub group_name_seed: [u8; 16],
}
/// All source-level output values before final correction/compressed counts.
/// `record.valid` remains false until the real finalizer makes its decision.
#[derive(Debug, Clone)]
pub struct Draft {
    pub record: Record,
    pub group_code: u32,
    pub group_name: [u8; 16],
    pub text_corrections: u32,
    pub alarm_code: Option<u32>,
    /// Invalid altitude conversion is not hidden by an accepted Unknown value.
    pub altitude_error: Option<Error>,
}

/// BAED0's degree==0 bug is intentional: only direction and BE16 fraction
/// constrain these four bytes. Do not add geographic degree/pole checks here.
pub fn valid_coordinate(raw: [u8; 4], latitude: bool) -> bool {
    (if latitude {
        matches!(raw[3], b'N' | b'S')
    } else {
        matches!(raw[3], b'E' | b'W')
    }) && u16::from_be_bytes([raw[1], raw[2]]) <= 59999
}
impl Draft {
    /// 8D534 alarm pre-count then 83FDC, retaining scratch changes on failure.
    /// The output C is C+bool(local_C), not simply bool(any correction).
    pub fn finalize(
        &mut self,
        whole: bool,
        offset: i16,
        alarm_seen: &mut bool,
    ) -> Result<Parsed, Error> {
        if let Some(error) = &self.altitude_error {
            return Err(error.clone());
        }
        let mut base = self.text_corrections;
        let alarm = self.alarm_code == Some(1) && !*alarm_seen;
        if self.alarm_code.is_some_and(|code| code != 0) && !alarm {
            base = base.wrapping_add(1);
        }
        let mut local = base;
        let raw = &mut self.record.bytes;
        let fatal = !valid_coordinate(raw[..4].try_into().expect("GPS latitude width"), true)
            || !valid_coordinate(raw[4..8].try_into().expect("GPS longitude width"), false);
        let altitude = i32::from_le_bytes(raw[14..18].try_into().expect("GPS altitude width"));
        if altitude != i32::MAX && altitude.wrapping_abs() > 328084 {
            raw[14..18].copy_from_slice(&i32::MAX.to_le_bytes());
            local = local.wrapping_add(1);
        }
        local = local.wrapping_add(u32::from(csv::nls::sanitize(&mut raw[18..], b' ')));
        let mut date: [u8; 6] = raw[8..14].try_into().expect("GPS datetime width");
        let result = datetime::finalize_gps_raw(&mut date, offset);
        raw[8..14].copy_from_slice(&date);
        local = local.wrapping_add(u32::from(result?));
        if self.group_code >= 27 {
            self.group_code = 0;
            if whole {
                local = local.wrapping_add(1);
            }
        }
        self.record.valid = !fatal;
        if !fatal && alarm {
            *alarm_seen = true;
        }
        Ok(Parsed {
            record: self.record,
            group: self.group_code as usize,
            group_name: self.group_name,
            corrections: base.wrapping_add(u32::from(!fatal && local != 0)),
            alarm: !fatal && alarm,
        })
    }
}

/// Lazy parser: caller may clear the target image before advancing the iterator.
pub struct Rows<'a, 'c> {
    reader: csv::Reader<'a>,
    binding: Binding,
    locale: csv::Locale,
    datetime: DateContext<'c>,
    hemispheres: [u8; 2],
    group_name_seed: [u8; 16],
    next: usize,
}

enum DateContext<'a> {
    Borrowed(datetime::Context<'a>),
    Owned(datetime::OwnedContext),
}

impl DateContext<'_> {
    fn with_context<T>(&self, call: impl FnOnce(&datetime::Context<'_>) -> T) -> T {
        match self {
            Self::Borrowed(context) => call(context),
            Self::Owned(context) => context.with_context(call),
        }
    }
    fn into_owned(self) -> DateContext<'static> {
        DateContext::Owned(match self {
            Self::Borrowed(context) => context.into_owned(),
            Self::Owned(context) => context,
        })
    }
}

impl<'a, 'c> Rows<'a, 'c> {
    /// Retain the cursor, header binding and date context without advancing rows.
    pub fn into_owned(self) -> Rows<'static, 'static> {
        Rows {
            reader: self.reader.into_owned(),
            binding: self.binding,
            locale: self.locale,
            datetime: self.datetime.into_owned(),
            hemispheres: self.hemispheres,
            group_name_seed: self.group_name_seed,
            next: self.next,
        }
    }

    /// Explicit ASCII CharLower context for the accepted built-in header labels.
    pub fn new(source: &'a [u8], options: Options<'c>) -> Result<Self, Error> {
        Self::with_lower(source, options, |bytes| {
            Ok(bytes.iter().map(u8::to_ascii_lowercase).collect())
        })
    }
    pub fn with_lower(
        source: &'a [u8],
        options: Options<'c>,
        lower: impl FnMut(&[u8]) -> Result<Vec<u8>, Error>,
    ) -> Result<Self, Error> {
        let reader = csv::Reader::new(source, options.locale)?;
        let labels: [&[u8]; 10] = [
            b"Group",
            b"Group Name",
            b"Name",
            b"Date",
            b"Time",
            b"Latitude",
            b"Longitude",
            b"Altitude",
            b"Alarm",
            b"Bank",
        ];
        let binding = Binding::new(
            reader.header(),
            options.locale.list,
            &labels,
            &[5, 6],
            lower,
        )?;
        if !binding.accepted {
            return Err(Error::Invalid(
                "GPS header requires Latitude and Longitude".into(),
            ));
        }
        Ok(Self {
            reader,
            binding,
            locale: options.locale,
            datetime: DateContext::Borrowed(options.datetime),
            hemispheres: options.hemispheres,
            group_name_seed: options.group_name_seed,
            next: 0,
        })
    }
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
    fn parse(&self, line: &[u8]) -> Result<Draft, Error> {
        let mut raw = [0; 34];
        raw[3] = self.hemispheres[0];
        raw[7] = self.hemispheres[1];
        raw[8..14].fill(0x99);
        raw[14..18].copy_from_slice(&i32::MAX.to_le_bytes());
        raw[18..].fill(b' ');
        let group = self.binding.cell(line, 0)?;
        let group_code = match group.or(self.binding.cell(line, 9)?) {
            Some(text) => csv::table::gps_group_ascii(text),
            None => u32::MAX,
        };
        let mut group_name = self.group_name_seed;
        let mut corrections = 0_u32;
        if let Some(text) = self.binding.cell(line, 1)? {
            let (name, changed) = csv::fixed_text::<16>(text);
            group_name = name;
            corrections = corrections.wrapping_add(u32::from(changed));
        }
        if let Some(text) = self.binding.cell(line, 2)? {
            let (name, changed) = csv::fixed_text::<16>(text);
            raw[18..].copy_from_slice(&name);
            corrections = corrections.wrapping_add(u32::from(changed));
        }
        let mut time = [0x99; 6];
        let date_cell = self.binding.cell(line, 3)?;
        let time_cell = self.binding.cell(line, 4)?;
        self.datetime
            .with_context(|context| datetime::import_raw(&mut time, date_cell, time_cell, context));
        raw[8..14].copy_from_slice(&time);
        if let Some(text) = self.binding.cell(line, 5)? {
            raw[..4].copy_from_slice(&csv::number::coordinate(text, true, self.locale.decimal));
        }
        if let Some(text) = self.binding.cell(line, 6)? {
            raw[4..8].copy_from_slice(&csv::number::coordinate(text, false, self.locale.decimal));
        }
        let mut altitude_error = None;
        if let Some(text) = self.binding.cell(line, 7)? {
            match csv::number::altitude(text, self.locale.decimal) {
                Ok(value) => raw[14..18].copy_from_slice(&value.to_le_bytes()),
                Err(error) => altitude_error = Some(error),
            }
        }
        let alarm_code = self
            .binding
            .cell(line, 8)?
            .map(csv::trim)
            .filter(|text| !text.is_empty())
            .map(|text| {
                if text.eq_ignore_ascii_case(b"off") {
                    0
                } else if text.eq_ignore_ascii_case(b"on") {
                    1
                } else {
                    0xffff
                }
            });
        Ok(Draft {
            record: Record {
                valid: false,
                bytes: raw,
            },
            group_code,
            group_name,
            text_corrections: corrections,
            alarm_code,
            altitude_error,
        })
    }
}
impl<'a, 'c> Iterator for Rows<'a, 'c> {
    type Item = Result<Draft, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        let line = self.reader.row(self.next)?;
        self.next += 1;
        Some(self.parse(line))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_keeps_cursor_context_and_late_finalizer_error() {
        let source = b"Latitude;Longitude;Altitude\n1;2;0\n3;4;bad\n".to_vec();
        let pattern = b"yyyy/mm/dd".to_vec();
        let options = Options {
            datetime: datetime::Context {
                short_date: &pattern,
                ..options().datetime
            },
            ..options()
        };
        let mut rows = Rows::new(&source, options).unwrap();
        assert_eq!(rows.next().unwrap().unwrap().record.bytes[0], 1);
        let mut owned = rows.into_owned();
        drop(source);
        drop(pattern);
        let mut second = owned.next().unwrap().unwrap();
        assert_eq!(second.record.bytes[0], 3);
        assert!(second.altitude_error.is_some());
        assert!(second.finalize(true, 0, &mut false).is_err());
        assert!(owned.next().is_none());
    }
    #[test]
    fn gps_finalizer_keeps_degree_bug_int_min_and_count_compression() {
        for degree in 0..=255 {
            assert!(valid_coordinate([degree, 0xea, 0x5f, b'N'], true));
        }
        assert!(!valid_coordinate([0, 0xea, 0x60, b'N'], true));
        let mut draft = Rows::new(
            b"Name;Latitude;Longitude\n12345678901234567;0;0\n",
            options(),
        )
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
        draft.record.bytes[14..18].copy_from_slice(&i32::MIN.to_le_bytes());
        let row = draft.finalize(true, 0, &mut false).unwrap();
        assert!(row.record.valid);
        assert_eq!(row.group, 0);
        assert_eq!(row.corrections, 2);
        assert_eq!(&row.record.bytes[14..18], &i32::MIN.to_le_bytes());
        let mut draft = Rows::new(b"Latitude;Longitude;Alarm\n;0;ON\n", options())
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        let mut seen = false;
        let row = draft.finalize(true, 0, &mut seen).unwrap();
        assert!(!row.record.valid);
        assert!(!seen);
        assert!(!row.alarm);
    }
    fn options() -> Options<'static> {
        Options {
            locale: csv::Locale::copied(b',', b','),
            datetime: datetime::Context {
                short_date: b"yyyy/mm/dd",
                date_separator: b'/',
                time_separator: b':',
                decimal_separator: b'.',
                am: b"AM",
                pm: b"PM",
                current_year: 2026,
                two_digit_window: 50,
                primary_language: 0x11,
                eras: &[],
            },
            hemispheres: *b"SW",
            group_name_seed: [0xa7; 16],
        }
    }
    #[test]
    fn gps_value_phase_keeps_group_precedence_empty_date_bug_and_unaccepted_draft() {
        let input=b"Bank;Group;Name;Date;Latitude;Longitude;Altitude;Alarm\nA;;12345678901234567;;-0;139,5;-1,0;ON\n";
        let draft = Rows::new(input, options())
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(draft.group_code, 0xffff);
        assert_eq!(draft.group_name, [0xa7; 16]);
        assert_eq!(&draft.record.bytes[18..], b"1234567890123456");
        assert_eq!(draft.text_corrections, 1);
        assert!(!draft.record.valid);
        assert_eq!(&draft.record.bytes[..4], &[0, 0, 0, b'S']);
        assert_eq!(&draft.record.bytes[4..8], &[139, 0x75, 0x30, b'E']);
        assert_eq!(&draft.record.bytes[8..14], &[0x99; 6]);
        assert_eq!(
            i32::from_le_bytes(draft.record.bytes[14..18].try_into().unwrap()),
            -30
        );
        assert_eq!(draft.alarm_code, Some(1));
        let short = Rows::new(b"Latitude;Longitude;Time\n\n ;0;0:1:2\n", options())
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(&short.record.bytes[..4], &[0xff; 4]);
        assert_eq!(&short.record.bytes[11..14], &[0, 1, 2]);
    }
    #[test]
    fn missing_group_differs_from_invalid_and_altitude_error_is_not_silent_unknown() {
        let draft = Rows::new(b"Latitude;Longitude;Altitude\r0;0;bad\r", options())
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(draft.group_code, u32::MAX);
        assert!(draft.altitude_error.is_some());
        assert!(!draft.record.valid);
        assert!(Rows::new(b"\xef\xbb\xbfLatitude;Longitude\n0;0\n", options()).is_err());
    }
}
