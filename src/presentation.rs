//! Independent UTF-8 presentation, never a storage or source-resource codec.
//!
//! Language resources live in `presentation/locales/*.tsv`. Each file owns
//! complete messages and named placeholders; this module only selects IDs and
//! supplies typed arguments. It performs no I/O or host-locale discovery.

use crate::icf::settings::{self, extended::FixedValue};
use std::{fmt, str::FromStr};

mod resources;

/// Existing semantic values, not captions guessed from raw numeric ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Boolean(bool),
    Agc(crate::icf::Agc),
    FilterShape(crate::icf::FilterShape),
    RepeaterMode(crate::icf::RepeaterMode),
    RepeaterDuplex(crate::icf::RepeaterDuplex),
    RepeaterTone(crate::icf::RepeaterTone),
    RepeaterPosition(crate::icf::RepeaterPosition),
}

/// Display languages with manufacturer-provided IC-705 manuals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Language {
    English,
    Japanese,
    German,
    French,
    Spanish,
    Italian,
}

impl Language {
    pub const ALL: [Self; 6] = [
        Self::English,
        Self::Japanese,
        Self::German,
        Self::French,
        Self::Spanish,
        Self::Italian,
    ];

    /// Stable language tags are independent of radio settings and native C00.
    pub const fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Japanese => "ja",
            Self::German => "de",
            Self::French => "fr",
            Self::Spanish => "es",
            Self::Italian => "it",
        }
    }
}

/// An invalid presentation request, not a source-codec failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    UnsupportedLanguage,
    UnregisteredField { key: u16 },
    InvalidIndex,
    InvalidSymbolWidth,
    AmbiguousField { key: u16 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.localized(Language::English))
    }
}
impl std::error::Error for Error {}

impl Error {
    pub fn localized(self, language: Language) -> String {
        let id = match self {
            Self::UnsupportedLanguage => "error.unsupported_language",
            Self::UnregisteredField { key } => {
                return resources::format(
                    language,
                    "error.unregistered_field",
                    &[("key", &format!("{key:04X}"))],
                );
            }
            Self::InvalidIndex => "error.invalid_index",
            Self::InvalidSymbolWidth => "error.invalid_symbol_width",
            Self::AmbiguousField { key } => {
                return resources::format(
                    language,
                    "error.ambiguous_field",
                    &[("key", &format!("{key:04X}"))],
                );
            }
        };
        resources::format(language, id, &[])
    }
}

impl FromStr for Language {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|language| language.code() == value)
            .ok_or(Error::UnsupportedLanguage)
    }
}

/// A complete message. Dynamic arguments are never translated word fragments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    Enabled,
    Disabled,
    NoSymbol,
    NotApplicable,
    AgcFast,
    AgcDelay100Millis,
    Code(i64),
    UnknownCode(i64),
    Number(i64),
    OutOfRange(i64),
    ReadOnly,
    Cancelled,
    NotPublished,
    PartialNotPublished,
    PartialPublished,
    WriteAcknowledged,
    ReadRadio,
    WriteRadio,
    SaveFile,
    ReadFile,
    RowsProcessed(u64),
}

/// Semantic identity stays available alongside the translated label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldText {
    pub key: u16,
    pub name: std::borrow::Cow<'static, str>,
    pub label: &'static str,
}

/// Schema membership is neither a guessed caption nor permission to edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueText {
    pub raw: i64,
    pub text: String,
    pub in_domain: bool,
}

/// Immutable per-client context, independent of CSV locale, C00 and LANGID.
///
/// ```
/// use cs705::presentation::{Language, Message, Presenter};
/// let language: Language = "de".parse()?;
/// let presenter = Presenter::new(language);
/// let text = presenter.message(Message::RowsProcessed(2));
/// assert!(text.contains('2'));
/// # Ok::<(), cs705::presentation::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Presenter {
    language: Language,
}

impl Presenter {
    pub const fn new(language: Language) -> Self {
        Self { language }
    }
    pub const fn language(self) -> Language {
        self.language
    }

    pub(crate) fn format(self, id: &str, args: &[(&str, &str)]) -> String {
        resources::format(self.language, id, args)
    }

