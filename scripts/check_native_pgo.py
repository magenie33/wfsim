# SPDX-License-Identifier: AGPL-3.0-or-later
"""PGO admission refuses numerical changes and keeps a faster baseline."""
import argparse
import copy
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import native_pgo as p


class PgoAdmission(unittest.TestCase):
    def test_complete_response_changes_are_refused(self):
        before = {"score": 1.25, "work": 400000, "counts": {"gas": 123, "hits": 7}, "extra": [1, "a"]}
        p.exact(before, copy.deepcopy(before))
        for key in ["score", "work", "counts", "extra"]:
            after = copy.deepcopy(before)
            after[key] = None
            with self.assertRaises(ValueError):
                p.exact(before, after)
        with self.assertRaises(ValueError):
            p.exact({"n": 1.0}, {"n": 1.0000000000000002})
        with self.assertRaises(ValueError):
            p.exact({"n": 0.0}, {"n": -0.0})
        p.exact({"n": 1}, {"n": 1.0})

    def test_speed_gain_and_individual_regression_both_matter(self):
        baseline = {"a": [10, 10, 10, 10], "b": [20, 20, 20, 20], "c": [30, 30, 30, 30]}
        faster = {k: [v * 0.8 for v in values] for k, values in baseline.items()}
        self.assertEqual(p.select(baseline, faster)[0], "pgo")
        self.assertEqual(p.select(baseline, baseline)[0], "baseline")
        faster["c"] = [32, 32, 32, 32]
        self.assertEqual(p.select(baseline, faster)[0], "baseline")
        with self.assertRaises(ValueError):
            p.select(baseline, {"a": [1]})

    def test_target_options_survive_profile_paths_with_spaces(self):
        profile = "-Cprofile-use=/a path/merged.profdata"
        self.assertEqual(p.flags("aarch64-apple-darwin", profile), ["-Cstrip=none", profile])
        self.assertEqual(p.flags("x86_64-pc-windows-msvc", profile), ["-Ctarget-feature=+crt-static", profile])

    def test_cross_target_cannot_train_before_any_build(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(p, "host", return_value="host-target"):
            args = argparse.Namespace(target="other-target", work_dir=Path(tmp), output=Path(tmp) / "out")
            with self.assertRaisesRegex(ValueError, "target host"):
                p.build(args)
            self.assertEqual(list(Path(tmp).iterdir()), [])


if __name__ == "__main__":
    unittest.main()
