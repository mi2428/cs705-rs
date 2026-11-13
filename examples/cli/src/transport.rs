//! Explicit serial and deterministic transcript transports for clone::Session.
//! No initializer, parser, help or test enumerates/opens a device. The only open
//! call is guarded by both live-I/O consent and transient-line-risk consent.

use crate::{AppError, Result, args::TransportArgs, io};
use cs705::clone::Transport;
use serde_json::Value;
use std::{
    collections::VecDeque,
    io::{Read, Write},
    time::{Duration, Instant},
};

pub struct Serial {
    port: Option<Box<dyn serialport::SerialPort>>,
    start: Instant,
    path: String,
    dtr: bool,
    rts: bool,
}
impl Serial {
    pub fn open(args: &TransportArgs) -> Result<Self> {
        if !args.allow_device_io || !args.acknowledge_line_risk {
            return Err(AppError::safety(
                "serial open requires --allow-device-io and --acknowledge-line-risk; OS open can transiently assert DTR/RTS",
            ));
        }
        let path = args
            .port
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::invalid("explicit --port is required"))?;
        let mut port = serialport::new(path, args.baud)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .flow_control(serialport::FlowControl::None)
            .timeout(Duration::from_millis(10))
            .dtr_on_open(args.dtr)
            .open()
            .map_err(serial_error)?;
        port.write_data_terminal_ready(args.dtr)
            .map_err(serial_error)?;
        port.write_request_to_send(args.rts).map_err(serial_error)?;
        Ok(Self {
            port: Some(port),
            start: Instant::now(),
            path: path.into(),
            dtr: args.dtr,
            rts: args.rts,
        })
    }
    fn port(&mut self) -> std::io::Result<&mut Box<dyn serialport::SerialPort>> {
        self.port
            .as_mut()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotConnected))
    }
}
fn serial_error(error: serialport::Error) -> AppError {
    let error: std::io::Error = error.into();
    error.into()
}
impl Read for Serial {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        match self.port()?.read(bytes) {
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                Ok(0)
            }
            other => other,
        }
    }
}
impl Write for Serial {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.port()?.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.port()?.flush()
    }
}
impl Transport for Serial {
    fn reconfigure(&mut self, token: &[u8]) -> std::io::Result<()> {
        let (baud, parity, bits, stop) = settings_token(token)?;
        self.port.take();
        let mut port = serialport::new(&self.path, baud)
            .parity(parity)
            .data_bits(bits)
            .stop_bits(stop)
            .flow_control(serialport::FlowControl::None)
            .timeout(Duration::from_millis(10))
            .dtr_on_open(self.dtr)
            .open()?;
        port.write_data_terminal_ready(self.dtr)?;
        port.write_request_to_send(self.rts)?;
        self.port = Some(port);
        Ok(())
    }
    fn now_ms(&self) -> std::io::Result<u64> {
        Ok(self.start.elapsed().as_millis().min(u64::MAX as u128) as u64)
    }
    fn wait_ms(&mut self, ms: u64) -> std::io::Result<()> {
        std::thread::sleep(Duration::from_millis(ms));
        Ok(())
    }
    fn pending_output(&mut self) -> std::io::Result<usize> {
        self.port()?
            .bytes_to_write()
            .map(|n| n as usize)
            .map_err(Into::into)
    }
    fn close(&mut self) -> std::io::Result<()> {
        self.port.take();
        Ok(())
    }
}

