use super::*;

impl App {
    pub(super) fn mouse(&mut self, mouse: MouseEvent) {
        if !matches!(
            mouse.kind,
            MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
        ) || self.dialog.is_some()
            || !self.size_ok
        {
            return;
        }
        let position = Position::new(mouse.column, mouse.row);
        if self.pane_areas[2].contains(position) {
            self.pane = Pane::Logs;
            self.filtering = false;
            match mouse.kind {
                MouseEventKind::ScrollUp => self.navigate(KeyCode::Up),
                MouseEventKind::ScrollDown => self.navigate(KeyCode::Down),
                _ => {}
            }
            return;
        }
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        if self.pane_areas[0].contains(position) {
            self.pane = Pane::Categories;
            self.filtering = false;
            if !self.tree_rows.contains(position) {
                return;
            }
            let index = self.groups.offset() + usize::from(mouse.row - self.tree_rows.y);
            let nodes = self.nodes();
            let Some(node) = nodes.get(index) else {
                return;
            };
            let disclosure = self.tree_rows.x + (node.depth * 2) as u16;
            let toggle = node.id.branch() && (disclosure..disclosure + 2).contains(&mouse.column);
            if self.navigation != index {
                self.navigation = index;
                self.select_node();
            }
            if toggle {
                self.tree_key(KeyCode::Enter);
            }
        } else if self.pane_areas[1].contains(position) {
            self.pane = Pane::Items;
            self.filtering = false;
            if !self.item_rows.contains(position) {
                return;
            }
            let index = self.rows.offset() + usize::from(mouse.row - self.item_rows.y);
            if index < self.visible().len() {
                self.row = index;
            }
        }
        // Mouse clicks never invoke an editor, confirm a dialog or start I/O.
    }

