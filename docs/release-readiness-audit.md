# Release Readiness Audit

Status as of 2026-05-16: local implementation and local verification are ready;
external release state is not complete because there is no release PR, no
pushed release tag, no GitHub Release artifacts, and no crates.io publish.

## Objective

Build `spotter`, a standalone MIT-licensed Rust CLI that replaces the
`mix spotter.transcripts.*` transcript command suite for Claude Code users, and
verify it against `GOAL.md`.

## Local Evidence

| GOAL item | Artifact or verifier |
| --- | --- |
| MIT/open-source project identity | `LICENSE`, `Cargo.toml` package license |
| Standalone local CLI | `src/main.rs`, `src/cli.rs`, no Phoenix/app dependency |
| No telemetry, listener, phone-home, or auto-update | `scripts/check-local-only.py` in CI and release workflows |
| Global SQLite DB with XDG-style defaults and overrides | `src/paths.rs`, `README.md` |
| Config-driven transcript roots and aliases | `src/config.rs`, `spotter init`, `spotter projects *` tests |
| Ported transcript subcommands exist | `tests/cli_surface.rs`, `tests/cli_goldens.rs` |
| Carried-over flag surface is accepted | `docs/subcommand-parity-checklist.md`, `tests/cli_flag_parity.rs` |
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
| Release workflow coverage | `.github/workflows/release.yml`, `scripts/check-release-workflow.py`; release tags fail fast if `CRATES_IO_TOKEN` is missing or the manifest package/version is not publishable on crates.io; `workflow_dispatch` can dry-run verify/build without publishing |
| Coverage thresholds | fresh `cargo llvm-cov`: 89.42% lines, 75.60% branches |
| Packaging and install path | `cargo package --allow-dirty --locked`, `cargo publish --dry-run --allow-dirty --locked`, `cargo install --path . --locked` |
| Public git history | Public `main` is pushed to `https://github.com/handgemacht-ai/spotter-rust` |

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
cargo package --allow-dirty --locked
cargo publish --dry-run --allow-dirty --locked
cargo install --path . --locked --root target/install-smoke
scripts/check-install-smoke.sh
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
| Release PR checklist signed off | No PR exists in this checkout |
| Tagged on `main` | Local `v0.1.0` tag points at `main`; it has not been pushed to `origin` |
| GitHub release matrix produced all five binaries | Release workflow exists but has not run from a remote tag |
| GitHub Release assets and checksums attached | Not done; requires pushing the release tag |
| Published to crates.io | Blocked by [issue #1](https://github.com/handgemacht-ai/spotter-rust/issues/1): `CRATES_IO_TOKEN` is not configured, and crates.io already has `spotter` under another owner; tag pushes fail in preflight until credentials, `CRATES_IO_OWNER_LOGIN`, and package/version publishability are resolved |
| Published binary `spotter --version` matches tag | Local binary reports `spotter 0.1.0`; published binaries do not exist yet |

## Handoff Steps

1. Resolve the crates.io package-name/ownership decision for `spotter`.
2. Configure the repository `CRATES_IO_TOKEN` secret and `CRATES_IO_OWNER_LOGIN` variable when publish ownership is ready.
3. Open or otherwise complete the release checklist in `.github/PULL_REQUEST_TEMPLATE.md`.
4. Run a manual release dry run with `gh workflow run release.yml --ref main -f publish=false`.
5. Push the `v0.1.0` tag on `main`.
6. Let `.github/workflows/release.yml` build and publish artifacts.
7. Verify GitHub Release assets/checksums and crates.io package availability.
