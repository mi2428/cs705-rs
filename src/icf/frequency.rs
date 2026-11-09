//! Native generated frequency ranges, classifiers and record predicates.
//!
//! A generated B owner and live Q are different sources. Querying does not
//! regenerate B, and missing inputs are not empty or universally allowed lists.
//! C9360/61CE0 and all nine containers share this engine. Population follows
//! the accepted DC78 recipe, not a copied factory image.

use super::context::Context;
use super::{Error, record::DecodedRecord};

/// Generated range payload. Bounds and flag are independent native values;
/// reversed bounds and nonzero flags are not silently repaired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrequencyRange {
    pub low: u32,
    pub high: u32,
    pub flag: u8,
}

const INVALID: FrequencyRange = FrequencyRange {
    low: u32::MAX,
    high: u32::MAX,
    flag: 1,
};
const MAX_COUNT: usize = 0x1555_5555;
fn range(low: u32, high: u32) -> FrequencyRange {
    FrequencyRange { low, high, flag: 0 }
}

/// Native B container offsets. Enumeration order is DC78's population order,
/// deliberately not numeric offset order (L80 is populated last).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Container {
    L8,
    L30,
    L58,
    LA8,
    LD0,
    LF8,
    L120,
    L148,
    L80,
}
pub const POPULATION_ORDER: [Container; 9] = [
    Container::L8,
    Container::L30,
    Container::L58,
    Container::LA8,
    Container::LD0,
    Container::LF8,
    Container::L120,
    Container::L148,
    Container::L80,
];

/// C308/E724's ten unsigned relationships, including wrap adjacency. Flag bytes
/// do not participate. Input R is compared against the existing node E.
pub fn relationship(r: FrequencyRange, e: FrequencyRange) -> u8 {
    let (l, h, a, b) = (r.low, r.high, e.low, e.high);
    if l == a && h == b {
        1
    } else if l <= a && h >= b {
        2
    } else if a < l && h < b {
        3
    } else if l == a && h < b {
        4
    } else if a < l && h == b {
        5
    } else if a < l && l <= b && b < h {
        6
    } else if l < a && a <= h && h < b {
        7
    } else if l.wrapping_sub(1) == b {
        8
    } else if h.wrapping_add(1) == a {
        9
    } else {
        0
    }
}

fn capacity(list: &mut Vec<FrequencyRange>, extra: usize) -> Result<(), Error> {
    if list.len().checked_add(extra).is_none_or(|n| n > MAX_COUNT) {
        return Err(invalid("native range count limit exceeded"));
    }
    list.try_reserve(extra)
        .map_err(|_| invalid("native range allocation failed"))
}

/// 1F5E8's corrected comparator. It is NOT a strict weak ordering on arbitrary
/// overlapping input. Do not substitute sort_by_key or a total-order adapter.
fn b_before_a(a: FrequencyRange, b: FrequencyRange) -> bool {
    a.low > b.low && (a.low > b.high || a.high >= b.high)
}
fn merge(a: Vec<FrequencyRange>, b: Vec<FrequencyRange>) -> Vec<FrequencyRange> {
    let (mut a, mut b) = (a.into_iter().peekable(), b.into_iter().peekable());
    let mut out = Vec::with_capacity(a.len() + b.len());
    while let (Some(&x), Some(&y)) = (a.peek(), b.peek()) {
        out.push(if b_before_a(x, y) {
            b.next().expect("peeked B")
        } else {
            a.next().expect("peeked A")
        });
    }
    out.extend(a);
    out.extend(b);
    out
}
/// C9E0's carry/bin merge, preserving payload order/flags and the 25-bin cap.
/// Heap node/sentinel addresses are intentionally not modeled.
pub fn sort_native(list: &mut Vec<FrequencyRange>) {
    if list.len() < 2 {
        return;
    }
    let mut bins: [Vec<FrequencyRange>; 26] = std::array::from_fn(|_| Vec::new());
    let mut fill = 0;
    for item in std::mem::take(list) {
        let mut carry = vec![item];
        let mut i = 0;
        while i < fill && !bins[i].is_empty() {
            carry = merge(std::mem::take(&mut bins[i]), carry);
            i += 1;
        }
        if i == 25 {
            bins[24] = merge(std::mem::take(&mut bins[24]), carry);
        } else {
            bins[i] = carry;
            if i == fill {
                fill += 1;
            }
        }
    }
    for i in 1..fill {
        bins[i] = merge(
            std::mem::take(&mut bins[i]),
            std::mem::take(&mut bins[i - 1]),
        );
    }
    *list = std::mem::take(&mut bins[fill - 1]);
}

/// C308: one forward walk. Sticky append suppression and raw-overlap loss are
/// native behavior; adjacent intervals are NOT merged here. Return is appended,
/// not a changed/count delta flag. Input flag wins only when appended.
pub fn add(list: &mut Vec<FrequencyRange>, mut pending: FrequencyRange) -> Result<bool, Error> {
    let mut append = true;
    let mut i = 0;
    while i < list.len() {
        match relationship(pending, list[i]) {
            1 | 2 => {
                list.remove(i);
            }
            6 => {
                pending.low = list.remove(i).low;
            }
            7 => {
                pending.high = list.remove(i).high;
            }
            3..=5 => {
                append = false;
                i += 1;
            }
            _ => i += 1,
        }
    }
    if append {
        capacity(list, 1)?;
        list.push(pending);
    }
    sort_native(list);
    Ok(append)
}

/// E724: split appends both flag0 fragments BEFORE deleting the original node;
/// appended fragments remain in the same walk. Result is the last erase-step,
/// not whether any mutation occurred. Original flags survive trim/retention.
pub fn cut(list: &mut Vec<FrequencyRange>, input: FrequencyRange) -> Result<bool, Error> {
    let mut i = 0;
    let mut erased = false;
    while i < list.len() {
        erased = false;
        let old = list[i];
        match relationship(input, old) {
            1 | 2 => {
                list.remove(i);
                erased = true;
            }
            3 => {
                capacity(list, 2)?;
                list.push(range(old.low, input.low.wrapping_sub(1)));
                list.push(range(input.high.wrapping_add(1), old.high));
                list.remove(i);
                erased = true;
            }
            4 | 7 => list[i].low = input.high.wrapping_add(1),
            5 | 6 => list[i].high = input.low.wrapping_sub(1),
            _ => {}
        }
        if !erased {
            i += 1;
        }
    }
    sort_native(list);
    Ok(erased)
}

/// E738: first argument is R in the ten-way classifier; preserve the selected
/// operand's flag for containment, use flag0 only for partial intersections.
pub fn intersection(a: FrequencyRange, b: FrequencyRange) -> FrequencyRange {
    match relationship(a, b) {
        1 | 2 => b,
        3..=5 => a,
        6 => range(a.low, b.high),
        7 => range(b.low, a.high),
        _ => INVALID,
    }
}

