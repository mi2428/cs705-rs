"""Run with python -m cs705.csv._smoke after building the native extension."""
from cs705 import MutableBytes, Error, Icf
from cs705.icf import ErrorData as IcfErrorData
from cs705.icf.context import Context as IcfContext
from cs705 import csv
from cs705.csv import channel, control, datetime as dt, date_input, dispatch, nls, table, payload, history, number


def main() -> None:
    locale = csv.Locale.copied(ord('.'), ord(','))
    assert list(csv.Cells(b'\x81,,x,', locale.list)) == [b'\x81,', b'x', b'']
    assert csv.Cells is csv.OwnedCells
    source = b'H\r\nx\n\0tail'
    reader = csv.Reader(source, locale)
    assert reader.original_bytes() == source
    rows = reader.rows()
    del reader
    assert list(rows) == [b'x']
    assert csv.column(b'a', None, 44) is None
    assert csv.column(b'a', 3, 44) == b''
    binding = dispatch.Binding.ascii(b'A,B', 44, [b'A', b'B', b'C'], [0])
    assert binding.cell(b'x', 1) == b'' and binding.cell(b'x', 2) is None
    assert csv.fixed_text(b'AB\x81Z', 3) == (b'AB ', True)
    binding_views = []
    memory_rows = channel.MemoryRows(b'Frequency\n145\n', channel.Kind.Memory, locale, False)
    assert memory_rows.binding(lambda b: b.accepted)
    memory_rows.binding(lambda b: binding_views.append(b))
    try:
        binding_views[0].accepted
    except ReferenceError:
        pass
    else:
        raise AssertionError('expired Binding remained live')
    for bad in (True, -1, 256):
        try:
            csv.is_lead(bad)
        except (TypeError, OverflowError):
            pass
        else:
            raise AssertionError('invalid byte accepted')

    context = dt.Context(b'yyyy/mm/dd', 47, 58, 46, b'AM', b'PM', 2026, 50, 0, [dt.Era(None, 0), dt.Era(b'', 0)])
    assert context.eras[0].name is None and context.eras[1].name == b''
    context.am = b'a.m.'
    assert context.am == b'a.m.'
    date = dt.Date(2024, 2, 29)
    date.day = 28
    assert date == dt.Date(2024, 2, 28)
    token = dt.DateToken(date, ord('d'), 2)
    token.count = 3
    assert token.count == 3
    selection = channel.Selection(0, 1, 1)
    selection.last = 2
    assert selection.last == 2
    constructor_report = channel.Import(control.Counts(0, 0, 0, 0), 0, 0, None)
    constructor_report.rows_read = 3
    constructor_export = channel.Export(b'old', 0, None)
    constructor_export.bytes = b'new'
    assert constructor_report.rows_read == 3 and constructor_export.bytes == b'new'
    memory_report = channel.MemoryImport(constructor_report, False, False)
    memory_report.call_invalidated = True
    assert memory_report.report is constructor_report and memory_report.call_invalidated
    profile = nls.Profile(False, True, True, b'/', 8)
    profile.maximum = 9
    assert profile.maximum == 9
    entry = table.Entry(b'old', 1)
    entry.label = b'new'
    assert entry.label == b'new'
    payload_context = payload.Context(44, 46, 0, 0, 0, b'old', False, 0, False, False, False)
    payload_context.group_name = b'new'
    request = payload.Request(payload.Adapter.Mode, b'\x00', 0)
    request.source = b'\x01'
    assert payload_context.group_name == b'new' and request.source == b'\x01'
    paint = history.paint.Context(None, 1, None, None)
    paint.date_order = 2
    assert paint.date_order == 2
    gate = control.ImportGate.Header(1)
    assert isinstance(gate, control.ImportGate.Header) and gate.reaches_rows()
    text_input = date_input.Input.Text(b'2024/03/01')
    assert isinstance(text_input, date_input.Input.Text) and text_input.bytes == b'2024/03/01'
    for constructor in (control.ImportGate.Header, date_input.Input.Begin):
        try:
            constructor(True)
        except TypeError:
            pass
        else:
            raise AssertionError('bool variant byte accepted')
    locale_views = []
    def read_context(view):
        locale_views.append(view)
        assert view.am == b'a.m.'
        try:
            view.am = b'blocked'
        except RuntimeError:
            pass
        else:
            raise AssertionError('readonly context mutated')
        return dt.parse_date(b'2024/02/29', view).day
    assert context.with_context(read_context) == 29
    try:
        locale_views[0].am
    except ReferenceError:
        pass
    else:
        raise AssertionError('borrowed Context survived its frame')
    era_view = context.eras[1]
    era_view.offset = 6
    assert context.eras[1].offset == 6
    context.eras = [dt.Era(None, 0), dt.Era(b'', 0)]
    try:
        era_view.offset
    except ReferenceError:
        pass
    else:
        raise AssertionError('replaced era view remained live')
    date_view = token.date
    date_view.day = 27
    assert token.date.day == 27 and date.day == 28
    token.date = date
    try:
        date_view.day
    except ReferenceError:
        pass
    else:
        raise AssertionError('replaced date view remained live')
    options = channel.MemoryOptions(locale, 46, b'', None, IcfContext.from_bytes(bytes(24)), None)
    option_locale = options.locale
    option_locale.list = 59
    assert options.locale.list == 59 and locale.list == 44
    options.locale = locale
    try:
        option_locale.list
    except ReferenceError:
        pass
    else:
        raise AssertionError('replaced locale view remained live')
    assert channel.Kind.from_context(2) is channel.Kind.Memory
    assert dispatch.ImportKind.Memory.required_labels() == [b'Frequency']
    assert dt.parse_date(b'2024/02/29', context).day == 29
    original = RuntimeError('callback failed')
    calls = []

    def fail(*args):
        calls.append(args)
        raise original

    try:
        csv.find_column_with(b'A,B', b'A', 44, fail)
    except RuntimeError as error:
        assert error is original and len(calls) == 1
    else:
        raise AssertionError('callback failure swallowed')
    era_context = dt.Context(b'gg yyyy/mm/dd', 47, 58, 46, b'AM', b'PM', 2026, 50, 0, [dt.Era(b'E', 2019)])
    before_calls = len(calls)
    try:
        dt.parse_date_with(b'E 5/2/28', era_context, fail)
    except RuntimeError as error:
        assert error is original and len(calls) == before_calls + 1
    else:
        raise AssertionError('date comparison failure swallowed')

    retained = []

    def map_bytes(source, destination):
        retained.append(destination)
        assert destination[:] == b' ' * len(destination)
        destination[:len(source)] = source
        assert destination[:len(source)] == source
        assert destination[::-1] == destination.to_bytes()[::-1]
        before = destination.to_bytes()
        destination[::-1] = before[::-1]
        assert destination.to_bytes() == before
        destination[1:1] = b''
        try:
            destination[True]
        except TypeError:
            pass
        else:
            raise AssertionError('bool native index accepted')
        try:
            destination[:] = b''
        except ValueError:
            assert destination.to_bytes() == before
        else:
            raise AssertionError('native buffer resized')
        destination[-1] = ord(' ')
        return 0

    shared = MutableBytes(b'ab')
    assert nls.map_copy_back(shared, map_bytes) == 0
    assert shared.to_bytes() == b'ab'
    try:
        retained[0].to_bytes()
    except ReferenceError:
        pass
    else:
        raise AssertionError('expired native buffer remained live')

    shared = MutableBytes(b'\x01a')
    try:
        nls.callsign_fixed(shared, fail)
    except RuntimeError as error:
        assert error is original and shared.to_bytes() == b'a '
    else:
        raise AssertionError('NLS failure swallowed')

    seen = []
    def write(value):
        seen.append(value)
        return control.WriteResult(0 if value == b'bad' else 1, 0)
    assert not control.export_ranges(b'H', [[b'bad', b'skip'], [b'last']], True, write)
    assert seen == [b'H', b'bad', b'last']
    counts = control.Counts(0, 0, 0, 0)
    try:
        control.scan_row(counts, control.ScanRow(1, True, 2, 3), control.ScanRange(0, 5, False), fail)
    except RuntimeError as error:
        assert error is original and (counts.c2, counts.c3, counts.c4) == (2, 3, 0)
    else:
        raise AssertionError('apply failure swallowed')
    corrections = table.Corrections(0)
    assert table.mode_ascii(b'alias', [], [table.Entry(b'alias', 13)], corrections) == 13
    assert corrections.value == 1

    raw = MutableBytes(bytes.fromhex('290224233040'))
    runtime = date_input.Runtime(context, b'hh:nn:ss', 1, None)
    effect = date_input.Effect.default()
    date_input.edit_masked(raw, 1, 60, b'03/03/2024', runtime, effect)
    assert raw.to_bytes() == bytes.fromhex('020324233040') and effect.changed
    views = []
    effect.with_dialog(lambda dialog: views.append(dialog))
    try:
        views[0].enabled
    except ReferenceError:
        pass
    else:
        raise AssertionError('expired dialog remained live')
    edge_rows = channel.UserEdgeRows(b'No,Frequency Low,Frequency High\n1,145,146\n', locale, channel.Selection(0, 1, 1), False)
    edge = next(edge_rows)
    assert edge.position == 1 and edge.holder.valid()
    lazy_source = b'No,Frequency Low,Frequency High\n1,' + b'1' * 4097 + b',146\n1,145,146\n'
    lazy_rows = channel.UserEdgeRows(lazy_source, locale, channel.Selection(0, 1, 1), False)
    try:
        next(lazy_rows)
    except Error as error:
        assert error.domain == 'csv' and error.kind == 'Limit'
    else:
        raise AssertionError('row conversion failure swallowed')
    assert next(lazy_rows).holder.valid()
    typed_error = csv.ErrorData.Invalid('runtime 理由\0é')
    typed_failure = channel.Failure.Text(typed_error)
    assert typed_failure.error.native == typed_error
    constructor_report.failure = typed_failure
    assert constructor_report.failure.error.native == typed_error
    image_error = IcfErrorData.Operation('image reason')
    assert channel.Failure.Image(image_error).error.native == image_error
    for constructor, wrong in ((channel.Failure.Text, image_error),
                               (channel.Failure.Image, typed_error),
                               (channel.Failure.Text, {'diagnostic': typed_error.diagnostic})):
        try:
            constructor(wrong)
        except TypeError:
            pass
        else:
            raise AssertionError('noncanonical or wrong-domain error accepted')
    try:
        number.decimal(b'bad', 46)
    except Error as native_csv_error:
        failure = channel.Failure.Text(native_csv_error)
        assert isinstance(failure, channel.Failure.Text)
        assert failure.error.kind == native_csv_error.kind
        constructor_report.failure = failure
        assert constructor_report.failure == failure
        try:
            channel.Failure.Image(native_csv_error)
        except TypeError:
            pass
        else:
            raise AssertionError('wrong-domain failure accepted')
    else:
        raise AssertionError('decimal conversion accepted bad input')
    constructed_edge = channel.EdgeRow(edge.holder, 1, 0, 0)
    constructed_edge.position = 2
    constructed_call = channel.CallsignRow(edge.holder, False, None)
    constructed_call.accepted = True
    assert constructed_edge.holder is edge.holder and constructed_edge.position == 2 and constructed_call.accepted
    coordinate_dialog = number.CoordinateDialog.area(bytes(4), bytes(0x396b5), None)
    coordinate_dialog.mask = b'custom mask'
    assert coordinate_dialog.mask == b'custom mask'
    runtime.locale.am = b'live AM'
    assert runtime.locale.am == b'live AM'
    runtime.clock = date_input.Clock(2026, 11, 12, 1, 2, 3, 0)
    clock_view = runtime.clock
    clock_view.hour = 4
    assert runtime.clock.hour == 4
    runtime.clock = None
    try:
        clock_view.hour
    except ReferenceError:
        pass
    else:
        raise AssertionError('removed clock view remained live')
    literal_dialog = date_input.DateDialog(coordinate_dialog, True)
    literal_effect = date_input.Effect(False, None, b'', (0, 0), literal_dialog)
    dialog_view = literal_effect.dialog
    editor_view = dialog_view.editor
    editor_view.control = b'live control'
    assert literal_effect.dialog.editor.control == b'live control'
    dialog_view.enabled = False
    assert not literal_effect.dialog.enabled
    dialog_view.editor = coordinate_dialog
    try:
        editor_view.control
    except ReferenceError:
        pass
    else:
        raise AssertionError('replaced editor view remained live')
    latest_dialog = literal_effect.dialog
    literal_effect.dialog = None
    try:
        latest_dialog.enabled
    except ReferenceError:
        pass
    else:
        raise AssertionError('removed dialog view remained live')
    memory = bytearray(0x3c9c0)
    memory[0x355a1] = 6
    memory[0x3c9b0:] = b'IcomCloneFormat3'
    image = Icf.from_memory(bytes(memory))
    report = channel.import_user_edge(image, b'No,Frequency Low,Frequency High\n1,145,146\n', channel.Selection(0, 0, 1), False, locale)
    assert report.failure is None and report.applied == 1
    assert report.counts is report.counts
    report.counts.c3 = 7
    assert report.counts.c3 == 7
    assert channel.export_user_edge(image, channel.Selection(0, 0, 1), locale).rows == 1
    dispatch_value = dispatch.Dispatch(0, payload.Kind.Memory, dispatch.ImportKind.Memory, True)
    fields = [
        (locale, 'decimal list'),
        (selection, 'first last number_base'),
        (constructor_report, 'counts rows_read applied failure'),
        (constructor_export, 'bytes rows failure'),
        (memory_report, 'report home_invalidated call_invalidated'),
        (options, 'locale crt_decimal generation ranges live_q system_langid'),
        (constructed_edge, 'holder position discard corrections'),
        (constructed_call, 'holder accepted corrections'),
        (counts, 's c2 c3 c4'),
        (control.WriteResult(1, 0), 'bool_value bytes_written'),
        (control.ScanRow(0, False, 0, 0), 'position valid c2_delta c3_delta'),
        (control.ScanRange(0, 0, False), 'base last stop_on_excess'),
        (dispatch_value, 'slot export import skip_invalid_export'),
        (binding, 'positions accepted separator'),
        (date, 'year month day'),
        (dt.Time(1, 2, 3, 4), 'hour minute second millisecond'),
        (dt.Era(None, 0), 'name offset'),
        (context, 'short_date date_separator time_separator decimal_separator am pm current_year two_digit_window primary_language eras'),
        (token, 'date letter count'),
        (date_input.Clock(2026, 11, 12, 1, 2, 3, 0), 'year month day hour minute second millisecond'),
        (runtime, 'locale time_format date_order clock'),
        (literal_dialog, 'editor enabled'),
        (literal_effect, 'changed state_out display selection dialog'),
        (coordinate_dialog, 'mode mask seed control phase2 cancel done release_requested'),
        (profile, 'uppercase keep_kana keep_spaces punctuation_exceptions maximum'),
        (entry, 'label value'),
        (table.ToneCandidate(0, b'0Hz'), 'value label'),
        (payload_context, 'separator decimal row number_base group group_name alarm utc_offset_minutes memory_group_columns memory_selection_column memory_pair'),
        (request, 'adapter source utc_offset_minutes'),
        (paint, 'date_order gui_language system_langid time_separators'),
    ]
    for value, names in fields:
        for name in names.split():
            setattr(value, name, getattr(value, name))
    try:
        Icf.from_memory(b'')
    except Error as native_image_error:
        image_failure = channel.Failure.Image(native_image_error)
        assert isinstance(image_failure, channel.Failure.Image)
        assert image_failure.error.domain == 'icf'
    else:
        raise AssertionError('invalid image accepted')
    assert csv.Locale is type(options.locale)
    assert dt.Context is dt.OwnedContext and dt.Era is dt.OwnedEra
    assert isinstance(table.DTCS_CODES, tuple) and len(table.DTCS_CODES) == 104
    print(f'CSV smoke: OK ({len(fields)} structs, {sum(len(names.split()) for _, names in fields)} field assignments)')


if __name__ == '__main__':
    main()
