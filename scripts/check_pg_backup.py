"""A failed dump/upload/readback never publishes a recovery manifest."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('pg_backup', Path(__file__).with_name('pg_backup.py'))
backup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(backup)


class Store:
    def __init__(self, fail=False, corrupt=False):
        self.objects = {}
        self.fail = fail
        self.corrupt = corrupt

    def put(self, key, data):
        if self.fail:
            raise OSError('upload failed')
        self.objects[key] = data

    def digest(self, key):
        data = self.objects[key] + (b'corrupt' if self.corrupt else b'')
        return len(data), hashlib.sha256(data).hexdigest()


class Dump:
    def __init__(self, data=b'0123456789abcdefghi', code=0):
        self.stdout = io.BytesIO(data)
        self.code = code

    def wait(self, timeout=None):
        return self.code

    def poll(self):
        return self.code


class BackupCheck(unittest.TestCase):
    def run_backup(self, store, code=0):
        with patch.object(backup.subprocess, 'Popen', side_effect=lambda *a, **kw: Dump(code=code)):
            return backup.backup(store, backup.ROOT + '/maintenance/check', chunk_bytes=8)

    def test_parts_restore_exact_original_bytes(self):
        store = Store()
        receipt = self.run_backup(store)
        manifest = json.loads(store.objects[receipt['object']])
        self.assertEqual([x['database'] for x in manifest['databases']], list(backup.DATABASES))
        self.assertFalse(manifest['verification']['isolated_restore'])
        for entry in manifest['databases']:
            data = b''.join(store.objects[p['object']] for p in entry['parts'])
            self.assertEqual(data, b'0123456789abcdefghi')
            self.assertEqual(hashlib.sha256(data).hexdigest(), entry['sha256'])
            self.assertTrue(all(p['bytes'] <= 8 for p in entry['parts']))
        self.assertTrue(list(store.objects)[-1].endswith('/manifest.json'))

    def test_dump_upload_and_corrupt_readback_cannot_commit(self):
        for store, code in [(Store(), 1), (Store(fail=True), 0), (Store(corrupt=True), 0)]:
            with self.subTest(code=code, fail=store.fail, corrupt=store.corrupt):
                with self.assertRaises((RuntimeError, OSError)):
                    self.run_backup(store, code)
                self.assertFalse(any(key.endswith('/manifest.json') for key in store.objects))

    def test_public_bucket_rejected(self):
        for prefix in ['r2:wfsim-public/backups', backup.ROOT + '/../wfsim-public', backup.ROOT + '/daily//x']:
            with self.assertRaises(ValueError):
                backup.backup(Store(), prefix)


if __name__ == '__main__':
    unittest.main()
