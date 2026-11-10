//! Bounded original resource producers. No GUI bus or second field codec.

pub mod menu;

use super::{Grid, ViewKind, ViewState, invalid};
use crate::csv::history::pe_tail;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

const RESOURCE_VECTORS: [(u32, usize); 3] = [(0x301918, 6084), (0x307828, 6084), (0x30d738, 6088)];
const RESOURCE_END: &[u8] = b"#End_Of_Ovl";
const STRING_BOUND: usize = 4096;

fn data_tail(source: &[u8], rva: u32) -> io::Result<&[u8]> {
    if !(0x190000..0x464600).contains(&rva) {
        return Err(invalid("native source RVA outside data section"));
    }
    let tail =
        pe_tail(source, rva).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let expected = source
        .get(rva as usize - 0x1200..)
        .ok_or_else(|| invalid("native source RVA has no file bytes"))?;
    if tail.as_ptr() != expected.as_ptr() {
        return Err(invalid(
            "native resource asset has a different section layout",
        ));
    }
    Ok(tail)
}

fn source_string(source: &[u8], pointer: u32, bound: usize) -> io::Result<&[u8]> {
    let rva = pointer
        .checked_sub(0x400000)
        .ok_or_else(|| invalid("native C-string pointer outside PE"))?;
    let tail = data_tail(source, rva)?;
    let end = tail
        .iter()
        .take(bound)
        .position(|byte| *byte == 0)
        .ok_or_else(|| invalid("native C-string has no bounded NUL"))?;
    Ok(&tail[..end])
}

fn capture_string(
    reader: &mut (impl io::Read + io::Seek),
    source: &mut [u8],
    pointer: u32,
) -> io::Result<()> {
    let rva = pointer
        .checked_sub(0x400000)
        .ok_or_else(|| invalid("native C-string pointer outside PE"))?;
    let size = data_tail(source, rva)?.len().min(STRING_BOUND);
    let mut bytes = [0; STRING_BOUND];
    reader.seek(io::SeekFrom::Start(u64::from(rva - 0x1200)))?;
    reader.read_exact(&mut bytes[..size])?;
    let end = bytes[..size]
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| invalid("native C-string has no bounded NUL"))?;
    let at = rva as usize - 0x1200;
    source[at..at + end + 1].copy_from_slice(&bytes[..end + 1]);
    Ok(())
}

/// Read PE headers, the three registered vectors and their bounded C-strings.
/// The returned sparse buffer is never an executable or a published payload.
pub fn read_source(mut reader: impl io::Read + io::Seek) -> io::Result<Vec<u8>> {
    let mut source = vec![0; 0x463400];
    reader.read_exact(&mut source[..0x600])?;
    for (rva, size) in RESOURCE_VECTORS
        .into_iter()
        .map(|(rva, count)| (rva, count * 4))
        .chain([
            (0x3f2ae0, 0x64),
            (0x3f2b44, 0x3f8e00 - 0x3f2b44),
            (0x3f90d0, 0x80),
            (0x418f18, 0x419520 - 0x418f18),
            (0x40a183, 4),
            (0x4077b8, 0x11c),
            (0x409600, 0x104),
        ])
    {
        data_tail(&source, rva)?
            .get(..size)
            .ok_or_else(|| invalid("native fixed source window unavailable"))?;
        let at = rva as usize - 0x1200;
        reader.seek(io::SeekFrom::Start(at as u64))?;
        reader.read_exact(&mut source[at..at + size])?;
    }
    let mut captured = BTreeSet::new();
    for (rva, count) in RESOURCE_VECTORS {
        for index in 0..count {
            let at = rva as usize - 0x1200 + 4 * index;
            let pointer = u32::from_le_bytes(source[at..at + 4].try_into().unwrap());
            if captured.insert(pointer) {
                capture_string(&mut reader, &mut source, pointer)?;
            }
            let line = source_string(&source, pointer, STRING_BOUND)?;
            if (line == RESOURCE_END) != (index + 1 == count) {
                return Err(invalid("native resource vector has an invalid end marker"));
            }
        }
    }
    // CSV uses locale0; native paint can select locale1. Capture both targets.
    // Capture those exact targets, not an unobserved zero-filled PE tail.
    let slots = (0..96)
        .flat_map(|i| {
            [
                0x418f18 + i * 8,
                0x418f1c + i * 8,
                0x419218 + i * 8,
                0x41921c + i * 8,
            ]
        })
        .chain([0x419518, 0x41951c]);
    for slot in slots {
        let at = slot - 0x1200;
        let pointer = u32::from_le_bytes(source[at..at + 4].try_into().unwrap());
        if pointer == 0 {
            continue;
        }
        let rva = pointer
            .checked_sub(0x400000)
            .filter(|rva| (0x190000..=0x464600 - 39).contains(rva))
            .ok_or_else(|| invalid("native symbol caption outside source data section"))?
            as usize;
        reader.seek(io::SeekFrom::Start((rva - 0x1200) as u64))?;
        reader.read_exact(&mut source[rva - 0x1200..rva - 0x1200 + 39])?;
    }
    // PHG/DataType pointers may only reference their actually captured strings.
    for pointer in source[0x4077b8 - 0x1200..0x4078d4 - 0x1200]
        .as_chunks::<4>()
        .0
    {
        let rva = u32::from_le_bytes(*pointer)
            .checked_sub(0x400000)
            .filter(|rva| (0x409600..0x409704).contains(rva))
            .ok_or_else(|| invalid("native Rx label points outside its captured source"))?
            as usize;
        let tail = &source[rva - 0x1200..(rva + 9).min(0x409704) - 0x1200];
        if !tail.contains(&0) {
            return Err(invalid("native Rx label has no captured bounded NUL"));
        }
    }
    Ok(source)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Base,
    Gps,
    Common,
    Preset,
    ColCut,
    Simplex,
    Split,
    Satellite,
    RxHistory,
    Groups,
    Repeater,
    Ur,
    Allowed,
}

