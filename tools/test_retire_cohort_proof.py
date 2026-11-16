import fcntl
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import retire_cohort_proof
from retire_queue import Queue


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value)+'\n')


def jsonl(path, values):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(''.join(json.dumps(value)+'\n' for value in values))


def spec(path):
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


class CohortRetryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.queue = Queue(self.root/'queue.sqlite')
        self.queue.add_checkpoint('base', 'proof')
        self.queue.add_jobs([{'id': 'job0', 'rules': ['r0a', 'r0b']}])
        self.claim = self.queue.claim('worker')
        self.attempt = self.claim['attempt_id']
        self.run = self.root/'.cache/attempts'/self.attempt
        self.inputs = self.root/'frozen-run'
        self.frozen = self.inputs/'frozen'
        self.frozen.mkdir(parents=True)
        self.run.mkdir(parents=True)
        (self.run/'inputs').symlink_to(self.inputs, target_is_directory=True)
        self.eval = self.run/'evaluations/job0/evaluation'
        self.eval.mkdir(parents=True)
        self.lease = self.root/'leases/job0'
        self.lease.parent.mkdir()
        self.lease.touch()
        child = subprocess.Popen(['true'])
        child.wait()
        self.queue.mark_started(self.attempt, pid=child.pid)
        self.populate()

    def populate(self):
        gold = [{'faces': [{'name': 'A'}, {'name': 'B'}]}, {'faces': [{'name': 'B'}, {'name': 'C'}]}]
        index = {}
        for ordinal, (record, face, name) in enumerate([(0, 0, 'A'), (0, 1, 'B'), (1, 0, 'B'), (1, 1, 'C')]):
            row = {'name': name, 'text': name, 'status': 'equal', 'readings': 1, 'exact': True}
            index[str(ordinal)] = {'record': record, 'face': face, 'name': name, 'text': name,
                                   'parse': row, 'flags': [True, True, True, True]}
        write(self.frozen/'index.json', index)
        jsonl(self.frozen/'gold.jsonl', gold)
        manifest = {'format': 3, 'commit': 'base', 'files': {
            'index.json': spec(self.frozen/'index.json')['sha256'],
            'gold.jsonl': spec(self.frozen/'gold.jsonl')['sha256']}}
        write(self.inputs/'manifest.json', manifest)
        write(self.root/'checkpoints/base.json', {'inputs': str(self.inputs)})
        descriptor = self.run/'tasks/job0.json'
        write(descriptor, {'id': 'job0', 'rules': ['r0a', 'r0b'], 'cohort': ['A']})
        support = self.run/'support.json'
        write(support, {'tasks': {'job0': spec(descriptor)['sha256']}})
        jsonl(self.eval/'gold.jsonl', [gold[0]])
        jsonl(self.eval/'before.jsonl', [index[str(i)]['parse'] for i in range(3)])
        jsonl(self.eval/'rows.jsonl', [index[str(i)]['parse'] for i in range(2)])
        jsonl(self.eval/'commands.jsonl', [{'program': 'family-check', 'returncode': 0},
                                          {'program': 'score', 'returncode': 0},
                                          {'program': 'validate-readings', 'returncode': 1}])
        (self.eval/'validate-readings.log').write_text('Error: "old/current score order or populations differ"\n')
        self.report = self.eval/'report.json'
        write(self.report, {'task': 'job0', 'source': 'base', 'passed': False, 'family_returncode': 0,
                            'failures': ['validate-readings returned non-zero'], 'directory': str(self.eval),
                            'descriptor_sha256': spec(descriptor)['sha256'],
                            'support_sha256': spec(support)['sha256']})
        self.proof = {'kind': 'shared-name-cohort-closure',
                      'controller_terminal': {'unit': 'controller', 'invocation': 'one',
                                              'verification': {'inactive': True}},
                      'report': spec(self.report), 'descriptor': spec(descriptor),
                      'manifest': spec(self.inputs/'manifest.json'),
                      'commands': spec(self.eval/'commands.jsonl'), 'log': spec(self.eval/'validate-readings.log')}
        for key in ('gold', 'before', 'rows'):
            self.proof[key] = spec(self.eval/(key+'.jsonl'))

    def finish(self):
        self.queue.finish(self.attempt, 'needs-review', {'report': str(self.report)},
                          {rule: {'status': 'needs-review', 'reason': 'cohort', 'evidence': 'log'}
                           for rule in self.claim['job']['rules']})

    def test_proved_mismatch_can_retry_once_with_history_intact(self):
        with self.assertRaisesRegex(ValueError, 'terminal'):
            self.queue.retry_cohort_tainted('job0', self.attempt, self.proof)
        self.finish()
        with self.lease.open('w') as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, 'still held'):
                self.queue.retry_cohort_tainted('job0', self.attempt, self.proof)
        with patch('retire_queue.os.kill', return_value=None):
            with self.assertRaisesRegex(ValueError, 'still present'):
                self.queue.retry_cohort_tainted('job0', self.attempt, self.proof)
        previous = self.queue.get_attempt(self.attempt)
        self.queue.retry_cohort_tainted('job0', self.attempt, self.proof)
        self.assertEqual(self.queue.get_attempt(self.attempt), previous)
        self.assertEqual(self.queue.coverage()['counts'], {'pending': 2})
        event = self.queue.audit()[-1]
        self.assertEqual(event['kind'], 'retry-cohort-tainted')
        self.assertEqual(event['data']['proof']['original_occurrences'], 2)
        self.assertEqual(event['data']['proof']['old_before_occurrences'], 3)
        self.assertEqual(event['data']['proof']['closed_occurrences'], 4)
        again = self.queue.claim('worker')
        child = subprocess.Popen(['true'])
        child.wait()
        self.queue.mark_started(again['attempt_id'], pid=child.pid)
        self.queue.finish(again['attempt_id'], 'failed', {'report': str(self.report)},
                          {rule: {'status': 'needs-review', 'reason': 'failure', 'evidence': 'x'}
                           for rule in again['job']['rules']})
        # Bounded to one explicit retry per job, regardless of later outcomes.
        with patch('retire_cohort_proof.verify', return_value={}):
            with self.assertRaisesRegex(ValueError, 'already used'):
                self.queue.retry_cohort_tainted('job0', again['attempt_id'], self.proof)

    def test_unknown_pid_is_never_retried(self):
        self.queue.add_jobs([{'id': 'job1', 'rules': ['r1']}])
        claim = self.queue.claim('other')
        (self.root/'leases/job1').touch()
        self.queue.mark_started(claim['attempt_id'], receipt='unknown pid')
        self.queue.finish(claim['attempt_id'], 'failed', {'report': str(self.report)},
                          {'r1': {'status': 'needs-review', 'reason': 'failure', 'evidence': 'x'}})
        with self.assertRaisesRegex(ValueError, 'identity is unknown'):
            self.queue.retry_cohort_tainted('job1', claim['attempt_id'], self.proof)

    def test_tampered_or_unrelated_failures_cannot_retry(self):
        self.finish()
        (self.eval/'before.jsonl').write_text('{}\n')
        with self.assertRaisesRegex(ValueError, 'hash differs'):
            self.queue.retry_cohort_tainted('job0', self.attempt, self.proof)
        self.populate()
        rows = [json.loads(line) for line in (self.eval/'rows.jsonl').read_text().splitlines()]
        rows[0]['status'] = 'unequal'
        jsonl(self.eval/'rows.jsonl', rows)
        self.proof['rows'] = spec(self.eval/'rows.jsonl')
        with self.assertRaisesRegex(ValueError, 'separate semantic or print failure'):
            self.queue.retry_cohort_tainted('job0', self.attempt, self.proof)
        self.populate()
        (self.eval/'validate-readings.log').write_text('Error: "unrelated native failure"\n')
        self.proof['log'] = spec(self.eval/'validate-readings.log')
        with self.assertRaisesRegex(ValueError, 'not the old cohort'):
            self.queue.retry_cohort_tainted('job0', self.attempt, self.proof)
        self.assertEqual(self.queue.coverage()['counts'], {'needs-review': 2})


if __name__ == '__main__':
    unittest.main()
