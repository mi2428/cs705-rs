//! Windows standard NLS producers; no application DLL or captured-output path.
//! LCMapStringA uses LCID0411/flags600000 and distinct explicit2n buffers.
//! CharUpperBuffA uses the separate n-byte copy, including bytes after NUL.

#![cfg(windows)]
#![allow(unsafe_code)]

use super::{Error, native_map_count, native_uppercase_count};
use std::ffi::c_char;

#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "LCMapStringA"]
    fn lc_map_string_a(
        locale: u32,
        flags: u32,
        source: *const c_char,
        source_count: i32,
        destination: *mut c_char,
        destination_count: i32,
    ) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
    #[link_name = "CharUpperBuffA"]
    fn char_upper_buff_a(buffer: *mut c_char, count: u32) -> u32;
}

pub(super) fn map(source: &[u8], destination: &mut [u8]) -> Result<i32, Error> {
    let count = native_map_count(source.len(), destination.len())?;
    // SAFETY: positive INT counts fit both live slices. Rust's shared source
    // and exclusive destination references cannot overlap. Neither is a C string.
    // Keep zero and any partial writes exactly as returned; do not retry or gate.
    Ok(unsafe {
        lc_map_string_a(
            0x0411,
            0x0060_0000,
            source.as_ptr().cast(),
            count,
            destination.as_mut_ptr().cast(),
            count,
        )
    })
}

pub(super) fn uppercase(bytes: &mut [u8]) -> Result<u32, Error> {
    let count = native_uppercase_count(bytes.len())?;
    // Supply a live pointer even for count0, still returning the actual API
    // result rather than inventing a zero/success return for an empty copy.
    let mut empty = 0_u8;
    let buffer: *mut u8 = if bytes.is_empty() {
        &mut empty
    } else {
        bytes.as_mut_ptr()
    };
    // SAFETY: the exclusive buffer is live for the call and covers count bytes.
    // The ANSI API walks the explicit count, not a NUL-terminated Rust string.
    Ok(unsafe { char_upper_buff_a(buffer.cast(), count) })
}
