"""Explicit language selection and independent native presentation resources."""
from ._native import presentation as _p
from ._native import Error
ErrorData = _p.ErrorData
FieldText = _p.FieldText
FixedValue = _p.FixedValue
Language = _p.Language
Message = _p.Message
Presenter = _p.Presenter
Value = _p.Value
ValueText = _p.ValueText
ChannelField = _p.ChannelField
RepeaterField = _p.RepeaterField
language_code = _p.language_code
Language.ALL = (Language.English, Language.Japanese, Language.German,
                Language.French, Language.Spanish, Language.Italian)
def _language_code(self):
    return language_code(self)
Language.code = _language_code
parse_language = _p.parse_language
localized_error = _p.localized_error
fixed_value_source_label = _p.fixed_value_source_label
__all__ = ["Error", "ErrorData", "FieldText", "FixedValue", "Language", "Message", "Presenter",
           "Value", "ValueText", "ChannelField", "RepeaterField", "language_code", "parse_language", "localized_error", "fixed_value_source_label"]
