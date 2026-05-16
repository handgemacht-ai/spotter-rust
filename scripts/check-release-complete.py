#!/usr/bin/env python3
import argparse
import json
import os
import shutil
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path

import tomllib


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "Cargo.toml"
CHANGELOG = ROOT / "CHANGELOG.md"
ASSET_CHECK = ROOT / "scripts" / "check-github-release-assets.py"
DEFAULT_REPO = "handgemacht-ai/spotter-rust"
CRATES_IO_API_BASE = os.environ.get("CRATES_IO_API_BASE", "https://crates.io/api/v1")
USER_AGENT = "spotter-rust-release-check (https://github.com/handgemacht-ai/spotter-rust)"

EXPECTED_ASSETS = {
    "spotter-x86_64-unknown-linux-gnu",
    "spotter-x86_64-unknown-linux-gnu.sha256",
    "spotter-aarch64-unknown-linux-gnu",
    "spotter-aarch64-unknown-linux-gnu.sha256",
    "spotter-x86_64-apple-darwin",
    "spotter-x86_64-apple-darwin.sha256",
    "spotter-aarch64-apple-darwin",
    "spotter-aarch64-apple-darwin.sha256",
    "spotter-x86_64-pc-windows-msvc.exe",
    "spotter-x86_64-pc-windows-msvc.sha256",
}


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Verify the published spotter release satisfies GOAL.md release requirements."
    )
    parser.add_argument("--repo", default=DEFAULT_REPO)
    parser.add_argument("--release-dir", type=Path, default=ROOT / "target" / "release-verify")
    parser.add_argument(
        "--install-root",
        type=Path,
        default=ROOT / "target" / "release-install-smoke",
    )
    parser.add_argument("--skip-cargo-install", action="store_true")
    parser.add_argument("--skip-asset-version-check", action="store_true")
    parser.add_argument("--skip-runnable-host-asset-check", action="store_true")
    args = parser.parse_args()

    package = tomllib.loads(MANIFEST.read_text(encoding="utf-8"))["package"]
    name = package["name"]
    version = package["version"]
    tag = f"v{version}"

    require_changelog_entry(version)
    verify_git_tag_on_main(tag)
    verify_crates_io_version(name, version)
    verify_github_release(
        args.repo,
        tag,
        version,
        args.release_dir,
        check_asset_version=not args.skip_asset_version_check,
        require_runnable_host=not args.skip_runnable_host_asset_check,
    )
    if not args.skip_cargo_install:
        verify_cargo_install(name, version, args.install_root)

    print(
        f"Release {tag} is complete: crates.io, git tag, GitHub Release assets, and cargo install verified"
    )
    return 0


def require_changelog_entry(version: str) -> None:
    heading = f"## {version}"
    if heading not in CHANGELOG.read_text(encoding="utf-8").splitlines():
        abort(f"CHANGELOG.md is missing release heading: {heading}")


def verify_git_tag_on_main(tag: str) -> None:
    run(["git", "fetch", "origin", "main", "--tags"])
    head = run(["git", "rev-parse", "HEAD"])
    origin_main = run(["git", "rev-parse", "origin/main"])
    tag_result = run_result(["git", "rev-parse", "-q", "--verify", f"refs/tags/{tag}^{{commit}}"])
    if tag_result.returncode != 0:
        abort(f"missing fetched release tag: {tag}")
    tag_commit = tag_result.stdout.strip()
    if head != tag_commit:
        abort(f"current HEAD {head} does not match {tag} target {tag_commit}")
    ancestor = run_result(["git", "merge-base", "--is-ancestor", tag, "origin/main"])
    if ancestor.returncode != 0:
        abort(f"release tag {tag} is not on origin/main {origin_main}")


def verify_crates_io_version(name: str, version: str) -> None:
    payload = fetch_json(f"{CRATES_IO_API_BASE}/crates/{name}/{version}", allow_not_found=True)
    if payload is None:
        abort(f"crates.io does not have {name} {version}")
    published = payload.get("version", {}).get("num")
    if published != version:
        abort(f"crates.io returned {name} {published!r}, expected {version!r}")


def verify_github_release(
    repo: str,
    tag: str,
    version: str,
    release_dir: Path,
    *,
    check_asset_version: bool,
    require_runnable_host: bool,
) -> None:
    payload = json.loads(
        run(["gh", "release", "view", tag, "-R", repo, "--json", "tagName,isDraft,isPrerelease,assets"])
    )
    if payload.get("tagName") != tag:
        abort(f"GitHub Release tag mismatch: expected {tag}, got {payload.get('tagName')!r}")
    if payload.get("isDraft"):
        abort(f"GitHub Release {tag} is still a draft")

    actual_assets = {asset.get("name") for asset in payload.get("assets", [])}
    if actual_assets != EXPECTED_ASSETS:
        missing = sorted(EXPECTED_ASSETS - actual_assets)
        extra = sorted(actual_assets - EXPECTED_ASSETS)
        abort(f"GitHub Release assets mismatch; missing={missing} extra={extra}")

    shutil.rmtree(release_dir, ignore_errors=True)
    release_dir.mkdir(parents=True, exist_ok=True)
    run(["gh", "release", "download", tag, "-R", repo, "--dir", str(release_dir)])
    command = [sys.executable, str(ASSET_CHECK), str(release_dir)]
    if check_asset_version:
        command.extend(["--expect-version", version])
    if check_asset_version and require_runnable_host:
        command.append("--require-runnable-host")
    run(command)


def verify_cargo_install(name: str, version: str, install_root: Path) -> None:
    shutil.rmtree(install_root, ignore_errors=True)
    run(
        [
            "cargo",
            "install",
            name,
            "--version",
            version,
            "--locked",
            "--root",
            str(install_root),
            "--force",
        ]
    )
    binary = install_root / "bin" / (f"{name}.exe" if os.name == "nt" else name)
    output = run([str(binary), "--version"])
    expected = f"{name} {version}"
    if output != expected:
        abort(f"{binary} --version returned {output!r}, expected {expected!r}")


def fetch_json(url: str, *, allow_not_found: bool = False) -> dict | None:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        if allow_not_found and error.code == 404:
            return None
        abort(f"failed to query crates.io: {url}: HTTP {error.code}")
    except urllib.error.URLError as error:
        abort(f"failed to query crates.io: {url}: {error.reason}")


def run(command: list[str]) -> str:
    process = run_result(command)
    if process.returncode != 0:
        abort(
            f"command failed ({process.returncode}): {' '.join(command)}\n"
            f"{process.stdout}{process.stderr}"
        )
    return process.stdout.strip()


def run_result(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def abort(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(1)


if __name__ == "__main__":
    raise SystemExit(main())
