#!/usr/bin/env python3
"""Publish the checked site artifact to the website host without building it."""
import argparse
import gzip
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


def connection(config=None):
    if config is None:
        config = json.loads(os.environ["WEBSITE_TARGET"])
    host = config["host"]
    port = str(config.get("port", 22))
    if not re.fullmatch(r"[a-z_][a-z0-9_-]*@[a-zA-Z0-9.:-]+", host) or not port.isdigit():
        raise ValueError("invalid website SSH target")
    return ["ssh", "-p", port, "-i", str(ROOT / config["key"]),
            "-o", "UserKnownHostsFile=" + str(ROOT / config["known_hosts"]),
            "-o", "IdentitiesOnly=yes", "-o", "StrictHostKeyChecking=yes",
            "-o", "BatchMode=yes", "-o", "ConnectTimeout=20", "-o", "ServerAliveInterval=15", host]


def targets():
    configured = []
    for variable, hostname in (("WEBSITE_TARGET", "wfsim.app"), ("WEBSITE_MIRROR_TARGET", "cn.wfsim.app")):
        if os.environ.get(variable):
            config = json.loads(os.environ[variable])
            if config.get("hostname", hostname) != hostname:
                raise ValueError("website target names the wrong entrance")
            configured.append({**config, "hostname": hostname})
    if not configured:
        raise ValueError("no website target is configured")
    addresses = [(t["host"], str(t.get("port", 22))) for t in configured]
    if len(addresses) != len(set(addresses)):
        raise ValueError("website entrances must have separate hosts")
    return configured


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
for entry in manifest.get('support',[]):
 p=pathlib.PurePosixPath(entry['p'])
 if p.is_absolute() or '..' in p.parts: raise SystemExit('unsafe support path')
 f=root/p
 if not f.is_file() or f.is_symlink() or f.stat().st_size!=entry['n'] or hashlib.sha256(f.read_bytes()).hexdigest()!=entry['h']: raise SystemExit('support mismatch: '+str(p))
release=json.loads((site/'release.json').read_text())
if release['release']!=manifest['release']: raise SystemExit('release mismatch')
print('verified',len(manifest['files']),'website files, release',manifest['release'])
'''

REMOTE_ACTIVATE = '''import json,os,pathlib,runpy,shutil,sys,time,urllib.error,urllib.request
base=pathlib.Path(sys.argv[2]) if len(sys.argv)>2 else pathlib.Path('/srv/wfsim')
target=pathlib.Path(sys.argv[1])/'site'
current=base/'current'
previous=base/'previous'
old=current.resolve() if current.exists() else None
hostname=sys.argv[4] if len(sys.argv)>4 else 'wfsim.app'
configuration=None
if hostname=='cn.wfsim.app':
 configuration=runpy.run_path(str(target.parent/'hk.py'))['Configuration'](target.parent/'nginx')
def point(path,destination):
 temporary=path.with_name(path.name+'.next')
 temporary.unlink(missing_ok=True)
 temporary.symlink_to(destination)
 temporary.replace(path)
if old and old!=target: point(previous,old)
point(current,target)
try:
 if configuration: configuration.apply()
 health=sys.argv[3] if len(sys.argv)>3 else 'http://127.0.0.1/__health/release'
 expected=json.loads((target/'release.json').read_text())
 opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
 deadline=time.monotonic()+8
 ready=0
 while True:
  remaining=deadline-time.monotonic()
  if remaining<=0: raise ValueError('website health route did not become ready')
  req=urllib.request.Request(health,headers={'Host':hostname,'Connection':'close'})
  try:
   with opener.open(req,timeout=min(2,remaining)) as response: live=json.load(response)
  except urllib.error.HTTPError as error:
   if error.code!=404: raise
   ready=0
  else:
   if live!=expected: raise ValueError('website health mismatch')
   ready+=1
   if ready==2: break
  time.sleep(min(0.1,max(0,deadline-time.monotonic())))
 if configuration: configuration.check_bot()
except Exception:
 if old: point(current,old)
 else: current.unlink(missing_ok=True)
 if configuration: configuration.rollback()
 raise
keep={target.parent,old.parent if old else None}
if previous.exists(): keep.add(previous.resolve().parent)
generations=sorted((p for p in (base/'releases').iterdir() if p.is_dir() and not p.name.startswith('.')),key=lambda p:p.stat().st_mtime,reverse=True)
keep.update(generations[:3])
for directory in generations:
 if directory not in keep: shutil.rmtree(directory)
