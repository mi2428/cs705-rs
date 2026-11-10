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
    /// CSV dispatch slot. Its explicit default0 does not register an unknown UI.
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
                    } else if property.starts_with(b"%") {
                        if let Some(at) = property.iter().position(|b| *b == b':') {
                            command.shortcut18 = shortcut(&property[at + 1..])?;
                        }
                    }
                }
            }
            commands.push(command);
        }
        Ok(commands)
    }
}
