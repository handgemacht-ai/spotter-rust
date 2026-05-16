#!/usr/bin/env python3
import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

import tomllib


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "Cargo.toml"
PR_TEMPLATE = ROOT / ".github" / "PULL_REQUEST_TEMPLATE.md"
DEFAULT_REPO = "handgemacht-ai/spotter-rust"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Verify the release commit has a merged PR with the best-practice checklist signed off."
    )
    parser.add_argument("--repo", default=DEFAULT_REPO)
    parser.add_argument("--commit", default=os.environ.get("GITHUB_SHA"))
    parser.add_argument("--version")
    args = parser.parse_args()

    version = args.version or tomllib.loads(MANIFEST.read_text(encoding="utf-8"))["package"][
        "version"
    ]
    commit = args.commit or run(["git", "rev-parse", "HEAD"])
    items = checklist_items()
    prs = associated_pull_requests(args.repo, commit)

    if not prs:
        abort(f"no merged PR associated with commit {commit}")

    evaluated = []
    for pr in prs:
        number = pr.get("number", "?")
        title = pr.get("title") or ""
        body = pr.get("body") or ""
        merged_at = pr.get("merged_at")
        base = ((pr.get("base") or {}).get("ref")) or ""

        if pr.get("state") != "closed" or not merged_at or base != "main":
            evaluated.append(f"#{number}: not a merged main PR")
            continue

        missing = missing_checked_items(body, items)
        mentions_version = (
            version in title
            or version in body
            or f"v{version}" in title
            or f"v{version}" in body
        )
        if not missing and mentions_version:
            print(
                f"Release PR #{number} signs off {len(items)} checklist items for v{version}"
            )
            return 0

        reasons = []
        if missing:
            reasons.append(
                "missing checked checklist items: " + ", ".join(missing[:3])
            )
            if len(missing) > 3:
                reasons.append(f"{len(missing) - 3} more unchecked items")
        if not mentions_version:
            reasons.append(f"does not mention {version} or v{version}")
        evaluated.append(f"#{number}: {'; '.join(reasons)}")

    abort(
        f"no merged release PR associated with commit {commit} has the full checklist signed off\n"
        + "\n".join(evaluated)
    )


def checklist_items() -> list[str]:
    items = []
    for line in PR_TEMPLATE.read_text(encoding="utf-8").splitlines():
        if line.startswith("- [ ] "):
            items.append(line.removeprefix("- [ ] ").strip())
    if not items:
        abort(f"no checklist items found in {PR_TEMPLATE}")
    return items


def missing_checked_items(body: str, items: list[str]) -> list[str]:
    return [
        item
        for item in items
        if not re.search(
            rf"^\s*-\s*\[[xX]\]\s+{re.escape(item)}\s*$", body, re.MULTILINE
        )
    ]


def associated_pull_requests(repo: str, commit: str) -> list[dict]:
    return json.loads(
        run(
            [
                "gh",
                "api",
                "-H",
                "Accept: application/vnd.github+json",
                f"repos/{repo}/commits/{commit}/pulls",
            ]
        )
    )


def run(command: list[str]) -> str:
    process = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if process.returncode != 0:
        abort(
            f"command failed ({process.returncode}): {' '.join(command)}\n"
            f"{process.stdout}{process.stderr}"
        )
    return process.stdout.strip()


def abort(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(1)


if __name__ == "__main__":
    raise SystemExit(main())
