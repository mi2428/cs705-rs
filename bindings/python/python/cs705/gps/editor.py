"""Native GPS row loader, saver and alarm choices."""
from .._native import gps as _gps

RowAlarmChoice = _gps.editor.RowAlarmChoice
AlarmView = _gps.editor.AlarmView
RowAlarmEffect = _gps.editor.RowAlarmEffect
GroupNameEffect = _gps.editor.GroupNameEffect
row_alarm = _gps.editor.row_alarm
altitude = _gps.editor.altitude
group_name = _gps.editor.group_name
load = _gps.editor.load
save = _gps.editor.save
