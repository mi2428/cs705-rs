//! Typed user summaries with separate, lossless developer details.
//!
//! Localized text belongs to the independent presentation resources. Native
//! technical reasons remain explicit developer data: summaries neither parse
//! their English text nor silently append it to a translated user message.

use crate::{
    clone, csv, gps, icf,
    presentation::{Language, Presenter},
    repeater,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    Icf,
    Csv,
    GpsCsv,
    RepeaterCsv,
    Clone,
    Presentation,
    Io,
}

impl Domain {
    const fn message_id(self) -> &'static str {
        match self {
            Self::Icf => "domain.icf",
            Self::Csv => "domain.csv",
            Self::GpsCsv => "domain.gps_csv",
            Self::RepeaterCsv => "domain.repeater_csv",
            Self::Clone => "domain.clone",
            Self::Presentation => "domain.presentation",
            Self::Io => "domain.io",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Invalid,
    Unsupported,
    Limit,
    Conversion,
    SourceUnavailable,
    Operation,
    ChecksumMismatch,
    RowIndex,
    UnsupportedCommand,
    AddressOverflow,
    SourceBounds,
    Rejected,
    UnsupportedLanguage,
    UnregisteredField,
    InvalidIndex,
    InvalidSymbolWidth,
    AmbiguousField,
}

impl Kind {
    const fn message_id(self) -> &'static str {
        match self {
            Self::Invalid => "diagnostic.invalid",
            Self::Unsupported => "diagnostic.unsupported",
            Self::Limit => "diagnostic.limit",
            Self::Conversion => "diagnostic.conversion",
            Self::SourceUnavailable => "diagnostic.source_unavailable",
            Self::Operation => "diagnostic.operation",
            Self::ChecksumMismatch => "diagnostic.checksum_mismatch",
            Self::RowIndex => "diagnostic.row_index",
            Self::UnsupportedCommand => "diagnostic.unsupported_command",
            Self::AddressOverflow => "diagnostic.address_overflow",
            Self::SourceBounds => "diagnostic.source_bounds",
            Self::Rejected => "diagnostic.rejected",
            Self::UnsupportedLanguage => "diagnostic.unsupported_language",
            Self::UnregisteredField => "diagnostic.unregistered_field",
            Self::InvalidIndex => "diagnostic.invalid_index",
            Self::InvalidSymbolWidth => "diagnostic.invalid_symbol_width",
            Self::AmbiguousField => "diagnostic.ambiguous_field",
        }
    }
}

/// User classification and locations, retaining the original technical reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diagnostic<'a> {
    pub domain: Domain,
    pub kind: Kind,
    pub line: Option<usize>,
    pub index: Option<usize>,
    pub value: Option<u32>,
    technical_detail: Option<&'a str>,
}

impl<'a> Diagnostic<'a> {
    const fn new(domain: Domain, kind: Kind) -> Self {
        Self {
            domain,
            kind,
            line: None,
            index: None,
            value: None,
            technical_detail: None,
        }
    }

    /// Original developer detail, deliberately not represented as translated.
    pub const fn technical_detail(self) -> Option<&'a str> {
        self.technical_detail
    }

    pub fn summary(self, language: Language) -> String {
        let presenter = Presenter::new(language);
        let domain = presenter.format(self.domain.message_id(), &[]);
        let message = presenter.format(self.kind.message_id(), &[]);
        let line = self
            .line
            .map(|line| presenter.format("diagnostic.line", &[("line", &line.to_string())]))
            .unwrap_or_default();
        let index = self
            .index
            .map(|index| presenter.format("diagnostic.index", &[("index", &index.to_string())]))
            .unwrap_or_default();
        let value = self
            .value
            .map(|value| presenter.format("diagnostic.value", &[("value", &format!("{value:X}"))]))
            .unwrap_or_default();
        presenter.format(
            "diagnostic.summary",
            &[
                ("domain", &domain),
                ("message", &message),
                ("line", &line),
                ("index", &index),
                ("value", &value),
            ],
        )
    }
}

impl<'a> From<&'a icf::Error> for Diagnostic<'a> {
    fn from(error: &'a icf::Error) -> Self {
        let (kind, line, detail) = match error {
            icf::Error::Invalid { line, reason } => (Kind::Invalid, *line, Some(*reason)),
            icf::Error::Unsupported { line, reason } => (Kind::Unsupported, *line, Some(*reason)),
            icf::Error::ChecksumMismatch { line } => (Kind::ChecksumMismatch, *line, None),
        };
        Self {
            line: (line != 0).then_some(line),
            technical_detail: detail,
            ..Self::new(Domain::Icf, kind)
        }
    }
}

impl<'a> From<&'a csv::Error> for Diagnostic<'a> {
    fn from(error: &'a csv::Error) -> Self {
        let (kind, detail) = match error {
            csv::Error::Limit(s) => (Kind::Limit, *s),
            csv::Error::Invalid(s) => (Kind::Invalid, *s),
            csv::Error::Conversion(s) => (Kind::Conversion, *s),
            csv::Error::SourceUnavailable(s) => (Kind::SourceUnavailable, *s),
            csv::Error::Operation(s) => (Kind::Operation, *s),
        };
        Self {
            technical_detail: Some(detail),
            ..Self::new(Domain::Csv, kind)
        }
    }
}

