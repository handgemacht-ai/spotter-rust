#!/usr/bin/env python3
from pathlib import Path
import re
import sys


CALL_PATTERN = re.compile(r"\.(unwrap|expect)\s*\(")
ALLOWED_FILES = {
    Path("src/main.rs"),
}


def main() -> int:
    failures = []
    for path in sorted(Path("src").glob("**/*.rs")):
        if path in ALLOWED_FILES:
            continue
        text = path.read_text()
        in_test_module = False
        for line_number, line in enumerate(text.splitlines(), start=1):
            stripped = line.strip()
            if stripped == "#[cfg(test)]":
                in_test_module = True
            if in_test_module:
                continue
            if CALL_PATTERN.search(line):
                failures.append(f"{path}:{line_number}: production unwrap/expect")

    if failures:
        for failure in failures:
            print(failure, file=sys.stderr)
        return 1

    print("production unwrap/expect check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
