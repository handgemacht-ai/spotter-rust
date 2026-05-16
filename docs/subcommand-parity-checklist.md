# Subcommand Parity Checklist

This checklist maps the carried-over Elixir command surface to the Rust CLI.
Every checked row has a fixture-backed integration path in `tests/`.

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

| Status | Rust command | Flags |
| --- | --- | --- |
| [x] | `spotter init` | `--claude-projects <path>`, `--yes` |
| [x] | `spotter projects list` | none |
| [x] | `spotter projects add` | `<alias> <path>` |
| [x] | `spotter projects remove` | `<alias>` |
| [x] | `spotter projects alias` | `<old-alias> <new-alias>` |
