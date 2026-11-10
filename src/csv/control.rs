//! Ordered counters and native write gates (89231/91320/835D0/88E04).
//! C4 is an event/marker DWORD, never a successful-write count.

use super::Error;

/// Exact four DWORD outputs in ABI order. Arithmetic wraps like x86 ADD/INC.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub s: u32,
    pub c2: u32,
    pub c3: u32,
    pub c4: u32,
}
impl Counts {
    /// 878C4 selected: code2 is capacity, not a discarded event.
    pub fn selected_event(&mut self, code: u32, delta: u32) {
        match code {
            0 => {
                self.c3 = self.c3.wrapping_add(delta);
                self.c4 = self.c4.wrapping_add(1);
            }
            1 => self.c2 = self.c2.wrapping_add(1),
            2 => self.s = 2,
            _ => {}
        }
    }
    /// 91320 whole: code2 discards; code3 sets S2 without a C4 marker.
    pub fn whole_event(&mut self, code: u32, delta: u32) {
        match code {
            0 => self.selected_event(0, delta),
            1 | 2 => self.c2 = self.c2.wrapping_add(1),
            3 => self.s = 2,
            _ => {}
        }
    }
    /// Capacity stop in GPS/callsign/84DBC deliberately overwrites C4 with 1.
    pub fn capacity_marker(&mut self, status: u32) {
        self.s = status;
        self.c4 = 1;
    }
    /// Selected merge's final group-count adjustment, not used by whole import.
    pub fn finish_selected(&mut self, group_count: u32) {
        if self.s == 2 && group_count < 2500 {
            self.s = 1;
        }
    }
    /// 7FFEC report order; returned messages do not control rollback/acceptance.
    pub fn messages(&self) -> Vec<Message> {
        if self.c4 == 0 {
            return vec![Message::NoData];
        }
        let mut out = Vec::new();
        match self.s {
            1 => out.push(Message::NoChannelSpace),
            2 => out.push(Message::ChannelOver),
            3 => out.push(Message::GroupChannelOver),
            _ => {}
        }
        if self.c2 != 0 {
            out.push(Message::Discard);
        }
        if self.c3 != 0 {
            out.push(Message::Correct);
        }
        out
    }
}
/// Semantic message keys, not cached language text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    NoData,
    NoChannelSpace,
    ChannelOver,
    GroupChannelOver,
    Discard,
    Correct,
}

/// Pair correction selection, not a sum of the two row corrections (82818).
pub fn pair_correction(
    upper: u32,
    lower_valid: bool,
    lower_c2: u32,
    lower_c3: u32,
    old_split: bool,
) -> u32 {
    if upper != 0 {
        upper
    } else if lower_valid {
        lower_c3
    } else if lower_c2 != 0 || old_split {
        lower_c2
    } else {
        0
    }
}

/// Native WriteFile result. Any nonzero BOOL succeeds even for short/count0.
#[derive(Debug, Clone, Copy)]
pub struct WriteResult {
    pub bool_value: u32,
    pub bytes_written: usize,
}

/// Export header then ranges. A false Memory write stops only its inner range;
/// all other families stop the single row loop. Closure errors retain prefix.
/// The caller owns file creation/truncation/close; close results are not a gate.
pub fn export_ranges(
    header: &[u8],
    ranges: &[Vec<Vec<u8>>],
    memory_ranges: bool,
    mut write: impl FnMut(&[u8]) -> Result<WriteResult, Error>,
) -> Result<bool, Error> {
    if write(header)?.bool_value == 0 {
        return Ok(false);
    }
    let mut success = true;
    for range in ranges {
        for row in range {
            if write(row)?.bool_value == 0 {
                success = false;
                if !memory_ranges {
                    return Ok(false);
                }
                break;
            }
        }
    }
    Ok(success)
}

