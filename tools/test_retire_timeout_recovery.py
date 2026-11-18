"""Timeout recovery never attributes missing model output to a semantic review."""
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
import fcntl
import hashlib
import io
import json
from pathlib import Path
import shlex
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

from retire_queue import Queue
import retire_timeout_recovery as recovery


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value)+'\n')


def spec(path):
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


class TimeoutRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.queue = Queue(self.root/'queue.sqlite')
        old_inputs = self.root/'checkpoints/old/inputs'
        manifest = old_inputs/'manifest.json'; write(manifest, {'format': 3, 'commit': 'old'})
        self.queue.add_checkpoint('old', {'kind': 'accepted-input', 'manifest': spec(manifest)['sha256']})
        self.rules = [f'm_rule_{n}' for n in range(getattr(self, 'rule_count', 6))]
        self.job = {'id': 'family', 'kind': 'author', 'rules': self.rules,
                    'goal': 'review', 'source_fragments': [{'name': rule} for rule in self.rules],
                    'target_faces': ['old target']}
        self.queue.add_jobs([self.job])
        claim = self.queue.claim('worker')
        self.attempt = claim['attempt_id']
        child = subprocess.Popen(['true']); child.wait()
        self.run = self.root/'.cache/attempts'/self.attempt
        worker = self.run/'workers/family'
        worker.mkdir(parents=True)
        receipt = worker/'receipt.json'
        self.queue.mark_started(self.attempt, pid=child.pid, receipt=str(receipt))
        lease = self.root/'leases/family'; lease.parent.mkdir(); lease.touch(); self.lease = lease
        final = worker/'final.txt'; final.write_bytes(b'')
        write(receipt, {'task': 'family', 'pid': child.pid, 'timed_out': True, 'status': 'needs-review',
                        'returncode': -15, 'started_unix': 1, 'finished_unix': 902})
        events = worker/'events.jsonl'; write(events, {'type': 'thread.started', 'thread_id': 'assigned'})
        descriptor = self.run/'tasks/family.json'
        write(descriptor, {'id': 'family', 'rules': self.rules, 'wall_budget_seconds': 900,
                           'checkout': str(self.root/'workspaces'/self.attempt)})
        support = self.run/'support.json'; write(support, {'tasks': {'family': spec(descriptor)['sha256']}})
        (self.run/'inputs').symlink_to(old_inputs, target_is_directory=True)
        write(self.root/'checkpoints/old.json', {'commit': 'old', 'inputs': str(old_inputs)})
        terminal = self.root/'.cache/receipts'/f'{self.attempt}.json'
        write(terminal, {'run': str(self.run), 'task': 'family', 'base_commit': 'old',
                         'status': 'operational-failure', 'error': 'Expecting value: line 1 column 1 (char 0)'})
        self.queue.finish(self.attempt, 'needs-review', {'receipt': str(terminal)},
                          {rule: {'status': 'needs-review', 'reason': 'timed out', 'evidence': str(terminal)}
                           for rule in self.rules})
        archive = self.root/'.cache/archives'/f'{self.attempt}.tar.gz'; archive.parent.mkdir(parents=True)
        archive_manifest = {'task': 'family', 'attempt': self.attempt, 'base_commit': 'old',
                            'descriptor_sha256': spec(descriptor)['sha256'], 'receipt_sha256': spec(terminal)['sha256']}
        with tarfile.open(archive, 'w:gz') as tar:
            data = json.dumps(archive_manifest).encode(); info = tarfile.TarInfo('manifest.json'); info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
        metadata = self.run/'archive.json'; write(metadata, {'archive': str(archive), 'sha256': spec(archive)['sha256'],
                                                              'checkout_removed': True})
        inputs = self.root/'checkpoints/new/inputs'
        write(self.root/'checkpoints/new.json', {'commit': 'new', 'inputs': str(inputs)})
        new_manifest = inputs/'manifest.json'; write(new_manifest, {'format': 3, 'commit': 'new', 'population': 4})
        scan = inputs/'scan/rules.json'; write(scan, {'summary': {'faces': 4, 'equal': 4, 'at_risk': 0, 'unused_rules': 0},
                                                     'unused': [], 'needed': {rule: [f'face{n}'] for n,rule in enumerate(self.rules)}})
        marker = inputs/'scan-complete.json'; write(marker, {'report': spec(scan)['sha256']})
        commands = inputs/'scan/commands.jsonl'; write(commands, {'program': 'rule-audit', 'returncode': 0})
        self.queue.add_checkpoint('new', {'inputs': str(inputs)}, expected_parent='old')
        self.proof = {'kind': 'timeout-split-recovery', **{name: spec(path) for name,path in {
            'final':final,'model_receipt':receipt,'events':events,'descriptor':descriptor,'support':support,
            'base_manifest':manifest,'archive_metadata':metadata,'archive':archive,'terminal_receipt':terminal}.items()},
            'scan_proof': {'kind': 'checkpoint-unused-rehabilitation', 'manifest':spec(new_manifest),
                           'scan':spec(scan),'scan_complete':spec(marker),'commands':spec(commands)}}

    def split(self):
        return self.queue.rehabilitate_timeout('family', self.attempt, 'new', self.proof)

    def test_legacy_diagnostic_requires_the_exact_prompt_and_unfinished_command(self):
        worker = self.run/'workers/family'
        prompt = worker/'prompt.txt'
        events = worker/'events.jsonl'
        descriptor = self.run/'tasks/family.json'
        write(descriptor, {**json.loads(descriptor.read_text()), 'full_pass': True})
        self.proof['descriptor'] = spec(descriptor)
        command = ['/usr/bin/python3', str(self.run/'support/runner/retire_author.py'), str(self.run), 'family']
        shell = shlex.join(['/bin/bash', '-lc', 'ORACLE_GRAMMAR_THREADS=2 '+shlex.join(command)])
        started = {'type': 'item.started',
                   'item': {'id': 'check', 'type': 'command_execution', 'command': shell, 'status': 'in_progress'}}

        def verify(body, observed):
            write(prompt, body)
            events.write_text(''.join(json.dumps(event)+'\n' for event in observed))
            timeout = {**self.proof, 'events': spec(events)}
            recovery.verify_legacy_full_diagnostic(self.root, 'family', self.attempt, timeout, {'prompt': spec(prompt)})

        verify(command, [started])
        with self.assertRaisesRegex(ValueError, 'old campaign prompt'):
            verify([*command, '--preview'], [started])
        with self.assertRaisesRegex(ValueError, 'unfinished full worker diagnostic'):
            verify(command, [started, {'type': 'item.completed', 'item': {'id': 'check'}}])
        with self.assertRaisesRegex(ValueError, 'unfinished full worker diagnostic'):
            verify(command, [{**started, 'item': {**started['item'], 'command': shlex.join([*command, '--preview'])}}])

    def test_split_preserves_attempt_and_exact_coverage_without_acceptance(self):
        original = self.queue.get_attempt(self.attempt)
        result = self.split()
        self.assertEqual(self.queue.get_attempt(self.attempt), original)
        self.assertEqual(self.queue.coverage()['counts'], {'pending': 6})
        self.assertEqual(self.queue.coverage()['accepted_rules'], 0)
        children = [entry['job'] for entry in self.queue.get_jobs('pending')]
        self.assertEqual([x['rules'] for x in children], [self.rules[:3], self.rules[3:]])
        self.assertTrue(all(x['rule_dispositions_required'] == x['rules'] for x in children))
        self.assertEqual([x['target_faces'] for x in children], [[f'face{n}' for n in range(3)], [f'face{n}' for n in range(3,6)]])
        self.assertTrue(all(x['timeout_origin']['attempt_id'] == self.attempt for x in children))
        self.assertEqual(set(result['children']), {x['id'] for x in children})
        with self.assertRaises(ValueError): self.split()
        self.assertEqual(self.queue.claim('next')['base_commit'], 'new')

    def test_four_and_five_rule_timeouts_split_balanced(self):
        for count, sizes in ((4, [2, 2]), (5, [3, 2])):
            with self.subTest(count=count):
                fixture = TimeoutRecoveryTests('runTest')
                fixture.rule_count = count
                fixture.setUp()
                try:
                    original = fixture.queue.get_attempt(fixture.attempt)
                    result = fixture.split()
                    children = [entry['job'] for entry in fixture.queue.get_jobs('pending')]
                    self.assertEqual([len(child['rules']) for child in children], sizes)
                    self.assertEqual([rule for child in children for rule in child['rules']], fixture.rules)
                    self.assertTrue(all(child['rule_dispositions_required'] == child['rules'] for child in children))
                    self.assertEqual(fixture.queue.coverage()['counts'], {'pending': count})
                    self.assertEqual(fixture.queue.coverage()['accepted_rules'], 0)
                    self.assertEqual(fixture.queue.get_attempt(fixture.attempt), original)
                    self.assertEqual(len(result['children']), 2)
                    with self.assertRaises(ValueError): fixture.split()
                finally:
                    fixture.doCleanups()

    def test_out_of_range_rule_counts_are_refused(self):
        for count in (1, 2, 3, 7):
            with self.subTest(count=count):
                fixture = TimeoutRecoveryTests('runTest')
                fixture.rule_count = count
                fixture.setUp()
                try:
                    with self.assertRaisesRegex(ValueError, 'four-to-six-rule'): fixture.split()
                    self.assertEqual(fixture.queue.coverage()['counts'], {'needs-review': count})
                finally:
                    fixture.doCleanups()

    def test_small_retry_keeps_rules_owner_and_preserves_finished_attempt(self):
        fixture = TimeoutRecoveryTests('runTest')
        fixture.rule_count = 1
        fixture.setUp()
        try:
            original = fixture.queue.get_attempt(fixture.attempt)
            proof = dict(fixture.proof, kind='timeout-small-retry')
            result = fixture.queue.rehabilitate_timeout('family', fixture.attempt, 'new', proof)
            self.assertEqual(result['job_id'], 'family')
            self.assertEqual(fixture.queue.get_attempt(fixture.attempt), original)
            self.assertEqual(fixture.queue.coverage()['counts'], {'pending': 1})
            self.assertEqual(fixture.queue.get_jobs('pending')[0]['job'], fixture.job)
            self.assertEqual(fixture.queue.audit()[-1]['kind'], 'retried-small-timeout')
            claim = fixture.queue.claim('next')
            self.assertEqual(claim['base_commit'], 'new')
            self.assertNotEqual(claim['attempt_id'], fixture.attempt)
        finally:
            fixture.doCleanups()

    def test_small_retry_cannot_act_on_a_wide_or_previously_retried_job(self):
        with self.assertRaisesRegex(ValueError, 'one-to-three-rule'):
            self.queue.rehabilitate_timeout('family', self.attempt, 'new', dict(self.proof, kind='timeout-small-retry'))
        fixture = TimeoutRecoveryTests('runTest')
        fixture.rule_count = 2
        fixture.setUp()
        try:
            proof = dict(fixture.proof, kind='timeout-small-retry')
            with fixture.queue._transaction() as db:
                fixture.queue._event(db, 'retried-small-timeout', {'job_id': 'family'})
            with self.assertRaisesRegex(ValueError, 'already used'):
                fixture.queue.rehabilitate_timeout('family', fixture.attempt, 'new', proof)
            self.assertEqual(fixture.queue.coverage()['counts'], {'needs-review': 2})
        finally:
            fixture.doCleanups()

    def test_concurrent_split_exactly_one_succeeds(self):
        def go():
            try: return self.split()
            except ValueError: return None
        with ThreadPoolExecutor(max_workers=2) as pool:
            outcomes = list(pool.map(lambda _: go(), range(2)))
        self.assertEqual(sum(x is not None for x in outcomes), 1)
        self.assertEqual(self.queue.coverage()['counts'], {'pending': 6})

    def test_held_lease_live_pid_and_stale_checkpoint_are_refused(self):
        with self.lease.open('r+') as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, 'lease is still held'): self.split()
        with patch('retire_queue.os.kill', return_value=None):
            with self.assertRaisesRegex(ValueError, 'still present'): self.split()
        with self.assertRaisesRegex(ValueError, 'stale'):
            self.queue.rehabilitate_timeout('family', self.attempt, 'old', self.proof)
        self.assertEqual(self.queue.coverage()['counts'], {'needs-review': 6})

    def test_final_evidence_tampering_and_completed_final_refused(self):
        path = Path(self.proof['final']['path']); path.write_text('{}')
        with self.assertRaisesRegex(ValueError, 'hash or size'): self.split()
        self.proof['final'] = spec(path)
        with self.assertRaisesRegex(ValueError, 'exactly empty'): self.split()
        self.assertEqual(self.queue.coverage()['counts'], {'needs-review': 6})

    def test_mutated_receipt_and_archive_refused(self):
        path = Path(self.proof['model_receipt']['path']); data = json.loads(path.read_text()); data['timed_out'] = False
        write(path, data); self.proof['model_receipt'] = spec(path)
        with self.assertRaisesRegex(ValueError, 'timed-out process'): self.split()

    def test_completed_thread_and_archive_hash_changes_are_refused(self):
        path = Path(self.proof['events']['path'])
        with path.open('a') as stream: stream.write(json.dumps({'type': 'turn.completed'})+'\n')
        self.proof['events'] = spec(path)
        with self.assertRaisesRegex(ValueError, 'unfinished assigned thread'): self.split()
        write(path, {'type': 'thread.started', 'thread_id': 'assigned'})
        self.proof['events'] = spec(path)
        archive = Path(self.proof['archive']['path'])
        with archive.open('ab') as stream: stream.write(b'changed')
        with self.assertRaisesRegex(ValueError, 'hash or size'): self.split()

    def test_timeout_descendant_is_never_rehabilitated(self):
        with self.queue._transaction() as db:
            data = json.loads(db.execute("SELECT payload FROM jobs WHERE id='family'").fetchone()[0])
            data['timeout_origin'] = {'job_id': 'older'}
            db.execute("UPDATE jobs SET payload=? WHERE id='family'", (json.dumps(data),))
        with self.assertRaisesRegex(ValueError, 'original four-to-six-rule author job'): self.split()

    def test_evidence_rechecked_before_transactional_split(self):
        real = recovery.verify
        def mutate(*args):
            result = real(*args)
            Path(self.proof['final']['path']).write_text('changed')
            return result
        with patch.object(recovery, 'verify', side_effect=mutate):
            with self.assertRaisesRegex(ValueError, 'hash or size'): self.split()
        self.assertEqual(self.queue.coverage()['counts'], {'needs-review': 6})

    def test_recheck_catches_checkpoint_race(self):
        real = recovery.verify
        def move(*args):
            result = real(*args)
            self.queue.add_checkpoint('later', {'inputs': 'newer'}, expected_parent='new')
            return result
        with patch.object(recovery, 'verify', side_effect=move):
            with self.assertRaisesRegex(ValueError, 'changed during'): self.split()
        self.assertEqual(self.queue.coverage()['counts'], {'needs-review': 6})


if __name__ == '__main__': unittest.main()
