//! GPS dataset operations on checked current 6 storage, distinct from local CSV
//! setters and core's atomic delete-set/carry convenience APIs.
//!
//! C8210/C8328 alter order/starts/alarm but do not erase freed physical records.
//! Repeated insert/delete re-read current state after synchronous callbacks.
//! Default type 20 data is supplied at the actual load point; no factory/clock
//! defaults are invented. Parameter checks are safe-API additions, not claims
//! that the original unguarded lower-level routine rejected malformed topology.

use crate::icf::{Error, Icf};

const ORDER: usize = 0x3c385;
const STARTS: usize = 0x3c5dd;
const RECORDS: usize = 0x32c19;
const NAMES: usize = 0x353f1;
const ALARM: usize = 0x3c622;

/// Exact decoded GPS field bytes in physical storage order; no group field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    pub valid: bool,
    pub bytes: [u8; 34],
}
/// Observable native notification, not a guarantee of pixel refresh/delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub id: u32,
    pub synchronous: bool,
    pub arg1: u32,
    pub arg2: u32,
}
impl Event {
    fn sync(id: u32) -> Self {
        Self {
            id,
            synchronous: true,
            arg1: 0,
            arg2: 0,
        }
    }
    fn queued(id: u32) -> Self {
        Self {
            id,
            synchronous: false,
            arg1: 0,
            arg2: 0,
        }
    }
}
/// Normal-return progress. A failing callback returns Err with image prefix kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub completed: usize,
    pub capacity_stopped: bool,
    /// Requested range, not narrowed to completed count on capacity stop.
    pub selection: (usize, usize),
}

fn argument(reason: &'static str) -> Error {
    Error::Invalid {
        line: 0,
        reason: reason.into(),
    }
}
fn word(image: &Icf<'_>, at: usize) -> u16 {
    u16::from_le_bytes([image.memory()[at], image.memory()[at + 1]])
}
fn put_word(image: &mut Icf<'_>, at: usize, value: u16) -> Result<(), Error> {
    image.patch_memory(&[(at, &value.to_le_bytes())])
}

/// C793C/C808C reads all 300 words, including any live IDs beyond the prefix.
/// Unlike strict core gps_groups(), this helper does not normalize tail holes.
pub fn first_free_physical(image: &Icf<'_>) -> Option<usize> {
    let mut used = [false; 300];
    for row in 0..300 {
        let p = usize::from(word(image, ORDER + 2 * row));
        if p < 300 {
            used[p] = true;
        }
    }
    used.iter().position(|used| !used)
}
pub(crate) fn range(image: &Icf<'_>, group: usize) -> Result<(usize, usize), Error> {
    if group >= 27 {
        return Err(argument("GPS group outside 0..27"));
    }
    let groups = image.gps_groups()?;
    Ok((
        groups[..group]
            .iter()
            .map(|g| g.physical_slots().len())
            .sum(),
        groups[group].physical_slots().len(),
    ))
}
fn physical(image: &Icf<'_>, absolute: usize) -> Result<usize, Error> {
    if absolute >= 300 {
        return Err(argument("GPS absolute row outside 0..300"));
    }
    let p = usize::from(word(image, ORDER + 2 * absolute));
    if p >= 300 {
        return Err(argument("GPS target order does not name a physical record"));
    }
    Ok(p)
}
fn record(image: &Icf<'_>, absolute: usize) -> Result<Record, Error> {
    let p = physical(image, absolute)?;
    let mut bytes = [0; 34];
    bytes.copy_from_slice(&image.memory()[RECORDS + 34 * p..RECORDS + 34 * (p + 1)]);
    Ok(Record { valid: true, bytes })
}
pub(crate) fn save(image: &mut Icf<'_>, absolute: usize, record: Record) -> Result<(), Error> {
    if !record.valid {
        return Ok(());
    }
    let p = physical(image, absolute)?;
    image.patch_memory(&[(RECORDS + 34 * p, &record.bytes)])
}

