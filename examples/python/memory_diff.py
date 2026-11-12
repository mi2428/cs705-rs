"""Compare normal-memory CSV projections of two canonical MapRev 6 ICF backups.

Read-only: this is neither a full-image diff nor a radio-compatibility check.
"""

# pyright: strict

import argparse
import os
import sys
from collections.abc import Sequence
from difflib import unified_diff
from pathlib import Path

from cs705 import Error, Icf
from cs705.csv import Locale
from cs705.csv.channel import Kind, export_memory


def csv_lines(path: Path) -> list[str]:
    """Export completely, then render each byte line without guessing an encoding."""
    image = Icf.parse(path.read_bytes())
    report = export_memory(image, Kind.Memory, None, None, Locale(ord("."), ord(",")))
    if (failure := report.failure) is not None:
        # A failed native export can contain a prefix; never publish that prefix.
        raise failure.error
    # repr distinguishes an encoded byte from a literal backslash escape and
    # prevents stored control bytes from reaching the terminal as commands.
    return [f"{line!r}\n" for line in report.bytes.splitlines(keepends=True)]


def main(argv: Sequence[str] | None = None) -> int:
    """Return 0 for equal projections, 1 for differences, and 2 for errors."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path, help="earlier ICF backup")
    parser.add_argument("after", type=Path, help="later ICF backup")
    args = parser.parse_args(argv)
    before: Path = args.before
    after: Path = args.after
    snapshots: list[list[str]] = []
    for path in (before, after):
        try:
            snapshots.append(csv_lines(path))
        except (OSError, Error) as error:
            parser.exit(2, f"{parser.prog}: {str(path)!a}: {error}\n")
    difference = list(
        unified_diff(
            *snapshots,
            fromfile=ascii(str(before)),
            tofile=ascii(str(after)),
        )
    )
    try:
        sys.stdout.writelines(difference)
        sys.stdout.flush()
    except OSError as error:
        # Avoid a second buffered-write error during interpreter shutdown.
        with open(os.devnull, "wb") as sink:
            os.dup2(sink.fileno(), sys.stdout.fileno())
        parser.exit(2, f"{parser.prog}: cannot write comparison: {error}\n")
    return int(bool(difference))


if __name__ == "__main__":
    raise SystemExit(main())
