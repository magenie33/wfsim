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

spec = importlib.util.spec_from_file_location("website_dns", Path(__file__).with_name("website_dns.py"))
dns = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dns)
release = {"release": "012345abcdef", "engine": "engine", "wasm": "wasm"}


def exercise(permit, fail_second=False):
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
                patch.object(dns.urllib.request, "urlopen", return_value=io.BytesIO(json.dumps(release).encode())), \
                contextlib.redirect_stdout(io.StringIO()):
                try: dns.main()
                except (PermissionError, OSError): failed = True
                else: failed = False
            if not permit:
                assert failed and not removed and not records
            elif fail_second:
                assert failed and not records
                assert {w["hostname"] for w in workers} == set(dns.DOMAINS)
            else:
                assert not failed and not workers
                assert {r["name"] for r in records} == set(dns.DOMAINS)
                assert all(r["type"] == "A" and not r["proxied"] for r in records)
        finally:
            os.chdir(old)


exercise(False)
exercise(True, True)
exercise(True)
print("website DNS: denied edits preserve routing; partial failure restores both domains; validated switch succeeds")
