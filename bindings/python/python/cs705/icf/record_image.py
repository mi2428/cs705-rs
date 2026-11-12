"""Codecs use immutable bytes for reads and MutableBytes for image writes.

Stores preserve earlier commits and holder corrections if a writer raises.
Readonly ChannelWrite callback payloads expire when that callback returns.
"""
from .._native import icf as _icf

load_into = _icf.record_image.load_into
store_into = _icf.record_image.store_into
callsign_count = _icf.record_image.callsign_count
print_dataset_count = _icf.record_image.print_dataset_count
callsign_bad = _icf.record_image.callsign_bad
ChannelKind = _icf.record_image.ChannelKind
ChannelAddress = _icf.record_image.ChannelAddress
channel_address = _icf.record_image.channel_address
load_channel_into = _icf.record_image.load_channel_into
load_range_into = _icf.record_image.load_range_into
load_compact_into = _icf.record_image.load_compact_into
load_paired_into = _icf.record_image.load_paired_into
load_scan_into = _icf.record_image.load_scan_into
load_callsign_into = _icf.record_image.load_callsign_into
store_callsign_into = _icf.record_image.store_callsign_into
store_range_into = _icf.record_image.store_range_into
store_compact_into = _icf.record_image.store_compact_into
store_paired_into = _icf.record_image.store_paired_into
store_scan_into = _icf.record_image.store_scan_into
store_channel_into = _icf.record_image.store_channel_into
print_channel_row = _icf.record_image.print_channel_row
ChannelView = _icf.record_image.ChannelView
ChannelFilter = _icf.record_image.ChannelFilter
channel_bad_count = _icf.record_image.channel_bad_count
