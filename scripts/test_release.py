"""Release contracts, using independent minimal Mach-O fixtures and a fake gh adapter."""

import hashlib
import io
import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import release


def executable(*, cpu=0x0100000C, minimum=0x000D0000, library=b"/usr/lib/libSystem.B.dylib"):
    build = struct.pack("<6I", 0x32, 24, 1, minimum, 0x000E0000, 0)
    name = library + b"\0"
    size = (24 + len(name) + 7) // 8 * 8
    dylib = struct.pack("<6I", 0xC, size, 24, 0, 0, 0) + name.ljust(size - 24, b"\0")
    commands = build + dylib
    return struct.pack("<8I", 0xFEEDFACF, cpu, 0, 2, 2, len(commands), 0, 0) + commands


class ReleaseContracts(unittest.TestCase):
    def test_tag_is_stable_and_cannot_be_an_arbitrary_ref_or_shell_fragment(self):
        self.assertEqual("1.2.3", release.version_from_tag("v1.2.3"))
        for tag in ("main", "v1.2", "v01.2.3", "v1.2.3-beta", "v1.2.3;bad", "v1.2.3\n"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release.version_from_tag(tag)

    def test_cli_rejects_tag_that_does_not_match_the_source(self):
        result = subprocess.run([
            sys.executable, str(release.ROOT / "scripts/release.py"), "check-version", "--tag", "v999.0.0",
        ], capture_output=True, text=True)
        self.assertEqual(1, result.returncode)
        self.assertIn("differs from Cargo.toml", result.stderr)

    def test_arm64_package_can_only_use_the_declared_os_floor_and_system_libraries(self):
        release.verify_macho(executable())
        for data in (
            executable(cpu=0x01000007), executable(minimum=0x000E0000),
            executable(library=b"/opt/homebrew/lib/libforeign.dylib"),
            executable(library=b"@rpath/libforeign.dylib"), executable()[:-1], b"MZnot-a-Mach-O",
        ):
            with self.subTest(data=data[:24]), self.assertRaises(ValueError):
                release.verify_macho(data)

    def test_truncated_load_command_does_not_pass_validation(self):
        data = bytearray(executable())
        struct.pack_into("<I", data, 36, 4)
        with self.assertRaisesRegex(ValueError, "length"):
            release.verify_macho(data)

    def test_package_contains_only_executable_and_licenses_and_checksums_real_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "binary"
            binary.write_bytes(executable())
            archive, digest = release.package(binary, "0.1.0", root / "dist")
            self.assertEqual(hashlib.sha256(archive.read_bytes()).hexdigest(), digest)
            self.assertEqual(f"{digest}  {archive.name}\n", (archive.parent / "SHA256SUMS").read_text())
            with tarfile.open(archive) as package:
                members = package.getmembers()
                self.assertEqual(["novelite-tui", "LICENSE", "THIRD_PARTY_NOTICES.md"], [Path(item.name).name for item in members])
                self.assertEqual(0o755, members[0].mode)
                self.assertEqual(executable(), package.extractfile(members[0]).read())
            formula = (archive.parent / "novelite-tui.rb").read_text()
            self.assertIn(f'sha256 "{digest}"', formula)
            self.assertIn("/releases/download/v0.1.0/novelite-tui-v0.1.0-aarch64-apple-darwin.tar.gz", formula)
            self.assertIn("depends_on arch: :arm64", formula)
            self.assertNotIn('depends_on "rust"', formula)
            self.assertNotIn("file://", formula)

    def test_archive_rejects_links_and_extra_paths_without_extracting_them(self):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / release.archive_name("0.1.0")
            with tarfile.open(archive, "w:gz") as package:
                member = tarfile.TarInfo("../escaped")
                member.size = 1
                package.addfile(member, io.BytesIO(b"x"))
            with self.assertRaises(ValueError):
                release.verify_archive(archive, "0.1.0")
            self.assertFalse((archive.parent.parent / "escaped").exists())

    def test_smoke_url_is_local_and_cannot_change_the_production_url(self):
        self.assertIn('url "file:///tmp/local.tar.gz"', release.render_formula("0.1.0", "a" * 64, "file:///tmp/local.tar.gz"))
        for url in ("https://elsewhere.invalid/file", "file://relative", 'file:///tmp/#{danger}.tar.gz'):
            if "#{" in url:
                self.assertIn(r"\#{danger}", release.render_formula("0.1.0", "a" * 64, url))
            else:
                with self.assertRaises(ValueError):
                    release.render_formula("0.1.0", "a" * 64, url)
        with self.assertRaises(ValueError):
            release.render_formula("0.1.0", "placeholder")

    def update_fixture(self, directory, *, broken_checksum=False, draft=False):
        root = Path(directory)
        binary = root / "binary"
        binary.write_bytes(executable())
        archive, digest = release.package(binary, "0.1.0", root / "release")
        tap = root / "tap"
        (tap / "Formula").mkdir(parents=True)
        formula = tap / "Formula/novelite-tui.rb"
        formula.write_text("original formula\n")
        metadata = {"tagName": "v0.1.0", "isDraft": draft, "isPrerelease": False,
                    "assets": [{"name": archive.name, "size": archive.stat().st_size}, {"name": "SHA256SUMS", "size": 200}]}

        def download(command, *, check):
            self.assertTrue(check)
            self.assertEqual(["gh", "release", "download", "v0.1.0"], command[:4])
            destination = Path(command[command.index("--dir") + 1])
            shutil.copyfile(archive, destination / archive.name)
            checksum = "b" * 64 if broken_checksum else digest
            (destination / "SHA256SUMS").write_text(f"{checksum}  {archive.name}\n")

        return tap, formula, metadata, download, digest

    def test_tap_update_downloads_and_verifies_a_published_release_before_replacement(self):
        with tempfile.TemporaryDirectory() as directory:
            tap, formula, metadata, download, digest = self.update_fixture(directory)
            with patch("release.subprocess.check_output", return_value=json.dumps(metadata)), patch("release.subprocess.run", side_effect=download):
                release.update_tap("v0.1.0", tap)
            self.assertIn(f'sha256 "{digest}"', formula.read_text())
            self.assertIn('version "0.1.0"', formula.read_text())
            self.assertNotIn("file://", formula.read_text())

    def test_corrupt_download_never_changes_the_existing_tap(self):
        with tempfile.TemporaryDirectory() as directory:
            tap, formula, metadata, download, _ = self.update_fixture(directory, broken_checksum=True)
            with patch("release.subprocess.check_output", return_value=json.dumps(metadata)), patch("release.subprocess.run", side_effect=download):
                with self.assertRaisesRegex(ValueError, "checksum"):
                    release.update_tap("v0.1.0", tap)
            self.assertEqual("original formula\n", formula.read_text())

    def test_draft_release_cannot_become_an_installable_formula(self):
        with tempfile.TemporaryDirectory() as directory:
            tap, formula, metadata, _, _ = self.update_fixture(directory, draft=True)
            with patch("release.subprocess.check_output", return_value=json.dumps(metadata)), patch("release.subprocess.run") as download:
                with self.assertRaisesRegex(ValueError, "published stable"):
                    release.update_tap("v0.1.0", tap)
                download.assert_not_called()
            self.assertEqual("original formula\n", formula.read_text())


if __name__ == "__main__":
    unittest.main()
