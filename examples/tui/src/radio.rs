//! Explicit serial adapter over the library's one-use protocol sessions.
//! No GUI/native Document defaults are synthesized. This example changes only
//! explicit draft deltas, using the source's captured profile and context.

use crate::{
    Result,
    activity::{self, Entry as LogEntry, Level},
    model::{STORAGE, Snapshot},
    storage,
};
use cs705::{
    clone::{self, Outcome, Session, SessionConfig, Transport, WriteDisposition},
    icf::context::Context,
};
use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Radio {
    pub port: String,
    pub baud: u32,
    pub timeout_ms: u64,
}

struct Serial {
    port: Option<Box<dyn serialport::SerialPort>>,
    start: Instant,
    log: Sender<LogEntry>,
    received: u64,
    accepted: u64,
    last_progress: Instant,
}

impl Serial {
    fn progress(&mut self) {
        if self.last_progress.elapsed() >= Duration::from_secs(1) {
            activity::report(
                &self.log,
                Level::Info,
                format!(
                    "Serial progress: RX {} wire bytes, TX accepted {} wire bytes",
                    self.received, self.accepted
                ),
            );
            self.last_progress = Instant::now();
        }
    }
}

impl Radio {
    fn config(&self) -> SessionConfig {
        let mut config = SessionConfig {
            timeout_ms: self.timeout_ms,
            ..SessionConfig::default()
        };
        config.settings.baud = self.baud;
        config
    }

    fn execute(
        &self,
        session: Session,
        cancelled: &AtomicBool,
        log: &Sender<LogEntry>,
        phase: &str,
    ) -> Result<Outcome> {
        if cancelled.load(Ordering::Relaxed) {
            return Err("cancelled before opening serial endpoint".into());
        }
        activity::report(
            log,
            Level::Info,
            format!("{phase}: opening {} at {} baud", self.port, self.baud),
        );
        // The CLI requires both live-I/O and transient-line-risk consent before
        // constructing Radio. Open can still pulse DTR/RTS at the OS boundary.
        let mut port = serialport::new(&self.port, self.baud)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .flow_control(serialport::FlowControl::None)
            .timeout(Duration::from_millis(10))
            .dtr_on_open(false)
            .open()?;
        port.write_data_terminal_ready(false)?;
        port.write_request_to_send(false)?;
        activity::report(
            log,
            Level::Info,
            format!("{phase}: serial connection opened"),
        );
        let mut serial = Serial {
            port: Some(port),
            start: Instant::now(),
            log: log.clone(),
            received: 0,
            accepted: 0,
            last_progress: Instant::now(),
        };
        let started = Instant::now();
        let outcome = session.run(&mut serial, || {
            cancelled.load(Ordering::Relaxed) || started.elapsed() >= Duration::from_secs(90)
        });
        activity::report(
            log,
            Level::Info,
            format!(
                "{phase}: session returned after {} ms; RX {} wire bytes, TX accepted {} wire bytes, terminal phase {:?}",
                started.elapsed().as_millis(),
                serial.received,
                serial.accepted,
                outcome.terminal_phase
            ),
        );
        if let Some(error) = &outcome.failure {
            return Err(format!(
                "protocol failed: {error:?}; write state {:?}; no retry",
                outcome.write_disposition
            )
            .into());
        }
        if let Some(error) = &outcome.close_error {
            return Err(format!(
                "serial close failed: {error:?}; write state {:?}",
                outcome.write_disposition
            )
            .into());
        }
        Ok(outcome)
    }

