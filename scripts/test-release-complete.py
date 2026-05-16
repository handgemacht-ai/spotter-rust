#!/usr/bin/env python3
import json
import os
import stat
import subprocess
import sys
import tempfile
import textwrap
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts" / "check-release-complete.py"
HEAD = "0123456789abcdef0123456789abcdef01234567"
EXPECTED_SUCCESS = "Release v0.1.5 is complete"


def main() -> int:
    run_case(
        "success",
        {"/crates/spotter/0.1.5": (200, {"version": {"num": "0.1.5"}})},
        {},
        0,
        EXPECTED_SUCCESS,
    )
    run_case(
        "missing tag",
        {"/crates/spotter/0.1.5": (200, {"version": {"num": "0.1.5"}})},
        {"FAKE_MISSING_TAG": "1"},
        1,
        "missing fetched release tag: v0.1.5",
    )
    run_case(
        "missing crates version",
        {"/crates/spotter/0.1.5": (404, {})},
        {},
        1,
        "crates.io does not have spotter 0.1.5",
    )
    run_case(
        "missing release asset",
        {"/crates/spotter/0.1.5": (200, {"version": {"num": "0.1.5"}})},
        {"FAKE_GH_MISSING_ASSET": "1"},
        1,
        "GitHub Release assets mismatch",
    )
    run_case(
        "prerelease",
        {"/crates/spotter/0.1.5": (200, {"version": {"num": "0.1.5"}})},
        {"FAKE_GH_PRERELEASE": "1"},
        1,
        "GitHub Release v0.1.5 is marked as a prerelease",
    )
    run_case(
        "installed version mismatch",
        {"/crates/spotter/0.1.5": (200, {"version": {"num": "0.1.5"}})},
        {"FAKE_CARGO_BAD_VERSION": "1"},
        1,
        "--version returned",
    )
    print("release completion verifier tests passed")
    return 0


def run_case(
    label: str,
    routes: dict[str, tuple[int, dict]],
    env: dict[str, str],
    expected_returncode: int,
    expected_output: str,
) -> None:
    with tempfile.TemporaryDirectory() as temp:
        root = Path(temp)
        bin_dir = root / "bin"
        bin_dir.mkdir()
        write_fake_commands(bin_dir)
        server = route_server(routes)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            process = subprocess.run(
                [
                    sys.executable,
                    str(CHECK),
                    "--repo",
                    "handgemacht-ai/spotter-rust",
                    "--release-dir",
                    str(root / "release-assets"),
                    "--install-root",
                    str(root / "install"),
                    "--skip-asset-version-check",
                    "--skip-runnable-host-asset-check",
                ],
                cwd=ROOT,
                env={
                    **os.environ,
                    "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
                    "CRATES_IO_API_BASE": f"http://127.0.0.1:{server.server_port}",
                    "SPOTTER_TEST_ROOT": str(ROOT),
                    "FAKE_HEAD": HEAD,
                    **env,
                },
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
        finally:
            server.shutdown()
            thread.join(timeout=5)

    if process.returncode != expected_returncode:
        raise SystemExit(
            f"{label}: expected exit {expected_returncode}, got {process.returncode}\n{process.stdout}"
        )
    if expected_output not in process.stdout:
        raise SystemExit(
            f"{label}: expected output containing {expected_output!r}\n{process.stdout}"
        )


def write_fake_commands(bin_dir: Path) -> None:
    write_executable(
        bin_dir / "git",
        """
        import os
        import sys

        args = sys.argv[1:]
        head = os.environ.get("FAKE_HEAD")
        main = os.environ.get("FAKE_MAIN", head)
        tag = os.environ.get("FAKE_TAG", head)

        if args[:4] == ["fetch", "origin", "main", "--tags"]:
            raise SystemExit(0)
        if args == ["rev-parse", "HEAD"]:
            print(head)
            raise SystemExit(0)
        if args == ["rev-parse", "origin/main"]:
            print(main)
            raise SystemExit(0)
        if args == ["rev-parse", "-q", "--verify", "refs/tags/v0.1.5^{commit}"]:
            if os.environ.get("FAKE_MISSING_TAG"):
                raise SystemExit(1)
            print(tag)
            raise SystemExit(0)
        if args == ["merge-base", "--is-ancestor", "v0.1.5", "origin/main"]:
            raise SystemExit(1 if os.environ.get("FAKE_TAG_OFF_MAIN") else 0)

        print(f"unexpected fake git args: {args}", file=sys.stderr)
        raise SystemExit(2)
        """,
    )
    write_executable(
        bin_dir / "gh",
        """
        import json
        import os
        import runpy
        import sys
        from pathlib import Path

        args = sys.argv[1:]
        root = Path(os.environ["SPOTTER_TEST_ROOT"])
        assets = [
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
        ]
        if os.environ.get("FAKE_GH_MISSING_ASSET"):
            assets = assets[:-1]

        if args[:2] == ["release", "view"]:
            print(json.dumps({
                "tagName": "v0.1.5",
                "isDraft": False,
                "isPrerelease": bool(os.environ.get("FAKE_GH_PRERELEASE")),
                "assets": [{"name": name} for name in assets],
            }))
            raise SystemExit(0)

        if args[:2] == ["release", "download"]:
            out_dir = Path(args[args.index("--dir") + 1])
            out_dir.mkdir(parents=True, exist_ok=True)
            namespace = runpy.run_path(str(root / "scripts" / "test-github-release-assets.py"))
            namespace["write_release_assets"](out_dir)
            raise SystemExit(0)

        print(f"unexpected fake gh args: {args}", file=sys.stderr)
        raise SystemExit(2)
        """,
    )
    write_executable(
        bin_dir / "cargo",
        """
        import os
        import stat
        import sys
        from pathlib import Path

        args = sys.argv[1:]
        if args[:2] != ["install", "spotter"]:
            print(f"unexpected fake cargo args: {args}", file=sys.stderr)
            raise SystemExit(2)

        root = Path(args[args.index("--root") + 1])
        binary = root / "bin" / "spotter"
        binary.parent.mkdir(parents=True, exist_ok=True)
        version = "0.0.0" if os.environ.get("FAKE_CARGO_BAD_VERSION") else "0.1.5"
        binary.write_text(f"#!/bin/sh\\necho spotter {version}\\n", encoding="utf-8")
        binary.chmod(binary.stat().st_mode | stat.S_IXUSR)
        raise SystemExit(0)
        """,
    )


def write_executable(path: Path, body: str) -> None:
    path.write_text(
        f"#!{sys.executable}\n{textwrap.dedent(body).lstrip()}",
        encoding="utf-8",
    )
    path.chmod(path.stat().st_mode | stat.S_IXUSR)


def route_server(routes: dict[str, tuple[int, dict]]) -> HTTPServer:
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            status, body = routes.get(self.path, (404, {}))
            data = json.dumps(body).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def log_message(self, *args: object) -> None:
            return

    return HTTPServer(("127.0.0.1", 0), Handler)


if __name__ == "__main__":
    raise SystemExit(main())
