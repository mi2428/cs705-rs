use super::*;
use crate::icf::frequency::Container;

pub(super) fn source_class(s: &Sources<'_>, row: u32, extra: u32) -> Result<u32, Error> {
    if row >= 2500 {
        return Ok(4);
    }
    let h = repeater(s, row)?;
    let hz = h.channel_field(C::FrequencyHz);
    if hz == 0 || h.channel_field(C::Duplex) == 0 {
        return Ok(1);
    }
    if extra as u8 != 0 {
        if !s.ranges.is_complete() {
            return Err(invalid("DR source requires completed B generation"));
        }
        let supported = s.ranges.eb98(hz)
            && s.ranges
                .list(Container::L58)
                .iter()
                .any(|r| r.low <= hz && hz <= r.high);
        if !supported || h.offset_hz() > 9_999_900 {
            return Ok(1);
        }
    }
    Ok(if h.repeater_text(T::Gateway)[7] == b'G' {
        3
    } else {
        2
    })
}
pub(super) fn preview_allowed(source: u32, target: u32) -> bool {
    match source {
        0 => matches!(target, 0 | 5 | 6),
        2 => matches!(target, 1 | 2 | 5 | 6),
        3 => matches!(target, 1 | 2 | 4 | 5 | 6 | 7 | 8),
        _ => false,
    }
}

/// BB1E8 writes source class before strlen/row reads and can rewrite UR16.
pub(super) fn classify(
    s: &Sources<'_>,
    source: &[u8; 16],
    value: &mut [u8; 16],
    mode: u32,
    extra: u32,
    reads: &ReadInputs<'_>,
    source_out: &mut u32,
) -> Result<u32, Error> {
    let source_row = if blank(source) {
        *source_out = 0;
        reads.blank_source_index.ok_or(invalid(
            "BB1E8 incoming blank-source stack index unavailable",
        ))?
    } else {
        let row = named_or_call(s, source, mode)?;
        *source_out = source_class(s, row, extra)?;
        row
    };
    let mut normalized = c_normalized(value, reads)?;
    if normalized[0] == b'/' {
        let call = unslash_call(normalized[..8].try_into().unwrap(), reads)?;
        normalized[..8].copy_from_slice(&call);
    }
    let source_holder = if source_row < 2500 {
        Some(repeater(s, source_row)?)
    } else {
        None
    };
    let internet_value = padded::<16>(internet(s.language));
    let cq_value = padded::<16>(b"CQCQCQ");
    let mut target = if blank(&normalized) {
        0
    } else if normalized == internet_value {
        8
    } else if mode == 3
        && value[0] != b'/'
        && *source_out == 3
        && blank(&normalized[7..])
        && source_holder
            .as_ref()
            .is_some_and(|h| normalized[..7] == h.repeater_text(T::Gateway)[..7])
    {
        8
    } else if normalized == cq_value {
        5
    } else if lookup_ur_name(s, &normalized)? < 300 {
        6
    } else {
        let named = lookup_repeater_name(s, &normalized, mode)?;
        let rejected = if named != 0xffff {
            let h = repeater(s, named)?;
            (mode == 2 && blank(h.repeater_text(T::Call)))
                || (mode == 3 && h.channel_field(C::Duplex) != 3)
        } else {
            false
        };
        if rejected {
            0
        } else {
            let row = if named < 2500 {
                named
            } else {
                lookup_repeater(s, &normalized, None, 0, mode)?
            };
            if row >= 2500 {
                6
            } else if matches!(*source_out, 0 | 4) {
                1
            } else {
                let q = repeater(s, row)?;
                let a = source_holder
                    .as_ref()
                    .ok_or(invalid("DR pair source row unavailable"))?;
                let call = q.repeater_text(T::Call);
                let gateway = q.repeater_text(T::Gateway);
                if mode == 3
                    && call[7] == b' '
                    && blank(q.name())
                    && call[..7] == a.repeater_text(T::Gateway)[..7]
                {
                    8
                } else if call == a.repeater_text(T::Call) {
                    1
                } else if !blank(gateway) && gateway == a.repeater_text(T::Gateway) {
                    2
                } else if gateway[7] == b'G' {
                    4
                } else {
                    3
                }
            }
        }
    };
    if value[0] == b'/' {
        match target {
            1 | 2 => {
                let call = unslash_call(value[..8].try_into().unwrap(), reads)?;
                value[..8].copy_from_slice(&call);
            }
            3 => {
                value.fill(b' ');
                target = 0;
            }
            _ => target = 7,
        }
    }
    Ok(target)
}