/// 1EC40 is a separate, unsorted all-later-node adjacency pass. It can produce
/// [MAX,0] from [0,0],[MAX,MAX] due to the native wrap comparison.
pub fn join_adjacent(list: &mut Vec<FrequencyRange>) {
    let mut i = 0;
    while i < list.len() {
        let mut j = i + 1;
        while j < list.len() {
            match relationship(list[i], list[j]) {
                8 => {
                    list[i].low = list.remove(j).low;
                }
                9 => {
                    list[i].high = list.remove(j).high;
                }
                _ => j += 1,
            }
        }
        i += 1;
    }
}
fn batch(
    list: &mut Vec<FrequencyRange>,
    input: impl IntoIterator<Item = FrequencyRange>,
) -> Result<(), Error> {
    for r in input {
        add(list, r)?;
    }
    sort_native(list);
    Ok(())
}
/// E6B0 optional-source protocol. Missing is an error; a known MAX marker skips
/// insertion but still sorts. Its source has only two DWORDs: E6B4 constructs
/// flag0, rather than reading a third supplied field. Generic add is different.
pub fn optional(
    list: &mut Vec<FrequencyRange>,
    input: Option<FrequencyRange>,
) -> Result<(), Error> {
    let r = input.ok_or(invalid("optional native range source unavailable"))?;
    if r.low != u32::MAX && r.high != u32::MAX {
        add(list, range(r.low, r.high))?;
    }
    sort_native(list);
    Ok(())
}