impl Class {
    fn parse(name: &[u8]) -> io::Result<Self> {
        Ok(match name {
            b"" => Self::Base,
            b"St_ChGps" => Self::Gps,
            b"St_Common" => Self::Common,
            b"St_Preset" => Self::Preset,
            b"St_ColCut" => Self::ColCut,
            b"St_ColCut_Simplex" => Self::Simplex,
            b"St_ColCut_Split" => Self::Split,
            b"St_ColCut_Satellite" => Self::Satellite,
            b"St_ColCutRxHis" => Self::RxHistory,
            b"St_GrpOpe" => Self::Groups,
            b"St_RptList" => Self::Repeater,
            b"St_UrList" => Self::Ur,
            b"St_Allowcs" => Self::Allowed,
            _ => return Err(invalid("unregistered native printer class")),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Factory {
    pub class: Class,
    pub resource: String,
    pub node: i32,
    pub group: i32,
    pub parameter: Vec<u8>,
}

impl Factory {
    /// 239D8/2C27C: class and resource are separate, explicit bindings.
    pub fn parse(target: &[u8]) -> io::Result<Self> {
        let target = trim(target);
        let (class, target) = if target.first() == Some(&b'[') {
            let end = target
                .iter()
                .position(|b| *b == b']')
                .ok_or_else(|| invalid("unterminated native printer class"))?;
            (Class::parse(&target[1..end])?, &target[end + 1..])
        } else {
            (Class::Base, target)
        };
        let mut parts = target.splitn(3, |b| *b == b',');
        let resource = ascii(trim(parts.next().unwrap()))?.to_owned();
        if !resource.starts_with("GI_Sheet_") {
            return Err(invalid("native factory requires a sheet resource"));
        }
        let node = parts.next().map(integer).transpose()?.unwrap_or(0);
        let parameter = trim(parts.next().unwrap_or_default()).to_vec();
        let group = match class {
            Class::Gps => node - 252,
            Class::Repeater => node - 123,
            Class::Preset => node - 422,
            Class::Split if node == 103 => 100,
            Class::Split => node - 2,
            _ => 0,
        };
        Ok(Self {
            class,
            resource,
            node,
            group,
            parameter,
        })
    }

    pub fn view(&self, current: &[u8], show_split: u32) -> io::Result<Option<ViewState>> {
        let kind = match self.class {
            Class::Gps => ViewKind::Gps,
            Class::Repeater => ViewKind::Repeater,
            Class::Ur => ViewKind::Ur,
            Class::Allowed => ViewKind::Allowed,
            Class::Preset => ViewKind::Preset,
            Class::Split if self.node == 103 => ViewKind::Call,
            Class::Split if (2..102).contains(&self.node) => ViewKind::Memory,
            Class::ColCut if self.node == 104 => ViewKind::Scan,
            Class::Simplex if self.node == 401 => ViewKind::Bandstack,
            Class::Simplex if self.node == 402 => ViewKind::Memopad,
            Class::Base | Class::Common | Class::Groups | Class::RxHistory => return Ok(None),
            Class::Satellite => return Err(invalid("satellite printer has no IC-705 tree source")),
            _ => {
                return Err(invalid(
                    "native printer class has an unsupported dataset binding",
                ));
            }
        };
        let group = if kind == ViewKind::Call {
            0
        } else {
            self.group
        };
        ViewState::from_image_with_split(current, kind, group, show_split).map(Some)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellNode {
    pub group: u32,
    pub key: u32,
    pub index: i32,
    pub name: Vec<u8>,
    pub flags1c: u8,
    pub flags1d: u8,
    pub comb: i32,
    pub above: usize,
    pub below: usize,
    pub merge: bool,
    pub typed_index: Option<u32>,
    pub record_visible: bool,
    pub literal: Vec<u8>,
    pub alias: Option<Vec<u8>>,
    pub checked: Option<bool>,
}

impl Default for CellNode {
    fn default() -> Self {
        Self {
            group: 0x19,
            key: 0x3ae,
            index: 0,
            name: Vec::new(),
            flags1c: 0,
            flags1d: 0x10,
            comb: 0,
            above: 0,
            below: 0,
            merge: false,
            typed_index: None,
            record_visible: true,
            literal: Vec::new(),
            alias: None,
            checked: None,
        }
    }
}

/// Input and paint contexts have different producers. These scalar fields are
/// generated from the selected resource node, never restored from a packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineFrame {
    pub column: usize,
    pub row: usize,
    pub node: CellNode,
    pub factory: Factory,
    pub input_code: u32,
    pub layout: u32,
    pub warning_bits: u8,
    pub focus: u8,
    pub action: u32,
    pub command: u32,
    pub phase: u32,
    pub notify: u32,
    pub initial_flags: u32,
    pub grid4: bool,
    pub grid8: bool,
    pub callback: u32,
    pub text: Vec<u8>,
}

impl InlineFrame {
    /// 32394 supplies code0D; 302A8 replaces action with0 for non-NULL text.
    pub fn commit(grid: &Grid, column: usize, row: usize, text: &[u8]) -> io::Result<Self> {
        let nodes = grid
            .nodes
            .as_ref()
            .ok_or_else(|| invalid("inline input requires native resource cells"))?;
        let node = nodes
            .get(row)
            .and_then(|r| r.get(column))
            .ok_or_else(|| invalid("native editor cell outside grid"))?
            .clone();
        if node.key == 0x3ae || node.flags1c & 0x80 != 0 {
            return Err(invalid("native editor requires a named owning cell"));
        }
        if !node.record_visible {
            return Err(invalid("native editor cell belongs to a hidden split side"));
        }
        let factory = grid
            .factory
            .clone()
            .ok_or_else(|| invalid("native editor factory unavailable"))?;
        Ok(Self {
            column,
            row,
            node,
            factory,
            input_code: 0x0d,
            layout: grid.show_split,
            // 14388 clears paint bits0..3 and70; 300F4/5D2E8 then marks
            // this selected software cell, with no printer attached to input.
            warning_bits: 1,
            focus: 0,
            action: 0,
            command: 0,
            phase: 1,
            notify: 1,
            // 63734 starts with1. This phase1 producer has no phase2 event
            // pointer, so it does not add the selection-event bit8.
            initial_flags: 1,
            grid4: true,
            grid8: true,
            callback: 0x3173c,
            text: text.to_vec(),
        })
    }
}

/// The selected software cell is a gesture, not a completed context packet.
/// Guard clear, frame construction, actual dispatch and normal hide keep the
/// original failure prefix. A second1B sees a hidden editor and cannot recommit.
pub fn sync_editor<E: From<io::Error>>(
    editor: &mut super::Editor,
    grid: &Grid,
    column: usize,
    row: usize,
    dispatch: impl FnOnce(&InlineFrame) -> Result<(), E>,
) -> Result<(), E> {
    super::sync_editor(editor, |text| {
        let frame = InlineFrame::commit(grid, column, row, text).map_err(E::from)?;
        dispatch(&frame)
    })
}

fn trim(s: &[u8]) -> &[u8] {
    crate::csv::trim(s)
}
fn ascii(s: &[u8]) -> io::Result<&str> {
    std::str::from_utf8(s)
        .ok()
        .filter(|s| s.is_ascii())
        .ok_or_else(|| invalid("native resource identifier must be ASCII"))
}
fn integer(s: &[u8]) -> io::Result<i32> {
    crate::csv::number::int32(trim(s)).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
fn lead(b: u8) -> bool {
    matches!(b, 0x81..=0x9f | 0xe0..=0xfc)
}
fn split(s: &[u8], delimiter: u8) -> Vec<&[u8]> {
    let mut result = Vec::new();
    let (mut start, mut i) = (0, 0);
    while i < s.len() {
        if lead(s[i]) || s[i] == b'\\' {
            i += 2;
            continue;
        }
        if s[i] == delimiter {
            result.push(&s[start..i]);
            start = i + 1;
        }
        i += 1;
    }
    result.push(&s[start..]);
    result
}

type Entries<'a> = BTreeMap<&'a [u8], Vec<&'a [u8]>>;

/// The source remains the original asset. No compiled resource table is copied
/// into Rust, and callers never supply completed cells, pages or reply lists.
pub struct Resources<'a> {
    pe: &'a [u8],
    entries: [Entries<'a>; 3],
    keys: BTreeMap<&'a [u8], u32>,
    source: Option<usize>,
}

fn parse_headers<'a>(
    lines: impl IntoIterator<Item = io::Result<&'a [u8]>>,
) -> io::Result<Entries<'a>> {
    let mut entries = BTreeMap::new();
    let mut name: Option<&[u8]> = None;
    for line in lines {
        let line = line?;
        if line == RESOURCE_END {
            return Ok(entries);
        }
        if let Some(header) = line.strip_prefix(b"#") {
            name = (!header.is_empty()).then_some(header);
            if let Some(name) = name {
                entries.entry(name).or_insert_with(Vec::new);
            }
        } else if let Some(name) = name {
            entries.get_mut(name).unwrap().push(line);
        }
    }
    Err(invalid("native resource vector has no end marker"))
}

impl<'a> Resources<'a> {
    pub fn from_pe(pe: &'a [u8]) -> io::Result<Self> {
        let mut entries = std::array::from_fn(|_| BTreeMap::new());
        for (source, (rva, count)) in RESOURCE_VECTORS.into_iter().enumerate() {
            let pointers = data_tail(pe, rva)?
                .get(..count * 4)
                .ok_or_else(|| invalid("native resource vector unavailable"))?;
            entries[source] = parse_headers(pointers.as_chunks::<4>().0.iter().enumerate().map(
                |(index, pointer)| {
                    let line = source_string(pe, u32::from_le_bytes(*pointer), STRING_BOUND)?;
                    if (line == RESOURCE_END) != (index + 1 == count) {
                        return Err(invalid("native resource vector has an invalid end marker"));
                    }
                    Ok(line)
                },
            ))?;
            if !entries[source].contains_key(b"CellModel_Basic".as_slice()) {
                return Err(invalid("native resource source has no Basic model"));
            }
        }
        let mut keys = BTreeMap::new();
        for (address, count) in [(0x3f2ae0, 0x19), (0x3f2b44, 0x3ae)] {
            let pointers = data_tail(pe, address)?
                .get(..count * 4)
                .ok_or_else(|| invalid("native cell-name source unavailable"))?;
            for (key, pointer) in pointers.as_chunks::<4>().0.iter().enumerate() {
                let value = source_string(pe, u32::from_le_bytes(*pointer), 256)?;
                if !value.is_empty() {
                    keys.insert(value, key as u32);
                }
            }
        }
        Ok(Self {
            pe,
            entries,
            keys,
            source: None,
        })
    }

    /// 226E0 binds the complete resource graph, including inherited models.
    /// system_japanese is48B0C's primary-LANGID predicate, not C00 or live Q.
    /// Without a binding, consumers require equal results from all three sources.
    pub fn with_language(mut self, gui_language: u32, system_japanese: bool) -> Self {
        self.source = Some(Self::language_source(gui_language, system_japanese));
        self
    }

    fn language_source(gui_language: u32, system_japanese: bool) -> usize {
        if gui_language != 1 {
            2
        } else {
            usize::from(!system_japanese)
        }
    }

    fn bound<T: PartialEq>(&self, mut read: impl FnMut(usize) -> io::Result<T>) -> io::Result<T> {
        match self.source {
            Some(source) => read(source),
            None => self.selected(None, None, read),
        }
    }

    /// Borrow the same sparse asset for the existing notice/symbol providers.
    /// Only bounded windows and targets captured by read_source are populated.
    pub fn source_pe(&self) -> &'a [u8] {
        self.pe
    }

    /// B03C8/B05D4's raw captions, before the separate47CF0 renderer.
    pub fn metadata_empty_caption(
        &self,
        field: crate::icf::settings::extended::MetadataField,
        gui_language: Option<u32>,
        system_langid: Option<u16>,
    ) -> io::Result<&'a [u8]> {
        use crate::icf::settings::extended::MetadataField;
        let caption = match field {
            MetadataField::ConnectionIpList => {
                self.resource_line(b"GI_IPAddressListNotUsed", 0, gui_language, system_langid)?
            }
            MetadataField::TransmitPicture => self.source_line(0x40a183, 4, 0)?,
        };
        if caption.is_empty() {
            return Err(invalid("native metadata caption source is empty"));
        }
        Ok(caption)
    }

    fn entry(&self, name: &[u8]) -> io::Result<&[&'a [u8]]> {
        self.bound(|source| self.entry_at(source, name))
    }

    fn entry_at(&self, source: usize, name: &[u8]) -> io::Result<&[&'a [u8]]> {
        self.entries[source]
            .get(name)
            .map(Vec::as_slice)
            .ok_or_else(|| invalid("native named resource is not registered"))
    }

    fn source_line(&self, rva: u32, width: usize, index: usize) -> io::Result<&'a [u8]> {
        pe_tail(self.pe, rva)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
            .get(..width)
            .and_then(|bytes| bytes.strip_suffix(b"\0"))
            .and_then(|bytes| bytes.split(|byte| *byte == 0).nth(index))
            .ok_or_else(|| invalid("native provider line is absent or unterminated"))
    }

    // 226E0: missing inputs leave a finite set of possible registered sources.
    // Returning without those inputs requires equal actual results from ALL of
    // them, including the selected source's models. No source1 fallback exists.
    fn selected<T: PartialEq>(
        &self,
        gui: Option<u32>,
        system: Option<u16>,
        mut read: impl FnMut(usize) -> io::Result<T>,
    ) -> io::Result<T> {
        let candidates: &[usize] = match (gui, system) {
            (Some(1), Some(id)) if id & 0x3ff == 0x11 => &[0],
            (Some(1), Some(_)) => &[1],
            (Some(1), None) => &[0, 1],
            (Some(_), _) => &[2],
            (None, Some(id)) if id & 0x3ff == 0x11 => &[0, 2],
            (None, Some(_)) => &[1, 2],
            (None, None) => &[0, 1, 2],
        };
        let result = read(candidates[0])?;
        for source in &candidates[1..] {
            if read(*source)? != result {
                return Err(invalid(
                    "native caption requires its actual C00/SYSTEM source",
                ));
            }
        }
        Ok(result)
    }

    /// The same registered-source selection as selector captions. Empty VALUE
    /// lines remain empty; missing resources/indices are errors, never defaults.
    pub fn resource_line(
        &self,
        name: &[u8],
        index: usize,
        gui_language: Option<u32>,
        system_langid: Option<u16>,
    ) -> io::Result<&'a [u8]> {
        self.selected(gui_language, system_langid, |source| {
            self.entry_at(source, name)?
                .get(index)
                .copied()
                .ok_or_else(|| invalid("native provider line is unavailable"))
        })
    }

