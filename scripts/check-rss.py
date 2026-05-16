#!/usr/bin/env python3
import argparse
import re
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--max-kb", type=int, required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()

    command = args.command
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        parser.error("command is required after --")

    completed = subprocess.run(
        ["/usr/bin/time", "-v", *command],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    sys.stdout.write(completed.stdout)
    sys.stderr.write(completed.stderr)

    match = re.search(r"Maximum resident set size \(kbytes\):\s+(\d+)", completed.stderr)
    if not match:
        print("could not find maximum RSS in /usr/bin/time output", file=sys.stderr)
        return 2

    rss_kb = int(match.group(1))
    print(f"max RSS: {rss_kb}KB (maximum {args.max_kb}KB)")
    if completed.returncode != 0:
        return completed.returncode
    if rss_kb > args.max_kb:
        print("max RSS exceeded threshold", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