/// Snapshot existing content, not physical IDs, group membership or alarm.
pub fn copy(
    image: &Icf<'_>,
    group: usize,
    first: usize,
    last: usize,
) -> Result<Vec<Record>, Error> {
    let (start, count) = range(image, group)?;
    if first > last || last >= count {
        return Err(argument("GPS copy range outside group"));
    }
    (first..=last)
        .map(|row| record(image, start + row))
        .collect()
}

/// Paste after caller approval; invalid source skips write, target metadata and
/// alarm stay intact. `limit` is the planned transfer length, not valid count.
pub fn paste(
    image: &mut Icf<'_>,
    group: usize,
    first: usize,
    source: &[Record],
    limit: usize,
) -> Result<usize, Error> {
    let (start, count) = range(image, group)?;
    if first > count || limit > source.len() || limit > count - first {
        return Err(argument("GPS paste range outside group"));
    }
    for (i, row) in source[..limit].iter().enumerate() {
        save(image, start + first + i, *row)?;
    }
    Ok(limit)
}

/// C8210 metadata primitive. Source clock/default load deliberately follows it.
pub(crate) fn allocate(image: &mut Icf<'_>, group: usize, row: usize) -> Result<bool, Error> {
    let (start, count) = range(image, group)?;
    if row > count {
        return Err(argument("GPS insertion row outside group"));
    }
    let total: usize = image
        .gps_groups()?
        .iter()
        .map(|g| g.physical_slots().len())
        .sum();
    if total >= 300 {
        return Ok(false);
    }
    let p = first_free_physical(image)
        .ok_or_else(|| argument("GPS allocation has no unused physical slot"))?;
    let absolute = start + row;
    let mut order = image.memory()[ORDER..ORDER + 600].to_vec();
    order.copy_within(2 * absolute..598, 2 * (absolute + 1));
    order[2 * absolute..2 * absolute + 2].copy_from_slice(&(p as u16).to_le_bytes());
    image.patch_memory(&[(ORDER, &order)])?;
    if count == 0 {
        put_word(image, STARTS + 2 * group, absolute as u16)?;
    }
    for g in group + 1..27 {
        let s = word(image, STARTS + 2 * g);
        if s != u16::MAX {
            put_word(image, STARTS + 2 * g, s.wrapping_add(1))?;
        }
    }
    let alarm = image.memory();
    if alarm[ALARM] == 3
        && usize::from(alarm[ALARM + 1]) == group
        && usize::from(word(image, ALARM + 2)) >= row
    {
        put_word(image, ALARM + 2, word(image, ALARM + 2).wrapping_add(1))?;
    }
    Ok(true)
}

