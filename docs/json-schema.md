# JSON output schema and ID conventions

Contract for `--format json` output of the query verbs, designed so one verb's
output feeds another verb's input (JSON in / JSON out, composable via pipes).
The `transcripts` (SQLite) and `scan` (in-memory) backends emit identical JSON;
integration tests pin that parity.

## Stable entity IDs

- **Session id** (`session_id`): the Claude Code session id from the
  transcript. Subagent sessions use `<parent_session_id>:agent:<agent_id>`.
  Present on every run, message hit, and audit row.
- **Run id** (`run_id`): `{session_id}:{tool_use_id}`, composed entirely from
  transcript data (never SQLite rowids), so it is stable across re-syncs and
  identical on both backends. Every serialized tool-call run (any JSON object
  carrying both `session_id` and `tool_use_id`) is decorated with it. Because
  subagent session ids contain `:` themselves, split a run id at its **last**
  `:` to recover the parts; tool use ids never contain `:`.
- **Message ordinals**: runs carry `start_ordinal`/`end_ordinal`; message hits
  carry `session_id` + `ordinal`. Together they address exact transcript
  windows.

## IDs in (drill-down)

- `inspect --session <id> [--tool-use-id <id>]` — existing entry points.
- `inspect --run <session_id>:<tool_use_id>` — convenience form taking a run id
  straight from `search`/`errors` output. Conflicts with `--session` and
  `--tool-use-id`.
- `inspect --ordinals <min>:<max>` — keep only runs whose
  `[start_ordinal, end_ordinal]` overlaps the window.
- `errors` patterns carry `sample_runs` (up to 3 run ids per pattern) for
  direct drill-down into failing runs.
- `similar --to-run <session_id>:<tool_use_id>` takes a run id as its
  exemplar (resolved independently of the scope filters).

## `--fields` projection

`search`, `inspect`, `errors`, `aggregate`, `profile`, `sample`, and
`similar` accept `--fields <keys>` (repeatable, comma-separated) to keep only
the named top-level JSON keys (`similar` is transcripts-only; the others work
on both backends). For array output the projection applies to each element;
for object output to the object's own keys. Unknown keys are a loud error
listing the available fields. Table output ignores `--fields`.

## Output shapes

Top-level key sets per verb, verified against real output by
`tests/json_schema_doc.rs` (sorted, comma-separated):

```json-keys
search.run: agent_id, canonical_cwd, command, command_args, command_fingerprint, command_program, duration_ms, end_ordinal, error_content, external_session_id, file_paths, finished_at, input_size, input_summary, is_subagent, output_size, parent_session_id, project_alias, read_lines, read_total_lines, read_truncated, run_id, session_id, source_scope, start_ordinal, started_at, status, tool_name, tool_use_id, worktree_name
search.message_hit: external_session_id, ordinal, project_alias, record_type, role, score, session_id, snippet, timestamp
search.session_group: matches, ordinal_range, project_alias, session_id, tool_use_ids, worktree_name
inspect.run: agent_id, canonical_cwd, command, command_args, command_fingerprint, command_program, duration_ms, end_ordinal, error_content, external_session_id, file_paths, finished_at, input_size, input_summary, is_subagent, output_size, parent_session_id, project_alias, read_lines, read_total_lines, read_truncated, run_id, session_id, source_scope, start_ordinal, started_at, status, tool_name, tool_use_id, worktree_name
inspect.with_messages: context, runs
inspect.message_hit: external_session_id, ordinal, project_alias, record_type, role, session_id, snippet, timestamp
errors.top_level: pattern_count, patterns, total_errors, total_tool_calls
errors.pattern: category, count, error_rate, fingerprint, first_seen, last_seen, preventability, sample_error, sample_runs, sample_sessions, tool_name, total_tool_calls
aggregate.top_level: groups, session_count, top_errors, total_runs
aggregate.group: avg_duration_ms, count, error_pct, errors, key, p95_duration_ms
aggregate.top_error: count, fingerprint, sample, tool_name
sequences.top_level: frequent_sequences, retry_patterns, session_count
sequences.row: count, pattern
sequences.recovery_row: avg_retries, category, recovery_rate, retry_rate, total_errors
profile.top_level: errors, outliers, overview, recovery, token_health, tools
profile.overview: first_started_at, last_ended_at, message_count, projects, run_count, session_count
profile.tool: avg_duration_ms, count, error_pct, errors, p95_duration_ms, tool_name
profile.error_pattern: category, count, fingerprint, preventability, sample_runs, tool_name
profile.error_category: category, count, preventability
profile.recovery_row: avg_retries, category, recovery_rate, retry_rate, total_errors
profile.token_health: flagged_sessions, peak_context, session_count, total_cache_misses, total_jumps, total_waste_tokens
profile.flagged_session: cache_misses, jumps, session_id
profile.slowest_run: duration_ms, run_id, tool_name
profile.error_hotspot: error_pct, errors, runs, session_id
sample.top_level: population, runs, seed, strata, stratify_by
sample.run: agent_id, canonical_cwd, command, command_args, command_fingerprint, command_program, duration_ms, end_ordinal, error_content, external_session_id, file_paths, finished_at, input_size, input_summary, is_subagent, output_size, parent_session_id, project_alias, read_lines, read_total_lines, read_truncated, run_id, session_id, source_scope, start_ordinal, started_at, status, tool_name, tool_use_id, worktree_name
sample.stratum: key, population, sampled
similar.run: agent_id, canonical_cwd, command, command_args, command_fingerprint, command_program, duration_ms, end_ordinal, error_content, external_session_id, file_paths, finished_at, input_size, input_summary, is_subagent, output_size, parent_session_id, project_alias, read_lines, read_total_lines, read_truncated, run_id, session_id, similarity, source_scope, start_ordinal, started_at, status, tool_name, tool_use_id, worktree_name
```

