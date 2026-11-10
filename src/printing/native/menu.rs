//! Raw menu/toolbar descriptors from the existing named-resource and cell parser.

use super::{CellNode, Factory, Resources, integer, invalid, lead, split};
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub node: CellNode,
    pub depth: usize,
    pub separator: bool,
    pub flags1e: u8,
    pub shortcut18: u32,
    pub image_index: Option<i32>,
}

impl Factory {
    /// 62DA4's view-node classification, distinct from a stored record kind or
    /// CSV dispatch slot. Its explicit default 0 does not register an unknown UI.
    pub fn context_type(&self) -> u32 {
        match self.node {
            0 | 1 | 102 => 1,
            2..=101 => 2,
            103 => 3,
            401 => 4,
            402 => 5,
            104 => 8,
            250 | 251 => 9,
            252..=278 => 10,
            120 => 13,
            121 | 122 => 11,
            123..=172 => 12,
            210..=215 => self.node as u32 - 195,
            216 => 14,
            404 => 21,
            421 => 22,
            222 => 23,
            427 => 24,
            _ => 0,
        }
    }
}

pub(crate) fn recent_caption(index: i32, path: &[u8]) -> io::Result<Vec<u8>> {
    let mut result = format!("&{} ", index + 1).into_bytes();
    let mut at = 0;
    while at < path.len() {
        let byte = path[at];
        result.push(byte);
        at += 1;
        if lead(byte) {
            result.push(
                *path
                    .get(at)
                    .ok_or_else(|| invalid("truncated ANSI recent path"))?,
            );
            at += 1;
        } else if byte == b'&' {
            result.push(byte);
        }
    }
    Ok(result)
}

fn shortcut(text: &[u8]) -> io::Result<u32> {
    let parts = split(text, b'+');
    let (key, modifiers) = parts
        .split_last()
        .ok_or_else(|| invalid("empty menu shortcut"))?;
    let mut result = 0;
    for modifier in modifiers {
        let bit = match *modifier {
            b"Shift" => 0x2000,
            b"Ctrl" => 0x4000,
            b"Alt" => 0x8000,
            _ => return Err(invalid("unsupported native shortcut modifier")),
        };
        if result & bit != 0 {
            return Err(invalid("duplicate native shortcut modifier"));
        }
        result |= bit;
    }
    let key = match *key {
        [byte @ (b'A'..=b'Z' | b'0'..=b'9')] => u32::from(*byte),
        function if function.starts_with(b"F") => {
            let number = integer(&function[1..])?;
            if !(1..=24).contains(&number) {
                return Err(invalid("native function shortcut outside F1..F24"));
            }
            0x6f + number as u32
        }
        _ => return Err(invalid("unsupported native shortcut key")),
    };
    Ok(result | key)
}

