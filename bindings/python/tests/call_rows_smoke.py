"""Fixed type-3 pairs from the core fixture, using native codecs only."""
import importlib.util
import traceback
from cs705 import MutableBytes, Error, presentation, _native
from cs705.icf.context import Context
from cs705.icf.frequency import Ranges
from cs705.icf.normalizer import ReferenceImage
from cs705.icf.record import reset_holder, ChannelField, ChannelStage
from cs705.icf.record_image import ChannelKind, store_channel_into, load_channel_into
from cs705.icf.rows import native as n
from cs705.icf.rows.native import call as c

assert importlib.util.resolve_name("...._native",c.__package__) == "cs705._native"
assert c.Selection is _native.icf.rows.native.call.Selection
assert c.Selection is not n.Selection
assert c.Selection.__module__ == "cs705.icf.rows.native.call"
def raises(kind,fn):
    try: fn()
    except kind as error: return error
    raise AssertionError(f"expected {kind.__name__}")
def select(first,last=None,show=False):
    return c.Selection(first,first,first if last is None else last,False,show)
def fixture():
    raw=bytearray(b"\xa6"*0x4997A)
    raw[0x392FA:0x392FD]=b"\xff"*3
    raw[0x3C84B]=0
    image=MutableBytes(bytes(raw))
    for index in range(8):
        holder=reset_holder(3,index,1)
        holder.set_valid(True)
        holder.set_split(0)
        holder.set_name(b"TARGET NAME     ")
        holder.set_mode(13)
        holder.set_filter(2)
        holder.set_channel_field(ChannelField.FrequencyHz,145000000+index*1000)
        store_channel_into(image,ChannelKind.Call,index,holder,lambda write:None)
    return image
def holder(image,index):
    result=reset_holder(3,index,0)
    load_channel_into(image.to_bytes(),ChannelKind.Call,index,result)
    return result
def frequencies(image):
    return [holder(image,i).channel_field(ChannelField.FrequencyHz) for i in range(8)]
ranges=Ranges.generate(Context.from_bytes(b"\0"*24),None)
image=fixture()
clip=n.Clipboard.default()
effects=n.Effects.default()
c.copy(image,select(2),clip,effects)
assert (clip.mode,clip.declared_count,clip.logical_count,effects.selection)==(3,2,1,(2,3))
assert clip.source_kind == 3 and effects.context_flags == 4
partial=n.Clipboard.default()
partial_effects=n.Effects.default()
raises(Error,lambda:c.copy(MutableBytes(image.to_bytes()[:0xA21C+83]),select(0,2),partial,partial_effects))
assert (partial.mode,partial.declared_count,partial.logical_count,len(partial.records_snapshot()))==(3,4,1,2)
assert partial_effects.context_flags==0
source=fixture()
c.copy(source,select(4,show=True),clip,n.Effects.default())
assert clip.mode == 2 and clip.count(False)==1
confirmations=[]
before=image.to_bytes()
out=c.paste(image,select(0),clip,ranges,n.Effects.default(),lambda kind:confirmations.append(kind) or 0,lambda i,w:None)
assert not out.processed and image.to_bytes()==before
assert confirmations==[n.Confirmation.Overwrite]

sentinel=ValueError("fixed call store failed")
saved=[]
buffers=[]
writes=[]
effects=n.Effects.default()
selection=select(0)
def fail_store(index,write):
    selection.show_both=True  # Entry Copy, never a PyRef across this callback.
    saved.append(write)
    if write.bytes is not None: buffers.append(write.bytes)
    raises(RuntimeError,image.to_bytes)
    raises(RuntimeError,clip.records_snapshot)
    raises(RuntimeError,ranges.is_complete)
    if write.stage()==ChannelStage.Frequency:
        writes.append(index)
        if index==1: raise sentinel
assert raises(ValueError,lambda:c.paste(image,selection,clip,ranges,effects,lambda _:1,fail_store)) is sentinel
assert writes==[0,1]
assert frequencies(image)[:2]==[145004000,145001000]
assert not effects.notifications_snapshot()
assert any(frame.name=="fail_store" for frame in traceback.extract_tb(sentinel.__traceback__))
raises(ReferenceError,lambda:saved[0].variant)
raises(ReferenceError,buffers[0].to_bytes)
selection=select(0)
assert raises(ValueError,lambda:c.paste(image,selection,clip,ranges,effects,lambda _:(_ for _ in ()).throw(sentinel),lambda i,w:None)) is sentinel
raises(TypeError,lambda:c.paste(image,selection,clip,ranges,effects,lambda _:True,lambda i,w:None))
out=c.paste(image,select(0),clip,ranges,n.Effects.default(),lambda _:1,lambda i,w:None)
assert out.written==2 and out.native_count==2
image=fixture()
out=c.carry(image,select(2,3),-1,ranges,n.Effects.default(),lambda i,w:None)
assert out.processed and out.written==4
assert frequencies(image)==[145002000,145003000,145000000,145001000,145004000,145005000,145006000,145007000]
image=fixture()
before=image.to_bytes()
assert not c.carry(image,select(0),-1,ranges,n.Effects.default(),lambda i,w:None).processed
assert image.to_bytes()==before
generation=fixture()
image=fixture()
edited=holder(image,2)
edited.set_name(b"CHANGED NAME    ")
store_channel_into(image,ChannelKind.Call,2,edited,lambda w:None)
effects=n.Effects.default()
out=c.reset_pair(image,select(2),ReferenceImage.Separate(generation.to_bytes()),1,effects,lambda i,w:None)
assert out.written==2 and image.to_bytes()==generation.to_bytes()
assert [notice.id for notice in effects.notifications_snapshot()]==[0x14,0x14,0x11,0x3C]
assert effects.display_mode==2
assert c.reset_pair(image,select(0,3),ReferenceImage.Working,1,n.Effects.default(),lambda i,w:None).written==4
assert not c.reset_pair(image,select(8),ReferenceImage.Separate(b""),0,n.Effects.default(),lambda i,w:None).processed
for fn in (lambda:c.Selection(True,0,0,False,False),lambda:c.Selection(0,0,0,1,False),lambda:c.Selection(-1,0,0,False,False),lambda:c.copy(image,n.Selection(0,0,0,0,False,False),clip,effects)):
    raises((TypeError,OverflowError),fn)
selection=select(0)
for name,value in (("active",1),("first",1),("last",2),("merged",True),("show_both",True)):
    setattr(selection,name,value)
    assert getattr(selection,name)==value
raises(TypeError,lambda:setattr(selection,"show_both",1))
raises(TypeError,lambda:c.carry(image,select(0),True,ranges,effects,lambda i,w:None))
raises(TypeError,lambda:c.reset_pair(image,select(0),ReferenceImage.Working,1,effects,lambda i,w:True))
assert presentation.ErrorData is _native.presentation.ErrorData
error=presentation.ErrorData.AmbiguousField(0x34A)
assert error.key==0x34A and error.with_key(0x34B).key==0x34B
assert error.localized(presentation.Language.English)
assert error.exception().native == error
print("fixed call copy/paste/carry/reset/identity/failure smoke: passed")
