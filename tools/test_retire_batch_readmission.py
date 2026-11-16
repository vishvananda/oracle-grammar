import fcntl
import hashlib
import json
from contextlib import closing
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from retire_queue import Queue


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value)+'\n')


def spec(path):
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


class BatchReadmissionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.queue = Queue(self.root/'queue.sqlite')
        self.queue.add_checkpoint('parent', 'proof')
        self.queue.add_jobs([{'id': 'author', 'kind': 'author', 'rules': ['a']},
                             {'id': 'mechanical', 'kind': 'mechanical', 'rules': ['b']},
                             {'id': 'culprit', 'kind': 'author', 'rules': ['c']}])
        self.items, self.reports = [], {}
        for job in ('author', 'mechanical', 'culprit'):
            claim = self.queue.claim('worker-'+job)
            attempt = claim['attempt_id']
            child = subprocess.Popen(['true'])
            child.wait()
            self.queue.mark_started(attempt, pid=child.pid)
            lease = self.root/'leases'/job
            lease.parent.mkdir(exist_ok=True)
            lease.touch()
            run = self.root/'.cache/attempts'/attempt
            directory = run/'evaluations'/job/'attempt-1'
            directory.mkdir(parents=True)
            descriptor = run/'tasks'/f'{job}.json'
            write(descriptor, {'id': job, 'rules': claim['job']['rules']})
            support = run/'support.json'
            write(support, {'tasks': {job: spec(descriptor)['sha256']}})
            grammar = directory/'grammar'
            grammar.mkdir()
            (grammar/'test.ebnf').write_text('test ::= "a"')
            family = directory/'family.json'
            write(family, {'family': 'test', 'cases': []})
            report = directory/'report.json'
            data = {'task': job, 'source': 'parent', 'directory': str(directory), 'passed': True,
                    'failures': [], 'faces': 1, 'descriptor_sha256': spec(descriptor)['sha256'],
                    'support_sha256': spec(support)['sha256'], 'grammar': {'test.ebnf': spec(grammar/'test.ebnf')['sha256']}}
            if job == 'mechanical':
                data.update(kind='mechanical', hooks=[])
            else:
                data.update(fixture_sha256=spec(family)['sha256'], family_returncode=0)
            write(report, data)
            self.reports[job] = report
            rules = claim['job']['rules']
            decision = {rule: {'status': 'proposed', 'reason': 'reviewed', 'evidence': 'proof'} for rule in rules}
            if job != 'mechanical':
                for rule in rules:
                    decision[rule]['rule'] = rule
            self.queue.finish(attempt, 'provisional-pass', {'kind': claim['job']['kind'], 'report': str(report),
                              'dispositions': decision}, {rule: {'status': 'pass', 'evidence': 'proof'} for rule in rules})
            self.items.append({'job_id': job, 'attempt_id': attempt, 'status': 'included',
                               'base_commit': 'parent', 'rules': rules, 'kind': claim['job']['kind'],
                               'descriptor_sha256': spec(descriptor)['sha256'], 'support_sha256': spec(support)['sha256']})
        folder = self.root/'.cache/batches/batch'
        folder.mkdir(parents=True)
        manifest = folder/'manifest.json'
        write(manifest, {'id': 'batch', 'base_commit': 'parent', 'commit': 'candidate', 'jobs': self.items})
        validation = {'status': 'failed', 'scope': 'combined-batch', 'reason': 'repository gates failed',
                      'repository_check_returncode': 1, 'candidate': 'candidate', 'source_checkpoint': 'parent',
                      'included_jobs': [item['job_id'] for item in self.items]}
        self.validation = folder/'validation.json'
        write(self.validation, validation)
        self.queue.reject_jobs([item['job_id'] for item in self.items], {'manifest': 'batch', 'validation': validation})
        self.log = folder/'repository-check.log'
        self.log.write_text(''.join(f'{name} {"FAILED" if name == "families" else "passed"} 1.0 s\n'
                                    for name in ('fmt', 'pin', 'hygiene', 'tools', 'clippy', 'build', 'families', 'test')))
        logs = {}
        for name in ('base_log', 'trial_log', 'without_modifier_log', 'probe_log'):
            path = self.root/'.cache'/f'{name}.log'
            path.write_text('recorded evidence')
            logs[name] = spec(path)
        self.isolation = self.root/'.cache/isolation.json'
        write(self.isolation, {'kind': 'isolated-ambiguity-regression', 'manifest': 'batch',
                               'candidate': 'candidate', 'parent': 'parent', 'culprit_job': 'culprit',
                               'family': 'shared-clauses', 'case': 'entry control', 'reason': 'isolated regression',
                               'evidence': logs})
        self.proof = {'kind': 'isolated-batch-family-revalidation', 'manifest': spec(manifest),
                      'validation': spec(self.validation), 'repository_log': spec(self.log),
                      'isolation': spec(self.isolation)}

    def attempt(self, name):
        return next(item['attempt_id'] for item in self.items if item['job_id'] == name)

    def readmit(self, name):
        proof = {**self.proof, 'report': spec(self.reports[name])}
        self.queue.readmit_batch_proposal(name, self.attempt(name), 'parent', proof)

    def test_author_and_mechanical_are_only_provisional_and_preserve_history(self):
        before = {name: self.queue.get_attempt(self.attempt(name)) for name in ('author', 'mechanical')}
        self.readmit('author')
        self.readmit('mechanical')
        coverage = self.queue.coverage()
        self.assertEqual(coverage['counts'], {'provisional-pass': 2, 'integration-rejected': 1})
        self.assertEqual(coverage['accepted_rules'], 0)
        for name, old in before.items():
            self.assertEqual(self.queue.get_attempt(self.attempt(name)), old)
        event = self.queue.audit()[-1]['data']
        self.assertTrue(event['proof']['requires_fresh_full_validation'])
        self.assertEqual(event['previous_adjudication']['status'], 'integration-rejected')
        with self.assertRaisesRegex(ValueError, 'culprit'):
            self.readmit('culprit')
        previous = json.loads((self.root/'.cache/batches/batch/validation.json').read_text())
        self.queue.reject_jobs(['author'], {'manifest': 'batch', 'validation': previous})
        with self.assertRaisesRegex(ValueError, 'already used'):
            self.readmit('author')

    def test_missing_dispositions_or_tampered_individual_report_are_rejected(self):
        attempt = self.attempt('author')
        with closing(self.queue._connect()) as db:
            raw = db.execute('SELECT result FROM attempts WHERE id=?', (attempt,)).fetchone()[0]
            value = json.loads(raw)
            del value['evidence']['dispositions']['a']
            db.execute('UPDATE attempts SET result=? WHERE id=?', (json.dumps(value), attempt))
        with self.assertRaisesRegex(ValueError, 'per-rule dispositions'):
            self.readmit('author')
        with closing(self.queue._connect()) as db:
            db.execute('UPDATE attempts SET result=? WHERE id=?', (raw, attempt))
        self.reports['author'].write_text('{}')
        with self.assertRaisesRegex(ValueError, 'did not pass'):
            self.readmit('author')
        self.assertEqual(self.queue.coverage()['accepted_rules'], 0)

    def test_failure_identity_and_process_guards(self):
        with (self.root/'leases/author').open('w') as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, 'still held'):
                self.readmit('author')
        with patch('retire_queue.os.kill', return_value=None):
            with self.assertRaisesRegex(ValueError, 'still present'):
                self.readmit('author')
        with self.assertRaisesRegex(ValueError, 'stale'):
            self.queue.readmit_batch_proposal('author', self.attempt('author'), 'wrong', self.proof)
        self.log.write_text(self.log.read_text().replace('tools passed', 'tools FAILED'))
        self.proof['repository_log'] = spec(self.log)
        with self.assertRaisesRegex(ValueError, 'not limited to families'):
            self.readmit('author')
        self.assertEqual(self.queue.coverage()['counts'], {'integration-rejected': 3})


if __name__ == '__main__':
    unittest.main()