    pub fn tree(&self, memory_groups: usize) -> io::Result<Vec<Factory>> {
        self.bound(|source| self.tree_at(source, memory_groups))
    }

    fn tree_at(&self, source: usize, memory_groups: usize) -> io::Result<Vec<Factory>> {
        if memory_groups > 100 {
            return Err(invalid("native memory tree exceeds100 groups"));
        }
        let mut result = Vec::new();
        for line in self.entry_at(source, b"GI_Tree_Main_Body")? {
            if let Some(start) = line.windows(12).position(|s| s == b"(%CallSheet:") {
                let target = &line[start + 12..];
                let end = target
                    .iter()
                    .position(|b| *b == b')')
                    .ok_or_else(|| invalid("unterminated tree CallSheet target"))?;
                let factory = Factory::parse(&target[..end])?;
                let groups = factory.class == Class::Groups;
                result.push(factory);
                if groups {
                    for group in 0..memory_groups {
                        result.push(Factory::parse(
                            format!(
                                "[St_ColCut_Split]GI_Sheet_MemoryCH_Split,{},{}",
                                group + 2,
                                group
                            )
                            .as_bytes(),
                        )?);
                    }
                }
            }
        }
        if result.is_empty() {
            return Err(invalid("native printer tree has no factories"));
        }
        Ok(result)
    }

    pub fn registered(&self, image: &crate::icf::Icf<'_>) -> io::Result<Vec<Factory>> {
        self.tree(image.memory_groups().map_err(super::data_error)?.len())
    }

    /// Bound-source rows; native paint consumers use selector_caption instead.
    /// Selector rows retain their real Flex key/index and raw ANSI literal.
    /// This is the same BasicCell/model/cell parser, not a second label decoder.
    pub fn selector_cells(&self, resource: &[u8]) -> io::Result<Vec<CellNode>> {
        self.bound(|source| self.selector_cells_at(source, resource))
    }

    fn selector_cells_at(&self, source: usize, resource: &[u8]) -> io::Result<Vec<CellNode>> {
        let factory = Factory::parse(resource)?;
        let grid = self.schema_at(source, &factory, &[], |_| {
            Err(invalid("native selector requires a condition source"))
        })?;
        Ok(grid
            .nodes
            .unwrap()
            .into_iter()
            .flatten()
            .filter(|node| node.key != 0x3ae)
            .collect())
    }

    /// 19D78's caller-selected class result, not CSV text or the raw row literal.
    /// Mode uses key7; the other connected selectors use key1. Explicit
    /// colon aliases (including empty) take precedence. Full-width value lookup
    /// must match a real Flex0 index; unknown values do not become captions.
    pub fn selector_caption(
        &self,
        resource: &[u8],
        selector_key: u32,
        value: i64,
        gui_language: Option<u32>,
        system_langid: Option<u16>,
    ) -> io::Result<Vec<u8>> {
        self.selected(gui_language, system_langid, |source| {
            let node = self
                .selector_cells_at(source, resource)?
                .into_iter()
                .find(|node| node.key == selector_key && i64::from(node.index) == value)
                .ok_or_else(|| invalid("native selector has no stored full-width value"))?;
            Ok(node.alias.unwrap_or(node.literal))
        })
    }

