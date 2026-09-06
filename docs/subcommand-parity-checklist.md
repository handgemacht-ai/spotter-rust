# Subcommand Parity Checklist

This checklist maps the carried-over Elixir command surface to the Rust CLI.
Every checked row has a fixture-backed integration path in `tests/`.

## Source Verification

The carried-over command and flag rows were checked against the source
`@spotter` Mix task implementations, not only against prose documentation:

- `lib/mix/tasks/spotter.transcripts.ex`
- `lib/mix/tasks/spotter.transcripts.sync.ex`
- `lib/mix/tasks/spotter.transcripts.search.ex`
- `lib/mix/tasks/spotter.transcripts.inspect.ex`
- `lib/mix/tasks/spotter.transcripts.compare.ex`
- `lib/mix/tasks/spotter.transcripts.aggregate.ex`
- `lib/mix/tasks/spotter.transcripts.audit.ex`
- `lib/mix/tasks/spotter.transcripts.errors.ex`
- `lib/mix/tasks/spotter.transcripts.health.ex`
- `lib/mix/tasks/spotter.transcripts.sequences.ex`
- `lib/mix/tasks/spotter.transcripts.slice.register.ex`

For commands using `@switches` or `OptionParser.parse(strict: ...)`, this
checklist follows the actual Elixir switch atoms. In particular, source docs may
refer to `--min-duration-ms`, but the carried-over search task accepts
`--min-duration`; `slice.register` is the only transcript Mix task using
`--min-duration-ms`, and that Phoenix-specific command is deliberately dropped.

## Ported Commands

