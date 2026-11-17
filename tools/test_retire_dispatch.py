"""Small real Git and frozen-artifact canaries for campaign integration."""
from contextlib import redirect_stderr, redirect_stdout
from concurrent.futures import ThreadPoolExecutor
import fcntl
import io
import sys
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
from types import SimpleNamespace
import unittest
import uuid
from unittest.mock import Mock, patch

import retire as r
import retire_batch
import retire_cpu
import retire_dispatch as dispatch
import retire_rehabilitation as rehabilitation
import retire_tasks


def git(root, *args):
    return subprocess.check_output(['git', *args], cwd=root, text=True, stderr=subprocess.DEVNULL).strip()


def repository(path):
    path.mkdir()
    subprocess.run(['git', 'init', '-q', str(path)], check=True)
    git(path, 'config', 'user.email', 'test@example.invalid')
    git(path, 'config', 'user.name', 'Test')
    (path/'grammar/syntax').mkdir(parents=True)
    (path/'families').mkdir()
    (path/'tests').mkdir()
    (path/'grammar/syntax/mined.ebnf').write_text(
        'mined_action ::= m_one | never_action\n'
        '/* @sem Operation::Test */\nm_one ::= "one"\n'
        '/* @sem () */\nnever_action ::= "never"\n')
    (path/'families/base.json').write_text('{}\n')
    (path/'tests/families.json').write_text('[]\n')
    git(path, 'add', '.')
    git(path, 'commit', '-qm', 'base')
    return git(path, 'rev-parse', 'HEAD')


