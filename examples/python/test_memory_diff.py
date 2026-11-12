"""Exercise the example against the installed native binding and synthetic images."""

# pyright: strict

import io
import os
import subprocess
import sys
import unittest
from contextlib import chdir, redirect_stderr, redirect_stdout
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch

from cs705 import Icf
from cs705.csv import ErrorData
from cs705.csv.channel import Export, Failure
from cs705.icf.format import Layout, write_storage
from cs705.icf.record import ChannelField, reset_holder
from cs705.icf.rows import NormalMemory

from memory_diff import csv_lines, main


def sample_image() -> Icf:
    """Seed only an empty format envelope; let native row APIs build the topology."""
    working = bytearray(0x4997A)
    working[0x355A1] = 6
    working[0x3C9B0:0x3C9C0] = b"IcomCloneFormat3"
    working[0x3662D : 0x3662D + 500] = b"\x01" * 500
    # The expanded map uses DWORD 0x0000ffff, not an all-ones DWORD (NOTES.md 2.1).
    working[0x3FCD4 : 0x3FCD4 + 40_000] = (0xFFFF).to_bytes(4, "little") * 10_000
    model = NormalMemory.from_working(bytes(working))
    assert model.add_group()
    assert model.allocate(0, range(1)).allocated == 1
    holder = reset_holder(2, 0, 0)
    holder.set_valid(True)
    holder.set_mode(13)
    holder.set_name(b"LOCAL".ljust(16))
    holder.set_channel_field(ChannelField.FrequencyHz, 145_500_000)
    model.store_channel(0, holder)
    return model.to_icf()


class MemoryDiffTests(unittest.TestCase):
    """Check useful comparisons, byte fidelity, and failure publication boundaries."""

    def setUp(self) -> None:
        """Create disposable inputs without any original executable or radio data."""
        temporary = TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.before = self.directory / "before.icf"
        self.after = self.directory / "after.icf"
        self.image = sample_image()
        self.source = self.image.to_bytes()
        self.before.write_bytes(self.source)
        self.after.write_bytes(self.source)

    def invoke(self, *arguments: str) -> tuple[int, str, str]:
        """Capture only this invocation's text streams, including argparse exits."""
        output, errors = io.StringIO(), io.StringIO()
        with redirect_stdout(output), redirect_stderr(errors):
            try:
                status = main(arguments)
            except SystemExit as error:
                assert isinstance(error.code, int)
                status = error.code
        return status, output.getvalue(), errors.getvalue()

    def test_projection_not_full_image(self) -> None:
        """Ignore unexported data without implying image equality."""
        self.image.set_scope_mode_raw(2)
        self.after.write_bytes(
            write_storage(
                self.image.memory(), Layout.from_map_revision(6), b"new comment", 0
            )
        )
        self.assertNotEqual(self.source, self.after.read_bytes())
        self.assertEqual(self.invoke(str(self.before), str(self.after)), (0, "", ""))

    def test_native_memory_change(self) -> None:
        """Report a real channel edit without changing either input file."""
        model = NormalMemory.from_icf(self.image)
        self.assertTrue(model.rename_group(0, b"Renamed group"))
        self.image = model.to_icf()
        self.image.set_physical_memory_name_ascii(0, "EDITED")
        self.after.write_bytes(self.image.to_bytes())
        after = self.after.read_bytes()
        status, output, errors = self.invoke(str(self.before), str(self.after))
        self.assertEqual(status, 1)
        self.assertEqual(errors, "")
        self.assertIn("@@", output)
        self.assertIn("LOCAL", output)
        self.assertIn("EDITED", output)
        self.assertIn("Renamed group", output)
        self.assertEqual(self.before.read_bytes(), self.source)
        self.assertEqual(self.after.read_bytes(), after)

    def test_input_errors_publish_nothing(self) -> None:
        """Reject missing, malformed, and incomplete arguments before output."""
        self.after.write_bytes(b"not an ICF file")
        for arguments in [
            (str(self.before), str(self.after)),
            (str(self.directory / "missing.icf"), str(self.before)),
            (str(self.before),),
        ]:
            with self.subTest(arguments=arguments):
                status, output, errors = self.invoke(*arguments)
                self.assertEqual((status, output), (2, ""))
                self.assertTrue(errors)

    def test_failed_export_discards_prefix(self) -> None:
        """Keep a native report's partial bytes out of the comparison."""
        report = Export(b"partial", 1, Failure.Text(ErrorData.Invalid("export failed")))
        with patch("memory_diff.export_memory", return_value=report):
            status, output, errors = self.invoke(str(self.before), str(self.after))
        self.assertEqual((status, output), (2, ""))
        self.assertIn("export failed", errors)

    def test_byte_escapes_are_unambiguous(self) -> None:
        """Distinguish raw bytes, literal escapes, and line endings losslessly."""
        payloads = [
            b"\x82\xa0\x00\x1b\r\n",
            b"\\x82\\xa0\\x00\\x1b\r\n",
            b"\x82\xa0\x00\x1b\n",
        ]
        rendered: list[str] = []
        for payload in payloads:
            with patch(
                "memory_diff.export_memory", return_value=Export(payload, 1, None)
            ):
                lines = csv_lines(self.before)
            self.assertTrue(all(line.isascii() for line in lines))
            self.assertNotIn("\x1b", "".join(lines))
            self.assertEqual(b"".join(ast_line(line) for line in lines), payload)
            rendered.append("".join(lines))
        self.assertEqual(len(set(rendered)), len(payloads))

    def test_closed_output_pipe(self) -> None:
        """Report a closed consumer without a second shutdown-flush exception."""
        self.image.set_physical_memory_name_ascii(0, "EDITED")
        self.after.write_bytes(self.image.to_bytes())
        reader, writer = os.pipe()
        os.close(reader)
        with os.fdopen(writer, "wb") as output:
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).with_name("memory_diff.py")),
                    str(self.before),
                    str(self.after),
                ],
                stdout=output,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
                timeout=10,
            )
        self.assertEqual(result.returncode, 2)
        self.assertIn("cannot write comparison", result.stderr)
        self.assertNotIn("Traceback", result.stderr)

    def test_readme_snippet(self) -> None:
        """Keep the short public snippet executable against the real binding."""
        readme = Path(__file__).resolve().parents[2] / "README.md"
        snippet = readme.read_text(encoding="ascii").split("```python\n", 1)[1]
        snippet = snippet.split("\n```", 1)[0]
        (self.directory / "settings.icf").write_bytes(self.source)
        output = io.StringIO()
        with chdir(self.directory), redirect_stdout(output):
            exec(compile(snippet, "README.md", "exec"), {})
        self.assertIn("145500000 Hz", output.getvalue())
        self.assertIn("LOCAL", output.getvalue())
        self.assertEqual((self.directory / "settings.icf").read_bytes(), self.source)


def ast_line(line: str) -> bytes:
    """Read the example's own bytes literal, never execute source from an input file."""
    import ast

    value = ast.literal_eval(line)
    assert isinstance(value, bytes)
    return value


if __name__ == "__main__":
    unittest.main()
