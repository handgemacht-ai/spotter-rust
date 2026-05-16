#!/usr/bin/env python3
import re
import sys
from pathlib import Path


ROOT = Path("tests/fixtures")

FORBIDDEN = {
    "personal home path": re.compile(r"/home/(?!USER\b)[A-Za-z0-9._-]+"),
    "encoded personal home path": re.compile(r"-home-(?!USER\b)[A-Za-z0-9._-]+"),
    "personal name": re.compile(r"\b(?:marco|rotilli|rotili)\b", re.IGNORECASE),
    "company marker": re.compile(r"handgemacht", re.IGNORECASE),
    "email address": re.compile(
        r"[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}", re.IGNORECASE
    ),
    "GitHub token": re.compile(r"(?:github_pat|gh[pousr]_[A-Za-z0-9_])"),
    "OpenAI-style secret": re.compile(r"sk-[A-Za-z0-9]{20,}"),
    "private key": re.compile(r"BEGIN (?:RSA|OPENSSH|PRIVATE) KEY"),
    "unredacted signature": re.compile(
        r'"signature"\s*:\s*"(?!\[REDACTED_SIGNATURE\])[^"]+"'
    ),
    "source transcript marker": re.compile(
        r"(?:spotter-gqc|origin/|beads\.db|Claude Marketplace|bradleygolden|"
        r"project review|review token|xterm|tmux|\.mcp)",
        re.IGNORECASE,
    ),
}


def main() -> int:
    failures = []
    for path in sorted(ROOT.rglob("*.jsonl")):
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for label, pattern in FORBIDDEN.items():
                if pattern.search(line):
                    failures.append(f"{path}:{number}: fixture contains {label}")

    if failures:
        for failure in failures:
            print(failure, file=sys.stderr)
        return 1

    print("fixture scrub check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
