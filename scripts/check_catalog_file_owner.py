#!/usr/bin/env python3
"""Exercise the file handoff with two real, disjoint unprivileged OS identities."""
import contextlib
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import tempfile

from run_rank_catalog_batch import as_user, serve_files


def main():
    if os.getuid() != 0:
        raise SystemExit("run inside an isolated Linux container as root")
    controller, reader = as_user("daemon"), as_user("nobody")
    repo = Path(__file__).resolve().parent
    node = shutil.which("node")
    with tempfile.TemporaryDirectory() as tmp:
        base = Path(tmp)
        base.chmod(0o711)
        code = base / "code"
        code.mkdir(mode=0o755)
        pending = ["board_catalog_cut_files.mjs", "board_catalog_file_owner.mjs"]
        while pending:
            name = pending.pop()
            if (code / name).exists():
                continue
            body = (repo / name).read_text()
            (code / name).write_text(body)
            pending.extend(re.findall(r"""from ['"]\./([a-z_0-9]+\.mjs)['"]""", body))
        if subprocess.run(["test", "-x", node], **reader).returncode:
            node = shutil.copyfile(node, code / "node")
            Path(node).chmod(0o755)
        state, work = base / "state", base / "work"
        for path, user in [(state, controller), (work, reader)]:
            path.mkdir(mode=0o700)
            os.chown(path, user["user"], user["group"])
        manifest = state / "manifest"
        manifest.mkdir(mode=0o700)
        os.chown(manifest, controller["user"], controller["group"])
        source = manifest / "inputs.ndjson"
        data = b'{"kind":"meta","weapon":"strun"}\n{"kind":"complete","total":0}\n'
        source.write_bytes(data)
        os.chown(source, controller["user"], controller["group"])
        assert subprocess.run(["test", "-r", source], **reader).returncode != 0
        assert subprocess.run(["test", "-x", work], **controller).returncode != 0
        sha = hashlib.sha256(data).hexdigest()
        short = sha[:32]
        partial_dir = work / ("rank-cut-" + "-".join((short[:8], short[8:12], short[12:16], short[16:20], short[20:])))
        partial_dir.mkdir(mode=0o750)
        os.chown(partial_dir, reader["user"], reader["group"])
        partial = partial_dir / "inputs.partial"
        partial.write_bytes(data[:11])
        os.chown(partial, reader["user"], reader["group"])
        partial.chmod(0o440)
        env = {"PATH": os.environ["PATH"]}
        probe = """
import assert from 'node:assert/strict';
import {connectFileOwner} from './board_catalog_file_owner.mjs';
const [fd,work,device,sha,n]=process.argv.slice(1);
const ctl=await connectFileOwner(+fd,work,device);
try{
 const m={sha256:sha,bytes:+n};
 await assert.rejects(ctl.stage({...m,sha256:'f'.repeat(64)}),/differs from the sealed input/);
 const first=await ctl.stage(m),again=await ctl.stage(m);
 assert.deepEqual(first,again);assert.equal(first.meta.weapon,'strun');
 assert.equal(await ctl.verify({path:first.path,cut:sha,bytes:+n}),first.path);
 await assert.rejects(ctl.verify({path:first.path,cut:'0'.repeat(64),bytes:+n}),/digest changed/);
 await assert.rejects(ctl.verify({path:'/other/inputs.ndjson',cut:sha,bytes:+n}),/outside/);
 console.log(JSON.stringify(first));
}finally{ctl.close();}
"""
        with contextlib.ExitStack() as stack:
            parent, child = socket.socketpair()
            stack.enter_context(parent)
            stack.enter_context(child)
            proc = subprocess.Popen([node, "--input-type=module", "-e", probe, str(child.fileno()), str(work),
                                     str(work.stat().st_dev), sha, str(len(data))], cwd=code, env=env,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, pass_fds=(child.fileno(),), **controller)
            child.close()
            serve_files(parent, node, code / "board_catalog_cut_files.mjs", work, state, controller, reader, env)
            result, error = proc.communicate(timeout=10)
            assert proc.returncode == 0, error.decode()
        receipt = json.loads(result)
        path = Path(receipt["path"])
        assert path.read_bytes() == data and path.stat().st_uid == reader["user"] and path.stat().st_mode & 0o777 == 0o440
        assert len(list(work.glob("*/inputs.ndjson"))) == 1
        assert not partial.exists()
        assert subprocess.run(["test", "-r", path], **controller).returncode != 0
        print("ok — disjoint service uids, sealed input descriptor, reader-owned recoverable cut and verified old cut")


if __name__ == "__main__":
    main()