macro_rules! csv_error {
    ($error:path, $domain:expr) => {
        impl<'a> From<&'a $error> for Diagnostic<'a> {
            fn from(error: &'a $error) -> Self {
                use $error as E;
                match error {
                    E::Invalid { line, reason } => Self {
                        line: (*line != 0).then_some(*line),
                        technical_detail: Some(reason),
                        ..Self::new($domain, Kind::Invalid)
                    },
                    E::Unsupported { line, reason } => Self {
                        line: (*line != 0).then_some(*line),
                        technical_detail: Some(reason),
                        ..Self::new($domain, Kind::Unsupported)
                    },
                    E::RowIndexOutOfBounds { index } => Self {
                        index: Some(*index),
                        ..Self::new($domain, Kind::RowIndex)
                    },
                }
            }
        }
    };
}
csv_error!(gps::Error, Domain::GpsCsv);
csv_error!(repeater::Error, Domain::RepeaterCsv);

impl<'a> From<&'a clone::Error> for Diagnostic<'a> {
    fn from(error: &'a clone::Error) -> Self {
        let mut diagnostic = Self::new(Domain::Clone, Kind::Invalid);
        match error {
            clone::Error::Invalid(reason) => diagnostic.technical_detail = Some(reason),
            clone::Error::UnsupportedCommand(value) => {
                diagnostic.kind = Kind::UnsupportedCommand;
                diagnostic.value = Some(u32::from(*value));
            }
            clone::Error::ChecksumMismatch => diagnostic.kind = Kind::ChecksumMismatch,
            clone::Error::AddressOverflow => diagnostic.kind = Kind::AddressOverflow,
            clone::Error::SourceBounds => diagnostic.kind = Kind::SourceBounds,
            clone::Error::Rejected(value) => {
                diagnostic.kind = Kind::Rejected;
                diagnostic.value = Some(u32::from(*value));
            }
        }
        diagnostic
    }
}

impl<'a> From<&'a crate::presentation::Error> for Diagnostic<'a> {
    fn from(error: &'a crate::presentation::Error) -> Self {
        use crate::presentation::Error as E;
        let mut diagnostic = Self::new(Domain::Presentation, Kind::Invalid);
        diagnostic.kind = match error {
            E::UnsupportedLanguage => Kind::UnsupportedLanguage,
            E::UnregisteredField { key } => {
                diagnostic.value = Some(u32::from(*key));
                Kind::UnregisteredField
            }
            E::AmbiguousField { key } => {
                diagnostic.value = Some(u32::from(*key));
                Kind::AmbiguousField
            }
            E::InvalidIndex => Kind::InvalidIndex,
            E::InvalidSymbolWidth => Kind::InvalidSymbolWidth,
        };
        diagnostic
    }
}

impl Presenter {
    /// A localized summary; inspect technical_detail explicitly for developer data.
    pub fn diagnostic(self, diagnostic: Diagnostic<'_>) -> String {
        diagnostic.summary(self.language())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn summaries_preserve_locations_and_do_not_mix_english_details() {
        let error = icf::Error::Invalid {
            line: 37,
            reason: "original technical invariant",
        };
        let diagnostic = Diagnostic::from(&error);
        assert_eq!(
            diagnostic.technical_detail(),
            Some("original technical invariant")
        );
        assert_eq!(diagnostic.line, Some(37));
        for language in Language::ALL {
            let text = diagnostic.summary(language);
            assert!(text.contains("37"));
            assert!(!text.contains("original technical invariant"));
            for kind in [
                Kind::Invalid,
                Kind::Unsupported,
                Kind::Limit,
                Kind::Conversion,
                Kind::SourceUnavailable,
                Kind::Operation,
                Kind::ChecksumMismatch,
                Kind::RowIndex,
                Kind::UnsupportedCommand,
                Kind::AddressOverflow,
                Kind::SourceBounds,
                Kind::Rejected,
                Kind::UnsupportedLanguage,
                Kind::UnregisteredField,
                Kind::InvalidIndex,
                Kind::InvalidSymbolWidth,
                Kind::AmbiguousField,
            ] {
                for domain in [
                    Domain::Icf,
                    Domain::Csv,
                    Domain::GpsCsv,
                    Domain::RepeaterCsv,
                    Domain::Clone,
                    Domain::Presentation,
                    Domain::Io,
                ] {
                    assert!(!Diagnostic::new(domain, kind).summary(language).is_empty());
                }
            }
        }
        let zero = icf::Error::Invalid {
            line: 0,
            reason: "not a line location",
        };
        assert_eq!(Diagnostic::from(&zero).line, None);
        let row = gps::Error::RowIndexOutOfBounds { index: 0 };
        assert_eq!(Diagnostic::from(&row).index, Some(0));
        let rejected = clone::Error::Rejected(255);
        let diagnostic = Diagnostic::from(&rejected);
        assert_eq!(diagnostic.value, Some(255));
        assert!(diagnostic.summary(Language::Japanese).contains("0xFF"));
    }
}
