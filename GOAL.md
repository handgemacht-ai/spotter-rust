# spotter-rust — Goal & Verification

**Target repo:** `spotter-rust` (new, standalone, public)
**Source of truth being ported:** `mix spotter.transcripts.*` (today's Elixir CLI in this repo)
**Status:** Goal definition

This document defines **what we're building** and **how we know we're done**. Implementation decisions (data model details, crate boundaries, test pyramid, CI tooling choice) are explicitly *not* in scope here — those belong to whoever implements it.

---

## 1. Goal

A standalone open-source Rust CLI called `spotter` that replaces the `mix spotter.transcripts.*` command suite for any Claude Code user, not just this workspace.

### 1.1 Product identity

- **Open source, MIT licensed.**
- **Standalone.** No dependency on the Phoenix app. No HTTP listener. No hook ingestion. Lives entirely as a CLI invoked from the user's shell.
- **Fully local.** No telemetry, no phone-home, no auto-update check. `tracing` logs are local-only.
- **Single user, opportunistic.** Designed for a developer running it from their own shell. Concurrent invocations are not a supported scenario; SQLite's default locking is sufficient.

### 1.2 Distribution

- Published to **crates.io** (`cargo install spotter`).
- **Pre-built binaries** on GitHub Releases from day one, covering:
  - Linux x86_64, Linux aarch64
  - macOS x86_64, macOS aarch64
  - Windows x86_64
- Versioning is CHANGELOG-led with no formal stability contract — users follow the CHANGELOG when upgrading.

### 1.3 Data ownership

- **One global SQLite DB** at `~/.local/share/spotter/spotter.db` (XDG-respecting, overridable via env var).
- `spotter-rust` owns the schema. The DB is *not* shared with any other process or product.
- **Schema migrations run automatically** on every invocation. Before any breaking migration runs, the existing DB is snapshotted to `spotter.db.bak.<version>`.
- **No hook events** flow into this DB. Tool calls are derived purely from Claude Code's JSONL transcript files on disk.

### 1.4 Configuration & discovery

- One config file at `~/.config/spotter/config.toml`.
- Transcript locations are **config-driven only** — no implicit filesystem walks.
- The config also defines **user-friendly project aliases** mapping paths to short names used in CLI output and `--project` filters.

### 1.5 Command surface

`spotter` exposes the following subcommands. The 9 carried over from the Elixir CLI keep their flag surface; the 2 new ones bridge first-run + config management.

**Ported (subcommand surface must match today's `mix spotter.transcripts.*`):**

| Subcommand                         | Carries over from                   |
| ---------------------------------- | ----------------------------------- |
| `spotter transcripts` (help index) | `mix spotter.transcripts`           |
| `spotter transcripts sync`         | `mix spotter.transcripts.sync`      |
| `spotter transcripts search`       | `mix spotter.transcripts.search`    |
| `spotter transcripts inspect`      | `mix spotter.transcripts.inspect`   |
| `spotter transcripts compare`      | `mix spotter.transcripts.compare`   |
| `spotter transcripts aggregate`    | `mix spotter.transcripts.aggregate` |
| `spotter transcripts audit`        | `mix spotter.transcripts.audit`     |
| `spotter transcripts errors`       | `mix spotter.transcripts.errors`    |
| `spotter transcripts health`       | `mix spotter.transcripts.health`    |
| `spotter transcripts sequences`    | `mix spotter.transcripts.sequences` |

**Explicitly dropped:** `mix spotter.transcripts.slice.register` — it only existed to bookmark slices for the Phoenix session viewer. With no Phoenix in scope, the subcommand has no purpose.

**Added for first-run / management:**

- `spotter init` — interactive setup. Scans `~/.claude/projects/`, reads each project's `cwd` metadata, presents a multi-select TUI for the user to choose which projects to track and what alias to give each, then writes `config.toml`.
- `spotter projects` family — `list` / `add` / `remove` / `alias` for managing the project list without hand-editing the config.

### 1.6 Behavioral expectations

- **JSONL parsing** preserves the exhaustiveness invariant from today's Elixir parser: every field in Claude Code's JSONL is explicitly consumed, and unknown fields produce a loud error (so a new Claude Code field is never silently dropped). The format being parsed is Claude Code's, not ours, so this part *is* a faithful port.
- **Tool-call derivation is redefined**, not ported. The Rust version captures **richer context** per tool call — sizes of inputs/outputs, parsed command components for Bash, file paths touched by file-mutating tools — beyond what today's `tool_call_run` records. Specific shape is an implementation decision; the *direction* (richer than today, optimized for analytics) is the goal.
- **Subagent transcripts are fully supported.** Subagent sessions are ingested as first-class sessions with a link to their parent session; their tool calls appear in `search` and `inspect` output alongside main-session tool calls.
- **`search` covers content as well as tool calls.** The same `search` subcommand also supports full-text search across transcript message content, not just tool-call filtering.

### 1.7 Definition of "complete"

The port is complete when **subcommand surface parity** is reached:

- Every subcommand listed in §1.5 exists and runs.
- For every flag accepted by the Elixir version of a carried-over subcommand, the Rust version accepts an equivalent flag with equivalent semantics.
- On representative inputs, every subcommand exits cleanly and produces output that is recognizably what a user familiar with the Elixir CLI would expect.

Underlying data shape, exact output formatting, and performance characteristics may differ from the Elixir version — they are governed by §2, not by this definition.

---

## 2. Verification

Every check below has to pass before a release is cut. They split into eight independent classes; each catches a different failure mode.

### 2.1 Subcommand surface parity

The single concrete check for "complete."

- A checklist file in the repo enumerates every Elixir subcommand × every flag, mapped to its Rust counterpart.
- CI runs each Rust subcommand on a representative fixture and asserts: exit code 0 on the happy path, non-empty stdout, non-empty `--help` output.
- For each `--format json` path, CI parses stdout as JSON and asserts the parse succeeds.
- The checklist is the gate. Missing entries fail CI. The release tag refuses to cut if any entry is unticked.

### 2.2 Code quality gates

Run in CI on every PR; all must be green.

| Gate                | Tool                                                             |
| ------------------- | ---------------------------------------------------------------- |
| Formatting          | `cargo fmt --check` clean                                        |
| Linting             | `cargo clippy --all-targets --all-features -- -D warnings` clean |
| Unsafe code         | `#![forbid(unsafe_code)]` at every crate root                    |
| Dependency audit    | `cargo deny check` (advisories + licenses) clean                 |
| Unused dependencies | `cargo machete` clean                                            |
| Doc coverage        | `cargo rustdoc -- -D missing_docs` clean on library crates       |
| MSRV                | Pinned in `rust-toolchain.toml`; verified in CI                  |
| Test coverage       | `cargo llvm-cov` ≥ 80% lines, ≥ 70% branches                     |

Clippy runs with `pedantic` + `nursery` allowed-by-default; exceptions are documented inline. The point is loud signal, not zero false positives.

### 2.3 Rust best-practice checklist (PR-gated)

Reviewers tick these on every PR. Stored in `.github/PULL_REQUEST_TEMPLATE.md`.

- Errors use `thiserror` in libraries, `anyhow` in the binary crate.
- No `unwrap()` / `expect()` outside `#[cfg(test)]` and `main.rs` (clippy enforces).
- Public APIs in library crates have doc comments with examples; examples compile via `cargo test --doc`.
- I/O paths are `&Path`, not `String`.
- CLI parsing uses `clap` derive macros.
- Subcommand handlers return typed values; output formatting is a separate concern, allowing handlers to be tested without rendering.
- Long-running operations are cancellable on SIGINT.

### 2.4 JSONL exhaustiveness

A hard requirement (CLAUDE.md: "Data Completeness is Priority #1").

- Deserialization uses `serde(deny_unknown_fields)` at every nesting level (top-level, `message`, `usage`).
- A property test mutates known-good fixtures by inserting random extra keys and asserts the parser rejects them.
- A corpus test walks an anonymized fixture corpus checked into the repo and asserts every file parses cleanly.

If Claude Code ships a new transcript field, the parser fails in CI immediately. That is the point.

### 2.5 Schema & data correctness

- **Schema snapshot:** running all migrations against an empty DB produces a schema file checked into the repo (`tests/golden/schema.sql`). PRs that change migrations must update the snapshot; CI verifies it matches a fresh migration run.
- **Migration round-trip:** for every migration, a test runs it forward against a seeded DB and asserts no data loss, declared indexes exist, and `PRAGMA integrity_check` is clean.
- **Derive determinism:** running `sync` twice on the same JSONL input produces a byte-identical DB state, compared via SHA-256 of a canonicalized `sqlite3 .dump`. Catches non-deterministic ordering in derivation.

### 2.6 Performance

Speed is a primary motivation for the port. Numbers are measured by `hyperfine` in CI and tracked over time; regressions block the PR that introduces them.

Absolute targets on the CI runner:

| Scenario                                                 | Target   |
| -------------------------------------------------------- | -------- |
| `spotter --help` (cold start)                            | < 50 ms  |
| `spotter transcripts search` (10k tool calls, no filter) | < 200 ms |
| `spotter transcripts inspect` (single session, 5k msgs)  | < 150 ms |
| `spotter transcripts sync` (1 MB JSONL)                  | < 400 ms |
| `spotter transcripts sync` (100 MB JSONL), peak RSS      | < 100 MB |

Hot paths (`jsonl::parse_session_file`, `analytics::derive_runs`) have `criterion` microbenches. A PR that regresses either by more than 5 % versus `main` fails CI.

### 2.7 End-to-end CLI tests

- Each subcommand has golden outputs (`tests/golden/<subcommand>/<case>.txt`) covering at minimum: happy path, empty-result path, error path.
- Goldens are owned by `spotter-rust` from day one — no Elixir oracle, no cross-process parity to maintain. They capture the *desired* Rust behavior.
- Trace IDs, absolute paths, and timestamps are normalized by a documented redactor before comparison.
- `xtask regen-golden` regenerates them; the PR that changes them must justify the diff in its description.

### 2.8 Release & distribution

A release is only valid when:

- Tagged on `main` with a CHANGELOG entry covering every user-visible change.
- Published to crates.io.
- Pre-built binaries attached to the GitHub Release for all five target triples (§1.2).
- Binary checksums are reproducible — building the same source twice produces identical artifacts.
- `spotter --version` on the published binary matches the git tag.

### 2.9 Acceptance summary

A version of `spotter-rust` is ready to release when, on a clean checkout:

1. The §2.1 subcommand-parity checklist is fully ticked.
2. All §2.2 code-quality gates are green.
3. §2.3 best-practice checklist is signed off on the release PR.
4. §2.4 JSONL exhaustiveness tests pass on the current fixture corpus.
5. §2.5 schema/data correctness tests pass.
6. §2.6 performance targets are met on the CI runner.
7. §2.7 golden tests are green and unchanged (or changes are justified in the release PR).
8. §2.8 release pipeline produces all five binaries with matching checksums.

Once shipped, the CHANGELOG carries the compatibility story — there is no formal stability contract beyond that.