Notes on individual shapes:

- `search` emits an array of runs (`search.run`). With `--content-contains` it
  emits ranked message hits (`search.message_hit`); with `--group-by-session`
  it emits per-session groups (`search.session_group`, where `ordinal_range` is
  a `"<min>-<max>"` string and `tool_use_ids` lists the matching ids).
- Content search (`--content-contains`) is BM25-ranked full-text retrieval.
  Default semantics: the needle is tokenized (lowercase, split on
  non-alphanumerics) and a chunk must contain **all** terms (AND). With
  `--exact` the needle is matched as a single phrase (consecutive tokens — the
  pre-BM25 behavior). `score` is the FTS5 `bm25()` rank value of the message's
  best chunk: negative, **lower is a better match**, rounded to 6 decimals on
  both backends. Long messages (> 2000 chars) are indexed as overlapping
  chunks (2000 chars, 200 overlap); each hit resolves to its parent message
  `ordinal` (usable with `inspect --ordinals`) and `snippet` is a ~180-char
  window anchored at the first match in the best chunk. Both backends compute
  scores with the identical formula (idf `ln((N - n + 0.5) / (n + 0.5))`,
  clamped below at `1e-6`; `k1 = 1.2`, `b = 0.75`), so ranked JSON is
  byte-identical between `transcripts` and `scan`.
- `inspect` emits an array of runs (`inspect.run`, same shape as
  `search.run`). With `--with-messages` it emits an object
  (`inspect.with_messages`) whose `runs` and `context` (message hits) arrays
  carry the session ids and ordinals needed to request exact windows. Context
  hits (`inspect.message_hit`) are ordinal-window lookups, not ranked — they
  carry no `score`.
- `errors` emits `errors.top_level`; each pattern (`errors.pattern`) groups
  failures by normalized fingerprint. `category`, `preventability`,
  `total_tool_calls`, and `error_rate` are null unless `--classify` is passed.
- `aggregate` emits `aggregate.top_level`; group rows (`aggregate.group`) key
  on the `--group-by` fields and do not map to individual runs. `top_errors`
  rows (`aggregate.top_error`) are fingerprints, not runs.
- `sequences` emits `sequences.top_level`; with `--recovery` a
  `recovery_stats` array of `sequences.recovery_row` objects is added.
  Sequence rows (`sequences.row`) are n-gram patterns over tool names and do
  not map to individual runs; `retry_patterns` rows share the same keys but
  `pattern` is a single string rather than an array.
- `profile` (both backends, default `--format json`) emits a single envelope
  (`profile.top_level`) composed from the other verbs' analytics: scope
  `overview`, per-tool distribution (`profile.tool`), error taxonomy
  (`profile.error_category` + `profile.error_pattern` with `sample_runs` for
  immediate `inspect --run` drill-down), per-category `recovery` rates
  (`profile.recovery_row`), `token_health` totals plus flagged sessions
  (`profile.flagged_session`), and `outliers` — slowest runs
  (`profile.slowest_run` with `run_id`), the most-retried pattern (null when
  none), and per-session error hotspots (`profile.error_hotspot`). Scoped by
  `--session`/`--project`/`--since` like other verbs. Error patterns, slowest
  runs, flagged sessions, and hotspots are each capped at `--top N`
  (default 5) so the envelope stays small enough to read whole.
  `--fields` selects envelope sections (e.g. `--fields overview,errors`).
- `sample` (both backends, default `--format json`) draws a stratified,
  seeded sample of tool-call runs (`sample.top_level`). `runs` uses the same
  shape as `search.run` (run ids included), `strata` (`sample.stratum`)
  reports per-stratum population vs sampled counts, and `seed`/`population`/
  `stratify_by` echo the draw parameters. Stratification policy: every
  non-empty stratum first gets an equal share `count / strata` (capped at its
  population) so rare strata are always represented; leftover slots fill one
  at a time into the stratum with the most remaining headroom. Sampling is
  seeded (splitmix64, default seed 42) and never wall-clock random: the same
  seed over the same corpus draws the same runs on both backends.
  `--stratify-by` accepts `tool_name` (default), `status`, `category` (the
  12-class error taxonomy; non-error runs form `none`), or `session`. The
  draw happens over the filtered population (`--session`/`--project`/
  `--since`/`--tool`/`--status`); no `--limit`-style cap applies beyond
  `-n/--count`.
- `similar` (**transcripts-only**, like `scan read-scores`/`relations` are
  scan-only) fans out "more like this" from one exemplar: `transcripts
  similar --to-run <run_id> [-n K]` emits a JSON array of `similar.run`
  objects — the full `search.run` shape plus `similarity` (cosine similarity
  of L2-normalized embeddings, descending; rounded to 6 decimals). Each run
  is embedded as a compact document (tool name + command/input summary +
  error content) by a local `all-MiniLM-L6-v2` BERT model running in-process
  via candle — no cloud calls; the only network access anywhere is the
  explicit one-time `spotter embed init` download into the model cache dir
  (override with `--model-dir` or `SPOTTER_MODEL_DIR`). Embeddings are cached
  in the derived `run_embeddings` table keyed by
  `(session_id, tool_use_id, model)` (rebuildable derived data, same pattern
  as `messages_fts`), and `similar` fails fast with the remedy when the model
  files are absent. Scope flags (`--session`/`--project`/`--since`/`--tool`/
  `--status`) narrow the population fanned out over; the exemplar itself is
  resolved independently of them and excluded from results.
