//! Existing current6 GPS storage only; no allocation, GUI conversion or device I/O.

use super::{Error, Icf, RECORD_BYTES};

const RECORDS: usize = 0x32c19;
const ORDER: usize = 0x3c385;
const STARTS: usize = 0x3c5dd;
const NAMES: usize = 0x353f1;
const ALARM: usize = 0x3c622;

/// Raw coordinate precision is 0.001 minute; negative zero is preserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpsCoordinate {
    /// Whole degrees, at most 90 for latitude or 180 for longitude.
    pub degrees: u8,
    /// Fractional degree in thousandths of a minute, strictly below 60000.
    pub thousandths_of_minute: u16,
    /// South or west, including a negative zero magnitude.
    pub negative: bool,
}
/// A stored Gregorian date, not a local-time conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpsDate {
    /// Stored year, 2000..=2099.
    pub year: u16,
    /// Calendar month, 1..=12.
    pub month: u8,
    /// Existing day in the selected month.
    pub day: u8,
}
/// Stored UTC time components, without external offset conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpsTime {
    /// Hour, 0..=23.
    pub hour: u8,
    /// Minute, 0..=59.
    pub minute: u8,
    /// Second, 0..=59.
    pub second: u8,
}
/// Stored altitude units, not meters or rounded integer feet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpsAltitude {
    /// The explicit 0x7fffffff sentinel.
    Unknown,
    /// Signed tenths of a foot; getters retain old out-of-input-range values.
    TenthsOfFoot(i32),
}
/// Explicit global alarm selection, not the GUI's type-change defaulting action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpsAlarm {
    /// Disable the alarm.
    Off,
    /// Receive alarm.
    Receive,
    /// All memories (`None`) or row group 0..26, including empty groups.
    Group(Option<usize>),
    /// Zero-based index within a nonempty row group 0..26.
    Memory {
        /// Row group, not the type-2 wire group.
        group: usize,
        /// Index relative to the selected group, not a physical slot.
        index: usize,
    },
}
/// One of all 27 stored row groups, retaining intermediate and trailing empties.
pub struct GpsGroup<'a> {
    name: Option<&'a [u8]>,
    slots: Vec<usize>,
}
impl GpsGroup<'_> {
    /// Borrow the raw A..Z name; group zero has no name record.
    pub fn name_bytes(&self) -> Option<&[u8]> {
        self.name
    }
    /// Physical slots in saved logical order.
    pub fn physical_slots(&self) -> &[usize] {
        &self.slots
    }
}
fn unsupported(at: usize, reason: &'static str) -> Error {
    Error::Unsupported {
        line: 5 + at / RECORD_BYTES,
        reason,
    }
}
fn invalid(at: usize, reason: &'static str) -> Error {
    Error::Invalid {
        line: 5 + at / RECORD_BYTES,
        reason,
    }
}
fn argument(reason: &'static str) -> Error {
    Error::Unsupported { line: 5, reason }
}
fn le16(memory: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes([memory[at], memory[at + 1]]))
}
fn bcd(raw: u8, at: usize) -> Result<u8, Error> {
    if raw >> 4 > 9 || raw & 15 > 9 {
        return Err(unsupported(at, "GPS invalid packed BCD"));
    }
    Ok((raw >> 4) * 10 + (raw & 15))
}
fn encode_bcd(value: u8) -> u8 {
    ((value / 10) << 4) | (value % 10)
}
fn valid_date(value: GpsDate) -> bool {
    if !(2000..=2099).contains(&value.year) {
        return false;
    }
    let leap = value.year.is_multiple_of(4)
        && (!value.year.is_multiple_of(100) || value.year.is_multiple_of(400));
    let days = match value.month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    value.day > 0 && value.day <= days
}
fn valid_time(value: GpsTime) -> bool {
    value.hour < 24 && value.minute < 60 && value.second < 60
}
fn coordinate_bytes(value: GpsCoordinate, longitude: bool) -> Option<[u8; 4]> {
    let max = if longitude { 180 } else { 90 };
    if value.degrees > max
        || value.thousandths_of_minute >= 60000
        || (value.degrees == max && value.thousandths_of_minute != 0)
    {
        return None;
    }
    let fraction = value.thousandths_of_minute.to_be_bytes();
    let hemisphere = match (longitude, value.negative) {
        (false, false) => b'N',
        (false, true) => b'S',
        (true, false) => b'E',
        (true, true) => b'W',
    };
    Some([value.degrees, fraction[0], fraction[1], hemisphere])
}

