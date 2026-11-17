"""Auth recovery preserves terminal attempts and refuses semantic or live work."""
import hashlib
import io
import json
import os
from pathlib import Path
import sqlite3
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import retire as r
import retire_auth_recovery as auth
from retire_queue import Queue


def digest(data):
    return hashlib.sha256(data).hexdigest()


class AuthRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.queue = Queue(self.root/'queue.sqlite')
        (self.root/'leases').mkdir()
        (self.root/'leases/author').touch()
        self.queue.add_checkpoint('base', {'proof': True})
        self.job = 'author'
        self.rule = 'm_rule'
        self.queue.add_jobs([{'id': self.job, 'kind': 'author', 'rules': [self.rule]}])
        claim = self.queue.claim('worker')
        self.attempt = claim['attempt_id']
        self.run = self.root/'.cache/attempts'/self.attempt
        self.worker = self.run/'workers'/self.job
        self.worker.mkdir(parents=True)
        descriptor = self.run/'tasks'/f'{self.job}.json'
        descriptor.parent.mkdir()
        r.write(descriptor, {'id': self.job, 'rules': [self.rule],
                             'checkout': str(self.root/'workspaces'/self.attempt)})
        r.write(self.run/'support.json', {'tasks': {self.job: r.digest(descriptor)}})
        pid = 999999999
        self.queue.mark_started(self.attempt, pid, str(self.worker/'receipt.json'))
        r.write(self.worker/'receipt.json', {'pid': pid, 'task': self.job, 'status': 'needs-review',
                                            'returncode': 1, 'started_unix': 1, 'finished_unix': 2})
        (self.worker/'final.txt').write_text('')
        (self.worker/'events.jsonl').write_text(json.dumps({'type': 'turn.failed',
            'error': {'message': 'unexpected status 401 Unauthorized: Missing authentication'}})+'\n')
        self.receipt = self.root/'.cache/receipts'/f'{self.attempt}.json'
        self.receipt.parent.mkdir()
        record = {'attempt': self.attempt, 'job': self.job, 'task': self.job, 'kind': 'author',
                  'status': 'operational-failure', 'run': str(self.run), 'base_commit': 'base',
                  'error': 'Expecting value: line 1 column 1 (char 0)'}
        r.write(self.receipt, record)
        self.queue.finish(self.attempt, 'needs-review', {'receipt': str(self.receipt), **record},
                          {self.rule: {'status': 'needs-review', 'reason': 'Operational failure',
                                       'evidence': str(self.receipt)}})
        self.archive = self.root/'.cache/archives'/f'{self.attempt}.tar.gz'
        self.archive.parent.mkdir()
        self.manifest = {'format': 1, 'attempt': self.attempt, 'task': self.job, 'base_commit': 'base',
                         'receipt': record, 'receipt_sha256': r.digest(self.receipt),
                         'descriptor_sha256': r.digest(descriptor), 'files': {}, 'deleted': [],
                         'patch_sha256': digest(b'')}
        self.write_archive()
        r.write(self.root/'config.json', {'expected_rules': [self.rule]})
        self.plan = self.root/'plan.json'
        self.receipt_out = self.root/'applied.json'

    def write_archive(self, file_data=None):
        members = {'manifest.json': json.dumps(self.manifest).encode(), 'candidate.patch': b''}
        if file_data:
            members.update({'files/'+k: v for k, v in file_data.items()})
        with tarfile.open(self.archive, 'w:gz') as archive:
            for name, data in members.items():
                info = tarfile.TarInfo(name); info.size = len(data); info.mode = 0o600
                archive.addfile(info, io.BytesIO(data))
        r.write(self.run/'archive.json', {'archive': str(self.archive), 'sha256': r.digest(self.archive),
                                         'checkout_removed': True})

    def plan_it(self):
        return auth.dryplan(self.root, self.plan, [self.attempt])

    def test_plan_is_readonly_and_apply_preserves_original(self):
        before = self.queue.get_attempt(self.attempt)
        events = self.queue.audit()
        plan = self.plan_it()
        self.assertEqual(self.queue.get_attempt(self.attempt), before)
        self.assertEqual(self.queue.audit(), events)
        self.assertEqual(self.queue.coverage()['counts'], {'needs-review': 1})
        result = auth.apply(self.root, self.plan, r.digest(self.plan), self.receipt_out)
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(self.queue.coverage()['counts'], {'pending': 1})
        self.assertEqual(self.queue.get_attempt(self.attempt), before)
        self.assertEqual(self.queue.audit()[-1]['kind'], 'retry-auth')
        self.assertEqual(plan['jobs'][0]['reuse']['sha256'], r.digest(self.archive))
        with self.assertRaises(ValueError):
            auth.apply(self.root, self.plan, r.digest(self.plan), self.root/'again.json')
        self.assertEqual(len([x for x in self.queue.audit() if x['kind'] == 'retry-auth']), 1)

    def test_live_pid_held_lease_and_nonempty_final_refuse(self):
        with patch.object(auth.os, 'kill', return_value=None), self.assertRaisesRegex(ValueError, 'PID is present'):
            self.plan_it()
        with self.queue._retry_lease(self.job), self.assertRaisesRegex(ValueError, 'lease is still held'):
            self.plan_it()
        (self.worker/'final.txt').write_text('{"rules": []}')
        with self.assertRaisesRegex(ValueError, 'nonempty final'):
            self.plan_it()

    def test_only_terminal_401_without_completed_model_turn_qualifies(self):
        path = self.worker/'events.jsonl'
        for events in ([{'type': 'turn.failed', 'error': {'message': '429 retry later'}}],
                       [{'type': 'turn.failed', 'error': {'message': '401 Unauthorized'}}, {'type': 'turn.started'}],
                       [{'type': 'turn.completed'}, {'type': 'turn.failed', 'error': {'message': '401 Unauthorized'}}],
                       [{'type': 'item.completed', 'item': {'type': 'agent_message', 'text': 'reviewed'}},
                        {'type': 'turn.failed', 'error': {'message': '401 Unauthorized'}}]):
            with self.subTest(events=events):
                path.write_text(''.join(json.dumps(x)+'\n' for x in events))
                with self.assertRaisesRegex(ValueError, 'terminal HTTP 401'):
                    self.plan_it()

    def test_exact_signed_out_refresh_error_qualifies_and_is_identified(self):
        path = self.worker/'events.jsonl'
        path.write_text(json.dumps({'type': 'turn.failed',
                                    'error': {'message': auth.SIGNED_OUT_REFRESH_ERROR}})+'\n')
        plan = self.plan_it()
        self.assertEqual(plan['jobs'][0]['auth_failure_kind'], 'signed-out-token-refresh')
        self.assertEqual(auth.apply(self.root, self.plan, r.digest(self.plan), self.receipt_out)['status'],
                         'passed')
        self.assertEqual(self.queue.coverage()['counts'], {'pending': 1})

    def test_refresh_near_matches_semantic_refusal_and_quota_do_not_qualify(self):
        path = self.worker/'events.jsonl'
        messages = [auth.SIGNED_OUT_REFRESH_ERROR+' Extra reason.',
                    auth.SIGNED_OUT_REFRESH_ERROR.replace('Please sign in again.', 'Try again.'),
                    'Your access token could not be refreshed.',
                    'Authentication failed for an unspecified reason',
                    'Unsupported semantic contract; needs deeper review',
                    'You have exceeded your current quota', '429 Too Many Requests']
        for message in messages:
            with self.subTest(message=message):
                path.write_text(json.dumps({'type': 'turn.failed', 'error': {'message': message}})+'\n')
                with self.assertRaisesRegex(ValueError, 'terminal HTTP 401 or exact signed-out'):
                    self.plan_it()

    def test_support_archive_tamper_and_timeout_refuse(self):
        support = self.run/'support.json'; old = support.read_bytes()
        r.write(support, {'tasks': {self.job: '0'*64}})
        with self.assertRaisesRegex(ValueError, 'binding differs'):
            self.plan_it()
        support.write_bytes(old)
        with self.archive.open('ab') as stream: stream.write(b'tamper')
        with self.assertRaisesRegex(ValueError, 'binding differs'):
            self.plan_it()
        self.write_archive()
        receipt = self.worker/'receipt.json'; data = r.read(receipt); data['timed_out'] = True; r.write(receipt, data)
        with self.assertRaisesRegex(ValueError, 'binding differs'):
            self.plan_it()

    def test_plan_code_artifact_and_current_attempt_races_refuse(self):
        self.plan_it()
        original_sha = r.digest(self.plan)
        with self.assertRaisesRegex(ValueError, 'plan hash'):
            auth.apply(self.root, self.plan, '0'*64, self.receipt_out)
        with patch.object(auth, 'verifier_hashes', return_value={}), self.assertRaisesRegex(ValueError, 'plan identity'):
            auth.apply(self.root, self.plan, original_sha, self.receipt_out)
        (self.worker/'events.jsonl').write_text((self.worker/'events.jsonl').read_text()+'\n')
        with self.assertRaisesRegex(ValueError, 'evidence changed'):
            auth.apply(self.root, self.plan, original_sha, self.receipt_out)
        self.assertEqual(r.read(self.receipt_out)['status'], 'stopped')
        with sqlite3.connect(self.root/'queue.sqlite') as db:
            db.execute("UPDATE jobs SET attempt_id=NULL,status='pending' WHERE id=?", (self.job,))
        with self.assertRaisesRegex(ValueError, 'current terminal'):
            auth.apply(self.root, self.plan, original_sha, self.root/'race.json')

    def test_retry_result_may_be_claimed_immediately(self):
        self.plan_it()
        real = auth.retry
        def retry_then_claim(queue, *args):
            real(queue, *args)
            self.assertIsNotNone(queue.claim('new-worker'))
        with patch.object(auth, 'retry', side_effect=retry_then_claim):
            result = auth.apply(self.root, self.plan, r.digest(self.plan), self.receipt_out)
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(self.queue.coverage()['counts'], {'running': 1})

    def test_semantic_disposition_or_missing_archive_refuses(self):
        with sqlite3.connect(self.root/'queue.sqlite') as db:
            raw = db.execute('SELECT result FROM attempts WHERE id=?', (self.attempt,)).fetchone()[0]
            result = json.loads(raw)
            result['evidence']['dispositions'] = {self.rule: {'status': 'needs-review'}}
            db.execute('UPDATE attempts SET result=? WHERE id=?', (json.dumps(result), self.attempt))
        with self.assertRaisesRegex(ValueError, 'semantic disposition'):
            self.plan_it()
        with sqlite3.connect(self.root/'queue.sqlite') as db:
            db.execute('UPDATE attempts SET result=? WHERE id=?', (raw, self.attempt))
        self.archive.unlink()
        with self.assertRaisesRegex(ValueError, 'evidence path'):
            self.plan_it()

    def test_other_jobs_and_rule_owners_are_unchanged(self):
        self.queue.add_jobs([{'id': 'other', 'rules': ['m_other'], 'kind': 'author'}])
        r.write(self.root/'config.json', {'expected_rules': [self.rule, 'm_other']})
        original = next(row for row in self.queue.get_jobs() if row['id'] == 'other')
        owners = {name: row['job_id'] for name, row in self.queue.coverage()['rules'].items()}
        self.plan_it()
        auth.apply(self.root, self.plan, r.digest(self.plan), self.receipt_out)
        self.assertEqual(next(row for row in self.queue.get_jobs() if row['id'] == 'other'), original)
        self.assertEqual({name: row['job_id'] for name, row in self.queue.coverage()['rules'].items()}, owners)

    def test_partial_archived_work_is_verified_and_retained(self):
        data = b'bounded private candidate\n'
        self.manifest['files'] = {'grammar/syntax/example.ebnf': {'sha256': digest(data), 'mode': 0o600}}
        self.write_archive({'grammar/syntax/example.ebnf': data})
        self.assertEqual(self.plan_it()['jobs'][0]['archived_files'], 1)
        self.assertTrue(self.archive.is_file())


if __name__ == '__main__':
    unittest.main()
