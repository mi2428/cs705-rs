"""Headless calculations and callback prefix/lifetime checks."""
from cs705 import display as d, icf

s = d.Settings(0, 1, 100, 0, 0, 1, 1, 0, 15, 0)
metrics = d.FontMetrics(16, 8, 16, 96, 1, 0)
metrics.validate()
s.toggle_bit(33)
assert s.bit(33) and s.query_menu(0x40, 0, 0, 0) == (128, 0)
assert not s.input_options().japanese()
for target, name, bad in [(s, "language", True), (s, "column_bits", -1), (metrics, "em", 1 << 31)]:
    before = getattr(target, name)
    try:
        setattr(target, name, bad)
    except (TypeError, OverflowError):
        assert getattr(target, name) == before
    else:
        raise AssertionError((name, bad))
cell = d.Cell(1, b"a\0b", 3, 4)
row = d.Row(0, 1, 0, True, [cell], 4)
cell.text = b"later"
assert row.cells_snapshot()[0].text == b"a\0b"
row.set_cells([cell])
assert row.cells_snapshot()[0].text == b"later"
cells = row.cells_snapshot()
cells[0].text = b"snapshot edit"
assert row.cells_snapshot()[0].text == b"later"
row.set_cells(cells)
assert row.cells_snapshot()[0].text == b"snapshot edit"
assert d.notice_cell(None, d.NoticeParent.Agc, 0x337, 4, False).text == b"FAST"
assert d.notice_cell(None, d.NoticeParent.Agc, 0, 0, False) is None
try:
    d.notice_cell(None, d.NoticeParent.Filter, 0x12F, 36, False)
except OSError:
    pass
else:
    raise AssertionError("missing PE asset accepted")

state = d.State(s, metrics, (0, 0, 0, 0))
settings = state.settings_snapshot()
settings.fixed_name = 1
assert state.settings_snapshot().fixed_name == 0
state.set_settings(settings)
assert state.settings_snapshot().fixed_name == 1
copied_metrics = state.metrics_snapshot()
copied_metrics.padding = 2
assert state.metrics_snapshot().padding == 1
state.set_metrics(copied_metrics)
assert state.metrics_snapshot().padding == 2
copied_toolbar = state.toolbar_snapshot()
copied_toolbar.visible = True
assert not state.toolbar_snapshot().visible
state.set_toolbar(copied_toolbar)
assert state.toolbar_snapshot().visible
queue = state.queue_snapshot()
queue.append(d.Queued(0x19, 0, 0))
assert not state.queue_snapshot()
state.set_queue(queue)
assert state.queue_snapshot()[0].event == 0x19
state.set_queue([])
state.menu(b"", 0x37, 0x0D, 1, 0, 0, None)
assert state.resource_language == 0 and state.queue_snapshot()[0].event == 0x26
state.drain(b"", None)
assert state.resource_language == 1 and state.resource_generation == 1
views = state.views
assert views.register(d.ViewState(d.ViewKind.Preset, 0)) == 0
assert len(state.views.states_snapshot()) == 1 and views.nonempty(b"", 0)
states = views.states_snapshot()
states[0].current_row = 23
assert views.states_snapshot()[0].current_row != 23
views.set_states(states)
assert state.views.states_snapshot()[0].current_row == 23
state.open(bytes(0x4997A), d.ViewKind.Call, 0, None)
grid = state.current_snapshot()
assert grid.rows_snapshot() and grid.columns_snapshot()
assert all(isinstance(c.text, bytes) for c in grid.rows_snapshot()[0].cells_snapshot())
assert len(grid.column_positions(-(1 << 31))) == len(grid.columns_snapshot())
grid.selected_column = 13
assert state.current_snapshot().selected_column != 13
state.set_current(grid)
assert state.current_snapshot().selected_column == 13
columns = grid.columns_snapshot()
columns[0].width = 123
assert grid.columns_snapshot()[0].width != 123
grid.set_columns(columns)
assert grid.columns_snapshot()[0].width == 123
rows = grid.rows_snapshot()
rows[0].height = 777
assert grid.rows_snapshot()[0].height != 777
grid.set_rows(rows)
assert grid.rows_snapshot()[0].height == 777
try:
    state.views.set_states([])
