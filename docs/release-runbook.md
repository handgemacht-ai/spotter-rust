# Release Runbook

Use this runbook after the implementation audit in
`docs/release-readiness-audit.md` is green except for external release state.

## Prerequisites

- The release commit is on `origin/main`.
- `CHANGELOG.md` has an entry matching the package version in `Cargo.toml`.
- The Rust best-practice checklist in `.github/PULL_REQUEST_TEMPLATE.md` is
  signed off for the release.
- The crates.io package decision in `docs/crates-io-name-decision.md` is
  resolved:
  - either `spotter` is owned by the release account or team, or
  - `Cargo.toml` has been intentionally renamed and the CHANGELOG/README
    install instructions have been updated.
  If keeping the `spotter` package name, use
  `docs/crates-io-owner-request.md` as the maintainer-approved public contact
  body; do not open a third-party issue from an account that is not authorized
  to represent the release decision.
- The GitHub repository has:
  - `CRATES_IO_TOKEN` as a repository secret,
  - `CRATES_IO_OWNER_LOGIN` as a repository variable matching a crates.io
    owner login when publishing an existing crate name.

## Preflight

Run these from a clean checkout of `main`:

```sh
git checkout main
git pull --ff-only origin main
git status --short --branch

scripts/check-crates-io-release-ready.py
cargo package --locked

gh workflow run release.yml \
  -R handgemacht-ai/spotter-rust \
  --ref main \
  -f publish=false
```

Watch the dry-run release workflow and require `verify` plus all five `build`
matrix jobs to pass. The `release-preflight` and `publish` jobs should be
skipped for `publish=false`.

## Publish

Only publish after preflight passes:

```sh
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
tag="v${version}"

if git rev-parse -q --verify "refs/tags/${tag}" >/dev/null; then
  test "$(git rev-list -n 1 "$tag")" = "$(git rev-parse HEAD)"
else
  git tag "$tag"
fi
git push origin "$tag"
```

The tag push runs `.github/workflows/release.yml` with publishing enabled. The
workflow must publish to crates.io before creating the GitHub Release.

## Verify

```sh
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
tag="v${version}"

gh run list -R handgemacht-ai/spotter-rust --workflow Release --limit 5
gh release view "$tag" -R handgemacht-ai/spotter-rust --json tagName,assets
scripts/check-release-complete.py
```

For asset-only debugging, download the release assets and verify each checksum
file against its binary:

```sh
rm -rf target/release-verify
mkdir -p target/release-verify
gh release download "$tag" \
  -R handgemacht-ai/spotter-rust \
  --dir target/release-verify
scripts/check-github-release-assets.py \
  target/release-verify \
  --expect-version "$version" \
  --require-runnable-host
```

The release satisfies `GOAL.md` only after crates.io contains the published
version and the GitHub Release contains all five binaries plus checksums:

- `spotter-x86_64-unknown-linux-gnu`
- `spotter-aarch64-unknown-linux-gnu`
- `spotter-x86_64-apple-darwin`
- `spotter-aarch64-apple-darwin`
- `spotter-x86_64-pc-windows-msvc.exe`
