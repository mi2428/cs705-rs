"""Native field selection and paint gates."""
from .._native import repeater as _repeater

TypeSource = _repeater.fields.TypeSource
Selection = _repeater.fields.Selection
Effects = _repeater.fields.Effects
PaintMeta = _repeater.fields.PaintMeta
Field = _repeater.fields.Field
Gate = _repeater.fields.Gate
Outcome = _repeater.fields.Outcome
transform_type = _repeater.fields.transform_type
candidate_allowed = _repeater.fields.candidate_allowed
quantize_offset_hz = _repeater.fields.quantize_offset_hz
suppress_mode_code = _repeater.fields.suppress_mode_code
paint_meta = _repeater.fields.paint_meta
select_type = _repeater.fields.select_type
select = _repeater.fields.select
select_offset = _repeater.fields.select_offset
select_utc_offset = _repeater.fields.select_utc_offset
