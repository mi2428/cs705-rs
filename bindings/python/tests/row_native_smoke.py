"""Run with the interpreter containing the proper maturin-built wheel."""
import threading
import traceback

from cs705 import MutableBytes
from cs705.icf import Icf, MemoryCallsign
from cs705.icf.context import Context
from cs705.icf.frequency import Ranges
from cs705.icf.normalizer import ReferenceImage
from cs705.icf.record import ChannelField, ChannelStage, reset_holder
from cs705.icf.record_image import ChannelKind, store_channel_into
from cs705.icf.rows import NormalMemory, WorkingUr, native as n, ur
from cs705.icf.settings.extended import ChoiceInput, FixedTextInput
from cs705.icf.callsign_dialog import Sources, ReadInputs


def raises(kind, operation):
    try:
        operation()
    except kind as error:
        return error
    raise AssertionError(f"expected {kind.__name__}")


class Marker(Exception):
    pass


local = threading.local()
local.token = object()
token = local.token
q = Context.from_bytes(b"\0" * 24)
ranges = Ranges.generate(q, None)
context = n.EditorContext(ranges, ReferenceImage.Working)
memory = bytearray(b"\xa6" * 0x4997A)
memory[0x355A1] = 6
memory[0x3C9B0:0x3C9C0] = b"IcomCloneFormat3"
memory[0x3FB40:0x3FB44] = (2).to_bytes(4, "little")
memory[0x3FB44:0x3FCD4] = b"\0" * (0x3FCD4 - 0x3FB44)
memory[0x3FCD4:0x49914] = (65535).to_bytes(4, "little") * 10000
memory[0x3662D:0x3662D + 500] = b"\1" * 500
memory[0x392FA:0x392FD] = b"\xff" * 3
model = NormalMemory.from_working(bytes(memory))
assert model.allocate(0, range(0, 1)).allocated == 1
holder = reset_holder(2, 0, 19)
holder.set_valid(True)
holder.set_mode(0)
holder.set_filter(2)
holder.set_channel_field(ChannelField.FrequencyHz, 14100000)
model.store_channel(0, holder)
state = n.EditorState.initialized()
effects = n.Effects.default()
holder.set_name(b"PREFIX          ")
before = model.working()
views, buffers, frequency_stores = [], [], []
failure = Marker("second cached store failed")


def before_store(index, write):
    assert local.token is token and index == 0
    raises(RuntimeError, model.working)
    views.append(write)
    if write.bytes is not None:
        buffer = write.bytes
        buffers.append(buffer)
        raises(RuntimeError, lambda: buffer.__setitem__(0, 0))
    if write.stage() == ChannelStage.Frequency:
        frequency_stores.append(index)
        if len(frequency_stores) == 2:
            raise failure


assert raises(Marker, lambda: n.native_save_cached(model, holder, state, n.CacheWrite(None, 0x105), context, effects, before_store)) is failure
assert frequency_stores == [0, 0] and model.working() != before
assert holder.name() == b"PREFIX          "
for operation in (lambda: views[-1].variant, views[-1].stage, lambda: views[-1].offset, buffers[-1].to_bytes):
    raises(ReferenceError, operation)
assert any(frame.name == "before_store" for frame in traceback.extract_tb(failure.__traceback__))
raises(TypeError, lambda: n.native_save_cached(model, holder, state, n.CacheWrite(None, 0x105), context, effects, lambda i, w: True))
raises(TypeError, lambda: n.Selection(True, 0, 0, 0, False, False))
raises(TypeError, lambda: n.ChannelEntry.Redirect(True, 0, 0))
selection = n.Selection(0, 0, 0, 0, False, False)
clipboard = n.Clipboard.default()
n.native_copy(model, selection, clipboard, effects)
clipboard.validate()
assert clipboard.logical_count > 0
outcome = n.native_choice_input(model, holder, state, n.ChoiceRequest(ChoiceInput.Next, context, None, n.ChannelChoice.Duplex), effects, lambda i, w: None)
assert outcome.stored

# Native split supplies a live frame and a readonly notice, both with a deadline.
holder.set_split(0)
split_frame = n.SplitFrame(0, 1, 0, 0, 0, 0, 0, 0)
notices = []
def notice(write, frame):
    notices.append((write, frame))
    frame.notify_flags |= 0x80
    raises(RuntimeError, lambda: setattr(write, "id", 0))

assert n.native_split_input(model, holder, state, n.SplitRequest(ChoiceInput.Next, context, split_frame), effects, lambda i, w: None, notice).stored
assert split_frame.notify_flags & 0xA0 == 0xA0
raises(ReferenceError, lambda: notices[0][0].id)
raises(ReferenceError, lambda: notices[0][1].active)

# Modal Sources and ReadInputs are captured before native work; no PyRef survives.
raw = MutableBytes(b"\0" * 0x4997A)
call_holder = reset_holder(3, 0, 7)
call_holder.set_valid(True)
call_holder.set_mode(16)
call_holder.set_channel_field(ChannelField.FrequencyHz, 145000000)
call_holder.set_callsign(MemoryCallsign.Your, b"CQCQCQ  ")
store_channel_into(raw, ChannelKind.Call, 0, call_holder, lambda w: None)
sources = Sources(raw.to_bytes(), b"", ranges, None)
reads = ReadInputs(b" " * 8, 2500, None, b"\0" * 16)
dialogs = []
def actions(dialog):
    sources.system_langid = None
    reads.string_tail = b"\0" * 16
    dialogs.append(dialog)
    dialog.checkbox = True

modal = n.CallsignModalRequest(n.CallsignModal.Dr(2), sources, reads, context, MemoryCallsign.Your)
assert n.channel_callsign_modal_input_into(raw, call_holder, n.EditorState.initialized(), modal, n.Effects.default(), actions, lambda i, w: None).stored
raises(ReferenceError, lambda: dialogs[0].checkbox)

# UR inserts before a failing map and sanitizes the caller's real text buffer.
stored = bytearray(0x3C9C0)
stored[0x355A1] = 6
stored[0x3C9B0:] = b"IcomCloneFormat3"
stored[0x3C0A7:0x3C0A7 + 600] = b"\xff" * 600
stored[0x3C0A7:0x3C0A9] = b"\0\0"
working = WorkingUr.from_icf(Icf.from_memory(bytes(stored)))
source = MutableBytes(b"x\0y")
request = ur.Request(1, ur.Field.Call, FixedTextInput.Text(source, 1), 0, ReferenceImage.Working)
frame = ur.Frame(13, 0, False, 0, 1, 0xA7, 0x80, 0x20, False)
effect = ur.Effect.default()
destinations = []
map_failure = Marker("UR map failed")
def mapping(src, dst):
    assert local.token is token and src == b"x y   "
    raises(RuntimeError, source.to_bytes)
    destinations.append(dst)
    dst[0] = 65
    raise map_failure

def unreachable_upper(dst):
    raise AssertionError("uppercase ran after map failure")

assert raises(Marker, lambda: ur.native_text_with_nls(working, request, frame, effect, mapping, unreachable_upper)) is map_failure
assert working.count() == 2 and source.to_bytes() == b"x y"
assert effect.inserted and not effect.stored and effect.notifications_snapshot() == [0x40, 0x39]
assert frame.node_flags == 0xA4
raises(ReferenceError, destinations[0].to_bytes)
for module in (n, ur):
    assert all(hasattr(module, name) for name in module.__all__)
print("row native/UR smoke OK")
