"""DNS permission failures preserve the site; partial cutovers restore it."""
import contextlib
import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("website_dns", os.environ.get("WFSIM_WEBSITE_DNS_MODULE")
    or Path(__file__).with_name("website_dns.py"))
dns = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dns)
release = {"release": "012345abcdef", "engine": "engine", "wasm": "wasm"}


def exercise(permit, fail_second=False, fail_delete=False):
    workers = [{"hostname": h, "id": str(i), "zone_id": "zone", "service": "wfsim-edge",
                "environment": "production"} for i, h in enumerate(dns.DOMAINS)]
    records = []
    removed = []

    def call(path, method="GET", data=None):
        if path == "/zones?name=wfsim.app":
            return [{"id": "zone", "account": {"id": "account"}}]
        if path.endswith("/workers/domains"):
            if method == "GET": return list(workers)
            if method == "PUT": workers.append({**data, "id": data["hostname"]}); return {}
        if "/workers/domains/" in path:
            record = next(w for w in workers if path.endswith("/" + w["id"]))
            workers.remove(record)
            removed.append(record["hostname"])
            if fail_delete: raise ValueError("response lost after domain detached")
            return {}
        if "/dns_records" in path:
            if method == "GET": return list(records)
            if method == "POST":
                if not permit: raise PermissionError("DNS edit unavailable")
                if fail_second and data.get("name") == "www.wfsim.app": raise OSError("second create failed")
                record = {**data, "id": str(len(records))}
                records.append(record)
                return record
            if method == "DELETE":
                record = next(r for r in records if path.endswith("/" + r["id"]))
                records.remove(record)
                return {}
        raise AssertionError((path, method))

    with tempfile.TemporaryDirectory() as directory:
        old = Path.cwd()
        os.chdir(directory)
        try:
            with patch.object(dns, "call", call), patch.dict(os.environ, {
                "WEBSITE_TARGET": '{"host":"root@203.0.113.2"}'}), \
                patch.object(sys, "argv", ["website_dns.py", "--apply"]), \
                patch.object(dns.subprocess, "run", return_value=SimpleNamespace(stdout=json.dumps(release))), \
                patch.object(dns, "backend_release", return_value=release), \
                contextlib.redirect_stdout(io.StringIO()):
                try: dns.main()
                except (PermissionError, OSError, ValueError): failed = True
                else: failed = False
            if not permit:
                assert failed and not removed and not records
            elif fail_second or fail_delete:
                assert failed and not records
                assert {w["hostname"] for w in workers} == set(dns.DOMAINS)
            else:
                assert not failed and not workers
                assert {r["name"] for r in records} == set(dns.DOMAINS)
                assert all(r["type"] == "A" and not r["proxied"] for r in records)
        finally:
            os.chdir(old)


def transport(method, raw, status):
    def run(args, **kwargs):
        output = raw + "\n" + str(status) if "--write-out" in args else raw
        return SimpleNamespace(stdout=output)

    with patch.dict(os.environ, {"CLOUDFLARE_API_TOKEN": "test"}), patch.object(dns.subprocess, "run", run):
        return dns.call("/accounts/account/workers/domains/domain", method)


assert transport("DELETE", "", 204) is None
assert transport("DELETE", "", 200) is None
assert transport("GET", '{"success":true,"result":[1]}', 200) == [1]
for method, raw, status in [("DELETE", "", 403), ("DELETE", "", 302),
                            ("POST", "", 200), ("GET", '{"success":false}', 200)]:
    try: transport(method, raw, status)
    except (ValueError, RuntimeError): pass
    else: raise AssertionError((method, raw, status))

exercise(False)
exercise(True, True)
exercise(True, fail_delete=True)
exercise(True)
print("website DNS: empty successful deletes accepted; denied edits and lost responses restore routing; validated switch succeeds")

proxy_spec = importlib.util.spec_from_file_location("website_proxy", os.environ.get("WFSIM_WEBSITE_PROXY_MODULE")
    or Path(__file__).with_name("website_proxy.py"))
proxy = importlib.util.module_from_spec(proxy_spec)
proxy_spec.loader.exec_module(proxy)


