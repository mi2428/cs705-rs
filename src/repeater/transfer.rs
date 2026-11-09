//! Concrete merge/preserve/replace selected/whole CSV roots over typed storage.
//! Native NLS and generated frequency/default profile values are explicit;
//! neither callback parses or supplies an accepted row or an output CSV packet.

use super::{
    groups::{NAMES, REFS},
    parser,
    staging::{self, Commit, PreserveSource, Record, Staging},
    working::{DefaultContext, Working},
};
use crate::{
    csv::{self, control::Counts, payload},
    icf::{
        self,
        record::{self, DecodedRecord},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Whole,
    Group(usize),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    Merge,
    PreserveUse,
    Replace,
}

/// Actual P+4/+8/+C source and reader for mode1's oldCount0 native heap case.
/// The values are not initialized to zero or clamped to the old snapshot.
/// Needed only if mode1 finishes with a nonempty new range. Empty imports,
/// mode0 and mode2 do not ask their callers for native heap contents.
pub struct UnwrittenSource<'a> {
    pub source: PreserveSource,
    pub read_budget: usize,
    pub read: &'a mut dyn FnMut(u32, u32) -> Result<Record, csv::Error>,
}
pub struct ImportOptions<'a, 'c> {
    pub mode: ImportMode,
    pub parser: &'a parser::Options<'c>,
    pub context: DefaultContext,
    pub reset_words: [u16; 2],
    pub unwritten: Option<UnwrittenSource<'a>>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Text(csv::Error),
    Image(icf::Error),
}
impl From<csv::Error> for Failure {
    fn from(error: csv::Error) -> Self {
        Self::Text(error)
    }
}
impl From<icf::Error> for Failure {
    fn from(error: icf::Error) -> Self {
        Self::Image(error)
    }
}
#[derive(Debug)]
pub struct Import {
    pub counts: Counts,
    pub rows_read: usize,
    /// Successful import/flush row saves, excluding shifts and USE restoration.
    pub applied: usize,
    pub group_notifications: Vec<usize>,
    pub failure: Option<Failure>,
}
#[derive(Debug)]
pub struct Export {
    pub bytes: Vec<u8>,
    pub rows: usize,
    pub failure: Option<Failure>,
}

/// Storage-ICF root for the explicit portable-ASCII profile. Real stage8
/// expansion precedes staging; conversion back preserves the caller's metadata
/// and the committed working prefix even when the import report contains Err.
pub fn import_csv(
    image: &mut icf::Icf<'_>,
    scope: Scope,
    source: &[u8],
    options: &mut ImportOptions<'_, '_>,
) -> Import {
    let failure = |error| Import {
        counts: Counts::default(),
        rows_read: 0,
        applied: 0,
        group_notifications: Vec::new(),
        failure: Some(error),
    };
    if let Err(error) = parser::Rows::new(source, options.parser.locale) {
        return failure(Failure::Text(error));
    }
    let mut working = match Working::from_icf(image, options.context) {
        Ok(working) => working,
        Err(error) => return failure(Failure::Image(error)),
    };
    let mut out = import_working_csv(&mut working, scope, source, options, csv::nls::ascii_map);
    let saved = working.to_icf();
    match saved.and_then(|saved| image.patch_memory(&[(0, saved.memory())])) {
        Ok(()) => {}
        Err(error) => {
            if out.failure.is_none() {
                out.failure = Some(Failure::Image(error));
            }
        }
    }
    out
}

pub fn import_working_csv(
    image: &mut Working,
    scope: Scope,
    source: &[u8],
    options: &mut ImportOptions<'_, '_>,
    nls: impl FnMut(&[u8], &mut [u8]) -> Result<i32, csv::Error>,
) -> Import {
    match options.mode {
        ImportMode::Merge => import_csv_merge(
            image,
            scope,
            source,
            options.parser,
            options.reset_words,
            nls,
        ),
        ImportMode::PreserveUse | ImportMode::Replace => {
            replace_csv(image, scope, source, options, nls)
        }
    }
}

fn read_record(
    image: &Working,
    index: usize,
    options: &parser::Options<'_>,
) -> Result<Record, Failure> {
    let mut holder = record::reset_holder(0x0c, index as u32, options.default.dword(8));
    image.load_into(index, &mut holder)?;
    let frequency = u32::from_le_bytes(
        holder.as_bytes()[0x3c..0x40]
            .try_into()
            .expect("frequency width"),
    );
    Ok(Record {
        bytes: Box::new(*holder.as_bytes()),
        frequency_class: (options.frequency)(frequency)?.identity_class,
    })
}

