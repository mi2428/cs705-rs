"""Synthetic sources and headless callbacks; no original assets or OS delivery."""
import io
import struct
from cs705 import printing as p, display as d, icf
assert p.Resources is d.Resources is p.native.Resources
assert p.Views is d.Views and p.ViewState is d.ViewState and p.ViewKind is d.ViewKind
assert p.Grid is not d.Grid
assert p.axis_spans([10]*4,1,[False]*4,35) == b"\0\2\0\1"
m = p.Metrics(-12,12,6,12,1)
paper = p.Paper(200,20)
sentinel = ValueError("callback failure")
class FailingReader:
    calls = 0
    def read(self, size):
        self.calls += 1
        raise interrupted
    def seek(self, *args): raise AssertionError("read failure must stop")
    def close(self): raise AssertionError("input must not be closed")
interrupted = InterruptedError("Python reader exception is terminal")
failing = FailingReader()
try:
    p.native.read_source(failing)
except InterruptedError as error:
    assert error is interrupted and failing.calls == 1
else:
    raise AssertionError("reader exception lost")
event = p.Event.Tick(50, "initial")
event.due_ms, event.name = 100, "changed"
assert (event.due_ms,event.name) == (100,"changed")
try:
    event.due_ms = True
except TypeError:
    assert event.due_ms == 100
else:
    raise AssertionError("bool accepted as event time")
editor = p.Editor(99,True,b"text\0raw")
saved = []
def fail_editor(text):
    saved.append(text)
    assert text.to_bytes() == b"text\0raw"
    raise sentinel
try:
    p.sync_editor(editor,fail_editor)
except ValueError as error:
    assert error is sentinel
else:
    raise AssertionError("editor error lost")
assert editor.validation_guard == 0 and editor.visible
try:
    saved[0].to_bytes()
except ReferenceError:
    pass
else:
    raise AssertionError("editor view survived")
assert not p.eligible(1,p.PredicateOrder.BaseFirst,lambda: (_ for _ in ()).throw(sentinel))
try:
    p.eligible(1,p.PredicateOrder.DatasetFirst,lambda: (_ for _ in ()).throw(sentinel))
except ValueError as error:
    assert error is sentinel

# Synthetic PE with three registered vectors; the rows contain no vendor data.
asset = bytearray(0x463400)
def put(at,value): struct.pack_into("<I",asset,at,value)
asset[:2] = b"MZ"
put(0x3C,0x80)
asset[0x80:0x84] = b"PE\0\0"
struct.pack_into("<HH",asset,0x84,0x14C,1)
struct.pack_into("<H",asset,0x94,0xE0)
struct.pack_into("<H",asset,0x98,0x10B)
put(0xB4,0x400000)
for offset,value in [(8,0x2D4600),(12,0x190000),(16,0x2D4600),(20,0x18EE00)]: put(0x178+offset,value)
pool = 0x440000
def string(value):
    global pool
    pointer = 0x400000+pool
    at=pool-0x1200
    asset[at:at+len(value)+1]=value+b"\0"
    pool += len(value)+1
    return pointer
empty = string(b"")
end = string(b"#End_Of_Ovl")
for rva,count in [(0x301918,6084),(0x307828,6084),(0x30D738,6088)]:
    for index in range(count): put(rva-0x1200+index*4,empty)
    put(rva-0x1200+(count-1)*4,end)
for rva,count in [(0x3F2AE0,0x19),(0x3F2B44,0x3AE)]:
    for index in range(count): put(rva-0x1200+index*4,empty)
key_pointer=string(b"CL_Value")
put(0x3F2B44-0x1200+0x136*4,key_pointer)
for slot in range(0x4077B8,0x4078D4,4): put(slot-0x1200,0x400000+0x409600)
asset[0x40A183-0x1200:0x40A187-0x1200] = b"PIC\0"
for rva,language in [(0x301918,b"JP-system"),(0x307828,b"JP-other"),(0x30D738,b"English")]:
    rows=[b"#CellModel_Basic",b"<-WH",b"#GI_Sheet_Local_Style",b"(FixedCols=0)(FixedRows=0)",
          b"#GI_Sheet_Local_BasicCell",b"{($Basic)}",b"#GI_Sheet_Local_Cells",language,
          b":(Required 1)",b"Value{(%CL_Value)}",b"Two{(%CL_Value)}",b"Three{(%CL_Value)}",
          b"#GI_Tree_Main_Body",b"caption{(%CallSheet:[St_Common]GI_Sheet_Local,403)}",
          b"#GI_Menu_Main_BasicCell",b"{($Basic)}",b"#GI_Menu_Main_Body",b"Menu{(%CL_Value)}",
          b"#GI_IPAddressListNotUsed",language+b"-IP",b"#UnusedPadding"]
    for index,row in enumerate(rows): put(rva-0x1200+index*4,string(row))
    put(rva-0x1200+len(rows)*4,key_pointer)
