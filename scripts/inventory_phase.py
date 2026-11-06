#!/usr/bin/env python3
"""Run one inventory phase with progress and child resource measurements."""
import json
import os
import resource
import signal
import subprocess
import sys
import time


def run(phase, command):
    started = time.monotonic()
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    stopped = []

    def report(state, **fields):
        print(json.dumps({"event": "inventory_phase", "phase": phase,
                          "state": state, "wall_seconds": round(time.monotonic() - started, 3),
                          **fields}, separators=(",", ":")), file=sys.stderr, flush=True)

    report("start")
    try:
        child = subprocess.Popen(command, start_new_session=True)
    except OSError as error:
        report("end", exit_code=127, error=str(error))
        return 127

    def forward(number, _frame):
        if not stopped:
            stopped.extend([number, time.monotonic()])
        try:
            os.killpg(child.pid, number)
        except ProcessLookupError:
            pass

    for number in (signal.SIGINT, signal.SIGTERM):
        signal.signal(number, forward)
    heartbeat = time.monotonic() + 30
    while True:
        try:
            status = child.wait(timeout=1)
            break
        except subprocess.TimeoutExpired:
            now = time.monotonic()
            if stopped and now - stopped[1] >= 10:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            if now >= heartbeat:
                report("progress")
                heartbeat = now + 30
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    status = 128 - status if status < 0 else status
    if stopped:
        status = 128 + stopped[0]
    report("end", exit_code=status,
           user_seconds=round(after.ru_utime - before.ru_utime, 3),
           system_seconds=round(after.ru_stime - before.ru_stime, 3),
           peak_rss_bytes=after.ru_maxrss * (1 if sys.platform == "darwin" else 1024))
    return status


if __name__ == "__main__":
    if len(sys.argv) < 4 or sys.argv[2] != "--":
        sys.exit("usage: inventory_phase.py <phase> -- <command> [args...]")
    sys.exit(run(sys.argv[1], sys.argv[3:]))
