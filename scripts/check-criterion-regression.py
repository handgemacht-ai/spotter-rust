#!/usr/bin/env python3
import argparse
import json
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("criterion_dir")
    parser.add_argument("--max-regression-percent", type=float, required=True)
    parser.add_argument("benchmarks", nargs="+")
    args = parser.parse_args()

    failed = False
    for benchmark in args.benchmarks:
        path = Path(args.criterion_dir) / benchmark / "change" / "estimates.json"
        try:
            with path.open("r", encoding="utf-8") as handle:
                report = json.load(handle)
            mean = report["mean"]
            change_pct = float(mean["point_estimate"]) * 100.0
            lower_pct = float(
                mean.get("confidence_interval", {}).get("lower_bound", mean["point_estimate"])
            ) * 100.0
            upper_pct = float(
                mean.get("confidence_interval", {}).get("upper_bound", mean["point_estimate"])
            ) * 100.0
        except (OSError, KeyError, TypeError, ValueError) as exc:
            print(f"{benchmark}: unable to read Criterion change estimate: {exc}", file=sys.stderr)
            return 2

        print(
            f"{benchmark}: mean change {change_pct:+.2f}% "
            f"(95% CI {lower_pct:+.2f}%..{upper_pct:+.2f}%, "
            f"maximum regression {args.max_regression_percent:.2f}%)"
        )
        if lower_pct > args.max_regression_percent:
            failed = True

    if failed:
        print("Criterion regression threshold exceeded", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
