#!/usr/bin/env python3
from pathlib import Path
import sys


WORKFLOW = Path(".github/workflows/ci.yml")

REQUIRED_SNIPPETS = {
    "main branch push": "branches: [main]",
    "Node 24 checkout action": "actions/checkout@v6",
    "MSRV toolchain": "toolchain: 1.75.0",
    "format": "cargo fmt --check",
    "check": "cargo check --all-targets --all-features --locked",
    "strict clippy": "cargo clippy --all-targets --all-features --locked -- -D warnings -W clippy::pedantic -W clippy::nursery",
    "all tests": "cargo test --all-targets --all-features --locked",
    "missing docs": "cargo rustdoc --lib --locked -- -D missing_docs",
    "doctests": "cargo test --doc --locked",
    "benches compile": "cargo check --benches --locked",
    "install smoke": "scripts/check-install-smoke.sh",
    "cargo deny": "cargo deny check",
    "cargo machete": "cargo machete",
    "crate roots": "scripts/check-crate-roots.py",
    "local only": "scripts/check-local-only.py",
    "no production unwrap": "scripts/check-no-production-unwrap.py",
    "fixture generation": "scripts/make-test-fixtures.py",
    "fixture generation diff": "git diff --exit-code -- tests/fixtures/transcripts",
    "fixture scrub": "scripts/check-fixtures-scrubbed.py",
    "release workflow": "scripts/check-release-workflow.py",
    "CI workflow": "scripts/check-ci-workflow.py",
    "crates release preflight tests": "scripts/test-crates-io-release-ready.py",
    "GitHub Release asset verifier tests": "scripts/test-github-release-assets.py",
    "release completion verifier tests": "scripts/test-release-complete.py",
    "coverage run": "cargo llvm-cov --all-targets --all-features --branch --json --output-path target/llvm-cov.json",
    "coverage threshold": "scripts/check-coverage-json.py target/llvm-cov.json --lines 80 --branches 70",
    "flag parity test": "--test cli_flag_parity",
    "path override test": "--test cli_path_overrides",
    "golden regen": "./xtask regen-golden",
    "golden diff": "git diff --exit-code -- tests/golden",
    "parity checklist": "scripts/check-parity-checklist.sh",
    "fixtures": "scripts/make-performance-fixtures.py target/perf",
    "criterion regression": "scripts/check-criterion-regression.py target/criterion --max-regression-percent 5 jsonl_parse_session_file analytics_derive_runs_via_ingest",
    "help perf": "scripts/check-hyperfine-json.py target/perf/help.json --name 'spotter --help' --max-ms 50",
    "search perf": "scripts/check-hyperfine-json.py target/perf/search-10k.json --name 'search 10k tool calls' --max-ms 200",
    "inspect perf": "scripts/check-hyperfine-json.py target/perf/inspect-5k.json --name 'inspect 5k-message session' --max-ms 150",
    "sync perf": "scripts/check-hyperfine-json.py target/perf/sync-1mib.json --name 'sync 1 MiB JSONL' --max-ms 400",
    "rss perf": "scripts/check-rss.py --max-kb 102400",
}

BLOCKED_SNIPPETS = {
    "outdated checkout action": "actions/checkout@v4",
}


def main() -> int:
    text = WORKFLOW.read_text()
    missing = [
        label for label, snippet in REQUIRED_SNIPPETS.items() if snippet not in text
    ]
    missing.extend(
        label for label, snippet in BLOCKED_SNIPPETS.items() if snippet in text
    )
    if missing:
        for label in missing:
            print(f"missing CI workflow requirement: {label}", file=sys.stderr)
        return 1

    print("CI workflow covers code quality, parity, coverage, performance, and release-readiness gates")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
