//! A local draft over a checked storage snapshot. No device or filesystem I/O.
//!
//! Only explicitly edited bytes are adopted into the native image. In
//! particular, a 5/6 roundtrip must not normalize unrelated settings.
//! New simplex entries use the explicit holder-reset constants and user input.
//! Native type-1D/G initialization and copying an existing entry stay separate.

use crate::{Result, encoding::TextEncoding};
use cs705::{
    clone::{self, Identity, Profile},
    icf::{
        self, Icf,
        record::{self, ChannelField, DecodedRecord},
        record_image::{self, ChannelKind},
        rows::NormalMemory,
    },
};

pub const STORAGE: usize = 0x3c9c0;
pub const NORMAL: [std::ops::Range<usize>; 2] = [0..0xa21c, 0x355c0..0x36821];
pub const NEW_MODES: [(&str, icf::MemoryMode); 9] = [
    ("FM", icf::MemoryMode::Fm),
    ("WFM", icf::MemoryMode::Wfm),
    ("AM", icf::MemoryMode::Am),
    ("LSB", icf::MemoryMode::Lsb),
    ("USB", icf::MemoryMode::Usb),
    ("CW", icf::MemoryMode::Cw),
    ("CW-R", icf::MemoryMode::CwReverse),
    ("RTTY", icf::MemoryMode::Rtty),
    ("RTTY-R", icf::MemoryMode::RttyReverse),
];

#[derive(Clone)]
pub struct Snapshot {
    pub native: Vec<u8>,
    pub profile: Profile,
    pub comment: Vec<u8>,
    pub extra: u32,
    pub identity: Option<Identity>,
    pub file_bytes: Option<Vec<u8>>,
}

impl Snapshot {
    pub fn from_file(bytes: &[u8]) -> Result<Self> {
        let file = icf::format::read(bytes)?;
        if file.status != icf::format::Status::Success {
            return Err("ICF structure, revision or coverage rejected".into());
        }
        let profile = file.layout.ok_or("missing ICF layout")?.selector();
        let canonical = match profile {
            5 => icf::convert_map_rev5_to6(bytes)?,
            6 => bytes.to_vec(),
            _ => return Err("this example supports only MapRev 5 and 6".into()),
        };
        // The normal native reader ignores CD. The strict parser checks it here.
        let checked = Icf::parse(&canonical)?;
        let snapshot = Self {
            native: file.image,
            profile: profile.try_into()?,
            comment: checked.comment_bytes().to_vec(),
            extra: checked.extra_data(),
            identity: None,
            file_bytes: Some(bytes.to_vec()),
        };
        snapshot.memory()?;
        Ok(snapshot)
    }

    pub fn memory(&self) -> Result<NormalMemory> {
        if !matches!(self.profile.selector(), 5 | 6) {
            return Err("this example supports only profiles 5 and 6".into());
        }
        if self.native.len() != clone::IMAGE_CAPACITY
            || self.native[0x355a1] != self.profile.selector()
        {
            return Err("native image size or revision disagrees with captured profile".into());
        }
        Ok(NormalMemory::from_icf(&self.canonical()?)?)
    }

    pub fn canonical(&self) -> Result<Icf<'static>> {
        let canonical = clone::convert_image(&self.native, self.profile, 6.try_into()?, None)?;
        Ok(Icf::from_memory(canonical[..STORAGE].to_vec())?)
    }

    pub fn icf(&self) -> Result<Vec<u8>> {
        Ok(icf::format::write_storage(
            &self.native[..STORAGE],
            icf::format::Layout::from_map_revision(u32::from(self.profile.selector())),
            &self.comment,
            self.extra,
        )?)
    }

    pub fn same_memory(&self, other: &Self) -> bool {
        self.profile == other.profile
            && NORMAL
                .iter()
                .all(|r| self.native[r.clone()] == other.native[r.clone()])
    }
}