/// One B owner, independently retained across later live-Q changes. The initial
/// constructor's selected5/empty9 state is explicit native DB98/D988 behavior,
/// not a successful population or a missing-source fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ranges {
    selected: u32,
    lists: [Vec<FrequencyRange>; 9],
    source_q: Option<[u8; 24]>,
    complete: bool,
}
impl Ranges {
    pub fn unpopulated() -> Self {
        Self {
            selected: 5,
            lists: std::array::from_fn(|_| Vec::new()),
            source_q: None,
            complete: false,
        }
    }
    /// Normal 24A20 caller: capture source_selector once, then DC78 explicitly.
    pub fn generate(q: &Context, system_langid: Option<u16>) -> Result<Self, Error> {
        let selected = u32::from(q.source_selector(system_langid)?);
        let mut out = Self::unpopulated();
        out.regenerate(selected, q, system_langid)?;
        Ok(out)
    }
    pub fn selected(&self) -> u32 {
        self.selected
    }
    pub fn source_q(&self) -> Option<&[u8; 24]> {
        self.source_q.as_ref()
    }
    /// Resolve the captured B selector, never a later live Q. An unpopulated
    /// Rust owner has no captured source and cannot imply the native source0.
    pub fn default_source_selector(&self, system_langid: Option<u16>) -> Result<usize, Error> {
        if self.source_q.is_none() {
            return Err(invalid("B source selector has not been captured"));
        }
        source_selector(self.selected, system_langid)
    }
    pub fn is_complete(&self) -> bool {
        self.complete
    }
    pub fn list(&self, which: Container) -> &[FrequencyRange] {
        &self.lists[which as usize]
    }
    /// DB98 never tests Q or refreshes B. Its frequency argument is unused.
    pub fn db98(&self, _frequency: u32) -> &Self {
        self
    }
    pub fn regenerate(
        &mut self,
        selected: u32,
        q: &Context,
        system_langid: Option<u16>,
    ) -> Result<(), Error> {
        self.regenerate_with(selected, q, system_langid, |_| Ok(()))
    }
    /// DC78: selected store and LANGID mapping precede list population. Each
    /// callback precedes that container's clear. Failure retains all prior list
    /// updates and later old lists; complete stays false. This hook does not
    /// emulate allocator addresses or pretend failed heap operations succeeded.
    pub fn regenerate_with(
        &mut self,
        selected: u32,
        q: &Context,
        system_langid: Option<u16>,
        mut before_clear: impl FnMut(Container) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.selected = selected;
        self.complete = false;
        self.source_q = Some(*q.as_bytes());
        let s = source_selector(selected, system_langid)?;
        self.selected = s as u32;
        let raw = q.as_bytes();
        let c = u32::from_le_bytes(raw[8..12].try_into().expect("Q C"));
        for which in POPULATION_ORDER {
            before_clear(which)?;
            self.lists[which as usize].clear();
            match which {
                Container::L8 => batch(&mut self.lists[which as usize], class_bands())?,
                Container::L30 => {
                    for r in common_bands() {
                        add(&mut self.lists[which as usize], r)?;
                    }
                    optional(
                        &mut self.lists[which as usize],
                        Some(range(400_000_000, 470_000_000)),
                    )?;
                }
                Container::L58 => {
                    let list = &mut self.lists[which as usize];
                    batch(list, profile_receive(s))?;
                    if raw[4] == 0 {
                        batch(list, common_bands())?;
                    }
                    if raw[5] == 0 {
                        optional(list, Some(range(400_000_000, 470_000_000)))?;
                    }
                    if q.effective_f()? == 0 {
                        cut(list, range(30_000_000, 74_799_999))?;
                    }
                }
                Container::LA8 => {
                    let list = &mut self.lists[which as usize];
                    batch(list, profile_transmit(s))?;
                    match c {
                        0 => {
                            batch(
                                list,
                                [range(100_000, 29_999_999), range(30_000_000, 74_799_999)],
                            )?;
                            optional(list, Some(range(74_800_000, 199_999_999)))?;
                            optional(list, Some(range(400_000_000, 470_000_000)))?;
                        }
                        1 => {
                            batch(
                                list,
                                [
                                    range(1_600_000, 29_999_999),
                                    range(30_000_000, 54_000_000),
                                    range(68_000_000, 74_799_999),
                                ],
                            )?;
                            optional(list, Some(range(118_000_000, 174_000_000)))?;
                            optional(list, Some(range(400_000_000, 470_000_000)))?;
                        }
                        2 => {
                            batch(
                                list,
                                [
                                    range(1_600_000, 25_999_999),
                                    range(28_000_000, 29_999_999),
                                    range(50_000_000, 54_000_000),
                                ],
                            )?;
                            optional(list, Some(range(144_000_000, 146_000_000)))?;
                            optional(list, Some(range(430_000_000, 440_000_000)))?;
                        }
                        3 => {
                            if raw[12] == 0 {
                                add(list, range(5_255_000, 5_405_000))?;
                            } else if s != 1 {
                                cut(list, range(5_255_000, 5_405_000))?;
                            }
                        }
                        _ => {}
                    }
                    if raw[13] == 0 {
                        optional(list, Some(range(7_000_000, 7_300_000)))?;
                    }
                    if q.effective_u()? == 0 {
                        cut(list, range(68_000_000, 74_799_999))?;
                    }
                    if q.effective_f()? == 0 || (q.selector()? == 4 && c == 3) {
                        cut(list, range(30_000_000, 74_799_999))?;
                    }
                    if q.selector()? == 4 && c == 3 {
                        cut(list, range(430_000_000, 440_000_000))?;
                    }
                    join_adjacent(list);
                }
                Container::LD0 => {
                    let a = self.list(Container::LA8).to_vec();
                    let b = self.list(Container::L58).to_vec();
                    intersect_into(&mut self.lists[which as usize], &a, &b)?;
                    join_adjacent(&mut self.lists[which as usize]);
                }
                Container::LF8 => {
                    if s == 0 {
                        let a = japanese_transmit();
                        let b = self.list(Container::LD0).to_vec();
                        intersect_into(&mut self.lists[which as usize], &a, &b)?;
                    }
                }
                Container::L120 => batch(&mut self.lists[which as usize], auxiliary_bands())?,
                Container::L148 => {
                    let mut remainder = Vec::new();
                    add(&mut remainder, range(30_000, 470_000_000))?;
                    for &r in self.list(Container::L120) {
                        cut(&mut remainder, r)?;
                    }
                    let b = self.list(Container::L58).to_vec();
                    intersect_into(&mut self.lists[which as usize], &remainder, &b)?;
                }
                Container::L80 => batch(
                    &mut self.lists[which as usize],
                    [
                        range(30_000, 99_999_999),
                        range(100_000_000, 199_999_999),
                        range(400_000_000, 499_999_999),
                        range(1_200_000_000, 1_399_999_999),
                    ],
                )?,
            }
        }
        self.complete = true;
        Ok(())
    }
    pub fn frequency_context(&self, live_kind: i32) -> FrequencyContext<'_> {
        FrequencyContext {
            common_kind: live_kind,
            l30: Some(self.list(Container::L30)),
            l58: Some(self.list(Container::L58)),
            l120: Some(self.list(Container::L120)),
        }
    }
    pub fn eb64(&self, live_q: &Context, frequency: u32) -> bool {
        common_bad(Some(&self.frequency_context(q_kind(live_q))), frequency)
            .expect("bound native B lists")
    }
    /// EB98 tests L30 membership even when EB64 bypasses on live kind>=16.
    pub fn eb98(&self, frequency: u32) -> bool {
        hit(self.list(Container::L30), frequency)
    }
    pub fn f32c(&self, live_q: &Context, frequency: u32) -> bool {
        q_kind(live_q) < 16 && !hit(self.list(Container::LD0), frequency)
    }
    /// ED10/ED3C's special200MHz point is NOT ordinary L30 membership. This
    /// predicate belongs to the class-range warning, not user F32C.
    pub fn ed3c(&self, live_q: &Context, frequency: u32) -> bool {
        q_kind(live_q) < 16
            && frequency != 200_000_000
            && !hit(self.list(Container::L30), frequency)
    }
    /// Repeater warning ECDC/EC2C admits zero separately from L80 membership.
    pub fn ecdc(&self, live_q: &Context, frequency: u32) -> bool {
        q_kind(live_q) < 16 && frequency != 0 && !hit(self.list(Container::L80), frequency)
    }
    /// ED70 is first HIGH>=frequency, not membership or nearest interval.
    pub fn ed70(&self, frequency: u32) -> u32 {
        first_high(self.list(Container::LA8), frequency) as u32
    }
    pub fn eecc(&self, frequency: u32) -> u32 {
        first_high(self.list(Container::L120), frequency) as u32
    }
    pub fn ef4c(&self, frequency: u32) -> u32 {
        first_high(self.list(Container::L148), frequency) as u32
    }
    pub fn f094(&self, frequency: u32) -> u32 {
        first_high(self.list(Container::L80), frequency) as u32
    }
    pub fn f014(&self, frequency: u32) -> i32 {
        self.list(Container::L8)
            .iter()
            .rposition(|r| frequency >= r.low)
            .map_or(-1, |i| i as i32)
    }
    pub fn efcc(&self, frequency: u32) -> i32 {
        match self.f014(frequency) as u32 {
            0..=9 => 0,
            10..=12 => 1,
            13..=15 => 2,
            16 => 3,
            _ => 4,
        }
    }
    pub fn edf0(&self, kind: u32, frequency: u32) -> i32 {
        match kind {
            2 | 5 | 6 | 8 | 0x15 | 0x18 => self.efcc(frequency),
            3 | 4 => self.eecc(frequency) as i32,
            0xc => self.f094(frequency) as i32,
            0x16 => self.f014(frequency),
            _ => -1,
        }
    }
    pub fn f760(&self, kind: u32, index: i32, frequency: u32) -> FrequencyRange {
        if !matches!(kind, 2..=6 | 8 | 0xc | 0x15 | 0x16 | 0x18) {
            return INVALID;
        }
        if index == -1 {
            return range(
                30_000,
                if kind == 0xc {
                    1_399_999_999
                } else {
                    470_000_000
                },
            );
        }
        match kind {
            2 | 5 | 6 | 8 | 0x15 | 0x18 => get(self.list(Container::L30), index),
            3 | 4 => self.f5c4(index, frequency),
            0xc => get(self.list(Container::L80), index),
            0x16 => {
                let mut r = get(self.list(Container::L8), index);
                if index != 16 {
                    r.high = r.high.wrapping_add(1);
                }
                r
            }
            _ => INVALID,
        }
    }
    pub fn f5c4(&self, index: i32, frequency: u32) -> FrequencyRange {
        if (0..=13).contains(&index) {
            get(self.list(Container::L120), index)
        } else if index == 14 {
            get(self.list(Container::L148), self.ef4c(frequency) as i32)
        } else {
            INVALID
        }
    }
    /// Strict integer-Hz collection for scan/user's SAME-input two-pass path.
    /// Initial index is literal-1, second query frequency is literal0. The second
    /// value clamps ORIGINAL candidate, never the first clamped value.
    pub fn frequency_input(
        &self,
        kind: u32,
        candidate: u32,
    ) -> Result<super::special::FrequencyInput, Error> {
        if !matches!(kind, 8 | 0x15) {
            return Err(invalid("two-pass frequency input requires kind8 or15h"));
        }
        if !self.complete {
            return Err(invalid("frequency input requires a completed B generation"));
        }
        let first = self.f760(kind, -1, 0);
        let first_value = clamp_unsigned(candidate, first);
        let class = self.edf0(kind, first_value);
        let second = self.f760(kind, class, 0);
        Ok(super::special::FrequencyInput {
            first_accepted: true,
            second_value: clamp_unsigned(candidate, second),
            second_accepted: true,
        })
    }
}
fn q_kind(q: &Context) -> i32 {
    i32::from_le_bytes(q.as_bytes()[..4].try_into().expect("Q kind"))
}

/// F20C's normal four-class offset schema. It selects from LIVE Q through
/// 56CC8, not the B generation's captured selector. Values are integer Hz.
/// A class outside the four-entry source row would read adjacent native data;
/// this bounded API reports that missing source instead of inventing offset0.
pub fn f20c(live_q: &Context, system_langid: Option<u16>, class: i32) -> Result<u32, Error> {
    let selected = live_q.source_selector(system_langid)?;
    offset_for_source(selected, class)
}

