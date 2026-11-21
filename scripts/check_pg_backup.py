"""Backup publication and retirement preserve complete recovery points."""
import datetime
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import unittest
from unittest.mock import call, patch

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

    def test_retention_keeps_three_weekly_points_as_days_roll_forward(self):
        today = datetime.datetime(2026, 10, 1, 4, tzinfo=datetime.timezone.utc)
        stored = set()
        completed = []
        for offset in range(65):
            if offset in (23, 24, 38, 54):
                continue
            now = today + datetime.timedelta(days=offset)
            published = now.strftime('%Y%m%dT031700Z') + '-00000001'
            completed.append((published, now.date()))
            stored.add(published)
            paths = (name + '/manifest.json' for name in stored)
            stored.difference_update(name for name, _ in backup.retention_plan(paths, published, now))
            daily_cutoff = now.date() - datetime.timedelta(days=6)
            daily = [name for name, day in completed if day >= daily_cutoff]
            weekly = {}
            for name, day in completed:
                if day < daily_cutoff:
                    weekly[day.isocalendar()[:2]] = name
            self.assertEqual(stored, set(daily + list(weekly.values())[-3:]), now.date())

    def test_prune_r2_only_after_confirming_current_manifest(self):
        now = datetime.datetime(2026, 11, 21, 4, tzinfo=datetime.timezone.utc)
        names = {
            'fresh': '20261121T031700Z-00000001',
            'recent_failed': '20261121T021700Z-00000001',
            'same_day': '20261121T011700Z-00000001',
            'older_failed': '20261112T031700Z-00000001',
            'expired': '20261015T031700Z-00000001',
            'newer': '20261121T041700Z-00000001',
        }
        paths = [name + '/wfsim/part-000000' for name in names.values()]
        paths += [names['expired'] + '/manifest.json', names['same_day'] + '/manifest.json',
                  'maintenance/manifest.json', names['older_failed'] + '/wfsim/manifest.json']
        def inventory():
            return json.dumps([{'Path': path} for path in paths])

        common = ['--s3-no-check-bucket']
        with patch.object(backup.subprocess, 'run') as run:
            run.return_value.stdout = inventory()
            with self.assertRaisesRegex(RuntimeError, 'New backup manifest missing'):
                backup.prune(names['fresh'], now)
            self.assertEqual(run.call_count, 1)
            paths.append(names['fresh'] + '/manifest.json')
            run.return_value.stdout = inventory()
            run.reset_mock()
            backup.prune(names['fresh'], now)
            expired = backup.DAILY + '/' + names['expired']
            duplicate = backup.DAILY + '/' + names['same_day']
            self.assertEqual(run.call_args_list[1:], [
                call(['rclone', 'deletefile', duplicate + '/manifest.json', *common], check=True),
                call(['rclone', 'purge', duplicate, *common], check=True),
                call(['rclone', 'purge', backup.DAILY + '/' + names['older_failed'], *common], check=True),
                call(['rclone', 'deletefile', expired + '/manifest.json', *common], check=True),
                call(['rclone', 'purge', expired, *common], check=True),
                call(['rclone', 'delete', backup.ROOT + '/daily/', '--min-age', '30d', *common], check=True),
            ])


if __name__ == '__main__':
    unittest.main()
