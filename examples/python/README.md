# Python example: compare memory backups

`memory_diff.py` is a small, read-only tool for reviewing normal-memory changes
between two canonical MapRev 6 ICF backups. Python supplies argument parsing and
`difflib`; the Rust library supplies ICF parsing and the memory CSV projection.
There is no duplicate format decoder, radio connection, or configuration state.

## Run

First [build and install the Python bindings](../../bindings/python/README.md#build-and-install-from-this-checkout)
into a CPython 3.15+ environment. From the repository root, using that environment:

```sh
PYTHON="$PWD/bindings/python/target/venv/bin/python"
"$PYTHON" examples/python/memory_diff.py before.icf after.icf
```

The tool prints a unified diff to stdout: removed lines begin with `-`, added lines
with `+`. Exit status is 0 for equal projections, 1 for differences, and 2 for
input/export/output errors. Both exports must succeed before anything is printed;
a native report containing partial bytes is an error, not a usable snapshot.
Neither input file is modified, even on failure. Paths and stored bytes are
escaped for terminal display; no UTF-8 or SJIS decoding is guessed, and a literal
backslash sequence stays distinct from an encoded byte.

Comparison uses exactly `csv.channel.export_memory(..., Kind.Memory, ...)` with
decimal `.` and separator `,`. It is **not a full-image comparison**: other sheets,
radio settings, header comments, and data not represented by that
CSV projection are outside its scope. Equality is not evidence that two radio
configurations are interchangeable. Reordering visible rows is a change; raw
physical slot numbers are not compared. No original executable or proprietary
UI assets are needed.

## Check

Runtime needs only the installed `cs705` package and the Python standard library.
Development checks additionally use Ruff and Pyright:

```sh
make -C examples/python quality PYTHON="$PYTHON"
```

The tests use synthetic images and the actual installed extension, exercise
byte escaping and failed exports, and execute the root README's Python snippet.
They use standard-library `unittest`, not a separate test framework.
