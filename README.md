# CS-705

[![Library](https://github.com/mi2428/cs705-rs/actions/workflows/library.yml/badge.svg?branch=main)](https://github.com/mi2428/cs705-rs/actions/workflows/library.yml)
[![PyO3](https://github.com/mi2428/cs705-rs/actions/workflows/pyo3.yml/badge.svg?branch=main)](https://github.com/mi2428/cs705-rs/actions/workflows/pyo3.yml)
[![Examples](https://github.com/mi2428/cs705-rs/actions/workflows/examples.yml/badge.svg?branch=main)](https://github.com/mi2428/cs705-rs/actions/workflows/examples.yml)
[![Releases](https://github.com/mi2428/cs705-rs/actions/workflows/release.yml/badge.svg)](https://github.com/mi2428/cs705-rs/actions/workflows/release.yml)

A native Rust library for reading and editing ICOM IC-705 configuration files and handling clone protocols.
Use it to build your own configuration tools without running the CS-705 Windows application or loading ICOM EXE/DLL files at runtime.

- Refer to the [API documentation](https://docs.rs/cs705) for public interfaces and their contracts.
- Read [`NOTES.md`](https://github.com/mi2428/cs705-rs/blob/main/NOTES.md) for technical details of the recovered formats, editing behavior, and clone protocol.
- See [`examples/cli`](https://github.com/mi2428/cs705-rs/tree/main/examples/cli) and [`example/tui`](https://github.com/mi2428/cs705-rs/tree/main/examples/tui) for applications built on the library, and [`example/jupyter`](examples/jupyter) for interactive tables and plots.

## Features

- ICF parsing, serialization, and explicit layout conversion.
  Typed editing of memories, radio settings, repeater lists, GPS memories, and callsigns.
- Clone protocol frames and transfer sessions over caller-owned transports.
- CS-705 CSV import and export with explicit locale and byte-string handling.

### Case: List Memory Channels and Rename the First One

Read an existing canonical MapRev 6 ICF file and list the name and main/RX frequency of each programmed memory channel.
Then rename the first listed channel to `LOCAL` and save a separate file, leaving the input unchanged.
The example requires at least one programmed channel and performs no radio I/O.

```rust
use cs705::icf::{Icf, MemorySide};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load a configuration file without opening a connection to the radio.
    let source = std::fs::read("settings.icf")?;
    let mut image = Icf::parse(&source)?;

    // Resolve logical group/row positions to physical slots while listing channels.
    let mut first_slot = None;
    for (group_index, group) in image.memory_groups()?.iter().enumerate() {
        for (row, slot) in group.slots().iter().enumerate() {
            let Some(slot) = *slot else {
                continue;
            };
            let name = image.physical_memory_name_raw(slot)?.ok_or("channel is empty")?;
            let hz = image.physical_memory_frequency_hz(slot, MemorySide::Main)?;
            // Escape raw name bytes for display without changing the stored name.
            println!(
                "{group_index:02}/{row:02}: {} {hz} Hz",
                name.trim_ascii_end().escape_ascii()
            );
            first_slot.get_or_insert(slot);
        }
    }

    // Rename the first listed channel, preserving its frequency and mode.
    let slot = first_slot.ok_or("no programmed memory channels")?;
    image.set_physical_memory_name_ascii(slot, "LOCAL")?;

    // Save the edited configuration without overwriting the input file.
    std::fs::write("settings-edited.icf", image.to_bytes())?;
    println!("Renamed the first listed channel to LOCAL; saved settings-edited.icf");
    Ok(())
}
```

### Case: Export Memory Channels to CSV

Export all memory groups from `settings.icf` to `memories.csv` using the CS-705 CSV format.
The output keeps the stored text encoding rather than converting names to UTF-8.

```rust
use cs705::{
    csv::{
        Locale,
        channel::{self, Kind},
    },
    icf::Icf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::fs::read("settings.icf")?;
    let image = Icf::parse(&source)?;

    // Export all groups and rows with explicit decimal and list separators.
    let locale = Locale {
        decimal: b'.',
        list: b',',
    };
    let exported = channel::export_memory(&image, Kind::Memory, None, None, locale);

    // An export can fail after producing a prefix; do not save partial output.
    if let Some(error) = exported.failure {
        return Err(format!("CSV export failed: {error:?}").into());
    }
    std::fs::write("memories.csv", &exported.bytes)?;
    println!("Exported {} rows to memories.csv", exported.rows);
    Ok(())
}
```

### Case: Read and Write a Raw Clone Image

Use the functions below with an application-owned `Transport` and the radio's matching profile.
The application selects the port and supplies bounded I/O; each session closes its transport after running.
See the [CLI transport adapter](examples/cli/src/transport.rs) for a serial-port implementation.

```rust
use cs705::clone::{self, Outcome, Profile, Session, SessionConfig, Transport, WriteDisposition};
use std::error::Error;

fn check_clone_result(outcome: Outcome) -> Result<Outcome, Box<dyn Error>> {
    // Preserve both the protocol error and the final write state in diagnostics.
    if outcome.failure.is_some() || outcome.close_error.is_some() {
        return Err(format!(
            "clone failed: {:?}; close: {:?}; write state: {:?}",
            outcome.failure, outcome.close_error, outcome.write_disposition
        )
        .into());
    }
    Ok(outcome)
}

fn read_raw(transport: &mut impl Transport, profile: Profile) -> Result<Outcome, Box<dyn Error>> {
    // Query the identity, check the profile, then read the raw image.
    let session = Session::read(SessionConfig::default(), profile)?;
    let outcome = check_clone_result(session.run(transport, || Ok(false)))?;

    // Keep image and coverage together; the buffer can contain an unacquired tail.
    let required = clone::read_range(profile.selector())?;
    if required
        .into_iter()
        .any(|at| !outcome.acquired(at as usize))
    {
        return Err("raw read did not acquire the complete profile range".into());
    }
    Ok(outcome)
}

fn write_raw(
    transport: &mut impl Transport,
    profile: Profile,
    mask: u8,
    image: &[u8],
    approved: bool,
) -> Result<Outcome, Box<dyn Error>> {
    // Require explicit approval before any protocol bytes are sent.
    if !approved {
        return Err("raw write was not approved".into());
    }
    let session = Session::write(SessionConfig::default(), profile, mask, image)?;
    let outcome = check_clone_result(session.run(transport, || Ok(false)))?;

    // Protocol acknowledgement does not verify the resulting radio settings.
    if outcome.write_disposition != WriteDisposition::Acknowledged {
        return Err("raw write was not acknowledged; do not retry automatically".into());
    }
    Ok(outcome)
}
```

`read_raw` returns the image together with its coverage bitmap, rather than treating every buffer byte as acquired data.
`write_raw` takes a full internal image for the selected profile; `mask` selects optional regions and does not remove permanent write exclusions.
Raw writes skip Document validation and normalization, so use only a verified image for the intended radio and keep a fresh backup.
Never retry a failed or unconfirmed write automatically; acknowledgement is not a readback or persistence check.

### Case: List Memory Channels with Python

Use a Python interpreter supported by [`bindings/python/pyproject.toml`](bindings/python/pyproject.toml) and the Rust toolchain declared in [`rust-toolchain.toml`](rust-toolchain.toml).
From the repository root, build and install the bindings into a virtual environment:

```sh
python3 -m venv bindings/python/target/venv
PYTHON="$PWD/bindings/python/target/venv/bin/python"
"$PYTHON" -m pip install ./bindings/python
```

List the saved logical channels in a canonical MapRev 6 ICF file without radio I/O.
Names remain bytes; no text encoding is assumed.

```python
from pathlib import Path
from cs705 import Icf
from cs705.icf import MemorySide

image = Icf.parse(Path("settings.icf").read_bytes())
for group, bank in enumerate(image.memory_groups()):
    for row, slot in enumerate(bank.slots()):
        if slot is not None:
            hz = image.physical_memory_frequency_hz(slot, MemorySide.Main)
            name = image.physical_memory_name_raw(slot)
            print(f"{group:02}/{row:02} {hz} Hz {name!r}")
```

To check the native bindings and the installed package, run:

```sh
make quality TARGETS=python PYTHON="$PYTHON"
make test TARGETS=python TESTS=wheel PYTHON="$PYTHON"
```

The Makefile defaults to the Rust library; use `TARGETS=rust,python` to check both components.
If Cargo's offline checks lack dependencies, run `cargo fetch --locked --manifest-path bindings/python/Cargo.toml` first.

## License

Licensed under either the [MIT](https://github.com/mi2428/cs705-rs/blob/main/LICENSE-MIT) or the [Apache-2.0](https://github.com/mi2428/cs705-rs/blob/main/LICENSE-APACHE), at your option.

>[!IMPORTANT]
> **Reverse Engineering Notice:**
> This is an unofficial, cross-platform Rust library for the Icom IC-705.
> It was written independently using public documentation, observations of the radio's behavior, and reverse engineering of the CS-705 release listed below:
> 
> - **Software:** [Icom CS-705](https://www.icomjapan.com/support/firmware_driver/4423/)
> - **Version and Release Date:** v1.30 (August 21, 2025)
> 
> Icom, IC-705, and CS-705 are trademarks or product names of their respective owners.