/// Insert at the same group-relative position repeatedly. Each seed is obtained
/// after that row's metadata insertion; failure leaves its new order/start/alarm.
pub fn insert(
    image: &mut Icf<'_>,
    group: usize,
    row: usize,
    requested: usize,
    selection: (usize, usize),
    mut seed: impl FnMut(&Icf<'_>) -> Result<Record, Error>,
    mut notify: impl FnMut(&mut Icf<'_>, Event) -> Result<(), Error>,
) -> Result<Outcome, Error> {
    if requested > 300 {
        return Err(argument("GPS insertion request exceeds 300 rows"));
    }
    let mut out = Outcome {
        completed: 0,
        capacity_stopped: false,
        selection,
    };
    for _ in 0..requested {
        if !allocate(image, group, row)? {
            out.capacity_stopped = true;
            break;
        }
        let row_data = seed(image)?;
        let (start, _) = range(image, group)?;
        save(image, start + row, row_data)?;
        out.completed += 1;
        notify(image, Event::sync(0x40))?;
        notify(image, Event::sync(0x39))?;
    }
    Ok(out)
}

/// Native type 20 producer, not a completed record seed. Each local-clock read
/// follows that row's metadata insertion. A clock/default failure retains the
/// inserted metadata; notification failure retains all preceding record writes.
#[allow(
    clippy::too_many_arguments,
    reason = "Clock and notification callbacks run on opposite sides of each committed metadata insertion."
)]
pub fn insert_initialized(
    image: &mut Icf<'_>,
    group: usize,
    row: usize,
    requested: usize,
    selection: (usize, usize),
    q: &crate::icf::context::Context,
    system_langid: Option<u16>,
    mut clock: impl FnMut() -> Result<crate::csv::date_input::Clock, Error>,
    notify: impl FnMut(&mut Icf<'_>, Event) -> Result<(), Error>,
) -> Result<Outcome, Error> {
    insert(
        image,
        group,
        row,
        requested,
        selection,
        |image| crate::icf::record::gps_initial::seed(image, q, system_langid, clock()?),
        notify,
    )
}

/// Delete one row with primitive order: shift/tail, starts, then alarm. Physical
/// content and name tables are untouched, even when a group becomes empty.
pub(crate) fn delete_one(image: &mut Icf<'_>, group: usize, row: usize) -> Result<(), Error> {
    let (start, count) = range(image, group)?;
    if row >= count {
        return Err(argument("GPS deletion row outside group"));
    }
    let absolute = start + row;
    let mut order = image.memory()[ORDER..ORDER + 600].to_vec();
    order.copy_within(2 * (absolute + 1)..600, 2 * absolute);
    order[598..].fill(0xff);
    image.patch_memory(&[(ORDER, &order)])?;
    if count == 1 {
        put_word(image, STARTS + 2 * group, u16::MAX)?;
    }
    for g in group + 1..27 {
        let s = word(image, STARTS + 2 * g);
        if s != u16::MAX {
            put_word(image, STARTS + 2 * g, s.wrapping_sub(1))?;
        }
    }
    if image.memory()[ALARM] == 3 && usize::from(image.memory()[ALARM + 1]) == group {
        let index = usize::from(word(image, ALARM + 2));
        if index == row {
            image.patch_memory(&[(ALARM, &[0; 4])])?;
        } else if index > row {
            put_word(image, ALARM + 2, (index as u16).wrapping_sub(1))?;
        }
    }
    Ok(())
}
/// Caller handles permission/confirmation. The fixed row is re-resolved each
/// iteration; callback failure retains all completed deletions and notifications.
pub fn delete(
    image: &mut Icf<'_>,
    group: usize,
    row: usize,
    requested: usize,
    mut notify: impl FnMut(&mut Icf<'_>, Event) -> Result<(), Error>,
) -> Result<usize, Error> {
    let (_, count) = range(image, group)?;
    if requested > count.saturating_sub(row) || row > count {
        return Err(argument("GPS deletion request outside group"));
    }
    for _ in 0..requested {
        delete_one(image, group, row)?;
        notify(image, Event::sync(0x41))?;
        notify(image, Event::sync(0x39))?;
    }
    notify(image, Event::queued(0x11))?;
    notify(image, Event::sync(0x39))?;
    Ok(requested)
}

/// Clear is a single supplied type 20 source copied to occupied target records,
/// not a delete. The alarm is cleared only when it names the selected content.
/// Transfer queue 14 delivery belongs to the caller's common transfer layer.
pub fn clear(
    image: &mut Icf<'_>,
    group: usize,
    first: usize,
    last: usize,
    seed: Record,
) -> Result<usize, Error> {
    let (start, count) = range(image, group)?;
    if first > last || last >= count {
        return Err(argument("GPS clear range outside group"));
    }
    for row in first..=last {
        save(image, start + row, seed)?;
    }
    if image.memory()[ALARM] == 3
        && usize::from(image.memory()[ALARM + 1]) == group
        && (first..=last).contains(&usize::from(word(image, ALARM + 2)))
    {
        image.patch_memory(&[(ALARM, &[0; 4])])?;
    }
    Ok(last - first + 1)
}

/// Carry preserves order IDs and writes the selected block before its neighbor;
/// direction 0 follows the original nonnegative/down branch. Unlike core's
/// atomic convenience method, a native callback failure is not rolled back.
pub fn carry(
    image: &mut Icf<'_>,
    group: usize,
    first: usize,
    last: usize,
    direction: i32,
    mut notify: impl FnMut(&mut Icf<'_>, Event) -> Result<(), Error>,
) -> Result<bool, Error> {
    let (start, count) = range(image, group)?;
    if first > last || last >= count {
        return Err(argument("GPS carry range outside group"));
    }
    let up = direction < 0;
    if (up && first == 0) || (!up && last + 1 == count) {
        return Ok(false);
    }
    let selected = copy(image, group, first, last)?;
    let neighbor_row = if up { first - 1 } else { last + 1 };
    let neighbor = record(image, start + neighbor_row)?;
    let target = if up {
        start + first - 1
    } else {
        start + first + 1
    };
    for (i, row) in selected.iter().enumerate() {
        save(image, target + i, *row)?;
    }
    save(image, start + if up { last } else { first }, neighbor)?;
    if image.memory()[ALARM] == 3 && usize::from(image.memory()[ALARM + 1]) == group {
        let old = usize::from(word(image, ALARM + 2));
        let next = if (first..=last).contains(&old) {
            if up { old - 1 } else { old + 1 }
        } else if old == neighbor_row {
            if up { last } else { first }
        } else {
            old
        };
        put_word(image, ALARM + 2, next as u16)?;
        // BA16 CBD4D is inside the type 3/same-group guard. Both directions
        // notify here even when the selected alarm position is unchanged.
        notify(image, Event::sync(0x39))?;
    }
    Ok(true)
}

/// Explicit raw 16-byte group rename; group 0 has no name record. Normalization
/// and its native failures must occur before this storage-only operation.
pub fn rename_group_raw(image: &mut Icf<'_>, group: usize, name: [u8; 16]) -> Result<(), Error> {
    if !(1..=26).contains(&group) {
        return Err(argument("GPS named group outside 1..27"));
    }
    image.patch_memory(&[(NAMES + 16 * (group - 1), &name)])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> Icf<'static> {
        let mut m = vec![0xa7; 0x3c9c0];
        m[0x355a1] = 6;
        m[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        m[ORDER..ORDER + 600].fill(0xff);
        m[STARTS..STARTS + 54].fill(0xff);
        m[ALARM..ALARM + 4].fill(0);
        Icf::from_memory(m).unwrap()
    }
    #[test]
    fn library_r2_initialized_insert_uses_clock_after_metadata_and_keeps_failure_prefix() {
        use crate::{csv::date_input::Clock, icf::context::Context};
        let mut q = Context::before_registry();
        q.decode_packed(0x40001);
        let clock = Clock {
            year: 2024,
            month: 3,
            day: 1,
            hour: 0,
            minute: 30,
            second: 40,
            millisecond: 17,
        };
        let mut i = image();
        i.patch_memory(&[(0x396ff, &60_i16.to_le_bytes())]).unwrap();
        let mut calls = 0;
        let mut events = Vec::new();
        let out = insert_initialized(
            &mut i,
            3,
            0,
            2,
            (0, 1),
            &q,
            None,
            || {
                calls += 1;
                Ok(Clock {
                    second: 39 + calls,
                    ..clock
                })
            },
            |_, event| {
                events.push(event.id);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!((calls, out.completed), (2, 2));
        assert_eq!(events, [0x40, 0x39, 0x40, 0x39]);
        assert_eq!(&i.memory()[ORDER..ORDER + 6], &[1, 0, 0, 0, 255, 255]);
        let mut first = [0_u8; 34];
        first[..8].copy_from_slice(&[0, 0, 0, b'N', 0, 0, 0, b'W']);
        first[8..14].copy_from_slice(&[0x29, 2, 0x24, 0x23, 0x30, 0x40]);
        first[14..18].copy_from_slice(&i32::MAX.to_le_bytes());
        first[18..].fill(b' ');
        assert_eq!(&i.memory()[RECORDS..RECORDS + 34], &first);
        first[13] = 0x41;
        assert_eq!(&i.memory()[RECORDS + 34..RECORDS + 68], &first);

        let mut i = image();
        i.patch_memory(&[(0x396ff, &60_i16.to_le_bytes())]).unwrap();
        let mut calls = 0;
        let mut events = Vec::new();
        assert!(
            insert_initialized(
                &mut i,
                3,
                0,
                3,
                (0, 2),
                &q,
                None,
                || {
                    calls += 1;
                    Ok(Clock {
                        year: if calls == 2 { 0 } else { 2024 },
                        ..clock
                    })
                },
                |_, event| {
                    events.push(event.id);
                    Ok(())
                }
            )
            .is_err()
        );
        assert_eq!(calls, 2);
        assert_eq!(events, [0x40, 0x39]);
        assert_eq!(&i.memory()[ORDER..ORDER + 6], &[1, 0, 0, 0, 255, 255]);
        first[13] = 0x40;
        assert_eq!(&i.memory()[RECORDS..RECORDS + 34], &first);
        assert_eq!(&i.memory()[RECORDS + 34..RECORDS + 68], &[0xa7; 34]);
    }
    #[test]
    fn insert_partial_capacity_prefix_and_delete_preserve_physical_data() {
        let mut i = image();
        let mut events = Vec::new();
        insert(
            &mut i,
            1,
            0,
            2,
            (0, 1),
            |_| {
                Ok(Record {
                    valid: true,
                    bytes: [0x41; 34],
                })
            },
            |_, e| {
                events.push(e.id);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(&i.memory()[ORDER..ORDER + 6], &[1, 0, 0, 0, 255, 255]);
        assert_eq!(events, [0x40, 0x39, 0x40, 0x39]);
        let physical_before = i.memory()[RECORDS..RECORDS + 68].to_vec();
        delete(&mut i, 1, 0, 1, |_, _| Ok(())).unwrap();
        assert_eq!(&i.memory()[RECORDS..RECORDS + 68], physical_before);
        assert_eq!(&i.memory()[ORDER..ORDER + 4], &[0, 0, 255, 255]);
        let old = i.memory().to_vec();
        let error = insert(
            &mut i,
            1,
            0,
            1,
            (0, 2),
            |_| Err(argument("injected source failure")),
            |_, _| Ok(()),
        );
        assert!(error.is_err());
        assert_ne!(i.memory(), old);
        assert_eq!(&i.memory()[ORDER..ORDER + 4], &[1, 0, 0, 0]);
    }
    #[test]
    fn copy_paste_carry_clear_use_content_not_order_and_alarm_follows() {
        let mut i = image();
        let mut next = 0;
        insert(
            &mut i,
            0,
            0,
            3,
            (0, 2),
            |_| {
                next += 1;
                Ok(Record {
                    valid: true,
                    bytes: [next; 34],
                })
            },
            |_, _| Ok(()),
        )
        .unwrap();
        i.patch_memory(&[(ALARM, &[3, 0, 1, 0])]).unwrap();
        let order = i.memory()[ORDER..ORDER + 600].to_vec();
        carry(&mut i, 0, 0, 1, 0, |_, e| {
            assert_eq!(e.id, 0x39);
            assert!(e.synchronous);
            Ok(())
        })
        .unwrap();
        assert_eq!(
            copy(&i, 0, 0, 2)
                .unwrap()
                .iter()
                .map(|r| r.bytes[0])
                .collect::<Vec<_>>(),
            [1, 3, 2]
        );
        assert_eq!(word(&i, ALARM + 2), 2);
        assert_eq!(&i.memory()[ORDER..ORDER + 600], order);
        let source = copy(&i, 0, 1, 1).unwrap();
        paste(&mut i, 0, 0, &source, 1).unwrap();
        assert_eq!(word(&i, ALARM + 2), 2);
        clear(
            &mut i,
            0,
            2,
            2,
            Record {
                valid: true,
                bytes: [9; 34],
            },
        )
        .unwrap();
        assert_eq!(&i.memory()[ALARM..ALARM + 4], &[0; 4]);
        assert_eq!(&i.memory()[ORDER..ORDER + 600], order);
    }
    #[test]
    fn free_scan_includes_tail_and_capacity_keeps_original_selection() {
        let mut i = image();
        put_word(&mut i, ORDER + 598, 0).unwrap();
        assert_eq!(first_free_physical(&i), Some(1));
        put_word(&mut i, ORDER + 598, u16::MAX).unwrap();
        insert(
            &mut i,
            26,
            0,
            299,
            (0, 298),
            |_| {
                Ok(Record {
                    valid: true,
                    bytes: [1; 34],
                })
            },
            |_, _| Ok(()),
        )
        .unwrap();
        let out = insert(
            &mut i,
            26,
            0,
            3,
            (0, 2),
            |_| {
                Ok(Record {
                    valid: true,
                    bytes: [2; 34],
                })
            },
            |_, _| Ok(()),
        )
        .unwrap();
        assert_eq!(
            out,
            Outcome {
                completed: 1,
                capacity_stopped: true,
                selection: (0, 2)
            }
        );
    }

    #[test]
    fn carry_notification_uses_alarm_guard_even_without_position_change() {
        fn fixture(alarm: [u8; 4]) -> Icf<'static> {
            let mut i = image();
            i.patch_memory(&[
                (ORDER, &[7, 0, 2, 0, 9, 0, 4, 0, 11, 0]),
                (STARTS + 4, &[0, 0]),
                (ALARM, &alarm),
            ])
            .unwrap();
            for (index, p) in [7, 2, 9, 4, 11].into_iter().enumerate() {
                i.patch_memory(&[(RECORDS + 34 * p, &[b'A' + index as u8; 34])])
                    .unwrap();
            }
            i
        }
        for direction in [-1, 0] {
            for alarm in [[0, 0, 0, 0], [3, 1, 4, 0], [3, 2, 4, 0]] {
                let mut i = fixture(alarm);
                let mut events = Vec::new();
                assert!(
                    carry(&mut i, 2, 1, 2, direction, |_, event| {
                        events.push(event);
                        Ok(())
                    })
                    .unwrap()
                );
                assert_eq!(events.len(), usize::from(alarm[0] == 3 && alarm[1] == 2));
                for event in events {
                    assert_eq!(
                        event,
                        Event {
                            id: 0x39,
                            synchronous: true,
                            arg1: 0,
                            arg2: 0
                        }
                    );
                }
                assert_eq!(&i.memory()[ALARM..ALARM + 4], &alarm);
                let contents: Vec<_> = [7, 2, 9, 4, 11]
                    .into_iter()
                    .map(|p| i.memory()[RECORDS + 34 * p])
                    .collect();
                assert_eq!(
                    contents,
                    if direction < 0 {
                        b"BCADE".to_vec()
                    } else {
                        b"ADBCE".to_vec()
                    }
                );
            }
        }
        let mut i = fixture([3, 2, 1, 0]);
        let before = i.memory().to_vec();
        assert!(
            carry(&mut i, 2, 1, 2, 0, |_, _| Err(argument(
                "injected alarm notification failure"
            )))
            .is_err()
        );
        assert_ne!(i.memory(), before);
        assert_eq!(word(&i, ALARM + 2), 2);
        let mut i = fixture([0; 4]);
        assert!(
            carry(&mut i, 2, 1, 2, 0, |_, _| panic!(
                "unrelated alarm must not notify"
            ))
            .unwrap()
        );
        let mut i = fixture([3, 2, 1, 0]);
        let before = i.memory().to_vec();
        assert!(!carry(&mut i, 2, 0, 0, -1, |_, _| panic!()).unwrap());
        assert_eq!(i.memory(), before);
    }
}