/// Shared finite source schema. Callers such as type1D already captured their
/// actual source selector; F20C instead obtains it from live Q at each query.
pub fn offset_for_source(selected: u8, class: i32) -> Result<u32, Error> {
    if selected > 4 {
        return Err(invalid(
            "offset source selector requires its actual finite schema",
        ));
    }
    Ok(match (class, selected) {
        (0, 0) => 100_000,
        (0, _) => 500_000,
        (1, 0) => 0,
        (1, _) => 600_000,
        (2, 2 | 3) => 7_600_000,
        (2, _) => 5_000_000,
        (3, 0) => 20_000_000,
        (3, 1) => 12_000_000,
        (3, 2 | 3) => 28_000_000,
        (3, 4) => 0,
        _ => {
            return Err(invalid(
                "F20C class requires source outside its four-entry row",
            ));
        }
    })
}
fn first_high(list: &[FrequencyRange], frequency: u32) -> usize {
    list.iter()
        .position(|r| frequency <= r.high)
        .unwrap_or(list.len())
}
fn get(list: &[FrequencyRange], index: i32) -> FrequencyRange {
    usize::try_from(index)
        .ok()
        .and_then(|i| list.get(i))
        .copied()
        .unwrap_or(INVALID)
}
fn source_selector(selected: u32, langid: Option<u16>) -> Result<usize, Error> {
    if (selected as i32) < 0 {
        return Err(invalid(
            "negative source selector requires table-before source",
        ));
    }
    if selected < 5 {
        Ok(selected as usize)
    } else {
        let id = langid.ok_or(invalid("system LANGID source unavailable"))?;
        Ok(usize::from(id & 0x3ff != 0x11))
    }
}
fn clamp_unsigned(value: u32, bounds: FrequencyRange) -> u32 {
    // The numeric consumer ignores flag and compares lower then upper.
    if value < bounds.low {
        bounds.low
    } else if value > bounds.high {
        bounds.high
    } else {
        value
    }
}
fn intersect_into(
    out: &mut Vec<FrequencyRange>,
    outer: &[FrequencyRange],
    inner: &[FrequencyRange],
) -> Result<(), Error> {
    for &a in outer {
        for &b in inner {
            let r = intersection(a, b);
            if r.flag == 0 {
                add(out, r)?;
            }
        }
    }
    Ok(())
}

// Fixed native interval schemas, verified at the bounded source-schema RVAs.
// They are not copied factory images, seeds, heap nodes or serialized tables.
fn common_bands() -> [FrequencyRange; 3] {
    [
        range(30_000, 29_999_999),
        range(30_000_000, 74_799_999),
        range(74_800_000, 199_999_999),
    ]
}
fn class_bands() -> Vec<FrequencyRange> {
    // RVA191C60: sixteen consecutive classes followed by a separate UHF class.
    let edges = [
        30_000,
        1_600_000,
        2_000_000,
        6_000_000,
        8_000_000,
        11_000_000,
        15_000_000,
        20_000_000,
        22_000_000,
        26_000_000,
        30_000_000,
        45_000_000,
        60_000_000,
        74_800_000,
        108_000_000,
        137_000_000,
        200_000_000,
    ];
    let mut out = edges
        .windows(2)
        .map(|p| range(p[0], p[1] - 1))
        .collect::<Vec<_>>();
    out.push(range(400_000_000, 470_000_000));
    out
}
fn profile_receive(s: usize) -> Vec<FrequencyRange> {
    match s {
        0 => vec![
            range(30_000, 29_999_999),
            range(30_000_000, 59_999_999),
            range(60_000_000, 74_799_999),
            range(74_800_000, 146_000_000),
            range(430_000_000, 440_000_000),
        ],
        1..=3 => {
            let mut out = common_bands().to_vec();
            out.push(range(400_000_000, 470_000_000));
            out
        }
        4 => vec![
            range(30_000, 29_700_000),
            range(30_000_000, 74_799_999),
            range(144_000_000, 147_000_000),
            range(430_000_000, 440_000_000),
        ],
        _ => unreachable!("checked source selector"),
    }
}
fn profile_transmit(s: usize) -> Vec<FrequencyRange> {
    // RVA191600 base with the finite per-source differences at191708/77C/7F0/864.
    let mut v = vec![
        range(1_800_000, 1_999_999),
        range(3_500_000, 3_999_999),
        range(5_255_000, 5_405_000),
        range(7_000_000, 7_300_000),
        range(10_100_000, 10_150_000),
        range(14_000_000, 14_350_000),
        range(18_068_000, 18_168_000),
        range(21_000_000, 21_450_000),
        range(24_890_000, 24_990_000),
        range(28_000_000, 29_700_000),
        range(50_000_000, 54_000_000),
        range(68_000_000, 74_799_999),
        range(144_000_000, 146_000_000),
        range(430_000_000, 440_000_000),
    ];
    match s {
        0 => {}
        1 => {
            v[12].high = 148_000_000;
            v[13].high = 450_000_000;
        }
        2 | 3 => {
            v[0].low = 1_810_000;
            v[0].high = if s == 2 { 1_999_999 } else { 1_850_000 };
            v[1].high = 3_800_000;
            v[3].high = 7_200_000;
            v[10].high = 52_000_000;
            v[11] = if s == 2 {
                range(70_000_000, 70_500_000)
            } else {
                range(70_150_000, 70_250_000)
            };
        }
        4 => {
            v[0].high = 1_825_000;
            v[1].high = 3_540_000;
            v[3].high = 7_100_000;
            v[12].high = 147_000_000;
            v.remove(2);
        }
        _ => unreachable!("checked source selector"),
    }
    v
}
fn japanese_transmit() -> Vec<FrequencyRange> {
    // RVA191674: nine Japanese HF pieces, common upper-HF/50M, then VHF/UHF.
    let mut v = vec![
        range(1_800_000, 1_875_000),
        range(1_907_500, 1_912_500),
        range(3_500_000, 3_580_000),
        range(3_599_000, 3_612_000),
        range(3_662_000, 3_687_000),
        range(3_702_000, 3_716_000),
        range(3_745_000, 3_770_000),
        range(3_791_000, 3_805_000),
        range(7_000_000, 7_200_000),
    ];
    let base = profile_transmit(0);
    v.extend_from_slice(&base[4..11]);
    v.extend_from_slice(&base[12..14]);
    v
}
fn auxiliary_bands() -> Vec<FrequencyRange> {
    // RVA191960: call/BSR auxiliary bands; index12/13 are NOT logical row ids.
    vec![
        range(1_800_000, 1_999_999),
        range(3_400_000, 4_099_999),
        range(6_900_000, 7_499_999),
        range(9_900_000, 10_499_999),
        range(13_900_000, 14_499_999),
        range(17_900_000, 18_499_999),
        range(20_900_000, 21_499_999),
        range(24_400_000, 25_099_999),
        range(28_000_000, 29_999_999),
        range(50_000_000, 54_000_000),
        range(74_800_000, 107_999_999),
        range(108_000_000, 136_999_999),
        range(144_000_000, 148_000_000),
        range(420_000_000, 450_000_000),
    ]
}

/// Explicit DB98 object binding for the common/call predicate. None denotes a
/// missing source while Some(&[]) denotes an actually empty generated container.
pub struct FrequencyContext<'a> {
    pub common_kind: i32,
    pub l30: Option<&'a [FrequencyRange]>,
    pub l58: Option<&'a [FrequencyRange]>,
    pub l120: Option<&'a [FrequencyRange]>,
}

fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 0, reason }
}
fn hit(ranges: &[FrequencyRange], frequency: u32) -> bool {
    ranges
        .iter()
        .any(|r| r.low <= frequency && frequency <= r.high)
}