fn copy_calls(h: &mut DecodedRecord, ur: &[u8; 8], r1: &[u8; 8], r2: &[u8; 8]) {
    h.set_callsign(MemoryCallsign::Your, ur);
    h.set_callsign(MemoryCallsign::Repeater1, r1);
    h.set_callsign(MemoryCallsign::Repeater2, r2);
}

/// BC08C's scalar-first consumer. It never calls a saver or creates a cache.
pub fn apply_requests(
    s: &Sources<'_>,
    target: &mut DecodedRecord,
    other: &DecodedRecord,
    source: u32,
    your: u32,
    rpt: u32,
    raw: &[u8; 16],
    reads: &ReadInputs<'_>,
) -> Result<(), Error> {
    // The native source scratch is reset even when its row is not loaded.
    let a = if source < 2500 {
        repeater(s, source)?
    } else {
        record::reset_holder(0, 0, 0)
    };
    let old_valid = target.valid();
    if !old_valid {
        if target.kind() == 6 {
            return Err(invalid("type1E native NULL seed source"));
        }
        let selector = s.ranges.default_source_selector(s.system_langid)?;
        let mut seed = record::reset_holder(0x1d, 0, 0);
        record::initial_normal_channel_into(s.defaults, 0, selector, &mut seed)?;
        if matches!(seed.mode(), 0 | 1) {
            seed.set_mode(u32::from(a.channel_field(C::FrequencyHz) >= 10_000_000));
        }
        target.copy_body_from(&seed);
        target.set_valid(true);
    }
    if source < 2500 {
        target.set_channel_field(C::FrequencyHz, a.channel_field(C::FrequencyHz));
        let dup = a.channel_field(C::Duplex) as u8;
        target.set_channel_field(C::Duplex, if dup <= 3 { u32::from(dup) } else { 0 });
        let mode = match a.repeater_field(R::Mode) as u8 {
            1 => 0x0e,
            2 => 0x10,
            3 => 0x11,
            _ => 0x0d,
        };
        target.set_mode(mode);
        if mode == 0x10 {
            target.set_offset_hz(a.offset_hz());
        }
    } else {
        target.set_mode(0x10);
    }
    if !old_valid {
        target.set_filter(2);
    } else if source >= 2500 || a.repeater_field(R::Mode) == 2 {
        target.set_filter(if other.mode() == 0x10 {
            other.filter()
        } else {
            2
        });
    } else if a.repeater_field(R::Mode) == 3 {
        target.set_filter(0);
    }

    let empty = [b' '; 8];
    let raw8: &[u8; 8] = raw[..8].try_into().unwrap();
    if your == 2500 {
        copy_calls(target, &empty, &empty, &empty);
    } else if your == 2503 {
        let mut call = *a.repeater_text(T::Gateway);
        call[7] = b' ';
        copy_calls(
            target,
            &call,
            a.repeater_text(T::Call),
            a.repeater_text(T::Gateway),
        );
    } else if source == 2500 && rpt == 2500 {
        let value = if your < 2500 {
            *repeater(s, your)?.repeater_text(T::Call)
        } else if your == 2501 {
            CQ
        } else {
            *raw8
        };
        copy_calls(target, &value, &empty, &empty);
    } else if your == 2501 {
        let a = repeater(s, source)?;
        copy_calls(target, &CQ, a.repeater_text(T::Call), &empty);
    } else if your < 2500 {
        let q = repeater(s, your)?;
        if a.repeater_text(T::Call) == q.repeater_text(T::Call) {
            copy_calls(target, &CQ, a.repeater_text(T::Call), &empty);
        } else if a.repeater_text(T::Gateway) == q.repeater_text(T::Gateway) {
            copy_calls(
                target,
                &CQ,
                a.repeater_text(T::Call),
                q.repeater_text(T::Call),
            );
        } else {
            // C-string begins at +160; gateway and later holder fields are real
            // readable bytes, not a fabricated NUL directly after call8.
            let span = &q.as_bytes()[0x160..];
            let length = span
                .iter()
                .position(|b| *b == 0)
                .or_else(|| {
                    reads
                        .string_tail
                        .iter()
                        .position(|b| *b == 0)
                        .map(|n| span.len() + n)
                })
                .ok_or(invalid("Area CQ source C-string NUL unavailable"))?;
            if length < 9 {
                return Err(invalid("Area CQ native AnsiString index9 unavailable"));
            }
            let call = slash_call(q.repeater_text(T::Call), reads)?;
            copy_calls(
                target,
                &call,
                a.repeater_text(T::Call),
                a.repeater_text(T::Gateway),
            );
        }
    } else if your == 2502 {
        if raw8[0] == b'/' {
            copy_calls(
                target,
                raw8,
                a.repeater_text(T::Call),
                a.repeater_text(T::Gateway),
            );
        } else if rpt == 2500 {
            copy_calls(target, raw8, a.repeater_text(T::Call), &empty);
        } else if rpt == 2501 {
            copy_calls(
                target,
                raw8,
                a.repeater_text(T::Call),
                a.repeater_text(T::Gateway),
            );
        } else {
            let r = repeater(s, rpt)?;
            copy_calls(
                target,
                raw8,
                a.repeater_text(T::Call),
                r.repeater_text(T::Call),
            );
        }
    }
    Ok(())
}