impl Resources<'_> {
    /// Uses the SAME BasicCell inheritance, key table, indices and ANSI escapes
    /// as sheet cells. The added properties are menu metadata, never query output.
    pub fn commands(&self, resource: &[u8]) -> io::Result<Vec<Command>> {
        self.bound(|source| self.commands_at(source, resource))
    }

    /// Register commands against live display settings, not a previous binding.
    pub fn commands_for_language(
        &self,
        resource: &[u8],
        gui_language: u32,
        system_japanese: bool,
    ) -> io::Result<Vec<Command>> {
        self.commands_at(
            Self::language_source(gui_language, system_japanese),
            resource,
        )
    }

    fn commands_at(&self, source_index: usize, resource: &[u8]) -> io::Result<Vec<Command>> {
        let name = |suffix: &[u8]| [resource, suffix].concat();
        let mut basic = CellNode::default();
        let mut index = 0;
        for source in self.entry_at(source_index, &name(b"_BasicCell"))? {
            basic = self.cell_at(source_index, source, &basic, &mut index)?;
        }
        let mut commands = Vec::new();
        for source in self.entry_at(source_index, &name(b"_Body"))? {
            if source.is_empty() {
                continue;
            }
            let depth = source.iter().take_while(|b| **b == b'\t').count();
            let source = &source[depth..];
            let node = self.cell_at(source_index, source, &basic, &mut index)?;
            let mut command = Command {
                separator: node.literal == b"-",
                node,
                depth,
                flags1e: 0,
                shortcut18: 0,
                image_index: None,
            };
            for section in split(source, b'{').into_iter().skip(1) {
                let format = split(section, b'}')[0];
                for property in format.split(|b| *b == b'(').skip(1) {
                    let property = property.split(|b| *b == b')').next().unwrap();
                    if let Some(value) = property.strip_prefix(b"ImageIndex=") {
                        command.image_index = Some(integer(value)?);
                    } else if property.starts_with(b"%")
                        && let Some(at) = property.iter().position(|b| *b == b':')
                    {
                        command.shortcut18 = shortcut(&property[at + 1..])?;
                    }
                }
            }
            commands.push(command);
        }
        Ok(commands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        clone::{
            IMAGE_CAPACITY,
            information::{
                Controller, DocumentReceiver, Life, NativeReturn, ReadMode, RegistrySource,
            },
        },
        display::Settings,
        icf::{
            context::Context,
            document::{
                Document, InitialState, Session,
                lifecycle::{Lifecycle, PathComparison, RecentFiles, UiState},
            },
        },
    };
    use std::{collections::BTreeMap, sync::Arc};

    fn settings() -> Settings {
        Settings {
            language: 0,
            system_language: 0,
            font_percent: 120,
            toolbar: 1,
            split: 0,
            callsign_dialog: 1,
            auto_correct: 1,
            fixed_name: 2,
            column_bits: 15,
            column_bits_high: 0,
        }
    }
    fn resources() -> Resources<'static> {
        let mut entries = BTreeMap::new();
        let mut put = |name: &'static [u8], lines: &[&'static [u8]]| {
            entries.insert(name, lines.to_vec());
        };
        put(b"CellModel_Basic", &[b"<-"]);
        put(b"GI_Menu_Main_BasicCell", &[b"{($Basic)}"]);
        put(b"GI_ToolBar_Main_BasicCell", &[b"{($Basic)}"]);
        put(
            b"GI_Menu_Main_Body",
            &[
                b"File{(%CL_Null)}",
                b"\tNew{(%CL_Menu_File_New:Ctrl+N)}",
                b"Fixed{(%CL_Menu_View_Fixed)}",
                b"Font{(%CL_Menu_View_Font[120])}",
                b"Import{(%CL_Menu_File_Import)}",
                b"Print{(%CL_Menu_File_Print)}",
                b"Recent{(%CL_Menu_File_Recent[0])}",
                b"Recent2{(%CL_Menu_File_Recent[1])}",
                b"-",
            ],
        );
        put(
            b"GI_ToolBar_Main_Body",
            &[b"Save{(ImageIndex=2)(%CL_Menu_File_Save)}"],
        );
        let keys = BTreeMap::from([
            (b"CL_Null".as_slice(), 0),
            (b"CL_Menu_File_New".as_slice(), 0x18),
            (b"CL_Menu_View_Fixed".as_slice(), 0x2b),
            (b"CL_Menu_View_Font".as_slice(), 0x36),
            (b"CL_Menu_File_Import".as_slice(), 0x1c),
            (b"CL_Menu_File_Print".as_slice(), 0x26),
            (b"CL_Menu_File_Recent".as_slice(), 0x28),
            (b"CL_Menu_File_Save".as_slice(), 0x1a),
        ]);
        Resources {
            pe: &[],
            source: Some(0),
            entries: [entries.clone(), entries.clone(), entries],
            keys,
        }
    }
    fn controller() -> Controller {
        let mut c = Controller::new(7, b"7".to_vec(), [6, 0, 0]);
        c.construct_com(
            &RegistrySource {
                values: &[],
                crt_point: None,
            },
            &ReadMode::Current,
        )
        .unwrap();
        c
    }
    struct NoComparison;
    impl PathComparison for NoComparison {
        fn compare(&mut self, _: &[u8], _: &[u8]) -> Result<i32, crate::icf::Error> {
            Err(crate::icf::Error::Invalid {
                line: 0,
                reason: "unexpected native path comparison",
            })
        }
    }

    #[test]
    fn r2_command_registration_uses_live_settings_and_keeps_failure_prefix() {
        let mut source = resources();
        for (index, literal) in [
            b"JP-system{(%CL_Null)}".as_slice(),
            b"JP-other{(%CL_Null)}".as_slice(),
            b"English{(%CL_Null)}".as_slice(),
        ]
        .into_iter()
        .enumerate()
        {
            source.entries[index]
                .get_mut(b"GI_Menu_Main_Body".as_slice())
                .unwrap()[0] = literal;
        }
        let mut ui = UiState::headless();
        for (gui, system, expected) in [
            (0, 0, b"English".as_slice()),
            (1, 0, b"JP-other"),
            (1, 1, b"JP-system"),
        ] {
            let mut settings = settings();
            settings.language = gui;
            settings.system_language = system;
            ui.register_native_commands(&source, settings).unwrap();
            assert_eq!(ui.menu[0].caption, expected);
        }
        source.entries[2].remove(b"GI_ToolBar_Main_Body".as_slice());
        assert!(ui.register_native_commands(&source, settings()).is_err());
        assert_eq!(ui.menu[0].caption, b"English");
        assert_eq!(ui.menu.len(), 9);
        assert_eq!(ui.display_settings.as_ref().unwrap().language, 0);
    }

    #[test]
    fn native_ui_self_check() {
        let source = resources();
        let commands = source.commands(b"GI_Menu_Main").unwrap();
        assert_eq!((commands[1].depth, commands[1].shortcut18), (1, 0x404e));
        assert_eq!(commands[1].node.literal, b"New");
        assert!(commands[8].separator);
        assert_eq!(
            source.commands(b"GI_ToolBar_Main").unwrap()[0].image_index,
            Some(2)
        );
        assert_eq!(shortcut(b"Alt+F4").unwrap(), 0x8073);
        assert!(shortcut(b"Ctrl+Ctrl+N").is_err());
        assert_eq!(
            recent_caption(0, b"C:\\A&B.icf").unwrap(),
            b"&1 C:\\A&&B.icf"
        );
        assert_eq!(
            recent_caption(0, &[0x81, b'&']).unwrap(),
            [b"&1 ".as_slice(), &[0x81, b'&']].concat()
        );

        let mut doc = Document::from_current(vec![1; IMAGE_CAPACITY], b"KEEP".to_vec(), 6);
        doc.begin_pending_name(4, b"PENDING".to_vec());
        let mut session = Session::new(Context::before_registry(), None, 1, [9, 8]);
        let mut lifecycle = Lifecycle {
            recent: RecentFiles::default(),
            ui: UiState::headless(),
        };
        lifecycle
            .ui
            .register_native_commands(&source, settings())
            .unwrap();
        lifecycle.sync02(&mut doc, &mut session).unwrap();
        assert!(lifecycle.ui.menu[2].property1ae);
        assert!(lifecycle.ui.menu[3].property1ae);
        assert!(!lifecycle.ui.menu[4].enabled);
        assert!(!lifecycle.ui.menu[5].enabled);
        assert_eq!(
            lifecycle.ui.menu[6].descriptor.as_ref().unwrap().flags1e & 3,
            2
        );
        assert_eq!(
            lifecycle.ui.menu[7].descriptor.as_ref().unwrap().flags1e & 3,
            3
        );
        lifecycle.ui.display_settings.as_mut().unwrap().fixed_name = 0;
        lifecycle.ui.display_settings.as_mut().unwrap().font_percent = 100;
        lifecycle
            .recent
            .insert(b"C:\\A&B.icf".to_vec(), &mut NoComparison)
            .unwrap();
        lifecycle.sync02(&mut doc, &mut session).unwrap();
        assert!(!lifecycle.ui.menu[2].property1ae);
        assert!(!lifecycle.ui.menu[3].property1ae);
        assert!(lifecycle.ui.menu[6].enabled);
        assert_eq!(lifecycle.ui.menu[6].caption, b"&1 C:\\A&&B.icf");
        assert_eq!(lifecycle.ui.menu[1].shortcut, 0x404e);

        let mut defaults = vec![0; IMAGE_CAPACITY];
        defaults[0x355a1] = 6;
        defaults[0x3c9b0..0x3c9c0].copy_from_slice(b"IcomCloneFormat3");
        session
            .adopt_captured_defaults(defaults.clone(), 1)
            .unwrap();
        let mut q = [0; 24];
        q[0] = 1;
        session.context = Context::from_bytes(q);
        lifecycle.ui.form.as_mut().unwrap().visible57 = 0;
        let mut initial = InitialState::default();
        let mut c = controller();
        let mut receiver = DocumentReceiver {
            document: &mut doc,
            session: &mut session,
            initial: &mut initial,
            ui: &mut lifecycle.ui,
            recent: &lifecycle.recent,
        };
        assert!(
            c.sync49_document(&mut receiver, &[NativeReturn::Returned(0); 3])
                .unwrap()
        );
        assert_eq!(receiver.ui.profile_ready, 1);
        assert_eq!(receiver.document.current(), defaults);
        assert_eq!(receiver.document.baseline(), defaults);
        assert_eq!(receiver.document.path(), b"KEEP");
        assert_eq!(receiver.document.pending_name().unwrap().name, b"PENDING");
        assert_eq!(receiver.session.document_flags, [0, 0]);
        assert_eq!(receiver.ui.form.as_ref().unwrap().visible57, 1);
        assert_eq!(c.form, Life::ReleaseRequested);
        assert!(!c.pair_registered);
        assert!(!c.sync49_document(&mut receiver, &[]).unwrap());

        let mut c = controller();
        receiver.ui.form.as_mut().unwrap().visible57 = 0;
        receiver.document.patch_current(&[(0, &[7])]).unwrap();
        assert!(c.sync49_document(&mut receiver, &[]).is_err());
        assert_eq!(receiver.document.current(), defaults);
        assert!(c.pair_registered);
        assert_eq!(c.form, Life::Open);

        lifecycle.ui.menu[1].descriptor.as_mut().unwrap().key = u32::MAX;
        lifecycle.ui.menu[1].descriptor.as_mut().unwrap().flags1e = 7;
        assert!(lifecycle.sync02(&mut doc, &mut session).is_err());
        assert_eq!(
            lifecycle.ui.menu[1].descriptor.as_ref().unwrap().flags1e & 7,
            0
        );

        if let Ok(path) = std::env::var("CS705_UI_SOURCE") {
            let asset: Arc<[u8]> = super::super::read_source(std::fs::File::open(path).unwrap())
                .unwrap()
                .into();
            doc.patch_current(&[
                (0x3fb40, &1_u32.to_le_bytes()),
                (0x3fb44, &1_u32.to_le_bytes()),
            ])
            .unwrap();
            let mut ui = UiState::from_native_resources(
                asset.clone(),
                &mut doc,
                &mut session,
                settings(),
                &lifecycle.recent,
            )
            .unwrap();
            assert!(ui.menu.len() > 50);
            assert!(ui.native.is_some());
            assert!(
                ui.menu
                    .iter()
                    .filter(|i| i.descriptor.is_some())
                    .all(|i| i.query.is_some())
            );
            let factory = ui
                .native
                .as_ref()
                .unwrap()
                .factories
                .iter()
                .find(|f| f.node == 2)
                .unwrap()
                .clone();
            let base_bc = u8::from(
                Resources::from_pe(&asset)
                    .unwrap()
                    .no_print(&factory)
                    .unwrap(),
            );
            let mut dataset = factory.view(doc.current(), 0).unwrap().unwrap();
            dataset.bc = (dataset.bc & !1) | base_bc;
            ui.native.as_mut().unwrap().active =
                Some(crate::icf::document::lifecycle::NativeView {
                    factory,
                    dataset: Some(dataset),
                    base_bc,
                });
            ui.sync02(&mut doc, &mut session, &lifecycle.recent)
                .unwrap();
            let enabled = |ui: &UiState, key| {
                ui.menu
                    .iter()
                    .find(|i| i.descriptor.as_ref().is_some_and(|n| n.key == key))
                    .unwrap()
                    .enabled
            };
            assert!(enabled(&ui, 0x24));
            assert!(enabled(&ui, 0x26));
            // CSV uses the actual cached count; print uses the existing dataset
            // predicate. Do not silently recount/repair inconsistent metadata.
            doc.patch_current(&[(0x3fb44, &0_u32.to_le_bytes())])
                .unwrap();
            ui.sync02(&mut doc, &mut session, &lifecycle.recent)
                .unwrap();
            assert!(!enabled(&ui, 0x24));
            assert!(enabled(&ui, 0x26));
        }
    }
}