fn replace_csv(
    image: &mut Working,
    scope: Scope,
    source: &[u8],
    options: &mut ImportOptions<'_, '_>,
    mut nls: impl FnMut(&[u8], &mut [u8]) -> Result<i32, csv::Error>,
) -> Import {
    let mut out = Import {
        counts: Counts::default(),
        rows_read: 0,
        applied: 0,
        group_notifications: Vec::new(),
        failure: None,
    };
    let result = (|| -> Result<(), Failure> {
        let parser = options.parser;
        let mut reader = parser::Rows::new(source, parser.locale)?;
        let old = Working::from_working(image.working().to_vec())?;
        let old_groups = old.groups()?;
        let (old_start, old_count) = match scope {
            Scope::Whole => (0, old_groups.total() as usize),
            Scope::Group(g) => (
                old_groups.start(g)? as usize,
                usize::from(old_groups.counts[g]),
            ),
        };
        let old_reference = old_groups.references[0];
        let seed = record::reset_holder(0x0c, 0, parser.default.dword(8));
        let default = DecodedRecord::from_bytes(*parser.default.bytes);
        for _ in 0..old_count {
            let groups = image.groups()?;
            let group = groups.group_of(old_start as i32);
            let relative = old_start - groups.start(group)? as usize;
            image.delete_one(
                group,
                relative,
                &seed,
                |_| Ok(default.clone()),
                |_, which| Ok(options.reset_words[which]),
            )?;
        }
        match scope {
            Scope::Whole => {
                for group in 0..50 {
                    image.rename_group_raw(group, [b' '; 16])?;
                }
            }
            Scope::Group(group) => image.rename_group_raw(group, [b' '; 16])?,
        }
        while let Some(row) = reader.next_with(parser.default.clone(), parser, &mut nls) {
            out.rows_read += 1;
            let row = row?;
            if !row.accepted {
                out.counts.c2 = out.counts.c2.wrapping_add(1);
                continue;
            }
            let group = match scope {
                Scope::Whole => row.record.group() as usize,
                Scope::Group(group) => group,
            };
            let groups = image.groups()?;
            if groups.total() >= 2500 {
                out.counts.capacity_marker(2);
                break;
            }
            let absolute = groups.start(group)? as usize + usize::from(groups.counts[group]);
            if !image.insert_one(
                group,
                usize::from(groups.counts[group]),
                &seed,
                |_| Ok(default.clone()),
                |_, which| Ok(options.reset_words[which]),
            )? {
                return Err(
                    csv::Error::Operation("repeater capacity changed during replacement").into(),
                );
            }
            image.save(absolute, &DecodedRecord::from_bytes(*row.record.bytes))?;
            out.applied += 1;
            out.counts.c3 = out.counts.c3.wrapping_add(row.corrections);
            out.counts.c4 = out.counts.c4.wrapping_add(1);
            if image.groups()?.names[group] == [b' '; 16] {
                image.rename_group_raw(group, row.group_name)?;
            }
        }
        let groups = image.groups()?;
        let (start, count) = match scope {
            Scope::Whole => (0, groups.total() as usize),
            Scope::Group(g) => (groups.start(g)? as usize, usize::from(groups.counts[g])),
        };
        if count == 0 {
            if let Scope::Group(_) = scope {
                out.counts.finish_selected(0);
            }
            return Ok(());
        }
        let mut rows = (start..start + count)
            .map(|index| read_record(image, index, parser))
            .collect::<Result<Vec<_>, _>>()?;
        if options.mode == ImportMode::PreserveUse {
            let mut read_error = None;
            let mut save_error = None;
            let mut save = |ordinal: usize, row: &Record| -> Result<(), csv::Error> {
                let mut holder = DecodedRecord::from_bytes(*row.bytes);
                holder.set_header(0x0c, (start + ordinal) as u32, holder.binding());
                image.save_reload(&mut holder).map_err(|error| {
                    save_error = Some(Failure::Image(error));
                    csv::Error::Operation("preserved USE typed save failed")
                })
            };
            let result = if old_count != 0 {
                staging::restore_use(
                    &mut rows,
                    PreserveSource {
                        raw: 0,
                        first: old_start as u32,
                        last: (old_start + old_count - 1) as u32,
                        unwritten: false,
                    },
                    count * old_count,
                    |_, index| {
                        read_record(&old, index as usize, parser).map_err(|error| {
                            read_error = Some(error);
                            csv::Error::Operation("preserved USE snapshot read failed")
                        })
                    },
                    &mut save,
                )
            } else {
                let source = options
                    .unwritten
                    .as_mut()
                    .ok_or(csv::Error::SourceUnavailable(
                        "mode1 oldCount0 requires actual unwritten three-word source",
                    ))?;
                staging::restore_use(
                    &mut rows,
                    source.source,
                    source.read_budget,
                    &mut source.read,
                    &mut save,
                )
            };
            if let Some(error) = read_error.or(save_error) {
                return Err(error);
            }
            result?;
            // C6050 loads the actual post-81E8 target generation, not the
            // pre-save holder snapshots used during the USE pass.
            rows = (start..start + count)
                .map(|index| read_record(image, index, parser))
                .collect::<Result<Vec<_>, _>>()?;
        }
        let reference_applies = match scope {
            Scope::Whole => true,
            Scope::Group(g) => old_groups.group_of(i32::from(old_reference)) == g,
        };
        if reference_applies {
            let reference = if old_reference == u16::MAX {
                u16::MAX
            } else {
                let old_reference = read_record(&old, usize::from(old_reference), parser)?;
                staging::restored_reference(&rows, start as u32, &old_reference)
            };
            image.patch(REFS[0], &reference.to_le_bytes())?;
        }
        if let Scope::Group(_) = scope {
            out.counts.finish_selected(count as u32);
        }
        Ok(())
    })();
    if let Err(error) = result {
        out.failure = Some(error);
    }
    out
}

