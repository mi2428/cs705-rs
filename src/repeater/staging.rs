//! Decoded repeater import roots: 85364/85D88/862B8/878C4 and preservation.
//!
//! Holder identity is an arena index, separate from V/Q container positions.
//! This retains the whole-import outer-erase/inner-apply alias bug without raw
//! pointers or unsafe code. Header 12 is preserved on replacement; body 604 is
//! copied, then old group/USE/reference mark is restored in the original order.
//! Classification is re-evaluated from holder fields by C72D0's exact rules.
//! The supplied frequency class is F094 output; no band inference is inserted.

use crate::csv::{Error, control::Counts};

/// A decoded holder plus independently produced F094 frequency-class output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub bytes: Box<[u8; 616]>,
    pub frequency_class: u32,
}
impl Record {
    pub fn group(&self) -> u32 {
        self.dword(0x20)
    }
    pub(crate) fn dword(&self, at: usize) -> u32 {
        u32::from_le_bytes([
            self.bytes[at],
            self.bytes[at + 1],
            self.bytes[at + 2],
            self.bytes[at + 3],
        ])
    }
    pub(crate) fn put(&mut self, at: usize, n: u32) {
        self.bytes[at..at + 4].copy_from_slice(&n.to_le_bytes());
    }
    fn blank_call(&self) -> bool {
        self.bytes[0x160..0x168] == [b' '; 8]
    }
    fn repeatable(&self) -> bool {
        let k = classify(self);
        matches!(k, 1 | 3) || (k == 2 && self.blank_call())
    }
    fn cloned_with_header(&self, header: [u8; 12]) -> Self {
        let mut row = self.clone();
        row.bytes[..12].copy_from_slice(&header);
        row
    }
}
/// Exact C752C identity, not normalized text or a unique hash key.
pub fn same(a: &Record, b: &Record) -> bool {
    let k = classify(a);
    if k != classify(b) {
        return false;
    }
    let frequency = a.dword(0x3c) == b.dword(0x3c);
    let call = a.bytes[0x160..0x168] == b.bytes[0x160..0x168];
    match k {
        0 => call,
        1 => frequency && a.dword(0x3c) != 0,
        2 => {
            if a.blank_call() && b.blank_call() {
                frequency
            } else {
                call && a.frequency_class == b.frequency_class
            }
        }
        3 => frequency,
        _ => false,
    }
}

/// C72D0: full DWORD kind, strict binary 8 call and full DWORD DUP. Invalid and
/// unknown-kind holders classify 0; this is not a type/mode repair operation.
pub fn classify(row: &Record) -> u32 {
    classify_bytes(&row.bytes)
}
/// Same C72D0 classifier for borrowed decoded holders, without a row allocation.
pub fn classify_bytes(bytes: &[u8; 616]) -> u32 {
    if bytes[12] & 1 == 0 {
        return 0;
    }
    let dword = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("holder DWORD width"));
    match dword(0x15c) {
        2 => u32::from(bytes[0x160..0x168] == [b' '; 8]),
        0 | 1 => {
            if dword(0x44) != 0 {
                2
            } else {
                3
            }
        }
        3 => {
            if dword(0x44) == 3 {
                4
            } else {
                5
            }
        }
        _ => 0,
    }
}

#[derive(Debug, Clone)]
struct Pending {
    name: [u8; 16],
    id: usize,
}