pub struct Editor {
    pub encoding: TextEncoding,
    pub original: Snapshot,
    base: NormalMemory,
    pub draft: NormalMemory,
    pub settings: Icf<'static>,
    pub base_settings: Icf<'static>,
    pub edited_categories: Vec<crate::catalog::Category>,
    pub dirty: bool,
}

impl Editor {
    pub fn new(original: Snapshot) -> Result<Self> {
        let base = original.memory()?;
        let settings = original.canonical()?;
        Ok(Self {
            encoding: TextEncoding::default(),
            original,
            draft: base.clone(),
            base,
            base_settings: settings.clone(),
            settings,
            edited_categories: Vec::new(),
            dirty: false,
        })
    }

    pub fn group(&self, index: usize) -> String {
        group_name(&self.draft, index, self.encoding)
    }

    pub fn text(&self, bytes: &[u8]) -> String {
        self.encoding.display(bytes)
    }

    pub fn row(&self, group: usize, row: usize) -> Result<Option<DecodedRecord>> {
        row_holder(&self.draft, group, row)
    }

    pub fn original_row(&self, group: usize, row: usize) -> Result<Option<DecodedRecord>> {
        row_holder(&self.base, group, row)
    }

    pub fn candidate(&self) -> Result<Snapshot> {
        self.candidate_from(&self.draft)
    }

    fn candidate_from(&self, memory: &NormalMemory) -> Result<Snapshot> {
        let mut result = self.original.clone();
        // The two normal-memory domains have identical addresses in 5 and 6.
        // Keep the source's revision, excluded bytes and all other settings.
        // A settings-only edit must not canonicalize the source directory.
        if memory != &self.base {
            let saved = memory.to_icf()?;
            for range in NORMAL {
                result.native[range.clone()].copy_from_slice(&saved.memory()[range]);
            }
        }
        // Layouts 5/6 share these addresses. Adopt only semantic edit deltas,
        // never the converter's unrelated revision/normalization stores.
        for (at, (&old, &new)) in self
            .base_settings
            .memory()
            .iter()
            .zip(self.settings.memory())
            .enumerate()
        {
            if old != new {
                if NORMAL.iter().any(|r| r.contains(&at)) || at == 0x355a1 {
                    return Err("setting edit overlaps memory directory or revision".into());
                }
                if self.original.profile.selector() == 5
                    && ((at == 0x399bb && new > 1) || (at == 0x399c0 && new > 2))
                {
                    return Err("selected value requires profile 6".into());
                }
                result.native[at] = new;
            }
        }
        result.file_bytes = None;
        result.memory()?;
        Ok(result)
    }

    fn commit(&mut self, next: NormalMemory) -> Result<()> {
        let dirty = self.original.native != self.candidate_from(&next)?.native;
        self.draft = next;
        self.dirty = dirty;
        Ok(())
    }

    pub fn discard(&mut self) {
        self.draft = self.base.clone();
        self.settings = self.base_settings.clone();
        self.edited_categories.clear();
        self.dirty = false;
    }

