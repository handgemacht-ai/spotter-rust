#!/usr/bin/env python3
from pathlib import Path
import re
import sys
import tomllib


FORBIDDEN_DEPENDENCIES = {
    "axum",
    "hyper",
    "opentelemetry",
    "reqwest",
    "self_update",
    "sentry",
    "tonic",
    "tracing-opentelemetry",
    "ureq",
    "warp",
}

FORBIDDEN_SOURCE_PATTERNS = {
    "std::net": re.compile(r"\bstd::net::"),
    "tcp listener": re.compile(r"\bTcpListener\b"),
    "tcp stream": re.compile(r"\bTcpStream\b"),
    "udp socket": re.compile(r"\bUdpSocket\b"),
    "unix listener": re.compile(r"\bUnixListener\b"),
    "external http client": re.compile(r"\b(reqwest|ureq|hyper)::"),
    "telemetry client": re.compile(r"\b(opentelemetry|sentry|tracing_opentelemetry)::"),
    "self update": re.compile(r"\b(self_update|update_informer)::"),
}


def main() -> int:
    failures = []
    manifest = tomllib.loads(Path("Cargo.toml").read_text())
    for section in ("dependencies", "dev-dependencies", "build-dependencies"):
        for name in manifest.get(section, {}):
            if name in FORBIDDEN_DEPENDENCIES:
                failures.append(f"forbidden dependency in {section}: {name}")

    for path in sorted(Path("src").glob("**/*.rs")):
        text = path.read_text()
        for label, pattern in FORBIDDEN_SOURCE_PATTERNS.items():
            if pattern.search(text):
                failures.append(f"{path}: forbidden local-only pattern: {label}")

    if failures:
        for failure in failures:
            print(failure, file=sys.stderr)
        return 1

    print("local-only check passed: no network/listener/telemetry/update dependencies or source patterns found")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
