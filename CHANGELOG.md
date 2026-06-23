# Changelog

## Unreleased

- Captured `Read` line metadata on tool-call runs (`read_total_lines`,
  `read_lines`, `read_truncated`) from the transcript's `toolUseResult.file`,
  and added a `--min-read-lines N` search filter that selects reads which put
  at least `N` lines into the transcript. The count comes from the recorded
  `numLines`, so it reflects what actually entered context rather than the
  file's size on disk (the two diverge when Claude truncates a large read).
  The `search` table gained a `lines_in_context` column. Adds `tool_call_runs`
  columns (schema version 5).

## 0.1.5

- Initial standalone Rust CLI for local Claude Code transcript analytics.
- Added SQLite-backed `transcripts sync`, `search`, `inspect`, `compare`,
  `aggregate`, `audit`, `errors`, `health`, and `sequences`.
- Added `init` and `projects` config-management commands.
- Starts at `0.1.5` because the `spotter` crates.io package already has
  unrelated `0.1.1` through `0.1.4` releases.