impl Icf<'_> {
    // Order/count tables: RVA 0xc80cc/0xc793c; current6 load/save 0x69a3/0x7e3e.
    fn gps_order(&self) -> Result<Vec<usize>, Error> {
        let mut order = Vec::new();
        let mut seen = [false; 300];
        let mut ended = false;
        for row in 0..300 {
            let at = ORDER + 2 * row;
            let p = le16(&self.memory, at);
            if p == 65535 {
                ended = true;
                continue;
            }
            if ended || p >= 300 || seen[p] {
                return Err(unsupported(
                    at,
                    "GPS order is not a unique prefix with FFFF suffix",
                ));
            }
            seen[p] = true;
            order.push(p);
        }
        Ok(order)
    }
    fn gps_base(&self, p: usize) -> Result<Option<usize>, Error> {
        if p >= 300 {
            return Err(argument("GPS physical slot outside 0..300"));
        }
        Ok(self.gps_order()?.contains(&p).then_some(RECORDS + 34 * p))
    }
    fn gps_target(&self, p: usize) -> Result<usize, Error> {
        self.gps_base(p)?
            .ok_or_else(|| unsupported(ORDER, "GPS physical slot is unused"))
    }

    // Group tables/count: RVA 0xc8100/0xc842c; named-group handler 0x6be5a.
    /// Read all 27 groups with strict order/start validation. No layout inference.
    /// # Errors
    /// Rejects duplicate/order holes, non-FFFF suffix, missing/unsorted/out-of-prefix starts.
    pub fn gps_groups(&self) -> Result<Vec<GpsGroup<'_>>, Error> {
        let order = self.gps_order()?;
        let mut starts = [None; 27];
        let mut previous = None;
        for (g, start) in starts.iter_mut().enumerate() {
            let at = STARTS + 2 * g;
            let raw = le16(&self.memory, at);
            if raw == 65535 {
                continue;
            }
            if raw >= order.len()
                || previous.is_some_and(|p| raw <= p)
                || (previous.is_none() && raw != 0)
            {
                return Err(unsupported(
                    at,
                    "GPS group starts outside strict increasing prefix",
                ));
            }
            *start = Some(raw);
            previous = Some(raw);
        }
        if !order.is_empty() && previous.is_none() {
            return Err(unsupported(STARTS, "GPS nonempty order has no group start"));
        }
        let mut groups = Vec::with_capacity(27);
        for g in 0..27 {
            let slots = match starts[g] {
                Some(start) => {
                    let end = starts[g + 1..]
                        .iter()
                        .flatten()
                        .next()
                        .copied()
                        .unwrap_or(order.len());
                    order[start..end].to_vec()
                }
                None => Vec::new(),
            };
            let name = if g == 0 {
                None
            } else {
                Some(&self.memory[NAMES + 16 * (g - 1)..NAMES + 16 * g])
            };
            groups.push(GpsGroup { name, slots });
        }
        Ok(groups)
    }
    /// Replace only a fixed A..Z raw name (group 1..26), even for an empty group.
    /// Topology is deliberately not decoded. No GUI character normalization occurs.
    /// # Errors
    /// Rejects group zero/out-of-range or non-printable/overlength new ASCII.
    pub fn set_gps_group_name_ascii(&mut self, group: usize, value: &str) -> Result<(), Error> {
        if !(1..=26).contains(&group) {
            return Err(argument("GPS named group outside 1..=26"));
        }
        let at = NAMES + 16 * (group - 1);
        self.gps_ascii(at, value)
    }
    fn gps_ascii(&mut self, at: usize, value: &str) -> Result<(), Error> {
        if value.len() > 16 || !value.bytes().all(|b| (32..=126).contains(&b)) {
            return Err(invalid(
                at,
                "GPS name requires at most 16 printable ASCII bytes",
            ));
        }
        let mut bytes = [b' '; 16];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        self.memory[at..at + 16].copy_from_slice(&bytes);
        Ok(())
    }

    // Record table/load/store: RVA 0x69bf..0x6a80 / 0x7e48..0x7ee0, stride 34.
    /// Borrow all 34 raw bytes for an occupied physical slot; unused returns None.
    /// Occupancy depends only on strict order, never names/zero/FF or group starts.
    /// # Errors
    /// Rejects out-of-range slots or inconsistent order before record arithmetic.
    pub fn physical_gps_record_raw(&self, p: usize) -> Result<Option<&[u8]>, Error> {
        Ok(self.gps_base(p)?.map(|at| &self.memory[at..at + 34]))
    }
    // Key 0x1b5: load RVA 0x69bf; store 0x7e48..0x7e68 / 0x6ba1d -> 0x47fd0.
    /// Borrow all 16 raw name bytes of an occupied record, without decoding/trim.
    /// # Errors
    /// Rejects invalid slot/order; unused slots return None.
    pub fn physical_gps_name_raw(&self, p: usize) -> Result<Option<&[u8]>, Error> {
        Ok(self.gps_base(p)?.map(|at| &self.memory[at + 18..at + 34]))
    }
    /// Store at most 16 printable ASCII bytes with space padding. No allocation.
    /// # Errors
    /// Rejects invalid/unused slots, order or new text; unknown current names may be replaced.
    pub fn set_physical_gps_name_ascii(&mut self, p: usize, value: &str) -> Result<(), Error> {
        let at = self.gps_target(p)?;
        self.gps_ascii(at + 18, value)
    }

    // Key 0x1b6: load 0x69e3..0x6a01; store 0x7e6b..0x7e82; BCD parse 0x9e028.
    /// Read real Gregorian dates across 2000..2099, including 2000-01-01.
    /// This raw getter's valid dates are wider than the canonical new-value range.
    /// # Errors
    /// Rejects invalid BCD/nonexistent days, slot or order. Unused returns None.
    pub fn physical_gps_date(&self, p: usize) -> Result<Option<GpsDate>, Error> {
        let Some(base) = self.gps_base(p)? else {
            return Ok(None);
        };
        let at = base + 8;
        let value = GpsDate {
            day: bcd(self.memory[at], at)?,
            month: bcd(self.memory[at + 1], at)?,
            year: 2000 + u16::from(bcd(self.memory[at + 2], at)?),
        };
        if !valid_date(value) {
            return Err(unsupported(at, "GPS invalid Gregorian date"));
        }
        Ok(Some(value))
    }
    /// Store a real date in 2000-01-02..2099-12-30, without changing time or UTC.
    /// # Errors
    /// Rejects new noncanonical dates or unavailable slots/order; unknown current dates may be replaced.
    pub fn set_physical_gps_date(&mut self, p: usize, value: GpsDate) -> Result<(), Error> {
        let at = self.gps_target(p)? + 8;
        if !valid_date(value)
            || (value.year == 2000 && value.month == 1 && value.day == 1)
            || (value.year == 2099 && value.month == 12 && value.day == 31)
        {
            return Err(invalid(at, "GPS new date outside canonical range"));
        }
        self.memory[at..at + 3].copy_from_slice(&[
            encode_bcd(value.day),
            encode_bcd(value.month),
            encode_bcd((value.year - 2000) as u8),
        ]);
        Ok(())
    }
    // Key 0x1b7: load 0x6a01..0x6a24; store 0x7e85..0x7e9c; BCD parse 0x9e442.
    /// Read stored UTC BCD time, without external UTC offset or companion date conversion.
    /// # Errors
    /// Rejects invalid BCD/time, slot/order. Unused returns None.
    pub fn physical_gps_time(&self, p: usize) -> Result<Option<GpsTime>, Error> {
        let Some(base) = self.gps_base(p)? else {
            return Ok(None);
        };
        let at = base + 11;
        let value = GpsTime {
            hour: bcd(self.memory[at], at)?,
            minute: bcd(self.memory[at + 1], at)?,
            second: bcd(self.memory[at + 2], at)?,
        };
        if !valid_time(value) {
            return Err(unsupported(at, "GPS invalid stored time"));
        }
        Ok(Some(value))
    }
    /// Store UTC time only; do not complement/repair date or convert local time.
    /// # Errors
    /// Rejects invalid new time or slot/order, without decoding the current value.
    pub fn set_physical_gps_time(&mut self, p: usize, value: GpsTime) -> Result<(), Error> {
        let at = self.gps_target(p)? + 11;
        if !valid_time(value) {
            return Err(invalid(at, "GPS invalid new time"));
        }
        self.memory[at..at + 3].copy_from_slice(&[
            encode_bcd(value.hour),
            encode_bcd(value.minute),
            encode_bcd(value.second),
        ]);
        Ok(())
    }

    fn gps_coordinate(&self, p: usize, longitude: bool) -> Result<Option<GpsCoordinate>, Error> {
        let Some(base) = self.gps_base(p)? else {
            return Ok(None);
        };
        let at = base + if longitude { 4 } else { 0 };
        let negative = match (longitude, self.memory[at + 3]) {
            (false, b'N') | (true, b'E') => false,
            (false, b'S') | (true, b'W') => true,
            _ => return Err(unsupported(at, "GPS unknown hemisphere")),
        };
        let value = GpsCoordinate {
            degrees: self.memory[at],
            thousandths_of_minute: u16::from_be_bytes([self.memory[at + 1], self.memory[at + 2]]),
            negative,
        };
        if coordinate_bytes(value, longitude).is_none() {
            return Err(unsupported(at, "GPS coordinate outside stored range"));
        }
        Ok(Some(value))
    }
    fn set_gps_coordinate(
        &mut self,
        p: usize,
        value: GpsCoordinate,
        longitude: bool,
    ) -> Result<(), Error> {
        let at = self.gps_target(p)? + if longitude { 4 } else { 0 };
        let bytes = coordinate_bytes(value, longitude)
            .ok_or_else(|| invalid(at, "GPS new coordinate outside range"))?;
        self.memory[at..at + 4].copy_from_slice(&bytes);
        Ok(())
    }
    // Keys 0x1b8/0x1b9: load 0x6a29..0x6a69; store 0x7e9f..0x7ecd.
    /// Read latitude components, preserving S negative zero and raw 0.001-minute ticks.
    /// # Errors
    /// Rejects unknown coordinates/slot/order; unused returns None.
    pub fn physical_gps_latitude(&self, p: usize) -> Result<Option<GpsCoordinate>, Error> {
        self.gps_coordinate(p, false)
    }
    /// Store latitude only; no GUI 10/6-tick grid or rounding.
    /// # Errors
    /// Rejects invalid new components or unavailable slots/order before any store.
    pub fn set_physical_gps_latitude(
        &mut self,
        p: usize,
        value: GpsCoordinate,
    ) -> Result<(), Error> {
        self.set_gps_coordinate(p, value, false)
    }
    /// Read longitude components, preserving W negative zero and raw precision.
    /// # Errors
    /// Rejects unknown coordinates/slot/order; unused returns None.
    pub fn physical_gps_longitude(&self, p: usize) -> Result<Option<GpsCoordinate>, Error> {
        self.gps_coordinate(p, true)
    }
    /// Store longitude only, without decimal-degree conversion or rounding.
    /// # Errors
    /// Rejects invalid new components or unavailable slots/order before any store.
    pub fn set_physical_gps_longitude(
        &mut self,
        p: usize,
        value: GpsCoordinate,
    ) -> Result<(), Error> {
        self.set_gps_coordinate(p, value, true)
    }

    // Key 0x1ba: load 0x6a6c..0x6a80; store 0x7ed0..0x7ee0; sentinel 0xa0949.
    /// Read all signed tenths-foot bits; only 7fffffff is Unknown, even for old out-of-range values.
    /// # Errors
    /// Rejects invalid slot/order. Unused returns None.
    pub fn physical_gps_altitude(&self, p: usize) -> Result<Option<GpsAltitude>, Error> {
        let Some(base) = self.gps_base(p)? else {
            return Ok(None);
        };
        let at = base + 14;
        let raw = i32::from_le_bytes(
            self.memory[at..at + 4]
                .try_into()
                .expect("four altitude bytes"),
        );
        Ok(Some(if raw == i32::MAX {
            GpsAltitude::Unknown
        } else {
            GpsAltitude::TenthsOfFoot(raw)
        }))
    }
    /// Store Unknown or -328080..328080 tenths of a foot, without unit rounding.
    /// # Errors
    /// Rejects invalid new altitude or slot/order. Unknown current bits may be replaced.
    pub fn set_physical_gps_altitude(&mut self, p: usize, value: GpsAltitude) -> Result<(), Error> {
        let at = self.gps_target(p)? + 14;
        let raw = match value {
            GpsAltitude::Unknown => i32::MAX,
            GpsAltitude::TenthsOfFoot(n) if (-328080..=328080).contains(&n) => n,
            _ => return Err(invalid(at, "GPS new altitude outside tenths-foot range")),
        };
        self.memory[at..at + 4].copy_from_slice(&raw.to_le_bytes());
        Ok(())
    }

    // Alarm tables c8508/c855c; keys 1c9..1cb stores 708d7..7090c / 70b7a.
    /// Decode the explicit global alarm. Off/Receive ignore inactive group/row;
    /// Group ignores row/topology. Memory requires strict topology and a valid reference.
    /// # Errors
    /// Rejects unknown type/group/reference, without repairing inactive raw bytes.
    pub fn gps_alarm(&self) -> Result<GpsAlarm, Error> {
        let g = usize::from(self.memory[ALARM + 1]);
        match self.memory[ALARM] {
            0 => Ok(GpsAlarm::Off),
            1 => Ok(GpsAlarm::Receive),
            2 if g <= 27 => Ok(GpsAlarm::Group(if g == 0 { None } else { Some(g - 1) })),
            3 if g <= 26 => {
                let groups = self.gps_groups()?;
                let index = le16(&self.memory, ALARM + 2);
                if index >= groups[g].physical_slots().len() {
                    return Err(unsupported(
                        ALARM + 2,
                        "GPS alarm memory index outside group",
                    ));
                }
                Ok(GpsAlarm::Memory { group: g, index })
            }
            _ => Err(unsupported(ALARM, "GPS unknown alarm type or group")),
        }
    }
    /// Replace only four bytes with an explicit alarm tuple, even from unknown current type.
    /// Group permits empty groups; Memory validates its supplied reference, never auto-selects.
    /// # Errors
    /// Invalid new group/index is Invalid; inconsistent current topology is Unsupported.
    pub fn set_gps_alarm(&mut self, value: GpsAlarm) -> Result<(), Error> {
        let (kind, group, index) = match value {
            GpsAlarm::Off => (0, 0, 0),
            GpsAlarm::Receive => (1, 0, 0),
            GpsAlarm::Group(None) => (2, 0, 0),
            GpsAlarm::Group(Some(g)) if g <= 26 => (2, g + 1, 0),
            GpsAlarm::Memory { group, index } if group <= 26 && index < 300 => {
                let groups = self.gps_groups()?;
                if index >= groups[group].physical_slots().len() {
                    return Err(invalid(
                        ALARM + 2,
                        "GPS new alarm memory index outside group",
                    ));
                }
                (3, group, index)
            }
            _ => return Err(invalid(ALARM, "GPS new alarm group or index outside range")),
        };
        let row = (index as u16).to_le_bytes();
        self.memory[ALARM..ALARM + 4].copy_from_slice(&[kind, group as u8, row[0], row[1]]);
        Ok(())
    }
}