def proxy_case(mode):
    record = {"id": "root", "name": "wfsim.app", "type": "A", "content": "203.0.113.2",
              "proxied": False, "ttl": 600, "comment": "keep", "modified_on": "before"}
    www = {**record, "id": "www", "name": "www.wfsim.app"}
    mx = {"id": "mail", "name": "wfsim.app", "type": "MX", "content": "mail.example", "ttl": 300}
    state = {"records": [record, www, mx], "routes": [{"id": "route", "pattern": "wfsim.app/*", "script": "wfsim-edge"}], "domains": []}
    original = copy.deepcopy(state)
    plan = {"zone": "zone", "account": "account", "record": copy.deepcopy(record)}
    writes, receipts = [], {}
    if mode == "stale-id": plan["record"]["id"] = "another"
    if mode == "stale-origin": plan["record"]["content"] = "203.0.113.3"
    if mode == "already-proxied": plan["record"]["proxied"] = True
    if mode == "extra-address": state["records"].append({**record, "id": "ipv6", "type": "AAAA", "content": "2001:db8::1"})

    def call(path, method="GET", data=None):
        if method == "GET":
            if "/dns_records?name=" in path:
                name = path.split("name=", 1)[1].split("&", 1)[0]
                return copy.deepcopy([r for r in state["records"] if r["name"] == name])
            if path.endswith("/workers/routes"): return copy.deepcopy(state["routes"])
            if path.endswith("/workers/domains"): return copy.deepcopy(state["domains"])
        assert method == "PATCH" and path == "/zones/zone/dns_records/root", (method, path)
        assert set(data) == {"proxied", "ttl"}, data
        writes.append(copy.deepcopy(data))
        if mode == "denied": raise PermissionError("edit denied")
        record.update(data)
        record["modified_on"] = "after"
        if len(writes) == 1:
            if mode == "lost-response": raise OSError("reply lost after patch")
            if mode == "www-drift": www["content"] = "203.0.113.8"
            if mode == "route-drift": state["routes"][0]["script"] = "changed-independently"
            if mode == "domain-drift": state["domains"].append({"id": "new-domain", "hostname": "another.example"})
            if mode == "root-drift": record["content"] = "203.0.113.9"
        return copy.deepcopy(record)

    def verify(_plan, pinned):
        if mode == "preflight-drift" and pinned:
            record.update({"proxied": True, "ttl": 1})
        if mode == "bad-preflight" or mode == "bad-health" and not pinned:
            raise ValueError("identity mismatch")

    with patch.object(proxy, "call", call), patch.object(proxy, "verify", verify), \
            patch.object(proxy, "save", lambda name, data: receipts.update({name: copy.deepcopy(data)})):
        try: proxy.recover(plan, mode != "plan")
        except (ValueError, OSError, PermissionError): failed = True
        else: failed = False
        if mode in ("plan", "success", "manual-rollback"):
            assert not failed, mode
            assert writes == ([] if mode == "plan" else [{"proxied": True, "ttl": 1}]), (mode, writes)
            assert proxy.other_state(state, plan) == proxy.other_state(original, plan)
            if mode == "manual-rollback":
                receipt = receipts["after"]
                proxy.rollback(receipt["plan"], receipt["before"])
                assert record["proxied"] is False and record["ttl"] == 600
        elif mode in ("stale-id", "stale-origin", "already-proxied", "extra-address", "bad-preflight", "preflight-drift"):
            assert failed and not writes, (mode, failed, writes)
            if mode == "preflight-drift": assert record["proxied"] is True
        elif mode == "denied":
            assert failed and len(writes) == 1 and state == original
        elif mode == "root-drift":
            assert failed and len(writes) == 1 and record["content"] == "203.0.113.9"
        else:
            assert failed and writes == [{"proxied": True, "ttl": 1}, {"proxied": False, "ttl": 600}], (mode, failed, writes)
            assert proxy.same_record(record, original["records"][0])
            if mode == "www-drift": assert www["content"] == "203.0.113.8"
            elif mode == "route-drift": assert state["routes"][0]["script"] == "changed-independently"
            elif mode == "domain-drift": assert state["domains"][0]["id"] == "new-domain"
            else: assert proxy.other_state(state, plan) == proxy.other_state(original, plan)


for proxy_mode in ("plan", "success", "manual-rollback", "stale-id", "stale-origin", "already-proxied", "extra-address",
                   "bad-preflight", "preflight-drift", "denied", "lost-response", "bad-health", "www-drift", "route-drift", "domain-drift", "root-drift"):
    proxy_case(proxy_mode)

app_bytes = b"reviewed javascript"
identity = {"commit": "source", "release": "release", "engine": "engine", "wasm": "wasm",
            "app": "/asset/app.012345abcdef.js", "app_sha256": hashlib.sha256(app_bytes).hexdigest()}
identity_plan = {"identity": identity, "edge_address": "203.0.113.5"}
for broken in (None, "commit", "release", "engine", "wasm", "html", "app", "ping"):
    def fetch(path, address):
        assert address == "203.0.113.5"
        if path == "/release.json":
            return json.dumps({k: "wrong" if broken == k else identity[k] for k in ("commit", "release", "engine", "wasm")}).encode()
        if path == "/": return b"wrong page" if broken == "html" else identity["app"].encode()
        if path == identity["app"]: return b"wrong bytes" if broken == "app" else app_bytes
        if path == "/api/board/ping": return json.dumps({"ok": broken != "ping"}).encode()
        raise AssertionError(path)
    with patch.object(proxy, "fetch", fetch):
        try: proxy.verify(identity_plan, True)
        except ValueError: assert broken is not None
        else: assert broken is None

wrong_target = {"record": {"id": "www", "name": "www.wfsim.app", "type": "A", "content": "203.0.113.2", "proxied": False, "ttl": 300}}
with patch.object(proxy, "call", side_effect=AssertionError("rollback must reject another target before API access")):
    try: proxy.rollback(wrong_target, {"records": [wrong_target["record"]]})
    except ValueError: pass
    else: raise AssertionError("rollback accepted the www record")
print("root proxy recovery: receipt guards, unchanged www/routes/domains, exact app identity and owned rollback pass")
