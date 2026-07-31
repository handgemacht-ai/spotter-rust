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
spotter transcripts profile --session <session-id>
spotter transcripts sample -n 5 --stratify-by status
spotter embed init  # one-time: downloads the local embedding model
spotter transcripts similar --to-run <session-id>:<tool-use-id>
```

All parsing and storage is local. There is no telemetry, no HTTP listener, and
no auto-update check. The one explicit exception is `spotter embed init`,
which downloads the local embedding model (all-MiniLM-L6-v2, ~87 MB from
Hugging Face) into a cache dir once; inference in `transcripts similar` then
runs fully offline in-process. The cache dir defaults to
`~/.local/share/spotter/models/all-MiniLM-L6-v2` and is overridable with
`--model-dir` or `SPOTTER_MODEL_DIR`. `similar` is transcripts-only and fails
fast with a remedy message when the model is not cached.

## Agent-facing search SDK

Every query verb speaks JSON (`--format json`), and every ID one verb emits is
accepted as input by another, so an agent can drive spotter through bash
pipes to investigate transcripts without knowing the vocabulary of a failure
in advance. `docs/json-schema.md` is the exhaustive reference for output
shapes and ID conventions; this section is the map.

### Primitives

- `profile` — one compact envelope (overview, tools, errors, recovery, token
  health, outliers) over a scope. The hypothesis generator: read it whole and
  notice what looks odd.
- `sample` — a stratified, seeded draw of real runs (`--stratify-by`, `-n`,
  `--seed`). Eyeball a few instances to learn the failure's vocabulary; rare
  strata are always represented.
- `search` — filter runs by tool/status/session/duration/file path, or
  BM25-ranked full-text over message content (`--content-contains`, `--exact`
  for a phrase). Returns full run objects.
- `similar` — fan out "more like this" from one exemplar run via local
  embeddings, for when you have one instance and no words for it.
- `inspect` — confirm one run with its neighbors and message context.
- `errors`, `sequences`, `health`, `aggregate` — the dimensional analytics
  `profile` composes; call them directly to go deeper on one dimension.
- `scan relations`, `scan read-scores` — cross-file co-change, rework,
  friction, and read-frequency metrics (scan-only).

### IDs out, IDs in

- `session_id` — on every run, message hit, and audit row. Accepted by
  `--session` on most verbs.
- `run_id` = `session_id:tool_use_id` — attached to every serialized run, to
  `errors` `sample_runs`, `profile` outliers, `sample` draws, and `similar`
  hits. Split at the **last** colon to parse (subagent session ids contain
  colons themselves). Accepted by `inspect --run` and `similar --to-run`.
- Ordinal windows — runs carry `start_ordinal`/`end_ordinal`, message hits
  carry `ordinal`. Accepted by `inspect --ordinals <min>:<max>`.

### The hypothesis-first loop

```sh
# 1. Survey a scope; notice an anomaly (error hotspot, slow tool, token jump).
spotter transcripts profile --project my-app

# 2. Eyeball real instances to learn the vocabulary.
spotter transcripts sample --status error -n 5 --seed 7

# 3. Pull more like the representative one (one-time model setup first).
spotter embed init
spotter transcripts similar --to-run <session_id>:<tool_use_id> -n 10 \
  --fields run_id,similarity,tool_name,command

# 4. Confirm with context around a run or a message window.
spotter transcripts inspect --run <session_id>:<tool_use_id> --context 2 --with-messages
spotter transcripts inspect --session <session_id> --ordinals 40:60
```

`--fields a,b` on these verbs trims JSON output to the named top-level keys
(unknown keys are a loud error), which keeps agent context small.

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
spotter scan --root ~/.claude/projects search --tool Read --min-read-lines 1000
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
| `scan search`               | Filter tool-call runs by project/worktree/session/tool/command/error/file path, duration, status, since. `--min-read-lines N` keeps only `Read` runs that put at least `N` lines into the transcript (from the transcript's recorded `numLines`, so it counts what actually entered context, not the file's size on disk). `--content-contains` BM25-ranks transcript message content (`--exact` for phrase match). `--group-by-session` aggregates rows. |
| `scan inspect --session`    | Show tool-call runs for one session sorted by ordinal. `--tool-use-id`, `--status`, `--context`, `--with-messages` work the same as the DB path. |
| `scan compare`              | Compare tool runs between two session cohorts (`--left-session`/`--right-session`, repeatable). |
| `scan aggregate`            | Group tool usage by `tool_name`, `status`, etc. with counts, error rates, p50/p95 durations, top errors. |
| `scan audit`                | Report JSONL line counts, parsed message counts, and message-type histograms per file. |
| `scan errors`               | Group tool-call errors into normalized fingerprints. `--classify` adds category and preventability. |
| `scan health --session`     | Per-session token-health analysis: cache window, cache misses, token jumps, peak context, total waste. |
| `scan health` (no session)  | Project-level rollup of token-health metrics. |
| `scan sequences`            | Detect frequent tool-call n-grams and retry patterns. `--recovery` adds recovery-rate stats. |
| `scan profile`              | One-call dimensional overview of a scope (overview/tools/errors/recovery/token-health/outliers) for anomaly discovery. |
| `scan sample`               | Stratified, seeded sample of tool-call runs (`--stratify-by tool_name|status|category|session`, `-n`, `--seed`) for eyeballing real failures. |

### Output formats

The two backends split the work: `transcripts` answers from the SQLite
database, `scan` parses JSONL directly. On shared verbs they emit
byte-identical JSON, pinned by integration tests that sync a fixture into
SQLite, run both paths, and assert equal output. `similar` is transcripts-only
(embeddings cache in the DB), just as `read-scores` and `relations` are
scan-only.

Every subcommand accepts `--format table` or `--format json` (default table;
`profile`, `sample`, and `similar` default to json).

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

The model-dependent integration tests (`tests/cli_similar.rs`, the
`similar.*` cases in `tests/json_schema_doc.rs`) need the embedding model in
the cache dir and fail fast without it: run `spotter embed init` once, or set
`SPOTTER_MODEL_DIR`. Environments without network access can opt out with
`SPOTTER_SKIP_MODEL_TESTS=1`.

The checked-in command parity list is at
`docs/subcommand-parity-checklist.md`. The JSON output shapes and stable ID
conventions (`run_id`, `--run`, `--ordinals`, `--fields`) are documented in
`docs/json-schema.md` and verified by `tests/json_schema_doc.rs`. Golden CLI
outputs live under
`tests/golden/` and can be regenerated with `./xtask regen-golden`. Release
readiness evidence and external release blockers are tracked in
`docs/release-readiness-audit.md`; the crates.io name conflict is detailed in
`docs/crates-io-name-decision.md`; the publish steps are in
`docs/release-runbook.md`.
