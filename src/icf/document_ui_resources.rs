//! Native command registration and the registered Form2 profile notification.

use super::{
    Document, Error, InitialBoundary, Session, invalid,
    lifecycle::{MenuContext, MenuDescriptor, MenuItem, MenuQuery, RecentFiles, UiState},
};
use crate::{
    display,
    icf::{record_image, rows},
    printing::{self, native::Resources},
};
use std::sync::Arc;

fn source_error(_: std::io::Error) -> Error {
    invalid("native command source/query failed")
}

fn query_display(context: &mut MenuContext<'_>, node: &mut MenuDescriptor) -> Result<(), Error> {
    context
        .settings
        .query_menu(
            node.key,
            node.index as u32,
            &mut node.flags1d,
            &mut node.flags1e,
        )
        .map_err(source_error)
}

fn query_print(context: &mut MenuContext<'_>, node: &mut MenuDescriptor) -> Result<(), Error> {
    let printable = if let Some(active) = context.active {
        if let Some(view) = &active.dataset {
            let mut views = printing::Views::default();
            let id = views.register(view.clone());
            views
                .nonempty(context.document.current(), id)
                .map_err(source_error)?
        } else {
            active.base_bc & 1 == 0
        }
    } else {
        false
    };
    if !printable {
        node.flags1e |= 2;
    }
    Ok(())
}

fn cached_word(current: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        current
            .get(at..at + 4)
            .ok_or(invalid("cached menu count source unavailable"))?
            .try_into()
            .unwrap(),
    ))
}

fn query_csv(context: &mut MenuContext<'_>, node: &mut MenuDescriptor) -> Result<(), Error> {
    let kind = context.active.map_or(0, |v| v.factory.context_type());
    let group = context.active.map_or(0, |v| {
        v.dataset.as_ref().map_or(v.factory.group, |d| d.group)
    });
    let current = context.document.current();
    let mut import = false;
    let mut export = false;
    let mut grouped = false;
    let mut whole_export = false;
    match kind {
        1 | 2 => {
            grouped = true;
            if kind == 2 {
                if !(0..100).contains(&group) {
                    return Err(invalid("memory menu group outside0..100"));
                }
                import = true;
                export = cached_word(current, 0x3fb44 + 4 * group as usize)? != 0;
            }
            whole_export = rows::native::registration_count(current)? as u32 != 0;
        }
        9 | 10 | 11 | 12 => {
            grouped = true;
            let record_kind = if kind <= 10 { 0x0a } else { 0x0c };
            if matches!(kind, 10 | 12) {
                import = true;
                export = record_image::print_dataset_count(current, record_kind, group)? != 0;
            }
            let groups = if kind <= 10 { 27 } else { 50 };
            let mut total = 0_u32;
            for group in 0..groups {
                total = total.wrapping_add(record_image::print_dataset_count(
                    current,
                    record_kind,
                    group,
                )?);
            }
            whole_export = if kind <= 10 {
                total as u16 != 0
            } else {
                total != 0
            };
        }
        3 | 8 | 13 | 21 => {
            import = true;
            export = match kind {
                8 | 13 => record_image::print_dataset_count(current, kind, 0)? != 0,
                _ => true,
            };
        }
        14..=20 => {
            export = true;
        }
        _ => {}
    }
    // 95550 retains Disabled(bit1) and Hidden(bit0) as separate properties.
    // Its alternate-selection local is0;20/25 therefore remain hidden.
    let (disabled, hidden) = match node.key {
        0x1c => (!import, grouped),
        0x1d | 0x1e => (!grouped, !grouped),
        0x1f => (!import, !grouped),
        0x20 => (!import, true),
        0x21 => (!export, grouped),
        0x22 | 0x23 => (!whole_export, !grouped),
        0x24 => (!export, !grouped),
        0x25 => (!export, true),
        _ => return Err(invalid("command is outside CSV menu query")),
    };
    node.flags1e = node.flags1e & !3 | u8::from(disabled) << 1 | u8::from(hidden);
    Ok(())
}

fn query_recent(context: &mut MenuContext<'_>, node: &mut MenuDescriptor) -> Result<(), Error> {
    if !(0..4).contains(&node.index) {
        return Err(invalid("recent menu index outside0..4"));
    }
    // Persist/load use ten actual slots; unused trailing slots are empty paths.
    let path = context
        .recent
        .paths()
        .get(node.index as usize)
        .map_or(&[][..], Vec::as_slice);
    if path.is_empty() {
        node.flags1e |= 2;
        node.flags1e = node.flags1e & !1 | u8::from(node.index != 0);
    } else {
        node.caption =
            printing::native::menu::recent_caption(node.index, path).map_err(source_error)?;
        node.flags1e |= 4;
    }
    Ok(())
}

