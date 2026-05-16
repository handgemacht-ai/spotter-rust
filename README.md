# spotter

`spotter` is a standalone local CLI for indexing Claude Code JSONL transcripts
into a user-owned SQLite database and querying tool-call analytics.

The default database path is `~/.local/share/spotter/spotter.db`. The default
config path is `~/.config/spotter/config.toml`. Both are overridable:

```sh
spotter --db /tmp/spotter.db --config ./config.toml transcripts sync --file session.jsonl
```

## Commands

```sh
spotter init --yes
spotter projects list
spotter projects add my-project /path/to/project
spotter transcripts sync --transcript-root ~/.claude/projects
spotter transcripts search --tool Bash --format json
spotter transcripts inspect --session <session-id>
spotter transcripts compare --left-session <id> --right-session <id>
spotter transcripts aggregate --group-by tool_name,status
spotter transcripts audit --session <session-id>
spotter transcripts errors --classify
spotter transcripts health --session <session-id>
spotter transcripts sequences --recovery
```

All parsing and storage is local. There is no telemetry, no HTTP listener, and
no auto-update check.

## Current Release Status

`spotter` is not currently published to crates.io and there is no GitHub Release
tag for `0.1.5`. Until that decision changes, install or run it from this
source checkout:

```sh
cargo install --path . --locked
cargo run --locked -- --help
```

## Verification

```sh
cargo fmt --check
cargo check --all-targets --all-features --locked
cargo clippy --all-targets --all-features --locked -- -D warnings -W clippy::pedantic -W clippy::nursery
cargo test --all-targets --all-features --locked
cargo rustdoc --lib --locked -- -D missing_docs
cargo test --doc --locked
cargo check --benches --locked
scripts/check-parity-checklist.sh
scripts/check-release-workflow.py
scripts/check-ci-workflow.py
scripts/check-crate-roots.py
scripts/check-local-only.py
scripts/check-no-production-unwrap.py
scripts/make-test-fixtures.py
git diff --exit-code -- tests/fixtures/transcripts
scripts/check-fixtures-scrubbed.py
scripts/test-crates-io-release-ready.py
scripts/test-github-release-config.py
scripts/test-release-pr-signoff.py
scripts/test-github-release-assets.py
scripts/test-release-complete.py
cargo package --locked
cargo publish --dry-run --locked
scripts/check-install-smoke.sh
```

The checked-in command parity list is at
`docs/subcommand-parity-checklist.md`. Golden CLI outputs live under
`tests/golden/` and can be regenerated with `./xtask regen-golden`. Release
readiness evidence and external release blockers are tracked in
`docs/release-readiness-audit.md`; the crates.io name conflict is detailed in
`docs/crates-io-name-decision.md`; the publish steps are in
`docs/release-runbook.md`.
