#!/usr/bin/env python3
import argparse
import json
import sys


def metric_percent(totals, name):
    metric = totals.get(name)
    if not isinstance(metric, dict):
        raise KeyError(name)
    if "percent" in metric:
        return float(metric["percent"])
    count = float(metric.get("count", 0))
    covered = float(metric.get("covered", 0))
    if count == 0:
        return 100.0
    return covered / count * 100.0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("coverage_json")
    parser.add_argument("--lines", type=float, required=True)
    parser.add_argument("--branches", type=float, required=True)
    args = parser.parse_args()

    with open(args.coverage_json, "r", encoding="utf-8") as handle:
        report = json.load(handle)

    try:
        totals = report["data"][0]["totals"]
        line_pct = metric_percent(totals, "lines")
        branch_pct = metric_percent(totals, "branches")
    except (KeyError, IndexError, TypeError, ValueError) as exc:
        print(f"coverage JSON is missing required totals: {exc}", file=sys.stderr)
        return 2

    print(f"line coverage: {line_pct:.2f}% (minimum {args.lines:.2f}%)")
    print(f"branch coverage: {branch_pct:.2f}% (minimum {args.branches:.2f}%)")

    failed = False
    if line_pct < args.lines:
        print("line coverage is below threshold", file=sys.stderr)
        failed = True
    if branch_pct < args.branches:
        print("branch coverage is below threshold", file=sys.stderr)
        failed = True

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
