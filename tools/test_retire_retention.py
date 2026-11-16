import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import retire as r
import retire_retention as retention
from retire_queue import Queue


class RetentionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.origin = self.root/'origin'
        self.origin.mkdir()
        subprocess.run(['git', 'init', '-q'], cwd=self.origin, check=True)
        (self.origin/'grammar').mkdir()
        (self.origin/'grammar/shared.ebnf').write_text('original\n')
        (self.origin/'grammar/remove.ebnf').write_text('remove\n')
        (self.origin/'.gitignore').write_text('.cache/\n')
        subprocess.run(['git', 'add', '.'], cwd=self.origin, check=True)
        subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=test@invalid',
                        'commit', '-qm', 'base'], cwd=self.origin, check=True)
        self.commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=self.origin, text=True).strip()
        self.campaign = self.root/'campaign'
        self.campaign.mkdir()
        self.queue = Queue(self.campaign/'queue.sqlite')
        self.queue.add_jobs([{'id': 'job-one', 'rules': ['m_one']}])
        self.queue.add_checkpoint(self.commit, {'verified': True})
        self.claim = self.queue.claim('worker')
        self.run = self.campaign/'.cache/attempts'/self.claim['attempt_id']
        self.run.mkdir(parents=True)
        self.checkout = self.campaign/'workspaces'/self.claim['attempt_id']
        self.checkout.parent.mkdir()
        subprocess.run(['git', 'worktree', 'add', '--detach', str(self.checkout), self.commit],
                       cwd=self.origin, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        (self.run/'tasks').mkdir()
        self.taskid = 'job-one'
        self.descriptor = self.run/'tasks/job-one.json'
        r.write(self.descriptor, {'checkout': str(self.checkout), 'rules': ['m_one'],
                                  'hooks': '.cache/author-hooks.json',
                                  'write_files': ['grammar/shared.ebnf', 'grammar/remove.ebnf',
                                                  'families/new.json']})
        r.write(self.run/'support.json', {'tasks': {self.taskid: r.digest(self.descriptor)}})
        (self.run/'inputs').mkdir()
        r.write(self.run/'inputs/manifest.json', {'commit': self.commit})
        self.receipt = self.campaign/'.cache/receipts'/f"{self.claim['attempt_id']}.json"
        self.receipt.parent.mkdir(parents=True)
        r.write(self.receipt, {'run': str(self.run), 'task': self.taskid, 'base_commit': self.commit,
                               'report': 'review-evidence', 'dispositions': {}})

    def tearDown(self):
        self.tmp.cleanup()

    def finish(self):
        self.queue.finish(self.claim['attempt_id'], 'needs-review', {'receipt': str(self.receipt)},
                          {'m_one': {'status': 'needs-review', 'reason': 'unresolved', 'evidence': 'report'}})

    def test_archive_restores_changes_and_preserves_evaluation(self):
        (self.checkout/'grammar/shared.ebnf').write_text('changed\n')
        (self.checkout/'grammar/remove.ebnf').unlink()
        (self.checkout/'families').mkdir()
        (self.checkout/'families/new.json').write_text('{"test":true}')
        hooks = self.checkout/'.cache/author-hooks.json'
        hooks.parent.mkdir()
        hooks.write_text('[{"root":"ability","production":"shared"}]')
        diagnostics = self.checkout/'.cache/author-checks/attempt-1'
        (diagnostics/'grammar').mkdir(parents=True)
        (diagnostics/'report.json').write_text('{"passed":false}')
        (diagnostics/'grammar/huge.json').write_text('exclude this')
        evaluation = self.run/'evaluations/job-one'
        evaluation.mkdir(parents=True)
        (evaluation/'report.json').write_text('{"passed":false}')
        self.finish()
        result = retention.archive_checkout(self.run, self.taskid, self.receipt)
        self.assertTrue(result['checkout_removed'])
        self.assertFalse(self.checkout.exists())
        self.assertTrue((evaluation/'report.json').exists())
        with tarfile.open(result['archive'], 'r:gz') as archive:
            manifest = json.load(archive.extractfile('manifest.json'))
            self.assertEqual(manifest['deleted'], ['grammar/remove.ebnf'])
            self.assertEqual(archive.extractfile('files/grammar/shared.ebnf').read(), b'changed\n')
            self.assertEqual(archive.extractfile('files/families/new.json').read(), b'{"test":true}')
            self.assertIn('files/.cache/author-checks/attempt-1/report.json', archive.getnames())
            hook_copy = archive.extractfile('files/.cache/author-hooks.json').read()
            self.assertEqual(hook_copy, b'[{"root":"ability","production":"shared"}]')
            self.assertNotIn('files/.cache/author-checks/attempt-1/grammar/huge.json', archive.getnames())
            self.assertIn(b'grammar/shared.ebnf', archive.extractfile('candidate.patch').read())
            self.assertIn(b'grammar/remove.ebnf', archive.extractfile('candidate.patch').read())
            restored = self.root/'restored'
            subprocess.run(['git', 'worktree', 'add', '--detach', str(restored), self.commit],
                           cwd=self.origin, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            subprocess.run(['git', 'apply', '--binary', '-'], cwd=restored, check=True,
                           input=archive.extractfile('candidate.patch').read())
            (restored/'.cache').mkdir()
            (restored/'.cache/author-hooks.json').write_bytes(hook_copy)
            self.assertEqual((restored/'.cache/author-hooks.json').read_bytes(), hook_copy)
            (restored/'families').mkdir()
            (restored/'families/new.json').write_bytes(archive.extractfile('files/families/new.json').read())
            self.assertEqual((restored/'grammar/shared.ebnf').read_text(), 'changed\n')
            self.assertFalse((restored/'grammar/remove.ebnf').exists())
            self.assertEqual((restored/'families/new.json').read_text(), '{"test":true}')
        self.assertNotIn(str(self.checkout), subprocess.check_output(
            ['git', 'worktree', 'list', '--porcelain'], cwd=self.origin, text=True))

    def test_hooks_without_evaluation_and_unsafe_paths(self):
        (self.checkout/'.cache').mkdir()
        (self.checkout/'.cache/author-hooks.json').write_text('[{"root":"ability"}]')
        task = r.read(self.descriptor)
        files, modes, _, _ = retention._snapshot(self.checkout, task)
        self.assertEqual(files['.cache/author-hooks.json'], b'[{"root":"ability"}]')
        self.assertIn('.cache/author-hooks.json', modes)
        for bad in ('../outside', '/tmp/outside', '.cache/../outside', 'grammar/shared.ebnf', '.cache'):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                retention._snapshot(self.checkout, dict(task, hooks=bad))
        (self.checkout/'.cache/author-hooks.json').unlink()
        (self.checkout/'.cache/author-hooks.json').symlink_to(self.checkout/'grammar/shared.ebnf')
        with self.assertRaises(ValueError):
            retention._snapshot(self.checkout, task)

    def test_refuses_running_and_unmatched_receipts(self):
        with self.assertRaises(ValueError):
            retention.archive_checkout(self.run, self.taskid, self.receipt)
        self.assertTrue(self.checkout.exists())
        self.finish()
        receipt = r.read(self.receipt)
        receipt['task'] = 'different'
        r.write(self.receipt, receipt)
        with self.assertRaises(ValueError):
            retention.archive_checkout(self.run, self.taskid, self.receipt)
        self.assertTrue(self.checkout.exists())

    def test_refuses_outside_changes_and_failed_verification(self):
        self.finish()
        (self.checkout/'unexpected').write_text('protected')
        with self.assertRaises(ValueError):
            retention.archive_checkout(self.run, self.taskid, self.receipt)
        (self.checkout/'unexpected').unlink()
        with patch.object(retention, '_verify_archive', side_effect=ValueError('verification failed')):
            with self.assertRaises(ValueError):
                retention.archive_checkout(self.run, self.taskid, self.receipt)
        self.assertTrue(self.checkout.exists())
        self.assertFalse(list((self.campaign/'.cache/archives').glob('*.tar.gz')))


if __name__ == '__main__':
    unittest.main()