/// Working V/Names/availability/Q state. It is not a committed ICF image.
pub struct Staging {
    arena: Vec<Record>,
    order: Vec<usize>,
    pending: Vec<Pending>,
    pub names: [[u8; 16]; 50],
    original_names: [[u8; 16]; 50],
    initial_empty: [bool; 50],
    available: [bool; 50],
    pub old_reference_existed: bool,
    pub counts: Counts,
}
impl Staging {
    /// 90340 valid-only preload. oldref existence is independent of whether that
    /// ordinal survives valid filtering. Invalid old rows are not re-created.
    pub fn preload(
        rows: impl IntoIterator<Item = Record>,
        names: [[u8; 16]; 50],
        old_reference: u16,
    ) -> Result<Self, Error> {
        let mut s = Self {
            arena: Vec::new(),
            order: Vec::new(),
            pending: Vec::new(),
            names,
            original_names: names,
            initial_empty: [true; 50],
            available: [true; 50],
            old_reference_existed: old_reference != u16::MAX,
            counts: Counts::default(),
        };
        for (index, mut row) in rows.into_iter().enumerate() {
            if index >= 2500 {
                return Err(Error::Limit("repeater preload exceeds 2500 rows"));
            }
            if row.bytes[12] & 1 == 0 {
                continue;
            }
            let g = usize::try_from(row.group())
                .ok()
                .filter(|g| *g < 50)
                .ok_or(Error::Invalid("repeater group outside 0..50"))?;
            row.bytes[0x134] = 0;
            row.bytes[0x135] = u8::from(index == usize::from(old_reference));
            s.initial_empty[g] = false;
            let id = s.allocate(row);
            s.order.push(id);
        }
        s.available = s.initial_empty;
        Ok(s)
    }
    fn allocate(&mut self, row: Record) -> usize {
        let id = self.arena.len();
        self.arena.push(row);
        id
    }
    pub fn rows(&self) -> impl Iterator<Item = &Record> {
        self.order.iter().map(|id| &self.arena[*id])
    }
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
    pub fn first_reference_ordinal(&self) -> u16 {
        self.order
            .iter()
            .position(|id| self.arena[*id].bytes[0x135] != 0)
            .map_or(u16::MAX, |i| i as u16)
    }

    fn replace(&mut self, id: usize, input: &Record) {
        let group = self.arena[id].dword(0x20);
        let use_value = self.arena[id].dword(0x17c);
        let mark = self.arena[id].bytes[0x135];
        self.arena[id].bytes[12..].copy_from_slice(&input.bytes[12..]);
        self.arena[id].frequency_class = input.frequency_class;
        self.arena[id].put(0x20, group);
        self.arena[id].put(0x17c, use_value);
        self.arena[id].bytes[0x135] = mark;
    }

    /// Selected code 0/1/2 differs from whole code 0/1/2/3. Input stage is set even
    /// on rejection. New holder header is the caller's live binding, not H.header.
    pub fn selected_row(
        &mut self,
        input: &mut Record,
        name: [u8; 16],
        group: usize,
        clone_header: [u8; 12],
        delta: u32,
    ) -> Result<u32, Error> {
        if group >= 50 {
            return Err(Error::Invalid("selected repeater group outside 0..50"));
        }
        input.bytes[0x134] = 1;
        let mut append = true;
        for position in 0..self.order.len() {
            let id = self.order[position];
            if !same(&self.arena[id], input) {
                continue;
            }
            if self.arena[id].group() == group as u32 {
                if self.arena[id].bytes[0x134] == 0 {
                    self.replace(id, input);
                    self.arena[id].put(0x20, group as u32);
                    if self.names[group] == [b' '; 16] && name != [b' '; 16] {
                        self.names[group] = name;
                    }
                    self.counts.selected_event(0, delta);
                    return Ok(0);
                }
                if !input.repeatable() {
                    append = false;
                    break;
                }
            } else if !input.repeatable() {
                append = false;
            }
        }
        let code = if !append {
            1
        } else if self.order.len() >= 2500 {
            2
        } else {
            let mut row = input.cloned_with_header(clone_header);
            row.put(0x20, group as u32);
            let id = self.allocate(row);
            self.order.push(id);
            if self.names[group] == [b' '; 16] && name != [b' '; 16] {
                self.names[group] = name;
            }
            0
        };
        self.counts.selected_event(code, delta);
        Ok(code)
    }

