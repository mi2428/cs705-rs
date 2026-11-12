"""Minimal clipboard contracts against the installed native wheel."""
from cs705 import Error, Icf, _native
from cs705.icf import special as s


def rejects(error_type, call):
    try:
        call()
    except error_type:
        return
    raise AssertionError(f'expected {error_type.__name__}')


def image():
    raw = bytearray(b'\xa5' * 0x3c9c0)
    raw[0x355a1] = 6
    raw[0x3c9b0:] = b'IcomCloneFormat3'
    raw[0xa60b:0xa60f] = bytes([255] * 4)
    raw[0x37802] = 0
    raw[0x37807] = 1
    return Icf.from_memory(bytes(raw))


def check():
    for name in ('CompactClipboard', 'ScanClipboard', 'PasteApproval',
                 'CompactChannel', 'ScanEdge', 'RowState', 'Effects', 'Notifications'):
        assert getattr(s, name) is getattr(_native.icf.special, name)
    approval = s.PasteApproval(False, True)
    approval.tail = True
    approval.overwrite = False
    assert approval.tail and not approval.overwrite
    rejects(TypeError, lambda: s.PasteApproval(1, True))
    compact = s.CompactClipboard(0, [], [])
    compact.reserve_count = -1
    compact.set_source_rows([1, 2])
    assert compact.source_rows_snapshot() == [1, 2]
    compact.set_channels([])
    rejects(TypeError, lambda: compact.set_source_rows([True]))
    rejects(TypeError, lambda: compact.set_channels([object()]))
    rejects(TypeError, lambda: setattr(compact, 'reserve_count', True))
    g = bytearray(0x4997a)
    for row in range(45):
        at = s.CompactKind.BandStack.offset(row)
        g[at:at + 4] = (30_000 + row).to_bytes(4, 'little')
        g[at + 4] = 0x10
    compact.copy(bytes(g), s.CompactKind.BandStack, range(45), False)
    assert compact.reserve_count == 42
    assert compact.source_rows_snapshot() == list(range(27)) + list(range(30, 45))
    snapshot = compact.channels_snapshot()
    assert len(snapshot) == 42 and snapshot[0].frequency_hz == 30_000
    snapshot[0].frequency_hz = 123
    assert compact.channels_snapshot()[0].frequency_hz == 30_000
    compact.set_channels(snapshot)
    assert compact.channels_snapshot()[0].frequency_hz == 123
    rejects(Error, lambda: compact.copy(bytes(g), s.CompactKind.BandStack, range(24, 28), False))
    assert compact.reserve_count == 1 and compact.source_rows_snapshot() == [24]
    assert len(compact.channels_snapshot()) == 1
    rejects(TypeError, lambda: compact.copy(bytes(g), s.CompactKind.BandStack, [0, 1], False))
    rejects(TypeError, lambda: compact.copy(bytearray(g), s.CompactKind.BandStack, range(1), False))
    assert s.CompactClipboard.default().reserve_count == 0

    row = s.ScanEdge(True, 30_000, 60_000, 0x11, 1, 2, b'row'.ljust(16))
    scan = s.ScanClipboard([row], 1)
    scan.transfer_count = 1
    rows = scan.rows_snapshot()
    rows[0].name = b'edit'.ljust(16)
    assert scan.rows_snapshot()[0].name == b'row'.ljust(16)
    scan.set_rows(rows)
    assert scan.rows_snapshot()[0].name == b'edit'.ljust(16)
    rejects(TypeError, lambda: scan.set_rows([object()]))
    rejects(TypeError, lambda: setattr(scan, 'transfer_count', True))
    rejects(OverflowError, lambda: setattr(scan, 'transfer_count', -1))
    state = s.RowState.default()
    target = image()
    # Explicit G source; default/native sources are never synthesized by bindings.
    generation = target.memory()
    effect = scan.paste(target, generation, 0, s.PasteApproval(True, True), state, False)
    assert effect.writes == 1 and effect.callback_flags == 8
    assert (state.start, state.end, state.state) == (0, 0, 1)
    assert scan.rows_snapshot()[0].data == 0 and scan.rows_snapshot()[0].filter == 255
    assert row.data == 1 and row.filter == 2
    copied = s.ScanClipboard.default()
    copied.copy(target.memory(), range(1), state)
    assert copied.transfer_count == 1 and state.bits & 4
    assert copied.rows_snapshot()[0].name == b'edit'.ljust(16)
    before = target.memory()
    assert copied.paste(target, b'', 0, s.PasteApproval(True, False), state, False).writes == 0
    assert target.memory() == before
    copied.set_rows([row, row])
    copied.transfer_count = 2
    assert copied.paste(target, b'', 24, s.PasteApproval(False, True), state, False).writes == 0

    # A late G read fails after row stores, clipboard normalization and selection.
    target = image()
    scan = s.ScanClipboard([row], 1)
    state = s.RowState(9, 10, 11, 12, 13)
    rejects(Error, lambda: scan.paste(target, b'', 1, s.PasteApproval(True, True), state, True))
    assert s.ScanEdge.read(target.memory(), 1).data == 0
    assert scan.rows_snapshot()[0].filter == 255
    assert target.memory()[0x37802] == 4 and target.memory()[0x37807] == 0
    assert (state.bits, state.start, state.end, state.active, state.state) == (9, 10, 11, 12, 13)
    # A later read fails after clearing and capturing the first clipboard row.
    source = bytes(0xa60c)
    rejects(Error, lambda: scan.copy(source, range(7, 9), state))
    assert scan.transfer_count == 0 and len(scan.rows_snapshot()) == 1
    assert state.bits == 9 and state.start == 10
    print('Clipboard check: OK (fields/snapshots, native copy/paste, approvals, clipboard mutation and error prefixes)')


if __name__ == '__main__':
    check()