    pub fn edit_settings(
        &mut self,
        edit: impl FnOnce(&mut Icf<'static>) -> Result<()>,
    ) -> Result<()> {
        let mut next = self.settings.clone();
        edit(&mut next)?;
        let previous = std::mem::replace(&mut self.settings, next);
        match self.candidate() {
            Ok(candidate) => {
                self.dirty = candidate.native != self.original.native;
                Ok(())
            }
            Err(error) => {
                self.settings = previous;
                Err(error)
            }
        }
    }

    pub fn rename(&mut self, group: usize, name: &str) -> Result<()> {
        if group >= 100 {
            return Err("group must be 00..99".into());
        }
        let old = &self.draft.working()[0x355c3 + 19 * group..0x355d3 + 19 * group];
        let name: [u8; 16] = self
            .encoding
            .fixed(name, old.try_into().expect("fixed group name width"))?;
        if group < self.draft.group_count() && name == old {
            return Ok(());
        }
        let mut next = self.draft.clone();
        ensure_group(&mut next, group)?;
        next.rename_group(group, Some(&name))?;
        self.commit(next)
    }

    pub fn edit(&mut self, group: usize, row: usize, name: &str, mhz: &str) -> Result<()> {
        let holder = self
            .row(group, row)?
            .ok_or("empty row: create a new channel instead of editing it")?;
        let name = self.encoding.fixed(name, holder.name())?;
        let old_hz = holder.channel_field(ChannelField::FrequencyHz);
        let hz = (mhz != mhz_text(old_hz))
            .then(|| frequency(mhz))
            .transpose()?;
        if name == *holder.name() && hz.is_none_or(|hz| hz == old_hz) {
            return Ok(());
        }
        let mut next = self.draft.clone();
        next.set_channel_name_and_frequency(group, row, &name, hz)?;
        self.commit(next)
    }

    /// Application-defined simplex creation, not native factory registration.
    /// Every active setting is explicit: no G/Q or existing channel is used as
    /// a substitute template. The shared codec preserves opaque unused bytes.
    pub fn create(
        &mut self,
        group: usize,
        row: usize,
        name: &str,
        mhz: &str,
        mode: icf::MemoryMode,
    ) -> Result<()> {
        if !NEW_MODES.iter().any(|&(_, candidate)| candidate == mode) {
            return Err(
                "new channels support analog modes only; DV requires additional settings".into(),
            );
        }
        let name = self.encoding.fixed(name, &[b' '; 16])?;
        let hz = frequency(mhz)?;
        let index = typed_index(group, row)?;
        let mut next = self.allocate_row(group, row)?;
        for side in 0..2 {
            let mut holder = record::reset_holder(2, index + side, 0);
            holder.set_group(group as u32);
            holder.set_name(&name);
            holder.set_channel_field(ChannelField::FrequencyHz, hz);
            holder.set_mode(record::decode_mode(mode as u8));
            holder.set_filter(if mode == icf::MemoryMode::Wfm { 15 } else { 0 });
            holder.set_data(0);
            holder.set_split(0);
            holder.set_offset_hz(0);
            holder.set_channel_field(ChannelField::Duplex, 0);
            holder.set_channel_field(ChannelField::Tone, 0);
            holder.set_channel_field(ChannelField::Selection, 0);
            next.store_channel(index + side, &mut holder)?;
        }
        self.commit(next)
    }

    fn allocate_row(&self, group: usize, row: usize) -> Result<NormalMemory> {
        typed_index(group, row)?;
        let mut next = self.draft.clone();
        ensure_group(&mut next, group)?;
        if next.slot(group, row)?.is_some() {
            return Err("destination is occupied; no overwrite was performed".into());
        }
        let allocation = next.allocate(group, row..row + 1)?;
        if allocation.allocated != 1 || allocation.exhausted {
            return Err("no free physical memory slot".into());
        }
        Ok(next)
    }

    pub fn copy(
        &mut self,
        source: (usize, usize),
        target: (usize, usize),
        name: &str,
        mhz: &str,
    ) -> Result<()> {
        let (group, row) = target;
        let source_holder = self
            .row(source.0, source.1)?
            .ok_or("copy source is empty")?;
        let name = self.encoding.fixed(name, source_holder.name())?;
        let old_hz = source_holder.channel_field(ChannelField::FrequencyHz);
        let hz = (mhz != mhz_text(old_hz))
            .then(|| frequency(mhz))
            .transpose()?;
        typed_index(group, row)?;
        let mut next = self.draft.clone();
        ensure_group(&mut next, group)?;
        next.copy_channel(source, target)?;
        next.set_channel_name_and_frequency(group, row, &name, hz)?;
        self.commit(next)
    }

    pub fn changes(&self) -> Result<Vec<String>> {
        let mut changes = Vec::new();
        for group in 0..100 {
            if group_name(&self.base, group, self.encoding) != self.group(group) {
                changes.push(format!(
                    "Group {group:02}\n  Original: {:?}\n  Draft: {:?}",
                    group_name(&self.base, group, self.encoding),
                    self.group(group)
                ));
            }
            for row in 0..100 {
                let old = if group < self.base.group_count() {
                    self.base.record(group, row)?
                } else {
                    None
                };
                let new = if group < self.draft.group_count() {
                    self.draft.record(group, row)?
                } else {
                    None
                };
                if old != new {
                    let describe = |h: Option<DecodedRecord>| {
                        h.map_or_else(
                            || "empty".into(),
                            |h| {
                                format!(
                                    "{} MHz {} {:?}",
                                    mhz_text(h.channel_field(ChannelField::FrequencyHz)),
                                    mode_name(h.mode()),
                                    self.text(h.name())
                                )
                            },
                        )
                    };
                    changes.push(format!(
                        "{group:02}/{row:02}\n  Original: {}\n  Draft: {}",
                        describe(row_holder(&self.base, group, row)?),
                        describe(self.row(group, row)?)
                    ));
                }
            }
        }
        changes.extend(crate::catalog::changes(self)?);
        Ok(changes)
    }
}

fn ensure_group(memory: &mut NormalMemory, group: usize) -> Result<()> {
    if group >= 100 {
        return Err("group must be 00..99".into());
    }
    while memory.group_count() <= group {
        if !memory.add_group() {
            return Err("group capacity exhausted".into());
        }
    }
    Ok(())
}

fn typed_index(group: usize, row: usize) -> Result<u32> {
    if group >= 100 || row >= 100 {
        return Err("group and row must be 00..99".into());
    }
    Ok((group * 200 + row * 2) as u32)
}

fn row_holder(memory: &NormalMemory, group: usize, row: usize) -> Result<Option<DecodedRecord>> {
    let index = typed_index(group, row)?;
    if group >= memory.group_count() || memory.slot(group, row)?.is_none() {
        return Ok(None);
    }
    let mut h = record::reset_holder(2, index, 0);
    record_image::load_channel_into(memory.working(), ChannelKind::Normal, index, &mut h)?;
    Ok(Some(h))
}

fn group_name(memory: &NormalMemory, group: usize, encoding: TextEncoding) -> String {
    // from_icf has already put names into logical ordinal order, including holes.
    encoding.display(&memory.working()[0x355c3 + 19 * group..0x355d3 + 19 * group])
}

#[cfg(test)]
pub fn text(bytes: &[u8]) -> String {
    bytes
        .trim_ascii_end()
        .iter()
        .flat_map(|b| std::ascii::escape_default(*b))
        .map(char::from)
        .collect()
}

pub fn mhz_text(hz: u32) -> String {
    String::from_utf8(cs705::csv::number::frequency_text(hz, b'.'))
        .expect("ASCII frequency formatter")
}

pub fn frequency(value: &str) -> Result<u32> {
    let value = value.trim();
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next();
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction
            .is_some_and(|p| p.len() > 6 || p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
        || parts.next().is_some()
    {
        return Err("MHz requires decimal digits and at most six decimal places".into());
    }
    let hz = cs705::csv::number::frequency_hz(value.as_bytes(), b'.')?;
    if !(30_000..=470_000_000).contains(&hz) {
        return Err("frequency outside 0.030..470 MHz".into());
    }
    Ok(hz)
}

pub fn mode_name(code: u32) -> String {
    for mode in [
        icf::MemoryMode::Lsb,
        icf::MemoryMode::Usb,
        icf::MemoryMode::Cw,
        icf::MemoryMode::CwReverse,
        icf::MemoryMode::Rtty,
        icf::MemoryMode::RttyReverse,
        icf::MemoryMode::Am,
        icf::MemoryMode::Fm,
        icf::MemoryMode::Wfm,
        icf::MemoryMode::Dv,
    ] {
        if record::decode_mode(mode as u8) == code {
            return format!("{mode:?}");
        }
    }
    format!("Code {code}")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn snapshot(profile: u8) -> Snapshot {
        let mut raw = vec![0; clone::IMAGE_CAPACITY];
        raw[0x355a1] = 6;
        raw[0x3c9b0..STORAGE].copy_from_slice(b"IcomCloneFormat3");
        raw[0x3fb40..0x3fb44].copy_from_slice(&2_u32.to_le_bytes());
        raw[0x3662d..0x36821].fill(1);
        for cell in raw[0x3fcd4..0x49914].as_chunks_mut::<4>().0 {
            cell.copy_from_slice(&0xffff_u32.to_le_bytes());
        }
        for group in 0..100 {
            raw[0x355c3 + 19 * group..0x355d3 + 19 * group].fill(b' ');
        }
        let mut memory = NormalMemory::from_working(raw).unwrap();
        memory.rename_group(0, Some(b"Home")).unwrap();
        memory.rename_group(1, Some(b"Travel")).unwrap();
        for group in 0..2 {
            memory.allocate(group, 0..1).unwrap();
            for side in 0..2 {
                let index = group as u32 * 200 + side;
                let mut h = record::reset_holder(2, index, 0);
                h.set_valid(true);
                h.set_mode(record::decode_mode(if side == 0 { 8 } else { 7 }));
                h.set_filter(if side == 0 { 255 } else { 0 });
                h.set_name(b"Source          ");
                h.set_channel_field(
                    ChannelField::FrequencyHz,
                    if side == 0 { 76_200_000 } else { 145_000_000 },
                );
                memory.store_channel(index, &mut h).unwrap();
            }
        }
        let image = memory.to_icf().unwrap();
        let mut full = image.memory().to_vec();
        full.resize(clone::IMAGE_CAPACITY, 0);
        full[0x3a318] = 0xa7; // Conversion-normalized setting outside our edit domain.
        let mut native = clone::convert_image(
            &full,
            6.try_into().unwrap(),
            profile.try_into().unwrap(),
            None,
        )
        .unwrap();
        native[0x3a318] = 0xa7;
        Snapshot {
            native,
            profile: profile.try_into().unwrap(),
            comment: b"Synthetic".to_vec(),
            extra: 0,
            identity: None,
            file_bytes: None,
        }
    }

    #[test]
    fn draft_batch_sparse_copy_discard_and_file_roundtrip_preserve_other_domains() {
        for profile in [5, 6] {
            let source = snapshot(profile);
            let original = source.native.clone();
            let mut editor = Editor::new(source).unwrap();
            editor.rename(99, "FM RADIO").unwrap();
            editor.copy((0, 0), (99, 0), "TOKYO FM", "80.0").unwrap();
            editor.edit(1, 0, "Travel new", "81.250001").unwrap();
            assert!(editor.dirty);
            assert_eq!(editor.original.native, original);
            assert_eq!(editor.draft.group_count(), 100);
            assert_eq!(editor.draft.used_slots(), 3);
            assert_eq!(
                editor
                    .row(99, 0)
                    .unwrap()
                    .unwrap()
                    .channel_field(ChannelField::FrequencyHz),
                80_000_000
            );
            let mut other = record::reset_holder(2, 19_801, 0);
            record_image::load_channel_into(
                editor.draft.working(),
                ChannelKind::Normal,
                19_801,
                &mut other,
            )
            .unwrap();
            assert_eq!(other.channel_field(ChannelField::FrequencyHz), 145_000_000);
            assert_eq!(other.mode(), record::decode_mode(7));
            assert_eq!(other.name(), b"TOKYO FM        ");
            let candidate = editor.candidate().unwrap();
            assert_eq!(candidate.native[0x355a1], profile);
            for (i, byte) in original.iter().enumerate() {
                if !NORMAL.iter().any(|r| r.contains(&i)) {
                    assert_eq!(candidate.native[i], *byte, "outside edit domain at {i:x}");
                }
            }
            let reopened = Snapshot::from_file(&candidate.icf().unwrap()).unwrap();
            assert!(reopened.same_memory(&candidate));
            assert_eq!(reopened.comment, b"Synthetic");
            assert_eq!(editor.changes().unwrap().len(), 3);
            editor.discard();
            assert!(!editor.dirty);
            assert_eq!(editor.candidate().unwrap().native, original);
        }
    }

    #[test]
    fn invalid_edits_and_occupied_copy_do_not_commit_a_prefix() {
        let mut editor = Editor::new(snapshot(5)).unwrap();
        let before = editor.draft.clone();
        for value in ["", "80MHz", "NaN", "-1", "80.0000001", "0", "471", "80..1"] {
            assert!(editor.edit(0, 0, "new", value).is_err(), "{value}");
            assert_eq!(editor.draft, before);
        }
        assert!(editor.copy((0, 0), (1, 0), "new", "80").is_err());
        assert!(editor.copy((0, 0), (99, 100), "new", "80").is_err());
        assert!(editor.rename(100, "new").is_err());
        assert!(editor.rename(99, "name too long for the radio").is_err());
        assert!(editor.edit(0, 0, "bad\nname", "80").is_err());
        assert_eq!(editor.draft, before);
        assert!(!editor.dirty);
        assert_eq!(frequency("0.030000").unwrap(), 30_000);
        assert_eq!(frequency("470.000000").unwrap(), 470_000_000);
    }

    #[test]
    fn settings_only_edits_do_not_rebuild_a_nonidentity_memory_directory() {
        let mut source = snapshot(6);
        let first = source.native[0x355c0..0x355d3].to_vec();
        source.native.copy_within(0x355d3..0x355e6, 0x355c0);
        source.native[0x355d3..0x355e6].copy_from_slice(&first);
        source.native[0x355c0] = 2;
        source.native[0x355d3] = 0;
        source.native[0x36628] = 1;
        let mut editor = Editor::new(source.clone()).unwrap();
        assert_eq!(editor.candidate().unwrap().native, source.native);
        editor
            .edit_settings(|image| {
                image.patch_memory(&[(0x3969d, &[42])])?;
                Ok(())
            })
            .unwrap();
        let candidate = editor.candidate().unwrap();
        for range in NORMAL {
            assert_eq!(candidate.native[range.clone()], source.native[range]);
        }
    }

    #[test]
    fn memory_name_and_frequency_edits_preserve_unedited_record_bytes() {
        let mut source = snapshot(6);
        // A valid occupied row with a noncanonical old offset. Re-encoding the
        // holder would clamp this unrelated field during a name-only edit.
        source.native[30..34].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut editor = Editor::new(source).unwrap();
        let before = editor.draft.record(0, 0).unwrap().unwrap().to_vec();
        editor.edit(0, 0, "Renamed", "76.200000").unwrap();
        let mut expected = before;
        expected[67..83].copy_from_slice(b"Renamed         ");
        assert_eq!(editor.draft.record(0, 0).unwrap().unwrap(), expected);
        editor
            .copy((0, 0), (1, 99), "Renamed", "76.200000")
            .unwrap();
        assert_eq!(editor.draft.record(1, 99).unwrap().unwrap(), expected);
    }

    #[test]
    fn rejected_settings_preserve_an_existing_memory_draft_and_dirty_state() {
        let mut editor = Editor::new(snapshot(5)).unwrap();
        editor.edit(0, 0, "Staged", "80").unwrap();
        let before = editor.candidate().unwrap().native;
        let settings = editor.settings.memory().to_vec();
        // Profile 6-only values and overlap with the separately owned normal
        // memory directory must fail before the replacement becomes visible.
        for (at, value) in [(0x399bb, 2), (0x399c0, 3), (0x355a1, 5), (0x355c3, b'X')] {
            assert!(
                editor
                    .edit_settings(|image| {
                        image.patch_memory(&[(at, &[value])])?;
                        Ok(())
                    })
                    .is_err()
            );
            assert_eq!(editor.settings.memory(), settings);
            assert_eq!(editor.candidate().unwrap().native, before);
            assert!(editor.dirty);
        }
    }

    #[test]
    fn strict_input_checks_and_display_escaping_are_not_silent_repairs() {
        let mut source = snapshot(5);
        source.native[0x355a1] = 6;
        assert!(source.memory().is_err());
        source.native.truncate(10);
        assert!(source.memory().is_err());
        let source = snapshot(6);
        let mut file = source.icf().unwrap();
        let last = file.iter().rposition(|b| b.is_ascii_hexdigit()).unwrap();
        file[last] = if file[last] == b'0' { b'1' } else { b'0' };
        assert!(Snapshot::from_file(&file).is_err());
        assert_eq!(text(b"a\x1b\x82 "), "a\\x1b\\x82");
        let original = [
            0x82, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32,
        ];
        let encoding = TextEncoding::default();
        assert_eq!(
            encoding
                .fixed(&encoding.display(&original), &original)
                .unwrap(),
            original
        );
    }

    #[test]
    fn tui_new_simplex_roundtrips_both_sides_without_a_factory_source() {
        for profile in [5, 6] {
            let source = snapshot(profile);
            let original = source.native.clone();
            let mut editor = Editor::new(source).unwrap();
            let existing = editor.draft.record(0, 0).unwrap().unwrap().to_vec();
            editor
                .create(99, 7, "\u{65b0}\u{898f}FM", "80", icf::MemoryMode::Wfm)
                .unwrap();
            assert!(editor.dirty);
            assert_eq!(editor.original.native, original);
            assert_eq!(editor.draft.group_count(), 100);
            assert_eq!(editor.draft.used_slots(), 3);
            assert_eq!(editor.draft.record(0, 0).unwrap().unwrap(), existing);
            let candidate = editor.candidate().unwrap();
            let reopened =
                Editor::new(Snapshot::from_file(&candidate.icf().unwrap()).unwrap()).unwrap();
            for side in 0..2 {
                let index = typed_index(99, 7).unwrap() + side;
                let mut row = record::reset_holder(2, index, 0);
                record_image::load_channel_into(
                    reopened.draft.working(),
                    ChannelKind::Normal,
                    index,
                    &mut row,
                )
                .unwrap();
                assert!(row.valid());
                assert_eq!(row.channel_field(ChannelField::FrequencyHz), 80_000_000);
                assert_eq!(row.mode(), record::decode_mode(8));
                assert_eq!(
                    (row.filter(), row.data(), row.split(), row.offset_hz()),
                    (15, 0, 0, 0)
                );
                assert_eq!(row.channel_field(ChannelField::Duplex), 0);
                assert_eq!(reopened.text(row.name()), "\u{65b0}\u{898f}FM");
            }
            for (index, &byte) in original.iter().enumerate() {
                if !NORMAL.iter().any(|range| range.contains(&index)) {
                    assert_eq!(candidate.native[index], byte);
                }
            }
        }
    }

    #[test]
    fn tui_new_simplex_rejects_occupied_invalid_and_full_without_partial_edits() {
        let mut editor = Editor::new(snapshot(5)).unwrap();
        let before = editor.draft.clone();
        for (group, row, name, mhz, mode) in [
            (0, 0, "New", "80", icf::MemoryMode::Fm),
            (99, 100, "New", "80", icf::MemoryMode::Fm),
            (99, 0, "New", "", icf::MemoryMode::Fm),
            (
                99,
                0,
                "Name exceeds sixteen bytes",
                "80",
                icf::MemoryMode::Fm,
            ),
            (99, 0, "New", "80", icf::MemoryMode::Dv),
        ] {
            assert!(editor.create(group, row, name, mhz, mode).is_err());
            assert_eq!(editor.draft, before);
            assert!(!editor.dirty);
        }
        while editor.draft.group_count() < 5 {
            assert!(editor.draft.add_group());
        }
        for group in 0..5 {
            editor.draft.allocate(group, 0..100).unwrap();
        }
        let full = editor.draft.clone();
        assert!(
            editor
                .create(99, 0, "Full", "145.5", icf::MemoryMode::Fm)
                .is_err()
        );
        assert_eq!(editor.draft, full);
    }
}
