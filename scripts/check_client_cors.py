"""Check additive CLI CORS updates without contacting a bucket."""
import importlib.util
import io
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.error
import xml.etree.ElementTree as ET

location = Path(os.environ.get("WFSIM_CORS_HELPER", Path(__file__).with_name("client_cors.py")))
spec = importlib.util.spec_from_file_location("client_cors", location)
cors = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cors)
BASE = b'<CORSConfiguration><CORSRule><ID>old</ID><AllowedOrigin>https://other.example</AllowedOrigin><AllowedMethod>PUT</AllowedMethod><AllowedHeader>x-old</AllowedHeader></CORSRule><ResponseVary>true</ResponseVary></CORSConfiguration>'


class CorsChecks(unittest.TestCase):
    def test_append_preserves_rules_and_top_options(self):
        after = cors.plan(BASE)
        self.assertTrue(cors.preserves(BASE, after))
        root = cors.tree(after)
        self.assertEqual(len(root), 3)
        self.assertEqual(cors.fingerprint(root[0]), cors.fingerprint(cors.tree(BASE)[0]))
        self.assertEqual(root[-1].tag, "ResponseVary")
        self.assertEqual(root[-1].text, "true")
        added = {node.tag: node.text for node in root[1]}
        self.assertEqual(added["AllowedOrigin"], "https://wfsim.app")
        self.assertEqual(added["AllowedMethod"], "GET")
        self.assertNotIn("AllowedHeader", added)

    def test_existing_access_is_unchanged(self):
        for origin in (cors.ORIGIN, "*"):
            raw = f'<CORSConfiguration><CORSRule><AllowedOrigin>{origin}</AllowedOrigin><AllowedMethod>GET</AllowedMethod></CORSRule></CORSConfiguration>'.encode()
            self.assertEqual(cors.plan(raw), raw)

    def test_namespace_and_id_collision(self):
        raw = BASE.replace(b"<CORSConfiguration>", b'<CORSConfiguration xmlns="urn:cos">')
        self.assertTrue(cors.preserves(raw, cors.plan(raw)))
        with self.assertRaises(ValueError):
            cors.plan(BASE.replace(b">old<", b">wfsim-cli-web-get<"))

    def test_dry_run_never_puts(self):
        with tempfile.TemporaryDirectory() as d, patch.object(cors, "request", return_value=BASE) as req:
            result = cors.ensure({}, Path(d))
            self.assertFalse(result["applied"])
            self.assertEqual([c.args[1] for c in req.call_args_list], ["GET"])

    def test_apply_checks_race_and_readback(self):
        after = cors.plan(BASE)
        with tempfile.TemporaryDirectory() as d, patch.object(cors, "request", side_effect=[BASE, BASE, b"", after]) as req, patch.object(cors, "probe"):
            self.assertTrue(cors.ensure({}, Path(d), True)["applied"])
            self.assertEqual([c.args[1] for c in req.call_args_list], ["GET", "GET", "PUT", "GET"])
        with tempfile.TemporaryDirectory() as d, patch.object(cors, "request", side_effect=[BASE, b"<CORSConfiguration/>"]) as req:
            with self.assertRaisesRegex(ValueError, "changed since"):
                cors.ensure({}, Path(d), True)
            self.assertNotIn("PUT", [c.args[1] for c in req.call_args_list])

    def test_existing_access_needs_no_put(self):
        raw = cors.plan(BASE)
        with tempfile.TemporaryDirectory() as d, patch.object(cors, "request", return_value=raw) as req, patch.object(cors, "probe"):
            cors.ensure({}, Path(d), True)
            self.assertNotIn("PUT", [c.args[1] for c in req.call_args_list])

    def test_first_publication_defers_manifest_probe(self):
        after = cors.plan(b"<CORSConfiguration/>")
        with tempfile.TemporaryDirectory() as d, patch.object(cors, "request", side_effect=[None, None, b"", after]), patch.object(cors, "probe", side_effect=FileNotFoundError) as probe:
            result = cors.ensure({}, Path(d), True, verify_manifest=False)
            self.assertTrue(result["applied"])
            self.assertFalse(result["manifest_verified"])
            probe.assert_not_called()
        with tempfile.TemporaryDirectory() as d, patch.object(cors, "request", side_effect=[None, None, b"", after]), patch.object(cors, "probe", side_effect=FileNotFoundError):
            with self.assertRaises(FileNotFoundError):
                cors.ensure({}, Path(d), True)

    def test_readback_and_blind_replacement_are_refused(self):
        replacement = cors.plan(b"<CORSConfiguration/>")
        with tempfile.TemporaryDirectory() as d, patch.object(cors, "request", side_effect=[BASE, BASE, b"", replacement]), patch.object(cors, "probe"):
            with self.assertRaisesRegex(ValueError, "readback"):
                cors.ensure({}, Path(d), True)
        with tempfile.TemporaryDirectory() as d, patch.object(cors, "plan", return_value=replacement), patch.object(cors, "request", return_value=BASE) as req:
            with self.assertRaisesRegex(ValueError, "preserve"):
                cors.ensure({}, Path(d), True)
            self.assertNotIn("PUT", [c.args[1] for c in req.call_args_list])

    def test_permission_and_unknown_404_fail_closed(self):
        for status, code in [(403, "AccessDenied"), (404, "NoSuchBucket"), (404, "Unknown")]:
            error = urllib.error.HTTPError("https://test/?cors", status, code, {}, io.BytesIO(f"<Error><Code>{code}</Code></Error>".encode()))
            with patch.object(cors.cos, "_open", side_effect=error), patch.object(cors.urllib.request, "urlopen") as put:
                with self.assertRaises(RuntimeError):
                    cors.request({"bucket": "test", "region": "test", "secret_id": "test", "secret_key": "test"}, "GET")
                put.assert_not_called()

    def test_readback_cannot_broaden_the_new_rule(self):
        after = cors.plan(BASE)
        broad = after.replace(b"<AllowedOrigin>https://wfsim.app</AllowedOrigin>", b"<AllowedOrigin>*</AllowedOrigin>")
        broad = broad.replace(b"<AllowedMethod>GET</AllowedMethod>", b"<AllowedMethod>GET</AllowedMethod><AllowedMethod>POST</AllowedMethod>")
        with tempfile.TemporaryDirectory() as d, patch.object(cors, "request", side_effect=[BASE, BASE, b"", broad]), patch.object(cors, "probe") as probe:
            with self.assertRaisesRegex(ValueError, "exact proposal"):
                cors.ensure({}, Path(d), True)
            probe.assert_not_called()

    def test_only_known_missing_configuration_allows_creation(self):
        error = urllib.error.HTTPError("https://test/?cors", 404, "missing", {}, io.BytesIO(b"<Error><Code>NoSuchCORSConfiguration</Code></Error>"))
        with patch.object(cors.cos, "_open", side_effect=error):
            self.assertIsNone(cors.request({"bucket": "test", "region": "test", "secret_id": "test", "secret_key": "test"}, "GET"))

    def test_workflow_keeps_protected_cors_only_path(self):
        workflow = Path(".github/workflows/client-release.yml").read_text()
        self.assertIn("cors_only:", workflow)
        self.assertIn("expected_sha:", workflow)
        self.assertIn('test "$EXPECTED_SHA" = "$GITHUB_SHA"', workflow)
        self.assertIn('node scripts/ci_runs.mjs ship "$EXPECTED_SHA"', workflow)
        self.assertIn('test "$(git rev-parse HEAD)" = "$EXPECTED_SHA"', workflow)
        self.assertIn('test -n "$proof"', workflow)
        self.assertIn("python3 scripts/client_cors.py --apply", workflow)
        self.assertIn("if: ${{ !inputs.cors_only }}", workflow)
        self.assertIn("inputs.cors_only && github.ref == 'refs/heads/main'", workflow)
        self.assertGreaterEqual(workflow.count("environment: release"), 2)
        job = workflow.split("  cors:\n", 1)[1].split("\n  version:", 1)[0]
        for forbidden in ("cargo ", "gh release", "npm publish", "scripts/release_client.py"):
            self.assertNotIn(forbidden, job)
        publish = workflow.split("\n  publish:", 1)[1]
        self.assertNotIn("scripts/client_cors.py", publish)
        self.assertIn("python3 scripts/release_client.py target/client-release", publish)


if __name__ == "__main__":
    unittest.main()
