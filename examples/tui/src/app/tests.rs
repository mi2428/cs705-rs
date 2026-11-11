//! Local keyboard/render checks; no serial endpoints are opened.
use super::*;
use crate::model::tests::snapshot;
use ratatui::{Terminal, backend::TestBackend};

fn press(app: &mut App, code: KeyCode) {
    app.key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn text(app: &mut App, value: &str) {
    for c in value.chars() {
        press(app, KeyCode::Char(c));
    }
}

#[test]
fn keyboard_batch_sparse_group_copy_diff_and_discard_never_start_io() {
    let mut app = App::new(Source::File("unused.icf".into()), "unused-backups".into());
    app.finish(Ok(Finished::Loaded(snapshot(5)))).unwrap();
    press(&mut app, KeyCode::Tab);
    assert!(app.pane == Pane::Items);
    press(&mut app, KeyCode::Char('g'));
    text(&mut app, "99");
    press(&mut app, KeyCode::Tab);
    text(&mut app, "Radio");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.group, 99);
    app.group = 0;
    press(&mut app, KeyCode::Char('c'));
    text(&mut app, "99");
    press(&mut app, KeyCode::Tab);
    text(&mut app, "0");
    press(&mut app, KeyCode::Tab);
    text(&mut app, "Station");
    press(&mut app, KeyCode::Tab);
    text(&mut app, "80");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.group, 99);
    let editor = app.editor.as_ref().unwrap();
    assert!(editor.dirty);
    assert_eq!(
        editor
            .row(99, 0)
            .unwrap()
            .unwrap()
            .channel_field(ChannelField::FrequencyHz),
        80_000_000
    );
    assert!(app.job.is_none());
    let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let rendered: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(
        rendered.contains("LOCAL CHANGES")
            && rendered.contains("Station")
            && rendered.contains("80.000000")
    );
    press(&mut app, KeyCode::Char('d'));
    assert!(matches!(app.dialog, Some(Dialog::Diff(_, _))));
    press(&mut app, KeyCode::Esc);
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(matches!(app.dialog, Some(Dialog::Apply(_))));
    press(&mut app, KeyCode::Esc);
    assert!(app.job.is_none());
    press(&mut app, KeyCode::Char('x'));
    press(&mut app, KeyCode::Char('n'));
    assert!(app.editor.as_ref().unwrap().dirty);
    press(&mut app, KeyCode::Char('x'));
    press(&mut app, KeyCode::Char('y'));
    assert!(!app.editor.as_ref().unwrap().dirty);
    assert!(app.editor.as_ref().unwrap().row(99, 0).unwrap().is_none());
}

#[test]
fn acknowledged_write_blocks_edits_and_demands_operator_restart_confirmation() {
    let mut app = App::new(
        Source::Radio(Radio {
            port: "never-open-test-port".into(),
            baud: 115200,
            timeout_ms: 1000,
        }),
        "unused".into(),
    );
    app.finish(Ok(Finished::Loaded(snapshot(5)))).unwrap();
    let expected = app.editor.as_ref().unwrap().candidate().unwrap();
    app.finish(Ok(Finished::Written(expected, "before.icf".into())))
        .unwrap();
    assert!(app.needs_verification());
    press(&mut app, KeyCode::Char('e'));
    assert!(app.dialog.is_none());
    press(&mut app, KeyCode::Char('v'));
    assert!(matches!(app.dialog, Some(Dialog::Verify)));
    press(&mut app, KeyCode::Esc);
    assert!(app.job.is_none());
    press(&mut app, KeyCode::Char('q'));
    press(&mut app, KeyCode::Char('n'));
    assert!(!app.quit);
    press(&mut app, KeyCode::Char('q'));
    press(&mut app, KeyCode::Char('y'));
    assert!(app.quit && app.needs_verification());
}