except IndexError:
    pass
else:
    raise AssertionError("active grid invalidated")
for obj, names in [(state, ("settings", "metrics", "toolbar", "current", "queue")),
                   (grid, ("columns", "rows")), (row, ("cells",)), (views, ("states",))]:
    for name in names:
        assert not hasattr(obj, name), name
        try:
            setattr(obj, name, None)
        except AttributeError:
            pass
        else:
            raise AssertionError((name, "legacy property remained"))
state.set_current(None)
replacement = d.Views()
replacement.register(d.ViewState(d.ViewKind.Preset, 0))
state.views = replacement
assert len(views.states_snapshot()) == 1
try:
    replacement.states_snapshot()
except RuntimeError:
    pass
else:
    raise AssertionError("transferred owner remained usable")
assert d.sql_mask(b"", 0, s, views, None) == 0
assert d.column_hidden(0xED, False, s, 0)
try:
    state.resources(b"")
except OSError:
    pass
else:
    raise AssertionError("missing resources accepted")

toolbar = d.Toolbar(False, (1, 2, 30, 40), 0)
sentinel = ValueError("notify failure")
saved = []
def notify(view):
    saved.append(view)
    assert view.visible and view.occupied_height() == 40 and s.system_japanese()
    try:
        toolbar.visible
    except RuntimeError:
        pass
    else:
        raise AssertionError("busy owner accessible")
    try:
        view.visible = False
    except RuntimeError:
        pass
    else:
        raise AssertionError("readonly callback view was writable")
    raise sentinel
try:
    toolbar.set_visible(True, notify)
except ValueError as error:
    assert error is sentinel
else:
    raise AssertionError("notification failure lost")
assert toolbar.visible and toolbar.visible_changes == 1
try:
    saved[0].visible
except ReferenceError:
    pass
else:
    raise AssertionError("notification view survived")
toolbar.set_visible(True, lambda _: (_ for _ in ()).throw(AssertionError("must not notify")))

calls = []
def label(source, key, value):
    calls.append((source, key, value))
    return b""
assert d.channel_value_paint(0x108, 0xFFFFFFFF, None, label) == b""
assert calls == [(b"GI_Sheet_Sel_Mch_Mode", 7, 0xFFFFFFFF)]
def fail_label(*args):
    raise sentinel
try:
    d.channel_value_paint(0x108, 13, None, fail_label)
except ValueError as error:
    assert error is sentinel
else:
    raise AssertionError("label failure lost")
assert d.channel_value_paint(0x113, 7, None, fail_label)
record = icf.record.reset_holder(2, 0, 0)
record.set_valid(True)
record.set_mode(13)
def record_label(source, key, value):
    assert (source, key, value) == (b"GI_Sheet_Sel_Mch_Mode", 7, 13)
    try:
        record.valid()
    except RuntimeError:
        pass
    else:
        raise AssertionError("record owner was not busy")
    return b"actual label"
assert d.channel_paint(record, 0x108, None, record_label) == b"actual label"
assert record.valid()
try:
    d.channel_paint(record, 0x108, None, fail_label)
except ValueError as error:
    assert error is sentinel and record.valid()
else:
    raise AssertionError("record label exception lost")
memory = bytearray(0x3C9C0)
memory[0x355A1] = 6
memory[-16:] = b"IcomCloneFormat3"
image = icf.Icf.from_memory(bytes(memory))
assert d.filter.dd_rows_visible()
assert d.filter.paint(image, 0x131, 0, None) == b"---"
assert d.filter.paint(image, 0x132, 0, None) == b"0"
print("display headless smoke: passed")
