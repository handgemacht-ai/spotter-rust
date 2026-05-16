#!/usr/bin/env python3
import argparse
import json
import sys


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("hyperfine_json")
    parser.add_argument("--max-ms", type=float, required=True)
    parser.add_argument("--name", default="benchmark")
    args = parser.parse_args()

    with open(args.hyperfine_json, "r", encoding="utf-8") as handle:
        report = json.load(handle)

    try:
        mean_seconds = float(report["results"][0]["mean"])
    except (KeyError, IndexError, TypeError, ValueError) as exc:
        print(f"hyperfine JSON is missing result mean: {exc}", file=sys.stderr)
        return 2

    mean_ms = mean_seconds * 1000.0
    print(f"{args.name}: {mean_ms:.2f}ms mean (maximum {args.max_ms:.2f}ms)")
    if mean_ms > args.max_ms:
        print(f"{args.name} exceeded threshold", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