/// DB98 -> EB64: even common_kind>=16 requires the owner binding. L58 is read
/// after an L30 miss; only the final Boolean AND may short-circuit.
pub fn common_bad(context: Option<&FrequencyContext<'_>>, frequency: u32) -> Result<bool, Error> {
    let context = context.ok_or(invalid("frequency range object unavailable"))?;
    if context.common_kind >= 16 {
        return Ok(false);
    }
    let h30 = hit(
        context
            .l30
            .ok_or(invalid("frequency L30 source unavailable"))?,
        frequency,
    );
    let h58 = hit(
        context
            .l58
            .ok_or(invalid("frequency L58 source unavailable"))?,
        frequency,
    );
    Ok(!(h30 && h58))
}

fn holder_frequency(holder: &DecodedRecord) -> u32 {
    u32::from_le_bytes(
        holder.as_bytes()[0x3c..0x40]
            .try_into()
            .expect("holder frequency"),
    )
}

// C9360 has no kind gate on EB64. Only its extra F760 query is kind3/4.
// The callback lets the normal/call borrowed boundary share the same ordered
// predicate without inventing its missing L148 source or copying the logic.
fn channel_frequency_bad_with(
    holder: &DecodedRecord,
    context: Option<&FrequencyContext<'_>>,
    auxiliary: impl FnOnce(u32, i32, u32) -> Result<FrequencyRange, Error>,
) -> Result<bool, Error> {
    if !holder.valid() {
        return Ok(false);
    }
    let frequency = holder_frequency(holder);
    let mut bad = common_bad(context, frequency)?;
    if matches!(holder.kind(), 3 | 4) {
        let range_kind = i32::from_le_bytes(
            holder.as_bytes()[0x38..0x3c]
                .try_into()
                .expect("holder auxiliary range kind"),
        );
        let range = auxiliary(holder.kind(), range_kind, frequency)?;
        // Native closed-bound comparison ignores flag, including invalid's
        // [MAX,MAX] bounds. Do not replace this with "flag means bad".
        bad |= !(range.low <= frequency && frequency <= range.high);
    }
    Ok(bad)
}

/// C9360 for normal/call, compact04/05 and externally valid19/1A/1B holders.
/// Valid0 does not query B or Q. Valid1 always binds EB64; kind3/4 MUST query
/// F760(kind,H38,freq), even after shared bad=true or live kind>=16. H38 is
/// auxiliary12/13 for calls, not their row. Other kinds do not read H38.
/// This predicate intentionally does not include the mode24 test.
pub fn channel_frequency_bad(
    holder: &DecodedRecord,
    ranges: &Ranges,
    live_q: &Context,
) -> Result<bool, Error> {
    if !holder.valid() {
        return Ok(false);
    }
    channel_frequency_bad_with(
        holder,
        Some(&ranges.frequency_context(q_kind(live_q))),
        |kind, index, f| Ok(ranges.f760(kind, index, f)),
    )
}

/// 61CE0: arg0 is C9360 then full-DWORD mode24; arg1 is C9420's broad
/// 30000..470000000 closed-bound check only. Invalid holders bypass both.
/// Broad never reads B/Q, and does not include mode or machine permission.
pub fn channel_bad(
    holder: &DecodedRecord,
    ranges: &Ranges,
    live_q: &Context,
    broad: bool,
) -> Result<bool, Error> {
    if broad {
        return Ok(holder.valid() && !(30_000..=470_000_000).contains(&holder_frequency(holder)));
    }
    Ok(channel_frequency_bad(holder, ranges, live_q)? || (holder.valid() && holder.mode() == 24))
}

/// C9C80/C9DC8 for one Auto Repeater edge. Native side1 is the low edge.
/// The specific path captures R before checking Q13/validity; broad reads no Q.
/// Warning evaluation never changes validity, DUP, endpoints or reserved bytes.
pub fn auto_repeater_edge_bad(
    holder: &DecodedRecord,
    ranges: &Ranges,
    live_q: &Context,
    low: bool,
    numeric_only: bool,
    system_langid: Option<u16>,
) -> Result<bool, Error> {
    if holder.kind() != 0x18 {
        return Err(invalid("Auto Repeater predicate requires kind18h"));
    }
    let frequency = if low {
        u32::from_le_bytes(holder.as_bytes()[0x138..0x13c].try_into().unwrap())
    } else {
        holder_frequency(holder)
    };
    if numeric_only {
        return Ok(holder.valid() && !(30_000..=470_000_000).contains(&frequency));
    }
    // 569FC ignores its nominal C argument and queries live Q through100F4.
    // source_selector is the existing single implementation of that R boundary.
    let r = live_q.source_selector(system_langid)? == 0;
    if live_q.as_bytes()[0x13] != 1 || r || !holder.valid() {
        return Ok(false);
    }
    if !low
        && !auto_repeater_edge_bad(holder, ranges, live_q, true, false, system_langid)?
        && u32::from_le_bytes(holder.as_bytes()[0x138..0x13c].try_into().unwrap()) > frequency
    {
        return Ok(true);
    }
    let auxiliary = i32::from_le_bytes(holder.as_bytes()[0x38..0x3c].try_into().unwrap());
    let bounds = ranges.f760(holder.kind(), auxiliary, 0);
    Ok(!(bounds.low..=bounds.high).contains(&frequency) || ranges.eb64(live_q, frequency))
}

/// 61E88: the specific C9DA4 and numeric C9DC8 paths both try low, then high
/// only on false. The shared edge predicate also serves actual field warnings.
pub fn auto_repeater_bad(
    holder: &DecodedRecord,
    ranges: &Ranges,
    live_q: &Context,
    numeric_only: bool,
    system_langid: Option<u16>,
) -> Result<bool, Error> {
    Ok(
        auto_repeater_edge_bad(holder, ranges, live_q, true, numeric_only, system_langid)?
            || auto_repeater_edge_bad(holder, ranges, live_q, false, numeric_only, system_langid)?,
    )
}

/// Borrowed normal/call61CE0 arg0. This boundary requires the real L120 source
/// at call H38=aux12/13, even after shared bad=true. It cannot synthesize L148
/// for arbitrary H38. Full-kind consumers use channel_bad with an owned B.
pub fn previous_channel_bad(
    holder: &DecodedRecord,
    context: Option<&FrequencyContext<'_>>,
) -> Result<bool, Error> {
    if !matches!(holder.kind(), 2 | 3) {
        return Err(invalid("61CE0 channel predicate requires kind2 or3"));
    }
    let bad = channel_frequency_bad_with(holder, context, |_, index, _| {
        context
            .expect("checked object binding")
            .l120
            .ok_or(invalid("call L120 source unavailable"))?
            .get(index as u32 as usize)
            .copied()
            .ok_or(invalid("call auxiliary range index unavailable"))
    })?;
    // C94A8 and10144 are real constant-false functions, not unknown fallbacks.
    Ok(bad || (holder.valid() && holder.mode() == 24))
}

#[cfg(test)]
mod tests {
    use super::super::record;
    use super::*;