#[test]
fn failed_started_write_stays_uncertain_without_retry_or_draft_loss() {
    let mut app = App::new(Source::File("unused".into()), "unused".into());
    app.finish(Ok(Finished::Loaded(snapshot(5)))).unwrap();
    app.editor
        .as_mut()
        .unwrap()
        .edit(0, 0, "Changed", "80")
        .unwrap();
    let (sender, receiver) = mpsc::channel();
    sender
        .send(Err("injected transfer failure".into()))
        .unwrap();
    app.job = Some(Job {
        receiver,
        logs: mpsc::channel().1,
        thread: thread::spawn(|| {}),
        cancelled: Arc::new(AtomicBool::new(false)),
        write_started: Arc::new(AtomicBool::new(true)),
    });
    app.poll();
    assert!(app.uncertain && app.editor.as_ref().unwrap().dirty);
    assert!(app.status.contains("UNCONFIRMED"));
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(app.job.is_none() && app.dialog.is_none());
}

#[test]
fn mouse_uses_rendered_offsets_and_never_confirms_a_dialog() {
    let mut app = App::new(Source::File("unused.icf".into()), "unused".into());
    app.finish(Ok(Finished::Loaded(snapshot(5)))).unwrap();
    app.row = 50;
    let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let expected = app.rows.offset() + 1;
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: app.item_rows.x,
        row: app.item_rows.y + 1,
        modifiers: KeyModifiers::NONE,
    });
    assert!(app.pane == Pane::Items && app.row == expected);
    let click_log = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: app.log_rows.x,
        row: app.log_rows.y,
        modifiers: KeyModifiers::NONE,
    };
    app.mouse(click_log);
    assert!(app.pane == Pane::Logs);
    app.dialog = Some(Dialog::Apply(Vec::new()));
    app.mouse(MouseEvent {
        column: app.tree_rows.x,
        row: app.tree_rows.y,
        ..click_log
    });
    assert!(app.pane == Pane::Logs && matches!(app.dialog, Some(Dialog::Apply(_))));
    assert!(app.job.is_none() && !app.editor.as_ref().unwrap().dirty);
}

