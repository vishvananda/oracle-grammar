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
                self.assertEqual(kwargs['env']['ORACLE_GRAMMAR_THREADS'], str(expected_threads))
                self.assertEqual(len(kwargs['pass_fds']), expected_threads)
                for fd in kwargs['pass_fds']:
                    os.fstat(fd)
                self.assertIn(f'--module=events={trial/"syntax/events.ebnf"}', args[0])
                kwargs['stdout'].write(json.dumps({'family': 'one', 'cases': 1, 'passed': 1, 'failures': []}))
                kwargs['stdout'].flush()
                return SimpleNamespace(returncode=0)
            with patch.object(batch.dev, 'source', return_value='source'), patch.object(batch.dev, 'modules', return_value=['--module=events=/base/events.ebnf']), \
                    patch.object(batch.subprocess, 'run', side_effect=execute):
                for expected_threads in (6, 12):
                    if expected_threads == 12:
                        checkpoint['family_check_threads'] = expected_threads
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

    def test_complete_native_preservation_rejection_is_recorded(self):
        self._validate_threads(16, True, native_rejection=True)

    def test_parse_screen_pass_runs_full_checks(self):
        self._validate_threads(16, True, screen='passed')

    def test_parse_screen_reject_skips_full_checks(self):
        self._validate_threads(16, True, screen='failed')

    def test_configured_parse_screen_budget_leaves_full_audit_budget_intact(self):
        self._validate_threads(24, True, screen='passed', screen_threads=12)

    def test_checkpoint_scan_keeps_the_full_audit_reservation(self):
        self._validate_threads(16, True, screen='passed', scan=True)
        self._validate_threads(16, True, native_rejection=True, scan=True)

    def _validate_threads(self, expected, configured, gate_code=0, driver_changed=False,
                          native_rejection=False, screen=None, screen_threads=None, scan=False):
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
            if screen is not None:
                cp['full_parse_screen'] = True
            if screen_threads is not None:
                cp['parse_screen_threads'] = screen_threads
            order = []
            audit_fds = []
            def git(_source, *args):
                return 'candidate' if args[0] == 'rev-parse' else ''
            def checks(run, directory, grammar, names, data, threads):
                order.append('checks')
                self.assertEqual(threads, expected)
                self.assertEqual(len(batch.r._LEASES), expected)
                audit_fds.extend(batch.r._LEASES)
                for fd in batch.r._LEASES:
                    os.fstat(fd)
                (directory/'rows.jsonl').write_text('rows')
                if native_rejection:
                    raise subprocess.CalledProcessError(1, ['validate-readings'])
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
                (args.run/'frozen').mkdir()
                batch.r.write(args.run/'manifest.json', {'files': batch.r.tree(args.run/'frozen')})
            def screen_run(run, directory, grammar, data, threads):
                order.append('screen')
                self.assertEqual(threads, screen_threads or 6)
                self.assertEqual(len(batch.r._LEASES), threads)
                for fd in batch.r._LEASES:
                    os.fstat(fd)
                with batch.retire_cpu.reserve(Path(cp['cpu_pool']), 32-threads, 32, wait=False) as available:
                    self.assertEqual(len(available), 32-threads)
                directory.mkdir()
                result = {'status': screen, 'failures': ['bad'] if screen == 'failed' else []}
                batch.r.write(directory/'result.json', result)
                return result
            def scan_input(args):
                order.append('scan')
                self.assertEqual(args.run, folder/'validation-inputs')
                self.assertIsNone(args.rules)
                self.assertEqual(batch.r._LEASES, audit_fds)
                for fd in audit_fds:
                    os.fstat(fd)
                self.assertEqual(batch.r.read(folder/'validation.json')['status'], 'passed')
                with self.assertRaises(BlockingIOError):
                    with batch.retire_cpu.reserve(Path(cp['cpu_pool']), 33-expected, 32, wait=False):
                        self.fail('the full audit lease was released before the scan')
            with patch.object(batch.retire_parse_screen, 'run', side_effect=screen_run), patch.dict(os.environ, {'ORACLE_GRAMMAR_DIR': '/wrong/checkout/grammar', 'ORACLE_GRAMMAR_POISON': 'wrong'}), patch.object(batch, 'REPOSITORY_DRIVER', driver), patch.object(batch, '_git', side_effect=git), patch.object(batch.r, 'freeze', side_effect=freeze_input) as freeze, patch.object(batch.w, 'payload', return_value=({'0': {'name': 'Example'}}, [], [])), patch.object(batch.w, 'checks', side_effect=checks) as full_checks, patch.object(batch.subprocess, 'run', side_effect=gate), patch.object(batch.retire_native_outcome, 'preservation_failure', return_value={'faces': 1, 'failures': ['native readings changed or failed validation']}) as recovered, patch.object(batch.r, 'scan', side_effect=scan_input):
                result = batch.validate(manifest, cp, scan=scan)
            self.assertEqual(freeze.call_args.args[0].threads, expected)
            self.assertEqual(freeze.call_args.args[0].slots, 1)
            if screen is not None:
                self.assertEqual(result['parse_screen_threads'], screen_threads or 6)
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
            if screen == 'failed':
                self.assertEqual(order, ['freeze', 'gate', 'screen'])
                full_checks.assert_not_called()
                self.assertEqual(result['reason'], 'full allowed-population parse screen failed')
                self.assertNotIn('full_checks', result)
                self.assertEqual(result['parse_screen']['status'], 'failed')
                self.assertEqual(batch.r._LEASES, [])
                return
            self.assertEqual(order, ['freeze', 'gate'] + (['screen'] if screen else []) + ['checks']
                             + (['scan'] if scan and not native_rejection else []))
            if native_rejection:
                recovered.assert_called_once()
                self.assertEqual(result['status'], 'failed')
                self.assertEqual(result['reason'], 'full allowed-population checks failed')
                self.assertEqual(result['full_checks']['faces'], 1)
                self.assertTrue(result['full_checks']['failures'])
                self.assertNotIn('rows', result)
                self.assertEqual(batch.r._LEASES, [])
                return
            recovered.assert_not_called()
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

    def test_validate_rejects_invalid_parse_screen_budgets(self):
        for threads in (False, 0, 17, '12'):
            with self.subTest(threads=threads), self.assertRaisesRegex(ValueError, 'parse_screen_threads'):
                batch._validate({'status': 'provisional'}, {'parse_screen_threads': threads}, 12)

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

    def test_base_control_reuses_exact_positive_receipt_and_refreshes_after_tampering(self):
        with tempfile.TemporaryDirectory() as tmp:
            campaign = Path(tmp)
            inputs, grammar = campaign/'inputs', campaign/'grammar'
            inputs.mkdir()
            grammar.mkdir()
            source = grammar/'syntax.ebnf'
            source.write_text('sample ::= "sample"\n')
            batch.r.write(inputs/'manifest.json', {'files': {'grammar/syntax.ebnf': batch.r.digest(source)}})
            control = {'run': inputs, 'manifest_sha256': batch.r.digest(inputs/'manifest.json'),
                       'profile_sha256': 'profile', 'threads': 2, 'occurrences': 2}
            original, restored, refreshed = ((campaign/'.cache/batches'/name/'native-control-base')
                                               for name in ('original', 'restored', 'refreshed'))
            for directory in (original, restored, refreshed):
                directory.parent.mkdir(parents=True)
            result = {'faces': 2, 'failures': []}

            def check(cp, controls, grammar_dir, directory):
                directory.mkdir()
                for name in batch.BASE_CONTROL_ARTIFACTS - {'result.json'}:
                    (directory/name).write_text(name)
                batch.r.write(directory/'result.json', result)
                return result

            with patch.object(batch, '_control_verify'), patch.object(batch, '_control_check', side_effect=check) as run:
                self.assertEqual(batch._control_baseline(campaign, {}, control, grammar, original), result)
                self.assertEqual(batch._control_baseline(campaign, {}, control, grammar, restored), result)
                run.assert_called_once()
                self.assertEqual(batch.r.read(restored/'reuse.json')['origin'], str(original))
                self.assertEqual(batch.r.tree(original), {k: v for k, v in batch.r.tree(restored).items() if k != 'reuse.json'})
                (original/'rows.jsonl').write_text('changed')
                self.assertEqual(batch._control_baseline(campaign, {}, control, grammar, refreshed), result)
                self.assertEqual(run.call_count, 2)
                self.assertFalse((refreshed/'reuse.json').exists())

    def test_native_focus_requires_a_named_and_complete_rejection(self):
        control = {'occurrences': 2, 'names': ['A', 'B']}
        print_loss = {'face': 'A', 'lost': ['exact_default']}
        failure = {'face': 'B', 'gate': 'parse/reading-count/text'}
        cases = [(1, [print_loss]), (2, [print_loss, failure]),
                 (2, ['native readings changed or failed validation']),
                 (2, [{'face': 'elsewhere', 'lost': ['exact_default']}])]
        self.assertEqual(batch._retained_native_failure(control, {'faces': 2, 'failures': [print_loss]}),
                         {'face': 'A', 'failures': [print_loss]})
        for faces, failures in cases:
            with self.subTest(faces=faces, failures=failures):
                self.assertIsNone(batch._retained_native_failure(control, {'faces': faces, 'failures': failures}))

    def test_native_focus_accepts_a_bounded_group_with_the_same_print_loss(self):
        names = [f'Card {index:02d}' for index in range(17)]
        control = {'occurrences': len(names), 'names': names}
        failures = [{'face': name, 'lost': ['exact_default']} for name in reversed(names)]
        report = {'faces': len(names), 'failures': failures[:16]}
        self.assertEqual(batch._retained_native_failure(control, report),
                         {'faces': names[1:], 'failures': failures[:16]})
        mixed = [failures[0], {'face': failures[1]['face'], 'lost': ['default']}]
        self.assertIsNone(batch._retained_native_failure(control, {**report, 'failures': mixed}))
        self.assertIsNone(batch._retained_native_failure(control, {**report, 'failures': failures}))

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
            def proposal(p, *, input_cache=None):
                name = p['job_id']
                return root/name, {'fixture': 'families/existing.json' if name == 'overwrite' else f'families/{name}.json',
                                   'rules': ['m_'+('old' if name == 'conflict' else 'good')]}, root/name, []
            def merge(base, current, proposed, rules, hooks, *, canonical_removal):
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
            def proposal(p, *, input_cache=None):
                name = p['job_id']
                return root/name, {'fixture': f'families/{name}.json', 'rules': [f'm_{name}']}, root/name, []
            def merge(base, trial, proposed, rules, hooks, *, canonical_removal):
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
                    patch.object(batch, '_mechanical_removal'), \
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
            def proposal(p, *, input_cache=None):
                name = p['job_id']
                return root/name, {'fixture': f'families/{name}.json', 'rules': [f'm_{name}']}, root/name, []
            def merge(base, trial, proposed, rules, hooks, *, canonical_removal):
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
        for mode in ('pass', 'selected-pass', 'selected-family-interaction', 'two-groups', 'family-interaction', 'native-interaction', 'library-interaction', 'structural', 'missing-metadata'):
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
                          stage_group_size=2 if mode in ('two-groups', 'selected-pass', 'selected-family-interaction') else 4)
                report = {'task': 'task', 'source': commit, 'descriptor_sha256': 'd',
                          'support_sha256': 's', 'fixture_sha256': 'f', 'edited': []}
                names = ('first', 'bad', 'last', 'mech')
                proposals = [{'job_id': name, 'attempt_id': name, 'report': dict(report),
                              'kind': 'mechanical' if name == 'mech' else 'author'} for name in names]
                if mode == 'missing-metadata':
                    del proposals[1]['report']['descriptor_sha256']
                def proposal(item, *, input_cache=None):
                    name = item['job_id']
                    if mode == 'structural' and name == 'bad':
                        raise ValueError('bad immutable report')
                    return root/name, {'fixture': f'families/{name}.json', 'rules': [f'm_{name}']}, root/name, []
                def merge(base, trial, proposed, rules, hooks, *, canonical_removal):
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
                    if mode in ('family-interaction', 'selected-family-interaction') and 'first\n' in text and 'bad\n' in text:
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
                        patch.object(batch, '_family', return_value={'family': 'new', 'cases': []}) as family_prepare, \
                        patch.object(batch.merger, 'merge_grammar', side_effect=merge), \
                        patch.object(batch.mined, 'remove', side_effect=remove), \
                        patch.object(batch, '_mechanical_removal'), \
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
                    family_prepare.reset_mock()
                    result = batch.stage(root, cp, proposals, combine_selected=mode.startswith('selected-'))
                    caches = [call.kwargs['gold_cache'] for call in family_prepare.call_args_list]
                    self.assertTrue(caches)
                    self.assertEqual(len({id(cache) for cache in caches}), 1)
                self.assertEqual(result['status'], 'provisional')
                self.assertEqual(result['stage_group_size'], 2 if mode == 'two-groups' else 4)
                self.assertIs(result['combine_selected'], mode.startswith('selected-'))
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
                elif mode in ('pass', 'selected-pass'):
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

    def test_oversized_selected_failure_uses_guarded_subgroups(self):
        for mode in ('clean-subgroups', 'bad-member', 'cross-interaction', 'fatal-integrity',
                     'trailing-single', 'equal-size', 'configured-singletons',
                     'isolated-family', 'multiple-families', 'family-reordered-pass',
                     'family-survivors-still-fail', 'removed-rule', 'merge-conflicts',
                     'retained-family', 'retained-family-interaction', 'retained-family-after-deletion',
                     'retained-family-mutated-prefix', 'retained-native-after-merge',
                     'retained-native-not-reproduced'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                repo = root/'repo'
                (repo/'grammar/syntax').mkdir(parents=True)
                (repo/'tests').mkdir()
                focused = mode.startswith('retained-family')
                native = mode.startswith('retained-native')
                fixture = 'families/retained.json'
                if focused:
                    (repo/'families').mkdir()
                    batch.r.write(repo/fixture, {'family': 'retained', 'cases': []})
                batch.r.write(repo/'tests/families.json', [fixture] if focused else [])
                count = 9 if mode == 'trailing-single' else 4 if mode in ('equal-size', 'configured-singletons') else 8
                names = [f'j{index}' for index in range(count)]
                (repo/'grammar/syntax/mined.ebnf').write_text(
                    ''.join(f'm_{name} ::= "{name}"\n' for name in names))
                subprocess.run(['git', 'init', '-q', str(repo)], check=True)
                subprocess.run(['git', 'add', '.'], cwd=repo, check=True)
                subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=t@x',
                                'commit', '-qm', 'base'], cwd=repo, check=True)
                commit = batch._git(repo, 'rev-parse', 'HEAD')
                report = {'task': 'task', 'source': commit, 'descriptor_sha256': 'd',
                          'support_sha256': 's', 'fixture_sha256': 'f', 'edited': []}
                proposals = [{'job_id': name, 'attempt_id': name, 'report': dict(report)} for name in names]
                def proposal(item, *, input_cache=None):
                    name = item['job_id']
                    return root/name, {'fixture': f'families/{name}.json', 'rules': [f'm_{name}']}, root/name, []
                def merge(base, trial, proposed, rules, hooks, *, canonical_removal):
                    name = rules[0][2:]
                    path = trial/'grammar/syntax/shared.ebnf'
                    prior = path.read_text() if path.exists() else ''
                    competing = {'j2': 'j0', 'j6': 'j4'}
                    if ((mode == 'merge-conflicts' or native and name == 'j2')
                            and name in competing and competing[name] in prior.splitlines()):
                        raise batch.merger.MergeConflict('conflicting addition or deletion')
                    return {'grammar/syntax/shared.ebnf': prior+name+'\n'}
                def remove(trial, rules):
                    path = trial/'grammar/syntax/mined.ebnf'
                    name = rules[0][2:]
                    path.write_text(path.read_text().replace(f'm_{name} ::= "{name}"\n', ''))
                    return rules, {}
                calls = []
                def family(checkpoint, source, fixtures, log, grammar_dir=None, *, threads=None):
                    shared = grammar_dir/'syntax/shared.ebnf'
                    text = shared.read_text().splitlines() if shared.exists() else []
                    calls.append((log.parent.name, text))
                    log.write_text('family proof')
                    if native:
                        return
                    if focused:
                        if log.parent.name.startswith('probe-'):
                            self.assertEqual(threads, 2)
                            if mode == 'retained-family-mutated-prefix':
                                (source/'grammar/syntax/mined.ebnf').write_text('unexpected grammar write')
                                raise ValueError('helper aborted')
                        if mode == 'retained-family-after-deletion' and log.parent.name == 'group-000':
                            raise batch._FamilyCheckFailure(log)
                        failed = ('j1' in text and 'j6' in text if mode == 'retained-family-interaction'
                                  else 'j6' in text)
                        if failed:
                            batch.r.write(log.parent/'family-000.log',
                                          {'failures': [{'name': 'retained example', 'why': 'unexpected reading'}]})
                            raise batch._FamilyCheckFailure(log, (0,))
                        return
                    if log.parent.name == 'group-000':
                        if mode == 'removed-rule':
                            raise batch._FamilyCheckFailure(log)
                        if mode == 'multiple-families':
                            raise batch._FamilyCheckFailure(log, (2, 6))
                        if mode in ('isolated-family', 'family-reordered-pass', 'family-survivors-still-fail'):
                            raise batch._FamilyCheckFailure(log, (2,))
                        raise ValueError('initial union conflict')
                    if log.parent.name == 'proposal-002' and mode in ('isolated-family', 'multiple-families', 'removed-rule'):
                        raise ValueError('exact print failure reproduced')
                    if mode == 'family-survivors-still-fail' and log.parent.name == 'without-failed-families':
                        raise ValueError('remaining grammar does not pass')
                    if mode == 'fatal-integrity' and log.parent.name == 'subgroup-004':
                        raise batch._UncertainPromotion('accepted prefix was modified')
                    if mode == 'bad-member' and 'j2' in text:
                        raise ValueError('bad member')
                    if mode == 'cross-interaction' and 'j0' in text and 'j4' in text:
                        raise ValueError('cross-group interaction')
                cp = {'source': str(repo), 'commit': commit,
                      'stage_group_size': 1 if mode == 'configured-singletons' else 4}
                if native:
                    cp.update(control_fixture(root, commit))
                native_calls = []

                def closure(inputs, detail, *, max_occurrences):
                    names = ['A'] if max_occurrences == 16 else ['A', 'B']
                    return {'names': names, 'closure_occurrence_count': len(names)}

                def native_check(run, directory, grammar, cohort, data, threads):
                    shared = grammar/'syntax/shared.ebnf'
                    contents = shared.read_text().splitlines() if shared.exists() else []
                    native_calls.append((directory.parent.name, list(cohort), contents, threads))
                    failed = 'j6' in contents and (len(cohort) > 1 or mode != 'retained-native-not-reproduced')
                    veto = ['native readings changed or failed validation',
                            {'face': 'A', 'gate': 'parse/reading-count/text'}]
                    return {'faces': len(cohort), 'failures': veto if failed else []}

                with patch.object(batch, '_proposal', side_effect=proposal), \
                        patch.object(batch, '_family', return_value={'family': 'test', 'cases': []}), \
                        patch.object(batch.merger, 'merge_grammar', side_effect=merge), \
                        patch.object(batch.mined, 'remove', side_effect=remove), \
                        patch.object(batch.a, 'apply_hooks'), patch.object(batch, '_helper', side_effect=family), \
                        patch.object(batch, '_control_baseline'), \
                        patch.object(batch.tasks, 'close_cohort', side_effect=closure), \
                        patch.object(batch.w, 'checks', side_effect=native_check), \
                        patch.object(batch, '_rule_reference_conflict', return_value=(
                            {'without_proposal_indices': [2]}
                            if mode in ('removed-rule', 'retained-family-after-deletion') else None)):
                    result = batch.stage(root, cp, proposals, combine_selected=True)
                if native:
                    merge_retry = batch.r.read(result['merge_retry_attempts'][0]['proof']['path'])
                    self.assertEqual(merge_retry['retained_native_failure']['face'], 'A')
                    probes = sorted((Path(result['folder'])/'retained-native-000').glob('probe-*/probe.json'))
                    outcomes = [batch.r.read(path)['outcome'] for path in probes]
                    self.assertEqual(outcomes[0], 'passed')
                    if mode == 'retained-native-not-reproduced':
                        self.assertEqual(outcomes, ['passed', 'passed'])
                        self.assertNotIn('retained_native_retry', result)
                        self.assertIn('fallback_trials', result)
                    else:
                        self.assertEqual(outcomes[1], 'reproduced')
                        self.assertNotIn('fallback_trials', result)
                        receipt = result['retained_native_retry']
                        self.assertEqual(receipt['without_proposal_indices'], [2, 6])
                        self.assertEqual(receipt['status'], 'passed')
                        self.assertTrue(any(label == 'without-retained-native-regression' and cohort == ['A', 'B']
                                            for label, cohort, _, _ in native_calls))
                        self.assertTrue(all(cohort == ['A'] and threads == 2 for label, cohort, _, threads in native_calls
                                            if label.startswith('probe-')))
                    continue
                if mode == 'retained-family-mutated-prefix':
                    self.assertEqual(result['status'], 'failed')
                    self.assertIn('prefix changed', result['reason'])
                    self.assertEqual(result['jobs'], [])
                    self.assertNotIn('retained_family_retry', result)
                    continue
                if focused:
                    observed = result['group_trials'][0]['proof']
                    if mode == 'retained-family-after-deletion':
                        observed = result['dependency_retry']['proof']
                    diagnosis = batch.r.read(Path(observed['path']))['retained_family_failure']
                    self.assertEqual(diagnosis['fixture'], fixture)
                    self.assertEqual(diagnosis['failures'][0]['name'], 'retained example')
                    self.assertEqual([row['job_id'] for row in result['jobs'] if row['status'] != 'included'], ['j6'])
                    probes = list((Path(result['folder'])/'retained-family-000').glob('probe-*/probe.json'))
                    self.assertTrue(probes)
                    self.assertLessEqual(len(probes), 7)
                    if mode == 'retained-family-interaction':
                        self.assertIn('fallback_trials', result)
                        retry = result['retained_family_retry']
                        self.assertEqual(retry['without_proposal_indices'], [6])
                        self.assertIn('subgroup-004-without-retained-family-regression', retry['proof']['path'])
                    else:
                        self.assertNotIn('fallback_trials', result)
                        retry = result['retained_family_retry']
                        self.assertEqual(retry['status'], 'passed')
                        self.assertEqual(retry['without_proposal_indices'],
                                         [2, 6] if mode == 'retained-family-after-deletion' else [6])
                        self.assertNotIn('j6', retry['jobs'])
                        self.assertIn('without-retained-family-regression', [name for name, _ in calls])
                        self.assertIn('proposal-006', [name for name, _ in calls])
                    continue
                if mode == 'merge-conflicts':
                    self.assertNotIn('fallback_trials', result)
                    self.assertEqual([name for name, _ in calls], ['without-merge-conflict-002'])
                    attempts = result['merge_retry_attempts']
                    self.assertEqual([x['without_proposal_indices'] for x in attempts], [[2, 6]])
                    self.assertEqual([x['status'] for x in attempts], ['passed'])
                    initial = batch.r.read(Path(result['group_trials'][0]['proof']['path']))
                    self.assertEqual(initial['merge_conflict_at'], 2)
                    self.assertEqual([item['proposal_index'] for item in initial['merge_conflicts']], [2, 6])
                    self.assertEqual(initial['prefix_grammar'],
                                     batch.r.read(Path(attempts[0]['proof']['path']))['prefix_grammar'])
                    self.assertEqual({x['job_id'] for x in result['jobs'] if x['status'] != 'included'}, {'j2', 'j6'})
                    self.assertEqual(result['rules'], [f'm_{name}' for name in names if name not in ('j2', 'j6')])
                    continue
                if mode == 'equal-size':
                    self.assertNotIn('fallback_group_size', result)
                    self.assertNotIn('fallback_trials', result)
                    self.assertEqual([name for name, _ in calls], ['group-000', 'proposal-000', 'proposal-001',
                                                                  'proposal-002', 'proposal-003'])
                    self.assertEqual([x['status'] for x in result['jobs']], ['included']*4)
                    continue
                if mode in ('isolated-family', 'multiple-families', 'family-reordered-pass', 'removed-rule'):
                    withheld = [2, 6] if mode == 'multiple-families' else [2]
                    retained = [name for index, name in enumerate(names) if index not in withheld]
                    self.assertNotIn('fallback_trials', result)
                    label, receipt = ('without-conflicting-removal', 'dependency_retry') if mode == 'removed-rule' else (
                        'without-failed-families', 'family_retry')
                    self.assertEqual([name for name, _ in calls],
                                     ['group-000', label] +
                                     [f'proposal-{index:03d}' for index in withheld])
                    self.assertEqual(result[receipt]['status'], 'passed')
                    self.assertEqual(result[receipt]['without_proposal_indices'], withheld)
                    self.assertEqual([x['job_id'] for x in result['jobs']], retained+[names[i] for i in withheld])
                    bad = set() if mode == 'family-reordered-pass' else {'j2'}
                    self.assertEqual({x['job_id'] for x in result['jobs'] if x['status'] != 'included'}, bad)
                    self.assertTrue(all(x[receipt] == result[receipt]['proof'] for x in result['jobs']))
                    text = (Path(result['source'])/'grammar/syntax/shared.ebnf').read_text().splitlines()
                    self.assertEqual(text, retained+[names[i] for i in withheld if names[i] not in bad])
                    continue
                self.assertEqual(result['fallback_group_size'], 1 if mode == 'configured-singletons' else 4)
                self.assertEqual(result['group_trials'][0]['status'], 'failed')
                if mode == 'fatal-integrity':
                    self.assertEqual(result['status'], 'failed')
                    self.assertEqual(result['failure_scope'], 'combined-batch')
                    self.assertEqual([name for name, _ in calls], ['group-000', 'subgroup-000', 'subgroup-004'])
                    self.assertEqual([item['job_id'] for item in result['jobs']], names[:4])
                    self.assertTrue(result.get('failed_candidate_commit'))
                    continue
                self.assertEqual(result['status'], 'provisional')
                self.assertEqual([x['job_id'] for x in result['jobs']], names)
                if mode == 'configured-singletons':
                    self.assertNotIn('fallback_trials', result)
                    self.assertEqual([name for name, _ in calls], ['group-000', 'proposal-000', 'proposal-001',
                                                                  'proposal-002', 'proposal-003'])
                    self.assertEqual([x['status'] for x in result['jobs']], ['included']*4)
                    continue
                self.assertEqual(len(result['fallback_trials']), 2)
                full = result['group_trials'][0]['proof']
                self.assertTrue(all(item['group_attempt'] == full for item in result['jobs']))
                if mode in ('clean-subgroups', 'trailing-single', 'family-survivors-still-fail'):
                    expected_calls = ['group-000', 'subgroup-000', 'subgroup-004']
                    if mode == 'trailing-single': expected_calls.append('proposal-008')
                    if mode == 'family-survivors-still-fail':
                        self.assertEqual(result['family_retry']['status'], 'failed')
                        expected_calls.insert(1, 'without-failed-families')
                    self.assertEqual([name for name, _ in calls], expected_calls)
                    self.assertEqual([x['status'] for x in result['jobs']], ['included']*count)
                    self.assertEqual([x['status'] for x in result['fallback_trials']], ['passed', 'passed'])
                else:
                    rejected = 'j2' if mode == 'bad-member' else 'j4'
                    statuses = {x['job_id']: x['status'] for x in result['jobs']}
                    self.assertEqual(statuses[rejected], 'integration-rejected')
                    self.assertEqual(sum(value == 'included' for value in statuses.values()), 7)
                    affected = result['jobs'][0:4] if mode == 'bad-member' else result['jobs'][4:8]
                    failed = next(x['proof'] for x in result['fallback_trials'] if x['status'] == 'failed')
                    self.assertTrue(all(item['subgroup_attempt'] == failed for item in affected))
                    text = (Path(result['source'])/'grammar/syntax/shared.ebnf').read_text().splitlines()
                    self.assertEqual(text, [name for name in names if name != rejected])
                    self.assertFalse((Path(result['source'])/f'families/{rejected}.json').exists())

    def test_mixed_fallback_can_validate_the_mechanical_prefix_together(self):
        for prefix_passes in (True, False):
            with self.subTest(prefix_passes=prefix_passes), tempfile.TemporaryDirectory() as temporary:
                folder = Path(temporary)
                entries = [(i, {'job_id': f'job-{i}', 'kind': 'mechanical' if i < 8 else 'author'})
                           for i in range(12)]
                context = {'folder': folder, 'manifest': {'jobs': []}}
                calls = []

                def trial(ctx, proposals, label):
                    jobs = [item['job_id'] for _, item in proposals]
                    calls.append((label, jobs))
                    passed = label != 'mechanical-prefix' or prefix_passes
                    proof = {'jobs': jobs, 'status': 'passed' if passed else 'failed'}
                    return [{'job_id': job, 'trial_proof': {'path': label}} for job in jobs], proof

                whole_proof = {'path': 'failed mixed trial'}
                with patch.object(batch, '_stage_trial', side_effect=trial):
                    batch._subgroup_fallback(context, entries, whole_proof, 4)
                expected = ['mechanical-prefix', 'subgroup-008'] if prefix_passes else [
                    'mechanical-prefix', 'subgroup-000', 'subgroup-004', 'subgroup-008']
                self.assertEqual([label for label, _ in calls], expected)
                self.assertEqual(calls[0][1], [f'job-{i}' for i in range(8)])
                manifest = batch.r.read(folder/'manifest.json')
                self.assertEqual([item['job_id'] for item in manifest['jobs']],
                                 [item['job_id'] for _, item in entries])
                self.assertTrue(all(item['group_attempt'] == whole_proof for item in manifest['jobs']))
                self.assertEqual(manifest['fallback_trials'][0]['status'], 'passed' if prefix_passes else 'failed')

    def test_unknown_compiler_rule_finds_only_a_unique_proposal_deletion(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            declaration = 'choose_land_type ::= "choose" "a" "land" "type"\n'
            def grammar(folder, text):
                syntax = folder/'grammar/syntax'
                syntax.mkdir(parents=True, exist_ok=True)
                (syntax/'actions.ebnf').write_text(text)
            grammar(root/'source', declaration)
            entries = []
            for index in range(2):
                run = root/f'run-{index}'
                response = root/f'report-{index}'
                grammar(run/'inputs/frozen', declaration)
                grammar(response, declaration if index == 0 else '/*\n'+declaration+'*/\n')
                entries.append((index, {'run': str(run), 'report': {'directory': str(response)}}))
            log = root/'family.log'
            log.write_text('Error: "choice_and_name_first: unknown rule choose_land_type"\n')
            observed = batch._rule_reference_conflict(root/'source', entries, log)
            self.assertEqual(observed, {'reference':'choice_and_name_first', 'missing_rule':'choose_land_type',
                                        'without_proposal_indices':[1]})
            grammar(root/'report-0', 'choose_new ::= "choose"\n')
            self.assertIsNone(batch._rule_reference_conflict(root/'source', entries, log))
            log.write_text('Error: "some other compiler message"\n')
            self.assertIsNone(batch._rule_reference_conflict(root/'source', entries, log))

    def test_failing_subgroups_share_guards_after_a_witnessed_single_culprit(self):
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            entries = [(i, {'job_id': f'job-{i}'}) for i in range(8)]
            context = {'folder': folder, 'manifest': {'jobs': []}}
            calls = []

            def trial(_, proposals, label):
                indices = [i for i, _ in proposals]
                jobs = [item['job_id'] for _, item in proposals]
                calls.append((label, indices))
                proof = {'jobs': jobs, 'status': 'passed'}
                if label in ('subgroup-000', 'subgroup-004'):
                    proof.update(status='failed', merge_conflict_at=indices[1])
                if label in ('proposal-001', 'proposal-005'):
                    proof.update(status='failed')
                return [{'job_id': job, 'trial_proof': {'path': label}} for job in jobs], proof

            with patch.object(batch, '_stage_trial', side_effect=trial):
                batch._subgroup_fallback(context, entries, {'path': 'whole'}, 4)
            self.assertEqual(calls, [
                ('subgroup-000', [0, 1, 2, 3]), ('subgroup-000-without-merge-conflict', [0, 2, 3]),
                ('proposal-001', [1]), ('subgroup-004', [4, 5, 6, 7]),
                ('subgroup-004-without-merge-conflict', [4, 6, 7]), ('proposal-005', [5])])
            self.assertEqual([job['subgroup_attempt']['path'] for job in context['manifest']['jobs']],
                             ['subgroup-000']*4+['subgroup-004']*4)

    def test_multiple_unknown_rules_require_a_unique_owner_for_each_diagnostic(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            declarations = ('choose_land_type ::= "choose" "a" "land" "type"\n',
                            'event_object_mutates ::= object_ref "mutates"\n')

            def grammar(folder, text):
                path = folder/'grammar/syntax/actions.ebnf'
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)

            grammar(root/'source', ''.join(declarations))
            entries = []
            for index in range(2):
                run, response = root/f'run-{index}', root/f'report-{index}'
                grammar(run/'inputs/frozen', ''.join(declarations))
                grammar(response, declarations[1-index])
                entries.append((index, {'run': str(run), 'report': {'directory': str(response)}}))
            log = root/'family.log'
            diagnostic = ('choice_and_name_first: unknown rule choose_land_type\\n'
                          'event_first_subject: unknown rule event_object_mutates')
            log.write_text(f'Error: "{diagnostic}"\n')
            observed = batch._rule_reference_conflict(root/'source', entries, log)
            self.assertEqual(observed['without_proposal_indices'], [0, 1])
            self.assertEqual([item['missing_rule'] for item in observed['conflicts']],
                             ['choose_land_type', 'event_object_mutates'])
            log.write_text(f'Error: "{diagnostic}\\nan unrelated compiler failure"\n')
            self.assertIsNone(batch._rule_reference_conflict(root/'source', entries, log))
            grammar(root/'report-0', '')
            log.write_text(f'Error: "{diagnostic}"\n')
            self.assertIsNone(batch._rule_reference_conflict(root/'source', entries, log))

    def test_merge_retry_can_follow_multiple_missing_rules_with_fresh_guards(self):
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            entries = [(i, {'job_id': f'job-{i}'}) for i in range(8)]
            context = {'folder': folder, 'manifest': {'jobs': []}}
            calls = []

            def trial(ctx, proposals, label):
                jobs = [item['job_id'] for _, item in proposals]
                calls.append((label, jobs))
                proof = {'jobs': jobs, 'status': 'passed'}
                if label == 'without-merge-conflict-004':
                    proof.update(status='failed', removed_rule_conflict={'without_proposal_indices': [4, 5]})
                return [{'job_id': job, 'trial_proof': {'path': label}} for job in jobs], proof

            with patch.object(batch, '_stage_trial', side_effect=trial):
                self.assertTrue(batch._retry_without(context, entries, {'path': 'whole'},
                                                     [0, 1, 2, 3], 8, route='merge'))
            self.assertEqual(calls[:2], [('without-merge-conflict-004', ['job-4', 'job-5', 'job-6', 'job-7']),
                                         ('without-merge-conflict-006', ['job-6', 'job-7'])])
            self.assertEqual([name for name, _ in calls[2:]], [f'proposal-{i:03d}' for i in range(6)])
            self.assertEqual(context['manifest']['merge_retry']['without_proposal_indices'], list(range(6)))

    def test_retained_family_retry_can_route_a_second_family_then_a_native_veto(self):
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            entries = [(i, {'job_id': f'job-{i}'}) for i in range(8)]
            context = {'folder': folder, 'manifest': {'jobs': []}}
            calls, probes = [], []

            def family(_, selection, failure, output):
                output.mkdir()
                probes.append(output.name)
                return failure['culprit']

            def trial(_, proposals, label):
                (folder/label).mkdir()
                jobs = [item['job_id'] for _, item in proposals]
                calls.append((label, jobs))
                proof = {'jobs': jobs, 'status': 'passed'}
                if label == 'without-retained-family-regression':
                    proof.update(status='failed', retained_family_failure={'culprit': 2})
                if label == 'without-retained-family-regression-004':
                    proof.update(status='failed', retained_native_failure={'face': 'B'})
                return [{'job_id': job, 'trial_proof': {'path': label}} for job in jobs], proof

            with patch.object(batch, '_stage_trial', side_effect=trial), \
                 patch.object(batch, '_isolate_retained_family', side_effect=family), \
                 patch.object(batch, '_isolate_retained_native', return_value=4) as native:
                self.assertTrue(batch._retry_retained_control(context, entries,
                    {'retained_family_failure': {'culprit': 1}}, {'path': 'whole'}, 5, [0, 3]))
            self.assertEqual(probes, ['retained-family-000', 'retained-family-000-003'])
            self.assertEqual([index for index, _ in native.call_args.args[1]], [4, 5, 6, 7])
            self.assertEqual(calls[2], ('without-retained-native-regression', ['job-5', 'job-6', 'job-7']))
            self.assertEqual([label for label, _ in calls[3:]], [f'proposal-{i:03d}' for i in range(5)])
            self.assertEqual(len(context['manifest']['jobs']), 8)

    def test_stale_rule_reference_routes_only_a_unique_new_symbolic_use(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def grammar(folder, text):
                path = folder/'grammar/syntax/actions.ebnf'
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            old = 'counter_named_spell ::= "counter" named_spell\n'
            use = 'instead_counter_named ::= counter_named_spell\n'
            grammar(root/'source', 'counter_shared ::= "counter" named_spell\n')
            entries = []
            for index in range(2):
                run, response = root/f'run-{index}', root/f'report-{index}'
                grammar(run/'inputs/frozen', old)
                grammar(response, old+(use if index else 'instead_counter_named ::= "counter_named_spell"\n'))
                entries.append((index, {'run':str(run), 'report':{'directory':str(response)}}))
            log = root/'family.log'
            log.write_text('Error: "instead_counter_named: unknown rule counter_named_spell"\n')
            self.assertEqual(batch._rule_reference_conflict(root/'source', entries, log),
                             {'reference':'instead_counter_named','missing_rule':'counter_named_spell',
                              'without_proposal_indices':[1],'stale_reference':True})
            grammar(root/'report-0', old+use)
            self.assertIsNone(batch._rule_reference_conflict(root/'source', entries, log))

    def test_proposals_share_frozen_hashes_and_detect_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            frozen = root/'checkpoint/frozen'
            frozen.mkdir(parents=True)
            gold = frozen/'gold.jsonl'
            gold.write_text('baseline\n')
            manifest = {'commit': 'checkpoint', 'files': batch.r.tree(frozen)}
            batch.r.write(frozen.parent/'manifest.json', manifest)
            aliases = [root/'attempt-one', root/'attempt-two']
            for alias in aliases:
                alias.symlink_to(frozen.parent, target_is_directory=True)
            cache = batch._ProposalInputCache()
            with patch.object(batch.r, 'tree', wraps=batch.r.tree) as hashed:
                for alias in aliases:
                    self.assertEqual(cache.read(alias), manifest)
                self.assertTrue(cache.unchanged())
                later = cache.trial()
                self.assertEqual(later.read(aliases[0]), manifest)
                self.assertTrue(later.unchanged())
                hashed.assert_called_once_with(frozen)
            gold.write_text('modified\n')
            self.assertFalse(cache.unchanged())
            self.assertFalse(later.unchanged())
            self.assertTrue(cache.trial().unchanged())
            with self.assertRaisesRegex(ValueError, 'frozen inputs changed'):
                cache.read(aliases[1])
            with self.assertRaisesRegex(ValueError, 'frozen inputs changed'):
                cache.trial().read(aliases[0])

            gold.write_text('baseline\n')
            new_cache = batch._ProposalInputCache()
            self.assertEqual(new_cache.read(aliases[0]), manifest)
            aliases[0].unlink()
            aliases[0].symlink_to(root/'a-different-checkpoint', target_is_directory=True)
            self.assertFalse(new_cache.unchanged())
            with self.assertRaisesRegex(ValueError, 'input location changed'):
                new_cache.read(aliases[0])

    def test_canonical_removal_reuse_requires_the_current_verified_base(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            grammar = root/'checkpoint/frozen/grammar/syntax'
            grammar.mkdir(parents=True)
            source = grammar/'mined.ebnf'
            source.write_text('mined_action ::= m_a | m_b | never_action\n'
                              'm_a ::= "a"\nm_b ::= "b"\nnever_action ::= "never"\n')
            frozen = grammar.parents[1]
            alias = root/'attempt-inputs'
            alias.symlink_to(frozen.parent, target_is_directory=True)
            batch.r.write(frozen.parent/'manifest.json', {'files': batch.r.tree(frozen)})
            cache = batch._ProposalInputCache()
            with patch.object(batch.merger, 'canonical_seeded_removal',
                              wraps=batch.merger.canonical_seeded_removal) as canonical:
                with self.assertRaisesRegex(ValueError, 'verified proposal base'):
                    cache.canonical_seeded_removal(alias/'frozen', ['m_a'])
                cache.read(alias)
                first = cache.canonical_seeded_removal(alias/'frozen', ['m_a'])
                next_trial = cache.trial()
                next_trial.read(frozen.parent)
                self.assertEqual(next_trial.canonical_seeded_removal(frozen, ['m_a']), first)
                canonical.assert_called_once_with(frozen, ['m_a'])
                self.assertNotEqual(next_trial.canonical_seeded_removal(frozen, ['m_b']), first)
                self.assertEqual(canonical.call_count, 2)
                source.write_text('m_a ::= "changed"\n')
                with self.assertRaisesRegex(ValueError, 'frozen inputs changed'):
                    cache.trial().read(alias)

    def test_family_failure_hint_requires_a_complete_matching_report(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixtures = [root/'baseline.json', root/'candidate.json']
            for path in fixtures:
                batch.r.write(path, {'family': path.stem, 'cases': [{}]})
            before = {'family': 'baseline', 'cases': 1, 'passed': 1, 'failures': []}
            failed = {'family': 'candidate', 'cases': 1, 'passed': 0, 'failures': [{}]}
            self.assertEqual(batch._failed_fixtures(fixtures, [before, failed], 1), (1,))
            self.assertEqual(batch._failed_fixtures(fixtures, [dict(before, passed=0, failures=[{}]), failed], 1),
                             (0, 1))
            self.assertEqual(batch._failed_fixtures(fixtures, [before], 1), ())
            self.assertEqual(batch._failed_fixtures(fixtures, [before, failed], 2), ())
            self.assertEqual(batch._failed_fixtures(fixtures, [before, dict(failed, family='other')], 1), ())
            self.assertEqual(batch._failed_fixtures(fixtures, [before, dict(failed, passed=1)], 1), ())
            self.assertEqual(batch._failed_fixtures(fixtures, [dict(before, failures=[{}]), failed], 1), ())

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

    def test_combined_selection_requires_boolean_and_at_most_96(self):
        with tempfile.TemporaryDirectory() as temporary:
            for value in (None, 1, 'true'):
                with self.subTest(value=value), self.assertRaisesRegex(ValueError, 'combine_selected'):
                    batch.stage(temporary, {}, [{'job_id': 'j'}], combine_selected=value)
            with self.assertRaisesRegex(ValueError, 'ninety-six'):
                batch.stage(temporary, {}, [{}] * 97, combine_selected=True)

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
