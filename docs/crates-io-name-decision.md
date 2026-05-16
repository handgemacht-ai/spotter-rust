# crates.io Name Decision

`GOAL.md` requires publishing this CLI as `spotter` so users can run
`cargo install spotter`.

Current maintainer decision as of 2026-05-16: do not publish this package to
crates.io and do not create a release tag for now. Keep verification green and
use `publish=false` release dry runs only.

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
| Public issues | none returned by `gh issue list -R kohbis/spotter --state all --limit 20` on 2026-05-16 |

This is a semantic package and binary conflict, not an empty-name reservation.
The planned release version here is `0.1.5` so it is newer than the existing
`0.1.4` max version. That only removes the version-order blocker; the
package-name and ownership decision still has to be resolved before publishing.

## crates.io Policy Constraints

The Cargo Book documents the relevant crates.io constraints:

- crate names are first-come-first-serve; once a crate name is taken, it cannot
  be used for another crate,
- publishing a version is permanent; that version cannot be overwritten and the
  code cannot be deleted, and
- owners can publish new versions and manage ownership.

crates.io package names are flat. A scoped package name such as
`@handgemacht-ai/spotter` is npm-style syntax and is not a crates.io package
name. GitHub users or teams can own a crate, but ownership does not namespace the
published package name.

Primary references:

- `https://doc.rust-lang.org/cargo/reference/publishing.html`
- `https://doc.rust-lang.org/cargo/commands/cargo-owner.html`

## Valid Paths

### Do Not Publish For Now

This is the active path.

1. Do not create or push `v0.1.5`.
2. Do not configure `CRATES_IO_TOKEN` or `CRATES_IO_OWNER_LOGIN`.
3. Continue using CI, local package checks, `cargo publish --dry-run --locked`,
   and `publish=false` release workflow runs as verification evidence.
4. Revisit one of the publish paths below only if the maintainer decision
   changes.

### Keep The `spotter` Name

This path preserves the literal `GOAL.md` install command.

1. Ask the current owner whether they are willing to transfer or share the
   crates.io package name for the Claude Code transcript analytics CLI.
2. If the owner agrees, configure `CRATES_IO_OWNER_LOGIN` to the owner login
   that will publish this package.
3. Keep `Cargo.toml` and `CHANGELOG.md` at a version newer than the existing
   crates.io `spotter` max version.
4. Re-run `scripts/check-crates-io-release-ready.py`.
5. Follow `docs/release-runbook.md`.

Suggested request text:

```text
Hi,

We are preparing to release an open-source Rust CLI named `spotter` for local
Claude Code transcript analytics:
https://github.com/handgemacht-ai/spotter-rust

The project goal currently requires `cargo install spotter`, but crates.io
already has your unrelated AWS EC2 Spot Instance Advisor CLI under that package
name. Would you be open to discussing a crates.io ownership transfer or another
arrangement? We do not want to overwrite or confuse users of your existing CLI
without your explicit agreement.
```

Suggested GitHub issue command, if the maintainer agrees this contact should be
made publicly from an authorized account:

```sh
gh issue create -R kohbis/spotter \
  --title "Question about the crates.io spotter package name" \
  --body-file docs/crates-io-owner-request.md
```

### Rename The Crate

This path avoids colliding with an existing package, but it changes the
explicit `GOAL.md` requirement.

1. Pick a new flat crates.io package name, for example `handgemacht-spotter`.
2. Update `Cargo.toml`, `CHANGELOG.md`, `README.md`, `GOAL.md`, release
   workflow checks, and install documentation.
3. Decide whether the installed binary should remain `spotter` while the
   package name changes, or match the new package name.
4. Re-run the full verification matrix and release dry run.

## Preflight Enforcement

`scripts/check-crates-io-release-ready.py` fails if:

- the package name exists and `CRATES_IO_OWNER_LOGIN` is unset, reporting the
  current owner logins,
- the configured owner does not match a crates.io owner,
- the package version already exists, or
- the manifest version is not newer than the existing crates.io max version.