    pub fn read(&self, cancelled: &AtomicBool, log: &Sender<LogEntry>) -> Result<Snapshot> {
        let query = self.execute(
            Session::query(self.config())?,
            cancelled,
            log,
            "Identity query",
        )?;
        let identity = query.identity.as_ref().ok_or("query has no identity")?;
        let profile = identity.validate()?;
        if !matches!(profile.selector(), 5 | 6) {
            return Err("this example supports only radio profiles 5 and 6".into());
        }
        activity::report(
            log,
            Level::Info,
            format!("Identity validated: profile {}", profile.selector()),
        );
        let mut config = self.config();
        config.expected_context = Some(identity.context()?);
        config.saved_selector = profile.selector();
        config.flag57 = 1;
        let outcome = self.execute(
            Session::read_after_query(config, profile, &query)?,
            cancelled,
            log,
            "Clone read",
        )?;
        if outcome.image.len() != clone::IMAGE_CAPACITY
            || outcome.payload_bytes != STORAGE as u64
            || !(0..STORAGE).all(|address| outcome.acquired(address))
        {
            return Err("radio read lacks complete storage coverage".into());
        }
        if outcome.identity.as_ref() != Some(identity) {
            return Err("identity changed during read".into());
        }
        let mut context = Context::before_registry();
        context.adopt(identity.context()?);
        let snapshot = Snapshot {
            native: outcome.image,
            profile,
            comment: Vec::new(),
            extra: context.encode_packed(),
            identity: Some(identity.clone()),
            file_bytes: None,
        };
        snapshot.memory()?;
        activity::report(
            log,
            Level::Info,
            format!(
                "Read complete: {} payload bytes in {} blocks; storage coverage validated",
                outcome.payload_bytes, outcome.blocks
            ),
        );
        Ok(snapshot)
    }

    pub fn apply(
        &self,
        original: &Snapshot,
        candidate: &Snapshot,
        directory: &Path,
        cancelled: &AtomicBool,
        write_started: &AtomicBool,
        log: &Sender<LogEntry>,
    ) -> Result<(Snapshot, PathBuf)> {
        activity::report(
            log,
            Level::Info,
            "Reading current radio state for the conflict check.",
        );
        let fresh = self.read(cancelled, log)?;
        let expected = prepare_apply(original, candidate, &fresh)?;
        activity::report(log, Level::Info, "Conflict check passed.");
        // Sync the actual pre-write snapshot BEFORE any E3/data/end can be sent.
        let backup = storage::artifact(directory, "before-radio", &fresh.icf()?)?;
        activity::report(
            log,
            Level::Info,
            format!("Pre-write backup synced: {}", backup.display()),
        );
        storage::artifact(directory, "pending-radio", &expected.icf()?)?;
        activity::report(log, Level::Info, "Pending image saved before transmission.");
        let query = self.execute(
            Session::query(self.config())?,
            cancelled,
            log,
            "Pre-write identity query",
        )?;
        if query.identity != fresh.identity {
            return Err("radio identity changed before write".into());
        }
        let mut config = self.config();
        config.expected_context = Some(
            fresh
                .identity
                .as_ref()
                .ok_or("missing original identity")?
                .context()?,
        );
        config.saved_selector = fresh.profile.selector();
        config.flag57 = 1;
        let mask = write_mask(original, candidate)?;
        let write =
            Session::write_after_query(config, fresh.profile, mask, &expected.native, &query)?;
        if cancelled.load(Ordering::Relaxed) {
            return Err("cancelled before write".into());
        }
        write_started.store(true, Ordering::Relaxed);
        activity::report(
            log,
            Level::Warning,
            format!(
                "Starting ONE clone write for all staged edits; mask {mask:02X}. Do not disconnect or power off."
            ),
        );
        let outcome = self
            .execute(write, cancelled, log, "Clone write")
            .map_err(|error| format!("{error}; pre-write backup: {}", backup.display()))?;
        if outcome.write_disposition != WriteDisposition::Acknowledged {
            return Err("write was not acknowledged".into());
        }
        activity::report(
            log,
            Level::Warning,
            "E6 success acknowledged. Persistence is NOT verified; physical restart and Verify are required.",
        );
        // E6 status 0 is NOT a persistence result. The UI must stop here and
        // ask for the manual's physical power cycle before starting verification.
        Ok((expected, backup))
    }

    pub fn verify(
        &self,
        original: &Snapshot,
        candidate: &Snapshot,
        expected: &Snapshot,
        cancelled: &AtomicBool,
        log: &Sender<LogEntry>,
    ) -> Result<Snapshot> {
        activity::report(
            log,
            Level::Info,
            "Verification read requested after operator-confirmed restart.",
        );
        let actual = self.read(cancelled, log)?;
        verify_readback(original, candidate, expected, &actual)?;
        activity::report(
            log,
            Level::Info,
            "Verification passed for the edited data and required memory domains.",
        );
        Ok(actual)
    }
}