    // 85D88 returns 0 replaced/1 unmatched/2 stage-blocked; no group match gate.
    fn whole_match(&mut self, input: &mut Record, name: [u8; 16]) -> u32 {
        input.bytes[0x134] = 1;
        for position in 0..self.order.len() {
            let id = self.order[position];
            if !same(&self.arena[id], input) {
                continue;
            }
            if self.arena[id].bytes[0x134] == 0 {
                let g = self.arena[id].group() as usize;
                if g < 50 && input.group() == g as u32 && self.names[g] == [b' '; 16] {
                    self.names[g] = name;
                }
                self.replace(id, input);
                return 0;
            }
            if !input.repeatable() {
                return 2;
            }
        }
        1
    }
    fn sorted_insert(&mut self, id: usize, group: usize) {
        self.arena[id].put(0x20, group as u32);
        let position = self
            .order
            .iter()
            .rposition(|id| self.arena[*id].group() <= group as u32)
            .map_or(0, |p| p + 1);
        self.order.insert(position, id);
    }
    fn apply_pending(&mut self, id: usize, group: usize, name: [u8; 16]) -> u32 {
        let mut row = self.arena[id].clone();
        match self.whole_match(&mut row, name) {
            0 => 0,
            2 => 1,
            _ => {
                if self.order.len() >= 2500 {
                    3
                } else {
                    self.arena[id] = row;
                    self.sorted_insert(id, group);
                    self.available[group] = false;
                    0
                }
            }
        }
    }
    /// Whole reader phase; pending queue has a separate safe bound of 100000, not
    /// a fabricated contribution to the original V.size<2500 capacity test.
    pub fn whole_row(
        &mut self,
        input: &mut Record,
        name: [u8; 16],
        clone_header: [u8; 12],
        delta: u32,
    ) -> Result<(), Error> {
        let source_group = usize::try_from(input.group())
            .ok()
            .filter(|g| *g < 50)
            .ok_or(Error::Invalid(
                "whole CSV group outside normal 0..50 domain",
            ))?;
        let code = self.whole_match(input, name);
        if code == 0 {
            self.counts.whole_event(0, delta);
            return Ok(());
        }
        if code == 2 {
            self.counts.whole_event(1, delta);
            return Ok(());
        }
        if self.order.len() >= 2500 {
            self.counts.whole_event(3, delta);
            return Ok(());
        }
        let mut target = if name == self.original_names[source_group] {
            Some(source_group)
        } else if name != [b' '; 16] {
            self.names.iter().position(|n| *n == name)
        } else {
            None
        };
        let mut last_drain_code = None;
        if target.is_none()
            && self.original_names[source_group] == [b' '; 16]
            && self.initial_empty[source_group]
        {
            target = Some(source_group);
            if self.names[source_group] == [b' '; 16] && name != [b' '; 16] {
                self.names[source_group] = name;
                let mut at = 0;
                while at < self.pending.len() {
                    if self.pending[at].name == name {
                        let id = self.pending[at].id;
                        let code = self.apply_pending(id, source_group, name);
                        self.counts.whole_event(code, delta);
                        last_drain_code = Some(code);
                        self.pending.remove(at);
                    } else {
                        at += 1;
                    }
                }
            }
        }
        if let Some(g) = target {
            let id = self.allocate(input.cloned_with_header(clone_header));
            self.sorted_insert(id, g);
            self.available[g] = false;
            self.counts.whole_event(last_drain_code.unwrap_or(0), delta);
        } else {
            if self.pending.len() >= 100000 {
                return Err(Error::Limit(
                    "whole import pending queue exceeds safe bound",
                ));
            }
            let id = self.allocate(input.cloned_with_header(clone_header));
            self.pending.push(Pending { name, id });
            self.counts.whole_event(0, delta);
        }
        Ok(())
    }

    /// EOF named/blank/fallback passes. Inner applies erase the OUTER cursor.
    /// Thus an A/B/A queue can apply last A, erase B, and re-evaluate same A.
    pub fn finish_whole(&mut self) {
        for named in [true, false] {
            let mut outer = 0;
            while outer < self.pending.len() {
                let item = self.pending[outer].clone();
                if (item.name != [b' '; 16]) != named {
                    outer += 1;
                    continue;
                }
                let Some(g) = (0..50).find(|g| self.names[*g] == [b' '; 16] && self.available[*g])
                else {
                    outer += 1;
                    continue;
                };
                let original_group = self.arena[item.id].group();
                let code = self.apply_pending(item.id, g, item.name);
                self.counts.whole_event(code, 0);
                self.pending.remove(outer);
                if code == 0 {
                    if named {
                        self.names[g] = item.name;
                    }
                    let mut inner = outer;
                    while inner < self.pending.len() {
                        let p = self.pending[inner].clone();
                        let matches = if named {
                            p.name == item.name
                        } else {
                            p.name == [b' '; 16] && self.arena[p.id].group() == original_group
                        };
                        if matches {
                            let code = self.apply_pending(p.id, g, p.name);
                            self.counts.whole_event(code, 0);
                            self.pending.remove(outer);
                            inner = outer;
                        } else {
                            inner += 1;
                        }
                    }
                }
            }
        }
        while !self.pending.is_empty() {
            let p = self.pending.remove(0);
            let code = self.apply_pending(p.id, 49, [b' '; 16]);
            self.counts.whole_event(code, 0);
        }
    }

