use pyo3::prelude::*;
#[test]
fn printing_smoke() {
    let _gc = super::GC_TESTS.lock().unwrap();
    Python::initialize();
    Python::attach(|py| {
        let root = super::package(py);
        let locals = pyo3::types::PyDict::new(py);
        locals
            .set_item("p", root.getattr("printing").unwrap())
            .unwrap();
        py.run(c"assert p.axis_spans([10,10,10,10],1,[False]*4,35)==b'\\x00\\x02\\x00\\x01'\nm=p.Metrics(-12,12,6,12,1)\nm.validate()\ng=p.Grid('name',[['fixed','x'],['row','value']],1,1,[False,False],[False,False],None,None,0)\nlayout=p.layout(g,p.Paper(200,200),m)\nassert layout.pages_snapshot()\nassert '<svg' in p.html_page(g,layout.pages_snapshot()[0],p.Paper(200,200),m,1)\n",Some(&locals),Some(&locals)).unwrap();
        py.run(
            &std::ffi::CString::new(include_str!("printing_smoke.py")).unwrap(),
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
        py.run(
            cr#"
import gc
def qa_error(call, expected):
    try:
        call()
    except expected:
        pass
    else:
        raise AssertionError('invalid callback operation accepted')
editor.visible = True
saved.clear()
def qa_readonly(frame):
    saved.append(frame)
    gc.collect()
    qa_error(lambda: setattr(frame, 'text', b'changed'), RuntimeError)
    qa_error(lambda: editor.text, RuntimeError)
    qa_error(grid.cells_snapshot, RuntimeError)
    node = frame.node_snapshot()
    node.key = 0xffffffff
    assert frame.node_snapshot().key != node.key
    assert frame.text == editor_text
editor_text = editor.text
p.native.sync_editor(editor, grid, 0, 1, qa_readonly)
assert not editor.visible and editor.text == editor_text
qa_error(lambda: saved[0].text, ReferenceError)
painted.clear()
def qa_paint(node):
    painted.append(node)
    gc.collect()
    qa_error(lambda: setattr(node, 'key', 0), RuntimeError)
    qa_error(image.memory, RuntimeError)
    return b'paint'
resources.produce(image, bytes(memory), factory, 0, lambda _: True, qa_paint)
assert painted and image.memory() == bytes(memory)
qa_error(lambda: painted[0].key, ReferenceError)
class OverReader:
    def read(self, size): return b'x' * (size + 1)
    def seek(self, *args): raise AssertionError('over-read must stop')
qa_error(lambda: p.native.read_source(OverReader()), ValueError)
for bad_asset in (b'', b'MZ', bytes(0x600)):
    qa_error(lambda: p.Resources.from_pe(bad_asset), OSError)
exhausted = p.Job.begin(['overflow'])
exhausted.pages = (1 << 64) - 1
output = p.HtmlBuffer('prefix')
events = [event.variant for event in exhausted.events_snapshot()]
qa_error(lambda: resources.render(bytes(memory), grid, layout, p.Views(), exhausted,
                                  paper, m, output,
                                  lambda _: (_ for _ in ()).throw(AssertionError('overflow reached paint'))),
         OSError)
assert exhausted.pages == (1 << 64) - 1 and output.text == 'prefix'
assert [event.variant for event in exhausted.events_snapshot()] == events
editor.visible = True
qa_error(lambda: p.sync_editor(editor, lambda _: False), TypeError)
assert editor.visible and editor.validation_guard == 0
class BytearrayReader:
    def read(self, size): return bytearray(size)
qa_error(lambda: p.native.read_source(BytearrayReader()), TypeError)
class BoolSeek(io.BytesIO):
    def seek(self, *args): return False
reader = BoolSeek(asset)
qa_error(lambda: p.native.read_source(reader), TypeError)
assert not reader.closed
"#,
            Some(&locals),
            Some(&locals),
        )
        .unwrap();
    });
}
