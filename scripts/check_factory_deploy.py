"""Factory deployment uses one versioned artifact and explicit target credentials."""
import importlib.util
import io
import os
from pathlib import Path
import tarfile
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("factory_deploy", Path(__file__).with_name("ship_factoryd.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

with patch.dict(os.environ, {"FACTORY_TARGET": "root@new-host", "FACTORY_SSH_KEY": "/key", "FACTORY_KNOWN_HOSTS": "/hosts"}, clear=True):
    assert module.target() == {"host": "root@new-host", "key": "/key", "known_hosts": "/hosts"}
with patch.dict(os.environ, {"FACTORY_TARGET": "root@new-host"}, clear=True):
    try:
        module.target()
        raise AssertionError("an incomplete target must fail")
    except SystemExit:
        pass
with patch.dict(os.environ, {"R2_ACCESS_KEY_ID": "only-one-value"}, clear=True):
    try:
        module.r2_token()
        raise AssertionError("partial credentials must fail")
    except SystemExit:
        pass
sql = module.index_sql()
assert sql.count("CREATE INDEX CONCURRENTLY IF NOT EXISTS") == 3
assert "INCLUDE (measured_by)" in sql and "WHERE odd = 1" in sql
assert "BEGIN" not in sql and "DROP" not in sql
with tarfile.open(fileobj=io.BytesIO(module.bundle("c0ffee")), mode="r:gz") as archive:
    names = archive.getnames()
    for name in ["scripts/factoryd.mjs", "scripts/factory_capacity.mjs", "scripts/sigv4.mjs", "worker/contribution.js", "COMMIT"]:
        assert name in names, name
    assert any(name.startswith("scripts/pg/node_modules/postgres/src/") for name in names)
    assert not any(name.startswith("private/") or name.endswith(".env") for name in names)
    assert archive.extractfile("COMMIT").read() == b"c0ffee\n"
print("factory deployment checks passed")
