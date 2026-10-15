#!/usr/bin/env python3
"""WHO IS BUILDING OR SHIPPING FROM THIS CHECKOUT, so a deploy never takes a
half-written `site/` (docs/DEVELOPMENT.md §8).

`build_site_app.py` and `ship.py` hold `.build.lock` while they run; a child
they start inherits the hold through `WFSIM_LOCK_OWNER`. `ship_bot.py` run by
hand refuses while another live process holds it.

    python scripts/checkout_lock.py              # who holds it, if anyone
    python scripts/checkout_lock.py --self-test
"""
import contextlib
import json
import os
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCK = ROOT / ".build.lock"
OWNER_ENV = "WFSIM_LOCK_OWNER"


def alive(pid: int) -> bool:
    """WHETHER A PROCESS STILL RUNS. Never `os.kill(pid, 0)` on Windows: there
    any signal but CTRL_C/CTRL_BREAK is TerminateProcess, and it ends the process."""
    if pid <= 0:
        return False
    if os.name == "nt":
        out = subprocess.run(["tasklist", "/FI", f"PID eq {pid}", "/NH", "/FO", "CSV"],
                             capture_output=True, text=True).stdout
        return f'"{pid}"' in out
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def holder(lock: pathlib.Path = LOCK) -> dict | None:
    """The live process holding the lock, or None. A lock whose process is gone
    is a leftover and holds nothing."""
    try:
        h = json.loads(lock.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    return h if alive(int(h.get("pid", 0))) else None


def held_by_other(lock: pathlib.Path = LOCK) -> dict | None:
    """The holder, unless it is this process or the one that started it."""
    h = holder(lock)
    if not h:
        return None
    mine = {os.getpid(), int(os.environ.get(OWNER_ENV, "0") or 0)}
    return None if int(h["pid"]) in mine else h


@contextlib.contextmanager
def hold(what: str, lock: pathlib.Path = LOCK):
    """Hold the lock for the body. Refuses when another live process holds it;
    a child of the holder runs inside the hold and takes nothing."""
    other = held_by_other(lock)
    if other:
        sys.exit(f"{what}: {other['what']} (pid {other['pid']}, since {other['at']}) is using this "
                 f"checkout — wait for it, or work in a worktree")
    if holder(lock):
        yield
        return
    lock.write_text(json.dumps({"pid": os.getpid(), "what": what,
                                "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}), encoding="utf-8")
    os.environ[OWNER_ENV] = str(os.getpid())
    try:
        yield
    finally:
        with contextlib.suppress(OSError):
            if json.loads(lock.read_text(encoding="utf-8")).get("pid") == os.getpid():
                lock.unlink()


def self_test() -> int:
    import tempfile
    bad = 0

    def say(ok: bool, what: str) -> None:
        nonlocal bad
        bad += 0 if ok else 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}")

    with tempfile.TemporaryDirectory() as d:
        lock = pathlib.Path(d) / ".build.lock"
        say(holder(lock) is None, "no lock file is nobody")
        lock.write_text(json.dumps({"pid": 999999, "what": "x", "at": "t"}), encoding="utf-8")
        say(holder(lock) is None, "a lock whose process is gone holds nothing")
        sleeper = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"])
        try:
            lock.write_text(json.dumps({"pid": sleeper.pid, "what": "build_site_app", "at": "t"}), encoding="utf-8")
            say(held_by_other(lock) is not None, "another live process holds it")
            say(alive(sleeper.pid), "…and probing it leaves it running")
            os.environ[OWNER_ENV] = str(sleeper.pid)
            say(held_by_other(lock) is None, "a child of the holder runs inside its hold")
            os.environ.pop(OWNER_ENV, None)
        finally:
            sleeper.kill()
        lock.unlink()
        with hold("check", lock):
            say(json.loads(lock.read_text(encoding="utf-8"))["pid"] == os.getpid(), "hold writes this process")
        say(not lock.exists(), "…and lets go when the body ends")
    print(f"\n{'checkout lock ok' if not bad else f'{bad} failed'}")
    return 1 if bad else 0


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    h = holder()
    print(f"{h['what']} (pid {h['pid']}) since {h['at']}" if h else "nobody holds this checkout")
