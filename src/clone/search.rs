//! Pure RadioSch/COM contracts. No DLL, registry, device or audio API is called.
//! Native observations and resource filters are explicit inputs. These bytes
//! are ANSI/DBCS, not UTF-8: unprovided locale-dependent behavior is rejected
//! by ASCII-only entry points rather than silently transliterated.

use super::Error;
use std::collections::BTreeMap;

pub const EXPORTS: [&str; 4] = ["OpenDll", "CloseDll", "SearchUsbDev", "GetUsbDev"];
pub const RECORD_BYTES: usize = 12 + 8 * 68;
pub const PORTS_GUID: &str = "4D36E978-E325-11CE-BFC1-08002BE10318";
pub const USB_GUID: &str = "A5DCBF10-6530-11D2-901F-00C04FB951ED";
pub const PORTS_FLAGS: u32 = 0x0a;
pub const USB_FLAGS: u32 = 0x12;

fn ascii(s: &[u8]) -> Result<&[u8], Error> {
    if !s.is_ascii() || s.contains(&0) {
        Err(Error::Invalid("search requires bounded ASCII observation"))
    } else {
        Ok(s)
    }
}
fn contains(s: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || s.windows(needle.len()).any(|w| w == needle)
}
fn find(s: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        Some(0)
    } else {
        s.windows(needle.len()).position(|w| w == needle)
    }
}

/// 1809B4 numeric prefix: leading ASCII space/sign, decimal prefix, u32 wrap.
/// The manual commit alone applies the signed 1..256 gate.
pub fn manual_number(text: &[u8]) -> Result<i32, Error> {
    let text = ascii(text)?;
    let mut i = 0;
    while i < text.len() && text[i].is_ascii_whitespace() {
        i += 1;
    }
    let negative = text.get(i) == Some(&b'-');
    if matches!(text.get(i), Some(b'-' | b'+')) {
        i += 1;
    }
    let mut n = 0u32;
    while let Some(&b @ b'0'..=b'9') = text.get(i) {
        n = n.wrapping_mul(10).wrapping_add(u32::from(b - b'0'));
        i += 1;
    }
    Ok(if negative { n.wrapping_neg() } else { n } as i32)
}
pub fn manual_commit(text: &[u8]) -> Result<u16, Error> {
    let n = manual_number(text)?;
    if (1..=256).contains(&n) {
        Ok(n as u16)
    } else {
        Err(Error::Invalid("manual COM outside 1..=256"))
    }
}

/// 20304 uses only three bytes following COM=, trims, then StrToIntDef(-1).
/// This is not the manual prefix parser. Startup accepts anything except -1;
/// interactive search writes text only for values >0.
pub fn label_number(label: &[u8]) -> Result<i32, Error> {
    let label = ascii(label)?;
    let Some(p) = find(label, b"COM=") else {
        return Ok(-1);
    };
    let s = &label[p + 4..(p + 7).min(label.len())];
    let start = s
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map_or(start, |i| i + 1);
    let s = &s[start..end];
    let mut i = 0;
    let negative = s.first() == Some(&b'-');
    if matches!(s.first(), Some(b'-' | b'+')) {
        i = 1;
    }
    let rest = &s[i..];
    let (radix, skip) = if rest.starts_with(b"0x") || rest.starts_with(b"0X") {
        (16, 2)
    } else if matches!(rest.first(), Some(b'$' | b'x' | b'X')) {
        (16, 1)
    } else {
        (10, 0)
    };
    i += skip;
    if i == s.len() {
        return Ok(-1);
    }
    let Ok(text) = std::str::from_utf8(&s[i..]) else {
        return Ok(-1);
    };
    let Ok(n) = u32::from_str_radix(text, radix) else {
        return Ok(-1);
    };
    Ok(if negative { n.wrapping_neg() } else { n } as i32)
}