fn verify_readback(
    original: &Snapshot,
    candidate: &Snapshot,
    expected: &Snapshot,
    actual: &Snapshot,
) -> Result<()> {
    let edited_match = (0..STORAGE).all(|at| {
        original.native[at] == candidate.native[at] || actual.native[at] == expected.native[at]
    });
    let memory_match = original.same_memory(candidate) || actual.same_memory(expected);
    if actual.identity != expected.identity || !edited_match || !memory_match {
        return Err(
            "post-restart edited-data verification failed; backup and draft retained".into(),
        );
    }
    Ok(())
}

pub fn prepare_apply(
    original: &Snapshot,
    candidate: &Snapshot,
    fresh: &Snapshot,
) -> Result<Snapshot> {
    if original.identity.is_none()
        || original.identity != fresh.identity
        || original.profile != candidate.profile
        || !original.same_memory(fresh)
    {
        return Err(
            "radio memory or identity changed since load; reload instead of overwriting it".into(),
        );
    }
    write_mask(original, candidate)?;
    let settings_changed = (0..STORAGE).any(|at| {
        original.native[at] != candidate.native[at]
            && !crate::model::NORMAL.iter().any(|r| r.contains(&at))
    });
    if settings_changed {
        // Settings edits may correct linked fields. Until narrower domain
        // contracts exist, reject any concurrent writable-setting change.
        for block in clone::write_blocks(original.profile.selector(), 0x1f)? {
            let range = block.start as usize..block.end as usize;
            if original.native[range.clone()] != fresh.native[range] {
                return Err(
                    "radio settings changed since load; reload before applying a settings draft"
                        .into(),
                );
            }
        }
    }
    let mut expected = fresh.clone();
    for at in 0..STORAGE {
        if candidate.native[at] != original.native[at] {
            expected.native[at] = candidate.native[at];
        }
    }
    expected.memory()?;
    Ok(expected)
}

fn write_mask(original: &Snapshot, candidate: &Snapshot) -> Result<u8> {
    let changes = (0..STORAGE)
        .filter(|&at| original.native[at] != candidate.native[at])
        .collect::<Vec<_>>();
    for mask in 0..=0x1f {
        let mut writable = vec![false; STORAGE];
        for block in clone::write_blocks(original.profile.selector(), mask)? {
            writable[block.start as usize..block.end as usize].fill(true);
        }
        if changes.iter().all(|&at| writable[at]) {
            return Ok(mask);
        }
    }
    Err("draft changes a non-writable clone region; no radio write was started".into())
}

