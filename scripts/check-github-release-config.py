#!/usr/bin/env python3
import argparse
import json
import subprocess
import sys


DEFAULT_REPO = "handgemacht-ai/spotter-rust"
DEFAULT_SECRET = "CRATES_IO_TOKEN"
DEFAULT_OWNER_VARIABLE = "CRATES_IO_OWNER_LOGIN"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Verify GitHub repository release configuration is present."
    )
    parser.add_argument("--repo", default=DEFAULT_REPO)
    parser.add_argument("--secret-name", default=DEFAULT_SECRET)
    parser.add_argument("--owner-variable", default=DEFAULT_OWNER_VARIABLE)
    parser.add_argument("--expected-owner")
    args = parser.parse_args()

    secrets = gh_json(["gh", "secret", "list", "-R", args.repo, "--json", "name"])
    variables = gh_json(
        ["gh", "variable", "list", "-R", args.repo, "--json", "name,value"]
    )

    secret_names = {secret.get("name") for secret in secrets}
    variables_by_name = {
        variable.get("name"): str(variable.get("value", "")).strip()
        for variable in variables
    }

    failures = []
    if args.secret_name not in secret_names:
        failures.append(f"missing repository secret: {args.secret_name}")

    owner = variables_by_name.get(args.owner_variable, "")
    if not owner:
        failures.append(f"missing repository variable: {args.owner_variable}")
    elif args.expected_owner and owner != args.expected_owner:
        failures.append(
            f"repository variable {args.owner_variable} is {owner!r}, expected {args.expected_owner!r}"
        )

    if failures:
        for failure in failures:
            print(failure, file=sys.stderr)
        return 1

    print(
        f"GitHub release config is present: {args.secret_name} secret, "
        f"{args.owner_variable}={owner} variable"
    )
    return 0


def gh_json(command: list[str]) -> list[dict]:
    process = subprocess.run(
        command,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if process.returncode != 0:
        print(
            f"command failed ({process.returncode}): {' '.join(command)}\n"
            f"{process.stdout}{process.stderr}",
            file=sys.stderr,
        )
        raise SystemExit(1)
    try:
        payload = json.loads(process.stdout)
    except json.JSONDecodeError as error:
        print(f"failed to parse gh JSON output: {error}", file=sys.stderr)
        raise SystemExit(1) from error
    if not isinstance(payload, list):
        print("gh JSON output was not a list", file=sys.stderr)
        raise SystemExit(1)
    return payload


if __name__ == "__main__":
    raise SystemExit(main())