#[test]
fn form_sections_have_equal_gaps_after_wrapping() {
    for width in [80, 120] {
        let mut app = App::new(Source::File("unused.icf".into()), "unused".into());
        app.dialog = Some(Dialog::Form(Form {
            kind: FormKind::Group,
            title: "Spacing".into(),
            hint: format!("{}HINT-END", "Header explanation ".repeat(6)),
            error: Some(format!("{}NOTE-END", "Footer explanation ".repeat(6))),
            fields: vec![
                Field::text("Label A", "value-a", 16),
                Field::text("Label B", "value-b", 16),
            ],
            selected: 0,
            replace: true,
        }));
        let mut terminal = Terminal::new(TestBackend::new(width, 32)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let rows = terminal
            .backend()
            .buffer()
            .content
            .chunks(width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>();
        let line = |text: &str| rows.iter().position(|row| row.contains(text)).unwrap();
        assert_eq!(line("Label A") - line("HINT-END") - 1, 1);
        assert_eq!(line("Footer explanation") - line("value-b") - 1, 1);
    }
}

#[test]
fn mocha_render_uses_terminal_background_and_keeps_semantic_states() {
    let terminal_bg = ratatui::style::Color::Reset;
    let palette = [
        theme::CRUST,
        theme::SURFACE0,
        theme::TEXT,
        theme::SUBTEXT0,
        theme::SUBTEXT1,
        theme::OVERLAY0,
        theme::BLUE,
        theme::MAUVE,
        theme::LAVENDER,
        theme::GREEN,
        theme::YELLOW,
        theme::RED,
    ];
    let check_palette = |buffer: &ratatui::buffer::Buffer| {
        for cell in &buffer.content {
            assert!(
                palette.contains(&cell.fg),
                "foreground outside Mocha: {:?}",
                cell.fg
            );
            assert!(
                cell.bg == terminal_bg || [theme::SURFACE0, theme::CRUST].contains(&cell.bg),
                "background outside terminal default or selection: {:?}",
                cell.bg
            );
        }
    };
    let find_message = |buffer: &ratatui::buffer::Buffer, text: &str| {
        let matches = buffer
            .content
            .windows(text.len())
            .enumerate()
            .filter_map(|(index, cells)| {
                cells
                    .iter()
                    .zip(text.bytes())
                    .all(|(cell, byte)| cell.symbol().as_bytes() == [byte])
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "message must appear once: {text}");
        matches[0]
    };
    let mut app = App::new(Source::File("unused.icf".into()), "unused".into());
    app.finish(Ok(Finished::Loaded(snapshot(5)))).unwrap();
    app.editor
        .as_mut()
        .unwrap()
        .edit(0, 0, "Modified", "80")
        .unwrap();
    app.pane = Pane::Items;
    app.set_status(Level::Error, "Palette status");
    let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    check_palette(buffer);
    let selected = &buffer[(app.item_rows.x, app.item_rows.y)];
    assert_eq!((selected.fg, selected.bg), (theme::YELLOW, theme::SURFACE0));
    let inactive = &buffer[(app.pane_areas[0].x, app.pane_areas[0].y)];
    assert_eq!((inactive.fg, inactive.bg), (theme::OVERLAY0, terminal_bg));
    let active = &buffer[(app.pane_areas[1].x, app.pane_areas[1].y)];
    assert_eq!((active.fg, active.bg), (theme::LAVENDER, terminal_bg));
    assert_eq!(buffer[(app.log_rows.x, app.log_rows.y)].bg, terminal_bg);
    assert_eq!(
        buffer[(app.item_rows.x, app.item_rows.y + 1)].bg,
        terminal_bg
    );
    assert_eq!(buffer[(0, 0)].bg, terminal_bg);
    assert_eq!(buffer[(0, 31)].bg, terminal_bg);
    let status = &buffer.content[find_message(buffer, "Palette status")];
    assert_eq!((status.fg, status.bg), (theme::RED, terminal_bg));

    app.dialog = Some(Dialog::Form(Form {
        kind: FormKind::Edit,
        title: "Palette form".into(),
        hint: "Form explanation".into(),
        error: Some("Palette error".into()),
        fields: vec![Field::text("Name", "\u{6771}\u{4eac}", 16)],
        selected: 0,
        replace: true,
    }));
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    check_palette(buffer);
    let error = find_message(buffer, "Palette error");
    assert_eq!(
        (buffer.content[error].fg, buffer.content[error].bg),
        (theme::RED, terminal_bg)
    );
}

#[test]
fn tui_empty_enter_and_ctrl_s_stage_without_implicit_copy_or_io() {
    let source = snapshot(5);
    let original = source.native.clone();
    let mut app = App::new(Source::File("unused.icf".into()), "unused".into());
    app.finish(Ok(Finished::Loaded(source))).unwrap();
    app.pane = Pane::Items;
    app.row = 1;
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        app.dialog,
        Some(Dialog::Form(Form {
            kind: FormKind::New,
            ..
        }))
    ));
    text(&mut app, "\u{6771}\u{4eac}FM");
    press(&mut app, KeyCode::Tab);
    text(&mut app, "80");
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Right); // FM to WFM, explicitly selected.
    press(&mut app, KeyCode::Enter);
    let editor = app.editor.as_ref().unwrap();
    let row = editor.row(0, 1).unwrap().unwrap();
    assert_eq!(editor.text(row.name()), "\u{6771}\u{4eac}FM");
    assert_eq!(row.channel_field(ChannelField::FrequencyHz), 80_000_000);
    assert_eq!(row.mode(), cs705::icf::record::decode_mode(8));
    assert_eq!(row.offset_hz(), 0);
    assert_eq!(editor.original.native, original);
    assert!(editor.dirty && app.job.is_none() && app.dialog.is_none());
    press(&mut app, KeyCode::F(5));
    assert!(app.dialog.is_none());
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(matches!(app.dialog, Some(Dialog::Apply(_))) && app.job.is_none());
    press(&mut app, KeyCode::Esc);

    app.row = 2;
    press(&mut app, KeyCode::Enter);
    text(&mut app, "Another");
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(matches!(
        app.dialog,
        Some(Dialog::Form(Form { error: Some(_), .. }))
    ));
    assert!(app.editor.as_ref().unwrap().row(0, 2).unwrap().is_none());
    press(&mut app, KeyCode::Tab);
    text(&mut app, "145.5");
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(matches!(app.dialog, Some(Dialog::Apply(_))) && app.job.is_none());
    assert!(app.editor.as_ref().unwrap().row(0, 2).unwrap().is_some());
}