    fn focus(&mut self, reverse: bool) {
        self.pane = match (self.pane, reverse) {
            (Pane::Categories, false) | (Pane::Logs, true) => Pane::Items,
            (Pane::Items, false) | (Pane::Categories, true) => Pane::Logs,
            _ => Pane::Categories,
        };
    }
    fn navigate(&mut self, code: KeyCode) {
        if self.pane == Pane::Logs {
            let last = self
                .logs
                .len()
                .saturating_sub(self.log_rows.height as usize);
            let page = usize::from(self.log_rows.height.max(1));
            match code {
                KeyCode::Up => self.log_scroll = (self.log_scroll + 1).min(last),
                KeyCode::Down => self.log_scroll = self.log_scroll.saturating_sub(1),
                KeyCode::PageUp => self.log_scroll = (self.log_scroll + page).min(last),
                KeyCode::PageDown => self.log_scroll = self.log_scroll.saturating_sub(page),
                KeyCode::Home => self.log_scroll = last,
                KeyCode::End => {
                    self.log_scroll = 0;
                    self.log_horizontal = 0;
                }
                KeyCode::Left => self.log_horizontal = self.log_horizontal.saturating_sub(4),
                KeyCode::Right => self.log_horizontal = self.log_horizontal.saturating_add(4),
                _ => {}
            }
            return;
        }
        let len = match self.pane {
            Pane::Categories => self.nodes().len(),
            Pane::Items => self.visible().len(),
            Pane::Logs => unreachable!("log scrolling handled above"),
        };
        let selected = if self.pane == Pane::Categories {
            &mut self.navigation
        } else {
            &mut self.row
        };
        *selected = match code {
            KeyCode::Up => selected.saturating_sub(1),
            KeyCode::Down => (*selected + 1).min(len.saturating_sub(1)),
            KeyCode::PageUp => selected.saturating_sub(10),
            KeyCode::PageDown => (*selected + 10).min(len.saturating_sub(1)),
            KeyCode::Home => 0,
            KeyCode::End => len.saturating_sub(1),
            _ => *selected,
        };
        if self.pane == Pane::Categories {
            self.select_node();
        }
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        let control_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        if control_c {
            self.dialog = None;
            self.filtering = false;
        }
        if let Some(dialog) = self.dialog.take() {
            return self.dialog_key(dialog, key);
        }
        if save_key(key) {
            self.filtering = false;
            return self.review_apply();
        }
        if self.filtering {
            match key.code {
                KeyCode::Esc => {
                    self.filter.clear();
                    self.filtering = false;
                }
                KeyCode::Enter => self.filtering = false,
                KeyCode::Tab | KeyCode::BackTab => {
                    self.filtering = false;
                    self.focus(key.code == KeyCode::BackTab);
                }
                KeyCode::Backspace => {
                    self.filter.pop();
                }
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && !c.is_control()
                        && self.filter.chars().count() < 80 =>
                {
                    self.filter.push(c)
                }
                _ => {}
            }
            self.row = 0;
            return Ok(());
        }
        if key.code == KeyCode::Char('q') || control_c {
            if self.editor.as_ref().is_some_and(|e| e.dirty) || self.needs_verification() {
                self.dialog = Some(Dialog::Quit);
            } else {
                self.quit = true;
            }
            return Ok(());
        }
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => self.focus(key.code == KeyCode::BackTab),
            KeyCode::Char('1') => self.pane = Pane::Categories,
            KeyCode::Char('2') => self.pane = Pane::Items,
            KeyCode::Char('3') => self.pane = Pane::Logs,
            KeyCode::Left | KeyCode::Right if self.pane == Pane::Logs => self.navigate(key.code),
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End => self.navigate(key.code),
            KeyCode::Char('?') | KeyCode::F(1) => self.dialog = Some(Dialog::Help(0)),
            KeyCode::Char('E') => self.encoding_form(),
            KeyCode::Esc => {
                self.filter.clear();
                self.pane = Pane::Categories;
                self.row = 0;
            }
            KeyCode::Char('/')
                if self.pane == Pane::Items && self.section.is_none() && self.editor.is_some() =>
            {
                self.filtering = true
            }
            KeyCode::Char('i')
                if self.pane == Pane::Items
                    && self.section.is_none()
                    && self.selected().is_some() =>
            {
                self.dialog = Some(Dialog::Details(0))
            }
            KeyCode::Char('d') => {
                self.dialog = Some(Dialog::Diff(
                    self.editor.as_ref().ok_or("no image loaded")?.changes()?,
                    0,
                ))
            }
            KeyCode::Char('v') if self.pending.is_some() => self.dialog = Some(Dialog::Verify),
            KeyCode::Char('r') if self.pending.is_none() => self.dialog = Some(Dialog::Reload),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') | KeyCode::Enter
                if self.pane == Pane::Categories =>
            {
                self.tree_key(key.code)
            }
            KeyCode::Enter if self.section.is_some() => {
                let section = self.section.expect("section was checked");
                if let Some(category) = self.selected().and_then(|i| section.categories().get(i)) {
                    self.open_node(NodeId::Category(*category));
                }
            }
            KeyCode::Enter
            | KeyCode::Char('e')
            | KeyCode::Char('c')
            | KeyCode::Char('g')
            | KeyCode::Char('x') => {
                if self.pending.is_some() || self.uncertain {
                    return Err(
                        "write pending or unconfirmed: verify/reload before further edits".into(),
                    );
                }
                match key.code {
                    KeyCode::Char('x') => self.dialog = Some(Dialog::Discard),
                    KeyCode::Char('g')
                        if self.section.is_none() && self.category == Category::Memory =>
                    {
                        self.group_form()?
                    }
                    KeyCode::Char('c')
                        if self.section.is_none()
                            && self.category == Category::Memory
                            && self.pane == Pane::Items =>
                    {
                        self.memory_form(true)?
                    }
                    KeyCode::Enter | KeyCode::Char('e')
                        if self.section.is_none() && self.pane == Pane::Items =>
                    {
                        self.edit_form()?
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn review_apply(&mut self) -> Result<()> {
        if self.pending.is_some() || self.uncertain {
            return Err("write pending or unconfirmed: verify/reload before saving".into());
        }
        if let Some(editor) = &self.editor
            && editor.dirty
        {
            self.dialog = Some(Dialog::Apply(editor.changes()?));
        } else {
            self.set_status(Level::Info, "No local changes to save.");
        }
        Ok(())
    }

    fn open_form(&mut self, kind: FormKind, title: String, hint: String, fields: Vec<Field>) {
        self.dialog = Some(Dialog::Form(Form {
            kind,
            title,
            hint,
            error: None,
            fields,
            selected: 0,
            replace: true,
        }));
    }
    fn encoding_form(&mut self) {
        let mut choices = [
            "shift_jis",
            "utf-8",
            "windows-1252",
            "gbk",
            "big5",
            "euc-kr",
        ]
        .into_iter()
        .map(|name| {
            name.parse::<TextEncoding>()
                .expect("known encoding")
                .to_string()
        })
        .collect::<Vec<_>>();
        let current = self.encoding.to_string();
        if !choices.contains(&current) {
            choices.insert(0, current.clone());
        }
        self.open_form(FormKind::Encoding, "Text encoding".into(),
            "Select the actual encoding of stored text. This changes display and future text input only; it does not transcode stored bytes or transmit anything.".into(),
            vec![Field { label: "Storage encoding".into(), value: current, choices, max_len: 40, choice_key: None }]);
    }

    pub(super) fn paste(&mut self, text: &str) {
        if self.job.is_some() || !self.size_ok {
            return;
        }
        if let Some(Dialog::Form(form)) = &mut self.dialog {
            let field = &mut form.fields[form.selected];
            if !field.choices.is_empty() {
                return;
            }
            let length = if form.replace {
                0
            } else {
                field.value.chars().count()
            } + text.chars().count();
            if text.chars().any(char::is_control) || length > field.max_len {
                form.error = Some("Paste rejected: control characters or too many characters. Nothing was changed.".into());
                return;
            }
            if form.replace {
                field.value.clear();
            }
            field.value.push_str(text);
            form.replace = false;
            form.error = None;
        } else if self.filtering
            && !text.chars().any(char::is_control)
            && self.filter.chars().count() + text.chars().count() <= 80
        {
            self.filter.push_str(text);
            self.row = 0;
        }
        // A paste outside a text field is never interpreted as command keys.
    }
    fn group_form(&mut self) -> Result<()> {
        let editor = self.editor.as_ref().ok_or("no image loaded")?;
        let fields = vec![
            Field::text("Group (00..99)", self.group.to_string(), 2),
            Field::text("Name (16 encoded bytes)", editor.group(self.group), 16),
        ];
        self.open_form(
            FormKind::Group,
            "Rename memory group".into(),
            "An empty group is created explicitly if required. Other rows are not copied.".into(),
            fields,
        );
        Ok(())
    }
    fn memory_form(&mut self, copy: bool) -> Result<()> {
        let row = self.selected().ok_or("no selected row")?;
        let editor = self.editor.as_ref().ok_or("no image loaded")?;
        let Some(h) = editor.row(self.group, row)? else {
            if copy {
                return Err(
                    "Copy requires an occupied source row; use Enter to create a new channel"
                        .into(),
                );
            }
            self.open_form(FormKind::New, format!("New memory {:02}/{row:02}", self.group),
                "New simplex channel. SPLIT, DATA, DUP and tones are off; offset is zero. Filter 1 (WFM: no filter). Both stored sides use your input. No existing channel or factory template is copied.".into(),
                vec![Field::text("Name (16 encoded bytes)", "", 16), Field::text("Receive frequency (MHz)", "", 14),
                    Field { label: "Mode".into(), value: "FM".into(), choices: model::NEW_MODES.iter().map(|(name, _)| (*name).into()).collect(), max_len: 8, choice_key: None }]);
            return Ok(());
        };
        let mut fields = Vec::new();
        if copy {
            let mut free = None;
            for r in 0..100 {
                if editor.row(self.group, r)?.is_none() {
                    free = Some(r);
                    break;
                }
            }
            fields.push(Field::text(
                "Destination group (00..99)",
                self.group.to_string(),
                2,
            ));
            fields.push(Field::text(
                "Empty destination row (00..99)",
                free.map(|r| r.to_string()).unwrap_or_default(),
                2,
            ));
        }
        fields.push(Field::text(
            "Name (16 encoded bytes)",
            self.text(h.name()),
            16,
        ));
        fields.push(Field::text(
            "Main/RX frequency (MHz)",
            model::mhz_text(h.channel_field(ChannelField::FrequencyHz)),
            14,
        ));
        self.open_form(if copy { FormKind::Copy } else { FormKind::Edit }, format!("{} memory {:02}/{row:02}", if copy { "Copy" } else { "Edit" }, self.group), "Mode, SPLIT and the other side are preserved. Copy uses this explicit source row; it never invents defaults.".into(), fields);
        Ok(())
    }
    fn edit_form(&mut self) -> Result<()> {
        if self.category == Category::Memory {
            return self.memory_form(false);
        }
        let entry = self
            .entries
            .get(self.selected().ok_or("no selected item")?)
            .ok_or("no selected item")?;
        if !entry.editable {
            return Err(entry.help.clone().into());
        }
        let fields = catalog::fields(
            &self.editor.as_ref().ok_or("no image loaded")?.settings,
            entry.target,
            self.encoding,
        )?;
        self.open_form(
            FormKind::Setting(entry.target),
            entry.label.clone(),
            entry.help.clone(),
            fields,
        );
        Ok(())
    }
    fn accept_form(&mut self, form: &Form) -> Result<()> {
        if matches!(form.kind, FormKind::Encoding) {
            self.set_encoding(form.fields[0].value.parse()?);
            self.set_status(
                Level::Info,
                format!(
                    "Text encoding: {}. Stored bytes were not transcoded or sent.",
                    self.encoding
                ),
            );
            return Ok(());
        }
        let selected = self.selected();
        let editor = self.editor.as_mut().ok_or("no image loaded")?;
        match form.kind {
            FormKind::Group => {
                let group = form.fields[0].value.parse()?;
                editor.rename(group, &form.fields[1].value)?;
                self.group = group;
            }
            FormKind::Edit => editor.edit(
                self.group,
                selected.ok_or("no selected row")?,
                &form.fields[0].value,
                &form.fields[1].value,
            )?,
            FormKind::New => {
                let mode = model::NEW_MODES
                    .iter()
                    .find(|(name, _)| *name == form.fields[2].value)
                    .ok_or("unsupported new-channel mode")?
                    .1;
                editor.create(
                    self.group,
                    selected.ok_or("no selected row")?,
                    &form.fields[0].value,
                    &form.fields[1].value,
                    mode,
                )?;
            }
            FormKind::Copy => {
                let group = form.fields[0].value.parse()?;
                let row = form.fields[1].value.parse()?;
                editor.copy(
                    (self.group, selected.ok_or("no selected source")?),
                    (group, row),
                    &form.fields[2].value,
                    &form.fields[3].value,
                )?;
                self.group = group;
                self.filter.clear();
                self.row = row;
            }
            FormKind::Setting(target) => catalog::edit(editor, target, &form.fields)?,
            FormKind::Encoding => unreachable!("encoding handled above"),
        }
        if self.category == Category::Memory {
            self.expanded.insert(NodeId::Section(Section::Memories));
            self.expanded.insert(NodeId::Category(Category::Memory));
            self.sync_navigation();
        }
        self.refresh();
        self.set_status(Level::Info, match self.source { Source::Radio(_) => "Edit staged locally. Ctrl+S reviews and sends ALL staged changes; Enter never transmits.", Source::File(_) => "Edit staged locally. Ctrl+S reviews and saves ALL staged changes to a new ICF." });
        Ok(())
    }

    fn dialog_key(&mut self, mut dialog: Dialog, key: KeyEvent) -> Result<()> {
        if key.code == KeyCode::Esc {
            return Ok(());
        }
        if let Dialog::Form(form) = &mut dialog {
            // Ctrl+S stages the active form, then reviews the whole batch.
            // It never bypasses validation or confirms the batch automatically.
            if save_key(key) || key.code == KeyCode::Enter {
                if let Err(error) = self.accept_form(form) {
                    let message = encoding::safe_text(&error.to_string());
                    self.push_log(LogEntry::new(
                        Level::Warning,
                        format!("Edit was not staged: {message}"),
                    ));
                    form.error = Some(message);
                    self.dialog = Some(dialog);
                    return Ok(());
                }
                return if save_key(key) {
                    self.review_apply()
                } else {
                    Ok(())
                };
            }
            let field = &mut form.fields[form.selected];
            match key.code {
                KeyCode::Tab | KeyCode::Down => {
                    form.selected = (form.selected + 1) % form.fields.len();
                    form.replace = true;
                }
                KeyCode::BackTab | KeyCode::Up => {
                    form.selected = (form.selected + form.fields.len() - 1) % form.fields.len();
                    form.replace = true;
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')
                    if !field.choices.is_empty() =>
                {
                    let old = field.choices.iter().position(|v| v == &field.value);
                    let next = match (old, key.code) {
                        (Some(i), KeyCode::Left) => {
                            (i + field.choices.len() - 1) % field.choices.len()
                        }
                        (Some(i), _) => (i + 1) % field.choices.len(),
                        _ => 0,
                    };
                    field.value = field.choices[next].clone();
                }
                KeyCode::Backspace if field.choices.is_empty() => {
                    if form.replace {
                        field.value.clear();
                    } else {
                        field.value.pop();
                    }
                    form.replace = false;
                }
                KeyCode::Char('u')
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && field.choices.is_empty() =>
                {
                    field.value.clear();
                    form.replace = false;
                }
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && !c.is_control()
                        && field.choices.is_empty() =>
                {
                    if form.replace {
                        field.value.clear();
                    }
                    form.replace = false;
                    if field.value.chars().count() < field.max_len {
                        field.value.push(c);
                    }
                }
                _ => {}
            }
            self.dialog = Some(dialog);
            return Ok(());
        }
        if matches!(dialog, Dialog::Details(_)) {
            match key.code {
                KeyCode::Enter | KeyCode::Char('e') => {
                    if self.pending.is_some() || self.uncertain {
                        return Err("write pending or unconfirmed".into());
                    }
                    return self.edit_form();
                }
                _ => {}
            }
        }
        if matches!(
            dialog,
            Dialog::Diff(..) | Dialog::Help(_) | Dialog::Details(_)
        ) {
            let last = self.dialog_scroll_limit(&dialog);
            let scroll = match &mut dialog {
                Dialog::Diff(_, scroll) | Dialog::Help(scroll) | Dialog::Details(scroll) => scroll,
                _ => unreachable!("scrollable dialog checked above"),
            };
            match key.code {
                KeyCode::Up => *scroll = scroll.saturating_sub(1),
                KeyCode::Down => *scroll = scroll.saturating_add(1).min(last),
                KeyCode::PageUp => *scroll = scroll.saturating_sub(10),
                KeyCode::PageDown => *scroll = scroll.saturating_add(10).min(last),
                KeyCode::Home => *scroll = 0,
                KeyCode::End => *scroll = last,
                KeyCode::Char('q') => return Ok(()),
                _ => {}
            }
            self.dialog = Some(dialog);
            return Ok(());
        }
        if key.code != KeyCode::Char('y')
            || key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Ok(());
        }
        match dialog {
            Dialog::Apply(_) => self.apply()?,
            Dialog::Verify => self.verify()?,
            Dialog::Discard => {
                if let Some(e) = &mut self.editor {
                    e.discard();
                }
                self.sync_navigation();
                self.refresh();
                self.set_status(Level::Info, "All local changes discarded; no device write.");
            }
            Dialog::Reload => self.reload(),
            Dialog::Quit => self.quit = true,
            _ => {}
        }
        Ok(())
    }
}

fn save_key(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('s' | 'S')) && key.modifiers.contains(KeyModifiers::CONTROL)
}
