#!/usr/bin/env bash
set -euo pipefail

checklist="docs/subcommand-parity-checklist.md"

if grep -n "\[ \]" "$checklist"; then
  echo "Parity checklist has unticked entries" >&2
  exit 1
fi

required_rows=(
  '`mix spotter.transcripts` | help index'
  '`mix spotter.transcripts.sync` | `--session <id>`'
  '`mix spotter.transcripts.sync` | `--file <path>`'
  '`mix spotter.transcripts.sync` | `--transcript-root <path>`'
  '`mix spotter.transcripts.search` | `--project <id>`'
  '`mix spotter.transcripts.search` | `--worktree <name>`'
  '`mix spotter.transcripts.search` | `--session <id>`'
  '`mix spotter.transcripts.search` | `--tool <name>`'
  '`mix spotter.transcripts.search` | `--command-contains <text>`'
  '`mix spotter.transcripts.search` | `--error-contains <text>`'
  '`mix spotter.transcripts.search` | `--file-path <path>`'
  '`mix spotter.transcripts.search` | `--min-duration <ms>`'
  '`mix spotter.transcripts.search` | `--max-duration <ms>`'
  '`mix spotter.transcripts.search` | `--status <status>`'
  '`mix spotter.transcripts.search` | `--limit <n>`'
  '`mix spotter.transcripts.search` | `--format <fmt>`'
  '`mix spotter.transcripts.search` | `--group-by-session`'
  'new search behavior | content search'
  '`mix spotter.transcripts.inspect` | `--session <id>`'
  '`mix spotter.transcripts.inspect` | `--tool-use-id <id>`'
  '`mix spotter.transcripts.inspect` | `--context <n>`'
  '`mix spotter.transcripts.inspect` | `--status <status>`'
  '`mix spotter.transcripts.inspect` | `--with-messages`'
  '`mix spotter.transcripts.inspect` | `--format <fmt>`'
  '`mix spotter.transcripts.compare` | `--left-session <id>`'
  '`mix spotter.transcripts.compare` | `--right-session <id>`'
  '`mix spotter.transcripts.compare` | `--tool <name>`'
  '`mix spotter.transcripts.compare` | `--command-contains <text>`'
  '`mix spotter.transcripts.compare` | `--group-by <field>`'
  '`mix spotter.transcripts.compare` | `--format <fmt>`'
  '`mix spotter.transcripts.aggregate` | `--project <id>`'
  '`mix spotter.transcripts.aggregate` | `--since <YYYY-MM-DD>`'
  '`mix spotter.transcripts.aggregate` | `--tool <name>`'
  '`mix spotter.transcripts.aggregate` | `--group-by <fields>`'
  '`mix spotter.transcripts.aggregate` | `--format <fmt>`'
  '`mix spotter.transcripts.audit` | `--file <path>`'
  '`mix spotter.transcripts.audit` | `--session <id>`'
  '`mix spotter.transcripts.audit` | `--project <id>`'
  '`mix spotter.transcripts.audit` | `--limit <n>`'
  '`mix spotter.transcripts.audit` | `--format <fmt>`'
  '`mix spotter.transcripts.errors` | `--project <id>`'
  '`mix spotter.transcripts.errors` | `--session <id>`'
  '`mix spotter.transcripts.errors` | `--since <YYYY-MM-DD>`'
  '`mix spotter.transcripts.errors` | `--tool <name>`'
  '`mix spotter.transcripts.errors` | `--top <n>`'
  '`mix spotter.transcripts.errors` | `--classify`'
  '`mix spotter.transcripts.errors` | `--format <fmt>`'
  '`mix spotter.transcripts.health` | `--session <id>`'
  '`mix spotter.transcripts.health` | `--project <id>`'
  '`mix spotter.transcripts.health` | `--since <YYYY-MM-DD>`'
  '`mix spotter.transcripts.health` | `--limit <n>`'
  '`mix spotter.transcripts.health` | `--format <fmt>`'
  '`mix spotter.transcripts.sequences` | `--project <id>`'
  '`mix spotter.transcripts.sequences` | `--since <YYYY-MM-DD>`'
  '`mix spotter.transcripts.sequences` | `--min-length <n>`'
  '`mix spotter.transcripts.sequences` | `--max-length <n>`'
  '`mix spotter.transcripts.sequences` | `--min-occurrences <n>`'
  '`mix spotter.transcripts.sequences` | `--recovery`'
  '`mix spotter.transcripts.sequences` | `--format <fmt>`'
  '`mix spotter.transcripts.slice.register` | Phoenix session-viewer bookmarking is out of scope'
  '`spotter init` | `--claude-projects <path>`, `--yes`'
  '`spotter projects list` | none'
  '`spotter projects add` | `<alias> <path>`'
  '`spotter projects remove` | `<alias>`'
  '`spotter projects alias` | `<old-alias> <new-alias>`'
)

for row in "${required_rows[@]}"; do
  if ! grep -F "$row" "$checklist" >/dev/null; then
    echo "Missing checklist row: $row" >&2
    exit 1
  fi
done
