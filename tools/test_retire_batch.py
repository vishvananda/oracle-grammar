import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import retire_batch as batch


class BatchTests(unittest.TestCase):
    def test_edited_family_gold_is_independently_reconstructed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            frozen, evaluation = root/'inputs/frozen', root/'evaluation'
            frozen.mkdir(parents=True)
            (evaluation/'edited-0').mkdir(parents=True)
            (evaluation/'family.json').write_text(json.dumps({'family': 'test', 'cases': [
                {'name': 'positive', 'expected': [{'kind': 'Original'}]},
                {'name': 'reject', 'reject': True}, {'name': 'other'},
                {'name': 'explicit', 'compositional': True}]}))
            original = {'faces': [{'name': 'Example', 'characteristics': {'types': ['Instant']}, 'abilities': [{'amount': 1}]}]}
            edited = {'faces': [{'name': 'Example', 'characteristics': {'types': ['Instant']}, 'abilities': [{'amount': 2}]}]}
            (frozen/'gold.jsonl').write_text(json.dumps(original)+'\n')
            (evaluation/'edited-0/gold.jsonl').write_text(json.dumps(edited)+'\n')
            (evaluation/'edited-0/rows.jsonl').write_text(json.dumps({'name': 'Example', 'text': 'two', 'status': 'equal'})+'\n')
            report = {'edited': [{'face': 'Example', 'mutations': [{'path': [0, 'amount'], 'value': 2}], 'flags': [True, False, True, False]}]}
            family = batch._family(evaluation, report, root)
            case = family['cases'][-1]
            self.assertEqual(case['expected'], [{'amount': 2}])
            self.assertFalse(case['exact'])
            self.assertEqual(case['context'], {'types': ['Instant']})
            self.assertNotIn('compositional', case)
            modes = batch._family(evaluation, report, root, modes=True)
            self.assertIs(modes['cases'][-1]['compositional'], False)
            self.assertEqual(modes['cases'][-1]['expected'], edited['faces'][0]['abilities'])
            self.assertEqual(modes['cases'][-1]['context'], edited['faces'][0]['characteristics'])
            self.assertEqual(modes['cases'][:-1], family['cases'][:-1])
            authored = json.loads((evaluation/'family.json').read_text())
            authored['cases'][0]['compositional'] = False
            (evaluation/'family.json').write_text(json.dumps(authored))
            with self.assertRaisesRegex(ValueError, 'compositional'):
                batch._family(evaluation, report, root, modes=True)
            authored['cases'][0].pop('compositional')
            (evaluation/'family.json').write_text(json.dumps(authored))
            edited['faces'][0]['abilities'][0]['amount'] = 3
            (evaluation/'edited-0/gold.jsonl').write_text(json.dumps(edited)+'\n')
            with self.assertRaises(ValueError):
                batch._family(evaluation, report, root)

    def test_batch_bound_rejects_empty_and_over_ninety_six(self):
        with tempfile.TemporaryDirectory() as temporary:
            for proposals in ([], [{}] * 97):
                with self.assertRaisesRegex(ValueError, 'ninety-six'):
                    batch.stage(temporary, {}, proposals)
            with patch.object(batch, '_git', side_effect=RuntimeError('past bound')):
                with self.assertRaisesRegex(RuntimeError, 'past bound'):
                    batch.stage(temporary, {'source': temporary, 'commit': 'base'}, [{}] * 96)

    def test_validate_reserves_cpu_and_uses_24_threads(self):
        self._validate_threads(24, False)

    def test_trial_helper_uses_verified_binary_and_trial_grammar(self):
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            helpers = root/'helpers'
            helpers.mkdir()
            binary = helpers/'family-check'
            binary.write_bytes(b'helper')
            (helpers/'build.json').write_text(json.dumps({'source_sha256': 'source',
                                                         'binaries': {'family-check': batch.r.digest(binary)}}))
            trial = root/'trial/grammar'
            trial.mkdir(parents=True)
            checkpoint = {'helpers': str(helpers), 'cpu_pool': str(root/'cpu'), 'cpu_capacity': 32}
            def execute(*args, **kwargs):
                self.assertEqual(kwargs['env']['ORACLE_GRAMMAR_DIR'], str(trial))
                self.assertEqual(kwargs['env']['ORACLE_GRAMMAR_THREADS'], '2')
                self.assertEqual(len(kwargs['pass_fds']), 2)
                return SimpleNamespace(returncode=0)
            with patch.object(batch.dev, 'source', return_value='source'), \
                    patch.object(batch.subprocess, 'run', side_effect=execute):
                batch._helper(checkpoint, root, root/'family.json', root/'check.log', grammar_dir=trial)
            self.assertEqual(batch.r._LEASES, [])

    def test_validate_reserves_cpu_and_uses_16_threads(self):
        self._validate_threads(16, True)

    def _validate_threads(self, expected, configured):
        import os
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root/'workspace'
            (source/'grammar').mkdir(parents=True)
            (source/'grammar/a.ebnf').write_text('root ::= one\n')
            frozen = root/'checkpoint/frozen'
            frozen.mkdir(parents=True)
            (frozen/'gold.jsonl').write_text('gold')
            (root/'checkpoint/manifest.json').write_text(json.dumps({'files': batch.r.tree(frozen)}))
            baseline = root/'baseline.jsonl'
            baseline.write_text('baseline')
            folder = root/'batch'
            folder.mkdir()
            manifest = {'status': 'provisional', 'commit': 'candidate', 'source': str(source),
                        'folder': str(folder), 'grammar': batch.r.tree(source/'grammar'), 'jobs': [{'job_id': 'j1', 'status': 'included'}]}
            cp = {'commit': 'base', 'inputs': str(root/'checkpoint'), 'baseline': str(baseline),
                  'helpers': str(root/'helpers'), 'cpu_pool': str(root/'cpu'), 'cpu_capacity': 32}
            if configured:
                cp['audit_threads'] = expected
            def git(_source, *args):
                return 'candidate' if args[0] == 'rev-parse' else ''
            def checks(run, directory, grammar, names, data, threads):
                self.assertEqual(threads, expected)
                self.assertEqual(len(batch.r._LEASES), expected)
                for fd in batch.r._LEASES:
                    os.fstat(fd)
                (directory/'rows.jsonl').write_text('rows')
                return {'failures': []}
            def gate(*args, **kwargs):
                self.assertEqual(len(kwargs['pass_fds']), expected)
                self.assertEqual(kwargs['env']['CARGO_BUILD_JOBS'], str(expected - 4))
                for name in ('RAYON_NUM_THREADS', 'ORACLE_GRAMMAR_THREADS', 'RUST_TEST_THREADS'):
                    self.assertEqual(kwargs['env'][name], str(expected//2-2))
                return SimpleNamespace(returncode=0)
            with patch.object(batch, '_git', side_effect=git), patch.object(batch.r, 'freeze') as freeze, patch.object(batch.w, 'payload', return_value=({'0': {'name': 'Example'}}, [], [])), patch.object(batch.w, 'checks', side_effect=checks), patch.object(batch.subprocess, 'run', side_effect=gate):
                result = batch.validate(manifest, cp)
            self.assertEqual(freeze.call_args.args[0].threads, expected)
            self.assertEqual(freeze.call_args.args[0].slots, 1)
            self.assertEqual(result['status'], 'passed')
            self.assertEqual(result['scope'], 'combined-batch')
            self.assertEqual(result['audit_threads'], expected)
            self.assertEqual(result['included_jobs'], ['j1'])
            self.assertEqual(result['inputs'], str(folder/'validation-inputs'))
            self.assertEqual(batch.r._LEASES, [])

    def test_validate_rejects_unsafe_audit_thread_budgets(self):
        manifest = {'status': 'provisional'}
        for threads, capacity in ((15, 32), (25, 32), (16, 15), (True, 32), (16, 33)):
            with self.subTest(threads=threads, capacity=capacity), self.assertRaises(ValueError):
                batch._validate(manifest, {'audit_threads': threads, 'cpu_capacity': capacity}, 12)

    def test_mechanical_stages_without_fixtures_and_reuses_clean_workspace(self):
        import shutil
        import mined
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root/'repo'
            repo.mkdir()
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo/'grammar/syntax').mkdir(parents=True)
            (repo/'tests').mkdir()
            (repo/'tests/families.json').write_text('[]\n')
            (repo/'grammar/syntax/mined.ebnf').write_text('mined_action ::= m_one | m_two | never_action\n/* @sem One */\nm_one ::= "one"\n/* @sem Two */\nm_two ::= "two"\n/* @sem () */\nnever_action ::= "never"\n')
            subprocess.run(['git', 'add', '.'], cwd=repo, check=True)
            subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=t@x', 'commit', '-qm', 'base'], cwd=repo, check=True)
            cp = {'source': str(repo), 'commit': batch._git(repo, 'rev-parse', 'HEAD'), 'integration_workspace': str(root/'workspace')}
            def execute(rule, index):
                run = root/f'run{index}'
                run.mkdir()
                (run/'inputs').mkdir()
                frozen = run/'inputs/frozen'
                frozen.mkdir()
                shutil.copytree((repo if index == 1 else root/'workspace')/'grammar', frozen/'grammar')
                attempt = root/f'attempt{index}'
                attempt.mkdir()
                shutil.copytree(frozen/'grammar', attempt/'grammar')
                mined.remove(attempt, [rule])
                report = {'task': f'task{index}', 'source': cp['commit'], 'descriptor_sha256': 'd', 'support_sha256': 's'}
                job = {'job_id': f'j{index}', 'attempt_id': f'a{index}', 'kind': 'mechanical', 'run': str(run), 'report': report}
                with patch.object(batch, '_proposal', return_value=(run, {'rules': [rule]}, attempt, [])):
                    return batch.stage(root, cp, [job])
            first = execute('m_one', 1)
            self.assertEqual(first['status'], 'provisional')
            self.assertEqual(first['fixtures'], [])
            self.assertEqual(first['jobs'][0]['kind'], 'mechanical')
            self.assertEqual(batch._git(repo, 'rev-parse', 'HEAD'), cp['commit'])
            cp['commit'] = first['commit']
            with self.assertRaisesRegex(ValueError, 'terminal'):
                execute('m_two', 2)
            Path(first['folder'], 'validation.json').write_text('{"status":"passed"}')
            second = execute('m_two', 3)
            self.assertEqual(second['status'], 'provisional')
            self.assertEqual(second['source'], first['source'])
            self.assertNotIn('m_two ::= ', (root/'workspace/grammar/syntax/mined.ebnf').read_text())
            self.assertIn('m_one ::= ', (repo/'grammar/syntax/mined.ebnf').read_text())

    def test_conflicts_are_parked_without_removing_rules_or_overwriting_fixture(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root/'repo'
            source.mkdir()
            subprocess.run(['git', 'init', '-q', str(source)], check=True)
            for folder in ('grammar/syntax', 'tests', 'families'):
                (source/folder).mkdir(parents=True)
            (source/'grammar/syntax/mined.ebnf').write_text('m_old ::= "old"\nm_good ::= "good"\n')
            (source/'tests/families.json').write_text('[]\n')
            (source/'families/existing.json').write_text('protected')
            subprocess.run(['git', 'add', '.'], cwd=source, check=True)
            subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=t@x', 'commit', '-qm', 'base'], cwd=source, check=True)
            commit = batch._git(source, 'rev-parse', 'HEAD')
            report = {'edited': [], 'task': 'task', 'source': commit, 'descriptor_sha256': 'd', 'support_sha256': 's', 'fixture_sha256': 'f'}
            proposals = [{'job_id': n, 'attempt_id': n, 'run': str(root/n), 'report': dict(report)} for n in ('conflict', 'overwrite', 'good')]
            def proposal(p):
                name = p['job_id']
                return root/name, {'fixture': 'families/existing.json' if name == 'overwrite' else f'families/{name}.json',
                                   'rules': ['m_'+('old' if name == 'conflict' else 'good')]}, root/name, []
            def merge(base, current, proposed, rules, hooks):
                if rules == ['m_old']:
                    raise batch.merger.MergeConflict('semantic conflict')
                return {'grammar/syntax/added.ebnf': 'shared ::= "good"\n'}
            def remove(trial, rules):
                path = trial/'grammar/syntax/mined.ebnf'
                path.write_text(path.read_text().replace('m_good ::= "good"\n', ''))
                return rules, {}
            with patch.object(batch, '_proposal', side_effect=proposal), patch.object(batch, '_family', return_value={'family': 'new', 'cases': []}) as family_mock, patch.object(batch.merger, 'merge_grammar', side_effect=merge), patch.object(batch.mined, 'remove', side_effect=remove), patch.object(batch.a, 'apply_hooks'), patch.object(batch, '_helper'):
                result = batch.stage(root, {'source': str(source), 'commit': commit,
                                            'family_case_modes': True}, proposals)
            self.assertTrue(family_mock.call_args_list)
            self.assertTrue(all(call.kwargs['modes'] is True for call in family_mock.call_args_list))
            self.assertEqual(result['status'], 'provisional')
            self.assertEqual([j['status'] for j in result['jobs']], ['integration-rejected', 'integration-rejected', 'included'])
            candidate = Path(result['source'])
            self.assertIn('m_old', (candidate/'grammar/syntax/mined.ebnf').read_text())
            self.assertEqual((candidate/'families/existing.json').read_text(), 'protected')
            self.assertEqual(json.loads((candidate/'tests/families.json').read_text()), ['families/good.json'])
            self.assertEqual(batch._git(source, 'rev-parse', 'HEAD'), commit)
            self.assertNotIn('shared', (source/'grammar/syntax/mined.ebnf').read_text())

    def test_trial_family_failure_rejects_only_new_proposal(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root/'repo'
            repo.mkdir()
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo/'grammar/syntax').mkdir(parents=True)
            (repo/'tests').mkdir()
            (repo/'tests/families.json').write_text('[]\n')
            (repo/'grammar/syntax/mined.ebnf').write_text(
                'm_first ::= "first"\nm_bad ::= "bad"\nm_last ::= "last"\nm_mech ::= "mech"\n')
            subprocess.run(['git', 'add', '.'], cwd=repo, check=True)
            subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=t@x',
                            'commit', '-qm', 'base'], cwd=repo, check=True)
            commit = batch._git(repo, 'rev-parse', 'HEAD')
            report = {'task': 'task', 'source': commit, 'descriptor_sha256': 'd',
                      'support_sha256': 's', 'fixture_sha256': 'f', 'edited': []}
            names = ('first', 'bad', 'last', 'mech')
            proposals = [{'job_id': n, 'attempt_id': n, 'run': str(root/n),
                          'report': dict(report), 'kind': 'mechanical' if n == 'mech' else 'author'}
                         for n in names]
            def proposal(p):
                name = p['job_id']
                return root/name, {'fixture': f'families/{name}.json', 'rules': [f'm_{name}']}, root/name, []
            def merge(base, trial, proposed, rules, hooks):
                if rules == ['m_mech']:
                    return {}
                name = rules[0][2:]
                existing = trial/'grammar/syntax/shared.ebnf'
                return {'grammar/syntax/shared.ebnf': (existing.read_text() if existing.exists() else '') + name+'\n'}
            def remove(trial, rules):
                path = trial/'grammar/syntax/mined.ebnf'
                name = rules[0][2:]
                path.write_text(path.read_text().replace(f'm_{name} ::= "{name}"\n', ''))
                return rules, {}
            calls = []
            def helper(checkpoint, source, fixture, log, grammar_dir=None):
                text = (grammar_dir/'syntax/shared.ebnf').read_text()
                calls.append((text, fixture.name))
                log.write_text('checked')
                if 'bad\n' in text:
                    raise ValueError('interaction family failure')
            with patch.object(batch, '_proposal', side_effect=proposal), \
                    patch.object(batch, '_family', return_value={'family': 'new', 'cases': []}), \
                    patch.object(batch.merger, 'merge_grammar', side_effect=merge), \
                    patch.object(batch.mined, 'remove', side_effect=remove), \
                    patch.object(batch.a, 'apply_hooks'), patch.object(batch, '_helper', side_effect=helper):
                result = batch.stage(root, {'source': str(repo), 'commit': commit}, proposals)
            self.assertEqual(result['status'], 'provisional')
            self.assertEqual([j['status'] for j in result['jobs']],
                             ['included', 'integration-rejected', 'included', 'included'])
            candidate = Path(result['source'])
            self.assertEqual((candidate/'grammar/syntax/shared.ebnf').read_text(), 'first\nlast\n')
            self.assertIn('m_bad ::= ', (candidate/'grammar/syntax/mined.ebnf').read_text())
            self.assertEqual(json.loads((candidate/'tests/families.json').read_text()),
                             ['families/first.json', 'families/last.json'])
            self.assertEqual([name for text, name in calls if text == 'first\nlast\n'],
                             ['first.json', 'family.json', 'first.json', 'last.json'])
            self.assertTrue((Path(result['jobs'][1]['family_logs'])/'family-000.log').is_file())
            self.assertFalse((repo/'grammar/syntax/shared.ebnf').exists())


if __name__ == '__main__':
    unittest.main()
