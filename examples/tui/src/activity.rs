//! In-memory, payload-free operation events. Logging never controls serial I/O.
use crate::encoding;
use std::{
    sync::mpsc::Sender,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy)]
pub enum Level {
    Info,
    Warning,
    Error,
}
pub struct Entry {
    pub time: SystemTime,
    pub level: Level,
    pub message: String,
}
impl Entry {
    pub fn new(level: Level, message: impl Into<String>) -> Self {
        Self {
            time: SystemTime::now(),
            level,
            message: encoding::safe_text(&message.into()),
        }
    }
    pub fn clock(&self) -> String {
        match self.time.duration_since(UNIX_EPOCH) {
            Ok(time) => {
                let secs = time.as_secs() % 86_400;
                format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
            }
            Err(_) => "Clock error".into(),
        }
    }
}
pub fn report(log: &Sender<Entry>, level: Level, message: impl Into<String>) {
    // A closing UI may drop its receiver; serial cleanup must still finish.
    let _ = log.send(Entry::new(level, message));
}
