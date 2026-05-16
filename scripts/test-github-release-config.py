#!/usr/bin/env python3
import os
import stat
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts" / "check-github-release-config.py"


def main() -> int:
    with tempfile.TemporaryDirectory() as directory:
        fake_bin = Path(directory)
        write_fake_gh(fake_bin / "gh")
        run_case(
            "configured",
            fake_bin,
            {"FAKE_GH_SECRETS": "CRATES_IO_TOKEN", "FAKE_GH_VARIABLES": "CRATES_IO_OWNER_LOGIN=kohbis"},
            [],
            0,
            "GitHub release config is present",
        )
        run_case(
            "missing secret",
            fake_bin,
            {"FAKE_GH_VARIABLES": "CRATES_IO_OWNER_LOGIN=kohbis"},
            [],
            1,
            "missing repository secret: CRATES_IO_TOKEN",
        )
        run_case(
            "missing variable",
            fake_bin,
            {"FAKE_GH_SECRETS": "CRATES_IO_TOKEN"},
            [],
            1,
            "missing repository variable: CRATES_IO_OWNER_LOGIN",
        )
        run_case(
            "wrong owner",
            fake_bin,
            {"FAKE_GH_SECRETS": "CRATES_IO_TOKEN", "FAKE_GH_VARIABLES": "CRATES_IO_OWNER_LOGIN=marot"},
            ["--expected-owner", "kohbis"],
            1,
            "repository variable CRATES_IO_OWNER_LOGIN is 'marot', expected 'kohbis'",
        )

    print("GitHub release config verifier tests passed")
    return 0


def run_case(
    label: str,
    fake_bin: Path,
    env: dict[str, str],
    args: list[str],
    expected_returncode: int,
    expected_output: str,
) -> None:
    process = subprocess.run(
        [sys.executable, str(CHECK), *args],
        cwd=ROOT,
        env={
            "PATH": f"{fake_bin}{os.pathsep}{os.environ['PATH']}",
            **env,
        },
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if process.returncode != expected_returncode:
        print(
            f"{label}: expected exit {expected_returncode}, got {process.returncode}\n{process.stdout}",
            file=sys.stderr,
        )
        raise SystemExit(1)
    if expected_output not in process.stdout:
        print(
            f"{label}: expected output containing {expected_output!r}\n{process.stdout}",
            file=sys.stderr,
        )
        raise SystemExit(1)


def write_fake_gh(path: Path) -> None:
    path.write_text(
        """#!/usr/bin/env python3
import json
import os
import sys

args = sys.argv[1:]
if args[:2] == ["secret", "list"]:
    names = [name for name in os.environ.get("FAKE_GH_SECRETS", "").split(",") if name]
    print(json.dumps([{"name": name} for name in names]))
elif args[:2] == ["variable", "list"]:
    variables = []
    for item in os.environ.get("FAKE_GH_VARIABLES", "").split(","):
        if not item:
            continue
        name, _, value = item.partition("=")
        variables.append({"name": name, "value": value})
    print(json.dumps(variables))
else:
    print(f"unexpected gh arguments: {args}", file=sys.stderr)
    raise SystemExit(2)
""",
        encoding="utf-8",
    )
    path.chmod(path.stat().st_mode | stat.S_IXUSR)


if __name__ == "__main__":
    raise SystemExit(main())
