# CS-705

[![Library](https://github.com/mi2428/cs705-rs/actions/workflows/library.yml/badge.svg?branch=main)](https://github.com/mi2428/cs705-rs/actions/workflows/library.yml)
[![Examples](https://github.com/mi2428/cs705-rs/actions/workflows/examples.yml/badge.svg?branch=main)](https://github.com/mi2428/cs705-rs/actions/workflows/examples.yml)
[![Releases](https://github.com/mi2428/cs705-rs/actions/workflows/release.yml/badge.svg)](https://github.com/mi2428/cs705-rs/actions/workflows/release.yml)

A native Rust library for reading and editing ICOM IC-705 configuration files and handling clone protocols.
Use it to build your own configuration tools without running the CS-705 Windows application or loading ICOM EXE/DLL files at runtime.

- Refer to the [API documentation](https://docs.rs/cs705) for public interfaces and their contracts.
- Read [`NOTES.md`](https://github.com/mi2428/cs705-rs/blob/main/NOTES.md) for technical details of the recovered formats, editing behavior, and clone protocol.
- The [`examples/cli`](https://github.com/mi2428/cs705-rs/tree/main/examples/cli) and [`example/tui`](https://github.com/mi2428/cs705-rs/tree/main/examples/tui) folders contain separate applications built on the library.

## Features

- ICF parsing, serialization, and explicit layout conversion. Typed editing of memories, radio settings, repeater lists, GPS memories, and callsigns.
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

## License

Licensed under either the [MIT](https://github.com/mi2428/cs705-rs/blob/main/LICENSE-MIT) or the [Apache 2.0](https://github.com/mi2428/cs705-rs/blob/main/LICENSE-APACHE), at your option.

>[!IMPORTANT]
> **Reverse Engineering Notice:**
> This is an unofficial, cross-platform Rust library for the Icom IC-705.
> It was written independently using public documentation, observations of the radio's behavior, and reverse engineering of the CS-705 release listed below:
> 
> - **Software:** [Icom CS-705](https://www.icomjapan.com/support/firmware_driver/4423/)
> - **Version and Release Date:** v1.30 (August 21, 2025)
> 
> Icom, IC-705, and CS-705 are trademarks or product names of their respective owners.
