# SPDX-License-Identifier: AGPL-3.0-or-later
"""Compare two embedded-data native builds on riven cards and fixed-seed work."""
import argparse
from collections import Counter
import ctypes as C
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent.parent


class Native:
    def __init__(self, path):
        self.lib = C.CDLL(str(path.resolve()))
        self.lib.wfsim_api.argtypes = [C.c_char_p, C.c_size_t, C.c_char_p, C.c_size_t, C.POINTER(C.c_size_t)]
        self.lib.wfsim_api.restype = C.c_void_p
        self.lib.wfsim_free.argtypes = [C.c_void_p, C.c_size_t]
        self.lib.wfsim_free.restype = None

    def api(self, path, body):
        path = path.encode()
        body = json.dumps(body, separators=(",", ":"), allow_nan=False).encode()
        length = C.c_size_t()
        ptr = self.lib.wfsim_api(path, len(path), body, len(body), C.byref(length))
        try:
            return C.string_at(ptr, length.value)
        finally:
            self.lib.wfsim_free(ptr, length.value)


def read(path):
    return json.loads((ROOT / path).read_text(encoding="utf-8"))


def board_case(weapon):
    row = next(r for r in read(f"site/board/{weapon}.json") if r.get("riven"))
    riven = row["riven"]
    rolls = riven["rolls"]
    bonuses = [{"id": stat, "roll": roll} for stat, roll in zip(riven["bonuses"], rolls)]
    malus = {"id": riven["malus"], "roll": rolls[len(bonuses)]} if riven.get("malus") else None
    record = {k: row[k] for k in ("mods", "arcanes", "evolutions", "valence", "exilus") if k in row}
    record.update(weapon=weapon, riven_pos=riven["bonuses"], riven_neg=riven.get("malus"), riven_rolls=rolls)
    return {"weapon": weapon, "bonuses": bonuses, "malus": malus, "rank": 8}, {
        "record": record, "ruler": row["benchmark"], "mode": row.get("mode") or "base",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("current", type=Path)
    args = parser.parse_args()
    baseline, current = Native(args.baseline), Native(args.current)
    digest, calls = hashlib.sha256(), Counter()

    def same(path, body):
        a, b = baseline.api(path, body), current.api(path, body)
        assert a == b, (path, body, a[:400], b[:400])
        digest.update(b)
        calls[path] += 1
        return json.loads(b)

    index = (ROOT / "site/index.html").read_text(encoding="utf-8")
    meta_file = re.search(r"meta\.[a-f0-9]+\.json", index)[0]
    meta = read(f"site/pkg/{meta_file}")
    known_weapon_cards = 0
    for w in meta["weapons"]:
        if w.get("no_riven"):
            assert not same("/api/riven", {"weapon": w["id"]})["ok"]
            continue
        pool = meta["riven_stats"].get(w["riven_class"], [])
        stats = [s["id"] for s in pool if s["bonus"] and not s["spliced"] and s["id"] not in w["riven_excludes"]]
        if len(stats) < 2:
            continue
        physical = next((s for s in w["riven_unconfirmed"] + w["riven_excludes"]
                         if s in ("impact", "puncture", "slash")), "impact")
        out = same("/api/riven", {"weapon": w["id"], "bonuses": [
            {"id": stats[0], "roll": 1.038}, {"id": stats[1], "roll": 1.064},
        ], "malus": {"id": physical, "value": -53}, "rank": 6})
        assert out["ok"]
        known_weapon_cards += 1

    for weapon in ("torid", "laetum", "furis", "magistar"):
        card, order = board_case(weapon)
        out = same("/api/riven", card)
        assert out["ok"] and not out["illegal"], out
        slots = card["bonuses"] + ([card["malus"]] if card["malus"] else [])
        assert [s["roll"] for s in out["stats"]] == [s["roll"] for s in slots]
        typed = [{"id": s["id"], "value": s["shown"]} for s in out["stats"]]
        assert same("/api/riven", {"weapon": weapon, "rank": 8, "positives": typed[:len(card["bonuses"])],
                                 "curse": typed[-1] if card["malus"] else None})["ok"]

        frozen = same("/api/board/order", order)
        assert frozen["ok"], frozen
        request = {**frozen["request"], "runs": 3, "duration": 4, "seed": 12648430}
        shards = same("/api/board/runs", {"request": request, "from": 0, "count": 3})["shards"]
        assert len(shards) == 3
        acc = same("/api/board/fold", {"request": request, "pieces": shards})["acc"]
        score = same("/api/board/score", {"request": request, "ruler": order["ruler"], "acc": acc})
        assert score["ok"] and score["work"] > 0 and score["counts"]["gas"] > 0, score

    req = read("tests/client_search.json")["request"]
    card = {"bonuses": [{"id": "damage", "roll": 0.94}, {"id": "critical_chance", "roll": 1.027}],
            "malus": {"id": "status_duration", "roll": 1.05}, "rank": 7}
    req["mods"]["riven:branch"] = "fixed"
    req.update(rivens=[{"id": "branch", "spec": card}], search_strategy="four_elements",
               starts=[{"slots": ["riven:branch", "split_chamber", "point_strike"],
                        "fixed": [{"kind": "mods", "idx": 0}]}])
    result = same("/api/compute/optimize", req)
    assert result["ok"] and result["results"], result
    assert "riven:branch" in result["results"][0]["mods"], result["results"][0]
    print(f"{dict(calls)}; {known_weapon_cards} roster cards; complete stable response SHA-256 {digest.hexdigest()}")


if __name__ == "__main__":
    main()
