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
cargo package --allow-dirty --locked
cargo publish --dry-run --allow-dirty --locked
scripts/check-install-smoke.sh
```

The checked-in command parity list is at
`docs/subcommand-parity-checklist.md`. Golden CLI outputs live under
`tests/golden/` and can be regenerated with `./xtask regen-golden`. Release
readiness evidence and external release blockers are tracked in
`docs/release-readiness-audit.md`; the crates.io name conflict is detailed in
`docs/crates-io-name-decision.md`; the publish steps are in
`docs/release-runbook.md`.