    // Independently calculated from35 hashed PE source windows using normal
    // point-set union/difference/intersection, not these native algorithms.
    const GENERATION_ORACLE: u64 = 0xc509_23aa_09c8_39dd;

    fn q(kind: i32, (a, b, c, d, e, u, f): (u8, u8, u32, u8, u8, u8, u8)) -> Context {
        let mut raw = [0; 24];
        raw[..4].copy_from_slice(&kind.to_le_bytes());
        raw[4] = a;
        raw[5] = b;
        raw[8..12].copy_from_slice(&c.to_le_bytes());
        raw[12] = d;
        raw[13] = e;
        raw[14] = u;
        raw[15] = f;
        Context::from_bytes(raw)
    }
    fn bytes(mut h: u64, data: &[u8]) -> u64 {
        for &b in data {
            h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
        }
        h
    }
    fn digest(mut h: u64, r: &Ranges) -> u64 {
        for which in POPULATION_ORDER {
            h = bytes(h, &(r.list(which).len() as u32).to_le_bytes());
            for item in r.list(which) {
                h = bytes(h, &item.low.to_le_bytes());
                h = bytes(h, &item.high.to_le_bytes());
                h = bytes(h, &[item.flag]);
            }
        }
        h
    }
    fn bits(rows: &[FrequencyRange]) -> u8 {
        (0..5).fold(0, |bits, f| bits | u8::from(hit(rows, f)) << f)
    }

    #[test]
    fn interval_cases_flags_raw_loss_and_non_total_merge_order_are_literal() {
        let examples = [
            ((10, 11), (2, 5)),
            ((3, 5), (3, 5)),
            ((2, 6), (3, 5)),
            ((3, 5), (2, 6)),
            ((2, 5), (2, 6)),
            ((3, 6), (2, 6)),
            ((4, 8), (2, 6)),
            ((2, 5), (3, 8)),
            ((7, 8), (3, 6)),
            ((3, 6), (7, 8)),
        ];
        for (case, &((l, h), (a, b))) in examples.iter().enumerate() {
            assert_eq!(relationship(range(l, h), range(a, b)), case as u8);
        }
        for (a, b, first) in [
            ((3, 7), (1, 5), (1, 5)),
            ((3, 4), (1, 5), (3, 4)),
            ((3, 5), (1, 5), (1, 5)),
        ] {
            let mut v = vec![range(a.0, a.1), range(b.0, b.1)];
            v[0].flag = 0x80;
            v[1].flag = 255;
            sort_native(&mut v);
            assert_eq!((v[0].low, v[0].high), first);
            assert_eq!(v.iter().map(|r| u32::from(r.flag)).sum::<u32>(), 383);
        }
        let mut raw = vec![
            FrequencyRange {
                low: 10,
                high: 20,
                flag: 7,
            },
            range(15, 30),
        ];
        assert!(!add(&mut raw, range(12, 18)).unwrap());
        assert_eq!(
            raw,
            [FrequencyRange {
                low: 10,
                high: 20,
                flag: 7
            }]
        );
        let mut v = vec![FrequencyRange {
            low: 0,
            high: 10,
            flag: 0x80,
        }];
        assert!(!cut(&mut v, range(3, 7)).unwrap());
        assert_eq!(v, [range(0, 2), range(8, 10)]);
        for flag in 0..=255u8 {
            let mut v = vec![FrequencyRange {
                low: 3,
                high: 9,
                flag,
            }];
            assert!(
                add(
                    &mut v,
                    FrequencyRange {
                        low: 3,
                        high: 9,
                        flag: flag ^ 255
                    }
                )
                .unwrap()
            );
            assert_eq!(v[0].flag, flag ^ 255);
            cut(&mut v, range(3, 4)).unwrap();
            assert_eq!(v[0].flag, flag ^ 255);
            let a = FrequencyRange {
                low: 3,
                high: 5,
                flag,
            };
            let b = FrequencyRange {
                low: 1,
                high: 9,
                flag: flag ^ 255,
            };
            assert_eq!(intersection(a, b), a);
            assert_eq!(intersection(b, a), a);
        }
    }

    #[test]
    fn canonical_algebra_matches_independent_five_point_sets() {
        let inputs = (0..5)
            .flat_map(|l| (l..5).map(move |h| range(l, h)))
            .collect::<Vec<_>>();
        for &a in &inputs {
            for &b in &inputs {
                for &c in &inputs {
                    let mut v = Vec::new();
                    for r in [a, b, c] {
                        add(&mut v, r).unwrap();
                    }
                    let expected = bits(&[a, b, c]);
                    assert_eq!(bits(&v), expected);
                    assert!(v.windows(2).all(|w| w[0].high < w[1].low));
                    for &remove in &inputs {
                        let mut cut_v = v.clone();
                        cut(&mut cut_v, remove).unwrap();
                        assert_eq!(bits(&cut_v), expected & !bits(&[remove]));
                    }
                }
            }
        }
    }

    #[test]
    fn optional_missing_marker_and_wrap_join_are_distinct_from_union() {
        let mut v = vec![range(4, 6), range(7, 8), range(9, 13)];
        v[0].flag = 19;
        assert_eq!(v.len(), 3);
        join_adjacent(&mut v);
        assert_eq!(
            v,
            [FrequencyRange {
                low: 4,
                high: 13,
                flag: 19
            }]
        );
        let mut v = vec![
            FrequencyRange {
                low: 0,
                high: 0,
                flag: 5,
            },
            range(u32::MAX, u32::MAX),
        ];
        join_adjacent(&mut v);
        assert_eq!(
            v,
            [FrequencyRange {
                low: u32::MAX,
                high: 0,
                flag: 5
            }]
        );
        let mut v = vec![range(3, 7), range(1, 5)];
        let old = v.clone();
        assert!(optional(&mut v, None).is_err());
        assert_eq!(v, old);
        optional(&mut v, Some(INVALID)).unwrap();
        assert_eq!(v, [range(1, 5), range(3, 7)]);
        let mut source = Vec::new();
        optional(
            &mut source,
            Some(FrequencyRange {
                low: 9,
                high: 10,
                flag: 255,
            }),
        )
        .unwrap();
        assert_eq!(source, [range(9, 10)]);
        let mut v = Vec::new();
        add(&mut v, range(u32::MAX, u32::MAX)).unwrap();
        assert_eq!(v, [range(u32::MAX, u32::MAX)]);
        assert_eq!(intersection(range(1, 2), range(3, 4)), INVALID);
    }

