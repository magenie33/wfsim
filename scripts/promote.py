#!/usr/bin/env python3
"""Promote a staged release: check it, sign one pointer, publish it.

THE HALF THAT NEEDS THE KEY, and it is the only half. Everything it publishes
already exists in the bucket (`scripts/stage.py`); this BUILDS NOTHING, which is
the property that matters — a promotion cannot produce bytes that differ from
the ones staged and deployed, because it has no way to produce bytes at all.

IT VERIFIES BEFORE IT SIGNS, and that is not ceremony. Signing whatever digest a
runner reported would make a compromised runner a compromised reader, and the
gate would be theatre: the whole point of keeping the key off CI is that a
person checks what CI produced. So the staged manifest is fetched back and every
file it names is hashed against this tree.

    python scripts/promote.py                  # the release this tree holds
    python scripts/promote.py <digest>         # a specific staged manifest
    python scripts/promote.py --dry-run

A DIRTY TREE IS NOT REFUSED here — `site/` is what is being checked, not the
commit. What is refused is a tree whose `site/` does not match the manifest.
"""
import argparse
import hashlib
import json
import pathlib
import subprocess
import sys
import urllib.request

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import cos  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
CHANNEL = "https://wfsim-1388973035.cos.ap-shanghai.myqcloud.com"
# Written INTO the pointer, so the channel can be moved without shipping a new
# shell — a client adopts the list it is served.
SOURCES = [CHANNEL, "https://wfsim.app"]


def updatekit(*args: str) -> None:
    exe = pathlib.Path.home() / ".cargo" / "bin" / "cargo.exe"
    cargo = str(exe) if exe.exists() else "cargo"
    # UTF-8, NOT THE LOCALE: `text=True` alone decodes with the console
    # codepage, and on a Chinese Windows that raises inside the reader thread —
    # so the failure would report the word "None" instead of the error.
    r = subprocess.run(
        [cargo, "run", "--quiet", "--manifest-path", str(ROOT / "desktop" / "Cargo.toml"),
         "--example", "updatekit", "--", *args],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    if r.returncode != 0:
        sys.exit(f"updatekit {' '.join(args)} failed:\n{r.stdout}\n{r.stderr}")


def fetch(url: str) -> bytes:
    with urllib.request.urlopen(url, timeout=60) as r:
        return r.read()


def site_release() -> str:
    f = SITE / "release.json"
    return json.loads(f.read_text(encoding="utf-8"))["release"] if f.exists() else ""


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("digest", nargs="?", help="a staged manifest's digest; default: this tree's")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    # WHICH MANIFEST TO PROMOTE, resolved through the release this tree holds.
    #
    # NOT RECOMPUTED FROM THE TREE. A manifest names the commit it was built
    # from, so recomputing it here would produce a different digest from the
    # staged one on any later commit — and then this would fetch a manifest that
    # does not exist and refuse a release that is sitting there, correct.
    if args.digest:
        digest = args.digest
    else:
        release = site_release()
        if not release:
            sys.exit("site/release.json is missing — build site/ before promoting it")
        marker = json.loads(fetch(f"{CHANNEL}/release/{release}.json"))
        digest = marker["manifest"]
        print(f"release {release} was staged as manifest {digest[:12]}")

    body = fetch(f"{CHANNEL}/manifest/{digest}.json")
    got = hashlib.sha256(body).hexdigest()
    if got != digest:
        sys.exit(f"the bucket's manifest/{digest}.json hashes to {got} — refusing")
    manifest = json.loads(body)

    # EVERY FILE, AGAINST THIS TREE. The staged manifest is a claim by whatever
    # produced it; this is the only place that claim is tested.
    bad = []
    for e in manifest["files"]:
        f = SITE / e["p"]
        if not f.exists() or hashlib.sha256(f.read_bytes()).hexdigest() != e["h"]:
            bad.append(e["p"])
    if bad:
        print(f"THE STAGED RELEASE IS NOT THIS TREE — {len(bad)} file(s) differ:")
        for p in bad[:10]:
            print(f"  {p}")
        sys.exit("\nBuild `site/` from the commit that was staged, or stage this tree.")
    print(f"verified {len(manifest['files'])} files against site/  —  manifest {digest[:12]}")

    signed = json.dumps(
        {"manifest": digest, "release": site_release(), "version": manifest["version"],
         "sources": SOURCES},
        ensure_ascii=False, separators=(",", ":"), sort_keys=True,
    )
    tmp = ROOT / "desktop" / "target" / "channel-body.json"
    tmp.parent.mkdir(parents=True, exist_ok=True)
    tmp.write_text(signed, encoding="utf-8", newline="")
    updatekit("sign", str(tmp))
    sig = (tmp.parent / "channel-body.json.sig").read_text(encoding="utf-8").strip()
    pointer = json.dumps({"sig": sig, "signed": signed},
                         ensure_ascii=False, separators=(",", ":")).encode("utf-8")

    if args.dry_run:
        print(f"\n[DRY RUN] would publish {len(pointer)} bytes:\n{signed}")
        return

    c = cos.creds()
    cos.put(c, "channel.json", pointer, "application/json; charset=utf-8")
    print(f"published channel.json — release {site_release() or '(none)'}")

    live = json.loads(fetch(f"{CHANNEL}/channel.json"))
    if json.loads(live["signed"])["manifest"] != digest:
        sys.exit("the channel came back naming a different manifest — publish again")
    print("read back — the channel points at what was just verified")


if __name__ == "__main__":
    main()
