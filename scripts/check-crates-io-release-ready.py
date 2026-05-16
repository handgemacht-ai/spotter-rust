#!/usr/bin/env python3
import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path

import tomllib


MANIFEST = Path("Cargo.toml")
CRATES_IO_API_BASE = os.environ.get("CRATES_IO_API_BASE", "https://crates.io/api/v1")
EXPECTED_OWNER = os.environ.get("CRATES_IO_OWNER_LOGIN", "").strip()
USER_AGENT = "spotter-rust-release-check (https://github.com/handgemacht-ai/spotter-rust)"


def main() -> int:
    package = tomllib.loads(MANIFEST.read_text(encoding="utf-8"))["package"]
    name = package["name"]
    version = package["version"]

    crate = fetch_json(f"{CRATES_IO_API_BASE}/crates/{name}", allow_not_found=True)
    if crate is None:
        print(f"crates.io package name is available: {name}")
        return 0

    versions = crate.get("versions", [])
    existing_versions = {item.get("num") for item in versions if item.get("num")}
    if version in existing_versions:
        print(
            f"crates.io already has {name} {version}; choose a new package name or version",
            file=sys.stderr,
        )
        return 1

    if existing_versions:
        newest_existing = max(existing_versions, key=semver_key)
        if semver_key(version) <= semver_key(newest_existing):
            print(
                f"manifest version {version} is not newer than existing crates.io {name} {newest_existing}; "
                f"bump the version after ownership or choose a new package name",
                file=sys.stderr,
            )
            return 1

    owners = fetch_json(f"{CRATES_IO_API_BASE}/crates/{name}/owners")["users"]
    owner_logins = {owner.get("login") for owner in owners}
    if not EXPECTED_OWNER:
        print(
            f"crates.io package name already exists: {name}; set CRATES_IO_OWNER_LOGIN after ownership is ready",
            file=sys.stderr,
        )
        return 1

    if EXPECTED_OWNER not in owner_logins:
        print(
            f"crates.io package {name} is owned by {sorted(owner_logins)}, not {EXPECTED_OWNER}",
            file=sys.stderr,
        )
        return 1

    print(f"crates.io package owner is configured for {name}: {EXPECTED_OWNER}")
    return 0


def fetch_json(url: str, *, allow_not_found: bool = False) -> dict | None:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        if allow_not_found and error.code == 404:
            return None
        print(f"failed to query crates.io: {url}: HTTP {error.code}", file=sys.stderr)
        return abort()
    except urllib.error.URLError as error:
        print(f"failed to query crates.io: {url}: {error.reason}", file=sys.stderr)
        return abort()


def abort() -> None:
    raise SystemExit(1)


def semver_key(version: str) -> tuple:
    public = version.split("+", maxsplit=1)[0]
    core, separator, prerelease = public.partition("-")
    try:
        parts = tuple(int(part) for part in core.split("."))
    except ValueError:
        print(f"invalid semver version: {version}", file=sys.stderr)
        return abort()
    if len(parts) != 3:
        print(f"invalid semver version: {version}", file=sys.stderr)
        return abort()
    prerelease_key = (
        (1, ())
        if not separator
        else (0, tuple(prerelease_part_key(part) for part in prerelease.split(".")))
    )
    return (*parts, prerelease_key)


def prerelease_part_key(part: str) -> tuple[int, int | str]:
    if part.isdigit():
        return (0, int(part))
    return (1, part)


if __name__ == "__main__":
    raise SystemExit(main())
