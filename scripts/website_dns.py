#!/usr/bin/env python3
"""Check DNS write access or move the validated website, with rollback."""
import argparse
import ipaddress
import json
import os
from pathlib import Path
import subprocess
import uuid
from ship_website import backend_release

API = "https://api.cloudflare.com/client/v4"
DOMAINS = ("wfsim.app", "www.wfsim.app")


def call(path, method="GET", data=None):
    config = '\n'.join(['url = ' + json.dumps(API + path), 'request = ' + json.dumps(method),
        'header = ' + json.dumps('Authorization: Bearer ' + os.environ['CLOUDFLARE_API_TOKEN']),
        'header = "Content-Type: application/json"']) + '\n'
    if data is not None: config += 'data = ' + json.dumps(json.dumps(data)) + '\n'
    response = subprocess.run(["curl", "--fail-with-body", "--silent", "--show-error", "--max-time", "40",
                               "--write-out", "\n%{http_code}", "--config", "-"],
                              input=config, capture_output=True, text=True, check=True)
    raw, status = response.stdout.rsplit("\n", 1)
    if not 200 <= int(status) < 300:
        raise RuntimeError("Cloudflare operation failed: " + path)
    if method == "DELETE" and not raw.strip():
        return None
    body = json.loads(raw)
    if not body.get("success"):
        raise RuntimeError("Cloudflare operation failed: " + path)
    return body.get("result")


def dns_write_probe(zone):
    path = f"/zones/{zone}/dns_records"
    record = call(path, "POST", {"type": "TXT", "name": "_website-migration-check.wfsim.app",
        "content": "wfsim-check-" + uuid.uuid4().hex, "ttl": 60})
    call(path + "/" + record["id"], "DELETE")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    zone_data = call("/zones?name=wfsim.app")[0]
    zone = zone_data["id"]
    account = zone_data["account"]["id"]
    records_path = f"/zones/{zone}/dns_records"
    worker_path = f"/accounts/{account}/workers/domains"
    records = [r for r in call(records_path + "?per_page=100") if r["name"] in DOMAINS]
    workers = [r for r in call(worker_path) if r["hostname"] in DOMAINS]
    backup = {"zone": zone, "account": account, "records": records, "worker_domains": workers}
    Path("website-dns-before.json").write_text(json.dumps(backup, indent=2) + "\n")
    dns_write_probe(zone)
    print("website DNS read/write verified")
    if not args.apply:
        return
    target = json.loads(os.environ["WEBSITE_TARGET"])
    address = str(ipaddress.IPv4Address(target["host"].split("@", 1)[-1]))
    result = subprocess.run(["curl", "--fail", "--silent", "--show-error", "--max-time", "30",
        "--resolve", f"wfsim.app:443:{address}", "https://wfsim.app/release.json"],
        capture_output=True, text=True, check=True)
    website = json.loads(result.stdout)
    backend = backend_release()
    if any(website.get(k) != backend.get(k) for k in ("release", "engine", "wasm")):
        raise RuntimeError("website and backend releases differ; DNS unchanged")
    if not workers:
        if all(any(r["name"] == domain and r["type"] == "A" and r["content"] == address
                   and not r["proxied"] for r in records) for domain in DOMAINS):
            print("website DNS already points directly to this host")
            return
        raise RuntimeError("unexpected existing website routing; DNS unchanged")
    if {w["hostname"] for w in workers} != set(DOMAINS) or any(w["service"] != "wfsim-edge" for w in workers):
        raise RuntimeError("unexpected Worker mapping; DNS unchanged")
    created = []
    removed = []
    try:
        for worker in workers:
            removed.append(worker)
            call(worker_path + "/" + worker["id"], "DELETE")
        for domain in DOMAINS:
            existing = [r for r in call(records_path + "?name=" + domain) if r["name"] == domain]
            for record in existing:
                if record["type"] in ("A", "AAAA", "CNAME"):
                    call(records_path + "/" + record["id"], "DELETE")
            created.append(call(records_path, "POST", {"type": "A", "name": domain,
                "content": address, "proxied": False, "ttl": 300}))
        Path("website-dns-after.json").write_text(json.dumps(created, indent=2) + "\n")
    except Exception as original:
        failures = []
        for record in created:
            try: call(records_path + "/" + record["id"], "DELETE")
            except Exception: failures.append("remove new " + record["name"])
        for worker in removed:
            try:
                current = call(worker_path)
                if not any(w["hostname"] == worker["hostname"] and w["service"] == worker["service"]
                           for w in current):
                    call(worker_path, "PUT", {k: worker[k] for k in ("hostname", "service", "environment", "zone_id")})
            except Exception: failures.append("restore " + worker["hostname"])
        if failures:
            raise RuntimeError("DNS rollback incomplete: " + ", ".join(failures)) from original
        raise
    print("website DNS changed after TLS and release checks; previous mapping saved")


if __name__ == "__main__":
    main()
