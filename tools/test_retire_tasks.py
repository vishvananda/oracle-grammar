import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import uuid
from unittest.mock import patch

import retire as r
import retire_tasks as tasker
import retire_queue


class TaskTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name)
        self.source = self.base/'source'
        self.source.mkdir()
        for directory in ('grammar/syntax', 'grammar/tail', 'families', 'tests/fixtures'):
            (self.source/directory).mkdir(parents=True)
        for path in ('grammar/syntax/mined.ebnf', 'grammar/tail/mined.ebnf',
                     'grammar/syntax/actions.ebnf'):
            (self.source/path).write_text('original\n')
        (self.source/'.gitignore').write_text('.cache/\n')
        self.git('init', '-q')
        self.git('config', 'user.email', 'tests@example.invalid')
        self.git('config', 'user.name', 'Test')
        self.git('add', '.')
        self.git('commit', '-qm', 'base')
        self.commit = self.git('rev-parse', 'HEAD')
        (self.source/'.cache').mkdir()
        (self.source/'.cache/ignored').write_text('private')
        self.campaign = self.base/'campaign'
        runner = self.campaign/'runner'
        runner.mkdir(parents=True)
        here = Path(tasker.__file__).parent
        for name in tasker.RUNNERS:
            shutil.copy2(here/name, runner/name)
        self.helpers = self.base/'helpers'
        self.helpers.mkdir()
        for name in tasker.HELPERS:
            (self.helpers/name).write_text(name)
        build = {'source_sha256': 'compiled', 'binaries': {n: r.digest(self.helpers/n) for n in tasker.HELPERS}}
        r.write(self.helpers/'build.json', build)
        self.inputs = self.base/'inputs'
        frozen = self.inputs/'frozen'
        frozen.mkdir(parents=True)
        r.write(frozen/'build.json', build)
        r.write(frozen/'index.json', {'0': {'name': 'A', 'record': 0, 'face': 0, 'flags': [True]*4},
                                      '1': {'name': 'B', 'record': 1, 'face': 0, 'flags': [False]*4}})
        r.jsonl(frozen/'gold.jsonl', [{'faces': [{'name': 'A', 'abilities': []}]},
                                      {'faces': [{'name': 'B', 'abilities': []}]}])
        r.write(frozen/'oracle.json', [{'cards': [{'name': 'A', 'text': 'A text'}]},
                                        {'cards': [{'name': 'B', 'text': 'B text'}]}])
        r.write(self.inputs/'manifest.json', {'format': 3, 'source': 'compiled', 'commit': self.commit,
                                              'files': r.tree(frozen), 'tooling': r.tooling()})
        self.checkpoint = {'commit': self.commit, 'source': self.source, 'inputs': self.inputs,
                           'helpers': self.helpers, 'baseline': self.base/'baseline'}
        self.job = {'id': 'job-123', 'rules': ['m_one'], 'goal': 'test',
                    'target_faces': ['A'], 'allowedwritepaths': ['grammar/syntax/actions.ebnf'],
                    'source_fragments': [{'name': 'm_one', 'sem': 'example'}]}
        self.cohort = {'names': ['A', 'B'], 'sampled': False}
        self.patch = patch.object(tasker.dev, 'source', return_value='compiled')
        self.patch.start()

    def tearDown(self):
        self.patch.stop()
        tasker._packet_data.cache_clear()
        self.tmp.cleanup()

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.source, text=True).strip()

    def prepare(self, attempt='aaaaaaaaaaaa4aaa8aaaaaaaaaaaaaaa'):
        return tasker.prepare_task(self.campaign, {'attempt_id': attempt, 'base_commit': self.commit,
                                                   'job': self.job}, self.checkpoint, self.job, self.cohort)

    def test_private_attempt_and_manifest(self):
        result = self.prepare()
        run, checkout = Path(result['run']), Path(result['checkout'])
        self.assertEqual(r.verify(run/'inputs')['commit'], self.commit)
        descriptor = r.read(run/'tasks/job-123.json')
        support = r.read(run/'support.json')
        self.assertEqual(support['tasks']['job-123'], r.digest(run/'tasks/job-123.json'))
        self.assertEqual(support['packet_sha256'], r.digest(run/'packet.json'))
        self.assertEqual(support['files'], r.tree(run/'support'))
        self.assertEqual(descriptor['rule_dispositions_required'], ['m_one'])
        self.assertEqual(descriptor['max_diagnostic_attempts'], 3)
        self.assertEqual(descriptor['model'], 'gpt-6.1-terra')
        self.assertEqual(descriptor['effort'], 'high')
        self.assertEqual(descriptor['cohort'], ['A', 'B'])
        packet = r.read(run/'packet.json')
        self.assertEqual([g['value']['faces'][0]['name'] for g in packet['gold_records']], ['A'])
        self.assertEqual([x['value']['cards'][0]['name'] for x in packet['oracle_records']], ['A'])
        self.assertEqual(packet['baseline'][0]['record'], 0)
        self.assertFalse((checkout/'.cache/ignored').exists())
        self.assertTrue((checkout/'.git').is_file())
        file = 'grammar/syntax/actions.ebnf'
        self.assertNotEqual(os.stat(checkout/file).st_ino, os.stat(self.source/file).st_ino)
        (checkout/file).write_text('changed')
        self.assertEqual((self.source/file).read_text(), 'original\n')
        for name in tasker.HELPERS:
            self.assertEqual(os.stat(run/'support/bin'/name).st_ino, os.stat(self.helpers/name).st_ino)
        other = self.prepare('bbbbbbbbbbbb4bbb8bbbbbbbbbbbbbbb')
        self.assertNotEqual(result['run'], other['run'])
        self.assertEqual(support, r.read(run/'support.json'))

    def test_reject_mismatched_identity_and_population(self):
        self.checkpoint['commit'] = 'wrong'
        with self.assertRaises(ValueError):
            self.prepare()
        self.checkpoint['commit'] = self.commit
        self.job['target_faces'] = ['outside']
        with self.assertRaises(ValueError):
            self.prepare()

    def test_real_queue_claim_uuid_prepares_task(self):
        queue = retire_queue.Queue(self.base/'queue.db')
        queue.add_jobs([self.job])
        queue.add_checkpoint(self.commit, {'verified': True})
        claim = queue.claim('worker-1')
        self.assertEqual(str(uuid.UUID(claim['attempt_id'])), claim['attempt_id'])
        result = tasker.prepare_task(self.campaign, claim, self.checkpoint, self.job, self.cohort)
        self.assertEqual(Path(result['run']).name, claim['attempt_id'])

    def test_missing_modules_marked_for_discovery(self):
        self.job['allowedwritepaths'] = ['grammar/syntax/nonexistent.ebnf']
        result = self.prepare()
        task = r.read(Path(result['run'])/'tasks/job-123.json')
        self.assertTrue(task['module_discovery_required'])
        self.assertIn('grammar/syntax/actions.ebnf', task['write_files'])
        self.assertNotIn('grammar/syntax/nonexistent.ebnf', task['write_files'])


if __name__ == '__main__':
    unittest.main()
