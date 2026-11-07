#!/usr/bin/env python3
"""Inventory heartbeats retain pressure evidence before a runner disappears."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SOURCE = Path(os.environ.get("WFSIM_INVENTORY_PHASE_SOURCE", Path(__file__).with_name("inventory_phase.py")))
spec = importlib.util.spec_from_file_location("inventory_phase", SOURCE)
phase = importlib.util.module_from_spec(spec)
spec.loader.exec_module(phase)


class Measurements(unittest.TestCase):
    def test_linux_pressure(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for path, body in {
                "proc/42/status": "VmRSS: 4096 kB\nVmHWM: 8192 kB\nVmSwap: 512 kB\n",
                "proc/42/cgroup": "0::/job\n",
                "proc/meminfo": "MemAvailable: 16384 kB\n",
                "proc/pressure/memory": "full avg10=2.00 total=1234\n",
                "proc/pressure/io": "some avg10=1.00 total=5678\n",
                "cgroup/job/memory.current": "9999\n",
                "cgroup/job/memory.events": "high 3\noom 2\noom_kill 1\n",
            }.items():
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(body)
            reading = phase.resources(42, root / "proc", root / "cgroup")
            self.assertEqual(reading["process_rss_bytes"], 4194304)
            self.assertEqual(reading["process_peak_rss_bytes"], 8388608)
            self.assertEqual(reading["process_swap_bytes"], 524288)
            self.assertEqual(reading["memory_available_bytes"], 16777216)
            self.assertEqual(reading["cgroup_memory_bytes"], 9999)
            self.assertEqual(reading["cgroup_memory_events"]["oom_kill"], 1)
            self.assertIn("avg10=2.00", reading["memory_pressure"])
            self.assertIn("avg10=1.00", reading["io_pressure"])
            self.assertGreater(reading["scratch_free_bytes"], 0)
            self.assertNotIn("process_rss_bytes", phase.resources(99, root / "proc", root / "cgroup"))

    def test_real_child_exit_and_early_evidence(self):
        result = subprocess.run([sys.executable, str(SOURCE), "fixture", "--", sys.executable,
                                 "-c", "import time;time.sleep(.1);raise SystemExit(7)"],
                                capture_output=True, text=True, timeout=5)
        self.assertEqual(result.returncode, 7)
        rows = [json.loads(line) for line in result.stderr.splitlines()]
        self.assertEqual(rows[0]["state"], "start")
        self.assertEqual(rows[-1]["exit_code"], 7)
        early = next(row for row in rows if row["state"] == "progress")
        self.assertLess(early["wall_seconds"], 2)
        self.assertGreater(early["resources"]["scratch_free_bytes"], 0)
        self.assertGreater(early["pid"], 0)


if __name__ == "__main__":
    unittest.main()
