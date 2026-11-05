"""DNS permission failures preserve the site; partial cutovers restore it."""
import contextlib
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