fn apply_text(
    s: &Sources<'_>,
    h: &mut DecodedRecord,
    other: &DecodedRecord,
    source: &[u8; 16],
    value: &[u8; 16],
    rpt_text: &[u8],
    flag: u32,
    mode: u32,
    reads: &ReadInputs<'_>,
) -> Result<(), Error> {
    let a = if blank(source) {
        2500
    } else {
        named_or_call(s, source, mode)?.min(2500)
    };
    let mut normalized = c_normalized(value, reads)?;
    let b = if blank(&normalized) {
        2500
    } else if mode == 3 && normalized == padded::<16>(internet(s.language)) {
        2503
    } else if mode == 3
        && flag != 0
        && blank(&normalized[7..])
        && a < 2500
        && normalized[..7] == repeater(s, a)?.repeater_text(T::Gateway)[..7]
    {
        2503
    } else if normalized == padded::<16>(b"CQCQCQ") {
        2501
    } else {
        let u = lookup_ur_name(s, &normalized)?;
        if u < 300 {
            normalized[..8].copy_from_slice(ur(s, u)?.callsign(MemoryCallsign::Your));
            normalized[8] = 0;
            2502
        } else {
            let row = named_or_call(s, &normalized, mode)?;
            if row < 2500 { row } else { 2502 }
        }
    };
    let c = match flag {
        0 | 1 => 2500,
        3 => 2501,
        _ => named_or_call(s, &padded(rpt_text), mode)?,
    };
    apply_requests(s, h, other, a, b, c, &normalized, reads)
}