/// 835D0's lazy row assembly. The classifier returns None for an invalid row,
/// otherwise (side24, builder bytes). In pair mode side0 REPLACES the buffer
/// without writing; side!=0 appends and writes. Buffer survives range boundaries
/// and false writes, and false stops only that range's remaining source reads.
pub fn export_memory<S>(
    header: &[u8],
    ranges: &[Vec<S>],
    pair: bool,
    mut build: impl FnMut(&S) -> Result<Option<(u8, Vec<u8>)>, Error>,
    mut write: impl FnMut(&[u8]) -> Result<WriteResult, Error>,
) -> Result<bool, Error> {
    if write(header)?.bool_value == 0 {
        return Ok(false);
    }
    let mut success = true;
    let mut buffer = Vec::new();
    for range in ranges {
        for source in range {
            let Some((side, bytes)) = build(source)? else {
                continue;
            };
            if !pair || side == 0 {
                buffer = bytes;
            } else {
                if buffer
                    .len()
                    .checked_add(bytes.len())
                    .is_none_or(|n| n > 16 * 1024 * 1024)
                {
                    return Err(Error::Limit("memory pair buffer exceeds 16 MiB"));
                }
                buffer.extend_from_slice(&bytes);
            }
            if pair && side == 0 {
                continue;
            }
            if write(&buffer)?.bool_value == 0 {
                success = false;
                break;
            }
        }
    }
    Ok(success)
}

/// 83C60 scan count/apply order. Out-of-range row-local counts do not propagate;
/// an in-range write failure leaves C2/C3 already added but C4 not incremented.
#[derive(Debug, Clone, Copy)]
pub struct ScanRow {
    pub position: u32,
    pub valid: bool,
    pub c2_delta: u32,
    pub c3_delta: u32,
}
#[derive(Debug, Clone, Copy)]
pub struct ScanRange {
    pub base: u32,
    pub last: i32,
    pub stop_on_excess: bool,
}
pub fn scan_row(
    counts: &mut Counts,
    row: ScanRow,
    range: ScanRange,
    mut apply: impl FnMut(u32) -> Result<(), Error>,
) -> Result<bool, Error> {
    let relative = row.position.wrapping_sub(range.base);
    if row.position == 0xffff || row.position < range.base {
        return Ok(true);
    }
    if (relative as i32) > range.last {
        if range.stop_on_excess {
            counts.s = 2;
            return Ok(false);
        }
        return Ok(true);
    }
    counts.c2 = counts.c2.wrapping_add(row.c2_delta);
    counts.c3 = counts.c3.wrapping_add(row.c3_delta);
    if row.valid {
        apply(relative)?;
        counts.c4 = counts.c4.wrapping_add(1);
    }
    Ok(true)
}

/// Import cancel boundaries; inner cancel still reaches document stage9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportGate {
    FileCancelled,
    InnerCancelled,
    Header(u8),
}
impl ImportGate {
    pub fn reaches_rows(self) -> bool {
        matches!(self, Self::Header(1))
    }
    pub fn reaches_stage9(self) -> bool {
        self != Self::FileCancelled
    }
}

