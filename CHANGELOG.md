# Changelog

## Unreleased

- Extended the fail-on-unknown parsing rule beyond JSONL record fields:
  transcript record types and content block types are now checked against
  explicit known sets (`tool_use` and `tool_result` blocks also have field
  allow-lists), `config.toml` rejects unknown keys at both levels, and
  `--format` / `--group-by` are validated enum flags. Previously an
  unrecognized record type became `system`, an unrecognized block type was
  dropped, unknown config keys were ignored, an unrecognized `--format` fell
  back to table output, and an unrecognized `--group-by` key produced an
  `unsupported:<key>` group. All of these are now errors.

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