fn ur_metadata(s: &Sources<'_>, h: &mut DecodedRecord) -> Result<(), Error> {
    let call = *h.callsign(MemoryCallsign::Your);
    h.as_mut_bytes()[0x90..0xa0].fill(b' ');
    h.as_mut_bytes()[0x90..0x98].copy_from_slice(&call);
    put(h, 0xa0, 2500);
    let row = lookup_ur_call(s, &call)?;
    if row < 300 {
        let q = ur(s, row)?;
        if !blank(q.name()) {
            h.as_mut_bytes()[0x90..0xa0].copy_from_slice(q.name());
        }
        return Ok(());
    }
    let row = lookup_slash(s, &call, derived_mode(h.mode()))?;
    if row != 0xffff {
        let q = repeater(s, row)?;
        put(h, 0xa0, row);
        if !blank(q.name()) {
            h.as_mut_bytes()[0x90..0xa0].copy_from_slice(q.name());
        }
        return Ok(());
    }
    if h.mode() == 0x11
        && call[7] == b' '
        && [MemoryCallsign::Repeater1, MemoryCallsign::Repeater2]
            .iter()
            .any(|f| {
                let rpt = h.callsign(*f);
                rpt[7] == b'G' && rpt[..7] == call[..7]
            })
    {
        let token = internet(s.language);
        h.as_mut_bytes()[0x90..0xa0].fill(b' ');
        h.as_mut_bytes()[0x90..0x90 + token.len()].copy_from_slice(token);
    }
    Ok(())
}
fn rpt_metadata(s: &Sources<'_>, h: &mut DecodedRecord) -> Result<(), Error> {
    let mode = derived_mode(h.mode());
    for (field, at, index_at) in [
        (MemoryCallsign::Repeater1, 0xac, 0xc0),
        (MemoryCallsign::Repeater2, 0xcc, 0xe0),
    ] {
        let call = *h.callsign(field);
        h.as_mut_bytes()[at..at + 17].fill(b' ');
        let row = if field == MemoryCallsign::Repeater1 {
            lookup_call(s, &call, mode)?
        } else {
            lookup_rpt2(s, h.callsign(MemoryCallsign::Repeater1), &call, 1, mode)?
        };
        if row != 0xffff {
            let q = repeater(s, row)?;
            let accepted = field == MemoryCallsign::Repeater2
                || (q.channel_field(C::FrequencyHz) != 0 && q.channel_field(C::Duplex) != 0);
            put(h, index_at, if accepted { row } else { 2500 });
            if !blank(q.name()) {
                h.as_mut_bytes()[at..at + 16].copy_from_slice(q.name());
                if call[7] == b'G' {
                    h.as_mut_bytes()[at + 16] = b'G';
                }
                continue;
            }
        } else {
            put(h, index_at, 2500);
        }
        h.as_mut_bytes()[at..at + 8].copy_from_slice(&call);
    }
    Ok(())
}

