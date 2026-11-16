from contextlib import closing
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import unittest

from retire_queue import Queue
import retire_review as exporter


class ReviewTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root/'.cache').mkdir()
        self.queue = Queue(self.root/'queue.sqlite')
        self.queue.add_checkpoint('base', 'proof')
        self.queue.add_jobs([{'id': 'a', 'kind': 'author', 'rules': ['r1', 'r2'],
                              'primarymodules': ['actions'], 'semantic_atoms': ['CopyOf'],
                              'source_fragments': [{'sem': 'Ability::Triggered{}',
                                                    'body': 'card text', 'provenance': 'hidden text'}]},
                             {'id': 'b', 'kind': 'author', 'rules': ['r3']}])

    def test_current_owned_rules_artifacts_and_prior_exclusion(self):
        old = self.queue.claim('first', kinds=['author'])
        outcomes = {'r1': {'status': 'needs-review', 'reason': 'old reason', 'evidence': 'x'},
                    'r2': {'status': 'needs-review', 'reason': 'old reason', 'evidence': 'x'}}
        self.queue.finish(old['attempt_id'], 'needs-review', 'old', outcomes)
        with closing(sqlite3.connect(self.root/'queue.sqlite', isolation_level=None)) as db:
            db.execute("UPDATE jobs SET status='pending',attempt_id=NULL WHERE id='a'")
        current = self.queue.claim('second', kinds=['author'])
        attempt = current['attempt_id']
        report = self.root/'.cache/attempts'/attempt/'evaluations/job/report.json'
        report.parent.mkdir(parents=True)
        report.write_text('{"passed":false}')
        archive = self.root/'.cache/archives'/f'{attempt}.tar.gz'
        archive.parent.mkdir(parents=True)
        archive.write_bytes(b'candidate')
        (self.root/'.cache/attempts'/attempt/'archive.json').write_text(json.dumps(
            {'archive': str(archive), 'sha256': hashlib.sha256(b'candidate').hexdigest()}))
        dispositions = {'r1': {'status': 'proposed', 'reason': 'shared syntax'},
                        'r2': {'status': 'needs-review', 'reason': 'native binding needed'}}
        outcomes = {name: {'status': 'needs-review', 'reason': 'bundle gate failed', 'evidence': 'x'}
                    for name in ('r1', 'r2')}
        self.queue.finish(attempt, 'needs-review', {'report': str(report), 'dispositions': dispositions}, outcomes)
        provisional = self.queue.claim('third', kinds=['author'])
        self.queue.finish(provisional['attempt_id'], 'provisional-pass', 'ok',
                          {'r3': {'status': 'pass', 'evidence': 'x'}})
        result = exporter.review(self.root)
        self.assertEqual(result['latest_checkpoint'], 'base')
        self.assertEqual(result['rule_count'], 2)
        job = result['jobs'][0]
        self.assertEqual(job['attempt_id'], attempt)
        self.assertEqual(job['base_checkpoint'], 'base')
        self.assertEqual(job['semantic_roots'], ['Ability'])
        self.assertEqual(job['report']['sha256'], hashlib.sha256(report.read_bytes()).hexdigest())
        self.assertEqual(job['candidate_archive']['status'], 'verified')
        self.assertEqual(job['rules'][0]['assessment'], 'model-proposed-bundle-not-passed')
        self.assertEqual(job['rules'][1]['model_reason'], 'native binding needed')
        self.assertIn('native-binding/Rust-contract', job['rules'][1]['suggested_lanes'])
        self.assertNotIn('old reason', json.dumps(result))
        self.assertNotIn('card text', json.dumps(result))
        self.assertNotIn('hidden text', json.dumps(result))

    def test_rejected_provisional_and_cli_output_guard(self):
        claim = self.queue.claim('worker')
        self.queue.finish(claim['attempt_id'], 'provisional-pass', 'proof',
                          {rule: {'status': 'pass', 'evidence': 'x'} for rule in claim['job']['rules']})
        identity = 'a'*32
        folder = self.root/'.cache/batches'/identity
        folder.mkdir(parents=True)
        manifest = folder/'manifest.json'
        manifest.write_text(json.dumps({'jobs': [{'job_id': claim['job']['id'], 'reason': 'family conflict'},
                                                  {'job_id': 'other', 'reason': 'other reason'}]}))
        (folder/'validation.json').write_text(json.dumps({'reason': 'native validation failed'}))
        self.queue.reject_jobs([claim['job']['id']], {'manifest': identity, 'reason': 'batch gate'})
        result = exporter.review(self.root)
        self.assertEqual(result['rule_count'], 2)
        job = result['jobs'][0]
        self.assertEqual(job['integration_reason'], 'batch gate')
        self.assertEqual(job['integration_job_reason'], 'family conflict')
        self.assertEqual(job['integration_validation_reason'], 'native validation failed')
        self.assertEqual(job['integration_manifest']['sha256'], hashlib.sha256(manifest.read_bytes()).hexdigest())
        self.assertEqual(exporter._integration(self.root, '../escape', claim['job']['id']), (None, None, None))
        self.assertTrue(all('integration/tooling' in row['suggested_lanes'] for row in result['jobs'][0]['rules']))
        script = Path(exporter.__file__)
        target = self.root/'.cache/review.json'
        subprocess.run([sys.executable, str(script), str(self.root), '--output', str(target)], check=True)
        self.assertEqual(json.loads(target.read_text())['rule_count'], 2)
        rejected = subprocess.run([sys.executable, str(script), str(self.root), '--output', str(self.root/'bad')],
                                  capture_output=True)
        self.assertNotEqual(rejected.returncode, 0)

    def test_mechanical_review_is_not_labeled_as_model(self):
        self.queue.add_jobs([{'id': 'mechanical', 'kind': 'mechanical', 'rules': ['m1']}])
        claim = self.queue.claim('mechanic', kinds=['mechanical'])
        self.queue.finish(claim['attempt_id'], 'needs-review',
                          {'dispositions': {'m1': {'status': 'needs-review', 'reason': 'check'}}},
                          {'m1': {'status': 'needs-review', 'reason': 'check', 'evidence': 'proof'}})
        row = exporter.review(self.root)['jobs'][0]['rules'][0]
        self.assertEqual(row['assessment'], 'mechanical-needs-review')


if __name__ == '__main__':
    unittest.main()
