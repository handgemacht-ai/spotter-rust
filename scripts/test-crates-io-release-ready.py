#!/usr/bin/env python3
import json
import subprocess
import sys
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts" / "check-crates-io-release-ready.py"


def main() -> int:
    run_case(
        "available package",
        {"/crates/spotter": (404, {})},
        {},
        0,
        "crates.io package name is available: spotter",
    )
    run_case(
        "duplicate version",
        {"/crates/spotter": crate_response(["0.1.5"])},
        {},
        1,
        "crates.io already has spotter 0.1.5",
    )
    run_case(
        "owner unset",
        {
            "/crates/spotter": crate_response(["0.0.9"]),
            "/crates/spotter/owners": owners_response(["kohbis"]),
        },
        {},
        1,
        "set CRATES_IO_OWNER_LOGIN",
    )
    run_case(
        "wrong owner",
        {
            "/crates/spotter": crate_response(["0.0.9"]),
            "/crates/spotter/owners": owners_response(["kohbis"]),
        },
        {"CRATES_IO_OWNER_LOGIN": "marot"},
        1,
        "owned by ['kohbis'], not marot",
    )
    run_case(
        "matching owner",
        {
            "/crates/spotter": crate_response(["0.0.9"]),
            "/crates/spotter/owners": owners_response(["marot"]),
        },
        {"CRATES_IO_OWNER_LOGIN": "marot"},
        0,
        "crates.io package owner is configured for spotter: marot",
    )
    run_case(
        "older than existing max",
        {"/crates/spotter": crate_response(["0.1.6", "0.0.9"])},
        {},
        1,
        "manifest version 0.1.5 is not newer than existing crates.io spotter 0.1.6",
    )
    print("crates.io release preflight tests passed")
    return 0


def run_case(
    label: str,
    routes: dict[str, tuple[int, dict]],
    env: dict[str, str],
    expected_returncode: int,
    expected_output: str,
) -> None:
    server = route_server(routes)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    base_url = f"http://127.0.0.1:{server.server_port}"
    try:
        process = subprocess.run(
            [sys.executable, str(CHECK)],
            cwd=ROOT,
            env={
                "CRATES_IO_API_BASE": base_url,
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


def crate_response(versions: list[str]) -> tuple[int, dict]:
    return 200, {"versions": [{"num": version} for version in versions]}


def owners_response(logins: list[str]) -> tuple[int, dict]:
    return 200, {"users": [{"login": login} for login in logins]}


if __name__ == "__main__":
    raise SystemExit(main())