print('website active:',live['release'])
'''


def deploy(site, config=None):
    if config is None:
        config = json.loads(os.environ["WEBSITE_TARGET"])
    hostname = config.get("hostname", "wfsim.app")
    if hostname not in ("wfsim.app", "cn.wfsim.app"):
        raise ValueError("website target must name a public entrance")
    release = json.loads((site / "release.json").read_text())
    if not re.fullmatch(r"[a-f0-9]{12,64}", release["release"]):
        raise ValueError("invalid release identity")
    backend_matches(release)
    support = {}
    if hostname == "cn.wfsim.app":
        from website_config import files
        support = {"nginx/" + name: body.encode() for name, body in files().items()}
        support["hk.py"] = (ROOT / "scripts/website_hk.py").read_bytes()
    entries = [{"p": name, "h": hashlib.sha256(body).hexdigest(), "n": len(body)} for name, body in support.items()]
    manifest = json.dumps({"release": release["release"], "files": inventory(site), "support": entries},
                          sort_keys=True, separators=(",", ":")).encode()
    digest = hashlib.sha256(manifest).hexdigest()
    destination = "/srv/wfsim/releases/" + release["release"] + "-" + digest[:16]
    command = ("set -eu; install -d -m 755 /srv/wfsim /srv/wfsim/releases; "
               "stage=$(mktemp -d /srv/wfsim/releases/.incoming.XXXXXX); "
               "trap 'rm -rf \"$stage\"' EXIT; tar -xzf - -C \"$stage\"; chmod 755 \"$stage\"; "
               "python3 -c " + shlex.quote(REMOTE_VERIFY) + ' "$stage" ' + digest + "; "
               "if [ ! -d " + shlex.quote(destination) + " ]; then mv \"$stage\" " + shlex.quote(destination) + "; fi; "
               "chmod 755 " + shlex.quote(destination) + "; "
               "python3 -c " + shlex.quote(REMOTE_VERIFY) + " " + shlex.quote(destination) + " " + digest + "; "
               "python3 -c " + shlex.quote(REMOTE_ACTIVATE) + " " + shlex.quote(destination)
               + " /srv/wfsim http://127.0.0.1/__health/release " + shlex.quote(hostname))
    ssh = connection(config)
    if hostname == "cn.wfsim.app" and os.environ.get("BOT_VIA_COS") == "1":
        import cos
        import secrets
        c, key = cos.creds(), "website-courier/" + secrets.token_hex(16) + ".tar.gz"
        try:
            with tempfile.TemporaryFile() as packed:
                pack(packed, site, manifest, support)
                packed.seek(0)
                cos.put(c, key, packed.read(), "application/gzip")
            command = command.replace("tar -xzf - -C", "curl --fail --silent --show-error --max-time 300 "
                + shlex.quote("https://" + cos.host(c) + "/" + key) + " | tar -xzf - -C", 1)
            subprocess.run(ssh + ["bash -o pipefail -c " + shlex.quote(command)], check=True, timeout=360)
        finally:
            cos.delete(c, key)
        return
    with tempfile.TemporaryFile() as errors:
        process = subprocess.Popen(ssh + [command], stdin=subprocess.PIPE, stderr=errors)
        try:
            pack(process.stdin, site, manifest, support)
        finally:
            process.stdin.close()
        code = process.wait(timeout=300)
        if code:
            errors.seek(0)
            raise RuntimeError("website deployment failed: " + errors.read().decode(errors="replace")[-2000:])


def pack(stream, site, manifest, support):
    with gzip.GzipFile(filename="", fileobj=stream, mode="wb", compresslevel=1, mtime=0) as zipped:
        with tarfile.open(fileobj=zipped, mode="w|") as archive:
            for name, body in {"manifest.json": manifest, **support}.items():
                info = tarfile.TarInfo(name)
                info.size = len(body)
                info.mode = 0o644
                archive.addfile(info, io.BytesIO(body))
            for entry in json.loads(manifest)["files"]:
                archive.add(site / entry["p"], arcname="site/" + entry["p"], recursive=False)


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
        support = stage / "support.py"
        support.write_bytes(b"checked activation helper")
        supported = json.dumps({"release": "012345abcdef", "files": files, "support": [
            {"p": "support.py", "n": support.stat().st_size, "h": hashlib.sha256(support.read_bytes()).hexdigest()},
        ]}).encode()
        (stage / "manifest.json").write_bytes(supported)
        support_args = args[:-1] + [hashlib.sha256(supported).hexdigest()]
        assert subprocess.run(support_args, capture_output=True).returncode == 0
        support.write_bytes(b"tampered activation helper")
        assert subprocess.run(support_args, capture_output=True).returncode != 0
        (stage / "manifest.json").write_bytes(body)
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
    from check_hk_website import check
    check()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--site", type=Path, default=ROOT / "site")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        for config in targets():
            deploy(args.site, config)
