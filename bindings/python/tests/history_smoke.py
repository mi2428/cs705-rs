"""Native history query order, live holder projections and failure prefixes."""
import traceback
import gc
import weakref
from cs705 import Error, icf, presentation as p
from cs705.icf import record as r

def raises(kind,fn):
    try: fn()
    except kind as error: return error
    raise AssertionError(f"expected {kind.__name__}")
assert tuple(p.Language)==p.Language.ALL
assert [v.code() for v in p.Language.ALL]==["en","ja","de","fr","es","it"]
assert r.HistoryLoad.Repeater(-1).index==-1
assert r.HistoryLoad.Ur(0xFFFFFFFF).index==0xFFFFFFFF
assert r.HistoryQuery.FromFm(b"RAW CALL",0x12345678,-1).limit==-1
for fn in (lambda:r.HistoryLoad.Ur(-1),lambda:r.HistoryQuery.FromDv(b"short"),lambda:r.HistoryValue.Unsigned(True),lambda:r.HistoryQuery.FromFm(b"RAW CALL",1,True)):
    raises((TypeError,ValueError,OverflowError),fn)
for variant,value in ((r.HistoryValue.Signed,-1),(r.HistoryValue.Unsigned,0xFFFFFFFF)):
    result=variant(value)
    assert (result.signed if result.variant=="Signed" else result.unsigned)==value
assert r.HistoryValue.Bytes(b"raw\0bytes").bytes.to_bytes()==b"raw\0bytes"
class BytesSubclass(bytes): pass
class Marker: pass
for keep_value in (True,False):
    source=BytesSubclass(b"gc edge")
    marker=Marker()
    reference=weakref.ref(marker)
    source.marker=marker
    value=r.HistoryValue.Bytes(source)
    source.edge=value if keep_value else value.bytes
    del source,marker,value
    gc.collect()
    assert reference() is None

holder=r.reset_holder(0x10,9,0xDEADBEEF)
seen=[]
loads=[]
saved=[]
sentinel=ValueError("native supplement load failed")
def query(value):
    seen.append(value)
    if len(seen)==1: assert value.variant=="RepeaterCount";return 2
    if len(seen)==2:
        assert (value.variant,value.call,value.frequency_hz,value.limit)==("FromFm",b"RAW CALL",0x12345678,2)
        return 3
    assert len(seen)==3 and value.variant=="RepeaterCount"
    return 4
def fail_load(value,live):
    assert (value.variant,value.index)==("Repeater",3)
    loads.append(value)
    assert live.history_supplemented() and live.as_bytes()[0x194]==1
    raises(RuntimeError,holder.as_bytes)
    live.set_name(b"LOAD PREFIX     ")
    saved.append(live)
    saved.append(dict(r.receive_history_fields(live))["caller"].bytes)
    assert len(saved[-1])==8
    raise sentinel
assert raises(ValueError,lambda:r.decode_history_into(holder,0x10,b"RAW CALL\x78\x56\x34\x12",query,fail_load)) is sentinel
assert len(seen)==3 and len(loads)==1 and holder.history_call()==b"RAW CALL"
assert holder.name()==b"LOAD PREFIX     " and holder.as_bytes()[0x194]==1
assert (holder.kind(),holder.index(),holder.binding())==(0x10,9,0xDEADBEEF)
assert any(frame.name=="fail_load" for frame in traceback.extract_tb(sentinel.__traceback__))
raises(ReferenceError,saved[0].as_bytes)
raises(ReferenceError,saved[1].to_bytes)

holder=r.reset_holder(0x12,0,77)
seen=[]
def ur_query(value):
    seen.append(value.variant)
    return 300 if value.variant=="Ur" else 299
def ur_load(value,live):
    assert (value.variant,value.index)==("Ur",299)
    assert live.history_supplemented()
r.decode_history_into(holder,0x12,b"RAW CALL",ur_query,ur_load)
assert seen==["Ur","UpperUr"]
seen.clear()
r.decode_history_into(holder,0x12,b"RAW CALL",lambda q:seen.append(q.variant) or 1,lambda q,h:None)
assert seen==["Ur"]
holder=r.reset_holder(0x0F,0,0)
seen=[]
r.decode_history_into(holder,0x0F,b"RAW CALL",lambda q:-1 if q.variant=="FromDv" else 0,lambda q,h:seen.append(q.index))
assert seen==[-1]
raises(TypeError,lambda:r.decode_history_into(holder,0x0F,b"RAW CALL",lambda q:True,lambda q,h:None))

rx=r.reset_holder(0x0E,0,0)
r.decode_receive_history_into(rx,b"A"*48,b"MESSAGE"+b" "*13,0,b"\0"*92)
view=r.receive_history_raw(rx,0x157)
fields=dict(r.receive_history_fields(rx))
assert isinstance(fields["caller"].bytes,r.HistoryBytes)
before=view.to_bytes()
r.decode_receive_history_into(rx,b"B"*48,b"changed"+b" "*13,0,b"\0"*92)
assert view.to_bytes()!=before and fields["caller"].bytes.to_bytes()==view.to_bytes()
assert not hasattr(view,"__setitem__")
memory=bytearray(0x3C9C0)
memory[0x355A1]=6
memory[-16:]=b"IcomCloneFormat3"
memory[0x3C331:0x3C359]=b"\xff"*40
memory[0x3C331]=0
memory[0x32935:0x3293D]=b"RAW CALL"
image=icf.Icf.from_memory(bytes(memory))
from cs705.repeater.working import Working
repeaters=Working.from_working(bytes(0x4997A))
ur=icf.rows.WorkingUr.from_icf(image)
assert r.resolve_history_query(repeaters,ur,r.HistoryQuery.RepeaterCount())==0
assert r.resolve_history_query(repeaters,ur,r.HistoryQuery.Ur(b"RAW CALL"))==300
assert r.history_selection_present(image,0x0F,0,0)
assert r.delete_history_storage(image,0x0F,0,0)
assert not r.history_selection_present(image,0x0F,0,0)
r.load_history_storage_into(image,0x0F,0,repeaters,ur,holder)
assert not holder.valid()
r.load_reflector_storage_into(image,0,holder)
r.load_receive_history_storage_into(image,0,rx)
print("history query/supplement/prefix/live-bytes smoke: passed")
