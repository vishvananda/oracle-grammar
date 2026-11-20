import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import retire as r
import retire_batch as batch
import retire_speculative as speculative


def git(root, *args):
    return subprocess.check_output(['git', *args], cwd=root, text=True).strip()


class SpeculativeStageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.run = Path(self.temp.name)/'campaign'
        self.repo = Path(self.temp.name)/'repo'
        (self.repo/'grammar').mkdir(parents=True)
        (self.repo/'grammar/a.ebnf').write_text('a ::= "a"\n')
        git(self.repo, 'init', '-q')
        git(self.repo, '-c', 'user.name=t', '-c', 'user.email=t@localhost', 'add', '.')
        git(self.repo, '-c', 'user.name=t', '-c', 'user.email=t@localhost', 'commit', '-qm', 'base')
        self.base = git(self.repo, 'rev-parse', 'HEAD')
        self.batch_id = 'b'*32
        self.folder = self.run/'.cache/batches'/self.batch_id
        self.folder.mkdir(parents=True)
        source = self.folder/'source'
        git(self.repo, 'worktree', 'add', '-q', '-b', 'spec', str(source), self.base)
        (source/'grammar/a.ebnf').write_text('a ::= "b"\n')
        git(source, '-c', 'user.name=t', '-c', 'user.email=t@localhost', 'commit', '-qam', 'stage')
        self.inputs = Path(self.temp.name)/'inputs'
        self.helpers = Path(self.temp.name)/'helpers'
        self.manifest = {'id': self.batch_id, 'folder': str(self.folder), 'source': str(source),
                         'status': 'provisional', 'base_commit': self.base, 'combine_selected': True,
                         'commit': git(source, 'rev-parse', 'HEAD'), 'grammar': r.tree(source/'grammar'),
                         'jobs': [{'job_id': 'j1', 'attempt_id': 'a1', 'status': 'included'},
                                  {'job_id': 'j2', 'attempt_id': 'a2', 'status': 'rejected'}],
                         'native_regression_controls': {'profile_sha256': 'n'*64},
                         'speculative': {'parent_candidate': self.base, 'inputs': str(self.inputs),
                                         'helpers': str(self.helpers)}}
        r.write(self.folder/'manifest.json', self.manifest)
        self.checkpoint = {'commit': self.base, 'inputs': str(self.inputs), 'helpers': str(self.helpers),
                           'native_regression_controls': {'path': '/profile', 'sha256': 'n'*64}}
        self.proposals = [{'job_id': 'j1', 'attempt_id': 'a1'}, {'job_id': 'j2', 'attempt_id': 'a2'}]

    def test_adopts_only_the_exact_selection_once(self):
        mismatches = [
            dict(self.checkpoint, commit='0'*40),
            dict(self.checkpoint, inputs=str(self.inputs)+'-other'),
            dict(self.checkpoint, native_regression_controls={'path': '/profile', 'sha256': 'm'*64}),
            dict(self.checkpoint, library_regression_controls={'path': '/library', 'sha256': 'l'*64}),
        ]
        for checkpoint in mismatches:
            with self.subTest(checkpoint=checkpoint):
                self.assertIsNone(batch.adopt_staged(self.run, checkpoint, self.proposals, self.batch_id))
        self.assertIsNone(batch.adopt_staged(self.run, self.checkpoint, self.proposals[::-1], self.batch_id))
        self.assertIsNone(batch.adopt_staged(self.run, self.checkpoint, self.proposals, 'not-an-id'))
        adopted = batch.adopt_staged(self.run, self.checkpoint, self.proposals, self.batch_id)
        self.assertEqual(adopted['adopted']['checkpoint'], self.base)
        self.assertIsNone(batch.adopt_staged(self.run, self.checkpoint, self.proposals, self.batch_id))

    def test_refuses_a_changed_or_dirty_private_branch(self):
        source = self.folder/'source'
        (source/'grammar/a.ebnf').write_text('a ::= "c"\n')
        self.assertIsNone(batch.adopt_staged(self.run, self.checkpoint, self.proposals, self.batch_id))
        git(source, 'checkout', '-q', '--', '.')
        r.write(self.folder/'manifest.json', dict(self.manifest, speculative=None))
        self.assertIsNone(batch.adopt_staged(self.run, self.checkpoint, self.proposals, self.batch_id))

    def receipt(self, **fields):
        value = {'checkpoint': self.base, 'jobs': [{'job': 'j1', 'attempt': 'a1'}], **fields}
        r.write(self.run/speculative.RECEIPT, value)

    def test_ready_matches_exact_receipt_and_bounds_its_wait(self):
        self.assertIsNone(speculative.ready(self.run, self.base, [('j1', 'a1')]))
        self.receipt(status='staged', manifest=self.batch_id, sha256=r.digest(self.folder/'manifest.json'))
        self.assertEqual(speculative.ready(self.run, self.base, [('j1', 'a1')]), self.batch_id)
        self.assertIsNone(speculative.ready(self.run, self.base, [('j1', 'other')]))
        self.assertIsNone(speculative.ready(self.run, 'f'*40, [('j1', 'a1')]))
        self.receipt(status='staged', manifest=self.batch_id, sha256='0'*64)
        self.assertIsNone(speculative.ready(self.run, self.base, [('j1', 'a1')]))
        child = subprocess.Popen(['true'])
        child.wait()
        self.receipt(status='staging', pid=child.pid)
        self.assertIsNone(speculative.ready(self.run, self.base, [('j1', 'a1')], wait_seconds=60, poll_seconds=0))
        self.receipt(status='staging', pid=os.getpid())
        self.assertIsNone(speculative.ready(self.run, self.base, [('j1', 'a1')], wait_seconds=0))

    def test_predicts_only_a_preflight_lane_for_the_candidate(self):
        plan = self.run/'plan.json'
        r.write(plan, {'kind': 'prospective-retirement-batch', 'checkpoint': 'c'*40,
                       'jobs': [{'job': 'j1', 'attempt': 'a1'}]})
        spec = {'path': str(plan), 'sha256': r.digest(plan)}
        config = {'batch_size': 48, 'verified_ready_batch_size': 72, 'batch_preflight_selection': spec}
        candidate = {'commit': 'c'*40}
        state = {'lane': 'preflight-verified', 'batches_since_stale': 0}
        self.assertEqual(speculative.next_selection(self.run, config, state, candidate), (spec, [('j1', 'a1')]))
        self.assertIsNone(speculative.next_selection(self.run, config, state, {'commit': 'd'*40}))
        stale = dict(config, stale_provisional_hours=24, stale_provisional_every=3)
        self.assertEqual(speculative.next_selection(self.run, stale, state, candidate)[1], [('j1', 'a1')])
        self.assertIsNone(speculative.next_selection(self.run, stale, dict(state, batches_since_stale=1), candidate))


if __name__ == '__main__':
    unittest.main()