/// Strict event script: {"events":[{"write_hex":"..."},{"read_hex":"..."}]}.
/// Short writes use {"write_hex":"...","accept":N}; errors use write_error.
/// Input is validated in full before the session starts. Unknown members fail.
pub struct Mock {
    events: VecDeque<Event>,
    now: u64,
    closed: bool,
    pub writes: usize,
}
enum Event {
    Read(VecDeque<u8>),
    Write(Vec<u8>, usize),
    WriteError,
    Reconfigure(Vec<u8>),
}
impl Mock {
    pub fn from_file(path: &str) -> Result<Self> {
        let value: Value = serde_json::from_slice(&io::read(path)?)
            .map_err(|_| AppError::invalid("invalid mock JSON"))?;
        let object = value
            .as_object()
            .filter(|o| o.len() == 1 && o.contains_key("events"))
            .ok_or_else(|| AppError::invalid("mock requires only an events array"))?;
        let list = object["events"]
            .as_array()
            .ok_or_else(|| AppError::invalid("mock events must be an array"))?;
        let mut events = VecDeque::new();
        for value in list {
            let event = value
                .as_object()
                .ok_or_else(|| AppError::invalid("mock event must be an object"))?;
            if let Some(text) = event.get("read_hex").and_then(Value::as_str) {
                if event.len() != 1 {
                    return Err(AppError::invalid("read event has unknown members"));
                }
                let bytes = io::decode_hex(text)?;
                if bytes.is_empty() {
                    return Err(AppError::invalid("mock read must not be empty"));
                }
                events.push_back(Event::Read(bytes.into()));
            } else if let Some(text) = event.get("write_hex").and_then(Value::as_str) {
                if event.keys().any(|k| k != "write_hex" && k != "accept") {
                    return Err(AppError::invalid("write event has unknown members"));
                }
                let bytes = io::decode_hex(text)?;
                let accept = match event.get("accept") {
                    Some(value) => value
                        .as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .ok_or_else(|| AppError::invalid("mock accept must be a byte count"))?,
                    None => bytes.len(),
                };
                if accept > bytes.len() {
                    return Err(AppError::invalid("mock accepts more bytes than requested"));
                }
                events.push_back(Event::Write(bytes, accept));
            } else if let Some(text) = event.get("reconfigure_hex").and_then(Value::as_str) {
                if event.len() != 1 {
                    return Err(AppError::invalid("reconfigure event has unknown members"));
                }
                events.push_back(Event::Reconfigure(io::decode_hex(text)?));
            } else if event.len() == 1 && event.get("write_error") == Some(&Value::Bool(true)) {
                events.push_back(Event::WriteError);
            } else {
                return Err(AppError::invalid("unknown mock event"));
            }
        }
        Ok(Self {
            events,
            now: 0,
            closed: false,
            writes: 0,
        })
    }
}
impl Read for Mock {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if self.closed {
            return Err(std::io::ErrorKind::NotConnected.into());
        }
        let Some(Event::Read(source)) = self.events.front_mut() else {
            return Ok(0);
        };
        let n = bytes.len().min(source.len());
        for byte in &mut bytes[..n] {
            *byte = source.pop_front().expect("bounded event read");
        }
        if source.is_empty() {
            self.events.pop_front();
        }
        Ok(n)
    }
}
impl Write for Mock {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.closed {
            return Err(std::io::ErrorKind::NotConnected.into());
        }
        self.writes += 1;
        match self.events.pop_front() {
            Some(Event::Write(expected, n)) if expected == bytes => Ok(n),
            Some(Event::WriteError) => Err(std::io::ErrorKind::BrokenPipe.into()),
            _ => Err(std::io::ErrorKind::InvalidData.into()),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl Transport for Mock {
    fn reconfigure(&mut self, token: &[u8]) -> std::io::Result<()> {
        if self.closed {
            return Err(std::io::ErrorKind::NotConnected.into());
        }
        match self.events.pop_front() {
            Some(Event::Reconfigure(expected)) if expected == token => Ok(()),
            _ => Err(std::io::ErrorKind::InvalidData.into()),
        }
    }
    fn now_ms(&self) -> std::io::Result<u64> {
        Ok(self.now)
    }
    fn wait_ms(&mut self, ms: u64) -> std::io::Result<()> {
        self.now = self.now.saturating_add(ms);
        Ok(())
    }
    fn pending_output(&mut self) -> std::io::Result<usize> {
        Ok(0)
    }
    fn close(&mut self) -> std::io::Result<()> {
        if self.closed {
            return Err(std::io::ErrorKind::NotConnected.into());
        }
        self.closed = true;
        if self.events.is_empty() {
            Ok(())
        } else {
            Err(std::io::ErrorKind::InvalidData.into())
        }
    }
}

/// One protocol operation borrows the already prepared native sequence. Mock
/// transcripts retain their deque across logical closes; real ports close at
/// each Session boundary. The owner reopens that same endpoint before the next
/// Session and always performs terminal close, including Mock's event guard.
pub struct SequenceLease<'a, T: ?Sized> {
    transport: &'a mut T,
    closed: bool,
    retain_transcript: bool,
}
impl<'a, T: Transport + ?Sized> SequenceLease<'a, T> {
    pub fn new(transport: &'a mut T, retain_transcript: bool) -> Self {
        Self {
            transport,
            closed: false,
            retain_transcript,
        }
    }
    fn active(&mut self) -> std::io::Result<&mut T> {
        if self.closed {
            Err(std::io::ErrorKind::NotConnected.into())
        } else {
            Ok(self.transport)
        }
    }
}
impl<T: Transport + ?Sized> Read for SequenceLease<'_, T> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.active()?.read(bytes)
    }
}
impl<T: Transport + ?Sized> Write for SequenceLease<'_, T> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.active()?.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.active()?.flush()
    }
}
impl<T: Transport + ?Sized> Transport for SequenceLease<'_, T> {
    fn now_ms(&self) -> std::io::Result<u64> {
        self.transport.now_ms()
    }
    fn wait_ms(&mut self, ms: u64) -> std::io::Result<()> {
        self.active()?.wait_ms(ms)
    }
    fn pending_output(&mut self) -> std::io::Result<usize> {
        self.active()?.pending_output()
    }
    fn reconfigure(&mut self, token: &[u8]) -> std::io::Result<()> {
        self.active()?.reconfigure(token)
    }
    fn close(&mut self) -> std::io::Result<()> {
        if self.closed {
            return Err(std::io::ErrorKind::NotConnected.into());
        }
        self.closed = true;
        if self.retain_transcript {
            Ok(())
        } else {
            self.transport.close()
        }
    }
}

