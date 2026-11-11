use super::*;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Clear, List, ListItem, Padding, Paragraph, Row, Table, Wrap},
};

impl App {
    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        self.size_ok = area.width >= 72 && area.height >= 18;
        self.pane_areas = [Rect::default(); 3];
        self.tree_rows = Rect::default();
        self.item_rows = Rect::default();
        self.log_rows = Rect::default();
        frame.render_widget(Paragraph::new("").style(theme::base()), area);
        if !self.size_ok {
            frame.render_widget(Paragraph::new("Resize to at least 72 columns and 18 rows. Editing is paused. Escape cancels the current dialog.")
                .style(theme::base()).wrap(Wrap { trim: false }), area);
            return;
        }
        let layout = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(7),
            Constraint::Length(1),
        ])
        .split(area);
        let (phase, color) = if self.job.is_some() {
            ("BUSY", theme::YELLOW)
        } else if self.pending.is_some() {
            ("RESTART REQUIRED", theme::YELLOW)
        } else if self.uncertain {
            ("WRITE UNCONFIRMED", theme::RED)
        } else if self.editor.is_none() {
            ("NOT LOADED", theme::RED)
        } else if self.editor.as_ref().is_some_and(|e| e.dirty) {
            ("LOCAL CHANGES", theme::YELLOW)
        } else {
            ("CLEAN", theme::GREEN)
        };
        let source = match &self.source {
            Source::File(path) => path.display().to_string(),
            Source::Radio(radio) => radio.port.clone(),
        };
        let header =
            Layout::horizontal([Constraint::Min(1), Constraint::Length(20)]).split(layout[0]);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    " CS-705 ",
                    theme::base().fg(theme::MAUVE).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(
                    " {}  {}",
                    self.encoding,
                    encoding::safe_text(&source)
                )),
            ]))
            .style(theme::base()),
            header[0],
        );
        frame.render_widget(
            Paragraph::new(phase)
                .right_aligned()
                .style(theme::base().fg(color)),
            header[1],
        );
        let columns =
            Layout::horizontal([Constraint::Length(30), Constraint::Min(30)]).split(layout[1]);
        self.pane_areas = [columns[0], columns[1], layout[2]];
        self.draw_categories(frame, columns[0]);
        self.draw_items(frame, columns[1]);
        self.draw_logs(frame, layout[2]);
        self.draw_legend(frame, layout[3]);
        if let Some(dialog) = &self.dialog {
            let overlay = Rect::new(
                layout[1].x,
                layout[1].y,
                layout[1].width,
                layout[1].height + layout[2].height,
            );
            self.draw_dialog(frame, overlay, dialog);
        }
    }

    fn draw_categories(&mut self, frame: &mut Frame, area: Rect) {
        let nodes = self.nodes();
        self.navigation = self.navigation.min(nodes.len().saturating_sub(1));
        self.groups.select(Some(self.navigation));
        let count = format!("{} of {}", self.navigation + 1, nodes.len());
        let items = nodes
            .into_iter()
            .map(|node| {
                // These triangles convey expansion state, not cursor position.
                let disclosure = if node.id.branch() {
                    if self.expanded.contains(&node.id) {
                        "\u{25be} "
                    } else {
                        "\u{25b8} "
                    }
                } else {
                    "  "
                };
                ListItem::new(format!(
                    "{}{disclosure}{}",
                    "  ".repeat(node.depth),
                    node.label
                ))
                .style(Style::default().fg(if node.depth == 0 {
                    theme::MAUVE
                } else {
                    theme::TEXT
                }))
            })
            .collect::<Vec<_>>();
        let block = theme::panel("[1] Settings", self.pane == Pane::Categories)
            .title_bottom(Line::from(count).right_aligned());
        self.tree_rows = block.inner(area);
        frame.render_stateful_widget(
            List::new(items)
                .style(theme::base())
                .block(block)
                .highlight_style(theme::selected(self.pane == Pane::Categories)),
            area,
            &mut self.groups,
        );
    }

    fn page_title(&self) -> String {
        if let Some(section) = self.section {
            return section.title().into();
        }
        if self.category == Category::Memory
            && let Some(editor) = &self.editor
        {
            return format!("Memory CH / {:02} {}", self.group, editor.group(self.group))
                .trim_end()
                .into();
        }
        match self.row_group {
            Some(group) => format!("{} / {group:02}", self.category.title()),
            None => self.category.title().into(),
        }
    }
    fn draw_items(&mut self, frame: &mut Frame, area: Rect) {
        let visible = self.visible();
        self.rows.select((!visible.is_empty()).then_some(self.row));
        let count = if self.filter.is_empty() && !self.filtering {
            if visible.is_empty() {
                "0 items".into()
            } else {
                format!("{} of {}", self.row + 1, visible.len())
            }
        } else {
            format!("Filter: {}   {} matches", self.filter, visible.len())
        };
        let block = theme::panel(
            format!("[2] {}", self.page_title()),
            self.pane == Pane::Items,
        )
        .title_bottom(Line::from(count).right_aligned());
        if self.editor.is_none() {
            let body = if self.job.is_some() {
                "Reading source. No data is available yet.".into()
            } else {
                format!(
                    "No image loaded.\n\n{}\n\nCorrect the source and press r to reload.",
                    self.source_error.as_deref().unwrap_or(&self.status)
                )
            };
            frame.render_widget(
                Paragraph::new(body)
                    .style(theme::base().fg(if self.job.is_some() {
                        theme::SUBTEXT0
                    } else {
                        theme::RED
                    }))
                    .wrap(Wrap { trim: false })
                    .block(block),
                area,
            );
            return;
        }
        if let Some(error) = &self.load_error {
            frame.render_widget(
                Paragraph::new(format!(
                    "Cannot open this category.\n\n{error}\n\nNo data was replaced."
                ))
                .style(theme::base().fg(theme::RED))
                .wrap(Wrap { trim: false })
                .block(block),
                area,
            );
            return;
        }
        if let Some(section) = self.section {
            self.item_rows = table_body(block.inner(area));
            let rows = section
                .categories()
                .iter()
                .map(|category| Row::new(vec![category.title(), category.description()]))
                .collect::<Vec<_>>();
            frame.render_stateful_widget(
                Table::new(rows, [Constraint::Percentage(40), Constraint::Min(15)])
                    .header(
                        Row::new(["Editor", "Contents"])
                            .style(Style::default().fg(theme::SUBTEXT1)),
                    )
                    .block(block)
                    .row_highlight_style(theme::selected(self.pane == Pane::Items)),
                area,
                &mut self.rows,
            );
            return;
        }
        if visible.is_empty() {
            let message = if !self.filter.is_empty() {
                "No matches. Escape clears the filter."
            } else {
                "No existing rows in this group."
            };
            frame.render_widget(
                Paragraph::new(message)
                    .style(theme::base().fg(theme::SUBTEXT0))
                    .wrap(Wrap { trim: false })
                    .block(block),
                area,
            );
            return;
        }
        if self.category == Category::Memory {
            self.item_rows = table_body(block.inner(area));
            let editor = self.editor.as_ref().expect("loaded editor");
            let rows = visible
                .into_iter()
                .map(|row| match editor.row(self.group, row) {
                    Ok(Some(h)) => {
                        let changed = editor
                            .original_row(self.group, row)
                            .is_ok_and(|old| old.as_ref() != Some(&h));
                        Row::new(vec![
                            format!("{row:02}"),
                            self.text(h.name()),
                            model::mhz_text(h.channel_field(ChannelField::FrequencyHz)),
                            format!(
                                "{}{}",
                                model::mode_name(h.mode()),
                                if h.split() != 0 { " / SPLIT" } else { "" }
                            ),
                            if changed {
                                "Modified".into()
                            } else {
                                String::new()
                            },
                        ])
                        .style(Style::default().fg(if changed {
                            theme::YELLOW
                        } else {
                            theme::TEXT
                        }))
                    }
                    Ok(None) => Row::new(vec![
                        format!("{row:02}"),
                        "Empty".into(),
                        String::new(),
                        String::new(),
                        String::new(),
                    ])
                    .style(Style::default().fg(theme::SUBTEXT0)),
                    Err(error) => Row::new(vec![
                        format!("{row:02}"),
                        encoding::safe_text(&error.to_string()),
                        String::new(),
                        String::new(),
                        "Error".into(),
                    ])
                    .style(Style::default().fg(theme::RED)),
                })
                .collect::<Vec<_>>();
            frame.render_stateful_widget(
                Table::new(
                    rows,
                    [
                        Constraint::Length(3),
                        Constraint::Min(10),
                        Constraint::Length(11),
                        Constraint::Length(12),
                        Constraint::Length(9),
                    ],
                )
                .header(
                    Row::new(["CH", "Name", "RX MHz", "Mode", "State"])
                        .style(Style::default().fg(theme::SUBTEXT1)),
                )
                .block(block)
                .row_highlight_style(theme::selected(self.pane == Pane::Items)),
                area,
                &mut self.rows,
            );
        } else {
            self.item_rows = table_body(block.inner(area));
            let rows = visible
                .into_iter()
                .map(|index| {
                    let entry = &self.entries[index];
                    let state = if entry.changed() {
                        "Modified"
                    } else if !entry.editable {
                        "Read only"
                    } else {
                        ""
                    };
                    Row::new(vec![entry.label.clone(), entry.value.clone(), state.into()]).style(
                        Style::default().fg(if entry.changed() {
                            theme::YELLOW
                        } else if !entry.editable {
                            theme::SUBTEXT0
                        } else {
                            theme::TEXT
                        }),
                    )
                })
                .collect::<Vec<_>>();
            frame.render_stateful_widget(
                Table::new(
                    rows,
                    [
                        Constraint::Percentage(46),
                        Constraint::Min(12),
                        Constraint::Length(9),
                    ],
                )
                .header(
                    Row::new(["Setting / Row", "Draft value", "State"])
                        .style(Style::default().fg(theme::SUBTEXT1)),
                )
                .block(block)
                .row_highlight_style(theme::selected(self.pane == Pane::Items)),
                area,
                &mut self.rows,
            );
        }
    }

    fn draw_logs(&mut self, frame: &mut Frame, area: Rect) {
        let block = theme::panel("[3] Activity log (UTC)", self.pane == Pane::Logs).title_bottom(
            Line::from(if self.log_scroll == 0 {
                "Live"
            } else {
                "History"
            })
            .right_aligned(),
        );
        self.log_rows = block.inner(area);
        let last = self
            .logs
            .len()
            .saturating_sub(self.log_rows.height as usize);
        self.log_scroll = self.log_scroll.min(last);
        let start = last - self.log_scroll;
        let lines = self
            .logs
            .iter()
            .skip(start)
            .take(self.log_rows.height as usize)
            .map(|entry| {
                let (level, color) = match entry.level {
                    Level::Info => ("INFO", theme::BLUE),
                    Level::Warning => ("WARN", theme::YELLOW),
                    Level::Error => ("ERROR", theme::RED),
                };
                Line::from(vec![
                    Span::styled(
                        format!("{} ", entry.clock()),
                        Style::default().fg(theme::SUBTEXT0),
                    ),
                    Span::styled(format!("{level:<5}  "), Style::default().fg(color)),
                    Span::styled(
                        entry.message.clone(),
                        Style::default().fg(match entry.level {
                            Level::Info => theme::TEXT,
                            _ => color,
                        }),
                    ),
                ])
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(lines)
                .style(theme::base())
                .scroll((0, self.log_horizontal))
                .block(block),
            area,
        );
    }

    fn detail(&self) -> Result<Vec<Line<'static>>> {
        let index = self.selected().ok_or("no selected item")?;
        let (title, original, current, help, editable) = if self.category == Category::Memory {
            let editor = self.editor.as_ref().ok_or("no image loaded")?;
            let describe = |h: Option<cs705::icf::record::DecodedRecord>| {
                h.map_or_else(
                    || "Empty".into(),
                    |h| {
                        format!(
                            "{} | {} MHz | {}{}",
                            self.text(h.name()),
                            model::mhz_text(h.channel_field(ChannelField::FrequencyHz)),
                            model::mode_name(h.mode()),
                            if h.split() != 0 { " / SPLIT" } else { "" }
                        )
                    },
                )
            };
            (format!("Memory {:02}/{index:02}", self.group), describe(editor.original_row(self.group, index)?), describe(editor.row(self.group, index)?),
                "Enter on Empty creates a new simplex channel from name, frequency and mode. Occupied rows edit name and Main/RX frequency; other settings are preserved. Copy uses an explicitly selected occupied row.".into(), true)
        } else {
            let entry = &self.entries[index];
            (
                entry.label.clone(),
                entry.original.clone(),
                entry.value.clone(),
                entry.help.clone(),
                entry.editable,
            )
        };
        Ok(vec![
            Line::styled(
                title,
                Style::default()
                    .fg(theme::LAVENDER)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::styled(
                if editable { "Editable" } else { "Read only" },
                Style::default().fg(theme::SUBTEXT1),
            ),
            Line::from(""),
            Line::styled("Original", Style::default().fg(theme::SUBTEXT1)),
            Line::from(original.clone()),
            Line::from(""),
            Line::styled(
                "Draft",
                Style::default().fg(if original == current {
                    theme::GREEN
                } else {
                    theme::YELLOW
                }),
            ),
            Line::from(current),
            Line::from(""),
            Line::from(help),
        ])
    }

    fn draw_legend(&self, frame: &mut Frame, area: Rect) {
        let (actions, common): (Vec<_>, Vec<_>) = match &self.dialog {
            Some(Dialog::Form(form)) => {
                let mut keys = vec![
                    ("Tab", "Field"),
                    (
                        "Enter",
                        if matches!(form.kind, FormKind::Encoding) {
                            "Use encoding"
                        } else {
                            "Stage edit"
                        },
                    ),
                ];
                if form.fields[form.selected].choices.is_empty() {
                    keys.extend([("Type", "Replace"), ("Ctrl+U", "Clear")]);
                } else {
                    keys.push(("Left/Right", "Choice"));
                }
                (keys, vec![("Esc", "Cancel")])
            }
            Some(Dialog::Help(_)) | Some(Dialog::Diff(..)) => (
                vec![("Up/Down", "Scroll"), ("PgUp/PgDn", "Page")],
                vec![("Esc", "Close")],
            ),
            Some(Dialog::Details(_)) => (
                vec![("Enter", "Edit"), ("Up/Down", "Scroll")],
                vec![("Esc", "Close")],
            ),
            Some(_) => (vec![("y", "Confirm")], vec![("Esc", "Cancel")]),
            None if self.job.is_some() => (
                vec![("Up/Down", "Browse"), ("End", "Live log")],
                vec![("Tab", "Pane"), ("Esc", "Cancel operation")],
            ),
            None if self.filtering => (
                vec![("Enter", "Keep filter"), ("Tab", "Pane")],
                vec![("Esc", "Clear")],
            ),
            None => {
                let mut keys = if self.pane == Pane::Logs {
                    vec![
                        ("Up/Down", "Scroll"),
                        ("Left/Right", "Pan"),
                        ("End", "Live"),
                    ]
                } else if self.pane == Pane::Categories {
                    vec![("Enter", "Open/Fold"), ("Left/Right", "Tree")]
                } else if self.section.is_some() {
                    vec![("Enter", "Open")]
                } else {
                    vec![
                        (
                            "Enter",
                            if self.category == Category::Memory {
                                "New/Edit"
                            } else {
                                "Edit"
                            },
                        ),
                        ("i", "Details"),
                        ("/", "Filter"),
                    ]
                };
                if self.pending.is_some() {
                    keys.push(("v", "Verify"));
                } else if self.uncertain || self.editor.is_none() {
                    keys.push(("r", "Reload"));
                } else {
                    keys.insert(
                        0,
                        (
                            "Ctrl+S",
                            match self.source {
                                Source::Radio(_) => "Send all",
                                Source::File(_) => "Save all",
                            },
                        ),
                    );
                    keys.push(("d", "Changes"));
                    if self.pane == Pane::Items
                        && self.section.is_none()
                        && self.category == Category::Memory
                    {
                        keys.extend([("c", "Copy"), ("g", "Group")]);
                    }
                    keys.extend([("x", "Discard"), ("r", "Reload")]);
                }
                (
                    keys,
                    vec![
                        ("E", "Encoding"),
                        ("Tab", "Pane"),
                        ("?", "Help"),
                        ("q", "Quit"),
                    ],
                )
            }
        };
        let common_width = common
            .iter()
            .map(|(key, text)| key.len() + text.len() + 3)
            .sum::<usize>() as u16;
        let zones = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(common_width.min(area.width)),
        ])
        .split(area);
        frame.render_widget(
            Paragraph::new(key_legend(&actions, zones[0].width)).style(theme::base()),
            zones[0],
        );
        frame.render_widget(
            Paragraph::new(key_legend(&common, zones[1].width)).style(theme::base()),
            zones[1],
        );
    }

    fn draw_dialog(&self, frame: &mut Frame, area: Rect, dialog: &Dialog) {
        if let Dialog::Form(form) = dialog {
            let hint = Paragraph::new(form.hint.as_str())
                .style(theme::base().fg(theme::SUBTEXT1))
                .wrap(Wrap { trim: false });
            let note = Paragraph::new(form.error.as_deref().unwrap_or(
                "Enter stages locally. Ctrl+S reviews ALL staged changes before sending or saving.",
            ))
            .style(theme::base().fg(if form.error.is_some() {
                theme::RED
            } else {
                theme::SUBTEXT1
            }))
            .wrap(Wrap { trim: false });
            // Measure with the same wrapper and width used to render. Borders
            // and padding use four columns/rows; section gaps use two rows.
            let text_width = 96.min(area.width.saturating_sub(4)).saturating_sub(4);
            let hint_height = hint.line_count(text_width) as u16;
            let note_height = note.line_count(text_width) as u16;
            let height = hint_height + note_height + (form.fields.len() * 2) as u16 + 6;
            let popup = centered(area, 96, height);
            frame.render_widget(Clear, popup);
            let block =
                theme::panel(format!("Edit / {}", form.title), true).padding(Padding::uniform(1));
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let zones = Layout::vertical([
                Constraint::Length(hint_height),
                Constraint::Length(1),
                Constraint::Min(2),
                Constraint::Length(1),
                Constraint::Length(note_height),
            ])
            .split(inner);
            frame.render_widget(hint, zones[0]);
            let fields = form
                .fields
                .iter()
                .map(|field| {
                    ListItem::new(vec![
                        Line::styled(field.label.clone(), Style::default().fg(theme::SUBTEXT1)),
                        Line::styled(
                            format!("  {}", field.display()),
                            Style::default().fg(theme::TEXT),
                        ),
                    ])
                })
                .collect::<Vec<_>>();
            let mut state = ListState::default().with_selected(Some(form.selected));
            frame.render_stateful_widget(
                List::new(fields)
                    .style(theme::base())
                    .highlight_style(theme::selected(true)),
                zones[2],
                &mut state,
            );
            frame.render_widget(note, zones[4]);
            return;
        }
        let popup = dialog_popup(area, dialog);
        frame.render_widget(Clear, popup);
        let (title, body, scroll) = self.dialog_content(dialog);
        frame.render_widget(
            Paragraph::new(body)
                .style(theme::base())
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0))
                .block(theme::panel(title, true)),
            popup,
        );
    }

    pub(super) fn dialog_scroll_limit(&self, dialog: &Dialog) -> u16 {
        let area = self.pane_areas[0]
            .union(self.pane_areas[1])
            .union(self.pane_areas[2]);
        let inner = theme::panel("", true).inner(dialog_popup(area, dialog));
        let (_, body, _) = self.dialog_content(dialog);
        // Count wrapped display lines, not newlines or a fixed terminal height.
        Paragraph::new(body)
            .wrap(Wrap { trim: false })
            .line_count(inner.width)
            .saturating_sub(usize::from(inner.height))
            .min(u16::MAX as usize) as u16
    }

    fn dialog_content(&self, dialog: &Dialog) -> (&'static str, Text<'static>, u16) {
        if let Dialog::Details(scroll) = dialog {
            let lines = self.detail().unwrap_or_else(|error| {
                vec![Line::styled(
                    encoding::safe_text(&error.to_string()),
                    Style::default().fg(theme::RED),
                )]
            });
            return ("Details", lines.into(), *scroll);
        }
        let (title, body, scroll): (_, String, _) = match dialog {
            Dialog::Diff(lines, scroll) => ("Local changes", if lines.is_empty() { "No local changes.".into() } else { lines.join("\n\n") }, *scroll),
            Dialog::Apply(changes) => (match self.source { Source::Radio(_) => "Send ALL staged changes to the radio", Source::File(_) => "Save ALL staged changes to a new ICF" }, format!("{}\n\nThis is the only send/save step; Enter in an editor never transmits.\nA private pre-write backup is mandatory. No automatic retry.\nRadio writes require a physical restart, then Verify.\nAll edited categories are included. This is not RF safety certification.\n\n{} changes. Cancel and use d to see the complete diff.\n\n{}", match self.source { Source::File(_) => "Save a NEW edited ICF. The original is never overwritten.", Source::Radio(_) => "Fresh-read conflict check, backup, then ONE clone write." }, changes.len(), changes.iter().take(3).cloned().collect::<Vec<_>>().join("\n\n")), 0),
            Dialog::Verify => ("Confirm physical restart", "Has the radio been power-cycled and returned to its normal screen?\nConfirm only after this manual step. Verify reads; it never writes.".into(), 0),
            Dialog::Discard => ("Discard all local edits", "Every category will return to the loaded snapshot. No device write.".into(), 0),
            Dialog::Reload => ("Reload source", "Read the source again and discard the draft. No write or automatic recovery.\nAfter a write failure, inspect and restart the device first.".into(), 0),
            Dialog::Quit => ("Quit and lose local edits", "A pending or unconfirmed radio write is NOT undone.\nKeep the backup and verify the device before further writes.".into(), 0),
            Dialog::Help(scroll) => ("Keyboard and mouse", "NAVIGATION\n  Tab / Shift-Tab   Cycle tree, main view and activity log\n  1 / 2 / 3         Focus tree / main view / activity log\n  Left click        Focus a pane and select the clicked row\n  Click triangle    Expand or collapse a tree branch\n  Up / Down         Move inside the focused pane\n  PgUp / PgDn       Page the focused view\n  Enter / Space     Expand or collapse a tree branch\n  Right / Left      Enter a child / return to the parent\n  Enter             New channel on Empty; edit an occupied row\n  i                 Open item details; Escape closes them\n  /                 Filter the item list; Enter keeps the filter\n  E                 Select storage encoding; bytes are not transcoded\n  Escape            Clear filter and return to the tree\n\nACTIVITY LOG\n  Up / Down, wheel  Browse history, including during communication\n  Left / Right      Pan long messages horizontally\n  Home / End        Oldest entries / resume live following\n  Up to 500 events are kept in memory. Times are UTC.\n  Radio payloads, names and credentials are not dumped.\n\nEDIT SESSION\n  Enter in a form   Stage locally ONLY; never send to the device\n  Ctrl+S            Stage the current form, then review ALL changes\n  y                 Confirm batch save or radio write\n  c Copy memory row   g Rename memory group\n  d Changes   x Discard all   r Reload   v Verify   q Quit\n  Yellow text / Modified: staged edits. Read only: no editor.\n\nNew channels are explicit simplex analog entries, not factory defaults.\nNames use the selected encoding; capacity is measured in stored bytes.\nForms: Tab changes field, typing replaces text, Escape cancels.\nLeft/Right chooses a value. Paste is accepted only inside text fields.".into(), *scroll),
            Dialog::Form(_) | Dialog::Details(_) => unreachable!("handled above"),
        };
        (title, body.into(), scroll)
    }
}

fn dialog_popup(area: Rect, dialog: &Dialog) -> Rect {
    let height = match dialog {
        Dialog::Details(_) => 18,
        Dialog::Help(_) => 26,
        Dialog::Diff(..) => area.height,
        Dialog::Apply(_) => 24,
        _ => 12,
    };
    centered(area, 96, height)
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4));
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}
fn table_body(inner: Rect) -> Rect {
    // All item tables have a one-line header and single-line rows.
    Rect::new(
        inner.x,
        inner.y.saturating_add(1),
        inner.width,
        inner.height.saturating_sub(1),
    )
}
fn key_legend(hints: &[(&str, &str)], width: u16) -> Line<'static> {
    let mut spans = Vec::new();
    let mut used = 0;
    for &(key, text) in hints {
        let length = key.len() + text.len() + 3;
        if used + length > width as usize {
            break;
        }
        spans.push(Span::styled(
            format!(" {key} "),
            theme::base().fg(theme::BLUE).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{text} "),
            theme::base().fg(theme::SUBTEXT1),
        ));
        used += length;
    }
    Line::from(spans)
}