| Status | Elixir command | Elixir flag | Rust command | Rust flag |
| --- | --- | --- | --- | --- |
| [x] | `mix spotter.transcripts` | help index | `spotter transcripts` | help index |
| [x] | `mix spotter.transcripts.sync` | `--session <id>` | `spotter transcripts sync` | `--session <id>` |
| [x] | `mix spotter.transcripts.sync` | `--file <path>` | `spotter transcripts sync` | `--file <path>` |
| [x] | `mix spotter.transcripts.sync` | `--transcript-root <path>` | `spotter transcripts sync` | `--transcript-root <path>` |
| [x] | `mix spotter.transcripts.search` | `--project <id>` | `spotter transcripts search` | `--project <alias>` |
| [x] | `mix spotter.transcripts.search` | `--worktree <name>` | `spotter transcripts search` | `--worktree <name>` |
| [x] | `mix spotter.transcripts.search` | `--session <id>` | `spotter transcripts search` | `--session <id>` |
| [x] | `mix spotter.transcripts.search` | `--tool <name>` | `spotter transcripts search` | `--tool <name>` |
| [x] | `mix spotter.transcripts.search` | `--command-contains <text>` | `spotter transcripts search` | `--command-contains <text>` |
| [x] | `mix spotter.transcripts.search` | `--error-contains <text>` | `spotter transcripts search` | `--error-contains <text>` |
| [x] | `mix spotter.transcripts.search` | `--file-path <path>` | `spotter transcripts search` | `--file-path <path>` |
| [x] | `mix spotter.transcripts.search` | `--min-duration <ms>` | `spotter transcripts search` | `--min-duration <ms>` |
| [x] | `mix spotter.transcripts.search` | `--max-duration <ms>` | `spotter transcripts search` | `--max-duration <ms>` |
| [x] | `mix spotter.transcripts.search` | `--status <status>` | `spotter transcripts search` | `--status <status>` |
| [x] | `mix spotter.transcripts.search` | `--limit <n>` | `spotter transcripts search` | `--limit <n>` |
| [x] | `mix spotter.transcripts.search` | `--format <fmt>` | `spotter transcripts search` | `--format <fmt>` |
| [x] | `mix spotter.transcripts.search` | `--group-by-session` | `spotter transcripts search` | `--group-by-session` |
| [x] | new search behavior | content search | `spotter transcripts search` | `--content-contains <text>` |
| [x] | `mix spotter.transcripts.inspect` | `--session <id>` | `spotter transcripts inspect` | `--session <id>` |
| [x] | `mix spotter.transcripts.inspect` | `--tool-use-id <id>` | `spotter transcripts inspect` | `--tool-use-id <id>` |
| [x] | `mix spotter.transcripts.inspect` | `--context <n>` | `spotter transcripts inspect` | `--context <n>` |
| [x] | `mix spotter.transcripts.inspect` | `--status <status>` | `spotter transcripts inspect` | `--status <status>` |
| [x] | `mix spotter.transcripts.inspect` | `--with-messages` | `spotter transcripts inspect` | `--with-messages` |
| [x] | `mix spotter.transcripts.inspect` | `--format <fmt>` | `spotter transcripts inspect` | `--format <fmt>` |
| [x] | `mix spotter.transcripts.compare` | `--left-session <id>` (repeatable) | `spotter transcripts compare` | `--left-session <id>` (repeatable) |
| [x] | `mix spotter.transcripts.compare` | `--right-session <id>` (repeatable) | `spotter transcripts compare` | `--right-session <id>` (repeatable) |
| [x] | `mix spotter.transcripts.compare` | `--tool <name>` | `spotter transcripts compare` | `--tool <name>` |
| [x] | `mix spotter.transcripts.compare` | `--command-contains <text>` | `spotter transcripts compare` | `--command-contains <text>` |
| [x] | `mix spotter.transcripts.compare` | `--group-by <field>` | `spotter transcripts compare` | `--group-by <field>` |
| [x] | `mix spotter.transcripts.compare` | `--format <fmt>` | `spotter transcripts compare` | `--format <fmt>` |
| [x] | `mix spotter.transcripts.aggregate` | `--project <id>` | `spotter transcripts aggregate` | `--project <alias>` |
| [x] | `mix spotter.transcripts.aggregate` | `--since <YYYY-MM-DD>` | `spotter transcripts aggregate` | `--since <YYYY-MM-DD>` |
| [x] | `mix spotter.transcripts.aggregate` | `--tool <name>` | `spotter transcripts aggregate` | `--tool <name>` |
| [x] | `mix spotter.transcripts.aggregate` | `--group-by <fields>` | `spotter transcripts aggregate` | `--group-by <fields>` |
| [x] | `mix spotter.transcripts.aggregate` | `--format <fmt>` | `spotter transcripts aggregate` | `--format <fmt>` |
| [x] | `mix spotter.transcripts.audit` | `--file <path>` | `spotter transcripts audit` | `--file <path>` |
| [x] | `mix spotter.transcripts.audit` | `--session <id>` | `spotter transcripts audit` | `--session <id>` |
| [x] | `mix spotter.transcripts.audit` | `--project <id>` | `spotter transcripts audit` | `--project <alias>` |
| [x] | `mix spotter.transcripts.audit` | `--limit <n>` | `spotter transcripts audit` | `--limit <n>` |
| [x] | `mix spotter.transcripts.audit` | `--format <fmt>` | `spotter transcripts audit` | `--format <fmt>` |
| [x] | `mix spotter.transcripts.errors` | `--project <id>` | `spotter transcripts errors` | `--project <alias>` |
| [x] | `mix spotter.transcripts.errors` | `--session <id>` | `spotter transcripts errors` | `--session <id>` |
| [x] | `mix spotter.transcripts.errors` | `--since <YYYY-MM-DD>` | `spotter transcripts errors` | `--since <YYYY-MM-DD>` |
| [x] | `mix spotter.transcripts.errors` | `--tool <name>` | `spotter transcripts errors` | `--tool <name>` |
| [x] | `mix spotter.transcripts.errors` | `--top <n>` | `spotter transcripts errors` | `--top <n>` |
| [x] | `mix spotter.transcripts.errors` | `--classify` | `spotter transcripts errors` | `--classify` |
| [x] | `mix spotter.transcripts.errors` | `--format <fmt>` | `spotter transcripts errors` | `--format <fmt>` |
| [x] | `mix spotter.transcripts.health` | `--session <id>` | `spotter transcripts health` | `--session <id>` |
| [x] | `mix spotter.transcripts.health` | `--project <id>` | `spotter transcripts health` | `--project <alias>` |
| [x] | `mix spotter.transcripts.health` | `--since <YYYY-MM-DD>` | `spotter transcripts health` | `--since <YYYY-MM-DD>` |
| [x] | `mix spotter.transcripts.health` | `--limit <n>` | `spotter transcripts health` | `--limit <n>` |
| [x] | `mix spotter.transcripts.health` | `--format <fmt>` | `spotter transcripts health` | `--format <fmt>` |
| [x] | `mix spotter.transcripts.sequences` | `--project <id>` | `spotter transcripts sequences` | `--project <alias>` |
| [x] | `mix spotter.transcripts.sequences` | `--since <YYYY-MM-DD>` | `spotter transcripts sequences` | `--since <YYYY-MM-DD>` |
| [x] | `mix spotter.transcripts.sequences` | `--min-length <n>` | `spotter transcripts sequences` | `--min-length <n>` |
| [x] | `mix spotter.transcripts.sequences` | `--max-length <n>` | `spotter transcripts sequences` | `--max-length <n>` |
| [x] | `mix spotter.transcripts.sequences` | `--min-occurrences <n>` | `spotter transcripts sequences` | `--min-occurrences <n>` |
| [x] | `mix spotter.transcripts.sequences` | `--recovery` | `spotter transcripts sequences` | `--recovery` |
| [x] | `mix spotter.transcripts.sequences` | `--format <fmt>` | `spotter transcripts sequences` | `--format <fmt>` |

