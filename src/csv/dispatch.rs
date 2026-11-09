//! 94AAC/4011F0 dispatch and header-cache binding. Runtime context type is not
//! a stored record type. Unknown contexts select initial slot0 in the original;
//! this explicit rule does not prove that every context has a registered UI.

use super::{Error, column, find_column_with, payload::Kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    Memory,
    FixedPair,
    AlternatingPair,
    Gps,
    Repeater,
    Callsign,
    Scan,
    UserEdge,
}
/// Header/row callback slot, not a claim that a given field codec is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dispatch {
    pub slot: u8,
    pub export: Kind,
    pub import: Option<ImportKind>,
    pub skip_invalid_export: bool,
}

pub fn for_context(context_type: u32) -> Dispatch {
    let (slot, export, import, skip) = match context_type {
        3 => (1, Kind::Memory, Some(ImportKind::FixedPair), true),
        6 => (2, Kind::Memory, Some(ImportKind::AlternatingPair), true),
        9 | 10 => (3, Kind::Gps, Some(ImportKind::Gps), false),
        11 | 12 => (4, Kind::Repeater, Some(ImportKind::Repeater), false),
        13 => (5, Kind::Callsign, Some(ImportKind::Callsign), false),
        8 => (6, Kind::Scan, Some(ImportKind::Scan), false),
        14 => (7, Kind::ReceiveHistory, None, true),
        15 => (8, Kind::FromHistory, None, true),
        16 => (9, Kind::FromHistory, None, true),
        17 => (10, Kind::FromDataHistory, None, true),
        18 => (11, Kind::ToHistory, None, true),
        19 => (12, Kind::ToHistory, None, true),
        20 => (13, Kind::Reflector, None, true),
        21 => (14, Kind::UserEdge, Some(ImportKind::UserEdge), false),
        _ => (0, Kind::Memory, Some(ImportKind::Memory), true),
    };
    Dispatch {
        slot,
        export,
        import,
        skip_invalid_export: skip,
    }
}

impl ImportKind {
    /// Required predicates of six known callbacks, not all columns. These labels
    /// are the accepted normal built-in resource sources; overlay labels may be
    /// supplied explicitly through Binding::new's required-index argument.
    pub fn required_labels(self) -> &'static [&'static [u8]] {
        match self {
            Self::Memory | Self::FixedPair | Self::AlternatingPair => &[b"Frequency"],
            Self::Scan | Self::UserEdge => &[b"Frequency Low", b"Frequency High"],
            Self::Gps => &[b"Latitude", b"Longitude"],
            Self::Repeater => &[b"Repeater Call Sign"],
            Self::Callsign => &[b"Call Sign"],
        }
    }
}

/// Header cache stays observable even when its required-column predicate fails.
/// Missing columns are None; a found column in a short data row is Some(empty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub positions: Vec<Option<usize>>,
    pub accepted: bool,
    pub separator: u8,
}
impl Binding {
    /// Labels are caller's actual resource bytes in callback lookup order.
    /// Every lookup re-runs native lower, preserving separate source generations.
    pub fn new(
        header: &[u8],
        separator: u8,
        labels: &[&[u8]],
        required: &[usize],
        mut lower: impl FnMut(&[u8]) -> Result<Vec<u8>, Error>,
    ) -> Result<Self, Error> {
        if labels.len() > 256 || required.iter().any(|i| *i >= labels.len()) {
            return Err(Error::Invalid("header label schema"));
        }
        let mut binding = Self {
            positions: vec![None; labels.len()],
            accepted: false,
            separator,
        };
        binding.rebind(header, labels, required, &mut lower)?;
        Ok(binding)
    }
    /// Update existing cache slots as each original894E0 call returns. A later
    /// native failure retains earlier slot writes and the previous predicate.
    pub fn rebind(
        &mut self,
        header: &[u8],
        labels: &[&[u8]],
        required: &[usize],
        mut lower: impl FnMut(&[u8]) -> Result<Vec<u8>, Error>,
    ) -> Result<bool, Error> {
        if labels.len() != self.positions.len() || required.iter().any(|i| *i >= labels.len()) {
            return Err(Error::Invalid("header rebind schema"));
        }
        for (position, label) in self.positions.iter_mut().zip(labels) {
            *position = find_column_with(header, label, self.separator, &mut lower)?;
        }
        self.accepted = required.iter().all(|i| self.positions[*i].is_some());
        Ok(self.accepted)
    }
    pub fn ascii(
        header: &[u8],
        separator: u8,
        labels: &[&[u8]],
        required: &[usize],
    ) -> Result<Self, Error> {
        Self::new(header, separator, labels, required, |text| {
            Ok(text.iter().map(u8::to_ascii_lowercase).collect())
        })
    }
    pub fn cell<'a>(&self, line: &'a [u8], field: usize) -> Result<Option<&'a [u8]>, Error> {
        let index = self
            .positions
            .get(field)
            .copied()
            .ok_or(Error::Invalid("header field index"))?;
        Ok(column(line, index, self.separator))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dispatch_and_header_presence_do_not_invent_import_or_row_validity() {
        assert_eq!(for_context(0xffff).slot, 0);
        assert_eq!(for_context(9), for_context(10));
        assert_eq!(for_context(11), for_context(12));
        assert_eq!(for_context(6).import, Some(ImportKind::AlternatingPair));
        for kind in 14..=20 {
            assert!(for_context(kind).skip_invalid_export);
            assert!(for_context(kind).import.is_none());
        }
        assert!(!for_context(21).skip_invalid_export);
        let labels: [&[u8]; 3] = [b"Latitude", b"Longitude", b"Name"];
        let binding = Binding::ascii(
            b" Name, Latitude,LATITUDE,Longitude",
            b',',
            &labels,
            &[0, 1],
        )
        .unwrap();
        assert!(binding.accepted);
        assert_eq!(binding.positions, [Some(1), Some(3), Some(0)]);
        assert_eq!(binding.cell(b"N,1", 1).unwrap(), Some(b"".as_slice()));
        let missing =
            Binding::ascii(b"\xef\xbb\xbfLatitude,Longitude", b',', &labels, &[0, 1]).unwrap();
        assert!(!missing.accepted);
        assert_eq!(missing.cell(b"1,2", 0).unwrap(), None);
        assert_eq!(
            ImportKind::Repeater.required_labels(),
            [b"Repeater Call Sign".as_slice()]
        );
        let mut changed = binding;
        let mut calls = 0;
        assert!(
            changed
                .rebind(b"Latitude", &labels, &[0, 1], |bytes| {
                    calls += 1;
                    if calls == 5 {
                        return Err(Error::Operation("injected header native failure"));
                    }
                    Ok(bytes.iter().map(u8::to_ascii_lowercase).collect())
                })
                .is_err()
        );
        assert_eq!(changed.positions, [Some(0), None, Some(0)]);
        assert!(changed.accepted);
    }
}