class DispatchCanaries(unittest.TestCase):
    def test_invalid_paid_model_response_keeps_usage_in_queue_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            run = Path(temporary)
            (run/'leases').mkdir()
            estimator = run/'estimator.py'
            estimator.write_text('estimator')
            metrics = {'usage_estimator': str(estimator), 'estimator_sha256': r.digest(estimator)}
            r.write(run/'config.json', {'metrics': metrics})
            queue = dispatch.Queue(run/'queue.sqlite')
            queue.add_checkpoint('base', {'proof': 'test'})
            job = {'id': 'one', 'rules': ['r_one'], 'kind': 'author'}
            queue.add_jobs([job])
            claim = queue.claim('worker')
            working = run/'attempt'
            working.mkdir()
            contexts = SimpleNamespace(get=lambda cp: (('index', 'gold', 'oracle'), {}, 'inverted'))
            paid = {'status': 'recorded', 'estimated_usd': 0.42, 'tokens': {'input_tokens': 120}}
            receipt = {'finished_unix': 100, 'returncode': 0}
            with patch.object(dispatch.campaign_api, 'checkpoint', return_value={'commit': 'base'}), \
                 patch.object(dispatch.planner, 'select_cohort_detail', return_value={'names': []}), \
                 patch.object(dispatch, 'prepare_task', return_value={'run': str(working), 'task_id': 'one'}), \
                 patch.object(dispatch.models, 'launch', return_value=receipt), \
                 patch.object(dispatch, 'decisions', side_effect=ValueError('invalid final JSON')), \
                 patch.object(dispatch.retire_metrics, 'collect', return_value=paid) as collect, \
                 patch.object(dispatch.retire_retention, 'archive_checkout', return_value={}):
                dispatch.perform(run, claim, contexts)
            collect.assert_called_once_with(working, 'one', receipt, metrics)
            attempt = queue.get_attempt(claim['attempt_id'])
            self.assertEqual(attempt['result']['status'], 'needs-review')
            evidence = attempt['result']['evidence']
            self.assertEqual(evidence['usage'], paid)
            self.assertEqual(evidence['kind'], 'author')
            self.assertIn('invalid final JSON', evidence['error'])
            self.assertEqual(r.read(Path(evidence['receipt']))['usage'], paid)

    def test_missing_receipt_unknown_and_mechanical_skips_collection(self):
        with tempfile.TemporaryDirectory() as temporary:
            run = Path(temporary)
            with patch.object(dispatch.retire_metrics, 'collect') as collect:
                unknown = dispatch.attempt_usage(run, {'id': 'x', 'kind': 'author'}, run, None)
                self.assertEqual(unknown['status'], 'unavailable')
                self.assertIsNone(unknown['estimated_usd'])
                self.assertIsNone(dispatch.attempt_usage(run, {'id': 'x', 'kind': 'mechanical'}, run, None))
                collect.assert_not_called()

    def test_failed_launch_without_receipt_records_unknown_in_queue(self):
        with tempfile.TemporaryDirectory() as temporary:
            run = Path(temporary)
            (run/'leases').mkdir()
            queue = dispatch.Queue(run/'queue.sqlite')
            queue.add_checkpoint('base', {'proof': 'test'})
            queue.add_jobs([{'id': 'one', 'rules': ['r_one'], 'kind': 'author'}])
            claim = queue.claim('worker')
            working = run/'attempt'
            working.mkdir()
            contexts = SimpleNamespace(get=lambda cp: (('index', 'gold', 'oracle'), {}, 'inverted'))
            with patch.object(dispatch.campaign_api, 'checkpoint', return_value={'commit': 'base'}), \
                 patch.object(dispatch.planner, 'select_cohort_detail', return_value={'names': []}), \
                 patch.object(dispatch, 'prepare_task', return_value={'run': str(working), 'task_id': 'one'}), \
                 patch.object(dispatch.models, 'launch', side_effect=RuntimeError('launch failed')), \
                 patch.object(dispatch.retire_metrics, 'collect') as collect, \
                 patch.object(dispatch.retire_retention, 'archive_checkout', return_value={}):
                dispatch.perform(run, claim, contexts)
            collect.assert_not_called()
            result = queue.get_attempt(claim['attempt_id'])['result']
            self.assertEqual(result['status'], 'failed')
            self.assertEqual(result['evidence']['usage']['status'], 'unavailable')
            self.assertIsNone(result['evidence']['usage']['estimated_usd'])

    def test_estimator_hash_guard_preserves_unknown(self):
        with tempfile.TemporaryDirectory() as temporary:
            run = Path(temporary)
            estimator = run/'estimator'
            estimator.write_text('changed')
            r.write(run/'config.json', {'metrics': {'usage_estimator': str(estimator),
                                                    'estimator_sha256': '0'*64}})
            with patch.object(dispatch.retire_metrics, 'collect') as collect:
                usage = dispatch.attempt_usage(run, {'id': 'one', 'kind': 'author'}, run,
                                               {'finished_unix': 10})
                self.assertEqual(usage['status'], 'unavailable')
                self.assertIn('estimator changed', usage['error'])
                self.assertIsNone(usage['estimated_usd'])
                collect.assert_not_called()

    def test_drain_stops_before_claiming_or_launching(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root/'runner').mkdir()
            r.write(root/'runner.json', {})
            r.write(root/'config.json', {'workers': 2, 'main_repo': str(root),
                                        'max_operational_retries': 1, 'first_batch_size': 12,
                                        'batch_size': 48, 'batch_wait_seconds': 900,
                                        'initial_commit': 'base'})
            r.write(root/'drain.json', {'reason': 'planned controller drain'})
            queue = dispatch.Queue(root/'queue.sqlite')
            queue.add_checkpoint('base', {'proof': 'test'})
            queue.add_jobs([{'id': 'one', 'rules': ['r_one'], 'kind': 'mechanical'},
                            {'id': 'two', 'rules': ['r_two'], 'kind': 'author'}])
            disk = SimpleNamespace(free=100 * 1024**3)
            with patch.object(dispatch.shutil, 'disk_usage', return_value=disk), \
                    patch.object(dispatch, 'perform') as perform, \
                    patch.object(dispatch.subprocess, 'Popen') as launch:
                dispatch.run_campaign(root)
            perform.assert_not_called()
            launch.assert_not_called()
            self.assertEqual(queue.coverage()['counts'], {'pending': 2})
            status = r.read(root/'status.json')
            self.assertEqual(status['status'], 'needs-operator')
            self.assertEqual(status['reason'], 'planned controller drain')

    def test_reclassify_rehabilitates_only_newly_unused_parked_rules(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            queue = dispatch.Queue(root/'queue.sqlite')
            queue.add_checkpoint('base', {'proof': 'old'})
            queue.add_jobs([{'id': 'old', 'rules': ['m_unused', 'm_still_used'], 'kind': 'author',
                             'source_fragments': [{'name': 'm_unused'}, {'name': 'm_still_used'}]},
                            {'id': 'other', 'rules': ['m_other'], 'kind': 'author', 'source_fragments': []}])
            child = subprocess.Popen([sys.executable, '-c', 'pass'])
            child.wait()
            for worker in ('first', 'second'):
                claim = queue.claim(worker)
                queue.mark_started(claim['attempt_id'], pid=child.pid)
                queue.finish(claim['attempt_id'], 'failed', 'proof',
                             {rule: {'status': 'needs-review', 'reason': 'review', 'evidence': 'x'}
                              for rule in claim['job']['rules']})
                lease = root/'leases'/claim['job']['id']
                lease.parent.mkdir(exist_ok=True)
                lease.touch()
            inputs = root/'inputs'
            (inputs/'scan').mkdir(parents=True)
            r.write(inputs/'scan/rules.json', {'unused': ['m_unused'], 'needed': {}})
            for name in ('scan-complete.json', 'manifest.json', 'scan/commands.jsonl'):
                (inputs/name).write_text('{}')
            cp = {'commit': 'base', 'inputs': str(inputs)}
            with patch.object(rehabilitation, 'verify_scan', return_value=(inputs, {'m_unused'}, 'manifest', 'scan')), \
                    patch.object(rehabilitation, 'verify_rule_receipts'):
                dispatch.reclassify(root, cp)
                self.assertEqual(queue.coverage()['counts'], {'failed': 3})
                queue.add_checkpoint('next', {'proof': 'new'}, expected_parent='base')
                dispatch.reclassify(root, dict(cp, commit='next'))
            coverage = queue.coverage()
            self.assertEqual(coverage['total_rules'], 3)
            self.assertEqual(coverage['rules']['m_unused']['disposition'], 'pending')
            self.assertEqual(coverage['rules']['m_still_used']['disposition'], 'needs-review')
            self.assertEqual(coverage['rules']['m_other']['disposition'], 'failed')
            mechanical = next(row for row in queue.get_jobs('pending') if row['job']['kind'] == 'mechanical')
            self.assertEqual(mechanical['job']['rules'], ['m_unused'])

    def test_reclassify_does_not_hide_invalid_scan_proof(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inputs = root/'inputs'
            (inputs/'scan').mkdir(parents=True)
            r.write(inputs/'scan/rules.json', {'unused': ['m_one'], 'needed': {}})
            for name in ('scan-complete.json', 'manifest.json', 'scan/commands.jsonl'):
                (inputs/name).write_text('{}')
            job = {'id': 'parked', 'rules': ['m_one'], 'kind': 'author'}
            with patch.object(dispatch, 'Queue') as queue_type:
                queue = queue_type.return_value
                queue.get_jobs.side_effect = lambda status: [{'id': 'parked', 'job': job}] if status == 'failed' else []
                queue.rehabilitate_unused.side_effect = ValueError('scan integrity failed')
                with self.assertRaisesRegex(ValueError, 'scan integrity failed'):
                    dispatch.reclassify(root, {'commit': 'next', 'inputs': str(inputs)})
                queue.rehabilitate_unused.side_effect = dispatch.RehabilitationIneligible('worker still running')
                dispatch.reclassify(root, {'commit': 'next', 'inputs': str(inputs)})

    def test_priority_refresh_is_opt_in_and_checkpoint_bound(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plan = {'checkpoint': 'next'}
            self.assertIsNone(dispatch.priority_policy({}))
            with self.assertRaisesRegex(ValueError, 'unsupported'):
                dispatch.priority_policy({'priority_policy': 'wrong'})
            with patch.object(dispatch.retire_priority, 'plan', return_value=plan) as planner, \
                    patch.object(dispatch, 'Queue') as queue:
                self.assertIsNone(dispatch.refresh_priorities(root, 'next', None))
                planner.assert_not_called()
                with self.assertRaisesRegex(ValueError, 'unsupported'):
                    dispatch.refresh_priorities(root, 'next', 'other')
                with self.assertRaisesRegex(ValueError, 'checkpoint changed'):
                    dispatch.refresh_priorities(root, 'stale', dispatch.retire_priority.VERSION)
                queue.assert_not_called()
                dispatch.refresh_priorities(root, 'next', dispatch.retire_priority.VERSION)
                queue.return_value.publish_priorities.assert_called_once_with('next', plan)

    def test_batch_refreshes_priority_after_accept_and_reclassification(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            r.write(root/'config.json', {'initial_commit': 'base', 'first_batch_size': 1,
                                        'batch_size': 1, 'priority_policy': dispatch.retire_priority.VERSION})
            queue = Mock()
            queue.coverage.return_value = {'checkpoint': 'base'}
            order = []
            queue.advance_and_accept.side_effect = lambda *args: order.append('accept')
            manifest = {'status': 'provisional', 'commit': 'next', 'id': 'batch',
                        'jobs': [{'job_id': 'one', 'status': 'included'}]}
            result = {'status': 'passed', 'inputs': str(root/'inputs'), 'rows': 'rows'}
            with patch.object(dispatch, 'Queue', return_value=queue), \
                    patch.object(dispatch.campaign_api, 'checkpoint', return_value={'commit': 'base', 'source': str(root),
                                                                                   'helpers': str(root/'helpers')}), \
                    patch.object(dispatch, 'proposals', return_value=[{'job_id': 'one'}]), \
                    patch.object(dispatch.retire_batch, 'stage', return_value=manifest), \
                    patch.object(dispatch.retire_batch, 'validate', return_value=result), \
                    patch.object(dispatch.subprocess, 'run'), patch.object(dispatch, 'scan_checkpoint'), \
                    patch.object(dispatch.campaign_api, 'save_checkpoint'), \
                    patch.object(dispatch, 'reclassify', side_effect=lambda *args: order.append('reclassify')), \
                    patch.object(dispatch, 'refresh_priorities', side_effect=lambda *args: order.append('priority')) as refresh, \
                    patch.object(dispatch, 'publish_main'):
                dispatch._batch(root)
            self.assertEqual(order, ['accept', 'reclassify', 'priority'])
            refresh.assert_called_once_with(root, 'next', dispatch.retire_priority.VERSION)

    def test_optimized_helpers_propagate_only_after_pass(self):
        for passed in (False, True):
            with self.subTest(passed=passed), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                folder = root/'batch'
                snapshot = folder/'candidate-validation-helpers'
                snapshot.mkdir(parents=True)
                inputs = root/'inputs'
                frozen = inputs/'frozen'
                (frozen/'bin').mkdir(parents=True)
                (frozen/'build.json').write_text('build manifest')
                binaries = {}
                for name in r.HELPERS:
                    (frozen/'bin'/name).write_text(name)
                    binaries[name] = r.digest(frozen/'bin'/name)
                override = {'path': str(root/'trusted'), 'build_sha256': r.digest(frozen/'build.json')}
                r.write(root/'config.json', {'initial_commit': 'base', 'first_batch_size': 1,
                                            'batch_size': 1, 'candidate_validation_helpers': override})
                queue = Mock()
                queue.coverage.return_value = {'checkpoint': 'base'}
                manifest = {'status': 'provisional', 'commit': 'next', 'id': 'batch', 'folder': str(folder),
                            'jobs': [{'job_id': 'one', 'status': 'included'}]}
                result = {'status': 'passed' if passed else 'failed', 'inputs': str(inputs), 'rows': 'rows',
                          'helpers': str(snapshot)}
                with patch.object(dispatch, 'Queue', return_value=queue), \
                        patch.object(dispatch.campaign_api, 'checkpoint', return_value={
                            'commit': 'base', 'source': str(root), 'helpers': 'old-helpers'}), \
                        patch.object(dispatch, 'proposals', return_value=[{'job_id': 'one'}]), \
                        patch.object(dispatch.retire_batch, '_candidate_helper_preflight'), \
                        patch.object(dispatch.retire_batch, 'stage', return_value=manifest) as stage, \
                        patch.object(dispatch.retire_batch, 'validate', return_value=result), \
                        patch.object(dispatch.retire_batch, '_verified_helper_build', return_value={'binaries': binaries}), \
                        patch.object(dispatch.subprocess, 'run'), \
                        patch.object(dispatch, 'scan_checkpoint') as scan, \
                        patch.object(dispatch.campaign_api, 'save_checkpoint') as save, \
                        patch.object(dispatch, 'reclassify'), patch.object(dispatch, 'publish_main'):
                    dispatch._batch(root)
                self.assertEqual(stage.call_args.args[1]['candidate_validation_helpers'], override)
                if passed:
                    next_cp = save.call_args.args[1]
                    self.assertEqual(next_cp['helpers'], str(snapshot))
                    self.assertEqual(next_cp['inputs'], str(inputs))
                    self.assertNotIn('candidate_validation_helpers', next_cp)
                    self.assertEqual(scan.call_args.args[0], inputs)
                    queue.advance_and_accept.assert_called_once()
                else:
                    save.assert_not_called()
                    scan.assert_not_called()
                    queue.advance_and_accept.assert_not_called()

    def test_invalid_candidate_helper_configuration_does_not_stage_or_reject(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            r.write(root/'config.json', {'candidate_validation_helpers': {'path': '/bad', 'build_sha256': 'bad'}})
            queue = Mock()
            queue.coverage.return_value = {'checkpoint': 'base'}
            with patch.object(dispatch, 'Queue', return_value=queue), \
                    patch.object(dispatch.campaign_api, 'checkpoint', return_value={'commit': 'base', 'source': str(root)}), \
                    patch.object(dispatch.retire_batch, 'stage') as stage:
                with self.assertRaises(ValueError):
                    dispatch._batch(root)
            stage.assert_not_called()
            queue.reject_jobs.assert_not_called()
            queue.advance_and_accept.assert_not_called()

    def test_controller_accepts_verified_explicit_runner_and_refuses_tampering(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root/'runner').mkdir()
            r.write(root/'runner.json', {})
            version = root/'runners/v7'
            version.mkdir(parents=True)
            for name in dispatch.campaign_api.RUNNERS:
                shutil.copy2(Path(dispatch.__file__).parent/name, version/name)
            r.write(version.with_suffix('.json'), r.tree(version))
            r.write(root/'config.json', {'workers': 1, 'main_repo': str(root),
                                        'max_operational_retries': 1, 'first_batch_size': 12,
                                        'batch_size': 48, 'batch_wait_seconds': 900,
                                        'initial_commit': 'base'})
            r.write(root/'drain.json', {'reason': 'test'})
            queue = dispatch.Queue(root/'queue.sqlite')
            queue.add_checkpoint('base', {'proof': 'test'})
            queue.add_jobs([{'id': 'one', 'rules': ['r_one'], 'kind': 'author'}])
            with patch.object(dispatch.shutil, 'disk_usage', return_value=SimpleNamespace(free=100 * 1024**3)), \
                    patch.object(dispatch, 'perform') as perform, patch.object(dispatch.subprocess, 'Popen') as launch:
                dispatch.run_campaign(root, runner=version)
            perform.assert_not_called()
            launch.assert_not_called()
            self.assertEqual(r.read(root/'status.json')['reason'], 'test')
            (version/'retire_dispatch.py').write_text('tampered')
            with self.assertRaisesRegex(ValueError, 'runner changed'):
                dispatch.run_campaign(root, runner=version)
            r.write(version.with_suffix('.json'), r.tree(version))
            with self.assertRaisesRegex(ValueError, 'execute from the selected'):
                dispatch.run_campaign(root, runner=version)

    def test_batch_lock_held_by_another_process_skips_staging(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            code = ("import fcntl, sys, time; "
                    "f=open(sys.argv[1], 'a'); fcntl.flock(f, fcntl.LOCK_EX); "
                    "print('locked', flush=True); time.sleep(60)")
            child = subprocess.Popen([sys.executable, '-c', code, str(root/'batch.lock')],
                                     stdout=subprocess.PIPE, text=True)
            try:
                self.assertEqual(child.stdout.readline().strip(), 'locked')
                with patch.object(dispatch, '_batch') as work, patch.object(dispatch.retire_batch, 'stage') as stage:
                    dispatch.batch(root)
                work.assert_not_called()
                stage.assert_not_called()
            finally:
                child.terminate()
                child.wait()
                child.stdout.close()

    def test_selected_proposals_are_exact_ordered_and_provisional(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            queue = dispatch.Queue(root/'queue.sqlite')
            queue.add_checkpoint('base', 'proof')
            queue.add_jobs([{'id': name, 'rules': [f'r_{name}'], 'kind': 'author'}
                            for name in ('one', 'two', 'three', 'review')])
            for name in ('one', 'two', 'three', 'review'):
                claim = queue.claim('worker')
                report = root/f'{name}.json'
                r.write(report, {'task': name})
                passing = name != 'review'
                outcome = {'status': 'pass', 'evidence': 'x'} if passing else {
                    'status': 'needs-review', 'reason': 'review', 'evidence': 'x'}
                queue.finish(claim['attempt_id'], 'provisional-pass' if passing else 'needs-review',
                             {'report': str(report), 'run': str(root), 'kind': 'author'},
                             {f'r_{name}': outcome})
            rows = dispatch.proposals(root, 1, ['three', 'one'])
            self.assertEqual([row['job_id'] for row in rows], ['three', 'one'])
            control = {'path': '/future/profile.json', 'sha256': 'abc'}
            r.write(root/'config.json', {'initial_commit': 'base', 'first_batch_size': 1,
                                        'batch_size': 1, 'audit_threads': 16,
                                        'native_regression_controls': control, 'stage_group_size': 4})
            with patch.object(dispatch.campaign_api, 'checkpoint', return_value={'commit': 'base'}), \
                    patch.object(dispatch.retire_batch, 'stage', return_value={'jobs': [], 'status': 'no-proposals'}) as stage:
                dispatch._batch(root, ['three', 'one'])
            self.assertEqual([row['job_id'] for row in stage.call_args.args[2]], ['three', 'one'])
            self.assertEqual(stage.call_args.args[1]['native_regression_controls'], control)
            self.assertEqual(stage.call_args.args[1]['stage_group_size'], 4)
            self.assertNotIn('combine_selected', stage.call_args.kwargs)
            with patch.object(dispatch.campaign_api, 'checkpoint', return_value={'commit': 'base'}), \
                    patch.object(dispatch.retire_batch, 'stage', return_value={'jobs': [], 'status': 'no-proposals'}) as combined:
                dispatch._batch(root, ['three', 'one'], combine_selected=True)
            self.assertTrue(combined.call_args.kwargs['combine_selected'])
            self.assertEqual([row['job_id'] for row in combined.call_args.args[2]], ['three', 'one'])
            for invalid in ([], ['one', 'one'], ['one', 'unknown'], ['review'], ['one']*97, 'one'):
                with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                    dispatch.proposals(root, 48, invalid)
            self.assertEqual([row['id'] for row in queue.get_jobs('provisional-pass')], ['one', 'two', 'three'])

    def test_combine_selected_cli_requires_explicit_nonempty_batch_jobs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            empty = root/'empty.json'
            r.write(empty, [])
            for args in (['run', str(root), '--combine-selected'],
                         ['mechanical', str(root), '--combine-selected'],
                         ['batch', str(root), '--combine-selected'],
                         ['batch', str(root), '--combine-selected', '--jobs-file', str(empty)]):
                with self.subTest(args=args), patch.object(sys, 'argv', ['retire_dispatch.py', *args]), \
                        redirect_stderr(io.StringIO()), patch.object(dispatch, 'batch') as batch:
                    with self.assertRaises(SystemExit):
                        dispatch.main()
                    batch.assert_not_called()
            selected = root/'selected.json'
            r.write(selected, ['one', 'two'])
            with patch.object(sys, 'argv', ['retire_dispatch.py', 'batch', str(root), '--jobs-file', str(selected),
                                            '--combine-selected', '--wait']), patch.object(dispatch, 'batch') as batch:
                dispatch.main()
            batch.assert_called_once_with(root.resolve(), ['one', 'two'], wait=True, combine_selected=True)
            for value in (None, 1, 'yes'):
                with self.subTest(value=value), self.assertRaises(ValueError):
                    dispatch.batch(root, ['one'], combine_selected=value)
            with self.assertRaises(ValueError):
                dispatch.batch(root, combine_selected=True)

    def test_manual_batch_waits_for_authoritative_lock(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            invoked = threading.Event()
            def work(run, selected):
                invoked.set()
                return selected
            with (root/'batch.lock').open('w') as held:
                fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
                with ThreadPoolExecutor(max_workers=1) as executor, patch.object(dispatch, '_batch', side_effect=work):
                    future = executor.submit(dispatch.batch, root, ['one', 'two'], wait=True)
                    self.assertFalse(invoked.wait(.1))
                    fcntl.flock(held, fcntl.LOCK_UN)
                    self.assertEqual(future.result(timeout=5), ['one', 'two'])
                    self.assertTrue(invoked.is_set())

    def test_controller_does_not_spawn_while_external_batch_holds_lease(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root/'runner').mkdir()
            r.write(root/'runner.json', {})
            r.write(root/'config.json', {'workers': 1, 'main_repo': str(root),
                                        'max_operational_retries': 1, 'first_batch_size': 1,
                                        'batch_size': 1, 'batch_wait_seconds': 0,
                                        'initial_commit': 'base'})
            queue = dispatch.Queue(root/'queue.sqlite')
            queue.add_checkpoint('base', {'proof': 'test'})
            queue.add_jobs([{'id': 'one', 'rules': ['r_one'], 'kind': 'author'}])
            claim = queue.claim('prior')
            queue.finish(claim['attempt_id'], 'provisional-pass', 'proof',
                         {'r_one': {'status': 'pass', 'evidence': 'proof'}})
            def drain(_seconds):
                r.write(root/'drain.json', {'reason': 'test drain'})
            with (root/'batch.lock').open('w') as held:
                fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
                with patch.object(dispatch.shutil, 'disk_usage', return_value=SimpleNamespace(free=100 * 1024**3)), \
                        patch.object(dispatch.time, 'sleep', side_effect=drain), \
                        patch.object(dispatch.subprocess, 'Popen') as launch:
                    dispatch.run_campaign(root)
                launch.assert_not_called()
                self.assertEqual(queue.coverage()['counts'], {'provisional-pass': 1})
                self.assertEqual(r.read(root/'status.json')['reason'], 'test drain')
            self.assertTrue(dispatch.batch_available(root))

    def test_checkpoint_scan_reserves_cpu_and_inherits_batch_guard(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cp = {'cpu_pool': str(root/'cpu'), 'cpu_capacity': 16, 'audit_threads': 16}
            child_code = """
import fcntl, os, pathlib, sys
sys.path.insert(0, sys.argv[1])
import retire_cpu
for fd in map(int, sys.argv[4:]):
    os.fstat(fd)
try:
    with retire_cpu.reserve(pathlib.Path(sys.argv[2]), 1, 16, wait=False):
        raise AssertionError('CPU pool was not held')
except BlockingIOError:
    pass
with open(sys.argv[3], 'a') as guard:
    try:
        fcntl.flock(guard, fcntl.LOCK_EX | fcntl.LOCK_NB)
        raise AssertionError('batch guard was not held')
    except BlockingIOError:
        pass
"""
            for fail in (False, True):
                def scan(args):
                    self.assertEqual(args.run, root/'inputs')
                    self.assertIsNone(args.rules)
                    fds = tuple(r._LEASES)
                    self.assertEqual(len(fds), 17)
                    paths = [os.readlink(f'/proc/self/fd/{fd}') for fd in fds]
                    self.assertEqual(sum('/token-' in path for path in paths), 16)
                    self.assertIn(str(root/'batch.lock'), paths)
                    subprocess.run([sys.executable, '-c', child_code, str(Path(retire_cpu.__file__).parent),
                                    cp['cpu_pool'], str(root/'batch.lock'), *map(str, fds)],
                                   pass_fds=fds, check=True)
                    if fail:
                        raise RuntimeError('scan failed')
                with patch.object(dispatch.r, 'scan', side_effect=scan), \
                        patch.object(dispatch, '_batch', side_effect=lambda _: dispatch.scan_checkpoint(root/'inputs', cp)):
                    if fail:
                        with self.assertRaisesRegex(RuntimeError, 'scan failed'):
                            dispatch.batch(root)
                    else:
                        dispatch.batch(root)
                self.assertEqual(r._LEASES, [])
                with retire_cpu.reserve(root/'cpu', 16, 16, wait=False) as fds:
                    self.assertEqual(len(fds), 16)

    def test_real_mechanical_task_report_and_batch_stage(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, campaign, helpers, inputs = (root/'source', root/'campaign', root/'helpers', root/'inputs')
            commit = repository(source)
            campaign.mkdir()
            (campaign/'runner').mkdir()
            tools = Path(__file__).resolve().parent
            for name in retire_tasks.RUNNERS:
                shutil.copy2(tools/name, campaign/'runner'/name)
            r.write(campaign/'runner.json', r.tree(campaign/'runner'))
            helpers.mkdir()
            for name in retire_tasks.dev.BINS:
                (helpers/name).write_text('helper '+name)
            build = {'source_sha256': 'compiled', 'binaries': {n: r.digest(helpers/n) for n in retire_tasks.dev.BINS}}
            r.write(helpers/'build.json', build)
            frozen = inputs/'frozen'
            frozen.mkdir(parents=True)
            shutil.copytree(source/'grammar', frozen/'grammar')
            r.write(frozen/'build.json', build)
            (frozen/'bin').mkdir()
            for name in r.HELPERS:
                shutil.copy2(helpers/name, frozen/'bin'/name)
            r.write(frozen/'index.json', {
                '0': {'name': 'Example', 'text': 'one', 'record': 0, 'face': 0, 'flags': [True]*4},
                '1': {'name': 'Companion', 'text': 'other', 'record': 0, 'face': 1, 'flags': [True]*4},
                '2': {'name': 'Companion', 'text': 'other', 'record': 1, 'face': 0, 'flags': [True]*4}})
            r.jsonl(frozen/'gold.jsonl', [
                {'faces': [{'name': 'Example', 'abilities': []}, {'name': 'Companion', 'abilities': []}]},
                {'faces': [{'name': 'Companion', 'abilities': []}]}])
            r.write(frozen/'oracle.json', [{'cards': [{'name': 'Example', 'text': 'one'},
                                                   {'name': 'Companion', 'text': 'other'}]},
                                            {'cards': [{'name': 'Companion', 'text': 'other'}]}])
            manifest = {'format': 3, 'commit': commit, 'source': 'compiled', 'files': r.tree(frozen), 'tooling': r.tooling()}
            r.write(inputs/'manifest.json', manifest)
            (inputs/'scan').mkdir()
            r.write(inputs/'scan/rules.json', {'unused': ['m_one']})
            checkpoint = {'source': str(source), 'commit': commit, 'inputs': str(inputs), 'helpers': str(helpers)}
            job = {'id': 'mech', 'rules': ['m_one'], 'goal': 'remove audited unused rule',
                   'kind': 'mechanical', 'target_faces': ['Example']}
            claim = {'attempt_id': str(uuid.uuid4()), 'base_commit': commit, 'job': job}
            detail = {'names': ['Example']}
            with patch.object(retire_tasks.dev, 'source', return_value='compiled'):
                prepared = retire_tasks.prepare_task(campaign, claim, checkpoint, job, detail)
                run = Path(prepared['run'])
                with patch.object(dispatch.w, 'checks', return_value={'faces': 3, 'failures': []}) as checks, redirect_stdout(io.StringIO()):
                    dispatch.mechanical(run, prepared['task_id'])
                self.assertEqual(set(checks.call_args.args[3]), {'Example', 'Companion'})
                self.assertEqual(r.read(run/'tasks/mech.json')['target_faces'], ['Example'])
                reports = list((run/'evaluations'/'mech').glob('*/report.json'))
                self.assertEqual(len(reports), 1)
                report = r.read(reports[0])
                self.assertTrue(report['passed'])
                proposal = {'kind': 'mechanical', 'job_id': 'mech', 'attempt_id': claim['attempt_id'],
                            'run': str(run), 'report': report}
                candidate = retire_batch.stage(campaign, checkpoint, [proposal])
            self.assertEqual(candidate['status'], 'provisional', candidate)
            self.assertEqual(candidate['fixtures'], [])
            self.assertEqual(candidate['jobs'][0]['status'], 'included')
            self.assertNotIn('m_one ::=', (Path(candidate['source'])/'grammar/syntax/mined.ebnf').read_text())
            self.assertIn('m_one ::=', (source/'grammar/syntax/mined.ebnf').read_text())

    def test_publish_main_checks_semantic_parent_tree(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            main, campaign = root/'main', root/'campaign'
            base = repository(main)
            campaign.mkdir()
            previous = root/'previous'
            subprocess.run(['git', 'worktree', 'add', '--detach', str(previous), base], cwd=main, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            candidate = root/'candidate'
            subprocess.run(['git', 'worktree', 'add', '-b', 'candidate', str(candidate), base], cwd=main, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            (candidate/'grammar/syntax/mined.ebnf').write_text('never_action ::= "never"\n')
            git(candidate, 'add', '.')
            git(candidate, 'commit', '-qm', 'candidate')
            commit = git(candidate, 'rev-parse', 'HEAD')
            # An unrelated clean main commit must not block a semantic-parent match.
            (main/'readme.txt').write_text('unrelated\n')
            git(main, 'add', '.')
            git(main, 'commit', '-qm', 'unrelated')
            r.write(campaign/'config.json', {'main_repo': str(main)})
            folder = root/'proof'
            folder.mkdir()
            with patch.object(dispatch.dev, 'source', return_value='compiled'):
                dispatch.publish_main(campaign, {'source': str(previous)}, {'commit': commit}, {'folder': str(folder)})
            proof = r.read(folder/'main-integration.json')
            self.assertEqual(proof['status'], 'integrated')
            self.assertEqual(r.tree(main/'grammar'), r.tree(candidate/'grammar'))
            # Replaying against the old semantic parent must fail without changing main.
            before = git(main, 'rev-parse', 'HEAD')
            with patch.object(dispatch.dev, 'source', return_value='compiled'):
                dispatch.publish_main(campaign, {'source': str(previous)}, {'commit': commit}, {'folder': str(folder)})
            proof = r.read(folder/'main-integration.json')
            self.assertEqual(proof['status'], 'pending')
            self.assertIn('semantic source differs', proof['reason'])
            self.assertEqual(git(main, 'rev-parse', 'HEAD'), before)


if __name__ == '__main__':
    unittest.main()