impl Read for Serial {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let port = self.port.as_mut().ok_or(io::ErrorKind::NotConnected)?;
        match port.read(bytes) {
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                Ok(0)
            }
            Ok(count) => {
                self.received += count as u64;
                self.progress();
                Ok(count)
            }
            Err(error) => Err(error),
        }
    }
}
impl Write for Serial {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = self
            .port
            .as_mut()
            .ok_or(io::ErrorKind::NotConnected)?
            .write(bytes)?;
        self.accepted += count as u64;
        self.progress();
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.port
            .as_mut()
            .ok_or(io::ErrorKind::NotConnected)?
            .flush()
    }
}
impl Transport for Serial {
    fn reconfigure(&mut self, _token: &[u8]) -> io::Result<()> {
        // This TUI never supplies startup scripts or changes the selected baud
        // mid-operation. Reject an unexpected settings request instead of
        // pretending it was applied or selecting another endpoint.
        Err(io::ErrorKind::Unsupported.into())
    }
    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    fn wait_ms(&mut self, ms: u64) -> io::Result<()> {
        std::thread::sleep(Duration::from_millis(ms));
        Ok(())
    }
    fn pending_output(&mut self) -> io::Result<usize> {
        self.port
            .as_ref()
            .ok_or(io::ErrorKind::NotConnected)?
            .bytes_to_write()
            .map(|n| n as usize)
            .map_err(Into::into)
    }
    fn close(&mut self) -> io::Result<()> {
        self.port.take();
        activity::report(&self.log, Level::Info, "Serial connection closed.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Editor, tests::snapshot};

    fn identified(profile: u8) -> Snapshot {
        let mut source = snapshot(profile);
        let mut tail = vec![0; 33];
        tail[0] = 33;
        tail[1] = profile;
        source.identity = Some(clone::Identity {
            model: 0x4075,
            revision: 1,
            tail,
        });
        source
    }

    #[test]
    fn fresh_merge_preserves_nonmemory_changes_and_rejects_memory_or_identity_conflicts() {
        let mut original = identified(5);
        let mut editor = Editor::new(original.clone()).unwrap();
        editor.copy((0, 0), (99, 0), "Radio", "80").unwrap();
        let candidate = editor.candidate().unwrap();
        let mut fresh = original.clone();
        fresh.native[0x3778a] ^= 0x31;
        fresh.native[0x37dae] ^= 0x42;
        let merged = prepare_apply(&original, &candidate, &fresh).unwrap();
        assert!(merged.same_memory(&candidate));
        assert_eq!(merged.native[0x3778a], fresh.native[0x3778a]);
        assert_eq!(merged.native[0x37dae], fresh.native[0x37dae]);
        fresh.native[0] ^= 1;
        assert!(prepare_apply(&original, &candidate, &fresh).is_err());
        fresh = original.clone();
        fresh.identity = None;
        assert!(prepare_apply(&original, &candidate, &fresh).is_err());
        original.identity = None;
        assert!(prepare_apply(&original, &candidate, &fresh).is_err());
    }

    #[test]
    fn settings_conflicts_and_nonwritable_deltas_fail_before_transmission() {
        for profile in [5, 6] {
            let original = identified(profile);
            let mut editor = Editor::new(original.clone()).unwrap();
            editor
                .edit_settings(|image| {
                    image.patch_memory(&[(0x3969d, &[42])])?;
                    Ok(())
                })
                .unwrap();
            let candidate = editor.candidate().unwrap();
            let mut fresh = original.clone();
            fresh.native[0x3778a] ^= 1;
            assert!(prepare_apply(&original, &candidate, &fresh).is_err());
            fresh = original.clone();
            fresh.native[0x37dae] ^= 1; // Always excluded by the clone planner.
            let expected = prepare_apply(&original, &candidate, &fresh).unwrap();
            assert_eq!(expected.native[0x37dae], fresh.native[0x37dae]);
            assert_eq!(expected.native[0x3969d], 42);
            let mut forbidden = candidate.clone();
            forbidden.native[0x37dae] ^= 1;
            assert!(prepare_apply(&original, &forbidden, &original).is_err());
            assert_eq!(write_mask(&original, &candidate).unwrap(), 0);
        }
    }

    #[test]
    fn readback_requires_edited_bytes_memory_topology_and_the_same_identity() {
        let original = identified(6);
        let mut editor = Editor::new(original.clone()).unwrap();
        editor.edit(0, 0, "Verify", "80").unwrap();
        let candidate = editor.candidate().unwrap();
        let expected = prepare_apply(&original, &candidate, &original).unwrap();
        assert!(verify_readback(&original, &candidate, &expected, &original).is_err());
        let mut actual = expected.clone();
        actual.native[0x3778a] ^= 1;
        verify_readback(&original, &candidate, &expected, &actual).unwrap();
        actual.native[0x3662d] ^= 1;
        assert!(verify_readback(&original, &candidate, &expected, &actual).is_err());
        actual = expected.clone();
        actual.identity = None;
        assert!(verify_readback(&original, &candidate, &expected, &actual).is_err());
    }

    #[test]
    fn already_cancelled_read_does_not_open_the_endpoint() {
        let radio = Radio {
            port: "never-open-test-port".into(),
            baud: 115200,
            timeout_ms: 1000,
        };
        let (log, _) = std::sync::mpsc::channel();
        let error = radio.read(&AtomicBool::new(true), &log).err().unwrap();
        assert!(error.to_string().contains("cancelled before opening"));
    }
}
