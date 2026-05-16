# Release Readiness Audit

Status as of 2026-05-16: repo-owned implementation and verification gates are
green in CI; release PR signoff is enforced by the release preflight and the
current live signoff evidence is tracked in issue #1. The current maintainer
decision is not to publish to crates.io or create a release tag for now, so
external release state is intentionally not complete. `README.md` documents the
current source-only install/run path while that decision is active.

Current run IDs and artifact-download evidence are recorded in
[issue #1](https://github.com/handgemacht-ai/spotter-rust/issues/1). Refresh
that issue after any new release-candidate commit; this document records the
audit map and durable gates, not an always-current run log.

The final completion gates are intentionally still failing:

```sh
scripts/check-crates-io-release-ready.py
# crates.io package name already exists: spotter; current owners: ['kohbis']; set CRATES_IO_OWNER_LOGIN after ownership is ready

scripts/check-github-release-config.py --repo handgemacht-ai/spotter-rust
# missing repository secret: CRATES_IO_TOKEN
# missing repository variable: CRATES_IO_OWNER_LOGIN

scripts/check-release-complete.py
# missing fetched release tag: v0.1.5
```

## Objective

Build `spotter`, a standalone MIT-licensed Rust CLI that replaces the
`mix spotter.transcripts.*` transcript command suite for Claude Code users, and
verify it against `GOAL.md`.

## Completion Audit Checklist

This checklist maps the explicit `GOAL.md` requirements to concrete evidence.
Items marked blocked require external release ownership or credentials and are
not satisfied by local implementation alone.

| GOAL requirement | Status | Evidence |
| --- | --- | --- |
| Public standalone Rust CLI named `spotter` | Done | `Cargo.toml` package name is `spotter`; `src/main.rs` is a CLI entrypoint |
| Open source under MIT | Done | `LICENSE`; `Cargo.toml` `license = "MIT"` |
| No Phoenix app dependency, HTTP listener, hook ingestion, telemetry, phone-home, or auto-update | Verified | `scripts/check-local-only.py` is run by CI and release workflows; `README.md` documents local-only behavior |
| Single-user local operation with SQLite default locking | Done | `rusqlite` is used directly; no server or multi-process coordination layer exists |
| Published to crates.io as `spotter` | Deferred | Current maintainer decision is no crates.io publish for now. `README.md` documents source-only install/run commands. `scripts/check-crates-io-release-ready.py` still fails because crates.io already has `spotter` owned by `kohbis`; crates.io has no `@handgemacht-ai/spotter`-style package scope, so any future publish requires owner transfer/sharing or a flat rename such as `handgemacht-spotter` |
| Prebuilt GitHub Release binaries for Linux x86_64, Linux aarch64, macOS x86_64, macOS aarch64, Windows x86_64 | Partially verified | `.github/workflows/release.yml` contains all five targets; `publish=false` dry runs have built all five targets and their downloaded artifacts have passed `scripts/check-github-release-assets.py --expect-version 0.1.5 --require-runnable-host`; no tag-triggered GitHub Release exists yet |
| CHANGELOG-led versioning | Done | `CHANGELOG.md` has a `0.1.5` entry; release workflow checks manifest version against the tag and changelog |
| Global SQLite DB at XDG-style data dir with override | Done | `src/paths.rs`; CLI accepts `--db`; README documents `~/.local/share/spotter/spotter.db` |
| Config file at XDG-style config dir with override | Done | `src/paths.rs`, `src/config.rs`; CLI accepts `--config`; README documents `~/.config/spotter/config.toml` |
| Schema owned by this project with automatic migrations | Verified | `src/db.rs` runs migrations in `open`; `tests/golden/schema.sql` and `tests/schema_and_determinism.rs` verify schema and migrations |
| Breaking migration snapshots to `spotter.db.bak.<version>` | Verified | `src/db.rs` `snapshot_database`; `tests/schema_and_determinism.rs` checks backup creation |
| Tool calls derived only from Claude Code JSONL transcripts on disk | Verified | `src/db.rs` ingest derives runs from parsed JSONL; `scripts/check-local-only.py` prevents network/listener dependencies |
| Transcript locations config-driven, with no implicit filesystem walks | Verified | `src/config.rs`, `src/cli.rs`; `tests/cli_path_overrides.rs` covers path/config behavior |
| Project aliases for output and `--project` filters | Verified | `src/config.rs`; `spotter projects` tests and parity tests cover alias filtering |
| `spotter init` first-run setup | Verified | `src/cli.rs`; `tests/cli_goldens.rs` covers init happy/empty/error outputs |
| `spotter projects list/add/remove/alias` | Verified | `src/cli.rs`; golden tests cover all project commands |
| Ported transcript command index and commands | Verified | `docs/subcommand-parity-checklist.md`; `tests/cli_surface.rs`; `tests/cli_*` integration tests |
| Carried-over Elixir flags accepted with equivalent semantics | Verified | `docs/subcommand-parity-checklist.md`; `tests/cli_flag_parity.rs`; `scripts/check-parity-checklist.sh` |
| `mix spotter.transcripts.slice.register` intentionally dropped | Done | `docs/subcommand-parity-checklist.md` documents the drop; no Rust command is exposed |
| JSONL parser rejects unknown fields at all required levels | Verified | `serde(deny_unknown_fields)` in `src/jsonl.rs`; property and corpus tests in `src/jsonl.rs` and `tests/jsonl_corpus.rs` |
| Richer tool-call context than Elixir `tool_call_run` | Verified | `src/db.rs` stores command components, fingerprints, sizes, file paths, source scope, and error content; covered by CLI search/flag tests |
| Subagent transcripts are first-class and linked to parents | Verified | `src/db.rs` session/tool-run schema has parent and subagent fields; subagent fixture and tests cover sync/search/inspect output |
| Search covers message content as well as tool calls | Verified | `messages_fts` path in `src/db.rs`; `--content-contains` in `src/cli.rs`; tests cover content search |
| Code quality gates: fmt, clippy, tests, rustdoc, doctests, deny, machete, unsafe, no production unwrap, MSRV | Verified | `.github/workflows/ci.yml`; `scripts/check-ci-workflow.py`; current release-candidate CI evidence is tracked in issue #1 |
| PR best-practice checklist exists | Done | `.github/PULL_REQUEST_TEMPLATE.md` |
| Release PR checklist signed off | Gated | `.github/workflows/release.yml` runs `scripts/check-release-pr-signoff.py` during publish preflight, and `scripts/check-release-complete.py` re-checks the fetched release tag commit; current live signoff evidence is tracked in issue #1 |
| Coverage thresholds 80 percent lines and 70 percent branches | Verified | CI `coverage` job runs `scripts/check-coverage-json.py target/llvm-cov.json --lines 80 --branches 70`; local audit recorded 89.42 percent lines and 75.60 percent branches |
| Schema snapshot, migration round-trip, and deterministic sync | Verified | `tests/golden/schema.sql`; `tests/schema_and_determinism.rs` |
| Performance targets and hot-path benchmarks | Verified | `.github/workflows/ci.yml` performance job enforces the absolute targets and hot-path benchmark coverage; current release-candidate CI evidence is tracked in issue #1 |
| Golden CLI outputs with redaction and regen command | Verified | `tests/golden/**`, `tests/cli_goldens.rs`, `xtask`; CI checks `./xtask regen-golden` leaves no diff |
| Release tag on `main` | Deferred | No local or remote `v0.1.5` tag exists; do not create it while the no-publish decision is active |
| GitHub Release assets and checksums attached | Deferred | No GitHub Release exists; `publish=false` release dry runs verify the five target artifacts without creating a release |
| Published binary `spotter --version` matches tag | Deferred | Release workflow verifies runnable targets in dry runs, but no published binaries exist while the no-publish decision is active |

## Local Evidence

| GOAL item | Artifact or verifier |
| --- | --- |
| MIT/open-source project identity | `LICENSE`, `Cargo.toml` package license |
| Standalone local CLI | `src/main.rs`, `src/cli.rs`, no Phoenix/app dependency |
| No telemetry, listener, phone-home, or auto-update | `scripts/check-local-only.py` in CI and release workflows |
| Global SQLite DB with XDG-style defaults and overrides | `src/paths.rs`, `README.md` |
| Config-driven transcript roots and aliases | `src/config.rs`, `spotter init`, `spotter projects *` tests |
| Ported transcript subcommands exist | `tests/cli_surface.rs`, `tests/cli_goldens.rs` |
| Carried-over flag surface is accepted | `docs/subcommand-parity-checklist.md`, source Mix task verification notes in that checklist, `tests/cli_flag_parity.rs` |
| Repeatable compare cohort flags match Elixir `:keep` switches | `tests/cli_flag_parity.rs` |
| `slice.register` intentionally dropped | `docs/subcommand-parity-checklist.md` |
| JSONL unknown fields fail loudly | `serde(deny_unknown_fields)` structs and property tests in `src/jsonl.rs` |
| JSONL corpus parses | `tests/jsonl_corpus.rs` |
| Fixture corpus is synthetic and public-safe | `scripts/make-test-fixtures.py`, `scripts/check-fixtures-scrubbed.py` |
| Rich tool-call derivation | `src/db.rs`, `tests/cli_sync_search.rs`, `tests/cli_flag_parity.rs` |
| Subagent transcripts are first-class | `tests/cli_remaining_commands.rs`, subagent fixture under `tests/fixtures/transcripts/` |
| Content search | `src/db.rs` FTS path and `tests/cli_sync_search.rs` |
| Schema snapshot | `tests/golden/schema.sql`, `tests/schema_and_determinism.rs` |
| Migration snapshot/backfill/integrity | `migration_from_v2_snapshots_backfills_and_preserves_data` in `tests/schema_and_determinism.rs` |
| Sync determinism | `syncing_same_jsonl_twice_is_deterministic` in `tests/schema_and_determinism.rs` |
| Golden CLI outputs | `tests/golden/**`, `tests/cli_goldens.rs`, `./xtask regen-golden` |
| Code quality gates | `.github/workflows/ci.yml`, `scripts/check-ci-workflow.py` |
| Release workflow coverage | `.github/workflows/release.yml`, `scripts/check-release-workflow.py`; release tags fail fast if the release PR signoff is missing, `CRATES_IO_TOKEN` is missing, or the manifest package/version is not publishable on crates.io; `workflow_dispatch` can dry-run verify/build without publishing; workflow actions are pinned to Node 24-compatible majors |
| GitHub release config preflight | `scripts/check-github-release-config.py --repo handgemacht-ai/spotter-rust` verifies the `CRATES_IO_TOKEN` repository secret name and `CRATES_IO_OWNER_LOGIN` repository variable before tagging |
| Coverage thresholds | fresh `cargo llvm-cov`: 89.42% lines, 75.60% branches |
| Source-only install path | `README.md`; `cargo install --path . --locked`; `cargo run --locked -- --help` |
| Package dry run | `cargo package --locked`; `cargo publish --dry-run --locked`; current clean-checkout evidence is tracked in issue #1 |
| Release dry run | GitHub Actions Release `publish=false` dry runs verify, build all five target artifacts/checksums, and skip publish; current run evidence is tracked in issue #1 |
| Public git history | Public `main` is pushed to `https://github.com/handgemacht-ai/spotter-rust` |
| Current CI evidence | Tracked in issue #1 after each release-candidate commit |
| Current release dry-run evidence | Tracked in issue #1 after each release-candidate commit |
| Dry-run artifact verification | `scripts/check-github-release-assets.py <download-dir> --expect-version 0.1.5 --require-runnable-host` verifies the downloaded workflow artifacts |
| Final release completion verifier | `scripts/check-release-complete.py` verifies tag ancestry, release PR signoff, CHANGELOG, crates.io version, GitHub Release metadata/assets, asset checksums, and `cargo install spotter --version <version>` |

## Verified Commands

The following local checks have passed on the current tree:

```sh
cargo fmt --check
cargo check --all-targets --all-features --locked
cargo clippy --all-targets --all-features --locked -- -D warnings -W clippy::pedantic -W clippy::nursery
cargo test --all-targets --all-features --locked
cargo rustdoc --lib --locked -- -D missing_docs
cargo test --doc --locked
cargo check --benches --locked
cargo deny check
cargo machete
cargo +nightly llvm-cov --all-targets --all-features --branch --json --output-path target/llvm-cov.json
scripts/check-coverage-json.py target/llvm-cov.json --lines 80 --branches 70
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
cargo install --path . --locked --root target/install-smoke
scripts/check-install-smoke.sh
```

The latest focused audit slice also passed:

```sh
cargo test --locked --test cli_surface --test cli_flag_parity
scripts/check-parity-checklist.sh
scripts/check-release-workflow.py
scripts/check-ci-workflow.py
```

Performance checks have also passed locally:

```sh
cargo bench --locked --bench hot_paths -- --sample-size 10
scripts/check-criterion-regression.py target/criterion --max-regression-percent 5 jsonl_parse_session_file analytics_derive_runs_via_ingest
scripts/check-rss.py --max-kb 102400 -- target/release/spotter --db target/perf-rss-full/sync-100mib-final.db transcripts sync --file target/perf-rss-full/sync-100mib.jsonl
```

Observed local performance evidence:

| Scenario | Result |
| --- | --- |
| `spotter --help` | about 0.00s |
| `transcripts search` on 10k calls | about 0.04s |
| `transcripts inspect` on 5k messages | about 0.01s |
| `transcripts sync` on 1 MiB JSONL | about 0.13s |
| `transcripts sync` on 100 MiB JSONL | max RSS 63760 KiB |

## External Blockers

These GOAL requirements still need external release work:

| Requirement | Current state |
| --- | --- |
| Tagged on `main` | Deferred by maintainer decision; no local or remote `v0.1.5` tag exists |
| GitHub release matrix produced all five binaries | Manual `publish=false` dry runs have succeeded for all five build targets; tag-triggered release has not run |
| GitHub Release assets and checksums attached | Deferred by maintainer decision; requires pushing the release tag |
| Published to crates.io | Deferred by maintainer decision. If this changes, [issue #1](https://github.com/handgemacht-ai/spotter-rust/issues/1) still tracks the required ownership/name work: `CRATES_IO_TOKEN` is not configured or visible, crates.io already has `spotter` under owner `kohbis`, and crates.io has no scoped package name like `@handgemacht-ai/spotter` |
| Published binary `spotter --version` matches tag | Local binary reports `spotter 0.1.5`; published binaries do not exist yet |

## Handoff Steps

1. Do not create `v0.1.5`, do not publish to crates.io, and do not create a
   GitHub Release while the no-publish decision is active.
2. Keep `README.md` aligned with the source-only install/run path while the
   no-publish decision is active.
3. Keep CI and `publish=false` release dry runs green for the current
   release-candidate commit.
4. If the publish decision changes, resolve the crates.io package-name path:
   owner transfer/sharing for `spotter`, or a flat rename such as
   `handgemacht-spotter` while deciding whether the installed binary remains
   `spotter`.
5. Configure the repository `CRATES_IO_TOKEN` secret and `CRATES_IO_OWNER_LOGIN`
   variable only after publish ownership is ready.
6. Before tagging, run `scripts/check-release-pr-signoff.py`; if it fails,
   repeat the release PR signoff for the final release commit.
7. Create and push the `v0.1.5` tag on `main`, let
   `.github/workflows/release.yml` publish, and verify GitHub Release
   assets/checksums plus crates.io package availability.