#[test]
fn busy_session_ignores_edits_and_cancels_without_losing_the_draft() {
    let mut app = App::new(Source::File("unused".into()), "unused".into());
    app.finish(Ok(Finished::Loaded(snapshot(6)))).unwrap();
    app.editor
        .as_mut()
        .unwrap()
        .edit(0, 0, "Draft", "80")
        .unwrap();
    let before = app.editor.as_ref().unwrap().candidate().unwrap().native;
    let (sender, receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    app.job = Some(Job {
        receiver,
        logs: mpsc::channel().1,
        thread: thread::spawn(|| {}),
        cancelled: cancelled.clone(),
        write_started: Arc::new(AtomicBool::new(false)),
    });
    press(&mut app, KeyCode::Char('e'));
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    app.paste("must not edit");
    assert!(app.dialog.is_none());
    press(&mut app, KeyCode::Esc);
    assert!(cancelled.load(Ordering::Relaxed));
    sender.send(Err("cancelled".into())).unwrap();
    app.poll();
    assert!(app.job.is_none() && !app.needs_verification());
    assert_eq!(
        app.editor.as_ref().unwrap().candidate().unwrap().native,
        before
    );
    assert!(app.editor.as_ref().unwrap().dirty);
}

#[test]
fn modified_confirmation_and_command_keys_never_send_or_become_form_text() {
    let mut app = App::new(Source::File("unused".into()), "unused".into());
    app.finish(Ok(Finished::Loaded(snapshot(6)))).unwrap();
    app.pane = Pane::Items;
    for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
        app.dialog = Some(Dialog::Apply(Vec::new()));
        app.key(KeyEvent::new(KeyCode::Char('y'), modifiers));
        assert!(app.job.is_none());
    }
    press(&mut app, KeyCode::Enter);
    let Some(Dialog::Form(form)) = &app.dialog else {
        panic!("expected memory form");
    };
    let original = form.fields[0].value.clone();
    app.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
    let Some(Dialog::Form(form)) = &app.dialog else {
        panic!("expected memory form");
    };
    assert_eq!(form.fields[0].value, original);
}

#[test]
fn rendering_small_terminals_and_log_eviction_keep_navigation_bounded() {
    let mut app = App::new(Source::File("unused".into()), "unused".into());
    app.finish(Ok(Finished::Loaded(snapshot(6)))).unwrap();
    for (width, height) in [(0, 0), (1, 1), (71, 17), (72, 18), (120, 32)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        if width < 72 || height < 18 {
            app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
            assert!(app.dialog.is_none());
        }
    }
    for index in 0..600 {
        app.push_log(LogEntry::new(Level::Info, format!("Event {index}")));
    }
    assert_eq!(app.logs.len(), 500);
    assert_eq!(app.logs.front().unwrap().message, "Event 100");
    app.pane = Pane::Logs;
    press(&mut app, KeyCode::Up);
    let scroll = app.log_scroll;
    app.push_log(LogEntry::new(Level::Warning, "New event"));
    assert_eq!(app.log_scroll, scroll + 1);
    press(&mut app, KeyCode::Home);
    assert!(app.log_scroll < 500);
    press(&mut app, KeyCode::End);
    assert_eq!((app.log_scroll, app.log_horizontal), (0, 0));
}

#[test]
fn wrapped_dialogs_scroll_to_their_real_end_at_the_minimum_terminal_size() {
    let mut app = App::new(Source::File("unused".into()), "unused".into());
    app.finish(Ok(Finished::Loaded(snapshot(6)))).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(72, 18)).unwrap();
    for (dialog, last_line) in [
        (Dialog::Help(0), "fields."),
        (
            Dialog::Diff(
                vec![format!(
                    "{}\nLAST-CHANGE",
                    "Long wrapped change ".repeat(200)
                )],
                0,
            ),
            "LAST-CHANGE",
        ),
    ] {
        app.dialog = Some(dialog);
        terminal.draw(|frame| app.draw(frame)).unwrap();
        press(&mut app, KeyCode::End);
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains(last_line));
        let limit = app.dialog_scroll_limit(app.dialog.as_ref().unwrap());
        for _ in 0..4 {
            press(&mut app, KeyCode::PageDown);
        }
        let scroll = match app.dialog.as_ref().unwrap() {
            Dialog::Diff(_, scroll) | Dialog::Help(scroll) => *scroll,
            _ => panic!("expected scrollable dialog"),
        };
        assert_eq!(scroll, limit);
    }
}