/// Stable generated source profile; reset words are the live generation's two
/// explicit source words. General reentrant/aliased G production is not modeled
/// by this immutable-profile entry; Working's lower row APIs expose live reads.
pub fn import_csv_merge(
    image: &mut Working,
    scope: Scope,
    source: &[u8],
    options: &parser::Options<'_>,
    reset: [u16; 2],
    mut nls: impl FnMut(&[u8], &mut [u8]) -> Result<i32, csv::Error>,
) -> Import {
    let mut out = Import {
        counts: Counts::default(),
        rows_read: 0,
        applied: 0,
        group_notifications: Vec::new(),
        failure: None,
    };
    let result = (|| -> Result<(), Failure> {
        let mut reader = parser::Rows::new(source, options.locale)?;
        let groups = image.groups()?;
        if let Scope::Group(g) = scope {
            groups.start(g)?;
        }
        let mut old = Vec::with_capacity(groups.total() as usize);
        let binding = options.default.dword(8);
        for index in 0..groups.total() as usize {
            old.push(read_record(image, index, options)?);
        }
        let mut stage = Staging::preload(old, groups.names, groups.references[0])?;
        let header: [u8; 12] = record::reset_holder(0x0c, 0, binding).as_bytes()[..12]
            .try_into()
            .expect("holder header width");
        while let Some(row) = reader.next_with(options.default.clone(), options, &mut nls) {
            out.rows_read += 1;
            let mut row = row?;
            if !row.accepted {
                stage.counts.c2 = stage.counts.c2.wrapping_add(1);
                out.counts = stage.counts;
                continue;
            }
            match scope {
                Scope::Whole => {
                    stage.whole_row(&mut row.record, row.group_name, header, row.corrections)?
                }
                Scope::Group(group) => {
                    stage.selected_row(
                        &mut row.record,
                        row.group_name,
                        group,
                        header,
                        row.corrections,
                    )?;
                }
            }
            out.counts = stage.counts;
        }
        if scope == Scope::Whole {
            stage.finish_whole();
        }
        out.counts = stage.counts;
        let seed = record::reset_holder(0x0c, 0, binding);
        let default = DecodedRecord::from_bytes(*options.default.bytes);
        let mut apply = |commit: Commit<'_>| -> Result<(), Failure> {
            match commit {
                Commit::Delete { absolute } => {
                    let live = image.groups()?;
                    let group = live.group_of(absolute as i32);
                    let relative = absolute - live.start(group)? as usize;
                    image.delete_one(
                        group,
                        relative,
                        &seed,
                        |_| Ok(default.clone()),
                        |_, which| Ok(reset[which]),
                    )?;
                }
                Commit::Insert {
                    absolute,
                    group,
                    record,
                } => {
                    let live = image.groups()?;
                    let start = live.start(group)? as usize;
                    let absolute = absolute.unwrap_or(start + usize::from(live.counts[group]));
                    if !image.insert_one(
                        group,
                        absolute - start,
                        &seed,
                        |_| Ok(default.clone()),
                        |_, which| Ok(reset[which]),
                    )? {
                        return Err(csv::Error::Operation(
                            "repeater capacity changed during flush",
                        )
                        .into());
                    }
                    image.save(absolute, &DecodedRecord::from_bytes(*record.bytes))?;
                    out.applied += 1;
                }
                Commit::NameByte { group, byte, value } => {
                    image.patch(NAMES + 16 * group + byte, &[value])?
                }
                Commit::Reference(value) => image.patch(REFS[0], &value.to_le_bytes())?,
                Commit::GroupNotification { group } => out.group_notifications.push(group),
            }
            Ok(())
        };
        match scope {
            Scope::Whole => stage.flush_whole(groups.total() as usize, &mut apply)?,
            Scope::Group(g) => {
                stage.flush_selected(
                    g,
                    groups.start(g)? as usize,
                    usize::from(groups.counts[g]),
                    &mut apply,
                )?;
                out.counts
                    .finish_selected(u32::from(image.groups()?.counts[g]));
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        out.failure = Some(error);
    }
    out
}

/// Source-driven17-cell export. Public static labels and all scalar expressions
/// are portable; no caller-supplied per-field formatter is needed for Repeater.
pub fn export_working_csv(image: &Working, scope: Scope, locale: csv::Locale) -> Export {
    let header = csv::split_cells(&super::HEADER[..super::HEADER.len() - 2], b',');
    let header: Vec<_> = header.into_iter().map(<[u8]>::to_vec).collect();
    let mut out = Export {
        bytes: Vec::new(),
        rows: 0,
        failure: None,
    };
    let result = (|| -> Result<(), Failure> {
        out.bytes = csv::join(&header, locale.list, true)?;
        let groups = image.groups()?;
        let (start, count) = match scope {
            Scope::Whole => (0, groups.total() as usize),
            Scope::Group(g) => (groups.start(g)? as usize, usize::from(groups.counts[g])),
        };
        let mut holder = record::reset_holder(0x0c, 0, 0);
        for relative in 0..count {
            let index = start + relative;
            image.load_into(index, &mut holder)?;
            let live = image.groups()?;
            let group = live.group_of(index as i32);
            if group >= 50 {
                return Err(csv::Error::SourceUnavailable(
                    "export group-name source outside working groups",
                )
                .into());
            }
            let context = payload::Context {
                separator: locale.list,
                decimal: locale.decimal,
                row: if scope == Scope::Whole {
                    index as u32
                } else {
                    relative as u32
                },
                number_base: 0,
                group: group as u32,
                group_name: &live.names[group],
                alarm: false,
                utc_offset_minutes: 0,
                memory_group_columns: false,
                memory_selection_column: false,
                memory_pair: false,
            };
            let bytes = payload::build_portable(
                payload::Kind::Repeater,
                payload::Holder::new(holder.as_bytes())?,
                context,
                |_| {
                    Err(csv::Error::SourceUnavailable(
                        "unexpected repeater runtime label",
                    ))
                },
            )?;
            out.bytes.extend_from_slice(&bytes);
            out.rows += 1;
        }
        Ok(())
    })();
    if let Err(error) = result {
        out.failure = Some(error);
    }
    out
}

pub fn export_csv(
    image: &icf::Icf<'_>,
    scope: Scope,
    locale: csv::Locale,
    context: DefaultContext,
) -> Export {
    match Working::from_icf(image, context) {
        Ok(working) => export_working_csv(&working, scope, locale),
        Err(error) => Export {
            bytes: Vec::new(),
            rows: 0,
            failure: Some(Failure::Image(error)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> Working {
        let mut memory = vec![0; 0x4997a];
        memory[0x355a1] = 6;
        memory[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
        memory[NAMES..NAMES + 800].fill(b' ');
        for at in REFS {
            memory[at..at + 2].fill(255);
        }
        Working::from_working(memory).unwrap()
    }
    fn default() -> Record {
        let mut raw = *record::reset_holder(0x22, 0, 0x1234).as_bytes();
        raw[12] &= !1;
        raw[0x25d] = b'N';
        raw[0x261] = b'E';
        let mut row = Record {
            bytes: Box::new(raw),
            frequency_class: 1,
        };
        row.put(0x48, 600000);
        row.put(0x178, 8);
        row
    }
    fn datetime() -> csv::datetime::Context<'static> {
        csv::datetime::Context {
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
        }
    }
    fn options<'a>(
        default: &'a Record,
        time: &'a csv::datetime::Context<'a>,
    ) -> parser::Options<'a> {
        parser::Options {
            locale: csv::Locale::copied(b'.', b';'),
            datetime: time,
            default,
            types: super::super::fields::TypeSource {
                placeholder: *b"PLACEHLD",
                gateway_marker: *b"GATEWAY ",
            },
            group_name_seed: [b' '; 16],
            crt_decimal: b'.',
            frequency: &profile,
        }
    }
    fn profile(_: u32) -> Result<parser::FrequencyProfile, csv::Error> {
        Ok(parser::FrequencyProfile {
            identity_class: 1,
            offset: super::super::finalize::OffsetProfile {
                class: 0,
                default_hz: 600000,
            },
        })
    }
    fn import_options<'a, 'c>(
        parser: &'a parser::Options<'c>,
        mode: ImportMode,
    ) -> ImportOptions<'a, 'c> {
        ImportOptions {
            mode,
            parser,
            context: DefaultContext {
                five_mhz: false,
                hemispheres: *b"NE",
            },
            reset_words: [65535; 2],
            unwritten: None,
        }
    }
    fn old_image() -> Working {
        let mut image = image();
        image
            .patch(super::super::groups::COUNTS, &1_u16.to_le_bytes())
            .unwrap();
        image
            .patch(super::super::groups::COUNTS + 6, &2_u16.to_le_bytes())
            .unwrap();
        for (index, call, used) in [
            (0, b"OUTSIDE ", 1),
            (1, b"SAME   B", 0),
            (2, b"SAME   B", 1),
        ] {
            let mut holder = record::reset_holder(0x0c, index as u32, 0);
            holder.set_valid(true);
            holder.set_channel_field(record::ChannelField::FrequencyHz, 145_000_000);
            holder.set_channel_field(record::ChannelField::Duplex, 1);
            holder.set_repeater_field(record::RepeaterField::Mode, 2);
            holder.set_repeater_field(record::RepeaterField::Use, used);
            holder.set_repeater_text(record::RepeaterText::Call, call);
            image.save(index, &holder).unwrap();
        }
        image.rename_group_raw(0, *b"OUTSIDE         ").unwrap();
        image.rename_group_raw(3, *b"OLD             ").unwrap();
        image.patch(REFS[0], &2_u16.to_le_bytes()).unwrap();
        image
    }
    const REPLACEMENT: &[u8] = b"Group No;Group Name;Repeater Call Sign;Frequency;Dup;Mode;RPT1USE\n3;NEW;SAME   B;145;DUP-;DV;YES\n3;SECOND;SAME   B;433;DUP-;DV;YES\n";
    #[test]
    fn both_replace_modes_restore_first_reference_but_only_mode1_reuses_first_old_use() {
        let default = default();
        let time = datetime();
        let parser = options(&default, &time);
        for scope in [Scope::Whole, Scope::Group(3)] {
            for mode in [ImportMode::PreserveUse, ImportMode::Replace] {
                let mut image = old_image();
                let out = import_working_csv(
                    &mut image,
                    scope,
                    REPLACEMENT,
                    &mut import_options(&parser, mode),
                    csv::nls::ascii_map,
                );
                assert!(out.failure.is_none(), "{:?}", out.failure);
                assert_eq!((out.rows_read, out.counts.c4), (2, 2));
                assert_eq!(out.applied, 2);
                let group = if scope == Scope::Whole { 2 } else { 3 };
                let groups = image.groups().unwrap();
                let start = groups.start(group).unwrap();
                assert_eq!(groups.counts[group], 2);
                assert_eq!(groups.references[0], start as u16);
                assert_eq!(groups.names[group], *b"NEW             ");
                for index in start..start + 2 {
                    let row = read_record(&image, index as usize, &parser).unwrap();
                    assert_eq!(row.dword(0x17c), u32::from(mode == ImportMode::Replace));
                }
                assert_eq!(groups.counts[0], u16::from(scope != Scope::Whole));
                if scope != Scope::Whole {
                    assert_eq!(groups.names[0], *b"OUTSIDE         ");
                }
            }
        }
    }
    #[test]
    fn replacement_header_gate_precedes_clear_and_late_parser_failure_keeps_stored_prefix() {
        let default = default();
        let time = datetime();
        let parser = options(&default, &time);
        let mut image = old_image();
        let before = image.working().to_vec();
        let out = import_working_csv(
            &mut image,
            Scope::Group(3),
            b"Frequency\n145\n",
            &mut import_options(&parser, ImportMode::Replace),
            csv::nls::ascii_map,
        );
        assert!(out.failure.is_some());
        assert_eq!(out.applied, 0);
        assert_eq!(image.working(), before);
        let out=import_working_csv(&mut image,Scope::Group(3),b"Group Name;Repeater Call Sign;Frequency;Mode\nNEW;SAME   B;145;DV\nBROKEN;\xa1;145;DV\n",
            &mut import_options(&parser,ImportMode::PreserveUse),csv::nls::ascii_map);
        assert!(out.failure.is_some());
        assert_eq!((out.rows_read, out.counts.c4), (2, 1));
        assert_eq!(out.applied, 1);
        let groups = image.groups().unwrap();
        assert_eq!(groups.counts[3], 1);
        assert_eq!(groups.names[3], *b"NEW             ");
        assert_eq!(groups.references[0], 65535);
        for scope in [Scope::Whole, Scope::Group(3)] {
            let mut image = old_image();
            let out = import_working_csv(
                &mut image,
                scope,
                b"Repeater Call Sign\n",
                &mut import_options(&parser, ImportMode::PreserveUse),
                csv::nls::ascii_map,
            );
            assert!(out.failure.is_none());
            assert_eq!((out.rows_read, out.counts.c4), (0, 0));
            assert_eq!(out.applied, 0);
            assert_eq!(image.groups().unwrap().counts[3], 0);
            assert_eq!(image.groups().unwrap().names[3], [b' '; 16]);
        }
    }
    #[test]
    fn old_count_zero_requires_explicit_heap_source_and_keeps_late_use_prefix() {
        let default = default();
        let time = datetime();
        let parser = options(&default, &time);
        let mut image = image();
        let out = import_working_csv(
            &mut image,
            Scope::Whole,
            REPLACEMENT,
            &mut import_options(&parser, ImportMode::PreserveUse),
            csv::nls::ascii_map,
        );
        assert!(matches!(
            out.failure,
            Some(Failure::Text(csv::Error::SourceUnavailable(_)))
        ));
        assert_eq!(out.counts.c4, 2);
        assert_eq!(out.applied, 2);
        assert_eq!(image.groups().unwrap().total(), 2);
        let source = read_record(&image, 0, &parser).unwrap();
        let mut old = source.clone();
        old.put(0x17c, 0);
        let mut calls = 0;
        let mut read = |raw, index| {
            assert_eq!((raw, index), (0x12345678, 7));
            calls += 1;
            if calls == 1 {
                Ok(old.clone())
            } else {
                Err(csv::Error::Operation("injected second heap read failure"))
            }
        };
        let mut image = super::tests::image();
        let mut options = import_options(&parser, ImportMode::PreserveUse);
        options.unwritten = Some(UnwrittenSource {
            source: PreserveSource {
                raw: 0x12345678,
                first: 7,
                last: 7,
                unwritten: true,
            },
            read_budget: 2,
            read: &mut read,
        });
        let out = import_working_csv(
            &mut image,
            Scope::Whole,
            REPLACEMENT,
            &mut options,
            csv::nls::ascii_map,
        );
        assert!(out.failure.is_some());
        assert_eq!(out.counts.c4, 2);
        assert_eq!(out.applied, 2);
        assert_eq!(read_record(&image, 0, &parser).unwrap().dword(0x17c), 0);
        assert_eq!(read_record(&image, 1, &parser).unwrap().dword(0x17c), 1);
        let mut options = import_options(&parser, ImportMode::PreserveUse);
        let mut read = |_, _| panic!("empty source range does not read an image");
        options.unwritten = Some(UnwrittenSource {
            source: PreserveSource {
                raw: 0,
                first: 9,
                last: 8,
                unwritten: true,
            },
            read_budget: 0,
            read: &mut read,
        });
        let mut image = super::tests::image();
        let out = import_working_csv(
            &mut image,
            Scope::Whole,
            REPLACEMENT,
            &mut options,
            csv::nls::ascii_map,
        );
        assert!(out.failure.is_none(), "{:?}", out.failure);
    }
    #[test]
    fn empty_old_range_does_not_require_heap_input_outside_the_native_read_branch() {
        let default = default();
        let time = datetime();
        let parser = options(&default, &time);
        for mode in [
            ImportMode::Merge,
            ImportMode::PreserveUse,
            ImportMode::Replace,
        ] {
            for scope in [Scope::Whole, Scope::Group(3)] {
                let mut image = image();
                let out = import_working_csv(
                    &mut image,
                    scope,
                    b"Repeater Call Sign\n",
                    &mut import_options(&parser, mode),
                    csv::nls::ascii_map,
                );
                assert!(out.failure.is_none(), "{:?}", out.failure);
                assert_eq!(
                    (
                        out.rows_read,
                        out.counts.c4,
                        image.groups().unwrap().total()
                    ),
                    (0, 0, 0)
                );
            }
        }
        for mode in [ImportMode::Merge, ImportMode::Replace] {
            let mut image = image();
            let out = import_working_csv(
                &mut image,
                Scope::Whole,
                REPLACEMENT,
                &mut import_options(&parser, mode),
                csv::nls::ascii_map,
            );
            assert!(out.failure.is_none(), "{:?}", out.failure);
            assert!(image.groups().unwrap().total() != 0);
        }
    }
    #[test]
    fn selected_capacity_overwrites_event_count_and_retains_other_groups() {
        let default = default();
        let time = datetime();
        let parser = options(&default, &time);
        let mut image = image();
        image
            .patch(super::super::groups::COUNTS + 2, &2498_u16.to_le_bytes())
            .unwrap();
        let input = [REPLACEMENT, b"3;THIRD;SAME   B;145;DUP-;DV;YES\n"].concat();
        let out = import_working_csv(
            &mut image,
            Scope::Group(0),
            &input,
            &mut import_options(&parser, ImportMode::Replace),
            csv::nls::ascii_map,
        );
        assert!(out.failure.is_none(), "{:?}", out.failure);
        assert_eq!((out.rows_read, out.counts.s, out.counts.c4), (3, 1, 1));
        assert_eq!(out.applied, 2);
        assert_eq!(image.groups().unwrap().counts[..2], [2, 2498]);
    }
    #[test]
    fn storage_replace_modes_preserve_metadata_and_expand_compress_real_rows() {
        let default = default();
        let time = datetime();
        let parser = options(&default, &time);
        let old = old_image();
        let storage = old.to_icf().unwrap();
        let source = icf::format::write_storage(
            storage.memory(),
            icf::format::Layout::from_map_revision(6),
            b"KEEP",
            0x00040001,
        )
        .unwrap();
        for mode in [ImportMode::PreserveUse, ImportMode::Replace] {
            let mut storage = icf::Icf::parse(&source).unwrap();
            let mut options = import_options(&parser, mode);
            let out = import_csv(&mut storage, Scope::Group(3), REPLACEMENT, &mut options);
            assert!(out.failure.is_none(), "{:?}", out.failure);
            assert_eq!(out.applied, 2);
            assert_eq!(storage.comment_bytes(), b"KEEP");
            assert_eq!(storage.extra_data(), 0x00040001);
            let reloaded = Working::from_icf(&storage, options.context).unwrap();
            assert_eq!(reloaded.groups().unwrap().counts[3], 2);
            assert_eq!(reloaded.groups().unwrap().references[0], 1);
            assert_eq!(
                read_record(&reloaded, 1, &parser).unwrap().dword(0x17c),
                u32::from(mode == ImportMode::Replace)
            );
        }
    }
    #[test]
    fn whole_reference_restoration_does_not_add_an_old_total_or_valid_guard() {
        let default = default();
        let time = datetime();
        let parser = options(&default, &time);
        let mut image = image();
        let mut inactive = record::reset_holder(0x0c, 2499, 0);
        inactive.set_repeater_field(record::RepeaterField::Mode, 2);
        inactive.set_repeater_text(record::RepeaterText::Call, b"SAME   B");
        image.save(2499, &inactive).unwrap();
        image.patch(REFS[0], &2499_u16.to_le_bytes()).unwrap();
        let out = import_working_csv(
            &mut image,
            Scope::Whole,
            REPLACEMENT,
            &mut import_options(&parser, ImportMode::Replace),
            csv::nls::ascii_map,
        );
        assert!(out.failure.is_none(), "{:?}", out.failure);
        assert_eq!(image.groups().unwrap().references[0], 0);
    }
    #[test]
    fn csv_bytes_finalize_merge_export_and_save_icf_use_shared_codecs_and_first_gateway() {
        let mut image = image();
        let default = default();
        let time = datetime();
        let profile = |_| {
            Ok(parser::FrequencyProfile {
                identity_class: 1,
                offset: super::super::finalize::OffsetProfile {
                    class: 0,
                    default_hz: 600000,
                },
            })
        };
        let options = parser::Options {
            locale: csv::Locale::copied(b'.', b';'),
            datetime: &time,
            default: &default,
            types: super::super::fields::TypeSource {
                placeholder: *b"PLACEHLD",
                gateway_marker: *b"GATEWAY ",
            },
            group_name_seed: [b' '; 16],
            crt_decimal: b'.',
            frequency: &profile,
        };
        let input=b"Group No;Group Name;Name;Sub Name;Repeater Call Sign;Gateway Call Sign;Frequency;Dup;Offset;Mode;TONE;Repeater Tone;RPT1USE;Position;Latitude;Longitude;UTC Offset\n1;G;FIRST;S;CALLA  B;NETAA  G;145.125;DUP-;0.6;FM-N;TONE;150.0Hz;YES;None;0;0;+9:07\n2;H;SECOND;T;CALLB  B;NETAA  G;433.5;DUP+;5;DV;OFF;88.5Hz;NO;Exact;-0;0;--:--\n";
        let out = import_csv_merge(
            &mut image,
            Scope::Whole,
            input,
            &options,
            [65535; 2],
            csv::nls::ascii_map,
        );
        assert!(out.failure.is_none(), "{:?}", out.failure);
        assert_eq!((out.rows_read, out.counts.c4, out.counts.c3), (2, 2, 2));
        assert_eq!(out.applied, 2);
        assert_eq!(out.group_notifications, (0..50).collect::<Vec<_>>());
        let exported = export_working_csv(&image, Scope::Whole, options.locale);
        assert!(exported.failure.is_none());
        assert_eq!(exported.rows, 2);
        assert!(exported.bytes.ends_with(b"1;G;FIRST;S;CALLA  B;NETAA  G;145.125000;DUP-;0.600000;FM;TONE;151.4Hz;YES;None;0.000000;0.000000;+9:05\r\n2;H;SECOND;T;CALLB  B;NETAA  G;433.500000;DUP+;5.000000;DV;;;NO;Exact;-0.000000;0.000000;0:00\r\n"));
        let saved = image.to_icf().unwrap();
        assert_eq!(&saved.memory()[0x3ab82..0x3ab88], &[0, 0, 1, 0, 255, 255]);
        assert_eq!(&saved.memory()[0xaa0f + 52 + 8..0xaa0f + 52 + 10], &[0, 0]);
        assert_eq!(
            saved.physical_repeater(1).unwrap().unwrap().gateway_call,
            *b"NETAA  G"
        );
        assert_eq!(saved.repeater_groups().unwrap()[1].physical_slots(), &[1]);
        let mut saved = saved;
        let context = DefaultContext {
            five_mhz: false,
            hemispheres: *b"NE",
        };
        let merged=import_csv(&mut saved,Scope::Group(0),
            b"Name;Repeater Call Sign;Frequency;Dup;Mode;RPT1USE\nCHANGED;CALLA  B;145.125;DUP-;FM;NO\n",
            &mut ImportOptions {mode:ImportMode::Merge,parser:&options,context,reset_words:[65535;2],unwritten:None});
        assert!(merged.failure.is_none(), "{:?}", merged.failure);
        assert_eq!((merged.rows_read, merged.counts.c4), (1, 1));
        assert_eq!(merged.applied, 1);
        let first = saved.physical_repeater(0).unwrap().unwrap();
        assert!(first.name.starts_with(b"CHANGED"));
        assert!(first.use_repeater);
        let selected = export_csv(&saved, Scope::Group(1), options.locale, context);
        assert!(selected.failure.is_none());
        assert_eq!(selected.rows, 1);
    }
    #[test]
    fn staged_reader_error_keeps_counter_prefix_but_does_not_begin_image_flush() {
        let mut image = image();
        let default = default();
        let time = datetime();
        let profile = |_| {
            Ok(parser::FrequencyProfile {
                identity_class: 1,
                offset: super::super::finalize::OffsetProfile {
                    class: 0,
                    default_hz: 600000,
                },
            })
        };
        let options = parser::Options {
            locale: csv::Locale::copied(b'.', b';'),
            datetime: &time,
            default: &default,
            types: super::super::fields::TypeSource {
                placeholder: *b"PLACEHLD",
                gateway_marker: *b"GATEWAY ",
            },
            group_name_seed: [b' '; 16],
            crt_decimal: b'.',
            frequency: &profile,
        };
        let before = image.working().to_vec();
        let out = import_csv_merge(
            &mut image,
            Scope::Group(3),
            b"Frequency;Repeater Call Sign\n145;FIRST\n433;\xa1\n",
            &options,
            [65535; 2],
            csv::nls::ascii_map,
        );
        assert!(out.failure.is_some());
        assert_eq!((out.rows_read, out.counts.c4), (2, 1));
        assert_eq!(out.applied, 0);
        assert_eq!(image.working(), before);
    }

    #[test]
    fn merge_applied_counts_retained_rows_saved_by_flush_not_input_or_shifts() {
        let default = default();
        let time = datetime();
        let parser = options(&default, &time);
        let mut image = old_image();
        let out = import_working_csv(
            &mut image,
            Scope::Group(3),
            b"Repeater Call Sign\n",
            &mut import_options(&parser, ImportMode::Merge),
            csv::nls::ascii_map,
        );
        assert!(out.failure.is_none(), "{:?}", out.failure);
        assert_eq!((out.rows_read, out.applied), (0, 2));
        assert_eq!(image.groups().unwrap().counts[..4], [1, 0, 0, 2]);
    }

    #[test]
    fn frequency_source_error_keeps_real_save_prefix_and_never_supplies_zero_profile() {
        let default = default();
        let time = datetime();
        let frequency = |hz| {
            if hz == 433_000_000 {
                Err(csv::Error::SourceUnavailable("injected frequency source"))
            } else {
                profile(hz)
            }
        };
        let mut parser = options(&default, &time);
        parser.frequency = &frequency;
        for mode in [
            ImportMode::Merge,
            ImportMode::PreserveUse,
            ImportMode::Replace,
        ] {
            let mut image = old_image();
            let before = image.working().to_vec();
            let out = import_working_csv(
                &mut image,
                Scope::Group(3),
                REPLACEMENT,
                &mut import_options(&parser, mode),
                csv::nls::ascii_map,
            );
            assert_eq!(
                out.failure,
                Some(Failure::Text(csv::Error::SourceUnavailable(
                    "injected frequency source"
                )))
            );
            assert_eq!(
                (out.rows_read, out.applied),
                (2, usize::from(mode != ImportMode::Merge))
            );
            if mode == ImportMode::Merge {
                assert_eq!(image.working(), before);
            } else {
                assert_eq!(image.groups().unwrap().counts[3], 1);
            }
        }
    }
}
