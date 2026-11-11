# CLI Example

An example command-line application for inspecting, editing, and transferring IC-705 configuration data with the [`cs705`](../../README.md) library.

```console
$ cargo run --locked -- --help

Native CS-705 configuration tools.
Inspect and edit files offline, or use clone subcommands for radio transfers. Indices are zero-based.

Usage: cs705 [OPTIONS] <COMMAND>

Commands:
  item-help    Browse independent English help; inspect original resources only as an optional oracle
  information  Drive information and COM controls from explicit source observations
  print        Print typed sheets and fields with independent English labels; an oracle is optional
  document     Execute native Document open, New, normalization and Save operations
  lifecycle    Drive native save dialogs, recent files and document lifecycle state
  display      Show typed views and display settings; source-state operations are an optional oracle
  editor       Invoke native compound setting editors with explicit runtime sources
  repeater     Invoke native Repeater field handlers with actual Q/B/G sources
  special      Edit and inspect special-memory, scan-edge and preset records
  recent       Open a zero-based entry from the explicit config's recent-files list
  parse        Parse an input document and report its structure
  check        Validate multiple documents, reporting every failure (exit 5 if mixed)
  show         Show semantic fields or a bounded sheet view; unknown values stay errors
  edit         Apply semantic edits; publish only after actual approval and no failures
  new          Create a native document from an explicit source generation and state
  convert      Convert an explicitly selected representation or revision
  import       Import CSV with explicit target and partial-update publication policy
  export       Export a sheet as CSV or a print-ready terminal/JSON report
  group        List/rename logical groups without confusing them with physical slots
  row          Copy/paste/move/clear/delete/insert rows using explicit sheet coordinates
  clone        Clone tools. Live I/O requires explicit port and safety acknowledgement
  config       Explicit application preferences, stored only in the requested file
  help         Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>
          Explicit application preference file; no implicit home-directory state

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

### How It Works

- Handlers call the library for ICF parsing, typed edits, CSV conversion, and clone sessions.
- [`io.rs`](src/io.rs) keeps payloads on stdout and diagnostics on stderr, and handles atomic file output.
  Existing output files are not replaced unless `--overwrite` is given.
- [`transport.rs`](src/transport.rs) adapts serial ports and recorded transcripts to the library's `clone::Transport` interface.
