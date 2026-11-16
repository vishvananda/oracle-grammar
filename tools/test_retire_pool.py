"""Model pools reuse evidence, never acceptance, and have fixed disjoint jobs."""
from argparse import Namespace
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

import retire as r
import retire_pool as p


class PoolTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.run = Path(self.tmp.name)/'epoch'
        (self.run/'scan').mkdir(parents=True)
        (self.run/'jobs').mkdir()
        r.write(self.run/'scan/rules.json', {'unused': ['m_a', 'm_b', 't_c'],
                                            'needed': {'m_d': ['D']}})
        self.manifest = {'slots': 2, 'files': {'x': 'y'}, 'source': 'frozen'}
        (self.run/'manifest.json').write_text('{}')

    def test_jobs_are_partitioned_once_and_models_cannot_import_prior_acceptance(self):
        captured = []
        def launch(run, assignment, model, effort):
            captured.extend(assignment['rules'])
            return {'returncode': 0}
        with patch.object(r, 'verify', return_value=self.manifest), \
                patch.object(p, 'launch', side_effect=launch), \
                patch.object(p, 'summarize', return_value={
                    'remaining': 0, 'provisional_passes': 0, 'escalations': []}):
            p.run_pool(Namespace(run=self.run, workers=2, prior=None,
                                 model='gpt-6-luna', effort='max', validate=False))
        self.assertCountEqual(captured, ['m_a', 'm_b', 't_c'])
        self.assertEqual(len(captured), len(set(captured)))
        self.assertEqual(r.read(self.run/'pool/inventory.json')[-1]['lane'], 'shared-grammar')
        with patch.object(r, 'verify', return_value=self.manifest):
            with self.assertRaises(FileExistsError):
                p.run_pool(Namespace(run=self.run, workers=2))

    def test_cross_source_prior_evidence_is_rejected(self):
        with patch.object(r, 'verify', side_effect=[{'files': {}}, self.manifest]):
            with self.assertRaisesRegex(ValueError, 'different inputs'):
                p.prior_findings(Path('/prior'), self.run)

    def test_pending_results_do_not_establish_acceptance_and_failed_gates_escalate(self):
        (self.run/'pool').mkdir()
        for name in ('m_a', 'm_b'):
            folder = self.run/'jobs'/name
            folder.mkdir()
            r.write(folder/'job.json', {'rule': name})
        r.write(self.run/'jobs/m_b/result.json', {'status': 'needs-review',
                'failures': [{'face': 'B', 'lost': ['exact_default']}]})
        result = p.summarize(self.run)
        self.assertEqual(result['remaining'], 1)
        self.assertEqual(result['provisional_passes'], 0)
        self.assertEqual(result['escalations'][0]['lane'], 'larger-model')
        self.assertEqual(result['escalations'][0]['failures'][0]['lost'], ['exact_default'])

    def test_large_cohort_and_operational_errors_do_not_automatically_need_a_larger_model(self):
        (self.run/'pool').mkdir()
        for name, result in [('m_a', {'status': 'needs-review', 'reason': 'large cohort'}),
                             ('m_b', {'status': 'failed', 'reason': 'helper execution'})]:
            folder = self.run/'jobs'/name
            folder.mkdir()
            r.write(folder/'job.json', {'rule': name})
            r.write(folder/'result.json', result)
        entries = p.summarize(self.run)['escalations']
        self.assertEqual([e['lane'] for e in entries],
                         ['dependency-or-broader-validation', 'operational'])

    def test_worker_uses_fixed_model_and_explicit_write_root(self):
        folder = self.run/'pool/luna-01'
        cmd = p.command(self.run, folder, 'gpt-6-luna', 'max')
        self.assertEqual(cmd[cmd.index('--model')+1], 'gpt-6-luna')
        self.assertEqual(cmd[cmd.index('--add-dir')+1], str(self.run))
        self.assertIn('model_reasoning_effort="max"', cmd)
        text = p.prompt(self.run, ['m_a'], 'luna-01', self.run/'knowledge.json')
        command_line = next(line for line in text.splitlines() if line.startswith('['))
        cli = json.loads(command_line)
        self.assertEqual(cli[cli.index('--rules')+1], 'm_a')
        self.assertEqual(cli[cli.index('--limit')+1], '1')

    def test_startup_serializes_initialization_and_preserves_one_model_process(self):
        (self.run/'pool').mkdir()
        process = Mock(returncode=0)
        process.poll.return_value = None
        def spawn(cmd, **kwargs):
            kwargs['stdout'].write('{"type":"thread.started","thread_id":"test"}\n')
            kwargs['stdout'].flush()
            return process
        process.pid = 123
        with patch.object(p.subprocess, 'Popen', side_effect=spawn) as start:
            result = p.launch(self.run, {'name': 'luna-01', 'rules': ['m_a']},
                              'gpt-6-luna', 'max')
        self.assertEqual(start.call_count, 1)
        process.wait.assert_called_once()
        self.assertEqual(result['status'], 'completed')
        self.assertEqual(result['returncode'], 0)


if __name__ == '__main__':
    unittest.main()