/// BE120 -> BD33C -> BC7F0, including metadata that survives outer Cancel.
pub fn prepare_metadata(s: &Sources<'_>, h: &mut DecodedRecord) -> Result<(), Error> {
    ur_metadata(s, h)?;
    rpt_metadata(s, h)?;
    let mut scratch = record::reset_holder(h.kind(), h.index(), h.binding());
    scratch.copy_body_from(h);
    let dup = scratch.channel_field(C::Duplex) != 0;
    if blank(scratch.callsign(MemoryCallsign::Your)) {
        scratch.set_callsign(MemoryCallsign::Your, &CQ);
    }
    let r1 = *scratch.callsign(MemoryCallsign::Repeater1);
    let r2 = *scratch.callsign(MemoryCallsign::Repeater2);
    if blank(&r1) && !blank(&r2) {
        let mut r = r2;
        r[7] = if r[7] == b'G' { b' ' } else { r[7] };
        scratch.set_callsign(MemoryCallsign::Repeater1, &r);
    } else if !blank(&r1) && blank(&r2) {
        scratch.set_callsign(MemoryCallsign::Repeater2, &r1);
        let mut r = r1;
        if r[7] == b'G' {
            r[7] = b' ';
        }
        scratch.set_callsign(MemoryCallsign::Repeater1, &r);
    }
    ur_metadata(s, &mut scratch)?;
    rpt_metadata(s, &mut scratch)?;
    put(h, 0xe4, 0);
    for (at, len) in [(0xe8, 11), (0xf3, 16), (0x108, 16), (0x11c, 16)] {
        h.as_mut_bytes()[at..at + len].fill(b' ');
    }
    for at in [0x104, 0x118, 0x12c] {
        put(h, at, 2500);
    }
    put(h, 0x130, 1);
    let u = *scratch.callsign(MemoryCallsign::Your);
    let r1 = *scratch.callsign(MemoryCallsign::Repeater1);
    let r2 = *scratch.callsign(MemoryCallsign::Repeater2);
    let direct = !dup || (blank(&r1) && blank(&r2));
    let pair = word(&scratch, 0xc0) < 2500 && word(&scratch, 0xe0) < 2500;
    let cq = u == CQ;
    let slash = u[0] == b'/';
    let same = r1 == r2;
    let gateway = r2[7] == b'G';
    let (status, label, with_ur, with_r1, with_r2, selector): (
        u32,
        &[u8; 11],
        bool,
        bool,
        bool,
        u32,
    ) = if cq {
        if direct {
            (1, b"Direct     ", true, false, false, 1)
        } else if same {
            (
                if pair { 2 } else { 7 },
                if pair { b"Single     " } else { b"Single?    " },
                true,
                true,
                false,
                1,
            )
        } else if !gateway {
            (
                if pair { 3 } else { 8 },
                if pair { b"Asist Link " } else { b"Asist Link?" },
                true,
                true,
                false,
                1,
            )
        } else {
            (0, b"?          ", false, false, false, 1)
        }
    } else if !slash {
        if direct {
            (1, b"Direct     ", true, false, false, 1)
        } else if same {
            (
                if pair { 2 } else { 7 },
                if pair { b"Single     " } else { b"Single?    " },
                true,
                true,
                true,
                if blank(&h.as_bytes()[0xcc..0xdd]) {
                    1
                } else {
                    2
                },
            )
        } else if !gateway {
            (
                if pair { 3 } else { 8 },
                if pair { b"Asist Link " } else { b"Asist Link?" },
                true,
                true,
                true,
                2,
            )
        } else {
            (
                if pair { 4 } else { 9 },
                if pair { b"Gateway    " } else { b"Gateway?   " },
                true,
                true,
                false,
                3,
            )
        }
    } else if direct || same || !gateway {
        (0, b"?          ", false, false, false, 1)
    } else {
        (
            if pair { 4 } else { 9 },
            if pair { b"Gateway    " } else { b"Gateway?   " },
            true,
            true,
            false,
            1,
        )
    };
    put(h, 0xe4, status);
    h.as_mut_bytes()[0xe8..0xf3].copy_from_slice(label);
    if with_ur {
        let (at, index) = if cq && !direct && !same && !gateway {
            (0xcc, 0xe0)
        } else {
            (0x90, 0xa0)
        };
        h.as_mut_bytes()[0xf3..0x103].copy_from_slice(&scratch.as_bytes()[at..at + 16]);
        put(h, 0x104, word(&scratch, index));
    }
    if with_r1 {
        h.as_mut_bytes()[0x108..0x118].copy_from_slice(&scratch.as_bytes()[0xac..0xbc]);
        put(h, 0x118, word(&scratch, 0xc0));
    }
    if with_r2 {
        if same {
            if selector == 2 {
                h.as_mut_bytes().copy_within(0xcc..0xdc, 0x11c);
            }
        } else {
            h.as_mut_bytes()[0x11c..0x12c].copy_from_slice(&scratch.as_bytes()[0xcc..0xdc]);
        }
    }
    if with_r2 || (!cq && !slash && !direct && !same && gateway) {
        put(h, 0x12c, word(&scratch, 0xe0));
    }
    put(h, 0x130, selector);
    Ok(())
}