## Deliberately Dropped

| Status | Elixir command | Reason |
| --- | --- | --- |
| [x] | `mix spotter.transcripts.slice.register` | Phoenix session-viewer bookmarking is out of scope for the standalone CLI. |

## Added Commands

New Rust-only commands beyond the carried-over Elixir surface. `spotter scan`
runs the same analytics as `spotter transcripts <verb>` on a DB-less,
in-memory store (`src/scan.rs`). Bare `spotter scan` prints a help index, and
the scan-level options `--file <path>` (repeatable), `--root <path>`
(repeatable), and `--no-subagents` apply to every scan subcommand (`src/cli.rs`).
With no `--file`/`--root` and no configured `transcript_roots`, scan falls
back to walking `~/.claude/projects` and `~/.claude_agents/projects` when
they exist (`src/scan.rs` `default_roots`).

Nine of the scan rows below are backed by `tests/cli_scan.rs`,
`tests/scan_loader.rs`, `tests/relations.rs`, `tests/relations_read_clusters.rs`,
and `tests/cli_surface.rs`. Two rows currently lack a dedicated scan-path
integration test and are left unchecked:

- `spotter scan compare` — the only compare tests
  (`tests/cli_flag_parity.rs:142,170`; `tests/cli_inspect_compare_aggregate.rs:6`)
  exercise the `spotter transcripts compare` (DB) path, not `scan compare`.
- `spotter scan read-scores` — no test in `tests/` references `read-scores`,
  `read_scores`, or its `--half-life-days` flag.

| Status | Rust command | Flags |
| --- | --- | --- |
| [x] | `spotter init` | `--claude-projects <path>`, `--yes` |
| [x] | `spotter projects list` | none |
| [x] | `spotter projects add` | `<alias> <path>` |
| [x] | `spotter projects remove` | `<alias>` |
| [x] | `spotter projects alias` | `<old-alias> <new-alias>` |
| [x] | `spotter scan` | help index; scan-level `--file <path>`, `--root <path>`, `--no-subagents` (global to every scan subcommand) |
| [x] | `spotter scan search` | `--project <alias>`, `--worktree <name>`, `--session <id>`, `--tool <name>`, `--command-contains <text>`, `--error-contains <text>`, `--file-path <path>`, `--content-contains <text>`, `--min-duration <ms>`, `--max-duration <ms>`, `--min-read-lines <n>`, `--status <status>`, `--since <date or timestamp>`, `--limit <n>`, `--format <fmt>`, `--group-by-session` |
| [x] | `spotter scan inspect` | `--session <id>` (required), `--tool-use-id <id>`, `--context <n>`, `--status <status>`, `--with-messages`, `--format <fmt>` |
| [ ] | `spotter scan compare` | `--left-session <id>` (repeatable), `--right-session <id>` (repeatable), `--tool <name>`, `--command-contains <text>`, `--group-by <field>`, `--format <fmt>` |
| [x] | `spotter scan aggregate` | `--project <alias>`, `--since <YYYY-MM-DD>`, `--tool <name>`, `--group-by <fields>`, `--format <fmt>` |
| [x] | `spotter scan audit` | `--limit <n>`, `--format <fmt>` |
| [x] | `spotter scan errors` | `--project <alias>`, `--session <id>`, `--since <YYYY-MM-DD>`, `--tool <name>`, `--top <n>`, `--classify`, `--format <fmt>` |
| [x] | `spotter scan health` | `--session <id>`, `--project <alias>`, `--since <YYYY-MM-DD>`, `--limit <n>`, `--format <fmt>` |
| [x] | `spotter scan sequences` | `--project <alias>`, `--since <YYYY-MM-DD>`, `--min-length <n>`, `--max-length <n>`, `--min-occurrences <n>`, `--recovery`, `--format <fmt>` |
| [ ] | `spotter scan read-scores` | `--half-life-days <days>`, `--under <prefix>`, `--ext <ext>`, `--limit <n>`, `--format <fmt>` |
| [x] | `spotter scan relations` | `--since <days>`, `--under <prefix>`, `--fanout-cap <K>`, `--format <fmt>` |
