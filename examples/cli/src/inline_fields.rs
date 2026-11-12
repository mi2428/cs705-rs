//! Non-channel native inline commits. The library owns profiles and savers.

use crate::{AppError, Result, records::Runtime};
use cs705::{
    csv,
    icf::{
        self, Icf, MemoryCallsign, record,
        settings::{self, digital, extended as e, gps},
    },
    printing::native::{Class, InlineFrame},
};

fn native_error(error: impl std::fmt::Display) -> AppError {
    AppError::invalid(error.to_string())
}
fn text_effect(effect: e::TextEffect) -> (bool, u32) {
    (effect.stored, effect.notify_flags)
}

pub fn edit_non_channel(
    image: &mut Icf<'_>,
    frame: &InlineFrame,
    runtime: &Runtime,
) -> Result<(bool, u32)> {
    if frame.input_code != 13
        || frame.phase != 1
        || frame.action != 0
        || frame.command != 0
        || !frame.grid4
        || !frame.grid8
    {
        return Err(AppError::invalid(
            "non-channel inline requires its generated phase1 text frame",
        ));
    }
    if !frame.text.is_ascii() {
        return Err(AppError::invalid(
            "native inline text requires the explicit ASCII domain",
        ));
    }
    let index = usize::try_from(frame.node.index)
        .map_err(|_| AppError::invalid("negative native inline index"))?;
    let key = u16::try_from(frame.node.key)
        .map_err(|_| AppError::invalid("native inline key exceeds WORD"))?;
    let mut source = frame.text.clone();
    match frame.factory.class {
        Class::Split | Class::Simplex | Class::Satellite | Class::RxHistory | Class::Preset => {
            return Err(AppError::invalid(
                "native dataset has no ordinary non-channel inline text entry",
            ));
        }
        Class::Repeater => return repeater(image, frame, runtime),
        Class::Ur => return ur(image, frame, runtime),
        Class::Gps => return gps_record(image, frame, runtime),
        Class::Groups => {
            if key != 0x101 {
                return Err(AppError::invalid("memory-group inline column is read-only"));
            }
            let mut working = icf::rows::NormalMemory::from_icf(image)?;
            let accepted = working.edit_group_name(index, Some(&mut source))?;
            image.patch_memory(&[(0, working.to_icf()?.memory())])?;
            if accepted {
                runtime.inline_notification(0x3e, false)?;
            }
            return Ok((accepted, 0));
        }
        Class::Allowed => {
            if key != 0x1ae {
                return Err(AppError::invalid("allowed callsign column is read-only"));
            }
            let mut effect = digital::AllowedEffect::default();
            let result = digital::edit_allowed(
                image,
                index as u32,
                frame.input_code as i32,
                e::FixedTextInput::Text {
                    source: &mut source,
                    phase: frame.phase,
                },
                runtime.display_input_options().auto_correct,
                &mut effect,
            );
            for &event in &effect.notifications {
                runtime.inline_notification(event, true)?;
            }
            result.map_err(native_error)?;
            return Ok((effect.stored, effect.notify_flags));
        }
        _ => {}
    }
    if matches!(key, 0x119 | 0x11a | 0x2bd | 0x2be) {
        return runtime.with_captured_ranges(|ranges| {
            let scan = matches!(key, 0x119 | 0x11a);
            let q = runtime.display_context()?;
            let input = ranges.frequency_text_input(
                if scan { 8 } else { 0x15 },
                &source,
                runtime.inline_decimal()?,
            )?;
            let side = if matches!(key, 0x119 | 0x2bd) {
                icf::special::EdgeSide::Low
            } else {
                icf::special::EdgeSide::High
            };
            let effect = if scan {
                let reset = icf::special::scan_reset_enabled(image.memory(), 0)?;
                // Missing G remains unbound until the actual late reset read.
                icf::special::edit_scan_frequency(
                    image,
                    runtime.generation().unwrap_or(&[]),
                    index,
                    side,
                    input,
                    reset,
                    |frequency| Ok(ranges.eb64(q, frequency)),
                )?
            } else {
                icf::special::edit_user_frequency(
                    image,
                    index,
                    side,
                    input,
                    |frequency| Ok(ranges.f32c(q, frequency)),
                    |frequency| Ok(ranges.ed70(frequency)),
                )?
            };
            Ok((effect.writes != 0, effect.callback_flags))
        });
    }
    if key == 0x118 {
        let name = csv::nls::name_candidate(&mut source, 16).map_err(native_error)?;
        let effect = icf::special::edit_scan_field(
            image,
            index,
            icf::special::ScanField::Name,
            None,
            Some(&name),
        )?;
        return Ok((effect.writes != 0, effect.callback_flags));
    }
    if key == 0x2a7 {
        let current = image.memory().to_vec();
        let effect = e::edit_program_scan(
            image,
            &current,
            runtime.generation().unwrap_or(&[]),
            e::ProgramScanInput::Text(&mut source),
        )?;
        return Ok((effect.stored, effect.notify_flags));
    }
    if key == 0x1b3 || key == 0x141 {
        if key == 0x1b3 {
            let name = csv::nls::name_candidate(&mut source, 16).map_err(native_error)?;
            let mut fixed = [b' '; 16];
            fixed[..name.len()].copy_from_slice(&name);
            cs705::gps::operations::rename_group_raw(image, index, fixed)?;
            runtime.inline_notification(0x3e, false)?;
            return Ok((true, 0));
        }
        let mut effect = cs705::repeater::editor::Effect::default();
        cs705::repeater::editor::group_name(image, index, e::TextInput::Text(&source), &mut effect)
            .map_err(native_error)?;
        for &(event, synchronous) in &effect.notifications {
            runtime.inline_notification(event, synchronous)?;
        }
        return Ok((effect.native.save_returned, effect.native.flags));
    }
    if matches!(key, 0x377 | 0x378 | 0x37a | 0x37b | 0x37d | 0x383) {
        return runtime.with_captured_ranges(|ranges| {
            let q = runtime.display_context()?;
            let frequency = e::FrequencyRuntime {
                ranges,
                live_q: q,
                binding: 1,
                crt_point: if key == 0x383 {
                    b'.'
                } else {
                    runtime.inline_decimal()?
                },
            };
            let input = e::NumberInput::Text(&source);
            if key == 0x383 {
                let working = cs705::repeater::working::Working::from_icf(
                    image,
                    cs705::repeater::working::DefaultContext::from_context(
                        q,
                        runtime.display_system_langid(),
                    )?,
                )?;
                let effect = e::edit_home_dr_memory(
                    image,
                    e::HomeDrRuntime {
                        frequency,
                        source: working.working(),
                        current: working.working(),
                    },
                    e::HomeDrMemoryInput::Text(&mut source),
                    u32::from(frame.warning_bits),
                )?;
                return Ok((effect.stored, effect.notify_flags));
            }
            if key == 0x37d {
                let effect = e::edit_home_vfo(
                    image,
                    ranges,
                    q,
                    input,
                    runtime.inline_decimal()?,
                    u32::from(frame.warning_bits),
                )?;
                return Ok((effect.number.stored, effect.number.notify_flags));
            }
            let side = if matches!(key, 0x377 | 0x37a) {
                e::EdgeSide::Low
            } else {
                e::EdgeSide::High
            };
            if matches!(key, 0x377 | 0x378) {
                let effect = e::edit_fixed_edge(image, frequency, index, side, input)?;
                Ok((effect.stored, effect.notify_flags))
            } else {
                let effect = e::edit_auto_repeater_edge(
                    image,
                    e::AutoRepeaterRuntime {
                        frequency,
                        system_langid: runtime.display_system_langid(),
                    },
                    index,
                    side,
                    input,
                    e::AutoRepeaterInvocation {
                        code: frame.action,
                        argument: frame.command,
                        warning_bits: frame.warning_bits,
                    },
                )?;
                Ok((effect.stored, effect.notify_flags))
            }
        });
    }
    if let Some(field) = digital::TextField::ALL
        .into_iter()
        .find(|field| field.key() == key)
    {
        let alignment = if field == digital::TextField::MyCall {
            runtime.display_input_options().auto_correct
        } else {
            0
        };
        return field
            .edit(
                image,
                index,
                e::FixedTextInput::Text {
                    source: &mut source,
                    phase: frame.phase,
                },
                alignment,
            )
            .map(text_effect)
            .map_err(native_error);
    }
    if let Some(field) = e::FixedTextField::ALL
        .into_iter()
        .find(|field| field.key() == key)
    {
        let alignment = if matches!(
            field,
            e::FixedTextField::GatewayCallsign | e::FixedTextField::PictureRecipient
        ) {
            runtime.display_input_options().auto_correct
        } else {
            0
        };
        return e::edit_fixed_text(
            image,
            field,
            index,
            e::FixedTextInput::Text {
                source: &mut source,
                phase: frame.phase,
            },
            alignment,
        )
        .map(|effect| text_effect(effect.text))
        .map_err(native_error);
    }
    if let Some(field) = gps::text_fields().iter().find(|field| {
        key.checked_sub(field.key)
            .is_some_and(|offset| usize::from(offset) < field.count)
    }) {
        let index = if field.count > 1 {
            usize::from(key - field.key)
        } else {
            index
        };
        return field
            .edit(
                image,
                index,
                e::FixedTextInput::Text {
                    source: &mut source,
                    phase: frame.phase,
                },
            )
            .map(text_effect)
            .map_err(native_error);
    }
    if let Some(field) = gps::symbol_fields().iter().find(|field| {
        key.checked_sub(field.key)
            .is_some_and(|offset| usize::from(offset) < 2 * field.count)
    }) {
        let offset = key - field.key;
        if offset % 2 == 0 {
            return Err(AppError::invalid(
                "GPS symbol menu cell has no phase1 text commit producer",
            ));
        }
        let index = usize::from(offset / 2);
        return field
            .edit(
                image,
                index,
                e::FixedTextInput::Text {
                    source: &mut source,
                    phase: frame.phase,
                },
            )
            .map(text_effect)
            .map_err(native_error);
    }
    if let Some(field) = gps::coordinate_fields()
        .iter()
        .chain(std::iter::once(&gps::ALARM_AREA))
        .find(|field| field.key == key)
    {
        let mut dialog = field
            .dialog(image, image.memory(), None)
            .map_err(native_error)?;
        dialog.replace(&source);
        let current = image.memory().to_vec();
        let effect = field
            .apply_dialog(
                image,
                &mut dialog,
                csv::number::CoordinateEvent::Accept,
                &current,
            )
            .map_err(native_error)?;
        return Ok((effect.stored, 0));
    }
    if let Some(field) = gps::number_fields()
        .iter()
        .find(|field| field.setting.key == key)
    {
        let input = e::NumberInput::Text(&source);
        let point = runtime.inline_decimal()?;
        let effect = if let Some(unit) = runtime.inline_unit_output(key) {
            field.edit(image, input, point, Some(unit))?
        } else {
            field.edit_live(
                image,
                input,
                point,
                gps::Resources {
                    language_descriptor: runtime.display_input_options().gui_language,
                    system_langid: runtime.display_system_langid(),
                },
            )?
        };
        return Ok((effect.stored, effect.notify_flags));
    }
    if let Some(field) = e::number_fields()
        .into_iter()
        .find(|field| field.key == key)
    {
        let effect = e::edit_number_at(
            image,
            field.name,
            index,
            e::NumberInput::Text(&source),
            if key == 0x307 {
                b'.'
            } else {
                runtime.inline_decimal()?
            },
        )?;
        return Ok((effect.stored, effect.notify_flags));
    }
    if settings::find("utc_offset_minutes")?.key == key {
        let effect = e::edit_utc_offset(image, e::NumberInput::Text(&source))?;
        return Ok((effect.stored, effect.notify_flags));
    }
    let number = e::NumberInput::Text(&source);
    match key {
        0x11f => {
            return e::edit_dtmf(image, index, e::TextInput::Text(&source))
                .map(text_effect)
                .map_err(native_error);
        }
        0x2d1 => {
            return e::edit_rtty(image, index, e::TextInput::Text(&source))
                .map(text_effect)
                .map_err(native_error);
        }
        0x12a => {
            let effect = e::edit_bandwidth(image, index, number, runtime.inline_decimal()?)?;
            return Ok((effect.number.stored, effect.number.notify_flags));
        }
        0x12b | 0x12c => {
            let side = if key == 0x12b {
                icf::PbtSide::One
            } else {
                icf::PbtSide::Two
            };
            let effect = e::edit_pbt(image, index, side, number, runtime.inline_decimal()?)?;
            return Ok((effect.stored, effect.notify_flags));
        }
        0x319 | 0x31b..=0x31d => {
            let field = match key {
                0x319 => settings::network::Field::Address,
                0x31b => settings::network::Field::Gateway,
                0x31c => settings::network::Field::PrimaryDns,
                _ => settings::network::Field::SecondaryDns,
            };
            let effect = image.edit_network_address(field, Some(&source));
            return Ok((effect.stored, effect.notify_flags));
        }
        _ => {}
    }
    if settings::catalog().iter().any(|field| field.key == key)
        || e::choice_fields()
            .iter()
            .chain(e::rf_choice_fields())
            .chain(e::key_assignment_fields())
            .any(|field| field.key() == key)
        || gps::choice_fields()
            .iter()
            .any(|field| field.setting.key == key)
        || e::PackedField::ALL
            .into_iter()
            .any(|field| field.key() == key)
        || digital::SelectionField::ALL
            .into_iter()
            .any(|field| field.key() == key)
        || e::ColorField::ALL
            .into_iter()
            .any(|field| field.key() == key)
        || gps::SentenceField::ALL
            .into_iter()
            .any(|field| field.key() == key)
        || gps::AlarmField::ALL
            .into_iter()
            .any(|field| field.key() == key)
        || e::ConditionalField::ALL
            .into_iter()
            .any(|field| field.key() == key)
        || e::AmplifierField::ALL
            .into_iter()
            .any(|field| field.key() == key)
        || e::DrField::ALL.into_iter().any(|field| field.key() == key)
        || e::MetadataField::ALL
            .into_iter()
            .any(|field| field.key() == key)
        || matches!(key,0x12d..=0x132|0x31a|0x37c|0x37e..=0x384)
    {
        return Err(AppError::invalid(
            "selection-only native field has no phase1 text commit producer",
        ));
    }
    Err(AppError::invalid(format!(
        "native inline field has no implemented input producer: key={key:04X}"
    )))
}

