#!/usr/bin/env python3
"""Prepare Apple Silicon releases and update the Homebrew tap without extra tokens."""

import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import struct
import subprocess
import tarfile
import tempfile
import tomllib

REPOSITORY = "hirovel/novelite-tui"
TARGET = "aarch64-apple-darwin"
MINIMUM_MACOS = (13, 0, 0)
ROOT = Path(__file__).resolve().parents[1]


def version_from_tag(tag):
    if not re.fullmatch(r"v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", tag):
        raise ValueError("Use a stable version tag such as v0.1.0")
    return tag[1:]


def archive_name(version):
    version_from_tag(f"v{version}")
    return f"novelite-tui-v{version}-{TARGET}.tar.gz"


def verify_macho(data):
    """Reject another architecture, a raised OS floor, or non-system linkage."""
    if len(data) < 32:
        raise ValueError("Truncated Mach-O executable")
    magic, cpu, _, filetype, count, size, _, _ = struct.unpack_from("<8I", data)
    if (magic, cpu, filetype) != (0xFEEDFACF, 0x0100000C, 2):
        raise ValueError("Expected an arm64 Mach-O executable")
    end = 32 + size
    if end > len(data):
        raise ValueError("Truncated Mach-O load commands")
    offset = 32
    minimum = None
    for _ in range(count):
        if offset + 8 > end:
            raise ValueError("Truncated Mach-O load command")
        command, length = struct.unpack_from("<2I", data, offset)
        if length < 8 or offset + length > end:
            raise ValueError("Invalid Mach-O load command length")
        if command == 0x32:  # LC_BUILD_VERSION
            if length < 24:
                raise ValueError("Truncated Mach-O build version")
            platform, minimum = struct.unpack_from("<2I", data, offset + 8)
            if platform != 1:
                raise ValueError("Expected the macOS platform")
        elif command == 0x24:  # LC_VERSION_MIN_MACOSX
            if length < 16:
                raise ValueError("Truncated Mach-O minimum version")
            minimum = struct.unpack_from("<I", data, offset + 8)[0]
        elif command in (0xC, 0x80000018, 0x8000001F, 0x80000023):
            if length < 24:
                raise ValueError("Truncated Mach-O dylib command")
            name_offset = struct.unpack_from("<I", data, offset + 8)[0]
            if not 24 <= name_offset < length:
                raise ValueError("Invalid Mach-O dylib name")
            name = data[offset + name_offset:offset + length].split(b"\0", 1)[0]
            if not name.startswith((b"/usr/lib/", b"/System/Library/")):
                raise ValueError(f"Non-system dynamic dependency: {name!r}")
        offset += length
    if offset != end or minimum is None:
        raise ValueError("Missing or invalid Mach-O macOS deployment target")
    actual = (minimum >> 16, (minimum >> 8) & 255, minimum & 255)
    if actual > MINIMUM_MACOS:
        raise ValueError(f"Deployment target {actual} exceeds {MINIMUM_MACOS}")


def render_formula(version, checksum, url=None):
    name = archive_name(version)
    if not re.fullmatch(r"[0-9a-f]{64}", checksum):
        raise ValueError("Expected a lowercase SHA-256 checksum")
    if url is None:
        url = f"https://github.com/{REPOSITORY}/releases/download/v{version}/{name}"
    elif not url.startswith("file:///"):
        raise ValueError("A smoke-test URL must be an absolute local file URI")
    quoted_url = json.dumps(url).replace("#{", "\\#{")
    return f'''class NoveliteTui < Formula
  desc "Focused writing in your terminal"
  homepage "https://github.com/{REPOSITORY}"
  url {quoted_url}
  version "{version}"
  sha256 "{checksum}"
  license "GPL-3.0-only"

  depends_on arch: :arm64
  depends_on macos: :ventura

  def install
    bin.install "novelite-tui"
    doc.install "LICENSE", "THIRD_PARTY_NOTICES.md"
  end

  test do
    assert_equal "Novelite TUI #{{version}}", shell_output("#{{bin}}/novelite-tui --version").strip
    assert_match "Usage: novelite-tui", shell_output("#{{bin}}/novelite-tui --help")
    assert_match "Interactive terminal required",
                 shell_output("#{{bin}}/novelite-tui #{{testpath}}/draft.md 2>&1", 1)
    refute_path_exists testpath/"draft.md"
  end
end
'''


def verify_archive(path, version):
    prefix = path.name.removesuffix(".tar.gz")
    if path.name != archive_name(version):
        raise ValueError("Unexpected release archive name")
    expected = {f"{prefix}/{name}" for name in ("novelite-tui", "LICENSE", "THIRD_PARTY_NOTICES.md")}
    with tarfile.open(path, "r:gz") as archive:
        members = archive.getmembers()
        if {member.name for member in members} != expected or len(members) != 3:
            raise ValueError("Archive must contain only the executable and license notices")
        for member in members:
            if not member.isfile() or member.size > 64 * 1024 * 1024:
                raise ValueError("Archive has an invalid member")
            if member.name.endswith("/novelite-tui"):
                if member.mode & 0o111 != 0o111:
                    raise ValueError("Release executable must be executable")
                verify_macho(archive.extractfile(member).read())


