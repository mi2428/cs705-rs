"""Run with the interpreter into which the cs705 wheel was installed."""
from cs705 import MutableBytes, _native
from cs705.icf import normalizer as n
from cs705.icf.frequency import FrequencyRange
from cs705.icf.record_image import ChannelKind
from cs705.icf.record import ChannelWrite, ChannelStage


def check():
    assert n.FrequencyRange is FrequencyRange
    assert n.ReferenceImage is _native.icf.normalizer.ReferenceImage
    values = n.ChannelValues(5, 1, 1, 1000, 50, 50)
    n.normalize_channel_values(values, False, True)
    source = MutableBytes(bytes(0x379b0))
    calls = []
    def count():
        calls.append('count')
        return 1
    def get(index):
        calls.append(index)
        return FrequencyRange(10, 20, 0)
    n.stage7_live(source, count, get)
    result = source.to_bytes()
    assert result[0x378c0:0x378c8] == (10).to_bytes(4, 'little') + (20).to_bytes(4, 'little')
    assert calls[:4] == ['count', 0, 0, 'count'] and calls.count('count') == 30
    assert result[0x378c8:] == bytes(29 * 8)
    separate = n.ReferenceImage.Separate(result)
    assert separate.source == result and separate.kind == 'Separate'
    assert n.ReferenceImage.Working.source is None

    marker = LookupError('normalizer prefix')
    calls.clear()
    def failing_count():
        calls.append('count')
        if len(calls) > 3:
            raise marker
        try:
            source.to_bytes()
        except RuntimeError:
            pass
        else:
            raise AssertionError('busy native owner was accessible')
        return 1
    source = MutableBytes(b'\xa6' * 0x379b0)
    try:
        n.stage7_live(source, failing_count, get)
    except LookupError as error:
        assert error is marker
        frame = error.__traceback__
        while frame.tb_next:
            frame = frame.tb_next
        assert frame.tb_frame.f_code.co_name == 'failing_count'
    else:
        raise AssertionError('native callback failure was swallowed')
    result = source.to_bytes()
    assert result[0x378c0:0x378c8] == (10).to_bytes(4, 'little') + (20).to_bytes(4, 'little')
    assert result[0x378c8:] == b'\xa6' * (29 * 8)
    assert calls == ['count', 0, 0, 'count']
    # The core's small raw-channel fixture, not a production default image.
    raw = bytearray(0x4997a)
    raw[0x3fcd4:0x49914] = (0xffff).to_bytes(4, 'little') * ((0x49914 - 0x3fcd4) // 4)
    raw[0x3fcd4:0x3fcd8] = (3).to_bytes(4, 'little')
    for side, frequency, offset, mode in ((0, 321, 149, 0x82), (1, 456, 150, 0x70)):
        at = 250 + 33 * side
        raw[at:at + 4] = frequency.to_bytes(4, 'little')
        raw[at + 4] = mode
        raw[at + 5] = 1
        raw[at + 9:at + 11] = bytes([50, 50])
        raw[at + 29:at + 33] = offset.to_bytes(4, 'little')
    image = MutableBytes(bytes(raw))
    retained = []
    def previous(source, holder):
        assert holder.offset_hz() == 200
        retained.extend((source, holder))
        return True
    def store(write):
        assert isinstance(write, ChannelWrite)
        retained.append(write)
        if write.variant == 'Body':
            retained.append(write.bytes)
        if write.stage() is ChannelStage.Mode:
            raise marker
    try:
        n.normalize_channel_at(image, n.ReferenceImage.Working,
                               n.ChannelNormalization(ChannelKind.Normal, 1, 17, 29, 0, 0),
                               previous, store)
    except LookupError as error:
        assert error is marker
    else:
        raise AssertionError('channel store failure was swallowed')
    raw[283:287] = (321).to_bytes(4, 'little')
    assert image.to_bytes() == bytes(raw)
    for view in retained:
        try:
            if isinstance(view, ChannelWrite):
                view.stage()
            elif hasattr(type(view), 'to_bytes'):
                view.to_bytes()
            else:
                view.offset_hz()
        except ReferenceError:
            pass
        else:
            raise AssertionError('native readonly view survived its frame')
    value = n.ChannelNormalization(ChannelKind.Normal, 0, 1, 2, 0, 3)
    for field in ('kind', 'index', 'working_binding', 'default_binding', 'q12', 'previous_reload_guard'):
        setattr(value, field, getattr(value, field))
    try:
        value.q12 = True
    except TypeError:
        pass
    else:
        raise AssertionError('bool accepted as a byte')
    lists = n.ResetLists(0, 1, 2, 3)
    report = n.ResetReport(0, lists)
    report.lists.deleted_ur = 4
    assert report.lists is lists and report.lists.deleted_ur == 4
    state = n.ResetState(1, 2)
    state.home_invalidated = 3
    assert state.home_invalidated == 3
    print('Normalizer check: OK (stage7/channel native order, error prefixes, loan expiry, shared types and fields)')


if __name__ == '__main__':
    check()
