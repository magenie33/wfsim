#!/usr/bin/env python3
"""Stale outputs and failed observations cannot look like a healthy service."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
import pg_health as health


class Health(unittest.TestCase):
    def fixture(self):
        now = 1800000000
        return now, {name: {'ok': True, 'value': value} for name, value in {
            'database': {'snapshot': {'daemon_at': health.instant(now), 'engine': 'engine', 'settle_queue_rows': 0},
                         'inventory': {'started_at': health.instant(now), 'engine': 'engine'}, 'sessions': []},
            'board': {'newest_epoch': now},
            'services': {'wfsim-factoryd.service': {'ActiveState': 'active'},
                         'wfsim-task-archive.timer': {'ActiveState': 'active'},
                         'wfsim-task-archive.service': {'ExecMainStatus': '0'}},
            'resources': {'disk_free_bytes': 20 * 1024**3}}.items()}

    def test_active_services_do_not_hide_stale_outputs_or_lock_failure(self):
        now, probes = self.fixture()
        self.assertEqual(health.assess(probes, now=now)['status'], 'healthy')
        probes['board']['value']['newest_epoch'] -= 86400
        db = probes['database']['value']
        db['inventory']['started_at'] = health.instant(now - 14400)
        db['sessions'] = [{'state': 'idle in transaction', 'transaction_seconds': 400, 'idle_seconds': 400}, {'blockers': [123]}]
        report = health.assess(probes, now=now)
        self.assertEqual(set(report['alerts']), {'board_stale', 'inventory_stale', 'idle_transaction', 'database_lock_wait'})
        self.assertEqual(report['event'], 'incident')
        repeated = health.assess(probes, report, now + 60)
        self.assertEqual(repeated['incident_id'], report['incident_id'])
        self.assertEqual(repeated['event'], 'sample')
        recovered = health.assess(self.fixture()[1], repeated, now + 60)
        self.assertEqual(recovered['event'], 'recovered')
        self.assertEqual(recovered['incident_id'], report['incident_id'])

    def test_long_capture_with_recent_activity_is_not_an_idle_stall(self):
        now, probes = self.fixture()
        probes['database']['value']['sessions'] = [{'state': 'idle in transaction', 'transaction_seconds': 1800, 'idle_seconds': 2}]
        self.assertNotIn('idle_transaction', health.assess(probes, now=now)['alerts'])

    def test_unknown_observation_is_failure_and_capture_age_wins(self):
        now, probes = self.fixture()
        for name in probes:
            bad = copy.deepcopy(probes); bad[name] = {'ok': False, 'error': 'timeout'}
            self.assertIn('probe_failed:' + name, health.assess(bad, now=now)['alerts'])
        probes['database']['value']['inventory'].update(at=health.instant(now), started_at=health.instant(now - 14400), engine='old')
        self.assertIn('inventory_stale', health.assess(probes, now=now)['alerts'])
        self.assertIn('inventory_engine_mismatch', health.assess(probes, now=now)['alerts'])

    def test_settlement_backlog_recovery_and_bounded_history(self):
        now, probes = self.fixture()
        probes['database']['value']['snapshot']['settle_queue_rows'] = 3
        first = health.assess(probes, now=now)
        self.assertNotIn('settlement_backlog', first['alerts'])
        later = health.assess(probes, first, now + 601)
        self.assertIn('settlement_backlog', later['alerts'])
        probes['database']['value']['snapshot']['settle_queue_rows'] = 0
        self.assertIsNone(health.assess(probes, later, now + 602)['settlement_backlog_since'])
        with tempfile.TemporaryDirectory() as path:
            directory = Path(path)
            for i in range(20):
                health.persist(directory, {'i': i}, limit=50)
            self.assertEqual(json.loads((directory / 'status.json').read_text()), {'i': 19})
            self.assertLessEqual(sum(p.stat().st_size for p in directory.glob('events*')), 150)
            self.assertFalse((directory / 'status.partial').exists())


if __name__ == '__main__':
    unittest.main()
