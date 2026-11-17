import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import retire_batch as batch


def control_fixture(root, commit):
    run = root/'inputs'
    frozen = run/'frozen'
    (frozen/'bin').mkdir(parents=True)
    (frozen/'allowed.json').write_text(json.dumps({'cards': ['A', 'B']}))
    (frozen/'gold.jsonl').write_text(json.dumps({'faces': [{'name': 'A'}, {'name': 'B'}]})+'\n')
    (frozen/'oracle.json').write_text(json.dumps([{'cards': [{'name': 'A'}, {'name': 'B'}]}]))
    (frozen/'index.json').write_text(json.dumps({'0': {'name': 'A', 'record': 0, 'face': 0},
                                                '1': {'name': 'B', 'record': 0, 'face': 1}}))
    for name in ('score', 'validate-readings'):
        (frozen/'bin'/name).write_bytes(name.encode())
    hashes = {name: batch.r.digest(frozen/name) for name in batch.CONTROL_INPUTS}
    (run/'manifest.json').write_text(json.dumps({'format': 3, 'commit': commit, 'files': hashes}))
    profile = root/'profile.json'
    profile.write_text(json.dumps({'cards': ['A']}))
    return {'inputs': str(run), 'commit': commit, 'cpu_pool': str(root/'cpu'), 'cpu_capacity': 32,
            'native_regression_controls': {'path': str(profile), 'sha256': batch.r.digest(profile)}}


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
        import os
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
            (trial/'syntax').mkdir(parents=True)
            (trial/'syntax/events.ebnf').write_text('event ::= "trial"')
            checkpoint = {'helpers': str(helpers), 'cpu_pool': str(root/'cpu'), 'cpu_capacity': 32}
            def execute(*args, **kwargs):
                self.assertEqual(kwargs['env']['ORACLE_GRAMMAR_DIR'], str(trial))
                self.assertEqual(kwargs['env']['ORACLE_GRAMMAR_THREADS'], '6')
                self.assertEqual(len(kwargs['pass_fds']), 6)
                for fd in kwargs['pass_fds']:
                    os.fstat(fd)
                self.assertIn(f'--module=events={trial/"syntax/events.ebnf"}', args[0])
                kwargs['stdout'].write(json.dumps({'family': 'one', 'cases': 1, 'passed': 1, 'failures': []}))
                kwargs['stdout'].flush()
                return SimpleNamespace(returncode=0)
            with patch.object(batch.dev, 'source', return_value='source'), patch.object(batch.dev, 'modules', return_value=['--module=events=/base/events.ebnf']), \
                    patch.object(batch.subprocess, 'run', side_effect=execute):
                batch._helper(checkpoint, root, root/'family.json', root/'check.log', grammar_dir=trial)
            self.assertEqual(batch.r._LEASES, [])

    def test_trial_helper_parses_multi_family_and_writes_individual_proofs(self):
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            helpers = root/'helpers'
            helpers.mkdir()
            binary = helpers/'family-check'
            binary.write_bytes(b'helper')
            (helpers/'build.json').write_text(json.dumps({'source_sha256': 'source',
                                                         'binaries': {'family-check': batch.r.digest(binary)}}))
            fixtures = [root/'first.json', root/'second.json']
            reports = [{'family': 'first', 'cases': 1, 'passed': 1, 'failures': []},
                       {'family': 'second', 'cases': 1, 'passed': 0,
                        'failures': [{'name': 'edited', 'why': 'failure'}]}]
            def execute(command, **kwargs):
                self.assertEqual(command[1:-1], [str(path) for path in fixtures])
                kwargs['stdout'].write(json.dumps({'schema': 1, 'families': reports})+'\n')
                kwargs['stdout'].flush()
                return SimpleNamespace(returncode=1)
            with patch.object(batch.dev, 'source', return_value='source'), patch.object(batch.dev, 'modules', return_value=[]), \
                    patch.object(batch.subprocess, 'run', side_effect=execute):
                with self.assertRaisesRegex(ValueError, 'family check failed'):
                    batch._helper({'helpers': str(helpers)}, root, fixtures, root/'family.log')
            self.assertEqual(json.loads((root/'family-001.log').read_text()), reports[1])
            self.assertEqual(json.loads((root/'family-001.log').read_text().splitlines()[0]), reports[1])

    def test_validate_reserves_cpu_and_uses_16_threads(self):
        self._validate_threads(16, True)

    def test_repository_failure_does_not_start_full_score(self):
        self._validate_threads(16, True, gate_code=1)

    def _validate_threads(self, expected, configured, gate_code=0):
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
            order = []
            def git(_source, *args):
                return 'candidate' if args[0] == 'rev-parse' else ''
            def checks(run, directory, grammar, names, data, threads):
                order.append('checks')
                self.assertEqual(threads, expected)
                self.assertEqual(len(batch.r._LEASES), expected)
                for fd in batch.r._LEASES:
                    os.fstat(fd)
                (directory/'rows.jsonl').write_text('rows')
                return {'failures': []}
            def gate(*args, **kwargs):
                order.append('gate')
                self.assertEqual(len(kwargs['pass_fds']), expected)
                self.assertEqual(kwargs['env']['CARGO_BUILD_JOBS'], str(expected - 4))
                for name in ('RAYON_NUM_THREADS', 'ORACLE_GRAMMAR_THREADS', 'RUST_TEST_THREADS'):
                    self.assertEqual(kwargs['env'][name], str(expected//2-2))
                return SimpleNamespace(returncode=gate_code)
            def freeze_input(args):
                order.append('freeze')
                args.run.mkdir()
            with patch.object(batch, '_git', side_effect=git), patch.object(batch.r, 'freeze', side_effect=freeze_input) as freeze, patch.object(batch.w, 'payload', return_value=({'0': {'name': 'Example'}}, [], [])), patch.object(batch.w, 'checks', side_effect=checks) as full_checks, patch.object(batch.subprocess, 'run', side_effect=gate):
                result = batch.validate(manifest, cp)
            self.assertEqual(freeze.call_args.args[0].threads, expected)
            self.assertEqual(freeze.call_args.args[0].slots, 1)
            if gate_code:
                self.assertEqual(order, ['freeze', 'gate'])
                full_checks.assert_not_called()
                self.assertEqual(result['status'], 'failed')
                self.assertEqual(result['reason'], 'repository gates failed')
                self.assertEqual(result['repository_check_returncode'], gate_code)
                self.assertTrue((folder/'repository-check.log').is_file())
                self.assertTrue((folder/'validation-inputs').is_dir())
                self.assertFalse((folder/'full-checks').exists())
                self.assertNotIn('rows', result)
                self.assertEqual(json.loads((folder/'validation.json').read_text()), result)
                self.assertEqual(batch.r._LEASES, [])
                return
            self.assertEqual(order, ['freeze', 'gate', 'checks'])
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

    def test_native_control_profile_closure_and_tampering_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cp = control_fixture(root, 'base')
            control = batch._control_setup(cp)
            self.assertEqual(control['names'], ['A', 'B'])
            self.assertEqual(control['occurrences'], 2)
            self.assertEqual(len(control['data'][0]), 2)
            with self.assertRaisesRegex(ValueError, 'CPU lease'):
                batch._control_setup(dict(cp, cpu_pool=None))
            binary = Path(cp['inputs'])/'frozen/bin/score'
            binary.write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError, 'frozen input changed'):
                batch._control_verify(control)
            binary.write_bytes(b'score')
            Path(cp['native_regression_controls']['path']).write_text('{"cards":["B"]}')
            with self.assertRaisesRegex(ValueError, 'profile changed'):
                batch._control_verify(control)
            with self.assertRaisesRegex(ValueError, 'SHA-256 differs'):
                batch._control_setup(cp)
            cp['native_regression_controls']['path'] = str(root/'missing.json')
            with self.assertRaises(ValueError):
                batch._control_setup(cp)

    def test_native_control_rejects_unknown_duplicate_and_disallowed_faces(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cp = control_fixture(root, 'base')
            profile = Path(cp['native_regression_controls']['path'])
            for names in (['A', 'A'], ['missing'], [], ['A']*501):
                profile.write_text(json.dumps({'cards': names}))
                cp['native_regression_controls']['sha256'] = batch.r.digest(profile)
                with self.subTest(names=names[:2]), self.assertRaises(ValueError):
                    batch._control_setup(cp)

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
            self.assertEqual(result['family_check_threads'], 6)
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
            (repo/'tests/fixtures').mkdir()
            (repo/'tests/fixtures/reported-mana-reminders.json').write_text('{"family":"reported","cases":[]}')
            (repo/'families').mkdir()
            (repo/'families/baseline.json').write_text('{"family":"baseline","cases":[]}')
            (repo/'tests/families.json').write_text(
                '["families/baseline.json","tests/fixtures/reported-mana-reminders.json"]\n')
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
            def helper(checkpoint, source, fixtures, log, grammar_dir=None):
                text = (grammar_dir/'syntax/shared.ebnf').read_text()
                calls.append((text, [fixture.name for fixture in fixtures]))
                log.write_text('checked')
                if 'bad\n' in text:
                    self.assertEqual(fixtures[0].name, 'baseline.json')
                    raise ValueError('baseline family failure')
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
                             ['families/baseline.json', 'tests/fixtures/reported-mana-reminders.json',
                              'families/first.json', 'families/last.json'])
            self.assertEqual([names for text, names in calls if text == 'first\nlast\n'],
                             [['baseline.json', 'reported-mana-reminders.json', 'first.json', 'family.json'],
                              ['baseline.json', 'reported-mana-reminders.json', 'first.json', 'last.json']])
            self.assertEqual(len(calls), 4)
            self.assertTrue((Path(result['jobs'][1]['family_logs'])/'family.log').is_file())
            self.assertEqual(result['jobs'][2]['family_checks'][-1],
                             {'fixture': 'families/last.json', 'kind': 'new-proposal', 'log': 'family-003.log'})
            self.assertFalse((repo/'grammar/syntax/shared.ebnf').exists())

    def test_baseline_paths_do_not_relax_author_fixture_scope(self):
        self.assertEqual(batch._baseline_fixture_path('tests/fixtures/reported-mana-reminders.json'),
                         Path('tests/fixtures/reported-mana-reminders.json'))
        for invalid in ('tests/other/x.json', 'tests/fixtures/../../secret.json', '/tests/fixtures/x.json'):
            with self.subTest(path=invalid), self.assertRaises(ValueError):
                batch._baseline_fixture_path(invalid)
        with self.assertRaises(ValueError):
            batch._fixture_path('tests/fixtures/reported-mana-reminders.json')

    def test_native_trial_guard_rejects_bad_proposal_and_keeps_later_good(self):
        import os
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root/'repo'
            repo.mkdir()
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo/'grammar/syntax').mkdir(parents=True)
            (repo/'tests').mkdir()
            (repo/'tests/families.json').write_text('[]\n')
            (repo/'grammar/syntax/mined.ebnf').write_text(
                'm_first ::= "first"\nm_bad ::= "bad"\nm_last ::= "last"\n')
            subprocess.run(['git', 'add', '.'], cwd=repo, check=True)
            subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=t@x',
                            'commit', '-qm', 'base'], cwd=repo, check=True)
            commit = batch._git(repo, 'rev-parse', 'HEAD')
            cp = dict(control_fixture(root, commit), source=str(repo))
            report = {'task': 'task', 'source': commit, 'descriptor_sha256': 'd',
                      'support_sha256': 's', 'fixture_sha256': 'f', 'edited': []}
            proposals = [{'job_id': n, 'attempt_id': n, 'report': dict(report)}
                         for n in ('first', 'bad', 'last')]
            def proposal(p):
                name = p['job_id']
                return root/name, {'fixture': f'families/{name}.json', 'rules': [f'm_{name}']}, root/name, []
            def merge(base, trial, proposed, rules, hooks):
                name = rules[0][2:]
                existing = trial/'grammar/syntax/shared.ebnf'
                return {'grammar/syntax/shared.ebnf': (existing.read_text() if existing.exists() else '') + name+'\n'}
            def remove(trial, rules):
                path = trial/'grammar/syntax/mined.ebnf'
                name = rules[0][2:]
                path.write_text(path.read_text().replace(f'm_{name} ::= "{name}"\n', ''))
                return rules, {}
            calls = []
            def checks(run, directory, grammar, names, data, threads):
                self.assertEqual(threads, 6)
                self.assertEqual(len(batch.r._LEASES), 6)
                for fd in batch.r._LEASES:
                    os.fstat(fd)
                self.assertEqual(names, ['A', 'B'])
                self.assertEqual(len(data[0]), 2)
                marker = grammar/'syntax/shared.ebnf'
                text = marker.read_text() if marker.exists() else ''
                calls.append(text)
                return {'faces': 2, 'failures': ['native change'] if 'bad\n' in text else []}
            with patch.object(batch, '_proposal', side_effect=proposal), \
                    patch.object(batch, '_family', return_value={'family': 'new', 'cases': []}), \
                    patch.object(batch.merger, 'merge_grammar', side_effect=merge), \
                    patch.object(batch.mined, 'remove', side_effect=remove), \
                    patch.object(batch.a, 'apply_hooks'), patch.object(batch, '_helper'), \
                    patch.object(batch.w, 'checks', side_effect=checks):
                result = batch.stage(root, cp, proposals)
            self.assertEqual(calls, ['', 'first\n', 'first\nbad\n', 'first\nlast\n'])
            self.assertEqual([job['status'] for job in result['jobs']],
                             ['included', 'integration-rejected', 'included'])
            self.assertEqual(result['status'], 'provisional')
            self.assertEqual(result['native_regression_controls']['names'], ['A', 'B'])
            self.assertEqual(json.loads((Path(result['jobs'][1]['native_control'])/'result.json').read_text())['failures'],
                             ['native change'])
            self.assertEqual((Path(result['source'])/'grammar/syntax/shared.ebnf').read_text(), 'first\nlast\n')
            self.assertEqual(batch.r._LEASES, [])

    def test_native_base_failure_stops_before_any_trial(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root/'repo'
            repo.mkdir()
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo/'grammar').mkdir()
            (repo/'grammar/x.ebnf').write_text('x ::= "x"\n')
            (repo/'tests').mkdir()
            (repo/'tests/families.json').write_text('[]')
            subprocess.run(['git', 'add', '.'], cwd=repo, check=True)
            subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=t@x',
                            'commit', '-qm', 'base'], cwd=repo, check=True)
            cp = dict(control_fixture(root, batch._git(repo, 'rev-parse', 'HEAD')), source=str(repo))
            with patch.object(batch.w, 'checks', return_value={'faces': 2, 'failures': ['base failed']}), \
                    patch.object(batch, '_proposal') as proposal:
                result = batch.stage(root, cp, [{'job_id': 'j', 'attempt_id': 'a'}])
            proposal.assert_not_called()
            self.assertEqual(result['status'], 'failed')
            self.assertEqual(result['failure_scope'], 'baseline-native-control')
            self.assertTrue((Path(result['folder'])/'native-control-base/result.json').exists())


if __name__ == '__main__':
    unittest.main()