fn repeater(image: &mut Icf<'_>, frame: &InlineFrame, runtime: &Runtime) -> Result<(bool, u32)> {
    use cs705::repeater::{
        editor as lib,
        working::{DefaultContext, Working},
    };
    if frame.node.group != 11 {
        return Err(AppError::invalid(
            "repeater inline context differs from CT_Rptlist",
        ));
    }
    let index = usize::try_from(frame.node.index)
        .map_err(|_| AppError::invalid("negative repeater row"))?;
    let q = runtime.display_context()?;
    runtime.with_captured_ranges(|ranges| {
        let mut working = Working::from_icf(image,DefaultContext::from_context(q,runtime.display_system_langid())?)?;
        let mut holder = record::reset_holder(0x0c,index as u32,1);
        working.load_into(index,&mut holder)?;
        if !holder.valid() { return Err(AppError::invalid("inactive repeater row has no inline text producer; Type insertion is a separate native gesture")); }
        let r = lib::Runtime { ranges,live_q:q,system_langid:runtime.display_system_langid(),generation:runtime.generation(),align_callsign:runtime.display_input_options().auto_correct_enabled(),crt_decimal:if matches!(frame.node.key,0x14a|0x14c|0x14e|0x153) { runtime.inline_decimal()? } else { b'.' } };
        let mut effect = lib::Effect::default();
        let mut nls = lib::Nls { map:&mut csv::nls::platform_map,upper:&mut csv::nls::platform_uppercase };
        let result = match frame.node.key {
            0x144 | 0x145 | 0x147 | 0x148 => {
                let field = match frame.node.key { 0x144=>lib::TextField::Name,0x145=>lib::TextField::SubName,0x147=>lib::TextField::Call,_=>lib::TextField::Gateway };
                lib::text(&mut working,&mut holder,field,e::TextInput::Text(&frame.text),&r,&mut effect,&mut nls)
            }
            0x14a | 0x14c | 0x14e | 0x153 => {
                let field = match frame.node.key { 0x14a=>lib::NumberField::Frequency,0x14c=>lib::NumberField::Offset,0x14e=>lib::NumberField::ToneFrequency,_=>lib::NumberField::UtcOffset };
                lib::number(&mut working,&mut holder,field,e::NumberInput::Text(&frame.text),&r,&mut effect,&mut nls)
            }
            0x151 | 0x152 => lib::coordinate_input(&mut working,&mut holder,frame.node.key==0x152,e::TextInput::Text(&frame.text),&[csv::number::CoordinateEvent::Accept],&r,&mut effect),
            _ => return Err(AppError::invalid("repeater inline field is read-only or selection-only")),
        };
        if effect.native.save_requested || effect.inserted {
            image.patch_memory(&[(0,working.to_icf()?.memory())])?;
        }
        for &(event,synchronous) in &effect.notifications { runtime.inline_notification(event,synchronous)?; }
        result.map_err(native_error)?;
        Ok((effect.native.save_returned,effect.native.flags))
    })
}

