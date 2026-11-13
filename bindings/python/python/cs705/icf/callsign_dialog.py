"""Native callsign lookup sources and list-dialog operations."""
from .._native import icf as _icf

Sources = _icf.callsign_dialog.Sources
ReadInputs = _icf.callsign_dialog.ReadInputs
ListSelection = _icf.callsign_dialog.ListSelection
ListOutcome = _icf.callsign_dialog.ListOutcome
ListItem = _icf.callsign_dialog.ListItem
Category = _icf.callsign_dialog.Category
ListModel = _icf.callsign_dialog.ListModel
Action = _icf.callsign_dialog.Action
Dialog = _icf.callsign_dialog.Dialog
DialogOutcome = _icf.callsign_dialog.DialogOutcome
ByteRows = _icf.callsign_dialog.ByteRows
lookup_ur_name = _icf.callsign_dialog.lookup_ur_name
lookup_repeater_name = _icf.callsign_dialog.lookup_repeater_name
lookup_repeater = _icf.callsign_dialog.lookup_repeater
lookup_rpt2 = _icf.callsign_dialog.lookup_rpt2
slash_call = _icf.callsign_dialog.slash_call
list_model = _icf.callsign_dialog.list_model
list_into = _icf.callsign_dialog.list_into
initial_selection = _icf.callsign_dialog.initial_selection
prepare_metadata = _icf.callsign_dialog.prepare_metadata
apply_requests = _icf.callsign_dialog.apply_requests
