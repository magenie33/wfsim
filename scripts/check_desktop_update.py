#!/usr/bin/env python3
"""Verify the desktop update channel by actually performing an update.

WHY THIS EXISTS. The update path is the one part of the client that cannot be
checked by using the app: it does nothing until a newer version exists, and on
the machine that builds it, one never does. Every failure along it — an
unreachable source, a signature that will not verify, a manifest that will not
parse, a diff computed wrong, a directory swap that half-happens — presents to
a reader as the same silent symptom: updates stop arriving, on a client that
otherwise works perfectly. And it is the ONE defect this design cannot recover
from, because the mechanism that would deliver the fix is the broken one.

So the test manufactures a release. It publishes a baseline, changes one file,
publishes again, and drives a real client through seeing, fetching and applying
that change — then puts the bucket back where it found it.

WHAT IT ASSERTS, beyond "no error":
  · the client sees an update at all (source, signature, parse, compare)
  · it fetches a DIFF — a one-file release must not pull 764 files, and that
    number is the difference between a 40 KB update and a 29 MB one
  · the swap actually happened, checked by reading the changed file out of
    `current/` rather than by trusting the status the app reported

Usage:  python scripts/check_desktop_update.py
"""
import hashlib
import json
import os
import pathlib
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import cos  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
CARGO = pathlib.Path.home() / ".cargo" / "bin" / "cargo.exe"
EXE = ROOT / "desktop" / "target" / "debug" / "wfsim-desktop.exe"
CURRENT = pathlib.Path(os.environ["LOCALAPPDATA"]) / "WFSim" / "current"

# Small, unmistakable, and not something any other check reads.
VICTIM = ROOT / "site" / "logo.svg"
MARK = "<!-- update-channel probe -->"
# A second, distinct change for the refusal tests: they need a release the
# client has not already got, so the bucket is genuinely ahead when the blob or
# the signature is then corrupted.
MARK2 = b"\n<!-- update-channel probe 2 -->\n"

FAILED = []


def step(msg: str) -> None:
    print(f"\n=== {msg}")


def run(*cmd: str, quiet: bool = False) -> str:
    r = subprocess.run(cmd, capture_output=True, text=True, cwd=ROOT,
                       encoding="utf-8", errors="replace")
    if r.returncode != 0:
        print(r.stdout[-3000:])
        print(r.stderr[-3000:])
        sys.exit(f"command failed: {' '.join(cmd)}")
    if not quiet:
        tail = [ln for ln in r.stdout.split("\n") if ln.strip()][-3:]
        for ln in tail:
            print("    " + ln, flush=True)
    return r.stdout


def build() -> None:
    run(str(CARGO), "build", "--manifest-path", str(ROOT / "desktop" / "Cargo.toml"), quiet=True)


def publish() -> None:
    run(sys.executable, str(ROOT / "scripts" / "release_desktop.py"))


def client(*args: str, timeout: int = 300) -> str:
    """Run the shell in a check mode and return its output."""
    r = subprocess.run([str(EXE), *args], capture_output=True, text=True, timeout=timeout,
                       encoding="utf-8", errors="replace")
    out = "\n".join(
        ln for ln in (r.stdout + r.stderr).split("\n")
        if not ln.startswith("[serve]") and "PICKER" not in ln
    )
    print(out.strip()[-2500:], flush=True)
    if r.returncode != 0:
        FAILED.append(" ".join(args))
    return out


def main() -> None:
    if not EXE.exists():
        sys.exit(f"missing {EXE} — run cargo build first")
    original = VICTIM.read_bytes()

    try:
        step("1. publish the baseline and install a client at it")
        build()
        publish()
        # --reset forces the payload to be unpacked, so `current/` is known to
        # match what was just published.
        client("--selftest", "--reset")
        if MARK.encode() in (CURRENT / VICTIM.name).read_bytes():
            sys.exit("the baseline already carries the probe mark — clean site/ first")

        step("2. change one file and publish that")
        VICTIM.write_bytes(original + f"\n{MARK}\n".encode())
        build()
        publish()

        step("3. restore the working tree, so only the BUCKET is ahead")
        VICTIM.write_bytes(original)

        step("4. drive a real client through the update")
        client("--selftest-update")

        step("5. read the changed file out of current/")
        got = (CURRENT / VICTIM.name).read_bytes()
        if MARK.encode() in got:
            print(f"    PASS  current/{VICTIM.name} carries the published change")
        else:
            print(f"    FAIL  current/{VICTIM.name} is still the old copy")
            FAILED.append("swap did not take effect")

        step("6. the updated client still runs")
        client("--selftest")

        # ---- the negative half. An update channel that cannot REFUSE is not a
        # channel with a weak guarantee, it is a way to run anything on every
        # reader's machine. Both refusals are exercised against a live bucket,
        # because the code path that matters is the one behind the network.

        step("7. a blob that does not match its hash must be refused")
        VICTIM.write_bytes(original + MARK2)
        build()
        publish()
        VICTIM.write_bytes(original)
        c = cos.creds()
        manifest = json.loads((ROOT / "desktop" / "target" / "manifest.json").read_text(encoding="utf-8"))
        vh = next(f["h"] for f in manifest["files"] if f["p"] == VICTIM.name)
        good_blob = (ROOT / "desktop" / "target" / "good_blob.bin")
        good_blob.write_bytes(original + MARK2)
        cos.put(c, f"blob/{vh}", b"this is not what the manifest promised", "image/svg+xml")
        before = (CURRENT / VICTIM.name).read_bytes()
        client("--selftest-update", "--expect-refused")
        if (CURRENT / VICTIM.name).read_bytes() == before:
            print("    PASS  current/ untouched by the refused update")
        else:
            print("    FAIL  current/ was modified by an update that should have been refused")
            FAILED.append("corrupt blob was applied")
        cos.put(c, f"blob/{vh}", good_blob.read_bytes(), "image/svg+xml")

        step("8. a manifest whose signature does not verify must be refused")
        good_sig = (ROOT / "desktop" / "target" / "manifest.json.sig").read_bytes()
        bad = bytearray(good_sig)
        bad[-1] = ord("0") if bad[-1] != ord("0") else ord("1")
        cos.put(c, "manifest.json.sig", bytes(bad), "text/plain; charset=utf-8")
        before = (CURRENT / VICTIM.name).read_bytes()
        client("--selftest-update", "--expect-refused")
        if (CURRENT / VICTIM.name).read_bytes() == before:
            print("    PASS  current/ untouched by an unsigned update")
        else:
            print("    FAIL  an update with a bad signature was applied")
            FAILED.append("bad signature was accepted")
        cos.put(c, "manifest.json.sig", good_sig, "text/plain; charset=utf-8")

    finally:
        step("cleanup: restore the tree and republish the baseline")
        VICTIM.write_bytes(original)
        build()
        publish()
        # And bring the client back to the baseline too, so a rerun starts from
        # the same place this one did.
        client("--selftest-update", timeout=300)

    print("\n" + "=" * 60)
    if FAILED:
        print("FAILED: " + ", ".join(FAILED))
        sys.exit(1)
    print("update channel OK: sees, diffs, downloads, verifies, swaps, runs")


if __name__ == "__main__":
    main()
