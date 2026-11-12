# Python bindings

An in-process CPython 3.15+ interface to the Rust library. Domain operations
run in Rust; bytes remain bytes, and callbacks and I/O sources are explicit.
No ICOM executable, original UI resources, radio, or serial port is needed
to build or run the tests below.

## Build and install from this checkout

Use Rust 1.99.1 (the repository toolchain), CPython 3.15+, and the declared
build backend, maturin 1.16.0. On macOS, use one Python installation for
the venv, PyO3, and the wheel consumer; do not mix Homebrew and Nix Python.
From the repository root:

```sh
python3.15 -m venv bindings/python/target/venv
PYTHON="$PWD/bindings/python/target/venv/bin/python"
"$PYTHON" -m pip install 'maturin==1.16.0'
# Only needed when the locked Rust dependencies are not already cached:
cargo fetch --locked --manifest-path bindings/python/Cargo.toml
make -C bindings/python wheel PYTHON="$PYTHON"
"$PYTHON" -m pip install --no-deps --force-reinstall bindings/python/target/wheels/cs705-1.0.0-*.whl
"$PYTHON" -c 'import cs705; print(cs705.__version__)'
```

On Homebrew, replace the first `python3.15` with
`/opt/homebrew/bin/python3.15` if it is not already on PATH. The wheel is
platform-specific. Build and install a real wheel; do not rename or directly
load a Cargo-produced dynamic library. The source package is not importable
until the native extension has been built and installed.

## Checks

With `PYTHON` still pointing to the venv above:

```sh
make -C bindings/python quality PYTHON="$PYTHON"
make -C bindings/python test-python PYTHON="$PYTHON"
```

`quality` checks Rust formatting, compilation, Clippy, all Rust test targets,
and rustdoc with warnings rejected. Cargo commands default to
`--offline --locked`; `CARGO_FLAGS` and `TARGET_DIR` can be overridden.
`test-python` runs the existing smoke scripts and package self-checks against
the installed wheel. Rebuild and reinstall after changing Rust or the facade.
Do not use `python -O`: these checks use assertions.

## API and limits

Import `cs705` and its typed submodules; `Icf` is also available as
`cs705.Icf`. Public `.pyi` files and `py.typed` ship with the package.
The [API ledger](api-ledger.md) is a historical Rust inventory, not a current
completeness claim. Its `pending` rows are not an unimplemented-operation
count. Some Rust operations use Python-specific constructors or explicit
holder types; availability alone does not prove equivalent behavior.

The headless checks do not validate real radio I/O, OS UI delivery, original
resource assets, other operating systems, or free-threaded CPython behavior.
The parent project's [license and reverse-engineering notice](../../README.md#license)
also apply to these bindings.
The wheel carries `MIT OR Apache-2.0` metadata and both license texts. Local
license copies must match the canonical files at the repository root;
`make license-check` checks this before building a wheel.
