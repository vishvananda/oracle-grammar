from contextlib import closing
import fcntl
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from retire_queue import Queue, RehabilitationIneligible


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value)+'\n')


def spec(path):
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


class RehabilitationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.queue = Queue(self.root/'queue.sqlite')
        self.queue.add_checkpoint('old', 'proof')
        self.rules = ['m_one', 'm_two', 'm_three']
        self.queue.add_jobs([{'id': 'family', 'kind': 'author', 'rules': self.rules,
                              'source_fragments': [{'name': rule} for rule in self.rules],
                              'target_faces': ['target']}])
        self.claim = self.queue.claim('worker')
        self.attempt = self.claim['attempt_id']
        child = subprocess.Popen(['true'])
        child.wait()
        self.queue.mark_started(self.attempt, pid=child.pid)
        self.lease = self.root/'leases/family'
        self.lease.parent.mkdir()
        self.lease.touch()
        self.queue.finish(self.attempt, 'provisional-pass', {'reason': 'individual pass'},
                          {rule: {'status': 'pass', 'evidence': rule+' evidence'} for rule in self.rules})
        self.queue.reject_jobs(['family'], {'reason': 'combined integration failure'})
        self.new_checkpoint('new', ['m_one'], 'old')

    def new_checkpoint(self, commit, unused, parent):
        inputs = self.root/'checkpoints'/commit/'inputs'
        write(self.root/'checkpoints'/f'{commit}.json', {'commit': commit, 'inputs': str(inputs)})
        manifest = inputs/'manifest.json'
        write(manifest, {'format': 3, 'commit': commit, 'population': 2})
        scan = inputs/'scan/rules.json'
        write(scan, {'summary': {'faces': 2, 'equal': 2, 'at_risk': 0, 'unused_rules': len(unused)}, 'unused': unused})
        marker = inputs/'scan-complete.json'
        write(marker, {'report': spec(scan)['sha256']})
        commands = inputs/'scan/commands.jsonl'
        write(commands, {'program': 'rule-audit', 'returncode': 0})
        for rule in unused:
            identity = hashlib.sha256(rule.encode()).hexdigest()[:20]
            write(inputs/'jobs'/identity/'job.json', {'rule': rule, 'scan': spec(scan)['sha256'],
                                                       'snapshot': spec(manifest)['sha256']})
        self.queue.add_checkpoint(commit, {'inputs': str(inputs)}, expected_parent=parent)
        self.proof = {'kind': 'checkpoint-unused-rehabilitation', 'scan': spec(scan),
                      'scan_complete': spec(marker), 'manifest': spec(manifest), 'commands': spec(commands)}

    def parent_snapshot(self):
        with closing(self.queue._connect()) as db:
            return tuple(db.execute('SELECT payload,attempt_id,adjudication FROM jobs WHERE id=?', ('family',)).fetchone())

    def test_exact_split_preserves_history_and_projects_review(self):
        old, old_attempt = self.parent_snapshot(), self.queue.get_attempt(self.attempt)
        out = self.queue.rehabilitate_unused('family', 'new', self.proof)
        self.assertEqual(out['rules'], ['m_one'])
        self.assertEqual(self.parent_snapshot(), old)
        self.assertEqual(self.queue.get_attempt(self.attempt), old_attempt)
        coverage = self.queue.coverage()
        self.assertEqual(coverage['total_rules'], 3)
        self.assertEqual(coverage['counts'], {'pending': 1, 'needs-review': 2})
        self.assertEqual(coverage['accepted_rules'], 0)
        self.assertEqual(coverage['rules']['m_two']['disposition'], 'needs-review')
        self.assertEqual(coverage['rules']['m_two']['outcome']['status'], 'pass')
        review = next(job for job in coverage['jobs'] if job['id'] == out['review_job'])
        self.assertIsNone(review['attempt_id'])
        self.assertEqual(review['review_origin']['job_id'], 'family')
        self.assertEqual(review['review_origin']['attempt_id'], self.attempt)
        self.assertEqual(set(review['outcomes']), {'m_two', 'm_three'})
        mechanical = self.queue.claim('mechanical', kinds=['mechanical'])
        self.assertEqual(mechanical['job']['rules'], ['m_one'])
        self.assertEqual(mechanical['base_commit'], 'new')
        self.assertNotEqual(mechanical['attempt_id'], self.attempt)
        with self.assertRaises(RehabilitationIneligible):
            self.queue.rehabilitate_unused('family', 'new', self.proof)

    def test_inherited_review_can_split_only_after_later_checkpoint(self):
        first = self.queue.rehabilitate_unused('family', 'new', self.proof)
        review = first['review_job']
        with self.assertRaises(RehabilitationIneligible):
            self.queue.rehabilitate_unused(review, 'new', self.proof)
        self.new_checkpoint('newer', ['m_two'], 'new')
        second = self.queue.rehabilitate_unused(review, 'newer', self.proof)
        self.assertEqual(second['rules'], ['m_two'])
        coverage = self.queue.coverage()
        self.assertEqual(coverage['counts'], {'pending': 2, 'needs-review': 1})
        child = next(job for job in coverage['jobs'] if job['id'] == second['review_job'])
        self.assertEqual(child['rules'], ['m_three'])
        self.assertEqual(child['review_origin']['job_id'], 'family')
        self.assertEqual(child['review_origin']['attempt_id'], self.attempt)

    def test_guards_failed_scan_held_lease_and_checkpoint_race(self):
        with self.lease.open('w') as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaises(RehabilitationIneligible):
                self.queue.rehabilitate_unused('family', 'new', self.proof)
        with patch('retire_queue.os.kill', return_value=None):
            with self.assertRaisesRegex(RehabilitationIneligible, 'still present'):
                self.queue.rehabilitate_unused('family', 'new', self.proof)
        commands = Path(self.proof['commands']['path'])
        write(commands, {'program': 'rule-audit', 'returncode': 1})
        self.proof['commands'] = spec(commands)
        with self.assertRaisesRegex(ValueError, 'completion'):
            self.queue.rehabilitate_unused('family', 'new', self.proof)
        write(commands, {'program': 'rule-audit', 'returncode': 0})
        self.proof['commands'] = spec(commands)
        import retire_rehabilitation
        real = retire_rehabilitation.verify_rule_receipts
        def advance(*args):
            real(*args)
            self.queue.add_checkpoint('raced', {'inputs': 'other'}, expected_parent='new')
        with patch('retire_rehabilitation.verify_rule_receipts', side_effect=advance):
            with self.assertRaises(RehabilitationIneligible):
                self.queue.rehabilitate_unused('family', 'new', self.proof)
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})

    def test_semantic_mechanical_failure_creates_a_targeted_author_owner(self):
        self.new_checkpoint('mechanical-tip', ['m_extra'], 'new')
        job = {'id': 'mechanical-extra', 'kind': 'mechanical', 'rules': ['m_extra'],
               'goal': 'Retire the assigned fallback.', 'target_faces': []}
        self.queue.add_jobs([job])
        claim = self.queue.claim('worker', kinds=['mechanical'])
        attempt = claim['attempt_id']
        child = subprocess.Popen(['true'])
        child.wait()
        self.queue.mark_started(attempt, pid=child.pid)
        (self.root/'leases'/job['id']).touch()
        self.queue.finish(attempt, 'needs-review', {'kind': 'mechanical'},
                          {'m_extra': {'status': 'needs-review', 'reason': 'exact print regressed', 'evidence': 'receipt'}})
        before = self.queue.get_attempt(attempt)
        evidence = {'kind': 'mechanical-author-escalation', 'scan': self.proof}
        with patch('retire_queue._verify_mechanical_readmission', return_value={}) as verify, \
             patch('retire_rehabilitation.mechanical_failure', return_value=['Target']), \
             patch('retire_rehabilitation.recheck_mechanical_failure', side_effect=ValueError('proof changed')):
            with self.assertRaisesRegex(ValueError, 'proof changed'):
                self.queue.escalate_mechanical_failure(job['id'], attempt, 'mechanical-tip', evidence)
            self.assertTrue(all(call.kwargs == {'passed': False} for call in verify.call_args_list))
        self.assertEqual(self.queue.coverage()['rules']['m_extra']['disposition'], 'needs-review')
        with patch('retire_queue._verify_mechanical_readmission', return_value={}), \
             patch('retire_rehabilitation.mechanical_failure', return_value=['Target']), \
             patch('retire_rehabilitation.recheck_mechanical_failure'):
            decision = self.queue.escalate_mechanical_failure(job['id'], attempt, 'mechanical-tip', evidence)
            with self.assertRaisesRegex(ValueError, 'stale'):
                self.queue.escalate_mechanical_failure(job['id'], attempt, 'mechanical-tip', evidence)
        self.assertEqual(self.queue.get_attempt(attempt), before)
        accepted = self.queue.claim('terra', kinds=['author'])
        self.assertEqual(accepted['job']['id'], decision['child_job'])
        self.assertEqual(accepted['job']['rules'], ['m_extra'])
        self.assertEqual(accepted['job']['target_faces'], ['Target'])
        self.assertEqual(self.queue.coverage()['total_rules'], 4)
        self.assertEqual(self.queue.coverage()['accepted_rules'], 0)

    def test_parked_singleton_mechanical_and_terminal_failed(self):
        self.queue.add_jobs([{'id': 'unused-mechanical', 'kind': 'mechanical', 'rules': ['m_other']}])
        claim = self.queue.claim('mechanical')
        child = subprocess.Popen(['true'])
        child.wait()
        self.queue.mark_started(claim['attempt_id'], pid=child.pid)
        (self.root/'leases/unused-mechanical').touch()
        self.queue.finish(claim['attempt_id'], 'failed', 'old print failure',
                          {'m_other': {'status': 'needs-review', 'reason': 'print', 'evidence': 'old'}})
        self.new_checkpoint('newer', ['m_other'], 'new')
        split = self.queue.rehabilitate_unused('unused-mechanical', 'newer', self.proof)
        self.assertIsNone(split['review_job'])
        job = next(job for job in self.queue.get_jobs('pending') if job['id'] == split['mechanical_jobs'][0])
        self.assertEqual(job['job']['kind'], 'mechanical')
        self.assertEqual(job['job']['rules'], ['m_other'])
        self.assertEqual(self.queue.coverage()['accepted_rules'], 0)
        self.assertEqual(self.queue.get_attempt(claim['attempt_id'])['result']['status'], 'failed')

    def test_new_actual_review_attempt_takes_precedence_over_inherited_history(self):
        split = self.queue.rehabilitate_unused('family', 'new', self.proof)
        review_id = split['review_job']
        with closing(self.queue._connect()) as db:
            db.execute("UPDATE jobs SET status='pending' WHERE id=?", (review_id,))
        claim = self.queue.claim('reviewer', kinds=['author'])
        self.assertEqual(claim['job']['id'], review_id)
        child = subprocess.Popen(['true'])
        child.wait()
        self.queue.mark_started(claim['attempt_id'], pid=child.pid)
        (self.root/'leases'/review_id).touch()
        self.queue.finish(claim['attempt_id'], 'needs-review', 'new assessment',
                          {rule: {'status': 'needs-review', 'reason': 'new finding', 'evidence': 'new'}
                           for rule in claim['job']['rules']})
        coverage = self.queue.coverage()
        review = next(job for job in coverage['jobs'] if job['id'] == review_id)
        self.assertEqual(review['attempt_id'], claim['attempt_id'])
        self.assertEqual(review['prior_review_origin']['attempt_id'], self.attempt)
        self.assertEqual(coverage['rules']['m_two']['outcome']['reason'], 'new finding')
        self.assertEqual(coverage['rules']['m_two']['disposition'], 'needs-review')
        self.new_checkpoint('newer', ['m_two'], 'new')
        next_split = self.queue.rehabilitate_unused(review_id, 'newer', self.proof)
        current = next(job for job in self.queue.coverage()['jobs'] if job['id'] == next_split['review_job'])
        self.assertEqual(current['review_origin']['job_id'], review_id)
        self.assertEqual(current['review_origin']['attempt_id'], claim['attempt_id'])

    def test_inherited_outcome_tampering_is_detected(self):
        split = self.queue.rehabilitate_unused('family', 'new', self.proof)
        with closing(self.queue._connect()) as db:
            raw = db.execute('SELECT result FROM attempts WHERE id=?', (self.attempt,)).fetchone()[0]
            value = json.loads(raw)
            value['outcomes']['m_two']['evidence'] = 'changed'
            db.execute('UPDATE attempts SET result=? WHERE id=?', (json.dumps(value), self.attempt))
        with self.assertRaisesRegex(ValueError, 'origin hash differs'):
            self.queue.coverage()
        self.assertIsNotNone(split['review_job'])

    def test_provisional_and_unknown_pid_are_not_reclassified(self):
        self.queue.add_jobs([{'id': 'unknown', 'kind': 'author', 'rules': ['m_four']}])
        claim = self.queue.claim('other')
        (self.root/'leases/unknown').touch()
        self.queue.mark_started(claim['attempt_id'], receipt='unknown')
        self.queue.finish(claim['attempt_id'], 'needs-review', 'failure',
                          {'m_four': {'status': 'needs-review', 'reason': 'unknown', 'evidence': 'x'}})
        with self.assertRaises(RehabilitationIneligible):
            self.queue.rehabilitate_unused('unknown', 'new', self.proof)
        self.queue.add_jobs([{'id': 'passed', 'kind': 'author', 'rules': ['m_five']}])
        claim = self.queue.claim('third')
        self.queue.finish(claim['attempt_id'], 'provisional-pass', 'proof',
                          {'m_five': {'status': 'pass', 'evidence': 'x'}})
        with self.assertRaises(RehabilitationIneligible):
            self.queue.rehabilitate_unused('passed', 'new', self.proof)


if __name__ == '__main__':
    unittest.main()