class ShortReader(io.BytesIO):
    def read(self,size=-1): return super().read(min(size,1000))
reader=ShortReader(asset)
sparse=p.native.read_source(reader)
assert not reader.closed
unbound=p.Resources.from_pe(sparse)
resources=unbound.with_language(0,False)
metadata=icf.settings.extended.MetadataField
assert unbound.metadata_empty_caption(metadata.TransmitPicture,None,None) == b"PIC"
assert resources.metadata_empty_caption(metadata.ConnectionIpList,0,None) == b"English-IP"
assert resources.metadata_empty_caption(metadata.ConnectionIpList,1,0x411) == b"JP-system-IP"
assert resources.metadata_empty_caption(metadata.ConnectionIpList,1,0x409) == b"JP-other-IP"
try:
    resources.metadata_empty_caption(metadata.ConnectionIpList,None,None)
except OSError:
    pass
else:
    raise AssertionError("metadata language guessed")
factory=p.native.Factory.parse(b"[St_Common]GI_Sheet_Local,403")
assert factory.context_type() >= 0 and resources.tree(0)[0].resource == factory.resource
assert p.native.Factory(factory.class_,factory.resource,factory.node,factory.group,factory.parameter).resource == factory.resource
node=p.native.CellNode.default()
node.key=0x136
command=p.native.menu.Command(node,0,False,0,0,None)
detached=command.node_snapshot()
detached.key=0
assert command.node_snapshot().key == 0x136
command.set_node(detached)
assert command.node_snapshot().key == 0
try:
    unbound.schema(factory,[],lambda _: True)
except OSError:
    pass
else:
    raise AssertionError("unbound language guessed")
conditions=[]
grid=resources.schema(factory,[],lambda name: conditions.append(name) or True)
assert conditions == [b"Required"] and grid.cells_snapshot()[0][0] == "English"
calls=[]
def fail_condition(name):
    calls.append(name)
    raise sentinel
try:
    resources.schema(factory,[],fail_condition)
except ValueError as error:
    assert error is sentinel and calls == [b"Required"]
else:
    raise AssertionError("condition failure lost")
memory=bytearray(0x3C9C0)
memory[0x355A1]=6
memory[-16:]=b"IcomCloneFormat3"
image=icf.Icf.from_memory(bytes(memory))
grid=resources.produce(image,bytes(memory),factory,0,lambda _: True,lambda node: b"actual")
assert "actual" in [text for row in grid.cells_snapshot() for text in row]
assert resources.commands(b"GI_Menu_Main")[0].node_snapshot().key == 0x136
saved.clear()
def dispatch(frame):
    saved.append(frame)
    assert frame.input_code == 0x0D and frame.phase == 1 and frame.text == b"text\0raw"
    raise sentinel
try:
    p.native.sync_editor(editor,grid,0,1,dispatch)
except ValueError as error:
    assert error is sentinel
assert editor.visible and editor.validation_guard == 0
try:
    saved[0].text
except ReferenceError:
    pass
else:
    raise AssertionError("inline frame survived")
job=p.Job.begin(["native"])
output=p.HtmlBuffer("prefix")
layout=p.layout(grid,paper,m)
painted=[]
def fail_paint(node):
    painted.append(node)
    if len(painted)==2: raise sentinel
    return b"page"
try:
    resources.render(bytes(memory),grid,layout,p.Views(),job,paper,m,output,fail_paint)
except ValueError as error:
    assert error is sentinel
else:
    raise AssertionError("native rendering failure lost")
assert job.pages >= 1 and output.text.startswith("prefix<section") and not job.ended
assert "EndDoc" not in [e.variant for e in job.events_snapshot()]
try:
    painted[0].key
except ReferenceError:
    pass
else:
    raise AssertionError("paint node survived")
job=p.Job.begin(["native"])
def renderer(name,live):
    resources.render(bytes(memory),grid,layout,p.Views(),live,paper,m,p.HtmlBuffer(""),lambda _: b"page")
    assert live.pages >= 1
assert not job.tick(renderer) and job.ended
assert [e.variant for e in job.events_snapshot()][-2:] == ["EndDoc","ClearNames"]
failed_job=p.Job.begin(["first","second"])
saved.clear()
def fail_job(name,live):
    saved.append(live)
    live.begin_page()
    live.page_drawn()
    raise sentinel
try:
    failed_job.tick(fail_job)
except ValueError as error:
    assert error is sentinel
else:
    raise AssertionError("job failure lost")
assert failed_job.pages == 1 and not failed_job.ended
assert failed_job.remaining_names() == ("first","second")
try:
    saved[0].pages
except ReferenceError:
    pass
else:
    raise AssertionError("job view survived")
print("printing portable/native/callback smoke: passed")
