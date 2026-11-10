"""Task detail is pruned only after verified private archive publication."""
import copy
import hashlib
import json
import subprocess
from unittest import TestCase, main
from unittest.mock import patch

import pg_task_archive as archive


class Store:
    def __init__(self, fail=None):
        self.objects = {}
        self.fail = fail

    def put(self, key, data):
        if self.fail == 'upload' or (self.fail == 'manifest' and key.endswith('/manifest.json')):
            raise OSError('upload failed')
        self.objects[key] = data

    def digest(self, key):
        data = self.objects[key]
        if self.fail == 'readback' or (self.fail == 'manifest-readback' and key.endswith('/manifest.json')):
            data += b'corrupt'
        return len(data), hashlib.sha256(data).hexdigest()

    def read(self, key):
        return self.objects[key]


class Database:
    def __init__(self):
        self.rows = [{'question': {'id': 'q1', 'request': '{"seed":42,"text":"a\u0027b\\c"}',
                      'fact_engine': 'engine', 'canon': 'answer'},
                      'answers': [{'id': 1, 'work': 9007199254740993, 'credited': 1}],
                      'demands': [{'producer': 'sweep', 'state': 'served'}], 'engine_refusals': [], 'led_questions': []}]
        self.pruned = False

    def candidates(self, before, size):
        return copy.deepcopy(self.rows[:size])

    def prune(self, before, rows, receipt):
        assert receipt['object'].endswith('/manifest.json') and len(receipt['sha256']) == 64
        self.pruned = True
        return [row['question']['id'] for row in rows]


class TaskArchiveCheck(TestCase):
    def test_archive_cutoff_controls_the_candidate_plan(self):
        result = subprocess.CompletedProcess(['psql'], 0, stdout='[]', stderr='')
        with patch.object(archive.subprocess, 'run', return_value=result) as run:
            self.assertEqual(archive.Postgres().candidates('2026-11-02T00:00:00.000Z', 100), [])
        sql = run.call_args.kwargs['input']
        self.assertIn('REPEATABLE READ READ ONLY', sql)
        self.assertIn("plan_cache_mode='force_custom_plan'", sql)
        self.assertIn("factory_archive_candidates('2026-11-02T00:00:00.000Z',100)", sql)

    def test_exact_roundtrip_and_manifest_before_prune(self):
        store, database = Store(), Database()
        original = database.prune

        def prune(before, rows, receipt):
            self.assertEqual(archive.read_archive(store, receipt['object'])['tasks'], database.rows)
            return original(before, rows, receipt)

        database.prune = prune
        result = archive.archive_once(store, database, '2026-11-01T00:00:00.000Z')
        self.assertEqual((result['captured'], result['deleted']), (1, 1))
        self.assertTrue(database.pruned)
        self.assertEqual(archive.read_archive(store, result['manifest']['object'])['tasks'][0]['answers'][0]['work'], 9007199254740993)
        self.assertEqual(list(store.objects)[-1], result['manifest']['object'])

    def test_each_upload_or_readback_failure_retains_database_detail(self):
        for fail in ['upload', 'readback', 'manifest', 'manifest-readback']:
            with self.subTest(fail=fail):
                store, database = Store(fail), Database()
                with self.assertRaises((RuntimeError, OSError)):
                    archive.archive_once(store, database, 'before')
                self.assertFalse(database.pruned)

    def test_corruption_and_foreign_payload_are_rejected(self):
        for mutation in ['bytes', 'foreign']:
            store, database = Store(), Database()
            result = archive.archive_once(store, database, 'before')
            key = result['manifest']['object']
            manifest = json.loads(store.objects[key])
            if mutation == 'bytes':
                store.objects[manifest['payload']['object']] += b'corrupt'
            else:
                manifest['payload']['object'] = 'r2:wfsim-public/foreign'
                store.objects[key] = json.dumps(manifest).encode()
            with self.assertRaises((RuntimeError, ValueError)):
                archive.read_archive(store, key)

    def test_empty_and_oversized_tasks_are_not_pruned(self):
        database = Database()
        database.rows = []
        self.assertEqual(archive.archive_once(Store(), database, 'before')['captured'], 0)
        self.assertFalse(database.pruned)
        database = Database()
        database.rows[0]['question']['request'] = 'x' * (archive.MAX_BYTES + 1)
        with self.assertRaises(RuntimeError):
            archive.archive_once(Store(), database, 'before', size=1)
        self.assertFalse(database.pruned)

    def test_contract_and_additive_schema(self):
        self.assertEqual(archive.keep_days(), 7)
        sql = archive.schema_sql()
        self.assertIn('RETURN 0;', sql)
        self.assertIn('factory_forget_archived', sql)
        self.assertNotIn('DROP TABLE', sql)
        self.assertNotIn('DELETE FROM state_board_measurement_selection', sql)
        self.assertNotIn('DELETE FROM entity_compute_devices', sql)

    def test_one_verified_object_precedes_every_small_prune(self):
        store, database = Store(), Database()
        original = database.rows[0]
        database.rows = [copy.deepcopy(original) for _ in range(archive.CAPTURE_TASKS)]
        for number, row in enumerate(database.rows):
            row['question']['id'] = 'q' + str(number)
        chunks = []

        def prune(before, rows, receipt):
            stored = archive.read_archive(store, receipt['object'])['tasks']
            self.assertEqual(stored, database.rows)
            self.assertLessEqual(len(rows), archive.PRUNE_TASKS)
            chunks.extend(row['question']['id'] for row in rows)
            return [row['question']['id'] for row in rows]

        database.prune = prune
        result = archive.archive_once(store, database, 'before')
        self.assertEqual(result['captured'], archive.CAPTURE_TASKS)
        self.assertEqual(result['deleted'], archive.CAPTURE_TASKS)
        self.assertEqual(len(store.objects), 2)
        self.assertEqual(chunks, [r['question']['id'] for r in database.rows])
        self.assertGreater(result['prune_statements'], 1)

    def test_candidate_timeout_reduces_capture_without_pruning_unverified_data(self):
        store, database = Store(), Database()
        original = database.candidates
        sizes = []

        def candidates(before, size):
            sizes.append(size)
            if size > archive.PRUNE_TASKS:
                raise subprocess.CalledProcessError(1, 'psql', stderr='canceling statement due to statement timeout')
            return original(before, size)

        database.candidates = candidates
        result = archive.archive_once(store, database, 'before')
        self.assertGreater(sizes[0], archive.PRUNE_TASKS)
        self.assertEqual(sizes[-1], archive.PRUNE_TASKS)
        self.assertEqual(result['deleted'], 1)

    def test_failed_large_manifest_readback_prevents_all_prune_chunks(self):
        store, database = Store('manifest-readback'), Database()
        database.rows *= archive.CAPTURE_TASKS
        with self.assertRaises(RuntimeError):
            archive.archive_once(store, database, 'before')
        self.assertFalse(database.pruned)


if __name__ == '__main__':
    main()
