import json
from pathlib import Path
import tempfile
import unittest

import retire_metrics as metrics


THREAD = '12345678-1234-1234-1234-123456789abc'
OTHER = 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'
# Minimal estimator double exercises the adapter without depending on a machine path.
ESTIMATOR = '''\nimport json\nRATES = {}\nSOURCES = {}\ndef estimate(t, model, long_context=False):\n    r = RATES.get(model)\n    if r is None: return None\n    return ((t['input_tokens'] - t['cached_input_tokens']) * r[0] + t['cached_input_tokens'] * r[1] + t['output_tokens'] * r[3]) / 1000000\ndef parse_rollout(path, thread):\n    data = json.loads(path.read_text())\n    if data['thread'] != thread: raise ValueError('wrong thread')\n    t = data['tokens']\n    cost = estimate(t, data['model'])\n    return dict(t, status='recorded' if cost is not None else 'partial', estimated_usd=cost, unpriced_requests=int(cost is None), models=[])\n'''


class MetricsTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        estimator = self.root / 'estimator.py'
        estimator.write_text(ESTIMATOR)
        self.config = {'usage_estimator': str(estimator), 'sessions_root': str(self.root / 'sessions'),
                       'pricing': {'version': 'test-date', 'source': 'test-source', 'basis': 'equivalent',
                                   'rates': {'terra': [1, .05, 1.25, 5]}}}
        self.receipt = {'model': 'terra', 'effort': 'high', 'started_unix': 1767225600,
                        'wall_seconds': 10, 'status': 'completed'}
        self.events = self.root / 'run/workers/task/events.jsonl'
        self.events.parent.mkdir(parents=True)

    def write_events(self, *events):
        self.events.write_text(''.join(json.dumps(e) + '\n' for e in events))

    def collect(self):
        return metrics.collect(self.root / 'run', 'task', self.receipt, self.config)

    def test_cli_duplicate_completions_fallback_and_pricing(self):
        usage = {'input_tokens': 1000, 'cached_input_tokens': 800, 'output_tokens': 100,
                 'reasoning_output_tokens': 70}
        done = {'type': 'turn.completed', 'usage': usage}
        self.write_events({'type': 'thread.started', 'thread_id': THREAD}, done, done)
        row = self.collect()
        self.assertEqual(row['status'], 'partial')
        self.assertEqual(row['tokens']['input_tokens'], 1000)
        self.assertAlmostEqual(row['estimated_usd'], (200 + 800 * .05 + 100 * 5) / 1e6)
        self.assertNotIn('thread_id', row)

    def test_missing_usage_and_unpriced_are_not_free(self):
        self.write_events({'type': 'thread.started', 'thread_id': THREAD})
        self.assertIsNone(self.collect()['estimated_usd'])
        self.receipt['model'] = 'unknown'
        self.write_events({'type': 'thread.started', 'thread_id': THREAD},
                          {'type': 'turn.completed', 'usage': {'input_tokens': 1, 'output_tokens': 1}})
        row = self.collect()
        self.assertIsNone(row['estimated_usd'])
        aggregate = metrics.summary([row])[0]
        self.assertEqual(aggregate['unpriced'], 1)
        self.assertIsNone(aggregate['estimated_usd'])

    def test_only_exact_assigned_uuid_and_metadata(self):
        self.write_events({'type': 'thread.started', 'thread_id': THREAD})
        day = self.root / 'sessions/2026/01/01'
        day.mkdir(parents=True)
        payload = {'thread': THREAD, 'model': 'terra', 'tokens': {'input_tokens': 1, 'output_tokens': 1}}
        (day / f'rollout-{OTHER}.jsonl').write_text(json.dumps(payload))
        self.assertEqual(self.collect()['status'], 'unavailable')
        payload['thread'] = OTHER
        (day / f'rollout-{THREAD}.jsonl').write_text(json.dumps(payload))
        with self.assertRaisesRegex(ValueError, 'wrong thread'):
            self.collect()

    def test_multiple_or_invalid_thread_events_do_not_read_rollouts(self):
        self.write_events({'type': 'thread.started', 'thread_id': THREAD},
                          {'type': 'thread.started', 'thread_id': OTHER})
        self.assertEqual(self.collect()['status'], 'unavailable')
        with self.assertRaises(ValueError):
            metrics.collect(self.root / 'run', '../task', self.receipt, self.config)


if __name__ == '__main__':
    unittest.main()
