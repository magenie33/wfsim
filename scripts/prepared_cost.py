# SPDX-License-Identifier: AGPL-3.0-or-later
"""Freeze preparation cases and summarize zero-transfer reuse measurements."""
import argparse
import json
from pathlib import Path
import statistics

from native_pgo import Native, ROOT


def inputs(args):
    calls = json.loads((ROOT / "tests/fixtures/simulation-cost.json").read_text())
    cases = [{"id": "api/" + c["id"], "request": c["body"]["request"]}
             for c in calls if c["path"] == "/api/board/fold"]
    engine = Native(args.library, args.pack)
    cases += [{"id": "board/" + r["id"], "request": r["request"]} for r in engine.rows()]
    dense = next(c for c in cases if c["id"] == "api/torid-25-fold")
    request = json.loads(json.dumps(dense["request"]))
    request["formation"] = [{"at": [x, y]} for x in range(-9, 10, 3) for y in range(1, 20, 3) if (x, y) != (0, 10)]
    request["formation"].append({"at": [12, 10]})
    cases.append({"id": "api/torid-50-fold", "request": request})
    crowd = json.loads(json.dumps(dense["request"]))
    crowd["formation"] = [{"at": [x, y]} for x in range(-27, 28, 3) for y in range(-17, 38, 3) if (x, y) != (0, 10)]
    cases.append({"id": "api/torid-361-fold", "request": crowd})
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(cases, indent=2) + "\n")
    print(f"{len(cases)} frozen preparation cases: {args.output}")


def summary(args):
    rows = json.loads(args.input.read_text())
    result = []
    for row in rows:
        for v in row["variants"]:
            current = statistics.median(v["current_ms"])
            retained = statistics.median(v["retained_ms"])
            result.append({"id": row["id"], "width": v["width"], "prepare_ms": row["prepare_ms"],
                           "request_bytes": row["request_bytes"], "current_ms": current, "retained_ms": retained,
                           "best_case_saving_pct": 100 * (1 - retained / current),
                           "prep_estimate_pct": 100 * ((40 + v["width"] - 1) // v["width"]) * row["prepare_ms"] / current,
                           "prepare_share_of_250ms_pct": 100 * row["prepare_ms"] / 250})
    groups = {str(w): {"median_best_case_saving_pct": statistics.median(r["best_case_saving_pct"] for r in result if r["width"] == w),
                      "median_prep_estimate_pct": statistics.median(r["prep_estimate_pct"] for r in result if r["width"] == w),
                      "range_best_case_saving_pct": [min(r["best_case_saving_pct"] for r in result if r["width"] == w),
                                                     max(r["best_case_saving_pct"] for r in result if r["width"] == w)]}
              for w in [1, 7, 40]}
    out = {"groups": groups, "rows": result, "scope": f"{args.runtime}; zero preparation, transfer and decoding for the retained variant; not an implemented upstream cache"}
    args.output.write_text(json.dumps(out, indent=2) + "\n")
    print(json.dumps(groups, indent=2))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="mode", required=True)
    i = sub.add_parser("inputs")
    i.add_argument("--library", type=Path, required=True)
    i.add_argument("--pack", type=Path, required=True)
    s = sub.add_parser("summary")
    s.add_argument("--input", type=Path, required=True)
    s.add_argument("--runtime", default="native optimized release, mimalloc, no PGO")
    for command in [i, s]:
        command.add_argument("--output", type=Path, required=True)
    args = p.parse_args()
    {"inputs": inputs, "summary": summary}[args.mode](args)


if __name__ == "__main__":
    main()