    /// Shared TX endpoint keys require a semantic descriptor, not a help ID.
    pub fn field_label(self, key: u16) -> Result<&'static str, Error> {
        if matches!(key, 0x34a..=0x34d) {
            return Err(Error::AmbiguousField { key });
        }
        self.key_label(key)
    }

    fn key_label(self, key: u16) -> Result<&'static str, Error> {
        resources::lookup(self.language, &format!("field.{key:x}"))
            .ok_or(Error::UnregisteredField { key })
    }

    pub fn setting(self, setting: &settings::Setting) -> Result<FieldText, Error> {
        Ok(FieldText {
            key: setting.key,
            name: setting.name.clone(),
            label: self.field_label(setting.key)?,
        })
    }

    pub fn setting_value(
        self,
        setting: &settings::Setting,
        index: usize,
        raw: i64,
    ) -> Result<ValueText, Error> {
        if index >= setting.count {
            return Err(Error::InvalidIndex);
        }
        self.setting(setting)?;
        let d = setting.domain;
        let in_domain = d.step > 0
            && raw >= d.min
            && raw <= d.max
            && (i128::from(raw) - i128::from(d.min)) % i128::from(d.step) == 0;
        let message = match (&setting.kind, in_domain) {
            (settings::Kind::Integer(_), true) => Message::Number(raw),
            (settings::Kind::Integer(_), false) => Message::OutOfRange(raw),
            (settings::Kind::Selector, true) => Message::Code(raw),
            (settings::Kind::Selector, false) => Message::UnknownCode(raw),
        };
        Ok(ValueText {
            raw,
            text: self.message(message),
            in_domain,
        })
    }

    pub fn choice(
        self,
        field: &settings::extended::ChoiceField,
        index: usize,
        raw: i64,
    ) -> Result<ValueText, Error> {
        if !field.accepts_index(index) {
            return Err(Error::InvalidIndex);
        }
        self.choice_label(field, index)?;
        if let Some(value) = field.fixed_value(index).map_err(|_| Error::InvalidIndex)? {
            return Ok(ValueText {
                raw,
                text: self.fixed_value(value),
                in_domain: raw == 0,
            });
        }
        let d = field.domain();
        let in_domain = raw >= d.min && raw <= d.max;
        Ok(ValueText {
            raw,
            text: self.message(if in_domain {
                Message::Code(raw)
            } else {
                Message::UnknownCode(raw)
            }),
            in_domain,
        })
    }

    /// Endpoints have full independent labels, not concatenated word fragments.
    pub fn choice_label(
        self,
        field: &settings::extended::ChoiceField,
        index: usize,
    ) -> Result<String, Error> {
        if !field.accepts_index(index) {
            return Err(Error::InvalidIndex);
        }
        if matches!(field.key(), 0x34a..=0x34d) {
            Ok(self.format(&format!("choice.{}", field.name()), &[]))
        } else {
            self.key_label(field.key()).map(str::to_owned)
        }
    }

    pub fn fixed_value(self, value: FixedValue) -> String {
        self.message(match value {
            FixedValue::AgcFast => Message::AgcFast,
            FixedValue::AgcDelay100Millis => Message::AgcDelay100Millis,
            FixedValue::NotApplicable => Message::NotApplicable,
        })
    }

    pub fn channel_label(
        self,
        field: crate::icf::record::ChannelField,
    ) -> Result<&'static str, Error> {
        use crate::icf::record::ChannelField::*;
        self.field_label(match field {
            FrequencyHz => 0x107,
            Duplex => 0x10b,
            Selection => 0x117,
            Tone => 0x10d,
            Dtcs => 0x110,
            Polarity => 0x111,
            DigitalSquelch => 0x112,
            DigitalCode => 0x113,
        })
    }

    pub fn callsign_label(self, field: crate::icf::MemoryCallsign) -> Result<&'static str, Error> {
        use crate::icf::MemoryCallsign::*;
        self.field_label(match field {
            Your => 0x114,
            Repeater1 => 0x115,
            Repeater2 => 0x116,
        })
    }

    pub fn repeater_label(
        self,
        field: crate::icf::record::RepeaterField,
    ) -> Result<&'static str, Error> {
        use crate::icf::record::RepeaterField::*;
        let key = match field {
            Mode => 0x146,
            Tone => 0x14d,
            ToneIndex => 0x14e,
            Use => 0x14f,
            Position => 0x150,
            UtcOffset => 0x153,
            GatewayIndex => {
                return Ok(
                    resources::lookup(self.language, "repeater.gateway_reference")
                        .expect("registered gateway reference"),
                );
            }
        };
        self.field_label(key)
    }

    pub fn network_label(self, field: settings::network::Field) -> Result<&'static str, Error> {
        use settings::network::Field::*;
        self.field_label(match field {
            Address => 0x319,
            Gateway => 0x31b,
            PrimaryDns => 0x31c,
            SecondaryDns => 0x31d,
        })
    }

    pub fn value(self, value: Value) -> String {
        use crate::icf::{
            Agc, FilterShape, RepeaterDuplex, RepeaterMode, RepeaterPosition, RepeaterTone,
        };
        let id = match value {
            Value::Boolean(value) => {
                return self.message(if value {
                    Message::Enabled
                } else {
                    Message::Disabled
                });
            }
            Value::Agc(Agc::Fast) => return self.message(Message::AgcFast),
            Value::Agc(Agc::Mid) => "value.agc_mid",
            Value::Agc(Agc::Slow) => "value.agc_slow",
            Value::FilterShape(FilterShape::Sharp) => "value.filter_shape_sharp",
            Value::FilterShape(FilterShape::Soft) => "value.filter_shape_soft",
            Value::RepeaterMode(RepeaterMode::Dv) => "value.repeater_mode_dv",
            Value::RepeaterMode(RepeaterMode::Fm) => "value.repeater_mode_fm",
            Value::RepeaterMode(RepeaterMode::FmNarrow) => "value.repeater_mode_fm_narrow",
            Value::RepeaterMode(RepeaterMode::Dd) => "value.repeater_mode_dd",
            Value::RepeaterDuplex(RepeaterDuplex::Off) => "value.repeater_duplex_off",
            Value::RepeaterDuplex(RepeaterDuplex::Minus) => "value.repeater_duplex_minus",
            Value::RepeaterDuplex(RepeaterDuplex::Plus) => "value.repeater_duplex_plus",
            Value::RepeaterDuplex(RepeaterDuplex::Rps) => {
                return self.format("value.repeater_duplex_rps", &[]);
            }
            Value::RepeaterTone(RepeaterTone::Off) => "value.repeater_tone_off",
            Value::RepeaterTone(RepeaterTone::Tone) => "value.repeater_tone_tone",
            Value::RepeaterTone(RepeaterTone::ToneSquelch) => "value.repeater_tone_tone_squelch",
            Value::RepeaterPosition(RepeaterPosition::None) => "value.repeater_position_none",
            Value::RepeaterPosition(RepeaterPosition::Approximate) => {
                "value.repeater_position_approximate"
            }
            Value::RepeaterPosition(RepeaterPosition::Exact) => "value.repeater_position_exact",
        };
        self.format(id, &[])
    }

    /// Only FF in the first byte denotes missing. The raw pair remains data.
    pub fn symbol(self, raw: &[u8]) -> Result<String, Error> {
        let &[first, second] = raw else {
            return Err(Error::InvalidSymbolWidth);
        };
        Ok(if first == 255 {
            self.message(Message::NoSymbol)
        } else {
            self.format(
                "symbol.aprs",
                &[
                    ("first", &format!("{first:02X}")),
                    ("second", &format!("{second:02X}")),
                ],
            )
        })
    }

    pub fn message(self, message: Message) -> String {
        use Message::*;
        let id = match message {
            Enabled => "message.enabled",
            Disabled => "message.disabled",
            NoSymbol => "message.no_symbol",
            NotApplicable => "message.not_applicable",
            AgcFast => "message.agc_fast",
            AgcDelay100Millis => "message.agc_delay_100_millis",
            ReadOnly => "message.read_only",
            Cancelled => "message.cancelled",
            NotPublished => "message.not_published",
            PartialNotPublished => "message.partial_not_published",
            PartialPublished => "message.partial_published",
            WriteAcknowledged => "message.write_acknowledged",
            ReadRadio => "message.read_radio",
            WriteRadio => "message.write_radio",
            SaveFile => "message.save_file",
            ReadFile => "message.read_file",
            Code(value) => return self.format("message.code", &[("value", &value.to_string())]),
            UnknownCode(value) => {
                return self.format("message.unknown_code", &[("value", &value.to_string())]);
            }
            Number(value) => {
                return self.format("message.number", &[("value", &value.to_string())]);
            }
            OutOfRange(value) => {
                return self.format("message.out_of_range", &[("value", &value.to_string())]);
            }
            RowsProcessed(count) => {
                return self.format(
                    if count == 1 || self.language == Language::French && count == 0 {
                        "message.rows_processed.one"
                    } else {
                        "message.rows_processed.other"
                    },
                    &[("count", &count.to_string())],
                );
            }
        };
        self.format(id, &[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_and_messages_are_explicit_and_complete() {
        for language in Language::ALL {
            assert_eq!(language.code().parse(), Ok(language));
            let p = Presenter::new(language);
            for message in [
                Message::Enabled,
                Message::Disabled,
                Message::NoSymbol,
                Message::NotApplicable,
                Message::AgcFast,
                Message::AgcDelay100Millis,
                Message::ReadOnly,
                Message::Cancelled,
                Message::NotPublished,
                Message::PartialNotPublished,
                Message::PartialPublished,
                Message::WriteAcknowledged,
                Message::ReadRadio,
                Message::WriteRadio,
                Message::SaveFile,
                Message::ReadFile,
                Message::Code(i64::MIN),
                Message::UnknownCode(i64::MAX),
                Message::Number(i64::MIN),
                Message::OutOfRange(i64::MAX),
                Message::RowsProcessed(u64::MAX),
            ] {
                assert!(!p.message(message).is_empty());
            }
            assert!(p.field_label(0xffff).is_err());
            assert!(p.symbol(&[1]).is_err());
            assert_eq!(p.symbol(b"/>").unwrap(), "APRS 2F3E");
            assert_eq!(p.symbol(&[255, 17]).unwrap(), p.message(Message::NoSymbol));
            for error in [
                Error::UnsupportedLanguage,
                Error::UnregisteredField { key: 0xffff },
                Error::InvalidIndex,
                Error::InvalidSymbolWidth,
                Error::AmbiguousField { key: 0x34a },
            ] {
                assert!(!error.localized(language).is_empty());
            }
        }
        for unsupported in ["", "ru", "ko", "en-US", "English", "JA"] {
            assert!(unsupported.parse::<Language>().is_err());
        }
        assert_eq!(
            Presenter::new(Language::Japanese).message(Message::Enabled),
            "\u{6709}\u{52b9}"
        );
        assert_eq!(
            Presenter::new(Language::English).message(Message::Enabled),
            "Enabled"
        );
    }

    #[test]
    fn every_setting_has_six_labels_without_feature_id_aliasing() {
        for p in Language::ALL.map(Presenter::new) {
            for setting in settings::catalog() {
                let label = p.setting(setting).unwrap();
                assert!(!label.label.is_empty(), "{}", setting.name);
                assert_eq!(label.name, setting.name);
                assert!(p.setting_value(setting, setting.count, 0).is_err());
                assert!(!p.setting_value(setting, 0, i64::MIN).unwrap().in_domain);
            }
            for field in settings::extended::choice_fields()
                .iter()
                .chain(settings::extended::rf_choice_fields())
            {
                for index in field.first_index..field.first_index + field.count {
                    if field.accepts_index(index) {
                        assert!(!p.choice_label(field, index).unwrap().is_empty());
                        p.choice(field, index, 0).unwrap();
                    }
                }
            }
            for field in settings::gps::choice_fields() {
                p.setting(&field.setting).unwrap();
            }
        }
        assert_ne!(
            Presenter::new(Language::English).field_label(0x32a),
            Presenter::new(Language::English).field_label(0x32d)
        );
    }

    #[test]
    fn typed_values_and_full_choice_labels_resolve_all_resources() {
        use crate::icf::{
            Agc, FilterShape, RepeaterDuplex, RepeaterMode, RepeaterPosition, RepeaterTone,
        };
        for p in Language::ALL.map(Presenter::new) {
            let mut values = vec![Value::Boolean(false), Value::Boolean(true)];
            values.extend([Agc::Fast, Agc::Mid, Agc::Slow].map(Value::Agc));
            values.extend([FilterShape::Sharp, FilterShape::Soft].map(Value::FilterShape));
            values.extend(
                [
                    RepeaterMode::Dv,
                    RepeaterMode::Fm,
                    RepeaterMode::FmNarrow,
                    RepeaterMode::Dd,
                ]
                .map(Value::RepeaterMode),
            );
            values.extend(
                [
                    RepeaterDuplex::Off,
                    RepeaterDuplex::Minus,
                    RepeaterDuplex::Plus,
                    RepeaterDuplex::Rps,
                ]
                .map(Value::RepeaterDuplex),
            );
            values.extend(
                [
                    RepeaterTone::Off,
                    RepeaterTone::Tone,
                    RepeaterTone::ToneSquelch,
                ]
                .map(Value::RepeaterTone),
            );
            values.extend(
                [
                    RepeaterPosition::None,
                    RepeaterPosition::Approximate,
                    RepeaterPosition::Exact,
                ]
                .map(Value::RepeaterPosition),
            );
            for value in values {
                assert!(!p.value(value).is_empty());
            }
            for count in [0, 1, 2, u64::MAX] {
                assert!(
                    p.message(Message::RowsProcessed(count))
                        .contains(&count.to_string())
                );
            }
            let fields = settings::extended::rf_choice_fields();
            let low = fields
                .iter()
                .find(|f| f.name() == "transmit_wide_low")
                .unwrap();
            let high = fields
                .iter()
                .find(|f| f.name() == "transmit_wide_high")
                .unwrap();
            assert_ne!(
                p.choice_label(low, 0).unwrap(),
                p.choice_label(high, 1).unwrap()
            );
            assert!(p.field_label(low.key()).is_err());
        }
    }

    #[test]
    fn public_editor_descriptors_have_labels_in_every_language() {
        use settings::{digital as d, extended as e, gps as g};
        let mut keys: Vec<_> = e::number_fields().iter().map(|f| f.key).collect();
        keys.extend(e::ColorField::ALL.map(e::ColorField::key));
        keys.extend(e::PackedField::ALL.map(e::PackedField::key));
        keys.extend(e::FixedTextField::ALL.map(e::FixedTextField::key));
        keys.extend(e::MetadataField::ALL.map(e::MetadataField::key));
        keys.extend(g::number_fields().iter().map(|f| f.setting.key));
        keys.extend(g::symbol_fields().iter().map(|f| f.key));
        keys.extend(g::coordinate_fields().iter().map(|f| f.key));
        keys.push(g::ALARM_AREA.key);
        keys.extend(g::text_fields().iter().map(|f| f.key));
        keys.extend(g::SentenceField::ALL.map(g::SentenceField::key));
        keys.extend(d::SelectionField::ALL.map(d::SelectionField::key));
        keys.extend(d::TextField::ALL.map(d::TextField::key));
        keys.sort_unstable();
        keys.dedup();
        let missing: Vec<_> = keys
            .iter()
            .filter(|&&key| Presenter::new(Language::English).field_label(key).is_err())
            .map(|key| format!("{key:x}"))
            .collect();
        assert!(
            missing.is_empty(),
            "untranslated descriptor keys: {missing:?}"
        );
        for p in Language::ALL.map(Presenter::new) {
            for &key in &keys {
                assert!(!p.field_label(key).unwrap().is_empty());
            }
        }
    }

    #[test]
    fn numeric_units_and_native_fixed_values_do_not_become_choice_codes() {
        let field = settings::find("lcd_backlight_raw").unwrap();
        for p in Language::ALL.map(Presenter::new) {
            let value = p.setting_value(field, 0, 17).unwrap();
            assert!(value.in_domain);
            assert_eq!(value.raw, 17);
            assert_eq!(value.text, "17");
            let value = p.setting_value(field, 0, -1).unwrap();
            assert!(!value.in_domain);
            assert_eq!(value.raw, -1);
            assert_eq!(value.text, p.message(Message::OutOfRange(-1)));
            for (value, source) in [
                (FixedValue::AgcFast, "FAST"),
                (FixedValue::AgcDelay100Millis, "0.1sec"),
                (FixedValue::NotApplicable, "---"),
            ] {
                assert_eq!(value.source_label(), source);
                assert!(!p.fixed_value(value).is_empty());
            }
        }
        let p = Presenter::new(Language::French);
        assert_eq!(
            p.message(Message::RowsProcessed(0)),
            p.format("message.rows_processed.one", &[("count", "0")])
        );
    }
}
