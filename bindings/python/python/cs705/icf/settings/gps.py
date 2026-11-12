"""Native GPS settings with explicit resources and shared editor inputs."""
from ..._native import icf as _icf

_gps = _icf.settings.gps
Resources = _gps.Resources
CoordinateEffect = _gps.CoordinateEffect
SentenceEffect = _gps.SentenceEffect
SentenceEvent = _gps.SentenceEvent
SentenceDialog = _gps.SentenceDialog
NativeSymbolPairs = _gps.NativeSymbolPairs
ChoiceField = _gps.ChoiceField
NumberField = _gps.NumberField
SymbolField = _gps.SymbolField
CoordinateField = _gps.CoordinateField
TextField = _gps.TextField
ALARM_AREA = _gps.ALARM_AREA
choice_fields = _gps.choice_fields
find_choice = _gps.find_choice
number_fields = _gps.number_fields
symbol_fields = _gps.symbol_fields
find_symbol = _gps.find_symbol
coordinate_fields = _gps.coordinate_fields
find_coordinate = _gps.find_coordinate
text_fields = _gps.text_fields
find_text = _gps.find_text
AlarmField = _gps.AlarmField
SentenceField = _gps.SentenceField

def _alarm_native_name(self):
    return _gps.alarm_native_name(self)
def _alarm_feature_id(self):
    return _gps.alarm_feature_id(self)
def _alarm_key(self):
    return _gps.alarm_key(self)
def _alarm_read(self, image):
    return _gps.alarm_read(self, image)
def _alarm_choices(self, image):
    return _gps.alarm_choices(self, image)
def _alarm_edit(self, image, input):
    return _gps.alarm_edit(self, image, input)
AlarmField.native_name = _alarm_native_name
AlarmField.feature_id = _alarm_feature_id
AlarmField.key = _alarm_key
AlarmField.read = _alarm_read
AlarmField.choices = _alarm_choices
AlarmField.edit = _alarm_edit
AlarmField.ALL = tuple(AlarmField)

def _sentence_native_name(self):
    return _gps.sentence_native_name(self)
def _sentence_feature_id(self):
    return _gps.sentence_feature_id(self)
def _sentence_key(self):
    return _gps.sentence_key(self)
def _sentence_count(self):
    return _gps.sentence_count(self)
def _sentence_paint(self, image, provider):
    return _gps.sentence_paint(self, image, provider)
SentenceField.native_name = _sentence_native_name
SentenceField.feature_id = _sentence_feature_id
SentenceField.key = _sentence_key
SentenceField.count = _sentence_count
SentenceField.paint = _sentence_paint
SentenceField.ALL = tuple(SentenceField)

find_sentence = _gps.find_sentence
paint_alarm_group = _gps.paint_alarm_group
gps_group_count_source = _gps.gps_group_count_source
paint_altitude_raw = _gps.paint_altitude_raw
edit_altitude_raw = _gps.edit_altitude_raw
edit_altitude_live = _gps.edit_altitude_live

__all__ = ["Resources", "ChoiceField", "choice_fields", "find_choice", "AlarmField",
           "paint_alarm_group", "gps_group_count_source", "NumberField", "number_fields",
           "paint_altitude_raw", "edit_altitude_raw", "edit_altitude_live", "NativeSymbolPairs",
           "SymbolField", "symbol_fields", "find_symbol", "CoordinateField", "ALARM_AREA",
           "coordinate_fields", "find_coordinate", "CoordinateEffect", "TextField",
           "text_fields", "find_text", "SentenceField", "find_sentence", "SentenceEvent",
           "SentenceDialog", "SentenceEffect"]
