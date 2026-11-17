import fcntl
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from retire_queue import Queue


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data)+'\n')


def spec(path):
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


class DiskRetryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.queue = Queue(self.root/'queue.sqlite')
        self.queue.add_checkpoint('base', 'proof')
        self.queue.add_jobs([{'id': 'job0', 'kind': 'author', 'rules': ['a', 'b']}])
        self.claim = self.queue.claim('worker')
        self.attempt = self.claim['attempt_id']
        self.run = self.root/'.cache/attempts'/self.attempt
        self.worker = self.run/'workers/job0'
        self.worker.mkdir(parents=True)
        self.descriptor = self.run/'tasks/job0.json'
        write(self.descriptor, {'id': 'job0', 'rules': ['a', 'b']})
        self.support = self.run/'support.json'
        write(self.support, {'tasks': {'job0': spec(self.descriptor)['sha256']}})
        self.lease = self.root/'leases/job0'
        self.lease.parent.mkdir()
        self.lease.touch()
        child = subprocess.Popen(['true'])
        child.wait()
        controller = subprocess.Popen(['true'])
        controller.wait()
        self.receipt = self.worker/'receipt.json'
        self.log = self.worker/'evaluation.log'
        self.final = self.worker/'final.txt'
        self.decisions = {rule: {'rule': rule, 'status': 'proposed', 'reason': 'diagnosed', 'evidence': 'source'}
                          for rule in ('a', 'b')}
        write(self.final, {'summary': 'complete', 'rules': list(self.decisions.values())})
        write(self.receipt, {'task': 'job0', 'pid': child.pid, 'status': 'completed', 'returncode': 0,
                             'started_unix': 1, 'finished_unix': 2, 'evaluation': {'passed': False,
                             'error': 'independent evaluation failed; inspect evaluation.log'}})
        self.log.write_text(f"fatal: sha1 file '/some/.git/worktrees/{self.attempt}/index.lock' "
                            "write error. Out of diskspace\nTraceback (most recent call last):\n"
                            "changed_files\nCommand '['git', 'diff', '--name-only']' "
                            "CalledProcessError: non-zero exit status 128\n")
        self.queue.mark_started(self.attempt, pid=child.pid, receipt=str(self.receipt))
        self.evidence = {'kind': 'git-index-enospc', 'controller_terminal': {
            'unit': 'old.service', 'invocation': 'old-id', 'verification': {
                'inactive': True, 'main_pid': 0, 'previous_pid': controller.pid}},
            'evaluation_log': spec(self.log), 'model_receipt': spec(self.receipt),
            'descriptor': spec(self.descriptor), 'support': spec(self.support), 'final': spec(self.final)}
        self.original = {'kind': 'author', 'task': 'job0', 'base_commit': 'base', 'run': str(self.run),
                         'report': None, 'dispositions': self.decisions}

    def finish(self):
        self.queue.finish(self.attempt, 'needs-review', self.original,
                          {rule: {'status': 'needs-review',
                                  'reason': 'independent evaluation failed; inspect evaluation.log',
                                  'evidence': 'log'} for rule in ('a', 'b')})

    def test_valid_only_after_terminal_and_lease_and_keeps_history(self):
        with self.assertRaisesRegex(ValueError, 'terminal'):
            self.queue.retry_disk_tainted('job0', self.attempt, self.evidence)
        self.finish()
        with self.lease.open('w') as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, 'still held'):
                self.queue.retry_disk_tainted('job0', self.attempt, self.evidence)
        with patch('retire_queue.os.kill', return_value=None):
            with self.assertRaisesRegex(ValueError, 'still present'):
                self.queue.retry_disk_tainted('job0', self.attempt, self.evidence)
        controller_pid = self.evidence['controller_terminal']['verification']['previous_pid']
        def probe(pid, _signal):
            if pid == controller_pid:
                return None
            raise ProcessLookupError()
        with patch('retire_queue.os.kill', side_effect=probe):
            with self.assertRaisesRegex(ValueError, 'still present'):
                self.queue.retry_disk_tainted('job0', self.attempt, self.evidence)
        prior = self.queue.get_attempt(self.attempt)
        self.queue.retry_disk_tainted('job0', self.attempt, self.evidence)
        self.assertEqual(self.queue.get_attempt(self.attempt), prior)
        self.assertEqual(self.queue.coverage()['counts'], {'pending': 2})
        self.assertEqual(self.queue.audit()[-1]['kind'], 'retry-disk-tainted')
        second = self.queue.claim('other')
        child = subprocess.Popen(['true'])
        child.wait()
        self.queue.mark_started(second['attempt_id'], pid=child.pid)
        self.queue.finish(second['attempt_id'], 'failed', 'proof',
                          {rule: {'status': 'needs-review', 'reason': 'failure', 'evidence': 'x'}
                           for rule in ('a', 'b')})
        with patch('retire_queue._verify_disk_retry'):
            with self.assertRaisesRegex(ValueError, 'already used'):
                self.queue.retry_disk_tainted('job0', second['attempt_id'], self.evidence)

    def test_unknown_worker_pid_never_retries(self):
        self.queue.add_jobs([{'id': 'job1', 'kind': 'author', 'rules': ['x']}])
        claim = self.queue.claim('other')
        (self.root/'leases/job1').touch()
        self.queue.mark_started(claim['attempt_id'], receipt='unknown worker')
        self.queue.finish(claim['attempt_id'], 'failed', 'failure',
                          {'x': {'status': 'needs-review', 'reason': 'failure', 'evidence': 'x'}})
        with self.assertRaisesRegex(ValueError, 'identity is unknown'):
            self.queue.retry_disk_tainted('job1', claim['attempt_id'], self.evidence)

    def test_invalid_log_missing_proof_and_changed_artifacts_are_rejected(self):
        self.finish()
        with self.assertRaisesRegex(ValueError, 'controller evidence'):
            self.queue.retry_disk_tainted('job0', self.attempt, {**self.evidence, 'controller_terminal': {}})
        self.log.write_text('codex_rollout::recorder: No space left on device (os error 28)\n')
        self.evidence['evaluation_log'] = spec(self.log)
        with self.assertRaisesRegex(ValueError, 'does not prove Git index'):
            self.queue.retry_disk_tainted('job0', self.attempt, self.evidence)
        self.receipt.write_text('{}')
        with self.assertRaisesRegex(ValueError, 'hash differs'):
            self.queue.retry_disk_tainted('job0', self.attempt, self.evidence)
        self.assertEqual(self.queue.coverage()['counts'], {'needs-review': 2})

    def test_nonproposed_or_existing_independent_report_cannot_retry(self):
        self.finish()
        bad = json.loads(self.final.read_text())
        bad['rules'][0]['status'] = 'needs-review'
        write(self.final, bad)
        self.evidence['final'] = spec(self.final)
        with self.assertRaisesRegex(ValueError, 'dispositions'):
            self.queue.retry_disk_tainted('job0', self.attempt, self.evidence)
        write(self.final, {'summary': 'complete', 'rules': list(self.decisions.values())})
        self.evidence['final'] = spec(self.final)
        write(self.run/'evaluations/job0/attempt-1/report.json', {'passed': False})
        with self.assertRaisesRegex(ValueError, 'report exists'):
            self.queue.retry_disk_tainted('job0', self.attempt, self.evidence)


if __name__ == '__main__':
    unittest.main()