#[derive(Clone, Copy)]
pub enum Action<'a> {
    SourceText(&'a [u8]),
    UrText(&'a [u8]),
    RepeaterText(&'a [u8]),
    SourceList(ListSelection),
    UrList(ListSelection),
    Designation { selected: i32, accept: bool },
    Mode(u32),
}

pub struct Dialog {
    pub holder: DecodedRecord,
    pub other: DecodedRecord,
    pub mode: u32,
    pub source: [u8; 16],
    pub your: [u8; 16],
    pub repeater_text: Vec<u8>,
    pub repeater_caption: Vec<u8>,
    pub source_class: u32,
    pub your_class: u32,
    pub candidates: Vec<Vec<u8>>,
    pub selected: i32,
    pub first_marker: Option<i32>,
    pub end_marker: i32,
    pub repeater_flag: u32,
    pub repeater_enabled: bool,
    pub preview: [Vec<u8>; 3],
    pub checkbox: bool,
    pub last_list_mode: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogOutcome {
    pub accepted: bool,
    pub save: bool,
    pub checkbox: bool,
}

impl Dialog {
    pub fn open(
        s: &Sources<'_>,
        original: &mut DecodedRecord,
        reads: &ReadInputs<'_>,
    ) -> Result<Self, Error> {
        prepare_metadata(s, original)?;
        let holder = original.clone();
        let mut other = record::reset_holder(0, 0, original.binding());
        for field in [C::FrequencyHz, C::Duplex] {
            other.set_channel_field(field, original.channel_field(field));
        }
        other.set_offset_hz(original.offset_hz());
        other.set_mode(original.mode());
        other.set_filter(original.filter());
        other.set_valid(original.valid());
        let mode = if other.valid() && other.channel_field(C::Duplex) != 0 {
            derived_mode(other.mode())
        } else {
            4
        };
        let (source, your, repeater_text, flag) = if mode == 4 {
            ([b' '; 16], padded(b"CQCQCQ"), Vec::new(), 0)
        } else {
            (
                original.as_bytes()[0x108..0x118].try_into().unwrap(),
                original.as_bytes()[0xf3..0x103].try_into().unwrap(),
                trim_right(prefix(&original.as_bytes()[0x11c..0x12c])).to_vec(),
                word(original, 0x130),
            )
        };
        let mut d = Self {
            holder,
            other,
            mode,
            source,
            your,
            repeater_text,
            repeater_caption: Vec::new(),
            source_class: 0,
            your_class: 0,
            candidates: Vec::new(),
            selected: -1,
            first_marker: reads.initial_first_marker,
            end_marker: -1,
            repeater_flag: flag,
            repeater_enabled: false,
            preview: std::array::from_fn(|_| Vec::new()),
            checkbox: false,
            last_list_mode: None,
        };
        if mode != 4 {
            apply_text(
                s,
                &mut d.holder,
                &d.other,
                &d.source,
                &d.your,
                &d.repeater_text,
                flag,
                mode,
                reads,
            )?;
        } else {
            copy_calls(&mut d.holder, &CQ, &[b' '; 8], &[b' '; 8]);
        }
        d.your_class = classify(
            s,
            &d.source,
            &mut d.your,
            d.mode,
            1,
            reads,
            &mut d.source_class,
        )?;
        d.rebuild(s, reads)?;
        d.display(false);
        Ok(d)
    }

    fn classify(&mut self, s: &Sources<'_>, reads: &ReadInputs<'_>) -> Result<(), Error> {
        self.your_class = classify(
            s,
            &self.source,
            &mut self.your,
            self.mode,
            1,
            reads,
            &mut self.source_class,
        )?;
        Ok(())
    }
    fn display(&mut self, fold: bool) {
        self.preview = std::array::from_fn(|i| {
            let field = [
                MemoryCallsign::Your,
                MemoryCallsign::Repeater1,
                MemoryCallsign::Repeater2,
            ][i];
            let mut bytes = *self.holder.callsign(field);
            if i == 0 && fold {
                for b in &mut bytes {
                    *b = b.to_ascii_uppercase();
                }
                if bytes.iter().any(|b| {
                    !(b.is_ascii_uppercase() || b.is_ascii_digit() || matches!(b, b'/' | b' '))
                }) {
                    bytes.fill(b' ');
                }
            }
            prefix(&bytes).to_vec()
        });
    }
    fn update(&mut self, s: &Sources<'_>, reads: &ReadInputs<'_>, fold: bool) -> Result<(), Error> {
        if preview_allowed(self.source_class, self.your_class) {
            apply_text(
                s,
                &mut self.holder,
                &self.other,
                &self.source,
                &self.your,
                &self.repeater_text,
                self.repeater_flag,
                self.mode,
                reads,
            )?;
        } else {
            copy_calls(&mut self.holder, &[b' '; 8], &[b' '; 8], &[b' '; 8]);
        }
        self.display(fold);
        Ok(())
    }
    fn rebuild(&mut self, s: &Sources<'_>, reads: &ReadInputs<'_>) -> Result<(), Error> {
        self.candidates.clear();
        self.selected = -1;
        self.end_marker = -1;
        let route = match (self.source_class, self.your_class) {
            (2, 6) => 2,
            (3, 6) => 3,
            (_, 8) => 1,
            _ => 0,
        };
        self.repeater_enabled = matches!(route, 2 | 3);
        if route == 0 {
            return self.designation_caption(s, reads);
        }
        if route == 1 {
            self.candidates.push(rpt_token(s.language, true).to_vec());
            self.selected = 0;
            self.end_marker = 0;
        } else {
            if self.repeater_flag == 0 {
                self.repeater_flag = 3;
            }
            self.candidates.push(rpt_token(s.language, false).to_vec());
            self.first_marker = Some(0);
            let mut source = lookup_repeater_name(s, &self.source, self.mode)?;
            if source >= 2500 {
                source = lookup_call(s, &self.source[..8], self.mode)?;
            }
            if source < 2500 {
                let a = repeater(s, source)?;
                for row in 0..total(s)? {
                    let h = repeater(s, row)?;
                    if !eligible(&h, self.mode) {
                        continue;
                    }
                    if h.repeater_text(T::Call) == a.repeater_text(T::Call)
                        || (!blank(h.repeater_text(T::Gateway))
                            && h.repeater_text(T::Gateway) == a.repeater_text(T::Gateway))
                    {
                        let text = row_label(&h);
                        if text == self.repeater_text {
                            self.selected = self.candidates.len() as i32;
                        }
                        self.candidates.push(text);
                    }
                }
            }
            if self.repeater_flag == 1 {
                self.selected = self.first_marker_value()?;
            }
            if route == 3 {
                self.end_marker = self.candidates.len() as i32;
                self.candidates.push(rpt_token(s.language, true).to_vec());
                if self.repeater_flag == 3 {
                    self.selected = self.end_marker;
                }
            }
        }
        // C19A6..C19CC chooses an existing marker before C13CC. This is not
        // a host-side recovery from an invalid native selected getter.
        if self.selected < 0 {
            self.selected = if self.end_marker > 0 {
                self.end_marker
            } else {
                self.first_marker_value()?
            };
        }
        self.designation_caption(s, reads)?;
        Ok(())
    }
    fn designation_caption(
        &mut self,
        s: &Sources<'_>,
        reads: &ReadInputs<'_>,
    ) -> Result<(), Error> {
        self.repeater_text.clear();
        self.repeater_flag = if self.candidates.is_empty() {
            0
        } else if self.selected == self.first_marker_value()? {
            1
        } else if self.selected == self.end_marker {
            3
        } else {
            self.repeater_text = selected(&self.candidates, self.selected)?.clone();
            2
        };
        let caption = if self.candidates.is_empty() {
            Vec::new()
        } else {
            selected(&self.candidates, self.selected)?.clone()
        };
        self.set_caption(s, caption, reads)
    }
    fn set_caption(
        &mut self,
        s: &Sources<'_>,
        caption: Vec<u8>,
        reads: &ReadInputs<'_>,
    ) -> Result<(), Error> {
        if self.repeater_caption == caption {
            return Ok(());
        }
        self.repeater_caption = caption;
        self.classify(s, reads)?;
        self.update(s, reads, true)
    }
    fn first_marker_value(&self) -> Result<i32, Error> {
        self.first_marker.ok_or(invalid(
            "native initial first-marker heap value unavailable",
        ))
    }
    fn set_designation(
        &mut self,
        s: &Sources<'_>,
        selected_index: i32,
        reads: &ReadInputs<'_>,
    ) -> Result<(), Error> {
        self.selected = selected_index;
        self.designation_caption(s, reads)?;
        // The original count-zero path still performs this final selected read.
        let caption = selected(&self.candidates, self.selected)?.clone();
        self.set_caption(s, caption, reads)
    }
    pub fn action(
        &mut self,
        s: &Sources<'_>,
        action: Action<'_>,
        reads: &ReadInputs<'_>,
    ) -> Result<(), Error> {
        match action {
            Action::SourceText(text) | Action::UrText(text) => {
                let source = matches!(action, Action::SourceText(_));
                let value = padded(trim_right(text));
                if source {
                    self.source = value;
                } else {
                    self.your = value;
                }
                self.classify(s, reads)?;
                self.rebuild(s, reads)?;
                self.update(s, reads, matches!(self.your_class, 6 | 7))?;
            }
            Action::RepeaterText(text) => {
                self.repeater_caption = text.to_vec();
                self.classify(s, reads)?;
                self.update(s, reads, true)?;
            }
            Action::SourceList(selection) | Action::UrList(selection) => {
                let source = matches!(action, Action::SourceList(_));
                let list_mode = if source { 4 } else { 3 };
                self.last_list_mode = Some(list_mode);
                let model = super::list_model_with_source(
                    s,
                    list_mode,
                    self.mode,
                    &self.source,
                    selection,
                    reads,
                )?;
                self.checkbox = selection.checkbox;
                if selection.modal_result == 0 {
                    return Ok(());
                }
                let old_mode = self.mode;
                if let Some(value) = list_value::<16>(&model, selection, reads, s.language)? {
                    if source {
                        self.source = value;
                        self.mode = if (3..54).contains(&model.tag) {
                            repeater(s, selected(&model.names, selection.name)?.row)?
                                .repeater_field(R::Mode)
                        } else {
                            4
                        };
                    } else {
                        self.your = value;
                    }
                }
                if matches!((old_mode, self.mode), (2, 3) | (3, 2))
                    && !matches!(self.your_class, 5 | 6)
                {
                    self.your = padded(b"CQCQCQ");
                }
                self.classify(s, reads)?;
                if (self.source_class == 2 && matches!(self.your_class, 3 | 4 | 7))
                    || (self.source_class == 3 && self.your_class == 3)
                    || (self.source_class == 0 && !matches!(self.your_class, 5 | 6))
                {
                    self.your = padded(b"CQCQCQ");
                    self.classify(s, reads)?;
                }
                if !source || self.selected == self.first_marker_value()? {
                    self.repeater_flag = if self.candidates.is_empty() { 0 } else { 3 };
                    if self.repeater_flag == 0 {
                        self.repeater_text.clear();
                    }
                }
                self.rebuild(s, reads)?;
                self.update(s, reads, true)?;
            }
            Action::Designation { selected, accept } => {
                self.last_list_mode = Some(5);
                if accept {
                    self.set_designation(s, selected, reads)?;
                }
            }
            Action::Mode(mode) => {
                self.mode = mode;
                self.classify(s, reads)?;
                self.rebuild(s, reads)?;
                self.update(s, reads, true)?;
            }
        }
        Ok(())
    }
    /// Outer Cancel skips only the clone copy. The caller must still run its
    /// existing save/reload path after the metadata prefix written by open.
    pub fn finish(self, original: &mut DecodedRecord, modal_result: u32) -> DialogOutcome {
        let accepted = modal_result == 1;
        if accepted {
            *original = self.holder;
        }
        DialogOutcome {
            accepted,
            save: true,
            checkbox: self.checkbox,
        }
    }

    pub fn list_model(
        &self,
        s: &Sources<'_>,
        source: bool,
        selection: ListSelection,
        reads: &ReadInputs<'_>,
    ) -> Result<ListModel, Error> {
        list_model_with_source(
            s,
            if source { 4 } else { 3 },
            self.mode,
            &self.source,
            selection,
            reads,
        )
    }
    pub fn initial_selection(
        &self,
        s: &Sources<'_>,
        source: bool,
        reads: &ReadInputs<'_>,
    ) -> Result<ListSelection, Error> {
        initial_selection_with_source(
            s,
            &self.holder,
            if source { 4 } else { 3 },
            self.mode,
            if source { &self.source } else { &self.your },
            self.other.mode(),
            &self.source,
            reads,
        )
    }
}