/// COM MI is decimal exactly two digits, position >0. INI removes from &MI_
/// even for invalid digits; no &MI_ means Delete(-1,...) clears all COM text.
pub fn mi_index(s: &[u8]) -> i32 {
    if let Some(p) = find(s, b"MI_").filter(|&p| p > 0)
        && let Some(d) = s
            .get(p + 3..p + 5)
            .filter(|d| d.iter().all(u8::is_ascii_digit))
    {
        return i32::from(d[0] - b'0') * 10 + i32::from(d[1] - b'0');
    }
    -1
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub hub: Vec<u8>,
    pub audio: Vec<u8>,
    pub com: Vec<u8>,
    pub mi: i32,
    pub id: Vec<u8>,
    pub cid: Vec<u8>,
}
impl Filter {
    pub fn from_ini(
        hub: &[u8],
        audio: &[u8],
        com: &[u8],
        id: &[u8],
        cid: &[u8],
    ) -> Result<Self, Error> {
        for s in [hub, audio, com, id, cid] {
            ascii(s)?;
        }
        let mi = mi_index(com);
        let mut c = com.to_vec();
        if find(com, b"MI_").is_some_and(|p| p > 0 && com.len() >= p + 5) {
            c.truncate(find(com, b"&MI_").unwrap_or(0));
        }
        Ok(Self {
            hub: hub.to_vec(),
            audio: audio.to_vec(),
            com: c,
            mi,
            id: id.to_ascii_uppercase(),
            cid: cid.to_ascii_uppercase(),
        })
    }
}

pub fn com_matches(filters: &[Filter], p: &[u8], index: i32) -> bool {
    filters
        .iter()
        .any(|f| f.com == b"*" || (contains(p, &f.com) && (f.mi == -1 || f.mi == index)))
}
pub fn com_only(filters: &[Filter], p: &[u8]) -> bool {
    filters.iter().any(|f| f.com == b"*" || contains(p, &f.com))
}
fn field_matches(pattern: &[u8], value: &[u8]) -> bool {
    if pattern.is_empty() {
        value.is_empty()
    } else {
        pattern == b"*" || contains(value, pattern)
    }
}
/// Separate existential COM/HUB/AUDIO gates; matching entries need not be the
/// same INI section. Empty HUB/AUDIO match only empty observed values.
pub fn device_matches(filters: &[Filter], com: &[u8], hub: &[u8], audio: &[u8]) -> bool {
    com_only(filters, com)
        && filters.iter().any(|f| field_matches(&f.hub, hub))
        && filters.iter().any(|f| field_matches(&f.audio, audio))
}
pub fn marker(filters: &[Filter], com: &[u8]) -> Option<u8> {
    let matched = filters
        .iter()
        .find(|f| f.com == b"*" || contains(com, &f.com))?;
    matched
        .id
        .first()
        .copied()
        .or_else(|| filters.first()?.cid.first().copied())
}

