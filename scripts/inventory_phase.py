#!/usr/bin/env python3
"""Run one inventory phase with progress and child resource measurements."""
import json
import os
from pathlib import Path
import resource
import shutil
import signal
import subprocess
import sys
import time
import tempfile


def readings(path):
    try:
        return dict(line.split(":", 1) for line in path.read_text().splitlines() if ":" in line)
    except OSError:
        return {}


def resources(pid, proc=Path("/proc"), cgroup=Path("/sys/fs/cgroup")):
    result = {"scratch_free_bytes": shutil.disk_usage(tempfile.gettempdir()).free}
    if not proc.is_dir():
        reading = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True)
        if reading.stdout.strip().isdigit():
            result["process_rss_bytes"] = int(reading.stdout.strip()) * 1024
        return result
    status = readings(proc / str(pid) / "status")
    for name, key in [("VmRSS", "process_rss_bytes"), ("VmHWM", "process_peak_rss_bytes"),
                      ("VmSwap", "process_swap_bytes")]:
        if name in status:
            result[key] = int(status[name].split()[0]) * 1024
    memory = readings(proc / "meminfo")
    if "MemAvailable" in memory:
        result["memory_available_bytes"] = int(memory["MemAvailable"].split()[0]) * 1024
    for name in ("memory", "io"):
        try:
            result[name + "_pressure"] = (proc / "pressure" / name).read_text().strip()
        except OSError:
            pass
    try:
        group = next(line.split(":", 2)[2] for line in (proc / str(pid) / "cgroup").read_text().splitlines()
                     if line.startswith("0::"))
        root = cgroup / group.lstrip("/")
        result["cgroup_memory_bytes"] = int((root / "memory.current").read_text())
        result["cgroup_memory_events"] = dict((key, int(value)) for key, value in
                                               (line.split() for line in (root / "memory.events").read_text().splitlines()))
    except (OSError, StopIteration, ValueError):
        pass
    return result


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
    report("progress", pid=child.pid, resources=resources(child.pid))
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
                report("progress", pid=child.pid, resources=resources(child.pid))
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