fn ur(image: &mut Icf<'_>, frame: &InlineFrame, runtime: &Runtime) -> Result<(bool, u32)> {
    if frame.node.group != 12 || !matches!(frame.node.key, 0x13e | 0x13f) {
        return Err(AppError::invalid("UR inline field is not Name or Code"));
    }
    let index =
        u32::try_from(frame.node.index).map_err(|_| AppError::invalid("negative UR row"))?;
    let mut working = icf::rows::WorkingUr::from_icf(image)?;
    let old_bad = index >= u32::from(working.count());
    if old_bad && frame.node.key == 0x13e {
        return Ok((false, 0));
    }
    if old_bad {
        // An unbound T fails its actual word read AFTER shift/count prefixes;
        // an empty slice never supplies replacement/default reference bytes.
        let result = working.insert_one(
            index,
            icf::normalizer::ReferenceImage::Separate(runtime.reference_defaults().unwrap_or(&[])),
        );
        image.patch_memory(&[(0, working.to_icf()?.memory())])?;
        if !result? {
            return Ok((false, 0));
        }
        runtime.inline_notification(0x40, true)?;
        runtime.inline_notification(0x39, true)?;
    }
    let mut holder = record::reset_holder(0x0d, index, 1);
    working.load_into(index, &mut holder)?;
    let mut source = frame.text.clone();
    if frame.node.key == 0x13e {
        let name = csv::nls::name_candidate(&mut source, 16).map_err(native_error)?;
        let mut fixed = [b' '; 16];
        fixed[..name.len()].copy_from_slice(&name);
        holder.set_name(&fixed);
    } else {
        let candidate = csv::nls::whole_text(
            &mut source,
            csv::nls::Profile {
                uppercase: true,
                keep_kana: false,
                keep_spaces: true,
                punctuation_exceptions: b"/",
                maximum: 8,
            },
            csv::nls::platform_map,
            csv::nls::platform_uppercase,
        )
        .map_err(native_error)?;
        if candidate.is_empty() {
            return Ok((false, 0));
        }
        let mut fixed = [b' '; 8];
        fixed[..candidate.len()].copy_from_slice(&candidate);
        if runtime.display_input_options().auto_correct_enabled() {
            e::align_callsign(&mut fixed);
        }
        holder.set_callsign(MemoryCallsign::Your, &fixed);
    }
    working.save(index, &holder)?;
    image.patch_memory(&[(0, working.to_icf()?.memory())])?;
    Ok((true, if frame.node.key == 0x13f { 4 } else { 0 }))
}

