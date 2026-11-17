import json
import os
from pathlib import Path
import shutil
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


def optimized_fixture(root):
    candidate = root/'trusted-build'
    candidate.mkdir()
    for name in batch.dev.BINS:
        (candidate/name).write_bytes(('optimized-'+name).encode())
    compiler = {'environment': {}, 'versions': ['rustc-test', 'cargo-test']}
    build = {'profile': 'optimized', 'source_sha256': 'compiled', 'compiler': compiler,
             'binaries': {name: batch.r.digest(candidate/name) for name in batch.dev.BINS}}
    (candidate/'build.json').write_text(json.dumps(build))
    inputs = root/'checkpoint-inputs/frozen'
    inputs.mkdir(parents=True)
    (inputs/'build.json').write_text(json.dumps({'source_sha256': 'compiled', 'compiler': compiler}))
    folder = root/'campaign/.cache/batches/test'
    folder.mkdir(parents=True)
    source = root/'campaign/workspaces/coordinator'
    source.mkdir(parents=True)
    cp = {'inputs': str(inputs.parent), 'helpers': str(root/'old-helpers')}
    spec = {'path': str(candidate), 'build_sha256': batch.r.digest(candidate/'build.json')}
    return cp, spec, source, folder, build


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

    def test_gate_driver_mutation_does_not_start_full_score(self):
        self._validate_threads(16, True, driver_changed=True)

    def _validate_threads(self, expected, configured, gate_code=0, driver_changed=False):
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
            driver = root/'retire_repository_checks.py'
            driver.write_bytes(b'original coordinator gate driver')
            driver_hash = batch.r.digest(driver)
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
                command = args[0]
                self.assertEqual(command[1:4], [str(driver), str(source), '--lease-fds'])
                self.assertEqual(command[4], ','.join(map(str, kwargs['pass_fds'])))
                order.append('gate')
                self.assertEqual(len(kwargs['pass_fds']), expected)
                self.assertEqual(kwargs['env']['CARGO_BUILD_JOBS'], str(expected - 4))
                self.assertNotIn('ORACLE_GRAMMAR_POISON', kwargs['env'])
                self.assertNotIn('ORACLE_GRAMMAR_DIR', kwargs['env'])
                for name in ('RAYON_NUM_THREADS', 'ORACLE_GRAMMAR_THREADS', 'RUST_TEST_THREADS'):
                    self.assertEqual(kwargs['env'][name], str(expected//2-2))
                if driver_changed:
                    driver.write_bytes(b'changed coordinator gate driver')
                return SimpleNamespace(returncode=gate_code)
            def freeze_input(args):
                order.append('freeze')
                args.run.mkdir()
            with patch.dict(os.environ, {'ORACLE_GRAMMAR_DIR': '/wrong/checkout/grammar', 'ORACLE_GRAMMAR_POISON': 'wrong'}), patch.object(batch, 'REPOSITORY_DRIVER', driver), patch.object(batch, '_git', side_effect=git), patch.object(batch.r, 'freeze', side_effect=freeze_input) as freeze, patch.object(batch.w, 'payload', return_value=({'0': {'name': 'Example'}}, [], [])), patch.object(batch.w, 'checks', side_effect=checks) as full_checks, patch.object(batch.subprocess, 'run', side_effect=gate):
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
            if driver_changed:
                self.assertEqual(order, ['freeze', 'gate'])
                full_checks.assert_not_called()
                self.assertEqual(result['status'], 'failed')
                self.assertEqual(result['reason'], 'repository gate driver changed during validation')
                self.assertFalse((folder/'full-checks').exists())
                self.assertNotIn('rows', result)
                self.assertEqual(batch.r._LEASES, [])
                return
            self.assertEqual(order, ['freeze', 'gate', 'checks'])
            self.assertEqual(result['repository_driver_sha256'], driver_hash)
            self.assertEqual(result['repository_grammar'], str(source/'grammar'))
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

    def test_candidate_snapshot_copies_seven_pinned_helpers(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cp, spec, source, folder, build = optimized_fixture(root)
            with patch.object(batch.dev, 'source', return_value='compiled'):
                snapshot, copied = batch._candidate_helper_snapshot(spec, cp, source, folder)
            self.assertEqual(copied, build)
            self.assertEqual(batch.r.digest(snapshot/'build.json'), spec['build_sha256'])
            for name in batch.dev.BINS:
                self.assertNotEqual(os.stat(snapshot/name).st_ino, os.stat(Path(spec['path'])/name).st_ino)
                self.assertEqual(batch.r.digest(snapshot/name), build['binaries'][name])
            (Path(spec['path'])/'probe').write_bytes(b'changed after snapshot')
            self.assertEqual(batch._verified_helper_build(snapshot, spec['build_sha256']), build)

    def test_candidate_snapshot_rejects_bad_provenance_or_workspace(self):
        for case in ('source', 'compiler', 'profile', 'hash', 'binary', 'workspace'):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                cp, spec, source, folder, build = optimized_fixture(root)
                original = Path(spec['path'])
                if case == 'source':
                    build['source_sha256'] = 'wrong'
                elif case == 'compiler':
                    build['compiler'] = {'environment': {}, 'versions': ['other']}
                elif case == 'profile':
                    build['profile'] = 'quick'
                if case in ('source', 'compiler', 'profile'):
                    (original/'build.json').write_text(json.dumps(build))
                    spec['build_sha256'] = batch.r.digest(original/'build.json')
                elif case == 'hash':
                    spec['build_sha256'] = '0'*64
                elif case == 'binary':
                    (original/'score').write_bytes(b'tampered')
                elif case == 'workspace':
                    target = source/'worker-build'
                    shutil.copytree(original, target)
                    spec['path'] = str(target)
                with patch.object(batch.dev, 'source', return_value='compiled'), self.assertRaises(ValueError):
                    batch._candidate_helper_snapshot(spec, cp, source, folder)

    def test_optimized_families_use_snapshot_and_case_modes(self):
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root/'tests').mkdir()
            (root/'families').mkdir()
            (root/'grammar').mkdir()
            (root/'tests/families.json').write_text('["families/a.json"]')
            (root/'families/a.json').write_text('{"cases":[{"compositional":false}]}')
            def execute(command, **kwargs):
                self.assertEqual(command[0], str(root/'helpers/family-check'))
                self.assertIn('--compositional', command)
                self.assertIn('--module=events=trial', command)
                kwargs['stdout'].write(json.dumps({'family': 'a', 'cases': 1, 'passed': 1,
                                                  'compositional': False, 'failures': []}))
                kwargs['stdout'].flush()
                return SimpleNamespace(returncode=0)
            with patch.object(batch.dev, 'modules', return_value=['--module=events=trial']), \
                    patch.object(batch.subprocess, 'run', side_effect=execute):
                log = batch._optimized_families(root, root/'helpers', root, 6)
            self.assertTrue(log.is_file())

    def test_candidate_validation_only_publishes_verified_snapshot_after_gates(self):
        from types import SimpleNamespace
        for mode in ('pass', 'family-failure', 'snapshot-tamper'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                cp, spec, source, folder, _ = optimized_fixture(root)
                (source/'grammar').mkdir()
                (source/'grammar/x.ebnf').write_text('x ::= "x"')
                (source/'tests').mkdir()
                (source/'tests/families.json').write_text('["families/x.json"]')
                (source/'families').mkdir()
                (source/'families/x.json').write_text('{}')
                frozen = Path(cp['inputs'])/'frozen'
                (Path(cp['inputs'])/'manifest.json').write_text(json.dumps({'files': batch.r.tree(frozen)}))
                baseline = root/'baseline.jsonl'
                baseline.write_text('baseline')
                cp.update(commit='base', baseline=str(baseline), cpu_pool=str(root/'cpu'),
                          cpu_capacity=32, audit_threads=16, candidate_validation_helpers=spec)
                manifest = {'status': 'provisional', 'commit': 'candidate', 'source': str(source),
                            'folder': str(folder), 'grammar': batch.r.tree(source/'grammar'),
                            'jobs': [{'job_id': 'one', 'status': 'included'}]}
                def freeze(args):
                    dest = args.run/'frozen'
                    (dest/'bin').mkdir(parents=True)
                    shutil.copy2(args.helpers/'build.json', dest/'build.json')
                    for name in batch.r.HELPERS:
                        shutil.copy2(args.helpers/name, dest/'bin'/name)
                    (args.run/'manifest.json').write_text(json.dumps({'files': batch.r.tree(dest)}))
                def execute(command, **kwargs):
                    if Path(command[0]).name == 'family-check':
                        kwargs['stdout'].write(json.dumps({'family': 'x', 'cases': 1, 'passed': 1, 'failures': []}))
                        kwargs['stdout'].flush()
                        return SimpleNamespace(returncode=int(mode == 'family-failure'))
                    return SimpleNamespace(returncode=0)
                def checks(run, directory, grammar, names, data, threads):
                    (directory/'rows.jsonl').write_text('rows')
                    if mode == 'snapshot-tamper':
                        binary = folder/'candidate-validation-helpers/probe'
                        binary.chmod(0o755)
                        binary.write_bytes(b'tampered')
                    return {'faces': 1, 'failures': []}
                with patch.object(batch.dev, 'source', return_value='compiled'), \
                        patch.object(batch.dev, 'modules', return_value=[]), \
                        patch.object(batch, '_git', side_effect=lambda src, *args: 'candidate' if args[0] == 'rev-parse' else ''), \
                        patch.object(batch.r, 'freeze', side_effect=freeze), \
                        patch.object(batch.w, 'payload', return_value=({'0': {'name': 'A'}}, [], [])), \
                        patch.object(batch.w, 'checks', side_effect=checks) as full, \
                        patch.object(batch.subprocess, 'run', side_effect=execute):
                    result = batch.validate(manifest, cp)
                if mode == 'pass':
                    self.assertEqual(result['status'], 'passed')
                    self.assertEqual(result['helpers'], str(folder/'candidate-validation-helpers'))
                    self.assertEqual(result['candidate_helper_build_sha256'], spec['build_sha256'])
                else:
                    self.assertEqual(result['status'], 'failed')
                    self.assertNotIn('helpers', result)
                    if mode == 'family-failure':
                        full.assert_not_called()
                self.assertEqual(batch.r._LEASES, [])

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

    def test_optional_group_trials_gate_union_and_fallback_without_partial_publication(self):
        for mode in ('pass', 'two-groups', 'family-interaction', 'native-interaction', 'library-interaction', 'structural', 'missing-metadata'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                repo = root/'repo'
                repo.mkdir()
                subprocess.run(['git', 'init', '-q', str(repo)], check=True)
                (repo/'grammar/syntax').mkdir(parents=True)
                (repo/'tests').mkdir()
                (repo/'families').mkdir()
                (repo/'families/base.json').write_text('{"family":"base","cases":[]}')
                (repo/'tests/families.json').write_text('["families/base.json"]')
                (repo/'grammar/syntax/mined.ebnf').write_text(
                    'm_first ::= "first"\nm_bad ::= "bad"\nm_last ::= "last"\nm_mech ::= "mech"\n')
                subprocess.run(['git', 'add', '.'], cwd=repo, check=True)
                subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=t@x',
                                'commit', '-qm', 'base'], cwd=repo, check=True)
                commit = batch._git(repo, 'rev-parse', 'HEAD')
                cp = dict(control_fixture(root, commit), source=str(repo),
                          stage_group_size=2 if mode == 'two-groups' else 4)
                report = {'task': 'task', 'source': commit, 'descriptor_sha256': 'd',
                          'support_sha256': 's', 'fixture_sha256': 'f', 'edited': []}
                names = ('first', 'bad', 'last', 'mech')
                proposals = [{'job_id': name, 'attempt_id': name, 'report': dict(report),
                              'kind': 'mechanical' if name == 'mech' else 'author'} for name in names]
                if mode == 'missing-metadata':
                    del proposals[1]['report']['descriptor_sha256']
                def proposal(item):
                    name = item['job_id']
                    if mode == 'structural' and name == 'bad':
                        raise ValueError('bad immutable report')
                    return root/name, {'fixture': f'families/{name}.json', 'rules': [f'm_{name}']}, root/name, []
                def merge(base, trial, proposed, rules, hooks):
                    name = rules[0][2:]
                    if name == 'mech':
                        return {}
                    path = trial/'grammar/syntax/shared.ebnf'
                    return {'grammar/syntax/shared.ebnf': (path.read_text() if path.exists() else '')+name+'\n'}
                def remove(trial, rules):
                    path = trial/'grammar/syntax/mined.ebnf'
                    name = rules[0][2:]
                    path.write_text(path.read_text().replace(f'm_{name} ::= "{name}"\n', ''))
                    return rules, {}
                family_calls, native_calls, progress = [], [], []
                def family(checkpoint, source, fixtures, log, grammar_dir=None):
                    text = (grammar_dir/'syntax/shared.ebnf').read_text()
                    family_calls.append((text, [p.name for p in fixtures]))
                    progress.append(json.loads((log.parent.parent/'manifest.json').read_text()))
                    log.write_text('family proof')
                    if mode == 'family-interaction' and 'first\n' in text and 'bad\n' in text:
                        raise ValueError('union family conflict')
                def native(run, directory, grammar, names, data, threads):
                    path = grammar/'syntax/shared.ebnf'
                    text = path.read_text() if path.exists() else ''
                    native_calls.append(text)
                    failed = mode == 'native-interaction' and 'first\n' in text and 'bad\n' in text
                    return {'faces': 2, 'failures': ['union native conflict'] if failed else []}
                library = {'profile': root/'profile', 'profile_sha256': 'p', 'binary_sha256': 'b',
                           'source_sha256': 's', 'tests': ['tests::one']}
                def library_check(checkpoint, control, grammar, directory):
                    directory.mkdir()
                    path = grammar/'syntax/shared.ebnf'
                    text = path.read_text() if path.exists() else ''
                    failed = 'first\nbad\n' in text
                    (directory/'result.json').write_text(json.dumps({'passed': not failed}))
                    if failed:
                        raise ValueError('library interaction')
                with patch.object(batch, '_proposal', side_effect=proposal), \
                        patch.object(batch, '_family', return_value={'family': 'new', 'cases': []}), \
                        patch.object(batch.merger, 'merge_grammar', side_effect=merge), \
                        patch.object(batch.mined, 'remove', side_effect=remove), \
                        patch.object(batch.a, 'apply_hooks'), patch.object(batch, '_helper', side_effect=family), \
                        patch.object(batch.w, 'checks', side_effect=native), \
                        patch.object(batch.library_controls, 'setup', return_value=library if mode == 'library-interaction' else None), \
                        patch.object(batch.library_controls, 'check', side_effect=library_check), \
                        patch.object(batch.library_controls, 'verify'):
                    if mode in ('family-interaction', 'native-interaction'):
                        isolated = batch.stage(root, cp, [proposals[1]])
                        self.assertEqual(isolated['jobs'][0]['status'], 'included')
                        family_calls.clear()
                        native_calls.clear()
                        progress.clear()
                    result = batch.stage(root, cp, proposals)
                self.assertEqual(result['status'], 'provisional')
                self.assertEqual(result['stage_group_size'], 2 if mode == 'two-groups' else 4)
                candidate = Path(result['source'])
                if mode == 'two-groups':
                    self.assertEqual([item['status'] for item in result['jobs']], ['included']*4)
                    self.assertEqual(len(family_calls), 2)
                    self.assertEqual(family_calls[1][1], ['base.json', 'first.json', 'bad.json', 'family-000.json'])
                    self.assertEqual(native_calls, ['', 'first\nbad\n', 'first\nbad\nlast\n'])
                    self.assertEqual(progress[1]['status'], 'staging')
                    self.assertEqual(len(progress[1]['jobs']), 2)
                    self.assertEqual(result['jobs'][0]['trial_proof'], result['jobs'][1]['trial_proof'])
                    self.assertEqual(result['jobs'][2]['trial_proof'], result['jobs'][3]['trial_proof'])
                    self.assertNotEqual(result['jobs'][0]['trial_proof'], result['jobs'][2]['trial_proof'])
                elif mode == 'pass':
                    self.assertEqual([item['status'] for item in result['jobs']], ['included']*4)
                    self.assertEqual(len(family_calls), 1)
                    self.assertEqual(native_calls, ['', 'first\nbad\nlast\n'])
                    self.assertEqual(family_calls[0][1], ['base.json', 'family-000.json', 'family-001.json', 'family-002.json'])
                    self.assertEqual(len({item['trial_proof']['sha256'] for item in result['jobs']}), 1)
                    self.assertEqual((candidate/'grammar/syntax/shared.ebnf').read_text(), 'first\nbad\nlast\n')
                else:
                    self.assertEqual([item['status'] for item in result['jobs']],
                                     ['included', 'integration-rejected', 'included', 'included'])
                    self.assertEqual((candidate/'grammar/syntax/shared.ebnf').read_text(), 'first\nlast\n')
                    self.assertFalse((candidate/'families/bad.json').exists())
                    group = result['jobs'][0]['group_attempt']
                    self.assertTrue(all(item['group_attempt'] == group for item in result['jobs']))
                    proof = json.loads(Path(group['path']).read_text())
                    self.assertEqual(proof['status'], 'failed')
                    self.assertEqual(proof['jobs'], list(names))
                    self.assertEqual(batch.r.digest(Path(group['path'])), group['sha256'])
                    if mode not in ('structural', 'missing-metadata'):
                        self.assertEqual(family_calls[0][0], 'first\nbad\nlast\n')
                        self.assertIn('first\n', [text for text, _ in family_calls])
                        self.assertIn('first\nbad\n', [text for text, _ in family_calls])
                    fallback_progress = progress[0 if mode in ('structural', 'missing-metadata') else 1]
                    self.assertEqual(fallback_progress['status'], 'staging')
                    self.assertEqual(fallback_progress['group_trials'][0]['status'], 'failed')
                self.assertEqual(batch.r._LEASES, [])

    def test_library_control_bad_baseline_fails_without_staging(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root/'repo'
            repo.mkdir()
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            (repo/'grammar').mkdir()
            (repo/'grammar/example.ebnf').write_text('example')
            (repo/'tests').mkdir()
            (repo/'tests/families.json').write_text('[]')
            subprocess.run(['git', 'add', '.'], cwd=repo, check=True)
            subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=t@x',
                            'commit', '-qm', 'base'], cwd=repo, check=True)
            cp = {'source': str(repo), 'commit': batch._git(repo, 'rev-parse', 'HEAD')}
            library = {'profile': root/'profile', 'profile_sha256': 'p', 'binary_sha256': 'b',
                       'source_sha256': 's', 'tests': ['tests::one']}
            with patch.object(batch.library_controls, 'setup', return_value=library), \
                    patch.object(batch.library_controls, 'check', side_effect=ValueError('baseline fails')), \
                    patch.object(batch, '_proposal') as proposal:
                result = batch.stage(root, cp, [{'job_id': 'j'}])
            self.assertEqual(result['status'], 'failed')
            self.assertEqual(result['failure_scope'], 'baseline-library-control')
            self.assertEqual(result['jobs'], [])
            proposal.assert_not_called()

    def test_group_size_bounds_and_promotion_rolls_back_all_fixtures(self):
        for invalid in (0, 5, True, '2'):
            with self.subTest(size=invalid), tempfile.TemporaryDirectory() as temporary:
                with self.assertRaisesRegex(ValueError, 'stage_group_size'):
                    batch.stage(temporary, {'stage_group_size': invalid}, [{'job_id': 'j'}])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, trial = root/'source', root/'trial'
            (source/'grammar').mkdir(parents=True)
            (trial/'grammar').mkdir(parents=True)
            (source/'grammar/x').write_text('old')
            (trial/'grammar/x').write_text('new')
            (trial/'a').write_text('A')
            (trial/'b').write_text('B')
            real_replace = os.replace
            def replace(src, dst):
                if Path(src) == trial/'b':
                    raise OSError('second fixture failed')
                return real_replace(src, dst)
            with patch.object(batch.os, 'replace', side_effect=replace):
                with self.assertRaisesRegex(OSError, 'second fixture failed'):
                    batch._promote_trial(source, trial, [(trial/'a', Path('families/a.json')),
                                                         (trial/'b', Path('families/b.json'))])
            self.assertEqual((source/'grammar/x').read_text(), 'old')
            self.assertFalse((source/'families/a.json').exists())

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