/// BuildCommDCB's normal legacy four-token form, not a permissive settings guess.
/// Extended OS-specific grammar is rejected before closing the selected port.
fn settings_token(
    token: &[u8],
) -> std::io::Result<(
    u32,
    serialport::Parity,
    serialport::DataBits,
    serialport::StopBits,
)> {
    let invalid = || std::io::Error::from(std::io::ErrorKind::InvalidInput);
    let text = std::str::from_utf8(token).map_err(|_| invalid())?;
    if !text.is_ascii() {
        return Err(invalid());
    }
    let parts: Vec<_> = text.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return Err(std::io::ErrorKind::Unsupported.into());
    }
    let baud = parts[0]
        .parse::<u32>()
        .ok()
        .filter(|n| (300..=4_000_000).contains(n))
        .ok_or_else(invalid)?;
    let parity = match parts[1] {
        "n" | "N" => serialport::Parity::None,
        "e" | "E" => serialport::Parity::Even,
        "o" | "O" => serialport::Parity::Odd,
        _ => return Err(invalid()),
    };
    let bits = parts[2]
        .parse::<u8>()
        .ok()
        .and_then(|n| serialport::DataBits::try_from(n).ok())
        .ok_or_else(invalid)?;
    let stop = match parts[3] {
        "1" => serialport::StopBits::One,
        "2" => serialport::StopBits::Two,
        _ => return Err(invalid()),
    };
    Ok((baud, parity, bits, stop))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_transcript_lease_cannot_consume_the_next_query_generation() {
        use cs705::clone::{self, Direction, Frame, Identity, Session, SessionConfig};
        let mut tail = vec![0; 33];
        tail[0] = 33;
        tail[1] = 6;
        let response = Frame::Response(Identity {
            model: 0x4075,
            revision: 1,
            tail,
        })
        .encode(Direction::FromRadio)
        .unwrap();
        let request = Frame::Identify.encode(Direction::ToRadio).unwrap();
        let mut events = VecDeque::new();
        for _ in 0..2 {
            events.push_back(Event::Write(request.clone(), request.len()));
            events.push_back(Event::Read(response.clone().into()));
        }
        let mut mock = Mock {
            events,
            now: 0,
            closed: false,
            writes: 0,
        };
        let mut config = SessionConfig::default();
        config.settings.echo = clone::Echo::None;
        {
            let mut lease = SequenceLease::new(&mut mock, true);
            let first = Session::query(config.clone())
                .unwrap()
                .run(&mut lease, || Ok(false));
            assert_eq!(first.result_code(), 0);
            assert_eq!(
                lease.write(&request).unwrap_err().kind(),
                std::io::ErrorKind::NotConnected
            );
            assert_eq!(
                lease.read(&mut [0; 64]).unwrap_err().kind(),
                std::io::ErrorKind::NotConnected
            );
        }
        assert_eq!(mock.events.len(), 2);
        assert_eq!(mock.writes, 1);
        assert!(!mock.closed);
        let second = Session::query(config)
            .unwrap()
            .run(&mut SequenceLease::new(&mut mock, true), || Ok(false));
        assert_eq!(second.result_code(), 0);
        assert_eq!(mock.writes, 2);
        mock.close().unwrap();
    }
    #[test]
    fn native_sequence_forwards_real_close_errors_but_defers_transcript_close() {
        struct Closing(usize);
        impl Read for Closing {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                unreachable!()
            }
        }
        impl Write for Closing {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                unreachable!()
            }
            fn flush(&mut self) -> std::io::Result<()> {
                unreachable!()
            }
        }
        impl Transport for Closing {
            fn now_ms(&self) -> std::io::Result<u64> {
                Ok(0)
            }
            fn wait_ms(&mut self, _: u64) -> std::io::Result<()> {
                unreachable!()
            }
            fn pending_output(&mut self) -> std::io::Result<usize> {
                unreachable!()
            }
            fn reconfigure(&mut self, _: &[u8]) -> std::io::Result<()> {
                unreachable!()
            }
            fn close(&mut self) -> std::io::Result<()> {
                self.0 += 1;
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
        }
        let mut port = Closing(0);
        let mut lease = SequenceLease::new(&mut port, false);
        assert_eq!(
            lease.close().unwrap_err().kind(),
            std::io::ErrorKind::BrokenPipe
        );
        for result in [
            lease.read(&mut [0]).map(|_| ()),
            lease.write(&[0]).map(|_| ()),
            lease.flush(),
            lease.wait_ms(1),
            lease.pending_output().map(|_| ()),
            lease.reconfigure(b"115200,N,8,1"),
            lease.close(),
        ] {
            assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotConnected);
        }
        assert_eq!(port.0, 1);
        SequenceLease::new(&mut port, true).close().unwrap();
        assert_eq!(port.0, 1);
        assert_eq!(
            port.close().unwrap_err().kind(),
            std::io::ErrorKind::BrokenPipe
        );
        assert_eq!(port.0, 2);
    }
    #[test]
    fn settings_tokens_are_explicit_and_never_open_a_port() {
        assert_eq!(
            settings_token(b"115200,N,8,1").unwrap(),
            (
                115200,
                serialport::Parity::None,
                serialport::DataBits::Eight,
                serialport::StopBits::One
            )
        );
        for invalid in [
            b"115200".as_slice(),
            b"0,N,8,1",
            b"115200,X,8,1",
            b"115200,N,9,1",
            b"115200,N,8,1.5",
            b"baud=115200",
            b"115200,N,8,1\xff",
        ] {
            assert!(settings_token(invalid).is_err());
        }
    }
}
