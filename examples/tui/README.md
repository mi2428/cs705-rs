# TUI Example

A Ratatui example that edits IC-705 settings through the [`cs705`](../../README.md) library.

```console
$ cargo run --locked -- --help

IC-705 settings editing session; Ctrl+S reviews and saves all staged changes

Usage: cs705-tui [OPTIONS] --backup-dir <BACKUP_DIR>

Options:
      --encoding <ENCODING>      Encoding of stored names/text, not the UTF-8 terminal (cp932 is accepted) [default: shift_jis]
      --file <FILE>              Canonical MapRev 5 or 6 ICF; no device is opened in file mode
      --port <PORT>              Explicit serial endpoint. OS open can transiently assert DTR/RTS
      --allow-device-io
      --acknowledge-line-risk
      --backup-dir <BACKUP_DIR>  Private backup and edited-file destination; originals are never overwritten
      --baud <BAUD>              [default: 115200]
      --timeout-ms <TIMEOUT_MS>  [default: 1000]
  -h, --help                     Print help
  -V, --version                  Print version
```

[![](screenshot.png)](screenshot.png)

### How It Works

- [`app.rs`](src/app.rs) runs the terminal event loop and sends file/radio jobs to a single background I/O thread.
- [`model.rs`](src/model.rs) keeps the original snapshot and a local draft, using the library's memory and settings APIs.
- [`storage.rs`](src/storage.rs) checks that the input file has not changed, writes a backup, and saves a new ICF without overwriting the source.
- [`radio.rs`](src/radio.rs) uses library clone sessions with a fresh-read conflict check and a pre-write backup.
  Writes are not retried automatically.