    #[test]
    fn all_five_source_schemas_and1600_flag_compounds_match_independent_pe_oracle() {
        let mut h = 0xcbf2_9ce4_8422_2325;
        for s in 0..5 {
            for c in 0..5 {
                for mask in 0..64 {
                    let flags = std::array::from_fn::<_, 6, _>(|i| ((mask >> i) & 1) as u8);
                    let q = q(
                        [0, 1, 2, 4, 5][s],
                        (
                            flags[0], flags[1], c, flags[2], flags[3], flags[4], flags[5],
                        ),
                    );
                    let generated = Ranges::generate(&q, None).unwrap();
                    assert!(generated.is_complete());
                    assert_eq!(generated.selected(), s as u32);
                    assert_eq!(generated.source_q(), Some(q.as_bytes()));
                    for value in [s as u32, c, mask] {
                        h = bytes(h, &value.to_le_bytes());
                    }
                    h = digest(h, &generated);
                }
            }
        }
        assert_eq!(h, GENERATION_ORACLE);
        let low = q(0, (1, 1, 3, 1, 1, 1, 1));
        let high = q(0, (128, 128, 3, 128, 128, 128, 128));
        assert_eq!(
            Ranges::generate(&low, None).unwrap().lists,
            Ranges::generate(&high, None).unwrap().lists
        );
        let unknown_c = q(0, (1, 1, 0x103, 1, 1, 1, 1));
        let other = Ranges::generate(&unknown_c, None).unwrap();
        assert!(hit(other.list(Container::LA8), 5_300_000));
        assert!(!hit(
            Ranges::generate(&low, None).unwrap().list(Container::LA8),
            5_300_000
        ));
        let mut override_raw = *low.as_bytes();
        override_raw[22] = 128;
        assert_eq!(
            Ranges::generate(&Context::from_bytes(override_raw), None)
                .unwrap()
                .lists,
            Ranges::generate(&q(1, (1, 1, 3, 1, 1, 1, 1)), None)
                .unwrap()
                .lists
        );
        let live = q(5, (1, 1, 3, 1, 1, 1, 1));
        let mut captured = Ranges::unpopulated();
        captured.regenerate(0, &live, None).unwrap();
        assert_eq!(captured.selected(), 0);
        assert!(!hit(captured.list(Container::L58), 40_000_000));
        assert!(!hit(captured.list(Container::LA8), 435_000_000));
    }

    #[test]
    fn generation_failure_prefix_and_independent_live_q_do_not_refresh_b() {
        let q0 = q(0, (1, 1, 3, 1, 1, 1, 1));
        let mut r = Ranges::unpopulated();
        assert_eq!(r.selected(), 5);
        assert!(!r.is_complete());
        assert!(r.source_q().is_none());
        for list in &mut r.lists {
            *list = vec![range(123, 456)];
        }
        assert!(
            r.regenerate_with(2, &q0, None, |which| if which == Container::LD0 {
                Err(invalid("injected population stop"))
            } else {
                Ok(())
            })
            .is_err()
        );
        assert_eq!(r.list(Container::L8).len(), 17);
        assert_eq!(r.list(Container::L30).len(), 4);
        for which in [
            Container::LD0,
            Container::LF8,
            Container::L120,
            Container::L148,
            Container::L80,
        ] {
            assert_eq!(r.list(which), [range(123, 456)]);
        }
        assert!(!r.is_complete());
        assert!(r.frequency_input(8, 145_000_000).is_err());
        let lists = r.lists.clone();
        assert!(r.regenerate(16, &q0, None).is_err());
        assert_eq!(r.selected(), 16);
        assert_eq!(r.lists, lists);
        let generated = Ranges::generate(&q0, None).unwrap();
        let old = generated.clone();
        let live = q(16, (0, 0, 0, 0, 0, 0, 0));
        assert!(!generated.eb64(&live, 1));
        assert!(!generated.f32c(&live, 1));
        assert_eq!(generated, old);
        assert!(std::ptr::eq(generated.db98(123), &generated));
        assert!(generated.eb64(&q(-1, (0, 0, 0, 0, 0, 0, 0)), 1));
        let jp = Ranges::generate(&Context::before_registry(), Some(0x411)).unwrap();
        let other = Ranges::generate(&Context::before_registry(), Some(0x409)).unwrap();
        assert_eq!((jp.selected(), other.selected()), (0, 1));
        assert!(!jp.list(Container::LF8).is_empty());
        assert!(other.list(Container::LF8).is_empty());
    }

    #[test]
    fn classifiers_use_the_native_endpoint_direction_and_query_wrap() {
        let q0 = q(0, (1, 1, 3, 1, 1, 1, 1));
        let r = Ranges::generate(&q0, None).unwrap();
        assert_eq!(r.f014(0), -1);
        assert_eq!(r.efcc(0), 4);
        for (i, item) in r.list(Container::L8).iter().enumerate() {
            assert_eq!(r.f014(item.low), i as i32);
            assert_eq!(r.f014(item.high), i as i32);
            assert_eq!(r.f014(item.low - 1), i as i32 - 1);
        }
        assert_eq!(r.ed70(0), 0);
        assert_eq!(r.ed70(2_500_000), 1);
        assert!(r.f32c(&q0, 2_500_000));
        assert_eq!(r.ed70(u32::MAX), r.list(Container::LA8).len() as u32);
        for (i, item) in r.list(Container::LA8).iter().enumerate() {
            assert_eq!(r.ed70(item.low - 1), i as u32);
            assert_eq!(r.ed70(item.high), i as u32);
            assert_eq!(r.ed70(item.high + 1), i as u32 + 1);
        }
        assert!(!r.ed3c(&q0, 200_000_000));
        assert!(r.ed3c(&q0, 200_000_001));
        assert!(r.f32c(&q0, 200_000_000));
        assert!(!r.ecdc(&q0, 0));
        assert!(r.ecdc(&q0, 1));
        assert!(!r.ecdc(&q0, 1_200_000_000));
        assert_eq!(r.f760(0x16, 0, 0), range(30_000, 1_600_000));
        assert_eq!(r.f760(0x16, 16, 0), range(400_000_000, 470_000_000));
        assert_eq!(
            r.f760(0x16, 17, 0),
            FrequencyRange {
                low: u32::MAX,
                high: 0,
                flag: 1
            }
        );
        assert_eq!(r.f760(0xc, -1, 0), range(30_000, 1_399_999_999));
        for kind in 0..=31 {
            let known = matches!(kind, 2..=6 | 8 | 0xc | 0x15 | 0x16 | 0x18);
            assert_eq!(r.f760(kind, -1, 0).flag, u8::from(!known));
            if !known {
                assert_eq!(r.edf0(kind, 145_000_000), -1);
            }
        }
        assert_eq!(r.edf0(0xc, 0), 0);
        assert_eq!(r.edf0(0xc, 1_400_000_000), 4);
        assert_eq!(r.f760(3, 12, 1), range(144_000_000, 148_000_000));
        assert_eq!(r.f760(3, 13, 1), range(420_000_000, 450_000_000));
        assert_eq!(r.f760(3, 14, 0), r.list(Container::L148)[0]);
        assert_eq!(r.f760(3, 14, u32::MAX), INVALID);
    }