    fn format(
        &self,
        source_index: usize,
        source: &[u8],
        node: &mut CellNode,
        env: &mut i32,
        depth: usize,
    ) -> io::Result<()> {
        if depth > 16 {
            return Err(invalid("native cell model inheritance cycle"));
        }
        let (mut i, mut negate) = (0, 0_u8);
        while i < source.len() {
            let ch = source[i];
            i += 1;
            if ch == b'(' {
                // 10FD4 protects escaped bytes/DBCS trails before testing ')'.
                let end = i + split(&source[i..], b')')[0].len();
                if end == source.len() {
                    return Err(invalid("unterminated native cell property"));
                }
                let property = &source[i..end];
                i = end + 1;
                if let Some(model) = property.strip_prefix(b"$") {
                    let mut name = b"CellModel_".to_vec();
                    name.extend_from_slice(model);
                    for value in self.entry_at(source_index, &name)? {
                        self.format(source_index, value, node, env, depth + 1)?;
                    }
                } else if let Some(value) = property.strip_prefix(b"%") {
                    let mut parts = value.splitn(2, |byte| *byte == b':');
                    // 11D85 reads the index through ']', not through padding
                    // before ':'. Only the identifier token is trimmed.
                    let mut name = trim(parts.next().unwrap());
                    if let Some(alias) = parts.next() {
                        // 11DFF removes a quoting backslash, not a DBCS trail.
                        let mut bytes = alias.iter().copied();
                        let mut decoded = Vec::with_capacity(alias.len());
                        while let Some(mut byte) = bytes.next() {
                            if byte == b'\\' {
                                let Some(quoted) = bytes.next() else { break };
                                byte = quoted;
                            }
                            decoded.push(byte);
                            if lead(byte) {
                                decoded.extend(bytes.next());
                            }
                        }
                        node.alias = Some(decoded);
                    }
                    if let Some(at) = name.iter().position(|b| *b == b'.') {
                        node.group = *self
                            .keys
                            .get(&name[..at])
                            .ok_or_else(|| invalid("unknown native context type name"))?;
                        name = &name[at + 1..];
                    }
                    if let Some(at) = name.iter().position(|b| *b == b'[') {
                        let index = name
                            .get(at + 1..name.len().saturating_sub(1))
                            .filter(|_| name.last() == Some(&b']'))
                            .ok_or_else(|| invalid("invalid native cell index expression"))?;
                        node.index = if index == b"@ch0" {
                            *env
                        } else {
                            integer(index)?
                        };
                        name = &name[..at];
                    }
                    if !name.is_empty() {
                        node.key = *self
                            .keys
                            .get(name)
                            .ok_or_else(|| invalid("unknown native field name"))?;
                        node.name = name.to_vec();
                    }
                } else if let Some(value) = property.strip_prefix(b"@ch0=") {
                    *env = integer(value)?;
                } else if property == b"@ch0++" {
                    *env = env.wrapping_add(1);
                } else if let Some(value) = property.strip_prefix(b"Comb=") {
                    let parts: Vec<_> = value
                        .split(|b| *b == b',')
                        .map(integer)
                        .collect::<io::Result<_>>()?;
                    if parts.len() != 3 || parts[1] < 0 || parts[2] < 0 {
                        return Err(invalid("invalid native cell Comb property"));
                    }
                    node.comb = parts[0];
                    node.above = parts[1] as usize;
                    node.below = parts[2] as usize;
                    node.merge = true;
                }
            } else if ch == b'!' {
                negate = 2;
            } else {
                let on = u8::from(negate == 0);
                match ch {
                    b'<' => node.flags1c &= !3,
                    b'^' => node.flags1c = (node.flags1c & !3) | 1,
                    b'>' => node.flags1c = (node.flags1c & !3) | 2,
                    b'~' => node.flags1c &= !12,
                    b'-' => node.flags1c = (node.flags1c & !12) | 4,
                    b'_' => node.flags1c = (node.flags1c & !12) | 8,
                    b'W' => node.flags1c = (node.flags1c & !0x10) | on << 4,
                    b'H' => node.flags1c = (node.flags1c & !0x20) | on << 5,
                    b'=' => node.flags1c = (node.flags1c & !0x40) | on << 6,
                    b'+' => node.flags1c = (node.flags1c & !0x80) | on << 7,
                    b'B' => node.flags1d = (node.flags1d & !1) | on,
                    b'X' => node.flags1d = (node.flags1d & !6) | on << 1,
                    b'Q' => node.flags1d = (node.flags1d & !6) | on << 2,
                    b'S' => node.flags1d = (node.flags1d & !8) | on << 3,
                    b'I' => node.flags1d = (node.flags1d & !0x20) | on << 5,
                    b'T' => node.flags1d = (node.flags1d & !0x40) | on << 6,
                    b')' | b' ' | b'\t' | b'\r' | b'\n' => (),
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!(
                                "unsupported native cell format token0x{ch:02X} offset{} source{} key0x{:X} depth{depth}",
                                i - 1,
                                source_index + 1,
                                node.key,
                            ),
                        ));
                    }
                }
            }
            negate = negate.saturating_sub(1);
        }
        Ok(())
    }

    fn cell_at(
        &self,
        source_index: usize,
        source: &[u8],
        basic: &CellNode,
        env: &mut i32,
    ) -> io::Result<CellNode> {
        let mut node = basic.clone();
        node.index = *env;
        node.literal.clear();
        let (mut i, mut format) = (0, Vec::new());
        let mut inside = false;
        while i < source.len() {
            let b = source[i];
            i += 1;
            if b == b'{' && source.get(i..i + 2) == Some(b"(`") {
                // Original resource author annotations run to the end of row.
                break;
            }
            if lead(b) || b == b'\\' {
                let target = if inside {
                    &mut format
                } else {
                    &mut node.literal
                };
                if b != b'\\' || inside {
                    target.push(b);
                }
                let next = *source
                    .get(i)
                    .ok_or_else(|| invalid("truncated native cell character"))?;
                target.push(next);
                i += 1;
            } else if b == b'{' {
                inside = true;
            } else if b == b'}' {
                inside = false;
            } else if inside {
                format.push(b);
            } else {
                node.literal.push(b);
            }
        }
        if inside {
            return Err(invalid("unterminated native cell format"));
        }
        self.format(source_index, &format, &mut node, env, 0)?;
        while node.literal.last() == Some(&b'\t') {
            node.literal.pop();
        }
        Ok(node)
    }

    /// Bound-source grid construction, including BasicCell and model inheritance.
    /// Explicit row indices are generated by the existing dataset/view source.
    /// Conditional resource directives call the shared source predicate, not a
    /// default-true substitute. Display invokes the existing field dispatcher.
    pub fn schema(
        &self,
        factory: &Factory,
        indices: &[i32],
        mut condition: impl FnMut(&[u8]) -> io::Result<bool>,
    ) -> io::Result<Grid> {
        self.bound(|source| self.schema_at(source, factory, indices, &mut condition))
    }

    fn schema_at(
        &self,
        source: usize,
        factory: &Factory,
        indices: &[i32],
        mut condition: impl FnMut(&[u8]) -> io::Result<bool>,
    ) -> io::Result<Grid> {
        let style_name = format!("{}_Style", factory.resource);
        let style = self.entry_at(source, style_name.as_bytes())?.concat();
        let scalar = |key: &[u8], default| -> io::Result<usize> {
            if let Some(at) = style.windows(key.len()).position(|v| v == key) {
                let tail = &style[at + key.len()..];
                let end = tail
                    .iter()
                    .position(|b| *b == b')')
                    .ok_or_else(|| invalid("native grid style scalar unavailable"))?;
                usize::try_from(integer(&tail[..end])?)
                    .map_err(|_| invalid("negative native fixed axis"))
            } else {
                Ok(default)
            }
        };
        let fixed_columns = scalar(b"(FixedCols=", 1)?;
        let fixed_rows = scalar(b"(FixedRows=", 1)?;
        let mut basic = CellNode::default();
        let mut env = 0;
        for value in self.entry_at(source, format!("{}_BasicCell", factory.resource).as_bytes())? {
            basic = self.cell_at(source, value, &basic, &mut env)?;
        }
        let rows = self.entry_at(source, format!("{}_Cells", factory.resource).as_bytes())?;
        let (mut nodes, mut i) = (Vec::new(), 0);
        while i < rows.len() {
            let line = rows[i];
            i += 1;
            if let Some(command) = line.strip_prefix(b":(").and_then(|v| v.strip_suffix(b")")) {
                if let Some(count) = command.strip_prefix(b"LoopFrom") {
                    let end = rows[i..]
                        .iter()
                        .position(|v| *v == b":(LoopTo)")
                        .ok_or_else(|| invalid("native cell loop has no end"))?
                        + i;
                    let count = trim(count);
                    let values = if count.is_empty() {
                        indices.to_vec()
                    } else {
                        let n = integer(count)?;
                        if !(0..=20000).contains(&n) {
                            return Err(invalid("native cell loop outside bound"));
                        }
                        (env..env
                            .checked_add(n)
                            .ok_or_else(|| invalid("native loop index overflow"))?)
                            .collect()
                    };
                    for index in values {
                        env = index;
                        for row in &rows[i..end] {
                            if row.starts_with(b":(") {
                                return Err(invalid("nested native loop directive"));
                            }
                            nodes.push(
                                split(row, b'|')
                                    .into_iter()
                                    .map(|s| self.cell_at(source, s, &basic, &mut env))
                                    .collect::<io::Result<Vec<_>>>()?,
                            );
                        }
                    }
                    i = end + 1;
                } else if command == b"LoopTo" {
                    return Err(invalid("native cell loop end without start"));
                } else {
                    let parts: Vec<_> = command
                        .split(|b| b.is_ascii_whitespace())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if parts.len() > 2 || parts.is_empty() {
                        return Err(invalid("invalid native conditional directive"));
                    }
                    let count = if parts.len() == 2 {
                        usize::try_from(integer(parts[1])?)
                            .map_err(|_| invalid("negative native condition extent"))?
                    } else {
                        1
                    };
                    let retain = if parts[0] == b"Hide_Filter_DD"
                        && factory.class == Class::Common
                        && factory.resource == "GI_Sheet_Filter"
                    {
                        crate::display::filter::dd_rows_visible()
                    } else {
                        condition(parts[0])?
                    };
                    if !retain {
                        i = i
                            .checked_add(count)
                            .filter(|end| *end <= rows.len())
                            .ok_or_else(|| invalid("native condition outside resource rows"))?;
                    }
                }
            } else {
                nodes.push(
                    split(line, b'|')
                        .into_iter()
                        .map(|s| self.cell_at(source, s, &basic, &mut env))
                        .collect::<io::Result<Vec<_>>>()?,
                );
            }
        }
        let columns = nodes.iter().map(Vec::len).max().unwrap_or(0);
        if nodes.len() < fixed_rows || columns < fixed_columns {
            return Err(invalid("native cells do not cover fixed axes"));
        }
        for row in &mut nodes {
            row.resize_with(columns, || basic.clone());
        }
        let cells = nodes
            .iter()
            .map(|row| row.iter().map(|n| byte_text(&n.literal)).collect())
            .collect();
        let column_boundaries = (0..columns)
            .map(|c| {
                nodes[..fixed_rows]
                    .iter()
                    .any(|row| row[c].flags1c & 0x80 != 0)
            })
            .collect();
        let row_boundaries = nodes
            .iter()
            .map(|row| row[..fixed_columns].iter().any(|n| n.above != 0))
            .collect();
        Ok(Grid {
            name: format!("{}/{}", factory.resource, factory.node),
            cells,
            fixed_rows,
            fixed_columns,
            column_boundaries,
            row_boundaries,
            nodes: Some(nodes),
            factory: Some(factory.clone()),
            show_split: 0,
        })
    }

    pub fn no_print(&self, factory: &Factory) -> io::Result<bool> {
        self.bound(|source| {
            Ok(self
                .entry_at(source, format!("{}_Style", factory.resource).as_bytes())?
                .iter()
                .any(|line| line.windows(9).any(|s| s == b"(NoPrint)")))
        })
    }

    pub fn eligible(
        &self,
        current: &[u8],
        factory: &Factory,
        show_split: u32,
        views: &mut super::Views,
    ) -> io::Result<bool> {
        let disabled = self.no_print(factory)?;
        let Some(mut view) = factory.view(current, show_split)? else {
            return Ok(!disabled);
        };
        view.bc = (view.bc & !1) | u8::from(disabled);
        let id = views.register(view);
        let answer = views.nonempty(current, id)?;
        views.deactivate(id);
        Ok(answer)
    }

    /// Generate loop rows and split merge metadata from the live image before
    /// reaching the shared display dispatcher. Unknown bindings never mean empty.
    pub fn instantiate(
        &self,
        image: &crate::icf::Icf<'_>,
        current: &[u8],
        factory: &Factory,
        show_split: u32,
        condition: impl FnMut(&[u8]) -> io::Result<bool>,
    ) -> io::Result<Grid> {
        let view = factory.view(current, show_split)?;
        let count = match view.as_ref().map(|v| v.kind) {
            Some(ViewKind::Memory | ViewKind::Call) => {
                let v = view.as_ref().unwrap();
                super::split_index(
                    current,
                    v,
                    if v.kind == ViewKind::Memory { 200 } else { 8 },
                    false,
                )?
            }
            Some(ViewKind::Gps | ViewKind::Repeater | ViewKind::Ur | ViewKind::Allowed) => {
                let v = view.as_ref().unwrap();
                crate::icf::record_image::print_dataset_count(
                    current,
                    v.record_type().unwrap(),
                    v.group,
                )
                .map_err(super::data_error)? as i32
            }
            _ if factory.class == Class::Groups => {
                image.memory_groups().map_err(super::data_error)?.len() as i32
            }
            _ => {
                if self
                    .entry(format!("{}_Cells", factory.resource).as_bytes())?
                    .contains(&b":(LoopFrom)".as_slice())
                {
                    return Err(invalid(
                        "native factory has no registered dynamic row producer",
                    ));
                }
                0
            }
        };
        if !(0..=20000).contains(&count) {
            return Err(invalid("native print row count outside grid bound"));
        }
        let indices: Vec<_> = (0..count).collect();
        let mut grid = self.schema(factory, &indices, condition)?;
        grid.show_split = show_split;
        let nodes = grid.nodes.as_mut().unwrap();
        for (r, row) in nodes.iter_mut().enumerate() {
            for node in row.iter_mut() {
                if factory.class == Class::Preset && node.key == 0x385 {
                    let preset = usize::try_from(factory.group)
                        .map_err(|_| invalid("negative native preset group"))?;
                    let field = u32::try_from(node.index)
                        .map_err(|_| invalid("negative native preset checkbox field"))?;
                    node.checked = crate::icf::special::Preset::read(current, preset)
                        .and_then(|holder| holder.checkbox_paint(field))
                        .map_err(super::data_error)?
                        .1;
                }
                if r >= grid.fixed_rows
                    && let Some(v) = &view
                    && matches!(v.kind, ViewKind::Memory | ViewKind::Call)
                {
                    let typed = super::split_index(current, v, node.index, true)?;
                    let typed_index =
                        u32::try_from(typed).map_err(|_| invalid("negative native split index"))?;
                    if typed_index >= if v.kind == ViewKind::Memory { 200 } else { 8 } {
                        return Err(invalid("native split index outside dataset"));
                    }
                    let record_row = typed_index / 2;
                    let paired = show_split == 1
                        || crate::icf::record_image::print_channel_row(
                            current,
                            v.record_type().unwrap(),
                            v.group,
                            record_row,
                        )
                        .map_err(super::data_error)?
                            == Some(true);
                    let side = (typed & 1) as usize;
                    node.typed_index = Some(if v.kind == ViewKind::Memory {
                        v.group as u32 * 200 + typed_index
                    } else {
                        typed_index
                    });
                    node.comb = typed / 2;
                    node.above = side;
                    node.below = usize::from(paired).saturating_sub(side);
                    node.merge = paired && matches!(node.key, 0x105 | 0x106 | 0x117);
                    node.record_visible = side == 0 || paired;
                }
            }
        }
        grid.row_boundaries = nodes
            .iter()
            .map(|row| row[..grid.fixed_columns].iter().any(|n| n.above != 0))
            .collect();
        Ok(grid)
    }

    pub fn produce(
        &self,
        image: &crate::icf::Icf<'_>,
        current: &[u8],
        factory: &Factory,
        show_split: u32,
        condition: impl FnMut(&[u8]) -> io::Result<bool>,
        mut display: impl FnMut(&CellNode) -> io::Result<Vec<u8>>,
    ) -> io::Result<Grid> {
        let mut grid = self.instantiate(image, current, factory, show_split, condition)?;
        for (r, row) in grid.nodes.as_ref().unwrap().iter().enumerate() {
            for (c, node) in row.iter().enumerate() {
                if !node.record_visible && (0x105..=0x117).contains(&node.key) {
                    grid.cells[r][c].clear();
                } else if node.key != 0x3ae
                    && node.flags1c & 0x80 == 0
                    && !(node.merge && node.above != 0)
                {
                    grid.cells[r][c] = byte_text(&display(node)?);
                }
            }
        }
        Ok(grid)
    }

    /// Cancellation still reaches the concrete factory and predicate first.
    /// This draws the resource grid through the same span/page/job consumer.
    pub fn render(
        &self,
        current: &[u8],
        grid: &Grid,
        layout: &super::Layout,
        views: &mut super::Views,
        job: &mut super::Job,
        paper: super::Paper,
        metrics: super::Metrics,
        html: &mut String,
        mut display: impl FnMut(&CellNode) -> io::Result<Vec<u8>>,
    ) -> io::Result<()> {
        let factory = grid
            .factory
            .as_ref()
            .ok_or_else(|| invalid("native print grid has no factory"))?;
        if self.eligible(current, factory, grid.show_split, views)? && !job.cancelled {
            grid.measure(metrics.font_height, metrics)?;
            let mut painted = grid.clone();
            for page in &layout.pages {
                if !self.eligible(current, factory, grid.show_split, views)? || job.cancelled {
                    break;
                }
                if page.columns.start < grid.fixed_columns
                    || page.columns.end > grid.column_boundaries.len()
                    || page.rows.start < grid.fixed_rows
                    || page.rows.end > grid.cells.len()
                    || page.columns.is_empty()
                    || page.rows.is_empty()
                {
                    return Err(invalid("native page does not belong to its generated grid"));
                }
                job.begin_page();
                let mut visited = std::collections::BTreeSet::new();
                for r in (0..grid.fixed_rows).chain(page.rows.clone()) {
                    for c in (0..grid.fixed_columns).chain(page.columns.clone()) {
                        let (columns, rows) = grid.cell_span(c, r)?;
                        let (c, r) = (columns.start, rows.start);
                        if !visited.insert((c, r)) {
                            continue;
                        }
                        let node = &grid
                            .nodes
                            .as_ref()
                            .ok_or_else(|| invalid("native paint nodes unavailable"))?[r][c];
                        if !node.record_visible && (0x105..=0x117).contains(&node.key) {
                            painted.cells[r][c].clear();
                        } else if node.key != 0x3ae {
                            painted.cells[r][c] = byte_text(&display(node)?);
                        }
                    }
                }
                html.push_str(&super::html_page(
                    &painted,
                    page,
                    paper,
                    metrics,
                    job.pages + 1,
                )?);
                job.page_drawn();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printing::{Metrics, Paper, html_page, layout};

    fn localized_source() -> Resources<'static> {
        let entries = |literal: &'static [u8], model: &'static [u8]| {
            BTreeMap::from([
                (b"CellModel_Basic".as_slice(), vec![model]),
                (
                    b"GI_Tree_Main_Body".as_slice(),
                    vec![b"caption{(%CallSheet:[St_Common]GI_Sheet_Local,403)}".as_slice()],
                ),
                (
                    b"GI_Sheet_Local_Style".as_slice(),
                    vec![b"(FixedCols=0)(FixedRows=0)".as_slice()],
                ),
                (
                    b"GI_Sheet_Local_BasicCell".as_slice(),
                    vec![b"{($Basic)}".as_slice()],
                ),
                (
                    b"GI_Sheet_Local_Cells".as_slice(),
                    vec![literal, b"Value{(%CL_Value)}".as_slice()],
                ),
                (
                    b"GI_Menu_Main_BasicCell".as_slice(),
                    vec![b"{($Basic)}".as_slice()],
                ),
                (b"GI_Menu_Main_Body".as_slice(), vec![literal]),
            ])
        };
        Resources {
            pe: &[],
            entries: [
                entries(b"JP-system", b"<-WH"),
                entries(b"JP-other", b"^-WH"),
                entries(b"English", b">-WH"),
            ],
            keys: BTreeMap::from([(b"CL_Value".as_slice(), 0x136)]),
            source: None,
        }
    }

    #[test]
    fn r2_language_binds_schema_models_menu_and_live_producer_without_fallback() {
        let factory = Factory::parse(b"[St_Common]GI_Sheet_Local,403").unwrap();
        let source = localized_source();
        assert_eq!(source.tree(0).unwrap(), [factory.clone()]);
        assert!(
            source
                .schema(&factory, &[], |_| panic!("no condition"))
                .is_err()
        );
        assert!(source.commands(b"GI_Menu_Main").is_err());
        let mut memory = vec![0; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        let image = crate::icf::Icf::from_memory(memory).unwrap();
        for (gui, system, literal, alignment) in [
            (1, true, b"JP-system".as_slice(), 0),
            (1, false, b"JP-other".as_slice(), 1),
            (0, true, b"English".as_slice(), 2),
            (2, false, b"English".as_slice(), 2),
            (u32::MAX, true, b"English".as_slice(), 2),
        ] {
            let source = localized_source().with_language(gui, system);
            let grid = source
                .produce(
                    &image,
                    image.memory(),
                    &factory,
                    0,
                    |_| panic!("no condition"),
                    |node| {
                        assert_eq!(node.key, 0x136);
                        Ok(b"live".to_vec())
                    },
                )
                .unwrap();
            assert_eq!(grid.nodes.as_ref().unwrap()[0][0].literal, literal);
            assert_eq!(grid.nodes.as_ref().unwrap()[0][0].flags1c & 3, alignment);
            assert_eq!(grid.cells[1][0], "live");
            let commands = source.commands(b"GI_Menu_Main").unwrap();
            assert_eq!(commands[0].node.literal, literal);
            assert_eq!(commands[0].node.flags1c & 3, alignment);
        }
        let mut source = localized_source().with_language(0, false);
        source.entries[2].remove(b"CellModel_Basic".as_slice());
        assert!(
            source
                .schema(&factory, &[], |_| panic!("no condition"))
                .is_err()
        );
        assert!(source.commands(b"GI_Menu_Main").is_err());
    }

    #[test]
    fn r2_filter_dd_is_the_original_specific_noop_not_unknown_name_success() {
        let factory = Factory::parse(b"[St_Common]GI_Sheet_Filter,403").unwrap();
        let mut source = localized_source().with_language(1, true);
        source.entries[0].extend([
            (
                b"GI_Sheet_Filter_Style".as_slice(),
                vec![b"(FixedCols=0)(FixedRows=0)".as_slice()],
            ),
            (
                b"GI_Sheet_Filter_BasicCell".as_slice(),
                vec![b"{($Basic)}".as_slice()],
            ),
            (
                b"GI_Sheet_Filter_Cells".as_slice(),
                vec![
                    b"before".as_slice(),
                    b":(Hide_Filter_DD 6)",
                    b"DD0",
                    b"DD1",
                    b"DD2",
                    b"DD3",
                    b"DD4",
                    b"DD5",
                    b"after",
                ],
            ),
        ]);
        assert!(crate::display::filter::dd_rows_visible());
        let grid = source
            .schema(&factory, &[], |_| panic!("IC-705 has no DD predicate"))
            .unwrap();
        assert_eq!(
            grid.cells
                .iter()
                .map(|row| row[0].as_str())
                .collect::<Vec<_>>(),
            ["before", "DD0", "DD1", "DD2", "DD3", "DD4", "DD5", "after"]
        );
        let mut wrong = factory.clone();
        wrong.class = Class::Base;
        assert!(
            source
                .schema(&wrong, &[], |_| Err(invalid("unregistered condition")))
                .is_err()
        );
        source.entries[0]
            .get_mut(b"GI_Sheet_Filter_Cells".as_slice())
            .unwrap()[1] = b":(Unknown 6)";
        assert!(
            source
                .schema(&factory, &[], |_| Err(invalid("unregistered condition")))
                .is_err()
        );
    }

    #[test]
    #[ignore = "requires CS705_PRINT_SOURCE pointing to the approved original PE; bounded read only"]
    fn r2_original_language_drain_rebinds_real_schema_and_filter() {
        use crate::display::{FontMetrics, Settings, State};
        let bytes =
            read_source(std::fs::File::open(std::env::var("CS705_PRINT_SOURCE").unwrap()).unwrap())
                .unwrap();
        let settings = Settings {
            language: 0,
            system_language: 0,
            font_percent: 100,
            toolbar: 1,
            split: 0,
            callsign_dialog: 1,
            auto_correct: 1,
            fixed_name: 0,
            column_bits: 0,
            column_bits_high: 0,
        };
        let mut state = State::new(
            settings,
            FontMetrics {
                em: 16,
                advance: 8,
                line: 16,
                dpi: 96,
                padding: 1,
                explicit_height: 0,
            },
            [0, 0, 100, 20],
        )
        .unwrap();
        let calls = Factory::parse(b"[St_Common]GI_Sheet_My_Callsign,200").unwrap();
        let schema = |state: &State| {
            state
                .resources(&bytes)
                .unwrap()
                .schema(&calls, &[], |_| panic!("no condition"))
                .unwrap()
        };
        let english = schema(&state);
        let literals = |grid: &Grid, text: &[u8]| {
            grid.nodes
                .as_ref()
                .unwrap()
                .iter()
                .flatten()
                .any(|node| node.literal == text)
        };
        assert!(literals(&english, b"Select"));
        assert!(literals(&english, b"Call Sign"));
        assert!(!literals(&english, b"\x91I\x91\xf0"));
        let mut checked = 0;
        let mut disabled = 0;
        assert!(
            state
                .menu(&[], 0x37, 0x0d, 1, &mut checked, &mut disabled, None)
                .is_err()
        );
        assert_eq!(state.settings.language, 0);
        assert!(state.queue.is_empty());
        state
            .menu(&[], 0x37, 0x0d, 1, &mut checked, &mut disabled, Some(0))
            .unwrap();
        assert_eq!(state.settings.language, 0);
        assert!(state.queue.is_empty());
        state
            .menu(&[], 0x37, 0x0d, 1, &mut checked, &mut disabled, Some(1))
            .unwrap();
        assert_eq!((state.settings.language, state.resource_language), (1, 0));
        assert_eq!(schema(&state), english);
        state.drain(&[], None).unwrap();
        assert_eq!((state.resource_language, state.resource_generation), (1, 1));
        assert!(literals(&schema(&state), b"\x91I\x91\xf0"));
        let resources = state.resources(&bytes).unwrap();
        let filter = resources
            .tree(0)
            .unwrap()
            .into_iter()
            .find(|f| f.resource == "GI_Sheet_Filter")
            .unwrap();
        let grid = resources
            .schema(&filter, &[], |_| panic!("no registered DD predicate"))
            .unwrap();
        assert!(
            grid.nodes
                .as_ref()
                .unwrap()
                .iter()
                .flatten()
                .any(|node| node.index >= 45)
        );
    }

    #[test]
    fn metadata_captions_reject_uncaptured_sources_instead_of_defaulting() {
        use crate::icf::settings::extended::MetadataField as F;
        let mut source = Resources {
            pe: &[],
            source: Some(0),
            entries: [
                BTreeMap::from([(
                    b"GI_IPAddressListNotUsed".as_slice(),
                    vec![b"fixture".as_slice()],
                )]),
                BTreeMap::new(),
                BTreeMap::new(),
            ],
            keys: BTreeMap::new(),
        };
        assert_eq!(
            source
                .metadata_empty_caption(F::ConnectionIpList, Some(1), Some(0x411))
                .unwrap(),
            b"fixture"
        );
        assert!(
            source
                .metadata_empty_caption(F::ConnectionIpList, Some(0), None)
                .is_err()
        );
        assert!(
            source
                .metadata_empty_caption(F::TransmitPicture, Some(1), None)
                .is_err()
        );
        source.entries[0].insert(b"GI_IPAddressListNotUsed", vec![b""]);
        assert!(
            source
                .metadata_empty_caption(F::ConnectionIpList, Some(1), Some(0x411))
                .is_err()
        );
        source.entries[0].clear();
        assert!(
            source
                .metadata_empty_caption(F::ConnectionIpList, Some(1), Some(0x411))
                .is_err()
        );
        source.entries[0].insert(b"GI_SecUnit", vec![b"fixture"]);
        assert_eq!(
            source
                .resource_line(b"GI_SecUnit", 0, Some(1), Some(0x411))
                .unwrap(),
            b"fixture"
        );
        assert!(
            source
                .resource_line(b"GI_SecUnit", 0, Some(0), None)
                .is_err()
        );
        assert!(source.resource_line(b"GI_SecUnit", 0, None, None).is_err());
        assert!(
            source
                .resource_line(b"GI_SecUnit", 1, Some(1), Some(0x411))
                .is_err()
        );
        assert!(
            source
                .resource_line(b"GI_CsvExp_RxHis_HeaderRptInfo", 0, None, None)
                .is_err()
        );
        assert!(source.resource_line(b"unknown", 0, Some(1), None).is_err());
    }

    #[test]
    fn resource_flags_merge_spans_and_inline_frame_share_the_same_nodes() {
        let entries: BTreeMap<&[u8], Vec<&[u8]>> = BTreeMap::from([
            (b"CellModel_Basic".as_slice(), vec![b"<-WH".as_slice()]),
            (
                b"GI_Sheet_Test_Style".as_slice(),
                vec![b"(FixedCols=1)(FixedRows=2)".as_slice()],
            ),
            (
                b"GI_Sheet_Test_BasicCell".as_slice(),
                vec![b"{($Basic)(%CT_Common.)}".as_slice()],
            ),
            (
                b"GI_Sheet_Test_Cells".as_slice(),
                vec![
                    b"#|{B}Header|{+}".as_slice(),
                    b"|left|right",
                    b":(LoopFrom)",
                    b"|{(%CL_Value[@ch0])X}WW|value",
                    b":(LoopTo)",
                ],
            ),
        ]);
        let source = Resources {
            pe: &[],
            source: Some(0),
            entries: [entries, BTreeMap::new(), BTreeMap::new()],
            keys: BTreeMap::from([
                (b"CT_Common".as_slice(), 25),
                (b"CL_Value".as_slice(), 0x136),
            ]),
        };
        let factory = Factory::parse(b"[St_Common]GI_Sheet_Test,200").unwrap();
        let ansi = source
            .cell_at(
                0,
                b"{(Font=\x83\x5c)}\x83\x7c\\|",
                &CellNode::default(),
                &mut 0,
            )
            .unwrap();
        assert_eq!(ansi.literal, [0x83, 0x7c, b'|']);
        assert_eq!(split(b"|\x83\x7c\\||", b'|').len(), 3);
        let grid = source
            .schema(&factory, &[0, 1], |_| panic!("no condition in source"))
            .unwrap();
        assert_eq!(grid.column_boundaries, [false, false, true]);
        assert_eq!(grid.nodes.as_ref().unwrap()[0][1].flags1d & 1, 1);
        let frame = InlineFrame::commit(&grid, 1, 3, b"new text").unwrap();
        assert_eq!(
            (frame.node.key, frame.node.index, frame.node.group),
            (0x136, 1, 25)
        );
        assert_eq!(
            (frame.action, frame.command, frame.phase, frame.notify),
            (0, 0, 1, 1)
        );
        assert_eq!(
            (frame.input_code, frame.warning_bits, frame.focus),
            (13, 1, 0)
        );
        assert_eq!(frame.initial_flags, 1);
        assert!(frame.grid4 && frame.grid8);
        let mut editor = crate::printing::Editor {
            validation_guard: 7,
            visible: true,
            text: b"new text".to_vec(),
        };
        let mut prefix = Vec::new();
        let failed = sync_editor(&mut editor, &grid, 1, 3, |frame| {
            prefix.extend_from_slice(&frame.text[..3]);
            Err::<(), _>(invalid("mock dispatcher failed after its first write"))
        });
        assert!(failed.is_err() && editor.visible && editor.validation_guard == 0);
        assert_eq!(prefix, b"new");
        sync_editor(&mut editor, &grid, 1, 3, |frame| {
            assert_eq!(frame.phase, 1);
            Ok::<_, io::Error>(())
        })
        .unwrap();
        sync_editor::<io::Error>(&mut editor, &grid, 1, 3, |_| {
            panic!("hidden editor cannot dispatch twice")
        })
        .unwrap();
        assert!(!editor.visible);
        assert!(InlineFrame::commit(&grid, 2, 0, b"bad").is_err());
        let metrics = Metrics {
            font_height: -12,
            em: 12,
            advance_em: 6,
            line_em: 12,
            padding: 1,
        };
        let paper = Paper {
            width: 100,
            height: 100,
        };
        let plan = layout(&grid, paper, metrics).unwrap();
        assert_eq!(plan.horizontal, [0, 2]);
        let html = html_page(&grid, &plan.pages[0], paper, metrics, 1).unwrap();
        assert_eq!(html.matches("Header</text>").count(), 1);
        assert!(html.contains("data-node-1c") && html.contains("<path "));
        let mut views = super::super::Views::default();
        let mut job = super::super::Job::begin(vec![grid.name.clone()]).unwrap();
        let mut painted = String::new();
        source
            .render(
                &[],
                &grid,
                &plan,
                &mut views,
                &mut job,
                paper,
                metrics,
                &mut painted,
                |_| Ok(b"paint".to_vec()),
            )
            .unwrap();
        assert!(painted.contains(">paint</text>"));
        let prefix = painted.clone();
        assert!(
            source
                .render(
                    &[],
                    &grid,
                    &plan,
                    &mut views,
                    &mut job,
                    paper,
                    metrics,
                    &mut painted,
                    |_| Err(invalid("mock paint failure"))
                )
                .is_err()
        );
        assert_eq!(painted, prefix);
        assert_eq!(job.pages, 1);
        assert_eq!(job.events.last(), Some(&super::super::Event::NewPage));
        assert!(!job.ended);
        job.cancelled = true;
        source
            .render(
                &[],
                &grid,
                &plan,
                &mut views,
                &mut job,
                paper,
                metrics,
                &mut painted,
                |_| panic!("Cancel must not query paint data"),
            )
            .unwrap();
        assert!(Factory::parse(b"[Unknown]GI_Sheet_Test").is_err());
        assert_eq!(
            Factory::parse(b"GI_Sheet_My_Callsign,200").unwrap().class,
            Class::Base
        );
        let mut node = CellNode::default();
        source
            .format(0, b"X!BQ(Comb=2,1,2)", &mut node, &mut 0, 0)
            .unwrap();
        assert_eq!(
            (
                node.flags1d & 7,
                node.comb,
                node.above,
                node.below,
                node.merge
            ),
            (4, 2, 1, 2, true)
        );
    }

    #[test]
    #[ignore = "requires the original private PE asset; bounded read only"]
    fn native_provider_captures_preserve_language_branches_and_symbol_targets() {
        let path = std::env::var("CS705_PRINT_SOURCE").expect("CS705_PRINT_SOURCE");
        let bytes = read_source(std::fs::File::open(path).unwrap()).unwrap();
        let source = Resources::from_pe(&bytes).unwrap().with_language(1, true);
        use crate::icf::settings::extended::MetadataField as F;
        let ip = source
            .metadata_empty_caption(F::ConnectionIpList, Some(1), None)
            .unwrap();
        let other_ip = source
            .metadata_empty_caption(F::ConnectionIpList, Some(0), None)
            .unwrap();
        assert!(!ip.is_empty() && !other_ip.is_empty());
        assert_ne!(ip, other_ip);
        assert_eq!(
            other_ip,
            source
                .metadata_empty_caption(F::ConnectionIpList, Some(2), None)
                .unwrap()
        );
        let picture = source
            .metadata_empty_caption(F::TransmitPicture, None, None)
            .unwrap();
        assert_eq!(picture.len(), 3);
        assert_eq!(
            picture,
            source
                .metadata_empty_caption(F::TransmitPicture, Some(1), None)
                .unwrap()
        );
        let seconds = source
            .resource_line(b"GI_SecUnit", 0, Some(1), None)
            .unwrap();
        let other_seconds = source
            .resource_line(b"GI_SecUnit", 0, Some(0), None)
            .unwrap();
        assert_ne!(seconds, other_seconds);
        assert_eq!(
            other_seconds,
            source
                .resource_line(b"GI_SecUnit", 0, Some(2), None)
                .unwrap()
        );
        assert_eq!(
            source
                .resource_line(b"GI_CsvExp_RxHis_HeaderRptInfo", 0, None, None)
                .unwrap(),
            b""
        );
        for index in 1..4 {
            assert!(
                !source
                    .resource_line(b"GI_CsvExp_RxHis_HeaderRptInfo", index, None, None)
                    .unwrap()
                    .is_empty()
            );
        }
        assert!(
            source
                .resource_line(b"GI_CsvExp_RxHis_HeaderRptInfo", 4, Some(1), None)
                .is_err()
        );
        let locales = [
            (Some(1), Some(0x411)),
            (Some(1), Some(0x409)),
            (Some(0), None),
        ];
        // Independent expected bytes from10FD4/11DFF, not selector self-comparison.
        for (cell, expected) in [
            (
                b"row{S(%Flex0[7]:left(right\\)/end)B}".as_slice(),
                b"left(right)/end".as_slice(),
            ),
            (b"row{S(%Flex0[7]:\x81\x5c\\)tail)B}", b"\x81\x5c)tail"),
            (b"row{S(%Flex0[7]:a\\\\b)B}", b"a\\b"),
        ] {
            let node = source
                .cell_at(0, cell, &CellNode::default(), &mut 0)
                .unwrap();
            assert_eq!(node.alias.as_deref(), Some(expected));
            assert_eq!((node.key, node.index, node.flags1d & 9), (1, 7, 9));
            assert_eq!(node.literal, b"row");
        }
        assert!(
            source
                .cell_at(0, b"{(%Flex0[0]:a\\)}", &CellNode::default(), &mut 0)
                .is_err()
        );
        let unknown = source
            .cell_at(0, b"{?}", &CellNode::default(), &mut 0)
            .unwrap_err();
        assert!(unknown.to_string().contains("token0x3F"));
        // Identifier padding is not part of the independently expected alias.
        for (cell, index, expected) in [
            (b"{(%Flex0[0  ]\t:  )}".as_slice(), 0, b"  ".as_slice()),
            (
                b"{(%Flex0[20 ]\t:  lead(inner\\)/tail  )}",
                20,
                b"  lead(inner)/tail  ",
            ),
            (b"{(%Flex0[100]\t: last )}", 100, b" last "),
        ] {
            let node = source
                .cell_at(0, cell, &CellNode::default(), &mut 0)
                .unwrap();
            assert_eq!((node.key, node.index), (1, index));
            assert_eq!(node.alias.as_deref(), Some(expected));
        }
        for (resource, key, value) in [
            (b"GI_Sheet_Sel_Misc_OnOff2".as_slice(), 1, 0),
            (b"GI_Sheet_Sel_Mch_Mode", 7, 0),
            (b"GI_Sheet_Sel_Mch_Filter", 1, 1),
            (b"GI_Sheet_Sel_Mch_Dup", 1, 0),
            (b"GI_Sheet_Sel_Mch_TSQL", 1, 0),
            (b"GI_Sheet_Sel_Mch_DTCS", 1, 0),
            (b"GI_Sheet_Sel_Mch_DTCS_Polarity", 1, 2),
            (b"GI_Sheet_Sel_Mch_DSQL", 1, 0),
            (b"GI_Sheet_Sel_Mch_Scan_Sel", 1, 0),
            (b"GI_Sheet_Sel_GPS_Alarm", 1, 0),
            (b"GI_Sheet_Sel_Common_Gps_Alarm_Bank_Group", 1, 27),
            (b"GI_Sheet_Sel_Common_Gps_Alarm_Bank_CH", 1, 26),
        ] {
            let captions = locales.map(|(gui, system)| {
                source
                    .selector_caption(resource, key, value, gui, system)
                    .unwrap_or_else(|error| panic!(
                        "selector {} key0x{key:X} value{value} gui{gui:?} system{system:?}: {error}",
                        std::str::from_utf8(resource).unwrap(),
                    ))
            });
            for index in 0..3 {
                let node = source
                    .selector_cells_at(index, resource)
                    .unwrap()
                    .into_iter()
                    .find(|node| node.key == key && i64::from(node.index) == value)
                    .unwrap();
                assert_eq!(captions[index], node.alias.unwrap_or(node.literal));
            }
            if captions[0] == captions[1] && captions[0] == captions[2] {
                assert_eq!(
                    source
                        .selector_caption(resource, key, value, None, None)
                        .unwrap(),
                    captions[0]
                );
            } else {
                assert!(
                    source
                        .selector_caption(resource, key, value, None, None)
                        .is_err()
                );
            }
        }
        for (gui, system) in locales {
            assert!(
                source
                    .selector_caption(b"GI_Sheet_Sel_Mch_DTCS_Polarity", 1, 0, gui, system)
                    .is_err()
            );
        }
        let log = locales.map(|(gui, system)| {
            source
                .resource_line(b"GI_GpsLogSentence", 1, gui, system)
                .unwrap()
        });
        assert_eq!(log[0], log[1]);
        assert_ne!(log[0], log[2]);
        assert!(
            source
                .resource_line(b"GI_GpsLogSentence", 1, None, None)
                .is_err()
        );
        assert!(
            source
                .resource_line(b"GI_GpsLogSentence", 0, None, None)
                .is_ok()
        );
        assert_eq!(
            source
                .selector_caption(b"GI_Sheet_Sel_Mch_Dup", 1, 0, None, None)
                .unwrap(),
            b""
        );

        let mut current = vec![0; 0x3c9c0];
        current[0x355a1] = 6;
        current[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        let image = crate::icf::Icf::from_memory(current.clone()).unwrap();
        let preset = source
            .tree(0)
            .unwrap()
            .into_iter()
            .find(|factory| factory.class == Class::Preset && factory.group == 0)
            .unwrap();
        assert_eq!(CellNode::default().checked, None);
        for (mask, mode, field, checked) in [
            (0, 0, 1, Some(true)),
            (1, 0, 1, Some(false)),
            (255, 0, 1, Some(false)),
            (0, 9, 2, None),
        ] {
            current[0xa6dc] = mask;
            current[0xa60f] = mode;
            let grid = source
                .instantiate(&image, &current, &preset, 0, |_| Ok(true))
                .unwrap();
            let node = grid
                .nodes
                .as_ref()
                .unwrap()
                .iter()
                .flatten()
                .find(|node| node.key == 0x385 && node.index == field)
                .unwrap();
            assert_eq!(node.checked, checked);
            let holder = crate::icf::special::Preset::read(&current, 0).unwrap();
            let (text, expected) = holder.checkbox_paint(field as u32).unwrap();
            assert_eq!(node.checked, expected);
            assert_eq!(
                text,
                if checked.is_none() {
                    b"--".to_vec()
                } else {
                    Vec::new()
                }
            );
            assert_eq!(grid.clone().nodes, grid.nodes);
        }

        // Synthetic contrasts use the stock asset's real maps/keys/geometry.
        // They do not create a production bypass for flat or short vectors.
        let mut contrast = Resources::from_pe(&bytes).unwrap();
        for (index, value) in [b"one".as_slice(), b"two", b"three"]
            .into_iter()
            .enumerate()
        {
            contrast.entries[index].insert(b"Proof", vec![value]);
            contrast.entries[index].insert(b"Common", vec![b"same"]);
        }
        for ((gui, system), expected) in
            locales
                .into_iter()
                .zip([b"one".as_slice(), b"two", b"three"])
        {
            assert_eq!(
                contrast.resource_line(b"Proof", 0, gui, system).unwrap(),
                expected
            );
        }
        assert_eq!(
            contrast
                .resource_line(b"Proof", 0, Some(1), Some(0x8411))
                .unwrap(),
            b"one"
        );
        assert!(contrast.resource_line(b"Proof", 0, Some(1), None).is_err());
        assert!(
            contrast
                .resource_line(b"Proof", 0, None, Some(0x411))
                .is_err()
        );
        assert!(
            contrast
                .resource_line(b"Proof", 0, None, Some(0x409))
                .is_err()
        );
        assert!(contrast.resource_line(b"Proof", 0, None, None).is_err());
        assert_eq!(
            contrast.resource_line(b"Common", 0, None, None).unwrap(),
            b"same"
        );
        contrast.entries[2].remove(b"Common".as_slice());
        assert_eq!(
            contrast.resource_line(b"Common", 0, Some(1), None).unwrap(),
            b"same"
        );
        assert!(
            contrast
                .resource_line(b"Common", 0, None, Some(0x411))
                .is_err()
        );
        assert!(contrast.resource_line(b"Common", 0, Some(0), None).is_err());
        for entries in &mut contrast.entries {
            entries.insert(
                b"GI_Sheet_Sel_Proof_Style",
                vec![b"(FixedCols=0)(FixedRows=0)"],
            );
            entries.insert(b"GI_Sheet_Sel_Proof_BasicCell", vec![b"{($Basic)}"]);
            entries.insert(
                b"GI_Sheet_Sel_Proof_Cells",
                vec![
                    b"visible{(%Flex0[0]:)}",
                    b"visible{(%Flex0[1]:alias)}",
                    b"visible{(%Flex0[2])}",
                ],
            );
        }
        for (value, expected) in [(0, b"".as_slice()), (1, b"alias"), (2, b"visible")] {
            assert_eq!(
                contrast
                    .selector_caption(b"GI_Sheet_Sel_Proof", 1, value, None, None)
                    .unwrap(),
                expected
            );
        }
        for value in [-1, 0x1_0000_0000, i64::MAX] {
            assert!(
                contrast
                    .selector_caption(b"GI_Sheet_Sel_Proof", 1, value, None, None)
                    .is_err()
            );
        }
        for value in [-1, 0x1_0000_0000, i64::MAX] {
            assert!(
                source
                    .selector_caption(b"GI_Sheet_Sel_Mch_Mode", 7, value, None, None)
                    .is_err()
            );
        }
        assert!(
            source
                .selector_caption(b"GI_Sheet_Sel_Mch_Mode", u32::MAX, 0, None, None)
                .is_err()
        );
        assert!(
            contrast
                .selector_caption(b"GI_Sheet_Sel_Proof", u32::MAX, 0, None, None)
                .is_err()
        );
        contrast.entries[0].insert(
            b"GI_Sheet_Preset_Proof_Style",
            vec![b"(FixedCols=0)(FixedRows=0)"],
        );
        contrast.entries[0].insert(b"GI_Sheet_Preset_Proof_BasicCell", vec![b"{($Basic)}"]);
        contrast.entries[0].insert(
            b"GI_Sheet_Preset_Proof_Cells",
            vec![b"{(%CL_Common_Preset_CheckBox[-1])}"],
        );
        let negative = Factory::parse(b"[St_Preset]GI_Sheet_Preset_Proof,422").unwrap();
        assert!(
            contrast
                .instantiate(&image, &current, &negative, 0, |_| Ok(true))
                .is_err()
        );
        // Independently compare every locale target with its actual file bytes;
        // a sparse uncaptured target must not become an observed empty caption.
        use std::io::{Read, Seek};
        let mut original =
            std::fs::File::open(std::env::var("CS705_PRINT_SOURCE").unwrap()).unwrap();
        for locale in [0, 4] {
            for slot in (0..96)
                .flat_map(|i| [0x418f18 + 8 * i + locale, 0x419218 + 8 * i + locale])
                .chain(std::iter::once(0x419518 + locale))
            {
                let pointer =
                    u32::from_le_bytes(pe_tail(&bytes, slot).unwrap()[..4].try_into().unwrap());
                if pointer == 0 {
                    continue;
                }
                let rva = pointer - 0x400000;
                let mut expected = [0; 39];
                original
                    .seek(std::io::SeekFrom::Start(u64::from(rva - 0x1200)))
                    .unwrap();
                original.read_exact(&mut expected).unwrap();
                assert_eq!(&pe_tail(&bytes, rva).unwrap()[..39], expected);
            }
        }
        assert!(read_source(std::io::Cursor::new(bytes.clone())).is_ok());
        let mut altered = bytes.clone();
        altered[0x4077b8 - 0x1200..0x4077bc - 0x1200].copy_from_slice(&0x809704_u32.to_le_bytes());
        assert!(read_source(std::io::Cursor::new(altered)).is_err());
        let at = RESOURCE_VECTORS[0].0 as usize - 0x1200;
        let last = at + (RESOURCE_VECTORS[0].1 - 1) * 4;
        let first = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let end = u32::from_le_bytes(bytes[last..last + 4].try_into().unwrap());
        for (slot, pointer) in [(at, 0), (at, 0x400001), (at, end), (last, first)] {
            let mut altered = bytes.clone();
            altered[slot..slot + 4].copy_from_slice(&pointer.to_le_bytes());
            assert!(Resources::from_pe(&altered).is_err());
            assert!(read_source(std::io::Cursor::new(altered)).is_err());
        }
        let mut altered = bytes.clone();
        let target = (first - 0x400000) as usize - 0x1200;
        altered[target..target + STRING_BOUND].fill(b'X');
        assert!(Resources::from_pe(&altered).is_err());
        assert!(read_source(std::io::Cursor::new(altered)).is_err());
    }

    #[test]
    #[ignore = "requires the original private PE asset; bounded read only"]
    fn original_resource_schema_and_registered_factories() {
        let path = std::env::var("CS705_PRINT_SOURCE").expect("CS705_PRINT_SOURCE");
        let bytes = read_source(std::fs::File::open(path).unwrap()).unwrap();
        let source = Resources::from_pe(&bytes).unwrap().with_language(1, true);
        let labels = source.selector_cells(b"GI_Sheet_Sel_Mch_Dup").unwrap();
        assert!(labels.iter().any(|n| n.index == 4 && !n.literal.is_empty()));
        let current = vec![0; 0x3c9c0];
        let image = crate::icf::Icf::from_memory(current.clone()).unwrap();
        let call = Factory::parse(b"[St_ColCut_Split]GI_Sheet_CallCh_Split,103,0").unwrap();
        let split = source
            .instantiate(&image, &current, &call, 2, |_| {
                panic!("call has no Only rows")
            })
            .unwrap();
        assert_eq!(split.cells.len(), 5 + 8);
        let second = &split.nodes.as_ref().unwrap()[6][3];
        assert_eq!(second.typed_index, Some(1));
        assert!(split.row_boundaries[6]);
        assert!(!second.merge && !second.record_visible);
        assert!(InlineFrame::commit(&split, 3, 6, b"hidden").is_err());
        let tree = source.tree(2).unwrap();
        assert!(tree.iter().any(|f| f.resource == "GI_Sheet_My_Callsign"));
        assert!(tree.iter().any(|f| f.resource == "GI_Sheet_Message"));
        assert!(tree.iter().any(|f| f.class == Class::Common));
        assert_eq!(
            tree.iter()
                .filter(|f| f.resource == "GI_Sheet_MemoryCH_Split")
                .count(),
            2
        );
        for factory in &tree {
            source.no_print(factory).expect(&factory.resource);
            // Mock the Q gate only to check every registered resource grammar.
            // Runtime predicate values are tested by their existing owner.
            source
                .schema(factory, &[0, 1], |_| Ok(true))
                .expect(&factory.resource);
        }
        for target in [
            b"[St_ColCut_Split]GI_Sheet_CallCh_Split,103,0".as_slice(),
            b"[St_ChGps]GI_Sheet_GPS_BankCH,252,GpsMemory 0",
            b"[St_RptList]GI_Sheet_RepeaterList,123,RptCH 0",
            b"GI_Sheet_My_Callsign,200",
            b"GI_Sheet_Message,201",
        ] {
            let factory = Factory::parse(target).unwrap();
            let grid = source
                .schema(&factory, &[0, 1], |_| {
                    panic!("these resources have no conditional rows")
                })
                .unwrap();
            assert!(grid.nodes.is_some());
            assert_eq!(grid.fixed_columns, 3);
            if matches!(factory.class, Class::Split | Class::Repeater) {
                assert!(grid.column_boundaries.iter().any(|b| *b));
            }
        }
    }
}

/// Byte notation is an explicit portable glyph projection, not SJIS decoding.
pub fn byte_text(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for &b in bytes {
        if (32..=126).contains(&b) || b == b'\n' {
            out.push(char::from(b));
        } else if b == b'\t' {
            out.push(' ');
        } else {
            write!(out, "\\x{b:02X}").unwrap();
        }
    }
    out
}
