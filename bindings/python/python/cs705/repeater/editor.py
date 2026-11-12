"""Native repeater field invocations and explicit NLS sources."""
from collections.abc import Callable
from cs705.csv.nls import Bytes
from .._native import repeater as _repeater

Runtime = _repeater.editor.Runtime
Effect = _repeater.editor.Effect
Nls = _repeater.editor.Nls
ChoiceField = _repeater.editor.ChoiceField
TextField = _repeater.editor.TextField
NumberField = _repeater.editor.NumberField
TYPE_SOURCE = _repeater.editor.TYPE_SOURCE
Map = Callable[[bytes, Bytes], int]
begin_type = _repeater.editor.begin_type
group_name = _repeater.editor.group_name
choice = _repeater.editor.choice
number = _repeater.editor.number
text = _repeater.editor.text
coordinate = _repeater.editor.coordinate
coordinate_input = _repeater.editor.coordinate_input