    #[test]
    fn common_product_and_previous_predicate_keep_source_read_boundaries() {
        let yes = [range(10, 20)];
        let no = [];
        for a in [false, true] {
            for b in [false, true] {
                let ctx = FrequencyContext {
                    common_kind: 0,
                    l30: Some(if a { &yes } else { &no }),
                    l58: Some(if b { &yes } else { &no }),
                    l120: None,
                };
                assert_eq!(common_bad(Some(&ctx), 15).unwrap(), !(a && b));
            }
        }
        let mut h = record::reset_holder(2, 0, 0);
        h.set_valid(false);
        h.set_mode(24);
        assert!(!previous_channel_bad(&h, None).unwrap());
        h.set_valid(true);
        assert!(previous_channel_bad(&h, None).is_err());
        h.set_mode(0);
        let missing = FrequencyContext {
            common_kind: 0,
            l30: Some(&no),
            l58: None,
            l120: None,
        };
        assert!(previous_channel_bad(&h, Some(&missing)).is_err());
        let bypass = FrequencyContext {
            common_kind: 16,
            l30: None,
            l58: None,
            l120: None,
        };
        assert!(!previous_channel_bad(&h, Some(&bypass)).unwrap());
        for mode in 0..=255 {
            h.set_mode(mode);
            assert_eq!(previous_channel_bad(&h, Some(&bypass)).unwrap(), mode == 24);
        }
        h.set_mode(24);
        assert!(previous_channel_bad(&h, None).is_err());
        h.set_mode(0x118);
        assert!(!previous_channel_bad(&h, Some(&bypass)).unwrap());
        h.set_header(3, 0, 0);
        h.as_mut_bytes()[0x38..0x3c].copy_from_slice(&12u32.to_le_bytes());
        assert!(previous_channel_bad(&h, Some(&missing)).is_err());
        let mut aux = [range(0, 1); 14];
        aux[12] = range(10, 20);
        let ctx = FrequencyContext {
            common_kind: 0,
            l30: Some(&no),
            l58: Some(&no),
            l120: Some(&aux),
        };
        assert!(previous_channel_bad(&h, Some(&ctx)).unwrap());
        let unavailable = FrequencyContext { l120: None, ..ctx };
        assert!(previous_channel_bad(&h, Some(&unavailable)).is_err());
    }

    #[test]
    fn c9360_and61ce0_share_one_predicate_for_all_channel_kinds_and_broad_is_separate() {
        let q0 = q(0, (1, 1, 3, 1, 1, 1, 1));
        let ranges = Ranges::generate(&q0, None).unwrap();
        let live = q(16, (1, 1, 3, 1, 1, 1, 1));
        for kind in [2, 3, 4, 5, 0x19, 0x1a, 0x1b] {
            let mut h = record::reset_holder(kind, 0, 0);
            h.set_valid(false);
            h.set_mode(24);
            assert!(!channel_frequency_bad(&h, &Ranges::unpopulated(), &q0).unwrap());
            assert!(!channel_bad(&h, &ranges, &q0, false).unwrap());
            assert!(!channel_bad(&h, &ranges, &q0, true).unwrap());
            h.set_valid(true);
            h.as_mut_bytes()[0x38..0x3c].copy_from_slice(&12u32.to_le_bytes());
            for (f, shared_bad, aux_bad) in [
                (1, true, true),
                (144_000_000, false, false),
                (148_000_000, true, false),
                (148_000_001, true, true),
                (u32::MAX, true, true),
            ] {
                h.as_mut_bytes()[0x3c..0x40].copy_from_slice(&f.to_le_bytes());
                assert_eq!(channel_frequency_bad(&h, &ranges, &q0).unwrap(), shared_bad);
                assert_eq!(
                    channel_frequency_bad(&h, &ranges, &live).unwrap(),
                    matches!(kind, 3 | 4) && aux_bad
                );
                assert!(channel_bad(&h, &ranges, &q0, false).unwrap()); // Full mode24.
                assert_eq!(
                    channel_bad(&h, &ranges, &q0, true).unwrap(),
                    !(30_000..=470_000_000).contains(&f)
                );
            }
            h.set_mode(0x118);
            h.as_mut_bytes()[0x3c..0x40].copy_from_slice(&144_000_000u32.to_le_bytes());
            assert!(!channel_bad(&h, &ranges, &q0, false).unwrap());
            if matches!(kind, 2 | 3) {
                assert_eq!(
                    channel_bad(&h, &ranges, &q0, false).unwrap(),
                    previous_channel_bad(&h, Some(&ranges.frequency_context(0))).unwrap()
                );
            }
        }
        // Auxiliary query is mandatory even for kind>=16 or shared bad=true;
        // other kinds never query it. Invalid returns before either source.
        for kind in [2, 3, 4, 5, 0x19, 0x1a, 0x1b] {
            let mut h = record::reset_holder(kind, 0, 0);
            h.set_valid(false);
            assert!(
                !channel_frequency_bad_with(&h, None, |_, _, _| panic!("invalid source read"))
                    .unwrap()
            );
            h.set_valid(true);
            for common_kind in [0, 16] {
                let empty = [];
                let ctx = FrequencyContext {
                    common_kind,
                    l30: Some(&empty),
                    l58: Some(&empty),
                    l120: None,
                };
                assert_eq!(
                    channel_frequency_bad_with(&h, Some(&ctx), |_, _, _| Err(invalid(
                        "required auxiliary source"
                    )))
                    .is_err(),
                    matches!(kind, 3 | 4)
                );
            }
        }
        let mut h = record::reset_holder(4, 0, 0);
        h.set_valid(true);
        h.as_mut_bytes()[0x38..0x3c].copy_from_slice(&14u32.to_le_bytes());
        h.as_mut_bytes()[0x3c..0x40].copy_from_slice(&30_000u32.to_le_bytes());
        assert!(!channel_frequency_bad(&h, &ranges, &q0).unwrap());
        h.as_mut_bytes()[0x38..0x3c].copy_from_slice(&15u32.to_le_bytes());
        assert!(channel_frequency_bad(&h, &ranges, &q0).unwrap());
        h.as_mut_bytes()[0x3c..0x40].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(!channel_frequency_bad(&h, &ranges, &live).unwrap()); // INVALID flag ignored, MAX matches [MAX,MAX].
        for f in [
            29_999u32,
            30_000,
            30_001,
            469_999_999,
            470_000_000,
            470_000_001,
        ] {
            h.as_mut_bytes()[0x3c..0x40].copy_from_slice(&f.to_le_bytes());
            assert_eq!(
                channel_bad(&h, &ranges, &live, true).unwrap(),
                !(30_000..=470_000_000).contains(&f)
            );
        }
    }

    #[test]
    fn integer_hz_input_reparses_original_candidate_and_keeps_broad_band_gaps_separate() {
        let q0 = q(0, (1, 1, 3, 1, 1, 1, 1));
        let mut r = Ranges::generate(&q0, None).unwrap();
        for (candidate, expected) in [
            (0, 30_000),
            (29_999, 30_000),
            (30_000, 30_000),
            (200_000_000, 199_999_999),
            (399_999_999, 199_999_999),
            (400_000_000, 400_000_000),
            (u32::MAX, 470_000_000),
        ] {
            let input = r.frequency_input(8, candidate).unwrap();
            assert!(input.first_accepted && input.second_accepted);
            assert_eq!(input.second_value, expected);
            assert_eq!(
                r.frequency_input(0x15, candidate).unwrap().second_value,
                expected
            );
        }
        assert!(r.frequency_input(2, 1).is_err());
        r.lists[Container::L30 as usize][3] = range(0, 1_000_000_000);
        assert_eq!(
            r.frequency_input(8, u32::MAX).unwrap().second_value,
            1_000_000_000
        ); // Not470M from the first clamp.
    }
}
