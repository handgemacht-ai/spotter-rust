#!/usr/bin/env python3
import json
import os
import stat
import subprocess
import sys
import tempfile
import textwrap
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts" / "check-release-pr-signoff.py"
COMMIT = "0123456789abcdef0123456789abcdef01234567"


def main() -> int:
    signed_body = checklist_body(checked=True)
    unsigned_body = checklist_body(checked=False)
    release_body = "Release v0.1.5\n\n" + signed_body

    run_case(
        "success",
        [
            {
                "number": 42,
                "title": "Release v0.1.5",
                "body": release_body,
                "state": "closed",
                "merged_at": "2026-05-16T12:00:00Z",
                "base": {"ref": "main"},
            }
        ],
        0,
        "Release PR #42 signs off",
    )
    run_case("no associated PR", [], 1, "no merged PR associated")
    run_case(
        "unchecked checklist",
        [
            {
                "number": 43,
                "title": "Release v0.1.5",
                "body": "Release v0.1.5\n\n" + unsigned_body,
                "state": "closed",
                "merged_at": "2026-05-16T12:00:00Z",
                "base": {"ref": "main"},
            }
        ],
        1,
        "missing checked checklist items",
    )
    run_case(
        "not release version",
        [
            {
                "number": 44,
                "title": "Implementation PR",
                "body": signed_body,
                "state": "closed",
                "merged_at": "2026-05-16T12:00:00Z",
                "base": {"ref": "main"},
            }
        ],
        1,
        "does not mention 0.1.5 or v0.1.5",
    )
    run_case(
        "not merged main",
        [
            {
                "number": 45,
                "title": "Release v0.1.5",
                "body": release_body,
                "state": "open",
                "merged_at": None,
                "base": {"ref": "main"},
            }
        ],
        1,
        "not a merged main PR",
    )

    print("release PR signoff verifier tests passed")
    return 0


def checklist_body(*, checked: bool) -> str:
    marker = "- [x] " if checked else "- [ ] "
    lines = []
    for line in (ROOT / ".github" / "PULL_REQUEST_TEMPLATE.md").read_text(
        encoding="utf-8"
    ).splitlines():
        if line.startswith("- [ ] "):
            lines.append(marker + line.removeprefix("- [ ] "))
    return "\n".join(lines)


def run_case(
    label: str, prs: list[dict], expected_returncode: int, expected_output: str
) -> None:
    with tempfile.TemporaryDirectory() as temp:
        root = Path(temp)
        bin_dir = root / "bin"
        bin_dir.mkdir()
        write_fake_gh(bin_dir)
        process = subprocess.run(
            [
                sys.executable,
                str(CHECK),
                "--repo",
                "handgemacht-ai/spotter-rust",
                "--commit",
                COMMIT,
                "--version",
                "0.1.5",
            ],
            cwd=ROOT,
            env={
                **os.environ,
                "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
                "FAKE_PRS": json.dumps(prs),
            },
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )

    if process.returncode != expected_returncode:
        raise SystemExit(
            f"{label}: expected exit {expected_returncode}, got {process.returncode}\n{process.stdout}"
        )
    if expected_output not in process.stdout:
        raise SystemExit(
            f"{label}: expected output containing {expected_output!r}\n{process.stdout}"
        )


def write_fake_gh(bin_dir: Path) -> None:
    path = bin_dir / "gh"
    path.write_text(
        f"#!{sys.executable}\n"
        + textwrap.dedent(
            """
            import os
            import sys

            args = sys.argv[1:]
            if args[:1] == ["api"]:
                print(os.environ["FAKE_PRS"])
                raise SystemExit(0)

            print(f"unexpected fake gh args: {args}", file=sys.stderr)
            raise SystemExit(2)
            """
        ).lstrip(),
        encoding="utf-8",
    )
    path.chmod(path.stat().st_mode | stat.S_IXUSR)


if __name__ == "__main__":
    raise SystemExit(main())
