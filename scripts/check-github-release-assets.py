#!/usr/bin/env python3
import argparse
import hashlib
import os
import platform
import re
import stat
import struct
import subprocess
import sys
from pathlib import Path


TARGETS = {
    "x86_64-unknown-linux-gnu": {
        "binary": "spotter-x86_64-unknown-linux-gnu",
        "kind": "elf",
        "machine": 62,
    },
    "aarch64-unknown-linux-gnu": {
        "binary": "spotter-aarch64-unknown-linux-gnu",
        "kind": "elf",
        "machine": 183,
    },
    "x86_64-apple-darwin": {
        "binary": "spotter-x86_64-apple-darwin",
        "kind": "macho",
        "cpu": 0x01000007,
    },
    "aarch64-apple-darwin": {
        "binary": "spotter-aarch64-apple-darwin",
        "kind": "macho",
        "cpu": 0x0100000C,
    },
    "x86_64-pc-windows-msvc": {
        "binary": "spotter-x86_64-pc-windows-msvc.exe",
        "kind": "pe",
        "machine": 0x8664,
    },
}

HASH_RE = re.compile(r"^[0-9a-fA-F]{64}$")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Verify spotter GitHub Release assets and checksums."
    )
    parser.add_argument("release_dir", type=Path)
    parser.add_argument("--expect-version")
    parser.add_argument("--require-runnable-host", action="store_true")
    args = parser.parse_args()

    release_dir = args.release_dir
    if not release_dir.is_dir():
        print(f"release asset directory does not exist: {release_dir}", file=sys.stderr)
        return 1

    files = collect_files(release_dir)
    expected_names = expected_asset_names()
    actual_names = set(files)
    missing = sorted(expected_names - actual_names)
    extra = sorted(actual_names - expected_names)
    if missing or extra:
        for name in missing:
            print(f"missing release asset: {name}", file=sys.stderr)
        for name in extra:
            print(f"unexpected release asset: {name}", file=sys.stderr)
        return 1

    for target, metadata in TARGETS.items():
        binary_name = metadata["binary"]
        checksum_name = f"spotter-{target}.sha256"
        binary_path = files[binary_name]
        checksum_path = files[checksum_name]
        expected_digest, checksum_binary_name = parse_checksum(checksum_path)
        if checksum_binary_name != binary_name:
            print(
                f"{checksum_name} references {checksum_binary_name}, expected {binary_name}",
                file=sys.stderr,
            )
            return 1
        actual_digest = sha256(binary_path)
        if actual_digest != expected_digest:
            print(
                f"checksum mismatch for {binary_name}: expected {expected_digest}, got {actual_digest}",
                file=sys.stderr,
            )
            return 1
        verify_header(binary_path, metadata)

    if args.expect_version:
        verify_runnable_host(files, args.expect_version, args.require_runnable_host)

    print("GitHub Release assets cover five targets with matching checksums")
    return 0


def collect_files(root: Path) -> dict[str, Path]:
    files: dict[str, Path] = {}
    for path in root.rglob("*"):
        if not path.is_file():
            continue
        name = path.name
        if name in files:
            raise SystemExit(f"duplicate release asset basename: {name}")
        files[name] = path
    return files


def expected_asset_names() -> set[str]:
    names = set()
    for target, metadata in TARGETS.items():
        names.add(metadata["binary"])
        names.add(f"spotter-{target}.sha256")
    return names


def parse_checksum(path: Path) -> tuple[str, str]:
    fields = path.read_text(encoding="utf-8").strip().split()
    if len(fields) != 2:
        raise SystemExit(f"invalid checksum file format: {path}")
    digest = fields[0].lower()
    if not HASH_RE.match(digest):
        raise SystemExit(f"invalid sha256 digest in {path}: {fields[0]}")
    binary_name = Path(fields[1].lstrip("*")).name
    return digest, binary_name


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_header(path: Path, metadata: dict[str, object]) -> None:
    data = path.read_bytes()
    kind = metadata["kind"]
    if kind == "elf":
        verify_elf(path, data, int(metadata["machine"]))
    elif kind == "macho":
        verify_macho(path, data, int(metadata["cpu"]))
    elif kind == "pe":
        verify_pe(path, data, int(metadata["machine"]))
    else:
        raise SystemExit(f"unknown binary kind for {path}: {kind}")


def verify_elf(path: Path, data: bytes, expected_machine: int) -> None:
    if len(data) < 20 or data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        raise SystemExit(f"{path.name} is not a little-endian ELF64 binary")
    machine = struct.unpack_from("<H", data, 18)[0]
    if machine != expected_machine:
        raise SystemExit(f"{path.name} ELF machine {machine}, expected {expected_machine}")


def verify_macho(path: Path, data: bytes, expected_cpu: int) -> None:
    if len(data) < 16:
        raise SystemExit(f"{path.name} is too small to be Mach-O")
    magic, cpu, _subtype, filetype = struct.unpack_from("<IIII", data, 0)
    if magic != 0xFEEDFACF or filetype != 2:
        raise SystemExit(f"{path.name} is not a 64-bit Mach-O executable")
    if cpu != expected_cpu:
        raise SystemExit(f"{path.name} Mach-O CPU {cpu:#x}, expected {expected_cpu:#x}")


def verify_pe(path: Path, data: bytes, expected_machine: int) -> None:
    if len(data) < 0x40 or data[:2] != b"MZ":
        raise SystemExit(f"{path.name} is not a PE binary")
    pe_offset = struct.unpack_from("<I", data, 0x3C)[0]
    if len(data) < pe_offset + 6 or data[pe_offset : pe_offset + 4] != b"PE\0\0":
        raise SystemExit(f"{path.name} has no PE header")
    machine = struct.unpack_from("<H", data, pe_offset + 4)[0]
    if machine != expected_machine:
        raise SystemExit(f"{path.name} PE machine {machine:#x}, expected {expected_machine:#x}")


def verify_runnable_host(
    files: dict[str, Path], expected_version: str, require_runnable_host: bool
) -> None:
    if platform.system() != "Linux" or platform.machine() not in {"x86_64", "AMD64"}:
        if require_runnable_host:
            raise SystemExit("runnable host version check requires Linux x86_64")
        return
    binary = files["spotter-x86_64-unknown-linux-gnu"]
    mode = binary.stat().st_mode
    if not mode & stat.S_IXUSR:
        os.chmod(binary, mode | stat.S_IXUSR)
    output = subprocess.check_output([str(binary), "--version"], text=True).strip()
    expected = f"spotter {expected_version}"
    if output != expected:
        raise SystemExit(f"{binary.name} --version returned {output!r}, expected {expected!r}")


if __name__ == "__main__":
    raise SystemExit(main())
