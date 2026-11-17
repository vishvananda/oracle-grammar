from contextlib import closing
import json
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import unittest

import retire_observe as observe
from retire_queue import Queue


class ObserveTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root/'.cache').mkdir()
        (self.root/'config.json').write_text(json.dumps({'main_repo': str(self.root)}))
        self.queue = Queue(self.root/'queue.sqlite')
        self.queue.add_checkpoint('base', 'proof')
        self.queue.add_jobs([{'id': 'one', 'kind': 'author', 'rules': ['secret card text']},
                             {'id': 'two', 'kind': 'mechanical', 'rules': ['rule-two']}])

    def test_counts_retries_cost_privacy_and_unknowns(self):
        first = self.queue.claim('worker', kinds=['author'])
        usage = {'status': 'recorded', 'model': 'terra', 'elapsed_seconds': 10.5,
                 'estimated_usd': .25, 'tokens': {'input_tokens': 100, 'cached_input_tokens': 90,
                 'output_tokens': 5, 'reasoning_output_tokens': 3}}
        self.queue.finish(first['attempt_id'], 'needs-review', {'usage': usage, 'run': '/secret/path'},
                          {'secret card text': {'status': 'needs-review', 'reason': 'card secret', 'evidence': 'x'}})
        with closing(sqlite3.connect(self.root/'queue.sqlite', isolation_level=None)) as db:
            db.execute("INSERT INTO events(kind,data) VALUES ('retry-tainted', ?)",
                       (json.dumps({'attempt_id': first['attempt_id']}),))
        self.queue.claim('other', kinds=['mechanical'])
        snapshot = observe.snapshot(self.root)
        self.assertEqual(sum(row['count'] for row in snapshot['jobs']), 2)
        self.assertEqual(sum(row['count'] for row in snapshot['rules']), 2)
        tainted = next(row for row in snapshot['attempts'] if row['cohort'] == 'retry-tainted')
        self.assertEqual(tainted['known_estimated_usd'], .25)
        self.assertEqual(tainted['model_wall_median_seconds'], 10.5)
        self.assertEqual(tainted['claim_finish_samples'], 1)
        self.assertEqual(tainted['tokens']['reasoning_output_tokens'], 3)
        running = next(row for row in snapshot['attempts'] if row['status'] == 'running')
        self.assertEqual(running['estimated_usd'], 0)
        self.assertEqual(running['usage_status'], {'not-applicable': 1})
        self.assertEqual(running['model'], 'none')
        encoded = json.dumps(snapshot)
        self.assertNotIn('secret', encoded)
        self.assertNotIn('/secret/path', encoded)
        self.assertEqual(snapshot['failure_categories'][0]['rules'], 1)
        self.assertEqual(snapshot['cost_total']['known_estimated_usd'], .25)
        self.assertEqual(snapshot['cost_total']['estimated_usd'], .25)

    def test_verified_unstarted_is_zero_but_unknown_author_is_not(self):
        first = self.queue.claim('worker', kinds=['author'])
        self.queue.finish(first['attempt_id'], 'failed', 'prep failure',
                          {'secret card text': {'status': 'needs-review', 'reason': 'prep', 'evidence': 'x'}})
        terminal = {'unit': 'service', 'invocation': 'id', 'verification': {'inactive': True}}
        with closing(sqlite3.connect(self.root/'queue.sqlite', isolation_level=None)) as db:
            db.execute("INSERT INTO events(kind,data) VALUES ('retry-unstarted', ?)",
                       (json.dumps({'attempt_id': first['attempt_id'], 'evidence': {
                           'launch_not_attempted': True, 'controller_terminal': terminal}}),))
        snapshot = observe.snapshot(self.root)
        row = snapshot['attempts'][0]
        self.assertEqual((row['model'], row['estimated_usd']), ('none', 0))
        self.assertEqual(snapshot['cost_total']['estimated_usd'], 0)
        with closing(sqlite3.connect(self.root/'queue.sqlite', isolation_level=None)) as db:
            db.execute("UPDATE events SET data=? WHERE kind='retry-unstarted'",
                       (json.dumps({'attempt_id': first['attempt_id'], 'evidence': {
                           'launch_not_attempted': True, 'controller_terminal': {}}}),))
        self.assertIsNone(observe.snapshot(self.root)['cost_total']['estimated_usd'])

    def test_running_author_cost_is_unknown(self):
        self.queue.claim('worker', kinds=['author'])
        row = observe.snapshot(self.root)['attempts'][0]
        self.assertIsNone(row['estimated_usd'])
        self.assertEqual(row['unknown_cost'], 1)

    def test_partial_usage_is_not_complete_cost(self):
        claim = self.queue.claim('worker', kinds=['author'])
        self.queue.finish(claim['attempt_id'], 'provisional-pass',
                          {'usage': {'status': 'partial', 'model': 'terra', 'estimated_usd': .1}},
                          {'secret card text': {'status': 'pass', 'evidence': 'x'}})
        result = observe.snapshot(self.root)
        self.assertEqual(result['cost_total']['known_estimated_usd'], .1)
        self.assertIsNone(result['cost_total']['estimated_usd'])
        self.assertEqual(result['cost_total']['unknown_attempts'], 1)

    def test_superseded_parent_attempt_cost_counted_once(self):
        claim = self.queue.claim('worker', kinds=['author'])
        self.queue.finish(claim['attempt_id'], 'needs-review',
                          {'usage': {'status': 'recorded', 'model': 'terra', 'estimated_usd': .2}},
                          {'secret card text': {'status': 'needs-review', 'reason': 'x', 'evidence': 'x'}})
        with closing(sqlite3.connect(self.root/'queue.sqlite', isolation_level=None)) as db:
            db.execute("UPDATE jobs SET status='superseded' WHERE id='one'")
            db.execute("INSERT INTO jobs(id,payload,status) VALUES (?,?,?)",
                       ('child', json.dumps({'id': 'child', 'kind': 'author', 'rules': ['secret card text']}),
                        'needs-review'))
            db.execute("UPDATE rules SET job_id='child' WHERE job_id='one'")
        result = observe.snapshot(self.root)
        self.assertEqual(sum(row['count'] for row in result['jobs']), 2)
        self.assertEqual(sum(row['count'] for row in result['rules']), 2)
        self.assertEqual(result['cost_total']['known_estimated_usd'], .2)
        self.assertEqual(sum(row['attempts'] for row in result['attempts']), 1)
        self.assertEqual(result['attempts'][0]['kind'], 'author')

    def test_cli_output_constrained_to_cache(self):
        script = Path(observe.__file__)
        target = self.root/'.cache/observed.json'
        subprocess.run([sys.executable, str(script), str(self.root), '--output', str(target)], check=True)
        self.assertIn('jobs', json.loads(target.read_text()))
        rejected = subprocess.run([sys.executable, str(script), str(self.root), '--output', str(self.root/'bad')],
                                  capture_output=True)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertFalse((self.root/'bad').exists())


if __name__ == '__main__':
    unittest.main()