    /// Execute selected's typed delete/insert/name/ref phases. Callback errors
    /// return immediately: already performed raw/count/ref/name changes remain.
    /// The callback must call core's typed codec, never memmove 52.
    pub fn flush_selected<E: From<Error>>(
        &self,
        group: usize,
        old_start: usize,
        old_count: usize,
        mut apply: impl FnMut(Commit<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        if group >= 50
            || old_start > 2500
            || old_start
                .checked_add(old_count)
                .is_none_or(|end| end > 2500)
        {
            return Err(Error::Invalid("selected flush bounds").into());
        }
        for _ in 0..old_count {
            apply(Commit::Delete {
                absolute: old_start,
            })?;
        }
        let mut position = old_start;
        let mut restore = false;
        for id in &self.order {
            let row = &self.arena[*id];
            if row.group() == group as u32 {
                if position >= 2500 {
                    return Err(Error::Invalid("selected flush insertion outside 0..2500").into());
                }
                apply(Commit::Insert {
                    absolute: Some(position),
                    group,
                    record: row,
                })?;
                position = position
                    .checked_add(1)
                    .ok_or_else(|| E::from(Error::Invalid("selected flush insertion overflow")))?;
                restore |= row.bytes[0x135] != 0;
            }
        }
        self.flush_names(&mut apply)?;
        if restore {
            apply(Commit::Reference(self.first_reference_ordinal()))?;
        }
        Ok(())
    }
    pub fn flush_whole<E: From<Error>>(
        &self,
        old_total: usize,
        mut apply: impl FnMut(Commit<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        if old_total > 2500 {
            return Err(Error::Invalid("whole flush old total").into());
        }
        for _ in 0..old_total {
            apply(Commit::Delete { absolute: 0 })?;
        }
        for id in &self.order {
            let row = &self.arena[*id];
            apply(Commit::Insert {
                absolute: None,
                group: row.group() as usize,
                record: row,
            })?;
        }
        self.flush_names(&mut apply)?;
        if self.old_reference_existed {
            apply(Commit::Reference(self.first_reference_ordinal()))?;
        }
        for group in 0..50 {
            apply(Commit::GroupNotification { group })?;
        }
        Ok(())
    }
    fn flush_names<E>(&self, apply: &mut impl FnMut(Commit<'_>) -> Result<(), E>) -> Result<(), E> {
        for group in 0..50 {
            for byte in 0..16 {
                apply(Commit::NameByte {
                    group,
                    byte,
                    value: self.names[group][byte],
                })?;
            }
        }
        Ok(())
    }
}

/// Original commit work; None insertion index means re-read the live group end.
pub enum Commit<'a> {
    Delete {
        absolute: usize,
    },
    Insert {
        absolute: Option<usize>,
        group: usize,
        record: &'a Record,
    },
    NameByte {
        group: usize,
        byte: usize,
        value: u8,
    },
    Reference(u16),
    GroupNotification {
        group: usize,
    },
}

/// Explicit original P+4/P+8/P+C source, including unwritten heap words. The
/// `raw` token is passed untouched to the reader; it is not a Rust pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreserveSource {
    pub raw: u32,
    pub first: u32,
    pub last: u32,
    pub unwritten: bool,
}

/// C5F3C restores USE from first matching old row without consuming it. An
/// unreadable explicit source returns Err AFTER preceding new-row imports and
/// previous USE saves. Empty ranges never call the source reader. A finite
/// budget is a named safe-API difference, not an original range clamp.
pub fn restore_use(
    rows: &mut [Record],
    source: PreserveSource,
    read_budget: usize,
    mut read: impl FnMut(u32, u32) -> Result<Record, Error>,
    mut save: impl FnMut(usize, &Record) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut reads = 0;
    for (index, row) in rows.iter_mut().enumerate() {
        if source.first > source.last {
            continue;
        }
        let mut at = source.first;
        loop {
            if reads >= read_budget {
                return Err(Error::Limit("explicit preserve source read budget"));
            }
            reads += 1;
            let old = read(source.raw, at)?;
            if same(&old, row) {
                row.put(0x17c, old.dword(0x17c));
                save(index, row)?;
                break;
            }
            if at == source.last {
                break;
            }
            at = at.wrapping_add(1);
        }
    }
    Ok(())
}
/// Both preserve modes restore the first new matching old reference. Caller
/// supplies the actual old-reference holder even for out-of-old range refs.
pub fn restored_reference(rows: &[Record], new_absolute_start: u32, old_reference: &Record) -> u16 {
    rows.iter()
        .position(|row| same(row, old_reference))
        .map_or(u16::MAX, |i| {
            new_absolute_start.wrapping_add(i as u32) as u16
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(group: u32, key: u8, k: u32, use_value: u32) -> Record {
        let mut r = Record {
            bytes: Box::new([0; 616]),
            frequency_class: 0,
        };
        r.bytes[12] = 1;
        r.put(0x20, group);
        r.put(0x3c, u32::from(key));
        r.put(0x17c, use_value);
        r.bytes[0x160..0x168].fill(b' ');
        r.bytes[0x160] = key;
        match k {
            0 => r.put(0x15c, 2),
            1 => {
                r.put(0x15c, 2);
                r.bytes[0x160..0x168].fill(b' ');
            }
            2 => {
                r.put(0x15c, 0);
                r.put(0x44, 1);
            }
            3 => r.put(0x15c, 0),
            4 => {
                r.put(0x15c, 3);
                r.put(0x44, 3);
            }
            5 => r.put(0x15c, 3),
            _ => r.put(0x15c, u32::MAX),
        }
        r
    }
    #[test]
    fn selected_preserves_old_header_use_ref_and_new_rows_append() {
        let mut old = row(2, b'A', 0, 7);
        old.bytes[..12].fill(0xa1);
        let mut s = Staging::preload([old, row(3, b'B', 0, 8)], [[b' '; 16]; 50], 0).unwrap();
        let mut incoming = row(4, b'A', 0, 0);
        incoming.bytes[..12].fill(0xb2);
        assert_eq!(
            s.selected_row(&mut incoming, [b'N'; 16], 2, [0xc3; 12], 2)
                .unwrap(),
            0
        );
        let output = s.rows().next().unwrap();
        assert_eq!(&output.bytes[..12], &[0xa1; 12]);
        assert_eq!(output.group(), 2);
        assert_eq!(output.dword(0x17c), 7);
        assert_eq!(output.bytes[0x135], 1);
        assert_eq!(
            s.selected_row(&mut incoming, [b'N'; 16], 2, [0xc3; 12], 9)
                .unwrap(),
            1
        );
        assert_eq!(incoming.bytes[0x134], 1);
        assert_eq!(
            s.counts,
            Counts {
                s: 0,
                c2: 1,
                c3: 2,
                c4: 1
            }
        );
        let mut actions = Vec::new();
        let failure = s.flush_selected(2, 10, 2, |a| {
            match a {
                Commit::Delete { absolute } => actions.push(absolute),
                Commit::Insert { .. } => return Err(Error::Operation("injected typed failure")),
                _ => panic!(),
            }
            Ok(())
        });
        assert!(failure.is_err());
        assert_eq!(actions, [10, 10]);
    }
    #[test]
    fn selected_flush_bounds_empty_endpoint_and_late_failures_do_not_unwind() {
        let s = Staging::preload(
            [row(2, b'A', 0, 0), row(2, b'B', 0, 0)],
            [[b' '; 16]; 50],
            u16::MAX,
        )
        .unwrap();
        for (start, count) in [(usize::MAX, 0), (1, usize::MAX), (2501, 0), (2499, 2)] {
            assert!(
                s.flush_selected::<Error>(2, start, count, |_| panic!(
                    "invalid old range reached callback"
                ))
                .is_err()
            );
        }
        let mut actions = Vec::new();
        assert!(
            s.flush_selected(2, 2499, 1, |event| {
                match event {
                    Commit::Delete { absolute } => actions.push((b'D', absolute)),
                    Commit::Insert {
                        absolute: Some(absolute),
                        ..
                    } => actions.push((b'I', absolute)),
                    _ => panic!("late bound failure must precede names"),
                }
                Ok::<_, Error>(())
            })
            .is_err()
        );
        assert_eq!(actions, [(b'D', 2499), (b'I', 2499)]);
        let mut inserts = 0;
        assert!(
            s.flush_selected(2, 2498, 0, |event| {
                if let Commit::Insert { .. } = event {
                    inserts += 1;
                    if inserts == 2 {
                        return Err(Error::Operation("injected second insertion failure"));
                    }
                }
                Ok(())
            })
            .is_err()
        );
        assert_eq!(inserts, 2);
        assert!(
            s.flush_selected::<Error>(2, 2500, 0, |_| panic!("full endpoint must not insert"))
                .is_err()
        );
        let empty = Staging::preload([], [[b' '; 16]; 50], u16::MAX).unwrap();
        let mut names = 0;
        empty
            .flush_selected(2, 2500, 0, |event| {
                assert!(matches!(event, Commit::NameByte { .. }));
                names += 1;
                Ok::<_, Error>(())
            })
            .unwrap();
        assert_eq!(names, 800);
    }
    #[test]
    fn whole_outer_erase_bug_keeps_alias_and_counts_events_not_rows() {
        let names = std::array::from_fn(|g| {
            if g == 49 {
                [b' '; 16]
            } else {
                [b'O' + (g % 5) as u8; 16]
            }
        });
        let rows = (0..49).map(|g| row(g, 128 + g as u8, 0, 1));
        let mut s = Staging::preload(rows, names, u16::MAX).unwrap();
        for (key, name) in [(1, b'A'), (2, b'B'), (3, b'A')] {
            let mut input = row(0, key, 0, 0);
            s.whole_row(&mut input, [name; 16], [0; 12], 0).unwrap();
        }
        assert_eq!(s.pending_count(), 3);
        s.finish_whole();
        assert_eq!(s.pending_count(), 0);
        assert_eq!(s.counts.c4, 5);
        assert_eq!(s.counts.c2, 1);
        let keys: Vec<_> = s
            .rows()
            .filter(|r| r.group() == 49)
            .map(|r| r.dword(0x3c))
            .collect();
        assert_eq!(keys, [1, 3]);
    }
    #[test]
    fn unwritten_source_is_explicit_and_first_old_match_is_reused() {
        let mut rows = [row(0, b'A', 0, 0), row(0, b'A', 0, 0)];
        let mut saves = Vec::new();
        let source = PreserveSource {
            raw: 0x1234,
            first: 2,
            last: 2,
            unwritten: true,
        };
        restore_use(
            &mut rows,
            source,
            2,
            |raw, index| {
                assert_eq!((raw, index), (0x1234, 2));
                Ok(row(0, b'A', 0, 9))
            },
            |i, r| {
                saves.push((i, r.dword(0x17c)));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(saves, [(0, 9), (1, 9)]);
        assert!(
            restore_use(
                &mut rows,
                source,
                2,
                |_, _| Err(Error::SourceUnavailable("unwritten source unreadable")),
                |_, _| panic!()
            )
            .is_err()
        );
        let empty = PreserveSource {
            first: 3,
            last: 2,
            ..source
        };
        restore_use(&mut rows, empty, 0, |_, _| panic!(), |_, _| panic!()).unwrap();
        assert_eq!(restored_reference(&rows, 20, &row(9, b'A', 0, 1)), 20);
    }
}
