# crates.io Name Decision

`GOAL.md` requires publishing this CLI as `spotter` so users can run
`cargo install spotter`.

That name is already occupied on crates.io by an unrelated package:

| Field | Current value |
| --- | --- |
| Package | `spotter` |
| Description | AWS EC2 Spot Instance Advisor CLI Tool |
| Owner | `kohbis` |
| Repository | `https://github.com/kohbis/spotter` |
| Existing binary | `spotter` |
| Latest crates.io version | `0.1.4` |
| Latest GitHub release | `v0.1.4` |
| Repository state | public, not archived, issues enabled |

This is a semantic package and binary conflict, not an empty-name reservation.
The current manifest version here is `0.1.0`, so even an ownership transfer
would not make `cargo install spotter` install this CLI unless the release
version is bumped above the existing `0.1.4`.

## crates.io Policy Constraints

The Cargo Book documents the relevant crates.io constraints:

- crate names are first-come-first-serve; once a crate name is taken, it cannot
  be used for another crate,
- publishing a version is permanent; that version cannot be overwritten and the
  code cannot be deleted, and
- owners can publish new versions and manage ownership.

Primary references:

- `https://doc.rust-lang.org/cargo/reference/publishing.html`
- `https://doc.rust-lang.org/cargo/commands/cargo-owner.html`

## Valid Paths

### Keep The `spotter` Name

This path preserves the literal `GOAL.md` install command.

1. Ask the current owner whether they are willing to transfer or share the
   crates.io package name for the Claude Code transcript analytics CLI.
2. If the owner agrees, configure `CRATES_IO_OWNER_LOGIN` to the owner login
   that will publish this package.
3. Bump `Cargo.toml` and `CHANGELOG.md` to a version newer than the existing
   crates.io `spotter` max version.
4. Re-run `scripts/check-crates-io-release-ready.py`.
5. Follow `docs/release-runbook.md`.

Suggested request text:

```text
Hi Kohei,

We are preparing to release an open-source Rust CLI named `spotter` for local
Claude Code transcript analytics:
https://github.com/handgemacht-ai/spotter-rust

The project goal currently requires `cargo install spotter`, but crates.io
already has your unrelated AWS EC2 Spot Instance Advisor CLI under that package
name. Would you be open to discussing a crates.io ownership transfer or another
arrangement? We do not want to overwrite or confuse users of your existing CLI
without your explicit agreement.
```

### Rename The Crate

This path avoids colliding with an existing package, but it changes the
explicit `GOAL.md` requirement.

1. Pick a new crates.io package name.
2. Update `Cargo.toml`, `CHANGELOG.md`, `README.md`, `GOAL.md`, release
   workflow checks, and install documentation.
3. Decide whether the installed binary should remain `spotter` or match the new
   package name.
4. Re-run the full verification matrix and release dry run.

## Preflight Enforcement

`scripts/check-crates-io-release-ready.py` fails if:

- the package name exists and `CRATES_IO_OWNER_LOGIN` is unset,
- the configured owner does not match a crates.io owner,
- the package version already exists, or
- the manifest version is not newer than the existing crates.io max version.
