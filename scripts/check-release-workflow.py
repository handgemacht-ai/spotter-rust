#!/usr/bin/env python3
from pathlib import Path
import sys


WORKFLOW = Path(".github/workflows/release.yml")

REQUIRED_TARGETS = {
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
}

REQUIRED_SNIPPETS = {
    "tag trigger": 'tags:\n      - "v*.*.*"',
    "manual dry run trigger": "workflow_dispatch:",
    "manual publish input": "publish:",
    "release preflight": "release-preflight:",
    "conditional release preflight": "if: github.event_name == 'push' || inputs.publish",
    "release preflight dependency": "needs: release-preflight",
    "skipped preflight allowed for dry run": "needs.release-preflight.result == 'skipped'",
    "dry run build after skipped preflight": "needs.verify.result == 'success'",
    "manual publish tag guard": "Manual publishing must run from a tag ref",
    "crates.io token preflight": "CRATES_IO_TOKEN repository secret is required for release publishing",
    "crates.io package preflight": "scripts/check-crates-io-release-ready.py",
    "main ancestry check": 'git merge-base --is-ancestor "$GITHUB_SHA" origin/main',
    "changelog check": 'grep -q "^## ${manifest_version}$" CHANGELOG.md',
    "flag parity test": "--test cli_flag_parity",
    "path override test": "--test cli_path_overrides",
    "golden regen": "./xtask regen-golden",
    "golden diff": "git diff --exit-code -- tests/golden",
    "parity checklist": "scripts/check-parity-checklist.sh",
    "crate roots": "scripts/check-crate-roots.py",
    "local only": "scripts/check-local-only.py",
    "no production unwrap": "scripts/check-no-production-unwrap.py",
    "fixture generation": "scripts/make-test-fixtures.py",
    "fixture generation diff": "git diff --exit-code -- tests/fixtures/transcripts",
    "fixture scrub": "scripts/check-fixtures-scrubbed.py",
    "release workflow": "scripts/check-release-workflow.py",
    "CI workflow": "scripts/check-ci-workflow.py",
    "crates release preflight tests": "scripts/test-crates-io-release-ready.py",
    "rustdoc": "cargo rustdoc --lib --locked -- -D missing_docs",
    "doctests": "cargo test --doc --locked",
    "install smoke": "scripts/check-install-smoke.sh",
    "package check": "cargo package --locked",
    "repro build a": "CARGO_TARGET_DIR=target/repro-a cargo build --release --locked --target ${{ matrix.target }}",
    "repro build b": "CARGO_TARGET_DIR=target/repro-b cargo build --release --locked --target ${{ matrix.target }}",
    "binary comparison": 'cmp "target/repro-a/${{ matrix.target }}/release/$bin" "target/repro-b/${{ matrix.target }}/release/$bin"',
    "version match": 'test "$output" = "spotter $version"',
    "linux/mac checksum": "shasum -a 256 dist/spotter-${{ matrix.target }} > dist/spotter-${{ matrix.target }}.sha256",
    "windows checksum": "sha256sum dist/spotter-${{ matrix.target }}.exe > dist/spotter-${{ matrix.target }}.sha256",
    "github release": "softprops/action-gh-release@v2",
    "crates publish": 'cargo publish --locked --token "$CRATES_IO_TOKEN"',
    "conditional publish": "if: github.event_name == 'push' || inputs.publish",
}

ORDERED_SNIPPETS = [
    (
        'cargo publish --locked --token "$CRATES_IO_TOKEN"',
        "softprops/action-gh-release@v2",
        "crates.io publish must happen before GitHub Release creation",
    ),
]


def main() -> int:
    text = WORKFLOW.read_text()
    missing = []

    for target in REQUIRED_TARGETS:
        if target not in text:
            missing.append(f"target: {target}")

    for label, snippet in REQUIRED_SNIPPETS.items():
        if snippet not in text:
            missing.append(label)

    for before, after, label in ORDERED_SNIPPETS:
        if before in text and after in text and text.index(before) > text.index(after):
            missing.append(label)

    if missing:
        for item in missing:
            print(f"missing release workflow requirement: {item}", file=sys.stderr)
        return 1

    print("release workflow covers tag metadata, five targets, reproducibility, checksums, GitHub Release, and crates.io publish")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
