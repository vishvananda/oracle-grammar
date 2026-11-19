import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import retire as r
import retire_retention as retention
import retire_storage as storage
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

    def test_archived_edits_require_the_independent_digest_and_do_not_restore_checkout(self):
        task = r.read(self.descriptor)
        edit_name = 'fixtures/edits.json'
        task['edits'] = edit_name
        task['write_files'].append(edit_name)
        r.write(self.descriptor, task)
        r.write(self.run/'support.json', {'tasks': {self.taskid: r.digest(self.descriptor)}})
        edit_path = self.checkout/edit_name
        edit_path.parent.mkdir()
        edits = [{'face':'Card', 'set':[{'path':['count'], 'value':2}]}]
        r.write(edit_path, edits)
        checked = r.digest(edit_path)
        self.finish()
        archive = retention.archive_checkout(self.run, self.taskid, self.receipt)
        r.write(self.run/'archive.json', archive)
        self.assertEqual(retention.archived_edits(self.run, self.taskid, checked), edits)
        self.assertFalse(self.checkout.exists())
        with self.assertRaisesRegex(ValueError, 'independent evaluation'):
            retention.archived_edits(self.run, self.taskid, '0'*64)
        r.write(self.run/'archive.json', dict(archive, sha256='0'*64))
        with self.assertRaisesRegex(ValueError, 'checkout hash'):
            retention.archived_edits(self.run, self.taskid, checked)

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

    def test_cleanup_retries_an_interrupted_verified_archive(self):
        original = retention.subprocess.run

        def interrupt(command, *args, **kwargs):
            if command[0] == 'git' and 'remove' in command:
                raise subprocess.CalledProcessError(1, command)
            return original(command, *args, **kwargs)

        attempt = self.claim['attempt_id']
        (self.campaign/'leases').mkdir()
        (self.campaign/'leases'/self.taskid).touch()
        worker = subprocess.Popen(['true'])
        worker.wait()
        self.queue.mark_started(attempt, pid=worker.pid, receipt=str(self.receipt))
        self.queue.finish(attempt, 'needs-review',
                          {'receipt': str(self.receipt), 'run': str(self.run), 'task': self.taskid},
                          {'m_one': {'status': 'needs-review', 'reason': 'unresolved', 'evidence': 'report'}})
        path = self.checkout/'grammar/shared.ebnf'
        path.write_text('draft\n')
        with patch.object(retention.subprocess, 'run', side_effect=interrupt):
            with self.assertRaises(subprocess.CalledProcessError):
                retention.archive_checkout(self.run, self.taskid, self.receipt)
        archived, = (self.campaign/'.cache/archives').glob('*.tar.gz')
        digest = r.digest(archived)
        path.write_text('different draft\n')
        self.assertEqual(storage.clean(self.campaign, True)['removed'], [])
        self.assertTrue(self.checkout.exists())
        self.assertEqual(r.digest(archived), digest)
        path.write_text('draft\n')
        self.assertEqual(storage.clean(self.campaign, False)['eligible'], [attempt])
        self.assertTrue(self.checkout.exists())
        self.assertEqual(storage.clean(self.campaign, True)['removed'], [attempt])
        self.assertEqual(r.read(self.run/'archive.json')['sha256'], digest)
        self.assertFalse(self.checkout.exists())

    def test_cleanup_respects_live_process_and_worker_lease(self):
        attempt = self.claim['attempt_id']
        (self.campaign/'leases').mkdir()
        lease = self.campaign/'leases'/self.taskid
        lease.touch()
        self.queue.mark_started(attempt, pid=os.getpid(), receipt=str(self.receipt))
        self.assertEqual(storage.clean(self.campaign, True)['eligible'], [])
        self.queue.finish(attempt, 'needs-review',
                          {'receipt': str(self.receipt), 'run': str(self.run), 'task': self.taskid},
                          {'m_one': {'status': 'needs-review', 'reason': 'unresolved', 'evidence': 'report'}})
        with r.locked(lease):
            locked = storage.clean(self.campaign, True)
            self.assertIn('lease', locked['retained'][0]['reason'])
        live = storage.clean(self.campaign, True)
        self.assertIn('process is still present', live['retained'][0]['reason'])
        self.assertTrue(self.checkout.exists())

    def test_cleanup_can_run_before_the_first_checkout(self):
        prepared = self.root/'prepared'
        prepared.mkdir()
        Queue(prepared/'queue.sqlite')
        with patch('sys.argv', ['retire_storage.py', str(prepared), '--apply']), patch('builtins.print'):
            storage.main()
        self.assertEqual(r.read(prepared/'.cache/storage/worktree-cleanup.json'),
                         {'applied': True, 'eligible': [], 'removed': [], 'retained': []})


if __name__ == '__main__':
    unittest.main()
