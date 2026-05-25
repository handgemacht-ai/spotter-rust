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

## Scanning transcripts without the database

`spotter scan` runs the same analytics as `spotter transcripts <verb>`, but
parses JSONL transcripts on demand instead of reading from SQLite. Use it
when you do not want to sync into the DB first, when you only care about a
single file, or when you want to query a transcript root the DB has never
seen.

```sh
spotter scan --file session.jsonl search --tool Bash --limit 20
spotter scan --root ~/.claude_agents/projects errors --classify
spotter scan --file session.jsonl health --session <session-id>
```

### Target selection

`--file <path>` (repeatable) and `--root <path>` (repeatable) are global to
every subcommand. With no targets given, scan walks `~/.claude/projects` and
`~/.claude_agents/projects` if they exist, or the `transcript_roots` in
config. Pass `--no-subagents` to skip `subagents/` directories when walking a
root.

### Subcommands (mirror `transcripts`)

| Subcommand                  | What it does                                                                    |
|-----------------------------|---------------------------------------------------------------------------------|
| `scan search`               | Filter tool-call runs by project/worktree/session/tool/command/error/file path, duration, status, since. `--content-contains` substring-matches transcript message content. `--group-by-session` aggregates rows. |
| `scan inspect --session`    | Show tool-call runs for one session sorted by ordinal. `--tool-use-id`, `--status`, `--context`, `--with-messages` work the same as the DB path. |
| `scan compare`              | Compare tool runs between two session cohorts (`--left-session`/`--right-session`, repeatable). |
| `scan aggregate`            | Group tool usage by `tool_name`, `status`, etc. with counts, error rates, p50/p95 durations, top errors. |
| `scan audit`                | Report JSONL line counts, parsed message counts, and message-type histograms per file. |
| `scan errors`               | Group tool-call errors into normalized fingerprints. `--classify` adds category and preventability. |
| `scan health --session`     | Per-session token-health analysis: cache window, cache misses, token jumps, peak context, total waste. |
| `scan health` (no session)  | Project-level rollup of token-health metrics. |
| `scan sequences`            | Detect frequent tool-call n-grams and retry patterns. `--recovery` adds recovery-rate stats. |

### Output formats

Every subcommand accepts `--format table` (default) or `--format json`. The
JSON shape is identical to the matching `transcripts <verb>` JSON shape; this
is pinned by integration tests that sync a fixture into SQLite, run both
paths, and assert byte-equivalent JSON.

### When to prefer `scan` vs `transcripts`

- Use `scan` for one-off questions against arbitrary transcripts, ad-hoc
  forensics ("which session deleted this file?"), or running analytics
  against transcripts that live outside your normal `transcript_roots`.
- Use `transcripts` when you want fast repeated queries over the same set of
  transcripts (the DB amortizes parsing cost), full-text search via FTS, or
  the message-context output that depends on stored message rows.

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