def package(binary, version, output):
    executable = binary.read_bytes()
    verify_macho(executable)
    output.mkdir(parents=True, exist_ok=True)
    path = output / archive_name(version)
    prefix = path.name.removesuffix(".tar.gz")
    files = [("novelite-tui", executable, 0o755)]
    files += [(name, (ROOT / name).read_bytes(), 0o644) for name in ("LICENSE", "THIRD_PARTY_NOTICES.md")]
    with path.open("wb") as destination, gzip.GzipFile(fileobj=destination, mode="wb", mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode="w") as archive:
            for name, data, mode in files:
                member = tarfile.TarInfo(f"{prefix}/{name}")
                member.size, member.mode = len(data), mode
                archive.addfile(member, io.BytesIO(data))
    verify_archive(path, version)
    checksum = hashlib.sha256(path.read_bytes()).hexdigest()
    (output / "SHA256SUMS").write_text(f"{checksum}  {path.name}\n", encoding="utf-8")
    (output / "novelite-tui.rb").write_text(render_formula(version, checksum), encoding="utf-8")
    return path, checksum


def update_tap(tag, tap):
    version = version_from_tag(tag)
    formula = tap.resolve(strict=True) / "Formula" / "novelite-tui.rb"
    if (formula.parent.exists() and not formula.parent.is_dir()) or formula.parent.is_symlink() or formula.is_symlink():
        raise ValueError("Use a tap with a real Formula directory")
    if formula.exists():
        previous = re.search(r'^  version "(\d+\.\d+\.\d+)"$', formula.read_text(encoding="utf-8"), re.MULTILINE)
        if previous and tuple(map(int, previous[1].split("."))) > tuple(map(int, version.split("."))):
            raise ValueError("Refusing to downgrade the tap")
    release = json.loads(subprocess.check_output([
        "gh", "release", "view", tag, "--repo", REPOSITORY,
        "--json", "tagName,isDraft,isPrerelease,assets",
    ], text=True))
    if release["tagName"] != tag or release["isDraft"] or release["isPrerelease"]:
        raise ValueError("Only a published stable release can update the tap")
    names = [asset["name"] for asset in release["assets"]]
    for required in (archive_name(version), "SHA256SUMS"):
        if names.count(required) != 1:
            raise ValueError(f"Missing or duplicated release asset: {required}")
    if any(asset["size"] > 64 * 1024 * 1024 for asset in release["assets"] if asset["name"] == archive_name(version)):
        raise ValueError("Release archive exceeds the size limit")
    with tempfile.TemporaryDirectory(prefix="novelite-release-") as directory:
        subprocess.run([
            "gh", "release", "download", tag, "--repo", REPOSITORY,
            "--dir", directory, "--pattern", archive_name(version), "--pattern", "SHA256SUMS",
        ], check=True)
        archive = Path(directory) / archive_name(version)
        checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
        expected = f"{checksum}  {archive.name}\n"
        if (Path(directory) / "SHA256SUMS").read_text(encoding="utf-8") != expected:
            raise ValueError("Release checksum mismatch; tap was not changed")
        verify_archive(archive, version)
        contents = render_formula(version, checksum)
    # Validate everything before replacing the sole product file; never commit or push implicitly.
    formula.parent.mkdir(exist_ok=True)
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", newline="\n", dir=formula.parent, delete=False) as draft:
        draft.write(contents)
        staged = Path(draft.name)
    try:
        staged.replace(formula)
    finally:
        staged.unlink(missing_ok=True)
    print(f"Updated {formula.name} to {tag}; review and commit the tap change.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    check = commands.add_parser("check-version")
    check.add_argument("--tag", required=True)
    build = commands.add_parser("package")
    build.add_argument("--tag", required=True)
    build.add_argument("--binary", type=Path, required=True)
    build.add_argument("--output", type=Path, required=True)
    smoke = commands.add_parser("smoke-formula")
    smoke.add_argument("--tag", required=True)
    smoke.add_argument("--archive", type=Path, required=True)
    smoke.add_argument("--output", type=Path, required=True)
    tap = commands.add_parser("update-tap")
    tap.add_argument("--tag", required=True)
    tap.add_argument("--tap", type=Path, required=True)
    args = parser.parse_args()
    try:
        version = version_from_tag(args.tag)
        if args.command == "check-version":
            declared = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
            if declared != version:
                raise ValueError(f"Tag version differs from Cargo.toml ({declared})")
            print(f"tag={args.tag}\nversion={version}")
        elif args.command == "package":
            path, checksum = package(args.binary, version, args.output)
            print(f"{checksum}  {path.name}")
        elif args.command == "smoke-formula":
            verify_archive(args.archive, version)
            checksum = hashlib.sha256(args.archive.read_bytes()).hexdigest()
            args.output.write_text(render_formula(version, checksum, args.archive.resolve().as_uri()), encoding="utf-8")
        else:
            update_tap(args.tag, args.tap)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Release preparation failed: {error}\n")


if __name__ == "__main__":
    main()
