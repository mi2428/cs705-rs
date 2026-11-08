//! Native, offline IC-705 configuration tools.
//!
//! This experimental crate handles the established [`icf`] subset, limited lossless
//! CSV column editing in [`repeater`] and [`gps`], and pure [`clone`] frames.
//! It does not implement full CS-705 compatibility, transfers or device access.

#![forbid(unsafe_code)]

pub mod clone;
pub mod gps;
pub mod icf;
pub mod repeater;
