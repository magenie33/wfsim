# SPDX-License-Identifier: AGPL-3.0-or-later
"""PGO admission refuses numerical changes and keeps a faster baseline."""
import argparse
import copy
import json
import os
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
        self.assertEqual(p.flags(p.WINDOWS_X64, profile, target_cpu=p.V3),
                         ["-Ctarget-feature=+crt-static", "-Ctarget-cpu=x86-64-v3", profile])

    def test_cross_target_cannot_train_before_any_build(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(p, "host", return_value="host-target"):
            args = argparse.Namespace(target="other-target", windows_v3=False,
                                      work_dir=Path(tmp), output=Path(tmp) / "out")
            with self.assertRaisesRegex(ValueError, "target host"):
                p.build(args)
            self.assertEqual(list(Path(tmp).iterdir()), [])

    def test_v3_host_check_refuses_other_targets_and_missing_features(self):
        required = {"avx2", "bmi2", "fma", "xsave", "movbe"}
        with patch.object(p.sys, "platform", "win32"), patch.object(p.C, "sizeof", return_value=8), \
                patch.object(p, "target_features", side_effect=[required, required - {"movbe"}]) as probe:
            with self.assertRaisesRegex(ValueError, "Windows x64"):
                p.windows_v3("x86_64-unknown-linux-gnu")
            probe.assert_not_called()
            with self.assertRaisesRegex(ValueError, "missing CPU/OS features: movbe"):
                p.windows_v3(p.WINDOWS_X64)
        with patch.object(p.sys, "platform", "win32"), patch.object(p.C, "sizeof", return_value=8), \
                patch.object(p, "target_features", side_effect=[required, required | {"avx512f"}]):
            self.assertEqual(p.windows_v3(p.WINDOWS_X64)["required_target_features"], sorted(required))

    def test_build_stages_and_receipt_preserve_default_and_share_opt_in_profile(self):
        for tuned in (False, True):
            with self.subTest(windows_v3=tuned), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                args = argparse.Namespace(target=p.WINDOWS_X64, windows_v3=tuned, work_dir=root / "work dir",
                                          output=root / "selected.dll")
                bin_dir = root / "lib" / "rustlib" / p.WINDOWS_X64 / "bin"
                bin_dir.mkdir(parents=True)
                profdata = bin_dir / ("llvm-profdata.exe" if os.name == "nt" else "llvm-profdata")
                profdata.touch()
                builds = {}
                expected = {"api": {"sample": {"ok": True}}, "rows": {"row": {"score": 1.25}}}

                def run(command, *, cwd=None, env=None, stdout=None, check):
                    if command[0] == "cargo":
                        target_dir = Path(env["CARGO_TARGET_DIR"])
                        stage = target_dir.name
                        builds[stage] = env
                        pack = target_dir / "packs"
                        pack.mkdir(parents=True)
                        (pack / "data.json").write_bytes(b"fixed data")
                        library = target_dir / args.target / "release" / "wfsim_native.dll"
                        library.parent.mkdir(parents=True)
                        library.write_bytes(stage.encode())
                        stdout.write(json.dumps({"reason": "build-script-executed", "package_id": "wfsim-engine",
                                                 "out_dir": str(target_dir)}) + "\n")
                    elif command[0] == str(profdata):
                        Path(command[3]).write_bytes(b"merged profile")
                    else:
                        mode = command[command.index("--mode") + 1]
                        if mode == "train":
                            Path(env["LLVM_PROFILE_FILE"]).with_name("dummy.profraw").touch()
                        result = ({"row": {"ms": [10, 10, 10], "result": expected["rows"]["row"]}}
                                  if mode == "bench" else expected)
                        p.write(Path(command[-1]), result)

                profile = {"target_cpu": p.V3, "lto": "thin", "codegen_units": 1,
                           "compatibility": "Windows x64 CPUs and OS support for x86-64-v3 only"}
                with patch.dict(p.os.environ, {}, clear=True), patch.object(p, "host", return_value=args.target), \
                        patch.object(p, "windows_v3", return_value=profile) as compat, \
                        patch.object(p, "source_hash", return_value="fixed-source"), \
                        patch.object(p.subprocess, "check_output", side_effect=["test rustc", str(root)]), \
                        patch.object(p.subprocess, "run", side_effect=run), patch("builtins.print"):
                    p.build(args)
                if tuned:
                    compat.assert_called_once_with(args.target)
                else:
                    compat.assert_not_called()
                receipt = json.loads(args.output.with_suffix(".pgo.json").read_bytes())
                self.assertEqual(receipt.get("build_tuning"), profile if tuned else None)
                self.assertEqual(args.output.read_bytes(), b"baseline")
                base = ["-Ctarget-feature=+crt-static"] + (["-Ctarget-cpu=x86-64-v3"] if tuned else [])
                self.assertEqual(list(builds), ["baseline", "generate", "pgo"])
                self.assertEqual(receipt["build_flags"]["baseline"], base)
                for stage, env in builds.items():
                    self.assertEqual(env.get("CARGO_PROFILE_RELEASE_LTO"), "thin" if tuned else None)
                    self.assertEqual(env.get("CARGO_PROFILE_RELEASE_CODEGEN_UNITS"), "1" if tuned else None)
                    self.assertEqual(env["CARGO_ENCODED_RUSTFLAGS"].split("\x1f"), receipt["build_flags"][stage])
                    self.assertEqual(receipt["build_flags"][stage][:len(base)], base)
                    self.assertNotIn("WFSIM_GAS_OFF", env)
                self.assertIn("-Cprofile-generate=", receipt["build_flags"]["generate"][-1])
                self.assertIn("-Cprofile-use=", receipt["build_flags"]["pgo"][-2])

    def test_cached_baseline_snapshots_outputs_and_baseline_only_verifies(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            cache = root / "cargo-cache"
            args = argparse.Namespace(target="x86_64-apple-darwin", windows_v3=False,
                                      cache_dir=cache, baseline_only=True,
                                      work_dir=root / "work", output=root / "libwfsim_native.dylib")
            expected = {"api": {"sample": {"ok": True}}, "rows": {"row": {"score": 2.5}}}
            exercised_libs = []

            def run(command, *, cwd=None, env=None, stdout=None, check):
                if command[0] == "cargo":
                    target_dir = Path(env["CARGO_TARGET_DIR"])
                    self.assertEqual(target_dir, cache.resolve())
                    pack = target_dir / "packs"
                    pack.mkdir(parents=True, exist_ok=True)
                    (pack / "data.json").write_bytes(b"darwin data")
                    library = target_dir / args.target / "release" / "libwfsim_native.dylib"
                    library.parent.mkdir(parents=True, exist_ok=True)
                    library.write_bytes(b"darwin baseline")
                    stdout.write(json.dumps({"reason": "build-script-executed", "package_id": "wfsim-engine",
                                             "out_dir": str(target_dir)}) + "\n")
                else:
                    self.assertEqual(command[command.index("--mode") + 1], "verify")
                    lib_path = Path(command[command.index("--library") + 1])
                    exercised_libs.append(lib_path)
                    # Mutating the shared cache after compilation must not affect the snapshotted library.
                    (cache / args.target / "release" / "libwfsim_native.dylib").write_bytes(b"mutated cache")
                    p.write(Path(command[-1]), expected)

            with patch.dict(p.os.environ, {}, clear=True), patch.object(p, "host", return_value=args.target), \
                    patch.object(p, "source_hash", return_value="fixed-source"), \
                    patch.object(p.subprocess, "check_output", return_value="test rustc"), \
                    patch.object(p.subprocess, "run", side_effect=run), patch("builtins.print"):
                p.build(args)
            self.assertEqual(len(exercised_libs), 1)
            self.assertNotEqual(exercised_libs[0].parent, cache / args.target / "release")
            self.assertEqual(args.output.read_bytes(), b"darwin baseline")
            receipt = json.loads(args.output.with_suffix(".pgo.json").read_bytes())
            self.assertTrue(receipt["baseline_only"])
            self.assertEqual(receipt["selected"], "baseline")
            self.assertIsNone(receipt["pgo_sha256"])
            self.assertEqual(receipt["selected_sha256"], p.sha(args.output))
            self.assertEqual(receipt["verification_sha256"], p.sha(args.output.with_suffix(".verify.json")))


if __name__ == "__main__":
    unittest.main()
