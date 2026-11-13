"""Pure native search contracts over explicit observations."""
from .._native import clone as _clone

EXPORTS = _clone.search.EXPORTS
RECORD_BYTES = _clone.search.RECORD_BYTES
PORTS_GUID = _clone.search.PORTS_GUID
USB_GUID = _clone.search.USB_GUID
PORTS_FLAGS = _clone.search.PORTS_FLAGS
USB_FLAGS = _clone.search.USB_FLAGS
Filter = _clone.search.Filter
Port = _clone.search.Port
Slot = _clone.search.Slot
SearchRecord = _clone.search.SearchRecord
Observation = _clone.search.Observation
SearchContext = _clone.search.SearchContext
Candidate = _clone.search.Candidate
manual_number = _clone.search.manual_number
manual_commit = _clone.search.manual_commit
label_number = _clone.search.label_number
mi_index = _clone.search.mi_index
com_matches = _clone.search.com_matches
com_only = _clone.search.com_only
device_matches = _clone.search.device_matches
marker = _clone.search.marker
instance_prefix = _clone.search.instance_prefix
node_key = _clone.search.node_key
join_ports = _clone.search.join_ports
header_query = _clone.search.header_query
candidates = _clone.search.candidates

__all__ = ["EXPORTS", "RECORD_BYTES", "PORTS_GUID", "USB_GUID", "PORTS_FLAGS", "USB_FLAGS", "Filter", "Port", "Slot", "SearchRecord", "Observation", "SearchContext", "Candidate", "manual_number", "manual_commit", "label_number", "mi_index", "com_matches", "com_only", "device_matches", "marker", "instance_prefix", "node_key", "join_ports", "header_query", "candidates"]
