#!/usr/bin/env python3
"""Publish the checked site artifact to the website host without building it."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
BACKEND = "https://wfsim.mgnmz1999.workers.dev"


def inventory(site):
    files = []
    for path in sorted(site.rglob("*")):
        relative = path.relative_to(site).as_posix()
        if relative == "board.meta.json" or relative.startswith("board/"):
            continue
        if path.is_symlink():
            raise ValueError("website artifacts cannot contain links")
        if path.is_file():
            files.append({"p": relative, "h": hashlib.sha256(path.read_bytes()).hexdigest(),
                          "n": path.stat().st_size})
    return files


def backend_release():
    response = subprocess.run(["curl", "--fail", "--silent", "--show-error", "--max-time", "40",
                               BACKEND + "/release.json"], capture_output=True, text=True, check=True)
    return json.loads(response.stdout)


def backend_matches(release):
    live = backend_release()
    if any(live.get(key) != release.get(key) for key in ("release", "engine", "wasm")):
        raise ValueError("backend and website artifact are different releases")


def connection():
    config = json.loads(os.environ["WEBSITE_TARGET"])
    host = config["host"]
    port = str(config.get("port", 22))
    if not re.fullmatch(r"[a-z_][a-z0-9_-]*@[a-zA-Z0-9.:-]+", host) or not port.isdigit():
        raise ValueError("invalid website SSH target")
    return ["ssh", "-p", port, "-i", str(ROOT / config["key"]),
            "-o", "UserKnownHostsFile=" + str(ROOT / config["known_hosts"]),
            "-o", "IdentitiesOnly=yes", "-o", "StrictHostKeyChecking=yes",
            "-o", "BatchMode=yes", "-o", "ConnectTimeout=20", "-o", "ServerAliveInterval=15", host]


REMOTE_VERIFY = '''import hashlib,json,os,pathlib,sys
root=pathlib.Path(sys.argv[1])
body=(root/'manifest.json').read_bytes()
if hashlib.sha256(body).hexdigest()!=sys.argv[2]: raise SystemExit('manifest mismatch')
manifest=json.loads(body)
site=root/'site'
for entry in manifest['files']:
 p=pathlib.PurePosixPath(entry['p'])
 if p.is_absolute() or '..' in p.parts: raise SystemExit('unsafe artifact path')
 f=site/p
 if not f.is_file() or f.is_symlink() or f.stat().st_size!=entry['n'] or hashlib.sha256(f.read_bytes()).hexdigest()!=entry['h']: raise SystemExit('artifact mismatch: '+str(p))
release=json.loads((site/'release.json').read_text())
if release['release']!=manifest['release']: raise SystemExit('release mismatch')
print('verified',len(manifest['files']),'website files, release',manifest['release'])
'''

REMOTE_ACTIVATE = '''import json,os,pathlib,shutil,sys,urllib.request
base=pathlib.Path(sys.argv[2]) if len(sys.argv)>2 else pathlib.Path('/srv/wfsim')
target=pathlib.Path(sys.argv[1])/'site'
current=base/'current'
previous=base/'previous'
old=current.resolve() if current.exists() else None
def point(path,destination):
 temporary=path.with_name(path.name+'.next')
 temporary.unlink(missing_ok=True)
 temporary.symlink_to(destination)
 temporary.replace(path)
if old and old!=target: point(previous,old)
point(current,target)
try:
 health=sys.argv[3] if len(sys.argv)>3 else 'http://127.0.0.1/__health/release'
 req=urllib.request.Request(health,headers={'Host':'wfsim.app'})
 with urllib.request.urlopen(req,timeout=15) as response: live=json.load(response)
 expected=json.loads((target/'release.json').read_text())
 if live['release']!=expected['release']: raise ValueError('website health mismatch')
except Exception:
 if old: point(current,old)
 else: current.unlink(missing_ok=True)
 raise
keep={target.parent,old.parent if old else None}
if previous.exists(): keep.add(previous.resolve().parent)
generations=sorted((p for p in (base/'releases').iterdir() if p.is_dir() and not p.name.startswith('.')),key=lambda p:p.stat().st_mtime,reverse=True)
keep.update(generations[:3])
for directory in generations:
 if directory not in keep: shutil.rmtree(directory)
print('website active:',live['release'])
'''


def deploy(site):
    release = json.loads((site / "release.json").read_text())
    if not re.fullmatch(r"[a-f0-9]{12,64}", release["release"]):
        raise ValueError("invalid release identity")
    backend_matches(release)
    manifest = json.dumps({"release": release["release"], "files": inventory(site)},
                          sort_keys=True, separators=(",", ":")).encode()
    digest = hashlib.sha256(manifest).hexdigest()
    destination = "/srv/wfsim/releases/" + release["release"] + "-" + digest[:16]
    command = ("set -eu; stage=$(mktemp -d /srv/wfsim/releases/.incoming.XXXXXX); "
               "trap 'rm -rf \"$stage\"' EXIT; tar -xzf - -C \"$stage\"; chmod 755 \"$stage\"; "
               "python3 -c " + shlex.quote(REMOTE_VERIFY) + ' "$stage" ' + digest + "; "
               "if [ ! -d " + shlex.quote(destination) + " ]; then mv \"$stage\" " + shlex.quote(destination) + "; fi; "
               "chmod 755 " + shlex.quote(destination) + "; "
               "python3 -c " + shlex.quote(REMOTE_VERIFY) + " " + shlex.quote(destination) + " " + digest + "; "
               "python3 -c " + shlex.quote(REMOTE_ACTIVATE) + " " + shlex.quote(destination))
    ssh = connection()
    with tempfile.TemporaryFile() as errors:
        process = subprocess.Popen(ssh + [command], stdin=subprocess.PIPE, stderr=errors)
        try:
            with tarfile.open(fileobj=process.stdin, mode="w|gz", compresslevel=1) as archive:
                info = tarfile.TarInfo("manifest.json")
                info.size = len(manifest)
                info.mode = 0o644
                archive.addfile(info, io.BytesIO(manifest))
                for entry in json.loads(manifest)["files"]:
                    archive.add(site / entry["p"], arcname="site/" + entry["p"], recursive=False)
        finally:
            process.stdin.close()
        code = process.wait(timeout=300)
        if code:
            errors.seek(0)
            raise RuntimeError("website deployment failed: " + errors.read().decode(errors="replace")[-2000:])


def self_test():
    with tempfile.TemporaryDirectory() as directory:
        site = Path(directory)
        (site / "board").mkdir()
        (site / "board/stale.json").write_text("stale")
        (site / "index.html").write_text("checked artifact")
        (site / "release.json").write_text('{"release":"012345abcdef"}')
        files = inventory(site)
        assert [f["p"] for f in files] == ["index.html", "release.json"]
        stage = site / "stage"
        (stage / "site").mkdir(parents=True)
        for entry in files:
            (stage / "site" / entry["p"]).write_bytes((site / entry["p"]).read_bytes())
        body = json.dumps({"release": "012345abcdef", "files": files}).encode()
        (stage / "manifest.json").write_bytes(body)
        args = ["python3", "-c", REMOTE_VERIFY, str(stage), hashlib.sha256(body).hexdigest()]
        assert subprocess.run(args, capture_output=True).returncode == 0
        (stage / "site/index.html").write_text("corrupted artifact")
        assert subprocess.run(args, capture_output=True).returncode != 0
        assert subprocess.run(args[:-1] + ["0" * 64], capture_output=True).returncode != 0
        base = site / "host"
        old = base / "releases/old/site"
        old.mkdir(parents=True)
        (old / "release.json").write_text('{"release":"old"}')
        (base / "current").symlink_to(old)
        failed = subprocess.run(["python3", "-c", REMOTE_ACTIVATE, str(stage), str(base),
                                 "http://127.0.0.1:9/"], capture_output=True)
        assert failed.returncode != 0
        assert (base / "current").resolve() == old.resolve()
    print("website publication: live board excluded; intact artifact accepted; corrupt content and manifest rejected")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--site", type=Path, default=ROOT / "site")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        deploy(args.site)
