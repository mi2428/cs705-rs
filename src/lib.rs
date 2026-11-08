//! Native, offline IC-705 configuration tools.
//!
//! This experimental crate handles the established [`icf`] subset, lossless
//! CSV name editing in [`repeater`] and pure [`clone`] frames. It does not
//! implement full CS-705 compatibility, transfers or device access.

#![forbid(unsafe_code)]

pub mod clone;
pub mod icf;
pub mod repeater;
