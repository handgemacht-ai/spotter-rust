#!/usr/bin/env python3
from pathlib import Path
import sys


CRATE_ROOTS = [
    Path("src/lib.rs"),
    Path("src/main.rs"),
]


def main() -> int:
    failures = []
    for path in CRATE_ROOTS:
        text = path.read_text()
        if "#![forbid(unsafe_code)]" not in text:
            failures.append(f"{path}: missing #![forbid(unsafe_code)]")

    if failures:
        for failure in failures:
            print(failure, file=sys.stderr)
        return 1

    print("crate root safety attributes passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