fn query_main(context: &mut MenuContext<'_>, node: &mut MenuDescriptor) -> Result<(), Error> {
    // 3E338..386 resets dynamic menu bits BEFORE the registered phase0 case.
    node.flags1e &= !7;
    node.flags1d &= 0x7f;
    match node.key {
        0
        | 0x3ae
        | 0x18..=0x1b
        | 0x27
        | 0x29
        | 0x2a
        | 0x32..=0x35
        | 0x39
        | 0x3b..=0x3d
        | 0x3f
        | 0x42
        | 0x43 => {
            // Explicit original case bodies: context40==0D gates their actions;
            // phase0 leaves flags unchanged. 3AE is the resource literal node,
            // not a named command. Unknown keys still fail below.
            Ok(())
        }
        0x2b..=0x31 | 0x36..=0x38 | 0x40 | 0x41 | 0xe6..=0xe9 => query_display(context, node),
        0x1c..=0x25 => query_csv(context, node),
        0x26 => query_print(context, node),
        0x28 => query_recent(context, node),
        _ => Err(invalid("native command phase0 handler is not registered")),
    }
}

impl UiState {
    pub fn from_native_resources(
        asset: Arc<[u8]>,
        document: &mut Document,
        session: &mut Session,
        settings: display::Settings,
        recent: &RecentFiles,
    ) -> Result<Self, Error> {
        let mut ui = Self::from_native_source(asset.clone(), document, session, settings.split)?;
        let resources = Resources::from_pe(&asset).map_err(source_error)?;
        ui.register_native_commands(&resources, settings)?;
        ui.sync02(document, session, recent)?;
        Ok(ui)
    }

    /// Main menu and toolbar share raw nodes and the SAME registered query body.
    pub fn register_native_commands(
        &mut self,
        resources: &Resources<'_>,
        settings: display::Settings,
    ) -> Result<(), Error> {
        self.menu.clear();
        let language = settings.language;
        let system_japanese = settings.system_japanese();
        self.display_settings = Some(settings);
        for resource in [b"GI_Menu_Main".as_slice(), b"GI_ToolBar_Main"] {
            for command in resources
                .commands_for_language(resource, language, system_japanese)
                .map_err(source_error)?
            {
                let caption = command.node.literal.clone();
                let (descriptor, query) = if command.separator {
                    (None, None)
                } else {
                    if command.node.group != 0x19 {
                        return Err(invalid("native main command has a different context group"));
                    }
                    let query: MenuQuery = query_main;
                    (
                        Some(MenuDescriptor {
                            group: command.node.group,
                            key: command.node.key,
                            index: command.node.index,
                            name: command.node.name,
                            caption: command.node.literal,
                            flags1d: command.node.flags1d,
                            flags1e: command.flags1e,
                            shortcut18: command.shortcut18,
                        }),
                        Some(query),
                    )
                };
                self.menu.push(MenuItem {
                    descriptor,
                    query,
                    property19a: false,
                    property1ae: false,
                    property1a1: false,
                    enabled: true,
                    shortcut: 0,
                    caption,
                });
            }
        }
        Ok(())
    }
    /// 4A5EC/event49/4A stores8646AC BEFORE any nested notification or Close.
    /// This is the raw signed-kind predicate, not Profile::selector validation.
    pub fn form2_profile(&mut self, session: &Session) -> bool {
        self.profile_ready = u8::from((session.context.fields().0[0] as i32) < 16);
        self.profile_ready != 0
    }

    /// Derived document event2B only adopts G while main visible57 is ZERO.
    /// Path/target/pending stay on the same document. Slot2C clears1AC/1AD.
    pub fn sync2b(&mut self, document: &mut Document, session: &mut Session) -> Result<(), Error> {
        if self
            .form
            .as_ref()
            .ok_or(invalid("main form is absent"))?
            .visible57
            != 0
        {
            return Ok(());
        }
        document.current = session.defaults()?.to_vec();
        session.document_flags = [0, 0];
        document.baseline.clone_from(&document.current);
        document.dirty = 0;
        self.initial_boundary(InitialBoundary::Title, document, session)
    }
}
