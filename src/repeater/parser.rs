//! Repeater byte CSV value phase followed by84A58. The required header is only
//! Repeater Call Sign. Missing columns preserve type22 fields; empty present cells do not.

use super::{
    fields::TypeSource,
    finalize::{self, OffsetProfile},
    staging::Record,
};
use crate::csv::{self, Error, datetime, dispatch::Binding, table::Entry};

#[derive(Clone, Copy)]
pub struct FrequencyProfile {
    pub identity_class: u32,
    pub offset: OffsetProfile,
}
pub struct Options<'a> {
    pub locale: csv::Locale,
    pub datetime: &'a datetime::Context<'a>,
    pub default: &'a Record,
    pub types: TypeSource,
    pub group_name_seed: [u8; 16],
    pub crt_decimal: u8,
    /// F094 and DB98/EDF0/F20C scalar outputs, not a row/parser provider.
    pub frequency: &'a dyn Fn(u32) -> Result<FrequencyProfile, Error>,
}
pub struct Parsed {
    pub record: Record,
    pub group_name: [u8; 16],
    pub corrections: u32,
    pub accepted: bool,
}
pub struct Rows<'a> {
    reader: csv::Reader<'a>,
    binding: Binding,
    next: usize,
}
impl<'a> Rows<'a> {
    pub fn new(source: &'a [u8], locale: csv::Locale) -> Result<Self, Error> {
        let reader = csv::Reader::new(source, locale)?;
        let labels = csv::split_cells(&super::HEADER[..super::HEADER.len() - 2], b',');
        let binding = Binding::builtin(
            reader.header(),
            locale.list,
            &labels,
            csv::dispatch::ImportKind::Repeater,
        )?;
        if !binding.accepted {
            return Err(Error::Invalid(
                "repeater header requires Repeater Call Sign",
            ));
        }
        Ok(Self {
            reader,
            binding,
            next: 0,
        })
    }
    /// The supplied record is this row's actual type22 load, not zero-filled
    /// scratch. NLS is requested only for a present callsign column.
    pub fn next_with(
        &mut self,
        mut row: Record,
        options: &Options<'_>,
        mut nls: impl FnMut(&[u8], &mut [u8]) -> Result<i32, Error>,
    ) -> Option<Result<Parsed, Error>> {
        let line = *self.reader.rows().get(self.next)?;
        self.next += 1;
        Some((|| {
            row.bytes[12] &= !1;
            let mut corrections = 0_u32;
            let mut group_name = options.group_name_seed;
            if let Some(text) = self.binding.cell(line, 4)? {
                let mut shared = text.to_vec();
                let (mut call, clipped) = csv::nls::callsign_fixed(&mut shared, &mut nls)?;
                corrections = corrections.wrapping_add(u32::from(clipped));
                corrections =
                    corrections.wrapping_add(u32::from(csv::nls::fold_callsign(&mut call)));
                row.bytes[0x160..0x168].copy_from_slice(&call);
                row.bytes[12] |= 1;
            }
            let group = if let Some(text) = self.binding.cell(line, 0)? {
                let parsed = match csv::number::decimal(csv::trim(text), options.locale.decimal) {
                    Ok(n) if n >= i64::MIN as f64 && n < (i64::MAX as f64) => {
                        n.trunc() as i64 as u32
                    }
                    Ok(_) => {
                        return Err(Error::SourceUnavailable(
                            "repeater group signed64 conversion overflow",
                        ));
                    }
                    Err(Error::Conversion(_)) => u32::MAX,
                    Err(error) => return Err(error),
                };
                (if parsed == 0 { 20 } else { parsed }).wrapping_sub(1)
            } else {
                u32::MAX
            };
            row.put(0x20, group);
            if let Some(text) = self.binding.cell(line, 1)? {
                let (name, changed) = csv::fixed_text::<16>(text);
                group_name = name;
                corrections = corrections.wrapping_add(u32::from(changed));
            }
            for (column, at, width) in [(2, 0x26, 16), (3, 0x154, 8)] {
                if let Some(text) = self.binding.cell(line, column)? {
                    if width == 16 {
                        let (name, changed) = csv::fixed_text::<16>(text);
                        row.bytes[at..at + 16].copy_from_slice(&name);
                        corrections = corrections.wrapping_add(u32::from(changed));
                    } else {
                        let (name, changed) = csv::fixed_text::<8>(text);
                        row.bytes[at..at + 8].copy_from_slice(&name);
                        corrections = corrections.wrapping_add(u32::from(changed));
                    }
                }
            }
            if let Some(text) = self.binding.cell(line, 5)? {
                let mut shared = text.to_vec();
                let (call, clipped) = csv::nls::callsign_fixed(&mut shared, &mut nls)?;
                row.bytes[0x168..0x170].copy_from_slice(&call);
                corrections = corrections.wrapping_add(u32::from(clipped));
            }
            if let Some(text) = self.binding.cell(line, 6)? {
                let mut hz = csv::number::frequency_hz(text, options.locale.decimal)?;
                if !(30000..=1399999999).contains(&hz) {
                    hz = 0;
                    corrections = corrections.wrapping_add(1);
                }
                row.put(0x3c, hz);
            }
            if let Some(text) = self.binding.cell(line, 8)? {
                row.put(0x48, csv::number::offset_hz(text, options.locale.decimal)?);
            }
            for (column, at, labels) in [
                (
                    7,
                    0x44,
                    &[
                        Entry {
                            label: b"OFF",
                            value: 0,
                        },
                        Entry {
                            label: b"DUP-",
                            value: 1,
                        },
                        Entry {
                            label: b"DUP+",
                            value: 2,
                        },
                        Entry {
                            label: b"RPS",
                            value: 3,
                        },
                    ][..],
                ),
                (
                    9,
                    0x15c,
                    &[
                        Entry {
                            label: b"DV",
                            value: 2,
                        },
                        Entry {
                            label: b"FM",
                            value: 0,
                        },
                        Entry {
                            label: b"FM-N",
                            value: 1,
                        },
                        Entry {
                            label: b"D-STAR",
                            value: 2,
                        },
                        Entry {
                            label: b"DD",
                            value: 3,
                        },
                    ][..],
                ),
                (
                    10,
                    0x174,
                    &[
                        Entry {
                            label: b"OFF",
                            value: 0,
                        },
                        Entry {
                            label: b"TONE",
                            value: 1,
                        },
                        Entry {
                            label: b"TSQL",
                            value: 2,
                        },
                    ][..],
                ),
                (
                    12,
                    0x17c,
                    &[
                        Entry {
                            label: b"NO",
                            value: 0,
                        },
                        Entry {
                            label: b"YES",
                            value: 1,
                        },
                    ][..],
                ),
                (
                    13,
                    0x180,
                    &[
                        Entry {
                            label: b"None",
                            value: 0,
                        },
                        Entry {
                            label: b"Approximate",
                            value: 1,
                        },
                        Entry {
                            label: b"Exact",
                            value: 2,
                        },
                    ][..],
                ),
            ] {
                if let Some(text) = self.binding.cell(line, column)? {
                    row.put(at, csv::table::import_ascii(text, labels).unwrap_or(0xffff));
                }
            }
            if let Some(text) = self.binding.cell(line, 11)? {
                row.put(
                    0x178,
                    csv::table::tone_builtin_ascii(
                        text,
                        false,
                        options.crt_decimal,
                        &mut corrections,
                    )?,
                );
            }
            for (column, at, latitude) in [(14, 0x25a, true), (15, 0x25e, false)] {
                if let Some(text) = self.binding.cell(line, column)? {
                    row.bytes[at..at + 4].copy_from_slice(&csv::number::coordinate(
                        text,
                        latitude,
                        options.locale.decimal,
                    ));
                }
            }
            if let Some(text) = self.binding.cell(line, 16)? {
                row.put(0x184, utc(text, options.datetime));
            }
            let profile = (options.frequency)(row.dword(0x3c))?;
            row.frequency_class = profile.identity_class;
            let outcome = finalize::finalize(
                &mut row,
                corrections,
                options.default,
                profile.offset,
                options.types,
            );
            Ok(Parsed {
                record: row,
                group_name,
                corrections: outcome.corrections,
                accepted: outcome.accepted,
            })
        })())
    }
}

fn utc(source: &[u8], context: &datetime::Context<'_>) -> u32 {
    let source = csv::trim(source);
    if source.len() < 3 {
        return 0xffff;
    }
    if source == b"--:--" {
        return 0x7fff;
    }
    let (negative, text) = match source.first() {
        Some(b'-') => (true, &source[1..]),
        Some(b'+') => (false, &source[1..]),
        _ => (false, source),
    };
    match datetime::parse_time(text, context) {
        Ok(time) => {
            let value = i32::from(time.hour) * 60 + i32::from(time.minute);
            (if negative { -value } else { value }) as u32
        }
        Err(_) => 0xffff,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn header_gate_is_cache14_callsign_not_frequency_or_export_column_ordinal() {
        let locale = csv::Locale::copied(b'.', b';');
        assert!(Rows::new(b"Frequency\n145\n", locale).is_err());
        assert!(Rows::new(b"Repeater Call Sign\nCALL\n", locale).is_ok());
        assert!(Rows::new(b"\xef\xbb\xbfRepeater Call Sign\nCALL\n", locale).is_err());
    }
}