/// 94C20/9514F ordering over the same live state. A normally returned row
/// report (including zero writes or partial rejection) still reaches stage9;
/// an exceptional row error does not. Neither cancellation resets the state.
pub fn import_with_gate<S, R, T, E>(
    state: &mut S,
    gate: ImportGate,
    rows: impl FnOnce(&mut S) -> Result<R, E>,
    stage9: impl FnOnce(&mut S) -> Result<T, E>,
) -> Result<(Option<R>, Option<T>), E> {
    if !gate.reaches_stage9() {
        return Ok((None, None));
    }
    let rows = if gate.reaches_rows() {
        Some(rows(state)?)
    } else {
        None
    };
    Ok((rows, Some(stage9(state)?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_gate_uses_live_state_and_preserves_exception_prefixes() {
        for (gate, expected) in [
            (ImportGate::FileCancelled, b"".as_slice()),
            (ImportGate::InnerCancelled, b"N"),
            (ImportGate::Header(0), b"N"),
            (ImportGate::Header(1), b"RN"),
            (ImportGate::Header(2), b"N"),
            (ImportGate::Header(255), b"N"),
        ] {
            let mut state = Vec::new();
            let (rows, stage9) = import_with_gate(
                &mut state,
                gate,
                |s| {
                    s.push(b'R');
                    Ok::<_, Error>(0)
                },
                |s| {
                    assert_eq!(
                        s.as_slice(),
                        if gate.reaches_rows() {
                            b"R".as_slice()
                        } else {
                            b""
                        }
                    );
                    s.push(b'N');
                    Ok(7)
                },
            )
            .unwrap();
            assert_eq!(state, expected);
            assert_eq!(rows, gate.reaches_rows().then_some(0));
            assert_eq!(stage9, gate.reaches_stage9().then_some(7));
        }
        let mut state = Vec::new();
        assert!(
            import_with_gate(
                &mut state,
                ImportGate::Header(1),
                |s| {
                    s.push(b'R');
                    Err::<(), _>(Error::Operation("injected row failure"))
                },
                |_| -> Result<(), Error> { panic!("exception skips stage9") },
            )
            .is_err()
        );
        assert_eq!(state, b"R");
        assert!(
            import_with_gate(
                &mut state,
                ImportGate::InnerCancelled,
                |_| -> Result<(), Error> { panic!("cancel skips rows") },
                |s| {
                    s.push(b'N');
                    Err::<(), _>(Error::Operation("injected stage9 failure"))
                },
            )
            .is_err()
        );
        assert_eq!(state, b"RN");
    }
    #[test]
    fn counters_wrap_and_same_status_does_not_mean_same_report() {
        let mut c = Counts {
            c2: u32::MAX,
            c3: u32::MAX,
            c4: u32::MAX,
            ..Counts::default()
        };
        c.whole_event(0, 2);
        c.whole_event(2, 0);
        assert_eq!(
            c,
            Counts {
                s: 0,
                c2: 0,
                c3: 1,
                c4: 0
            }
        );
        c.whole_event(3, 0);
        assert_eq!(c.messages(), [Message::NoData]);
        c.capacity_marker(2);
        assert_eq!(c.messages(), [Message::ChannelOver, Message::Correct]);
        c.finish_selected(2499);
        assert_eq!(c.s, 1);
        assert_eq!(pair_correction(2, true, 0, 4, false), 2);
        assert_eq!(pair_correction(0, false, 1, 4, true), 1);
        assert!(!ImportGate::Header(2).reaches_rows());
        assert!(ImportGate::InnerCancelled.reaches_stage9());
    }
    #[test]
    fn literal_partial_output_and_memory_outer_continuation() {
        let ranges = vec![vec![b"A".to_vec(), b"B".to_vec()], vec![b"C".to_vec()]];
        for (memory, expected) in [(true, b"HAC".as_slice()), (false, b"HA")] {
            let mut bytes = Vec::new();
            assert!(
                !export_ranges(b"H", &ranges, memory, |row| {
                    bytes.extend_from_slice(row);
                    Ok(WriteResult {
                        bool_value: u32::from(row != b"A"),
                        bytes_written: 0,
                    })
                })
                .unwrap()
            );
            assert_eq!(bytes, expected);
        }
        assert!(
            export_ranges(b"H", &ranges, false, |_| Ok(WriteResult {
                bool_value: 0x100,
                bytes_written: 0
            }))
            .unwrap()
        );
    }

    #[test]
    fn pair_buffer_survives_failed_range_and_scan_failure_preserves_prior_counts() {
        let ranges = vec![
            vec![(0, b"A".as_slice()), (1, b"B"), (1, b"D")],
            vec![(1, b"C")],
        ];
        let mut output = Vec::new();
        let mut calls = 0;
        let mut built = Vec::new();
        let ok = export_memory(
            b"H",
            &ranges,
            true,
            |source| {
                built.push(source.1.to_vec());
                Ok(Some((source.0, source.1.to_vec())))
            },
            |bytes| {
                calls += 1;
                if calls == 2 {
                    output.extend_from_slice(&bytes[..1]);
                    Ok(WriteResult {
                        bool_value: 0,
                        bytes_written: 1,
                    })
                } else {
                    output.extend_from_slice(bytes);
                    Ok(WriteResult {
                        bool_value: 1,
                        bytes_written: bytes.len(),
                    })
                }
            },
        )
        .unwrap();
        assert!(!ok);
        assert_eq!(output, b"HAABC");
        assert_eq!(built, [b"A".to_vec(), b"B".to_vec(), b"C".to_vec()]);
        let mut counts = Counts::default();
        let row = ScanRow {
            position: 11,
            valid: true,
            c2_delta: 1,
            c3_delta: 2,
        };
        let range = ScanRange {
            base: 10,
            last: 3,
            stop_on_excess: false,
        };
        assert!(
            scan_row(&mut counts, row, range, |relative| {
                assert_eq!(relative, 1);
                Err(Error::Operation("injected writer failure"))
            })
            .is_err()
        );
        assert_eq!(
            counts,
            Counts {
                s: 0,
                c2: 1,
                c3: 2,
                c4: 0
            }
        );
        scan_row(
            &mut counts,
            ScanRow {
                position: 99,
                ..row
            },
            range,
            |_| panic!(),
        )
        .unwrap();
        assert_eq!(counts.c2, 1);
    }
}
