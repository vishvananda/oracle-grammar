import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

import retire_observe
import retire_usage_refresh as refresh
from retire_queue import Queue
from test_retire_metrics import ESTIMATOR


class UsageRefreshTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root/'.cache').mkdir()
        estimator = self.root/'estimator.py'
        estimator.write_text(ESTIMATOR)
        self.sessions = self.root/'sessions'
        config = {'main_repo': str(self.root), 'metrics': {
            'usage_estimator': str(estimator), 'estimator_sha256': hashlib.sha256(estimator.read_bytes()).hexdigest(),
            'sessions_root': str(self.sessions), 'pricing': {'version': 'test', 'source': 'test', 'basis': 'api',
                                                            'rates': {'terra': [1, .05, 1.25, 5]}}}}
        (self.root/'config.json').write_text(json.dumps(config))
        self.queue = Queue(self.root/'queue.sqlite')
        self.queue.add_checkpoint('base', 'proof')
        self.queue.add_jobs([{'id': 'author', 'kind': 'author', 'rules': ['r1']}])
        lease = self.root/'leases/author'
        lease.parent.mkdir()
        lease.touch()

    def attempt(self, thread, evidence):
        claim = self.queue.claim('worker')
        child = subprocess.Popen(['true'])
        child.wait()
        folder = self.root/'.cache/attempts'/claim['attempt_id']/'workers/author'
        folder.mkdir(parents=True)
        receipt = {'task': 'author', 'pid': child.pid, 'model': 'terra', 'effort': 'high',
                   'started_unix': 1767225600, 'finished_unix': 1767225610,
                   'wall_seconds': 10, 'status': 'completed'}
        (folder/'receipt.json').write_text(json.dumps(receipt))
        events = [{'type': 'thread.started', 'thread_id': thread}]
        (folder/'events.jsonl').write_text(''.join(json.dumps(e)+'\n' for e in events))
        self.queue.mark_started(claim['attempt_id'], pid=child.pid, receipt=str(folder/'receipt.json'))
        self.queue.finish(claim['attempt_id'], 'failed', evidence,
                          {'r1': {'status': 'needs-review', 'reason': 'missing final', 'evidence': 'x'}})
        return claim, folder

    def rollout(self, thread):
        folder = self.sessions/'2026/01/01'
        folder.mkdir(parents=True, exist_ok=True)
        (folder/f'rollout-{thread}.jsonl').write_text(json.dumps({
            'thread': thread, 'model': 'terra',
            'tokens': {'input_tokens': 1000, 'cached_input_tokens': 800, 'output_tokens': 100}}))

    def test_paid_missing_final_historical_and_current_are_counted_once(self):
        threads = ['12345678-1234-1234-1234-123456789abc', 'aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa']
        first, _ = self.attempt(threads[0], {'reason': 'missing structured final'})
        self.rollout(threads[0])
        original = self.queue.get_attempt(first['attempt_id'])
        self.queue.retry_failed('author', first['attempt_id'], 'verified', 1)
        second, folder = self.attempt(threads[1], {'reason': 'missing structured final'})
        self.rollout(threads[1])
        counts = refresh.refresh(self.root)
        self.assertEqual((counts['examined'], counts['written']), (2, 2))
        path = self.root/'.cache/usage-receipts'/f"{first['attempt_id']}.json"
        initial = path.read_bytes()
        self.assertEqual(refresh.refresh(self.root)['unchanged'], 2)
        self.assertEqual(path.read_bytes(), initial)
        self.assertEqual(self.queue.get_attempt(first['attempt_id']), original)
        observed = retire_observe.snapshot(self.root)
        self.assertEqual(sum(row['attempts'] for row in observed['attempts']), 2)
        self.assertAlmostEqual(observed['cost_total']['estimated_usd'], 2 * .00074)
        self.assertNotIn(str(self.root), json.dumps(observed))
        # A changed source invalidates the cached sidecar until refreshed.
        with (folder/'events.jsonl').open('a') as out:
            out.write('{}\n')
        observed = retire_observe.snapshot(self.root)
        self.assertIsNone(observed['cost_total']['estimated_usd'])
        self.assertEqual(observed['cost_total']['unknown_attempts'], 1)
        self.assertEqual(refresh.refresh(self.root)['written'], 1)
        self.assertEqual(retire_observe.snapshot(self.root)['cost_total']['unknown_attempts'], 0)
        self.assertNotEqual(first['attempt_id'], second['attempt_id'])

    def test_original_usage_takes_precedence_over_existing_sidecar(self):
        usage = {'status': 'recorded', 'model': 'terra', 'estimated_usd': .12}
        claim, _ = self.attempt('12345678-1234-1234-1234-123456789abc', {'usage': usage})
        self.assertEqual(refresh.refresh(self.root)['original'], 1)
        record = self.queue.get_attempt(claim['attempt_id'])
        import sqlite3
        db = sqlite3.connect(self.root/'queue.sqlite')
        try:
            started, result = db.execute('SELECT started,result FROM attempts WHERE id=?',
                                         (claim['attempt_id'],)).fetchone()
        finally:
            db.close()
        config = json.loads((self.root/'config.json').read_text())
        identity = refresh._inputs(self.root, claim['attempt_id'], record['job_id'], started, result, config)[2]
        folder = self.root/'.cache/usage-receipts'
        folder.mkdir(exist_ok=True)
        (folder/f"{claim['attempt_id']}.json").write_text(json.dumps(
            {'schema': 1, 'identity': identity, 'usage': {'status': 'recorded', 'model': 'terra', 'estimated_usd': .99}}))
        self.assertEqual(retire_observe.snapshot(self.root)['cost_total']['estimated_usd'], .12)

    def test_unassigned_thread_is_unknown_and_estimator_hash_is_pinned(self):
        _, folder = self.attempt('12345678-1234-1234-1234-123456789abc', 'failure')
        (folder/'events.jsonl').write_text('{}\n')
        self.assertEqual(refresh.refresh(self.root)['written'], 1)
        self.assertIsNone(retire_observe.snapshot(self.root)['cost_total']['estimated_usd'])
        (self.root/'estimator.py').write_text('modified')
        self.assertEqual(refresh.refresh(self.root)['unavailable'], 1)
        self.assertIsNone(retire_observe.snapshot(self.root)['cost_total']['estimated_usd'])


if __name__ == '__main__':
    unittest.main()
