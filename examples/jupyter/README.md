# Jupyter Example

A notebook for exploring memory channels with Pandas and plotting stored repeater coordinates with Matplotlib, using the `cs705` Python bindings.

### Setup

Install [Rust](https://rustup.rs/) and use a Python interpreter supported by the [package metadata](../../bindings/python/pyproject.toml).
Rustup selects the toolchain declared in [`rust-toolchain.toml`](../../rust-toolchain.toml).
From the repository root, create a virtual environment, install the bindings and notebook dependencies, and start JupyterLab:

```console
$ python3 -m venv bindings/python/target/venv
$ PYTHON="$PWD/bindings/python/target/venv/bin/python"
$ "$PYTHON" -m pip install ./bindings/python jupyterlab pandas matplotlib
$ "$PYTHON" -m jupyterlab --no-browser examples/jupyter/memory-analysis.ipynb
```

### How It Works

- [`memory-analysis.ipynb`](memory-analysis.ipynb) contains both examples and their rendered results from synthetic data.
- The folded setup cell builds a fictional ICF through the native binding; it needs no external data files.
- Memory cells resolve logical rows to physical slots, then use a DataFrame to filter, group, and find repeated Main/RX frequency/mode pairs.
  Stored names remain bytes rather than being decoded with an assumed encoding.
- Repeater cells convert signed coordinates to decimal degrees and plot frequency as color, distinguishing exact and approximate positions by marker shape.
  Missing positions are counted and omitted; `GROUP` and `MODE` in the final cell filter the plot.
- Backups are read without modification, and plotted positions do not imply radio coverage.
