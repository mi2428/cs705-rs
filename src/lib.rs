//! CS-705 1.30 configuration formats, record editing and clone protocols.
//!
//! Canonical ICF validation and original-source reader semantics are separate
//! APIs. CSV codecs preserve byte strings and explicit locale inputs; clone
//! sessions use caller-owned transports and never discover or open devices.
//! Static-source fidelity is scoped by each operation's contract, not a claim of
//! complete application compatibility, radio suitability or hardware validation.

#![deny(unsafe_code)]

pub mod clone;
pub mod csv;
pub mod diagnostic;
pub mod display;
pub mod gps;
pub mod icf;
pub mod item_help;
pub mod presentation;
pub mod printing;
pub mod repeater;
