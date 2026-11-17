"""Recover the root website through its existing edge route, with guarded rollback."""
import argparse
import hashlib
import ipaddress
import json
import os
import re
import subprocess
from website_dns import call

ROOT = "wfsim.app"


def save(name, value):
    with open("website-proxy-" + name + ".json", "w") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())


def snapshot(plan):
    zone, account = plan["zone"], plan["account"]
    records = []
    for name in (ROOT, "www." + ROOT):
        records.extend(r for r in call(f"/zones/{zone}/dns_records?name={name}&per_page=100") if r["name"] == name)
    return {"records": sorted(records, key=lambda r: r["id"]),
            "routes": sorted(call(f"/zones/{zone}/workers/routes"), key=lambda r: r["id"]),
            "domains": sorted(call(f"/accounts/{account}/workers/domains"), key=lambda r: r["id"])}


def target(state, plan):
    matches = [r for r in state["records"] if r["id"] == plan["record"]["id"]]
    if len(matches) != 1:
        raise ValueError("reviewed record is missing or ambiguous")
    return matches[0]


def guard(state, plan):
    before = plan["record"]
    ipaddress.IPv4Address(before["content"])
    addresses = [r for r in state["records"] if r["name"] == ROOT and r["type"] in ("A", "AAAA", "CNAME")]
    if before["name"] != ROOT or before["type"] != "A" or before["proxied"] is not False:
        raise ValueError("plan must name the unproxied root A record")
    if addresses != [before] or target(state, plan) != before:
        raise ValueError("root DNS differs from the reviewed receipt")


def other_state(state, plan):
    return {**state, "records": sorted((r for r in state["records"] if r["id"] != plan["record"]["id"]), key=lambda r: r["id"]),
            "routes": sorted(state["routes"], key=lambda r: r["id"]), "domains": sorted(state["domains"], key=lambda r: r["id"])}


def same_record(actual, expected):
    return {k: v for k, v in actual.items() if k != "modified_on"} == {k: v for k, v in expected.items() if k != "modified_on"}


def fetch(path, address=None):
    args = ["curl", "--fail", "--silent", "--show-error", "--connect-timeout", "8", "--max-time", "30"]
    if address:
        args += ["--resolve", f"{ROOT}:443:{ipaddress.IPv4Address(address)}"]
    return subprocess.run(args + ["https://" + ROOT + path], capture_output=True, check=True).stdout


def verify(plan, pinned):
    expected = plan["identity"]
    address = plan["edge_address"] if pinned else None
    release = json.loads(fetch("/release.json", address))
    if any(release.get(k) != expected[k] for k in ("commit", "release", "engine", "wasm")):
        raise ValueError("website release differs from the reviewed identity")
    app = expected["app"]
    if not re.fullmatch(r"/asset/app\.[a-f0-9]{12,64}\.js", app) or app.encode() not in fetch("/", address):
        raise ValueError("website does not name the reviewed app")
    if hashlib.sha256(fetch(app, address)).hexdigest() != expected["app_sha256"]:
        raise ValueError("website app digest differs")
    if json.loads(fetch("/api/board/ping", address)).get("ok") is not True:
        raise ValueError("website API ping failed")


def rollback(plan, before):
    guard(before, plan)
    current = snapshot(plan)
    record = target(current, plan)
    original = target(before, plan)
    owned = {**original, "proxied": True, "ttl": 1}
    if same_record(record, original):
        save("rollback", {"plan": plan, "before": before, "after": current})
        return current
    if not same_record(record, owned):
        raise ValueError("root record changed independently; rollback requires review")
    call(f"/zones/{plan['zone']}/dns_records/{record['id']}", "PATCH", {"proxied": original["proxied"], "ttl": original["ttl"]})
    after = snapshot(plan)
    if not same_record(target(after, plan), original) or other_state(after, plan) != other_state(current, plan):
        raise ValueError("root rollback could not be verified")
    save("rollback", {"plan": plan, "before": before, "after": after})
    return after


def recover(plan, apply=False):
    before = snapshot(plan)
    guard(before, plan)
    verify(plan, True)
    save("before", {"plan": plan, "before": before})
    if not apply:
        return before
    # A fresh receipt guards the interval spent checking the edge payload.
    if snapshot(plan) != before:
        raise ValueError("routing changed during preflight")
    try:
        call(f"/zones/{plan['zone']}/dns_records/{plan['record']['id']}", "PATCH", {"proxied": True, "ttl": 1})
        after = snapshot(plan)
        if not same_record(target(after, plan), {**plan["record"], "proxied": True, "ttl": 1}) or other_state(after, plan) != other_state(before, plan):
            raise ValueError("recovery changed an unexpected routing field")
        verify(plan, False)
        save("after", {"plan": plan, "before": before, "after": after})
        return after
    except Exception:
        rollback(plan, before)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--rollback", action="store_true")
    args = parser.parse_args()
    if args.apply and args.rollback:
        parser.error("apply and rollback are separate operations")
    if args.rollback:
        receipt = json.loads(os.environ["WEBSITE_PROXY_RECEIPT"])
        rollback(receipt["plan"], receipt["before"])
    else:
        recover(json.loads(os.environ["WEBSITE_PROXY_PLAN"]), args.apply)
    print("root website proxy operation verified")


if __name__ == "__main__":
    main()
