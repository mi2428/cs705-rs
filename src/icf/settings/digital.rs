//! Native MyCall/TX selection cases over the shared choice input calculation.
//! Storage field edits, UI delivery and the document publisher are separate.

use super::extended::{ChoiceInput, FixedTextInput, TextEffect, choice_candidate};
use crate::{
    csv::{Error, nls},
    icf::{Icf, MemoryCallsign, record, record_image},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionField {
    MyCall,
    TxMessage,
}
impl SelectionField {
    pub const ALL: [Self; 2] = [Self::MyCall, Self::TxMessage];
    pub fn name(self) -> &'static str {
        match self {
            Self::MyCall => "my-call-selected",
            Self::TxMessage => "tx-message-selected",
        }
    }
    pub fn feature_id(self) -> &'static str {
        match self {
            Self::MyCall => "A2-H55779-01",
            Self::TxMessage => "A2-H55787-01",
        }
    }
    pub fn key(self) -> u16 {
        match self {
            Self::MyCall => 0x134,
            Self::TxMessage => 0x13b,
        }
    }
    pub fn count(self) -> usize {
        match self {
            Self::MyCall => 6,
            Self::TxMessage => 5,
        }
    }
    pub fn active_base(self) -> u8 {
        u8::from(self == Self::TxMessage)
    }
    fn at(self) -> usize {
        match self {
            Self::MyCall => 0x3ab81,
            Self::TxMessage => 0x3ab80,
        }
    }
    fn check(self, index: usize) -> Result<(), Error> {
        if index < self.count() {
            Ok(())
        } else {
            Err(Error::Invalid("digital native index outside field axis"))
        }
    }
    pub fn active_raw(self, image: &Icf<'_>) -> u8 {
        image.memory()[self.at()]
    }
    pub fn selected(self, image: &Icf<'_>, index: usize) -> Result<bool, Error> {
        self.check(index)?;
        Ok(usize::from(self.active_raw(image)) == index + usize::from(self.active_base()))
    }
    pub fn find(name_or_id: &str) -> Result<Self, Error> {
        Self::ALL
            .into_iter()
            .find(|f| f.name() == name_or_id || f.feature_id() == name_or_id)
            .ok_or(Error::Invalid("unknown digital selection field"))
    }
    /// 6C009/6C28B keep MyCall OFF distinct from TX Message OFF. Unknown old
    /// active bytes are comparison inputs, not a reason to reject a new event.
    pub fn edit(
        self,
        image: &mut Icf<'_>,
        index: usize,
        input: ChoiceInput,
    ) -> Result<SelectionEffect, Error> {
        let old = i64::from(self.selected(image, index)?);
        let candidate = choice_candidate(old, &[0, 1], input);
        let stored = candidate.is_some_and(|n| self == Self::TxMessage || n == 1);
        if stored {
            let raw = if candidate == Some(1) {
                index as u8 + self.active_base()
            } else {
                0
            };
            image
                .patch_memory(&[(self.at(), &[raw])])
                .map_err(|_| Error::Invalid("digital selection image store"))?;
        }
        Ok(SelectionEffect {
            helper_accepted: candidate.is_some(),
            stored,
            candidate,
            active_raw: self.active_raw(image),
            notify_flags: if stored {
                if self == Self::MyCall { 0x20 } else { 4 }
            } else {
                0
            },
            popup_requested: input == ChoiceInput::Begin,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionEffect {
    pub helper_accepted: bool,
    pub stored: bool,
    pub candidate: Option<i32>,
    pub active_raw: u8,
    pub notify_flags: u32,
    pub popup_requested: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextField {
    MyCall,
    MyCallSuffix,
    TxMessage,
}
impl TextField {
    pub const ALL: [Self; 3] = [Self::MyCall, Self::MyCallSuffix, Self::TxMessage];
    pub fn name(self) -> &'static str {
        match self {
            Self::MyCall => "my-call",
            Self::MyCallSuffix => "my-call-suffix",
            Self::TxMessage => "tx-message",
        }
    }
    pub fn feature_id(self) -> &'static str {
        match self {
            Self::MyCall => "A2-H55779-02",
            Self::MyCallSuffix => "A2-H55779-03",
            Self::TxMessage => "A2-H55787-02",
        }
    }
    pub fn key(self) -> u16 {
        match self {
            Self::MyCall => 0x136,
            Self::MyCallSuffix => 0x137,
            Self::TxMessage => 0x13a,
        }
    }
    pub fn count(self) -> usize {
        if self == Self::TxMessage { 5 } else { 6 }
    }
    pub fn width(self) -> usize {
        match self {
            Self::MyCall => 8,
            Self::MyCallSuffix => 4,
            Self::TxMessage => 20,
        }
    }
    pub fn find(name_or_id: &str) -> Result<Self, Error> {
        Self::ALL
            .into_iter()
            .find(|f| f.name() == name_or_id || f.feature_id() == name_or_id)
            .ok_or(Error::Invalid("unknown digital text field"))
    }
    fn at(self, index: usize) -> Result<usize, Error> {
        if index >= self.count() {
            return Err(Error::Invalid("digital text index outside native axis"));
        }
        Ok(match self {
            Self::MyCall => 0x2ed5b + 12 * index,
            Self::MyCallSuffix => 0x2ed63 + 12 * index,
            Self::TxMessage => 0x32bb5 + 20 * index,
        })
    }
    pub fn raw<'a>(self, image: &'a Icf<'_>, index: usize) -> Result<&'a [u8], Error> {
        let at = self.at(index)?;
        Ok(&image.memory()[at..at + self.width()])
    }
    pub fn display(self, image: &Icf<'_>, index: usize) -> Result<Vec<u8>, Error> {
        Ok(c_prefix(self.raw(image, index)?).to_vec())
    }
    pub fn edit(
        self,
        image: &mut Icf<'_>,
        index: usize,
        input: FixedTextInput<'_>,
        alignment: u32,
    ) -> Result<TextEffect, Error> {
        self.edit_with_nls(
            image,
            index,
            input,
            alignment,
            nls::platform_map,
            nls::platform_uppercase,
        )
    }
    pub fn edit_with_nls(
        self,
        image: &mut Icf<'_>,
        index: usize,
        input: FixedTextInput<'_>,
        alignment: u32,
        map: impl FnMut(&[u8], &mut [u8]) -> Result<i32, Error>,
        upper: impl FnMut(&mut [u8]) -> Result<u32, Error>,
    ) -> Result<TextEffect, Error> {
        let at = self.at(index)?;
        let mut display = self.display(image, index)?;
        let state_out = 2 * u32::from(matches!(input, FixedTextInput::Begin));
        let stored = matches!(input, FixedTextInput::Text { .. });
        if let FixedTextInput::Text { source, phase } = input {
            display = normalize(
                source,
                profile(self.width(), self != Self::TxMessage, b"/"),
                self == Self::MyCall,
                phase,
                alignment,
                map,
                upper,
            )?;
            let mut raw = vec![b' '; self.width()];
            raw[..display.len()].copy_from_slice(&display);
            image
                .patch_memory(&[(at, &raw)])
                .map_err(|_| Error::Invalid("digital text image store"))?;
        }
        Ok(TextEffect {
            stored,
            display,
            state_out,
            notify_flags: 0,
        })
    }
}

fn c_prefix(bytes: &[u8]) -> &[u8] {
    bytes.split(|b| *b == 0).next().unwrap()
}
fn profile(maximum: usize, call: bool, exceptions: &[u8]) -> nls::Profile<'_> {
    nls::Profile {
        uppercase: call,
        keep_kana: !call,
        keep_spaces: true,
        punctuation_exceptions: if call {
            exceptions
        } else {
            b"!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"
        },
        maximum,
    }
}
fn normalize(
    source: &mut [u8],
    profile: nls::Profile<'_>,
    aligned: bool,
    phase: u32,
    alignment: u32,
    map: impl FnMut(&[u8], &mut [u8]) -> Result<i32, Error>,
    upper: impl FnMut(&mut [u8]) -> Result<u32, Error>,
) -> Result<Vec<u8>, Error> {
    if !matches!(phase, 1 | 2) {
        return Err(Error::Invalid("digital text requires native phase1 or2"));
    }
    let mut candidate = nls::whole_text(source, profile, map, upper)?;
    if aligned {
        candidate.resize(8, b' ');
        if alignment != 0 && phase == 1 {
            super::extended::align_callsign(candidate.as_mut_slice().try_into().unwrap());
        }
    }
    Ok(candidate)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowedView {
    pub prefix_count: u32,
    pub bad: bool,
    pub duplicate_warning: bool,
    pub parent_enable_raw: i64,
    pub raw: [u8; 8],
    pub display: Vec<u8>,
}
fn typed_error(_: crate::icf::Error) -> Error {
    Error::Invalid("digital typed storage operation")
}
fn allowed_holder(image: &Icf<'_>, index: u32) -> Result<record::DecodedRecord, Error> {
    if index > 30 {
        return Err(Error::Invalid(
            "allowed callsign row outside native0..30 axis",
        ));
    }
    let mut holder = record::reset_holder(0x17, index, 0);
    record_image::load_into(image.memory(), 0x17, index, &mut holder).map_err(typed_error)?;
    Ok(holder)
}
pub fn allowed_view(image: &Icf<'_>, index: u32) -> Result<AllowedView, Error> {
    let count = record_image::callsign_count(image.memory(), 0x17).map_err(typed_error)?;
    if index > count {
        return Err(Error::Invalid(
            "allowed row exceeds current prefix plus add row",
        ));
    }
    let holder = allowed_holder(image, index)?;
    let bad = count == 0 || index >= count;
    let raw = *holder.callsign(MemoryCallsign::Your);
    let super::Value::Number(parent_enable_raw) = super::find("CL_Common_Gateway_CallSignSet")
        .map_err(typed_error)?
        .read(image, 0)
        .map_err(typed_error)?;
    Ok(AllowedView {
        prefix_count: count,
        bad,
        duplicate_warning: !bad
            && record_image::callsign_bad(image.memory(), &holder).map_err(typed_error)?,
        parent_enable_raw,
        raw,
        display: if bad {
            Vec::new()
        } else {
            c_prefix(&raw).to_vec()
        },
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllowedEffect {
    pub inserted: bool,
    pub stored: bool,
    pub full_warning: bool,
    pub view_bad: bool,
    pub duplicate_warning: bool,
    pub state_out: u32,
    pub notify_flags: u32,
    /// Ordered synchronous requests, not evidence of GUI delivery.
    pub notifications: Vec<u32>,
    pub display: Vec<u8>,
}
pub fn edit_allowed(
    image: &mut Icf<'_>,
    index: u32,
    input_code: i32,
    input: FixedTextInput<'_>,
    alignment: u32,
    effect: &mut AllowedEffect,
) -> Result<(), Error> {
    edit_allowed_with_nls(
        image,
        index,
        input_code,
        input,
        alignment,
        effect,
        nls::platform_map,
        nls::platform_uppercase,
    )
}
/// 6FEB3/D196C mutate the actual image before the editor. A later false return
/// or NLS error does not roll back that prefix; the publisher decides policy.
#[allow(clippy::too_many_arguments)]
pub fn edit_allowed_with_nls(
    image: &mut Icf<'_>,
    index: u32,
    input_code: i32,
    input: FixedTextInput<'_>,
    alignment: u32,
    effect: &mut AllowedEffect,
    map: impl FnMut(&[u8], &mut [u8]) -> Result<i32, Error>,
    upper: impl FnMut(&mut [u8]) -> Result<u32, Error>,
) -> Result<(), Error> {
    let view = allowed_view(image, index)?;
    *effect = AllowedEffect {
        view_bad: view.bad,
        ..AllowedEffect::default()
    };
    let trigger = matches!(input_code, 8 | 13 | 0x7100) || (0x20..=0x7e).contains(&input_code);
    if view.bad && trigger {
        if view.prefix_count >= 30 {
            effect.full_warning = true;
            return Ok(());
        }
        for destination in ((index + 1)..30).rev() {
            let mut holder = allowed_holder(image, destination - 1)?;
            record_image::store_into(&mut image.memory, 0x17, destination, &mut holder)
                .map_err(typed_error)?;
        }
        let mut seed = crate::icf::rows::ur_default(index, 0);
        record_image::store_into(&mut image.memory, 0x17, index, &mut seed).map_err(typed_error)?;
        effect.inserted = true;
        effect.notifications.extend([0x40, 0x39]);
    }
    let mut holder = allowed_holder(image, index)?;
    effect.display = c_prefix(holder.callsign(MemoryCallsign::Your)).to_vec();
    effect.state_out = 2 * u32::from(matches!(input, FixedTextInput::Begin));
    if let FixedTextInput::Text { source, phase } = input {
        let candidate = normalize(
            source,
            profile(8, true, b""),
            true,
            phase,
            alignment,
            map,
            upper,
        )?;
        holder.set_callsign(
            MemoryCallsign::Your,
            candidate.as_slice().try_into().unwrap(),
        );
        record_image::store_into(&mut image.memory, 0x17, index, &mut holder)
            .map_err(typed_error)?;
        record_image::load_into(image.memory(), 0x17, index, &mut holder).map_err(typed_error)?;
        effect.stored = true;
        effect.notify_flags = 4;
        effect.display = candidate;
    }
    if view.bad {
        effect.display.clear();
    } else {
        effect.duplicate_warning =
            record_image::callsign_bad(image.memory(), &holder).map_err(typed_error)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReflectorView {
    pub valid: bool,
    pub raw: Option<[u8; 8]>,
    pub display: Vec<u8>,
}
/// H55830 is a current-image history view, not an editable callsign list.
pub fn reflector_view(image: &Icf<'_>, index: u32) -> Result<ReflectorView, crate::icf::Error> {
    let mut holder = record::reset_holder(0x14, index, 0);
    record::load_reflector_storage_into(image, index, &mut holder)?;
    let raw = *holder.history_call();
    Ok(ReflectorView {
        valid: holder.valid(),
        raw: holder.valid().then_some(raw),
        display: if holder.valid() {
            c_prefix(&raw).to_vec()
        } else {
            Vec::new()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icf::{Icf, settings::Selection};
    fn image() -> Icf<'static> {
        let mut memory = vec![0xa6; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        Icf::from_memory(memory).unwrap()
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
    fn text_profiles_indices_phases_literal_widths_and_nls_failure_preserve_image() {
        for field in TextField::ALL {
            for index in 0..field.count() {
                let mut image = image();
                let before = image.memory().to_vec();
                let mut source = b"ab/c.d".to_vec();
                let effect = field
                    .edit(
                        &mut image,
                        index,
                        FixedTextInput::Text {
                            source: &mut source,
                            phase: 2,
                        },
                        1,
                    )
                    .unwrap();
                assert!(effect.stored);
                let mut expected = before;
                let at = field.at(index).unwrap();
                let literal: &[u8] = match field {
                    TextField::MyCall => b"AB/C D  ",
                    TextField::MyCallSuffix => b"AB/C",
                    TextField::TxMessage => b"ab/c.d              ",
                };
                expected[at..at + field.width()].copy_from_slice(literal);
                assert_eq!(image.memory(), expected);
                assert!(
                    field
                        .edit(
                            &mut image,
                            index,
                            FixedTextInput::Text {
                                source: &mut b"ab/c.d".to_vec(),
                                phase: 2
                            },
                            1
                        )
                        .unwrap()
                        .stored
                );
                for input in [
                    FixedTextInput::Cancel,
                    FixedTextInput::Begin,
                    FixedTextInput::Display,
                ] {
                    assert!(!field.edit(&mut image, index, input, 0).unwrap().stored);
                    assert_eq!(image.memory(), expected);
                }
                for phase in [0, 3, u32::MAX] {
                    let mut source = b"a\0b".to_vec();
                    assert!(
                        field
                            .edit(
                                &mut image,
                                index,
                                FixedTextInput::Text {
                                    source: &mut source,
                                    phase
                                },
                                1
                            )
                            .is_err()
                    );
                    assert_eq!(source, b"a\0b");
                    assert_eq!(image.memory(), expected);
                }
            }
            let mut image = image();
            let before = image.memory().to_vec();
            for index in [field.count(), usize::MAX] {
                assert!(
                    field
                        .edit(&mut image, index, FixedTextInput::Display, 0)
                        .is_err()
                );
                assert_eq!(image.memory(), before);
            }
            let mut source = b"a\0b".to_vec();
            assert!(
                field
                    .edit_with_nls(
                        &mut image,
                        0,
                        FixedTextInput::Text {
                            source: &mut source,
                            phase: 1
                        },
                        0,
                        |from, _| {
                            assert_eq!(&from[..3], b"a b");
                            Err(Error::SourceUnavailable("injected NLS failure"))
                        },
                        nls::platform_uppercase
                    )
                    .is_err()
            );
            assert_eq!(source, b"a b");
            assert_eq!(image.memory(), before);
        }
        for (phase, alignment, literal) in [
            (1, 1, b"AB    CD"),
            (1, 0, b"AB CD   "),
            (2, 1, b"AB CD   "),
        ] {
            let mut image = image();
            TextField::MyCall
                .edit(
                    &mut image,
                    5,
                    FixedTextInput::Text {
                        source: &mut b"ab cd".to_vec(),
                        phase,
                    },
                    alignment,
                )
                .unwrap();
            assert_eq!(TextField::MyCall.raw(&image, 5).unwrap(), literal);
        }
        let mut image = image();
        let mut source = vec![0xa1, b'/', 0xdf, b'a'];
        let stage = std::cell::Cell::new(0);
        TextField::MyCall
            .edit_with_nls(
                &mut image,
                0,
                FixedTextInput::Text {
                    source: &mut source,
                    phase: 2,
                },
                0,
                |from, to| {
                    assert_eq!(&from[..4], &[0xa1, b'/', 0xdf, b'a']);
                    to[..4].copy_from_slice(&[0xa1, b'/', 0xdf, b'a']);
                    stage.set(1);
                    Ok(0)
                },
                |to| {
                    assert_eq!(stage.get(), 1);
                    to[3] = b'A';
                    stage.set(2);
                    Ok(0)
                },
            )
            .unwrap();
        assert_eq!(stage.get(), 2);
        assert_eq!(TextField::MyCall.raw(&image, 0).unwrap(), b" / A    ");
        TextField::TxMessage
            .edit_with_nls(
                &mut image,
                4,
                FixedTextInput::Text {
                    source: &mut [0xa1, b'/', 0xdf, b'a'],
                    phase: 2,
                },
                0,
                |_, to| {
                    to[..4].copy_from_slice(&[0xa1, b'/', 0xdf, b'a']);
                    Ok(0)
                },
                |_| panic!("TX must not call Upper"),
            )
            .unwrap();
        assert_eq!(
            &TextField::TxMessage.raw(&image, 4).unwrap()[..4],
            &[0xa1, b'/', 0xdf, b'a']
        );
        image
            .patch_memory(&[(0x2ed5b, &[0x82, 0xa0, b' ', 0, b'Z', b' ', b' ', b' '])])
            .unwrap();
        assert_eq!(
            TextField::MyCall.display(&image, 0).unwrap(),
            [0x82, 0xa0, b' ']
        );
    }
    fn allowed_image(count: u32) -> Icf<'static> {
        let mut image = image();
        image.memory[0x395ad..0x3969d].fill(b' ');
        for index in 0..count {
            let mut holder = record::reset_holder(0x17, index, 0);
            let mut call = *b"ROW00000";
            call[6] = b'0' + (index / 10) as u8;
            call[7] = b'0' + (index % 10) as u8;
            holder.set_callsign(MemoryCallsign::Your, &call);
            record_image::store_into(&mut image.memory, 0x17, index, &mut holder).unwrap();
        }
        image
    }
    #[cfg(not(windows))]
    #[test]
    fn platform_defaults_reject_nonascii_and_keep_sanitation_and_insert_prefixes() {
        for field in TextField::ALL {
            let mut image = image();
            let before = image.memory().to_vec();
            let mut source = [b'a', 0, 0x82, 0xa0];
            let result = field.edit(
                &mut image,
                0,
                FixedTextInput::Text {
                    source: &mut source,
                    phase: 1,
                },
                0,
            );
            assert!(matches!(result, Err(Error::SourceUnavailable(_))));
            assert_eq!(source, [b'a', b' ', 0x82, 0xa0]);
            assert_eq!(image.memory(), before);
        }
        let mut image = allowed_image(0);
        let mut expected = image.memory().to_vec();
        expected[0x395ad..0x395b5].copy_from_slice(b"CALLSIGN");
        let mut source = [b'a', 0, 0x82, 0xa0];
        let mut effect = AllowedEffect::default();
        let result = edit_allowed(
            &mut image,
            0,
            13,
            FixedTextInput::Text {
                source: &mut source,
                phase: 1,
            },
            0,
            &mut effect,
        );
        assert!(matches!(result, Err(Error::SourceUnavailable(_))));
        assert_eq!(source, [b'a', b' ', 0x82, 0xa0]);
        assert!(effect.inserted && !effect.stored);
        assert_eq!(effect.notifications, [0x40, 0x39]);
        assert_eq!(image.memory(), expected);
    }

    #[test]
    fn allowed_prefix_insert_false_edit_full_row_duplicate_warning_and_late_nls_prefix() {
        for count in 0..=30 {
            let mut image = allowed_image(count);
            let before = image.memory().to_vec();
            let mut effect = AllowedEffect::default();
            assert!(allowed_view(&image, count).unwrap().bad);
            edit_allowed(
                &mut image,
                count,
                13,
                FixedTextInput::Cancel,
                0,
                &mut effect,
            )
            .unwrap();
            let mut expected = before;
            if count < 30 {
                for destination in ((count + 1)..30).rev() {
                    let at = 0x395ad + 8 * destination as usize;
                    expected.copy_within(at - 8..at, at);
                }
                let at = 0x395ad + 8 * count as usize;
                expected[at..at + 8].copy_from_slice(b"CALLSIGN");
                assert!(effect.inserted && !effect.stored);
                assert_eq!(effect.notifications, [0x40, 0x39]);
                assert_eq!(
                    record_image::callsign_count(image.memory(), 0x17).unwrap(),
                    count + 1
                );
            } else {
                assert!(effect.full_warning);
                assert!(!effect.inserted);
                assert!(effect.notifications.is_empty());
            }
            assert_eq!(image.memory(), expected);
            assert!(effect.display.is_empty());
        }
        for code in [i32::MIN, -1, 0, 7, 27, 0x7f, 0xa1, i32::MAX] {
            let mut image = allowed_image(0);
            let before = image.memory().to_vec();
            let mut effect = AllowedEffect::default();
            edit_allowed(&mut image, 0, code, FixedTextInput::Cancel, 0, &mut effect).unwrap();
            assert!(!effect.inserted);
            assert_eq!(image.memory(), before);
        }
        for index in 0..30 {
            let mut image = allowed_image(30);
            let before = image.memory().to_vec();
            let mut effect = AllowedEffect::default();
            image.memory[0x395ac] = 0;
            edit_allowed(
                &mut image,
                index,
                13,
                FixedTextInput::Text {
                    source: &mut b"ab/c d".to_vec(),
                    phase: 1,
                },
                1,
                &mut effect,
            )
            .unwrap();
            let mut expected = before;
            expected[0x395ac] = 0;
            let at = 0x395ad + 8 * index as usize;
            expected[at..at + 8].copy_from_slice(b"AB C   D");
            assert_eq!(image.memory(), expected);
            assert!(effect.stored && !effect.inserted);
            assert_eq!(effect.notify_flags, 4);
        }
        let mut image = allowed_image(2);
        image.memory[0x395ad..0x395bd].copy_from_slice(b"AB\0CD   AB\0CD   ");
        assert!(!allowed_view(&image, 0).unwrap().duplicate_warning);
        assert!(allowed_view(&image, 1).unwrap().duplicate_warning);
        image.memory[0x395bd..0x395c5].fill(b' ');
        image.memory[0x395c5..0x395cd].copy_from_slice(b"LATE    ");
        assert_eq!(allowed_view(&image, 2).unwrap().prefix_count, 2);
        assert!(allowed_view(&image, 3).is_err());
        let mut effect = AllowedEffect::default();
        let before = image.memory().to_vec();
        let mut source = b"a\0b".to_vec();
        assert!(
            edit_allowed_with_nls(
                &mut image,
                2,
                8,
                FixedTextInput::Text {
                    source: &mut source,
                    phase: 1
                },
                0,
                &mut effect,
                |_, _| Err(Error::SourceUnavailable("injected late NLS failure")),
                nls::platform_uppercase
            )
            .is_err()
        );
        let mut expected = before;
        expected.copy_within(0x395bd..0x39695, 0x395c5);
        expected[0x395bd..0x395c5].copy_from_slice(b"CALLSIGN");
        assert_eq!(image.memory(), expected);
        assert_eq!(source, b"a b");
        assert!(effect.inserted && !effect.stored);
        assert_eq!(effect.notifications, [0x40, 0x39]);
        for index in [31, u32::MAX] {
            assert!(allowed_view(&image, index).is_err());
        }
    }
    #[test]
    fn reflector_uses_current_map_raw8_nul_high_bytes_spaces_and_no_physical_clamp() {
        let mut image = image();
        for index in 0..5 {
            let physical = [0, 1, 4, 17, 254][index];
            image.memory[0x3c363 + index] = physical;
            let at = 0x32aed + 8 * usize::from(physical);
            image.memory[at..at + 8]
                .copy_from_slice(&[0x82, 0xa0, b' ', 0, b'Z', b' ', b' ', b' ']);
        }
        let before = image.memory().to_vec();
        for index in 0..5 {
            let view = reflector_view(&image, index).unwrap();
            assert!(view.valid);
            assert_eq!(
                view.raw,
                Some([0x82, 0xa0, b' ', 0, b'Z', b' ', b' ', b' '])
            );
            assert_eq!(view.display, [0x82, 0xa0, b' ']);
            assert_eq!(image.memory(), before);
        }
        image.memory[0x3c363] = 255;
        assert!(!reflector_view(&image, 0).unwrap().valid);
        assert!(reflector_view(&image, 0).unwrap().display.is_empty());
        image.memory[0x32af5..0x32afd].copy_from_slice(b"CALL    ");
        assert_eq!(reflector_view(&image, 1).unwrap().display, b"CALL    ");
        for index in [5, u32::MAX] {
            assert!(reflector_view(&image, index).is_err());
        }
    }
    #[test]
    fn active_bases_off_same_value_unknown_old_and_all_native_indices_are_distinct() {
        for field in SelectionField::ALL {
            for index in 0..field.count() {
                let mut image = image();
                image.patch_memory(&[(field.at(), &[255])]).unwrap();
                let before = image.memory().to_vec();
                let effect = field.edit(&mut image, index, selected(1)).unwrap();
                assert!(effect.helper_accepted && effect.stored);
                assert_eq!(effect.active_raw, index as u8 + field.active_base());
                let mut expected = before;
                expected[field.at()] = effect.active_raw;
                assert_eq!(image.memory(), expected);
                assert_eq!(
                    effect.notify_flags,
                    if field == SelectionField::MyCall {
                        32
                    } else {
                        4
                    }
                );
                assert!(field.edit(&mut image, index, selected(1)).unwrap().stored);
                let active = field.active_raw(&image);
                let off = field.edit(&mut image, index, selected(0)).unwrap();
                assert!(off.helper_accepted);
                assert_eq!(
                    (off.stored, off.active_raw),
                    if field == SelectionField::MyCall {
                        (false, active)
                    } else {
                        (true, 0)
                    }
                );
                let other = field.edit(&mut image, index, selected(0x101)).unwrap();
                assert_eq!(
                    other.active_raw,
                    if field == SelectionField::MyCall {
                        active
                    } else {
                        0
                    }
                );
            }
        }
        for field in SelectionField::ALL {
            let mut image = image();
            let before = image.memory().to_vec();
            for input in [
                ChoiceInput::Cancel,
                ChoiceInput::Display,
                ChoiceInput::Begin,
                ChoiceInput::Selected(Selection {
                    key: 2,
                    phase: 2,
                    candidate: 1,
                    accepted: true,
                }),
                ChoiceInput::Selected(Selection {
                    key: 1,
                    phase: 1,
                    candidate: 1,
                    accepted: true,
                }),
                ChoiceInput::Selected(Selection {
                    key: 1,
                    phase: 2,
                    candidate: 1,
                    accepted: false,
                }),
            ] {
                let effect = field.edit(&mut image, 0, input).unwrap();
                assert!(!effect.helper_accepted && !effect.stored);
                assert_eq!(image.memory(), before);
            }
            for index in [field.count(), usize::MAX] {
                assert!(field.edit(&mut image, index, selected(1)).is_err());
                assert_eq!(image.memory(), before);
            }
            assert!(field.edit(&mut image, 0, ChoiceInput::Next).unwrap().stored);
        }
    }
}
