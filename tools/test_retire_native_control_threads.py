"""Native-only concurrency must match its kernel leases and recorded identity."""
from pathlib import Path
import json
import os
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import retire as r
import retire_batch as batch
import retire_dispatch as dispatch
from test_retire_batch import control_fixture


class NativeControlThreadTests(unittest.TestCase):
    def test_default_configured_and_small_pool_use_exact_leases_and_helper_threads(self):
        for configured, capacity, expected in ((None, 32, 6), (12, 32, 12), (16, 16, 16), (1, 1, 1)):
            with self.subTest(configured=configured), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                cp = control_fixture(root, 'base'); cp['cpu_capacity'] = capacity
                if configured is not None: cp['native_control_threads'] = configured
                control = batch._control_setup(cp)
                self.assertEqual(control['threads'], expected)
                def checks(run, directory, grammar, names, data, threads):
                    self.assertEqual(threads, expected)
                    self.assertEqual(len(r._LEASES), expected)
                    for fd in r._LEASES: os.fstat(fd)
                    self.assertEqual(names, ['A', 'B'])
                    return {'faces': 2, 'failures': []}
                with patch.object(batch.retire_cpu, 'reserve', wraps=batch.retire_cpu.reserve) as reserve, \
                        patch.object(batch.w, 'checks', side_effect=checks):
                    batch._control_check(cp, control, root/'grammar', root/'result')
                self.assertEqual(reserve.call_args.args, (Path(cp['cpu_pool']), expected, capacity))
                self.assertEqual(r.read(root/'result/result.json'), {'faces': 2, 'failures': []})
                self.assertEqual(r._LEASES, [])

    def test_invalid_threads_and_capacity_refuse_before_native_execution(self):
        invalid = [(value, 32) for value in (True, False, 0, -1, 17, 1.5, '12', None)]
        invalid += [(12, 8), (6, True), (6, 0), (6, 33)]
        for threads, capacity in invalid:
            with self.subTest(threads=threads, capacity=capacity), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary); cp = control_fixture(root, 'base')
                cp.update(native_control_threads=threads, cpu_capacity=capacity)
                with patch.object(batch.w, 'checks') as checks, self.assertRaisesRegex(ValueError, 'native_control_threads'):
                    batch._control_setup(cp)
                checks.assert_not_called()

    def test_thread_change_after_setup_is_not_silently_misreported(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); cp = control_fixture(root, 'base')
            cp['native_control_threads'] = 12
            control = batch._control_setup(cp); cp['native_control_threads'] = 6
            with patch.object(batch.w, 'checks') as checks, self.assertRaisesRegex(ValueError, 'identity changed'):
                batch._control_check(cp, control, root/'grammar', root/'result')
            self.assertFalse((root/'result').exists()); checks.assert_not_called()

    def test_manifest_native_threads_match_execution_and_family_default_stays_six(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); repo = root/'repo'
            (repo/'grammar/syntax').mkdir(parents=True); (repo/'tests').mkdir()
            (repo/'grammar/syntax/mined.ebnf').write_text('m_rule ::= "rule"\n')
            (repo/'tests/families.json').write_text('[]')
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            subprocess.run(['git', 'add', '.'], cwd=repo, check=True)
            subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=t@x',
                            'commit', '-qm', 'base'], cwd=repo, check=True)
            commit = batch._git(repo, 'rev-parse', 'HEAD')
            cp = dict(control_fixture(root, commit), source=str(repo), native_control_threads=12, audit_threads=16)
            calls = []
            def checks(run, directory, grammar, names, data, threads):
                calls.append(threads)
                self.assertEqual(len(r._LEASES), 12)
                return {'faces': 2, 'failures': []}
            with patch.object(batch, '_proposal', side_effect=ValueError('parked test proposal')), \
                    patch.object(batch.w, 'checks', side_effect=checks):
                result = batch.stage(root, cp, [{'job_id': 'job', 'attempt_id': 'attempt'}])
            self.assertEqual(result['status'], 'no-proposals')
            self.assertEqual(calls, [12])
            self.assertEqual(result['native_regression_controls']['threads'], 12)
            self.assertEqual(result['family_check_threads'], 6)
            self.assertEqual(cp['audit_threads'], 16)

    def test_dispatch_runtime_override_and_disabling_restore_six(self):
        for configured, expected in ((None, 6), (12, 12)):
            with self.subTest(configured=configured), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                config = {'initial_commit': 'base', 'first_batch_size': 12, 'batch_size': 24,
                          'audit_threads': 16}
                if configured is not None: config['native_control_threads'] = configured
                r.write(root/'config.json', config)
                cp = {'commit': 'base', 'native_control_threads': 12, 'cpu_capacity': 32}
                queue = SimpleNamespace(coverage=lambda: {'checkpoint': 'base'})
                with patch.object(dispatch, 'Queue', return_value=queue), \
                        patch.object(dispatch.campaign_api, 'checkpoint', return_value=cp), \
                        patch.object(dispatch, 'proposals', return_value=[{'job_id': 'job'}]), \
                        patch.object(batch, 'stage', return_value={'jobs': [], 'status': 'no-proposals'}) as stage:
                    dispatch._batch(root)
                self.assertEqual(stage.call_args.args[1]['native_control_threads'], expected)
                self.assertEqual(stage.call_args.args[1]['audit_threads'], 16)
                self.assertEqual(cp['native_control_threads'], 12)

    def test_invalid_config_is_rejected_even_without_a_native_profile(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            r.write(root/'config.json', {'native_control_threads': True})
            queue = SimpleNamespace(coverage=lambda: {'checkpoint': 'base'})
            with patch.object(dispatch, 'Queue', return_value=queue), \
                    patch.object(dispatch.campaign_api, 'checkpoint', return_value={'commit': 'base'}), \
                    patch.object(batch, 'stage') as stage, self.assertRaisesRegex(ValueError, 'native_control_threads'):
                dispatch._batch(root)
            stage.assert_not_called()


if __name__ == '__main__': unittest.main()