fn gps_record(image: &mut Icf<'_>, frame: &InlineFrame, runtime: &Runtime) -> Result<(bool, u32)> {
    if frame.node.group != 10 {
        return Err(AppError::invalid("GPS inline context differs from CT_Gps"));
    }
    let group = usize::try_from(frame.factory.group)
        .map_err(|_| AppError::invalid("negative GPS group"))?;
    let row =
        usize::try_from(frame.node.index).map_err(|_| AppError::invalid("negative GPS row"))?;
    let physical = image
        .gps_groups()?
        .get(group)
        .and_then(|g| g.physical_slots().get(row))
        .copied();
    let inserted = physical.is_none();
    let physical = if let Some(physical) = physical {
        physical
    } else {
        if frame.node.key != 0x1b5 {
            return Err(AppError::invalid(
                "empty GPS row redirects ordinary text input to Name",
            ));
        }
        let outcome = cs705::gps::operations::insert(
            image,
            group,
            row,
            1,
            (row, row),
            |image| {
                let q = runtime.display_context().map_err(|_| icf::Error::Invalid {
                    line: 0,
                    reason: "native GPS initializer Q source unavailable".into(),
                })?;
                let clock = runtime.inline_clock().map_err(|_| icf::Error::Invalid {
                    line: 0,
                    reason: "native GPS initializer clock source unavailable".into(),
                })?;
                record::gps_initial::seed(image, q, runtime.display_system_langid(), clock)
            },
            |_, event| {
                runtime
                    .inline_notification(event.id, event.synchronous)
                    .map_err(|_| icf::Error::Invalid {
                        line: 0,
                        reason: "GPS initial notification request failed".into(),
                    })
            },
        )?;
        if outcome.completed == 0 {
            return Ok((false, 0));
        }
        image
            .gps_groups()?
            .get(group)
            .and_then(|g| g.physical_slots().get(row))
            .copied()
            .ok_or_else(|| AppError::invalid("inserted GPS row has no live physical slot"))?
    };
    if frame.node.key == 0x1ba {
        let absolute = image
            .gps_groups()?
            .iter()
            .take(group)
            .map(|group| group.physical_slots().len())
            .sum::<usize>()
            + row;
        let mut holder = cs705::gps::editor::load(image, absolute, 1)?;
        let effect = cs705::gps::editor::altitude(
            image,
            absolute,
            &mut holder,
            e::NumberInput::Text(&frame.text),
            runtime.inline_decimal()?,
        )?;
        return Ok((effect.accepted, effect.notify_flags));
    }
    let raw = image
        .physical_gps_record_raw(physical)?
        .ok_or_else(|| AppError::invalid("GPS physical record unavailable"))?;
    let at = raw.as_ptr() as usize - image.memory().as_ptr() as usize;
    let mut raw: [u8; 34] = raw
        .try_into()
        .map_err(|_| AppError::invalid("GPS record width differs from typed saver"))?;
    let mut holder = record::reset_holder(0x0a, row as u32, 1);
    record::decode_gps_into(&mut holder, &raw, group as u32);
    holder.set_valid(!inserted);
    match frame.node.key {
        0x1b5 => {
            let mut source = frame.text.clone();
            let name = csv::nls::name_candidate(&mut source, 16).map_err(native_error)?;
            let mut fixed = [b' '; 16];
            fixed[..name.len()].copy_from_slice(&name);
            holder.as_mut_bytes()[0x244..0x254].copy_from_slice(&fixed);
        }
        0x1b8 | 0x1b9 => {
            let longitude = frame.node.key == 0x1b9;
            let mut coordinate = *holder.coordinate_bytes(longitude);
            let mut dialog =
                csv::number::CoordinateDialog::new(coordinate, longitude, image.memory(), None)
                    .map_err(native_error)?;
            dialog.replace(&frame.text);
            if let Some(candidate) = dialog.event(csv::number::CoordinateEvent::Accept) {
                csv::number::coordinate_native_phase2(&mut coordinate, &candidate, image.memory())
                    .map_err(native_error)?;
                holder.set_coordinate_bytes(longitude, &coordinate);
                dialog.dispatched();
            } else {
                return Ok((false, 0));
            }
        }
        0x1b6 | 0x1b7 => {
            let mode = if frame.node.key == 0x1b7 {
                3
            } else {
                runtime.inline_date_order()?.min(2) as u8
            };
            let mut date: [u8; 6] = holder.as_bytes()[0x254..0x25a].try_into().unwrap();
            let offset = i16::from_le_bytes(image.memory()[0x396ff..0x39701].try_into().unwrap());
            let mut effect = csv::date_input::Effect::default();
            runtime.with_date_input_runtime(mode, |r| {
                csv::date_input::edit_masked(&mut date, mode, offset, &frame.text, r, &mut effect)
                    .map_err(native_error)
            })?;
            if !effect.changed {
                return Ok((false, 0));
            }
            holder.as_mut_bytes()[0x254..0x25a].copy_from_slice(&date);
        }
        _ => {
            return Err(AppError::invalid(
                "GPS inline field is read-only or selection-only",
            ));
        }
    }
    record::encode_gps_into(&holder, &mut raw, |offset, bytes| {
        image.patch_memory(&[(at + offset, bytes)])
    })?;
    Ok((true, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r3_settings_inline_wlan_and_gps_live_units_need_no_private_resources() {
        use cs705::printing::native::{CellNode, Factory};
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/private/cli-r3")
            .join(format!("inline-settings-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let format = root.join("format.json");
        std::fs::write(
            &format,
            br#"{"profile":"portable-ascii","crt_decimal":"."}"#,
        )
        .unwrap();
        let runtime = crate::records::Options {
            native_format_context: Some(format.to_str().unwrap().to_owned()),
            ..Default::default()
        }
        .load(std::iter::empty())
        .unwrap()
        .with_input_options(cs705::display::InputOptions {
            gui_language: Some(1),
            ..Default::default()
        });
        let frame = |class, group, key, text: &[u8]| InlineFrame {
            column: 0,
            row: 0,
            node: CellNode {
                group: if class == Class::Gps { 10 } else { 0 },
                key,
                index: 0,
                ..Default::default()
            },
            factory: Factory {
                class,
                resource: "cli-test".to_owned(),
                node: 0,
                group,
                parameter: Vec::new(),
            },
            input_code: 13,
            layout: 0,
            warning_bits: 0,
            focus: 0,
            action: 0,
            command: 0,
            phase: 1,
            notify: 0,
            initial_flags: 1,
            grid4: true,
            grid8: true,
            callback: 0,
            text: text.to_vec(),
        };
        let mut raw = vec![0xa5; 0x3c9c0];
        raw[0x355a1] = 6;
        raw[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        raw[0x396b5] = 0;
        raw[0x3c385..0x3c613].fill(255);
        raw[0x3c385..0x3c389].copy_from_slice(&[7, 0, 9, 0]);
        raw[0x3c5e1..0x3c5e3].copy_from_slice(&0_u16.to_le_bytes());
        let mut image = Icf::from_memory(raw).unwrap();
        for (key, at, text, expected) in [
            (0x328, 0x39383, b"a b\\c".as_slice(), b"abc".as_slice()),
            (0x329, 0x39393, b"abcd1234", b"abcd1234"),
            (0x32b, 0x393a4, b"a b\\c", b"abc"),
            (0x32c, 0x393b4, b"abcd1234", b"abcd1234"),
        ] {
            let mut expected_image = image.memory().to_vec();
            expected_image[at..at + 16].fill(b' ');
            expected_image[at..at + expected.len()].copy_from_slice(expected);
            assert!(
                edit_non_channel(&mut image, &frame(Class::Common, 0, key, text), &runtime)
                    .unwrap()
                    .0
            );
            assert_eq!(image.memory(), expected_image);
        }
        let altitude = frame(Class::Common, 0, 0x1c4, b"100");
        assert!(edit_non_channel(&mut image, &altitude, &runtime).unwrap().0);
        assert_eq!(&image.memory()[0x3c61b..0x3c61f], &3281_i32.to_le_bytes());
        let course = frame(Class::Common, 0, 0x1ef, b"123");
        assert!(edit_non_channel(&mut image, &course, &runtime).unwrap().0);
        assert_eq!(&image.memory()[0x3c776..0x3c778], &123_u16.to_le_bytes());
        let record = frame(Class::Gps, 2, 0x1ba, b"100");
        let mut expected = image.memory().to_vec();
        expected[0x32c19 + 34 * 7 + 14..0x32c19 + 34 * 7 + 18]
            .copy_from_slice(&3281_i32.to_le_bytes());
        assert!(edit_non_channel(&mut image, &record, &runtime).unwrap().0);
        assert_eq!(image.memory(), expected);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires the original private PE asset; bounded resource reads only"]
    fn native_resources_route_logical_text_to_real_field_effects() {
        use cs705::printing::native::{Resources, read_source};
        let path = std::env::var("CS705_PRINT_SOURCE").expect("CS705_PRINT_SOURCE");
        let asset = std::fs::File::open(path).unwrap();
        let bytes = read_source(asset).unwrap();
        let resources = Resources::from_pe(&bytes).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/private/logic-inline-fields");
        std::fs::create_dir_all(&root).unwrap();
        let range_path = root.join(format!("test-range-{}.json", std::process::id()));
        let format_path = root.join(format!("test-format-{}.json", std::process::id()));
        let mut q = [0; 24];
        q[0] = 1;
        let q_hex = q
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        std::fs::write(
            &range_path,
            serde_json::to_vec(&serde_json::json!({
                "generation_q_hex":q_hex,"live_q_hex":q_hex,
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(&format_path, serde_json::to_vec(&serde_json::json!({
            "profile":"portable-ascii","crt_decimal":".","rtl_decimal":".",
            "short_date":"dd.mm.yyyy","date_separator":".","time_separator":":",
            "current_year":2026,"two_digit_window":50,"clock":"12:34:56",
            "native_date_order":2,"native_clock_date":[2026,11,10],"native_time_format":"hh:nn:ss",
        })).unwrap()).unwrap();
        let options = crate::records::Options {
            native_range_context: Some(range_path.to_str().unwrap().to_owned()),
            native_format_context: Some(format_path.to_str().unwrap().to_owned()),
            inline_unit_output: vec!["442=6d".to_owned()],
            ..Default::default()
        };
        let runtime = options
            .load(std::iter::empty())
            .unwrap()
            .with_input_options(cs705::display::InputOptions {
                gui_language: Some(0),
                auto_correct: 0x100,
                ..Default::default()
            });
        let mut memory = vec![0; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        memory[0x355c0..0x35d2c].fill(255);
        memory[0x3c0a7..0x3c2ff].fill(255);
        memory[0x3c0a7..0x3c0a9].copy_from_slice(&0_u16.to_le_bytes());
        memory[0x3c385..0x3c613].fill(255);
        memory[0x2eda3..0x2f703].fill(b' ');
        memory[0x2eda3..0x2edab].copy_from_slice(b"FIRST   ");
        memory[0x396ff..0x39701].copy_from_slice(&60_i16.to_le_bytes());
        let mut image = Icf::from_memory(memory).unwrap();
        let tree = resources.tree(1).unwrap();
        let frame_at = |resource: &str, key: u32, index: i32, text: &[u8]| {
            let factory = tree
                .iter()
                .find(|factory| factory.resource == resource)
                .unwrap();
            let grid = resources
                .schema(factory, &[index], |name| {
                    crate::display::print_condition(name, &runtime).map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, e.message)
                    })
                })
                .unwrap();
            let (row, column) = grid
                .nodes
                .as_ref()
                .unwrap()
                .iter()
                .enumerate()
                .find_map(|(row, nodes)| {
                    nodes
                        .iter()
                        .position(|node| node.key == key && node.flags1c & 0x80 == 0)
                        .map(|column| (row, column))
                })
                .unwrap();
            InlineFrame::commit(&grid, column, row, text).unwrap()
        };
        let frame = |resource: &str, key: u32, text: &[u8]| frame_at(resource, key, 0, text);
        let comment = frame("GI_Sheet_Common", 0x2b0, b"{\"action\":\"cancel\"}");
        assert!(edit_non_channel(&mut image, &comment, &runtime).unwrap().0);
        assert!(image.clone_comment_raw().starts_with(b"{\"action\":"));
        assert!(edit_non_channel(&mut image, &comment, &runtime).unwrap().0);
        let civ = frame("GI_Sheet_Connectors", 0x307, b"aA");
        assert!(edit_non_channel(&mut image, &civ, &runtime).unwrap().0);
        assert_eq!(image.memory()[0x3934d], 170);
        let suffix = frame(
            "GI_Sheet_My_Callsign",
            u32::from(digital::TextField::MyCallSuffix.key()),
            b"TeSt",
        );
        assert_eq!(
            edit_non_channel(&mut image, &suffix, &runtime).unwrap(),
            (true, 0)
        );
        assert_eq!(
            digital::TextField::MyCallSuffix.raw(&image, 0).unwrap(),
            b"TEST"
        );
        let name = frame("GI_Sheet_You_Callsign", 0x13e, b"new name");
        assert_eq!(
            edit_non_channel(&mut image, &name, &runtime).unwrap(),
            (true, 0)
        );
        let mut ur = record::reset_holder(0x0d, 0, 1);
        record::load_callsign_storage_into(&image, 0, &mut ur).unwrap();
        assert_eq!(ur.name(), b"new name        ");
        assert_eq!(ur.callsign(MemoryCallsign::Your), b"FIRST   ");
        let code = frame("GI_Sheet_You_Callsign", 0x13f, b"AB C");
        assert_eq!(
            edit_non_channel(&mut image, &code, &runtime).unwrap(),
            (true, 4)
        );
        record::load_callsign_storage_into(&image, 0, &mut ur).unwrap();
        assert_eq!(ur.callsign(MemoryCallsign::Your), b"AB     C");
        let comment3 = frame("GI_Sheet_GPS", 0x1dc, b"third");
        assert!(edit_non_channel(&mut image, &comment3, &runtime).unwrap().0);
        assert!(
            gps::find_text("position-comment")
                .unwrap()
                .read(&image, 2)
                .unwrap()
                .starts_with(b"third")
        );
        let symbol3 = frame("GI_Sheet_GPS", 0x1d6, b"/>");
        assert_eq!(
            edit_non_channel(&mut image, &symbol3, &runtime).unwrap(),
            (true, 0x20)
        );
        assert_eq!(
            gps::find_symbol("position-symbol")
                .unwrap()
                .read(&image, 2)
                .unwrap(),
            *b"/>"
        );
        let readonly = frame(
            "GI_Sheet_My_Callsign",
            u32::from(digital::SelectionField::MyCall.key()),
            b"1",
        );
        let before = image.memory().to_vec();
        assert!(edit_non_channel(&mut image, &readonly, &runtime).is_err());
        assert_eq!(image.memory(), before);
        let scan = frame("GI_Sheet_ScanEdge", 0x119, b"no digits");
        assert_eq!(
            edit_non_channel(&mut image, &scan, &runtime).unwrap(),
            (false, 8)
        );
        for (key, text) in [(0x119, b"145.000".as_slice()), (0x11a, b"146.000")] {
            let scan = frame("GI_Sheet_ScanEdge", key, text);
            assert_eq!(
                edit_non_channel(&mut image, &scan, &runtime).unwrap(),
                (true, 8)
            );
        }
        let program_scan = frame("GI_Sheet_Common", 0x2a7, b"0");
        assert_eq!(
            edit_non_channel(&mut image, &program_scan, &runtime).unwrap(),
            (true, 0x20)
        );
        let edge = frame("GI_Sheet_BandEdge", 0x2bd, b"145.500");
        assert_eq!(
            edit_non_channel(&mut image, &edge, &runtime).unwrap(),
            (true, 0x28)
        );
        assert_eq!(
            icf::special::UserEdge::read(image.memory(), 0)
                .unwrap()
                .low_hz,
            145_500_000
        );
        let empty_name = frame("GI_Sheet_GPS_BankCH", 0x1b5, b"not saved on insert");
        let mut partial = Icf::from_memory(image.memory().to_vec()).unwrap();
        let mut missing_clock = options.clone();
        missing_clock.native_format_context = None;
        let missing_clock = missing_clock.load(std::iter::empty()).unwrap();
        assert!(edit_non_channel(&mut partial, &empty_name, &missing_clock).is_err());
        assert_eq!(partial.gps_groups().unwrap()[0].physical_slots(), &[0]);
        assert_eq!(
            edit_non_channel(&mut image, &empty_name, &runtime).unwrap(),
            (true, 0)
        );
        assert_eq!(
            image.physical_gps_name_raw(0).unwrap().unwrap(),
            b"                "
        );
        assert_eq!(
            &image.physical_gps_record_raw(0).unwrap().unwrap()[8..14],
            &[0x10, 0x11, 0x26, 0x11, 0x34, 0x56]
        );
        let date = frame("GI_Sheet_GPS_BankCH", 0x1b6, b"29/02/2024");
        assert_eq!(
            edit_non_channel(&mut image, &date, &runtime).unwrap(),
            (true, 0)
        );
        let time = frame("GI_Sheet_GPS_BankCH", 0x1b7, b"01:45:10");
        assert_eq!(
            edit_non_channel(&mut image, &time, &runtime).unwrap(),
            (true, 0)
        );
        assert_eq!(
            &image.physical_gps_record_raw(0).unwrap().unwrap()[8..14],
            &[0x29, 0x02, 0x24, 0, 0x45, 0x10]
        );
        let altitude = frame("GI_Sheet_GPS_BankCH", 0x1ba, b"100");
        assert_eq!(
            edit_non_channel(&mut image, &altitude, &runtime).unwrap(),
            (true, 0)
        );
        assert_eq!(
            &image.physical_gps_record_raw(0).unwrap().unwrap()[14..18],
            &3281_i32.to_le_bytes()
        );
        let insert_ur = frame_at("GI_Sheet_You_Callsign", 0x13f, 1, b"NEXT");
        assert!(edit_non_channel(&mut image, &insert_ur, &runtime).is_err());
        assert_eq!(icf::rows::WorkingUr::from_icf(&image).unwrap().count(), 2);
        record::load_callsign_storage_into(&image, 1, &mut ur).unwrap();
        assert_eq!(ur.callsign(MemoryCallsign::Your), b"CALLSIGN");
        std::fs::remove_file(range_path).unwrap();
        std::fs::remove_file(format_path).unwrap();
    }
}