/// 6BB0 prefix calculation: find FIRST occurrence of final component, not the
/// last separator. None means the Ports producer keeps its previous P; child
/// query instead uses an explicitly cleared destination.
pub fn instance_prefix(id: &[u8]) -> Option<Vec<u8>> {
    let start = id.iter().rposition(|&b| b == b'\\').map_or(0, |i| i + 1);
    let q = find(id, &id[start..])?;
    (q > 0).then(|| id[..q - 1].to_vec())
}
pub fn node_key(suffix: &[u8], full_id: &[u8], parent: Option<&[u8]>) -> Vec<u8> {
    let mut k = if suffix.is_empty() {
        full_id.to_vec()
    } else {
        suffix.to_vec()
    };
    if !suffix.is_empty() && k.len() >= 2 {
        let end = &k[k.len() - 2..];
        if end.contains(&b'_') && end[1].is_ascii_alphabetic() {
            k.truncate(k.len() - 2);
        }
    }
    if let Some(p) = parent {
        k.push(b'_');
        k.extend_from_slice(p);
    }
    k
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    pub instance_id: Vec<u8>,
    pub parent_id: Vec<u8>,
    pub prefix: Vec<u8>,
    pub mi: i32,
    pub number: u32,
    pub description: Vec<u8>,
}
/// Full exact instance matches suppress the entire parent-match branch.
pub fn join_ports<'a>(id: &[u8], ports: &'a [Port]) -> (bool, Vec<&'a Port>) {
    let direct: Vec<_> = ports.iter().filter(|p| p.instance_id == id).collect();
    if !direct.is_empty() {
        (false, direct)
    } else {
        (true, ports.iter().filter(|p| p.parent_id == id).collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub number: u32,
    pub name: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRecord {
    pub wave_in: i32,
    pub wave_out: i32,
    pub slots: Vec<Slot>,
}
impl SearchRecord {
    pub fn to_bytes(&self) -> Result<[u8; RECORD_BYTES], Error> {
        if self.slots.len() > 8 {
            return Err(Error::Invalid("search slots exceed eight"));
        }
        let mut out = [0; RECORD_BYTES];
        out[..4].copy_from_slice(&self.wave_in.to_le_bytes());
        out[4..8].copy_from_slice(&self.wave_out.to_le_bytes());
        out[8..12].copy_from_slice(&(self.slots.len() as u32).to_le_bytes());
        for (i, s) in self.slots.iter().enumerate() {
            let a = 12 + i * 68;
            out[a..a + 4].copy_from_slice(&s.number.to_le_bytes());
            let n = s
                .name
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(s.name.len())
                .min(63);
            out[a + 4..a + 4 + n].copy_from_slice(&s.name[..n]);
        }
        Ok(out)
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != RECORD_BYTES {
            return Err(Error::Invalid("search record length"));
        }
        let n = u32::from_le_bytes(bytes[8..12].try_into().expect("fixed length")) as usize;
        if n > 8 {
            return Err(Error::Invalid("search slot count"));
        }
        let mut slots = Vec::new();
        for i in 0..n {
            let a = 12 + i * 68;
            let name = &bytes[a + 4..a + 68];
            let end = name
                .iter()
                .position(|&b| b == 0)
                .ok_or(Error::Invalid("search name lacks NUL"))?;
            slots.push(Slot {
                number: u32::from_le_bytes(bytes[a..a + 4].try_into().expect("fixed slot")),
                name: name[..end].to_vec(),
            });
        }
        Ok(Self {
            wave_in: i32::from_le_bytes(bytes[..4].try_into().expect("fixed header")),
            wave_out: i32::from_le_bytes(bytes[4..8].try_into().expect("fixed header")),
            slots,
        })
    }
}

/// 5DE0 preserves the second ordinal on second-list miss and both on first
/// miss. Empty WINMM slots are retained, so ordinal != nonempty-item rank.
pub fn header_query(
    record: &mut SearchRecord,
    prefix: &[u8],
    parent_prefix: &[u8],
    wave_in: &[Vec<u8>],
    wave_out: &[Vec<u8>],
) -> bool {
    let normalize = |s: &[u8]| {
        s.iter()
            .map(|b| {
                if *b == b'\\' {
                    b'#'
                } else {
                    b.to_ascii_uppercase()
                }
            })
            .collect::<Vec<_>>()
    };
    let target = normalize(&[prefix, b"&MI_00\\", parent_prefix].concat());
    let Some(a) = wave_in
        .iter()
        .position(|s| contains(&normalize(s), &target))
    else {
        return false;
    };
    record.wave_in = a as i32;
    if let Some(b) = wave_out
        .iter()
        .position(|s| contains(&normalize(s), &target))
    {
        record.wave_out = b as i32;
    }
    true
}

/// Explicit observations after native registry/child queries. This keeps
/// failed native calls, P0/B0 carry and readable NUL windows out of invented
/// defaults. No observation is obtained by this library.
#[derive(Debug, Clone)]
pub struct Observation {
    pub key: Vec<u8>,
    pub base: Vec<u8>,
    pub com: Vec<u8>,
    pub hub: Vec<u8>,
    pub audio: Vec<u8>,
    pub parent_path: bool,
    pub ports: Vec<Port>,
    pub wave_in: i32,
    pub wave_out: i32,
}

#[derive(Debug, Default)]
pub struct SearchContext {
    pub results: Vec<SearchRecord>,
}
impl SearchContext {
    /// Source Search clears old results BEFORE consuming observations. On an
    /// invalid observation, already-published new groups remain, never old data.
    /// Groups use key order, first metadata wins, duplicate Ports are appended.
    pub fn search(
        &mut self,
        filters: &[Filter],
        observations: &[Observation],
    ) -> Result<usize, Error> {
        self.results.clear();
        let mut groups: BTreeMap<Vec<u8>, Observation> = BTreeMap::new();
        for item in observations {
            if let Some(g) = groups.get_mut(&item.key) {
                g.ports.extend_from_slice(&item.ports);
            } else {
                groups.insert(item.key.clone(), item.clone());
            }
        }
        for (_, mut g) in groups {
            for s in [&g.base, &g.com, &g.hub, &g.audio] {
                ascii(s)?;
            }
            if !device_matches(filters, &g.com, &g.hub, &g.audio) {
                continue;
            }
            let mut mark = if g.parent_path {
                marker(filters, &g.com)
            } else {
                None
            };
            if mark.is_some() {
                g.ports.sort_by_key(|p| p.mi);
            }
            let mut slots = Vec::new();
            for p in g.ports.iter().take(8) {
                ascii(&p.description)?;
                let name = if let Some(m) = mark {
                    let mut name = g.base.clone();
                    name.push(b' ');
                    name.push(m);
                    mark = Some(m.wrapping_add(1));
                    name
                } else {
                    p.description.clone()
                };
                let name = name
                    .into_iter()
                    .map(|b| if b == b'_' { b' ' } else { b })
                    .collect();
                slots.push(Slot {
                    number: p.number,
                    name,
                });
            }
            // 9D10: signed last-byte stable bubble sort; empty names are
            // barriers. Sort BEFORE 63-byte public truncation.
            for i in 0..slots.len() {
                for j in (i + 1..slots.len()).rev() {
                    if let (Some(&a), Some(&b)) = (slots[j - 1].name.last(), slots[j].name.last())
                        && (b as i8) < (a as i8)
                    {
                        slots.swap(j - 1, j);
                    }
                }
            }
            if !slots.is_empty() {
                self.results.push(SearchRecord {
                    wave_in: g.wave_in,
                    wave_out: g.wave_out,
                    slots,
                });
            }
        }
        Ok(self.results.len())
    }
    /// Invalid index preserves the caller's old output, just like GetUsbDev.
    pub fn get(&self, index: usize, out: &mut [u8; RECORD_BYTES]) -> Result<bool, Error> {
        let Some(record) = self.results.get(index) else {
            return Ok(false);
        };
        *out = record.to_bytes()?;
        Ok(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub label: Vec<u8>,
    pub number: u32,
}
/// EXE consumes ONLY each group's first public slot. Prefix exact + final byte
/// equal to uppercased suffix FIRST byte, not general suffix matching.
pub fn candidates(
    records: &[SearchRecord],
    prefix: &[u8],
    suffixes: &[Vec<u8>],
    previous: u32,
) -> Result<(Vec<Candidate>, Option<usize>), Error> {
    ascii(prefix)?;
    let mut out = Vec::new();
    let mut selected = None;
    for record in records {
        let Some(slot) = record.slots.first() else {
            continue;
        };
        let name = &slot.name[..slot.name.len().min(63)];
        if !name.starts_with(prefix) {
            continue;
        }
        let mut accepted = false;
        for suffix in suffixes {
            ascii(suffix)?;
            if suffix == b"#" {
                break;
            }
            if let (Some(last), Some(first)) = (name.last(), suffix.first()) {
                accepted |= *last == first.to_ascii_uppercase();
            }
        }
        if !accepted {
            continue;
        }
        let mut label = name.to_vec();
        label.extend_from_slice(format!("  COM={}", slot.number as i32).as_bytes());
        if selected.is_none() || slot.number == previous {
            selected = Some(out.len());
        }
        out.push(Candidate {
            label,
            number: slot.number,
        });
    }
    Ok((out, selected))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_numeric_policies_and_filter_domains() {
        for (s, n) in [
            ("12tail", 12),
            ("1e2", 1),
            ("256.9", 256),
            ("4294967297", 1),
            ("-1", -1),
        ] {
            assert_eq!(manual_number(s.as_bytes()).unwrap(), n);
        }
        assert!(manual_commit(b"257").is_err());
        assert_eq!(manual_commit(b"256.9").unwrap(), 256);
        assert_eq!(label_number(b"X COM=1000").unwrap(), 100);
        assert_eq!(label_number(b"X COM=$FF").unwrap(), 255);
        assert_eq!(label_number(b"X COM=1x").unwrap(), -1);
        let f = Filter::from_ini(b"", b"", b"USB&MI_0A", b"ab", b"xy").unwrap();
        assert_eq!((&f.com[..], f.mi, &f.id[..]), (&b"USB"[..], -1, &b"AB"[..]));
        let filters = vec![
            Filter::from_ini(b"", b"", b"C", b"", b"A").unwrap(),
            Filter::from_ini(b"H", b"", b"no", b"", b"B").unwrap(),
            Filter::from_ini(b"", b"A", b"no", b"", b"C").unwrap(),
        ];
        assert!(device_matches(&filters, b"COM", b"HUB", b"AUDIO"));
        assert_eq!(marker(&filters, b"COM"), Some(b'A'));
        assert_eq!(
            instance_prefix(b"USB\\X&MI_09\\TAIL"),
            Some(b"USB\\X&MI_09".to_vec())
        );
        assert_eq!(instance_prefix(b"NOSLASH"), None);
        assert_eq!(node_key(b"SER_a", b"id", Some(b"P")), b"SER_P");
    }
    #[test]
    fn literal_public_layout_last_selection_and_failed_get_preservation() {
        let r = SearchRecord {
            wave_in: -1,
            wave_out: 2,
            slots: vec![Slot {
                number: 256,
                name: b"X A".to_vec(),
            }],
        };
        let bytes = r.to_bytes().unwrap();
        assert_eq!(
            &bytes[..19],
            &[
                255, 255, 255, 255, 2, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, b'X', b' ', b'A'
            ]
        );
        assert_eq!(SearchRecord::from_bytes(&bytes).unwrap(), r);
        let c = SearchContext::default();
        let mut out = [0xa5; RECORD_BYTES];
        assert!(!c.get(0, &mut out).unwrap());
        assert_eq!(out, [0xa5; RECORD_BYTES]);
        let (c, s) = candidates(
            &[r.clone(), r],
            b"X",
            &[b"alphabet".to_vec(), b"#".to_vec()],
            256,
        )
        .unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(s, Some(1));
        let mut r = SearchRecord {
            wave_in: 9,
            wave_out: 7,
            slots: vec![],
        };
        assert!(header_query(
            &mut r,
            b"USB",
            b"P",
            &[vec![], b"usb&mi_00#p".to_vec()],
            &[]
        ));
        assert_eq!((r.wave_in, r.wave_out), (1, 7));
    }
    #[test]
    fn group_first_metadata_key_order_duplicates_slot_limit_and_truncation() {
        let port = |number, mi, name: &[u8]| Port {
            instance_id: b"D".to_vec(),
            parent_id: b"P".to_vec(),
            prefix: b"C".to_vec(),
            mi,
            number,
            description: name.to_vec(),
        };
        let ports = vec![port(9, 9, b"Z"), port(1, 1, b"A")];
        assert_eq!(join_ports(b"D", &ports).1.len(), 2);
        assert!(!join_ports(b"D", &ports).0);
        assert!(join_ports(b"P", &ports).0);
        let filters = vec![Filter::from_ini(b"*", b"*", b"*", b"Z", b"Q").unwrap()];
        let observation = |key: &[u8], base: &[u8], wave_in, ports| Observation {
            key: key.to_vec(),
            base: base.to_vec(),
            com: b"C".to_vec(),
            hub: vec![],
            audio: vec![],
            parent_path: true,
            ports,
            wave_in,
            wave_out: -1,
        };
        let mut c = SearchContext::default();
        let a = observation(b"b", b"SECOND", 8, vec![ports[0].clone()]);
        let b = observation(b"a", &[b'X'; 63], 3, vec![ports[1].clone(); 9]);
        let mut replacement = observation(b"a", b"WRONG", 99, vec![ports[0].clone()]);
        replacement.parent_path = false;
        assert_eq!(c.search(&filters, &[a, b, replacement]).unwrap(), 2);
        assert_eq!(c.results[0].wave_in, 3);
        assert_eq!(c.results[0].slots.len(), 8);
        assert!(c.results[0].slots.iter().all(|s| s.number == 1)); // First eight, no dedup.
        let public = SearchRecord::from_bytes(&c.results[0].to_bytes().unwrap()).unwrap();
        assert!(public.slots.iter().all(|s| s.name == [b'X'; 63]));
        let (candidate, _) =
            candidates(&[public], b"X", &[b"Z".to_vec(), b"#".to_vec()], 1).unwrap();
        assert!(candidate.is_empty()); // Internal marker sorted, then truncated before EXE filter.
        assert_eq!(c.search(&filters, &[]).unwrap(), 0); // Search clears old results.
    }
}
