#!/usr/bin/env python3
"""Run the catalog controller and its cut-file operations under their existing service accounts."""
import argparse
import contextlib
import json
import os
from pathlib import Path
import pwd
import shutil
import signal
import socket
import stat
import subprocess


def as_user(name):
    account = pwd.getpwnam(name)
    if not account.pw_uid:
        raise ValueError("service account may not be root")
    return dict(user=account.pw_uid, group=account.pw_gid, extra_groups=[])


def exact_directory(value):
    path = Path(value)
    if path.resolve() != path or not path.is_dir():
        raise ValueError("expected an absolute existing service directory")
    return path


def serve_files(link, node, helper, work, state, controller, reader, env):
    with link.makefile("rwb", buffering=0) as stream:
        while line := stream.readline(8193):
            if len(line) > 8192 or not line.endswith(b"\n"):
                raise ValueError("invalid transfer frame")
            try:
                request = json.loads(line)
                action = request["action"]
                if action not in ("probe", "stage", "verify"):
                    raise ValueError("unknown file action")
                with contextlib.ExitStack() as stack:
                    source = subprocess.DEVNULL
                    if action == "stage":
                        directory = exact_directory(state / "manifest")
                        if directory.stat().st_uid != controller["user"]:
                            raise ValueError("unexpected sealed input owner")
                        parent = os.open(directory, os.O_DIRECTORY | os.O_NOFOLLOW)
                        stack.callback(os.close, parent)
                        fd = os.open("inputs.ndjson", os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
                        source = stack.enter_context(os.fdopen(fd, "rb"))
                        info = os.fstat(fd)
                        if not stat.S_ISREG(info.st_mode) or info.st_uid != controller["user"] or info.st_size != request["bytes"]:
                            raise ValueError("unexpected sealed input file")
                    child = subprocess.run([node, helper, work, json.dumps(request)], stdin=source, stdout=subprocess.PIPE,
                                           stderr=subprocess.PIPE, env=env, **reader, timeout=150, check=False)
                    if child.returncode:
                        reason = next((x.strip() for x in child.stderr.decode(errors="replace").splitlines()
                                       if x.startswith(("Error:", "AssertionError", "BoardRankQualificationError:"))), "stage or digest check failed")
                        raise ValueError("reader-owned file verification failed: " + reason[:300])
                    response = json.loads(child.stdout)
                    if response["uid"] != reader["user"] or response["gid"] != reader["group"]:
                        raise ValueError("cut ran under a different reader account")
            except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
                response = {"error": str(error)}
            stream.write((json.dumps(response, separators=(",", ":")) + "\n").encode())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("plan", "step", "run", "status"))
    parser.add_argument("config", type=Path)
    args = parser.parse_args()
    if os.geteuid() != 0:
        parser.error("the service supervisor must launch both existing service identities")
    config = json.loads(args.config.read_bytes())
    controller, reader = as_user("postgres"), as_user("wfsim-board")
    state, work = exact_directory(config["state"]), exact_directory(config["work"])
    if state.stat().st_uid != controller["user"] or work.stat().st_uid != reader["user"]:
        raise ValueError("catalog controller and ranking work require their respective owners")
    node = shutil.which("node")
    if not node:
        raise ValueError("node is unavailable")
    names = Path(__file__).resolve().parent
    # The reader receives one input descriptor. Database connection settings belong to the controller.
    file_env = {k: v for k, v in os.environ.items() if k in ("PATH", "LANG", "LC_ALL", "WFSIM_BOARD_DATA_MOUNT", "WFSIM_BOARD_DATA_UUID")}
    file_env.update(WFSIM_BOARD_WORK_ROOT=str(work), WFSIM_BOARD_STORAGE_REQUIRED="1")
    if "WFSIM_BOARD_DATA_UUID" not in file_env or "WFSIM_BOARD_DATA_MOUNT" not in file_env:
        raise ValueError("production Data mount and UUID are required")
    permitted = ("PG_URL", "PGHOST", "PGPORT", "PGDATABASE", "PGUSER", "PGPASSFILE", "PGSSLMODE")
    env = {**file_env, **{key: os.environ[key] for key in permitted if key in os.environ}}
    def stop(_sig, _frame):
        raise SystemExit(143)
    signal.signal(signal.SIGTERM, stop)
    with contextlib.ExitStack() as stack:
        parent, child = socket.socketpair()
        stack.enter_context(parent)
        stack.enter_context(child)
        env["WFSIM_CATALOG_FILE_FD"] = str(child.fileno())
        proc = subprocess.Popen([node, names / "adopt_rank_catalog_batch.mjs", args.action, args.config],
                                env=env, pass_fds=(child.fileno(),), **controller)
        child.close()
        try:
            serve_files(parent, node, names / "board_catalog_cut_files.mjs", work, state, controller, reader, file_env)
        finally:
            parent.close()
            if proc.poll() is None:
                try:
                    proc.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    proc.terminate()
                    proc.wait(timeout=8)
        return proc.returncode


if __name__ == "__main__":
    raise SystemExit(main())
