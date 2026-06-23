# Changelog

## Unreleased

- Captured `Read` file-size metadata on tool-call runs (`read_total_lines`,
  `read_lines`, `read_truncated`) from the transcript's `toolUseResult.file`,
  and added a `--min-read-lines N` search filter that selects reads of files
  with at least `N` total lines. The count comes from the transcript's
  recorded `totalLines`, so it reflects the file's true size even when Claude
  truncated the visible content. The `search` table gained a `file_lines`
  column. Adds `tool_call_runs` columns (schema version 5).

## 0.1.5

- Initial standalone Rust CLI for local Claude Code transcript analytics.
- Added SQLite-backed `transcripts sync`, `search`, `inspect`, `compare`,
  `aggregate`, `audit`, `errors`, `health`, and `sequences`.
- Added `init` and `projects` config-management commands.
- Starts at `0.1.5` because the `spotter` crates.io package already has
  unrelated `0.1.1` through `0.1.4` releases.
