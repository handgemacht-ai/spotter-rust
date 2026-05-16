#!/usr/bin/env python3
import hashlib
import struct
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts" / "check-github-release-assets.py"


def main() -> int:
    with tempfile.TemporaryDirectory() as temp:
        root = Path(temp)
        write_release_assets(root)
        run_check(root, 0, "matching checksums")

        (root / "spotter-x86_64-unknown-linux-gnu").write_bytes(elf_header(183))
        run_check(root, 1, "checksum mismatch")

    with tempfile.TemporaryDirectory() as temp:
        root = Path(temp)
        write_release_assets(root)
        (root / "spotter-aarch64-apple-darwin").unlink()
        run_check(root, 1, "missing release asset")

    with tempfile.TemporaryDirectory() as temp:
        root = Path(temp)
        write_release_assets(root)
        (root / "spotter-aarch64-apple-darwin").write_bytes(macho_header(0x01000007))
        refresh_checksum(root, "aarch64-apple-darwin")
        run_check(root, 1, "Mach-O CPU")

    print("GitHub Release asset verifier tests passed")
    return 0


def write_release_assets(root: Path) -> None:
    for target, (binary_name, header) in TARGETS.items():
        binary = root / binary_name
        binary.write_bytes(header + b"\0test payload\n")
        refresh_checksum(root, target)


def refresh_checksum(root: Path, target: str) -> None:
    binary_name, _header = TARGETS[target]
    binary = root / binary_name
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    displayed_name = f"*dist/{binary_name}" if binary_name.endswith(".exe") else f"dist/{binary_name}"
    (root / f"spotter-{target}.sha256").write_text(
        f"{digest}  {displayed_name}\n",
        encoding="utf-8",
    )


def run_check(root: Path, expected_returncode: int, expected_output: str) -> None:
    process = subprocess.run(
        [sys.executable, str(CHECK), str(root)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if process.returncode != expected_returncode:
        raise SystemExit(
            f"expected exit {expected_returncode}, got {process.returncode}\n{process.stdout}"
        )
    if expected_output not in process.stdout:
        raise SystemExit(
            f"expected output containing {expected_output!r}\n{process.stdout}"
        )


def elf_header(machine: int) -> bytes:
    data = bytearray(64)
    data[:6] = b"\x7fELF\x02\x01"
    struct.pack_into("<H", data, 16, 3)
    struct.pack_into("<H", data, 18, machine)
    return bytes(data)


def macho_header(cpu: int) -> bytes:
    return struct.pack("<IIIIIIII", 0xFEEDFACF, cpu, 0, 2, 1, 0, 0, 0)


def pe_header(machine: int) -> bytes:
    data = bytearray(160)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<H", data, 0x84, machine)
    return bytes(data)


TARGETS = {
    "x86_64-unknown-linux-gnu": ("spotter-x86_64-unknown-linux-gnu", elf_header(62)),
    "aarch64-unknown-linux-gnu": ("spotter-aarch64-unknown-linux-gnu", elf_header(183)),
    "x86_64-apple-darwin": ("spotter-x86_64-apple-darwin", macho_header(0x01000007)),
    "aarch64-apple-darwin": ("spotter-aarch64-apple-darwin", macho_header(0x0100000C)),
    "x86_64-pc-windows-msvc": ("spotter-x86_64-pc-windows-msvc.exe", pe_header(0x8664)),
}


if __name__ == "__main__":
    raise SystemExit(main())
